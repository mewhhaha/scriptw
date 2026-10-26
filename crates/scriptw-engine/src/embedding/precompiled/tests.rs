use super::*;
use std::sync::OnceLock;

fn compiler_disabled() -> wasmtime::Engine {
    let mut config = wasmtime::Config::new();
    configure_precompiled_wasm_runtime(&mut config);
    wasmtime::Engine::new(&config).unwrap()
}

fn script(source: &str) -> Artifact {
    configure_compilation_jobs(1).unwrap();
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_script(source, CompileOptions::default())
        .unwrap();
    compiler.emit_wasm(&unit).unwrap()
}

fn state(runtime: &EmbeddedRuntime) -> WasmtimeStore<WasmStoreState> {
    let mut store = WasmtimeStore::new(
        &runtime.engine,
        runtime.new_state(RealmBuilder::new().build(), ()).unwrap(),
    );
    store.limiter(|state| state.limits_mut());
    store.set_epoch_deadline(u64::MAX / 2);
    store
}

fn normal(completion: JsCompletion) -> JsValue {
    match completion {
        JsCompletion::Normal(value) => value,
        JsCompletion::Throw(value) => panic!("unexpected throw: {value:?}"),
    }
}

#[test]
fn precompiled_standalone_executes_on_an_engine_that_cannot_compile_wasm() {
    let artifact = script("42;");
    assert!(artifact.runtime.is_none());
    let image = EmbeddedRuntime::precompile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let engine = compiler_disabled();
    assert_eq!(engine.get_cranelift_opt_level(), None);
    assert!(WasmtimeModule::new(&engine, b"\0asm\x01\0\0\0")
        .unwrap_err()
        .to_string()
        .contains("compiler"));
    let runtime = EmbeddedRuntime::from_precompiled(&engine, &image).unwrap();
    let mut store = state(&runtime);
    assert!(store.data().native_source_compiler.is_none());
    let mut linker = WasmtimeLinker::new(&engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert_eq!(
        instance.evaluate(&mut store).unwrap().completion,
        ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(42.0)))
    );
}

fn linked_image() -> &'static PrecompiledEmbeddedProgram {
    static IMAGE: OnceLock<PrecompiledEmbeddedProgram> = OnceLock::new();
    IMAGE.get_or_init(|| {
        let compiler = Engine::new(RealmBuilder::new().build());
        let declarations =
            ForeignModuleRegistry::new(vec![scriptw_ir::foreign::ForeignModuleDeclaration::new(
                "fixture:host",
                vec![scriptw_ir::foreign::ForeignFunctionDeclaration::new(
                    "add",
                    "fixture",
                    "add",
                    vec![scriptw_ir::foreign::ForeignValueType::Number; 2],
                    scriptw_ir::foreign::ForeignResultType::Number,
                )
                .unwrap()],
            )
            .unwrap()])
            .unwrap();
        let unit = compiler
            .compile_module(
                r#"
import {add} from 'fixture:host';
export const marker = {answer:42};
export function echo(value) { return value; }
export function same(value) { return value === marker; }
export function fail() { throw marker; }
export function host(value) { return add(value, 1); }
print('precompiled-ready');
"#,
                CompileOptions {
                    foreign_modules: declarations,
                    ..CompileOptions::default()
                },
            )
            .unwrap();
        let artifact = compiler.emit_wasm(&unit).unwrap();
        assert!(artifact.runtime.is_some());
        EmbeddedRuntime::precompile(&shared_wasm_engine().unwrap(), &artifact).unwrap()
    })
}

