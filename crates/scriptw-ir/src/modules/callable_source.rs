//! Original callable text is recovered through compiler-owned source edits.

use crate::*;

mod definitions;
pub(super) use definitions::CallableSourceDefinitions;

impl super::synchronous_definition::ModuleExecutionAnalysis {
    pub(crate) fn class_method_to_string_representation(
        &self,
        method: &ClassMethodDefinition,
        source_text: CompilerSourceText<'_>,
    ) -> CallableToStringRepresentation {
        CallableToStringRepresentation::ExactSource(
            self.original_callable_source(method.linear_span())
                .map(str::to_owned)
                .unwrap_or_else(|| class_method_source_slice(method, source_text)),
        )
    }
}

/// A rewritten unit carries the original byte boundary for every surviving
/// boundary. Inserted interiors have no source authority. Callable boundaries
/// must resolve to original UTF-8 boundaries before a slice can be published.
#[derive(Debug)]
pub(super) struct OriginalUnitSource {
    original: String,
    text: String,
    boundaries: Vec<Option<usize>>,
}

impl OriginalUnitSource {
    pub(super) fn new(source: &str) -> Self {
        Self {
            original: source.into(),
            text: source.into(),
            boundaries: (0..=source.len()).map(Some).collect(),
        }
    }

    pub(super) fn text(&self) -> &str {
        &self.text
    }

    pub(super) fn into_text(self) -> String {
        self.text
    }

    pub(super) fn stable_rewrite(mut self, text: String) -> Self {
        assert_eq!(
            text.len(),
            self.text.len(),
            "this rewrite preserves byte offsets"
        );
        self.text = text;
        self
    }

    /// `text` is this source after [`super::record::rewrite_import_meta`]:
    /// each `import.meta` site's bytes now hold the `name_len`-byte cell name
    /// and then padding. A callable whose source ends with the
    /// meta-property ends, in `text`, after the name, inside the site; its
    /// original text ends at the site's end. So every position from the end
    /// of the name to the end of the site maps there, and the positions
    /// inside the name map to no original position.
    pub(super) fn rewrite_import_meta(
        self,
        text: String,
        sites: &[scriptw_front::SourceSpan],
        name_len: usize,
    ) -> Self {
        let mut source = self.stable_rewrite(text);
        for site in sites {
            let name_end = site.start + name_len;
            assert!(
                site.start < name_end && name_end <= site.end,
                "the import.meta cell name fits its site"
            );
            source.boundaries[site.start + 1..name_end].fill(None);
            source.boundaries[name_end..site.end].fill(Some(site.end));
        }
        source
    }

    pub(super) fn replace(&mut self, start: usize, end: usize, replacement: &str) {
        assert!(
            !replacement.is_empty(),
            "source edits retain both endpoint authorities"
        );
        assert!(
            self.text.get(start..end).is_some(),
            "source edit is a UTF-8 range"
        );
        let original_end = self.boundaries[end];
        let interiors = std::iter::repeat_n(None, replacement.len() - 1);
        self.boundaries.splice(
            start + 1..end + 1,
            interiors.chain(std::iter::once(original_end)),
        );
        self.text.replace_range(start..end, replacement);
        assert_eq!(self.boundaries.len(), self.text.len() + 1);
    }

    /// Resolves original parser ranges after edits, without rescanning JavaScript.
    pub(super) fn current_ranges(
        &self,
        spans: &[scriptw_front::SourceSpan],
    ) -> Result<Vec<std::ops::Range<usize>>, String> {
        if spans.is_empty() {
            return Ok(Vec::new());
        }
        let mut endpoints: BTreeMap<usize, Option<(usize, usize)>> = BTreeMap::new();
        for span in spans {
            if self.original.get(span.start..span.end).is_none() {
                return Err("dynamic-import parser range is outside its original source".into());
            }
            endpoints.insert(span.start, None);
            endpoints.insert(span.end, None);
        }
        for (current, original) in self.boundaries.iter().enumerate() {
            if let Some(endpoint) = original.and_then(|offset| endpoints.get_mut(&offset)) {
                match endpoint {
                    Some((_, last)) => *last = current,
                    None => *endpoint = Some((current, current)),
                }
            }
        }
        spans
            .iter()
            .map(|span| {
                // Insertions at a boundary belong before a start and after an end.
                let start = endpoints[&span.start].map(|(_, last)| last);
                let end = endpoints[&span.end].map(|(first, _)| first);
                match (start, end) {
                    (Some(start), Some(end)) if self.text.get(start..end).is_some() => {
                        Ok(start..end)
                    }
                    _ => Err("dynamic-import parser range was lost during source rewriting".into()),
                }
            })
            .collect()
    }

    pub(super) fn wrap(mut self, prefix: &str, suffix: &str) -> Self {
        self.boundaries
            .splice(0..0, std::iter::repeat_n(None, prefix.len()));
        self.boundaries
            .extend(std::iter::repeat_n(None, suffix.len()));
        self.text = format!("{prefix}{}{suffix}", self.text);
        assert_eq!(self.boundaries.len(), self.text.len() + 1);
        self
    }

    fn original_slice(&self, span: boa_ast::LinearSpan) -> Option<String> {
        let bytes = source_byte_range_from_utf16_span(&self.text, span);
        let start = self.boundaries[bytes.start]?;
        let end = self.boundaries[bytes.end]?;
        let original = self
            .original
            .get(start..end)
            .expect("callable origins are original UTF-8 boundaries");
        Some(encode_js_string_utf16(
            &original.encode_utf16().collect::<Vec<_>>(),
        ))
    }
}
