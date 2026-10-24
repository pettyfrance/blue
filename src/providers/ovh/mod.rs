use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::provider::{FieldSchema, Provider, ResourceType, Schema, ValueType};

/// OVH provider definition, not a configured client. Stores schemas only;
/// credentials and discovered account values belong to future runtime support.
#[derive(Debug)]
pub struct OvhProvider {
    config_schema: Schema,
    output_schema: Schema,
}

impl OvhProvider {
    pub fn new() -> Self {
        let config_schema = Schema {
            fields: BTreeMap::from([
                ("endpoint".into(), string_field(false)),
                ("application_key".into(), string_field(true)),
                ("application_secret".into(), string_field(true)),
                ("consumer_key".into(), string_field(true)),
            ]),
        };
        let output_schema = Schema {
            fields: BTreeMap::from([("account".into(), string_field(false))]),
        };
        Self {
            config_schema,
            output_schema,
        }
    }

    /// Validate a concrete endpoint once its configuration expression has been
    /// evaluated. Allowed endpoint names are OVH policy, not generic schema rules.
    /// This performs no authentication or network access.
    pub fn validate_endpoint(&self, endpoint: &str) -> Result<(), OvhConfigError> {
        match endpoint {
            "ovh-eu" | "ovh-ca" | "ovh-us" => Ok(()),
            _ => Err(OvhConfigError::InvalidEndpoint),
        }
    }
}

impl Default for OvhProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for OvhProvider {
    fn provider_type(&self) -> &str {
        "ovh"
    }

    fn config_schema(&self) -> &Schema {
        &self.config_schema
    }

    fn output_schema(&self) -> &Schema {
        &self.output_schema
    }

    fn resource_type(&self, _name: &str) -> Option<&dyn ResourceType> {
        None
    }
}

fn string_field(sensitive: bool) -> FieldSchema {
    FieldSchema {
        ty: ValueType::String,
        required: true,
        sensitive,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OvhConfigError {
    InvalidEndpoint,
}

impl fmt::Display for OvhConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEndpoint => {
                f.write_str("invalid OVH endpoint; expected ovh-eu, ovh-ca, or ovh-us")
            }
        }
    }
}

impl Error for OvhConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_ovh_definition_through_generic_provider_interface() {
        let ovh = OvhProvider::new();
        let provider: &dyn Provider = &ovh;
        assert_eq!(provider.provider_type(), "ovh");
        assert!(std::ptr::eq(provider.config_schema(), &ovh.config_schema));
        assert!(std::ptr::eq(provider.output_schema(), &ovh.output_schema));
        assert_eq!(
            OvhProvider::default().config_schema(),
            provider.config_schema()
        );
    }

    #[test]
    fn configuration_has_only_required_endpoint_and_sensitive_credentials() {
        let ovh = OvhProvider::new();
        let fields = &ovh.config_schema().fields;
        assert_eq!(fields.len(), 4);
        for name in [
            "endpoint",
            "application_key",
            "application_secret",
            "consumer_key",
        ] {
            assert_eq!(fields[name].ty, ValueType::String);
            assert!(fields[name].required);
            assert_eq!(fields[name].sensitive, name != "endpoint");
        }
        assert!(!fields.contains_key("project_id"));
        assert!(!fields.contains_key("client_id"));
        assert!(!fields.contains_key("client_secret"));
    }

    #[test]
    fn account_is_an_explicit_non_sensitive_output_not_config() {
        let ovh = OvhProvider::new();
        assert_eq!(
            ovh.output_schema().fields,
            BTreeMap::from([(
                "account".into(),
                FieldSchema {
                    ty: ValueType::String,
                    required: true,
                    sensitive: false,
                }
            )])
        );
        assert!(!ovh.config_schema().fields.contains_key("account"));
        assert!(!ovh.output_schema().fields.contains_key("account_id"));
    }

    #[test]
    fn accepts_only_the_three_supported_endpoint_names() {
        let ovh = OvhProvider::new();
        for endpoint in ["ovh-eu", "ovh-ca", "ovh-us"] {
            assert_eq!(ovh.validate_endpoint(endpoint), Ok(()));
        }
        for endpoint in [
            "",
            "OVH-EU",
            "ovh-eu ",
            "eu",
            "https://eu.api.ovh.com/1.0",
            "kimsufi-eu",
            "soyoustart-eu",
            "arbitrary-value",
        ] {
            assert_eq!(
                ovh.validate_endpoint(endpoint),
                Err(OvhConfigError::InvalidEndpoint)
            );
        }
    }

    #[test]
    fn endpoint_error_lists_choices_without_echoing_the_supplied_value() {
        let error = OvhProvider::new()
            .validate_endpoint("accidental-secret")
            .unwrap_err();
        let message = error.to_string();
        for endpoint in ["ovh-eu", "ovh-ca", "ovh-us"] {
            assert!(message.contains(endpoint));
        }
        assert!(!message.contains("accidental-secret"));
    }

    #[test]
    fn does_not_advertise_resources_before_their_implementation() {
        let ovh = OvhProvider::new();
        let provider: &dyn Provider = &ovh;
        for name in ["instance", "ovh.instance", "", "missing"] {
            assert!(provider.resource_type(name).is_none());
        }
    }
}
