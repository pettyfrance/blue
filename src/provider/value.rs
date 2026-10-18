use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProviderValue {
    Null,
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<ProviderValue>),
    Object(BTreeMap<String, ProviderValue>),
}

impl ProviderValue {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::String(_) => "string",
            Self::Integer(_) => "integer",
            Self::Float(_) => "float",
            Self::Bool(_) => "bool",
            Self::Array(_) => "array",
            Self::Object(_) => "object",
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(value) => Some(*value),
            Self::Integer(value) => Some(*value as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[ProviderValue]> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, ProviderValue>> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }
}

impl fmt::Display for ProviderValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::String(value) => write!(f, "{value}"),
            Self::Integer(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value}"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Array(values) => {
                write!(f, "[")?;
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{value}")?;
                }
                write!(f, "]")
            }
            Self::Object(values) => {
                write!(f, "{{")?;
                for (index, (key, value)) in values.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{key}: {value}")?;
                }
                write!(f, "}}")
            }
        }
    }
}

impl From<String> for ProviderValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for ProviderValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<i64> for ProviderValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<i32> for ProviderValue {
    fn from(value: i32) -> Self {
        Self::Integer(value.into())
    }
}

impl From<f64> for ProviderValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<bool> for ProviderValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<Vec<ProviderValue>> for ProviderValue {
    fn from(value: Vec<ProviderValue>) -> Self {
        Self::Array(value)
    }
}

impl From<BTreeMap<String, ProviderValue>> for ProviderValue {
    fn from(value: BTreeMap<String, ProviderValue>) -> Self {
        Self::Object(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_type_names() {
        assert_eq!(ProviderValue::Null.type_name(), "null");
        assert_eq!(ProviderValue::from("hello").type_name(), "string");
        assert_eq!(ProviderValue::from(1_i64).type_name(), "integer");
        assert_eq!(ProviderValue::from(1.5_f64).type_name(), "float");
        assert_eq!(ProviderValue::from(true).type_name(), "bool");
        assert_eq!(ProviderValue::from(Vec::new()).type_name(), "array");
        assert_eq!(ProviderValue::from(BTreeMap::new()).type_name(), "object");
    }

    #[test]
    fn exposes_typed_accessors() {
        assert_eq!(ProviderValue::from("hello").as_str(), Some("hello"));
        assert_eq!(ProviderValue::from(42_i64).as_i64(), Some(42));
        assert_eq!(ProviderValue::from(42_i64).as_f64(), Some(42.0));
        assert_eq!(ProviderValue::from(1.5_f64).as_f64(), Some(1.5));
        assert_eq!(ProviderValue::from(true).as_bool(), Some(true));

        let array = ProviderValue::Array(vec![ProviderValue::from("hello")]);
        assert_eq!(array.as_array().unwrap()[0].as_str(), Some("hello"));

        let object = ProviderValue::Object(BTreeMap::from([(
            "name".into(),
            ProviderValue::from("example"),
        )]));
        assert_eq!(
            object.as_object().unwrap()["name"].as_str(),
            Some("example")
        );
    }

    #[test]
    fn displays_values() {
        assert_eq!(ProviderValue::Null.to_string(), "null");
        assert_eq!(ProviderValue::from("hello").to_string(), "hello");
        assert_eq!(ProviderValue::from(42_i64).to_string(), "42");
        assert_eq!(ProviderValue::from(true).to_string(), "true");
        assert_eq!(
            ProviderValue::Array(vec![ProviderValue::from("a"), ProviderValue::from("b")])
                .to_string(),
            "[a, b]"
        );
        assert_eq!(
            ProviderValue::Object(BTreeMap::from([(
                "name".into(),
                ProviderValue::from("example"),
            )]))
            .to_string(),
            "{name: example}"
        );
    }
}
