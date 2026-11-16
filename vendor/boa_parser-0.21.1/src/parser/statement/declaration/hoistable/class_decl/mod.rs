#[cfg(test)]
mod tests;

use crate::{
    Error,
    lexer::{Error as LexError, TokenKind, token::ContainsEscapeSequence},
    parser::{
        AllowAwait, AllowDefault, AllowYield, Cursor, OrAbrupt, ParseResult, TokenParser,
        expression::{
            AssignmentExpression, AsyncGeneratorMethod, AsyncMethod, BindingIdentifier,
            Expression as ExpressionParser, GeneratorMethod, LeftHandSideExpression, PropertyName,
        },
        function::{FUNCTION_BREAK_TOKENS, FormalParameter, FunctionBody, UniqueFormalParameters},
        statement::StatementList,
    },
    source::ReadChar,
};
use ast::{
    function::FunctionBody as AstFunctionBody,
    function::PrivateName,
    operations::{
        check_labels, contains_invalid_object_literal, top_level_lexically_declared_names,
        top_level_var_declared_names,
    },
    property::MethodDefinitionKind,
};
use boa_ast::{
    self as ast, Expression, Keyword, LinearPosition, Position, Punctuator, Span, Spanned,
    expression::Identifier,
    function::{
        self, ClassDeclaration as ClassDeclarationNode, ClassElementName, ClassFieldDefinition,
        ClassMethodDefinition, FormalParameterList, FunctionExpression, PrivateFieldDefinition,
        StaticBlockBody,
    },
    operations::{ContainsSymbol, contains, contains_arguments},
};
use boa_interner::{Interner, Sym};
use boa_macros::utf16;
use rustc_hash::{FxHashMap, FxHashSet};

/// Class declaration parsing.
///
/// More information:
///  - [MDN documentation][mdn]
///  - [ECMAScript specification][spec]
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Statements/class
/// [spec]: https://tc39.es/ecma262/#prod-ClassDeclaration
#[derive(Debug, Clone, Copy)]
pub(in crate::parser) struct ClassDeclaration {
    allow_yield: AllowYield,
    allow_await: AllowAwait,
    is_default: AllowDefault,
}

impl ClassDeclaration {
    /// Creates a new `ClassDeclaration` parser.
    pub(in crate::parser) fn new<Y, A, D>(allow_yield: Y, allow_await: A, is_default: D) -> Self
    where
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
        D: Into<AllowDefault>,
    {
        Self {
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
            is_default: is_default.into(),
        }
    }
}

impl<R> TokenParser<R> for ClassDeclaration
where
    R: ReadChar,
{
    type Output = ClassDeclarationNode;

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        let decorators =
            parse_decorator_list(cursor, interner, self.allow_yield, self.allow_await)?
                .into_boxed_slice();
        let token = cursor.expect((Keyword::Class, false), "class declaration", interner)?;
        let span = token.span();
        let start_linear_span = token.linear_span();
        let strict = cursor.strict();
        cursor.set_strict(true);

        let token = cursor.peek(0, interner).or_abrupt()?;
        let name = match token.kind() {
            TokenKind::IdentifierName(_)
            | TokenKind::Keyword((Keyword::Yield | Keyword::Await, _)) => {
                BindingIdentifier::new(self.allow_yield, self.allow_await)
                    .parse(cursor, interner)?
            }
            TokenKind::Keyword((k, _)) if !k.to_sym().is_reserved_identifier() => {
                BindingIdentifier::new(self.allow_yield, self.allow_await)
                    .parse(cursor, interner)?
            }
            _ if self.is_default.0 => Identifier::new(Sym::DEFAULT, span),
            _ => {
                return Err(Error::unexpected(
                    token.to_string(interner),
                    token.span(),
                    "expected class identifier",
                ));
            }
        };
        cursor.set_strict(strict);

        let (super_ref, constructor, elements, _end, linear_end) =
            ClassTail::new(name, self.allow_yield, self.allow_await).parse(cursor, interner)?;

        let mut declaration = ClassDeclarationNode::new_with_decorators(
            name,
            super_ref,
            constructor,
            elements.into_boxed_slice(),
            decorators,
        );
        declaration.set_linear_span(start_linear_span.union(linear_end));

        Ok(declaration)
    }
}

/// Class Tail parsing.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-ClassTail
#[derive(Debug, Clone, Copy)]
pub(in crate::parser) struct ClassTail {
    name: Option<Identifier>,
    allow_yield: AllowYield,
    allow_await: AllowAwait,
}

impl ClassTail {
    /// Creates a new `ClassTail` parser.
    pub(in crate::parser) fn new<N, Y, A>(name: N, allow_yield: Y, allow_await: A) -> Self
    where
        N: Into<Option<Identifier>>,
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
    {
        Self {
            name: name.into(),
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
        }
    }
}

impl<R> TokenParser<R> for ClassTail
where
    R: ReadChar,
{
    type Output = (
        Option<Expression>,
        Option<FunctionExpression>,
        Vec<function::ClassElement>,
        Position,
        LinearPosition,
    );

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        let token = cursor.peek(0, interner).or_abrupt()?;
        let super_ref = match token.kind() {
            TokenKind::Keyword((Keyword::Extends, true)) => {
                return Err(Error::general(
                    "Keyword must not contain escaped characters",
                    token.span().start(),
                ));
            }
            TokenKind::Keyword((Keyword::Extends, false)) => Some(
                ClassHeritage::new(self.allow_yield, self.allow_await).parse(cursor, interner)?,
            ),
            _ => None,
        };

        cursor.expect(Punctuator::OpenBlock, "class tail", interner)?;

        // Temporarily disable strict mode because "strict" may be parsed as a keyword.
        let strict = cursor.strict();
        cursor.set_strict(false);
        let token = cursor.peek(0, interner).or_abrupt()?;
        let token_span_end = token.span().end();
        let token_linear_span_end = token.linear_span().end();
        let is_close_block = token.kind() == &TokenKind::Punctuator(Punctuator::CloseBlock);
        cursor.set_strict(strict);

        if is_close_block {
            cursor.advance(interner);
            Ok((
                super_ref,
                None,
                Vec::new(),
                token_span_end,
                token_linear_span_end,
            ))
        } else {
            let body_start = cursor.peek(0, interner).or_abrupt()?.span().start();
            let (constructor, elements) =
                ClassBody::new(self.name, self.allow_yield, self.allow_await)
                    .parse(cursor, interner)?;
            let close = cursor.expect(Punctuator::CloseBlock, "class tail", interner)?;
            let end = close.span().start();
            let linear_end = close.linear_span().end();

            if super_ref.is_none()
                && let Some(constructor) = &constructor
                && contains(constructor, ContainsSymbol::SuperCall)
            {
                return Err(Error::lex(LexError::Syntax(
                    "base class constructor cannot contain direct super call".into(),
                    body_start,
                )));
            }

            Ok((super_ref, constructor, elements, end, linear_end))
        }
    }
}

