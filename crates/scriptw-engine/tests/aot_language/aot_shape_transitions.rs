//! Plain data adds keep fresh ordinary objects compact along canonical shapes.
//! Every other structural change keeps the ordinary dictionary semantics.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};
use std::collections::BTreeMap;
use wasmparser::{Name, Operator, Parser, Payload, TypeRef};

fn options() -> CompileOptions {
    CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    }
}

fn run(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for directive in ["", "'use strict';\n"] {
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &format!(
                    "{directive}function check(ok, message) {{ if (!ok) throw new Error(message); }}\n{source}\nprint('shapes:ok');"
                ),
                options(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(60_000),
                    ..RunOptions::default()
                },
            )
            .expect("shape transitions execute compiled Wasm");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{directive:?}: {:?}",
            observed.completion
        );
        assert_eq!(
            observed.output_events,
            [HostOutputEvent::PrintLine("shapes:ok".into())],
            "{directive:?}"
        );
    }
}

/// Defined-function names of a module by absolute function index.
fn function_names(bytes: &[u8]) -> BTreeMap<u32, String> {
    let mut names = BTreeMap::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CustomSection(section) = payload.unwrap() {
            if let wasmparser::KnownCustom::Name(reader) = section.as_known() {
                for name in reader {
                    if let Name::Function(map) = name.unwrap() {
                        for naming in map {
                            let naming = naming.unwrap();
                            names.insert(naming.index, naming.name.to_owned());
                        }
                    }
                }
            }
        }
    }
    names
}

/// The direct callees of every defined function, by absolute function index.
fn direct_calls(bytes: &[u8]) -> BTreeMap<u32, Vec<u32>> {
    let mut imported = 0;
    let mut next = None;
    let mut calls = BTreeMap::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            Payload::ImportSection(section) => {
                for import in section.into_imports() {
                    if matches!(import.unwrap().ty, TypeRef::Func(_) | TypeRef::FuncExact(_)) {
                        imported += 1;
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let index = *next.get_or_insert(imported);
                next = Some(index + 1);
                let callees = body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .filter_map(|operator| match operator.unwrap() {
                        Operator::Call { function_index } => Some(function_index),
                        _ => None,
                    })
                    .collect();
                calls.insert(index, callees);
            }
            _ => {}
        }
    }
    calls
}

/// P's imported functions that resolve to `R` function `target`.
fn program_imports_of(program: &[u8], target: u32) -> Vec<u32> {
    let mut ordinal = 0;
    let mut found = Vec::new();
    for payload in Parser::new(0).parse_all(program) {
        if let Payload::ImportSection(section) = payload.unwrap() {
            for import in section.into_imports() {
                let import = import.unwrap();
                if matches!(import.ty, TypeRef::Func(_) | TypeRef::FuncExact(_)) {
                    if import.name == format!("f{target}") {
                        found.push(ordinal);
                    }
                    ordinal += 1;
                }
            }
        }
    }
    found
}

#[test]
fn fresh_objects_and_receiver_definitions_route_through_compact_shapes() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let unit = engine
        .compile_script(
            "function F(a) { this.a = a; this.b = a; } var o = {}; o.a = 1; new F(o).b === o;",
            options(),
        )
        .unwrap();
    let artifact = engine.emit_wasm(&unit).unwrap();
    let runtime = artifact.runtime.as_ref().expect("linked runtime").bytes();
    let names = function_names(runtime);
    let index = |name: &str| {
        names
            .iter()
            .find_map(|(index, actual)| (actual == name).then_some(*index))
            .unwrap_or_else(|| panic!("runtime registers {name}"))
    };
    let allocate = index("helper::compact_object_allocate");
    let define = index("helper::compact_data_define");
    let calls = direct_calls(runtime);
    // OrdinarySet appends to its receiver only through [[DefineOwnProperty]],
    // whose complete default data definition reaches the transition kernel.
    assert!(calls[&index("helper::ordinary_set_data_on_receiver")].contains(&define));
    // Ordinary [[Construct]] allocates its receiver on the empty shape.
    assert!(
        calls
            .iter()
            .any(|(caller, callees)| *caller != allocate && callees.contains(&allocate)),
        "some runtime constructor path allocates on the empty compact shape"
    );
    // The `{}` literal in P does too.
    let imports = program_imports_of(&artifact.bytes, allocate);
    assert_eq!(imports.len(), 1, "P imports the compact allocator once");
    assert!(direct_calls(&artifact.bytes)
        .values()
        .any(|callees| callees.contains(&imports[0])));
}

