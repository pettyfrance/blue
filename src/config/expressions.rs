use std::error::Error;
use std::fmt;

use super::model::{Expr, ExprKind, Reference, TemplatePart, Value};
use super::source::{SourceId, SourceSpan, Spanned};

#[derive(Debug, Clone, PartialEq)]
pub enum ExpressionError {
    UnclosedExpression,
    UnexpectedClosingBraces,
    EmptyExpression,
    InvalidReference(String),
}

impl fmt::Display for ExpressionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnclosedExpression => {
                write!(f, "expression is missing closing braces `}}}}`")
            }
            Self::UnexpectedClosingBraces => {
                write!(f, "unexpected closing braces `}}}}`")
            }
            Self::EmptyExpression => {
                write!(f, "expression cannot be empty")
            }
            Self::InvalidReference(reference) => {
                write!(f, "invalid reference `{reference}`")
            }
        }
    }
}

impl Error for ExpressionError {}

pub trait ExpressionSpanMapper {
    fn span(&self, start: usize, end: usize) -> SourceSpan;
}

#[derive(Debug, Clone, Copy)]
pub struct SimpleExpressionSpanMapper {
    pub source_id: SourceId,
    pub base_offset: usize,
}

impl SimpleExpressionSpanMapper {
    pub fn new(source_id: SourceId, base_offset: usize) -> Self {
        Self {
            source_id,
            base_offset,
        }
    }
}

impl ExpressionSpanMapper for SimpleExpressionSpanMapper {
    fn span(&self, start: usize, end: usize) -> SourceSpan {
        SourceSpan::new(self.source_id, self.base_offset + start, self.base_offset + end)
    }
}

// Used by parse_string to find the end of an expression,
// taking into account escaped closing braces.
fn find_expression_end(input: &str, start: usize) -> Option<usize> {
    let mut position = start;

    while position < input.len() {
        let remaining = &input[position..];

        if remaining.starts_with(r"\}}") {
            position += 3;
            continue;
        }

        if remaining.starts_with("}}") {
            return Some(position);
        }

        let ch = remaining.chars().next()?;
        position += ch.len_utf8();
    }

    None
}

pub fn parse_string(
    input: &str,
    span_mapper: &impl ExpressionSpanMapper,
) -> Result<Expr, ExpressionError> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut literal_start = None;
    let mut position = 0;

    while position < input.len() {
        let remaining = &input[position..];

        if remaining.starts_with(r"\{{") {
            literal_start.get_or_insert(position);
            literal.push_str("{{");
            position += 3;
            continue;
        }

        if remaining.starts_with(r"\}}") {
            literal_start.get_or_insert(position);
            literal.push_str("}}");
            position += 3;
            continue;
        }

        if remaining.starts_with("{{") {
            if !literal.is_empty() {
                let start = literal_start.take().expect("literal start is set");
                parts.push(TemplatePart::Literal(Spanned::new(
                    std::mem::take(&mut literal),
                    span_mapper.span(start, position),
                )));
            }

            let expression_start = position + 2;

            let Some(end) = find_expression_end(input, expression_start) else {
                return Err(ExpressionError::UnclosedExpression);
            };

            let raw_expression = &input[expression_start..end];
            let leading_whitespace = raw_expression.len() - raw_expression.trim_start().len();
            let expression = raw_expression.trim();

            if expression.is_empty() {
                return Err(ExpressionError::EmptyExpression);
            }

            let reference = parse_reference(
                span_mapper,
                expression,
                expression_start + leading_whitespace,
            )?;

            parts.push(TemplatePart::Expr(Box::new(Spanned::new(
                ExprKind::Reference(reference),
                span_mapper.span(position, end + 2),
            ))));

            position = end + 2;
            continue;
        }

        if remaining.starts_with("}}") {
            return Err(ExpressionError::UnexpectedClosingBraces);
        }

        let ch = remaining.chars().next().expect("position is inside input");
        literal_start.get_or_insert(position);
        literal.push(ch);
        position += ch.len_utf8();
    }

    if !literal.is_empty() {
        let start = literal_start.take().expect("literal start is set");
        parts.push(TemplatePart::Literal(Spanned::new(
            literal,
            span_mapper.span(start, input.len()),
        )));
    }

    if parts.is_empty() {
        return Ok(Spanned::new(
            ExprKind::Literal(Value::String(String::new())),
            span_mapper.span(0, 0),
        ));
    }

    if parts.len() == 1 {
        match parts.remove(0) {
            TemplatePart::Expr(expr) => Ok(*expr),
            TemplatePart::Literal(value) => Ok(value.map(|value| ExprKind::Literal(Value::String(value)))),
        }
    } else {
        Ok(Spanned::new(
            ExprKind::Template(parts),
            span_mapper.span(0, input.len()),
        ))
    }
}

