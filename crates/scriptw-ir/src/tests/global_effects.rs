#[test]
fn spread_object_assign_accounts_for_proxy_trap_effects() {
    assert_caller_flow_invalidation_reaches_final_addition(
            "const proxy = new Proxy({}, { ownKeys() { holder = {}; return []; } }); let holder = { value: 1 }; Object.assign(...[{}, proxy]); holder.value + 1;",
        );
}

#[test]
fn spread_define_property_call_cannot_claim_precisely_accounted_effects() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "let other; \
             let proxy = new Proxy({{}}, {{ defineProperty: function () {{ \
                 delete other.evalScript; return true; \
             }} }}); \
             Object.defineProperty(...[], proxy, (other = {create_realm}(), 'x'), {{ value: 0 }}); \
             other.evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn spread_define_property_alias_invalidates_effects_after_later_arguments() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "let other; \
             let proxy = new Proxy({{}}, {{ defineProperty: function () {{ \
                 delete other.evalScript; return true; \
             }} }}); \
             let define = Object.defineProperty; \
             define(...[], proxy, (other = {create_realm}(), 'x'), {{ value: 0 }}); \
             other.evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn multi_target_define_property_call_invalidates_a_created_realm_shape() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "let define = Math.random() < 0.5 \
                 ? Object.defineProperty \
                 : Reflect.defineProperty; \
             let other = {create_realm}(); \
             define(other, 'evalScript', {{ value: 0 }}); \
             other.evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn define_property_on_a_named_global_invalidates_a_wrapper_dependency() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "let cached = {create_realm}(); \
             function makeRealm() {{ return {create_realm}(); }} \
             Object.defineProperty(globalThis, '{create_realm}', {{ \
                 value: function () {{ return cached; }} \
             }}); \
             delete cached.evalScript; \
             makeRealm().evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn define_property_with_unproven_inherited_fields_widens_a_wrapper_dependency() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "function makeRealm() {{ return {create_realm}(); }} \
             Object.defineProperty(globalThis, 'unrelated', {{ value: 0 }}); \
             makeRealm().evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
}

#[test]
fn descriptor_fields_without_current_inherited_proof_retain_possible_getter_effects() {
    for define in ["Object.defineProperty", "Reflect.defineProperty"] {
        assert_caller_flow_invalidation_reaches_final_addition(&format!(
            "let holder = {{ value: 1 }}; \
             {define}({{}}, 'field', {{ value: 7 }}); \
             holder.value + 1;"
        ));
    }
}

#[test]
fn mutable_factory_wrapper_does_not_publish_a_recreated_return() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "let cached = {create_realm}(); \
             let factory = {create_realm}; \
             function createRealm() {{ return factory(); }} \
             factory = function () {{ return cached; }}; \
             delete cached.evalScript; \
             createRealm().evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn wrapper_dependency_includes_intermediate_script_global_targets() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "let cached = {create_realm}(); \
             var factory = {create_realm}; \
             function makeRealm() {{ return factory(); }} \
             factory = function () {{ return cached; }}; \
             delete cached.evalScript; \
             makeRealm().evalScript('source'); \
             factory = {create_realm};"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn implicit_fallthrough_is_part_of_a_recreated_return_shape() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "function maybeRealm(flag) {{ \
                 if (flag) return {create_realm}(); \
             }} \
             maybeRealm(false).evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    let script = program.script.as_ref().expect("script ir should exist");
    assert!(!script
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
    let maybe_realm = script
        .functions
        .iter()
        .find(|function| function.name == "maybeRealm")
        .expect("maybeRealm should be lowered");
    assert_eq!(maybe_realm.return_kind, ValueKind::Dynamic);
    assert!(maybe_realm.return_shape.is_none());
}

#[test]
fn explicit_undefined_is_part_of_a_recreated_return_shape() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "function maybeRealm(flag) {{ \
                 if (flag) return undefined; \
                 return {create_realm}(); \
             }} \
             maybeRealm(false).evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    let script = program.script.as_ref().expect("script ir should exist");
    assert!(!script
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
    let maybe_realm = script
        .functions
        .iter()
        .find(|function| function.name == "maybeRealm")
        .expect("maybeRealm should be lowered");
    assert_eq!(maybe_realm.return_kind, ValueKind::Dynamic);
    assert!(maybe_realm.return_shape.is_none());
}

