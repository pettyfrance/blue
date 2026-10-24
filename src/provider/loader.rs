use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::config::{ParsedProviderConfig, SourceSpan};
use crate::providers::ovh::OvhProvider;

use super::{Provider, ProviderRegistry};

/// Built-in discovery only. The registry itself has no knowledge of built-ins,
/// sources, or configuration parsing. Custom source loading can be added later.
pub struct BuiltinProviderLoader;

impl BuiltinProviderLoader {
    /// Load definitions, not configured clients. Only names, types, and source
    /// metadata are inspected; configuration attributes are not evaluated.
    /// Returns all loading errors, never a partially populated registry.
    pub fn load(
        providers: &[ParsedProviderConfig],
    ) -> Result<ProviderRegistry, Vec<ProviderLoadError>> {
        let mut registry = ProviderRegistry::new();
        let mut names = BTreeMap::new();
        let mut errors = Vec::new();

        for config in providers {
            let name = &config.name.value;
            if let Some(first_span) = names.get(name) {
                errors.push(ProviderLoadError::DuplicateName {
                    name: name.clone(),
                    first_span: *first_span,
                    duplicate_span: config.name.span,
                });
                continue;
            }
            names.insert(name.clone(), config.name.span);

            if let Some(source) = &config.source {
                if source.value != "builtin" {
                    errors.push(ProviderLoadError::UnsupportedSource {
                        name: name.clone(),
                        span: source.span,
                    });
                    continue;
                }
            }

            let provider: Box<dyn Provider> = match config.provider_type.value.as_str() {
                "ovh" => Box::new(OvhProvider::new()),
                _ => {
                    errors.push(ProviderLoadError::UnknownType {
                        name: name.clone(),
                        provider_type: config.provider_type.value.clone(),
                        span: config.provider_type.span,
                    });
                    continue;
                }
            };
            registry
                .register(name.clone(), provider)
                .expect("provider configuration names were checked for duplicates");
        }
        if errors.is_empty() {
            Ok(registry)
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderLoadError {
    UnknownType {
        name: String,
        provider_type: String,
        span: SourceSpan,
    },
    UnsupportedSource {
        name: String,
        span: SourceSpan,
    },
    DuplicateName {
        name: String,
        first_span: SourceSpan,
        duplicate_span: SourceSpan,
    },
}

impl ProviderLoadError {
    /// Primary diagnostic location. For duplicate names, the original location
    /// remains in `first_span` for a related-location note.
    pub fn primary_span(&self) -> SourceSpan {
        match self {
            Self::UnknownType { span, .. } | Self::UnsupportedSource { span, .. } => *span,
            Self::DuplicateName { duplicate_span, .. } => *duplicate_span,
        }
    }
}

impl fmt::Display for ProviderLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownType {
                name,
                provider_type,
                ..
            } => write!(
                f,
                "unknown built-in provider type `{provider_type}` for `{name}`"
            ),
            Self::UnsupportedSource { name, .. } => write!(
                f,
                "unsupported source for provider `{name}`; omit source or use `builtin`; custom provider loading is not implemented"
            ),
            Self::DuplicateName { name, .. } => {
                write!(f, "duplicate provider configuration name `{name}`")
            }
        }
    }
}

