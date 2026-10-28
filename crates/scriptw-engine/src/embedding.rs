//! Real compiled P/R in an embedder-owned Wasmtime Engine, Store and Linker.
//!
//! This Portable boundary retains no source compiler. Declared foreign
//! implementations live in the embedder's linker; mutable application data,
//! retained JavaScript roots and pending resolvers belong to its actual Store.
use super::*;
use scriptw_ir::foreign::ForeignModuleRegistry;
use std::any::Any;
use wasmtime::{AsContext, AsContextMut, ExnRef, ExnRefPre, ExnType, FuncType, Instance, ValType};

#[cfg(feature = "benchmark-module-images")]
mod benchmark_module_images;
#[cfg(feature = "benchmark-module-images")]
pub use benchmark_module_images::{
    BenchmarkModuleImage, BenchmarkModuleImages, BenchmarkModuleSection, BenchmarkModuleSections,
};

mod async_host;
mod foreign_host;
mod operations;
mod precompiled;
mod values;
use async_host::ForeignAsyncState;
pub use async_host::ForeignCompleter;
pub use foreign_host::ForeignValue;
pub use precompiled::{configure_precompiled_wasm_runtime, PrecompiledEmbeddedProgram};
use values::{embedded_error, retained_completion};
pub use values::{EmbeddedEvaluation, JsCompletion, JsPromiseState, JsValue, JsValueKind};

#[cfg(test)]
mod decimal_helper_tests;
#[cfg(test)]
mod tests;

/// Opaque Scriptw host state with one independently owned application value.
///
/// Create this with [`EmbeddedRuntime::new_state`], then put it into the
/// embedder's own `wasmtime::Store`. Do not replace/move the state out of a
/// live Store: Wasmtime resources and the registered exception tag belong to
/// that Store. No state, Store, JavaScript Realm or foreign implementation is
/// pooled by this API.
pub struct WasmStoreState {
    pub(super) weak: wasm_gc_weak_host::WeakGcState,
    pub(super) native_source_compiler:
        Option<Arc<wasm_native_source_compiler::NativeSourceCompilerContext>>,
    pub(super) native_template_instances: memory_module_cache::MemoryNativeInstanceCache,
    pub(super) realm: Realm,
    pub(super) output_events: WasmOutputEvents,
    pub(super) intl_kernel: intl_data_images::WasmIntlKernel,
    pub(super) can_block: bool,
    pub(super) monotonic_clock_origin: MonotonicClockInstant,
    pub(super) shared_memory_backing: Option<Arc<WasmSharedMemoryBacking>>,
    pub(super) async_waiters: Arc<WasmStoreAsyncWaiters>,
    pub(super) agent_group: Option<Arc<WasmAgentGroup>>,
    pub(super) agent_commands: Option<Arc<Mutex<std::sync::mpsc::Receiver<WasmAgentCommand>>>>,
    pub(super) agent_leaving: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub(super) limits: WasmtimeStoreLimits,
    pub(super) user_data: Option<Box<dyn Any + Send>>,
    pub(super) embedding: Option<StoreEmbeddingState>,
}

impl WasmStoreState {
    /// Retrieve only this Store's application data, with checked Rust typing.
    pub fn user_data<T: Any>(&self) -> Option<&T> {
        self.user_data.as_ref()?.downcast_ref()
    }

