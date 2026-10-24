use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::error::Error;
use std::fmt;

use super::Provider;

/// Selected provider implementations, keyed by configuration name rather than
/// provider type. Discovery/loading is responsible for selecting each entry.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: BTreeMap<String, Box<dyn Provider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an implementation without replacing an existing name.
    /// Multiple names may use the same provider type or different versions of it.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        provider: Box<dyn Provider>,
    ) -> Result<(), ProviderRegistrationError> {
        match self.providers.entry(name.into()) {
            Entry::Vacant(entry) => {
                entry.insert(provider);
                Ok(())
            }
            Entry::Occupied(entry) => Err(ProviderRegistrationError::DuplicateName {
                name: entry.key().clone(),
            }),
        }
    }

    pub fn get(&self, name: &str) -> Option<&dyn Provider> {
        self.providers.get(name).map(|provider| provider.as_ref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderRegistrationError {
    DuplicateName { name: String },
}

impl fmt::Display for ProviderRegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateName { name } => {
                write!(f, "provider name `{name}` is already registered")
            }
        }
    }
}

impl Error for ProviderRegistrationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ResourceType, Schema};
    use crate::providers::ovh::OvhProvider;

    // An independent implementation advertising the same type as the built-in.
    // Its different schema makes it observable which implementation was selected.
    #[derive(Default)]
    struct AlternateOvh {
        schema: Schema,
    }

    impl Provider for AlternateOvh {
        fn provider_type(&self) -> &str {
            "ovh"
        }
        fn config_schema(&self) -> &Schema {
            &self.schema
        }
        fn output_schema(&self) -> &Schema {
            &self.schema
        }
        fn resource_type(&self, _name: &str) -> Option<&dyn ResourceType> {
            None
        }
    }

    #[test]
    fn new_and_default_registries_are_empty() {
        assert!(ProviderRegistry::new().get("production").is_none());
        assert!(ProviderRegistry::default().get("production").is_none());
    }

    #[test]
    fn production_and_staging_can_both_use_ovh() {
        let mut registry = ProviderRegistry::new();
        registry
            .register("production", Box::new(OvhProvider::new()))
            .unwrap();
        registry
            .register(String::from("staging"), Box::new(OvhProvider::new()))
            .unwrap();
        assert_eq!(registry.get("production").unwrap().provider_type(), "ovh");
        assert_eq!(registry.get("staging").unwrap().provider_type(), "ovh");
        // Provider type is not an implicit registration name.
        assert!(registry.get("ovh").is_none());
    }

    #[test]
    fn independent_implementations_of_the_same_type_can_coexist() {
        let mut registry = ProviderRegistry::new();
        registry
            .register("production", Box::new(OvhProvider::new()))
            .unwrap();
        registry
            .register("staging", Box::new(AlternateOvh::default()))
            .unwrap();
        let production = registry.get("production").unwrap();
        let staging = registry.get("staging").unwrap();
        assert_eq!(production.provider_type(), staging.provider_type());
        assert!(production.config_schema().fields.contains_key("endpoint"));
        assert!(staging.config_schema().fields.is_empty());
    }

    #[test]
    fn duplicate_names_are_rejected_without_replacing_the_first() {
        let mut registry = ProviderRegistry::new();
        registry
            .register("production", Box::new(OvhProvider::new()))
            .unwrap();
        let error = registry
            .register("production", Box::new(AlternateOvh::default()))
            .unwrap_err();
        assert_eq!(
            error,
            ProviderRegistrationError::DuplicateName {
                name: "production".into()
            }
        );
        assert_eq!(
            error.to_string(),
            "provider name `production` is already registered"
        );
        assert!(
            registry
                .get("production")
                .unwrap()
                .config_schema()
                .fields
                .contains_key("endpoint")
        );
    }

    #[test]
    fn missing_name_returns_none_without_affecting_registered_providers() {
        let mut registry = ProviderRegistry::new();
        registry
            .register("production", Box::new(OvhProvider::new()))
            .unwrap();
        assert!(registry.get("missing").is_none());
        assert!(registry.get("Production").is_none());
        assert!(registry.get("production").is_some());
    }
}
