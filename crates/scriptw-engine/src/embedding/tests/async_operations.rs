use super::*;
use std::collections::VecDeque;

type QueuedWork = Arc<Mutex<VecDeque<(String, Vec<ForeignValue>, ForeignCompleter)>>>;

fn declarations() -> ForeignModuleRegistry {
    let functions = [
        (
            "number",
            ForeignValueType::Number,
            ForeignResultType::Number,
        ),
        (
            "text",
            ForeignValueType::StringUtf16,
            ForeignResultType::StringUtf16,
        ),
        ("bytes", ForeignValueType::Bytes, ForeignResultType::Bytes),
    ]
    .into_iter()
    .map(|(name, parameter, result)| {
        ForeignFunctionDeclaration::new_async(name, "io", name, vec![parameter], result).unwrap()
    })
    .collect();
    ForeignModuleRegistry::new(vec![
        ForeignModuleDeclaration::new("io:host", functions).unwrap()
    ])
    .unwrap()
}

fn async_store(runtime: &EmbeddedRuntime) -> WasmtimeStore<WasmStoreState> {
    let mut store = WasmtimeStore::new(
        &runtime.engine,
        runtime.new_state(RealmBuilder::new().build(), ()).unwrap(),
    );
    store.limiter(|state| state.limits_mut());
    store.set_epoch_deadline(u64::MAX / 2);
    store
}

fn async_linker(runtime: &EmbeddedRuntime, work: &QueuedWork) -> WasmtimeLinker<WasmStoreState> {
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    for name in ["number", "text", "bytes"] {
        let queue = Arc::clone(work);
        runtime
            .register_async_foreign(&mut linker, "io", name, move |_, values, completion| {
                queue
                    .lock()
                    .unwrap()
                    .push_back((name.to_owned(), values, completion));
                Ok(())
            })
            .unwrap();
    }
    linker
}

fn normal(completion: JsCompletion) -> JsValue {
    match completion {
        JsCompletion::Normal(value) => value,
        JsCompletion::Throw(value) => panic!("unexpected JavaScript throw: {value:?}"),
    }
}

fn settled(evaluation: EmbeddedEvaluation) {
    match evaluation {
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_)) => {}
        other => panic!("expected completed module evaluation: {other:?}"),
    }
}

fn invoke(
    instance: &EmbeddedInstance,
    store: &mut WasmtimeStore<WasmStoreState>,
    name: &str,
    arguments: &[JsValue],
) -> JsValue {
    let function = normal(instance.get_export(store, name).unwrap());
    normal(instance.call(store, &function, None, arguments).unwrap())
}

fn fulfilled(
    instance: &EmbeddedInstance,
    store: &mut WasmtimeStore<WasmStoreState>,
    promise: &JsValue,
) -> JsValue {
    match instance.promise_state(store, promise).unwrap() {
        JsPromiseState::Fulfilled(value) => value,
        other => panic!("expected fulfilled retained Promise: {other:?}"),
    }
}