    pub fn user_data_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.user_data.as_mut()?.downcast_mut()
    }

    /// Optional standard Wasmtime limiter storage. The embedder chooses
    /// whether to attach it using `store.limiter`, or supplies its own limiter.
    pub fn limits_mut(&mut self) -> &mut wasmtime::StoreLimits {
        &mut self.limits
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StorePhase {
    Fresh,
    Registered,
    Instantiated,
    Evaluated,
    Failed,
}

pub(super) struct StoreEmbeddingState {
    module: Arc<()>,
    store: Arc<()>,
    phase: StorePhase,
    error_tag: Option<wasmtime::Tag>,
    async_host: ForeignAsyncState,
}

/// Initialize Scriptw's required Wasmtime configuration. Apply any embedder
/// compilation, fuel or interruption settings after this call. This sets the
/// normal Fast/copying-GC baseline, including epoch checks; callers own Store
/// deadlines and any engine epoch ticker. No deadline is changed by evaluation.
pub fn configure_wasm_runtime(config: &mut wasmtime::Config) {
    *config = wasmtime_config::base_config(
        WasmNativeCompilationMode::Fast,
        wasmtime::WasmBacktraceDetails::Environment,
    );
    config.parallel_compilation(false);
}

/// Immutable, admitted program and its matching runtime for one actual Engine.
/// Different applications may share the same R; their foreign functions remain
/// P imports and are resolved separately in each caller-owned Linker/Store.
#[cfg_attr(
    not(feature = "benchmark-module-images"),
    doc = "
The default product API has no benchmark module-image/section hook:
```compile_fail,E0432
use scriptw_engine::{BenchmarkModuleImage, BenchmarkModuleImages, BenchmarkModuleSection, BenchmarkModuleSections};
```
"
)]
pub struct EmbeddedRuntime {
    engine: WasmtimeEngine,
    modules: wasm_runtime_link::WasmModules,
    foreign: ForeignModuleRegistry,
    intl: intl_data_images::WasmIntlKernel,
    authority: Arc<()>,
}

impl EmbeddedRuntime {
    /// Admit persisted requirements and compile actual Wasm, without parsing JS,
    /// resolving source files or installing a Rust foreign-closure registry.
    pub fn compile(engine: &wasmtime::Engine, artifact: &Artifact) -> Result<Self, EngineError> {
        run_on_sized_stack(|| {
            let foreign = artifact.foreign_modules()?;
            PRODUCT_WASMTIME_POLICY
                .verify_engine(engine)
                .map_err(EngineError::new)?;
            if scriptw_aot_wasm::RuntimeSourceCompilationProfile::from_module(&artifact.bytes)
                .map_err(|error| EngineError::new(error.to_string()))?
                != scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable
            {
                return Err(EngineError::new(
                    "embedding requires Portable Wasm; native source compilation is not an embedding capability",
                ));
            }
            let mode = match engine.get_cranelift_opt_level() {
                Some(wasmtime::OptLevel::None) => WasmNativeCompilationMode::Fast,
                Some(wasmtime::OptLevel::Speed) => WasmNativeCompilationMode::Speed,
                Some(wasmtime::OptLevel::SpeedAndSize) => WasmNativeCompilationMode::SizeOptimized,
                Some(other) => {
                    return Err(EngineError::new(format!(
                        "unsupported embedding optimization level: {other:?}"
                    )));
                }
                None => {
                    return Err(EngineError::new(
                        "embedding requires a Wasmtime native compiler",
                    ));
                }
            };
            // P/R must be inert until explicit embedder instantiation. Generated
            // modules have no start function; external bytes cannot add one.
            for bytes in [
                Some(artifact.bytes.as_slice()),
                artifact.runtime.as_ref().map(LinkedRuntime::bytes),
            ]
            .into_iter()
            .flatten()
            {
                for payload in WasmParser::new(0).parse_all(bytes) {
                    if matches!(
                        payload.map_err(|error| EngineError::new(error.to_string()))?,
                        WasmPayload::StartSection { .. }
                    ) {
                        return Err(EngineError::new(
                            "embedding rejects implicit Wasm start functions",
                        ));
                    }
                }
            }
            let program = artifact.program();
            let intl = wasm_runtime_link::intl_kernel_for(program)?;
            let modules = wasm_runtime_link::compile_modules(
                engine,
                program,
                WasmModuleMemoryCachePolicy::BypassRetention,
                true,
                mode,
                WasmRuntimeImagePolicy::EmbeddedBuild,
            )?;
            Ok(Self::from_admitted_modules(engine, modules, foreign, intl))
        })
    }

    fn from_admitted_modules(
        engine: &WasmtimeEngine,
        modules: wasm_runtime_link::WasmModules,
        foreign: ForeignModuleRegistry,
        intl: intl_data_images::WasmIntlKernel,
    ) -> Self {
        Self {
            engine: engine.clone(),
            modules,
            foreign,
            intl,
            authority: Arc::new(()),
        }
    }

