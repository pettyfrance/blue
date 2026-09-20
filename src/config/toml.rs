use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::ops::Range;

use super::expressions::{self, ExpressionError, ExpressionSpanMapper};
use super::model::{Attribute, Attributes, Config, Data, Expr, ExprKind, Parameter, Resource, Value};
use super::reader::ConfigReader;
use super::source::{SourceId, SourceSpan, Spanned};

pub struct TomlReader;

#[derive(Debug)]
pub enum TomlReaderError {
    Syntax(String),
    Expression(ExpressionError),
    InvalidSection { section: String, message: String },
    UnknownTopLevelSection(String),
    UnsupportedValue(String),
    UnexpectedEof,
    UnexpectedEvent(String),
    MissingSpan(String),
}

impl fmt::Display for TomlReaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(err) => write!(f, "failed to parse TOML syntax: {err}"),
            Self::Expression(err) => write!(f, "failed to parse expression: {err}"),
            Self::InvalidSection { section, message } => {
                write!(f, "invalid `{section}` section: {message}")
            }
            Self::UnknownTopLevelSection(section) => {
                write!(f, "unknown top-level section `{section}`")
            }
            Self::UnsupportedValue(value) => write!(f, "unsupported TOML value: {value}"),
            Self::UnexpectedEof => write!(f, "unexpected end of TOML input"),
            Self::UnexpectedEvent(event) => write!(f, "unexpected TOML event: {event}"),
            Self::MissingSpan(value) => write!(f, "missing TOML source span for {value}"),
        }
    }
}

impl Error for TomlReaderError {}

impl From<ExpressionError> for TomlReaderError {
    fn from(value: ExpressionError) -> Self {
        Self::Expression(value)
    }
}

impl ConfigReader for TomlReader {
    type Error = TomlReaderError;

    fn read(&self, source_id: SourceId, input: &str) -> Result<Config, Self::Error> {
        let document = parse_toml_syntax(source_id, input)?;
        build_config(source_id, input, document)
    }
}

#[derive(Debug, Clone)]
struct TomlDocument {
    entries: Vec<TomlEntry>,
}

#[derive(Debug, Clone)]
enum TomlEntry {
    Table {
        path: Vec<Spanned<String>>,
        span: SourceSpan,
    },
    KeyValue {
        table_path: Vec<Spanned<String>>,
        key_path: Vec<Spanned<String>>,
        value: TomlValue,
        span: SourceSpan,
    },
}

#[derive(Debug, Clone)]
enum TomlValue {
    String {
        value: String,
        span: SourceSpan,
        raw_span: Range<usize>,
    },
    Integer(Spanned<i64>),
    Float(Spanned<f64>),
    Bool(Spanned<bool>),
    Array {
        values: Vec<TomlValue>,
        span: SourceSpan,
    },
    InlineTable {
        attributes: Vec<TomlKeyValue>,
        span: SourceSpan,
    },
    Datetime(SourceSpan),
}

impl TomlValue {
    fn span(&self) -> SourceSpan {
        match self {
            Self::String { span, .. } => *span,
            Self::Integer(value) => value.span,
            Self::Float(value) => value.span,
            Self::Bool(value) => value.span,
            Self::Array { span, .. } => *span,
            Self::InlineTable { span, .. } => *span,
            Self::Datetime(span) => *span,
        }
    }
}

#[derive(Debug, Clone)]
struct TomlKeyValue {
    key_path: Vec<Spanned<String>>,
    value: TomlValue,
    span: SourceSpan,
}

fn parse_toml_syntax(source_id: SourceId, input: &str) -> Result<TomlDocument, TomlReaderError> {
    let source = toml_parser::Source::new(input);
    let tokens = source.lex().into_vec();
    let mut events = Vec::new();
    let mut errors = Vec::new();

    toml_parser::parser::parse_document(&tokens, &mut events, &mut errors);

    if let Some(error) = errors.first() {
        return Err(TomlReaderError::Syntax(error.description().to_owned()));
    }

    TomlEventParser {
        source_id,
        source,
        events,
        position: 0,
        current_table_path: Vec::new(),
        entries: Vec::new(),
    }
    .parse()
}

struct TomlEventParser<'a> {
    source_id: SourceId,
    source: toml_parser::Source<'a>,
    events: Vec<toml_parser::parser::Event>,
    position: usize,
    current_table_path: Vec<Spanned<String>>,
    entries: Vec<TomlEntry>,
}

