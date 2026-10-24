use super::*;
use crate::config::SourceId;

const SOURCE: SourceId = SourceId(37);

fn error(input: &str) -> TomlReaderError {
    TomlReader.read(SOURCE, input).unwrap_err()
}

fn text_span(input: &str, text: &str) -> SourceSpan {
    let start = input.find(text).unwrap();
    SourceSpan::new(SOURCE, start, start + text.len())
}

#[test]
fn syntax_error_preserves_the_underlying_parser_range() {
    let input = "[parameter.test";
    let source = toml_parser::Source::new(input);
    let mut errors = Vec::new();
    toml_parser::parser::parse_document(&source.lex().into_vec(), &mut Vec::new(), &mut errors);
    let underlying = &errors[0];
    let expected = underlying
        .unexpected()
        .or_else(|| underlying.context())
        .unwrap();
    let error = error(input);
    assert!(matches!(error.kind, TomlReaderErrorKind::Syntax(_)));
    assert_eq!(
        error.span,
        Some(parser_span_to_source_span(SOURCE, expected))
    );
}

#[test]
fn scalar_decoder_errors_are_absolute_not_relative_to_the_string() {
    let input = "[parameter.test]\ndefault = \"bad \\q\"\n";
    let error = error(input);
    assert!(matches!(error.kind, TomlReaderErrorKind::Syntax(_)));
    let span = error.span.unwrap();
    assert_eq!(span.source, SOURCE);
    // Decoder error spans are rebased by toml_parser::Raw, not by our wrapper.
    assert!(span.span.start >= input.find("\\q").unwrap());
    assert!(span.span.end <= input.len());
}

#[test]
fn unknown_and_malformed_sections_use_key_path_spans() {
    let input = "# comment\n[resorce.server.app]\n";
    let error = error(input);
    assert_eq!(
        error.kind,
        TomlReaderErrorKind::UnknownTopLevelSection("resorce".into())
    );
    assert_eq!(error.span, Some(text_span(input, "resorce")));
    let input = "[resource.server]\n";
    let error = TomlReader.read(SOURCE, input).unwrap_err();
    assert!(matches!(
        error.kind,
        TomlReaderErrorKind::InvalidSection { .. }
    ));
    assert_eq!(error.span, Some(text_span(input, "resource.server")));
}

#[test]
fn missing_provider_type_points_to_its_header() {
    let input = "# comment\n[provider.cloud]\nregion = \"eu\"\n";
    let error = error(input);
    assert!(
        matches!(error.kind, TomlReaderErrorKind::InvalidSection { ref message, .. } if message.contains("missing required `type`"))
    );
    assert_eq!(error.span, Some(text_span(input, "[provider.cloud]")));
}

#[test]
fn invalid_metadata_types_point_to_full_values_without_echoing_them() {
    for (input, value) in [
        ("[provider.cloud]\ntype = 123\n", "123"),
        (
            "[provider.cloud]\ntype = \"ovh\"\ndefault = \"accidental-secret\"\n",
            "\"accidental-secret\"",
        ),
        ("[resource.server.app]\nprovider = false\n", "false"),
    ] {
        let error = error(input);
        assert!(matches!(
            error.kind,
            TomlReaderErrorKind::InvalidSection { .. }
        ));
        assert_eq!(error.span, Some(text_span(input, value)));
        assert!(!error.to_string().contains("accidental-secret"));
    }
}

#[test]
fn expression_failures_use_original_string_spans_with_escapes_and_unicode() {
    for raw in [
        r#""préfixe {{ parameter. }}""#,
        r#""\u00e9 {{ parameter. }}""#,
        "\"\"\"\npréfixe\n{{ parameter. }}\n\"\"\"",
    ] {
        let input = format!("[parameter.test]\ndefault = {raw}\n");
        let error = error(&input);
        assert!(matches!(
            error.kind,
            TomlReaderErrorKind::Expression(ExpressionError::InvalidReference(_))
        ));
        assert_eq!(error.span, Some(text_span(&input, raw)));
        let span = error.span.unwrap().span;
        assert!(input.is_char_boundary(span.start));
        assert!(input.is_char_boundary(span.end));
    }
}

#[test]
fn nested_expression_errors_point_to_the_inner_string_not_outer_container() {
    let input = "[parameter.test]\ndefault = [{ nested = [\"ok\", \"{{ parameter.test\"] }]\n";
    let error = error(input);
    assert_eq!(
        error.kind,
        TomlReaderErrorKind::Expression(ExpressionError::UnclosedExpression)
    );
    assert_eq!(error.span, Some(text_span(input, "\"{{ parameter.test\"")));
}

#[test]
fn unsupported_values_have_structured_spans() {
    let input = "[parameter.test]\ndefault = 2026-01-02\n";
    let error = error(input);
    assert_eq!(
        error.kind,
        TomlReaderErrorKind::UnsupportedValue("datetime".into())
    );
    assert_eq!(error.span, Some(text_span(input, "2026-01-02")));
    let input = "[[resource.server.app]]\n";
    let error = TomlReader.read(SOURCE, input).unwrap_err();
    assert!(matches!(
        error.kind,
        TomlReaderErrorKind::UnsupportedValue(_)
    ));
    assert_eq!(error.span, Some(SourceSpan::new(SOURCE, 0, 2)));
}

#[test]
fn numeric_conversion_errors_have_value_spans() {
    let span = SourceSpan::new(SOURCE, 30, 60);
    let error = parse_integer("99999999999999999999999999999", span).unwrap_err();
    assert_eq!(
        error.kind,
        TomlReaderErrorKind::Syntax("invalid integer".into())
    );
    assert_eq!(error.span, Some(span));
}

#[test]
fn end_of_input_errors_use_zero_width_eof_spans() {
    let input = "[parameter.test]\ndefault = [1,";
    let error = error(input);
    assert_eq!(
        error.span,
        Some(SourceSpan::new(SOURCE, input.len(), input.len()))
    );
    // Exercise our event parser's EOF path independently of syntax rejection.
    let mut parser = TomlEventParser {
        source_id: SOURCE,
        source: toml_parser::Source::new(input),
        events: vec![],
        position: 0,
        current_table_path: vec![],
        entries: vec![],
    };
    let error = parser.parse_value().unwrap_err();
    assert_eq!(error.kind, TomlReaderErrorKind::UnexpectedEof);
    assert_eq!(
        error.span,
        Some(SourceSpan::new(SOURCE, input.len(), input.len()))
    );
}

#[test]
fn errors_without_a_trustworthy_location_remain_unlocated() {
    let underlying = toml_parser::ParseError::new("test syntax failure");
    let error = syntax_error(SOURCE, &underlying, None);
    assert_eq!(error.span, None);
    assert_eq!(
        error.to_string(),
        "failed to parse TOML syntax: test syntax failure"
    );
    let fallback = SourceSpan::new(SOURCE, 10, 20);
    assert_eq!(
        syntax_error(SOURCE, &underlying, Some(fallback)).span,
        Some(fallback)
    );
    let error = TomlStringSpanMapper::new(SOURCE, "test", "", 100..110)
        .err()
        .unwrap();
    assert_eq!(error.span, None);
    assert!(matches!(error.kind, TomlReaderErrorKind::MissingSpan(_)));
}

#[test]
fn nested_expression_error_preserves_the_error_chain() {
    let error = error("[parameter.test]\ndefault = \"{{ parameter.test\"\n");
    let kind = Error::source(&error).unwrap();
    assert_eq!(kind.to_string(), error.to_string());
    assert!(kind.source().unwrap().is::<ExpressionError>());
}
