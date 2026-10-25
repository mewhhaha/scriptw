//! One selected provider owns emitted frames and compile-time catalogues.

mod minimal_admission;

use crate::number_format::options::CurrencyCode;
use crate::number_format::{InvalidNumberingSystemOption, NumberingSystemOption};
use crate::supported_values::SupportedValuesCatalogue;
use crate::{
    embedded_collator_data_image, embedded_date_time_data_image, embedded_display_names_data_image,
    embedded_duration_data_image, embedded_list_data_image, embedded_locale_data_image,
    embedded_named_time_zone_data_image, embedded_native_locale_information_data_image,
    embedded_number_profiles_data_image, embedded_relative_time_data_image,
    embedded_segmenter_data_image, embedded_time_zone_names_data_image, CollatorDataImage,
    CustomProfileId, DateTimeDataImage, DisplayNamesDataImage, DurationDataImage,
    EmbeddedIntlProvider, EmbeddedIntlProviderSetupError, IntlDataIdentity, IntlDataProfile,
    IntlProvider, InvalidLocaleId, ListDataImage, LocaleDataImage, LocaleId,
    NamedTimeZoneDataImage, NativeLocaleInformationDataImage, NumberProfilesDataImage,
    RelativeTimeDataImage, SegmenterDataImage, SupportedValuesKey, SupportedValuesList,
    SupportedValuesSetupError, TimeZoneNamesDataImage, INTL_COLLATOR_DATA_CUSTOM_SECTION,
    INTL_DATETIME_DATA_CUSTOM_SECTION, INTL_DISPLAY_NAMES_DATA_CUSTOM_SECTION,
    INTL_DURATION_DATA_CUSTOM_SECTION, INTL_LIST_DATA_CUSTOM_SECTION,
    INTL_LOCALE_DATA_CUSTOM_SECTION, INTL_NAMED_TIME_ZONE_DATA_CUSTOM_SECTION,
    INTL_NATIVE_LOCALE_INFORMATION_CUSTOM_SECTION, INTL_NUMBER_DATA_CUSTOM_SECTION,
    INTL_RELATIVE_TIME_DATA_CUSTOM_SECTION, INTL_SEGMENTER_DATA_CUSTOM_SECTION,
    INTL_TIME_ZONE_NAMES_DATA_CUSTOM_SECTION,
};
use crate::{CheckedIntlServiceSelection, IntlDataComponent, IntlService, IntlServiceSet};
use std::sync::{Arc, Mutex, OnceLock};
mod manifest;
pub use manifest::InvalidCustomIntlManifest;
mod export;
pub use export::IntlBundleExportError;
#[cfg(test)]
mod selected_profile_cache_tests;
#[cfg(test)]
mod service_tests;

/// Profiles resolved through actual typed image producers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum IntlCompilationProfile {
    #[default]
    Minimal,
    /// Complete unprojected pinned data and checked provider capability closure.
    /// This data selection does not assert that compiler conformance tests pass.
    Conformance,
    /// A named selection of the complete pinned component domains.
    Custom(CustomProfileId),
    /// Actual component projections sharing one checked Custom identity.
    CustomProjection(CustomIntlProfile),
}

#[cfg(test)]
mod conformance_tests {
    use super::*;
    use crate::image::{DataImageComponent, DataImageEnvelope};
    use crate::number_image::NUMBER_IMAGE_MARKERS;

    #[test]
    fn complete_conformance_selection_publishes_actual_frames_and_catalogues() {
        let selection = IntlDataSelection::new(IntlCompilationProfile::Conformance);
        let bundle = selection.selected().unwrap();
        assert_eq!(
            bundle.identity().profile().profile(),
            &IntlDataProfile::Conformance
        );
        for (_, bytes) in bundle.component_sections() {
            assert!(!bytes.is_empty());
        }
        for key in SupportedValuesKey::ALL {
            let list = bundle.supported_values(key).unwrap();
            assert_eq!(list.provider_identity(), bundle.identity());
        }
        assert!(std::ptr::eq(bundle, selection.selected().unwrap()));
        assert_ne!(
            bundle.identity(),
            IntlDataSelection::new(IntlCompilationProfile::Minimal)
                .selected()
                .unwrap()
                .identity()
        );
    }

    #[test]
    fn checksum_valid_projection_cannot_publish_conformance_data() {
        let id = CustomProfileId::parse("partial-number").unwrap();
        let projection = CustomIntlProfile::new(
            id,
            None,
            None,
            None,
            None,
            Some(&["en-US"]),
            None,
            None,
            None,
        )
        .unwrap();
        let selected = IntlDataSelection::new(IntlCompilationProfile::CustomProjection(projection));
        let partial = selected.selected().unwrap();
        assert!(matches!(
            partial.numbers.as_ref().unwrap().require_complete_source(),
            Err(crate::IntlDataImageError::IncompleteConformance)
        ));
        let decoded = DataImageEnvelope::decode(
            partial.numbers.as_ref().unwrap().bytes(),
            DataImageComponent::NumberProfiles,
            NUMBER_IMAGE_MARKERS,
        )
        .unwrap();
        let relabeled = DataImageEnvelope::encode(
            DataImageComponent::NumberProfiles,
            &IntlDataProfile::Conformance,
            NUMBER_IMAGE_MARKERS,
            decoded.blob(),
        )
        .unwrap();
        let complete = IntlDataSelection::new(IntlCompilationProfile::Conformance);
        let full = complete.selected().unwrap();
        assert!(NumberProfilesDataImage::from_bytes(
            relabeled,
            &full.locale,
            full.lists.as_ref().unwrap()
        )
        .is_err());
    }
}

/// One Custom identity and only the component filters with real producers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomIntlProfile {
    id: CustomProfileId,
    list: Option<CustomListProfile>,
    relative_time: Option<Box<[LocaleId]>>,
    display_names: Option<Box<[LocaleId]>>,
    duration: Option<Box<[LocaleId]>>,
    number: Option<Box<[LocaleId]>>,
    date_time: Option<Box<[LocaleId]>>,
    collator: Option<Box<[LocaleId]>>,
    segmenter: Option<Box<[LocaleId]>>,
    currency_codes: Option<Box<[CurrencyCode]>>,
    date_time_calendars: Option<Box<[crate::DateTimeCalendar]>>,
    numbering_systems: Option<Box<[NumberingSystemOption]>>,
    named_time_zones: Option<Box<[crate::TimeZoneId]>>,
    services: Option<CheckedIntlServiceSelection>,
}

