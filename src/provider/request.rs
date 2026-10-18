use std::collections::BTreeMap;

use crate::planner::DesiredInputs;
use crate::snapshot::SnapshotResource;

use super::{ProviderValue, ResourceIdentity, ResourceState};

#[derive(Debug, Clone, PartialEq)]
pub struct DiffResourceRequest {
    pub provider_name: String,
    pub prior: Option<SnapshotResource>,
    pub refreshed: Option<SnapshotResource>,
    pub desired_inputs: DesiredInputs,
}

impl DiffResourceRequest {
    pub fn new(provider_name: impl Into<String>, desired_inputs: DesiredInputs) -> Self {
        Self {
            provider_name: provider_name.into(),
            prior: None,
            refreshed: None,
            desired_inputs,
        }
    }

    pub fn with_prior(mut self, prior: SnapshotResource) -> Self {
        self.prior = Some(prior);
        self
    }

    pub fn with_refreshed(mut self, refreshed: SnapshotResource) -> Self {
        self.refreshed = Some(refreshed);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateResourceRequest {
    pub provider_name: String,
    pub inputs: BTreeMap<String, ProviderValue>,
}

impl CreateResourceRequest {
    pub fn new(provider_name: impl Into<String>, inputs: BTreeMap<String, ProviderValue>) -> Self {
        Self {
            provider_name: provider_name.into(),
            inputs,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReadResourceRequest {
    pub provider_name: String,
    pub identity: ResourceIdentity,
    pub prior_state: Option<ResourceState>,
}

impl ReadResourceRequest {
    pub fn new(provider_name: impl Into<String>, identity: ResourceIdentity) -> Self {
        Self {
            provider_name: provider_name.into(),
            identity,
            prior_state: None,
        }
    }

    pub fn with_prior_state(mut self, prior_state: ResourceState) -> Self {
        self.prior_state = Some(prior_state);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateResourceRequest {
    pub provider_name: String,
    pub identity: ResourceIdentity,
    pub prior_state: ResourceState,
    pub inputs: BTreeMap<String, ProviderValue>,
}

impl UpdateResourceRequest {
    pub fn new(
        provider_name: impl Into<String>,
        identity: ResourceIdentity,
        prior_state: ResourceState,
        inputs: BTreeMap<String, ProviderValue>,
    ) -> Self {
        Self {
            provider_name: provider_name.into(),
            identity,
            prior_state,
            inputs,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeleteResourceRequest {
    pub provider_name: String,
    pub identity: ResourceIdentity,
    pub prior_state: ResourceState,
}

impl DeleteResourceRequest {
    pub fn new(
        provider_name: impl Into<String>,
        identity: ResourceIdentity,
        prior_state: ResourceState,
    ) -> Self {
        Self {
            provider_name: provider_name.into(),
            identity,
            prior_state,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReadDataRequest {
    pub provider_name: String,
    pub inputs: BTreeMap<String, ProviderValue>,
}

impl ReadDataRequest {
    pub fn new(provider_name: impl Into<String>, inputs: BTreeMap<String, ProviderValue>) -> Self {
        Self {
            provider_name: provider_name.into(),
            inputs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> BTreeMap<String, ProviderValue> {
        BTreeMap::from([("name".into(), ProviderValue::from("example.com"))])
    }

    fn resource_state() -> ResourceState {
        ResourceState::new(ResourceIdentity::new("record-123")).with_output("name", "example.com")
    }

    #[test]
    fn creates_diff_resource_request() {
        let prior =
            crate::snapshot::SnapshotResource::new("ovh_prod", ResourceIdentity::new("record-123"));
        let refreshed =
            crate::snapshot::SnapshotResource::new("ovh_prod", ResourceIdentity::new("record-123"));
        let request = DiffResourceRequest::new(
            "ovh_prod",
            BTreeMap::from([(
                "name".into(),
                crate::planner::DesiredValue::known("example.com"),
            )]),
        )
        .with_prior(prior.clone())
        .with_refreshed(refreshed.clone());

        assert_eq!(request.provider_name, "ovh_prod");
        assert_eq!(request.prior, Some(prior));
        assert_eq!(request.refreshed, Some(refreshed));
        assert!(request.desired_inputs["name"].is_known());
    }

    #[test]
    fn creates_create_resource_request() {
        let request = CreateResourceRequest::new("ovh_prod", inputs());

        assert_eq!(request.provider_name, "ovh_prod");
        assert_eq!(request.inputs["name"].as_str(), Some("example.com"));
    }

    #[test]
    fn creates_read_resource_request() {
        let request = ReadResourceRequest::new("ovh_prod", ResourceIdentity::new("record-123"));

        assert_eq!(request.provider_name, "ovh_prod");
        assert_eq!(request.identity.id, "record-123");
        assert!(request.prior_state.is_none());
    }

    #[test]
    fn creates_read_resource_request_with_prior_state() {
        let request = ReadResourceRequest::new("ovh_prod", ResourceIdentity::new("record-123"))
            .with_prior_state(resource_state());

        assert_eq!(
            request.prior_state.unwrap().outputs["name"].as_str(),
            Some("example.com")
        );
    }

    #[test]
    fn creates_update_resource_request() {
        let request = UpdateResourceRequest::new(
            "ovh_prod",
            ResourceIdentity::new("record-123"),
            resource_state(),
            inputs(),
        );

        assert_eq!(request.provider_name, "ovh_prod");
        assert_eq!(request.identity.id, "record-123");
        assert_eq!(request.prior_state.identity.id, "record-123");
        assert_eq!(request.inputs["name"].as_str(), Some("example.com"));
    }

    #[test]
    fn creates_delete_resource_request() {
        let request = DeleteResourceRequest::new(
            "ovh_prod",
            ResourceIdentity::new("record-123"),
            resource_state(),
        );

        assert_eq!(request.provider_name, "ovh_prod");
        assert_eq!(request.identity.id, "record-123");
        assert_eq!(request.prior_state.identity.id, "record-123");
    }

    #[test]
    fn creates_read_data_request() {
        let request = ReadDataRequest::new("ovh_prod", inputs());

        assert_eq!(request.provider_name, "ovh_prod");
        assert_eq!(request.inputs["name"].as_str(), Some("example.com"));
    }
}
