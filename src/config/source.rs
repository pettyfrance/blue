#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    pub source: SourceId,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMap {
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: SourceSpan,
}

impl Source {
    /// Translate a UTF-8 byte offset into a 1-based line and column.
    /// Columns count Unicode scalar values, not bytes or terminal display cells.
    /// EOF is valid; out-of-bounds offsets and partial UTF-8 characters are not.
    pub fn line_column(&self, byte_offset: usize) -> Option<(usize, usize)> {
        let prefix = self.text.get(..byte_offset)?;
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        Some((line, column))
    }
}

impl SourceId {
    pub const SYNTHETIC: SourceId = SourceId(usize::MAX);
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

impl SourceSpan {
    pub const SYNTHETIC: SourceSpan = SourceSpan {
        source: SourceId::SYNTHETIC,
        span: Span { start: 0, end: 0 },
    };

    pub const fn new(source: SourceId, start: usize, end: usize) -> Self {
        Self {
            source,
            span: Span::new(start, end),
        }
    }
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: impl Into<String>, text: impl Into<String>) -> SourceId {
        let id = SourceId(self.sources.len());

        self.sources.push(Source {
            name: name.into(),
            text: text.into(),
        });

        id
    }

    pub fn get(&self, id: SourceId) -> Option<&Source> {
        self.sources.get(id.0)
    }
}

impl<T> Spanned<T> {
    pub fn new(value: T, span: SourceSpan) -> Self {
        Self { value, span }
    }

    pub fn synthetic(value: T) -> Self {
        Self {
            value,
            span: SourceSpan::SYNTHETIC,
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Spanned<U> {
        Spanned {
            value: f(self.value),
            span: self.span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_locations_handle_lines_unicode_and_eof() {
        let source = Source {
            name: "test".into(),
            text: "é界\nnext\r\n".into(),
        };
        assert_eq!(source.line_column(0), Some((1, 1)));
        assert_eq!(source.line_column("é".len()), Some((1, 2)));
        assert_eq!(source.line_column("é界".len()), Some((1, 3)));
        assert_eq!(source.line_column("é界\n".len()), Some((2, 1)));
        assert_eq!(source.line_column("é界\nnext".len()), Some((2, 5)));
        assert_eq!(source.line_column(source.text.len()), Some((3, 1)));
        assert_eq!(source.line_column(1), None);
        assert_eq!(source.line_column(3), None);
        assert_eq!(source.line_column(source.text.len() + 1), None);
        assert_eq!(source.line_column(usize::MAX), None);
    }

    #[test]
    fn source_locations_handle_empty_input_and_no_trailing_newline() {
        let mut source = Source {
            name: "test".into(),
            text: String::new(),
        };
        assert_eq!(source.line_column(0), Some((1, 1)));
        assert_eq!(source.line_column(1), None);
        source.text = "one\ntwo".into();
        assert_eq!(source.line_column(source.text.len()), Some((2, 4)));
    }

    #[test]
    fn adding_source_returns_first_source_id() {
        let mut source_map = SourceMap::new();

        let source_id = source_map.add("infra.toml", "[parameter.size]");

        assert_eq!(source_id, SourceId(0));
    }

    #[test]
    fn adding_multiple_sources_returns_stable_incrementing_ids() {
        let mut source_map = SourceMap::new();

        let first = source_map.add("infra.toml", "first");
        let second = source_map.add("vars.json", "second");

        assert_eq!(first, SourceId(0));
        assert_eq!(second, SourceId(1));
    }

    #[test]
    fn retrieving_source_by_id_returns_source() {
        let mut source_map = SourceMap::new();

        let source_id = source_map.add("infra.toml", "[parameter.size]");

        assert_eq!(
            source_map.get(source_id),
            Some(&Source {
                name: "infra.toml".into(),
                text: "[parameter.size]".into(),
            })
        );
    }

    #[test]
    fn synthetic_span_is_available() {
        assert_eq!(
            SourceSpan::SYNTHETIC,
            SourceSpan {
                source: SourceId::SYNTHETIC,
                span: Span { start: 0, end: 0 },
            }
        );
    }

    #[test]
    fn spanned_map_preserves_span() {
        let span = SourceSpan::new(SourceId(3), 10, 20);
        let value = Spanned::new("hello", span);

        let mapped = value.map(str::len);

        assert_eq!(mapped.value, 5);
        assert_eq!(mapped.span, span);
    }
}
