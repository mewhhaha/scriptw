use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str, module: bool, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let compile = CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    };
    let options = RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    };
    let observed = if module {
        engine.observe_module(source, compile, options)
    } else {
        engine.observe_script(source, compile, options)
    }
    .expect("visited-set control executes compiled Wasm");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "module={module}: {:?}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        [HostOutputEvent::PrintLine(expected.into())]
    );
}

#[test]
fn visited_sets_keep_content_shadowing_private_hooks_reentry_and_whole_throws() {
    let source = include_str!("../fixtures/for_in_visited_set.js");
    run(source, false, "for-in-visited-set:ok");
    run(
        &format!("'use strict';\n{source}"),
        false,
        "for-in-visited-set:ok",
    );
    run(source, true, "for-in-visited-set:ok");
}

#[test]
fn async_and_async_generator_cursors_retain_sets_across_await_yield_mutation_and_gc() {
    let source = include_str!("../fixtures/for_in_visited_set_async.js");
    run(source, false, "for-in-visited-set-async:ok");
    run(source, true, "for-in-visited-set-async:ok");
}

#[test]
fn visited_set_keeps_scriptw_descriptor_before_mark_and_lazy_prototype_order() {
    run(
        r#"
function check(value, message) { if (!value) throw message; }
var events = [], reads = 0;
var parent = new Proxy({}, {
  ownKeys() { events.push('parent keys'); return ['missing','hidden','visible']; },
  getOwnPropertyDescriptor(target, key) {
    events.push('parent:' + key); return {value: 1, enumerable: true, configurable: true};
  },
  getPrototypeOf() { events.push('parent end'); return null; }
});
var child = new Proxy({}, {
  ownKeys() { events.push('child keys'); return ['missing','hidden','visible']; },
  getOwnPropertyDescriptor(target, key) {
    events.push('child:' + key); gc();
    if (key === 'missing') return undefined;
    return {get() { reads++; throw 'value read'; }, enumerable: key === 'visible', configurable: true};
  },
  getPrototypeOf() { events.push('child end'); return parent; }
});
for (var key in child) events.push('body:' + key);
check(events.join('|') === 'child keys|child:missing|child:hidden|child:visible|body:visible|child end|parent keys|parent:missing|body:missing|parent end', 'lazy descriptor and prototype order');
check(reads === 0, 'enumeration does not invoke source getters');
var changing = Object.create({stale: 1}); changing.first = 1;
var observed = [];
for (var changingKey in changing) {
  observed.push(changingKey);
  if (changingKey === 'first') Object.setPrototypeOf(changing, {fresh: 1});
}
check(observed.join('|') === 'first|fresh', 'prototype is observed after the current snapshot drains');
print('for-in-visited-order:ok');
"#,
        false,
        "for-in-visited-order:ok",
    );
}

#[test]
fn for_in_scalar_descriptor_observation_remains_live_without_reading_values() {
    let source = r#"
function check(value, message) { if (!value) throw message; }
var compactKeys = [];
for (var key in {left: 1, right: 2}) compactKeys.push(key);
check(compactKeys.join('|') === 'left|right', 'compact descriptor observation');
function exercise(object) {
  var parent = {missing: 1, demote: 1, inherited: 1};
  Object.setPrototypeOf(object, parent);
  Object.defineProperty(object, 'promote', {
    get() { throw 'property value read'; }, configurable: true, enumerable: false
  });
  var seen = [];
  for (var key in object) {
    seen.push(key);
    if (key === 'first') {
      Object.defineProperty(object, 'promote', {enumerable: true});
      Object.defineProperty(object, 'demote', {enumerable: false});
      delete object.missing;
      gc();
    }
  }
  check(seen.join('|') === 'first|promote|missing|inherited', 'live scalar descriptor flags');
}
exercise({first: 1, promote: 2, demote: 3, missing: 4});
var dictionary = Object.create(null);
dictionary.first = 1; dictionary.promote = 2; dictionary.demote = 3; dictionary.missing = 4;
exercise(dictionary);
var array = [1, 2, 3], keys = [];
for (var key in array) {
  keys.push(key);
  if (key === '0') {
    Object.defineProperty(array, '1', {enumerable: false});
    Object.defineProperty(array, '2', {get() { throw 'array value read'; }});
    gc();
  }
}
check(keys.join('|') === '0|2', 'array flags remain live and length stays hidden');
var marker = {whole: 'descriptor field'}, caught;
try {
  for (var key in new Proxy({}, {
    ownKeys() { return ['key']; },
    getOwnPropertyDescriptor() {
      return {get enumerable() { gc(); throw marker; }};
    }
  })) throw 'unexpected body';
} catch (error) { caught = error; }
check(caught === marker, 'exotic descriptor conversion retains whole Throw');
print('for-in-descriptor-observation:ok');
"#;
    run(source, false, "for-in-descriptor-observation:ok");
    run(source, true, "for-in-descriptor-observation:ok");
}

#[test]
fn module_for_in_and_for_of_keep_cursors_live_across_top_level_await() {
    run(
        r#"
var object = {first: 1, hidden: 2, last: 3}, keys = [];
for (var key in object) {
  await Promise.resolve();
  keys.push(key);
  if (key === 'first') Object.defineProperty(object, 'hidden', {enumerable: false});
  gc();
}
if (keys.join('|') !== 'first|last') throw 'module for-in cursor or live flags';
var values = [];
for (const value of ['a', 'b']) {
  await Promise.resolve();
  values.push(value);
  gc();
}
if (values.join('|') !== 'a|b') throw 'module for-of cursor';
print('module-iteration-await:ok');
"#,
        true,
        "module-iteration-await:ok",
    );
}
