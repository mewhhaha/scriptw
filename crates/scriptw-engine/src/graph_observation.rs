//! Opt-in graph execution uses its own artifact identity. Legacy structured
//! observations and product artifacts never acquire the snapshot root exports.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRunOutcome {
    pub backend_used: ExecutionBackend,
    pub completion: SnapshotCompletion,
    pub output_events: Vec<HostOutputEvent>,
    pub note: String,
}

impl Engine {
    pub fn observe_script_graph(
        &self,
        source: &str,
        options: CompileOptions,
        run: RunOptions,
        limits: SnapshotLimits,
    ) -> Result<GraphRunOutcome, EngineError> {
        self.observe_source_graph(source, ParseGoal::Script, options, run, limits)
    }
    pub fn observe_module_graph(
        &self,
        source: &str,
        options: CompileOptions,
        run: RunOptions,
        limits: SnapshotLimits,
    ) -> Result<GraphRunOutcome, EngineError> {
        self.observe_source_graph(source, ParseGoal::Module, options, run, limits)
    }
    fn observe_source_graph(
        &self,
        source: &str,
        goal: ParseGoal,
        options: CompileOptions,
        run: RunOptions,
        limits: SnapshotLimits,
    ) -> Result<GraphRunOutcome, EngineError> {
        validate_execution_intl_profile(&options.intl_profile, run.backend)?;
        match run.backend {
            ExecutionBackend::WasmAot => {
                run_on_sized_stack(|| self.observe_graph_wasm(source, goal, options, run, limits))
            }
            ExecutionBackend::SpecExec => {
                self.observe_graph_spec_exec(source, goal, options, run, limits)
            }
        }
    }
    fn observe_graph_wasm(
        &self,
        source: &str,
        goal: ParseGoal,
        options: CompileOptions,
        run: RunOptions,
        limits: SnapshotLimits,
    ) -> Result<GraphRunOutcome, EngineError> {
        let cache = program_wasm_cache();
        let prepared = self.prepare_compilation(source, goal, &options)?;
        let base = cache
            .as_ref()
            .and_then(|_| program_cache_key(source, goal, &options, prepared.modules.graph()));
        let key = base.map(graph_artifact_key);
        // Snapshot artifacts link against a snapshot runtime that the program
        // cache cannot name, so only standalone entries are cached or reused.
        let cached = cache.as_ref().zip(key).and_then(|(cache, key)| {
            let entry = cache.read(&key)?;
            let bytes = wasm_runtime_link::decode_standalone_cache_entry(&entry)?;
            Some(ProgramWasmArtifact {
                bytes: Arc::from(bytes),
                runtime: None,
                compilation_policy: WasmCompilationPolicy::from_options(&options),
                cached_entry: Some(ProgramWasmCacheEntry {
                    cache: Arc::clone(cache),
                    key,
                }),
            })
        });
        let artifact = if let Some(artifact) = cached {
            artifact
        } else {
            self.compile_graph_artifact(prepared, cache.clone(), key)?
        };
        let result = self.execute_with_wasm_bytes_inner(
            artifact.program(),
            run.timeout_ms,
            run.can_block,
            WasmModuleMemoryCachePolicy::Retain,
            &WasmExecutionMode::Graph(limits),
        );
        let result = if let Err(error) = &result {
            if artifact.evict_if_invalid(error) {
                let prepared = self.prepare_compilation(source, goal, &options)?;
                let artifact = self.compile_graph_artifact(prepared, cache, key)?;
                self.execute_with_wasm_bytes_inner(
                    artifact.program(),
                    run.timeout_ms,
                    run.can_block,
                    WasmModuleMemoryCachePolicy::Retain,
                    &WasmExecutionMode::Graph(limits),
                )
            } else {
                result
            }
        } else {
            result
        };
        match result? {
            WasmExecutionOutcome::Graph(outcome) => Ok(outcome),
            WasmExecutionOutcome::Legacy(_) | WasmExecutionOutcome::Structured(_) => {
                Err(EngineError::new("internal graph execution mode mismatch"))
            }
        }
    }
    fn compile_graph_artifact(
        &self,
        prepared: PreparedCompilation,
        cache: Option<Arc<cache::FunctionCache>>,
        key: Option<[u8; 32]>,
    ) -> Result<ProgramWasmArtifact, EngineError> {
        let unit = self.compile_prepared_on_current_thread(prepared)?;
        let runtime_cache = cache
            .as_deref()
            .map(wasm_runtime_link::RuntimeWasmCache::new);
        let artifact = scriptw_aot_wasm::emit_with_rooted_snapshot_and_runtime_inputs(
            &unit.ir,
            unit.promise_rejection_policy,
            &unit.intl_profile,
            embedded_runtime::inputs(
                runtime_cache
                    .as_ref()
                    .map(|cache| cache as &dyn scriptw_aot_wasm::RuntimeArtifactCache),
                scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost,
            ),
        )
        .map_err(|error| EngineError::from_wasm_emit_error(&unit.ir, error))?;
        let runtime = artifact.runtime().cloned();
        if let (Some(cache), Some(key), None) = (cache, key, &runtime) {
            cache.write(
                &key,
                wasm_runtime_link::encode_cache_entry(WasmProgramRef::standalone(&artifact.bytes)),
            );
        }
        Ok(ProgramWasmArtifact {
            bytes: Arc::from(artifact.bytes),
            runtime,
            cached_entry: None,
            compilation_policy: WasmCompilationPolicy::from_unit(&unit),
        })
    }
    #[cfg(feature = "spec-exec-oracle")]
    fn observe_graph_spec_exec(
        &self,
        source: &str,
        goal: ParseGoal,
        options: CompileOptions,
        run: RunOptions,
        limits: SnapshotLimits,
    ) -> Result<GraphRunOutcome, EngineError> {
        let host_hooks = self.realm.host_hooks();
        run_on_sized_stack(move || {
            let outcome = match goal {
                ParseGoal::Script => scriptw_spec_exec::observe_script_graph_with_prelude(
                    source,
                    options.filename.as_deref(),
                    options.module_loading_policy,
                    options.module_prelude.as_deref(),
                    &run.argv,
                    run.can_block,
                    host_hooks,
                    limits,
                ),
                ParseGoal::Module => scriptw_spec_exec::observe_module_graph(
                    source,
                    options.filename.as_deref(),
                    scriptw_spec_exec::ModuleHostConfig {
                        module_root: run.module_root.clone().map(Into::into),
                        test_path: run.test_path.clone().map(Into::into),
                        module_loading_policy: options.module_loading_policy,
                        prelude: options.module_prelude,
                    },
                    &run.argv,
                    run.can_block,
                    host_hooks,
                    limits,
                ),
            }
            .map_err(EngineError::from_spec_exec)?;
            Ok(GraphRunOutcome {
                backend_used: ExecutionBackend::SpecExec,
                completion: outcome.completion,
                output_events: outcome.output_events,
                note: outcome.note,
            })
        })
    }
    #[cfg(not(feature = "spec-exec-oracle"))]
    fn observe_graph_spec_exec(
        &self,
        _source: &str,
        _goal: ParseGoal,
        _options: CompileOptions,
        _run: RunOptions,
        _limits: SnapshotLimits,
    ) -> Result<GraphRunOutcome, EngineError> {
        Err(EngineError::new(
            "rooted spec-exec observations require the developer-only spec-exec-oracle feature",
        ))
    }
}

fn graph_artifact_key(base: [u8; 32]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"scriptw-rooted-snapshot-artifact-v1");
    hash.update(base);
    hash.finalize().into()
}
