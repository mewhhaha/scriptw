use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_reads(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compiler worker");
    for strict in [false, true] {
        let source = format!(
            "{}function assert(value, message) {{ if (!value) throw new Error(message); }}\n{source}",
            if strict { "'use strict';\n" } else { "" }
        );
        let outcome = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(30_000),
                    ..RunOptions::default()
                },
            )
            .expect("property plans execute compiled Wasm");
        assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(outcome.completion, ObservedCompletion::Normal(_)),
            "{:?}\n{source}",
            outcome.completion
        );
    }
}

#[test]
fn property_slot_candidates_check_actual_order_and_later_descriptors() {
    assert_reads(
        r#"
var token = {}, pair = {x: token, y: 7}, reversed = {z: 11, a: token};
assert(pair.x === token && pair.y === 7, 'ordinary own data');
assert(reversed.a === token && reversed.z === 11, 'shape name order is not insertion order');
var reads = 0;
Object.defineProperty(pair, 'x', {get: function() { reads++; return token; }, configurable: true});
assert(pair.x === token && reads === 1, 'data to accessor remains observable');
delete pair.y; pair.y = 19;
assert(pair.y === 19 && Object.keys(pair).join(',') === 'x,y', 'deleted slot and later insertion');
var inherited = Object.create({x: token});
assert(inherited.x === token, 'inherited value is not an own-slot fact');
var proxy = new Proxy(pair, {get: function(target, key, receiver) { reads++; return Reflect.get(target, key, receiver); }});
assert(proxy.x === token && reads === 3, 'proxy and getter retain both effects');
"#,
    );
}

#[test]
fn numeric_array_reads_preserve_nonindices_holes_and_receivers() {
    assert_reads(
        r#"
var token = {}, a = [token, 7], indexes = [-0, 0, 1];
for (var i = 0; i < indexes.length; i++) assert(a[indexes[i]] === (i === 2 ? 7 : token), 'numeric index');
a[-1] = 11; a[1.5] = 13; a[NaN] = 17; a[Infinity] = 19; a[4294967295] = 23;
assert(a[-1] === 11 && a[1.5] === 13 && a[NaN] === 17 && a[Infinity] === 19 && a[4294967295] === 23, 'nonindices remain names');
a['-0'] = 29;
assert(a[-0] === token && a['-0'] === 29 && a.length === 2, 'negative zero and length');
var proto = Object.create(Array.prototype), reads = 0;
Object.defineProperty(proto, '3', {get: function() { reads++; assert(this === a, 'inherited receiver'); return token; }, configurable: true});
Object.setPrototypeOf(a, proto); a.length = 4;
assert(a[3] === token && reads === 1 && !Object.hasOwn(a, '3'), 'hole preserves prototype getter');
Object.defineProperty(a, '1', {get: function() { reads++; throw token; }, configurable: true});
try { a[1]; throw 1; } catch (error) { assert(error === token && reads === 2, 'own accessor abrupt identity'); }
"#,
    );
}

#[test]
fn property_reads_evaluate_keys_once_before_receiver_errors() {
    assert_reads(
        r#"
var token = {}, a = [token], evaluations = 0, coercions = 0;
function numeric() { evaluations++; return 0; }
assert(a[numeric()] === token && evaluations === 1, 'numeric key evaluation once');
var key = {}; key[Symbol.toPrimitive] = function(hint) { coercions++; assert(hint === 'string', 'key hint'); a[0] = 7; return '0'; };
assert(a[key] === 7 && coercions === 1, 'effectful key conversion before lookup');
try { null[numeric()]; throw 1; } catch (error) { assert(error instanceof TypeError && evaluations === 2, 'key evaluated before nullish check'); }
try { undefined[key]; throw 1; } catch (error) { assert(error instanceof TypeError && coercions === 1, 'nullish check precedes key conversion'); }
"#,
    );
}

#[test]
fn numeric_keys_observe_receiver_mutation_before_own_data_dispatch() {
    assert_reads(
        r#"
var before = {}, after = {}, array = [before], evaluations = 0, reads = 0;
function replace() { evaluations++; array[0] = after; return 0; }
assert(array[replace()] === after && evaluations === 1, 'numeric key mutation precedes data read');
var proto = Object.create(Array.prototype);
Object.defineProperty(proto, '0', {get: function() { reads++; assert(this === array, 'deleted index keeps receiver'); return before; }, configurable: true});
Object.setPrototypeOf(array, proto);
function remove() { evaluations++; delete array[0]; return 0; }
assert(array[remove()] === before && evaluations === 2 && reads === 1, 'key deletion reaches inherited getter');
function redefine() {
  evaluations++;
  Object.defineProperty(array, '0', {get: function() { reads++; assert(this === array, 'redefined getter receiver'); throw after; }, configurable: true});
  return 0;
}
try { array[redefine()]; throw 1; } catch (error) {
  assert(error === after && evaluations === 3 && reads === 2, 'key redefinition preserves thrown identity');
}
var proxyReads = 0, target = [after], proxy;
proxy = new Proxy(target, {get: function(object, key, receiver) {
  proxyReads++;
  assert(object === target && key === '0' && receiver === proxy, 'numeric proxy trap arguments');
  return Reflect.get(object, key, receiver);
}});
assert(proxy[0] === after && proxyReads === 1, 'numeric proxy read is never an array own-data shortcut');
"#,
    );
}

