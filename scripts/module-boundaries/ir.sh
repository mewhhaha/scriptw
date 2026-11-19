for module in analysis builtins diagnostics early_errors ir lowering lowering_helpers names operations; do
  require_file "crates/scriptw-ir/src/${module}.rs"
  require_module_decl "$ir_lib" "$module"
done

require_pub_use "$ir_lib" '^pub use ir::\*;' 'IR data types'
require_pub_use "$ir_lib" '^pub use lowering::\{?lower' 'the lowering entry point'
require_pub_use "$ir_lib" '^pub use operations::' 'shared operation enums'
# Optional References, conditional facts, object-literal evaluation and staged
# compound References have
# actual private owners; the expression dispatcher retains their sole entries.
for lowering_owner_entry in \
  'optional_chain:lower_optional_property_chain' \
  'conditional_flow:capture_conditional_flow_facts' \
  'object_literal:lower_object_literal' \
  'generator_compound_assignment:lower_generator_compound_assignment'
do
  lowering_owner="${lowering_owner_entry%%:*}"
  lowering_entry="${lowering_owner_entry#*:}"
  lowering_owner_path="crates/scriptw-ir/src/lowering/${lowering_owner}.rs"
  require_file "$lowering_owner_path"
  require_exact_line_count "$ir_lowering" "mod ${lowering_owner};" 1 'private expression-family owner attachment'
  if grep -Eq "^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+${lowering_owner};" "$ir_lowering"; then
    fail "$ir_lowering must keep ${lowering_owner} private"
  fi
  require_fixed_string_count "$lowering_owner_path" "pub(super) fn ${lowering_entry}(" 1 'consumed expression-family entry'
  require_fixed_string_count "$ir_lowering" "fn ${lowering_entry}(" 0 'no parent expression-family implementation copy'
  check_no_inline_legacy_includes "$lowering_owner_path"
done
require_fixed_string_count "$ir_lowering" 'self.lower_generator_compound_assignment(' 1 'sole staged compound Reference dispatch consumer'
check_raw_line_budget "crates/scriptw-ir/src/lowering/generator_compound_assignment.rs" 120
# Suspended compound/logical assignment retains one native Identifier Reference.
# The allocator and branch consumer stay beside lowering; the backend restores
# that same reference through its private transport owner.
for reference_lowering_owner in generator_identifier_reference generator_logical_assignment; do
  reference_lowering_path="crates/scriptw-ir/src/lowering/${reference_lowering_owner}.rs"
  require_file "$reference_lowering_path"
  require_exact_line_count "$ir_lowering" "mod ${reference_lowering_owner};" 1 'private suspended Identifier Reference owner attachment'
  if grep -Eq "^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+${reference_lowering_owner};" "$ir_lowering"; then
    fail "$ir_lowering must keep ${reference_lowering_owner} private"
  fi
  check_no_inline_legacy_includes "$reference_lowering_path"
done
require_regex_count crates/scriptw-ir/src/lowering/generator_identifier_reference.rs '^pub\(super\) struct RetainedGeneratorIdentifierReference \{' 1 'private consumed Identifier Reference lifecycle'
ir_resumable_reference="crates/scriptw-ir/src/lowering/async_generator_assignment.rs"
require_exact_line_count "$ir_lowering" 'mod async_generator_assignment;' 1 'private shared assignment Reference owner attachment'
require_tree_regex_count crates/scriptw-ir/src/lowering '^[[:space:]]*pub\(super\) fn capture_resumable_assignment_reference\(' 1 'sole checked compound/logical Reference acquisition algorithm'
require_fixed_string_count "$ir_resumable_reference" 'RetainedGeneratorIdentifierReference::capture(' 1 'shared Identifier Get-before-RHS acquisition'
require_fixed_string_count crates/scriptw-ir/src/lowering/generator_compound_assignment.rs 'self.capture_resumable_assignment_reference(' 1 'compound consumes the shared actual Reference owner'
require_fixed_string_count crates/scriptw-ir/src/lowering/generator_logical_assignment.rs 'self.capture_resumable_assignment_reference(' 1 'logical consumes the shared actual Reference owner'
# The shared read/write target now owns plain assignment's write-only capture
# as well as compound/logical GetValue: 151 measured lines, one lifecycle.
check_raw_line_budget crates/scriptw-ir/src/lowering/generator_identifier_reference.rs 165
check_raw_line_budget crates/scriptw-ir/src/lowering/generator_logical_assignment.rs 120
# Complete eager operands, plain References, object properties and eager binding
# patterns have consumed private producers. The parent keeps their dispatch.
for staged_owner_entry in \
  'generator_plain_assignment:lower_staged_generator_identifier_assignment:35' \
  'generator_eager_value:lower_staged_generator_eager_value:190' \
  'generator_object_literal:lower_staged_generator_object_literal:135' \
  'generator_pattern_initializer:lower_generator_lexical_pattern_initializer:85' \
  'generator_pattern_assignment:lower_staged_generator_pattern_assignment:45' \
  'for_lexical_environment:lower_for_lexical_environment:80' \
  'operator_values:combine_unary_value:260'
do
  staged_owner="${staged_owner_entry%%:*}"
  staged_entry_budget="${staged_owner_entry#*:}"
  staged_entry="${staged_entry_budget%%:*}"
  staged_budget="${staged_entry_budget#*:}"
  staged_owner_path="crates/scriptw-ir/src/lowering/${staged_owner}.rs"
  require_file "$staged_owner_path"
  require_exact_line_count "$ir_lowering" "mod ${staged_owner};" 1 'private complete staged/semantic owner attachment'
  if grep -Eq "^pub(\([^)]*\))?[[:space:]]+mod[[:space:]]+${staged_owner};" "$ir_lowering"; then
    fail "$ir_lowering must keep ${staged_owner} private"
  fi
  require_fixed_string_count "$staged_owner_path" "pub(super) fn ${staged_entry}(" 1 'actual staged/semantic entry owner'
  require_fixed_string_count "$ir_lowering" "fn ${staged_entry}(" 0 'no parent staged/semantic body copy'
  check_no_inline_legacy_includes "$staged_owner_path"
  check_raw_line_budget "$staged_owner_path" "$staged_budget"
done
require_fixed_string_count "$ir_lowering" 'self.lower_staged_generator_identifier_assignment(' 2 'value and discarded plain Reference dispatch'
require_fixed_string_count crates/scriptw-ir/src/lowering/statement/expression.rs 'self.lower_staged_generator_identifier_assignment(' 1 'ordinary statement plain Reference consumer'
require_fixed_string_count crates/scriptw-ir/src/lowering/generator_plain_assignment.rs 'RetainedGeneratorIdentifierTarget::capture_write_only(' 1 'plain assignment uses the shared write-only target'
require_fixed_string_count "$ir_lowering" 'self.lower_staged_generator_eager_value(' 1 'complete eager operator dispatch'
require_fixed_string_count "$ir_lowering" 'self.lower_staged_generator_object_literal(' 1 'complete staged object dispatch'
require_fixed_string_count "$ir_lowering" 'self.lower_staged_generator_pattern_assignment(' 1 'complete staged pattern assignment dispatch'
require_fixed_string_count crates/scriptw-ir/src/lowering/lexical_declaration.rs 'self.lower_generator_lexical_pattern_initializer(' 1 'actual lexical pattern initializer consumer'
require_fixed_string_count crates/scriptw-ir/src/lowering/var_declaration.rs 'self.lower_generator_var_pattern_initializer(' 1 'actual var pattern initializer consumer'
require_fixed_string_count crates/scriptw-ir/src/lowering/generator_pattern_initializer.rs 'pub(super) fn lower_generator_var_pattern_initializer(' 1 'shared actual var pattern initializer owner'
for operator_entry in combine_unary_value combine_relational; do
  require_fixed_string_count crates/scriptw-ir/src/lowering/operator_values.rs "pub(super) fn ${operator_entry}(" 1 'shared evaluated-Value operator body'
  require_fixed_string_count "$ir_lowering" "fn ${operator_entry}(" 0 'no ordinary operator body in parent'
  require_fixed_string_count "$ir_lowering" "self.${operator_entry}(" 1 'ordinary evaluated-Value operator consumer'
  require_fixed_string_count crates/scriptw-ir/src/lowering/generator_eager_value.rs "self.${operator_entry}(" 1 'staged evaluated-Value operator consumer'
done
check_raw_line_budget crates/scriptw-ir/src/lowering/object_literal.rs 580

# Checked pattern source carriers and complete optional regions own their
# constructor/state obligations beside the source planner and actual consumer.
ir_generator_source="crates/scriptw-ir/src/generator_value_branch_source.rs"
for pattern_source_owner_type in \
  'pattern_initializer:GeneratorPatternInitializerSource:85' \
  'pattern_assignment:GeneratorPatternAssignmentSource:85'
do
  pattern_source_owner="${pattern_source_owner_type%%:*}"
  pattern_source_type_budget="${pattern_source_owner_type#*:}"
  pattern_source_type="${pattern_source_type_budget%%:*}"
  pattern_source_budget="${pattern_source_type_budget#*:}"
  pattern_source_path="crates/scriptw-ir/src/generator_value_branch_source/${pattern_source_owner}.rs"
  require_file "$pattern_source_path"
  require_exact_line_count "$ir_generator_source" "mod ${pattern_source_owner};" 1 'private checked pattern source owner attachment'
  require_exact_line_count "$ir_generator_source" "pub(crate) use ${pattern_source_owner}::${pattern_source_type};" 1 'actual checked pattern source projection'
  require_regex_count "$pattern_source_path" "^pub\(crate\) struct ${pattern_source_type}<'ast>" 1 'checked pattern source carrier'
  require_fixed_string_count "$ir_generator_source" "struct ${pattern_source_type}" 0 'no parent checked pattern carrier copy'
  check_no_inline_legacy_includes "$pattern_source_path"
  check_raw_line_budget "$pattern_source_path" "$pattern_source_budget"
done
# Object-owned suspension has protocol-specific source plans and one physical
# element/Reference algorithm. Opaque operations reach the same GetV/rest/Put.
ir_object_pattern_source="crates/scriptw-ir/src/generator_value_branch_source/object_pattern.rs"
ir_object_pattern_lowerer="crates/scriptw-ir/src/lowering/generator_object_pattern.rs"
ir_pattern_target_lowerer="crates/scriptw-ir/src/lowering/generator_pattern_target.rs"
ir_object_pattern_elements="crates/scriptw-ir/src/lowering/resumable_pattern/object.rs"
ir_pattern_reference_owner="crates/scriptw-ir/src/lowering/pattern_target.rs"
ir_object_pattern_operation="crates/scriptw-ir/src/object_destructuring_operation.rs"
wasm_object_pattern="crates/scriptw-aot-wasm/src/control_flow/object_destructuring.rs"
for object_pattern_owner in "$ir_object_pattern_source" "$ir_object_pattern_lowerer" "$ir_object_pattern_operation" "$wasm_object_pattern"; do
  require_file "$object_pattern_owner"
  check_no_inline_legacy_includes "$object_pattern_owner"
done
require_exact_line_count "$ir_generator_source" 'mod object_pattern;' 1 'private object-pattern source attachment'
require_exact_line_count "$ir_generator_source" 'pub(crate) use object_pattern::GeneratorObjectPatternSource;' 1 'consumed checked object-pattern source'
require_regex_count "$ir_object_pattern_source" "^pub\(crate\) struct GeneratorObjectPatternSource<'ast>" 1 'sole checked object-pattern source carrier'
require_fixed_string_count "$ir_generator_source" 'struct GeneratorObjectPatternSource' 0 'no parent object-pattern source copy'
require_exact_line_count "$ir_lowering" 'mod generator_object_pattern;' 1 'private staged object-pattern attachment'
for object_pattern_entry in lower_staged_generator_object_pattern lower_staged_generator_object_pattern_with_storage_names; do
  require_fixed_string_count "$ir_object_pattern_lowerer" "pub(super) fn ${object_pattern_entry}(" 1 'actual staged object-pattern entry'
  require_fixed_string_count "$ir_lowering" "fn ${object_pattern_entry}(" 0 'no parent object-pattern lowering copy'
done
require_fixed_string_count crates/scriptw-ir/src/lowering/generator_pattern_assignment.rs 'self.lower_staged_generator_object_pattern(' 1 'assignment consumes the selected object-pattern owner'
require_fixed_string_count crates/scriptw-ir/src/lowering/generator_pattern_initializer.rs 'self.lower_staged_generator_object_pattern(' 2 'lexical and var initialization consume the same object-pattern owner'
require_fixed_string_present "$ir_object_pattern_lowerer" 'self.lower_resumable_object_pattern_elements(' 'Generator adapter consumes the shared physical Object owner'
require_tree_regex_count crates/scriptw-ir/src/lowering '^[[:space:]]*pub\(in crate::lowering\) fn lower_resumable_object_pattern_elements\(' 1 'sole shared Object element algorithm'
require_fixed_string_count "$ir_object_pattern_elements" 'ObjectDestructuringSourceIr::prepare(' 1 'raw acquisition and actual ToObject source factory'
require_fixed_string_count "$ir_object_pattern_elements" 'ObjectDestructuringKeyIr::prepare(' 1 'actual normalized PropertyName producer'
require_fixed_string_count "$ir_object_pattern_elements" 'ObjectDestructuringOperationIr::get_v(' 1 'actual retained GetV operation'
require_fixed_string_count "$ir_object_pattern_elements" 'ObjectDestructuringOperationIr::rest(' 2 'binding and assignment rest operations'
require_fixed_string_count "$ir_pattern_reference_owner" 'ObjectDestructuringOperationIr::put_target(' 1 'actual captured target publication'
require_fixed_string_count "$ir_pattern_reference_owner" 'DestructuringPropertyKeyIr::Computed(raw)' 1 'member Reference preserves its raw key until PutValue'
require_fixed_string_present "$ir_pattern_reference_owner" 'StatementIr::DeclarationEvaluation(value)' 'lexical initialization retains Empty completion evidence'
require_exact_line_count crates/scriptw-ir/src/lib.rs 'mod object_destructuring_operation;' 1 'private opaque operation owner attachment'
require_exact_line_count "$ir_object_pattern_operation" 'mod tests;' 1 'actual private constructor controls'
for object_pattern_carrier in ObjectDestructuringSourceIr ObjectDestructuringKeyIr; do
  require_regex_count "$ir_object_pattern_operation" "^pub struct ${object_pattern_carrier} \\{" 1 'sole retained operand carrier'
  object_pattern_fields="$(sed -n "/^pub struct ${object_pattern_carrier} {$/,/^}$/p" "$ir_object_pattern_operation")"
  require_text_regex_count "$object_pattern_fields" '^[[:space:]]+pub(\([^)]*\))?[[:space:]]+' 0 'private constructor-owned operand fields'
done
require_exact_line_count "$ir_object_pattern_operation" 'pub struct ObjectDestructuringOperationIr(ObjectDestructuringOperation);' 1 'opaque closed operation payload'
require_exact_line_count "$ir_object_pattern_operation" 'enum ObjectDestructuringOperation {' 1 'private exhaustive operation domain'
require_fixed_string_count "$ir_object_pattern_operation" 'TypedExpr::spec_to_object(raw_read.clone())' 1 'real ToObject before pattern operands'
require_fixed_string_count "$ir_object_pattern_operation" 'TypedExpr::spec_to_property_key(raw_key)' 1 'real ToPropertyKey before GetV'
require_fixed_string_count "$ir_object_pattern_operation" 'row.name == binding.name || row.slot == binding.slot' 1 'exact allocated operand alias validation'
require_fixed_string_count "$ir_object_pattern_operation" 'require_distinct(&previous.binding, &key.binding)?;' 1 'rest excludes pairwise cell aliases'
require_fixed_string_count crates/scriptw-aot-wasm/src/operations.rs '    fn from_converted_value(value: ValueLocals) -> Self {' 1 'private normalized property-key mint'
require_regex_count crates/scriptw-aot-wasm/src/operations.rs '^[[:space:]]+pub(\([^)]*\))?[[:space:]]+fn from_converted_value' 0 'no public unchecked property-key mint'
require_fixed_string_count crates/scriptw-aot-wasm/src/operations.rs "pub(crate) fn compile_object_destructuring_operation_keys<'operation>(" 1 'typed operation key projection owner'
require_exact_line_count crates/scriptw-aot-wasm/src/control_flow.rs 'mod object_destructuring;' 1 'private complete native object-destructuring attachment'
require_fixed_string_count "$wasm_object_pattern" 'pub(crate) fn compile_object_destructuring_operation_to_value(' 1 'sole incremental native operation entry'
require_fixed_string_count "$wasm_object_pattern" 'self.compile_object_destructuring_operation_keys(operation, function)?' 1 'native consumer uses the admitted key projection'
require_fixed_string_count crates/scriptw-aot-wasm/src/expressions.rs 'self.compile_object_destructuring_operation_to_value(operation, output, function)?' 1 'actual exhaustive expression dispatch'
require_fixed_string_count crates/scriptw-aot-wasm/src/control_flow.rs 'operation.visit_bindings(&mut |mode, name| {' 1 'direct lexical instantiation sees opaque pattern bindings'
ir_resumable_for_initializer="crates/scriptw-ir/src/lowering/resumable_for_initializer.rs"
require_file "$ir_resumable_for_initializer"
require_exact_line_count "$ir_lowering" 'mod resumable_for_initializer;' 1 'private actual common For-head initialization owner'
require_tree_regex_count crates/scriptw-ir/src/lowering '^[[:space:]]*pub\(super\) fn lower_resumable_generator_for_init\(' 1 'sole common For initializer algorithm'
require_fixed_string_count "$ir_resumable_for_initializer" 'lower_staged_generator_object_pattern_with_storage_names(' 1 'common For consumes the same staged Object pattern owner'
object_pattern_for_head="$(sed -n '/let storage_names =/,/if self.async_generator_entry_state()/p' "$ir_resumable_for_initializer")"
require_text_regex_count "$object_pattern_for_head" 'scoped_lexical_binding_storage_name\(' 1 'For patterns keep the analyzed scoped storage map'
for for_initializer_consumer in generator_loop async_classic_loop async_generator_loop; do
  require_fixed_string_count "crates/scriptw-ir/src/lowering/${for_initializer_consumer}.rs" 'self.lower_resumable_generator_for_init(source.init())' 1 'each checked protocol consumes the same actual For initializer'
done
pattern_assignment_yield_count="$(sed -n '/let source = GeneratorPatternAssignmentSource::new(/,/staged_generator_expression_yield_count(source.rhs())/p' crates/scriptw-ir/src/lowering_helpers.rs)"
require_text_regex_count "$pattern_assignment_yield_count" 'staged_generator_expression_yield_count\(source.rhs\(\)\)' 1 'retained pattern-assignment RHS getter has its actual eager count consumer'
check_raw_line_budget "$ir_object_pattern_source" 165
check_raw_line_budget "$ir_object_pattern_lowerer" 210
check_raw_line_budget "$ir_object_pattern_operation" 330
check_raw_line_budget "$wasm_object_pattern" 210
require_file vendor/boa_ast-0.21.1/src/expression/literal/object.rs
require_exact_line_count Cargo.toml 'boa_ast = { path = "vendor/boa_ast-0.21.1" }' 1 'workspace selects the actual corrected cover converter'
require_exact_line_count crates/scriptw-front/Cargo.toml 'boa_ast = { version = "0.21.1", features = ["annex-b"] }' 1 'frontend consumes the pinned patched AST'
require_fixed_string_count vendor/boa_ast-0.21.1/src/expression/literal/object.rs 'pub fn to_pattern(&self, strict: bool) -> Option<ObjectPattern>' 1 'actual vendored cover-conversion owner'
for object_pattern_control in \
  crates/scriptw-front/tests/object_assignment_cover_patterns.rs \
  crates/scriptw-ir/tests/generator_object_patterns.rs \
  crates/scriptw-ir/src/object_destructuring_operation/tests.rs \
  crates/scriptw-ir/tests/generator_pattern_assignments.rs \
  crates/scriptw-ir/tests/generator_pattern_initializers.rs \
  crates/scriptw-ir/tests/generator_throw_regions.rs \
  crates/scriptw-engine/tests/aot_generators/aot_generator_object_patterns.rs \
  crates/scriptw-engine/tests/fixtures/generator_object_patterns/object_patterns.js \
  crates/scriptw-engine/tests/fixtures/generator_object_patterns/with_references.js
