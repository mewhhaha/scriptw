use crate::collator::COLLATOR_DATA_SHA256;
use crate::display_names::{DisplayNamesProfiles, DISPLAY_NAMES_KERNEL_SHA256};
use crate::duration_format::{
    format_duration_parts, DurationError, DurationPartition, DurationProfiles,
    DurationSupportedLocalesRequest, ResolvedDurationLocale, DURATION_FORMAT_DATA_SHA256,
};
use crate::duration_wire::DurationPartitionRequest;
use crate::list_format::LIST_FORMAT_DATA_SHA256;
use crate::plural_rules::{
    resolve_plural_locale, select_plural_operation, select_plural_range_operation,
    supported_plural_locales,
};
use crate::relative_time_format::{RelativeProfiles, RELATIVE_TIME_KERNEL_SHA256};
use crate::segmenter::{
    ResolvedSegmenterLocale, SegmentUtf16Request, SegmenterError, SegmenterLocaleRequest,
    SegmenterProfiles, SegmenterResult, SegmenterSupportedLocalesResult, SEGMENTER_KERNEL_SHA256,
};
use crate::{
    embedded_collator_data_image, embedded_date_time_data_image, embedded_display_names_data_image,
    embedded_duration_data_image, embedded_list_data_image, embedded_locale_data_image,
    embedded_named_time_zone_data_image, embedded_native_locale_information_data_image,
    embedded_number_profiles_data_image, embedded_relative_time_data_image,
    embedded_segmenter_data_image, embedded_time_zone_names_data_image, CollatorDataImage,
    DateTimeDataImage, DisplayNamesDataImage, DurationDataImage, IntlDataImageError, ListDataImage,
    LocaleDataImage, NamedTimeZoneDataImage, NativeLocaleInformationDataImage,
    NumberProfilesDataImage, RelativeTimeDataImage, SegmenterDataImage, TimeZoneNamesDataImage,
};
use crate::{
    CollatorLocaleRequest, CollatorOperationError, CollatorOrdering, CollatorProfiles,
    CollatorSupportedLocalesRequest, CollatorSupportedLocalesResult, CompareCollator,
    CompareCollatorRequest, ResolveCollatorLocale, ResolvedCollatorLocale,
    SupportedCollatorLocales,
};
use crate::{
    DisplayName, DisplayNameRequest, DisplayNameResult, DisplayNamesError,
    DisplayNamesLocaleRequest, DisplayNamesSupportedLocalesResult, FormatRelativeTimeParts,
    FormatRelativeTimePartsRequest, RelativePartition, RelativeTimeError,
    ResolveDisplayNamesLocale, ResolveRelativeTimeLocale, ResolvedDisplayNamesLocale,
    ResolvedRelativeTimeLocale, SupportedDisplayNamesLocales, SupportedRelativeTimeLocales,
};
use crate::{
    FormatListParts, FormatListPartsRequest, ListFormatOperationError, ListLocaleRequest,
    ListParts, ListProfiles, ListSupportedLocalesRequest, ListSupportedLocalesResult,
    ResolveListLocale, ResolvedListLocale, SupportedListLocales,
};
use crate::{PartitionDurationFormat, ResolveDurationFormatLocale, SupportedDurationFormatLocales};
use crate::{
    PluralCategory, PluralLocaleRequest, PluralRulesOperationError, PluralSupportedLocalesRequest,
    PluralSupportedLocalesResult, ResolvePluralLocale, ResolvedPluralLocale, SelectPlural,
    SelectPluralRange, SelectPluralRangeRequest, SelectPluralRequest, SupportedPluralLocales,
};
use crate::{ResolveSegmenterLocale, SegmentUtf16, SupportedSegmenterLocales};
use core::fmt;
use std::sync::Arc;

use crate::{
    FindNamedTimeZoneTransition, FindNamedTimeZoneTransitionRequest,
    FindNamedTimeZoneTransitionResult, NamedTimeZoneDataError, NamedTimeZoneOffset,
    NamedTimeZoneOffsetRequest, NamedTimeZoneOffsetSeconds, PossibleNamedTimeZoneEpochs,
    PossibleNamedTimeZoneEpochsRequest, PossibleNamedTimeZoneEpochsResult,
};

use sha2::{Digest as _, Sha256};

use crate::number_format::{
    filter_number_locales, resolve_number_locale, InvalidNumberProfile, NumberLocaleRequest,
    NumberProfiles, NumberSupportedLocalesRequest, PartitionLimits, RangeNumberPartition,
    ResolvedNumberLocale, ScalarNumberPartition, NUMBER_FORMAT_DATA_SHA256,
};
use crate::number_operation::{format_number_parts_operation, format_number_range_parts_operation};
use crate::{
    FormatNumberParts, FormatNumberRangeParts, NumberFormatOperationError, NumberFormatRequest,
    NumberRangeFormatRequest, NumberSupportedLocalesResult, ResolveNumberLocale,
    SupportedNumberLocales,
};

mod conformance;
mod datetime;
mod keyword_aliases;
mod language_domain;
#[cfg(test)]
mod likely_subtags_tests;
pub(crate) mod locale_calendars;
pub(crate) mod locale_collations;
pub(crate) mod locale_hour_cycles;
pub(crate) mod locale_numbering_systems;
pub(crate) mod locale_text;
pub(crate) mod locale_time_zones;
pub(crate) mod locale_week;
mod named_time_zones;
mod region_preference;
mod time_zone_names;
mod time_zone_snapshot;
pub(crate) use datetime::{DateTimeProvider, PreparedDateTimeProvider};
pub(crate) use keyword_aliases::KeywordAliasData;
pub(crate) use language_domain::LikelySubtags;
pub(crate) use language_domain::LocaleCanonicalizationData;
pub(crate) use named_time_zones::CertifiedNamedGapBoundary;
pub(crate) use named_time_zones::NamedTimeZones;
pub(crate) use time_zone_names::TimeZoneNames;

use crate::{
    CanonicalLocaleId, CanonicalizeLocale, DateTimeFormatError, DateTimeFormatRequest,
    DateTimeLocaleRequest, DateTimeLocaleResult, DateTimeParts, DateTimePlanRequest,
    DateTimePlanResult, DateTimeRangeParts, DateTimeRangeRequest, DateTimeSupportedLocalesRequest,
    DateTimeSupportedLocalesResult, EmptyIntlProfile, FormatDateTimeParts,
    FormatDateTimeRangeParts, IntlDataDigest, IntlDataIdentity, IntlDataPlacement,
    IntlOperationProvider, IntlProfilePlan, IntlProvider, IntlService, IntlServiceSet,
    InvalidCanonicalLocaleId, InvalidTimeZoneData, LocaleTransformError, LocaleTransformRequest,
    LocaleTransformResult, LookupNamedTimeZone, LookupNamedTimeZoneRequest,
    LookupNamedTimeZoneResult, MaximizeLocale, MinimizeLocale, ResolveDateTimeLocale,
    ResolveTimeZone, ResolveTimeZoneRequest, ResolvedTimeZoneSnapshot, SelectDateTimeFormat,
    SupportedDateTimeLocales, TimeZoneResolveError, TimeZoneSelection, UnknownTimeZone,
};

/// Composite SHA-256 of exact locale, IANA transition/catalogue and CLDR name
/// manifests. Recompute from their generated digests so no independently stored
/// outer digest can remain stale when a component is regenerated.
fn selected_intl_data_digest(
    locale: &LocaleDataImage,
    lists: &ListDataImage,
    collators: &CollatorDataImage,
    numbers: &NumberProfilesDataImage,
    segmenters: &SegmenterDataImage,
    display_names: &DisplayNamesDataImage,
    relative_times: &RelativeTimeDataImage,
    durations: &DurationDataImage,
    named_zones: &NamedTimeZoneDataImage,
    date_time: &DateTimeDataImage,
    time_zone_names: &TimeZoneNamesDataImage,
    locale_information: &NativeLocaleInformationDataImage,
) -> IntlDataDigest {
    let mut digest = Sha256::new();
    digest.update(b"scriptw-intl-provider-v21-immutable-components\0");
    digest.update(locale.digest().as_bytes());
    digest.update(lists.digest().as_bytes());
    digest.update(collators.digest().as_bytes());
    digest.update(numbers.digest().as_bytes());
    digest.update(segmenters.digest().as_bytes());
    digest.update(display_names.digest().as_bytes());
    digest.update(relative_times.digest().as_bytes());
    digest.update(durations.digest().as_bytes());
    digest.update(named_zones.digest().as_bytes());
    digest.update(date_time.digest().as_bytes());
    digest.update(time_zone_names.digest().as_bytes());
    digest.update(locale_information.digest().as_bytes());
    digest.update(locale_calendars::LOCALE_CALENDARS_KERNEL_SHA256);
    digest.update(locale_collations::LOCALE_COLLATIONS_KERNEL_SHA256);
    digest.update(locale_time_zones::LOCALE_TIME_ZONES_KERNEL_SHA256);
    digest.update(locale_hour_cycles::LOCALE_HOUR_CYCLES_KERNEL_SHA256);
    digest.update(locale_numbering_systems::LOCALE_NUMBERING_SYSTEMS_KERNEL_SHA256);
    digest.update(locale_text::LOCALE_TEXT_KERNEL_SHA256);
    digest.update(locale_week::LOCALE_WEEK_KERNEL_SHA256);
    digest.update(keyword_aliases::PROVIDER_DATA_SHA256);
    digest.update(named_time_zones::PROVIDER_DATA_SHA256);
    digest.update(time_zone_names::PROVIDER_DATA_SHA256);
    digest.update(datetime::PROVIDER_DATA_SHA256);
    digest.update(NUMBER_FORMAT_DATA_SHA256);
    digest.update(LIST_FORMAT_DATA_SHA256);
    digest.update(COLLATOR_DATA_SHA256);
    digest.update(DISPLAY_NAMES_KERNEL_SHA256);
    digest.update(RELATIVE_TIME_KERNEL_SHA256);
    digest.update(SEGMENTER_KERNEL_SHA256);
    digest.update(DURATION_FORMAT_DATA_SHA256);
    IntlDataDigest::from_sha256(digest.finalize().into())
}