    /// Construct one fresh host state. The caller creates and retains the Store
    /// and chooses its resource limiter, fuel and interruption policy.
    pub fn new_state<T: Any + Send>(
        &self,
        realm: Realm,
        user_data: T,
    ) -> Result<WasmStoreState, EngineError> {
        if self
            .intl
            .identity()
            .is_some_and(|identity| !realm.system_time_zone().is_compatible_with(identity))
        {
            return Err(EngineError::new(
                "configured system time zone differs from embedded Intl data",
            ));
        }
        let shared = self
            .modules
            .host()
            .imports()
            .find_map(|import| {
                (import.module() == WASM_HOST_IMPORT_NAMESPACE
                    && import.name() == WASM_HOST_IMPORT_SHARED_MEMORY)
                    .then(|| import.ty())
                    .and_then(|ty| match ty {
                        WasmtimeExternType::Memory(ty) => Some(ty),
                        _ => None,
                    })
            })
            .map(|ty| {
                WasmtimeSharedMemory::new(&self.engine, ty)
                    .map(WasmSharedMemoryBacking::new)
                    .map_err(|error| {
                        EngineError::new(format!("embedding shared memory: {error:#}"))
                    })
            })
            .transpose()?;
        Ok(WasmStoreState {
            weak: Default::default(),
            native_source_compiler: None,
            native_template_instances: Default::default(),
            monotonic_clock_origin: realm.host_clock().monotonic_instant(),
            realm,
            output_events: WasmOutputEvents::for_mode(&WasmExecutionMode::Structured),
            intl_kernel: self.intl.clone(),
            can_block: false,
            async_waiters: WasmStoreAsyncWaiters::new(shared.as_ref().map(Arc::clone)),
            shared_memory_backing: shared,
            agent_group: None,
            agent_commands: None,
            agent_leaving: None,
            limits: WasmtimeStoreLimitsBuilder::new()
                .memory_size(WASM_STORE_MEMORY_CAP_BYTES)
                .build(),
            user_data: Some(Box::new(user_data)),
            embedding: Some(StoreEmbeddingState {
                module: Arc::clone(&self.authority),
                store: Arc::new(()),
                phase: StorePhase::Fresh,
                error_tag: None,
                async_host: ForeignAsyncState::new(),
            }),
        })
    }

    fn check(
        &self,
        store: &WasmtimeStore<WasmStoreState>,
        linker: &WasmtimeLinker<WasmStoreState>,
        phase: StorePhase,
    ) -> Result<(), EngineError> {
        if !WasmtimeEngine::same(store.engine(), &self.engine)
            || !WasmtimeEngine::same(linker.engine(), &self.engine)
        {
            return Err(EngineError::new(
                "embedding Store/Linker belongs to a different actual Engine",
            ));
        }
        let state = store
            .data()
            .embedding
            .as_ref()
            .ok_or_else(|| EngineError::new("Store has no admitted embedding state"))?;
        if !Arc::ptr_eq(&state.module, &self.authority) || state.phase != phase {
            return Err(EngineError::new(
                "embedding program/Store authority or lifecycle differs",
            ));
        }
        Ok(())
    }

    /// Add Scriptw's checked host callbacks and private per-Store resources to
    /// an ordinary caller-owned Linker. Foreign `func_wrap` entries are supplied
    /// independently by the embedder. This never changes Store limits/deadlines.
    pub fn register(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
        linker: &mut WasmtimeLinker<WasmStoreState>,
    ) -> Result<(), EngineError> {
        self.check(store, linker, StorePhase::Fresh)?;
        store.data_mut().embedding.as_mut().unwrap().phase = StorePhase::Failed;
        host_imports::register(linker, self.modules.host())?;
        for import in self.modules.host().imports() {
            if import.module() == WASM_HOST_IMPORT_NAMESPACE
                && import.name() == WASM_HOST_IMPORT_PRIVATE_MEMORY
            {
                let WasmtimeExternType::Memory(ty) = import.ty() else {
                    return Err(EngineError::new(
                        "Scriptw private memory import is not a memory",
                    ));
                };
                let memory = WasmtimeMemory::new(&mut *store, ty).map_err(|error| {
                    EngineError::new(format!("embedding private memory: {error:#}"))
                })?;
                linker
                    .define(&*store, import.module(), import.name(), memory)
                    .map_err(|error| EngineError::new(error.to_string()))?;
            }
        }
        if let Some(memory) = store
            .data()
            .shared_memory_backing
            .as_ref()
            .map(|backing| backing.memory.clone())
        {
            linker
                .define(
                    &*store,
                    WASM_HOST_IMPORT_NAMESPACE,
                    WASM_HOST_IMPORT_SHARED_MEMORY,
                    memory,
                )
                .map_err(|error| EngineError::new(error.to_string()))?;
        }
        if let Some(declaration) = self.foreign.error_tag() {
            let exception = ExnType::new(&self.engine, [ValType::I32])
                .map_err(|error| EngineError::new(error.to_string()))?;
            let tag = wasmtime::Tag::new(&mut *store, &exception.tag_type())
                .map_err(|error| EngineError::new(error.to_string()))?;
            linker
                .define(&*store, declaration.module(), declaration.name(), tag)
                .map_err(|error| EngineError::new(error.to_string()))?;
            store.data_mut().embedding.as_mut().unwrap().error_tag = Some(tag);
        }
        store.data_mut().embedding.as_mut().unwrap().phase = StorePhase::Registered;
        Ok(())
    }