impl CustomIntlProfile {
    pub fn new(
        id: CustomProfileId,
        list_locales: Option<&[&str]>,
        relative_time_locales: Option<&[&str]>,
        display_names_locales: Option<&[&str]>,
        duration_locales: Option<&[&str]>,
        number_locales: Option<&[&str]>,
        date_time_locales: Option<&[&str]>,
        collator_locales: Option<&[&str]>,
        segmenter_locales: Option<&[&str]>,
    ) -> Result<Self, InvalidCustomIntlProfile> {
        if list_locales.is_none()
            && relative_time_locales.is_none()
            && display_names_locales.is_none()
            && duration_locales.is_none()
            && number_locales.is_none()
            && date_time_locales.is_none()
            && collator_locales.is_none()
            && segmenter_locales.is_none()
        {
            return Err(InvalidCustomIntlProfile::NoProjection);
        }
        let list = list_locales
            .map(|locales| CustomListProfile::new(id.clone(), locales))
            .transpose()
            .map_err(InvalidCustomIntlProfile::ListLocales)?;
        let relative_time = relative_time_locales
            .map(checked_locale_projection)
            .transpose()
            .map_err(InvalidCustomIntlProfile::RelativeTimeLocales)?;
        let display_names = display_names_locales
            .map(checked_locale_projection)
            .transpose()
            .map_err(InvalidCustomIntlProfile::DisplayNamesLocales)?;
        let duration = duration_locales
            .map(checked_locale_projection)
            .transpose()
            .map_err(InvalidCustomIntlProfile::DurationLocales)?;
        let number = number_locales
            .map(checked_locale_projection)
            .transpose()
            .map_err(InvalidCustomIntlProfile::NumberLocales)?;
        let date_time = date_time_locales
            .map(checked_locale_projection)
            .transpose()
            .map_err(InvalidCustomIntlProfile::DateTimeLocales)?;
        let collator = collator_locales
            .map(checked_locale_projection)
            .transpose()
            .map_err(InvalidCustomIntlProfile::CollatorLocales)?;
        let segmenter = segmenter_locales
            .map(checked_locale_projection)
            .transpose()
            .map_err(InvalidCustomIntlProfile::SegmenterLocales)?;
        Ok(Self {
            id,
            list,
            relative_time,
            display_names,
            duration,
            number,
            date_time,
            collator,
            segmenter,
            currency_codes: None,
            date_time_calendars: None,
            numbering_systems: None,
            named_time_zones: None,
            services: None,
        })
    }

    pub fn id(&self) -> &CustomProfileId {
        &self.id
    }
    pub fn list_projection(&self) -> Option<&CustomListProfile> {
        self.list.as_ref()
    }
    pub fn relative_time_locales(&self) -> Option<&[LocaleId]> {
        self.relative_time.as_deref()
    }
    pub fn display_names_locales(&self) -> Option<&[LocaleId]> {
        self.display_names.as_deref()
    }
    pub fn duration_locales(&self) -> Option<&[LocaleId]> {
        self.duration.as_deref()
    }
    /// The coupled public NumberFormat and PluralRules locale domain.
    pub fn number_locales(&self) -> Option<&[LocaleId]> {
        self.number.as_deref()
    }
    pub fn date_time_locales(&self) -> Option<&[LocaleId]> {
        self.date_time.as_deref()
    }
    pub fn collator_locales(&self) -> Option<&[LocaleId]> {
        self.collator.as_deref()
    }
    pub fn segmenter_locales(&self) -> Option<&[LocaleId]> {
        self.segmenter.as_deref()
    }
    /// Select localized currency rows from both Number and DisplayNames.
    /// The global currency fraction authority remains complete.
    pub fn for_currency_codes(
        id: CustomProfileId,
        codes: &[&str],
    ) -> Result<Self, InvalidCustomIntlProfile> {
        Ok(Self {
            id,
            list: None,
            relative_time: None,
            display_names: None,
            duration: None,
            number: None,
            date_time: None,
            collator: None,
            segmenter: None,
            currency_codes: Some(
                checked_currency_projection(codes)
                    .map_err(InvalidCustomIntlProfile::CurrencyCodes)?,
            ),
            date_time_calendars: None,
            numbering_systems: None,
            named_time_zones: None,
            services: None,
        })
    }
    pub fn with_currency_codes(mut self, codes: &[&str]) -> Result<Self, InvalidCustomIntlProfile> {
        self.currency_codes = Some(
            checked_currency_projection(codes).map_err(InvalidCustomIntlProfile::CurrencyCodes)?,
        );
        self.validate_service_dimensions()?;
        Ok(self)
    }
    pub fn currency_codes(&self) -> Option<&[CurrencyCode]> {
        self.currency_codes.as_deref()
    }
    /// Select localized DateTime calendar rows. Global calendar kernels and
    /// LocaleInfo availability remain complete.
    pub fn for_date_time_calendars(
        id: CustomProfileId,
        calendars: &[&str],
    ) -> Result<Self, InvalidCustomIntlProfile> {
        Ok(Self {
            id,
            list: None,
            relative_time: None,
            display_names: None,
            duration: None,
            number: None,
            date_time: None,
            collator: None,
            segmenter: None,
            currency_codes: None,
            date_time_calendars: Some(
                checked_calendar_projection(calendars)
                    .map_err(InvalidCustomIntlProfile::DateTimeCalendars)?,
            ),
            numbering_systems: None,
            named_time_zones: None,
            services: None,
        })
    }
    pub fn with_date_time_calendars(
        mut self,
        calendars: &[&str],
    ) -> Result<Self, InvalidCustomIntlProfile> {
        self.date_time_calendars = Some(
            checked_calendar_projection(calendars)
                .map_err(InvalidCustomIntlProfile::DateTimeCalendars)?,
        );
        self.validate_service_dimensions()?;
        Ok(self)
    }
    pub fn date_time_calendars(&self) -> Option<&[crate::DateTimeCalendar]> {
        self.date_time_calendars.as_deref()
    }
    /// Select localized Number and DateTime numbering associations. Each
    /// producer retains actual defaults and the complete global digit authority.
    pub fn for_numbering_systems(
        id: CustomProfileId,
        systems: &[&str],
    ) -> Result<Self, InvalidCustomIntlProfile> {
        Ok(Self {
            id,
            list: None,
            relative_time: None,
            display_names: None,
            duration: None,
            number: None,
            date_time: None,
            collator: None,
            segmenter: None,
            currency_codes: None,
            date_time_calendars: None,
            numbering_systems: Some(
                checked_numbering_projection(systems)
                    .map_err(InvalidCustomIntlProfile::NumberingSystems)?,
            ),
            named_time_zones: None,
            services: None,
        })
    }
    pub fn with_numbering_systems(
        mut self,
        systems: &[&str],
    ) -> Result<Self, InvalidCustomIntlProfile> {
        self.numbering_systems = Some(
            checked_numbering_projection(systems)
                .map_err(InvalidCustomIntlProfile::NumberingSystems)?,
        );
        self.validate_service_dimensions()?;
        Ok(self)
    }
    pub fn numbering_systems(&self) -> Option<&[NumberingSystemOption]> {
        self.numbering_systems.as_deref()
    }
    /// Select actual named transition records. Original identity/country data
    /// remain complete; known omitted transition data report an explicit error.
    pub fn for_named_time_zones(
        id: CustomProfileId,
        zones: &[&str],
    ) -> Result<Self, InvalidCustomIntlProfile> {
        Ok(Self {
            id,
            list: None,
            relative_time: None,
            display_names: None,
            duration: None,
            number: None,
            date_time: None,
            collator: None,
            segmenter: None,
            currency_codes: None,
            date_time_calendars: None,
            numbering_systems: None,
            named_time_zones: Some(
                checked_named_time_zone_projection(zones)
                    .map_err(InvalidCustomIntlProfile::NamedTimeZones)?,
            ),
            services: None,
        })
    }
    pub fn with_named_time_zones(
        mut self,
        zones: &[&str],
    ) -> Result<Self, InvalidCustomIntlProfile> {
        self.named_time_zones = Some(
            checked_named_time_zone_projection(zones)
                .map_err(InvalidCustomIntlProfile::NamedTimeZones)?,
        );
        self.validate_service_dimensions()?;
        Ok(self)
    }
    pub fn named_time_zones(&self) -> Option<&[crate::TimeZoneId]> {
        self.named_time_zones.as_deref()
    }

