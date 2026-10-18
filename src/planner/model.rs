use std::collections::BTreeMap;

use crate::provider::ProviderValue;
use crate::snapshot::{Snapshot, SnapshotData, SnapshotProvider, SnapshotResource};

#[derive(Debug, Clone, PartialEq)]
pub enum DesiredValue {
    Known(ProviderValue),
    Unknown { reason: Option<String> },
}

impl DesiredValue {
    pub fn known(value: impl Into<ProviderValue>) -> Self {
        Self::Known(value.into())
    }

    pub fn unknown() -> Self {
        Self::Unknown { reason: None }
    }

    pub fn unknown_with_reason(reason: impl Into<String>) -> Self {
        Self::Unknown {
            reason: Some(reason.into()),
        }
    }

    pub fn is_known(&self) -> bool {
        matches!(self, Self::Known(_))
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown { .. })
    }

    pub fn as_known(&self) -> Option<&ProviderValue> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unknown { .. } => None,
        }
    }

    pub fn unknown_reason(&self) -> Option<&str> {
        match self {
            Self::Known(_) => None,
            Self::Unknown { reason } => reason.as_deref(),
        }
    }
}

pub type DesiredInputs = BTreeMap<String, DesiredValue>;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DesiredSnapshot {
    pub resources: BTreeMap<String, DesiredResource>,
    pub data: BTreeMap<String, DesiredData>,
}

impl DesiredSnapshot {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesiredResource {
    pub provider: String,
    pub resource_type: String,
    pub inputs: DesiredInputs,
    pub dependencies: Vec<String>,
}

impl DesiredResource {
    pub fn new(provider: impl Into<String>, resource_type: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            resource_type: resource_type.into(),
            inputs: BTreeMap::new(),
            dependencies: Vec::new(),
        }
    }

    pub fn with_input(mut self, key: impl Into<String>, value: DesiredValue) -> Self {
        self.inputs.insert(key.into(), value);
        self
    }

