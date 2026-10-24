use super::*;
use crate::config::{ConfigReader, SourceId, TomlReader};
use std::collections::BTreeSet;

fn assert_cycle(input: &str, expected: &[&str]) {
    let config = TomlReader.read(SourceId(0), input).unwrap();
    let errors = DesiredGraphBuilder::build(&config).unwrap_err();
    let [GraphBuildError::Cycle { path }] = errors.as_slice() else {
        panic!("expected cycle error: {errors:?}")
    };
    assert_eq!(path.first(), path.last());
    assert_eq!(path.len(), expected.len() + 1);
    assert_eq!(
        path.iter()
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>(),
        expected.iter().map(|id| (*id).into()).collect()
    );

    // Every adjacent pair in the diagnostic must be an actual directed edge.
    let mut graph = DesiredGraph::new();
    register_declarations(&config, &mut graph).unwrap();
    resolve_providers(&config, &mut graph).unwrap();
    add_reference_dependencies(&config, &mut graph).unwrap();
    for pair in path.windows(2) {
        assert!(graph.dependencies_of(&pair[1]).unwrap().contains(&pair[0]));
    }
    let message = errors[0].to_string();
    for id in expected {
        assert!(message.contains(id));
    }
}

#[test]
fn rejects_self_reference_with_closed_path() {
    assert_cycle(
        "[parameter.self_ref]\ndefault = \"{{ parameter.self_ref }}\"\n",
        &["parameter.self_ref"],
    );
}

#[test]
fn rejects_multi_parameter_cycle_including_in_disconnected_graph() {
    assert_cycle(
        r#"
[parameter.independent]
default = "ready"
[parameter.a]
default = "{{ parameter.b }}"
[parameter.b]
default = "{{ parameter.c }}"
[parameter.c]
default = "{{ parameter.a }}"
"#,
        &["parameter.a", "parameter.b", "parameter.c"],
    );
}

#[test]
fn rejects_provider_configuration_cycle_through_structural_provider_edge() {
    assert_cycle(
        r#"
[provider.cloud]
type = "example"
endpoint = "{{ resource.server.app.host }}"
[resource.server.app]
"#,
        &["provider.cloud", "resource.server.app"],
    );
}

#[test]
fn rejects_cycle_through_resource_data_parameter_and_template() {
    assert_cycle(
        r#"
[provider.cloud]
type = "example"
[resource.server.app]
name = "app-{{ parameter.endpoint }}"
[data.info.main]
lookup = "{{ resource.server.app.id }}"
[parameter.endpoint]
default = [{ host = "{{ data.info.main.host }}" }]
"#,
        &[
            "resource.server.app",
            "data.info.main",
            "parameter.endpoint",
        ],
    );
}

#[test]
fn valid_dependency_chain_builds_and_orders_prerequisites_first() {
    let config = TomlReader
        .read(
            SourceId(0),
            r#"
[parameter.region]
default = "eu"
[provider.cloud]
type = "example"
region = "{{ parameter.region }}"
[resource.server.app]
[data.info.main]
lookup = "{{ resource.server.app.id }}"
[parameter.endpoint]
default = "{{ data.info.main.host }}"
"#,
        )
        .unwrap();
    let graph = DesiredGraphBuilder::build(&config).unwrap();
    assert_eq!(
        graph
            .topological_order()
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec![
            "parameter.region",
            "provider.cloud",
            "resource.server.app",
            "data.info.main",
            "parameter.endpoint",
        ]
    );
}

#[test]
fn invalid_references_are_reported_before_cycle_validation() {
    let config = TomlReader
        .read(
            SourceId(0),
            r#"
[parameter.a]
default = ["{{ parameter.a }}", "{{ parameter.missing }}"]
"#,
        )
        .unwrap();
    assert!(matches!(
        DesiredGraphBuilder::build(&config).unwrap_err().as_slice(),
        [GraphBuildError::UnknownReference { .. }]
    ));
}
