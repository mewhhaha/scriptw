//! Store-independent host import registration. Templates retain only host
//! callbacks and Engine-canonical types. Private/shared memory, runtime instances,
//! native-source contexts and all mutable JavaScript state are added to clones.
use super::*;
use std::collections::HashSet;

struct Template {
    key: WasmModuleMemoryCacheKey,
    additional_names: Box<[Box<str>]>,
    linker: WasmtimeLinker<WasmHostState>,
}

impl Template {
    fn matches<'name>(
        &self,
        engine: &WasmtimeEngine,
        key: WasmModuleMemoryCacheKey,
        additional_names: impl Iterator<Item = &'name str>,
    ) -> bool {
        self.key == key
            && WasmtimeEngine::same(self.linker.engine(), engine)
            && self
                .additional_names
                .iter()
                .map(Box::as_ref)
                .eq(additional_names)
    }
}

/// Bounded independently of native module retention: a template contains no
/// Module or artifact bytes, and can therefore outlive an evicted code image.
/// Exact Engine ownership is checked even for identical host bytes/configuration.
const RETAINED_TEMPLATES: usize = 2;

#[derive(Default)]
struct Templates {
    ready: VecDeque<Template>,
    // A first use goes directly to execution. Retain only weak Engine
    // identities here so one-shot use leaves no host functions or Engine.
    seen: [Option<(WasmModuleMemoryCacheKey, wasmtime::EngineWeak)>; RETAINED_TEMPLATES],
}

pub(super) fn linker<'module>(
    engine: &WasmtimeEngine,
    module: &'module WasmtimeModule,
    additional_names: impl Iterator<Item = &'module str> + Clone,
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
    static TEMPLATES: OnceLock<Mutex<Templates>> = OnceLock::new();
    let templates = TEMPLATES.get_or_init(Default::default);
    let seen_before = {
        let mut templates = templates
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(position) = templates
            .ready
            .iter()
            .position(|entry| entry.matches(engine, key, additional_names.clone()))
        {
            let entry = templates
                .ready
                .remove(position)
                .expect("template key exists");
            let linker = entry.linker.clone();
            templates.ready.push_back(entry);
            return Ok((linker, WasmRuntimeHostImportCacheOutcome::Hit));
        }
        let seen = &mut templates.seen;
        let seen_before = seen.iter().flatten().any(|(seen_key, seen_engine)| {
            *seen_key == key
                && seen_engine
                    .upgrade()
                    .is_some_and(|seen_engine| WasmtimeEngine::same(&seen_engine, engine))
        });
        if !seen_before {
            seen.rotate_right(1);
            seen[0] = Some((key, engine.weak()));
        }
        seen_before
    };
    // R owns the concrete callback signatures. P can additionally request
    // fixed-signature callbacks, so a retained template includes its selection.
    // Validation and host trampoline construction do not hold the cache mutex.
    let linker = build(engine, module, additional_names.clone())?;
    if !seen_before {
        return Ok((linker, WasmRuntimeHostImportCacheOutcome::Miss));
    }
    let mut templates = templates
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let templates = &mut templates.ready;
    if let Some(position) = templates
        .iter()
        .position(|entry| entry.matches(engine, key, additional_names.clone()))
    {
        let entry = templates.remove(position).expect("template key exists");
        let linker = entry.linker.clone();
        templates.push_back(entry);
        return Ok((linker, WasmRuntimeHostImportCacheOutcome::Miss));
    }
    if templates.len() == RETAINED_TEMPLATES {
        templates.pop_front();
    }
    templates.push_back(Template {
        key,
        additional_names: additional_names.map(Box::from).collect(),
        linker: linker.clone(),
    });
    Ok((linker, WasmRuntimeHostImportCacheOutcome::Miss))
}

pub(super) fn build<'module>(
    engine: &WasmtimeEngine,
    module: &'module WasmtimeModule,
    additional_names: impl Iterator<Item = &'module str>,
) -> Result<WasmtimeLinker<WasmHostState>, EngineError> {
    let mut linker = WasmtimeLinker::new(engine);
    register(&mut linker, module, additional_names)?;
    Ok(linker)
}

