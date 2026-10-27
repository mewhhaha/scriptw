//! Private static-Cargo execution authority. Receipt decoding alone is inert.
use super::*;
use crate::EngineError;
use scriptw_intl::{IntlCompilationData, IntlDataComponent, IntlDataIdentity};
use scriptw_runtime_image::execution::ExecutionReceipt;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub(crate) struct CargoExecutionRuntime {
    native: &'static EmbeddedNativeRuntime,
    receipt: ExecutionReceipt<'static>,
    identity: IntlDataIdentity,
    compilation: OnceLock<Result<Arc<IntlCompilationData>, String>>,
}

impl CargoExecutionRuntime {
    pub(crate) fn portable() -> Result<Arc<Self>, EngineError> {
        static CURRENT: OnceLock<Result<Arc<CargoExecutionRuntime>, String>> = OnceLock::new();
        CURRENT.get_or_init(|| Self::admit().map(Arc::new).ok_or_else(||
            "Cargo Portable execution receipt failed source/key/layout/Intl/native admission".to_owned()))
            .as_ref().map(Arc::clone).map_err(|error| EngineError::new(error.clone()))
    }

    // No caller bytes, paths, cache entries or claimed digests reach this
    // constructor. Only exact immutable Cargo accessors supply every input.
    fn admit() -> Option<Self> {
        let profile = RuntimeSourceCompilationProfile::Portable;
        let images = Images::for_profile(profile);
        let receipt =
            ExecutionReceipt::decode(scriptw_runtime_image::portable_execution_metadata())?;
        let intl = scriptw_runtime_image::intl_metadata();
        let manifest = manifest::Manifest::decode(images.manifest)?;
        if receipt.source != *expected_source()
            || receipt.layout != scriptw_aot_wasm::RuntimeBuildArtifact::execution_layout_version()
            || receipt.profile != 0
            || receipt.package_length != images.package.len() as u64
            || receipt.key != manifest.runtime_key
            || receipt.package_digest != manifest.package_digest
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
            identity,
            compilation: OnceLock::new(),
        })
    }

    pub(crate) fn identity(&self) -> &IntlDataIdentity {
        &self.identity
    }

    pub(crate) fn load(
        &self,
        engine: &Engine,
        key: scriptw_aot_wasm::RuntimeArtifactKey,
        mode: WasmNativeCompilationMode,
    ) -> Result<Module, EngineError> {
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
        self.native.load_admitted(engine, mode, &mut Default::default()).ok_or_else(||
            EngineError::new("matching Cargo Portable native runtime is unavailable or incompatible; no compiler fallback"))
    }

    pub(crate) fn compilation_data(&self) -> Result<&IntlCompilationData, EngineError> {
        self.compilation
            .get_or_init(|| {
                self.decode_compilation_data()
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .map(Arc::as_ref)
            .map_err(|error| EngineError::new(error.clone()))
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
        let owner = CargoExecutionRuntime::admit().expect("current Cargo execution receipt");
        assert!(owner.compilation.get().is_none());
        assert_eq!(
            owner.identity().profile().profile(),
            &scriptw_intl::IntlDataProfile::Minimal
        );
        assert!(
            owner.compilation.get().is_none(),
            "identity/timezone checks must remain inert"
        );
        let first = owner.compilation_data().unwrap();
        assert_eq!(first.identity(), owner.identity());
        let second = owner.compilation_data().unwrap();
        assert!(std::ptr::eq(first, second));
        assert!(owner.compilation.get().is_some());
    }
    #[test]
    fn lazy_cargo_data_errors_propagate_without_resolving_another_provider() {
        let owner = CargoExecutionRuntime::admit().expect("current Cargo execution receipt");
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
        let owner = CargoExecutionRuntime::admit().expect("current Cargo execution receipt");
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
