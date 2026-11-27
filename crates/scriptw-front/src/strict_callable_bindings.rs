//! Callable headers acquire strictness from their own body directive.
//!
//! Boa reads a name and formal parameters before it reads that directive. Its
//! late checks cover eval/arguments but omit the strict reserved identifiers.
//! Validate those lexical bindings on successful syntax, using the parser's
//! decoded symbols. Contextual Yield/Await grammar remains the parser's owner.

use super::{early_error_code::TYPED_STRICT_BINDING_CODE, ParseError, SourceSpan};
use boa_ast::{
    expression::{
        literal::ObjectMethodDefinition, Expression, Identifier, ImportCall,
        RESERVED_IDENTIFIERS_STRICT,
    },
    function::{
        ArrowFunction, AsyncArrowFunction, AsyncFunctionDeclaration, AsyncFunctionExpression,
        AsyncGeneratorDeclaration, AsyncGeneratorExpression, FormalParameterList, FunctionBody,
        FunctionDeclaration, FunctionExpression, GeneratorDeclaration, GeneratorExpression,
    },
    property::PropertyName,
    visitor::{VisitWith, Visitor},
    Position, Spanned,
};
use std::ops::ControlFlow;

/// Admit every callable binding and retain whether the same walk saw `import()`.
pub(super) fn validate<N: VisitWith>(
    ast: &N,
    span: impl FnOnce(Position) -> Option<SourceSpan>,
) -> Result<bool, ParseError> {
    let mut headers = CallableHeaders::default();
    if let ControlFlow::Break(identifier) = ast.visit_with(&mut headers) {
        return Err(ParseError::early_error(
            TYPED_STRICT_BINDING_CODE,
            "strict callable header contains a reserved binding identifier",
            span(identifier.span().start()),
        ));
    }
    Ok(headers.has_dynamic_import)
}

struct ParameterBindings;

impl<'ast> Visitor<'ast> for ParameterBindings {
    type BreakTy = Identifier;

    fn visit_identifier(&mut self, node: &'ast Identifier) -> ControlFlow<Self::BreakTy> {
        if RESERVED_IDENTIFIERS_STRICT.contains(&node.sym()) {
            ControlFlow::Break(*node)
        } else {
            ControlFlow::Continue(())
        }
    }

    // BoundNames excludes initializer expressions and property keys. Nested
    // callables in those expressions are visited separately by CallableHeaders.
    fn visit_expression(&mut self, _: &'ast Expression) -> ControlFlow<Self::BreakTy> {
        ControlFlow::Continue(())
    }

    fn visit_property_name(&mut self, _: &'ast PropertyName) -> ControlFlow<Self::BreakTy> {
        ControlFlow::Continue(())
    }
}

fn check_header(
    name: Option<Identifier>,
    parameters: &FormalParameterList,
    body: &FunctionBody,
) -> ControlFlow<Identifier> {
    if body.strict() {
        if let Some(name) = name {
            ParameterBindings.visit_identifier(&name)?;
        }
        parameters.visit_with(&mut ParameterBindings)?;
    }
    ControlFlow::Continue(())
}

#[derive(Default)]
struct CallableHeaders {
    has_dynamic_import: bool,
}

macro_rules! declaration_header {
    ($visit:ident, $node:ident) => {
        fn $visit(&mut self, node: &'ast $node) -> ControlFlow<Self::BreakTy> {
            check_header(Some(node.name()), node.parameters(), node.body())?;
            node.visit_with(self)
        }
    };
}

macro_rules! expression_header {
    ($visit:ident, $node:ident) => {
        fn $visit(&mut self, node: &'ast $node) -> ControlFlow<Self::BreakTy> {
            // Inferred display names are not lexical BindingIdentifiers.
            let name = if node.has_binding_identifier() {
                node.name()
            } else {
                None
            };
            check_header(name, node.parameters(), node.body())?;
            node.visit_with(self)
        }
    };
}

macro_rules! unnamed_header {
    ($visit:ident, $node:ident) => {
        fn $visit(&mut self, node: &'ast $node) -> ControlFlow<Self::BreakTy> {
            check_header(None, node.parameters(), node.body())?;
            node.visit_with(self)
        }
    };
}

impl<'ast> Visitor<'ast> for CallableHeaders {
    type BreakTy = Identifier;

    fn visit_import_call(&mut self, node: &'ast ImportCall) -> ControlFlow<Self::BreakTy> {
        self.has_dynamic_import = true;
        node.visit_with(self)
    }

    declaration_header!(visit_function_declaration, FunctionDeclaration);
    declaration_header!(visit_generator_declaration, GeneratorDeclaration);
    declaration_header!(visit_async_function_declaration, AsyncFunctionDeclaration);
    declaration_header!(visit_async_generator_declaration, AsyncGeneratorDeclaration);
    expression_header!(visit_function_expression, FunctionExpression);
    expression_header!(visit_generator_expression, GeneratorExpression);
    expression_header!(visit_async_function_expression, AsyncFunctionExpression);
    expression_header!(visit_async_generator_expression, AsyncGeneratorExpression);
    unnamed_header!(visit_arrow_function, ArrowFunction);
    unnamed_header!(visit_async_arrow_function, AsyncArrowFunction);
    unnamed_header!(visit_object_method_definition, ObjectMethodDefinition);
}
