use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, RealmBuilder, RunOptions,
};

fn assert_leaf_semantics(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
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
        .unwrap_or_else(|error| panic!("leaf activation execution failed: {error}\n{source}"));
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        outcome.note.contains("boolean(true)"),
        "{}\n{source}",
        outcome.note
    );
}

#[test]
fn guarded_numeric_leaves_preserve_once_evaluated_arguments_and_fallbacks() {
    assert_leaf_semantics(
        r#"
function arithmetic(a,b){return a*b+1;}
var original=arithmetic, log=[];
var first=arithmetic((log.push('left'),3),(log.push('right'),gc(),4));
var receiver={value:6}, converted={valueOf(){log.push('coerce');return 5;}};
var coerced=arithmetic(converted,2);
var replaced=arithmetic((arithmetic=function(a,b){return this.value+a+b;},2),3);
var changed=arithmetic.call(receiver,2,3);
var missing=original(2), extra=original(2,3,99);
var bound=original.bind(null,2), proxy=new Proxy(original,{apply(target,receiver,args){log.push('proxy');return Reflect.apply(target,receiver,args);}});
var proxied=proxy(2,4);
var negative=original(-0,3), nan=original(NaN,2), infinite=original(Infinity,0);
first===13 && coerced===11 && replaced===7 && changed===11 &&
Number.isNaN(missing) && extra===7 && bound(4)===9 && proxied===9 &&
negative===1 && Number.isNaN(nan) && Number.isNaN(infinite) &&
log.join('|')==='left|right|coerce|proxy' && original.arguments===null;
"#,
    );
}

#[test]
fn numeric_leaf_callee_from_another_program_keeps_its_actual_code() {
    assert_leaf_semantics(
        r#"
function arithmetic(a,b){return a*b;}
var original=arithmetic;
var source='function arithmetic(a,b){return a+b;}';
(0,eval)(source);
var changed=arithmetic(3,4);
changed===7 && original(3,4)===12;
"#,
    );
}

#[test]
fn private_argument_buffer_growth_keeps_exact_lengths_and_rooted_snapshots() {
    assert_leaf_semantics(
        r#"
function capture(){return arguments;}
var first={},last=Symbol('last'),visits=0,log=[];
var source={ [Symbol.iterator](){var i=0;return {next(){visits++;gc();return i<33?{done:false,value:i++}:{done:true};}};} };
var values=capture((log.push('first'),first),...source,(log.push('last'),last));
var original=[first,undefined,-0,NaN,11n,last], repeated=capture(...original,...original);
var empty=capture(...[]), one=capture(...[first]), exact=capture(...[0,1,2,3,4,5,6,7]);
gc();
values.length===35 && values[0]===first && values[1]===0 && values[33]===32 && values[34]===last &&
visits===34 && log.join('|')==='first|last' && empty.length===0 && one.length===1 && one[0]===first && exact.length===8 && exact[7]===7 &&
repeated.length===12 && repeated[0]===first && repeated[6]===first && repeated[1]===undefined &&
Object.is(repeated[2],-0) && Number.isNaN(repeated[3]) && repeated[4]===11n && repeated[11]===last;
"#,
    );
}

#[test]
fn leaf_returns_still_execute_copy_values_and_make_fresh_rooted_objects() {
    assert_leaf_semantics(
        r#"
function identity(value) { return value; }
function data(value) { return {first: value, nested: [value, null]}; }
function array(value) { return [value,,value]; }
function select(value) { return value ? {value} : [value]; }
function duplicate(value, other, value) { return [value, other]; }
function strictCopy(value) { 'use strict'; return value; }
var marker = {}, log = [];
var copied = identity((log.push('value'), marker), (log.push('extra'), 17));
var one = data(marker), two = data(marker), values = array(marker);
var absent = duplicate(1), present = duplicate(1, 2, 3);
var selected = select(marker), rejected = select(0);
gc();
copied === marker && log.join('|') === 'value|extra' &&
one !== two && one.nested !== two.nested && one.first === marker && one.nested[0] === marker &&
values.length === 3 && !(1 in values) && values[0] === marker && values[2] === marker &&
absent[0] === undefined && absent[1] === undefined && present[0] === 3 && present[1] === 2 &&
selected.value === marker && rejected[0] === 0 &&
Object.is(strictCopy(-0), -0) && Number.isNaN(strictCopy(NaN)) &&
identity(12345678901234567890n) === 12345678901234567890n &&
identity.arguments === null && identity.caller === null;
"#,
    );
}

