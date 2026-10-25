use crate::collator::{
    CollatorLocaleRequest, CollatorOperationError, CollatorOrdering,
    CollatorSupportedLocalesRequest, CollatorSupportedLocalesResult, CompareCollatorRequest,
    ResolvedCollatorLocale,
};
use crate::display_names::{
    DisplayNameRequest, DisplayNameResult, DisplayNamesError, DisplayNamesLocaleRequest,
    DisplayNamesSupportedLocalesResult, ResolvedDisplayNamesLocale,
};
use crate::duration_format::{
    DurationError, DurationPartition, DurationSupportedLocalesRequest, ResolvedDurationLocale,
};
use crate::duration_wire::DurationPartitionRequest;
use crate::list_format::{
    FormatListPartsRequest, ListFormatOperationError, ListLocaleRequest, ListParts,
    ListSupportedLocalesRequest, ListSupportedLocalesResult, ResolvedListLocale,
};
use crate::number_format::DecimalNumberingSystem;
use crate::plural_rules::{
    PluralCategory, PluralLocaleRequest, PluralRulesOperationError, PluralSupportedLocalesRequest,
    PluralSupportedLocalesResult, ResolvedPluralLocale, SelectPluralRangeRequest,
    SelectPluralRequest,
};
use crate::relative_time_format::{
    FormatRelativeTimePartsRequest, RelativePartition, RelativeTimeError,
    ResolvedRelativeTimeLocale,
};
use crate::segmenter::{
    ResolvedSegmenterLocale, SegmentUtf16Request, SegmenterError, SegmenterLocaleRequest,
    SegmenterResult, SegmenterSupportedLocalesResult,
};
use crate::{
    LocaleCalendars, LocaleCalendarsError, LocaleCalendarsRequest, LocaleCollations,
    LocaleCollationsError, LocaleCollationsRequest, LocaleTimeZones, LocaleTimeZonesError,
    LocaleTimeZonesRequest,
};
use crate::{LocaleHourCycles, LocaleHourCyclesError, LocaleHourCyclesRequest};
use crate::{LocaleNumberingSystemsError, LocaleNumberingSystemsRequest};
use crate::{LocaleTextDirection, LocaleTextError, LocaleTextInfoRequest};
use crate::{LocaleWeekError, LocaleWeekInfo, LocaleWeekRequest};
use core::{fmt, marker::PhantomData};

mod gc_host_request;
pub use gc_host_request::{
    IntlGcHostRequest, IntlGcHostRequestError, INTL_GC_REQUEST_PREFIX_BYTES,
};

use crate::{
    FindNamedTimeZoneTransitionRequest, FindNamedTimeZoneTransitionResult, NamedTimeZoneDataError,
    NamedTimeZoneOffsetRequest, NamedTimeZoneOffsetSeconds, PossibleNamedTimeZoneEpochsRequest,
    PossibleNamedTimeZoneEpochsResult,
};

use crate::number_format::{
    NumberLocaleRequest, NumberSupportedLocalesRequest, RangeNumberPartition, ResolvedNumberLocale,
    ScalarNumberPartition,
};
use crate::{
    NumberFormatOperationError, NumberFormatRequest, NumberRangeFormatRequest,
    NumberSupportedLocalesResult,
};

use crate::{
    CanonicalLocaleId, DateTimeFormatError, DateTimeFormatRequest, DateTimeLocaleRequest,
    DateTimeLocaleResult, DateTimeParts, DateTimePlanRequest, DateTimePlanResult,
    DateTimeRangeParts, DateTimeRangeRequest, DateTimeSupportedLocalesRequest,
    DateTimeSupportedLocalesResult, IntlCapabilitySet, IntlDataCapability, IntlDataIdentity,
    InvalidCanonicalLocaleId, LocaleId, LookupNamedTimeZoneRequest, LookupNamedTimeZoneResult,
    ResolveTimeZoneRequest, ResolvedTimeZoneSnapshot, TimeZoneId, TimeZoneResolveError,
};

/// Packed offset/length span read by an Intl host operation.
///
/// Both halves are unsigned 32-bit values and every `i64` bit pattern therefore
/// decodes to exactly one span. Keeping read spans distinct from write spans
/// prevents a request length from being mistaken for output capacity in host
/// bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IntlHostReadSpan(u64);

impl IntlHostReadSpan {
    #[must_use]
    pub const fn new(offset: u32, length: u32) -> Self {
        Self(((offset as u64) << 32) | length as u64)
    }

    #[must_use]
    pub const fn from_wire(wire: i64) -> Self {
        Self(wire as u64)
    }

    #[must_use]
    pub const fn wire(self) -> i64 {
        self.0 as i64
    }

    #[must_use]
    pub const fn offset(self) -> u32 {
        (self.0 >> 32) as u32
    }

    #[must_use]
    pub const fn length(self) -> u32 {
        self.0 as u32
    }
}

/// Packed offset/capacity span written by an Intl host operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IntlHostWriteSpan(u64);

impl IntlHostWriteSpan {
    #[must_use]
    pub const fn new(offset: u32, capacity: u32) -> Self {
        Self(((offset as u64) << 32) | capacity as u64)
    }

    #[must_use]
    pub const fn from_wire(wire: i64) -> Self {
        Self(wire as u64)
    }

    #[must_use]
    pub const fn wire(self) -> i64 {
        self.0 as i64
    }

    #[must_use]
    pub const fn offset(self) -> u32 {
        (self.0 >> 32) as u32
    }

    #[must_use]
    pub const fn capacity(self) -> u32 {
        self.0 as u32
    }
}

/// Closed result domain for the shared `(op, request_span, result_span) -> i64`
/// host ABI.
///
/// Non-negative `u32` values are successful byte counts. `-1` rejects the
/// request; `-2 - required_capacity` requests a larger output span without
/// writing any bytes. Every other `i64` is an ABI fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntlHostCallOutcome {
    Written(u32),
    RequiredCapacity(u32),
    Rejected,
}

