use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("compact ForIn control executes compiled Wasm");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        [HostOutputEvent::PrintLine(expected.into())]
    );
}

#[test]
fn compact_for_in_preserves_string_order_and_array_index_boundaries() {
    run(
        r#"
function keys(object) {
  var result = [];
  for (var key in object) result.push(key);
  return result.join('|');
}
var compact = {z: 1, '-0': 2, '4294967295': 3, a: 4};
var indexed = {z: 1, '4294967295': 2, '2': 3, '-0': 4, '0': 5, '4294967294': 6, a: 7};
var symbol = Symbol('excluded');
indexed[symbol] = 8;
print(keys(compact) + ';' + keys(indexed) + ';' + keys({}));
"#,
        "z|-0|4294967295|a;0|2|4294967294|z|4294967295|-0|a;",
    );
}

#[test]
fn compact_for_in_value_updates_and_callbacks_remain_live_across_gc() {
    run(
        r#"
var first = {name: 'old'}, replacement = {name: 'new'};
var object = {first: first, second: first, last: first};
var callbacks = [], events = [];
function inspect(key, value) {
  gc();
  if (value !== object[key]) throw 'stale value';
  return key + ':' + value.name;
}
for (let key in object) {
  var value = object[key];
  callbacks.push(function () { gc(); return key + ':' + object[key].name; });
  events.push(inspect(key, value));
  if (key === 'first') {
    object.second = replacement;
    object.last = replacement;
  }
  gc();
}
gc();
events.push(callbacks[0](), callbacks[1](), callbacks[2]());
print(events.join('|'));
"#,
        "first:old|second:new|last:new|first:old|second:new|last:new",
    );
}

#[test]
fn compact_for_in_fallback_keeps_initial_snapshot_and_deleted_keys_unvisited() {
    run(
        r#"
var object = {first: 1, removed: 2, reinserted: 3, last: 4};
var parent = {first: 10, removed: 20, inherited: 30};
var seen = [];
for (var key in object) {
  seen.push(key + ':' + object[key]);
  if (key === 'first') {
    delete object.removed;
    delete object.reinserted;
    object.added = 5;
    object.reinserted = 6;
    Object.setPrototypeOf(object, parent);
    gc();
  }
}
print(seen.join('|'));
"#,
        "first:1|reinserted:6|last:4|removed:20|inherited:30",
    );
}

#[test]
fn compact_for_in_fallback_observes_live_attributes_without_reading_accessors() {
    run(
        r#"
var object = {first: 1, accessor: 2, hidden: 3, removed: 4, last: 5};
var seen = [], reads = 0;
for (var key in object) {
  seen.push(key);
  if (key === 'first') {
    Object.defineProperty(object, 'accessor', {
      get() { reads++; throw 'enumeration read value'; }, enumerable: true
    });
    Object.defineProperty(object, 'hidden', {enumerable: false});
    delete object.removed;
    Object.setPrototypeOf(object, {hidden: 30, removed: 40, inherited: 60});
    gc();
  }
}
print(seen.join('|') + ';reads:' + reads);
"#,
        "first|accessor|last|removed|inherited;reads:0",
    );
}

#[test]
fn compact_for_in_rechecks_object_prototype_after_first_and_final_own_yields() {
    run(
        r#"
var first = [], final = [];
try {
  for (var key in {first: 1, last: 2}) {
    first.push(key);
    if (key === 'first') Object.prototype.compactInherited = 3;
    gc();
  }
  delete Object.prototype.compactInherited;
  for (var key in {only: 1}) {
    final.push(key);
    Object.defineProperty(Object.prototype, 'compactAfterLast', {
      value: 4, enumerable: true, configurable: true
    });
    gc();
  }
} finally {
  delete Object.prototype.compactInherited;
  delete Object.prototype.compactAfterLast;
}
print(first.join('|') + ';' + final.join('|'));
"#,
        "first|last|compactInherited;only|compactAfterLast",
    );
}

#[test]
fn compact_for_in_rechecks_current_ordinary_prototype_and_visited_prefix() {
    run(
        r#"
var seen = [];
var object = {first: 1, last: 2};
for (var key in object) {
  seen.push(key);
  if (key === 'first') Object.setPrototypeOf(object, {stale: 1});
  if (key === 'last') Object.setPrototypeOf(object, {first: 3, last: 4, fresh: 5});
  gc();
}
var nullTail = {only: 1}, bare = [];
for (var key in nullTail) {
  bare.push(key);
  Object.setPrototypeOf(nullTail, null);
  gc();
}
print(seen.join('|') + ';' + bare.join('|'));
"#,
        "first|last|fresh;only",
    );
}