/// Pure locale transforms, time-zone snapshots, and date/time, number and list partitions.
/// Every operation consumes the retained selected component image owners.
/// The finite pinned Minimal and named Custom closures have Embedded placement.
pub struct EmbeddedIntlProvider {
    identity: IntlDataIdentity,
    services: crate::CheckedIntlServiceSelection,
    locale_data: LocaleCanonicalizationData,
    locale_information: Option<NativeLocaleInformationDataImage>,
    named_time_zones: Option<Arc<NamedTimeZones>>,
    locale_time_zones: Option<Arc<locale_time_zones::LocaleTimeZoneProfiles>>,
    time_zone_names: Option<Arc<TimeZoneNames>>,
    date_time: Option<Arc<DateTimeProvider>>,
    numbers: Option<Arc<NumberProfiles>>,
    lists: Option<Arc<ListProfiles>>,
    collators: Option<Arc<CollatorProfiles>>,
    display_names: Option<DisplayNamesDataImage>,
    relative_times: Option<Arc<RelativeProfiles>>,
    segmenters: Option<Arc<SegmenterProfiles>>,
    durations: Option<Arc<DurationProfiles>>,
}

impl fmt::Debug for EmbeddedIntlProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddedIntlProvider")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl EmbeddedIntlProvider {
    pub(crate) fn list_profiles(&self) -> Option<&ListProfiles> {
        self.lists.as_deref()
    }
    pub(crate) fn collator_profiles(&self) -> Option<&CollatorProfiles> {
        self.collators.as_deref()
    }
    pub(crate) fn named_time_zone_identifiers(&self) -> Option<impl Iterator<Item = &str>> {
        self.named_time_zones
            .as_ref()
            .map(|zones| zones.identifiers())
    }
    pub(crate) fn number_profiles(&self) -> Option<&Arc<NumberProfiles>> {
        self.numbers.as_ref()
    }
    pub(crate) fn available_calendar_kernels(&self) -> Option<Vec<crate::DateTimeCalendar>> {
        self.date_time
            .as_ref()
            .map(|date_time| date_time.available_calendar_kernels())
    }
    pub(crate) fn available_numbering_system_kernels(&self) -> Option<Vec<Box<str>>> {
        self.date_time
            .as_ref()
            .map(|date_time| date_time.available_numbering_system_kernels())
    }
    pub(crate) fn relative_time_profiles(&self) -> Option<&RelativeProfiles> {
        self.relative_times.as_deref()
    }
    pub(crate) fn permits_service(&self, service: IntlService) -> bool {
        self.services.permits(service)
    }
    fn service_data<'a, T>(
        &self,
        service: IntlService,
        data: Option<&'a T>,
    ) -> Result<&'a T, crate::UnavailableIntlService> {
        if !self.services.permits(service) {
            return Err(crate::UnavailableIntlService(service));
        }
        data.ok_or(crate::UnavailableIntlService(service))
    }
    fn foundation_data<'a, T>(
        &self,
        service: IntlService,
        data: Option<&'a T>,
    ) -> Result<&'a T, crate::UnavailableIntlService> {
        data.ok_or(crate::UnavailableIntlService(service))
    }

    pub fn new() -> Result<Self, EmbeddedIntlProviderSetupError> {
        Self::with_data_images(
            embedded_locale_data_image().map_err(EmbeddedIntlProviderSetupError::DataImage)?,
            embedded_list_data_image().map_err(EmbeddedIntlProviderSetupError::DataImage)?,
            embedded_collator_data_image().map_err(EmbeddedIntlProviderSetupError::DataImage)?,
            embedded_number_profiles_data_image()
                .map_err(EmbeddedIntlProviderSetupError::DataImage)?,
            embedded_segmenter_data_image().map_err(EmbeddedIntlProviderSetupError::DataImage)?,
            embedded_display_names_data_image()
                .map_err(EmbeddedIntlProviderSetupError::DataImage)?,
            embedded_relative_time_data_image()
                .map_err(EmbeddedIntlProviderSetupError::DataImage)?,
            embedded_duration_data_image().map_err(EmbeddedIntlProviderSetupError::DataImage)?,
            embedded_named_time_zone_data_image()
                .map_err(EmbeddedIntlProviderSetupError::DataImage)?,
            embedded_date_time_data_image().map_err(EmbeddedIntlProviderSetupError::DataImage)?,
            embedded_time_zone_names_data_image()
                .map_err(EmbeddedIntlProviderSetupError::DataImage)?,
            embedded_native_locale_information_data_image()
                .map_err(EmbeddedIntlProviderSetupError::DataImage)?,
        )
    }

    /// Selects actual immutable consumers as one coherent component profile.
    /// Projected foundations bind the actual dependent public catalogues and
    /// retain the same native owners used by every operation consumer.
    pub fn with_data_images(
        locale: LocaleDataImage,
        lists: ListDataImage,
        collators: CollatorDataImage,
        numbers: NumberProfilesDataImage,
        segmenters: SegmenterDataImage,
        display_names: DisplayNamesDataImage,
        relative_times: RelativeTimeDataImage,
        durations: DurationDataImage,
        named_zones: NamedTimeZoneDataImage,
        date_time: DateTimeDataImage,
        time_zone_names: TimeZoneNamesDataImage,
        locale_information: NativeLocaleInformationDataImage,
    ) -> Result<Self, EmbeddedIntlProviderSetupError> {
        if locale.profile() != lists.profile()
            || locale.profile() != collators.profile()
            || lists.locale_digest() != locale.digest()
            || collators.locale_digest() != locale.digest()
            || locale.profile() != numbers.profile()
            || locale.profile() != segmenters.profile()
            || numbers.locale_digest() != locale.digest()
            || !numbers.uses_locale(&locale)
            || numbers
                .list_digest()
                .is_some_and(|digest| digest != lists.digest())
            || segmenters.locale_digest() != locale.digest()
            || locale.profile() != display_names.profile()
            || locale.profile() != relative_times.profile()
            || locale.profile() != durations.profile()
            || display_names.locale_digest() != locale.digest()
            || numbers.currency_codes() != display_names.currency_codes()
            || numbers.numbering_system_selection() != date_time.numbering_system_selection()
            || relative_times.locale_digest() != locale.digest()
            || durations.locale_digest() != locale.digest()
            || relative_times.number_digest() != numbers.digest()
            || durations.number_digest() != numbers.digest()
            || durations.list_digest() != lists.digest()
            || !relative_times.uses_number_profiles(&numbers.profiles())
            || !durations.uses_foundations(&numbers.profiles(), &lists.profiles())
            || numbers
                .required_relative_time_locales()
                .is_some_and(|required| {
                    !required.iter().map(|name| name.as_ref()).eq(relative_times
                        .profiles_ref()
                        .available_locales()
                        .iter()
                        .copied())
                })
            || numbers.required_duration_locales().is_some_and(|required| {
                !required.iter().map(|name| name.as_ref()).eq(durations
                    .profiles_ref()
                    .available_locales()
                    .map(|name| name.as_str()))
            })
            || locale.profile() != named_zones.profile()
            || locale.profile() != date_time.profile()
            || locale.profile() != time_zone_names.profile()
            || locale.profile() != locale_information.profile()
            || date_time.locale_digest() != locale.digest()
            || locale_information.locale_digest() != locale.digest()
            || date_time.named_time_zone_digest() != named_zones.digest()
            || time_zone_names.named_digest() != named_zones.digest()
            || date_time.named_time_zone_selection() != named_zones.named_time_zone_selection()
            || time_zone_names.named_time_zone_selection()
                != named_zones.named_time_zone_selection()
            || locale_information.date_time_digest() != date_time.digest()
            || !date_time.uses_locale(&locale)
            || !date_time.uses_named_time_zones(&named_zones.zones())
            || !time_zone_names.uses_named_zones(&named_zones.zones())
            || !locale_information.uses_foundations(&locale, &date_time.provider())
        {
            return Err(EmbeddedIntlProviderSetupError::DataImage(
                IntlDataImageError::consumer(
                    "selected Intl components have different profiles or actual data foundations",
                ),
            ));
        }
        let identity = selected_intl_data_identity(
            &locale,
            &lists,
            &collators,
            &numbers,
            &segmenters,
            &display_names,
            &relative_times,
            &durations,
            &named_zones,
            &date_time,
            &time_zone_names,
            &locale_information,
        )?;
        let locale_data = LocaleCanonicalizationData::from_image(&locale)
            .map_err(EmbeddedIntlProviderSetupError::ReservedLanguageData)?;
        let provider = Self {
            identity,
            services: crate::CheckedIntlServiceSelection::new(IntlServiceSet::ALL)
                .map_err(EmbeddedIntlProviderSetupError::EmptyProfile)?,
            locale_data,
            locale_information: Some(locale_information),
            display_names: Some(display_names),
            relative_times: Some(relative_times.profiles()),
            segmenters: Some(segmenters.profiles()),
            durations: Some(durations.profiles()),
            named_time_zones: Some(named_zones.zones()),
            locale_time_zones: Some(named_zones.country_profiles()),
            time_zone_names: Some(time_zone_names.names()),
            date_time: Some(date_time.provider()),
            numbers: Some(numbers.profiles()),
            lists: Some(lists.profiles()),
            collators: Some(collators.profiles()),
        };
        if matches!(
            provider.identity.profile().profile(),
            crate::IntlDataProfile::Conformance
        ) {
            // Validate the actual public global domains and dependent consumers,
            // not a declared capability bitset or a nominal profile label.
            crate::supported_values::SupportedValuesCatalogue::from_consumed_provider(&provider)
                .map_err(|error| {
                    EmbeddedIntlProviderSetupError::DataImage(IntlDataImageError::consumer(error))
                })?;
        }
        Ok(provider)
    }

    /// Admit exactly the frame closure derived by the checked service owner.
    /// Missing consumers remain absent; no embedded source is loaded here.
    pub fn with_selected_data_images(
        services: crate::CheckedIntlServiceSelection,
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
    ) -> Result<Self, EmbeddedIntlProviderSetupError> {
        use crate::IntlDataComponent as C;
        let bad = || {
            EmbeddedIntlProviderSetupError::DataImage(IntlDataImageError::consumer(
                "selected Intl services and actual component foundations differ",
            ))
        };
        let inventory = [
            (C::Locale, Some(locale.digest())),
            (C::List, lists.as_ref().map(ListDataImage::digest)),
            (
                C::Collator,
                collators.as_ref().map(CollatorDataImage::digest),
            ),
            (
                C::Number,
                numbers.as_ref().map(NumberProfilesDataImage::digest),
            ),
            (
                C::Segmenter,
                segmenters.as_ref().map(SegmenterDataImage::digest),
            ),
            (
                C::DisplayNames,
                display_names.as_ref().map(DisplayNamesDataImage::digest),
            ),
            (
                C::RelativeTime,
                relative_times.as_ref().map(RelativeTimeDataImage::digest),
            ),
            (
                C::Duration,
                durations.as_ref().map(DurationDataImage::digest),
            ),
            (
                C::NamedTimeZones,
                named_zones.as_ref().map(NamedTimeZoneDataImage::digest),
            ),
            (
                C::DateTime,
                date_time.as_ref().map(DateTimeDataImage::digest),
            ),
            (
                C::TimeZoneNames,
                time_zone_names.as_ref().map(TimeZoneNamesDataImage::digest),
            ),
            (
                C::LocaleInformation,
                locale_information
                    .as_ref()
                    .map(NativeLocaleInformationDataImage::digest),
            ),
        ];
        if inventory
            .iter()
            .any(|(component, digest)| services.contains_component(*component) != digest.is_some())
        {
            return Err(bad());
        }
        // The unchanged complete admission still owns all original full-image,
        // Conformance and nominal Custom behavior and identity bytes.
        if services.requested() == IntlServiceSet::ALL {
            return Self::with_data_images(
                locale,
                lists.ok_or_else(bad)?,
                collators.ok_or_else(bad)?,
                numbers.ok_or_else(bad)?,
                segmenters.ok_or_else(bad)?,
                display_names.ok_or_else(bad)?,
                relative_times.ok_or_else(bad)?,
                durations.ok_or_else(bad)?,
                named_zones.ok_or_else(bad)?,
                date_time.ok_or_else(bad)?,
                time_zone_names.ok_or_else(bad)?,
                locale_information.ok_or_else(bad)?,
            );
        }
        let crate::IntlDataProfile::Custom(id) = locale.profile() else {
            return Err(bad());
        };
        for (profile, foundation) in lists
            .as_ref()
            .map(|image| (image.profile(), image.locale_digest()))
            .into_iter()
            .chain(
                collators
                    .as_ref()
                    .map(|image| (image.profile(), image.locale_digest())),
            )
            .chain(
                numbers
                    .as_ref()
                    .map(|image| (image.profile(), image.locale_digest())),
            )
            .chain(
                segmenters
                    .as_ref()
                    .map(|image| (image.profile(), image.locale_digest())),
            )
            .chain(
                display_names
                    .as_ref()
                    .map(|image| (image.profile(), image.locale_digest())),
            )
            .chain(
                relative_times
                    .as_ref()
                    .map(|image| (image.profile(), image.locale_digest())),
            )
            .chain(
                durations
                    .as_ref()
                    .map(|image| (image.profile(), image.locale_digest())),
            )
            .chain(
                date_time
                    .as_ref()
                    .map(|image| (image.profile(), image.locale_digest())),
            )
            .chain(
                locale_information
                    .as_ref()
                    .map(|image| (image.profile(), image.locale_digest())),
            )
        {
            if profile != locale.profile() || foundation != locale.digest() {
                return Err(bad());
            }
        }
        if named_zones
            .as_ref()
            .is_some_and(|image| image.profile() != locale.profile())
            || time_zone_names
                .as_ref()
                .is_some_and(|image| image.profile() != locale.profile())
        {
            return Err(bad());
        }
        if let Some(numbers) = &numbers {
            let lists = lists.as_ref().ok_or_else(bad)?;
            if !numbers.uses_locale(&locale)
                || numbers
                    .list_digest()
                    .is_some_and(|digest| digest != lists.digest())
            {
                return Err(bad());
            }
            if let Some(display) = &display_names {
                if numbers.currency_codes() != display.currency_codes() {
                    return Err(bad());
                }
            }
            if let Some(date_time) = &date_time {
                if numbers.numbering_system_selection() != date_time.numbering_system_selection() {
                    return Err(bad());
                }
            }
        }
        if let Some(relative) = &relative_times {
            let numbers = numbers.as_ref().ok_or_else(bad)?;
            if relative.number_digest() != numbers.digest()
                || !relative.uses_number_profiles(&numbers.profiles())
                || numbers
                    .required_relative_time_locales()
                    .is_some_and(|required| {
                        !required.iter().map(|name| name.as_ref()).eq(relative
                            .profiles_ref()
                            .available_locales()
                            .iter()
                            .copied())
                    })
            {
                return Err(bad());
            }
        }
        if let Some(duration) = &durations {
            let numbers = numbers.as_ref().ok_or_else(bad)?;
            let lists = lists.as_ref().ok_or_else(bad)?;
            if duration.number_digest() != numbers.digest()
                || duration.list_digest() != lists.digest()
                || !duration.uses_foundations(&numbers.profiles(), &lists.profiles())
                || numbers.required_duration_locales().is_some_and(|required| {
                    !required.iter().map(|name| name.as_ref()).eq(duration
                        .profiles_ref()
                        .available_locales()
                        .map(|name| name.as_str()))
                })
            {
                return Err(bad());
            }
        }
        if let Some(date_time) = &date_time {
            let named = named_zones.as_ref().ok_or_else(bad)?;
            if date_time.named_time_zone_digest() != named.digest()
                || date_time.named_time_zone_selection() != named.named_time_zone_selection()
                || !date_time.uses_locale(&locale)
                || !date_time.uses_named_time_zones(&named.zones())
            {
                return Err(bad());
            }
        }
        if let Some(names) = &time_zone_names {
            let named = named_zones.as_ref().ok_or_else(bad)?;
            if names.named_digest() != named.digest()
                || names.named_time_zone_selection() != named.named_time_zone_selection()
                || !names.uses_named_zones(&named.zones())
            {
                return Err(bad());
            }
        }
        if let Some(information) = &locale_information {
            let date_time = date_time.as_ref().ok_or_else(bad)?;
            if information.date_time_digest() != date_time.digest()
                || !information.uses_foundations(&locale, &date_time.provider())
            {
                return Err(bad());
            }
        }
        let mut digest = Sha256::new();
        digest.update(b"scriptw-intl-selected-services-v1\0");
        digest.update(services.wire().to_le_bytes());
        for (component, frame) in inventory {
            if let Some(frame) = frame {
                digest.update(component.code().to_le_bytes());
                digest.update(frame.as_bytes());
            }
        }
        let mut profile = IntlProfilePlan::custom(id.clone(), services.requested())
            .map_err(EmbeddedIntlProviderSetupError::EmptyProfile)?;
        if named_zones.is_some() {
            profile = profile
                .with_operation::<LookupNamedTimeZone>()
                .with_operation::<NamedTimeZoneOffset>()
                .with_operation::<PossibleNamedTimeZoneEpochs>()
                .with_operation::<FindNamedTimeZoneTransition>();
        }
        if time_zone_names.is_some() {
            profile = profile.with_operation::<ResolveTimeZone>();
        }
        let identity = IntlDataIdentity::new(
            profile,
            CanonicalLocaleId::from_data("en-US")
                .map_err(EmbeddedIntlProviderSetupError::InvalidDefaultLocale)?,
            IntlDataPlacement::Embedded,
            IntlDataDigest::from_sha256(digest.finalize().into()),
        );
        let locale_data = LocaleCanonicalizationData::from_image(&locale)
            .map_err(EmbeddedIntlProviderSetupError::ReservedLanguageData)?;
        Ok(Self {
            identity,
            services,
            locale_data,
            locale_information,
            display_names,
            relative_times: relative_times.as_ref().map(RelativeTimeDataImage::profiles),
            segmenters: segmenters.as_ref().map(SegmenterDataImage::profiles),
            durations: durations.as_ref().map(DurationDataImage::profiles),
            named_time_zones: named_zones.as_ref().map(NamedTimeZoneDataImage::zones),
            locale_time_zones: named_zones
                .as_ref()
                .map(NamedTimeZoneDataImage::country_profiles),
            time_zone_names: time_zone_names.as_ref().map(TimeZoneNamesDataImage::names),
            date_time: date_time.as_ref().map(DateTimeDataImage::provider),
            numbers: numbers.as_ref().map(NumberProfilesDataImage::profiles),
            lists: lists.as_ref().map(ListDataImage::profiles),
            collators: collators.as_ref().map(CollatorDataImage::profiles),
        })
    }
}