impl IntlHostCallOutcome {
    pub const REJECTED_WIRE: i64 = -1;

    #[must_use]
    pub const fn wire(self) -> i64 {
        match self {
            Self::Written(length) => length as i64,
            Self::RequiredCapacity(capacity) => -2 - capacity as i64,
            Self::Rejected => Self::REJECTED_WIRE,
        }
    }

    #[must_use]
    pub const fn from_wire(wire: i64) -> Option<Self> {
        if wire == Self::REJECTED_WIRE {
            Some(Self::Rejected)
        } else if wire <= -2 && wire >= -2 - u32::MAX as i64 {
            Some(Self::RequiredCapacity((-2 - wire) as u32))
        } else if wire >= 0 && wire <= u32::MAX as i64 {
            Some(Self::Written(wire as u32))
        } else {
            None
        }
    }
}

macro_rules! intl_operations {
    (
        $(
            $operation:ident {
                code: $code:literal,
                name: $name:literal,
                request: $request:ty,
                response: $response:ty,
                error: $error:ty,
                capabilities: [$($capability:path),* $(,)?],
            }
        )+
    ) => {
        /// Stable operation tags for the provider-independent Intl kernel
        /// boundary. The same row also generates the only operation marker
        /// that can carry each request/result/error association.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u16)]
        pub enum IntlHostOp {
            $($operation = $code),+
        }

        impl IntlHostOp {
            pub const ALL: &'static [Self] = &[$(Self::$operation),+];

            #[must_use]
            pub const fn code(self) -> u16 {
                self as u16
            }

            #[must_use]
            pub const fn wire(self) -> i64 {
                self as i64
            }

            #[must_use]
            pub const fn from_wire(wire: i64) -> Option<Self> {
                match wire {
                    $($code => Some(Self::$operation),)+
                    _ => None,
                }
            }

            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$operation => $name),+
                }
            }

            #[must_use]
            pub const fn required_capabilities(self) -> IntlCapabilitySet {
                match self {
                    $(
                        Self::$operation => IntlCapabilitySet::EMPTY
                            $(.with($capability))*,
                    )+
                }
            }
        }

        $(
            #[derive(Debug)]
            pub enum $operation {}

            impl sealed::Sealed for $operation {}

            impl IntlOperation for $operation {
                type Request = $request;
                type Response = $response;
                type Error = $error;

                const HOST_OP: IntlHostOp = IntlHostOp::$operation;
            }
        )+

        const _: () = {
            // The census type bounds every shift independently of wire tags.
            assert!(IntlHostOp::ALL.len() < u64::BITS as usize);
            let mut mask = 0u64;
            let mut index = 0;
            while index < IntlHostOp::ALL.len() {
                let tag = IntlHostOp::ALL[index] as u32;
                assert!(tag < u64::BITS);
                let bit = 1u64 << tag;
                assert!(mask & bit == 0);
                mask |= bit;
                assert!(!IntlHostOp::ALL[index]
                    .required_capabilities()
                    .is_empty());
                index += 1;
            }
            assert!(mask == (1u64 << IntlHostOp::ALL.len()) - 1);
        };
    };
}

mod sealed {
    pub trait Sealed {}
}

impl IntlHostOp {
    /// Public formatter operations require the requested service even when a
    /// different service retains the same physical data as a foundation.
    pub const fn required_service(self) -> Option<crate::IntlService> {
        use crate::IntlService as S;
        match self {
            Self::CanonicalizeLocale
            | Self::MaximizeLocale
            | Self::MinimizeLocale
            | Self::LookupNamedTimeZone
            | Self::NamedTimeZoneOffset
            | Self::PossibleNamedTimeZoneEpochs
            | Self::FindNamedTimeZoneTransition => None,
            Self::ResolveTimeZone
            | Self::ResolveDateTimeLocale
            | Self::SupportedDateTimeLocales
            | Self::SelectDateTimeFormat
            | Self::FormatDateTimeParts
            | Self::FormatDateTimeRangeParts => Some(S::DateTimeFormat),
            Self::ResolveNumberLocale
            | Self::SupportedNumberLocales
            | Self::FormatNumberParts
            | Self::FormatNumberRangeParts => Some(S::NumberFormat),
            Self::ResolvePluralLocale
            | Self::SupportedPluralLocales
            | Self::SelectPlural
            | Self::SelectPluralRange => Some(S::PluralRules),
            Self::ResolveListLocale | Self::SupportedListLocales | Self::FormatListParts => {
                Some(S::ListFormat)
            }
            Self::ResolveCollatorLocale
            | Self::SupportedCollatorLocales
            | Self::CompareCollator => Some(S::Collator),
            Self::ResolveDisplayNamesLocale
            | Self::SupportedDisplayNamesLocales
            | Self::DisplayName => Some(S::DisplayNames),
            Self::ResolveRelativeTimeLocale
            | Self::SupportedRelativeTimeLocales
            | Self::FormatRelativeTimeParts => Some(S::RelativeTimeFormat),
            Self::ResolveSegmenterLocale | Self::SupportedSegmenterLocales | Self::SegmentUtf16 => {
                Some(S::Segmenter)
            }
            Self::ResolveDurationFormatLocale
            | Self::SupportedDurationFormatLocales
            | Self::PartitionDurationFormat => Some(S::DurationFormat),
            Self::LocaleWeekInfoOperation
            | Self::LocaleTextInfoOperation
            | Self::LocaleHourCyclesOperation
            | Self::LocaleNumberingSystemsOperation
            | Self::LocaleCalendarsOperation
            | Self::LocaleCollationsOperation
            | Self::LocaleTimeZonesOperation => Some(S::Locale),
        }
    }
}

/// A closed association between one host operation and its request, response,
/// failure and data-capability contract.
pub trait IntlOperation: sealed::Sealed {
    type Request;
    type Response;
    type Error: std::error::Error + Send + Sync + 'static;

    const HOST_OP: IntlHostOp;
}