do
  require_file "$object_pattern_control"
done
require_fixed_string_count crates/scriptw-front/tests/object_assignment_cover_patterns.rs 'fn computed_single_name_defaults_keep_inferred_and_explicit_definition_names()' 1 'actual cover conversion and NamedEvaluation control'
require_fixed_string_count crates/scriptw-ir/tests/generator_object_patterns.rs 'fn object_pattern_acquisition_reference_and_lazy_defaults_have_exact_source_order()' 1 'actual source ordering control'
require_fixed_string_count crates/scriptw-ir/tests/generator_object_patterns.rs 'fn object_pattern_member_put_keeps_raw_keys_and_lexical_for_cells_are_original()' 1 'raw member Reference and analyzed For cells control'
require_fixed_string_count crates/scriptw-ir/src/object_destructuring_operation/tests.rs 'fn key_factory_normalizes_once_and_rest_rejects_cell_aliases_without_rejecting_equal_keys()' 1 'meaningful private key/alias constructor control'

# Array-owned suspension retains a genuine native IteratorRecord in one exact
# original-invocation cell. Source kinds/states and whole suspension ranges are
# consumed by the opaque complete close owner, never by an expression opcode.
ir_array_pattern_source="crates/scriptw-ir/src/generator_value_branch_source/array_pattern.rs"
ir_array_pattern_lowerer="crates/scriptw-ir/src/lowering/generator_array_pattern.rs"
ir_array_pattern_operation="crates/scriptw-ir/src/array_destructuring_operation.rs"
ir_array_pattern_whole="crates/scriptw-ir/src/generator_array_destructuring.rs"
ir_async_array_pattern_whole="crates/scriptw-ir/src/async_array_destructuring.rs"
ir_mixed_array_pattern_whole="crates/scriptw-ir/src/async_generator_array_destructuring.rs"
ir_shared_pattern_target="crates/scriptw-ir/src/lowering/pattern_target.rs"
ir_shared_pattern_elements="crates/scriptw-ir/src/lowering/resumable_pattern.rs"
ir_shared_array_elements="crates/scriptw-ir/src/lowering/resumable_pattern/array.rs"
ir_shared_object_elements="crates/scriptw-ir/src/lowering/resumable_pattern/object.rs"
wasm_array_pattern="crates/scriptw-aot-wasm/src/control_flow/array_destructuring.rs"
wasm_resumable_array_pattern="crates/scriptw-aot-wasm/src/control_flow/resumable_array_destructuring.rs"
wasm_retained_array_iterator="crates/scriptw-aot-wasm/src/environments/retained_array_iterator.rs"
for array_pattern_owner in \
  "$ir_array_pattern_source" "$ir_array_pattern_lowerer" "$ir_pattern_target_lowerer" \
  "$ir_array_pattern_operation" "$ir_array_pattern_whole" \
  "$ir_async_array_pattern_whole" "$ir_mixed_array_pattern_whole" \
  "$ir_shared_pattern_target" "$ir_shared_pattern_elements" \
  "$ir_shared_array_elements" "$ir_shared_object_elements" \
  "$wasm_array_pattern" "$wasm_resumable_array_pattern" "$wasm_retained_array_iterator"
do
  require_file "$array_pattern_owner"
  check_no_inline_legacy_includes "$array_pattern_owner"
done
require_exact_line_count "$ir_generator_source" 'mod array_pattern;' 1 'private complete Array source owner'
require_exact_line_count "$ir_generator_source" 'pub(crate) use array_pattern::{GeneratorArrayPatternSource, GeneratorArrayPatternSourceStates};' 1 'actual source and operation tape projection'
require_regex_count "$ir_array_pattern_source" "^pub\(crate\) struct GeneratorArrayPatternSource<'ast>" 1 'sole actual ArrayPattern source carrier'
require_exact_line_count "$ir_array_pattern_source" '    pub(crate) fn operations(&self) -> &[(ArrayDestructuringOperationKindIr, u32)] {' 1 'actual ordered operation kind/state evidence'
require_fixed_string_count "$ir_generator_source" 'struct GeneratorArrayPatternSource' 0 'no parent Array source carrier copy'
for pattern_lowerer in generator_array_pattern generator_pattern_target pattern_target resumable_pattern; do
  require_exact_line_count "$ir_lowering" "mod ${pattern_lowerer};" 1 'private actual Array/shared target lowerer attachment'
  if grep -Eq "^pub(\([^)]*\))?[[:space:]]+mod[[:space:]]+${pattern_lowerer};" "$ir_lowering"; then
    fail "$ir_lowering must keep ${pattern_lowerer} private"
  fi
done
for array_pattern_entry in lower_staged_generator_array_pattern lower_staged_generator_array_pattern_with_storage_names; do
  require_fixed_string_count "$ir_array_pattern_lowerer" "pub(super) fn ${array_pattern_entry}(" 1 'actual staged Array entry'
  require_fixed_string_count "$ir_lowering" "fn ${array_pattern_entry}(" 0 'no parent Array lowering body'
done
require_fixed_string_present crates/scriptw-ir/src/lowering/generator_pattern_assignment.rs 'lower_staged_generator_array_pattern(' 'actual Array assignment producer'
require_fixed_string_present crates/scriptw-ir/src/lowering/generator_pattern_initializer.rs 'lower_staged_generator_array_pattern(' 'actual lexical and var Array producers'
require_fixed_string_count "$ir_resumable_for_initializer" 'lower_staged_generator_array_pattern_with_storage_names(' 1 'common For original scoped Array bindings'
for array_adapter in \
  'generator_array_pattern.rs:Generator:OrdinaryGeneratorArrayDestructuringIr' \
  'async_pattern/array.rs:Async:AsyncFunctionArrayDestructuringIr' \
  'async_generator_pattern.rs:AsyncGenerator:AsyncGeneratorArrayDestructuringIr'
do
  array_adapter_path="${array_adapter%%:*}"
  array_adapter_tail="${array_adapter#*:}"
  array_adapter_protocol="${array_adapter_tail%%:*}"
  array_adapter_carrier="${array_adapter_tail#*:}"
  array_adapter_path="crates/scriptw-ir/src/lowering/$array_adapter_path"
  require_fixed_string_present "$array_adapter_path" 'ArrayIteratorStorageIr::new(' 'checked original iterator storage'
  require_fixed_string_present "$array_adapter_path" "${array_adapter_carrier}::new(" 'actual protocol-specific complete carrier'
  require_fixed_string_present "$array_adapter_path" "PatternContinuation::${array_adapter_protocol}" 'closed source execution protocol'
  require_fixed_string_present "$array_adapter_path" 'self.lower_resumable_array_pattern_elements(' 'one physical Array element algorithm'
done
require_tree_regex_count crates/scriptw-ir/src/lowering '^[[:space:]]*pub\(in crate::lowering\) fn lower_resumable_array_pattern_elements\(' 1 'sole shared Array element algorithm'
require_exact_line_count "$ir_shared_pattern_elements" 'mod array;' 1 'private physical Array element attachment'
require_exact_line_count "$ir_shared_pattern_elements" 'mod object;' 1 'private physical Object element attachment'
for array_operation in step_value elision rest_array; do
  require_fixed_string_count "$ir_shared_array_elements" "ArrayDestructuringOperationIr::${array_operation}(" 1 'shared source-owned native Array protocol operation'
done
# Protocol adapters select defaults; Reference acquisition and Put have one
# physical body shared by both pattern shapes and all three protocols.
for pattern_target_method in retain_pattern_identifier retain_pattern_member put_pattern_target; do
  require_tree_regex_count crates/scriptw-ir/src/lowering "^[[:space:]]*pub\\(super\\) fn ${pattern_target_method}\\(" 1 'sole actual pattern Reference/Put owner'
  for pattern_consumer in "$ir_shared_object_elements" "$ir_shared_array_elements"; do
    require_fixed_string_present "$pattern_consumer" "self.${pattern_target_method}(" 'shared pattern Reference/Put algorithm'
  done
done
require_fixed_string_count "$ir_pattern_target_lowerer" '    fn lower_generator_pattern_default_region(' 1 'single lazy complete default region compiler'
for pattern_retention in owned_pattern_binding retain_pattern_value; do
  require_tree_regex_count crates/scriptw-ir/src/lowering "^pub\\(super\\) fn ${pattern_retention}\\(" 1 'sole whole Value activation retention owner'
done
require_fixed_string_count "$ir_shared_pattern_target" 'RetainedGeneratorIdentifierTarget::capture_write_only(' 1 'pattern Identifier retains original Reference before default'
require_fixed_string_present "$ir_shared_pattern_target" 'self.lower_pattern_expression(execution, access.target())?' 'Member/private original base evaluation'
require_fixed_string_count "$ir_shared_pattern_target" 'DestructuringPropertyKeyIr::Computed(raw)' 1 'Reference key remains raw until PutValue'
require_fixed_string_present "$ir_shared_pattern_target" 'StatementIr::DeclarationEvaluation(value)' 'lexical initialization Empty completion'
require_fixed_string_count "$ir_pattern_target_lowerer" 'OrdinaryGeneratorIfIr::new(' 1 'lazy defaults use complete actual branch regions'
require_fixed_string_present "$ir_shared_pattern_target" 'lower_staged_generator_object_pattern_with_storage_names(' 'nested Object actual checked owner'
require_fixed_string_present "$ir_shared_pattern_target" 'lower_staged_generator_array_pattern_with_storage_names(' 'nested Array distinct original record'
for array_carrier in array_destructuring_operation generator_array_destructuring; do
  require_exact_line_count crates/scriptw-ir/src/lib.rs "mod ${array_carrier};" 1 'private native Array proof attachment'
  require_exact_line_count "crates/scriptw-ir/src/${array_carrier}.rs" 'mod tests;' 1 'actual private damaged-allocation/source controls'
done
require_regex_count "$ir_array_pattern_operation" '^pub struct ArrayIteratorStorageIr \{' 1 'opaque real activation storage carrier'
require_exact_line_count "$ir_array_pattern_operation" 'pub struct ArrayDestructuringOperationIr(ArrayDestructuringOperation);' 1 'opaque closed Array operation'
require_exact_line_count "$ir_array_pattern_operation" 'enum ArrayDestructuringOperation {' 1 'private exhaustive Array operation payload'
require_exact_line_count "$ir_array_pattern_operation" 'pub enum ArrayDestructuringOperationKindIr {' 1 'closed source/native operation kind tape'
require_regex_count "$ir_array_pattern_whole" '^pub struct OrdinaryGeneratorArrayDestructuringIr \{' 1 'opaque complete acquisition/body/close owner'
for array_proof_owner in "$ir_array_pattern_operation" "$ir_array_pattern_whole" "$ir_async_array_pattern_whole" "$ir_mixed_array_pattern_whole"; do
  require_regex_count "$array_proof_owner" '^[[:space:]]+pub[[:space:]]+(binding|storage|raw_source|body|result)[[:space:]]*:' 0 'private constructor-owned Array fields'
  require_regex_count "$array_proof_owner" '^[[:space:]]+pub[[:space:]]+fn[[:space:]]+(new|step_value|elision|rest_array)[[:space:]]*[(]' 0 'no unchecked public native Array constructor'
done
require_fixed_string_count "$ir_array_pattern_operation" 'row.name == binding.name || row.slot == binding.slot' 1 'exact original storage allocation name/slot alias proof'
require_fixed_string_count "$ir_async_array_pattern_whole" 'row.name == binding.name || row.slot == binding.slot' 1 'shared complete pattern-cell allocation proof'
require_fixed_string_present "$ir_mixed_array_pattern_whole" 'PatternCells::new(inventory)' 'shared global pattern-cell alias census'
require_fixed_string_count "$ir_array_pattern_operation" 'StatementIr::ArrayDestructuringOperation(' 2 'operation factory emits only dedicated statements'
require_fixed_string_count "$ir_array_pattern_whole" 'if operations != states.operations()' 1 'complete native body consumes actual operation order/states'
require_fixed_string_count "$ir_array_pattern_whole" 'if points != states.suspensions()' 1 'whole source suspension tape is consumed exactly'
require_fixed_string_count "$ir_array_pattern_whole" 'state = sequence_end(' 1 'native operation positions use actual complete region progression'
require_fixed_string_count "$ir_array_pattern_whole" 'require_allocated(result, inventory)?;' 1 'complete body revalidates actual result cells'
require_fixed_string_count "$ir_array_pattern_whole" 'require_allocated(&plan.raw_binding, inventory)?;' 1 'nested raw source is in the same allocation inventory'
for array_proof_owner in "$ir_async_array_pattern_whole" "$ir_mixed_array_pattern_whole"; do
  require_fixed_string_present "$array_proof_owner" 'operations != states.operations()' 'actual ordered source operation tape'
  require_fixed_string_present "$array_proof_owner" 'operation.storage() != storage' 'foreign iterator-operation refusal'
done
require_fixed_string_count "$ir_async_array_pattern_whole" 'awaits != states.awaits()' 1 'plain Async exact source Await tape'
require_fixed_string_present "$ir_mixed_array_pattern_whole" 'validate_tape(states.suspensions(), &suspensions)' 'mixed exact source Await/Yield tape'
require_fixed_string_present "$ir_mixed_array_pattern_whole" 'AsyncGeneratorLoopRegionIr::from_array_pattern(CheckedArrayBody {' 'body region minted only by actual Array proof'
require_fixed_string_count crates/scriptw-ir/src/ir.rs 'ArrayDestructuringOperation(Box<crate::ArrayDestructuringOperationIr>),' 1 'sole dedicated Statement operation variant'
array_operation_expression_domain="$(braced_rust_item_source crates/scriptw-ir/src/ir.rs '^pub[[:space:]]+enum[[:space:]]+ExprIr[[:space:]]*[{]')"
require_text_regex_count "$array_operation_expression_domain" 'ArrayDestructuringOperation' 0 'native iterator operations cannot inhabit expressions'
for array_native_child in array_destructuring resumable_array_destructuring; do
  require_exact_line_count crates/scriptw-aot-wasm/src/control_flow.rs "mod ${array_native_child};" 1 'private actual native Array semantic child'
  if grep -Eq "^pub(\([^)]*\))?[[:space:]]+mod[[:space:]]+${array_native_child};" crates/scriptw-aot-wasm/src/control_flow.rs; then
    fail 'native Array semantic children must remain private'
  fi
done
require_fixed_string_count "$wasm_array_pattern" 'pub(super) fn compile_array_destructuring_operation_statement(' 1 'sole native statement-only operation entry'
for array_native_entry in compile_ordinary_generator_array_destructuring compile_async_function_array_destructuring compile_async_generator_array_destructuring; do
  require_fixed_string_count "$wasm_resumable_array_pattern" "pub(super) fn ${array_native_entry}(" 1 'actual checked protocol adapter'
  require_fixed_string_count crates/scriptw-aot-wasm/src/control_flow.rs "self.${array_native_entry}(plan, function)?;" 1 'actual closed native carrier dispatch'
done
require_tree_regex_count crates/scriptw-aot-wasm/src '^[[:space:]]*fn compile_resumable_array_destructuring\(' 1 'sole physical retained acquisition/body/close pipeline'
require_fixed_string_present "$wasm_resumable_array_pattern" 'meta.protocol().execution_kind() == plan.execution_kind()' 'exact native carrier execution admission'
require_fixed_string_present "$wasm_resumable_array_pattern" 'CheckedAsyncGeneratorEnvironmentOwner::for_array_destructuring(plan)' 'mixed environment token minted from actual opaque carrier'
for moved_array_entry in compile_array_destructure_to_locals compile_array_destructure_from_value_locals; do
  require_fixed_string_count "$wasm_array_pattern" "pub(crate) fn ${moved_array_entry}(" 1 'same ordinary complete Array compiler in actual child'
  require_fixed_string_count crates/scriptw-aot-wasm/src/control_flow.rs "fn ${moved_array_entry}(" 0 'no complete Array compiler copy in parent'
done
require_fixed_string_count crates/scriptw-aot-wasm/src/control_flow.rs 'self.compile_array_destructuring_operation_statement(operation, function)?;' 1 'actual closed native operation dispatch'
require_fixed_string_count "$wasm_array_pattern" 'self.emit_array_destructuring_rest_array(' 2 'ordinary and retained rest share one physical builder'
require_fixed_string_count "$wasm_array_pattern" '    fn emit_array_destructuring_rest_array(' 1 'sole real Array rest drain body'
require_exact_line_count crates/scriptw-aot-wasm/src/environments.rs 'mod retained_array_iterator;' 1 'private real activation edge owner'
require_fixed_string_count "$wasm_retained_array_iterator" 'pub(crate) fn array_iterator_storage(' 1 'native storage validates actual ordinary invocation owner'
require_fixed_string_count "$wasm_retained_array_iterator" 'InvocationFrameSchema::INVOCATION_ENVIRONMENT' 1 'record edge stays in original invocation cell'
require_fixed_string_count "$wasm_retained_array_iterator" 'self.owned_env_slot(&binding.name) != Some(binding.slot)' 1 'exact native owned storage slot proof'
require_fixed_string_present "$wasm_retained_array_iterator" 'owner.is_region()' 'mixed admission requires checked environment owner'
for iterator_edge_entry in emit_publish_retained_array_iterator emit_load_retained_array_iterator emit_retire_retained_array_iterator; do
  require_fixed_string_count "$wasm_retained_array_iterator" "pub(crate) fn ${iterator_edge_entry}(" 1 'actual native record edge lifecycle owner'
done
require_fixed_string_count "$wasm_resumable_array_pattern" 'self.emit_publish_retained_array_iterator(&storage, acquired.record(), function);' 1 'acquisition retains same genuine native record'
require_fixed_string_count "$wasm_resumable_array_pattern" 'self.emit_retire_retained_array_iterator(&storage, function);' 1 'whole close alone retires record edge'
require_fixed_string_count "$wasm_resumable_array_pattern" 'self.emit_sync_iterator_close(&iterator, &pending, &closed, function)?;' 1 'shared original whole-completion close algorithm'
require_fixed_string_count "$wasm_array_pattern" 'self.write_generator_statement_list_binding(binding, &value, function);' 1 'protocol result writes the original invocation cell'
require_fixed_string_count crates/scriptw-aot-wasm/src/gc_types/layouts.rs 'DESTRUCTURING_ITERATOR_RECORD: GcRef<IteratorRecord>, Mutable, Nullable;' 1 'single actual nullable GC record edge'
check_raw_line_budget "$ir_array_pattern_source" 210
check_raw_line_budget "$ir_array_pattern_lowerer" 225
check_raw_line_budget "$ir_pattern_target_lowerer" 330
check_raw_line_budget "$ir_shared_pattern_target" 330
check_raw_line_budget "$ir_array_pattern_operation" 205
check_raw_line_budget "$ir_array_pattern_whole" 310
check_raw_line_budget "$wasm_array_pattern" 200
# The three small adapters are checked above against one physical pipeline;
# the old Generator-only raw-line budget cannot describe that shared owner.
check_raw_line_budget "$wasm_retained_array_iterator" 125
for array_pattern_control in \
  crates/scriptw-ir/src/array_destructuring_operation/tests.rs \
  crates/scriptw-ir/src/generator_array_destructuring/tests.rs \
  crates/scriptw-ir/tests/generator_array_patterns.rs \
  crates/scriptw-engine/tests/aot_generators/aot_generator_array_patterns.rs \
  crates/scriptw-engine/tests/fixtures/generator_array_patterns/array_patterns.js \
  crates/scriptw-engine/tests/fixtures/generator_array_patterns/with_references.js \
  crates/scriptw-aot-wasm/tests/structure_async/generator_array_destructuring_structure.rs \
  crates/scriptw-aot-wasm/tests/structure_language/destructuring_iterator_step_kind_structure.rs \
  crates/scriptw-aot-wasm/tests/structure_language/destructuring_iterator_locals_ownership_structure.rs \
  crates/scriptw-aot-wasm/tests/structure_async/sync_iterator_consumer_capability_structure.rs