#[test]
fn retained_async_calls_settle_on_store_pump_with_gc_microtasks_rejection_and_cancellation() {
    let artifact = artifact(
        r#"
import {number,text,bytes} from 'io:host';
const OriginalPromise=Promise;
globalThis.Promise=function(){throw 'must use the defining Realm intrinsic'};
export function start(value) {
  print('start');
  return number(value).then(value=>{
    print('first');
    return OriginalPromise.resolve().then(()=>{print('second');return value+1;});
  });
}
export function exchangeText(value) { return text(value); }
export function exchangeBytes(value) { return bytes(value); }
export function rejected() { return number(0).catch(reason=>reason); }
export function cancelled() { return number(0).then(()=>false,reason=>reason instanceof Error); }
export function invalid() {
  try { number({valueOf(){throw 'coerced'}}, print('extra')); return false; }
  catch(reason) { return reason instanceof TypeError; }
}
print('sync');
"#,
        declarations(),
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let work = Arc::new(Mutex::new(VecDeque::new()));
    let mut store = async_store(&runtime);
    let mut linker = async_linker(&runtime, &work);
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    settled(instance.initialize(&mut store).unwrap());
    assert_eq!(
        instance.take_output(&mut store).unwrap(),
        [HostOutputEvent::PrintLine("sync".into())]
    );

    let argument = instance
        .value(&mut store, ForeignValue::Number(41.0))
        .unwrap();
    let promise = invoke(&instance, &mut store, "start", &[argument]);
    assert!(matches!(
        instance.promise_state(&mut store, &promise).unwrap(),
        JsPromiseState::Pending
    ));
    assert_eq!(instance.pending_host_operations(&store).unwrap(), 1);
    assert_eq!(
        instance.take_output(&mut store).unwrap(),
        [HostOutputEvent::PrintLine("start".into())]
    );
    let (name, arguments, completion) = work.lock().unwrap().pop_front().unwrap();
    assert_eq!(name, "number");
    let [ForeignValue::Number(argument)] = arguments.as_slice() else {
        panic!("strict numeric argument")
    };
    assert_eq!(*argument, 41.0);
    std::thread::spawn(move || completion.resolve(ForeignValue::Number(42.0)))
        .join()
        .unwrap()
        .unwrap();
    store.gc(None).unwrap();
    assert!(
        matches!(
            instance.promise_state(&mut store, &promise).unwrap(),
            JsPromiseState::Pending
        ),
        "worker completion must not enter JS before the Store pump"
    );
    settled(instance.pump(&mut store).unwrap());
    assert_eq!(
        fulfilled(&instance, &mut store, &promise).as_number(),
        Some(43.0)
    );
    assert_eq!(instance.pending_host_operations(&store).unwrap(), 0);
    assert_eq!(
        instance.take_output(&mut store).unwrap(),
        [
            HostOutputEvent::PrintLine("first".into()),
            HostOutputEvent::PrintLine("second".into())
        ]
    );

    let units = vec![0, 0xd800, 0x61, 0xdc00, 0xffff];
    let argument = instance
        .value(&mut store, ForeignValue::StringUtf16(units.clone()))
        .unwrap();
    let promise = invoke(&instance, &mut store, "exchangeText", &[argument]);
    let (name, arguments, completion) = work.lock().unwrap().pop_front().unwrap();
    assert_eq!(name, "text");
    let [ForeignValue::StringUtf16(received)] = arguments.as_slice() else {
        panic!("strict string argument")
    };
    assert_eq!(received, &units);
    completion
        .resolve(ForeignValue::StringUtf16(units.clone()))
        .unwrap();
    store.gc(None).unwrap();
    settled(instance.pump(&mut store).unwrap());
    assert_eq!(
        fulfilled(&instance, &mut store, &promise)
            .to_utf16(&mut store)
            .unwrap(),
        units
    );

    let bytes = vec![0, 1, 127, 255];
    let argument = instance
        .value(&mut store, ForeignValue::Bytes(bytes.clone()))
        .unwrap();
    let promise = invoke(&instance, &mut store, "exchangeBytes", &[argument]);
    let (name, arguments, completion) = work.lock().unwrap().pop_front().unwrap();
    assert_eq!(name, "bytes");
    let [ForeignValue::Bytes(received)] = arguments.as_slice() else {
        panic!("strict bytes argument")
    };
    assert_eq!(received, &bytes);
    completion
        .resolve(ForeignValue::Bytes(bytes.clone()))
        .unwrap();
    settled(instance.pump(&mut store).unwrap());
    let value = fulfilled(&instance, &mut store, &promise);
    assert_eq!(instance.to_bytes(&mut store, &value).unwrap(), bytes);

    let promise = invoke(&instance, &mut store, "rejected", &[]);
    let (_, _, completion) = work.lock().unwrap().pop_front().unwrap();
    completion
        .reject(ForeignValue::StringUtf16(units.clone()))
        .unwrap();
    settled(instance.pump(&mut store).unwrap());
    assert_eq!(
        fulfilled(&instance, &mut store, &promise)
            .to_utf16(&mut store)
            .unwrap(),
        units
    );

    let promise = invoke(&instance, &mut store, "cancelled", &[]);
    let (_, _, completion) = work.lock().unwrap().pop_front().unwrap();
    drop(completion);
    settled(instance.pump(&mut store).unwrap());
    assert_eq!(
        fulfilled(&instance, &mut store, &promise).as_boolean(),
        Some(true)
    );
    assert_eq!(instance.pending_host_operations(&store).unwrap(), 0);

    assert_eq!(
        invoke(&instance, &mut store, "invalid", &[]).as_boolean(),
        Some(true)
    );
    assert!(
        work.lock().unwrap().is_empty(),
        "invalid arguments must not enter the host"
    );
    assert_eq!(
        instance.take_output(&mut store).unwrap(),
        [HostOutputEvent::PrintLine("extra".into())]
    );

    let argument = instance
        .value(&mut store, ForeignValue::Number(1.0))
        .unwrap();
    let _promise = invoke(&instance, &mut store, "start", &[argument]);
    let (_, _, completion) = work.lock().unwrap().pop_front().unwrap();
    completion.resolve(ForeignValue::Number(2.0)).unwrap();
    store.set_epoch_deadline(0);
    assert!(
        instance.pump(&mut store).is_err(),
        "pump must preserve the host's interruption policy"
    );
    store.set_epoch_deadline(u64::MAX / 2);
    assert!(
        instance
            .pump(&mut store)
            .unwrap_err()
            .to_string()
            .contains("failed"),
        "a failed detached batch must make subsequent execution fail clearly"
    );
}

#[test]
fn top_level_await_keeps_each_stores_queued_settlement_and_initializes_exactly_once() {
    let artifact = artifact(
        "import {number} from 'io:host'; print('before'); export const answer=await number(41); print(answer);",
        declarations(),
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let work = Arc::new(Mutex::new(VecDeque::new()));
    let mut first = async_store(&runtime);
    let mut first_linker = async_linker(&runtime, &work);
    runtime.register(&mut first, &mut first_linker).unwrap();
    let mut first_instance = runtime.instantiate(&mut first, &mut first_linker).unwrap();
    assert!(matches!(
        first_instance.initialize(&mut first).unwrap(),
        EmbeddedEvaluation::Pending
    ));
    assert!(
        first_instance.initialize(&mut first).is_err(),
        "pending evaluation must not replay main"
    );
    assert!(first_instance.get_export(&mut first, "answer").is_err());

    let mut second = async_store(&runtime);
    let mut second_linker = async_linker(&runtime, &work);
    runtime.register(&mut second, &mut second_linker).unwrap();
    let mut second_instance = runtime
        .instantiate(&mut second, &mut second_linker)
        .unwrap();
    assert!(matches!(
        second_instance.initialize(&mut second).unwrap(),
        EmbeddedEvaluation::Pending
    ));
    let (_, _, completion) = work.lock().unwrap().pop_front().unwrap();
    completion.resolve(ForeignValue::Number(42.0)).unwrap();
    assert!(
        first_instance.pump(&mut second).is_err(),
        "wrong Store must fail before consuming either mailbox"
    );
    assert!(matches!(
        second_instance.pump(&mut second).unwrap(),
        EmbeddedEvaluation::Pending
    ));
    assert_eq!(first_instance.pending_host_operations(&first).unwrap(), 1);
    assert_eq!(second_instance.pending_host_operations(&second).unwrap(), 1);
    first.gc(None).unwrap();
    settled(first_instance.pump(&mut first).unwrap());
    assert_eq!(
        normal(first_instance.get_export(&mut first, "answer").unwrap()).as_number(),
        Some(42.0)
    );
    assert_eq!(
        first_instance.take_output(&mut first).unwrap(),
        [
            HostOutputEvent::PrintLine("before".into()),
            HostOutputEvent::PrintLine("42".into())
        ]
    );

    let (_, _, completion) = work.lock().unwrap().pop_front().unwrap();
    completion
        .reject(ForeignValue::StringUtf16(
            "io rejected".encode_utf16().collect(),
        ))
        .unwrap();
    match second_instance.pump(&mut second).unwrap() {
        EmbeddedEvaluation::Settled(JsCompletion::Throw(reason)) => assert_eq!(
            reason.to_utf16(&mut second).unwrap(),
            "io rejected".encode_utf16().collect::<Vec<_>>()
        ),
        other => panic!("rejected top-level await must retain its exact thrown value: {other:?}"),
    }
    assert_eq!(second_instance.pending_host_operations(&second).unwrap(), 0);
    assert_eq!(
        second_instance.take_output(&mut second).unwrap(),
        [HostOutputEvent::PrintLine("before".into())]
    );
}

#[test]
fn synchronous_async_host_completion_stays_queued_and_declared_startup_errors_reject() {
    let registry = ForeignModuleRegistry::new(vec![ForeignModuleDeclaration::new(
        "io:host",
        vec![ForeignFunctionDeclaration::new_async(
            "number",
            "io",
            "number",
            vec![ForeignValueType::Number],
            ForeignResultType::Number,
        )
        .unwrap()],
    )
    .unwrap()])
    .unwrap()
    .with_error_tag(ForeignErrorTag::code_v1());
    let artifact = artifact(
        "import {number} from 'io:host'; export const answer=await number(41).catch(reason=>reason.code);",
        registry,
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let mut store = async_store(&runtime);
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    runtime
        .register_async_foreign(&mut linker, "io", "number", |_, _, completion| {
            completion
                .resolve(ForeignValue::Number(42.0))
                .map_err(wasmtime::Error::new)
        })
        .unwrap();
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert!(matches!(
        instance.initialize(&mut store).unwrap(),
        EmbeddedEvaluation::Pending
    ));
    settled(instance.pump(&mut store).unwrap());
    assert_eq!(
        normal(instance.get_export(&mut store, "answer").unwrap()).as_number(),
        Some(42.0)
    );

    let mut failed_store = async_store(&runtime);
    let mut failed_linker = WasmtimeLinker::new(&runtime.engine);
    let signature = runtime
        .modules
        .program
        .imports()
        .find_map(|import| {
            (import.module() == "io" && import.name() == "number")
                .then(|| import.ty())
                .and_then(|ty| match ty {
                    WasmtimeExternType::Func(ty) => Some(ty),
                    _ => None,
                })
        })
        .unwrap();
    failed_linker
        .func_new("io", "number", signature, |caller, _, _| {
            throw_foreign_error(caller, -17)
        })
        .unwrap();
    runtime
        .register(&mut failed_store, &mut failed_linker)
        .unwrap();
    let mut failed_instance = runtime
        .instantiate(&mut failed_store, &mut failed_linker)
        .unwrap();
    settled(failed_instance.initialize(&mut failed_store).unwrap());
    assert_eq!(
        normal(
            failed_instance
                .get_export(&mut failed_store, "answer")
                .unwrap()
        )
        .as_number(),
        Some(-17.0)
    );
    assert_eq!(
        failed_instance
            .pending_host_operations(&failed_store)
            .unwrap(),
        0
    );
}
