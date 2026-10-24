//! Inference consumes ordinary bodies without retaining their emitted IR.
//!
//! Statement lowering remains the single transfer function for bindings,
//! coercions, calls and prepared-source ownership. Each completed statement is
//! reduced with the existing caller-flow and completion rules, then dropped.

use super::*;

#[cfg(test)]
thread_local! {
    static RETAIN_INFERENCE_BODIES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static STREAMED_INFERENCE_BODIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static STREAMED_ROOT_PREPASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn retain_complete_inference() -> bool {
    RETAIN_INFERENCE_BODIES.get()
}

/// Only an ordinary root list without resource declarations can be streamed.
/// Resource suffix ownership and suspension postprocessing retain complete IR.
pub(super) struct InferenceStatementList<'a>(&'a [StatementListItem]);

impl<'a> InferenceStatementList<'a> {
    pub(super) fn new(function: &FunctionPlan<'a>) -> Option<Self> {
        if function.protocol.execution_kind() != FunctionExecutionKind::Ordinary {
            return None;
        }
        Self::without_root_resources(function.body.statements())
    }

    pub(super) fn root(
        items: &'a [StatementListItem],
        lowerer: &ScriptLowerer<'_>,
    ) -> Option<Self> {
        if lowerer.current_generator_resume_state.is_some()
            || lowerer.current_async_resume_state.is_some()
        {
            return None;
        }
        Self::without_root_resources(items)
    }

    fn without_root_resources(items: &'a [StatementListItem]) -> Option<Self> {
        #[cfg(test)]
        if RETAIN_INFERENCE_BODIES.get() {
            return None;
        }
        if items.iter().any(|item| {
            matches!(item, StatementListItem::Declaration(declaration)
            if matches!(declaration.as_ref(), Declaration::Lexical(
                LexicalDeclaration::Using(_) | LexicalDeclaration::AwaitUsing(_)
            )))
        }) {
            return None;
        }
        Some(Self(items))
    }
}

pub(super) struct InferenceBodySummary {
    has_statements: bool,
    ends_in_return: bool,
    caller_flow: SourceCallFlowEffects,
}

impl InferenceBodySummary {
    pub(super) fn new() -> Self {
        Self {
            has_statements: false,
            ends_in_return: false,
            caller_flow: SourceCallFlowEffects::for_finalized_parameters(&[]),
        }
    }

    pub(super) fn observe(&mut self, statement: &StatementIr) {
        self.has_statements = true;
        self.ends_in_return =
            ScriptLowerer::statement_list_ends_in_return(std::slice::from_ref(statement));
        self.caller_flow = self
            .caller_flow
            .combine_caller_flow(SourceCallFlowEffects::for_finalized_statement(statement));
    }

    pub(super) fn append(&mut self, suffix: Self) {
        if suffix.has_statements {
            self.has_statements = true;
            self.ends_in_return = suffix.ends_in_return;
        }
        self.caller_flow = self.caller_flow.combine_caller_flow(suffix.caller_flow);
    }

    pub(super) fn ends_in_return(&self) -> bool {
        self.ends_in_return
    }

    pub(super) fn invocation_effects(&self, params: &[FunctionParamIr]) -> SourceCallFlowEffects {
        SourceCallFlowEffects::for_finalized_parameters(params)
            .combine_caller_flow(self.caller_flow)
    }
}

