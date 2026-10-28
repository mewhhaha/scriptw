#!/usr/bin/env python3
"""ABI8 native direct-eval host boundary source controls. No compiler execution."""
from __future__ import annotations
import hashlib
import json
from pathlib import Path
import re
import sys

ENGINE = "crates/scriptw-engine/"
HOST = ENGINE + "src/wasm_native_direct_eval_compiler"
AOT = "crates/scriptw-aot-wasm/src/"
TEMPLATE_PLAN = AOT + "native_program_template/plan.rs"
IR = "crates/scriptw-ir/src/"
# Exact registered exclusions and historical selector baselines are data, never
# directory-name patterns that silently remove new production compiler inputs.
HISTORICAL = {'crates/scriptw-engine/src/tests.rs': {'count': 707, 'sha256': '40ea9988aad82012f98b3e75882992ac7d397a8e91a468abacf476ad2db1fdf7', 'allowed_new': ['closed_module_cache_hit_skips_lowering_and_rechecks_dependency_bytes', 'initial_module_cache_candidate_does_not_name_a_prepared_source_closure']}, 'crates/scriptw-engine/src/native_global_script_compiler_tests/declarations.rs': {'count': 3, 'sha256': '8ac78b25340eec881eda091cade607e3487c7af1c4df5a9133e56dda8d1ea1ac', 'allowed_new': []}, 'crates/scriptw-engine/src/native_global_script_compiler_tests/ownership.rs': {'count': 1, 'sha256': '7e25995c2649f48f77f675aa4b87b7165f1c3ece86cf1a9a74e428cbe529bc40', 'allowed_new': []}, 'crates/scriptw-engine/src/native_global_script_compiler_tests/realm.rs': {'count': 2, 'sha256': '377e3bec89cda3d3ce062568d48793718efdab229965f7bf3c24519d29c3490a', 'allowed_new': []}, 'crates/scriptw-engine/src/wasm_gc_completion.rs': {'count': 6, 'sha256': 'fb742bbb18e75d7f4416d3974e30662fdf9955d15147284e1168b81b6d1ca9f9', 'allowed_new': []}, 'crates/scriptw-engine/tests/structure/compiler_fingerprint.rs': {'count': 12, 'sha256': '5dc6147aa5ba69006a6b3ae53a55dd03afb6a02669ed04445ecbcf06d3afd7df', 'allowed_new': ['reused_build_script_must_match_its_actual_fingerprint_implementation']}}
TEST_ONLY_ALLOWLIST = ['crates/scriptw-front/tests', 'crates/scriptw-ir/tests', 'crates/scriptw-ir/src/tests', 'crates/scriptw-aot-wasm/tests', 'crates/scriptw-aot-wasm/src/runtime_artifact/build_package/tests.rs', 'crates/scriptw-aot-wasm/src/runtime_artifact/cache/tests.rs', 'crates/scriptw-aot-wasm/src/runtime_artifact/validated_profile/tests.rs', 'crates/scriptw-runtime/tests', 'crates/scriptw-intl/tests', 'crates/scriptw-engine/src/embedded_runtime/compile_tests.rs', 'crates/scriptw-engine/src/embedded_runtime/tests.rs', 'crates/scriptw-engine/src/wasm_runtime_link/tests.rs', 'crates/scriptw-engine/src/wasm_compilation_policy/tests.rs', 'crates/scriptw-engine/src/tests.rs', 'crates/scriptw-engine/src/regexp_program_boundary_tests.rs', 'crates/scriptw-engine/src/runtime_regexp_compiler_tests.rs', 'crates/scriptw-engine/src/array_present_index_tests.rs', 'crates/scriptw-engine/src/module_async_runtime_tests.rs', 'crates/scriptw-engine/src/wasm_native_source_compiler/tests.rs', 'crates/scriptw-engine/src/native_function_compiler_tests.rs', 'crates/scriptw-engine/src/native_function_compiler_tests', 'crates/scriptw-engine/src/wasm_native_function_compiler/observer.rs', 'crates/scriptw-engine/src/wasm_native_function_compiler/tests.rs', 'crates/scriptw-engine/src/wasm_native_function_compiler/tests', 'crates/scriptw-engine/src/native_global_script_compiler_tests.rs', 'crates/scriptw-engine/src/native_global_script_compiler_tests', 'crates/scriptw-engine/src/wasm_native_global_script_compiler/observer.rs', 'crates/scriptw-engine/src/wasm_native_global_script_compiler/tests.rs', 'crates/scriptw-engine/src/wasm_native_global_script_compiler/tests', 'crates/scriptw-engine/src/native_direct_eval_compiler_tests.rs', 'crates/scriptw-engine/src/native_direct_eval_compiler_tests', 'crates/scriptw-engine/src/wasm_native_direct_eval_compiler/observer.rs', 'crates/scriptw-engine/src/wasm_native_direct_eval_compiler/tests.rs', 'crates/scriptw-engine/src/wasm_native_direct_eval_compiler/tests', 'crates/scriptw-engine/src/native_language_feature_policy_tests.rs']
TEST_ONLY_ALLOWLIST = ['crates/scriptw-front/src/tests', 'crates/scriptw-front/src/json/tests.rs', 'crates/scriptw-ir/src/ir/tests.rs', 'crates/scriptw-ir/src/analysis/private_environment_tests.rs', 'crates/scriptw-ir/src/runtime_global_script/tests.rs', 'crates/scriptw-aot-wasm/src/tests.rs', 'crates/scriptw-aot-wasm/src/tests', 'crates/scriptw-aot-wasm/src/planning/tests.rs', 'crates/scriptw-aot-wasm/src/control_flow/tests.rs', 'crates/scriptw-runtime/src/rooted_snapshot/tests.rs', 'crates/scriptw-engine/src/tests'] + TEST_ONLY_ALLOWLIST
ENGINE_TEST_CHILDREN = tuple(ENGINE + "src/tests/" + name + ".rs" for name in (
    "arrays", "cache", "control_flow", "execution", "functions", "intl", "modules", "objects", "strings"))