impl TomlEventParser<'_> {
    fn parse(mut self) -> Result<TomlDocument, TomlReaderError> {
        use toml_parser::parser::EventKind;

        while self.position < self.events.len() {
            self.skip_trivia();
            let Some(event) = self.peek() else {
                break;
            };

            match event.kind() {
                EventKind::StdTableOpen => {
                    let (path, span) = self.parse_table_header()?;
                    self.current_table_path = path.clone();
                    self.entries.push(TomlEntry::Table { path, span });
                }
                EventKind::ArrayTableOpen => {
                    return Err(TomlReaderError::UnsupportedValue(
                        "arrays of tables are not supported".into(),
                    ));
                }
                EventKind::SimpleKey => {
                    let (key_path, value, span) = self.parse_key_value()?;
                    self.entries.push(TomlEntry::KeyValue {
                        table_path: self.current_table_path.clone(),
                        key_path,
                        value,
                        span,
                    });
                }
                EventKind::Error => {
                    return Err(TomlReaderError::Syntax("invalid TOML syntax".into()));
                }
                _ => {
                    self.position += 1;
                }
            }
        }

        Ok(TomlDocument {
            entries: self.entries,
        })
    }

    fn parse_table_header(&mut self) -> Result<(Vec<Spanned<String>>, SourceSpan), TomlReaderError> {
        use toml_parser::parser::EventKind;

        let open = self.expect(EventKind::StdTableOpen)?;
        let path = self.parse_key_path_until(&[EventKind::StdTableClose])?;
        let close = self.expect(EventKind::StdTableClose)?;
        let span = SourceSpan::new(self.source_id, open.span().start(), close.span().end());

        Ok((path, span))
    }

    fn parse_key_value(
        &mut self,
    ) -> Result<(Vec<Spanned<String>>, TomlValue, SourceSpan), TomlReaderError> {
        use toml_parser::parser::EventKind;

        let key_path = self.parse_key_path_until(&[EventKind::KeyValSep])?;
        let start = key_path
            .first()
            .map(|key| key.span.span.start)
            .ok_or_else(|| TomlReaderError::Syntax("missing key".into()))?;
        self.expect(EventKind::KeyValSep)?;
        let value = self.parse_value()?;
        let span = SourceSpan::new(self.source_id, start, value.span().span.end);

        Ok((key_path, value, span))
    }

    fn parse_inline_key_value(&mut self) -> Result<TomlKeyValue, TomlReaderError> {
        use toml_parser::parser::EventKind;

        let key_path = self.parse_key_path_until(&[EventKind::KeyValSep])?;
        let start = key_path
            .first()
            .map(|key| key.span.span.start)
            .ok_or_else(|| TomlReaderError::Syntax("missing inline table key".into()))?;
        self.expect(EventKind::KeyValSep)?;
        let value = self.parse_value()?;
        let span = SourceSpan::new(self.source_id, start, value.span().span.end);

        Ok(TomlKeyValue {
            key_path,
            value,
            span,
        })
    }

    fn parse_key_path_until(
        &mut self,
        terminators: &[toml_parser::parser::EventKind],
    ) -> Result<Vec<Spanned<String>>, TomlReaderError> {
        use toml_parser::parser::EventKind;

        let mut path = Vec::new();

        loop {
            self.skip_inline_trivia();
            let Some(event) = self.peek() else {
                return Err(TomlReaderError::UnexpectedEof);
            };

            if terminators.contains(&event.kind()) {
                break;
            }

            let key = self.expect(EventKind::SimpleKey)?;
            path.push(self.decode_key_event(&key)?);
            self.skip_inline_trivia();

            let Some(next) = self.peek() else {
                return Err(TomlReaderError::UnexpectedEof);
            };

            if next.kind() == EventKind::KeySep {
                self.position += 1;
                continue;
            }

            if terminators.contains(&next.kind()) {
                break;
            }

            return Err(TomlReaderError::UnexpectedEvent(format!(
                "expected key separator or terminator, found {}",
                next.kind().description()
            )));
        }

        Ok(path)
    }

    fn parse_value(&mut self) -> Result<TomlValue, TomlReaderError> {
        use toml_parser::decoder::ScalarKind;
        use toml_parser::parser::EventKind;

        self.skip_trivia();
        let Some(event) = self.peek().cloned() else {
            return Err(TomlReaderError::UnexpectedEof);
        };

        match event.kind() {
            EventKind::Scalar => {
                self.position += 1;
                let raw = self
                    .source
                    .get(&event)
                    .ok_or_else(|| TomlReaderError::MissingSpan("scalar".into()))?;
                let mut decoded = String::new();
                let mut errors = Vec::new();
                let kind = raw.decode_scalar(&mut decoded, &mut errors);
                if let Some(error) = errors.first() {
                    return Err(TomlReaderError::Syntax(error.description().to_owned()));
                }

                let span = parser_span_to_source_span(self.source_id, event.span());
                let raw_span = event.span().start()..event.span().end();

                match kind {
                    ScalarKind::String => Ok(TomlValue::String {
                        value: decoded,
                        span,
                        raw_span,
                    }),
                    ScalarKind::Boolean(value) => Ok(TomlValue::Bool(Spanned::new(value, span))),
                    ScalarKind::Integer(_) => {
                        let value = parse_integer(raw.as_str())?;
                        Ok(TomlValue::Integer(Spanned::new(value, span)))
                    }
                    ScalarKind::Float => {
                        let value = raw
                            .as_str()
                            .replace('_', "")
                            .parse::<f64>()
                            .map_err(|_| TomlReaderError::Syntax("invalid float".into()))?;
                        Ok(TomlValue::Float(Spanned::new(value, span)))
                    }
                    ScalarKind::DateTime => Ok(TomlValue::Datetime(span)),
                }
            }
            EventKind::ArrayOpen => self.parse_array(),
            EventKind::InlineTableOpen => self.parse_inline_table(),
            other => Err(TomlReaderError::UnexpectedEvent(format!(
                "expected value, found {}",
                other.description()
            ))),
        }
    }

    fn parse_array(&mut self) -> Result<TomlValue, TomlReaderError> {
        use toml_parser::parser::EventKind;

        let open = self.expect(EventKind::ArrayOpen)?;
        let mut values = Vec::new();

        loop {
            self.skip_trivia();
            let Some(event) = self.peek() else {
                return Err(TomlReaderError::UnexpectedEof);
            };

            if event.kind() == EventKind::ArrayClose {
                let close = self.expect(EventKind::ArrayClose)?;
                let span = SourceSpan::new(self.source_id, open.span().start(), close.span().end());
                return Ok(TomlValue::Array { values, span });
            }

            values.push(self.parse_value()?);
            self.skip_trivia();

            if matches!(self.peek().map(|event| event.kind()), Some(EventKind::ValueSep)) {
                self.position += 1;
            }
        }
    }

    fn parse_inline_table(&mut self) -> Result<TomlValue, TomlReaderError> {
        use toml_parser::parser::EventKind;

        let open = self.expect(EventKind::InlineTableOpen)?;
        let mut attributes = Vec::new();

        loop {
            self.skip_trivia();
            let Some(event) = self.peek() else {
                return Err(TomlReaderError::UnexpectedEof);
            };

            if event.kind() == EventKind::InlineTableClose {
                let close = self.expect(EventKind::InlineTableClose)?;
                let span = SourceSpan::new(self.source_id, open.span().start(), close.span().end());
                return Ok(TomlValue::InlineTable { attributes, span });
            }

            attributes.push(self.parse_inline_key_value()?);
            self.skip_trivia();

            if matches!(self.peek().map(|event| event.kind()), Some(EventKind::ValueSep)) {
                self.position += 1;
            }
        }
    }

    fn decode_key_event(
        &self,
        event: &toml_parser::parser::Event,
    ) -> Result<Spanned<String>, TomlReaderError> {
        let raw = self
            .source
            .get(event)
            .ok_or_else(|| TomlReaderError::MissingSpan("key".into()))?;
        let mut decoded = String::new();
        let mut errors = Vec::new();
        raw.decode_key(&mut decoded, &mut errors);
        if let Some(error) = errors.first() {
            return Err(TomlReaderError::Syntax(error.description().to_owned()));
        }

        Ok(Spanned::new(
            decoded,
            parser_span_to_source_span(self.source_id, event.span()),
        ))
    }

    fn skip_trivia(&mut self) {
        use toml_parser::parser::EventKind;
        while matches!(
            self.peek().map(|event| event.kind()),
            Some(EventKind::Whitespace | EventKind::Comment | EventKind::Newline)
        ) {
            self.position += 1;
        }
    }

    fn skip_inline_trivia(&mut self) {
        use toml_parser::parser::EventKind;
        while matches!(
            self.peek().map(|event| event.kind()),
            Some(EventKind::Whitespace | EventKind::Comment)
        ) {
            self.position += 1;
        }
    }

    fn expect(
        &mut self,
        kind: toml_parser::parser::EventKind,
    ) -> Result<toml_parser::parser::Event, TomlReaderError> {
        let Some(event) = self.peek().cloned() else {
            return Err(TomlReaderError::UnexpectedEof);
        };

        if event.kind() != kind {
            return Err(TomlReaderError::UnexpectedEvent(format!(
                "expected {}, found {}",
                kind.description(),
                event.kind().description()
            )));
        }

        self.position += 1;
        Ok(event)
    }

    fn peek(&self) -> Option<&toml_parser::parser::Event> {
        self.events.get(self.position)
    }
}

