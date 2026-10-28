use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn check_bootstrap(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
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
        .expect("compact intrinsic installation executes through compiled Wasm");
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "completion: {:?}; source:\n{source}",
        outcome.completion,
    );
    assert_eq!(
        outcome.output_events,
        [HostOutputEvent::PrintLine("ok".into())],
        "source:\n{source}",
    );
}

#[test]
fn ordered_method_rows_preserve_descriptors_native_metadata_and_eager_identities() {
    check_bootstrap(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var keys = Reflect.ownKeys(Math);
check(keys.slice(0, -1).join(',') ===
  'E,LN10,LN2,LOG10E,LOG2E,PI,SQRT1_2,SQRT2,abs,acos,acosh,asin,asinh,atan,atan2,atanh,cbrt,ceil,clz32,cos,cosh,exp,expm1,f16round,floor,fround,hypot,imul,log,log10,log1p,log2,pow,random,round,sign,sin,sinh,sqrt,sumPrecise,tan,tanh,trunc,min,max', 'Math key order');
check(keys[keys.length - 1] === Symbol.toStringTag, 'Math symbol order');
var cases = [
  [Array, 'from', 'from', 1], [Array.prototype, 'map', 'map', 1],
  [Object, 'entries', 'entries', 1], [Math, 'abs', 'abs', 1],
  [Date.prototype, 'getUTCFullYear', 'getUTCFullYear', 0],
  [Map.prototype, 'clear', 'clear', 0], [Number.prototype, 'toFixed', 'toFixed', 1],
  [String.prototype, 'charAt', 'charAt', 1], [Promise, 'all', 'all', 1],
  [RegExp.prototype, Symbol.match, '[Symbol.match]', 1],
  [Temporal.Instant.prototype, 'add', 'add', 1]
];
for (var item of cases) {
  var descriptor = Object.getOwnPropertyDescriptor(item[0], item[1]);
  check(descriptor.writable && !descriptor.enumerable && descriptor.configurable,
    'method attributes ' + item[2]);
  check(descriptor.get === undefined && descriptor.set === undefined,
    'data descriptor ' + item[2]);
  check(descriptor.value.name === item[2] && descriptor.value.length === item[3],
    'native metadata ' + item[2]);
  check(Object.getPrototypeOf(descriptor.value) === Function.prototype,
    'function prototype ' + item[2]);
}
var primitive = Object.getOwnPropertyDescriptor(Date.prototype, Symbol.toPrimitive);
check(!primitive.writable && !primitive.enumerable && primitive.configurable,
  'nonwritable Symbol method');
check(primitive.value.name === '[Symbol.toPrimitive]' && primitive.value.length === 1,
  'Symbol native metadata');
check(Array.prototype.values === Array.prototype[Symbol.iterator], 'Array alias');
check(Array.prototype.constructor === Array && Function.prototype.constructor === Function,
  'eager constructor cycles');
check(Uint8Array.prototype.toString === Array.prototype.toString, 'canonical method identity');
check(Date.prototype.toUTCString === Date.prototype.toGMTString, 'Date alias');
check(String.prototype.trimStart === String.prototype.trimLeft &&
  String.prototype.trimEnd === String.prototype.trimRight, 'String aliases');
var species = Object.getOwnPropertyDescriptor(Array, Symbol.species);
check(typeof species.get === 'function' && species.set === undefined &&
  !species.enumerable && species.configurable && species.get.call(Array) === Array,
  'accessor boundary');
var objectKeys = Reflect.ownKeys(Object.prototype);
check(objectKeys.indexOf('__lookupSetter__') < objectKeys.indexOf('__proto__') &&
  objectKeys.indexOf('__proto__') < objectKeys.indexOf('propertyIsEnumerable'),
  'accessor installation order');
print('ok');
"#,
    );
}

