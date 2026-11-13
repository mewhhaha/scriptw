//! Each static property site keeps its record in a global of P and answers
//! a recorded own compact slot inline; a compound, logical or update
//! assignment reads through such a site before its write. Every use must
//! re-prove the record against the live receiver; these scripts change, between
//! two executions of one site, the receiver's storage layout, shape,
//! attributes, prototype and kind. The computed-key scripts do the same to
//! one `o[k]` site's own-data probe. Expected lines are Node's.
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
fn a_read_site_follows_layout_shape_kind_and_prototype() {
    // Inline and array storage, a function, primitives, an inherited value,
    // deletion and re-adding, an accessor, value kinds that change the array
    // backings, a Proxy and nullish bases reach one site.
    run(
        r#"
var log = [];
function read(o) { return o.k; }
var one = { k: 1 }, two = { a: 0, k: 2 }, many = { a: 0, b: 0, k: 3 };
log.push(read(one), read(two), read(many), read(one), read(many));
var fn = function () {}; fn.k = "f";
log.push(read(fn), read(many), read("str"), read(5));
String.prototype.k = "sp"; Number.prototype.k = "np";
log.push(read("str"), read(5));
delete String.prototype.k; delete Number.prototype.k;
var proto = { k: "inherited" };
var child = Object.create(proto); child.a = 1;
log.push(read(child), read(many));
delete many.k;
log.push(read(many));
many.k = "readded";
log.push(read(many));
Object.defineProperty(many, "k", { get: function () { return "getter"; }, configurable: true });
log.push(read(many));
var big = { a: 0, b: 0, c: 0, d: 0, k: "s" };
log.push(read(big));
big.k = 7.5;
log.push(read(big));
big.k = null;
log.push(String(read(big)));
var p = new Proxy({ k: "target" }, { get: function (t, key) { return "trap " + String(key); } });
log.push(read(p), read(one));
try { read(null); } catch (e) { log.push(e instanceof TypeError); }
try { read(undefined); } catch (e) { log.push(e instanceof TypeError); }
print(log.join());
"#,
        "1,2,3,1,3,f,3,,,sp,np,inherited,3,,readded,getter,s,7.5,null,trap k,1,true,true",
    );
}

#[test]
fn an_assignment_site_stores_only_what_its_record_proves() {
    // Values that need an absent array backing, a nullish base after its
    // right-hand side, a primitive with an inherited setter, and freezing or a
    // read-only attribute after the site recorded the slot.
    run(
        r#"
var log = [];
function set(o, v) { o.k = v; return o.k; }
var one = { k: 1 }, many = { a: 0, b: 0, k: 3 };
log.push(set(one, 10), set(many, 30), set(one, 11), set(many, "s"), set(one, 12));
var strs = { a: "x", b: "y", k: "z" };
log.push(set(strs, 1.5), set(strs, "back"), strs.a);
var nums = { a: 1, b: 2, k: 3 };
log.push(set(nums, "ref"), set(nums, 4), String(set(nums, null)), set(nums, undefined) === undefined, nums.a);
function setNull(o) { o.k = (log.push("rhs"), 1); }
try { setNull(null); } catch (e) { log.push(e instanceof TypeError); }
Object.defineProperty(Number.prototype, "k", { set: function (v) { log.push("num setter " + typeof this + " " + v); }, configurable: true });
set(5, 6);
delete Number.prototype.k;
var f = { a: 0, b: 0, k: 0 };
set(f, 1); Object.freeze(f);
log.push(set(f, 2));
var g = { k: 0 };
set(g, 1); Object.defineProperty(g, "k", { writable: false });
log.push(set(g, 2), set(one, 13));
function strictSet(o, v) { "use strict"; o.k = v; return o.k; }
var h = { a: 0, k: 0 };
log.push(strictSet(h, 1));
Object.defineProperty(h, "k", { writable: false });
try { strictSet(h, 2); } catch (e) { log.push(e instanceof TypeError, h.k); }
print(log.join());
"#,
        "10,30,11,s,12,1.5,back,x,ref,4,null,true,1,rhs,true,num setter object 6,1,1,13,1,true,1",
    );
}

