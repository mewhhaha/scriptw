//! Removing module-goal-only syntax from a module's source text.
//!
//! [`link`] links a graph by concatenating every unit's body, in evaluation
//! order, into one Script-goal source that the ordinary single-script pipeline
//! lowers. The only thing standing between a module body and that pipeline is
//! the module-goal-only syntax itself: `import` declarations, which declare
//! bindings rather than execute, and the `export` modifier, which decorates a
//! declaration without changing what it does.
//!
//! [`link`]: super::link
//!
//! Module-only ranges come from the original parser, including ASI boundaries,
//! import attributes and default declaration ends. Strings, regex literals,
//! templates and property names never pass through a second lexer here.
//!
//! Deleted bytes are replaced with the same number of space *bytes* and every
//! line terminator inside the deleted range is kept, so the stripped text has
//! the same byte length and line count as the original. Replacements carry the
//! same invariant in a closed type. Later tokens therefore keep their byte
//! offsets and line numbers, including the distinction between one CRLF
//! sequence and separate CR/LF sequences, although a replacement may move a
//! sequence within its own span and therefore does not promise column fidelity.

use crate::{MergedName, DEFAULT_BINDING_ASSIGN, DEFAULT_BINDING_LET, DEFAULT_BINDING_VAR};

/// Module syntax the linker cannot express as Script text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StripError {
    /// Human-readable reason, already phrased as a diagnostic message body.
    pub(crate) reason: String,
}

impl StripError {
    fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

/// One source edit, whose kind exhaustively determines how its span is rebuilt.
struct SourceEdit {
    start: usize,
    end: usize,
    kind: ModuleSyntaxEdit,
}

enum ModuleSyntaxEdit {
    /// Keep line-terminator sequences; replace every other source byte with a
    /// space.
    Blank,
    /// Replace with text proved stable against this edit's source span.
    Replace(SpanStableReplacement),
}

/// Replacement text with the byte width and ordered ECMAScript
/// LineTerminatorSequences of the source span it erases.
///
/// There is deliberately no raw-string constructor. The only constructor
/// receives the erased source slice and reserves its terminator sequences
/// before it admits generated text, so neither length nor line structure can be
/// forgotten at a call site.
struct SpanStableReplacement(String);

enum SpanStableReplacementError {
    InvalidSpan,
    GeneratedLineTerminator,
    DoesNotFit,
}

impl SourceEdit {
    fn blank(source: &str, start: usize, end: usize) -> Result<Self, StripError> {
        source.get(start..end).ok_or_else(|| {
            StripError::new(format!(
                "module-syntax edit {start}..{end} is not a span of this source text"
            ))
        })?;
        Ok(Self {
            start,
            end,
            kind: ModuleSyntaxEdit::Blank,
        })
    }