#[test]
fn precompiled_linked_images_keep_foreign_metadata_retained_calls_throws_and_gc_roots() {
    let bytes = linked_image().as_bytes().to_vec();
    // SAFETY: exact, unchanged image produced by the actual serialization above.
    let image = unsafe { PrecompiledEmbeddedProgram::from_bytes(&bytes) }.unwrap();
    drop(bytes);
    let engine = compiler_disabled();
    let runtime = EmbeddedRuntime::from_precompiled(&engine, &image).unwrap();
    drop(image);
    let mut missing_store = state(&runtime);
    let mut missing_linker = WasmtimeLinker::new(&engine);
    runtime
        .register(&mut missing_store, &mut missing_linker)
        .unwrap();
    assert!(runtime
        .instantiate(&mut missing_store, &mut missing_linker)
        .err()
        .unwrap()
        .to_string()
        .contains("missing declared foreign import"));
    let mut store = state(&runtime);
    let mut linker = WasmtimeLinker::new(&engine);
    linker
        .func_wrap("fixture", "add", |a: f64, b: f64| a + b)
        .unwrap();
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert!(matches!(
        instance.initialize(&mut store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    assert_eq!(
        instance.take_output(&mut store).unwrap(),
        vec![HostOutputEvent::PrintLine("precompiled-ready".into())]
    );
    let marker = normal(instance.get_export(&mut store, "marker").unwrap());
    let echo = normal(instance.get_export(&mut store, "echo").unwrap());
    let same = normal(instance.get_export(&mut store, "same").unwrap());
    let echoed = normal(instance.call(&mut store, &echo, None, &[marker]).unwrap());
    store.gc(None).unwrap();
    assert_eq!(
        normal(instance.call(&mut store, &same, None, &[echoed]).unwrap()).as_boolean(),
        Some(true)
    );
    let fail = normal(instance.get_export(&mut store, "fail").unwrap());
    let JsCompletion::Throw(error) = instance.call(&mut store, &fail, None, &[]).unwrap() else {
        panic!("compiled JavaScript throw must retain its completion");
    };
    store.gc(None).unwrap();
    assert_eq!(
        normal(instance.call(&mut store, &same, None, &[error]).unwrap()).as_boolean(),
        Some(true)
    );
    let host = normal(instance.get_export(&mut store, "host").unwrap());
    let n = instance
        .value(&mut store, ForeignValue::Number(41.0))
        .unwrap();
    assert_eq!(
        normal(instance.call(&mut store, &host, None, &[n]).unwrap()).as_number(),
        Some(42.0)
    );
    assert!(store.data().native_source_compiler.is_none());
}

#[test]
fn precompiled_rejects_wrong_runtime_missing_link_and_incompatible_native_engine_without_fallback()
{
    let engine = compiler_disabled();
    let mut wrong = linked_image().clone();
    wrong.runtime_key.as_mut().unwrap()[0] ^= 1;
    assert!(EmbeddedRuntime::from_precompiled(&engine, &wrong)
        .err()
        .unwrap()
        .to_string()
        .contains("runtime key"));
    wrong.runtime_key = None;
    assert!(EmbeddedRuntime::from_precompiled(&engine, &wrong)
        .err()
        .unwrap()
        .to_string()
        .contains("linkage differs"));
    assert!(
        EmbeddedRuntime::from_precompiled(&shared_wasm_engine().unwrap(), linked_image())
            .err()
            .unwrap()
            .to_string()
            .contains("compiler-disabled")
    );
    let standalone =
        EmbeddedRuntime::precompile(&shared_wasm_engine().unwrap(), &script("42;")).unwrap();
    let mut config = wasmtime::Config::new();
    configure_precompiled_wasm_runtime(&mut config);
    config
        .module_version(wasmtime::ModuleVersionStrategy::Custom(
            "different-native-version".into(),
        ))
        .unwrap();
    let incompatible = wasmtime::Engine::new(&config).unwrap();
    assert!(
        EmbeddedRuntime::from_precompiled(&incompatible, linked_image())
            .err()
            .unwrap()
            .to_string()
            .contains("no compiler fallback")
    );
    assert!(
        EmbeddedRuntime::from_precompiled(&incompatible, &standalone)
            .err()
            .unwrap()
            .to_string()
            .contains("native program is incompatible")
    );
}

#[test]
fn precompiled_rejects_implicit_start_native_host_and_nonminimal_policy() {
    let mut artifact = script("42;");
    artifact.bytes =
        b"\0asm\x01\0\0\0\x01\x04\x01\x60\0\0\x03\x02\x01\0\x08\x01\0\x0a\x04\x01\x02\0\x0b"
            .to_vec();
    let name = scriptw_aot_wasm::RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION.as_bytes();
    artifact
        .bytes
        .extend_from_slice(&[0, (name.len() + 2) as u8, name.len() as u8]);
    artifact.bytes.extend_from_slice(name);
    artifact.bytes.push(0);
    WasmtimeModule::validate(&shared_wasm_engine().unwrap(), &artifact.bytes).unwrap();
    assert!(
        EmbeddedRuntime::precompile(&shared_wasm_engine().unwrap(), &artifact)
            .err()
            .unwrap()
            .to_string()
            .contains("implicit Wasm start")
    );
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_script("print(1);", CompileOptions::default())
        .unwrap();
    let native = run_on_sized_stack(|| {
        compiler.emit_wasm_with_cache_on_current_thread(
            &unit,
            None,
            scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost,
        )
    })
    .unwrap();
    assert!(
        EmbeddedRuntime::precompile(&shared_wasm_engine().unwrap(), &native)
            .err()
            .unwrap()
            .to_string()
            .contains("Portable")
    );
    let mut custom = script("42;");
    custom.compilation_policy = Some(WasmCompilationPolicy::from_options(&CompileOptions {
        intl_profile: IntlCompilationProfile::Custom(
            scriptw_intl::CustomProfileId::parse("precompiled-other").unwrap(),
        ),
        ..CompileOptions::default()
    }));
    assert!(
        EmbeddedRuntime::precompile(&shared_wasm_engine().unwrap(), &custom)
            .err()
            .unwrap()
            .to_string()
            .contains("Minimal Intl")
    );
}

#[test]
fn precompiled_framing_rejects_corruption_truncation_overflow_and_trailing_data_before_native_admission(
) {
    // These inert frames never acquire the unsafe native-origin capability.
    let bytes = envelope::encode(
        b"raw-program",
        b"native-image",
        None,
        WasmNativeCompilationMode::Fast,
    )
    .unwrap();
    for end in 0..bytes.len() {
        assert!(envelope::decode(&bytes[..end]).is_err());
    }
    let mut damaged = bytes.clone();
    damaged[95] ^= 1;
    assert!(envelope::decode(&damaged).is_err());
    let resign = |bytes: &mut [u8]| {
        let end = bytes.len() - 32;
        let digest = sha2::Sha256::digest(&bytes[..end]);
        bytes[end..].copy_from_slice(&digest);
    };
    let mut overflow = bytes.clone();
    overflow[74..82].copy_from_slice(&u64::MAX.to_le_bytes());
    resign(&mut overflow);
    assert!(envelope::decode(&overflow).is_err());
    let mut source = bytes.clone();
    source[8] ^= 1;
    resign(&mut source);
    assert!(envelope::decode(&source).is_err());
    let mut extra = bytes;
    extra.insert(extra.len() - 32, 0);
    resign(&mut extra);
    assert!(envelope::decode(&extra).is_err());
}
