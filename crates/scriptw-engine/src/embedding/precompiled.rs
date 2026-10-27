//! Explicit native-image ownership for source-free, compiler-disabled embedding.
use super::*;
use scriptw_aot_wasm::RuntimeArtifactKey;
use std::ops::Range;
use std::time::Instant;

mod admission;
mod envelope;
#[cfg(test)]
mod tests;

/// The normal required Wasmtime runtime, with its native compiler disabled.
/// Apply fuel/deadline settings separately; configuring compiler options after
/// this call is unsupported by Wasmtime. Loading never enables a compiler.
pub fn configure_precompiled_wasm_runtime(config: &mut wasmtime::Config) {
    configure_wasm_runtime(config);
    config.enable_compiler(false);
}

/// A native P produced by Wasmtime, its exact raw P metadata, and the identity
/// of the matching Cargo Minimal Portable R. Loading requires no JavaScript
/// source files or source compiler; P can retain function `toString` text.
/// The only external-native constructor is explicitly unsafe.
#[derive(Clone)]
pub struct PrecompiledEmbeddedProgram {
    bytes: Arc<[u8]>,
    program: Range<usize>,
    native: Range<usize>,
    execution_facts: Range<usize>,
    facts: ProgramExecutionFacts,
    mode: WasmNativeCompilationMode,
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
        &self.bytes
    }
    pub fn program_wasm(&self) -> &[u8] {
        &self.bytes[self.program.clone()]
    }
    pub fn serialized_program_len(&self) -> usize {
        self.native.len()
    }
    /// Canonical foreign-registry part of the producer-bound execution facts.
    /// Runtime linkage and mode are in the fixed envelope header. These bytes
    /// describe requirements and grant no native-code authority.
    pub fn serialized_execution_facts(&self) -> &[u8] {
        &self.bytes[self.execution_facts.clone()]
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
        Ok(Self {
            bytes: Arc::from(bytes),
            program: frame.program,
            native: frame.native,
            execution_facts: frame.execution_facts,
            facts: frame.facts,
            mode: frame.mode,
        })
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
                    ))
                }
            };
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
            // Its actual immutable Wasmtime serialization is copied unchanged.
            unsafe { PrecompiledEmbeddedProgram::from_bytes(&bytes) }
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
            // SAFETY: only actual serialization or the explicitly unsafe
            // external-origin constructor can produce this opaque owner. The
            // exact P native bytes remain immutable throughout deserialization.
            let program =
                unsafe { WasmtimeModule::deserialize(engine, &image.bytes[image.native.clone()]) }
                    .map_err(|error| {
                        EngineError::new(format!(
                            "precompiled native program is incompatible: {error:#}"
                        ))
                    })?;
            loading.program = started.elapsed();
            let modules = wasm_runtime_link::WasmModules {
                program,
                runtime,
                memory_cache_outcome: WasmModuleMemoryCacheOutcome::Bypassed,
                matched_runtime_image_cache: WasmRuntimeMatchedImageCacheOutcome::NotAccessed,
                loading,
            };
            Ok(Self::from_admitted_modules(engine, modules, foreign, intl))
        })
    }
}
