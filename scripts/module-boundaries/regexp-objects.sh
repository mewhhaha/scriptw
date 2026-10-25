# T02's RegExp range-search owner. The parent matcher may request the semantic
# mismatch operation, but only the private child may select encoded range
# bounds or read their raw offsets.
wasm_regexp_builtins="crates/scriptw-aot-wasm/src/builtins/regexp.rs"
wasm_regexp_range_search="crates/scriptw-aot-wasm/src/builtins/regexp/range_search.rs"
wasm_regexp_counted="crates/scriptw-aot-wasm/src/builtins/regexp/counted.rs"
wasm_regexp_workspace="crates/scriptw-aot-wasm/src/builtins/regexp/matcher_workspace.rs"
for regexp_state_owner in "$wasm_regexp_counted" "$wasm_regexp_workspace"; do
  require_file "$regexp_state_owner"
  check_no_inline_legacy_includes "$regexp_state_owner"
done
require_exact_line_count "$wasm_regexp_builtins" 'mod counted;' 1 'private counted-repeat owner attachment'
require_exact_line_count "$wasm_regexp_builtins" 'mod matcher_workspace;' 1 'private matcher workspace attachment'
require_fixed_string_count "$wasm_regexp_counted" 'pub(super) fn emit_regexp_counted_dispatch(' 1 'consumed counted-repeat dispatcher'
require_fixed_string_count "$wasm_regexp_builtins" 'fn emit_regexp_counted_dispatch(' 0 'no parent counted-repeat dispatcher copy'
require_fixed_string_count "$wasm_regexp_builtins" 'self.emit_regexp_counted_dispatch(' 1 'actual native matcher counted-repeat dispatch'
require_fixed_string_count "$wasm_regexp_builtins" 'MatcherWorkspace::allocate(' 1 'actual matcher workspace allocation'
require_fixed_string_count "$wasm_regexp_builtins" 'workspace.choices().push_snapshot(' 2 'ordinary and progress choices consume the same linked full-state owner'
require_fixed_string_count "$wasm_regexp_counted" 'workspace.choices().push_snapshot(' 1 'counted fallback consumes the same linked full-state owner'
require_fixed_string_count "$wasm_regexp_builtins" 'workspace.choices().find_assertion(' 1 'assertion rollback searches logical entries'
require_fixed_string_count "$wasm_regexp_builtins" 'workspace.choices().find_progress(' 1 'progress searches logical entries'
for obsolete_choice_access in 'workspace.ensure_frame(' 'workspace.save_state(' 'choice_depth' 'frame_width'; do
  require_fixed_string_count "$wasm_regexp_builtins" "$obsolete_choice_access" 0 'no fixed-depth choice indexing in the matcher'
  require_fixed_string_count "$wasm_regexp_counted" "$obsolete_choice_access" 0 'no fixed-depth choice indexing in counted dispatch'
done
wasm_regexp_compiler="crates/scriptw-aot-wasm/src/builtins/regexp/compiler.rs"
wasm_regexp_program="crates/scriptw-aot-wasm/src/builtins/regexp/program.rs"
for regexp_descriptor_owner in \
  'compiler:repeat_bounds:emit_regexp_compile_repeat_bounds:300' \
  'program:repeat_bounds:emit_validate_regexp_repeat_bounds:370' \
  'program:counted_regions:emit_validate_regexp_counted_regions:575'
do
  regexp_descriptor_parent="${regexp_descriptor_owner%%:*}"
  regexp_descriptor_tail="${regexp_descriptor_owner#*:}"
  regexp_descriptor_child="${regexp_descriptor_tail%%:*}"
  regexp_descriptor_tail="${regexp_descriptor_tail#*:}"
  regexp_descriptor_entry="${regexp_descriptor_tail%%:*}"
  regexp_descriptor_budget="${regexp_descriptor_tail#*:}"
  regexp_descriptor_parent_path="crates/scriptw-aot-wasm/src/builtins/regexp/${regexp_descriptor_parent}.rs"
  regexp_descriptor_child_path="crates/scriptw-aot-wasm/src/builtins/regexp/${regexp_descriptor_parent}/${regexp_descriptor_child}.rs"
  require_file "$regexp_descriptor_child_path"
  require_exact_line_count "$regexp_descriptor_parent_path" "mod ${regexp_descriptor_child};" 1 'private exact RegExp descriptor owner attachment'
  if grep -Eq "^pub(\([^)]*\))?[[:space:]]+mod[[:space:]]+${regexp_descriptor_child};" "$regexp_descriptor_parent_path"; then
    fail "$regexp_descriptor_parent_path must keep ${regexp_descriptor_child} private"
  fi
  require_fixed_string_count "$regexp_descriptor_child_path" "pub(super) fn ${regexp_descriptor_entry}(" 1 'consumed exact RegExp descriptor entry'
  require_fixed_string_count "$regexp_descriptor_parent_path" "self.${regexp_descriptor_entry}(" 1 'actual descriptor publication/admission consumer'
  require_fixed_string_count "$regexp_descriptor_parent_path" "fn ${regexp_descriptor_entry}(" 0 'no parent exact descriptor implementation copy'
  check_no_inline_legacy_includes "$regexp_descriptor_child_path"
  check_raw_line_budget "$regexp_descriptor_child_path" "$regexp_descriptor_budget"
done
check_raw_line_budget "$wasm_regexp_compiler" 1900
check_raw_line_budget "$wasm_regexp_program" 720
wasm_regexp_natural_counter="crates/scriptw-aot-wasm/src/builtins/regexp/counted/natural_counter.rs"
wasm_regexp_required_empty="crates/scriptw-aot-wasm/src/builtins/regexp/counted/required_empty.rs"
wasm_regexp_required_run="crates/scriptw-aot-wasm/src/builtins/regexp/counted/required_choice_run.rs"
wasm_regexp_independent_iterations="crates/scriptw-aot-wasm/src/builtins/regexp/counted/independent_iterations.rs"
wasm_regexp_choices="crates/scriptw-aot-wasm/src/builtins/regexp/matcher_workspace/choice_entries.rs"
wasm_regexp_run_playback="crates/scriptw-aot-wasm/src/builtins/regexp/matcher_workspace/choice_entries/required_run.rs"
for regexp_counter_child in natural_counter completed_exact_child input_only_assertion required_empty independent_iterations; do
  regexp_counter_path="crates/scriptw-aot-wasm/src/builtins/regexp/counted/${regexp_counter_child}.rs"
  require_file "$regexp_counter_path"
  require_exact_line_count "$wasm_regexp_counted" "mod ${regexp_counter_child};" 1 'private native counted-repeat child attachment'
  if grep -Eq "^pub(\([^)]*\))?[[:space:]]+mod[[:space:]]+${regexp_counter_child};" "$wasm_regexp_counted"; then
    fail "$wasm_regexp_counted must keep ${regexp_counter_child} private"
  fi
  check_no_inline_legacy_includes "$regexp_counter_path"
done
require_exact_line_count "$wasm_regexp_counted" 'pub(super) mod required_choice_run;' 1 'Run proof is visible only to its same-matcher physical consumer'
require_file "$wasm_regexp_required_run"
check_no_inline_legacy_includes "$wasm_regexp_required_run"
require_regex_count "$wasm_regexp_natural_counter" '^pub\(super\) struct MinimumCounter;' 1 'private minimum counter role'
require_regex_count "$wasm_regexp_natural_counter" '^pub\(super\) struct MaximumCounter;' 1 'private maximum counter role'
require_fixed_string_count "$wasm_regexp_counted" 'NaturalCounterLocals::reserve(' 2 'actual minimum/maximum counter allocation'
# Private scoped proofs retain the actual admitted pair and workspace. Begin
# translates only independent iterations; End preserves ordered empty replay
# or publishes the exact proven template, retaining the finite optional gap.
for regexp_proof in \
  'required_empty:ProvenRequiredEmptyReplay:RequiredEmptyReplayLocals' \
  'required_choice_run:ProvenRequiredChoiceRun:RequiredChoiceRunLocals' \
  'independent_iterations:ProvenIndependentIterations:IndependentIterationLocals'
do
  regexp_proof_child="${regexp_proof%%:*}"
  regexp_proof_tail="${regexp_proof#*:}"
  regexp_proof_type="${regexp_proof_tail%%:*}"
  regexp_proof_locals="${regexp_proof_tail#*:}"
  regexp_proof_path="crates/scriptw-aot-wasm/src/builtins/regexp/counted/${regexp_proof_child}.rs"
  require_fixed_string_count "$regexp_proof_path" 'struct ProofScope;' 1 'private scoped acceleration authority'
  regexp_proof_fields="$(sed -n "/struct ${regexp_proof_type}</,/^}/p" "$regexp_proof_path")"
  require_text_regex_count "$regexp_proof_fields" '^[[:space:]]+pub(\([^)]*\))?[[:space:]]+' 0 'no caller-manufactured acceleration proof'
  require_fixed_string_present "$regexp_proof_path" "fn emit_if_proven<'body>(" 'actual pair/workspace proof constructor'
  require_fixed_string_count "$wasm_regexp_counted" "${regexp_proof_locals}::emit_if_proven(" 1 'actual counted dispatcher consumes its proof'
done
require_fixed_string_count "$wasm_regexp_counted" 'proof.finish_required_batch(' 2 'checked End consumes empty replay or ordered Run publication'
require_fixed_string_count "$wasm_regexp_counted" 'proof.translate_required_count(builder, f)' 1 'checked Begin consumes independent-iteration translation'
require_fixed_string_count "$wasm_regexp_counted" 'workspace.choices().observe_repeat_end(self, p.state, f);' 1 'actual End observes the authorized live row before its counter semantics'
for regexp_batch_proof in "$wasm_regexp_required_empty" "$wasm_regexp_required_run"; do
  require_fixed_string_present "$regexp_batch_proof" 'self.pair.maximum.subtract_minimum(' 'finite maximum preserves the exact optional gap'
  require_fixed_string_present "$regexp_batch_proof" 'self.pair.minimum.clear(' 'completed mandatory batch retires its minimum'
done
require_fixed_string_count "$wasm_regexp_required_run" '.append_required_run(builder, &self, f)?;' 1 'only a same-workspace proven template publishes a Run'
for bounded_counter_operation in greater_than_bounded subtract_bounded retain_bounded; do
  require_tree_regex_count crates/scriptw-aot-wasm/src/builtins/regexp/counted "^[[:space:]]*pub\\(in crate::builtins::regexp::counted\\) fn ${bounded_counter_operation}\\(" 1 'sole canonical source-sized bounded counter operation'
  require_fixed_string_present "$wasm_regexp_independent_iterations" ".${bounded_counter_operation}(" 'independent translation consumes full-limb arithmetic'
done
require_fixed_string_present "$wasm_regexp_independent_iterations" 'self.pair.maximum.subtract_minimum(' 'independent translation preserves max-minus-min'
require_exact_line_count "$wasm_regexp_workspace" 'mod choice_entries;' 1 'private linked choice arena attachment'
require_fixed_string_count "$wasm_regexp_workspace" 'pub(super) fn choices(&self)' 1 'sole same-workspace choice authority'
require_fixed_string_count "$wasm_regexp_workspace" 'fn ensure_choice_bytes(' 1 'exclusive transient-tail growth owner'
require_exact_line_count "$wasm_regexp_choices" 'mod required_run;' 1 'private physical Run playback attachment'
for regexp_run_child in natural logical exhaustion path tree; do
  regexp_run_child_path="crates/scriptw-aot-wasm/src/builtins/regexp/matcher_workspace/choice_entries/required_run/${regexp_run_child}.rs"
  require_file "$regexp_run_child_path"
  require_exact_line_count "$wasm_regexp_run_playback" "mod ${regexp_run_child};" 1 'private actual Run semantic owner'
  check_no_inline_legacy_includes "$regexp_run_child_path"