    /// Check actual foreign import values/types before instantiating R or P,
    /// then create a fresh instance pair in the caller's Store.
    pub fn instantiate(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
        linker: &mut WasmtimeLinker<WasmStoreState>,
    ) -> Result<EmbeddedInstance, EngineError> {
        run_on_sized_stack(|| self.instantiate_inner(store, linker))
    }

    fn instantiate_inner(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
        linker: &mut WasmtimeLinker<WasmStoreState>,
    ) -> Result<EmbeddedInstance, EngineError> {
        self.check(store, linker, StorePhase::Registered)?;
        store.data_mut().embedding.as_mut().unwrap().phase = StorePhase::Failed;
        for import in self.modules.program.imports() {
            if matches!(
                import.module(),
                WASM_HOST_IMPORT_NAMESPACE | scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE
            ) {
                continue;
            }
            let actual = linker
                .get(&mut *store, import.module(), import.name())
                .map_err(|error| {
                    EngineError::new(format!(
                        "missing declared foreign import {}.{}: {error:#}",
                        import.module(),
                        import.name()
                    ))
                })?;
            let matches = match (import.ty(), actual.ty(&*store)) {
                (WasmtimeExternType::Func(expected), WasmtimeExternType::Func(actual)) => {
                    FuncType::eq(&expected, &actual)
                }
                (WasmtimeExternType::Tag(expected), WasmtimeExternType::Tag(actual)) => {
                    FuncType::eq(expected.ty(), actual.ty())
                }
                _ => false,
            };
            if !matches {
                return Err(EngineError::new(format!(
                    "foreign import type mismatch: {}.{}",
                    import.module(),
                    import.name()
                )));
            }
            if let WasmtimeExtern::Tag(actual_tag) = actual {
                let expected_tag = store
                    .data()
                    .embedding
                    .as_ref()
                    .unwrap()
                    .error_tag
                    .ok_or_else(|| {
                        EngineError::new("foreign error tag has no registered Store authority")
                    })?;
                if !wasmtime::Tag::eq(&actual_tag, &expected_tag, &*store) {
                    return Err(EngineError::new(
                        "foreign error tag differs from the registered Store tag",
                    ));
                }
            }
        }
        let (program, roots) = wasm_runtime_link::instantiate(linker, store, &self.modules)?;
        let main = program
            .get_typed_func::<(), GcMainCompletion>(&mut *store, "main")
            .map_err(|error| EngineError::new(format!("embedding main signature: {error:#}")))?;
        let state = store.data_mut().embedding.as_mut().unwrap();
        state.phase = StorePhase::Instantiated;
        Ok(EmbeddedInstance {
            main,
            program,
            roots,
            store: Arc::clone(&state.store),
        })
    }
}

/// One P/R pair in its owning Store. Initialize once, then retain values and
/// invoke exports repeatedly without replaying module entry.
pub struct EmbeddedInstance {
    main: wasmtime::TypedFunc<(), GcMainCompletion>,
    program: Instance,
    roots: Instance,
    store: Arc<()>,
}

