use std::collections::BTreeMap;

use super::semantic::{Config, Data, ProviderConfig, Resource};
use super::{Parameter, SourceSpan};
use crate::graph::NodeId;

#[derive(Debug)]
pub struct ConfigIndex<'a> {
    providers: BTreeMap<String, &'a ProviderConfig>,
    parameters: BTreeMap<String, &'a Parameter>,
    data: BTreeMap<(String, String), &'a Data>,
    resources: BTreeMap<(String, String), &'a Resource>,
}

impl<'a> ConfigIndex<'a> {
    pub fn new(config: &'a Config) -> Self {
        let providers = config
            .providers
            .iter()
            .map(|provider| (provider.name.value.clone(), provider))
            .collect();
        let parameters = config
            .parameters
            .iter()
            .map(|parameter| (parameter.name.value.clone(), parameter))
            .collect();
        let data = config
            .data
            .iter()
            .map(|data| {
                (
                    (data.data_type.value.clone(), data.name.value.clone()),
                    data,
                )
            })
            .collect();
        let resources = config
            .resources
            .iter()
            .map(|resource| {
                (
                    (
                        resource.resource_type.value.clone(),
                        resource.name.value.clone(),
                    ),
                    resource,
                )
            })
            .collect();

        Self {
            providers,
            parameters,
            data,
            resources,
        }
    }

    pub fn provider(&self, id: &NodeId) -> Option<&'a ProviderConfig> {
        let NodeId::Provider { name } = id else {
            return None;
        };

        self.providers.get(name).copied()
    }

    pub fn parameter(&self, id: &NodeId) -> Option<&'a Parameter> {
        let NodeId::Parameter { name } = id else {
            return None;
        };

        self.parameters.get(name).copied()
    }

    pub fn data(&self, id: &NodeId) -> Option<&'a Data> {
        let NodeId::Data { data_type, name } = id else {
            return None;
        };

        self.data.get(&(data_type.clone(), name.clone())).copied()
    }

    pub fn resource(&self, id: &NodeId) -> Option<&'a Resource> {
        let NodeId::Resource {
            resource_type,
            name,
        } = id
        else {
            return None;
        };

        self.resources
            .get(&(resource_type.clone(), name.clone()))
            .copied()
    }

    pub fn span(&self, id: &NodeId) -> Option<SourceSpan> {
        match id {
            NodeId::Provider { .. } => self.provider(id).map(|provider| provider.span),
            NodeId::Parameter { .. } => self.parameter(id).map(|parameter| parameter.span),
            NodeId::Data { .. } => self.data(id).map(|data| data.span),
            NodeId::Resource { .. } => self.resource(id).map(|resource| resource.span),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SourceId, Spanned};
    use std::collections::BTreeMap;

    fn spanned(value: &str) -> Spanned<String> {
        Spanned::synthetic(value.to_string())
    }

    fn span(start: usize, end: usize) -> SourceSpan {
        SourceSpan::new(SourceId(0), start, end)
    }

    fn config() -> Config {
        Config {
            providers: vec![ProviderConfig {
                name: spanned("blue"),
                provider_type: spanned("blue"),
                source: None,
                default: None,
                attributes: BTreeMap::new(),
                span: span(0, 10),
            }],
            parameters: vec![Parameter {
                name: spanned("content"),
                attributes: BTreeMap::new(),
                span: span(10, 20),
            }],
            data: vec![Data {
                provider: spanned("blue"),
                data_type: spanned("script"),
                name: spanned("setup"),
                attributes: BTreeMap::new(),
                span: span(20, 30),
            }],
            resources: vec![Resource {
                provider: spanned("blue"),
                resource_type: spanned("file"),
                name: spanned("config"),
                attributes: BTreeMap::new(),
                span: span(30, 40),
            }],
        }
    }

    #[test]
    fn looks_up_declarations_by_node_id() {
        let config = config();
        let index = ConfigIndex::new(&config);

        assert_eq!(
            index
                .provider(&NodeId::Provider {
                    name: "blue".into()
                })
                .unwrap()
                .name
                .value,
            "blue"
        );
        assert_eq!(
            index
                .parameter(&NodeId::Parameter {
                    name: "content".into()
                })
                .unwrap()
                .name
                .value,
            "content"
        );
        assert_eq!(
            index
                .data(&NodeId::Data {
                    data_type: "script".into(),
                    name: "setup".into(),
                })
                .unwrap()
                .name
                .value,
            "setup"
        );
        assert_eq!(
            index
                .resource(&NodeId::Resource {
                    resource_type: "file".into(),
                    name: "config".into(),
                })
                .unwrap()
                .name
                .value,
            "config"
        );
    }

    #[test]
    fn looks_up_spans_by_node_id() {
        let config = config();
        let index = ConfigIndex::new(&config);

        assert_eq!(
            index.span(&NodeId::Provider {
                name: "blue".into()
            }),
            Some(span(0, 10))
        );
        assert_eq!(
            index.span(&NodeId::Parameter {
                name: "content".into()
            }),
            Some(span(10, 20))
        );
        assert_eq!(
            index.span(&NodeId::Data {
                data_type: "script".into(),
                name: "setup".into(),
            }),
            Some(span(20, 30))
        );
        assert_eq!(
            index.span(&NodeId::Resource {
                resource_type: "file".into(),
                name: "config".into(),
            }),
            Some(span(30, 40))
        );
    }

    #[test]
    fn returns_none_for_wrong_node_kind() {
        let config = config();
        let index = ConfigIndex::new(&config);

        assert!(
            index
                .resource(&NodeId::Parameter {
                    name: "content".into()
                })
                .is_none()
        );
    }
}
