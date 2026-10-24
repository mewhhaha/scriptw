use boa_ast::{Module, Script, scope::Scope};
use boa_interner::Interner;
use boa_parser::{Parser, Source};
use std::ops::Deref;
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;

// The closed domain of pre-evaluation rejection codes and the one table that
// classifies boa's static-semantics messages into it. See
// `docs/rust-rewrite/contracts/early-error-taxonomy.md`.
mod dynamic_function;
mod early_error_code;
mod eval_source;
mod json;
mod strict_callable_bindings;
mod utf16_source;

// Generated source uses exactly the lexer's decoded-code-point grammar.
// Re-export the authority instead of maintaining another Unicode classifier.
pub use boa_parser::lexer::{is_identifier_part, is_identifier_start};
pub use dynamic_function::{
    FunctionParseKind, prepare_dynamic_function, prepare_dynamic_function_utf16,
};
pub use eval_source::{
    DirectEvalParseContext, EvalInvocationContext, EvalParseContext, prepare_eval_source,
    prepare_eval_source_utf16,
};
pub use json::{JsonParseError, JsonValue, ParsedJson, parse_json};
pub use utf16_source::{
    JS_STRING_SURROGATE_SENTINEL, SourceTextEncoding, decode_js_string_utf16,
    encode_js_string_utf16,
};

pub use early_error_code::{
    EarlyErrorCode, NO_EARLY_ERROR_CODE, ParseClassified, classify_parse_failure,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseGoal {
    Script,
    Module,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseOptions {
    pub goal: ParseGoal,
    pub filename: Option<String>,
}

impl ParseOptions {
    pub fn script() -> Self {
        Self {
            goal: ParseGoal::Script,
            filename: None,
        }
    }

    pub fn module() -> Self {
        Self {
            goal: ParseGoal::Module,
            filename: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUnit {
    pub goal: ParseGoal,
    pub filename: Option<String>,
    /// Exact UTF-8 host text or losslessly encoded UTF-16, selected by `encoding`.
    pub source_text: String,
    pub encoding: SourceTextEncoding,
}

/// A successfully parsed Script compilation unit.
///
/// The syntax tree and its interner are one allocation and cannot be separated
/// or replaced. That relationship matters: every `Sym` in Boa's tree belongs
/// to precisely this interner. Compiler stages can only borrow the pair through
/// [`ParsedScript::with_compiler_session`], so neither half can escape with a
/// longer lifetime than the other.
#[derive(Clone)]
pub struct ParsedScript {
    source: SourceUnit,
    syntax: Rc<ScriptSyntax>,
}

struct ScriptSyntax {
    ast: Script,
    interner: Interner,
    source_text: boa_ast::SourceText,
}

/// Identity of one successful Script parse, retaining its actual owned syntax
/// allocation. Cloning a parsed Script preserves this identity; parsing equal
/// source text again creates a different owner.
///
/// The erased allocation cannot expose or replace the syntax/interner pair.
/// Its only consumer is compilation-local source ownership, never a source hash
/// or a process-wide parser counter.
#[derive(Clone)]
pub struct ParsedScriptIdentity(Rc<dyn std::any::Any>);

impl PartialEq for ParsedScriptIdentity {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ParsedScriptIdentity {}

impl std::hash::Hash for ParsedScriptIdentity {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::ptr::hash(Rc::as_ptr(&self.0).cast::<()>(), state);
    }
}

impl core::fmt::Debug for ParsedScriptIdentity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ParsedScriptIdentity")
    }
}

/// Identity retaining one Module AST/interner allocation, independently of
/// source-text metadata equality. The owner cannot expose either syntax half.
#[derive(Clone)]
pub struct ParsedModuleIdentity(Rc<dyn std::any::Any>);

impl PartialEq for ParsedModuleIdentity {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ParsedModuleIdentity {}

impl core::fmt::Debug for ParsedModuleIdentity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ParsedModuleIdentity")
    }
}

/// A successfully parsed Module compilation unit.
///
/// Like [`ParsedScript`], this owns the AST and the exact interner that produced
/// it. A module record therefore consumes parsed syntax rather than source text
/// that it could accidentally parse a second time.
#[derive(Clone)]
pub struct ParsedModule {
    source: SourceUnit,
    syntax: Rc<ModuleSyntax>,
}

struct ModuleSyntax {
    ast: Module,
    interner: Interner,
    source_text: boa_ast::SourceText,
}

/// The closed result of a successful parse.
///
/// Keeping the parse goal in the variant makes passing Script syntax to a
/// Module-only static-semantics operation a type error after the variant is
/// selected. Raw [`SourceUnit`] metadata is deliberately a different type and
/// is not accepted by the IR lowerer.
#[derive(Clone)]
pub enum ParsedSource {
    Script(ParsedScript),
    Module(ParsedModule),
}

impl ParsedSource {
    #[must_use]
    pub const fn source(&self) -> &SourceUnit {
        match self {
            Self::Script(source) => source.source(),
            Self::Module(source) => source.source(),
        }
    }

    #[must_use]
    pub const fn goal(&self) -> ParseGoal {
        match self {
            Self::Script(_) => ParseGoal::Script,
            Self::Module(_) => ParseGoal::Module,
        }
    }

    #[must_use]
    pub const fn as_script(&self) -> Option<&ParsedScript> {
        match self {
            Self::Script(source) => Some(source),
            Self::Module(_) => None,
        }
    }

    #[must_use]
    pub const fn as_module(&self) -> Option<&ParsedModule> {
        match self {
            Self::Module(source) => Some(source),
            Self::Script(_) => None,
        }
    }
}

impl ParsedScript {
    #[must_use]
    pub const fn source(&self) -> &SourceUnit {
        &self.source
    }

    /// Exact semantic source units retained with the AST/interner allocation.
    /// Parser linear spans index this owner, never its encoded presentation.
    #[doc(hidden)]
    #[must_use]
    pub fn compiler_source_utf16(&self) -> &[u16] {
        self.syntax
            .source_text
            .get_code_points_from_pos(boa_ast::LinearPosition::new(0))
    }

    /// Gives compiler source ownership the identity of this AST/interner
    /// allocation. Metadata equality deliberately remains source equality.
    #[doc(hidden)]
    #[must_use]
    pub fn syntax_identity(&self) -> ParsedScriptIdentity {
        ParsedScriptIdentity(self.syntax.clone())
    }

    /// Borrows Boa's syntax implementation as one non-escaping compiler
    /// session.
    ///
    /// This is an internal workspace seam, not Scriptw's syntax or IR contract.
    /// Boa types are intentionally absent from every stored public field and
    /// from all returned Scriptw IR. Keeping the callback here makes an AST and
    /// the wrong interner impossible to pair and gives a future parser swap one
    /// narrow adapter to replace.
    #[doc(hidden)]
    pub fn with_compiler_session<R>(
        &self,
        consume: impl for<'syntax> FnOnce(&'syntax Script, &'syntax Interner) -> R,
    ) -> R {
        consume(&self.syntax.ast, &self.syntax.interner)
    }
}

impl ParsedModule {
    #[must_use]
    pub const fn source(&self) -> &SourceUnit {
        &self.source
    }

    /// Retains this Module's exact syntax/interner owner without copying text.
    #[doc(hidden)]
    #[must_use]
    pub fn syntax_identity(&self) -> ParsedModuleIdentity {
        ParsedModuleIdentity(self.syntax.clone())
    }

    /// Exact semantic source units retained with the AST/interner allocation.
    /// Parser linear spans index this owner, never its encoded presentation.
    #[doc(hidden)]
    #[must_use]
    pub fn compiler_source_utf16(&self) -> &[u16] {
        self.syntax
            .source_text
            .get_code_points_from_pos(boa_ast::LinearPosition::new(0))
    }

    /// Module counterpart of [`ParsedScript::with_compiler_session`].
    #[doc(hidden)]
    pub fn with_compiler_session<R>(
        &self,
        consume: impl for<'syntax> FnOnce(&'syntax Module, &'syntax Interner) -> R,
    ) -> R {
        consume(&self.syntax.ast, &self.syntax.interner)
    }
}

impl Deref for ParsedScript {
    type Target = SourceUnit;

    fn deref(&self) -> &Self::Target {
        self.source()
    }
}

impl Deref for ParsedModule {
    type Target = SourceUnit;

    fn deref(&self) -> &Self::Target {
        self.source()
    }
}

impl core::fmt::Debug for ParsedScript {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("ParsedScript").field(&self.source).finish()
    }
}

impl core::fmt::Debug for ParsedModule {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("ParsedModule").field(&self.source).finish()
    }
}