MOVED_TEST_GATES = {
    "crates/scriptw-front/src/lib.rs": "#[cfg(test)]modtests{",
    "crates/scriptw-front/src/json.rs": "#[cfg(test)]modtests;",
    IR + "ir.rs": "#[cfg(test)]modtests;",
    IR + "analysis.rs": "#[cfg(test)]modprivate_environment_tests;",
    IR + "runtime_global_script.rs": "#[cfg(test)]modtests;",
    AOT + "lib.rs": "#[cfg(test)]modtests;",
    AOT + "planning.rs": "#[cfg(test)]modtests;",
    AOT + "control_flow.rs": "#[cfg(test)]modtests;",
    "crates/scriptw-runtime/src/rooted_snapshot.rs": "#[cfg(test)]modtests;",
}
NATIVE_TEST_GATE = '#[cfg(all(test,feature="native-source-compiler"))]'
# Native controls need both test isolation and the source-compiler capability.
# Observer fields and their initializers must carry the same complete gate.
NATIVE_TEST_GATES = {
    "modnative_direct_eval_compiler_tests;": 1,
    "modnative_function_compiler_tests;": 1,
    "modnative_global_script_compiler_tests;": 1,
    "modnative_language_feature_policy_tests;": 1,
    "native_direct_eval_compilation_observer:": 2,
    "native_function_compilation_observer:": 2,
    "native_global_script_compilation_observer:": 2,
}

