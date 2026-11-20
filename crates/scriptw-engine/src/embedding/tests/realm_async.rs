//! Async transfer results use the foreign operation's actual defining Realm.
use super::*;
use std::collections::VecDeque;

fn normal(completion: JsCompletion) -> JsValue {
    match completion {
        JsCompletion::Normal(value) => value,
        JsCompletion::Throw(value) => panic!("unexpected JavaScript throw: {value:?}"),
    }
}

#[test]
fn async_byte_fulfillment_rejection_and_cancellation_use_the_wrapper_realm() {
    configure_compilation_jobs(1).unwrap();
    let source = r#"
const realm = new ShadowRealm();
const start = await realm.importValue('inner', 'start');
const read = await realm.importValue('inner', 'read');
export function run() { start(); }
export function state() { return read(); }
"#;
    let inner = r#"
import {bytes} from 'io:host';
let state = 0;
export function start() {
  void bytes(new Uint8Array([1])).then(value => {
    state |= value instanceof Uint8Array && value[0] === 11 ? 1 : 64;
  });
  void bytes(new Uint8Array([2])).then(undefined, reason => {
    state |= reason instanceof Uint8Array && reason[0] === 22 ? 2 : 128;
  });
  void bytes(new Uint8Array([3])).then(undefined, reason => {
    state |= reason instanceof Error && reason.message === 'Host operation cancelled' ? 4 : 256;
  });
}
export function read() { return state; }
"#;
    let graph = EmbeddedModuleGraph::try_new(
        EmbeddedModuleEntryInput {
            goal: EmbeddedModuleGoal::Module,
            identity: "entry.js".into(),
            source: source.into(),
            meta_url: "scriptw://async-realm/entry.js".into(),
        },
        vec![EmbeddedModuleSourceInput {
            identity: "inner.js".into(),
            source: inner.into(),
            meta_url: "scriptw://async-realm/inner.js".into(),
        }],
        vec![EmbeddedModuleResolutionInput {
            referrer: EmbeddedModuleReferrer::Realm,
            specifier: "inner".into(),
            attributes: Vec::new(),
            target: "inner.js".into(),
        }],
    )
    .unwrap();
    let foreign_modules = ForeignModuleRegistry::new(vec![ForeignModuleDeclaration::new(
        "io:host",
        vec![ForeignFunctionDeclaration::new_async(
            "bytes",
            "io",
            "bytes",
            vec![ForeignValueType::Bytes],
            ForeignResultType::Bytes,
        )
        .unwrap()],
    )
    .unwrap()])
    .unwrap();
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_module(
            source,
            CompileOptions {
                module_loading_policy: ModuleLoadingPolicy::Embedded(graph),
                foreign_modules,
                ..CompileOptions::default()
            },
        )
        .expect("explicit Realm catalog and foreign declarations compile together");
    let artifact = compiler.emit_wasm(&unit).unwrap();
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let mut store = store(&runtime, 0.0);
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    let work = Arc::new(Mutex::new(VecDeque::new()));
    let queue = Arc::clone(&work);
    runtime
        .register_async_foreign(&mut linker, "io", "bytes", move |_, values, completion| {
            queue.lock().unwrap().push_back((values, completion));
            Ok(())
        })
        .unwrap();
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert!(matches!(
        instance.initialize(&mut store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    let run = normal(instance.get_export(&mut store, "run").unwrap());
    assert!(normal(instance.call(&mut store, &run, None, &[]).unwrap()).is_undefined());
    assert_eq!(instance.pending_host_operations(&store).unwrap(), 3);
    store.gc(None).unwrap();
    let ready: Vec<_> = work.lock().unwrap().drain(..).collect();
    assert_eq!(ready.len(), 3);
    for (values, completion) in ready {
        let [ForeignValue::Bytes(bytes)] = values.as_slice() else {
            panic!("wrong byte input snapshot: {values:?}");
        };
        match bytes.as_slice() {
            [1] => completion.resolve(ForeignValue::Bytes(vec![11])).unwrap(),
            [2] => completion.reject(ForeignValue::Bytes(vec![22])).unwrap(),
            [3] => drop(completion),
            _ => panic!("unexpected operation bytes: {bytes:?}"),
        }
    }
    assert!(matches!(
        instance.pump(&mut store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    let state = normal(instance.get_export(&mut store, "state").unwrap());
    let state = normal(instance.call(&mut store, &state, None, &[]).unwrap());
    assert_eq!(state.as_number(), Some(7.0));
    assert_eq!(instance.pending_host_operations(&store).unwrap(), 0);
}
