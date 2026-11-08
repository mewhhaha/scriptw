use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let result = Engine::new(RealmBuilder::new().build())
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
        .expect("cached input contract executes compiled Wasm");
    assert_eq!(result.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(result.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        result.completion
    );
    assert_eq!(
        result.output_events,
        [HostOutputEvent::PrintLine(expected.into())]
    );
}

#[test]
fn cached_input_narrowing_preserves_primitive_receivers_keys_and_abrupt_order() {
    for directive in ["", "\"use strict\";\n"] {
        run(
            &format!(
                "{directive}{}",
                include_str!("../fixtures/cached_read_inputs.js")
            ),
            "cached-inputs:ok",
        );
    }
}

#[test]
fn cached_input_fallback_keeps_execution_and_getter_realms() {
    run(
        r#"
function same(actual, expected) {
  if (!Object.is(actual, expected)) throw new Error('unexpected value identity');
}
function throwsConstructor(expected, callback) {
  try { callback(); }
  catch (error) {
    if (error.constructor === expected) return;
    throw new Error('unexpected error constructor');
  }
  throw new Error('expected an exception');
}
var realm = __scriptwCreateRealm(), foreign = realm.global;
var foreignRead = realm.evalScript('(function(value, unused, extra) { return value.field; })');
var foreignGetter = realm.evalScript('(function() { throw new TypeError("foreign getter"); })');
var object = {};
Object.defineProperty(object, 'field', {get: foreignGetter});
function localRead(value, unused, extra) { return value.field; }
throwsConstructor(foreign.TypeError, function() { localRead(object); });
throwsConstructor(TypeError, function() { localRead(null); });
throwsConstructor(foreign.TypeError, function() { foreignRead(null); });
realm.evalScript('Object.defineProperty(Number.prototype, "field", {configurable: true, get: function() { "use strict"; return this; }});');
same(foreignRead(11), 11);
same(localRead(11), undefined);
print('cached-input-realms:ok');
"#,
        "cached-input-realms:ok",
    );
}

#[test]
fn compact_cache_hits_reload_live_values_storage_and_prototypes_after_callbacks() {
    run(
        r#"
function same(actual, expected, label) {
  if (!Object.is(actual, expected)) throw new Error(label);
}
function read(record, unused, extra) { return record.then; }
function visit(record, callback) { callback(record); return record.then; }
var record = {then: 1}, symbol = Symbol('live'), object = {};
for (var value of [1, -0, NaN, undefined, null, symbol, object, 'text', 3n]) {
  record.then = value;
  same(read(record), value, 'live inline value');
  gc();
  same(read(record), value, 'value survives collection');
}
var wide = {a: 1, b: 2, then: 3, c: 4};
same(read(wide), 3, 'array-backed compact value');
wide.then = object;
gc();
same(read(wide), object, 'array-backed reference replacement');
Object.defineProperty(record, 'then', {writable: false});
same(read(record), 3n, 'attribute transition preserves value');
var receiver;
Object.defineProperty(record, 'then', {configurable: true, get() { receiver = this; return 7; }});
same(read(record), 7, 'data-to-accessor invalidates own hit');
same(receiver, record, 'original getter receiver');
delete record.then;
Object.setPrototypeOf(record, {then: 8});
same(read(record), 8, 'deletion exposes inherited value');
var callbackRecord = {then: 9};
same(visit(callbackRecord, function(o) { o.then = 10; }), 10, 'callback first own value');
same(visit(callbackRecord, function(o) {
  Object.defineProperty(o, 'then', {get() { gc(); return 11; }});
}), 11, 'callback invalidates warmed storage before hit');
print('compact-cache-live:ok');
"#,
        "compact-cache-live:ok",
    );
}