do
  require_file "$array_pattern_control"
done
require_fixed_string_count crates/scriptw-ir/src/generator_array_destructuring/tests.rs 'fn complete_array_constructor_rejects_removed_extra_and_substituted_protocol_operations()' 1 'actual complete protocol tape damage control'
require_fixed_string_count crates/scriptw-ir/tests/generator_array_patterns.rs 'fn array_pattern_owns_acquisition_complete_body_and_lazy_default_states()' 1 'real source complete-close/range control'
require_fixed_string_count crates/scriptw-engine/tests/aot_generators/aot_generator_array_patterns.rs 'fn generator_array_patterns_preserve_iterator_records_close_and_original_targets()' 1 'actual strict/sloppy Wasm protocol/completion cohort'

# Parser name-scope provenance separates a source BindingIdentifier from an
# inferred display label. Analysis, lowering and fact publication share it.
ir_source_class_name="crates/scriptw-ir/src/class_name_source.rs"
require_file "$ir_source_class_name"
require_exact_line_count crates/scriptw-ir/src/lib.rs 'mod class_name_source;' 1 'private class-name source authority'
require_exact_line_count crates/scriptw-ir/src/lib.rs 'pub(crate) use class_name_source::SourceClassName;' 1 'consumed source class-name proof'
require_exact_line_count "$ir_source_class_name" 'pub(crate) struct SourceClassName(ClassNameSource);' 1 'opaque source binding versus label owner'
require_exact_line_count "$ir_source_class_name" 'enum ClassNameSource {' 1 'closed private class-name provenance'
require_fixed_string_count "$ir_source_class_name" 'class.name_scope().has_binding(' 1 'declaration consumes the actual parser name scope'
require_fixed_string_count "$ir_source_class_name" 'class.name_scope().is_some()' 1 'expression consumes source BindingIdentifier provenance'
for class_name_consumer in crates/scriptw-ir/src/analysis.rs "$ir_lowering"; do
  require_fixed_string_count "$class_name_consumer" 'SourceClassName::from_declaration(' 1 'source declaration name proof consumer'
  require_fixed_string_count "$class_name_consumer" 'SourceClassName::from_expression(' 1 'source expression name proof consumer'
done
require_fixed_string_count crates/scriptw-ir/src/analysis.rs 'class_name.binding_name().map(' 1 'only real class names allocate analysis environments'
require_fixed_string_count "$ir_lowering" 'class_name.binding_name().map(' 1 'only real class names allocate lowering environments'
require_fixed_string_count "$ir_lowering" 'class_name.into_label(),' 1 'metadata consumes the separate display label'
require_fixed_string_count crates/scriptw-ir/src/lowering/class_definition.rs 'if let (Some(class_name), Some(_)) = (&class_name, &name_binding)' 2 'class fact updates require an actual name binding'
check_no_inline_legacy_includes "$ir_source_class_name"
check_raw_line_budget "$ir_source_class_name" 85
require_file crates/scriptw-ir/tests/class_name_source.rs
require_file crates/scriptw-engine/tests/aot_language/aot_class_name_source.rs
require_file crates/scriptw-engine/tests/fixtures/class_name_source/names.js
require_fixed_string_count crates/scriptw-ir/tests/class_name_source.rs 'fn inferred_class_labels_capture_outer_cells_and_explicit_names_capture_inner_cells()' 1 'actual inferred-label versus source-binding capture control'
require_fixed_string_count crates/scriptw-ir/tests/class_name_source.rs 'fn yielded_heritage_retains_only_actual_source_class_name_environments()' 1 'actual suspended class name environment control'
require_fixed_string_count crates/scriptw-ir/tests/class_name_source.rs 'fn anonymous_default_export_preserves_its_display_label_without_a_class_name_cell()' 1 'metadata-only class label control'
require_fixed_string_count crates/scriptw-engine/tests/aot_language/aot_class_name_source.rs 'fn class_labels_preserve_outer_tdz_cells_and_explicit_inner_names_through_wasm()' 1 'actual defining class/outer TDZ Wasm cohort'
require_file crates/scriptw-ir/tests/class_computed_name_suspension.rs
require_file crates/scriptw-engine/tests/aot_generators/aot_class_computed_name_suspension.rs

ir_optional_source="crates/scriptw-ir/src/generator_value_branch_source/optional_chain.rs"
ir_optional_consumer="crates/scriptw-ir/src/lowering/generator_value_branch/optional_chain.rs"
require_file "$ir_optional_source"
require_file "$ir_optional_consumer"
require_exact_line_count "$ir_generator_source" 'mod optional_chain;' 1 'private complete optional source attachment'
require_exact_line_count crates/scriptw-ir/src/lowering/generator_value_branch.rs 'mod optional_chain;' 1 'private complete optional consumer attachment'
require_regex_count "$ir_optional_source" "^pub\(crate\) struct GeneratorOptionalChainSource<'ast>" 1 'actual complete optional source carrier'
require_fixed_string_count "$ir_generator_source" 'struct GeneratorOptionalChainSource' 0 'no parent optional source carrier copy'
for optional_region_entry in lower_generator_optional_chain lower_generator_grouped_optional_reference lower_generator_optional_delete; do
  require_fixed_string_count "$ir_optional_consumer" "pub(in crate::lowering) fn ${optional_region_entry}(" 1 'actual checked optional region entry'
  require_fixed_string_count crates/scriptw-ir/src/lowering/generator_value_branch.rs "fn ${optional_region_entry}(" 0 'no parent complete optional compiler copy'
done
require_fixed_string_count "$ir_optional_consumer" 'fn lower_generator_optional_chain_to(' 1 'one shared optional region destination body'
require_fixed_string_count "$ir_lowering" 'return self.lower_generator_optional_chain(source);' 1 'actual complete optional value dispatch'
ir_eager_reference_operands="crates/scriptw-ir/src/lowering/generator_eager_value/reference_operands.rs"
require_file "$ir_eager_reference_operands"
require_exact_line_count crates/scriptw-ir/src/lowering/generator_eager_value.rs 'mod reference_operands;' 1 'private shared eager Reference operand attachment'
require_fixed_string_count "$ir_eager_reference_operands" 'self.lower_generator_optional_delete(source)' 1 'actual complete optional Delete dispatch in the private Reference operand owner'
check_no_inline_legacy_includes "$ir_eager_reference_operands"
# Update/Delete/Super References keep original operand order under the shared
# protocol adapter; arithmetic/Calls remain in the bounded eager parent.
check_raw_line_budget "$ir_eager_reference_operands" 270
check_no_inline_legacy_includes "$ir_optional_source"
check_no_inline_legacy_includes "$ir_optional_consumer"
# These bounded owners now stage ordinary, async and mixed optional operands,
# including private references, grouped Calls and Delete without a second algorithm.
check_raw_line_budget "$ir_optional_source" 500
check_raw_line_budget "$ir_optional_consumer" 650
# Signature propagation and declaration bodies are genuine private owners. The
# parent retains its exhaustive declaration dispatcher and shared facts. Var
# and lexical owners now initialize mapped pattern cells across all protocols
# and preserve declaration Empty completion.
for declaration_owner_entry in \
  'signature_evidence:propagate_function_signatures:1050' \
  'var_declaration:lower_var_declarator:570' \
  'function_declaration:lower_function_declaration:225' \
  'lexical_declaration:lower_lexical_declaration:460'
do
  declaration_owner="${declaration_owner_entry%%:*}"
  declaration_entry_budget="${declaration_owner_entry#*:}"
  declaration_entry="${declaration_entry_budget%%:*}"
  declaration_budget="${declaration_entry_budget#*:}"
  declaration_owner_path="crates/scriptw-ir/src/lowering/${declaration_owner}.rs"
  require_file "$declaration_owner_path"
  require_exact_line_count "$ir_lowering" "mod ${declaration_owner};" 1 'private signature/declaration owner attachment'
  if grep -Eq "^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+${declaration_owner};" "$ir_lowering"; then
    fail "$ir_lowering must keep ${declaration_owner} private"
  fi
  require_fixed_string_count "$declaration_owner_path" "pub(super) fn ${declaration_entry}(" 1 'consumed signature/declaration entry'
  require_fixed_string_count "$ir_lowering" "fn ${declaration_entry}(" 0 'no parent signature/declaration body copy'
  check_no_inline_legacy_includes "$declaration_owner_path"
  check_raw_line_budget "$declaration_owner_path" "$declaration_budget"
done
require_fixed_string_count "$ir_lowering" 'self.propagate_function_signatures();' 1 'actual signature propagation consumer'
# T12's module subsystem. `modules/` is a directory module, so the flat-file
# loop above cannot cover it: declaring `mod modules;` without the directory,
# or adding a submodule without registering it, is exactly the failure this
# catches.
ir_modules_mod="crates/scriptw-ir/src/modules/mod.rs"
require_file "$ir_modules_mod"
require_module_decl "$ir_lib" "modules"
for module in callable_source dynamic early graph link namespace record source; do
  require_file "crates/scriptw-ir/src/modules/${module}.rs"
  require_module_decl "$ir_modules_mod" "$module"
done
check_no_inline_legacy_includes "$ir_modules_mod"
check_raw_line_budget "crates/scriptw-ir/src/modules/callable_source.rs" 340
require_file "crates/scriptw-ir/src/lowering/function_environment.rs"
require_module_decl "$ir_lowering" "function_environment"
check_raw_line_budget "crates/scriptw-ir/src/lowering/function_environment.rs" 140
require_pub_use "$ir_lib" '^pub use modules::\{' 'the module-record surface'

# 160 against a CODE-ONLY count, measured 140 at batch 6.
#
# 140 was the budget for the RAW line count, and after `non_test_lines` started
# excluding blanks and comments this file sat at exactly 140 of 140 — zero
# headroom, so the next `mod`/`use`/`pub use` line any lane adds to this crate
# root reddens a SHARED script for a reason unrelated to that lane. 160 is 20
# lines of headroom over the measurement and still far below the 169 raw lines
# the old number rejected. Re-tighten it the next time this crate root actually
# shrinks; do not raise it again without saying here what it was measured at.
check_orchestration_surface "$ir_lib" 160
check_no_inline_legacy_includes "$ir_lib"

# T02's pure builtin-shape boundary. Keeping these 98 metadata constructors in
# a child module leaves lowering.rs responsible for orchestration and semantic
# lowering rather than making it the mandatory edit point for every builtin.
ir_builtin_shapes="crates/scriptw-ir/src/lowering/builtin_shapes.rs"
require_file "$ir_builtin_shapes"
require_module_decl "$ir_lowering" "builtin_shapes"
# The intrinsic-method gate owns the one name -> prototype-builtin catalogue and
# the only construction of its proof token, so a static method resolution that
# wants the exact builtin cannot skip the live-prototype proof.
ir_intrinsic_method_lowering="crates/scriptw-ir/src/lowering/intrinsic_method.rs"
require_file "$ir_intrinsic_method_lowering"
require_module_decl "$ir_lowering" "intrinsic_method"
require_fixed_string_count \
  "$ir_intrinsic_method_lowering" \
  'pub(super) fn catalogued_method(' \
  1 \
  'intrinsic-method catalogue owner'
require_fixed_string_count \
  "$ir_intrinsic_method_lowering" \
  'IntrinsicMethod { builtin }' \
  1 \
  'intrinsic-method proof construction'
check_no_inline_legacy_includes "$ir_intrinsic_method_lowering"
# T02's assignment-expression boundary owns the exhaustive AssignOp/target
# dispatch across identifier, property, private, destructuring, logical and
# eager compound writes. Its specialized Reference lifecycles remain in their
# typed child modules; the parent expression dispatcher cannot regrow a second
# assignment implementation.
ir_assignment_lowering="crates/scriptw-ir/src/lowering/assignment.rs"
require_file "$ir_assignment_lowering"
require_module_decl "$ir_lowering" "assignment"
require_fixed_string_count \
  "$ir_assignment_lowering" \
  'pub(super) fn lower_assign(' \
  1 \
  'assignment-expression lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_assign(' \
  0 \
  'assignment-expression lowering outside child module'
check_no_inline_legacy_includes "$ir_assignment_lowering"
# The assignment owner also stages checked property/private/Super destinations
# across all three resumable protocols, retaining the original Reference.
check_raw_line_budget "$ir_assignment_lowering" 820
# T02's delete-expression boundary owns the complete target dispatch for
# property, private, super, identifier and value deletion. The unary
# dispatcher remains its sole caller and cannot regrow a second implementation.
ir_delete_expression_lowering="crates/scriptw-ir/src/lowering/delete_expression.rs"
require_file "$ir_delete_expression_lowering"
require_module_decl "$ir_lowering" "delete_expression"
require_fixed_string_count \
  "$ir_delete_expression_lowering" \
  'pub(super) fn lower_delete(' \
  1 \
  'delete-expression lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_delete(' \
  0 \
  'delete-expression lowering outside child module'
check_no_inline_legacy_includes "$ir_delete_expression_lowering"
# Measured after formatting the extraction: 217 raw lines. The margin is for
# maintenance of this exhaustive dispatcher, not unrelated lowering.
check_raw_line_budget "$ir_delete_expression_lowering" 250
# T02's new-expression boundary owns constructor target resolution, argument
# lowering, builtin/user result typing, dynamic-source rejection and static
# RegExp compilation. The parent expression dispatcher is its sole caller.
ir_new_expression_lowering="crates/scriptw-ir/src/lowering/new_expression.rs"
require_file "$ir_new_expression_lowering"
require_module_decl "$ir_lowering" "new_expression"
require_fixed_string_count \
  "$ir_new_expression_lowering" \
  'pub(super) fn lower_new(' \
  1 \
  'new-expression lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_new(' \
  0 \
  'new-expression lowering outside child module'
check_no_inline_legacy_includes "$ir_new_expression_lowering"
# Measured after formatting the extraction: 248 raw lines. The margin is for
# maintenance of constructor-expression lowering only.
check_raw_line_budget "$ir_new_expression_lowering" 290
# T02's statement boundary owns the exhaustive Statement dispatcher. Its
# expression child owns resumable expression-statement staging; control-flow
# implementations remain in their focused owners.
ir_statement_lowering="crates/scriptw-ir/src/lowering/statement.rs"
require_file "$ir_statement_lowering"
require_module_decl "$ir_lowering" "statement"
require_fixed_string_count \
  "$ir_statement_lowering" \
  'pub(super) fn lower_statement(' \
  1 \
  'statement lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_statement(' \
  0 \
  'statement lowering outside child module'
ir_statement_expression="crates/scriptw-ir/src/lowering/statement/expression.rs"
require_file "$ir_statement_expression"
require_exact_line_count "$ir_statement_lowering" 'mod expression;' 1 'private expression-statement module'
require_fixed_string_count "$ir_statement_lowering" 'Statement::Expression(expression) => self.lower_expression_statement(expression),' 1 'expression-statement dispatch consumption'
require_regex_count "$ir_statement_expression" '^[[:space:]]*pub\(in crate::lowering\)[[:space:]]+fn[[:space:]]+lower_expression_statement\(' 1 'expression-statement staging owner shared only inside lowering'
require_fixed_string_count "$ir_statement_lowering" 'fn lower_expression_statement(' 0 'expression-statement body outside child'
check_no_inline_legacy_includes "$ir_statement_lowering"
check_no_inline_legacy_includes "$ir_statement_expression"
# The dispatcher selects complete protocol/resource owners; its expression
# child stages their operands and preserves source Empty completion.
check_raw_line_budget "$ir_statement_lowering" 185
check_raw_line_budget "$ir_statement_expression" 350
# T02's for-in boundary owns the complete initializer/target/body lowering
# family. Retired source-shape recognizers must stay absent. The
# eager wrapper and common initializer utilities cross only the lowering-private
# boundary; the typed head child shares the original TDZ/target preparation.
ir_for_in_lowering="crates/scriptw-ir/src/lowering/for_in.rs"
require_file "$ir_for_in_lowering"
require_exact_line_count "$ir_lowering" 'mod for_in;' 1 'private for-in module declaration'
require_regex_count \
  "$ir_for_in_lowering" \
  '^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+lower_for_in_loop[[:space:]]*\(' \
  1 \
  'for-in statement-facing owner'
require_regex_count \
  "$ir_lowering" \
  '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+lower_for_in_loop[[:space:]]*\(' \
  0 \
  'for-in lowerer outside child module'
for private_owner in \
  widen_for_in_parameter_target \
  lower_for_in_initializer_prefix \
  prepend_statement
do
  require_regex_count \
    "$ir_for_in_lowering" \
    "^[[:space:]]*pub\\(super\\)[[:space:]]+fn[[:space:]]+${private_owner}[[:space:]]*[<(]" \
    1 \
    "private ${private_owner} owner"
  require_regex_count \
    "$ir_lowering" \
    "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+${private_owner}[[:space:]]*[<(]" \
    0 \
    "${private_owner} outside child module"
done
ir_for_in_head="crates/scriptw-ir/src/lowering/for_in/head.rs"
require_file "$ir_for_in_head"
require_exact_line_count "$ir_for_in_lowering" 'mod head;' 1 'private original ForIn head owner'
require_exact_line_count "$ir_for_in_lowering" 'pub(super) use head::ForInIterationHead;' 1 'lowering-private checked head projection'
require_fixed_string_count "$ir_for_in_head" 'pub(in crate::lowering) struct ForInIterationHead {' 1 'opaque original head shared by eager and complete protocols'
require_fixed_string_count "$ir_for_in_head" 'pub(in crate::lowering) fn prepare_for_in_iteration_head(' 1 'sole original ForIn head preparation'
for for_in_head_parent in "$ir_lowering" "$ir_for_in_lowering"; do
  require_regex_count "$for_in_head_parent" '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+prepare_for_in_iteration_head[[:space:]]*\(' 0 'no original ForIn head preparation copy outside the private head child'
done
# Target/reference selection and mapped pattern initialization for all protocols
# share the original head/TDZ preparation instead of copying it into each loop.
check_no_inline_legacy_includes "$ir_for_in_head"
check_raw_line_budget "$ir_for_in_head" 450
require_tree_regex_count crates/scriptw-ir/src/lowering '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+for_in_initializer_binding[[:space:]]*[<(]' 0 'retired untyped ForIn initializer helper'
for retired_for_in_recognizer in \
  for_in_known_empty_target \
  for_in_global_non_enumerable_guard_only \
  for_in_builtin_non_enumerable_assert_only \
  for_in_static_builtin_target \
  for_in_initializer_name \
  for_in_non_enumerable_guarded_assignment \
  for_in_non_enumerable_guard_name \
  for_in_not_same_value_guard_name \
  statement_is_simple_false_assignment \
  for_in_global_target \
  single_statement \
  expr_is_identifier_named \
  is_known_non_enumerable_global \
  is_known_non_enumerable_builtin_property
do
  require_tree_regex_count \
    crates/scriptw-ir/src \
    "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+${retired_for_in_recognizer}[[:space:]]*[<(]" \
    0 \
    "retired for-in source-shape recognizer ${retired_for_in_recognizer}"
done
require_fixed_string_count "$ir_for_in_lowering" 'self.lower_for_in_head_target(&head, for_in.target(),' 1 'for-in evaluates the original target through the shared head owner'
require_fixed_string_count crates/scriptw-aot-wasm/src/control_flow/for_in.rs 'self.emit_create_for_in_enumerator(&source, function)?' 1 'for-in consumes actual runtime property enumeration'
# Four lowering-private methods and the opaque typed head reexport are the
# complete facade; the head algorithm itself remains in its private child.
require_regex_count \
  "$ir_for_in_lowering" \
  '^[[:space:]]*fn[[:space:]]+' \
  0 \
  'private method'
require_regex_count \
  "$ir_for_in_lowering" \
  '^[[:space:]]*((pub(\([^)]*\))?|const|async|unsafe|extern|"[^"]*")[[:space:]]+)*fn[[:space:]]+[[:alnum:]_]+' \
  4 \
  'total function declaration'
require_regex_count \
  "$ir_for_in_lowering" \
  '^[[:space:]]*pub(\([^)]*\))?[[:space:]]+' \
  5 \
  'Rust-visible item'
