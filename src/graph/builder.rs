use std::fmt;

use super::model::{DesiredGraph, EdgeKind, GraphNode, GraphNodeKind, NodeId};
use crate::config::{Attributes, Config, Reference, SourceSpan};

pub struct DesiredGraphBuilder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphBuildError {
    MalformedReference { path: Vec<String>, span: SourceSpan },
    UnknownReference { path: Vec<String>, span: SourceSpan },
    Cycle { path: Vec<NodeId> },
}

impl fmt::Display for GraphBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedReference { path, .. } => {
                write!(f, "malformed reference `{}`", format_reference_path(path))
            }
            Self::UnknownReference { path, .. } => {
                write!(f, "unknown reference `{}`", format_reference_path(path))
            }
            Self::Cycle { path } => write!(
                f,
                "dependency cycle detected: {}",
                path.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" -> ")
            ),
        }
    }
}

impl std::error::Error for GraphBuildError {}

impl DesiredGraphBuilder {
    pub fn build(config: &Config) -> Result<DesiredGraph, Vec<GraphBuildError>> {
        let mut graph = DesiredGraph::new();
        let mut errors = Vec::new();

        for provider in &config.providers {
            let id = NodeId::Provider {
                name: provider.name.value.clone(),
            };
            graph.add_node(GraphNode {
                id,
                kind: GraphNodeKind::Provider,
            });
        }

        for parameter in &config.parameters {
            let id = NodeId::Parameter {
                name: parameter.name.value.clone(),
            };
            graph.add_node(GraphNode {
                id,
                kind: GraphNodeKind::Parameter,
            });
        }

        for data in &config.data {
            let id = NodeId::Data {
                data_type: data.data_type.value.clone(),
                name: data.name.value.clone(),
            };
            graph.add_node(GraphNode {
                id: id.clone(),
                kind: GraphNodeKind::Data {
                    provider: data.provider.value.clone(),
                },
            });
            graph.add_edge(
                &NodeId::Provider {
                    name: data.provider.value.clone(),
                },
                &id,
                EdgeKind::Provider,
            );
        }

        for resource in &config.resources {
            let id = NodeId::Resource {
                resource_type: resource.resource_type.value.clone(),
                name: resource.name.value.clone(),
            };
            graph.add_node(GraphNode {
                id: id.clone(),
                kind: GraphNodeKind::Resource {
                    provider: resource.provider.value.clone(),
                },
            });
            graph.add_edge(
                &NodeId::Provider {
                    name: resource.provider.value.clone(),
                },
                &id,
                EdgeKind::Provider,
            );
        }

        for provider in &config.providers {
            let owner = NodeId::Provider {
                name: provider.name.value.clone(),
            };
            add_reference_edges(&mut graph, &owner, &provider.attributes, &mut errors);
        }

        for parameter in &config.parameters {
            let owner = NodeId::Parameter {
                name: parameter.name.value.clone(),
            };
            add_reference_edges(&mut graph, &owner, &parameter.attributes, &mut errors);
        }

        for data in &config.data {
            let owner = NodeId::Data {
                data_type: data.data_type.value.clone(),
                name: data.name.value.clone(),
            };
            add_reference_edges(&mut graph, &owner, &data.attributes, &mut errors);
        }

        for resource in &config.resources {
            let owner = NodeId::Resource {
                resource_type: resource.resource_type.value.clone(),
                name: resource.name.value.clone(),
            };
            add_reference_edges(&mut graph, &owner, &resource.attributes, &mut errors);
        }

        if let Some(path) = graph.find_cycle() {
            errors.push(GraphBuildError::Cycle { path });
        }

        if errors.is_empty() {
            Ok(graph)
        } else {
            Err(errors)
        }
    }
}