fn parse_integer(raw: &str) -> Result<i64, TomlReaderError> {
    let mut value = raw.replace('_', "");
    let sign = if let Some(stripped) = value.strip_prefix('-') {
        value = stripped.to_string();
        -1
    } else if let Some(stripped) = value.strip_prefix('+') {
        value = stripped.to_string();
        1
    } else {
        1
    };

    let (radix, digits) = if let Some(digits) = value.strip_prefix("0x") {
        (16, digits)
    } else if let Some(digits) = value.strip_prefix("0o") {
        (8, digits)
    } else if let Some(digits) = value.strip_prefix("0b") {
        (2, digits)
    } else {
        (10, value.as_str())
    };

    let parsed = i64::from_str_radix(digits, radix)
        .map_err(|_| TomlReaderError::Syntax("invalid integer".into()))?;
    Ok(parsed * sign)
}

fn parser_span_to_source_span(source_id: SourceId, span: toml_parser::Span) -> SourceSpan {
    SourceSpan::new(source_id, span.start(), span.end())
}

fn build_config(
    source_id: SourceId,
    input: &str,
    document: TomlDocument,
) -> Result<Config, TomlReaderError> {
    let mut parameters: BTreeMap<String, Parameter> = BTreeMap::new();
    let mut resources: BTreeMap<(String, String), Resource> = BTreeMap::new();
    let mut data: BTreeMap<(String, String), Data> = BTreeMap::new();

    for entry in document.entries {
        match entry {
            TomlEntry::Table { path, span } => {
                apply_table_entry(&mut parameters, &mut resources, &mut data, path, span)?;
            }
            TomlEntry::KeyValue {
                table_path,
                key_path,
                value,
                span,
            } => {
                apply_key_value_entry(
                    source_id,
                    input,
                    &mut parameters,
                    &mut resources,
                    &mut data,
                    table_path,
                    key_path,
                    value,
                    span,
                )?;
            }
        }
    }

    Ok(Config {
        parameters: parameters.into_values().collect(),
        resources: resources.into_values().collect(),
        data: data.into_values().collect(),
    })
}