#[test]
fn compound_assignments_read_through_a_static_site_before_writing() {
    // Update, compound and logical assignments on compact receivers, accessors,
    // a String value, nullish bases (GetValue throws before the right-hand
    // side), a primitive with inherited accessors, and a getter that changes
    // the receiver's shape between the read and the write.
    run(
        r#"
var log = [];
function inc(o) { return o.k++; }
function addTo(o, v) { o.k += v; return o.k; }
function orSet(o, v) { return o.k ||= v; }
var c = { a: 0, k: 1 };
log.push(inc(c), inc(c), c.k, addTo(c, 10), orSet(c, 99), orSet({ k: 0 }, 5));
var big = { a: 0, b: 0, c: 0, k: 1 };
log.push(inc(big), addTo(big, 0.5), orSet(big, 7));
var acc = { get k() { log.push("get"); return 1; }, set k(v) { log.push("set " + v); } };
inc(acc); addTo(acc, 2); orSet(acc, 3);
var s = { a: 0, k: "1" };
log.push(inc(s), typeof s.k, s.k);
try { inc(null); } catch (e) { log.push(e instanceof TypeError); }
function addNull(o) { o.k += (log.push("rhs"), 1); }
try { addNull(undefined); } catch (e) { log.push(e instanceof TypeError); }
Object.defineProperty(Number.prototype, "k", { get: function () { log.push("nget " + typeof this); return 41; }, set: function (v) { log.push("nset " + typeof this + " " + v); }, configurable: true });
inc(5); addTo(5, 1);
delete Number.prototype.k;
var shift = { a: 0, b: 0 };
Object.defineProperty(shift, "k", { get: function () { delete shift.k; shift.z = 1; return 5; }, configurable: true });
log.push(addTo(shift, 1), Object.keys(shift).join(""));
print(log.join());
"#,
        "1,2,3,13,13,5,1,2.5,2.5,get,set 2,get,set 3,get,get,1,number,2,true,true,nget object,nset object 42,nget object,nset object 42,nget object,6,abzk",
    );
}

#[test]
fn a_computed_read_site_follows_key_identity_shape_and_kind() {
    // One computed site sees keys of other identities and shapes, deletion
    // and re-adding, an accessor, prototype data, Arrays, Strings, a Symbol,
    // a Proxy and a function between two of its executions.
    run(
        r#"
var log = [];
function get(o, k) { return o[k]; }
var a = { x: 1, y: 2, z: 3 }, b = { y: "b" };
log.push(get(a, "x"), get(a, "x"), get(a, "y"), get(b, "y"), get(a, "y"));
var k2 = ["x", ""].join("");
log.push(get(a, k2), get(a, "x"));
a.x = 10;
log.push(get(a, "x"));
delete a.x;
log.push(String(get(a, "x")));
a.x = 11;
log.push(get(a, "x"), get(a, "z"));
Object.defineProperty(a, "z", { get: function () { return "gz"; }, configurable: true });
log.push(get(a, "z"));
var proto = { w: "pw" };
var c = Object.create(proto);
c.v = 1;
log.push(get(c, "w"), get(c, "v"), get(c, "w"));
proto.w = "pw2";
log.push(get(c, "w"));
log.push(get([5, 6], 1), get("str", 1), get("str", "length"));
var sym = Symbol("s");
var d = {};
d[sym] = "sv";
log.push(get(d, sym));
log.push(get({ x: 1 }, "x"), get({ x: 1, y: 2 }, "x"), get({ y: 0, x: 3 }, "x"));
var p = new Proxy({ x: "target" }, { get: function () { return "trap"; } });
log.push(get(p, "x"));
var f = function () {};
f.x = "fx";
log.push(get(f, "x"), get(a, "x"));
print(log.join());
"#,
        "1,1,2,b,2,1,1,10,undefined,11,3,gz,pw,1,pw,pw2,6,t,3,sv,1,1,3,trap,fx,11",
    );
}

