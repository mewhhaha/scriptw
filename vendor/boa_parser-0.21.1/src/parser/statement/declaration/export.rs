//! Export declaration parsing
//!
//! This parses `export` declarations.
//!
//! More information:
//! - [MDN documentation][mdn]
//!  - [ECMAScript specification][spec]
//!
//! [spec]: https://tc39.es/ecma262/#sec-exports
//! [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Statements/export

use crate::{
    lexer::{Token, TokenKind, token::ContainsEscapeSequence},
    parser::{
        Error, OrAbrupt, ParseResult, TokenParser,
        cursor::Cursor,
        expression::AssignmentExpression,
        statement::{declaration::ClassDeclaration, variable::VariableStatement},
    },
    source::ReadChar,
};
use boa_ast::{
    Keyword, Punctuator, Spanned,
    declaration::{ExportDeclaration as AstExportDeclaration, ReExportKind},
};
use boa_interner::{Interner, Sym};

use super::{
    Declaration, FromClause, FunctionDeclaration,
    hoistable::{AsyncFunctionDeclaration, AsyncGeneratorDeclaration, GeneratorDeclaration},
    import::parse_re_export_request,
};

/// Parses an export declaration.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-ExportDeclaration
#[derive(Debug, Clone, Copy)]
pub(in crate::parser) struct ExportDeclaration;

impl<R> TokenParser<R> for ExportDeclaration
where
    R: ReadChar,
{
    type Output = AstExportDeclaration;

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        cursor.expect((Keyword::Export, false), "export declaration", interner)?;

        let tok = cursor.peek(0, interner).or_abrupt()?;
        let span = tok.span();

        let export_clause: Self::Output = match tok.kind() {
            TokenKind::Punctuator(Punctuator::Mul) => {
                cursor.advance(interner);

                let next = cursor.peek(0, interner).or_abrupt()?;

                let (kind, specifier) = match next.kind() {
                    TokenKind::IdentifierName((Sym::AS, _)) => {
                        cursor.advance(interner);
                        let tok = cursor.next(interner).or_abrupt()?;

                        let alias = match tok.kind() {
                            TokenKind::StringLiteral((export_name, _))
                            | TokenKind::IdentifierName((export_name, _)) => *export_name,
                            TokenKind::Keyword((kw, _)) => kw.to_sym(),
                            _ => {
                                return Err(Error::expected(
                                    ["identifier name".to_owned(), "string literal".to_owned()],
                                    tok.to_string(interner),
                                    tok.span(),
                                    "export declaration",
                                ));
                            }
                        };

                        let specifier =
                            FromClause::new("export declaration").parse(cursor, interner)?;

                        (ReExportKind::Namespaced { name: Some(alias) }, specifier)
                    }
                    TokenKind::IdentifierName((Sym::FROM, _)) => {
                        let specifier =
                            FromClause::new("export declaration").parse(cursor, interner)?;

                        (ReExportKind::Namespaced { name: None }, specifier)
                    }
                    _ => {
                        return Err(Error::expected(
                            ["as".to_owned(), "from".to_owned()],
                            next.to_string(interner),
                            next.span(),
                            "export declaration",
                        ));
                    }
                };

                let request = parse_re_export_request(cursor, interner, specifier)?;
                cursor.expect_semicolon("star re-export", interner)?;

                AstExportDeclaration::ReExport { kind, request }
            }
            TokenKind::Punctuator(Punctuator::OpenBlock) => {
                let names = NamedExports.parse(cursor, interner)?;

                let next = cursor.peek(0, interner).or_abrupt()?;

                if matches!(
                    next.kind(),
                    TokenKind::IdentifierName((Sym::FROM, ContainsEscapeSequence(false)))
                ) {
                    let specifier =
                        FromClause::new("export declaration").parse(cursor, interner)?;

                    let request = parse_re_export_request(cursor, interner, specifier)?;
                    cursor.expect_semicolon("named re-exports", interner)?;

                    AstExportDeclaration::ReExport {
                        kind: ReExportKind::Named { names },
                        request,
                    }
                } else {
                    cursor.expect_semicolon("named exports", interner)?;

                    for specifier in &*names {
                        let name = specifier.private_name();

                        if specifier.string_literal() {
                            let name = interner.resolve_expect(name);
                            return Err(Error::general(
                                format!(
                                    "local referenced binding `{name}` cannot be a string literal",
                                ),
                                span.start(),
                            ));
                        }

                        if name == Sym::AWAIT
                            || name.is_reserved_identifier()
                            || name.is_strict_reserved_identifier()
                        {
                            let name = interner.resolve_expect(name);
                            return Err(Error::general(
                                format!(
                                    "local referenced binding `{name}` cannot be a reserved word",
                                ),
                                span.start(),
                            ));
                        }
                    }

                    AstExportDeclaration::List(names)
                }
            }
            // Scriptw: the compiler's reparse deletes `export`, so the
            // VariableStatement is reached through a Statement there; it is
            // charged that level here (see `MODULE_BODY_ENCLOSING_LEVELS`).
            TokenKind::Keyword((Keyword::Var, false)) => cursor
                .nested(interner, |cursor, interner| {
                    VariableStatement::new(false, true).parse(cursor, interner)
                })
                .map(AstExportDeclaration::VarStatement)?,
            TokenKind::Keyword((Keyword::Default, false)) => {
                cursor.advance(interner);

                // Scriptw: only `async [no LineTerminator here] function` starts a
                // declaration here (ECMA-262 16.2.3); any other `async` begins the
                // AssignmentExpression (`async () => 1`, `async(1)`, the identifier).
                let async_function_declaration = if cursor.peek(0, interner).or_abrupt()?.kind()
                    == &TokenKind::Keyword((Keyword::Async, false))
                {
                    let skip_n = if cursor.peek_is_line_terminator(0, interner).or_abrupt()? {
                        2
                    } else {
                        1
                    };
                    let line_terminator = cursor
                        .peek_is_line_terminator(skip_n, interner)?
                        .unwrap_or(true);
                    !line_terminator
                        && matches!(
                            cursor.peek(1, interner)?.map(Token::kind),
                            Some(TokenKind::Keyword((Keyword::Function, _)))
                        )
                } else {
                    false
                };

                let reparse =
                    DefaultDeclarationReparse::of(cursor, interner, async_function_declaration)?;
                reparse.charged(cursor, interner, |cursor, interner| {
                    let tok = cursor.peek(0, interner).or_abrupt()?;

                    Ok(match tok.kind() {
                        TokenKind::Keyword((Keyword::Function, false)) => {
                            let next_token = cursor.peek(1, interner).or_abrupt()?;
                            if next_token.kind() == &TokenKind::Punctuator(Punctuator::Mul) {
                                AstExportDeclaration::DefaultGeneratorDeclaration(
                                    GeneratorDeclaration::new(false, true, true)
                                        .parse(cursor, interner)?,
                                )
                            } else {
                                AstExportDeclaration::DefaultFunctionDeclaration(
                                    FunctionDeclaration::new(false, true, true)
                                        .parse(cursor, interner)?,
                                )
                            }
                        }
                        TokenKind::Keyword((Keyword::Async, false))
                            if async_function_declaration =>
                        {
                            let next_token = cursor.peek(2, interner).or_abrupt()?;
                            if next_token.kind() == &TokenKind::Punctuator(Punctuator::Mul) {
                                AstExportDeclaration::DefaultAsyncGeneratorDeclaration(
                                    AsyncGeneratorDeclaration::new(false, true, true)
                                        .parse(cursor, interner)?,
                                )
                            } else {
                                AstExportDeclaration::DefaultAsyncFunctionDeclaration(
                                    AsyncFunctionDeclaration::new(false, true, true)
                                        .parse(cursor, interner)?,
                                )
                            }
                        }
                        TokenKind::Keyword((Keyword::Class, false)) => {
                            AstExportDeclaration::DefaultClassDeclaration(
                                ClassDeclaration::new(false, true, true)
                                    .parse(cursor, interner)?
                                    .into(),
                            )
                        }
                        _ => {
                            let expr = AssignmentExpression::new(true, false, true)
                                .parse(cursor, interner)?;

                            cursor.expect_semicolon("default expression export", interner)?;

                            AstExportDeclaration::DefaultAssignmentExpression(expr)
                        }
                    })
                })?
            }
            _ => AstExportDeclaration::Declaration(
                Declaration::new(false, true).parse(cursor, interner)?,
            ),
        };

        Ok(export_clause)
    }
}

