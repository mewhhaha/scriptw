use super::*;
use sha2::{Digest, Sha256};
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
    store
}

fn normal(completion: JsCompletion) -> JsValue {
    match completion {
        JsCompletion::Normal(value) => value,
        JsCompletion::Throw(value) => panic!("unexpected throw: {value:?}"),
    }
}

fn resign(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let digest = blake3::hash(&bytes[..end]);
    bytes[end..].copy_from_slice(digest.as_bytes());
}

struct ImageDirectory(std::path::PathBuf);

impl ImageDirectory {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "scriptw-swep-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for ImageDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
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
    #[cfg(feature = "native-source-compiler")]
    assert!(store.data().native_source_compiler.is_none());
    let mut linker = WasmtimeLinker::new(&engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert_eq!(
        instance.evaluate(&mut store).unwrap().completion,
        ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(42.0)))
    );
}

#[cfg(all(unix, not(miri)))]
#[test]
fn precompiled_file_executes_from_the_admitted_inode_after_path_replacement() {
    use std::fs;

    let artifact = script("42;");
    let produced = EmbeddedRuntime::precompile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let directory = ImageDirectory::new();
    let path = directory.0.join("program.swep");
    let original = directory.0.join("original.swep");
    fs::write(&path, produced.as_bytes()).unwrap();
    // SAFETY: the fixture owns the exact producer output and keeps the inode
    // immutable. Rename and unlink below only change directory entries.
    let image = unsafe { PrecompiledEmbeddedProgram::from_file(&path) }.unwrap();
    assert_eq!(image.program_wasm(), artifact.bytes.as_slice());
    assert_eq!(image.as_bytes(), produced.as_bytes());
    assert_eq!(
        image.serialized_execution_facts(),
        produced.serialized_execution_facts()
    );
    assert_eq!(image.runtime_key(), produced.runtime_key());
    assert_eq!(
        image.serialized_program_len(),
        produced.serialized_program_len()
    );
    assert_eq!(
        image.image_sha256().as_slice(),
        Sha256::digest(produced.as_bytes()).as_slice()
    );
    assert_eq!(image.image_sha256(), produced.image_sha256());
    assert_eq!(image.clone().image_sha256(), produced.image_sha256());

    fs::rename(&path, &original).unwrap();
    fs::write(&path, b"separate inode").unwrap();
    let engine = compiler_disabled();
    let first = EmbeddedRuntime::from_precompiled(&engine, &image).unwrap();
    let second = EmbeddedRuntime::from_precompiled(&engine, &image).unwrap();
    drop(image);

    #[cfg(target_os = "linux")]
    {
        // Both native modules map the same inode at file offset zero through
        // distinct COW views. Reading /proc maps checks the backing itself,
        // so a successful anonymous-copy fallback would fail this control.
        use std::os::unix::fs::MetadataExt;

        let inode = fs::metadata(&original).unwrap().ino();
        let maps = fs::read_to_string("/proc/self/maps").unwrap();
        let ranges = [&first, &second].map(|runtime| runtime.modules.program.image_range());
        assert_ne!(ranges[0].start, ranges[1].start);
        for range in ranges {
            let start = range.start as usize;
            assert_eq!(
                range.end as usize - start,
                produced.serialized_program_len()
            );
            assert!(maps.lines().any(|line| {
                let mut fields = line.split_whitespace();
                let address = fields.next().unwrap().split_once('-').unwrap().0;
                let permissions = fields.next().unwrap();
                let offset = fields.next().unwrap();
                let _device = fields.next().unwrap();
                let mapped_inode = fields.next().unwrap();
                usize::from_str_radix(address, 16) == Ok(start)
                    && permissions.ends_with('p')
                    && u64::from_str_radix(offset, 16) == Ok(0)
                    && mapped_inode.parse::<u64>() == Ok(inode)
            }));
        }
    }
    fs::remove_file(original).unwrap();
    for runtime in [first, second] {
        let mut store = state(&runtime);
        let mut linker = WasmtimeLinker::new(&engine);
        runtime.register(&mut store, &mut linker).unwrap();
        let instance = runtime.instantiate(&mut store, &mut linker).unwrap();
        assert_eq!(
            instance.evaluate(&mut store).unwrap().completion,
            ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(42.0)))
        );
    }
}

