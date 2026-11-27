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
use embedded_runtime::CargoExecutionRuntime;
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
    pub(super) runtime: Option<ProgramRuntimeRef<'a>>,
    pub(super) compilation_policy: Option<&'a WasmCompilationPolicy>,
    profile_verified: bool,
}

#[derive(Clone)]
pub(super) enum ProgramRuntime {
    Raw(Arc<RuntimeArtifact>),
    Cargo(&'static Arc<CargoExecutionRuntime>),
}

impl ProgramRuntime {
    pub(super) fn as_ref(&self) -> ProgramRuntimeRef<'_> {
        match self {
            Self::Raw(runtime) => ProgramRuntimeRef::Raw(runtime),
            Self::Cargo(runtime) => ProgramRuntimeRef::Cargo(runtime),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum ProgramRuntimeRef<'a> {
    Raw(&'a Arc<RuntimeArtifact>),
    Cargo(&'static Arc<CargoExecutionRuntime>),
}

impl<'a> ProgramRuntimeRef<'a> {
    pub(super) fn key(self) -> RuntimeArtifactKey {
        match self {
            Self::Raw(runtime) => runtime.key(),
            Self::Cargo(runtime) => runtime.key(),
        }
    }

    pub(super) fn source_compilation_profile(self) -> scriptw_aot_wasm::RuntimeSourceCompilationProfile {
        match self {
            Self::Raw(runtime) => runtime.source_compilation_profile(),
            Self::Cargo(runtime) => runtime.profile(),
        }
    }

    pub(super) fn module_source(self, policy: WasmRuntimeImagePolicy) -> WasmModuleSource<'a> {
        match self {
            Self::Raw(runtime) => WasmModuleSource::runtime(runtime, policy),
            Self::Cargo(runtime) => WasmModuleSource::CargoRuntime(runtime, policy),
        }
    }
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
            runtime: runtime.map(ProgramRuntimeRef::Raw),
            compilation_policy: None,
            profile_verified: false,
        }
    }

    pub(super) fn with_compilation_policy(mut self, policy: &'a WasmCompilationPolicy) -> Self {
        self.compilation_policy = Some(policy);
        self
    }

