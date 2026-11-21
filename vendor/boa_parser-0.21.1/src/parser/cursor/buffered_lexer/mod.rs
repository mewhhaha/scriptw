use crate::{
    Error,
    lexer::{InputElement, Lexer, Token, TokenKind},
    parser::ParseResult,
    source::{ReadChar, UTF8Input},
};
use boa_ast::{LinearPosition, PositionGroup, Punctuator};
use boa_interner::Interner;
use std::collections::VecDeque;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(super) struct BufferedLexer<R> {
    lexer: Lexer<R>,
    peeked: VecDeque<Option<Token>>,
    last_linear_pos: LinearPosition,
}

impl<R> From<Lexer<R>> for BufferedLexer<R>
where
    R: ReadChar,
{
    fn from(lexer: Lexer<R>) -> Self {
        Self {
            lexer,
            peeked: VecDeque::new(),
            last_linear_pos: LinearPosition::default(),
        }
    }
}

impl<R> From<R> for BufferedLexer<R>
where
    R: ReadChar,
{
    fn from(reader: R) -> Self {
        Lexer::new(reader).into()
    }
}

impl<'a> From<&'a [u8]> for BufferedLexer<UTF8Input<&'a [u8]>> {
    fn from(reader: &'a [u8]) -> Self {
        Lexer::from(reader).into()
    }
}

