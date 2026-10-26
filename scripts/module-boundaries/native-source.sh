# ABI6 native Function adapter and typed P bootstrap ownership.
for native_owner in runtime_function.rs runtime_function/runtime_profile.rs runtime_function/tests.rs runtime_function/runtime_profile/tests.rs emit/body_compilation/program_initialize.rs gc_types/value/program_initialize.rs builtins/function/dynamic_constructor/native_compiler.rs emitted_function/program_initialize_tests.rs; do
  require_file "crates/scriptw-aot-wasm/src/$native_owner"
  check_no_inline_legacy_includes "crates/scriptw-aot-wasm/src/$native_owner"
done
require_module_decl crates/scriptw-aot-wasm/src/lib.rs runtime_function
require_module_decl crates/scriptw-aot-wasm/src/gc_types/value.rs program_initialize
require_module_decl "$wasm_body_compilation" program_initialize
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_function.rs 180
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_function/runtime_profile.rs 120
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_function/tests.rs 260
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_function/runtime_profile/tests.rs 120
check_raw_line_budget crates/scriptw-aot-wasm/src/emit/body_compilation/program_initialize.rs 180
check_raw_line_budget crates/scriptw-aot-wasm/src/gc_types/value/program_initialize.rs 80
check_raw_line_budget crates/scriptw-aot-wasm/src/builtins/function/dynamic_constructor/native_compiler.rs 180
check_raw_line_budget crates/scriptw-aot-wasm/src/emitted_function/program_initialize_tests.rs 260
require_fixed_string_count "$wasm_body_compilation" 'self.bind_entry_program(program, &mut function);' 1 'main binds the returned explicit P before Realm entry'
for removed_writer in legacy_static_data install_program_static_data ensure_heap_ptr_after_static_data 'Instruction::MemoryInit' PRIVATE_BYTE_CURSOR_GLOBAL_INDEX; do
  require_fixed_string_count "$wasm_body_compilation" "$removed_writer" 0 'P initialization cannot install bytes or reset R scratch'
  require_fixed_string_count crates/scriptw-aot-wasm/src/emit/body_compilation/program_initialize.rs "$removed_writer" 0 'pure P bootstrap cannot install bytes or reset R scratch'
done
if [[ -e crates/scriptw-aot-wasm/src/emit/body_compilation/legacy_static_data.rs ]]; then
  fail 'removed P linear metadata writer must not remain as detached source'
fi
require_fixed_string_count crates/scriptw-aot-wasm/src/emitted_function.rs 'pub(crate) fn program_initialize(body: crate::emit::CompletedProgramInitializeBody)' 1 'bootstrap declaration requires its completed typed owner'
require_fixed_string_count crates/scriptw-aot-wasm/src/builtins/function/dynamic_constructor.rs 'self.emit_native_dynamic_function_compile(' 1 'one native compile path after prepared miss'


# Coherent ABI6 literal/metadata/entry owner attachments.
require_module_decl "$wasm_module_assembly" program_entries
require_file crates/scriptw-aot-wasm/src/emit/module_assembly/program_entries.rs
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/emit/module_assembly/program_entries.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/emit/module_assembly/program_entries.rs 180
require_file crates/scriptw-aot-wasm/src/data/pooled_literal_domain.rs
require_module_decl crates/scriptw-aot-wasm/src/data.rs pooled_literal_domain
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/data/pooled_literal_domain.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/data/pooled_literal_domain.rs 140
require_file crates/scriptw-aot-wasm/src/data/program_metadata.rs
require_module_decl crates/scriptw-aot-wasm/src/data.rs program_metadata
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/data/program_metadata.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/data/program_metadata.rs 180
require_file crates/scriptw-aot-wasm/src/data/program_string_descriptors/tests.rs
require_module_decl crates/scriptw-aot-wasm/src/data/program_string_descriptors.rs tests
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/data/program_string_descriptors/tests.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/data/program_string_descriptors/tests.rs 160
require_file crates/scriptw-aot-wasm/src/emit/body_entry/literal_roots/runtime_strings.rs
require_module_decl crates/scriptw-aot-wasm/src/emit/body_entry/literal_roots.rs runtime_strings
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/emit/body_entry/literal_roots/runtime_strings.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/emit/body_entry/literal_roots/runtime_strings.rs 160
require_file crates/scriptw-aot-wasm/src/functions/legacy_activation/property.rs
require_module_decl crates/scriptw-aot-wasm/src/functions/legacy_activation.rs property
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/functions/legacy_activation/property.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/functions/legacy_activation/property.rs 280
require_file crates/scriptw-aot-wasm/src/gc_types/value/native_function_compiler.rs
require_module_decl crates/scriptw-aot-wasm/src/gc_types/value.rs native_function_compiler
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/gc_types/value/native_function_compiler.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/gc_types/value/native_function_compiler.rs 70
require_file crates/scriptw-aot-wasm/src/gc_types/value/program_instance.rs
require_module_decl crates/scriptw-aot-wasm/src/gc_types/value.rs program_instance
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/gc_types/value/program_instance.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/gc_types/value/program_instance.rs 420
require_file crates/scriptw-aot-wasm/src/gc_types/value/program_instance/literal_index.rs
require_module_decl crates/scriptw-aot-wasm/src/gc_types/value/program_instance.rs literal_index
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/gc_types/value/program_instance/literal_index.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/gc_types/value/program_instance/literal_index.rs 60
require_file crates/scriptw-aot-wasm/src/gc_types/value/program_instance/metadata.rs
require_module_decl crates/scriptw-aot-wasm/src/gc_types/value/program_instance.rs metadata
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/gc_types/value/program_instance/metadata.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/gc_types/value/program_instance/metadata.rs 320
require_file crates/scriptw-aot-wasm/src/module/static_signature/program_ownership_tests.rs
require_module_decl crates/scriptw-aot-wasm/src/module/static_signature.rs program_ownership_tests
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/module/static_signature/program_ownership_tests.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/module/static_signature/program_ownership_tests.rs 100
require_fixed_string_count crates/scriptw-aot-wasm/src/runtime_helpers.rs 'Self::Runtime | Self::Hook(_) | Self::ContextHook => HelperTypeRegion::Core' 1 'hook signatures share the core GC recursion group'
require_fixed_string_count crates/scriptw-aot-wasm/src/runtime_helpers.rs 'Self::Program => HelperTypeRegion::Program' 1 'ordinary P signatures remain outside core recursion'
require_fixed_string_count "$wasm_gc_types" 'struct ProgramHookGlobals' 0 'no shared mutable P hook globals'
require_fixed_string_count "$wasm_gc_types" 'fn program_hook_global(' 0 'no ambient P hook lookup'
require_fixed_string_count crates/scriptw-aot-wasm/src/program_hooks.rs 'owner.reference(schema, function)' 1 'explicit retained hook owner supplies its actual rooted P'
require_fixed_string_count crates/scriptw-aot-wasm/src/gc_types/type_registration_tests.rs 'instance_group.contains(&(helper.type_index() as usize))' 1 'actual serialized hook/ProgramInstance recursion-group control'
require_fixed_string_count "$wasm_gc_values" 'ProgramMetadataView' 1 'actual bounded metadata reader is reexported'
wasm_native_function_public_adapter="$(braced_rust_item_source crates/scriptw-aot-wasm/src/lib.rs '^pub use runtime_function::[{]' | tr -d '[:space:]')"
require_regex_count crates/scriptw-aot-wasm/src/lib.rs '^pub use runtime_function::[{]$' 1 'actual public AOT native Function adapter owner'
require_text_regex_count "$wasm_native_function_public_adapter" '^pubuseruntime_function::[{]emit_runtime_function,emit_runtime_function_with_validated_runtime,RuntimeFunctionArtifact,[}];$' 1 'actual checked and once-validated public AOT native Function adapters'

