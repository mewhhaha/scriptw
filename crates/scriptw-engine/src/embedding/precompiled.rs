//! Explicit native-image ownership for source-free, compiler-disabled embedding.
use super::*;
use scriptw_aot_wasm::RuntimeArtifactKey;
#[cfg(not(unix))]
use std::fs::File;
use std::ops::Range;
use std::path::Path;
use std::time::Instant;

mod admission;
mod envelope;
#[cfg(all(unix, not(miri)))]
mod file_bytes;
mod foreign_facts;
#[cfg(test)]
mod tests;

/// The normal required Wasmtime runtime, with its native compiler disabled.
/// Apply fuel/deadline settings separately; configuring compiler options after
/// this call is unsupported by Wasmtime. Loading never enables a compiler, so
/// the image's own recorded policy, not this Engine's, determines its code.
pub fn configure_precompiled_wasm_runtime(config: &mut wasmtime::Config) {
    // No compiler runs here; the policy argument is inert once it is disabled,
    // and no compilation job count is chosen for the process.
    configure_runtime_capabilities(config, NativeCompilationPolicy::Fast);
    config.parallel_compilation(false);
    config.enable_compiler(false);
}

/// A native P produced by Wasmtime, its exact raw P metadata, and the identity
/// of the matching Cargo Minimal Portable R. Loading requires no JavaScript
/// source files or source compiler; P can retain function `toString` text.
/// External-native constructors are explicitly unsafe.
#[derive(Clone)]
pub struct PrecompiledEmbeddedProgram {
    backing: ImageBacking,
    program: Range<usize>,
    native: Range<usize>,
    execution_facts: Range<usize>,
    facts: ProgramExecutionFacts,
    mode: WasmNativeCompilationMode,
    image_sha256: std::sync::OnceLock<[u8; 32]>,
}

#[derive(Clone)]
enum ImageBacking {
    Owned(Arc<Vec<u8>>),
    #[cfg(all(unix, not(miri)))]
    File(Arc<file_bytes::FileBytes>),
}

/// Execution facts are minted after real compilation, or recovered structurally
/// inside the unsafe unchanged-producer-envelope boundary. They are never a
/// native-code authority independently of PrecompiledEmbeddedProgram.
#[derive(Clone)]
struct ProgramExecutionFacts {
    foreign: ForeignModuleRegistry,
    runtime: ProgramRuntimeRequirement,
}

#[derive(Clone, Copy)]
enum ProgramRuntimeRequirement {
    Standalone,
    CargoMinimalPortable { key: RuntimeArtifactKey },
}

impl ProgramExecutionFacts {
    fn from_compiled(runtime: &EmbeddedRuntime, key: Option<RuntimeArtifactKey>) -> Self {
        Self {
            foreign: runtime.foreign.clone(),
            runtime: match key {
                None => ProgramRuntimeRequirement::Standalone,
                Some(key) => ProgramRuntimeRequirement::CargoMinimalPortable { key },
            },
        }
    }

    fn runtime_key(&self) -> Option<&RuntimeArtifactKey> {
        match &self.runtime {
            ProgramRuntimeRequirement::Standalone => None,
            ProgramRuntimeRequirement::CargoMinimalPortable { key } => Some(key),
        }
    }
}

