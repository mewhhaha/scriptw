//! Actual IANA2026a catalogue, TZif records and explicit country membership.

use crate::image::{DataImageComponent, DataImageEnvelope};
use crate::provider::locale_time_zones::LocaleTimeZoneProfiles;
use crate::provider::NamedTimeZones;
use crate::{IntlDataDigest, IntlDataImageError, IntlDataProfile};
use std::sync::{Arc, OnceLock};

pub(crate) mod projection;

pub const INTL_NAMED_TIME_ZONE_DATA_CUSTOM_SECTION: &str = "scriptw.intl-named-time-zone-data.v1";
// A native catalogue/record schema, rather than an ICU data marker.
pub(crate) const NAMED_TIME_ZONE_IMAGE_MARKERS: &[&str] = &["scriptw/time-zone/iana/v1"];
const MAGIC: &[u8; 8] = b"SCRWTZ01";
const PINNED_PAYLOAD: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/named-time-zone-data.blob"));

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, size: usize) -> Result<&'a [u8], IntlDataImageError> {
        let end = self
            .position
            .checked_add(size)
            .ok_or_else(|| IntlDataImageError::consumer("named image extent overflow"))?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| IntlDataImageError::consumer("named image truncated extent"))?;
        self.position = end;
        Ok(bytes)
    }
    fn count(&mut self) -> Result<usize, IntlDataImageError> {
        usize::try_from(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four-byte count"),
        ))
        .map_err(IntlDataImageError::consumer)
    }
    fn bytes(&mut self) -> Result<&'a [u8], IntlDataImageError> {
        let size = self.count()?;
        self.take(size)
    }
    fn text(&mut self) -> Result<&'a str, IntlDataImageError> {
        core::str::from_utf8(self.bytes()?).map_err(IntlDataImageError::consumer)
    }
}

struct AdmittedNamedTimeZoneData {
    envelope: DataImageEnvelope,
    zones: Arc<NamedTimeZones>,
    countries: Arc<LocaleTimeZoneProfiles>,
}

/// Complete catalogue, transition, gap and country-projection validation occurs
/// before publication. Both consumers retain the same actual named-zone owner.
#[derive(Clone)]
pub struct NamedTimeZoneDataImage(Arc<AdmittedNamedTimeZoneData>);
impl core::fmt::Debug for NamedTimeZoneDataImage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NamedTimeZoneDataImage")
            .field("profile", self.profile())
            .field("digest", &self.digest())
            .finish_non_exhaustive()
    }
}
impl NamedTimeZoneDataImage {
    pub fn for_custom_projection(
        id: &crate::CustomProfileId,
        zones: &[crate::TimeZoneId],
    ) -> Result<Self, IntlDataImageError> {
        let payload = projection::produce(id, zones)?;
        Self::from_bytes(DataImageEnvelope::encode(
            DataImageComponent::NamedTimeZones,
            &IntlDataProfile::Custom(id.clone()),
            NAMED_TIME_ZONE_IMAGE_MARKERS,
            &payload,
        )?)
    }
    pub fn for_profile(profile: IntlDataProfile) -> Result<Self, IntlDataImageError> {
        Self::from_bytes(DataImageEnvelope::encode(
            DataImageComponent::NamedTimeZones,
            &profile,
            NAMED_TIME_ZONE_IMAGE_MARKERS,
            PINNED_PAYLOAD,
        )?)
    }
    pub fn from_bytes(bytes: Arc<[u8]>) -> Result<Self, IntlDataImageError> {
        let envelope = DataImageEnvelope::decode(
            bytes,
            DataImageComponent::NamedTimeZones,
            NAMED_TIME_ZONE_IMAGE_MARKERS,
        )?;
        let (native, zones) = if envelope.blob() == PINNED_PAYLOAD {
            let native = read_native(envelope.blob())?;
            if native.records.len() != 598 {
                return Err(IntlDataImageError::consumer("named record domain"));
            }
            let zones = NamedTimeZones::from_image_data(native.catalogue, &native.records)
                .map_err(IntlDataImageError::consumer)?;
            (native, zones)
        } else {
            let catalogue = projection::admit(envelope.blob(), envelope.profile())?;
            let zones = NamedTimeZones::from_projection(&catalogue)
                .map_err(IntlDataImageError::consumer)?;
            (read_native(catalogue.native())?, zones)
        };
        let zones = Arc::new(zones);
        let countries = Arc::new(
            LocaleTimeZoneProfiles::from_image_data(
                native.zone_tab,
                native.regions,
                Arc::clone(&zones),
            )
            .map_err(IntlDataImageError::consumer)?,
        );
        if !countries.uses_named_zones(&zones) {
            return Err(IntlDataImageError::consumer(
                "country membership has a different actual named-zone owner",
            ));
        }
        Ok(Self(Arc::new(AdmittedNamedTimeZoneData {
            envelope,
            zones,
            countries,
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
    pub(crate) fn zones(&self) -> Arc<NamedTimeZones> {
        Arc::clone(&self.0.zones)
    }
    pub(crate) fn zones_ref(&self) -> &NamedTimeZones {
        &self.0.zones
    }
    pub(crate) fn named_time_zone_selection(&self) -> Option<&[crate::TimeZoneId]> {
        self.0.zones.named_time_zone_selection()
    }
    pub(crate) fn country_profiles(&self) -> Arc<LocaleTimeZoneProfiles> {
        Arc::clone(&self.0.countries)
    }
    #[cfg(test)]
    pub(crate) fn country_profiles_ref(&self) -> &LocaleTimeZoneProfiles {
        &self.0.countries
    }
}

struct NativePayload<'a> {
    catalogue: &'a str,
    zone_tab: &'a str,
    regions: &'a str,
    records: Vec<(&'a str, &'a [u8])>,
}
fn read_native(bytes: &[u8]) -> Result<NativePayload<'_>, IntlDataImageError> {
    let mut reader = Reader { bytes, position: 0 };
    if reader.take(MAGIC.len())? != MAGIC {
        return Err(IntlDataImageError::consumer("named payload schema"));
    }
    let catalogue = reader.text()?;
    let zone_tab = reader.text()?;
    let regions = reader.text()?;
    let count = reader.count()?;
    if count == 0 || count > 598 {
        return Err(IntlDataImageError::consumer("named record domain"));
    }
    let mut records = Vec::new();
    records
        .try_reserve_exact(count)
        .map_err(IntlDataImageError::consumer)?;
    for _ in 0..count {
        records.push((reader.text()?, reader.bytes()?));
    }
    if reader.position != bytes.len() {
        return Err(IntlDataImageError::consumer("trailing named payload"));
    }
    Ok(NativePayload {
        catalogue,
        zone_tab,
        regions,
        records,
    })
}

pub(crate) fn embedded_named_time_zone_data_image_ref(
) -> Result<&'static NamedTimeZoneDataImage, IntlDataImageError> {
    static IMAGE: OnceLock<Result<NamedTimeZoneDataImage, IntlDataImageError>> = OnceLock::new();
    IMAGE
        .get_or_init(|| NamedTimeZoneDataImage::for_profile(IntlDataProfile::Minimal))
        .as_ref()
        .map_err(Clone::clone)
}
pub fn embedded_named_time_zone_data_image() -> Result<NamedTimeZoneDataImage, IntlDataImageError> {
    embedded_named_time_zone_data_image_ref().cloned()
}

#[cfg(test)]
mod tests;