# Native global Script compilation has its own typed host import and owned P factory.
native_global_script="crates/scriptw-aot-wasm/src/runtime_global_script.rs"
native_global_compiler="crates/scriptw-aot-wasm/src/prepared_script/native_compiler.rs"
native_global_operands="crates/scriptw-aot-wasm/src/gc_types/value/native_global_script_compiler.rs"
require_module_decl crates/scriptw-aot-wasm/src/lib.rs runtime_global_script
require_module_decl crates/scriptw-aot-wasm/src/prepared_script.rs native_compiler
require_module_decl crates/scriptw-aot-wasm/src/gc_types/value.rs native_global_script_compiler
for native_global_file in "$native_global_script" "$native_global_compiler" "$native_global_operands"; do
  require_file "$native_global_file"
  check_no_inline_legacy_includes "$native_global_file"
  check_raw_line_budget "$native_global_file" 180
done
require_fixed_string_count crates/scriptw-aot-wasm/src/gc_types/host.rs \
  'CompileGlobalScript => ("compile_global_script", HostCompileGlobalScript),' 1 'distinct global Script host compiler declaration'
require_fixed_string_count "$native_global_compiler" \
  '.gc_host_import(GcHostImport::CompileGlobalScript)' 1 'actual global-only native compiler consumer'
require_fixed_string_count "$native_global_script" \
  'let unit = require_root_source_row(program, first)?;' 1 'exact source/kind/root witness admission'
require_fixed_string_count crates/scriptw-aot-wasm/src/runtime_artifact.rs \
  'const LAYOUT_VERSION: u32 = 21;' 1 'current typed native global-source runtime contract'
for native_global_raw_test in \
  crates/scriptw-aot-wasm/src/runtime_artifact/cache/tests.rs \
  crates/scriptw-aot-wasm/src/runtime_artifact/build_package/tests.rs; do
  require_fixed_string_count "$native_global_raw_test" 'let previous_version = 15u32;' 1 'authenticated previous ABI15 envelope rejection'
  require_fixed_string_count "$native_global_raw_test" 'let previous_version = 6u32;' 1 'new ABI6 envelope rejection'
done
native_global_dispatch="$(braced_rust_item_source crates/scriptw-aot-wasm/src/prepared_script.rs '^[[:space:]]*fn emit_prepared_script_dispatch_with_owner[(]')"
require_text_regex_count "$native_global_dispatch" 'self[.]emit_native_global_script_compile[(]' 1 'one native miss after actual prepared hook dispatch'
require_text_regex_count "$native_global_dispatch" 'PreparedScriptDispatch::Global[(]_[)]' 2 'closed global Realm selection and native miss owner'
require_text_regex_count "$native_global_dispatch" 'PreparedScriptDispatch::ShadowRealm' 3 'separate ShadowRealm selection, rejection and status owner'

# Opaque Realm Script source is admitted only through actual callable identity.
realm_script_resolution="crates/scriptw-ir/src/lowering/dynamic_source.rs"
require_fixed_string_count "$realm_script_resolution" \
  '    RealmScriptInvocation(AdmittedRealmScriptInvocation),' 1 'closed runtime Realm Script invocation proof'
require_fixed_string_count "$realm_script_resolution" \
  '                AdmittedRealmScriptInvocation(()),' 1 'single actual-identity proof producer'
require_fixed_string_count crates/scriptw-ir/src/lowering.rs \
  'Some(ResolvedDynamicSourceCall::RealmScriptInvocation(proof))' 2 'plain and spread proof consumers'
require_fixed_string_count crates/scriptw-ir/src/lowering/call_candidate_analysis.rs \
  'Some(ResolvedDynamicSourceCall::RealmScriptInvocation(proof))' 1 'actual call-candidate proof consumer'
require_module_decl "$realm_script_resolution" native_global_script_tests
require_file crates/scriptw-ir/src/lowering/dynamic_source/native_global_script_tests.rs

# Both native source callbacks consume one exact-R/original-budget execution authority.
require_module_decl crates/scriptw-engine/src/lib.rs wasm_native_source_compiler
require_module_decl crates/scriptw-engine/src/lib.rs wasm_native_global_script_compiler
native_source_context="crates/scriptw-engine/src/wasm_native_source_compiler.rs"
native_global_host="crates/scriptw-engine/src/wasm_native_global_script_compiler.rs"
native_global_factory="crates/scriptw-engine/src/wasm_native_global_script_compiler/factory.rs"
for native_global_host_file in "$native_source_context" "$native_global_host" "$native_global_factory" \
  crates/scriptw-engine/src/wasm_native_global_script_compiler/signature.rs \
  crates/scriptw-engine/src/wasm_native_global_script_compiler/sources.rs \
  crates/scriptw-engine/src/wasm_native_global_script_compiler/outcome.rs \
  crates/scriptw-engine/src/wasm_native_global_script_compiler/observer.rs; do
  require_file "$native_global_host_file"
  check_no_inline_legacy_includes "$native_global_host_file"
  check_raw_line_budget "$native_global_host_file" 220