    pub fn for_services(
        id: CustomProfileId,
        services: &[&str],
    ) -> Result<Self, InvalidCustomIntlProfile> {
        Self {
            id,
            list: None,
            relative_time: None,
            display_names: None,
            duration: None,
            number: None,
            date_time: None,
            collator: None,
            segmenter: None,
            currency_codes: None,
            date_time_calendars: None,
            numbering_systems: None,
            named_time_zones: None,
            services: None,
        }
        .with_services(services)
    }
    pub fn with_services(mut self, services: &[&str]) -> Result<Self, InvalidCustomIntlProfile> {
        let mut requested = IntlServiceSet::EMPTY;
        for name in services {
            let service = IntlService::ALL
                .iter()
                .copied()
                .find(|service| service.name() == *name)
                .ok_or_else(|| InvalidCustomIntlProfile::InvalidService((*name).into()))?;
            if requested.contains(service) {
                return Err(InvalidCustomIntlProfile::DuplicateService((*name).into()));
            }
            requested = requested.with(service);
        }
        self.services = Some(
            CheckedIntlServiceSelection::new(requested)
                .map_err(|_| InvalidCustomIntlProfile::EmptyServices)?,
        );
        self.validate_service_dimensions()?;
        Ok(self)
    }
    pub fn service_selection(&self) -> Option<&CheckedIntlServiceSelection> {
        self.services.as_ref()
    }