done
require_tree_regex_count crates/scriptw-aot-wasm/src/builtins/regexp/matcher_workspace '^[[:space:]]*pub\(in crate::builtins::regexp\) fn append_required_run\(' 1 'sole physical Run publication owner'
require_fixed_string_present "$wasm_regexp_run_playback" 'core::ptr::eq(self.workspace, proof.workspace())' 'proof belongs to the original arena'
require_fixed_string_present "$wasm_regexp_run_playback" '(proof.template_bytes(), layout.template_bytes)' 'Run copies a measured immutable tree rather than unfolding repetitions'
require_fixed_string_present "$wasm_regexp_run_playback" 'layout.initialize_tree(self.workspace, builder, f);' 'copied child paths use checked relative playback state'
for regexp_tree_reader in with_template_snapshots compare_template_tree; do
  require_tree_regex_count crates/scriptw-aot-wasm/src/builtins/regexp/matcher_workspace "^[[:space:]]*pub\\(in crate::builtins::regexp\\) fn ${regexp_tree_reader}\\(" 1 'sole bounded same-workspace tree reader'
  require_fixed_string_present "$wasm_regexp_required_run" ".${regexp_tree_reader}(" 'proof consumes the actual physical tree'
done
require_fixed_string_present "$wasm_regexp_builtins" 'entry.restore_required_run(' 'native failure resumes the selected virtual continuation'
require_fixed_string_present "$wasm_regexp_builtins" 'entry.restore_repeats(builder, function);' 'logical assertion rollback restores the complete counter slab'
for regexp_choice_owner in "$wasm_regexp_choices" "$wasm_regexp_run_playback"; do
  check_no_inline_legacy_includes "$regexp_choice_owner"
done
require_file "$wasm_regexp_range_search"
check_no_inline_legacy_includes "$wasm_regexp_builtins"
check_no_inline_legacy_includes "$wasm_regexp_range_search"
require_exact_line_count \
  "$wasm_regexp_builtins" \
  'mod range_search;' \
  1 \
  'private RegExp range-search module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+range_search;' "$wasm_regexp_builtins"; then
  fail "$wasm_regexp_builtins must keep range_search private"
fi
if grep -Eq 'RegExpRangeBound|emit_regexp_range_bound_load\(|range_search::' "$wasm_regexp_builtins"; then
  fail "$wasm_regexp_builtins must not name, construct, project or import the private RegExp range-bound policy"
fi
require_fixed_string_count \
  "$wasm_regexp_range_search" \
  'RegExpRangeBound' \
  5 \
  'RegExp range-bound owner uses'
require_fixed_string_count \
  "$wasm_regexp_range_search" \
  'RegExpRangeBound::' \
  2 \
  'RegExp range-bound producer selections'
require_fixed_string_count \
  "$wasm_regexp_range_search" \
  'emit_regexp_range_bound_load(' \
  3 \
  'RegExp range-bound reader and consumers'
require_regex_count \
  "$wasm_regexp_range_search" \
  '^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+emit_regexp_unicode_property_mismatch[[:space:]]*\(' \
  1 \
  'RegExp semantic range-search surface'
require_fixed_string_count \
  "$wasm_regexp_builtins" \
  'self.emit_regexp_unicode_property_mismatch(' \
  2 \
  'unchanged forward and reverse RegExp range-search calls'
# GC program/input roots and typed scalar locals bring the matcher to 3,960 lines.
# Range-search remains its separate 120-line owner with a narrow margin.
check_raw_line_budget "$wasm_regexp_builtins" 4050
check_raw_line_budget "$wasm_regexp_range_search" 145

# String protocol acquisition and RegExp flag selection have private children.
# Fixed Standard entries consume those owners; the dispatcher cannot select a
# private protocol or recreate a public-property lookup.
wasm_string_builtins="crates/scriptw-aot-wasm/src/builtins/string.rs"
wasm_string_symbol_method="crates/scriptw-aot-wasm/src/builtins/string/symbol_method.rs"
wasm_regexp_protocol="crates/scriptw-aot-wasm/src/builtins/string/regexp_protocol.rs"
wasm_regexp_substitution="crates/scriptw-aot-wasm/src/builtins/string/regexp_substitution.rs"
wasm_string_literal_replacement_scope="crates/scriptw-aot-wasm/src/builtins/string/string_literal_replacement_scope.rs"
for string_child in symbol_method regexp_protocol regexp_substitution string_literal_replacement_scope; do
  string_child_source="crates/scriptw-aot-wasm/src/builtins/string/${string_child}.rs"
  require_file "$string_child_source"
  require_exact_line_count "$wasm_string_builtins" "mod ${string_child};" 1 'private String algorithm child'
  check_no_inline_legacy_includes "$string_child_source"
  if grep -Eq "^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+${string_child};" "$wasm_string_builtins"; then
    fail "$wasm_string_builtins must keep ${string_child} private"
  fi
done
check_no_inline_legacy_includes "$wasm_string_builtins"
require_regex_count "$wasm_string_symbol_method" '^enum NativeStringProtocol \{' 1 'private closed String protocol domain'
require_fixed_string_count "$wasm_string_symbol_method" 'fn emit_native_string_protocol(' 1 'sole String protocol owner'
require_fixed_string_count "$wasm_string_symbol_method" 'pub(super) struct NullableStringSymbolMethod {' 1 'retained acquired String method owner'
for string_protocol_non_owner in "$wasm_string_builtins" "$wasm_standard_builtins"; do
  if grep -Eq 'NativeStringProtocol|emit_native_string_protocol\(' "$string_protocol_non_owner"; then
    fail "$string_protocol_non_owner must consume fixed String protocol entries"
  fi
done
for string_symbol_hook_entry in match match_all replace replace_all search split; do
  require_regex_count "$wasm_string_symbol_method" \
    "^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_string_${string_symbol_hook_entry}_builtin[[:space:]]*\(" \
    1 "fixed String protocol entry ${string_symbol_hook_entry}"
  require_fixed_string_count "$wasm_standard_builtins" \
    "self.emit_string_${string_symbol_hook_entry}_builtin(function)?;" 1 \
    "fixed String protocol route ${string_symbol_hook_entry}"
done
require_fixed_string_count "$wasm_regexp_protocol" 'fn emit_native_regexp_flag_getter(' 1 'sole RegExp flag-getter owner'
if grep -Eq 'NativeRegExpFlag|emit_native_regexp_flag_getter\(' "$wasm_standard_builtins"; then
  fail "$wasm_standard_builtins must consume fixed RegExp flag-getter entries"
fi
for regexp_flag_getter_entry in has_indices global ignore_case multiline dot_all unicode unicode_sets sticky; do
  require_regex_count "$wasm_regexp_protocol" \
    "^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_regexp_prototype_${regexp_flag_getter_entry}_getter_builtin[[:space:]]*\(" \
    1 "fixed RegExp flag-getter entry ${regexp_flag_getter_entry}"
  require_fixed_string_count "$wasm_standard_builtins" \
    "self.emit_regexp_prototype_${regexp_flag_getter_entry}_getter_builtin(function)?;" 1 \
    "fixed RegExp flag-getter route ${regexp_flag_getter_entry}"
done

# Completed capture lists and named capture conversion belong to GetSubstitution.
# Both RegExp replacement and literal String replacement consume this one owner.
for capture_owner in SubstitutionCaptureList SubstitutionCaptures NamedSubstitutionCaptures; do
  require_regex_count "$wasm_regexp_substitution" \
    "^pub\(super\) struct ${capture_owner}[({]" 1 'opaque completed substitution input owner'
done
require_regex_count "$wasm_regexp_substitution" \
  '^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+emit_regexp_get_substitution[[:space:]]*\(' \
  1 'sole semantic GetSubstitution surface'
for substitution_consumer in "$wasm_regexp_protocol" "$wasm_string_literal_replacement_scope"; do
  require_fixed_string_count "$substitution_consumer" 'emit_regexp_get_substitution(' 1 'actual GetSubstitution consumer'
done
require_fixed_string_count "$wasm_string_builtins" 'fn emit_regexp_get_substitution(' 0 'no parent GetSubstitution copy'
check_raw_line_budget "$wasm_regexp_substitution" 520

# The private literal-replacement scope owns first/all selection. The observed
# String protocol child consumes only its two semantic wrappers.
require_regex_count "$wasm_string_literal_replacement_scope" '^enum StringLiteralReplacementScope \{' 1 'private replacement scope domain'
for replacement_non_owner in "$wasm_string_builtins" "$wasm_string_symbol_method" "$wasm_standard_builtins"; do
  if grep -Eq 'StringLiteralReplacementScope|emit_string_replace_literal_from_string_locals\(' "$replacement_non_owner"; then
    fail "$replacement_non_owner must consume fixed literal-replacement wrappers"
  fi
done
for semantic_replacement in emit_string_replace_literal_first_occurrence_from_string_locals emit_string_replace_literal_all_occurrences_from_string_locals; do
  require_regex_count "$wasm_string_literal_replacement_scope" \
    "^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+${semantic_replacement}[[:space:]]*\(" \
    1 'literal-replacement semantic wrapper'
  require_fixed_string_count "$wasm_string_symbol_method" "${semantic_replacement}(" 1 'observed String protocol replacement consumer'
done
check_raw_line_budget "$wasm_string_builtins" 19980
# Whole GC strings/captures and completion cleanup add typed lifecycle code;
# the private literal-replacement owner is 564 lines with a narrow margin.
check_raw_line_budget "$wasm_string_literal_replacement_scope" 600

# T20's Number-to-32-bit residue arithmetic remains in one typed numeric owner.
# GC cutover moved its consumers into private host, String and indexed-property
# children; a frozen call-count/file inventory is not the operation contract.
wasm_uint32_authority="crates/scriptw-aot-wasm/src/operations.rs"
uint32_modulus='Instruction::F64Const(Ieee64::from(4_294_967_296.0))'
uint32_modulus_files="$(grep -RFl --include='*.rs' "$uint32_modulus" crates/scriptw-aot-wasm/src || true)"
if [ "$uint32_modulus_files" != "$wasm_uint32_authority" ]; then
  fail "the exact modulo-2^32 implementation must exist only in $wasm_uint32_authority (found: ${uint32_modulus_files:-none})"
fi
require_regex_count "$wasm_uint32_authority" '^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_to_uint32_i64_from_number_payload[[:space:]]*\(' 1 'sole typed ToUint32 kernel'
require_file crates/scriptw-engine/tests/aot_gc_entries/aot_gc_numeric_values.rs
require_file crates/scriptw-engine/tests/aot_gc_entries/aot_gc_binary_data_entries.rs

# T20's variadic Math extremum walk. The call ABI already owns an arbitrary
# argc/argv domain, so min/max must consume the runtime vector rather than grow
# another reviewed-looking finite prefix. The private enum owns the paired
# identity/reduction decisions; the loop owns every argument conversion.
math_extremum_file="crates/scriptw-aot-wasm/src/builtins/math.rs"
require_fixed_string_count "$math_extremum_file" 'enum MathExtremum {' 1 'closed Math extremum domain'
require_regex_count "$math_extremum_file" '^[[:space:]]*const fn identity\(&self\) -> f64 \{' 1 'private Math extremum identity owner'
require_regex_count "$math_extremum_file" '^[[:space:]]*fn emit_combine\(' 1 'private Math extremum reduction owner'
require_fixed_string_count "$math_extremum_file" 'emit_math_extremum_builtin(' 3 'Math extremum body and two fixed consumers'
require_fixed_string_count "$math_extremum_file" 'fn emit_math_coerce_number(' 1 'sole whole-completion Math argument coercion owner'
# Argument traversal consumes the actual nonnull ValueArray from BodyEntryLocals.
# Exact scalar opcode/count recipes are obsolete; the retained CLI arity fixture
# below checks the complete variadic behavior and abrupt conversion ordering.
if grep -Eq 'argv_param_local\(|argc_param_local\(|emit_builtin_arg_to_locals\(' "$math_extremum_file"; then
  fail 'Math must consume complete rooted argument vectors instead of scalar ABI accessors'