done
require_fixed_string_count crates/scriptw-engine/src/lib.rs \
  'wasm_native_source_compiler::NativeSourceCompilerContext::new(' 1 'single actual-R execution compiler context producer'
require_fixed_string_count crates/scriptw-engine/src/wasm_gc_host.rs \
  'wasm_native_global_script_compiler::link(linker, module)?;' 1 'distinct actual global Script compiler linker'
require_fixed_string_count "$native_global_factory" \
  'require_exact_request(request, program)?;' 1 'actual decoded source/kind/root witness check before factory execution'
require_fixed_string_count "$native_global_factory" \
  'scriptw_aot_wasm::NativeProgramTemplate::for_global_script(&artifact)' 1 'fresh typed P data with bounded actual-code compilation'
require_fixed_string_count "$native_global_factory" \
  'validate_factory_type(&factory.ty(&*store), signature)?;' 1 'actual canonical P/String/Realm factory admission'
require_fixed_string_count crates/scriptw-engine/src/wasm_native_global_script_compiler/sources.rs \
  '_calling_program: OwnedRooted<StructRef>,' 1 'calling P retained across the actual request'
require_fixed_string_count crates/scriptw-engine/src/wasm_native_global_script_compiler/outcome.rs \
  'scriptw_aot_wasm::check_gc_main_completion(kind, target).map_err(abi_error)?;' 1 'Normal/Throw and no local-target result validation'

require_module_decl crates/scriptw-aot-wasm/src/emit/module_assembly.rs import_indices
require_fixed_string_count crates/scriptw-aot-wasm/src/emit/module_assembly/import_indices.rs \
  'pub(super) fn claim_optional_host_import_function_index(next: &mut u32) -> u32 {' 1 'ordered optional-host import index allocator owner'
check_raw_line_budget crates/scriptw-aot-wasm/src/emit/module_assembly/import_indices.rs 12
# The new ABI7 signature control has its own cfg(test) owner; ABI6 keeps cap100.
native_global_signature_tests="crates/scriptw-aot-wasm/src/module/static_signature/native_global_script_tests.rs"
require_file "$native_global_signature_tests"
require_exact_line_count crates/scriptw-aot-wasm/src/module/static_signature.rs 'mod native_global_script_tests;' 1 'private ABI7 static-signature test owner'
native_global_signature_parent_code="$(tr -d '[:space:]' < crates/scriptw-aot-wasm/src/module/static_signature.rs)"
if [[ "$native_global_signature_parent_code" != *'#[cfg(test)]modnative_global_script_tests;'* ]]; then
  fail 'the ABI7 static-signature test owner must stay cfg(test)'
fi
check_no_inline_legacy_includes "$native_global_signature_tests"
check_raw_line_budget "$native_global_signature_tests" 60
require_fixed_string_count "$native_global_signature_tests" 'fn native_global_script_compiler_has_a_distinct_exact_gc_request_and_status_domain()' 1 'preserved ABI7 signature-domain test'
require_fixed_string_count crates/scriptw-aot-wasm/src/module/static_signature/program_ownership_tests.rs 'fn native_global_script_compiler_has_a_distinct_exact_gc_request_and_status_domain()' 0 'no duplicate ABI7 test in the capped ABI6 owner'

# Append after helper ownership checks in the complete ABI8 union guard.
# Replace the earlier fixed Core-region assertion to include ContextHook.
require_module_decl crates/scriptw-aot-wasm/src/functions.rs instance_elements_hook
require_module_decl crates/scriptw-aot-wasm/src/gc_types/value.rs instance_elements
require_module_decl crates/scriptw-aot-wasm/src/emit/module_assembly.rs program_helper_bodies
check_raw_line_budget crates/scriptw-aot-wasm/src/functions/instance_elements_hook.rs 260
check_raw_line_budget crates/scriptw-aot-wasm/src/gc_types/value/instance_elements.rs 80
check_raw_line_budget crates/scriptw-aot-wasm/src/emit/module_assembly/program_helper_bodies.rs 180
python3 scripts/check-abi8-instance-elements.py

# ABI8 direct eval keeps its caller witness and P2 factory separate from global Script.
require_module_decl crates/scriptw-aot-wasm/src/lib.rs runtime_direct_eval
require_module_decl crates/scriptw-aot-wasm/src/runtime_direct_eval.rs witness
require_module_decl crates/scriptw-aot-wasm/src/runtime_direct_eval.rs tests
require_module_decl crates/scriptw-aot-wasm/src/functions.rs native_direct_eval
require_module_decl crates/scriptw-aot-wasm/src/functions.rs native_direct_eval_factory
require_module_decl crates/scriptw-aot-wasm/src/gc_types/value.rs native_direct_eval_compiler
require_module_decl crates/scriptw-ir/src/lib.rs runtime_direct_eval
require_module_decl crates/scriptw-ir/src/lib.rs import_call_source_origin
require_file crates/scriptw-aot-wasm/src/runtime_direct_eval.rs
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/runtime_direct_eval.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_direct_eval.rs 120
require_file crates/scriptw-aot-wasm/src/runtime_direct_eval/witness.rs
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/runtime_direct_eval/witness.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_direct_eval/witness.rs 120
require_file crates/scriptw-aot-wasm/src/runtime_direct_eval/tests.rs
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/runtime_direct_eval/tests.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_direct_eval/tests.rs 260
require_file crates/scriptw-aot-wasm/src/functions/native_direct_eval.rs
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/functions/native_direct_eval.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/functions/native_direct_eval.rs 180
require_file crates/scriptw-aot-wasm/src/functions/native_direct_eval_factory.rs
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/functions/native_direct_eval_factory.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/functions/native_direct_eval_factory.rs 160
require_file crates/scriptw-aot-wasm/src/gc_types/value/native_direct_eval_compiler.rs
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/gc_types/value/native_direct_eval_compiler.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/gc_types/value/native_direct_eval_compiler.rs 80
require_file crates/scriptw-ir/src/runtime_direct_eval.rs
check_no_inline_legacy_includes crates/scriptw-ir/src/runtime_direct_eval.rs
check_raw_line_budget crates/scriptw-ir/src/runtime_direct_eval.rs 150
require_file crates/scriptw-ir/src/runtime_direct_eval/caller.rs
check_no_inline_legacy_includes crates/scriptw-ir/src/runtime_direct_eval/caller.rs
check_raw_line_budget crates/scriptw-ir/src/runtime_direct_eval/caller.rs 210
require_file crates/scriptw-ir/src/runtime_direct_eval/codec.rs
check_no_inline_legacy_includes crates/scriptw-ir/src/runtime_direct_eval/codec.rs
check_raw_line_budget crates/scriptw-ir/src/runtime_direct_eval/codec.rs 180
require_file crates/scriptw-ir/src/runtime_direct_eval/private_capacity.rs
check_no_inline_legacy_includes crates/scriptw-ir/src/runtime_direct_eval/private_capacity.rs
check_raw_line_budget crates/scriptw-ir/src/runtime_direct_eval/private_capacity.rs 110
require_file crates/scriptw-ir/src/runtime_direct_eval/tests.rs
check_no_inline_legacy_includes crates/scriptw-ir/src/runtime_direct_eval/tests.rs
check_raw_line_budget crates/scriptw-ir/src/runtime_direct_eval/tests.rs 440
require_file crates/scriptw-ir/src/import_call_source_origin.rs
check_no_inline_legacy_includes crates/scriptw-ir/src/import_call_source_origin.rs
check_raw_line_budget crates/scriptw-ir/src/import_call_source_origin.rs 25
for abi8_raw_test in crates/scriptw-aot-wasm/src/runtime_artifact/cache/tests.rs crates/scriptw-aot-wasm/src/runtime_artifact/build_package/tests.rs; do
  require_fixed_string_count "$abi8_raw_test" 'let previous_version = 7u32;' 1 'retained authenticated ABI7 envelope rejection'
