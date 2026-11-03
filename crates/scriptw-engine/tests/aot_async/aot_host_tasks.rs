use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, ObservedCompletion, RealmBuilder,
    RunOptions,
};

fn run(source: &str) -> scriptw_engine::ObservedRunOutcome {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("host tasks execute compiled Wasm")
}

#[test]
fn timers_are_tasks_and_microtasks_share_the_promise_fifo() {
    let outcome = run(r#"
const trace = [];
setTimeout((a,b) => {
  trace.push(a+b);
  queueMicrotask(() => trace.push('after-a'));
  Promise.resolve().then(() => trace.push('promise-a'));
}, 0, 'timer', '-a');
setTimeout(() => { trace.push('timer-b'); print(trace.join(',')); }, 0);
queueMicrotask(() => { trace.push('micro-a'); queueMicrotask(() => trace.push('micro-b')); });
Promise.resolve().then(() => trace.push('promise'));
trace.push('sync');
"#);
    assert!(matches!(outcome.completion, ObservedCompletion::Normal(_)));
    assert_eq!(
        outcome.output_events,
        [HostOutputEvent::PrintLine(
            "sync,micro-a,promise,micro-b,timer-a,after-a,promise-a,timer-b".into()
        )]
    );
}

#[test]
fn timers_wait_for_real_deadlines_and_cancellation_does_not_wait() {
    let outcome = run(r#"
const trace = [];
const start = Date.now();
const cancelled = setTimeout(() => { throw 'cancelled timer ran'; }, 100000);
clearTimeout(cancelled);
setTimeout(() => { trace.push('late'); print(trace.join(',')); print(Date.now()-start >= 10); }, 20);
setTimeout(function(a) { trace.push(a); print(this === globalThis); }, 0, 'early');
clearTimeout(-1);
"#);
    assert!(matches!(outcome.completion, ObservedCompletion::Normal(_)));
    assert_eq!(
        outcome.output_events,
        [
            HostOutputEvent::PrintLine("true".into()),
            HostOutputEvent::PrintLine("early,late".into()),
            HostOutputEvent::PrintLine("true".into()),
        ]
    );
}

#[test]
fn task_exceptions_are_reported_and_the_remaining_queue_continues() {
    let outcome = run(r#"
globalThis.Promise = function() { throw 'Promise constructor consulted'; };
queueMicrotask(() => { print('called'); throw 'microtask failure'; });
queueMicrotask(() => print('next microtask'));
setTimeout(() => { throw 'timer failure'; }, 0);
setTimeout(() => print('next timer'), 0);
print('scheduled');
"#);
    assert!(matches!(outcome.completion, ObservedCompletion::Normal(_)));
    let events = outcome.output_events;
    assert_eq!(events.len(), 6);
    assert_eq!(events[0], HostOutputEvent::PrintLine("scheduled".into()));
    assert_eq!(events[1], HostOutputEvent::PrintLine("called".into()));
    assert!(
        matches!(&events[2], HostOutputEvent::ReportedException(message) if message.contains("microtask failure"))
    );
    assert_eq!(
        events[3],
        HostOutputEvent::PrintLine("next microtask".into())
    );
    assert!(
        matches!(&events[4], HostOutputEvent::ReportedException(message) if message.contains("timer failure"))
    );
    assert_eq!(events[5], HostOutputEvent::PrintLine("next timer".into()));
}

#[test]
fn task_callbacks_validate_callability_and_preserve_coercion_errors() {
    let outcome = run(r#"
for (const operation of [queueMicrotask, setTimeout]) {
  try { operation(0); } catch (error) { print(error instanceof TypeError); }
}

const marker = {};
try { setTimeout(() => {}, {valueOf() { throw marker; }}); }
catch (error) { print(error === marker); }
try { clearTimeout({valueOf() { throw marker; }}); }
catch (error) { print(error === marker); }
"#);
    assert!(matches!(outcome.completion, ObservedCompletion::Normal(_)));
    assert_eq!(
        outcome.output_events,
        vec![HostOutputEvent::PrintLine("true".into()); 4]
    );
}

#[test]
fn callbacks_keep_their_defining_realm_and_timer_owner_separate() {
    use scriptw_engine::HostSurfacePolicy;
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let outcome = Engine::new(RealmBuilder::new().build()).observe_script(r#"
const realm = __scriptwCreateRealm();
realm.evalScript("globalThis.micro = function() { 'use strict'; globalThis.microCheck = this === undefined; }; globalThis.timer = function(expected, label) { 'use strict'; globalThis[label] = this === expected; };");
const foreign = realm.global;
const foreignSetTimeout = foreign.setTimeout;
const foreignClearTimeout = foreign.clearTimeout;
queueMicrotask(foreign.micro);
setTimeout(foreign.timer, 0, globalThis, 'entryOwnerCheck');
foreignSetTimeout(foreign.timer, 0, foreign, 'foreignOwnerCheck');
const foreignId = foreignSetTimeout(() => { foreign.foreignSurvived = true; }, 0);
clearTimeout(foreignId);
const entryId = setTimeout(() => { foreign.entrySurvived = true; }, 0);
foreignClearTimeout(entryId);
const ownEntryId = setTimeout(() => { throw 'entry cancellation failed'; }, 0);
clearTimeout(ownEntryId);
const ownForeignId = foreignSetTimeout(() => { throw 'foreign cancellation failed'; }, 0);
foreignClearTimeout(ownForeignId);
setTimeout(() => {
  print(foreign.microCheck);
  print(foreign.entryOwnerCheck);
  print(foreign.foreignOwnerCheck);
  print(foreign.foreignSurvived);
  print(foreign.entrySurvived);
}, 0);
"#, CompileOptions { host_surface_policy: HostSurfacePolicy::Test262, ..CompileOptions::default() }, RunOptions {
        backend: ExecutionBackend::WasmAot, timeout_ms: Some(30_000), ..RunOptions::default()
    }).expect("callbacks from created Realms execute through Wasm");
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "completion: {:?}; output: {:?}; note: {}",
        outcome.completion,
        outcome.output_events,
        outcome.note,
    );
    assert_eq!(
        outcome.output_events,
        vec![HostOutputEvent::PrintLine("true".into()); 5]
    );
}

#[test]
fn waiting_for_a_timer_keeps_earlier_atomics_deadlines_live() {
    let outcome = run(r#"
const trace = [];
const cells = new Int32Array(new SharedArrayBuffer(4));
const longTimer = setTimeout(() => { throw 'waiter failed to cancel long timer'; }, 100000);
Atomics.waitAsync(cells, 0, 0, 2).value.then(() => {
  trace.push('waiter'); clearTimeout(longTimer);
});
setTimeout(() => { trace.push('timer'); print(trace.join(',')); }, 20);
"#);
    assert!(matches!(outcome.completion, ObservedCompletion::Normal(_)));
    assert_eq!(
        outcome.output_events,
        [HostOutputEvent::PrintLine("waiter,timer".into())]
    );
}