impl core::fmt::Debug for ParsedSource {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Script(source) => source.fmt(f),
            Self::Module(source) => source.fmt(f),
        }
    }
}

impl PartialEq for ParsedScript {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Eq for ParsedScript {}

impl PartialEq for ParsedModule {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Eq for ParsedModule {}

impl PartialEq for ParsedSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Script(left), Self::Script(right)) => left == right,
            (Self::Module(left), Self::Module(right)) => left == right,
            (Self::Script(_), Self::Module(_)) | (Self::Module(_), Self::Script(_)) => false,
        }
    }
}

impl Eq for ParsedSource {}

/// What sort of thing the front end rejected. A **return type**, not a field:
/// it is a function of [`ParseCode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseDiagnosticKind {
    MalformedJavaScript,
    UnsupportedParserFeature,
}

/// When the rejection was decided. A **return type**, not a field.
///
/// 16.1.4 `ParseScript` and 16.2.1.6.1 `ParseModule` fix the reporting phase per
/// producing operation; clause 17 makes it a property of *where* the rejection
/// comes from, never a free parameter of a call site. Storing it as a field was
/// the opportunity for one condition to be reported under two phases depending
/// on which path found it, and that had already happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseDiagnosticPhase {
    Parse,
    Early,
}

/// Half-open UTF-8 byte range in `SourceUnit::source_text`. For UTF-16
/// input these are bytes of the lossless transport encoding, not code units.
/// Boa positions are one-based lines and Unicode code-point columns; CRLF
/// occupies one line break. EOF is represented by an empty range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

/// Everything [`parse`] can report, as one closed domain.
///
/// The two `P_...` codes are **compiler-gap** codes, not spec rejections, and
/// keeping them out of [`EarlyErrorCode`] is deliberate: an `EarlyErrorCode`
/// must always name a program that ECMAScript rejects. A source boa could not
/// read, or a parse that aborted, is a fact about this front end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseCode {
    /// Boa rejected the source and we do not model the wording.
    Malformed,
    /// Boa's parser aborted; the panic was caught in [`parse`].
    UnsupportedParserFeature,
    /// A modelled spec rejection, projected from a typed parser error or
    /// classified by [`classify_parse_failure`] at the boundary every unit
    /// uses, so one source cannot report under two codes depending on whether it
    /// was the entry file or an import.
    ///
    /// The payload is [`ParseClassified`], not a bare [`EarlyErrorCode`]: this
    /// variant reports at [`ParseDiagnosticPhase::Early`], and a link-only code
    /// reported there is one condition under two phases from two paths. The
    /// witness makes that `error[E0308]` at the call site rather than a
    /// convention the table's assertion P7 can only state for the table.
    Early(ParseClassified),
}

impl ParseCode {
    /// The single spelling authority for the two `P_...` codes; an early code
    /// delegates to [`EarlyErrorCode::wire_name`], which owns all of
    /// the `E_...` spellings.
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Malformed => "P_PARSE_MALFORMED",
            Self::UnsupportedParserFeature => "P_PARSE_UNSUPPORTED",
            Self::Early(code) => code.code().wire_name(),
        }
    }

    #[must_use]
    pub const fn kind(self) -> ParseDiagnosticKind {
        match self {
            Self::Malformed | Self::Early(_) => ParseDiagnosticKind::MalformedJavaScript,
            Self::UnsupportedParserFeature => ParseDiagnosticKind::UnsupportedParserFeature,
        }
    }

    #[must_use]
    pub const fn phase(self) -> ParseDiagnosticPhase {
        match self {
            Self::Early(_) => ParseDiagnosticPhase::Early,
            Self::Malformed | Self::UnsupportedParserFeature => ParseDiagnosticPhase::Parse,
        }
    }

    /// The error the program would have thrown, if the spec says it throws one.
    ///
    /// The one `"SyntaxError"` literal in this crate. 16.1.4 and 16.2.1.6.1
    /// both return "a List of **SyntaxError** objects", and every
    /// `parse`/`resolution` negative in the pinned test262 suite is a
    /// `SyntaxError` — there is no second inhabitant to choose between. It
    /// cannot be `scriptw_ir::NativeErrorKind` because that type lives in a
    /// crate this one is *below*; closing that requires moving
    /// `NativeErrorKind` down and is another lane's file (ledger L2).
    ///
    /// **`UnsupportedParserFeature` returns `None`, and that is a fix.** It is
    /// the caught-panic case ([`parse`]: "parser aborted while handling
    /// source") — a compiler gap, not a program ECMAScript rejects. Returning
    /// `"SyntaxError"` for it made `compile_negative_error_matches` score a
    /// **pass** for any `parse`/`SyntaxError` negative whose source merely
    /// crashed boa's parser, because `phase()` is already `Parse`. Clause 17:
    /// an implementation "must not treat other kinds of error as early errors".
    /// This is the same shape `scriptw_ir::IrDiagnosticKind::error_type`
    /// already has, and what `module_parse_failure_diagnostic`'s doc comment
    /// forbids in words.
    #[must_use]
    pub const fn error_type(self) -> Option<&'static str> {
        match self {
            // boa read the source and rejected it: a real syntax error, whether
            // or not the message-pattern table models its wording.
            Self::Malformed | Self::Early(_) => Some("SyntaxError"),
            Self::UnsupportedParserFeature => None,
        }
    }
}

/// A front-end rejection: one closed code, plus payload.
///
/// `kind`, `phase` and `error_type` used to be independent fields beside
/// `code`; they are now accessors derived from `code`, so no call site can pair
/// them inconsistently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDiagnostic {
    pub code: ParseCode,
    pub span: Option<SourceSpan>,
    pub message: String,
}

impl ParseDiagnostic {
    #[must_use]
    pub const fn kind(&self) -> ParseDiagnosticKind {
        self.code.kind()
    }

    #[must_use]
    pub const fn phase(&self) -> ParseDiagnosticPhase {
        self.code.phase()
    }