intl_operations! {
    CanonicalizeLocale {
        code: 0,
        name: "canonicalize-locale",
        request: LocaleTransformRequest,
        response: LocaleTransformResult,
        error: LocaleTransformError,
        capabilities: [IntlDataCapability::LocaleAliases],
    }
    LookupNamedTimeZone {
        code: 1,
        name: "lookup-named-time-zone",
        request: LookupNamedTimeZoneRequest,
        response: LookupNamedTimeZoneResult,
        error: NamedTimeZoneLookupError,
        capabilities: [IntlDataCapability::TimeZoneTransitions],
    }
    MaximizeLocale {
        code: 2,
        name: "maximize-locale",
        request: LocaleTransformRequest,
        response: LocaleTransformResult,
        error: LocaleTransformError,
        capabilities: [IntlDataCapability::LocaleAliases, IntlDataCapability::LikelySubtags],
    }
    MinimizeLocale {
        code: 3,
        name: "minimize-locale",
        request: LocaleTransformRequest,
        response: LocaleTransformResult,
        error: LocaleTransformError,
        capabilities: [IntlDataCapability::LocaleAliases, IntlDataCapability::LikelySubtags],
    }
    ResolveTimeZone {
        code: 4,
        name: "resolve-time-zone",
        request: ResolveTimeZoneRequest,
        response: ResolvedTimeZoneSnapshot,
        error: TimeZoneResolveError,
        capabilities: [IntlDataCapability::TimeZoneTransitions, IntlDataCapability::TimeZoneNames],
    }
    ResolveDateTimeLocale {
        code: 5,
        name: "resolve-date-time-locale",
        request: DateTimeLocaleRequest,
        response: DateTimeLocaleResult,
        error: DateTimeFormatError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::Calendars,
            IntlDataCapability::NumberingSystems, IntlDataCapability::DateTimePatterns],
    }
    SupportedDateTimeLocales {
        code: 6,
        name: "supported-date-time-locales",
        request: DateTimeSupportedLocalesRequest,
        response: DateTimeSupportedLocalesResult,
        error: DateTimeFormatError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::DateTimePatterns],
    }
    SelectDateTimeFormat {
        code: 7,
        name: "select-date-time-format",
        request: DateTimePlanRequest,
        response: DateTimePlanResult,
        error: DateTimeFormatError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::Calendars,
            IntlDataCapability::NumberingSystems, IntlDataCapability::DateTimePatterns],
    }
    FormatDateTimeParts {
        code: 8,
        name: "format-date-time-parts",
        request: DateTimeFormatRequest,
        response: DateTimeParts,
        error: DateTimeFormatError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::Calendars,
            IntlDataCapability::NumberingSystems, IntlDataCapability::DateTimePatterns,
            IntlDataCapability::TimeZoneNames, IntlDataCapability::TimeZoneTransitions],
    }
    FormatDateTimeRangeParts {
        code: 9,
        name: "format-date-time-range-parts",
        request: DateTimeRangeRequest,
        response: DateTimeRangeParts,
        error: DateTimeFormatError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::Calendars,
            IntlDataCapability::NumberingSystems, IntlDataCapability::DateTimePatterns,
            IntlDataCapability::TimeZoneNames, IntlDataCapability::TimeZoneTransitions],
    }
    ResolveNumberLocale {
        code: 10,
        name: "resolve-number-locale",
        request: NumberLocaleRequest,
        response: ResolvedNumberLocale,
        error: NumberFormatOperationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::NumberingSystems,
            IntlDataCapability::DecimalPatterns],
    }
    SupportedNumberLocales {
        code: 11,
        name: "supported-number-locales",
        request: NumberSupportedLocalesRequest,
        response: NumberSupportedLocalesResult,
        error: NumberFormatOperationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::DecimalPatterns],
    }
    FormatNumberParts {
        code: 12,
        name: "format-number-parts",
        request: NumberFormatRequest,
        response: ScalarNumberPartition,
        error: NumberFormatOperationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::NumberingSystems,
            IntlDataCapability::DecimalPatterns, IntlDataCapability::PluralRules,
            IntlDataCapability::UnitsAndCurrencies],
    }
    FormatNumberRangeParts {
        code: 13,
        name: "format-number-range-parts",
        request: NumberRangeFormatRequest,
        response: RangeNumberPartition,
        error: NumberFormatOperationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::NumberingSystems,
            IntlDataCapability::DecimalPatterns, IntlDataCapability::PluralRules,
            IntlDataCapability::UnitsAndCurrencies],
    }
    NamedTimeZoneOffset {
        code: 14,
        name: "named-time-zone-offset",
        request: NamedTimeZoneOffsetRequest,
        response: NamedTimeZoneOffsetSeconds,
        error: NamedTimeZoneDataError,
        capabilities: [IntlDataCapability::TimeZoneTransitions],
    }
    PossibleNamedTimeZoneEpochs {
        code: 15,
        name: "possible-named-time-zone-epochs",
        request: PossibleNamedTimeZoneEpochsRequest,
        response: PossibleNamedTimeZoneEpochsResult,
        error: NamedTimeZoneDataError,
        capabilities: [IntlDataCapability::TimeZoneTransitions],
    }
    FindNamedTimeZoneTransition {
        code: 16,
        name: "find-named-time-zone-transition",
        request: FindNamedTimeZoneTransitionRequest,
        response: FindNamedTimeZoneTransitionResult,
        error: NamedTimeZoneDataError,
        capabilities: [IntlDataCapability::TimeZoneTransitions],
    }
    ResolvePluralLocale {
        code: 17,
        name: "resolve-plural-locale",
        request: PluralLocaleRequest,
        response: ResolvedPluralLocale,
        error: PluralRulesOperationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::PluralRules],
    }
    SupportedPluralLocales {
        code: 18,
        name: "supported-plural-locales",
        request: PluralSupportedLocalesRequest,
        response: PluralSupportedLocalesResult,
        error: PluralRulesOperationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::PluralRules],
    }
    SelectPlural {
        code: 19,
        name: "select-plural",
        request: SelectPluralRequest,
        response: PluralCategory,
        error: PluralRulesOperationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::NumberingSystems, IntlDataCapability::DecimalPatterns, IntlDataCapability::PluralRules],
    }
    SelectPluralRange {
        code: 20,
        name: "select-plural-range",
        request: SelectPluralRangeRequest,
        response: PluralCategory,
        error: PluralRulesOperationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::NumberingSystems, IntlDataCapability::DecimalPatterns, IntlDataCapability::PluralRules],
    }

    ResolveListLocale {
        code: 21, name: "resolve-list-locale", request: ListLocaleRequest, response: ResolvedListLocale, error: ListFormatOperationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::ListPatterns],
    }
    SupportedListLocales {
        code: 22, name: "supported-list-locales", request: ListSupportedLocalesRequest, response: ListSupportedLocalesResult, error: ListFormatOperationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::ListPatterns],
    }
    FormatListParts {
        code: 23, name: "format-list-parts", request: FormatListPartsRequest, response: ListParts, error: ListFormatOperationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::ListPatterns],
    }
    ResolveCollatorLocale {
        code: 24, name: "resolve-collator-locale", request: CollatorLocaleRequest, response: ResolvedCollatorLocale, error: CollatorOperationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::Collation],
    }
    SupportedCollatorLocales {
        code: 25, name: "supported-collator-locales", request: CollatorSupportedLocalesRequest, response: CollatorSupportedLocalesResult, error: CollatorOperationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::Collation],
    }
    CompareCollator {
        code: 26, name: "compare-collator", request: CompareCollatorRequest, response: CollatorOrdering, error: CollatorOperationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::Collation],
    }
    ResolveDisplayNamesLocale {
        code: 27, name: "resolve-display-names-locale", request: DisplayNamesLocaleRequest, response: ResolvedDisplayNamesLocale, error: DisplayNamesError,
        capabilities: [IntlDataCapability::LocaleAliases, IntlDataCapability::ParentLocales, IntlDataCapability::DisplayNames],
    }
    SupportedDisplayNamesLocales {
        code: 28, name: "supported-display-names-locales", request: DisplayNamesLocaleRequest, response: DisplayNamesSupportedLocalesResult, error: DisplayNamesError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::DisplayNames],
    }
    DisplayName {
        code: 29, name: "display-name", request: DisplayNameRequest, response: DisplayNameResult, error: DisplayNamesError,
        capabilities: [IntlDataCapability::LocaleAliases, IntlDataCapability::DisplayNames],
    }
    ResolveRelativeTimeLocale {
        code: 30, name: "resolve-relative-time-locale", request: NumberLocaleRequest, response: ResolvedRelativeTimeLocale, error: RelativeTimeError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::NumberingSystems, IntlDataCapability::DecimalPatterns, IntlDataCapability::RelativeTimePatterns],
    }
    SupportedRelativeTimeLocales {
        code: 31, name: "supported-relative-time-locales", request: NumberSupportedLocalesRequest, response: NumberSupportedLocalesResult, error: RelativeTimeError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::RelativeTimePatterns],
    }
    FormatRelativeTimeParts {
        code: 32, name: "format-relative-time-parts", request: FormatRelativeTimePartsRequest, response: RelativePartition, error: RelativeTimeError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::NumberingSystems, IntlDataCapability::DecimalPatterns, IntlDataCapability::PluralRules, IntlDataCapability::RelativeTimePatterns],
    }
    ResolveSegmenterLocale {
        code: 33, name: "resolve-segmenter-locale", request: SegmenterLocaleRequest, response: ResolvedSegmenterLocale, error: SegmenterError,
        capabilities: [IntlDataCapability::LocaleAliases, IntlDataCapability::ParentLocales, IntlDataCapability::Segmentation],
    }
    SupportedSegmenterLocales {
        code: 34, name: "supported-segmenter-locales", request: SegmenterLocaleRequest, response: SegmenterSupportedLocalesResult, error: SegmenterError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::Segmentation],
    }
    SegmentUtf16 {
        code: 35, name: "segment-utf16", request: SegmentUtf16Request, response: SegmenterResult, error: SegmenterError,
        capabilities: [IntlDataCapability::Segmentation],
    }
    ResolveDurationFormatLocale {
        code: 36, name: "resolve-duration-format-locale", request: NumberLocaleRequest, response: ResolvedDurationLocale, error: DurationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::NumberingSystems, IntlDataCapability::DurationPatterns],
    }
    SupportedDurationFormatLocales {
        code: 37, name: "supported-duration-format-locales", request: DurationSupportedLocalesRequest, response: Box<[CanonicalLocaleId]>, error: DurationError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::DurationPatterns],
    }
    PartitionDurationFormat {
        code: 38, name: "partition-duration-format", request: DurationPartitionRequest, response: DurationPartition, error: DurationError,
        capabilities: [IntlDataCapability::DurationPatterns, IntlDataCapability::NumberingSystems, IntlDataCapability::DecimalPatterns, IntlDataCapability::PluralRules, IntlDataCapability::ListPatterns, IntlDataCapability::UnitsAndCurrencies],
    }
    LocaleWeekInfoOperation {
        code: 39, name: "locale-week-info", request: LocaleWeekRequest, response: LocaleWeekInfo, error: LocaleWeekError,
        capabilities: [IntlDataCapability::LocaleAliases, IntlDataCapability::LikelySubtags, IntlDataCapability::WeekData],
    }
    LocaleTextInfoOperation {
        code: 40, name: "locale-text-info", request: LocaleTextInfoRequest, response: Option<LocaleTextDirection>, error: LocaleTextError,
        capabilities: [IntlDataCapability::LocaleAliases, IntlDataCapability::LikelySubtags, IntlDataCapability::ScriptDirection],
    }
    LocaleHourCyclesOperation {
        code: 41, name: "locale-hour-cycles", request: LocaleHourCyclesRequest, response: LocaleHourCycles, error: LocaleHourCyclesError,
        capabilities: [IntlDataCapability::LocaleAliases, IntlDataCapability::LikelySubtags, IntlDataCapability::HourCycles],
    }
    LocaleNumberingSystemsOperation {
        code: 42, name: "locale-numbering-systems", request: LocaleNumberingSystemsRequest, response: DecimalNumberingSystem, error: LocaleNumberingSystemsError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::NumberingSystems],
    }
    LocaleCalendarsOperation {
        code: 43, name: "locale-calendars", request: LocaleCalendarsRequest, response: LocaleCalendars, error: LocaleCalendarsError,
        capabilities: [IntlDataCapability::LocaleAliases, IntlDataCapability::LikelySubtags, IntlDataCapability::Calendars, IntlDataCapability::CalendarPreferences],
    }
    LocaleCollationsOperation {
        code: 44, name: "locale-collations", request: LocaleCollationsRequest, response: LocaleCollations, error: LocaleCollationsError,
        capabilities: [IntlDataCapability::ParentLocales, IntlDataCapability::Collation],
    }
    LocaleTimeZonesOperation {
        code: 45, name: "locale-time-zones", request: LocaleTimeZonesRequest, response: LocaleTimeZones, error: LocaleTimeZonesError,
        capabilities: [IntlDataCapability::TimeZoneRegions],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleTransformRequest {
    locale: LocaleId,
}

impl LocaleTransformRequest {
    /// Creates a request from a structurally validated locale identifier.
    ///
    /// Raw strings cannot cross the kernel boundary.
    ///
    /// ```compile_fail
    /// use scriptw_intl::LocaleTransformRequest;
    ///
    /// let _ = LocaleTransformRequest::new("en-US");
    /// ```
    #[must_use]
    pub const fn new(locale: LocaleId) -> Self {
        Self { locale }
    }

    #[must_use]
    pub const fn locale(&self) -> &LocaleId {
        &self.locale
    }

    #[must_use]
    pub fn into_locale(self) -> LocaleId {
        self.locale
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleTransformResult {
    locale: CanonicalLocaleId,
}

impl LocaleTransformResult {
    #[must_use]
    pub const fn new(locale: CanonicalLocaleId) -> Self {
        Self { locale }
    }

    #[must_use]
    pub const fn locale(&self) -> &CanonicalLocaleId {
        &self.locale
    }

    #[must_use]
    pub fn into_locale(self) -> CanonicalLocaleId {
        self.locale
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedLocale {
    locale: LocaleId,
}

impl UnsupportedLocale {
    #[must_use]
    pub const fn new(locale: LocaleId) -> Self {
        Self { locale }
    }

    #[must_use]
    pub const fn locale(&self) -> &LocaleId {
        &self.locale
    }
}

impl fmt::Display for UnsupportedLocale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Intl provider does not support locale {:?}",
            self.locale.as_str()
        )
    }
}

impl std::error::Error for UnsupportedLocale {}

/// Provider failure for locale transforms, separating an expected
/// unsupported input from corrupt/non-canonical provider output.
#[derive(Debug)]
pub enum LocaleTransformError {
    Unsupported(UnsupportedLocale),
    InvalidProviderResult(InvalidCanonicalLocaleId),
}

impl fmt::Display for LocaleTransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(error) => error.fmt(f),
            Self::InvalidProviderResult(error) => {
                write!(
                    f,
                    "Intl provider returned invalid canonical locale: {error}"
                )
            }
        }
    }
}

