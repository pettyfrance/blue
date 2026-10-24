use super::*;
use crate::config::{Attribute, ConfigReader, ExprKind, SourceId, Spanned, TomlReader};

fn parse(input: &str) -> ParsedConfig {
    TomlReader.read(SourceId(9), input).unwrap()
}
fn parameter(name: &str) -> DeclarationId {
    DeclarationId::Parameter { name: name.into() }
}
fn provider(name: &str) -> DeclarationId {
    DeclarationId::Provider { name: name.into() }
}
fn resource(name: &str) -> DeclarationId {
    DeclarationId::Resource {
        resource_type: "server".into(),
        name: name.into(),
    }
}
fn data(name: &str) -> DeclarationId {
    DeclarationId::Data {
        data_type: "info".into(),
        name: name.into(),
    }
}

#[test]
fn discovers_dependencies_in_every_block_kind() {
    let config = parse(
        r#"
[parameter.region]
default = "eu"
[parameter.endpoint]
default = "{{ data.info.main.connection.host }}"
[provider.cloud]
type = "example"
region = "{{ parameter.region }}"
[resource.server.app]
name = "app-{{ parameter.region }}"
account = "{{ provider.cloud.account_id }}"
endpoint = "{{ parameter.endpoint }}"
[resource.server.upstream]
[data.info.main]
lookup = "{{ resource.server.upstream.id }}"
"#,
    );
    let graph = DesiredGraphBuilder::build(&config).unwrap();
    assert_eq!(
        graph.dependencies_of(&provider("cloud")).unwrap(),
        vec![parameter("region")]
    );
    assert_eq!(
        graph.dependencies_of(&parameter("endpoint")).unwrap(),
        vec![data("main")]
    );
    let mut expected = vec![
        provider("cloud"),
        parameter("region"),
        parameter("endpoint"),
    ];
    expected.sort();
    assert_eq!(graph.dependencies_of(&resource("app")).unwrap(), expected);
    let mut expected = vec![provider("cloud"), resource("upstream")];
    expected.sort();
    assert_eq!(graph.dependencies_of(&data("main")).unwrap(), expected);
}

#[test]
fn walks_nested_arrays_inline_objects_subtables_and_templates() {
    let config = parse(
        r#"
[parameter.a]
default = "a"
[parameter.b]
default = "b"
[parameter.c]
default = "c"
[parameter.d]
default = "d"
[parameter.result]
default = [{ nested = ["{{ parameter.a }}", { host = "prefix-{{ parameter.b }}" }] }, "{{ parameter.c }}"]
[parameter.result.labels]
name = "{{ parameter.d }}"
"#,
    );
    let graph = DesiredGraphBuilder::build(&config).unwrap();
    assert_eq!(
        graph.dependencies_of(&parameter("result")).unwrap(),
        vec![
            parameter("a"),
            parameter("b"),
            parameter("c"),
            parameter("d")
        ]
    );
}

#[test]
fn accepts_whole_declaration_references_without_validating_output_fields() {
    let config = parse(
        r#"
[provider.cloud]
type = "example"
[resource.server.app]
[data.info.main]
[parameter.result]
a = "{{ provider.cloud }}"
b = "{{ resource.server.app }}"
c = "{{ data.info.main }}"
d = "{{ resource.server.app.not_yet_known.deeply.nested }}"
"#,
    );
    let graph = DesiredGraphBuilder::build(&config).unwrap();
    let mut expected = vec![provider("cloud"), resource("app"), data("main")];
    expected.sort();
    assert_eq!(
        graph.dependencies_of(&parameter("result")).unwrap(),
        expected
    );
}

#[test]
fn repeated_references_and_provider_relationship_share_one_edge() {
    let config = parse(
        r#"
[provider.cloud]
type = "example"
[resource.server.app]
a = "{{ provider.cloud.account_id }}"
b = ["{{ provider.cloud.region }}", "x-{{ provider.cloud.account_id }}"]
"#,
    );
    let graph = DesiredGraphBuilder::build(&config).unwrap();
    assert_eq!(
        graph.dependencies_of(&resource("app")).unwrap(),
        vec![provider("cloud")]
    );
    // Two declarations and one edge produce two entries, not repeated work.
    assert_eq!(
        graph.topological_order().unwrap(),
        vec![provider("cloud"), resource("app")]
    );
}