done
python3 scripts/check-abi8-native-direct-eval.py

# Append after the complete ABI8 native direct-eval source guards. Existing caps stay unchanged.
require_file scripts/check-abi8-native-direct-eval-host.py
check_raw_line_budget scripts/check-abi8-native-direct-eval-host.py 280
require_module_decl crates/scriptw-engine/src/lib.rs wasm_native_direct_eval_compiler
require_module_decl crates/scriptw-engine/src/lib.rs native_direct_eval_compiler_tests
require_module_decl crates/scriptw-engine/src/wasm_native_direct_eval_compiler.rs factory
require_module_decl crates/scriptw-engine/src/wasm_native_direct_eval_compiler.rs outcome
require_module_decl crates/scriptw-engine/src/wasm_native_direct_eval_compiler.rs schema
require_module_decl crates/scriptw-engine/src/wasm_native_direct_eval_compiler.rs signature
require_module_decl crates/scriptw-engine/src/wasm_native_direct_eval_compiler.rs sources
require_module_decl crates/scriptw-engine/src/wasm_native_direct_eval_compiler/sources.rs invocation
require_module_decl crates/scriptw-engine/src/wasm_native_direct_eval_compiler/sources.rs private_chain
require_file crates/scriptw-engine/src/wasm_native_direct_eval_compiler.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_native_direct_eval_compiler.rs 160
require_file crates/scriptw-engine/src/wasm_native_direct_eval_compiler/factory.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_native_direct_eval_compiler/factory.rs 180
require_file crates/scriptw-engine/src/wasm_native_direct_eval_compiler/signature.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_native_direct_eval_compiler/signature.rs 200
require_file crates/scriptw-engine/src/wasm_native_direct_eval_compiler/schema.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_native_direct_eval_compiler/schema.rs 140
require_file crates/scriptw-engine/src/wasm_native_direct_eval_compiler/sources.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_native_direct_eval_compiler/sources.rs 140
require_file crates/scriptw-engine/src/wasm_native_direct_eval_compiler/sources/private_chain.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_native_direct_eval_compiler/sources/private_chain.rs 90
require_file crates/scriptw-engine/src/wasm_native_direct_eval_compiler/sources/invocation.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_native_direct_eval_compiler/sources/invocation.rs 150
require_file crates/scriptw-engine/src/wasm_native_direct_eval_compiler/outcome.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_native_direct_eval_compiler/outcome.rs 180
require_file crates/scriptw-engine/src/wasm_native_direct_eval_compiler/observer.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_native_direct_eval_compiler/observer.rs 80
require_file crates/scriptw-engine/src/wasm_native_direct_eval_compiler/tests.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_native_direct_eval_compiler/tests.rs 150
require_file crates/scriptw-engine/src/wasm_native_direct_eval_compiler/tests/fixture.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_native_direct_eval_compiler/tests/fixture.rs 80
require_file crates/scriptw-engine/src/native_direct_eval_compiler_tests.rs
check_raw_line_budget crates/scriptw-engine/src/native_direct_eval_compiler_tests.rs 120
require_file crates/scriptw-engine/src/native_direct_eval_compiler_tests/caller_records.rs
check_raw_line_budget crates/scriptw-engine/src/native_direct_eval_compiler_tests/caller_records.rs 100
require_file crates/scriptw-engine/src/native_direct_eval_compiler_tests/ownership.rs
check_raw_line_budget crates/scriptw-engine/src/native_direct_eval_compiler_tests/ownership.rs 80
require_file crates/scriptw-engine/src/native_direct_eval_compiler_tests/source_and_errors.rs
check_raw_line_budget crates/scriptw-engine/src/native_direct_eval_compiler_tests/source_and_errors.rs 110
require_fixed_string_count crates/scriptw-aot-wasm/src/gc_types/host.rs 'function_field(&FunctionContextSchema::INSTANCE_ELEMENTS_INITIALIZER)' 1 'declaration-owned typed constructor initializer projection'
require_fixed_string_count crates/scriptw-engine/src/wasm_native_direct_eval_compiler/signature.rs 'schema::field(&function_context, Field::ContextInstanceElementsInitializer)?;' 1 'native signature validates initializer storage from actual context'
python3 scripts/check-abi8-native-direct-eval-host.py

# Profile-proof successor adds only new-owner caps; existing owner caps remain.
require_file scripts/check-abi8-validated-runtime-profile.py
check_raw_line_budget scripts/check-abi8-validated-runtime-profile.py 220
require_module_decl crates/scriptw-aot-wasm/src/runtime_artifact.rs validated_profile
require_file crates/scriptw-aot-wasm/src/runtime_artifact/validated_profile.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_artifact/validated_profile.rs 100
require_file crates/scriptw-aot-wasm/src/runtime_artifact/validated_profile/tests.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_artifact/validated_profile/tests.rs 320
require_file crates/scriptw-engine/src/wasm_native_source_compiler/tests.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_native_source_compiler/tests.rs 160
python3 scripts/check-abi8-validated-runtime-profile.py

