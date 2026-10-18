use std::collections::BTreeMap;
use std::error::Error;

use async_trait::async_trait;
use std::fmt;
use std::sync::Arc;

use crate::config::{Config, ProviderConfig};

use super::identity::ProviderIdentity;
use super::operation::{ProviderOperationError, ResourceDiff};
use super::request::{
    CreateResourceRequest, DeleteResourceRequest, DiffResourceRequest, ReadDataRequest,
    ReadResourceRequest, UpdateResourceRequest,
};
use super::schema::Schema;
use super::state::{DataState, ResourceState};

pub trait Provider: Send + Sync {
    fn provider_type(&self) -> &str;

    fn resource_type(&self, resource_type: &str) -> Option<&dyn ResourceType>;

    fn data_type(&self, data_type: &str) -> Option<&dyn DataType>;

    fn configure(
        &self,
        config: &ProviderConfig,
    ) -> Result<Box<dyn ProviderInstance>, ProviderError>;
}

#[async_trait]
pub trait ResourceType: Send + Sync {
    fn name(&self) -> &str;

    fn schema(&self) -> &Schema;

    async fn diff(
        &self,
        _request: DiffResourceRequest,
    ) -> Result<ResourceDiff, ProviderOperationError> {
        Err(ProviderOperationError::unsupported("resource.diff"))
    }

    async fn create(
        &self,
        _request: CreateResourceRequest,
    ) -> Result<ResourceState, ProviderOperationError> {
        Err(ProviderOperationError::unsupported("resource.create"))
    }

    async fn read(
        &self,
        _request: ReadResourceRequest,
    ) -> Result<Option<ResourceState>, ProviderOperationError> {
        Err(ProviderOperationError::unsupported("resource.read"))
    }

    async fn update(
        &self,
        _request: UpdateResourceRequest,
    ) -> Result<ResourceState, ProviderOperationError> {
        Err(ProviderOperationError::unsupported("resource.update"))
    }

    async fn delete(&self, _request: DeleteResourceRequest) -> Result<(), ProviderOperationError> {
        Err(ProviderOperationError::unsupported("resource.delete"))
    }
}

#[async_trait]
pub trait DataType: Send + Sync {
    fn name(&self) -> &str;

    fn schema(&self) -> &Schema;

    async fn read(&self, _request: ReadDataRequest) -> Result<DataState, ProviderOperationError> {
        Err(ProviderOperationError::unsupported("data.read"))
    }
}

pub trait ProviderInstance: Send + Sync {
    fn provider_name(&self) -> &str;
    fn provider_type(&self) -> &str;
    fn identity(&self) -> Result<ProviderIdentity, ProviderError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    DuplicateProviderType {
        provider_type: String,
    },
    UnknownProviderType {
        provider_type: String,
    },
    ConfigureFailed {
        provider_name: String,
        provider_type: String,
        message: String,
    },
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateProviderType { provider_type } => {
                write!(f, "duplicate provider type `{provider_type}`")
            }
            Self::UnknownProviderType { provider_type } => {
                write!(f, "unknown provider type `{provider_type}`")
            }
            Self::ConfigureFailed {
                provider_name,
                provider_type,
                message,
            } => write!(
                f,
                "failed to configure provider `{provider_name}` of type `{provider_type}`: {message}"
            ),
        }
    }
}

impl Error for ProviderError {}

#[derive(Default)]
pub struct ProviderRegistry {
    providers: BTreeMap<String, Arc<dyn Provider>>,
}

impl fmt::Debug for ProviderRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderRegistry")
            .field("provider_types", &self.providers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: Arc<dyn Provider>) -> Result<(), ProviderError> {
        let provider_type = provider.provider_type().to_string();

        if self.providers.contains_key(&provider_type) {
            return Err(ProviderError::DuplicateProviderType { provider_type });
        }

        self.providers.insert(provider_type, provider);
        Ok(())
    }

    pub fn get(&self, provider_type: &str) -> Option<Arc<dyn Provider>> {
        self.providers.get(provider_type).cloned()
    }

    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    pub fn configure(
        &self,
        config: &ProviderConfig,
    ) -> Result<Box<dyn ProviderInstance>, ProviderError> {
        let provider_type = &config.provider_type.value;
        let provider = self.providers.get(provider_type).ok_or_else(|| {
            ProviderError::UnknownProviderType {
                provider_type: provider_type.clone(),
            }
        })?;

        provider.configure(config)
    }
}

#[derive(Default)]
pub struct ConfiguredProviders {
    instances: BTreeMap<String, Box<dyn ProviderInstance>>,
}

