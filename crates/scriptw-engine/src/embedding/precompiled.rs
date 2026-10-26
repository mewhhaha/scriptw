//! Explicit native-image ownership for source-free, compiler-disabled embedding.
use super::*;
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
    runtime_key: Option<[u8; 32]>,
    mode: WasmNativeCompilationMode,
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
    pub fn runtime_key(&self) -> Option<&[u8; 32]> {
        self.runtime_key.as_ref()
    }

    /// Recover a persisted image without parsing or compiling JavaScript/Wasm.
    /// Framing/source/checksum checks run before an owner can be constructed.
    ///
    /// # Safety
    ///
    /// The accepted native payload must be the unchanged output of Wasmtime's
    /// `Module::serialize` for the exact accompanying raw program. It must have
    /// been produced through `EmbeddedRuntime::precompile`, with its matching
    /// compiler/runtime identity. Never deserialize attacker-controlled native
    /// bytes. A matching checksum or static lifetime does not establish this
    /// provenance. This has Wasmtime `Module::deserialize`'s native-code trust
    /// requirement, plus the exact raw/native P correspondence requirement.
    pub unsafe fn from_bytes(bytes: &[u8]) -> Result<Self, EngineError> {
        let frame = envelope::decode(bytes)?;
        Ok(Self {
            bytes: Arc::from(bytes),
            program: frame.program,
            native: frame.native,
            runtime_key: frame.runtime_key,
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
            let key = artifact.runtime.as_ref().map(|r| *r.key().as_bytes());
            admission::program(&artifact.bytes, key.is_some())?;
            if let Some(key) = &key {
                admission::cargo_runtime(key)?;
            }
            let runtime = Self::compile(engine, artifact)?;
            let native = runtime.modules.program.serialize().map_err(|error| {
                EngineError::new(format!(
                    "precompiled program serialization failed: {error:#}"
                ))
            })?;
            let bytes = envelope::encode(&artifact.bytes, &native, key, mode)?;
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
            let foreign = admission::program(image.program_wasm(), image.runtime_key.is_some())?;
            // This opaque owner carries the exact successful producer output.
            // Self::compile validated that same raw P before serialization;
            // external recovery requires that correspondence in its unsafe
            // contract. Re-running the full Wasm body validator here faults in
            // compiler-only pages without strengthening native provenance.
            // Portable/linkage/foreign metadata above still admits its actual
            // requirements, and deserialize checks destination compatibility.
            let mut loading = WasmRuntimeModuleLoading::default();
            let (runtime, intl) = match &image.runtime_key {
                Some(key) => {
                    let started = Instant::now();
                    let raw = admission::cargo_runtime(key)?;
                    let intl = wasm_runtime_link::intl_kernel_for(WasmProgramRef::new(
                        image.program_wasm(),
                        Some(&raw),
                    ))?;
                    let timings = loading.embedded_image.get_or_insert_with(Default::default);
                    let module = embedded_runtime::load_native(engine, &raw, image.mode, timings)
                        .ok_or_else(|| EngineError::new("matching Cargo Portable native runtime is unavailable or incompatible; no compiler fallback"))?;
                    loading.runtime = started.elapsed();
                    loading.runtime_cache = Some(WasmRuntimeModuleCacheOutcome::Bypassed);
                    (Some(module), intl)
                }
                // admission::program proved there are no Intl imports/metadata.
                None => (None, intl_data_images::WasmIntlKernel::Absent),
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