impl Error for ProviderLoadError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigReader, SourceId, Spanned, TomlReader};

    fn configs(input: &str) -> Vec<ParsedProviderConfig> {
        TomlReader.read(SourceId(12), input).unwrap().providers
    }

    #[test]
    fn primary_spans_select_type_source_and_duplicate_name_locations() {
        let type_span = SourceSpan::new(SourceId(1), 10, 20);
        let source_span = SourceSpan::new(SourceId(2), 30, 40);
        let first_span = SourceSpan::new(SourceId(3), 50, 60);
        let duplicate_span = SourceSpan::new(SourceId(4), 70, 80);
        let cases = [
            (
                ProviderLoadError::UnknownType {
                    name: "production".into(),
                    provider_type: "unknown".into(),
                    span: type_span,
                },
                type_span,
            ),
            (
                ProviderLoadError::UnsupportedSource {
                    name: "production".into(),
                    span: source_span,
                },
                source_span,
            ),
            (
                ProviderLoadError::DuplicateName {
                    name: "production".into(),
                    first_span,
                    duplicate_span,
                },
                duplicate_span,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.primary_span(), expected);
            if let ProviderLoadError::DuplicateName {
                first_span: original,
                ..
            } = error
            {
                assert_eq!(original, first_span);
            }
        }
    }

    #[test]
    fn empty_input_returns_empty_registry() {
        assert!(
            BuiltinProviderLoader::load(&[])
                .unwrap()
                .get("production")
                .is_none()
        );
    }

    #[test]
    fn loads_ovh_by_config_name_with_omitted_or_builtin_source() {
        let configs = configs(
            r#"
[provider.production]
type = "ovh"
[provider.staging]
type = "ovh"
source = "builtin"
"#,
        );
        let registry = BuiltinProviderLoader::load(&configs).unwrap();
        assert_eq!(registry.get("production").unwrap().provider_type(), "ovh");
        assert_eq!(registry.get("staging").unwrap().provider_type(), "ovh");
        assert!(registry.get("ovh").is_none());
    }

    #[test]
    fn unknown_types_report_the_type_value_span() {
        let configs = configs("[provider.production]\ntype = \"unknown\"\n");
        let errors = BuiltinProviderLoader::load(&configs).err().unwrap();
        assert_eq!(
            errors,
            vec![ProviderLoadError::UnknownType {
                name: "production".into(),
                provider_type: "unknown".into(),
                span: configs[0].provider_type.span,
            }]
        );
        assert!(errors[0].to_string().contains("unknown"));
    }

    #[test]
    fn rejects_custom_sources_without_builtin_fallback_or_echoing_source_secrets() {
        for source in [
            "https://user:secret@example.com/provider.git",
            "git@example.com:provider.git",
            "Builtin",
            "",
        ] {
            let mut configs = configs("[provider.production]\ntype = \"ovh\"\n");
            let span = SourceSpan::new(SourceId(12), 40, 60);
            configs[0].source = Some(Spanned::new(source.into(), span));
            let errors = BuiltinProviderLoader::load(&configs).err().unwrap();
            assert_eq!(
                errors,
                vec![ProviderLoadError::UnsupportedSource {
                    name: "production".into(),
                    span
                }]
            );
            assert!(!errors[0].to_string().contains("secret"));
        }
    }

    #[test]
    fn checks_source_before_resolving_type() {
        let configs = configs(
            "[provider.custom]\ntype = \"custom\"\nsource = \"https://example.com/provider.git\"\n",
        );
        assert!(matches!(
            BuiltinProviderLoader::load(&configs)
                .err()
                .unwrap()
                .as_slice(),
            [ProviderLoadError::UnsupportedSource { .. }]
        ));
    }

    #[test]
    fn collects_errors_from_independent_blocks() {
        let configs = configs(
            r#"
[provider.a]
type = "unknown"
[provider.b]
type = "ovh"
source = "https://example.com/provider.git"
[provider.c]
type = "ovh"
"#,
        );
        let errors = BuiltinProviderLoader::load(&configs).err().unwrap();
        assert_eq!(errors.len(), 2);
        assert!(matches!(errors[0], ProviderLoadError::UnknownType { .. }));
        assert!(matches!(
            errors[1],
            ProviderLoadError::UnsupportedSource { .. }
        ));
    }

    #[test]
    fn duplicate_names_retain_original_and_duplicate_name_spans() {
        let mut configs = configs("[provider.production]\ntype = \"ovh\"\n");
        let mut duplicate = configs[0].clone();
        let duplicate_span = SourceSpan::new(SourceId(13), 10, 20);
        duplicate.name.span = duplicate_span;
        configs.push(duplicate);
        assert_eq!(
            BuiltinProviderLoader::load(&configs).err().unwrap(),
            vec![ProviderLoadError::DuplicateName {
                name: "production".into(),
                first_span: configs[0].name.span,
                duplicate_span,
            }]
        );
    }

    #[test]
    fn duplicate_names_are_rejected_even_if_the_first_definition_cannot_load() {
        let mut configs = configs("[provider.production]\ntype = \"unknown\"\n");
        let mut duplicate = configs[0].clone();
        duplicate.provider_type.value = "ovh".into();
        configs.push(duplicate);
        let errors = BuiltinProviderLoader::load(&configs).err().unwrap();
        assert!(matches!(
            errors.as_slice(),
            [
                ProviderLoadError::UnknownType { .. },
                ProviderLoadError::DuplicateName { .. }
            ]
        ));
    }

    #[test]
    fn loading_neither_validates_nor_evaluates_config_attributes() {
        let configs = configs(
            r#"
[provider.production]
type = "ovh"
endpoint = "not-a-valid-endpoint"
application_secret = "{{ parameter.not_evaluated }}"
extra_field = true
"#,
        );
        let before = configs.clone();
        let registry = BuiltinProviderLoader::load(&configs).unwrap();
        assert!(
            registry
                .get("production")
                .unwrap()
                .config_schema()
                .fields
                .contains_key("endpoint")
        );
        assert_eq!(configs, before);
    }
}
