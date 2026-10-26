for module in abi arguments_protocol control_flow data emit environments expressions functions gc_types heap module modules objects operations planning; do
  require_file "crates/scriptw-aot-wasm/src/${module}.rs"
  require_module_decl "$wasm_lib" "$module"
done

# T15 gives async functions and plain generators one complete synchronous
# iterator owner. Typed wrappers select the closed activation protocol; the
# obligation ledger names the shared owner and both production entry points.
wasm_control_flow="crates/scriptw-aot-wasm/src/control_flow.rs"
wasm_async_function_for_of_iterator="crates/scriptw-aot-wasm/src/control_flow/async_function_for_of_iterator.rs"
wasm_resumable_sync_for_of_iterator="crates/scriptw-aot-wasm/src/control_flow/resumable_sync_for_of_iterator.rs"
wasm_resumable_sync_for_of_plan="crates/scriptw-aot-wasm/src/control_flow/resumable_sync_for_of_iterator/plan.rs"
wasm_for_await_iterator_plan="crates/scriptw-aot-wasm/src/control_flow/for_await_iterator_plan.rs"
wasm_emission_sites="crates/scriptw-aot-wasm/src/emission_sites.rs"
for private_child in async_function_for_of_iterator resumable_sync_for_of_iterator for_await_iterator_plan; do
  private_child_file="crates/scriptw-aot-wasm/src/control_flow/${private_child}.rs"
  require_file "$private_child_file"
  require_exact_line_count \
    "$wasm_control_flow" \
    "mod ${private_child};" \
    1 \
    "private ${private_child} child declarations"
  require_tree_regex_count \
    crates/scriptw-aot-wasm/src \
    "^[[:space:]]*mod[[:space:]]+${private_child};" \
    1 \
    "private ${private_child} child declarations"
  if grep -Eq "^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+${private_child};" "$wasm_control_flow"; then
    fail "$wasm_control_flow must keep $private_child private"
  fi
  check_no_inline_legacy_includes "$private_child_file"
done
check_no_inline_legacy_includes "$wasm_control_flow"

require_file "$wasm_resumable_sync_for_of_plan"
require_exact_line_count "$wasm_resumable_sync_for_of_iterator" 'mod plan;' 1 'private resumable plan projection module'
if grep -Eq '^pub(\([^)]*\))? mod plan;' "$wasm_resumable_sync_for_of_iterator"; then
  fail "$wasm_resumable_sync_for_of_iterator must keep its plan projection child private"
fi
check_no_inline_legacy_includes "$wasm_resumable_sync_for_of_plan"

require_exact_line_count \
  "$wasm_resumable_sync_for_of_iterator" \
  '    pub(crate) fn compile_resumable_sync_for_of_iterator(' \
  1 \
  'crate-visible shared resumable synchronous for-of owner'
require_tree_regex_count \
  crates/scriptw-aot-wasm/src \
  '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+compile_resumable_sync_for_of_iterator[[:space:]]*\(' \
  1 \
  'complete shared resumable synchronous for-of owners'
require_fixed_string_count \
  "$wasm_async_function_for_of_iterator" \
  '.compile_resumable_sync_for_of_iterator(' \
  2 \
  'typed async and generator calls to the shared owner'
require_exact_line_count \
  "$wasm_emission_sites" \
  '            let _ = FunctionBuilder::compile_resumable_sync_for_of_iterator;' \
  1 \
  'shared resumable synchronous for-of obligation-ledger reference'
require_tree_regex_count \
  crates/scriptw-aot-wasm/src \
  'compile_resumable_sync_for_of_iterator' \
  4 \
  'shared owner, two typed wrappers and obligation-ledger sites'

for wrapper in compile_async_function_for_of_iterator compile_generator_for_of_iterator; do
  require_exact_line_count \
    "$wasm_async_function_for_of_iterator" \
    "    pub(crate) fn ${wrapper}(" \
    1 \
    "typed ${wrapper} wrapper declarations"
  require_tree_regex_count \
    crates/scriptw-aot-wasm/src \
    "^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+${wrapper}[[:space:]]*\(" \
    1 \
    "typed ${wrapper} wrapper owners"
  require_fixed_string_count \
    "$wasm_control_flow" \
    "self.${wrapper}(iterable, plan, function)?;" \
    1 \
    "${wrapper} statement-dispatch calls"
  require_exact_line_count \
    "$wasm_emission_sites" \
    "            let _ = FunctionBuilder::${wrapper};" \
    1 \
    "${wrapper} obligation-ledger references"
  require_tree_regex_count \
    crates/scriptw-aot-wasm/src \
    "$wrapper" \
    3 \
    "${wrapper} owner, dispatch and obligation-ledger sites"
done
if grep -Eq 'reserve_temp_local|emit_get_iterator|emit_sync_iterator_step_value|emit_iterator_close|compile_(async|generator)_statement_sequence|finally_stack' "$wasm_async_function_for_of_iterator"; then
  fail "$wasm_async_function_for_of_iterator must remain typed delegation without an inline iterator body"
fi

# The complete typed owner retains the Iterator Record through suspension and
# keeps the pending completion intact across close and activation dispatch.
# Digest pins and raw completion/iterator-local recipes belonged to the retired ABI.
resumable_sync_for_of_owner="$(braced_rust_item_source "$wasm_resumable_sync_for_of_iterator" '^pub[(]crate[)][[:space:]]+fn[[:space:]]+compile_resumable_sync_for_of_iterator[[:space:]]*[(]')"
if ! awk '
  index($0, "self.emit_get_sync_iterator(") && !acquire { acquire = NR }
  index($0, "let loop_frame = self.open_frame") && !loop { loop = NR }
  index($0, "self.emit_sync_iterator_step_value(") && !step { step = NR }
  index($0, "self.breakable_stack.push(break_frame);") && !break_publish { break_publish = NR }
  index($0, "self.loop_stack.push(LoopTargets { continue_frame });") && !continue_publish { continue_publish = NR }
  index($0, "self.finally_stack.push(close_frame);") && !close_publish { close_publish = NR }
  index($0, "self.compile_resumable_statement_sequence(") && !body { body = NR }
  index($0, "CompletionKind::Continue.code()") && !continue_kind { continue_kind = NR }
  index($0, "self.set_completion_kind(CompletionKind::Normal, function);") && body && !continue_consume { continue_consume = NR }
  index($0, "self.loop_stack.pop();") && !continue_retire { continue_retire = NR }
  index($0, "pending.copy_from(self.completion(), function);") && !save { save = NR }
  index($0, "self.emit_sync_iterator_close(") && !iterator_close { iterator_close = NR }
  index($0, "self.completion().copy_from(&closed, function);") && !restore { restore = NR }
  index($0, "owner.emit_dispatch_completion(self, function)?;") && !dispatch { dispatch = NR }
  index($0, "self.breakable_stack.pop();") && !break_retire { break_retire = NR }
  /^[[:space:]]*iterator[.]clear[(]function[)];[[:space:]]*$/ && !release { release = NR }
  END {
    exit !(acquire && acquire < loop && loop < step && step < break_publish \
      && break_publish < continue_publish && continue_publish < close_publish \
      && close_publish < body && body < continue_kind && continue_kind < continue_consume \
      && continue_consume < continue_retire && continue_retire < save && save < iterator_close \
      && iterator_close < restore && restore < dispatch && dispatch < break_retire && break_retire < release)
  }
' <<<"$resumable_sync_for_of_owner"; then
  fail "$wasm_resumable_sync_for_of_iterator must preserve typed acquire, step, local target lifetime, whole close and dispatch order"
fi
require_file crates/scriptw-engine/tests/aot_generators/aot_generator_for_of_continuations.rs
require_file crates/scriptw-engine/tests/fixtures/generator_for_of_continuations/iterator-close-completion-precedence.js
require_file crates/scriptw-engine/tests/fixtures/generator_for_of_continuations/assignment-head-errors-close.js

# Both closed owners use the same reviewed target lifecycle. The GC parent now
# measures 378 raw lines and its consumed projection child 135. Preserve the
# parent's earlier 638-line ceiling; keep the child within its measured scope.
# The parent adds narrow checked whole-protocol dispatch, range/TDZ joins and
# committed completion routing; iterator/loop/resource algorithms stay children.
check_raw_line_budget "$wasm_control_flow" 9800
check_raw_line_budget "$wasm_async_function_for_of_iterator" 60
# The opaque paired projection and body owner measures 150 raw lines.
check_raw_line_budget "$wasm_for_await_iterator_plan" 170
check_raw_line_budget "$wasm_resumable_sync_for_of_iterator" 638
check_raw_line_budget "$wasm_resumable_sync_for_of_plan" 150

# T05's central GC schema owns raw struct/array instruction encoding.
# Typed construction, fields and complete values are implemented by its private
# layouts/value children; builtin-local encoders would bypass those types.
wasm_gc_types="crates/scriptw-aot-wasm/src/gc_types.rs"
wasm_gc_layouts="crates/scriptw-aot-wasm/src/gc_types/layouts.rs"
wasm_gc_values="crates/scriptw-aot-wasm/src/gc_types/value.rs"
require_file "$wasm_gc_layouts"
require_file "$wasm_gc_values"
require_exact_line_count "$wasm_gc_types" 'mod value;' 1 'private whole GC value module'
check_no_inline_legacy_includes "$wasm_gc_values"
require_exact_line_count "$wasm_gc_types" 'mod layouts;' 1 'private typed GC layout module'
check_no_inline_legacy_includes "$wasm_gc_layouts"
compiled_module_package="crates/scriptw-aot-wasm/src/module/compiled_module_package.rs"
require_file "$compiled_module_package"
require_exact_line_count \
  crates/scriptw-aot-wasm/src/module.rs \
  'mod compiled_module_package;' \
  1 \
  'private compiled-module-package owner declarations'
if grep -Eq '^pub(\(crate\))? mod compiled_module_package;' crates/scriptw-aot-wasm/src/module.rs; then
  fail 'the compiled-module-package owner must remain private'
fi
compiled_package_reexport="$(
  sed -n '/^pub(crate) use compiled_module_package::{/,/^};/p' crates/scriptw-aot-wasm/src/module.rs
)"
for surface in ModuleAssemblySections ModuleGlobalSectionBuilder ModuleTypeRegistry; do
  require_text_regex_count "$compiled_package_reexport" "[ ,]${surface}[, ]" 1 "compiled-module-package ${surface} re-exports"
done
for private_state in FinalizedModuleSections CompiledModulePackage CallableFunctionTableSections; do
  if printf '%s\n' "$compiled_package_reexport" | grep -Fq "$private_state"; then
    fail "$private_state must remain private to $compiled_module_package"
  fi
done
for sole_owner in ModuleTypeRegistry FinalizedModuleSections CompiledModulePackage \
                  CallableFunctionTableSections ModuleAssemblySections \
                  ModuleGlobalSectionBuilder; do
  require_fixed_string_count "$compiled_module_package" "struct ${sole_owner}" 1 "${sole_owner} owners"
  if sed '/^#\[cfg(test)\]/,$d' crates/scriptw-aot-wasm/src/module.rs \
    | grep -Fq "struct ${sole_owner}"; then
    fail "$sole_owner must be owned only by $compiled_module_package"
  fi
done
# Native template binary fixtures have exact statically test-only owners.
native_template_fixture_source="crates/scriptw-aot-wasm/src/native_program_template/tests.rs"
require_file "$native_template_fixture_source"
if [[ "$(head -n 1 "$native_template_fixture_source")" != '#![cfg(test)]' ]]; then
  fail 'native template binary fixtures must be compiled only for tests'
fi
native_template_literal_fixture_source="crates/scriptw-aot-wasm/src/native_program_template/tests/literal_operands.rs"
require_file "$native_template_literal_fixture_source"
if [[ "$(head -n 1 "$native_template_literal_fixture_source")" != '#![cfg(test)]' ]]; then
  fail 'native literal operand binary fixtures must be compiled only for tests'
fi
native_template_shared_body_fixture_source="crates/scriptw-aot-wasm/src/native_program_template/share_bodies/tests.rs"
require_file "$native_template_shared_body_fixture_source"
if [[ "$(head -n 1 "$native_template_shared_body_fixture_source")" != '#![cfg(test)]' ]]; then
  fail 'native shared-body binary fixtures must be compiled only for tests'
fi
native_template_signature_fixture_source="crates/scriptw-aot-wasm/src/native_program_template/unused_function_types/tests.rs"
require_file "$native_template_signature_fixture_source"
if [[ "$(head -n 1 "$native_template_signature_fixture_source")" != '#![cfg(test)]' ]]; then
  fail 'native signature binary fixtures must be compiled only for tests'
fi
gc_instruction_escapes="$(
  rg -n 'Instruction::(Struct(New|Get|Set)|Array(New|Get|Set|Fill|Copy|Len))' \
    crates/scriptw-aot-wasm/src --glob '*.rs' --glob '!**/gc_types/**' \
      --glob '!crates/scriptw-aot-wasm/src/native_program_template/tests.rs' \
      --glob '!crates/scriptw-aot-wasm/src/native_program_template/tests/program_owned_payload.rs' || true
)"
if [ -n "$gc_instruction_escapes" ]; then
  fail "raw Wasm-GC record/array instructions must stay in the central schema: $gc_instruction_escapes"
fi
require_fixed_string_count \
  "$wasm_gc_layouts" \
  'function.instruction(&Instruction::StructNew(' \
  1 \
  'typed StructNew encoder boundary'
require_fixed_string_count "$wasm_gc_layouts" 'GcStorageGet::Unpacked => Instruction::StructGet {' 1 'typed unpacked StructGet encoder boundary'
require_fixed_string_count "$wasm_gc_layouts" 'GcStorageGet::UnsignedPacked => Instruction::StructGetU {' 1 'typed packed StructGet encoder boundary'
require_fixed_string_count \
  "$wasm_gc_layouts" \
  'function.instruction(&Instruction::StructSet {' \
  1 \
  'typed mutable-field StructSet encoder boundary'
require_fixed_string_count "$compiled_module_package" 'registered: RuntimeModuleTypes::register(kind),' 1 'sole consumed ownership-aware callable/layout type registration'

# The consumed global finalizer declares each concrete root against the actual
# encoded section. Generic typed globals keep their index construction private.
if grep -R -Eq 'runtime_gc_root_global_index|fn bind_root\(' crates/scriptw-aot-wasm/src; then
  fail 'runtime GC roots must come from the consumed global section'
fi
gc_root_finalizer="$(braced_rust_item_source "$wasm_gc_types" '^fn finalize_globals[(]')"
gc_root_finalizer_code="$(printf '%s\n' "$gc_root_finalizer" | tr -d '[:space:]')"
gc_registered_finalizer="$(braced_rust_item_source "$wasm_gc_types" '^pub[(]crate[)] fn finalize_globals[(]')"
gc_registered_finalizer_code="$(printf '%s\n' "$gc_registered_finalizer" | tr -d '[:space:]')"
if [[ "$gc_root_finalizer_code" != *'self,mutglobals:GlobalLedger,snapshot:bool,module_guard_count:u32,)->FinalizedModuleGlobals{'* ]] \
  || [[ "$gc_root_finalizer_code" != *'snapshot.then(||snapshot::SnapshotRoots::declare(&self.layouts,&mutglobals))'* ]] \
  || [[ "$gc_root_finalizer_code" != *'FinalizedModuleGlobals{global_prelude_phase:None,section:globals,runtime_schema,}'* ]] \
  || [[ "$gc_registered_finalizer_code" != *'letmutglobals=self.runtime.finalize_globals(globals,snapshot,module_guard_count);'* ]] \
  || [[ "$gc_registered_finalizer_code" != *'ifself.global_prelude{globals.declare_global_prelude_phase();}'* ]] \
  || [[ "$gc_registered_finalizer_code" != *'FinalizedRuntimeModule{types:FinalizedModuleTypes{section:self.section,},globals,}'* ]] \
  || ! grep -Fq 'current_realm: GcRootGlobal<RealmRecord>,' "$wasm_gc_types" \
  || ! grep -Fq 'section: GlobalLedger,' "$wasm_gc_types"; then
  fail 'both finalization modes must consume the actual global ledger through one typed-root owner and seal its matching schema'
fi
require_text_regex_count "$gc_root_finalizer" 'GcRootGlobal::declare[(]&self.layouts, &mut globals[)]' 17 'declared semantic GC roots in the sole finalizer'
require_fixed_string_count "$wasm_gc_types" 'active_source_function: GcRootGlobal<FunctionObject>,' 1 'typed active source activation root'
require_text_regex_count "$gc_root_finalizer" 'CollectionKeyHashCounterGlobal::declare[(]&mut globals[)]' 1 'declared scalar collection identity root'
require_regex_count "$wasm_gc_types" '^[[:space:]]+fn finalize_globals[(]' 1 'sole concrete GC-root finalizer'
require_regex_count "$wasm_gc_types" '^[[:space:]]+pub[(]crate[)] fn finalize_globals[(]' 1 'sole registered type and global finalizer'
wasm_gc_snapshot="crates/scriptw-aot-wasm/src/gc_types/snapshot.rs"
require_file "$wasm_gc_snapshot"
require_exact_line_count "$wasm_gc_types" 'mod snapshot;' 1 'private declaration-derived snapshot projection owner'
gc_snapshot_roots="$(braced_rust_item_source "$wasm_gc_snapshot" '^pub[(]super[)] struct SnapshotRoots[[:space:]]*[{]')"
if grep -Eq '^[[:space:]]+pub([[:space:]]|[(])' <<<"$gc_snapshot_roots" \
  || ! grep -Fq 'entry: GcRootGlobal<RealmRecord>,' <<<"$gc_snapshot_roots" \
  || ! grep -Fq 'inventory: GcRootGlobal<SnapshotRealmInventory>,' <<<"$gc_snapshot_roots"; then
  fail 'opt-in snapshot roots must retain private typed Realm and inventory identities'
fi
gc_snapshot_declaration="$(braced_rust_item_source "$wasm_gc_snapshot" '^pub[(]super[)] fn declare[(]')"
for snapshot_root_invariant in \
  'let entry = GcRootGlobal::declare(layouts, globals);' \
  'let inventory = GcRootGlobal::declare(layouts, globals);' \
  'let index = globals.len();' \
  'layouts.reference(layout.layout(), GcNullability::Nullable)' \
  'mutable: false,' \
  '&ConstExpr::ref_null(reference.heap_type)'; do
  if ! grep -Fq "$snapshot_root_invariant" <<<"$gc_snapshot_declaration"; then
    fail "snapshot witnesses must derive immutable globals from their actual layout and section: $snapshot_root_invariant"
  fi
done
require_fixed_string_count "$wasm_gc_types" 'snapshot: Option<snapshot::SnapshotRoots>,' 1 'sealed optional snapshot root owner'
require_fixed_string_count "$wasm_gc_snapshot" 'let Some(roots) = &self.snapshot else {' 2 'opt-in export and Realm-retention gates'
require_fixed_string_count "$wasm_gc_snapshot" 'fn publish_snapshot_record(self, _record: GcStackReference<T>, function: &mut Function)' 1 'private typed inventory publication boundary'
require_fixed_string_count crates/scriptw-aot-wasm/src/functions.rs 'schema.retain_snapshot_realm(realm, function)' 1 'sole original Realm allocator retention consumer'
if grep -Eq 'pub\(crate\).*RuntimeModuleSchema|fn (runtime_schema|section)\(&self\).*GlobalSection' "$wasm_gc_types"; then
  fail "$wasm_gc_types must not expose a copyable runtime schema or the cloneable raw global section"
fi
gc_schema_escapes="$(
  find crates/scriptw-aot-wasm/src -type f -name '*.rs' ! -path "$wasm_gc_types" -print0 \
    | xargs -0 grep -Fn 'RuntimeModuleSchema' || true
)"
if [ -n "$gc_schema_escapes" ]; then
  fail "the private runtime GC schema must not escape $wasm_gc_types: $gc_schema_escapes"
fi
require_fixed_string_count "$compiled_module_package" 'runtime: self.registered.finalize_globals(' 1 'global-ledger finalization through the typed runtime registry'
require_fixed_string_count crates/scriptw-aot-wasm/src/emit/module_assembly.rs \
  'module_types.finalize_globals(globals, uses_heap, module_guard_count)' 1 'both module modes share one sealed global finalizer'
package_global_finalizer="$(braced_rust_item_source "$compiled_module_package" '^pub[(]crate[)] fn finalize_globals[(]')"
package_global_finalizer_code="$(printf '%s\n' "$package_global_finalizer" | tr -d '[:space:]')"
if [[ "$package_global_finalizer_code" != *'self,globals:ModuleGlobalSectionBuilder,snapshot_roots:bool,module_guard_count:u32,)->FinalizedModuleSections{'* ]] \
  || [[ "$package_global_finalizer_code" != *'runtime:self.registered.finalize_globals(globals.section,snapshot_roots,module_guard_count,)'* ]]; then
  fail 'global finalization must consume registered types and the actual ledger into the same sealed package, retaining snapshot and module-guard policies'
fi
require_fixed_string_count crates/scriptw-aot-wasm/src/emit/module_assembly.rs \
  'entry_exports.append(' 1 'package-owned typed entry export plan'
wasm_program_entries="crates/scriptw-aot-wasm/src/emit/module_assembly/program_entries.rs"
require_file "$wasm_program_entries"
require_exact_line_count crates/scriptw-aot-wasm/src/emit/module_assembly.rs 'mod program_entries;' 1 'private typed program-export and entry owner'
program_exports_append="$(braced_rust_item_source "$wasm_program_entries" '^pub[(]super[)] fn append[(]')"
program_exports_append_code="$(printf '%s\n' "$program_exports_append" | tr -d '[:space:]')"
program_exports_new="$(braced_rust_item_source "$wasm_program_entries" '^pub[(]super[)] fn new[(]')"
if [[ "$program_exports_append_code" != *'ifself.owns_globals{schema.export_throw_diagnostics(exports);schema.export_snapshot_roots(exports);}'* ]] \
  || ! grep -Fq 'owns_globals: kind.owns_globals(),' <<<"$program_exports_new"; then
  fail 'the typed ProgramExports plan must gate snapshot exports on actual module global ownership'
fi
require_fixed_string_count "$wasm_program_entries" 'schema.export_snapshot_roots(exports);' 1 'package-owned opt-in snapshot exports'
require_tree_regex_count crates/scriptw-aot-wasm/src '[.]export_snapshot_roots[(]' 1 'sole typed opt-in snapshot export consumer'
require_fixed_string_count crates/scriptw-aot-wasm/src/emit/module_assembly.rs 'export_snapshot_roots(' 0 'no copied snapshot export implementation in assembly'
program_exports_assembly_code="$(tr -d '[:space:]' < crates/scriptw-aot-wasm/src/emit/module_assembly.rs)"
if [[ "$program_exports_assembly_code" != *'entry_exports.append(module_package.schema(),module_package.global_prelude_phase(),script.module_entry_evaluation().is_some(),&mutexports,);'* ]] \
  || [[ "$program_exports_append_code" != *'schema:&RuntimeSchema,global_prelude:Option<crate::gc_types::GlobalScriptPreludePhase>,has_module_entry:bool,exports:&mutExportSection,'* ]] \
  || [[ "$program_exports_append_code" != *'ifletSome(main)=self.main{ifletSome(phase)=global_prelude{phase.export(exports);}'* ]]; then
  fail 'the typed ProgramExports plan must consume the same finalized module package schema'
fi
# Body compilation and entry roots are independent private owners, while the
# assembly owner consumes their planned bodies and the finalized module package.
wasm_emit_parent="crates/scriptw-aot-wasm/src/emit.rs"
wasm_body_compilation="crates/scriptw-aot-wasm/src/emit/body_compilation.rs"
wasm_body_entry="crates/scriptw-aot-wasm/src/emit/body_entry.rs"
wasm_module_assembly="crates/scriptw-aot-wasm/src/emit/module_assembly.rs"
for emitter_owner in body_compilation body_entry module_assembly; do
  emitter_owner_path="crates/scriptw-aot-wasm/src/emit/${emitter_owner}.rs"
  require_file "$emitter_owner_path"
  require_exact_line_count "$wasm_emit_parent" "mod ${emitter_owner};" 1 'private emitter owner attachment'
  if grep -Eq "^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+${emitter_owner};" "$wasm_emit_parent"; then
    fail "$wasm_emit_parent must keep ${emitter_owner} private"
  fi
  check_no_inline_legacy_includes "$emitter_owner_path"
