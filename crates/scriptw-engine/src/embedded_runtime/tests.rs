use super::*;
use crate::wasmtime_config::base_config;
use scriptw_intl::{IntlCompilationProfile, IntlDataSelection};
use wasmtime::WasmBacktraceDetails;

#[test]
fn precompiled_runtime_functions_reuse_stencils_without_reusing_stale_code() {
    use std::sync::Arc;
    let directory = std::env::temp_dir().join(format!(
        "scriptw-build-runtime-function-cache-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    let identity = env!("SCRIPTW_NATIVE_COMPILER_FINGERPRINT");
    let compile =
        |value, mode, allocator: manifest::RuntimeRegisterAllocation, cached, identity| {
            // (module (func (export "answer") (result i32) i32.const VALUE))
            let wasm = vec![
                0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 2, 1, 0, 7, 10, 1, 6, 97,
                110, 115, 119, 101, 114, 0, 0, 10, 6, 1, 4, 0, 65, value, 11,
            ];
            let mut config = base_config(mode, WasmBacktraceDetails::Disable);
            config.cranelift_regalloc_algorithm(allocator.algorithm());
            config.target(env!("SCRIPTW_RUNTIME_BUILD_TARGET")).unwrap();
            config.parallel_compilation(false);
            crate::wasmtime_config::configure_native_version(&mut config, identity);
            let store =
                Arc::new(crate::cache::FunctionCache::new(directory.clone(), 1024 * 1024).unwrap());
            let cache = Arc::new(crate::cache::NativeFunctionCache::new(store, identity).unwrap());
            if cached {
                config
                    .enable_incremental_compilation(cache.clone())
                    .unwrap();
            }
            let engine = Engine::new(&config).unwrap();
            let native = engine.precompile_module(&wasm).unwrap();
            let module = wasmtime::Module::new(&engine, &wasm).unwrap();
            let mut store = wasmtime::Store::new(&engine, ());
            store.set_epoch_deadline(1);
            let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
            let answer = instance
                .get_typed_func::<(), i32>(&mut store, "answer")
                .unwrap();
            assert_eq!(answer.call(&mut store, ()).unwrap(), i32::from(value));
            (native, cache.counters())
        };
    let mode = manifest::COMPILATION_MODES.runtime;
    let allocator = manifest::COMPILATION_MODES.register_allocation;
    let (cold, cold_counts) = compile(7, mode, allocator, true, identity);
    assert!(cold_counts.1 > 0);
    let (warm, warm_counts) = compile(7, mode, allocator, true, identity);
    assert_eq!(cold, warm);
    assert!(warm_counts.0 > 0);
    assert_eq!(warm_counts.1, 0);
    let (uncached, _) = compile(7, mode, allocator, false, identity);
    assert_eq!(warm, uncached);
    let (changed, changed_counts) = compile(8, mode, allocator, true, identity);
    assert_ne!(warm, changed);
    assert!(changed_counts.1 > 0);
    let (_, different_mode_counts) = compile(
        7,
        WasmNativeCompilationMode::Fast,
        manifest::RuntimeRegisterAllocation::SinglePass,
        true,
        identity,
    );
    assert!(different_mode_counts.1 > 0);
    let (_, different_allocator_counts) = compile(
        7,
        mode,
        manifest::RuntimeRegisterAllocation::SinglePass,
        true,
        identity,
    );
    assert!(different_allocator_counts.1 > 0);
    let mut changed_identity = identity.as_bytes().to_vec();
    changed_identity[0] = if changed_identity[0] == b'0' {
        b'1'
    } else {
        b'0'
    };
    let changed_identity = String::from_utf8(changed_identity).unwrap();
    let (_, different_dependency_counts) = compile(7, mode, allocator, true, &changed_identity);
    assert!(different_dependency_counts.1 > 0);

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn embedded_manifest_rejects_metadata_and_native_mismatches_before_deserialize() {
    for profile in [
        RuntimeSourceCompilationProfile::NativeHost,
        RuntimeSourceCompilationProfile::Portable,
    ] {
        let images = Images::for_profile(profile);
        if images.manifest.is_empty() {
            assert!(images.package.is_empty());
            assert!(!cfg!(all(
                target_endian = "little",
                any(
                    target_arch = "x86_64",
                    target_arch = "aarch64",
                    target_arch = "riscv64"
                )
            )));
            continue;
        }
        for end in 0..images.manifest.len() {
            assert!(manifest::Manifest::decode(&images.manifest[..end]).is_none());
        }
        let mut extended = images.manifest.to_vec();
        extended.push(0);
        assert!(manifest::Manifest::decode(&extended).is_none());
        let bundle = EmbeddedNativeRuntime::admit(images).unwrap();
        let native = match profile {
            RuntimeSourceCompilationProfile::NativeHost => scriptw_runtime_image::native().unwrap(),
            RuntimeSourceCompilationProfile::Portable => {
                scriptw_runtime_image::portable_native().unwrap()
            }
        };
        assert_eq!(&digests::digest(native.bytes()), native.digest());
        assert!(bundle.accepts_native(&native));
        let mut previous = images.manifest.to_vec();
        previous[8..12].copy_from_slice(&3_u32.to_le_bytes());
        assert!(manifest::Manifest::decode(&previous).is_none());
        for (offset, unknown) in [(16, 3), (17, 4), (18, 2), (17, 255)] {
            let mut changed = images.manifest.to_vec();
            changed[offset] = unknown;
            assert!(manifest::Manifest::decode(&changed).is_none());
        }
        for field in [
            "source",
            "target",
            "configuration",
            "runtime mode",
            "execution mode",
            "runtime register allocation",
            "package digest",
        ] {
            let mut changed = manifest::Manifest::decode(images.manifest).unwrap();
            match field {
                "source" => changed.source[0] ^= 1,
                "target" => changed.target.push_str("-different"),
                "configuration" => changed.configuration += 1,
                "runtime mode" => changed.modes.runtime = WasmNativeCompilationMode::Fast,
                "execution mode" => changed.modes.execution = manifest::ExecutionModes::Fast,
                "runtime register allocation" => {
                    changed.modes.register_allocation =
                        manifest::RuntimeRegisterAllocation::SinglePass
                }
                "package digest" => changed.package_digest[0] ^= 1,
                _ => unreachable!(),
            }
            assert!(
                EmbeddedNativeRuntime::admit_manifest(images, changed).is_none(),
                "{field}"
            );
        }
        let mut changed = manifest::Manifest::decode(images.manifest).unwrap();
        changed.native_digest[0] ^= 1;
        let metadata = EmbeddedNativeRuntime::admit_manifest(images, changed).unwrap();
        assert!(
            !metadata.accepts_native(&native),
            "native digest must reject before deserialize"
        );
        let other_profile = match profile {
            RuntimeSourceCompilationProfile::NativeHost => {
                RuntimeSourceCompilationProfile::Portable
            }
            RuntimeSourceCompilationProfile::Portable => {
                RuntimeSourceCompilationProfile::NativeHost
            }
        };
        let other = Images::for_profile(other_profile);
        let original = manifest::Manifest::decode(images.manifest).unwrap();
        let other_manifest = manifest::Manifest::decode(other.manifest).unwrap();
        assert_ne!(original.runtime_key, other_manifest.runtime_key);
        assert_ne!(original.native_digest, other_manifest.native_digest);
        // The public leaf type has no arbitrary-byte constructor. Swap the
        // independent expected digest instead; actual bytes remain Cargo-owned.
        let mut changed = manifest::Manifest::decode(images.manifest).unwrap();
        changed.native_digest = other_manifest.native_digest;
        let metadata = EmbeddedNativeRuntime::admit_manifest(images, changed).unwrap();
        assert!(!metadata.accepts_native(&native));
        assert!(EmbeddedNativeRuntime::admit_manifest(
            Images {
                package: other.package,
                ..images
            },
            original
        )
        .is_none());
    }
}

#[cfg(all(
    target_endian = "little",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )
))]
#[test]
fn embedded_runtime_matches_target_emission_and_preserves_native_compatibility_checks() {
    std::thread::Builder::new()
        .name("embedded-runtime-target-parity".to_owned())
        .stack_size(crate::ENGINE_WORKER_STACK_SIZE)
        .spawn(|| {
            for profile in [
                RuntimeSourceCompilationProfile::NativeHost,
                RuntimeSourceCompilationProfile::Portable,
            ] {
                let images = Images::for_profile(profile);
                let selection = IntlDataSelection::new(IntlCompilationProfile::Minimal);
                let runtime = scriptw_aot_wasm::runtime_artifact_with_inputs(
                    &selection,
                    inputs(None, profile),
                )
                .expect("embedded package passes full target Intl/Wasm/layout admission");
                let target =
                    scriptw_aot_wasm::build_runtime_artifact_with_source_compilation_profile(
                        &selection,
                        expected_source(),
                        profile,
                    )
                    .expect("target compiler independently emits R");
                assert_eq!(runtime.source_compilation_profile(), profile);
                assert_eq!(
                    scriptw_aot_wasm::RuntimeSourceCompilationProfile::from_module(target.wasm())
                        .unwrap(),
                    runtime.source_compilation_profile()
                );
                assert_eq!(runtime.key(), target.key());
                assert_eq!(
                    target.package_bytes(),
                    images.package,
                    "the entire host/target raw package is byte-identical"
                );
                assert_eq!(
                    runtime.bytes(),
                    target.wasm(),
                    "host-produced R equals ordinary target R exactly"
                );
                let bundle = EmbeddedNativeRuntime::admit(images).unwrap();
                assert_eq!(
                    bundle.manifest.modes.runtime,
                    WasmNativeCompilationMode::SizeOptimized
                );
                assert_eq!(
                    bundle.manifest.modes.execution,
                    manifest::ExecutionModes::FastSizeOptimizedAndSpeed
                );
                assert_eq!(
                    bundle.manifest.modes.register_allocation,
                    manifest::RuntimeRegisterAllocation::Backtracking,
                );
                let mut runtime_config =
                    base_config(bundle.manifest.modes.runtime, WasmBacktraceDetails::Disable);
                runtime_config.cranelift_regalloc_algorithm(
                    bundle.manifest.modes.register_allocation.algorithm(),
                );
                let runtime_compiler = Engine::new(&runtime_config).unwrap();
                assert_eq!(
                    runtime_compiler.get_cranelift_opt_level(),
                    Some(wasmtime::OptLevel::SpeedAndSize),
                    "the Cargo producer's selected R mode is optimized"
                );
                let config = base_config(
                    WasmNativeCompilationMode::Fast,
                    WasmBacktraceDetails::Disable,
                );
                let engine = Engine::new(&config).unwrap();
                assert_eq!(
                    engine.get_cranelift_opt_level(),
                    Some(wasmtime::OptLevel::None)
                );
                crate::wasmtime_policy::PRODUCT_WASMTIME_POLICY
                    .verify_engine(&engine)
                    .unwrap();
                let first = bundle
                .load(&engine, &runtime, WasmNativeCompilationMode::Fast, &mut Default::default())
                .expect(
                    "Backtracking-produced R loads into the actual Fast/SinglePass target engine",
                );
                assert!(
                    Engine::same(first.engine(), &engine),
                    "optimized R is owned by the actual Fast execution Engine"
                );
                assert!(
                    !Engine::same(first.engine(), &runtime_compiler),
                    "the build compiler is not an execution or retention owner"
                );
                let fast_admitted = load_native(
                    &engine,
                    &runtime,
                    WasmNativeCompilationMode::Fast,
                    &mut Default::default(),
                )
                .expect("the product factory selects this source profile in Fast mode");
                assert!(Engine::same(fast_admitted.engine(), &engine));
                let second = bundle
                    .load(
                        &engine,
                        &runtime,
                        WasmNativeCompilationMode::Fast,
                        &mut Default::default(),
                    )
                    .expect("a second factory call owns a new native mapping");
                assert_ne!(
                    first.image_range().start,
                    second.image_range().start,
                    "the embedded authority retains no native Module"
                );
                let size_engine = Engine::new(&base_config(
                    WasmNativeCompilationMode::SizeOptimized,
                    WasmBacktraceDetails::Disable,
                ))
                .unwrap();
                crate::wasmtime_policy::PRODUCT_WASMTIME_POLICY
                    .verify_engine(&size_engine)
                    .unwrap();
                let optimized = bundle
                    .load(
                        &size_engine,
                        &runtime,
                        WasmNativeCompilationMode::SizeOptimized,
                        &mut Default::default(),
                    )
                    .expect(
                        "the same image loads into the SizeOptimized/SinglePass product Engine",
                    );
                assert!(Engine::same(optimized.engine(), &size_engine));
                let admitted = load_native(
                    &size_engine,
                    &runtime,
                    WasmNativeCompilationMode::SizeOptimized,
                    &mut Default::default(),
                )
                .expect("the product entry admits both declared execution modes");
                assert!(Engine::same(admitted.engine(), &size_engine));
                let other_profile = match profile {
                    RuntimeSourceCompilationProfile::NativeHost => {
                        RuntimeSourceCompilationProfile::Portable
                    }
                    RuntimeSourceCompilationProfile::Portable => {
                        RuntimeSourceCompilationProfile::NativeHost
                    }
                };
                let mut wrong_profile =
                    EmbeddedNativeRuntime::admit(Images::for_profile(other_profile)).unwrap();
                wrong_profile.manifest.runtime_key = *runtime.key().as_bytes();
                assert!(wrong_profile
                    .load(
                        &engine,
                        &runtime,
                        WasmNativeCompilationMode::Fast,
                        &mut Default::default()
                    )
                    .is_none());
                let mut foreign_key = EmbeddedNativeRuntime::admit(images).unwrap();
                foreign_key.manifest.runtime_key[0] ^= 1;
                assert!(
                    foreign_key
                        .load(
                            &engine,
                            &runtime,
                            WasmNativeCompilationMode::Fast,
                            &mut Default::default()
                        )
                        .is_none(),
                    "native R cannot attach to another raw runtime key"
                );
                let mut mismatched = base_config(
                    WasmNativeCompilationMode::Fast,
                    WasmBacktraceDetails::Disable,
                );
                mismatched.epoch_interruption(false);
                let incompatible = Engine::new(&mismatched).unwrap();
                assert!(
                    bundle
                        .load(
                            &incompatible,
                            &runtime,
                            WasmNativeCompilationMode::Fast,
                            &mut Default::default()
                        )
                        .is_none(),
                    "known-valid native bytes with incompatible tunables are an optimization miss"
                );
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
