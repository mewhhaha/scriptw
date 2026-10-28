use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, RealmBuilder, RunOptions,
};
fn run(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
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
        .unwrap();
    assert!(
        outcome.note.contains("boolean(true)"),
        "{}\n{source}",
        outcome.note
    );
}
#[test]
fn compact_nested_literals_are_fresh_mutable_real_arrays_with_ordinary_own_descriptors() {
    let pairs = (0..128)
        .map(|index| format!("['Ā\\0𝄞',{index},,null,'\\uD800']"))
        .collect::<Vec<_>>()
        .join(",");
    run(&format!(
        r#"
function factory() {{ return [{pairs}]; }}
var first = factory(), second = factory(), originalPrototype = Array.prototype;
var descriptor = Object.getOwnPropertyDescriptor(first[0], '0');
var fresh = first !== second && first[0] !== second[0] && first[0] !== first[1];
var shape = Array.isArray(first) && Array.isArray(first[0]) && Object.getPrototypeOf(first) === originalPrototype && Object.getPrototypeOf(first[0]) === originalPrototype;
var holes = first[0].length === 5 && !(2 in first[0]) && (3 in first[0]) && first[0][3] === null && first[0][4].charCodeAt(0) === 0xD800;
var flags = descriptor.writable && descriptor.enumerable && descriptor.configurable;
Object.defineProperty(originalPrototype, '2', {{get() {{return 'hole inherited';}}, configurable:true}});
var inheritedHole = second[0][2] === 'hole inherited' && Object.getOwnPropertyDescriptor(second[0], '2') === undefined;
delete originalPrototype[2];
first[0][0] = 'changed'; first[0].push(9); delete first[0][1];
Object.defineProperty(first[1], '1', {{value:77, writable:false, enumerable:false, configurable:true}});
globalThis.Array = function () {{ throw 'public Array constructor'; }};
var third = factory();
fresh && shape && holes && flags && inheritedHole && second[0][0] === 'Ā\0𝄞' && second[0][1] === 0 && second[0].length === 5 &&
  !(1 in first[0]) && first[1][1] === 77 && second[1][1] === 1 && third[0][0] === 'Ā\0𝄞' && Object.getPrototypeOf(third) === originalPrototype;
"#
    ));
}
#[test]
fn compact_literal_publication_bypasses_inherited_setters_and_effectful_boundaries_stay_ordered() {
    let numbers = (0..128)
        .map(|index| index.to_string())
        .collect::<Vec<_>>()
        .join(",");
    run(&format!(
        r#"
var calls = 0;
Object.defineProperty(Array.prototype, '0', {{get() {{return 'inherited';}}, set(value) {{calls++;}}, configurable:true}});
function pure() {{ return [{numbers}]; }}
var array = pure(), descriptor = Object.getOwnPropertyDescriptor(array, '0');
var own = array[0] === 0 && descriptor.value === 0 && descriptor.writable && descriptor.enumerable && descriptor.configurable && calls === 0;
delete Array.prototype[0];
var order = [], marker = {{}}, caught;
function effect(label) {{ order.push(label); return label; }}
var mixed = [[1, 2], effect('middle'), [3, 4]];
try {{ [effect('before'), {{[Symbol.toPrimitive]() {{order.push('coerce'); throw marker;}}}} + 1, effect('after')]; }} catch(error) {{caught = error;}}
function bigintFactory() {{ return [{numbers}, 1n, -1n]; }}
var big = bigintFactory(), again = bigintFactory();
var numeric = [0, -0, NaN, Infinity, -Infinity];
own && order.join(',') === 'middle,before,coerce' && caught === marker && mixed[0] !== mixed[2] && mixed[1] === 'middle' &&
  big !== again && big[128] === 1n && big[129] === -1n && Object.is(numeric[1], -0) && numeric[2] !== numeric[2];
"#
    ));
}