done
require_regex_count "$wasm_emit_parent" "^impl<'a> FunctionBuilder<'a>[[:space:]]*[{]" 1 'single parent FunctionBuilder implementation to audit'
parent_function_builder="$(braced_rust_item_source "$wasm_emit_parent" "^impl<'a> FunctionBuilder<'a>[[:space:]]*[{]")"
for body_compiler in compile_callable compile_builtin_callable compile; do
  require_fixed_string_count "$wasm_body_compilation" "pub(super) fn ${body_compiler}(" 1 'private planned-body compiler entry'
  require_text_regex_count "$parent_function_builder" "fn[[:space:]]+${body_compiler}[(]" 0 'no parent FunctionBuilder body compiler copy'
done
require_fixed_string_count "$wasm_body_compilation" 'fn compile_builtin(' 1 'private native body implementation'
require_fixed_string_count "$wasm_module_assembly" 'builder.compile_callable()?' 2 'source and prepared callable compilation consumers'
require_fixed_string_count "$wasm_module_assembly" 'builder.compile_builtin_callable()?' 2 'standard and host callable compilation consumers'
# Runtime modules compile every native body through the same private compiler;
# imported program functions have no source-shaped or placeholder stub body.
require_fixed_string_count "$wasm_module_assembly" 'StandardBuiltinId::all_functions().to_vec()' 1 'complete runtime standard-builtin inventory'
require_fixed_string_count "$wasm_module_assembly" 'HostBuiltinId::ALL.to_vec()' 1 'complete runtime host-builtin inventory'
for body_owner in "$wasm_emit_parent" "$wasm_body_compilation" "$wasm_module_assembly"; do
  require_fixed_string_count "$body_owner" 'compile_shared_stub_callable' 0 'no retired native stub body path'
done
require_fixed_string_count "$wasm_body_entry" 'pub(crate) fn begin_helper_body(' 1 'registered helper body entry owner'
require_fixed_string_count "$wasm_emit_parent" 'fn begin_helper_body(' 0 'no parent helper entry copy'
require_fixed_string_count "$wasm_body_entry" 'FunctionModuleState::RuntimeOperation(actual) if actual == helper' 1 'helper entry matches its registered body'
require_fixed_string_count "$wasm_body_compilation" 'self.clear_body_entry_roots(&mut function);' 2 'source and native root retirement consumers'
require_fixed_string_count "$wasm_program_entries" 'pub(crate) fn emit_script_module(' 1 'canonical physical module assembly entry'
require_exact_line_count "$wasm_module_assembly" 'pub(super) use program_entries::emit_script_module;' 1 'private assembly entry re-export'
require_fixed_string_count "$wasm_module_assembly" 'fn emit_script_module(' 0 'no assembly entry implementation copy'
require_tree_regex_count crates/scriptw-aot-wasm/src '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+emit_script_module[[:space:]]*[(]' 1 'sole physical module assembly entry owner'
require_fixed_string_count "$wasm_emit_parent" 'module_assembly::emit_script_module(' 2 'runtime and program assembly consumers'
require_fixed_string_count "$wasm_emit_parent" 'fn emit_script_module(' 0 'no parent assembly implementation copy'
program_module_emission="$(braced_rust_item_source "$wasm_emit_parent" '^fn emit_script[(]')"
runtime_module_emission="$(braced_rust_item_source "$wasm_emit_parent" '^pub[(]crate[)] fn emit_runtime_module[(]')"
require_text_regex_count "$program_module_emission" 'module_assembly::emit_script_module[(]' 1 'program or standalone assembly consumer'
require_text_regex_count "$program_module_emission" 'Some[(]runtime[)] => ModuleKind::Program[(]runtime[)]' 1 'heap program links its selected runtime'
require_text_regex_count "$program_module_emission" 'None => ModuleKind::Standalone' 1 'heap-free standalone module selection'
require_text_regex_count "$runtime_module_emission" 'module_assembly::emit_script_module[(]' 1 'runtime assembly consumer'
require_text_regex_count "$runtime_module_emission" 'ModuleKind::Runtime\(source_compilation_profile\),' 1 'runtime caller selects its explicit source-compilation profile'
# The callable builder now carries typed resume-environment/resource authority
# and sparse Intl frame selection. Entry owns the per-point ancestry selection,
# retaining the original saved-chain path for uncertified linear points.
check_raw_line_budget "$wasm_emit_parent" 2320
require_module_decl "$wasm_emit_parent" async_generator_admission
check_raw_line_budget crates/scriptw-aot-wasm/src/emit/async_generator_admission.rs 550
check_raw_line_budget "$wasm_body_compilation" 550
check_raw_line_budget "$wasm_body_entry" 600
wasm_literal_roots="crates/scriptw-aot-wasm/src/emit/body_entry/literal_roots.rs"
require_file "$wasm_literal_roots"
require_exact_line_count "$wasm_body_entry" 'mod literal_roots;' 1 'private literal-root initialization owner'
wasm_runtime_string_roots="crates/scriptw-aot-wasm/src/emit/body_entry/literal_roots/runtime_strings.rs"
require_file "$wasm_runtime_string_roots"
require_exact_line_count "$wasm_literal_roots" 'mod runtime_strings;' 1 'private R string algorithm owner'
require_fixed_string_count "$wasm_literal_roots" 'fn initialize_runtime_literal_roots(' 1 'literal-root runtime algorithm owner'
for literal_root_entry in compile_pooled_strings_initialize_helper; do
  require_fixed_string_count "$wasm_runtime_string_roots" "fn ${literal_root_entry}(" 1 'literal-root R string algorithm owner'
done
for literal_root_entry in initialize_runtime_literal_roots compile_pooled_strings_initialize_helper; do
  require_fixed_string_count "$wasm_body_entry" "fn ${literal_root_entry}(" 0 'no parent literal-root algorithm copy'
  require_tree_regex_count crates/scriptw-aot-wasm/src "^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+${literal_root_entry}[[:space:]]*[(]" 1 'sole canonical literal-root algorithm owner'
done
require_tree_regex_count crates/scriptw-aot-wasm/src 'fn[[:space:]]+initialize_gc_literal_roots[[:space:]]*[(]' 0 'no retired combined R/P literal-root entry'
require_tree_regex_count crates/scriptw-aot-wasm/src 'fn[[:space:]]+emit_pooled_string_slot[[:space:]]*[(]' 0 'no unrolled per-string runtime initialization owner'
require_fixed_string_count "$wasm_runtime_string_roots" 'pub(crate) fn compile_pooled_strings_initialize_helper(' 1 'registered runtime helper implementation visibility'
for literal_root_entry in produce_program_literal_roots emit_pooled_string_reference; do
  require_fixed_string_count "$wasm_literal_roots" "fn ${literal_root_entry}" 1 'actual literal-root algorithm owner'
  require_fixed_string_count "$wasm_body_entry" "fn ${literal_root_entry}" 0 'no parent literal-root algorithm copy'
done
for runtime_string_entry in emit_pooled_string_slot compile_pooled_strings_initialize_helper; do
  require_fixed_string_count "$wasm_literal_roots" "fn ${runtime_string_entry}(" 0 'no copied R string algorithm in literal owner'
done
require_fixed_string_count "$wasm_literal_roots" 'pub(in crate::emit) fn produce_program_literal_roots' 1 'one fresh P producer inside emission'
require_fixed_string_count "$wasm_literal_roots" 'PooledStringsInitializeArguments::new()' 1 'R bootstrap has no implicit or dummy P operand'
check_no_inline_legacy_includes "$wasm_runtime_string_roots"
require_fixed_string_count "$wasm_literal_roots" 'pub(in crate::emit) fn initialize_runtime_literal_roots(' 1 'literal-root entry stays inside emission'
wasm_program_initialize="crates/scriptw-aot-wasm/src/emit/body_compilation/program_initialize.rs"
require_file "$wasm_program_initialize"
require_exact_line_count "$wasm_body_compilation" 'mod program_initialize;' 1 'private typed P bootstrap compiler'
program_initialize_compiler="$(braced_rust_item_source "$wasm_program_initialize" '^pub[(]crate[)] fn compile[(]')"
require_text_regex_count "$program_initialize_compiler" 'builder[.]initialize_runtime_literal_roots[(]&mut function[)][?];' 1 'typed P bootstrap consumes the canonical R literal-root initializer'
require_text_regex_count "$program_initialize_compiler" 'builder[.]produce_program_literal_roots[(]self.image, &mut function[)][?];' 1 'typed P bootstrap consumes its checked P literal image'
require_fixed_string_count "$wasm_body_compilation" 'initialize_gc_literal_roots(' 0 'no retired source-body literal initialization bypass'
require_fixed_string_count crates/scriptw-aot-wasm/src/emit/body_compilation/program_initialize.rs 'builder.initialize_runtime_literal_roots(&mut function)?;' 1 'pure bootstrap owns R-only initialization'
require_fixed_string_count crates/scriptw-aot-wasm/src/emit/body_compilation/program_initialize.rs 'builder.produce_program_literal_roots(' 1 'pure bootstrap owns unconditional P publication'
source_body_compiler="$(braced_rust_item_source "$wasm_body_compilation" '^fn compile_javascript_body[(]')"
source_body_compiler_code="$(printf '%s\n' "$source_body_compiler" | tr -d '[:space:]')"
if [[ "$source_body_compiler_code" != *'ifself.is_main(){letindex=initializer.expect('* ]] \
  || [[ "$source_body_compiler_code" != *'letprogram=self.runtime_schema().call_program_initialize(index,&mutfunction);self.bind_entry_program(program,&mutfunction);'* ]]; then
  fail 'heap source main must bind the P returned by its typed initializer before executing source'
fi
require_fixed_string_count "$wasm_module_assembly" 'compile_pooled_strings_initialize_helper' 1 'runtime assembly consumes the canonical pooled-string helper'
module_assembly_code="$(tr -d '[:space:]' < "$wasm_module_assembly")"
if [[ "$module_assembly_code" != *'register_runtime_helper!(PooledStringsInitialize,compile_pooled_strings_initialize_helper);'* ]]; then
  fail 'the pooled-string body owner must compile the registered PooledStringsInitialize helper'
fi
check_no_inline_legacy_includes "$wasm_literal_roots"
check_raw_line_budget "$wasm_literal_roots" 180
wasm_program_string_roots="crates/scriptw-aot-wasm/src/emit/body_entry/literal_roots/program_strings.rs"
wasm_program_string_descriptors="crates/scriptw-aot-wasm/src/data/program_string_descriptors.rs"
wasm_gc_program_string_descriptors="crates/scriptw-aot-wasm/src/gc_types/value/program_string_descriptors.rs"
require_module_decl "$wasm_literal_roots" program_strings
require_module_decl crates/scriptw-aot-wasm/src/data.rs program_string_descriptors
require_module_decl crates/scriptw-aot-wasm/src/gc_types/value.rs program_string_descriptors
for program_string_owner in "$wasm_program_string_roots" "$wasm_program_string_descriptors" "$wasm_gc_program_string_descriptors"; do
  require_file "$program_string_owner"
  check_no_inline_legacy_includes "$program_string_owner"
done
require_fixed_string_count "$wasm_literal_roots" 'self.initialize_program_string_roots(image, function);' 1 'checked P-image string bootstrap consumer'
program_literal_producer="$(braced_rust_item_source "$wasm_literal_roots" '^pub[(]in crate::emit[)] fn produce_program_literal_roots')"
require_text_regex_count "$program_literal_producer" 'image[.]require_pool[(]self.strings[)][?];' 1 'P literal producer admits its matching pool image'
require_text_regex_count "$program_literal_producer" 'self[.]initialize_program_string_roots[(]image, function[)];' 1 'bounded program-string bootstrap consumer'
program_string_initializer="$(braced_rust_item_source "$wasm_program_string_roots" '^pub[(]super[)] fn initialize_program_string_roots')"
require_text_regex_count "$program_string_initializer" 'function[.]instruction[(]&Instruction::Loop[(]BlockType::Empty[)][)];' 1 'one bounded descriptor-loop bootstrap'
require_text_regex_count "$program_string_initializer" 'ProgramLiteralRoots::admit_complete_table[(]schema, image, pool, function[)]' 1 'image-owned completed literal table result'
require_tree_regex_count crates/scriptw-aot-wasm/src/emit/body_entry/literal_roots 'program_pooled_string_initializers[(][)]' 0 'no unrolled program-string bootstrap in child owners'
require_fixed_string_count "$wasm_literal_roots" 'program_pooled_string_initializers()' 0 'no unrolled program-string bootstrap'
check_raw_line_budget "$wasm_program_string_roots" 100
check_raw_line_budget "$wasm_program_string_descriptors" 180
check_raw_line_budget "$wasm_gc_program_string_descriptors" 180
# New native/ordinary entry and typed export planning live in program_entries;
# the existing assembly owner's 1950-line cap remains unchanged.
check_raw_line_budget "$wasm_module_assembly" 1950
wasm_module_metadata="crates/scriptw-aot-wasm/src/emit/module_assembly/metadata.rs"
require_file "$wasm_module_metadata"
require_exact_line_count "$wasm_module_assembly" 'mod metadata;' 1 'private completed-function metadata owner'
for metadata_entry in append_function_attribution append_function_name_and_check_budget append_memory_attribution append_module_declarations; do
  require_fixed_string_count "$wasm_module_metadata" "pub(super) fn ${metadata_entry}(" 1 'private completed-function metadata entry'
  require_fixed_string_count "$wasm_module_assembly" "metadata::${metadata_entry}(" 1 'assembly consumes each metadata phase once'
  require_fixed_string_count "$wasm_module_assembly" "fn ${metadata_entry}(" 0 'no parent metadata implementation copy'
done
require_fixed_string_count "$wasm_module_metadata" 'let function_sizes = function_table.summaries();' 1 'metadata derives summaries from the completed function table'
require_fixed_string_count "$wasm_module_metadata" 'module.section(&function_table.name_section());' 1 'function names use the same completed table'
require_fixed_string_count "$wasm_module_metadata" 'function_table.check_budget(budget)?;' 1 'body budgets consume the same completed table'
require_fixed_string_count "$wasm_module_assembly" 'function_table.summaries()' 0 'no second summary producer in assembly'
check_no_inline_legacy_includes "$wasm_module_metadata"
check_raw_line_budget "$wasm_module_metadata" 100
package_main_compiler="$(braced_rust_item_source "$compiled_module_package" '^pub[(]crate[)] fn compile_main[(]')"
package_main_compiler_code="$(printf '%s\n' "$package_main_compiler" | tr -d '[:space:]')"
package_assembly="$(braced_rust_item_source "$compiled_module_package" '^pub[(]crate[)] fn append_to_module[(]')"
package_assembly_code="$(printf '%s\n' "$package_assembly" | tr -d '[:space:]')"
if ! grep -Fq "Main(&'a FinalizedModuleGlobals, PromiseRejectionPolicy)" "$wasm_emit_parent" \
  || ! grep -Fq 'module_package.compile_main(MainFunctionCompilation::new(' "$wasm_module_assembly" \
  || [[ "$package_main_compiler_code" != *"&mutself,compilation:MainFunctionCompilation<'_>,)->Result<(),EmitError>"* ]] \
  || [[ "$package_main_compiler_code" != *'letinitializer=self.program_initialize.as_ref().map(|(index,_)|*index);let(main,emitted_local_count)=compilation.compile(self.runtime.globals(),initializer)?;self.main=Some(main);'* ]] \
  || [[ "$package_assembly_code" != *'ifletSome(main)=main{code.push(main);}ifletSome((_,initialize))=program_initialize{code.push(initialize);}forfunctioninprogram_functions{code.push(function);}let(functions,code,function_table)=code.finish();'* ]] \
  || ! grep -Fq 'CompiledModulePackage::compile_main;' "$compiled_module_package" \
  || ! grep -Fq 'CompiledModulePackage::append_functions;' "$compiled_module_package"; then
  fail 'main must compile against its own finalized globals and stay in that package until ordered code publication'
fi
compiled_package_state="$(braced_rust_item_source "$compiled_module_package" '^pub[(]crate[)] struct CompiledModulePackage[[:space:]]*[{]')"
require_text_regex_count "$compiled_package_state" '^    main: Option<EmittedFunction>,$' 1 'privately retained compiled main'
require_text_regex_count "$compiled_package_state" '^    program_functions: Vec<EmittedFunction>,$' 1 'privately retained post-main program bodies'
require_text_regex_count "$compiled_package_state" '^    program_initialize: Option<[(]$' 1 'privately retained typed bootstrap index/body pair'
require_fixed_string_count "$compiled_module_package" 'let completed = compilation.compile(self.runtime.globals())?;' 1 'bootstrap compiles against its package-owned schema'
require_fixed_string_count "$compiled_module_package" 'CompiledModulePackage::compile_program_initialize;' 1 'typed bootstrap lifecycle gate'
require_text_regex_count "$compiled_package_state" '^[[:space:]]+pub([[:space:]]|[(])' 0 'no public replaceable module-package state'
compiled_package_code="$(tr -d '[:space:]' < "$compiled_module_package")"
if [[ "$compiled_package_code" != *"const_:fn(&mutCompiledModulePackage,MainFunctionCompilation<'_>)->Result<(),EmitError>=CompiledModulePackage::compile_main;"* ]] \
  || [[ "$compiled_package_code" != *'const_:fn(&mutCompiledModulePackage,Vec<EmittedFunction>,Vec<EmittedFunction>)=CompiledModulePackage::append_functions;'* ]]; then
  fail 'compile-time lifecycle signatures must retain main and keep runtime/program body groups separate'
fi
for rejected_surface in runtime_globals push_main_to append_types_to append_globals_to; do
  if grep -Fq "${rejected_surface}(" "$compiled_module_package"; then
    fail "the finalized module package must not expose split assembly surface: ${rejected_surface}"
  fi
done
if grep -Fq 'impl FnOnce(&FinalizedModuleGlobals)' "$compiled_module_package"; then
  fail 'the finalized module package must use the closed main compiler, not an arbitrary callback'
fi
if grep -Fq 'CompilingModulePackage' "$compiled_module_package"; then
  fail 'main compilation must return the one compiled package, not an independently consumable code package'
fi
require_fixed_string_count \
  "$compiled_module_package" \
  'pub(crate) fn append_to_module(' \
  1 \
  'consume-once compiled-package assembly transition'
require_fixed_string_count \
  crates/scriptw-aot-wasm/src/emit/module_assembly.rs \
  'module_package.append_to_module(' \
  1 \
  'compiled-package assembly consumer'
# Cargo and product execution share the native configuration owner, with an
# explicit R-image/execution-mode pair. The only build-native loader is an
# embedded authority inside the existing cache factory.
engine_native_config="crates/scriptw-engine/src/wasmtime_config.rs"
engine_embedded_runtime="crates/scriptw-engine/src/embedded_runtime.rs"
engine_embedded_build="crates/scriptw-engine/src/embedded_runtime/build.rs"
engine_embedded_compile="crates/scriptw-engine/src/embedded_runtime/compile.rs"
engine_embedded_publication="crates/scriptw-engine/src/embedded_runtime/publication.rs"
engine_native_stencils="crates/scriptw-engine/src/cache/native.rs"
runtime_image_build="crates/scriptw-runtime-image/build.rs"
runtime_image_source="crates/scriptw-runtime-image/src/lib.rs"
runtime_image_metadata="crates/scriptw-runtime-image/src/metadata.rs"
runtime_image_objects="crates/scriptw-runtime-image/build/objects.rs"
wasm_build_package="crates/scriptw-aot-wasm/src/runtime_artifact/build_package.rs"
for native_owner in "$engine_native_config" "$engine_embedded_runtime" "$engine_embedded_build" "$engine_embedded_compile" "$engine_native_stencils" "$runtime_image_build" "$runtime_image_source" "$runtime_image_objects" crates/scriptw-runtime-image/Cargo.toml "$wasm_build_package"; do
  require_file "$native_owner"
done
require_exact_line_count crates/scriptw-engine/src/lib.rs 'mod wasmtime_config;' 1 'shared native configuration owner'
require_exact_line_count crates/scriptw-engine/src/lib.rs 'mod embedded_runtime;' 1 'private embedded native authority'
require_exact_line_count "$runtime_image_build" '#[path = "../scriptw-engine/src/wasmtime_config.rs"]' 1 'same Cargo configuration owner'
require_exact_line_count "$runtime_image_build" '#[path = "../scriptw-engine/src/wasmtime_policy.rs"]' 1 'same Cargo capability policy'
require_exact_line_count "$runtime_image_build" '#[path = "../scriptw-engine/src/embedded_runtime/build.rs"]' 1 'same checked raw runtime producer'
require_fixed_string_count "$runtime_image_build" 'native_runtime_build::write_bundle(' 1 'one image production call'
require_fixed_string_count crates/scriptw-engine/build.rs 'native_runtime_build::write_bundle(' 0 'Engine metadata build does not repeat image production'
engine_embedded_images="crates/scriptw-engine/src/embedded_runtime/images.rs"
require_file "$engine_embedded_images"
require_fixed_string_count "$engine_embedded_images" 'use scriptw_runtime_image as image;' 1 'Engine consumes only immutable image accessors'
require_fixed_string_count "$engine_embedded_runtime" 'Images::package_for_profile(source_compilation_profile)' 1 'raw candidate uses its exact source profile'
require_fixed_string_count "$engine_embedded_runtime" 'self.images.profile == runtime.source_compilation_profile()' 1 'native candidate rejects a foreign source profile'
require_fixed_string_count "$engine_embedded_runtime" 'include_bytes!' 0 'image bytes do not enter Engine initializers'
require_fixed_string_count "$runtime_image_source" '#[inline(never)]' 7 'seven non-inline image and Intl metadata accessors'
require_file "$runtime_image_metadata"
require_module_decl "$runtime_image_source" metadata
require_exact_line_count "$runtime_image_source" 'pub use metadata::{metadata, portable_metadata};' 1 'the two immutable receipt accessors'
require_fixed_string_count "$runtime_image_metadata" '#[inline(never)]' 2 'two non-inline receipt accessors'
require_fixed_string_count "$runtime_image_metadata" 'return unsafe { &SCRIPTW_RUNTIME_METADATA };' 1 'NativeHost receipt uses its exact linked array'
require_fixed_string_count "$runtime_image_metadata" 'return unsafe { &SCRIPTW_RUNTIME_PORTABLE_METADATA };' 1 'Portable receipt uses its exact linked array'
require_fixed_string_count "$runtime_image_metadata" 'include_bytes!' 0 'receipts do not add Rust data initializers'
check_raw_line_budget "$runtime_image_metadata" 30
require_fixed_string_count "$runtime_image_source" 'include_bytes!' 0 'no large Rust initializer in leaf metadata'
require_fixed_string_count "$runtime_image_build" 'objects::embed(&out, &target);' 1 'checked images become immutable linked data'
require_fixed_string_count "$runtime_image_objects" '"--emit=obj"' 1 'data compilation emits no Rust metadata'
require_fixed_string_count "$runtime_image_objects" '"-Cembed-bitcode=no"' 1 'data-only object contains no duplicate bitcode image'
require_fixed_string_count "$runtime_image_objects" '.compile("scriptw_runtime_images")' 1 'target archiver owns the complete linked image'
require_exact_line_count "$runtime_image_objects" 'const PARTS: [(&str, &str); 9] = [' 1 'exact image and receipt definition inventory'
for image_part in package native manifest metadata; do
  if [ "$image_part" = native ]; then
    require_fixed_string_count "$runtime_image_objects" 'scriptw-runtime-native.packed' 1 'one compressed NativeHost native storage definition'
    require_fixed_string_count "$runtime_image_objects" 'scriptw-runtime-portable-native.packed' 1 'one compressed Portable native storage definition'
  else
    require_fixed_string_count "$runtime_image_objects" "scriptw-runtime-${image_part}.bin" 1 'one raw NativeHost package/manifest/receipt definition'
    require_fixed_string_count "$runtime_image_objects" "scriptw-runtime-portable-${image_part}.bin" 1 'one raw data definition per Portable package/manifest/receipt'
  fi
done
require_exact_line_count crates/scriptw-aot-wasm/src/runtime_artifact.rs 'mod build_package;' 1 'private checked build-package owner'
require_fixed_string_count "$engine_native_config" 'pub(crate) fn base_config(' 1 'one shared native configuration constructor'
require_fixed_string_count "$engine_native_config" 'PRODUCT_WASMTIME_POLICY.configure(&mut config);' 1 'same required product capability configuration'
require_fixed_string_count "$engine_embedded_build" 'manifest::COMPILATION_MODES.runtime' 1 'build R uses the admitted image compilation mode'
require_fixed_string_count "$engine_embedded_build" 'manifest::COMPILATION_MODES.register_allocation.algorithm()' 1 'immutable R uses its separately admitted register allocator'
require_fixed_string_count "$engine_native_config" 'config.cranelift_regalloc_algorithm(RegallocAlgorithm::SinglePass);' 1 'ordinary P retains the linear native compilation allocator'
require_fixed_string_count "$engine_embedded_runtime" 'manifest.modes != manifest::COMPILATION_MODES' 1 'native authority checks both image and execution modes'
require_fixed_string_count "$engine_embedded_build" '.target(target)' 1 'explicit Cargo target without host ISA inference'
require_fixed_string_count "$engine_embedded_build" 'config.parallel_compilation(false);' 1 'serial native build compilation'
require_fixed_string_count "$engine_embedded_build" '.compile(&runtime)' 1 'compile only the freshly checked raw owner'
require_fixed_string_count "$engine_embedded_build" 'let module_cache = if host == target {' 1 'native module cache is unavailable to cross-target builds'
require_fixed_string_count "$engine_embedded_build" 'build_cache::module_cache()' 1 'build-only module cache directory selection'
engine_embedded_build_cache="crates/scriptw-engine/src/embedded_runtime/build_cache.rs"
require_file "$engine_embedded_build_cache"
require_fixed_string_count "$engine_embedded_build_cache" 'crate::cache::module_cache()' 1 'unset build override retains the ordinary module cache'
require_fixed_string_count "$engine_embedded_build_cache" 'crate::cache::module_cache_at(' 1 'override reuses the existing bounded module cache owner'
require_fixed_string_count "$engine_embedded_build_cache" 'crate::cache::module_cache_limit_bytes()' 1 'build override preserves the existing budget'
require_fixed_string_count "$engine_embedded_build_cache" 'SCRIPTW_BUILD_MODULE_CACHE_DIR' 1 'one build-only cache directory override'
require_fixed_string_count "$runtime_image_build" '"SCRIPTW_BUILD_MODULE_CACHE_DIR"' 1 'Cargo tracks build cache directory changes'
check_raw_line_budget "$engine_embedded_build_cache" 40
require_fixed_string_count "$engine_embedded_compile" 'self.compile_wasm(runtime.wasm())' 1 'typed runtime admission reaches the native compiler'
require_fixed_string_count "$engine_embedded_compile" 'Module::new(&self.engine, wasm)?.serialize()' 1 'Wasmtime owns exact-Wasm native cache admission'
require_fixed_string_count "$engine_embedded_compile" 'self.engine.precompile_module(wasm)' 1 'cross-target and cache-unavailable compilation remains direct'
require_fixed_string_count "$engine_embedded_compile" 'Module::deserialize(' 0 'no custom native cache deserialization'
require_fixed_string_count crates/scriptw-engine/build.rs 'cargo:rustc-env=SCRIPTW_NATIVE_COMPILER_FINGERPRINT=' 1 'product native dependency identity is compiled in'
require_fixed_string_count "$runtime_image_build" 'compiler_fingerprint::native_fingerprint(&workspace)' 1 'build native dependency identity uses the same census'
require_fixed_string_count "$engine_native_config" 'ModuleVersionStrategy::Custom(' 1 'build and product serialization share the source-bound native version'
require_fixed_string_count "$engine_embedded_compile" 'configure_native_version(&mut config, native_identity);' 1 'build compiler always binds its actual native dependency identity'
require_fixed_string_count "$engine_embedded_build" 'crate::cache::NativeFunctionCache::new(' 1 'build stencils bind native dependencies'
require_fixed_string_count crates/scriptw-engine/src/lib.rs 'cache::NativeFunctionCache::new(' 1 'product stencils bind native dependencies'
engine_embedded_load="crates/scriptw-engine/src/embedded_runtime/load.rs"
require_file "$engine_embedded_load"
require_module_decl "$engine_embedded_runtime" load
require_fixed_string_count "$engine_embedded_load" 'Module::deserialize(engine, bytes)' 2 'both file-unavailable fallbacks use the admitted private Cargo-byte lease'
require_fixed_string_count "$engine_embedded_load" 'if !self.accepts_native(&lease)' 1 'both decoded profiles match their independent native manifest'
require_fixed_string_count "$engine_embedded_load" 'drop(lease);' 1 'native mapping releases the transient decoded-byte owner'
check_raw_line_budget "$engine_embedded_load" 120
require_fixed_string_count "$engine_embedded_runtime" 'OnceLock<Option<EmbeddedNativeRuntime>>' 2 'independent lazy admission per source profile'
require_fixed_string_count "$engine_embedded_runtime" 'OnceLock<Module>' 0 'no hidden retained native module'
wasm_build_envelope="crates/scriptw-aot-wasm/src/runtime_artifact/build_package/envelope.rs"
require_file "$wasm_build_envelope"
require_module_decl "$wasm_build_package" envelope
check_raw_line_budget "$wasm_build_envelope" 110
require_fixed_string_count "$wasm_build_envelope" 'cache::admit_payload(payload, identity, version, jobs, body_validation)?' 1 'canonical complete raw layout admission consumes checked version and explicit validation mode'
require_fixed_string_count "$wasm_build_envelope" 'CurrentRuntimeLayoutVersion::admit(u32::from_le_bytes(cache::take(&mut payload)?))?' 1 'build envelope admits actual current layout before raw payload'
require_fixed_string_count "$wasm_build_envelope" 'actual_digest.as_slice() != digest' 1 'raw build envelope retains complete checksum admission'
require_fixed_string_count "$wasm_build_envelope" 'runtime.key().as_bytes() == &expected_runtime' 2 'both raw build storage owners retain exact runtime key admission'
require_fixed_string_count crates/scriptw-engine/src/wasm_runtime_link.rs 'wasm_module_for_execution_with_factory(' 1 'typed embedded R uses the shared bounded retention owner'
linked_runtime_code="$(tr -d '[:space:]' < crates/scriptw-engine/src/wasm_runtime_link.rs)"
if [[ "$linked_runtime_code" != *'wasm_module_for_execution_with_factory(engine,source,policy,mode,||matchsource.runtime_image_policy()'* ]]; then
  fail 'typed embedded R must use its actual Engine, source, policy and mode in the shared retention factory'
fi
check_raw_line_budget "$engine_native_config" 100
check_raw_line_budget "$engine_embedded_runtime" 190
check_raw_line_budget "$engine_embedded_images" 45
check_raw_line_budget "$engine_embedded_build" 165
require_file "$engine_embedded_publication"
require_fixed_string_count "$engine_embedded_build" 'publication::publish(' 2 'publish unsupported-target and complete native bundles through one owner'
require_fixed_string_count "$engine_embedded_publication" 'output::write(&out.join(format!("{prefix}-{suffix}.bin")), bytes)'  1 'one atomic complete bundle writer'
require_fixed_string_count "$engine_embedded_publication" '("metadata", metadata),' 1 'each complete bundle includes its checked receipt'
check_raw_line_budget "$engine_embedded_publication" 40
check_raw_line_budget "$engine_embedded_compile" 100
check_raw_line_budget "$engine_native_stencils" 120
check_raw_line_budget "$runtime_image_build" 80
require_module_decl "$runtime_image_source" native_storage
require_module_decl "$runtime_image_source" native_lease
require_module_decl "$runtime_image_source" lease_tests
require_file crates/scriptw-runtime-image/src/native_lease.rs
require_file crates/scriptw-runtime-image/src/lease_tests.rs
require_fixed_string_count crates/scriptw-runtime-image/src/native_lease.rs 'Mutex<Result<Weak<NativeRuntimeBytes>, ()>>' 2 'each fixed profile has a weak cache with no strong decoded bytes'
check_raw_line_budget crates/scriptw-runtime-image/src/native_lease.rs 70
check_raw_line_budget crates/scriptw-runtime-image/src/lease_tests.rs 70
check_raw_line_budget "$runtime_image_source" 90
check_raw_line_budget "$runtime_image_objects" 120
check_raw_line_budget "$wasm_build_package" 230
require_module_decl "$wasm_build_package" inputs
require_file crates/scriptw-aot-wasm/src/runtime_artifact/build_package/inputs.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_artifact/build_package/inputs.rs 230
wasm_build_inputs="crates/scriptw-aot-wasm/src/runtime_artifact/build_package/inputs.rs"
wasm_build_admission="crates/scriptw-aot-wasm/src/runtime_artifact/build_package/inputs/admission.rs"
wasm_build_metadata="crates/scriptw-aot-wasm/src/runtime_artifact/build_package/metadata.rs"
require_module_decl "$wasm_build_inputs" admission
require_file "$wasm_build_admission"
check_raw_line_budget "$wasm_build_admission" 90
require_exact_line_count "$wasm_build_package" 'pub(super) mod metadata;' 1 'private immutable receipt owner'
require_file "$wasm_build_metadata"
check_raw_line_budget "$wasm_build_metadata" 150
for receipt_module in encode decode tests; do
  require_module_decl "$wasm_build_metadata" "$receipt_module"
  require_file "crates/scriptw-aot-wasm/src/runtime_artifact/build_package/metadata/${receipt_module}.rs"
done
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_artifact/build_package/metadata/encode.rs 130
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_artifact/build_package/metadata/decode.rs 190
check_raw_line_budget crates/scriptw-aot-wasm/src/runtime_artifact/build_package/metadata/tests.rs 220
# The cache damage controls deliberately construct foreign Wasm layouts. The
# file-level gate makes those constructors unavailable to production Rust,
# even if a future parent accidentally drops its own test-only module gate.
runtime_cache_fixture_source="crates/scriptw-aot-wasm/src/runtime_artifact/cache/tests.rs"
if [[ "$(head -n 1 "$runtime_cache_fixture_source")" != '#![cfg(test)]' ]]; then
  fail 'foreign runtime-cache fixture constructors must be compiled only for tests'
fi
# Source-profile metadata fixtures have one exact statically test-only owner.
runtime_source_profile_fixture_source="crates/scriptw-aot-wasm/src/runtime_artifact/source_compilation/tests.rs"
runtime_source_profile_fixture_parent="crates/scriptw-aot-wasm/src/runtime_artifact/source_compilation.rs"
require_file "$runtime_source_profile_fixture_source"
if [[ "$(head -n 1 "$runtime_source_profile_fixture_source")" != '#![cfg(test)]' ]]; then
  fail 'source-profile metadata fixtures must be compiled only for tests'
fi
if [[ "$(sed -n '/^#\[cfg(test)\]$/{N;p;}' "$runtime_source_profile_fixture_parent")" != $'#[cfg(test)]\nmod tests;' ]]; then
  fail 'source-profile metadata fixture parent must retain its exact test-only gate'
fi
runtime_validation_fixture_source="crates/scriptw-aot-wasm/src/runtime_artifact/validation/tests.rs"
if [[ "$(head -n 1 "$runtime_validation_fixture_source")" != '#![cfg(test)]' ]]; then
  fail 'runtime validation fixture constructors must be compiled only for tests'
fi
require_fixed_string_count crates/scriptw-aot-wasm/src/runtime_artifact/validation.rs '#[cfg(test)]' 1 'validation fixture parent retains its test-only gate'
require_module_decl crates/scriptw-aot-wasm/src/runtime_artifact/validation.rs tests
gc_type_registration_fixture_source="crates/scriptw-aot-wasm/src/gc_types/type_registration_tests.rs"
require_module_decl "$wasm_gc_types" type_registration_tests
if [[ "$(head -n 1 "$gc_type_registration_fixture_source")" != '#![cfg(test)]' ]]; then
  fail 'GC declaration fixtures must be compiled only for tests'
fi
for sealed_section in types globals code; do
  source_profile_fixture_exclusions=()
  if [ "$sealed_section" = types ]; then
    source_profile_fixture_exclusions=(-not -path "$runtime_source_profile_fixture_source")
  fi
  sealed_section_escapes="$(
    find crates/scriptw-aot-wasm/src -type f -name '*.rs' \
      ! -path "$compiled_module_package" \
      ! -path "$runtime_cache_fixture_source" \
      ! -path "$runtime_validation_fixture_source" \
      ! -path "$gc_type_registration_fixture_source" \
      ! -path "$native_template_fixture_source" \
      ! -path "$native_template_literal_fixture_source" \
      ! -path "$native_template_shared_body_fixture_source" \
      ! -path "$native_template_signature_fixture_source" \
      ! -path 'crates/scriptw-aot-wasm/src/native_program_template/tests/program_owned_payload.rs' \
      "${source_profile_fixture_exclusions[@]}" \
      ! -path 'crates/scriptw-aot-wasm/src/module.rs' -print0 \
      | xargs -0 grep -Fn "module.section(&${sealed_section})" || true
  )"
  module_parent_escape="$(
    sed '/^#\[cfg(test)\]/,$d' crates/scriptw-aot-wasm/src/module.rs \
      | grep -Fn "module.section(&${sealed_section})" || true
  )"
  if [ -n "$sealed_section_escapes" ] || [ -n "$module_parent_escape" ]; then
    fail "sealed runtime ${sealed_section} section escaped consume-once package assembly: ${sealed_section_escapes}"
  fi