impl std::error::Error for LocaleTransformError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unsupported(error) => Some(error),
            Self::InvalidProviderResult(error) => Some(error),
        }
    }
}

impl From<UnsupportedLocale> for LocaleTransformError {
    fn from(error: UnsupportedLocale) -> Self {
        Self::Unsupported(error)
    }
}

impl From<InvalidCanonicalLocaleId> for LocaleTransformError {
    fn from(error: InvalidCanonicalLocaleId) -> Self {
        Self::InvalidProviderResult(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTimeZone {
    time_zone: TimeZoneId,
}

impl UnknownTimeZone {
    #[must_use]
    pub const fn new(time_zone: TimeZoneId) -> Self {
        Self { time_zone }
    }

    #[must_use]
    pub const fn time_zone(&self) -> &TimeZoneId {
        &self.time_zone
    }
}

impl fmt::Display for UnknownTimeZone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Intl provider does not know time zone {:?}",
            self.time_zone.as_str()
        )
    }
}

impl std::error::Error for UnknownTimeZone {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamedTimeZoneLookupError {
    Unknown(UnknownTimeZone),
    UnavailableService(crate::IntlService),
}
impl fmt::Display for NamedTimeZoneLookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(error) => error.fmt(f),
            Self::UnavailableService(service) => crate::UnavailableIntlService(*service).fmt(f),
        }
    }
}
impl std::error::Error for NamedTimeZoneLookupError {}
impl From<UnknownTimeZone> for NamedTimeZoneLookupError {
    fn from(error: UnknownTimeZone) -> Self {
        Self::Unknown(error)
    }
}

