//! Observations of the product Store; no alternate collector or forced GC.
use super::*;
use std::time::Duration;
mod inlining;
mod native_compilation;
mod process_memory;
pub(crate) use inlining::PINNED_WASMTIME_INLINING_LIMITS;
pub use inlining::{WasmProfileInlining, WasmProfileInliningLimits};
pub use native_compilation::WasmProgramNativeCompilationProfile;
pub(super) use process_memory::ProcessMemorySampler;
pub use process_memory::{
    WasmProcessMemoryObservation, WasmProcessMemorySample, WasmProcessMemoryScope,
    WasmProcessMemoryUnavailable,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmRuntimeModuleCacheOutcome {
    Hit,
    Miss,
    Bypassed,
}

/// Nested spans within module loading. Embedded timings are absent when the
/// module cache avoids image access or a diagnostic matched image is selected.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WasmRuntimeEmbeddedImageTimings {
    /// Immutable manifest/source/target/configuration and package digest checks.
    pub admission: Duration,
    /// First bounded Cargo decode/digest and private backing creation, or
    /// lookup of an existing backing. No decoded image-sized lease is required.
    pub native_backing_preparation: Duration,
    /// Either profile's transient decoding and exact native digest check.
    /// Reusing an admitted private backing file needs no decoded lease.
    pub native_lease_decode_and_digest: Duration,
    pub deserialize: Duration,
}

/// R and P are reported separately: a P cache hit does not establish an R hit.
/// All fields are nested inside WasmRuntimeTimings::module, not additive to it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WasmRuntimeModuleLoading {
    pub runtime_cache: Option<WasmRuntimeModuleCacheOutcome>,
    pub runtime: Duration,
    pub program: Duration,
    pub embedded_image: Option<WasmRuntimeEmbeddedImageTimings>,
}

/// Only immutable, Store-independent host function registration is reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmRuntimeHostImportCacheOutcome {
    Hit,
    Miss,
}

/// Diagnostic-only capacity-matched optimized R image cache. The cache retains
/// at most two serialized images and 256 MiB; it never retains mutable JS state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmRuntimeMatchedImageCacheOutcome {
    NotAccessed,
    Hit,
    Miss,
    Oversized,
}
impl From<WasmModuleMemoryCacheOutcome> for WasmRuntimeModuleCacheOutcome {
    fn from(value: WasmModuleMemoryCacheOutcome) -> Self {
        match value {
            WasmModuleMemoryCacheOutcome::Hit => Self::Hit,
            WasmModuleMemoryCacheOutcome::Miss => Self::Miss,
            WasmModuleMemoryCacheOutcome::Bypassed => Self::Bypassed,
        }
    }
}

/// These spans follow the existing execution pipeline. Module includes native
/// compilation or cache lookup; execution includes host work and completion
/// decoding. Total also includes admission and gaps between these phases, and
/// the opt-in memory sampler's setup/retirement. It excludes worker creation
/// and Store destruction.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WasmRuntimeTimings {
    /// Artifact/Intl admission before selecting the native execution Engine.
    pub admission: Duration,
    pub engine: Duration,
    pub module: Duration,
    pub store_and_linker: Duration,
    /// Subspan of store_and_linker: registering host imports on the fresh linker.
    pub host_import_registration: Duration,
    pub instantiate: Duration,
    pub export_lookup: Duration,
    pub execution_and_completion: Duration,
    pub total: Duration,
}

/// Capacity is reserved GC heap space, not allocated/live bytes or a peak.
/// The first snapshot follows instantiation; the second follows completion
/// decoding and dropping host completion roots. Neither requests a collection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WasmRuntimeHeapObservation {
    pub capacity_before_execution_bytes: usize,
    pub capacity_after_execution_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmRuntimeMetricUnavailable {
    /// Pinned Wasmtime 47 exposes capacity but no public per-Store counter.
    WasmtimeHasNoPublicCounter,
    /// Boundary capacity snapshots do not establish a peak memory usage.
    NoPerInvocationPeakSampler,
}

/// The same required runtime policy used to construct both native profiles.
pub fn wasm_runtime_profile_policy() -> String {
    PRODUCT_WASMTIME_POLICY.report()
}