    #[must_use]
    pub const fn error_type(&self) -> Option<&'static str> {
        self.code.error_type()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    diagnostic: ParseDiagnostic,
    message: String,
}

impl ParseError {
    pub fn malformed(message: impl Into<String>, span: Option<SourceSpan>) -> Self {
        Self::new(ParseCode::Malformed, message, span)
    }

    pub fn unsupported_parser_feature(
        message: impl Into<String>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self::new(ParseCode::UnsupportedParserFeature, message, span)
    }

    /// A modelled spec rejection. The `error_type` parameter is gone: passing
    /// `"SyntaxError"` here was never a choice, and passing anything else was
    /// never correct.
    ///
    /// The code parameter is a [`ParseClassified`], not a bare
    /// [`EarlyErrorCode`]: this constructor reports at
    /// [`ParseDiagnosticPhase::Early`], so it must not be able to name a
    /// link-only condition. Obtain one from [`classify_parse_failure`], or —
    /// for a producer that names its code directly — from
    /// [`ParseClassified::from_parse_table`] in a `const` initializer.
    pub fn early_error(
        code: ParseClassified,
        message: impl Into<String>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self::new(ParseCode::Early(code), message, span)
    }

    fn new(code: ParseCode, message: impl Into<String>, span: Option<SourceSpan>) -> Self {
        let message = message.into();
        Self {
            diagnostic: ParseDiagnostic {
                code,
                span,
                message: message.clone(),
            },
            message,
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn diagnostic(&self) -> &ParseDiagnostic {
        &self.diagnostic
    }
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ParseError {}

pub fn parse(
    source_text: impl Into<String>,
    options: ParseOptions,
) -> Result<ParsedSource, ParseError> {
    let source_text = source_text.into();
    let mut interner = Interner::default();
    let scope = Scope::new_global();
    let source = if let Some(filename) = &options.filename {
        Source::from_bytes(source_text.as_bytes()).with_path(std::path::Path::new(filename))
    } else {
        Source::from_bytes(source_text.as_bytes())
    };

    let ast = parse_with_boundary(&source_text, || match options.goal {
        ParseGoal::Script => Parser::new(source)
            .parse_script_with_source(&scope, &mut interner)
            .map(|(ast, source)| ParsedAst::Script(ast, source)),
        ParseGoal::Module => Parser::new(source)
            .parse_module_with_source(&scope, &mut interner)
            .map(|(ast, source)| ParsedAst::Module(ast, source)),
    })?;

    match &ast {
        ParsedAst::Script(ast, _) => strict_callable_bindings::validate(ast, |position| {
            source_span_at_position(&source_text, position)
        }),
        ParsedAst::Module(ast, _) => strict_callable_bindings::validate(ast, |position| {
            source_span_at_position(&source_text, position)
        }),
    }?;

    let source = SourceUnit {
        goal: options.goal,
        filename: options.filename,
        source_text,
        encoding: SourceTextEncoding::Utf8,
    };
    Ok(parsed_source(ast, source, interner))
}

/// Parses a prepared Script supplied as an ECMAScript string. Module host
/// files keep their separate UTF-8 byte-range owner through `parse`.
pub fn parse_script_utf16(source_units: &[u16]) -> Result<ParsedScript, ParseError> {
    let mut interner = Interner::default();
    let scope = Scope::new_global();
    let (ast, source_text) = parse_utf16_with_boundary(source_units, || {
        Parser::new(Source::from_utf16(source_units))
            .parse_script_with_source(&scope, &mut interner)
    })?;
    strict_callable_bindings::validate(&ast, |position| {
        source_span_utf16_at_position(source_units, position)
    })?;
    Ok(ParsedScript {
        source: SourceUnit {
            goal: ParseGoal::Script,
            filename: None,
            source_text: encode_js_string_utf16(source_units),
            encoding: SourceTextEncoding::EncodedUtf16,
        },
        syntax: Rc::new(ScriptSyntax {
            ast,
            interner,
            source_text,
        }),
    })
}

fn parsed_source(ast: ParsedAst, source: SourceUnit, interner: Interner) -> ParsedSource {
    match ast {
        ParsedAst::Script(ast, source_text) => ParsedSource::Script(ParsedScript {
            source,
            syntax: Rc::new(ScriptSyntax {
                ast,
                interner,
                source_text,
            }),
        }),
        ParsedAst::Module(ast, source_text) => ParsedSource::Module(ParsedModule {
            source,
            syntax: Rc::new(ModuleSyntax {
                ast,
                interner,
                source_text,
            }),
        }),
    }
}

fn parse_with_boundary<T>(
    source_text: &str,
    operation: impl FnOnce() -> Result<T, boa_parser::Error>,
) -> Result<T, ParseError> {
    parse_with_boundary_and_span(operation, |error| parse_error_span(source_text, error))
}

fn parse_utf16_with_boundary<T>(
    source_units: &[u16],
    operation: impl FnOnce() -> Result<T, boa_parser::Error>,
) -> Result<T, ParseError> {
    parse_with_boundary_and_span(operation, |error| {
        parse_error_span_utf16(source_units, error)
    })
}

fn parse_with_boundary_and_span<T>(
    operation: impl FnOnce() -> Result<T, boa_parser::Error>,
    span: impl FnOnce(&boa_parser::Error) -> Option<SourceSpan>,
) -> Result<T, ParseError> {
    match panic::catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(parsed)) => Ok(parsed),
        Ok(Err(err)) => {
            let typed_code = ParseClassified::from_boa_error(&err);
            let span = span(&err);
            let err = err.to_string();
            let message = format!("parse error: {err}");
            // The typed condition already carries its code. Other variants use
            // Boa's bare message before presentation context is added.
            if let Some(code) = typed_code.or_else(|| classify_parse_failure(&err)) {
                Err(ParseError::early_error(code, message, span))
            } else {
                Err(ParseError::malformed(message, span))
            }
        }
        Err(payload) => Err(ParseError::unsupported_parser_feature(
            format!(
                "parse unsupported by current frontend: parser aborted while handling source ({})",
                parser_abort_message(&payload)
            ),
            None,
        )),
    }
}

enum ParsedAst {
    Script(Script, boa_ast::SourceText),
    Module(Module, boa_ast::SourceText),
}

fn parse_error_position(error: &boa_parser::Error) -> Option<boa_ast::Position> {
    Some(match error {
        boa_parser::Error::Expected { span, .. } | boa_parser::Error::Unexpected { span, .. } => {
            span.start()
        }
        boa_parser::Error::General { position, .. }
        | boa_parser::Error::ClassMethodHasDirectSuper { position }
        | boa_parser::Error::Lex {
            err: boa_parser::lexer::Error::Syntax(_, position),
        } => *position,
        boa_parser::Error::AbruptEnd
        | boa_parser::Error::Lex {
            err: boa_parser::lexer::Error::IO(_),
        } => return None,
    })
}

fn parse_error_span(source_text: &str, error: &boa_parser::Error) -> Option<SourceSpan> {
    source_span_at_position(source_text, parse_error_position(error)?)
}

fn source_span_at_position(source_text: &str, position: boa_ast::Position) -> Option<SourceSpan> {
    let start = byte_offset_for_line_col(
        source_text,
        position.line_number() as usize,
        position.column_number() as usize,
    )?;
    let width = source_text[start..]
        .chars()
        .next()
        .map(char::len_utf8)
        .unwrap_or_default();
    Some(SourceSpan {
        start,
        end: start + width,
    })
}

/// Diagnostic byte ranges refer to the explicitly encoded SourceUnit text.
/// Traverse actual source code points for line/column ownership, then encode
/// the selected boundary; marker characters are ordinary user data here.
fn parse_error_span_utf16(source: &[u16], error: &boa_parser::Error) -> Option<SourceSpan> {
    source_span_utf16_at_position(source, parse_error_position(error)?)
}

fn source_span_utf16_at_position(
    source: &[u16],
    position: boa_ast::Position,
) -> Option<SourceSpan> {
    let target = (
        position.line_number() as usize,
        position.column_number() as usize,
    );
    let (mut offset, mut line, mut column) = (0, 1, 1);
    while offset < source.len() {
        let first = source[offset];
        let width = if (0xD800..=0xDBFF).contains(&first)
            && source
                .get(offset + 1)
                .is_some_and(|unit| (0xDC00..=0xDFFF).contains(unit))
        {
            2
        } else {
            1
        };
        if (line, column) == target {
            let start = encode_js_string_utf16(&source[..offset]).len();
            let end = start + encode_js_string_utf16(&source[offset..offset + width]).len();
            return Some(SourceSpan { start, end });
        }
        offset += width;
        match first {
            0x000D | 0x000A | 0x2028 | 0x2029 => {
                if first == 0x000D && source.get(offset) == Some(&0x000A) {
                    offset += 1;
                }
                line += 1;
                column = 1;
            }
            _ => column += 1,
        }
    }
    ((line, column) == target).then(|| {
        let end = encode_js_string_utf16(source).len();
        SourceSpan { start: end, end }
    })
}

fn byte_offset_for_line_col(
    source_text: &str,
    line_one_based: usize,
    code_point_column_one_based: usize,
) -> Option<usize> {
    let mut characters = source_text.char_indices().peekable();
    let (mut current_line, mut current_col) = (1, 1);
    while let Some((offset, character)) = characters.next() {
        if (current_line, current_col) == (line_one_based, code_point_column_one_based) {
            return Some(offset);
        }
        match character {
            '\r' | '\n' | '\u{2028}' | '\u{2029}' => {
                if character == '\r' && characters.peek().is_some_and(|(_, next)| *next == '\n') {
                    characters.next();
                }
                current_line += 1;
                current_col = 1;
            }
            _ => current_col += 1,
        }
    }
    ((current_line, current_col) == (line_one_based, code_point_column_one_based))
        .then_some(source_text.len())
}

fn parser_abort_message(payload: &Box<dyn core::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string abort payload".to_string()
    }
}

