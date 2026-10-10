use super::model::{DesiredGraph, EdgeKind, GraphNode, GraphNodeKind, NodeId};
use crate::config::{Attributes, Config, Reference, SourceSpan};

pub struct DesiredGraphBuilder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphBuildError {
    MalformedReference { path: Vec<String>, span: SourceSpan },
    UnknownReference { path: Vec<String>, span: SourceSpan },
}

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
