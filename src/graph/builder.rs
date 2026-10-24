use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::config::{ParsedConfig, Reference, SourceSpan};

use super::desired::{ConstructionError, DesiredGraph};
use super::model::{Declaration, DeclarationId};

/// Build a validated desired graph directly from borrowed parsed declarations.
pub struct DesiredGraphBuilder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphBuildError {
    DuplicateDeclaration {
        id: DeclarationId,
        first_span: SourceSpan,
        duplicate_span: SourceSpan,
    },
    UnknownProvider {
        declaration: DeclarationId,
        provider: String,
        span: SourceSpan,
    },
    MissingProvider {
        declaration: DeclarationId,
        span: SourceSpan,
    },
    AmbiguousProvider {
        declaration: DeclarationId,
        candidates: Vec<DeclarationId>,
        span: SourceSpan,
    },
    MultipleDefaultProviders {
        providers: Vec<(DeclarationId, SourceSpan)>,
    },
    Cycle {
        /// The first declaration is repeated at the end to close the cycle.
        path: Vec<DeclarationId>,
    },
    MalformedReference {
        path: Vec<String>,
        span: SourceSpan,
    },
    UnknownReference {
        target: DeclarationId,
        path: Vec<String>,
        span: SourceSpan,
    },
}

impl fmt::Display for GraphBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateDeclaration { id, .. } => write!(f, "duplicate declaration `{id}`"),
            Self::UnknownProvider {
                declaration,
                provider,
                ..
            } => write!(
                f,
                "unknown provider `{provider}` selected by `{declaration}`"
            ),
            Self::MissingProvider { declaration, .. } => {
                write!(f, "no provider is configured for `{declaration}`")
            }
            Self::AmbiguousProvider {
                declaration,
                candidates,
                ..
            } => {
                write!(
                    f,
                    "provider selection is ambiguous for `{declaration}`; select a provider explicitly or mark one default; candidates: "
                )?;
                for (index, candidate) in candidates.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{candidate}")?;
                }
                Ok(())
            }
            Self::MalformedReference { path, .. } => {
                write!(f, "malformed reference `{}`", format_reference(path))
            }
            Self::UnknownReference { target, path, .. } => write!(
                f,
                "reference `{}` targets undeclared `{target}`",
                format_reference(path)
            ),
            Self::Cycle { path } => {
                f.write_str("dependency cycle: ")?;
                for (index, id) in path.iter().enumerate() {
                    if index > 0 {
                        f.write_str(" → ")?;
                    }
                    write!(f, "{id}")?;
                }
                Ok(())
            }
            Self::MultipleDefaultProviders { providers } => {
                f.write_str("multiple providers are marked default: ")?;
                for (index, (provider, _)) in providers.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{provider}")?;
                }
                Ok(())
            }
        }
    }
}

impl GraphBuildError {
    /// Primary diagnostic location. Related spans remain in the error fields;
    /// cycles have no single source location and retain their full path instead.
    pub fn primary_span(&self) -> Option<SourceSpan> {
        match self {
            Self::DuplicateDeclaration { duplicate_span, .. } => Some(*duplicate_span),
            Self::UnknownProvider { span, .. }
            | Self::MissingProvider { span, .. }
            | Self::AmbiguousProvider { span, .. }
            | Self::MalformedReference { span, .. }
            | Self::UnknownReference { span, .. } => Some(*span),
            Self::MultipleDefaultProviders { providers } => {
                providers.first().map(|(_, span)| *span)
            }
            Self::Cycle { .. } => None,
        }
    }
}

impl Error for GraphBuildError {}

impl DesiredGraphBuilder {
    /// Return a complete graph, or errors from the first failing validation pass.
    /// No partial graph is exposed on failure.
    pub fn build(config: &ParsedConfig) -> Result<DesiredGraph<'_>, Vec<GraphBuildError>> {
        let mut graph = DesiredGraph::new();
        register_declarations(config, &mut graph)?;
        resolve_providers(config, &mut graph)?;
        add_reference_dependencies(config, &mut graph)?;
        check_cycles(&graph).map_err(|error| vec![error])?;
        Ok(graph)
    }
}

