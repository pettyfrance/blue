use std::collections::{BTreeMap, BTreeSet};

use crate::graph::{DesiredGraph, GraphNodeKind, NodeId};
use crate::snapshot::Snapshot;

use super::{DesiredSnapshot, Plan, PlanAction};

#[cfg(test)]
#[path = "operation_graph_input_tests.rs"]
mod tests;

/// Provider checks require current configuration, including for historical deletes.
/// Provider identities in the snapshot are not reconnectable configurations.
pub struct OperationGraphInput<'a> {
    pub plan: &'a Plan,
    pub desired: &'a DesiredSnapshot,
    pub graph: &'a DesiredGraph,
    pub prior: Option<&'a Snapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OperationGraphInputError {
    #[error("duplicate plan entry `{address}`")]
    DuplicateChange { address: String },
    #[error("missing plan entry `{address}` (unchanged declarations need NoOp entries)")]
    MissingChange { address: String },
    #[error("inconsistent declaration `{address}`: {message}")]
    InconsistentDeclaration { address: String, message: String },
    #[error("blocked plan entry `{address}`")]
    BlockedChange { address: String },
    #[error("invalid action for `{address}`")]
    InvalidAction { address: String },
    #[error("missing historical resource `{address}`")]
    MissingHistoricalResource { address: String },
    #[error(
        "provider `{provider}` needed by `{address}` is missing from current configuration; restore its provider block until its resources are destroyed"
    )]
    MissingProvider { address: String, provider: String },
    #[error("missing historical dependency metadata for `{address}`")]
    MissingHistoricalDependencies { address: String },
    #[error("historical dependency cycle through `{address}`")]
    HistoricalCycle { address: String },
    #[error("desired graph contains a cycle: {path:?}")]
    DesiredCycle { path: Vec<NodeId> },
}

impl OperationGraphInput<'_> {
    pub fn validate(&self) -> Result<(), Vec<OperationGraphInputError>> {
        use OperationGraphInputError as E;
        let mut errors = Vec::new();
        if let Some(path) = self.graph.find_cycle() {
            errors.push(E::DesiredCycle { path });
        }
        let mut changes = BTreeMap::new();
        for change in &self.plan.changes {
            if changes.insert(change.address.as_str(), change).is_some() {
                errors.push(E::DuplicateChange {
                    address: change.address.clone(),
                });
            }
            if change.action == PlanAction::Blocked {
                errors.push(E::BlockedChange {
                    address: change.address.clone(),
                });
            }
        }
        if let Some(prior) = self.prior {
            for address in prior.resources.keys().chain(prior.data.keys()) {
                if !changes.contains_key(address.as_str()) {
                    errors.push(E::MissingChange {
                        address: address.clone(),
                    });
                }
            }
        }
        let nodes = self
            .graph
            .node_indices()
            .keys()
            .map(|id| (id.to_string(), id))
            .collect::<BTreeMap<_, _>>();
        let mut declarations = BTreeSet::new();
        for (address, provider, resource) in self
            .desired
            .resources
            .iter()
            .map(|(a, r)| (a, &r.provider, true))
            .chain(
                self.desired
                    .data
                    .iter()
                    .map(|(a, d)| (a, &d.provider, false)),
            )
        {
            declarations.insert(address.as_str());
            if !changes.contains_key(address.as_str()) {
                errors.push(E::MissingChange {
                    address: address.clone(),
                });
            }
            let matches = nodes
                .get(address)
                .and_then(|id| self.graph.node(id))
                .is_some_and(|node| match &node.kind {
                    GraphNodeKind::Resource { provider: p } => resource && p == provider,
                    GraphNodeKind::Data { provider: p } => !resource && p == provider,
                    _ => false,
                });
            let type_matches = match nodes.get(address) {
                Some(NodeId::Resource { resource_type, .. }) if resource => {
                    self.desired.resources[address].resource_type == *resource_type
                }
                Some(NodeId::Data { data_type, .. }) if !resource => {
                    self.desired.data[address].data_type == *data_type
                }
                _ => false,
            };
            if !matches || !type_matches {
                errors.push(E::InconsistentDeclaration {
                    address: address.clone(),
                    message: "desired model and graph disagree".into(),
                });
            }
            self.check_provider(address, provider, &mut errors);
        }
        for (address, id) in &nodes {
            if matches!(id, NodeId::Resource { .. } | NodeId::Data { .. })
                && !declarations.contains(address.as_str())
            {
                errors.push(E::InconsistentDeclaration {
                    address: address.clone(),
                    message: "graph declaration missing from desired model".into(),
                });
            }
        }
        for change in &self.plan.changes {
            let address = &change.address;
            let resource = self.desired.resources.contains_key(address);
            let data = self.desired.data.contains_key(address);
            let valid = match change.action {
                PlanAction::Create | PlanAction::Update | PlanAction::Replace => resource,
                PlanAction::Read => data,
                PlanAction::NoOp | PlanAction::Deferred => resource || data,
                PlanAction::Delete | PlanAction::Forget => !resource && !data,
                PlanAction::Blocked => true,
            };
            if !valid {
                errors.push(E::InvalidAction {
                    address: address.clone(),
                });
            }
            if matches!(
                change.action,
                PlanAction::Delete | PlanAction::Replace | PlanAction::Forget
            ) {
                let historical_resource = self.prior.and_then(|s| s.resources.get(address));
                let historical_data = self.prior.and_then(|s| s.data.get(address));
                if change.action == PlanAction::Forget && historical_data.is_some() {
                    continue;
                }
                if let Some(old) = historical_resource {
                    if change.action != PlanAction::Forget {
                        self.check_provider(address, &old.provider, &mut errors);
                        let snapshot = self.prior.unwrap();
                        if let Err(error) = check_history(
                            address,
                            snapshot,
                            &mut BTreeSet::new(),
                            &mut BTreeSet::new(),
                        ) {
                            errors.push(error);
                        }
                    }
                } else {
                    errors.push(E::MissingHistoricalResource {
                        address: address.clone(),
                    });
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn check_provider(
        &self,
        address: &str,
        provider: &str,
        errors: &mut Vec<OperationGraphInputError>,
    ) {
        if !self.graph.contains_node(&NodeId::Provider {
            name: provider.into(),
        }) {
            errors.push(OperationGraphInputError::MissingProvider {
                address: address.into(),
                provider: provider.into(),
            });
        }
    }
}

fn check_history(
    address: &str,
    snapshot: &Snapshot,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> Result<(), OperationGraphInputError> {
    if visited.contains(address) {
        return Ok(());
    }
    if !visiting.insert(address.into()) {
        return Err(OperationGraphInputError::HistoricalCycle {
            address: address.into(),
        });
    }
    let dependencies = snapshot.dependencies.get(address).ok_or_else(|| {
        OperationGraphInputError::MissingHistoricalDependencies {
            address: address.into(),
        }
    })?;
    for dependency in dependencies {
        check_history(dependency, snapshot, visiting, visited)?;
    }
    visiting.remove(address);
    visited.insert(address.into());
    Ok(())
}