    fn validate_service_dimensions(&self) -> Result<(), InvalidCustomIntlProfile> {
        let Some(selection) = &self.services else {
            return Ok(());
        };
        use IntlDataComponent as C;
        for (present, component, dimension) in [
            (self.list.is_some(), C::List, "list"),
            (
                self.relative_time.is_some(),
                C::RelativeTime,
                "relative_time",
            ),
            (
                self.display_names.is_some(),
                C::DisplayNames,
                "display_names",
            ),
            (self.duration.is_some(), C::Duration, "duration"),
            (self.number.is_some(), C::Number, "number_plural"),
            (self.date_time.is_some(), C::DateTime, "date_time"),
            (self.collator.is_some(), C::Collator, "collator"),
            (self.segmenter.is_some(), C::Segmenter, "segmenter"),
            (self.currency_codes.is_some(), C::Number, "currency_codes"),
            (
                self.currency_codes.is_some(),
                C::DisplayNames,
                "currency_codes",
            ),
            (
                self.date_time_calendars.is_some(),
                C::DateTime,
                "date_time_calendars",
            ),
            (
                self.numbering_systems.is_some(),
                C::Number,
                "numbering_systems",
            ),
            (
                self.numbering_systems.is_some(),
                C::DateTime,
                "numbering_systems",
            ),
            (
                self.named_time_zones.is_some(),
                C::NamedTimeZones,
                "named_time_zones",
            ),
        ] {
            if present && !selection.contains_component(component) {
                return Err(InvalidCustomIntlProfile::UnusedDimension(dimension));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidCustomNamedTimeZoneProjection {
    Empty,
    Invalid(crate::InvalidTimeZoneId),
    Unknown(crate::UnknownTimeZone),
    Duplicate(Box<str>),
    Source(crate::IntlDataImageError),
}
impl core::fmt::Display for InvalidCustomNamedTimeZoneProjection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => f.write_str("named-zone selection must not be empty"),
            Self::Invalid(error) => write!(f, "invalid named-zone selection: {error}"),
            Self::Unknown(error) => write!(f, "unknown selected IANA zone: {error}"),
            Self::Duplicate(zone) => write!(f, "duplicate selected primary IANA zone {zone}"),
            Self::Source(error) => write!(f, "named-zone source admission failed: {error}"),
        }
    }
}
impl std::error::Error for InvalidCustomNamedTimeZoneProjection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Invalid(error) => Some(error),
            Self::Unknown(error) => Some(error),
            Self::Source(error) => Some(error),
            Self::Empty | Self::Duplicate(_) => None,
        }
    }
}
fn checked_named_time_zone_projection(
    zones: &[&str],
) -> Result<Box<[crate::TimeZoneId]>, InvalidCustomNamedTimeZoneProjection> {
    if zones.is_empty() {
        return Err(InvalidCustomNamedTimeZoneProjection::Empty);
    }
    let source = crate::named_time_zone_image::embedded_named_time_zone_data_image_ref()
        .map_err(InvalidCustomNamedTimeZoneProjection::Source)?;
    let mut selected = zones
        .iter()
        .map(|zone| {
            let id = crate::TimeZoneId::parse(*zone)
                .map_err(InvalidCustomNamedTimeZoneProjection::Invalid)?;
            let identity = source
                .zones_ref()
                .lookup(&id)
                .map_err(InvalidCustomNamedTimeZoneProjection::Unknown)?;
            crate::TimeZoneId::parse(identity.primary_identifier())
                .map_err(InvalidCustomNamedTimeZoneProjection::Invalid)
        })
        .collect::<Result<Vec<_>, _>>()?;
    selected.sort_unstable();
    if let Some(pair) = selected.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(InvalidCustomNamedTimeZoneProjection::Duplicate(
            pair[0].as_str().into(),
        ));
    }
    Ok(selected.into_boxed_slice())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidCustomNumberingProjection {
    Empty,
    Malformed(String),
    Duplicate(String),
    Unavailable(String),
    Allocation,
    Source(crate::IntlDataImageError),
}
impl core::fmt::Display for InvalidCustomNumberingProjection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => f.write_str("numbering-system selection must not be empty"),
            Self::Malformed(name) => write!(f, "invalid numbering-system name {name}"),
            Self::Duplicate(name) => write!(f, "duplicate canonical numbering system {name}"),
            Self::Unavailable(name) => {
                write!(f, "numbering system {name} has no pinned decimal data")
            }
            Self::Allocation => f.write_str("numbering-system selection allocation failed"),
            Self::Source(error) => write!(f, "numbering-system source admission failed: {error}"),
        }
    }
}
impl std::error::Error for InvalidCustomNumberingProjection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Empty
            | Self::Malformed(_)
            | Self::Duplicate(_)
            | Self::Unavailable(_)
            | Self::Allocation => None,
        }
    }
}
fn checked_numbering_projection(
    systems: &[&str],
) -> Result<Box<[NumberingSystemOption]>, InvalidCustomNumberingProjection> {
    if systems.is_empty() {
        return Err(InvalidCustomNumberingProjection::Empty);
    }
    let mut selected = systems
        .iter()
        .map(|name| {
            NumberingSystemOption::parse(name).map_err(|error| match error {
                InvalidNumberingSystemOption::Syntax => {
                    InvalidCustomNumberingProjection::Malformed((*name).to_owned())
                }
                InvalidNumberingSystemOption::Allocation => {
                    InvalidCustomNumberingProjection::Allocation
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    selected.sort_unstable_by(|left, right| left.name().cmp(right.name()));
    if let Some(pair) = selected.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(InvalidCustomNumberingProjection::Duplicate(
            pair[0].name().to_owned(),
        ));
    }
    let source = crate::number_image::pinned_number_source()
        .map_err(InvalidCustomNumberingProjection::Source)?;
    for system in &selected {
        if !source
            .profiles()
            .numbering_systems()
            .iter()
            .any(|name| name.as_ref() == system.name())
        {
            return Err(InvalidCustomNumberingProjection::Unavailable(
                system.name().to_owned(),
            ));
        }
    }
    Ok(selected.into_boxed_slice())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidCustomCalendarProjection {
    Empty,
    Unknown(String),
    Duplicate(String),
}
impl core::fmt::Display for InvalidCustomCalendarProjection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => f.write_str("DateTime calendar selection must not be empty"),
            Self::Unknown(calendar) => write!(f, "unknown canonical DateTime calendar {calendar}"),
            Self::Duplicate(calendar) => write!(f, "duplicate DateTime calendar {calendar}"),
        }
    }
}
impl std::error::Error for InvalidCustomCalendarProjection {}
fn checked_calendar_projection(
    calendars: &[&str],
) -> Result<Box<[crate::DateTimeCalendar]>, InvalidCustomCalendarProjection> {
    if calendars.is_empty() {
        return Err(InvalidCustomCalendarProjection::Empty);
    }
    let mut selected = calendars
        .iter()
        .map(|calendar| {
            crate::DateTimeCalendar::parse(calendar)
                .ok_or_else(|| InvalidCustomCalendarProjection::Unknown((*calendar).to_owned()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    selected.sort_unstable_by_key(|calendar| calendar.as_str());
    if let Some(pair) = selected.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(InvalidCustomCalendarProjection::Duplicate(
            pair[0].as_str().to_owned(),
        ));
    }
    Ok(selected.into_boxed_slice())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidCustomCurrencyProjection {
    Empty,
    Malformed(String),
    Duplicate(String),
    Unavailable(String),
    Source(crate::IntlDataImageError),
}
impl core::fmt::Display for InvalidCustomCurrencyProjection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => f.write_str("currency selection must not be empty"),
            Self::Malformed(code) => write!(f, "invalid currency code {code}"),
            Self::Duplicate(code) => write!(f, "duplicate canonical currency code {code}"),
            Self::Unavailable(code) => {
                write!(f, "currency code {code} has no pinned localized data")
            }
            Self::Source(error) => write!(f, "currency source admission failed: {error}"),
        }
    }
}
impl std::error::Error for InvalidCustomCurrencyProjection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Empty | Self::Malformed(_) | Self::Duplicate(_) | Self::Unavailable(_) => None,
        }
    }
}
fn checked_currency_projection(
    codes: &[&str],
) -> Result<Box<[CurrencyCode]>, InvalidCustomCurrencyProjection> {
    if codes.is_empty() {
        return Err(InvalidCustomCurrencyProjection::Empty);
    }
    let mut selected = codes
        .iter()
        .map(|code| {
            CurrencyCode::parse(code)
                .map_err(|_| InvalidCustomCurrencyProjection::Malformed((*code).to_owned()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    selected.sort_unstable_by_key(|code| code.clone().ascii());
    let text = |code: &CurrencyCode| {
        code.clone()
            .ascii()
            .into_iter()
            .map(char::from)
            .collect::<String>()
    };
    if let Some(pair) = selected.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(InvalidCustomCurrencyProjection::Duplicate(text(&pair[0])));
    }
    let source = crate::number_image::pinned_number_source()
        .map_err(InvalidCustomCurrencyProjection::Source)?;
    for code in &selected {
        if !source.profiles().has_currency_data("en-US", code) {
            return Err(InvalidCustomCurrencyProjection::Unavailable(text(code)));
        }
    }
    Ok(selected.into_boxed_slice())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidCustomIntlProfile {
    NoProjection,
    ListLocales(InvalidCustomLocaleProjection),
    RelativeTimeLocales(InvalidCustomLocaleProjection),
    DisplayNamesLocales(InvalidCustomLocaleProjection),
    DurationLocales(InvalidCustomLocaleProjection),
    NumberLocales(InvalidCustomLocaleProjection),
    DateTimeLocales(InvalidCustomLocaleProjection),
    CollatorLocales(InvalidCustomLocaleProjection),
    SegmenterLocales(InvalidCustomLocaleProjection),
    CurrencyCodes(InvalidCustomCurrencyProjection),
    DateTimeCalendars(InvalidCustomCalendarProjection),
    NumberingSystems(InvalidCustomNumberingProjection),
    NamedTimeZones(InvalidCustomNamedTimeZoneProjection),
    EmptyServices,
    InvalidService(String),
    DuplicateService(String),
    UnusedDimension(&'static str),
}

impl core::fmt::Display for InvalidCustomIntlProfile {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoProjection => {
                formatter.write_str("a custom projection needs a locale or data filter")
            }
            Self::ListLocales(error) => write!(formatter, "invalid List locale filter: {error}"),
            Self::RelativeTimeLocales(error) => {
                write!(formatter, "invalid RelativeTime locale filter: {error}")
            }
            Self::DisplayNamesLocales(error) => {
                write!(formatter, "invalid DisplayNames locale filter: {error}")
            }
            Self::DurationLocales(error) => {
                write!(formatter, "invalid Duration locale filter: {error}")
            }
            Self::NumberLocales(error) => {
                write!(formatter, "invalid Number/Plural locale filter: {error}")
            }
            Self::DateTimeLocales(error) => {
                write!(formatter, "invalid DateTime locale filter: {error}")
            }
            Self::CollatorLocales(error) => {
                write!(formatter, "invalid Collator locale filter: {error}")
            }
            Self::SegmenterLocales(error) => {
                write!(formatter, "invalid Segmenter locale filter: {error}")
            }
            Self::CurrencyCodes(error) => write!(formatter, "invalid currency filter: {error}"),
            Self::DateTimeCalendars(error) => {
                write!(formatter, "invalid DateTime calendar filter: {error}")
            }
            Self::NumberingSystems(error) => {
                write!(formatter, "invalid numbering-system filter: {error}")
            }
            Self::NamedTimeZones(error) => write!(formatter, "invalid named-zone filter: {error}"),
            Self::EmptyServices => formatter.write_str("Custom Intl services must not be empty"),
            Self::InvalidService(name) => write!(formatter, "unknown Custom Intl service {name:?}"),
            Self::DuplicateService(name) => {
                write!(formatter, "duplicate Custom Intl service {name:?}")
            }
            Self::UnusedDimension(name) => write!(
                formatter,
                "Custom Intl dimension {name:?} targets an omitted data component"
            ),
        }
    }
}

impl std::error::Error for InvalidCustomIntlProfile {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoProjection
            | Self::EmptyServices
            | Self::InvalidService(_)
            | Self::DuplicateService(_)
            | Self::UnusedDimension(_) => None,
            Self::CurrencyCodes(error) => Some(error),
            Self::DateTimeCalendars(error) => Some(error),
            Self::NumberingSystems(error) => Some(error),
            Self::NamedTimeZones(error) => Some(error),
            Self::ListLocales(error)
            | Self::RelativeTimeLocales(error)
            | Self::DisplayNamesLocales(error)
            | Self::DurationLocales(error)
            | Self::NumberLocales(error)
            | Self::DateTimeLocales(error)
            | Self::CollatorLocales(error)
            | Self::SegmenterLocales(error) => Some(error),
        }
    }
}

/// Cheap checked input retained until selected Locale-backed image admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomListProfile {
    id: CustomProfileId,
    locales: Box<[LocaleId]>,
}

