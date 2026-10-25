//! Engine side of the runtime/program module split.
//!
//! A heap-using program is a small program module `P` that imports every
//! export it needs from the program-independent runtime module `R`, under
//! [`RUNTIME_IMPORT_NAMESPACE`]. `R` is identical for every program with the
//! same Intl profile. Raw R uses the bounded program-Wasm disk cache; native R
//! uses the same bounded memory cache as P and Wasmtime's module disk cache.
//! `R` and `P` must belong to the same execution `wasmtime::Engine`. The
//! embedded R image may use its separately admitted build optimization mode;
//! deserialization attaches it to the Engine chosen for P. Retention keys and
//! ordinary fallback compilation always use that execution Engine's mode.

use super::*;
use scriptw_aot_wasm::{
    RuntimeArtifact, RuntimeArtifactCache, RuntimeArtifactCacheKey, RuntimeArtifactKey,
};
use std::collections::VecDeque;

mod perf_map;
#[cfg(test)]
mod tests;

/// Raw R and P share one storage owner, budget, pruning and atomic publication
/// policy. Distinct key domains prevent one artifact kind from aliasing another.
pub(super) struct RuntimeWasmCache<'a> {
    cache: &'a cache::FunctionCache,
    compiler: &'static [u8; 32],
}

impl<'a> RuntimeWasmCache<'a> {
    pub(super) fn new(cache: &'a cache::FunctionCache) -> Option<Self> {
        Some(Self {
            cache,
            compiler: compiler_fingerprint()?,
        })
    }
}

impl RuntimeArtifactCache for RuntimeWasmCache<'_> {
    fn compiler_identity(&self) -> &[u8; 32] {
        self.compiler
    }

    fn load(&self, key: RuntimeArtifactCacheKey) -> Option<Vec<u8>> {
        self.cache.read(key.as_bytes())
    }

    fn store(&self, key: RuntimeArtifactCacheKey, entry: Vec<u8>) -> bool {
        self.cache.write(key.as_bytes(), entry)
    }

    fn remove(&self, key: RuntimeArtifactCacheKey) {
        self.cache.remove(key.as_bytes());
    }
}

/// What the engine carries from an emitted artifact to execution.
#[derive(Clone, Copy)]
pub(super) struct WasmProgramRef<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) runtime: Option<&'a Arc<RuntimeArtifact>>,
    pub(super) compilation_policy: Option<&'a WasmCompilationPolicy>,
}

impl<'a> WasmProgramRef<'a> {
    /// A standalone module: no runtime module to link against.
    pub(super) fn standalone(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            runtime: None,
            compilation_policy: None,
        }
    }

    pub(super) fn new(bytes: &'a [u8], runtime: Option<&'a Arc<RuntimeArtifact>>) -> Self {
        Self {
            bytes,
            runtime,
            compilation_policy: None,
        }
    }

    pub(super) fn with_compilation_policy(mut self, policy: &'a WasmCompilationPolicy) -> Self {
        self.compilation_policy = Some(policy);
        self
    }

    /// The module that owns the host imports, memory imports and Intl images.
    pub(super) fn host_bytes(&self) -> &'a [u8] {
        match self.runtime {
            Some(runtime) => runtime.bytes(),
            None => self.bytes,
        }
    }
}

/// The runtime reference an [`Artifact`] carries. Equality is by key: the key
/// fingerprints the compiler build and Intl profile that fix the bytes.
#[derive(Clone)]
pub struct LinkedRuntime(pub(super) Arc<RuntimeArtifact>);

impl LinkedRuntime {
    pub fn key(&self) -> RuntimeArtifactKey {
        self.0.key()
    }

    pub fn bytes(&self) -> &[u8] {
        self.0.bytes()
    }
}

impl core::fmt::Debug for LinkedRuntime {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("LinkedRuntime").field(&self.0.key()).finish()
    }
}

impl PartialEq for LinkedRuntime {
    fn eq(&self, other: &Self) -> bool {
        self.0.key() == other.0.key()
    }
}