/// Immutable identity exposed by a pure data provider.
pub trait IntlProvider {
    fn identity(&self) -> &IntlDataIdentity;
}

/// Provider implementation for one closed operation.
///
/// Implementations never receive JavaScript values: only the request type
/// associated with `O` can reach this method.
pub trait IntlOperationProvider<O: IntlOperation>: IntlProvider {
    fn execute(&self, request: O::Request) -> Result<O::Response, O::Error>;
}

/// An identity-matched provider ready to grant typed operation capabilities.
///
/// A profile plan alone is deliberately not sufficient to instantiate the
/// kernel; artifact schema, versions, placement and digest must all match.
///
/// ```compile_fail
/// use scriptw_intl::{IntlKernel, IntlProfilePlan, IntlProvider};
///
/// fn install<P: IntlProvider>(provider: P) {
///     let _ = IntlKernel::new(IntlProfilePlan::conformance(), provider);
/// }
/// ```
#[derive(Debug)]
pub struct IntlKernel<P> {
    identity: IntlDataIdentity,
    provider: P,
}

impl<P: IntlProvider> IntlKernel<P> {
    pub fn new(
        expected: IntlDataIdentity,
        provider: P,
    ) -> Result<Self, IntlProviderIdentityMismatch> {
        let actual: IntlDataIdentity = provider.identity().clone();
        if actual != expected {
            return Err(IntlProviderIdentityMismatch { expected, actual });
        }
        Ok(Self {
            identity: expected,
            provider,
        })
    }