# Native module-source objects retain their actual GC owner and defining Realm.
wasm_module_source='crates/scriptw-aot-wasm/src/modules/source.rs'
wasm_module_source_tests='crates/scriptw-aot-wasm/src/gc_types/module_source_tests.rs'
require_file "$wasm_module_source"
require_file "$wasm_module_source_tests"
require_module_decl 'crates/scriptw-aot-wasm/src/modules.rs' source
require_module_decl 'crates/scriptw-aot-wasm/src/gc_types.rs' module_source_tests
require_fixed_string_count 'crates/scriptw-aot-wasm/src/gc_types/layouts.rs' 'struct ModuleSourceObject => ModuleSourceObjectSchema {' 1 'real native module-source GC brand'
require_fixed_string_count 'crates/scriptw-aot-wasm/src/gc_types/layouts.rs' 'SOURCE_CELL: GcRef<BindingCell>, Immutable, NonNullable;' 1 'canonical strong source cell on actual ModuleRecord'
require_fixed_string_count 'crates/scriptw-aot-wasm/src/gc_types/layouts.rs' 'SOURCE_TAG: GcRef<StringValue>, Immutable, NonNullable;' 1 'host tag retained as actual GC String'
require_fixed_string_count "$wasm_module_source" '.field(ModuleRecordSchema::SOURCE_CELL)' 1 'canonical source-cell selection'
require_fixed_string_count "$wasm_module_source" '.field(ModuleRecordSchema::REALM)' 1 'defining Realm read from actual module owner'
require_fixed_string_count "$wasm_module_source" 'NonArrayRealmIntrinsicSlot::ModuleSourcePrototype,' 1 'actual defining-Realm stable host source prototype'
require_fixed_string_count "$wasm_module_source" 'schema.struct_type::<ModuleSourceObject>().construct(' 1 'single source-brand allocation owner'
require_fixed_string_count "$wasm_module_source" 'Instruction::RefTestNonNull(' 1 'getter tests concrete GC brand'
require_fixed_string_count "$wasm_module_source" '.field(ModuleSourceObjectSchema::SOURCE_TAG)' 1 'getter reads private actual source tag'
require_fixed_string_count 'crates/scriptw-aot-wasm/src/builtins/standard.rs' 'self.emit_module_source_to_string_tag_builtin(function)?;' 1 'consumed native intrinsic getter'
require_fixed_string_count 'crates/scriptw-aot-wasm/src/modules/synchronous.rs' 'self.emit_module_source_namespaces(graph, function)?;' 1 'native empty namespaces published without JS activation'
require_fixed_string_count 'crates/scriptw-aot-wasm/src/modules/synchronous.rs' 'self.module_source_cell(&record, *module, function)?' 1 'typed resolved source binding consumes canonical cell'
check_no_inline_legacy_includes "$wasm_module_source"
check_no_inline_legacy_includes "$wasm_module_source_tests"
check_raw_line_budget "$wasm_module_source" 180
check_raw_line_budget "$wasm_module_source_tests" 120
if rg -n 'emit_function_or_proxy_call|emit_private_module_resume|<module source>' "$wasm_module_source"; then
  fail 'native source records cannot invoke a JS activation or recognize a fixture locator'
fi
# Explicit proposal policy owns only new files/caps; no existing checks/caps removed.
require_file scripts/check-private-element-policy.py
check_raw_line_budget scripts/check-private-element-policy.py 180
require_file crates/scriptw-ir/src/language_features.rs
check_raw_line_budget crates/scriptw-ir/src/language_features.rs 80
require_module_decl crates/scriptw-ir/src/lib.rs language_features
require_file crates/scriptw-engine/src/native_language_feature_policy_tests.rs
check_raw_line_budget crates/scriptw-engine/src/native_language_feature_policy_tests.rs 160
require_module_decl crates/scriptw-engine/src/lib.rs native_language_feature_policy_tests
require_file crates/scriptw-engine/tests/aot_language/aot_private_element_extensibility.rs
check_raw_line_budget crates/scriptw-engine/tests/aot_language/aot_private_element_extensibility.rs 300
require_module_decl crates/scriptw-engine/tests/aot_language/main.rs aot_private_element_extensibility
python3 -B scripts/check-private-element-policy.py --root .
# Generic host ModuleSource records retain typed identity through real module cells.
ir_source_records='crates/scriptw-ir/src/modules/synchronous_execution.rs'
ir_source_definitions='crates/scriptw-ir/src/modules/synchronous_definition.rs'
ir_source_assembly='crates/scriptw-ir/src/modules/synchronous_source.rs'
ir_source_graph='crates/scriptw-ir/src/modules/graph.rs'
ir_source_loaded='crates/scriptw-ir/src/modules/loaded_sources.rs'
ir_source_tests='crates/scriptw-ir/src/modules/graph_source_phase_export_tests.rs'
engine_source_tests='crates/scriptw-engine/tests/aot_module_sources.rs'
require_file "$ir_source_tests"
require_file "$engine_source_tests"
require_module_decl 'crates/scriptw-ir/src/modules/graph_tests.rs' source_phase_exports
require_fixed_string_count "$ir_source_loaded" 'pub fn module_source(' 1 'host source constructor without JavaScript parsing'
ir_host_source_constructor="$(braced_rust_item_source "$ir_source_loaded" '^pub fn module_source[(]')"
require_text_regex_count "$ir_host_source_constructor" 'parse: ModuleParse::ModuleSource' 1 'native host payload constructor owns the typed non-syntax variant'
if printf '%s\n' "$ir_host_source_constructor" | grep -Eq 'scriptw_front::parse|boa_parser|LoweringSession'; then
  fail 'host ModuleSource data must not enter a JavaScript parse/lowering path'