#[test]
fn generated_method_fallthrough_is_part_of_a_recreated_return_shape() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "class Factory {{ \
                 maybeRealm(flag) {{ if (flag) return {create_realm}(); }} \
             }} \
             new Factory().maybeRealm(false).evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    let script = program.script.as_ref().expect("script ir should exist");
    assert!(!script
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
    let maybe_realm = script
        .functions
        .iter()
        .find(|function| function.name == "Factory.maybeRealm")
        .expect("maybeRealm should be lowered");
    assert_eq!(maybe_realm.return_kind, ValueKind::Dynamic);
    assert!(maybe_realm.return_shape.is_none());
}

#[test]
fn rebound_create_realm_global_invalidates_the_wrapper_dependency() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "let cached = {create_realm}(); \
             function createRealm() {{ return {create_realm}(); }} \
             {create_realm} = function () {{ return cached; }}; \
             delete cached.evalScript; \
             createRealm().evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn a_lexical_factory_shadow_cannot_inherit_a_global_recreated_return_proof() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "let cached = globalThis.{create_realm}(); \
             let {create_realm} = function () {{ return cached; }}; \
             function makeRealm() {{ return {create_realm}(); }} \
             delete cached.evalScript; \
             makeRealm().evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn reflective_function_mutation_does_not_alias_an_unrelated_function_shape() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "function AbstractModuleSource() {{}} \
             Object.defineProperty(AbstractModuleSource, 'x', {{ __proto__: null, value: 0 }}); \
             let $262 = {{ createRealm: function () {{ return {create_realm}(); }} }}; \
             function assert() {{}} Object.defineProperty(assert, 'sameValue', {{ __proto__: null, value: function () {{}} }}); \
             let other = $262.createRealm(); other.evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert_prepared_script(&program, PreparedScriptKind::RealmScript);
}

#[test]
fn nested_global_alias_mutation_preserves_a_sibling_realm_factory_shape() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    // Define the own data property without an inherited setter or descriptor getter.
    let source = format!(
        "var $262 = {{ \
                 global: globalThis, \
                 createRealm: function () {{ return {create_realm}(); }} \
             }}; \
             function assert() {{}} \
             Object.defineProperty(assert, 'sameValue', {{ \
                 __proto__: null, value: function () {{}} \
             }}); \
             let other = $262.createRealm(); other.evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert_prepared_script(&program, PreparedScriptKind::RealmScript);
}

#[test]
fn uncalled_function_bodies_do_not_erase_a_script_global_realm_shape() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let receive_broadcast = HostBuiltinId::AgentReceiveBroadcast
        .global_name()
        .expect("receive broadcast must have a harness global name");
    let source = format!(
        "var $262 = {{ \
                 agent: {{ receiveBroadcast: function (callback) {{ \
                     return callback({receive_broadcast}()); \
                 }} }}, \
                 createRealm: function () {{ return {create_realm}(); }} \
             }}; \
             var dormant = function (callback) {{ callback(); }}; \
             let other = $262.createRealm(); other.evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert_prepared_script(&program, PreparedScriptKind::RealmScript);
}

#[test]
fn global_object_property_read_observes_the_live_script_global_value() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "var other = {create_realm}(); \
             function run() {{ globalThis.other.evalScript('source'); }} \
             other = {{}}; \
             try {{ run(); }} catch {{}} \
             other = {create_realm}();"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn called_generated_method_observes_the_live_script_global_value() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "var other = {create_realm}(); \
             class Runner {{ run() {{ other.evalScript('source'); }} }} \
             other = {{}}; \
             try {{ new Runner().run(); }} catch {{}} \
             other = {create_realm}();"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn called_getter_observes_the_live_script_global_value() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "var other = {create_realm}(); \
             var holder = {{ get value() {{ other.evalScript('source'); }} }}; \
             other = {{}}; \
             try {{ holder.value; }} catch {{}} \
             other = {create_realm}();"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn builtin_callback_observes_the_live_script_global_value() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "var other = {create_realm}(); \
             function callback() {{ other.evalScript('source'); }} \
             other = {{}}; \
             try {{ [0].forEach(callback); }} catch {{}} \
             other = {create_realm}();"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn with_fallback_call_observes_the_live_script_global_value() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "var other = {create_realm}(); \
             function run() {{ other.evalScript('source'); }} \
             var fallback = run; \
             other = {{}}; \
             try {{ with ({{}}) {{ fallback(); }} }} catch {{}} \
             other = {create_realm}();"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn script_global_initializer_updates_the_global_object_shape() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "var $262 = {{ createRealm: function () {{ return {create_realm}(); }} }}; \
             var dormant = function (callback) {{ callback(); }}; \
             let other = globalThis.$262.createRealm(); other.evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert_prepared_script(&program, PreparedScriptKind::RealmScript);
}

