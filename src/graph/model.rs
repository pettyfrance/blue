use std::fmt;

use crate::config::{
    Attributes, Parameter, ParsedData, ParsedProviderConfig, ParsedResource, SourceSpan,
};

/// A declaration's identity, independent of provider selection or remote identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeclarationId {
    Provider { name: String },
    Parameter { name: String },
    Resource { resource_type: String, name: String },
    Data { data_type: String, name: String },
}

impl fmt::Display for DeclarationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider { name } => write!(f, "provider.{name}"),
            Self::Parameter { name } => write!(f, "parameter.{name}"),
            Self::Resource {
                resource_type,
                name,
            } => write!(f, "resource.{resource_type}.{name}"),
            Self::Data { data_type, name } => write!(f, "data.{data_type}.{name}"),
        }
    }
}

/// A borrowed parsed block. Attributes, expressions, and source spans remain in
/// the original parsed configuration rather than being copied into a new model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Declaration<'a> {
    Provider(&'a ParsedProviderConfig),
    Parameter(&'a Parameter),
    Resource(&'a ParsedResource),
    Data(&'a ParsedData),
}

impl<'a> Declaration<'a> {
    pub fn id(&self) -> DeclarationId {
        match self {
            Self::Provider(block) => DeclarationId::Provider {
                name: block.name.value.clone(),
            },
            Self::Parameter(block) => DeclarationId::Parameter {
                name: block.name.value.clone(),
            },
            Self::Resource(block) => DeclarationId::Resource {
                resource_type: block.resource_type.value.clone(),
                name: block.name.value.clone(),
            },
            Self::Data(block) => DeclarationId::Data {
                data_type: block.data_type.value.clone(),
                name: block.name.value.clone(),
            },
        }
    }

    pub fn attributes(&self) -> &'a Attributes {
        match self {
            Self::Provider(block) => &block.attributes,
            Self::Parameter(block) => &block.attributes,
            Self::Resource(block) => &block.attributes,
            Self::Data(block) => &block.attributes,
        }
    }

    pub fn span(&self) -> SourceSpan {
        match self {
            Self::Provider(block) => block.span,
            Self::Parameter(block) => block.span,
            Self::Resource(block) => block.span,
            Self::Data(block) => block.span,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use super::*;
    use crate::config::{ConfigReader, SourceId, Spanned, TomlReader};

    #[test]
    fn ids_format_and_distinguish_declaration_kinds_and_local_types() {
        let cases = [
            (
                DeclarationId::Provider { name: "app".into() },
                "provider.app",
            ),
            (
                DeclarationId::Parameter { name: "app".into() },
                "parameter.app",
            ),
            (
                DeclarationId::Resource {
                    resource_type: "server".into(),
                    name: "app".into(),
                },
                "resource.server.app",
            ),
            (
                DeclarationId::Resource {
                    resource_type: "database".into(),
                    name: "app".into(),
                },
                "resource.database.app",
            ),
            (
                DeclarationId::Data {
                    data_type: "server".into(),
                    name: "app".into(),
                },
                "data.server.app",
            ),
        ];
        for (id, text) in &cases {
            assert_eq!(id.to_string(), *text);
        }
        let ids = cases.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
        assert_eq!(
            ids.iter().cloned().collect::<BTreeSet<_>>().len(),
            cases.len()
        );
        assert_eq!(ids.into_iter().collect::<HashSet<_>>().len(), cases.len());
    }

    #[test]
    fn declarations_borrow_original_blocks_attributes_and_spans() {
        let parsed = TomlReader
            .read(
                SourceId(17),
                r#"
[provider.cloud]
type = "example"
region = "eu"
[parameter.environment]
default = "prod"
[resource.server.app]
provider = "cloud"
name = "app-{{ parameter.environment }}"
[data.info.main]
provider = "cloud"
lookup = "app"
"#,
            )
            .unwrap();
        let cases = [
            (
                Declaration::Provider(&parsed.providers[0]),
                "provider.cloud",
                &parsed.providers[0].attributes,
                parsed.providers[0].span,
            ),
            (
                Declaration::Parameter(&parsed.parameters[0]),
                "parameter.environment",
                &parsed.parameters[0].attributes,
                parsed.parameters[0].span,
            ),
            (
                Declaration::Resource(&parsed.resources[0]),
                "resource.server.app",
                &parsed.resources[0].attributes,
                parsed.resources[0].span,
            ),
            (
                Declaration::Data(&parsed.data[0]),
                "data.info.main",
                &parsed.data[0].attributes,
                parsed.data[0].span,
            ),
        ];
        for (declaration, id, attributes, span) in cases {
            assert_eq!(declaration.id().to_string(), id);
            assert!(std::ptr::eq(declaration.attributes(), attributes));
            assert_eq!(declaration.span(), span);
            assert_eq!(declaration.span().source, SourceId(17));
        }
        let Declaration::Resource(block) = Declaration::Resource(&parsed.resources[0]) else {
            unreachable!()
        };
        assert!(std::ptr::eq(block, &parsed.resources[0]));
    }

    #[test]
    fn resource_and_data_identity_exclude_provider_selection() {
        let parsed = TomlReader
            .read(
                SourceId(0),
                r#"
[resource.server.app]
provider = "first"
[data.info.main]
provider = "first"
"#,
            )
            .unwrap();
        let mut resource = parsed.resources[0].clone();
        let mut data = parsed.data[0].clone();
        resource.provider = Some(Spanned::synthetic("second".into()));
        data.provider = None;
        assert_eq!(
            Declaration::Resource(&resource).id(),
            Declaration::Resource(&parsed.resources[0]).id()
        );
        assert_eq!(
            Declaration::Data(&data).id(),
            Declaration::Data(&parsed.data[0]).id()
        );
    }

    #[test]
    fn attribute_borrow_outlives_temporary_declaration_wrapper() {
        let parsed = TomlReader
            .read(SourceId(0), "[parameter.environment]\ndefault = \"prod\"\n")
            .unwrap();
        let attributes = Declaration::Parameter(&parsed.parameters[0]).attributes();
        assert!(std::ptr::eq(attributes, &parsed.parameters[0].attributes));
        assert!(attributes.contains_key("default"));
    }
}
