use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, ObservedJsValue, ObservedNumber, RealmBuilder, RunOptions,
};

fn assert_modes(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{source}");
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
                CompileOptions {
                    host_surface_policy: HostSurfacePolicy::Test262,
                    ..CompileOptions::default()
                },
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(120_000),
                    ..RunOptions::default()
                },
            )
            .unwrap_or_else(|error| panic!("Error demand initialization: {error}\n{source}"));
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(262.0))),
            "{source}",
        );
        assert_eq!(
            observed.output_events,
            [HostOutputEvent::PrintLine("ok".into())]
        );
    }
}

#[test]
fn first_error_descriptor_publishes_complete_defining_realm_graph_with_original_order() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var names = ['Error','EvalError','AggregateError','SuppressedError','RangeError','SyntaxError','TypeError','URIError','ReferenceError'];
var retained = [];
for (var global of [globalThis, __scriptwCreateRealm().global, __scriptwCreateRealm().global]) {
  var before = Reflect.ownKeys(global);
  for (var name of names) check(before.indexOf(name) >= 0 && Reflect.has(global, name) &&
    Object.hasOwn(global, name) && !Object.prototype.propertyIsEnumerable.call(global, name), name + ' membership');
  var first = Object.getOwnPropertyDescriptor(global, 'TypeError');
  check(typeof first.value === 'function' && first.value === global.TypeError && first.writable &&
    !first.enumerable && first.configurable && !Object.hasOwn(first, 'get'), 'first descriptor is data');
  for (var name of names) {
    var property = Object.getOwnPropertyDescriptor(global, name), C = property.value, prototype = C.prototype;
    check(C === global[name] && property.writable && !property.enumerable && property.configurable &&
      !Object.hasOwn(property, 'get'), name + ' ordinary public property');
    check(C.name === name && C.length === (name === 'AggregateError' ? 2 : name === 'SuppressedError' ? 3 : 1) &&
      prototype.constructor === C && prototype.name === name && prototype.message === '', name + ' metadata cycle');
    var ownPrototype = Object.getOwnPropertyDescriptor(C, 'prototype');
    check(!ownPrototype.writable && !ownPrototype.enumerable && !ownPrototype.configurable, name + ' prototype flags');
    for (var key of ['name','length']) {
      var metadata = Object.getOwnPropertyDescriptor(C, key);
      check(!metadata.writable && !metadata.enumerable && metadata.configurable, name + ' function metadata flags');
    }
    for (var key of ['name','message','constructor']) {
      var member = Object.getOwnPropertyDescriptor(prototype, key);
      check(member.writable && !member.enumerable && member.configurable, name + ' prototype property flags');
    }
    check(Object.getPrototypeOf(C) === (name === 'Error' ? global.Function.prototype : global.Error) &&
      Object.getPrototypeOf(prototype) === (name === 'Error' ? global.Object.prototype : global.Error.prototype),
      name + ' inheritance');
    check(Reflect.ownKeys(prototype).join('|') === (name === 'Error' ?
      'name|message|constructor|toString|stack' : 'name|message|constructor'), name + ' prototype insertion order');
    check(Reflect.ownKeys(C).join('|') === (name === 'Error' ? 'length|name|prototype|isError' : 'length|name|prototype'),
      name + ' constructor insertion order');
    check(!global.Error.isError(prototype), name + ' prototype has no ErrorData');
  }
  var stack = Object.getOwnPropertyDescriptor(global.Error.prototype, 'stack');
  check(typeof stack.get === 'function' && typeof stack.set === 'function' && !stack.enumerable && stack.configurable &&
    Object.getPrototypeOf(stack.get) === global.Function.prototype && Object.getPrototypeOf(stack.set) === global.Function.prototype,
    'stack accessor defining Realm');
  check(Object.getPrototypeOf(global.Error.prototype.toString) === global.Function.prototype &&
    Object.getPrototypeOf(global.Error.isError) === global.Function.prototype, 'deferred methods defining Realm');
  check(Reflect.ownKeys(global).join('|') === before.join('|'), 'global insertion order retained');
  retained.push(global.Error);
}
gc();
check(retained[0] !== retained[1] && retained[1] !== retained[2] && retained[0].prototype !== retained[1].prototype &&
  retained[1].prototype.constructor === retained[1], 'independent stable Realm identities');
print('ok'); 262;
"#,
    );
}

