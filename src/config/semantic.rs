use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use super::model::{Attributes, Parameter, ParsedConfig, ParsedProviderConfig};
use super::source::{SourceSpan, Spanned};

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub providers: Vec<ProviderConfig>,
    pub parameters: Vec<Parameter>,
    pub resources: Vec<Resource>,
    pub data: Vec<Data>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderConfig {
    pub name: Spanned<String>,
    pub provider_type: Spanned<String>,
    pub source: Option<Spanned<String>>,
    pub default: Option<Spanned<bool>>,
    pub attributes: Attributes,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub provider: Spanned<String>,
    pub resource_type: Spanned<String>,
    pub name: Spanned<String>,
    pub attributes: Attributes,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Data {
    pub provider: Spanned<String>,
    pub data_type: Spanned<String>,
    pub name: Spanned<String>,
    pub attributes: Attributes,
    pub span: SourceSpan,
}

pub struct ConfigResolver;

#[derive(Debug, Clone, PartialEq)]
pub enum ResolveError {
    UnknownProvider { provider: Spanned<String> },
    NoProviders { span: SourceSpan },
    AmbiguousProvider { span: SourceSpan },
    MultipleDefaultProviders { providers: Vec<Spanned<String>> },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProvider { provider } => {
                write!(f, "unknown provider `{}`", provider.value)
            }
            Self::NoProviders { .. } => {
                write!(f, "provider is required, but no providers are configured")
            }
            Self::AmbiguousProvider { .. } => write!(
                f,
                "provider must be specified because multiple providers are configured"
            ),
            Self::MultipleDefaultProviders { providers } => write!(
                f,
                "multiple default providers configured: {}",
                providers
                    .iter()
                    .map(|provider| provider.value.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl Error for ResolveError {}

impl ConfigResolver {
    pub fn resolve(parsed: ParsedConfig) -> Result<Config, Vec<ResolveError>> {
        let mut errors = Vec::new();
        let providers = parsed
            .providers
            .into_iter()
            .map(provider_from_parsed)
            .collect::<Vec<_>>();
        let provider_names = providers
            .iter()
            .map(|provider| provider.name.value.clone())
            .collect::<BTreeSet<_>>();
        let default_providers = providers
            .iter()
            .filter(|provider| {
                provider
                    .default
                    .as_ref()
                    .is_some_and(|default| default.value)
            })
            .map(|provider| provider.name.clone())
            .collect::<Vec<_>>();

        if default_providers.len() > 1 {
            errors.push(ResolveError::MultipleDefaultProviders {
                providers: default_providers.clone(),
            });
        }

        let resources = parsed
            .resources
            .into_iter()
            .filter_map(|resource| {
                let provider = resolve_provider(
                    resource.provider.as_ref(),
                    resource.span,
                    &providers,
                    &provider_names,
                    &default_providers,
                );
                match provider {
                    Ok(provider) => Some(Resource {
                        provider,
                        resource_type: resource.resource_type,
                        name: resource.name,
                        attributes: resource.attributes,
                        span: resource.span,
                    }),
                    Err(error) => {
                        errors.push(error);
                        None
                    }
                }
            })
            .collect();

        let data = parsed
            .data
            .into_iter()
            .filter_map(|data| {
                let provider = resolve_provider(
                    data.provider.as_ref(),
                    data.span,
                    &providers,
                    &provider_names,
                    &default_providers,
                );
                match provider {
                    Ok(provider) => Some(Data {
                        provider,
                        data_type: data.data_type,
                        name: data.name,
                        attributes: data.attributes,
                        span: data.span,
                    }),
                    Err(error) => {
                        errors.push(error);
                        None
                    }
                }
            })
            .collect();

        if errors.is_empty() {
            Ok(Config {
                providers,
                parameters: parsed.parameters,
                resources,
                data,
            })
        } else {
            Err(errors)
        }
    }
}

fn provider_from_parsed(provider: ParsedProviderConfig) -> ProviderConfig {
    ProviderConfig {
        name: provider.name,
        provider_type: provider.provider_type,
        source: provider.source,
        default: provider.default,
        attributes: provider.attributes,
        span: provider.span,
    }
}

fn resolve_provider(
    explicit: Option<&Spanned<String>>,
    owner_span: SourceSpan,
    providers: &[ProviderConfig],
    provider_names: &BTreeSet<String>,
    default_providers: &[Spanned<String>],
) -> Result<Spanned<String>, ResolveError> {
    if let Some(explicit) = explicit {
        if provider_names.contains(&explicit.value) {
            return Ok(explicit.clone());
        }

        return Err(ResolveError::UnknownProvider {
            provider: explicit.clone(),
        });
    }

    if providers.is_empty() {
        return Err(ResolveError::NoProviders { span: owner_span });
    }

    if providers.len() == 1 {
        return Ok(providers[0].name.clone());
    }

    if default_providers.len() == 1 {
        return Ok(default_providers[0].clone());
    }

    Err(ResolveError::AmbiguousProvider { span: owner_span })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::config::model::{ParsedData, ParsedResource};
    use crate::config::source::{SourceId, SourceSpan, Spanned};

    fn span(start: usize, end: usize) -> SourceSpan {
        SourceSpan::new(SourceId(0), start, end)
    }

    fn spanned(value: &str) -> Spanned<String> {
        Spanned::synthetic(value.to_string())
    }

    fn provider(name: &str, default: Option<bool>) -> ParsedProviderConfig {
        ParsedProviderConfig {
            name: spanned(name),
            provider_type: spanned(name),
            source: None,
            default: default.map(Spanned::synthetic),
            attributes: BTreeMap::new(),
            span: SourceSpan::SYNTHETIC,
        }
    }

    fn resource(name: &str, explicit_provider: Option<&str>) -> ParsedResource {
        ParsedResource {
            provider: explicit_provider.map(spanned),
            resource_type: spanned("file"),
            name: spanned(name),
            attributes: BTreeMap::new(),
            span: span(10, 20),
        }
    }

    fn data(name: &str, explicit_provider: Option<&str>) -> ParsedData {
        ParsedData {
            provider: explicit_provider.map(spanned),
            data_type: spanned("script"),
            name: spanned(name),
            attributes: BTreeMap::new(),
            span: span(30, 40),
        }
    }

    fn parsed(
        providers: Vec<ParsedProviderConfig>,
        resources: Vec<ParsedResource>,
        data: Vec<ParsedData>,
    ) -> ParsedConfig {
        ParsedConfig {
            providers,
            parameters: Vec::new(),
            resources,
            data,
        }
    }

    #[test]
    fn infers_single_provider() {
        let config = ConfigResolver::resolve(parsed(
            vec![provider("blue", None)],
            vec![resource("config", None)],
            vec![data("setup", None)],
        ))
        .unwrap();

        assert_eq!(config.resources[0].provider.value, "blue");
        assert_eq!(config.data[0].provider.value, "blue");
    }

    #[test]
    fn infers_default_provider() {
        let config = ConfigResolver::resolve(parsed(
            vec![provider("blue", Some(true)), provider("aws", None)],
            vec![resource("config", None)],
            Vec::new(),
        ))
        .unwrap();

        assert_eq!(config.resources[0].provider.value, "blue");
    }

    #[test]
    fn explicit_provider_overrides_default() {
        let config = ConfigResolver::resolve(parsed(
            vec![provider("blue", Some(true)), provider("aws", None)],
            vec![resource("web", Some("aws"))],
            Vec::new(),
        ))
        .unwrap();

        assert_eq!(config.resources[0].provider.value, "aws");
    }

    #[test]
    fn unknown_explicit_provider_errors() {
        let errors = ConfigResolver::resolve(parsed(
            vec![provider("blue", None)],
            vec![resource("web", Some("aws"))],
            Vec::new(),
        ))
        .unwrap_err();

        assert!(matches!(errors[0], ResolveError::UnknownProvider { .. }));
    }

    #[test]
    fn no_providers_errors() {
        let errors =
            ConfigResolver::resolve(parsed(Vec::new(), vec![resource("web", None)], Vec::new()))
                .unwrap_err();

        assert!(matches!(errors[0], ResolveError::NoProviders { .. }));
    }

    #[test]
    fn multiple_providers_without_default_errors() {
        let errors = ConfigResolver::resolve(parsed(
            vec![provider("blue", None), provider("aws", None)],
            vec![resource("web", None)],
            Vec::new(),
        ))
        .unwrap_err();

        assert!(matches!(errors[0], ResolveError::AmbiguousProvider { .. }));
    }

    #[test]
    fn multiple_default_providers_errors() {
        let errors = ConfigResolver::resolve(parsed(
            vec![provider("blue", Some(true)), provider("aws", Some(true))],
            vec![resource("web", None)],
            Vec::new(),
        ))
        .unwrap_err();

        assert!(
            errors
                .iter()
                .any(|error| matches!(error, ResolveError::MultipleDefaultProviders { .. }))
        );
    }
}
