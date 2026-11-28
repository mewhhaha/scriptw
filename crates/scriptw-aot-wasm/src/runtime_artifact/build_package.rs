//! Build-owned raw R packages retain canonical identity and layout admission.
//!
//! The package is a candidate, never authority for unchecked layout or native
//! code. Its source identity deliberately excludes the eventual executable.
//! Safe candidates receive complete body validation. The explicitly unsafe
//! immutable Cargo boundary can reuse the producer's full body validation.

#[cfg(test)]
use super::emit_runtime_artifact;
use super::validation::FunctionBodyValidation;
use super::{
    cache, emit_runtime_artifact_with_pool, CurrentRuntimeLayoutVersion, RuntimeArtifact,
    RuntimeArtifactCache, RuntimeArtifactKey, RuntimeSourceCompilationProfile, LAYOUT_VERSION,
};
use crate::EmitError;
use sha2::{Digest, Sha256};
use std::sync::Arc;

mod envelope;
mod execution;
mod inputs;
pub(super) mod metadata;
#[cfg(test)]
use envelope::decode_with_jobs;
use envelope::{decode, decode_immutable_build, encode};
pub use inputs::RuntimeArtifactInputs;

const MAGIC: &[u8; 8] = b"SCRWRB01";
const DIGEST_BYTES: usize = 32;

/// Pre-sliced package metadata when the raw Wasm code section has been stripped
/// for `native-host-only` builds (retaining only the data pool and Intl frames).
pub(super) struct StrippedPackageSpans {
    pub(super) wasm_start: usize,
    pub(super) wasm_len: usize,
    pub(super) largest_body: usize,
    pub(super) data: Vec<(usize, usize, [u8; DIGEST_BYTES])>,
    pub(super) frames: Vec<(
        scriptw_intl::IntlDataComponent,
        usize,
        usize,
        [u8; DIGEST_BYTES],
    )>,
}

/// An exact emitted runtime paired with its fully checked raw package. There
/// is no constructor accepting a caller-supplied layout or native image.
pub struct RuntimeBuildArtifact {
    runtime: Arc<RuntimeArtifact>,
    package: Vec<u8>,
    compiler_prefix: Option<Vec<u8>>,
    stripped: Option<StrippedPackageSpans>,
}

impl RuntimeBuildArtifact {
    pub fn wasm(&self) -> &[u8] {
        self.runtime.bytes()
    }
    pub fn key(&self) -> RuntimeArtifactKey {
        self.runtime.key()
    }
    pub fn package_bytes(&self) -> &[u8] {
        &self.package
    }

    /// Compacts the build package for `native-host-only` artifacts by dropping
    /// the raw Wasm code section while keeping the exact data pool segments,
    /// Intl frames, and cryptographic envelope intact. `self.wasm()` continues
    /// to expose the full validated runtime Wasm for native compilation.
    pub fn strip_code_section_for_native_host(&mut self) -> Result<(), EmitError> {
        use scriptw_intl::IntlDataComponent;
        use wasmparser::{Parser, Payload};

        if self.stripped.is_some() {
            return Ok(());
        }
        let orig_offset = self.package.len() - DIGEST_BYTES - self.runtime.bytes().len();
        let orig_wasm = &self.package[orig_offset..self.package.len() - DIGEST_BYTES];
        let mut largest_body = 0usize;
        let mut data_slices = Vec::new();
        let mut frame_slices = std::collections::BTreeMap::new();
        for payload in Parser::new(0).parse_all(orig_wasm) {
            match payload.map_err(|error| EmitError::unsupported(error.to_string()))? {
                Payload::CodeSectionEntry(body) => {
                    largest_body = largest_body.max(body.range().len());
                }
                Payload::DataSection(reader) => {
                    for row in reader {
                        data_slices.push(
                            row.map_err(|error| EmitError::unsupported(error.to_string()))?
                                .data,
                        );
                    }
                }
                Payload::CustomSection(section) => {
                    if let Some(component) = IntlDataComponent::ALL
                        .into_iter()
                        .find(|component| component.section_name() == section.name())
                    {
                        if frame_slices.insert(component, section.data()).is_some() {
                            return Err(EmitError::unsupported("duplicate Intl frame"));
                        }
                    }
                }
                _ => {}
            }
        }
        if data_slices.len() != crate::data::RUNTIME_DATA_SEGMENT_COUNT
            || frame_slices.len() != IntlDataComponent::ALL.len()
        {
            return Err(EmitError::unsupported(
                "incomplete runtime sections for native-host-only package compaction",
            ));
        }

        let payload_len: usize = data_slices.iter().map(|s| s.len()).sum::<usize>()
            + frame_slices.values().map(|s| s.len()).sum::<usize>();
        let mut compacted = Vec::with_capacity(orig_offset + payload_len + DIGEST_BYTES);
        compacted.extend_from_slice(&self.package[..orig_offset - 8]);
        compacted.extend_from_slice(&(payload_len as u64).to_le_bytes());

        let mut data = Vec::with_capacity(data_slices.len());
        for bytes in data_slices {
            let start = compacted.len();
            compacted.extend_from_slice(bytes);
            data.push((start, bytes.len(), Sha256::digest(bytes).into()));
        }
        let mut frames = Vec::with_capacity(IntlDataComponent::ALL.len());
        for component in IntlDataComponent::ALL {
            let bytes = frame_slices[&component];
            let start = compacted.len();
            compacted.extend_from_slice(bytes);
            frames.push((component, start, bytes.len(), Sha256::digest(bytes).into()));
        }
        let envelope_digest = Sha256::digest(&compacted);
        compacted.extend_from_slice(&envelope_digest);

        self.package = compacted;
        self.stripped = Some(StrippedPackageSpans {
            wasm_start: orig_offset,
            wasm_len: payload_len,
            largest_body,
            data,
            frames,
        });
        Ok(())
    }
}

