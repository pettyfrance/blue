use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use petgraph::algo::toposort;
use petgraph::stable_graph::{NodeIndex, StableDiGraph};

use crate::graph::algorithms;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OperationKind {
    CheckProvider,
    Create,
    Update,
    Delete,
    ReadData,
    Forget,
    NoOp,
    DeferredDiff,
    ExecuteDeferred,
}

impl fmt::Display for OperationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::CheckProvider => "check-provider",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::ReadData => "read-data",
            Self::Forget => "forget",
            Self::NoOp => "no-op",
            Self::DeferredDiff => "deferred-diff",
            Self::ExecuteDeferred => "execute-deferred",
        })
    }
}

/// Identifies one execution step within a plan, not a remote object.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OperationId {
    pub address: String,
    pub kind: OperationKind,
}

impl OperationId {
    pub fn new(address: impl Into<String>, kind: OperationKind) -> Self {
        Self {
            address: address.into(),
            kind,
        }
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.address, self.kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationGraphError {
    DuplicateOperation { operation: OperationId },
    MissingOperation { operation: OperationId },
    Cycle { path: Vec<OperationId> },
}

impl fmt::Display for OperationGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateOperation { operation } => {
                write!(f, "duplicate operation `{operation}`")
            }
            Self::MissingOperation { operation } => {
                write!(f, "missing operation `{operation}`")
            }
            Self::Cycle { path } => {
                f.write_str("operation dependency cycle detected: ")?;
                for (index, operation) in path.iter().enumerate() {
                    if index > 0 {
                        f.write_str(" -> ")?;
                    }
                    write!(f, "{operation}")?;
                }
                Ok(())
            }
        }
    }
}

impl Error for OperationGraphError {}

/// Edges point from prerequisite to dependent execution step.
#[derive(Debug, Clone, Default)]
pub struct OperationGraph {
    graph: StableDiGraph<OperationId, ()>,
    operation_indices: BTreeMap<OperationId, NodeIndex>,
}

