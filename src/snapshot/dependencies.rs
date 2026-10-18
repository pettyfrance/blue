use std::collections::BTreeMap;

use crate::graph::{DesiredGraph, NodeId};

pub struct SnapshotDependencies;

impl SnapshotDependencies {
    pub fn from_graph(graph: &DesiredGraph) -> BTreeMap<String, Vec<String>> {
        graph
            .node_indices()
            .keys()
            .map(|node| (node.to_string(), Self::addresses_for_node(graph, node)))
            .collect()
    }

    pub fn addresses_for_node(graph: &DesiredGraph, node: &NodeId) -> Vec<String> {
        let mut addresses = graph
            .dependencies_of(node)
            .into_iter()
            .map(|dependency| dependency.to_string())
            .collect::<Vec<_>>();
        addresses.sort();
        addresses
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::config::{
        Attribute, Config, ExprKind, ProviderConfig, Resource, SourceSpan, Spanned,
    };
    use crate::graph::DesiredGraphBuilder;

    fn spanned(value: &str) -> Spanned<String> {
        Spanned::synthetic(value.to_string())
    }

    fn reference(path: &[&str]) -> crate::config::Expr {
        Spanned::synthetic(ExprKind::Reference(crate::config::Reference {
            path: path.iter().map(|part| spanned(part)).collect(),
        }))
    }

    fn attr(name: &str, value: crate::config::Expr) -> Attribute {
        Attribute {
            name: spanned(name),
            value,
            span: SourceSpan::SYNTHETIC,
        }
    }

    fn config() -> Config {
        Config {
            providers: vec![ProviderConfig {
                name: spanned("ovh_prod"),
                provider_type: spanned("ovh"),
                source: None,
                default: None,
                attributes: BTreeMap::new(),
                span: SourceSpan::SYNTHETIC,
            }],
            parameters: vec![crate::config::Parameter {
                name: spanned("zone"),
                attributes: BTreeMap::new(),
                span: SourceSpan::SYNTHETIC,
            }],
            data: vec![crate::config::Data {
                provider: spanned("ovh_prod"),
                data_type: spanned("domain_zone"),
                name: spanned("main"),
                attributes: BTreeMap::from([(
                    "name".into(),
                    attr("name", reference(&["parameter", "zone"])),
                )]),
                span: SourceSpan::SYNTHETIC,
            }],
            resources: vec![Resource {
                provider: spanned("ovh_prod"),
                resource_type: spanned("dns_record"),
                name: spanned("www"),
                attributes: BTreeMap::from([(
                    "zone_id".into(),
                    attr("zone_id", reference(&["data", "domain_zone", "main", "id"])),
                )]),
                span: SourceSpan::SYNTHETIC,
            }],
        }
    }

    #[test]
    fn captures_complete_topology_including_provider_and_parameter_paths() {
        let mut config = config();
        config.providers[0].attributes.insert(
            "endpoint".into(),
            attr("endpoint", reference(&["parameter", "zone"])),
        );
        let graph = DesiredGraphBuilder::build(&config).unwrap();
        let dependencies = SnapshotDependencies::from_graph(&graph);
        assert_eq!(dependencies["provider.ovh_prod"], vec!["parameter.zone"]);
        assert!(dependencies["parameter.zone"].is_empty());
        assert_eq!(dependencies.len(), graph.node_count());
        assert_eq!(
            dependencies["data.domain_zone.main"],
            vec!["parameter.zone", "provider.ovh_prod"]
        );
    }

    #[test]
    fn graph_returns_dependencies_for_node() {
        let graph = DesiredGraphBuilder::build(&config()).unwrap();

        let dependencies = graph.dependencies_of(&NodeId::Resource {
            resource_type: "dns_record".into(),
            name: "www".into(),
        });

        assert_eq!(
            dependencies,
            vec![
                NodeId::Provider {
                    name: "ovh_prod".into(),
                },
                NodeId::Data {
                    data_type: "domain_zone".into(),
                    name: "main".into(),
                },
            ]
        );
    }

    #[test]
    fn snapshot_dependency_addresses_are_sorted() {
        let graph = DesiredGraphBuilder::build(&config()).unwrap();

        let dependencies = SnapshotDependencies::addresses_for_node(
            &graph,
            &NodeId::Resource {
                resource_type: "dns_record".into(),
                name: "www".into(),
            },
        );

        assert_eq!(
            dependencies,
            vec!["data.domain_zone.main", "provider.ovh_prod"]
        );
    }

    #[test]
    fn data_dependencies_include_provider_and_referenced_parameter() {
        let graph = DesiredGraphBuilder::build(&config()).unwrap();

        let dependencies = SnapshotDependencies::addresses_for_node(
            &graph,
            &NodeId::Data {
                data_type: "domain_zone".into(),
                name: "main".into(),
            },
        );

        assert_eq!(dependencies, vec!["parameter.zone", "provider.ovh_prod"]);
    }

    #[test]
    fn unknown_node_has_no_dependencies() {
        let graph = DesiredGraphBuilder::build(&config()).unwrap();

        let dependencies = SnapshotDependencies::addresses_for_node(
            &graph,
            &NodeId::Resource {
                resource_type: "dns_record".into(),
                name: "missing".into(),
            },
        );

        assert!(dependencies.is_empty());
    }
}