/// `ClassHeritage` parsing.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-ClassHeritage
#[derive(Debug, Clone, Copy)]
pub(in crate::parser) struct ClassHeritage {
    allow_yield: AllowYield,
    allow_await: AllowAwait,
}

impl ClassHeritage {
    /// Creates a new `ClassHeritage` parser.
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

impl<R> TokenParser<R> for ClassHeritage
where
    R: ReadChar,
{
    type Output = Expression;

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        cursor.expect(
            TokenKind::Keyword((Keyword::Extends, false)),
            "class heritage",
            interner,
        )?;

        let strict = cursor.strict();
        cursor.set_strict(true);
        let lhs = LeftHandSideExpression::new(self.allow_yield, self.allow_await)
            .parse(cursor, interner)?;
        cursor.set_strict(strict);

        Ok(lhs)
    }
}

/// `ClassBody` parsing.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-ClassBody
#[derive(Debug, Clone, Copy)]
pub(in crate::parser) struct ClassBody {
    name: Option<Identifier>,
    allow_yield: AllowYield,
    allow_await: AllowAwait,
}

impl ClassBody {
    /// Creates a new `ClassBody` parser.
    pub(in crate::parser) fn new<N, Y, A>(name: N, allow_yield: Y, allow_await: A) -> Self
    where
        N: Into<Option<Identifier>>,
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
    {
        Self {
            name: name.into(),
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
        }
    }
}

impl<R> TokenParser<R> for ClassBody
where
    R: ReadChar,
{
    type Output = (Option<FunctionExpression>, Vec<function::ClassElement>);

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        let mut constructor = None;
        let mut elements = Vec::new();
        let mut private_elements_names = FxHashMap::default();

        // The identifier "static" is forbidden in strict mode but used as a keyword in classes.
        // Because of this, strict mode has to temporarily be disabled while parsing class field names.
        let strict = cursor.strict();
        cursor.set_strict(false);
        loop {
            let token = cursor.peek(0, interner).or_abrupt()?;
            let position = token.span().start();
            let (parsed_constructor, element) = match token.kind() {
                TokenKind::Punctuator(Punctuator::CloseBlock) => break,
                _ => ClassElement::new(self.name, self.allow_yield, self.allow_await)
                    .parse(cursor, interner)?,
            };
            if let Some(c) = parsed_constructor {
                if constructor.is_some() {
                    return Err(Error::general(
                        "a class may only have one constructor",
                        position,
                    ));
                }
                constructor = Some(c);
            }
            let Some(element) = element else {
                continue;
            };

            match &element {
                function::ClassElement::MethodDefinition(m) => {
                    // It is a Syntax Error if PropName of MethodDefinition is not "constructor" and HasDirectSuper of MethodDefinition is true.
                    if let ClassElementName::PropertyName(name) = m.name()
                        && contains(name, ContainsSymbol::SuperCall)
                    {
                        return Err(Error::lex(LexError::Syntax(
                            "invalid super call usage".into(),
                            position,
                        )));
                    }
                    if contains(m.parameters(), ContainsSymbol::SuperCall)
                        || contains(m.body(), ContainsSymbol::SuperCall)
                    {
                        return Err(Error::ClassMethodHasDirectSuper { position });
                    }

                    if let ClassElementName::PrivateName(name) = m.name() {
                        match m.kind() {
                            MethodDefinitionKind::Get => {
                                match private_elements_names.get(&name.description()) {
                                    Some(PrivateElement::StaticSetter) if m.is_static() => {
                                        private_elements_names.insert(
                                            name.description(),
                                            PrivateElement::StaticValue,
                                        );
                                    }
                                    Some(PrivateElement::Setter) if !m.is_static() => {
                                        private_elements_names
                                            .insert(name.description(), PrivateElement::Value);
                                    }
                                    Some(_) => {
                                        return Err(Error::general(
                                            "private identifier has already been declared",
                                            position,
                                        ));
                                    }
                                    None => {
                                        private_elements_names.insert(
                                            name.description(),
                                            if m.is_static() {
                                                PrivateElement::StaticGetter
                                            } else {
                                                PrivateElement::Getter
                                            },
                                        );
                                    }
                                }
                            }
                            MethodDefinitionKind::Set => {
                                match private_elements_names.get(&name.description()) {
                                    Some(PrivateElement::StaticGetter) if m.is_static() => {
                                        private_elements_names.insert(
                                            name.description(),
                                            PrivateElement::StaticValue,
                                        );
                                    }
                                    Some(PrivateElement::Getter) if !m.is_static() => {
                                        private_elements_names
                                            .insert(name.description(), PrivateElement::Value);
                                    }
                                    Some(_) => {
                                        return Err(Error::general(
                                            "private identifier has already been declared",
                                            position,
                                        ));
                                    }
                                    None => {
                                        private_elements_names.insert(
                                            name.description(),
                                            if m.is_static() {
                                                PrivateElement::StaticSetter
                                            } else {
                                                PrivateElement::Setter
                                            },
                                        );
                                    }
                                }
                            }
                            _ => {
                                if private_elements_names
                                    .insert(
                                        name.description(),
                                        if m.is_static() {
                                            PrivateElement::StaticValue
                                        } else {
                                            PrivateElement::Value
                                        },
                                    )
                                    .is_some()
                                {
                                    return Err(Error::general(
                                        "private identifier has already been declared",
                                        position,
                                    ));
                                }
                            }
                        }
                    }
                }
                function::ClassElement::PrivateFieldDefinition(field) => {
                    if let Some(node) = field.initializer()
                        && contains(node, ContainsSymbol::SuperCall)
                    {
                        return Err(Error::lex(LexError::Syntax(
                            "class field initializer cannot contain super call".into(),
                            position,
                        )));
                    }
                    if private_elements_names
                        .insert(field.name().description(), PrivateElement::Value)
                        .is_some()
                    {
                        return Err(Error::general(
                            "private identifier has already been declared",
                            position,
                        ));
                    }
                }
                function::ClassElement::PrivateStaticFieldDefinition(field) => {
                    if let Some(node) = field.initializer()
                        && contains(node, ContainsSymbol::SuperCall)
                    {
                        return Err(Error::lex(LexError::Syntax(
                            "class field initializer cannot contain super call".into(),
                            position,
                        )));
                    }
                    if private_elements_names
                        .insert(field.name().description(), PrivateElement::StaticValue)
                        .is_some()
                    {
                        return Err(Error::general(
                            "private identifier has already been declared",
                            position,
                        ));
                    }
                }
                function::ClassElement::FieldDefinition(field)
                | function::ClassElement::AccessorFieldDefinition(field)
                | function::ClassElement::StaticFieldDefinition(field) => {
                    if let Some(field) = field.initializer()
                        && contains(field, ContainsSymbol::SuperCall)
                    {
                        return Err(Error::lex(LexError::Syntax(
                            "class field initializer cannot contain super call".into(),
                            position,
                        )));
                    }
                }
                function::ClassElement::StaticAccessorFieldDefinition(field) => {
                    if let Some(field) = field.initializer()
                        && contains(field, ContainsSymbol::SuperCall)
                    {
                        return Err(Error::lex(LexError::Syntax(
                            "class field initializer cannot contain super call".into(),
                            position,
                        )));
                    }
                }
                function::ClassElement::StaticBlock(_) => {}
            }
            elements.push(element);
        }

        cursor.set_strict(strict);

        Ok((constructor, elements))
    }
}