impl<'a> ScriptLowerer<'a> {
    pub(super) fn infer_root_statement_items(
        &mut self,
        items: InferenceStatementList<'_>,
        root_functions: &[PendingFunction<'a>],
        mut scope: LexicalScopeInstantiation,
    ) -> InferenceBodySummary {
        #[cfg(test)]
        if self.is_function_body {
            STREAMED_INFERENCE_BODIES.with(|count| count.set(count.get() + 1));
        } else {
            STREAMED_ROOT_PREPASSES.with(|count| count.set(count.get() + 1));
        }
        let source_domain = self.async_generator_source_domain;
        let root_functions = root_functions
            .iter()
            .map(|function| (function.name.clone(), function.id.clone()))
            .collect::<Vec<_>>();
        self.prepare_annex_b_function_bindings();
        self.prepare_root_function_binding_ids(&root_functions);
        let mut summary = InferenceBodySummary::new();
        for statement in self.root_function_init_statements(&root_functions) {
            summary.observe(&statement);
        }
        for item in items.0 {
            if let Some(item) = self.lower_statement_list_item_after_hoisting(
                item,
                &mut scope,
                StatementListPlacement::Root,
            ) {
                match item {
                    LoweredStatementListItemIr::Statement { statement, .. } => {
                        summary.observe(&statement);
                    }
                    LoweredStatementListItemIr::SyncDisposableScope { .. }
                    | LoweredStatementListItemIr::AsyncDisposableScope(_) => {
                        unreachable!("checked inference list excludes root resource declarations")
                    }
                }
            }
        }
        self.async_generator_source_domain = source_domain;
        scope.finish(self);
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(text: &str, retain: bool) -> ProgramIr {
        struct Restore(bool);
        impl Drop for Restore {
            fn drop(&mut self) {
                RETAIN_INFERENCE_BODIES.set(self.0);
            }
        }
        let _restore = Restore(RETAIN_INFERENCE_BODIES.replace(retain));
        let parsed = scriptw_front::parse(text, scriptw_front::ParseOptions::script()).unwrap();
        crate::lower(&parsed)
    }

    #[test]
    fn streamed_inference_matches_retained_bodies_for_effects_and_completion() {
        for source in [
            "function f(a = 1) { let b = a; b += 2; return b; } f(40);",
            "function f(a) { if (a) { return 1; } else { return 2; } } f(true);",
            "function f(a) { { let value = a; return value; } } f(42);",
            "let value = { x: 1 }; function f() { value.x = 2; return value; } f();",
            "function f(a) { try { return a.x; } finally { a.y = 1; } } f({x: 42});",
            "function f(a) { function g() { return a; } return g; } f(42)();",
            "function f(a) { for (let i = 0; i < 2; i++) { a += i; } return a; } f(41);",
            "function f(a) { return +a; } f({valueOf() { return 42; }});",
            "function f(a = eval('40 + 2')) { return a; } f();",
            "function f() { ; } f();",
        ] {
            STREAMED_INFERENCE_BODIES.set(0);
            let streamed = lower(source, false);
            assert!(STREAMED_INFERENCE_BODIES.get() > 0, "{source}");
            let retained = lower(source, true);
            assert_eq!(streamed, retained, "{source}");
        }
    }

    #[test]
    fn resumable_and_root_resource_bodies_retain_the_complete_transfer() {
        for source in [
            "function* f() { yield 1; return 2; } f();",
            "async function f() { await 1; return 2; } f();",
            "async function* f() { yield await 1; return 2; } f();",
            "function f() { using resource = null; return 42; } f();",
        ] {
            STREAMED_INFERENCE_BODIES.set(0);
            let streamed = lower(source, false);
            assert_eq!(STREAMED_INFERENCE_BODIES.get(), 0, "{source}");
            assert_eq!(streamed, lower(source, true), "{source}");
        }
    }

    #[test]
    fn streamed_root_prepass_keeps_global_writes_sources_and_abrupt_completion() {
        for source in [
            "let value = 1; if (value) { value = 42; } value;",
            "var value = {x: 1}; function f() { return value.x; } value.x = 42; f();",
            "let value = 0; try { throw 42; } catch (error) { value = error; } value;",
            "new ShadowRealm().evaluate('40 + 2'); true;",
            r#"new ShadowRealm().evaluate('\uD800'); true;"#,
            r#"new ShadowRealm().evaluate("'\uD800';"); true;"#,
            "const ShadowRealm = null; new ShadowRealm().evaluate('40 + 2');",
            "{ let value = 42; throw value; }",
            "{ using resource = null; } true;",
        ] {
            STREAMED_ROOT_PREPASSES.set(0);
            let streamed = lower(source, false);
            assert!(STREAMED_ROOT_PREPASSES.get() > 0, "{source}");
            assert_eq!(streamed, lower(source, true), "{source}");
        }
        STREAMED_ROOT_PREPASSES.set(0);
        let resource = "using resource = null; true;";
        assert!(scriptw_front::parse(resource, scriptw_front::ParseOptions::script()).is_err());
        assert_eq!(STREAMED_ROOT_PREPASSES.get(), 0);
    }
}
