//! The retained exported call enters through a bridge typed once at
//! first use and an entry Realm/Environment/program retained once at
//! initialize. These tests pin that the retained identities stay exactly the
//! live ones and that sharing one empty argument array is unobservable.
use super::*;

fn normal(completion: JsCompletion) -> JsValue {
    match completion {
        JsCompletion::Normal(value) => value,
        JsCompletion::Throw(value) => panic!("unexpected JavaScript throw: {value:?}"),
    }
}

fn export(
    instance: &EmbeddedInstance,
    store: &mut WasmtimeStore<WasmStoreState>,
    name: &str,
) -> JsValue {
    normal(instance.get_export(store, name).unwrap())
}

fn boolean(completion: JsCompletion) -> bool {
    normal(completion).as_boolean().expect("Boolean result")
}

#[test]
fn retained_entry_context_and_typed_call_stay_exact_across_calls_and_gc() {
    let artifact = artifact(
        r#"
let kept = [];
let firstGlobal;
export function empty() {}
export function count() { return arguments.length; }
export function keep() {
  arguments[0] = kept.length;
  kept.push(arguments);
  const first = kept[0];
  return arguments.length === 0 && first[0] === 0 &&
    (kept.length === 1 || first !== arguments);
}
export function receiver() { return this === undefined; }
export function identity() { return this; }
export function global() {
  if (firstGlobal === undefined) firstGlobal = globalThis;
  return firstGlobal === globalThis;
}
export function boom() { throw 7; }
export function hello() { print('hello'); }
"#,
        ForeignModuleRegistry::default(),
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let mut store = store(&runtime, 0.0);
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert!(
        !instance.bridges.has_typed_call(),
        "instantiation leaves the call bridge untyped"
    );
    assert!(
        instance.entry.is_none(),
        "main has not created the entry Realm"
    );
    assert!(matches!(
        instance.initialize(&mut store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    assert_eq!(
        instance.entry_context_is_current(&mut store).unwrap(),
        Some(true),
        "initialize retains the live entry context"
    );

    let empty = export(&instance, &mut store, "empty");
    let count = export(&instance, &mut store, "count");
    let keep = export(&instance, &mut store, "keep");
    let receiver = export(&instance, &mut store, "receiver");
    let identity = export(&instance, &mut store, "identity");
    let global = export(&instance, &mut store, "global");
    let boom = export(&instance, &mut store, "boom");
    let hello = export(&instance, &mut store, "hello");
    let one = instance
        .value(&mut store, ForeignValue::Number(1.0))
        .unwrap();
    let two = instance
        .value(&mut store, ForeignValue::Number(2.0))
        .unwrap();
    assert!(
        !instance.bridges.has_typed_call(),
        "initialization and export reads leave the call bridge untyped"
    );

    for round in 0..2 {
        assert!(normal(instance.call(&mut store, &empty, None, &[]).unwrap()).is_undefined());
        assert!(
            instance.bridges.has_typed_call(),
            "the first call retains the typed bridge"
        );
        // Zero-argument calls share one empty argument array; arity and
        // each call's own arguments object stay exact around non-empty calls.
        for (arguments, expected) in [
            (&[][..], 0.0),
            (&[one.clone(), two.clone()][..], 2.0),
            (&[][..], 0.0),
        ] {
            assert_eq!(
                normal(instance.call(&mut store, &count, None, arguments).unwrap()).as_number(),
                Some(expected)
            );
        }
        for _ in 0..3 {
            assert!(boolean(
                instance.call(&mut store, &keep, None, &[]).unwrap()
            ));
        }
        // An absent receiver is undefined; an explicit one is passed as is.
        assert!(boolean(
            instance.call(&mut store, &receiver, None, &[]).unwrap()
        ));
        assert_eq!(
            normal(
                instance
                    .call(&mut store, &identity, Some(&two), &[])
                    .unwrap()
            )
            .as_number(),
            Some(2.0)
        );
        assert!(boolean(
            instance.call(&mut store, &global, None, &[]).unwrap()
        ));
        let JsCompletion::Throw(thrown) = instance.call(&mut store, &boom, None, &[]).unwrap()
        else {
            panic!("boom throws");
        };
        assert_eq!(thrown.as_number(), Some(7.0));
        assert!(normal(instance.call(&mut store, &hello, None, &[]).unwrap()).is_undefined());
        assert_eq!(
            instance.take_output(&mut store).unwrap(),
            [HostOutputEvent::PrintLine("hello".into())]
        );
        // A moving collection relocates the retained roots, never replaces
        // what they name.
        store.gc(None).unwrap();
        assert_eq!(
            instance.entry_context_is_current(&mut store).unwrap(),
            Some(true),
            "round {round}: the retained entry context went stale"
        );
    }
}