#[test]
fn compact_for_in_enters_replaced_proxy_prototype_with_lazy_trap_order() {
    run(
        r#"
var events = [], object = {first: 1, last: 2};
var parent = new Proxy({}, {
  ownKeys() { events.push('keys'); gc(); return ['missing', 'hidden', 'first', 'visible']; },
  getOwnPropertyDescriptor(target, key) {
    events.push('descriptor:' + key); gc();
    if (key === 'missing') return undefined;
    if (key === 'first') throw 'visited key descriptor';
    return {get() { throw 'enumeration read value'; }, enumerable: key === 'visible', configurable: true};
  },
  getPrototypeOf() { events.push('prototype'); gc(); return null; }
});
for (var key in object) {
  events.push('body:' + key);
  if (key === 'last') Object.setPrototypeOf(object, parent);
  gc();
}
print(events.join('|'));
"#,
        "body:first|body:last|keys|descriptor:missing|descriptor:hidden|descriptor:visible|body:visible|prototype",
    );
}

#[test]
fn compact_for_in_abrupt_bodies_keep_identity_and_lazy_prototype_observation() {
    run(
        r#"
var events = [], marker = {identity: 'throw'}, caught;
function callback(key) { gc(); events.push('callback:' + key); throw marker; }
var throwing = {first: 1, last: 2};
try {
  for (var key in throwing) callback(key);
} catch (error) { caught = error; }
if (caught !== marker) throw 'lost callback throw identity';
var breaking = {first: 1, last: 2};
var forbidden = new Proxy({}, {ownKeys() { throw 'break observed prototype'; }});
for (var key in breaking) {
  events.push('break:' + key);
  Object.setPrototypeOf(breaking, forbidden);
  gc();
  break;
}
var continuing = {first: 1, last: 2};
var parent = new Proxy({}, {
  ownKeys() { events.push('continued keys'); return ['inherited']; },
  getOwnPropertyDescriptor() { gc(); return {value: 3, enumerable: true, configurable: true}; },
  getPrototypeOf() { events.push('continued end'); return null; }
});
for (var key in continuing) {
  events.push('continue:' + key);
  if (key === 'first') Object.setPrototypeOf(continuing, parent);
  gc();
  continue;
}
events.push('identity:' + (caught === marker));
print(events.join('|'));
"#,
        "callback:first|break:first|continue:first|continue:last|continued keys|continue:inherited|continued end|identity:true",
    );
}

#[test]
fn generic_for_in_bitmap_boundaries_preserve_missing_hidden_and_symbol_slots() {
    run(
        r#"
function exercise(size) {
  var snapshot = [], parent = Object.create(null), absent = false;
  for (var index = 0; index < 60; index++) snapshot.push('k' + index);
  snapshot.push(Symbol('snapshot slot'), 'hidden', 'missing', 'edge');
  if (size === 65) snapshot.push('extra');
  var descriptors = 0, prototypes = 0, own = 0, inherited = [];
  var object = new Proxy({}, {
    ownKeys() { return snapshot; },
    getOwnPropertyDescriptor(target, key) {
      if (typeof key !== 'string') throw 'symbol descriptor observed';
      descriptors++;
      if (key === 'missing' && absent) return undefined;
      if (key === 'edge') gc();
      return {value: 1, enumerable: key !== 'hidden', configurable: true};
    },
    getPrototypeOf() { prototypes++; gc(); return parent; }
  });
  for (var key in object) {
    if (key === 'k0') {
      absent = true;
      parent.k0 = 10;
      parent.hidden = 20;
      parent.missing = 30;
      parent.edge = 40;
      parent.extra = 50;
      parent.tail = 60;
      gc();
    }
    if (key === 'hidden') throw 'nonenumerable shadow lost';
    if (key === 'missing' || key === 'tail' || (key === 'extra' && size === 64)) {
      inherited.push(key);
    } else {
      own++;
    }
  }
  return size + ':' + own + ':' + inherited.join('|') + ':' + descriptors + ':' + prototypes;
}
print(exercise(64) + ';' + exercise(65));
"#,
        "64:61:missing|extra|tail:63:1;65:62:missing|tail:64:1",
    );
}

#[test]
fn generic_for_in_dictionary_tail_observes_mutated_current_prototype() {
    run(
        r#"
var object = Object.create(null);
object.first = 1;
object.hidden = 2;
object.last = 3;
var seen = [];
for (var key in object) {
  seen.push(key);
  if (key === 'first') {
    Object.defineProperty(object, 'hidden', {enumerable: false});
    Object.setPrototypeOf(object, {stale: 1});
  }
  if (key === 'last') {
    object.added = 4;
    Object.setPrototypeOf(object, {first: 10, hidden: 20, last: 30, fresh: 40});
    gc();
  }
}
print(seen.join('|'));
"#,
        "first|last|fresh",
    );
}