/// Explicit diagnostic cohort only. Ordinary execution retains automatic
/// dispatch and its CodeTooLarge retry. Forced cohorts fail instead of silently
/// changing mode; every other product Wasmtime capability/limit stays identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasmProfileNativeMode {
    Fast,
    SizeOptimized,
    Speed,
}
impl WasmProfileNativeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::SizeOptimized => "size-optimized",
            Self::Speed => "speed",
        }
    }
    pub(crate) fn internal(self) -> WasmNativeCompilationMode {
        match self {
            Self::Fast => WasmNativeCompilationMode::Fast,
            Self::SizeOptimized => WasmNativeCompilationMode::SizeOptimized,
            Self::Speed => WasmNativeCompilationMode::Speed,
        }
    }
    pub(crate) fn from_internal(mode: WasmNativeCompilationMode) -> Self {
        match mode {
            WasmNativeCompilationMode::Fast => Self::Fast,
            WasmNativeCompilationMode::SizeOptimized => Self::SizeOptimized,
            WasmNativeCompilationMode::Speed => Self::Speed,
        }
    }
}

/// Bounded capacity cohorts for the copying collector. These are starting
/// capacities, not live-byte limits; the ordinary Store limit remains in force.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasmProfileGcInitialCapacity {
    MiB16,
    MiB32,
    MiB64,
    MiB128,
}

impl WasmProfileGcInitialCapacity {
    pub const fn mebibytes(self) -> usize {
        match self {
            Self::MiB16 => 16,
            Self::MiB32 => 32,
            Self::MiB64 => 64,
            Self::MiB128 => 128,
        }
    }

    pub const fn bytes(self) -> usize {
        self.mebibytes() * 1024 * 1024
    }
}

const _: () = assert!(
    WasmProfileGcInitialCapacity::MiB16.bytes() == wasmtime_config::WASM_GC_HEAP_INITIAL_BYTES
);
const _: () = assert!(WasmProfileGcInitialCapacity::MiB128.bytes() < WASM_STORE_MEMORY_CAP_BYTES);

/// Keep register allocation independent of the optimization-level experiment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasmProfileRegisterAllocation {
    SinglePass,
    Backtracking,
}

impl WasmProfileRegisterAllocation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SinglePass => "single-pass",
            Self::Backtracking => "backtracking",
        }
    }

    pub(crate) fn algorithm(self) -> wasmtime::RegallocAlgorithm {
        match self {
            Self::SinglePass => wasmtime::RegallocAlgorithm::SinglePass,
            Self::Backtracking => wasmtime::RegallocAlgorithm::Backtracking,
        }
    }
}

/// Explicit immutable experimental policy. Its closed domains bound both the
/// experimental heap capacities and the retained Engine configurations. No
/// process environment or ordinary execution policy is changed by profiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WasmRuntimeProfileOptions {
    native_mode: WasmProfileNativeMode,
    initial_gc_capacity: WasmProfileGcInitialCapacity,
    register_allocation: WasmProfileRegisterAllocation,
    native_inlining: WasmProfileInlining,
}

impl WasmRuntimeProfileOptions {
    pub const fn new(
        native_mode: WasmProfileNativeMode,
        initial_gc_capacity: WasmProfileGcInitialCapacity,
        register_allocation: WasmProfileRegisterAllocation,
    ) -> Self {
        Self {
            native_mode,
            initial_gc_capacity,
            register_allocation,
            native_inlining: WasmProfileInlining::Disabled,
        }
    }

    pub const fn native_mode(self) -> WasmProfileNativeMode {
        self.native_mode
    }

    pub const fn initial_gc_capacity(self) -> WasmProfileGcInitialCapacity {
        self.initial_gc_capacity
    }

    pub const fn register_allocation(self) -> WasmProfileRegisterAllocation {
        self.register_allocation
    }

    pub const fn with_native_inlining(mut self, inlining: WasmProfileInlining) -> Self {
        self.native_inlining = inlining;
        self
    }

    pub const fn native_inlining(self) -> WasmProfileInlining {
        self.native_inlining
    }

    pub(crate) fn configure_native_inlining(self, config: &mut WasmtimeConfig) {
        self.native_inlining.configure(config);
    }

    pub(crate) fn ordinary(native_mode: WasmNativeCompilationMode) -> Self {
        Self::new(
            WasmProfileNativeMode::from_internal(native_mode),
            WasmProfileGcInitialCapacity::MiB16,
            WasmProfileRegisterAllocation::SinglePass,
        )
    }

