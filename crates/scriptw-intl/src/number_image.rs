//! The actual native NumberFormat/PluralRules tables and their pinned descriptor.

use crate::image::{DataImageComponent, DataImageEnvelope};
use crate::number_format::{NumberProfiles, NUMBER_FORMAT_DATA_SHA256};
use crate::{
    CustomProfileId, IntlDataDigest, IntlDataImageError, IntlDataProfile, ListDataImage,
    LocaleDataImage, LocaleId,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::{Arc, OnceLock};

pub const INTL_NUMBER_DATA_CUSTOM_SECTION: &str = "scriptw.intl-number-profiles.v1";
// This is one genuine Scriptw-native schema descriptor, not an ICU data marker.
pub(crate) const NUMBER_IMAGE_MARKERS: &[&str] = &["scriptw/number/profiles/v2"];
const MAGIC: &[u8; 8] = b"LNPI0001";
const PINNED_DESCRIPTOR: &[u8] = include_bytes!("../data/number-cldr-47/payload-manifest.json");
const PINNED_BINARY: &[u8] = include_bytes!("../data/number-cldr-47/profiles.bin");

pub(crate) mod projection;

/// Source validation and physical serialization borrow the actual exact pinned
/// source owner. This authority cannot be minted from a selected image or cache.
pub(crate) struct PinnedNumberSource {
    profiles: Arc<NumberProfiles>,
}
impl PinnedNumberSource {
    pub(crate) fn profiles(&self) -> &Arc<NumberProfiles> {
        &self.profiles
    }
}
pub(crate) fn pinned_number_source() -> Result<PinnedNumberSource, IntlDataImageError> {
    let locale = crate::embedded_locale_data_image()?;
    let source = PinnedNumberSource {
        profiles: Arc::new(admit_native_payload(&native_payload()?, &locale)?),
    };
    if crate::number_format::encode_full(&source)
        .map_err(IntlDataImageError::consumer)?
        .as_slice()
        != PINNED_BINARY
    {
        return Err(IntlDataImageError::consumer(
            "Number canonical serializer differs from its exact pinned source",
        ));
    }
    Ok(source)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeDescriptor {
    schema: u32,
    canonical_sha256: String,
    payload_sha256: String,
    payload_bytes: usize,
    locales: usize,
    numbering_systems: usize,
    numbering_supplement: NumberingSupplement,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NumberingSupplement {
    cldr_commit: String,
    cldr_release: String,
    identifier: String,
    source_manifest_sha256: String,
    unicode_release: String,
}

fn native_payload() -> Result<Vec<u8>, IntlDataImageError> {
    let extent = 20usize
        .checked_add(PINNED_DESCRIPTOR.len())
        .and_then(|size| size.checked_add(PINNED_BINARY.len()))
        .ok_or_else(|| IntlDataImageError::consumer("native Number payload extent"))?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(extent)
        .map_err(IntlDataImageError::consumer)?;
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(
        &u32::try_from(PINNED_DESCRIPTOR.len())
            .map_err(IntlDataImageError::consumer)?
            .to_le_bytes(),
    );
    output.extend_from_slice(PINNED_DESCRIPTOR);
    output.extend_from_slice(
        &u64::try_from(PINNED_BINARY.len())
            .map_err(IntlDataImageError::consumer)?
            .to_le_bytes(),
    );
    output.extend_from_slice(PINNED_BINARY);
    Ok(output)
}

fn admit_native_payload(
    bytes: &[u8],
    locale: &LocaleDataImage,
) -> Result<NumberProfiles, IntlDataImageError> {
    if bytes.get(..8) != Some(MAGIC.as_slice()) {
        return Err(IntlDataImageError::consumer("native Number payload magic"));
    }
    let size = bytes
        .get(8..12)
        .ok_or_else(|| IntlDataImageError::consumer("native Number descriptor extent"))?;
    let size = u32::from_le_bytes(size.try_into().expect("four-byte extent")) as usize;
    if size > 64 * 1024 {
        return Err(IntlDataImageError::consumer(
            "native Number descriptor resource extent",
        ));
    }
    let end = 12usize
        .checked_add(size)
        .ok_or_else(|| IntlDataImageError::consumer("native Number descriptor overflow"))?;
    let binary_start = end
        .checked_add(8)
        .ok_or_else(|| IntlDataImageError::consumer("native Number payload overflow"))?;
    let descriptor = bytes
        .get(12..end)
        .ok_or_else(|| IntlDataImageError::consumer("native Number descriptor truncation"))?;
    let binary_size = bytes
        .get(end..binary_start)
        .ok_or_else(|| IntlDataImageError::consumer("native Number length truncation"))?;
    let binary_size = usize::try_from(u64::from_le_bytes(
        binary_size.try_into().expect("eight-byte extent"),
    ))
    .map_err(IntlDataImageError::consumer)?;
    if binary_start.checked_add(binary_size) != Some(bytes.len()) {
        return Err(IntlDataImageError::consumer(
            "native Number payload extent or trailing bytes",
        ));
    }
    let binary = &bytes[binary_start..];
    // Current Minimal/Custom frames must carry the exact locked data and source
    // descriptor; arbitrary self-consistent metadata cannot claim its provenance.
    if descriptor != PINNED_DESCRIPTOR || binary != PINNED_BINARY {
        return Err(IntlDataImageError::consumer(
            "native Number payload differs from its locked inputs",
        ));
    }
    let descriptor: NativeDescriptor =
        serde_json::from_slice(descriptor).map_err(IntlDataImageError::consumer)?;
    let supplement = &descriptor.numbering_supplement;
    if descriptor.schema != 1
        || descriptor.payload_bytes != binary.len()
        || descriptor.payload_sha256 != NUMBER_FORMAT_DATA_SHA256
        || descriptor.payload_sha256 != format!("{:x}", Sha256::digest(binary))
        || descriptor.canonical_sha256
            != "4bc17e2ed0a5a51a70ad342fa9bb7cf91af036db737775dedbf505204205699a"
        || supplement.identifier != "tols"
        || supplement.cldr_release != "48.0.0"
        || supplement.unicode_release != "17.0.0"
        || supplement.cldr_commit != "acd6d88ae493633240e19a87a721076a8a75c310"
        || supplement.source_manifest_sha256
            != "c0b70b5f5ffc5940c8f47e97ec99bd9c5596e1b81613901bffaa9a80323dc62f"
    {
        return Err(IntlDataImageError::consumer(
            "native Number source/version descriptor",
        ));
    }
    // This decoder admits every ordered table/index/range and the real en-US
    // default, including shared cardinal/ordinal rules and compact policies.
    let profiles =
        NumberProfiles::from_bytes(binary, locale).map_err(IntlDataImageError::consumer)?;
    if profiles.available_locales().len() != descriptor.locales
        || profiles.numbering_systems().len() != descriptor.numbering_systems
    {
        return Err(IntlDataImageError::consumer(
            "native Number descriptor inventory differs from admitted data",
        ));
    }
    Ok(profiles)
}

struct AdmittedNumberData {
    envelope: DataImageEnvelope,
    profiles: Arc<NumberProfiles>,
    locale_digest: IntlDataDigest,
    list_digest: Option<IntlDataDigest>,
}

/// Construction owns all decoded text/data and completes the native schema
/// admission before exposing a shared NumberFormat/PluralRules consumer.
#[derive(Clone)]
pub struct NumberProfilesDataImage(Arc<AdmittedNumberData>);
impl core::fmt::Debug for NumberProfilesDataImage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NumberProfilesDataImage")
            .field("profile", self.profile())
            .field("digest", &self.digest())
            .finish_non_exhaustive()
    }
}
impl NumberProfilesDataImage {
    pub(crate) fn require_complete_source(&self) -> Result<(), IntlDataImageError> {
        if self.0.envelope.blob() != native_payload()?.as_slice() {
            return Err(IntlDataImageError::IncompleteConformance);
        }
        Ok(())
    }

    pub fn for_profile(
        profile: IntlDataProfile,
        locale: &LocaleDataImage,
    ) -> Result<Self, IntlDataImageError> {
        if &profile != locale.profile() {
            return Err(IntlDataImageError::consumer(
                "Number and Locale image profiles differ",
            ));
        }
        let payload = native_payload()?;
        Self::from_complete_frame(
            DataImageEnvelope::encode(
                DataImageComponent::NumberProfiles,
                &profile,
                NUMBER_IMAGE_MARKERS,
                &payload,
            )?,
            locale,
        )
    }

    /// Admit the exact complete pinned schema without a projected List owner.
    /// Projected Number frames still use from_bytes and its actual dependencies.
    pub(crate) fn from_complete_frame(
        bytes: Arc<[u8]>,
        locale: &LocaleDataImage,
    ) -> Result<Self, IntlDataImageError> {
        let envelope = DataImageEnvelope::decode(
            bytes,
            DataImageComponent::NumberProfiles,
            NUMBER_IMAGE_MARKERS,
        )?;
        if envelope.profile() != locale.profile() {
            return Err(IntlDataImageError::consumer(
                "Number and Locale image profiles differ",
            ));
        }
        // The unchanged full branch has no List-specific selected dependency.
        // Its exact pinned native frame and one actual Number owner stay intact.
        let profiles = Arc::new(admit_native_payload(envelope.blob(), locale)?);
        Ok(Self(Arc::new(AdmittedNumberData {
            envelope,
            profiles,
            locale_digest: locale.digest(),
            list_digest: None,
        })))
    }
    pub fn from_bytes(
        bytes: Arc<[u8]>,
        locale: &LocaleDataImage,
        lists: &ListDataImage,
    ) -> Result<Self, IntlDataImageError> {
        let envelope = DataImageEnvelope::decode(
            bytes,
            DataImageComponent::NumberProfiles,
            NUMBER_IMAGE_MARKERS,
        )?;
        if envelope.profile() != locale.profile()
            || lists.profile() != locale.profile()
            || lists.locale_digest() != locale.digest()
        {
            return Err(IntlDataImageError::consumer(
                "Number, Locale and List image foundations differ",
            ));
        }
        let (profiles, list_digest) = if envelope.blob().starts_with(MAGIC) {
            (admit_native_payload(envelope.blob(), locale)?, None)
        } else {
            let catalogue = projection::admit(envelope.blob(), envelope.profile(), locale, lists)?;
            (
                NumberProfiles::from_projected_bytes(&catalogue, locale)
                    .map_err(IntlDataImageError::consumer)?,
                Some(lists.digest()),
            )
        };
        Ok(Self(Arc::new(AdmittedNumberData {
            envelope,
            profiles: Arc::new(profiles),
            locale_digest: locale.digest(),
            list_digest,
        })))
    }
    /// Publish one NumberFormat/PluralRules domain and retain the exact private
    /// rows needed by the supplied RelativeTime and Duration selectors.
    pub fn for_custom_projection(
        id: &CustomProfileId,
        public_locales: &[LocaleId],
        relative_time_locales: Option<&[LocaleId]>,
        duration_locales: Option<&[LocaleId]>,
        locale: &LocaleDataImage,
        lists: &ListDataImage,
    ) -> Result<Self, IntlDataImageError> {
        let profile = IntlDataProfile::Custom(id.clone());
        let payload = projection::produce(
            id,
            public_locales,
            relative_time_locales,
            duration_locales,
            locale,
            lists,
        )?;
        Self::from_bytes(
            DataImageEnvelope::encode(
                DataImageComponent::NumberProfiles,
                &profile,
                NUMBER_IMAGE_MARKERS,
                &payload,
            )?,
            locale,
            lists,
        )
    }
    /// Project actual localized currency data alongside the public locale
    /// domain. An absent locale selector retains every original public row.
    pub fn for_custom_data_projection(
        id: &CustomProfileId,
        public_locales: Option<&[LocaleId]>,
        currency_codes: &[crate::number_format::options::CurrencyCode],
        relative_time_locales: Option<&[LocaleId]>,
        duration_locales: Option<&[LocaleId]>,
        locale: &LocaleDataImage,
        lists: &ListDataImage,
    ) -> Result<Self, IntlDataImageError> {
        let payload = projection::produce_data(
            id,
            public_locales,
            currency_codes,
            relative_time_locales,
            duration_locales,
            locale,
            lists,
        )?;
        Self::from_bytes(
            DataImageEnvelope::encode(
                DataImageComponent::NumberProfiles,
                &IntlDataProfile::Custom(id.clone()),
                NUMBER_IMAGE_MARKERS,
                &payload,
            )?,
            locale,
            lists,
        )
    }

    pub fn bytes(&self) -> Arc<[u8]> {
        self.0.envelope.bytes()
    }
    pub fn for_custom_numbering_projection(
        id: &CustomProfileId,
        public_locales: Option<&[LocaleId]>,
        currency_codes: Option<&[crate::number_format::options::CurrencyCode]>,
        systems: &[crate::number_format::NumberingSystemOption],
        relative_time_locales: Option<&[LocaleId]>,
        duration_locales: Option<&[LocaleId]>,
        locale: &LocaleDataImage,
        lists: &ListDataImage,
    ) -> Result<Self, IntlDataImageError> {
        let payload = projection::produce_numbering(
            id,
            public_locales,
            currency_codes,
            systems,
            relative_time_locales,
            duration_locales,
            locale,
            lists,
        )?;
        Self::from_bytes(
            DataImageEnvelope::encode(
                DataImageComponent::NumberProfiles,
                &IntlDataProfile::Custom(id.clone()),
                NUMBER_IMAGE_MARKERS,
                &payload,
            )?,
            locale,
            lists,
        )
    }
    pub(crate) fn numbering_system_selection(
        &self,
    ) -> Option<&[crate::number_format::NumberingSystemOption]> {
        self.0.profiles.numbering_system_selection()
    }
    pub fn digest(&self) -> IntlDataDigest {
        self.0.envelope.digest()
    }
    pub fn profile(&self) -> &IntlDataProfile {
        self.0.envelope.profile()
    }
    pub fn profiles(&self) -> Arc<NumberProfiles> {
        Arc::clone(&self.0.profiles)
    }
    pub(crate) fn profiles_ref(&self) -> &NumberProfiles {
        &self.0.profiles
    }
    pub(crate) fn profiles_arc_ref(&self) -> &Arc<NumberProfiles> {
        &self.0.profiles
    }
    pub(crate) fn locale_digest(&self) -> IntlDataDigest {
        self.0.locale_digest
    }
    pub(crate) fn uses_locale(&self, locale: &LocaleDataImage) -> bool {
        self.0.profiles.uses_locale(locale)
    }
    pub(crate) fn list_digest(&self) -> Option<IntlDataDigest> {
        self.0.list_digest
    }
    pub(crate) fn required_relative_time_locales(&self) -> Option<&[Box<str>]> {
        self.0.profiles.required_relative_time_locales()
    }
    pub(crate) fn currency_codes(&self) -> Option<&[crate::number_format::options::CurrencyCode]> {
        self.0.profiles.currency_codes()
    }
    pub(crate) fn required_duration_locales(&self) -> Option<&[Box<str>]> {
        self.0.profiles.required_duration_locales()
    }
}

static MINIMAL_IMAGE: OnceLock<Result<NumberProfilesDataImage, IntlDataImageError>> =
    OnceLock::new();

pub(crate) fn embedded_number_profiles_data_image_ref(
) -> Result<&'static NumberProfilesDataImage, IntlDataImageError> {
    MINIMAL_IMAGE
        .get_or_init(|| {
            let locale = crate::embedded_locale_data_image()?;
            NumberProfilesDataImage::for_profile(IntlDataProfile::Minimal, &locale)
        })
        .as_ref()
        .map_err(Clone::clone)
}
pub fn embedded_number_profiles_data_image() -> Result<NumberProfilesDataImage, IntlDataImageError>
{
    embedded_number_profiles_data_image_ref().cloned()
}

mod selected;
pub(crate) use selected::selected_minimal_number_data_image;

#[cfg(test)]
mod tests;
