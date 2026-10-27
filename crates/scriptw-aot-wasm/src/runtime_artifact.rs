//! The program-independent runtime module (R) of a linked Wasm artifact.
//!
//! R holds every builtin, runtime helper and compiler-owned datum. It is
//! byte-identical for every program compiled by one compiler build and Intl
//! data selection, so hosts compile it once and link each small program
//! module (P) against it. R exports every function and global it defines
//! under a name derived from its index; P imports each of them from
//! [`RUNTIME_IMPORT_NAMESPACE`] in index order. Common host and runtime
//! function indices are identical in R and P. P then appends its declared
//! foreign imports before its own defined bodies; that application-owned
//! suffix never relocates an R function, global or export.

use std::borrow::Cow;
use std::ops::Range;
use std::sync::{Arc, Mutex, OnceLock};

use sha2::{Digest, Sha256};
use wasm_encoder::{EntityType, ExportKind, ExportSection, GlobalType, ImportSection};

use crate::data::{PoolBoundary, StringPool};
use crate::EmitError;

mod build_package;
mod cache;
mod residency;
mod source_compilation;
pub use source_compilation::{
    RuntimeSourceCompilationProfile, RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION,
};
mod validated_profile;
mod validation;

/// Producer P rewriting uses the same required backend feature set as R admission.
pub(crate) fn program_validation_features() -> wasmparser::WasmFeatures {
    validation::required_features()
}

pub use build_package::{
    admit_portable_runtime_package, build_runtime_artifact,
    build_runtime_artifact_with_source_compilation_profile, encode_portable_runtime_package,
    RuntimeArtifactInputs, RuntimeBuildArtifact,
};
pub use cache::{RuntimeArtifactCache, RuntimeArtifactCacheKey};
pub use validated_profile::ValidatedRuntimeArtifact;

/// Module namespace under which a program module imports runtime exports.
pub const RUNTIME_IMPORT_NAMESPACE: &str = "scriptw_runtime";
/// Pure Wasm entry; only fully lowered static-literal proofs select this kernel.
pub const NATIVE_REGEXP_LITERAL_FACTORY_EXPORT: &str = "scriptw.native_regexp_literal";

/// Bumped when the physical or semantic R/P contract changes.
// Version 3 admits whole-Completion tail calls, Identifier PutValue and native ByteArray allocation.
// Version 4 keeps direct program-helper signatures outside the stable core GC group.
// Version 5 adds per-FunctionObject legacy-reflection ownership and its GC state record.
// Version 6 gives each P instance its own typed hook/data owner and function captures.
// Version 7 adds the distinct typed native global-Script compiler core import.
// Version 8 adds native direct eval, its caller context and instance-element hook.
// Version 9 retains the per-Realm host ModuleSource prototype in its own intrinsic slot.
// Version 10 gives native P immutable source operands in a dedicated owner field.
// Version 11 adds typed shared function naming and native RegExp literal kernels.
// Version 12 adds shared array-pattern step, elision and close kernels.
// Version 13 adds the separate shared array-accumulation step-value kernel.
// Version 17 adds complete dense indexed descriptors beside sparse storage.
// Version 18 adds sealed compact ordinary storage, ProgramInstance metadata-only
// shapes/read caches, packed Number arrays, and their typed runtime kernels.
// Its read-cache boundary carries lookup, original receiver and canonical key.
// Version 19 indexes per-program cache metadata directly and gives Map/Set
// cursors stable insertion ordinals across bounded history compaction.
// Version 20 adds the checked retained embedding call/read/checkpoint kernels.
// Version 23 stores prototypes as nullable actual object payload references.
const LAYOUT_VERSION: u32 = 26;

/// A checked current envelope, or the compiler's own current emission.
/// Decoding a raw payload cannot silently mint its version authority.
#[derive(Debug, Clone, Copy)]
struct CurrentRuntimeLayoutVersion(u32);
impl CurrentRuntimeLayoutVersion {
    fn admit(encoded: u32) -> Option<Self> {
        (encoded == LAYOUT_VERSION).then_some(Self(encoded))
    }
    const fn emitted_current() -> Self {
        Self(LAYOUT_VERSION)
    }
}

fn function_export_name(index: u32) -> String {
    format!("f{index}")
}

fn global_export_name(index: u32) -> String {
    format!("g{index}")
}

/// Identity of one runtime module: its bytes, which a compiler build and an
/// Intl data selection fully determine. Equal keys mean byte-identical modules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeArtifactKey([u8; 32]);