    /// Read the executing Engine, independently of the requested cohort. A
    /// configuration outside this diagnostic domain cannot become plausible
    /// policy metadata by copying the request or assuming product defaults.
    pub(crate) fn observe_engine(engine: &WasmtimeEngine) -> Result<Self, EngineError> {
        let native_mode = match engine.get_cranelift_opt_level() {
            Some(wasmtime::OptLevel::None) => WasmProfileNativeMode::Fast,
            Some(wasmtime::OptLevel::SpeedAndSize) => WasmProfileNativeMode::SizeOptimized,
            Some(wasmtime::OptLevel::Speed) => WasmProfileNativeMode::Speed,
            Some(other) => {
                return Err(EngineError::new(format!(
                    "runtime profile cannot represent observed native optimization level {other:?}"
                )));
            }
            None => {
                return Err(EngineError::new(
                    "runtime profile cannot observe the executing Engine's native optimization level",
                ));
            }
        };
        const MIB16: u64 = WasmProfileGcInitialCapacity::MiB16.bytes() as u64;
        const MIB32: u64 = WasmProfileGcInitialCapacity::MiB32.bytes() as u64;
        const MIB64: u64 = WasmProfileGcInitialCapacity::MiB64.bytes() as u64;
        const MIB128: u64 = WasmProfileGcInitialCapacity::MiB128.bytes() as u64;
        let initial_gc_capacity = match engine.get_gc_heap_initial_size() {
            MIB16 => WasmProfileGcInitialCapacity::MiB16,
            MIB32 => WasmProfileGcInitialCapacity::MiB32,
            MIB64 => WasmProfileGcInitialCapacity::MiB64,
            MIB128 => WasmProfileGcInitialCapacity::MiB128,
            other => {
                return Err(EngineError::new(format!(
                    "runtime profile cannot represent observed initial GC capacity {other} bytes"
                )));
            }
        };
        let register_allocation = match engine.get_cranelift_regalloc_algorithm() {
            Some(wasmtime::RegallocAlgorithm::SinglePass) => {
                WasmProfileRegisterAllocation::SinglePass
            }
            Some(wasmtime::RegallocAlgorithm::Backtracking) => {
                WasmProfileRegisterAllocation::Backtracking
            }
            Some(other) => {
                return Err(EngineError::new(format!(
                    "runtime profile cannot represent observed native register allocator {other:?}"
                )));
            }
            None => {
                return Err(EngineError::new(
                    "runtime profile cannot observe the executing Engine's native register allocator",
                ));
            }
        };
        let inlining = WasmProfileInlining::observe(engine)?;
        Ok(
            Self::new(native_mode, initial_gc_capacity, register_allocation)
                .with_native_inlining(inlining),
        )
    }
}

#[derive(Debug, Clone)]
pub struct WasmRuntimeProfile {
    pub native_mode: WasmProfileNativeMode,
    /// Actual native compiler and heap configuration of the executing Engine.
    pub options: WasmRuntimeProfileOptions,
    pub outcome: ObservedRunOutcome,
    pub timings: WasmRuntimeTimings,
    pub module_cache: WasmRuntimeModuleCacheOutcome,
    pub module_loading: WasmRuntimeModuleLoading,
    pub host_import_cache: WasmRuntimeHostImportCacheOutcome,
    pub matched_runtime_image_cache: WasmRuntimeMatchedImageCacheOutcome,
    pub heap: WasmRuntimeHeapObservation,
    pub runtime_policy: String,
    pub allocation_count: WasmRuntimeMetricUnavailable,
    pub allocation_bytes: WasmRuntimeMetricUnavailable,
    pub live_gc_bytes: WasmRuntimeMetricUnavailable,
    pub collection_count: WasmRuntimeMetricUnavailable,
    pub gc_pause_time: WasmRuntimeMetricUnavailable,
    pub peak_gc_heap_bytes: WasmRuntimeMetricUnavailable,
    /// Sampled current process RSS during this invocation, with explicit scope
    /// and sampling policy. This does not claim the unsampled true peak.
    pub process_memory: WasmProcessMemoryObservation,
}