impl CustomListProfile {
    pub fn new(
        id: CustomProfileId,
        locales: &[&str],
    ) -> Result<Self, InvalidCustomLocaleProjection> {
        Ok(Self {
            id,
            locales: checked_locale_projection(locales)?,
        })
    }

    pub fn id(&self) -> &CustomProfileId {
        &self.id
    }

    pub fn requested_locales(&self) -> &[LocaleId] {
        &self.locales
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidCustomLocaleProjection {
    Empty,
    InvalidLocale(InvalidLocaleId),
    DuplicateLocale(Box<str>),
}

impl core::fmt::Display for InvalidCustomLocaleProjection {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => {
                formatter.write_str("a custom component projection needs at least one locale")
            }
            Self::InvalidLocale(error) => core::fmt::Display::fmt(error, formatter),
            Self::DuplicateLocale(locale) => {
                write!(formatter, "duplicate custom component locale: {locale}")
            }
        }
    }
}

impl std::error::Error for InvalidCustomLocaleProjection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidLocale(error) => Some(error),
            Self::Empty | Self::DuplicateLocale(_) => None,
        }
    }
}

fn checked_locale_projection(
    locales: &[&str],
) -> Result<Box<[LocaleId]>, InvalidCustomLocaleProjection> {
    if locales.is_empty() {
        return Err(InvalidCustomLocaleProjection::Empty);
    }
    let mut locales = locales
        .iter()
        .map(|locale| {
            LocaleId::parse(*locale).map_err(InvalidCustomLocaleProjection::InvalidLocale)
        })
        .collect::<Result<Vec<_>, _>>()?;
    locales.sort();
    if let Some(pair) = locales.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(InvalidCustomLocaleProjection::DuplicateLocale(
            pair[0].as_str().into(),
        ));
    }
    Ok(locales.into_boxed_slice())
}

impl IntlCompilationProfile {
    fn data_profile(&self) -> IntlDataProfile {
        match self {
            Self::Minimal => IntlDataProfile::Minimal,
            Self::Conformance => IntlDataProfile::Conformance,
            Self::Custom(id) => IntlDataProfile::Custom(id.clone()),
            Self::CustomProjection(selection) => IntlDataProfile::Custom(selection.id().clone()),
        }
    }
}

/// One emission retains one immutable selection through all builtin retries.
/// Construction is cheap; a program without Intl never resolves its data.
pub struct IntlDataSelection {
    profile: IntlCompilationProfile,
    custom: OnceLock<Result<Arc<SelectedIntlDataBundle>, EmbeddedIntlProviderSetupError>>,
    compilation: OnceLock<Result<Arc<crate::IntlCompilationData>, EmbeddedIntlProviderSetupError>>,
    initialize_compilation: Option<
        Box<
            dyn Fn() -> Result<Arc<crate::IntlCompilationData>, EmbeddedIntlProviderSetupError>
                + Send
                + Sync,
        >,
    >,
}

impl core::fmt::Debug for IntlDataSelection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("IntlDataSelection")
            .field("profile", &self.profile)
            .field("compilation_initialized", &self.compilation.get().is_some())
            .finish_non_exhaustive()
    }
}

impl IntlDataSelection {
    pub fn new(profile: IntlCompilationProfile) -> Self {
        Self {
            profile,
            custom: OnceLock::new(),
            compilation: OnceLock::new(),
            initialize_compilation: None,
        }
    }

    pub fn selected(&self) -> Result<&SelectedIntlDataBundle, &EmbeddedIntlProviderSetupError> {
        self.selected_with_jobs(std::num::NonZeroUsize::MIN)
    }