/// The general execution path can reuse an import-free instance with any
/// caller linker. Prepared instances with imports use the embedding path's
/// shared linker identity check.
pub(super) fn instantiate(
    linker: &WasmtimeLinker<WasmHostState>,
    store: &mut WasmtimeStore<WasmHostState>,
    module: &WasmtimeModule,
    import_free: Option<&OnceLock<wasmtime::InstancePre<WasmHostState>>>,
) -> wasmtime::Result<wasmtime::Instance> {
    let Some(slot) = import_free else {
        return linker.instantiate(store, module);
    };
    if let Some(prepared) = slot.get() {
        return prepared.instantiate(store);
    }
    let prepared = linker.instantiate_pre(module)?;
    slot.get_or_init(|| prepared).instantiate(store)
}

/// Prepare the host module using only the immutable callback table. The one
/// shared memory import, if present, is supplied by each registered Store.
pub(super) fn prepare(
    linker: &WasmtimeLinker<WasmHostState>,
    module: &WasmtimeModule,
    memories: &crate::host_memory_imports::HostMemoryImports,
) -> wasmtime::Result<wasmtime::InstancePre<WasmHostState>> {
    if memories.shared().is_some() {
        linker.instantiate_pre_with_shared_memory(
            module,
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_SHARED_MEMORY,
        )
    } else {
        linker.instantiate_pre(module)
    }
}

/// Register the same checked callbacks on an embedder-owned linker. This adds
/// no memory, runtime instance, foreign implementation or source compiler.
pub(super) fn register<'module>(
    linker: &mut WasmtimeLinker<WasmHostState>,
    module: &'module WasmtimeModule,
    additional_names: impl Iterator<Item = &'module str>,
) -> Result<(), EngineError> {
    let mut names = HashSet::new();
    let mut record_name = |name: &'module str| {
        names.try_reserve(1).map_err(|error| {
            EngineError::new(format!("wasmtime host import selection failed: {error}"))
        })?;
        names.insert(name);
        Ok(())
    };
    let scanned = wasm_gc_host::ScannedGcHostImports::scan(module, &mut record_name)?;
    for name in additional_names {
        record_name(name)?;
    }
    register_selected(linker, Some(module), &names, scanned)
}

/// Resolve a fixed callback first requested by a newly admitted native program.
/// Typed callbacks and Store resources still come from the original linker.
#[cfg(feature = "native-source-compiler")]
pub(super) fn resolve_fixed(
    mut store: impl wasmtime::AsContextMut<Data = WasmHostState>,
    name: &str,
) -> wasmtime::Result<WasmtimeExtern> {
    let mut linker = WasmtimeLinker::new(store.as_context().engine());
    register_selected(
        &mut linker,
        None,
        &HashSet::from([name]),
        Default::default(),
    )
    .map_err(wasmtime::Error::new)?;
    linker.get(&mut store, WASM_HOST_IMPORT_NAMESPACE, name)
}

/// Fixed callbacks use our own signatures. The selection of names never
/// derives a signature from the importing module.
pub(super) struct FixedHostFunctions<'linker, 'module> {
    pub(super) linker: &'linker mut WasmtimeLinker<WasmHostState>,
    names: Option<&'linker HashSet<&'module str>>,
}

impl FixedHostFunctions<'_, '_> {
    pub(super) fn includes(&self, name: &str) -> bool {
        self.names.is_none_or(|names| names.contains(name))
    }

    pub(super) fn func_wrap<Params, Results>(
        &mut self,
        namespace: &str,
        name: &str,
        func: impl wasmtime::IntoFunc<WasmHostState, Params, Results>,
    ) -> wasmtime::Result<()> {
        debug_assert_eq!(namespace, WASM_HOST_IMPORT_NAMESPACE);
        if self.includes(name) {
            self.linker.func_wrap(namespace, name, func)?;
        }
        Ok(())
    }
}

#[cfg(test)]
impl<'linker> FixedHostFunctions<'linker, '_> {
    pub(super) fn all(linker: &'linker mut WasmtimeLinker<WasmHostState>) -> Self {
        Self {
            linker,
            names: None,
        }
    }
}

