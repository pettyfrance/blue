use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::stable_graph::{NodeIndex, StableDiGraph};

use super::model::{Declaration, DeclarationId};

/// A desired graph borrows its declarations; edges point prerequisite → dependent.
/// Construction is internal so the builder can validate the complete graph
/// before exposing it. There is no public access to mutable graph internals.
#[derive(Debug)]
pub struct DesiredGraph<'a> {
    graph: StableDiGraph<Declaration<'a>, ()>,
    indices: BTreeMap<DeclarationId, NodeIndex>,
    providers: BTreeMap<DeclarationId, DeclarationId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphLookupError {
    MissingDeclaration { id: DeclarationId },
}

impl fmt::Display for GraphLookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDeclaration { id } => write!(f, "declaration `{id}` is not in the graph"),
        }
    }
}

impl Error for GraphLookupError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphCycle {
    /// The first declaration is repeated at the end to close the cycle.
    pub path: Vec<DeclarationId>,
}

impl fmt::Display for GraphCycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("dependency cycle: ")?;
        for (index, id) in self.path.iter().enumerate() {
            if index > 0 {
                f.write_str(" → ")?;
            }
            write!(f, "{id}")?;
        }
        Ok(())
    }
}

impl Error for GraphCycle {}

impl<'a> DesiredGraph<'a> {
    pub fn declaration(&self, id: &DeclarationId) -> Result<Declaration<'a>, GraphLookupError> {
        Ok(self.graph[self.index(id)?])
    }

    /// Providers and parameters have no selected provider. The builder will
    /// associate each resource/data declaration with its selected provider.
    pub fn provider_for(
        &self,
        id: &DeclarationId,
    ) -> Result<Option<DeclarationId>, GraphLookupError> {
        self.index(id)?;
        Ok(self.providers.get(id).cloned())
    }

    /// Direct prerequisites, sorted by declaration identity and deduplicated.
    pub fn dependencies_of(
        &self,
        id: &DeclarationId,
    ) -> Result<Vec<DeclarationId>, GraphLookupError> {
        let index = self.index(id)?;
        Ok(self
            .graph
            .neighbors_directed(index, Direction::Incoming)
            .map(|dependency| self.graph[dependency].id())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }

    /// Prerequisites precede dependents. Independent declarations have no
    /// guaranteed relative order. Cycles return their complete, closed path.
    pub fn topological_order(&self) -> Result<Vec<DeclarationId>, GraphCycle> {
        match toposort(&self.graph, None) {
            Ok(indices) => Ok(indices
                .into_iter()
                .map(|index| self.graph[index].id())
                .collect()),
            Err(_) => Err(GraphCycle {
                path: self
                    .find_cycle()
                    .expect("toposort reported a cycle")
                    .into_iter()
                    .map(|index| self.graph[index].id())
                    .collect(),
            }),
        }
    }

    fn index(&self, id: &DeclarationId) -> Result<NodeIndex, GraphLookupError> {
        self.indices
            .get(id)
            .copied()
            .ok_or_else(|| GraphLookupError::MissingDeclaration { id: id.clone() })
    }

    fn find_cycle(&self) -> Option<Vec<NodeIndex>> {
        fn visit(
            graph: &StableDiGraph<Declaration<'_>, ()>,
            node: NodeIndex,
            active: &mut BTreeMap<NodeIndex, usize>,
            finished: &mut BTreeSet<NodeIndex>,
            stack: &mut Vec<NodeIndex>,
        ) -> Option<Vec<NodeIndex>> {
            if let Some(&start) = active.get(&node) {
                let mut cycle = stack[start..].to_vec();
                cycle.push(node);
                return Some(cycle);
            }
            if finished.contains(&node) {
                return None;
            }
            active.insert(node, stack.len());
            stack.push(node);
            for next in graph.neighbors_directed(node, Direction::Outgoing) {
                if let Some(cycle) = visit(graph, next, active, finished, stack) {
                    return Some(cycle);
                }
            }
            stack.pop();
            active.remove(&node);
            finished.insert(node);
            None
        }

        let mut active = BTreeMap::new();
        let mut finished = BTreeSet::new();
        let mut stack = Vec::new();
        for &node in self.indices.values() {
            if let Some(cycle) = visit(&self.graph, node, &mut active, &mut finished, &mut stack) {
                return Some(cycle);
            }
        }
        None
    }
}

/// Errors from internal construction, to be translated into source-aware
/// diagnostics by the builder. Failed mutations leave the graph unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConstructionError {
    Lookup(GraphLookupError),
    DuplicateDeclaration {
        id: DeclarationId,
    },
    InvalidProviderAssociation {
        declaration: DeclarationId,
        provider: DeclarationId,
    },
    ProviderAlreadySelected {
        declaration: DeclarationId,
    },
}

impl From<GraphLookupError> for ConstructionError {
    fn from(error: GraphLookupError) -> Self {
        Self::Lookup(error)
    }
}

impl<'a> DesiredGraph<'a> {
    pub(super) fn new() -> Self {
        Self {
            graph: StableDiGraph::new(),
            indices: BTreeMap::new(),
            providers: BTreeMap::new(),
        }
    }

    pub(super) fn add_declaration(
        &mut self,
        declaration: Declaration<'a>,
    ) -> Result<(), ConstructionError> {
        let id = declaration.id();
        if self.indices.contains_key(&id) {
            return Err(ConstructionError::DuplicateDeclaration { id });
        }
        let index = self.graph.add_node(declaration);
        self.indices.insert(id, index);
        Ok(())
    }

    pub(super) fn add_dependency(
        &mut self,
        prerequisite: &DeclarationId,
        dependent: &DeclarationId,
    ) -> Result<(), ConstructionError> {
        let from = self.index(prerequisite)?;
        let to = self.index(dependent)?;
        if !self.graph.contains_edge(from, to) {
            self.graph.add_edge(from, to, ());
        }
        Ok(())
    }

    /// Store an already resolved association and its prerequisite edge. This
    /// validates node kinds, but does not choose or infer a provider.
    pub(super) fn set_provider(
        &mut self,
        declaration: &DeclarationId,
        provider: &DeclarationId,
    ) -> Result<(), ConstructionError> {
        let owner_index = self.index(declaration)?;
        let provider_index = self.index(provider)?;
        if !matches!(
            self.graph[owner_index],
            Declaration::Resource(_) | Declaration::Data(_)
        ) || !matches!(self.graph[provider_index], Declaration::Provider(_))
        {
            return Err(ConstructionError::InvalidProviderAssociation {
                declaration: declaration.clone(),
                provider: provider.clone(),
            });
        }
        if let Some(previous) = self.providers.get(declaration) {
            if previous != provider {
                return Err(ConstructionError::ProviderAlreadySelected {
                    declaration: declaration.clone(),
                });
            }
            return Ok(());
        }
        self.add_dependency(provider, declaration)?;
        self.providers.insert(declaration.clone(), provider.clone());
        Ok(())
    }
}

#[cfg(test)]
#[path = "desired_tests.rs"]
mod tests;