for shared_parent_owner in \
  lower_for_in_of_environment \
  lower_for_head_expression_with_tdz \
  static_string_expression \
  plain_async_entry_state \
  lower_loop_body \
  lower_web_compat_loop_assignment_target \
  unwrap_parenthesized_expr
do
  require_regex_count \
    "$ir_lowering" \
    "^[[:space:]]*fn[[:space:]]+${shared_parent_owner}[[:space:]]*[<(]" \
    1 \
    "parent-owned shared ${shared_parent_owner} helper"
  require_regex_count \
    "$ir_for_in_lowering" \
    "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+${shared_parent_owner}[[:space:]]*[<(]" \
    0 \
    "shared ${shared_parent_owner} helper copied into for-in owner"
done
require_fixed_string_count 'crates/scriptw-ir/src/lowering/var_declaration.rs' 'pub(super) fn lower_var_declarator(' 1 'shared var declarator authority'
require_fixed_string_count "$ir_for_in_lowering" 'self.lower_var_declarator(variable)?' 1 'for-in consumes shared var declaration owner'
require_regex_count "$ir_for_in_lowering" '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+lower_var_declarator[[:space:]]*[<(]' 0 'no var declarator body copied into for-in'
for shared_free_owner in supported_bound_names for_in_loop_binding_storage_name; do
  require_regex_count \
    "crates/scriptw-ir/src/lowering_helpers.rs" \
    "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+${shared_free_owner}[[:space:]]*[<(]" \
    1 \
    "shared ${shared_free_owner} free-helper owner"
  require_regex_count \
    "$ir_for_in_lowering" \
    "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+${shared_free_owner}[[:space:]]*[<(]" \
    0 \
    "shared ${shared_free_owner} free helper copied into for-in owner"
done
# Boa's generic containment visitor is imported rather than locally owned, but
# copying one into the child would fork the grammar predicate this family uses.
require_regex_count \
  "$ir_for_in_lowering" \
  '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+contains[[:space:]]*[<(]' \
  0 \
  'generic contains helper copied into for-in owner'
require_regex_count \
  "$ir_for_in_lowering" \
  '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?(struct|enum|union|type)[[:space:]]+ForInOfEnvironmentIr([[:space:]]|<|\{|\(|=|;|:)' \
  0 \
  'shared for-in/of environment type declaration copied into for-in owner'
require_tree_regex_count \
  "crates/scriptw-ir/src" \
  '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?(struct|enum|union|type)[[:space:]]+ForInOfEnvironmentIr([[:space:]]|<|\{|\(|=|;|:)' \
  1 \
  'shared for-in/of environment type'
check_no_inline_legacy_includes "$ir_for_in_lowering"
# Measured after formatting the extraction: 571 raw lines. The margin is for
# maintenance of this closed family, not unrelated lowering or shared helpers.
check_raw_line_budget "$ir_for_in_lowering" 650
# T02's classic-for boundary owns the complete head/environment/resumption
# lifecycle and the final For/GeneratorLoop choice. The statement dispatcher
# remains its sole caller and the parent cannot regrow a second implementation.
ir_for_loop_lowering="crates/scriptw-ir/src/lowering/for_loop.rs"
require_file "$ir_for_loop_lowering"
require_module_decl "$ir_lowering" "for_loop"
require_fixed_string_count \
  "$ir_for_loop_lowering" \
  'pub(super) fn lower_for_loop(' \
  1 \
  'classic-for lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_for_loop(' \
  0 \
  'classic-for lowering outside child module'
check_no_inline_legacy_includes "$ir_for_loop_lowering"
# Measured after formatting the extraction: 213 raw lines. The margin is for
# maintenance of the classic-for lifecycle, not unrelated loop lowering.
# The source-proof admission wrapper adds one line beyond the former margin.
# Classic For now selects complete resource-head and resumable phase owners.
check_raw_line_budget "$ir_for_loop_lowering" 310
require_module_decl "$ir_lowering" "synchronous_resource_loop"
require_file "crates/scriptw-ir/src/lowering/synchronous_resource_loop.rs"
check_raw_line_budget "crates/scriptw-ir/src/lowering/synchronous_resource_loop.rs" 155
# The shared current-activation visitor stays in a private child; the parent
# retains only proven-loop lowering and its three source-admission wrappers.
require_module_decl "crates/scriptw-ir/src/lowering/synchronous_resource_loop.rs" "source_suspension"
require_file "crates/scriptw-ir/src/lowering/synchronous_resource_loop/source_suspension.rs"
check_no_inline_legacy_includes "crates/scriptw-ir/src/lowering/synchronous_resource_loop/source_suspension.rs"
check_raw_line_budget "crates/scriptw-ir/src/lowering/synchronous_resource_loop/source_suspension.rs" 180
require_file "crates/scriptw-ir/src/synchronous_loop_body.rs"
# The eager-body predicate also rejects complete child protocol phases.
check_raw_line_budget "crates/scriptw-ir/src/synchronous_loop_body.rs" 210
# T02's for-of boundary owns every specialization decision and the
# lowering-only protocol carrier. The statement dispatcher is the sole caller;
# shared loop/environment helpers and public statement/protocol IR remain in
# their existing owners.
ir_for_of_lowering="crates/scriptw-ir/src/lowering/for_of.rs"
ir_for_of_protocol_lowering="crates/scriptw-ir/src/lowering/for_of/protocol.rs"
ir_for_of_async_lowering="crates/scriptw-ir/src/lowering/for_of/async_function.rs"
ir_async_for_of_iterator="crates/scriptw-ir/src/ir/async_for_of_iterator.rs"
require_file "$ir_for_of_lowering"
require_file "$ir_for_of_protocol_lowering"
require_file "$ir_for_of_async_lowering"
require_file "$ir_async_for_of_iterator"
require_exact_line_count "$ir_for_of_lowering" 'mod async_function;' 1 'private plain-async for-of producer child'
require_exact_line_count "crates/scriptw-ir/src/ir.rs" 'mod async_for_of_iterator;' 1 'private completed plain-async iterator owner'
require_exact_line_count "$ir_lowering" 'mod for_of;' 1 'private for-of module declaration'
require_exact_line_count \
  "$ir_for_of_lowering" \
  'mod protocol;' \
  1 \
  'private for-of protocol child declaration'
require_fixed_string_count \
  "$ir_for_of_lowering" \
  'pub(super) fn lower_for_of_loop(' \
  1 \
  'for-of statement-facing owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_for_of_loop(' \
  0 \
  'for-of lowering outside child module'
for private_owner in lower_for_of_head; do
  require_fixed_string_count \
    "$ir_for_of_lowering" \
    "fn ${private_owner}(" \
    1 \
    "private ${private_owner} owner"
  require_fixed_string_count \
    "$ir_lowering" \
    "fn ${private_owner}(" \
    0 \
    "${private_owner} outside child module"
done
for private_owner in lower_async_function_for_of_iterator_with_body_await lower_plain_async_for_await_with_body_await; do
  require_fixed_string_count "$ir_for_of_async_lowering" "fn ${private_owner}(" 1 "private ${private_owner} producer"
  require_fixed_string_count "$ir_for_of_lowering" "fn ${private_owner}(" 0 "${private_owner} definition outside producer leaf"
  require_fixed_string_count "$ir_lowering" "fn ${private_owner}(" 0 "${private_owner} definition in lowering parent"
done
require_fixed_string_count \
  "$ir_for_of_lowering" \
  'enum AsyncForOfArrayWalkForm' \
  0 \
  'retired resumable array-walk classification carrier'
require_fixed_string_count \
  "crates/scriptw-ir/src/lowering_helpers.rs" \
  'enum AsyncForOfArrayWalkForm' \
  0 \
  'resumable array-walk carrier declarations in the former helper owner'
require_tree_regex_count \
  "crates/scriptw-ir/src" \
  '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?enum[[:space:]]+AsyncForOfArrayWalkForm([[:space:]]|\{)' \
  0 \
  'retired resumable array-walk carrier'
require_fixed_string_count \
  "$ir_async_for_of_iterator" \
  'pub struct AsyncFunctionForOfIteratorPlanIr' \
  1 \
  'completed plain-async for-of plan owner'
require_fixed_string_count "crates/scriptw-ir/src/ir.rs" 'pub struct AsyncFunctionForOfIteratorPlanIr' 0 'completed plan definition in IR parent'
require_fixed_string_count \
  "$ir_for_of_lowering" \
  'struct AsyncFunctionForOfIteratorPlanIr' \
  0 \
  'resumable synchronous for-of plan declarations outside the IR owner'
require_tree_regex_count \
  "crates/scriptw-ir/src" \
  '^[[:space:]]*pub[[:space:]]+struct[[:space:]]+AsyncFunctionForOfIteratorPlanIr([[:space:]]|\{)' \
  1 \
  'resumable synchronous for-of plan'
require_fixed_string_count \
  "$ir_for_of_protocol_lowering" \
  'pub(super) struct ForOfLoweringIr' \
  1 \
  'private for-of protocol carrier'
require_fixed_string_count \
  "$ir_for_of_lowering" \
  'struct ForOfLoweringIr' \
  0 \
  'for-of protocol carrier declarations outside the protocol child'
require_fixed_string_count \
  "crates/scriptw-ir/src/ir.rs" \
  'struct ForOfLoweringIr' \
  0 \
  'for-of protocol carrier declarations in the former IR owner'
require_tree_regex_count \
  "crates/scriptw-ir/src" \
  '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?struct[[:space:]]+ForOfLoweringIr([[:space:]]|\{)' \
  1 \
  'for-of protocol carrier'
# The statement-facing wrapper and the proven resource-region entry are visible
# to the private lowering parent. The protocol carrier stays in its own child.
require_regex_count \
  "$ir_for_of_lowering" \
  '^[[:space:]]*pub(\([^)]*\))?[[:space:]]+' \
  2 \
  'Rust-visible item'
for shared_helper in \
  plain_async_entry_state \
  split_resumable_loop_body \
  lower_loop_body \
  lower_for_in_of_environment \
  lower_for_head_expression_with_tdz \
  for_of_loop_binding_storage_name
do
  require_fixed_string_count \
    "$ir_for_of_lowering" \
    "fn ${shared_helper}(" \
    0 \
    "shared ${shared_helper} helper copied into for-of owner"
done
require_fixed_string_count \
  "$ir_for_of_lowering" \
  'fn generator_loop_has_unsupported_control' \
  0 \
  'shared generic generator_loop_has_unsupported_control helper copied into for-of owner'
require_fixed_string_count \
  "$ir_for_of_lowering" \
  'enum LoweredForOfHeadKind' \
  0 \
  'async-disposable head-kind type copied into for-of owner'
check_no_inline_legacy_includes "$ir_for_of_lowering"
check_no_inline_legacy_includes "$ir_for_of_protocol_lowering"
check_no_inline_legacy_includes "$ir_for_of_async_lowering"
check_no_inline_legacy_includes "$ir_async_for_of_iterator"
for private_owner in "$ir_for_of_async_lowering" "$ir_async_for_of_iterator"; do
  if grep -Eq '^pub(\([^)]*\))? mod ' "$private_owner"; then
    fail "$private_owner must keep implementation children private"
  fi
done
# Measured after separating the protocol witness carrier: 731 raw lines. The
# margin is for maintenance of the complete for-of lowering family, not
# unrelated lowering.
# Measured canonical ForOf head/body owner: 762 raw lines. The neutral
# resumable local-control ownership import added one line to the 761-line owner.
# Bare identifier assignment adds five actual dispatch lines to pass the already
# lowered head classification into its consuming generator proof constructor.
# Ordinary-property assignment adds 22 measured dispatch lines to consume the
# same fused Reference completion as plain assignment and the checked head plan.
# Eager generator lexical patterns add the actual semantic initializer and
# checked lexical-head dispatch. The complete current owner measures 876 lines.
check_raw_line_budget "$ir_for_of_lowering" 876
# The consumed producer measures 188 lines and the completed plan 365.
# Keep their maintenance margins local to these actual owners.
check_raw_line_budget "$ir_for_of_async_lowering" 210
check_raw_line_budget "$ir_async_for_of_iterator" 390
# The shared protocol carrier measures 129 lines after admitting the completed
# plain-async plan; its constructors retain one protocol authority.
check_raw_line_budget "$ir_for_of_protocol_lowering" 140
# T15's assignment head proof is a real private child consumed by the existing
# lowering and iterator-plan constructors. Reuse the sole summary visitor for
# entry-local escape checks rather than adding a second opcode walker.
ir_generator_for_of_plan="crates/scriptw-ir/src/generator_for_of_iterator.rs"
ir_generator_for_of_head="crates/scriptw-ir/src/generator_for_of_iterator/head.rs"
ir_resumable_sync_for_of_binding="crates/scriptw-ir/src/resumable_sync_for_of_binding.rs"
require_file "$ir_generator_for_of_head"
require_file "$ir_resumable_sync_for_of_binding"
require_exact_line_count "$ir_generator_for_of_plan" 'mod head;' 1 'private generator head proof module'
if grep -Eq '^pub(\([^)]*\))? mod head;' "$ir_generator_for_of_plan"; then
  fail "$ir_generator_for_of_plan must keep its head proof child private"
fi
check_no_inline_legacy_includes "$ir_generator_for_of_head"
require_fixed_string_count \
  'crates/scriptw-ir/src/lowering/for_of/generator.rs' \
  'GeneratorForOfAssignmentIr::identifier(' \
  1 \
  'actual generator identifier-head proof producer'
require_fixed_string_count \
  'crates/scriptw-ir/src/lowering/for_of/generator.rs' \
  'GeneratorForOfAssignmentIr::ordinary_property(' \
  1 \
  'actual generator ordinary-property head proof producer'
require_fixed_string_count \
  "$ir_resumable_sync_for_of_binding" \
  'pub(crate) struct ValidatedResumableSyncForOfLexicalPatternIr' \
  1 \
  'sole checked shared lexical-pattern head'
for lexical_pattern_consumer in "$ir_for_of_lowering"; do
  require_fixed_string_count \
    "$lexical_pattern_consumer" \
    'ValidatedResumableSyncForOfLexicalPatternIr::new(' \
    1 \
    'lowering-only shared lexical-pattern proof constructor'
done
# Async IR retains its own complete initialization validator; generator heads
# consume the same lowering-produced lexical-pattern proof without rebuilding it.
require_fixed_string_count "$ir_generator_for_of_head" 'pattern: ValidatedResumableSyncForOfLexicalPatternIr,' 2 'generator borrowed/owned lexical-pattern proof input'
require_fixed_string_count \
  'crates/scriptw-ir/src/lowering/for_of/generator.rs' \
  'GeneratorForOfLexicalPatternIr::new(' \
  1 \
  'actual generator initializer-prefix proof consumer'
require_fixed_string_count \
  "$ir_for_of_lowering" \
  'self.complete_ordinary_property_plain_assignment(' \
  1 \
  'consumed ordinary-property Reference completion in generator head'
require_fixed_string_count \
  'crates/scriptw-ir/src/ir.rs' \
  'pub(crate) fn statements_reference_storage(' \
  1 \
  'sole exhaustive storage-query owner'
require_fixed_string_count \
  "$ir_generator_for_of_head" \
  'statements_reference_storage(rest, &self.value_name)' \
  1 \
  'consumed assignment-head body escape check'
# Existing consumed head/shared-proof files measured after the complete lexical
# initializer and lifetime proof join; no alternate opcode visitor is added.
check_raw_line_budget "$ir_generator_for_of_head" 413
check_raw_line_budget "$ir_resumable_sync_for_of_binding" 459

# T02's if-statement boundary owns branch lowering, flow-fact joins and the
# generator split/merge lifecycle. The shared static expression helpers remain
# parent-owned and the parent cannot regrow a second if implementation.
ir_if_statement_lowering="crates/scriptw-ir/src/lowering/if_statement.rs"
ir_generator_if_environment_owner="crates/scriptw-ir/src/lowering/if_statement/checked_environment.rs"
require_file "$ir_generator_if_environment_owner"
require_module_decl "$ir_if_statement_lowering" "checked_environment"
require_fixed_string_count \
  "$ir_if_statement_lowering" \
  'self.generator_if_branch_owns_runtime_environment(if_statement)' \
  1 \
  'checked runtime-record conditional admission consumer'
require_fixed_string_count \
  "$ir_generator_if_environment_owner" \
  'fn generator_if_branch_owns_runtime_environment(' \
  1 \
  'checked runtime-record conditional admission owner'
require_fixed_string_count \
  "crates/scriptw-ir/src/lowering/function_definition.rs" \
  'lowerer.generator_plan_for_lowered_named_environment(' \
  1 \
  'checked named-record generator plan consumer'
require_fixed_string_count \
  "$ir_generator_if_environment_owner" \
  'fn generator_plan_for_lowered_named_environment(' \
  1 \
  'checked named-record generator plan owner'
check_raw_line_budget "$ir_generator_if_environment_owner" 64
require_file "$ir_if_statement_lowering"
require_module_decl "$ir_lowering" "if_statement"
require_fixed_string_count \
  "$ir_if_statement_lowering" \
  'pub(super) fn lower_if_statement(' \
  1 \
  'if-statement lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_if_statement(' \
  0 \
  'if-statement lowering outside child module'
require_fixed_string_count \
  "$ir_if_statement_lowering" \
  'fn split_generator_if_branch(' \
  1 \
  'generator if-branch split helper'
require_fixed_string_count \
  "$ir_lowering" \
  'fn split_generator_if_branch(' \
  0 \
  'generator if-branch split helper outside child module'
require_fixed_string_count \
  "$ir_if_statement_lowering" \
  'fn statement_completes_by_throw(' \
  1 \
  'if-branch abrupt-completion helper'
require_fixed_string_count \
  "$ir_lowering" \
  'fn statement_completes_by_throw(' \
  0 \
  'if-branch abrupt-completion helper outside child module'
require_fixed_string_count \
  "$ir_lowering" \
  'fn static_bool_expr(' \
  1 \
  'shared static boolean helper in parent'
require_fixed_string_count \
  "$ir_if_statement_lowering" \
  'fn static_bool_expr(' \
  0 \
  'static boolean helper copied into if-statement owner'
check_no_inline_legacy_includes "$ir_if_statement_lowering"
# Measured with typed plain-async branch ranges: 190 raw lines. The margin is
# for if-statement lowering only; plan invariants remain in async_if.rs.
# Complete plain-async branches share the checked region path.
check_raw_line_budget "$ir_if_statement_lowering" 240
# T02's labelled-statement boundary owns nested label collection, target-kind
# classification and final Labelled IR assembly. The active-label stack types
# remain parent-owned because break/continue lowering also consumes them.
ir_labelled_statement_lowering="crates/scriptw-ir/src/lowering/labelled_statement.rs"
require_file "$ir_labelled_statement_lowering"
require_module_decl "$ir_lowering" "labelled_statement"
require_fixed_string_count \
  "$ir_labelled_statement_lowering" \
  'pub(super) fn lower_labelled(' \
  1 \
  'labelled-statement lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_labelled(' \
  0 \
  'labelled-statement lowering outside child module'
require_fixed_string_count \
  "$ir_labelled_statement_lowering" \
  'fn collect_labels' \
  1 \
  'nested-label collection helper'
require_fixed_string_count \
  "$ir_lowering" \
  'fn collect_labels' \
  0 \
  'nested-label collection helper outside child module'
require_fixed_string_count \
  "$ir_labelled_statement_lowering" \
  'fn label_target_kind' \
  1 \
  'label target-kind helper'
require_fixed_string_count \
  "$ir_lowering" \
  'fn label_target_kind' \
  0 \
  'label target-kind helper outside child module'
