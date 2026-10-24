use super::*;

fn normal(completion: JsCompletion) -> JsValue {
    match completion {
        JsCompletion::Normal(value) => value,
        JsCompletion::Throw(value) => panic!("unexpected JS throw: {value:?}"),
    }
}

#[test]
fn owned_utf16_and_uint8array_transfer_are_lossless_strict_and_source_free() {
    let declarations = ForeignModuleRegistry::new(vec![ForeignModuleDeclaration::new(
        "dd:transfer",
        vec![
            ForeignFunctionDeclaration::new(
                "text",
                "transfer",
                "text",
                vec![ForeignValueType::StringUtf16],
                ForeignResultType::StringUtf16,
            )
            .unwrap(),
            ForeignFunctionDeclaration::new(
                "bytes",
                "transfer",
                "bytes",
                vec![ForeignValueType::Bytes],
                ForeignResultType::Bytes,
            )
            .unwrap(),
            ForeignFunctionDeclaration::new(
                "both",
                "transfer",
                "both",
                vec![ForeignValueType::StringUtf16, ForeignValueType::Bytes],
                ForeignResultType::Void,
            )
            .unwrap(),
        ],
    )
    .unwrap()])
    .unwrap();
    let compiled = artifact(
        r#"
import {text,bytes,both} from 'dd:transfer';
const original='\0\ud800\udfff\ud83d\ude00';
if(text(original)!==original || text('')!=='') throw 'UTF16';
for(const value of [undefined,null,42,true,{},new String('boxed')]) {
  try { text(value); throw 'accepted text'; } catch(error) { if(!(error instanceof TypeError)) throw error; }
}
const backing=new Uint8Array([9,0,127,128,255,7]);
const copied=bytes(backing.subarray(1,5));
if(!(copied instanceof Uint8Array) || copied.join(',')!=='0,127,128,255') throw 'bytes view';
copied[0]=33;
if(backing[1]!==0 || bytes(new Uint8Array()).length!==0) throw 'copy isolation';
for(const value of [undefined,{},new Uint16Array(2),new DataView(new ArrayBuffer(2)),new Proxy(backing,{})]) {
  try { bytes(value); throw 'accepted bytes'; } catch(error) { if(!(error instanceof TypeError)) throw error; }
}
let touched=false;
try { both('valid',{valueOf(){touched=true;throw 'coercion'}}); throw 'accepted pair'; }
catch(error) { if(!(error instanceof TypeError)) throw error; }
if(touched) throw 'coerced';
const resized=new ArrayBuffer(4,{maxByteLength:8}), tracking=new Uint8Array(resized);
tracking.set([1,2,3,4]); resized.resize(6);
if(bytes(tracking).join(',')!=='1,2,3,4,0,0') throw 'tracking length';
const fixed=new Uint8Array(resized,2,4); resized.resize(1);
try { bytes(fixed); throw 'accepted OOB'; } catch(error) { if(!(error instanceof TypeError)) throw error; }
const detached=new Uint8Array(new ArrayBuffer(2)); detached.buffer.transfer();
try { bytes(detached); throw 'accepted detached'; } catch(error) { if(!(error instanceof TypeError)) throw error; }
const shared=new Uint8Array(new SharedArrayBuffer(2));shared.set([128,255]);
if(bytes(shared).join(',')!=='128,255') throw 'shared snapshot';
print('transfer-ok');
"#,
        declarations,
    );
    let bundle = compiled
        .encode_portable_bundle(&IntlCompilationProfile::Minimal)
        .unwrap();
    let loaded = Engine::new(RealmBuilder::new().build())
        .load_portable_bundle(&bundle)
        .unwrap();
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &loaded).unwrap();
    let mut store = store(&runtime, 0.0);
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    runtime
        .register_foreign(&mut linker, "transfer", "text", |caller, args| {
            caller.data_mut().user_data_mut::<UserData>().unwrap().calls += 1;
            Ok(args.into_iter().next().unwrap())
        })
        .unwrap();
    runtime
        .register_foreign(&mut linker, "transfer", "bytes", |caller, args| {
            caller.data_mut().user_data_mut::<UserData>().unwrap().calls += 1;
            Ok(args.into_iter().next().unwrap())
        })
        .unwrap();
    runtime
        .register_foreign(&mut linker, "transfer", "both", |_, _| {
            panic!("all arguments must validate before host entry")
        })
        .unwrap();
    runtime.register(&mut store, &mut linker).unwrap();
    let instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert_output(instance.evaluate(&mut store).unwrap(), "transfer-ok");
    assert_eq!(store.data().user_data::<UserData>().unwrap().calls, 6);
}

