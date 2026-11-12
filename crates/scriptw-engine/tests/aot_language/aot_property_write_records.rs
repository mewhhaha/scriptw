//! A static assignment site records an existing own data property of a
//! compact shape and the add of its key by a shape transition. Every use must
//! re-prove the record; these scripts change, between two executions of the
//! same assignment, one fact a record depends on. Expected lines are Node's.
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
fn an_own_write_record_follows_the_receiver_shape_and_attributes() {
    // Two shapes reach one site; attribute changes, accessors and deletion move
    // the receiver off the recorded shape.
    run(
        r#"
var log = [];
function set(o, v) { o.b = v; return o.b; }
var a = { a: 1, b: 2 }, c = { b: 1, a: 2 };
log.push(set(a, 10), set(a, 11), set(c, 20), set(a, 12), a.a, c.a);
Object.defineProperty(a, "b", { writable: false });
log.push(set(a, 13), set({ a: 1, b: 2 }, 14));
log.push(set(Object.freeze({ a: 1, b: 2 }), 15), set(Object.seal({ a: 1, b: 2 }), 16));
log.push(set(Object.preventExtensions({ a: 1, b: 2 }), 17));
var h = { a: 1, b: 2 };
Object.defineProperty(h, "b", {
  get: function () { return "got"; },
  set: function (v) { log.push("own setter " + v); },
  configurable: true
});
log.push(set(h, 18));
var d = { a: 1, b: 2 };
delete d.b;
log.push(set(d, 19), Object.keys(d).join(""));
log.push(set(d, 20), Object.keys(d).join(""));
var e = { a: 1, b: 2 };
Object.defineProperty(e, "b", { enumerable: false });
log.push(set(e, 21), Object.keys(e).join(""));
print(log.join());
"#,
        "10,11,20,12,1,2,12,14,2,16,17,own setter 18,got,19,ab,20,ab,21,a",
    );
}

#[test]
fn a_refused_write_throws_only_in_strict_code() {
    // Freezing, sealing, preventExtensions and a read-only prototype property
    // refuse a write that the site has recorded.
    run(
        r#"
var log = [];
function strictSet(o, v) { "use strict"; o.b = v; return o.b; }
function strictAdd(o, v) { "use strict"; o.z = v; return o.z; }
function sloppyAdd(o, v) { o.z = v; return o.z; }
function attempt(f, o, v) {
  try { return f(o, v); } catch (error) { return error instanceof TypeError ? "TypeError" : "other"; }
}
var a = { a: 1, b: 2 };
log.push(strictSet(a, 3), strictSet(a, 4));
Object.freeze(a);
log.push(attempt(strictSet, a, 5), a.b);
log.push(strictAdd({ a: 1 }, 1), strictAdd({ a: 1 }, 2));
var sealed = Object.seal({ a: 1 });
log.push(attempt(strictAdd, sealed, 3), "z" in sealed);
var closed = Object.preventExtensions({ a: 1 });
log.push(attempt(strictAdd, closed, 4), "z" in closed);
log.push(strictAdd({ a: 1 }, 5));
log.push(sloppyAdd({ a: 1 }, 6), sloppyAdd({ a: 1 }, 7));
log.push(String(sloppyAdd(Object.preventExtensions({ a: 1 }), 8)), sloppyAdd({ a: 1 }, 9));
class Fixed { constructor(x) { this.x = x; } }
log.push(new Fixed(1).x, new Fixed(2).x);
Object.defineProperty(Fixed.prototype, "x", { value: "ro", writable: false, configurable: true });
log.push(attempt(function (unused, v) { return new Fixed(v).x; }, null, 3));
delete Fixed.prototype.x;
log.push(new Fixed(4).x);
print(log.join());
"#,
        "3,4,TypeError,4,1,2,TypeError,false,TypeError,false,5,6,7,undefined,9,1,2,TypeError,4",
    );
}

