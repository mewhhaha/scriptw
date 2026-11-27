//! Actual immutable Segmenter rules, complex models and locale associations.

use crate::image::{DataImageComponent, DataImageEnvelope};
use crate::{
    CustomProfileId, IntlDataDigest, IntlDataImageError, IntlDataProfile, LocaleDataImage,
    LocaleId, SegmenterProfiles,
};
use icu_provider::buf::{AsDeserializingBufferProvider, BufferMarker};
use icu_provider::prelude::*;
use icu_provider_adapters::fallback::LocaleFallbackProvider;
use icu_provider_blob::BlobDataProvider;
use icu_segmenter::provider::*;
use std::fmt;
use std::sync::{Arc, OnceLock};

pub const INTL_SEGMENTER_DATA_CUSTOM_SECTION: &str = "scriptw.intl-segmenter-data.v1";
const MAGIC: &[u8; 8] = b"SCRWSEG1";
const MARKERS: [&str; 7] = [
    "segmenter/break/grapheme/cluster/v1",
    "segmenter/break/word/v1",
    "segmenter/break/sentence/v1",
    "segmenter/break/word/override/v1",
    "segmenter/break/sentence/override/v1",
    "segmenter/dictionary/auto/v1",
    "segmenter/lstm/auto/v1",
];
const PINNED_PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/segmenter-data.blob"));

mod projection;
pub(crate) use projection::SegmenterCatalogue;

enum SegmenterBacking {
    Full(BlobDataProvider),
    Projected(projection::RawRows),
}
impl DynamicDataProvider<BufferMarker> for SegmenterBacking {
    fn load_data(
        &self,
        marker: DataMarkerInfo,
        request: DataRequest,
    ) -> Result<DataResponse<BufferMarker>, DataError> {
        match self {
            Self::Full(provider) => provider.load_data(marker, request),
            Self::Projected(provider) => provider.load_data(marker, request),
        }
    }
}

fn split_payload(payload: &[u8]) -> Result<(&[u8], &[u8]), IntlDataImageError> {
    if payload.get(..MAGIC.len()) != Some(MAGIC.as_slice()) {
        return Err(IntlDataImageError::consumer("Segmenter payload framing"));
    }
    let length = payload
        .get(8..12)
        .ok_or_else(|| IntlDataImageError::consumer("Segmenter descriptor extent"))?;
    let length = u32::from_le_bytes(length.try_into().expect("four-byte extent")) as usize;
    if !(1..=64 * 1024).contains(&length) {
        return Err(IntlDataImageError::consumer("Segmenter descriptor extent"));
    }
    let end = 12usize
        .checked_add(length)
        .ok_or_else(|| IntlDataImageError::consumer("Segmenter descriptor overflow"))?;
    let descriptor = payload
        .get(12..end)
        .ok_or_else(|| IntlDataImageError::consumer("truncated Segmenter descriptor"))?;
    let blob = payload
        .get(end..)
        .filter(|blob| !blob.is_empty())
        .ok_or_else(|| IntlDataImageError::consumer("missing Segmenter ICU payload"))?;
    Ok((descriptor, blob))
}

/// A typed provider exists only after every exact marker row deserializes.
/// Its selected Locale image supplies the real fallback authority.
pub(crate) struct SegmenterImageProvider {
    provider: LocaleFallbackProvider<SegmenterBacking>,
    locale: LocaleDataImage,
}
impl fmt::Debug for SegmenterImageProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SegmenterImageProvider")
            .field("locale_digest", &self.locale.digest())
            .finish_non_exhaustive()
    }
}
impl SegmenterImageProvider {
    fn checked(blob: &[u8], locale: &LocaleDataImage) -> Result<Self, IntlDataImageError> {
        let provider = BlobDataProvider::try_new_from_blob(blob.into())
            .map_err(IntlDataImageError::consumer)?;
        macro_rules! admit {
            ($marker:ty) => {{
                let ids = IterableDynamicDataProvider::<BufferMarker>::iter_ids_for_marker(
                    &provider,
                    <$marker>::INFO,
                )
                .map_err(IntlDataImageError::consumer)?;
                if (ids.is_empty() && <$marker>::INFO != SegmenterBreakWordOverrideV1::INFO)
                    || (<$marker>::INFO.is_singleton
                        && (ids.len() != 1
                            || !ids.iter().all(|id| {
                                id.locale.is_unknown() && id.marker_attributes.is_empty()
                            })))
                {
                    return Err(IntlDataImageError::consumer(concat!(
                        "missing or invalid exact Segmenter identifiers for ",
                        stringify!($marker)
                    )));
                }
                for id in ids {
                    DataProvider::<$marker>::load(
                        &provider.as_deserializing(),
                        DataRequest {
                            id: id.as_borrowed(),
                            metadata: Default::default(),
                        },
                    )
                    .map_err(IntlDataImageError::consumer)?;
                }
            }};
        }
        admit!(SegmenterBreakGraphemeClusterV1);
        admit!(SegmenterBreakWordV1);
        admit!(SegmenterBreakSentenceV1);
        admit!(SegmenterBreakWordOverrideV1);
        admit!(SegmenterBreakSentenceOverrideV1);
        admit!(SegmenterDictionaryAutoV1);
        admit!(SegmenterLstmAutoV1);
        Ok(Self {
            provider: LocaleFallbackProvider::new(
                SegmenterBacking::Full(provider),
                locale.fallbacker(),
            ),
            locale: locale.clone(),
        })
    }
    fn projected(rows: projection::RawRows, locale: &LocaleDataImage) -> Self {
        Self {
            provider: LocaleFallbackProvider::new(
                SegmenterBacking::Projected(rows),
                locale.fallbacker(),
            ),
            locale: locale.clone(),
        }
    }
}
macro_rules! image_marker {
    ($marker:ty) => {
        impl DataProvider<$marker> for SegmenterImageProvider {
            fn load(&self, request: DataRequest) -> Result<DataResponse<$marker>, DataError> {
                DataProvider::<$marker>::load(&self.provider.as_deserializing(), request)
            }
        }
    };
}
image_marker!(SegmenterBreakGraphemeClusterV1);
image_marker!(SegmenterBreakWordV1);
image_marker!(SegmenterBreakSentenceV1);
image_marker!(SegmenterBreakWordOverrideV1);
image_marker!(SegmenterBreakSentenceOverrideV1);
image_marker!(SegmenterDictionaryAutoV1);
image_marker!(SegmenterLstmAutoV1);

