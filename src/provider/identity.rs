use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderIdentity {
    pub provider_type: String,
    pub attributes: BTreeMap<String, String>,
}

impl ProviderIdentity {
    pub fn new(provider_type: impl Into<String>) -> Self {
        Self {
            provider_type: provider_type.into(),
            attributes: BTreeMap::new(),
        }
    }

    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceIdentity {
    pub id: String,
    pub attributes: BTreeMap<String, String>,
}

impl ResourceIdentity {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            attributes: BTreeMap::new(),
        }
    }

    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_provider_identity() {
        let identity = ProviderIdentity::new("ovh").with_attribute("account_id", "abc123");

        assert_eq!(identity.provider_type, "ovh");
        assert_eq!(
            identity.attributes.get("account_id"),
            Some(&"abc123".into())
        );
    }

    #[test]
    fn creates_resource_identity() {
        let identity = ResourceIdentity::new("record-123").with_attribute("zone", "example.com");

        assert_eq!(identity.id, "record-123");
        assert_eq!(identity.attributes.get("zone"), Some(&"example.com".into()));
    }
}