fn add_reference_edges(
    graph: &mut DesiredGraph,
    owner: &NodeId,
    attributes: &Attributes,
    errors: &mut Vec<GraphBuildError>,
) {
    for attribute in attributes.values() {
        for reference in attribute.value.references() {
            let target = match reference_to_node_id(reference) {
                Ok(target) => target,
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            };

            if graph.contains_node(&target) {
                graph.add_edge(&target, owner, EdgeKind::Reference);
            } else {
                errors.push(GraphBuildError::UnknownReference {
                    path: reference_path(reference),
                    span: reference_span(reference),
                });
            }
        }
    }
}

fn reference_to_node_id(reference: &Reference) -> Result<NodeId, GraphBuildError> {
    let path = reference_path(reference);
    let span = reference_span(reference);

    match path.first().map(String::as_str) {
        Some("parameter") if path.len() >= 2 => Ok(NodeId::Parameter {
            name: path[1].clone(),
        }),
        Some("provider") if path.len() >= 2 => Ok(NodeId::Provider {
            name: path[1].clone(),
        }),
        Some("data") if path.len() >= 3 => Ok(NodeId::Data {
            data_type: path[1].clone(),
            name: path[2].clone(),
        }),
        Some("resource") if path.len() >= 3 => Ok(NodeId::Resource {
            resource_type: path[1].clone(),
            name: path[2].clone(),
        }),
        _ => Err(GraphBuildError::MalformedReference { path, span }),
    }
}

fn format_reference_path(path: &[String]) -> String {
    if path.is_empty() {
        "<empty>".into()
    } else {
        path.join(".")
    }
}

fn reference_path(reference: &Reference) -> Vec<String> {
    reference
        .path
        .iter()
        .map(|segment| segment.value.clone())
        .collect()
}