fn apply_table_entry(
    parameters: &mut BTreeMap<String, Parameter>,
    resources: &mut BTreeMap<(String, String), Resource>,
    data: &mut BTreeMap<(String, String), Data>,
    path: Vec<Spanned<String>>,
    span: SourceSpan,
) -> Result<(), TomlReaderError> {
    match path.first().map(|segment| segment.value.as_str()) {
        Some("parameter") => {
            if path.len() < 2 {
                return Err(invalid_section(&path, "expected parameter.<name>"));
            }
            let parameter = ensure_parameter(parameters, &path[1], span);
            insert_object_path(&mut parameter.attributes, &path[2..], span);
        }
        Some("resource") => {
            if path.len() < 3 {
                return Err(invalid_section(&path, "expected resource.<type>.<name>"));
            }
            let resource = ensure_resource(resources, &path[1], &path[2], span);
            insert_object_path(&mut resource.attributes, &path[3..], span);
        }
        Some("data") => {
            if path.len() < 3 {
                return Err(invalid_section(&path, "expected data.<type>.<name>"));
            }
            let data = ensure_data(data, &path[1], &path[2], span);
            insert_object_path(&mut data.attributes, &path[3..], span);
        }
        Some(other) => return Err(TomlReaderError::UnknownTopLevelSection(other.to_owned())),
        None => {}
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_key_value_entry(
    source_id: SourceId,
    input: &str,
    parameters: &mut BTreeMap<String, Parameter>,
    resources: &mut BTreeMap<(String, String), Resource>,
    data: &mut BTreeMap<(String, String), Data>,
    table_path: Vec<Spanned<String>>,
    key_path: Vec<Spanned<String>>,
    value: TomlValue,
    span: SourceSpan,
) -> Result<(), TomlReaderError> {
    match table_path.first().map(|segment| segment.value.as_str()) {
        Some("parameter") => {
            if table_path.len() < 2 {
                return Err(invalid_section(&table_path, "expected parameter.<name>"));
            }
            let parameter = ensure_parameter(parameters, &table_path[1], table_path_span(&table_path));
            let mut attribute_path = table_path[2..].to_vec();
            attribute_path.extend(key_path);
            let attribute = toml_value_to_attribute(source_id, input, attribute_path.last().unwrap(), value, span)?;
            insert_attribute_path(&mut parameter.attributes, &attribute_path, attribute);
        }
        Some("resource") => {
            if table_path.len() < 3 {
                return Err(invalid_section(&table_path, "expected resource.<type>.<name>"));
            }
            let resource = ensure_resource(
                resources,
                &table_path[1],
                &table_path[2],
                table_path_span(&table_path),
            );
            let mut attribute_path = table_path[3..].to_vec();
            attribute_path.extend(key_path);
            let attribute = toml_value_to_attribute(source_id, input, attribute_path.last().unwrap(), value, span)?;
            insert_attribute_path(&mut resource.attributes, &attribute_path, attribute);
        }
        Some("data") => {
            if table_path.len() < 3 {
                return Err(invalid_section(&table_path, "expected data.<type>.<name>"));
            }
            let data = ensure_data(
                data,
                &table_path[1],
                &table_path[2],
                table_path_span(&table_path),
            );
            let mut attribute_path = table_path[3..].to_vec();
            attribute_path.extend(key_path);
            let attribute = toml_value_to_attribute(source_id, input, attribute_path.last().unwrap(), value, span)?;
            insert_attribute_path(&mut data.attributes, &attribute_path, attribute);
        }
        Some(other) => return Err(TomlReaderError::UnknownTopLevelSection(other.to_owned())),
        None => {
            return Err(TomlReaderError::InvalidSection {
                section: "root".into(),
                message: "key/value pairs must be inside parameter/resource/data sections".into(),
            });
        }
    }

    Ok(())
}

fn ensure_parameter<'a>(
    parameters: &'a mut BTreeMap<String, Parameter>,
    name: &Spanned<String>,
    span: SourceSpan,
) -> &'a mut Parameter {
    parameters
        .entry(name.value.clone())
        .or_insert_with(|| Parameter {
            name: name.clone(),
            attributes: BTreeMap::new(),
            span,
        })
}