/// Persist an already emitted portable R. This does not consult a cache or
/// emit a replacement. The selected Intl data and every Wasm body/layout are
/// checked by the same safe admission as a disk package.
pub fn encode_portable_runtime_package(
    runtime: &RuntimeArtifact,
    selection: &scriptw_intl::IntlDataSelection,
    compiler_source: &[u8; DIGEST_BYTES],
) -> Result<Vec<u8>, EmitError> {
    if runtime.source_compilation_profile() != RuntimeSourceCompilationProfile::Portable {
        return Err(EmitError::unsupported(
            "portable package requires a portable runtime",
        ));
    }
    let identity =
        cache::RuntimeIdentity::admit(selection, RuntimeSourceCompilationProfile::Portable)?;
    let package = encode(runtime, &identity, compiler_source);
    let admitted = admit_portable_runtime_package(&package, selection, compiler_source)?;
    if admitted.key() != runtime.key() || admitted.layout() != runtime.layout() {
        return Err(EmitError::unsupported(
            "portable package changed the runtime identity or layout",
        ));
    }
    Ok(package)
}

/// Strict safe raw R admission. Invalid, mismatched or missing bytes are an
/// error; this API has no emitter/cache fallback and grants no native compiler
/// or machine-code deserialization authority.
pub fn admit_portable_runtime_package(
    package: &[u8],
    selection: &scriptw_intl::IntlDataSelection,
    compiler_source: &[u8; DIGEST_BYTES],
) -> Result<Arc<RuntimeArtifact>, EmitError> {
    let identity =
        cache::RuntimeIdentity::admit(selection, RuntimeSourceCompilationProfile::Portable)?;
    decode(package, &identity, compiler_source)
        .map(Arc::new)
        .ok_or_else(|| {
            EmitError::unsupported(
                "portable runtime package failed source, Intl, checksum, Wasm or layout admission",
            )
        })
}

/// Emit and round-trip one raw R for a build producer before it precompiles
/// these exact Wasm bytes. The normal executable-bound cache is not consulted.
pub fn build_runtime_artifact(
    selection: &scriptw_intl::IntlDataSelection,
    compiler_source: &[u8; DIGEST_BYTES],
) -> Result<RuntimeBuildArtifact, EmitError> {
    build_runtime_artifact_with_source_compilation_profile(
        selection,
        compiler_source,
        RuntimeSourceCompilationProfile::Portable,
    )
}

/// A native-image producer explicitly selects native source compilation.
/// Portable artifacts use the ordinary checked wrapper above.
pub fn build_runtime_artifact_with_source_compilation_profile(
    selection: &scriptw_intl::IntlDataSelection,
    compiler_source: &[u8; DIGEST_BYTES],
    source_compilation_profile: RuntimeSourceCompilationProfile,
) -> Result<RuntimeBuildArtifact, EmitError> {
    let identity = cache::RuntimeIdentity::admit(selection, source_compilation_profile)?;
    let (runtime, pool) = emit_runtime_artifact_with_pool(selection, source_compilation_profile)?;
    let package = encode(&runtime, &identity, compiler_source);
    let admitted = decode(&package, &identity, compiler_source).ok_or_else(|| {
        let detail = super::validation::explain_wasm_validation(runtime.bytes()).err()
            .unwrap_or_else(|| "Wasm validates, but canonical package metadata/layout admission rejected it".to_owned());
        EmitError::unsupported(format!(
            "compiler invariant: emitted build runtime {source_compilation_profile:?} failed its canonical raw admission: {detail}"
        ))
    })?;
    if admitted.key() != runtime.key() || admitted.layout() != runtime.layout() {
        return Err(EmitError::unsupported(
            "compiler invariant: build runtime round-trip changed its key or typed layout",
        ));
    }
    let data = identity.compilation();
    let compiler_prefix = if source_compilation_profile == RuntimeSourceCompilationProfile::Portable
        && data.identity().profile().profile() == &scriptw_intl::IntlDataProfile::Minimal
        && data.identity().profile().services() == scriptw_intl::IntlServiceSet::ALL
        && data.service_selection().is_none()
    {
        Some(crate::data::CompilerOwnedPool::encode_build_prefix(&pool)?)
    } else {
        None
    };
    Ok(RuntimeBuildArtifact {
        runtime,
        package,
        compiler_prefix,
        stripped: None,
    })
}

#[cfg(test)]
mod tests;
