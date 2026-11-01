use super::*;

#[test]
fn program_cache_decoder_reattaches_only_the_recorded_runtime() {
    let root = std::env::temp_dir().join(format!(
        "scriptw-engine-linked-cache-decoder-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let cache = cache::FunctionCache::new(root.clone(), 64 * 1024 * 1024).unwrap();
    let profile = IntlCompilationProfile::default();
    let selection = IntlDataSelection::new(profile.clone());
    let runtime_cache = RuntimeWasmCache::new(&cache);
    let runtime = scriptw_aot_wasm::runtime_artifact_with_inputs(
        &selection,
        embedded_runtime::inputs(
            Some(&runtime_cache),
            scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost,
        ),
    )
    .unwrap();
    for mode in [
        WasmNativeCompilationMode::Fast,
        WasmNativeCompilationMode::SizeOptimized,
    ] {
        assert_eq!(
            wasm_module_memory_cache_key(WasmModuleSource::Runtime(&runtime), mode),
            wasm_module_memory_cache_key(WasmModuleSource::Program(runtime.bytes()), mode),
            "retained runtime digests must preserve the actual-byte cache identity"
        );
        let diagnostic =
            wasm_module_memory_cache_key(WasmModuleSource::DiagnosticRuntime(&runtime), mode);
        let ordinary = wasm_module_memory_cache_key(WasmModuleSource::Runtime(&runtime), mode);
        assert_eq!(diagnostic.wasm_sha256, ordinary.wasm_sha256);
        assert_ne!(
            diagnostic, ordinary,
            "image-treatment authority must not alias ordinary retained R"
        );
    }
    let program_for_profile = |profile| {
        let mut module = b"\0asm\x01\0\0\0".to_vec();
        let section = |module: &mut Vec<u8>, id, payload: &[u8]| {
            // Every section in these two minimal fixtures has a one-byte length.
            assert!(payload.len() < 128);
            module.extend_from_slice(&[id, u8::try_from(payload.len()).unwrap()]);
            module.extend_from_slice(payload);
        };
        let wire = match profile {
            scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable => 0,
            scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost => {
                // One () -> () type and three function imports suffice for
                // decoder admission; this fixture does not instantiate them.
                section(&mut module, 1, &[1, 0x60, 0, 0]);
                let mut imports = vec![3];
                for name in [
                    "compile_dynamic_function",
                    "compile_global_script",
                    "compile_direct_eval",
                ] {
                    assert!(name.len() < 128);
                    let host = b"scriptw_host";
                    imports.push(u8::try_from(host.len()).unwrap());
                    imports.extend_from_slice(host);
                    imports.push(u8::try_from(name.len()).unwrap());
                    imports.extend_from_slice(name.as_bytes());
                    imports.extend_from_slice(&[0, 0]); // function, type index 0
                }
                section(&mut module, 2, &imports);
                1
            }
        };
        let name = scriptw_aot_wasm::RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION.as_bytes();
        assert!(name.len() < 128);
        let mut profile_section = vec![u8::try_from(name.len()).unwrap()];
        profile_section.extend_from_slice(name);
        profile_section.push(wire);
        section(&mut module, 0, &profile_section);
        wasmparser::Validator::new().validate_all(&module).unwrap();
        assert_eq!(
            scriptw_aot_wasm::RuntimeSourceCompilationProfile::from_module(&module).unwrap(),
            profile
        );
        module
    };
    let native_program =
        program_for_profile(scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost);
    let entry = encode_cache_entry(WasmProgramRef::new(&native_program, Some(&runtime)));
    let (decoded, linked) = decode_cache_entry(&entry, &profile, &cache).unwrap();
    assert_eq!(decoded.as_ref(), native_program.as_slice());
    assert_eq!(linked.unwrap().key(), runtime.key());
    assert!(decode_standalone_cache_entry(&entry).is_none());

    let mut different_runtime = entry.clone();
    different_runtime[1] ^= 1;
    assert!(decode_cache_entry(&different_runtime, &profile, &cache).is_none());
    assert!(decode_cache_entry(&entry[..KEY_BYTES], &profile, &cache).is_none());
    assert!(decode_cache_entry(&[], &profile, &cache).is_none());
    assert!(decode_cache_entry(&[0xff], &profile, &cache).is_none());

    let portable_program =
        program_for_profile(scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable);
    let standalone = encode_cache_entry(WasmProgramRef::standalone(&portable_program));
    let (decoded, linked) = decode_cache_entry(&standalone, &profile, &cache).unwrap();
    assert_eq!(decoded.as_ref(), portable_program.as_slice());
    assert!(linked.is_none());
    assert_eq!(
        decode_standalone_cache_entry(&standalone),
        Some(portable_program.as_slice())
    );
    // The original header-only fixture has no physical source-profile witness.
    let missing_profile = b"\0asm\x01\0\0\0";
    let missing_standalone = encode_cache_entry(WasmProgramRef::standalone(missing_profile));
    assert!(decode_standalone_cache_entry(&missing_standalone).is_none());
    assert!(decode_cache_entry(&missing_standalone, &profile, &cache).is_none());
    let missing_linked = encode_cache_entry(WasmProgramRef::new(missing_profile, Some(&runtime)));
    assert!(decode_cache_entry(&missing_linked, &profile, &cache).is_none());
    let wrong_profile = encode_cache_entry(WasmProgramRef::new(&portable_program, Some(&runtime)));
    assert!(decode_cache_entry(&wrong_profile, &profile, &cache).is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn linked_runtime_native_modules_honor_both_bypass_retention_paths() {
    run_on_sized_stack(|| {
        let engine = Engine::new(RealmBuilder::new().build());
        let unit = engine.compile_script(
            "const linkedRuntimeBypass = {value: 4815162342}; linkedRuntimeBypass.value;",
            CompileOptions::default(),
        )?;
        let artifact = engine.emit_wasm(&unit)?;
        let runtime = &artifact.runtime.as_ref().expect("fixture must link R").0;
        let program = WasmProgramRef::new(&artifact.bytes, Some(runtime));
        let mode = plan_native_compilation(program).mode;
        let native = shared_native_mode_wasm_engine(mode)?;
        for (policy, agent) in [
            (WasmModuleMemoryCachePolicy::BypassRetention, false),
            (WasmModuleMemoryCachePolicy::Retain, true),
        ] {
            let first = compile_modules(
                &native,
                program,
                policy,
                agent,
                mode,
                WasmRuntimeImagePolicy::EmbeddedBuild,
            )?;
            let second = compile_modules(
                &native,
                program,
                policy,
                agent,
                mode,
                WasmRuntimeImagePolicy::EmbeddedBuild,
            )?;
            assert!(matches!(
                first.memory_cache_outcome,
                WasmModuleMemoryCacheOutcome::Bypassed
            ));
            assert!(matches!(
                second.memory_cache_outcome,
                WasmModuleMemoryCacheOutcome::Bypassed
            ));
            // Keep both modules alive and repeat each path independently: a
            // hidden R cache must not retain either an explicit bypass or an
            // agent execution. Disk hits own distinct live image mappings.
            assert_ne!(
                first.runtime.as_ref().unwrap().image_range().start,
                second.runtime.as_ref().unwrap().image_range().start,
                "bypassed executions must not reuse a hidden retained R module (agent={agent})"
            );
        }
        Ok::<_, EngineError>(())
    })
    .expect("both linked modules compile with retention bypassed");
}

#[test]
fn runtime_only_retention_reuses_runtime_and_bypasses_program_modules() {
    run_on_sized_stack(|| {
        let engine = Engine::new(RealmBuilder::new().build());
        let unit = engine.compile_script(
            "const runtimeOnlyProgram = {answer: 42}; runtimeOnlyProgram.answer;",
            CompileOptions::default(),
        )?;
        let artifact = engine.emit_wasm(&unit)?;
        let runtime = &artifact.runtime.as_ref().expect("fixture must link R").0;
        let program = WasmProgramRef::new(&artifact.bytes, Some(runtime));
        let mode = plan_native_compilation(program).mode;
        let native = shared_native_mode_wasm_engine(mode)?;
        let first = compile_modules(
            &native,
            program,
            WasmModuleMemoryCachePolicy::RetainRuntime,
            false,
            mode,
            WasmRuntimeImagePolicy::EmbeddedBuild,
        )?;
        // Oversized runtime images execute without admission. Observe the
        // actual bounded cache instead of assuming every valid budget fits R.
        let key = wasm_module_memory_cache_key(WasmModuleSource::Runtime(runtime), mode);
        let retained = memory_wasm_modules()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&key);
        let second = compile_modules(
            &native,
            program,
            WasmModuleMemoryCachePolicy::RetainRuntime,
            false,
            mode,
            WasmRuntimeImagePolicy::EmbeddedBuild,
        )?;
        if let Some(retained) = retained {
            assert_eq!(
                first.runtime.as_ref().unwrap().image_range().start,
                retained.image_range().start,
                "the retained runtime must be the executed runtime Module"
            );
            assert_eq!(
                retained.image_range().start,
                second.runtime.as_ref().unwrap().image_range().start,
                "persistent requests should reuse an admitted immutable runtime Module"
            );
        } else {
            assert_ne!(
                first.runtime.as_ref().unwrap().image_range().start,
                second.runtime.as_ref().unwrap().image_range().start,
                "a runtime image outside the bounded cache must execute without hidden retention"
            );
        }
        assert_ne!(
            first.program.image_range().start,
            second.program.image_range().start
        );
        assert!(matches!(
            first.memory_cache_outcome,
            WasmModuleMemoryCacheOutcome::Bypassed
        ));
        assert!(matches!(
            second.memory_cache_outcome,
            WasmModuleMemoryCacheOutcome::Bypassed
        ));
        assert!(!memory_wasm_module_is_cached(&artifact.bytes));
        Ok::<_, EngineError>(())
    })
    .expect("runtime-only retention must leave program images unretained");
}

#[test]
fn agent_override_bypasses_runtime_only_retention() {
    run_on_sized_stack(|| {
        let engine = Engine::new(RealmBuilder::new().build());
        let unit = engine.compile_script(
            "const runtimeOnlyAgent = {answer: 19}; runtimeOnlyAgent.answer;",
            CompileOptions::default(),
        )?;
        let artifact = engine.emit_wasm(&unit)?;
        let runtime = &artifact.runtime.as_ref().expect("fixture must link R").0;
        let program = WasmProgramRef::new(&artifact.bytes, Some(runtime));
        let mode = plan_native_compilation(program).mode;
        let native = shared_native_mode_wasm_engine(mode)?;
        let first = compile_modules(
            &native,
            program,
            WasmModuleMemoryCachePolicy::RetainRuntime,
            true,
            mode,
            WasmRuntimeImagePolicy::EmbeddedBuild,
        )?;
        let second = compile_modules(
            &native,
            program,
            WasmModuleMemoryCachePolicy::RetainRuntime,
            true,
            mode,
            WasmRuntimeImagePolicy::EmbeddedBuild,
        )?;
        assert_ne!(
            first.runtime.as_ref().unwrap().image_range().start,
            second.runtime.as_ref().unwrap().image_range().start,
            "an agent-owned Engine must not borrow the global runtime cache"
        );
        assert_ne!(
            first.program.image_range().start,
            second.program.image_range().start
        );
        assert!(matches!(
            first.memory_cache_outcome,
            WasmModuleMemoryCacheOutcome::Bypassed
        ));
        assert!(matches!(
            second.memory_cache_outcome,
            WasmModuleMemoryCacheOutcome::Bypassed
        ));
        Ok::<_, EngineError>(())
    })
    .expect("agent execution must preserve the full bypass policy");
}

#[test]
fn persistent_script_and_module_entries_keep_fresh_globals_with_shared_runtime_code() {
    run_on_sized_stack(|| {
        let source = "globalThis.runtimeOnlyFreshGlobal = \
            (globalThis.runtimeOnlyFreshGlobal || 0) + 1; globalThis.runtimeOnlyFreshGlobal;";
        for _ in 0..2 {
            let engine = Engine::new(RealmBuilder::new().build());
            let outcome = engine.run_wasm_aot_script_on_current_thread(
                source,
                CompileOptions::default(),
                None,
                true,
            )?;
            assert!(outcome.note.contains("number(1)"), "{outcome:?}");
            let engine = Engine::new(RealmBuilder::new().build());
            let outcome = engine.run_wasm_aot_module_on_current_thread(
                "if (globalThis.runtimeOnlyFreshGlobal !== undefined) \
                     throw new Error('prior case global leaked'); \
                 globalThis.runtimeOnlyFreshGlobal = 99; export const answer = 42;",
                CompileOptions::default(),
                None,
                true,
            )?;
            assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
        }
        Ok::<_, EngineError>(())
    })
    .expect("runtime Module reuse must preserve fresh Script and Module case state");
}