require_fixed_string_count "$ir_lowering" 'struct ActiveLabel' 1 'shared active-label type in parent'
require_fixed_string_count "$ir_labelled_statement_lowering" 'struct ActiveLabel' 0 'active-label type copied into labelled owner'
require_fixed_string_count "$ir_lowering" 'enum LabelTargetKind' 1 'shared label-target type in parent'
require_fixed_string_count "$ir_labelled_statement_lowering" 'enum LabelTargetKind' 0 'label-target type copied into labelled owner'
check_no_inline_legacy_includes "$ir_labelled_statement_lowering"
# Measured after formatting the extraction: 72 raw lines. The margin is for
# maintenance of labelled-statement lowering only.
check_raw_line_budget "$ir_labelled_statement_lowering" 100
# T02's abrupt loop-control boundary owns all labelled/unlabelled break and
# continue validation plus final abrupt-control IR assembly. The active-label
# types stay parent-owned because labelled_statement produces them while this
# child consumes them.
ir_break_continue_lowering="crates/scriptw-ir/src/lowering/break_continue.rs"
require_file "$ir_break_continue_lowering"
require_module_decl "$ir_lowering" "break_continue"
require_fixed_string_count \
  "$ir_break_continue_lowering" \
  'pub(super) fn lower_break(' \
  1 \
  'break lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_break(' \
  0 \
  'break lowering outside child module'
require_fixed_string_count \
  "$ir_break_continue_lowering" \
  'pub(super) fn lower_continue(' \
  1 \
  'continue lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_continue(' \
  0 \
  'continue lowering outside child module'
require_fixed_string_count \
  "$ir_break_continue_lowering" \
  'struct ActiveLabel' \
  0 \
  'active-label type copied into break/continue owner'
require_fixed_string_count \
  "$ir_break_continue_lowering" \
  'enum LabelTargetKind' \
  0 \
  'label-target type copied into break/continue owner'
check_no_inline_legacy_includes "$ir_break_continue_lowering"
# Measured after formatting the extraction: 45 raw lines. The margin is for
# maintenance of break/continue lowering only.
check_raw_line_budget "$ir_break_continue_lowering" 70
# T02's while-family boundary owns ordinary/resumable while lowering and the
# explicit do-while suspension refusal. Shared loop resumption helpers remain
# parent-owned and the parent cannot regrow either loop implementation.
ir_while_loop_lowering="crates/scriptw-ir/src/lowering/while_loop.rs"
require_file "$ir_while_loop_lowering"
require_module_decl "$ir_lowering" "while_loop"
require_fixed_string_count \
  "$ir_while_loop_lowering" \
  'pub(super) fn lower_while_loop(' \
  1 \
  'while-loop lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_while_loop(' \
  0 \
  'while-loop lowering outside child module'
require_fixed_string_count \
  "$ir_while_loop_lowering" \
  'pub(super) fn lower_do_while_loop(' \
  1 \
  'do-while lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_do_while_loop(' \
  0 \
  'do-while lowering outside child module'
require_fixed_string_count \
  "$ir_lowering" \
  'fn plain_async_entry_state(' \
  1 \
  'shared plain-async loop-state helper in parent'
require_fixed_string_count \
  "$ir_while_loop_lowering" \
  'fn plain_async_entry_state(' \
  0 \
  'plain-async loop-state helper copied into while owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn split_resumable_loop_body(' \
  1 \
  'shared resumable-loop split helper in parent'
require_fixed_string_count \
  "$ir_while_loop_lowering" \
  'fn split_resumable_loop_body(' \
  0 \
  'resumable-loop split helper copied into while owner'
check_no_inline_legacy_includes "$ir_while_loop_lowering"
# Measured after formatting the extraction: 106 raw lines. The margin is for
# maintenance of while/do-while lowering only.
check_raw_line_budget "$ir_while_loop_lowering" 130

# T14's awaited while condition has its own checked IR/lowering/emission owners.
# Ordinary while and do-while remain within their existing 130-line boundary.
require_file crates/scriptw-ir/src/async_while.rs
require_module_decl "$ir_lib" async_while
require_exact_line_count "$ir_lib" 'pub use async_while::AsyncFunctionWhileConditionIr;' 1 'checked awaited while condition re-export'
require_file crates/scriptw-ir/src/lowering/awaited_while_condition.rs
require_module_decl "$ir_lowering" awaited_while_condition
require_file crates/scriptw-ir/src/lowering/eager_async_while_body.rs
require_module_decl "$ir_lowering" eager_async_while_body
require_fixed_string_count crates/scriptw-ir/src/lowering/awaited_while_condition.rs 'EagerAsyncWhileBody::new(while_loop.body())' 1 'checked eager async while body admission'
require_fixed_string_count crates/scriptw-ir/src/lowering/awaited_while_condition.rs 'let (body, kind) = body.lower(self);' 1 'checked eager async while body consumption'
require_fixed_string_count crates/scriptw-ir/src/lowering/awaited_while_condition.rs 'current_async_resume_state = None' 0 'unchecked continuation suppression in awaited while owner'
check_no_inline_legacy_includes crates/scriptw-ir/src/lowering/eager_async_while_body.rs
check_raw_line_budget crates/scriptw-ir/src/lowering/eager_async_while_body.rs 45
require_fixed_string_count crates/scriptw-ir/src/lowering/awaited_while_condition.rs 'pub(super) fn lower_awaited_while_condition(' 1 'awaited while condition lowering owner'
require_fixed_string_count "$ir_while_loop_lowering" 'self.lower_awaited_while_condition(while_loop, entry_state)' 1 'awaited while condition production admission'
require_fixed_string_count "$ir_lowering" 'fn lower_awaited_while_condition(' 0 'awaited while condition implementation copied into parent'
require_file crates/scriptw-aot-wasm/src/control_flow/async_function_while.rs
require_module_decl crates/scriptw-aot-wasm/src/control_flow.rs async_function_while
require_fixed_string_count crates/scriptw-aot-wasm/src/control_flow/async_function_while.rs 'pub(super) fn compile_async_function_while_condition(' 1 'awaited while condition emitter owner'
for awaited_while_owner in crates/scriptw-ir/src/async_while.rs crates/scriptw-ir/src/lowering/awaited_while_condition.rs crates/scriptw-aot-wasm/src/control_flow/async_function_while.rs; do
  check_no_inline_legacy_includes "$awaited_while_owner"
done
ir_async_while_prefix="crates/scriptw-ir/src/async_while/condition_prefix.rs"
ir_async_while_tests="crates/scriptw-ir/src/async_while/tests.rs"
require_file "$ir_async_while_prefix"
require_file "$ir_async_while_tests"
require_exact_line_count crates/scriptw-ir/src/async_while.rs 'mod condition_prefix;' 1 'private awaited-while prefix validator module'
require_exact_line_count crates/scriptw-ir/src/async_while.rs 'mod tests;' 1 'private awaited-while unit-control module'
require_fixed_string_count crates/scriptw-ir/src/async_while.rs 'condition_prefix::validate_condition_prefix(&condition_prefix, entry_state, ready_state)?;' 1 'mandatory awaited-while constructor validation'
require_regex_count "$ir_async_while_prefix" '^pub\(super\) fn validate_condition_prefix\(' 1 'restartable prefix validator owner'
require_regex_count "$ir_async_while_prefix" '^fn validate_value_arm\(' 1 'private value-arm validator owner'
check_no_inline_legacy_includes "$ir_async_while_prefix"
check_no_inline_legacy_includes "$ir_async_while_tests"
# Measured constructor70, prefix validator75 and existing unit controls249.
check_raw_line_budget crates/scriptw-ir/src/async_while.rs 90
check_raw_line_budget "$ir_async_while_prefix" 90
check_raw_line_budget "$ir_async_while_tests" 270
# Loop-context admission precedes condition lowering; its retained context
# save/restore accounts for five additional source lines.
# The eager While boundary admits checked References and routes class/complete
# child phases to the separate whole classic owner.
check_raw_line_budget crates/scriptw-ir/src/lowering/awaited_while_condition.rs 140
check_raw_line_budget crates/scriptw-aot-wasm/src/control_flow/async_function_while.rs 90
# T02's switch-statement boundary owns discriminant and selector evaluation,
# the one shared CaseBlock lexical environment, case-body fact joins and final
# Switch IR assembly. Statement-list and environment materialization helpers
# stay parent-owned because other statement families consume them too.
ir_switch_statement_lowering="crates/scriptw-ir/src/lowering/switch_statement.rs"
require_file "$ir_switch_statement_lowering"
require_module_decl "$ir_lowering" "switch_statement"
require_fixed_string_count \
  "$ir_switch_statement_lowering" \
  'pub(super) fn lower_switch(' \
  1 \
  'switch-statement lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_switch(' \
  0 \
  'switch-statement lowering outside child module'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_statement_items_without_function_initialization(' \
  1 \
  'shared statement-list helper in parent'
require_fixed_string_count \
  "$ir_switch_statement_lowering" \
  'fn lower_statement_items_without_function_initialization(' \
  0 \
  'statement-list helper copied into switch owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_materialized_lexical_environment(' \
  1 \
  'shared lexical-environment materializer in parent'
require_fixed_string_count \
  "$ir_switch_statement_lowering" \
  'fn lower_materialized_lexical_environment(' \
  0 \
  'lexical-environment materializer copied into switch owner'
check_no_inline_legacy_includes "$ir_switch_statement_lowering"
# Measured after formatting the extraction: 90 raw lines. The margin is for
# maintenance of switch-statement lowering only.
# The dispatcher selects the three complete protocols and CaseBlock resources.
check_raw_line_budget "$ir_switch_statement_lowering" 140
# T02's with-statement boundary owns the full Object Environment lifecycle:
# outer object evaluation, hidden binding materialization, ordered chain entry
# and exit, body lowering and lexical-block assembly. Shared allocation and
# suspension helpers plus reference lifecycle types remain in their owners.
ir_with_statement_lowering="crates/scriptw-ir/src/lowering/with_statement.rs"
require_file "$ir_with_statement_lowering"
require_module_decl "$ir_lowering" "with_statement"
require_fixed_string_count \
  "$ir_with_statement_lowering" \
  'pub(super) fn lower_with_statement(' \
  1 \
  'with-statement lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_with_statement(' \
  0 \
  'with-statement lowering outside child module'
for shared_helper in object_like_kind_set alloc_temp_binding_name add_suspension_owned_binding; do
  require_fixed_string_count \
    "$ir_with_statement_lowering" \
    "fn ${shared_helper}(" \
    0 \
    "shared ${shared_helper} helper copied into with-statement owner"
done
for reference_type in ObjectEnvironmentBindingObject CurrentScopeDepth OrderedWithEnvironmentChain; do
  require_fixed_string_count \
    "$ir_with_statement_lowering" \
    "struct ${reference_type}" \
    0 \
    "shared ${reference_type} lifecycle type copied into with-statement owner"
done
require_fixed_string_count \
  "$ir_with_statement_lowering" \
  'with_environment_chain.enter_current(' \
  1 \
  'with-environment chain entry'
require_fixed_string_count \
  "$ir_with_statement_lowering" \
  'with_environment_chain.leave_current();' \
  1 \
  'with-environment chain exit'
check_no_inline_legacy_includes "$ir_with_statement_lowering"
# Measured after formatting the extraction: 103 raw lines. The margin is for
# maintenance of with-statement lowering only.
# The analyzed continuation token selects whole or original linear With.
check_raw_line_budget "$ir_with_statement_lowering" 160
# T02's property-access boundary owns ordinary, private and super access
# dispatch plus the primitive/exotic target-kind split. Keep that split
# exhaustive; Number now consumes the same live primitive GetV owner as the
# other primitive prototypes, preserving mutable getter effects.
ir_property_access_lowering="crates/scriptw-ir/src/lowering/property_access.rs"
require_file "$ir_property_access_lowering"
require_module_decl "$ir_lowering" "property_access"
require_fixed_string_count \
  "$ir_property_access_lowering" \
  'pub(super) fn lower_property_access(' \
  1 \
  'property-access lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_property_access(' \
  0 \
  'property-access lowering outside child module'
require_fixed_string_count \
  "$ir_property_access_lowering" \
  'ValueKind::Number => self.lower_primitive_property_key(' \
  1 \
  'Number dispatch through the shared primitive GetV owner'
require_fixed_string_count "$ir_property_access_lowering" 'IntrinsicPrototype::Number,' 1 'Number selects its actual primitive prototype'
if grep -Fq '_ => self.unsupported_expr("property access on non-object target")' "$ir_property_access_lowering"; then
  fail "$ir_property_access_lowering must exhaust ValueKind instead of hiding future variants behind a catch-all"
fi
check_no_inline_legacy_includes "$ir_property_access_lowering"
# The shared private GetValue body observes accessor effects before ordinary
# and suspended calls consume the acquired value. Syntax dispatch lives beside
# that actual owner, rather than retaining a second parent implementation.
for private_get_value_entry in lower_private_get_value lower_private_property_access; do
  require_fixed_string_count "$ir_property_access_lowering" "pub(super) fn ${private_get_value_entry}(" 1 'sole private GetValue/syntax owner'
  require_fixed_string_count "$ir_lowering" "fn ${private_get_value_entry}(" 0 'no parent private GetValue/syntax copy'
done
require_fixed_string_count "$ir_property_access_lowering" 'self.lower_private_get_value(&mut target, private_name_id)' 1 'syntax private GetValue consumer'
require_fixed_string_count crates/scriptw-ir/src/lowering/call_expression.rs 'self.lower_private_get_value(&mut materialized_receiver, private_name_id)' 1 'acquired private Call GetValue consumer'
# Measured after the shared private GetValue join: 267 raw lines. The margin is
# for maintenance of this consumed property-access owner only.
check_raw_line_budget "$ir_property_access_lowering" 280
# T02's call-expression boundary keeps the public entry and direct
# identifier/property recognition in one child family. Its private nested child
# owns only the terminal non-property call path. The parent owns expression
# dispatch and reusable helpers, but cannot regrow a second call implementation.
ir_call_expression_lowering="crates/scriptw-ir/src/lowering/call_expression.rs"
ir_non_property_call_lowering="crates/scriptw-ir/src/lowering/call_expression/non_property_call.rs"
require_file "$ir_call_expression_lowering"
require_file "$ir_non_property_call_lowering"
require_module_decl "$ir_lowering" "call_expression"
require_exact_line_count \
  "$ir_call_expression_lowering" \
  'mod non_property_call;' \
  1 \
  'private non-property call child declaration'
require_fixed_string_count \
  "$ir_call_expression_lowering" \
  'pub(super) fn lower_call(' \
  1 \
  'call-expression lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_call(' \
  0 \
  'call-expression lowering body outside child module'
require_fixed_string_count \
  "$ir_non_property_call_lowering" \
  'pub(super) fn lower_non_property_call(' \
  1 \
  'non-property call lowering owner'
require_tree_regex_count \
  "crates/scriptw-ir/src" \
  '^[[:space:]]*((pub(\([^)]*\))?|default|const|async|unsafe|extern|"[^"]*")[[:space:]]+)*fn[[:space:]]+lower_non_property_call[[:space:]]*\(' \
  1 \
  'non-property call lowering owner'
require_fixed_string_count \
  "$ir_call_expression_lowering" \
  'self.lower_non_property_call(callee, args)' \
  1 \
  'non-property call lowering dispatch'
require_tree_regex_count \
  "crates/scriptw-ir/src" \
  '^[[:space:]]*self\.lower_non_property_call\(callee, args\)' \
  1 \
  'non-property call lowering dispatch'
call_expression_terminal_suffix="$(tail -n 3 "$ir_call_expression_lowering")"
expected_call_expression_terminal_suffix=$'        self.lower_non_property_call(callee, args)\n    }\n}'
if [ "$call_expression_terminal_suffix" != "$expected_call_expression_terminal_suffix" ]; then
  fail "$ir_call_expression_lowering must end by dispatching the non-property call owner"
fi
require_fixed_string_count \
  "$ir_call_expression_lowering" \
  'unsupported_call' \
  0 \
  'unreachable constructor fallback in property calls'
require_fixed_string_count \
  "$ir_non_property_call_lowering" \
  'unsupported_call' \
  0 \
  'unreachable constructor fallback in non-property calls'
check_no_inline_legacy_includes "$ir_call_expression_lowering"
check_no_inline_legacy_includes "$ir_non_property_call_lowering"
# Measured after binding-query and observable coercion repairs: 3,112 raw lines.
# Direct identifier/property recognition remains the only implementation owner.
check_raw_line_budget "$ir_call_expression_lowering" 3112
# Measured after extraction: 315 raw lines. This private child owns optional,
# erased, multi-target and exact-target calls after callee-value lowering.
check_raw_line_budget "$ir_non_property_call_lowering" 350
# T02's builtin call-result boundary owns the exhaustive StandardBuiltinId
# result analysis and its narrowly related observation updates. Construct,
# direct-call, RegExp-literal and well-known-symbol routing remain consumers;
# the parent orchestration file cannot regrow a second result table.
ir_builtin_call_info_lowering="crates/scriptw-ir/src/lowering/builtin_call_info.rs"
require_file "$ir_builtin_call_info_lowering"
require_module_decl "$ir_lowering" "builtin_call_info"
require_fixed_string_count \
  "$ir_builtin_call_info_lowering" \
  'pub(super) fn standard_builtin_call_info(' \
  1 \
  'builtin call-result analysis owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn standard_builtin_call_info(' \
  0 \
  'builtin call-result analysis outside child module'
check_no_inline_legacy_includes "$ir_builtin_call_info_lowering"
# Promise-specific catalog bypasses are a closed policy owned outside the
# exhaustive result table. The caller consumes one policy for executor
# observation and caller-flow invalidation; ad hoc booleans cannot regrow.
ir_promise_caller_flow_lowering="crates/scriptw-ir/src/lowering/promise_caller_flow.rs"
require_file "$ir_promise_caller_flow_lowering"
require_module_decl "$ir_lowering" "promise_caller_flow"
require_tree_regex_count \
  crates/scriptw-ir/src \
  '^[[:space:]]*pub\(super\)[[:space:]]+enum[[:space:]]+PromiseInvocationPolicy[[:space:]]*\{' \
  1 \
  'Promise invocation-policy owner'
require_fixed_string_count \
  "$ir_builtin_call_info_lowering" \
  'PromiseInvocationPolicy::for_call(builtin, args, &context)' \
  1 \
  'Promise invocation-policy consumption'
for retired_promise_effect_boolean in \
  promise_constructor_may_invoke_executor \
  promise_builtin_cannot_call_user_code
do
  require_tree_regex_count \
    crates/scriptw-ir/src \
    "$retired_promise_effect_boolean" \
    0 \
    'retired ad hoc Promise invocation booleans'
done
check_no_inline_legacy_includes "$ir_promise_caller_flow_lowering"
check_raw_line_budget "$ir_promise_caller_flow_lowering" 70
# Invocation-effect accounting is a linear proof owned outside the exhaustive
# result table. There is one private module, one raw unattached-proof producer,
# and no compatibility re-export through the former owner.
ir_invocation_effects_lowering="crates/scriptw-ir/src/lowering/invocation_effects.rs"
require_file "$ir_invocation_effects_lowering"
require_exact_line_count \
  "$ir_lowering" \
  'mod invocation_effects;' \
  1 \
  'private invocation-effects module declaration'
require_fixed_string_count \
  "$ir_lowering" \
  'pub use invocation_effects::' \
  0 \
  'public invocation-effects compatibility re-exports'
for invocation_effects_consumer in "$ir_lowering" "$ir_builtin_call_info_lowering"; do
  require_regex_count \
    "$invocation_effects_consumer" \
    '^[[:space:]]*pub(\([^)]*\))?[[:space:]]+use[[:space:]]+([^;]*::)?invocation_effects(::|[[:space:]]*;)' \
    0 \
    'visibility-qualified invocation-effects compatibility re-exports'
done
require_tree_regex_count \
  crates/scriptw-ir/src \
  '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?struct[[:space:]]+AccountedInvocationEffects[[:space:]]*\{' \
  1 \
  'AccountedInvocationEffects owner'
require_tree_regex_count \
  crates/scriptw-ir/src \
  '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?enum[[:space:]]+StandardBuiltinCallAnalysis[[:space:]]*\{' \
  1 \
  'StandardBuiltinCallAnalysis owner'
require_tree_regex_count \
  crates/scriptw-ir/src \
  '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?enum[[:space:]]+AnalyzedInvocationEffects[[:space:]]*\{' \
  1 \
  'AnalyzedInvocationEffects owner'
require_tree_regex_count \
  crates/scriptw-ir/src \
  '^[[:space:]]*pub\(super\)[[:space:]]+struct[[:space:]]+InvocationCallerFlowEffects\(InvocationCallerFlowState\);' \
  1 \
  'opaque invocation caller-flow owner'
require_regex_count \
  "$ir_builtin_call_info_lowering" \
  '^[[:space:]]*(pub\([^)]*\)[[:space:]]+)?(struct|enum)[[:space:]]+(AccountedInvocationEffects|StandardBuiltinCallAnalysis|AnalyzedInvocationEffects)' \
  0 \
  'invocation-effects declarations in the former owner'
require_fixed_string_count \
  "$ir_invocation_effects_lowering" \
  'attached_to_emitted_call: false,' \
  1 \
  'raw unattached invocation-effects proof producers'
require_fixed_string_count \
  "$ir_invocation_effects_lowering" \
  'pub(super) fn recorded() -> Self {' \
  1 \
  'canonical invocation-effects proof constructor'
require_fixed_string_count \
  "$ir_invocation_effects_lowering" \
  'impl Drop for AccountedInvocationEffects {' \
  1 \
  'unconsumed invocation-effects rejection boundary'
if grep -Eq '#\[derive\([^]]*(Clone|Copy)' "$ir_invocation_effects_lowering" \
  || grep -Eq 'impl[[:space:]]+(Clone|Copy)[[:space:]]+for[[:space:]]+AccountedInvocationEffects' "$ir_invocation_effects_lowering"; then
  fail "$ir_invocation_effects_lowering must keep AccountedInvocationEffects nonduplicable"
fi
check_no_inline_legacy_includes "$ir_invocation_effects_lowering"
# Measured after TypedArray.fill, Float16Array, Intl.Locale getter and
# likely-subtag entries, legacy accessor definers, six Instant methods
# (including toLocaleString), five ZonedDateTime methods
# (toJSON/valueOf/toLocaleString/toPlainTime/withPlainTime), three
# `Temporal.Now` members (plainDateTimeISO/plainDateISO/plainTimeISO) and
# `Instant.prototype.toZonedDateTimeISO`: 2,292 raw lines.
# This exhaustive result table must not acquire unrelated lowering.
# Nine DisplayNames/RelativeTimeFormat registrations add eight raw lines to
# the reviewed Collator/enumeration surface; optional DisplayNames.of has no
# fixed String result arm.
# Seven Segmenter entries add five formatted raw lines; containing retains
# a conservative optional-object result. Exact source census: 2,324.
# Locale.getWeekInfo adds a three-line Object result arm; getTextInfo shares
# that arm with one further exhaustive variant. HourCycles joins the Array
# result arm with one variant. Measured result table: 2,334.
# Calendar/Collation Array entries and precise TimeZones Array|Undefined add nine lines.
# Moved 2026-11-19 to the measured size after cargo fmt: a deliberate change
# overran it by at most 5%. Larger overruns stay red for their owners to split.
check_raw_line_budget "$ir_builtin_call_info_lowering" 2424
# Measured after adding the opaque source/host caller-flow aggregate: 192 raw
# lines. This owner must remain a bounded lifecycle, not become a second
# call-analysis implementation store.
check_raw_line_budget "$ir_invocation_effects_lowering" 210
# Argument evaluation carries one must-consume authority. Raw predecessor
# snapshot variants remain private to its consumption methods so call owners
# cannot partially apply the invalidation result.
require_tree_regex_count \
  crates/scriptw-ir/src \
  '^[[:space:]]*struct[[:space:]]+LoweredCallArguments[[:space:]]*\{' \
  1 \
  'lowered-call-argument authority owner'
require_exact_line_count \
  "$ir_lowering" \
  '#[must_use = "lowered call arguments must account for values captured before their evaluation"]' \
  1 \
  'lowered-call-argument must-use contract'
for predecessor_snapshot_variant in \
  'PreArgumentHeapShapeSnapshots::NoHeapShapes 1' \
  'PreArgumentHeapShapeSnapshots::OneHeapShape 2' \
  'PreArgumentHeapShapeSnapshots::TwoHeapShapes 2'
do
  set -- $predecessor_snapshot_variant
  require_fixed_string_count \
    "$ir_lowering" \
    "$1" \
    "$2" \
    'private predecessor snapshot consumption'
done
# Source-call caller-flow preservation is admitted only by an exhaustive walk
# over the finalized parameters and IR body. The public-to-lowering carrier
# hides its state; only the nonduplicable proof token can mint the proven-safe
# state.
ir_source_call_flow_proof="crates/scriptw-ir/src/source_call_flow_proof.rs"
require_file "$ir_source_call_flow_proof"
require_exact_line_count \
  "$ir_lib" \
  'mod source_call_flow_proof;' \
  1 \
  'private source-call flow-proof module declaration'
require_tree_regex_count \
  crates/scriptw-ir/src \
  '^[[:space:]]*pub\(crate\)[[:space:]]+struct[[:space:]]+SourceCallFlowEffects\(SourceCallFlowState\);' \
  1 \
  'opaque source-call flow state owner'
require_tree_regex_count \
  crates/scriptw-ir/src \
  '^[[:space:]]*pub\(crate\)[[:space:]]+struct[[:space:]]+ProvenNoCallerFlowInvalidation[[:space:]]*\{' \
  1 \
  'caller-flow proof token owner'
source_call_flow_proof_token="$(sed -n '/#\[must_use = "caller-flow preservation must be consumed by source-call admission"\]/,/^}/p' "$ir_source_call_flow_proof")"
require_text_regex_count \
  "$source_call_flow_proof_token" \
  '^#\[must_use = "caller-flow preservation must be consumed by source-call admission"\]$' \
  1 \
  'caller-flow proof consumption obligation'
require_text_regex_count \
  "$source_call_flow_proof_token" \
  '^[[:space:]]*_private: \(\),$' \
  1 \
  'private caller-flow proof constructor field'
if grep -Eq '#\[derive\([^]]*(Clone|Copy)' <<<"$source_call_flow_proof_token" \
  || grep -Eq 'impl[[:space:]]+(Clone|Copy)[[:space:]]+for[[:space:]]+ProvenNoCallerFlowInvalidation' "$ir_source_call_flow_proof"; then
  fail "$ir_source_call_flow_proof must keep ProvenNoCallerFlowInvalidation nonduplicable"
fi
require_fixed_string_count \
  "$ir_source_call_flow_proof" \
  'Self(SourceCallFlowState::ProvenNoFlowInvalidation)' \
  2 \
  'proof-preserving source-call state constructors'
require_fixed_string_count \
  "$ir_source_call_flow_proof" \
  'Some(proof) => Self::from_proof(proof),' \
  1 \
  'finalized-invocation proof admission'
require_fixed_string_count \
  "$ir_source_call_flow_proof" \
  'pub(crate) fn for_finalized_invocation(params: &[FunctionParamIr], body: &BlockIr) -> Self {' \
  1 \
  'parameter-and-body source-call proof boundary'
require_fixed_string_count \
  "$ir_source_call_flow_proof" \
  'for_finalized_body' \
  0 \
  'body-only source-call proof admission'
# Compare the actual closed enum/catalog names, not a historical variant count.
# The operation macro's one row list also generates its exhaustive enum.
for source_call_flow_variant_domain in StatementIr ExprIr SpecOperationIr; do
  case "$source_call_flow_variant_domain" in
    StatementIr|ExprIr)
      source_call_flow_domain_code="$(braced_rust_item_source crates/scriptw-ir/src/ir.rs "^pub enum ${source_call_flow_variant_domain}[[:space:]]*[{]")"
      expected_source_call_flow_variants="$(printf '%s\n' "$source_call_flow_domain_code" \
        | sed -nE 's/^    ([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*([(,{].*)$/\1/p' | sort -u)"
      ;;
    SpecOperationIr)
      source_call_flow_domain_code="$(braced_rust_item_source crates/scriptw-ir/src/operations.rs '^spec_operations![[:space:]]*[{]')"
      expected_source_call_flow_variants="$(printf '%s\n' "$source_call_flow_domain_code" \
        | sed -nE 's/^    ([A-Za-z_][A-Za-z0-9_]*)(\([^)]*\))?[[:space:]]*=>[[:space:]]*\{.*$/\1/p' | sort -u)"
      ;;
  esac
  observed_source_call_flow_variants="$({
    grep -oE "${source_call_flow_variant_domain}::[A-Za-z0-9_]+" "$ir_source_call_flow_proof" || true
  } | sed "s/^${source_call_flow_variant_domain}:://" | sort -u)"
  if [ -z "$expected_source_call_flow_variants" ] \
    || [ "$observed_source_call_flow_variants" != "$expected_source_call_flow_variants" ]; then
    fail "$ir_source_call_flow_proof must exhaust the actual ${source_call_flow_variant_domain} enum/catalog without missing or retired variants"
  fi
