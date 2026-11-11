use scriptw_engine::{CompileOptions, Engine, ExecutionBackend, RealmBuilder, RunOptions};

fn assert_direct_eval(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("direct eval executes through compiled Script units");
    assert!(
        outcome.note.contains("boolean(true)"),
        "{}\n{source}",
        outcome.note
    );
}

#[test]
fn direct_eval_mutates_the_selected_lexical_binding() {
    assert_direct_eval(
        r###"function run() { let value = 1; eval('value += 2'); return value === 3; } run();"###,
    );
}

#[test]
fn sloppy_eval_adds_nearer_variables_visible_to_existing_closures() {
    assert_direct_eval(
        r###"let value = 1; function run() { function read() { return value; } let before = read(); eval('var value = 3'); return before === 1 && read() === 3 && delete value && read() === 1; } run();"###,
    );
}

#[test]
fn strict_eval_isolates_declarations_and_reuses_outer_assignments() {
    assert_direct_eval(
        r###"function run() { 'use strict'; let value = 1; let result = eval('var hidden = 2; value = 3; hidden'); return result === 2 && value === 3 && typeof hidden === 'undefined'; } run();"###,
    );
}

#[test]
fn eval_binding_conflicts_fail_before_declaration_mutation() {
    assert_direct_eval(
        r###"function run() { let value = 1; try { eval('var added = 2; var value = 3;'); } catch (error) { return error instanceof SyntaxError && value === 1 && typeof added === 'undefined'; } return false; } run();"###,
    );
}

#[test]
fn direct_eval_preserves_invocation_this_and_new_target() {
    assert_direct_eval(
        r###"function Construct() { this.correct = eval('this') === this && eval('new.target') === Construct; } new Construct().correct;"###,
    );
}

#[test]
fn direct_eval_resolves_with_unscopables_at_execution() {
    assert_direct_eval(
        r###"let value = 2; var scope = { value: 9, [Symbol.unscopables]: { value: true } }; var first; with (scope) { first = eval('value'); eval('value = 4'); } first === 2 && value === 4 && scope.value === 9;"###,
    );
}

#[test]
fn eval_parameter_and_body_variable_environments_remain_distinct() {
    assert_direct_eval(
        r###"function run(value = eval('var other = 1; other')) { var other = 2; return value === 1 && other === 2; } run();"###,
    );
}

#[test]
fn direct_eval_observes_private_names_only_from_its_caller_context() {
    assert_direct_eval(
        r###"class Box { #value = 3; read() { return eval('this.#value'); } } var outside = false; try { eval('this.#value'); } catch (error) { outside = error instanceof SyntaxError; } outside && new Box().read() === 3;"###,
    );
}

#[test]
fn direct_eval_reference_is_selected_before_rhs_replaces_the_binding_table() {
    assert_direct_eval(
        "function f() { eval('var x = 1'); x = (eval('delete x; var x = 2'), 3); return x === 3; } f();",
    );
}

#[test]
fn direct_eval_identifier_operators_and_destructuring_use_live_cells() {
    assert_direct_eval(
        "function f() { let x = 1; eval('x += 2; x++; x &&= 7; [x] = [9]'); return x === 9; } f();",
    );
}

#[test]
fn direct_eval_super_call_updates_the_original_derived_this_binding() {
    assert_direct_eval(
        "class B { constructor() { this.x = 3; } } class C extends B { y = 4; constructor() { eval('super()'); } } let c = new C; c.x === 3 && c.y === 4;",
    );
}

#[test]
fn replaced_eval_retains_the_original_identifier_call_and_argument_effects() {
    assert_direct_eval(
        "function f() { let trace = ''; let eval = function(source, extra) { return source + extra; }; let result = eval((trace += 'a', 'x'), (trace += 'b', 'y')); return result === 'xy' && trace === 'ab'; } f();",
    );
}

#[test]
fn direct_eval_non_string_does_not_access_uninitialized_derived_this() {
    assert_direct_eval(
        "class B {} class C extends B { constructor() { let token = {}; if (eval(token) !== token) throw 1; super(); } } new C instanceof C;",
    );
}

#[test]
fn direct_eval_destructuring_var_initializer_uses_the_selected_with_record() {
    assert_direct_eval(
        "function f() { let object = { x: 0 }; with (object) { eval('var [x] = [7]'); } return object.x === 7 && x === undefined; } f();",
    );
}

#[test]
fn named_function_self_is_outside_the_eval_variable_record() {
    assert_direct_eval(
        "let f = function self() { let before = eval('self'); eval('var self = 9'); return before === f && self === 9; }; f();",
    );
}

#[test]
fn eval_assignment_to_named_function_self_respects_binding_strictness() {
    assert_direct_eval(
        "let f = function self() { eval('self = 3'); let ignored = self === f; let threw = false; try { eval('\"use strict\"; self = 4'); } catch (e) { threw = e instanceof TypeError; } return ignored && threw; }; f();",
    );
}

#[test]
fn top_level_arrow_grammar_does_not_make_its_eval_vars_global() {
    assert_direct_eval(
        "let f = () => { eval('var localOnly = 7'); return localOnly === 7; }; f() && typeof localOnly === 'undefined';",
    );
}