    pub fn compilation_data(
        &self,
    ) -> Result<&Arc<crate::IntlCompilationData>, &EmbeddedIntlProviderSetupError> {
        self.compilation_data_with_jobs(std::num::NonZeroUsize::MIN)
    }

    pub fn compilation_data_with_jobs(
        &self,
        jobs: std::num::NonZeroUsize,
    ) -> Result<&Arc<crate::IntlCompilationData>, &EmbeddedIntlProviderSetupError> {
        self.compilation
            .get_or_init(|| {
                if let Some(initialize) = &self.initialize_compilation {
                    let data = initialize()?;
                    if data.identity().profile().profile() != &IntlDataProfile::Minimal
                        || data.identity().profile().services() != IntlServiceSet::ALL
                        || data.service_selection().is_some()
                    {
                        return Err(EmbeddedIntlProviderSetupError::DataImage(
                            crate::IntlDataImageError::consumer(
                                "deferred compiler metadata is not complete canonical Minimal",
                            ),
                        ));
                    }
                    return Ok(data);
                }
                crate::IntlCompilationData::from_selected(self.selected_with_jobs(jobs).map_err(
                    |error| {
                        EmbeddedIntlProviderSetupError::DataImage(
                            crate::IntlDataImageError::consumer(error),
                        )
                    },
                )?)
                .map(Arc::new)
            })
            .as_ref()
    }

    /// Delay even frame hashing until a compiler operation needs metadata.
    /// The initializer must return the same opaque admitted Minimal owner;
    /// ordinary scalar compilation never invokes it.
    pub fn defer_minimal_compilation_data(
        initialize: impl Fn() -> Result<Arc<crate::IntlCompilationData>, EmbeddedIntlProviderSetupError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self {
            profile: IntlCompilationProfile::Minimal,
            custom: OnceLock::new(),
            compilation: OnceLock::new(),
            initialize_compilation: Some(Box::new(initialize)),
        }
    }

    /// Reuse a previously admitted complete canonical Minimal compilation owner.
    /// Custom projections must still resolve their exact requested frame closure.
    pub fn from_minimal_compilation_data(data: Arc<crate::IntlCompilationData>) -> Option<Self> {
        if data.identity().profile().profile() != &IntlDataProfile::Minimal
            || data.identity().profile().services() != IntlServiceSet::ALL
            || data.service_selection().is_some()
        {
            return None;
        }
        Some(Self {
            profile: IntlCompilationProfile::Minimal,
            custom: OnceLock::new(),
            compilation: OnceLock::from(Ok(data)),
            initialize_compilation: None,
        })
    }

    /// Admit the same immutable data, with explicitly bounded concurrent
    /// initialization of independent canonical Minimal components.
    /// Custom profiles retain their existing selection and admission order.
    pub fn selected_with_jobs(
        &self,
        jobs: std::num::NonZeroUsize,
    ) -> Result<&SelectedIntlDataBundle, &EmbeddedIntlProviderSetupError> {
        match &self.profile {
            IntlCompilationProfile::Minimal => {
                static MINIMAL: OnceLock<
                    Result<SelectedIntlDataBundle, EmbeddedIntlProviderSetupError>,
                > = OnceLock::new();
                MINIMAL
                    .get_or_init(|| {
                        minimal_admission::prewarm(jobs)?;
                        SelectedIntlDataBundle::build(&IntlCompilationProfile::Minimal)
                    })
                    .as_ref()
            }
            IntlCompilationProfile::Conformance
            | IntlCompilationProfile::Custom(_)
            | IntlCompilationProfile::CustomProjection(_) => self
                .custom
                .get_or_init(|| {
                    static SELECTED: SelectedProfileCache = SelectedProfileCache::new();
                    SELECTED.select(&self.profile, SELECTED_PROFILE_CACHE_BYTES)
                })
                .as_ref()
                .map(Arc::as_ref),
        }
    }
}

// Retain at most one complete immutable selection and at most 64 MiB of its
// actual frame bytes. This is a serialized-data budget, not a claim about total
// provider heap usage. Active callers independently own their admitted bundle.
// Full typed profile equality includes all filters, services and Custom identity.
const SELECTED_PROFILE_CACHE_BYTES: usize = 64 * 1024 * 1024;

struct SelectedProfileCache {
    entry: Mutex<Option<(IntlCompilationProfile, Arc<SelectedIntlDataBundle>)>>,
}

impl SelectedProfileCache {
    const fn new() -> Self {
        Self {
            entry: Mutex::new(None),
        }
    }

    fn select(
        &self,
        profile: &IntlCompilationProfile,
        byte_limit: usize,
    ) -> Result<Arc<SelectedIntlDataBundle>, EmbeddedIntlProviderSetupError> {
        // The producer does not recurse through IntlDataSelection. Serialize
        // construction so concurrent identical requests share one admitted owner.
        let mut entry = self
            .entry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((cached_profile, bundle)) = entry.as_ref() {
            if cached_profile == profile {
                return Ok(Arc::clone(bundle));
            }
        }
        // Every miss still uses the original checked producers and provider.
        // Failed selections never replace the previous admitted cache entry.
        let bundle = Arc::new(SelectedIntlDataBundle::build(profile)?);
        let bytes = bundle
            .component_sections()
            .iter()
            .try_fold(0usize, |total, (_, frame)| total.checked_add(frame.len()));
        *entry = bytes
            .filter(|bytes| *bytes <= byte_limit)
            .map(|_| (profile.clone(), Arc::clone(&bundle)));
        Ok(bundle)
    }
}

/// Constructor-admitted component topology and its actual provider identity.
/// Private fields prevent replacing a frame independently of its catalogue.
#[derive(Debug)]
pub struct SelectedIntlDataBundle {
    locale: LocaleDataImage,
    lists: Option<ListDataImage>,
    collators: Option<CollatorDataImage>,
    numbers: Option<NumberProfilesDataImage>,
    segmenters: Option<SegmenterDataImage>,
    display_names: Option<DisplayNamesDataImage>,
    relative_times: Option<RelativeTimeDataImage>,
    durations: Option<DurationDataImage>,
    named_zones: Option<NamedTimeZoneDataImage>,
    date_time: Option<DateTimeDataImage>,
    time_zone_names: Option<TimeZoneNamesDataImage>,
    locale_information: Option<NativeLocaleInformationDataImage>,
    service_selection: Option<CheckedIntlServiceSelection>,
    provider: EmbeddedIntlProvider,
    catalogues: [OnceLock<Result<Arc<SupportedValuesList>, SupportedValuesSetupError>>; 6],
}