impl PrecompiledEmbeddedProgram {
    /// Persist this exact native image. A digest detects corruption; it does
    /// not grant machine-code provenance to an external reader.
    pub fn as_bytes(&self) -> &[u8] {
        match &self.backing {
            ImageBacking::Owned(bytes) => bytes.as_slice(),
            #[cfg(all(unix, not(miri)))]
            ImageBacking::File(bytes) => bytes.as_slice(),
        }
    }
    /// SHA-256 of the complete envelope, including its stored checksum.
    /// Computed lazily on demand so admission never pays SHA-256 cost, and
    /// hashes file-backed images through a temporary mapping so the long-lived
    /// transport view stays unfaulted.
    pub fn image_sha256(&self) -> [u8; 32] {
        *self.image_sha256.get_or_init(|| {
            use sha2::{Digest, Sha256};
            #[cfg(all(unix, not(miri)))]
            if let ImageBacking::File(bytes) = &self.backing {
                // SAFETY: `bytes` already holds the checked nonzero bounded
                // length of an immutable file inode.
                if let Ok(temp) = unsafe {
                    file_bytes::FileBytes::map(Arc::clone(bytes.file()), bytes.as_slice().len())
                } {
                    temp.advise_sequential();
                    return Sha256::digest(temp.as_slice()).into();
                }
            }
            Sha256::digest(self.as_bytes()).into()
        })
    }
    pub fn program_wasm(&self) -> &[u8] {
        &self.as_bytes()[self.program.clone()]
    }
    pub fn serialized_program_len(&self) -> usize {
        self.native.len()
    }
    /// Private binary foreign-registry part of the producer-bound execution facts.
    /// Runtime linkage and mode are in the fixed envelope footer. These bytes
    /// describe requirements and grant no native-code authority.
    pub fn serialized_execution_facts(&self) -> &[u8] {
        &self.as_bytes()[self.execution_facts.clone()]
    }
    pub fn runtime_key(&self) -> Option<&[u8; 32]> {
        self.facts.runtime_key().map(RuntimeArtifactKey::as_bytes)
    }

    /// Recover a persisted image without parsing or compiling JavaScript/Wasm.
    /// Framing/source/checksum checks run before an owner can be constructed.
    ///
    /// # Safety
    ///
    /// The complete envelope must be the unchanged output of
    /// EmbeddedRuntime::precompile from this compiler/runtime identity,
    /// including its execution facts, foreign declarations, raw P and native P.
    /// The native payload must be the exact Module::serialize output for that
    /// accompanying validated raw program. Never recover attacker-controlled
    /// native bytes or rewritten execution facts. Checksums and static lifetime
    /// detect neither forged provenance nor incorrect raw/native correspondence.
    /// This includes Module::deserialize's native-code trust requirement.
    pub unsafe fn from_bytes(bytes: &[u8]) -> Result<Self, EngineError> {
        let frame = envelope::decode(bytes)?;
        Ok(Self::admitted(
            ImageBacking::Owned(Arc::new(bytes.to_vec())),
            frame,
        ))
    }

    /// Recover an image while taking ownership of the existing byte allocation.
    ///
    /// # Safety
    ///
    /// The same native-code provenance contract as [`Self::from_bytes`] applies.
    pub unsafe fn from_vec(bytes: Vec<u8>) -> Result<Self, EngineError> {
        let frame = envelope::decode(&bytes)?;
        Ok(Self::admitted(ImageBacking::Owned(Arc::new(bytes)), frame))
    }

