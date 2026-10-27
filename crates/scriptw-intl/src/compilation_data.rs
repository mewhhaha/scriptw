//! Compiler metadata is independent of the loaded operation providers.
//!
//! Only the immutable Cargo producer can substitute its previously admitted
//! catalogues for admission by the actual consumers in this process.
use crate::{
    CanonicalLocaleId, CheckedIntlServiceSelection, EmbeddedIntlProviderSetupError,
    IntlBundleExportError, IntlDataComponent, IntlDataDigest, IntlDataIdentity, IntlDataPlacement,
    IntlDataProfile, IntlDataSelection, IntlProfilePlan, IntlServiceSet, SelectedIntlDataBundle,
    SupportedValuesKey, SupportedValuesList, SupportedValuesSetupError,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

mod execution;

const MAGIC: &[u8; 8] = b"SCRWIC01";
const MAX_METADATA: usize = 256 * 1024;

/// Immutable physical frames, shared or borrowed from a validated build image.
#[derive(Clone)]
pub enum IntlCompilationBytes {
    Shared(Arc<[u8]>),
    Build(&'static [u8]),
}
impl core::fmt::Debug for IntlCompilationBytes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("IntlCompilationBytes")
            .field("len", &self.as_ref().len())
            .field("borrowed_build", &matches!(self, Self::Build(_)))
            .finish()
    }
}
impl AsRef<[u8]> for IntlCompilationBytes {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Shared(bytes) => bytes,
            Self::Build(bytes) => bytes,
        }
    }
}
impl PartialEq for IntlCompilationBytes {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}
impl Eq for IntlCompilationBytes {}

/// Actual frame identity and consumer-admitted constant catalogues, without
/// requiring formatter/provider ownership during compilation or non-Intl runs.
#[derive(Debug)]
pub struct IntlCompilationData {
    pub(crate) minimal_consumers: crate::minimal_consumer::MinimalConsumers,
    identity: IntlDataIdentity,
    services: Option<CheckedIntlServiceSelection>,
    frames: Vec<(IntlDataComponent, IntlCompilationBytes)>,
    values: [Option<Arc<SupportedValuesList>>; 6],
}
impl IntlCompilationData {
    pub(crate) fn from_selected(
        selected: &SelectedIntlDataBundle,
    ) -> Result<Self, EmbeddedIntlProviderSetupError> {
        let mut values = std::array::from_fn(|_| None);
        for key in SupportedValuesKey::ALL {
            values[key.index()] = match selected.supported_values(key) {
                Ok(list) => Some(list),
                Err(SupportedValuesSetupError::UnavailableData(missing)) if *missing == key => None,
                Err(error) => {
                    return Err(EmbeddedIntlProviderSetupError::DataImage(
                        crate::IntlDataImageError::consumer(error),
                    ))
                }
            };
        }
        Ok(Self {
            minimal_consumers: Default::default(),
            identity: selected.identity().clone(),
            services: selected.service_selection().cloned(),
            frames: selected
                .component_images()
                .into_iter()
                .map(|(component, bytes)| (component, IntlCompilationBytes::Shared(bytes)))
                .collect(),
            values,
        })
    }
    pub fn identity(&self) -> &IntlDataIdentity {
        &self.identity
    }
    pub fn service_selection(&self) -> Option<&CheckedIntlServiceSelection> {
        self.services.as_ref()
    }
    pub fn component_sections(&self) -> Vec<(&'static str, IntlCompilationBytes)> {
        self.frames
            .iter()
            .map(|(component, bytes)| (component.section_name(), bytes.clone()))
            .collect()
    }
    pub fn supported_values(
        &self,
        key: SupportedValuesKey,
    ) -> Result<Arc<SupportedValuesList>, SupportedValuesSetupError> {
        self.values[key.index()]
            .as_ref()
            .map(Arc::clone)
            .ok_or(SupportedValuesSetupError::UnavailableData(key))
    }
}