#[test]
fn precompiled_file_rejects_truncation_and_payload_corruption() {
    let artifact = script("42;");
    let image = EmbeddedRuntime::precompile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let directory = ImageDirectory::new();
    let path = directory.0.join("damaged.swep");
    let mut bytes = image.as_bytes().to_vec();
    for end in [envelope::FOOTER_BYTES - 1, bytes.len() - 1] {
        std::fs::write(&path, &bytes[..end]).unwrap();
        // SAFETY: this private fixture remains immutable while read and the
        // truncation is rejected before native-code admission.
        assert!(unsafe { PrecompiledEmbeddedProgram::from_file(&path) }.is_err());
    }
    bytes[0] ^= 1;
    std::fs::write(&path, bytes).unwrap();
    // SAFETY: checksum failure prevents the changed payload from being admitted.
    assert!(unsafe { PrecompiledEmbeddedProgram::from_file(&path) }.is_err());
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
    let mut wrong_store = state(&runtime);
    let mut wrong_linker = WasmtimeLinker::new(&engine);
    wrong_linker
        .func_wrap("fixture", "add", |a: i32, b: i32| a + b)
        .unwrap();
    runtime
        .register(&mut wrong_store, &mut wrong_linker)
        .unwrap();
    assert!(runtime
        .instantiate(&mut wrong_store, &mut wrong_linker)
        .err()
        .unwrap()
        .to_string()
        .contains("foreign import type mismatch"));
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
    #[cfg(feature = "native-source-compiler")]
    assert!(store.data().native_source_compiler.is_none());
}

#[test]
fn precompiled_rejects_wrong_runtime_missing_link_and_incompatible_native_engine_without_fallback()
{
    let engine = compiler_disabled();
    let mut wrong = linked_image().clone();
    let mut wrong_key = *wrong.runtime_key().unwrap();
    wrong_key[0] ^= 1;
    wrong.facts.runtime = ProgramRuntimeRequirement::CargoMinimalPortable {
        key: RuntimeArtifactKey::from_bytes(wrong_key),
    };
    assert!(EmbeddedRuntime::from_precompiled(&engine, &wrong)
        .err()
        .unwrap()
        .to_string()
        .contains("runtime key"));
    // Inert linkage is a producer-established fact. A public raw artifact
    // cannot mint Standalone facts for a P that imports Cargo R.
    let mut missing_link = script("var answer = 42; answer;");
    assert!(missing_link.runtime.take().is_some());
    assert!(
        EmbeddedRuntime::precompile(&shared_wasm_engine().unwrap(), &missing_link)
            .err()
            .unwrap()
            .to_string()
            .contains("linkage differs")
    );
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
        .compile_script("print({});", CompileOptions::default())
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
        &ProgramExecutionFacts {
            foreign: ForeignModuleRegistry::default(),
            runtime: ProgramRuntimeRequirement::Standalone,
        },
        WasmNativeCompilationMode::Fast,
    )
    .unwrap();
    for end in 0..bytes.len() {
        assert!(envelope::decode(&bytes[..end]).is_err());
    }
    let footer = bytes.len() - envelope::FOOTER_BYTES;
    let mut damaged = bytes.clone();
    damaged[0] ^= 1;
    assert!(envelope::decode(&damaged).is_err());
    let mut overflow = bytes.clone();
    overflow[footer + 74..footer + 82].copy_from_slice(&u64::MAX.to_le_bytes());
    resign(&mut overflow);
    assert!(envelope::decode(&overflow).is_err());
    let mut source = bytes.clone();
    source[footer + 8] ^= 1;
    resign(&mut source);
    let build = compiler_identity::build_source_fingerprint()
        .unwrap()
        .to_string();
    let error = envelope::decode(&source).err().unwrap().to_string();
    assert!(
        error.contains(&build) && error.contains("compiler source"),
        "{error}"
    );
    let mut extra = bytes;
    extra.insert(footer, 0);
    resign(&mut extra);
    assert!(envelope::decode(&extra).is_err());
}