impl SelectedIntlDataBundle {
    fn build(profile: &IntlCompilationProfile) -> Result<Self, EmbeddedIntlProviderSetupError> {
        let data_profile = profile.data_profile();
        let service_selection = match profile {
            IntlCompilationProfile::CustomProjection(profile) => {
                profile.service_selection().cloned()
            }
            _ => None,
        };
        let checked = service_selection.clone().unwrap_or_else(|| {
            CheckedIntlServiceSelection::new(IntlServiceSet::ALL).expect("ALL is nonempty")
        });
        macro_rules! component {
            ($kind:ident, $value:expr) => {
                if checked.contains_component(IntlDataComponent::$kind) {
                    Some($value.map_err(EmbeddedIntlProviderSetupError::DataImage)?)
                } else {
                    None
                }
            };
        }
        fn required<T>(value: &Option<T>) -> Result<&T, EmbeddedIntlProviderSetupError> {
            value
                .as_ref()
                .ok_or(EmbeddedIntlProviderSetupError::ReservedLanguageData(
                    "missing checked Intl component dependency",
                ))
        }
        macro_rules! select {
            ($embedded:ident, $custom:expr) => {
                match profile {
                    IntlCompilationProfile::Minimal => $embedded(),
                    IntlCompilationProfile::Conformance
                    | IntlCompilationProfile::Custom(_)
                    | IntlCompilationProfile::CustomProjection(_) => $custom,
                }
                .map_err(EmbeddedIntlProviderSetupError::DataImage)?
            };
        }
        let locale = select!(
            embedded_locale_data_image,
            LocaleDataImage::for_profile(data_profile.clone())
        );
        let lists = component!(
            List,
            match profile {
                IntlCompilationProfile::Minimal => embedded_list_data_image(),
                IntlCompilationProfile::Conformance | IntlCompilationProfile::Custom(_) => {
                    ListDataImage::for_profile(data_profile.clone(), &locale)
                }
                IntlCompilationProfile::CustomProjection(selection) => {
                    match selection.list_projection() {
                        Some(lists) => ListDataImage::for_custom_projection(lists, &locale),
                        None => ListDataImage::for_profile(data_profile.clone(), &locale),
                    }
                }
            }
        );
        let collators = component!(
            Collator,
            match profile {
                IntlCompilationProfile::Minimal => embedded_collator_data_image(),
                IntlCompilationProfile::Conformance | IntlCompilationProfile::Custom(_) => {
                    CollatorDataImage::for_profile(data_profile.clone(), &locale)
                }
                IntlCompilationProfile::CustomProjection(selection) => {
                    match selection.collator_locales() {
                        Some(locales) => CollatorDataImage::for_custom_projection(
                            selection.id(),
                            locales,
                            &locale,
                        ),
                        None => CollatorDataImage::for_profile(data_profile.clone(), &locale),
                    }
                }
            }
        );
        let numbers = component!(
            Number,
            match profile {
                IntlCompilationProfile::Minimal => embedded_number_profiles_data_image(),
                IntlCompilationProfile::Conformance | IntlCompilationProfile::Custom(_) => {
                    NumberProfilesDataImage::for_profile(data_profile.clone(), &locale)
                }
                IntlCompilationProfile::CustomProjection(selection) => {
                    match (
                        selection.number_locales(),
                        selection.currency_codes(),
                        selection.numbering_systems(),
                    ) {
                        (locales, codes, Some(systems)) => {
                            NumberProfilesDataImage::for_custom_numbering_projection(
                                selection.id(),
                                locales,
                                codes,
                                systems,
                                selection.relative_time_locales(),
                                selection.duration_locales(),
                                &locale,
                                required(&lists)?,
                            )
                        }
                        (locales, Some(codes), None) => {
                            NumberProfilesDataImage::for_custom_data_projection(
                                selection.id(),
                                locales,
                                codes,
                                selection.relative_time_locales(),
                                selection.duration_locales(),
                                &locale,
                                required(&lists)?,
                            )
                        }
                        (Some(locales), None, None) => {
                            NumberProfilesDataImage::for_custom_projection(
                                selection.id(),
                                locales,
                                selection.relative_time_locales(),
                                selection.duration_locales(),
                                &locale,
                                required(&lists)?,
                            )
                        }
                        (None, None, None) => {
                            NumberProfilesDataImage::for_profile(data_profile.clone(), &locale)
                        }
                    }
                }
            }
        );
        let segmenters = component!(
            Segmenter,
            match profile {
                IntlCompilationProfile::Minimal => embedded_segmenter_data_image(),
                IntlCompilationProfile::Conformance | IntlCompilationProfile::Custom(_) => {
                    SegmenterDataImage::for_profile(data_profile.clone(), &locale)
                }
                IntlCompilationProfile::CustomProjection(selection) => {
                    match selection.segmenter_locales() {
                        Some(locales) => SegmenterDataImage::for_custom_projection(
                            selection.id(),
                            locales,
                            &locale,
                        ),
                        None => SegmenterDataImage::for_profile(data_profile.clone(), &locale),
                    }
                }
            }
        );
        let display_names = component!(
            DisplayNames,
            match profile {
                IntlCompilationProfile::Minimal => embedded_display_names_data_image(),
                IntlCompilationProfile::Conformance | IntlCompilationProfile::Custom(_) => {
                    DisplayNamesDataImage::for_profile(data_profile.clone(), &locale)
                }
                IntlCompilationProfile::CustomProjection(selection) => {
                    match (
                        selection.display_names_locales(),
                        selection.currency_codes(),
                    ) {
                        (locales, Some(codes)) => {
                            DisplayNamesDataImage::for_custom_data_projection(
                                selection.id(),
                                locales,
                                codes,
                                &locale,
                            )
                        }
                        (Some(locales), None) => DisplayNamesDataImage::for_custom_projection(
                            selection.id(),
                            locales,
                            &locale,
                        ),
                        (None, None) => {
                            DisplayNamesDataImage::for_profile(data_profile.clone(), &locale)
                        }
                    }
                }
            }
        );
        let relative_times = component!(
            RelativeTime,
            match profile {
                IntlCompilationProfile::Minimal => embedded_relative_time_data_image(),
                IntlCompilationProfile::Conformance | IntlCompilationProfile::Custom(_) => {
                    RelativeTimeDataImage::for_profile(
                        data_profile.clone(),
                        &locale,
                        required(&numbers)?,
                    )
                }
                IntlCompilationProfile::CustomProjection(selection) =>
                    match selection.relative_time_locales() {
                        Some(locales) => RelativeTimeDataImage::for_custom_projection(
                            selection.id(),
                            locales,
                            &locale,
                            required(&numbers)?,
                        ),
                        None => RelativeTimeDataImage::for_profile(
                            data_profile.clone(),
                            &locale,
                            required(&numbers)?
                        ),
                    },
            }
        );
        let durations = component!(
            Duration,
            match profile {
                IntlCompilationProfile::Minimal => embedded_duration_data_image(),
                IntlCompilationProfile::Conformance | IntlCompilationProfile::Custom(_) => {
                    DurationDataImage::for_profile(
                        data_profile.clone(),
                        &locale,
                        required(&numbers)?,
                        required(&lists)?,
                    )
                }
                IntlCompilationProfile::CustomProjection(selection) =>
                    match selection.duration_locales() {
                        Some(locales) => DurationDataImage::for_custom_projection(
                            selection.id(),
                            locales,
                            &locale,
                            required(&numbers)?,
                            required(&lists)?,
                        ),
                        None => {
                            DurationDataImage::for_profile(
                                data_profile.clone(),
                                &locale,
                                required(&numbers)?,
                                required(&lists)?,
                            )
                        }
                    },
            }
        );
        let named_zones = component!(
            NamedTimeZones,
            match profile {
                IntlCompilationProfile::Minimal => embedded_named_time_zone_data_image(),
                IntlCompilationProfile::Conformance | IntlCompilationProfile::Custom(_) =>
                    NamedTimeZoneDataImage::for_profile(data_profile.clone()),
                IntlCompilationProfile::CustomProjection(selection) =>
                    match selection.named_time_zones() {
                        Some(zones) =>
                            NamedTimeZoneDataImage::for_custom_projection(selection.id(), zones),
                        None => NamedTimeZoneDataImage::for_profile(data_profile.clone()),
                    },
            }
        );
        let date_time = component!(
            DateTime,
            match profile {
                IntlCompilationProfile::Minimal => embedded_date_time_data_image(),
                IntlCompilationProfile::Conformance | IntlCompilationProfile::Custom(_) => {
                    DateTimeDataImage::for_profile(
                        data_profile.clone(),
                        &locale,
                        required(&named_zones)?,
                    )
                }
                IntlCompilationProfile::CustomProjection(selection) => {
                    match (
                        selection.date_time_locales(),
                        selection.date_time_calendars(),
                        selection.numbering_systems(),
                    ) {
                        (locales, calendars, Some(systems)) => {
                            DateTimeDataImage::for_custom_numbering_projection(
                                selection.id(),
                                locales,
                                calendars,
                                systems,
                                &locale,
                                required(&named_zones)?,
                            )
                        }
                        (locales, Some(calendars), None) => {
                            DateTimeDataImage::for_custom_data_projection(
                                selection.id(),
                                locales,
                                calendars,
                                &locale,
                                required(&named_zones)?,
                            )
                        }
                        (Some(locales), None, None) => DateTimeDataImage::for_custom_projection(
                            selection.id(),
                            locales,
                            &locale,
                            required(&named_zones)?,
                        ),
                        (None, None, None) => DateTimeDataImage::for_profile(
                            data_profile.clone(),
                            &locale,
                            required(&named_zones)?,
                        ),
                    }
                }
            }
        );
        let time_zone_names = if checked.contains_component(IntlDataComponent::TimeZoneNames) {
            Some(select!(
                embedded_time_zone_names_data_image,
                TimeZoneNamesDataImage::for_profile(data_profile.clone(), required(&named_zones)?)
            ))
        } else {
            None
        };
        let locale_information = if checked.contains_component(IntlDataComponent::LocaleInformation)
        {
            Some(select!(
                embedded_native_locale_information_data_image,
                NativeLocaleInformationDataImage::for_profile(
                    data_profile,
                    &locale,
                    required(&date_time)?
                )
            ))
        } else {
            None
        };
        let provider = EmbeddedIntlProvider::with_selected_data_images(
            checked,
            locale.clone(),
            lists.clone(),
            collators.clone(),
            numbers.clone(),
            segmenters.clone(),
            display_names.clone(),
            relative_times.clone(),
            durations.clone(),
            named_zones.clone(),
            date_time.clone(),
            time_zone_names.clone(),
            locale_information.clone(),
        )?;
        Ok(Self {
            locale,
            lists,
            collators,
            numbers,
            segmenters,
            display_names,
            relative_times,
            durations,
            named_zones,
            date_time,
            time_zone_names,
            locale_information,
            service_selection,
            provider,
            catalogues: std::array::from_fn(|_| OnceLock::new()),
        })
    }