#[cfg(test)]
mod tests {
    #[path = "declarations.rs"]
    mod declarations;
    #[path = "grammar.rs"]
    mod grammar;
    #[path = "parameters.rs"]
    mod parameters;
    #[path = "source_text.rs"]
    mod source_text;
    #[path = "super_grammar.rs"]
    mod super_grammar;

    use super::*;
    use boa_ast::operations::{annex_b_function_declarations, annex_b_function_declarations_names};
    use boa_ast::{Declaration, StatementListItem};

    /// The expected `ParseCode` for a modelled rejection.
    ///
    /// Goes through `ParseClassified::from_parse_table`, so a test that names a
    /// link-only code panics here instead of asserting against a `ParseCode`
    /// the product path cannot construct.
    fn early(code: EarlyErrorCode) -> ParseCode {
        ParseCode::Early(ParseClassified::from_parse_table(code))
    }

    const SCRIPT_BODY_SUPER_SOURCES: [&str; 19] = [
        "super();",
        "super.value;",
        r#""use strict"; super.value;"#,
        "() => { super(); };",
        "() => super.value;",
        "async () => { super(); };",
        "async () => super.value;",
        "async (value = super()) => value;",
        "async (value = super.value) => value;",
        "() => () => super.value;",
        "(value = super.value) => value;",
        "class C extends super.value {}",
        "(class extends super.value {});",
        "class C { [super.value]() {} }",
        "class C { [super.value]; }",
        "class C { static [super.value]; }",
        "({ [super.value]() {} });",
        "({ get [super.value]() { return 0; } });",
        "({ [super.value]: 0 });",
    ];

    #[test]
    fn parser_static_semantics_error_reports_early_phase() {
        let err = parse(
            "({ __proto__: null, __proto__: {} });",
            ParseOptions::script(),
        )
        .expect_err("duplicate __proto__ prototype setters should fail");
        assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
        assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
        assert_eq!(
            err.diagnostic().code,
            early(EarlyErrorCode::ObjectDuplicateProto)
        );
        assert!(
            err.diagnostic().span.is_some(),
            "diagnostic should carry Boa's source position when available: {err:?}"
        );
    }