#[test]
fn an_add_record_follows_the_prototype_chain() {
    // Null prototypes, deletion and re-adding, replaced prototypes with a setter,
    // a read-only property or a Proxy, and an Object.prototype from another Realm.
    run(
        r#"
var log = [];
function add(o, v) { o.k = v; return Object.getOwnPropertyNames(o).join("") + ":" + o.k; }
log.push(add({}, 1), add({}, 2), add({ a: 0 }, 3), add({ b: 0 }, 4), add({ a: 0 }, 5));
log.push(add(Object.create(null), 6), add(Object.create(null), 7));
var x = {};
log.push(add(x, 8), add(x, 9));
delete x.k;
log.push(add(x, 10), add(x, 11));
var withSetter = { set k(v) { log.push("parent setter " + v); } };
var o = {};
Object.setPrototypeOf(o, withSetter);
log.push(add(o, 12));
var readOnly = Object.defineProperty({}, "k", { value: "ro", writable: false });
var r = {};
Object.setPrototypeOf(r, readOnly);
log.push(add(r, 13));
var trap = new Proxy({}, {
  set: function (target, key, value, receiver) {
    log.push("trap " + key + " " + value);
    return Reflect.set(target, key, value, receiver);
  }
});
var p = {};
Object.setPrototypeOf(p, trap);
log.push(add(p, 14));
var n = {};
Object.setPrototypeOf(n, null);
log.push(add(n, 15), add({}, 16));
var realm = __scriptwCreateRealm();
var foreign = realm.evalScript("(function () { return {}; })");
log.push(add(foreign(), 17), add(foreign(), 18));
realm.evalScript('Object.defineProperty(Object.prototype, "k", { set: function (v) { globalThis.seen = v; }, configurable: true });');
log.push(add(foreign(), 19), realm.global.seen, add({}, 20));
print(log.join());
"#,
        "k:1,k:2,ak:3,bk:4,ak:5,k:6,k:7,k:8,k:9,k:10,k:11,parent setter 12,:undefined,:ro,trap k 14,k:14,k:15,k:16,k:17,k:18,:undefined,19,k:20",
    );
}

#[test]
fn an_add_record_follows_object_prototype_changes() {
    // A setter or read-only property appears on %Object.prototype% between adds,
    // both directly above the receiver and above a constructor prototype.
    run(
        r#"
var log = [];
function add(o, v) { o.k = v; return Object.getOwnPropertyNames(o).join("") + ":" + o.k; }
function P(v) { this.k = v; }
log.push(add({}, 1), add({}, 2), new P(3).k, new P(4).k);
Object.defineProperty(Object.prototype, "k", {
  set: function (v) { log.push("setter " + v); },
  get: function () { return "inherited"; },
  configurable: true
});
log.push(add({}, 5), new P(6).k);
delete Object.prototype.k;
log.push(add({}, 7), new P(8).k);
Object.defineProperty(Object.prototype, "k", { value: "ro", writable: false, configurable: true });
log.push(add({}, 9), new P(10).k);
delete Object.prototype.k;
log.push(add({}, 11), new P(12).k);
print(log.join());
"#,
        "k:1,k:2,3,4,setter 5,setter 6,:inherited,inherited,k:7,8,:ro,ro,k:11,12",
    );
}

#[test]
fn an_add_record_follows_the_constructor_prototype() {
    // Changes to the holder between the receiver and %Object.prototype%: accessors,
    // read-only and writable data, replacement and its own prototype.
    run(
        r#"
var log = [];
function P(a, b) { this.a = a; this.b = b; }
P.prototype.m = function () { return this.a + this.b; };
function shape(o) { return Object.keys(o).join("") + "=" + o.a + "," + o.b; }
log.push(shape(new P(1, 2)), shape(new P(3, 4)), new P(5, 6).m());
Object.defineProperty(P.prototype, "b", {
  get: function () { return "proto b"; },
  set: function (v) { log.push("holder setter " + v); },
  configurable: true
});
log.push(shape(new P(7, 8)));
delete P.prototype.b;
log.push(shape(new P(9, 10)));
Object.defineProperty(P.prototype, "a", { value: "ro", writable: false, configurable: true });
log.push(shape(new P(11, 12)));
delete P.prototype.a;
P.prototype.a = "data";
log.push(shape(new P(13, 14)), P.prototype.a);
delete P.prototype.a;
log.push(shape(new P(15, 16)));
P.prototype = { m: function () { return 0; } };
log.push(shape(new P(17, 18)), shape(new P(19, 20)));
Object.setPrototypeOf(P.prototype, { set a(v) { log.push("grand setter " + v); } });
log.push(shape(new P(21, 22)));
Object.setPrototypeOf(P.prototype, new Proxy({}, {
  set: function (target, key, value, receiver) {
    log.push("trap " + key);
    return Reflect.set(target, key, value, receiver);
  }
}));
log.push(shape(new P(23, 24)));
Object.setPrototypeOf(P.prototype, Object.prototype);
log.push(shape(new P(25, 26)));
P.prototype.b = "compact holder data";
log.push(shape(new P(27, 28)));
Object.setPrototypeOf(P.prototype, null);
log.push(shape(new P(29, 30)));
var made = new P(31, 32);
Object.preventExtensions(made);
P.call(made, 33, 34);
log.push(shape(made));
print(log.join());
"#,
        "ab=1,2,ab=3,4,11,holder setter 8,a=7,proto b,ab=9,10,b=ro,12,ab=13,14,data,ab=15,16,ab=17,18,ab=19,20,grand setter 21,b=undefined,22,trap a,trap b,ab=23,24,ab=25,26,ab=27,28,ab=29,30,ab=33,34",
    );
}