#[test]
fn property_candidates_preserve_last_index_and_descriptor_attribute_transitions() {
    assert_reads(
        r#"
var token = {}, edge = [];
edge[4294967294] = token;
edge[4294967295] = 17;
assert(edge.length === 4294967295 && edge[4294967294] === token && edge[4294967295] === 17, 'last array index differs from ordinary numeric name');
assert(Object.keys(edge).join(',') === '4294967294,4294967295', 'boundary keys retain order');
var sealed = [token];
Object.seal(sealed);
assert(Reflect.set(sealed, '0', 7) && sealed[0] === 7, 'sealed writable element can change');
assert(!Reflect.deleteProperty(sealed, '0') && sealed[0] === 7, 'sealed element remains present');
Object.freeze(sealed);
assert(!Reflect.set(sealed, '0', token) && sealed[0] === 7 && sealed.length === 1, 'frozen element read observes committed value');
var record = {x: token};
Object.seal(record);
assert(Reflect.set(record, 'x', 23) && record.x === 23, 'sealed ordinary slot can change');
Object.freeze(record);
assert(!Reflect.set(record, 'x', token) && record.x === 23, 'frozen ordinary slot retains its value');
"#,
    );
}

#[test]
fn canonical_numeric_read_keeps_presence_number_bits_and_storage_transitions() {
    assert_reads(
        r#"
function read(array, key) { return array[key]; }
function literalRead(value) { var array = [value]; return array[0]; }
var token = {}, array = [-0, NaN, 3], inheritedReads = 0;
assert(Object.is(read(array, -0), -0) && Number.isNaN(read(array, 1)), 'packed Number payloads');
assert(literalRead(token) === token && Object.is(literalRead(-0), -0), 'shaped source read uses the same numeric probe');
var proto = Object.create(Array.prototype);
Object.defineProperty(proto, '2', {get: function() { inheritedReads++; return token; }, configurable: true});
Object.setPrototypeOf(array, proto);
array[2] = undefined;
assert(read(array, 2) === undefined && inheritedReads === 0, 'present packed undefined shadows prototype');
delete array[2];
assert(read(array, 2) === token && inheritedReads === 1, 'a packed hole falls through to the prototype');
array['-0'] = 19;
assert(Object.is(read(array, -0), -0) && read(array, '-0') === 19, 'only numeric negative zero is index zero');
var keys = [-1, 1.5, NaN, Infinity, -Infinity, 4294967295, 9007199254740992];
for (var i = 0; i < keys.length; i++) array[keys[i]] = 100 + i;
for (var i = 0; i < keys.length; i++) assert(read(array, keys[i]) === 100 + i, 'nonindex Number keeps its exact property name');
Object.defineProperty(array, '1', {value: token, writable: false, configurable: false});
assert(read(array, 1) === token && Object.is(read(array, 0), -0), 'descriptor transition keeps current own data');
var edge = [];
edge[4294967294] = token; edge[4294967295] = 37;
assert(read(edge, 4294967294) === token && read(edge, 4294967295) === 37, 'last index and first nonindex survive sparse fallback');
assert(edge.length === 4294967295, 'numeric name does not extend array length');
"#,
    );
}

