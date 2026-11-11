//! A static read site records a data property found on the receiver's direct
//! prototype, and the position of an own dictionary entry. Every use must
//! re-prove the record; these scripts change each fact a record depends on
//! between two uses of the same site.
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
        .expect("the script executes compiled Wasm");
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
fn a_prototype_record_follows_changes_to_the_holder() {
    // Replaced value, redefinition as an accessor (which sees the receiver),
    // deletion (the grandparent answers), and a later own shadow.
    run(
        r#"
var log = [];
function read(o) { return o.m; }
var grand = { m: "grand" };
var proto = Object.create(grand);
proto.m = "first";
var o = Object.create(proto);
o.own = 1;
log.push(read(o), read(o));
proto.m = "second";
log.push(read(o));
Object.defineProperty(proto, "m", { get: function () { return this === o ? "getter" : "wrong"; }, configurable: true });
log.push(read(o), read(o));
delete proto.m;
log.push(read(o));
delete grand.m;
log.push(String(read(o)));
proto.m = "again";
log.push(read(o));
o.m = "shadow";
log.push(read(o));
delete o.m;
log.push(read(o));
print(log.join());
"#,
        "first,first,second,getter,getter,grand,undefined,again,shadow,again",
    );
}

#[test]
fn a_prototype_record_follows_the_receiver_shape_and_prototype() {
    run(
        r#"
var log = [];
function read(o) { return o.m; }
var a = { m: "a" }, b = { m: "b" };
var o = Object.create(a);
log.push(read(o), read(o));
Object.setPrototypeOf(o, b);
log.push(read(o), read(o));
Object.setPrototypeOf(o, null);
log.push(String(read(o)));
Object.setPrototypeOf(o, a);
log.push(read(o));
// Receivers of two literal shapes and one with the property itself share the site.
var p = { x: 1, __proto__: a }, q = { y: 2, __proto__: b }, r = { m: "own", __proto__: a };
log.push(read(p), read(q), read(p), read(r), read(q), read(r));
// A Proxy prototype keeps its trap.
var trapped = Object.create(new Proxy({}, { get: function (t, k, receiver) { return k === "m" ? "trap" : undefined; } }));
log.push(read(trapped), read(o), read(trapped));
print(log.join());
"#,
        "a,a,b,b,undefined,a,a,b,a,own,b,own,trap,a,trap",
    );
}

#[test]
fn builtin_prototype_records_follow_replaced_methods_and_exotic_own_keys() {
    run(
        r#"
var log = [];
function index(s) { return s.indexOf; }
function len(s) { return s.length; }
function first(s) { return s[0]; }
var original = String.prototype.indexOf;
log.push(index("ab") === original, index("cd") === original, index(new String("x")) === original);
String.prototype.indexOf = function replaced() {};
log.push(index("ab") === original, index("ab").name);
String.prototype.indexOf = original;
log.push(index("zz") === original);
log.push(len("abc"), len([1, 2]), len(new String("four")), first("q"), first([7]));
function fixed(n) { return n.toFixed; }
log.push(fixed(1) === Number.prototype.toFixed, fixed(2.5) === Number.prototype.toFixed);
Number.prototype.toFixed = 5;
log.push(fixed(1));
function push(a) { return a.push; }
var arr = [1];
log.push(push(arr) === Array.prototype.push, push([]) === Array.prototype.push);
arr.push = "own";
log.push(push(arr), push([]) === Array.prototype.push);
function get(m) { return m.get; }
var map = new Map(), weak = new WeakMap();
log.push(get(map) === Map.prototype.get, get(map) === Map.prototype.get, get(weak) === WeakMap.prototype.get, get(map) === Map.prototype.get);
map.get = 1;
log.push(get(map), get(new Map()) === Map.prototype.get);
function call(f) { return f.call; }
function plain() {}
log.push(call(plain) === Function.prototype.call, call(plain) === Function.prototype.call);
plain.call = 2;
log.push(call(plain), call(function () {}) === Function.prototype.call);
print(log.join());
"#,
        "true,true,true,false,replaced,true,3,2,4,q,7,true,true,5,true,true,own,true,true,true,true,true,1,true,true,true,2,true",
    );
}

#[test]
fn a_dictionary_position_follows_deletion_and_reinsertion() {
    run(
        r#"
var log = [];
function read(o) { return o.k; }
function Made(order) {
  if (order) { this.a = 1; this.k = "ak"; } else { this.k = "k"; this.a = 1; }
}
var x = new Made(true), y = new Made(false);
log.push(read(x), read(y), read(x), read(y));
x.k = "updated";
log.push(read(x));
delete x.k;
log.push(String(read(x)));
x.b = 2;
x.k = "back";
log.push(read(x), read(y));
Object.defineProperty(y, "k", { get: function () { return "getter"; } });
log.push(read(y), read(x));
var z = {};
for (var i = 0; i < 40; i++) z["p" + i] = i;
z.k = "late";
log.push(read(z), read(x), read(z));
print(log.join());
"#,
        "ak,k,ak,k,updated,undefined,back,k,getter,back,late,back,late",
    );
}

#[test]
fn an_absence_record_follows_additions_anywhere_on_the_chain() {
    run(
        r#"
var log = [];
function read(o) { return o.absentKey; }
function tag(o) { return o.$$tag; }
function plain() {}
var arr = [1], map = new Map(), empty = {}, shaped = { a: 1 };
log.push(String(read("text")), String(read("more")), String(read(7)), String(tag(plain)), String(tag(plain)));
log.push(String(tag(arr)), String(tag(arr)), String(tag(map)), String(tag(empty)), String(tag(shaped)), String(tag(shaped)));
String.prototype.absentKey = "string";
log.push(read("text"), String(read(7)));
delete String.prototype.absentKey;
Object.prototype.absentKey = "object";
log.push(read("text"), read(7), read(plain));
delete Object.prototype.absentKey;
log.push(String(read("text")));
Function.prototype.$$tag = "function";
log.push(tag(plain), String(tag(arr)), String(tag(shaped)));
delete Function.prototype.$$tag;
Object.defineProperty(Object.prototype, "$$tag", { get: function () { return typeof this; }, configurable: true });
log.push(tag(plain), tag(arr), tag(map), tag(empty), tag(shaped));
delete Object.prototype.$$tag;
log.push(String(tag(plain)), String(tag(arr)));
arr.$$tag = "own";
plain.$$tag = "ownfn";
log.push(tag(arr), tag(plain), String(tag([])), String(tag(function () {})));
print(log.join());
"#,
        "undefined,undefined,undefined,undefined,undefined,undefined,undefined,undefined,undefined,undefined,undefined,string,undefined,object,object,object,undefined,function,undefined,undefined,function,object,object,object,object,undefined,undefined,own,ownfn,undefined,undefined",
    );
}