    pub fn identity(&self) -> &IntlDataIdentity {
        self.provider.identity()
    }

    /// Each frame belongs to the same checked profile and captured foundations.
    pub fn service_selection(&self) -> Option<&CheckedIntlServiceSelection> {
        self.service_selection.as_ref()
    }

    pub fn into_provider(self) -> EmbeddedIntlProvider {
        self.provider
    }

    pub fn component_sections(&self) -> Vec<(&'static str, Arc<[u8]>)> {
        self.component_images()
            .into_iter()
            .map(|(component, bytes)| (component.section_name(), bytes))
            .collect()
    }

    /// The same physical frames consumed by Wasm emission and bundle export.
    /// Typed component identities also support data footprint accounting.
    pub fn component_images(&self) -> Vec<(IntlDataComponent, Arc<[u8]>)> {
        let mut sections = vec![(IntlDataComponent::Locale, self.locale.bytes())];
        macro_rules! add {
            ($component:ident, $image:expr) => {
                if let Some(image) = &$image {
                    sections.push((IntlDataComponent::$component, image.bytes()));
                }
            };
        }
        add!(List, self.lists);
        add!(Collator, self.collators);
        add!(Number, self.numbers);
        add!(Segmenter, self.segmenters);
        add!(DisplayNames, self.display_names);
        add!(RelativeTime, self.relative_times);
        add!(Duration, self.durations);
        add!(NamedTimeZones, self.named_zones);
        add!(DateTime, self.date_time);
        add!(TimeZoneNames, self.time_zone_names);
        add!(LocaleInformation, self.locale_information);
        sections
    }

    /// Owned immutable names validated by this exact selected provider.
    pub fn supported_values(
        &self,
        key: SupportedValuesKey,
    ) -> Result<Arc<SupportedValuesList>, &SupportedValuesSetupError> {
        self.catalogues[key.index()]
            .get_or_init(|| {
                SupportedValuesCatalogue::from_consumed_provider_key(&self.provider, key)
                    .map(Arc::new)
            })
            .as_ref()
            .map(Arc::clone)
    }
}