impl<R> BufferedLexer<R>
where
    R: ReadChar,
{
    /// Sets the goal symbol for the lexer.
    pub(super) fn set_goal(&mut self, elm: InputElement) {
        self.lexer.set_goal(elm);
    }

    /// Lexes the next tokens as a regex assuming that the starting '/' has already been consumed.
    /// If `init_with_eq` is `true`, then assuming that the starting '/=' has already been consumed.
    pub(super) fn lex_regex(
        &mut self,
        start: PositionGroup,
        interner: &mut Interner,
        init_with_eq: bool,
    ) -> ParseResult<Token> {
        self.set_goal(InputElement::RegExp);
        self.lexer
            .lex_slash_token(start, interner, init_with_eq)
            .map_err(Into::into)
    }

    /// Lexes the next tokens as template middle or template tail assuming that the starting
    /// '}' has already been consumed.
    pub(super) fn lex_template(
        &mut self,
        start: PositionGroup,
        interner: &mut Interner,
    ) -> ParseResult<Token> {
        self.lexer
            .lex_template(start, interner)
            .map_err(Error::from)
    }

    pub(super) const fn strict(&self) -> bool {
        self.lexer.strict()
    }

    pub(super) fn set_strict(&mut self, strict: bool) {
        self.lexer.set_strict(strict);
    }

    pub(super) const fn module(&self) -> bool {
        self.lexer.module()
    }

    pub(super) fn set_module(&mut self, module: bool) {
        self.lexer.set_module(module);
    }

    /// Fills the peeking buffer with the next token.
    ///
    /// It will not fill two line terminators one after the other.
    fn fill(&mut self, interner: &mut Interner) -> ParseResult<()> {
        let next = if self
            .peeked
            .back()
            .and_then(Option::as_ref)
            .is_some_and(|token| token.kind() == &TokenKind::LineTerminator)
        {
            // We don't want to have multiple contiguous line terminators in the buffer, since
            // they have no meaning.
            loop {
                self.lexer.skip_html_close(interner)?;
                let next = self.lexer.next_no_skip(interner)?;
                if let Some(ref token) = next {
                    match token.kind() {
                        TokenKind::LineTerminator => { /* skip */ }
                        TokenKind::Comment => self.lexer.skip_html_close(interner)?,
                        _ => break next,
                    }
                } else {
                    break None;
                }
            }
        } else {
            self.lexer.next(interner)?
        };
        self.peeked.push_back(next);

        Ok(())
    }

    /// Moves the cursor to the next token and returns the token.
    ///
    /// If `skip_line_terminators` is true then line terminators will be discarded.
    ///
    /// This follows iterator semantics in that a `peek(0, false)` followed by a `next(false)` will
    /// return the same value. Note that because a `peek(n, false)` may return a line terminator a
    /// subsequent `next(true)` may not return the same value.
    pub(super) fn next(
        &mut self,
        skip_line_terminators: bool,
        interner: &mut Interner,
    ) -> ParseResult<Option<Token>> {
        if self.peeked.is_empty() {
            self.fill(interner)?;
        }

        if let Some(Some(token)) = self.peeked.front() {
            if skip_line_terminators && token.kind() == &TokenKind::LineTerminator {
                // Keep the terminator in the buffer while filling so `fill`
                // coalesces further terminators, including those after comments.
                if self.peeked.len() == 1 {
                    self.fill(interner)?;
                }
                self.peeked.pop_front();
            }
            let tok = self.peeked.pop_front().flatten();

            if let Some(tok) = &tok {
                self.last_linear_pos = tok.linear_span().end();
            }

            Ok(tok)
        } else {
            // We do not update the read index, since we should always return `None` from now on.
            Ok(None)
        }
    }

    /// Peeks the `n`th token after the next token.
    ///
    /// If there are tokens `A`, `B`, `C`, `D`, `E` and `peek(0, false)` returns `A` then:
    ///  - `peek(1, false) == peek(1, true) == B`.
    ///  - `peek(2, false)` will return `C`.
    ///    where `A`, `B`, `C`, `D` and `E` are tokens but not line terminators.
    ///
    /// If `skip_line_terminators` is `true` then line terminators will be discarded.
    /// i.e. If there are tokens `A`, `\n`, `B` and `peek(0, false)` is `A` then the following
    /// will hold:
    ///  - `peek(0, true) == A`
    ///  - `peek(0, false) == A`
    ///  - `peek(1, true) == B`
    ///  - `peek(1, false) == \n`
    ///  - `peek(2, true) == None` (End of stream)
    ///  - `peek(2, false) == B`
    pub(super) fn peek(
        &mut self,
        skip_n: usize,
        skip_line_terminators: bool,
        interner: &mut Interner,
    ) -> ParseResult<Option<&Token>> {
        let mut read_index = 0;
        let mut count = 0;
        let res_token = loop {
            if read_index == self.peeked.len() {
                self.fill(interner)?;
            }

            if let Some(ref token) = self.peeked[read_index] {
                if skip_line_terminators && token.kind() == &TokenKind::LineTerminator {
                    read_index += 1;
                    // We only store 1 contiguous line terminator, so if the current token
                    // was a line terminator, we know that the next won't be one.
                    if read_index == self.peeked.len() {
                        self.fill(interner)?;
                    }
                }
                if count == skip_n {
                    break self.peeked[read_index].as_ref();
                }
            } else {
                break None;
            }
            read_index += 1;
            count += 1;
        };

        Ok(res_token)
    }

    /// Scriptw: the index, as [`Self::peek`] with `skip_line_terminators`
    /// counts it, of the `)` that closes the `(` at index `open`, or `None`
    /// when the source ends first. The tokens are scanned once: finding it by
    /// calling `peek` with each index walked the buffer from its start every
    /// time, quadratic in the tokens between the parentheses.
    pub(super) fn closing_paren(
        &mut self,
        open: usize,
        interner: &mut Interner,
    ) -> ParseResult<Option<usize>> {
        let mut read_index = 0;
        let mut count = 0;
        let mut depth = 0usize;
        loop {
            if read_index == self.peeked.len() {
                self.fill(interner)?;
            }
            let Some(token) = &self.peeked[read_index] else {
                return Ok(None);
            };
            if token.kind() != &TokenKind::LineTerminator {
                if count >= open {
                    match token.kind() {
                        TokenKind::Punctuator(Punctuator::OpenParen) => depth += 1,
                        TokenKind::Punctuator(Punctuator::CloseParen) => {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                return Ok(Some(count));
                            }
                        }
                        _ => {}
                    }
                }
                count += 1;
            }
            read_index += 1;
        }
    }

    /// Gets current linear position in the source code.
    #[inline]
    pub(super) fn linear_pos(&self) -> LinearPosition {
        self.last_linear_pos
    }

    pub(super) fn take_source(&mut self) -> boa_ast::SourceText {
        self.lexer.take_source()
    }
}