fn ensure_resource<'a>(
    resources: &'a mut BTreeMap<(String, String), Resource>,
    resource_type: &Spanned<String>,
    name: &Spanned<String>,
    span: SourceSpan,
) -> &'a mut Resource {
    resources
        .entry((resource_type.value.clone(), name.value.clone()))
        .or_insert_with(|| Resource {
            resource_type: resource_type.clone(),
            name: name.clone(),
            attributes: BTreeMap::new(),
            span,
        })
}

fn ensure_data<'a>(
    data: &'a mut BTreeMap<(String, String), Data>,
    data_type: &Spanned<String>,
    name: &Spanned<String>,
    span: SourceSpan,
) -> &'a mut Data {
    data.entry((data_type.value.clone(), name.value.clone()))
        .or_insert_with(|| Data {
            data_type: data_type.clone(),
            name: name.clone(),
            attributes: BTreeMap::new(),
            span,
        })
}

fn insert_object_path(attributes: &mut Attributes, path: &[Spanned<String>], span: SourceSpan) {
    if path.is_empty() {
        return;
    }

    let key = path[0].clone();
    let entry = attributes.entry(key.value.clone()).or_insert_with(|| Attribute {
        name: key,
        value: Spanned::new(ExprKind::Literal(Value::Object(BTreeMap::new())), span),
        span,
    });

    if let ExprKind::Literal(Value::Object(children)) = &mut entry.value.value {
        insert_object_path(children, &path[1..], span);
    }
}

fn insert_attribute_path(attributes: &mut Attributes, path: &[Spanned<String>], attribute: Attribute) {
    if path.is_empty() {
        return;
    }

    if path.len() == 1 {
        attributes.insert(path[0].value.clone(), attribute);
        return;
    }

    let key = path[0].clone();
    let entry = attributes.entry(key.value.clone()).or_insert_with(|| Attribute {
        name: key,
        value: Spanned::new(
            ExprKind::Literal(Value::Object(BTreeMap::new())),
            attribute.span,
        ),
        span: attribute.span,
    });

    if let ExprKind::Literal(Value::Object(children)) = &mut entry.value.value {
        insert_attribute_path(children, &path[1..], attribute);
    }
}

fn toml_value_to_attribute(
    source_id: SourceId,
    input: &str,
    key: &Spanned<String>,
    value: TomlValue,
    span: SourceSpan,
) -> Result<Attribute, TomlReaderError> {
    Ok(Attribute {
        name: key.clone(),
        value: toml_value_to_expr(source_id, input, value)?,
        span,
    })
}