impl SelectedIntlDataBundle {
    /// Export only small metadata. Physical component frames remain in R;
    /// this does not embed another copy of the component data in the executable.
    pub fn minimal_build_metadata(
        &self,
        source: &[u8; 32],
    ) -> Result<Vec<u8>, IntlBundleExportError> {
        if self.identity().profile().profile() != &IntlDataProfile::Minimal
            || self.service_selection().is_some()
            || self.identity().profile().services() != IntlServiceSet::ALL
        {
            return Err(IntlBundleExportError::Identity);
        }
        fn field(out: &mut Vec<u8>, bytes: &[u8]) {
            out.extend_from_slice(
                &u32::try_from(bytes.len())
                    .expect("bounded metadata field")
                    .to_le_bytes(),
            );
            out.extend_from_slice(bytes);
        }
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(source);
        out.extend_from_slice(self.identity().digest().as_bytes());
        field(
            &mut out,
            self.identity().default_locale().as_str().as_bytes(),
        );
        field(&mut out, self.identity().artifact_identity().as_bytes());
        let frames = self.component_images();
        if frames.len() != IntlDataComponent::ALL.len() {
            return Err(IntlBundleExportError::Identity);
        }
        for (expected, (component, bytes)) in IntlDataComponent::ALL.into_iter().zip(frames) {
            if component != expected {
                return Err(IntlBundleExportError::Identity);
            }
            out.extend_from_slice(&component.code().to_le_bytes());
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(&Sha256::digest(&bytes));
        }
        for key in SupportedValuesKey::ALL {
            let list = self
                .supported_values(key)
                .map_err(|error| IntlBundleExportError::Catalogue(error.to_string()))?;
            if list.provider_identity() != self.identity() {
                return Err(IntlBundleExportError::Identity);
            }
            out.extend_from_slice(&(list.values().len() as u32).to_le_bytes());
            for value in list.values() {
                field(&mut out, value.as_bytes());
            }
        }
        if out.len() + 32 > MAX_METADATA {
            return Err(IntlBundleExportError::TooLarge);
        }
        out.extend_from_slice(&Sha256::digest(&out));
        Ok(out)
    }
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], IntlBundleExportError> {
        let (head, tail) = self
            .0
            .split_at_checked(n)
            .ok_or(IntlBundleExportError::Framing(
                "truncated compiler metadata",
            ))?;
        self.0 = tail;
        Ok(head)
    }
    fn u32(&mut self) -> Result<usize, IntlBundleExportError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize)
    }
    fn field(&mut self) -> Result<&'a [u8], IntlBundleExportError> {
        let n = self.u32()?;
        self.take(n)
    }
    fn text(&mut self) -> Result<&'a str, IntlBundleExportError> {
        std::str::from_utf8(self.field()?)
            .map_err(|_| IntlBundleExportError::Framing("non-UTF8 compiler metadata"))
    }
}

impl IntlDataSelection {
    /// Reuse this compiler's immutable Minimal metadata and physical frames.
    ///
    /// The opaque token is constructed only at the explicit Cargo trust seam;
    /// all source, frame and canonical metadata checks below remain safe Rust.
    pub fn from_immutable_minimal_build(
        build: crate::ImmutableIntlBuildData<IntlDataComponent>,
    ) -> Result<Self, IntlBundleExportError> {
        let (metadata, source, frames) = build.into_parts();
        Self::from_minimal_compilation_data(Arc::new(decode_metadata(
            metadata,
            source,
            frames,
            FrameValidation::AdmittedAtBuild,
        )?))
        .ok_or(IntlBundleExportError::Identity)
    }
}

#[derive(Clone, Copy)]
enum FrameValidation {
    #[cfg(test)]
    AtUse,
    AdmittedAtBuild,
}

