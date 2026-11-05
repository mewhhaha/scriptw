//! Add as `mod execution_fiber_controls;` in embedding/tests.rs for fiber qualification.
use super::*;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};

fn normal(completion: JsCompletion) -> JsValue {
    match completion {
        JsCompletion::Normal(value) => value,
        JsCompletion::Throw(value) => panic!("unexpected JavaScript throw: {value:?}"),
    }
}

#[test]
fn host_callback_panic_restores_fiber_and_same_instance_calls_again() {
    let artifact = artifact(
        "import {add} from 'dd:host'; export function run(){return add(20,9);}",
        registry(false),
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let mut store = store(&runtime, 0.0);
    let panic_once = Arc::new(AtomicBool::new(true));
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    let panic_callback = Arc::clone(&panic_once);
    linker
        .func_wrap("dd", "add", move |a: f64, b: f64| -> f64 {
            if panic_callback.swap(false, Ordering::SeqCst) {
                panic!("fiber host callback panic control");
            }
            a + b
        })
        .unwrap();
    linker.func_wrap("dd", "record", |_: f64| {}).unwrap();
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert!(matches!(
        instance.initialize(&mut store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    let callable = normal(instance.get_export(&mut store, "run").unwrap());

    let first = panic::catch_unwind(AssertUnwindSafe(|| {
        instance.call(&mut store, &callable, None, &[])
    }));
    assert!(
        first.is_err(),
        "host callback panic propagates to call boundary"
    );
    let recovered = normal(instance.call(&mut store, &callable, None, &[]).unwrap());
    assert_eq!(recovered.as_number(), Some(29.0));
}
