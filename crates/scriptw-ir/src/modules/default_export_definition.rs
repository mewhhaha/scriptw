//! Anonymous default exports retain their actual canonical activation owner.

use super::record::DefaultExportFormIr;
use crate::*;

pub(super) fn rewrite_source(
    source: super::callable_source::OriginalUnitSource,
    syntax: Option<&super::source::ModuleSyntax>,
    rewrite: super::source::DefaultExportRewrite<'_>,
) -> Result<super::callable_source::OriginalUnitSource, String> {
    let Some(syntax) = syntax else {
        return Ok(source);
    };
    let text = source.text();
    let declaration_end = syntax.default_declaration_end(rewrite);
    let body =
        super::source::strip_module_syntax(text, syntax, rewrite).map_err(|error| error.reason)?;
    let mut source = source.stable_rewrite(body);
    if let Some(byte) = declaration_end {
        // A declaration needs no trailing semicolon, but its rewritten
        // variable initializer does. Module edits keep byte offsets stable;
        // insert only after it finishes, at the parsed definition boundary.
        // The later dynamic-import pass rescans this resulting text.
        source.replace(byte, byte, ";");
    }
    Ok(source)
}

/// The owner came from an exact trusted graph span, so inspect its actual AST
/// rather than reparsing a source-function wrapper with different lexical rules.
pub(super) fn apply_synchronous_default<'a>(
    body: &'a FunctionBody,
    spelling: &UnitLinkerSpelling,
    form: DefaultExportFormIr,
    analysis: &mut Analysis<'a>,
    interner: &Interner,
) {
    let DefaultExportFormIr::Anonymous { hoisted } = form else {
        return;
    };
    let binding_name = spelling.anonymous_default();
    let (binding, expression) = body
        .statements()
        .iter()
        .find_map(|statement| default_initializer(statement, binding_name.as_str(), interner))
        .expect("anonymous default retains its binding in the private module owner");
    let Some((name, _, key)) = definition(expression) else {
        assert!(!hoisted);
        return;
    };
    if name != Some(*binding) {
        assert!(!hoisted);
        return;
    }
    match key {
        DefinitionKey::Function(key) => {
            let id = analysis.function_expr_ids[&key].clone();
            analysis
                .function_plans
                .get_mut(&id)
                .expect("default function is analyzed")
                .name = "default".into();
            if hoisted {
                analysis.hoist_default_export_function(id, binding_name.as_str().to_string());
            }
        }
        DefinitionKey::Class(key) => {
            assert!(!hoisted);
            analysis
                .default_export_class_ids
                .insert(analysis.class_execution_ids[&key].clone());
        }
    }
}

fn default_initializer<'a>(
    statement: &'a StatementListItem,
    name: &str,
    interner: &Interner,
) -> Option<(&'a boa_ast::expression::Identifier, &'a Expression)> {
    let variables = match statement {
        StatementListItem::Declaration(declaration) => match declaration.as_ref() {
            Declaration::Lexical(declaration) => declaration.variable_list(),
            _ => return None,
        },
        StatementListItem::Statement(statement) => match statement.as_ref() {
            Statement::Var(declaration) => &declaration.0,
            _ => return None,
        },
    };
    variables.as_ref().iter().find_map(|variable| {
        let Binding::Identifier(identifier) = variable.binding() else {
            return None;
        };
        (interner.resolve_expect(identifier.sym()).to_string() == name)
            .then(|| {
                variable
                    .init()
                    .map(|expression| (identifier, expression.flatten()))
            })
            .flatten()
    })
}

enum DefinitionKey {
    Function(String),
    Class(String),
}

