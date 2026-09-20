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