fi
require_fixed_string_count "$ir_source_records" 'pub struct ModuleSourceRecordIr {' 1 'typed actual module/source-tag descriptor'
require_fixed_string_count "$ir_source_records" 'pub fn source_records(&self) -> &[ModuleSourceRecordIr]' 1 'immutable execution-graph source descriptor projection'
require_fixed_string_count "$ir_source_records" 'native source records own valid non-activation identities' 2 'production owner validation and focused invalid-owner control'
require_fixed_string_count "$ir_source_definitions" 'self.source_records.clone(),' 1 'actual descriptor retention into execution graph'
require_fixed_string_count "$ir_source_definitions" 'ModuleCellIr::ModuleSource { module: *module }' 1 'resolved source binding lowered to its distinct canonical cell'
require_fixed_string_count "$ir_source_assembly" '.filter(|request| request.phase() != ImportPhaseIr::Source)' 1 'source occurrences excluded from evaluation dependencies'
require_fixed_string_count "$ir_source_graph" 'graph.unit(*source).record.module_source_tag().is_none()' 2 'ordinary-record source-unavailable checks use actual resolved owner'
require_fixed_string_count 'crates/scriptw-ir/src/binding_names.rs' 'ModuleSource => "source";' 1 'source placeholder role distinct from namespaces'
require_fixed_string_count 'crates/scriptw-engine/src/module_loader.rs' 'match resolution.target_kind() {' 1 'Engine consumes validated actual target representation'
require_fixed_string_count 'crates/scriptw-runtime/src/embedded_module_graph.rs' 'pub const fn target_kind(&self) -> EmbeddedModuleKind' 1 'host target kind has one validated projection'
require_fixed_string_count 'crates/scriptw-runtime/src/embedded_module_graph.rs' 'target_kind: target.expect("checked declared target").kind(),' 1 'resolved host record kind retained rather than inferred later'
require_fixed_string_count 'crates/scriptw-runtime/src/embedded_module_graph.rs' 'hash.update([match resolution.target_kind {' 1 'actual host record kind bound into graph fingerprint'
require_fixed_string_count 'crates/scriptw-ir/src/modules/module_unit.rs' 'if let Some(tag) = self.record.module_source_tag() {' 1 'retained host tag participates in exact duplicate-load identity'
require_fixed_string_count 'crates/scriptw-test262/src/differential/embedded_graph.rs' '(ModuleKindWire::ModuleSource, Some(source_tag), None)' 1 'wire accepts exactly one declared source-tag payload'
require_fixed_string_count 'crates/scriptw-test262/src/module_catalog.rs' 'specifier == "<module source>" && attributes.is_empty()' 1 'Test262-only implementation-defined virtual host contract'
check_no_inline_legacy_includes "$ir_source_tests"
check_registered_module_source_test_child "$engine_source_tests"
check_raw_line_budget "$ir_source_tests" 180
check_raw_line_budget "$engine_source_tests" 280
if rg -n '<module source>' "$ir_source_loaded" "$ir_source_records" "$ir_source_definitions" "$ir_source_assembly" "$ir_source_graph" 'crates/scriptw-runtime/src/embedded_module_graph.rs' 'crates/scriptw-engine/src/module_loader.rs'; then
  fail 'compiler/runtime module-source support cannot recognize Test262 fixture locators'
fi
# Final async-generator body completion distinguishes runtime undefined from an
# outstanding return Await, while source expression/protocol Await stays owned.
wasm_async_generator_body='crates/scriptw-aot-wasm/src/functions/async_generator_body.rs'
wasm_async_generator_return_tests='crates/scriptw-engine/tests/aot_async_generator_returns.rs'
require_file "$wasm_async_generator_body"
require_file "$wasm_async_generator_return_tests"
require_fixed_string_count "$wasm_async_generator_body" 'body.value().tag().load(function);' 1 'actual runtime body-completion value classification'
require_fixed_string_count "$wasm_async_generator_body" 'WasmRuntimeValueTag::Undefined.tag()' 1 'undefined completion fast path after source evaluation'
require_fixed_string_count "$wasm_async_generator_body" 'AsyncGeneratorReturnStage::Awaited' 2 'preserved typed already-awaited stage writer and classifier'
require_fixed_string_count "$wasm_async_generator_return_tests" 'return-undefined-implicit-and-explicit.js' 1 'exact pinned fixture inclusion'
require_fixed_string_count "$wasm_async_generator_return_tests" 'for directive in ["", "'\''use strict'\'';\n"]' 1 'both language modes retain the original test body'
check_no_inline_legacy_includes "$wasm_async_generator_body"
check_no_inline_legacy_includes "$wasm_async_generator_return_tests"
check_raw_line_budget "$wasm_async_generator_body" 370
check_raw_line_budget "$wasm_async_generator_return_tests" 140
python3 - "$wasm_async_generator_body" <<'PY'
import pathlib, sys
source = pathlib.Path(sys.argv[1]).read_text()
start = source.index('body.kind().load(function);')
start = source.index('AsyncGeneratorReturnStage::Awaited,', start)
end = source.index('self.open_frame(ControlFrameKind::If, function);', start)
condition = source[start:end]
needles = [
    'AsyncGeneratorReturnStage::Awaited,',
    'body.value().tag().load(function);',
    'WasmRuntimeValueTag::Undefined.tag()',
    'Instruction::I32Or',
]
positions = [condition.index(needle) for needle in needles]
assert positions == sorted(positions), 'undefined must join the final already-awaited completion test'
assert 'Instruction::I32And' not in condition, 'either processed Return or runtime undefined must complete directly'
PY

# Typed P policy reader is outlined under its actual ProgramInstance owner.
require_module_decl crates/scriptw-aot-wasm/src/gc_types/value/program_instance.rs language_features
require_file crates/scriptw-aot-wasm/src/gc_types/value/program_instance/language_features.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/gc_types/value/program_instance/language_features.rs 60


# Exact-R immutable compiler-owned pool and P-local appendices.
require_module_decl crates/scriptw-aot-wasm/src/data.rs shared_pool_storage
require_module_decl crates/scriptw-aot-wasm/src/data.rs compiler_owned_pool
require_file crates/scriptw-aot-wasm/src/data/shared_pool_storage.rs
require_file crates/scriptw-aot-wasm/src/data/compiler_owned_pool.rs
require_module_decl crates/scriptw-aot-wasm/src/data/compiler_owned_pool.rs tests
require_file crates/scriptw-aot-wasm/src/data/compiler_owned_pool/tests.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/data/shared_pool_storage.rs 190
check_raw_line_budget crates/scriptw-aot-wasm/src/data/compiler_owned_pool.rs 230
check_raw_line_budget crates/scriptw-aot-wasm/src/data/compiler_owned_pool/tests.rs 230
require_module_decl crates/scriptw-aot-wasm/src/runtime_artifact/validated_profile.rs pool_tests
require_file crates/scriptw-aot-wasm/src/runtime_artifact/validated_profile/pool_tests.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_artifact/validated_profile/pool_tests.rs 120
require_file scripts/check-immutable-compiler-pool.py
check_raw_line_budget scripts/check-immutable-compiler-pool.py 200
if ! python3 -B scripts/check-immutable-compiler-pool.py .; then
  echo "immutable compiler-pool ownership controls failed" >&2
  failures=$((failures + 1))
