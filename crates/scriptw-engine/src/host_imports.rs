//! Store-independent host import registration. Templates retain only host
//! callbacks and Engine-canonical types. Private/shared memory, runtime instances,
//! native-source contexts and all mutable JavaScript state are added to clones.
use super::*;

struct Template {
    key: WasmModuleMemoryCacheKey,
    linker: WasmtimeLinker<WasmHostState>,
}

/// Bounded independently of native module retention: a template contains no
/// Module or artifact bytes, and can therefore outlive an evicted code image.
/// Exact Engine ownership is checked even for identical host bytes/configuration.
const RETAINED_TEMPLATES: usize = 2;

pub(super) fn linker(
    engine: &WasmtimeEngine,
    module: &WasmtimeModule,
    key: WasmModuleMemoryCacheKey,
) -> Result<
    (
        WasmtimeLinker<WasmHostState>,
        WasmRuntimeHostImportCacheOutcome,
    ),
    EngineError,
> {
    if !WasmtimeEngine::same(engine, module.engine()) {
        return Err(EngineError::new(
            "host import template module belongs to another Engine",
        ));
    }
    static TEMPLATES: OnceLock<Mutex<VecDeque<Template>>> = OnceLock::new();
    let templates = TEMPLATES.get_or_init(Default::default);
    {
        let mut templates = templates
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(position) = templates.iter().position(|entry| {
            entry.key == key && WasmtimeEngine::same(entry.linker.engine(), engine)
        }) {
            let entry = templates.remove(position).expect("template key exists");
            let linker = entry.linker.clone();
            templates.push_back(entry);
            return Ok((linker, WasmRuntimeHostImportCacheOutcome::Hit));
        }
    }
    // Validation and host trampoline construction do not hold the cache mutex.
    let linker = build(engine, module)?;
    let mut templates = templates
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(position) = templates.iter().position(|entry| entry.key == key) {
        let entry = templates.remove(position).expect("template key exists");
        if WasmtimeEngine::same(entry.linker.engine(), engine) {
            let linker = entry.linker.clone();
            templates.push_back(entry);
            return Ok((linker, WasmRuntimeHostImportCacheOutcome::Miss));
        }
    }
    if templates.len() == RETAINED_TEMPLATES {
        templates.pop_front();
    }
    templates.push_back(Template {
        key,
        linker: linker.clone(),
    });
    Ok((linker, WasmRuntimeHostImportCacheOutcome::Miss))
}

fn build(
    engine: &WasmtimeEngine,
    module: &WasmtimeModule,
) -> Result<WasmtimeLinker<WasmHostState>, EngineError> {
    let mut linker = WasmtimeLinker::new(engine);
    register(&mut linker, module)?;
    Ok(linker)
}