    #[must_use]
    pub const fn identity(&self) -> &IntlDataIdentity {
        &self.identity
    }

    pub(crate) fn provider(&self) -> &P {
        &self.provider
    }

    pub fn operation<O>(&self) -> Result<IntlOperationHandle<'_, P, O>, MissingIntlCapabilities>
    where
        O: IntlOperation,
        P: IntlOperationProvider<O>,
    {
        let available = self.identity.profile().capabilities();
        let required = O::HOST_OP.required_capabilities();
        if let Some(service) = O::HOST_OP.required_service() {
            if !self.identity.profile().services().contains(service) {
                return Err(MissingIntlCapabilities {
                    operation: O::HOST_OP,
                    missing: IntlCapabilitySet::EMPTY,
                    unavailable_service: Some(service),
                });
            }
        }
        if !available.contains_all(required) {
            return Err(MissingIntlCapabilities {
                operation: O::HOST_OP,
                missing: required.difference(available),
                unavailable_service: None,
            });
        }
        Ok(IntlOperationHandle {
            kernel: self,
            operation: PhantomData,
        })
    }
}

#[derive(Debug)]
pub struct IntlOperationHandle<'a, P, O> {
    kernel: &'a IntlKernel<P>,
    operation: PhantomData<fn() -> O>,
}

impl<P, O> IntlOperationHandle<'_, P, O>
where
    O: IntlOperation,
    P: IntlOperationProvider<O>,
{
    /// Executes the one operation authorized by this handle.
    ///
    /// A locale request cannot be sent through a time-zone handle (or vice
    /// versa), because the request type is selected by `O`.
    ///
    /// ```compile_fail
    /// use scriptw_intl::{
    ///     CanonicalizeLocale, LookupNamedTimeZoneRequest, IntlOperationHandle,
    ///     IntlOperationProvider, TimeZoneId,
    /// };
    ///
    /// fn wrong_request<P>(handle: IntlOperationHandle<'_, P, CanonicalizeLocale>)
    /// where
    ///     P: IntlOperationProvider<CanonicalizeLocale>,
    /// {
    ///     let zone = TimeZoneId::parse("UTC").unwrap();
    ///     let _ = handle.execute(LookupNamedTimeZoneRequest::new(zone));
    /// }
    /// ```
    pub fn execute(&self, request: O::Request) -> Result<O::Response, O::Error> {
        IntlOperationProvider::<O>::execute(&self.kernel.provider, request)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntlProviderIdentityMismatch {
    expected: IntlDataIdentity,
    actual: IntlDataIdentity,
}

impl IntlProviderIdentityMismatch {
    #[must_use]
    pub const fn expected(&self) -> &IntlDataIdentity {
        &self.expected
    }

    #[must_use]
    pub const fn actual(&self) -> &IntlDataIdentity {
        &self.actual
    }
}

impl fmt::Display for IntlProviderIdentityMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Intl provider identity does not match the artifact identity")
    }
}

impl std::error::Error for IntlProviderIdentityMismatch {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MissingIntlCapabilities {
    operation: IntlHostOp,
    missing: IntlCapabilitySet,
    unavailable_service: Option<crate::IntlService>,
}

impl MissingIntlCapabilities {
    #[must_use]
    pub const fn operation(&self) -> IntlHostOp {
        self.operation
    }

    #[must_use]
    pub const fn missing(&self) -> IntlCapabilitySet {
        self.missing
    }
    pub const fn unavailable_service(&self) -> Option<crate::IntlService> {
        self.unavailable_service
    }
}

impl fmt::Display for MissingIntlCapabilities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(service) = self.unavailable_service {
            return crate::UnavailableIntlService(service).fmt(f);
        }
        write!(f, "Intl operation {} requires", self.operation.name())?;
        for capability in self.missing.iter() {
            write!(f, " {}", capability.name())?;
        }
        Ok(())
    }
}

