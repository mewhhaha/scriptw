//! Cursor implementation for the parser.
mod buffered_lexer;

use crate::{
    Error,
    lexer::{InputElement, Lexer, Token, TokenKind},
    parser::{NestingCharge, OrAbrupt, ParseResult},
    source::ReadChar,
};
use boa_ast::{LinearPosition, PositionGroup, Punctuator, Spanned};
use boa_interner::Interner;
use buffered_lexer::BufferedLexer;

/// The result of a peek for a semicolon.
#[derive(Debug)]
pub(super) enum SemicolonResult<'s> {
    Found(Option<&'s Token>),
    NotFound(&'s Token),
}

/// Token cursor.
///
/// This internal structure gives basic testable operations to the parser.
#[derive(Debug)]
pub(super) struct Cursor<R> {
    buffered_lexer: BufferedLexer<R>,

    /// Tracks if the cursor is in a arrow function declaration.
    arrow: bool,

    /// Indicate if the cursor is used in `JSON.parse`.
    json_parse: bool,

    /// A unique identifier for each parser instance.
    /// This is used to generate unique identifiers tagged template literals.
    identifier: u32,

    /// Tracks the number of tagged templates that are currently being parsed.
    tagged_templates_count: u32,

    /// Scriptw: the tree height, in units, of the recursive grammar
    /// productions that enclose the current token (see
    /// [`crate::parser::MAX_NESTING_UNITS`]).
    nesting: u32,

    /// Scriptw: the greatest tree height, in units, of any node parsed since
    /// the innermost [`Self::measured`] began.
    reach: u32,

    /// Scriptw: whether the current token is in a generator or async function
    /// body, or at an ES module's top level (see
    /// [`crate::parser::BINARY_LINK_UNITS`]).
    resumable_body: bool,

    /// Scriptw: how many `await` and `yield` expressions of the current
    /// function body have been parsed so far; a chain compares it with its
    /// value when the chain began (see [`Self::binary_link`]).
    suspensions: u64,
}

/// Scriptw: the tree height, in units, of a binary, logical, `??` or comma
/// chain so far (see [`Cursor::binary_link`]): as charged, and as it would be
/// if every operator were charged [`NestingCharge::ResumableOperator`].
#[derive(Debug, Clone, Copy)]
pub(super) struct ChainHeight {
    plain: u32,
    resumable: u32,
}

impl ChainHeight {
    /// The height of one operand, `height` units high.
    pub(super) const fn operand(height: u32) -> Self {
        Self {
            plain: height,
            resumable: height,
        }
    }

    /// The height as charged.
    pub(super) const fn height(self) -> u32 {
        self.plain
    }
}

impl<R> Cursor<R>
where
    R: ReadChar,
{
    /// Creates a new cursor with the given reader.
    pub(super) fn new(reader: R) -> Self {
        Self {
            buffered_lexer: Lexer::new(reader).into(),
            arrow: false,
            json_parse: false,
            identifier: 0,
            tagged_templates_count: 0,
            nesting: 0,
            reach: 0,
            resumable_body: false,
            suspensions: 0,
        }
    }

    /// Scriptw: parses one recursive production one nesting level deeper.
    ///
    /// Every grammar cycle of the recursive-descent parser passes through a
    /// production that calls this, so the parser's own recursion is bounded.
    /// Trees that loops build deeper than the parser recursed are bounded by
    /// [`Self::measured`] and [`Self::chain_height`].
    pub(super) fn nested<T>(
        &mut self,
        interner: &mut Interner,
        parse: impl FnOnce(&mut Self, &mut Interner) -> ParseResult<T>,
    ) -> ParseResult<T> {
        self.nested_charge(NestingCharge::Level, interner, parse)
    }

    /// Scriptw: [`Self::nested`] for a production that costs `charge`.
    pub(super) fn nested_charge<T>(
        &mut self,
        charge: NestingCharge,
        interner: &mut Interner,
        parse: impl FnOnce(&mut Self, &mut Interner) -> ParseResult<T>,
    ) -> ParseResult<T> {
        let units = charge.units();
        if self.nesting + units > crate::parser::MAX_NESTING_UNITS {
            return Err(self.nesting_limit_error(charge, interner));
        }
        self.nesting += units;
        self.reach = self.reach.max(self.nesting);
        let parsed = parse(self, interner);
        self.nesting -= units;
        parsed
    }

    /// Scriptw: parses one operand of a loop-built node and returns it with
    /// the height, in units above the current nesting, of the tree parsed.
    ///
    /// A loop that wraps earlier operands in a new node (a left-deep chain) or
    /// nests later ones (an `else if` chain) makes the tree deeper than the
    /// nesting its operands were parsed at; it adds its own links to these
    /// heights with [`Self::chain_height`] and records the total with
    /// [`Self::record_height`].
    pub(super) fn measured<T>(
        &mut self,
        interner: &mut Interner,
        parse: impl FnOnce(&mut Self, &mut Interner) -> ParseResult<T>,
    ) -> ParseResult<(T, u32)> {
        let enclosing = core::mem::replace(&mut self.reach, self.nesting);
        let parsed = parse(self, interner);
        let height = self.reach - self.nesting;
        self.reach = enclosing.max(self.reach);
        parsed.map(|parsed| (parsed, height))
    }

    /// Scriptw: adds one link of a loop-built tree, costing `charge`, over
    /// operands `operands` units high, checks that the tree stays within
    /// [`crate::parser::MAX_NESTING_UNITS`] above the current nesting, and
    /// returns its height.
    pub(super) fn chain_height(
        &mut self,
        operands: u32,
        charge: NestingCharge,
        interner: &mut Interner,
    ) -> ParseResult<u32> {
        let height = operands.saturating_add(charge.units());
        if self.nesting.saturating_add(height) > crate::parser::MAX_NESTING_UNITS {
            return Err(self.nesting_limit_error(charge, interner));
        }
        Ok(height)
    }

    /// Scriptw: records a loop-built tree of a height that [`Self::chain_height`]
    /// accepted, for the enclosing [`Self::measured`].
    pub(super) fn record_height(&mut self, height: u32) {
        self.reach = self.reach.max(self.nesting + height);
    }

    /// Scriptw: the mark a binary, logical, `??` or comma chain takes before
    /// its first operand (see [`Self::binary_link`]).
    pub(super) const fn suspension_mark(&self) -> u64 {
        self.suspensions
    }

    /// Scriptw: records an `await` or `yield` expression.
    pub(super) fn note_suspension(&mut self) {
        self.suspensions += 1;
    }

    /// Scriptw: adds one binary, logical, `??` or comma operator over `lhs`
    /// and `rhs` to the chain that took `mark` before its first operand.
    ///
    /// The compiler walks these chains without recursing per operator, except
    /// in a generator or async function body when one of the chain's operands
    /// contains an `await` or `yield`: there the resumable lowering and state
    /// planners recurse once per operator from that operand on, and test each
    /// such operator's operands for a suspension. So once the chain has
    /// parsed a suspension, every one of its operators is charged
    /// [`NestingCharge::ResumableOperator`], the ones before it included.
    pub(super) fn binary_link(
        &mut self,
        lhs: ChainHeight,
        rhs: ChainHeight,
        mark: u64,
        interner: &mut Interner,
    ) -> ParseResult<ChainHeight> {
        let resumable_operands = lhs.resumable.max(rhs.resumable);
        let resumable = resumable_operands.saturating_add(NestingCharge::ResumableOperator.units());
        let plain = if self.resumable_body && self.suspensions != mark {
            self.chain_height(
                resumable_operands,
                NestingCharge::ResumableOperator,
                interner,
            )?
        } else {
            self.chain_height(lhs.plain.max(rhs.plain), NestingCharge::Operator, interner)?
        };
        Ok(ChainHeight { plain, resumable })
    }

    /// Scriptw: parses a function body, `resumable` when it is a generator or
    /// async function's. Its suspensions belong to it, not to a chain of the
    /// enclosing body that contains the function.
    pub(super) fn function_body<T>(
        &mut self,
        resumable: bool,
        interner: &mut Interner,
        parse: impl FnOnce(&mut Self, &mut Interner) -> ParseResult<T>,
    ) -> ParseResult<T> {
        let enclosing = core::mem::replace(&mut self.resumable_body, resumable);
        let suspensions = self.suspensions;
        let parsed = parse(self, interner);
        self.resumable_body = enclosing;
        self.suspensions = suspensions;
        parsed
    }

    /// Scriptw: parses an ES module's body (see
    /// [`crate::parser::MODULE_BODY_ENCLOSING_LEVELS`]): as deep as the
    /// compiler will parse it again, inside its module scaffolding, and as a
    /// resumable body, since that scaffolding is an async function and a
    /// module's top level may `await`.
    pub(super) fn module_body<T>(
        &mut self,
        interner: &mut Interner,
        parse: impl FnOnce(&mut Self, &mut Interner) -> ParseResult<T>,
    ) -> ParseResult<T> {
        let units =
            crate::parser::MODULE_BODY_ENCLOSING_LEVELS * crate::parser::NESTING_UNITS_PER_LEVEL;
        self.nesting += units;
        self.reach = self.reach.max(self.nesting);
        let parsed = self.function_body(true, interner, parse);
        self.nesting -= units;
        parsed
    }

    fn nesting_limit_error(&mut self, charge: NestingCharge, interner: &mut Interner) -> Error {
        let position = match self.peek(0, interner) {
            Ok(Some(token)) => token.span().start(),
            _ => boa_ast::Position::new(1, 1),
        };
        Error::general(
            format!(
                "source nesting exceeds the implementation limit of {} levels{}",
                crate::parser::MAX_NESTING_DEPTH,
                charge.description()
            ),
            position,
        )
    }

    /// Sets the goal symbol of the cursor to `Module`.
    pub(super) fn set_module(&mut self) {
        self.buffered_lexer.set_module(true);
    }

    /// Returns `true` if the cursor is currently parsing a `Module`.
    pub(super) const fn module(&self) -> bool {
        self.buffered_lexer.module()
    }

    pub(super) fn set_goal(&mut self, elm: InputElement) {
        self.buffered_lexer.set_goal(elm);
    }

    /// Lexes the next tokens as a regex assuming that the starting '/' has already been consumed.
    /// If `init_with_eq` is `true`, then assuming that the starting '/=' has already been consumed.
    pub(super) fn lex_regex(
        &mut self,
        start: PositionGroup,
        interner: &mut Interner,
        init_with_eq: bool,
    ) -> ParseResult<Token> {
        self.buffered_lexer.lex_regex(start, interner, init_with_eq)
    }

    pub(super) fn lex_template(
        &mut self,
        start: PositionGroup,
        interner: &mut Interner,
    ) -> ParseResult<Token> {
        self.buffered_lexer.lex_template(start, interner)
    }

    /// Advances the cursor and returns the next token.
    pub(super) fn next(&mut self, interner: &mut Interner) -> ParseResult<Option<Token>> {
        self.buffered_lexer.next(true, interner)
    }

    /// Advances the cursor without returning the next token.
    ///
    /// # Panics
    ///
    /// This function will panic if there is no further token in the cursor.
    #[track_caller]
    pub(super) fn advance(&mut self, interner: &mut Interner) {
        self.next(interner)
            .expect("tried to advance cursor, but the buffer was empty");
    }

    /// Peeks a future token, without consuming it or advancing the cursor.
    /// This peeking **skips** line terminators.
    ///
    /// You can skip some tokens with the `skip_n` option.
    pub(super) fn peek(
        &mut self,
        skip_n: usize,
        interner: &mut Interner,
    ) -> ParseResult<Option<&Token>> {
        self.buffered_lexer.peek(skip_n, true, interner)
    }

    /// Scriptw: the [`Self::peek`] index of the `)` that closes the `(` at
    /// index `open`, scanning the tokens between them once; `None` when the
    /// source ends first.
    pub(super) fn closing_paren(
        &mut self,
        open: usize,
        interner: &mut Interner,
    ) -> ParseResult<Option<usize>> {
        self.buffered_lexer.closing_paren(open, interner)
    }

    /// Peeks a future token, without consuming it or advancing the cursor.
    /// This peeking **does not skips** line terminators.
    ///
    /// You can skip some tokens with the `skip_n` option.
    pub(super) fn peek_no_skip_line_term(
        &mut self,
        skip_n: usize,
        interner: &mut Interner,
    ) -> ParseResult<Option<&Token>> {
        self.buffered_lexer.peek(skip_n, false, interner)
    }

    /// Gets the current strict mode for the cursor.
    pub(super) const fn strict(&self) -> bool {
        self.buffered_lexer.strict()
    }

    /// Sets the strict mode to strict or non-strict.
    pub(super) fn set_strict(&mut self, strict: bool) {
        self.buffered_lexer.set_strict(strict);
    }

    /// Returns if the cursor is currently in an arrow function declaration.
    pub(super) const fn arrow(&self) -> bool {
        self.arrow
    }

    /// Set if the cursor is currently in a arrow function declaration.
    pub(super) fn set_arrow(&mut self, arrow: bool) {
        self.arrow = arrow;
    }

    /// Returns if the cursor is currently used in `JSON.parse`.
    pub(super) const fn json_parse(&self) -> bool {
        self.json_parse
    }

    /// Set if the cursor is currently used in `JSON.parse`.
    pub(super) fn set_json_parse(&mut self, json_parse: bool) {
        self.json_parse = json_parse;
    }

    /// Set the identifier of the cursor.
    #[inline]
    pub(super) fn set_identifier(&mut self, identifier: u32) {
        self.identifier = identifier;
    }

    /// Get the identifier for a tagged template.
    #[inline]
    pub(super) fn tagged_template_identifier(&mut self) -> u64 {
        self.tagged_templates_count += 1;

        let identifier = u64::from(self.identifier);
        let count = u64::from(self.tagged_templates_count);

        (count << 32) | identifier
    }

    /// Returns an error if the next token is not of kind `kind`.
    pub(super) fn expect<K>(
        &mut self,
        kind: K,
        context: &'static str,
        interner: &mut Interner,
    ) -> ParseResult<Token>
    where
        K: Into<TokenKind>,
    {
        let next_token = self.next(interner).or_abrupt()?;
        let kind = kind.into();

        if next_token.kind() == &kind {
            Ok(next_token)
        } else {
            Err(Error::expected(
                [kind.to_string(interner)],
                next_token.to_string(interner),
                next_token.span(),
                context,
            ))
        }
    }

    /// It will peek for the next token, to see if it's a semicolon.
    ///
    /// It will automatically insert a semicolon if needed, as specified in the [spec][spec].
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-automatic-semicolon-insertion
    pub(super) fn peek_semicolon(
        &mut self,
        interner: &mut Interner,
    ) -> ParseResult<SemicolonResult<'_>> {
        self.peek_no_skip_line_term(0, interner)?
            .map_or(Ok(SemicolonResult::Found(None)), |tk| match tk.kind() {
                TokenKind::Punctuator(Punctuator::Semicolon | Punctuator::CloseBlock)
                | TokenKind::LineTerminator => Ok(SemicolonResult::Found(Some(tk))),
                _ => Ok(SemicolonResult::NotFound(tk)),
            })
    }

    /// Consumes the next token if it is a semicolon, or returns a `Errpr` if it's not.
    ///
    /// It will automatically insert a semicolon if needed, as specified in the [spec][spec].
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-automatic-semicolon-insertion
    pub(super) fn expect_semicolon(
        &mut self,
        context: &'static str,
        interner: &mut Interner,
    ) -> ParseResult<()> {
        match self.peek_semicolon(interner)? {
            SemicolonResult::Found(Some(tk)) => match *tk.kind() {
                TokenKind::Punctuator(Punctuator::Semicolon) | TokenKind::LineTerminator => {
                    let _next = self.buffered_lexer.next(false, interner)?;
                    Ok(())
                }
                _ => Ok(()),
            },
            SemicolonResult::Found(None) => Ok(()),
            SemicolonResult::NotFound(tk) => Err(Error::expected(
                [";".to_owned()],
                tk.to_string(interner),
                tk.span(),
                context,
            )),
        }
    }

    /// It will make sure that the peeked token (skipping n tokens) is not a line terminator.
    ///
    /// It expects that the token stream does not end here.
    ///
    /// This is just syntatic sugar for a `.peek(skip_n)` call followed by a check that the result
    /// is not a line terminator or `None`.
    pub(super) fn peek_expect_no_lineterminator(
        &mut self,
        skip_n: usize,
        context: &'static str,
        interner: &mut Interner,
    ) -> ParseResult<&Token> {
        let tok = self.peek_no_skip_line_term(skip_n, interner).or_abrupt()?;

        if tok.kind() == &TokenKind::LineTerminator {
            Err(Error::unexpected(
                tok.to_string(interner),
                tok.span(),
                context,
            ))
        } else {
            Ok(tok)
        }
    }

    /// Check if the peeked token is a line terminator.
    pub(super) fn peek_is_line_terminator(
        &mut self,
        skip_n: usize,
        interner: &mut Interner,
    ) -> ParseResult<Option<bool>> {
        self.peek_no_skip_line_term(skip_n, interner)?
            .map_or(Ok(None), |t| {
                Ok(Some(t.kind() == &TokenKind::LineTerminator))
            })
    }

    /// Advance the cursor to the next token and retrieve it, only if it's of `kind` type.
    ///
    /// When the next token is a `kind` token, get the token, otherwise return `None`.
    ///
    /// No next token also returns None.
    pub(super) fn next_if<K>(
        &mut self,
        kind: K,
        interner: &mut Interner,
    ) -> ParseResult<Option<Token>>
    where
        K: Into<TokenKind>,
    {
        if let Some(token) = self.peek(0, interner)?
            && token.kind() == &kind.into()
        {
            self.next(interner)
        } else {
            Ok(None)
        }
    }

    /// Gets current linear position in the source code.
    #[inline]
    pub(super) fn linear_pos(&self) -> LinearPosition {
        self.buffered_lexer.linear_pos()
    }

    pub(super) fn take_source(&mut self) -> boa_ast::SourceText {
        self.buffered_lexer.take_source()
    }
}