#[test]
fn callbacks_getters_and_coercion_keep_the_active_legacy_arguments_object() {
    assert_leaf_semantics(
        r#"
function invoke(callback, value) { return callback(value); }
function observe(value) {
  var args = invoke.arguments;
  return args[0] === observe && args[1] === value && observe.caller === invoke;
}
function read(value) { return value.x; }
var getterArgs;
var holder = { get x() { getterArgs = read.arguments; return getterArgs[0] === holder; } };
function convert(value) { return +value; }
var conversionArgs;
var coercible = { [Symbol.toPrimitive]() {
  conversionArgs = convert.arguments;
  return conversionArgs[0] === coercible ? 17 : 0;
} };
function freezing(callback) { return callback(); }
function freezeObserver() {
  var retained = freezing.arguments;
  Object.freeze(freezing);
  return retained;
}
var marker = {};
var called = invoke(observe, marker), got = read(holder), converted = convert(coercible);
var frozenArgs = freezing(freezeObserver);
called && got && converted === 17 && getterArgs[0] === holder && conversionArgs[0] === coercible &&
invoke.arguments === null && read.arguments === null && convert.arguments === null &&
frozenArgs[0] === freezeObserver && freezing.arguments === frozenArgs &&
Object.getOwnPropertyDescriptor(freezing, 'arguments').writable === false;
"#,
    );
}

#[test]
fn observable_arguments_keep_duplicate_mapping_capture_eval_and_parameter_initialization() {
    assert_leaf_semantics(
        r#"
function mapped(value, value) {
  value = 7;
  var first = arguments[0] === 1 && arguments[1] === 7;
  arguments[1] = 9;
  return first && value === 9;
}
function missing(value, value) {
  value = 7;
  return arguments[0] === 1 && !(1 in arguments) && value === 7;
}
function captureArguments(value) { return () => arguments; }
function captureParameter(value) { 'use strict'; return () => value; }
function evaluated(value) { 'use strict'; return eval('value + arguments[0]'); }
function defaults(value = arguments[1]) { return [value, arguments[0], arguments.length]; }
function rest(value, ...tail) { return [value, tail, arguments]; }
function destructured({value}) { return [value, arguments[0]]; }
var marker = {}, captured = captureArguments(marker), read = captureParameter(marker);
var defaulted = defaults(undefined, marker), rested = rest(1, marker), destructuredInput = {value: marker};
var unpacked = destructured(destructuredInput);
gc();
mapped(1, 2) && missing(1) && captured()[0] === marker && read() === marker && evaluated(4) === 8 &&
defaulted[0] === marker && defaulted[1] === undefined && defaulted[2] === 2 &&
rested[0] === 1 && rested[1][0] === marker && rested[2][1] === marker &&
unpacked[0] === marker && unpacked[1] === destructuredInput;
"#,
    );
}

#[test]
fn strict_parameter_storage_keeps_transitive_captures_and_resumable_cells() {
    assert_leaf_semantics(
        r#"
'use strict';
function outer(value) {
  function middle(unused) { function read() { return value; } return read; }
  return middle;
}
function mutable(value) { return [() => value, next => value = next]; }
function* resumed(value) { yield value; value += 1; return value; }
var first = {}, second = {}, read = outer(first)(0), pair = mutable(first), iterator = resumed(7);
var yielded = iterator.next();
pair[1](second);
gc();
read() === first && pair[0]() === second && yielded.value === 7 && !yielded.done &&
iterator.next().value === 8 && iterator.next().done;
"#,
    );
}

#[test]
fn leaf_construction_keeps_return_normalization_defining_realm_and_new_target_prototype() {
    assert_leaf_semantics(
        r#"
function object(value) { return {value}; }
function array(value) { return [value]; }
function primitive(value) { return value; }
class Captured { constructor(value) { this.read = () => value; } }
class Derived extends Captured {
  constructor(value) { super(value); this.supplied = arguments[0]; }
}
var marker = {}, other = __scriptwCreateRealm().global;
var classed = new Derived(marker);
var target = other.Function();
var constructedObject = Reflect.construct(object, [marker], target);
var constructedArray = Reflect.construct(array, [marker], target);
var foreignPrototype = Reflect.construct(primitive, [7], target);
target.prototype = null;
var foreignFallback = Reflect.construct(primitive, [7], target);
var plain = new primitive(7), returned = new primitive(marker);
Object.getPrototypeOf(constructedObject) === Object.prototype && constructedObject.value === marker &&
Object.getPrototypeOf(constructedArray) === Array.prototype && constructedArray[0] === marker &&
Object.getPrototypeOf(foreignPrototype) !== Object.prototype &&
Object.getPrototypeOf(foreignPrototype).constructor === target &&
Object.getPrototypeOf(foreignFallback) === other.Object.prototype &&
Object.getPrototypeOf(plain) === primitive.prototype && returned === marker &&
classed.read() === marker && classed.supplied === marker &&
object.arguments === null && array.arguments === null && primitive.arguments === null;
"#,
    );
}
