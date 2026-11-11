//! Complete CLDR47 English timezone-name catalogue with actual IANA ownership.

use crate::image::{DataImageComponent, DataImageEnvelope};
use crate::provider::{NamedTimeZones, TimeZoneNames};
use crate::{IntlDataDigest, IntlDataImageError, IntlDataProfile, NamedTimeZoneDataImage};
use std::sync::{Arc, OnceLock};
pub(crate) mod projection;

pub const INTL_TIME_ZONE_NAMES_DATA_CUSTOM_SECTION: &str = "scriptw.intl-time-zone-names-data.v1";
pub(crate) const TIME_ZONE_NAMES_IMAGE_MARKERS: &[&str] = &["scriptw/time-zone/names/cldr47/v1"];
const PINNED_PAYLOAD: &[u8] = include_bytes!("../data/zone-names-cldr-47/native-profile.json");

struct AdmittedTimeZoneNamesData {
    envelope: DataImageEnvelope,
    named_digest: IntlDataDigest,
    names: Arc<TimeZoneNames>,
}

/// Exact native names and the selected IANA owner are admitted together.
#[derive(Clone)]
pub struct TimeZoneNamesDataImage(Arc<AdmittedTimeZoneNamesData>);
impl core::fmt::Debug for TimeZoneNamesDataImage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TimeZoneNamesDataImage")
            .field("profile", self.profile())
            .field("digest", &self.digest())
            .finish_non_exhaustive()
    }
}
impl TimeZoneNamesDataImage {
    pub fn for_profile(
        profile: IntlDataProfile,
        named: &NamedTimeZoneDataImage,
    ) -> Result<Self, IntlDataImageError> {
        if let IntlDataProfile::Custom(id) = &profile {
            if named.named_time_zone_selection().is_some() {
                let payload = projection::produce(id, named)?;
                return Self::from_bytes(
                    DataImageEnvelope::encode(
                        DataImageComponent::TimeZoneNames,
                        &profile,
                        TIME_ZONE_NAMES_IMAGE_MARKERS,
                        &payload,
                    )?,
                    named,
                );
            }
        }
        Self::from_bytes(
            DataImageEnvelope::encode(
                DataImageComponent::TimeZoneNames,
                &profile,
                TIME_ZONE_NAMES_IMAGE_MARKERS,
                PINNED_PAYLOAD,
            )?,
            named,
        )
    }
    pub fn from_bytes(
        bytes: Arc<[u8]>,
        named: &NamedTimeZoneDataImage,
    ) -> Result<Self, IntlDataImageError> {
        let envelope = DataImageEnvelope::decode(
            bytes,
            DataImageComponent::TimeZoneNames,
            TIME_ZONE_NAMES_IMAGE_MARKERS,
        )?;
        if envelope.profile() != named.profile() {
            return Err(IntlDataImageError::consumer(
                "timezone names/IANA profile mismatch",
            ));
        }
        let names = if envelope.blob() == PINNED_PAYLOAD {
            if named.named_time_zone_selection().is_some() {
                return Err(IntlDataImageError::consumer(
                    "unprojected time-zone names paired with selected transition owner",
                ));
            }
            TimeZoneNames::from_json(envelope.blob(), named.zones())
                .map_err(IntlDataImageError::consumer)?
        } else {
            let catalogue = projection::admit(envelope.blob(), envelope.profile(), named)?;
            TimeZoneNames::from_projection(&catalogue, named.zones())
                .map_err(IntlDataImageError::consumer)?
        };
        let names = Arc::new(names);
        if !names.uses_named_zones(&named.zones()) {
            return Err(IntlDataImageError::consumer(
                "timezone names has a different actual IANA owner",
            ));
        }
        Ok(Self(Arc::new(AdmittedTimeZoneNamesData {
            envelope,
            named_digest: named.digest(),
            names,
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
    pub(crate) fn names(&self) -> Arc<TimeZoneNames> {
        Arc::clone(&self.0.names)
    }
    #[cfg(test)]
    pub(crate) fn names_ref(&self) -> &TimeZoneNames {
        &self.0.names
    }
    pub(crate) fn named_digest(&self) -> IntlDataDigest {
        self.0.named_digest
    }
    pub(crate) fn named_time_zone_selection(&self) -> Option<&[crate::TimeZoneId]> {
        self.0.names.named_time_zone_selection()
    }
    pub(crate) fn uses_named_zones(&self, named: &Arc<NamedTimeZones>) -> bool {
        self.0.names.uses_named_zones(named)
    }
}

pub(crate) fn embedded_time_zone_names_data_image_ref(
) -> Result<&'static TimeZoneNamesDataImage, IntlDataImageError> {
    static IMAGE: OnceLock<Result<TimeZoneNamesDataImage, IntlDataImageError>> = OnceLock::new();
    IMAGE
        .get_or_init(|| {
            let named = crate::embedded_named_time_zone_data_image()?;
            TimeZoneNamesDataImage::for_profile(IntlDataProfile::Minimal, &named)
        })
        .as_ref()
        .map_err(Clone::clone)
}
pub fn embedded_time_zone_names_data_image() -> Result<TimeZoneNamesDataImage, IntlDataImageError> {
    embedded_time_zone_names_data_image_ref().cloned()
}

#[cfg(test)]
mod tests;
