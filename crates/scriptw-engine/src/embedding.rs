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
    #[cfg(feature = "native-source-compiler")]
    pub(super) native_source_compiler:
        Option<Arc<wasm_native_source_compiler::NativeSourceCompilerContext>>,
    #[cfg(feature = "native-source-compiler")]
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

/// How Cranelift compiles an embedded program: its optimization level and
/// register allocator, always chosen together.
///
/// The default is [`Speed`](Self::Speed). An embedder compiles a program once
/// and runs it many times in retained Realms, where faster code repays the
/// longer compilation. [`Fast`](Self::Fast) suits one-shot execution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum NativeCompilationPolicy {
    /// Cranelift `OptLevel::Speed` with the backtracking register allocator.
    #[default]
    Speed,
    /// Cranelift `OptLevel::None` with the single-pass register allocator:
    /// the cheapest compilation, at the cost of slower code.
    Fast,
}

impl NativeCompilationPolicy {
    fn settings(self) -> (WasmNativeCompilationMode, wasmtime::RegallocAlgorithm) {
        match self {
            Self::Speed => (
                WasmNativeCompilationMode::Speed,
                wasmtime::RegallocAlgorithm::Backtracking,
            ),
            Self::Fast => (
                WasmNativeCompilationMode::Fast,
                wasmtime::RegallocAlgorithm::SinglePass,
            ),
        }
    }
}

/// Initialize Scriptw's required Wasmtime configuration with a native
/// compilation `policy` (pass `NativeCompilationPolicy::default()`, Speed,
/// unless compilation cost dominates) and the process job count `jobs` (pass
/// [`CompilationJobs::current()`], or the value [`configure_compilation_jobs`]
/// returned). With more than one job, Wasmtime compiles a module's functions in
/// parallel on Scriptw's bounded pool of exactly that many threads; see
/// [`CompilationJobs`] for the memory each job costs. Apply any fuel settings
/// after this call. This sets the copying-GC baseline without epoch checks:
/// compiled code is stopped through `wasmtime::Store::interrupt_handle`, and
/// callers own that handle and any deadline. Evaluation neither requests nor
/// cancels an interrupt.
pub fn configure_wasm_runtime(
    config: &mut wasmtime::Config,
    policy: NativeCompilationPolicy,
    jobs: CompilationJobs,
) {
    configure_runtime_capabilities(config, policy);
    config.parallel_compilation(jobs.parallel());
}

/// The required capabilities and native policy, without a compiler job count.
fn configure_runtime_capabilities(config: &mut wasmtime::Config, policy: NativeCompilationPolicy) {
    let (mode, allocator) = policy.settings();
    *config = wasmtime_config::base_config(mode, wasmtime::WasmBacktraceDetails::Environment);
    config.cranelift_regalloc_algorithm(allocator);
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
    host_linker: OnceLock<Arc<WasmtimeLinker<WasmStoreState>>>,
    prepared_linkage: OnceLock<wasm_runtime_link::PreparedLinkage>,
    /// Where a Collect-mode P keeps its execution profile; `None` for any
    /// other P.
    profile_sites: Option<execution_profile::ProfileSites>,
    /// Optional `NativeHost` runtime artifact + policy enabling runtime `eval`/`Function`
    /// native compilation via `wtime-aot` inside embedded Stores.
    #[cfg(feature = "native-source-compiler")]
    native_source: Option<(
        wasm_runtime_link::ProgramRuntime,
        WasmCompilationPolicy,
        WasmNativeCompilationMode,
    )>,
}

impl EmbeddedRuntime {
    /// Admit persisted requirements and compile actual Wasm, without parsing JS,
    /// resolving source files or installing a Rust foreign-closure registry.
    pub fn compile(engine: &wasmtime::Engine, artifact: &Artifact) -> Result<Self, EngineError> {
        Self::compile_with_profile(engine, artifact, false)
    }