fi
for math_extremum_entry in min max; do
  require_fixed_string_count "$math_extremum_file" "fn emit_math_${math_extremum_entry}_builtin(" 1 'fixed Math extremum entry'
  require_fixed_string_count "$wasm_standard_builtins" "self.emit_math_${math_extremum_entry}_builtin(function)?" 1 'fixed Standard Math extremum route'
done
require_fixed_string_count \
  crates/scriptw-cli/tests/cli/language_numerics.rs \
  'fn run_wasm_backend_succeeds_for_math_extremum_argument_reduction()' \
  1 \
  'Math extremum variadic CLI regression'
if [ ! -f crates/scriptw-cli/tests/fixtures/wasm_math_min_max_arity.js ]; then
  fail 'Math extremum variadic fixture must remain present'
fi

# Passive linear-record layout inventories are retired with the atomic GC model.
# Actual record schemas and their consumed field codecs live in gc_types/.

# Object.getOwnPropertyNames uses two fixed all-string Array operations.
# Enumerable selection belongs to the shared Object keys/entries/values path,
# which observes descriptors lazily after collecting own keys.
array_named_key_owner="crates/scriptw-aot-wasm/src/builtins/array.rs"
array_named_key_caller="crates/scriptw-aot-wasm/src/builtins/object.rs"
for retired_array_key_policy in \
  ArrayNamedStringKeySelection \
  emit_array_named_string_props_count \
  emit_array_named_string_props_write_keys \
  emit_array_enumerable_named_string_props_count \
  emit_array_enumerable_named_string_props_write_keys
do
  require_fixed_string_count "$array_named_key_owner" "$retired_array_key_policy" 0 'retired eager Array key selection'
  require_fixed_string_count "$array_named_key_caller" "$retired_array_key_policy" 0 'retired eager Object key selection'
done
require_fixed_string_count "$array_named_key_caller" 'fn emit_native_object_own_keys_builtin(' 1 'actual native own-key algorithm owner'
require_fixed_string_count "$array_named_key_caller" 'self.emit_native_object_own_keys_builtin(NativeOwnKeyKind::String' 1 'fixed own-name entry'
require_fixed_string_count "$array_named_key_caller" 'self.emit_native_object_own_keys_builtin(NativeOwnKeyKind::Symbol' 1 'fixed own-symbol entry'

# T16's raw sort output policy is private to fixed sort and toSorted entries.
array_sort_owner="crates/scriptw-aot-wasm/src/builtins/array.rs"
array_sort_caller="crates/scriptw-aot-wasm/src/builtins/standard.rs"
require_fixed_string_count "$array_sort_owner" 'enum ArraySortOutput {' 1 'private Array sort output policy'
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+enum[[:space:]]+ArraySortOutput' "$array_sort_owner"; then
  fail "$array_sort_owner must keep ArraySortOutput private"
fi
if grep -Eq 'ArraySortOutput|compile_array_sort_with_output\(' "$array_sort_caller"; then
  fail "$array_sort_caller must use fixed Array sort entries"
fi
require_fixed_string_count "$array_sort_owner" 'fn compile_array_sort_with_output(' 1 'private shared Array sort compiler'
require_fixed_string_count "$array_sort_owner" 'self.compile_array_sort_with_output(' 2 'fixed Array sort entry calls'
for array_sort_wrapper in \
  compile_array_prototype_sort_builtin \
  compile_array_prototype_to_sorted_builtin
do
  require_fixed_string_count "$array_sort_owner" "pub(crate) fn ${array_sort_wrapper}(" 1 "fixed Array sort entry $array_sort_wrapper"
  require_fixed_string_count "$array_sort_caller" "self.${array_sort_wrapper}(function)?" 1 "standard call to fixed Array sort entry $array_sort_wrapper"
done

# T16's find-family kind and raw compilers are private to eight fixed entries.
array_find_parent="crates/scriptw-aot-wasm/src/builtins/array.rs"
array_find_owner="crates/scriptw-aot-wasm/src/builtins/array/find_via_predicate.rs"
array_find_caller="crates/scriptw-aot-wasm/src/builtins/standard.rs"
require_fixed_string_count "$array_find_owner" 'enum FindViaPredicateKind {' 1 'private find-family kind'
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+enum[[:space:]]+FindViaPredicateKind' "$array_find_owner"; then
  fail "$array_find_owner must keep FindViaPredicateKind private"
fi
if grep -Eq 'FindViaPredicateKind|compile_(array|typed_array)_find_with_kind\(' "$array_find_parent" "$array_find_caller"; then
  fail 'Array parent and standard catalog must use fixed find-family entries'
fi
require_fixed_string_count "$array_find_owner" 'fn compile_array_find_with_kind(' 1 'private Array find compiler'
require_fixed_string_count "$array_find_owner" 'self.compile_array_find_with_kind(' 8 'fixed Array and TypedArray find entry calls'
for array_find_family in array typed_array; do
  for array_find_method in find find_index find_last find_last_index; do
    array_find_wrapper="compile_${array_find_family}_prototype_${array_find_method}_builtin"
    require_fixed_string_count "$array_find_owner" "pub(in crate::builtins) fn ${array_find_wrapper}(" 1 "fixed find-family entry $array_find_wrapper"
    require_fixed_string_count "$array_find_caller" "self.${array_find_wrapper}(function)?" 1 "standard call to fixed find-family entry $array_find_wrapper"
  done
done

# T16's callback receiver policy is private to six fixed reducer/forEach
# entries. The reducer entries are audited with their direction below.
array_callback_owner="crates/scriptw-aot-wasm/src/builtins/array.rs"
array_callback_caller="crates/scriptw-aot-wasm/src/builtins/standard.rs"
require_fixed_string_count "$array_callback_owner" 'enum ArrayCallbackReceiverKind {' 1 'private Array callback receiver policy'
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+enum[[:space:]]+ArrayCallbackReceiverKind' "$array_callback_owner"; then
  fail "$array_callback_owner must keep ArrayCallbackReceiverKind private"
fi
if grep -Eq 'ArrayCallbackReceiverKind|compile_array_like_for_each_builtin\(' "$array_callback_caller"; then
  fail "$array_callback_caller must use fixed Array callback entries"
fi
require_fixed_string_count "$array_callback_owner" 'fn compile_array_like_for_each_builtin(' 1 'private shared Array forEach compiler'
require_fixed_string_count "$array_callback_owner" 'self.compile_array_like_for_each_builtin(' 2 'fixed Array forEach entry calls'
for array_for_each_wrapper in \
  compile_array_prototype_for_each_builtin \
  compile_typed_array_prototype_for_each_builtin
do
  require_fixed_string_count "$array_callback_owner" "pub(super) fn ${array_for_each_wrapper}(" 1 "fixed Array forEach entry $array_for_each_wrapper"
  require_fixed_string_count "$array_callback_caller" "self.${array_for_each_wrapper}(function)?" 1 "standard call to fixed Array forEach entry $array_for_each_wrapper"
done

# T16's raw reducer direction is private to four fixed semantic entries.
array_reduce_owner="crates/scriptw-aot-wasm/src/builtins/array.rs"
array_reduce_caller="crates/scriptw-aot-wasm/src/builtins/standard.rs"
require_fixed_string_count "$array_reduce_owner" 'enum ArrayReduceDirection {' 1 'private Array reduce direction'
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+enum[[:space:]]+ArrayReduceDirection' "$array_reduce_owner"; then
  fail "$array_reduce_owner must keep ArrayReduceDirection private"
fi
if grep -Eq 'ArrayReduceDirection|compile_array_like_reduce_builtin\(' "$array_reduce_caller"; then
  fail "$array_reduce_caller must use fixed Array reducer entries"
fi
require_fixed_string_count "$array_reduce_owner" 'fn compile_array_like_reduce_builtin(' 1 'private shared Array reducer'
require_fixed_string_count "$array_reduce_owner" 'self.compile_array_like_reduce_builtin(' 4 'fixed Array reducer entry calls'
for array_reduce_wrapper in \
  compile_array_reduce_builtin \
  compile_array_reduce_right_builtin \
  compile_typed_array_reduce_builtin \
  compile_typed_array_reduce_right_builtin
do
  require_fixed_string_count "$array_reduce_owner" "pub(super) fn ${array_reduce_wrapper}(" 1 "fixed Array reducer entry $array_reduce_wrapper"
  require_fixed_string_count "$array_reduce_caller" "self.${array_reduce_wrapper}(function)?" 1 "standard call to fixed Array reducer entry $array_reduce_wrapper"
done

# T16/T17's raw Array/TypedArray at policy is private to two fixed entries.
array_at_owner="crates/scriptw-aot-wasm/src/builtins/array.rs"
array_at_caller="crates/scriptw-aot-wasm/src/builtins/standard.rs"
require_fixed_string_count "$array_at_owner" 'enum ArrayAtReceiverPolicy {' 1 'private Array at receiver policy'
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+enum[[:space:]]+ArrayAtReceiverPolicy' "$array_at_owner"; then
  fail "$array_at_owner must keep ArrayAtReceiverPolicy private"
fi
if grep -Eq 'ArrayAtReceiverPolicy|compile_array_like_at_builtin\(' "$array_at_caller"; then
  fail "$array_at_caller must use fixed Array at entries"
fi
require_fixed_string_count "$array_at_owner" 'fn compile_array_like_at_builtin(' 1 'private shared Array at compiler'
require_fixed_string_count "$array_at_owner" 'self.compile_array_like_at_builtin(' 2 'fixed Array at entry calls'
for array_at_wrapper in \
  compile_array_prototype_at_builtin \
  compile_typed_array_prototype_at_builtin
do
  require_fixed_string_count "$array_at_owner" "pub(super) fn ${array_at_wrapper}(" 1 "fixed Array at entry $array_at_wrapper"
  require_fixed_string_count "$array_at_caller" "self.${array_at_wrapper}(function)?" 1 "standard call to fixed Array at entry $array_at_wrapper"
done

# T10's complete Object.defineProperty descriptor family has one private owner.
# The standard dispatcher may call the fixed builtin entry, but neither it nor
# the parent may regain the raw descriptor carriers or Arguments-specialized
# implementation helpers.
object_define_property_parent="crates/scriptw-aot-wasm/src/builtins/object.rs"
object_define_property_file="crates/scriptw-aot-wasm/src/builtins/object/define_property.rs"
require_file "$object_define_property_file"
check_no_inline_legacy_includes "$object_define_property_file"
require_exact_line_count \
  "$object_define_property_parent" \
  'mod define_property;' \
  1 \
  'private Object.defineProperty module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+define_property;' "$object_define_property_parent"; then
  fail "$object_define_property_parent must keep define_property private"
fi
# Whole descriptor acquisition and validated publication use the shared actual
# descriptor kernel; the old Arguments-specialized carriers are retired.
require_fixed_string_count "$object_define_property_file" 'self.emit_to_property_descriptor(' 1 'whole descriptor acquisition caller'
require_fixed_string_count "$object_define_property_file" 'self.emit_object_define_entry_validated(' 1 'validated descriptor publication caller'
require_regex_count \
  "$object_define_property_file" \
  '^[[:space:]]*pub\(in crate::builtins\)[[:space:]]+fn[[:space:]]+compile_object_define_property_builtin[[:space:]]*\(' \
  1 \
  'Object.defineProperty builtin entry visibility'
if grep -Fq 'compile_object_define_property_builtin(' "$object_define_property_parent"; then
  fail "$object_define_property_parent must not regain the extracted Object.defineProperty entry"
fi
require_fixed_string_count \
  "$wasm_standard_builtins" \
  'self.compile_object_define_property_builtin(function)?' \
  1 \
  'standard dispatcher Object.defineProperty call'