#[test]
fn private_error_defaults_preserve_every_unread_public_constructor_mutation() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var names = ['Error','EvalError','AggregateError','SuppressedError','RangeError','SyntaxError','TypeError','URIError','ReferenceError'];
for (var mode of ['delete','assign','define','accessor']) {
  var global = __scriptwCreateRealm().global, marker = {}, gets = 0, sets = 0;
  var target = global.Array.bind(null);
  Object.defineProperty(target, 'prototype', {value: 17});
  for (var name of names) {
    if (mode === 'delete') check(Reflect.deleteProperty(global, name), 'delete unread global');
    else if (mode === 'assign') global[name] = marker;
    else if (mode === 'define') Object.defineProperty(global, name, {value: marker, writable: false, configurable: false});
    else Object.defineProperty(global, name, {get: function() {gets++; return marker;}, set: function() {sets++;}, configurable: true});
  }
  var prototypes = [], constructors = [], canonicalError;
  for (var name of names) {
    var args = name === 'AggregateError' ? [[], 'message'] : name === 'SuppressedError' ? [1, 2, 'message'] : ['message'];
    var instance = Reflect.construct(globalThis[name], args, target), prototype = Object.getPrototypeOf(instance), C = prototype.constructor;
    if (name === 'Error') canonicalError = C;
    check(C.name === name && C.prototype === prototype && C !== globalThis[name] && instance.message === 'message' &&
      Object.getPrototypeOf(C) === (name === 'Error' ? global.Function.prototype : canonicalError) &&
      Object.getPrototypeOf(prototype) === (name === 'Error' ? global.Object.prototype : canonicalError.prototype),
      name + ' foreign canonical default');
    prototypes.push(prototype); constructors.push(C);
  }
  check(gets === 0 && sets === 0, 'private graph never consults public accessors');
  for (var name of names) {
    var descriptor = Object.getOwnPropertyDescriptor(global, name);
    if (mode === 'delete') check(descriptor === undefined && !Object.hasOwn(global, name), 'deletion retained');
    else if (mode === 'accessor') check(typeof descriptor.get === 'function' && typeof descriptor.set === 'function', 'accessor retained');
    else check(descriptor.value === marker && descriptor.writable === (mode === 'assign'), 'replacement retained');
  }
  gc();
  for (var i = 0; i < names.length; ++i) {
    var name = names[i], args = name === 'AggregateError' ? [[]] : name === 'SuppressedError' ? [1,2] : [];
    check(Object.getPrototypeOf(Reflect.construct(globalThis[name], args, target)) === prototypes[i] &&
      prototypes[i].constructor === constructors[i], 'stable private graph after GC');
  }
  check(gets === 0 && sets === 0, 'repeat private defaults bypass public accessors');
}
print('ok'); 262;
"#,
    );
}

#[test]
fn first_error_publication_ignores_inherited_hooks_and_keeps_locked_data_descriptors() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
for (var mode of ['attributes','seal','freeze']) {
  var global = __scriptwCreateRealm().global;
  if (mode === 'attributes') Object.defineProperty(global, 'RangeError', {writable: false, configurable: false});
  else Object[mode](global);
  var descriptor = Object.getOwnPropertyDescriptor(global, 'RangeError'), C = descriptor.value;
  check(typeof C === 'function' && !Object.hasOwn(descriptor, 'get') && !descriptor.enumerable &&
    !descriptor.configurable && descriptor.writable === (mode === 'seal'), 'locked ordinary data descriptor');
  check(!Reflect.deleteProperty(global, 'RangeError') && Reflect.defineProperty(global, 'RangeError', {value: C}) &&
    new C('x').message === 'x' && C.prototype.constructor === C, 'complete graph behind locked global');
}
var global = __scriptwCreateRealm().global, hooks = 0;
var objectPrototype = global.Object.prototype, functionPrototype = global.Function.prototype;
function forbidden() { hooks++; throw new Error('initialization invoked user code'); }
Object.defineProperty(global, 'Object', {get: forbidden, configurable: true});
Object.defineProperty(global, 'Function', {get: forbidden, configurable: true});
for (var key of ['constructor','name','message','toString','stack'])
  Object.defineProperty(objectPrototype, key, {get: forbidden, set: forbidden, configurable: true});
for (var key of ['name','length','prototype'])
  Object.defineProperty(functionPrototype, key, {get: forbidden, set: forbidden, configurable: true});
var C = Object.getOwnPropertyDescriptor(global, 'ReferenceError').value;
check(C.name === 'ReferenceError' && C.prototype.constructor === C && Object.getPrototypeOf(Object.getPrototypeOf(C)) === functionPrototype &&
  hooks === 0, 'initialization has no user callbacks');
print('ok'); 262;
"#,
    );
}

