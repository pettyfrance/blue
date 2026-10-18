use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::config::{Attribute, Attributes, Config, Expr, ExprKind, Value};
use crate::graph::NodeId;

use super::ProviderRegistry;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Schema {
    pub attributes: BTreeMap<String, AttributeSchema>,
}

impl Schema {
    pub fn new(attributes: BTreeMap<String, AttributeSchema>) -> Self {
        Self { attributes }
    }

    pub fn empty() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeSchema {
    pub ty: SchemaType,
    pub required: bool,
    pub sensitive: bool,
}

impl AttributeSchema {
    pub fn required(ty: SchemaType) -> Self {
        Self {
            ty,
            required: true,
            sensitive: false,
        }
    }

    pub fn optional(ty: SchemaType) -> Self {
        Self {
            ty,
            required: false,
            sensitive: false,
        }
    }

    pub fn sensitive(mut self) -> Self {
        self.sensitive = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaType {
    String,
    Integer,
    Float,
    Bool,
    Array(Box<SchemaType>),
    Object(BTreeMap<String, AttributeSchema>),
    Any,
}

impl fmt::Display for SchemaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String => write!(f, "string"),
            Self::Integer => write!(f, "integer"),
            Self::Float => write!(f, "float"),
            Self::Bool => write!(f, "bool"),
            Self::Array(item) => write!(f, "array<{item}>"),
            Self::Object(_) => write!(f, "object"),
            Self::Any => write!(f, "any"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderSchemaError {
    UnknownProviderConfig {
        provider_name: String,
    },
    UnknownProviderType {
        provider_name: String,
        provider_type: String,
    },
    UnknownResourceType {
        provider_name: String,
        provider_type: String,
        resource_type: String,
    },
    UnknownDataType {
        provider_name: String,
        provider_type: String,
        data_type: String,
    },
    MissingRequiredAttribute {
        node: NodeId,
        attribute: String,
    },
    UnknownAttribute {
        node: NodeId,
        attribute: String,
    },
    TypeMismatch {
        node: NodeId,
        attribute: String,
        expected: SchemaType,
    },
}

impl fmt::Display for ProviderSchemaError {
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
            Self::UnknownResourceType {
                provider_name,
                provider_type,
                resource_type,
            } => write!(
                f,
                "provider `{provider_name}` of type `{provider_type}` does not support resource type `{resource_type}`"
            ),
            Self::UnknownDataType {
                provider_name,
                provider_type,
                data_type,
            } => write!(
                f,
                "provider `{provider_name}` of type `{provider_type}` does not support data type `{data_type}`"
            ),
            Self::MissingRequiredAttribute { node, attribute } => {
                write!(f, "missing required attribute `{attribute}` on `{node}`")
            }
            Self::UnknownAttribute { node, attribute } => {
                write!(f, "unknown attribute `{attribute}` on `{node}`")
            }
            Self::TypeMismatch {
                node,
                attribute,
                expected,
            } => write!(f, "attribute `{attribute}` on `{node}` must be {expected}"),
        }
    }
}

impl Error for ProviderSchemaError {}

pub struct ProviderSchemaValidator;

impl ProviderSchemaValidator {
    pub fn validate(
        config: &Config,
        registry: &ProviderRegistry,
    ) -> Result<(), Vec<ProviderSchemaError>> {
        let provider_configs = config
            .providers
            .iter()
            .map(|provider| (provider.name.value.as_str(), provider))
            .collect::<BTreeMap<_, _>>();
        let mut errors = Vec::new();

        for resource in &config.resources {
            let provider_name = &resource.provider.value;
            let node = NodeId::Resource {
                resource_type: resource.resource_type.value.clone(),
                name: resource.name.value.clone(),
            };

            let Some(provider_config) = provider_configs.get(provider_name.as_str()) else {
                errors.push(ProviderSchemaError::UnknownProviderConfig {
                    provider_name: provider_name.clone(),
                });
                continue;
            };

            let provider_type = &provider_config.provider_type.value;
            let Some(provider) = registry.get(provider_type) else {
                errors.push(ProviderSchemaError::UnknownProviderType {
                    provider_name: provider_name.clone(),
                    provider_type: provider_type.clone(),
                });
                continue;
            };

            let Some(resource_type) = provider.resource_type(&resource.resource_type.value) else {
                errors.push(ProviderSchemaError::UnknownResourceType {
                    provider_name: provider_name.clone(),
                    provider_type: provider_type.clone(),
                    resource_type: resource.resource_type.value.clone(),
                });
                continue;
            };

            validate_attributes(
                &node,
                &resource.attributes,
                resource_type.schema(),
                "",
                &mut errors,
            );
        }

        for data in &config.data {
            let provider_name = &data.provider.value;
            let node = NodeId::Data {
                data_type: data.data_type.value.clone(),
                name: data.name.value.clone(),
            };

            let Some(provider_config) = provider_configs.get(provider_name.as_str()) else {
                errors.push(ProviderSchemaError::UnknownProviderConfig {
                    provider_name: provider_name.clone(),
                });
                continue;
            };

            let provider_type = &provider_config.provider_type.value;
            let Some(provider) = registry.get(provider_type) else {
                errors.push(ProviderSchemaError::UnknownProviderType {
                    provider_name: provider_name.clone(),
                    provider_type: provider_type.clone(),
                });
                continue;
            };

            let Some(data_type) = provider.data_type(&data.data_type.value) else {
                errors.push(ProviderSchemaError::UnknownDataType {
                    provider_name: provider_name.clone(),
                    provider_type: provider_type.clone(),
                    data_type: data.data_type.value.clone(),
                });
                continue;
            };

            validate_attributes(&node, &data.attributes, data_type.schema(), "", &mut errors);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn validate_attributes(
    node: &NodeId,
    attributes: &Attributes,
    schema: &Schema,
    prefix: &str,
    errors: &mut Vec<ProviderSchemaError>,
) {
    for (name, attribute_schema) in &schema.attributes {
        if attribute_schema.required && !attributes.contains_key(name) {
            errors.push(ProviderSchemaError::MissingRequiredAttribute {
                node: node.clone(),
                attribute: path(prefix, name),
            });
        }
    }

    for (name, attribute) in attributes {
        let attribute_path = path(prefix, name);
        let Some(attribute_schema) = schema.attributes.get(name) else {
            errors.push(ProviderSchemaError::UnknownAttribute {
                node: node.clone(),
                attribute: attribute_path,
            });
            continue;
        };

        validate_attribute(
            node,
            attribute,
            &attribute_schema.ty,
            &attribute_path,
            errors,
        );
    }
}

fn validate_attribute(
    node: &NodeId,
    attribute: &Attribute,
    expected: &SchemaType,
    attribute_path: &str,
    errors: &mut Vec<ProviderSchemaError>,
) {
    if !expr_matches_type(&attribute.value, expected, node, attribute_path, errors) {
        errors.push(ProviderSchemaError::TypeMismatch {
            node: node.clone(),
            attribute: attribute_path.to_string(),
            expected: expected.clone(),
        });
    }
}

fn expr_matches_type(
    expr: &Expr,
    expected: &SchemaType,
    node: &NodeId,
    attribute_path: &str,
    errors: &mut Vec<ProviderSchemaError>,
) -> bool {
    match &expr.value {
        ExprKind::Reference(_) | ExprKind::Template(_) => true,
        ExprKind::Literal(value) => {
            value_matches_type(value, expected, node, attribute_path, errors)
        }
    }
}

fn value_matches_type(
    value: &Value,
    expected: &SchemaType,
    node: &NodeId,
    attribute_path: &str,
    errors: &mut Vec<ProviderSchemaError>,
) -> bool {
    match expected {
        SchemaType::Any => true,
        SchemaType::String => matches!(value, Value::String(_)),
        SchemaType::Integer => matches!(value, Value::Integer(_)),
        SchemaType::Float => matches!(value, Value::Float(_) | Value::Integer(_)),
        SchemaType::Bool => matches!(value, Value::Bool(_)),
        SchemaType::Array(item_type) => {
            let Value::Array(items) = value else {
                return false;
            };

            for (index, item) in items.iter().enumerate() {
                let item_path = format!("{attribute_path}[{index}]");
                if !expr_matches_type(item, item_type, node, &item_path, errors) {
                    errors.push(ProviderSchemaError::TypeMismatch {
                        node: node.clone(),
                        attribute: item_path,
                        expected: (**item_type).clone(),
                    });
                }
            }

            true
        }
        SchemaType::Object(fields) => {
            let Value::Object(attributes) = value else {
                return false;
            };

            validate_attributes(
                node,
                attributes,
                &Schema::new(fields.clone()),
                attribute_path,
                errors,
            );
            true
        }
    }
}

fn path(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::{
        Attribute, Data, ExprKind, ProviderConfig, Resource, SourceSpan, Spanned, Value,
    };
    use crate::provider::{
        DataType, Provider, ProviderError, ProviderIdentity, ProviderInstance, ResourceType,
    };

    struct SchemaProvider {
        provider_type: String,
        resources: BTreeMap<String, SchemaResourceType>,
        data: BTreeMap<String, SchemaDataType>,
    }

    struct SchemaResourceType {
        name: String,
        schema: Schema,
    }

    impl ResourceType for SchemaResourceType {
        fn name(&self) -> &str {
            &self.name
        }

        fn schema(&self) -> &Schema {
            &self.schema
        }
    }

    struct SchemaDataType {
        name: String,
        schema: Schema,
    }

    impl DataType for SchemaDataType {
        fn name(&self) -> &str {
            &self.name
        }

        fn schema(&self) -> &Schema {
            &self.schema
        }
    }

    impl SchemaProvider {
        fn new() -> Self {
            let mut dns_record_attrs = BTreeMap::new();
            dns_record_attrs.insert("zone".into(), AttributeSchema::required(SchemaType::String));
            dns_record_attrs.insert("ttl".into(), AttributeSchema::optional(SchemaType::Integer));
            dns_record_attrs.insert(
                "proxied".into(),
                AttributeSchema::optional(SchemaType::Bool),
            );
            dns_record_attrs.insert(
                "tags".into(),
                AttributeSchema::optional(SchemaType::Array(Box::new(SchemaType::String))),
            );

            let mut metadata_fields = BTreeMap::new();
            metadata_fields.insert(
                "owner".into(),
                AttributeSchema::required(SchemaType::String),
            );
            dns_record_attrs.insert(
                "metadata".into(),
                AttributeSchema::optional(SchemaType::Object(metadata_fields)),
            );

            let mut data_attrs = BTreeMap::new();
            data_attrs.insert("name".into(), AttributeSchema::required(SchemaType::String));

            Self {
                provider_type: "ovh".into(),
                resources: BTreeMap::from([(
                    "dns_record".into(),
                    SchemaResourceType {
                        name: "dns_record".into(),
                        schema: Schema::new(dns_record_attrs),
                    },
                )]),
                data: BTreeMap::from([(
                    "domain_zone".into(),
                    SchemaDataType {
                        name: "domain_zone".into(),
                        schema: Schema::new(data_attrs),
                    },
                )]),
            }
        }
    }

    impl Provider for SchemaProvider {
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
            Ok(Box::new(SchemaProviderInstance {
                provider_name: config.name.value.clone(),
                provider_type: self.provider_type.clone(),
            }))
        }
    }

    struct SchemaProviderInstance {
        provider_name: String,
        provider_type: String,
    }

    impl ProviderInstance for SchemaProviderInstance {
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

    fn expr(value: Value) -> crate::config::Expr {
        Spanned::synthetic(ExprKind::Literal(value))
    }

    fn attr(name: &str, value: Value) -> Attribute {
        Attribute {
            name: spanned(name),
            value: expr(value),
            span: SourceSpan::SYNTHETIC,
        }
    }

    fn attrs(values: Vec<(&str, Value)>) -> Attributes {
        values
            .into_iter()
            .map(|(name, value)| (name.to_string(), attr(name, value)))
            .collect()
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

    fn resource(resource_type: &str, attributes: Attributes) -> Resource {
        Resource {
            provider: spanned("ovh_prod"),
            resource_type: spanned(resource_type),
            name: spanned("www"),
            attributes,
            span: SourceSpan::SYNTHETIC,
        }
    }

    fn data(data_type: &str, attributes: Attributes) -> Data {
        Data {
            provider: spanned("ovh_prod"),
            data_type: spanned(data_type),
            name: spanned("main"),
            attributes,
            span: SourceSpan::SYNTHETIC,
        }
    }

    fn config() -> Config {
        Config {
            providers: vec![provider_config("ovh_prod", "ovh")],
            parameters: Vec::new(),
            resources: Vec::new(),
            data: Vec::new(),
        }
    }

    fn registry() -> ProviderRegistry {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(SchemaProvider::new())).unwrap();
        registry
    }

    #[test]
    fn valid_resource_attributes_pass() {
        let mut config = config();
        config.resources.push(resource(
            "dns_record",
            attrs(vec![
                ("zone", Value::String("example.com".into())),
                ("ttl", Value::Integer(60)),
                ("proxied", Value::Bool(false)),
                (
                    "tags",
                    Value::Array(vec![expr(Value::String("prod".into()))]),
                ),
            ]),
        ));

        assert!(ProviderSchemaValidator::validate(&config, &registry()).is_ok());
    }

    #[test]
    fn valid_data_attributes_pass() {
        let mut config = config();
        config.data.push(data(
            "domain_zone",
            attrs(vec![("name", Value::String("example.com".into()))]),
        ));

        assert!(ProviderSchemaValidator::validate(&config, &registry()).is_ok());
    }

    #[test]
    fn missing_required_attribute_errors() {
        let mut config = config();
        config
            .resources
            .push(resource("dns_record", BTreeMap::new()));

        let errors = ProviderSchemaValidator::validate(&config, &registry()).unwrap_err();

        assert_eq!(
            errors,
            vec![ProviderSchemaError::MissingRequiredAttribute {
                node: NodeId::Resource {
                    resource_type: "dns_record".into(),
                    name: "www".into()
                },
                attribute: "zone".into(),
            }]
        );
    }

    #[test]
    fn unknown_attribute_errors() {
        let mut config = config();
        config.resources.push(resource(
            "dns_record",
            attrs(vec![
                ("zone", Value::String("example.com".into())),
                ("extra", Value::String("bad".into())),
            ]),
        ));

        let errors = ProviderSchemaValidator::validate(&config, &registry()).unwrap_err();

        assert_eq!(
            errors,
            vec![ProviderSchemaError::UnknownAttribute {
                node: NodeId::Resource {
                    resource_type: "dns_record".into(),
                    name: "www".into()
                },
                attribute: "extra".into(),
            }]
        );
    }

    #[test]
    fn type_mismatch_errors() {
        let mut config = config();
        config.resources.push(resource(
            "dns_record",
            attrs(vec![("zone", Value::Integer(1))]),
        ));

        let errors = ProviderSchemaValidator::validate(&config, &registry()).unwrap_err();

        assert_eq!(
            errors,
            vec![ProviderSchemaError::TypeMismatch {
                node: NodeId::Resource {
                    resource_type: "dns_record".into(),
                    name: "www".into()
                },
                attribute: "zone".into(),
                expected: SchemaType::String,
            }]
        );
    }

    #[test]
    fn nested_object_attributes_are_validated() {
        let mut config = config();
        config.resources.push(resource(
            "dns_record",
            attrs(vec![
                ("zone", Value::String("example.com".into())),
                ("metadata", Value::Object(BTreeMap::new())),
            ]),
        ));

        let errors = ProviderSchemaValidator::validate(&config, &registry()).unwrap_err();

        assert_eq!(
            errors,
            vec![ProviderSchemaError::MissingRequiredAttribute {
                node: NodeId::Resource {
                    resource_type: "dns_record".into(),
                    name: "www".into()
                },
                attribute: "metadata.owner".into(),
            }]
        );
    }

    #[test]
    fn array_items_are_validated() {
        let mut config = config();
        config.resources.push(resource(
            "dns_record",
            attrs(vec![
                ("zone", Value::String("example.com".into())),
                ("tags", Value::Array(vec![expr(Value::Integer(1))])),
            ]),
        ));

        let errors = ProviderSchemaValidator::validate(&config, &registry()).unwrap_err();

        assert_eq!(
            errors,
            vec![ProviderSchemaError::TypeMismatch {
                node: NodeId::Resource {
                    resource_type: "dns_record".into(),
                    name: "www".into()
                },
                attribute: "tags[0]".into(),
                expected: SchemaType::String,
            }]
        );
    }

    #[test]
    fn unknown_provider_and_type_errors_are_reported() {
        let mut unknown_provider_config = config();
        unknown_provider_config.resources.push(Resource {
            provider: spanned("missing"),
            resource_type: spanned("dns_record"),
            name: spanned("www"),
            attributes: BTreeMap::new(),
            span: SourceSpan::SYNTHETIC,
        });

        let errors =
            ProviderSchemaValidator::validate(&unknown_provider_config, &registry()).unwrap_err();

        assert_eq!(
            errors,
            vec![ProviderSchemaError::UnknownProviderConfig {
                provider_name: "missing".into(),
            }]
        );

        let mut config = config();
        config.providers[0].provider_type = spanned("missing");
        config
            .resources
            .push(resource("dns_record", BTreeMap::new()));

        let errors = ProviderSchemaValidator::validate(&config, &registry()).unwrap_err();

        assert_eq!(
            errors,
            vec![ProviderSchemaError::UnknownProviderType {
                provider_name: "ovh_prod".into(),
                provider_type: "missing".into(),
            }]
        );
    }

    #[test]
    fn unknown_resource_and_data_type_errors_are_reported() {
        let mut config = config();
        config.resources.push(resource("unknown", BTreeMap::new()));
        config.data.push(data("unknown", BTreeMap::new()));

        let errors = ProviderSchemaValidator::validate(&config, &registry()).unwrap_err();

        assert_eq!(
            errors,
            vec![
                ProviderSchemaError::UnknownResourceType {
                    provider_name: "ovh_prod".into(),
                    provider_type: "ovh".into(),
                    resource_type: "unknown".into(),
                },
                ProviderSchemaError::UnknownDataType {
                    provider_name: "ovh_prod".into(),
                    provider_type: "ovh".into(),
                    data_type: "unknown".into(),
                },
            ]
        );
    }
}