impl Eq for LinkedRuntime {}

// Program-bytes disk cache entries: a tag, then for linked programs the key of
// the runtime the program was emitted against, then the program bytes.
const ENTRY_STANDALONE: u8 = 1;
const ENTRY_LINKED: u8 = 2;
const KEY_BYTES: usize = 32;

pub(super) fn encode_cache_entry(program: WasmProgramRef<'_>) -> Vec<u8> {
    let mut entry = Vec::with_capacity(1 + KEY_BYTES + program.bytes.len());
    match program.runtime {
        Some(runtime) => {
            entry.push(ENTRY_LINKED);
            entry.extend_from_slice(runtime.key().as_bytes());
        }
        None => entry.push(ENTRY_STANDALONE),
    }
    entry.extend_from_slice(program.bytes);
    entry
}

/// Decodes a standalone entry. A linked entry is a miss here: the caller has no
/// way to name the runtime it was emitted against.
pub(super) fn decode_standalone_cache_entry(entry: &[u8]) -> Option<&[u8]> {
    match entry.split_first()? {
        (&ENTRY_STANDALONE, program) => {
            (scriptw_aot_wasm::RuntimeSourceCompilationProfile::from_module(program).ok()?
                == scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable)
                .then_some(program)
        }
        _ => None,
    }
}

/// Decodes an entry and reattaches the runtime for a linked program. An entry
/// recorded against a different runtime key than the current one is a miss.
pub(super) fn decode_cache_entry(
    entry: &[u8],
    intl_profile: &IntlCompilationProfile,
    cache: &cache::FunctionCache,
) -> Option<(Arc<[u8]>, Option<Arc<RuntimeArtifact>>)> {
    match entry.split_first()? {
        (&ENTRY_STANDALONE, program) => {
            (scriptw_aot_wasm::RuntimeSourceCompilationProfile::from_module(program).ok()?
                == scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable)
                .then(|| (Arc::from(program), None))
        }
        (&ENTRY_LINKED, rest) => {
            let (recorded, program) = rest.split_at_checked(KEY_BYTES)?;
            let runtime_cache = RuntimeWasmCache::new(cache)?;
            let inputs = embedded_runtime::inputs(
                Some(&runtime_cache),
                scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost,
            );
            let selection = inputs.intl_selection(intl_profile).ok()?;
            let runtime =
                scriptw_aot_wasm::runtime_artifact_with_inputs(&selection, inputs).ok()?;
            (runtime.key().as_bytes().as_slice() == recorded
                && scriptw_aot_wasm::RuntimeSourceCompilationProfile::from_module(program).ok()?
                    == runtime.source_compilation_profile())
            .then(|| (Arc::from(program), Some(runtime)))
        }
        _ => None,
    }
}

// Keep two hot profiles, as does the raw-runtime residency cache. Active
// execution owns its kernel independently of these retained cache references.
const RETAINED_RUNTIME_PROFILES: usize = 2;

/// The native mode the runtime alone requires, scanned once while resident.
fn runtime_native_compilation_mode(runtime: &RuntimeArtifact) -> WasmNativeCompilationMode {
    static MODES: OnceLock<Mutex<VecDeque<(RuntimeArtifactKey, WasmNativeCompilationMode)>>> =
        OnceLock::new();
    let mut modes = MODES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mode = match modes.iter().position(|(key, _)| *key == runtime.key()) {
        Some(position) => modes.remove(position).unwrap().1,
        None => match runtime.immutable_build_largest_body_bytes() {
            Some(bytes) if bytes >= SIZE_OPTIMIZED_WASM_MIN_CODE_BODY_BYTES => {
                WasmNativeCompilationMode::SizeOptimized
            }
            Some(_) => WasmNativeCompilationMode::Fast,
            None => plan_wasm_native_compilation(runtime.bytes()).mode,
        },
    };
    if modes.len() == RETAINED_RUNTIME_PROFILES {
        modes.pop_front();
    }
    modes.push_back((runtime.key(), mode));
    mode
}