fn toml_value_to_expr(
    source_id: SourceId,
    input: &str,
    value: TomlValue,
) -> Result<Expr, TomlReaderError> {
    match value {
        TomlValue::String {
            value,
            span: _,
            raw_span,
        } => {
            let mapper = TomlStringSpanMapper::new(source_id, &value, input, raw_span)?;
            Ok(expressions::parse_string(&value, &mapper)?)
        }
        TomlValue::Integer(value) => Ok(Spanned::new(
            ExprKind::Literal(Value::Integer(value.value)),
            value.span,
        )),
        TomlValue::Float(value) => Ok(Spanned::new(
            ExprKind::Literal(Value::Float(value.value)),
            value.span,
        )),
        TomlValue::Bool(value) => Ok(Spanned::new(
            ExprKind::Literal(Value::Bool(value.value)),
            value.span,
        )),
        TomlValue::Array { values, span } => {
            let values = values
                .into_iter()
                .map(|value| toml_value_to_expr(source_id, input, value))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Spanned::new(ExprKind::Literal(Value::Array(values)), span))
        }
        TomlValue::InlineTable { attributes, span } => {
            let mut converted = BTreeMap::new();
            for key_value in attributes {
                let Some(key) = key_value.key_path.last().cloned() else {
                    continue;
                };
                let attribute = toml_value_to_attribute(
                    source_id,
                    input,
                    &key,
                    key_value.value,
                    key_value.span,
                )?;
                insert_attribute_path(&mut converted, &key_value.key_path, attribute);
            }
            Ok(Spanned::new(
                ExprKind::Literal(Value::Object(converted)),
                span,
            ))
        }
        TomlValue::Datetime(span) => Err(TomlReaderError::UnsupportedValue(format!(
            "datetime at {}..{}",
            span.span.start, span.span.end
        ))),
    }
}

fn table_path_span(path: &[Spanned<String>]) -> SourceSpan {
    let source = path.first().map(|segment| segment.span.source).unwrap_or(SourceId::SYNTHETIC);
    let start = path.first().map(|segment| segment.span.span.start).unwrap_or(0);
    let end = path.last().map(|segment| segment.span.span.end).unwrap_or(start);
    SourceSpan::new(source, start, end)
}

fn invalid_section(path: &[Spanned<String>], message: &str) -> TomlReaderError {
    TomlReaderError::InvalidSection {
        section: path
            .iter()
            .map(|segment| segment.value.as_str())
            .collect::<Vec<_>>()
            .join("."),
        message: message.to_owned(),
    }
}

struct TomlStringSpanMapper {
    source_id: SourceId,
    boundaries: Vec<usize>,
}

impl TomlStringSpanMapper {
    fn new(
        source_id: SourceId,
        decoded: &str,
        input: &str,
        raw_span: Range<usize>,
    ) -> Result<Self, TomlReaderError> {
        let raw = input
            .get(raw_span.clone())
            .ok_or_else(|| TomlReaderError::MissingSpan("string source text".into()))?;
        let boundaries = build_toml_string_boundaries(decoded, raw, raw_span.start)
            .unwrap_or_else(|| simple_string_boundaries(decoded, raw, raw_span.start));

        Ok(Self {
            source_id,
            boundaries,
        })
    }
}

impl ExpressionSpanMapper for TomlStringSpanMapper {
    fn span(&self, start: usize, end: usize) -> SourceSpan {
        let start = self.boundaries.get(start).copied().unwrap_or_else(|| {
            self.boundaries
                .last()
                .copied()
                .unwrap_or_default()
        });
        let end = self.boundaries.get(end).copied().unwrap_or(start);

        SourceSpan::new(self.source_id, start, end)
    }
}

fn simple_string_boundaries(decoded: &str, raw: &str, raw_start: usize) -> Vec<usize> {
    let content = string_content_range(raw, raw_start)
        .unwrap_or_else(|| raw_start + 1..raw_start + raw.len().saturating_sub(1));
    (0..=decoded.len()).map(|offset| content.start + offset).collect()
}

fn string_content_range(raw: &str, raw_start: usize) -> Option<Range<usize>> {
    if raw.starts_with("\"\"\"") && raw.ends_with("\"\"\"") && raw.len() >= 6 {
        Some(raw_start + 3..raw_start + raw.len() - 3)
    } else if raw.starts_with("'''") && raw.ends_with("'''") && raw.len() >= 6 {
        Some(raw_start + 3..raw_start + raw.len() - 3)
    } else if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        Some(raw_start + 1..raw_start + raw.len() - 1)
    } else if raw.starts_with('\'') && raw.ends_with('\'') && raw.len() >= 2 {
        Some(raw_start + 1..raw_start + raw.len() - 1)
    } else {
        None
    }
}