pub(super) struct RuntimeObservation {
    pub(super) requested_options: Option<WasmRuntimeProfileOptions>,
    pub(super) actual_native_mode: Option<WasmProfileNativeMode>,
    pub(super) actual_options: Option<WasmRuntimeProfileOptions>,
    pub(super) host_import_cache: Option<WasmRuntimeHostImportCacheOutcome>,
    pub(super) matched_runtime_image_cache: WasmRuntimeMatchedImageCacheOutcome,
    pub(super) timings: WasmRuntimeTimings,
    pub(super) module_cache: WasmRuntimeModuleCacheOutcome,
    pub(super) module_loading: WasmRuntimeModuleLoading,
    pub(super) heap: WasmRuntimeHeapObservation,
    pub(super) process_memory: Option<WasmProcessMemoryObservation>,
}
impl Default for RuntimeObservation {
    fn default() -> Self {
        Self {
            requested_options: None,
            actual_native_mode: None,
            actual_options: None,
            host_import_cache: None,
            matched_runtime_image_cache: WasmRuntimeMatchedImageCacheOutcome::NotAccessed,
            timings: WasmRuntimeTimings::default(),
            heap: WasmRuntimeHeapObservation::default(),
            process_memory: None,
            module_cache: WasmRuntimeModuleCacheOutcome::Bypassed,
            module_loading: WasmRuntimeModuleLoading::default(),
        }
    }
}

impl Engine {
    /// Execute already emitted product Wasm in a fresh Store under the same
    /// policy, imports, native-module cache and timeout as ordinary execution.
    /// JavaScript throws remain structured outcomes; compiler/host/trap failures
    /// remain errors. Parsing, lowering and emission are outside these spans.
    pub fn profile_wasm_execution(
        &self,
        artifact: &Artifact,
        timeout_ms: Option<u64>,
        can_block: bool,
    ) -> Result<WasmRuntimeProfile, EngineError> {
        self.profile_wasm_execution_options(artifact, timeout_ms, can_block, None)
    }

    /// Measure a separately labelled native compiler-policy cohort. This does
    /// not change product automatic policy, JS state isolation or cache keys.
    pub fn profile_wasm_execution_with_native_mode(
        &self,
        artifact: &Artifact,
        timeout_ms: Option<u64>,
        can_block: bool,
        native_mode: WasmProfileNativeMode,
    ) -> Result<WasmRuntimeProfile, EngineError> {
        self.profile_wasm_execution_with_options(
            artifact,
            timeout_ms,
            can_block,
            WasmRuntimeProfileOptions::ordinary(native_mode.internal()),
        )
    }

    /// Measure a separate compiler/GC configuration, retaining real product
    /// imports, cycle/weak reachability, Store limits, deadlines and fresh state.
    /// Explicit configurations fail rather than silently retrying another mode.
    pub fn profile_wasm_execution_with_options(
        &self,
        artifact: &Artifact,
        timeout_ms: Option<u64>,
        can_block: bool,
        options: WasmRuntimeProfileOptions,
    ) -> Result<WasmRuntimeProfile, EngineError> {
        self.profile_wasm_execution_options(artifact, timeout_ms, can_block, Some(options))
    }