/// Chooses the execution Engine and ordinary fallback mode for both modules:
/// the program's own plan, raised to size-optimized when the runtime needs it.
/// An embedded R image's independent optimization never changes this decision.
pub(super) fn plan_native_compilation(program: WasmProgramRef<'_>) -> WasmNativeCompilationPlan {
    let mut plan = plan_wasm_native_compilation(program.bytes);
    if let Some(runtime) = program.runtime {
        if runtime_native_compilation_mode(runtime) == WasmNativeCompilationMode::SizeOptimized {
            plan.mode = WasmNativeCompilationMode::SizeOptimized;
        }
    }
    plan
}

/// Intl kernel of the module that carries the images, shared across runs of the
/// same runtime so the large module is not re-scanned per run.
pub(super) fn intl_kernel_for(
    program: WasmProgramRef<'_>,
) -> Result<intl_data_images::WasmIntlKernel, EngineError> {
    let Some(runtime) = program.runtime else {
        return intl_data_images::deferred_kernel_for_artifact(program.bytes);
    };
    static KERNELS: OnceLock<
        Mutex<VecDeque<(RuntimeArtifactKey, intl_data_images::WasmIntlKernel)>>,
    > = OnceLock::new();
    let kernels = KERNELS.get_or_init(Default::default);
    {
        let mut kernels = kernels
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(position) = kernels.iter().position(|(key, _)| *key == runtime.key()) {
            let entry = kernels.remove(position).unwrap();
            let kernel = entry.1.clone();
            kernels.push_back(entry);
            return Ok(kernel);
        }
    }
    let kernel = match runtime
        .intl_compilation_data()
        .and_then(|data| intl_data_images::WasmIntlKernel::minimal(Arc::clone(data)))
    {
        Some(kernel) => kernel,
        None => intl_data_images::kernel_for_artifact(runtime.bytes())?.into(),
    };
    let mut kernels = kernels
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // Another caller can finish admission while this caller is decoding.
    let kernel = match kernels.iter().position(|(key, _)| *key == runtime.key()) {
        Some(position) => kernels.remove(position).unwrap().1,
        None => kernel,
    };
    if kernels.len() == RETAINED_RUNTIME_PROFILES {
        kernels.pop_front();
    }
    kernels.push_back((runtime.key(), kernel.clone()));
    Ok(kernel)
}

/// The program module, plus the runtime module when linked, both for `mode`.
pub(super) struct WasmModules {
    pub(super) program: WasmtimeModule,
    pub(super) runtime: Option<WasmtimeModule>,
    pub(super) memory_cache_outcome: WasmModuleMemoryCacheOutcome,
    pub(super) matched_runtime_image_cache: WasmRuntimeMatchedImageCacheOutcome,
    pub(super) loading: WasmRuntimeModuleLoading,
}

impl WasmModules {
    /// The module that owns the host imports and memory imports.
    pub(super) fn host(&self) -> &WasmtimeModule {
        self.runtime.as_ref().unwrap_or(&self.program)
    }
}

