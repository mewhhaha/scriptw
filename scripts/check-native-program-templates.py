#!/usr/bin/env python3
"""Native P transport ownership checks; semantic/compiler tests remain separate."""
from pathlib import Path
import re
import sys
A = 'crates/scriptw-aot-wasm/src/'
E = 'crates/scriptw-engine/src/'
T = A + 'native_program_template'
N = E + 'wasm_native_program_template'
M = E + 'memory_module_cache/native_templates'
CAPS = {
    A + 'module/compiled_module_package.rs': 430,
    A + 'objects/regexp_literal.rs': 120,
    A + 'objects/regexp_literal/native.rs': 140,
    'crates/scriptw-ir/src/runtime_direct_eval/regexp_literal.rs': 100,
    'crates/scriptw-ir/src/runtime_direct_eval/tests/regexp_literal.rs': 120,
    E + 'wasm_native_direct_eval_compiler/regexp_literal.rs': 150,
    'crates/scriptw-engine/tests/aot_native_program_templates/regexp_literals.rs': 110,
    T + '/empty_direct_eval.rs': 110,
    T + '/primitive_direct_eval.rs': 100,
    'crates/scriptw-ir/src/runtime_direct_eval/primitive_literal.rs': 100,
    'crates/scriptw-ir/src/runtime_direct_eval/tests/primitive_literal.rs': 140,
    E + 'wasm_native_direct_eval_compiler/primitive.rs': 110,
    'crates/scriptw-ir/src/runtime_direct_eval/empty.rs': 100,
    'crates/scriptw-ir/src/runtime_direct_eval/empty/metadata.rs': 90,
    E + 'wasm_native_direct_eval_compiler/empty.rs': 100,
    'crates/scriptw-engine/tests/aot_native_program_templates/function_names.rs': 90,
    'crates/scriptw-engine/tests/aot_native_program_templates/empty_eval.rs': 90,
    T + '/share_bodies.rs': 130, T + '/share_bodies/mapping.rs': 140,
    T + '/share_bodies/names.rs': 80, T + '/share_bodies/tests.rs': 180,
    T + '/tests/shared_emitted_bodies.rs': 100,
    'crates/scriptw-engine/tests/aot_native_program_templates/shared_function_bodies.rs': 100,
    A + 'program_literal_operands.rs': 180,
    T + '/literal_operands.rs': 50, T + '/plan/literal_operands.rs': 80,
    T + '/tests/literal_operands.rs': 260,
    A + 'native_program_contexts.rs': 150, A + 'gc_types/native_payload.rs': 100,
    T + '/contexts.rs': 200, T + '/contexts/signatures.rs': 180,
    T + '/contexts/facts.rs': 100,
    T + '/tests/emitted_names.rs': 100,
    T + '/payload.rs': 200, T + '/tests/program_owned_payload.rs': 150, T + '/error.rs': 30,
    T + '/plan/indices.rs': 120, T + '/reencode/declarations.rs': 160,
    T + '/reencode/payload.rs': 100,
    T + '.rs': 160, T + '/plan.rs': 260, T + '/plan/sites.rs': 120,
    T + '/references.rs': 110, T + '/reencode.rs': 200,
    T + '/reencode/names.rs': 80, T + '/tests.rs': 300,
    N + '/instance.rs': 120, N + '/initializer.rs': 80,
    N + '/bindings/allocation/payload.rs': 100, N + '/bindings/types.rs': 90,
    N + '/bindings/allocation/bulk.rs': 90,
    N + '/bindings/allocation/tests/bulk_reads.rs': 150,
    N + '/bindings/allocation/tests/payload_tests.rs': 120,
    M + '/instances.rs': 110, M + '/instances/tests.rs': 90,
    'crates/scriptw-engine/tests/aot_native_program_templates/program_owned_payload.rs': 130,
    N + '.rs': 90, N + '/cache.rs': 50, N + '/bindings.rs': 180,
    N + '/bindings/allocation.rs': 100, N + '/bindings/allocation/tests.rs': 160,
    M + '.rs': 70, M + '/tests.rs': 70,
    'crates/scriptw-engine/tests/aot_native_program_templates.rs': 110,
    'crates/scriptw-engine/tests/aot_native_program_templates/catalog_overlays.rs': 60,
}
KINDS = ('function', 'global_script', 'direct_eval')
FILES = tuple(CAPS) + (A + 'lib.rs', E + 'lib.rs', E + 'embedding.rs', E + 'wasm_native_source_compiler.rs',
    E + 'memory_module_cache.rs', 'crates/scriptw-ir/src/runtime_direct_eval.rs') + tuple(E + 'wasm_native_' + k + '_compiler/factory.rs' for k in KINDS)
