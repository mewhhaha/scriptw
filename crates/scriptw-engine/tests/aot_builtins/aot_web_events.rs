use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, ObservedCompletion, ObservedJsValue, RealmBuilder,
    RunOptions,
};

fn assert_events(source: &str) {
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
        .unwrap_or_else(|error| panic!("Web event execution: {error}"));
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{outcome:?}"
    );
    assert!(
        outcome.output_events.is_empty(),
        "unexpected callback exception: {outcome:?}"
    );
    assert!(
        matches!(
            outcome.completion,
            ObservedCompletion::Normal(ObservedJsValue::Boolean(true))
        ),
        "{outcome:?}"
    );
}

#[test]
fn abort_keeps_signal_and_first_reason_and_dispatches_synchronously_once() {
    assert_events(
        r#"
const controller = new AbortController();
const signal = controller.signal;
if (signal !== controller.signal || signal.aborted || signal.reason !== undefined) throw 'initial';
if (!(signal instanceof AbortSignal) || !(signal instanceof EventTarget)) throw 'inheritance';
signal.throwIfAborted();
const reason = {};
const trace = [];
signal.addEventListener('abort', function(event) {
  if (this !== signal || event.target !== signal || event.currentTarget !== signal || event.type !== 'abort') throw 'target';
  if (event.eventPhase !== 2 || event.bubbles || event.cancelable || !event.isTrusted) throw 'event state';
  if (!signal.aborted || signal.reason !== reason) throw 'abort ordering';
  trace.push('listener');
});
signal.onabort = () => trace.push('handler');
controller.abort(reason);
trace.push('returned');
controller.abort('replacement');
if (signal.reason !== reason || trace.join(',') !== 'listener,handler,returned') throw 'idempotent abort';
let caught = false;
try { signal.throwIfAborted(); } catch (error) { caught = error === reason; }
if (!caught) throw 'reason identity';
const defaultSignal = AbortSignal.abort();
if (!defaultSignal.aborted || !(defaultSignal.reason instanceof DOMException) || defaultSignal.reason.name !== 'AbortError' || defaultSignal.reason.code !== 20) throw 'default reason';
true;
"#,
    );
}

#[test]
fn listener_identity_mutation_once_capture_and_handler_order_are_observable() {
    assert_events(
        r#"
const target = new EventTarget();
const trace = [];
function removed() { throw 'removed'; }
function late() { trace.push('late'); }
function first() {
  trace.push('first');
  target.removeEventListener('tick', removed);
  target.addEventListener('tick', late);
}
target.addEventListener('tick', first, {once:true});
target.addEventListener('tick', first, {once:false});
target.addEventListener('tick', removed);
target.addEventListener('tick', () => trace.push('capture'), true);
target.addEventListener('tick', {handleEvent(event) { if (this.handleEvent === undefined) throw 'object receiver'; trace.push(event.type); }});
target.dispatchEvent(new Event('tick'));
target.dispatchEvent(new Event('tick'));
if (trace.join(',') !== 'capture,first,tick,capture,tick,late') throw trace.join(',');
const controller = new AbortController();
controller.signal.onabort = () => trace.push('old');
controller.signal.addEventListener('abort', () => trace.push('after'));
controller.signal.onabort = () => trace.push('new');
controller.abort();
if (trace.slice(-2).join(',') !== 'new,after') throw 'handler position';
true;
"#,
    );
}

#[test]
fn dispatch_preserves_cancellation_and_resets_transient_state() {
    assert_events(
        r#"
const target = new EventTarget();
let stoppedBefore=0;
target.addEventListener('before',()=>stoppedBefore++);
const before=new Event('before');before.stopImmediatePropagation();
target.dispatchEvent(before);
if(stoppedBefore!==0)throw 'pre-dispatch stop';
target.dispatchEvent(before);
if(stoppedBefore!==1)throw 'pre-dispatch stop cleanup';
const event = new Event('tick', {cancelable:true});
let calls = 0;
target.addEventListener('tick', e => { e.preventDefault(); calls++; }, {passive:true,once:true});
if (!target.dispatchEvent(event) || event.defaultPrevented) throw 'passive';
target.addEventListener('tick', e => { e.preventDefault(); e.stopImmediatePropagation(); calls++; });
target.addEventListener('tick', () => { throw 'stopped'; });
if (target.dispatchEvent(event) || !event.defaultPrevented || calls !== 2) throw 'cancellation';
if (event.currentTarget !== null || event.eventPhase !== 0 || event.target !== target) throw 'cleanup';
let recursion = false;
const nested = new Event('nested');
target.addEventListener('nested', () => { try { target.dispatchEvent(nested); } catch (e) { recursion = e instanceof DOMException && e.name === 'InvalidStateError' && e.code === 11; } });
target.dispatchEvent(nested);
if (!recursion) throw 'recursive event';
const signal = new AbortController().signal;
signal.onabort = () => false;
if (signal.dispatchEvent(new Event('abort', {cancelable:true}))) throw 'handler cancellation';
let trusted;
const controller = new AbortController();
controller.signal.onabort = event => { trusted = event; };
controller.abort();
if (!trusted.isTrusted) throw 'internal trust';
new EventTarget().dispatchEvent(trusted);
if (trusted.isTrusted) throw 'public trust reset';
true;
"#,
    );
}

#[test]
fn listener_abort_options_and_native_brands_cannot_be_forged() {
    assert_events(
        r#"
const target = new EventTarget();
const controller = new AbortController();
let called = 0;
target.addEventListener('tick', () => called++, {signal:controller.signal});
controller.abort();
target.dispatchEvent(new Event('tick'));
if (called) throw 'listener signal';
for (const operation of [
  () => AbortController(), () => new AbortSignal(), () => new Event(),
  () => AbortController.prototype.abort.call({}),
  () => AbortSignal.prototype.throwIfAborted.call(Object.create(AbortSignal.prototype)),
  () => EventTarget.prototype.dispatchEvent.call({}, new Event('x')),
  () => new EventTarget().dispatchEvent({type:'x'}),
]) {
  let caught = false;
  try { operation(); } catch (error) { caught = error instanceof TypeError; }
  if (!caught) throw 'native brand';
}
class Controller extends AbortController {}
const derived = new Controller();
if (!(derived instanceof Controller) || !(derived instanceof AbortController)) throw 'subclass';
true;
"#,
    );
}

#[test]
fn dom_exception_exposes_immutable_internal_name_message_and_legacy_code() {
    assert_events(
        r#"
const exception = new DOMException('detail', 'AbortError');
if (!(exception instanceof Error) || !(exception instanceof DOMException) || !Error.isError(exception)) throw 'exception prototype and brand';
if (exception.name !== 'AbortError' || exception.message !== 'detail' || exception.code !== 20) throw 'exception fields';
if (String(exception) !== 'AbortError: detail') throw 'exception string';
if (new DOMException().name !== 'Error' || new DOMException().message !== '' || new DOMException().code !== 0) throw 'exception defaults';
if (new DOMException('', 'made-up').code !== 0 || new DOMException('', 'TimeoutError').code !== 23) throw 'legacy code';
const getName = Object.getOwnPropertyDescriptor(DOMException.prototype, 'name').get;
let caught = false;
try { getName.call({name:'fake'}); } catch (error) { caught = error instanceof TypeError; }
if (!caught) throw 'exception brand';
true;
"#,
    );
}
