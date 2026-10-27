//! Compact receipt for the exact immutable output of the checked Cargo producer.
//!
//! Parsing this receipt grants no authority by itself. Only the unsafe static
//! package candidate may reuse these producer-admitted facts. Safe packages,
//! caller bytes and caches always take the complete raw admission path.
use super::super::RuntimeLayout;
use super::*;
use crate::data::PoolBoundary;
use scriptw_intl::IntlDataComponent;
use std::ops::Range;

mod decode;
mod encode;
#[cfg(test)]
mod tests;

const METADATA_MAGIC: &[u8; 8] = b"SCRWRM01";
const MAX_METADATA_BYTES: usize = 1024 * 1024;

pub(in crate::runtime_artifact) struct BuildIdentity {
    data: Arc<scriptw_intl::IntlCompilationData>,
    profile: RuntimeSourceCompilationProfile,
    digest: [u8; 32],
}
impl BuildIdentity {
    pub(in crate::runtime_artifact) fn into_parts(
        self,
    ) -> (
        Arc<scriptw_intl::IntlCompilationData>,
        RuntimeSourceCompilationProfile,
        [u8; 32],
    ) {
        (self.data, self.profile, self.digest)
    }
}

#[derive(Clone, Debug)]
struct SpanDigest {
    span: Range<usize>,
    digest: [u8; 32],
}

pub(in crate::runtime_artifact) struct BuildMetadata {
    package: &'static [u8],
    package_digest: [u8; 32],
    wasm: Range<usize>,
    wasm_digest: [u8; 32],
    key: [u8; 32],
    selection_digest: [u8; 32],
    intl_identity: Box<str>,
    profile: RuntimeSourceCompilationProfile,
    layout: RuntimeLayout,
    largest_body: usize,
    data: [SpanDigest; 4],
    frames: Vec<(IntlDataComponent, SpanDigest)>,
}

/// The small facts retained after the checked layout has moved into R.
#[derive(Debug)]
pub(in crate::runtime_artifact) struct BuildAdmission {
    package: &'static [u8],
    package_digest: [u8; 32],
    pub(in crate::runtime_artifact) largest_body: usize,
    data: [SpanDigest; 4],
}

impl BuildAdmission {
    pub(in crate::runtime_artifact) fn package_digest(&self, package: &[u8]) -> Option<[u8; 32]> {
        std::ptr::eq(self.package, package).then_some(self.package_digest)
    }

    pub(in crate::runtime_artifact) fn matches_pool(&self, utf16: &[u8], data: &[u8]) -> bool {
        let prefix = self.data[2].span.len();
        let normalization = self.data[1].span.len();
        let Some(end) = prefix.checked_add(normalization) else {
            return false;
        };
        let Some((head, tail)) = data.split_at_checked(prefix) else {
            return false;
        };
        let Some((normalization_bytes, suffix)) = tail.split_at_checked(end - prefix) else {
            return false;
        };
        [utf16, normalization_bytes, head, suffix]
            .into_iter()
            .zip(&self.data)
            .all(|(bytes, expected)| {
                bytes.len() == expected.span.len()
                    && Sha256::digest(bytes).as_slice() == expected.digest
            })
    }
}

impl BuildMetadata {
    pub(in crate::runtime_artifact) fn frames(&self) -> Vec<(IntlDataComponent, &'static [u8])> {
        self.frames
            .iter()
            .map(|(component, frame)| (*component, &self.package[frame.span.clone()]))
            .collect()
    }

    pub(in crate::runtime_artifact) fn identity(
        &self,
        selection: &scriptw_intl::IntlDataSelection,
    ) -> Result<Option<cache::RuntimeIdentity>, EmitError> {
        let data = selection
            .compilation_data()
            .map_err(|error| EmitError::unsupported(error.to_string()))?;
        if data.service_selection().is_some()
            || data.identity().profile().profile() != &scriptw_intl::IntlDataProfile::Minimal
            || data.identity().profile().services() != scriptw_intl::IntlServiceSet::ALL
            || data.identity().artifact_identity().as_bytes() != self.intl_identity.as_bytes()
        {
            return Ok(None);
        }
        Ok(Some(cache::RuntimeIdentity::from_admitted_build(
            BuildIdentity {
                data: Arc::clone(data),
                profile: self.profile,
                digest: self.selection_digest,
            },
        )))
    }

    pub(in crate::runtime_artifact) fn into_runtime(
        self,
        identity: &cache::RuntimeIdentity,
    ) -> Option<RuntimeArtifact> {
        if identity.digest() != self.selection_digest
            || identity.source_compilation_profile() != self.profile
        {
            return None;
        }
        Some(RuntimeArtifact {
            key: super::super::RuntimeArtifactKey(self.key),
            bytes: std::borrow::Cow::Borrowed(&self.package[self.wasm]),
            wasm_sha256: self.wasm_digest,
            layout: self.layout,
            layout_admission: Some(CurrentRuntimeLayoutVersion::emitted_current()),
            source_compilation_profile: self.profile,
            compilation_data: Some(Arc::clone(identity.compilation())),
            compiler_owned_pool: std::sync::OnceLock::new(),
            build_admission: Some(BuildAdmission {
                package: self.package,
                package_digest: self.package_digest,
                largest_body: self.largest_body,
                data: self.data,
            }),
        })
    }
}