#[derive(Debug)]
struct AdmittedSegmenterData {
    envelope: DataImageEnvelope,
    profiles: Arc<SegmenterProfiles>,
    locale_digest: IntlDataDigest,
}

/// All seven typed markers, required model/override certificates, three actual
/// constructors and the exact consumed locale descriptor precede publication.
#[derive(Debug, Clone)]
pub struct SegmenterDataImage(Arc<AdmittedSegmenterData>);
impl SegmenterDataImage {
    pub(crate) fn require_complete_source(&self) -> Result<(), IntlDataImageError> {
        if self.0.envelope.blob() != PINNED_PAYLOAD {
            return Err(IntlDataImageError::IncompleteConformance);
        }
        Ok(())
    }

    pub fn for_custom_projection(
        id: &CustomProfileId,
        public_locales: &[LocaleId],
        locale: &LocaleDataImage,
    ) -> Result<Self, IntlDataImageError> {
        let projected = projection::project(id, public_locales, locale)?;
        Self::from_bytes(
            DataImageEnvelope::encode(
                DataImageComponent::Segmenter,
                &IntlDataProfile::Custom(id.clone()),
                &MARKERS,
                &projected.bytes,
            )?,
            locale,
        )
    }
    pub fn for_profile(
        profile: IntlDataProfile,
        locale: &LocaleDataImage,
    ) -> Result<Self, IntlDataImageError> {
        Self::from_bytes(
            DataImageEnvelope::encode(
                DataImageComponent::Segmenter,
                &profile,
                &MARKERS,
                PINNED_PAYLOAD,
            )?,
            locale,
        )
    }
    pub fn from_bytes(
        bytes: Arc<[u8]>,
        locale: &LocaleDataImage,
    ) -> Result<Self, IntlDataImageError> {
        let envelope = DataImageEnvelope::decode(bytes, DataImageComponent::Segmenter, &MARKERS)?;
        if envelope.profile() != locale.profile() {
            return Err(IntlDataImageError::consumer(
                "Segmenter and Locale image profiles differ",
            ));
        }
        let profiles = if projection::is_projection(envelope.blob()) {
            let projected = projection::admit(envelope.blob(), envelope.profile(), locale)?;
            SegmenterProfiles::from_projection(
                Arc::new(SegmenterImageProvider::projected(projected.rows, locale)),
                &projected.catalogue,
            )
            .map_err(IntlDataImageError::consumer)?
        } else {
            if envelope.blob() != PINNED_PAYLOAD {
                return Err(IntlDataImageError::consumer(
                    "Segmenter payload differs from the exact locked component image",
                ));
            }
            let (descriptor, blob) = split_payload(envelope.blob())?;
            let data = Arc::new(SegmenterImageProvider::checked(blob, locale)?);
            SegmenterProfiles::from_image_data(data, descriptor)
                .map_err(IntlDataImageError::consumer)?
        };
        Ok(Self(Arc::new(AdmittedSegmenterData {
            envelope,
            profiles: Arc::new(profiles),
            locale_digest: locale.digest(),
        })))
    }
    pub fn bytes(&self) -> Arc<[u8]> {
        self.0.envelope.bytes()
    }
    pub fn digest(&self) -> IntlDataDigest {
        self.0.envelope.digest()
    }
    pub fn profile(&self) -> &IntlDataProfile {
        self.0.envelope.profile()
    }
    pub fn profiles(&self) -> Arc<SegmenterProfiles> {
        self.0.profiles.clone()
    }
    pub(crate) fn locale_digest(&self) -> IntlDataDigest {
        self.0.locale_digest
    }
    pub(crate) fn profiles_ref(&self) -> &SegmenterProfiles {
        &self.0.profiles
    }
}

pub(crate) fn embedded_segmenter_data_image_ref(
) -> Result<&'static SegmenterDataImage, IntlDataImageError> {
    static IMAGE: OnceLock<Result<SegmenterDataImage, IntlDataImageError>> = OnceLock::new();
    IMAGE
        .get_or_init(|| {
            SegmenterDataImage::for_profile(
                IntlDataProfile::Minimal,
                &crate::embedded_locale_data_image()?,
            )
        })
        .as_ref()
        .map_err(Clone::clone)
}
pub fn embedded_segmenter_data_image() -> Result<SegmenterDataImage, IntlDataImageError> {
    embedded_segmenter_data_image_ref().cloned()
}

#[cfg(test)]
mod tests;