impl EmbeddedInstance {
    /// Initialize real compiled main once. Pending top-level await retains the
    /// instance; deliver host completions with `pump` until it settles.
    pub fn initialize(
        &mut self,
        store: &mut WasmtimeStore<WasmStoreState>,
    ) -> Result<EmbeddedEvaluation, EngineError> {
        run_on_sized_stack(|| {
            let state = store
                .data_mut()
                .embedding
                .as_mut()
                .ok_or_else(|| EngineError::new("Store has no embedding authority"))?;
            if !Arc::ptr_eq(&state.store, &self.store) || state.phase != StorePhase::Instantiated {
                return Err(EngineError::new(
                    "instance belongs to a different Store or was already evaluated",
                ));
            }
            state.phase = StorePhase::Evaluated;
            let mut roots = wasmtime::RootScope::new(store);
            let completion = match self.main.call(&mut roots, ()) {
                Ok(completion) => completion,
                Err(error) => {
                    roots
                        .as_context_mut()
                        .data_mut()
                        .embedding
                        .as_mut()
                        .unwrap()
                        .phase = StorePhase::Failed;
                    return Err(embedded_error(error));
                }
            };
            if global_script_prelude::failed(&self.program, &mut roots)? {
                roots
                    .as_context_mut()
                    .data_mut()
                    .embedding
                    .as_mut()
                    .unwrap()
                    .phase = StorePhase::Failed;
                return Err(global_script_prelude::completion_error(
                    &self.roots,
                    &mut roots,
                    completion,
                )?);
            }
            match read_module_entry_status(&self.program, &mut roots)? {
                Some(WasmModuleEvaluationStatus::Pending) => Ok(EmbeddedEvaluation::Pending),
                Some(WasmModuleEvaluationStatus::NotStarted) => Err(EngineError::new(
                    "embedded Module did not reach evaluation completion",
                )),
                Some(WasmModuleEvaluationStatus::Settled) | None => Ok(
                    EmbeddedEvaluation::Settled(retained_completion(&mut roots, completion)?),
                ),
            }
        })
    }

    /// Convenience for a synchronously completing entry and diagnostic output.
    /// Use `initialize` to retain exports or resume asynchronous module entry.
    pub fn evaluate(
        mut self,
        store: &mut WasmtimeStore<WasmStoreState>,
    ) -> Result<ObservedRunOutcome, EngineError> {
        let completion = match self.initialize(store)? {
            EmbeddedEvaluation::Pending => {
                return Err(EngineError::from_execution_failure(
                    EngineExecutionFailure::IncompleteModuleEvaluation,
                    "embedded Module evaluation remained pending; use initialize and pump",
                ));
            }
            EmbeddedEvaluation::Settled(JsCompletion::Normal(value)) => {
                ObservedCompletion::Normal(value.observe(store)?)
            }
            EmbeddedEvaluation::Settled(JsCompletion::Throw(value)) => {
                ObservedCompletion::Throw(value.observe(store)?)
            }
        };
        Ok(ObservedRunOutcome {
            backend_used: ExecutionBackend::WasmAot,
            completion,
            output_events: store.data().output_events.take(),
            note: "embedder-owned Wasmtime Store/Linker execution".into(),
        })
    }
}

/// Throw the explicitly declared v1 numerical host-operation error. The P
/// wrapper catches only this tag and creates its ordinary JavaScript Error.
/// Returning any unrelated `wasmtime::Error` from a host function stays fatal.
pub fn throw_foreign_error<R>(
    mut caller: impl AsContextMut<Data = WasmStoreState>,
    code: i32,
) -> wasmtime::Result<R> {
    let tag = caller
        .as_context()
        .data()
        .embedding
        .as_ref()
        .and_then(|state| state.error_tag)
        .ok_or_else(|| {
            wasmtime::Error::msg(
                "foreign expected-error tag was not explicitly declared/registered",
            )
        })?;
    let ty = ExnType::from_tag_type(&tag.ty(&caller))?;
    let allocator = ExnRefPre::new(&mut caller, ty);
    let mut roots = wasmtime::RootScope::new(&mut caller);
    let exception = ExnRef::new(&mut roots, &allocator, &tag, &[WasmtimeVal::I32(code)])?;
    roots.as_context_mut().throw(exception)
}