done
global_section_constructor_escapes="$(
  find crates/scriptw-aot-wasm/src -type f -name '*.rs' \
    ! -path "$compiled_module_package" \
    ! -path "$runtime_cache_fixture_source" \
    ! -path "$native_template_fixture_source" \
      ! -path "$native_template_literal_fixture_source" \
      ! -path "$native_template_shared_body_fixture_source" \
      ! -path 'crates/scriptw-aot-wasm/src/native_program_template/tests/program_owned_payload.rs' \
    ! -path "$wasm_gc_types" -print0 \
    | xargs -0 grep -Fn 'GlobalSection::new()' || true
)"
if [ -n "$global_section_constructor_escapes" ] \
  || grep -Fq 'GlobalSection::new()' "$compiled_module_package"; then
  fail "production GlobalSection construction must stay in the canonical GC ledger encoder: $global_section_constructor_escapes"
fi
global_section_encoder="$(braced_rust_item_source "$wasm_gc_types" '^fn section_after[(]')"
require_text_regex_count "$global_section_encoder" 'GlobalSection::new[(][)]' 1 'sole raw encoder derived from the canonical global ledger'
gc_types_product_source="$(rust_source_without_inline_tests "$wasm_gc_types")"
require_text_regex_count "$gc_types_product_source" 'GlobalSection::new[(][)]' 1 'single production raw-global encoder site'
require_text_regex_count "$global_section_encoder" 'for [(]global_type, init[)] in &self[.]entries\[skip as usize[.][.]\]' 1 'defined globals preserve their actual declaration order'
require_text_regex_count "$global_section_encoder" 'section[.]global[(][*]global_type, init[)];' 1 'encoded globals use canonical type and initializer pairs'
global_section_view="$(braced_rust_item_source "$wasm_gc_types" '^pub[(]crate[)] fn defined_section[(]')"
global_section_view_code="$(printf '%s\n' "$global_section_view" | tr -d '[:space:]')"
if [[ "$global_section_view_code" != *"fndefined_section(&self,imported:u32)->implSection+'_"* ]]; then
  fail 'defined globals must expose only an opaque Section borrowed from their finalized owner'
fi
for global_view_invariant in \
  "structDefinedGlobalSection<'a>{ledger:&'aGlobalLedger,imported:u32,}" \
  "implEncodeforDefinedGlobalSection<'_>" \
  'self.ledger.section_after(self.imported).encode(sink);' \
  "implSectionforDefinedGlobalSection<'_>" \
  'DefinedGlobalSection{ledger:&self.section,imported,}'; do
  if [[ "$global_section_view_code" != *"$global_view_invariant"* ]]; then
    fail "the sealed global Section view must retain its original ledger and import boundary: $global_view_invariant"
  fi
done

for module in array atomics bigint binary_data boolean bootstrap date errors function \
              global_numeric host iterators json math number object proxy reflect \
              standard string symbol typed_array_set uri; do
  require_file "crates/scriptw-aot-wasm/src/builtins/${module}.rs"
  require_module_decl "$wasm_builtins_mod" "$module"
done

# Standard generator methods and Array/TypedArray iterator creation retain the
# exhaustive flat standard dispatch while their actual bodies have private owners.
for standard_owner_entry in \
  'generator:emit_generator_method_builtin:3:310' \
  'async_generator:emit_async_generator_method_builtin:3:390' \
  'array_iterator_creation:compile_array_iterator_method_builtin:6:110'
do
  standard_owner="${standard_owner_entry%%:*}"
  standard_owner_tail="${standard_owner_entry#*:}"
  standard_entry="${standard_owner_tail%%:*}"
  standard_owner_tail="${standard_owner_tail#*:}"
  standard_consumers="${standard_owner_tail%%:*}"
  standard_budget="${standard_owner_tail#*:}"
  standard_owner_path="crates/scriptw-aot-wasm/src/builtins/standard/${standard_owner}.rs"
  require_file "$standard_owner_path"
  require_exact_line_count "$wasm_standard_builtins" "mod ${standard_owner};" 1 'private standard helper owner attachment'
  if grep -Eq "^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+${standard_owner};" "$wasm_standard_builtins"; then
    fail "$wasm_standard_builtins must keep ${standard_owner} private"
  fi
  require_fixed_string_count "$standard_owner_path" "pub(super) fn ${standard_entry}(" 1 'consumed standard helper body'
  require_fixed_string_count "$wasm_standard_builtins" "fn ${standard_entry}(" 0 'no standard parent body copy'
  require_fixed_string_count "$wasm_standard_builtins" "self.${standard_entry}(" "$standard_consumers" 'flat standard dispatch consumers'
  check_no_inline_legacy_includes "$standard_owner_path"
  check_raw_line_budget "$standard_owner_path" "$standard_budget"
done
require_fixed_string_count crates/scriptw-aot-wasm/src/builtins/standard/generator.rs 'fn emit_generator_resume_call(' 1 'private synchronous generator resume owner'
require_fixed_string_count crates/scriptw-aot-wasm/src/builtins/standard/generator.rs 'self.emit_generator_resume_call(' 1 'sole synchronous generator resume consumer'
check_raw_line_budget "$wasm_standard_builtins" 3300

# Iterator.from acquisition and wrapper Call share one private consumed owner.
# The leaf measures 560 formatted lines; keep a bounded maintenance margin.
wasm_iterator_from_parent="crates/scriptw-aot-wasm/src/builtins/iterators.rs"
wasm_iterator_from_owner="crates/scriptw-aot-wasm/src/builtins/iterators/from.rs"
require_file "$wasm_iterator_from_owner"
require_exact_line_count "$wasm_iterator_from_parent" 'mod from;' 1 'private Iterator.from module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+from;' "$wasm_iterator_from_parent"; then
  fail "$wasm_iterator_from_parent must keep from private"
fi
for iterator_from_entry in \
  emit_iterator_from_builtin \
  emit_iterator_from_wrapper_next_builtin \
  emit_iterator_from_wrapper_return_builtin
do
  require_fixed_string_count "$wasm_iterator_from_owner" "pub(in crate::builtins) fn ${iterator_from_entry}(" 1 "Iterator.from leaf entry $iterator_from_entry"
  require_fixed_string_count crates/scriptw-aot-wasm/src/builtins/standard.rs "self.${iterator_from_entry}(function)?" 1 "live Iterator.from dispatch $iterator_from_entry"
done
check_no_inline_legacy_includes "$wasm_iterator_from_owner"
check_raw_line_budget "$wasm_iterator_from_owner" 600

wasm_builtin_bootstrap="crates/scriptw-aot-wasm/src/builtins/bootstrap.rs"
if ! grep -q 'match builtin\.intrinsic_installer()' "$wasm_builtin_bootstrap"; then
  fail "$wasm_builtin_bootstrap must dispatch through the catalog installer class"
fi
wasm_realm_initialization="crates/scriptw-aot-wasm/src/builtins/bootstrap/realm_initialization.rs"
require_file "$wasm_realm_initialization"
require_exact_line_count "$wasm_builtin_bootstrap" 'mod realm_initialization;' 1 'private shared Realm initialization helper owner'
require_fixed_string_count "$wasm_builtin_bootstrap" 'fn emit_initialize_realm_intrinsics_inner(' 1 'one ordered Realm initialization algorithm'
require_fixed_string_count "$wasm_realm_initialization" 'self.emit_initialize_realm_intrinsics_inner(' 1 'only the shared helper emits Realm initialization'
require_fixed_string_count "$wasm_builtin_bootstrap" 'self.emit_initialize_realm_intrinsics(' 2 'entry and created Realm consume the same helper'
require_fixed_string_count "$wasm_module_assembly" 'compile_realm_initialize_intrinsics_helper' 1 'registered Realm initialization body'
check_raw_line_budget "$wasm_realm_initialization" 130


# T02's Object, Proxy, Math, Symbol, BigInt, Boolean, Number, Function, Atomics,
# global numeric, URI, Error and JSON
# builtin body boundaries. The exhaustive StandardBuiltinId dispatch remains in
# standard.rs, but family bodies are one-line delegates so unrelated builtin
# work no longer collides with ~11k lines of Object descriptor/prototype
# implementation, the Proxy lifecycle, the Math emitter family, Symbol's
# registry/prototype implementation or BigInt's constructor, fixed-width and
# prototype implementation, Boolean's constructor and prototype receiver logic,
# Number's constructor, predicates and prototype methods, Function's constructor,
# four prototype methods and the typed callable dispatcher, the Error intrinsic
# family, the Atomics integer/wait family, or JSON's parse/stringify/raw-JSON
# wrappers. The two coercing global numeric predicates
# and the six global URI and Annex-B codec wrappers likewise stay out of the
# shared dispatcher.
check_no_inline_legacy_includes "$wasm_standard_builtins"
# Measured after the Atomics extraction: 30,567 raw lines before formatting.
# This margin is dispatch-maintenance headroom; substantive bodies belong in
# family modules.
check_raw_line_budget "$wasm_standard_builtins" 30800

# TypedArray.set owns brand/write admission and alias-aware ordered byte copies.
# The complete GC algorithm measures 695 lines, including private snapshots and
# whole completion cleanup; its three consumed family entries stay bounded.
wasm_typed_array_set="crates/scriptw-aot-wasm/src/builtins/typed_array_set.rs"
check_raw_line_budget "$wasm_typed_array_set" 730
for typed_set_entry in emit_typed_array_set_builtin emit_native_typed_array_byte_access emit_typed_array_set_byte_copy; do
  require_regex_count "$wasm_typed_array_set" "^[[:space:]]*pub\(in crate::builtins\)[[:space:]]+fn[[:space:]]+${typed_set_entry}[[:space:]]*\(" 1 'consumed TypedArray Set/byte owner'
  require_regex_count "$wasm_standard_builtins" "^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+${typed_set_entry}[[:space:]]*\(" 0 'no dispatcher copy of TypedArray Set/bytes'
done
require_fixed_string_count "$wasm_standard_builtins" 'self.emit_typed_array_set_builtin(function)?;' 1 'TypedArray Set fixed dispatch'
require_file crates/scriptw-engine/tests/aot_builtins/aot_typed_array_byte_copy.rs
require_file crates/scriptw-engine/tests/aot_gc_entries/aot_gc_typed_array_immutable_properties.rs

# Fixed Atomics entries consume the private operation and access proof owners.
# Async waiting has its own private GC queue/result lifecycle, not scalar locals.
wasm_atomics_builtins="crates/scriptw-aot-wasm/src/builtins/atomics.rs"
wasm_atomics_wait_async="crates/scriptw-aot-wasm/src/builtins/atomics/wait_async_result.rs"
check_no_inline_legacy_includes "$wasm_atomics_builtins"
require_file "$wasm_atomics_wait_async"
require_exact_line_count "$wasm_atomics_builtins" 'mod wait_async_result;' 1 'private Atomics async-wait owner'
for atomic_proof in AtomicsIntegerOperation PendingAtomicAccess RevalidatedAtomicAccess; do
  require_regex_count "$wasm_atomics_builtins" "^(enum|struct)[[:space:]]+${atomic_proof}[[:space:]]*\{" 1 'private Atomics operation/access authority'
  if grep -Eq "^pub(\([^)]*\))?[[:space:]]+(enum|struct)[[:space:]]+${atomic_proof}" "$wasm_atomics_builtins"; then
    fail "$atomic_proof must stay private to its complete Atomics owner"
  fi
