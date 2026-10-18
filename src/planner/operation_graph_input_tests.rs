use std::collections::BTreeMap;

use super::*;
use crate::graph::{GraphNode, GraphNodeKind};
use crate::planner::{DesiredResource, PlanChange, PlanChangeReason};
use crate::provider::ResourceIdentity;
use crate::snapshot::{SnapshotDependencies, SnapshotResource};

fn fixture() -> (DesiredGraph, DesiredSnapshot, Plan) {
    let mut graph = DesiredGraph::new();
    let provider = NodeId::Provider {
        name: "cloud".into(),
    };
    let resource = NodeId::Resource {
        resource_type: "server".into(),
        name: "app".into(),
    };
    graph.add_node(GraphNode {
        id: provider.clone(),
        kind: GraphNodeKind::Provider,
    });
    graph.add_node(GraphNode {
        id: resource.clone(),
        kind: GraphNodeKind::Resource {
            provider: "cloud".into(),
        },
    });
    graph.add_edge(&provider, &resource, crate::graph::EdgeKind::Provider);
    let mut desired = DesiredSnapshot::new();
    desired.resources.insert(
        resource.to_string(),
        DesiredResource::new("cloud", "server"),
    );
    let plan = Plan::new().with_change(PlanChange::new(
        resource.to_string(),
        PlanAction::NoOp,
        PlanChangeReason::NoChanges,
    ));
    (graph, desired, plan)
}

fn errors(
    graph: &DesiredGraph,
    desired: &DesiredSnapshot,
    plan: &Plan,
    prior: Option<&Snapshot>,
) -> Vec<OperationGraphInputError> {
    OperationGraphInput {
        graph,
        desired,
        plan,
        prior,
    }
    .validate()
    .unwrap_err()
}

#[test]
fn accepts_complete_noop_plan() {
    let (graph, desired, plan) = fixture();
    assert_eq!(
        OperationGraphInput {
            graph: &graph,
            desired: &desired,
            plan: &plan,
            prior: None
        }
        .validate(),
        Ok(())
    );
}

#[test]
fn rejects_missing_duplicate_and_blocked_changes() {
    let (graph, desired, mut plan) = fixture();
    assert!(
        errors(&graph, &desired, &Plan::new(), None)
            .iter()
            .any(|e| matches!(e, OperationGraphInputError::MissingChange { .. }))
    );
    plan.changes.push(plan.changes[0].clone());
    assert!(
        errors(&graph, &desired, &plan, None)
            .iter()
            .any(|e| matches!(e, OperationGraphInputError::DuplicateChange { .. }))
    );
    plan.changes.pop();
    plan.changes[0].action = PlanAction::Blocked;
    assert!(
        errors(&graph, &desired, &plan, None)
            .iter()
            .any(|e| matches!(e, OperationGraphInputError::BlockedChange { .. }))
    );
}

#[test]
fn rejects_inconsistent_desired_types_and_invalid_actions() {
    let (graph, mut desired, mut plan) = fixture();
    desired
        .resources
        .get_mut("resource.server.app")
        .unwrap()
        .resource_type = "other".into();
    assert!(
        errors(&graph, &desired, &plan, None)
            .iter()
            .any(|e| matches!(e, OperationGraphInputError::InconsistentDeclaration { .. }))
    );
    desired
        .resources
        .get_mut("resource.server.app")
        .unwrap()
        .resource_type = "server".into();
    plan.changes[0].action = PlanAction::Read;
    assert!(
        errors(&graph, &desired, &plan, None)
            .iter()
            .any(|e| matches!(e, OperationGraphInputError::InvalidAction { .. }))
    );
}

#[test]
fn historical_delete_requires_current_provider_block() {
    let graph = DesiredGraph::new();
    let desired = DesiredSnapshot::new();
    let mut prior = Snapshot::new();
    prior.resources.insert(
        "resource.server.old".into(),
        SnapshotResource::new("removed", ResourceIdentity::new("123")),
    );
    prior.dependencies = BTreeMap::from([
        (
            "resource.server.old".into(),
            vec!["provider.removed".into()],
        ),
        ("provider.removed".into(), vec![]),
    ]);
    let plan = Plan::new().with_change(PlanChange::new(
        "resource.server.old",
        PlanAction::Delete,
        PlanChangeReason::RemovedFromConfig,
    ));
    let errors = errors(&graph, &desired, &plan, Some(&prior));
    let error = errors
        .iter()
        .find(|e| matches!(e, OperationGraphInputError::MissingProvider { .. }))
        .unwrap();
    assert!(error.to_string().contains("restore its provider block"));
}

#[test]
fn validates_historical_paths_through_parameters_and_data() {
    let (graph, desired, plan) = fixture();
    let mut prior = Snapshot::new();
    prior.resources.insert(
        "resource.server.app".into(),
        SnapshotResource::new("cloud", ResourceIdentity::new("123")),
    );
    prior.dependencies = SnapshotDependencies::from_graph(&graph);
    prior
        .dependencies
        .insert("resource.server.app".into(), vec!["parameter.x".into()]);
    prior
        .dependencies
        .insert("parameter.x".into(), vec!["data.info.old".into()]);
    prior
        .dependencies
        .insert("data.info.old".into(), vec!["provider.cloud".into()]);
    let mut plan = plan;
    plan.changes[0].action = PlanAction::Replace;
    assert!(
        OperationGraphInput {
            graph: &graph,
            desired: &desired,
            plan: &plan,
            prior: Some(&prior)
        }
        .validate()
        .is_ok()
    );
    prior.dependencies.remove("data.info.old");
    assert!(errors(&graph, &desired, &plan, Some(&prior)).iter().any(|e| matches!(e, OperationGraphInputError::MissingHistoricalDependencies { address } if address == "data.info.old")));
    prior
        .dependencies
        .insert("data.info.old".into(), vec!["parameter.x".into()]);
    assert!(
        errors(&graph, &desired, &plan, Some(&prior))
            .iter()
            .any(|e| matches!(e, OperationGraphInputError::HistoricalCycle { .. }))
    );
}

#[test]
fn accepts_deferred_plan_and_rejects_replace_without_history() {
    let (graph, desired, mut plan) = fixture();
    plan.changes[0].action = PlanAction::Deferred;
    assert!(
        OperationGraphInput {
            graph: &graph,
            desired: &desired,
            plan: &plan,
            prior: None
        }
        .validate()
        .is_ok()
    );
    plan.changes[0].action = PlanAction::Replace;
    assert!(
        errors(&graph, &desired, &plan, None)
            .iter()
            .any(|e| matches!(
                e,
                OperationGraphInputError::MissingHistoricalResource { .. }
            ))
    );
}

#[test]
fn formats_provider_check_operation() {
    assert_eq!(
        crate::planner::OperationId::new(
            "provider.cloud",
            crate::planner::OperationKind::CheckProvider
        )
        .to_string(),
        "provider.cloud:check-provider"
    );
}