/// Register the same checked callbacks on an embedder-owned linker. This adds
/// no memory, runtime instance, foreign implementation or source compiler.
pub(super) fn register(
    linker: &mut WasmtimeLinker<WasmHostState>,
    module: &WasmtimeModule,
) -> Result<(), EngineError> {
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_REJECT_RUNTIME_SEMANTICS,
            wasm_reject_runtime_semantics,
        )
        .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_AGENT_CAN_SUSPEND,
            |caller: WasmtimeCaller<'_, WasmHostState>| -> i32 {
                i32::from(caller.data().can_block)
            },
        )
        .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
    wasm_gc_host::link(linker, module)?;
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_AGENT_CALL,
            wasm_agent_host::agent_call,
        )
        .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_RANDOM_F64,
            wasm_random_f64,
        )
        .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_WALL_CLOCK_MILLIS,
            |caller: WasmtimeCaller<'_, WasmHostState>| -> f64 {
                caller
                    .data()
                    .realm
                    .host_clock()
                    .utc_epoch_milliseconds()
                    .get() as f64
            },
        )
        .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_MONOTONIC_CLOCK_NANOS,
            |caller: WasmtimeCaller<'_, WasmHostState>| -> i64 {
                caller
                    .data()
                    .realm
                    .host_clock()
                    .monotonic_instant()
                    .saturating_duration_since(caller.data().monotonic_clock_origin)
                    .as_i64_saturating()
            },
        )
        .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_SLEEP_NANOS,
            |_caller: WasmtimeCaller<'_, WasmHostState>, nanos: i64| {
                let Ok(nanos) = u64::try_from(nanos) else {
                    return;
                };
                std::thread::sleep(std::time::Duration::from_nanos(nanos.min(1_000_000)));
            },
        )
        .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_PRINT_LINE_UTF8,
            |mut caller: WasmtimeCaller<'_, WasmHostState>,
             ptr: i32,
             len: i32|
             -> wasmtime::Result<()> {
                let memory = caller.get_export("memory").ok_or_else(|| {
                    wasmtime::Error::msg("wasmtime host import failed: missing exported memory")
                })?;
                let ptr = usize::try_from(ptr).map_err(|_| {
                    wasmtime::Error::msg("wasmtime host import failed: negative utf-8 pointer")
                })?;
                let len = usize::try_from(len).map_err(|_| {
                    wasmtime::Error::msg("wasmtime host import failed: negative utf-8 length")
                })?;
                let memory_byte_len = match &memory {
                    WasmtimeExtern::Memory(memory) => memory.data_size(&caller),
                    WasmtimeExtern::SharedMemory(memory) => memory.data_size(),
                    _ => {
                        return Err(wasmtime::Error::msg(
                            "wasmtime host import failed: missing exported memory",
                        ));
                    }
                };
                let span = wasm_memory_span(ptr, len, memory_byte_len, "wasm print input")
                    .map_err(|err| {
                        wasmtime::Error::msg(format!("wasmtime host import failed: {err}"))
                    })?;
                let mut bytes = vec![0; span.len()];
                match memory {
                    WasmtimeExtern::Memory(memory) => {
                        memory
                            .read(&caller, span.start, &mut bytes)
                            .map_err(|err| {
                                wasmtime::Error::msg(format!(
                                    "wasmtime host import failed: unable to read memory: {err}"
                                ))
                            })?;
                    }
                    WasmtimeExtern::SharedMemory(memory) => {
                        read_wasmtime_shared_memory(&memory, span.start, &mut bytes).map_err(
                            |err| {
                                wasmtime::Error::msg(format!(
                                    "wasmtime host import failed: unable to read memory: {err}"
                                ))
                            },
                        )?;
                    }
                    _ => unreachable!("memory kind was validated before allocation"),
                }
                let text = String::from_utf8(bytes).map_err(|err| {
                    wasmtime::Error::msg(format!(
                        "wasmtime host import failed: invalid utf-8: {err}"
                    ))
                })?;
                caller.data().output_events.record(&text);
                caller.data().realm.host_hooks().print_line(&text);
                Ok(())
            },
        )
        .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_NUMBER_POW,
            wasm_number_pow,
        )
        .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
    for (name, func) in WASM_MATH_UNARY_IMPORTS {
        linker
            .func_wrap(WASM_HOST_IMPORT_NAMESPACE, name, func)
            .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
    }
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_MATH_ATAN2,
            wasm_math_atan2,
        )
        .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(can_block: bool) -> WasmHostState {
        let realm = RealmBuilder::new().build();
        WasmHostState {
            user_data: None,
            embedding: None,
            weak: wasm_gc_weak_host::WeakGcState::default(),
            #[cfg(feature = "native-source-compiler")]
            native_source_compiler: None,
            #[cfg(feature = "native-source-compiler")]
            native_template_instances: Default::default(),
            monotonic_clock_origin: realm.host_clock().monotonic_instant(),
            realm,
            output_events: WasmOutputEvents::DelegateOnly,
            intl_kernel: shared_embedded_intl_kernel().unwrap().into(),
            can_block,
            async_waiters: WasmStoreAsyncWaiters::new(None),
            shared_memory_backing: None,
            agent_group: None,
            agent_commands: None,
            agent_leaving: None,
            limits: WasmtimeStoreLimitsBuilder::new()
                .memory_size(WASM_STORE_MEMORY_CAP_BYTES)
                .build(),
        }
    }

    #[test]
    fn cached_host_templates_keep_store_imports_and_caller_state_fresh() {
        let engine = shared_wasm_engine().unwrap();
        // (module (import "fresh" "memory" (memory 1)))
        let bytes = b"\0asm\x01\0\0\0\x02\x11\x01\x05fresh\x06memory\x02\0\x01";
        let module = compile_wasm_module(&engine, bytes).unwrap();
        let key = wasm_module_memory_cache_key(
            WasmModuleSource::Program(bytes),
            WasmNativeCompilationMode::Fast,
        );
        let (mut first, _) = linker(&engine, &module, key).unwrap();
        let mut first_store = WasmtimeStore::new(&engine, state(true));
        first_store.set_epoch_deadline(u64::MAX / 2);
        let memory =
            WasmtimeMemory::new(&mut first_store, wasmtime::MemoryType::new(1, None)).unwrap();
        first
            .define(&first_store, "fresh", "memory", memory)
            .unwrap();
        first.instantiate(&mut first_store, &module).unwrap();
        let (mut second, outcome) = linker(&engine, &module, key).unwrap();
        assert_eq!(outcome, WasmRuntimeHostImportCacheOutcome::Hit);
        let mut second_store = WasmtimeStore::new(&engine, state(false));
        second_store.set_epoch_deadline(u64::MAX / 2);
        assert!(second.get(&mut second_store, "fresh", "memory").is_err());
        assert!(second.instantiate(&mut second_store, &module).is_err());
        for (linker, store, expected) in [
            (&first, &mut first_store, 1),
            (&second, &mut second_store, 0),
        ] {
            let function = linker
                .get(
                    &mut *store,
                    WASM_HOST_IMPORT_NAMESPACE,
                    WASM_HOST_IMPORT_AGENT_CAN_SUSPEND,
                )
                .unwrap()
                .into_func()
                .unwrap();
            assert_eq!(
                function
                    .typed::<(), i32>(&*store)
                    .unwrap()
                    .call(&mut *store, ())
                    .unwrap(),
                expected
            );
        }
        let memory =
            WasmtimeMemory::new(&mut second_store, wasmtime::MemoryType::new(1, None)).unwrap();
        second
            .define(&second_store, "fresh", "memory", memory)
            .unwrap();
        second.instantiate(&mut second_store, &module).unwrap();
    }

    #[test]
    fn template_reuse_requires_the_actual_destination_engine() {
        let mode = WasmNativeCompilationMode::Fast;
        let first = product_wasmtime_engine(mode).unwrap();
        let second = product_wasmtime_engine(mode).unwrap();
        let bytes = b"\0asm\x01\0\0\0\0\x09\x08template";
        let key = wasm_module_memory_cache_key(WasmModuleSource::Program(bytes), mode);
        let first_module = compile_wasm_module(&first, bytes).unwrap();
        let second_module = compile_wasm_module(&second, bytes).unwrap();
        let (a, _) = linker(&first, &first_module, key).unwrap();
        let (b, outcome) = linker(&second, &second_module, key).unwrap();
        assert_eq!(outcome, WasmRuntimeHostImportCacheOutcome::Miss);
        assert!(WasmtimeEngine::same(a.engine(), &first));
        assert!(WasmtimeEngine::same(b.engine(), &second));
        assert!(linker(&first, &second_module, key).is_err());
    }
}
