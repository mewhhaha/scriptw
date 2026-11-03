use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, ObservedCompletion, RealmBuilder,
    RunOptions,
};

fn assert_stream(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|error| panic!("native byte stream: {error}"));
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{outcome:?}"
    );
    assert_eq!(
        outcome.output_events,
        [HostOutputEvent::PrintLine("stream:ok".into())],
        "{outcome:?}"
    );
}

#[test]
fn queued_views_transfer_the_whole_buffer_and_close_after_drain() {
    assert_stream(
        r#"
let controller;
const stream = new ReadableStream({type:'bytes', start(c) { controller = c; }}, {highWaterMark:0});
const buffer = new ArrayBuffer(6);
new Uint8Array(buffer).set([10,11,12,13,14,15]);
controller.enqueue(new DataView(buffer, 2, 3));
if (buffer.byteLength !== 0 || controller.desiredSize !== -3) throw 'transfer/backpressure';
controller.close();
const reader = stream.getReader();
if (!stream.locked || !(reader instanceof ReadableStreamDefaultReader)) throw 'reader';
let closed = false;
reader.closed.then(() => { closed = true; });
reader.read().then(first => {
  if (first.done || !(first.value instanceof Uint8Array) || first.value.byteOffset !== 2 || first.value.byteLength !== 3 || first.value.buffer.byteLength !== 6 || first.value.join(',') !== '12,13,14') throw 'view';
  return reader.read();
}).then(last => {
  if (!last.done || last.value !== undefined || !closed || controller.desiredSize !== 0) throw 'close';
  reader.releaseLock();
  if (stream.locked) throw 'unlock';
  print('stream:ok');
});
"#,
    );
}

#[test]
fn zero_hwm_waits_for_start_and_serializes_asynchronous_pulls() {
    assert_stream(
        r#"
let start;
let active = 0;
let maximum = 0;
let calls = 0;
const source = {type:'bytes', start(c) { return new Promise(resolve => { start = resolve; }); }, pull(c) {
  if (this !== source) throw 'receiver';
  active++; maximum = Math.max(maximum, active); calls++;
  return Promise.resolve().then(() => { c.enqueue(new Uint8Array([calls])); active--; if (calls === 2) c.close(); });
}};
const stream = new ReadableStream(source, {highWaterMark:0});
const reader = stream.getReader();
const first = reader.read();
const second = reader.read();
if (calls !== 0) throw 'start gate';
start();
Promise.all([first, second, reader.closed]).then(results => {
  if (calls !== 2 || maximum !== 1 || results[0].value[0] !== 1 || results[1].value[0] !== 2) throw 'pull serialization';
  print('stream:ok');
});
"#,
    );
}

#[test]
fn cancellation_closes_pending_reads_and_preserves_reason_and_failure() {
    assert_stream(
        r#"
const reason = {};
const rejection = {};
let cancelCalls = 0;
let finish;
const source = {type:'bytes', cancel(value) {
  if (this !== source || value !== reason) throw 'cancel receiver/reason';
  cancelCalls++;
  return new Promise((resolve, reject) => { finish = () => reject(rejection); });
}};
const stream = new ReadableStream(source);
const reader = stream.getReader();
const read = reader.read();
const cancelled = reader.cancel(reason);
if (cancelCalls !== 1) throw 'synchronous cancel';
let rejected = false;
cancelled.catch(error => { if (error !== rejection) throw error; rejected = true; });
finish();
Promise.all([read, reader.closed, reader.cancel('again')]).then(results => {
  if (!results[0].done || cancelCalls !== 1) throw 'close before cancel settlement';
  return cancelled.catch(() => undefined);
}).then(() => { if (!rejected) throw 'cancel rejection'; print('stream:ok'); });
"#,
    );
}

#[test]
fn reader_release_rejects_pending_reads_without_losing_the_source() {
    assert_stream(
        r#"
let controller;
const stream = new ReadableStream({type:'bytes',start(c){controller=c;}});
const reader = stream.getReader();
let rejected = 0;
const pending = reader.read().catch(e => { if (!(e instanceof TypeError)) throw e; rejected++; });
const closed = reader.closed.catch(e => { if (!(e instanceof TypeError)) throw e; rejected++; });
reader.releaseLock();
if (stream.locked) throw 'still locked';
const next = new ReadableStreamDefaultReader(stream);
controller.enqueue(new Uint8Array([42]));
controller.close();
Promise.all([pending, closed, next.read()]).then(values => {
  if (rejected !== 2 || values[2].value[0] !== 42 || values[2].done) throw 'reader release';
  return reader.read().then(() => {throw 'released read';}, e => {if (!(e instanceof TypeError)) throw e;});
}).then(() => print('stream:ok'));
"#,
    );
}

#[test]
fn thrown_start_and_rejected_pull_keep_abrupt_completion_identity() {
    assert_stream(
        r#"
const startError = {};
try { new ReadableStream({type:'bytes',start(){throw startError;}}); throw 'missing start throw'; }
catch(error) { if(error !== startError) throw error; }
const marker = {};
const stream = new ReadableStream({type:'bytes',pull(){return Promise.reject(marker);}});
const reader = stream.getReader();
let failures = 0;
function rejected(e) { if (e !== marker) throw e; failures++; }
Promise.all([reader.read().catch(rejected),reader.closed.catch(rejected)]).then(() => reader.read().catch(rejected)).then(() => {
  if (failures !== 3) throw 'error propagation';
  print('stream:ok');
});
"#,
    );
}