/// Scriptw: how the compiler's reparse of a module spells an `export default`
/// form (`crates/scriptw-ir/src/modules/source.rs`), so the Module parse
/// charges it the nesting levels that reparse adds (see
/// `MODULE_BODY_ENCLOSING_LEVELS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DefaultDeclarationReparse {
    /// A named declaration keeps its text without the keywords, and an
    /// expression becomes the initializer of `let <name> = …`: the reparse
    /// charges them as this parse does.
    Unchanged,
    /// `function () {}`, and its generator and async forms, become
    /// `var <name> = function () {}`: a Statement and an AssignmentExpression.
    AnonymousFunction,
    /// `class {}` becomes `let <name> = class {}`: an AssignmentExpression.
    AnonymousClass,
}

impl DefaultDeclarationReparse {
    /// The form of the `export default` whose first token is the next one.
    fn of<R: ReadChar>(
        cursor: &mut Cursor<R>,
        interner: &mut Interner,
        async_function_declaration: bool,
    ) -> ParseResult<Self> {
        let mut kind = |skip_n: usize| -> ParseResult<Option<TokenKind>> {
            Ok(cursor
                .peek(skip_n, interner)?
                .map(|token| token.kind().clone()))
        };
        let open_paren = Some(TokenKind::Punctuator(Punctuator::OpenParen));
        let star = Some(TokenKind::Punctuator(Punctuator::Mul));
        // The index of the token after `function`, and after its `*`.
        let name_at = match kind(0)? {
            Some(TokenKind::Keyword((Keyword::Function, false))) => Some(1),
            Some(TokenKind::Keyword((Keyword::Async, false))) if async_function_declaration => {
                Some(2)
            }
            Some(TokenKind::Keyword((Keyword::Class, false))) => {
                return Ok(match kind(1)? {
                    Some(TokenKind::Punctuator(Punctuator::OpenBlock))
                    | Some(TokenKind::Keyword((Keyword::Extends, _))) => Self::AnonymousClass,
                    _ => Self::Unchanged,
                });
            }
            _ => None,
        };
        let Some(mut name_at) = name_at else {
            return Ok(Self::Unchanged);
        };
        if kind(name_at)? == star {
            name_at += 1;
        }
        Ok(if kind(name_at)? == open_paren {
            Self::AnonymousFunction
        } else {
            Self::Unchanged
        })
    }

