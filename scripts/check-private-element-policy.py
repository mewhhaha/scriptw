#!/usr/bin/env python3
"""Source ownership guard for the explicit private-element proposal policy.

API: check(root: Path, overrides: dict[str,str] | None = None) -> list[str].
Overrides are used only by isolated source mutants; no source file is edited.
"""
import argparse,hashlib,re
from pathlib import Path

CAPS={
    'crates/scriptw-ir/src/language_features.rs':80,
    'crates/scriptw-aot-wasm/src/gc_types/value/program_instance/language_features.rs':60,
    'crates/scriptw-engine/src/native_language_feature_policy_tests.rs':160,
    'crates/scriptw-engine/tests/aot_language/aot_private_element_extensibility.rs':300,
}
POSITIVE='crates/scriptw-engine/tests/aot_language/aot_callable_capture_lifecycle.rs'
POSITIVE_NAME='private_methods_can_be_installed_on_a_nonextensible_receiver'
POSITIVE_SHA='5601fdc0649e5b0162491199294e7cfade4deb43fe7b8f1a063a6f90710fa869'


def check(root,overrides=None):
    root=Path(root);overrides={} if overrides is None else overrides;passed=[]
    def read(rel):return overrides[rel] if rel in overrides else (root/rel).read_text()
    def need(ok,name):
        if not ok:raise ValueError('private-element policy boundary: '+name)
        passed.append(name)
    def between(s,start,end):
        need(s.count(start)==1,'unique owner '+start)
        return s.split(start,1)[1].split(end,1)[0]
    for rel,cap in CAPS.items():need(len(read(rel).splitlines())<=cap,'fixed new-file cap '+rel)
    policy=read('crates/scriptw-ir/src/language_features.rs')
    need(re.search(r'#\[default\]\s*Ecma262\s*,',policy) is not None and
         'NonExtensibleAppliesToPrivate' in policy and 'pub struct LanguageFeaturePolicy' in policy and
         'private_element_extensibility: PrivateElementExtensibility' in policy and
         'PrivateElementExtensibility::Ecma262 => 0' in policy and
         'PrivateElementExtensibility::NonExtensibleAppliesToPrivate => 1' in policy,'typed baseline default and opt-in wire domain')
    need('mod language_features;' in read('crates/scriptw-ir/src/lib.rs') and
         'pub use language_features::{LanguageFeaturePolicy, PrivateElementExtensibility}' in read('crates/scriptw-ir/src/lib.rs'),'typed policy module remains attached and exported')
    ir=read('crates/scriptw-ir/src/ir.rs');lower=read('crates/scriptw-ir/src/lowering.rs')
    need('pub language_features: crate::LanguageFeaturePolicy' in ir and
         'script.language_features = policy' in ir and
         'language_features: crate::LanguageFeaturePolicy::default()' in lower,'ScriptIr explicit default and retained policy')
    layouts=read('crates/scriptw-aot-wasm/src/gc_types/layouts.rs')
    need('LANGUAGE_FEATURE_POLICY: scriptw_ir::LanguageFeaturePolicy, Immutable, NonNullable;' in layouts,'immutable typed P-owned policy')
    image=read('crates/scriptw-aot-wasm/src/gc_types/value/program_instance.rs')
    projection=read('crates/scriptw-aot-wasm/src/gc_types/value/program_instance/language_features.rs')
    need('mod language_features;' in image and 'fn emit_nonextensible_private_elements_enabled(' not in image and
         projection.count('fn emit_nonextensible_private_elements_enabled(')==1 and
         'impl ProgramInstanceLocals' in projection,'one typed child owns P policy projection')
    need('GcOperand::constant(image.language_features)' in image and
         '.field(ProgramInstanceSchema::LANGUAGE_FEATURE_POLICY)' in projection and
         '.read(&self.owner, schema, function)' in projection,'P publication and actual P projection')
    need('script.language_features' in read('crates/scriptw-aot-wasm/src/emit/module_assembly.rs') and
         'impl GcI32Constant for scriptw_ir::LanguageFeaturePolicy' in read('crates/scriptw-aot-wasm/src/gc_types/value.rs'),'P source policy and typed scalar codec')
    helpers=read('crates/scriptw-aot-wasm/src/runtime_helpers.rs')
    row=next((r for r in helpers.splitlines() if r.strip().startswith('PrivateFieldDefine /')),'')
    need('caller_execution_realm:(Ref RealmRecord NonNullable)' in row and
         'caller_environment:(Ref Environment Nullable),caller_program:Program' in row,'field helper retains caller realm/environment/P')
    private=read('crates/scriptw-aot-wasm/src/objects/private_elements.rs')
    body=between(private,'pub(crate) fn emit_private_element_extensibility_check(','/// One call per private field definition:')
    need(re.search(r'self\.current_program\(\)\s*\.emit_nonextensible_private_elements_enabled',body) is not None and
         body.index('self.current_program()')<body.index('self.emit_object_is_extensible') and
         body.count('self.emit_object_is_extensible')==1 and 'OrdinaryObjectSchema::EXTENSIBLE' not in body,'proposal gate uses actual P and observable IsExtensible')
    need(body.index('self.emit_object_is_extensible')<body.index('self.emit_object_operation_abrupt_exit(result, result, exit, function)')<body.index('self.compile_truthy_tagged_i32'),
         'IsExtensible abrupt value precedes boolean interpretation')
    field=between(private,'pub(crate) fn compile_private_field_define_helper(','    fn emit_private_element_append(')
    need(field.index('self.emit_private_element_extensibility_check')<field.index('self.emit_private_name_token_resolve')<field.index('self.emit_private_element_add_body'),
         'per-field check precedes duplicate lookup/insertion')
    add=between(private,'    fn emit_private_element_add_body(','    /// The opt-in proposal observes')
    need('emit_private_element_extensibility_check' not in add and 'emit_object_is_extensible' not in add,'method row insertion does not repeat batch check')
    captures=read('crates/scriptw-aot-wasm/src/functions/class_captures.rs')
    need(captures.count('self.emit_private_element_extensibility_check')==1,'one method-batch guard emitter')
    guard=captures.index('self.emit_private_element_extensibility_check')
    plan=captures.rfind('if !plan.private_method_brands.is_empty() {',0,guard)
    need(plan>=0 and guard<captures.index('let definitions =',plan)<captures.index('let next =',plan)<captures.index('self.emit_private_brand_add',plan),'true nonempty private plan gates check before method loop')
    fields=captures.index('for element in plan.elements')
    need(fields<captures.index('match initializer_id',fields)<captures.index('self.emit_private_field_define(',fields),'field initializer completes before field-helper barrier')
    hook=read('crates/scriptw-aot-wasm/src/functions/instance_elements_hook.rs')
    need('FunctionContextSchema::DEFINING_PROGRAM' in hook and 'parameters.caller_program.load' in hook and
         'Instruction::RefEq' in hook and 'self.emit_initialize_instance_elements(' in hook,'foreign-super hook retains actual constructor defining P')
    engine=read('crates/scriptw-engine/src/lib.rs')
    need('pub language_features: LanguageFeaturePolicy' in engine and 'language_features: LanguageFeaturePolicy::default()' in engine and
         'hash.update(options.language_features.wire_code().to_le_bytes())' in engine,'CompileOptions default and framed cache policy witness')
    need('language_features: options.language_features' in engine and 'language_features: self.language_features' in engine,'agent root/worker options inherit exact policy')
    need('#[cfg(all(test, feature = "native-source-compiler"))]\nmod native_language_feature_policy_tests;' in engine and
         'mod aot_private_element_extensibility;' in read('crates/scriptw-engine/tests/aot_language/main.rs'),'native observer and integration controls remain statically attached')
    retained=read('crates/scriptw-engine/src/wasm_compilation_policy.rs')
    need('language_features: options.language_features' in retained and 'language_features: unit.ir.language_features()' in retained,'retained execution policy derives exact IR authority')
    need('ir.with_language_features(options.language_features)' in read('crates/scriptw-engine/src/prepared_source_catalog.rs'),'loaded and prepared source catalog retains policy')
    context=read('crates/scriptw-engine/src/wasm_native_source_compiler.rs')
    need('pub(super) policy: WasmCompilationPolicy' in context and 'policy.intl_profile().clone()' in context and
         'execution: Arc<WasmAgentExecutionControl>' in context,'native source context owns complete policy and same execution')
    for stem in ['function','global_script','direct_eval']:
        callback=read('crates/scriptw-engine/src/wasm_native_'+stem+'_compiler.rs')
        need('context.policy.host_surface_policy()' in callback and
             'program.with_language_features(context.policy.language_features())' in callback,'native '+stem+' inherits exact feature policy')
        need('self.program = self.program.with_language_features(policy)' in read('crates/scriptw-ir/src/runtime_'+stem+'.rs'),'native '+stem+' IR witness retains selected policy')
    test262=read('crates/scriptw-test262/src/lib.rs')
    mapper=between(test262,'fn language_features_for_case(case: &TestCase)','fn compile_options_for_case(')
    need('case.features.contains("nonextensible-applies-to-private")' in mapper and
         'PrivateElementExtensibility::NonExtensibleAppliesToPrivate' in mapper and
         all(word not in mapper for word in ['source_path','original_source','execution_id','import-defer']),'metadata-only proposal admission without fixture dispatch')
    need('language_features: language_features_for_case(case)' in test262,'real Test262 compile options consume declared policy')
    positive=read(POSITIVE);start=positive.index('#[test]\nfn '+POSITIVE_NAME+'(');end=positive.index('\n#[test]',start+1)
    need(hashlib.sha256(positive[start:end].encode()).hexdigest()==POSITIVE_SHA,'unchanged named baseline positive behavior')
    return passed


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--root',type=Path,required=True);args=parser.parse_args()
    checks=check(args.root);print('private-element policy source boundary: PASS ('+str(len(checks))+' checks; native controls remain unrun)')

if __name__=='__main__':main()