fi

# Realm-owned host ModuleSource prototype is distinct from its abstract parent.
require_file scripts/check-module-source-prototype.py
check_raw_line_budget scripts/check-module-source-prototype.py 180
require_module_decl crates/scriptw-engine/tests/aot_module_sources.rs prototype_chain
require_file crates/scriptw-engine/tests/aot_module_sources/prototype_chain.rs
check_raw_line_budget crates/scriptw-engine/tests/aot_module_sources/prototype_chain.rs 100
python3 -B scripts/check-module-source-prototype.py .
for module_source_old_r in crates/scriptw-aot-wasm/src/runtime_artifact/cache/tests.rs crates/scriptw-aot-wasm/src/runtime_artifact/build_package/tests.rs; do
  require_fixed_string_count "$module_source_old_r" 'let previous_version = 8u32;' 1 'authenticated old intrinsic-table layout rejection'
done

# Immutable compiler catalogs retain only per-lowering fact deviations.
require_file crates/scriptw-ir/src/lowering/catalog_overlay.rs
require_file crates/scriptw-ir/src/lowering/catalog_overlay/tests.rs
require_file crates/scriptw-ir/src/lowering/catalog_overlay/tests/changed_facts.rs
require_file crates/scriptw-ir/src/lowering/builtin_catalog/flow_tests.rs
check_raw_line_budget crates/scriptw-ir/src/lowering/catalog_overlay.rs 220
check_raw_line_budget crates/scriptw-ir/src/lowering/catalog_overlay/tests.rs 160
check_raw_line_budget crates/scriptw-ir/src/lowering/catalog_overlay/tests/changed_facts.rs 70
check_raw_line_budget crates/scriptw-ir/src/lowering/builtin_catalog/flow_tests.rs 180
python3 -B scripts/check-ir-shared-catalog-overlay.py . || fail 'IR shared catalog ownership checks failed'

# Opt-in execution-local native dynamic-source phase measurements.
require_module_decl crates/scriptw-engine/src/wasm_native_source_compiler.rs trace
require_file crates/scriptw-engine/src/wasm_native_source_compiler/trace.rs
check_no_inline_legacy_includes crates/scriptw-engine/src/wasm_native_source_compiler/trace.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_native_source_compiler/trace.rs 180
if ! python3 -B scripts/check-native-source-phase-trace.py .; then
  fail 'native source phase trace ownership or default-off boundary changed'
fi

# A source representation does not grant host module evaluation capability.
require_module_decl crates/scriptw-aot-wasm/src/planning/module_graph.rs evaluation_capability
require_module_decl crates/scriptw-aot-wasm/src/planning/module_graph/evaluation_capability.rs tests
require_module_decl crates/scriptw-aot-wasm/src/modules.rs evaluation_capability
require_module_decl crates/scriptw-engine/tests/aot_module_sources.rs evaluation_capability
require_file crates/scriptw-aot-wasm/src/planning/module_graph/evaluation_capability.rs
require_file crates/scriptw-aot-wasm/src/planning/module_graph/evaluation_capability/tests.rs
require_file crates/scriptw-aot-wasm/src/modules/evaluation_capability.rs
require_file crates/scriptw-engine/tests/aot_module_sources/evaluation_capability.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/planning/module_graph/evaluation_capability.rs 80
check_raw_line_budget crates/scriptw-aot-wasm/src/planning/module_graph/evaluation_capability/tests.rs 140
check_raw_line_budget crates/scriptw-aot-wasm/src/modules/evaluation_capability.rs 70
check_raw_line_budget crates/scriptw-engine/tests/aot_module_sources/evaluation_capability.rs 100
require_fixed_string_count crates/scriptw-ir/src/runtime_semantics.rs 'Self::UnavailableCapability(RuntimeUnavailableCapability::HostModuleEvaluation) => 12,' 1 'distinct closed host module evaluation rejection code'
require_fixed_string_count crates/scriptw-aot-wasm/src/planning/module_graph/evaluation_capability.rs 'self.source_record(module).is_some()' 1 'actual host record authority'
require_fixed_string_count crates/scriptw-aot-wasm/src/planning/module_graph/evaluation_capability.rs 'ModuleRequestPhaseIr::Evaluation => Some(request.target()),' 1 'only static evaluation dependencies require bodies'
require_fixed_string_count crates/scriptw-aot-wasm/src/planning/module_graph/evaluation_capability.rs 'ModuleRequestPhaseIr::Defer => None,' 1 'defer does not silently become eager evaluation'
require_fixed_string_count crates/scriptw-aot-wasm/src/planning/module_graph/evaluation_capability.rs 'if !visited.insert(module)' 1 'closed cycle-safe evaluation walk'
require_fixed_string_count crates/scriptw-aot-wasm/src/modules/synchronous.rs 'if self.emit_module_evaluation_capability_rejection(plan.module(), output, function) {' 4 'each actual typed evaluation/defer demand checks host capability'
require_fixed_string_count crates/scriptw-aot-wasm/src/modules/evaluation_capability.rs 'scriptw_ir::RuntimeSemanticRejection::UnavailableCapability(capability),' 1 'unavailable host body stays outside catchable JavaScript completion'
require_fixed_string_count crates/scriptw-engine/tests/aot_module_sources/evaluation_capability.rs "import * as empty from 'host:first';" 3 'unsupported native namespace input retained in negative and unchosen-branch controls'

# Portable products and native source hosts have separate admitted authority.
require_module_decl crates/scriptw-aot-wasm/src/runtime_artifact.rs source_compilation
require_file crates/scriptw-aot-wasm/src/runtime_artifact/source_compilation.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_artifact/source_compilation.rs 160
require_module_decl crates/scriptw-aot-wasm/src/runtime_artifact/source_compilation.rs tests
require_file crates/scriptw-aot-wasm/src/runtime_artifact/source_compilation/tests.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_artifact/source_compilation/tests.rs 160
require_module_decl crates/scriptw-engine/src/lib.rs source_compilation_profile_tests
require_file crates/scriptw-engine/src/source_compilation_profile_tests.rs
check_raw_line_budget crates/scriptw-engine/src/source_compilation_profile_tests.rs 180
python3 -B scripts/check-runtime-source-compilation-profile.py . || fail 'typed Portable/NativeHost source-compilation profile boundary drifted'