done
if grep -Eq '(^|[|,(])[[:space:]]*_[[:space:]]*=>' "$ir_source_call_flow_proof" \
  || grep -Eq '\{[[:space:]]*\.\.[[:space:]]*\}' "$ir_source_call_flow_proof"; then
  fail "$ir_source_call_flow_proof must not hide new IR variants behind catch-all patterns"
fi
check_no_inline_legacy_includes "$ir_source_call_flow_proof"
# The owner includes parameter-default execution and all complete protocol
# variants; the bound remains for this exhaustive proof and its focused tests.
check_raw_line_budget "$ir_source_call_flow_proof" 890
# T02's class-definition boundary keeps the complete element planning,
# generated-function scheduling and typed ClassDefinitionIr construction in
# one child module. The parent retains only declaration/expression
# orchestration and cannot regrow a second copy of the implementation.
ir_class_definition_lowering="crates/scriptw-ir/src/lowering/class_definition.rs"
require_file "$ir_class_definition_lowering"
require_module_decl "$ir_lowering" "class_definition"
require_fixed_string_count \
  "$ir_class_definition_lowering" \
  'pub(super) fn lower_class_common_in_name_scope(' \
  1 \
  'class-definition lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_class_common_in_name_scope(' \
  0 \
  'class-definition lowering body outside child module'
ir_class_callable_flow="crates/scriptw-ir/src/lowering/class_definition/callable_flow.rs"
require_file "$ir_class_callable_flow"
require_exact_line_count "$ir_class_definition_lowering" 'mod callable_flow;' 1 'private class callable-flow module'
require_fixed_string_count "$ir_class_definition_lowering" 'self.finalize_class_callable_flow(' 1 'class callable-flow finalization consumption'
require_regex_count "$ir_class_callable_flow" '^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+finalize_class_callable_flow\(' 1 'class callable-flow finalization owner'
require_fixed_string_count "$ir_class_definition_lowering" 'fn finalize_class_callable_flow(' 0 'class flow finalization outside child'
ir_class_field_names="crates/scriptw-ir/src/lowering/class_definition/field_names.rs"
require_file "$ir_class_field_names"
require_exact_line_count "$ir_class_definition_lowering" 'mod field_names;' 1 'private class field-name owner attachment'
require_exact_line_count "$ir_class_definition_lowering" 'use self::field_names::class_field_initializer_name;' 1 'class planner imports its field-name owner'
require_fixed_string_count "$ir_class_field_names" 'pub(super) fn class_field_initializer_name(' 1 'sole class field initializer naming owner'
require_fixed_string_count "$ir_class_definition_lowering" 'fn class_field_initializer_name(' 0 'no parent field-name implementation copy'
require_fixed_string_count "$ir_class_definition_lowering" 'class_field_initializer_name(' 6 'actual field and accessor naming consumers'
check_no_inline_legacy_includes "$ir_class_field_names"
check_raw_line_budget "$ir_class_field_names" 24
check_no_inline_legacy_includes "$ir_class_definition_lowering"
check_no_inline_legacy_includes "$ir_class_callable_flow"
# Class planning includes separate fresh and later callable private-field facts.
check_raw_line_budget "$ir_class_definition_lowering" 1503
check_raw_line_budget "$ir_class_callable_flow" 90
# T02's ordinary-function boundary keeps the nested-lowerer lifecycle,
# parameter/body lowering, capture transfer, signature updates and final
# FunctionIr assembly together. The parent owns the seven orchestration calls
# and shared helpers used by generated iterators, class methods and object
# methods, but cannot regrow a second ordinary-function implementation.
ir_function_definition_lowering="crates/scriptw-ir/src/lowering/function_definition.rs"
require_file "$ir_function_definition_lowering"
require_module_decl "$ir_lowering" "function_definition"
require_fixed_string_count \
  "$ir_function_definition_lowering" \
  'pub(super) fn lower_function(' \
  1 \
  'ordinary-function lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_function(' \
  0 \
  'ordinary-function lowering outside child module'
check_no_inline_legacy_includes "$ir_function_definition_lowering"
# Measured with the private ModuleActivation protocol: 789 raw lines. The
# margin is for this function lifecycle, not unrelated lowering.
check_raw_line_budget "$ir_function_definition_lowering" 810
# T02's try-statement boundary owns catch-parameter environment construction,
# resumable entry/exit planning and final TryCatch/TryFinally IR assembly. The
# catch and finally lifecycle records are named so their generator and async
# states cannot be transposed through positional tuple access.
ir_try_statement_lowering="crates/scriptw-ir/src/lowering/try_statement.rs"
require_file "$ir_try_statement_lowering"
require_module_decl "$ir_lowering" "try_statement"
require_fixed_string_count \
  "$ir_try_statement_lowering" \
  'pub(super) fn lower_try(' \
  1 \
  'try-statement lowering owner'
require_fixed_string_count \
  "$ir_lowering" \
  'fn lower_try(' \
  0 \
  'try-statement lowering outside child module'
if ! grep -q '^struct LoweredCatchClause {' "$ir_try_statement_lowering" \
  || ! grep -q '^struct LoweredFinallyClause {' "$ir_try_statement_lowering"; then
  fail "$ir_try_statement_lowering must carry named catch/finally lifecycle records"
fi
if sed '/^[[:space:]]*\/\//d' "$ir_try_statement_lowering" | grep -Eq '\.[0-9]+'; then
  fail "$ir_try_statement_lowering must not recover lifecycle state through tuple positions"
fi
check_no_inline_legacy_includes "$ir_try_statement_lowering"
# Complete catch-pattern prefixes retain the thrown value and original catch
# environment through suspension; catch/finally lifecycle remains one owner.
check_raw_line_budget "$ir_try_statement_lowering" 400
# T02's throw-inference boundary owns the recursive statement analysis and
# carried-PutValue wrapper; its private expression leaf owns operand/key
# analysis. Only block inference crosses the outer private boundary for
# try-statement lowering; shared value/type algebra retains its owners.
ir_throw_inference_lowering="crates/scriptw-ir/src/lowering/throw_inference.rs"
require_file "$ir_throw_inference_lowering"
ir_throw_expression_lowering="crates/scriptw-ir/src/lowering/throw_inference/expression.rs"
require_file "$ir_throw_expression_lowering"
require_exact_line_count "$ir_throw_inference_lowering" 'mod expression;' 1 'private expression throw-analysis module'
require_regex_count "$ir_throw_expression_lowering" '^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+infer_expr_operand_throw_info\(' 1 'private-child operand analysis boundary'
require_regex_count "$ir_throw_expression_lowering" '^[[:space:]]*fn[[:space:]]+infer_property_key_throw_info\(' 1 'private property-key analysis owner'
require_regex_count "$ir_throw_expression_lowering" '^[[:space:]]*fn[[:space:]]+merge_object_property_throw_info\(' 1 'private object-property analysis owner'
require_exact_line_count \
  "$ir_lowering" \
  'mod throw_inference;' \
  1 \
  'private throw-inference module declaration'
require_regex_count \
  "$ir_throw_inference_lowering" \
  '^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+infer_block_throw_info[[:space:]]*\(' \
  1 \
  'block throw-inference entry point'
for private_owner in \
  merge_optional_value_info \
  infer_statement_sequence_throw_info \
  infer_statement_throw_info \
  infer_expr_throw_info
do
  require_regex_count \
    "$ir_throw_inference_lowering" \
    "^[[:space:]]*fn[[:space:]]+${private_owner}[[:space:]]*[<(]" \
    1 \
    "private ${private_owner} owner"
done
for throw_owner in \
  merge_optional_value_info \
  infer_block_throw_info \
  infer_statement_sequence_throw_info \
  infer_statement_throw_info \
  infer_expr_throw_info \
  infer_expr_operand_throw_info \
  merge_object_property_throw_info \
  infer_property_key_throw_info
do
  require_regex_count \
    "$ir_lowering" \
    "^[[:space:]]*((pub(\\([^)]*\\))?|default|const|async|unsafe|extern|\"[^\"]*\")[[:space:]]+)*fn[[:space:]]+${throw_owner}[[:space:]]*[<(]" \
    0 \
    "${throw_owner} outside throw-inference child"
done
# Four parent-private methods plus its outer block entry and three child
# operand/object/key methods form the eight-function closure. Count qualified
# declarations too, so modifiers cannot evade the closed owner inventory.
require_regex_count \
  "$ir_throw_inference_lowering" \
  '^[[:space:]]*fn[[:space:]]+' \
  4 \
  'private throw-inference parent method'
require_regex_count \
  "$ir_throw_inference_lowering" \
  '^[[:space:]]*((pub(\([^)]*\))?|default|const|async|unsafe|extern|"[^"]*")[[:space:]]+)*fn[[:space:]]+(r#)?[[:alpha:]_][[:alnum:]_]*[[:space:]]*[<(]' \
  5 \
  'total throw-inference parent function declaration'
require_regex_count \
  "$ir_throw_inference_lowering" \
  '^[[:space:]]*pub(\([^)]*\))?[[:space:]]+' \
  1 \
  'Rust-visible item'
require_regex_count "$ir_throw_expression_lowering" '^[[:space:]]*fn[[:space:]]+' 2 'private throw-expression object/key methods'
require_regex_count "$ir_throw_expression_lowering" '^[[:space:]]*((pub(\([^)]*\))?|default|const|async|unsafe|extern|"[^"]*")[[:space:]]+)*fn[[:space:]]+(r#)?[[:alpha:]_][[:alnum:]_]*[[:space:]]*[<(]' 3 'total throw-expression function declarations'
require_regex_count "$ir_throw_expression_lowering" '^[[:space:]]*pub(\([^)]*\))?[[:space:]]+' 1 'inner expression-analysis visible entry'
require_fixed_string_count \
  "$ir_try_statement_lowering" \
  'infer_block_throw_info' \
  1 \
  'throw-inference identifier use'
require_fixed_string_count \
  "$ir_lowering" \
  'infer_block_throw_info' \
  0 \
  'throw-inference identifier use outside child module'
while IFS= read -r caller; do
  case "$caller" in
    "$ir_throw_inference_lowering"|"$ir_try_statement_lowering") continue ;;
  esac
  if grep -Fq 'infer_block_throw_info' "$caller"; then
    fail "unexpected infer_block_throw_info identifier use: $caller"
  fi
done < <(find crates/scriptw-ir/src/lowering -type f -name '*.rs' -print)
for shared_parent_owner in \
  resolve_single_function_target \
  object_like_kind_set
do
  require_regex_count \
    "$ir_lowering" \
    "^[[:space:]]*((pub(\\([^)]*\\))?|default|const|async|unsafe|extern|\"[^\"]*\")[[:space:]]+)*fn[[:space:]]+${shared_parent_owner}[[:space:]]*[<(]" \
    1 \
    "parent-owned shared ${shared_parent_owner} helper"
  require_regex_count \
    "$ir_throw_inference_lowering" \
    "^[[:space:]]*((pub(\\([^)]*\\))?|default|const|async|unsafe|extern|\"[^\"]*\")[[:space:]]+)*fn[[:space:]]+${shared_parent_owner}[[:space:]]*[<(]" \
    0 \
    "shared ${shared_parent_owner} helper copied into throw-inference owner"