#[test]
fn sites_follow_the_receiver_between_a_read_and_its_write() {
    // A compound assignment whose right-hand side deletes, freezes, makes
    // read-only, converts or turns into an accessor the property it read; a
    // valueOf that changes the shape between the read and the write; a plain
    // assignment whose right-hand side rebinds the base variable; strict and
    // sloppy writes to primitives; sealed and non-extensible receivers; an
    // own data property shadowing an inherited setter; and two objects of one
    // shape of which one turns the property into an accessor.
    run(
        r#"
var log = [];
function addCall(o, f) { o.k += f(o); return o.k; }
var a = { a: 0, k: 1 };
log.push(addCall(a, function (o) { delete o.k; return 10; }), Object.keys(a).join(""));
var b = { a: 0, b: 0, k: 2 };
log.push(addCall(b, function (o) { Object.freeze(o); return 10; }), b.k);
function strictAdd(o, f) { "use strict"; o.k += f(o); return o.k; }
var c = { k: 3 };
try { strictAdd(c, function (o) { Object.defineProperty(o, "k", { writable: false }); return 1; }); } catch (e) { log.push(e instanceof TypeError, c.k); }
var d = { a: 0, k: 4 };
log.push(addCall(d, function (o) { for (var i = 0; i < 40; i++) o["p" + i] = i; return 1; }), d.k);
var e = { a: 0, k: 5 };
log.push(addCall(e, function (o) { Object.defineProperty(o, "k", { get: function () { return "g"; }, set: function (v) { log.push("set " + v); }, configurable: true }); return 1; }));
var v = { a: 0, k: 6 };
v.k += { valueOf: function () { delete v.k; v.z = 1; return 1; } };
log.push(v.k, Object.keys(v).join(""));
function setTo(o, x) { o.k = x; }
var w = { a: 0, k: 7 }, other = { a: 0, k: 70 };
var holder = w;
holder.k = (holder = other, 8);
log.push(w.k, other.k);
function strictSet(o, x) { "use strict"; o.k = x; return o.k; }
try { strictSet("str", 1); } catch (err) { log.push(err instanceof TypeError); }
try { strictSet(5, 1); } catch (err) { log.push(err instanceof TypeError); }
setTo(true, 1); setTo("s", 2);
var sealed = Object.seal({ a: 0, k: 9 }), closed = Object.preventExtensions({ k: 10 });
setTo(sealed, 90); setTo(closed, 100);
log.push(sealed.k, closed.k);
var proto = { set k(x) { log.push("proto setter"); } };
var own = Object.create(proto); own.k = 1;
var lit = { a: 0, k: 0 };
setTo(lit, 1); setTo(own, 2);
log.push(own.k, Object.getOwnPropertyNames(own).join(""));
var shared1 = { a: 1, k: "s1" }, shared2 = { a: 2, k: "s2" };
function read(o) { return o.k; }
log.push(read(shared1), read(shared2));
Object.defineProperty(shared1, "k", { get: function () { return "acc"; } });
log.push(read(shared1), read(shared2));
setTo(shared2, "s2b"); setTo(shared1, "ignored");
log.push(read(shared1), read(shared2));
print(log.join());
"#,
        "11,ak,2,2,true,3,5,5,set 6,g,7,azk,8,70,true,true,90,100,proto setter,proto setter,,,s1,s2,acc,s2,acc,s2b",
    );
}

#[test]
fn a_computed_read_site_follows_keys_attributes_and_storage() {
    // Numeric keys before and after a String key, index-like String keys, a
    // frozen receiver, a prototype change, a non-enumerable redefinition,
    // conversion to many keys, an add after a read, primitives and a nullish
    // base.
    run(
        r#"
var log = [];
function get(o, k) { return o[k]; }
var a = { x: 1, y: 2, z: 3 };
log.push(String(get(a, 0)), get(a, "x"), String(get(a, 1)), get({ "0": "z", a: 1 }, "0"), get({ "0": "z", a: 1 }, 0));
Object.freeze(a);
log.push(get(a, "x"), get(a, "y"));
var a2 = { x: "a2", y: 0, z: 0 };
log.push(get(a2, "x"));
Object.setPrototypeOf(a2, { w: "proto" });
log.push(get(a2, "x"), get(a2, "w"));
var b = { x: "b1", y: "b2", z: "b3" };
log.push(get(b, "x"));
Object.defineProperty(b, "x", { enumerable: false });
log.push(get(b, "x"), Object.keys(b).join(""));
for (var i = 0; i < 20; i++) b["p" + i] = i;
log.push(get(b, "x"), get(b, "p19"));
var c = { x: "c" };
log.push(get(c, "x"), get(c, "x"));
c.y = 1;
log.push(get(c, "x"));
log.push(get(true, "x") === undefined, get(1.5, "toFixed") === Number.prototype.toFixed);
try { get(null, "x"); } catch (e) { log.push(e instanceof TypeError); }
print(log.join());
"#,
        "undefined,1,undefined,z,z,1,2,a2,a2,proto,b1,b1,yz,b1,19,c,c,c,true,true,true",
    );
}