#[test]
fn native_brands_options_and_webidl_descriptors_are_observable() {
    assert_stream(
        r#"
function typeError(fn) { try { fn(); } catch(e) { if (e instanceof TypeError) return; throw e; } throw 'missing TypeError'; }
typeError(() => ReadableStream({type:'bytes'}));
typeError(() => new ReadableByteStreamController());
typeError(() => new ReadableStream({type:'bytes',pull:1}));
let sizeRange = false;
try { new ReadableStream({type:'bytes'}, {size(){return 1;}}); } catch(e) { sizeRange = e instanceof RangeError; }
if (!sizeRange) throw 'byte strategy size';
const events = [];
const source = {};
for (const name of ['autoAllocateChunkSize','cancel','pull','start','type']) Object.defineProperty(source,name,{get(){ events.push(name); if(name === 'type') return 'bytes'; }});
const strategy = {get highWaterMark(){events.push('highWaterMark');return 0;},get size(){events.push('size');}};
const target = new Proxy(function(){}, {get(object,key,receiver){if(key === 'prototype') events.push('prototype');return Reflect.get(object,key,receiver);}});
const stream = Reflect.construct(ReadableStream,[source,strategy],target);
if(events.join(',') !== 'highWaterMark,size,prototype,autoAllocateChunkSize,cancel,pull,start,type') throw 'dictionary order';
for(const [proto,key] of [[ReadableStream.prototype,'locked'],[ReadableStreamDefaultReader.prototype,'closed'],[ReadableByteStreamController.prototype,'desiredSize']]) {
  const d = Object.getOwnPropertyDescriptor(proto,key);
  if(!d.enumerable || !d.configurable || d.set !== undefined) throw 'accessor descriptor';
}
if(!Object.getOwnPropertyDescriptor(ReadableStream.prototype,'getReader').enumerable) throw 'method descriptor';
let controller;
const live = new ReadableStream({type:'bytes',start(c){controller=c;}});
typeError(() => controller.enqueue(new Uint8Array(0)));
typeError(() => controller.enqueue(new Uint8Array(new SharedArrayBuffer(1))));
controller.close();
typeError(() => controller.close());
typeError(() => controller.enqueue(new Uint8Array(1)));
controller.error('ignored after close');
const getter = Object.getOwnPropertyDescriptor(ReadableStreamDefaultReader.prototype,'closed').get;
getter.call({}).catch(error => { if (!(error instanceof TypeError)) throw error; print('stream:ok'); });
"#,
    );
}

#[test]
fn positive_hwm_fills_the_queue_and_invalid_hwm_throws_range_error() {
    assert_stream(
        r#"
for (const value of [-1,NaN]) {
  let caught = false;
  try { new ReadableStream({type:'bytes'}, {highWaterMark:value}); } catch(e) { caught = e instanceof RangeError; }
  if (!caught) throw 'invalid highWaterMark';
}
let calls = 0;
let controller;
const stream = new ReadableStream({type:'bytes',start(c){controller=c;},pull(c){calls++;c.enqueue(new Uint8Array([calls]));}}, {highWaterMark:2});
Promise.resolve().then(() => Promise.resolve()).then(() => Promise.resolve()).then(() => {
  if(calls !== 2 || controller.desiredSize !== 0) throw 'highWaterMark fill';
  return stream.cancel();
}).then(() => print('stream:ok'));
"#,
    );
}

#[test]
fn callback_throw_and_returned_promises_follow_webidl_assimilation() {
    assert_stream(
        r#"
const trace = [];
const thrown = {};
const stream = new ReadableStream({type:'bytes',cancel(){trace.push('cancel');throw thrown;}});
stream.cancel().catch(error => {if(error !== thrown) throw error;trace.push('rejected');});
Promise.resolve().then(() => trace.push('checkpoint'));
// Stream promise conversion must use then assimilation, not PromiseResolve's
// constructor identity shortcut.
let thenGets = 0;
const returned = Promise.resolve();
Object.defineProperty(returned, 'constructor', {get(){throw 'constructor observed';}});
// A custom then avoids the unrelated species lookup performed by the public
// Promise.prototype.then method; the conversion itself must not read constructor.
Object.defineProperty(returned, 'then', {get(){thenGets++;return function(resolve){resolve();};}, configurable:true});
let controller;
const ready = new ReadableStream({type:'bytes',start(c){controller=c;return returned;}});
const reader = ready.getReader();
const done = reader.read();
Promise.resolve().then(() => controller.close());
Promise.all([done, reader.closed]).then(() => Promise.resolve()).then(() => {
  if(thenGets !== 1 || trace.join(',') !== 'cancel,checkpoint,rejected') throw 'assimilation/order';
  print('stream:ok');
});
"#,
    );
}

#[test]
fn close_snapshots_pending_reads_before_result_then_getters_can_reenter() {
    assert_stream(
        r#"
let controller;
const stream = new ReadableStream({type:'bytes',start(c){controller=c;}});
const reader = stream.getReader();
const first = reader.read();
const second = reader.read();
let seen = 0;
Object.defineProperty(Object.prototype, 'then', {configurable:true,get(){
  if(Object.prototype.hasOwnProperty.call(this,'done')) {
    seen++;
    if(seen === 1) reader.releaseLock();
  }
  return undefined;
}});
controller.close();
delete Object.prototype.then;
Promise.all([first,second]).then(values => {
  if(seen !== 2 || !values[0].done || !values[1].done || stream.locked) throw 'snapshot';
  print('stream:ok');
});
"#,
    );
}