#[test]
fn retained_exports_preserve_live_bindings_receivers_proxies_throws_and_gc_roots() {
    let artifact = artifact(
        r#"
export let count=0;
export function next(n) { count+=n; return count; }
export { next as "\ud800" };
export const bound=next.bind(null,2);
export const proxy=new Proxy(next,{apply(target,receiver,args){return Reflect.apply(target,receiver,args)+100;}});
export function strictThis(){return this===undefined;}
export function echo(value){return value;}
export default {base:40, fetch(delta){return this.base+delta;}, get bad(){throw 'getter-error';}};
print('initialized');
"#,
        ForeignModuleRegistry::default(),
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let mut store = store(&runtime, 0.0);
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert!(matches!(
        instance.initialize(&mut store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    assert!(instance
        .initialize(&mut store)
        .unwrap_err()
        .to_string()
        .contains("already evaluated"));
    let next = normal(instance.get_export(&mut store, "next").unwrap());
    store.gc(None).unwrap();
    let three = instance
        .value(&mut store, ForeignValue::Number(3.0))
        .unwrap();
    assert_eq!(
        normal(instance.call(&mut store, &next, None, &[three]).unwrap()).as_number(),
        Some(3.0)
    );
    assert_eq!(
        normal(instance.get_export(&mut store, "count").unwrap()).as_number(),
        Some(3.0)
    );
    let named = normal(instance.get_export_utf16(&mut store, &[0xd800]).unwrap());
    let zero = instance
        .value(&mut store, ForeignValue::Number(0.0))
        .unwrap();
    assert_eq!(
        normal(instance.call(&mut store, &named, None, &[zero]).unwrap()).as_number(),
        Some(3.0)
    );
    let bound = normal(instance.get_export(&mut store, "bound").unwrap());
    assert_eq!(
        normal(instance.call(&mut store, &bound, None, &[]).unwrap()).as_number(),
        Some(5.0)
    );
    let proxy = normal(instance.get_export(&mut store, "proxy").unwrap());
    let one = instance
        .value(&mut store, ForeignValue::Number(1.0))
        .unwrap();
    assert_eq!(
        normal(instance.call(&mut store, &proxy, None, &[one]).unwrap()).as_number(),
        Some(106.0)
    );
    let strict = normal(instance.get_export(&mut store, "strictThis").unwrap());
    assert_eq!(
        normal(instance.call(&mut store, &strict, None, &[]).unwrap()).as_boolean(),
        Some(true)
    );
    let object = normal(instance.get_export(&mut store, "default").unwrap());
    let fetch = normal(
        instance
            .get_property(
                &mut store,
                &object,
                &"fetch".encode_utf16().collect::<Vec<_>>(),
            )
            .unwrap(),
    );
    let two = instance
        .value(&mut store, ForeignValue::Number(2.0))
        .unwrap();
    assert_eq!(
        normal(
            instance
                .call(&mut store, &fetch, Some(&object), &[two])
                .unwrap()
        )
        .as_number(),
        Some(42.0)
    );
    let JsCompletion::Throw(error) = instance
        .get_property(
            &mut store,
            &object,
            &"bad".encode_utf16().collect::<Vec<_>>(),
        )
        .unwrap()
    else {
        panic!("getter must throw")
    };
    assert_eq!(
        error.to_utf16(&mut store).unwrap(),
        "getter-error".encode_utf16().collect::<Vec<_>>()
    );
    let echo = normal(instance.get_export(&mut store, "echo").unwrap());
    let units = vec![0, 0xd800, 0xdc00, 0xffff];
    let text = instance
        .value(&mut store, ForeignValue::StringUtf16(units.clone()))
        .unwrap();
    let echoed = normal(instance.call(&mut store, &echo, None, &[text]).unwrap());
    store.gc(None).unwrap();
    assert_eq!(echoed.to_utf16(&mut store).unwrap(), units);
    let bytes = instance
        .value(&mut store, ForeignValue::Bytes(vec![0, 128, 255]))
        .unwrap();
    let echoed = normal(instance.call(&mut store, &echo, None, &[bytes]).unwrap());
    assert_eq!(
        instance.to_bytes(&mut store, &echoed).unwrap(),
        [0, 128, 255]
    );
    assert!(instance.to_bytes(&mut store, &next).is_err());
    let mut wrong_store = super::store(&runtime, 0.0);
    assert!(instance
        .call(&mut wrong_store, &next, None, &[])
        .unwrap_err()
        .to_string()
        .contains("different Store"));
    assert!(echoed
        .observe(&mut wrong_store)
        .unwrap_err()
        .to_string()
        .contains("different Store"));
    assert_eq!(
        instance.take_output(&mut store).unwrap(),
        [HostOutputEvent::PrintLine("initialized".into())]
    );
}