impl fmt::Debug for ConfiguredProviders {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfiguredProviders")
            .field("provider_names", &self.instances.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ConfiguredProviders {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn configure_all(
        registry: &ProviderRegistry,
        config: &Config,
    ) -> Result<Self, Vec<ProviderError>> {
        let mut configured = Self::new();
        let mut errors = Vec::new();

        for provider_config in &config.providers {
            match registry.configure(provider_config) {
                Ok(instance) => {
                    configured
                        .instances
                        .insert(provider_config.name.value.clone(), instance);
                }
                Err(error) => errors.push(error),
            }
        }

        if errors.is_empty() {
            Ok(configured)
        } else {
            Err(errors)
        }
    }

    pub fn insert(
        &mut self,
        provider_name: impl Into<String>,
        instance: Box<dyn ProviderInstance>,
    ) -> Option<Box<dyn ProviderInstance>> {
        self.instances.insert(provider_name.into(), instance)
    }

    pub fn get(&self, provider_name: &str) -> Option<&dyn ProviderInstance> {
        self.instances
            .get(provider_name)
            .map(|instance| instance.as_ref())
    }

    pub fn contains(&self, provider_name: &str) -> bool {
        self.instances.contains_key(provider_name)
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProviderConfig, SourceSpan, Spanned};

    #[derive(Debug)]
    struct TestProvider {
        provider_type: String,
    }

    impl TestProvider {
        fn new(provider_type: &str) -> Self {
            Self {
                provider_type: provider_type.into(),
            }
        }
    }

    impl Provider for TestProvider {
        fn provider_type(&self) -> &str {
            &self.provider_type
        }

        fn resource_type(&self, _resource_type: &str) -> Option<&dyn ResourceType> {
            None
        }

        fn data_type(&self, _data_type: &str) -> Option<&dyn DataType> {
            None
        }

        fn configure(
            &self,
            config: &ProviderConfig,
        ) -> Result<Box<dyn ProviderInstance>, ProviderError> {
            Ok(Box::new(TestProviderInstance {
                provider_name: config.name.value.clone(),
                provider_type: self.provider_type.clone(),
                account_id: config
                    .attributes
                    .get("account_id")
                    .and_then(|attribute| match &attribute.value.value {
                        crate::config::ExprKind::Literal(crate::config::Value::String(value)) => {
                            Some(value.clone())
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| "test-account".into()),
            }))
        }
    }

    #[derive(Debug)]
    struct TestProviderInstance {
        provider_name: String,
        provider_type: String,
        account_id: String,
    }

    impl ProviderInstance for TestProviderInstance {
        fn provider_name(&self) -> &str {
            &self.provider_name
        }

        fn provider_type(&self) -> &str {
            &self.provider_type
        }

        fn identity(&self) -> Result<ProviderIdentity, ProviderError> {
            Ok(ProviderIdentity::new(self.provider_type.clone())
                .with_attribute("account_id", self.account_id.clone()))
        }
    }

    fn provider_config(name: &str, provider_type: &str) -> ProviderConfig {
        ProviderConfig {
            name: Spanned::synthetic(name.into()),
            provider_type: Spanned::synthetic(provider_type.into()),
            source: None,
            default: None,
            attributes: BTreeMap::new(),
            span: SourceSpan::SYNTHETIC,
        }
    }

    fn config(provider_type: &str) -> Config {
        Config {
            providers: vec![provider_config("main", provider_type)],
            parameters: Vec::new(),
            resources: Vec::new(),
            data: Vec::new(),
        }
    }

    #[test]
    fn registers_and_gets_provider_by_type() {
        let mut registry = ProviderRegistry::new();

        registry
            .register(Arc::new(TestProvider::new("test")))
            .unwrap();

        assert_eq!(registry.provider_count(), 1);
        assert_eq!(registry.get("test").unwrap().provider_type(), "test");
    }

    #[test]
    fn duplicate_provider_types_error() {
        let mut registry = ProviderRegistry::new();

        registry
            .register(Arc::new(TestProvider::new("test")))
            .unwrap();
        let error = registry
            .register(Arc::new(TestProvider::new("test")))
            .unwrap_err();

        assert_eq!(
            error,
            ProviderError::DuplicateProviderType {
                provider_type: "test".into()
            }
        );
    }

    #[test]
    fn unknown_provider_type_errors() {
        let registry = ProviderRegistry::new();
        let error = match registry.configure(&provider_config("main", "missing")) {
            Ok(_) => panic!("expected unknown provider error"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            ProviderError::UnknownProviderType {
                provider_type: "missing".into()
            }
        );
    }

    #[test]
    fn configures_provider_from_semantic_config() {
        let mut registry = ProviderRegistry::new();
        registry
            .register(Arc::new(TestProvider::new("test")))
            .unwrap();

        let instance = registry
            .configure(&provider_config("main", "test"))
            .unwrap();

        assert_eq!(instance.provider_name(), "main");
        assert_eq!(instance.provider_type(), "test");
    }

    #[test]
    fn configures_all_providers_by_config_name() {
        let mut registry = ProviderRegistry::new();
        registry
            .register(Arc::new(TestProvider::new("test")))
            .unwrap();

        let configured = ConfiguredProviders::configure_all(&registry, &config("test")).unwrap();

        assert_eq!(configured.len(), 1);
        assert!(configured.contains("main"));
        assert_eq!(configured.get("main").unwrap().provider_type(), "test");
    }

    #[test]
    fn configuring_all_providers_collects_errors() {
        let registry = ProviderRegistry::new();

        let errors = ConfiguredProviders::configure_all(&registry, &config("missing")).unwrap_err();

        assert_eq!(
            errors,
            vec![ProviderError::UnknownProviderType {
                provider_type: "missing".into()
            }]
        );
    }

    #[test]
    fn provider_instance_exposes_identity() {
        let mut registry = ProviderRegistry::new();
        registry
            .register(Arc::new(TestProvider::new("test")))
            .unwrap();

        let instance = registry
            .configure(&provider_config("main", "test"))
            .unwrap();
        let identity = instance.identity().unwrap();

        assert_eq!(identity.provider_type, "test");
        assert_eq!(
            identity.attributes.get("account_id"),
            Some(&"test-account".to_string())
        );
    }
}