impl std::error::Error for MissingIntlCapabilities {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IntlDataDigest, IntlDataPlacement, IntlProfilePlan, IntlService, IntlServiceSet};

    #[derive(Debug)]
    struct FixtureProvider {
        identity: IntlDataIdentity,
    }

    impl IntlProvider for FixtureProvider {
        fn identity(&self) -> &IntlDataIdentity {
            &self.identity
        }
    }

    impl IntlOperationProvider<CanonicalizeLocale> for FixtureProvider {
        fn execute(
            &self,
            request: LocaleTransformRequest,
        ) -> Result<LocaleTransformResult, LocaleTransformError> {
            let locale = match request.locale().as_str() {
                "EN-us" => CanonicalLocaleId::from_data("en-US").unwrap(),
                "iw-IL" => CanonicalLocaleId::from_data("he-IL").unwrap(),
                _ => return Err(UnsupportedLocale::new(request.locale).into()),
            };
            Ok(LocaleTransformResult::new(locale))
        }
    }

    impl IntlOperationProvider<LookupNamedTimeZone> for FixtureProvider {
        fn execute(
            &self,
            request: LookupNamedTimeZoneRequest,
        ) -> Result<LookupNamedTimeZoneResult, NamedTimeZoneLookupError> {
            let identity = match request.identifier().as_str() {
                "europe/stockholm" => {
                    crate::NamedTimeZoneIdentity::from_data("Europe/Stockholm", "Europe/Stockholm")
                        .unwrap()
                }
                "Etc/UTC" => crate::NamedTimeZoneIdentity::from_data("Etc/UTC", "UTC").unwrap(),
                _ => return Err(UnknownTimeZone::new(request.into_identifier()).into()),
            };
            Ok(LookupNamedTimeZoneResult::new(identity))
        }
    }

    fn identity_for(services: IntlServiceSet, digest: u8) -> IntlDataIdentity {
        IntlDataIdentity::new(
            IntlProfilePlan::minimal(services).unwrap(),
            CanonicalLocaleId::from_data("en-US").unwrap(),
            IntlDataPlacement::External,
            IntlDataDigest::from_sha256([digest; 32]),
        )
    }

    #[test]
    fn kernel_consumes_typed_locale_and_time_zone_paths() {
        let identity = IntlDataIdentity::new(
            IntlProfilePlan::conformance(),
            CanonicalLocaleId::from_data("en-US").unwrap(),
            IntlDataPlacement::Embedded,
            IntlDataDigest::from_sha256([0x23; 32]),
        );
        let provider = FixtureProvider {
            identity: identity.clone(),
        };
        let kernel = IntlKernel::new(identity, provider).unwrap();

        let locale = kernel
            .operation::<CanonicalizeLocale>()
            .unwrap()
            .execute(LocaleTransformRequest::new(
                LocaleId::parse("iw-IL").unwrap(),
            ))
            .unwrap();
        assert_eq!(locale.locale().as_str(), "he-IL");

        let time_zone = kernel
            .operation::<LookupNamedTimeZone>()
            .unwrap()
            .execute(LookupNamedTimeZoneRequest::new(
                TimeZoneId::parse("europe/stockholm").unwrap(),
            ))
            .unwrap();
        assert_eq!(time_zone.identity().identifier(), "Europe/Stockholm");
    }

    #[test]
    fn profile_capabilities_are_checked_before_provider_execution() {
        let services = IntlServiceSet::EMPTY.with(IntlService::Locale);
        let identity = identity_for(services, 0x11);
        let provider = FixtureProvider {
            identity: identity.clone(),
        };
        let kernel = IntlKernel::new(identity, provider).unwrap();

        assert!(kernel.operation::<CanonicalizeLocale>().is_ok());
        let error = kernel.operation::<LookupNamedTimeZone>().unwrap_err();
        assert_eq!(error.operation(), IntlHostOp::LookupNamedTimeZone);
        assert!(error
            .missing()
            .contains(IntlDataCapability::TimeZoneTransitions));
    }

    #[test]
    fn provider_identity_mismatch_rejects_kernel_construction() {
        let services = IntlServiceSet::EMPTY.with(IntlService::Locale);
        let expected = identity_for(services, 0x11);
        let provider = FixtureProvider {
            identity: identity_for(services, 0x12),
        };

        assert!(IntlKernel::new(expected, provider).is_err());
    }

    #[test]
    fn intl_host_wire_domain_is_closed_and_stable() {
        assert_eq!(IntlHostOp::CanonicalizeLocale.wire(), 0);
        assert_eq!(IntlHostOp::LookupNamedTimeZone.wire(), 1);
        assert_eq!(
            IntlHostOp::from_wire(0),
            Some(IntlHostOp::CanonicalizeLocale)
        );
        assert_eq!(
            IntlHostOp::from_wire(1),
            Some(IntlHostOp::LookupNamedTimeZone)
        );
        assert_eq!(IntlHostOp::from_wire(-1), None);
        assert_eq!(IntlHostOp::MaximizeLocale.wire(), 2);
        assert_eq!(IntlHostOp::MinimizeLocale.wire(), 3);
        assert_eq!(IntlHostOp::from_wire(2), Some(IntlHostOp::MaximizeLocale));
        assert_eq!(IntlHostOp::from_wire(3), Some(IntlHostOp::MinimizeLocale));
        assert_eq!(IntlHostOp::ResolveTimeZone.wire(), 4);
        assert_eq!(IntlHostOp::from_wire(4), Some(IntlHostOp::ResolveTimeZone));
        for operation in IntlHostOp::ALL {
            assert_eq!(IntlHostOp::from_wire(operation.wire()), Some(*operation));
        }
        assert_eq!(IntlHostOp::from_wire(IntlHostOp::ALL.len() as i64), None);

        let read = IntlHostReadSpan::new(u32::MAX, u32::MAX);
        assert_eq!(IntlHostReadSpan::from_wire(read.wire()), read);
        assert_eq!(read.offset(), u32::MAX);
        assert_eq!(read.length(), u32::MAX);

        let write = IntlHostWriteSpan::new(u32::MAX, u32::MAX);
        assert_eq!(IntlHostWriteSpan::from_wire(write.wire()), write);
        assert_eq!(write.offset(), u32::MAX);
        assert_eq!(write.capacity(), u32::MAX);

        for outcome in [
            IntlHostCallOutcome::Rejected,
            IntlHostCallOutcome::Written(0),
            IntlHostCallOutcome::Written(u32::MAX),
            IntlHostCallOutcome::RequiredCapacity(0),
            IntlHostCallOutcome::RequiredCapacity(u32::MAX),
        ] {
            assert_eq!(
                IntlHostCallOutcome::from_wire(outcome.wire()),
                Some(outcome)
            );
        }
        assert_eq!(IntlHostCallOutcome::from_wire(-3 - u32::MAX as i64), None);
        assert_eq!(IntlHostCallOutcome::from_wire(u32::MAX as i64 + 1), None);
    }
}

impl IntlKernel<crate::EmbeddedIntlProvider> {
    /// Decode with the profile admitted before this identity-matched provider was published.
    pub fn decode_display_name_request(
        &self,
        bytes: &[u8],
    ) -> Result<crate::DisplayNameRequest, crate::DisplayNamesWireError> {
        crate::display_names_protocol::decode_display_name_request(
            bytes,
            self.provider
                .display_names_profiles()
                .filter(|_| {
                    self.provider
                        .permits_service(crate::IntlService::DisplayNames)
                })
                .ok_or(crate::UnavailableIntlService(
                    crate::IntlService::DisplayNames,
                ))?,
        )
    }
}