impl RuntimeArtifactKey {
    /// Represent a persisted identity. This neither validates runtime bytes nor
    /// grants native-code or Cargo-build admission authority.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// What a program module must import from the runtime, and the data boundary
/// its own pool continues from.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RuntimeLayout {
    imported_functions: u32,
    function_types: Vec<u32>,
    globals: Vec<GlobalType>,
    pool: PoolBoundary,
}

impl RuntimeLayout {
    pub(crate) fn new(
        imported_functions: u32,
        function_types: Vec<u32>,
        globals: Vec<GlobalType>,
        pool: PoolBoundary,
    ) -> Self {
        Self {
            imported_functions,
            function_types,
            globals,
            pool,
        }
    }

    /// A program's pool must continue exactly where the runtime's ended.
    pub(crate) fn require_pool(&self, pool: &StringPool) -> Result<(), EmitError> {
        if pool.compiler_owned_boundary() == self.pool {
            Ok(())
        } else {
            Err(EmitError::unsupported(
                "compiler invariant violated: the program's compiler-owned pool differs from the runtime module's",
            ))
        }
    }

    /// Declares every runtime function, then every runtime global, as imports
    /// in index order for canonical body emission. The final P pass compacts
    /// unused runtime imports after every structural reference exists.
    /// `host_imports` is the host function prefix shared with R.
    pub(crate) fn emit_runtime_imports(
        &self,
        host_imports: u32,
        imports: &mut ImportSection,
    ) -> Result<(), EmitError> {
        if host_imports != self.imported_functions {
            return Err(EmitError::unsupported(
                "compiler invariant violated: the program imports a different host function set than the runtime",
            ));
        }
        // Admit both complete index spaces before appending any import.
        if let Some(last) = self.function_types.len().checked_sub(1) {
            runtime_function_index(host_imports, last)?;
        }
        if let Some(last) = self.globals.len().checked_sub(1) {
            runtime_global_index(last)?;
        }
        for (position, type_index) in self.function_types.iter().enumerate() {
            let index = runtime_function_index(host_imports, position)?;
            imports.import(
                RUNTIME_IMPORT_NAMESPACE,
                &function_export_name(index),
                EntityType::Function(*type_index),
            );
        }
        for (index, global_type) in self.globals.iter().enumerate() {
            imports.import(
                RUNTIME_IMPORT_NAMESPACE,
                &global_export_name(runtime_global_index(index)?),
                EntityType::Global(*global_type),
            );
        }
        Ok(())
    }
}

fn runtime_function_index(host_imports: u32, position: usize) -> Result<u32, EmitError> {
    u32::try_from(position)
        .ok()
        .and_then(|position| host_imports.checked_add(position))
        .ok_or_else(|| {
            EmitError::unsupported("runtime function import index exceeds the Wasm index space")
        })
}
fn runtime_global_index(position: usize) -> Result<u32, EmitError> {
    u32::try_from(position).map_err(|_| {
        EmitError::unsupported("runtime global import index exceeds the Wasm index space")
    })
}

#[cfg(test)]
mod import_index_tests {
    use super::*;
    #[test]
    fn function_prefix_and_global_index_have_independent_checked_spaces() {
        assert_eq!(runtime_function_index(3, 2).unwrap(), 5);
        assert_eq!(runtime_global_index(2).unwrap(), 2);
        assert_eq!(runtime_function_index(u32::MAX, 0).unwrap(), u32::MAX);
        assert!(runtime_function_index(u32::MAX, 1).is_err());
        if let Ok(large) = usize::try_from(u64::from(u32::MAX) + 1) {
            assert!(runtime_function_index(0, large).is_err());
            assert!(runtime_global_index(large).is_err());
        }
    }
}

/// Exports the runtime's functions and globals under their index names.
pub(crate) fn export_runtime_symbols(
    exports: &mut ExportSection,
    functions: Range<u32>,
    globals: u32,
) {
    for index in functions {
        exports.export(&function_export_name(index), ExportKind::Func, index);
    }
    for index in 0..globals {
        exports.export(&global_export_name(index), ExportKind::Global, index);
    }
}

/// Which module of a linked artifact an emission produces.
#[derive(Clone, Copy)]
pub(crate) enum ModuleKind<'r> {
    /// A program without a heap: one self-contained module.
    Standalone,
    /// The program-independent runtime module.
    Runtime(RuntimeSourceCompilationProfile),
    /// A program module that links against exactly this runtime.
    Program(&'r Arc<RuntimeArtifact>),
}