fn definition(
    expression: &Expression,
) -> Option<(
    Option<boa_ast::expression::Identifier>,
    boa_ast::LinearSpan,
    DefinitionKey,
)> {
    let (name, span, key) = match expression {
        Expression::FunctionExpression(function) => (
            function.name(),
            function.linear_span()?,
            function_expression_key(function),
        ),
        Expression::GeneratorExpression(function) => (
            function.name(),
            function.linear_span(),
            generator_expression_key(function),
        ),
        Expression::AsyncFunctionExpression(function) => (
            function.name(),
            function.linear_span(),
            async_function_expression_key(function),
        ),
        Expression::AsyncGeneratorExpression(function) => (
            function.name(),
            function.linear_span(),
            async_generator_expression_key(function),
        ),
        Expression::ArrowFunction(function) => (
            function.name(),
            function.linear_span(),
            arrow_function_key(function),
        ),
        Expression::AsyncArrowFunction(function) => (
            function.name(),
            function.linear_span(),
            async_arrow_function_key(function),
        ),
        Expression::ClassExpression(class) => {
            return Some((
                class.name(),
                class.linear_span(),
                DefinitionKey::Class(
                    class
                        .constructor()
                        .map(class_constructor_key)
                        .unwrap_or_else(|| class_default_constructor_key(class.linear_span())),
                ),
            ));
        }
        _ => return None,
    };
    Some((name, span, DefinitionKey::Function(key)))
}

impl Analysis<'_> {
    fn hoist_default_export_function(&mut self, id: FunctionId, binding_name: String) {
        let plan = self
            .function_plans
            .get_mut(&id)
            .expect("default export is analyzed");
        assert!(
            matches!(
                plan.protocol,
                FunctionProtocolIr::OrdinaryCallAndConstruct
                    | FunctionProtocolIr::Generator
                    | FunctionProtocolIr::Async
                    | FunctionProtocolIr::AsyncGenerator
            ),
            "only hoistable declarations enter module instantiation"
        );
        // The rewritten var already owns the binding and capture storage. Move
        // initialization into that owner's existing declaration-instantiation
        // list without changing the anonymous callable's exact source text.
        plan.is_expression = false;
        let owner = plan.parent_owner_id.clone();
        let function = PendingFunction {
            id: id.clone(),
            name: binding_name.clone(),
            to_string_representation: plan.to_string_representation.clone(),
            protocol: plan.protocol,
            strict: plan.strict,
            self_binding_name: plan.self_binding_name.clone(),
            parameters: plan.parameters,
            body: plan.body,
            is_expression: false,
            capture_aliases: BTreeMap::new(),
        };
        self.owner_plans
            .get_mut(&owner)
            .expect("default export owner is analyzed")
            .function_bindings
            .insert(binding_name, id.clone());
        if owner == SCRIPT_OWNER_ID {
            self.script_root_functions.push(function);
        } else {
            self.function_plans
                .get_mut(&owner)
                .expect("default export wrapper is analyzed")
                .root_functions
                .push(function);
        }
        self.hoisted_default_export_function_ids.insert(id);
    }

    pub(crate) fn is_hoisted_default_export_initializer(&self, item: &StatementListItem) -> bool {
        if self.hoisted_default_export_function_ids.is_empty() {
            return false;
        }
        let StatementListItem::Statement(statement) = item else {
            return false;
        };
        let Statement::Var(declaration) = statement.as_ref() else {
            return false;
        };
        let [variable] = declaration.0.as_ref() else {
            return false;
        };
        let Some(expression) = variable.init() else {
            return false;
        };
        let Some((_, _, DefinitionKey::Function(key))) = definition(expression.flatten()) else {
            return false;
        };
        self.function_expr_ids
            .get(&key)
            .is_some_and(|id| self.hoisted_default_export_function_ids.contains(id))
    }

    pub(crate) fn class_display_name<'a>(
        &self,
        constructor: &FunctionId,
        binding_name: &'a Option<String>,
    ) -> Option<&'a str> {
        if self.default_export_class_ids.contains(constructor) {
            Some("default")
        } else {
            binding_name.as_deref()
        }
    }
}
