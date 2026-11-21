//! Left hand side expression parsing.
//!
//! More information:
//!  - [MDN documentation][mdn]
//!  - [ECMAScript specification][spec]
//!
//! [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Expressions_and_Operators#Left-hand-side_expressions
//! [spec]: https://tc39.es/ecma262/#sec-left-hand-side-expressions

#[cfg(test)]
mod tests;

mod arguments;
mod call;
mod member;
mod optional;
mod template;

use crate::{
    Error,
    lexer::{InputElement, TokenKind},
    parser::{
        AllowAwait, AllowYield, Cursor, NestingCharge, ParseResult, TokenParser,
        expression::{
            AssignmentExpression,
            left_hand_side::{
                arguments::Arguments,
                call::{CallExpression, CallExpressionTail},
                member::MemberExpression,
                optional::OptionalExpression,
            },
        },
    },
    source::ReadChar,
};
use boa_ast::{
    Expression, Keyword, LinearSpan, Position, Punctuator, Span, Spanned,
    expression::{ImportCall, SuperCall},
};
use boa_interner::{Interner, Sym};

/// Parses a left hand side expression.
///
/// More information:
///  - [MDN documentation][mdn]
///  - [ECMAScript specification][spec]
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Expressions_and_Operators#Left-hand-side_expressions
/// [spec]: https://tc39.es/ecma262/#prod-LeftHandSideExpression
#[derive(Debug, Clone, Copy)]
pub(in crate::parser) struct LeftHandSideExpression {
    allow_yield: AllowYield,
    allow_await: AllowAwait,
}

impl LeftHandSideExpression {
    /// Creates a new `LeftHandSideExpression` parser.
    pub(in crate::parser) fn new<Y, A>(allow_yield: Y, allow_await: A) -> Self
    where
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
    {
        Self {
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
        }
    }
}