#[test]
fn replacement_deletion_and_alias_descriptor_mutation_keep_independent_state() {
    check_bootstrap(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var marker = {};
Object.defineProperty(Math, 'acosh', { value: marker });
check(Math.acosh === marker && Math.abs(-7) === 7, 'replace unobserved recipe');
delete Math.acos;
Math.acos = marker;
var keys = Reflect.ownKeys(Math);
check(keys[keys.length - 2] === 'acos' && keys[keys.length - 1] === Symbol.toStringTag,
  'delete and reinsert order');
check(Object.getOwnPropertyDescriptor(Math, 'acos').enumerable, 'reinsert descriptor');
var trim = String.prototype.trimLeft;
Object.defineProperty(String.prototype, 'trimStart', { value: marker, writable: false });
check(String.prototype.trimLeft === trim && trim.call(' x ') === 'x ' &&
  String.prototype.trimStart === marker, 'alias recipe identity and independent descriptors');
check(Object.getOwnPropertyDescriptor(String.prototype, 'trimLeft').writable,
  'alias descriptor attributes remain independent');
var values = Array.prototype.values;
delete Array.prototype[Symbol.iterator];
Array.prototype[Symbol.iterator] = marker;
check(Array.prototype.values === values && Array.prototype[Symbol.iterator] === marker,
  'canonical alias replacement');
var symbols = Object.getOwnPropertySymbols(Array.prototype);
check(symbols.indexOf(Symbol.unscopables) < symbols.indexOf(Symbol.iterator),
  'Symbol reinsert order');
Object.defineProperty(String.prototype, 'toWellFormed', {
  get: function() { return marker; }, configurable: true
});
check(String.prototype.toWellFormed === marker, 'accessor replacement before materialization');
print('ok');
"#,
    );
}

#[test]
fn fresh_realms_capture_the_defining_realm_for_borrowed_methods() {
    check_bootstrap(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var left = __scriptwCreateRealm().global;
var right = __scriptwCreateRealm().global;
var marker = {};
left.Math.cos = marker;
check(left.Math.cos === marker && typeof right.Math.cos === 'function' &&
  typeof Math.cos === 'function', 'fresh recipe state');
check(left.Math.abs !== right.Math.abs && left.Math.abs !== Math.abs &&
  Object.getPrototypeOf(left.Math.abs) === left.Function.prototype &&
  Object.getPrototypeOf(right.Math.abs) === right.Function.prototype,
  'native identity and defining Function prototype');
Object.defineProperty(left.Math.abs, 'length', { value: 9 });
check(right.Math.abs.length === 1 && Math.abs.length === 1, 'fresh native metadata');
var entries = left.Object.entries({ a: 3 });
check(Object.getPrototypeOf(entries) === left.Array.prototype &&
  Object.getPrototypeOf(entries[0]) === left.Array.prototype && entries[0][1] === 3,
  'borrowed Object method allocations');
var mapped = left.Array.prototype.map.call({ 0: 4, length: 1 }, function(x) { return x + 1; });
check(Object.getPrototypeOf(mapped) === left.Array.prototype && mapped[0] === 5,
  'borrowed Array method allocations');
var threw = false;
try { left.Array.prototype.map.call(null, function(x) { return x; }); }
catch (error) { threw = Object.getPrototypeOf(error) === left.TypeError.prototype; }
check(threw, 'borrowed method error Realm');
check(left.Array.prototype.values === left.Array.prototype[Symbol.iterator] &&
  left.Array.prototype.values !== right.Array.prototype.values &&
  left.Date.prototype.toUTCString === left.Date.prototype.toGMTString,
  'Realm-local aliases');
delete left.Date.prototype.toISOString;
check(typeof right.Date.prototype.toISOString === 'function' &&
  typeof Date.prototype.toISOString === 'function', 'fresh descriptor state');
var species = Object.getOwnPropertyDescriptor(left.Array, Symbol.species);
check(Object.getPrototypeOf(species.get) === left.Function.prototype &&
  species.get.call(right.Array) === right.Array, 'foreign accessor identity');
print('ok');
"#,
    );
}