fn register_selected(
    linker: &mut WasmtimeLinker<WasmHostState>,
    print_caller: Option<&WasmtimeModule>,
    names: &HashSet<&str>,
    scanned: wasm_gc_host::ScannedGcHostImports<'_>,
) -> Result<(), EngineError> {
    let mut linker = FixedHostFunctions {
        linker,
        names: Some(names),
    };
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
    wasm_gc_host::link_scanned(&mut linker, scanned)?;
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
    // The host module's instances are print's callers: resolve their memory
    // export's index once instead of looking it up by name per line. Any
    // other module's caller (the index names another module) keeps the name
    // lookup, which yields the same export.
    let memory_export = linker
        .includes(WASM_HOST_IMPORT_PRINT_LINE_UTF8)
        .then(|| print_caller.and_then(|module| module.get_export_index("memory")))
        .flatten();
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_PRINT_LINE_UTF8,
            move |mut caller: WasmtimeCaller<'_, WasmHostState>,
                  ptr: i32,
                  len: i32|
                  -> wasmtime::Result<()> {
                let memory = memory_export
                    .as_ref()
                    .and_then(|export| caller.get_module_export(export))
                    .or_else(|| caller.get_export("memory"))
                    .ok_or_else(|| {
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
                let print = |bytes: &[u8], state: &WasmHostState| -> wasmtime::Result<()> {
                    let text = std::str::from_utf8(bytes).map_err(|err| {
                        wasmtime::Error::msg(format!(
                            "wasmtime host import failed: invalid utf-8: {err}"
                        ))
                    })?;
                    state.output_events.record(text);
                    state.realm.host_hooks().print_line(text);
                    Ok(())
                };
                match memory {
                    // Private memory cannot change while this import runs:
                    // decode the validated span in place.
                    WasmtimeExtern::Memory(memory) => {
                        print(&memory.data(&caller)[span], caller.data())
                    }
                    WasmtimeExtern::SharedMemory(memory) => {
                        let mut bytes = vec![0; span.len()];
                        read_wasmtime_shared_memory(&memory, span.start, &mut bytes).map_err(
                            |err| {
                                wasmtime::Error::msg(format!(
                                    "wasmtime host import failed: unable to read memory: {err}"
                                ))
                            },
                        )?;
                        print(&bytes, caller.data())
                    }
                    _ => unreachable!("memory kind was validated before allocation"),
                }
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
        let additional_names = || [WASM_HOST_IMPORT_AGENT_CAN_SUSPEND].into_iter();
        let (mut first, outcome) = linker(&engine, &module, additional_names(), key).unwrap();
        assert_eq!(outcome, WasmRuntimeHostImportCacheOutcome::Miss);
        assert!(!first.has_definition(WASM_HOST_IMPORT_NAMESPACE, WASM_HOST_IMPORT_AGENT_CALL));
        let mut first_store = WasmtimeStore::new(&engine, state(true));
        let memory =
            WasmtimeMemory::new(&mut first_store, wasmtime::MemoryType::new(1, None)).unwrap();
        first
            .define(&first_store, "fresh", "memory", memory)
            .unwrap();
        first.instantiate(&mut first_store, &module).unwrap();
        let (_, outcome) = linker(&engine, &module, additional_names(), key).unwrap();
        assert_eq!(outcome, WasmRuntimeHostImportCacheOutcome::Miss);
        let (mut second, outcome) = linker(&engine, &module, additional_names(), key).unwrap();
        assert_eq!(outcome, WasmRuntimeHostImportCacheOutcome::Hit);
        assert!(!second.has_definition(WASM_HOST_IMPORT_NAMESPACE, WASM_HOST_IMPORT_AGENT_CALL));
        let (without_extra, outcome) = linker(&engine, &module, std::iter::empty(), key).unwrap();
        assert_eq!(outcome, WasmRuntimeHostImportCacheOutcome::Miss);
        assert!(!without_extra.has_definition(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_AGENT_CAN_SUSPEND
        ));
        let mut second_store = WasmtimeStore::new(&engine, state(false));
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
        let _ = linker(&first, &first_module, std::iter::empty(), key).unwrap();
        let (a, _) = linker(&first, &first_module, std::iter::empty(), key).unwrap();
        let (b, outcome) = linker(&second, &second_module, std::iter::empty(), key).unwrap();
        assert_eq!(outcome, WasmRuntimeHostImportCacheOutcome::Miss);
        assert!(WasmtimeEngine::same(a.engine(), &first));
        assert!(WasmtimeEngine::same(b.engine(), &second));
        assert!(linker(&first, &second_module, std::iter::empty(), key).is_err());
    }

    /// `(module (import "scriptw_host" "print_line_utf8" (func (param i32 i32)))
    /// (memory (export "memory") 1) (data (i32.const 0) text)
    /// (func (export "run") (call 0 (i32.const 0) (i32.const len))))`.
    fn print_module(text: &[u8]) -> Vec<u8> {
        fn section(id: u8, payload: &[u8]) -> Vec<u8> {
            let mut bytes = vec![id, u8::try_from(payload.len()).unwrap()];
            bytes.extend_from_slice(payload);
            bytes
        }
        fn name(text: &str) -> Vec<u8> {
            let mut bytes = vec![u8::try_from(text.len()).unwrap()];
            bytes.extend_from_slice(text.as_bytes());
            bytes
        }
        let len = u8::try_from(text.len()).unwrap();
        assert!(len < 0x40, "single-byte signed LEB length");
        let mut module = b"\0asm\x01\0\0\0".to_vec();
        module.extend(section(1, &[2, 0x60, 2, 0x7f, 0x7f, 0, 0x60, 0, 0]));
        let mut import = vec![1];
        import.extend(name(WASM_HOST_IMPORT_NAMESPACE));
        import.extend(name(WASM_HOST_IMPORT_PRINT_LINE_UTF8));
        import.extend([0, 0]);
        module.extend(section(2, &import));
        module.extend(section(3, &[1, 1]));
        module.extend(section(5, &[1, 0, 1]));
        let mut export = vec![2];
        export.extend(name("memory"));
        export.extend([2, 0]);
        export.extend(name("run"));
        export.extend([0, 1]);
        module.extend(section(7, &export));
        let body = [0, 0x41, 0, 0x41, len, 0x10, 0, 0x0b];
        let mut code = vec![1, u8::try_from(body.len()).unwrap()];
        code.extend(body);
        module.extend(section(10, &code));
        let mut data = vec![1, 0, 0x41, 0, 0x0b, len];
        data.extend_from_slice(text);
        module.extend(section(11, &data));
        module
    }

    #[test]
    fn print_reads_the_host_module_memory_by_index_and_other_callers_by_name() {
        let engine = shared_wasm_engine().unwrap();
        let host = compile_wasm_module(&engine, &print_module(b"host line")).unwrap();
        let other = compile_wasm_module(&engine, &print_module(b"other line")).unwrap();
        assert!(host.get_export_index("memory").is_some());
        let mut linker = WasmtimeLinker::new(&engine);
        register(&mut linker, &host, std::iter::empty()).unwrap();
        assert!(!linker.has_definition(WASM_HOST_IMPORT_NAMESPACE, WASM_HOST_IMPORT_AGENT_CALL));
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut state = state(false);
        state.output_events = WasmOutputEvents::Capture(Arc::clone(&events));
        let mut store = WasmtimeStore::new(&engine, state);
        // The host module's callers use the index resolved at registration;
        // another module's index lookup misses and falls back to its name.
        for module in [&host, &other, &host] {
            let instance = linker.instantiate(&mut store, module).unwrap();
            instance
                .get_typed_func::<(), ()>(&mut store, "run")
                .unwrap()
                .call(&mut store, ())
                .unwrap();
        }
        assert_eq!(
            *events.lock().unwrap(),
            [
                HostOutputEvent::PrintLine("host line".into()),
                HostOutputEvent::PrintLine("other line".into()),
                HostOutputEvent::PrintLine("host line".into()),
            ]
        );
    }
}
