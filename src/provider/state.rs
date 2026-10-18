use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{ProviderValue, ResourceIdentity};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceState {
    pub identity: ResourceIdentity,
    pub outputs: BTreeMap<String, ProviderValue>,
}

impl ResourceState {
    pub fn new(identity: ResourceIdentity) -> Self {
        Self {
            identity,
            outputs: BTreeMap::new(),
        }
    }

    pub fn with_output(mut self, key: impl Into<String>, value: impl Into<ProviderValue>) -> Self {
        self.outputs.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DataState {
    pub outputs: BTreeMap<String, ProviderValue>,
}

impl DataState {
    pub fn new(outputs: BTreeMap<String, ProviderValue>) -> Self {
        Self { outputs }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn with_output(mut self, key: impl Into<String>, value: impl Into<ProviderValue>) -> Self {
        self.outputs.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_resource_state() {
        let state = ResourceState::new(ResourceIdentity::new("record-123"))
            .with_output("zone", "example.com")
            .with_output("ttl", 60_i64);

        assert_eq!(state.identity.id, "record-123");
        assert_eq!(
            state.outputs.get("zone"),
            Some(&ProviderValue::String("example.com".into()))
        );
        assert_eq!(state.outputs.get("ttl"), Some(&ProviderValue::Integer(60)));
    }

    #[test]
    fn creates_data_state() {
        let state = DataState::empty()
            .with_output("name", "example.com")
            .with_output("id", "zone-123");

        assert_eq!(
            state.outputs.get("name"),
            Some(&ProviderValue::String("example.com".into()))
        );
        assert_eq!(
            state.outputs.get("id"),
            Some(&ProviderValue::String("zone-123".into()))
        );
    }

    #[test]
    fn provider_results_serialize_outputs_only() {
        let resource =
            ResourceState::new(ResourceIdentity::new("record-123")).with_output("ttl", 60_i64);
        let json = serde_json::to_value(&resource).unwrap();
        assert_eq!(json["outputs"]["ttl"], 60);
        assert!(json.get("inputs").is_none());
        assert!(json.get("attributes").is_none());
        assert_eq!(
            serde_json::from_value::<ResourceState>(json).unwrap(),
            resource
        );

        let data = DataState::empty().with_output("id", "zone-123");
        let json = serde_json::to_value(&data).unwrap();
        assert_eq!(json["outputs"]["id"], "zone-123");
        assert!(json.get("inputs").is_none());
        assert!(json.get("attributes").is_none());
        assert_eq!(serde_json::from_value::<DataState>(json).unwrap(), data);
    }

    #[test]
    fn creates_data_state_from_outputs() {
        let state = DataState::new(BTreeMap::from([(
            "name".into(),
            ProviderValue::String("example.com".into()),
        )]));

        assert_eq!(state.outputs["name"].as_str(), Some("example.com"));
    }
}
