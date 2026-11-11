use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_script(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compiler worker");
    let observation = Engine::new(RealmBuilder::new().build())
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
        .expect("HasInstance local entry behavior executes through Wasm AOT");
    assert_eq!(observation.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observation.completion, ObservedCompletion::Normal(_)),
        "{:?}\n{source}",
        observation.completion,
    );
    assert_eq!(
        observation.output_events,
        vec![HostOutputEvent::PrintLine("ok".into())],
        "{source}",
    );
}

#[test]
fn direct_default_has_instance_calls_preserve_missing_extra_and_constructor_rules() {
    assert_script(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
var intrinsic = Function.prototype[Symbol.hasInstance];
function Parent() {}
var child = Object.create(Parent.prototype);
var notChild = {};
assert(intrinsic.call(Parent, child) === true, 'direct vector call');
assert(intrinsic.call(Parent, child, 'ignored', 3) === true, 'extra vector arguments are ignored');
assert(intrinsic.call(Parent) === false, 'missing candidate is undefined');
assert(intrinsic.call(Parent, notChild) === false, 'prototype chain mismatch');
assert(intrinsic.call({}, child) === false, 'non-callable receiver returns false');
var bound = Parent.bind(null);
assert(intrinsic.call(bound, child) === true, 'bound target prototype walk');
var arrowError = false;
try { intrinsic.call(() => {}, child); }
catch (error) { arrowError = error instanceof TypeError; }
assert(arrowError, 'callable without object prototype throws');
var constructError = false;
try { new intrinsic(); }
catch (error) { constructError = error instanceof TypeError; }
assert(constructError, 'the native method is not a constructor');
print('ok');
"#,
    );
}

#[test]
fn custom_instanceof_hooks_and_proxy_prototype_failures_keep_order_and_cleanup() {
    assert_script(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
var trace = [], candidate = {}, receiver = {}, getterCalls = 0, methodReceiver;
Object.defineProperty(receiver, Symbol.hasInstance, {
  get() {
    getterCalls++;
    trace.push('get');
    return function(value) {
      trace.push('call');
      methodReceiver = this;
      return value === candidate;
    };
  }
});
var customResult = candidate instanceof receiver;
var boundReceiver = {}, boundTarget = {};
Object.defineProperty(boundTarget, Symbol.hasInstance, {
  get() {
    trace.push('bound-get');
    return function(value) {
      trace.push('bound-call');
      gc();
      return this === boundReceiver && value === candidate;
    }.bind(boundReceiver);
  }
});
var boundResult = candidate instanceof boundTarget;
var proxyTarget, proxyHandler = new Proxy(function(value) {}, {
  apply(target, actualReceiver, args) {
    trace.push('proxy-call');
    gc();
    return actualReceiver === proxyTarget && args[0] === candidate;
  }
});
proxyTarget = new Proxy({}, {
  get(target, key, actualReceiver) {
    if (key === Symbol.hasInstance) {
      trace.push('proxy-get');
      return proxyHandler;
    }
    return Reflect.get(target, key, actualReceiver);
  }
});
var proxyResult = candidate instanceof proxyTarget;
var proxied = new Proxy(function Target() {}, {
  get(target, key, actualReceiver) {
    if (key === 'prototype') {
      trace.push('prototype');
      gc();
      if (actualReceiver !== proxied) throw 'proxy receiver';
      throw marker;
    }
    return Reflect.get(target, key, actualReceiver);
  }
});
var marker = {}, caught = false;
try { Function.prototype[Symbol.hasInstance].call(proxied, candidate); }
catch (error) { caught = error === marker; }
finally { trace.push('finally'); }
assert(customResult && getterCalls === 1 && methodReceiver === receiver,
  'ordinary instanceof reads and invokes the custom hook once');
assert(boundResult && proxyResult, 'bound and Proxy handlers use generic callback dispatch');
assert(caught, 'direct native call preserves the Proxy prototype throw');
assert(trace.join(',') === 'get,call,bound-get,bound-call,proxy-get,proxy-call,prototype,finally',
  'hook, bound/Proxy call, and abrupt-completion order');
print('ok');
"#,
    );
}

#[test]
fn instanceof_operator_hides_native_caller_and_restores_after_callback_throw() {
    assert_script(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
var prototype = {}, candidate = Object.create(prototype), marker = {};
var shouldThrow = true, seen = 'unset';
function prototypeGetter() {
  gc();
  if (shouldThrow) {
    shouldThrow = false;
    throw marker;
  }
  seen = prototypeGetter.caller;
  return prototype;
}
var callable = () => {};
Object.defineProperty(callable, 'prototype', {get: prototypeGetter});
function trigger() { return candidate instanceof callable; }
var caught = false;
try { trigger(); }
catch (error) { caught = error === marker; }
assert(caught, 'getter throw identity survives native local entry');
function directCaller() { prototypeGetter(); }
directCaller();
assert(seen === directCaller, 'native caller boundary was restored after the throw');
assert(trigger() === true && seen === null, 'operator callback sees an opaque native caller');
print('ok');
"#,
    );
}

#[test]
fn direct_ordinary_has_instance_restarts_through_the_method_before_user_code() {
    // The operator may skip calling the default method only while nothing
    // observable runs. A "prototype" getter and a getPrototypeOf trap must
    // still see the native method as their caller, and a method that an
    // accessor produced must not be looked up a second time.
    assert_script(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
var lookups = 0, prototypeReads = 0, seen = 'unset', prototype = {};
var callable = () => {};
Object.defineProperty(callable, Symbol.hasInstance, {
  get: function () { lookups++; return Function.prototype[Symbol.hasInstance]; }
});
function prototypeGetter() { prototypeReads++; seen = prototypeGetter.caller; return prototype; }
Object.defineProperty(callable, 'prototype', { get: prototypeGetter });
var candidate = Object.create(prototype);
function trigger(value) { return value instanceof callable; }
assert(trigger(candidate) === true, 'accessor prototype result');
assert(lookups === 1 && prototypeReads === 1 && seen === null, 'one lookup, one read, native caller: ' + lookups + ' ' + prototypeReads + ' ' + seen);
assert(trigger({}) === false && lookups === 2 && prototypeReads === 2 && seen === null, 'second use');

var traps = 0, trapCaller = 'unset';
function Plain() {}
function trap(target) { traps++; trapCaller = trap.caller; return Plain.prototype; }
var proxied = Object.create(new Proxy({}, { getPrototypeOf: trap }));
function plain(value) { return value instanceof Plain; }
assert(plain(new Plain()) === true && traps === 0, 'ordinary chain needs no trap');
assert(plain(proxied) === true && traps === 1 && trapCaller === null, 'trap runs once under the native method: ' + traps + ' ' + trapCaller);
assert(plain({}) === false && plain(proxied) === true && traps === 2, 'later uses');
print('ok');
"#,
    );
}