/// Identity expected by artifacts that use the host-embedded Intl provider.
///
/// Includes every consumed immutable component identity in the finite pinned closure.
pub fn embedded_intl_data_identity() -> Result<IntlDataIdentity, EmbeddedIntlProviderSetupError> {
    selected_intl_data_identity(
        &embedded_locale_data_image().map_err(EmbeddedIntlProviderSetupError::DataImage)?,
        &embedded_list_data_image().map_err(EmbeddedIntlProviderSetupError::DataImage)?,
        &embedded_collator_data_image().map_err(EmbeddedIntlProviderSetupError::DataImage)?,
        &embedded_number_profiles_data_image()
            .map_err(EmbeddedIntlProviderSetupError::DataImage)?,
        &embedded_segmenter_data_image().map_err(EmbeddedIntlProviderSetupError::DataImage)?,
        &embedded_display_names_data_image().map_err(EmbeddedIntlProviderSetupError::DataImage)?,
        &embedded_relative_time_data_image().map_err(EmbeddedIntlProviderSetupError::DataImage)?,
        &embedded_duration_data_image().map_err(EmbeddedIntlProviderSetupError::DataImage)?,
        &embedded_named_time_zone_data_image()
            .map_err(EmbeddedIntlProviderSetupError::DataImage)?,
        &embedded_date_time_data_image().map_err(EmbeddedIntlProviderSetupError::DataImage)?,
        &embedded_time_zone_names_data_image()
            .map_err(EmbeddedIntlProviderSetupError::DataImage)?,
        &embedded_native_locale_information_data_image()
            .map_err(EmbeddedIntlProviderSetupError::DataImage)?,
    )
}

