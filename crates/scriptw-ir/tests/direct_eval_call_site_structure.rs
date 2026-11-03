use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{
    lower, EnvironmentIdentifierOperationIr, ExprIr, PreparedScriptKind, PreparedScriptOutcome,
    StatementIr,
};
const IR_SOURCE: &str = include_str!("../src/ir.rs");
const CONDITIONAL_FLOW_SOURCE: &str = include_str!("../src/lowering/conditional_flow.rs");
const CALL_CANDIDATE_SOURCE: &str = include_str!("../src/lowering/call_candidate_analysis.rs");
const INVOCATION_PROVENANCE_SOURCE: &str = include_str!("../src/lowering/define_property_call.rs");
const DISPATCH: &str = include_str!("../../scriptw-aot-wasm/src/functions/direct_eval.rs");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn normalized(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[test]
fn bare_eval_retains_a_runtime_reference_and_prepares_both_identity_branches() {
    let program = lower(&parse("eval('1 + 2');", ParseOptions::script()).unwrap());
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.unwrap();
    let StatementIr::Expression(expression) = &script.body.statements[0] else {
        panic!("call statement retained");
    };
    let ExprIr::EnvironmentIdentifier(identifier) = &expression.expr else {
        panic!("runtime Reference retained");
    };
    assert_eq!(identifier.name, "eval");
    let EnvironmentIdentifierOperationIr::Call {
        args,
        direct_eval: Some(context),
        ..
    } = &identifier.operation
    else {
        panic!("bare syntax carries context");
    };
    assert_eq!(args.len(), 1);
    assert_eq!(
        context.invocation(),
        scriptw_front::EvalInvocationContext::Script
    );
    assert!(script
        .prepared_scripts
        .iter()
        .any(|source| matches!(source.kind, PreparedScriptKind::DirectEval(_))));
    assert!(script
        .prepared_scripts
        .iter()
        .any(|source| source.kind == PreparedScriptKind::IndirectEval));
}

#[test]
fn caller_parse_errors_are_deferred_and_do_not_reject_the_outer_script() {
    let program = lower(&parse("eval('new.target');", ParseOptions::script()).unwrap());
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.unwrap();
    let direct = script
        .prepared_scripts
        .iter()
        .find(|source| matches!(source.kind, PreparedScriptKind::DirectEval(_)))
        .unwrap();
    assert!(matches!(
        direct.outcome,
        PreparedScriptOutcome::DeferredSyntaxError { .. }
    ));
}

#[test]
fn dispatch_checks_the_original_realm_intrinsic_after_argument_evaluation() {
    let capture = bounded(
        DISPATCH,
        "pub(super) fn emit_direct_eval_call(",
        "pub(crate) fn emit_direct_eval_or_call_with_argv(",
    );
    assert!(
        capture
            .find("compile_expr_to_value(callee_expression")
            .unwrap()
            < capture.find("emit_call_args_vector(arguments").unwrap()
    );
    assert!(
        capture.find("emit_call_args_vector(arguments").unwrap()
            < capture.find("emit_direct_eval_or_call_with_argv(").unwrap()
    );
    let dispatch = bounded(
        DISPATCH,
        "pub(crate) fn emit_direct_eval_or_call_with_argv(",
        "    fn emit_direct_eval_argument(",
    );
    assert!(dispatch.contains("emit_load_realm_eval_intrinsic_to_local"));
    assert!(dispatch.contains("callee.reference().load(function)"));
    assert!(dispatch.contains("intrinsic.reference().load(function)"));
    assert!(dispatch.contains("Instruction::RefEq"));
    assert!(dispatch.contains("emit_function_or_proxy_call_with_argv("));
    assert!(dispatch.contains("self.completion().copy_from(&pending, function)"));
}

#[test]
fn exact_function_target_authority_is_independent_from_heap_shape() {
    let target_domain = normalized(bounded(
        IR_SOURCE,
        "pub enum FunctionTargetKnowledge {",
        "impl FunctionTargetKnowledge {",
    ));
    assert_eq!(
        target_domain,
        "Exact(BTreeSet<FunctionId>),Open(BTreeSet<FunctionId>),}"
    );
    let normalized_ir = normalized(IR_SOURCE);
    for compatibility_escape_hatch in [
        "implDerefforFunctionTargetKnowledge",
        "implIntoIteratorforFunctionTargetKnowledge",
        "implAsRefforFunctionTargetKnowledge",
    ] {
        assert!(!normalized_ir.contains(compatibility_escape_hatch));
    }

    let merge = normalized(bounded(
        CONDITIONAL_FLOW_SOURCE,
        "    pub(super) fn merge_value_infos(",
        "\n}\n",
    ));
    let target_join = merge
        .find("left.function_targets.join(right.function_targets)")
        .expect("value joins must merge target knowledge");
    let shape_join = merge
        .find("self.merge_heap_shapes")
        .expect("value joins must merge heap shapes");
    assert!(target_join < shape_join);

    let provenance = normalized(bounded(
        INVOCATION_PROVENANCE_SOURCE,
        "    fn classify(",
        "impl<'a> From<&'a ValueInfo> for InvocationTargetProvenance<'a> {",
    ));
    assert!(provenance.contains("function_targets.exact_targets()"));
    assert!(!provenance.contains("heap_shape"));

    let call_candidate_preflight = normalized(bounded(
        CALL_CANDIDATE_SOURCE,
        "    fn preflight_dynamic_source_call_candidates(",
        "    pub(super) fn preflight_function_prototype_call_dynamic_source(",
    ));
    assert!(call_candidate_preflight.contains("function_targets.known_targets()"));

    let call_candidate_analysis = normalized(bounded(
        CALL_CANDIDATE_SOURCE,
        "    pub(super) fn analyze_known_call_candidates(",
        "    pub(super) fn analyze_known_construct_candidates(",
    ));
    assert!(call_candidate_analysis.contains(
        "ifcallee.function_targets.exact_targets().is_none()||has_unaccounted_candidate{"
    ));
}

#[test]
fn construct_candidates_use_only_the_evaluated_callees_common_prototype() {
    let construct_analysis = normalized(bounded(
        CALL_CANDIDATE_SOURCE,
        "    pub(super) fn analyze_known_construct_candidates(",
        "    fn merge_call_candidate_result(",
    ));
    let prototype_read = construct_analysis
        .find("callee.heap_shape.as_deref().and_then(|shape|self.read_current_heap_shape_property(shape,\"prototype\"))")
        .expect("construct candidates must validate the evaluated callee's live prototype");
    assert!(
        !construct_analysis.contains("read_heap_shape_property("),
        "a stale prototype snapshot cannot replace the live property owner"
    );
    let prototype_install = construct_analysis
        .find("Self::with_instance_prototype(self.function_construct_instance_info(&signature),common_instance_prototype.clone(),)")
        .expect("construct candidates must replace definition-time prototype facts");
    let this_observation = construct_analysis
        .find("self.merge_function_this_info(function_id,constructed_this.clone())")
        .expect("the refreshed constructed this value must feed source-function analysis");
    assert!(prototype_read < prototype_install);
    assert!(prototype_install < this_observation);
}

#[test]
fn function_caller_new_target_reaches_an_executable_script_unit() {
    let program = lower(
        &parse(
            "function caller() { return eval('new.target'); } caller();",
            ParseOptions::script(),
        )
        .unwrap(),
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.unwrap();
    assert!(script.prepared_scripts.iter().any(|source| {
        matches!(&source.kind, PreparedScriptKind::DirectEval(context)
            if context.invocation() == scriptw_front::EvalInvocationContext::Function)
            && matches!(source.outcome, PreparedScriptOutcome::Executable(_))
    }));
}
