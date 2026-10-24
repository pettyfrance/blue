use super::Schema;

/// Describes a provider's configuration and supported local resource types.
/// This is not a configured API client and performs no remote operations.
pub trait Provider {
    fn provider_type(&self) -> &str;

    fn config_schema(&self) -> &Schema;

    /// Explicitly published provider outputs, separate from configuration.
    fn output_schema(&self) -> &Schema;

    /// Look up a provider-local resource type, such as `instance`.
    fn resource_type(&self, name: &str) -> Option<&dyn ResourceType>;
}

/// Describes the separate input and output contracts of a resource type.
pub trait ResourceType {
    fn input_schema(&self) -> &Schema;

    fn output_schema(&self) -> &Schema;
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::provider::{FieldSchema, ValueType};

    struct TestResource {
        inputs: Schema,
        outputs: Schema,
    }

    impl ResourceType for TestResource {
        fn input_schema(&self) -> &Schema {
            &self.inputs
        }
        fn output_schema(&self) -> &Schema {
            &self.outputs
        }
    }

    struct TestProvider {
        config: Schema,
        outputs: Schema,
        instance: TestResource,
    }

    impl Provider for TestProvider {
        fn provider_type(&self) -> &str {
            "test"
        }
        fn config_schema(&self) -> &Schema {
            &self.config
        }
        fn output_schema(&self) -> &Schema {
            &self.outputs
        }
        fn resource_type(&self, name: &str) -> Option<&dyn ResourceType> {
            match name {
                "instance" => Some(&self.instance),
                _ => None,
            }
        }
    }

    fn schema(name: &str, sensitive: bool) -> Schema {
        Schema {
            fields: BTreeMap::from([(
                name.into(),
                FieldSchema {
                    ty: ValueType::String,
                    required: true,
                    sensitive,
                },
            )]),
        }
    }

    fn provider() -> TestProvider {
        TestProvider {
            config: schema("token", true),
            outputs: schema("account", false),
            instance: TestResource {
                inputs: schema("region", false),
                outputs: schema("id", false),
            },
        }
    }

    #[test]
    fn trait_objects_expose_borrowed_provider_and_resource_schemas() {
        let concrete = provider();
        let provider: &dyn Provider = &concrete;
        assert_eq!(provider.provider_type(), "test");
        assert!(std::ptr::eq(provider.config_schema(), &concrete.config));
        assert!(std::ptr::eq(provider.output_schema(), &concrete.outputs));
        assert!(provider.output_schema().fields.contains_key("account"));
        let resource: &dyn ResourceType = provider.resource_type("instance").unwrap();
        assert!(std::ptr::eq(
            resource.input_schema(),
            &concrete.instance.inputs
        ));
        assert!(std::ptr::eq(
            resource.output_schema(),
            &concrete.instance.outputs
        ));
        assert!(provider.config_schema().fields["token"].sensitive);
        assert!(resource.input_schema().fields.contains_key("region"));
        assert!(resource.output_schema().fields.contains_key("id"));
    }

    #[test]
    fn resource_lookup_uses_local_names_and_rejects_unsupported_types() {
        let concrete = provider();
        let provider: &dyn Provider = &concrete;
        assert!(provider.resource_type("instance").is_some());
        assert!(provider.resource_type("missing").is_none());
        assert!(provider.resource_type("test.instance").is_none());
    }
}
