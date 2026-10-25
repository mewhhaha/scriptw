//! ICU Locale transforms and fallback consume one admitted immutable image.

use crate::image::{DataImageComponent, DataImageEnvelope};
use crate::provider::KeywordAliasData;
use crate::{IntlDataDigest, IntlDataImageError, IntlDataProfile};
use icu_locale::provider::LocaleAliasesV1;
use icu_locale::{LocaleCanonicalizer, LocaleExpander, LocaleFallbacker};
use icu_provider::buf::AsDeserializingBufferProvider;
use icu_provider::prelude::*;
use icu_provider_blob::BlobDataProvider;
use std::sync::{Arc, OnceLock};

pub const INTL_LOCALE_DATA_CUSTOM_SECTION: &str = "scriptw.intl-locale-data.v1";
const MARKERS: [&str; 6] = [
    "scriptw/keyword-aliases/profiles/v1",
    "locale/aliases/v1",
    "locale/likely/subtags/extended/v1",
    "locale/likely/subtags/language/v1",
    "locale/likely/subtags/script/region/v1",
    "locale/parents/v1",
];
const PINNED_BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/locale-data.blob"));
const PINNED_KEYWORDS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/keyword-data.json"));
const MAGIC: &[u8; 8] = b"LLTI0001";

fn selected_payload() -> Result<Vec<u8>, IntlDataImageError> {
    let size = 24usize
        .checked_add(PINNED_BLOB.len())
        .and_then(|n| n.checked_add(PINNED_KEYWORDS.len()))
        .ok_or_else(|| IntlDataImageError::consumer("Locale data payload extent"))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(IntlDataImageError::consumer)?;
    bytes.extend_from_slice(MAGIC);
    for payload in [PINNED_BLOB, PINNED_KEYWORDS] {
        bytes.extend_from_slice(
            &u64::try_from(payload.len())
                .map_err(IntlDataImageError::consumer)?
                .to_le_bytes(),
        );
        bytes.extend_from_slice(payload);
    }
    Ok(bytes)
}

fn admit_payload(bytes: &[u8]) -> Result<(&[u8], KeywordAliasData), IntlDataImageError> {
    let fail = || IntlDataImageError::consumer("Locale data payload framing or locked source");
    if bytes.get(..8) != Some(MAGIC.as_slice()) {
        return Err(fail());
    }
    let mut position = 8usize;
    let mut inputs = [&[][..]; 2];
    for (input, pinned) in inputs.iter_mut().zip([PINNED_BLOB, PINNED_KEYWORDS]) {
        let end = position.checked_add(8).ok_or_else(fail)?;
        let size = bytes.get(position..end).ok_or_else(fail)?;
        let size = usize::try_from(u64::from_le_bytes(
            size.try_into().expect("eight-byte extent"),
        ))
        .map_err(IntlDataImageError::consumer)?;
        position = end;
        let end = position.checked_add(size).ok_or_else(fail)?;
        *input = bytes.get(position..end).ok_or_else(fail)?;
        if *input != pinned {
            return Err(fail());
        }
        position = end;
    }
    if position != bytes.len() {
        return Err(fail());
    }
    Ok((inputs[0], KeywordAliasData::from_bytes(inputs[1])?))
}

#[derive(Debug)]
struct AdmittedLocaleData {
    envelope: DataImageEnvelope,
    aliases: DataPayload<LocaleAliasesV1>,
    keyword_aliases: Arc<KeywordAliasData>,
    canonicalizer: Arc<LocaleCanonicalizer>,
    expander: LocaleExpander,
    fallbacker: LocaleFallbacker,
}

/// A decoded frame alone cannot supply this type: all five genuine typed
/// payloads and the actual native keyword rows are admitted before publication.
#[derive(Debug, Clone)]
pub struct LocaleDataImage(Arc<AdmittedLocaleData>);
impl LocaleDataImage {
    pub fn for_profile(profile: IntlDataProfile) -> Result<Self, IntlDataImageError> {
        Self::from_bytes(DataImageEnvelope::encode(
            DataImageComponent::LocaleTransforms,
            &profile,
            &MARKERS,
            &selected_payload()?,
        )?)
    }