done
require_tree_regex_count crates/scriptw-ir/src/lowering '^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+merge_value_infos[[:space:]]*\(' 1 'sole shared value-fact merge owner'
require_fixed_string_count crates/scriptw-ir/src/lowering/conditional_flow.rs 'pub(super) fn merge_value_infos(' 1 'conditional-flow owns the shared value algebra'
require_regex_count "$ir_lowering" '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+merge_value_infos[[:space:]]*\(' 0 'no parent value-fact merge copy'
require_regex_count "$ir_throw_inference_lowering" '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+merge_value_infos[[:space:]]*\(' 0 'no throw-analysis value-fact merge copy'
require_regex_count \
  "$ir_lowering" \
  '^[[:space:]]*((pub(\([^)]*\))?|default|const|async|unsafe|extern|"[^"]*")[[:space:]]+)*fn[[:space:]]+unknown_runtime_value_info[[:space:]]*[<(]' \
  1 \
  'parent-owned unknown-runtime helper'
require_regex_count \
  "$ir_throw_inference_lowering" \
  '^[[:space:]]*((pub(\([^)]*\))?|default|const|async|unsafe|extern|"[^"]*")[[:space:]]+)*fn[[:space:]]+unknown_runtime_value_info[[:space:]]*[<(]' \
  0 \
  'unknown-runtime helper copied into throw-inference owner'
for shared_owner_spec in \
  'crates/scriptw-ir/src/lowering/builtin_shapes.rs:standard_error_instance_info' \
  'crates/scriptw-ir/src/reference.rs:carried_put_value_failure'
do
  shared_owner_file="${shared_owner_spec%%:*}"
  shared_owner_name="${shared_owner_spec##*:}"
  require_regex_count \
    "$shared_owner_file" \
    "^[[:space:]]*((pub(\\([^)]*\\))?|default|const|async|unsafe|extern|\"[^\"]*\")[[:space:]]+)*fn[[:space:]]+${shared_owner_name}[[:space:]]*[<(]" \
    1 \
    "shared ${shared_owner_name} helper owner"
  require_regex_count \
    "$ir_throw_inference_lowering" \
    "^[[:space:]]*((pub(\\([^)]*\\))?|default|const|async|unsafe|extern|\"[^\"]*\")[[:space:]]+)*fn[[:space:]]+${shared_owner_name}[[:space:]]*[<(]" \
    0 \
    "shared ${shared_owner_name} helper copied into throw-inference owner"
done
require_regex_count \
  "crates/scriptw-ir/src/reference.rs" \
  '^[[:space:]]*pub[[:space:]]+enum[[:space:]]+PutValueFailure([[:space:]]|<|\{|\(|=|;|:)' \
  1 \
  'closed PutValueFailure owner'
for throw_closed_owner in "$ir_throw_inference_lowering" "$ir_throw_expression_lowering"; do
require_regex_count \
  "$throw_closed_owner" \
  '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?((unsafe|auto)[[:space:]]+)*(struct|enum|union|type|trait)[[:space:]]+(r#)?[[:alpha:]_][[:alnum:]_]*([[:space:]]|<|\{|\(|=|;|:)' \
  0 \
  'local type or trait declaration'
# This owner needs no textual runtime data. Keeping non-line-comment block
# comments and string literals out also makes the executable shape checks below
# immune to code-looking text in /* ... */ or raw/multiline strings.
throw_non_line_comment_source="$(sed '/^[[:space:]]*\/\//d' "$throw_closed_owner")"
if printf '%s\n' "$throw_non_line_comment_source" | grep -Eq '/\*|\*/|"'; then
  fail "$throw_closed_owner must express throw analysis through typed values, not block comments or string literals"
fi
# A catch-all silently converts the closed IR/type algebra back into an open
# domain. Inspect each match-arm prefix, including inline and parenthesized
# arms, and every top-level or-pattern alternative. Guarded bindings are not
# catch-alls; `true` and `false` are exhaustive values rather than bindings.
throw_catch_all_arms="$(awk '
  function trim(value) {
    sub(/^[[:space:]]+/, "", value)
    sub(/[[:space:]]+$/, "", value)
    return value
  }
  function unparen(atom) {
    atom = trim(atom)
    while (atom ~ /^\(.*\)$/) {
      atom = trim(substr(atom, 2, length(atom) - 2))
    }
    return atom
  }
  function is_binding(atom) {
    atom = unparen(atom)
    if (atom == "true" || atom == "false") {
      return 0
    }
    return atom ~ /^(&[[:space:]]*(mut[[:space:]]+)?)?((ref[[:space:]]+mut|ref|mut)[[:space:]]+)?(r#)?[a-z][[:alnum:]_]*$/
  }
  function is_simple_catch_all(atom) {
    atom = unparen(atom)
    if (atom == "_" || atom == "..") {
      return 1
    }
    return is_binding(atom)
  }
  function is_catch_all(atom, inner, parts, count, part_index, at, binding, pattern) {
    atom = trim(atom)
    if (atom ~ /^\(.*\)$/) {
      inner = trim(substr(atom, 2, length(atom) - 2))
      if (inner ~ /,/) {
        count = split(inner, parts, /,/)
        for (part_index = 1; part_index <= count; part_index += 1) {
          if (!is_simple_catch_all(parts[part_index])) {
            return 0
          }
        }
        return count > 1
      }
      atom = inner
    }
    if ((at = index(atom, "@")) != 0) {
      binding = trim(substr(atom, 1, at - 1))
      pattern = unparen(substr(atom, at + 1))
      if (!is_binding(binding)) {
        return 0
      }
      count = split(pattern, parts, /\|/)
      for (part_index = 1; part_index <= count; part_index += 1) {
        if (is_simple_catch_all(parts[part_index])) {
          return 1
        }
      }
      return 0
    }
    count = split(atom, parts, /\|/)
    for (part_index = 1; part_index <= count; part_index += 1) {
      if (is_simple_catch_all(parts[part_index])) {
        return 1
      }
    }
    return 0
  }
  {
    rest = $0
    sub(/[[:space:]]*\/\/.*$/, "", rest)
    while ((arrow = index(rest, "=>")) != 0) {
      prefix = substr(rest, 1, arrow - 1)
      start = 0
      paren_depth = 0
      bracket_depth = 0
      brace_depth = 0
      for (i = length(prefix); i >= 1; i -= 1) {
        delimiter = substr(prefix, i, 1)
        if (delimiter == ")") {
          paren_depth += 1
        } else if (delimiter == "(" && paren_depth > 0) {
          paren_depth -= 1
        } else if (delimiter == "]") {
          bracket_depth += 1
        } else if (delimiter == "[" && bracket_depth > 0) {
          bracket_depth -= 1
        } else if (delimiter == "}") {
          brace_depth += 1
        } else if (delimiter == "{" && brace_depth > 0) {
          brace_depth -= 1
        } else if (paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 \
            && (delimiter == "{" || delimiter == ",")) {
          start = i
          break
        }
      }
      arm = trim(substr(prefix, start + 1))
      if (arm !~ /[[:space:]]if[[:space:]]/ && is_catch_all(arm)) {
        print NR ":" arm
      }
      rest = substr(rest, arrow + 2)
    }
  }
' "$throw_closed_owner")"
if [ -n "$throw_catch_all_arms" ]; then
  fail "$throw_closed_owner must keep every match exhaustive without catch-all arms"
fi
require_fixed_string_count \
  "$throw_closed_owner" \
  'unreachable!' \
  0 \
  'unreachable escape hatch'
require_fixed_string_count \
  "$throw_closed_owner" \
  'macro_rules!' \
  0 \
  'local macro definition'
require_regex_count \
  "$throw_closed_owner" \
  '^(    )?(::[[:space:]]*)?((r#)?[[:alpha:]_][[:alnum:]_]*[[:space:]]*::[[:space:]]*)*(r#)?[[:alpha:]_][[:alnum:]_]*[[:space:]]*!' \
  0 \
  'module-or-impl-level generated helper invocation'
done
require_regex_count \
  "$ir_throw_inference_lowering" \
  '^[[:space:]]*let[[:space:]]+strict_put_value_throw[[:space:]]*=[[:space:]]*match[[:space:]]+carried_put_value_failure\(&expr\.expr\)[[:space:]]*\{' \
  1 \
  'executable carried PutValue failure read'
require_regex_count \
  "$ir_throw_inference_lowering" \
  '^[[:space:]]*self\.infer_expr_operand_throw_info\(expr\),[[:space:]]*$' \
  1 \
  'executable wrapper-to-operand delegation'
require_regex_count \
  "$ir_throw_inference_lowering" \
  '^[[:space:]]*PutValueFailure::TypeErrorOnly[[:space:]]*=>[[:space:]]*type_error,[[:space:]]*$' \
  1 \
  'executable TypeError-only PutValue arm'
require_regex_count \
  "$ir_throw_inference_lowering" \
  '^[[:space:]]*PutValueFailure::TypeErrorOrReferenceError[[:space:]]*=>[[:space:]]*self\.merge_value_infos\([[:space:]]*$' \
  1 \
  'executable TypeError-or-ReferenceError PutValue arm'
require_regex_count \
  "$ir_throw_inference_lowering" \
  '^[[:space:]]*Some\(\(Strictness::Sloppy, _\)\)[[:space:]]*\|[[:space:]]*None[[:space:]]*=>[[:space:]]*None,[[:space:]]*$' \
  1 \
  'executable sloppy-or-absent PutValue arm'
check_no_inline_legacy_includes "$ir_throw_inference_lowering"
check_no_inline_legacy_includes "$ir_throw_expression_lowering"
# Actual closed statement owner529 includes whole Switch/Throw/Array paths;
# the exhaustive expression leaf retains its original550-line ceiling.
# Exhaustive inference includes complete loop/Switch/Array/resource runtime
# throws; its leaf also covers retained property/Super and object operands.
check_raw_line_budget "$ir_throw_inference_lowering" 660
check_raw_line_budget "$ir_throw_expression_lowering" 620
# Runtime JSON.parse owns parsing and the framed reviver walk. The retired
# static parser/protocol must not return as a separate compiler path.
for retired_json_path in \
  crates/scriptw-ir/src/lowering/static_json_parse.rs \
  crates/scriptw-aot-wasm/src/builtins/json/static_reviver.rs
do
  if [ -e "$retired_json_path" ]; then
    fail "retired static JSON compiler owner must remain absent: $retired_json_path"
  fi
done
for json_production_root in crates/scriptw-ir/src crates/scriptw-aot-wasm/src; do
  require_tree_regex_count \
    "$json_production_root" \
    'static_json_parse|static_reviver|JsonStaticParser|JsonStaticValueIr|JsonParseStaticReviver|PreparedStaticJsonParseReviver|prepare_static_json_parse_reviver|finish_static_json_parse_reviver' \
    0 \
    'retired static JSON compiler references'
done
ir_static_string_binding_facts_lowering="crates/scriptw-ir/src/lowering/static_string_binding_facts.rs"
require_file "$ir_static_string_binding_facts_lowering"
require_exact_line_count \
  "$ir_lowering" \
  'mod static_string_binding_facts;' \
  1 \
  'private static-string fact module declaration'
require_regex_count \
  "$ir_lowering" \
  '^(pub(\([^)]*\))?[[:space:]]+)?mod[[:space:]]+static_string_binding_facts;' \
  1 \
  'total static-string fact module declaration'
static_string_module_context="$(sed -n '/^mod statement;$/,+3p' "$ir_lowering")"
if [ "$static_string_module_context" != $'mod statement;\nmod static_literals;\nmod static_string_binding_facts;\nmod super_property_mutation;' ]; then
  fail "$ir_lowering must keep static_literals and static_string_binding_facts private between statement and super_property_mutation"
fi
# Static String facts are keyed by the binding storage identity, never the
# source spelling. The child-private raw map makes it impossible for the parent
# or a sibling lowerer to insert or query a fact without a BindingInfo proof.
require_exact_line_count \
  "$ir_lowering" \
  'use static_string_binding_facts::StaticStringBindingFacts;' \
  1 \
  'narrow static-string fact owner import'
require_exact_line_count \
  "$ir_lowering" \
  '    static_string_bindings: StaticStringBindingFacts,' \
  1 \
  'binding-owned lowerer static-string facts'
require_exact_line_count crates/scriptw-ir/src/lowering/conditional_flow.rs '    static_string_bindings: StaticStringBindingFacts,' 1 'conditional-flow snapshots retain the same binding-owned facts'
require_tree_regex_count \
  'crates/scriptw-ir/src' \
  '^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?struct[[:space:]]+StaticStringBindingFacts[[:space:]]*\{' \
  1 \
  'sole StaticStringBindingFacts owner'
require_exact_line_count \
  "$ir_static_string_binding_facts_lowering" \
  '    by_storage_name: BTreeMap<String, String>,' \
  1 \
  'child-private storage-identity fact map'
for static_string_fact_method in get contains_storage_name insert remove clear equal_intersection; do
  require_regex_count \
    "$ir_static_string_binding_facts_lowering" \
    "^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+${static_string_fact_method}[[:space:]]*[<(]" \
    1 \
    "StaticStringBindingFacts::${static_string_fact_method} owner"
done
require_regex_count \
  "$ir_static_string_binding_facts_lowering" \
  '^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+' \
  6 \
  'closed static-string fact operation inventory'
require_exact_line_count \
  "$ir_static_string_binding_facts_lowering" \
  '    pub(super) fn get(&self, binding: &BindingInfo) -> Option<&String> {' \
  1 \
  'binding-owned static-string fact read'
require_exact_line_count \
  "$ir_static_string_binding_facts_lowering" \
  '    pub(super) fn insert(&mut self, binding: &BindingInfo, value: String) {' \
  1 \
  'binding-owned static-string fact write'
require_exact_line_count \
  "$ir_static_string_binding_facts_lowering" \
  '    pub(super) fn remove(&mut self, binding: &BindingInfo) {' \
  1 \
  'binding-owned static-string fact invalidation'
while IFS= read -r static_string_fact_consumer; do
  if [ "$static_string_fact_consumer" != "$ir_static_string_binding_facts_lowering" ] \
    && grep -Fq '.by_storage_name' "$static_string_fact_consumer"; then
    fail "raw static-string storage map escaped into $static_string_fact_consumer"
  fi
done < <(find crates/scriptw-ir/src -type f -name '*.rs' -print)
check_no_inline_legacy_includes "$ir_static_string_binding_facts_lowering"
# Measured after replacing source-spelling keys with binding-owned storage
# identities: 33 raw lines. The margin is for the fact lifecycle only.
check_raw_line_budget "$ir_static_string_binding_facts_lowering" 45
# Reviver callback observations are consumed by the ordinary JSON.parse call
# analysis even though static parsing has been retired.
for retained_owner in known_json_parse_reviver_targets observe_json_parse_reviver_targets; do
  require_regex_count \
    "$ir_lowering" \
    "^[[:space:]]*fn[[:space:]]+${retained_owner}[[:space:]]*[<(]" \
    1 \
    "parent-owned ${retained_owner} helper"
  require_tree_regex_count \
    'crates/scriptw-ir/src' \
    "^[[:space:]]*((pub(\\([^)]*\\))?|default|const|async|unsafe|extern|\"[^\"]*\")[[:space:]]+)*fn[[:space:]]+${retained_owner}[[:space:]]*[<(]" \
    1 \
    "${retained_owner} helper owner"
done
require_exact_line_count \
  "$ir_lowering" \
  '            let reviver_targets = self.known_json_parse_reviver_targets(&lowered_args);' \
  1 \
  'runtime JSON.parse target discovery'
require_exact_line_count \
  "$ir_lowering" \
  '            self.observe_json_parse_reviver_targets(reviver_targets, &helper_context_id);' \
  1 \
  'runtime JSON.parse target observation'
ir_static_literals_lowering="crates/scriptw-ir/src/lowering/static_literals.rs"
require_file "$ir_static_literals_lowering"
require_exact_line_count "$ir_lowering" 'mod static_literals;' 1 'private static-literal compilation module'
for static_literal_entry in static_regexp_compilation_for_pattern static_regexp_compilation_for_direct_call; do
  require_regex_count "$ir_static_literals_lowering" "^[[:space:]]*pub\\(super\\)[[:space:]]+fn[[:space:]]+${static_literal_entry}[[:space:]]*\\(" 1 "consumed static-literal boundary ${static_literal_entry}"
  require_regex_count "$ir_lowering" "^[[:space:]]*(pub\\([^)]*\\)[[:space:]]+)?fn[[:space:]]+${static_literal_entry}[[:space:]]*\\(" 0 "static-literal body outside child ${static_literal_entry}"
done
require_fixed_string_count "$ir_lowering" 'static_compilation: Self::static_regexp_compilation_for_pattern(&pattern, &flags),' 1 'RegExp literal compilation consumption'
require_fixed_string_count "$ir_call_expression_lowering" 'self.static_regexp_compilation_for_direct_call(' 1 'direct RegExp call compilation consumption'
check_no_inline_legacy_includes "$ir_static_literals_lowering"
# Both unchecked call-site folds and their sole helpers are retired. The two
# consumed static RegExp compilation functions retain their private owner.
check_raw_line_budget "$ir_static_literals_lowering" 90
# T15's two array-literal lowerers share one typed ArrayAccumulation seam. Keep
# the ordinary and staged-generator walkers together in their child module so
# the 32k-line orchestration boundary does not become the edit point again.
ir_array_literal_lowering="crates/scriptw-ir/src/lowering/array_literal.rs"
require_file "$ir_array_literal_lowering"
require_module_decl "$ir_lowering" "array_literal"
require_fixed_string_count "$ir_array_literal_lowering" 'fn lower_array_literal(' 1 'ordinary array-literal lowerer'
require_fixed_string_count "$ir_array_literal_lowering" 'fn lower_staged_generator_array_literal(' 1 'staged array-literal lowerer'
require_fixed_string_count "$ir_lowering" 'fn lower_array_literal(' 0 'array-literal lowerer outside child module'
require_fixed_string_count "$ir_lowering" 'fn lower_staged_generator_array_literal(' 0 'staged array-literal lowerer outside child module'

# T15's array-destructuring result/declaration semantics are a closed operation
# carried from five lowering contexts into the live semantic consumers. Generic IR
# visitors that only transport the field are deliberately outside this census.
rust_item_attributes_source() {
  source_file="$1"
  item_start_pattern="$2"
  awk -v item_start_pattern="$item_start_pattern" '
    {
      trimmed_line = $0
      sub(/^[[:space:]]*/, "", trimmed_line)
    }
    trimmed_line ~ /^#\[/ {
      attributes = attributes trimmed_line "\n"
      next
    }
    match(trimmed_line, item_start_pattern) == 1 {
      printf "%s", attributes
      exit
    }
    trimmed_line == "" || trimmed_line ~ /^\/\// { next }
    { attributes = "" }
  ' "$source_file"
}

rust_source_without_inline_tests() {
  awk '
    skipping_test_item {
      opening_line = $0
      closing_line = $0
      openings = gsub(/\{/, "", opening_line)
      closings = gsub(/\}/, "", closing_line)
      depth += openings - closings
      if (openings > 0) {
        body_started = 1
      }
      if ((body_started && depth == 0) || (!body_started && /;/)) {
        skipping_test_item = 0
        body_started = 0
        depth = 0
      }
      next
    }
    /^[[:space:]]*#\[test\][[:space:]]*$/ {
      skipping_test_item = 1
      next
    }
    { print }
  ' "$1"
}

require_active_wasm_cli_rust_test() {
  source_file="$1"
  function_name="$2"
  test_description="$3"
  item_start_pattern="^fn[[:space:]]+${function_name}[[:space:]]*[(]"
  require_regex_count \
    "$source_file" \
    "^[[:space:]]*fn[[:space:]]+${function_name}[[:space:]]*\\(" \
    1 \
    "${test_description} test owner"
  test_attributes="$(rust_item_attributes_source "$source_file" "$item_start_pattern")"
  require_text_regex_count "$test_attributes" '^#\[test\]$' 1 "${test_description} live test attribute"
  if grep -Eq '^#\[(cfg|cfg_attr|ignore)([^[:alnum:]_]|$)' <<<"$test_attributes"; then
    fail "${test_description} must not be disabled by an attached cfg/ignore attribute"
  fi
  test_source="$(braced_rust_item_source "$source_file" "$item_start_pattern")"
  require_text_regex_count "$test_source" '^[[:space:]]*\.arg\("run"\)$' 1 "${test_description} run command"
  require_text_regex_count "$test_source" '^[[:space:]]*\.arg\("--execution-backend"\)$' 1 "${test_description} backend option"
  require_text_regex_count "$test_source" '^[[:space:]]*\.arg\("wasm"\)$' 1 "${test_description} Wasm backend"
  require_text_regex_count "$test_source" '^[[:space:]]*output\.status\.success\(\),$' 1 "${test_description} process-status assertion"
  require_text_regex_count "$test_source" '^[[:space:]]*assert!\(stdout\.contains\("backend_used: WasmAot"\)\);$' 1 "${test_description} backend assertion"
  require_text_regex_count "$test_source" '^[[:space:]]*assert!\(stdout\.contains\("boolean\(true\)"\), "\{stdout\}"\);$' 1 "${test_description} boolean-result assertion"
}

