//! Native DateTime patterns, all consumed calendar payloads and selected zones.

use crate::datetime::*;
use crate::image::{DataImageComponent, DataImageEnvelope};
use crate::provider::{DateTimeProvider, LocaleCanonicalizationData, NamedTimeZones};
use crate::{
    embedded_locale_data_image, embedded_named_time_zone_data_image, CustomProfileId,
    IntlDataDigest, IntlDataImageError, IntlDataProfile, LocaleDataImage, LocaleId,
    NamedTimeZoneDataImage,
};
use icu_provider_blob::BlobDataProvider;
use std::sync::{Arc, OnceLock};

pub(crate) mod projection;

pub const INTL_DATETIME_DATA_CUSTOM_SECTION: &str = "scriptw.intl-datetime-data.v1";
pub(crate) const DATETIME_IMAGE_MARKERS: &[&str] = &[
    "calendar/chinese/v1",
    "calendar/dangi/v1",
    "calendar/japanese/modern/v1",
    "scriptw/calendar/hijri/ummalqura/v1",
    "scriptw/datetime/profiles/v2",
];
const PAYLOAD_MAGIC: &[u8; 8] = b"LDTI0001";
const PINNED_NATIVE: &[u8] = include_bytes!("provider/datetime/generated/profile.json");
pub(crate) const PINNED_CALENDAR: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/calendar-data.blob"));

struct AdmittedDateTimeData {
    envelope: DataImageEnvelope,
    locale: LocaleDataImage,
    named: NamedTimeZoneDataImage,
    provider: Arc<DateTimeProvider>,
    numbering_systems: Option<Box<[crate::number_format::NumberingSystemOption]>>,
}

/// Pinned Minimal preparation owns no named-zone authority and cannot publish
/// an image. The existing image OnceLock performs this independent work before
/// waiting for the actual IANA owner, then consumes it during final admission.
struct PreparedPinnedDateTimeImage {
    envelope: DataImageEnvelope,
    locale: LocaleDataImage,
    provider: crate::provider::PreparedDateTimeProvider,
}

impl PreparedPinnedDateTimeImage {
    fn new(locale: &LocaleDataImage) -> Result<Self, IntlDataImageError> {
        let envelope = DataImageEnvelope::decode(
            DataImageEnvelope::encode(
                DataImageComponent::DateTime,
                &IntlDataProfile::Minimal,
                DATETIME_IMAGE_MARKERS,
                &payload(),
            )?,
            DataImageComponent::DateTime,
            DATETIME_IMAGE_MARKERS,
        )?;
        if envelope.profile() != locale.profile() {
            return Err(IntlDataImageError::consumer(
                "DateTime, Locale and IANA image profiles differ",
            ));
        }
        let (native, calendars) = split_payload(envelope.blob())?;
        if native != PINNED_NATIVE || calendars != PINNED_CALENDAR {
            return Err(IntlDataImageError::consumer(
                "prepared DateTime payload differs from exact locked data",
            ));
        }
        let canonicalizer =
            LocaleCanonicalizationData::from_image(locale).map_err(IntlDataImageError::consumer)?;
        let calendar_data = BlobDataProvider::try_new_from_blob(calendars.into())
            .map_err(IntlDataImageError::consumer)?;
        let provider = DateTimeProvider::prepare_image_data(
            core::str::from_utf8(native).map_err(IntlDataImageError::consumer)?,
            &calendar_data,
            &canonicalizer,
            locale.keyword_aliases(),
            envelope.digest(),
            locale.digest(),
        )
        .map_err(IntlDataImageError::consumer)?;
        Ok(Self {
            envelope,
            locale: locale.clone(),
            provider,
        })
    }