    /// Recover an image from a trusted file. On Unix, Wasmtime maps native P
    /// directly from the admitted inode. Other hosts use owned bytes.
    ///
    /// # Safety
    ///
    /// The same native-code provenance contract as [`Self::from_bytes`] applies.
    /// The caller must keep the opened inode's contents and length unchanged
    /// from before admission through the lifetime of this image and every
    /// derived runtime, module, Store, and instance. A read-only descriptor
    /// alone does not prevent another descriptor from writing or truncating it.
    pub unsafe fn from_file(path: &Path) -> Result<Self, EngineError> {
        let io_error =
            |error| EngineError::new(format!("precompiled image {}: {error}", path.display()));
        #[cfg(unix)]
        let file = {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
                .open(path)
                .map_err(io_error)?
        };
        #[cfg(not(unix))]
        let file = File::open(path).map_err(io_error)?;
        let metadata = file.metadata().map_err(io_error)?;
        let len = usize::try_from(metadata.len()).map_err(|_| envelope::invalid())?;
        if !metadata.is_file() || !(envelope::FOOTER_BYTES..=envelope::MAX_BYTES).contains(&len) {
            return Err(envelope::invalid());
        }

        #[cfg(all(unix, not(miri)))]
        let (backing, frame) = {
            let file = Arc::new(file);
            // SAFETY: caller keeps the inode immutable. This temporary view
            // faults only file-cache pages and is released after verification.
            let checked = unsafe { file_bytes::FileBytes::map(Arc::clone(&file), len) }?;
            checked.advise_sequential();
            let frame = envelope::decode(checked.as_slice())?;
            drop(checked);
            Self::check_file_runtime(&frame)?;
            // SAFETY: same immutable inode and exact admitted length. This
            // fresh, unfaulted view serves borrowed byte accessors on demand.
            let view = unsafe { file_bytes::FileBytes::map(file, len) }?;
            (ImageBacking::File(Arc::new(view)), frame)
        };
        #[cfg(not(all(unix, not(miri))))]
        let (backing, frame) = {
            use std::io::Read;

            let mut bytes = Vec::with_capacity(len);
            file.take(len as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(io_error)?;
            if bytes.len() != len {
                return Err(envelope::invalid());
            }
            let frame = envelope::decode(&bytes)?;
            Self::check_file_runtime(&frame)?;
            (ImageBacking::Owned(Arc::new(bytes)), frame)
        };
        Ok(Self::admitted(backing, frame))
    }

    fn check_file_runtime(frame: &envelope::Frame) -> Result<(), EngineError> {
        if let ProgramRuntimeRequirement::CargoMinimalPortable { key } = frame.facts.runtime {
            embedded_runtime::CargoExecutionRuntime::portable()?
                .check_precompiled(key, frame.mode)?;
        }
        Ok(())
    }

    fn admitted(backing: ImageBacking, frame: envelope::Frame) -> Self {
        Self {
            backing,
            program: frame.program,
            native: frame.native,
            execution_facts: frame.execution_facts,
            facts: frame.facts,
            mode: frame.mode,
            image_sha256: std::sync::OnceLock::new(),
        }
    }

    fn deserialize(&self, engine: &wasmtime::Engine) -> wasmtime::Result<WasmtimeModule> {
        // SAFETY: the opaque owner binds the exact immutable native bytes to
        // their producer. For file backing its constructor also requires the
        // inode to remain immutable for all derived Wasmtime owners.
        unsafe {
            match &self.backing {
                ImageBacking::Owned(bytes) => {
                    WasmtimeModule::deserialize(engine, &bytes[self.native.clone()])
                }
                #[cfg(all(unix, not(miri)))]
                ImageBacking::File(bytes) => {
                    match WasmtimeModule::deserialize_open_file_range(
                        engine,
                        Arc::clone(bytes.file()),
                        0,
                        self.native.len(),
                    ) {
                        Ok(module) => Ok(module),
                        Err(error)
                            if error.chain().any(|cause| {
                                cause
                                    .downcast_ref::<std::io::Error>()
                                    .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
                            }) =>
                        {
                            // A valid image on a `noexec` mount cannot be mapped
                            // with PROT_EXEC; fall back to anonymous copied pages.
                            WasmtimeModule::deserialize(engine, &bytes.as_slice()[self.native.clone()])
                        }
                        Err(error) => Err(error),
                    }
                }
            }
        }
    }
}

impl EmbeddedRuntime {
    /// Compile actual Portable Wasm once and retain its native P for another
    /// process. Linked programs must use this build's exact Cargo Minimal R;
    /// custom profiles and native source-compiler imports are unsupported.
    pub fn precompile(
        engine: &wasmtime::Engine,
        artifact: &Artifact,
    ) -> Result<PrecompiledEmbeddedProgram, EngineError> {
        run_on_sized_stack(|| {
            // Precompiled execution does not attach a collection session.
            if execution_profile::ProfileSites::in_program(&artifact.bytes)? {
                return Err(EngineError::from_execution_profile(
                    ProfileError::CollectingNotPrecompilable,
                ));
            }
            if artifact
                .compilation_policy
                .as_ref()
                .is_some_and(|p| *p.intl_profile() != IntlCompilationProfile::Minimal)
            {
                return Err(EngineError::new(
                    "precompiled embedding requires Minimal Intl",
                ));
            }
            let mode = match engine.get_cranelift_opt_level() {
                Some(wasmtime::OptLevel::None) => WasmNativeCompilationMode::Fast,
                Some(wasmtime::OptLevel::SpeedAndSize) => WasmNativeCompilationMode::SizeOptimized,
                Some(wasmtime::OptLevel::Speed) => WasmNativeCompilationMode::Speed,
                _ => {
                    return Err(EngineError::new(
                        "precompilation requires a supported Wasmtime compiler",
                    ));
                }
            };
            CompilationJobs::verify_engine(engine)?;
            let key = artifact.runtime.as_ref().map(|r| r.key());
            admission::program(&artifact.bytes, key.is_some())?;
            if let Some(key) = &key {
                admission::cargo_runtime(key.as_bytes())?;
            }
            let runtime = Self::compile(engine, artifact)?;
            let native = runtime.modules.program.serialize().map_err(|error| {
                EngineError::new(format!(
                    "precompiled program serialization failed: {error:#}"
                ))
            })?;
            // Only the successfully compiled exact P can mint these facts.
            // Producer admission above established Portable/inert/linkage/Intl
            // requirements; compile admitted the exact foreign signatures.
            let facts = ProgramExecutionFacts::from_compiled(&runtime, key);
            let bytes = envelope::encode(&artifact.bytes, &native, &facts, mode)?;
            // SAFETY: Self::compile compiled this exact P and the matching R.
            // The envelope carries its actual immutable Wasmtime serialization.
            unsafe { PrecompiledEmbeddedProgram::from_vec(bytes) }
        })
    }