fn selected_intl_data_identity(
    locale: &LocaleDataImage,
    lists: &ListDataImage,
    collators: &CollatorDataImage,
    numbers: &NumberProfilesDataImage,
    segmenters: &SegmenterDataImage,
    display_names: &DisplayNamesDataImage,
    relative_times: &RelativeTimeDataImage,
    durations: &DurationDataImage,
    named_zones: &NamedTimeZoneDataImage,
    date_time: &DateTimeDataImage,
    time_zone_names: &TimeZoneNamesDataImage,
    locale_information: &NativeLocaleInformationDataImage,
) -> Result<IntlDataIdentity, EmbeddedIntlProviderSetupError> {
    let services = IntlServiceSet::EMPTY
        .with(IntlService::Locale)
        .with(IntlService::DateTimeFormat)
        .with(IntlService::NumberFormat)
        .with(IntlService::PluralRules)
        .with(IntlService::ListFormat)
        .with(IntlService::Collator)
        .with(IntlService::DisplayNames)
        .with(IntlService::RelativeTimeFormat)
        .with(IntlService::Segmenter)
        .with(IntlService::DurationFormat);
    let profile = match locale.profile() {
        crate::IntlDataProfile::Minimal => IntlProfilePlan::minimal(services),
        crate::IntlDataProfile::Custom(id) => IntlProfilePlan::custom(id.clone(), services),
        crate::IntlDataProfile::Conformance => Ok(conformance::CheckedConformanceData::new(
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
        )
        .map_err(EmbeddedIntlProviderSetupError::DataImage)?
        .into_plan()),
    }
    .map_err(EmbeddedIntlProviderSetupError::EmptyProfile)?
    .with_operation::<LookupNamedTimeZone>()
    .with_operation::<ResolveTimeZone>()
    .with_operation::<NamedTimeZoneOffset>()
    .with_operation::<PossibleNamedTimeZoneEpochs>()
    .with_operation::<FindNamedTimeZoneTransition>();
    let default_locale = CanonicalLocaleId::from_data("en-US")
        .map_err(EmbeddedIntlProviderSetupError::InvalidDefaultLocale)?;
    Ok(IntlDataIdentity::new(
        profile,
        default_locale,
        IntlDataPlacement::Embedded,
        selected_intl_data_digest(
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
        ),
    ))
}

impl IntlProvider for EmbeddedIntlProvider {
    fn identity(&self) -> &IntlDataIdentity {
        &self.identity
    }
}

impl IntlOperationProvider<CanonicalizeLocale> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: LocaleTransformRequest,
    ) -> Result<LocaleTransformResult, LocaleTransformError> {
        let canonical = self.locale_data.canonicalize(&request.into_locale())?;
        Ok(LocaleTransformResult::new(canonical))
    }
}

impl IntlOperationProvider<MaximizeLocale> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: LocaleTransformRequest,
    ) -> Result<LocaleTransformResult, LocaleTransformError> {
        let locale = self
            .locale_data
            .apply_likely_subtags(&request.into_locale(), LikelySubtags::Maximize)?;
        Ok(LocaleTransformResult::new(locale))
    }
}

impl IntlOperationProvider<MinimizeLocale> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: LocaleTransformRequest,
    ) -> Result<LocaleTransformResult, LocaleTransformError> {
        let locale = self
            .locale_data
            .apply_likely_subtags(&request.into_locale(), LikelySubtags::Minimize)?;
        Ok(LocaleTransformResult::new(locale))
    }
}

impl IntlOperationProvider<crate::LocaleCalendarsOperation> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: crate::LocaleCalendarsRequest,
    ) -> Result<crate::LocaleCalendars, crate::LocaleCalendarsError> {
        let locale_information =
            self.service_data(IntlService::Locale, self.locale_information.as_ref())?;
        locale_information.resolve_calendars(request)
    }
}
impl IntlOperationProvider<crate::LocaleCollationsOperation> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: crate::LocaleCollationsRequest,
    ) -> Result<crate::LocaleCollations, crate::LocaleCollationsError> {
        let collators = self.service_data(IntlService::Locale, self.collators.as_ref())?;
        locale_collations::resolve_locale_collations(request, collators)
    }
}
impl IntlOperationProvider<crate::LocaleTimeZonesOperation> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: crate::LocaleTimeZonesRequest,
    ) -> Result<crate::LocaleTimeZones, crate::LocaleTimeZonesError> {
        let locale_time_zones =
            self.service_data(IntlService::Locale, self.locale_time_zones.as_ref())?;
        locale_time_zones::resolve_locale_time_zones(request, locale_time_zones)
    }
}
impl IntlOperationProvider<crate::LocaleHourCyclesOperation> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: crate::LocaleHourCyclesRequest,
    ) -> Result<crate::LocaleHourCycles, crate::LocaleHourCyclesError> {
        let locale_information =
            self.service_data(IntlService::Locale, self.locale_information.as_ref())?;
        locale_information.resolve_hour_cycles(request)
    }
}

impl IntlOperationProvider<crate::LocaleNumberingSystemsOperation> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: crate::LocaleNumberingSystemsRequest,
    ) -> Result<crate::number_format::DecimalNumberingSystem, crate::LocaleNumberingSystemsError>
    {
        let numbers = self.service_data(IntlService::Locale, self.numbers.as_ref())?;
        locale_numbering_systems::resolve_locale_numbering_systems(request, numbers)
    }
}

impl IntlOperationProvider<crate::LocaleTextInfoOperation> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: crate::LocaleTextInfoRequest,
    ) -> Result<Option<crate::LocaleTextDirection>, crate::LocaleTextError> {
        let locale_information =
            self.service_data(IntlService::Locale, self.locale_information.as_ref())?;
        locale_information.resolve_text(request)
    }
}

impl IntlOperationProvider<crate::LocaleWeekInfoOperation> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: crate::LocaleWeekRequest,
    ) -> Result<crate::LocaleWeekInfo, crate::LocaleWeekError> {
        let locale_information =
            self.service_data(IntlService::Locale, self.locale_information.as_ref())?;
        locale_information.resolve_week(request)
    }
}

impl IntlOperationProvider<LookupNamedTimeZone> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: LookupNamedTimeZoneRequest,
    ) -> Result<LookupNamedTimeZoneResult, crate::NamedTimeZoneLookupError> {
        let named_time_zones =
            self.foundation_data(IntlService::DateTimeFormat, self.named_time_zones.as_ref())?;
        named_time_zones
            .lookup(request.identifier())
            .map(LookupNamedTimeZoneResult::new)
            .map_err(Into::into)
    }
}

