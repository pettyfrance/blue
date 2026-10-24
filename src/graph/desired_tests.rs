use super::*;
use crate::config::{ConfigReader, ParsedConfig, SourceId, TomlReader};

fn config() -> ParsedConfig {
    TomlReader
        .read(
            SourceId(0),
            r#"
[provider.cloud]
type = "example"
[provider.other]
type = "example"
[parameter.region]
default = "eu"
[parameter.environment]
default = "prod"
[resource.server.app]
provider = "cloud"
[data.info.main]
provider = "cloud"
"#,
        )
        .unwrap()
}

fn registered(config: &ParsedConfig) -> DesiredGraph<'_> {
    let mut graph = DesiredGraph::new();
    for declaration in config
        .providers
        .iter()
        .map(Declaration::Provider)
        .chain(config.parameters.iter().map(Declaration::Parameter))
        .chain(config.resources.iter().map(Declaration::Resource))
        .chain(config.data.iter().map(Declaration::Data))
    {
        graph.add_declaration(declaration).unwrap();
    }
    graph
}

fn provider(config: &ParsedConfig) -> DeclarationId {
    Declaration::Provider(&config.providers[0]).id()
}
fn resource(config: &ParsedConfig) -> DeclarationId {
    Declaration::Resource(&config.resources[0]).id()
}
fn data(config: &ParsedConfig) -> DeclarationId {
    Declaration::Data(&config.data[0]).id()
}
fn missing() -> DeclarationId {
    DeclarationId::Parameter {
        name: "missing".into(),
    }
}

#[test]
fn empty_graph_has_empty_order() {
    assert!(DesiredGraph::new().topological_order().unwrap().is_empty());
}

#[test]
fn lookup_borrows_the_original_parsed_blocks() {
    let config = config();
    let graph = registered(&config);
    for declaration in config
        .providers
        .iter()
        .map(Declaration::Provider)
        .chain(config.parameters.iter().map(Declaration::Parameter))
        .chain(config.resources.iter().map(Declaration::Resource))
        .chain(config.data.iter().map(Declaration::Data))
    {
        let found = graph.declaration(&declaration.id()).unwrap();
        assert!(std::ptr::eq(found.attributes(), declaration.attributes()));
        assert_eq!(found.span(), declaration.span());
    }
}

#[test]
fn all_lookup_methods_reject_missing_ids() {
    let config = config();
    let graph = registered(&config);
    let expected = GraphLookupError::MissingDeclaration { id: missing() };
    assert_eq!(graph.declaration(&missing()), Err(expected.clone()));
    assert_eq!(graph.dependencies_of(&missing()), Err(expected.clone()));
    assert_eq!(graph.provider_for(&missing()), Err(expected.clone()));
    assert!(expected.to_string().contains("parameter.missing"));
}

#[test]
fn dependencies_are_direct_sorted_and_deduplicated() {
    let config = config();
    let mut graph = registered(&config);
    let target = resource(&config);
    let provider = provider(&config);
    let parameter = Declaration::Parameter(&config.parameters[0]).id();
    graph.add_dependency(&parameter, &provider).unwrap();
    graph.add_dependency(&provider, &target).unwrap();
    graph.add_dependency(&data(&config), &target).unwrap();
    graph.add_dependency(&provider, &target).unwrap();
    let mut expected = vec![provider, data(&config)];
    expected.sort();
    assert_eq!(graph.dependencies_of(&target).unwrap(), expected);
    assert_eq!(graph.dependencies_of(&parameter).unwrap(), vec![]);
    assert_eq!(graph.graph.edge_count(), 3);
}

#[test]
fn provider_associations_add_edges_for_resource_and_data_only() {
    let config = config();
    let mut graph = registered(&config);
    let provider = provider(&config);
    for target in [resource(&config), data(&config)] {
        graph.set_provider(&target, &provider).unwrap();
        graph.set_provider(&target, &provider).unwrap();
        assert_eq!(graph.provider_for(&target).unwrap(), Some(provider.clone()));
        assert_eq!(
            graph.dependencies_of(&target).unwrap(),
            vec![provider.clone()]
        );
    }
    assert_eq!(graph.graph.edge_count(), 2);
    assert_eq!(graph.provider_for(&provider).unwrap(), None);
    let parameter = Declaration::Parameter(&config.parameters[0]).id();
    assert_eq!(graph.provider_for(&parameter).unwrap(), None);
}