    fn finish(
        self,
        named: &NamedTimeZoneDataImage,
    ) -> Result<DateTimeDataImage, IntlDataImageError> {
        if self.envelope.profile() != named.profile() {
            return Err(IntlDataImageError::consumer(
                "DateTime, Locale and IANA image profiles differ",
            ));
        }
        if named.named_time_zone_selection().is_some() {
            return Err(IntlDataImageError::consumer(
                "unprojected DateTime names paired with selected IANA owner",
            ));
        }
        let provider = self.provider.bind_named_image(named);
        provider
            .available_calendars()
            .map_err(IntlDataImageError::consumer)?;
        Ok(DateTimeDataImage(Arc::new(AdmittedDateTimeData {
            envelope: self.envelope,
            locale: self.locale,
            named: named.clone(),
            provider: Arc::new(provider),
            numbering_systems: None,
        })))
    }
}

/// Construction admits every native table and calendar payload and retains the
/// exact Locale and IANA owners before an opaque plan can be minted.
#[derive(Clone)]
pub struct DateTimeDataImage(Arc<AdmittedDateTimeData>);
impl core::fmt::Debug for DateTimeDataImage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DateTimeDataImage")
            .field("profile", self.profile())
            .field("digest", &self.digest())
            .finish_non_exhaustive()
    }
}
impl DateTimeDataImage {
    pub(crate) fn require_complete_source(&self) -> Result<(), IntlDataImageError> {
        if self.0.envelope.blob() != payload().as_slice() {
            return Err(IntlDataImageError::IncompleteConformance);
        }
        Ok(())
    }