impl IntlOperationProvider<NamedTimeZoneOffset> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: NamedTimeZoneOffsetRequest,
    ) -> Result<NamedTimeZoneOffsetSeconds, NamedTimeZoneDataError> {
        let named_time_zones =
            self.foundation_data(IntlService::DateTimeFormat, self.named_time_zones.as_ref())?;
        named_time_zones.exact_offset(&request)
    }
}
impl IntlOperationProvider<PossibleNamedTimeZoneEpochs> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: PossibleNamedTimeZoneEpochsRequest,
    ) -> Result<PossibleNamedTimeZoneEpochsResult, NamedTimeZoneDataError> {
        let named_time_zones =
            self.foundation_data(IntlService::DateTimeFormat, self.named_time_zones.as_ref())?;
        named_time_zones.possible_epochs(&request)
    }
}
impl IntlOperationProvider<FindNamedTimeZoneTransition> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: FindNamedTimeZoneTransitionRequest,
    ) -> Result<FindNamedTimeZoneTransitionResult, NamedTimeZoneDataError> {
        let named_time_zones =
            self.foundation_data(IntlService::DateTimeFormat, self.named_time_zones.as_ref())?;
        named_time_zones.find_transition(&request)
    }
}

impl IntlOperationProvider<ResolveTimeZone> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: ResolveTimeZoneRequest,
    ) -> Result<ResolvedTimeZoneSnapshot, TimeZoneResolveError> {
        let time_zone_names =
            self.service_data(IntlService::DateTimeFormat, self.time_zone_names.as_ref())?;
        time_zone_names.resolve(request)
    }
}

impl IntlOperationProvider<ResolveDateTimeLocale> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: DateTimeLocaleRequest,
    ) -> Result<DateTimeLocaleResult, DateTimeFormatError> {
        let date_time = self.service_data(IntlService::DateTimeFormat, self.date_time.as_ref())?;
        date_time.resolve_locale(request)
    }
}

impl IntlOperationProvider<SupportedDateTimeLocales> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: DateTimeSupportedLocalesRequest,
    ) -> Result<DateTimeSupportedLocalesResult, DateTimeFormatError> {
        let date_time = self.service_data(IntlService::DateTimeFormat, self.date_time.as_ref())?;
        date_time.supported_locales(request)
    }
}

impl IntlOperationProvider<SelectDateTimeFormat> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: DateTimePlanRequest,
    ) -> Result<DateTimePlanResult, DateTimeFormatError> {
        let date_time = self.service_data(IntlService::DateTimeFormat, self.date_time.as_ref())?;
        date_time.select_plan(request)
    }
}

impl IntlOperationProvider<FormatDateTimeParts> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: DateTimeFormatRequest,
    ) -> Result<DateTimeParts, DateTimeFormatError> {
        let date_time = self.service_data(IntlService::DateTimeFormat, self.date_time.as_ref())?;
        date_time.format_parts(request)
    }
}

impl IntlOperationProvider<FormatDateTimeRangeParts> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: DateTimeRangeRequest,
    ) -> Result<DateTimeRangeParts, DateTimeFormatError> {
        let date_time = self.service_data(IntlService::DateTimeFormat, self.date_time.as_ref())?;
        date_time.format_range_parts(request)
    }
}

impl IntlOperationProvider<ResolveNumberLocale> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: NumberLocaleRequest,
    ) -> Result<ResolvedNumberLocale, NumberFormatOperationError> {
        let numbers = self.service_data(IntlService::NumberFormat, self.numbers.as_ref())?;
        Ok(resolve_number_locale(&request, numbers)?)
    }
}

impl IntlOperationProvider<SupportedNumberLocales> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: NumberSupportedLocalesRequest,
    ) -> Result<NumberSupportedLocalesResult, NumberFormatOperationError> {
        let numbers = self.service_data(IntlService::NumberFormat, self.numbers.as_ref())?;
        Ok(NumberSupportedLocalesResult {
            locales: filter_number_locales(&request, numbers)?,
        })
    }
}

impl IntlOperationProvider<FormatNumberParts> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: NumberFormatRequest,
    ) -> Result<ScalarNumberPartition, NumberFormatOperationError> {
        let numbers = self.service_data(IntlService::NumberFormat, self.numbers.as_ref())?;
        format_number_parts_operation(request, numbers, &PartitionLimits::HOST_ABI)
    }
}

impl IntlOperationProvider<FormatNumberRangeParts> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: NumberRangeFormatRequest,
    ) -> Result<RangeNumberPartition, NumberFormatOperationError> {
        let numbers = self.service_data(IntlService::NumberFormat, self.numbers.as_ref())?;
        format_number_range_parts_operation(request, numbers, &PartitionLimits::HOST_ABI)
    }
}

#[derive(Debug)]
pub enum EmbeddedIntlProviderSetupError {
    DataImage(IntlDataImageError),
    EmptyProfile(EmptyIntlProfile),
    InvalidDefaultLocale(InvalidCanonicalLocaleId),
    ReservedLanguageData(&'static str),
    TimeZoneData(InvalidTimeZoneData),
    TimeZoneNameData(InvalidTimeZoneData),
    DateTimeData(DateTimeFormatError),
    NumberData(InvalidNumberProfile),
    ListData(ListFormatOperationError),
    CollatorData(CollatorOperationError),
    DisplayNamesData(DisplayNamesError),
    RelativeTimeData(RelativeTimeError),
    SegmenterData(SegmenterError),
    DurationData(DurationError),
    LocaleWeekData(crate::LocaleWeekError),
    LocaleHourCyclesData(crate::LocaleHourCyclesError),
    LocaleCalendarsData(crate::LocaleCalendarsError),
    LocaleTimeZonesData(crate::LocaleTimeZonesError),
    LocaleTextData(crate::LocaleTextError),
}

impl fmt::Display for EmbeddedIntlProviderSetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DataImage(error) => error.fmt(f),
            Self::EmptyProfile(error) => error.fmt(f),
            Self::InvalidDefaultLocale(error) => error.fmt(f),
            Self::TimeZoneData(error) => write!(f, "pinned IANA data is incompatible: {error}"),
            Self::TimeZoneNameData(error) => {
                write!(f, "pinned time-zone name data is incompatible: {error}")
            }
            Self::DateTimeData(error) => {
                write!(f, "pinned date/time data is incompatible: {error}")
            }
            Self::NumberData(error) => {
                write!(f, "pinned number data is incompatible: {error}")
            }
            Self::ListData(error) => write!(f, "pinned list data is incompatible: {error}"),
            Self::CollatorData(error) => {
                write!(f, "pinned collation data is incompatible: {error}")
            }
            Self::DisplayNamesData(error) => {
                write!(f, "pinned display-name data is incompatible: {error}")
            }
            Self::RelativeTimeData(error) => {
                write!(f, "pinned relative-time data is incompatible: {error}")
            }
            Self::DurationData(error) => {
                write!(f, "pinned DurationFormat data is incompatible: {error}")
            }
            Self::LocaleCalendarsData(error) => write!(f, "invalid Locale calendar data: {error}"),
            Self::LocaleTimeZonesData(error) => {
                write!(f, "invalid Locale time-zone region data: {error}")
            }
            Self::LocaleHourCyclesData(error) => {
                write!(f, "invalid Locale hour-cycle data: {error}")
            }
            Self::LocaleTextData(error) => {
                write!(f, "pinned Locale text data is incompatible: {error}")
            }
            Self::LocaleWeekData(error) => {
                write!(f, "pinned Locale week data is incompatible: {error}")
            }
            Self::SegmenterData(error) => {
                write!(f, "pinned Segmenter data is incompatible: {error}")
            }
            Self::ReservedLanguageData(reason) => write!(
                f,
                "pinned Intl reserved-language data is incompatible: {reason}"
            ),
        }
    }
}