array_destructuring_evaluation_match_source() {
  awk '
    !capturing {
      if (!match($0, /match[[:space:]]+\*?evaluation[[:space:]]*\{/)) {
        next
      }
      $0 = substr($0, RSTART)
      capturing = 1
    }
    {
      print
      opening_line = $0
      closing_line = $0
      openings = gsub(/\{/, "", opening_line)
      closings = gsub(/\}/, "", closing_line)
      depth += openings - closings
      if (depth == 0) {
        exit
      }
    }
  '
}

require_array_destructuring_producer() {
  source_file="$1"
  function_name="$2"
  evaluation_variant="$3"
  item_start_pattern="^(pub([(][^)]*[)])?[[:space:]]+)?fn[[:space:]]+${function_name}[[:space:]]*[(]"
  require_regex_count "$source_file" "^[[:space:]]*${item_start_pattern#^}" 1 "${function_name} producer declaration"
  producer_source="$(braced_rust_item_source "$source_file" "$item_start_pattern")"
  require_text_regex_count \
    "$producer_source" \
    'evaluation:[[:space:]]*ArrayDestructuringEvaluationIr::[[:alnum:]_]+,' \
    1 \
    "${function_name} array-destructuring evaluation producer"
  require_text_regex_count \
    "$producer_source" \
    "evaluation:[[:space:]]*ArrayDestructuringEvaluationIr::${evaluation_variant}," \
    1 \
    "${function_name} ${evaluation_variant} producer"
}

require_array_destructuring_consumer() {
  source_file="$1"
  function_name="$2"
  item_start_pattern="^(pub([(][^)]*[)])?[[:space:]]+)?fn[[:space:]]+${function_name}[[:space:]]*[(]"
  require_regex_count "$source_file" "^[[:space:]]*${item_start_pattern#^}" 1 "${function_name} semantic consumer declaration"
  consumer_source="$(braced_rust_item_source "$source_file" "$item_start_pattern")"
  require_text_regex_count \
    "$consumer_source" \
    'match[[:space:]]+\*?evaluation[[:space:]]*\{' \
    1 \
    "${function_name} array-destructuring evaluation match"
  evaluation_match="$(printf '%s\n' "$consumer_source" | array_destructuring_evaluation_match_source)"
  require_text_regex_count \
    "$evaluation_match" \
    '^[[:space:]]*ArrayDestructuringEvaluationIr::BindingInitialization[[:space:]]*=>' \
    1 \
    "${function_name} BindingInitialization match arm"
  require_text_regex_count \
    "$evaluation_match" \
    '^[[:space:]]*ArrayDestructuringEvaluationIr::AssignmentEvaluation[[:space:]]*=>' \
    1 \
    "${function_name} AssignmentEvaluation match arm"
  require_text_regex_count \
    "$evaluation_match" \
    '^[[:space:]]*_[[:space:]]*=>' \
    0 \
    "${function_name} array-destructuring wildcard arm"
  require_text_regex_count \
    "$consumer_source" \
    'ArrayDestructuringEvaluationIr::' \
    2 \
    "${function_name} direct evaluation-variant use"
}

ir_ir="crates/scriptw-ir/src/ir.rs"
require_fixed_string_count "$ir_ir" 'pub enum ArrayDestructuringEvaluationIr {' 1 'public array-destructuring evaluation enum'
array_destructuring_evaluation_derive="$(awk '/^pub enum ArrayDestructuringEvaluationIr \{$/ { print preceding_line; exit } { preceding_line = $0 }' "$ir_ir")"
if [ "$array_destructuring_evaluation_derive" != '#[derive(Debug, Clone, Copy, PartialEq, Eq)]' ]; then
  fail 'ArrayDestructuringEvaluationIr must keep its exact non-Default derives'
fi
array_destructuring_evaluation_enum="$(braced_rust_item_source "$ir_ir" '^pub[[:space:]]+enum[[:space:]]+ArrayDestructuringEvaluationIr[[:space:]]*[{]')"
array_destructuring_evaluation_enum_code="$(printf '%s\n' "$array_destructuring_evaluation_enum" | sed '/^[[:space:]]*\/\//d; /^[[:space:]]*$/d')"
require_text_regex_count "$array_destructuring_evaluation_enum_code" '^    BindingInitialization,$' 1 'BindingInitialization unit variant'
require_text_regex_count "$array_destructuring_evaluation_enum_code" '^    AssignmentEvaluation,$' 1 'AssignmentEvaluation unit variant'
if [ "$(printf '%s\n' "$array_destructuring_evaluation_enum_code" | wc -l | tr -d '[:space:]')" -ne 4 ]; then
  fail "$ir_ir must keep ArrayDestructuringEvaluationIr to its public declaration, two unit variants and closing brace"
fi
if grep -Eq 'bool|Default' <<<"$array_destructuring_evaluation_enum_code" \
  || grep -Eq 'impl[[:space:]]+Default[[:space:]]+for[[:space:]]+ArrayDestructuringEvaluationIr' "$ir_ir"; then
  fail 'ArrayDestructuringEvaluationIr must not regain a bool field or Default implementation'
fi

require_array_destructuring_producer "$ir_lowering" lower_parameter_binding_pattern BindingInitialization
require_array_destructuring_producer "$ir_lowering" lower_pattern_assign_value_unscoped AssignmentEvaluation
array_assignment_scope="$(braced_rust_item_source "$ir_lowering" '^fn lower_pattern_assign_value[(]')"
require_text_regex_count "$array_assignment_scope" 'self[.]lower_pattern_assign_value_unscoped[(]pattern, value[)]' 1 'scoped assignment delegates to its sole evaluation producer'
require_array_destructuring_producer "$ir_lowering" lower_pattern_lexical_binding BindingInitialization
require_array_destructuring_producer "$ir_lowering" lower_pattern_var_binding_from_value BindingInitialization
require_array_destructuring_producer "$ir_lowering" lower_pattern_lexical_binding_from_value_with_storage_names BindingInitialization
require_fixed_string_count "$ir_lowering" 'evaluation: ArrayDestructuringEvaluationIr::' 5 'reviewed array-destructuring evaluation producers'

require_array_destructuring_consumer "$ir_ir" validate_async_function_for_of_initialization
# This exhaustive walker belongs to cfg(test) support, not the product graph.
ir_array_test_support="crates/scriptw-ir/src/tests/support.rs"
require_array_destructuring_consumer "$ir_array_test_support" collect_binding_storage_names
require_array_destructuring_consumer crates/scriptw-aot-wasm/src/control_flow.rs initialize_direct_lexical_bindings
require_array_destructuring_consumer crates/scriptw-aot-wasm/src/control_flow/array_destructuring.rs compile_array_destructure_to_locals
require_array_destructuring_consumer crates/scriptw-aot-wasm/src/planning.rs collect_hoisted_vars_statement

array_destructuring_variant_product_files="$({
  while IFS= read -r product_source_file; do
    if rust_source_without_inline_tests "$product_source_file" \
      | grep -F 'ArrayDestructuringEvaluationIr::' >/dev/null; then
      printf '%s\n' "$product_source_file"
    fi
  done < <(find crates -type f -path '*/src/*.rs' ! -path 'crates/scriptw-ir/src/tests/*' -print | sort)
})"
expected_array_destructuring_variant_product_files='crates/scriptw-aot-wasm/src/control_flow.rs
crates/scriptw-aot-wasm/src/control_flow/array_destructuring.rs
crates/scriptw-aot-wasm/src/planning.rs
crates/scriptw-ir/src/generator_for_of_iterator/head.rs
crates/scriptw-ir/src/generator_loop_control/head_bindings.rs
crates/scriptw-ir/src/ir.rs
crates/scriptw-ir/src/lowering.rs'
if [ "$array_destructuring_variant_product_files" != "$expected_array_destructuring_variant_product_files" ]; then
  fail "direct ArrayDestructuringEvaluationIr variant use must stay in the reviewed semantic proof and emission owners: $array_destructuring_variant_product_files"
fi
for product_variant_spec in \
  'crates/scriptw-ir/src/lowering.rs|5' \
  'crates/scriptw-ir/src/ir.rs|2' \
  'crates/scriptw-aot-wasm/src/control_flow.rs|2' \
  'crates/scriptw-aot-wasm/src/control_flow/array_destructuring.rs|2' \
  'crates/scriptw-aot-wasm/src/planning.rs|2' \
  'crates/scriptw-ir/src/generator_for_of_iterator/head.rs|1' \
  'crates/scriptw-ir/src/generator_loop_control/head_bindings.rs|2'
do
  product_source_file="${product_variant_spec%%|*}"
  expected_variant_uses="${product_variant_spec#*|}"
  product_variant_uses="$({
    rust_source_without_inline_tests "$product_source_file" \
      | grep -Fc 'ArrayDestructuringEvaluationIr::'
  } || true)"
  if [ "$product_variant_uses" -ne "$expected_variant_uses" ]; then
    fail "$product_source_file must contain $expected_variant_uses non-test direct array-destructuring evaluation variant uses (found $product_variant_uses)"
  fi
done

array_destructuring_cli="crates/scriptw-cli/tests/cli/array.rs"
array_destructuring_fixture="crates/scriptw-cli/tests/fixtures/wasm_array_destructuring_iterators.js"
require_file "$array_destructuring_cli"
require_file "$array_destructuring_fixture"
require_active_wasm_cli_rust_test \
  "$array_destructuring_cli" \
  run_wasm_backend_uses_iterators_for_array_destructuring \
  'array-destructuring CLI regression'
array_destructuring_cli_test="$(braced_rust_item_source "$array_destructuring_cli" '^fn[[:space:]]+run_wasm_backend_uses_iterators_for_array_destructuring[[:space:]]*[(]')"
require_text_regex_count "$array_destructuring_cli_test" 'fixture_path\("wasm_array_destructuring_iterators\.js"\)' 1 'array-destructuring CLI fixture wiring'
check_no_inline_legacy_includes "$ir_lowering"
# Measured after signature/declaration extraction and the native module-source
# retrieval signature: 18,167 parent lines.
# Further implementation growth needs a reviewed owner rather than restoring the
# former 32k-line implementation store.
check_raw_line_budget "$ir_lowering" 18200

ir_module_graph_lowering="crates/scriptw-ir/src/lowering/module_graph.rs"
require_file "$ir_module_graph_lowering"
require_module_decl "$ir_lowering" "module_graph"
require_fixed_string_count "$ir_module_graph_lowering" 'fn lower_graph(' 1 'module graph lowering owner'
require_fixed_string_count "$ir_lowering" 'fn lower_graph(' 0 'module graph lowering outside child module'
ir_module_graph_session="crates/scriptw-ir/src/lowering/module_graph/session.rs"
require_file "$ir_module_graph_session"
require_exact_line_count "$ir_module_graph_lowering" 'mod session;' 1 'private retained module-graph session owner'
require_fixed_string_count "$ir_module_graph_session" 'fn lower_graph_with_admission(' 1 'sole retained-session graph admission owner'
require_fixed_string_count "$ir_module_graph_lowering" 'fn lower_graph_with_admission(' 0 'no parent retained-session admission copy'
require_fixed_string_count "$ir_module_graph_session" 'self.lower_graph_with_admission(' 2 'loaded and declared catalogs consume the same session admission owner'
require_exact_line_count "$ir_module_graph_session" '        lower_graph(' 1 'session consumes the original graph lowerer'
check_no_inline_legacy_includes "$ir_module_graph_session"
check_raw_line_budget "$ir_module_graph_session" 64
# Source-goal validation, linking and independent Script prelude orchestration.
# Complete catalog admission and its owned partition add eighteen lines.
check_raw_line_budget "$ir_module_graph_lowering" 218

# T02's StandardBuiltinId registry. One macro row owns declaration order,
# function-index order, global installation order and every metadata field.
# Keeping the invocation in a real child module preserves an ownership seam;
# `include!` would merely hide the same monolith from line counts.
ir_callable_to_string="crates/scriptw-ir/src/builtins/callable_to_string.rs"
require_file "$ir_callable_to_string"
require_exact_line_count \
  "$ir_builtins" \
  'mod callable_to_string;' \
  1 \
  'private callable-to-string module declaration'
require_pub_use \
  "$ir_builtins" \
  '^pub use callable_to_string::CallableToStringRepresentation;$' \
  'the callable-to-string representation'
require_tree_regex_count \
  crates/scriptw-ir/src \
  '^[[:space:]]*pub[[:space:]]+enum[[:space:]]+CallableToStringRepresentation[[:space:]]*\{' \
  1 \
  'CallableToStringRepresentation owner'
require_regex_count \
  "$ir_builtins" \
  '^[[:space:]]*pub[[:space:]]+enum[[:space:]]+CallableToStringRepresentation[[:space:]]*\{' \
  0 \
  'callable-to-string representations in the parent'
require_fixed_string_count \
  "$ir_callable_to_string" \
  'impl CallableToStringRepresentation {' \
  1 \
  'callable-to-string materializer owner'
require_tree_regex_count \
  crates/scriptw-ir/src \
  '^[[:space:]]*fn[[:space:]]+callable_to_string_representations_materialize_spec_shapes[[:space:]]*\(' \
  1 \
  'colocated callable-to-string behavior test'
require_fixed_string_count \
  "$ir_builtins" \
  'fn callable_to_string_representations_materialize_spec_shapes(' \
  0 \
  'callable-to-string behavior tests in the parent'
if grep -Eq '^[[:space:]]*_[[:space:]]*=>' "$ir_callable_to_string"; then
  fail "$ir_callable_to_string must materialize every representation exhaustively"
fi
check_no_inline_legacy_includes "$ir_callable_to_string"
# Measured after extraction: 38 raw lines. This child owns only the closed
# representation and its materializer/test.
check_raw_line_budget "$ir_callable_to_string" 50
ir_native_name="crates/scriptw-ir/src/builtins/native_name.rs"
require_file "$ir_native_name"
require_exact_line_count "$ir_builtins" 'mod native_name;' 1 'private builtin native-name validator module'
require_regex_count "$ir_native_name" '^pub\(super\) const fn is_catalog_native_function_name\(' 1 'closed catalog native-name validator owner'
require_fixed_string_count crates/scriptw-ir/src/builtins/catalog.rs 'super::native_name::is_catalog_native_function_name(name)' 1 'catalog native-name validation consumption'
require_fixed_string_count "$ir_callable_to_string" 'is_catalog_native_function_name' 0 'native-name validation outside its owner'
check_no_inline_legacy_includes "$ir_native_name"
# Existing const validation and matching unit control occupy83 raw lines.
check_raw_line_budget "$ir_native_name" 100
ir_builtin_catalog="crates/scriptw-ir/src/builtins/catalog.rs"
ir_builtin_catalog_contract_tests="crates/scriptw-ir/src/builtins/catalog_contract_tests.rs"
require_file "$ir_builtin_catalog"
require_file "$ir_builtin_catalog_contract_tests"
require_module_decl "$ir_builtins" "catalog"
require_exact_line_count \
  "$ir_builtins" \
  'mod catalog_contract_tests;' \
  1 \
  'test-only builtin catalog contract module declaration'
require_exact_line_count \
  "$ir_builtin_catalog_contract_tests" \
  'fn indexed_receiver_mutation_is_owned_by_the_builtin_catalog() {' \
  1 \
  'indexed-receiver mutation catalog contract'
require_pub_use "$ir_builtins" '^pub use catalog::StandardBuiltinId;' 'the standard builtin ID'
require_pub_use "$ir_builtins" '^pub use catalog::StandardBuiltinInstaller;' 'the standard builtin installer class'
check_no_inline_legacy_includes "$ir_builtins"
if ! grep -q '^macro_rules! standard_builtin_catalog' "$ir_builtins"; then
  fail "$ir_builtins must generate StandardBuiltinId from standard_builtin_catalog"
fi
if ! grep -q '^standard_builtin_catalog!' "$ir_builtin_catalog"; then
  fail "$ir_builtin_catalog must be the single standard builtin catalog invocation"
fi
if ! grep -q 'function: FunctionOrdinal(' "$ir_builtin_catalog" \
  || ! grep -q 'global: GlobalOrdinal(' "$ir_builtin_catalog" \
  || ! grep -q 'installer: None' "$ir_builtin_catalog"; then
  fail "$ir_builtin_catalog must encode dense function/global ordinals and mandatory installer classes"
fi
check_no_inline_legacy_includes "$ir_builtin_catalog_contract_tests"
check_raw_line_budget "$ir_builtin_catalog_contract_tests" 40
# T24's host-builtin surface registry. Identity, callable/global name, function
# id, exposure class and realm scope come from one row source; the machinery
# stays in builtins.rs while the rows live in a real child module.
ir_host_builtin_catalog="crates/scriptw-ir/src/builtins/host_catalog.rs"
require_file "$ir_host_builtin_catalog"
require_module_decl "$ir_builtins" "host_catalog"
require_pub_use "$ir_builtins" '^pub use host_catalog::HostBuiltinId;' 'the host builtin ID'
check_no_inline_legacy_includes "$ir_host_builtin_catalog"
if ! grep -q '^macro_rules! host_builtin_catalog' "$ir_builtins"; then
  fail "$ir_builtins must generate HostBuiltinId from host_builtin_catalog"
fi
if ! grep -q '^host_builtin_catalog!' "$ir_host_builtin_catalog"; then
  fail "$ir_host_builtin_catalog must be the single host builtin catalog invocation"
fi
host_builtin_catalog_rows="$(grep -Ec '^    [A-Za-z][A-Za-z0-9]* \{$' "$ir_host_builtin_catalog")"
# The async disposal bridge invokes the real synchronous disposer host entry.
if [[ "$host_builtin_catalog_rows" != "24" ]]; then
  fail "$ir_host_builtin_catalog must contain the reviewed 24-row host builtin catalog (found $host_builtin_catalog_rows)"
fi
require_fixed_string_count "$ir_host_builtin_catalog" '    AsyncDisposableStackSyncDispose {' 1 'real sync-disposer host bridge in the sole catalogue'
require_fixed_string_count crates/scriptw-ir/src/builtins/host_surface.rs 'HostBuiltinId::AsyncDisposableStackSyncDispose,' 1 'actual disposable host surface consumes the bridge'
require_fixed_string_count \
  "$ir_host_builtin_catalog" \
  'pub(crate) const fn may_invalidate_caller_flow(self) -> bool {' \
  1 \
  'host caller-flow classification owner'
require_fixed_string_count \
  "$ir_host_builtin_catalog" \
  'Self::CreateRealm => false,' \
  1 \
  'sole caller-flow-preserving host builtin'
host_caller_flow_classifier="$(sed -n '/pub(crate) const fn may_invalidate_caller_flow(self)/,/^    }/p' "$ir_host_builtin_catalog")"
if grep -Eq '(^|[|,(])[[:space:]]*_[[:space:]]*=>' <<<"$host_caller_flow_classifier"; then
  fail "$ir_host_builtin_catalog must exhaust host caller-flow effects without a catch-all"
fi
# Measured after the Float16Array and Intl.Locale identities: 1,773 raw lines.
# Metadata rows belong in their catalogs; shared machinery should shrink rather
# than regrow.
# The consumed SystemTimeZone capability accessor adds seven actual lines.
# The private native_name module declaration adds one orchestration line.
check_raw_line_budget "$ir_builtins" 1788