    /// Like [`Self::compile`], but admits a `NativeHost` artifact emitted by
    /// [`Engine::emit_wasm_native_host`] so dynamic `eval` and `new Function`
    /// compile natively against the live embedded runtime `R`.
    #[cfg(feature = "native-source-compiler")]
    pub fn compile_native_host(
        engine: &wasmtime::Engine,
        artifact: &Artifact,
    ) -> Result<Self, EngineError> {
        Self::compile_with_profile(engine, artifact, true)
    }

    /// Compiles a `NativeHost` artifact into serialized native program bytes
    /// suitable for build-time embedding and later loading via
    /// [`Self::from_precompiled_native_host_program`].
    #[cfg(feature = "native-source-compiler")]
    pub fn precompile_native_host_program(
        engine: &wasmtime::Engine,
        artifact: &Artifact,
    ) -> Result<Vec<u8>, EngineError> {
        let runtime = Self::compile_native_host(engine, artifact)?;
        runtime.modules.program.serialize().map_err(|error| {
            EngineError::new(format!(
                "precompiled native-host program serialization failed: {error:#}"
            ))
        })
    }

    /// Loads a build-time precompiled `NativeHost` program module directly against
    /// the Cargo-embedded `NativeHost` runtime `R` without running JS lowering,
    /// Wasm emission, or AOT compilation.
    ///
    /// # Safety
    ///
    /// `serialized_program` must be the exact unchanged output of
    /// [`Self::precompile_native_host_program`] produced by this same compiler
    /// build and target architecture.
    #[cfg(feature = "native-source-compiler")]
    pub unsafe fn from_precompiled_native_host_program(
        engine: &wasmtime::Engine,
        serialized_program: &[u8],
        foreign: ForeignModuleRegistry,
    ) -> Result<Self, EngineError> {
        run_on_sized_stack(|| {
            PRODUCT_WASMTIME_POLICY
                .verify_engine(engine)
                .map_err(EngineError::new)?;
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
                        "NativeHost embedding requires a supported native compiler Engine",
                    ));
                }
            };
            let owner = embedded_runtime::CargoExecutionRuntime::native_host()
                .transpose()?
                .ok_or_else(|| {
                    EngineError::new("Cargo NativeHost embedded runtime is unavailable")
                })?;
            let mut loading = WasmRuntimeModuleLoading::default();
            let started = std::time::Instant::now();
            let runtime_module = owner.load(engine, owner.key(), mode)?;
            loading.runtime = started.elapsed();
            loading.runtime_cache = Some(WasmRuntimeModuleCacheOutcome::Bypassed);
            let intl = intl_data_images::WasmIntlKernel::cargo_minimal(Arc::clone(owner));
            let started = std::time::Instant::now();
            // SAFETY: caller upholds the `precompile_native_host_program` provenance contract.
            let program = unsafe { WasmtimeModule::deserialize(engine, serialized_program) }
                .map_err(|error| {
                    EngineError::new(format!(
                        "precompiled native-host program is incompatible: {error:#}"
                    ))
                })?;
            loading.program = started.elapsed();
            let modules = wasm_runtime_link::WasmModules::new(
                program,
                Some(runtime_module),
                WasmModuleMemoryCacheOutcome::Bypassed,
                WasmRuntimeMatchedImageCacheOutcome::NotAccessed,
                loading,
            )?;
            let mut runtime = Self::from_admitted_modules(engine, modules, foreign, intl);
            runtime.native_source = Some((
                wasm_runtime_link::ProgramRuntime::Cargo(owner),
                WasmCompilationPolicy::from_options(&CompileOptions::default()),
                mode,
            ));
            Ok(runtime)
        })
    }

    fn compile_with_profile(
        engine: &wasmtime::Engine,
        artifact: &Artifact,
        allow_native_host: bool,
    ) -> Result<Self, EngineError> {
        run_on_sized_stack(|| {
            let foreign = artifact.foreign_modules()?;
            PRODUCT_WASMTIME_POLICY
                .verify_engine(engine)
                .map_err(EngineError::new)?;
            // A parallel Engine compiling outside the bounded pool would use
            // Rayon's global pool, one thread per CPU.
            CompilationJobs::verify_engine(engine)?;
            let actual_profile =
                scriptw_aot_wasm::RuntimeSourceCompilationProfile::from_module(&artifact.bytes)
                    .map_err(|error| EngineError::new(error.to_string()))?;
            let expected_profile = if allow_native_host {
                scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost
            } else {
                scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable
            };
            if actual_profile != expected_profile {
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
            // The same walk keeps a Collect-mode P's profile sites section.
            let mut profile_sites = execution_profile::ProfileSitesScan::default();
            let skip_admitted_cargo_runtime = allow_native_host
                && artifact.runtime.as_ref().is_some_and(|rt| {
                    rt.0.immutable_build_package_digest(scriptw_runtime_image::package())
                        .is_some()
                });
            for bytes in [
                Some(artifact.bytes.as_slice()),
                (!skip_admitted_cargo_runtime)
                    .then(|| artifact.runtime.as_ref().map(LinkedRuntime::bytes))
                    .flatten(),
            ]
            .into_iter()
            .flatten()
            {
                let program = std::ptr::eq(bytes, artifact.bytes.as_slice());
                for payload in WasmParser::new(0).parse_all(bytes) {
                    match payload.map_err(|error| EngineError::new(error.to_string()))? {
                        WasmPayload::StartSection { .. } => {
                            return Err(EngineError::new(
                                "embedding rejects implicit Wasm start functions",
                            ));
                        }
                        WasmPayload::CustomSection(section) if program => {
                            profile_sites.observe(&section)?;
                        }
                        _ => {}
                    }
                }
            }
            let profile_sites = profile_sites.finish();
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
            let mut runtime = Self::from_admitted_modules(engine, modules, foreign, intl);
            runtime.profile_sites = profile_sites;
            #[cfg(feature = "native-source-compiler")]
            if allow_native_host {
                let rt_artifact = artifact
                    .runtime
                    .as_ref()
                    .map(|r| Arc::clone(&r.0))
                    .ok_or_else(|| {
                        EngineError::new("NativeHost embedding requires a linked runtime artifact")
                    })?;
                let policy = artifact.compilation_policy.clone().unwrap_or_else(|| {
                    WasmCompilationPolicy::from_options(&CompileOptions::default())
                });
                runtime.native_source = Some((
                    wasm_runtime_link::ProgramRuntime::Raw(rt_artifact),
                    policy,
                    mode,
                ));
            }
            Ok(runtime)
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
            modules: modules.retain_linkage(),
            foreign,
            intl,
            authority: Arc::new(()),
            host_linker: OnceLock::new(),
            prepared_linkage: OnceLock::new(),
            profile_sites: None,
            #[cfg(feature = "native-source-compiler")]
            native_source: None,
        }
    }

    fn cached_host_linker(&self) -> Result<&Arc<WasmtimeLinker<WasmStoreState>>, EngineError> {
        if let Some(linker) = self.host_linker.get() {
            return Ok(linker);
        }
        let built = host_imports::build(
            &self.engine,
            self.modules.host(),
            self.modules.additional_host_imports(),
        )?;
        Ok(self.host_linker.get_or_init(|| Arc::new(built)))
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
            .memories()
            .shared()
            .map(|ty| {
                WasmtimeSharedMemory::new(&self.engine, ty.clone())
                    .map(WasmSharedMemoryBacking::new)
                    .map_err(|error| {
                        EngineError::new(format!("embedding shared memory: {error:#}"))
                    })
            })
            .transpose()?;
        Ok(WasmStoreState {
            weak: Default::default(),
            #[cfg(feature = "native-source-compiler")]
            native_source_compiler: None,
            #[cfg(feature = "native-source-compiler")]
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
        if !linker.try_extend_from_shared(self.cached_host_linker()?) {
            host_imports::register(
                linker,
                self.modules.host(),
                self.modules.additional_host_imports(),
            )?;
        }
        // R defines its own private memory; instantiation links it into P.
        if let crate::host_memory_imports::PrivateMemory::Imported(ty) =
            self.modules.memories().private()
        {
            let memory = WasmtimeMemory::new(&mut *store, ty.clone()).map_err(|error| {
                EngineError::new(format!("embedding private memory: {error:#}"))
            })?;
            linker
                .define(
                    &*store,
                    WASM_HOST_IMPORT_NAMESPACE,
                    WASM_HOST_IMPORT_PRIVATE_MEMORY,
                    memory,
                )
                .map_err(|error| EngineError::new(error.to_string()))?;
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
        run_on_execution_stack(|| self.instantiate_inner(store, linker))
    }

    fn instantiate_inner(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
        linker: &mut WasmtimeLinker<WasmStoreState>,
    ) -> Result<EmbeddedInstance, EngineError> {
        self.check(store, linker, StorePhase::Registered)?;
        store.data_mut().embedding.as_mut().unwrap().phase = StorePhase::Failed;
        for import in self.modules.foreign_imports() {
            let actual = linker
                .get(&mut *store, &import.module, &import.name)
                .map_err(|error| {
                    EngineError::new(format!(
                        "missing declared foreign import {}.{}: {error:#}",
                        import.module, import.name
                    ))
                })?;
            let matches = match (&import.ty, actual.ty(&*store)) {
                (WasmtimeExternType::Func(expected), WasmtimeExternType::Func(actual)) => {
                    FuncType::eq(expected, &actual)
                }
                (WasmtimeExternType::Tag(expected), WasmtimeExternType::Tag(actual)) => {
                    FuncType::eq(expected.ty(), actual.ty())
                }
                _ => false,
            };
            if !matches {
                return Err(EngineError::new(format!(
                    "foreign import type mismatch: {}.{}",
                    import.module, import.name
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
        let prepared = self
            .host_linker
            .get()
            .filter(|base| linker.has_unshadowed_shared_definitions(base))
            .map(|base| {
                self.prepared_linkage
                    .get_or_init(|| wasm_runtime_link::PreparedLinkage::new(base, &self.modules))
            });
        let (program, roots) =
            wasm_runtime_link::instantiate_with_linkage(linker, store, &self.modules, prepared)?;
        #[cfg(feature = "native-source-compiler")]
        if let Some((ref rt_runtime, ref policy, native_mode)) = self.native_source {
            let execution_control = Arc::new(WasmAgentExecutionControl::new());
            execution_control.arm(std::time::Instant::now(), None);
            let context = wasm_native_source_compiler::NativeSourceCompilerContext::new(
                linker.clone(),
                roots,
                rt_runtime.as_ref(),
                policy.clone(),
                execution_control,
                native_mode,
            )?;
            store.data_mut().native_source_compiler = Some(Arc::new(context));
        }
        let main = match self
            .modules
            .program_main_export()
            .and_then(|export| program.get_module_export(&mut *store, export))
            .and_then(WasmtimeExtern::into_func)
        {
            Some(func) => func
                .typed::<(), GcMainCompletion>(&*store)
                .map_err(|error| {
                    EngineError::new(format!("embedding main signature: {error:#}"))
                })?,
            None => program
                .get_typed_func::<(), GcMainCompletion>(&mut *store, "main")
                .map_err(|error| {
                    EngineError::new(format!("embedding main signature: {error:#}"))
                })?,
        };
        let bridges = operations::Bridges::new(&mut *store, &program, &roots, &self.modules);
        let state = store.data_mut().embedding.as_mut().unwrap();
        state.phase = StorePhase::Instantiated;
        Ok(EmbeddedInstance {
            main,
            program,
            roots,
            store: Arc::clone(&state.store),
            bridges,
            entry: None,
            profile_sites: self.profile_sites.clone(),
            #[cfg(feature = "native-source-compiler")]
            has_native_source: self.native_source.is_some(),
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
    /// Store-local bridge handles, with typed call setup deferred until use.
    bridges: operations::Bridges,
    /// The entry Realm, global Environment and program, retained once
    /// `main` has created them; see `operations::EntryContext`.
    entry: Option<operations::EntryContext>,
    profile_sites: Option<execution_profile::ProfileSites>,
    #[cfg(feature = "native-source-compiler")]
    has_native_source: bool,
}

impl EmbeddedInstance {
    /// Allows an embedder to resume calls after recovering from a host-triggered
    /// `InterruptHandle` preemption (e.g. `cancel_termination`).
    pub fn recover_after_interrupt(&self, store: &mut WasmtimeStore<WasmStoreState>) {
        if let Some(state) = store.data_mut().embedding.as_mut() {
            if Arc::ptr_eq(&state.store, &self.store) && state.phase == StorePhase::Failed {
                state.phase = StorePhase::Evaluated;
            }
        }
    }

    /// Initialize real compiled main once. Pending top-level await retains the
    /// instance; deliver host completions with `pump` until it settles.
    pub fn initialize(
        &mut self,
        store: &mut WasmtimeStore<WasmStoreState>,
    ) -> Result<EmbeddedEvaluation, EngineError> {
        #[cfg(feature = "native-source-compiler")]
        let retain = !self.has_native_source;
        #[cfg(not(feature = "native-source-compiler"))]
        let retain = true;
        run_on_execution_stack(|| self.initialize_inner(store, retain))
    }

    /// Initialize with CPU time sampled on the actual execution worker.
    ///
    /// The span includes native main, synchronous host/GC work and retaining
    /// the completion. It excludes dispatch/waiting, subsequent pumps, caller
    /// observation/output collection and other workers. Clock failures are
    /// reported without changing execution. Sampling adds two clock reads;
    /// ordinary [`Self::initialize`] performs none.
    pub fn initialize_profiled(
        &mut self,
        store: &mut WasmtimeStore<WasmStoreState>,
    ) -> Result<(EmbeddedEvaluation, WasmThreadCpuObservation), EngineError> {
        self.initialize_profiled_inner(store, true)
    }

    fn initialize_profiled_inner(
        &mut self,
        store: &mut WasmtimeStore<WasmStoreState>,
        retain_entry_context: bool,
    ) -> Result<(EmbeddedEvaluation, WasmThreadCpuObservation), EngineError> {
        run_on_execution_stack(|| {
            let started = WasmThreadCpuInstant::now();
            let result = self.initialize_inner(store, retain_entry_context);
            let cpu = match started {
                Ok(started) => started.elapsed(),
                Err(reason) => WasmThreadCpuObservation::Unavailable(reason),
            };
            result.map(|evaluation| (evaluation, cpu))
        })
    }

    fn initialize_inner(
        &mut self,
        store: &mut WasmtimeStore<WasmStoreState>,
        retain_entry_context: bool,
    ) -> Result<EmbeddedEvaluation, EngineError> {
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
        if global_script_prelude::failed_global(self.bridges.prelude_phase, &mut roots)? {
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
        if retain_entry_context {
            // `main` has created the entry Realm and installed this program's
            // Module graph (also when its evaluation is pending or threw). If
            // entry left them unresolvable, every operation keeps reading them
            // afresh and reports exactly what it finds.
            self.entry = operations::EntryContext::resolve(&self.bridges, &mut roots).ok();
        }
        match module_entry_status(self.bridges.module_status.clone(), &mut roots)? {
            Some(WasmModuleEvaluationStatus::Pending) => Ok(EmbeddedEvaluation::Pending),
            Some(WasmModuleEvaluationStatus::NotStarted) => Err(EngineError::new(
                "embedded Module did not reach evaluation completion",
            )),
            Some(WasmModuleEvaluationStatus::Settled) | None => Ok(EmbeddedEvaluation::Settled(
                retained_completion(&mut roots, completion)?,
            )),
        }
    }

    /// Convenience for a synchronously completing entry and diagnostic output.
    /// Use `initialize` to retain exports or resume asynchronous module entry.
    pub fn evaluate(
        mut self,
        store: &mut WasmtimeStore<WasmStoreState>,
    ) -> Result<ObservedRunOutcome, EngineError> {
        let evaluation = run_on_execution_stack(|| self.initialize_inner(store, false))?;
        Self::finish_evaluation(evaluation, store)
    }

    /// Evaluate with the actual worker CPU span from [`Self::initialize_profiled`].
    /// Completion observation and output collection remain on the caller, as
    /// in [`Self::evaluate`], and are outside this CPU span.
    pub fn evaluate_profiled(
        mut self,
        store: &mut WasmtimeStore<WasmStoreState>,
    ) -> Result<(ObservedRunOutcome, WasmThreadCpuObservation), EngineError> {
        let (evaluation, cpu) = self.initialize_profiled_inner(store, false)?;
        let outcome = Self::finish_evaluation(evaluation, store)?;
        Ok((outcome, cpu))
    }

    /// The execution profile this instance recorded since instantiation,
    /// as a training run of `training_requests` requests: the program must
    /// have been compiled with [`ExecutionProfileUse::Collect`] and
    /// initialized. Call it once the training run has ended; the profile's
    /// read-site kinds are what the site records learned until then.
    pub fn collect_profile(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
        training_requests: std::num::NonZeroU32,
    ) -> Result<ExecutionProfile, EngineError> {
        let sites = self
            .profile_sites
            .as_ref()
            .ok_or_else(|| EngineError::from_execution_profile(ProfileError::NotCollecting))?;
        self.check_store(store)?;
        sites.collect(&self.program, store, training_requests)
    }

    fn finish_evaluation(
        evaluation: EmbeddedEvaluation,
        store: &mut WasmtimeStore<WasmStoreState>,
    ) -> Result<ObservedRunOutcome, EngineError> {
        let completion = match evaluation {
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

#[cfg(all(test, feature = "native-source-compiler"))]
mod native_host_precompile_tests {
    use super::*;

    #[test]
    fn precompile_native_host_program_round_trips_with_dynamic_eval() {
        let mut config = wasmtime::Config::new();
        configure_wasm_runtime(
            &mut config,
            NativeCompilationPolicy::default(),
            CompilationJobs::current(),
        );
        config.cranelift_opt_level(wasmtime::OptLevel::SpeedAndSize);
        let engine = wasmtime::Engine::new(&config).expect("engine");
        let compiler = Engine::new(RealmBuilder::new().build());
        let artifact = compiler
            .compile_and_emit_module_native_host_cached(
                "export function run(x) { return eval('x + 22'); }",
                CompileOptions::default(),
            )
            .expect("emit native host");
        let bytes = EmbeddedRuntime::precompile_native_host_program(&engine, &artifact)
            .expect("precompile native host");
        assert!(!bytes.is_empty());

        let runtime = unsafe {
            EmbeddedRuntime::from_precompiled_native_host_program(
                &engine,
                &bytes,
                ForeignModuleRegistry::default(),
            )
        }
        .expect("load precompiled native host");

        let state = runtime
            .new_state(RealmBuilder::new().build(), ())
            .expect("state");
        let mut store = wasmtime::Store::new(&engine, state);
        let mut linker = wasmtime::Linker::new(&engine);
        runtime.register(&mut store, &mut linker).expect("register");
        let mut instance = runtime
            .instantiate(&mut store, &mut linker)
            .expect("instantiate");
        assert!(matches!(
            instance.initialize(&mut store).expect("init"),
            EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
        ));
        let run_fn = match instance.get_export(&mut store, "run").expect("get run") {
            JsCompletion::Normal(v) => v,
            other => panic!("unexpected export: {other:?}"),
        };
        let arg = instance
            .value(&mut store, ForeignValue::Number(20.0))
            .expect("arg");
        let out = match instance
            .call(&mut store, &run_fn, None, &[arg])
            .expect("call run")
        {
            JsCompletion::Normal(v) => v,
            other => panic!("unexpected call outcome: {other:?}"),
        };
        assert_eq!(out.as_number(), Some(42.0));
    }
}