impl ModuleKind<'_> {
    pub(crate) fn source_compilation_profile(self) -> RuntimeSourceCompilationProfile {
        match self {
            Self::Standalone => RuntimeSourceCompilationProfile::Portable,
            Self::Runtime(profile) => profile,
            Self::Program(runtime) => runtime.source_compilation_profile(),
        }
    }

    pub(crate) const fn uses_heap(&self) -> bool {
        match self {
            Self::Standalone => false,
            Self::Runtime(_) | Self::Program(_) => true,
        }
    }

    /// Whether this module defines the runtime half of the function space.
    pub(crate) const fn compiles_runtime(&self) -> bool {
        match self {
            Self::Runtime(_) => true,
            Self::Standalone | Self::Program(_) => false,
        }
    }

    /// Whether this module defines `main` and the program's own bodies.
    pub(crate) const fn compiles_program(&self) -> bool {
        match self {
            Self::Standalone | Self::Program(_) => true,
            Self::Runtime(_) => false,
        }
    }

    /// Whether the module defines the globals the host reads and carries the
    /// Intl data images.
    pub(crate) const fn owns_globals(&self) -> bool {
        match self {
            Self::Standalone | Self::Runtime(_) => true,
            Self::Program(_) => false,
        }
    }
}

/// One emitted module and, for the runtime module, what programs import.
pub(crate) struct EmittedModule {
    pub(crate) artifact: crate::WasmArtifact,
    pub(crate) runtime_layout: Option<RuntimeLayout>,
}

pub struct RuntimeArtifact {
    key: RuntimeArtifactKey,
    // The artifact's outer Arc shares owned bytes. Immutable Cargo packages
    // can instead retain their validated static payload without a second R.
    bytes: Cow<'static, [u8]>,
    wasm_sha256: [u8; 32],
    layout: RuntimeLayout,
    layout_admission: Option<CurrentRuntimeLayoutVersion>,
    source_compilation_profile: RuntimeSourceCompilationProfile,
    compilation_data: Option<Arc<scriptw_intl::IntlCompilationData>>,
    compiler_owned_pool: OnceLock<Result<Arc<crate::data::CompilerOwnedPool>, EmitError>>,
    build_admission: Option<build_package::metadata::BuildAdmission>,
}

impl RuntimeArtifact {
    fn new(
        bytes: Cow<'static, [u8]>,
        layout: RuntimeLayout,
        version: CurrentRuntimeLayoutVersion,
        source_compilation_profile: RuntimeSourceCompilationProfile,
        compilation_data: Arc<scriptw_intl::IntlCompilationData>,
    ) -> Self {
        Self {
            key: runtime_key(&bytes),
            wasm_sha256: Sha256::digest(&bytes).into(),
            bytes,
            layout,
            layout_admission: Some(version),
            source_compilation_profile,
            compilation_data: Some(compilation_data),
            compiler_owned_pool: OnceLock::new(),
            build_admission: None,
        }
    }

    /// Synthetic cache/residency fixtures carry no product layout authority.
    #[cfg(test)]
    fn unadmitted_fixture(bytes: Vec<u8>, layout: RuntimeLayout) -> Self {
        Self {
            key: runtime_key(&bytes),
            wasm_sha256: Sha256::digest(&bytes).into(),
            bytes: Cow::Owned(bytes),
            layout,
            layout_admission: None,
            source_compilation_profile: RuntimeSourceCompilationProfile::Portable,
            compilation_data: None,
            compiler_owned_pool: OnceLock::new(),
            build_admission: None,
        }
    }

    pub const fn source_compilation_profile(&self) -> RuntimeSourceCompilationProfile {
        self.source_compilation_profile
    }

    /// Compiler metadata whose exact physical frames passed this R's admission.
    pub fn intl_compilation_data(&self) -> Option<&Arc<scriptw_intl::IntlCompilationData>> {
        self.compilation_data.as_ref()
    }

    pub fn key(&self) -> RuntimeArtifactKey {
        self.key
    }