impl OperationGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_operation(
        &mut self,
        operation: OperationId,
    ) -> Result<NodeIndex, OperationGraphError> {
        if self.operation_indices.contains_key(&operation) {
            return Err(OperationGraphError::DuplicateOperation { operation });
        }
        let index = self.graph.add_node(operation.clone());
        self.operation_indices.insert(operation, index);
        Ok(index)
    }

    /// Repeated edges are idempotent. Neither endpoint is created implicitly.
    pub fn add_edge(
        &mut self,
        prerequisite: &OperationId,
        dependent: &OperationId,
    ) -> Result<(), OperationGraphError> {
        let from = self.index(prerequisite)?;
        let to = self.index(dependent)?;
        if !self.graph.contains_edge(from, to) {
            self.graph.add_edge(from, to, ());
        }
        Ok(())
    }

    fn index(&self, operation: &OperationId) -> Result<NodeIndex, OperationGraphError> {
        self.operation_indices
            .get(operation)
            .copied()
            .ok_or_else(|| OperationGraphError::MissingOperation {
                operation: operation.clone(),
            })
    }

    pub fn contains_operation(&self, operation: &OperationId) -> bool {
        self.operation_indices.contains_key(operation)
    }

    pub fn operation(&self, operation: &OperationId) -> Option<&OperationId> {
        self.operation_indices
            .get(operation)
            .and_then(|index| self.graph.node_weight(*index))
    }

    pub fn operation_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub fn has_edge(&self, prerequisite: &OperationId, dependent: &OperationId) -> bool {
        match (self.index(prerequisite), self.index(dependent)) {
            (Ok(from), Ok(to)) => self.graph.contains_edge(from, to),
            _ => false,
        }
    }

    pub fn graph(&self) -> &StableDiGraph<OperationId, ()> {
        &self.graph
    }

    pub fn find_cycle(&self) -> Option<Vec<OperationId>> {
        algorithms::find_cycle(&self.graph).map(|cycle| {
            cycle
                .into_iter()
                .map(|index| self.graph[index].clone())
                .collect()
        })
    }

    pub fn validate(&self) -> Result<(), OperationGraphError> {
        match self.find_cycle() {
            Some(path) => Err(OperationGraphError::Cycle { path }),
            None => Ok(()),
        }
    }

    pub fn topological_order(&self) -> Result<Vec<OperationId>, OperationGraphError> {
        // Validate first so errors always contain a full cycle path.
        self.validate()?;
        let order = toposort(&self.graph, None).expect("validated operation graph must be acyclic");
        Ok(order
            .into_iter()
            .map(|index| self.graph[index].clone())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation(name: &str, kind: OperationKind) -> OperationId {
        OperationId::new(format!("resource.server.{name}"), kind)
    }

    #[test]
    fn stores_distinct_operations_for_one_address() {
        let mut graph = OperationGraph::new();
        let delete = operation("app", OperationKind::Delete);
        let create = operation("app", OperationKind::Create);
        graph.add_operation(delete.clone()).unwrap();
        graph.add_operation(create.clone()).unwrap();
        assert_eq!(graph.operation_count(), 2);
        assert!(graph.contains_operation(&delete));
        assert_eq!(graph.operation(&create), Some(&create));
        assert!(
            graph
                .operation(&operation("missing", OperationKind::Create))
                .is_none()
        );
    }

    #[test]
    fn rejects_duplicate_operations_without_mutation() {
        let mut graph = OperationGraph::new();
        let id = operation("app", OperationKind::Create);
        graph.add_operation(id.clone()).unwrap();
        assert_eq!(
            graph.add_operation(id.clone()),
            Err(OperationGraphError::DuplicateOperation { operation: id })
        );
        assert_eq!(graph.operation_count(), 1);
    }

    #[test]
    fn rejects_missing_edge_endpoints_without_mutation() {
        let mut graph = OperationGraph::new();
        let present = operation("app", OperationKind::Create);
        let missing = operation("missing", OperationKind::Create);
        graph.add_operation(present.clone()).unwrap();
        for (from, to) in [(&missing, &present), (&present, &missing)] {
            assert_eq!(
                graph.add_edge(from, to),
                Err(OperationGraphError::MissingOperation {
                    operation: missing.clone(),
                })
            );
        }
        assert_eq!(graph.edge_count(), 0);
        assert!(!graph.has_edge(&present, &missing));
    }

    #[test]
    fn orders_prerequisites_before_dependents_and_deduplicates_edges() {
        let mut graph = OperationGraph::new();
        let ids = [
            operation("a", OperationKind::Update),
            operation("b", OperationKind::DeferredDiff),
            operation("b", OperationKind::ExecuteDeferred),
        ];
        for id in &ids {
            graph.add_operation(id.clone()).unwrap();
        }
        for pair in ids.windows(2) {
            graph.add_edge(&pair[0], &pair[1]).unwrap();
            graph.add_edge(&pair[0], &pair[1]).unwrap();
            assert!(graph.has_edge(&pair[0], &pair[1]));
        }
        assert_eq!(graph.edge_count(), 2);
        assert_eq!(graph.validate(), Ok(()));
        assert_eq!(graph.topological_order().unwrap(), ids);
    }

    #[test]
    fn reports_full_cycle_path_in_disconnected_graph() {
        let mut graph = OperationGraph::new();
        graph
            .add_operation(operation("unrelated", OperationKind::NoOp))
            .unwrap();
        let a = operation("a", OperationKind::Update);
        let b = operation("b", OperationKind::Update);
        graph.add_operation(a.clone()).unwrap();
        graph.add_operation(b.clone()).unwrap();
        graph.add_edge(&a, &b).unwrap();
        graph.add_edge(&b, &a).unwrap();
        let path = graph.find_cycle().unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(path.first(), path.last());
        assert!(path.contains(&a));
        assert!(path.contains(&b));
        let error = OperationGraphError::Cycle { path };
        assert_eq!(graph.validate(), Err(error.clone()));
        assert_eq!(graph.topological_order(), Err(error));
    }

    #[test]
    fn reports_self_loop() {
        let mut graph = OperationGraph::new();
        let id = operation("app", OperationKind::Create);
        graph.add_operation(id.clone()).unwrap();
        graph.add_edge(&id, &id).unwrap();
        assert_eq!(graph.find_cycle(), Some(vec![id.clone(), id]));
    }

    #[test]
    fn empty_graph_is_valid() {
        let graph = OperationGraph::new();
        assert_eq!(graph.validate(), Ok(()));
        assert_eq!(graph.topological_order(), Ok(Vec::new()));
    }

    #[test]
    fn formats_ids_and_errors() {
        let id = operation("app", OperationKind::Create);
        assert_eq!(id.to_string(), "resource.server.app:create");
        assert_eq!(
            OperationGraphError::DuplicateOperation {
                operation: id.clone()
            }
            .to_string(),
            "duplicate operation `resource.server.app:create`"
        );
        assert_eq!(
            OperationGraphError::MissingOperation {
                operation: id.clone()
            }
            .to_string(),
            "missing operation `resource.server.app:create`"
        );
        assert_eq!(
            OperationGraphError::Cycle {
                path: vec![id.clone(), id]
            }
            .to_string(),
            "operation dependency cycle detected: resource.server.app:create -> resource.server.app:create"
        );
    }
}