    fn profile_wasm_execution_options(
        &self,
        artifact: &Artifact,
        timeout_ms: Option<u64>,
        can_block: bool,
        options: Option<WasmRuntimeProfileOptions>,
    ) -> Result<WasmRuntimeProfile, EngineError> {
        run_on_sized_stack(|| {
            let mut observation = RuntimeObservation {
                requested_options: options,
                ..RuntimeObservation::default()
            };
            let outcome = self
                .execute_with_wasm_bytes_profiled(
                    artifact.program(),
                    timeout_ms,
                    can_block,
                    WasmModuleMemoryCachePolicy::Retain,
                    None,
                    None,
                    &WasmExecutionMode::Structured,
                    Some(&mut observation),
                )?
                .into_structured()?;
            let process_memory = observation.process_memory.ok_or_else(|| {
                EngineError::new(
                    "completed runtime profile omitted its invocation memory observation",
                )
            })?;
            let unavailable = WasmRuntimeMetricUnavailable::WasmtimeHasNoPublicCounter;
            let peak = WasmRuntimeMetricUnavailable::NoPerInvocationPeakSampler;
            let actual_mode = observation.actual_native_mode.ok_or_else(|| {
                EngineError::new(
                    "completed runtime profile omitted its actual native compiler mode",
                )
            })?;
            let actual_options = observation.actual_options.ok_or_else(|| {
                EngineError::new(
                    "completed runtime profile omitted its actual Engine configuration",
                )
            })?;
            let host_import_cache = observation.host_import_cache.ok_or_else(|| {
                EngineError::new(
                    "completed runtime profile omitted its host-import cache observation",
                )
            })?;
            Ok(WasmRuntimeProfile {
                native_mode: actual_mode,
                options: actual_options,
                outcome,
                timings: observation.timings,
                module_cache: observation.module_cache,
                module_loading: observation.module_loading,
                host_import_cache,
                matched_runtime_image_cache: observation.matched_runtime_image_cache,
                heap: observation.heap,
                runtime_policy: wasm_runtime_profile_policy(),
                allocation_count: unavailable,
                allocation_bytes: unavailable,
                live_gc_bytes: unavailable,
                collection_count: unavailable,
                gc_pause_time: unavailable,
                peak_gc_heap_bytes: peak,
                process_memory,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiling_uses_product_wasm_completion_output_and_capacity_without_forced_collection() {
        let engine = Engine::new(RealmBuilder::new().build());
        let compilation = engine
            .profile_script_compilation(
                "var x = {value: 42}; print(x.value); x.value;",
                CompileOptions::default(),
            )
            .unwrap();
        let profile = engine
            .profile_wasm_execution(&compilation.artifact, None, false)
            .unwrap();
        assert_eq!(profile.outcome.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            profile.outcome.completion,
            ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(42.0)))
        );
        assert_eq!(
            profile.outcome.output_events,
            [HostOutputEvent::PrintLine("42".into())]
        );
        // Instantiation may leave the Store's GC heap uninitialized. The
        // program allocates its object only when main executes.
        assert!(
            profile.heap.capacity_before_execution_bytes
                <= profile.heap.capacity_after_execution_bytes
        );
        assert!(profile.heap.capacity_after_execution_bytes > 0);
        assert!(profile.timings.total >= profile.timings.execution_and_completion);
        assert!(
            profile.module_loading.runtime_cache.is_some(),
            "linked R has its own cache outcome"
        );
        assert!(profile.module_loading.runtime > Duration::ZERO);
        assert!(profile.module_loading.program > Duration::ZERO);
        assert!(
            profile.timings.module
                >= profile.module_loading.runtime + profile.module_loading.program
        );
        if let Some(image) = &profile.module_loading.embedded_image {
            assert!(image.deserialize > Duration::ZERO);
            assert!(
                profile.module_loading.runtime
                    >= image.admission
                        + image.native_backing_preparation
                        + image.native_lease_decode_and_digest
                        + image.deserialize
            );
        }
        assert_eq!(
            profile.collection_count,
            WasmRuntimeMetricUnavailable::WasmtimeHasNoPublicCounter
        );
        assert_eq!(
            profile.peak_gc_heap_bytes,
            WasmRuntimeMetricUnavailable::NoPerInvocationPeakSampler
        );
        if let WasmProcessMemoryObservation::Sampled(sample) = &profile.process_memory {
            assert_eq!(
                sample.scope(),
                WasmProcessMemoryScope::RuntimeInvocationProcessRss
            );
            assert!(sample.sample_count() >= 2);
            assert!(sample.maximum_rss_bytes() >= sample.initial_rss_bytes());
            assert!(sample.maximum_rss_bytes() >= sample.final_rss_bytes());
        }
        assert_eq!(profile.runtime_policy, PRODUCT_WASMTIME_POLICY.report());
    }

    #[test]
    fn runtime_profiling_preserves_throw_and_backend_failure_boundaries() {
        let engine = Engine::new(RealmBuilder::new().build());
        let compilation = engine
            .profile_script_compilation("throw 7;", CompileOptions::default())
            .unwrap();
        let profile = engine
            .profile_wasm_execution(&compilation.artifact, None, false)
            .unwrap();
        assert_eq!(
            profile.outcome.completion,
            ObservedCompletion::Throw(ObservedJsValue::Number(ObservedNumber::from_f64(7.0)))
        );
        assert!(engine
            .profile_wasm_execution(
                &Artifact {
                    bytes: b"invalid wasm".to_vec(),
                    description: String::new(),
                    debug_dump: String::new(),
                    runtime: None,
                    compilation_policy: None,
                },
                None,
                false,
            )
            .is_err());
    }
}

#[cfg(test)]
mod native_mode_cohort_tests {
    use super::*;

    #[test]
    fn forced_modes_are_reported_exactly_and_reuse_only_their_own_cached_modules() {
        let engine = Engine::new(RealmBuilder::new().build());
        let artifact = engine
            .profile_script_compilation("4 * 7;", CompileOptions::default())
            .unwrap()
            .artifact;
        assert!(
            artifact.runtime().is_none(),
            "mode-key isolation must not compete with R in a one-entry cache"
        );
        for requested in [
            WasmProfileNativeMode::Fast,
            WasmProfileNativeMode::SizeOptimized,
            WasmProfileNativeMode::Speed,
        ] {
            for repeat in 0..2 {
                let profile = engine
                    .profile_wasm_execution_with_native_mode(&artifact, None, false, requested)
                    .unwrap();
                assert_eq!(profile.native_mode, requested);
                assert_eq!(profile.options.native_mode(), requested);
                assert_eq!(
                    profile.outcome.completion,
                    ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(
                        28.0
                    )))
                );
                assert!(profile.outcome.output_events.is_empty());
                assert_eq!(
                    profile.module_cache,
                    if repeat == 0 {
                        WasmRuntimeModuleCacheOutcome::Miss
                    } else {
                        WasmRuntimeModuleCacheOutcome::Hit
                    }
                );
                assert_eq!(profile.runtime_policy, wasm_runtime_profile_policy());
            }
        }
        let ordinary = engine
            .profile_wasm_execution(&artifact, None, false)
            .unwrap();
        assert_eq!(ordinary.native_mode, WasmProfileNativeMode::Fast);
        assert_eq!(
            ordinary.matched_runtime_image_cache,
            WasmRuntimeMatchedImageCacheOutcome::NotAccessed
        );
    }

    #[test]
    fn explicit_capacity_and_allocator_configs_keep_product_capabilities_and_default_policy() {
        for mode in [WasmProfileNativeMode::Fast, WasmProfileNativeMode::Speed] {
            for capacity in [
                WasmProfileGcInitialCapacity::MiB16,
                WasmProfileGcInitialCapacity::MiB64,
            ] {
                for allocation in [
                    WasmProfileRegisterAllocation::SinglePass,
                    WasmProfileRegisterAllocation::Backtracking,
                ] {
                    let options = WasmRuntimeProfileOptions::new(mode, capacity, allocation);
                    let engine = shared_profile_wasm_engine(options).unwrap();
                    assert_eq!(engine.get_gc_heap_initial_size(), capacity.bytes() as u64);
                    assert_eq!(
                        engine.get_cranelift_regalloc_algorithm(),
                        Some(allocation.algorithm())
                    );
                    assert_eq!(
                        engine.get_cranelift_opt_level(),
                        Some(match mode {
                            WasmProfileNativeMode::Fast => wasmtime::OptLevel::None,
                            WasmProfileNativeMode::SizeOptimized =>
                                wasmtime::OptLevel::SpeedAndSize,
                            WasmProfileNativeMode::Speed => wasmtime::OptLevel::Speed,
                        })
                    );
                    PRODUCT_WASMTIME_POLICY.verify_engine(&engine).unwrap();
                    assert!(WasmtimeEngine::same(
                        &engine,
                        &shared_profile_wasm_engine(options).unwrap()
                    ));
                }
            }
        }
        let ordinary = shared_wasm_engine().unwrap();
        assert_eq!(
            ordinary.get_gc_heap_initial_size(),
            wasmtime_config::WASM_GC_HEAP_INITIAL_BYTES as u64
        );
        assert_eq!(
            ordinary.get_cranelift_opt_level(),
            Some(wasmtime::OptLevel::None)
        );
        assert_eq!(
            ordinary.get_cranelift_regalloc_algorithm(),
            Some(wasmtime::RegallocAlgorithm::SinglePass)
        );
    }

    #[test]
    fn diagnostic_capacity_engines_never_accept_modules_from_another_configuration() {
        let make_options = |capacity| {
            WasmRuntimeProfileOptions::new(
                WasmProfileNativeMode::Fast,
                capacity,
                WasmProfileRegisterAllocation::SinglePass,
            )
        };
        let first =
            shared_profile_wasm_engine(make_options(WasmProfileGcInitialCapacity::MiB32)).unwrap();
        let second =
            shared_profile_wasm_engine(make_options(WasmProfileGcInitialCapacity::MiB64)).unwrap();
        assert!(!WasmtimeEngine::same(&first, &second));
        // Unique custom section prevents an unrelated native test supplying a hit.
        let bytes = b"\0asm\x01\0\0\0\0\x08\x07cohorts";
        let source = WasmModuleSource::Program(bytes);
        for native in [&first, &second, &first] {
            let (module, _) =
                memory_cached_wasm_module(native, source, WasmNativeCompilationMode::Fast, || {
                    compile_wasm_module(native, bytes)
                })
                .unwrap();
            assert!(WasmtimeEngine::same(module.engine(), native));
        }
    }

    #[test]
    fn explicit_speed_capacity_profile_preserves_gc_roots_and_fresh_realm_state() {
        let engine = Engine::new(RealmBuilder::new().build());
        let artifact = engine.profile_script_compilation(
            "var seen = typeof seen === 'undefined' ? 1 : seen + 1; if (seen !== 1) throw 'shared Realm'; var cycle = {answer:42}; cycle.self = cycle; gc(); if (cycle.self !== cycle) throw 'lost cycle'; print(cycle.answer); cycle.answer;",
            CompileOptions { host_surface_policy: HostSurfacePolicy::Test262, ..CompileOptions::default() },
        ).unwrap().artifact;
        let options = WasmRuntimeProfileOptions::new(
            WasmProfileNativeMode::Speed,
            WasmProfileGcInitialCapacity::MiB64,
            WasmProfileRegisterAllocation::Backtracking,
        );
        for _ in 0..2 {
            let profile = engine
                .profile_wasm_execution_with_options(&artifact, Some(60_000), false, options)
                .unwrap();
            assert_eq!(profile.options, options);
            assert_eq!(
                profile.outcome.completion,
                ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(42.0)))
            );
            assert_eq!(
                profile.outcome.output_events,
                [HostOutputEvent::PrintLine("42".into())]
            );
            assert!(
                profile.heap.capacity_after_execution_bytes
                    >= options.initial_gc_capacity().bytes()
            );
            assert!(profile.timings.host_import_registration <= profile.timings.store_and_linker);
            assert!(profile.timings.admission <= profile.timings.total);
        }
    }

    #[test]
    fn explicit_16_and_32_mib_controls_access_the_same_matching_image_owner() {
        let engine = Engine::new(RealmBuilder::new().build());
        let artifact = engine
            .profile_script_compilation(
                "var imageTreatmentControl = {answer:42}; imageTreatmentControl.answer;",
                CompileOptions::default(),
            )
            .unwrap()
            .artifact;
        assert!(
            artifact.runtime.is_some(),
            "control must link the real shared runtime"
        );
        for capacity in [
            WasmProfileGcInitialCapacity::MiB16,
            WasmProfileGcInitialCapacity::MiB32,
        ] {
            let options = WasmRuntimeProfileOptions::new(
                WasmProfileNativeMode::Fast,
                capacity,
                WasmProfileRegisterAllocation::SinglePass,
            );
            let native = shared_profile_wasm_engine(options).unwrap();
            let mut previous = WasmRuntimeMatchedImageCacheOutcome::NotAccessed;
            for repeat in 0..2 {
                // Bypass only the Module LRU so this checks actual image-cache
                // access even when a developer's module budget can retain R.
                let modules = wasm_runtime_link::compile_modules(
                    &native,
                    artifact.program(),
                    WasmModuleMemoryCachePolicy::BypassRetention,
                    false,
                    WasmNativeCompilationMode::Fast,
                    WasmRuntimeImagePolicy::MatchedDiagnostic,
                )
                .unwrap();
                assert_ne!(
                    modules.matched_runtime_image_cache,
                    WasmRuntimeMatchedImageCacheOutcome::NotAccessed
                );
                if repeat == 1 && previous != WasmRuntimeMatchedImageCacheOutcome::Oversized {
                    assert_eq!(
                        modules.matched_runtime_image_cache,
                        WasmRuntimeMatchedImageCacheOutcome::Hit
                    );
                }
                previous = modules.matched_runtime_image_cache;
                assert!(WasmtimeEngine::same(
                    modules.runtime.as_ref().unwrap().engine(),
                    &native
                ));
            }
        }
    }
}