/// `bypass_retention` overrides `memory_cache_policy` (agent workers never
/// retain). Both modules share the one bounded in-memory LRU. Execution may
/// borrow both even when retaining P evicts R from that cache.
pub(super) fn compile_modules(
    engine: &WasmtimeEngine,
    program: WasmProgramRef<'_>,
    memory_cache_policy: WasmModuleMemoryCachePolicy,
    bypass_retention: bool,
    mode: WasmNativeCompilationMode,
    runtime_image_policy: WasmRuntimeImagePolicy,
) -> Result<WasmModules, EngineError> {
    if let Some(runtime) = program.runtime {
        let profile = scriptw_aot_wasm::RuntimeSourceCompilationProfile::from_module(program.bytes)
            .map_err(|error| {
                EngineError::new(format!(
                    "program source-compilation admission failed: {error}"
                ))
            })?;
        if profile != runtime.source_compilation_profile() {
            return Err(EngineError::new(
                "program/runtime source-compilation profiles differ",
            ));
        }
    }
    let trace = std::env::var_os("SCRIPTW_WASM_TRACE").is_some();
    let mut matched_runtime_image_cache = WasmRuntimeMatchedImageCacheOutcome::NotAccessed;
    let mut loading = WasmRuntimeModuleLoading::default();
    let mut module_for_execution = |label: &str,
                                    source: WasmModuleSource<'_>|
     -> Result<_, EngineError> {
        let bytes = source.bytes();
        let started = std::time::Instant::now();
        if trace {
            eprintln!(
                "scriptw wasm trace: {label}-module loading: {} bytes",
                bytes.len()
            );
        }
        let policy = if bypass_retention {
            WasmModuleMemoryCachePolicy::BypassRetention
        } else {
            memory_cache_policy
        };
        let result = match source {
            WasmModuleSource::Runtime(runtime) | WasmModuleSource::DiagnosticRuntime(runtime) => {
                wasm_module_for_execution_with_factory(
                    engine,
                    source,
                    policy,
                    mode,
                    || match source.runtime_image_policy() {
                        WasmRuntimeImagePolicy::MatchedDiagnostic => {
                            let (module, outcome) =
                                embedded_runtime::load_capacity_matched_native(engine, runtime)?;
                            matched_runtime_image_cache = outcome;
                            Ok(module)
                        }
                        WasmRuntimeImagePolicy::EmbeddedBuild => {
                            let timings =
                                loading.embedded_image.get_or_insert_with(Default::default);
                            if let Some(module) =
                                embedded_runtime::load_native(engine, runtime, mode, timings)
                            {
                                return Ok(module);
                            }
                            compile_wasm_module(engine, bytes)
                        }
                    },
                )?
            }
            WasmModuleSource::Program(_) => wasm_module_for_execution(engine, bytes, policy, mode)?,
        };
        if trace {
            eprintln!(
                "scriptw wasm trace: {label}-module loaded: {:?}",
                started.elapsed()
            );
        }
        match source {
            WasmModuleSource::Runtime(_) | WasmModuleSource::DiagnosticRuntime(_) => {
                loading.runtime = started.elapsed();
                loading.runtime_cache = Some(result.1.into());
            }
            WasmModuleSource::Program(_) => loading.program = started.elapsed(),
        }
        Ok(result)
    };
    let runtime = program
        .runtime
        .map(|runtime| {
            module_for_execution(
                "runtime",
                WasmModuleSource::runtime(runtime, runtime_image_policy),
            )
            .map(|(module, _)| module)
        })
        .transpose()?;
    let (module, memory_cache_outcome) =
        module_for_execution("program", WasmModuleSource::Program(program.bytes))?;
    let modules = WasmModules {
        program: module,
        runtime,
        memory_cache_outcome,
        matched_runtime_image_cache,
        loading,
    };
    perf_map::record(&modules, program);
    Ok(modules)
}

/// Instantiates the runtime module when present, publishes its exports to the
/// program under [`scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE`], then instantiates
/// the program. Returns (program, instance that owns diagnostics and roots).
pub(super) fn instantiate(
    linker: &mut WasmtimeLinker<WasmHostState>,
    store: &mut WasmtimeStore<WasmHostState>,
    modules: &WasmModules,
) -> Result<(wasmtime::Instance, wasmtime::Instance), EngineError> {
    let runtime = match &modules.runtime {
        Some(runtime_module) => {
            let runtime = linker
                .instantiate(&mut *store, runtime_module)
                .map_err(|err| {
                    EngineError::new(format!("wasmtime runtime instantiate failed: {err:#}"))
                })?;
            linker
                .instance(
                    &mut *store,
                    scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE,
                    runtime,
                )
                .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
            Some(runtime)
        }
        None => None,
    };
    let program = linker
        .instantiate(&mut *store, &modules.program)
        .map_err(|err| EngineError::new(format!("wasmtime instantiate failed: {err:#}")))?;
    Ok((program, runtime.unwrap_or(program)))
}
