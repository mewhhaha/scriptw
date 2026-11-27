use super::*;

use boa_ast::expression::operator::{assign::Assign, update::Update};
use boa_interner::Sym;
use std::convert::Infallible;

#[derive(Debug, Clone)]
pub(crate) struct InlineBinding {
    pub(crate) name: String,
    pub(crate) target: FunctionId,
}

pub(crate) type InlineBindings = BTreeMap<(EnvironmentId, String), InlineBinding>;

pub(super) fn collect(
    analysis: &Analysis<'_>,
    script: &Script,
    interner: &Interner,
    dynamically_visible: bool,
) -> InlineBindings {
    if dynamically_visible || !analysis.with_environment_ids.is_empty() {
        return BTreeMap::new();
    }

    let mut uses = SourceUses::default();
    let _ = script.visit_with(&mut uses);
    let mut bindings = BTreeMap::new();
    for (owner_id, owner) in &analysis.owner_plans {
        if owner_id == SCRIPT_OWNER_ID || owner.body_environment_id.is_some() {
            // Script var declarations are mutable global-object properties.
            continue;
        }
        for (name, id) in &owner.function_bindings {
            if !owner.strict && owner.parameter_names.contains(name) {
                // Sloppy mapped `arguments[i]` can overwrite a parameter-aliased
                // function declaration without a syntactic write to `name`.
                continue;
            }
            let Some(sym) = interner.get(name.as_str()) else {
                continue;
            };
            if !uses.stable_declaration(sym)
                || analysis
                    .annex_b_function_plans
                    .values()
                    .any(|plan| plan.copy_to_variable_environment && plan.source_name == *name)
            {
                continue;
            }
            bindings.insert(
                (owner.activation_environment_id, name.clone()),
                InlineBinding {
                    name: name.clone(),
                    target: id.clone(),
                },
            );
        }
    }

    for (key, identifier) in &uses.declarations {
        if !uses.stable_declaration(identifier.sym()) {
            continue;
        }
        let Some(id) = analysis.function_declaration_ids.get(key) else {
            continue;
        };
        let Some(owner) = analysis.owner_plans.get(id) else {
            continue;
        };
        let environment =
            &analysis.environment_plans[&owner.definition_environment_cursor.environment_id];
        if matches!(
            environment.kind,
            EnvironmentKind::Activation | EnvironmentKind::FunctionBody
        ) {
            // Root declarations were covered by the owner's winning binding.
            continue;
        }
        let name = interner.resolve_expect(identifier.sym()).to_string();
        let storage = analysis
            .annex_b_function_plans
            .get(key)
            .map(|plan| plan.block_storage_name.clone())
            .unwrap_or_else(|| scoped_lexical_binding_storage_name(&name, identifier.span()));
        if environment.binding_storage_names.contains(&storage) {
            bindings.insert(
                (environment.id, storage),
                InlineBinding {
                    name,
                    target: id.clone(),
                },
            );
        }
    }

    for (key, identifier) in uses.const_functions {
        if uses.constructs.contains(&identifier.sym()) {
            continue;
        }
        let Some(id) = analysis.function_expr_ids.get(&key) else {
            continue;
        };
        let Some(owner) = analysis.owner_plans.get(id) else {
            continue;
        };
        let environment =
            &analysis.environment_plans[&owner.definition_environment_cursor.environment_id];
        let name = interner.resolve_expect(identifier.sym()).to_string();
        let scoped = scoped_lexical_binding_storage_name(&name, identifier.span());
        let storage = if environment.binding_storage_names.contains(&scoped) {
            scoped
        } else if environment.binding_modes.get(&name) == Some(&BindingMode::Const) {
            name.clone()
        } else {
            continue;
        };
        bindings.insert(
            (environment.id, storage),
            InlineBinding {
                name,
                target: id.clone(),
            },
        );
    }
    bindings
}

/// A consumer that already captures the producer's binding can also read
/// the producer's other cells in that record. These cells already exist.
/// Lowering still checks the actual identifier resolution at each call.
pub(super) fn retain_shared_captures(analysis: &mut Analysis<'_>) {
    if analysis.closed_inline_bindings.is_empty() {
        return;
    }
    let mut additions = BTreeMap::new();
    for (caller_id, caller) in &analysis.function_plans {
        let owner = &analysis.owner_plans[caller_id];
        if owner.body_environment_id.is_some() {
            continue;
        }
        let mut additional = BTreeMap::new();
        for (target_storage, target_capture) in &caller.captures {
            let Some(target_id) =
                owner_target_for_capture(analysis, target_storage, target_capture)
            else {
                continue;
            };
            let Some(target) = analysis.function_plans.get(target_id) else {
                continue;
            };
            for (storage, capture) in &target.captures {
                if capture.environment_id != target_capture.environment_id
                    || capture.source_name.contains('.')
                    || owner.root_bindings.contains(&capture.source_name)
                    || caller
                        .captures
                        .values()
                        .any(|other| other.source_name == capture.source_name)
                    || additional
                        .values()
                        .any(|other: &CaptureBindingPlan| other.source_name == capture.source_name)
                {
                    continue;
                }
                let mut capture = capture.clone();
                capture.hops = target_capture.hops;
                additional.insert(storage.clone(), capture);
            }
        }
        if !additional.is_empty() {
            additions.insert(caller_id.clone(), additional);
        }
    }
    for (caller, captures) in additions {
        analysis
            .function_plans
            .get_mut(&caller)
            .expect("planned capture owner")
            .captures
            .extend(captures);
    }
}

