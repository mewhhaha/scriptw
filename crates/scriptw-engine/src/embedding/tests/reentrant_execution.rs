use super::*;
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::Duration;

fn normal(completion: JsCompletion) -> JsValue {
    match completion {
        JsCompletion::Normal(value) => value,
        JsCompletion::Throw(value) => panic!("unexpected JS throw: {value:?}"),
    }
}

#[test]
fn foreign_callback_calls_another_retained_store_on_a_fresh_stack() {
    let artifact = artifact(
        "import {add} from 'dd:host'; let count=0; export function run(){return add(++count,40);} print('initialized');",
        registry(false),
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let (inner_thread_tx, inner_thread_rx) = mpsc::channel();
    let mut inner_store = store(&runtime, 0.0);
    let mut inner_linker = WasmtimeLinker::new(&runtime.engine);
    inner_linker
        .func_wrap("dd", "add", move |a: f64, b: f64| {
            inner_thread_tx.send(thread::current().id()).unwrap();
            a + b
        })
        .unwrap();
    inner_linker.func_wrap("dd", "record", |_: f64| {}).unwrap();
    runtime
        .register(&mut inner_store, &mut inner_linker)
        .unwrap();
    let mut inner_instance = runtime
        .instantiate(&mut inner_store, &mut inner_linker)
        .unwrap();
    assert!(matches!(
        inner_instance.initialize(&mut inner_store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    assert_eq!(
        inner_instance.take_output(&mut inner_store).unwrap(),
        [HostOutputEvent::PrintLine("initialized".into())]
    );
    let inner_function = normal(inner_instance.get_export(&mut inner_store, "run").unwrap());
    let inner = Mutex::new((inner_store, inner_instance, inner_function));

    let (outer_thread_tx, outer_thread_rx) = mpsc::channel();
    let mut outer_store = store(&runtime, 0.0);
    let mut outer_linker = WasmtimeLinker::new(&runtime.engine);
    outer_linker
        .func_wrap("dd", "add", move |a: f64, b: f64| {
            let mut inner = inner.lock().unwrap();
            let (store, instance, function) = &mut *inner;
            let result = normal(instance.call(store, function, None, &[]).unwrap())
                .as_number()
                .unwrap();
            outer_thread_tx
                .send((thread::current().id(), a + b, result))
                .unwrap();
            result + 100.0
        })
        .unwrap();
    outer_linker.func_wrap("dd", "record", |_: f64| {}).unwrap();
    runtime
        .register(&mut outer_store, &mut outer_linker)
        .unwrap();
    let mut outer_instance = runtime
        .instantiate(&mut outer_store, &mut outer_linker)
        .unwrap();
    assert!(matches!(
        outer_instance.initialize(&mut outer_store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    let function = normal(outer_instance.get_export(&mut outer_store, "run").unwrap());
    for expected in [41.0, 42.0] {
        assert_eq!(
            normal(
                outer_instance
                    .call(&mut outer_store, &function, None, &[])
                    .unwrap()
            )
            .as_number(),
            Some(expected + 100.0)
        );
        let timeout = Duration::from_secs(10);
        let inner_thread = inner_thread_rx.recv_timeout(timeout).unwrap();
        let (outer_thread, outer_value, inner_value) =
            outer_thread_rx.recv_timeout(timeout).unwrap();
        assert_ne!(outer_thread, inner_thread);
        assert_eq!((outer_value, inner_value), (expected, expected));
    }
    assert_eq!(
        outer_instance.take_output(&mut outer_store).unwrap(),
        [HostOutputEvent::PrintLine("initialized".into())]
    );
}
