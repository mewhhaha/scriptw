#[cfg(test)]
mod tests;

use crate::{
    Error,
    lexer::{Token, TokenKind},
    parser::{
        AllowAwait, AllowYield, NestingCharge, OrAbrupt, ParseResult, TokenParser,
        cursor::Cursor,
        expression::Expression,
        expression::left_hand_side::{arguments::Arguments, call::access_charge},
    },
    source::ReadChar,
};
use ast::function::PrivateName;
use boa_ast::{
    self as ast, Punctuator, Span, Spanned,
    expression::{
        Identifier, Optional, OptionalOperation, OptionalOperationKind, access::PropertyAccessField,
    },
};
use boa_interner::{Interner, Sym};

/// Parses an optional expression.
///
/// More information:
///  - [MDN documentation][mdn]
///  - [ECMAScript specification][spec]
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Optional_chaining
/// [spec]: https://tc39.es/ecma262/multipage/ecmascript-language-expressions.html#prod-OptionalExpression
#[derive(Debug, Clone)]
pub(in crate::parser) struct OptionalExpression {
    allow_yield: AllowYield,
    allow_await: AllowAwait,
    target: ast::Expression,
    /// Scriptw: the tree height of `target` (see `Cursor::measured`).
    target_height: u32,
}

impl OptionalExpression {
    /// Creates a new `OptionalExpression` parser.
    pub(in crate::parser) fn new<Y, A>(
        allow_yield: Y,
        allow_await: A,
        target: ast::Expression,
        target_height: u32,
    ) -> Self
    where
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
    {
        Self {
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
            target,
            target_height,
        }
    }
}

impl<R> TokenParser<R> for OptionalExpression
where
    R: ReadChar,
{
    /// Scriptw: the chain and its tree height. The chain is one flat node, but
    /// each item is charged as the link it would be outside an optional chain,
    /// because its native compilation costs the same.
    type Output = (Optional, u32);

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        fn parse_const_access(
            token: &Token,
            interner: &Interner,
        ) -> ParseResult<(OptionalOperationKind, Span)> {
            let item = match token.kind() {
                TokenKind::IdentifierName((name, _)) => {
                    OptionalOperationKind::SimplePropertyAccess {
                        field: Identifier::new(*name, token.span()).into(),
                    }
                }
                TokenKind::Keyword((kw, _)) => OptionalOperationKind::SimplePropertyAccess {
                    field: Identifier::new(kw.to_sym(), token.span()).into(),
                },
                TokenKind::BooleanLiteral((true, _)) => {
                    OptionalOperationKind::SimplePropertyAccess {
                        field: Identifier::new(Sym::TRUE, token.span()).into(),
                    }
                }
                TokenKind::BooleanLiteral((false, _)) => {
                    OptionalOperationKind::SimplePropertyAccess {
                        field: Identifier::new(Sym::FALSE, token.span()).into(),
                    }
                }
                TokenKind::NullLiteral(_) => OptionalOperationKind::SimplePropertyAccess {
                    field: Identifier::new(Sym::NULL, token.span()).into(),
                },
                TokenKind::PrivateIdentifier(name) => {
                    OptionalOperationKind::PrivatePropertyAccess {
                        field: PrivateName::new(*name, token.span()),
                    }
                }
                _ => {
                    return Err(Error::expected(
                        ["identifier".to_owned()],
                        token.to_string(interner),
                        token.span(),
                        "optional chain",
                    ));
                }
            };
            Ok((item, token.span()))
        }

        let mut items = Vec::new();
        let mut height = self.target_height;
        // Whether the item before the next one is a property access, which a
        // call item makes a method call.
        let mut after_access = matches!(self.target, ast::Expression::PropertyAccess(_));

        while let Some(token) = cursor.peek(0, interner)? {
            let token_span = token.span();
            let shorted = match token.kind() {
                TokenKind::Punctuator(Punctuator::Optional) => {
                    cursor.advance(interner);
                    true
                }
                TokenKind::Punctuator(Punctuator::OpenParen | Punctuator::OpenBracket) => false,
                TokenKind::Punctuator(Punctuator::Dot) => {
                    cursor.advance(interner);
                    let field = cursor.next(interner).or_abrupt()?;

                    let (item, item_span) = parse_const_access(&field, interner)?;
                    let charge = access_charge(cursor, interner)?;
                    height = cursor.chain_height(height, charge, interner)?;
                    after_access = true;
                    items.push(OptionalOperation::new(
                        item,
                        false,
                        Span::new(token_span.start(), item_span.end()),
                    ));
                    continue;
                }
                TokenKind::TemplateMiddle(_) | TokenKind::TemplateNoSubstitution(_) => {
                    return Err(Error::general(
                        "Invalid tagged template on optional chain",
                        token.span().start(),
                    ));
                }
                _ => break,
            };

            let token = cursor.peek(0, interner).or_abrupt()?;
            let (item, item_span) = match token.kind() {
                TokenKind::Punctuator(Punctuator::OpenParen) => {
                    let ((args, args_span), args_height) =
                        cursor.measured(interner, |cursor, interner| {
                            Arguments::new(self.allow_yield, self.allow_await)
                                .parse(cursor, interner)
                        })?;
                    let charge = if after_access {
                        NestingCharge::MethodLink
                    } else {
                        NestingCharge::MemberLink
                    };
                    height = cursor.chain_height(height.max(args_height), charge, interner)?;
                    after_access = false;
                    (OptionalOperationKind::Call { args }, args_span)
                }
                TokenKind::Punctuator(Punctuator::OpenBracket) => {
                    cursor
                        .next(interner)?
                        .expect("open bracket punctuator token disappeared"); // We move the parser forward.
                    let (idx, idx_height) = cursor.measured(interner, |cursor, interner| {
                        Expression::new(true, self.allow_yield, self.allow_await)
                            .parse(cursor, interner)
                    })?;
                    let end = cursor
                        .expect(Punctuator::CloseBracket, "optional chain", interner)?
                        .span()
                        .end();
                    let charge = access_charge(cursor, interner)?;
                    height = cursor.chain_height(height.max(idx_height), charge, interner)?;
                    after_access = true;
                    (
                        OptionalOperationKind::SimplePropertyAccess {
                            field: PropertyAccessField::Expr(Box::new(idx)),
                        },
                        Span::new(token_span.start(), end),
                    )
                }
                TokenKind::TemplateMiddle(_) | TokenKind::TemplateNoSubstitution(_) => {
                    return Err(Error::general(
                        "Invalid tagged template on optional chain",
                        token_span.start(),
                    ));
                }
                _ => {
                    let token = cursor.next(interner)?.expect("token disappeared");
                    let (item, item_span) = parse_const_access(&token, interner)?;
                    let charge = access_charge(cursor, interner)?;
                    height = cursor.chain_height(height, charge, interner)?;
                    after_access = true;
                    (item, Span::new(token_span.start(), item_span.end()))
                }
            };

            items.push(OptionalOperation::new(item, shorted, item_span));
        }

        let end = items
            .last()
            .expect("There should be at least one item in the optional AST expression")
            .span()
            .end();

        let target_span_start = self.target.span().start();
        Ok((
            Optional::new(self.target, items.into(), Span::new(target_span_start, end)),
            height,
        ))
    }
}