#[cfg(test)]
mod engine_observation_tests {
    use super::*;

    #[test]
    fn policy_observation_reports_independent_engine_settings_instead_of_the_request() {
        let requested = WasmRuntimeProfileOptions::new(
            WasmProfileNativeMode::Fast,
            WasmProfileGcInitialCapacity::MiB16,
            WasmProfileRegisterAllocation::SinglePass,
        );
        // Deliberately avoid every Scriptw configuration/cache factory and
        // differ from the request in all three independently observed settings.
        let mut config = WasmtimeConfig::new();
        config.cranelift_opt_level(wasmtime::OptLevel::Speed);
        config.gc_heap_initial_size(64 * 1024 * 1024);
        config.cranelift_regalloc_algorithm(wasmtime::RegallocAlgorithm::Backtracking);
        let engine = WasmtimeEngine::new(&config).unwrap();
        let observed = WasmRuntimeProfileOptions::observe_engine(&engine).unwrap();
        assert_ne!(observed, requested);
        assert_eq!(observed.native_mode(), WasmProfileNativeMode::Speed);
        assert_eq!(
            observed.initial_gc_capacity(),
            WasmProfileGcInitialCapacity::MiB64
        );
        assert_eq!(
            observed.register_allocation(),
            WasmProfileRegisterAllocation::Backtracking
        );
    }

