//! Realm protectors answer property reads and packed pushes only while the
//! intrinsic prototypes they record are unchanged; every mutation that
//! could falsify one, in this Realm or another, takes the complete proof.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion,
    ObservedJsValue, RealmBuilder, RunOptions,
};

fn assert_wasm_true(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    for directive in ["", "'use strict';\n"] {
        let source =
            format!("{directive}var $262 = {{ createRealm: __scriptwCreateRealm }};\n{source}");
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
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
            .expect("protector fixture compiles and executes through Wasm AOT");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(ObservedJsValue::Boolean(true)),
            "{source}"
        );
    }
}

#[test]
fn absent_key_reads_follow_object_prototype_edits() {
    assert_wasm_true(
        r#"
function read(o) { return o.missing; }
var o = { a: 1 }, results = [];
results.push(read(o), read(o), read(o));
Object.prototype.missing = 42;
results.push(read(o), read(o));
delete Object.prototype.missing;
results.push(read(o), read(o));
Object.defineProperty(Object.prototype, 'missing', {
  get: function () { return this === o ? 'getter' : 'other'; }, configurable: true
});
results.push(read(o));
delete Object.prototype.missing;
results.push(read(o));
Reflect.defineProperty(Object.prototype, 'missing', { value: 'reflect', configurable: true });
results.push(read(o));
Reflect.deleteProperty(Object.prototype, 'missing');
results.push(read(o));
o.__proto__ = { missing: 'own chain' };
results.push(read(o));
Object.setPrototypeOf(o, null);
results.push(read(o));
var immutable = false;
try { Object.prototype.__proto__ = { missing: 'never' }; } catch (e) { immutable = e instanceof TypeError; }
var reflected = Reflect.setPrototypeOf(Object.prototype, { missing: 'never' });
results.join() === ',,,42,42,,,getter,,reflect,,own chain,' &&
  results[0] === undefined && results[5] === undefined && results[12] === undefined &&
  immutable && reflected === false && read({}) === undefined;
"#,
    );
}

#[test]
fn function_and_string_prototype_places_follow_their_edits() {
    assert_wasm_true(
        r#"
function id(x) { return x; }
function callIt(f) { return f.call(null, 2); }
function lacks(f) { return f.zz; }
function first(s) { return s.at(0); }
var results = [callIt(id), callIt(id), lacks(id), lacks(id), first('ab'), first('ab')];
var call = Function.prototype.call;
Function.prototype.call = function () { return 'assigned'; };
results.push(callIt(id));
Object.defineProperty(Function.prototype, 'call', {
  value: function () { return 'defined'; }, writable: true, configurable: true
});
results.push(callIt(id));
delete Function.prototype.call;
results.push(typeof id.call);
Object.defineProperty(Function.prototype, 'call', { value: call, writable: true, configurable: true });
results.push(callIt(id));
Object.setPrototypeOf(Function.prototype, { zz: 'parent' });
results.push(lacks(id));
Object.setPrototypeOf(Function.prototype, Object.prototype);
results.push(lacks(id));
var at = String.prototype.at;
String.prototype.at = function () { return 'patched'; };
results.push(first('ab'));
String.prototype.at = at;
results.push(first('ab'));
results.join() === '2,2,,,a,a,assigned,defined,undefined,2,parent,,patched,a';
"#,
    );
}

#[test]
fn packed_push_observes_indexed_prototype_properties_and_chain_edits() {
    assert_wasm_true(
        r#"
function pushIt(a, v) { return a.push(v); }
var calls = [], warm = [];
pushIt(warm, 1); pushIt(warm, 2);
Object.defineProperty(Array.prototype, '2', {
  set: function (v) { calls.push('array:' + v); }, configurable: true
});
var b = [0, 1];
var bLength = pushIt(b, 9);
delete Array.prototype[2];
var c = [0, 1, 2];
Object.defineProperty(Object.prototype, '3', {
  set: function (v) { calls.push('object:' + v); }, configurable: true
});
var cLength = pushIt(c, 7);
delete Object.prototype[3];
var hook = {};
Object.defineProperty(hook, '1', {
  set: function (v) { calls.push('hook:' + v); }, configurable: true
});
Object.setPrototypeOf(Array.prototype, hook);
var d = [0];
var dLength = pushIt(d, 5);
Object.setPrototypeOf(Array.prototype, Object.prototype);
var e = [0];
var eLength = pushIt(e, 6);
bLength === 3 && b.length === 3 && !b.hasOwnProperty(2) &&
  cLength === 4 && c.length === 4 && !c.hasOwnProperty(3) &&
  dLength === 2 && d.length === 2 && !d.hasOwnProperty(1) &&
  eLength === 2 && e[1] === 6 && e.hasOwnProperty(1) &&
  calls.join() === 'array:9,object:7,hook:5' && warm.join() === '1,2';
"#,
    );
}

#[test]
fn another_realms_prototypes_are_never_answered_by_this_realms_protectors() {
    assert_wasm_true(
        r#"
var foreign = $262.createRealm().global;
function read(o) { return o.ww; }
function pushIt(a, v) { return a.push(v); }
var fo = new foreign.Object(), lo = {}, results = [read(fo), read(fo), read(lo), read(lo)];
foreign.Object.prototype.ww = 'foreign';
results.push(read(fo), read(lo));
var calls = 0, fa = new foreign.Array();
pushIt(fa, 1); pushIt([], 1);
Object.defineProperty(foreign.Array.prototype, '1', {
  set: function () { calls++; }, configurable: true
});
var faLength = pushIt(fa, 2), la = [0], laLength = pushIt(la, 3);
results.join() === ',,,,foreign,' && results[5] === undefined &&
  faLength === 2 && fa.length === 2 && !fa.hasOwnProperty(1) && calls === 1 &&
  laLength === 2 && la[1] === 3;
"#,
    );
}