#[cfg(test)]
fn decode_build_metadata(
    metadata: &[u8],
    source: &[u8; 32],
    frames: Vec<(IntlDataComponent, &'static [u8])>,
) -> Result<IntlCompilationData, IntlBundleExportError> {
    decode_metadata(metadata, source, frames, FrameValidation::AtUse)
}

fn decode_metadata(
    metadata: &[u8],
    source: &[u8; 32],
    frames: Vec<(IntlDataComponent, &'static [u8])>,
    frame_validation: FrameValidation,
) -> Result<IntlCompilationData, IntlBundleExportError> {
    let (identity, mut r) = execution::identity_prefix(metadata, source)?;
    if frames.len() != IntlDataComponent::ALL.len() {
        return Err(IntlBundleExportError::Identity);
    }
    let mut admitted = Vec::with_capacity(IntlDataComponent::ALL.len());
    for (expected, (component, bytes)) in IntlDataComponent::ALL.into_iter().zip(frames) {
        if component != expected
            || r.take(2)? != component.code().to_le_bytes()
            || r.u32()? != bytes.len()
        {
            return Err(IntlBundleExportError::Identity);
        }
        let expected_digest = r.take(32)?;
        let verify = match frame_validation {
            #[cfg(test)]
            FrameValidation::AtUse => true,
            FrameValidation::AdmittedAtBuild => false,
        };
        if verify && expected_digest != Sha256::digest(bytes).as_slice() {
            return Err(IntlBundleExportError::Identity);
        }
        admitted.push((component, IntlCompilationBytes::Build(bytes)));
    }
    let mut values = std::array::from_fn(|_| None);
    for key in SupportedValuesKey::ALL {
        let count = r.u32()?;
        if count > r.0.len() / 4 {
            return Err(IntlBundleExportError::TooLarge);
        }
        let mut list: Vec<Box<str>> = Vec::with_capacity(count);
        for _ in 0..count {
            let value = r.text()?;
            if value.is_empty() || list.last().is_some_and(|last| last.as_ref() >= value) {
                return Err(IntlBundleExportError::Identity);
            }
            list.push(value.into());
        }
        if list.is_empty() && key != SupportedValuesKey::Collation {
            return Err(IntlBundleExportError::Identity);
        }
        values[key.index()] = Some(Arc::new(SupportedValuesList::from_build_catalogue(
            list.into_boxed_slice(),
            identity.clone(),
        )));
    }
    if !r.0.is_empty() {
        return Err(IntlBundleExportError::Framing("trailing compiler metadata"));
    }
    Ok(IntlCompilationData {
        minimal_consumers: Default::default(),
        identity,
        services: None,
        frames: admitted,
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;
    const SOURCE: [u8; 32] = [17; 32];
    struct Fixture {
        metadata: Vec<u8>,
        frames: Vec<(IntlDataComponent, &'static [u8])>,
    }
    fn fixture() -> &'static Fixture {
        static FIXTURE: OnceLock<Fixture> = OnceLock::new();
        FIXTURE.get_or_init(|| {
            let selection = IntlDataSelection::new(crate::IntlCompilationProfile::Minimal);
            let full = selection.selected().unwrap();
            Fixture {
                metadata: full.minimal_build_metadata(&SOURCE).unwrap(),
                frames: full
                    .component_images()
                    .into_iter()
                    .map(|(component, bytes)| {
                        let owner: &'static Arc<[u8]> = Box::leak(Box::new(bytes));
                        (component, owner.as_ref())
                    })
                    .collect(),
            }
        })
    }
    fn resign(mut body: Vec<u8>) -> Vec<u8> {
        body.extend_from_slice(&Sha256::digest(&body));
        body
    }

    #[test]
    fn build_metadata_matches_actual_frames_identity_and_all_consumer_catalogues() {
        let f = fixture();
        let parsed = decode_build_metadata(&f.metadata, &SOURCE, f.frames.clone()).unwrap();
        let owner = IntlDataSelection::new(crate::IntlCompilationProfile::Minimal);
        let full = owner.selected().unwrap();
        assert_eq!(parsed.identity(), full.identity());
        assert!(parsed.service_selection().is_none());
        for ((name, actual), (expected_name, expected)) in parsed
            .component_sections()
            .into_iter()
            .zip(full.component_sections())
        {
            assert_eq!(name, expected_name);
            assert_eq!(actual.as_ref(), expected.as_ref());
            assert!(matches!(actual, IntlCompilationBytes::Build(_)));
            assert!(std::ptr::eq(
                actual.as_ref(),
                f.frames
                    .iter()
                    .find(|(c, _)| c.section_name() == name)
                    .unwrap()
                    .1
            ));
        }
        for key in SupportedValuesKey::ALL {
            let actual = parsed.supported_values(key).unwrap();
            let expected = full.supported_values(key).unwrap();
            assert_eq!(actual.provider_identity(), expected.provider_identity());
            assert_eq!(actual.values(), expected.values());
        }
    }

    #[test]
    fn compiler_metadata_rejects_wrong_source_frames_and_noncanonical_framing() {
        let f = fixture();
        assert!(decode_build_metadata(&f.metadata, &[18; 32], f.frames.clone()).is_err());
        let mut reordered = f.frames.clone();
        reordered.swap(0, 1);
        assert!(decode_build_metadata(&f.metadata, &SOURCE, reordered).is_err());
        let mut missing = f.frames.clone();
        missing.pop();
        assert!(decode_build_metadata(&f.metadata, &SOURCE, missing).is_err());
        let mut changed = f.frames.clone();
        changed[0].1 = b"wrong frame";
        assert!(decode_build_metadata(&f.metadata, &SOURCE, changed).is_err());
        for n in [0, 8, 31, 100, f.metadata.len() - 1] {
            assert!(decode_build_metadata(&f.metadata[..n], &SOURCE, f.frames.clone()).is_err());
        }
        let mut bad_digest = f.metadata.clone();
        bad_digest[40] ^= 1;
        let last = bad_digest.len() - 32;
        let bad_identity = resign(bad_digest[..last].to_vec());
        assert!(decode_build_metadata(&bad_identity, &SOURCE, f.frames.clone()).is_err());
        let mut trailing = f.metadata[..f.metadata.len() - 32].to_vec();
        trailing.push(0);
        assert!(decode_build_metadata(&resign(trailing), &SOURCE, f.frames.clone()).is_err());
    }

    #[test]
    fn unused_compilation_selection_does_not_read_build_frames() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let observer = Arc::clone(&calls);
        let selection = IntlDataSelection::defer_minimal_compilation_data(move || {
            observer.fetch_add(1, Ordering::Relaxed);
            let f = fixture();
            decode_build_metadata(&f.metadata, &SOURCE, f.frames.clone())
                .map(Arc::new)
                .map_err(|error| {
                    EmbeddedIntlProviderSetupError::DataImage(crate::IntlDataImageError::consumer(
                        error,
                    ))
                })
        });
        let _ = format!("{selection:?}");
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        let first = selection.compilation_data().unwrap();
        let second = selection.compilation_data().unwrap();
        assert!(Arc::ptr_eq(first, second));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
}