#[test]
fn internal_errors_initialize_canonical_graphs_without_restoring_public_globals() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var names = ['Error','EvalError','AggregateError','SuppressedError','RangeError','SyntaxError','TypeError','URIError','ReferenceError'];
var globals = [], captured = [];
for (var operation of ['type','range','reference']) {
  var global = __scriptwCreateRealm().global, gets = 0;
  for (var name of names) Object.defineProperty(global, name, {get: function() { gets++; return null; }, configurable: true});
  var thrown;
  try {
    if (operation === 'type') global.Object.getPrototypeOf(null);
    else if (operation === 'range') global.Number.prototype.toFixed.call(1, 101);
    else {var stack = new global.DisposableStack(); stack.dispose(); stack.defer(function() {});}
  } catch (error) { thrown = error; }
  var prototype = Object.getPrototypeOf(thrown), C = prototype.constructor;
  check(C.name === (operation === 'type' ? 'TypeError' : operation === 'range' ? 'RangeError' : 'ReferenceError') &&
    Object.getPrototypeOf(C) === Object.getPrototypeOf(prototype).constructor && gets === 0,
    'internal error canonical Realm and no public lookup');
  check(Object.getPrototypeOf(Object.getPrototypeOf(C)) === global.Function.prototype &&
    typeof Object.getOwnPropertyDescriptor(global, 'TypeError').get === 'function', 'public accessors survive');
  globals.push(global); captured.push(thrown);
}
gc();
for (var i = 0; i < captured.length; ++i) check(captured[i].constructor.prototype === Object.getPrototypeOf(captured[i]) &&
  Object.getPrototypeOf(Object.getPrototypeOf(captured[i].constructor)) === globals[i].Function.prototype, 'rooted internal error graph');
print('ok'); 262;
"#,
    );
}

#[test]
fn aggregate_and_suppressed_internal_paths_keep_private_error_realms_and_instance_data() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var global = __scriptwCreateRealm().global, gets = 0, first = {}, second = {};
for (var name of ['Error','EvalError','AggregateError','SuppressedError','RangeError','SyntaxError','TypeError','URIError','ReferenceError'])
  Object.defineProperty(global, name, {get: function() { gets++; return null; }, configurable: true});
var stack = new global.DisposableStack(), suppressed;
stack.defer(function() {throw first;}); stack.defer(function() {throw second;});
try {stack.dispose();} catch (error) {suppressed = error;}
check(suppressed.constructor.name === 'SuppressedError' && suppressed.error === first && suppressed.suppressed === second &&
  Object.getPrototypeOf(Object.getPrototypeOf(suppressed.constructor)) === global.Function.prototype && gets === 0,
  'disposal allocates private canonical SuppressedError');
var promiseGlobal = __scriptwCreateRealm().global, promiseGets = 0;
for (var name of ['Error','EvalError','AggregateError','SuppressedError','RangeError','SyntaxError','TypeError','URIError','ReferenceError'])
  Object.defineProperty(promiseGlobal, name, {get: function() { promiseGets++; return null; }, configurable: true});
promiseGlobal.Promise.any([]).then(function() {throw new Error('unexpected fulfillment');}, function(aggregate) {
  check(aggregate.constructor.name === 'AggregateError' && aggregate.errors.length === 0 &&
    Object.getPrototypeOf(Object.getPrototypeOf(aggregate.constructor)) === promiseGlobal.Function.prototype &&
    Object.getPrototypeOf(aggregate.constructor) !== Object.getPrototypeOf(suppressed.constructor) && promiseGets === 0,
    'Promise.any initializes its independent private AggregateError graph');
  gc();
  check(suppressed.error === first && aggregate.errors.length === 0 &&
    typeof Object.getOwnPropertyDescriptor(global, 'SuppressedError').get === 'function' &&
    typeof Object.getOwnPropertyDescriptor(promiseGlobal, 'AggregateError').get === 'function', 'internal graphs remain rooted');
  print('ok');
});
262;
"#,
    );
}

#[test]
fn first_error_call_keeps_cause_stack_and_repeated_read_identity() {
    assert_modes(
        r#"
function check(value, label) { if (!value) throw new Error(label); }
var global = __scriptwCreateRealm().global, cause = {}, order = [];
var message = {toString: function() {order.push('message'); return 'failure';}};
var options = {get cause() {order.push('cause'); return cause;}};
var C = global.TypeError, error = C(message, options);
check(order.join('|') === 'message|cause' && error.cause === cause && error.message === 'failure' &&
  error.toString() === 'TypeError: failure' && global.Error.isError(error), 'message cause and ErrorData');
var descriptor = Object.getOwnPropertyDescriptor(error, 'cause');
check(descriptor.value === cause && descriptor.writable && !descriptor.enumerable && descriptor.configurable, 'cause descriptor');
check(typeof error.stack === 'string', 'captured stack');
error.stack = 'retained stack';
check(error.stack === 'retained stack' && C === global.TypeError && C === Object.getOwnPropertyDescriptor(global, 'TypeError').value,
  'stack setter and repeated canonical reads');
gc();
check(error.cause === cause && Object.getPrototypeOf(error) === C.prototype && error.stack === 'retained stack', 'instance and graph survive GC');
print('ok'); 262;
"#,
    );
}