#[test]
fn write_records_keep_receiver_and_key_rules() {
    // Arrays, functions, wrappers, primitives, "length", "__proto__", index keys,
    // super assignment, Reflect.set receivers and own getters.
    run(
        r#"
var log = [];
function setK(o, v) { o.k = v; return o.k; }
log.push(setK({}, 1), setK({ k: 0 }, 2), setK({}, 3), setK({ k: 0 }, 4));
var arr = [1, 2];
log.push(setK(arr, 5), setK(arr, 6), Object.keys(arr).join());
function fn() {}
log.push(setK(fn, 7), setK(fn, 8), Object.keys(fn).join());
var boxed = new String("s");
log.push(setK(boxed, 9), Object.keys(boxed).join());
log.push(String(setK("primitive", 10)));
function setLength(o, v) { o.length = v; return o.length; }
log.push(setLength([1, 2, 3], 1), setLength({}, 2), setLength({}, 3));
function setProto(o, v) { o.__proto__ = v; return Object.getPrototypeOf(o) === v; }
var target = {};
log.push(setProto({}, target), setProto({}, target));
var bare = Object.create(null);
setProto(bare, target);
log.push(Object.getPrototypeOf(bare) === null, bare.__proto__ === target);
function setZero(o, v) { o["0"] = v; return Object.keys(o).join(); }
log.push(setZero({ a: 1 }, 1), setZero({ a: 1 }, 2));
class Base {}
class Derived extends Base { viaSuper(v) { super.k = v; return Object.keys(this).join() + ":" + this.k; } }
var instance = new Derived();
log.push(instance.viaSuper(11), instance.viaSuper(12));
var receiver = {};
log.push(Reflect.set({}, "k", 13, receiver), receiver.k, Reflect.set({}, "k", 14, receiver), receiver.k);
var getterOnly = Object.defineProperty({}, "k", { get: function () { return "g"; }, configurable: true });
log.push(setK(getterOnly, 15));
print(log.join());
"#,
        "1,2,3,4,5,6,0,1,k,7,8,k,9,0,k,undefined,1,2,3,true,true,true,true,0,a,0,a,k:11,k:12,true,13,true,14,g",
    );
}

#[test]
fn a_recorded_store_allocates_value_backing_only_through_the_complete_store() {
    // A shape whose slots hold only undefined has neither scalar nor reference
    // backing yet: the first number and the first object stored through the
    // recorded slot must allocate it, and later null and zero overwrites must
    // retire the old payloads. Inline (one or two keys) and array storage share
    // one site; reads go through static read records of their own.
    run(
        r#"
var log = [];
function set(o, v) { o.c = v; }
function get(o) { return o.c; }
var wide = { a: undefined, b: undefined, c: undefined };
var narrow = { c: undefined };
set(wide, undefined); set(narrow, undefined); set(wide, undefined);
log.push(String(get(wide)), String(get(narrow)));
set(wide, 5); set(narrow, 6);
log.push(get(wide), get(narrow), String(wide.a), String(wide.b));
set(wide, "text"); set(narrow, { k: "object" });
log.push(get(wide), get(narrow).k);
set(wide, null); set(narrow, 0);
log.push(String(get(wide)), get(narrow));
set(wide, -0); set(narrow, NaN);
log.push(1 / get(wide), String(get(narrow)));
set(wide, true); set(narrow, 7.5);
log.push(get(wide), get(narrow), Object.keys(wide).join(), Object.keys(narrow).join());
print(log.join());
"#,
        "undefined,undefined,5,6,undefined,undefined,text,object,null,0,-Infinity,NaN,true,7.5,a,b,c,c",
    );
}