check_raw_line_budget "$object_define_property_parent" 5840
check_raw_line_budget "$object_define_property_file" 2560

# T10's complete Object.getOwnPropertyDescriptor compiler has one private
# owner. The parent retains only its module declaration and the standard
# dispatcher retains one fixed builtin call.
object_get_own_descriptor_parent="crates/scriptw-aot-wasm/src/builtins/object.rs"
object_get_own_descriptor_file="crates/scriptw-aot-wasm/src/builtins/object/get_own_property_descriptor.rs"
require_file "$object_get_own_descriptor_file"
check_no_inline_legacy_includes "$object_get_own_descriptor_file"
require_exact_line_count \
  "$object_get_own_descriptor_parent" \
  'mod get_own_property_descriptor;' \
  1 \
  'private Object.getOwnPropertyDescriptor module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+get_own_property_descriptor;' "$object_get_own_descriptor_parent"; then
  fail "$object_get_own_descriptor_parent must keep get_own_property_descriptor private"
fi
if grep -Fq 'compile_object_get_own_property_descriptor_builtin(' "$object_get_own_descriptor_parent"; then
  fail "$object_get_own_descriptor_parent must not regain the extracted Object.getOwnPropertyDescriptor entry"
fi
require_regex_count \
  "$object_get_own_descriptor_file" \
  '^[[:space:]]*pub\(in crate::builtins\)[[:space:]]+fn[[:space:]]+compile_object_get_own_property_descriptor_builtin[[:space:]]*\(' \
  1 \
  'Object.getOwnPropertyDescriptor builtin entry visibility'
require_fixed_string_count \
  "$wasm_standard_builtins" \
  'self.compile_object_get_own_property_descriptor_builtin(function)?' \
  1 \
  'standard dispatcher Object.getOwnPropertyDescriptor call'
check_raw_line_budget "$object_get_own_descriptor_parent" 4400
check_raw_line_budget "$object_get_own_descriptor_file" 1480

# 10.5.5 Proxy [[GetOwnProperty]] is the descriptor owner's private child: the
# parent declares it privately and calls its one entry, and the child owns the
# live-slot read, the trap call, the recursive targetDesc and the conversion.
object_get_own_descriptor_proxy_file="crates/scriptw-aot-wasm/src/builtins/object/get_own_property_descriptor/proxy.rs"
require_file "$object_get_own_descriptor_proxy_file"
check_no_inline_legacy_includes "$object_get_own_descriptor_proxy_file"
require_exact_line_count "$object_get_own_descriptor_file" 'mod proxy;' 1 'private Proxy [[GetOwnProperty]] module declaration'
require_fixed_string_count "$object_get_own_descriptor_file" 'builder.emit_proxy_get_own_property_descriptor(' 1 'Proxy [[GetOwnProperty]] step call'
require_fixed_string_count "$object_get_own_descriptor_proxy_file" 'pub(super) fn emit_proxy_get_own_property_descriptor(' 1 'Proxy [[GetOwnProperty]] step entry'
require_fixed_string_count "$object_get_own_descriptor_proxy_file" 'self.emit_to_property_descriptor(' 1 'Proxy trap-result ToPropertyDescriptor call'
require_fixed_string_count "$object_get_own_descriptor_proxy_file" 'self.emit_complete_property_descriptor(' 1 'Proxy trap-result CompletePropertyDescriptor call'
require_fixed_string_count "$object_get_own_descriptor_proxy_file" 'self.emit_native_descriptor_object(' 1 'Proxy result FromPropertyDescriptor call'
if grep -Eq 'emit_function_or_proxy_call|emit_to_property_descriptor|getOwnPropertyDescriptor trap' "$object_get_own_descriptor_file"; then
  fail "$object_get_own_descriptor_file must leave the Proxy trap step to its private child"
fi
check_raw_line_budget "$object_get_own_descriptor_proxy_file" 560

# T10's complete Object.getOwnPropertyDescriptors compiler has one private
# owner. The parent retains only its module declaration and the standard
# dispatcher retains one fixed builtin call.
object_get_own_descriptors_parent="crates/scriptw-aot-wasm/src/builtins/object.rs"
object_get_own_descriptors_file="crates/scriptw-aot-wasm/src/builtins/object/get_own_property_descriptors.rs"
require_file "$object_get_own_descriptors_file"
check_no_inline_legacy_includes "$object_get_own_descriptors_file"
require_exact_line_count \
  "$object_get_own_descriptors_parent" \
  'mod get_own_property_descriptors;' \
  1 \
  'private Object.getOwnPropertyDescriptors module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+get_own_property_descriptors;' "$object_get_own_descriptors_parent"; then
  fail "$object_get_own_descriptors_parent must keep get_own_property_descriptors private"
fi
if grep -Fq 'compile_object_get_own_property_descriptors_builtin(' "$object_get_own_descriptors_parent"; then
  fail "$object_get_own_descriptors_parent must not regain the extracted Object.getOwnPropertyDescriptors entry"
fi
require_regex_count \
  "$object_get_own_descriptors_file" \
  '^[[:space:]]*pub\(in crate::builtins\)[[:space:]]+fn[[:space:]]+compile_object_get_own_property_descriptors_builtin[[:space:]]*\(' \
  1 \
  'Object.getOwnPropertyDescriptors builtin entry visibility'
require_fixed_string_count \
  "$wasm_standard_builtins" \
  'self.compile_object_get_own_property_descriptors_builtin(function)?' \
  1 \
  'standard dispatcher Object.getOwnPropertyDescriptors call'
check_raw_line_budget "$object_get_own_descriptors_parent" 4220
check_raw_line_budget "$object_get_own_descriptors_file" 220

# T10's complete Object.assign compiler has one private owner. The parent
# retains only its module declaration and standard dispatch retains one fixed
# builtin call.
object_assign_parent="crates/scriptw-aot-wasm/src/builtins/object.rs"
object_assign_file="crates/scriptw-aot-wasm/src/builtins/object/assign.rs"
require_file "$object_assign_file"
check_no_inline_legacy_includes "$object_assign_file"
require_exact_line_count \
  "$object_assign_parent" \
  'mod assign;' \
  1 \
  'private Object.assign module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+assign;' "$object_assign_parent"; then
  fail "$object_assign_parent must keep assign private"
fi
if grep -Fq 'compile_object_assign_builtin(' "$object_assign_parent"; then
  fail "$object_assign_parent must not regain the extracted Object.assign entry"
fi
require_regex_count \
  "$object_assign_file" \
  '^[[:space:]]*pub\(in crate::builtins\)[[:space:]]+fn[[:space:]]+compile_object_assign_builtin[[:space:]]*\(' \
  1 \
  'Object.assign builtin entry visibility'
require_fixed_string_count \
  "$wasm_standard_builtins" \
  'self.compile_object_assign_builtin(function)?' \
  1 \
  'standard dispatcher Object.assign call'
check_raw_line_budget "$object_assign_parent" 3950
check_raw_line_budget "$object_assign_file" 300

# T10's user-facing own-descriptor predicates. These three builtins have
# different input sources and observable coercion orders, but consume the same
# public [[GetOwnProperty]] protocol. Keep those decisions in one closed Rust
# domain and prevent the deleted Array/arguments/ordinary representation scans
# from returning in any wrapper.
own_descriptor_predicate_parent="crates/scriptw-aot-wasm/src/builtins/object.rs"
own_descriptor_predicate_file="crates/scriptw-aot-wasm/src/builtins/object/own_descriptor_predicate.rs"
require_file "$own_descriptor_predicate_file"
check_no_inline_legacy_includes "$own_descriptor_predicate_file"
require_exact_line_count \
  "$own_descriptor_predicate_parent" \
  'mod own_descriptor_predicate;' \
  1 \
  'private own-descriptor predicate module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+own_descriptor_predicate;' "$own_descriptor_predicate_parent"; then
  fail "$own_descriptor_predicate_parent must keep own_descriptor_predicate private"
fi
if grep -Eq 'OwnDescriptorPredicateBuiltin|compile_object_own_descriptor_predicate_builtin\(|own_descriptor_predicate::' "$own_descriptor_predicate_parent"; then
  fail "$own_descriptor_predicate_parent must not name, construct, call or import the private own-descriptor predicate policy"
fi
require_fixed_string_count \
  "$own_descriptor_predicate_file" \
  'enum OwnDescriptorPredicateBuiltin {' \
  1 \
  'closed own-descriptor predicate builtin domain'
require_fixed_string_count \
  "$own_descriptor_predicate_file" \
  'fn compile_object_own_descriptor_predicate_builtin(' \
  1 \
  'shared own-descriptor predicate compiler'
require_fixed_string_count \
  "$own_descriptor_predicate_file" \
  'compile_object_own_descriptor_predicate_builtin(' \
  4 \
  'own-descriptor predicate compiler definition and three wrapper calls'
require_regex_count "$own_descriptor_predicate_file" '^[[:space:]]*pub\(in crate::builtins\)[[:space:]]+fn[[:space:]]+compile_object_.*_builtin[[:space:]]*\(' 3 'own-descriptor predicate semantic wrapper visibility'
own_descriptor_predicate_body="$(braced_rust_item_source "$own_descriptor_predicate_file" '^fn[[:space:]]+compile_object_own_descriptor_predicate_builtin[[:space:]]*[(]')"
if grep -Eq '^[[:space:]]*_ =>|unreachable!\(' <<<"$own_descriptor_predicate_body"; then
  fail 'own-descriptor predicate compiler must keep its closed source/order/projection decisions exhaustive'
fi
require_text_regex_count "$own_descriptor_predicate_body" 'self\.emit_proxy_target_own_descriptor[(]' 1 'sole completed GetOwnProperty descriptor consumer'
require_text_regex_count "$own_descriptor_predicate_body" 'descriptor\.emit_enumerable_i32[(]' 1 'non-observing completed enumerable projection'
# The live two source arms fuse the prototype forms while preserving each
# required coercion order. No canonical public-property relookup is involved.
if ! awk '
  /OwnDescriptorPredicateBuiltin::ObjectHasOwn =>/ { object_arm = NR }
  /emit_value_to_object_locals/ && object_arm && !prototype_arm && !object_conversion { object_conversion = NR }
  /emit_value_to_property_key_locals/ && object_arm && !prototype_arm && !object_key { object_key = NR }
  /OwnDescriptorPredicateBuiltin::PrototypeHasOwnProperty/ && object_key && !prototype_arm { prototype_arm = NR }
  /emit_value_to_property_key_locals/ && prototype_arm && !prototype_key { prototype_key = NR }
  /emit_value_to_object_locals/ && prototype_arm && !prototype_conversion { prototype_conversion = NR }
  END { exit !(object_arm && object_conversion && object_key && prototype_arm && prototype_key && prototype_conversion \
    && object_arm < object_conversion && object_conversion < object_key && object_key < prototype_arm \
    && prototype_arm < prototype_key && prototype_key < prototype_conversion) }
' <<<"$own_descriptor_predicate_body"; then
  fail 'own-descriptor predicates must preserve object-first and prototype key-first conversion order'
fi
for raw_own_predicate_scan in HEAP_ PROXY_HANDLER_ emit_object_own_property_present emit_known_array_index_from_property_key load_i64_to_local_from_offset; do
  if grep -Fq "$raw_own_predicate_scan" <<<"$own_descriptor_predicate_body"; then
    fail "own-descriptor predicate compiler must not regain raw storage scans through $raw_own_predicate_scan"
  fi
done

for own_predicate_wrapper_spec in \
  'compile_object_has_own_builtin|compile_object_prototype_has_own_property_builtin|OwnDescriptorPredicateBuiltin::ObjectHasOwn' \
  'compile_object_prototype_has_own_property_builtin|compile_object_prototype_property_is_enumerable_builtin|OwnDescriptorPredicateBuiltin::PrototypeHasOwnProperty' \
  'compile_object_prototype_property_is_enumerable_builtin|END|OwnDescriptorPredicateBuiltin::PrototypePropertyIsEnumerable'