impl std::error::Error for EmbeddedIntlProviderSetupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::DataImage(error) => Some(error),
            Self::EmptyProfile(error) => Some(error),
            Self::InvalidDefaultLocale(error) => Some(error),
            Self::ReservedLanguageData(_) => None,
            Self::TimeZoneData(error) | Self::TimeZoneNameData(error) => Some(error),
            Self::DateTimeData(error) => Some(error),
            Self::NumberData(error) => Some(error),
            Self::ListData(error) => Some(error),
            Self::CollatorData(error) => Some(error),
            Self::DisplayNamesData(error) => Some(error),
            Self::RelativeTimeData(error) => Some(error),
            Self::SegmenterData(error) => Some(error),
            Self::DurationData(error) => Some(error),
            Self::LocaleWeekData(error) => Some(error),
            Self::LocaleHourCyclesData(error) => Some(error),
            Self::LocaleCalendarsData(error) => Some(error),
            Self::LocaleTimeZonesData(error) => Some(error),
            Self::LocaleTextData(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IntlDataCapability, IntlKernel, LocaleId, LocaleTransformRequest};

    #[test]
    fn selected_components_reject_mixed_profile_or_locale_foundation_before_publication() {
        let custom = LocaleDataImage::for_profile(crate::IntlDataProfile::Custom(
            crate::CustomProfileId::parse("mixed-provider-control").unwrap(),
        ))
        .unwrap();
        let result = EmbeddedIntlProvider::with_data_images(
            custom,
            embedded_list_data_image().unwrap(),
            embedded_collator_data_image().unwrap(),
            embedded_number_profiles_data_image().unwrap(),
            embedded_segmenter_data_image().unwrap(),
            embedded_display_names_data_image().unwrap(),
            embedded_relative_time_data_image().unwrap(),
            embedded_duration_data_image().unwrap(),
            embedded_named_time_zone_data_image().unwrap(),
            embedded_date_time_data_image().unwrap(),
            embedded_time_zone_names_data_image().unwrap(),
            embedded_native_locale_information_data_image().unwrap(),
        );
        assert!(matches!(
            result,
            Err(EmbeddedIntlProviderSetupError::DataImage(_))
        ));
    }

    #[test]
    fn matching_component_digests_do_not_authorize_foreign_native_foundations() {
        let locale = embedded_locale_data_image().unwrap();
        let numbers = embedded_number_profiles_data_image().unwrap();
        let lists = embedded_list_data_image().unwrap();
        let foreign_numbers =
            NumberProfilesDataImage::from_bytes(numbers.bytes(), &locale, &lists).unwrap();
        let foreign_lists = ListDataImage::from_bytes(lists.bytes(), &locale).unwrap();
        assert_eq!(foreign_numbers.digest(), numbers.digest());
        assert_eq!(foreign_lists.digest(), lists.digest());
        let foreign_relative = RelativeTimeDataImage::for_profile(
            crate::IntlDataProfile::Minimal,
            &locale,
            &foreign_numbers,
        )
        .unwrap();
        let foreign_duration = DurationDataImage::for_profile(
            crate::IntlDataProfile::Minimal,
            &locale,
            &foreign_numbers,
            &lists,
        )
        .unwrap();
        for (selected_numbers, selected_lists, relative_times, durations) in [
            // Only Duration retains the foreign Number owner.
            (
                numbers.clone(),
                lists.clone(),
                embedded_relative_time_data_image().unwrap(),
                foreign_duration,
            ),
            // Only RelativeTime retains the foreign Number owner.
            (
                numbers.clone(),
                lists.clone(),
                foreign_relative,
                embedded_duration_data_image().unwrap(),
            ),
            // Only Duration retains a different List owner.
            (
                numbers,
                foreign_lists,
                embedded_relative_time_data_image().unwrap(),
                embedded_duration_data_image().unwrap(),
            ),
        ] {
            let result = EmbeddedIntlProvider::with_data_images(
                locale.clone(),
                selected_lists,
                embedded_collator_data_image().unwrap(),
                selected_numbers,
                embedded_segmenter_data_image().unwrap(),
                embedded_display_names_data_image().unwrap(),
                relative_times,
                durations,
                embedded_named_time_zone_data_image().unwrap(),
                embedded_date_time_data_image().unwrap(),
                embedded_time_zone_names_data_image().unwrap(),
                embedded_native_locale_information_data_image().unwrap(),
            );
            assert!(matches!(
                result,
                Err(EmbeddedIntlProviderSetupError::DataImage(_))
            ));
        }
    }

    #[test]
    fn matching_system_digests_do_not_authorize_foreign_selected_owners() {
        let locale = embedded_locale_data_image().unwrap();
        let named = embedded_named_time_zone_data_image().unwrap();
        let date_time = embedded_date_time_data_image().unwrap();
        let names = embedded_time_zone_names_data_image().unwrap();
        let information = embedded_native_locale_information_data_image().unwrap();
        let foreign_locale = LocaleDataImage::from_bytes(locale.bytes()).unwrap();
        let foreign_named = NamedTimeZoneDataImage::from_bytes(named.bytes()).unwrap();
        let foreign_date_time =
            DateTimeDataImage::from_bytes(date_time.bytes(), &locale, &named).unwrap();
        let foreign_names =
            TimeZoneNamesDataImage::from_bytes(names.bytes(), &foreign_named).unwrap();
        let foreign_information = NativeLocaleInformationDataImage::from_bytes(
            information.bytes(),
            &locale,
            &foreign_date_time,
        )
        .unwrap();
        assert_eq!(foreign_locale.digest(), locale.digest());
        assert_eq!(foreign_named.digest(), named.digest());
        assert_eq!(foreign_date_time.digest(), date_time.digest());
        assert_eq!(foreign_names.digest(), names.digest());
        assert_eq!(foreign_information.digest(), information.digest());
        for (locale, named, date_time, names, information) in [
            (
                foreign_locale,
                named.clone(),
                date_time.clone(),
                names.clone(),
                information.clone(),
            ),
            (
                locale.clone(),
                foreign_named,
                date_time.clone(),
                names.clone(),
                information.clone(),
            ),
            (
                locale.clone(),
                named.clone(),
                foreign_date_time,
                names.clone(),
                information.clone(),
            ),
            (
                locale.clone(),
                named.clone(),
                date_time.clone(),
                foreign_names,
                information.clone(),
            ),
            (locale, named, date_time, names, foreign_information),
        ] {
            let result = EmbeddedIntlProvider::with_data_images(
                locale,
                embedded_list_data_image().unwrap(),
                embedded_collator_data_image().unwrap(),
                embedded_number_profiles_data_image().unwrap(),
                embedded_segmenter_data_image().unwrap(),
                embedded_display_names_data_image().unwrap(),
                embedded_relative_time_data_image().unwrap(),
                embedded_duration_data_image().unwrap(),
                named,
                date_time,
                names,
                information,
            );
            assert!(matches!(
                result,
                Err(EmbeddedIntlProviderSetupError::DataImage(_))
            ));
        }
    }

    #[test]
    fn embedded_locale_provider_resolves_cldr_aliases() {
        let provider = EmbeddedIntlProvider::new().expect("embedded profile is valid");
        let identity = provider.identity().clone();
        assert_eq!(
            identity,
            embedded_intl_data_identity().expect("embedded profile identity is valid")
        );
        assert_eq!(identity.placement(), IntlDataPlacement::Embedded);
        assert_eq!(
            identity.digest(),
            selected_intl_data_digest(
                &embedded_locale_data_image().unwrap(),
                &embedded_list_data_image().unwrap(),
                &embedded_collator_data_image().unwrap(),
                &embedded_number_profiles_data_image().unwrap(),
                &embedded_segmenter_data_image().unwrap(),
                &embedded_display_names_data_image().unwrap(),
                &embedded_relative_time_data_image().unwrap(),
                &embedded_duration_data_image().unwrap(),
                &embedded_named_time_zone_data_image().unwrap(),
                &embedded_date_time_data_image().unwrap(),
                &embedded_time_zone_names_data_image().unwrap(),
                &embedded_native_locale_information_data_image().unwrap(),
            )
        );
        assert!(identity
            .profile()
            .capabilities()
            .contains(IntlDataCapability::LocaleAliases));

        let kernel = IntlKernel::new(identity, provider).expect("provider identity matches");
        let result = kernel
            .operation::<CanonicalizeLocale>()
            .expect("locale capability is present")
            .execute(LocaleTransformRequest::new(
                LocaleId::parse("iw-IL").expect("structurally valid locale"),
            ))
            .expect("pinned ICU4X data contains the alias");

        assert_eq!(result.locale().as_str(), "he-IL");
    }
}

impl IntlOperationProvider<ResolvePluralLocale> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: PluralLocaleRequest,
    ) -> Result<ResolvedPluralLocale, PluralRulesOperationError> {
        let numbers = self.service_data(IntlService::PluralRules, self.numbers.as_ref())?;
        resolve_plural_locale(&request, numbers, &PartitionLimits::HOST_ABI)
    }
}
impl IntlOperationProvider<SupportedPluralLocales> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: PluralSupportedLocalesRequest,
    ) -> Result<PluralSupportedLocalesResult, PluralRulesOperationError> {
        let numbers = self.service_data(IntlService::PluralRules, self.numbers.as_ref())?;
        supported_plural_locales(request, numbers)
    }
}
impl IntlOperationProvider<SelectPlural> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: SelectPluralRequest,
    ) -> Result<PluralCategory, PluralRulesOperationError> {
        let numbers = self.service_data(IntlService::PluralRules, self.numbers.as_ref())?;
        select_plural_operation(
            request,
            numbers,
            &crate::number_format::numeric::NumericLimits::HOST_ABI,
        )
    }
}
impl IntlOperationProvider<SelectPluralRange> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: SelectPluralRangeRequest,
    ) -> Result<PluralCategory, PluralRulesOperationError> {
        let numbers = self.service_data(IntlService::PluralRules, self.numbers.as_ref())?;
        select_plural_range_operation(
            request,
            numbers,
            &crate::number_format::numeric::NumericLimits::HOST_ABI,
        )
    }
}

impl IntlOperationProvider<ResolveListLocale> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: ListLocaleRequest,
    ) -> Result<ResolvedListLocale, ListFormatOperationError> {
        let lists = self.service_data(IntlService::ListFormat, self.lists.as_ref())?;
        lists.resolve(request)
    }
}
impl IntlOperationProvider<SupportedListLocales> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: ListSupportedLocalesRequest,
    ) -> Result<ListSupportedLocalesResult, ListFormatOperationError> {
        let lists = self.service_data(IntlService::ListFormat, self.lists.as_ref())?;
        lists.supported(request)
    }
}
impl IntlOperationProvider<FormatListParts> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: FormatListPartsRequest,
    ) -> Result<ListParts, ListFormatOperationError> {
        let lists = self.service_data(IntlService::ListFormat, self.lists.as_ref())?;
        lists.format_parts(request)
    }
}