#[test]
fn precompiled_execution_facts_are_canonical_bounded_and_product_versioned() {
    let facts = ProgramExecutionFacts {
        foreign: ForeignModuleRegistry::default(),
        runtime: ProgramRuntimeRequirement::Standalone,
    };
    // Inert frame parsing grants no native-origin authority.
    let bytes =
        envelope::encode(b"raw", b"native", &facts, WasmNativeCompilationMode::Fast).unwrap();
    let footer = bytes.len() - envelope::FOOTER_BYTES;
    assert_eq!(&bytes[footer..footer + 8], b"SCRWEP05");
    let frame = envelope::decode(&bytes).unwrap();
    assert!(frame.facts.foreign.is_empty());
    assert!(matches!(
        frame.facts.runtime,
        ProgramRuntimeRequirement::Standalone
    ));
    assert_eq!(&bytes[frame.program], b"raw");
    assert_eq!(frame.native, 0..6);
    assert_eq!(&bytes[frame.native], b"native");
    for magic in [b"SCRWEP01", b"SCRWEP02", b"SCRWEP03", b"SCRWEP04"] {
        let mut historical = bytes.clone();
        historical[footer..footer + 8].copy_from_slice(magic);
        resign(&mut historical);
        assert!(envelope::decode(&historical).is_err());
    }
    let mut excessive = bytes.clone();
    excessive[footer + 90..footer + 94].copy_from_slice(&u32::MAX.to_le_bytes());
    resign(&mut excessive);
    assert!(envelope::decode(&excessive).is_err());
    let canonical = foreign_facts::encode(&facts.foreign).unwrap();
    assert_eq!(canonical, [2, 0, 0, 0, 0, 0, 0, 0, 0]);
    for registry in [
        [canonical.as_slice(), &[0]].concat(),
        vec![3, 0, 0, 0, 0, 0, 0, 0, 0],
        vec![2, 0, 0, 0, 2, 0, 0, 0, 0],
        vec![2, 0, 0, 0, 0, 255, 255, 255, 255],
        canonical[..8].to_vec(),
        facts.foreign.to_normalized_json(),
    ] {
        let mut changed = bytes[..frame.execution_facts.start].to_vec();
        changed.extend_from_slice(&registry);
        changed.extend_from_slice(&bytes[footer..footer + 90]);
        changed.extend_from_slice(&(registry.len() as u32).to_le_bytes());
        changed.extend_from_slice(&bytes[footer + 94..]);
        resign(&mut changed);
        assert!(envelope::decode(&changed).is_err());
    }
}

#[test]
fn precompile_cannot_mint_facts_for_invalid_wasm_bodies() {
    let mut artifact = script("42;");
    // Valid inert section framing, but an invalid opcode in the function body.
    artifact.bytes =
        b"\0asm\x01\0\0\0\x01\x04\x01\x60\0\0\x03\x02\x01\0\x0a\x04\x01\x02\0\xff".to_vec();
    let name = scriptw_aot_wasm::RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION.as_bytes();
    artifact
        .bytes
        .extend_from_slice(&[0, (name.len() + 2) as u8, name.len() as u8]);
    artifact.bytes.extend_from_slice(name);
    artifact.bytes.push(0);
    assert!(EmbeddedRuntime::precompile(&shared_wasm_engine().unwrap(), &artifact).is_err());
}

#[test]
fn precompiled_facts_roundtrip_async_foreign_declarations_and_explicit_error_tag() {
    use scriptw_ir::foreign::{
        ForeignErrorTag, ForeignFunctionDeclaration, ForeignModuleDeclaration, ForeignResultType,
        ForeignValueType,
    };
    let registry = ForeignModuleRegistry::new(vec![ForeignModuleDeclaration::new(
        "fixture:async",
        vec![ForeignFunctionDeclaration::new_async(
            "read",
            "fixture",
            "read",
            vec![ForeignValueType::Bytes],
            ForeignResultType::StringUtf16,
        )
        .unwrap()],
    )
    .unwrap()])
    .unwrap()
    .with_error_tag(ForeignErrorTag::code_v1());
    let facts = ProgramExecutionFacts {
        foreign: registry.clone(),
        runtime: ProgramRuntimeRequirement::CargoMinimalPortable {
            key: RuntimeArtifactKey::from_bytes([7; 32]),
        },
    };
    let bytes =
        envelope::encode(b"raw", b"native", &facts, WasmNativeCompilationMode::Fast).unwrap();
    let recovered = envelope::decode(&bytes).unwrap();
    assert_eq!(recovered.facts.foreign, registry);
    assert_eq!(
        foreign_facts::decode(&bytes[recovered.execution_facts.clone()]),
        Some(registry)
    );
    assert_eq!(recovered.facts.runtime_key().unwrap().as_bytes(), &[7; 32]);
}

#[cfg(not(feature = "native-source-compiler"))]
#[test]
fn feature_off_precompiled_agent_start_has_no_source_compilation_authority() {
    configure_compilation_jobs(1).unwrap();
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_script(
            "__scriptwAgentStart('throw new Error(\"must not execute\");');",
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
        )
        .unwrap();
    let artifact = compiler.emit_wasm(&unit).unwrap();
    let image = EmbeddedRuntime::precompile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let engine = compiler_disabled();
    let runtime = EmbeddedRuntime::from_precompiled(&engine, &image).unwrap();
    let mut store = state(&runtime);
    let mut linker = WasmtimeLinker::new(&engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    let error = instance.evaluate(&mut store).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("agent.start source compilation is unavailable"),
        "{error}"
    );
}