#[test]
fn duplicate_registration_does_not_replace_the_original_block() {
    let config = config();
    let mut graph = registered(&config);
    let mut duplicate = config.resources[0].clone();
    duplicate.attributes.clear();
    let before = graph.graph.node_count();
    assert_eq!(
        graph.add_declaration(Declaration::Resource(&duplicate)),
        Err(ConstructionError::DuplicateDeclaration {
            id: resource(&config)
        })
    );
    let Declaration::Resource(found) = graph.declaration(&resource(&config)).unwrap() else {
        panic!("expected resource")
    };
    assert!(std::ptr::eq(found, &config.resources[0]));
    assert_eq!(graph.graph.node_count(), before);
}

#[test]
fn missing_edge_endpoints_leave_graph_unchanged() {
    let config = config();
    let mut graph = registered(&config);
    let provider = provider(&config);
    let expected = Err(ConstructionError::Lookup(
        GraphLookupError::MissingDeclaration { id: missing() },
    ));
    assert_eq!(graph.add_dependency(&missing(), &provider), expected);
    assert_eq!(graph.add_dependency(&provider, &missing()), expected);
    assert_eq!(graph.set_provider(&missing(), &provider), expected);
    assert_eq!(graph.set_provider(&resource(&config), &missing()), expected);
    assert_eq!(graph.graph.edge_count(), 0);
    assert!(graph.providers.is_empty());
}

#[test]
fn invalid_or_conflicting_provider_associations_do_not_mutate_graph() {
    let config = config();
    let mut graph = registered(&config);
    let provider = provider(&config);
    let resource = resource(&config);
    let parameter = Declaration::Parameter(&config.parameters[0]).id();
    for (owner, selected) in [
        (provider.clone(), provider.clone()),
        (parameter.clone(), provider.clone()),
        (resource.clone(), parameter),
    ] {
        assert_eq!(
            graph.set_provider(&owner, &selected),
            Err(ConstructionError::InvalidProviderAssociation {
                declaration: owner,
                provider: selected
            })
        );
    }
    assert!(graph.providers.is_empty());
    assert_eq!(graph.graph.edge_count(), 0);
    graph.set_provider(&resource, &provider).unwrap();
    let other = Declaration::Provider(&config.providers[1]).id();
    assert_eq!(
        graph.set_provider(&resource, &other),
        Err(ConstructionError::ProviderAlreadySelected {
            declaration: resource.clone()
        })
    );
    assert_eq!(graph.provider_for(&resource).unwrap(), Some(provider));
    assert_eq!(graph.graph.edge_count(), 1);
}

#[test]
fn topological_order_places_every_prerequisite_before_its_dependents() {
    let config = config();
    let mut graph = registered(&config);
    let parameter = Declaration::Parameter(&config.parameters[0]).id();
    let provider = provider(&config);
    let resource = resource(&config);
    let data = data(&config);
    let edges = [
        (&parameter, &provider),
        (&provider, &resource),
        (&provider, &data),
        (&data, &resource),
    ];
    for (from, to) in edges {
        graph.add_dependency(from, to).unwrap();
    }
    let order = graph.topological_order().unwrap();
    assert_eq!(order.len(), graph.indices.len());
    assert_eq!(order.iter().collect::<BTreeSet<_>>().len(), order.len());
    for (from, to) in edges {
        assert!(
            order.iter().position(|id| id == from).unwrap()
                < order.iter().position(|id| id == to).unwrap()
        );
    }
}

#[test]
fn cycle_error_reports_a_complete_closed_path_and_readable_diagnostic() {
    let config = config();
    let mut graph = registered(&config);
    let provider = provider(&config);
    let resource = resource(&config);
    let data = data(&config);
    graph.add_dependency(&provider, &resource).unwrap();
    graph.add_dependency(&resource, &data).unwrap();
    graph.add_dependency(&data, &provider).unwrap();
    let error = graph.topological_order().unwrap_err();
    assert_eq!(error.path.len(), 4);
    assert_eq!(error.path.first(), error.path.last());
    for pair in error.path.windows(2) {
        assert!(graph.dependencies_of(&pair[1]).unwrap().contains(&pair[0]));
    }
    let message = error.to_string();
    for id in [&provider, &resource, &data] {
        assert!(message.contains(&id.to_string()));
    }
}

#[test]
fn self_cycles_and_disconnected_cycles_are_reported() {
    let config = config();
    let mut graph = registered(&config);
    let resource = resource(&config);
    graph.add_dependency(&resource, &resource).unwrap();
    assert_eq!(
        graph.topological_order(),
        Err(GraphCycle {
            path: vec![resource.clone(), resource]
        })
    );
}