    fn replace_around_padding(
        source: &str,
        start: usize,
        end: usize,
        before_padding: &str,
        after_padding: &str,
    ) -> Result<Self, SpanStableReplacementError> {
        let erased = source
            .get(start..end)
            .ok_or(SpanStableReplacementError::InvalidSpan)?;
        let suffix = source
            .get(end..)
            .ok_or(SpanStableReplacementError::InvalidSpan)?;
        let replacement =
            SpanStableReplacement::around_padding(erased, suffix, before_padding, after_padding)?;
        Ok(Self {
            start,
            end,
            kind: ModuleSyntaxEdit::Replace(replacement),
        })
    }
}

impl SpanStableReplacement {
    fn around_padding(
        erased: &str,
        suffix: &str,
        before_padding: &str,
        after_padding: &str,
    ) -> Result<Self, SpanStableReplacementError> {
        if contains_ecmascript_line_terminator(before_padding)
            || contains_ecmascript_line_terminator(after_padding)
        {
            return Err(SpanStableReplacementError::GeneratedLineTerminator);
        }

        let mut terminators = Vec::new();
        let mut cursor = 0usize;
        while cursor < erased.len() {
            if let Some(sequence) = ecmascript_line_terminator_sequence_at(erased, cursor) {
                terminators.push(sequence);
                cursor += sequence.len();
            } else {
                cursor += erased[cursor..].chars().next().map_or(1, char::len_utf8);
            }
        }
        // Relocating separate CR and LF sequences next to one another would
        // turn them into one CRLF sequence. Each internal pair had at least one
        // non-terminator byte between it in `erased`. The edit-boundary pair
        // below had `default` between the erased CR and the untouched suffix
        // LF. Reserving one of those displaced bytes as a barrier therefore
        // cannot make an otherwise-fitting rewrite overflow.
        let internal_barriers = terminators
            .windows(2)
            .filter(|pair| pair[0] == "\r" && pair[1] == "\n")
            .count();
        let trailing_barrier = terminators.last() == Some(&"\r") && suffix.starts_with('\n');
        let terminator_width = terminators
            .iter()
            .map(|sequence| sequence.len())
            .sum::<usize>();
        let Some(generated_width) = before_padding.len().checked_add(after_padding.len()) else {
            return Err(SpanStableReplacementError::DoesNotFit);
        };
        let Some(required_width) = generated_width
            .checked_add(terminator_width)
            .and_then(|width| width.checked_add(internal_barriers))
            .and_then(|width| width.checked_add(if trailing_barrier { 1 } else { 0 }))
        else {
            return Err(SpanStableReplacementError::DoesNotFit);
        };
        let Some(padding) = erased.len().checked_sub(required_width) else {
            return Err(SpanStableReplacementError::DoesNotFit);
        };

        let mut replacement = String::with_capacity(erased.len());
        replacement.push_str(before_padding);
        replacement.extend(core::iter::repeat_n(' ', padding));
        replacement.push_str(after_padding);
        for (index, sequence) in terminators.iter().enumerate() {
            if index != 0 && terminators[index - 1] == "\r" && *sequence == "\n" {
                replacement.push(' ');
            }
            replacement.push_str(sequence);
        }
        if trailing_barrier {
            replacement.push(' ');
        }

        debug_assert_eq!(replacement.len(), erased.len());
        let mut replacement_with_suffix = String::with_capacity(replacement.len() + suffix.len());
        replacement_with_suffix.push_str(&replacement);
        replacement_with_suffix.push_str(suffix);
        let mut erased_with_suffix = String::with_capacity(erased.len() + suffix.len());
        erased_with_suffix.push_str(erased);
        erased_with_suffix.push_str(suffix);
        debug_assert_eq!(
            collect_ecmascript_line_terminator_sequences(&replacement_with_suffix),
            collect_ecmascript_line_terminator_sequences(&erased_with_suffix)
        );
        Ok(Self(replacement))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

/// The ECMAScript LineTerminatorSequence beginning at this byte offset.
///
/// CRLF is returned as one sequence; a standalone CR or LF is returned as one
/// sequence of its own. Replacements retain exactly this sequence inventory.
fn ecmascript_line_terminator_sequence_at(source: &str, index: usize) -> Option<&str> {
    let remaining = source.get(index..)?;
    for sequence in ["\r\n", "\r", "\n", "\u{2028}", "\u{2029}"] {
        if remaining.starts_with(sequence) {
            return remaining.get(..sequence.len());
        }
    }
    None
}

fn contains_ecmascript_line_terminator(source: &str) -> bool {
    let mut index = 0usize;
    while index < source.len() {
        if ecmascript_line_terminator_sequence_at(source, index).is_some() {
            return true;
        }
        index += source[index..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

pub(super) fn collect_ecmascript_line_terminator_sequences(source: &str) -> Vec<&str> {
    let mut sequences = Vec::new();
    let mut index = 0usize;
    while index < source.len() {
        if let Some(sequence) = ecmascript_line_terminator_sequence_at(source, index) {
            sequences.push(sequence);
            index += sequence.len();
        } else {
            index += source[index..].chars().next().map_or(1, char::len_utf8);
        }
    }
    sequences
}

/// How to rewrite the `export default` keyword pair, decided by the
/// record rather than re-derived from the text.
///
/// Once line terminators are reserved, the two keywords guarantee 13 bytes for
/// generated code even in the narrowest split pair, `export\ndefault`. That is
/// invariant B1, and const assertion V2 in `crate::binding_names` holds
/// `MergedName::anonymous_default` to it at compile time using the very
/// keyword and binding constants. The replacement constructor also verifies
/// the actual source span before emitting [`DEFAULT_BINDING_LET`] and
/// [`DEFAULT_BINDING_ASSIGN`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefaultExportRewrite<'a> {
    /// No `export default` in this unit; one found anyway is a disagreement
    /// between the record and the text, and is reported.
    None,
    /// The declaration binds its own name, so only the keywords are deleted.
    DeleteKeywords,
    /// The declaration binds nothing spellable, so the keywords become
    /// `var <name> =` or `let <name> =`.
    Bind {
        /// Merged-scope name to declare.
        ///
        /// A [`MergedName`], so the only thing that can be written here is a
        /// name of the scope the declaration lands in. A `[[LocalName]]` — in
        /// particular the `*default*` this rewrite exists to replace — is
        /// `E0308`.
        name: &'a MergedName,
        /// Use `var` rather than `let`, for a hoistable declaration.
        hoisted: bool,
    },
}

/// Source-only grammar recorded during the original Module parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ModuleSyntax {
    edits: Vec<ParsedEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedEdit {
    Blank(scriptw_front::SourceSpan),
    Default {
        keywords: scriptw_front::SourceSpan,
        declaration_end: Option<usize>,
    },
}

impl ModuleSyntax {
    pub(super) fn from_parsed(source: &str, items: &boa_ast::ModuleItemList) -> Self {
        use boa_ast::ModuleItemSourceSyntax as Syntax;
        assert_eq!(
            items.items().len(),
            items.source_syntax().len(),
            "module rewriting requires provenance from the original parser"
        );
        // All boundaries are ordered, so map UTF-16 coordinates back to bytes
        // in one walk, even for a module containing thousands of declarations.
        let mut chars = source.char_indices();
        let mut utf16 = 0usize;
        let mut byte = 0usize;
        let mut byte_at = |position: boa_ast::LinearPosition| {
            while utf16 < position.pos() {
                let (offset, ch) = chars.next().expect("parsed position is inside source");
                utf16 += ch.len_utf16();
                byte = offset + ch.len_utf8();
            }
            assert_eq!(
                utf16,
                position.pos(),
                "parsed positions are ordered scalar boundaries"
            );
            byte
        };
        let mut edits = Vec::new();
        for syntax in items.source_syntax() {
            match *syntax {
                Syntax::Statement => {}
                Syntax::Declaration(span) | Syntax::ExportKeyword(span) => {
                    edits.push(ParsedEdit::Blank(scriptw_front::SourceSpan {
                        start: byte_at(span.start()),
                        end: byte_at(span.end()),
                    }));
                }
                Syntax::DefaultExport {
                    keywords,
                    declaration_end,
                } => {
                    let keywords = scriptw_front::SourceSpan {
                        start: byte_at(keywords.start()),
                        end: byte_at(keywords.end()),
                    };
                    let declaration_end = declaration_end.map(&mut byte_at);
                    edits.push(ParsedEdit::Default {
                        keywords,
                        declaration_end,
                    });
                }
            }
        }
        Self { edits }
    }

    pub(super) fn default_declaration_end(
        &self,
        rewrite: DefaultExportRewrite<'_>,
    ) -> Option<usize> {
        if !matches!(rewrite, DefaultExportRewrite::Bind { .. }) {
            return None;
        }
        self.edits.iter().find_map(|edit| match edit {
            ParsedEdit::Default {
                declaration_end, ..
            } => *declaration_end,
            ParsedEdit::Blank(_) => None,
        })
    }
}

/// Deletes module-only grammar at ranges retained by the original parser.
/// All untouched bytes and line terminator sequences remain in place.
pub(super) fn strip_module_syntax(
    source: &str,
    syntax: &ModuleSyntax,
    default_export: DefaultExportRewrite<'_>,
) -> Result<String, StripError> {
    let edits = syntax
        .edits
        .iter()
        .map(|edit| match *edit {
            ParsedEdit::Blank(span) => SourceEdit::blank(source, span.start, span.end),
            ParsedEdit::Default { keywords, .. } => {
                rewrite_default_keywords(source, keywords.start, keywords.end, default_export)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut rebuilt = String::with_capacity(source.len());
    let mut cursor = 0usize;
    for edit in edits {
        rebuilt.push_str(source.get(cursor..edit.start).ok_or_else(|| {
            StripError::new("module syntax ranges are not ordered spans of this source")
        })?);
        match edit.kind {
            ModuleSyntaxEdit::Replace(replacement) => rebuilt.push_str(replacement.as_str()),
            ModuleSyntaxEdit::Blank => {
                let erased = &source[edit.start..edit.end];
                let mut index = 0usize;
                while index < erased.len() {
                    if let Some(sequence) = ecmascript_line_terminator_sequence_at(erased, index) {
                        rebuilt.push_str(sequence);
                        index += sequence.len();
                    } else {
                        let width = erased[index..]
                            .chars()
                            .next()
                            .expect("nonempty suffix")
                            .len_utf8();
                        rebuilt.extend(core::iter::repeat_n(' ', width));
                        index += width;
                    }
                }
            }
        }
        cursor = edit.end;
    }
    rebuilt.push_str(&source[cursor..]);
    Ok(rebuilt)
}

fn rewrite_default_keywords(
    source: &str,
    start: usize,
    end: usize,
    rewrite: DefaultExportRewrite<'_>,
) -> Result<SourceEdit, StripError> {
    let (name, hoisted) = match rewrite {
        DefaultExportRewrite::None => {
            return Err(StripError::new(
                "`export default` in a module whose record has no default export",
            ))
        }
        DefaultExportRewrite::DeleteKeywords => return SourceEdit::blank(source, start, end),
        DefaultExportRewrite::Bind { name, hoisted } => (name, hoisted),
    };
    let keyword = if hoisted {
        DEFAULT_BINDING_VAR
    } else {
        DEFAULT_BINDING_LET
    };
    let name = name.as_str();
    let width = end.saturating_sub(start);
    let before_padding = format!("{keyword}{name}");
    SourceEdit::replace_around_padding(source, start, end, &before_padding, DEFAULT_BINDING_ASSIGN)
        .map_err(|error| match error {
            SpanStableReplacementError::DoesNotFit => StripError::new(format!(
                "`export default` binding `{name}` does not fit in the {width} bytes it replaces after preserving its line terminators")),
            SpanStableReplacementError::InvalidSpan => StripError::new(format!(
                "`export default` span {start}..{end} is not a span of this module's source text")),
            SpanStableReplacementError::GeneratedLineTerminator => StripError::new(
                "generated `export default` declaration head contains a line terminator"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{UnitLinkerSpelling, MAX_LINKABLE_MODULE_UNIT_ID};

    fn strip_parsed(source: &str, rewrite: DefaultExportRewrite<'_>) -> Result<String, StripError> {
        let parsed = scriptw_front::parse(source, scriptw_front::ParseOptions::module())
            .expect("test source must be a valid module");
        let syntax = parsed
            .as_module()
            .unwrap()
            .with_compiler_session(|module, _| ModuleSyntax::from_parsed(source, module.items()));
        strip_module_syntax(source, &syntax, rewrite)
    }

    fn strip(source: &str) -> String {
        strip_parsed(source, DefaultExportRewrite::None).expect("source should strip")
    }

    #[test]
    fn export_modifier_is_deleted_and_the_declaration_stays() {
        assert_eq!(
            strip("export const value = 41;"),
            "       const value = 41;"
        );
    }

    #[test]
    fn import_declaration_is_deleted_whole() {
        let stripped = strip("import { value } from \"./a.mjs\";\nprint(value);\n");
        assert_eq!(stripped.trim_start(), "\nprint(value);\n".trim_start());
        assert!(stripped.ends_with("print(value);\n"));
    }

    #[test]
    fn line_structure_and_length_are_preserved() {
        let source = "import { a } from \"m\";\nexport const b = a;\n";
        let stripped = strip(source);
        assert_eq!(stripped.len(), source.len());
        assert_eq!(
            stripped.lines().count(),
            source.lines().count(),
            "line count must survive stripping"
        );
    }

    /// Blanking is measured in bytes, not Unicode scalar values. A single
    /// space for `π` or `☿` would move the marker that follows the erased
    /// declarations and invalidate every later span.
    #[test]
    fn non_ascii_module_syntax_is_blanked_without_moving_later_bytes() {
        let source = "import { π as value } from \"☿\";\n\
                      export { value as \"☿\" };\n\
                      const after_unicode_module_syntax = 1;";
        let stripped = strip(source);

        assert_eq!(stripped.len(), source.len());
        assert_eq!(
            stripped.find("after_unicode_module_syntax"),
            source.find("after_unicode_module_syntax")
        );
        assert_eq!(
            collect_ecmascript_line_terminator_sequences(&stripped),
            collect_ecmascript_line_terminator_sequences(source)
        );
    }

    #[test]
    fn export_inside_a_string_literal_is_left_alone() {
        let source = "const s = \"export const x = 1;\";";
        assert_eq!(strip(source), source);
    }

    #[test]
    fn export_as_a_property_name_is_left_alone() {
        let source = "const o = { export: 1 };\no.export;";
        assert_eq!(strip(source), source);
    }

    #[test]
    fn export_inside_a_template_substitution_is_left_alone() {
        let source = "const t = `${ 1 } export`;";
        assert_eq!(strip(source), source);
    }

    #[test]
    fn regexp_literal_containing_a_quote_does_not_open_a_string() {
        let source = "const r = /'/;\nexport const x = 1;";
        assert_eq!(strip(source), "const r = /'/;\n       const x = 1;");
    }

    #[test]
    fn grammar_context_keeps_regex_after_blocks_and_control_heads_untouched() {
        for prefix in [
            "if (true) {} /export/.test('export');",
            "if (true) /}/.test('}');",
            "function f() {} /export default/.test('export default');",
            "const template = `${/export/.source} ${`${'import'}`}`;",
        ] {
            let source = format!("{prefix}\nexport const value = 1;");
            assert_eq!(strip(&source), format!("{prefix}\n       const value = 1;"));
        }
    }

    #[test]
    fn division_slash_does_not_consume_the_following_export_as_a_regexp() {
        let source = "const quotient = dividend / divisor;\nexport const x = 1;";
        assert_eq!(
            strip(source),
            "const quotient = dividend / divisor;\n       const x = 1;"
        );
    }

    #[test]
    fn comments_preserve_division_and_regexp_slash_contexts() {
        let source = "const line_divide = dividend // line\n / divisor;\nconst line_regexp = (// line\n /line/);\nconst block_divide = dividend /* block */ / divisor;\nconst block_regexp = (/* block */ /block/);\nexport const x = 1;";
        assert_eq!(
            strip(source),
            "const line_divide = dividend // line\n / divisor;\nconst line_regexp = (// line\n /line/);\nconst block_divide = dividend /* block */ / divisor;\nconst block_regexp = (/* block */ /block/);\n       const x = 1;"
        );
    }

    #[test]
    fn export_list_clause_is_deleted() {
        let source = "const x = 1;\nexport { x };\n";
        let stripped = strip(source);
        assert_eq!(stripped.len(), source.len());
        assert!(!stripped.contains("export"));
        assert!(stripped.contains("const x = 1;"));
    }

    /// A record that says the unit has no default export disagrees with text
    /// that does: reported, never guessed at.
    #[test]
    fn export_default_without_a_record_entry_is_reported() {
        let error = strip_parsed("export default 1;", DefaultExportRewrite::None)
            .expect_err("must be reported");
        assert!(error.reason.contains("export default"), "{}", error.reason);
    }

    /// The anonymous form becomes a declaration of the minted name, in place
    /// and without moving a byte of the initializer.
    #[test]
    fn anonymous_export_default_becomes_a_declaration_of_the_minted_name() {
        let source = "export default 42;\nprint(1);\n";
        let stripped = strip_parsed(
            source,
            DefaultExportRewrite::Bind {
                name: &UnitLinkerSpelling::unreserved(0).anonymous_default(),
                hoisted: false,
            },
        )
        .expect("source should strip");
        assert_eq!(stripped.len(), source.len());
        assert_eq!(stripped, "let $d0$     = 42;\nprint(1);\n");
    }

    /// The grammar permits line terminators between `export` and `default`.
    /// Reserve every terminator byte before fitting the widest binding name,
    /// including CRLF as one sequence and the two Unicode forms.
    #[test]
    fn split_anonymous_defaults_preserve_bytes_and_ordered_line_sequences_at_the_cap() {
        let name = UnitLinkerSpelling::unreserved(MAX_LINKABLE_MODULE_UNIT_ID).anonymous_default();
        assert_eq!(name.as_str(), "$d9999$");

        let forms = [
            ("42", false, "let $d9999$"),
            ("function () {}", true, "var $d9999$"),
        ];
        for trivia in [
            "\n",
            "\r\n",
            "\u{2028}",
            "\u{2029}",
            "/*☿\r\n\u{2028}π\u{2029}*/",
        ] {
            for (initializer, hoisted, declaration) in forms {
                let source =
                    format!("export{trivia}default {initializer};\nconst after_split_default = 1;");
                let stripped = strip_parsed(
                    &source,
                    DefaultExportRewrite::Bind {
                        name: &name,
                        hoisted,
                    },
                )
                .expect("split default should strip");

                assert_eq!(stripped.len(), source.len(), "{trivia:?} {initializer}");
                assert_eq!(
                    stripped.find("after_split_default"),
                    source.find("after_split_default"),
                    "{trivia:?} {initializer}"
                );
                assert!(stripped.starts_with(declaration), "got {stripped}");
                let initializer_offset = stripped
                    .find(initializer)
                    .expect("initializer should stay present");
                let terminators = collect_ecmascript_line_terminator_sequences(trivia).concat();
                assert!(
                    stripped[..initializer_offset].ends_with(&format!("={terminators} ")),
                    "got {stripped}"
                );
                assert_eq!(
                    collect_ecmascript_line_terminator_sequences(&stripped),
                    collect_ecmascript_line_terminator_sequences(&source),
                    "got {stripped}"
                );
            }
        }
    }

    /// A standalone CR and a later standalone LF are two line-terminator
    /// sequences. Relocating their raw code points adjacently would silently
    /// collapse them into one CRLF sequence and move every later line number.
    #[test]
    fn relocated_separate_cr_and_lf_sequences_keep_a_non_terminator_barrier() {
        let source = "export/*\rseparate\n*/default 42;\nconst after = 1;";
        let stripped = strip_parsed(
            source,
            DefaultExportRewrite::Bind {
                name: &UnitLinkerSpelling::unreserved(0).anonymous_default(),
                hoisted: false,
            },
        )
        .expect("separated CR and LF should strip");

        assert_eq!(stripped.len(), source.len());
        assert_eq!(stripped.find("after"), source.find("after"));
        assert_eq!(
            collect_ecmascript_line_terminator_sequences(&stripped),
            vec!["\r", "\n", "\n"]
        );
        assert_eq!(
            collect_ecmascript_line_terminator_sequences(&stripped),
            collect_ecmascript_line_terminator_sequences(source)
        );
        assert!(stripped.contains("=\r \n 42;"), "got {stripped}");
    }

    /// The untouched suffix participates in line-sequence grouping too. A
    /// relocated standalone CR at the end of the edit must not fuse with an LF
    /// that begins the initializer suffix.
    #[test]
    fn relocated_cr_keeps_a_barrier_before_an_untouched_suffix_lf_at_the_cap() {
        let source = "export\rdefault\n42;\nconst after_boundary = 1;";
        let stripped = strip_parsed(
            source,
            DefaultExportRewrite::Bind {
                name: &UnitLinkerSpelling::unreserved(MAX_LINKABLE_MODULE_UNIT_ID)
                    .anonymous_default(),
                hoisted: false,
            },
        )
        .expect("edit-boundary CR and LF should stay separate");

        assert_eq!(stripped.len(), source.len());
        assert_eq!(
            stripped.find("after_boundary"),
            source.find("after_boundary")
        );
        assert_eq!(
            collect_ecmascript_line_terminator_sequences(&stripped),
            vec!["\r", "\n", "\n"]
        );
        assert_eq!(
            collect_ecmascript_line_terminator_sequences(&stripped),
            collect_ecmascript_line_terminator_sequences(source)
        );
        assert!(
            stripped.starts_with("let $d9999$=\r \n42;"),
            "got {stripped}"
        );
    }

    /// Both the lookahead scanner inside an export and the top-level scanner
    /// must end `//` at every ECMAScript LineTerminatorSequence, not only LF.
    #[test]
    fn line_comments_end_at_cr_ls_and_ps_around_a_split_default() {
        for terminator in ["\r", "\u{2028}", "\u{2029}"] {
            let source = format!(
                "// leading{terminator}export// between{terminator}default 42;\nconst after = 1;"
            );
            let stripped = strip_parsed(
                &source,
                DefaultExportRewrite::Bind {
                    name: &UnitLinkerSpelling::unreserved(0).anonymous_default(),
                    hoisted: false,
                },
            )
            .expect("line comments should stop at an ECMAScript line terminator");

            assert_eq!(stripped.len(), source.len(), "{terminator:?}");
            assert_eq!(stripped.find("after"), source.find("after"));
            assert!(stripped.contains("let $d0$"), "got {stripped}");
            assert_eq!(
                collect_ecmascript_line_terminator_sequences(&stripped),
                collect_ecmascript_line_terminator_sequences(&source),
                "got {stripped}"
            );
        }
    }

    /// Keyword lookahead uses scalar boundaries too: UTF-8 whitespace after a
    /// keyword must not be mistaken for another identifier byte.
    #[test]
    fn unicode_whitespace_terminates_looked_ahead_module_keywords() {
        let default_source = "export default\u{2028}42;\nconst after_default = 1;";
        let stripped_default = strip_parsed(
            default_source,
            DefaultExportRewrite::Bind {
                name: &UnitLinkerSpelling::unreserved(0).anonymous_default(),
                hoisted: false,
            },
        )
        .expect("line separator should terminate the default keyword");
        assert_eq!(stripped_default.len(), default_source.len());
        assert!(stripped_default.starts_with("let $d0$"));
        assert_eq!(
            stripped_default.find("after_default"),
            default_source.find("after_default")
        );

        let export_source =
            "const value = 1;\nexport { value } from\u{00A0}\"m\";\nconst after_export = 1;";
        let stripped_export = strip(export_source);
        assert_eq!(stripped_export.len(), export_source.len());
        assert!(!stripped_export.contains("from\u{00A0}\"m\""));
        assert_eq!(
            stripped_export.find("after_export"),
            export_source.find("after_export")
        );

        let import_source = "import \"m\" with\u{FEFF}{ type: \"json\" };\nconst after_import = 1;";
        let stripped_import = strip(import_source);
        assert_eq!(stripped_import.len(), import_source.len());
        assert!(!stripped_import.contains("type: \"json\""));
        assert_eq!(
            stripped_import.find("after_import"),
            import_source.find("after_import")
        );

        assert!(scriptw_front::parse(
            "export\u{0085}const value = 1;",
            scriptw_front::ParseOptions::module()
        )
        .is_err());
    }

    /// The scanner must recognize non-ASCII whitespace both before a word and
    /// while advancing through one. These are distinct production branches
    /// from the keyword lookahead above.
    #[test]
    fn unicode_whitespace_is_dispatched_and_terminates_scanned_words() {
        let leading_source = "\u{FEFF}export default 42;\nconst after_leading = 1;";
        let stripped_leading = strip_parsed(
            leading_source,
            DefaultExportRewrite::Bind {
                name: &UnitLinkerSpelling::unreserved(0).anonymous_default(),
                hoisted: false,
            },
        )
        .expect("leading byte-order mark should be scanner whitespace");
        assert_eq!(stripped_leading.len(), leading_source.len());
        assert!(stripped_leading.contains("let $d0$"));
        assert_eq!(
            stripped_leading.find("after_leading"),
            leading_source.find("after_leading")
        );

        let word_source = "export\u{00A0}default 42;\nconst after_word = 1;";
        let stripped_word = strip_parsed(
            word_source,
            DefaultExportRewrite::Bind {
                name: &UnitLinkerSpelling::unreserved(0).anonymous_default(),
                hoisted: false,
            },
        )
        .expect("non-breaking space should terminate the scanned export word");
        assert_eq!(stripped_word.len(), word_source.len());
        assert!(stripped_word.contains("let $d0$"));
        assert_eq!(
            stripped_word.find("after_word"),
            word_source.find("after_word")
        );
    }

    #[test]
    fn a_generated_replacement_cannot_add_a_line_terminator() {
        assert!(matches!(
            SpanStableReplacement::around_padding("export default", "", "let $d0$\n", "="),
            Err(SpanStableReplacementError::GeneratedLineTerminator)
        ));
    }

    /// A hoistable anonymous default keeps being initialized before the body
    /// runs, which `let` would have replaced with a TDZ.
    #[test]
    fn a_hoistable_anonymous_default_is_declared_with_var() {
        let stripped = strip_parsed(
            "export default function () {}",
            DefaultExportRewrite::Bind {
                name: &UnitLinkerSpelling::unreserved(3).anonymous_default(),
                hoisted: true,
            },
        )
        .expect("source should strip");
        assert_eq!(stripped, "var $d3$     = function () {}");
    }

    /// `export default function f() {}` already binds `f`, so the keywords are
    /// simply deleted and the declaration stays a declaration.
    #[test]
    fn a_named_export_default_only_loses_its_keywords() {
        let source = "export default function f() {}\n";
        let stripped = strip_parsed(source, DefaultExportRewrite::DeleteKeywords)
            .expect("source should strip");
        assert_eq!(stripped.len(), source.len());
        assert_eq!(stripped, "               function f() {}\n");
    }

    #[test]
    fn export_star_from_is_deleted_whole() {
        let source = "export * from \"./m.mjs\";\nprint(1);\n";
        let stripped = strip(source);
        assert_eq!(stripped.len(), source.len());
        assert!(!stripped.contains("export"), "got {stripped}");
        assert!(stripped.ends_with("print(1);\n"));
    }

    #[test]
    fn dynamic_import_call_is_not_a_declaration() {
        let source = "import(\"m\").then(f);";
        assert_eq!(strip(source), source);
    }

    #[test]
    fn import_meta_is_not_a_declaration() {
        let source = "print(import.meta.url);";
        assert_eq!(strip(source), source);
    }

    #[test]
    fn import_with_attributes_is_deleted_whole() {
        let source = "import a from \"m\" with { type: \"json\" };\n";
        let stripped = strip(source);
        assert_eq!(stripped.trim(), "");
    }
}
