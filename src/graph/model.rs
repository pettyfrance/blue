use std::collections::BTreeMap;
use std::fmt;

use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use petgraph::visit::EdgeRef;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeId {
    Provider { name: String },
    Parameter { name: String },
    Data { data_type: String, name: String },
    Resource { resource_type: String, name: String },
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider { name } => write!(f, "provider.{name}"),
            Self::Parameter { name } => write!(f, "parameter.{name}"),
            Self::Data { data_type, name } => write!(f, "data.{data_type}.{name}"),
            Self::Resource {
                resource_type,
                name,
            } => write!(f, "resource.{resource_type}.{name}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub id: NodeId,
    pub kind: GraphNodeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphNodeKind {
    Provider,
    Parameter,
    Data { provider: String },
    Resource { provider: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeKind {
    Provider,
    Reference,
}

#[derive(Debug, Clone)]
pub struct DesiredGraph {
    graph: StableDiGraph<GraphNode, EdgeKind>,
    node_indices: BTreeMap<NodeId, NodeIndex>,
}

impl DesiredGraph {
    pub fn new() -> Self {
        Self {
            graph: StableDiGraph::new(),
            node_indices: BTreeMap::new(),
        }
    }

    pub fn add_node(&mut self, node: GraphNode) -> NodeIndex {
        if let Some(index) = self.node_indices.get(&node.id).copied() {
            self.graph[index] = node;
            return index;
        }

        let id = node.id.clone();
        let index = self.graph.add_node(node);
        self.node_indices.insert(id, index);
        index
    }

    pub fn add_edge(&mut self, from: &NodeId, to: &NodeId, kind: EdgeKind) -> Option<()> {
        let from = self.node_indices.get(from).copied()?;
        let to = self.node_indices.get(to).copied()?;

        if self
            .graph
            .edges(from)
            .any(|edge| edge.target() == to && edge.weight() == &kind)
        {
            return Some(());
        }

        self.graph.add_edge(from, to, kind);
        Some(())
    }

    pub fn contains_node(&self, id: &NodeId) -> bool {
        self.node_indices.contains_key(id)
    }

    pub fn node(&self, id: &NodeId) -> Option<&GraphNode> {
        self.node_indices
            .get(id)
            .and_then(|index| self.graph.node_weight(*index))
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub fn has_edge(&self, from: &NodeId, to: &NodeId, kind: &EdgeKind) -> bool {
        self.edge_count_between(from, to, kind) > 0
    }

    pub fn edge_count_between(&self, from: &NodeId, to: &NodeId, kind: &EdgeKind) -> usize {
        let Some(from) = self.node_indices.get(from).copied() else {
            return 0;
        };
        let Some(to) = self.node_indices.get(to).copied() else {
            return 0;
        };

        self.graph
            .edges(from)
            .filter(|edge| edge.target() == to && edge.weight() == kind)
            .count()
    }

    pub fn graph(&self) -> &StableDiGraph<GraphNode, EdgeKind> {
        &self.graph
    }

    pub fn node_indices(&self) -> &BTreeMap<NodeId, NodeIndex> {
        &self.node_indices
    }

    pub fn topological_order(&self) -> Result<Vec<NodeId>, Vec<NodeId>> {
        match toposort(&self.graph, None) {
            Ok(order) => Ok(order
                .into_iter()
                .filter_map(|index| self.graph.node_weight(index))
                .map(|node| node.id.clone())
                .collect()),
            Err(_) => Err(self.find_cycle().unwrap_or_default()),
        }
    }

    pub fn dependencies_of(&self, node: &NodeId) -> Vec<NodeId> {
        let Some(index) = self.node_indices.get(node).copied() else {
            return Vec::new();
        };

        let mut dependencies = self
            .graph
            .edges_directed(index, Direction::Incoming)
            .filter_map(|edge| self.graph.node_weight(edge.source()))
            .map(|node| node.id.clone())
            .collect::<Vec<_>>();
        dependencies.sort();
        dependencies.dedup();
        dependencies
    }

    pub fn find_cycle(&self) -> Option<Vec<NodeId>> {
        super::algorithms::find_cycle(&self.graph).map(|cycle| {
            cycle
                .into_iter()
                .map(|index| self.graph[index].id.clone())
                .collect()
        })
    }
}

impl Default for DesiredGraph {
    fn default() -> Self {
        Self::new()
    }
}
