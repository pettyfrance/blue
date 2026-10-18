use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::config::Config;

use super::ProviderRegistry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderCapabilityError {
    UnknownProviderConfig {
        provider_name: String,
    },
    UnknownProviderType {
        provider_name: String,
        provider_type: String,
    },
    UnsupportedResourceType {
        provider_name: String,
        provider_type: String,
        resource_type: String,
    },
    UnsupportedDataType {
        provider_name: String,
        provider_type: String,
        data_type: String,
    },
}

impl fmt::Display for ProviderCapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProviderConfig { provider_name } => {
                write!(f, "unknown provider config `{provider_name}`")
            }
            Self::UnknownProviderType {
                provider_name,
                provider_type,
            } => write!(
                f,
                "provider `{provider_name}` uses unknown provider type `{provider_type}`"
            ),
            Self::UnsupportedResourceType {
                provider_name,
                provider_type,
                resource_type,
            } => write!(
                f,
                "provider `{provider_name}` of type `{provider_type}` does not support resource type `{resource_type}`"
            ),
            Self::UnsupportedDataType {
                provider_name,
                provider_type,
                data_type,
            } => write!(
                f,
                "provider `{provider_name}` of type `{provider_type}` does not support data type `{data_type}`"
            ),
        }
    }
}

impl Error for ProviderCapabilityError {}

pub struct ProviderCapabilityValidator;

