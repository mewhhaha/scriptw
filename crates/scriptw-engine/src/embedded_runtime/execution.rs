//! Private static-Cargo execution authority. Receipt decoding alone is inert.
use super::*;
use crate::EngineError;
use scriptw_aot_wasm::RuntimeArtifactKey;
use scriptw_intl::{IntlCompilationData, IntlDataComponent, IntlDataIdentity};
use scriptw_runtime_image::execution::ExecutionReceipt;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub(crate) struct CargoExecutionRuntime {
    native: &'static EmbeddedNativeRuntime,
    receipt: ExecutionReceipt<'static>,
    wasm: &'static [u8],
    largest_code_body_bytes: u64,
    identity: IntlDataIdentity,
    compilation: OnceLock<Result<Arc<IntlCompilationData>, String>>,
}

impl CargoExecutionRuntime {
    pub(crate) fn portable() -> Result<Arc<Self>, EngineError> {
        Self::current(RuntimeSourceCompilationProfile::Portable).map(Arc::clone)
    }

    pub(crate) fn native_host() -> Option<Result<&'static Arc<Self>, EngineError>> {
        (!scriptw_runtime_image::execution_metadata().is_empty())
            .then(|| {
                let trace = std::env::var_os("SCRIPTW_WASM_TRACE")
                    .map(|_| std::time::Instant::now());
                let admitted = Self::current(RuntimeSourceCompilationProfile::NativeHost);
                if let Some(started) = trace {
                    eprintln!(
                        "scriptw wasm trace: runtime-data admission: {:?}",
                        started.elapsed()
                    );
                }
                admitted
            })
    }

    fn current(
        profile: RuntimeSourceCompilationProfile,
    ) -> Result<&'static Arc<Self>, EngineError> {
        static PORTABLE: OnceLock<Result<Arc<CargoExecutionRuntime>, String>> = OnceLock::new();
        static NATIVE_HOST: OnceLock<Result<Arc<CargoExecutionRuntime>, String>> = OnceLock::new();
        let current = match profile {
            RuntimeSourceCompilationProfile::Portable => &PORTABLE,
            RuntimeSourceCompilationProfile::NativeHost => &NATIVE_HOST,
        };
        current
            .get_or_init(|| {
                Self::admit(profile).map(Arc::new).ok_or_else(|| {
                    format!("Cargo {profile:?} execution receipt failed source/key/layout/Intl/native admission")
                })
            })
            .as_ref()
            .map_err(|error| EngineError::new(error.clone()))
    }

    // No caller bytes, paths, cache entries or claimed digests reach this
    // constructor. Only exact immutable Cargo accessors supply every input.
    fn admit(profile: RuntimeSourceCompilationProfile) -> Option<Self> {
        let images = Images::for_profile(profile);
        let (wire_profile, metadata) = match profile {
            RuntimeSourceCompilationProfile::Portable => {
                (0, scriptw_runtime_image::portable_execution_metadata())
            }
            RuntimeSourceCompilationProfile::NativeHost => {
                (1, scriptw_runtime_image::execution_metadata())
            }
        };
        let (metadata, largest_code_body_bytes) = execution_metadata::decode(metadata)?;
        let receipt = ExecutionReceipt::decode(metadata)?;
        let intl = scriptw_runtime_image::intl_metadata();
        let manifest = manifest::Manifest::decode(images.manifest)?;
        if receipt.source != *expected_source()
            || receipt.layout != scriptw_aot_wasm::RuntimeBuildArtifact::execution_layout_version()
            || receipt.profile != wire_profile
            || receipt.package_length != images.package.len() as u64
            || receipt.key != manifest.runtime_key
            || receipt.package_digest != manifest.package_digest
            || largest_code_body_bytes > receipt.wasm_length
            || Sha256::digest(images.manifest).as_slice() != receipt.manifest_digest
            || Sha256::digest(intl).as_slice() != receipt.intl_digest
            || images
                .package
                .get(images.package.len().checked_sub(32)?..)?
                != receipt.envelope_digest
            || receipt.frames.len() != IntlDataComponent::ALL.len()
            || receipt
                .frames
                .iter()
                .zip(IntlDataComponent::ALL)
                .any(|(frame, component)| frame.component != component.code())
        {
            return None;
        }
        let wasm_start = usize::try_from(receipt.wasm_start).ok()?;
        let wasm_end = wasm_start.checked_add(usize::try_from(receipt.wasm_length).ok()?)?;
        let wasm = images.package.get(wasm_start..wasm_end)?;
        // Check representability/slicing now, without reading component bytes.
        for frame in &receipt.frames {
            let start = usize::try_from(frame.start).ok()?;
            let end = start.checked_add(usize::try_from(frame.length).ok()?)?;
            images.package.get(start..end)?;
        }
        let frame_identities: Vec<_> = receipt
            .frames
            .iter()
            .zip(IntlDataComponent::ALL)
            .map(|(frame, component)| {
                Some((component, usize::try_from(frame.length).ok()?, frame.digest))
            })
            .collect::<Option<_>>()?;
        let identity = IntlCompilationData::build_metadata_identity(
            intl,
            expected_source(),
            &frame_identities,
        )
        .ok()?;
        if identity.artifact_identity().as_bytes() != receipt.intl_identity {
            return None;
        }
        let package_digest = receipt.package_digest;
        let native = native_bundle(profile, || {
            EmbeddedNativeRuntime::admit_manifest_with_digest(images, manifest, || package_digest)
        })?;
        Some(Self {
            native,
            receipt,
            wasm,
            largest_code_body_bytes,
            identity,
            compilation: OnceLock::new(),
        })
    }

    pub(crate) fn profile(&self) -> RuntimeSourceCompilationProfile {
        self.native.images.profile
    }

    pub(crate) fn key(&self) -> RuntimeArtifactKey {
        RuntimeArtifactKey::from_bytes(self.receipt.key)
    }

    pub(crate) fn bytes(&self) -> &'static [u8] {
        self.wasm
    }

    pub(crate) fn wasm_sha256(&self) -> [u8; 32] {
        self.receipt.wasm_digest
    }

    pub(crate) fn native_mode(&self) -> WasmNativeCompilationMode {
        if self.largest_code_body_bytes >= crate::SIZE_OPTIMIZED_WASM_MIN_CODE_BODY_BYTES as u64
        {
            WasmNativeCompilationMode::SizeOptimized
        } else {
            WasmNativeCompilationMode::Fast
        }
    }

    pub(crate) fn identity(&self) -> &IntlDataIdentity {
        &self.identity
    }

    pub(crate) fn load(
        &self,
        engine: &Engine,
        key: RuntimeArtifactKey,
        mode: WasmNativeCompilationMode,
    ) -> Result<Module, EngineError> {
        self.check_precompiled(key, mode)?;
        let mut timings = Default::default();
        self.native.load_admitted(engine, mode, &mut timings).ok_or_else(|| {
            EngineError::new(format!(
                "matching Cargo {:?} native runtime is unavailable or incompatible; no compiler fallback",
                self.profile()
            ))
        })
    }

    pub(crate) fn check_precompiled(
        &self,
        key: RuntimeArtifactKey,
        mode: WasmNativeCompilationMode,
    ) -> Result<(), EngineError> {
        if key.as_bytes() != &self.receipt.key {
            return Err(EngineError::new(
                "precompiled program requires a different Cargo runtime key",
            ));
        }
        if !self.native.manifest.modes.execution.contains(mode) {
            return Err(EngineError::new(
                "Cargo execution mode is unavailable; no compiler fallback",
            ));
        }
        Ok(())
    }

    pub(crate) fn load_native(
        &self,
        engine: &Engine,
        mode: WasmNativeCompilationMode,
        timings: &mut crate::WasmRuntimeEmbeddedImageTimings,
    ) -> Option<Module> {
        if !self.native.manifest.modes.execution.contains(mode) {
            return None;
        }
        self.native.load_admitted(engine, mode, timings)
    }

    pub(crate) fn compilation_data(&self) -> Result<&IntlCompilationData, EngineError> {
        self.compilation_owner().map(Arc::as_ref)
    }

    fn compilation_owner(&self) -> Result<&Arc<IntlCompilationData>, EngineError> {
        self.compilation
            .get_or_init(|| {
                self.decode_compilation_data()
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .map_err(|error| EngineError::new(error.clone()))
    }

    pub(crate) fn compiler_runtime(&self) -> Result<Arc<RuntimeArtifact>, EngineError> {
        let selection = scriptw_intl::IntlDataSelection::from_minimal_compilation_data(Arc::clone(
            self.compilation_owner()?,
        ))
        .ok_or_else(|| EngineError::new("Cargo compiler runtime requires complete Minimal Intl"))?;
        // Reuse the compiler's bounded runtime residency. A compiler layout
        // and its pool must not become permanent through the static receipt.
        let runtime = scriptw_aot_wasm::runtime_artifact_with_inputs(
            &selection,
            inputs(None, self.profile()),
        )
        .map_err(|error| EngineError::new(error.to_string()))?;
        if runtime.key() != self.key()
            || runtime.wasm_sha256() != self.wasm_sha256()
            || runtime.source_compilation_profile() != self.profile()
        {
            return Err(EngineError::new(
                "Cargo compiler runtime differs from admitted execution runtime",
            ));
        }
        Ok(runtime)
    }

    fn decode_compilation_data(&self) -> Result<Arc<IntlCompilationData>, EngineError> {
        let package = self.native.images.package;
        let frames = self
            .receipt
            .frames
            .iter()
            .zip(IntlDataComponent::ALL)
            .map(|(frame, component)| {
                let start = usize::try_from(frame.start).expect("admitted frame start");
                let end = start + usize::try_from(frame.length).expect("admitted frame length");
                (component, &package[start..end])
            })
            .collect();
        // SAFETY: this private owner selected only immutable Cargo producer
        // arrays, checked source/layout/package/native/Intl receipt bindings,
        // exact complete frame codes and in-package bounds. The producer fully
        // admitted all consumers/catalogues before publishing these same spans.
        let build = unsafe {
            scriptw_intl::ImmutableIntlBuildData::new(
                scriptw_runtime_image::intl_metadata(),
                expected_source(),
                frames,
            )
        };
        let selection = scriptw_intl::IntlDataSelection::from_immutable_minimal_build(build)
            .map_err(|error| EngineError::new(error.to_string()))?;
        let data = selection
            .compilation_data()
            .map_err(|error| EngineError::new(error.to_string()))?;
        if data.identity() != self.identity() {
            return Err(EngineError::new("lazy Cargo Intl identity differs"));
        }
        Ok(Arc::clone(data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_execution_admits_identity_before_lazy_catalogue_and_reuses_one_owner() {
        for profile in [
            RuntimeSourceCompilationProfile::Portable,
            RuntimeSourceCompilationProfile::NativeHost,
        ] {
            let owner = CargoExecutionRuntime::admit(profile).expect("current Cargo receipt");
            assert_eq!(owner.profile(), profile);
            assert_eq!(
                owner.identity().profile().profile(),
                &scriptw_intl::IntlDataProfile::Minimal
            );
            let mode = owner.native_mode();
            assert!(owner.compilation.get().is_none());
            let first = owner.compilation_data().unwrap();
            assert_eq!(first.identity(), owner.identity());
            assert!(std::ptr::eq(first, owner.compilation_data().unwrap()));
            let runtime = owner.compiler_runtime().unwrap();
            assert_eq!(runtime.key(), owner.key());
            assert_eq!(
                crate::plan_wasm_native_compilation(runtime.bytes()).mode,
                mode
            );
            assert_eq!(
                crate::largest_wasm_code_body_size(runtime.bytes()).unwrap() as u64,
                owner.largest_code_body_bytes
            );
            assert_eq!(
                first.identity(),
                runtime.intl_compilation_data().unwrap().identity()
            );
        }
    }
    #[test]
    fn lazy_cargo_data_errors_propagate_without_resolving_another_provider() {
        let owner = CargoExecutionRuntime::admit(RuntimeSourceCompilationProfile::Portable)
            .expect("current Cargo execution receipt");
        owner
            .compilation
            .set(Err("injected lazy catalogue failure".into()))
            .unwrap();
        let kernel = crate::intl_data_images::WasmIntlKernel::cargo_minimal(Arc::new(owner));
        assert!(kernel.identity().is_some());
        let mut request = scriptw_intl::IntlHostOp::CanonicalizeLocale
            .wire()
            .to_le_bytes()
            .to_vec();
        request.extend_from_slice(b"EN-us");
        let error = crate::wasm_gc_intl_host::execute_deferred(
            &kernel,
            scriptw_intl::IntlGcHostRequest::decode(&request).unwrap(),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("injected lazy catalogue failure"));
    }

    #[test]
    fn eager_identity_binds_every_frame_digest_without_catalogue_construction() {
        let owner = CargoExecutionRuntime::admit(RuntimeSourceCompilationProfile::Portable)
            .expect("current Cargo execution receipt");
        let mut frames: Vec<_> = owner
            .receipt
            .frames
            .iter()
            .zip(IntlDataComponent::ALL)
            .map(|(frame, component)| (component, frame.length as usize, frame.digest))
            .collect();
        frames[3].2[0] ^= 1;
        assert!(IntlCompilationData::build_metadata_identity(
            scriptw_runtime_image::intl_metadata(),
            expected_source(),
            &frames,
        )
        .is_err());
        assert!(owner.compilation.get().is_none());
    }
}