/// Representation of private object elements.
#[derive(Debug, PartialEq)]
pub(crate) enum PrivateElement {
    Value,
    Getter,
    Setter,
    StaticValue,
    StaticSetter,
    StaticGetter,
}

fn parse_decorator_list<R: ReadChar>(
    cursor: &mut Cursor<R>,
    interner: &mut Interner,
    allow_yield: AllowYield,
    allow_await: AllowAwait,
) -> ParseResult<Vec<Expression>> {
    let mut decorators = Vec::new();

    while matches!(
        cursor.peek(0, interner).or_abrupt()?.kind(),
        TokenKind::Punctuator(Punctuator::At)
    ) {
        cursor.advance(interner);
        decorators
            .push(ExpressionParser::new(true, allow_yield, allow_await).parse(cursor, interner)?);
    }

    Ok(decorators)
}

fn apply_decorators_to_element(
    element: function::ClassElement,
    decorators: Box<[Expression]>,
) -> function::ClassElement {
    match element {
        function::ClassElement::MethodDefinition(mut method) => {
            method.set_decorators(decorators);
            function::ClassElement::MethodDefinition(method)
        }
        function::ClassElement::FieldDefinition(mut field) => {
            field.set_decorators(decorators);
            function::ClassElement::FieldDefinition(field)
        }
        function::ClassElement::AccessorFieldDefinition(mut field) => {
            field.set_decorators(decorators);
            function::ClassElement::AccessorFieldDefinition(field)
        }
        function::ClassElement::StaticFieldDefinition(mut field) => {
            field.set_decorators(decorators);
            function::ClassElement::StaticFieldDefinition(field)
        }
        function::ClassElement::StaticAccessorFieldDefinition(mut field) => {
            field.set_decorators(decorators);
            function::ClassElement::StaticAccessorFieldDefinition(field)
        }
        function::ClassElement::PrivateFieldDefinition(mut field) => {
            field.set_decorators(decorators);
            function::ClassElement::PrivateFieldDefinition(field)
        }
        function::ClassElement::PrivateStaticFieldDefinition(mut field) => {
            field.set_decorators(decorators);
            function::ClassElement::PrivateStaticFieldDefinition(field)
        }
        function::ClassElement::StaticBlock(block) => function::ClassElement::StaticBlock(block),
    }
}

/// `ClassElement` parsing.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-ClassElement
#[derive(Debug, Clone, Copy)]
pub(in crate::parser) struct ClassElement {
    name: Option<Identifier>,
    allow_yield: AllowYield,
    allow_await: AllowAwait,
}

impl ClassElement {
    /// Creates a new `ClassElement` parser.
    pub(in crate::parser) fn new<N, Y, A>(name: N, allow_yield: Y, allow_await: A) -> Self
    where
        N: Into<Option<Identifier>>,
        Y: Into<AllowYield>,
        A: Into<AllowAwait>,
    {
        Self {
            name: name.into(),
            allow_yield: allow_yield.into(),
            allow_await: allow_await.into(),
        }
    }
}