    #[test]
    fn object_literal_cover_initialized_name_rejections_cover_all_parser_contexts() {
        for (source, options) in [
            ("({ a = 1 });", ParseOptions::script()),
            ("function f() { ({ a = 1 }); }", ParseOptions::script()),
            ("function f() { ({ a = 1 }); }", ParseOptions::module()),
            ("export {}; ({ a = 1 });", ParseOptions::module()),
            (
                "class C { static { ({ a = 1 }); } }",
                ParseOptions::script(),
            ),
            (
                "class C { static { ({ a = 1 }); } }",
                ParseOptions::module(),
            ),
        ] {
            let err = parse(source, options)
                .expect_err("a surviving ObjectLiteral CoverInitializedName should fail");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::ObjectLiteralCoverInitializedName),
                "{source:?}: {err:?}"
            );
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }

    #[test]
    fn object_literal_cover_initialized_name_reinterpretations_remain_valid() {
        for source in [
            "let target = {}; ({ a = 1 } = target);",
            "let { a = 1 } = {};",
            "const f = ({ a = 1 }) => a;",
            "let a; ({ a });",
            "({ a: 1 });",
        ] {
            for options in [ParseOptions::script(), ParseOptions::module()] {
                parse(source, options).expect(
                    "assignment/binding reinterpretations and ordinary properties are valid",
                );
            }
        }
    }

    #[test]
    fn script_top_level_new_target_rejects_direct_and_arrow_carried_uses() {
        for source in ["new.target;", "() => { new.target; };"] {
            let err = parse(source, ParseOptions::script())
                .expect_err("ScriptBody Contains NewTarget should fail");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::ScriptTopLevelNewTarget),
                "{source:?}: {err:?}"
            );
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }

    #[test]
    fn script_top_level_new_target_function_and_class_boundaries_remain_valid() {
        for source in [
            "function F() { return new.target; }",
            "function F() { return (() => new.target)(); }",
            "class C { constructor() { new.target; } method() { new.target; } static method() { new.target; } static { new.target; } }",
        ] {
            for options in [ParseOptions::script(), ParseOptions::module()] {
                parse(source, options)
                    .expect("function and class boundaries make new.target parse-valid");
            }
        }
    }

    #[test]
    fn import_meta_outside_module_rejects_at_every_script_nesting_boundary() {
        for source in [
            "import.meta;",
            "\"use strict\";\nimport.meta;",
            "function f() { return import.meta; }",
            "function* f() { return import.meta; }",
            "async function f() { return import.meta; }",
            "const f = () => import.meta;",
            "class C { m() { return import.meta; } }",
            "class C { field = import.meta; }",
            "class C { static field = import.meta; }",
            "class C { static { void import.meta; } }",
            "\n  import.meta;",
        ] {
            let err = parse(source, ParseOptions::script())
                .expect_err("ImportMeta requires the Module syntactic goal");
            assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
            assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::ImportMetaOutsideModule),
                "{source:?}: {err:?}"
            );

            let start = source
                .find("import.meta")
                .expect("the rejection witness contains ImportMeta");
            assert_eq!(
                err.diagnostic().span,
                Some(SourceSpan {
                    start,
                    end: start + 1,
                }),
                "the parser position is the initial import token: {source:?}: {err:?}"
            );
        }
    }

    #[test]
    fn import_meta_module_goal_and_import_call_boundaries_remain_valid() {
        for source in [
            "import.meta;",
            "function f() { return import.meta; }",
            "function* f() { return import.meta; }",
            "async function f() { return import.meta; }",
            "const f = () => import.meta;",
            "class C { m() { return import.meta; } }",
            "class C { field = import.meta; }",
            "class C { static field = import.meta; }",
            "class C { static { void import.meta; } }",
        ] {
            parse(source, ParseOptions::module())
                .expect("lexical nesting does not replace the Module syntactic goal");
        }

        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse("import('./dep.mjs');", options)
                .expect("ImportCall is valid under both static source goals");
        }
    }

    #[test]
    fn import_meta_adjacent_failures_and_goal_precedence_remain_distinct() {
        let mixed = "import.meta; let x; let x;";
        let script = parse(mixed, ParseOptions::script())
            .expect_err("Script rejects ImportMeta before whole-source declaration analysis");
        assert_eq!(
            script.diagnostic().code,
            early(EarlyErrorCode::ImportMetaOutsideModule)
        );

        let module = parse(mixed, ParseOptions::module())
            .expect_err("Module accepts ImportMeta and reaches duplicate declaration analysis");
        assert_eq!(
            module.diagnostic().code,
            early(EarlyErrorCode::DuplicateLexicalDeclaration)
        );

        for source in [r"imp\u006frt.meta;", r"import.m\u0065ta;", "import.foo;"] {
            for options in [ParseOptions::script(), ParseOptions::module()] {
                let err = parse(source, options)
                    .expect_err("a spelling error must reject before the ImportMeta goal check");
                assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
                assert_ne!(
                    err.diagnostic().code,
                    early(EarlyErrorCode::ImportMetaOutsideModule),
                    "{source:?}: {err:?}"
                );
            }
        }

        let err = parse("import.meta = 0;", ParseOptions::module())
            .expect_err("ImportMeta is not a valid assignment target");
        assert_ne!(
            err.diagnostic().code,
            early(EarlyErrorCode::ImportMetaOutsideModule),
            "a Module assignment-target error must keep its distinct owner: {err:?}"
        );
    }

    #[test]
    fn user_export_names_cannot_forge_import_meta_goal_classification() {
        let err = parse(
            concat!(
                "const value = 0;\n",
                "export { value as \"invalid `import.meta` expression outside a module at line\" };\n",
                "export { value as \"invalid `import.meta` expression outside a module at line\" };",
            ),
            ParseOptions::module(),
        )
        .expect_err("the user-chosen exported name is duplicated");
        assert_eq!(
            err.diagnostic().code,
            early(EarlyErrorCode::ModuleDuplicateExport),
            "{err}"
        );
    }

    #[test]
    fn known_import_meta_goal_producer_stays_structurally_reviewed() {
        fn count_in_rust_sources(root: &std::path::Path, fragment: &str) -> usize {
            let entries = std::fs::read_dir(root)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()));
            let mut count = 0;
            for entry in entries {
                let path = entry.expect("failed to read vendored Boa entry").path();
                if path.is_dir() {
                    count += count_in_rust_sources(&path, fragment);
                } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                    let source = std::fs::read_to_string(&path).unwrap_or_else(|error| {
                        panic!("failed to read {}: {error}", path.display())
                    });
                    count += source.matches(fragment).count();
                }
            }
            count
        }

        const MESSAGE: &str = "invalid `import.meta` expression outside a module";
        const POSITION_CAPTURE: &str = "let position = token.span().start();";
        const INITIAL_TOKEN_CAPTURE: &str = r#"
            cursor.set_goal(InputElement::RegExp);

            let token = cursor.peek(0, interner).or_abrupt()?;
            let position = token.span().start();
            let mut lhs = match token.kind() {
        "#;
        const GOAL_BRANCH: &str = r#"if !cursor.module() {
                    return Err(Error::general(
                        "invalid `import.meta` expression outside a module",
                        position,
                    ));
                }"#;
        const MODULE_ENTRY: &str = r#"
            #[derive(Debug, Clone, Copy)]
            struct ModuleParser;

            impl<R> TokenParser<R> for ModuleParser
            where
                R: ReadChar,
            {
                type Output = ModuleParseOutput;

                fn parse(
                    self,
                    cursor: &mut Cursor<R>,
                    interner: &mut Interner
                ) -> ParseResult<Self::Output> {
                    cursor.set_module();

                    let module = boa_ast::Module::new(ModuleItemList.parse(cursor, interner)?);
        "#;
        const MODULE_TRUE_PROJECTION: &str = r#"
            pub(super) fn set_module(&mut self) {
                self.buffered_lexer.set_module(true);
            }

            /// Returns `true` if the cursor is currently parsing a `Module`.
            pub(super) const fn module(&self) -> bool {
                self.buffered_lexer.module()
            }
        "#;
        const BUFFERED_LEXER_MODULE_PROJECTION: &str = r#"
            pub(super) const fn module(&self) -> bool {
                self.lexer.module()
            }

            pub(super) fn set_module(&mut self, module: bool) {
                self.lexer.set_module(module);
            }
        "#;
        const LEXER_MODULE_READ: &str = r#"
            pub(super) const fn module(&self) -> bool {
                self.cursor.module()
            }
        "#;
        const LEXER_MODULE_WRITE: &str = r#"
            pub(super) fn set_module(&mut self, module: bool) {
                self.cursor.set_module(module);
            }
        "#;
        const TERMINAL_MODULE_READ: &str = r#"
            pub(super) const fn module(&self) -> bool {
                self.module
            }
        "#;
        const TERMINAL_MODULE_WRITE: &str = r#"
            pub(super) fn set_module(&mut self, module: bool) {
                self.module = module;
                self.strict = module;
            }
        "#;
        const INITIAL_LEXER_STATE: &str = r#"
            Self {
                iter: inner,
                pos: Position::new(1, 1),
                strict: false,
                module: false,
                peeked: [None; 4],
                source_collector: SourceText::default(),
            }
        "#;
        const ESCAPED_META_MESSAGE: &str = "`import.meta` cannot contain escaped characters";
        const MEMBER_SOURCE: &str = include_str!(
            "../../../vendor/boa_parser-0.21.1/src/parser/expression/left_hand_side/member.rs"
        );
        const PARSER_SOURCE: &str =
            include_str!("../../../vendor/boa_parser-0.21.1/src/parser/mod.rs");
        const PARSER_CURSOR_SOURCE: &str =
            include_str!("../../../vendor/boa_parser-0.21.1/src/parser/cursor/mod.rs");
        const BUFFERED_LEXER_SOURCE: &str = include_str!(
            "../../../vendor/boa_parser-0.21.1/src/parser/cursor/buffered_lexer/mod.rs"
        );
        const LEXER_SOURCE: &str =
            include_str!("../../../vendor/boa_parser-0.21.1/src/lexer/mod.rs");
        const LEXER_CURSOR_SOURCE: &str =
            include_str!("../../../vendor/boa_parser-0.21.1/src/lexer/cursor.rs");

        let without_whitespace = |source: &str| {
            source
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>()
        };

        let boa_package_root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/boa_parser-0.21.1");
        assert_eq!(count_in_rust_sources(&boa_package_root, MESSAGE), 1);
        assert_eq!(MEMBER_SOURCE.matches(MESSAGE).count(), 1);
        assert_eq!(MEMBER_SOURCE.matches(POSITION_CAPTURE).count(), 1);
        assert_eq!(
            without_whitespace(MEMBER_SOURCE)
                .matches(without_whitespace(INITIAL_TOKEN_CAPTURE).as_str())
                .count(),
            1,
            "the diagnostic position must come from the initial member-expression token",
        );
        assert_eq!(MEMBER_SOURCE.matches("cursor.module()").count(), 1);
        assert_eq!(MEMBER_SOURCE.matches(GOAL_BRANCH).count(), 1);
        assert!(
            MEMBER_SOURCE
                .find(ESCAPED_META_MESSAGE)
                .expect("the escaped-meta producer remains present")
                < MEMBER_SOURCE
                    .find(GOAL_BRANCH)
                    .expect("the ImportMeta goal branch remains present"),
            "escaped meta must reject before the syntactic-goal condition"
        );
        assert_eq!(LEXER_CURSOR_SOURCE.matches("module: false,").count(), 1);
        assert_eq!(
            without_whitespace(LEXER_CURSOR_SOURCE)
                .matches(without_whitespace(INITIAL_LEXER_STATE).as_str())
                .count(),
            1,
            "a fresh lexer cursor must start in Script rather than Module mode",
        );
        assert_eq!(PARSER_SOURCE.matches("cursor.set_module();").count(), 1);
        assert_eq!(
            without_whitespace(PARSER_SOURCE)
                .matches(without_whitespace(MODULE_ENTRY).as_str())
                .count(),
            1,
            "ModuleParser must enter Module mode before parsing ModuleItemList",
        );
        assert_eq!(
            without_whitespace(PARSER_CURSOR_SOURCE)
                .matches(without_whitespace(MODULE_TRUE_PROJECTION).as_str())
                .count(),
            1,
            "the parser's Module transition must project the true lexer state",
        );
        assert_eq!(
            without_whitespace(BUFFERED_LEXER_SOURCE)
                .matches(without_whitespace(BUFFERED_LEXER_MODULE_PROJECTION).as_str())
                .count(),
            1,
            "the buffered lexer must forward Module reads and writes unchanged",
        );
        assert_eq!(
            without_whitespace(LEXER_SOURCE)
                .matches(without_whitespace(LEXER_MODULE_READ).as_str())
                .count(),
            1,
            "the lexer must forward Module reads unchanged",
        );
        assert_eq!(
            without_whitespace(LEXER_SOURCE)
                .matches(without_whitespace(LEXER_MODULE_WRITE).as_str())
                .count(),
            1,
            "the lexer must forward Module writes unchanged",
        );
        assert_eq!(
            without_whitespace(LEXER_CURSOR_SOURCE)
                .matches(without_whitespace(TERMINAL_MODULE_READ).as_str())
                .count(),
            1,
            "the terminal cursor must read the Module state bit directly",
        );
        assert_eq!(
            without_whitespace(LEXER_CURSOR_SOURCE)
                .matches(without_whitespace(TERMINAL_MODULE_WRITE).as_str())
                .count(),
            1,
            "the terminal cursor must write the Module state bit and matching strictness directly",
        );
        assert_eq!(LEXER_CURSOR_SOURCE.matches("self.module =").count(), 1);
        assert_eq!(count_in_rust_sources(&boa_package_root, "set_module("), 8);
        assert_eq!(
            count_in_rust_sources(&boa_package_root, ".set_module(false)"),
            0
        );
        assert_eq!(
            count_in_rust_sources(&boa_package_root, ".set_module(true)"),
            1
        );
    }

    #[test]
    fn parser_rejects_unparenthesized_optional_constructor_targets() {
        for source in [
            "new object?.Constructor();",
            "new object?.['Constructor']();",
            "new object.Constructor?.();",
            "new Constructor?.();",
            "new object?.Constructor;",
            "new new Constructor()?.property;",
        ] {
            for options in [ParseOptions::script(), ParseOptions::module()] {
                let err = parse(source, options)
                    .expect_err("optional chain cannot be an unparenthesized constructor target");
                assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
            }
        }
    }

    #[test]
    fn parser_preserves_optional_chains_with_constructor_parenthesis_boundaries() {
        for source in [
            "new (object?.Constructor)();",
            "new (object?.['Constructor'])();",
            "new (object?.Constructor);",
            "(new Constructor)?.property;",
            "new Constructor()?.property;",
            "new Constructor(object?.property);",
            "new (object?.factory())();",
        ] {
            for options in [ParseOptions::script(), ParseOptions::module()] {
                parse(source, options).unwrap_or_else(|err| {
                    panic!(
                        "constructor/optional parenthesis boundary must parse: {source}: {err:?}"
                    )
                });
            }
        }
    }

    #[test]
    fn class_constructor_generator_boundaries_preserve_static_and_computed_methods() {
        for source in [
            r#"class C {
                constructor() {}
                static *constructor() {}
                *["constructor"]() {}
            }"#,
            r#"let C = class {
                constructor() {}
                static async *constructor() {}
                async *["constructor"]() {}
            };"#,
        ] {
            for options in [ParseOptions::script(), ParseOptions::module()] {
                parse(source, options).expect(
                    "static and computed generator methods are not constructor definitions",
                );
            }
        }
    }

    #[test]
    fn remaining_class_constructor_boundaries_preserve_static_and_computed_public_names() {
        for source in [
            r##"class C {
                constructor() {}
                static async constructor() {}
                async ["constructor"]() {}
                static get constructor() { return 1; }
                static set constructor(value) {}
                get ["constructor"]() { return 1; }
                set ["constructor"](value) {}
                ["#constructor"] = 1;
            }"##,
            r##"let C = class {
                constructor() {}
                static async constructor() {}
                static get constructor() { return 1; }
                static set constructor(value) {}
                ["#constructor"];
            };"##,
        ] {
            for options in [ParseOptions::script(), ParseOptions::module()] {
                parse(source, options)
                    .expect("static and computed public names are not forbidden constructor forms");
            }
        }
    }

    #[test]
    fn sloppy_private_delete_stays_owned_by_private_name_validation() {
        for source in [
            "delete object.#missing;",
            "delete ((object.#missing));",
            "object.#missing;",
        ] {
            let err = parse(source, ParseOptions::script())
                .expect_err("an undeclared private name should fail whole-source validation");
            assert_eq!(
                err.diagnostic().phase(),
                ParseDiagnosticPhase::Early,
                "{source:?}: {err:?}"
            );
            assert_eq!(
                err.diagnostic().error_type(),
                Some("SyntaxError"),
                "{source:?}: {err:?}"
            );
            assert_eq!(
                err.diagnostic().code,
                early(EarlyErrorCode::InvalidPrivateIdentifier),
                "{source:?}: {err:?}"
            );
            assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
        }
    }

    #[test]
    fn delete_reference_message_inventory_stays_at_one_reviewed_site_per_condition() {
        const SOURCE: &str =
            include_str!("../../../vendor/boa_parser-0.21.1/src/parser/expression/unary.rs");

        assert_eq!(
            SOURCE
                .matches("cannot delete variables in strict mode")
                .count(),
            1
        );
        assert_eq!(SOURCE.matches("cannot delete private fields").count(), 1);
    }

    #[test]
    fn class_static_block_await_rejections_cover_both_forms_and_goals() {
        for source in [
            "async function outer() { class C { static { await 0; } } }",
            "async function outer() { const C = class { static { await 0; } }; }",
        ] {
            for options in [ParseOptions::script(), ParseOptions::module()] {
                let err = parse(source, options)
                    .expect_err("an AwaitExpression in a class static block should fail");
                assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
                assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
                assert_eq!(
                    err.diagnostic().code,
                    early(EarlyErrorCode::ClassStaticBlockContainsAwait),
                    "{source:?}: {err:?}"
                );
                assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
            }
        }
    }

    #[test]
    fn class_static_block_await_stops_at_nested_function_boundaries() {
        let source = r#"class C {
            static {
                async function nested() { await 0; }
                const arrow = async () => await 0;
            }
        }"#;
        for options in [ParseOptions::script(), ParseOptions::module()] {
            parse(source, options)
                .expect("nested async ordinary and arrow functions own their AwaitExpressions");
        }
    }

    #[test]
    fn class_static_method_prototype_rejections_cover_all_forms_and_goals() {
        for element in [
            "prototype() {}",
            "*prototype() {}",
            "async prototype() {}",
            "async *prototype() {}",
            "get prototype() {}",
            "set prototype(value) {}",
            r#""prototype"() {}"#,
            r"prototyp\u0065() {}",
        ] {
            for source in [
                format!("class C {{ static {element} }}"),
                format!("const C = class {{ static {element} }};"),
            ] {
                for options in [ParseOptions::script(), ParseOptions::module()] {
                    let err = parse(&source, options)
                        .expect_err("a literal public static prototype method should fail");
                    assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Early);
                    assert_eq!(err.diagnostic().error_type(), Some("SyntaxError"));
                    assert_eq!(
                        err.diagnostic().code,
                        early(EarlyErrorCode::ClassStaticMethodPrototypeName),
                        "{source:?}: {err:?}"
                    );
                    assert!(err.diagnostic().span.is_some(), "{source:?}: {err:?}");
                }
            }
        }
    }

    #[test]
    fn class_static_method_prototype_computed_private_and_instance_names_remain_valid() {
        for element in [
            "prototype() {}",
            "*prototype() {}",
            "async prototype() {}",
            "async *prototype() {}",
            "get prototype() {}",
            "set prototype(value) {}",
        ] {
            let source = format!("class C {{ {element} }}");
            for options in [ParseOptions::script(), ParseOptions::module()] {
                parse(&source, options).expect("non-static literal prototype methods are valid");
            }
        }

        for element in [
            r#"["prototype"]() {}"#,
            r#"*["prototype"]() {}"#,
            r#"async ["prototype"]() {}"#,
            r#"async *["prototype"]() {}"#,
            r#"get ["prototype"]() {}"#,
            r#"set ["prototype"](value) {}"#,
            "#prototype() {}",
            "*#prototype() {}",
            "async #prototype() {}",
            "async *#prototype() {}",
            "get #prototype() {}",
            "set #prototype(value) {}",
        ] {
            let source = format!("class C {{ static {element} }}");
            for options in [ParseOptions::script(), ParseOptions::module()] {
                parse(&source, options)
                    .expect("computed and private static prototype names are parse-valid");
            }
        }
    }

    #[test]
    fn ordinary_tags_and_completed_optional_chains_remain_parse_valid() {
        for source in [
            "const tag = () => {}; tag`x`;",
            "const tag = () => {}; tag`x${1}`;",
            "const value = {}; value?.property;",
            "const callable = () => 0; callable?.();",
            "const value = { tag() {} }; (value?.tag)`x`;",
            "const value = { tag() {} }; (value?.tag)`x${1}`;",
        ] {
            for options in [ParseOptions::script(), ParseOptions::module()] {
                parse(source, options).expect(
                    "only a TemplateLiteral directly in the OptionalChain production is forbidden",
                );
            }
        }
    }

    #[test]
    fn user_export_names_cannot_forge_optional_chain_tagged_template_classification() {
        let err = parse(
            concat!(
                "const value = 0;\n",
                "export { value as \"Invalid tagged template on optional chain at line\" };\n",
                "export { value as \"Invalid tagged template on optional chain at line\" };",
            ),
            ParseOptions::module(),
        )
        .expect_err("the user-chosen exported name is duplicated");
        assert_eq!(
            err.diagnostic().code,
            early(EarlyErrorCode::ModuleDuplicateExport),
            "{err}"
        );
    }

    #[test]
    fn known_optional_chain_tagged_template_message_producers_stay_reviewed() {
        const MESSAGE: &str = "Invalid tagged template on optional chain";
        const TEMPLATE_TOKEN_PAIR: &str =
            "TokenKind::TemplateMiddle(_) | TokenKind::TemplateNoSubstitution(_)";
        const OPTIONAL_SOURCE: &str = include_str!(
            "../../../vendor/boa_parser-0.21.1/src/parser/expression/left_hand_side/optional/mod.rs"
        );

        assert_eq!(OPTIONAL_SOURCE.matches(MESSAGE).count(), 2);
        assert_eq!(OPTIONAL_SOURCE.matches(TEMPLATE_TOKEN_PAIR).count(), 2);
    }

    #[test]
    fn distinct_import_attribute_keys_and_trailing_commas_remain_valid() {
        for source in [
            r#"import "./dep.mjs" with { type: "json", mode: "strict", };"#,
            r#"export * from "./dep.mjs" with { type: "json", mode: "strict", };"#,
        ] {
            parse(source, ParseOptions::module())
                .expect("distinct static import-attribute keys should parse");
        }
    }

    /// The two goals agree on the *same* source, which is the property the two
    /// deleted tables could only promise in a doc comment.
    #[test]
    fn both_goals_classify_one_source_identically() {
        for source in [
            "({ __proto__: null, __proto__: {} });",
            "let x; const x = 1;",
            "break;",
        ] {
            let script = parse(source, ParseOptions::script())
                .expect_err("source is rejected in script goal");
            let module = parse(source, ParseOptions::module())
                .expect_err("source is rejected in module goal");
            assert_eq!(
                script.diagnostic().code,
                module.diagnostic().code,
                "goals disagree on {source:?}"
            );
        }
    }

    #[test]
    fn parser_rejects_unbalanced_delimiters() {
        let err = parse("if (true {", ParseOptions::script())
            .expect_err("unbalanced delimiters should fail");
        assert!(err.message().contains("parse error"));
        assert_eq!(err.diagnostic().phase(), ParseDiagnosticPhase::Parse);
    }

    #[test]
    fn parser_accepts_async_arrow_heads_longer_than_thirty_two_tokens() {
        let source = r#"
var ref = async (aFalse = falseCount +=1, aString = stringCount += 1, aNaN = nanCount += 1, a0 = zeroCount += 1, aNull = nullCount += 1, aObj = objCount +=1) => {};
"#;

        parse(source, ParseOptions::script()).expect("long async arrow head should parse");
    }

    /// MC4's call-site half. A code the message-pattern table cannot produce is not a
    /// `ParseClassified`, so it cannot be reported at
    /// `ParseDiagnosticPhase::Early` by any parse-stage producer.
    #[test]
    fn only_parse_table_codes_are_parse_classified() {
        assert!(ParseClassified::from_early(EarlyErrorCode::ObjectDuplicateProto).is_some());
        assert!(
            ParseClassified::from_early(EarlyErrorCode::ObjectLiteralCoverInitializedName)
                .is_some()
        );
        assert!(ParseClassified::from_early(EarlyErrorCode::ScriptTopLevelNewTarget).is_some());
        assert!(ParseClassified::from_early(EarlyErrorCode::ScriptTopLevelSuper).is_some());
        assert!(ParseClassified::from_early(EarlyErrorCode::ImportMetaOutsideModule).is_some());
        assert!(
            ParseClassified::from_early(EarlyErrorCode::ScriptTopLevelUsingDeclaration).is_some()
        );
        assert!(
            ParseClassified::from_early(EarlyErrorCode::ForHeadBodyDeclarationConflict).is_some()
        );
        assert!(
            ParseClassified::from_early(EarlyErrorCode::ForDeclarationDuplicateBoundName).is_some()
        );
        assert!(ParseClassified::from_early(EarlyErrorCode::LexicalBoundNameLet).is_some());
        assert!(ParseClassified::from_early(EarlyErrorCode::ForInUsingDeclaration).is_some());
        assert!(
            ParseClassified::from_early(EarlyErrorCode::SwitchClauseUsingDeclaration).is_some()
        );
        assert!(ParseClassified::from_early(
            EarlyErrorCode::GeneratorDeclarationParametersContainYield,
        )
        .is_some());
        assert!(
            ParseClassified::from_early(EarlyErrorCode::AsyncDeclarationParametersContainAwait,)
                .is_some()
        );
        assert!(
            ParseClassified::from_early(EarlyErrorCode::GeneratorExpressionParametersContainYield,)
                .is_some()
        );
        assert!(
            ParseClassified::from_early(
                EarlyErrorCode::AsyncGeneratorExpressionParametersContainYield,
            )
            .is_some()
        );
        assert!(
            ParseClassified::from_early(
                EarlyErrorCode::AsyncGeneratorExpressionParametersContainAwait,
            )
            .is_some()
        );
        assert!(
            ParseClassified::from_early(EarlyErrorCode::GeneratorMethodParametersContainYield,)
                .is_some()
        );
        assert!(ParseClassified::from_early(
            EarlyErrorCode::AsyncGeneratorMethodParametersContainYield,
        )
        .is_some());
        assert!(ParseClassified::from_early(
            EarlyErrorCode::AsyncGeneratorMethodParametersContainAwait,
        )
        .is_some());
        assert!(ParseClassified::from_early(EarlyErrorCode::ArrowParametersContainYield).is_some());
        assert!(ParseClassified::from_early(EarlyErrorCode::ArrowParametersContainAwait).is_some());
        assert!(
            ParseClassified::from_early(
                EarlyErrorCode::AsyncFunctionExpressionParametersContainAwait,
            )
            .is_some()
        );
        assert!(
            ParseClassified::from_early(EarlyErrorCode::AsyncMethodParametersContainAwait)
                .is_some()
        );
        assert!(
            ParseClassified::from_early(
                EarlyErrorCode::CallableNonSimpleParametersContainUseStrict,
            )
            .is_some()
        );
        assert!(ParseClassified::from_early(EarlyErrorCode::DuplicateFormalParameter).is_some());
        assert!(ParseClassified::from_early(EarlyErrorCode::DuplicateCatchParameter).is_some());
        assert!(
            ParseClassified::from_early(EarlyErrorCode::CatchBodyDeclarationConflict).is_some()
        );
        assert!(
            ParseClassified::from_early(EarlyErrorCode::ClassBaseConstructorHasDirectSuper)
                .is_some()
        );
        assert!(ParseClassified::from_early(EarlyErrorCode::ClassMethodHasDirectSuper).is_some());
        assert!(
            ParseClassified::from_early(EarlyErrorCode::ClassConstructorGeneratorMethod).is_some()
        );
        assert!(ParseClassified::from_early(EarlyErrorCode::ClassConstructorAsyncMethod).is_some());
        assert!(ParseClassified::from_early(EarlyErrorCode::ClassConstructorGetter).is_some());
        assert!(ParseClassified::from_early(EarlyErrorCode::ClassConstructorSetter).is_some());
        assert!(ParseClassified::from_early(EarlyErrorCode::ClassPrivateConstructorName).is_some());
        assert!(
            ParseClassified::from_early(EarlyErrorCode::ClassStaticMethodPrototypeName).is_some()
        );
        assert!(ParseClassified::from_early(EarlyErrorCode::ClassDuplicatePrivateName).is_some());
        assert!(ParseClassified::from_early(EarlyErrorCode::ClassFieldConstructorName).is_some());
        assert!(ParseClassified::from_early(
            EarlyErrorCode::ClassStaticFieldConstructorOrPrototypeName,
        )
        .is_some());
        assert!(
            ParseClassified::from_early(EarlyErrorCode::ClassStaticBlockContainsArguments)
                .is_some()
        );
        assert!(
            ParseClassified::from_early(EarlyErrorCode::ClassStaticBlockContainsSuperCall)
                .is_some()
        );
        assert!(
            ParseClassified::from_early(EarlyErrorCode::ClassStaticBlockContainsAwait).is_some()
        );
        assert!(ParseClassified::from_early(EarlyErrorCode::ClassFieldContainsArguments).is_some());
        assert!(ParseClassified::from_early(EarlyErrorCode::StrictModeWithStatement).is_some());
        assert!(
            ParseClassified::from_early(EarlyErrorCode::StrictModeDeleteIdentifierReference,)
                .is_some()
        );
        assert!(
            ParseClassified::from_early(EarlyErrorCode::StrictModeDeletePrivateReference,)
                .is_some()
        );
        assert!(
            ParseClassified::from_early(EarlyErrorCode::ModuleDuplicateImportAttributeKey,)
                .is_some()
        );
        assert!(ParseClassified::from_early(EarlyErrorCode::ModuleDuplicateExport).is_some());
        assert!(ParseClassified::from_early(EarlyErrorCode::ModuleMissingExport).is_none());
        assert!(ParseClassified::from_early(EarlyErrorCode::ModuleUnresolved).is_none());
        assert!(ParseClassified::from_early(EarlyErrorCode::ModuleTooManyUnits).is_none());
    }
}