#[test]
fn missing_targets_are_collected_with_exact_reference_spans() {
    let input = r#"
[parameter.result]
default = ["{{ parameter.absent }}", "prefix-{{ resource.server.missing.id }}", "{{ provider.missing.region }}", "{{ data.info.missing }}"]
"#;
    let config = parse(input);
    let errors = DesiredGraphBuilder::build(&config).unwrap_err();
    assert_eq!(errors.len(), 4);
    for (error, text, target) in [
        (&errors[0], "parameter.absent", parameter("absent")),
        (
            &errors[1],
            "resource.server.missing.id",
            resource("missing"),
        ),
        (&errors[2], "provider.missing.region", provider("missing")),
        (&errors[3], "data.info.missing", data("missing")),
    ] {
        let start = input.find(text).unwrap();
        assert_eq!(
            *error,
            GraphBuildError::UnknownReference {
                target,
                path: text.split('.').map(String::from).collect(),
                span: SourceSpan::new(SourceId(9), start, start + text.len())
            }
        );
        assert!(error.to_string().contains(text));
    }
}

#[test]
fn malformed_namespaces_and_incomplete_identities_are_reported() {
    let input = r#"
[parameter.result]
default = ["{{ nonsense.name }}", "{{ resource.server }}", "{{ data.info }}"]
"#;
    let errors = DesiredGraphBuilder::build(&parse(input)).unwrap_err();
    assert_eq!(errors.len(), 3);
    for (error, text) in errors
        .iter()
        .zip(["nonsense.name", "resource.server", "data.info"])
    {
        let start = input.find(text).unwrap();
        assert_eq!(
            *error,
            GraphBuildError::MalformedReference {
                path: text.split('.').map(String::from).collect(),
                span: SourceSpan::new(SourceId(9), start, start + text.len())
            }
        );
    }
}

#[test]
fn manually_constructed_empty_reference_uses_expression_span() {
    let mut config = parse("[parameter.result]\ndefault = 1\n");
    let span = SourceSpan::new(SourceId(10), 20, 30);
    config.parameters[0].attributes.insert(
        "default".into(),
        Attribute {
            name: Spanned::synthetic("default".into()),
            value: Spanned::new(ExprKind::Reference(Reference { path: vec![] }), span),
            span,
        },
    );
    assert_eq!(
        DesiredGraphBuilder::build(&config).unwrap_err(),
        vec![GraphBuildError::MalformedReference { path: vec![], span }]
    );
}

#[test]
fn registration_order_does_not_limit_reference_resolution() {
    let config = parse(
        r#"
[parameter.first]
default = "{{ parameter.last }}"
[parameter.last]
default = "ready"
"#,
    );
    let graph = DesiredGraphBuilder::build(&config).unwrap();
    assert_eq!(
        graph.dependencies_of(&parameter("first")).unwrap(),
        vec![parameter("last")]
    );
}

#[test]
fn reference_pass_records_self_edges_before_cycle_validation() {
    let config = parse("[parameter.self_ref]\ndefault = \"{{ parameter.self_ref }}\"\n");
    let mut graph = DesiredGraph::new();
    register_declarations(&config, &mut graph).unwrap();
    add_reference_dependencies(&config, &mut graph).unwrap();
    assert_eq!(
        graph.dependencies_of(&parameter("self_ref")).unwrap(),
        vec![parameter("self_ref")]
    );
    assert!(graph.topological_order().is_err());
}

#[test]
fn ordinary_strings_do_not_create_dependencies() {
    let config = parse("[parameter.result]\ndefault = \"resource.server.not_a_reference\"\n");
    let graph = DesiredGraphBuilder::build(&config).unwrap();
    assert!(
        graph
            .dependencies_of(&parameter("result"))
            .unwrap()
            .is_empty()
    );
}