done
if grep -Eq 'AtomicsIntegerOperation|PendingAtomicAccess|RevalidatedAtomicAccess' "$wasm_standard_builtins"; then
  fail 'the Standard dispatcher must use fixed Atomics entries, not private access policy'
fi
for atomics_builtin in add and compare_exchange exchange is_lock_free load notify or pause store sub wait wait_async xor; do
  require_regex_count "$wasm_atomics_builtins" "^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+emit_atomics_${atomics_builtin}_builtin[[:space:]]*\(" 1 'fixed Atomics entry'
  require_fixed_string_count "$wasm_standard_builtins" "self.emit_atomics_${atomics_builtin}_builtin(function)?" 1 "fixed Atomics ${atomics_builtin} route"
done
require_fixed_string_count "$wasm_atomics_builtins" 'pub(super) const ATOMICS_PUBLICATION_ORDER: [StandardBuiltinId; 14]' 1 'ordered Atomics publication surface'
require_fixed_string_count "$wasm_builtin_bootstrap" 'for builtin in ATOMICS_PUBLICATION_ORDER' 1 'sole shared Realm Atomics publication loop'
for atomics_wait_hook in emit_drain_atomics_wait_async_timeouts emit_poll_atomics_wait_async_timeouts; do
  require_regex_count "$wasm_atomics_wait_async" "^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+${atomics_wait_hook}[[:space:]]*\(" 1 'consumed async-wait checkpoint owner'
  require_regex_count "$wasm_atomics_builtins" "^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+${atomics_wait_hook}[[:space:]]*\(" 0 'no parent async-wait checkpoint copy'
done
require_fixed_string_count crates/scriptw-aot-wasm/src/emit/body_compilation.rs 'self.emit_drain_atomics_wait_async_timeouts(' 1 'Main event-loop actual async-wait drain consumer'
# Complete Generator, Async and AsyncGenerator phases consume one physical
# classic-loop lifecycle; the prior linear split remains a separate source owner.
wasm_generator_loop="crates/scriptw-aot-wasm/src/control_flow/generator_loop.rs"
require_file "$wasm_generator_loop"
require_exact_line_count crates/scriptw-aot-wasm/src/control_flow.rs 'mod generator_loop;' 1 'private ordinary generator loop owner attachment'
require_exact_line_count crates/scriptw-ir/src/lib.rs 'mod generator_loop_control;' 1 'checked ordinary generator phase domain attachment'
for generator_phase_entry in compile_ordinary_generator_loop compile_ordinary_generator_if; do
  require_fixed_string_count "$wasm_generator_loop" "pub(super) fn ${generator_phase_entry}(" 1 'actual checked ordinary phase compiler'
  require_fixed_string_count crates/scriptw-aot-wasm/src/control_flow.rs "fn ${generator_phase_entry}(" 0 'no ordinary phase implementation in parent'
done
require_fixed_string_count crates/scriptw-aot-wasm/src/control_flow.rs 'self.compile_ordinary_generator_loop(plan,' 2 'ordinary and labelled loop dispatch consumers'
require_fixed_string_count crates/scriptw-aot-wasm/src/control_flow.rs 'self.compile_ordinary_generator_if(plan,' 1 'ordinary conditional dispatch consumer'
require_fixed_string_count crates/scriptw-aot-wasm/src/control_flow.rs 'fn compile_plain_generator_loop(' 0 'retired single-yield ordinary loop implementation'
require_file crates/scriptw-engine/tests/aot_generators/aot_generator_classic_loops.rs
require_file crates/scriptw-engine/tests/fixtures/generator_classic_loops/phases_and_branches.js
require_file crates/scriptw-engine/tests/fixtures/generator_classic_loops/completions_and_environments.js
check_no_inline_legacy_includes "$wasm_generator_loop"
for shared_loop_entry in compile_resumable_generator_loop compile_resumable_generator_loop_phases; do
  require_tree_regex_count crates/scriptw-aot-wasm/src "^[[:space:]]*fn[[:space:]]+${shared_loop_entry}[[:space:]]*\\(" 1 'sole physical complete classic-loop lifecycle'
done
for complete_loop_protocol in Generator Async AsyncGenerator; do
  require_fixed_string_count "$wasm_generator_loop" "ResumableRegionProtocolIr::${complete_loop_protocol} => FunctionExecutionKind::${complete_loop_protocol}," 1 'closed checked loop protocol projection'
done
require_fixed_string_count "$wasm_generator_loop" 'Some(CheckedAsyncGeneratorEnvironmentOwner::for_loop(plan));' 1 'only the actual opaque loop mints region environment authority'
# Its bounded facade now owns three-protocol regions, source-certified scope
# tokens, resource-head lifetime and retained terminal operand callbacks.
check_raw_line_budget "$wasm_generator_loop" 730
# Ordinary Switch source ranges, one CaseBlock and recursive Empty completion
# have consumed private owners. Runtime V stays in the original invocation's
# real cells, and the generic sequence/Yield dispatch uses that same context.
ir_generator_switch_source="crates/scriptw-ir/src/generator_switch_source.rs"
ir_generator_switch="crates/scriptw-ir/src/generator_switch.rs"
ir_generator_switch_lowerer="crates/scriptw-ir/src/lowering/generator_switch.rs"
ir_resumable_switch_lowerer="crates/scriptw-ir/src/lowering/resumable_switch.rs"
wasm_generator_switch="crates/scriptw-aot-wasm/src/control_flow/generator_switch.rs"
wasm_statement_list_value="crates/scriptw-aot-wasm/src/control_flow/statement_completion.rs"
wasm_resumable_sequence="crates/scriptw-aot-wasm/src/control_flow/resumable_sequence.rs"
for generator_switch_owner in \
  "$ir_generator_switch_source" "$ir_generator_switch" \
  "$ir_generator_switch_lowerer" "$ir_resumable_switch_lowerer" "$wasm_generator_switch" \
  "$wasm_statement_list_value" "$wasm_resumable_sequence"
do
  require_file "$generator_switch_owner"
  check_no_inline_legacy_includes "$generator_switch_owner"
done
require_exact_line_count "$ir_lib" 'mod generator_switch;' 1 'opaque checked Switch domain attachment'
require_exact_line_count "$ir_lowering" 'mod generator_switch;' 1 'private source Switch lowerer attachment'
require_exact_line_count "$ir_lowering" 'mod resumable_switch;' 1 'private shared complete Switch algorithm attachment'
require_exact_line_count crates/scriptw-ir/src/lowering_helpers.rs 'mod generator_switch_source;' 1 'private actual Switch source-plan attachment'
require_exact_line_count "$wasm_control_flow" 'mod generator_switch;' 1 'private native Switch owner attachment'
for switch_private_parent in "$ir_lib" "$ir_lowering" "$wasm_control_flow"; do
  if grep -Eq '^pub(\([^)]*\))?[[:space:]]+mod[[:space:]]+generator_switch;' "$switch_private_parent"; then
    fail "$switch_private_parent must keep its generator Switch module private"
  fi
done
require_fixed_string_count "$ir_generator_switch_source" 'pub(crate) struct CheckedGeneratorSwitchSource<' 1 'checked actual Switch source owner'
require_fixed_string_count "$ir_generator_switch_source" 'pub(crate) struct CheckedEmptyStatementCompletionSource<' 1 'actual recursive Empty source proof'
require_fixed_string_count "$ir_generator_switch_lowerer" 'pub(super) fn lower_ordinary_generator_switch(' 1 'sole ordinary Switch producer'
require_fixed_string_count crates/scriptw-ir/src/lowering/switch_statement.rs 'self.lower_ordinary_generator_switch(switch)' 1 'actual ordinary Switch source dispatch'
require_fixed_string_count "$ir_lowering" 'fn lower_ordinary_generator_switch(' 0 'no parent Switch producer copy'
require_fixed_string_count "$ir_generator_switch_lowerer" 'CheckedGeneratorSwitchSource::new(' 1 'lowering consumes the actual checked source plan'
require_fixed_string_count "$ir_generator_switch_source" 'CheckedGeneratorSwitchSource::new(' 1 'function source planner consumes the same checked owner'
require_fixed_string_count "$ir_generator_switch_lowerer" 'self.lower_resumable_switch::<super::resumable_switch::OrdinarySwitch>(switch, states)' 1 'ordinary adapter consumes the sealed shared Switch owner'
require_fixed_string_count "$ir_resumable_switch_lowerer" 'pub(super) trait ResumableSwitchProtocol: sealed::Sealed {' 1 'closed source-to-region Switch protocol'
require_exact_line_count "$ir_resumable_switch_lowerer" 'mod sealed {' 1 'private Switch protocol sealing authority'
require_fixed_string_count "$ir_resumable_switch_lowerer" 'pub(super) fn lower_resumable_switch<P: ResumableSwitchProtocol>(' 1 'sole shared complete Switch lowering algorithm'
require_tree_regex_count crates/scriptw-ir/src/lowering '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+lower_resumable_switch[[:space:]]*<' 1 'one complete Switch lowering owner at any visibility'
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+mod[[:space:]]+resumable_switch;' "$ir_lowering" \
  || grep -Eq '^pub(\([^)]*\))?[[:space:]]+mod[[:space:]]+sealed' "$ir_resumable_switch_lowerer"; then
  fail 'the shared Switch algorithm and its protocol sealing authority must remain private modules'
fi
require_fixed_string_count "$ir_resumable_switch_lowerer" 'OrdinaryGeneratorSwitchIr::new(' 1 'shared owner consumes the actual opaque ordinary Switch constructor'
require_regex_count "$ir_generator_switch" '^pub struct (OrdinaryGeneratorSwitchIr|OrdinaryGeneratorSwitchCaseIr|EmptyStatementCompletionIr) \{' 3 'closed opaque Switch and original Empty item carriers'
require_regex_count "$ir_generator_switch" '^[[:space:]]*pub[[:space:]]+fn[[:space:]]+new\(' 0 'no unchecked public Switch or Empty constructor'
require_regex_count "$ir_generator_switch" '^[[:space:]]*pub[[:space:]]+(discriminant|cases|value_binding|statement)[[:space:]]*:' 0 'no public carrier fields'
ordinary_switch_protocol_code="$(braced_rust_item_source "$ir_resumable_switch_lowerer" '^impl ResumableSwitchProtocol for OrdinarySwitch')"
require_text_regex_count "$ordinary_switch_protocol_code" 'l\.ordinary_generator_region_depth \+= 1;' 1 'actual nested ordinary region entry in the sealed adapter'
require_text_regex_count "$ordinary_switch_protocol_code" 'l\.ordinary_generator_region_depth -= 1;' 1 'balanced ordinary region exit in the sealed adapter'
require_text_regex_count "$ordinary_switch_protocol_code" 'l\.ordinary_generator_switch_depth \+= 1;' 1 'actual ordinary Switch completion scope entry'
require_text_regex_count "$ordinary_switch_protocol_code" 'l\.ordinary_generator_switch_depth -= 1;' 1 'balanced ordinary Switch completion scope exit'
require_fixed_string_count "$ir_lowering" 'ordinary_generator_loop_depth' 0 'retired loop-only region context spelling'
require_fixed_string_count crates/scriptw-ir/src/lowering/statement.rs 'CheckedEmptyStatementCompletionSource::from_statement(statement)' 1 'actual Var/Empty/Debugger completion proof'
require_fixed_string_count "$ir_lowering" 'CheckedEmptyStatementCompletionSource::from_declaration(declaration)' 1 'actual declaration completion proof'
require_fixed_string_count "$ir_lowering" 'if self.ordinary_generator_switch_depth > 0' 1 'declaration Empty wrapping consumes actual enclosing complete contexts'
require_fixed_string_count crates/scriptw-ir/src/lowering/statement.rs 'if self.ordinary_generator_switch_depth > 0' 1 'statement Empty wrapping consumes actual enclosing complete contexts'
require_fixed_string_count "$wasm_generator_switch" 'pub(super) fn compile_ordinary_generator_switch(' 1 'sole native checked Switch compiler'
require_fixed_string_count "$wasm_control_flow" 'fn compile_ordinary_generator_switch(' 0 'no native Switch implementation copy in parent'
require_fixed_string_count "$wasm_control_flow" 'self.compile_ordinary_generator_switch(plan,' 2 'ordinary and labelled native Switch dispatch'
require_fixed_string_count "$wasm_generator_switch" 'self.begin_resumable_switch_statement_list_value(plan, break_target)?;' 1 'whole checked Switch constructs its actual completion context'
require_fixed_string_count "$wasm_generator_switch" 'self.end_generator_statement_list_value();' 1 'actual enclosing completion context restoration'
require_fixed_string_count "$wasm_generator_switch" 'builder.compile_switch_case_match(&value, selector.value(), function);' 1 'native selector strict equality consumes the terminal value inside its retained operand scope'
require_fixed_string_count "$wasm_generator_switch" 'self.compile_resumable_operand_region_then(' 1 'selector terminal match remains inside operand suppression and retained scope'
for shared_switch_entry in compile_resumable_generator_switch compile_resumable_generator_switch_selection_and_bodies; do
  require_tree_regex_count crates/scriptw-aot-wasm/src "^[[:space:]]*fn[[:space:]]+${shared_switch_entry}[[:space:]]*\\(" 1 'sole physical complete Switch lifecycle'
done
require_fixed_string_count "$wasm_statement_list_value" 'pub(crate) struct GeneratorStatementListValueContext {' 1 'sole native persistent statement value owner'
require_fixed_string_count "$wasm_statement_list_value" 'pub(super) fn compile_resumable_operand_region_then(' 1 'one retained terminal operand scope owner'
require_tree_regex_count crates/scriptw-aot-wasm/src '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+compile_resumable_operand_region_then[[:space:]]*\(' 1 'no duplicate terminal operand scope lifecycle'
operand_region_scope_code="$(braced_rust_item_source "$wasm_statement_list_value" '^pub[(]super[)] fn compile_resumable_operand_region_then')"
require_text_regex_count "$operand_region_scope_code" '\.and_then\(\|\(\)\| finish\(self, function\)\);' 1 'terminal value is consumed before exact scope restoration'
for operand_scope_restore in \
  'self.binding_scopes = previous_scopes;' \
  'self.statement_list_value_context = previous_context;' \
  'self.environment_depth = previous_environment_depth;'
do
  require_text_regex_count "$operand_region_scope_code" "$(printf '%s\n' "$operand_scope_restore" | sed 's/\./\\./g')" 1 'prior operand scope and completion context restore on success and Err'
done
require_fixed_string_count crates/scriptw-aot-wasm/src/emit.rs 'pub(crate) statement_list_value_context:' 1 'actual callable builder owns the completion scope'
require_fixed_string_count crates/scriptw-aot-wasm/src/emit.rs 'statement_list_value_context: None,' 1 'fresh callable has no Switch completion scope'
require_fixed_string_count "$wasm_statement_list_value" 'InvocationFrameSchema::INVOCATION_ENVIRONMENT' 1 'retained values consume original invocation cells'
require_fixed_string_count "$wasm_resumable_sequence" 'self.emit_generator_statement_list_entry(entry_state, function)?;' 1 'actual resumable sequence consumes persistent entry value'
require_fixed_string_count "$wasm_control_flow" 'self.compile_empty_statement_completion(item.statement(), function)?;' 1 'actual recursive Empty completion native dispatch'
require_fixed_string_count "$wasm_control_flow" 'self.emit_retire_generator_statement_list_values(Some(target), function);' 1 'leaving native branches retire private completion cells'
require_fixed_string_count "$wasm_control_flow" 'self.emit_retire_generator_statement_list_values(None, function);' 1 'committed native invocation exit retires private completion cells'
require_fixed_string_count "$wasm_control_flow" '!self.generator_statement_list_branch_retires(target)' 1 'conditional branch cannot bypass leaving-scope retirement'
require_fixed_string_count crates/scriptw-aot-wasm/src/generator_delegation.rs 'if !self.has_generator_statement_list_value() {' 1 'delegated completion respects the actual active complete StatementList value owner'
require_fixed_string_count crates/scriptw-ir/src/generator_loop_control/head_bindings.rs 'visit_head_statement_binding(item.statement(), visit);' 1 'actual head bindings delegate the original Empty item'
check_raw_line_budget "$ir_generator_switch_source" 225
check_raw_line_budget "$ir_generator_switch_lowerer" 240
# Ordinary/async/mixed typed adapters, shared selectors/CaseBlock lowering and
# whole CaseBlock resource registration live in this single bounded owner.
check_raw_line_budget "$ir_resumable_switch_lowerer" 800
check_raw_line_budget "$ir_generator_switch" 325
# Actual complete Array/Class prefix state and suspension joins measure956.
check_raw_line_budget crates/scriptw-ir/src/generator_loop_control.rs 980
# The Switch facade shares selection/fallthrough/CaseBlock cleanup across all
# protocols and whole resource lifetimes. Completion owns their retained V and
# the terminal operand callback, including exact restoration on compilation Err.
check_raw_line_budget "$wasm_generator_switch" 420
check_raw_line_budget "$wasm_statement_list_value" 600
check_raw_line_budget "$wasm_resumable_sequence" 110
require_module_decl "$wasm_control_flow" "async_suspension"
require_file crates/scriptw-aot-wasm/src/control_flow/async_suspension.rs
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/control_flow/async_suspension.rs
check_raw_line_budget crates/scriptw-aot-wasm/src/control_flow/async_suspension.rs 60
require_file crates/scriptw-ir/tests/generator_switch_regions.rs
require_file crates/scriptw-engine/tests/aot_generators/aot_generator_switch_regions.rs
require_file crates/scriptw-engine/tests/fixtures/generator_switch_regions/switches.js
# Constructor admission, classic For environment creation and activation
# retention share the real declaration-binding visitor, including patterns.
ir_generator_head_bindings="crates/scriptw-ir/src/generator_loop_control/head_bindings.rs"
require_file "$ir_generator_head_bindings"
require_exact_line_count crates/scriptw-ir/src/generator_loop_control.rs 'mod head_bindings;' 1 'private actual For head binding owner'
require_exact_line_count crates/scriptw-ir/src/generator_loop_control.rs 'pub(crate) use head_bindings::visit_lexical_head_statement_bindings;' 1 'shared actual head-binding projection'
require_fixed_string_count "$ir_generator_head_bindings" 'pub(crate) fn visit_lexical_head_statement_bindings(' 1 'sole declaration-binding visitor entry'
require_fixed_string_count crates/scriptw-ir/src/generator_loop_control.rs 'visit_lexical_head_statement_bindings(' 1 'checked loop constructor consumes actual bindings'
require_fixed_string_count crates/scriptw-ir/src/lowering/for_lexical_environment.rs 'visit_lexical_head_statement_bindings(' 1 'actual For environment consumes the same visitor'
require_fixed_string_count crates/scriptw-ir/src/lowering/resumable_for_initializer.rs 'Self::visit_for_head_lexical_bindings(' 1 'shared complete activation retains the same original head bindings'
for classic_for_consumer in for_loop generator_loop; do
  require_fixed_string_count "crates/scriptw-ir/src/lowering/${classic_for_consumer}.rs" 'self.lower_for_lexical_environment(' 1 'shared analyzed classic For environment consumer'
done
check_no_inline_legacy_includes "$ir_generator_head_bindings"
# Binding patterns, complete resource heads and Empty items share this original
# cell projection; it never visits a child scope's CaseBlock declarations.
check_raw_line_budget "$ir_generator_head_bindings" 140
wasm_identifier_reference_parent="crates/scriptw-aot-wasm/src/environments/environment_reference.rs"
wasm_identifier_reference_capture="crates/scriptw-aot-wasm/src/environments/environment_reference/captured_identifier_reference.rs"
wasm_global_identifier_read="crates/scriptw-aot-wasm/src/environments/environment_reference/global_identifier_read.rs"
require_file "$wasm_global_identifier_read"
require_exact_line_count "$wasm_identifier_reference_parent" 'mod global_identifier_read;' 1 'private complete global Get helper owner'
require_fixed_string_count "$wasm_global_identifier_read" 'pub(crate) fn emit_global_identifier_read(' 1 'sole consumed global Get facade'
require_fixed_string_count crates/scriptw-aot-wasm/src/environments.rs 'fn emit_global_identifier_read(' 0 'no parent inline global Get copy'
require_fixed_string_count "$wasm_global_identifier_read" '.emit_resolve_environment_identifier_from_record(' 1 'shared real ResolveBinding owner'
require_fixed_string_count "$wasm_global_identifier_read" 'self.emit_environment_identifier_get(' 1 'shared real GetBindingValue owner'
require_fixed_string_count "$wasm_global_identifier_read" 'self.release_environment_identifier_reference(' 1 'same owned Reference released'
check_no_inline_legacy_includes "$wasm_global_identifier_read"
check_raw_line_budget "$wasm_global_identifier_read" 180
wasm_identifier_reference_consumer="crates/scriptw-aot-wasm/src/expressions/environment_identifier.rs"
require_file "$wasm_identifier_reference_capture"
require_exact_line_count "$wasm_identifier_reference_parent" 'mod captured_identifier_reference;' 1 'private native Reference transport attachment'
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+mod[[:space:]]+captured_identifier_reference;' "$wasm_identifier_reference_parent"; then
  fail 'native Reference transport must remain private to its Environment Reference owner'
fi
for reference_transport_entry in emit_capture_identifier_reference emit_take_captured_identifier_reference emit_release_captured_identifier_reference; do
  require_fixed_string_count "$wasm_identifier_reference_capture" "pub(crate) fn ${reference_transport_entry}(" 1 'consumed native Reference transport entry'
  require_fixed_string_count "$wasm_identifier_reference_consumer" "self.${reference_transport_entry}(" 1 'closed Environment Identifier transport dispatch'
  require_fixed_string_count "$wasm_identifier_reference_parent" "fn ${reference_transport_entry}(" 0 'no parent native Reference transport copy'
done
require_fixed_string_count crates/scriptw-aot-wasm/src/control_flow.rs 'self.emit_retire_abandoned_identifier_references(function);' 4 'committed Return/caught Throw and whole async-generator completion dispatch retire original References'
require_fixed_string_count crates/scriptw-aot-wasm/src/control_flow.rs 'self.emit_retire_abandoned_identifier_references_if_throw(function);' 1 'actual uncaught Throw retirement at completion exit'
require_file crates/scriptw-engine/tests/aot_generators/aot_generator_identifier_reference.rs
require_file crates/scriptw-engine/tests/fixtures/generator_staged_operands/identifier_references.js
check_no_inline_legacy_includes "$wasm_identifier_reference_capture"
# The same field10 record now admits Generator/Async and checked mixed owners,
# retaining it through normal suspension and retiring only committed abrupt paths.
check_raw_line_budget "$wasm_identifier_reference_capture" 590
require_fixed_string_count crates/scriptw-aot-wasm/src/builtins/promise.rs 'self.emit_poll_atomics_wait_async_timeouts(' 1 'Promise checkpoint actual async-wait poll consumer'
# Current formatted owners: 1,187 operation lines and 367 queue/result lines.
check_raw_line_budget "$wasm_atomics_builtins" 1250
check_raw_line_budget "$wasm_atomics_wait_async" 400
require_file crates/scriptw-engine/tests/aot_gc_entries/aot_gc_binary_data_entries.rs
require_fixed_string_count crates/scriptw-engine/tests/aot_gc_entries/aot_gc_binary_data_entries.rs 'fn gc_atomics_keep_whole_abrupts_finite_waits_and_called_realms()' 1 'finite Atomics GC semantic control'