/// Reuse the sole pinned CLDR complete-value alias authority after option syntax validation.
pub(crate) fn canonical_collator_keyword(
    value: &str,
    aliases: &KeywordAliasData,
) -> Result<Box<str>, CollatorOperationError> {
    let source = format!("und-u-co-{value}");
    let mut locale: icu_locale::Locale = source.parse().map_err(|error| {
        CollatorOperationError::Data(
            format!("invalid syntax-valid collation keyword: {error}").into_boxed_str(),
        )
    })?;
    locale.extensions.unicode.keywords = keyword_aliases::lossless_unicode_keywords(&source);
    aliases.canonicalize_unicode_keywords(&mut locale);
    let value = locale
        .extensions
        .unicode
        .keywords
        .get(&icu_locale::extensions::unicode::key!("co"))
        .ok_or(CollatorOperationError::InvalidConfiguration)?;
    Ok(value.to_string().into_boxed_str())
}
impl IntlOperationProvider<ResolveCollatorLocale> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: CollatorLocaleRequest,
    ) -> Result<ResolvedCollatorLocale, CollatorOperationError> {
        let collators = self.service_data(IntlService::Collator, self.collators.as_ref())?;
        collators.resolve(request)
    }
}
impl IntlOperationProvider<SupportedCollatorLocales> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: CollatorSupportedLocalesRequest,
    ) -> Result<CollatorSupportedLocalesResult, CollatorOperationError> {
        let collators = self.service_data(IntlService::Collator, self.collators.as_ref())?;
        collators.supported(request)
    }
}
impl IntlOperationProvider<CompareCollator> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: CompareCollatorRequest,
    ) -> Result<CollatorOrdering, CollatorOperationError> {
        let collators = self.service_data(IntlService::Collator, self.collators.as_ref())?;
        collators.compare(request)
    }
}

/// Preserve complete Unicode keyword values in the already-validated locale.
pub(crate) fn collator_locale_keywords(source: &str) -> icu_locale::extensions::unicode::Keywords {
    keyword_aliases::lossless_unicode_keywords(source)
}

impl EmbeddedIntlProvider {
    pub(crate) fn display_names_profiles(&self) -> Option<&DisplayNamesProfiles> {
        self.display_names
            .as_ref()
            .map(DisplayNamesDataImage::profiles_ref)
    }
}
impl IntlOperationProvider<ResolveDisplayNamesLocale> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: DisplayNamesLocaleRequest,
    ) -> Result<ResolvedDisplayNamesLocale, DisplayNamesError> {
        let display_names =
            self.service_data(IntlService::DisplayNames, self.display_names.as_ref())?;
        display_names.profiles_ref().resolve(&request)
    }
}
impl IntlOperationProvider<SupportedDisplayNamesLocales> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: DisplayNamesLocaleRequest,
    ) -> Result<DisplayNamesSupportedLocalesResult, DisplayNamesError> {
        let display_names =
            self.service_data(IntlService::DisplayNames, self.display_names.as_ref())?;
        Ok(display_names.profiles_ref().supported_locales(&request))
    }
}
impl IntlOperationProvider<DisplayName> for EmbeddedIntlProvider {
    fn execute(&self, request: DisplayNameRequest) -> Result<DisplayNameResult, DisplayNamesError> {
        let display_names =
            self.service_data(IntlService::DisplayNames, self.display_names.as_ref())?;
        display_names.display_name(&request)
    }
}
impl IntlOperationProvider<ResolveRelativeTimeLocale> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: NumberLocaleRequest,
    ) -> Result<ResolvedRelativeTimeLocale, RelativeTimeError> {
        let relative_times = self.service_data(
            IntlService::RelativeTimeFormat,
            self.relative_times.as_ref(),
        )?;
        let numbers =
            self.foundation_data(IntlService::RelativeTimeFormat, self.numbers.as_ref())?;
        relative_times.resolve_locale(&request, numbers, &PartitionLimits::default())
    }
}
impl IntlOperationProvider<SupportedRelativeTimeLocales> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: NumberSupportedLocalesRequest,
    ) -> Result<NumberSupportedLocalesResult, RelativeTimeError> {
        let relative_times = self.service_data(
            IntlService::RelativeTimeFormat,
            self.relative_times.as_ref(),
        )?;
        Ok(NumberSupportedLocalesResult {
            locales: relative_times.supported_locales(
                &request.requested,
                request.matcher,
                &PartitionLimits::default(),
            )?,
        })
    }
}
impl IntlOperationProvider<FormatRelativeTimeParts> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: FormatRelativeTimePartsRequest,
    ) -> Result<RelativePartition, RelativeTimeError> {
        let relative_times = self.service_data(
            IntlService::RelativeTimeFormat,
            self.relative_times.as_ref(),
        )?;
        relative_times.format_parts(
            request.configuration(),
            request.value(),
            request.unit(),
            &PartitionLimits::default(),
        )
    }
}

#[cfg(test)]
mod display_relative_tests {
    use super::*;
    use crate::number_format::options::LocaleMatcher;
    use crate::{
        CheckedDisplayNamesConfiguration, DisplayNamesFallback, DisplayNamesSelection,
        DisplayNamesStyle, FiniteRelativeNumber, RelativeNumeric, RelativeStyle,
        RelativeTimeConfiguration, RelativeUnit,
    };

    #[test]
    fn checked_provider_executes_all_six_new_operations_with_real_profiles() {
        let provider = EmbeddedIntlProvider::new().unwrap();
        let number_profiles = Arc::clone(provider.number_profiles().unwrap());
        let kernel = crate::IntlKernel::new(provider.identity().clone(), provider).unwrap();
        let request = DisplayNamesLocaleRequest {
            requested: vec![CanonicalLocaleId::from_data("en-US").unwrap()].into_boxed_slice(),
            matcher: LocaleMatcher::Lookup,
        };
        let selected = kernel
            .operation::<ResolveDisplayNamesLocale>()
            .unwrap()
            .execute(request.clone())
            .unwrap();
        let supported = kernel
            .operation::<SupportedDisplayNamesLocales>()
            .unwrap()
            .execute(request)
            .unwrap();
        assert_eq!(supported.locales[0], *selected.resolved());
        let configuration = CheckedDisplayNamesConfiguration::new(
            selected,
            DisplayNamesSelection::Currency,
            DisplayNamesStyle::Long,
            DisplayNamesFallback::None,
        );
        let request =
            DisplayNameRequest::new(configuration, "USD".encode_utf16().collect()).unwrap();
        assert_eq!(
            kernel
                .operation::<DisplayName>()
                .unwrap()
                .execute(request)
                .unwrap()
                .name(),
            Some("US Dollar")
        );
        let request = NumberLocaleRequest {
            requested: vec![CanonicalLocaleId::from_data("en-US").unwrap()].into_boxed_slice(),
            matcher: LocaleMatcher::Lookup,
            numbering_system: None,
        };
        let selected = kernel
            .operation::<ResolveRelativeTimeLocale>()
            .unwrap()
            .execute(request.clone())
            .unwrap();
        let supported = kernel
            .operation::<SupportedRelativeTimeLocales>()
            .unwrap()
            .execute(NumberSupportedLocalesRequest {
                requested: request.requested,
                matcher: request.matcher,
            })
            .unwrap();
        assert_eq!(supported.locales[0], *selected.resolved());
        let configuration = RelativeTimeConfiguration::new(
            selected,
            RelativeStyle::Long,
            RelativeNumeric::Always,
            &number_profiles,
        )
        .unwrap();
        let request = FormatRelativeTimePartsRequest::new(
            configuration,
            FiniteRelativeNumber::new(-0.0).unwrap(),
            RelativeUnit::Day,
        );
        assert_eq!(
            kernel
                .operation::<FormatRelativeTimeParts>()
                .unwrap()
                .execute(request)
                .unwrap()
                .to_text()
                .unwrap(),
            "0 days ago"
        );
    }

    #[test]
    fn identity_matched_kernel_decodes_lossless_display_name_using_admitted_profile() {
        let provider = EmbeddedIntlProvider::new().unwrap();
        let kernel = crate::IntlKernel::new(provider.identity().clone(), provider).unwrap();
        let selected = kernel
            .operation::<ResolveDisplayNamesLocale>()
            .unwrap()
            .execute(DisplayNamesLocaleRequest {
                requested: vec![CanonicalLocaleId::from_data("en-US").unwrap()].into_boxed_slice(),
                matcher: LocaleMatcher::Lookup,
            })
            .unwrap();
        let configuration = CheckedDisplayNamesConfiguration::new(
            selected,
            DisplayNamesSelection::Region,
            DisplayNamesStyle::Long,
            DisplayNamesFallback::None,
        );
        let request =
            DisplayNameRequest::new(configuration, vec![0xd800].into_boxed_slice()).unwrap();
        let bytes = crate::encode_display_name_request(&request).unwrap();
        let decoded = kernel.decode_display_name_request(&bytes).unwrap();
        assert_eq!(decoded.code(), &[0xd800]);
        assert_eq!(
            kernel.operation::<DisplayName>().unwrap().execute(decoded),
            Err(DisplayNamesError::InvalidCode)
        );
        let mut trailing = bytes;
        trailing.push(0);
        assert!(kernel.decode_display_name_request(&trailing).is_err());
    }
}

