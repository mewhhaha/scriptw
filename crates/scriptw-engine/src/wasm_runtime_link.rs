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

/// Trusted precompiled P names only this Cargo build's Portable Minimal R.
/// No raw R reconstruction, compiler fallback or caller native blob is admitted.
pub(super) fn precompiled_minimal_runtime(
    engine: &WasmtimeEngine,
    key: RuntimeArtifactKey,
    mode: WasmNativeCompilationMode,
) -> Result<(WasmtimeModule, intl_data_images::WasmIntlKernel), EngineError> {
    let owner = embedded_runtime::CargoExecutionRuntime::portable()?;
    let module = owner.load(engine, key, mode)?;
    Ok((
        module,
        intl_data_images::WasmIntlKernel::cargo_minimal(owner),
    ))
}

#[cfg(test)]
mod import_exports_tests;
mod perf_map;
#[cfg(test)]
mod tests;

/// Raw R and P share one storage owner, budget, pruning and atomic publication
/// policy. Distinct key domains prevent one artifact kind from aliasing another.
pub(super) struct RuntimeWasmCache<'a> {
    cache: &'a cache::FunctionCache,
}

impl<'a> RuntimeWasmCache<'a> {
    pub(super) fn new(cache: &'a cache::FunctionCache) -> Self {
        Self { cache }
    }
}

impl RuntimeArtifactCache for RuntimeWasmCache<'_> {
    fn compiler_identity(&self) -> Option<&[u8; 32]> {
        compiler_fingerprint()
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
    pub(super) profile_verified: bool,
}