    /// Parses the declaration with the levels the reparse adds to it.
    fn charged<R: ReadChar, T>(
        self,
        cursor: &mut Cursor<R>,
        interner: &mut Interner,
        parse: impl FnOnce(&mut Cursor<R>, &mut Interner) -> ParseResult<T>,
    ) -> ParseResult<T> {
        match self {
            Self::Unchanged => parse(cursor, interner),
            Self::AnonymousClass => cursor.nested(interner, parse),
            Self::AnonymousFunction => {
                cursor.nested(interner, |cursor, interner| cursor.nested(interner, parse))
            }
        }
    }
}

/// Parses a named export list.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-NamedExports
#[derive(Debug, Clone, Copy)]
struct NamedExports;

impl<R> TokenParser<R> for NamedExports
where
    R: ReadChar,
{
    type Output = Box<[boa_ast::declaration::ExportSpecifier]>;

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        cursor.expect(Punctuator::OpenBlock, "export declaration", interner)?;

        let mut list = Vec::new();

        loop {
            let tok = cursor.peek(0, interner).or_abrupt()?;
            match tok.kind() {
                TokenKind::Punctuator(Punctuator::CloseBlock) => {
                    cursor.advance(interner);
                    break;
                }
                TokenKind::Punctuator(Punctuator::Comma) => {
                    if list.is_empty() {
                        return Err(Error::expected(
                            [
                                Punctuator::CloseBlock.to_string(),
                                "string literal".to_owned(),
                                "identifier".to_owned(),
                            ],
                            tok.to_string(interner),
                            tok.span(),
                            "export declaration",
                        ));
                    }
                    cursor.advance(interner);
                }
                TokenKind::StringLiteral(_)
                | TokenKind::IdentifierName(_)
                | TokenKind::Keyword(_) => {
                    list.push(ExportSpecifier.parse(cursor, interner)?);
                }
                _ => {
                    return Err(Error::expected(
                        [
                            Punctuator::CloseBlock.to_string(),
                            Punctuator::Comma.to_string(),
                        ],
                        tok.to_string(interner),
                        tok.span(),
                        "export declaration",
                    ));
                }
            }
        }

        Ok(list.into_boxed_slice())
    }
}

/// Parses a module export name.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-ModuleExportName
#[derive(Debug, Clone, Copy)]
pub(super) struct ModuleExportName;

impl<R> TokenParser<R> for ModuleExportName
where
    R: ReadChar,
{
    type Output = (Sym, bool);

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        let tok = cursor.next(interner).or_abrupt()?;

        match tok.kind() {
            TokenKind::StringLiteral((ident, _)) => {
                if interner.resolve_expect(*ident).utf8().is_none() {
                    return Err(Error::general(
                        "import specifiers don't allow unpaired surrogates",
                        tok.span().end(),
                    ));
                }
                Ok((*ident, true))
            }
            TokenKind::IdentifierName((ident, _)) => Ok((*ident, false)),
            TokenKind::Keyword((kw, _)) => Ok((kw.to_sym(), false)),
            TokenKind::BooleanLiteral((b, _)) => {
                Ok((if *b { Sym::TRUE } else { Sym::FALSE }, false))
            }
            TokenKind::NullLiteral(_) => Ok((Sym::NULL, false)),
            _ => Err(Error::expected(
                ["identifier".to_owned(), "string literal".to_owned()],
                tok.to_string(interner),
                tok.span(),
                "export specifier parsing",
            )),
        }
    }
}

/// Parses an export specifier.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-ExportSpecifier
#[derive(Debug, Clone, Copy)]
struct ExportSpecifier;

impl<R> TokenParser<R> for ExportSpecifier
where
    R: ReadChar,
{
    type Output = boa_ast::declaration::ExportSpecifier;

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        let (inner_name, string_literal) = ModuleExportName.parse(cursor, interner)?;

        if cursor
            .next_if(TokenKind::identifier(Sym::AS), interner)?
            .is_some()
        {
            let (export_name, _) = ModuleExportName.parse(cursor, interner)?;
            Ok(boa_ast::declaration::ExportSpecifier::new(
                export_name,
                inner_name,
                string_literal,
            ))
        } else {
            Ok(boa_ast::declaration::ExportSpecifier::new(
                inner_name,
                inner_name,
                string_literal,
            ))
        }
    }
}