impl IntlKernel<crate::EmbeddedIntlProvider> {
    /// Decode into the same model-certified owner installed before publication.
    pub fn decode_segment_utf16_request(
        &self,
        bytes: &[u8],
    ) -> Result<crate::SegmentUtf16Request, crate::SegmenterWireError> {
        crate::segmenter_protocol::decode_segment_utf16_request(
            self.provider
                .segmenter_profiles()
                .filter(|_| self.provider.permits_service(crate::IntlService::Segmenter))
                .ok_or(crate::UnavailableIntlService(crate::IntlService::Segmenter))?,
            bytes,
        )
    }
}

impl IntlKernel<crate::EmbeddedIntlProvider> {
    /// Decode only with the checked Duration/NF owners installed in this provider.
    pub fn decode_duration_request(
        &self,
        operation: crate::DurationHostOp,
        bytes: &[u8],
    ) -> Result<crate::DurationWireRequest, crate::DurationWireError> {
        crate::duration_wire::decode_duration_request(
            operation,
            bytes,
            self.provider
                .duration_profiles()
                .filter(|_| {
                    self.provider
                        .permits_service(crate::IntlService::DurationFormat)
                })
                .ok_or(crate::UnavailableIntlService(
                    crate::IntlService::DurationFormat,
                ))?,
            self.provider
                .duration_number_profiles()
                .filter(|_| {
                    self.provider
                        .permits_service(crate::IntlService::DurationFormat)
                })
                .ok_or(crate::UnavailableIntlService(
                    crate::IntlService::DurationFormat,
                ))?,
            &crate::number_format::PartitionLimits::HOST_ABI,
        )
    }
}

impl IntlKernel<crate::EmbeddedIntlProvider> {
    /// Decode against the actual admitted List image installed in this kernel.
    pub fn decode_list_format_request(
        &self,
        bytes: &[u8],
    ) -> Result<crate::FormatListPartsRequest, crate::ListWireError> {
        crate::FormatListPartsRequest::decode(
            bytes,
            self.provider
                .list_profiles()
                .filter(|_| {
                    self.provider
                        .permits_service(crate::IntlService::ListFormat)
                })
                .ok_or(crate::UnavailableIntlService(
                    crate::IntlService::ListFormat,
                ))?,
        )
    }
    /// Decode against the actual admitted Collator image installed in this kernel.
    pub fn decode_collator_compare_request(
        &self,
        bytes: &[u8],
    ) -> Result<crate::CompareCollatorRequest, crate::CollatorWireError> {
        crate::CompareCollatorRequest::decode(
            bytes,
            self.provider
                .collator_profiles()
                .filter(|_| self.provider.permits_service(crate::IntlService::Collator))
                .ok_or(crate::UnavailableIntlService(crate::IntlService::Collator))?,
        )
    }
}

impl<P: crate::NumberProfilesProvider> IntlKernel<P> {
    pub fn decode_number_format_request(
        &self,
        bytes: &[u8],
    ) -> Result<crate::NumberFormatRequest, crate::NumberWireError> {
        crate::NumberFormatRequest::decode(
            bytes,
            self.provider
                .number_profiles()
                .filter(|_| {
                    self.provider
                        .permits_number_service(crate::IntlService::NumberFormat)
                })
                .ok_or(crate::UnavailableIntlService(
                    crate::IntlService::NumberFormat,
                ))?,
        )
    }
    pub fn decode_number_range_request(
        &self,
        bytes: &[u8],
    ) -> Result<crate::NumberRangeFormatRequest, crate::NumberWireError> {
        crate::NumberRangeFormatRequest::decode(
            bytes,
            self.provider
                .number_profiles()
                .filter(|_| {
                    self.provider
                        .permits_number_service(crate::IntlService::NumberFormat)
                })
                .ok_or(crate::UnavailableIntlService(
                    crate::IntlService::NumberFormat,
                ))?,
        )
    }
    pub fn decode_plural_select_request(
        &self,
        bytes: &[u8],
    ) -> Result<crate::SelectPluralRequest, crate::PluralWireError> {
        crate::SelectPluralRequest::decode(
            bytes,
            self.provider
                .number_profiles()
                .filter(|_| {
                    self.provider
                        .permits_number_service(crate::IntlService::PluralRules)
                })
                .ok_or(crate::UnavailableIntlService(
                    crate::IntlService::PluralRules,
                ))?,
        )
    }
    pub fn decode_plural_range_request(
        &self,
        bytes: &[u8],
    ) -> Result<crate::SelectPluralRangeRequest, crate::PluralWireError> {
        crate::SelectPluralRangeRequest::decode(
            bytes,
            self.provider
                .number_profiles()
                .filter(|_| {
                    self.provider
                        .permits_number_service(crate::IntlService::PluralRules)
                })
                .ok_or(crate::UnavailableIntlService(
                    crate::IntlService::PluralRules,
                ))?,
        )
    }
}

impl IntlKernel<crate::EmbeddedIntlProvider> {
    pub fn decode_relative_time_request(
        &self,
        operation: crate::RelativeHostOp,
        bytes: &[u8],
    ) -> Result<crate::RelativeRequest, crate::RelativeWireError> {
        crate::relative_time_protocol::decode_relative_request(
            operation,
            bytes,
            self.provider
                .relative_time_profiles()
                .filter(|_| {
                    self.provider
                        .permits_service(crate::IntlService::RelativeTimeFormat)
                })
                .ok_or(crate::UnavailableIntlService(
                    crate::IntlService::RelativeTimeFormat,
                ))?,
            self.provider
                .number_profiles()
                .filter(|_| {
                    self.provider
                        .permits_service(crate::IntlService::RelativeTimeFormat)
                })
                .ok_or(crate::UnavailableIntlService(
                    crate::IntlService::RelativeTimeFormat,
                ))?,
            &crate::number_format::PartitionLimits::HOST_ABI,
        )
    }
}