    #[test]
    fn policy_observation_maps_each_supported_native_level_from_an_independent_engine() {
        for (level, capacity, allocator, expected) in [
            (
                wasmtime::OptLevel::None,
                16,
                wasmtime::RegallocAlgorithm::SinglePass,
                WasmRuntimeProfileOptions::new(
                    WasmProfileNativeMode::Fast,
                    WasmProfileGcInitialCapacity::MiB16,
                    WasmProfileRegisterAllocation::SinglePass,
                ),
            ),
            (
                wasmtime::OptLevel::SpeedAndSize,
                32,
                wasmtime::RegallocAlgorithm::Backtracking,
                WasmRuntimeProfileOptions::new(
                    WasmProfileNativeMode::SizeOptimized,
                    WasmProfileGcInitialCapacity::MiB32,
                    WasmProfileRegisterAllocation::Backtracking,
                ),
            ),
            (
                wasmtime::OptLevel::Speed,
                128,
                wasmtime::RegallocAlgorithm::SinglePass,
                WasmRuntimeProfileOptions::new(
                    WasmProfileNativeMode::Speed,
                    WasmProfileGcInitialCapacity::MiB128,
                    WasmProfileRegisterAllocation::SinglePass,
                ),
            ),
        ] {
            let mut config = WasmtimeConfig::new();
            config.cranelift_opt_level(level);
            config.gc_heap_initial_size(capacity * 1024 * 1024);
            config.cranelift_regalloc_algorithm(allocator);
            let engine = WasmtimeEngine::new(&config).unwrap();
            assert_eq!(
                WasmRuntimeProfileOptions::observe_engine(&engine).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn unrepresentable_or_unavailable_engine_settings_cannot_be_reported_as_a_cohort() {
        let mut config = WasmtimeConfig::new();
        config.cranelift_opt_level(wasmtime::OptLevel::None);
        config.gc_heap_initial_size(20 * 1024 * 1024);
        config.cranelift_regalloc_algorithm(wasmtime::RegallocAlgorithm::SinglePass);
        let engine = WasmtimeEngine::new(&config).unwrap();
        let error = WasmRuntimeProfileOptions::observe_engine(&engine).unwrap_err();
        assert!(error
            .to_string()
            .contains("observed initial GC capacity 20971520 bytes"));

        let mut config = WasmtimeConfig::new();
        config.enable_compiler(false);
        config.gc_heap_initial_size(16 * 1024 * 1024);
        let engine = WasmtimeEngine::new(&config).unwrap();
        let error = WasmRuntimeProfileOptions::observe_engine(&engine).unwrap_err();
        assert!(error
            .to_string()
            .contains("cannot observe the executing Engine's native optimization level"));
    }
}