#[test]
fn compact_negative_cache_revalidates_closed_prototype_tails_without_exotic_shortcuts() {
    run(
        r#"
function same(actual, expected, label) {
  if (!Object.is(actual, expected)) throw new Error(label);
}
function read(record, unused, extra) { return record.then; }
function shaped(value) { var record = {value: value}; return record.then; }
var plain = {value: 1};
for (var i = 0; i < 3; i++) {
  same(read(plain), undefined, 'canonical Object.prototype negative');
  same(shaped(i), undefined, 'known absent source shape');
}
var bare = {value: 1};
Object.setPrototypeOf(bare, null);
same(read(bare), undefined, 'null prototype negative registers');
gc();
same(read(bare), undefined, 'null prototype negative survives GC');
var receiver, getterCalls = 0;
Object.setPrototypeOf(bare, {get then() { receiver = this; getterCalls++; return 12; }});
same(read(bare), 12, 'new prototype invalidates null-tail hit');
same(receiver, bare, 'new inherited accessor receiver');
Object.defineProperty(Object.prototype, 'then', {
  configurable: true, get() { receiver = this; getterCalls++; return 13; }
});
same(read(plain), 13, 'intrinsic prototype mutation invalidates negative');
same(receiver, plain, 'intrinsic getter receiver');
delete Object.prototype.then;
same(read(plain), undefined, 'promoted prototype storage keeps full Get');
var trapCalls = 0;
var proxy = new Proxy({value: 1}, {get(target, key, actual) {
  trapCalls++;
  same(key, 'then', 'proxy key');
  same(actual, proxy, 'proxy receiver');
  return 14;
}});
same(read(proxy), 14, 'actual Proxy rejects negative fast path');
same(trapCalls, 1, 'one Proxy Get');
function callable() {}
Object.setPrototypeOf(callable, null);
same(read(callable), undefined, 'function own miss');
Object.setPrototypeOf(callable, {then: 15});
same(read(callable), 15, 'function miss cannot become ordinary negative');
var boxed = new String('abc');
Object.setPrototypeOf(boxed, null);
same(read(boxed), undefined, 'primitive box own miss');
Object.setPrototypeOf(boxed, {then: 16});
same(read(boxed), 16, 'box miss retains prototype lookup');
same(boxed[1], 'b', 'boxed String virtual indexed value');
var foreign = __scriptwCreateRealm();
var foreignObject = foreign.evalScript('({value: 1})');
same(read(foreignObject), undefined, 'foreign canonical closed tail');
gc();
foreign.evalScript('Object.defineProperty(Object.prototype, "then", {get() { return 17; }});');
same(read(foreignObject), 17, 'foreign prototype mutation invalidates negative');
same(read(plain), undefined, 'foreign mutation stays in defining Realm');
same(getterCalls, 2, 'negative hits never invoke extra getters');
print('compact-cache-negative:ok');
"#,
        "compact-cache-negative:ok",
    );
}

#[test]
fn warmed_cache_sites_survive_page_growth_gc_and_early_and_late_mutation() {
    // Cache leaves contain 64 sites. Distinct static reads with a three-key
    // cycle make an incorrect page/slot alias observable on the same shape.
    const SITES: usize = 96;
    const EARLY: usize = 16;
    let mut source = String::from(
        r#"
'use strict';
var records = [];
for (var index = 0; index < 96; index++) records.push({left: index + 1, right: 1000 + index});
if (Object.getPrototypeOf(records[0]) !== Object.prototype) throw 'initial prototype';
function check(actual, expected) { if (actual !== expected) throw new Error('checksum ' + actual + ' expected ' + expected); }
function checksum(records, includeLate) {
  var total = 0;
"#,
    );
    let mut early = 0;
    let mut complete = 0;
    for index in 0..SITES {
        if index == EARLY {
            source.push_str("if (includeLate) {\n");
        }
        let (key, value) = match index % 3 {
            0 => ("left", index + 1),
            1 => ("right", 1000 + index),
            2 => ("missing", 0),
            _ => unreachable!(),
        };
        source.push_str(&format!(
            "var value{index} = records[{index}].{key}; if (value{index} !== undefined) total += value{index};\n"
        ));
        complete += value;
        if index < EARLY {
            early += value;
        }
    }
    source.push_str("}\nreturn total;\n}\n");
    let changed =
        complete - 1 - 1064 - 1001 - 64 - 1094 + 5000 + 6000 + 7000 + 8000 + 9000 + 10000 + 11000;
    let changed_early = early - 1 - 1001 + 5000 + 9000 + 7000;
    source.push_str(&format!(
        r#"
var first = checksum(records, false);
check(first, {early});
check(checksum(records, false), {early});
var grown = checksum(records, true);
check(grown, {complete});
check(checksum(records, true), {complete});
check(checksum(records, false), {early});
gc();
check(checksum(records, true), {complete});
records[0].left = 5000;
records[64].right = 6000;
records[2].missing = 7000;
delete records[1].right;
Object.setPrototypeOf(records[1], {{right: 9000}});
var getterCalls = 0;
Object.defineProperty(records[63], 'left', {{get() {{
  if (this !== records[63]) throw 'data getter receiver';
  getterCalls++;
  return 10000;
}}}});
Object.setPrototypeOf(records[95], {{get missing() {{
  if (this !== records[95]) throw 'late getter receiver';
  getterCalls++;
  return 8000;
}}}});
records[94] = {{left: 95, right: 11000}};
gc();
var changed = checksum(records, true);
check(changed, {changed});
var changedEarly = checksum(records, false);
check(changedEarly, {changed_early});
gc();
check(checksum(records, true), {changed});
check(getterCalls, 4);
print('compact-cache-pages:' + first + ',' + grown + ',' + changed + ',' + changedEarly);
"#
    ));
    run(
        &source,
        &format!("compact-cache-pages:{early},{complete},{changed},{changed_early}"),
    );
}