AUTHORED_CONTROLS = [['crates/scriptw-engine/src/native_direct_eval_compiler_tests/caller_records.rs', 'native_direct_eval_keeps_caller_lexical_writes_var_admission_and_strictness'], ['crates/scriptw-engine/src/native_direct_eval_compiler_tests/caller_records.rs', 'native_direct_eval_uses_actual_this_arguments_and_new_target_records'], ['crates/scriptw-engine/src/native_direct_eval_compiler_tests/caller_records.rs', 'native_unicode_private_names_keep_hidden_ancestor_scope_and_escaping_gc_roots'], ['crates/scriptw-engine/src/native_direct_eval_compiler_tests/caller_records.rs', 'native_super_arrow_initializes_the_defining_constructor_after_gc'], ['crates/scriptw-engine/src/native_direct_eval_compiler_tests/ownership.rs', 'native_nested_eval_and_old_program_reentry_keep_separate_defining_owners'], ['crates/scriptw-engine/src/native_direct_eval_compiler_tests/ownership.rs', 'direct_eval_native_compilation_and_execution_keep_the_original_parent_timeout'], ['crates/scriptw-engine/src/native_direct_eval_compiler_tests/source_and_errors.rs', 'native_direct_syntax_uses_real_caller_grammar_and_intrinsic_error_identity'], ['crates/scriptw-engine/src/native_direct_eval_compiler_tests/source_and_errors.rs', 'native_direct_source_keeps_exact_lone_utf16_units_and_non_string_identity'], ['crates/scriptw-engine/src/native_direct_eval_compiler_tests/source_and_errors.rs', 'unused_external_import_bodies_do_not_reject_native_direct_eval'], ['crates/scriptw-engine/src/native_direct_eval_compiler_tests/source_and_errors.rs', 'external_import_operand_throws_precede_its_runtime_capability_boundary'], ['crates/scriptw-engine/src/native_direct_eval_compiler_tests/source_and_errors.rs', 'executed_external_import_keeps_its_distinct_uncatchable_capability_error'], ['crates/scriptw-engine/src/wasm_native_direct_eval_compiler/tests.rs', 'only_malformed_javascript_parse_rejections_become_direct_eval_syntax_messages'], ['crates/scriptw-engine/src/wasm_native_direct_eval_compiler/tests.rs', 'factory_request_identity_includes_hidden_private_floor_and_exact_utf16_source'], ['crates/scriptw-engine/src/wasm_native_direct_eval_compiler/tests.rs', 'actual_syntax_and_thrown_reference_roots_survive_temporary_scope_exit_and_gc']]
EMPTY_BLOCK_CONTROLS = [['crates/scriptw-engine/src/native_direct_eval_compiler_tests/empty_blocks.rs', 'native_direct_empty_blocks_preserve_update_empty_and_nested_values'], ['crates/scriptw-engine/src/native_direct_eval_compiler_tests/empty_blocks.rs', 'native_direct_empty_loop_bodies_keep_condition_and_update_effects'], ['crates/scriptw-engine/src/native_direct_eval_compiler_tests/empty_blocks.rs', 'native_direct_empty_blocks_keep_lexical_shadowing_and_early_errors'], ['crates/scriptw-engine/src/native_direct_eval_compiler_tests/empty_blocks.rs', 'native_direct_empty_try_clauses_keep_update_empty_and_catch_ownership'], ['crates/scriptw-engine/src/native_direct_eval_compiler_tests/empty_blocks.rs', 'native_direct_empty_statement_lists_keep_declarations_and_resource_lifetimes']]
EMPTY_COMMENT_CONTROLS = [[ENGINE + "src/native_direct_eval_compiler_tests/empty_blocks.rs", "native_direct_empty_comments_keep_real_parsing_and_nonempty_completion"]]
ALL_AUTHORED_CONTROLS = AUTHORED_CONTROLS + EMPTY_BLOCK_CONTROLS + EMPTY_COMMENT_CONTROLS