    /// Attach precompiled P and the matching Cargo R to an actual Engine whose
    /// compiler is disabled. This never consults a native disk cache or compiles
    /// a replacement on admission/deserialization failure.
    pub fn from_precompiled(
        engine: &wasmtime::Engine,
        image: &PrecompiledEmbeddedProgram,
    ) -> Result<Self, EngineError> {
        run_on_sized_stack(|| {
            if engine.get_cranelift_opt_level().is_some() {
                return Err(EngineError::new(
                    "precompiled embedding requires a compiler-disabled Engine",
                ));
            }
            PRODUCT_WASMTIME_POLICY
                .verify_engine(engine)
                .map_err(EngineError::new)?;
            // The opaque owner binds these producer-validated execution facts
            // to the exact raw/native P. Runtime consumers inspect actual native
            // Module imports/types; no raw-P parser is entered here.
            let foreign = image.facts.foreign.clone();
            let mut loading = WasmRuntimeModuleLoading::default();
            let (runtime, intl) = match image.facts.runtime {
                ProgramRuntimeRequirement::CargoMinimalPortable { key } => {
                    let started = Instant::now();
                    let (module, intl) =
                        wasm_runtime_link::precompiled_minimal_runtime(engine, key, image.mode)?;
                    loading.runtime = started.elapsed();
                    loading.runtime_cache = Some(WasmRuntimeModuleCacheOutcome::Bypassed);
                    (Some(module), intl)
                }
                ProgramRuntimeRequirement::Standalone => {
                    (None, intl_data_images::WasmIntlKernel::Absent)
                }
            };
            let started = Instant::now();
            let program = image.deserialize(engine).map_err(|error| {
                EngineError::new(format!(
                    "precompiled native program is incompatible: {error:#}"
                ))
            })?;
            loading.program = started.elapsed();
            let modules = wasm_runtime_link::WasmModules::new(
                program,
                runtime,
                WasmModuleMemoryCacheOutcome::Bypassed,
                WasmRuntimeMatchedImageCacheOutcome::NotAccessed,
                loading,
            )?;
            Ok(Self::from_admitted_modules(engine, modules, foreign, intl))
        })
    }
}