# Independent global harness and test Scripts retain separate grammar and entry owners.
require_module_decl crates/scriptw-ir/src/lowering.rs global_script_prelude
require_module_decl crates/scriptw-aot-wasm/src/gc_types.rs global_script_prelude
require_module_decl crates/scriptw-engine/src/lib.rs global_script_prelude
require_module_decl crates/scriptw-ir/src/lib.rs global_script_prelude_tests
require_module_decl crates/scriptw-engine/src/global_script_prelude.rs tests
for global_prelude_file in \
  crates/scriptw-ir/src/lowering/global_script_prelude.rs \
  crates/scriptw-aot-wasm/src/gc_types/global_script_prelude.rs \
  crates/scriptw-engine/src/global_script_prelude.rs; do
  require_file "$global_prelude_file"
  check_no_inline_legacy_includes "$global_prelude_file"
done
check_raw_line_budget crates/scriptw-ir/src/lowering/global_script_prelude.rs 150
check_raw_line_budget crates/scriptw-aot-wasm/src/gc_types/global_script_prelude.rs 80
check_raw_line_budget crates/scriptw-engine/src/global_script_prelude.rs 110
check_raw_line_budget crates/scriptw-ir/src/global_script_prelude_tests.rs 160
check_raw_line_budget crates/scriptw-engine/src/global_script_prelude/tests.rs 200
require_file scripts/check-independent-global-prelude.py
check_raw_line_budget scripts/check-independent-global-prelude.py 180
python3 -B scripts/check-independent-global-prelude.py

# Unicode Pattern scratch compaction owns no source value or public parser state.
require_file crates/scriptw-aot-wasm/src/builtins/regexp/compiler/compact_source.rs
require_module_decl crates/scriptw-aot-wasm/src/builtins/regexp/compiler.rs compact_source
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/builtins/regexp/compiler/compact_source.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/builtins/regexp/compiler/compact_source.rs 160
if ! python3 -B scripts/check-regexp-compact-unicode-source.py .; then
  fail 'private Unicode Pattern scratch compaction contract'
fi

# Empty-block native controls use a separate bounded cfg(test) child.
require_file crates/scriptw-engine/src/native_direct_eval_compiler_tests/empty_blocks.rs
check_raw_line_budget crates/scriptw-engine/src/native_direct_eval_compiler_tests/empty_blocks.rs 100
require_module_decl crates/scriptw-engine/src/native_direct_eval_compiler_tests.rs empty_blocks

# The existing Engine unit module retains its exact bounded cfg(test) owner.
require_module_decl crates/scriptw-engine/src/lib.rs tests
require_file crates/scriptw-engine/src/tests.rs
check_raw_line_budget crates/scriptw-engine/src/tests.rs 31719

# Canonical publication precedes independently instantiated global Scripts.
require_file scripts/check-canonical-global-bootstrap.py
check_raw_line_budget scripts/check-canonical-global-bootstrap.py 120
check_raw_line_budget crates/scriptw-ir/src/lowering/builtin_catalog/tests.rs 180
python3 -B scripts/check-canonical-global-bootstrap.py

# Bare lowered Empty never occupies root/block/function/CaseBlock list buffers.
require_file scripts/check-empty-statement-list-storage.py
check_raw_line_budget scripts/check-empty-statement-list-storage.py 120
python3 -B scripts/check-empty-statement-list-storage.py

# Unobserved arguments are omitted only for inert, empty strict ordinary bodies.
arguments_protocol=crates/scriptw-aot-wasm/src/arguments_protocol.rs
check_raw_line_budget "$arguments_protocol" 500
require_fixed_string_count "$arguments_protocol" 'if arguments_are_unobservable(function)' 1 'checked arguments materialization decision'
for condition in 'function.strict' 'FunctionExecutionKind::Ordinary' 'FunctionProtocolIr::ClassConstructor' 'function.captured_bindings.is_empty()' 'function.body.statements.is_empty()' 'has_simple_parameter_list(function)'; do
  require_fixed_string_present "$arguments_protocol" "$condition" 'conservative arguments observability proof'
done
require_fixed_string_present "$arguments_protocol" 'fn eval_visible_empty_strict_methods_have_no_arguments_observer()' 'actual eval-visible lowering regression'
require_file crates/scriptw-engine/tests/aot_native_program_templates/unobserved_arguments.rs
check_raw_line_budget crates/scriptw-engine/tests/aot_native_program_templates/unobserved_arguments.rs 100
require_module_decl crates/scriptw-engine/tests/aot_native_program_templates.rs unobserved_arguments
require_fixed_string_present crates/scriptw-engine/tests/aot_native_program_templates/unobserved_arguments.rs 'fn strict_empty_methods_and_observed_arguments_keep_their_actual_semantics()' 'actual native argument ownership regression'


# The typed empty-body path preserves callable entry, activation and root exit.
empty_strict_body=crates/scriptw-aot-wasm/src/emit/body_compilation/empty_strict.rs
require_file "$empty_strict_body"
check_raw_line_budget "$empty_strict_body" 90
require_module_decl "$wasm_body_compilation" empty_strict
require_fixed_string_count "$wasm_body_compilation" 'self.function_arguments_protocol.take_empty_strict_body()' 1 'consumed typed empty-body admission'
require_fixed_string_present "$arguments_protocol" 'pub(crate) struct EmptyStrictBody' 'private-constructible empty-body witness'
for lifecycle in 'self.take_owned_body()' 'self.init_template_source_execution(' 'self.emit_enter_source_activation(' 'self.open_source_activation_epilogue(' 'self.finish_source_activation_epilogue(' 'self.completion.emit(' 'self.clear_body_entry_roots(' 'execution.clear(' 'self.finish_function('; do
  require_fixed_string_count "$empty_strict_body" "$lifecycle" 1 'empty body retains callable lifecycle'
done


# Native-only code templates retain fresh source/data allocations and actual owners.
require_file scripts/check-native-program-templates.py
python3 scripts/check-native-program-templates.py

# Native regression for FunctionDeclarationInstantiation arguments ownership.
require_file crates/scriptw-engine/tests/aot_native_program_templates/annex_b_arguments.rs
check_raw_line_budget crates/scriptw-engine/tests/aot_native_program_templates/annex_b_arguments.rs 100
require_module_decl crates/scriptw-engine/tests/aot_native_program_templates.rs annex_b_arguments
