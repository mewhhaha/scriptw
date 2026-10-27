//! Producer-only compact execution facts, independent of compiler pool/layout vectors.
use super::*;
use scriptw_intl::IntlDataComponent;
use wasmparser::{Parser, Payload};
#[path = "../../../../scriptw-runtime-image/src/execution.rs"]
mod wire;

impl RuntimeBuildArtifact {
    pub const fn execution_layout_version() -> u32 {
        LAYOUT_VERSION
    }

    /// Only this checked producer can describe an immutable Cargo execution R.
    /// Decoding these wire bytes alone grants no native/build authority.
    pub fn immutable_execution_metadata(
        &self,
        selection: &scriptw_intl::IntlDataSelection,
        source: &[u8; 32],
        intl_metadata: &[u8],
        native_manifest: &[u8],
    ) -> Result<Vec<u8>, EmitError> {
        let data = selection
            .compilation_data()
            .map_err(|error| EmitError::unsupported(error.to_string()))?;
        if data.identity().profile().profile() != &scriptw_intl::IntlDataProfile::Minimal
            || data.service_selection().is_some()
            || data.identity().profile().services() != scriptw_intl::IntlServiceSet::ALL
        {
            return Err(EmitError::unsupported(
                "execution receipt requires complete Minimal Intl",
            ));
        }
        if self.package.get(12..44) != Some(source.as_slice())
            || self
                .runtime
                .intl_compilation_data()
                .is_none_or(|current| current.identity() != data.identity())
        {
            return Err(EmitError::unsupported(
                "execution receipt source/Intl differs from admitted runtime",
            ));
        }
        let start = self.package.len() - 32 - self.wasm().len();
        let wasm = &self.package[start..self.package.len() - 32];
        let mut sections = std::collections::BTreeMap::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::CustomSection(section) =
                payload.map_err(|error| EmitError::unsupported(error.to_string()))?
            {
                if let Some(component) = IntlDataComponent::ALL
                    .into_iter()
                    .find(|c| c.section_name() == section.name())
                {
                    if sections.insert(component, section.data()).is_some() {
                        return Err(EmitError::unsupported("duplicate Intl frame"));
                    }
                }
            }
        }
        let mut frames = Vec::new();
        for component in IntlDataComponent::ALL {
            let bytes = sections
                .remove(&component)
                .ok_or_else(|| EmitError::unsupported("missing Intl frame"))?;
            frames.push(wire::ExecutionFrame {
                component: component.code(),
                start: (bytes.as_ptr().addr() - self.package.as_ptr().addr()) as u64,
                length: bytes.len() as u64,
                digest: Sha256::digest(bytes).into(),
            });
        }
        let identity = data.identity().artifact_identity();
        let receipt = wire::ExecutionReceipt {
            layout: LAYOUT_VERSION,
            profile: self.runtime.source_compilation_profile().wire(),
            source: *source,
            key: *self.key().as_bytes(),
            package_length: self.package.len() as u64,
            wasm_start: start as u64,
            wasm_length: wasm.len() as u64,
            package_digest: Sha256::digest(&self.package).into(),
            envelope_digest: self.package[self.package.len() - 32..].try_into().unwrap(),
            wasm_digest: self.runtime.wasm_sha256(),
            manifest_digest: Sha256::digest(native_manifest).into(),
            intl_digest: Sha256::digest(intl_metadata).into(),
            intl_identity: identity.as_bytes(),
            frames,
        };
        let encoded = receipt
            .encode()
            .ok_or_else(|| EmitError::unsupported("execution receipt exceeds bounds"))?;
        if wire::ExecutionReceipt::decode(&encoded).as_ref() != Some(&receipt) {
            return Err(EmitError::unsupported("execution receipt roundtrip failed"));
        }
        Ok(encoded)
    }
}