fn reference_span(reference: &Reference) -> SourceSpan {
    let Some(first) = reference.path.first() else {
        return SourceSpan::SYNTHETIC;
    };
    let Some(last) = reference.path.last() else {
        return first.span;
    };

    SourceSpan::new(first.span.source, first.span.span.start, last.span.span.end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        Attribute, Data, ExprKind, Parameter, ProviderConfig, Resource, Spanned, Value,
    };
    use crate::config::{SourceId, SourceSpan};
    use std::collections::BTreeMap;

    fn spanned(value: &str) -> Spanned<String> {
        Spanned::synthetic(value.to_string())
    }

    fn reference(path: &[&str]) -> crate::config::Expr {
        Spanned::synthetic(ExprKind::Reference(Reference {
            path: path
                .iter()
                .map(|segment| Spanned::synthetic(segment.to_string()))
                .collect(),
        }))
    }

    fn attribute(name: &str, value: crate::config::Expr) -> Attribute {
        Attribute {
            name: spanned(name),
            value,
            span: SourceSpan::SYNTHETIC,
        }
    }

    fn attrs(attributes: Vec<(&str, crate::config::Expr)>) -> Attributes {
        attributes
            .into_iter()
            .map(|(name, value)| (name.to_string(), attribute(name, value)))
            .collect()
    }

    fn config() -> Config {
        Config {
            providers: vec![ProviderConfig {
                name: spanned("blue"),
                provider_type: spanned("blue"),
                source: None,
                default: None,
                attributes: BTreeMap::new(),
                span: SourceSpan::SYNTHETIC,
            }],
            parameters: vec![Parameter {
                name: spanned("content"),
                attributes: BTreeMap::new(),
                span: SourceSpan::SYNTHETIC,
            }],
            resources: vec![Resource {
                provider: spanned("blue"),
                resource_type: spanned("file"),
                name: spanned("config"),
                attributes: BTreeMap::new(),
                span: SourceSpan::SYNTHETIC,
            }],
            data: vec![Data {
                provider: spanned("blue"),
                data_type: spanned("script"),
                name: spanned("setup"),
                attributes: BTreeMap::new(),
                span: SourceSpan::SYNTHETIC,
            }],
        }
    }

    #[test]
    fn adds_provider_parameter_data_and_resource_nodes() {
        let graph = DesiredGraphBuilder::build(&config()).unwrap();

        assert!(graph.contains_node(&NodeId::Provider {
            name: "blue".into()
        }));
        assert!(graph.contains_node(&NodeId::Parameter {
            name: "content".into()
        }));
        assert!(graph.contains_node(&NodeId::Data {
            data_type: "script".into(),
            name: "setup".into()
        }));
        assert!(graph.contains_node(&NodeId::Resource {
            resource_type: "file".into(),
            name: "config".into()
        }));
    }

    #[test]
    fn adds_provider_edges_to_data_and_resource_nodes() {
        let graph = DesiredGraphBuilder::build(&config()).unwrap();

        assert!(graph.has_edge(
            &NodeId::Provider {
                name: "blue".into()
            },
            &NodeId::Data {
                data_type: "script".into(),
                name: "setup".into(),
            },
            &EdgeKind::Provider,
        ));
        assert!(graph.has_edge(
            &NodeId::Provider {
                name: "blue".into()
            },
            &NodeId::Resource {
                resource_type: "file".into(),
                name: "config".into(),
            },
            &EdgeKind::Provider,
        ));
    }

    #[test]
    fn stores_provider_metadata_on_data_and_resource_nodes() {
        let graph = DesiredGraphBuilder::build(&config()).unwrap();

        let data = graph
            .node(&NodeId::Data {
                data_type: "script".into(),
                name: "setup".into(),
            })
            .unwrap();
        assert_eq!(
            data.kind,
            GraphNodeKind::Data {
                provider: "blue".into()
            }
        );

        let resource = graph
            .node(&NodeId::Resource {
                resource_type: "file".into(),
                name: "config".into(),
            })
            .unwrap();
        assert_eq!(
            resource.kind,
            GraphNodeKind::Resource {
                provider: "blue".into()
            }
        );
    }

    #[test]
    fn adds_resource_to_parameter_reference_edge() {
        let mut config = config();
        config.resources[0].attributes =
            attrs(vec![("content", reference(&["parameter", "content"]))]);

        let graph = DesiredGraphBuilder::build(&config).unwrap();

        assert!(graph.has_edge(
            &NodeId::Parameter {
                name: "content".into()
            },
            &NodeId::Resource {
                resource_type: "file".into(),
                name: "config".into(),
            },
            &EdgeKind::Reference,
        ));
    }

    #[test]
    fn adds_resource_to_data_reference_edge() {
        let mut config = config();
        config.resources[0].attributes = attrs(vec![(
            "script",
            reference(&["data", "script", "setup", "output"]),
        )]);

        let graph = DesiredGraphBuilder::build(&config).unwrap();

        assert!(graph.has_edge(
            &NodeId::Data {
                data_type: "script".into(),
                name: "setup".into(),
            },
            &NodeId::Resource {
                resource_type: "file".into(),
                name: "config".into(),
            },
            &EdgeKind::Reference,
        ));
    }

    #[test]
    fn adds_resource_to_resource_reference_edge() {
        let mut config = config();
        config.resources.push(Resource {
            provider: spanned("blue"),
            resource_type: spanned("file"),
            name: spanned("other"),
            attributes: attrs(vec![(
                "path",
                reference(&["resource", "file", "config", "id"]),
            )]),
            span: SourceSpan::SYNTHETIC,
        });

        let graph = DesiredGraphBuilder::build(&config).unwrap();

        assert!(graph.has_edge(
            &NodeId::Resource {
                resource_type: "file".into(),
                name: "config".into(),
            },
            &NodeId::Resource {
                resource_type: "file".into(),
                name: "other".into(),
            },
            &EdgeKind::Reference,
        ));
    }

    #[test]
    fn deduplicates_duplicate_edges() {
        let mut config = config();
        config.resources[0].attributes = attrs(vec![
            ("content_a", reference(&["parameter", "content"])),
            ("content_b", reference(&["parameter", "content"])),
        ]);

        let graph = DesiredGraphBuilder::build(&config).unwrap();

        assert_eq!(
            graph.edge_count_between(
                &NodeId::Parameter {
                    name: "content".into()
                },
                &NodeId::Resource {
                    resource_type: "file".into(),
                    name: "config".into(),
                },
                &EdgeKind::Reference,
            ),
            1
        );
    }

    #[test]
    fn finds_references_in_nested_array_and_object_values() {
        let mut nested = BTreeMap::new();
        nested.insert(
            "inner".into(),
            attribute("inner", reference(&["parameter", "content"])),
        );
        let mut config = config();
        config.resources[0].attributes = attrs(vec![
            (
                "array",
                Spanned::synthetic(ExprKind::Literal(Value::Array(vec![reference(&[
                    "data", "script", "setup",
                ])]))),
            ),
            (
                "object",
                Spanned::synthetic(ExprKind::Literal(Value::Object(nested))),
            ),
        ]);

        let graph = DesiredGraphBuilder::build(&config).unwrap();

        assert!(graph.has_edge(
            &NodeId::Data {
                data_type: "script".into(),
                name: "setup".into(),
            },
            &NodeId::Resource {
                resource_type: "file".into(),
                name: "config".into(),
            },
            &EdgeKind::Reference,
        ));
        assert!(graph.has_edge(
            &NodeId::Parameter {
                name: "content".into()
            },
            &NodeId::Resource {
                resource_type: "file".into(),
                name: "config".into(),
            },
            &EdgeKind::Reference,
        ));
    }

    #[test]
    fn unknown_reference_errors() {
        let mut config = config();
        config.resources[0].attributes =
            attrs(vec![("content", reference(&["parameter", "missing"]))]);

        let errors = DesiredGraphBuilder::build(&config).unwrap_err();

        assert!(matches!(
            errors[0],
            GraphBuildError::UnknownReference { .. }
        ));
    }

    #[test]
    fn malformed_reference_errors() {
        let mut config = config();
        config.resources[0].attributes = attrs(vec![("content", reference(&["resource", "file"]))]);

        let errors = DesiredGraphBuilder::build(&config).unwrap_err();

        assert!(matches!(
            errors[0],
            GraphBuildError::MalformedReference { .. }
        ));
    }

    #[test]
    fn formats_node_ids() {
        assert_eq!(
            NodeId::Provider {
                name: "blue".into()
            }
            .to_string(),
            "provider.blue"
        );
        assert_eq!(
            NodeId::Parameter {
                name: "content".into()
            }
            .to_string(),
            "parameter.content"
        );
        assert_eq!(
            NodeId::Data {
                data_type: "script".into(),
                name: "setup".into(),
            }
            .to_string(),
            "data.script.setup"
        );
        assert_eq!(
            NodeId::Resource {
                resource_type: "file".into(),
                name: "config".into(),
            }
            .to_string(),
            "resource.file.config"
        );
    }

    #[test]
    fn formats_graph_build_errors() {
        assert_eq!(
            GraphBuildError::MalformedReference {
                path: vec!["resource".into(), "file".into()],
                span: SourceSpan::SYNTHETIC,
            }
            .to_string(),
            "malformed reference `resource.file`"
        );
        assert_eq!(
            GraphBuildError::UnknownReference {
                path: vec!["parameter".into(), "missing".into()],
                span: SourceSpan::SYNTHETIC,
            }
            .to_string(),
            "unknown reference `parameter.missing`"
        );
        assert_eq!(
            GraphBuildError::Cycle {
                path: vec![
                    NodeId::Resource {
                        resource_type: "file".into(),
                        name: "a".into(),
                    },
                    NodeId::Resource {
                        resource_type: "file".into(),
                        name: "b".into(),
                    },
                    NodeId::Resource {
                        resource_type: "file".into(),
                        name: "a".into(),
                    },
                ],
            }
            .to_string(),
            "dependency cycle detected: resource.file.a -> resource.file.b -> resource.file.a"
        );
    }

    #[test]
    fn topological_order_places_dependencies_before_dependents() {
        let mut config = config();
        config.resources[0].attributes =
            attrs(vec![("script", reference(&["data", "script", "setup"]))]);
        config.data[0].attributes = attrs(vec![("content", reference(&["parameter", "content"]))]);

        let graph = DesiredGraphBuilder::build(&config).unwrap();
        let order = graph.topological_order().unwrap();

        let provider = order
            .iter()
            .position(|id| {
                id == &NodeId::Provider {
                    name: "blue".into(),
                }
            })
            .unwrap();
        let parameter = order
            .iter()
            .position(|id| {
                id == &NodeId::Parameter {
                    name: "content".into(),
                }
            })
            .unwrap();
        let data = order
            .iter()
            .position(|id| {
                id == &NodeId::Data {
                    data_type: "script".into(),
                    name: "setup".into(),
                }
            })
            .unwrap();
        let resource = order
            .iter()
            .position(|id| {
                id == &NodeId::Resource {
                    resource_type: "file".into(),
                    name: "config".into(),
                }
            })
            .unwrap();

        assert!(provider < data);
        assert!(provider < resource);
        assert!(parameter < data);
        assert!(data < resource);
    }

    #[test]
    fn topological_order_returns_cycle_path() {
        let mut graph = DesiredGraph::new();
        let a = NodeId::Resource {
            resource_type: "file".into(),
            name: "a".into(),
        };
        let b = NodeId::Resource {
            resource_type: "file".into(),
            name: "b".into(),
        };
        graph.add_node(GraphNode {
            id: a.clone(),
            kind: GraphNodeKind::Resource {
                provider: "blue".into(),
            },
        });
        graph.add_node(GraphNode {
            id: b.clone(),
            kind: GraphNodeKind::Resource {
                provider: "blue".into(),
            },
        });
        graph.add_edge(&a, &b, EdgeKind::Reference);
        graph.add_edge(&b, &a, EdgeKind::Reference);

        let cycle = graph.topological_order().unwrap_err();

        assert_eq!(cycle.first(), cycle.last());
        assert!(cycle.contains(&a));
        assert!(cycle.contains(&b));
    }

    #[test]
    fn acyclic_graph_succeeds() {
        let mut config = config();
        config.resources.push(Resource {
            provider: spanned("blue"),
            resource_type: spanned("file"),
            name: spanned("other"),
            attributes: attrs(vec![("path", reference(&["resource", "file", "config"]))]),
            span: SourceSpan::SYNTHETIC,
        });

        assert!(DesiredGraphBuilder::build(&config).is_ok());
    }

    #[test]
    fn cycle_errors_with_full_path() {
        let mut config = config();
        config.resources[0].attributes =
            attrs(vec![("path", reference(&["resource", "file", "other"]))]);
        config.resources.push(Resource {
            provider: spanned("blue"),
            resource_type: spanned("file"),
            name: spanned("other"),
            attributes: attrs(vec![("path", reference(&["resource", "file", "config"]))]),
            span: SourceSpan::SYNTHETIC,
        });

        let errors = DesiredGraphBuilder::build(&config).unwrap_err();
        let cycle = errors
            .iter()
            .find_map(|error| match error {
                GraphBuildError::Cycle { path } => Some(path),
                _ => None,
            })
            .expect("expected cycle error");

        assert_eq!(cycle.first(), cycle.last());
        assert!(cycle.contains(&NodeId::Resource {
            resource_type: "file".into(),
            name: "config".into(),
        }));
        assert!(cycle.contains(&NodeId::Resource {
            resource_type: "file".into(),
            name: "other".into(),
        }));
        assert_eq!(cycle.len(), 3);
    }

    #[test]
    fn reference_span_covers_reference_path() {
        let reference = Reference {
            path: vec![
                Spanned::new("parameter".into(), SourceSpan::new(SourceId(0), 10, 19)),
                Spanned::new("content".into(), SourceSpan::new(SourceId(0), 20, 27)),
            ],
        };

        assert_eq!(
            reference_span(&reference),
            SourceSpan::new(SourceId(0), 10, 27)
        );
    }
}