#[test]
fn compact_nested_literals_use_the_defining_source_realm_array_prototype() {
    let pairs = (0..128)
        .map(|index| format!("['realm',{index}]"))
        .collect::<Vec<_>>()
        .join(",");
    let factory = format!("function factory(){{return [{pairs}];}} factory;");
    // This string is ASCII source; Rust's quoted Debug spelling is also a JS
    // string literal and does not introduce Unicode escape-policy ambiguity.
    run(&format!(
        r#"
// Raw Engine scripts expose the Test262 host primitive without its JS harness prelude.
var realm = __scriptwCreateRealm();
var foreignFactory = realm.evalScript({factory:?});
var foreignPrototype = realm.evalScript('Array.prototype');
var array = foreignFactory(), again = foreignFactory();
Object.getPrototypeOf(array) === foreignPrototype && Object.getPrototypeOf(array[0]) === foreignPrototype &&
  foreignPrototype !== Array.prototype && array !== again && array[0] !== again[0] && array[127][1] === 127 && array[0][0] === 'realm';
"#
    ));
}
#[test]
fn small_dynamic_literals_keep_numeric_bits_and_ordinary_mutation_after_batch_initialization() {
    run(r#"
function make(a,b,c,d) { return [a,b,c,d]; }
var first = make(0, -0, NaN, Infinity), second = make(0, -0, NaN, -Infinity);
var descriptor = Object.getOwnPropertyDescriptor(first, '1');
var numeric = first !== second && Array.isArray(first) && first.length === 4 &&
  Object.is(first[1], -0) && Number.isNaN(first[2]) && first[3] === Infinity && second[3] === -Infinity &&
  Object.keys(first).join(',') === '0,1,2,3' && descriptor.writable && descriptor.enumerable && descriptor.configurable;
var token = {kept:17};
delete first[0]; first[4] = token; first.length = 3;
Object.defineProperty(first, '2', {value:token, writable:false, enumerable:false, configurable:true});
var ordinary = !(0 in first) && !(4 in first) && first.length === 3 && first[2] === token &&
  Object.keys(first).join(',') === '1' && Number.isNaN(second[2]);
second.push(11); second[1] = 'promoted';
var mixed = make(undefined, null, true, 'kept'), symbol = Symbol('kept');
var other = make(token, symbol, 1n, function () { return 19; });
var previous;
function capture() { previous = {rooted:23}; return previous; }
function collect() { previous = null; gc(); return -0; }
var rooted = [capture(), collect(), symbol, token];
var eight = [1, 2, 3, 4, 5, 6, 7, 8], hole = [1,,3,4], long = [1,2,3,4,5,6,7,8,9];
gc();
numeric && ordinary && second.length === 5 && second[1] === 'promoted' && second[4] === 11 &&
  Object.hasOwn(mixed, '0') && mixed[0] === undefined && mixed[1] === null && mixed[2] === true && mixed[3] === 'kept' &&
  other[0] === token && other[1] === symbol && other[2] === 1n && other[3]() === 19 &&
  rooted[0].rooted === 23 && Object.is(rooted[1], -0) && rooted[2] === symbol && rooted[3] === token &&
  eight.length === 8 && eight[7] === 8 && hole.length === 4 && !(1 in hole) && long[8] === 9;
"#);
}

#[test]
fn small_dynamic_literal_element_effects_and_abrupt_completion_run_once_in_source_order() {
    run(r#"
var order = '', marker = {}, setterCalls = 0, first = 1;
Object.defineProperty(Array.prototype, '0', {
  get() { return 'inherited'; }, set(value) { setterCalls++; }, configurable:true
});
function effect(label, value) { order += label; return value; }
function change() { order += 'b'; first = 99; gc(); return 2; }
var numbers = [effect('a', first), change(), effect('c', 3), effect('d', 4)];
var mixed = [effect('e', 'own'), effect('f', undefined), effect('g', {}), effect('h', 4n)];
function fail() { order += 'y'; gc(); throw marker; }
var caught, coerceCaught;
try { [effect('x', 5), fail(), effect('z', 7), 8]; } catch (error) { caught = error; }
try {
  [effect('u', 1), {[Symbol.toPrimitive]() { order += 'v'; throw marker; }} + 1, effect('w', 3), 4];
} catch (error) { coerceCaught = error; }
var descriptor = Object.getOwnPropertyDescriptor(numbers, '0');
var own = numbers[0] === 1 && numbers[1] === 2 && mixed[0] === 'own' && Object.hasOwn(mixed, '1') &&
  descriptor.value === 1 && descriptor.writable && descriptor.enumerable && descriptor.configurable && setterCalls === 0;
delete Array.prototype[0];
own && first === 99 && order === 'abcdefghxyuv' && caught === marker && coerceCaught === marker;
"#);
}

#[test]
fn small_dynamic_literal_batch_uses_the_defining_source_realm_array_prototype() {
    run(r#"
var realm = __scriptwCreateRealm();
var foreignFactory = realm.evalScript('function factory(a,b,c,d){return[a,b,c,d];} factory;');
var foreignPrototype = realm.evalScript('Array.prototype');
var originalPrototype = Array.prototype;
var array = foreignFactory(1, 2, 3, 4), again = foreignFactory('mixed', null, undefined, 4n);
function localFactory(a,b,c,d) { return [a,b,c,d]; }
globalThis.Array = function () { throw 'public Array constructor'; };
var local = localFactory(5, 6, 7, 8);
gc();
Object.getPrototypeOf(array) === foreignPrototype && Object.getPrototypeOf(again) === foreignPrototype &&
  foreignPrototype !== originalPrototype && array !== again && array[3] === 4 && again[0] === 'mixed' &&
  again[3] === 4n && Object.getPrototypeOf(local) === originalPrototype && local[0] === 5;
"#);
}