#[test]
fn static_field_initializer_updates_later_script_global_flow() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "var $262; \
             class Harness {{ \
                 static realm = ($262 = {{ \
                     createRealm: function () {{ return {create_realm}(); }} \
                 }}); \
             }} \
             let other = $262.createRealm(); other.evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert_prepared_script(&program, PreparedScriptKind::RealmScript);
}

#[test]
fn static_block_live_flow_excludes_an_unexecuted_instance_initializer_summary() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "var $262; \
             class Harness {{ \
                 static {{ \
                     $262 = {{ \
                         createRealm: function () {{ return {create_realm}(); }} \
                     }}; \
                     class NeverInstantiated {{ field = ($262 = {{}}); }} \
                 }} \
             }} \
             let other = $262.createRealm(); other.evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert_prepared_script(&program, PreparedScriptKind::RealmScript);
}

#[test]
fn static_block_transfers_a_non_var_global_property_effect() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "let cached = {create_realm}(); \
             function makeRealm() {{ return {create_realm}(); }} \
             class Harness {{ \
                 static {{ \
                     globalThis.{create_realm} = function () {{ return cached; }}; \
                 }} \
             }} \
             delete cached.evalScript; \
             makeRealm().evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn static_block_invalidates_number_prototype_to_string_state() {
    let program = lower_script(
            "class Harness { static { Number.prototype.toString = Object.prototype.toString; } } (1).toString();",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
        panic!("expected follow-up call");
    };
    assert!(
        !matches!(&result.expr, ExprIr::String(value) if value == "1"),
        "the static block's prototype write must prevent stale intrinsic folding: {:?}",
        result.expr
    );
}

#[test]
fn static_block_function_declaration_does_not_create_a_global_binding() {
    let program = lower_script("class Harness { static { function hidden() {} } } typeof hidden;");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
        panic!("expected follow-up typeof expression");
    };
    assert!(
        matches!(
            &result.expr,
            ExprIr::TypeOfUnresolvedIdentifier { name } if name == "hidden"
        ),
        "the static block's local function must not become a global property: {:?}",
        result.expr
    );
}

#[test]
fn static_block_boolean_write_does_not_reuse_the_previous_literal_fold() {
    let program = lower_script(
        "var flag = true; class Harness { static { flag = false; } } flag.toString();",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
        panic!("expected follow-up Boolean method call");
    };
    assert!(
        !matches!(&result.expr, ExprIr::String(value) if value == "true"),
        "the static block's false write must prevent the stale true fold: {:?}",
        result.expr
    );
}

#[test]
fn static_block_replays_boolean_alias_invalidation_in_the_enclosing_function() {
    let program = lower_script(
            "function run() { var boxed = new Boolean(true); var alias = boxed; class Harness { static { boxed.toString = Number.prototype.toString; } } return alias.toString(); }",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let run = script
        .functions
        .iter()
        .find(|function| function.name == "run")
        .expect("run function should be lowered");
    let StatementIr::Return(result) = run.body.statements.last().unwrap() else {
        panic!("expected run to return the aliased Boolean method call");
    };
    assert!(
        !matches!(&result.expr, ExprIr::String(value) if value == "true"),
        "the static block's property write must invalidate the enclosing alias fold: {:?}",
        result.expr
    );
}

#[test]
fn static_field_this_write_updates_a_later_static_field() {
    let program = lower_script(
            "class Harness { static method() {} static first = (this.method = 1); static second = this.method; } Harness.second;",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
        panic!("expected final static field read");
    };
    assert_eq!(
            result.possible_kinds,
            KindSet::from_kind(ValueKind::Function).union(KindSet::from_kind(ValueKind::Number)),
            "the later field must include the write through this without losing the class shape: {result:?}"
        );
}

#[test]
fn static_field_class_name_write_updates_a_later_static_field() {
    let program = lower_script(
            "class Harness { static method() {} static first = (Harness.method = 1); static second = Harness.method; } Harness.second;",
        );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
        panic!("expected final static field read");
    };
    assert_eq!(
            result.possible_kinds,
            KindSet::from_kind(ValueKind::Function).union(KindSet::from_kind(ValueKind::Number)),
            "the later field must include the write through the class name without losing the class shape: {result:?}"
        );
}