#[test]
fn canonical_numeric_read_preserves_key_effects_and_distinct_getter_receivers() {
    assert_reads(
        r#"
function read(array, key) { return array[key]; }
var token = {}, array = [1], events = '', getterReads = 0;
function receiver() { events += 'receiver;'; return array; }
function key() {
  events += 'key;';
  Object.defineProperty(array, '0', {get: function() {
    getterReads++; events += 'get;'; assert(this === array, 'source getter receiver'); return token;
  }, configurable: true});
  return -0;
}
assert(read(receiver(), key()) === token && events === 'receiver;key;get;' && getterReads === 1, 'key effects finish before own-data admission');
var different = {mark: 41};
Object.defineProperty(array, '0', {get: function() { return this.mark; }, configurable: true});
assert(Reflect.get(array, 0, different) === 41, 'general Get preserves its explicit receiver');
var inherited = Object.create(array); inherited.mark = 43;
assert(read(inherited, 0) === 43, 'an ordinary object inheriting an Array retains its receiver');
var proxy, traps = 0;
proxy = new Proxy([token], {get: function(target, name, original) {
  traps++; assert(name === '0' && original === proxy, 'numeric proxy key and receiver'); return Reflect.get(target, name, original);
}});
assert(read(proxy, -0) === token && traps === 1, 'a Proxy never passes the concrete Array guard');
var evaluations = 0, coercions = 0, coercible = {};
coercible[Symbol.toPrimitive] = function(hint) {
  coercions++; assert(hint === 'string', 'property key hint'); array = [token]; return 0;
};
function evaluateKey() { evaluations++; return coercible; }
try { read(null, evaluateKey()); throw 1; } catch (error) {
  assert(error instanceof TypeError && evaluations === 1 && coercions === 0, 'key expression precedes nullish error but conversion does not');
}
var original = [7];
assert(read(original, evaluateKey()) === 7 && evaluations === 2 && coercions === 1, 'object conversion runs once and cannot replace the evaluated receiver');
var abrupt = {}; abrupt[Symbol.toPrimitive] = function() { throw token; };
try { read(original, abrupt); throw 1; } catch (error) { assert(error === token, 'coercion abrupt identity'); }
"#,
    );
}

#[test]
fn length_key_prefilter_preserves_computed_names_symbols_and_index_fallbacks() {
    assert_reads(
        r#"
function read(value, key) { return value[key]; }
var lengthKey = ['le', 'ng', 'th'].join('');
var nulKey = ['len', '\u0000', 'th'].join('');
var surrogateKey = ['len', '\ud800', 'th'].join('');
var symbol = Symbol('length'), otherSymbol = Symbol('length');
var array = [undefined, 2, 3], text = 'a\u0000\ud800', reads = 0;
assert(read(array, lengthKey) === 3 && read(text, lengthKey) === 3,
  'allocated six-unit length keys keep full content equality');
array.longer = 19;
Object.defineProperty(array, nulKey, {get: function() {
  reads++; assert(this === array, 'NUL getter receiver'); return 23;
}, configurable: true});
var proto = Object.create(Array.prototype);
Object.defineProperty(proto, surrogateKey, {get: function() {
  reads++; assert(this === array, 'surrogate getter receiver'); return 29;
}, configurable: true});
Object.defineProperty(proto, '1', {get: function() {
  reads++; assert(this === array, 'hole getter receiver'); return 31;
}, configurable: true});
Object.setPrototypeOf(array, proto);
delete array[1];
assert(read(array, 'longer') === 19 && read(array, nulKey) === 23 &&
  read(array, surrogateKey) === 29 && reads === 2,
  'six-unit misses retain ordinary own and prototype getters');
var primitiveReads = 0;
Object.defineProperty(String.prototype, nulKey, {get: function() {
  'use strict'; primitiveReads++; assert(this === text, 'primitive NUL receiver'); return 43;
}, configurable: true});
Object.defineProperty(String.prototype, surrogateKey, {get: function() {
  'use strict'; primitiveReads++; assert(this === text, 'primitive surrogate receiver'); return 47;
}, configurable: true});
assert(read(text, nulKey) === 43 && read(text, surrogateKey) === 47 && primitiveReads === 2,
  'six-unit primitive String misses retain ordinary prototype getters');
array[symbol] = 37;
Object.defineProperty(String.prototype, symbol, {get: function() {
  'use strict'; reads++; assert(this === text, 'primitive Symbol getter receiver'); return 41;
}, configurable: true});
assert(read(array, symbol) === 37 && read(text, symbol) === 41 &&
  read(array, otherSymbol) === undefined && read(text, otherSymbol) === undefined && reads === 3,
  'Symbol descriptions are not String keys');
assert(read(array, '0') === undefined && read(array, '1') === 31 && reads === 4,
  'non-six-unit index keys keep presence and hole lookup');
var coercions = 0, key = {};
key[Symbol.toPrimitive] = function(hint) {
  coercions++; assert(hint === 'string', 'key hint'); array.length = 4; return lengthKey;
};
assert(read(array, key) === 4 && coercions === 1,
  'key conversion runs once before intrinsic length is read');
var traps = 0, proxy;
proxy = new Proxy(array, {get: function(target, name, receiver) {
  traps++; assert(name === 'length' && receiver === proxy, 'Proxy keeps its key and receiver');
  return Reflect.get(target, name, receiver);
}});
assert(read(proxy, lengthKey) === 4 && traps === 1, 'Proxy does not enter the concrete Array path');
"#,
    );
}