def compact(text):
    return re.sub(r'\s+', '', re.sub(r'//[^\n]*', '', text))
def check(sources):
    failures = []
    def need(ok, key):
        if not ok: failures.append(key)
    c = {name: compact(text) for name, text in sources.items()}
    owner = c[T + '.rs'] + c[T + '/error.rs']
    need('pubstructNativeProgramTemplate{bytes:Arc<[u8]>,runtime:Arc<RuntimeArtifact>,images:Vec<NativeProgramDataImage>,factories:Vec<NativeProgramArrayFactory>,literal_operands:Vec<NativeProgramLiteralOperand>,payload:Option<NativeProgramPayload>,entry:TemplateEntry,}' in owner and
         'fnfrom_native(' in owner and 'pubfnfrom_native(' not in owner and
         all('pubfnfor_' + kind + '(artifact:&Runtime' + {'function':'Function','global_script':'GlobalScript','direct_eval':'DirectEval'}[kind] + 'Artifact' in owner for kind in KINDS), 'typed_artifact_only')
    need(sources[T + '/tests.rs'].splitlines()[0] == '#![cfg(test)]' and
         '#[cfg(test)]modtests;' in owner and sources[T + '/tests/literal_operands.rs'].splitlines()[0] == '#![cfg(test)]' and '[path="tests/literal_operands.rs"]modliteral_operand_tests;' in c[T + '/tests.rs'], 'exact_fixture_owner_is_statically_test_only')
    need('runtime:Arc::clone(runtime)' in owner and 'NativeProgramTemplateError(String)' in owner and
         'EmitError::unsupported' not in owner, 'opaque_owner_and_error_domain')
    plan = c[T + '/plan.rs'] + c[T + '/plan/indices.rs']
    need('#[cfg(debug_assertions)]' in plan and
         'wasmparser::Validator::new_with_features(features).validate_all(bytes)' in plan and
         'DataKind::Passive' in plan and 'returnOk(None)' in plan and
         all('Operator::' + op in plan for op in ('MemoryInit', 'DataDrop', 'ArrayInitData')) and
         'nativePmaynothaveastart' in plan and 'if!has_types||!has_imports' in plan, 'checked_fallback_and_no_start')
    need('!ty.composite_type.shared' in plan and 'StorageType::I8' in plan and 'StorageType::I16' in plan and
         'referenced.contains(&original)' in plan and 'self.retained_functions()+self.factories.len()asu32' in plan,
         'packed_types_and_function_map')
    sites = c[T + '/plan/sites.rs']
    need('Operator::ArrayNewData' in sites and 'Operator::ArrayNewFixed' in sites and
         'Operator::I32Const{value}' in sites and '(*valueasu16).to_le_bytes()' in sites and
         'self.sites.insert(*offset,Site{image,factory})' in sites and
         'self.suppressed.contains(position)' in sites, 'only_constant_operands_are_extracted')
    encoder = c[T + '/reencode.rs'] + c[T + '/reencode/declarations.rs'] + c[T + '/reencode/payload.rs'] + c[A + 'gc_types/native_payload.rs']
    need('fnfunction_index(' in encoder and 'fnglobal_index(' in encoder and
         'self.plan.function(index)' in encoder and 'index+self.plan.images.len()asu32' in encoder and
         'self.plan.imported_globals+site.image' in encoder and
         'self.plan.retained_functions()+site.factory' in encoder and
         'self.instruction(other)?' in encoder, 'all_original_instruction_indices_remain_mapped')
    need('types.ty().array(&wasm_encoder::StorageType::I8,false)' in encoder and
         'NativeProgramArrayConstruction::Slice=>vec![ValType::I32,ValType::I32,data]' in encoder and
         'Self::reference(factory.target_array_type)' in encoder and
         'mutable:false,shared:false' in encoder and
         'Instruction::GlobalGet' in encoder and 'Instruction::Call' in encoder,
         'immutable_images_and_typed_allocation_calls')
    references = c[T + '/references.rs']
    names = c[T + '/reencode/names.rs']
    need('self.0.insert(index)' in references and 'ElementKind::Declared' in references and
         'ElementItems::Functions' in references and
         all('Operator::' + op in references for op in ('Call', 'ReturnCall', 'RefFunc')) and
         all(section in references for section in ('TableSectionReader', 'GlobalSectionReader', 'ExportSectionReader', 'ElementSectionReader')) and
         'Name::Function(reader)' in names and 'Name::Local(reader)' in names and
         'Name::Label(reader)' in names and 'Name::Data(_)=>Ok(())' in names,
         'declarations_and_debug_names_follow_map')
    binder = c[N + '.rs'] + c[N + '/instance.rs']
    need('!WasmtimeEngine::same(engine,context.linker.engine())' in binder and
         '!Arc::ptr_eq(template.runtime(),context.runtime())' in binder and
         'Instance::new(&mut*store,&module,&imports)?' in binder and
         '.resolve_import(&mut*store,import.module(),import.name())' in binder and
         '!WasmtimeEngine::same(store.as_context().engine(),self.linker.engine())' in c[E + 'wasm_native_source_compiler.rs'] and
         binder.count('context.check_budget()?') >= 4, 'same_store_engine_runtime_and_budget')
    data = c[N + '/bindings.rs'] + c[N + '/bindings/types.rs']
    allocation = (c[N + '/bindings/allocation.rs'] + c[N + '/bindings/allocation/payload.rs'] +
                  c[N + '/bindings/allocation/bulk.rs'])
    need('global.mutability()!=Mutability::Const' in data and
         'array.mutability()!=Mutability::Const' in data and
         'array_reference(result)?' in data and 'FuncType::eq(ty,&function)' in data, 'actual_immutable_image_and_target_types')
    need('Arc::clone(context.execution())' in data and data.count('execution.remaining()') == 2 and
         'allocation::fresh(&mutcaller,inputs,outputs,&target,width,construction)?' in data and
         'cache.entries.len()<8' in data and 'Arc<[u8]>' not in data, 'allocator_cache_captures_no_source_data')
    need('ArrayRef::new_fixed(&mut*store,&pre,&values)' in allocation and
         'ArrayRef::new_from_i8_slice(&mut*store,&pre,&bytes)' in allocation and
         'u16::from_le_bytes([pair[0],pair[1]])' in allocation and
         '(*offsetasu32,Some(*lengthasu32),*image)' in allocation and
         'checked_sub(offset)' in allocation and 'wasmtime::Trap::ArrayOutOfBounds' in allocation and
         '*output=Val::AnyRef(Some(result.to_anyref()))' in allocation, 'fresh_allocations_exact_bits_and_bounds')
    cache = c[N + '/cache.rs']
    memory = c[M + '.rs']
    need('letmodules=memory_wasm_modules()' in cache and '.get_for_engine(&key,engine)' in cache and
         '.retain_compiled_for_engine(key,module)' in cache and
         'WasmModuleSource::Program(bytes),mode' in cache and
         'WasmtimeEngine::same(entry.module.engine(),engine)' in memory and
         '.checked_sub(removed.image_bytes)' in memory and 'self.retain_compiled(key,module)' in memory,
         'existing_aggregate_cache_and_actual_engine')
    for kind in KINDS:
        factory = c[E + 'wasm_native_' + kind + '_compiler/factory.rs']
        need('NativeProgramTemplate::for_' + kind + '(&artifact)' in factory and
             'crate::wasm_native_program_template::instantiate(&mut*store,engine,context,&template,trace,)?' in factory and
             '!Arc::ptr_eq(artifact.runtime(),context.runtime())' in factory and
             'context.validated_runtime()' in factory and 'EngineError::new(e.to_string())' in factory and
             'compile_wasm_module(' not in factory and factory.count('context.check_budget()?') >= 4,
             'actual_factory_route_' + kind)
    need('modnative_program_template;' in c[A + 'lib.rs'] and
         'modwasm_native_program_template;' in c[E + 'lib.rs'] and
         'modnative_templates;' in c[E + 'memory_module_cache.rs'] and
         'native_compilation_mode:WasmNativeCompilationMode' in c[E + 'wasm_native_source_compiler.rs'], 'private_owner_declarations')
    declarations = c[A + 'program_literal_operands.rs']
    scalar_sites = c[T + '/plan/literal_operands.rs']
    need('value:ProgramLiteralCount' in declarations and 'value:ProgramLiteralIndex' in declarations and
         'word(0)!=VERSION' in declarations and 'prior>=(body,operator)' in declarations and
         'Kind::Count' in declarations and 'Kind::Index' in declarations and
         'literaloperandrecordlengthmismatch' in declarations, 'closed_literal_operand_transport')
    need('Operator::I32Const{value}' in scalar_sites and '*value!=site.expected' in scalar_sites and
         'self.suppressed.contains(offset)' in scalar_sites and 'self.sites.contains_key(offset)' in scalar_sites and
         'site.operatorasusize' in scalar_sites and 'literaloperandbodyisoutofrange' in plan,
         'literal_relocations_check_exact_operators_and_disjoint_data_sites')
    need('section.name()==crate::program_literal_operands::SECTION_NAME' in encoder and
         'self.plan.images.len()asu32+self.plan.literal_operands.len()asu32' in encoder and
         'val_type:ValType::I32,mutable:false,shared:false' in encoder and
         'self.plan.literal_sites.get(&offset)' in encoder,
         'literal_operands_are_immutable_globals_and_marker_is_consumed')
    need(".rsplit('.')" in data and '.literal_operands().get(index)' in data and
         'operand.import_name()==import.name()' in data and
         '!matches!(global.content(),ValType::I32)' in data and 'Val::I32(operand.value())' in data,
         'fresh_exact_scalar_binding_with_constant_time_ordinal_lookup')
    need(sources[T + '/tests/program_owned_payload.rs'].splitlines()[0] == '#![cfg(test)]' and
         '[path="tests/program_owned_payload.rs"]modprogram_owned_payload;' in c[T + '/tests.rs'],
         'owned_payload_binary_fixture_is_statically_test_only')
    contexts = (c[T + '/contexts.rs'] + c[T + '/contexts/signatures.rs'] +
                c[T + '/contexts/facts.rs'])
    need('signatures.initializer(bootstrap_body,schema)?' in contexts and
         'self.signatures.local(index,body,*local,self.schema)?' in contexts and
         'if!facts.matches(schema)' in contexts and
         'Operator::LocalSet' in contexts and 'Operator::LocalTee' in contexts and
         'schema.payload_field' in contexts and 'StorageType::I8' in contexts and
         '!field.mutable' in contexts and 'ordinal>=*readyasusize' in contexts,
         'payload_owner_has_actual_typed_initialized_local_and_bootstrap_witness')
    need(all(fragment in plan for fragment in (
        'context_facts.record_type(result.types,ty);',
        'context_facts.import_function(ty);',
        'context_facts.define_function(ty.map_err(invalid)?);',
        'context_facts.exports(reader.clone())?;',
        'super::contexts::Check::new(rows,&context_facts,bodies.len())',
        'contexts.body(body_indexasu32,body,&operators,&result.sites,&result.literal_sites,)?;',
        '.map(super::contexts::Check::finish).transpose()?.flatten()')),
        'ownership_facts_share_the_actual_planning_pass')
    need('FunctionContext::Program{local,..}' in encoder and
         'native_payload::load(function,*local,contexts.schema)' in encoder and
         'field_index:schema.payload_field' in encoder and
         'Instruction::LocalGet(0)' in encoder and 'LocalGet(local_index+1)' in encoder and
         'self.plan.contexts.is_none()' in encoder,
         'owned_operands_use_actual_P_or_explicit_bootstrap_payload')
    store = c[M + '/instances.rs']
    need('entry.code.as_ref()==code' in store and 'Arc::ptr_eq(&entry.authority,authority)' in store and
         'WasmtimeEngine::same(entry.module.engine(),engine)' in store and
         'module_image_bytes(&module).and_then(|bytes|bytes.checked_add(code.len()))' in store and
         'self.limits.has_image_room(self.retained_bytes,bytes)' in store and
         '.native_template_instances.get(&key,template.bytes(),engine,&context.instance_authority)' in binder and
         'template.permits_instance_reuse()' in binder and
         '.is_some_and(|p|p.permits_instance_reuse())' in c[T + '/empty_direct_eval.rs'] and
         'native_template_instances:memory_module_cache::MemoryNativeInstanceCache' in c[E + 'embedding.rs'],
         'instance_reuse_is_store_engine_code_authority_and_original_budget_bound')
    need('fnfresh_from_payload<T>(' in allocation and 'directory.image_row(ordinal)' in allocation and
         'directory.image_span(&row)' in allocation and 'NativeProgramPayloadDirectory::from_footer' in allocation and
         'Arc::from' not in allocation and 'image_start.checked_add(offset)' in allocation and
         'image.copy_i8_range_to_slice(&mut*store,start,&mutbytes)?' in allocation and
         'length.checked_mul(width.bytes())' in allocation,
         'fresh_source_arrays_use_only_the_received_P_payload_image_bounds')
    ir = c['crates/scriptw-ir/src/runtime_direct_eval.rs']
    proof = c['crates/scriptw-ir/src/runtime_direct_eval/empty.rs']
    need('prepare_eval_source_utf16(' in ir and
         all(fragment in ir for fragment in (
             'ast.statements().statements().iter().all(|item|matchitem',
             'Statement::Empty=>true',
             'Statement::Block(block)=>block.statement_list().statements().is_empty()',
             'StatementListItem::Declaration(_)=>false')) and
         'lower_runtime_direct_eval_program(' in ir and 'if!self.empty_syntax' in proof and
         'unit.id!=self.script_id' in proof and 'unit.kind!=PreparedScriptKind::DirectEval' in proof and
         all(fragment in proof for fragment in ('!unit.body.statements.is_empty()',
             'unit.body.result_kind!=ValueKind::Undefined', 'unit.body.lexical_environment.is_some()',
             'unit.declarations!=RuntimeGlobalDeclarationPlan::default()', '!metadata::is_inert(unit)',
             '!unit.function_ids.is_empty()', '!script.functions.is_empty()',
             '!script.prepared_dynamic_functions.is_empty()', 'let[root]=script.prepared_scripts.as_slice()')),
         'empty_eval_requires_actual_syntax_and_lowered_no_effects_proof')
    metadata = c['crates/scriptw-ir/src/runtime_direct_eval/empty/metadata.rs']
    need('binding.name==DIRECT_EVAL_EXECUTION_CONTEXT_NAME' in metadata and
         'binding.slot==0' in metadata and 'EnvironmentBindingMutabilityIr::Immutable{strict:true}' in metadata and
         '!binding.declarations.is_declared()' in metadata and 'bindings.is_empty()' in metadata and
         'GlobalPropertyInitializerIr::SourceFunction(_)=>false' in metadata,
         'empty_eval_metadata_has_only_dead_intrinsics_and_exact_hidden_caller_capture')
    kernel = c[T + '/empty_direct_eval.rs']
    assembly = c[A + 'module/compiled_module_package.rs']
    entry = c[E + 'wasm_native_direct_eval_compiler/empty.rs']
    need("_proof:&EmptyRuntimeDirectEval<'_>" in kernel and
         'crate::module::empty_direct_eval_module(_proof,EXPORT)' in kernel and
         "_proof:&scriptw_ir::EmptyRuntimeDirectEval<'_>" in assembly and
         'WasmRuntimeValueTag::Undefined.tag()' in assembly and 'Instruction::RefNull(RefType::EQREF.heap_type)' in assembly and
         'CompletionKindIr::Normal.abi_code()' in assembly and 'entry:TemplateEntry::EmptyDirectEval' in kernel and
         'TemplateEntry::EmptyDirectEval|TemplateEntry::PrimitiveDirectEval=>true' in kernel and
         'crate::wasm_native_program_template::instantiate(&mut*store,engine,context,template,trace,)?' in entry and
         'context.empty_direct_eval_template.get_or_init(' in entry and
         'factory.call(&mut*store,&[],&mutoutput)?' in entry and
         'zip(&signature.completion)' in entry and entry.count('context.check_budget()?') >= 2,
         'empty_eval_executes_typed_stateless_wasm_with_original_authority_and_deadline')
    primitive = c['crates/scriptw-ir/src/runtime_direct_eval/primitive_literal.rs']
    primitive_kernel = c[T + '/primitive_direct_eval.rs']
    primitive_host = c[E + 'wasm_native_direct_eval_compiler/primitive.rs']
    need('letunit=self.inert_preparation()?' in primitive and
         'let[StatementIr::Expression(expression)]=unit.body.statements.as_slice()' in primitive and
         'expression.value_info()!=ValueInfo::new(kind)' in primitive and
         all('ExprIr::' + kind in primitive for kind in ('Undefined', 'Null', 'Boolean', 'Number')) and
         '_=>returnNone' in primitive and '_program:&' in primitive and
         'pubvalue:' not in primitive and 'pub_program:' not in primitive,
         'primitive_eval_requires_exact_scalar_IR_and_inert_preparation')
    need("proof:&scriptw_ir::PrimitiveRuntimeDirectEval<'_>" in primitive_kernel and
         'crate::module::primitive_direct_eval_module(proof,EXPORT)' in primitive_kernel and
         "_proof:&scriptw_ir::PrimitiveRuntimeDirectEval<'_>" in assembly and
         'stateless_direct_eval_module(export,DirectEvalCompletionOperands::Primitive)' in assembly and
         'DirectEvalCompletionOperands::Primitive=>&[ValType::I32,ValType::I64]' in assembly and
         'Instruction::LocalGet(0)' in assembly and 'Instruction::LocalGet(1)' in assembly and
         'context.primitive_direct_eval_template.get_or_init(' in primitive_host and
         'matchproof.value()' in primitive_host and 'Val::I64(bitsasi64)' in primitive_host and
         'factory.call(&mut*store,&arguments,&mutoutput)?' in primitive_host and
         'zip(&signature.completion)' in primitive_host and primitive_host.count('context.check_budget()?') >= 2,
         'primitive_eval_executes_real_stateless_wasm_with_current_operands_and_budget')
    regexp = c['crates/scriptw-ir/src/runtime_direct_eval/regexp_literal.rs']
    need('letunit=self.inert_preparation()?' in regexp and
         'let[StatementIr::Expression(expression)]=unit.body.statements.as_slice()' in regexp and
         'static_compilation:Some(StaticRegExpCompilation::Program(program))' in regexp and
         'ValidatedRegExpProgram::from_program(program).ok()?' in regexp,
         'regexp_eval_requires_single_lowered_literal_and_validated_program')
    allocate = c[A + 'objects/regexp_literal.rs']
    need('emit_alloc_plain_object_with_prototype(Some(&parameters.prototype)' in allocate and
         'ScalarValue::NumberBits(0)' in allocate and 'ScalarValue::Undefined' in allocate and
         'writable:true,enumerable:false,configurable:false' in allocate and
         'emit_ordinary_append_property_entry(&header,&key,&descriptor,' in allocate and
         'ObjectDefineData' not in allocate,
         'regexp_literal_installs_exact_lastindex_on_only_its_fresh_header')
    regexp_host = c[E + 'wasm_native_direct_eval_compiler/regexp_literal.rs']
    need('proof:&scriptw_ir::StaticRegExpRuntimeDirectEval' in regexp_host and
         'ArrayRef::new_fixed' in regexp_host and 'proof.compiled_bytes()' in regexp_host and
         'factory.call(&mut*store,&operands,&mutoutput)?' in regexp_host and
         regexp_host.count('context.check_budget()?') >= 3,
         'regexp_eval_executes_exact_R_wasm_with_fresh_raw_operands_and_original_budget')
    regexp_factory = c[A + 'objects/regexp_literal/native.rs']
    need('parameters.realm.load(schema,&mutfunction)' in regexp_factory and
         'Instruction::RefEq' in regexp_factory and
         'PreparedScriptStatus::Unmatched.code()' in regexp_factory and
         'request.operands[2]' in regexp_host and 'Val::I32(0)=>Ok(None)' in regexp_host and
         'StringConstruction::allocate(' in regexp_factory and
         'output.copy_units_from(' in regexp_factory and 'output.publish(schema,function)' in regexp_factory,
         'regexp_eval_checks_actual_realm_and_copies_raw_units_before_string_publication')
    for name, maximum in CAPS.items():
        need(len(sources[name].splitlines()) <= maximum, 'owner_cap:' + name)
    return failures

def main():
    root = Path(sys.argv[1]) if len(sys.argv) == 2 else Path.cwd()
    failures = check({name:(root/name).read_text() for name in FILES})
    for failure in failures: print('native program templates: ' + failure, file=sys.stderr)
    if not failures: print('native program template source controls passed')
    return bool(failures)
if __name__ == '__main__': raise SystemExit(main())