/// The possible kinds of the first value the final body of `run` returns.
fn final_run_return_kinds(source: &str) -> KindSet {
    let program = lower_script(source);
    assert!(
        program.is_wasm_supported(),
        "{source}: {:?}",
        program.diagnostics
    );
    let script = program.script.as_ref().expect("script IR should exist");
    let run = script
        .functions
        .iter()
        .find(|function| function.name == "run")
        .unwrap_or_else(|| panic!("missing run: {source}"));
    function_return(run)
        .unwrap_or_else(|| panic!("run must return: {source}"))
        .possible_kinds
}

/// A function body is lowered with the fresh-realm global facts, before the
/// root statements it may run after. Whatever path replaces a builtin global
/// or one of its methods, the body must not keep the intrinsic's result kind:
/// here `+` would otherwise be a Number addition of a string.
#[test]
fn a_final_body_does_not_keep_a_builtin_fact_any_global_write_can_change() {
    const RUN: &str = "function run() { return Math.imul(2, 3) + 1; }";
    const REPLACEMENT: &str = "{ imul() { return 'x'; } }";
    for write in [
        format!("globalThis['Ma' + 'th'] = {REPLACEMENT};"),
        format!("Object.defineProperty(globalThis, 'Math', {{ get() {{ return {REPLACEMENT}; }} }});"),
        format!("Reflect.set(globalThis, 'Math', {REPLACEMENT});"),
        format!("Reflect.defineProperty(globalThis, 'Math', {{ value: {REPLACEMENT} }});"),
        format!("Object.assign(globalThis, {{ Math: {REPLACEMENT} }});"),
        format!("var g = globalThis; g.Math = {REPLACEMENT};"),
        format!("this.Math = {REPLACEMENT};"),
        format!("function set(o, k, v) {{ o[k] = v; }} set(globalThis, 'Math', {REPLACEMENT});"),
        "Math.imul = function () { return 'x'; };".to_string(),
        "Math['im' + 'ul'] = function () { return 'x'; };".to_string(),
        "var m = Math; m.imul = function () { return 'x'; };".to_string(),
        "Object.assign(Math, { imul() { return 'x'; } });".to_string(),
        "function set(o, v) { o.imul = v; } function replace() { set(Math, function () { return 'x'; }); } replace();".to_string(),
        "(0, eval)(\"Math.imul = function () { return 'x'; }\");".to_string(),
    ] {
        let source = format!("{RUN} {write} run();");
        let kinds = final_run_return_kinds(&source);
        assert!(kinds.contains(ValueKind::String), "{source}: {kinds:?}");
    }
}

/// The same holds for identifier-resolved conversions and prototype methods,
/// which the lowerer otherwise folds or types from the intrinsic. Each
/// replacement returns a kind the intrinsic never does.
#[test]
fn a_final_body_does_not_fold_a_replaceable_builtin_conversion_or_method() {
    for (run, write, replacement_kind) in [
        (
            "function run() { return Number('3'); }",
            "globalThis['Num' + 'ber'] = function () { return 'n'; };",
            ValueKind::String,
        ),
        (
            "function run() { return parseInt('3'); }",
            "globalThis['parse' + 'Int'] = function () { return 'p'; };",
            ValueKind::String,
        ),
        (
            "function run() { return String(5); }",
            "globalThis['Str' + 'ing'] = function () { return 7; };",
            ValueKind::Number,
        ),
        (
            "function run() { return 'abc'.charCodeAt(0); }",
            "String.prototype['charCode' + 'At'] = function () { return 'c'; };",
            ValueKind::String,
        ),
    ] {
        let source = format!("{run} {write} run();");
        let kinds = final_run_return_kinds(&source);
        assert!(kinds.contains(replacement_kind), "{source}: {kinds:?}");
    }
}

/// Precision: a program that never changes a builtin fact keeps the
/// intrinsic's result kind in a function body, even after ordinary writes
/// that revisit every builtin shape.
#[test]
fn a_final_body_keeps_a_builtin_fact_no_program_state_changes() {
    let source = "function run() { return Math.imul(2, 3) + 1; } \
                  var o = {}; o.x = 1; var a = [1]; a[0] = 2; run();";
    assert_eq!(
        final_run_return_kinds(source),
        KindSet::from_kind(ValueKind::Number),
        "{source}"
    );
}
