pub mod dependencies;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::provider::{ProviderIdentity, ProviderValue, ResourceIdentity};

pub use dependencies::*;

pub const CURRENT_SNAPSHOT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub providers: BTreeMap<String, SnapshotProvider>,
    pub resources: BTreeMap<String, SnapshotResource>,
    pub data: BTreeMap<String, SnapshotData>,
    /// Complete historical declaration topology, including provider/parameter leaves.
    pub dependencies: BTreeMap<String, Vec<String>>,
}

impl Snapshot {
    pub fn new() -> Self {
        Self {
            version: CURRENT_SNAPSHOT_VERSION,
            providers: BTreeMap::new(),
            resources: BTreeMap::new(),
            data: BTreeMap::new(),
            dependencies: BTreeMap::new(),
        }
    }
}

impl Default for Snapshot {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotProvider {
    pub provider_type: String,
    pub identity: ProviderIdentity,
}

impl SnapshotProvider {
    pub fn new(provider_type: impl Into<String>, identity: ProviderIdentity) -> Self {
        Self {
            provider_type: provider_type.into(),
            identity,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotResource {
    pub provider: String,
    pub identity: ResourceIdentity,
    /// Evaluated inputs from the last successful operation.
    pub inputs: BTreeMap<String, ProviderValue>,
    /// Current provider-observed values; refresh updates these, not inputs.
    pub outputs: BTreeMap<String, ProviderValue>,
}

impl SnapshotResource {
    pub fn new(provider: impl Into<String>, identity: ResourceIdentity) -> Self {
        Self {
            provider: provider.into(),
            identity,
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
        }
    }

    pub fn with_input(mut self, key: impl Into<String>, value: impl Into<ProviderValue>) -> Self {
        self.inputs.insert(key.into(), value.into());
        self
    }

    pub fn with_output(mut self, key: impl Into<String>, value: impl Into<ProviderValue>) -> Self {
        self.outputs.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotData {
    pub provider: String,
    /// Evaluated inputs from the last successful operation.
    pub inputs: BTreeMap<String, ProviderValue>,
    /// Current provider-observed values; refresh updates these, not inputs.
    pub outputs: BTreeMap<String, ProviderValue>,
}

impl SnapshotData {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
        }
    }

    pub fn with_input(mut self, key: impl Into<String>, value: impl Into<ProviderValue>) -> Self {
        self.inputs.insert(key.into(), value.into());
        self
    }

    pub fn with_output(mut self, key: impl Into<String>, value: impl Into<ProviderValue>) -> Self {
        self.outputs.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        let mut snapshot = Snapshot::new();

        snapshot.providers.insert(
            "ovh_prod".into(),
            SnapshotProvider::new(
                "ovh",
                ProviderIdentity::new("ovh")
                    .with_attribute("account_id", "abc123")
                    .with_attribute("endpoint", "ovh-eu"),
            ),
        );

        snapshot.resources.insert(
            "resource.dns_record.www".into(),
            SnapshotResource::new(
                "ovh_prod",
                ResourceIdentity::new("record-123").with_attribute("zone", "example.com"),
            )
            .with_input("zone", "example.com")
            .with_input("ttl", 60_i64)
            .with_output("zone", "example.com")
            .with_output("ttl", 120_i64),
        );

        snapshot.data.insert(
            "data.domain_zone.main".into(),
            SnapshotData::new("ovh_prod")
                .with_input("name", "Example.com")
                .with_output("name", "example.com")
                .with_output("id", "zone-123"),
        );

        snapshot.dependencies = BTreeMap::from([
            ("provider.ovh_prod".into(), vec![]),
            (
                "resource.dns_record.www".into(),
                vec!["data.domain_zone.main".into(), "provider.ovh_prod".into()],
            ),
            (
                "data.domain_zone.main".into(),
                vec!["provider.ovh_prod".into()],
            ),
        ]);
        snapshot
    }

    #[test]
    fn creates_empty_snapshot() {
        let snapshot = Snapshot::new();

        assert_eq!(snapshot.version, CURRENT_SNAPSHOT_VERSION);
        assert!(snapshot.providers.is_empty());
        assert!(snapshot.resources.is_empty());
        assert!(snapshot.data.is_empty());
    }

    #[test]
    fn serializes_snapshot_to_json() {
        let json = serde_json::to_string_pretty(&snapshot()).unwrap();

        assert!(json.contains("\"version\": 1"));
        assert!(json.contains("\"ovh_prod\""));
        assert!(json.contains("\"resource.dns_record.www\""));
        assert!(json.contains("\"data.domain_zone.main\""));
        assert!(json.contains("\"account_id\": "));
        assert!(json.contains("\"dependencies\""));
    }

    #[test]
    fn serializes_inputs_and_outputs_separately() {
        let json = serde_json::to_value(snapshot()).unwrap();
        let resource = &json["resources"]["resource.dns_record.www"];
        assert_eq!(resource["inputs"]["ttl"], 60);
        assert_eq!(resource["outputs"]["ttl"], 120);
        assert!(resource.get("attributes").is_none());

        let data = &json["data"]["data.domain_zone.main"];
        assert_eq!(data["inputs"]["name"], "Example.com");
        assert_eq!(data["outputs"]["name"], "example.com");
        assert!(data["inputs"].get("id").is_none());
        assert_eq!(data["outputs"]["id"], "zone-123");
        assert!(data.get("attributes").is_none());
    }

    #[test]
    fn round_trips_snapshot_json() {
        let snapshot = snapshot();
        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        let decoded: Snapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, snapshot);
    }
}