    // Only compiler output and cache entries whose physical profile was
    // inspected during decode can enter through this constructor.
    pub(super) fn admitted(bytes: &'a [u8], runtime: Option<&'a ProgramRuntime>) -> Self {
        Self {
            bytes,
            runtime: runtime.map(ProgramRuntime::as_ref),
            compilation_policy: None,
            profile_verified: true,
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

/// Wrap the original buffer without moving the Wasm payload past its cache header.
#[derive(Clone)]
pub(super) struct ProgramWasmBytes {
    buffer: Arc<Vec<u8>>,
    offset: usize,
}

impl ProgramWasmBytes {
    pub(super) fn emitted(bytes: Vec<u8>) -> Self {
        Self {
            buffer: Arc::new(bytes),
            offset: 0,
        }
    }

    fn cached(entry: Vec<u8>, program_len: usize) -> Self {
        Self {
            offset: entry.len() - program_len,
            buffer: Arc::new(entry),
        }
    }
}

impl AsRef<[u8]> for ProgramWasmBytes {
    fn as_ref(&self) -> &[u8] {
        &self.buffer[self.offset..]
    }
}

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
pub(crate) fn decode_standalone_cache_entry(entry: &[u8]) -> Option<&[u8]> {
    match entry.split_first()? {
        (&ENTRY_STANDALONE, program) => {
            (scriptw_aot_wasm::RuntimeSourceCompilationProfile::from_module(program).ok()?
                == scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable)
                .then_some(program)
        }
        _ => None,
    }
}

pub(super) fn load_standalone_cache_entry(entry: Vec<u8>) -> Option<ProgramWasmBytes> {
    let len = decode_standalone_cache_entry(&entry)?.len();
    Some(ProgramWasmBytes::cached(entry, len))
}

/// Decodes an entry and reattaches the runtime for a linked program. An entry
/// recorded against a different runtime key than the current one is a miss.
#[cfg(test)]
pub(super) fn decode_cache_entry(
    entry: &[u8],
    intl_profile: &IntlCompilationProfile,
    cache: &cache::FunctionCache,
) -> Option<(ProgramWasmBytes, Option<ProgramRuntime>)> {
    decode_cache_entry_with_foreign(entry.to_vec(), intl_profile, cache)
        .map(|(bytes, runtime, _)| (bytes, runtime))
}

/// Decodes an entry, reattaches the runtime for a linked program, and returns
/// its validated foreign-module requirements from the same single Wasm parse.
pub(super) fn decode_cache_entry_with_foreign(
    entry: Vec<u8>,
    intl_profile: &IntlCompilationProfile,
    cache: &cache::FunctionCache,
) -> Option<(
    ProgramWasmBytes,
    Option<ProgramRuntime>,
    scriptw_ir::foreign::ForeignModuleRegistry,
)> {
    match entry.split_first()? {
        (&ENTRY_STANDALONE, program) => {
            let (profile, foreign) =
                scriptw_aot_wasm::inspect_linked_program_metadata(program).ok()?;
            let len = program.len();
            (profile == scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable)
                .then(|| (ProgramWasmBytes::cached(entry, len), None, foreign))
        }
        (&ENTRY_LINKED, rest) => {
            let (recorded, program) = rest.split_at_checked(KEY_BYTES)?;
            let (profile, foreign) =
                scriptw_aot_wasm::inspect_linked_program_metadata(program).ok()?;
            if profile != scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost {
                return None;
            }
            let cargo = if *intl_profile == IntlCompilationProfile::Minimal {
                CargoExecutionRuntime::native_host()
            } else {
                None
            };
            let runtime = match cargo {
                Some(owner) => ProgramRuntime::Cargo(owner.ok()?),
                None => {
                    let runtime_cache = RuntimeWasmCache::new(cache);
                    let inputs = embedded_runtime::inputs(Some(&runtime_cache), profile);
                    let selection = inputs.intl_selection(intl_profile).ok()?;
                    ProgramRuntime::Raw(
                        scriptw_aot_wasm::runtime_artifact_with_inputs(&selection, inputs).ok()?,
                    )
                }
            };
            let len = program.len();
            (runtime.as_ref().key().as_bytes().as_slice() == recorded)
                .then(|| (ProgramWasmBytes::cached(entry, len), Some(runtime), foreign))
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
    let runtime_forces_size_optimized = program.runtime.is_some_and(|runtime| {
        let mode = match runtime {
            ProgramRuntimeRef::Raw(runtime) => runtime_native_compilation_mode(runtime),
            ProgramRuntimeRef::Cargo(runtime) => runtime.native_mode(),
        };
        mode == WasmNativeCompilationMode::SizeOptimized
    });
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
    let runtime = match runtime {
        ProgramRuntimeRef::Raw(runtime) => runtime,
        ProgramRuntimeRef::Cargo(owner) => {
            return Ok(intl_data_images::WasmIntlKernel::cargo_minimal(Arc::clone(
                owner,
            )));
        }
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
    foreign: Box<[ForeignImport]>,
    /// A scratch allocation for sequential fresh Stores. It is empty whenever
    /// it is in the slot, so this plan cannot retain Store-owned imports.
    reusable_entries: Option<Mutex<Vec<WasmtimeExtern>>>,
    has_runtime_imports: bool,
    imports_runtime_private_memory: bool,
    runtime_private_memory_export: Option<wasmtime::ModuleExport>,
    program_main_export: Option<wasmtime::ModuleExport>,
    program_module_status_export: Option<wasmtime::ModuleExport>,
    program_prelude_phase_export: Option<wasmtime::ModuleExport>,
    roots_embed_promise_export: Option<(wasmtime::ModuleExport, usize)>,
    roots_snapshot_entry_realm_export: Option<wasmtime::ModuleExport>,
}

pub(super) struct ForeignImport {
    pub(super) module: Box<str>,
    pub(super) name: Box<str>,
    pub(super) ty: WasmtimeExternType,
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
        let mut foreign = Vec::new();
        let mut has_runtime_imports = false;
        let mut imports_runtime_private_memory = false;
        for import in program.imports() {
            let module_name = import.module();
            let item_name = import.name();
            if !matches!(module_name, crate::WASM_HOST_IMPORT_NAMESPACE)
                && module_name != runtime_namespace
            {
                foreign.push(ForeignImport {
                    module: module_name.into(),
                    name: item_name.into(),
                    ty: import.ty(),
                });
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
            foreign: foreign.into_boxed_slice(),
            reusable_entries: None,
            has_runtime_imports,
            imports_runtime_private_memory,
            runtime_private_memory_export,
            program_main_export: program.get_export_index("main"),
            program_module_status_export: program
                .get_export_index(scriptw_aot_wasm::MODULE_EVALUATION_STATUS_EXPORT),
            program_prelude_phase_export: program
                .get_export_index(scriptw_aot_wasm::GLOBAL_SCRIPT_PRELUDE_PHASE_EXPORT),
            roots_embed_promise_export,
            roots_snapshot_entry_realm_export: roots_module
                .get_export_index(scriptw_aot_wasm::SNAPSHOT_ENTRY_REALM_EXPORT),
        }
    }

    fn prepare(
        &self,
        linker: &WasmtimeLinker<WasmHostState>,
        program: &WasmtimeModule,
        runtime: &WasmtimeModule,
        shared_memory: Option<&wasmtime::MemoryType>,
    ) -> Option<PreparedProgram> {
        let mut shared_import = None;
        let mut exports = Vec::with_capacity(self.entries.len());
        for (entry, import) in self.entries.iter().zip(program.imports()) {
            let export = match entry {
                ProgramImportSource::RuntimeExport { export, .. } => Some((*export)?),
                ProgramImportSource::RuntimePrivateMemory => {
                    Some(self.runtime_private_memory_export?)
                }
                ProgramImportSource::Linker { module, name }
                    if module.as_ref() == WASM_HOST_IMPORT_NAMESPACE =>
                {
                    if name.as_ref() == WASM_HOST_IMPORT_SHARED_MEMORY {
                        let WasmtimeExternType::Memory(ty) = import.ty() else {
                            return None;
                        };
                        if shared_memory != Some(&ty) {
                            return None;
                        }
                        shared_import =
                            Some((WASM_HOST_IMPORT_NAMESPACE, WASM_HOST_IMPORT_SHARED_MEMORY));
                    } else if !linker.has_definition(module, name) {
                        return None;
                    }
                    None
                }
                ProgramImportSource::Linker { .. } => return None,
            };
            exports.push(export);
        }
        // A statically incompatible import may require live size information.
        // Leave it to the regular path, which also owns linkage diagnostics.
        let instance = linker
            .instantiate_pre_with_module_exports(program, runtime, &exports, shared_import)
            .ok()?;
        Some(PreparedProgram {
            instance,
            imports_shared_memory: shared_import.is_some(),
        })
    }
}

struct PreparedProgram {
    instance: wasmtime::InstancePre<WasmHostState>,
    imports_shared_memory: bool,
}

/// Retained only by the embedding owner of the immutable host linker. Neither
/// proof contains the actual shared memory or any Store-owned extern.
pub(super) struct PreparedLinkage {
    host: Option<wasmtime::InstancePre<WasmHostState>>,
    program: Option<PreparedProgram>,
}

impl PreparedLinkage {
    pub(super) fn new(linker: &WasmtimeLinker<WasmHostState>, modules: &WasmModules) -> Self {
        Self {
            host: host_imports::prepare(linker, modules.host(), modules.memories()).ok(),
            program: modules.runtime.as_ref().and_then(|runtime| {
                modules
                    .plan
                    .prepare(linker, &modules.program, runtime, modules.memories.shared())
            }),
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
    import_free_host: Option<OnceLock<wasmtime::InstancePre<WasmHostState>>>,
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
        let host = runtime.as_ref().unwrap_or(&program);
        let memories = crate::host_memory_imports::HostMemoryImports::admit(host)?;
        let plan = ProgramImportPlan::build(&program, runtime.as_ref(), &memories);
        Ok(Self {
            program,
            runtime,
            memories,
            import_free_host: None,
            plan,
            memory_cache_outcome,
            matched_runtime_image_cache,
            loading,
        })
    }

    pub(super) fn retain_linkage(mut self) -> Self {
        let import_free = self.host().imports().len() == 0;
        self.import_free_host = import_free.then(OnceLock::new);
        self.plan.reusable_entries = self.runtime.is_some().then(Mutex::default);
        self
    }

    /// The module that owns the host imports and memory imports.
    pub(super) fn host(&self) -> &WasmtimeModule {
        self.runtime.as_ref().unwrap_or(&self.program)
    }

    /// P may request fixed host callbacks beyond those directly used by R.
    pub(super) fn additional_host_imports(&self) -> impl Iterator<Item = &str> {
        self.plan.entries.iter().filter_map(|entry| match entry {
            ProgramImportSource::Linker { module, name }
                if module.as_ref() == WASM_HOST_IMPORT_NAMESPACE =>
            {
                Some(name.as_ref())
            }
            _ => None,
        })
    }

    /// The memory types to create for the host module's memory imports.
    pub(super) fn memories(&self) -> &crate::host_memory_imports::HostMemoryImports {
        &self.memories
    }

    pub(super) fn foreign_imports(&self) -> &[ForeignImport] {
        &self.plan.foreign
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
        wasm_module_memory_cache_key(runtime.module_source(runtime_image_policy), mode)
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
            WasmModuleSource::Runtime(_)
            | WasmModuleSource::DiagnosticRuntime(_)
            | WasmModuleSource::CargoRuntime(..) => {
                wasm_module_for_execution_with_factory(engine, source, policy, mode, None, || {
                    match source.runtime_image_policy() {
                        WasmRuntimeImagePolicy::MatchedDiagnostic => {
                            let (module, outcome) = match source {
                                WasmModuleSource::DiagnosticRuntime(runtime) => {
                                    embedded_runtime::load_capacity_matched_native(engine, runtime)?
                                }
                                WasmModuleSource::CargoRuntime(runtime, _) => {
                                    let artifact = runtime.compiler_runtime()?;
                                    embedded_runtime::load_capacity_matched_native(
                                        engine,
                                        artifact.as_ref(),
                                    )?
                                }
                                _ => unreachable!("matched diagnostic runtime source"),
                            };
                            matched_runtime_image_cache = outcome;
                            Ok(module)
                        }
                        WasmRuntimeImagePolicy::EmbeddedBuild => {
                            let timings =
                                loading.embedded_image.get_or_insert_with(Default::default);
                            match source {
                                WasmModuleSource::Runtime(runtime) => {
                                    match embedded_runtime::load_native(
                                        engine, runtime, mode, timings,
                                    ) {
                                        Some(module) => Ok(module),
                                        None => compile_wasm_module(engine, bytes),
                                    }
                                }
                                WasmModuleSource::CargoRuntime(runtime, _) => {
                                    match runtime.load_native(engine, mode, timings) {
                                        Some(module) => Ok(module),
                                        None => compile_wasm_module_with_digest(
                                            engine,
                                            bytes,
                                            Some(runtime.wasm_sha256()),
                                        ),
                                    }
                                }
                                _ => unreachable!("embedded runtime source"),
                            }
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
            WasmModuleSource::Runtime(_)
            | WasmModuleSource::DiagnosticRuntime(_)
            | WasmModuleSource::CargoRuntime(..) => {
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
            module_for_execution("runtime", runtime.module_source(runtime_image_policy))
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
    instantiate_with_linkage(linker, store, modules, None)
}

/// `prepared` is available only while the caller's linker still uses the
/// same fixed host definitions against which preparation ran.
pub(super) fn instantiate_with_linkage(
    linker: &mut WasmtimeLinker<WasmHostState>,
    store: &mut WasmtimeStore<WasmHostState>,
    modules: &WasmModules,
    prepared: Option<&PreparedLinkage>,
) -> Result<(wasmtime::Instance, wasmtime::Instance), EngineError> {
    let context = if modules.runtime.is_some() {
        "wasmtime runtime instantiate failed"
    } else {
        "wasmtime instantiate failed"
    };
    let host_pre = prepared.and_then(|plan| plan.host.as_ref());
    let program_pre = prepared.and_then(|plan| plan.program.as_ref());
    let shared = if modules.memories.shared().is_some()
        && (host_pre.is_some() || program_pre.is_some_and(|pre| pre.imports_shared_memory))
    {
        Some(
            linker
                .get(
                    &mut *store,
                    WASM_HOST_IMPORT_NAMESPACE,
                    WASM_HOST_IMPORT_SHARED_MEMORY,
                )
                .map_err(|error| crate::pooled_stores::classify_allocation_error(error, context))?
                .into_shared_memory()
                .ok_or_else(|| {
                    EngineError::new("wasmtime shared-memory definition is not shared memory")
                })?,
        )
    } else {
        None
    };
    let runtime = match (host_pre, shared.as_ref()) {
        (Some(pre), Some(memory)) => pre.instantiate_with_shared_memory(&mut *store, memory),
        (Some(pre), None) => pre.instantiate(&mut *store),
        (None, _) => host_imports::instantiate(
            linker,
            store,
            modules.host(),
            modules.import_free_host.as_ref(),
        ),
    }
    .map_err(|error| crate::pooled_stores::classify_allocation_error(error, context))?;
    if modules.runtime.is_none() {
        return Ok((runtime, runtime));
    }

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

    if let Some(program) = program_pre {
        let memory = shared.as_ref().filter(|_| program.imports_shared_memory);
        let program = program
            .instance
            .instantiate_with_module_exports(&mut *store, runtime, memory)
            .map_err(|error| {
                crate::pooled_stores::classify_allocation_error(
                    error,
                    "wasmtime instantiate failed",
                )
            })?;
        return Ok((program, runtime));
    }

    let slot = modules.plan.reusable_entries.as_ref();
    let mut imports = slot
        .map(|slot| std::mem::take(&mut *slot.lock().unwrap_or_else(|e| e.into_inner())))
        .unwrap_or_default();
    imports.reserve_exact(modules.plan.entries.len());
    let program = instantiate_linked_program(
        linker,
        store,
        modules,
        runtime,
        private_memory,
        &mut imports,
    );
    if let Some(slot) = slot {
        imports.clear();
        // A concurrent/reentrant entry may have already returned another
        // buffer. Never hold the lock while running Wasm or host code.
        let mut slot = slot.lock().unwrap_or_else(|e| e.into_inner());
        if slot.capacity() < imports.capacity() {
            *slot = imports;
        }
    }
    Ok((program?, runtime))
}

fn instantiate_linked_program(
    linker: &WasmtimeLinker<WasmHostState>,
    store: &mut WasmtimeStore<WasmHostState>,
    modules: &WasmModules,
    runtime: wasmtime::Instance,
    private_memory: Option<WasmtimeMemory>,
    imports: &mut Vec<WasmtimeExtern>,
) -> Result<wasmtime::Instance, EngineError> {
    let namespace = scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE;
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

    wasmtime::Instance::new(&mut *store, &modules.program, imports).map_err(|err| {
        crate::pooled_stores::classify_allocation_error(err, "wasmtime instantiate failed")
    })
}