fn owner_target_for_capture<'a>(
    analysis: &'a Analysis<'_>,
    storage: &str,
    capture: &CaptureBindingPlan,
) -> Option<&'a FunctionId> {
    let binding = analysis
        .closed_inline_bindings
        .get(&(capture.environment_id, storage.to_owned()))?;
    (binding.name == capture.source_name).then_some(&binding.target)
}

#[derive(Default)]
struct SourceUses {
    writes: BTreeSet<Sym>,
    constructs: BTreeSet<Sym>,
    const_functions: Vec<(String, boa_ast::expression::Identifier)>,
    declarations: Vec<(String, boa_ast::expression::Identifier)>,
}

impl SourceUses {
    fn stable_declaration(&self, name: Sym) -> bool {
        !self.writes.contains(&name) && !self.constructs.contains(&name)
    }

    fn record_loop_write(&mut self, initializer: &IterableLoopInitializer) {
        match initializer {
            IterableLoopInitializer::Identifier(name) => {
                self.writes.insert(name.sym());
            }
            IterableLoopInitializer::Var(variable) => {
                self.writes
                    .extend(boa_ast::operations::bound_names(variable.binding()));
            }
            IterableLoopInitializer::Pattern(pattern) => {
                self.writes
                    .extend(boa_ast::operations::bound_names(pattern));
            }
            _ => {}
        }
    }
}

impl<'ast> Visitor<'ast> for SourceUses {
    type BreakTy = Infallible;

    fn visit_function_declaration(
        &mut self,
        function: &'ast FunctionDeclaration,
    ) -> ControlFlow<Infallible> {
        self.declarations
            .push((function_declaration_key(function), function.name()));
        function.visit_with(self)
    }

    fn visit_lexical_declaration(
        &mut self,
        declaration: &'ast LexicalDeclaration,
    ) -> ControlFlow<Infallible> {
        if let LexicalDeclaration::Const(variables) = declaration {
            for variable in variables.as_ref() {
                let (Binding::Identifier(identifier), Some(init)) =
                    (variable.binding(), variable.init())
                else {
                    continue;
                };
                let key = match init.flatten() {
                    Expression::ArrowFunction(function) => arrow_function_key(function),
                    Expression::FunctionExpression(function) => function_expression_key(function),
                    _ => continue,
                };
                self.const_functions.push((key, *identifier));
            }
        }
        declaration.visit_with(self)
    }

    fn visit_var_declaration(
        &mut self,
        declaration: &'ast VarDeclaration,
    ) -> ControlFlow<Infallible> {
        for variable in declaration
            .0
            .as_ref()
            .iter()
            .filter(|variable| variable.init().is_some())
        {
            self.writes
                .extend(boa_ast::operations::bound_names(variable.binding()));
        }
        declaration.visit_with(self)
    }

    fn visit_assign(&mut self, assign: &'ast Assign) -> ControlFlow<Infallible> {
        match assign.lhs() {
            AssignTarget::Identifier(name) => {
                self.writes.insert(name.sym());
            }
            AssignTarget::Pattern(pattern) => {
                self.writes
                    .extend(boa_ast::operations::bound_names(pattern));
            }
            _ => {}
        }
        assign.visit_with(self)
    }

    fn visit_update(&mut self, update: &'ast Update) -> ControlFlow<Infallible> {
        if let UpdateTarget::Identifier(name) = update.target() {
            self.writes.insert(name.sym());
        }
        update.visit_with(self)
    }

    fn visit_for_in_loop(
        &mut self,
        source: &'ast boa_ast::statement::iteration::ForInLoop,
    ) -> ControlFlow<Infallible> {
        self.record_loop_write(source.initializer());
        source.visit_with(self)
    }

    fn visit_for_of_loop(&mut self, source: &'ast ForOfLoop) -> ControlFlow<Infallible> {
        self.record_loop_write(source.initializer());
        source.visit_with(self)
    }

    fn visit_new(&mut self, source: &'ast New) -> ControlFlow<Infallible> {
        if let Expression::Identifier(identifier) = source.constructor().flatten() {
            self.constructs.insert(identifier.sym());
        }
        source.visit_with(self)
    }
}