impl EmbeddedIntlProvider {
    pub(crate) fn segmenter_profiles(&self) -> Option<&SegmenterProfiles> {
        self.segmenters.as_deref()
    }
}
impl IntlOperationProvider<ResolveSegmenterLocale> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: SegmenterLocaleRequest,
    ) -> Result<ResolvedSegmenterLocale, SegmenterError> {
        let segmenters = self.service_data(IntlService::Segmenter, self.segmenters.as_ref())?;
        segmenters.resolve(&request)
    }
}
impl IntlOperationProvider<SupportedSegmenterLocales> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: SegmenterLocaleRequest,
    ) -> Result<SegmenterSupportedLocalesResult, SegmenterError> {
        let segmenters = self.service_data(IntlService::Segmenter, self.segmenters.as_ref())?;
        Ok(segmenters.supported_locales(&request))
    }
}
impl IntlOperationProvider<SegmentUtf16> for EmbeddedIntlProvider {
    fn execute(&self, request: SegmentUtf16Request) -> Result<SegmenterResult, SegmenterError> {
        let segmenters = self.service_data(IntlService::Segmenter, self.segmenters.as_ref())?;
        segmenters.segment(&request)
    }
}

#[cfg(test)]
mod segmenter_tests {
    use super::*;
    use crate::number_format::options::LocaleMatcher;
    use crate::{CheckedSegmenterConfiguration, IntlKernel, SegmenterGranularity};
    #[test]
    fn certified_shared_provider_executes_all_segmenter_operations_and_preserves_source_units() {
        let provider = EmbeddedIntlProvider::new().unwrap();
        let kernel = IntlKernel::new(provider.identity().clone(), provider).unwrap();
        assert!(kernel
            .identity()
            .profile()
            .services()
            .contains(IntlService::Segmenter));
        let requested = SegmenterLocaleRequest {
            requested: vec![CanonicalLocaleId::from_data("sv").unwrap()].into_boxed_slice(),
            matcher: LocaleMatcher::Lookup,
        };
        let locale = kernel
            .operation::<ResolveSegmenterLocale>()
            .unwrap()
            .execute(requested.clone())
            .unwrap();
        assert_eq!(locale.resolved().as_str(), "sv");
        assert_eq!(
            kernel
                .operation::<SupportedSegmenterLocales>()
                .unwrap()
                .execute(requested)
                .unwrap()
                .locales[0]
                .as_str(),
            "sv"
        );
        let input: Box<[u16]> = vec![0xd800, 0x61, 0xd83d, 0xde00].into_boxed_slice();
        let request = SegmentUtf16Request::new(
            CheckedSegmenterConfiguration::new(locale, SegmenterGranularity::Grapheme),
            input.clone(),
        )
        .unwrap();
        let bytes = crate::encode_segment_utf16_request(&request).unwrap();
        let checked = kernel.decode_segment_utf16_request(&bytes).unwrap();
        let result = kernel
            .operation::<SegmentUtf16>()
            .unwrap()
            .execute(checked)
            .unwrap();
        assert_eq!(result.input(), &*input);
        assert_eq!(
            result
                .boundaries()
                .iter()
                .map(|row| row.end())
                .collect::<Vec<_>>(),
            [1, 2, 4]
        );
        assert!(result
            .boundaries()
            .iter()
            .all(|row| row.is_word_like().is_none()));
        let mut malformed = bytes;
        malformed.push(0);
        assert!(kernel.decode_segment_utf16_request(&malformed).is_err());
        assert_eq!(crate::IntlHostOp::ALL.len(), 46);
    }
}

impl EmbeddedIntlProvider {
    pub(crate) fn duration_profiles(&self) -> Option<&DurationProfiles> {
        self.durations.as_deref()
    }
    pub(crate) fn duration_number_profiles(&self) -> Option<&Arc<NumberProfiles>> {
        self.numbers.as_ref()
    }
}
impl IntlOperationProvider<ResolveDurationFormatLocale> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: NumberLocaleRequest,
    ) -> Result<ResolvedDurationLocale, DurationError> {
        let durations = self.service_data(IntlService::DurationFormat, self.durations.as_ref())?;
        let numbers = self.foundation_data(IntlService::DurationFormat, self.numbers.as_ref())?;
        durations.resolve_locale(&request, numbers)
    }
}
impl IntlOperationProvider<SupportedDurationFormatLocales> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: DurationSupportedLocalesRequest,
    ) -> Result<Box<[CanonicalLocaleId]>, DurationError> {
        let durations = self.service_data(IntlService::DurationFormat, self.durations.as_ref())?;
        Ok(durations.supported_locales(request))
    }
}
impl IntlOperationProvider<PartitionDurationFormat> for EmbeddedIntlProvider {
    fn execute(
        &self,
        request: DurationPartitionRequest,
    ) -> Result<DurationPartition, DurationError> {
        let durations = self.service_data(IntlService::DurationFormat, self.durations.as_ref())?;
        format_duration_parts(
            request.configuration(),
            request.record(),
            durations,
            &PartitionLimits::HOST_ABI,
        )
    }
}
#[cfg(test)]
mod duration_tests {
    use super::*;
    use crate::number_format::options::LocaleMatcher;
    use crate::{
        CheckedDurationConfiguration, DurationOptions, DurationRecord, DurationStyle,
        DurationWireRequest, IntlKernel,
    };
    #[test]
    fn duration_provider_installs_all_fifteen_checked_locale_and_real_partition_owners() {
        let provider = EmbeddedIntlProvider::new().unwrap();
        assert!(provider
            .identity()
            .profile()
            .services()
            .contains(IntlService::DurationFormat));
        assert_eq!(
            provider
                .duration_profiles()
                .unwrap()
                .available_locales()
                .len(),
            15
        );
        for locale in provider.duration_profiles().unwrap().available_locales() {
            let request = NumberLocaleRequest {
                requested: vec![locale.clone()].into_boxed_slice(),
                matcher: LocaleMatcher::Lookup,
                numbering_system: None,
            };
            let selected = <EmbeddedIntlProvider as IntlOperationProvider<
                ResolveDurationFormatLocale,
            >>::execute(&provider, request)
            .unwrap();
            assert_eq!(selected.resolved(), locale);
            let supported = <EmbeddedIntlProvider as IntlOperationProvider<
                SupportedDurationFormatLocales,
            >>::execute(
                &provider,
                DurationSupportedLocalesRequest {
                    requested: vec![locale.clone()].into_boxed_slice(),
                    matcher: LocaleMatcher::BestFit,
                },
            )
            .unwrap();
            assert_eq!(supported.as_ref(), [locale.clone()]);
            let checked = CheckedDurationConfiguration::new(
                selected,
                DurationOptions {
                    style: DurationStyle::Digital,
                    ..Default::default()
                },
            )
            .unwrap();
            let request = DurationPartitionRequest::from_completed_number_fields(
                checked,
                [0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 0.0, 0.0, 0.0],
            )
            .unwrap();
            let parts =
                <EmbeddedIntlProvider as IntlOperationProvider<PartitionDurationFormat>>::execute(
                    &provider, request,
                )
                .unwrap();
            assert!(!parts.parts().is_empty());
            assert!(parts
                .parts()
                .iter()
                .any(|part| part.unit() == Some(crate::DurationUnit::Second)));
        }
    }
    #[test]
    fn duration_installed_kernel_decodes_with_its_owner_and_rejects_mixed_sign_before_execution() {
        let provider = EmbeddedIntlProvider::new().unwrap();
        let locale =
            <EmbeddedIntlProvider as IntlOperationProvider<ResolveDurationFormatLocale>>::execute(
                &provider,
                NumberLocaleRequest {
                    requested: vec![CanonicalLocaleId::from_data("en").unwrap()].into_boxed_slice(),
                    matcher: LocaleMatcher::Lookup,
                    numbering_system: None,
                },
            )
            .unwrap();
        let request = DurationWireRequest::Parts(
            DurationPartitionRequest::from_completed_number_fields(
                CheckedDurationConfiguration::new(locale, DurationOptions::default()).unwrap(),
                [0.0; 10],
            )
            .unwrap(),
        );
        let kernel = IntlKernel::new(provider.identity().clone(), provider).unwrap();
        let mut bytes = crate::encode_duration_request(&request).unwrap();
        assert!(kernel
            .decode_duration_request(crate::DurationHostOp::Parts, &bytes)
            .is_ok());
        let end = bytes.len();
        bytes[end - 80..end - 72].copy_from_slice(&1.0_f64.to_bits().to_le_bytes());
        bytes[end - 72..end - 64].copy_from_slice(&(-1.0_f64).to_bits().to_le_bytes());
        assert!(matches!(
            kernel.decode_duration_request(crate::DurationHostOp::Parts, &bytes),
            Err(crate::DurationWireError::Rejected(
                DurationError::InvalidRecord
            ))
        ));
        assert!(DurationRecord::from_number_fields([-0.0; 10]).is_ok());
    }
}