CAPS = {
    ENGINE + "src/tests.rs": 31719,
    HOST + ".rs": 160, HOST + "/factory.rs": 180,
    HOST + "/signature.rs": 200, HOST + "/schema.rs": 140,
    HOST + "/sources.rs": 140, HOST + "/sources/private_chain.rs": 90,
    HOST + "/sources/invocation.rs": 150, HOST + "/outcome.rs": 180,
    HOST + "/observer.rs": 80, HOST + "/tests.rs": 150,
    HOST + "/tests/fixture.rs": 80,
    ENGINE + "src/native_direct_eval_compiler_tests.rs": 120,
    ENGINE + "src/native_direct_eval_compiler_tests/caller_records.rs": 100,
    ENGINE + "src/native_direct_eval_compiler_tests/ownership.rs": 80,
    ENGINE + "src/native_direct_eval_compiler_tests/source_and_errors.rs": 110,
    ENGINE + "src/native_direct_eval_compiler_tests/empty_blocks.rs": 100,
    ENGINE + "src/native_direct_eval_compiler_tests/empty_cases.rs": 100,
}
FILES = tuple(sorted(set(CAPS) | set(HISTORICAL) | set(ENGINE_TEST_CHILDREN) | set(MOVED_TEST_GATES) | {
    ENGINE + "compiler_fingerprint.rs", ENGINE + "src/wasm_native_source_compiler.rs",
    ENGINE + "src/lib.rs", ENGINE + "src/wasm_runtime_link.rs",
    ENGINE + "src/wasm_compilation_policy.rs",
    AOT + "runtime_artifact/build_package.rs", AOT + "runtime_artifact/cache.rs",
    ENGINE + "src/wasm_gc_host.rs", AOT + "gc_types/host.rs",
    AOT + "gc_types/layouts.rs", AOT + "module/static_signature.rs",
    AOT + "runtime_helpers.rs", IR + "runtime_direct_eval/caller.rs",
    TEMPLATE_PLAN,
}))


def compact(text: str) -> str:
    return re.sub(r"\s+", "", re.sub(r"//[^\n]*", "", text))


