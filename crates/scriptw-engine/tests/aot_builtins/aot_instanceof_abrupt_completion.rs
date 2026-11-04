use scriptw_engine::{CompileOptions, Engine, ExecutionBackend, RealmBuilder, RunOptions};

fn assert_script_true(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
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
        .unwrap_or_else(|error| panic!("instanceof script failed: {error}\n{source}"));
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        outcome.note.contains("boolean(true)"),
        "{}\n{source}",
        outcome.note
    );
}

// InstanceofOperator step 3 calls @@hasInstance. Its abrupt completion must
// reach the enclosing user handler, not return from the current function.
#[test]
fn a_throwing_has_instance_method_is_caught_and_runs_finally() {
    assert_script_true(
        r#"
var marker = {}, trace = '';
var target = { [Symbol.hasInstance]() { trace += 'h'; throw marker; } };
var caught;
try { ({}) instanceof target; trace += 'x'; }
catch (error) { caught = error; trace += 'c'; }
finally { trace += 'f'; }
caught === marker && trace === 'hcf';
"#,
    );
}

// OrdinaryHasInstance step 4 reads C.prototype through a Proxy trap, and step 5
// throws a TypeError for a non-object prototype. Both run after the intrinsic
// Function.prototype[@@hasInstance] call, inside a user function's handler.
#[test]
fn ordinary_has_instance_errors_reach_the_function_handler() {
    assert_script_true(
        r#"
var marker = {};
var proxy = new Proxy(function () {}, {
  get(target, key) { if (key === 'prototype') throw marker; return target[key]; }
});
function nonObjectPrototype() {}
nonObjectPrototype.prototype = 1;
const arrow = () => {};
function check(constructor) {
  try { ({}) instanceof constructor; return 'none'; }
  catch (error) { return error; }
}
var fromProxy = check(proxy);
var fromPrimitive = check(nonObjectPrototype);
var fromArrow = check(arrow);
fromProxy === marker
  && fromPrimitive instanceof TypeError
  && fromArrow instanceof TypeError
  && ((function* () {})() instanceof (function* () {})) === false;
"#,
    );
}

#[test]
fn instanceof_holds_ordered_operands_through_reentrant_hooks_and_whole_throws() {
    assert_script_true(
        r#"
var trace = [], original = {}, replacement = {}, selected, actual;
actual = {
  get [Symbol.hasInstance]() {
    trace.push('get');
    selected = replacement;
    return function(value) {
      trace.push('call');
      if (this !== actual || value !== original) throw 'held operands';
      var inner = { [Symbol.hasInstance]() { throw undefined; } };
      var caught = false;
      try { value instanceof inner; }
      catch (error) { caught = true; if (error !== undefined) throw 'whole throw'; }
      finally { trace.push('inner'); }
      if (!caught) throw 'missing inner throw';
      return { valueOf() { throw 'ToBoolean must not coerce'; } };
    };
  }
};
selected = actual;
var current = original;
function left() { trace.push('left'); return current; }
function right() { trace.push('right'); current = replacement; return selected; }
var result = left() instanceof right();
result === true && current === replacement && selected === replacement
  && trace.join(',') === 'left,right,get,call,inner';
"#,
    );
}

#[test]
fn ordinary_entry_skips_hooks_before_bound_target_operator_reentry() {
    assert_script_true(
        r#"
var intrinsic = Function.prototype[Symbol.hasInstance];
var candidate = {}, marker = {}, reads = 0, calls = 0, outerReads = 0;
var nonCallable = {
  get [Symbol.hasInstance]() { throw 'ordinary entry read a hook'; },
  get prototype() { throw 'ordinary entry read a prototype'; }
};
function Target() {}
Object.defineProperty(Target, Symbol.hasInstance, {
  get() {
    reads++;
    return function(value) {
      calls++;
      if (this !== Target || value !== candidate) throw 'bound target operands';
      return { valueOf() { throw 'ToBoolean coercion'; } };
    };
  }
});
var bound = Target.bind(null);
Object.defineProperty(bound, Symbol.hasInstance, {
  get() { outerReads++; throw marker; }
});
var nonCallableResult = intrinsic.call(nonCallable, candidate);
var boundResult = intrinsic.call(bound, candidate);
var caught = false;
try { candidate instanceof bound; }
catch (error) { caught = error === marker; }
nonCallableResult === false && boundResult === true && caught
  && reads === 1 && calls === 1 && outerReads === 1;
"#,
    );
}
