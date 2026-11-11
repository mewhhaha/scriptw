use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Clock(Arc<AtomicU64>);
impl HostClock for Clock {
    fn utc_epoch_milliseconds(&self) -> UtcEpochMilliseconds {
        UtcEpochMilliseconds::new(0).unwrap()
    }
    fn monotonic_instant(&self) -> MonotonicClockInstant {
        MonotonicClockInstant::new(self.0.load(Ordering::Relaxed))
    }
}

#[test]
fn timer_pump_polls_readiness_and_keeps_callbacks_alive_across_gc() {
    let artifact = artifact(
        r#"
let value = 0;
export function start() {
  setTimeout(() => { value = 7; queueMicrotask(() => { value += 1; }); }, 10);
}
export function state() { return value; }
"#,
        ForeignModuleRegistry::default(),
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let clock = Arc::new(AtomicU64::new(0));
    let realm = RealmBuilder::new()
        .with_host_clock(Box::new(Clock(Arc::clone(&clock))))
        .build();
    let mut store = WasmtimeStore::new(&runtime.engine, runtime.new_state(realm, ()).unwrap());
    store.limiter(|state| state.limits_mut());
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert!(matches!(
        instance.initialize(&mut store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    let JsCompletion::Normal(start) = instance.get_export(&mut store, "start").unwrap() else {
        panic!("start export")
    };
    let JsCompletion::Normal(state) = instance.get_export(&mut store, "state").unwrap() else {
        panic!("state export")
    };
    assert!(matches!(
        instance.call(&mut store, &start, None, &[]).unwrap(),
        JsCompletion::Normal(_)
    ));
    store.gc(None).unwrap();
    assert!(matches!(
        instance.pump(&mut store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    let JsCompletion::Normal(value) = instance.call(&mut store, &state, None, &[]).unwrap() else {
        panic!("state result")
    };
    assert_eq!(value.as_number(), Some(0.0));
    clock.store(10_000_000, Ordering::Relaxed);
    assert!(matches!(
        instance.pump(&mut store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    let JsCompletion::Normal(value) = instance.call(&mut store, &state, None, &[]).unwrap() else {
        panic!("state result")
    };
    assert_eq!(value.as_number(), Some(8.0));
}