impl<R> TokenParser<R> for LeftHandSideExpression
where
    R: ReadChar,
{
    type Output = Expression;

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        /// Checks if we need to parse a keyword call expression `keyword()`.
        ///
        /// It first checks if the next token is `keyword`, and if it is, it checks if the second next
        /// token is the open parenthesis (`(`) punctuator.
        ///
        /// This is needed because the `if let` chain is very complex, and putting it inline in the
        /// initialization of `lhs` would make it very hard to return an expression over all
        /// possible branches of the `if let`s. Instead, we extract the check into its own function,
        /// then use it inside the condition of a simple `if ... else` expression.
        fn is_keyword_call<R: ReadChar>(
            keyword: Keyword,
            cursor: &mut Cursor<R>,
            interner: &mut Interner,
        ) -> ParseResult<Option<Position>> {
            if let Some(next) = cursor.peek(0, interner)?
                && let TokenKind::Keyword((kw, escaped)) = next.kind()
            {
                let keyword_token_start = next.span().start();
                if kw == &keyword {
                    if *escaped {
                        return Err(Error::general(
                            format!(
                                "keyword `{}` cannot contain escaped characters",
                                kw.as_str().0
                            ),
                            keyword_token_start,
                        ));
                    }
                    if let Some(next) = cursor.peek(1, interner)?
                        && next.kind() == &TokenKind::Punctuator(Punctuator::OpenParen)
                    {
                        return Ok(Some(keyword_token_start));
                    }
                }
            }
            Ok(None)
        }

        fn is_import_phase_call<R: ReadChar>(
            cursor: &mut Cursor<R>,
            interner: &mut Interner,
            phase_name: Sym,
            display_name: &str,
        ) -> ParseResult<Option<Position>> {
            let Some(import_token) = cursor.peek(0, interner)? else {
                return Ok(None);
            };
            let TokenKind::Keyword((Keyword::Import, escaped)) = import_token.kind() else {
                return Ok(None);
            };
            let import_start = import_token.span().start();
            if *escaped {
                return Err(Error::general(
                    "keyword `import` cannot contain escaped characters",
                    import_start,
                ));
            }

            let Some(dot) = cursor.peek(1, interner)? else {
                return Ok(None);
            };
            if dot.kind() != &TokenKind::Punctuator(Punctuator::Dot) {
                return Ok(None);
            }

            let Some(phase) = cursor.peek(2, interner)? else {
                return Ok(None);
            };
            let TokenKind::IdentifierName((name, escaped)) = phase.kind() else {
                return Ok(None);
            };
            if *name != phase_name {
                return Ok(None);
            }
            if *escaped != crate::lexer::token::ContainsEscapeSequence(false) {
                return Err(Error::general(
                    format!("`{display_name}` cannot contain escaped characters"),
                    phase.span().start(),
                ));
            }

            let Some(open) = cursor.peek(3, interner)? else {
                return Ok(None);
            };
            if open.kind() != &TokenKind::Punctuator(Punctuator::OpenParen) {
                return Ok(None);
            }

            Ok(Some(import_start))
        }

        fn parse_import_call<R: ReadChar>(
            cursor: &mut Cursor<R>,
            interner: &mut Interner,
            allow_yield: AllowYield,
            allow_await: AllowAwait,
            start: Position,
            phase: boa_ast::declaration::ImportPhase,
            context_name: &'static str,
        ) -> ParseResult<Expression> {
            let head_start = cursor
                .peek(0, interner)?
                .expect("import token")
                .linear_span()
                .start();
            cursor.advance(interner);
            if phase != boa_ast::declaration::ImportPhase::Evaluation {
                cursor.advance(interner);
                cursor.advance(interner);
            }
            let head_span = LinearSpan::new(head_start, cursor.linear_pos());
            cursor.advance(interner);

            let arg = AssignmentExpression::new(true, allow_yield, allow_await)
                .parse(cursor, interner)?;

            let mut options = None;
            if let Some(next) = cursor.peek(0, interner)?
                && next.kind() == &TokenKind::Punctuator(Punctuator::Comma)
            {
                cursor.advance(interner);

                if let Some(next) = cursor.peek(0, interner)?
                    && next.kind() != &TokenKind::Punctuator(Punctuator::CloseParen)
                {
                    options = Some(
                        AssignmentExpression::new(true, allow_yield, allow_await)
                            .parse(cursor, interner)?,
                    );

                    if let Some(next) = cursor.peek(0, interner)?
                        && next.kind() == &TokenKind::Punctuator(Punctuator::Comma)
                    {
                        cursor.advance(interner);
                    }
                }
            }

            let end = cursor
                .expect(
                    TokenKind::Punctuator(Punctuator::CloseParen),
                    context_name,
                    interner,
                )?
                .span()
                .end();

            Ok(
                ImportCall::new_with_phase_and_options(arg, options, phase, Span::new(start, end))
                    .with_head_span(head_span)
                    .into(),
            )
        }

        cursor.set_goal(InputElement::TemplateTail);
        let defer_sym = interner.get_or_intern("defer");
        let source_sym = interner.get_or_intern("source");

        // Scriptw: an optional chain charges each of its items as a link.
        let (mut lhs, mut height) = cursor.measured(interner, |cursor, interner| {
            Ok(
                if let Some(start) = is_keyword_call(Keyword::Super, cursor, interner)? {
                    cursor.advance(interner);
                    let (args, args_span) = Arguments::new(self.allow_yield, self.allow_await)
                        .parse(cursor, interner)?;
                    SuperCall::new(args, Span::new(start, args_span.end())).into()
                } else if let Some(start) =
                    is_import_phase_call(cursor, interner, defer_sym, "import.defer")?
                {
                    {
                        let (call, call_height) =
                            cursor.measured(interner, |cursor, interner| {
                                parse_import_call(
                                    cursor,
                                    interner,
                                    self.allow_yield,
                                    self.allow_await,
                                    start,
                                    boa_ast::declaration::ImportPhase::Defer,
                                    "import.defer call",
                                )
                            })?;
                        // Scriptw: the compiler parses the source again with
                        // each `import(…)` rewritten to a call of a generated
                        // function, so it is charged that call's link (see
                        // `MODULE_BODY_ENCLOSING_LEVELS`).
                        let call_height = cursor.chain_height(
                            call_height,
                            NestingCharge::MemberLink,
                            interner,
                        )?;
                        CallExpressionTail::new(
                            self.allow_yield,
                            self.allow_await,
                            call,
                            call_height,
                        )
                        .parse(cursor, interner)?
                    }
                } else if let Some(start) =
                    is_import_phase_call(cursor, interner, source_sym, "import.source")?
                {
                    {
                        let (call, call_height) =
                            cursor.measured(interner, |cursor, interner| {
                                parse_import_call(
                                    cursor,
                                    interner,
                                    self.allow_yield,
                                    self.allow_await,
                                    start,
                                    boa_ast::declaration::ImportPhase::Source,
                                    "import.source call",
                                )
                            })?;
                        // Scriptw: the compiler parses the source again with
                        // each `import(…)` rewritten to a call of a generated
                        // function, so it is charged that call's link (see
                        // `MODULE_BODY_ENCLOSING_LEVELS`).
                        let call_height = cursor.chain_height(
                            call_height,
                            NestingCharge::MemberLink,
                            interner,
                        )?;
                        CallExpressionTail::new(
                            self.allow_yield,
                            self.allow_await,
                            call,
                            call_height,
                        )
                        .parse(cursor, interner)?
                    }
                } else if let Some(start) = is_keyword_call(Keyword::Import, cursor, interner)? {
                    {
                        let (call, call_height) =
                            cursor.measured(interner, |cursor, interner| {
                                parse_import_call(
                                    cursor,
                                    interner,
                                    self.allow_yield,
                                    self.allow_await,
                                    start,
                                    boa_ast::declaration::ImportPhase::Evaluation,
                                    "import call",
                                )
                            })?;
                        // Scriptw: the compiler parses the source again with
                        // each `import(…)` rewritten to a call of a generated
                        // function, so it is charged that call's link (see
                        // `MODULE_BODY_ENCLOSING_LEVELS`).
                        let call_height = cursor.chain_height(
                            call_height,
                            NestingCharge::MemberLink,
                            interner,
                        )?;
                        CallExpressionTail::new(
                            self.allow_yield,
                            self.allow_await,
                            call,
                            call_height,
                        )
                        .parse(cursor, interner)?
                    }
                } else {
                    let (mut member, member_height) =
                        cursor.measured(interner, |cursor, interner| {
                            MemberExpression::new(self.allow_yield, self.allow_await)
                                .parse(cursor, interner)
                        })?;
                    if let Some(tok) = cursor.peek(0, interner)?
                        && tok.kind() == &TokenKind::Punctuator(Punctuator::OpenParen)
                    {
                        member = CallExpression::new(
                            self.allow_yield,
                            self.allow_await,
                            member,
                            member_height,
                        )
                        .parse(cursor, interner)?;
                    }
                    member
                },
            )
        })?;

        if let Some(tok) = cursor.peek(0, interner)?
            && tok.kind() == &TokenKind::Punctuator(Punctuator::Optional)
        {
            let (optional, optional_height) =
                OptionalExpression::new(self.allow_yield, self.allow_await, lhs, height)
                    .parse(cursor, interner)?;
            height = optional_height;
            cursor.record_height(height);
            lhs = optional.into();
        }

        Ok(lhs)
    }
}
