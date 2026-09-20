use std::collections::BTreeMap;

use super::source::{SourceSpan, Spanned};

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedConfig {
    pub providers: Vec<ParsedProviderConfig>,
    pub parameters: Vec<Parameter>,
    pub resources: Vec<ParsedResource>,
    pub data: Vec<ParsedData>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedProviderConfig {
    pub name: Spanned<String>,
    pub provider_type: Spanned<String>,
    pub source: Option<Spanned<String>>,
    pub default: Option<Spanned<bool>>,
    pub attributes: Attributes,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: Spanned<String>,
    pub attributes: Attributes,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedResource {
    pub provider: Option<Spanned<String>>,
    pub resource_type: Spanned<String>,
    pub name: Spanned<String>,
    pub attributes: Attributes,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedData {
    pub provider: Option<Spanned<String>>,
    pub data_type: Spanned<String>,
    pub name: Spanned<String>,
    pub attributes: Attributes,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    pub path: Vec<Spanned<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: Spanned<String>,
    pub value: Expr,
    pub span: SourceSpan,
}

pub type Expr = Spanned<ExprKind>;

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Literal(Value),
    Reference(Reference),
    Template(Vec<TemplatePart>),
    // Binary {
    //     lhs: Box<Expr>,
    //     op: BinaryOp,
    //     rhs: Box<Expr>,
    // },
    // List(Vec<Expr>),
    // Object(HashMap<String, Expr>),
}

impl Expr {
    pub fn references(&self) -> Vec<&Reference> {
        let mut references = Vec::new();
        self.collect_references(&mut references);
        references
    }

    fn collect_references<'a>(&'a self, references: &mut Vec<&'a Reference>) {
        match &self.value {
            ExprKind::Literal(value) => {
                value.collect_references(references);
            }

            ExprKind::Reference(reference) => {
                references.push(reference);
            }

            ExprKind::Template(parts) => {
                for part in parts {
                    part.collect_references(references);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePart {
    Literal(Spanned<String>),
    Expr(Box<Expr>),
}

impl TemplatePart {
    fn collect_references<'a>(&'a self, references: &mut Vec<&'a Reference>) {
        match self {
            TemplatePart::Literal(_) => {}

            TemplatePart::Expr(expr) => {
                expr.collect_references(references);
            }
        }
    }
}

/// Concrete literal values.
///
/// Scalar values are not individually spanned because the enclosing `Expr`
/// carries the source span for the literal. For example, an integer literal is
/// represented as:
///
/// `Expr { value: ExprKind::Literal(Value::Integer(...)), span: ... }`
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<Expr>),
    Object(Attributes),
}

impl Value {
    fn collect_references<'a>(&'a self, references: &mut Vec<&'a Reference>) {
        match self {
            Value::String(_) | Value::Integer(_) | Value::Float(_) | Value::Bool(_) => {}

            Value::Array(values) => {
                for value in values {
                    value.collect_references(references);
                }
            }

            Value::Object(values) => {
                for attribute in values.values() {
                    attribute.value.collect_references(references);
                }
            }
        }
    }
}

pub type Attributes = BTreeMap<String, Attribute>;

#[cfg(test)]
mod tests {
    use super::*;

    fn literal(value: Value) -> Expr {
        Spanned::synthetic(ExprKind::Literal(value))
    }

    fn reference(path: &[&str]) -> Expr {
        Spanned::synthetic(ExprKind::Reference(Reference {
            path: path
                .iter()
                .map(|segment| Spanned::synthetic(segment.to_string()))
                .collect(),
        }))
    }

    fn template(parts: Vec<TemplatePart>) -> Expr {
        Spanned::synthetic(ExprKind::Template(parts))
    }

    fn attribute(name: &str, value: Expr) -> Attribute {
        Attribute {
            name: Spanned::synthetic(name.to_string()),
            value,
            span: SourceSpan::SYNTHETIC,
        }
    }

    fn literal_part(value: &str) -> TemplatePart {
        TemplatePart::Literal(Spanned::synthetic(value.to_string()))
    }

    fn reference_paths(expr: &Expr) -> Vec<Vec<String>> {
        expr.references()
            .into_iter()
            .map(|reference| {
                reference
                    .path
                    .iter()
                    .map(|segment| segment.value.clone())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn literal_string_has_no_references() {
        let expr = literal(Value::String("hello".into()));

        assert!(expr.references().is_empty());
    }

    #[test]
    fn collects_direct_reference() {
        let expr = reference(&["parameter", "db_size"]);

        assert_eq!(
            reference_paths(&expr),
            vec![vec![String::from("parameter"), String::from("db_size")]]
        );
    }

    #[test]
    fn collects_reference_from_template() {
        let expr = template(vec![
            literal_part("db-"),
            TemplatePart::Expr(Box::new(reference(&["parameter", "environment"]))),
        ]);

        assert_eq!(
            reference_paths(&expr),
            vec![vec![String::from("parameter"), String::from("environment")]]
        );
    }

    #[test]
    fn collects_reference_from_array() {
        let expr = literal(Value::Array(vec![
            literal(Value::Integer(8080)),
            reference(&["parameter", "admin_port"]),
        ]));

        assert_eq!(
            reference_paths(&expr),
            vec![vec![String::from("parameter"), String::from("admin_port")]]
        );
    }

    #[test]
    fn collects_reference_from_object() {
        let mut values = BTreeMap::new();
        values.insert(
            "app".into(),
            attribute("app", literal(Value::String("web".into()))),
        );
        values.insert(
            "env".into(),
            attribute("env", reference(&["parameter", "environment"])),
        );

        let expr = literal(Value::Object(values));

        assert_eq!(
            reference_paths(&expr),
            vec![vec![String::from("parameter"), String::from("environment")]]
        );
    }

    #[test]
    fn collects_references_from_deeply_nested_expression() {
        let mut object = BTreeMap::new();
        object.insert(
            "name".into(),
            attribute(
                "name",
                template(vec![
                    literal_part("server-"),
                    TemplatePart::Expr(Box::new(reference(&["parameter", "environment"]))),
                ]),
            ),
        );
        object.insert(
            "ports".into(),
            attribute(
                "ports",
                literal(Value::Array(vec![
                    literal(Value::Integer(8080)),
                    reference(&["parameter", "admin_port"]),
                ])),
            ),
        );

        let expr = literal(Value::Object(object));

        assert_eq!(
            reference_paths(&expr),
            vec![
                vec![String::from("parameter"), String::from("environment")],
                vec![String::from("parameter"), String::from("admin_port")],
            ]
        );
    }
}