#[test]
fn shaped_objects_keep_order_attributes_and_enumeration() {
    run(r#"
function Point(x, y) { this.x = x; this.y = y; this.z = undefined; }
var p = new Point(1, 2), q = new Point('a', {});
check(Object.keys(p).join() === 'x,y,z' && JSON.stringify(p) === '{"x":1,"y":2}', 'constructor order');
check(Object.keys(q).join() === 'x,y,z' && q.x === 'a' && typeof q.y === 'object', 'shared shape, own values');
var d = Object.getOwnPropertyDescriptor(p, 'y');
check(d.value === 2 && d.writable && d.enumerable && d.configurable, 'default attributes');
p.x = 10;
check(p.x === 10 && q.x === 'a', 'writes stay per object');
var o = {}; o.b = 1; o.a = 2; o[2] = 'two'; o[0] = 'zero'; o.c = 3;
check(Object.keys(o).join() === '0,2,b,a,c', 'array indices first');
var e = {}; e.b = 1; e.a = 2; e['01'] = 3; e['-1'] = 4; e['4294967295'] = 5; e['1.5'] = 6;
check(Object.keys(e).join() === 'b,a,01,-1,4294967295,1.5', 'non-index strings keep insertion order');
var seen = []; for (var key in e) seen.push(key);
check(seen.join() === 'b,a,01,-1,4294967295,1.5', 'for-in order');
check(JSON.stringify({...e}) === '{"b":1,"a":2,"01":3,"-1":4,"4294967295":5,"1.5":6}', 'spread order');
check(Object.entries(Object.assign({}, e)).join() === 'b,1,a,2,01,3,-1,4,4294967295,5,1.5,6', 'assign order');
var symbol = Symbol('s'), f = {}; f.a = 1; f[symbol] = 2; f.b = 3;
var own = Reflect.ownKeys(f);
check(own.length === 3 && own[0] === 'a' && own[1] === 'b' && own[2] === symbol && f[symbol] === 2, 'symbols follow strings');
var g = {}; g.a = 1; g.b = 2;
var visited = [];
for (var key in g) { visited.push(key); if (key === 'a') { g.c = 3; delete g.b; } }
check(visited.join() === 'a' && Object.keys(g).join() === 'a,c', 'for-in after adds and deletes');
var literal = {a: 1}; literal.b = 2; literal.c = 3; literal.d = 4;
check(Object.keys(literal).join() === 'a,b,c,d' && literal.a === 1 && literal.d === 4, 'literal grows');
var computed = {['x' + 1]: 1, y: 2, m() { return this.y; }};
computed.z = 3;
check(Object.keys(computed).join() === 'x1,y,m,z' && computed.m() === 2, 'general literal');
var {x, ...rest} = {x: 1, y: 2, z: 3}; rest.w = 4;
check(x === 1 && Object.keys(rest).join() === 'y,z,w', 'rest object');
"#);
}

#[test]
fn non_data_adds_keep_dictionary_semantics() {
    run(r#"
var a = {}; a.x = 1; a.y = 2; delete a.x; a.x = 3;
check(Object.keys(a).join() === 'y,x' && a.x === 3, 'delete then add');
var b = {}; b.x = 1; Object.freeze(b);
check(!Reflect.set(b, 'y', 2) && !Reflect.set(b, 'x', 5) && !('y' in b) && b.x === 1 && Object.isFrozen(b), 'freeze');
var threw = false;
try { (function () { 'use strict'; b.z = 1; })(); } catch (error) { threw = error instanceof TypeError; }
check(threw, 'strict add to frozen object');
var c = {}; c.x = 1; Object.preventExtensions(c);
check(!Reflect.set(c, 'y', 2) && Reflect.set(c, 'x', 4) && !('y' in c) && c.x === 4 && !Object.isExtensible(c), 'preventExtensions');
var g = {}; g.x = 1; Object.seal(g); g.x = 2;
check(g.x === 2 && Object.isSealed(g) && !Reflect.deleteProperty(g, 'x'), 'seal');
var h = {}; h.a = 1; Object.defineProperty(h, 'b', {value: 2}); h.c = 3;
var hidden = Object.getOwnPropertyDescriptor(h, 'b');
check(!hidden.writable && !hidden.enumerable && !hidden.configurable && Object.keys(h).join() === 'a,c', 'non-default definition');
var i = {}; i.a = 1;
Object.defineProperty(i, 'g', {get() { return 7; }, enumerable: true, configurable: true}); i.z = 2;
check(i.g === 7 && Object.keys(i).join() === 'a,g,z', 'accessor');
var j = {}; j.a = 1;
Object.defineProperty(j, 'a', {value: 9, writable: true, enumerable: true, configurable: true}); j.b = 2;
check(j.a === 9 && Object.keys(j).join() === 'a,b', 'complete redefinition');
var k = {}; k.a = 1; Object.defineProperty(k, 'a', {enumerable: false}); k.b = 2;
check(Object.keys(k).join() === 'b' && k.a === 1 && Object.getOwnPropertyNames(k).join() === 'a,b', 'attribute redefinition');
var m = {}; m.a = 1; Object.defineProperty(m, 'b', {value: 2, writable: true, enumerable: true, configurable: true});
check(Object.keys(m).join() === 'a,b' && Object.getOwnPropertyDescriptor(m, 'b').writable, 'defineProperty default data');
var n = {}; Object.setPrototypeOf(n, null); n.a = 1; n.b = 2;
check(Object.keys(n).join() === 'a,b' && n.b === 2 && Object.getPrototypeOf(n) === null, 'null prototype');
"#);
}

#[test]
fn set_path_respects_prototype_setters_read_only_inherited_and_proxies() {
    run(r#"
var log = [];
var proto = {set s(value) { log.push('set:' + value); }};
Object.defineProperty(proto, 'ro', {value: 1, writable: false});
function C() { this.s = 1; this.ok = 3; }
C.prototype = proto;
var made = new C();
check(log.join() === 'set:1' && !Object.hasOwn(made, 's') && made.ok === 3, 'inherited setter');
check(!Reflect.set(made, 'ro', 2) && made.ro === 1 && !Object.hasOwn(made, 'ro'), 'read-only inherited');
var threw = false;
try { (function () { 'use strict'; made.ro = 2; })(); } catch (error) { threw = error instanceof TypeError; }
check(threw, 'strict read-only inherited');
var trapped = [];
var target = {};
function P() { this.a = 1; this.b = 2; }
P.prototype = new Proxy(target, {set(t, key, value, receiver) { trapped.push(key); return Reflect.set(t, key, value, receiver); }});
var viaProxy = new P();
check(trapped.join() === 'a,b' && Object.keys(viaProxy).join() === 'a,b' && Object.keys(target).length === 0, 'proxy prototype');
var other = {};
check(Reflect.set({}, 'q', 5, other) && other.q === 5 && Object.keys(other).join() === 'q', 'distinct receiver');
var parent = {inherited: 1};
var u = {}; u.__proto__ = parent;
check(Object.getPrototypeOf(u) === parent && !Object.hasOwn(u, '__proto__') && u.inherited === 1, '__proto__ assignment');
var w = {['__proto__']: 1}; w.next = 2;
check(Object.hasOwn(w, '__proto__') && w.__proto__ === 1 && Object.getPrototypeOf(w) === Object.prototype, 'computed __proto__');
check(Object.keys(w).join() === '__proto__,next', 'own __proto__ order');
var later = [];
Object.defineProperty(Object.prototype, 'late', {set(value) { later.push(value); }, configurable: true});
var l = {}; l.late = 5;
delete Object.prototype.late;
l.late = 6;
check(later.join() === '5' && l.late === 6 && Object.keys(l).join() === 'late', 'setter added after shapes exist');
Object.defineProperty(Object.prototype, 'frozenInherited', {value: 0, writable: false, configurable: true});
var r = {}; r.a = 1;
check(!Reflect.set(r, 'frozenInherited', 2) && !Object.hasOwn(r, 'frozenInherited'), 'read-only on Object.prototype');
delete Object.prototype.frozenInherited;
r.frozenInherited = 3;
check(r.frozenInherited === 3 && Object.keys(r).join() === 'a,frozenInherited', 'after removal');
"#);
}

#[test]
fn growth_bounds_and_values_survive_transitions_and_gc() {
    run(r#"
function Wide(n) { for (var i = 0; i < n; i++) this['k' + i] = i; }
var wide = new Wide(80), again = new Wide(80);
check(Object.keys(wide).length === 80 && wide.k79 === 79 && again.k0 === 0 && Object.keys(again)[70] === 'k70', 'many properties');
wide.k3 = 'three'; delete wide.k4;
check(wide.k3 === 'three' && !('k4' in wide) && again.k3 === 3 && Object.keys(wide).length === 79, 'past the bound');
function Forty() { for (var j = 0; j < 40; j++) this['f' + j] = j; }
var forty = new Forty(), twin = new Forty();
for (var j = 0; j < 40; j++) forty['f' + j] = forty['f' + j] * 2;
var marker = {};
forty.f39 = 'last'; forty.f7 = marker;
check(forty.f39 === 'last' && forty.f0 === 0 && forty.f20 === 40 && forty.f7 === marker, 'indexed reads and writes');
check(!('f40' in forty) && forty.f40 === undefined && !Object.hasOwn(forty, 'f') && twin.f39 === 39, 'indexed misses');
check(Object.getOwnPropertyDescriptor(forty, 'f21').value === 42 && Object.keys(forty)[39] === 'f39', 'indexed descriptors and order');
var many = [];
for (var i = 0; i < 100; i++) { var t = {}; t['first' + i] = i; t.second = -i; many.push(t); }
gc();
for (var i = 0; i < 100; i++) {
  check(many[i]['first' + i] === i && Object.is(many[i].second, -i) && Object.keys(many[i]).join() === 'first' + i + ',second', 'root fan-out ' + i);
}
var token = {id: 1}, symbol = Symbol('v'), fn = function () { return token; };
var v = {};
v.zero = -0; v.nan = NaN; gc(); v.big = 12345678901234567890n; v.ref = token; v.sym = symbol;
v.undef = undefined; v.nul = null; v.bool = true; gc(); v.str = 'text'; v.num = 1.5; v.fn = fn;
for (var i = 0; i < 20; i++) { v['extra' + i] = i % 2 ? token : i; if (i % 7 === 0) gc(); }
gc();
check(Object.is(v.zero, -0) && Number.isNaN(v.nan) && v.big === 12345678901234567890n && v.ref === token, 'kept scalars');
check(v.sym === symbol && Object.hasOwn(v, 'undef') && v.undef === undefined && v.nul === null && v.bool === true, 'kept tags');
check(v.str === 'text' && v.num === 1.5 && v.fn() === token && v.extra19 === token && v.extra18 === 18, 'kept values');
check(Object.keys(v).length === 31 && Object.keys(v)[11] === 'extra0', 'kept order');
var s1 = {}, s2 = {}; s1.a = 1; s2.a = 2; s1.b = token; s2.c = 3;
check(s1.a === 1 && s2.a === 2 && s1.b === token && !('c' in s1) && !('b' in s2) && s2.c === 3, 'divergent siblings');
var dynamic = {}, key = ['dy', 'namic'].join('');
dynamic[key] = 1; var same = {}; same.dynamic = 2;
check(dynamic.dynamic === 1 && same[key] === 2 && Object.keys(dynamic).join() === 'dynamic', 'equal key contents');
"#);
}

#[test]
fn shapes_are_shared_across_realms_without_sharing_values() {
    run(r#"
var other = __scriptwCreateRealm();
var make = other.evalScript('(function (value) { var o = {}; o.a = value; o.b = [value]; return o; })');
var local = {}; local.a = 1; local.b = [1];
var foreign = make(2);
check(Object.getPrototypeOf(foreign) !== Object.prototype && Object.keys(foreign).join() === 'a,b', 'foreign object');
check(foreign.a === 2 && Array.isArray(foreign.b) === true && foreign.b[0] === 2 && local.a === 1, 'own values');
foreign.c = 3; local.c = 4; gc();
check(foreign.c === 3 && local.c === 4 && JSON.stringify(foreign) === '{"a":2,"b":[2],"c":3}', 'later adds');
"#);
}