    /// Exact admitted immutable Wasm, shared through this runtime's owner.
    pub fn bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }

    /// SHA-256 of the exact immutable Wasm bytes, without the layout-key prefix.
    /// Computed once when runtime bytes are emitted or fully admitted.
    pub fn wasm_sha256(&self) -> [u8; 32] {
        self.wasm_sha256
    }

    /// Digest of this exact immutable Cargo package, previously checked by
    /// its producer. Another slice, even with equal length, grants no proof.
    pub fn immutable_build_package_digest(&self, package: &[u8]) -> Option<[u8; 32]> {
        self.build_admission.as_ref()?.package_digest(package)
    }

    /// Producer-admitted body extent for native planning without paging R in.
    pub fn immutable_build_largest_body_bytes(&self) -> Option<usize> {
        Some(self.build_admission.as_ref()?.largest_body)
    }

    pub(crate) fn admitted_compiler_pool_matches(&self, utf16: &[u8], data: &[u8]) -> Option<bool> {
        Some(self.build_admission.as_ref()?.matches_pool(utf16, data))
    }

    fn require_intl_profile(
        &self,
        selection: &scriptw_intl::IntlDataSelection,
    ) -> Result<(), EmitError> {
        let selected = selection
            .compilation_data()
            .map_err(|error| EmitError::unsupported(error.to_string()))?;
        if self.build_admission.is_some()
            && self
                .compilation_data
                .as_ref()
                .is_some_and(|data| Arc::ptr_eq(data, selected))
        {
            return Ok(());
        }
        crate::runtime_function::runtime_profile::require_runtime_profile(self.bytes(), selection)
    }

    pub(crate) fn layout(&self) -> &RuntimeLayout {
        &self.layout
    }
}

impl core::fmt::Debug for RuntimeArtifact {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RuntimeArtifact")
            .field("key", &self.key)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

impl PartialEq for RuntimeArtifact {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for RuntimeArtifact {}

/// The runtime module for `selection`, shared while resident in the bounded
/// process cache or while an emission for that selection is in flight.
pub fn runtime_artifact(
    selection: &scriptw_intl::IntlDataSelection,
) -> Result<Arc<RuntimeArtifact>, EmitError> {
    runtime_artifact_with_inputs(selection, RuntimeArtifactInputs::new(None))
}

/// Reuses a validated raw runtime artifact before falling back to emission.
/// Intl selection/admission always precedes either memory or disk cache lookup.
pub fn runtime_artifact_with_cache(
    selection: &scriptw_intl::IntlDataSelection,
    cache: Option<&dyn RuntimeArtifactCache>,
) -> Result<Arc<RuntimeArtifact>, EmitError> {
    runtime_artifact_with_inputs(selection, RuntimeArtifactInputs::new(cache))
}

/// Admit the current Intl selection before consulting either an untrusted raw
/// build-package candidate or the ordinary compiler/executable-bound cache.
pub fn runtime_artifact_with_inputs(
    selection: &scriptw_intl::IntlDataSelection,
    inputs: RuntimeArtifactInputs<'_>,
) -> Result<Arc<RuntimeArtifact>, EmitError> {
    static RUNTIMES: OnceLock<Mutex<residency::RuntimeResidency>> = OnceLock::new();
    let admission_started = std::time::Instant::now();
    let identity = inputs.admit_identity(selection)?;
    if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
        eprintln!(
            "scriptw wasm trace: runtime-data admission: {:?}",
            admission_started.elapsed()
        );
    }
    let runtimes = RUNTIMES.get_or_init(Default::default);
    let digest = identity.digest();
    let slot = runtimes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .slot(digest);
    // Initialization can compile a large module. Other profiles must not wait
    // for it under the cache lock; concurrent callers of this profile share it.
    let result = slot
        .get_or_init(|| {
            inputs.load_or_emit(&identity, || {
                emit_runtime_artifact(selection, inputs.source_compilation_profile())
            })
        })
        .clone();
    runtimes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .retain(digest, &slot);
    result
}

fn emit_runtime_artifact(
    selection: &scriptw_intl::IntlDataSelection,
    source_compilation_profile: RuntimeSourceCompilationProfile,
) -> Result<Arc<RuntimeArtifact>, EmitError> {
    let parsed = scriptw_front::parse(";", scriptw_front::ParseOptions::script())
        .map_err(|_| EmitError::unsupported("the empty runtime script must parse"))?;
    let script = scriptw_ir::lower(&parsed)
        .script
        .ok_or_else(|| EmitError::unsupported("the empty runtime script must lower"))?;
    let module = crate::emit::emit_runtime_module(&script, selection, source_compilation_profile)?;
    let layout = module
        .runtime_layout
        .ok_or_else(|| EmitError::unsupported("the runtime module must describe its layout"))?;
    let compilation = selection
        .compilation_data()
        .map_err(|error| EmitError::unsupported(error.to_string()))?;
    Ok(Arc::new(RuntimeArtifact::new(
        Cow::Owned(module.artifact.bytes),
        layout,
        CurrentRuntimeLayoutVersion::emitted_current(),
        source_compilation_profile,
        Arc::clone(compilation),
    )))
}

fn runtime_key(bytes: &[u8]) -> RuntimeArtifactKey {
    let mut hasher = Sha256::new();
    hasher.update(LAYOUT_VERSION.to_le_bytes());
    hasher.update(bytes);
    RuntimeArtifactKey(hasher.finalize().into())
}