fn register_declarations<'a>(
    config: &'a ParsedConfig,
    graph: &mut DesiredGraph<'a>,
) -> Result<(), Vec<GraphBuildError>> {
    let mut errors = Vec::new();
    for declaration in declarations(config) {
        match graph.add_declaration(declaration) {
            Ok(()) => {}
            Err(ConstructionError::DuplicateDeclaration { id }) => {
                let first = graph
                    .declaration(&id)
                    .expect("duplicate declaration is registered");
                errors.push(GraphBuildError::DuplicateDeclaration {
                    id,
                    first_span: first.span(),
                    duplicate_span: declaration.span(),
                });
            }
            Err(_) => {
                unreachable!("registering a declaration can only fail for a duplicate identity")
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn declarations(config: &ParsedConfig) -> impl Iterator<Item = Declaration<'_>> {
    config
        .providers
        .iter()
        .map(Declaration::Provider)
        .chain(config.parameters.iter().map(Declaration::Parameter))
        .chain(config.resources.iter().map(Declaration::Resource))
        .chain(config.data.iter().map(Declaration::Data))
}

fn resolve_providers(
    config: &ParsedConfig,
    graph: &mut DesiredGraph<'_>,
) -> Result<(), Vec<GraphBuildError>> {
    let providers = config
        .providers
        .iter()
        .map(|provider| (provider.name.value.as_str(), provider))
        .collect::<BTreeMap<_, _>>();
    let defaults = providers
        .values()
        .copied()
        .filter(|provider| {
            provider
                .default
                .as_ref()
                .is_some_and(|default| default.value)
        })
        .collect::<Vec<_>>();
    if defaults.len() > 1 {
        return Err(vec![GraphBuildError::MultipleDefaultProviders {
            providers: defaults
                .iter()
                .map(|provider| {
                    (
                        Declaration::Provider(provider).id(),
                        provider
                            .default
                            .as_ref()
                            .expect("default metadata is present")
                            .span,
                    )
                })
                .collect(),
        }]);
    }

    let mut errors = Vec::new();
    let declarations = config
        .resources
        .iter()
        .map(|resource| (Declaration::Resource(resource), resource.provider.as_ref()))
        .chain(
            config
                .data
                .iter()
                .map(|data| (Declaration::Data(data), data.provider.as_ref())),
        );
    for (declaration, explicit) in declarations {
        let id = declaration.id();
        let selected = if let Some(explicit) = explicit {
            match providers.get(explicit.value.as_str()) {
                Some(provider) => *provider,
                None => {
                    errors.push(GraphBuildError::UnknownProvider {
                        declaration: id,
                        provider: explicit.value.clone(),
                        span: explicit.span,
                    });
                    continue;
                }
            }
        } else if providers.len() == 1 {
            *providers.values().next().expect("one provider exists")
        } else if let Some(default) = defaults.first() {
            *default
        } else {
            errors.push(if providers.is_empty() {
                GraphBuildError::MissingProvider {
                    declaration: id,
                    span: declaration.span(),
                }
            } else {
                GraphBuildError::AmbiguousProvider {
                    declaration: id,
                    candidates: providers
                        .values()
                        .map(|provider| Declaration::Provider(provider).id())
                        .collect(),
                    span: declaration.span(),
                }
            });
            continue;
        };
        graph
            .set_provider(&id, &Declaration::Provider(selected).id())
            .expect("registered resource/data and provider permit a single association");
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn add_reference_dependencies(
    config: &ParsedConfig,
    graph: &mut DesiredGraph<'_>,
) -> Result<(), Vec<GraphBuildError>> {
    let mut errors = Vec::new();
    for declaration in declarations(config) {
        let owner = declaration.id();
        for attribute in declaration.attributes().values() {
            for reference in attribute.value.references() {
                let path = reference
                    .path
                    .iter()
                    .map(|part| part.value.clone())
                    .collect::<Vec<_>>();
                let span = reference_span(reference, attribute.value.span);
                let Some(target) = reference_target(&path) else {
                    errors.push(GraphBuildError::MalformedReference { path, span });
                    continue;
                };
                if graph.declaration(&target).is_err() {
                    errors.push(GraphBuildError::UnknownReference { target, path, span });
                    continue;
                }
                graph
                    .add_dependency(&target, &owner)
                    .expect("reference target and owner are registered");
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// Interpret declaration identity only; suffix fields are evaluated later.
fn reference_target(path: &[String]) -> Option<DeclarationId> {
    if path.iter().any(String::is_empty) {
        return None;
    }
    match path.first().map(String::as_str) {
        Some("provider") if path.len() >= 2 => Some(DeclarationId::Provider {
            name: path[1].clone(),
        }),
        Some("parameter") if path.len() >= 2 => Some(DeclarationId::Parameter {
            name: path[1].clone(),
        }),
        Some("resource") if path.len() >= 3 => Some(DeclarationId::Resource {
            resource_type: path[1].clone(),
            name: path[2].clone(),
        }),
        Some("data") if path.len() >= 3 => Some(DeclarationId::Data {
            data_type: path[1].clone(),
            name: path[2].clone(),
        }),
        _ => None,
    }
}

fn check_cycles(graph: &DesiredGraph<'_>) -> Result<(), GraphBuildError> {
    graph
        .topological_order()
        .map(|_| ())
        .map_err(|cycle| GraphBuildError::Cycle { path: cycle.path })
}

fn reference_span(reference: &Reference, fallback: SourceSpan) -> SourceSpan {
    match (reference.path.first(), reference.path.last()) {
        (Some(first), Some(last)) => {
            SourceSpan::new(first.span.source, first.span.span.start, last.span.span.end)
        }
        _ => fallback,
    }
}

fn format_reference(path: &[String]) -> String {
    if path.is_empty() {
        "<empty>".into()
    } else {
        path.join(".")
    }
}

#[cfg(test)]
#[path = "cycle_validation_tests.rs"]
mod cycle_validation_tests;

#[cfg(test)]
#[path = "reference_dependency_tests.rs"]
mod reference_dependency_tests;

#[cfg(test)]
#[path = "provider_resolution_tests.rs"]
mod provider_resolution_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigReader, SourceId, TomlReader};

    fn config() -> ParsedConfig {
        TomlReader
            .read(
                SourceId(3),
                r#"
[provider.app]
type = "example"
[parameter.app]
default = "prod"
[resource.server.app]
provider = "app"
[data.server.app]
provider = "app"
"#,
            )
            .unwrap()
    }

    #[test]
    fn empty_config_registers_no_declarations() {
        let config = ParsedConfig {
            providers: vec![],
            parameters: vec![],
            resources: vec![],
            data: vec![],
        };
        assert!(
            DesiredGraphBuilder::build(&config)
                .unwrap()
                .topological_order()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn registers_all_block_kinds_without_copying_them() {
        let config = config();
        let graph = DesiredGraphBuilder::build(&config).unwrap();
        let declarations = [
            Declaration::Provider(&config.providers[0]),
            Declaration::Parameter(&config.parameters[0]),
            Declaration::Resource(&config.resources[0]),
            Declaration::Data(&config.data[0]),
        ];
        assert_eq!(graph.topological_order().unwrap().len(), declarations.len());
        for original in declarations {
            let found = graph.declaration(&original.id()).unwrap();
            assert!(std::ptr::eq(found.attributes(), original.attributes()));
            assert_eq!(found.span(), original.span());
            let expected = match original {
                Declaration::Resource(_) | Declaration::Data(_) => {
                    vec![Declaration::Provider(&config.providers[0]).id()]
                }
                _ => vec![],
            };
            assert_eq!(graph.dependencies_of(&original.id()).unwrap(), expected);
        }
    }

    #[test]
    fn same_name_in_different_local_types_is_not_a_duplicate() {
        let mut config = config();
        let mut resource = config.resources[0].clone();
        resource.resource_type.value = "database".into();
        config.resources.push(resource);
        let mut data = config.data[0].clone();
        data.data_type.value = "database".into();
        config.data.push(data);
        assert_eq!(
            DesiredGraphBuilder::build(&config)
                .unwrap()
                .topological_order()
                .unwrap()
                .len(),
            6
        );
    }

    #[test]
    fn collects_duplicates_in_every_block_kind_with_both_spans() {
        let mut config = config();
        let duplicate_span = SourceSpan::new(SourceId(8), 10, 20);
        let expected = [
            Declaration::Provider(&config.providers[0]),
            Declaration::Parameter(&config.parameters[0]),
            Declaration::Resource(&config.resources[0]),
            Declaration::Data(&config.data[0]),
        ]
        .map(|declaration| GraphBuildError::DuplicateDeclaration {
            id: declaration.id(),
            first_span: declaration.span(),
            duplicate_span,
        });
        let mut provider = config.providers[0].clone();
        provider.span = duplicate_span;
        let mut parameter = config.parameters[0].clone();
        parameter.span = duplicate_span;
        let mut resource = config.resources[0].clone();
        resource.span = duplicate_span;
        let mut data = config.data[0].clone();
        data.span = duplicate_span;
        config.providers.push(provider);
        config.parameters.push(parameter);
        config.resources.push(resource);
        config.data.push(data);
        let errors = DesiredGraphBuilder::build(&config).unwrap_err();
        assert_eq!(errors, expected);
        assert!(errors[0].to_string().contains("provider.app"));
    }

    #[test]
    fn repeated_duplicates_preserve_first_declaration_and_continue_registration() {
        let mut config = config();
        let first_span = config.parameters[0].span;
        let mut duplicate = config.parameters[0].clone();
        duplicate.span = SourceSpan::new(SourceId(4), 10, 20);
        config.parameters.push(duplicate.clone());
        duplicate.span = SourceSpan::new(SourceId(5), 30, 40);
        config.parameters.push(duplicate);
        let mut graph = DesiredGraph::new();
        let errors = register_declarations(&config, &mut graph).unwrap_err();
        assert_eq!(errors.len(), 2);
        for error in errors {
            let GraphBuildError::DuplicateDeclaration {
                first_span: reported,
                ..
            } = error
            else {
                panic!("expected duplicate declaration")
            };
            assert_eq!(reported, first_span);
        }
        let id = Declaration::Parameter(&config.parameters[0]).id();
        let Declaration::Parameter(found) = graph.declaration(&id).unwrap() else {
            panic!("expected parameter")
        };
        assert!(std::ptr::eq(found, &config.parameters[0]));
        assert!(
            graph
                .declaration(&Declaration::Resource(&config.resources[0]).id())
                .is_ok()
        );
        assert!(
            graph
                .declaration(&Declaration::Data(&config.data[0]).id())
                .is_ok()
        );
    }

    #[test]
    fn registration_does_not_resolve_providers_or_references() {
        let config = TomlReader
            .read(
                SourceId(0),
                r#"
[resource.server.app]
provider = "not_declared"
name = "{{ parameter.not_declared }}"
"#,
            )
            .unwrap();
        let mut graph = DesiredGraph::new();
        register_declarations(&config, &mut graph).unwrap();
        let id = Declaration::Resource(&config.resources[0]).id();
        assert!(graph.dependencies_of(&id).unwrap().is_empty());
        assert_eq!(graph.provider_for(&id).unwrap(), None);
    }
}