fn build_toml_string_boundaries(decoded: &str, raw: &str, raw_start: usize) -> Option<Vec<usize>> {
    let (content_start, content_end, literal, multiline) = if raw.starts_with("\"\"\"") && raw.ends_with("\"\"\"") && raw.len() >= 6 {
        (3, raw.len() - 3, false, true)
    } else if raw.starts_with("'''") && raw.ends_with("'''") && raw.len() >= 6 {
        (3, raw.len() - 3, true, true)
    } else if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        (1, raw.len() - 1, false, false)
    } else if raw.starts_with('\'') && raw.ends_with('\'') && raw.len() >= 2 {
        (1, raw.len() - 1, true, false)
    } else {
        return None;
    };

    let raw_bytes = raw.as_bytes();
    let mut index = content_start;

    if multiline {
        if raw[index..content_end].starts_with("\r\n") {
            index += 2;
        } else if raw[index..content_end].starts_with('\n') {
            index += 1;
        }
    }

    if literal {
        return build_literal_string_boundaries(decoded, raw, raw_start, index, content_end);
    }

    let mut boundaries = Vec::with_capacity(decoded.len() + 1);
    boundaries.push(raw_start + index);
    let mut decoded_index = 0;

    while index < content_end {
        let byte = raw_bytes[index];

        if byte == b'\\' {
            index += 1;
            if index >= content_end {
                return None;
            }

            if multiline && (raw[index..content_end].starts_with("\r\n") || raw[index..content_end].starts_with('\n')) {
                if raw[index..content_end].starts_with("\r\n") {
                    index += 2;
                } else {
                    index += 1;
                }

                while index < content_end {
                    let Some(ch) = raw[index..content_end].chars().next() else {
                        break;
                    };
                    if ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' {
                        index += ch.len_utf8();
                    } else {
                        break;
                    }
                }

                if let Some(last) = boundaries.last_mut() {
                    *last = raw_start + index;
                }
                continue;
            }

            let escaped = raw.as_bytes()[index] as char;
            index += 1;
            let decoded_char = match escaped {
                'b' => '\u{0008}',
                't' => '\t',
                'n' => '\n',
                'f' => '\u{000C}',
                'r' => '\r',
                '"' => '"',
                '\\' => '\\',
                'u' => {
                    let end = index + 4;
                    if end > content_end {
                        return None;
                    }
                    let value = u32::from_str_radix(&raw[index..end], 16).ok()?;
                    index = end;
                    char::from_u32(value)?
                }
                'U' => {
                    let end = index + 8;
                    if end > content_end {
                        return None;
                    }
                    let value = u32::from_str_radix(&raw[index..end], 16).ok()?;
                    index = end;
                    char::from_u32(value)?
                }
                _ => return None,
            };

            push_decoded_char_boundary(
                decoded,
                &mut decoded_index,
                &mut boundaries,
                decoded_char,
                raw_start + index,
            )?;
        } else {
            let ch = raw[index..content_end].chars().next()?;
            index += ch.len_utf8();
            push_decoded_char_boundary(
                decoded,
                &mut decoded_index,
                &mut boundaries,
                ch,
                raw_start + index,
            )?;
        }
    }

    if decoded_index == decoded.len() && boundaries.len() == decoded.len() + 1 {
        Some(boundaries)
    } else {
        None
    }
}

fn build_literal_string_boundaries(
    decoded: &str,
    raw: &str,
    raw_start: usize,
    mut index: usize,
    content_end: usize,
) -> Option<Vec<usize>> {
    let mut boundaries = Vec::with_capacity(decoded.len() + 1);
    boundaries.push(raw_start + index);
    let mut decoded_index = 0;

    while index < content_end {
        let ch = raw[index..content_end].chars().next()?;
        index += ch.len_utf8();
        push_decoded_char_boundary(
            decoded,
            &mut decoded_index,
            &mut boundaries,
            ch,
            raw_start + index,
        )?;
    }

    if decoded_index == decoded.len() && boundaries.len() == decoded.len() + 1 {
        Some(boundaries)
    } else {
        None
    }
}