do
  wrapper="${own_predicate_wrapper_spec%%|*}"
  rest="${own_predicate_wrapper_spec#*|}"
  next_wrapper="${rest%%|*}"
  variant="${rest#*|}"
  if [ "$next_wrapper" = END ]; then
    wrapper_body="$(sed -n \
      "/^    pub(in crate::builtins) fn ${wrapper}(/,/^}/p" \
      "$own_descriptor_predicate_file")"
  else
    wrapper_body="$(sed -n \
      "/^    pub(in crate::builtins) fn ${wrapper}(/,/^    pub(in crate::builtins) fn ${next_wrapper}(/p" \
      "$own_descriptor_predicate_file")"
  fi
  if [ "$(grep -Fc 'self.compile_object_own_descriptor_predicate_builtin(' <<<"$wrapper_body" || true)" -ne 1 ] \
    || [ "$(grep -Fc "$variant" <<<"$wrapper_body" || true)" -ne 1 ] \
    || [ "$(grep -Fc 'self.' <<<"$wrapper_body" || true)" -ne 1 ]; then
    fail "$wrapper must be a one-call selection of $variant"
  fi
  if grep -Eq 'Instruction::|HEAP_|emit_|reserve_temp_local|StandardBuiltinId::' <<<"$wrapper_body"; then
    fail "$wrapper must not contain a representation-specific descriptor path"
  fi
done

# Measured after the later EnumerableOwnProperties extraction: 8,248 parent lines. The
# own-descriptor child remains 230 lines. The narrow margins are for maintenance
# of each owner.
check_raw_line_budget "$own_descriptor_predicate_parent" 8330
check_raw_line_budget "$own_descriptor_predicate_file" 260

require_fixed_string_count \
  crates/scriptw-cli/tests/cli/object.rs \
  'fn run_wasm_backend_succeeds_for_object_own_descriptor_predicates()' \
  1 \
  'exact own-descriptor-predicate CLI regression'
require_fixed_string_count \
  crates/scriptw-cli/tests/cli/object.rs \
  '"wasm_object_own_descriptor_predicates.js"' \
  1 \
  'own-descriptor-predicate fixture wiring'
if [ ! -f crates/scriptw-cli/tests/fixtures/wasm_object_own_descriptor_predicates.js ]; then
  fail 'own-descriptor-predicate fixture must remain present'
fi
if [ ! -f docs/rust-rewrite/contracts/own-descriptor-predicates.md ]; then
  fail 'own-descriptor-predicate contract must remain present'
fi

# T10's Object keys/entries/values policy has one private owner. The parent and
# standard dispatcher may invoke fixed semantic operations, but cannot construct
# or project the raw policy controlling diagnostics and result shape.
enumerable_own_properties_parent="crates/scriptw-aot-wasm/src/builtins/object.rs"
enumerable_own_properties_file="crates/scriptw-aot-wasm/src/builtins/object/enumerable_own_properties.rs"
require_file "$enumerable_own_properties_file"
check_no_inline_legacy_includes "$enumerable_own_properties_file"
require_exact_line_count \
  "$enumerable_own_properties_parent" \
  'mod enumerable_own_properties;' \
  1 \
  'private enumerable-own-properties module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+enumerable_own_properties;' "$enumerable_own_properties_parent"; then
  fail "$enumerable_own_properties_parent must keep enumerable_own_properties private"
fi
for enumerable_own_properties_non_owner in "$enumerable_own_properties_parent" "$wasm_standard_builtins"
do
  if grep -Eq 'EnumerableOwnProperties|compile_object_enumerable_own_properties_builtin\(|enumerable_own_properties::' "$enumerable_own_properties_non_owner"; then
    fail "$enumerable_own_properties_non_owner must not name, construct, call or import the private enumerable-own-properties policy"
  fi
done
require_fixed_string_count "$enumerable_own_properties_file" 'enum EnumerableOwnProperties {' 1 'closed enumerable-own-properties domain'
require_fixed_string_count \
  "$enumerable_own_properties_file" \
  'compile_object_enumerable_own_properties_builtin(' \
  4 \
  'private enumerable-own-properties compiler definition and wrapper calls'
require_regex_count \
  "$enumerable_own_properties_file" \
  '^[[:space:]]*pub\(in crate::builtins\)[[:space:]]+fn[[:space:]]+compile_object_(keys|entries|values)_builtin[[:space:]]*\(' \
  3 \
  'enumerable-own-properties semantic wrapper visibility'
enumerable_own_properties_body="$(sed -n \
  '/^    fn compile_object_enumerable_own_properties_builtin(/,/^    pub(in crate::builtins) fn compile_object_keys_builtin(/p' \
  "$enumerable_own_properties_file")"
# The shared emitter exhaustively selects diagnostics and result shape. The
# complete result-array owner, not the enum, carries the consuming publication.
if grep -Eq '_ =>' <<<"$enumerable_own_properties_body"; then
  fail 'enumerable-own-properties must exhaustively match its closed operation domain'
fi
require_regex_count "$enumerable_own_properties_file" '^pub\(super\) struct NativeObjectResultArray \{' 1 'private completed native Object result-array owner'
result_array_attributes="$(rust_item_attributes_source "$enumerable_own_properties_file" '^pub[(]super[)][[:space:]]+struct[[:space:]]+NativeObjectResultArray[[:space:]]*[{]')"
if grep -Eq '#\[derive\([^]]*(Clone|Copy)' <<<"$result_array_attributes"; then
  fail 'NativeObjectResultArray must retain its consuming publication capability'
fi
require_regex_count "$enumerable_own_properties_file" '^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+finish[[:space:]]*\(' 1 'consuming Object result-array publisher'
result_array_finish="$(braced_rust_item_source "$enumerable_own_properties_file" '^pub[(]super[)][[:space:]]+fn[[:space:]]+finish[[:space:]]*[(]')"
require_text_regex_count "$result_array_finish" '^[[:space:]]*self,$' 1 'result-array finish consumes its rooted owner'
require_text_regex_count "$enumerable_own_properties_body" 'self\.emit_object_own_property_keys[(]' 1 'completed Object own-key snapshot'
require_text_regex_count "$enumerable_own_properties_body" 'self\.emit_proxy_target_own_descriptor[(]' 1 'lazy completed Object descriptor observation'

for enumerable_own_properties_wrapper_spec in \
  'compile_object_keys_builtin|compile_object_entries_builtin|EnumerableOwnProperties::Keys' \
  'compile_object_entries_builtin|compile_object_values_builtin|EnumerableOwnProperties::Entries' \
  'compile_object_values_builtin|END|EnumerableOwnProperties::Values'
do
  wrapper="${enumerable_own_properties_wrapper_spec%%|*}"
  rest="${enumerable_own_properties_wrapper_spec#*|}"
  next_wrapper="${rest%%|*}"
  variant="${rest#*|}"
  if [ "$next_wrapper" = END ]; then
    wrapper_body="$(sed -n "/^    pub(in crate::builtins) fn ${wrapper}(/,/^}/p" "$enumerable_own_properties_file")"
  else
    wrapper_body="$(sed -n "/^    pub(in crate::builtins) fn ${wrapper}(/,/^    pub(in crate::builtins) fn ${next_wrapper}(/p" "$enumerable_own_properties_file")"
  fi
  if [ "$(grep -Fc 'self.compile_object_enumerable_own_properties_builtin(' <<<"$wrapper_body" || true)" -ne 1 ] \
    || [ "$(grep -Fc "$variant" <<<"$wrapper_body" || true)" -ne 1 ] \
    || [ "$(grep -Fc 'self.' <<<"$wrapper_body" || true)" -ne 1 ]; then
    fail "$wrapper must be a one-call selection of $variant"
  fi
  if grep -Eq 'Instruction::|emit_|reserve_temp_local|StandardBuiltinId::' <<<"$wrapper_body"; then
    fail "$wrapper must not contain enumerable-own-properties implementation policy"
  fi
  require_fixed_string_count "$wasm_standard_builtins" "self.${wrapper}(" 1 "standard dispatcher call to $wrapper"
done
check_raw_line_budget "$enumerable_own_properties_file" 380

# T10's Object integrity-test policy has one private owner. The parent and
# standard dispatcher may invoke fixed isSealed/isFrozen operations, but cannot
# construct or project the raw policy that controls the writability branch.
integrity_test_parent="crates/scriptw-aot-wasm/src/builtins/object.rs"
integrity_test_file="crates/scriptw-aot-wasm/src/builtins/object/integrity_test.rs"
require_file "$integrity_test_file"
check_no_inline_legacy_includes "$integrity_test_file"
require_exact_line_count \
  "$integrity_test_parent" \
  'mod integrity_test;' \
  1 \
  'private integrity-test module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+integrity_test;' "$integrity_test_parent"; then
  fail "$integrity_test_parent must keep integrity_test private"
fi
for integrity_test_non_owner in "$integrity_test_parent" "$wasm_standard_builtins"
do
  if grep -Eq 'IntegrityTest|compile_object_integrity_test_builtin\(|integrity_test::' "$integrity_test_non_owner"; then
    fail "$integrity_test_non_owner must not name, construct, call or import the private integrity-test policy"
  fi
done
require_fixed_string_count "$integrity_test_file" 'enum IntegrityTest {' 1 'closed integrity-test domain'
require_fixed_string_count "$integrity_test_file" 'IntegrityTest' 6 'integrity-test policy uses'
require_fixed_string_count "$integrity_test_file" 'IntegrityTest::Sealed' 2 'sealed policy uses'
require_fixed_string_count "$integrity_test_file" 'IntegrityTest::Frozen' 2 'frozen policy uses'
require_fixed_string_count \
  "$integrity_test_file" \
  'compile_object_integrity_test_builtin(' \
  3 \
  'private integrity-test compiler definition and wrapper calls'
require_regex_count \
  "$integrity_test_file" \
  '^[[:space:]]*pub\(in crate::builtins\)[[:space:]]+fn[[:space:]]+compile_object_is_(sealed|frozen)_builtin[[:space:]]*\(' \
  2 \
  'integrity-test semantic wrapper visibility'
integrity_test_body="$(sed -n \
  '/^    fn compile_object_integrity_test_builtin(/,/^    pub(in crate::builtins) fn compile_object_is_sealed_builtin(/p' \
  "$integrity_test_file")"
if grep -Eq '^[[:space:]]*_ =>|unreachable!\(' <<<"$integrity_test_body"; then
  fail 'integrity-test compiler must keep its closed policy exhaustive'
fi
require_text_regex_count "$integrity_test_body" 'self\.emit_object_is_extensible[(]' 1 'TestIntegrityLevel complete extensibility observation'
require_text_regex_count "$integrity_test_body" 'self\.emit_proxy_target_own_descriptor[(]' 1 'TestIntegrityLevel completed descriptor observation'

for integrity_test_wrapper_spec in \
  'compile_object_is_sealed_builtin|compile_object_is_frozen_builtin|IntegrityTest::Sealed' \
  'compile_object_is_frozen_builtin|END|IntegrityTest::Frozen'
do
  wrapper="${integrity_test_wrapper_spec%%|*}"
  rest="${integrity_test_wrapper_spec#*|}"
  next_wrapper="${rest%%|*}"
  variant="${rest#*|}"
  if [ "$next_wrapper" = END ]; then
    wrapper_body="$(sed -n "/^    pub(in crate::builtins) fn ${wrapper}(/,/^}/p" "$integrity_test_file")"
  else
    wrapper_body="$(sed -n "/^    pub(in crate::builtins) fn ${wrapper}(/,/^    pub(in crate::builtins) fn ${next_wrapper}(/p" "$integrity_test_file")"
  fi
  if [ "$(grep -Fc 'self.compile_object_integrity_test_builtin(' <<<"$wrapper_body" || true)" -ne 1 ] \
    || [ "$(grep -Fc "$variant" <<<"$wrapper_body" || true)" -ne 1 ] \
    || [ "$(grep -Fc 'self.' <<<"$wrapper_body" || true)" -ne 1 ]; then
    fail "$wrapper must be a one-call selection of $variant"
  fi
  if grep -Eq 'Instruction::|emit_|reserve_temp_local|StandardBuiltinId::' <<<"$wrapper_body"; then
    fail "$wrapper must not contain integrity-test implementation policy"
  fi
  require_fixed_string_count "$wasm_standard_builtins" "self.${wrapper}(" 1 "standard dispatcher call to $wrapper"
done
check_raw_line_budget "$integrity_test_file" 250

# T10's Annex-B prototype accessor lookup policy has one private owner. The
# parent and standard dispatcher may invoke the two fixed semantic operations,
# but cannot construct or project the raw getter/setter selection.
prototype_lookup_parent="crates/scriptw-aot-wasm/src/builtins/object.rs"
prototype_lookup_file="crates/scriptw-aot-wasm/src/builtins/object/prototype_lookup.rs"
require_file "$prototype_lookup_file"
check_no_inline_legacy_includes "$prototype_lookup_file"
require_exact_line_count \
  "$prototype_lookup_parent" \
  'mod prototype_lookup;' \
  1 \
  'private prototype-lookup module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+prototype_lookup;' "$prototype_lookup_parent"; then
  fail "$prototype_lookup_parent must keep prototype_lookup private"
fi
if grep -Eq 'PrototypeLookup|compile_object_prototype_lookup_builtin\(|prototype_lookup::' "$prototype_lookup_parent"; then
  fail "$prototype_lookup_parent must not name, construct, call or import the private prototype-lookup policy"
fi
if grep -Eq '(^|::)PrototypeLookup(::|[[:space:]]*(,|;|}|as[[:space:]]))|compile_object_prototype_lookup_builtin\(|prototype_lookup::' "$wasm_standard_builtins"; then
  fail "$wasm_standard_builtins must not construct, call or import the private prototype-lookup policy"
fi
require_fixed_string_count "$prototype_lookup_file" 'enum PrototypeLookup {' 1 'closed prototype-lookup domain'
require_fixed_string_count "$prototype_lookup_file" 'PrototypeLookup' 6 'prototype-lookup policy uses'
require_fixed_string_count "$prototype_lookup_file" 'PrototypeLookup::Getter' 2 'getter policy uses'
require_fixed_string_count "$prototype_lookup_file" 'PrototypeLookup::Setter' 2 'setter policy uses'
require_fixed_string_count \
  "$prototype_lookup_file" \
  'compile_object_prototype_lookup_builtin(' \
  3 \
  'private prototype-lookup compiler definition and wrapper calls'
require_regex_count \
  "$prototype_lookup_file" \
  '^[[:space:]]*pub\(in crate::builtins\)[[:space:]]+fn[[:space:]]+compile_object_prototype_lookup_(getter|setter)_builtin[[:space:]]*\(' \
  2 \
  'prototype-lookup semantic wrapper visibility'
prototype_lookup_body="$(sed -n \
  '/^    fn compile_object_prototype_lookup_builtin(/,/^    pub(in crate::builtins) fn compile_object_prototype_lookup_getter_builtin(/p' \
  "$prototype_lookup_file")"
if grep -Eq '^[[:space:]]*_ =>|unreachable!\(' <<<"$prototype_lookup_body"; then
  fail 'prototype-lookup compiler must keep its closed getter/setter selection exhaustive'
fi
require_text_regex_count "$prototype_lookup_body" 'self\.emit_proxy_target_own_descriptor[(]' 1 'completed prototype own-descriptor observation'
require_text_regex_count "$prototype_lookup_body" 'self\.emit_object_get_prototype_of[(]' 1 'complete prototype traversal authority'

for prototype_lookup_wrapper_spec in \
  'compile_object_prototype_lookup_getter_builtin|compile_object_prototype_lookup_setter_builtin|PrototypeLookup::Getter' \
  'compile_object_prototype_lookup_setter_builtin|END|PrototypeLookup::Setter'
do
  wrapper="${prototype_lookup_wrapper_spec%%|*}"
  rest="${prototype_lookup_wrapper_spec#*|}"
  next_wrapper="${rest%%|*}"
  variant="${rest#*|}"
  if [ "$next_wrapper" = END ]; then
    wrapper_body="$(sed -n "/^    pub(in crate::builtins) fn ${wrapper}(/,/^}/p" "$prototype_lookup_file")"
  else
    wrapper_body="$(sed -n "/^    pub(in crate::builtins) fn ${wrapper}(/,/^    pub(in crate::builtins) fn ${next_wrapper}(/p" "$prototype_lookup_file")"
  fi
  if [ "$(grep -Fc 'self.compile_object_prototype_lookup_builtin(' <<<"$wrapper_body" || true)" -ne 1 ] \
    || [ "$(grep -Fc "$variant" <<<"$wrapper_body" || true)" -ne 1 ] \
    || [ "$(grep -Fc 'self.' <<<"$wrapper_body" || true)" -ne 1 ]; then
    fail "$wrapper must be a one-call selection of $variant"
  fi
  if grep -Eq 'Instruction::|emit_|reserve_temp_local|StandardBuiltinId::' <<<"$wrapper_body"; then
    fail "$wrapper must not contain prototype-lookup implementation policy"
  fi
  require_fixed_string_count "$wasm_standard_builtins" "self.${wrapper}(" 1 "standard dispatcher call to $wrapper"
done
check_raw_line_budget "$prototype_lookup_file" 180
require_fixed_string_count \
  crates/scriptw-cli/tests/cli/object.rs \
  'fn run_wasm_backend_preserves_object_builtin_policy_domains()' \
  1 \
  'exact Object policy-domain CLI regression'
require_fixed_string_count \
  crates/scriptw-cli/tests/cli/object.rs \
  'fn run_wasm_backend_succeeds_for_object_prototype_accessor_lookup_fixture()' \
  1 \
  'exact prototype-accessor lookup CLI regression'
if [ ! -f docs/rust-rewrite/contracts/object-builtin-policy-domains.md ]; then
  fail 'Object builtin policy-domain contract must remain present'
fi

# HasProperty's public wrapper and private helper body consume the registered
# typed input/key/Environment and complete result, without a parallel tagged ABI.
require_fixed_string_count crates/scriptw-aot-wasm/src/objects/has_property.rs 'pub(crate) fn emit_object_has_property_i32(' 1 'actual whole HasProperty entry'
require_fixed_string_count crates/scriptw-aot-wasm/src/objects/has_property.rs 'pub(crate) fn compile_object_has_property_helper(' 1 'actual shared HasProperty body'
require_fixed_string_count crates/scriptw-aot-wasm/src/objects/has_property.rs 'fn emit_has_property_dispatch(' 1 'private HasProperty dispatch'
require_fixed_string_count \
  crates/scriptw-engine/src/tests/objects.rs \
  'fn wasm_backend_has_property_dispatches_every_live_exotic_branch()' \
  1 \
  'complete HasProperty runtime regression'

# Recursive target descriptor acquisition has one consumed completed owner.
# Its real native GPD call runs before nonobserving private stored-field reads.
wasm_proxy_target_descriptor="crates/scriptw-aot-wasm/src/objects/proxy_target_descriptor.rs"
require_file "$wasm_proxy_target_descriptor"
require_exact_line_count crates/scriptw-aot-wasm/src/objects.rs 'mod proxy_target_descriptor;' 1 'private completed target descriptor module'
check_no_inline_legacy_includes "$wasm_proxy_target_descriptor"
require_fixed_string_count "$wasm_proxy_target_descriptor" 'pub(crate) struct CompletedProxyTargetDescriptor {' 1 'opaque completed descriptor owner'
for completed_descriptor_entry in emit_proxy_target_own_descriptor emit_proxy_target_descriptor_fact; do
  require_fixed_string_count "$wasm_proxy_target_descriptor" "fn ${completed_descriptor_entry}(" 1 'completed target descriptor producer'
done
# Meaningful Get/Set/Reflect compiler fixtures remain attached below.
require_fixed_string_count crates/scriptw-cli/tests/cli/object.rs 'fn run_wasm_backend_succeeds_for_proxy_get_direct_descriptor_invariants()' 1 'exact Proxy-Get direct-descriptor CLI regression'
require_fixed_string_count crates/scriptw-cli/tests/cli/object.rs '"wasm_proxy_get_direct_descriptor_invariants.js"' 1 'Proxy-Get direct-descriptor fixture wiring'
if [ ! -f crates/scriptw-cli/tests/fixtures/wasm_proxy_get_direct_descriptor_invariants.js ]; then
  fail 'Proxy [[Get]] direct-descriptor invariant fixture must remain present'
fi
proxy_set_cli="crates/scriptw-cli/tests/cli/object.rs"
require_active_wasm_cli_rust_test \
  "$proxy_set_cli" \
  run_wasm_backend_succeeds_for_proxy_set_direct_descriptor_invariants \
  'Proxy-Set direct-descriptor CLI regression'
proxy_set_cli_test="$(braced_rust_item_source "$proxy_set_cli" '^fn[[:space:]]+run_wasm_backend_succeeds_for_proxy_set_direct_descriptor_invariants[[:space:]]*[(]')"
require_text_regex_count "$proxy_set_cli_test" '^[[:space:]]*"wasm_proxy_set_direct_descriptor_invariants\.js",$' 1 'Proxy-Set direct-descriptor fixture wiring'
if [ ! -f crates/scriptw-cli/tests/fixtures/wasm_proxy_set_direct_descriptor_invariants.js ]; then
  fail 'Proxy [[Set]] direct-descriptor invariant fixture must remain present'
fi
require_active_wasm_cli_rust_test \
  "$proxy_set_cli" \
  run_wasm_backend_succeeds_for_proxy_reflect_set_handler_protocol \
  'direct Reflect Set handler-protocol CLI regression'
proxy_reflect_set_cli_test="$(braced_rust_item_source "$proxy_set_cli" '^fn[[:space:]]+run_wasm_backend_succeeds_for_proxy_reflect_set_handler_protocol[[:space:]]*[(]')"
require_text_regex_count "$proxy_reflect_set_cli_test" 'fixture_path\("wasm_proxy_reflect_set_handler_protocol\.js"\)' 1 'direct Reflect Set handler-protocol fixture wiring'
if [ ! -f crates/scriptw-cli/tests/fixtures/wasm_proxy_reflect_set_handler_protocol.js ]; then
  fail 'direct Reflect Set handler-protocol fixture must remain present'
fi
builtin_arg_presence='emit_builtin_arg_is_present_i32('
require_regex_count crates/scriptw-aot-wasm/src/functions/argument_vectors.rs '^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_builtin_arg_is_present_i32[[:space:]]*\(' 1 'actual argument-vector optional-presence authority'
require_tree_regex_count crates/scriptw-aot-wasm/src '^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_builtin_arg_is_present_i32[[:space:]]*\(' 1 'sole builtin argument-presence owner'
# Reflect's two consumed routes cover receiver defaults and Construct newTarget.
require_fixed_string_count crates/scriptw-aot-wasm/src/builtins/reflect.rs "$builtin_arg_presence" 2 'Reflect optional-argument presence consumers'
require_active_wasm_cli_rust_test \
  "$proxy_set_cli" \
  run_wasm_backend_distinguishes_omitted_reflect_optional_arguments \
  'Reflect optional-argument presence CLI regression'
reflect_optional_presence_cli_test="$(braced_rust_item_source "$proxy_set_cli" '^fn[[:space:]]+run_wasm_backend_distinguishes_omitted_reflect_optional_arguments[[:space:]]*[(]')"
require_text_regex_count "$reflect_optional_presence_cli_test" 'fixture_path\("wasm_reflect_optional_argument_presence\.js"\)' 1 'Reflect optional-argument presence fixture wiring'
if [ ! -f crates/scriptw-cli/tests/fixtures/wasm_reflect_optional_argument_presence.js ]; then
  fail 'Reflect optional-argument presence fixture must remain present'
fi
# All six property entries share the one complete key conversion branch.
require_fixed_string_count crates/scriptw-aot-wasm/src/builtins/reflect.rs 'emit_value_to_property_key_locals(' 1 'shared Reflect complete PropertyKey authority'
require_fixed_string_count \
  crates/scriptw-aot-wasm/src/builtins/reflect.rs \
  'emit_value_to_property_key_payload(' \
  0 \
  'legacy payload-only Reflect ToPropertyKey consumers'
require_active_wasm_cli_rust_test \
  "$proxy_set_cli" \
  run_wasm_backend_preserves_reflect_property_key_conversion \
  'Reflect property-key conversion CLI regression'
reflect_property_key_cli_test="$(braced_rust_item_source "$proxy_set_cli" '^fn[[:space:]]+run_wasm_backend_preserves_reflect_property_key_conversion[[:space:]]*[(]')"
require_text_regex_count "$reflect_property_key_cli_test" 'fixture_path\("wasm_reflect_property_key_conversion\.js"\)' 1 'Reflect property-key conversion fixture wiring'
if [ ! -f crates/scriptw-cli/tests/fixtures/wasm_reflect_property_key_conversion.js ]; then
  fail 'Reflect property-key conversion fixture must remain present'
fi
if grep -RFl --include='*.rs' 'emit_proxy_array_target_own_descriptor_flags' crates/scriptw-aot-wasm/src >/dev/null; then
  fail 'Array-only Proxy own-descriptor mirrors must not bypass the typed authority'
fi

# Descriptor values/getters/setters are read as complete values through the
# completed target descriptor owner. Raw-zero and offset projections are gone.

# One live Proxy-slot reader roots its concrete record and exposes opaque whole
# target/handler values to the operation-specific revocation and Call routes.
require_fixed_string_count crates/scriptw-aot-wasm/src/objects.rs 'pub(crate) fn emit_load_live_proxy_slots(' 1 'live Proxy-slot reader authority'
require_tree_regex_count crates/scriptw-aot-wasm/src '^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_load_live_proxy_slots[[:space:]]*\(' 1 'sole live Proxy-slot reader'
# Reflect entries delegate to their shared whole-value property algorithms;
# neither handler-offset whitelists nor duplicated inline trap recipes apply.

# Native optional Get and terminal Delete consume one rooted prefix pipeline.
# Only its private destination changes the final operation or shorted result.
wasm_expressions="crates/scriptw-aot-wasm/src/expressions.rs"
wasm_optional_chain="crates/scriptw-aot-wasm/src/expressions/optional_chain.rs"
require_file "$wasm_optional_chain"
require_exact_line_count "$wasm_expressions" 'mod optional_chain;' 1 'private native optional pipeline attachment'
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+mod[[:space:]]+optional_chain;' "$wasm_expressions"; then
  fail "$wasm_expressions must keep its optional pipeline private"
fi
require_regex_count "$wasm_optional_chain" "^enum TerminalDestination<'a>" 1 'private native optional destination domain'
require_fixed_string_count "$wasm_optional_chain" 'fn compile_optional_chain_to_destination(' 1 'one rooted native optional prefix body'
require_fixed_string_count "$wasm_optional_chain" 'self.compile_optional_chain_to_destination(' 2 'Get and Delete consume the same prefix pipeline'
for native_optional_entry in compile_optional_property_chain_to_value compile_delete_optional_property_chain_to_value; do
  require_fixed_string_count "$wasm_optional_chain" "pub(crate) fn ${native_optional_entry}(" 1 'actual native optional destination entry'
  require_fixed_string_count "$wasm_expressions" "fn ${native_optional_entry}(" 0 'no parent optional pipeline copy'
done
require_fixed_string_count "$wasm_expressions" 'compile_optional_property_chain_to_value(' 2 'actual optional Value and captured Call dispatch'
require_fixed_string_count "$wasm_expressions" 'self.compile_delete_optional_property_chain_to_value(' 1 'actual terminal optional Delete dispatch'
require_fixed_string_count "$wasm_optional_chain" 'self.compile_delete_property_i32(' 1 'terminal optional Delete uses the existing whole property algorithm'
check_no_inline_legacy_includes "$wasm_optional_chain"
check_raw_line_budget "$wasm_optional_chain" 300
check_raw_line_budget "$wasm_expressions" 1980
wasm_regexp_expressions="crates/scriptw-aot-wasm/src/expressions/regexp_program.rs"
require_file "$wasm_regexp_expressions"
require_exact_line_count "$wasm_expressions" 'mod regexp_program;' 1 'private RegExp expression compiler owner'
for regexp_expression_entry in emit_collected_regexp_program emit_regexp_program_slots emit_runtime_regexp_program_slots emit_regexp_compiler_failures emit_gc_string_contains_ascii_byte_i32 emit_regexp_program_with_compatible_flags compile_regexp_literal_to_value; do
  require_fixed_string_count "$wasm_regexp_expressions" "fn ${regexp_expression_entry}(" 1 'RegExp expression algorithm owner'
  require_fixed_string_count "$wasm_expressions" "fn ${regexp_expression_entry}(" 0 'no parent RegExp expression algorithm copy'
done
require_fixed_string_count "$wasm_regexp_expressions" 'pub(super) fn compile_regexp_literal_to_value(' 1 'RegExp literal entry stays inside expressions'
require_fixed_string_count "$wasm_expressions" 'self.compile_regexp_literal_to_value(' 1 'RegExp expression dispatch consumes its private owner'
check_no_inline_legacy_includes "$wasm_regexp_expressions"
check_raw_line_budget "$wasm_regexp_expressions" 510
wasm_regexp_candidate="crates/scriptw-aot-wasm/src/expressions/regexp_program/candidate.rs"
require_file "$wasm_regexp_candidate"
require_exact_line_count "$wasm_regexp_expressions" 'mod candidate;' 1 'private finite RegExp candidate owner'
for regexp_candidate_entry in compile_regexp_program_candidate_hook emit_static_regexp_program_span emit_static_regexp_program_payload emit_regexp_program_metadata; do
  require_fixed_string_count "$wasm_regexp_candidate" "fn ${regexp_candidate_entry}(" 1 'finite RegExp candidate algorithm owner'
  require_fixed_string_count "$wasm_regexp_expressions" "fn ${regexp_candidate_entry}(" 0 'no parent finite RegExp candidate algorithm copy'
done
require_fixed_string_count crates/scriptw-aot-wasm/src/program_hooks.rs 'self.compile_regexp_program_candidate_hook()' 1 'actual RegExp candidate hook dispatch'
check_no_inline_legacy_includes "$wasm_regexp_candidate"
check_raw_line_budget "$wasm_regexp_candidate" 300

# T02 gives T09's private-element environment, storage and access lifecycle one
# private backend owner. Keep the complete family together: widening only some
# methods back into objects.rs would recreate the shared ownership surface this
# boundary removes.
wasm_objects="crates/scriptw-aot-wasm/src/objects.rs"
wasm_private_elements="crates/scriptw-aot-wasm/src/objects/private_elements.rs"
require_file "$wasm_private_elements"
require_exact_line_count "$wasm_objects" 'mod private_elements;' 1 'private-elements module declaration'
require_regex_count \
  "$wasm_objects" \
  '^(pub(\([^)]*\))?[[:space:]]+)?mod[[:space:]]+private_elements;' \
  1 \
  'private-elements module declarations'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+private_elements;' "$wasm_objects"; then
  fail "$wasm_objects must keep private_elements private"
fi

# Token lookup, table operations and language reads/writes stay in the private
# GC owner; raw carrier variants and the old method count are not contracts.
for private_element_entry in emit_private_name_token_to_local emit_private_element_find emit_private_brand_has_i32 emit_private_brand_add emit_private_field_define compile_private_read_to_locals compile_private_write_to_locals; do
  require_fixed_string_count "$wasm_private_elements" "fn ${private_element_entry}(" 1 'actual private-element algorithm owner'
  require_fixed_string_count "$wasm_objects" "fn ${private_element_entry}(" 0 'no parent private-element algorithm copy'
done
require_fixed_string_count "$wasm_private_elements" 'fn compile_private_field_define_helper(' 1 'complete private-field definition helper owner'
require_fixed_string_count "$wasm_private_elements" 'PrivateFieldDefineArguments::new(' 1 'private-field definition uses its typed helper call'
require_fixed_string_count "$wasm_objects" 'fn compile_private_field_define_helper(' 0 'no parent private-field helper copy'

check_no_inline_legacy_includes "$wasm_private_elements"
# The current whole-GC object owner is 4,178 lines. Literal property definition
# is one 235-line private body consumed by ordinary and resumed construction.
wasm_object_literal_property="crates/scriptw-aot-wasm/src/objects/object_literal_property.rs"
require_file "$wasm_object_literal_property"
require_exact_line_count "$wasm_objects" 'mod object_literal_property;' 1 'private shared native literal property owner'
if grep -Eq '^pub(\([^)]*\))?[[:space:]]+mod[[:space:]]+object_literal_property;' "$wasm_objects"; then
  fail "$wasm_objects must keep literal property definitions private"
fi
require_fixed_string_count "$wasm_object_literal_property" 'pub(super) fn emit_object_literal_property(' 1 'sole complete native literal property body'
require_fixed_string_count "$wasm_objects" 'fn emit_object_literal_property(' 0 'no parent literal property body copy'
require_fixed_string_count "$wasm_objects" 'self.emit_object_literal_property(' 1 'ordinary literal consumes the shared property body'
require_fixed_string_count "$wasm_object_literal_property" 'self.emit_object_literal_property(' 1 'retained object consumes the same property body'
require_fixed_string_count "$wasm_object_literal_property" 'pub(crate) fn compile_object_property_definition_payload(' 1 'actual checked retained-object definition entry'
require_fixed_string_count "$wasm_expressions" 'self.compile_object_property_definition_payload(' 1 'actual retained-object expression dispatch'
check_no_inline_legacy_includes "$wasm_object_literal_property"
check_raw_line_budget "$wasm_objects" 4250
check_raw_line_budget "$wasm_object_literal_property" 250
check_raw_line_budget "$wasm_private_elements" 1050

# The Arguments ParameterMap fact is captured, borrowed by indexed operations,
# and consumed by one private child. No parent or sibling can construct its
# paired mapped/slot locals directly.
wasm_functions=crates/scriptw-aot-wasm/src/functions.rs
wasm_arguments_index_mapping=crates/scriptw-aot-wasm/src/functions/arguments_index_mapping.rs
require_file "$wasm_arguments_index_mapping"
require_exact_line_count \
  "$wasm_functions" \
  'mod arguments_index_mapping;' \
  1 \
  'private arguments_index_mapping module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+arguments_index_mapping;' "$wasm_functions"; then
  fail "$wasm_functions must keep arguments_index_mapping private"
fi
require_fixed_string_count \
  "$wasm_functions" \
  'arguments_index_mapping::' \
  0 \
  'arguments_index_mapping imports or re-exports'
# The captured ParameterMap result is a nullable BindingCell reference. Its
# typed read/write owner preserves alias identity independently of descriptors.
for arguments_mapping_entry in emit_arguments_index_mapping emit_arguments_parameter_map_read emit_arguments_parameter_map_write; do
  require_fixed_string_count "$wasm_arguments_index_mapping" "fn ${arguments_mapping_entry}(" 1 'actual Arguments binding-cell owner'
  require_fixed_string_count "$wasm_functions" "fn ${arguments_mapping_entry}(" 0 'no parent Arguments mapping copy'
done

check_no_inline_legacy_includes "$wasm_arguments_index_mapping"
check_raw_line_budget "$wasm_arguments_index_mapping" 180

# A created realm's Array prototype progresses from reserved storage to an
# initialized Array exotic object in one private child. The parent and siblings
# can use the inferred states, but cannot name or construct either state.
wasm_created_realm_array_prototype=crates/scriptw-aot-wasm/src/functions/created_realm_array_prototype.rs
require_file "$wasm_created_realm_array_prototype"
require_exact_line_count \
  "$wasm_functions" \
  'mod created_realm_array_prototype;' \
  1 \
  'private created_realm_array_prototype module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+created_realm_array_prototype;' "$wasm_functions"; then
  fail "$wasm_functions must keep created_realm_array_prototype private"
fi
require_fixed_string_count \
  "$wasm_functions" \
  'created_realm_array_prototype::' \
  0 \
  'created-Realm Array prototype imports or re-exports'

# Reserved storage cannot be used as the completed Array prototype. The GC
# slot/value witnesses and consuming transition are private to this child.
for created_array_transition in reserve_realm_array_prototype_local emit_initialize_realm_array_prototype emit_define_realm_array_prototype_data_with_flags emit_bind_realm_array_constructor_prototype release_realm_array_prototype_local; do
  require_fixed_string_count "$wasm_created_realm_array_prototype" "fn ${created_array_transition}(" 1 'actual created-Realm Array lifecycle owner'
  require_fixed_string_count "$wasm_functions" "fn ${created_array_transition}(" 0 'no parent Array prototype lifecycle copy'
done

check_no_inline_legacy_includes "$wasm_created_realm_array_prototype"
check_raw_line_budget "$wasm_created_realm_array_prototype" 220

# Required ordinary default prototypes have one closed selector and one typed
# resolved-Realm witness owner. The parent sees the witness only as an inferred
# value; its raw field and construction stay private to this child.
wasm_required_resolved_realm_ordinary_prototype=crates/scriptw-aot-wasm/src/functions/required_resolved_realm_ordinary_prototype.rs
require_file "$wasm_required_resolved_realm_ordinary_prototype"
require_exact_line_count \
  "$wasm_functions" \
  'mod required_resolved_realm_ordinary_prototype;' \
  1 \
  'private required_resolved_realm_ordinary_prototype module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+required_resolved_realm_ordinary_prototype;' "$wasm_functions"; then
  fail "$wasm_functions must keep required_resolved_realm_ordinary_prototype private"
fi
require_exact_line_count \
  "$wasm_functions" \
  'pub(crate) use required_resolved_realm_ordinary_prototype::OrdinaryDefaultPrototype;' \
  1 \
  'OrdinaryDefaultPrototype narrow re-export'
require_fixed_string_count \
  "$wasm_functions" \
  'required_resolved_realm_ordinary_prototype::' \
  1 \
  'required resolved-Realm ordinary prototype imports or re-exports'
require_tree_regex_count \
  crates/scriptw-aot-wasm/src \
  '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?enum[[:space:]]+OrdinaryDefaultPrototype[[:space:]]*\{' \
  1 \
  'OrdinaryDefaultPrototype backend owner'
require_regex_count \
  "$wasm_functions" \
  '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?enum[[:space:]]+OrdinaryDefaultPrototype[[:space:]]*\{' \
  0 \
  'OrdinaryDefaultPrototype parent copies'
# The exhaustive slot selector supplies a typed non-Array intrinsic or the
# separately typed Array root. No fixed variant count or byte offset is needed.
require_fixed_string_count "$wasm_required_resolved_realm_ordinary_prototype" 'pub(super) struct ResolvedRealmOrdinaryPrototypeLocal(ValueLocals);' 1 'completed whole prototype witness'
require_fixed_string_count "$wasm_required_resolved_realm_ordinary_prototype" 'const fn slot(self) -> Option<NonArrayRealmIntrinsicSlot>' 1 'closed intrinsic selector owner'
for required_prototype_entry in emit_get_prototype_from_constructor emit_required_function_realm_ordinary_prototype_completion emit_required_new_target_realm_ordinary_prototype; do
  require_fixed_string_count "$wasm_required_resolved_realm_ordinary_prototype" "fn ${required_prototype_entry}(" 1 'actual constructor/prototype completion owner'
  require_fixed_string_count "$wasm_functions" "fn ${required_prototype_entry}(" 0 'no parent prototype algorithm copy'
done

check_no_inline_legacy_includes "$wasm_required_resolved_realm_ordinary_prototype"
# Rooted resolved-Realm/prototype completion now measures 249 formatted lines.
check_raw_line_budget "$wasm_required_resolved_realm_ordinary_prototype" 270

# The active function Realm's Array prototype is a one-shot proof: one private
# child constructs it, and five consumers install it with an Array payload.
# Iterator-toArray and Proxy dispatch retain their original consumers; Object
# owns two key-array consumers and Reflect owns one.
wasm_current_function_realm_array_prototype=crates/scriptw-aot-wasm/src/functions/current_function_realm_array_prototype.rs
wasm_proxy_execution_realm=crates/scriptw-aot-wasm/src/functions/proxy_execution_realm.rs
require_file "$wasm_current_function_realm_array_prototype"
require_file "$wasm_proxy_execution_realm"
require_exact_line_count \
  "$wasm_functions" \
  'mod current_function_realm_array_prototype;' \
  1 \
  'private current_function_realm_array_prototype module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+current_function_realm_array_prototype;' "$wasm_functions"; then
  fail "$wasm_functions must keep current_function_realm_array_prototype private"
fi
require_fixed_string_count \
  "$wasm_functions" \
  'current_function_realm_array_prototype::' \
  0 \
  'current-function Realm Array prototype imports or re-exports'
# The current function Realm selects a rooted concrete Array prototype; the
# actual allocator consumes that witness before publishing the result header.
for current_array_entry in emit_load_current_function_realm_array_prototype emit_alloc_array_with_current_function_realm_prototype; do
  require_fixed_string_count "$wasm_current_function_realm_array_prototype" "fn ${current_array_entry}(" 1 'current function Realm Array owner'
  require_fixed_string_count "$wasm_functions" "fn ${current_array_entry}(" 0 'no parent current-Realm Array lifecycle copy'
done
# Called-Realm result semantics are exercised by the maintained finite Array,
# Object, Reflect and Intl Engine/CLI cohorts, rather than allocation call counts.

check_no_inline_legacy_includes "$wasm_current_function_realm_array_prototype"
check_raw_line_budget "$wasm_current_function_realm_array_prototype" 110

# GetFunctionRealm's rooted result lifecycle has one private owner. The parent
# exposes only the route selected by sibling consumers and privately imports
# the resolved witness used by its retained allocation paths.
wasm_function_realm=crates/scriptw-aot-wasm/src/functions/function_realm.rs
require_file "$wasm_function_realm"
require_exact_line_count \
  "$wasm_functions" \
  'mod function_realm;' \
  1 \
  'private function_realm module declaration'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+function_realm;' "$wasm_functions"; then
  fail "$wasm_functions must keep function_realm private"
fi
require_exact_line_count \
  "$wasm_functions" \
  'pub(crate) use function_realm::FunctionRealmRevokedRoute;' \
  1 \
  'FunctionRealmRevokedRoute re-export'
require_regex_count \
  "$wasm_functions" \
  '^pub(\([^)]*\))?[[:space:]]+use[[:space:]]+function_realm::' \
  1 \
  'function_realm public re-exports'
require_exact_line_count \
  "$wasm_functions" \
  'use function_realm::ResolvedFunctionRealmLocal;' \
  1 \
  'private ResolvedFunctionRealmLocal import'

for function_realm_type in \
  FunctionRealmOutcome \
  FunctionRealmResultLocals \
  ResolvedFunctionRealmLocal \
  FunctionRealmRevokedRoute
do
  require_tree_regex_count \
    crates/scriptw-aot-wasm/src \
    "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?(enum|struct)[[:space:]]+${function_realm_type}([[:space:](<{]|$)" \
    1 \
    "$function_realm_type backend owner"
  require_regex_count \
    "$wasm_functions" \
    "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?(enum|struct)[[:space:]]+${function_realm_type}([[:space:](<{]|$)" \
    0 \
    "$function_realm_type parent copies"
done

for function_realm_method in \
  emit_get_function_realm \
  emit_route_function_realm_result \
  release_resolved_function_realm_local
do
  require_regex_count \
    "$wasm_function_realm" \
    "^[[:space:]]*pub\\(crate\\)[[:space:]]+fn[[:space:]]+${function_realm_method}[[:space:]]*\\(" \
    1 \
    "$function_realm_method private-child owner"
  require_regex_count \
    "$wasm_functions" \
    "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+${function_realm_method}[[:space:]]*\\(" \
    0 \
    "$function_realm_method parent copies"
  require_tree_regex_count \
    crates/scriptw-aot-wasm/src \
    "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+${function_realm_method}[[:space:]]*\\(" \
    1 \
    "$function_realm_method backend owner"
done

# Each resolved Realm result stays rooted until its consuming operation clears it.

check_no_inline_legacy_includes "$wasm_function_realm"
check_raw_line_budget "$wasm_functions" 12363
check_raw_line_budget "$wasm_function_realm" 310

# The write-never static-generator IR cache had one synthetic backend protocol.
# Its names, producer and close-time marker consumer must not regrow separately.
require_tree_regex_count \
  crates/scriptw-ir/src \
  'SCRIPTW_STATIC_GENERATOR_' \
  0 \
  'retired static-generator IR names'
for retired_static_generator_backend_spelling in \
  'SCRIPTW_STATIC_GENERATOR_' \
  '[$]ScriptwStaticGenerator' \
  'StaticGeneratorValues' \
  'emit_exhaust_static_generator_iterator_if_marked'
do
  require_tree_regex_count \
    crates/scriptw-aot-wasm/src \
    "$retired_static_generator_backend_spelling" \
    0 \
    'retired static-generator backend protocol'
done

# Every catch/finally clause seeds its own statement-list completion after
# preserving the incoming completion. Ordinary try clauses also seed undefined
# for TryStatement's UpdateEmpty, since empty blocks preserve the list value.
statement_completion_owner="crates/scriptw-aot-wasm/src/control_flow/statement_completion.rs"
require_file "$statement_completion_owner"
require_regex_count "$statement_completion_owner" \
  '^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_statement_result\(' 1 'sole complete statement-result initialization owner'
require_fixed_string_count "$statement_completion_owner" 'self.completion().value().set_undefined(function);' 1 'canonical empty statement value'
require_fixed_string_count "$statement_completion_owner" 'self.set_completion_kind(CompletionKind::Normal, function);' 1 'normal empty statement completion'
try_clause_seed_pattern='^[[:space:]]*self\.emit_statement_result\(function\);[[:space:]]*$'
for try_clause_seed_owner in \
  'compile_try_catch emit_generator_state_in_range 2' \
  'compile_generator_try_catch compile_generator_try_finally 1' \
  'compile_generator_try_finally compile_generator_try_catch_finally 1' \
  'compile_generator_try_catch_finally compile_async_try_catch 2' \
  'compile_async_try_catch compile_async_try_catch_finally 1' \
  'compile_async_try_catch_finally compile_async_try_finally 2' \
  'compile_async_try_finally compile_try_finally 1' \
  'compile_try_finally compile_async_disposable_scope 2' \
  'compile_try_catch_finally compile_while 3'
do
  set -- $try_clause_seed_owner
  try_clause_seed_owner_source="$(sed -n "/fn ${1}(/,/fn ${2}(/p" "$wasm_control_flow")"
  require_text_regex_count \
    "$try_clause_seed_owner_source" \
    "$try_clause_seed_pattern" \
    "$3" \
    "$1 empty-completion clause-entry seeds"
done