    pub fn with_dependency(mut self, dependency: impl Into<String>) -> Self {
        self.dependencies.push(dependency.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesiredData {
    pub provider: String,
    pub data_type: String,
    pub inputs: DesiredInputs,
    pub dependencies: Vec<String>,
}

impl DesiredData {
    pub fn new(provider: impl Into<String>, data_type: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            data_type: data_type.into(),
            inputs: BTreeMap::new(),
            dependencies: Vec::new(),
        }
    }

    pub fn with_input(mut self, key: impl Into<String>, value: DesiredValue) -> Self {
        self.inputs.insert(key.into(), value);
        self
    }

    pub fn with_dependency(mut self, dependency: impl Into<String>) -> Self {
        self.dependencies.push(dependency.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RefreshResult {
    NotRun,
    Complete(RefreshedSnapshot),
}

impl RefreshResult {
    pub fn is_not_run(&self) -> bool {
        matches!(self, Self::NotRun)
    }

    pub fn as_complete(&self) -> Option<&RefreshedSnapshot> {
        match self {
            Self::NotRun => None,
            Self::Complete(snapshot) => Some(snapshot),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RefreshedSnapshot {
    pub providers: BTreeMap<String, RefreshedProvider>,
    pub resources: BTreeMap<String, RefreshedResource>,
    pub data: BTreeMap<String, RefreshedData>,
}

impl RefreshedSnapshot {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RefreshedProvider {
    Present(SnapshotProvider),
    RefreshFailed { message: String },
}

impl RefreshedProvider {
    pub fn refresh_failed(message: impl Into<String>) -> Self {
        Self::RefreshFailed {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RefreshedResource {
    Present(SnapshotResource),
    MissingRemote,
    RefreshFailed { message: String },
}

impl RefreshedResource {
    pub fn refresh_failed(message: impl Into<String>) -> Self {
        Self::RefreshFailed {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RefreshedData {
    Present(SnapshotData),
    ReadFailed { message: String },
}

impl RefreshedData {
    pub fn read_failed(message: impl Into<String>) -> Self {
        Self::ReadFailed {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannerInput {
    pub desired: DesiredSnapshot,
    pub prior: Option<Snapshot>,
    pub refresh: RefreshResult,
}

impl PlannerInput {
    pub fn new(desired: DesiredSnapshot) -> Self {
        Self {
            desired,
            prior: None,
            refresh: RefreshResult::NotRun,
        }
    }

    pub fn with_prior(mut self, prior: Snapshot) -> Self {
        self.prior = Some(prior);
        self
    }

    pub fn with_refresh(mut self, refresh: RefreshResult) -> Self {
        self.refresh = refresh;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    pub changes: Vec<PlanChange>,
}

impl Plan {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_change(mut self, change: PlanChange) -> Self {
        self.changes.push(change);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlannedObject {
    pub inputs: BTreeMap<String, DesiredValue>,
    pub outputs: BTreeMap<String, DesiredValue>,
}

impl PlannedObject {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_input(mut self, key: impl Into<String>, value: DesiredValue) -> Self {
        self.inputs.insert(key.into(), value);
        self
    }

    pub fn with_output(mut self, key: impl Into<String>, value: DesiredValue) -> Self {
        self.outputs.insert(key.into(), value);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanChange {
    pub address: String,
    pub action: PlanAction,
    pub reason: PlanChangeReason,
    pub before: Option<PlannedObject>,
    pub after: Option<PlannedObject>,
}

impl PlanChange {
    pub fn new(address: impl Into<String>, action: PlanAction, reason: PlanChangeReason) -> Self {
        Self {
            address: address.into(),
            action,
            reason,
            before: None,
            after: None,
        }
    }

    pub fn with_before(mut self, before: PlannedObject) -> Self {
        self.before = Some(before);
        self
    }

    pub fn with_after(mut self, after: PlannedObject) -> Self {
        self.after = Some(after);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanAction {
    NoOp,
    Read,
    Create,
    Update,
    Delete,
    Replace,
    Forget,
    /// Requires concrete inputs before the action can be decided.
    Deferred,
    /// Cannot proceed because of an error.
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanChangeReason {
    NoChanges,
    ReadData,
    NewResource,
    DesiredChanged,
    RemovedFromConfig,
    RemoteMissing,
    AlreadyGone,
    RefreshFailed { message: String },
    ContainsUnknowns,
    DependsOnChanges { dependencies: Vec<String> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planned_object_defaults_to_empty_values() {
        let object = PlannedObject::new();
        assert!(object.inputs.is_empty());
        assert!(object.outputs.is_empty());
    }

    #[test]
    fn create_change_has_after_only_with_unknown_output() {
        let after = PlannedObject::new()
            .with_input("name", DesiredValue::known("new-name"))
            .with_output("urn", DesiredValue::unknown());
        let change = PlanChange::new(
            "resource.server.app",
            PlanAction::Create,
            PlanChangeReason::NewResource,
        )
        .with_after(after.clone());

        assert!(change.before.is_none());
        assert_eq!(change.after, Some(after));
        assert!(change.after.as_ref().unwrap().outputs["urn"].is_unknown());
    }

    #[test]
    fn delete_and_forget_changes_have_before_only() {
        let before = PlannedObject::new()
            .with_input("name", DesiredValue::known("old-name"))
            .with_output("urn", DesiredValue::known("old-urn"));
        for (action, reason) in [
            (PlanAction::Delete, PlanChangeReason::RemovedFromConfig),
            (PlanAction::Forget, PlanChangeReason::AlreadyGone),
        ] {
            let change =
                PlanChange::new("resource.server.app", action, reason).with_before(before.clone());

            assert_eq!(change.before, Some(before.clone()));
            assert!(change.after.is_none());
        }
    }

    #[test]
    fn update_and_replace_changes_have_before_and_after() {
        let before = PlannedObject::new()
            .with_input("name", DesiredValue::known("old-name"))
            .with_output("urn", DesiredValue::known("old-urn"));
        let after = PlannedObject::new()
            .with_input("name", DesiredValue::known("new-name"))
            .with_output(
                "urn",
                DesiredValue::unknown_with_reason("known after apply"),
            );
        for action in [PlanAction::Update, PlanAction::Replace] {
            let change = PlanChange::new(
                "resource.server.app",
                action,
                PlanChangeReason::DesiredChanged,
            )
            .with_before(before.clone())
            .with_after(after.clone());

            assert_eq!(change.before, Some(before.clone()));
            assert_eq!(change.after, Some(after.clone()));
        }
    }

    #[test]
    fn creates_known_desired_value() {
        let value = DesiredValue::known("example.com");

        assert!(value.is_known());
        assert!(!value.is_unknown());
        assert_eq!(value.as_known().unwrap().as_str(), Some("example.com"));
        assert_eq!(value.unknown_reason(), None);
    }

    #[test]
    fn creates_unknown_desired_value() {
        let value = DesiredValue::unknown();

        assert!(!value.is_known());
        assert!(value.is_unknown());
        assert!(value.as_known().is_none());
        assert_eq!(value.unknown_reason(), None);
    }

    #[test]
    fn creates_unknown_desired_value_with_reason() {
        let value =
            DesiredValue::unknown_with_reason("resource.dns_zone.main.id known after apply");

        assert!(value.is_unknown());
        assert_eq!(
            value.unknown_reason(),
            Some("resource.dns_zone.main.id known after apply")
        );
    }

    #[test]
    fn creates_desired_snapshot() {
        let mut desired = DesiredSnapshot::new();
        desired.resources.insert(
            "resource.dns_record.www".into(),
            DesiredResource::new("ovh_prod", "dns_record")
                .with_input("name", DesiredValue::known("www"))
                .with_input(
                    "zone_id",
                    DesiredValue::unknown_with_reason(
                        "data.domain_zone.main.id known after data read",
                    ),
                )
                .with_dependency("provider.ovh_prod")
                .with_dependency("data.domain_zone.main"),
        );
        desired.data.insert(
            "data.domain_zone.main".into(),
            DesiredData::new("ovh_prod", "domain_zone")
                .with_input("name", DesiredValue::known("example.com"))
                .with_dependency("provider.ovh_prod"),
        );

        assert_eq!(
            desired.resources["resource.dns_record.www"].provider,
            "ovh_prod"
        );
        assert_eq!(
            desired.resources["resource.dns_record.www"].resource_type,
            "dns_record"
        );
        assert!(desired.resources["resource.dns_record.www"].inputs["zone_id"].is_unknown());
        assert_eq!(
            desired.data["data.domain_zone.main"].data_type,
            "domain_zone"
        );
        assert_eq!(
            desired.data["data.domain_zone.main"].dependencies,
            vec!["provider.ovh_prod"]
        );
    }

    #[test]
    fn creates_refresh_result_model() {
        let mut refreshed = RefreshedSnapshot::new();
        refreshed.providers.insert(
            "ovh_prod".into(),
            RefreshedProvider::refresh_failed("authentication failed"),
        );
        refreshed.resources.insert(
            "resource.dns_record.www".into(),
            RefreshedResource::MissingRemote,
        );
        refreshed.data.insert(
            "data.domain_zone.main".into(),
            RefreshedData::read_failed("api timeout"),
        );

        let result = RefreshResult::Complete(refreshed);

        assert!(!result.is_not_run());
        let refreshed = result.as_complete().unwrap();
        assert_eq!(
            refreshed.providers["ovh_prod"],
            RefreshedProvider::RefreshFailed {
                message: "authentication failed".into()
            }
        );
        assert_eq!(
            refreshed.resources["resource.dns_record.www"],
            RefreshedResource::MissingRemote
        );
        assert_eq!(
            refreshed.data["data.domain_zone.main"],
            RefreshedData::ReadFailed {
                message: "api timeout".into()
            }
        );
    }

    #[test]
    fn refresh_result_can_be_not_run() {
        let result = RefreshResult::NotRun;

        assert!(result.is_not_run());
        assert!(result.as_complete().is_none());
    }

    #[test]
    fn creates_planner_input() {
        let desired = DesiredSnapshot::new();
        let prior = Snapshot::new();
        let refresh = RefreshResult::Complete(RefreshedSnapshot::new());

        let input = PlannerInput::new(desired.clone())
            .with_prior(prior.clone())
            .with_refresh(refresh.clone());

        assert_eq!(input.desired, desired);
        assert_eq!(input.prior, Some(prior));
        assert_eq!(input.refresh, refresh);
    }

    #[test]
    fn planner_input_defaults_to_no_prior_and_refresh_not_run() {
        let input = PlannerInput::new(DesiredSnapshot::new());

        assert!(input.prior.is_none());
        assert_eq!(input.refresh, RefreshResult::NotRun);
    }

    #[test]
    fn creates_plan_output_model() {
        let plan = Plan::new()
            .with_change(PlanChange::new(
                "data.domain_zone.main",
                PlanAction::Read,
                PlanChangeReason::ReadData,
            ))
            .with_change(PlanChange::new(
                "resource.dns_record.www",
                PlanAction::Create,
                PlanChangeReason::NewResource,
            ));

        assert!(!plan.is_empty());
        assert_eq!(plan.changes.len(), 2);
        assert_eq!(plan.changes[0].address, "data.domain_zone.main");
        assert_eq!(plan.changes[0].action, PlanAction::Read);
        assert_eq!(plan.changes[0].reason, PlanChangeReason::ReadData);
    }

    #[test]
    fn deferred_plan_change_is_distinct_from_blocked() {
        let change = PlanChange::new(
            "resource.dns_record.www",
            PlanAction::Deferred,
            PlanChangeReason::ContainsUnknowns,
        );

        assert_eq!(change.action, PlanAction::Deferred);
        assert_ne!(change.action, PlanAction::Blocked);
    }

    #[test]
    fn plan_change_reasons_can_carry_structured_context() {
        let refresh_failed = PlanChange::new(
            "resource.dns_record.www",
            PlanAction::Blocked,
            PlanChangeReason::RefreshFailed {
                message: "authentication failed".into(),
            },
        );
        let depends_on_changes = PlanChange::new(
            "resource.dns_record.alias",
            PlanAction::Blocked,
            PlanChangeReason::DependsOnChanges {
                dependencies: vec!["resource.dns_record.www".into()],
            },
        );

        assert_eq!(
            refresh_failed.reason,
            PlanChangeReason::RefreshFailed {
                message: "authentication failed".into()
            }
        );
        assert_eq!(
            depends_on_changes.reason,
            PlanChangeReason::DependsOnChanges {
                dependencies: vec!["resource.dns_record.www".into()]
            }
        );
    }

    #[test]
    fn desired_inputs_map_uses_desired_values() {
        let inputs = DesiredInputs::from([
            ("name".into(), DesiredValue::known("www")),
            (
                "zone_id".into(),
                DesiredValue::unknown_with_reason("data.domain_zone.main.id known after read"),
            ),
        ]);

        assert!(inputs["name"].is_known());
        assert!(inputs["zone_id"].is_unknown());
    }
}