    pub fn from_bytes(bytes: Arc<[u8]>) -> Result<Self, IntlDataImageError> {
        let envelope =
            DataImageEnvelope::decode(bytes, DataImageComponent::LocaleTransforms, &MARKERS)?;
        let (blob, keyword_aliases) = admit_payload(envelope.blob())?;
        let provider = BlobDataProvider::try_new_from_blob(blob.into())
            .map_err(IntlDataImageError::consumer)?;
        let typed = provider.as_deserializing();
        let aliases = DataProvider::<LocaleAliasesV1>::load(&typed, Default::default())
            .map_err(IntlDataImageError::consumer)?
            .payload;
        let canonicalizer = LocaleCanonicalizer::try_new_extended_unstable(&typed)
            .map_err(IntlDataImageError::consumer)?;
        let expander = LocaleExpander::try_new_extended_unstable(&typed)
            .map_err(IntlDataImageError::consumer)?;
        let fallbacker =
            LocaleFallbacker::try_new_unstable(&typed).map_err(IntlDataImageError::consumer)?;
        let mut default: icu_locale::Locale =
            "en-US".parse().map_err(IntlDataImageError::consumer)?;
        canonicalizer.canonicalize(&mut default);
        if default.to_string() != "en-US" {
            return Err(IntlDataImageError::consumer(
                "selected image does not retain the fixed canonical default",
            ));
        }
        Ok(Self(Arc::new(AdmittedLocaleData {
            envelope,
            aliases,
            keyword_aliases: Arc::new(keyword_aliases),
            canonicalizer: Arc::new(canonicalizer),
            expander,
            fallbacker,
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
    pub(crate) fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub(crate) fn keyword_aliases(&self) -> Arc<KeywordAliasData> {
        self.0.keyword_aliases.clone()
    }
    pub(crate) fn aliases(&self) -> DataPayload<LocaleAliasesV1> {
        self.0.aliases.clone()
    }
    pub(crate) fn canonicalizer(&self) -> Arc<LocaleCanonicalizer> {
        self.0.canonicalizer.clone()
    }
    pub(crate) fn expander(&self) -> LocaleExpander {
        self.0.expander.clone()
    }
    pub(crate) fn fallbacker(&self) -> LocaleFallbacker {
        self.0.fallbacker.clone()
    }
}

static MINIMAL_IMAGE: OnceLock<Result<LocaleDataImage, IntlDataImageError>> = OnceLock::new();

pub fn embedded_locale_data_image() -> Result<LocaleDataImage, IntlDataImageError> {
    MINIMAL_IMAGE
        .get_or_init(|| LocaleDataImage::for_profile(IntlDataProfile::Minimal))
        .clone()
}

/// Only an opaque complete Minimal selection may seed the canonical cache.
pub(crate) fn selected_minimal_locale_data_image(
    selection: &crate::IntlCompilationData,
) -> Result<LocaleDataImage, IntlDataImageError> {
    selected_frame(
        &MINIMAL_IMAGE,
        selection.minimal_frame(crate::IntlDataComponent::Locale)?,
    )
}

fn selected_frame(
    cache: &OnceLock<Result<LocaleDataImage, IntlDataImageError>>,
    bytes: crate::IntlCompilationBytes,
) -> Result<LocaleDataImage, IntlDataImageError> {
    let matches = |image: &LocaleDataImage| {
        if image.profile() != &IntlDataProfile::Minimal || image.bytes().as_ref() != bytes.as_ref()
        {
            return Err(IntlDataImageError::consumer(
                "selected Locale differs from canonical Minimal data",
            ));
        }
        Ok(image.clone())
    };
    if let Some(image) = cache.get() {
        return matches(image.as_ref().map_err(Clone::clone)?);
    }
    let owned = match &bytes {
        crate::IntlCompilationBytes::Shared(bytes) => Arc::clone(bytes),
        crate::IntlCompilationBytes::Build(bytes) => Arc::from(*bytes),
    };
    // Failed caller/frame admission must not poison the canonical cache.
    let image = LocaleDataImage::from_bytes(owned)?;
    matches(&image)?;
    matches(
        cache
            .get_or_init(|| Ok(image))
            .as_ref()
            .map_err(Clone::clone)?,
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod selected_tests {
    use super::*;
    #[test]
    fn failed_selected_locale_admission_does_not_poison_canonical_publication() {
        let cache = OnceLock::new();
        assert!(selected_frame(&cache, crate::IntlCompilationBytes::Build(b"invalid")).is_err());
        assert!(cache.get().is_none());
        let custom = LocaleDataImage::for_profile(IntlDataProfile::Custom(
            crate::CustomProfileId::parse("locale-cache-negative").unwrap(),
        ))
        .unwrap();
        assert!(
            selected_frame(&cache, crate::IntlCompilationBytes::Shared(custom.bytes())).is_err()
        );
        assert!(cache.get().is_none());
        let canonical = embedded_locale_data_image().unwrap();
        let selected = selected_frame(
            &cache,
            crate::IntlCompilationBytes::Shared(canonical.bytes()),
        )
        .unwrap();
        assert_eq!(selected.bytes(), canonical.bytes());
        assert!(cache.get().unwrap().is_ok());
    }
}