impl<R> TokenParser<R> for ClassElement
where
    R: ReadChar,
{
    type Output = (Option<FunctionExpression>, Option<function::ClassElement>);

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        let decorators =
            parse_decorator_list(cursor, interner, self.allow_yield, self.allow_await)?
                .into_boxed_slice();
        let token = cursor.peek(0, interner).or_abrupt()?;
        let r#static = match token.kind() {
            TokenKind::Punctuator(Punctuator::Semicolon) => {
                cursor.advance(interner);
                return Ok((None, None));
            }
            TokenKind::IdentifierName((Sym::STATIC, ContainsEscapeSequence(contains_escape))) => {
                let contains_escape = *contains_escape;
                let token = cursor.peek(1, interner).or_abrupt()?;
                match token.kind() {
                    TokenKind::IdentifierName(_)
                    | TokenKind::StringLiteral(_)
                    | TokenKind::NumericLiteral(_)
                    | TokenKind::Keyword(_)
                    | TokenKind::NullLiteral(_)
                    | TokenKind::PrivateIdentifier(_)
                    | TokenKind::Punctuator(
                        Punctuator::OpenBracket | Punctuator::Mul | Punctuator::OpenBlock,
                    ) => {
                        if contains_escape {
                            return Err(Error::general(
                                "keyword must not contain escaped characters",
                                token.span().start(),
                            ));
                        }
                        // this "static" is a keyword.
                        cursor.advance(interner);
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        };

        let is_keyword = !matches!(
            cursor.peek(1, interner).or_abrupt()?.kind(),
            TokenKind::Punctuator(
                Punctuator::Assign
                    | Punctuator::CloseBlock
                    | Punctuator::OpenParen
                    | Punctuator::Semicolon
            )
        );
        let starts_constructor_method = matches!(
            cursor.peek(1, interner).or_abrupt()?.kind(),
            TokenKind::Punctuator(Punctuator::OpenParen)
        );

        let token = cursor.peek(0, interner).or_abrupt()?.clone();
        let start_linear_span = token.linear_span();
        let start_linear_pos = start_linear_span.start();
        let accessor_sym = interner.get_or_intern_static("accessor", utf16!("accessor"));

        let position = token.span().start();
        let element = match token.kind() {
            TokenKind::IdentifierName((Sym::CONSTRUCTOR, _))
                if !r#static && starts_constructor_method =>
            {
                cursor.advance(interner);
                let strict = cursor.strict();
                cursor.set_strict(true);

                let parameters =
                    UniqueFormalParameters::new(false, false).parse(cursor, interner)?;
                let body =
                    FunctionBody::new(false, false, "class constructor").parse(cursor, interner)?;
                cursor.set_strict(strict);

                let span = Some(start_linear_span.union(body.linear_pos_end()));

                let function_span_end = body.span().end();
                return Ok((
                    Some(FunctionExpression::new(
                        self.name,
                        parameters,
                        body,
                        span,
                        false,
                        Span::new(position, function_span_end),
                    )),
                    None,
                ));
            }
            TokenKind::Punctuator(Punctuator::OpenBlock) if r#static => {
                cursor.advance(interner);
                let (statement_list, end) = if let Some(token) =
                    cursor.next_if(TokenKind::Punctuator(Punctuator::CloseBlock), interner)?
                {
                    (ast::StatementList::default(), token.span().end())
                } else {
                    let strict = cursor.strict();
                    cursor.set_strict(true);
                    let position = cursor.peek(0, interner).or_abrupt()?.span().start();
                    let (statement_list, _end) =
                        StatementList::new(false, true, false, &FUNCTION_BREAK_TOKENS, false, true)
                            .parse(cursor, interner)?;

                    let mut lexical_names = FxHashSet::default();

                    // It is a Syntax Error if the LexicallyDeclaredNames of
                    // ClassStaticBlockStatementList contains any duplicate entries.
                    //
                    // The LexicallyDeclaredNames of a ClassStaticBlockStatementList are the
                    // TopLevelLexicallyDeclaredNames of its StatementList, and its
                    // VarDeclaredNames the TopLevelVarDeclaredNames: a function declaration
                    // at the top level of the block is var scoped, as in a function body.
                    for name in &top_level_lexically_declared_names(&statement_list) {
                        if !lexical_names.insert(*name) {
                            return Err(Error::general(
                                "lexical name declared multiple times",
                                position,
                            ));
                        }
                    }

                    // It is a Syntax Error if any element of the LexicallyDeclaredNames of
                    // ClassStaticBlockStatementList also occurs in the VarDeclaredNames of
                    // ClassStaticBlockStatementList.
                    for name in top_level_var_declared_names(&statement_list) {
                        if lexical_names.contains(&name) {
                            return Err(Error::general(
                                "lexical name declared in var names",
                                position,
                            ));
                        }
                    }

                    // It is a Syntax Error if ContainsDuplicateLabels of
                    // ClassStaticBlockStatementList with argument « » is true.
                    // It is a Syntax Error if ContainsUndefinedBreakTarget of
                    // ClassStaticBlockStatementList with argument « » is true.
                    // It is a Syntax Error if ContainsUndefinedContinueTarget of
                    // ClassStaticBlockStatementList with arguments « » and « » is true.
                    check_labels(&statement_list).map_err(|error| {
                        Error::lex(LexError::Syntax(error.message(interner).into(), position))
                    })?;

                    // It is a Syntax Error if ContainsArguments of ClassStaticBlockStatementList is true.
                    if contains_arguments(&statement_list) {
                        return Err(Error::general(
                            "'arguments' not allowed in class static block",
                            position,
                        ));
                    }

                    // It is a Syntax Error if ClassStaticBlockStatementList Contains SuperCall is true.
                    if contains(&statement_list, ContainsSymbol::SuperCall) {
                        return Err(Error::general(
                            "class static block cannot contain super call",
                            position,
                        ));
                    }

                    // It is a Syntax Error if ClassStaticBlockStatementList Contains await is true.
                    if contains(&statement_list, ContainsSymbol::AwaitExpression) {
                        return Err(Error::general("invalid await usage", position));
                    }

                    if contains_invalid_object_literal(&statement_list) {
                        return Err(Error::lex(LexError::Syntax(
                            "invalid object literal in class static block statement list".into(),
                            position,
                        )));
                    }

                    let end = cursor
                        .expect(
                            TokenKind::Punctuator(Punctuator::CloseBlock),
                            "class definition",
                            interner,
                        )?
                        .span()
                        .end();

                    cursor.set_strict(strict);

                    (statement_list, end)
                };
                function::ClassElement::StaticBlock(StaticBlockBody::new(AstFunctionBody::new(
                    statement_list,
                    Span::new(position, end),
                )))
            }
            TokenKind::Punctuator(Punctuator::Mul) => {
                let token = cursor.peek(1, interner).or_abrupt()?;
                let name_position = token.span().start();
                if !r#static && let TokenKind::IdentifierName((Sym::CONSTRUCTOR, _)) = token.kind()
                {
                    return Err(Error::general(
                        "class constructor may not be a generator method",
                        token.span().start(),
                    ));
                }
                let strict = cursor.strict();
                cursor.set_strict(true);
                let (class_element_name, params, body) =
                    GeneratorMethod::new(self.allow_yield, self.allow_await)
                        .parse(cursor, interner)?;
                cursor.set_strict(strict);

                let name = match class_element_name {
                    ClassElementName::PropertyName(name) => {
                        if r#static && name.literal().map(Identifier::sym) == Some(Sym::PROTOTYPE) {
                            return Err(Error::general(
                                "class may not have static method definitions named 'prototype'",
                                name_position,
                            ));
                        }
                        ClassElementName::PropertyName(name)
                    }
                    ClassElementName::PrivateName(name) => {
                        if name.description() == Sym::CONSTRUCTOR {
                            return Err(Error::general(
                                "class constructor may not be a private method",
                                name_position,
                            ));
                        }
                        ClassElementName::PrivateName(name)
                    }
                };
                function::ClassElement::MethodDefinition(ClassMethodDefinition::new(
                    name,
                    params,
                    body,
                    MethodDefinitionKind::Generator,
                    r#static,
                    start_linear_pos,
                ))
            }
            TokenKind::Keyword((Keyword::Async, true)) if is_keyword => {
                return Err(Error::general(
                    "Keyword must not contain escaped characters",
                    token.span().start(),
                ));
            }
            TokenKind::Keyword((Keyword::Async, false)) if is_keyword => {
                cursor.advance(interner);
                cursor.peek_expect_no_lineterminator(0, "Async object methods", interner)?;
                let token = cursor.peek(0, interner).or_abrupt()?;
                match token.kind() {
                    TokenKind::Punctuator(Punctuator::Mul) => {
                        let token = cursor.peek(1, interner).or_abrupt()?;
                        let name_position = token.span().start();
                        match token.kind() {
                            TokenKind::PrivateIdentifier(Sym::CONSTRUCTOR) => {
                                return Err(Error::general(
                                    "class constructor may not be a private method",
                                    token.span().start(),
                                ));
                            }
                            TokenKind::IdentifierName((Sym::CONSTRUCTOR, _)) if !r#static => {
                                return Err(Error::general(
                                    "class constructor may not be a generator method",
                                    token.span().start(),
                                ));
                            }
                            _ => {}
                        }
                        let strict = cursor.strict();
                        cursor.set_strict(true);
                        let (class_element_name, params, body) =
                            AsyncGeneratorMethod::new(self.allow_yield, self.allow_await)
                                .parse(cursor, interner)?;
                        cursor.set_strict(strict);

                        let name = match class_element_name {
                            ClassElementName::PropertyName(name) => {
                                if r#static
                                    && name.literal().map(Identifier::sym) == Some(Sym::PROTOTYPE)
                                {
                                    return Err(Error::general(
                                        "class may not have static method definitions named 'prototype'",
                                        name_position,
                                    ));
                                }
                                ClassElementName::PropertyName(name)
                            }
                            ClassElementName::PrivateName(name) => {
                                ClassElementName::PrivateName(name)
                            }
                        };

                        function::ClassElement::MethodDefinition(ClassMethodDefinition::new(
                            name,
                            params,
                            body,
                            MethodDefinitionKind::AsyncGenerator,
                            r#static,
                            start_linear_pos,
                        ))
                    }
                    TokenKind::IdentifierName((Sym::CONSTRUCTOR, _)) if !r#static => {
                        return Err(Error::general(
                            "class constructor may not be an async method",
                            token.span().start(),
                        ));
                    }
                    _ => {
                        let name_position = token.span().start();
                        let strict = cursor.strict();
                        cursor.set_strict(true);
                        let (class_element_name, params, body) =
                            AsyncMethod::new(self.allow_yield, self.allow_await)
                                .parse(cursor, interner)?;
                        cursor.set_strict(strict);

                        let name = match class_element_name {
                            ClassElementName::PropertyName(name) => {
                                if r#static
                                    && name.literal().map(Identifier::sym) == Some(Sym::PROTOTYPE)
                                {
                                    return Err(Error::general(
                                        "class may not have static method definitions named 'prototype'",
                                        name_position,
                                    ));
                                }
                                ClassElementName::PropertyName(name)
                            }
                            ClassElementName::PrivateName(name) => {
                                if r#static && name.description() == Sym::CONSTRUCTOR {
                                    return Err(Error::general(
                                        "class constructor may not be a private method",
                                        name_position,
                                    ));
                                }
                                ClassElementName::PrivateName(name)
                            }
                        };
                        function::ClassElement::MethodDefinition(ClassMethodDefinition::new(
                            name,
                            params,
                            body,
                            MethodDefinitionKind::Async,
                            r#static,
                            start_linear_pos,
                        ))
                    }
                }
            }
            TokenKind::IdentifierName((Sym::GET | Sym::SET, ContainsEscapeSequence(true)))
                if is_keyword =>
            {
                return Err(Error::general(
                    "keyword must not contain escaped characters",
                    token.span().start(),
                ));
            }
            TokenKind::IdentifierName((name, ContainsEscapeSequence(true)))
                if is_keyword && *name == accessor_sym =>
            {
                let accessor_span = token.span();
                cursor.advance(interner);
                let next_token = cursor.peek(0, interner).or_abrupt()?;
                if next_token.span().start().line_number() != accessor_span.end().line_number() {
                    cursor.expect_semicolon("expected semicolon", interner)?;
                    let field = ClassFieldDefinition::new(
                        Identifier::new(accessor_sym, accessor_span).into(),
                        None,
                    );
                    if r#static {
                        function::ClassElement::StaticFieldDefinition(field)
                    } else {
                        function::ClassElement::FieldDefinition(field)
                    }
                } else {
                    return Err(Error::general(
                        "keyword must not contain escaped characters",
                        token.span().start(),
                    ));
                }
            }
            TokenKind::IdentifierName((name, ContainsEscapeSequence(false)))
                if is_keyword && *name == accessor_sym =>
            {
                let accessor_span = token.span();
                cursor.advance(interner);
                let next_token = cursor.peek(0, interner).or_abrupt()?;
                if next_token.span().start().line_number() != accessor_span.end().line_number() {
                    cursor.expect_semicolon("expected semicolon", interner)?;
                    let field = ClassFieldDefinition::new(
                        Identifier::new(accessor_sym, accessor_span).into(),
                        None,
                    );
                    if r#static {
                        function::ClassElement::StaticFieldDefinition(field)
                    } else {
                        function::ClassElement::FieldDefinition(field)
                    }
                } else {
                    let start = next_token.span().start();
                    match next_token.kind() {
                        TokenKind::PrivateIdentifier(Sym::CONSTRUCTOR) => {
                            return Err(Error::general(
                                "class constructor may not be a private method",
                                next_token.span().start(),
                            ));
                        }
                        TokenKind::PrivateIdentifier(name) => {
                            let name = *name;
                            let name_span = next_token.span();
                            cursor.advance(interner);
                            let token = cursor.peek(0, interner).or_abrupt()?;
                            match token.kind() {
                                TokenKind::Punctuator(Punctuator::Assign) => {
                                    cursor.advance(interner);
                                    let strict = cursor.strict();
                                    cursor.set_strict(true);
                                    let mut rhs = AssignmentExpression::new(true, false, false)
                                        .parse(cursor, interner)?;
                                    cursor.expect_semicolon("expected semicolon", interner)?;
                                    cursor.set_strict(strict);
                                    let function_name = interner.get_or_intern(
                                        [utf16!("#"), interner.resolve_expect(name).utf16()]
                                            .concat()
                                            .as_slice(),
                                    );
                                    rhs.set_anonymous_function_definition_name(&Identifier::new(
                                        function_name,
                                        Span::new((1234, 1234), (1234, 1234)),
                                    ));
                                    let field = PrivateFieldDefinition::new_auto_accessor(
                                        PrivateName::new(name, name_span),
                                        Some(rhs),
                                    );
                                    if r#static {
                                        function::ClassElement::PrivateStaticFieldDefinition(field)
                                    } else {
                                        function::ClassElement::PrivateFieldDefinition(field)
                                    }
                                }
                                _ => {
                                    cursor.expect_semicolon("expected semicolon", interner)?;
                                    let field = PrivateFieldDefinition::new_auto_accessor(
                                        PrivateName::new(name, name_span),
                                        None,
                                    );
                                    if r#static {
                                        function::ClassElement::PrivateStaticFieldDefinition(field)
                                    } else {
                                        function::ClassElement::PrivateFieldDefinition(field)
                                    }
                                }
                            }
                        }
                        _ => {
                            let name = PropertyName::new(self.allow_yield, self.allow_await)
                                .parse(cursor, interner)?;
                            let token = cursor.peek(0, interner).or_abrupt()?;
                            let field = match token.kind() {
                                TokenKind::Punctuator(Punctuator::Assign) => {
                                    if let Some(name) = name.literal() {
                                        if r#static {
                                            if [Sym::CONSTRUCTOR, Sym::PROTOTYPE]
                                                .contains(&name.sym())
                                            {
                                                return Err(Error::general(
                                                    "class may not have static field definitions named 'constructor' or 'prototype'",
                                                    start,
                                                ));
                                            }
                                        } else if name == Sym::CONSTRUCTOR {
                                            return Err(Error::general(
                                                "class may not have field definitions named 'constructor'",
                                                start,
                                            ));
                                        }
                                    }
                                    cursor.advance(interner);
                                    let strict = cursor.strict();
                                    cursor.set_strict(true);
                                    let mut rhs = AssignmentExpression::new(true, false, false)
                                        .parse(cursor, interner)?;
                                    cursor.expect_semicolon("expected semicolon", interner)?;
                                    cursor.set_strict(strict);
                                    if let Some(name) = name.literal() {
                                        rhs.set_anonymous_function_definition_name(&name);
                                    }
                                    ClassFieldDefinition::new(name, Some(rhs))
                                }
                                _ => {
                                    if let Some(name) = name.literal() {
                                        if r#static {
                                            if [Sym::CONSTRUCTOR, Sym::PROTOTYPE]
                                                .contains(&name.sym())
                                            {
                                                return Err(Error::general(
                                                    "class may not have static field definitions named 'constructor' or 'prototype'",
                                                    start,
                                                ));
                                            }
                                        } else if name == Sym::CONSTRUCTOR {
                                            return Err(Error::general(
                                                "class may not have field definitions named 'constructor'",
                                                start,
                                            ));
                                        }
                                    }
                                    cursor.expect_semicolon("expected semicolon", interner)?;
                                    ClassFieldDefinition::new(name, None)
                                }
                            };

                            if r#static {
                                function::ClassElement::StaticAccessorFieldDefinition(field)
                            } else {
                                function::ClassElement::AccessorFieldDefinition(field)
                            }
                        }
                    }
                }
            }
            TokenKind::IdentifierName((Sym::GET, ContainsEscapeSequence(false))) if is_keyword => {
                cursor.advance(interner);
                let token = cursor.peek(0, interner).or_abrupt()?;
                match token.kind() {
                    TokenKind::PrivateIdentifier(Sym::CONSTRUCTOR) => {
                        return Err(Error::general(
                            "class constructor may not be a private method",
                            token.span().start(),
                        ));
                    }
                    TokenKind::PrivateIdentifier(name) => {
                        let name = *name;
                        let name_span = token.span();
                        cursor.advance(interner);
                        cursor.expect(Punctuator::OpenParen, "class getter", interner)?;
                        cursor.expect(Punctuator::CloseParen, "class getter", interner)?;

                        let strict = cursor.strict();
                        cursor.set_strict(true);
                        let body = FunctionBody::new(false, false, "class getter")
                            .parse(cursor, interner)?;
                        cursor.set_strict(strict);
                        function::ClassElement::MethodDefinition(ClassMethodDefinition::new(
                            ClassElementName::PrivateName(PrivateName::new(name, name_span)),
                            FormalParameterList::default(),
                            body,
                            MethodDefinitionKind::Get,
                            r#static,
                            start_linear_pos,
                        ))
                    }
                    TokenKind::IdentifierName((Sym::CONSTRUCTOR, _)) if !r#static => {
                        return Err(Error::general(
                            "class constructor may not be a getter method",
                            token.span().start(),
                        ));
                    }
                    TokenKind::IdentifierName(_)
                    | TokenKind::StringLiteral(_)
                    | TokenKind::NumericLiteral(_)
                    | TokenKind::Keyword(_)
                    | TokenKind::NullLiteral(_)
                    | TokenKind::Punctuator(Punctuator::OpenBracket) => {
                        let name_position = token.span().start();
                        let name = PropertyName::new(self.allow_yield, self.allow_await)
                            .parse(cursor, interner)?;
                        cursor.expect(
                            TokenKind::Punctuator(Punctuator::OpenParen),
                            "class getter",
                            interner,
                        )?;
                        cursor.expect(
                            TokenKind::Punctuator(Punctuator::CloseParen),
                            "class getter",
                            interner,
                        )?;

                        let strict = cursor.strict();
                        cursor.set_strict(true);
                        let body = FunctionBody::new(false, false, "class getter")
                            .parse(cursor, interner)?;
                        cursor.set_strict(strict);

                        if r#static && name.literal().map(Identifier::sym) == Some(Sym::PROTOTYPE) {
                            return Err(Error::general(
                                "class may not have static method definitions named 'prototype'",
                                name_position,
                            ));
                        }
                        function::ClassElement::MethodDefinition(ClassMethodDefinition::new(
                            ClassElementName::PropertyName(name),
                            FormalParameterList::default(),
                            body,
                            MethodDefinitionKind::Get,
                            r#static,
                            start_linear_pos,
                        ))
                    }
                    _ => {
                        let span = token.span();
                        cursor.expect_semicolon("expected semicolon", interner)?;
                        let field =
                            ClassFieldDefinition::new(Identifier::new(Sym::GET, span).into(), None);
                        if r#static {
                            function::ClassElement::StaticFieldDefinition(field)
                        } else {
                            function::ClassElement::FieldDefinition(field)
                        }
                    }
                }
            }
            TokenKind::IdentifierName((Sym::SET, ContainsEscapeSequence(false))) if is_keyword => {
                cursor.advance(interner);
                let token = cursor.peek(0, interner).or_abrupt()?;
                let start = token.span().start();
                match token.kind() {
                    TokenKind::PrivateIdentifier(Sym::CONSTRUCTOR) => {
                        return Err(Error::general(
                            "class constructor may not be a private method",
                            token.span().start(),
                        ));
                    }
                    TokenKind::PrivateIdentifier(name) => {
                        let name = *name;
                        let name_span = token.span();
                        cursor.advance(interner);
                        let strict = cursor.strict();
                        cursor.set_strict(true);
                        cursor.expect(Punctuator::OpenParen, "class setter", interner)?;
                        let param = FormalParameter::new(false, false).parse(cursor, interner)?;
                        cursor.expect(Punctuator::CloseParen, "class setter", interner)?;
                        let params = FormalParameterList::from(param);

                        let body = FunctionBody::new(false, false, "class setter")
                            .parse(cursor, interner)?;

                        // Early Error: It is a Syntax Error if FunctionBodyContainsUseStrict of FunctionBody is true
                        // and IsSimpleParameterList of PropertySetParameterList is false.
                        if body.strict() && !params.is_simple() {
                            return Err(Error::lex(LexError::Syntax(
                            "Illegal 'use strict' directive in function with non-simple parameter list"
                                .into(),
                                start,
                        )));
                        }
                        cursor.set_strict(strict);
                        function::ClassElement::MethodDefinition(ClassMethodDefinition::new(
                            ClassElementName::PrivateName(PrivateName::new(name, name_span)),
                            params,
                            body,
                            MethodDefinitionKind::Set,
                            r#static,
                            start_linear_pos,
                        ))
                    }
                    TokenKind::IdentifierName((Sym::CONSTRUCTOR, _)) if !r#static => {
                        return Err(Error::general(
                            "class constructor may not be a setter method",
                            start,
                        ));
                    }
                    TokenKind::IdentifierName(_)
                    | TokenKind::StringLiteral(_)
                    | TokenKind::NumericLiteral(_)
                    | TokenKind::Keyword(_)
                    | TokenKind::NullLiteral(_)
                    | TokenKind::Punctuator(Punctuator::OpenBracket) => {
                        let name = PropertyName::new(self.allow_yield, self.allow_await)
                            .parse(cursor, interner)?;
                        let strict = cursor.strict();
                        cursor.set_strict(true);
                        cursor.expect(Punctuator::OpenParen, "class setter", interner)?;
                        let param = FormalParameter::new(false, false).parse(cursor, interner)?;
                        cursor.expect(Punctuator::CloseParen, "class setter", interner)?;
                        let params = FormalParameterList::from(param);
                        let body = FunctionBody::new(false, false, "class setter")
                            .parse(cursor, interner)?;

                        // Early Error: It is a Syntax Error if FunctionBodyContainsUseStrict of FunctionBody is true
                        // and IsSimpleParameterList of PropertySetParameterList is false.
                        if body.strict() && !params.is_simple() {
                            return Err(Error::lex(LexError::Syntax(
                            "Illegal 'use strict' directive in function with non-simple parameter list"
                                .into(),
                                start,
                        )));
                        }
                        cursor.set_strict(strict);
                        if r#static && name.literal().map(Identifier::sym) == Some(Sym::PROTOTYPE) {
                            return Err(Error::general(
                                "class may not have static method definitions named 'prototype'",
                                start,
                            ));
                        }
                        function::ClassElement::MethodDefinition(ClassMethodDefinition::new(
                            ClassElementName::PropertyName(name),
                            params,
                            body,
                            MethodDefinitionKind::Set,
                            r#static,
                            start_linear_pos,
                        ))
                    }
                    _ => {
                        let span = token.span();
                        cursor.expect_semicolon("expected semicolon", interner)?;
                        let field =
                            ClassFieldDefinition::new(Identifier::new(Sym::SET, span).into(), None);
                        if r#static {
                            function::ClassElement::StaticFieldDefinition(field)
                        } else {
                            function::ClassElement::FieldDefinition(field)
                        }
                    }
                }
            }
            TokenKind::PrivateIdentifier(Sym::CONSTRUCTOR) => {
                return Err(Error::general(
                    "class constructor may not be a private method",
                    token.span().start(),
                ));
            }
            TokenKind::PrivateIdentifier(name) => {
                let name = *name;
                let name_span = token.span();
                cursor.advance(interner);
                let token = cursor.peek(0, interner).or_abrupt()?;
                let start = token.span().start();
                match token.kind() {
                    TokenKind::Punctuator(Punctuator::Assign) => {
                        cursor.advance(interner);
                        let strict = cursor.strict();
                        cursor.set_strict(true);
                        let mut rhs = AssignmentExpression::new(true, false, false)
                            .parse(cursor, interner)?;
                        cursor.expect_semicolon("expected semicolon", interner)?;
                        cursor.set_strict(strict);
                        let function_name = interner.get_or_intern(
                            [utf16!("#"), interner.resolve_expect(name).utf16()]
                                .concat()
                                .as_slice(),
                        );
                        rhs.set_anonymous_function_definition_name(&Identifier::new(
                            function_name,
                            Span::new((1234, 1234), (1234, 1234)),
                        ));
                        let field = PrivateFieldDefinition::new(
                            PrivateName::new(name, name_span),
                            Some(rhs),
                        );
                        if r#static {
                            function::ClassElement::PrivateStaticFieldDefinition(field)
                        } else {
                            function::ClassElement::PrivateFieldDefinition(field)
                        }
                    }
                    TokenKind::Punctuator(Punctuator::OpenParen) => {
                        let strict = cursor.strict();
                        cursor.set_strict(true);
                        let params =
                            UniqueFormalParameters::new(false, false).parse(cursor, interner)?;
                        let body = FunctionBody::new(false, false, "method definition")
                            .parse(cursor, interner)?;

                        // Early Error: It is a Syntax Error if FunctionBodyContainsUseStrict of FunctionBody is true
                        // and IsSimpleParameterList of UniqueFormalParameters is false.
                        if body.strict() && !params.is_simple() {
                            return Err(Error::lex(LexError::Syntax(
                                "Illegal 'use strict' directive in function with non-simple parameter list".into(),
                                start,
                            )));
                        }
                        cursor.set_strict(strict);
                        function::ClassElement::MethodDefinition(ClassMethodDefinition::new(
                            ClassElementName::PrivateName(PrivateName::new(name, name_span)),
                            params,
                            body,
                            MethodDefinitionKind::Ordinary,
                            r#static,
                            start_linear_pos,
                        ))
                    }
                    _ => {
                        cursor.expect_semicolon("expected semicolon", interner)?;
                        let field =
                            PrivateFieldDefinition::new(PrivateName::new(name, name_span), None);
                        if r#static {
                            function::ClassElement::PrivateStaticFieldDefinition(field)
                        } else {
                            function::ClassElement::PrivateFieldDefinition(field)
                        }
                    }
                }
            }
            TokenKind::IdentifierName(_)
            | TokenKind::StringLiteral(_)
            | TokenKind::NumericLiteral(_)
            | TokenKind::Keyword(_)
            | TokenKind::NullLiteral(_)
            | TokenKind::Punctuator(Punctuator::OpenBracket) => {
                let start = token.span().start();
                let name = PropertyName::new(self.allow_yield, self.allow_await)
                    .parse(cursor, interner)?;
                let token = cursor.peek(0, interner).or_abrupt()?;
                match token.kind() {
                    TokenKind::Punctuator(Punctuator::Assign) => {
                        if let Some(name) = name.literal() {
                            if r#static {
                                if [Sym::CONSTRUCTOR, Sym::PROTOTYPE].contains(&name.sym()) {
                                    return Err(Error::general(
                                        "class may not have static field definitions named 'constructor' or 'prototype'",
                                        start,
                                    ));
                                }
                            } else if name == Sym::CONSTRUCTOR {
                                return Err(Error::general(
                                    "class may not have field definitions named 'constructor'",
                                    start,
                                ));
                            }
                        }
                        cursor.advance(interner);
                        let strict = cursor.strict();
                        cursor.set_strict(true);
                        // Initializers execute in their own ordinary function;
                        // computed names above retain the enclosing parameters.
                        // See the spec-text issue tc39/ecma262#3333 and the
                        // pinned await-identifier Script/Module controls.
                        let mut rhs = AssignmentExpression::new(true, false, false)
                            .parse(cursor, interner)?;
                        cursor.expect_semicolon("expected semicolon", interner)?;
                        cursor.set_strict(strict);
                        if let Some(name) = name.literal() {
                            rhs.set_anonymous_function_definition_name(&name);
                        }
                        let field = ClassFieldDefinition::new(name, Some(rhs));
                        if r#static {
                            function::ClassElement::StaticFieldDefinition(field)
                        } else {
                            function::ClassElement::FieldDefinition(field)
                        }
                    }
                    TokenKind::Punctuator(Punctuator::OpenParen) => {
                        if r#static && name.literal().map(Identifier::sym) == Some(Sym::PROTOTYPE) {
                            return Err(Error::general(
                                "class may not have static method definitions named 'prototype'",
                                start,
                            ));
                        }
                        let strict = cursor.strict();
                        cursor.set_strict(true);
                        let params =
                            UniqueFormalParameters::new(false, false).parse(cursor, interner)?;

                        let body = FunctionBody::new(false, false, "method definition")
                            .parse(cursor, interner)?;

                        // Early Error: It is a Syntax Error if FunctionBodyContainsUseStrict of FunctionBody is true
                        // and IsSimpleParameterList of UniqueFormalParameters is false.
                        if body.strict() && !params.is_simple() {
                            return Err(Error::lex(LexError::Syntax(
                            "Illegal 'use strict' directive in function with non-simple parameter list"
                                .into(),
                                start,
                        )));
                        }
                        cursor.set_strict(strict);
                        if !r#static
                            && name.literal().map(Identifier::sym) == Some(Sym::CONSTRUCTOR)
                        {
                            let linear_span = start_linear_span.union(body.linear_pos_end());
                            let function_span_end = body.span().end();
                            return Ok((
                                Some(FunctionExpression::new(
                                    self.name,
                                    params,
                                    body,
                                    Some(linear_span),
                                    false,
                                    Span::new(start, function_span_end),
                                )),
                                None,
                            ));
                        }
                        function::ClassElement::MethodDefinition(ClassMethodDefinition::new(
                            ClassElementName::PropertyName(name),
                            params,
                            body,
                            MethodDefinitionKind::Ordinary,
                            r#static,
                            start_linear_pos,
                        ))
                    }
                    _ => {
                        if let Some(name) = name.literal() {
                            if r#static {
                                if [Sym::CONSTRUCTOR, Sym::PROTOTYPE].contains(&name.sym()) {
                                    return Err(Error::general(
                                        "class may not have static field definitions named 'constructor' or 'prototype'",
                                        start,
                                    ));
                                }
                            } else if name == Sym::CONSTRUCTOR {
                                return Err(Error::general(
                                    "class may not have field definitions named 'constructor'",
                                    start,
                                ));
                            }
                        }
                        cursor.expect_semicolon("expected semicolon", interner)?;
                        let field = ClassFieldDefinition::new(name, None);
                        if r#static {
                            function::ClassElement::StaticFieldDefinition(field)
                        } else {
                            function::ClassElement::FieldDefinition(field)
                        }
                    }
                }
            }
            _ => return Err(Error::general("unexpected token", token.span().start())),
        };

        match &element {
            // FieldDefinition : ClassElementName Initializer [opt]
            // It is a Syntax Error if Initializer is present and ContainsArguments of Initializer is true.
            function::ClassElement::FieldDefinition(field)
            | function::ClassElement::AccessorFieldDefinition(field)
            | function::ClassElement::StaticFieldDefinition(field) => {
                if let Some(field) = field.initializer()
                    && contains_arguments(field)
                {
                    return Err(Error::general(
                        "'arguments' not allowed in class field definition",
                        position,
                    ));
                }
            }
            function::ClassElement::StaticAccessorFieldDefinition(field) => {
                if let Some(field) = field.initializer()
                    && contains_arguments(field)
                {
                    return Err(Error::general(
                        "'arguments' not allowed in class field definition",
                        position,
                    ));
                }
            }
            function::ClassElement::PrivateFieldDefinition(field)
            | function::ClassElement::PrivateStaticFieldDefinition(field) => {
                if let Some(node) = field.initializer()
                    && contains_arguments(node)
                {
                    return Err(Error::general(
                        "'arguments' not allowed in class field definition",
                        position,
                    ));
                }
            }

            _ => {}
        }

        let element = if decorators.is_empty() {
            element
        } else {
            apply_decorators_to_element(element, decorators)
        };

        Ok((None, Some(element)))
    }
}