# Boolean construction consumes GetPrototypeFromConstructor; the private closed
# prototype policy is shared only by the two fixed ToString/ValueOf wrappers.
wasm_boolean_builtins="crates/scriptw-aot-wasm/src/builtins/boolean.rs"
check_no_inline_legacy_includes "$wasm_boolean_builtins"
require_regex_count "$wasm_boolean_builtins" '^enum BooleanPrototypeOperation \{' 1 'private Boolean prototype domain'
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+enum[[:space:]]+BooleanPrototypeOperation' "$wasm_boolean_builtins"   || grep -Eq 'BooleanPrototypeOperation|emit_boolean_prototype_builtin\(' "$wasm_standard_builtins"; then
  fail 'Boolean prototype policy must stay inside its native owner'
fi
for boolean_entry in constructor prototype_to_string prototype_value_of; do
  require_regex_count "$wasm_boolean_builtins" "^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+emit_boolean_${boolean_entry}_builtin[[:space:]]*\(" 1 'fixed Boolean entry'
  require_fixed_string_count "$wasm_standard_builtins" "self.emit_boolean_${boolean_entry}_builtin(function)?" 1 'fixed Boolean dispatch'
done
check_raw_line_budget "$wasm_boolean_builtins" 200

wasm_math_builtins="crates/scriptw-aot-wasm/src/builtins/math.rs"
check_no_inline_legacy_includes "$wasm_math_builtins"
if ! grep -q '^enum MathBuiltin' "$wasm_math_builtins" \
  || ! grep -q '^enum MathUnaryBuiltin' "$wasm_math_builtins" \
  || ! grep -q '^        match builtin {' "$wasm_math_builtins" \
  || ! grep -q '^                match unary {' "$wasm_math_builtins"; then
  fail "$wasm_math_builtins must dispatch through the closed nested Math domains"
fi
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+enum[[:space:]]+(MathBuiltin|MathUnaryBuiltin)' "$wasm_math_builtins"; then
  fail "$wasm_math_builtins must keep both Math domains private"
fi
if grep -Eq 'MathBuiltin|MathUnaryBuiltin|MathFn|UnaryMathFn|emit_math\(' "$wasm_standard_builtins"; then
  fail "$wasm_standard_builtins must use fixed Math entries"
fi
require_fixed_string_count "$wasm_math_builtins" 'fn emit_math(' 1 'private Math emitter'
require_fixed_string_count "$wasm_math_builtins" 'self.emit_math(' 37 'fixed Math entry calls'
require_fixed_string_count "$wasm_math_builtins" 'pub(super) fn emit_math_' 37 'fixed Math entry definitions'
require_fixed_string_count "$wasm_standard_builtins" 'self.emit_math_' 37 'fixed Math routes'
if grep -q 'StandardBuiltinId::' "$wasm_math_builtins"; then
  fail "$wasm_math_builtins must accept only its closed family domains, not StandardBuiltinId"
fi
# The private sumPrecise owner keeps the exact arithmetic and iterator lifecycle
# together; its state, limb operation and completed witness cannot escape into
# the family dispatcher.
wasm_math_sum_precise="crates/scriptw-aot-wasm/src/builtins/math/sum_precise.rs"
require_file "$wasm_math_sum_precise"
check_no_inline_legacy_includes "$wasm_math_sum_precise"
require_exact_line_count "$wasm_math_builtins" 'mod sum_precise;' 1 'private Math.sumPrecise owner'
require_fixed_string_count "$wasm_math_builtins" 'self.emit_runtime_math_sum_precise(' 1 'Math.sumPrecise semantic consumer'
require_fixed_string_count "$wasm_math_sum_precise" 'pub(super) fn emit_runtime_math_sum_precise(' 1 'Math.sumPrecise semantic entry'
require_fixed_string_count "$wasm_math_sum_precise" 'pub(super) fn' 1 'sole parent-visible Math.sumPrecise entry'
require_fixed_string_count "$wasm_math_sum_precise" 'StandardBuiltinId::' 0 'Math.sumPrecise family-domain isolation'
if grep -Eq '^pub|pub\(crate\)' "$wasm_math_sum_precise"; then
  fail "$wasm_math_sum_precise must keep its arithmetic domains and lifecycle private"
fi
for sum_precise_policy in \
  MATH_SUM_PRECISE \
  MathSumPreciseState \
  MathSumPreciseLimbOperation \
  MathSumPreciseAccumulator \
  CompletedMathSumPreciseReduction; do
  sum_precise_policy_files="$(grep -RFl --include='*.rs' "$sum_precise_policy" crates/scriptw-aot-wasm/src || true)"
  if [ "$sum_precise_policy_files" != "$wasm_math_sum_precise" ]; then
    fail "$sum_precise_policy must remain owned only by $wasm_math_sum_precise"
  fi
done
# Measured after the complete sumPrecise lifecycle extraction: 1,625 parent
# lines and 1,034 child lines. The existing parent budget is unchanged; the
# child margin permits narrow maintenance of the exact reduction owner.
check_raw_line_budget "$wasm_math_builtins" 2430
check_raw_line_budget "$wasm_math_sum_precise" 1050

wasm_number_builtins="crates/scriptw-aot-wasm/src/builtins/number.rs"
check_no_inline_legacy_includes "$wasm_number_builtins"
for number_domain in NumberPredicate NumberPrototypeOperation; do
  require_regex_count "$wasm_number_builtins" "^enum ${number_domain} \{" 1 'private closed Number policy owner'
  if grep -Eq "^pub(\([^)]*\))?[[:space:]]+enum[[:space:]]+${number_domain}" "$wasm_number_builtins"; then
    fail 'Number policy domains must remain private to their actual native owner'
  fi
done
if grep -Eq 'NumberPredicate|NumberPrototypeOperation|emit_number_predicate\(|emit_number_prototype_builtin\(' "$wasm_standard_builtins"; then
  fail 'the Standard dispatcher must use fixed Number entries'
fi
for number_internal_entry in emit_number_predicate emit_number_prototype_builtin; do
  require_fixed_string_count "$wasm_number_builtins" "fn ${number_internal_entry}(" 1 'private Number algorithm owner'
done
require_fixed_string_count \
  crates/scriptw-cli/tests/cli/language_numerics.rs \
  'fn run_wasm_backend_succeeds_for_number_builtin_family_fixture()' \
  1 \
  'Number builtin-family CLI regression'
if [ ! -f crates/scriptw-cli/tests/fixtures/wasm_number_builtin_family.js ]; then
  fail 'Number builtin-family fixture must remain present'
fi
# Measured after closing the eleven fixed entries: 400 raw lines. The narrow
# margin is for maintenance of this family, not adjacent builtin implementations.
check_raw_line_budget "$wasm_number_builtins" 430

wasm_function_builtins="crates/scriptw-aot-wasm/src/builtins/function.rs"
check_no_inline_legacy_includes "$wasm_function_builtins"
require_regex_count "$wasm_function_builtins" '^enum FunctionBuiltin \{' 1 'private closed Function builtin family'
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+enum[[:space:]]+FunctionBuiltin' "$wasm_function_builtins"; then
  fail 'FunctionBuiltin must remain private'
fi
if grep -Eq 'FunctionBuiltin|emit_function_builtin\(' "$wasm_standard_builtins"; then
  fail 'the Standard dispatcher must use fixed Function entries'
fi
require_fixed_string_count "$wasm_function_builtins" 'fn emit_function_builtin(' 1 'private Function algorithm owner'
# Bound invocation uses the typed callable dispatch/record owner. Its real
# call, construct, new.target and root fixtures below remain required.
require_fixed_string_count \
  crates/scriptw-cli/tests/cli/functions.rs \
  'fn run_wasm_backend_succeeds_for_supported_bind_builtin_fixture()' \
  1 \
  'bound-function call/construct regression'
require_fixed_string_count \
  crates/scriptw-cli/tests/cli/language_errors.rs \
  'fn run_wasm_backend_succeeds_for_bound_construct_new_target_identity_fixture()' \
  1 \
  'bound-function new.target regression'
require_fixed_string_count \
  crates/scriptw-cli/tests/cli/heap.rs \
  'fn run_wasm_backend_succeeds_for_heap_rooted_bound_function_fixture()' \
  1 \
  'bound-function heap-rooting regression'
# Measured after closing eight fixed entries: 508 raw lines. The narrow margin
# is for maintenance of this family, not adjacent builtin implementations.
check_raw_line_budget "$wasm_function_builtins" 525

wasm_date_builtins="crates/scriptw-aot-wasm/src/builtins/date.rs"
wasm_date_constructor="crates/scriptw-aot-wasm/src/builtins/date/constructor.rs"
wasm_date_components="crates/scriptw-aot-wasm/src/builtins/date/components.rs"
wasm_date_local_string="crates/scriptw-aot-wasm/src/builtins/date/local_string.rs"
for date_owner in "$wasm_date_constructor" "$wasm_date_components" "$wasm_date_local_string"
do
  require_file "$date_owner"
  check_no_inline_legacy_includes "$date_owner"
done
check_no_inline_legacy_includes "$wasm_date_builtins"
for date_child in constructor components zone
do
  require_exact_line_count "$wasm_date_builtins" "mod $date_child;" 1 'private Date child declaration'
done
for date_domain in DateTimeBasis DateComponentGetter DateComponentSetter
do
  require_fixed_string_count "$wasm_date_components" "enum $date_domain {" 1 'closed Date operation owner'
done
if grep -Eq '^[[:space:]]*_ =>|unreachable!\(' "$wasm_date_components" "$wasm_date_constructor"; then
  fail 'Date component and construction decisions must keep exhaustive closed domains'
fi
require_fixed_string_count "$wasm_date_components" 'fn emit_date_component_getter(' 1 'Date getter emitter'
require_fixed_string_count "$wasm_date_components" 'fn emit_date_component_setter(' 1 'Date setter emitter'
require_fixed_string_count "$wasm_standard_builtins" 'self.emit_date_component_getter(' 18 'all local/UTC/legacy getter delegates'
require_fixed_string_count "$wasm_standard_builtins" 'self.emit_date_component_setter(' 15 'all local/UTC/legacy setter delegates'
require_fixed_string_count "$wasm_date_constructor" 'fn emit_date_constructor(' 1 'Date construction owner'
require_fixed_string_count "$wasm_standard_builtins" 'self.emit_date_constructor(function)?;' 1 'Date constructor semantic delegate'
require_fixed_string_count "$wasm_standard_builtins" 'self.emit_date_utc(function)?;' 1 'Date.UTC semantic delegate'
require_fixed_string_count "$wasm_standard_builtins" 'self.emit_date_set_time(function)?;' 1 'Date setTime semantic delegate'
require_fixed_string_count "$wasm_date_builtins" 'fn emit_date_store_clip(' 1 'sole typed Date setter storage owner'
require_fixed_string_count "$wasm_date_constructor" 'self.emit_date_store_clip(' 1 'setTime actual storage caller'
require_fixed_string_count "$wasm_date_components" 'self.emit_date_store_clip(' 1 'component actual storage caller'
require_fixed_string_count "$wasm_date_constructor" 'schema.struct_type::<DateObject>().construct(' 1 'sole concrete GC Date allocation owner'
date_allocation_body="$(braced_rust_item_source "$wasm_date_constructor" '^fn[[:space:]]+emit_date_alloc_with_new_target[[:space:]]*[(]')"
require_text_regex_count "$date_allocation_body" 'clip: &CompletedDateClipLocals,' 1 'sole Date allocator requires completed TimeClip'
date_number_result_body="$(sed -n '/fn emit_date_clip_number_result(/,$p' "$wasm_date_constructor")"
require_text_regex_count "$date_number_result_body" 'clip: &CompletedDateClipLocals,' 1 'Date Number result borrows completed TimeClip'
require_fixed_string_count "$wasm_date_builtins" 'clip: &CompletedDateClipLocals' 1 'Date writer requires completed TimeClip'
require_exact_line_count \
  "$wasm_date_builtins" \
  'mod local_string;' \
  1 \
  'private Date local-string module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+local_string;' "$wasm_date_builtins"; then
  fail "$wasm_date_builtins must keep local_string private"
fi
if grep -Eq 'DateLocalStringFormat|DateTimeValueSource|emit_date_time_value_from_source\(|wall_clock_millis_import_function_index|local_string::' "$wasm_date_builtins"; then
  fail "$wasm_date_builtins must not name, construct, project or import the private Date local-string and time-source policies"
fi
require_regex_count \
  "$wasm_date_local_string" \
  '^enum[[:space:]]+DateTimeValueSource[[:space:]]*\{' \
  1 \
  'private Date time-value source owner'
require_regex_count \
  "$wasm_date_local_string" \
  '^[[:space:]]*fn[[:space:]]+emit_date_time_value_from_source[[:space:]]*[(]' \
  1 \
  'sole private Date time-value source dispatcher'
require_fixed_string_count \
  "$wasm_date_local_string" \
  '.wall_clock_millis_import_function_index()' \
  1 \
  'sole Date clock-import access'
require_regex_count \
  "$wasm_date_local_string" \
  '^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_date_current_time_payload[[:space:]]*\(' \
  1 \
  'Date current-time semantic wrapper owner'
require_fixed_string_count \
  "$wasm_date_builtins" \
  'self.emit_date_current_time_payload(bits, function)?;' \
  1 \
  'unchanged Date.now semantic delegate'
require_fixed_string_count \
  "$wasm_date_constructor" \
  'self.emit_date_current_time_payload(bits, function)?;' \
  1 \
  'Date constructor current-time semantic delegate'
require_regex_count \
  "$wasm_date_local_string" \
  '^enum[[:space:]]+DateLocalStringFormat[[:space:]]*\{' \
  1 \
  'private Date local-string format owner'
require_fixed_string_count \
  "$wasm_date_local_string" \
  'emit_date_local_string(' \
  5 \
  'Date local-string consumer and producer sites'
for date_local_string_surface in \
  emit_date_function_call \
  emit_date_to_date_string \
  emit_date_to_time_string \
  emit_date_to_string
do
  require_regex_count \
    "$wasm_date_local_string" \
    "^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+${date_local_string_surface}[[:space:]]*\(" \
    1 \
    "$date_local_string_surface semantic wrapper owner"
done
require_fixed_string_count "$wasm_date_constructor" 'self.emit_date_function_call(function)?;' 1 \
  'Date function-call local-string delegate'
for date_local_string_call in \
  'self.emit_date_to_date_string(function)?;' \
  'self.emit_date_to_time_string(function)?;' \
  'self.emit_date_to_string(function)?;'
do
  require_fixed_string_count \
    "$wasm_standard_builtins" \
    "$date_local_string_call" \
    1 \
    'Date local-string semantic delegate'
done
for date_locale_format in Date Time DateAndTime
do
  require_fixed_string_count \
    "$wasm_standard_builtins" \
    "self.emit_date_to_locale_string(DateLocaleFormat::$date_locale_format, function)?;" \
    1 \
    "$date_locale_format Date locale-method delegate"
done
# GC construction now retains typed prototype and completion owners (317 lines).
# Keep the existing parent/component/string bounds and a narrow constructor margin.
check_raw_line_budget "$wasm_date_builtins" 1650
check_raw_line_budget "$wasm_date_constructor" 350
check_raw_line_budget "$wasm_date_components" 390
check_raw_line_budget "$wasm_date_local_string" 480

wasm_bigint_builtins="crates/scriptw-aot-wasm/src/builtins/bigint.rs"
wasm_bigint_radix_formatting="crates/scriptw-aot-wasm/src/builtins/bigint/radix_formatting.rs"
wasm_numeric_operations="crates/scriptw-aot-wasm/src/operations.rs"
wasm_emit="crates/scriptw-aot-wasm/src/emit.rs"
wasm_host_builtins="crates/scriptw-aot-wasm/src/builtins/host.rs"
wasm_host_detach_array_buffer="crates/scriptw-aot-wasm/src/builtins/host/detach_array_buffer.rs"
require_file "$wasm_host_detach_array_buffer"
require_exact_line_count "$wasm_host_builtins" 'mod detach_array_buffer;' 1 \
  'private host ArrayBuffer detach module declaration'
require_fixed_string_count "$wasm_host_detach_array_buffer" \
  'fn compile_host_detach_array_buffer_builtin(' 1 'host ArrayBuffer detach owner'
require_fixed_string_count "$wasm_host_builtins" \
  'fn compile_host_detach_array_buffer_builtin(' 0 'no parent host ArrayBuffer detach copy'
check_raw_line_budget "$wasm_host_detach_array_buffer" 50
check_no_inline_legacy_includes "$wasm_host_builtins"
# Created globals use the shared completed Realm bootstrap; the retired private
# WeakRef installer cannot provide a weak facility through ordinary GC fields.
require_file crates/scriptw-aot-wasm/src/builtins/weak.rs
require_module_decl crates/scriptw-aot-wasm/src/builtins/mod.rs weak
check_no_inline_legacy_includes "$wasm_bigint_builtins"
check_no_inline_legacy_includes "$wasm_bigint_radix_formatting"
for bigint_domain in BigIntFixedWidthOperation BigIntPrototypeOperation; do
  require_regex_count "$wasm_bigint_builtins" "^enum ${bigint_domain} \{" 1 'private closed BigInt policy owner'
  if grep -Eq "^pub(\([^)]*\))?[[:space:]]+enum[[:space:]]+${bigint_domain}" "$wasm_bigint_builtins"; then
    fail 'BigInt native policy domains must remain private'
  fi
done
if grep -Eq 'BigIntFixedWidthOperation|BigIntPrototypeOperation|emit_bigint_builtin\(' "$wasm_standard_builtins"; then
  fail 'the Standard dispatcher must use fixed BigInt entries'
fi
require_exact_line_count "$wasm_bigint_builtins" 'mod radix_formatting;' 1 'private BigInt radix owner'
require_regex_count "$wasm_bigint_radix_formatting" '^struct PreparedBigIntRadix\(I64Local\);$' 1 'private completed scalar radix'
require_fixed_string_count "$wasm_bigint_radix_formatting" 'pub(super) fn emit_bigint_radix_string_result(' 1 'radix semantic wrapper'
if grep -Eq 'PreparedBigIntRadix|radix_formatting::' "$wasm_bigint_builtins"; then
  fail 'BigInt parent must consume the radix wrapper without constructing its private proof'
fi

# Numeric helpers carry whole input and the actual nullable caller Environment
# through the same registry row used by both helper bodies and call adapters.
require_file crates/scriptw-aot-wasm/src/runtime_helpers.rs
for numeric_helper_arguments in ValueToNumberArguments ValueToNumericArguments; do
  require_fixed_string_count crates/scriptw-aot-wasm/src/runtime_helpers.rs "$numeric_helper_arguments" 1 'registered typed numeric helper argument owner'
done

# BigInt callable publication uses the shared retained-Realm function factory.
# The former per-error raw header slots are not part of the GC representation.

# Measured after the prepared-radix lifecycle extraction: 807 parent lines and
# 94 child lines. The narrow margins are for maintenance of each owner.
# Measured after closing the six fixed entries: 855 raw lines. The narrow
# margin is for maintenance of this family, not adjacent builtin work.
check_raw_line_budget "$wasm_bigint_builtins" 875
check_raw_line_budget "$wasm_bigint_radix_formatting" 120
# The parseFloat decimal-prefix grammar and its compiler entry share a private
# owner; only the existing product compiler entry is crate-visible.
wasm_host_parse_float="crates/scriptw-aot-wasm/src/builtins/host/parse_float.rs"
require_file "$wasm_host_parse_float"
check_no_inline_legacy_includes "$wasm_host_parse_float"
require_exact_line_count "$wasm_host_builtins" 'mod parse_float;' 1 'private parseFloat owner'
require_fixed_string_count "$wasm_host_parse_float" 'pub(crate) fn compile_host_parse_float_builtin(' 1 'parseFloat product compiler entry'
require_fixed_string_count "$wasm_host_parse_float" 'pub(crate) fn' 1 'sole crate-visible parseFloat entry'
require_fixed_string_count "$wasm_host_parse_float" 'fn emit_host_decimal_digits(' 1 'private parseFloat digit grammar owner'
require_fixed_string_count "$wasm_host_builtins" 'fn emit_host_decimal_digits(' 0 'parseFloat grammar isolation'
require_fixed_string_count "$wasm_host_builtins" 'compile_host_parse_float_builtin' 0 'parseFloat compiler ownership'
# Measured after the complete parseFloat extraction: 8,634 parent lines and
# 402 child lines. Existing host and WeakRef budgets are unchanged.
check_raw_line_budget "$wasm_host_builtins" 9000
check_raw_line_budget "$wasm_host_parse_float" 430

wasm_intl_locale="crates/scriptw-aot-wasm/src/builtins/intl.rs"
wasm_intl_locale_construction="crates/scriptw-aot-wasm/src/builtins/intl/construction_lifecycle.rs"
require_file "$wasm_intl_locale"
require_file "$wasm_intl_locale_construction"
check_no_inline_legacy_includes "$wasm_intl_locale"
check_no_inline_legacy_includes "$wasm_intl_locale_construction"
require_exact_line_count "$wasm_intl_locale" 'mod construction_lifecycle;' 1 'private Intl.Locale lifecycle attachment'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+construction_lifecycle;' "$wasm_intl_locale"; then
  fail "$wasm_intl_locale must keep construction_lifecycle private"
fi
# The reserved header and initialized branded record are consuming GC states.
# Their tuple fields remain private to the child; native entry wrappers do not
# acquire a constructor-table classification or a raw heap address.
require_exact_line_count "$wasm_intl_locale_construction" \
  'pub(super) struct ReservedIntlLocaleObjectLocal(GcLocal<OrdinaryObject>);' 1 'reserved Locale GC header proof'
require_exact_line_count "$wasm_intl_locale_construction" \
  'pub(super) struct InitializedIntlLocaleObjectLocal(GcLocal<IntlLocaleObject>);' 1 'initialized Locale GC record proof'
if grep -Eq '^#\[derive\([^]]*(Clone|Copy)' "$wasm_intl_locale_construction"; then
  fail "$wasm_intl_locale_construction must not clone or copy consuming lifecycle states"
fi
for lifecycle_transition in emit_reserve_intl_locale_object emit_initialize_intl_locale_object emit_publish_intl_locale_object; do
  require_regex_count "$wasm_intl_locale_construction" \
    "^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+${lifecycle_transition}[[:space:]]*\(" 1 'Locale lifecycle transition owner'
  require_fixed_string_count "$wasm_intl_locale" ".${lifecycle_transition}(" 1 'Locale constructor lifecycle consumer'
done
require_fixed_string_count "$wasm_intl_locale_construction" \
  'reserved: ReservedIntlLocaleObjectLocal,' 1 'consuming Locale initialization input'
require_fixed_string_count "$wasm_intl_locale_construction" \
  'initialized: InitializedIntlLocaleObjectLocal,' 1 'consuming Locale publication input'
require_exact_line_count "$wasm_intl_locale" 'enum LocaleStringSlot {' 1 'private Locale string-slot domain'
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+enum[[:space:]]+LocaleStringSlot' "$wasm_intl_locale"; then
  fail "$wasm_intl_locale must keep LocaleStringSlot private"
fi
require_regex_count "$wasm_intl_locale" \
  '^[[:space:]]*fn[[:space:]]+emit_intl_locale_string_getter[[:space:]]*\(' 1 'private typed Locale string getter'
for intl_locale_entry in language_getter:Language script_getter:Script region_getter:Region base_name_getter:BaseName to_string:Tag; do
  intl_locale_method="${intl_locale_entry%%:*}"
  intl_locale_slot="${intl_locale_entry#*:}"
  require_regex_count "$wasm_intl_locale" \
    "^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+emit_intl_locale_${intl_locale_method}_builtin[[:space:]]*\(" 1 'fixed Locale string entry'
  require_fixed_string_count "$wasm_intl_locale" \
    "self.emit_intl_locale_string_getter(LocaleStringSlot::${intl_locale_slot}," 1 'closed Locale string-slot selection'
  require_fixed_string_count "$wasm_standard_builtins" \
    "self.emit_intl_locale_${intl_locale_method}_builtin(function)?;" 1 'fixed standard Locale string route'
done
if grep -Eq 'LocaleStringSlot|emit_intl_locale_string_getter\(' "$wasm_standard_builtins"; then
  fail "$wasm_standard_builtins must use fixed Locale string entries"
fi
# MAIN129's actual GC owners measure 655 parent and 88 lifecycle raw lines.
# These caps retain the parent/child boundary with a narrow maintenance margin.
check_raw_line_budget "$wasm_intl_locale" 700
check_raw_line_budget "$wasm_intl_locale_construction" 100

wasm_intl_date_time_format="crates/scriptw-aot-wasm/src/builtins/intl_datetimeformat.rs"
wasm_intl_date_time_format_construction="crates/scriptw-aot-wasm/src/builtins/intl_datetimeformat/construction_lifecycle.rs"
wasm_intl_date_time_format_initialization="crates/scriptw-aot-wasm/src/builtins/intl_datetimeformat/initialization.rs"
wasm_intl_date_time_format_time_zone="crates/scriptw-aot-wasm/src/builtins/intl_datetimeformat/time_zone.rs"
require_file "$wasm_intl_date_time_format"
check_no_inline_legacy_includes "$wasm_intl_date_time_format"
# Pattern data and partition algorithms belong to scriptw-intl. The AOT shell
# owns observations, typed GC inputs and current-Realm result records.
for intl_dtf_leaf in construction_lifecycle initialization provider_input provider_render provider_wire time_zone zoned_locale; do
  intl_dtf_leaf_path="crates/scriptw-aot-wasm/src/builtins/intl_datetimeformat/${intl_dtf_leaf}.rs"
  require_file "$intl_dtf_leaf_path"
  require_exact_line_count "$wasm_intl_date_time_format" "mod ${intl_dtf_leaf};" 1 'private DateTimeFormat child attachment'
  check_no_inline_legacy_includes "$intl_dtf_leaf_path"
done
require_module_decl "crates/scriptw-intl/src/lib.rs" "datetime"
require_module_decl "crates/scriptw-intl/src/lib.rs" "datetime_protocol"
require_module_decl "crates/scriptw-intl/src/provider.rs" "datetime"
require_module_decl "crates/scriptw-engine/src/lib.rs" "intl_datetime_host"
require_tree_regex_count crates/scriptw-aot-wasm/src \
  '(struct[[:space:]]+DtfComponentLocals|enum[[:space:]]+DtfRangePattern|enum[[:space:]]+IntlDtfRelevantExtensionKey|const[[:space:]]+INTL_DTF_(MONTHS|WEEKDAYS)_)' 0 \
  'retired DateTimeFormat pattern and locale authority in the Wasm emitter'
intl_time_zone_domain="crates/scriptw-intl/src/time_zone.rs"
require_file "$intl_time_zone_domain"
require_exact_line_count "$intl_time_zone_domain" 'pub enum TimeZoneNameStyle {' 1 'shared time-zone-name style domain'
require_tree_regex_count crates/scriptw-aot-wasm/src 'enum[[:space:]]+TimeZoneNameStyle' 0 'duplicate Wasm time-zone-name style domain'
require_fixed_string_count "$wasm_intl_date_time_format_initialization" \
  'pub(super) zone_name: GcI32DomainLocal<Option<TimeZoneNameStyle>>,' 1 'typed DateTimeFormat time-zone-name option'
require_exact_line_count "$intl_time_zone_domain" 'pub struct FixedTimeZoneOffset(i32);' 1 'validated private fixed-offset representation'
require_exact_line_count "$wasm_intl_date_time_format_time_zone" \
  'pub(super) struct ResolvedDtfTimeZone {' 1 'family-owned completed DateTimeFormat zone'
if grep -Eq '^pub(\(crate\)|\(in [^)]*\))?[[:space:]]+struct[[:space:]]+ResolvedDtfTimeZone' "$wasm_intl_date_time_format_time_zone"; then
  fail "$wasm_intl_date_time_format_time_zone must keep ResolvedDtfTimeZone inside the DateTimeFormat family"
fi
require_fixed_string_count "$wasm_intl_date_time_format_time_zone" \
  'pub(super) kind: GcI32DomainLocal<TimeZoneKind>,' 1 'closed named/fixed DateTimeFormat zone domain'
require_tree_regex_count crates/scriptw-aot-wasm/src \
  '(struct[[:space:]]+(TzOffsetMinutes|IntlDtfNamedZone)|const[[:space:]]+INTL_DTF_NAMED_ZONES)' 0 \
  'retired constant-offset named-zone authority'
require_exact_line_count "$wasm_intl_date_time_format" 'enum DtfReceiverOperation {' 1 'private DateTimeFormat receiver domain'
require_regex_count "$wasm_intl_date_time_format" \
  '^[[:space:]]*fn[[:space:]]+emit_dtf_record_from_receiver[[:space:]]*\(' 1 'private branded GC DateTimeFormat receiver admission'
# Initialization consumes the reserved GC header. Its sibling needs the
# private qualified import and family-visible header field; it publishes the
# completed IntlDateTimeFormatObject directly through whole Completion.
require_exact_line_count "$wasm_intl_date_time_format_construction" \
  'pub(super) struct ReservedIntlDateTimeFormatObjectLocal(pub(super) GcLocal<OrdinaryObject>);' 1 'family-owned DateTimeFormat reserved GC header'
require_exact_line_count "$wasm_intl_date_time_format_initialization" \
  'use construction_lifecycle::ReservedIntlDateTimeFormatObjectLocal;' 1 'private initialization lifecycle import'
if grep -Eq '^[[:space:]]*pub(\([^)]*\))?[[:space:]]+use.*construction_lifecycle' \
  "$wasm_intl_date_time_format" "$wasm_intl_date_time_format_initialization"; then
  fail 'DateTimeFormat reserved lifecycle state must not be re-exported'
fi
if grep -Eq '^#\[derive\([^]]*(Clone|Copy)' "$wasm_intl_date_time_format_construction"; then
  fail "$wasm_intl_date_time_format_construction must not clone or copy its reserved header"
fi
for lifecycle_transition in emit_reserve_intl_date_time_format_object emit_reserve_intrinsic_date_time_format_object; do
  require_regex_count "$wasm_intl_date_time_format_construction" \
    "^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+${lifecycle_transition}[[:space:]]*\(" 1 'DateTimeFormat header reservation owner'
  require_fixed_string_count "$wasm_intl_date_time_format_initialization" ".${lifecycle_transition}(" 1 'DateTimeFormat purpose-specific reservation'
done
require_regex_count "$wasm_intl_date_time_format_initialization" \
  '^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+emit_dtf_complete_initialization[[:space:]]*\(' 1 'consuming DateTimeFormat initialization owner'
require_regex_count "$wasm_intl_date_time_format_initialization" \
  '^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_intl_create_date_time_format[[:space:]]*\(' 1 'whole Completion DateTimeFormat creation entry'
require_fixed_string_count "$wasm_intl_date_time_format_initialization" \
  'schema.struct_type::<IntlDateTimeFormatObject>().construct(' 1 'branded GC DateTimeFormat record publication'
# After the announced formatting overlay: parent 510, initialization 895,
# lifecycle 84 raw lines. Typed initialization now owns the complete GC recipe.
check_raw_line_budget "$wasm_intl_date_time_format" 550
check_raw_line_budget "$wasm_intl_date_time_format_initialization" 925
check_raw_line_budget "$wasm_intl_date_time_format_construction" 100
require_module_decl "$wasm_date_builtins" "locale_string"
check_raw_line_budget "crates/scriptw-aot-wasm/src/builtins/date/locale_string.rs" 115
# Observable reservation/order/Realm behavior stays covered by real Engine
# and CLI controls; this source gate does not execute those controls.
for intl_witness in \
  crates/scriptw-engine/tests/aot_intl/aot_intl_locale_constructor.rs \
  crates/scriptw-engine/tests/aot_intl/aot_intl_locale_likely_subtags.rs \
  crates/scriptw-engine/tests/aot_intl/aot_intl_datetime_provider.rs \
  crates/scriptw-engine/tests/aot_intl/aot_intl_datetime_range_endpoints.rs \
  crates/scriptw-cli/tests/cli/intl.rs \
  crates/scriptw-cli/tests/fixtures/wasm_intl_locale_construction_order.js \
  crates/scriptw-cli/tests/fixtures/wasm_intl_date_time_format_construction_order.js; do
  require_file "$intl_witness"
done
for intl_cli_witness in \
  run_wasm_intl_locale_construction_order_fixture_succeeds \
  run_wasm_intl_date_time_format_construction_order_fixture_succeeds \
  run_wasm_intl_canonical_locale_tag_roles_fixture_succeeds; do
  require_regex_count crates/scriptw-cli/tests/cli/intl.rs \
    "^fn ${intl_cli_witness}[[:space:]]*\(" 1 'real CLI Intl ownership/order witness'
done

wasm_global_numeric_builtins="crates/scriptw-aot-wasm/src/builtins/global_numeric.rs"
check_no_inline_legacy_includes "$wasm_global_numeric_builtins"
if ! grep -q '^enum GlobalNumericBuiltin' "$wasm_global_numeric_builtins" \
  || ! grep -q '^        match builtin {' "$wasm_global_numeric_builtins"; then
  fail "$wasm_global_numeric_builtins must dispatch through the closed GlobalNumericBuiltin domain"
fi
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+enum[[:space:]]+GlobalNumericBuiltin' "$wasm_global_numeric_builtins"; then
  fail "$wasm_global_numeric_builtins must keep GlobalNumericBuiltin private"
fi
if grep -Eq 'GlobalNumericBuiltin|emit_global_numeric_builtin\(' "$wasm_standard_builtins"; then
  fail "$wasm_standard_builtins must use fixed global numeric entries"
fi
require_fixed_string_count \
  "$wasm_standard_builtins" \
  'self.emit_global_is_finite_builtin(function)?' \
  1 \
  'fixed global isFinite delegate'
require_fixed_string_count \
  "$wasm_standard_builtins" \
  'self.emit_global_is_nan_builtin(function)?' \
  1 \
  'fixed global isNaN delegate'
require_fixed_string_count \
  "$wasm_global_numeric_builtins" \
  'self.emit_global_numeric_builtin(' \
  2 \
  'private global numeric producer calls'
# Measured immediately after extraction: 51 raw lines. The narrow margin is
# for maintenance of this family, not adjacent builtin implementations.
check_raw_line_budget "$wasm_global_numeric_builtins" 90

wasm_symbol_builtins="crates/scriptw-aot-wasm/src/builtins/symbol.rs"
check_no_inline_legacy_includes "$wasm_symbol_builtins"
if ! grep -q '^enum SymbolBuiltin' "$wasm_symbol_builtins" \
  || ! grep -q '^        match builtin {' "$wasm_symbol_builtins"; then
  fail "$wasm_symbol_builtins must dispatch through the closed SymbolBuiltin domain"
fi
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+enum[[:space:]]+SymbolBuiltin' "$wasm_symbol_builtins"; then
  fail "$wasm_symbol_builtins must keep SymbolBuiltin private"
fi
if grep -Eq 'SymbolBuiltin|SymbolFn|emit_symbol\(' "$wasm_standard_builtins"; then
  fail "$wasm_standard_builtins must use fixed Symbol entries"
fi
require_fixed_string_count "$wasm_symbol_builtins" 'fn emit_symbol(' 1 'private Symbol emitter'
require_fixed_string_count "$wasm_symbol_builtins" 'self.emit_symbol(' 7 'fixed Symbol entry calls'
# Measured immediately after extraction: 518 raw lines. The narrow margin is
# for maintenance of this family, not adjacent builtin implementations.
check_raw_line_budget "$wasm_symbol_builtins" 550

wasm_uri_builtins="crates/scriptw-aot-wasm/src/builtins/uri.rs"
check_no_inline_legacy_includes "$wasm_uri_builtins"
if ! grep -q '^enum UriBuiltin' "$wasm_uri_builtins" \
  || ! grep -q '^        match builtin {' "$wasm_uri_builtins"; then
  fail "$wasm_uri_builtins must privately dispatch through the closed UriBuiltin domain"
fi
if grep -Fq 'UriBuiltin' "$wasm_standard_builtins" \
  || grep -Fq 'self.emit_uri_builtin(' "$wasm_standard_builtins"; then
  fail "$wasm_standard_builtins must use fixed URI operations instead of the raw policy"
fi
require_fixed_string_count "$wasm_uri_builtins" 'fn emit_uri_builtin(' 1 'private URI compiler'
require_fixed_string_count "$wasm_uri_builtins" 'self.emit_uri_builtin(' 6 'fixed URI wrapper calls'
require_regex_count \
  "$wasm_uri_builtins" \
  '^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+emit_(escape|unescape|encode_uri|encode_uri_component|decode_uri|decode_uri_component)_builtin[[:space:]]*\(' \
  6 \
  'fixed URI semantic wrappers'
for uri_wrapper in \
  emit_escape_builtin \
  emit_unescape_builtin \
  emit_encode_uri_builtin \
  emit_encode_uri_component_builtin \
  emit_decode_uri_builtin \
  emit_decode_uri_component_builtin
do
  require_fixed_string_count "$wasm_standard_builtins" "self.${uri_wrapper}(function)?" 1 "URI dispatcher call to $uri_wrapper"
done
# The whole UTF-16 codec implementation now lives here (865 formatted lines),
# including validation and completion cleanup rather than six raw wrappers.
check_raw_line_budget "$wasm_uri_builtins" 900
require_file crates/scriptw-engine/tests/aot_gc_entries/aot_gc_uri_entries.rs

wasm_error_builtins="crates/scriptw-aot-wasm/src/builtins/errors.rs"
require_file "$wasm_error_builtins"
require_regex_count "$wasm_error_builtins" '^enum ErrorBuiltin[[:space:]]*\{' 1 'private closed Error-family domain'
require_regex_count "$wasm_error_builtins" '^[[:space:]]*fn emit_error_builtin\(' 1 'private Error-family compiler'
error_dispatch_body="$(braced_rust_item_source "$wasm_error_builtins" '^fn[[:space:]]+emit_error_builtin[[:space:]]*[(]')"
require_text_regex_count "$error_dispatch_body" '^[[:space:]]*match builtin[[:space:]]*\{' 1 'exhaustive Error-family dispatch'
if grep -Eq '^[[:space:]]*_ =>' <<<"$error_dispatch_body"; then
  fail 'Error-family operation and constructor-kind dispatch must remain exhaustive'
fi
if grep -Eq 'ErrorBuiltin|NativeErrorKind|self\.emit_error_builtin\(' "$wasm_standard_builtins"; then
  fail "$wasm_standard_builtins must use fixed Error-family operations instead of the raw policy"
fi
for error_wrapper in \
  emit_error_constructor_builtin \
  emit_error_is_error_builtin \
  emit_eval_error_constructor_builtin \
  emit_aggregate_error_constructor_builtin \
  emit_suppressed_error_constructor_builtin \
  emit_range_error_constructor_builtin \
  emit_syntax_error_constructor_builtin \
  emit_type_error_constructor_builtin \
  emit_uri_error_constructor_builtin \
  emit_reference_error_constructor_builtin \
  emit_error_prototype_to_string_builtin
do
  require_regex_count "$wasm_error_builtins" \
    "^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+${error_wrapper}[[:space:]]*\(" \
    1 "fixed Error-family entry $error_wrapper"
  require_fixed_string_count "$wasm_standard_builtins" "self.${error_wrapper}(function)?" 1 "standard call to fixed Error-family entry $error_wrapper"
done
for error_child in aggregate_error_preparation constructor promise_any prototype_to_string runtime_error; do
  error_child_source="crates/scriptw-aot-wasm/src/builtins/errors/${error_child}.rs"
  require_file "$error_child_source"
  require_exact_line_count "$wasm_error_builtins" "mod ${error_child};" 1 'private Error algorithm child'
  check_no_inline_legacy_includes "$error_child_source"
  if grep -Eq "^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+${error_child};" "$wasm_error_builtins"; then
    fail "$wasm_error_builtins must keep ${error_child} private"
  fi
done
check_no_inline_legacy_includes "$wasm_error_builtins"
wasm_aggregate_error_preparation="crates/scriptw-aot-wasm/src/builtins/errors/aggregate_error_preparation.rs"
wasm_error_constructor="crates/scriptw-aot-wasm/src/builtins/errors/constructor.rs"
wasm_promise_any_error="crates/scriptw-aot-wasm/src/builtins/errors/promise_any.rs"
require_tree_regex_count crates/scriptw-aot-wasm/src 'aggregate_error_preparation::' 0 'AggregateError preparation imports or re-exports'
require_regex_count "$wasm_aggregate_error_preparation" \
  '^pub\(super\) struct PreparedAggregateErrorLocal[(]' 1 'opaque prepared AggregateError owner'
require_fixed_string_count "$wasm_error_builtins" 'PreparedAggregateErrorLocal' 0 'prepared AggregateError parent names'
for aggregate_error_preparation_method in emit_prepare_aggregate_error_instance emit_finish_aggregate_error_instance emit_aggregate_error_iterable_to_list; do
  require_regex_count "$wasm_aggregate_error_preparation" \
    "^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+${aggregate_error_preparation_method}[[:space:]]*\(" \
    1 "$aggregate_error_preparation_method private-child owner"
done
# The prepared object is consumed only after message/cause preparation and the
# actual shared IteratorToList walk. Promise.any has its own retained-Realm path.
aggregate_error_finish_body="$(braced_rust_item_source "$wasm_aggregate_error_preparation" '^pub[(]super[)][[:space:]]+fn[[:space:]]+emit_finish_aggregate_error_instance[[:space:]]*[(]')"
require_text_regex_count "$aggregate_error_finish_body" '^[[:space:]]*prepared: PreparedAggregateErrorLocal,' 1 'consuming AggregateError completion boundary'
require_regex_count "$wasm_error_constructor" '^pub\(super\) struct PreparedNativeErrorInstance[(]' 1 'opaque unpublished native Error owner'
require_regex_count "$wasm_error_constructor" '^[[:space:]]*pub\(super\)[[:space:]]+fn publish\(' 1 'consuming native Error publication owner'
if grep -Eq 'struct PreparedAggregateErrorLocal[(][[:space:]]*pub' "$wasm_aggregate_error_preparation" \
  || grep -Eq 'struct PreparedNativeErrorInstance[(][[:space:]]*pub' "$wasm_error_constructor"; then
  fail 'prepared Error publication authorities must keep their tuple fields private'
fi
require_regex_count "$wasm_promise_any_error" \
  '^[[:space:]]*pub\(in crate::builtins\)[[:space:]]+fn emit_promise_any_aggregate_error\(' 1 'retained-Realm Promise.any AggregateError owner'
# Raw scalar fields and recursive proof/projection counts do not constrain the
# concrete GC record or consuming publication path.
# Actual MAIN129 owner sizes: Error parent 346, preparation 73, constructor 210.
check_raw_line_budget "$wasm_error_builtins" 400
check_raw_line_budget "$wasm_aggregate_error_preparation" 100
check_raw_line_budget "$wasm_error_constructor" 250
wasm_error_stack="crates/scriptw-aot-wasm/src/builtins/errors/stack.rs"
require_file "$wasm_error_stack"
require_exact_line_count "$wasm_error_builtins" 'mod stack;' 1 'private Error stack owner'
require_fixed_string_count "$wasm_error_stack" 'NativeErrorObjectSchema::STACK' 1 'getter reads native ErrorData storage'
require_fixed_string_count "$wasm_error_constructor" 'self.emit_append_error_data(header, "stack"' 0 'constructors do not shadow the inherited stack accessor'
check_raw_line_budget "$wasm_error_stack" 200

wasm_iterator_helpers="crates/scriptw-aot-wasm/src/builtins/standard/iterator_helpers.rs"
wasm_iterator_buffered="crates/scriptw-aot-wasm/src/builtins/standard/iterator_helpers/buffered.rs"
require_file "$wasm_iterator_buffered"
require_exact_line_count "$wasm_iterator_helpers" 'mod buffered;' 1 'private buffered helper owner'
for iterator_buffer_child in buffer_queue buffer_resume; do
  iterator_buffer_path="crates/scriptw-aot-wasm/src/builtins/standard/iterator_helpers/buffered/$iterator_buffer_child.rs"
  require_file "$iterator_buffer_path"
  require_exact_line_count "$wasm_iterator_buffered" "mod $iterator_buffer_child;" 1 'private buffered helper child'
  check_no_inline_legacy_includes "$iterator_buffer_path"
  check_raw_line_budget "$iterator_buffer_path" 225
done
check_raw_line_budget "$wasm_iterator_buffered" 225

wasm_promise_builtins="crates/scriptw-aot-wasm/src/builtins/promise.rs"
require_file "$wasm_promise_builtins"
check_no_inline_legacy_includes "$wasm_promise_builtins"
for promise_child in \
  promise_internal_function_materialization \
  promise_try_callback_type_error \
  promise_prototype_receiver_type_error \
  promise_prototype_then_invocation \
  promise_settlement_record_allocation \
  promise_with_resolvers_result_allocation
do
  promise_child_source="crates/scriptw-aot-wasm/src/builtins/promise/${promise_child}.rs"
  require_file "$promise_child_source"
  check_no_inline_legacy_includes "$promise_child_source"
  require_exact_line_count "$wasm_promise_builtins" "mod ${promise_child};" 1 'private Promise lifecycle child'
  if grep -Eq "^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+${promise_child};" "$wasm_promise_builtins"; then
    fail "$wasm_promise_builtins must keep ${promise_child} private"
  fi
  if grep -Eq "^pub([^[:space:]]*[[:space:]]+)?use[[:space:]]+.*${promise_child}" "$wasm_promise_builtins"; then
    fail "$wasm_promise_builtins must not re-export ${promise_child} lifecycle authorities"
  fi
done
wasm_promise_internal_function_materialization="crates/scriptw-aot-wasm/src/builtins/promise/promise_internal_function_materialization.rs"
wasm_promise_try_callback_type_error="crates/scriptw-aot-wasm/src/builtins/promise/promise_try_callback_type_error.rs"
wasm_promise_prototype_receiver_type_error="crates/scriptw-aot-wasm/src/builtins/promise/promise_prototype_receiver_type_error.rs"
wasm_promise_prototype_then_invocation="crates/scriptw-aot-wasm/src/builtins/promise/promise_prototype_then_invocation.rs"
wasm_promise_settlement_record_allocation="crates/scriptw-aot-wasm/src/builtins/promise/promise_settlement_record_allocation.rs"
wasm_promise_with_resolvers_result_allocation="crates/scriptw-aot-wasm/src/builtins/promise/promise_with_resolvers_result_allocation.rs"
require_regex_count "$wasm_promise_builtins" \
  '^use self::promise_internal_function_materialization::' 1 'private Promise materialization authority import'
require_regex_count "$wasm_promise_internal_function_materialization" \
  '^pub\(crate\) struct PromiseInternalFunctionMaterializationContext[(]' 1 'opaque Promise materialization context owner'
if grep -Eq 'struct PromiseInternalFunctionMaterializationContext[(][[:space:]]*pub' "$wasm_promise_internal_function_materialization"; then
  fail 'Promise materialization must keep its retained Realm context private'
fi
require_regex_count "$wasm_promise_internal_function_materialization" \
  "^pub\(super\) enum PromiseInternalFunction<'a>" 1 'closed typed Promise capture admission owner'
require_regex_count "$wasm_promise_internal_function_materialization" \
  '^[[:space:]]*fn publication\(' 1 'private exhaustive Promise publication selector'
promise_capture_selection="$(braced_rust_item_source "$wasm_promise_internal_function_materialization" '^fn[[:space:]]+publication[[:space:]]*[(]')"
if grep -Eq '^[[:space:]]*_ =>' <<<"$promise_capture_selection"; then
  fail 'Promise internal-function capture publication must remain exhaustive'
fi
for promise_materialization_factory in \
  emit_promise_internal_function_materialization_context_from_realm \
  emit_current_function_promise_internal_function_materialization_context \
  release_promise_internal_function_materialization_context
do
  require_regex_count "$wasm_promise_internal_function_materialization" \
    "^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+${promise_materialization_factory}[[:space:]]*\(" \
    1 "Promise materialization owner $promise_materialization_factory"
done
require_regex_count "$wasm_promise_internal_function_materialization" \
  '^[[:space:]]*pub\(super\)[[:space:]]+fn emit_promise_internal_function_value\(' 1 'typed Promise internal-function publication owner'
promise_publication_body="$(braced_rust_item_source "$wasm_promise_internal_function_materialization" '^pub[(]super[)][[:space:]]+fn[[:space:]]+emit_promise_internal_function_value[[:space:]]*[(]')"
require_text_regex_count "$promise_publication_body" \
  "^[[:space:]]*entry: PromiseInternalFunction<'_>," 1 'closed typed Promise capture admission boundary'
require_text_regex_count "$promise_publication_body" \
  '^[[:space:]]*context: &PromiseInternalFunctionMaterializationContext,' 1 'retained Realm materialization boundary'
if grep -q 'materialization_context.realm_local' crates/scriptw-aot-wasm/src/builtins/promise/promise_resolve_realm_context.rs; then
  fail 'PromiseResolve must load the materialization Realm through the child-owned capability'
fi

# Opaque authorities carry complete values and are consumed by their fixed
# child operations. No scalar layout, construction/projection or recursive-use
# census is required to describe those typed lifecycle boundaries.
for promise_opaque_owner in \
  'promise_try_callback_type_error PromiseTryCallbackTypeErrorPrototypeLocal' \
  'promise_prototype_receiver_type_error PromisePrototypeReceiverTypeErrorPrototypeLocal' \
  'promise_prototype_then_invocation ValidatedPromisePrototypeThenInvocationLocals' \
  'promise_settlement_record_allocation PromiseSettlementRecordAllocationContext' \
  'promise_with_resolvers_result_allocation PromiseWithResolversResultAllocationContext'
do
  set -- $promise_opaque_owner
  promise_opaque_source="crates/scriptw-aot-wasm/src/builtins/promise/${1}.rs"
  require_regex_count "$promise_opaque_source" \
    "^pub\(super\) struct ${2}[[:space:]]*[({]" 1 'opaque Promise lifecycle authority owner'
  require_tree_regex_count crates/scriptw-aot-wasm/src "${1}::" 0 'Promise lifecycle authority imports or re-exports'
  require_fixed_string_count "$wasm_promise_builtins" "$2" 0 'Promise lifecycle authority parent names'
  if grep -Eq '^[[:space:]]+pub(\([^)]*\))?[[:space:]]+[[:alnum:]_]+:' "$promise_opaque_source" \
    || grep -Eq "struct ${2}[(][[:space:]]*pub" "$promise_opaque_source"; then
    fail "$promise_opaque_source must keep lifecycle authority fields private"
  fi
done
require_regex_count "$wasm_promise_prototype_receiver_type_error" \
  '^enum PromisePrototypeReceiverError[[:space:]]*\{' 1 'private closed Promise receiver-error domain'
if grep -Eq 'PromisePrototypeReceiverError|emit_throw_promise_prototype_receiver_error\(' "$wasm_promise_builtins"; then
  fail 'Promise receiver errors must consume fixed child operations'
fi
for promise_lifecycle_method_owner in \
  'promise_try_callback_type_error emit_load_promise_try_callback_type_error_prototype' \
  'promise_try_callback_type_error emit_throw_promise_try_non_callable_callback' \
  'promise_prototype_receiver_type_error emit_load_promise_prototype_receiver_type_error_prototype' \
  'promise_prototype_receiver_type_error emit_throw_promise_then_incompatible_receiver_error' \
  'promise_prototype_receiver_type_error emit_throw_promise_finally_non_object_receiver_error' \
  'promise_prototype_then_invocation emit_validate_promise_prototype_then_invocation' \
  'promise_prototype_then_invocation emit_call_validated_promise_prototype_then_invocation' \
  'promise_settlement_record_allocation emit_self_backed_promise_settlement_record_allocation_context' \
  'promise_settlement_record_allocation emit_alloc_promise_settlement_record' \
  'promise_with_resolvers_result_allocation emit_current_function_promise_with_resolvers_result_allocation_context' \
  'promise_with_resolvers_result_allocation emit_alloc_promise_with_resolvers_result'
do
  set -- $promise_lifecycle_method_owner
  promise_lifecycle_source="crates/scriptw-aot-wasm/src/builtins/promise/${1}.rs"
  require_regex_count "$promise_lifecycle_source" \
    "^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+${2}[[:space:]]*\(" \
    1 'fixed Promise lifecycle child operation'
  require_regex_count "$wasm_promise_builtins" \
    "^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+${2}[[:space:]]*\(" \
    0 'no parent Promise lifecycle operation copy'
done
promise_then_call_body="$(braced_rust_item_source "$wasm_promise_prototype_then_invocation" '^pub[(]super[)][[:space:]]+fn[[:space:]]+emit_call_validated_promise_prototype_then_invocation[[:space:]]*[(]')"
require_text_regex_count "$promise_then_call_body" \
  '^[[:space:]]*invocation: ValidatedPromisePrototypeThenInvocationLocals,' 1 'observed then Reference consumed by its call'
for promise_allocation_boundary in \
  'promise_settlement_record_allocation emit_alloc_promise_settlement_record PromiseSettlementRecordAllocationContext' \
  'promise_with_resolvers_result_allocation emit_alloc_promise_with_resolvers_result PromiseWithResolversResultAllocationContext'
do
  set -- $promise_allocation_boundary
  promise_allocation_source="crates/scriptw-aot-wasm/src/builtins/promise/${1}.rs"
  promise_allocation_body="$(braced_rust_item_source "$promise_allocation_source" "^pub[(]super[)][[:space:]]+fn[[:space:]]+${2}[[:space:]]*[(]")"
  require_text_regex_count "$promise_allocation_body" "^[[:space:]]*context: ${3}," 1 'selected Promise prototype consumed by allocation'
done
# Actual MAIN129 sizes are 4,628/280/40/85/61/65/70 lines respectively.
check_raw_line_budget "$wasm_promise_builtins" 4800
check_raw_line_budget "$wasm_promise_internal_function_materialization" 320
check_raw_line_budget "$wasm_promise_try_callback_type_error" 80
check_raw_line_budget "$wasm_promise_prototype_receiver_type_error" 120
check_raw_line_budget "$wasm_promise_prototype_then_invocation" 80
check_raw_line_budget "$wasm_promise_settlement_record_allocation" 100
check_raw_line_budget "$wasm_promise_with_resolvers_result_allocation" 110

wasm_json_builtins="crates/scriptw-aot-wasm/src/builtins/json.rs"
wasm_json_parse_frame_state="crates/scriptw-aot-wasm/src/builtins/json/parse_frame_state.rs"
wasm_json_parse="crates/scriptw-aot-wasm/src/builtins/json/parse.rs"
wasm_json_reviver="crates/scriptw-aot-wasm/src/builtins/json/reviver.rs"
wasm_json_stringify="crates/scriptw-aot-wasm/src/builtins/json/stringify.rs"
require_file "$wasm_json_builtins"
check_no_inline_legacy_includes "$wasm_json_builtins"
for json_child in grammar parse parse_array parse_frame_state quote reviver string_output stringify stringify_replacer; do
  json_child_source="crates/scriptw-aot-wasm/src/builtins/json/${json_child}.rs"
  require_file "$json_child_source"
  check_no_inline_legacy_includes "$json_child_source"
  require_exact_line_count "$wasm_json_builtins" "mod ${json_child};" 1 'private JSON algorithm child'
  if grep -Eq "^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+${json_child};" "$wasm_json_builtins"; then
    fail "$wasm_json_builtins must keep ${json_child} private"
  fi
done
# The private parser imports only its closed metadata-key discriminator. This
# carries no frame or record authority; all other qualified owner references,
# including aliases/re-exports and copies in other modules, remain forbidden.
json_child_key_import='use super::parse_frame_state::JsonParseChildKey;'
require_exact_line_count "$wasm_json_parse" "$json_child_key_import" 1 'private JSON metadata key import'
require_exact_line_count "$wasm_json_parse_frame_state" "pub(super) enum JsonParseChildKey<'a> {" 1 'child-owned metadata key domain'
require_exact_line_count "$wasm_json_parse_frame_state" "    ObjectKey(&'a GcLocal<StringValue>)," 1 'duplicate-capable object metadata key'
require_exact_line_count "$wasm_json_parse_frame_state" '    FreshArrayIndex(I64Local),' 1 'fresh sequential array metadata key'
json_frame_owner_references="$(grep -RH --include='*.rs' 'parse_frame_state::' crates/scriptw-aot-wasm/src || true)"
json_unregistered_frame_owner_references="$(printf '%s\n' "$json_frame_owner_references" | grep -Fvx "$wasm_json_parse:$json_child_key_import" || true)"
require_text_regex_count "$json_unregistered_frame_owner_references" 'parse_frame_state::' 0 'JSON frame-owner imports or re-exports outside the exact metadata key seam'
require_regex_count "$wasm_json_builtins" '^enum JsonBuiltin[[:space:]]*\{' 1 'private closed JSON operation domain'
require_regex_count "$wasm_json_builtins" '^[[:space:]]*fn emit_json_builtin\(' 1 'private exhaustive JSON dispatcher'
json_dispatch_body="$(braced_rust_item_source "$wasm_json_builtins" '^fn[[:space:]]+emit_json_builtin[[:space:]]*[(]')"
require_text_regex_count "$json_dispatch_body" '^[[:space:]]*match operation[[:space:]]*\{' 1 'exhaustive JSON operation dispatch'
if grep -Eq '^[[:space:]]*_ =>' <<<"$json_dispatch_body"; then
  fail 'JSON operation dispatch must remain exhaustive'
fi
if grep -Eq 'JsonBuiltin|emit_json_builtin\(' "$wasm_standard_builtins"; then
  fail "$wasm_standard_builtins must consume four fixed JSON operations"
fi
for json_builtin_wrapper in emit_json_parse_builtin emit_json_stringify_builtin emit_json_raw_json_builtin emit_json_is_raw_json_builtin; do
  require_regex_count "$wasm_json_builtins" \
    "^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+${json_builtin_wrapper}[[:space:]]*\(" \
    1 'fixed JSON entry owner'
  require_fixed_string_count "$wasm_standard_builtins" "self.${json_builtin_wrapper}(function)?" 1 'standard JSON route'
done
# Closed domains and concrete rooted records replace the scalar validated-state
# carrier and linear stack. Preserve each private semantic operation owner.
for json_state_domain in JsonParseFrameState JsonReviverFrameState JsonReviverPropertyRole; do
  require_regex_count "$wasm_json_builtins" "^json_domain!\(${json_state_domain}[[:space:]]*\{" 1 'closed JSON frame domain'
done
for json_frame_method in emit_json_parse_frame emit_json_parse_state emit_json_parse_record emit_json_parse_child; do
  require_regex_count "$wasm_json_parse_frame_state" \
    "^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+${json_frame_method}[[:space:]]*\(" \
    1 'private JSON frame lifecycle owner'
done
require_regex_count "$wasm_json_parse" '^[[:space:]]*fn emit_json_parse_text\(' 1 'private runtime JSON parser owner'
require_regex_count "$wasm_json_parse" '^[[:space:]]*fn emit_json_complete_container\(' 1 'completed JSON container publication owner'
require_regex_count "$wasm_json_reviver" '^[[:space:]]*pub\(super\)[[:space:]]+fn emit_json_revive\(' 1 'rooted iterative JSON reviver owner'
for json_entry_owner in 'parse emit_json_parse_entry' 'parse emit_json_raw_entry' 'stringify emit_json_stringify_entry'; do
  set -- $json_entry_owner
  json_entry_source="crates/scriptw-aot-wasm/src/builtins/json/${1}.rs"
  require_regex_count "$json_entry_source" \
    "^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+${2}[[:space:]]*\(" \
    1 'private JSON semantic entry owner'
  require_fixed_string_count "$wasm_json_builtins" "self.${2}(" 1 'fixed JSON entry consumption'
done
# Parent/frame/parse/reviver/stringify MAIN129 sizes: 324/157/494/494/898.
check_raw_line_budget "$wasm_json_builtins" 360
check_raw_line_budget "$wasm_json_parse_frame_state" 200
check_raw_line_budget "$wasm_json_parse" 550
wasm_json_parse_array="crates/scriptw-aot-wasm/src/builtins/json/parse_array.rs"
require_exact_line_count "$wasm_json_parse_array" 'pub(super) struct JsonArrayDataDescriptors {' 1 'private undefined data-descriptor authority'
require_exact_line_count "$wasm_json_parse_array" "struct FreshJsonArrayElement<'a> {" 1 'private fresh JSON frame insertion owner'
require_regex_count "$wasm_json_parse_array" '^[[:space:]]*fn from_frame\(' 1 'sole private JSON frame insertion constructor'
require_regex_count "$wasm_json_parse_array" '^[[:space:]]*pub\(super\) fn emit_json_parse_array_append\(' 1 'fixed fresh JSON array append entry'
require_fixed_string_count "$wasm_json_parse" 'self.emit_json_parse_array_append(&frame, &stored, &array_descriptors, ordinal, f)?;' 1 'fresh frame append consumes the original immutable StoredValue'
check_no_inline_legacy_includes "$wasm_json_parse_array"
check_raw_line_budget "$wasm_json_parse_array" 220
check_raw_line_budget "$wasm_json_reviver" 550
check_raw_line_budget "$wasm_json_stringify" 950
wasm_json_output="crates/scriptw-aot-wasm/src/builtins/json/string_output.rs"
require_fixed_string_count "$wasm_json_output" 'pub(super) struct StringOutputConstruction' 1 'private consuming JSON output owner'
check_raw_line_budget "$wasm_json_output" 180
wasm_json_replacer="crates/scriptw-aot-wasm/src/builtins/json/stringify_replacer.rs"
require_fixed_string_count "$wasm_json_replacer" 'pub(super) struct JsonStringifyReplacerInvocationLocals' 1 'one private JSON replacer role authority'
require_fixed_string_count "$wasm_json_replacer" 'pub(super) fn emit_json_apply_replacer_with_this(' 1 'one consuming JSON replacer boundary'
require_fixed_string_count "$wasm_json_stringify" 'self.emit_json_apply_replacer_with_this(' 1 'shared SerializeJSONProperty replacer producer'
check_raw_line_budget "$wasm_json_replacer" 120

# Real finite semantic witnesses retain their live registration and fixtures.
# These controls exercise the compiler/Engine/CLI rather than count old ABI
# fields, proof projections or recursive emitter calls.
for semantic_control in \
  'crates/scriptw-engine/tests/aot_builtins/aot_aggregate_error_iterator.rs aggregate_error_preserves_prefix_order_and_never_closes_protocol_abrupts' \
  'crates/scriptw-engine/tests/aot_realm_modules/aot_aggregate_error_constructor_realm.rs prototype_getters_preserve_objects_and_abrupt_completions' \
  'crates/scriptw-engine/tests/aot_builtins/aot_promise_combinator_intrinsics.rs any_uses_the_canonical_aggregate_error_for_empty_and_rejected_inputs' \
  'crates/scriptw-engine/tests/aot_builtins/aot_promise_combinator_intrinsics.rs borrowed_foreign_combinators_separate_method_and_capability_realms' \
  'crates/scriptw-engine/tests/aot_builtins/aot_json_canonical_reviver.rs source_context_uses_same_value_and_final_duplicate_source_without_static_values' \
  'crates/scriptw-engine/tests/aot_builtins/aot_json_reviver_definitions.rs reviver_propagates_proxy_definition_throws_through_catch_and_finally' \
  'crates/scriptw-engine/tests/aot_builtins/aot_json_stringify_preparation.rs gc_serialization_retains_context_path_lists_callbacks_and_abrupt_values' \
  'crates/scriptw-cli/tests/cli/functions.rs run_wasm_backend_publishes_created_realm_promise_foundation' \
  'crates/scriptw-cli/tests/cli/functions.rs run_wasm_backend_preserves_created_realm_promise_internal_callbacks' \
  'crates/scriptw-cli/tests/cli/language_numerics.rs run_wasm_backend_succeeds_for_json_parse_dynamic_reviver_frame_fixture' \
  'crates/scriptw-cli/tests/cli/language_numerics.rs run_wasm_backend_preserves_json_stringify_replacer_invocation_roles'
do
  set -- $semantic_control
  require_file "$1"
  require_regex_count "$1" "^fn[[:space:]]+${2}[[:space:]]*\(" 1 'finite Error/Promise/JSON semantic test owner'
  semantic_control_attributes="$(rust_item_attributes_source "$1" "^fn[[:space:]]+${2}[[:space:]]*[(]")"
  require_text_regex_count "$semantic_control_attributes" '^#\[test\]$' 1 'live Error/Promise/JSON semantic test registration'
  if grep -Eq '^#\[(cfg|cfg_attr|ignore)([^[:alnum:]_]|$)' <<<"$semantic_control_attributes"; then
    fail "$1 ${2} semantic witness must remain enabled"
  fi
done
for semantic_fixture_binding in \
  'crates/scriptw-engine/tests/aot_builtins/aot_aggregate_error_iterator.rs aggregate_error_preserves_prefix_order_and_never_closes_protocol_abrupts ../fixtures/aggregate_error_iterator/ordering_and_abrupt.js' \
  'crates/scriptw-engine/tests/aot_builtins/aot_json_canonical_reviver.rs source_context_uses_same_value_and_final_duplicate_source_without_static_values ../fixtures/json_canonical_reviver/source_context_mutation.js' \
  'crates/scriptw-engine/tests/aot_builtins/aot_json_stringify_preparation.rs gc_serialization_retains_context_path_lists_callbacks_and_abrupt_values ../fixtures/json_stringify_preparation/gc_roots_and_abrupt.js' \
  'crates/scriptw-cli/tests/cli/functions.rs run_wasm_backend_publishes_created_realm_promise_foundation wasm_promise_created_realm.js' \
  'crates/scriptw-cli/tests/cli/functions.rs run_wasm_backend_preserves_created_realm_promise_internal_callbacks wasm_promise_internal_callback_realm.js' \
  'crates/scriptw-cli/tests/cli/language_numerics.rs run_wasm_backend_succeeds_for_json_parse_dynamic_reviver_frame_fixture wasm_json_parse_dynamic_reviver_frame.js' \
  'crates/scriptw-cli/tests/cli/language_numerics.rs run_wasm_backend_preserves_json_stringify_replacer_invocation_roles wasm_json_stringify_replacer_invocation_roles.js'
do
  set -- $semantic_fixture_binding
  semantic_witness_body="$(braced_rust_item_source "$1" "^fn[[:space:]]+${2}[[:space:]]*[(]")"
  if ! grep -Fq "\"${3}\"" <<<"$semantic_witness_body"; then
    fail "$1 ${2} must consume its actual semantic fixture ${3}"
  fi
done
for semantic_fixture in \
  crates/scriptw-engine/tests/fixtures/aggregate_error_iterator/ordering_and_abrupt.js \
  crates/scriptw-engine/tests/fixtures/aggregate_error_iterator/realms_and_protocol.js \
  crates/scriptw-engine/tests/fixtures/json_canonical_reviver/source_context_mutation.js \
  crates/scriptw-engine/tests/fixtures/json_stringify_preparation/gc_roots_and_abrupt.js \
  crates/scriptw-cli/tests/fixtures/wasm_promise_internal_callback_realm.js \
  crates/scriptw-cli/tests/fixtures/wasm_promise_created_realm.js \
  crates/scriptw-cli/tests/fixtures/wasm_json_parse_dynamic_reviver_frame.js \
  crates/scriptw-cli/tests/fixtures/wasm_json_stringify_replacer_invocation_roles.js
do
  require_file "$semantic_fixture"
done
# T02's strong Map get-or-insert owner. Its two crate-visible semantic entry
# points remain product-callable, but only the private child may construct the
# raw value-source policy or call the shared parameterized emitter. WeakMap's
# collector-backed weak storage is owned by weak, independently of this strong algorithm.
wasm_collections_builtins="crates/scriptw-aot-wasm/src/builtins/collections.rs"
wasm_map_get_or_insert="crates/scriptw-aot-wasm/src/builtins/collections/map_get_or_insert.rs"
require_file "$wasm_map_get_or_insert"
check_no_inline_legacy_includes "$wasm_collections_builtins"
check_no_inline_legacy_includes "$wasm_map_get_or_insert"
require_exact_line_count \
  "$wasm_collections_builtins" \
  'mod map_get_or_insert;' \
  1 \
  'private Map get-or-insert module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+map_get_or_insert;' "$wasm_collections_builtins"; then
  fail "$wasm_collections_builtins must keep map_get_or_insert private"
fi
if grep -Eq 'MapGetOrInsertValueSource|emit_collection_get_or_insert\(|map_get_or_insert::' "$wasm_collections_builtins"; then
  fail "$wasm_collections_builtins must not name, construct, project or import the private Map get-or-insert policy"
fi
require_fixed_string_count \
  "$wasm_map_get_or_insert" \
  'MapGetOrInsertValueSource' \
  8 \
  'Map get-or-insert value-source owner lines'
require_fixed_string_count \
  "$wasm_map_get_or_insert" \
  'MapGetOrInsertValueSource::' \
  6 \
  'Map get-or-insert qualified value-source uses'
require_fixed_string_count \
  "$wasm_map_get_or_insert" \
  'emit_collection_get_or_insert(' \
  3 \
  'private Map get-or-insert emitter definition and calls'
for semantic_get_or_insert in \
  emit_map_prototype_get_or_insert \
  emit_map_prototype_get_or_insert_computed
do
  require_regex_count \
    "$wasm_map_get_or_insert" \
    "^[[:space:]]*pub\\(crate\\)[[:space:]]+fn[[:space:]]+$semantic_get_or_insert[[:space:]]*\\(" \
    1 \
    "Map get-or-insert semantic surface $semantic_get_or_insert"
  require_fixed_string_count \
    "$wasm_standard_builtins" \
    "self.$semantic_get_or_insert(function)?;" \
    1 \
    "Map get-or-insert product call $semantic_get_or_insert"
done
# Measured immediately after extraction: 6,491 parent lines and 322 child
# lines. The narrow margins are for maintenance of each owner.
check_raw_line_budget "$wasm_collections_builtins" 6560
check_raw_line_budget "$wasm_map_get_or_insert" 360

# The Temporal record/constructor/accessor vs prototype-method-body boundary.
# GC record schemas own storage; parent files own admission, constructors and
# accessors, and method leaves consume their typed records and completed proofs.
# ZonedDateTime constructors and records are owned by temporal.rs, included below.
for module in temporal temporal_plain_date temporal_plain_date_methods \
              temporal_plain_time temporal_plain_time_methods \
              temporal_plain_date_time temporal_plain_date_time_methods \
              temporal_zoned_date_time_methods; do
  require_file "crates/scriptw-aot-wasm/src/builtins/${module}.rs"
  require_module_decl "$wasm_builtins_mod" "$module"
  check_no_inline_legacy_includes "crates/scriptw-aot-wasm/src/builtins/${module}.rs"
done

wasm_temporal_plain_date="crates/scriptw-aot-wasm/src/builtins/temporal_plain_date.rs"
wasm_temporal_exact="crates/scriptw-aot-wasm/src/builtins/temporal_zone_provider/exact.rs"
require_file "$wasm_temporal_exact"
require_exact_line_count "$wasm_temporal_exact" \
  'pub(crate) struct TemporalCalendarSlotLocals {' 1 'completed Temporal GC calendar slot owner'
temporal_calendar_slot_fields="$(sed -n '/^pub(crate) struct TemporalCalendarSlotLocals {$/,/^}$/p' "$wasm_temporal_exact")"
if grep -Eq '^[[:space:]]+pub(\([^)]*\))?[[:space:]]+' <<<"$temporal_calendar_slot_fields"; then
  fail "$wasm_temporal_exact must keep calendar-slot fields private"
fi
require_regex_count "$wasm_temporal_exact" \
  '^[[:space:]]*pub\(in crate::builtins\)[[:space:]]+fn[[:space:]]+emit_temporal_calendar_slot_from_identifier[[:space:]]*\(' 1 'Temporal calendar admission producer'
if grep -Eq 'TemporalCalendarCarrier|emit_temporal_calendar_slot_fast_path\(' "$wasm_temporal_plain_date"; then
  fail "$wasm_temporal_plain_date must consume completed GC calendar slots rather than a raw calendar carrier"
fi
wasm_temporal_plain_month_day="crates/scriptw-aot-wasm/src/builtins/temporal_plain_month_day.rs"
require_file "$wasm_temporal_plain_month_day"
require_exact_line_count "$wasm_temporal_plain_month_day" \
  'struct TemporalParsedMonthDayYear {' 1 'private parsed MonthDay source-year proof'
require_regex_count "$wasm_temporal_plain_month_day" \
  '^[[:space:]]*fn[[:space:]]+emit_temporal_parse_month_day_string[[:space:]]*\(' 1 'private GC MonthDay parser'
require_regex_count "$wasm_temporal_plain_month_day" \
  '^[[:space:]]*fn[[:space:]]+emit_temporal_month_day_string_reference_year[[:space:]]*\(' 1 'private completed MonthDay reference conversion'
if grep -Eq 'pub(\([^)]*\))?[[:space:]]+(struct[[:space:]]+TemporalParsedMonthDayYear|fn[[:space:]]+emit_temporal_parse_month_day_string|fn[[:space:]]+emit_temporal_month_day_string_reference_year)' "$wasm_temporal_plain_month_day"; then
  fail "$wasm_temporal_plain_month_day must keep parsed-year admission and reference conversion private"
fi
require_fixed_string_count "$wasm_temporal_plain_month_day" \
  'parsed: TemporalParsedMonthDayYear,' 1 'consuming parsed MonthDay reference conversion input'
require_fixed_string_count "$wasm_temporal_plain_month_day" \
  'self.emit_temporal_parse_month_day_string(' 1 'MonthDay string parser consumer'
require_fixed_string_count "$wasm_temporal_plain_month_day" \
  'self.emit_temporal_month_day_string_reference_year(' 1 'MonthDay source-year policy consumer'

wasm_temporal_duration="crates/scriptw-aot-wasm/src/builtins/temporal_duration.rs"
wasm_temporal_duration_fields="crates/scriptw-aot-wasm/src/builtins/temporal_duration/fields.rs"
wasm_temporal_plain_date_time="crates/scriptw-aot-wasm/src/builtins/temporal_plain_date_time.rs"
wasm_gc_layouts="crates/scriptw-aot-wasm/src/gc_types/layouts.rs"
require_file "$wasm_temporal_duration"
require_file "$wasm_temporal_duration_fields"
require_file "$wasm_gc_layouts"
require_module_decl "$wasm_temporal_duration" fields
require_exact_line_count "$wasm_temporal_duration_fields" \
  'pub(crate) struct TemporalDurationFields([I64Local; 10]);' 1 'private-representation Duration Number-bits proof'
require_exact_line_count "$wasm_temporal_duration" \
  'pub(super) enum TemporalDurationField {' 1 'closed Duration accessor selector'
require_exact_line_count "$wasm_temporal_plain_date_time" \
  'pub(crate) enum TemporalPlainDateTimeField {' 1 'closed PlainDateTime accessor selector'
# GC schema names, rather than manual byte offsets, join allocation and reads.
for temporal_gc_record in TemporalDurationObject TemporalPlainDateTimeObject TemporalInstantObject; do
  require_regex_count "$wasm_gc_layouts" \
    "^[[:space:]]*struct ${temporal_gc_record} => ${temporal_gc_record}Schema \{" 1 'central Temporal GC record schema'
done
require_fixed_string_count "$wasm_temporal_duration" \
  'schema.struct_type::<TemporalDurationObject>().construct(' 1 'GC Duration allocator'
require_fixed_string_count "$wasm_temporal_duration" \
  'self.emit_temporal_record_from_receiver::<TemporalDurationObject>(' 1 'GC Duration receiver admission'
require_fixed_string_count "$wasm_temporal_plain_date_time" \
  'self.emit_temporal_record_from_receiver::<TemporalPlainDateTimeObject>(' 2 'GC PlainDateTime receiver entries'
if grep -Eq 'TEMPORAL_(DURATION|PLAIN_DATE_TIME)_FIELD_OFFSETS' "$wasm_temporal_duration" "$wasm_temporal_plain_date_time"; then
  fail 'Temporal Duration/PlainDateTime records must not restore manual field-offset tables'
fi
wasm_temporal_instant="crates/scriptw-aot-wasm/src/builtins/temporal_instant.rs"
wasm_temporal_epoch="crates/scriptw-aot-wasm/src/builtins/temporal/epoch.rs"
require_file "$wasm_temporal_instant"
require_file "$wasm_temporal_epoch"
require_module_decl crates/scriptw-aot-wasm/src/builtins/temporal.rs epoch
require_exact_line_count "$wasm_temporal_epoch" \
  'pub(in crate::builtins) struct TemporalEpochNanoseconds {' 1 'completed Instant epoch proof owner'
temporal_epoch_fields="$(sed -n '/^pub(in crate::builtins) struct TemporalEpochNanoseconds {$/,/^}$/p' "$wasm_temporal_epoch")"
if grep -Eq '^[[:space:]]+pub(\([^)]*\))?[[:space:]]+' <<<"$temporal_epoch_fields"; then
  fail "$wasm_temporal_epoch must keep the validated epoch representation private"
fi
require_regex_count "$wasm_temporal_epoch" \
  '^[[:space:]]*pub\(in crate::builtins\)[[:space:]]+fn[[:space:]]+emit_alloc_temporal_instant[[:space:]]*\(' 1 'completed epoch Instant allocator'
require_fixed_string_count "$wasm_temporal_epoch" \
  'epoch: &TemporalEpochNanoseconds,' 1 'Instant allocator completed-proof input'
for instant_diagnostic in \
  TEMPORAL_INSTANT_FROMEPOCHMILLISECONDS_REQUIRES_AN_INTEGRAL_NUMBER \
  TEMPORAL_INSTANT_DOES_NOT_SUPPORT_IMPLICIT_CONVERSION_USE_COMPARE_OR_EQUALS; do
  require_fixed_string_count "$wasm_temporal_instant" "RuntimeErrorMessage::${instant_diagnostic}" 1 'typed Instant diagnostic consumer'
done
# These real Engine controls retain record values, ordered duration conversion,
# negative epochs, UTF-16 syntax and provider/Realm joins across the GC change.
require_file crates/scriptw-engine/tests/aot_gc_entries/aot_gc_temporal_entries.rs
require_file crates/scriptw-engine/tests/aot_temporal/aot_temporal_created_realm.rs
for temporal_gc_fixture in records_duration_order instant_epoch_utf16 zoned_transitions_realms; do
  require_file "crates/scriptw-engine/tests/fixtures/temporal_gc/${temporal_gc_fixture}.js"
  require_fixed_string_count crates/scriptw-engine/tests/aot_gc_entries/aot_gc_temporal_entries.rs \
    "fixtures/temporal_gc/${temporal_gc_fixture}.js" 1 'real Engine Temporal GC fixture attachment'
done

wasm_string_trim="crates/scriptw-aot-wasm/src/operations/string_trim.rs"
wasm_string_operations="crates/scriptw-aot-wasm/src/operations.rs"
require_file "$wasm_string_trim"
require_module_decl "$wasm_string_operations" string_trim
require_exact_line_count "$wasm_string_trim" 'pub(crate) enum EcmaTrimMode {' 1 'crate-local closed UTF-16 trim boundary domain'
require_exact_line_count "$wasm_string_operations" 'pub(crate) use string_trim::EcmaTrimMode;' 1 'shared crate-local trim policy reexport'
for trim_entry in start:Start end:End both:Both; do
  trim_method="${trim_entry%%:*}"
  trim_mode="${trim_entry#*:}"
  require_regex_count "$wasm_string_trim" \
    "^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_ecmascript_trim_${trim_method}_payload_from_locals[[:space:]]*\(" 1 'fixed GC trim entry'
  require_fixed_string_count "$wasm_string_trim" \
    "self.emit_ecmascript_trim_payload_from_locals(string, EcmaTrimMode::${trim_mode}," 1 'closed trim mode selection'
done
require_regex_count "$wasm_string_trim" \
  '^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_gc_string_trim_range[[:space:]]*\(' 1 'shared GC UTF-16 trim range owner'
require_regex_count "$wasm_string_trim" \
  '^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_gc_string_trim_range_mode[[:space:]]*\(' 1 'shared mode-specific GC trim range owner'
require_regex_count "$wasm_string_operations" \
  '^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_ecmascript_whitespace_i32[[:space:]]*\(' 1 'shared ECMAScript whitespace policy owner'
require_fixed_string_count "$wasm_string_trim" \
  'self.emit_gc_string_trim_range_mode(string, start, end, mode, function);' 1 'typed GC trim range consumer'
require_fixed_string_count "$wasm_string_trim" \
  'self.emit_gc_string_trim_range_mode(string, start, end, EcmaTrimMode::Both, function);' 1 'numeric callers preserve both-edge policy'
if grep -Eq 'ECMASCRIPT_NON_ASCII_WHITESPACE_UTF8' "$wasm_string_trim" "$wasm_builtins_mod"; then
  fail 'String trimming must use the shared UTF-16 whitespace owner rather than a byte-pattern table'
fi
require_file crates/scriptw-cli/tests/fixtures/wasm_string_annexb_substr_trim_core.js
require_regex_count crates/scriptw-cli/tests/cli/string.rs \
  '^fn run_wasm_backend_succeeds_for_string_annexb_substr_trim_fixture[[:space:]]*\(' 1 'real CLI trim semantics witness'
for obsolete_builtin_emitter in \
  emit_date_time_within_day \
  emit_throw_if_shared_array_buffer \
  emit_string_match_all_global_ascii_word_iterator_from_string_locals
do
  require_tree_regex_count \
    crates/scriptw-aot-wasm/src \
    "\\b${obsolete_builtin_emitter}\\b" \
    0 \
    "obsolete builtin emitter $obsolete_builtin_emitter"
done
for obsolete_core_backend_api in \
  static_number_expr_value \
  buffer_memarg32 \
  buffer_memarg16 \
  emit_store_realm_type_error_prototype \
  standard_builtin_prototype_global_index
do
  require_tree_regex_count \
    crates/scriptw-aot-wasm/src \
    "\\b${obsolete_core_backend_api}\\b" \
    0 \
    "obsolete core backend API $obsolete_core_backend_api"
done
wasm_planning="crates/scriptw-aot-wasm/src/planning.rs"
for obsolete_planning_api in \
  is_large_deferred_standard_builtin \
  script_uses_env \
  script_uses_calls \
  script_uses_function_heap \
  script_uses_function_table \
  block_uses_function_table \
  block_uses_calls \
  statement_uses_calls \
  for_init_uses_calls \
  statement_uses_function_table \
  for_init_uses_function_table \
  expr_uses_function_table \
  expr_uses_calls
do
  require_fixed_string_count \
    "$wasm_planning" \
    "$obsolete_planning_api" \
    0 \
    "obsolete planning API $obsolete_planning_api"
done
require_fixed_string_count \
  "$wasm_planning" \
  'super_constructor_target' \
  0 \
  'obsolete Wasm metadata super-constructor projection'
require_exact_line_count \
  "$wasm_planning" \
  '    pub(crate) fn iter(&self) -> impl Iterator<Item = (&FunctionId, &WasmFunctionMeta)> {' \
  0 \
  'obsolete function-meta registry iterator'

wasm_objects_property_read="$(braced_rust_item_source crates/scriptw-aot-wasm/src/objects.rs '^pub[(]crate[)][[:space:]]+fn[[:space:]]+compile_property_read_from_locals[[:space:]]*[(]')"
require_text_regex_count "$wasm_objects_property_read" 'receiver: &ValueLocals,' 1 'raw whole receiver property-read input'
require_text_regex_count "$wasm_objects_property_read" 'output: &ValueLocals,' 1 'whole property-read result'
require_text_regex_count "$wasm_objects_property_read" 'crate::runtime_helpers::DynamicPropertyReadArguments::new[(]' 1 'sole raw-key property-read helper consumer'
wasm_raw_key_property_read="$(braced_rust_item_source crates/scriptw-aot-wasm/src/emit/runtime_operations.rs '^pub[(]super[)][[:space:]]+fn[[:space:]]+compile_dynamic_property_read_helper[[:space:]]*[(]')"
require_text_regex_count "$wasm_raw_key_property_read" 'self\.emit_value_to_property_key_completion[(]' 1 'raw-key conversion completion boundary'
require_text_regex_count "$wasm_raw_key_property_read" 'self\.emit_dynamic_property_read_with_key_locals[(]' 1 'sole converted dynamic property-read consumer'
wasm_dynamic_property_read="$(braced_rust_item_source crates/scriptw-aot-wasm/src/objects.rs '^pub[(]crate[)][[:space:]]+fn[[:space:]]+emit_dynamic_property_read_with_key_locals[[:space:]]*[(]')"
require_text_regex_count "$wasm_dynamic_property_read" 'crate::runtime_helpers::ObjectReadArguments::new[(]' 1 'registered whole property-read helper boundary'
require_text_regex_count "$wasm_dynamic_property_read" 'receiver: &ValueLocals,' 1 'retained whole Get receiver'
require_text_regex_count "$wasm_dynamic_property_read" 'result: &CompletionLocals,' 1 'complete Get completion publication'
require_exact_line_count \
  "$wasm_lib" \
  'pub(crate) use functions::RealmRecordLocal;' \
  0 \
  'obsolete RealmRecordLocal crate-root re-export'
require_exact_line_count \
  crates/scriptw-aot-wasm/src/operations.rs \
  'use scriptw_ir::StaticRegExpCompilation;' \
  1 \
  'owner-local StaticRegExpCompilation import'

ir_lowering="crates/scriptw-ir/src/lowering.rs"
for obsolete_lowering_specialization in \
  target_has_private_brand \
  lower_generated_iterator_function_expression \
  lower_generated_iterator_function \
  lower_this_range_generator_function_body \
  single_lexical_number_binding \
  single_lexical_expression_binding \
  expression_is_this_unsigned_right_shift_zero \
  while_body_yields_and_increments \
  alloc_generated_iterator_values_name \
  lower_generator_body_as_array_iterator \
  lower_yield_star_generator_iife \
  delegate_method_returns_non_object \
  static_generator_declaration_elements \
  static_generator_statement_list_elements \
  static_generator_yield_string_element \
  static_generator_for_loop_string_elements \
  static_generator_string_for_loop_initializer \
  static_generator_string_for_loop_body \
  static_string_from_char_code_yield_name \
  static_generator_yield_identifier_name \
  static_string_from_char_code_arg_is_named \
  static_string_from_char_code_arg_name \
  static_negated_string_match_regex \
  static_string_from_char_code_value \
  static_generator_declaration_elements_by_name \
  merge_operand_shapes
do
  require_fixed_string_count \
    "$ir_lowering" \
    "$obsolete_lowering_specialization" \
    0 \
    "obsolete lowering specialization $obsolete_lowering_specialization"
done
require_fixed_string_count \
  crates/scriptw-ir/src/lowering_helpers.rs \
  'StaticStringGeneratorLoopBody' \
  0 \
  'obsolete String-generator loop domain'
generated_function_output="$(sed -n '/pub(crate) struct GeneratedFunctionOutput {/,/^}/p' "$ir_lowering")"
require_text_regex_count \
  "$generated_function_output" \
  '^[[:space:]]*pub\(crate\) [a-z_][a-z_]*:' \
  2 \
  'observed generated-function output fields'
require_text_regex_count \
  "$generated_function_output" \
  '^[[:space:]]*pub\(crate\) (function_id|this_info):' \
  0 \
  'unread generated-function output fields'
require_exact_line_count \
  crates/scriptw-ir/src/lib.rs \
  'use regress::Regex;' \
  0 \
  'obsolete broad Regex import'
require_fixed_string_count \
  crates/scriptw-ir/src/regexp.rs \
  'use regress::{' \
  1 \
  'direct regexp compiler import'

for obsolete_static_generator_cache_surface in \
  static_generator_sum_values \
  static_generator_element_values \
  prepare_static_generator_declarations \
  is_static_generator_declaration \
  static_generator_call_values \
  static_generator_call_elements_owned \
  static_generator_call_name \
  static_generator_call_is_known \
  static_generator_declaration_values_by_name \
  static_generator_call_overrides \
  static_object_iterator_literal_values \
  array_iterator_from_static_generator_values \
  array_iterator_from_lowered_elements
do
  for static_generator_source in \
    "$ir_lowering" \
    crates/scriptw-ir/src/lowering/assignment.rs \
    crates/scriptw-ir/src/lowering/call_expression.rs \
    crates/scriptw-ir/src/lowering/for_of.rs
  do
    require_fixed_string_count \
      "$static_generator_source" \
      "$obsolete_static_generator_cache_surface" \
      0 \
      "obsolete static-generator cache surface $obsolete_static_generator_cache_surface"
  done
done
# Generator calls and assignments retain their actual values through the
# non-property Call and assignment owners checked above, never cached yields.
require_fixed_string_count \
  crates/scriptw-ir/src/lowering/for_of.rs \
  'let element_info = if plain_async_await_body && iterable_is_array {' \
  0 \
  'obsolete resumable Array-walk element analysis boundary'
require_fixed_string_count \
  crates/scriptw-ir/src/lowering/for_of.rs \
  'let element_info = ValueInfo {' \
  1 \
  'generic synchronous iterator result analysis boundary'
require_fixed_string_count \
  crates/scriptw-ir/src/lowering/for_of.rs \
  'kind: ValueKind::Dynamic,' \
  1 \
  'generic synchronous iterator dynamic result kind'

test262_differential="crates/scriptw-test262/src/differential.rs"
test262_worker_controls="crates/scriptw-test262/tests/differential_worker_execution.rs"
require_file crates/scriptw-test262/src/differential/worker.rs
require_file crates/scriptw-test262/src/differential/worker_process.rs
require_file crates/scriptw-test262/tests/differential_oracle_compile_boundary_structure.rs
require_file "$test262_worker_controls"
require_exact_line_count "$test262_differential" 'mod worker;' 1 'feature-only differential worker attachment'
worker_module_attributes="$(rust_item_attributes_source "$test262_differential" '^mod[[:space:]]+worker;')"
require_text_regex_count "$worker_module_attributes" '^#\[cfg\(feature = "spec-exec-oracle"\)\]$' 1 'worker module explicit oracle compile gate'
# The existing compile-boundary test checks every gated declaration and the
# feature-off refusal. The executable controls are bound to the required worker.
require_exact_line_count "$test262_worker_controls" '#![cfg(feature = "spec-exec-oracle")]' 1 'feature-only paired worker controls'
require_fixed_string_count "$test262_worker_controls" 'env!("CARGO_BIN_EXE_scriptw-differential-worker")' 3 'actual Cargo worker selection, explicit native jobs and image identity controls'
worker_jobs_control="$(braced_rust_item_source "$test262_worker_controls" '^fn selected_worker_accepts_explicit_native_compilation_jobs[(]')"
for worker_jobs_requirement in 'new_with_compilation_jobs(selected, 0).is_err()' 'new_with_compilation_jobs(selected, 2).unwrap()' 'replay_case(&case_v1(), SpecExecOracle::explicitly_enabled(), &runner)' 'assert!(report.is_green()'; do
  require_fixed_string_present <(printf '%s\n' "$worker_jobs_control") "$worker_jobs_requirement" 'explicit native-job worker control'
done
require_fixed_string_count "$test262_worker_controls" 'fn module_loader_context_sources(' 1 'feature-only module-loader fixture owner'
require_exact_line_count crates/scriptw-test262/src/lib.rs 'fn skip_template_source(bytes: &[u8], mut idx: usize) -> usize {' 0 'obsolete template-source scanner'
require_exact_line_count crates/scriptw-test262/src/lib.rs '    fn values_mut(&mut self) -> Option<&mut Vec<T>> {' 1 'test-only wire-list mutation entry'

wasm_temporal_zoned_date_time_methods="crates/scriptw-aot-wasm/src/builtins/temporal_zoned_date_time_methods.rs"
wasm_temporal_zoned_arithmetic="crates/scriptw-aot-wasm/src/builtins/temporal_zoned_arithmetic.rs"
require_file "$wasm_temporal_zoned_arithmetic"
for direction_domain in TemporalZonedArithmeticOperation TemporalZonedDifferenceOperation; do
  require_fixed_string_count \
    "$wasm_temporal_zoned_arithmetic" \
    "pub(super) enum $direction_domain" \
    1 \
    "shared ZonedDateTime direction authority $direction_domain"
  if grep -Eq "enum[[:space:]]+$direction_domain" "$wasm_temporal_zoned_date_time_methods"; then
    fail "$direction_domain must be owned by the shared ZonedDateTime arithmetic authority"
  fi
  if grep -Eq "$direction_domain" "$wasm_standard_builtins" "$wasm_builtins_mod"; then
    fail "$direction_domain must not escape its ZonedDateTime authority and method entries"
  fi
done
for zoned_method in add subtract until since; do
  require_fixed_string_count \
    "$wasm_temporal_zoned_date_time_methods" \
    "pub(super) fn emit_temporal_zoned_date_time_${zoned_method}_builtin(" \
    1 \
    "fixed ZonedDateTime $zoned_method entry"
  require_fixed_string_count \
    "$wasm_standard_builtins" \
    "self.emit_temporal_zoned_date_time_${zoned_method}_builtin(function)?;" \
    1 \
    "fixed ZonedDateTime $zoned_method route"
done
require_fixed_string_count \
  "$wasm_temporal_zoned_date_time_methods" \
  'fn emit_temporal_zoned_date_time_add_or_subtract(' \
  1 \
  'private ZonedDateTime arithmetic emitter'
require_fixed_string_count \
  "$wasm_temporal_zoned_date_time_methods" \
  'fn emit_temporal_zoned_date_time_until_or_since(' \
  1 \
  'private ZonedDateTime difference emitter'
if grep -Eq 'emit_temporal_zoned_date_time_(add_or_subtract|until_or_since)\(' "$wasm_standard_builtins"; then
  fail "$wasm_standard_builtins must use fixed ZonedDateTime arithmetic and difference entries"
fi
check_raw_line_budget "$wasm_temporal_zoned_date_time_methods" 700

# T02's realm-bootstrap boundary. These files hold the per-family property and
# descriptor installation extracted out of the single
# init_builtin_constructor_object function, which every builtin lane previously
# had to edit. Requiring them keeps that split from silently collapsing back.
require_file "$wasm_intrinsics_mod"
require_module_decl "$wasm_lib" "intrinsics"
for module in array binary_data collections date errors function iterator numeric object promise proxy regexp string symbol temporal; do
  require_file "crates/scriptw-aot-wasm/src/intrinsics/${module}.rs"
  require_module_decl "$wasm_intrinsics_mod" "$module"
done
wasm_module_source_intrinsic='crates/scriptw-aot-wasm/src/intrinsics/abstract_module_source.rs'
wasm_module_source_host='crates/scriptw-aot-wasm/src/builtins/host/abstract_module_source.rs'
require_file "$wasm_module_source_intrinsic"
require_file "$wasm_module_source_host"
require_exact_line_count "$wasm_intrinsics_mod" 'mod abstract_module_source;' 1 'private native module-source intrinsic owner'
require_exact_line_count 'crates/scriptw-aot-wasm/src/builtins/host.rs' 'mod abstract_module_source;' 1 'private Test262 intrinsic retrieval owner'
require_fixed_string_count "$wasm_module_source_intrinsic" 'pub(crate) fn emit_initialize_abstract_module_source_intrinsic(' 1 'actual GC native intrinsic bootstrap owner'
require_fixed_string_count 'crates/scriptw-aot-wasm/src/builtins/bootstrap.rs' 'self.emit_initialize_abstract_module_source_intrinsic(' 1 'consumed Realm bootstrap entry'
require_fixed_string_count "$wasm_module_source_host" 'pub(crate) fn compile_host_get_abstract_module_source_builtin(' 1 'actual defining-Realm intrinsic retrieval owner'
require_fixed_string_count 'crates/scriptw-aot-wasm/src/emit/body_compilation.rs' 'self.compile_host_get_abstract_module_source_builtin(' 1 'consumed native host body entry'
check_no_inline_legacy_includes "$wasm_module_source_intrinsic"
check_no_inline_legacy_includes "$wasm_module_source_host"
check_raw_line_budget "$wasm_module_source_intrinsic" 75
check_raw_line_budget "$wasm_module_source_host" 45
check_no_inline_legacy_includes "$wasm_intrinsics_mod"

require_pub_use "$wasm_lib" '^pub use emit::emit;' 'the Wasm emit entry point'
# 180 against a CODE-ONLY count, measured 101 at batch 6 (118 raw). Unlike the
# `scriptw-ir` budget above this one was never near its limit, so the switch to a
# code-only count did not need a matching adjustment.
# Two actual declaration/body owners, function_entry and emitted_function,
# add their consumed module declarations to the crate orchestration surface.
check_orchestration_surface "$wasm_lib" 182
check_no_inline_legacy_includes "$wasm_lib"
check_no_inline_legacy_includes "$wasm_builtins_mod"
