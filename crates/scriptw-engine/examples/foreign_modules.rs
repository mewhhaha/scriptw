//! Compile once, persist P/R, then execute using an ordinary embedder Linker.
use scriptw_engine::{
    configure_compilation_jobs, configure_wasm_runtime,
    foreign::{
        ForeignFunctionDeclaration, ForeignModuleDeclaration, ForeignModuleRegistry,
        ForeignResultType, ForeignValueType,
    },
    CompileOptions, EmbeddedRuntime, Engine, HostOutputEvent, IntlCompilationProfile,
    ObservedCompletion, RealmBuilder, WasmStoreState,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    configure_compilation_jobs(1)?;
    let declarations = ForeignModuleRegistry::new(vec![ForeignModuleDeclaration::new(
        "dd:host",
        vec![ForeignFunctionDeclaration::new(
            "add",
            "dd",
            "add",
            vec![ForeignValueType::Number; 2],
            ForeignResultType::Number,
        )?],
    )?])?;
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler.compile_module(
        "import { add } from 'dd:host'; print(add(20,22));",
        CompileOptions {
            foreign_modules: declarations,
            ..CompileOptions::default()
        },
    )?;
    let bytes = compiler
        .emit_wasm(&unit)?
        .encode_portable_bundle(&IntlCompilationProfile::Minimal)?;
    // These bytes may be written to disk and loaded by another process. Loading
    // uses their admitted declarations, without source files or a closure registry.
    drop(unit);
    drop(compiler);
    let artifact = Engine::new(RealmBuilder::new().build()).load_portable_bundle(&bytes)?;
    let mut configuration = wasmtime::Config::new();
    configure_wasm_runtime(&mut configuration);
    let engine = wasmtime::Engine::new(&configuration)?;
    let runtime = EmbeddedRuntime::compile(&engine, &artifact)?;
    let state = runtime.new_state(RealmBuilder::new().build(), ())?;
    let mut store = wasmtime::Store::new(&engine, state);
    store.limiter(|state| state.limits_mut());
    // The embedder owns interruption. A service may instead advance engine
    // epochs and choose a finite deadline, or configure/use Wasmtime fuel.
    store.set_epoch_deadline(u64::MAX / 2);
    let mut linker = wasmtime::Linker::<WasmStoreState>::new(&engine);
    linker.func_wrap("dd", "add", |a: f64, b: f64| a + b)?;
    runtime.register(&mut store, &mut linker)?;
    let instance = runtime.instantiate(&mut store, &mut linker)?;
    let outcome = instance.evaluate(&mut store)?;
    if !matches!(outcome.completion, ObservedCompletion::Normal(_)) {
        return Err("foreign module evaluation threw a JavaScript exception".into());
    }
    for event in outcome.output_events {
        match event {
            HostOutputEvent::PrintLine(line) => println!("{line}"),
            HostOutputEvent::ReportedException(line) => return Err(line.into()),
        }
    }
    Ok(())
}