def check(sources: dict[str, str]) -> list[dict[str, str]]:
    failures = []
    def require(ok: bool, key: str, reason: str) -> None:
        if not ok:
            failures.append({"invariant": key, "reason": reason})
    c = {name: compact(text) for name, text in sources.items()}
    signature = c[HOST + "/signature.rs"]
    request_shape = "[source,descriptor,realm,lexical,variable,private,ValType::I32,ValType::I64,this_ref,ValType::I32,ValType::I64,target_ref,invocation,program]"
    result_shape = "[ValType::I32,ValType::I64,value,ValType::I32,ValType::I32,ValType::I32,message]"
    require(request_shape in signature and result_shape in signature,
            "request14", "exact fourteen operands and Completion5/status/syntax results")
    require("forthis" not in signature and "forvaluein[this_ref,target_ref,value]{schema::eq_reference(value)?;}" in signature and
            "schema::peer(&schema::structure(variable,true,Layout::Environment)?,&environment,)" in signature,
            "canonical_request", "Value3 eqrefs and lexical/variable canonical peer shapes")
    aot_sig = c[AOT + "module/static_signature.rs"]
    require("HostCompileDirectEval=>([Gc(GcLayout::StringValue,NonNullable),Gc(GcLayout::ImmutableByteArray,NonNullable),Gc(GcLayout::RealmRecord,NonNullable),Gc(GcLayout::Environment,Nullable),Gc(GcLayout::Environment,Nullable),Gc(GcLayout::PrivateEnvironment,Nullable),I32,I64,EqRef,I32,I64,EqRef,Gc(GcLayout::DirectEvalExecutionContext,NonNullable),Gc(GcLayout::ProgramInstance,NonNullable)],[I32,I64,EqRef,I32,I32,I32,Gc(GcLayout::StringValue,Nullable)])" in aot_sig,
            "aot_request14", "host request declaration must match actual native operand shape")
    row = c[AOT + "runtime_helpers.rs"]
    require('NativeDirectEvalFactory/NativeDirectEvalFactoryArguments/NativeDirectEvalFactoryParameters/"native_direct_eval_factory"{caller_program:Program,source:(RefStringValueNonNullable),descriptor:(RefImmutableByteArrayNonNullable),realm:(RefRealmRecordNonNullable),lexical:(RefEnvironmentNullable),variable:(RefEnvironmentNullable),private:(RefPrivateEnvironmentNullable),this_value:Value,new_target:Value,invocation:(RefDirectEvalExecutionContextNonNullable),calling_program:Program}=>StatusCompletion;' in row,
            "aot_factory15", "factory prefixes exactly P2! before the unmodified request14")
    factory = c[HOST + "/factory.rs"]
    require("arguments.len()!=signature.arguments.len()+1" in factory and
            "!program_reference(&arguments[0],signature)" in factory and
            "arguments[1..].iter().zip(&signature.arguments).all(|(a,e)|ValType::eq(a,e))" in factory and
            "results.len()!=6" in factory and "results[..5].iter().zip(&signature.completion).all(|(a,e)|ValType::eq(a,e))" in factory and
            "matches!(results[5],ValType::I32)" in factory,
            "factory15", "exact canonical P2/request/Completion5/status factory signature")
    schema = c[HOST + "/schema.rs"]
    require("reference.is_nullable()!=nullable" in schema and
            "layout.field_count()!=Some(ty.fields().len()asu32)" in schema and
            "layout.array_mutable()!=Some(ty.mutability()==Mutability::Var)" in schema and
            "(actual.mutability()==Mutability::Var)!=definition.mutable()" in schema and
            "storage_matches(definition.storage(),actual.element_type())" in schema and
            "!StructType::eq(actual,expected)" in schema and
            all("Field::" + name in signature for name in ("DirectEvalClassContext", "ContextDefiningProgram", "ContextActiveFunction", "FunctionObjectContext", "DirectEvalHomeObject", "DirectEvalDerivedBindings", "DerivedThisCell", "DerivedThisStatusCell", "DerivedNewTargetCell", "DerivedActiveFunctionCell", "BindingCellValue")),
            "canonical_fields", "declaration-owned field counts, nullability, storage, mutability and canonical peer edges remain checked")
    decoded = c[HOST + "/sources.rs"]
    require("operands:[Val;14]" in decoded and "_roots:Vec<OwnedRooted<AnyRef>>" in decoded and
            "forvalueininputs{ifletVal::AnyRef(Some(reference))=value{roots.push(reference.to_owned_rooted(&mut*store)?);}}" in decoded and
            "_roots:roots" in decoded and "operands:inputs.try_into()" in decoded,
            "owned_inputs", "every reference input remains OwnedRooted alongside the original operand array")
    require(decoded.find("forvalueininputs{") < decoded.find("letsource=required_structure("),
            "root_order", "root caller operands before decoding or allocation")
    require("wasm_gc_completion::string_units(store,source.to_eqref())" in decoded and
            "RuntimeDirectEvalCallerDescriptor::decode(&bytes).map_err(abi_error)?" in decoded,
            "exact_source", "actual UTF16 and immutable caller descriptor decode without text conversion")
    require("require_exact_program(&request.caller,&request.source,program)?;" in factory and
            "program.caller()!=caller" in factory and "root.kind!=kind" in factory and
            "root.source!=scriptw_front::encode_js_string_utf16(source)" in factory and
            "unit.id==program.script_id()&&unit.kind==kind" in factory and
            "artifact.caller()!=&request.caller||artifact.source()!=request.source.as_slice()" in factory,
            "full_witness", "full caller, exact UTF16, kind and actual executable root witness")
    caller = c[IR + "runtime_direct_eval/caller.rs"]
    require("#[derive(Debug,Clone,PartialEq,Eq)]pubstructRuntimeDirectEvalCaller{" in caller and
            "private_environment_floor:u32" in caller and "private_environment_floor," in caller and
            "implPartialEqforRuntimeDirectEvalCaller" not in caller,
            "hidden_floor_equality", "derived complete caller equality includes the hidden private allocation floor")
    require("!Arc::ptr_eq(artifact.runtime(),context.runtime())" in factory,
            "runtime_identity", "new P must retain the exact admitted immutable R Arc")
    chain = c[HOST + "/sources/private_chain.rs"]
    require("whileletSome(environment)=current{execution.remaining().map_err(wasmtime::Error::new)?;" in chain and
            "!seen.insert(scope)" in chain and "u32::try_from(scope)" in chain and
            "!ArrayType::eq(&names.ty(&*store)?,&signature.private_names)" in chain and
            "RuntimeDirectEvalPrivateScope::new(scope,names.len(&*store)?,)" in chain and
            "current=nullable_structure(store,&parent,&signature.private_environment)?;" in chain,
            "actual_private_chain", "visit every original-deadline checked row; reject cycles and validate actual table lengths")
    require(".checked_add(1).ok_or(RuntimeDirectEvalCallerError(" in caller and
            "private_environment_floor=private_environment_floor.max(next);" in caller and
            "id.name_ordinal()<*slots" in caller and "tables.insert(scope.class_scope,scope.slot_count).is_some()" in caller and
            "private_chain::scan(store,private,signature,execution)?" in decoded and
            "RuntimeDirectEvalCaller::validate(descriptor,scopes).map_err(abi_error)?" in decoded,
            "private_capacity", "all visible/hidden scopes contribute checked max+1 floor and real ordinal capacity")
    invocation = c[HOST + "/sources/invocation.rs"]
    require("(Field::DirectEvalThisSnapshot,&operands[6..9])" in invocation and
            "(Field::DirectEvalNewTargetSnapshot,&operands[9..12])" in invocation and
            "Rooted::ref_eq(store,c,z)?" in invocation and "a!=x||b!=y||!same_reference" in invocation and
            all("Field::" + name in invocation for name in ("DerivedThisCell", "DerivedThisStatusCell", "DerivedNewTargetCell", "DerivedActiveFunctionCell", "ContextDefiningProgram")),
            "invocation_identity", "actual snapshots and all four retained derived cell records remain validated")
    context = c[ENGINE + "src/wasm_native_source_compiler.rs"]
    engine = c[ENGINE + "src/lib.rs"]
    require("execution:Arc<WasmAgentExecutionControl>" in context and
            "&self.execution" in context and
            "policy.clone(),Arc::clone(&execution_control)," in engine and "pub(super)policy:WasmCompilationPolicy" in context and
            "store.data_mut().native_source_compiler=Some(Arc::new(context));" in engine,
            "original_execution", "native callbacks share the original execution-control Arc")
    production = "".join(c[name] for name in FILES if name.startswith(HOST) and not any(part in name for part in ("/tests", "/observer.rs"))) + context
    require(not any(token in production for token in ("Engine::new(", "RealmBuilder", "Realm::new(", "Realm::default(", "Store::new(", "WasmAgentExecutionControl::new(", "Instant::now(", ".arm(", "set_epoch_deadline(")),
            "no_replacement_execution", "no new Engine, Store, Realm or timeout budget in production host path")
    host = c[HOST + ".rs"]
    require("WasmtimeEngine::same(caller.engine(),context.linker.engine())" in host and
            "ConvertedRequest::decode(&mutroots,inputs,signature,context.execution())?" in host and
            host.count("context.check_budget()?") >= 5 and factory.count("context.check_budget()?") >= 5 and
            "crate::wasm_native_program_template::instantiate(&mut*store,engine,context,&template,trace,)?" in factory and
            "NativeProgramTemplate::for_direct_eval(&artifact)" in factory and
            "initialize.call(&mut*store,&initialization,&mutowner)?" in factory and
            "crate::wasm_native_program_template::initializer_arguments(" in factory and
            "arguments.push(owner[0]);arguments.extend(request.operands);" in factory and
            "factory.call(&mut*store,&arguments,&mutoutput)?" in factory,
            "same_store_factory", "compile/instantiate/initialize/factory preserve original Store and deadline")
    template_plan = c[TEMPLATE_PLAN]
    require("Payload::StartSection{..}" in template_plan and "parse_all(artifact.bytes())" not in factory and
            "results.as_slice(),[owner]ifprogram_reference(owner,signature)" in factory,
            "bootstrap_only", "raw P keeps []->P while the checked native template receives its fresh payload; no module start function")
    outcome = c[HOST + "/outcome.rs"]
    require("reference:Option<OwnedRooted<AnyRef>>" in outcome and "Syntax(OwnedRooted<AnyRef>)" in outcome and
            ".map(|r|r.to_owned_rooted(&mut*store)).transpose()?" in outcome and
            "string.to_anyref().to_owned_rooted(&mut*store)?" in outcome and
            "message.to_rooted(&mut*store)" in outcome and
            "check_gc_main_completion(kind,target).map_err(abi_error)?" in outcome,
            "escaped_roots", "only validated Completion and owned syntax/result roots escape RootScope")
    require("RuntimeDirectEvalCompileError::Parse(error)=>matcherror.diagnostic().kind(){ParseDiagnosticKind::MalformedJavaScript=>error.to_string()," in outcome and
            "ParseDiagnosticKind::UnsupportedParserFeature=>{returnErr(" in outcome and
            "RuntimeDirectEvalCompileError::Lowering(diagnostics)=>{returnErr(" in outcome and
            "RuntimeDirectEvalCompileError::InvalidCompilerOutput(message)=>{returnErr(abi_error(message));}" in outcome,
            "parser_only_syntax", "only genuine malformed parser rejection mints a SyntaxError message")
    projection = c[AOT + "gc_types/host.rs"]
    storage = c[ENGINE + "src/wasm_gc_completion.rs"]
    require("function_field(&FunctionContextSchema::INSTANCE_ELEMENTS_INITIALIZER)" in projection and
            "field:&GcField<O,GcFunctionRef<R>,M,N>" in projection and
            "storage:GcHostStorage::FunctionReference{nullable:N::NULLABLE,}" in projection and
            "GcHostStorage::FunctionReference{nullable},StorageType::ValType(ValType::Ref(reference))," in storage and
            "reference.is_nullable()==nullable&&matches!(reference.heap_type(),wasmtime::HeapType::ConcreteFunc(_))" in storage and
            "schema::field(&function_context,Field::ContextInstanceElementsInitializer)?;" in signature and
            "INSTANCE_ELEMENTS_INITIALIZER:GcFunctionRef<InitializeInstanceElementsHook>,Immutable,Nullable;" in c[AOT + "gc_types/layouts.rs"],
            "typed_initializer_storage", "constructor hook remains an immutable nullable typed function reference")
    fingerprint = sources[ENGINE + "compiler_fingerprint.rs"]
    found = re.search(r"const TEST_ONLY_INPUTS:.*?= &\[(.*?)\];", fingerprint, re.S)
    paths = re.findall(r'"([^"\n]+)"', found.group(1)) if found else []
    require(paths == TEST_ONLY_ALLOWLIST, "fingerprint_allowlist", "only exact registered cfg(test) paths may leave product fingerprint")
    require(all("#[cfg(test)]modtests;" in c[name] for name in (
                ENGINE + "src/lib.rs", ENGINE + "src/wasm_runtime_link.rs",
                ENGINE + "src/wasm_compilation_policy.rs",
                AOT + "runtime_artifact/build_package.rs", AOT + "runtime_artifact/cache.rs")),
            "pure_test_owner_gates", "every newly excluded exact test file retains its statically cfg(test) parent")
    require(all(gate in c[name] for name, gate in MOVED_TEST_GATES.items()),
            "moved_test_gates", "every newly excluded owner remains behind cfg(test)")
    require(all(c[ENGINE + "src/tests.rs"].count(f'#[path="tests/{Path(name).name}"]mod{Path(name).stem};') == 1
                for name in ENGINE_TEST_CHILDREN),
            "moved_test_gates", "all nine engine children have one exact cfg(test) parent declaration")
    require(all(engine.count(NATIVE_TEST_GATE + owner) == count
                for owner, count in NATIVE_TEST_GATES.items()) and
            "#[cfg(test)]modobserver;" in host and "#[cfg(test)]modtests;" in host and
            "#[cfg(test)]pub(super)useobserver::{" in host and
            "#[cfg(test)]letcontext=context.with_test_observers(" in engine and
            "#[cfg(test)]pub(super)direct_eval_test_observer:" in context and
            "#[cfg(test)]pub(super)fnwith_test_observers(" in context and
            '"crates/scriptw-engine/src"' in fingerprint,
            "static_test_gates", "native controls and observer fields require test plus source capability; child observers remain test-only and production stays covered")
    helper = c[ENGINE + "src/native_direct_eval_compiler_tests.rs"]
    require("!script.prepared_scripts.iter().any(" in helper and
            "decode_js_string_utf16(&row.source)==*source" in helper and
            "assert_eq!(outcome.backend_used,ExecutionBackend::WasmAot)" in helper and
            "observed.requests,observed.completed_factories,observed.syntax_rejections" in helper,
            "actual_native_controls", "controls prove finite-guard miss and count actual native phases")
    actual_controls = []
    for name in sorted(set(path for path, _ in ALL_AUTHORED_CONTROLS)):
        for test in re.findall(r"#\[test\]\s*fn\s+(\w+)\s*\(", sources[name]):
            actual_controls.append([name, test])
    require(sorted(actual_controls) == sorted(ALL_AUTHORED_CONTROLS) and len(actual_controls) == 20 and
            len(AUTHORED_CONTROLS) == 14 and len(EMPTY_BLOCK_CONTROLS) == 5 and len(EMPTY_COMMENT_CONTROLS) == 1 and
            not any("#[ignore" in c[path] for path, _ in ALL_AUTHORED_CONTROLS),
            "nineteen_controls", "retain all original fourteen selectors, five empty-block controls and the new empty-comment control")
    protocol = c[HOST + "/tests.rs"]
    records = c[ENGINE + "src/native_direct_eval_compiler_tests/caller_records.rs"]
    ownership = c[ENGINE + "src/native_direct_eval_compiler_tests/ownership.rs"]
    errors = c[ENGINE + "src/native_direct_eval_compiler_tests/source_and_errors.rs"]
    require("assert_ne!(caller,other);" in protocol and
            "require_exact_program(&other,&source,&program).is_err()" in protocol and
            "require_exact_program(&caller,&[0xd800],&program).is_err()" in protocol and
            protocol.count("store.gc(None).unwrap();") >= 2 and "Rooted::ref_eq(&store,&message,&returned).unwrap()" in protocol and
            "observed.private_environment_floor>observed.visible_private_floor" in records and
            "Some(WasmExecutionFailureKind::Timeout)" in ownership and
            "vec![RuntimeUnavailableCapability::ExternalCallerImport]" in errors,
            "authored_control_assertions", "identity, hidden-floor, GC, original-timeout and capability controls retain their assertions")
    for name, baseline in HISTORICAL.items():
        owners = (name, *ENGINE_TEST_CHILDREN) if name == ENGINE + "src/tests.rs" else (name,)
        selectors = re.findall(r"#\[test\]\s*fn\s+(\w+)\s*\(", "\n".join(sources[p] for p in owners))
        tests = set(selectors)
        require(len(selectors) == len(tests), "historical_selectors", "duplicate historical selector: " + name)
        tests.difference_update(baseline["allowed_new"])
        digest = hashlib.sha256("\n".join(sorted(tests)).encode()).hexdigest()
        require(len(tests) == baseline["count"] and digest == baseline["sha256"],
                "historical_selectors", "historical selector ownership changed: " + name)
    for name, maximum in CAPS.items():
        require(len(sources[name].splitlines()) <= maximum, "owner_cap", name + " exceeds its registered cap")
    require("modwasm_native_direct_eval_compiler;" in engine and
            "wasm_native_direct_eval_compiler::link(linker,module)?;" in c[ENGINE + "src/wasm_gc_host.rs"] and
            all("mod" + name + ";" in host for name in ("factory", "outcome", "schema", "signature", "sources")) and
            "modempty_blocks;" in helper,
            "module_ownership", "one private host compiler owner is linked and its children remain attached")
    return failures


def main() -> int:
    root = Path(sys.argv[1]) if len(sys.argv) == 2 else Path.cwd()
    failures = check({name: (root / name).read_text() for name in FILES})
    for row in failures:
        print("ABI8 native direct eval host: " + row["invariant"] + ": " + row["reason"], file=sys.stderr)
    if not failures:
        print("ABI8 native direct-eval host source controls passed")
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