    pub fn for_profile(
        profile: IntlDataProfile,
        locale: &LocaleDataImage,
        named: &NamedTimeZoneDataImage,
    ) -> Result<Self, IntlDataImageError> {
        if let IntlDataProfile::Custom(id) = &profile {
            if named.named_time_zone_selection().is_some() {
                let native = projection::produce_named(id, locale, named)?;
                return Self::from_bytes(
                    DataImageEnvelope::encode(
                        DataImageComponent::DateTime,
                        &profile,
                        DATETIME_IMAGE_MARKERS,
                        &payload_with_native(&native)?,
                    )?,
                    locale,
                    named,
                );
            }
        }
        Self::from_bytes(
            DataImageEnvelope::encode(
                DataImageComponent::DateTime,
                &profile,
                DATETIME_IMAGE_MARKERS,
                &payload(),
            )?,
            locale,
            named,
        )
    }
    /// Project real pinned locale rows and their complete reachable native pools.
    /// The shared calendar data and named-zone inventory retain their full extent.
    pub fn for_custom_projection(
        id: &CustomProfileId,
        public_locales: &[LocaleId],
        locale: &LocaleDataImage,
        named: &NamedTimeZoneDataImage,
    ) -> Result<Self, IntlDataImageError> {
        let native = projection::produce(id, public_locales, locale, named)?;
        Self::from_bytes(
            DataImageEnvelope::encode(
                DataImageComponent::DateTime,
                &IntlDataProfile::Custom(id.clone()),
                DATETIME_IMAGE_MARKERS,
                &payload_with_native(&native)?,
            )?,
            locale,
            named,
        )
    }
    /// Select localized calendar records while retaining each locale's actual
    /// default and the complete shared calculation kernels.
    pub fn for_custom_data_projection(
        id: &CustomProfileId,
        public_locales: Option<&[LocaleId]>,
        calendar_types: &[DateTimeCalendar],
        locale: &LocaleDataImage,
        named: &NamedTimeZoneDataImage,
    ) -> Result<Self, IntlDataImageError> {
        let native = projection::produce_data(id, public_locales, calendar_types, locale, named)?;
        Self::from_bytes(
            DataImageEnvelope::encode(
                DataImageComponent::DateTime,
                &IntlDataProfile::Custom(id.clone()),
                DATETIME_IMAGE_MARKERS,
                &payload_with_native(&native)?,
            )?,
            locale,
            named,
        )
    }
    pub fn from_bytes(
        bytes: Arc<[u8]>,
        locale: &LocaleDataImage,
        named: &NamedTimeZoneDataImage,
    ) -> Result<Self, IntlDataImageError> {
        let envelope =
            DataImageEnvelope::decode(bytes, DataImageComponent::DateTime, DATETIME_IMAGE_MARKERS)?;
        if envelope.profile() != locale.profile() || envelope.profile() != named.profile() {
            return Err(IntlDataImageError::consumer(
                "DateTime, Locale and IANA image profiles differ",
            ));
        }
        let (native, calendars) = split_payload(envelope.blob())?;
        if calendars != PINNED_CALENDAR {
            return Err(IntlDataImageError::consumer(
                "DateTime calendar payload differs from exact locked data",
            ));
        }
        let canonicalizer =
            LocaleCanonicalizationData::from_image(locale).map_err(IntlDataImageError::consumer)?;
        let calendar_data = BlobDataProvider::try_new_from_blob(calendars.into())
            .map_err(IntlDataImageError::consumer)?;
        let (provider, numbering_systems) = if native == PINNED_NATIVE {
            if named.named_time_zone_selection().is_some() {
                return Err(IntlDataImageError::consumer(
                    "unprojected DateTime names paired with selected IANA owner",
                ));
            }
            (
                DateTimeProvider::from_image_data(
                    core::str::from_utf8(native).map_err(IntlDataImageError::consumer)?,
                    &calendar_data,
                    &canonicalizer,
                    locale.keyword_aliases(),
                    named.zones(),
                    envelope.digest(),
                    locale.digest(),
                    named.digest(),
                ),
                None,
            )
        } else {
            let catalogue =
                projection::admit(native, envelope.profile(), locale, named, &canonicalizer)?;
            (
                DateTimeProvider::from_projection(
                    &catalogue,
                    &calendar_data,
                    &canonicalizer,
                    locale.keyword_aliases(),
                    named.zones(),
                    envelope.digest(),
                    locale.digest(),
                    named.digest(),
                ),
                catalogue
                    .numbering_systems()
                    .map(|systems| systems.to_vec().into_boxed_slice()),
            )
        };
        let provider = provider.map_err(IntlDataImageError::consumer)?;
        // Localized availability is derived from the actual checked records,
        // independently of the complete calendar calculation kernel inventory.
        provider
            .available_calendars()
            .map_err(IntlDataImageError::consumer)?;
        Ok(Self(Arc::new(AdmittedDateTimeData {
            envelope,
            locale: locale.clone(),
            named: named.clone(),
            provider: Arc::new(provider),
            numbering_systems,
        })))
    }
    pub fn for_custom_numbering_projection(
        id: &CustomProfileId,
        public_locales: Option<&[LocaleId]>,
        calendar_types: Option<&[DateTimeCalendar]>,
        systems: &[crate::number_format::NumberingSystemOption],
        locale: &LocaleDataImage,
        named: &NamedTimeZoneDataImage,
    ) -> Result<Self, IntlDataImageError> {
        let native = projection::produce_numbering(
            id,
            public_locales,
            calendar_types,
            systems,
            locale,
            named,
        )?;
        Self::from_bytes(
            DataImageEnvelope::encode(
                DataImageComponent::DateTime,
                &IntlDataProfile::Custom(id.clone()),
                DATETIME_IMAGE_MARKERS,
                &payload_with_native(&native)?,
            )?,
            locale,
            named,
        )
    }
    pub(crate) fn numbering_system_selection(
        &self,
    ) -> Option<&[crate::number_format::NumberingSystemOption]> {
        self.0.numbering_systems.as_deref()
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
    pub(crate) fn provider(&self) -> Arc<DateTimeProvider> {
        Arc::clone(&self.0.provider)
    }
    #[cfg(test)]
    pub(crate) fn provider_ref(&self) -> &DateTimeProvider {
        &self.0.provider
    }
    pub(crate) fn locale_digest(&self) -> IntlDataDigest {
        self.0.locale.digest()
    }
    pub(crate) fn named_time_zone_digest(&self) -> IntlDataDigest {
        self.0.named.digest()
    }
    pub(crate) fn named_time_zone_selection(&self) -> Option<&[crate::TimeZoneId]> {
        self.0.provider.named_time_zone_selection()
    }
    pub(crate) fn uses_locale(&self, locale: &LocaleDataImage) -> bool {
        self.0.locale.same_owner(locale)
    }
    pub(crate) fn uses_named_time_zones(&self, zones: &Arc<NamedTimeZones>) -> bool {
        self.0.provider.uses_named_time_zones(zones)
    }

    pub fn resolve_locale(
        &self,
        request: DateTimeLocaleRequest,
    ) -> Result<DateTimeLocaleResult, DateTimeFormatError> {
        self.0.provider.resolve_locale(request)
    }
    pub fn supported_locales(
        &self,
        request: DateTimeSupportedLocalesRequest,
    ) -> Result<DateTimeSupportedLocalesResult, DateTimeFormatError> {
        self.0.provider.supported_locales(request)
    }
    pub fn select_plan(
        &self,
        request: DateTimePlanRequest,
    ) -> Result<DateTimePlanResult, DateTimeFormatError> {
        self.0.provider.select_plan(request)
    }
    pub fn format_parts(
        &self,
        request: DateTimeFormatRequest,
    ) -> Result<DateTimeParts, DateTimeFormatError> {
        self.0.provider.format_parts(request)
    }
    pub fn format_range_parts(
        &self,
        request: DateTimeRangeRequest,
    ) -> Result<DateTimeRangeParts, DateTimeFormatError> {
        self.0.provider.format_range_parts(request)
    }
}

fn payload() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(24 + PINNED_NATIVE.len() + PINNED_CALENDAR.len());
    bytes.extend_from_slice(PAYLOAD_MAGIC);
    for data in [PINNED_NATIVE, PINNED_CALENDAR] {
        // Both are compile-time byte slices, bounded by the shared component limit.
        bytes.extend_from_slice(
            &u64::try_from(data.len())
                .expect("bounded pinned data")
                .to_le_bytes(),
        );
        bytes.extend_from_slice(data);
    }
    bytes
}
fn payload_with_native(native: &[u8]) -> Result<Vec<u8>, IntlDataImageError> {
    let extent = 24usize
        .checked_add(native.len())
        .and_then(|n| n.checked_add(PINNED_CALENDAR.len()))
        .filter(|&n| n <= crate::MAX_INTL_COMPONENT_IMAGE_BYTES)
        .ok_or_else(|| IntlDataImageError::consumer("DateTime/calendar payload extent"))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(extent)
        .map_err(IntlDataImageError::consumer)?;
    bytes.extend_from_slice(PAYLOAD_MAGIC);
    for data in [native, PINNED_CALENDAR] {
        bytes.extend_from_slice(
            &u64::try_from(data.len())
                .map_err(IntlDataImageError::consumer)?
                .to_le_bytes(),
        );
        bytes.extend_from_slice(data);
    }
    Ok(bytes)
}
fn split_payload(bytes: &[u8]) -> Result<(&[u8], &[u8]), IntlDataImageError> {
    let fail = || IntlDataImageError::consumer("invalid native DateTime/calendar payload extents");
    let mut cursor = 8usize;
    if bytes.get(..cursor) != Some(PAYLOAD_MAGIC.as_slice()) {
        return Err(fail());
    }
    let mut field = || -> Result<&[u8], IntlDataImageError> {
        let length_end = cursor.checked_add(8).ok_or_else(fail)?;
        let length = u64::from_le_bytes(
            bytes
                .get(cursor..length_end)
                .ok_or_else(fail)?
                .try_into()
                .map_err(|_| fail())?,
        );
        let end = length_end
            .checked_add(usize::try_from(length).map_err(|_| fail())?)
            .ok_or_else(fail)?;
        let result = bytes.get(length_end..end).ok_or_else(fail)?;
        cursor = end;
        Ok(result)
    };
    let native = field()?;
    let calendar = field()?;
    if cursor != bytes.len() {
        return Err(fail());
    }
    Ok((native, calendar))
}

pub fn embedded_date_time_data_image() -> Result<DateTimeDataImage, IntlDataImageError> {
    static IMAGE: OnceLock<Result<DateTimeDataImage, IntlDataImageError>> = OnceLock::new();
    IMAGE
        .get_or_init(|| {
            let prepared = PreparedPinnedDateTimeImage::new(&embedded_locale_data_image()?)?;
            prepared.finish(&embedded_named_time_zone_data_image()?)
        })
        .clone()
}
#[cfg(test)]
mod tests;