fn push_decoded_char_boundary(
    decoded: &str,
    decoded_index: &mut usize,
    boundaries: &mut Vec<usize>,
    ch: char,
    source_end: usize,
) -> Option<()> {
    let mut buffer = [0; 4];
    let encoded = ch.encode_utf8(&mut buffer);
    if !decoded.get(*decoded_index..)?.starts_with(&*encoded) {
        return None;
    }

    *decoded_index += encoded.len();
    for _ in 0..encoded.len() {
        boundaries.push(source_end);
    }

    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{ExprKind, TemplatePart};

    fn read(input: &str) -> Config {
        TomlReader.read(SourceId(0), input).unwrap()
    }

    fn reference_path(expr: &Expr) -> Vec<String> {
        let ExprKind::Reference(reference) = &expr.value else {
            panic!("expected reference");
        };

        reference
            .path
            .iter()
            .map(|segment| segment.value.clone())
            .collect()
    }

    #[test]
    fn reads_example_file() {
        let input = include_str!("../../tests/fixtures/toml/example.toml");

        let config = TomlReader.read(SourceId(0), input).unwrap();

        dbg!(config);
    }

    #[test]
    fn reads_parameter_with_spans() {
        let input = "[parameter.db_size]\ndefault = 20\n";
        let config = read(input);

        let parameter = &config.parameters[0];
        assert_eq!(parameter.name.value, "db_size");
        assert_eq!(parameter.name.span, SourceSpan::new(SourceId(0), 11, 18));
        assert_eq!(parameter.span, SourceSpan::new(SourceId(0), 0, 19));

        let default = parameter.attributes.get("default").unwrap();
        assert_eq!(default.name.span, SourceSpan::new(SourceId(0), 20, 27));
        assert_eq!(default.value.span, SourceSpan::new(SourceId(0), 30, 32));
    }

    #[test]
    fn reads_resource_reference() {
        let input = r#"
[parameter.db_size]
default = 20

[resource.database.main]
size = "{{ parameter.db_size }}"
"#;

        let config = read(input);
        let resource = &config.resources[0];

        assert_eq!(resource.resource_type.value, "database");
        assert_eq!(resource.name.value, "main");

        let size = &resource.attributes.get("size").unwrap().value;
        assert_eq!(reference_path(size), vec!["parameter", "db_size"]);
    }

    #[test]
    fn reads_resource_interpolation() {
        let input = r#"
[resource.database.main]
name = "db-{{ parameter.environment }}-primary"
"#;

        let config = read(input);
        let resource = &config.resources[0];
        let name = &resource.attributes.get("name").unwrap().value;

        let ExprKind::Template(parts) = &name.value else {
            panic!("expected template");
        };

        assert_eq!(parts.len(), 3);

        let TemplatePart::Literal(prefix) = &parts[0] else {
            panic!("expected prefix literal");
        };
        assert_eq!(prefix.value, "db-");
    }

    #[test]
    fn maps_expression_spans_after_toml_string_escape() {
        let input = r#"[resource.server.main]
name = "line\n{{ parameter.environment }}"
"#;

        let config = read(input);
        let resource = &config.resources[0];
        let name = &resource.attributes.get("name").unwrap().value;

        let ExprKind::Template(parts) = &name.value else {
            panic!("expected template");
        };

        let TemplatePart::Expr(expr) = &parts[1] else {
            panic!("expected expression part");
        };

        let ExprKind::Reference(reference) = &expr.value else {
            panic!("expected reference");
        };

        let parameter_start = input.find("parameter").unwrap();
        let environment_start = input.find("environment").unwrap();

        assert_eq!(
            reference.path[0].span,
            SourceSpan::new(SourceId(0), parameter_start, parameter_start + "parameter".len())
        );
        assert_eq!(
            reference.path[1].span,
            SourceSpan::new(
                SourceId(0),
                environment_start,
                environment_start + "environment".len()
            )
        );
    }

    #[test]
    fn reads_array_containing_expression() {
        let input = r#"
[resource.server.main]
ports = [8080, "{{ parameter.admin_port }}"]
"#;

        let config = read(input);
        let resource = &config.resources[0];
        let ports = &resource.attributes.get("ports").unwrap().value;

        let ExprKind::Literal(Value::Array(values)) = &ports.value else {
            panic!("expected array");
        };

        assert_eq!(values.len(), 2);
        assert_eq!(reference_path(&values[1]), vec!["parameter", "admin_port"]);
    }

    #[test]
    fn reads_nested_subtable_as_object_attribute() {
        let input = r#"
[resource.server.main]

[resource.server.main.labels]
app = "web"
env = "{{ parameter.environment }}"
"#;

        let config = read(input);
        let resource = &config.resources[0];
        let labels = &resource.attributes.get("labels").unwrap().value;

        let ExprKind::Literal(Value::Object(labels)) = &labels.value else {
            panic!("expected object");
        };

        assert!(labels.contains_key("app"));
        assert_eq!(
            reference_path(&labels.get("env").unwrap().value),
            vec!["parameter", "environment"]
        );
    }

    #[test]
    fn nested_subtable_introduces_resource_with_full_header_span() {
        let input = "[resource.server.main.labels]\napp = \"web\"\n";
        let config = read(input);
        let resource = &config.resources[0];

        assert_eq!(resource.span, SourceSpan::new(SourceId(0), 0, 29));
    }
}
