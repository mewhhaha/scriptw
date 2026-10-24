//! Source-free Portable embedding with retained methods and host-owned async I/O.
use scriptw_engine::{
    configure_compilation_jobs, configure_wasm_runtime,
    foreign::{
        ForeignFunctionDeclaration, ForeignModuleDeclaration, ForeignModuleRegistry,
        ForeignResultType, ForeignValueType,
    },
    CompileOptions, EmbeddedEvaluation, EmbeddedRuntime, Engine, ForeignCompleter, ForeignValue,
    IntlCompilationProfile, JsCompletion, JsPromiseState, JsValue, RealmBuilder, WasmStoreState,
};
use std::sync::mpsc;
use std::time::Duration;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Work = (Vec<ForeignValue>, ForeignCompleter);

// NUL, two isolated surrogates and an astral pair remain exact code units.
const TEXT: &[u16] = &[0, 65, 0xd800, 66, 0xdc00, 0xd83d, 0xde80];
const BYTES: &[u8] = &[0, 127, 128, 255];

fn normal(completion: JsCompletion) -> Result<JsValue> {
    match completion {
        JsCompletion::Normal(value) => Ok(value),
        JsCompletion::Throw(_) => Err("JavaScript operation threw".into()),
    }
}

fn complete_work((arguments, completer): Work) -> Result<()> {
    let [ForeignValue::StringUtf16(text), ForeignValue::Bytes(bytes)] = arguments.as_slice() else {
        return Err("host work has the wrong declared arguments".into());
    };
    if text.as_slice() != TEXT || bytes.as_slice() != BYTES {
        return Err("foreign transfer changed a code unit or byte".into());
    }
    // A worker or async executor may own these copied arguments and completer.
    // Sending settlement never enters JavaScript or borrows its Store.
    completer.resolve(ForeignValue::Bytes(bytes.clone()))?;
    Ok(())
}

fn main() -> Result<()> {
    configure_compilation_jobs(1)?;
    let declarations = ForeignModuleRegistry::new(vec![ForeignModuleDeclaration::new(
        "example:host",
        vec![
            ForeignFunctionDeclaration::new(
                "echo",
                "example",
                "echo",
                vec![ForeignValueType::StringUtf16],
                ForeignResultType::StringUtf16,
            )?,
            ForeignFunctionDeclaration::new_async(
                "request",
                "example",
                "request",
                vec![ForeignValueType::StringUtf16, ForeignValueType::Bytes],
                ForeignResultType::Bytes,
            )?,
        ],
    )?])?;
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler.compile_module(
        r#"
import { echo, request } from 'example:host';
export default {
    marker: 42,
    echo(text) { return echo(text); },
    async request(text, input) {
        const bytes = await request(text, input);
        bytes[0] = this.marker;
        return bytes;
    }
};
"#,
        CompileOptions {
            foreign_modules: declarations,
            ..CompileOptions::default()
        },
    )?;
    let bundle = compiler
        .emit_wasm(&unit)?
        .encode_portable_bundle(&IntlCompilationProfile::Minimal)?;
    drop(unit);
    drop(compiler);
    // A different process can load these bytes without source files, compiler
    // callbacks or a private JavaScript evaluator.
    let artifact = Engine::new(RealmBuilder::new().build()).load_portable_bundle(&bundle)?;
    let mut configuration = wasmtime::Config::new();
    configure_wasm_runtime(&mut configuration);
    let engine = wasmtime::Engine::new(&configuration)?;
    let runtime = EmbeddedRuntime::compile(&engine, &artifact)?;
    let state = runtime.new_state(RealmBuilder::new().build(), ())?;
    let mut store = wasmtime::Store::new(&engine, state);
    store.limiter(|state| state.limits_mut());
    // The host owns deadlines and any epoch ticker; this finite example opts
    // out of epoch interruption. Compilation and host work remain serial.
    store.set_epoch_deadline(u64::MAX / 2);
    let mut linker = wasmtime::Linker::<WasmStoreState>::new(&engine);
    runtime.register_foreign(&mut linker, "example", "echo", |_, mut arguments| {
        arguments
            .pop()
            .ok_or_else(|| wasmtime::Error::msg("missing UTF-16 argument"))
    })?;
    let (work_sender, work_receiver) = mpsc::channel::<Work>();
    runtime.register_async_foreign(
        &mut linker,
        "example",
        "request",
        move |_, arguments, completer| {
            work_sender
                .send((arguments, completer))
                .map_err(|_| wasmtime::Error::msg("host work receiver was dropped"))
        },
    )?;
    runtime.register(&mut store, &mut linker)?;
    let mut instance = runtime.instantiate(&mut store, &mut linker)?;
    match instance.initialize(&mut store)? {
        EmbeddedEvaluation::Settled(completion) => {
            normal(completion)?;
        }
        EmbeddedEvaluation::Pending => return Err("unexpected pending module entry".into()),
    }
    let object = normal(instance.get_export(&mut store, "default")?)?;
    let echo = normal(instance.get_property(
        &mut store,
        &object,
        &"echo".encode_utf16().collect::<Vec<_>>(),
    )?)?;
    let request = normal(instance.get_property(
        &mut store,
        &object,
        &"request".encode_utf16().collect::<Vec<_>>(),
    )?)?;
    let text = instance.value(&mut store, ForeignValue::StringUtf16(TEXT.to_vec()))?;
    let bytes = instance.value(&mut store, ForeignValue::Bytes(BYTES.to_vec()))?;
    let echoed = normal(instance.call(&mut store, &echo, Some(&object), &[text.clone()])?)?;
    if echoed.to_utf16(&mut store)? != TEXT {
        return Err("UTF-16 round trip changed code units".into());
    }
    // The retained receiver preserves ordinary JavaScript method `this`.
    let promise = normal(instance.call(&mut store, &request, Some(&object), &[text, bytes])?)?;
    let fulfilled = loop {
        match instance.promise_state(&mut store, &promise)? {
            JsPromiseState::Fulfilled(value) => break value,
            JsPromiseState::Rejected(_) => return Err("request Promise rejected".into()),
            JsPromiseState::Pending => {}
        }
        // Integrate this with any host event loop. This example blocks for
        // queued I/O work instead of spinning or imposing an async executor.
        complete_work(work_receiver.recv_timeout(Duration::from_secs(5))?)?;
        for work in work_receiver.try_iter() {
            complete_work(work)?;
        }
        if let EmbeddedEvaluation::Settled(completion) = instance.pump(&mut store)? {
            normal(completion)?;
        }
    };
    let result = instance.to_bytes(&mut store, &fulfilled)?;
    if result != [42, 127, 128, 255] {
        return Err("retained receiver or byte transfer failed".into());
    }
    println!("UTF-16 preserved; retained async request returned {result:?}");
    Ok(())
}