impl ProviderCapabilityValidator {
    pub fn validate(
        config: &Config,
        registry: &ProviderRegistry,
    ) -> Result<(), Vec<ProviderCapabilityError>> {
        let provider_configs = config
            .providers
            .iter()
            .map(|provider| (provider.name.value.as_str(), provider))
            .collect::<BTreeMap<_, _>>();
        let mut errors = Vec::new();

        for resource in &config.resources {
            let provider_name = &resource.provider.value;
            let Some(provider_config) = provider_configs.get(provider_name.as_str()) else {
                errors.push(ProviderCapabilityError::UnknownProviderConfig {
                    provider_name: provider_name.clone(),
                });
                continue;
            };

            let provider_type = &provider_config.provider_type.value;
            let Some(provider) = registry.get(provider_type) else {
                errors.push(ProviderCapabilityError::UnknownProviderType {
                    provider_name: provider_name.clone(),
                    provider_type: provider_type.clone(),
                });
                continue;
            };

            if provider
                .resource_type(&resource.resource_type.value)
                .is_none()
            {
                errors.push(ProviderCapabilityError::UnsupportedResourceType {
                    provider_name: provider_name.clone(),
                    provider_type: provider_type.clone(),
                    resource_type: resource.resource_type.value.clone(),
                });
            }
        }

        for data in &config.data {
            let provider_name = &data.provider.value;
            let Some(provider_config) = provider_configs.get(provider_name.as_str()) else {
                errors.push(ProviderCapabilityError::UnknownProviderConfig {
                    provider_name: provider_name.clone(),
                });
                continue;
            };

            let provider_type = &provider_config.provider_type.value;
            let Some(provider) = registry.get(provider_type) else {
                errors.push(ProviderCapabilityError::UnknownProviderType {
                    provider_name: provider_name.clone(),
                    provider_type: provider_type.clone(),
                });
                continue;
            };

            if provider.data_type(&data.data_type.value).is_none() {
                errors.push(ProviderCapabilityError::UnsupportedDataType {
                    provider_name: provider_name.clone(),
                    provider_type: provider_type.clone(),
                    data_type: data.data_type.value.clone(),
                });
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::{Data, ProviderConfig, Resource, SourceSpan, Spanned};
    use crate::provider::{
        DataType, Provider, ProviderError, ProviderIdentity, ProviderInstance, ResourceType, Schema,
    };

    struct CapabilityProvider {
        provider_type: String,
        resources: BTreeMap<String, CapabilityResourceType>,
        data: BTreeMap<String, CapabilityDataType>,
    }

    struct CapabilityResourceType {
        name: String,
        schema: Schema,
    }

    impl ResourceType for CapabilityResourceType {
        fn name(&self) -> &str {
            &self.name
        }

        fn schema(&self) -> &Schema {
            &self.schema
        }
    }

    struct CapabilityDataType {
        name: String,
        schema: Schema,
    }

    impl DataType for CapabilityDataType {
        fn name(&self) -> &str {
            &self.name
        }

        fn schema(&self) -> &Schema {
            &self.schema
        }
    }

    impl CapabilityProvider {
        fn new(provider_type: &str, resources: &[&str], data: &[&str]) -> Self {
            Self {
                provider_type: provider_type.into(),
                resources: resources
                    .iter()
                    .map(|value| {
                        (
                            value.to_string(),
                            CapabilityResourceType {
                                name: value.to_string(),
                                schema: Schema::empty(),
                            },
                        )
                    })
                    .collect(),
                data: data
                    .iter()
                    .map(|value| {
                        (
                            value.to_string(),
                            CapabilityDataType {
                                name: value.to_string(),
                                schema: Schema::empty(),
                            },
                        )
                    })
                    .collect(),
            }
        }
    }

    impl Provider for CapabilityProvider {
        fn provider_type(&self) -> &str {
            &self.provider_type
        }

        fn resource_type(&self, resource_type: &str) -> Option<&dyn ResourceType> {
            self.resources
                .get(resource_type)
                .map(|resource_type| resource_type as &dyn ResourceType)
        }

        fn data_type(&self, data_type: &str) -> Option<&dyn DataType> {
            self.data
                .get(data_type)
                .map(|data_type| data_type as &dyn DataType)
        }

        fn configure(
            &self,
            config: &ProviderConfig,
        ) -> Result<Box<dyn ProviderInstance>, ProviderError> {
            Ok(Box::new(CapabilityProviderInstance {
                provider_name: config.name.value.clone(),
                provider_type: self.provider_type.clone(),
            }))
        }
    }

    struct CapabilityProviderInstance {
        provider_name: String,
        provider_type: String,
    }

    impl ProviderInstance for CapabilityProviderInstance {
        fn provider_name(&self) -> &str {
            &self.provider_name
        }

        fn provider_type(&self) -> &str {
            &self.provider_type
        }

        fn identity(&self) -> Result<ProviderIdentity, ProviderError> {
            Ok(ProviderIdentity::new(self.provider_type.clone()))
        }
    }

    fn spanned(value: &str) -> Spanned<String> {
        Spanned::synthetic(value.to_string())
    }

    fn provider_config(name: &str, provider_type: &str) -> ProviderConfig {
        ProviderConfig {
            name: spanned(name),
            provider_type: spanned(provider_type),
            source: None,
            default: None,
            attributes: BTreeMap::new(),
            span: SourceSpan::SYNTHETIC,
        }
    }

    fn resource(provider: &str, resource_type: &str, name: &str) -> Resource {
        Resource {
            provider: spanned(provider),
            resource_type: spanned(resource_type),
            name: spanned(name),
            attributes: BTreeMap::new(),
            span: SourceSpan::SYNTHETIC,
        }
    }

    fn data(provider: &str, data_type: &str, name: &str) -> Data {
        Data {
            provider: spanned(provider),
            data_type: spanned(data_type),
            name: spanned(name),
            attributes: BTreeMap::new(),
            span: SourceSpan::SYNTHETIC,
        }
    }

    fn registry() -> ProviderRegistry {
        let mut registry = ProviderRegistry::new();
        registry
            .register(Arc::new(CapabilityProvider::new(
                "ovh",
                &["dns_record"],
                &["domain_zone"],
            )))
            .unwrap();
        registry
    }

    fn config() -> Config {
        Config {
            providers: vec![provider_config("ovh_prod", "ovh")],
            parameters: Vec::new(),
            resources: Vec::new(),
            data: Vec::new(),
        }
    }

    #[test]
    fn supported_resource_passes() {
        let mut config = config();
        config
            .resources
            .push(resource("ovh_prod", "dns_record", "www"));

        assert!(ProviderCapabilityValidator::validate(&config, &registry()).is_ok());
    }

    #[test]
    fn unsupported_resource_errors() {
        let mut config = config();
        config
            .resources
            .push(resource("ovh_prod", "domain_zone", "main"));

        let errors = ProviderCapabilityValidator::validate(&config, &registry()).unwrap_err();

        assert_eq!(
            errors,
            vec![ProviderCapabilityError::UnsupportedResourceType {
                provider_name: "ovh_prod".into(),
                provider_type: "ovh".into(),
                resource_type: "domain_zone".into(),
            }]
        );
    }

    #[test]
    fn supported_data_passes() {
        let mut config = config();
        config.data.push(data("ovh_prod", "domain_zone", "main"));

        assert!(ProviderCapabilityValidator::validate(&config, &registry()).is_ok());
    }

    #[test]
    fn unsupported_data_errors() {
        let mut config = config();
        config.data.push(data("ovh_prod", "dns_record", "www"));

        let errors = ProviderCapabilityValidator::validate(&config, &registry()).unwrap_err();

        assert_eq!(
            errors,
            vec![ProviderCapabilityError::UnsupportedDataType {
                provider_name: "ovh_prod".into(),
                provider_type: "ovh".into(),
                data_type: "dns_record".into(),
            }]
        );
    }

    #[test]
    fn unknown_provider_config_errors() {
        let mut config = config();
        config
            .resources
            .push(resource("missing", "dns_record", "www"));

        let errors = ProviderCapabilityValidator::validate(&config, &registry()).unwrap_err();

        assert_eq!(
            errors,
            vec![ProviderCapabilityError::UnknownProviderConfig {
                provider_name: "missing".into(),
            }]
        );
    }

    #[test]
    fn unknown_provider_type_errors() {
        let mut config = config();
        config.providers[0].provider_type = spanned("missing");
        config
            .resources
            .push(resource("ovh_prod", "dns_record", "www"));

        let errors = ProviderCapabilityValidator::validate(&config, &registry()).unwrap_err();

        assert_eq!(
            errors,
            vec![ProviderCapabilityError::UnknownProviderType {
                provider_name: "ovh_prod".into(),
                provider_type: "missing".into(),
            }]
        );
    }

    #[test]
    fn validates_provider_local_type_names() {
        let mut config = config();
        config
            .resources
            .push(resource("ovh_prod", "ovh_dns_record", "www"));

        let errors = ProviderCapabilityValidator::validate(&config, &registry()).unwrap_err();

        assert_eq!(
            errors,
            vec![ProviderCapabilityError::UnsupportedResourceType {
                provider_name: "ovh_prod".into(),
                provider_type: "ovh".into(),
                resource_type: "ovh_dns_record".into(),
            }]
        );
    }
}
