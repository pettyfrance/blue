use super::*;
use crate::config::{ConfigReader, SourceId, Spanned, TomlReader};

fn config(providers: &str) -> ParsedConfig {
    TomlReader.read(SourceId(7), &format!("{providers}\n[resource.server.app]\nname = \"app\"\n[data.info.main]\nlookup = \"app\"\n")).unwrap()
}

fn selected(config: &ParsedConfig, name: &str) {
    let graph = DesiredGraphBuilder::build(config).unwrap();
    let provider = DeclarationId::Provider { name: name.into() };
    for declaration in [
        Declaration::Resource(&config.resources[0]),
        Declaration::Data(&config.data[0]),
    ] {
        assert_eq!(
            graph.provider_for(&declaration.id()).unwrap(),
            Some(provider.clone())
        );
        assert_eq!(
            graph.dependencies_of(&declaration.id()).unwrap(),
            vec![provider.clone()]
        );
        let order = graph.topological_order().unwrap();
        assert!(
            order.iter().position(|id| id == &provider).unwrap()
                < order.iter().position(|id| id == &declaration.id()).unwrap()
        );
    }
}

#[test]
fn infers_sole_provider_without_mutating_parsed_config() {
    let config = config("[provider.cloud]\ntype = \"example\"\n");
    let before = config.clone();
    selected(&config, "cloud");
    assert_eq!(config, before);
    assert!(config.resources[0].provider.is_none());
    assert!(config.data[0].provider.is_none());
}

#[test]
fn sole_provider_need_not_be_marked_default() {
    selected(
        &config("[provider.cloud]\ntype = \"example\"\ndefault = false\n"),
        "cloud",
    );
}

#[test]
fn selects_single_default_among_multiple_providers() {
    selected(
        &config(
            "[provider.alpha]\ntype = \"example\"\ndefault = false\n[provider.cloud]\ntype = \"example\"\ndefault = true\n",
        ),
        "cloud",
    );
}

#[test]
fn explicit_selection_overrides_default() {
    let mut config = config(
        "[provider.alpha]\ntype = \"example\"\n[provider.cloud]\ntype = \"example\"\ndefault = true\n",
    );
    config.resources[0].provider = Some(Spanned::synthetic("alpha".into()));
    config.data[0].provider = Some(Spanned::synthetic("alpha".into()));
    selected(&config, "alpha");
}

#[test]
fn explicit_selection_works_with_multiple_providers_and_no_default() {
    let mut config =
        config("[provider.alpha]\ntype = \"example\"\n[provider.cloud]\ntype = \"example\"\n");
    config.resources[0].provider = Some(Spanned::synthetic("cloud".into()));
    config.data[0].provider = Some(Spanned::synthetic("cloud".into()));
    selected(&config, "cloud");
}

#[test]
fn unknown_explicit_selections_do_not_fall_back_and_retain_selection_spans() {
    let mut config = config("[provider.cloud]\ntype = \"example\"\ndefault = true\n");
    let span = SourceSpan::new(SourceId(42), 20, 30);
    config.resources[0].provider = Some(Spanned::new("absent".into(), span));
    config.data[0].provider = Some(Spanned::new("absent".into(), span));
    let errors = DesiredGraphBuilder::build(&config).unwrap_err();
    assert_eq!(
        errors,
        vec![
            GraphBuildError::UnknownProvider {
                declaration: Declaration::Resource(&config.resources[0]).id(),
                provider: "absent".into(),
                span
            },
            GraphBuildError::UnknownProvider {
                declaration: Declaration::Data(&config.data[0]).id(),
                provider: "absent".into(),
                span
            },
        ]
    );
    assert!(errors[0].to_string().contains("absent"));
}

#[test]
fn no_providers_reports_each_implicit_selection_at_its_block_span() {
    let config = config("");
    let errors = DesiredGraphBuilder::build(&config).unwrap_err();
    assert_eq!(
        errors,
        vec![
            GraphBuildError::MissingProvider {
                declaration: Declaration::Resource(&config.resources[0]).id(),
                span: config.resources[0].span
            },
            GraphBuildError::MissingProvider {
                declaration: Declaration::Data(&config.data[0]).id(),
                span: config.data[0].span
            },
        ]
    );
}

#[test]
fn ambiguity_reports_sorted_candidates_and_block_spans() {
    let config =
        config("[provider.zeta]\ntype = \"example\"\n[provider.alpha]\ntype = \"example\"\n");
    let errors = DesiredGraphBuilder::build(&config).unwrap_err();
    assert_eq!(errors.len(), 2);
    for (error, declaration) in errors.iter().zip([
        Declaration::Resource(&config.resources[0]),
        Declaration::Data(&config.data[0]),
    ]) {
        assert_eq!(
            *error,
            GraphBuildError::AmbiguousProvider {
                declaration: declaration.id(),
                span: declaration.span(),
                candidates: vec![
                    DeclarationId::Provider {
                        name: "alpha".into()
                    },
                    DeclarationId::Provider {
                        name: "zeta".into()
                    }
                ],
            }
        );
    }
    assert!(errors[0].to_string().contains("provider.alpha"));
}

#[test]
fn multiple_defaults_are_global_errors_with_all_default_spans() {
    let mut config = config(
        "[provider.alpha]\ntype = \"example\"\ndefault = true\n[provider.zeta]\ntype = \"example\"\ndefault = true\n",
    );
    config.resources.clear();
    config.data.clear();
    let errors = DesiredGraphBuilder::build(&config).unwrap_err();
    assert_eq!(
        errors,
        vec![GraphBuildError::MultipleDefaultProviders {
            providers: config
                .providers
                .iter()
                .map(|provider| (
                    Declaration::Provider(provider).id(),
                    provider.default.as_ref().unwrap().span
                ))
                .collect(),
        }]
    );
    assert!(errors[0].to_string().contains("provider.alpha"));
    assert!(errors[0].to_string().contains("provider.zeta"));
}

#[test]
fn parameters_do_not_require_providers() {
    let config = TomlReader
        .read(SourceId(0), "[parameter.region]\ndefault = \"eu\"\n")
        .unwrap();
    let graph = DesiredGraphBuilder::build(&config).unwrap();
    assert_eq!(
        graph
            .provider_for(&Declaration::Parameter(&config.parameters[0]).id())
            .unwrap(),
        None
    );
}

#[test]
fn provider_names_not_implementation_types_are_used_for_selection() {
    let mut config = config("[provider.cloud]\ntype = \"example\"\n");
    config.resources[0].provider = Some(Spanned::synthetic("example".into()));
    assert!(
        matches!(DesiredGraphBuilder::build(&config).unwrap_err().as_slice(), [GraphBuildError::UnknownProvider { provider, .. }] if provider == "example")
    );
}
