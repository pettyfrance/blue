use std::collections::BTreeMap;

use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use petgraph::visit::EdgeRef;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeId {
    Provider { name: String },
    Parameter { name: String },
    Data { data_type: String, name: String },
    Resource { resource_type: String, name: String },
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
        let Some(from) = self.node_indices.get(from).copied() else {
            return false;
        };
        let Some(to) = self.node_indices.get(to).copied() else {
            return false;
        };

        self.graph
            .edges(from)
            .any(|edge| edge.target() == to && edge.weight() == kind)
    }

    pub fn graph(&self) -> &StableDiGraph<GraphNode, EdgeKind> {
        &self.graph
    }

    pub fn node_indices(&self) -> &BTreeMap<NodeId, NodeIndex> {
        &self.node_indices
    }
}

impl Default for DesiredGraph {
    fn default() -> Self {
        Self::new()
    }
}