fn parse_reference(
    span_mapper: &impl ExpressionSpanMapper,
    input: &str,
    expression_offset: usize,
) -> Result<Reference, ExpressionError> {
    let mut path = Vec::new();
    let mut offset = 0;

    for segment in input.split('.') {
        let trimmed = segment.trim();
        let leading_whitespace = segment.len() - segment.trim_start().len();
        let segment_start = offset + leading_whitespace;
        let segment_end = segment_start + trimmed.len();

        path.push(Spanned::new(
            trimmed.to_string(),
            span_mapper.span(
                expression_offset + segment_start,
                expression_offset + segment_end,
            ),
        ));

        offset += segment.len() + 1;
    }

    if path.len() < 2 {
        return Err(ExpressionError::InvalidReference(input.to_owned()));
    }

    if path
        .iter()
        .any(|segment| segment.value.is_empty() || !is_valid_identifier(&segment.value))
    {
        return Err(ExpressionError::InvalidReference(input.to_owned()));
    }

    Ok(Reference { path })
}

fn is_valid_identifier(value: &str) -> bool {
    let mut chars = value.chars();

    let Some(first) = chars.next() else {
        return false;
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Result<Expr, ExpressionError> {
        parse_string(
            input,
            &SimpleExpressionSpanMapper::new(SourceId::SYNTHETIC, 0),
        )
    }

    fn parse_normalized(input: &str) -> Result<Expr, ExpressionError> {
        parse(input).map(normalize_expr)
    }

    fn normalize_expr(expr: Expr) -> Expr {
        let value = match expr.value {
            ExprKind::Literal(value) => ExprKind::Literal(normalize_value(value)),
            ExprKind::Reference(reference) => ExprKind::Reference(normalize_reference(reference)),
            ExprKind::Template(parts) => ExprKind::Template(
                parts
                    .into_iter()
                    .map(normalize_template_part)
                    .collect(),
            ),
        };

        Spanned::synthetic(value)
    }

    fn normalize_value(value: Value) -> Value {
        match value {
            Value::Array(values) => Value::Array(values.into_iter().map(normalize_expr).collect()),
            other => other,
        }
    }

    fn normalize_reference(reference: Reference) -> Reference {
        Reference {
            path: reference
                .path
                .into_iter()
                .map(|segment| Spanned::synthetic(segment.value))
                .collect(),
        }
    }

    fn normalize_template_part(part: TemplatePart) -> TemplatePart {
        match part {
            TemplatePart::Literal(value) => TemplatePart::Literal(Spanned::synthetic(value.value)),
            TemplatePart::Expr(expr) => TemplatePart::Expr(Box::new(normalize_expr(*expr))),
        }
    }

    fn literal_string(value: &str) -> Expr {
        Spanned::synthetic(ExprKind::Literal(Value::String(value.into())))
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

    fn literal_part(value: &str) -> TemplatePart {
        TemplatePart::Literal(Spanned::synthetic(value.to_string()))
    }

    #[test]
    fn parses_literal_string() {
        assert_eq!(parse_normalized("large").unwrap(), literal_string("large"));
    }

    #[test]
    fn parses_empty_string() {
        assert_eq!(parse_normalized("").unwrap(), literal_string(""));
    }

    #[test]
    fn parses_reference() {
        assert_eq!(
            parse_normalized("{{ parameter.db_size }}").unwrap(),
            reference(&["parameter", "db_size"])
        );
    }

    #[test]
    fn parses_unknown_namespace_as_reference() {
        assert_eq!(
            parse_normalized("{{ potato.foo }}").unwrap(),
            reference(&["potato", "foo"])
        );
    }

    #[test]
    fn parses_long_reference_path() {
        assert_eq!(
            parse_normalized("{{ resource.database.main.id }}").unwrap(),
            reference(&["resource", "database", "main", "id"])
        );
    }

    #[test]
    fn parses_interpolation() {
        assert_eq!(
            parse_normalized("db-{{ parameter.environment }}-primary").unwrap(),
            template(vec![
                literal_part("db-"),
                TemplatePart::Expr(Box::new(reference(&["parameter", "environment"]))),
                literal_part("-primary"),
            ])
        );
    }

    #[test]
    fn parses_multiple_interpolations() {
        assert_eq!(
            parse_normalized("{{ parameter.environment }}-{{ parameter.region }}").unwrap(),
            template(vec![
                TemplatePart::Expr(Box::new(reference(&["parameter", "environment"]))),
                literal_part("-"),
                TemplatePart::Expr(Box::new(reference(&["parameter", "region"]))),
            ])
        );
    }

    #[test]
    fn parses_prefix_interpolation() {
        assert_eq!(
            parse_normalized("{{ parameter.name }}-suffix").unwrap(),
            template(vec![
                TemplatePart::Expr(Box::new(reference(&["parameter", "name"]))),
                literal_part("-suffix"),
            ])
        );
    }

    #[test]
    fn parses_suffix_interpolation() {
        assert_eq!(
            parse_normalized("prefix-{{ parameter.name }}").unwrap(),
            template(vec![
                literal_part("prefix-"),
                TemplatePart::Expr(Box::new(reference(&["parameter", "name"]))),
            ])
        );
    }

    #[test]
    fn parses_escaped_opening_braces_as_literal() {
        assert_eq!(
            parse_normalized(r"\{{ not an expression \}}").unwrap(),
            literal_string("{{ not an expression }}")
        );
    }

    #[test]
    fn parses_escaped_closing_braces_as_literal() {
        assert_eq!(
            parse_normalized(r"not an expression \}}").unwrap(),
            literal_string("not an expression }}")
        );
    }

    #[test]
    fn parses_escaped_braces_inside_literal_text() {
        assert_eq!(
            parse_normalized(r"hello \{{ name \}}").unwrap(),
            literal_string("hello {{ name }}")
        );
    }

    #[test]
    fn parses_template_with_escaped_braces_and_expression() {
        assert_eq!(
            parse_normalized(r"literal \{{ and {{ parameter.name }}").unwrap(),
            template(vec![
                literal_part("literal {{ and "),
                TemplatePart::Expr(Box::new(reference(&["parameter", "name"]))),
            ])
        );
    }

    #[test]
    fn keeps_unrelated_backslashes() {
        assert_eq!(
            parse_normalized(r"path\to\file").unwrap(),
            literal_string(r"path\to\file")
        );
    }

    #[test]
    fn emits_absolute_span_for_literal_string() {
        let source_id = SourceId(7);
        let mapper = SimpleExpressionSpanMapper::new(source_id, 100);
        let expr = parse_string("large", &mapper).unwrap();

        assert_eq!(expr.span, SourceSpan::new(source_id, 100, 105));
    }

    #[test]
    fn emits_absolute_spans_for_reference_expression_and_segments() {
        let source_id = SourceId(7);
        let mapper = SimpleExpressionSpanMapper::new(source_id, 100);
        let expr = parse_string("{{ parameter.db_size }}", &mapper).unwrap();

        assert_eq!(expr.span, SourceSpan::new(source_id, 100, 123));

        let ExprKind::Reference(reference) = expr.value else {
            panic!("expected reference");
        };

        assert_eq!(reference.path[0].span, SourceSpan::new(source_id, 103, 112));
        assert_eq!(reference.path[1].span, SourceSpan::new(source_id, 113, 120));
    }

    #[test]
    fn emits_absolute_spans_for_template_literal_parts() {
        let source_id = SourceId(7);
        let mapper = SimpleExpressionSpanMapper::new(source_id, 100);
        let expr = parse_string("db-{{ parameter.environment }}-primary", &mapper).unwrap();

        assert_eq!(expr.span, SourceSpan::new(source_id, 100, 138));

        let ExprKind::Template(parts) = expr.value else {
            panic!("expected template");
        };

        let TemplatePart::Literal(prefix) = &parts[0] else {
            panic!("expected prefix literal");
        };

        assert_eq!(prefix.span, SourceSpan::new(source_id, 100, 103));

        let TemplatePart::Expr(expr) = &parts[1] else {
            panic!("expected expression part");
        };

        assert_eq!(expr.span, SourceSpan::new(source_id, 103, 130));

        let TemplatePart::Literal(suffix) = &parts[2] else {
            panic!("expected suffix literal");
        };

        assert_eq!(suffix.span, SourceSpan::new(source_id, 130, 138));
    }

    #[test]
    fn emits_literal_span_covering_escaped_source_range() {
        let source_id = SourceId(7);
        let mapper = SimpleExpressionSpanMapper::new(source_id, 100);
        let expr = parse_string(r"\{{ literal \}}", &mapper).unwrap();

        assert_eq!(expr.span, SourceSpan::new(source_id, 100, 115));
    }

    #[test]
    fn rejects_unclosed_expression() {
        assert_eq!(
            parse("db-{{ parameter.environment").unwrap_err(),
            ExpressionError::UnclosedExpression
        );
    }

    #[test]
    fn rejects_unexpected_closing_braces() {
        assert_eq!(
            parse("hello }}").unwrap_err(),
            ExpressionError::UnexpectedClosingBraces
        );
    }

    #[test]
    fn rejects_empty_expression() {
        assert_eq!(
            parse("{{ }}").unwrap_err(),
            ExpressionError::EmptyExpression
        );
    }

    #[test]
    fn rejects_invalid_reference() {
        assert_eq!(
            parse("{{ parameter..size }}").unwrap_err(),
            ExpressionError::InvalidReference("parameter..size".into())
        );
    }

    #[test]
    fn rejects_identifier_starting_with_digit() {
        assert_eq!(
            parse("{{ parameter.123 }}").unwrap_err(),
            ExpressionError::InvalidReference("parameter.123".into())
        );
    }

    #[test]
    fn rejects_single_segment_reference() {
        assert_eq!(
            parse("{{ parameter }}").unwrap_err(),
            ExpressionError::InvalidReference("parameter".into())
        );
    }

    #[test]
    fn rejects_unclosed_expression_after_escaped_braces() {
        assert_eq!(
            parse(r"\{{ literal \}} and {{ parameter.foo").unwrap_err(),
            ExpressionError::UnclosedExpression
        );
    }
}
