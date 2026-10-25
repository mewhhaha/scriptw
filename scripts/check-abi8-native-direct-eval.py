#!/usr/bin/env python3
"""Source ownership and operand checks for the ABI8 native direct-eval seam."""
from pathlib import Path
import re
import sys

AOT = 'crates/scriptw-aot-wasm/src/'
FILES = tuple(AOT + name for name in (
    'lib.rs', 'runtime_artifact.rs', 'runtime_direct_eval.rs',
    'runtime_direct_eval/witness.rs', 'runtime_direct_eval/tests.rs',
    'gc_types/host.rs', 'gc_types/value/native_direct_eval_compiler.rs',
    'module/static_signature.rs', 'runtime_helpers.rs',
    'functions/direct_eval.rs', 'functions/native_direct_eval.rs',
    'functions/native_direct_eval_factory.rs', 'modules.rs',
    'emit/module_assembly/native_program.rs',
    'runtime_artifact/cache/tests.rs', 'runtime_artifact/build_package/tests.rs',
    'runtime_artifact/validated_profile.rs',
)) + ('crates/scriptw-ir/src/runtime_semantics.rs',)

def compact(text):
    text = re.sub(r'//[^\n]*', '', text)
    return re.sub(r'\s+', '', text)

def check(sources):
    c = {name: compact(text) for name, text in sources.items()}
    failures = []
    def require(ok, reason):
        if not ok:
            failures.append(reason)
    def a(name):
        return c[AOT + name]
    require((current_layout := re.search(r'constLAYOUT_VERSION:u32=(\d+);', a('runtime_artifact.rs'))) is not None and int(current_layout.group(1)) >= 15, 'current layout version retains direct eval, Realm intrinsics and program payload ownership with shared empty storage')
    host = a('gc_types/host.rs')
    require('CompileDirectEval=>("compile_direct_eval",HostCompileDirectEval),' in host and
            '|GcHostImport::CompileDirectEval=>{source_compilation_profile==crate::RuntimeSourceCompilationProfile::NativeHost}' in host,
            'dedicated NativeHost-only typed compiler import')
    signature = a('module/static_signature.rs')
    params = ('Gc(GcLayout::StringValue,NonNullable),Gc(GcLayout::ImmutableByteArray,NonNullable),'
              'Gc(GcLayout::RealmRecord,NonNullable),Gc(GcLayout::Environment,Nullable),'
              'Gc(GcLayout::Environment,Nullable),Gc(GcLayout::PrivateEnvironment,Nullable),'
              'I32,I64,EqRef,I32,I64,EqRef,Gc(GcLayout::DirectEvalExecutionContext,NonNullable),'
              'Gc(GcLayout::ProgramInstance,NonNullable)')
    results = 'I32,I64,EqRef,I32,I32,I32,Gc(GcLayout::StringValue,Nullable)'
    require('HostCompileDirectEval=>([' + params + '],[' + results + ']),' in signature,
            'exact request14 and completion/status/syntax7 signature')
    helper = a('runtime_helpers.rs')
    require('NativeDirectEvalFactory/NativeDirectEvalFactoryArguments/NativeDirectEvalFactoryParameters/"native_direct_eval_factory"{caller_program:Program,source:(RefStringValueNonNullable),descriptor:(RefImmutableByteArrayNonNullable),realm:(RefRealmRecordNonNullable),lexical:(RefEnvironmentNullable),variable:(RefEnvironmentNullable),private:(RefPrivateEnvironmentNullable),this_value:Value,new_target:Value,invocation:(RefDirectEvalExecutionContextNonNullable),calling_program:Program}=>StatusCompletion;' in helper,
            'factory15 preserves actual P2 separately from calling P1')
    require('Self::NativeDirectEvalFactory=>HelperOwner::Program,' in helper,
            'factory body belongs to actual P')
    native_program = a('emit/module_assembly/native_program.rs')
    require('DirectEval(&\'ascriptw_ir::RuntimeDirectEvalProgram)' in native_program and
            'crate::runtime_direct_eval::require_factory_plan(program,functions)?;' in native_program and
            'RuntimeHelperId::NativeDirectEvalFactory,' in native_program and
            'Some(NativeProgramFactory::DirectEval(program))' in native_program,
            'distinct witnessed native factory publication')
    route = native_program.split('fnemit_runtime_direct_eval_module(', 1)[1]
    require('ModuleKind::Program(validated.runtime())' in route and 'validated.selection()' in route and 'Some(validated)' in route and 'emit_script_module_with_factory(' in route and
            'emit_module(' not in route and 'ModuleKind::Runtime' not in route,
            'fresh direct P links supplied R without main/global fallback')
    direct = a('functions/direct_eval.rs')
    require('self.emit_native_direct_eval(context,realm,source,pending,function)?;' in direct and
            'emit_reject_dynamic_source' not in direct, 'actual direct intrinsic finite miss reaches native seam')
    request = a('functions/native_direct_eval.rs')
    for token in ('letdescriptor_bytes=context.runtime_descriptor().encode();',
                  'schema.array_type::<ImmutableByteArray>().fixed(',
                  'self.emit_capture_direct_eval_invocation(function)?;',
                  '.gc_host_import(GcHostImport::CompileDirectEval)',
                  'source,&descriptor,realm,&lexical,&variable,&private,invocation.this_value(),invocation.new_target(),&required,self.current_program(),output,status,schema,function,'):
        require(token in request, 'native request retains actual descriptor/invocation/operand roots: '+token)
    require('GcHostImport::CompileGlobalScript' not in request and 'RuntimeGlobalScriptKind' not in request,
            'direct caller cannot become global Script compilation')
    operands = a('gc_types/value/native_direct_eval_compiler.rs')
    order = ['source.load(schema,function);','descriptor.load(schema,function);','realm.load(schema,function);',
             'lexical.load(schema,function);','variable.load(schema,function);','private.load(schema,function);',
             'this_value.emit(function);','new_target.emit(function);','invocation.load(schema,function);',
             'calling_program.reference(schema,function);','self.emit_call_instruction(function);',
             '.initialize(GcStackReference::new(),function);','status.store(function);',
             'completion.store_call(GcCallResult::emitted(),function);']
    indices = [operands.find(token) for token in order]
    require(all(i >= 0 for i in indices) and indices == sorted(indices),
            'request order and syntax/status/completion root binding')
    factory = a('functions/native_direct_eval_factory.rs')
    require('root.filter(|root|matches!(&root.kind,PreparedScriptKind::DirectEval(_)))' in factory and
            factory.count('Instruction::Unreachable') == 2,
            'ordinary and deferred rows cannot become direct factory roots')
    require('PreparedScriptInputs::new(&parameters.lexical,&parameters.this_value,&parameters.new_target,&parameters.variable,&parameters.private,&invocation,self.current_program(),)' in factory and
            'self.replace_current_realm(&parameters.realm,&mutfunction);' in factory and
            'self.replace_current_realm(&saved_realm,&mutfunction);' in factory,
            'factory enters P2 with exact caller environment/context/values and restores Realm')
    artifact = a('runtime_direct_eval.rs')
    for token in ('caller:RuntimeDirectEvalCaller,','source:Arc<[u16]>,',
                  'caller:program.caller().clone(),','source:Arc::from(source),',
                  'validated.require_linked_program(&emitted,"nativedirectevaldidnotretainitsexactlinkedruntime",)?;',
                  'letvalidated=ValidatedRuntimeArtifact::new(Arc::clone(runtime),intl_profile.clone())?;'):
        require(token in artifact, 'complete cached witness and exact R/profile: '+token)
    proof = a('runtime_artifact/validated_profile.rs')
    require(('pub(crate)fnrequire_linked_program(&self,emitted:&super::EmittedModule,'
             "diagnostic:&'staticstr,)->Result<(),EmitError>{"
             'ifemitted.runtime_layout.is_some()||!emitted.artifact.runtime().is_some_and('
             '|actual|Arc::ptr_eq(actual,self.runtime())){'
             'returnErr(EmitError::unsupported(diagnostic));}Ok(())}') in proof,
            'complete cached witness and exact R/profile: shared linkage owner')
    require('letruntime=validated.runtime();' in artifact and 'emit_runtime_direct_eval_module(program,validated)?' in artifact and
            'runtime:Arc<RuntimeArtifact>,selection:IntlDataSelection,' in proof and
            proof.count('runtime.require_intl_profile(&selection)?;') == 1 and
            'ifself.build_admission.is_some()&&self.compilation_data.as_ref().is_some_and(|data|Arc::ptr_eq(data,selected)){returnOk(());}' in a('runtime_artifact.rs') and
            'crate::runtime_function::runtime_profile::require_runtime_profile(self.bytes(),selection)' in a('runtime_artifact.rs') and
            'Ok(Self{runtime,selection})' in proof,
            'complete cached witness and exact R/profile: retained opaque profile admission')
    witness = a('runtime_direct_eval/witness.rs')
    for token in ('ifactual!=root(program)?',
                  'PreparedScriptKind::DirectEval(program.caller().context().clone())',
                  'ifunit.kind!=expected_kind||unit.id!=program.script_id()',
                  'PreparedScriptOutcome::Executable(unit)',
                  'scriptw_front::decode_js_string_utf16(&root(program)?.source,)',
                  'letunit=require_root_source_row(program,actual)?;'):
        require(token in witness, 'exact root role/source/script witness: '+token)
    for field, schema in (
        ('PrivateEnvironmentParent','PrivateEnvironmentSchema::PARENT'),
        ('PrivateEnvironmentClassScope','PrivateEnvironmentSchema::CLASS_SCOPE'),
        ('PrivateEnvironmentNames','PrivateEnvironmentSchema::NAMES'),
        ('DirectEvalClassContext','DirectEvalExecutionContextSchema::CLASS_CONTEXT'),
        ('DirectEvalHomeObject','DirectEvalExecutionContextSchema::HOME_OBJECT'),
        ('DirectEvalDerivedBindings','DirectEvalExecutionContextSchema::DERIVED_BINDINGS'),
        ('DirectEvalThisSnapshot','DirectEvalExecutionContextSchema::THIS_SNAPSHOT'),
        ('DirectEvalNewTargetSnapshot','DirectEvalExecutionContextSchema::NEW_TARGET_SNAPSHOT'),
        ('DerivedThisCell','DirectEvalDerivedBindingsSchema::THIS_CELL'),
        ('DerivedThisStatusCell','DirectEvalDerivedBindingsSchema::THIS_STATUS_CELL'),
        ('DerivedNewTargetCell','DirectEvalDerivedBindingsSchema::NEW_TARGET_CELL'),
        ('DerivedActiveFunctionCell','DirectEvalDerivedBindingsSchema::ACTIVE_FUNCTION_CELL'),
        ('BindingCellValue','BindingCellSchema::VALUE'),
        ('ContextActiveFunction','FunctionContextSchema::ACTIVE_FUNCTION'),
        ('FunctionObjectContext','FunctionObjectSchema::CONTEXT'),
        ('FunctionObjectDerivedConstructor','FunctionObjectSchema::DERIVED_CONSTRUCTOR')):
        kind = 'scalar' if field in ('PrivateEnvironmentClassScope','FunctionObjectDerivedConstructor') else 'reference'
        require(re.search(r'Self::'+field+r'=>\{?'+kind+r'_field\(&'+schema+r'\)\}?', host),
                'host declaration-owned projection: '+field)
    require('Self::PrivateNameTable=>Some(reference_array_element::<PrivateNameTable,PrivateName>())' in host and
            '<<ImmutableByteArrayasGcArrayHeapType>::ElementasHostFieldStorage>::HOST_STORAGE' in host,
            'host private-name and immutable descriptor arrays use real schema storage')
    require('Self::ContextInstanceElementsInitializer=>{function_field(&FunctionContextSchema::INSTANCE_ELEMENTS_INITIALIZER)}' in host and
            'storage:GcHostStorage::FunctionReference{nullable:N::NULLABLE,}' in host and
            'index:field.ordinal().raw(),' in host,
            'nullable initializer fixture field has typed schema-derived projection')
    rejection = c['crates/scriptw-ir/src/runtime_semantics.rs']
    require('Self::UnavailableCapability(RuntimeUnavailableCapability::ExternalCallerImport)=>11,' in rejection and
            '11=>Some(Self::UnavailableCapability(RuntimeUnavailableCapability::ExternalCallerImport,))' in rejection,
            'external caller import owns distinct roundtripping capability11')
    module = a('modules.rs')
    require('referrer:scriptw_ir::ImportCallSourceOriginIr,' in module and
            'scriptw_ir::ImportCallSourceOriginIr::Local(referrer)=>referrer,' in module,
            'local and external import origins stay typed and exhaustive')
    external = module.split('scriptw_ir::ImportCallSourceOriginIr::ExternalCaller(_origin)=>{',1)[1].split('letdispatcher=',1)[0]
    steps = ['self.compile_expr_to_value(specifier,&argument,function)?;',
             'self.emit_propagate_current_throw_if_needed(function);',
             'self.compile_expr_to_value(options,&import_options,function)?;',
             'self.emit_reject_runtime_semantics(',
             'scriptw_ir::RuntimeUnavailableCapability::ExternalCallerImport']
    positions = [external.find(token) for token in steps]
    require(all(i >= 0 for i in positions) and positions == sorted(positions) and
            external.count('self.emit_propagate_current_throw_if_needed(function);') == 2 and
            'returnOk(());' in external and '=>None' not in external,
            'execution-only external import preserves operand effects/abrupts before capability rejection')
    for name in ('runtime_artifact/cache/tests.rs', 'runtime_artifact/build_package/tests.rs'):
        for version in (15,6,7,8):
            require(a(name).count('letprevious_version='+str(version)+'u32;') == 1,
                    'authenticated old-ABI envelope rejection '+name+':'+str(version))
    tests = a('runtime_direct_eval/tests.rs')
    require('Validator::new_with_features(WasmFeatures::all()).validate_all(artifact.bytes()).unwrap();' in tests and
            'assert_eq!(ty.params().len(),15);' in tests and
            'assert_ne!(first,second);' in tests,
            'authored typed-Wasm/factory and full-caller controls remain present')
    return failures

def main():
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd()
    failures = check({name:(root/name).read_text() for name in FILES})
    if failures:
        for failure in failures:
            print('ABI8 native direct eval: '+failure, file=sys.stderr)
        return 1
    print('ABI8 native direct-eval source controls passed')
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