impl<'a> WasmProgramRef<'a> {
    /// A standalone module: no runtime module to link against.
    pub(super) fn standalone(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            runtime: None,
            compilation_policy: None,
            profile_verified: false,
        }
    }

    pub(super) fn new(bytes: &'a [u8], runtime: Option<&'a Arc<RuntimeArtifact>>) -> Self {
        Self {
            bytes,
            runtime,
            compilation_policy: None,
            profile_verified: false,
        }
    }

    pub(super) fn with_compilation_policy(mut self, policy: &'a WasmCompilationPolicy) -> Self {
        self.compilation_policy = Some(policy);
        self
    }

    pub(super) fn with_profile_verified(mut self) -> Self {
        self.profile_verified = true;
        self
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
#[cfg(test)]
pub(super) fn decode_cache_entry(
    entry: &[u8],
    intl_profile: &IntlCompilationProfile,
    cache: &cache::FunctionCache,
) -> Option<(Arc<[u8]>, Option<Arc<RuntimeArtifact>>)> {
    decode_cache_entry_with_foreign(entry, intl_profile, cache)
        .map(|(bytes, runtime, _)| (bytes, runtime))
}

/// Decodes an entry, reattaches the runtime for a linked program, and returns
/// its validated foreign-module requirements from the same single Wasm parse.
pub(super) fn decode_cache_entry_with_foreign(
    entry: &[u8],
    intl_profile: &IntlCompilationProfile,
    cache: &cache::FunctionCache,
) -> Option<(
    Arc<[u8]>,
    Option<Arc<RuntimeArtifact>>,
    scriptw_ir::foreign::ForeignModuleRegistry,
)> {
    match entry.split_first()? {
        (&ENTRY_STANDALONE, program) => {
            let (profile, foreign) =
                scriptw_aot_wasm::inspect_linked_program_metadata(program).ok()?;
            (profile == scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable)
                .then(|| (Arc::from(program), None, foreign))
        }
        (&ENTRY_LINKED, rest) => {
            let (recorded, program) = rest.split_at_checked(KEY_BYTES)?;
            let runtime_cache = RuntimeWasmCache::new(cache);
            let inputs = embedded_runtime::inputs(
                Some(&runtime_cache),
                scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost,
            );
            let selection = inputs.intl_selection(intl_profile).ok()?;
            let runtime =
                scriptw_aot_wasm::runtime_artifact_with_inputs(&selection, inputs).ok()?;
            if runtime.key().as_bytes().as_slice() != recorded {
                return None;
            }
            let (profile, foreign) =
                scriptw_aot_wasm::inspect_linked_program_metadata(program).ok()?;
            (profile == runtime.source_compilation_profile())
                .then(|| (Arc::from(program), Some(runtime), foreign))
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
    let runtime_forces_size_optimized = program
        .runtime
        .is_some_and(|runtime| runtime_native_compilation_mode(runtime) == WasmNativeCompilationMode::SizeOptimized);
    if runtime_forces_size_optimized && std::env::var_os("SCRIPTW_WASM_TRACE").is_none() {
        return WasmNativeCompilationPlan {
            mode: WasmNativeCompilationMode::SizeOptimized,
            largest_code_body_bytes: None,
        };
    }
    let mut plan = plan_wasm_native_compilation(program.bytes);
    if runtime_forces_size_optimized {
        plan.mode = WasmNativeCompilationMode::SizeOptimized;
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

enum ProgramImportSource {
    RuntimeExport {
        name: Box<str>,
        export: Option<wasmtime::ModuleExport>,
    },
    RuntimePrivateMemory,
    Linker {
        module: Box<str>,
        name: Box<str>,
    },
}

struct ProgramImportPlan {
    entries: Box<[ProgramImportSource]>,
    has_foreign_imports: bool,
    has_runtime_imports: bool,
    imports_runtime_private_memory: bool,
    runtime_private_memory_export: Option<wasmtime::ModuleExport>,
    program_main_export: Option<wasmtime::ModuleExport>,
    program_module_status_export: Option<wasmtime::ModuleExport>,
    program_prelude_phase_export: Option<wasmtime::ModuleExport>,
    roots_embed_call_export: Option<wasmtime::ModuleExport>,
    roots_embed_call_types: Option<(wasmtime::ArrayType, wasmtime::StructType)>,
    roots_embed_promise_export: Option<(wasmtime::ModuleExport, usize)>,
    roots_snapshot_entry_realm_export: Option<wasmtime::ModuleExport>,
}

fn extract_embed_call_types(
    roots: &WasmtimeModule,
) -> Option<(wasmtime::ArrayType, wasmtime::StructType)> {
    let WasmtimeExternType::Func(ty) = roots.get_export("scriptw.embed.call")? else {
        return None;
    };
    let arguments = ty
        .params()
        .nth(6)
        .and_then(|p| p.as_ref().and_then(|r| r.heap_type().as_concrete_array()).cloned())?;
    let wasmtime::StorageType::ValType(wasmtime::ValType::Ref(element)) = arguments.element_type()
    else {
        return None;
    };
    let stored = element.heap_type().as_concrete_struct().cloned()?;
    Some((arguments, stored))
}

impl ProgramImportPlan {
    fn build(
        program: &WasmtimeModule,
        runtime: Option<&WasmtimeModule>,
        memories: &crate::host_memory_imports::HostMemoryImports,
    ) -> Self {
        let runtime_defines_private_memory = matches!(
            memories.private(),
            crate::host_memory_imports::PrivateMemory::Defined
        );
        let runtime_namespace = scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE;
        let mut entries = Vec::new();
        let mut has_foreign_imports = false;
        let mut has_runtime_imports = false;
        let mut imports_runtime_private_memory = false;
        for import in program.imports() {
            let module_name = import.module();
            let item_name = import.name();
            if !matches!(module_name, crate::WASM_HOST_IMPORT_NAMESPACE)
                && module_name != runtime_namespace
            {
                has_foreign_imports = true;
            }
            if let Some(runtime_module) = runtime {
                if module_name == runtime_namespace {
                    has_runtime_imports = true;
                    entries.push(ProgramImportSource::RuntimeExport {
                        name: item_name.into(),
                        export: runtime_module.get_export_index(item_name),
                    });
                } else if runtime_defines_private_memory
                    && module_name == crate::WASM_HOST_IMPORT_NAMESPACE
                    && item_name == crate::WASM_HOST_IMPORT_PRIVATE_MEMORY
                {
                    imports_runtime_private_memory = true;
                    entries.push(ProgramImportSource::RuntimePrivateMemory);
                } else {
                    entries.push(ProgramImportSource::Linker {
                        module: module_name.into(),
                        name: item_name.into(),
                    });
                }
            }
        }
        let runtime_private_memory_export = runtime.and_then(|runtime_module| {
            runtime_module
                .get_export_index(crate::host_memory_imports::HOST_MODULE_MEMORY_EXPORT)
        });
        let roots_module = runtime.unwrap_or(program);
        let roots_embed_call_export = roots_module.get_export_index("scriptw.embed.call");
        let roots_embed_call_types = roots_embed_call_export
            .as_ref()
            .and_then(|_| extract_embed_call_types(roots_module));
        let roots_embed_promise_export = roots_module
            .get_export_index("scriptw.embed.promise")
            .and_then(|export| {
                let WasmtimeExternType::Func(ty) = roots_module.get_export("scriptw.embed.promise")?
                else {
                    return None;
                };
                let results = ty.results().len();
                Some((export, results))
            });
        Self {
            entries: entries.into_boxed_slice(),
            has_foreign_imports,
            has_runtime_imports,
            imports_runtime_private_memory,
            runtime_private_memory_export,
            program_main_export: program.get_export_index("main"),
            program_module_status_export: program
                .get_export_index(scriptw_aot_wasm::MODULE_EVALUATION_STATUS_EXPORT),
            program_prelude_phase_export: program
                .get_export_index(scriptw_aot_wasm::GLOBAL_SCRIPT_PRELUDE_PHASE_EXPORT),
            roots_embed_call_export,
            roots_embed_call_types,
            roots_embed_promise_export,
            roots_snapshot_entry_realm_export: roots_module
                .get_export_index(scriptw_aot_wasm::SNAPSHOT_ENTRY_REALM_EXPORT),
        }
    }
}

/// The program module, plus the runtime module when linked, both for `mode`.
pub(super) struct WasmModules {
    pub(super) program: WasmtimeModule,
    pub(super) runtime: Option<WasmtimeModule>,
    /// The memories the host module imports, admitted within the Store cap.
    /// Private, so every `WasmModules` comes from [`WasmModules::new`].
    memories: crate::host_memory_imports::HostMemoryImports,
    plan: ProgramImportPlan,
    pub(super) memory_cache_outcome: WasmModuleMemoryCacheOutcome,
    pub(super) matched_runtime_image_cache: WasmRuntimeMatchedImageCacheOutcome,
    pub(super) loading: WasmRuntimeModuleLoading,
}

impl WasmModules {
    /// Admits the host module's memory imports once, for every Store the
    /// modules will serve.
    pub(super) fn new(
        program: WasmtimeModule,
        runtime: Option<WasmtimeModule>,
        memory_cache_outcome: WasmModuleMemoryCacheOutcome,
        matched_runtime_image_cache: WasmRuntimeMatchedImageCacheOutcome,
        loading: WasmRuntimeModuleLoading,
    ) -> Result<Self, EngineError> {
        let memories = crate::host_memory_imports::HostMemoryImports::admit(
            runtime.as_ref().unwrap_or(&program),
        )?;
        let plan = ProgramImportPlan::build(&program, runtime.as_ref(), &memories);
        Ok(Self {
            program,
            runtime,
            memories,
            plan,
            memory_cache_outcome,
            matched_runtime_image_cache,
            loading,
        })
    }

    /// The module that owns the host imports and memory imports.
    pub(super) fn host(&self) -> &WasmtimeModule {
        self.runtime.as_ref().unwrap_or(&self.program)
    }

    /// The memory types to create for the host module's memory imports.
    pub(super) fn memories(&self) -> &crate::host_memory_imports::HostMemoryImports {
        &self.memories
    }

    pub(super) fn has_foreign_imports(&self) -> bool {
        self.plan.has_foreign_imports
    }

    pub(super) fn runtime_private_memory_export(&self) -> Option<&wasmtime::ModuleExport> {
        self.plan.runtime_private_memory_export.as_ref()
    }

    pub(super) fn program_main_export(&self) -> Option<&wasmtime::ModuleExport> {
        self.plan.program_main_export.as_ref()
    }

    pub(super) fn program_module_status_export(&self) -> Option<&wasmtime::ModuleExport> {
        self.plan.program_module_status_export.as_ref()
    }

    pub(super) fn program_prelude_phase_export(&self) -> Option<&wasmtime::ModuleExport> {
        self.plan.program_prelude_phase_export.as_ref()
    }

    pub(super) fn roots_embed_call_export(&self) -> Option<&wasmtime::ModuleExport> {
        self.plan.roots_embed_call_export.as_ref()
    }

    pub(super) fn roots_embed_call_types(
        &self,
    ) -> Option<&(wasmtime::ArrayType, wasmtime::StructType)> {
        self.plan.roots_embed_call_types.as_ref()
    }

    pub(super) fn roots_embed_promise_export(&self) -> Option<(&wasmtime::ModuleExport, usize)> {
        self.plan
            .roots_embed_promise_export
            .as_ref()
            .map(|(export, results)| (export, *results))
    }

    pub(super) fn roots_snapshot_entry_realm_export(&self) -> Option<&wasmtime::ModuleExport> {
        self.plan.roots_snapshot_entry_realm_export.as_ref()
    }
}

/// `bypass_retention` overrides `memory_cache_policy` (agent workers never
/// retain). Both modules share the one bounded in-memory LRU. P admission
/// preserves its admitted R dependency, bypassing P retention if both cannot fit.
pub(super) fn compile_modules(
    engine: &WasmtimeEngine,
    program: WasmProgramRef<'_>,
    memory_cache_policy: WasmModuleMemoryCachePolicy,
    bypass_retention: bool,
    mode: WasmNativeCompilationMode,
    runtime_image_policy: WasmRuntimeImagePolicy,
) -> Result<WasmModules, EngineError> {
    if let Some(runtime) = program.runtime {
        if !program.profile_verified {
            let profile =
                scriptw_aot_wasm::RuntimeSourceCompilationProfile::from_module(program.bytes)
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
    }
    let trace = std::env::var_os("SCRIPTW_WASM_TRACE").is_some();
    let mut matched_runtime_image_cache = WasmRuntimeMatchedImageCacheOutcome::NotAccessed;
    let mut loading = WasmRuntimeModuleLoading::default();
    let runtime_key = program.runtime.map(|runtime| {
        wasm_module_memory_cache_key(
            WasmModuleSource::runtime(runtime, runtime_image_policy),
            mode,
        )
    });
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
                wasm_module_for_execution_with_factory(engine, source, policy, mode, None, || {
                    match source.runtime_image_policy() {
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
                    }
                })?
            }
            WasmModuleSource::Program(_) | WasmModuleSource::ProgramWithDigest(_, _) => {
                wasm_module_for_execution(engine, bytes, policy, mode, runtime_key)?
            }
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
            WasmModuleSource::Program(_) | WasmModuleSource::ProgramWithDigest(_, _) => {
                loading.program = started.elapsed()
            }
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
    let modules = WasmModules::new(
        module,
        runtime,
        memory_cache_outcome,
        matched_runtime_image_cache,
        loading,
    )?;
    perf_map::record(&modules, program);
    Ok(modules)
}

/// Instantiates the runtime module when present, resolving program imports
/// against the live runtime instance by precomputed export index without
/// polluting the caller's linker, then instantiates the program.
/// Returns (program, instance that owns diagnostics and roots).
pub(super) fn instantiate(
    linker: &mut WasmtimeLinker<WasmHostState>,
    store: &mut WasmtimeStore<WasmHostState>,
    modules: &WasmModules,
) -> Result<(wasmtime::Instance, wasmtime::Instance), EngineError> {
    let Some(runtime_module) = &modules.runtime else {
        let program = linker
            .instantiate(&mut *store, &modules.program)
            .map_err(|err| {
                crate::pooled_stores::classify_allocation_error(err, "wasmtime instantiate failed")
            })?;
        return Ok((program, program));
    };

    let runtime = linker
        .instantiate(&mut *store, runtime_module)
        .map_err(|err| {
            crate::pooled_stores::classify_allocation_error(
                err,
                "wasmtime runtime instantiate failed",
            )
        })?;

    // R defines the Store's private memory; P imports it from the host
    // namespace, and native source compilation also resolves host imports
    // through `linker`.
    let mut private_memory = None;
    if modules.plan.imports_runtime_private_memory {
        let memory = modules
            .plan
            .runtime_private_memory_export
            .as_ref()
            .and_then(|export| runtime.get_module_export(&mut *store, export))
            .and_then(WasmtimeExtern::into_memory)
            .ok_or_else(|| EngineError::new("wasmtime runtime exports no private memory"))?;
        linker
            .define(
                &*store,
                crate::WASM_HOST_IMPORT_NAMESPACE,
                crate::WASM_HOST_IMPORT_PRIVATE_MEMORY,
                memory,
            )
            .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
        private_memory = Some(memory);
    }

    let namespace = scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE;
    if modules.plan.has_runtime_imports && linker.has_module(namespace) {
        // Never mask a caller-supplied definition in the runtime namespace.
        // Only runs when the caller actually registered something under `ns`.
        for entry in &modules.plan.entries {
            if let ProgramImportSource::RuntimeExport { name, .. } = entry {
                if linker.has_definition(namespace, name) {
                    return Err(EngineError::new(format!(
                        "wasmtime linker setup failed: import of `{namespace}::{name}` defined twice"
                    )));
                }
            }
        }
    }

    let mut imports = Vec::with_capacity(modules.plan.entries.len());
    for entry in &modules.plan.entries {
        match entry {
            ProgramImportSource::RuntimeExport { name, export } => {
                let export = export
                    .as_ref()
                    .and_then(|export| runtime.get_module_export(&mut *store, export))
                    .ok_or_else(|| {
                        EngineError::new(format!(
                            "wasmtime runtime export missing: {namespace}::{name}"
                        ))
                    })?;
                imports.push(export);
            }
            ProgramImportSource::RuntimePrivateMemory => {
                imports.push(WasmtimeExtern::Memory(
                    private_memory.expect("runtime private memory resolved above"),
                ));
            }
            ProgramImportSource::Linker { module, name } => {
                let item = linker.get(&mut *store, module, name).map_err(|err| {
                    crate::pooled_stores::classify_allocation_error(
                        err,
                        "wasmtime instantiate failed",
                    )
                })?;
                imports.push(item);
            }
        }
    }

    let program = wasmtime::Instance::new(&mut *store, &modules.program, &imports).map_err(
        |err| crate::pooled_stores::classify_allocation_error(err, "wasmtime instantiate failed"),
    )?;
    Ok((program, runtime))
}
