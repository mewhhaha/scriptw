//! `Temporal.PlainDate` codegen.
//!
//! Temporal proposal 3: a calendar date with no time and no time zone. The
//! record is three plain `i64` ISO fields plus an interned calendar payload —
//! `RejectISODate` bounds every field, so nothing here needs the BigInt
//! machinery the epoch-nanosecond types carry.
//!
//! ISO 8601, Gregorian, Buddhist and ROC dates all use proleptic Gregorian month
//! and day arithmetic over the stored ISO date. Buddhist calendar years are
//! ISO years plus 543 and ROC calendar years are ISO years minus 1911.
//! Indian and Persian use distinct twelve-month solar arithmetic. Coptic and
//! Ethiopian calendars use thirteen months. Islamic civil and tabular use
//! Type-II lunar arithmetic; Umm al-Qura uses the required table and civil
//! arithmetic outside its range. Those lunar domains have twelve months.
//! Hebrew uses a civil Tishrei-first year with twelve or thirteen months.
//! Chinese and Dangi use related Gregorian years and twelve or thirteen lunar
//! months, selected by the checked retained catalog or the exact mean model.
//! Constructors and annotated strings retain ISO fields. Property bags resolve calendar years first;
//! [`TemporalResolvedCalendarYear`] is consumed only when complete calendar
//! fields convert to ISO.
//!
//! [`TemporalCalendarArithmetic`] is the closed arithmetic contract consumed
//! by field projection and year resolution. A calendar with different month
//! arithmetic must extend that domain and replace the ISO date-add/difference
//! and month-day reference-date paths before it can use these emitters.
//! Only ISO defines week numbering. Eras, calendar annotations and calendar
//! equality remain separate calendar-specific operations.

use super::temporal_zone_provider::TemporalCalendarSlotLocals;
use crate::gc_types::*;

use super::super::*;
use crate::data::{TemporalEastAsianCalendar, TEMPORAL_EAST_ASIAN_YEAR_ENVELOPE};
use crate::intrinsics::temporal::{TemporalIntrinsicFamily, TemporalPrototypeSource};

// Each closed enum and its actual consumer census share one declaration.
// Appending a variant cannot omit it from canonicalization or source pooling.
macro_rules! temporal_closed_enum {
    ($(#[$enum_attribute:meta])* $visibility:vis enum $name:ident {
        $( $(#[$documentation:meta])* $variant:ident $(= $encoding:literal)?, )*
    }) => {
        $(#[$enum_attribute])*
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        $visibility enum $name {
            $( $(#[$documentation])* $variant $(= $encoding)?, )*
        }
        impl $name {
            pub(crate) const ALL: [Self; [$(stringify!($variant)),*].len()] = [
                $(Self::$variant,)*
            ];
        }
    };
}

temporal_closed_enum! {
    /// Every calendar identifier this backend answers to.
    pub(crate) enum TemporalCalendarId {
        /// The proposal's default. No eras, and `calendarName: "auto"` suppresses
        /// its annotation.
        Iso8601,
        /// Proleptic Gregorian. Identical arithmetic to [`Self::Iso8601`]; it adds
        /// the `ce`/`bce` era pair and is always annotated under `auto`.
        ///
        /// Both halves of the era feature are implemented: the accessors
        /// ([`FunctionBuilder::emit_temporal_gregorian_era_field`]) and the
        /// property-bag direction ([`FunctionBuilder::emit_temporal_resolve_era_to_calendar_year`]),
        /// which is what makes `{ era: "bce", eraYear: 1 }` a year source and an
        /// unknown era a RangeError.
        Gregory,
        /// Proleptic Gregorian month/day arithmetic with year = ISO year + 543.
        Buddhist,
        /// Proleptic Gregorian month/day arithmetic with year = ISO year - 1911.
        ///
        /// ROC year 1 is ISO 1912; the `roc` era counts forward from there and
        /// `broc` counts backwards (`broc` 1 is ISO 1911). Months, leap years and
        /// every other date field are identical to Gregorian — only the year
        /// numbering, the eras and the annotation differ.
        Roc,
        /// Proleptic Gregorian month/day arithmetic with ISO year numbering and
        /// seven eras: five imperial eras selected by exact start date plus
        /// `ce`/`bce` for older dates. See [`JapaneseEra`].
        Japanese,
        /// Fixed twelve-month solar arithmetic in the Shaka era.
        Indian,
        /// Twelve-month Persian solar arithmetic in the signed Anno Persico era.
        Persian,
        /// Thirteen-month Coptic arithmetic with signed Anno Martyrum years.
        Coptic,
        /// Ethiopian arithmetic with Amete Mihret and Amete Alem eras.
        Ethiopic,
        /// Ethiopian arithmetic with the Amete Alem epoch and arithmetic year.
        Ethioaa,
        /// Type-II tabular Hijri arithmetic with the Friday civil epoch.
        IslamicCivil,
        /// Type-II tabular Hijri arithmetic with the Thursday epoch.
        IslamicTbla,
        /// Required Umm al-Qura table, with civil arithmetic outside its range.
        IslamicUmalqura,
        /// Civil Tishrei-first Hebrew arithmetic with a Metonic leap month.
        Hebrew,
        /// Chinese related-ISO years with retained astronomical data.
        Chinese,
        /// Korean related-ISO years with their own observation policy.
        Dangi,
    }
}

temporal_closed_enum! {
    /// Canonical codes consumed by suitability, projection output and pooling.
    #[repr(i64)]
    pub(crate) enum TemporalCalendarMonthCode {
        M01 = 1,
        M02 = 2,
        M03 = 3,
        M04 = 4,
        M05 = 5,
        M06 = 6,
        M07 = 7,
        M08 = 8,
        M09 = 9,
        M10 = 10,
        M11 = 11,
        M12 = 12,
        M13 = 13,
        M05L = 14,
        M01L = 15,
        M02L = 16,
        M03L = 17,
        M04L = 18,
        M06L = 19,
        M07L = 20,
        M08L = 21,
        M09L = 22,
        M10L = 23,
        M11L = 24,
        M12L = 25,
    }
}

impl TemporalCalendarMonthCode {
    pub(crate) const fn encoding(self) -> i64 {
        self as i64
    }

    pub(crate) const fn spelling(self) -> &'static str {
        match self {
            Self::M01 => "M01",
            Self::M02 => "M02",
            Self::M03 => "M03",
            Self::M04 => "M04",
            Self::M05 => "M05",
            Self::M06 => "M06",
            Self::M07 => "M07",
            Self::M08 => "M08",
            Self::M09 => "M09",
            Self::M10 => "M10",
            Self::M11 => "M11",
            Self::M12 => "M12",
            Self::M13 => "M13",
            Self::M05L => "M05L",
            Self::M01L => "M01L",
            Self::M02L => "M02L",
            Self::M03L => "M03L",
            Self::M04L => "M04L",
            Self::M06L => "M06L",
            Self::M07L => "M07L",
            Self::M08L => "M08L",
            Self::M09L => "M09L",
            Self::M10L => "M10L",
            Self::M11L => "M11L",
            Self::M12L => "M12L",
        }
    }

    pub(crate) const fn month_number(self) -> i64 {
        match self {
            Self::M01 | Self::M01L => 1,
            Self::M02 | Self::M02L => 2,
            Self::M03 | Self::M03L => 3,
            Self::M04 | Self::M04L => 4,
            Self::M05 | Self::M05L => 5,
            Self::M06 | Self::M06L => 6,
            Self::M07 | Self::M07L => 7,
            Self::M08 | Self::M08L => 8,
            Self::M09 | Self::M09L => 9,
            Self::M10 | Self::M10L => 10,
            Self::M11 | Self::M11L => 11,
            Self::M12 | Self::M12L => 12,
            Self::M13 => 13,
        }
    }

    pub(crate) const fn is_leap(self) -> bool {
        match self {
            Self::M01
            | Self::M02
            | Self::M03
            | Self::M04
            | Self::M05
            | Self::M06
            | Self::M07
            | Self::M08
            | Self::M09
            | Self::M10
            | Self::M11
            | Self::M12
            | Self::M13 => false,
            Self::M01L
            | Self::M02L
            | Self::M03L
            | Self::M04L
            | Self::M05L
            | Self::M06L
            | Self::M07L
            | Self::M08L
            | Self::M09L
            | Self::M10L
            | Self::M11L
            | Self::M12L => true,
        }
    }

    /// Original code chronology, independent of a constrained target ordinal.
    pub(crate) const fn rank(self) -> i64 {
        self.month_number() * 2 + self.is_leap() as i64
    }
}

/// Closed month rules used by field suitability, count and emitted arithmetic.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TemporalCalendarMonthArithmetic {
    Twelve,
    Thirteen,
    HebrewMetonic,
    EastAsianLunisolar(TemporalEastAsianCalendar),
}

impl TemporalCalendarMonthArithmetic {
    pub(crate) const fn accepts_code(self, code: TemporalCalendarMonthCode) -> bool {
        match code {
            TemporalCalendarMonthCode::M01
            | TemporalCalendarMonthCode::M02
            | TemporalCalendarMonthCode::M03
            | TemporalCalendarMonthCode::M04
            | TemporalCalendarMonthCode::M05
            | TemporalCalendarMonthCode::M06
            | TemporalCalendarMonthCode::M07
            | TemporalCalendarMonthCode::M08
            | TemporalCalendarMonthCode::M09
            | TemporalCalendarMonthCode::M10
            | TemporalCalendarMonthCode::M11
            | TemporalCalendarMonthCode::M12 => match self {
                Self::Twelve
                | Self::Thirteen
                | Self::HebrewMetonic
                | Self::EastAsianLunisolar(_) => true,
            },
            TemporalCalendarMonthCode::M13 => match self {
                Self::Twelve | Self::HebrewMetonic | Self::EastAsianLunisolar(_) => false,
                Self::Thirteen => true,
            },
            TemporalCalendarMonthCode::M05L => match self {
                Self::Twelve | Self::Thirteen => false,
                Self::HebrewMetonic | Self::EastAsianLunisolar(_) => true,
            },
            TemporalCalendarMonthCode::M01L
            | TemporalCalendarMonthCode::M02L
            | TemporalCalendarMonthCode::M03L
            | TemporalCalendarMonthCode::M04L
            | TemporalCalendarMonthCode::M06L
            | TemporalCalendarMonthCode::M07L
            | TemporalCalendarMonthCode::M08L
            | TemporalCalendarMonthCode::M09L
            | TemporalCalendarMonthCode::M10L
            | TemporalCalendarMonthCode::M11L
            | TemporalCalendarMonthCode::M12L => match self {
                Self::Twelve | Self::Thirteen | Self::HebrewMetonic => false,
                Self::EastAsianLunisolar(_) => true,
            },
        }
    }

    const fn is_year_sensitive(self) -> bool {
        match self {
            Self::Twelve | Self::Thirteen => false,
            Self::HebrewMetonic | Self::EastAsianLunisolar(_) => true,
        }
    }
}

/// The real projection footer must choose a complete calendar year authority.
#[derive(Clone, Copy)]
pub(crate) enum TemporalCalendarYearLength {
    SolarCommonPlusLeap,
    LunarCommonPlusLeap,
    HebrewNewYearDifference,
    EastAsianNewYearDifference,
}

/// Arithmetic domains projected from the ISO date stored in every carrier.
/// Gregorian offsets change year labels; other domains also change month/day fields.
#[derive(Clone, Copy)]
pub(crate) enum TemporalCalendarArithmetic {
    ProlepticGregorian { year_offset: i64 },
    IndianSolar,
    PersianSolar,
    ThirteenMonthSolar(TemporalThirteenMonthCalendar),
    TabularIslamic(TemporalIslamicCalendar),
    UmmAlQura,
    HebrewLunisolar,
    EastAsianLunisolar(TemporalEastAsianCalendar),
}

impl TemporalCalendarArithmetic {
    pub(crate) const fn month_arithmetic(self) -> TemporalCalendarMonthArithmetic {
        match self {
            Self::ProlepticGregorian { .. }
            | Self::IndianSolar
            | Self::PersianSolar
            | Self::TabularIslamic(_)
            | Self::UmmAlQura => TemporalCalendarMonthArithmetic::Twelve,
            Self::ThirteenMonthSolar(_) => TemporalCalendarMonthArithmetic::Thirteen,
            Self::HebrewLunisolar => TemporalCalendarMonthArithmetic::HebrewMetonic,
            Self::EastAsianLunisolar(kind) => {
                TemporalCalendarMonthArithmetic::EastAsianLunisolar(kind)
            }
        }
    }

    pub(crate) const fn year_length(self) -> TemporalCalendarYearLength {
        match self {
            Self::ProlepticGregorian { .. }
            | Self::IndianSolar
            | Self::PersianSolar
            | Self::ThirteenMonthSolar(_) => TemporalCalendarYearLength::SolarCommonPlusLeap,
            Self::TabularIslamic(_) | Self::UmmAlQura => {
                TemporalCalendarYearLength::LunarCommonPlusLeap
            }
            Self::HebrewLunisolar => TemporalCalendarYearLength::HebrewNewYearDifference,
            Self::EastAsianLunisolar(_) => TemporalCalendarYearLength::EastAsianNewYearDifference,
        }
    }
}

/// One inseparable epoch and reference policy for each thirteen-month calendar.
/// The private arithmetic leaf accepts this closed kind instead of independent
/// raw epoch, era-offset and reference-year arguments.
#[derive(Clone, Copy)]
pub(crate) enum TemporalThirteenMonthCalendar {
    Coptic,
    Ethiopic,
    Ethioaa,
}

impl TemporalThirteenMonthCalendar {
    pub(crate) const fn epoch_day(self) -> i64 {
        match self {
            Self::Coptic => -615_558,
            Self::Ethiopic => -716_367,
            Self::Ethioaa => -2_725_242,
        }
    }

    /// The common calendar year beginning in ISO 1972; later month/day pairs
    /// use the previous year, and M13 day6 uses the preceding leap year.
    pub(crate) const fn reference_year(self) -> i64 {
        match self {
            Self::Coptic => 1689,
            Self::Ethiopic => 1965,
            Self::Ethioaa => 7465,
        }
    }

    pub(crate) const fn leap_reference_year(self) -> i64 {
        self.reference_year() - 2
    }
}

/// Type-II lunar arithmetic with one inseparable epoch/reference policy.
/// The private leaf accepts this kind rather than independent raw constants.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TemporalIslamicCalendar {
    Civil,
    Tbla,
}

impl TemporalIslamicCalendar {
    pub(crate) const fn calendar(self) -> TemporalCalendarId {
        match self {
            Self::Civil => TemporalCalendarId::IslamicCivil,
            Self::Tbla => TemporalCalendarId::IslamicTbla,
        }
    }

    pub(crate) const fn epoch_day(self) -> i64 {
        match self {
            Self::Civil => -492_148,
            Self::Tbla => -492_149,
        }
    }

    /// Calendar year beginning in ISO 1972. Both epochs use the same year;
    /// their exact cutoff date is selected after native ordinal conversion.
    pub(crate) const fn reference_year(self) -> i64 {
        match self {
            Self::Civil | Self::Tbla => 1392,
        }
    }

    /// The latest leap year ending on or before 1972-12-31. Year1391 and
    /// year1392 are common in the specified Type-II thirty-year cycle.
    pub(crate) const fn leap_reference_year(self) -> i64 {
        self.reference_year() - 2
    }
}

/// Era membership shared by Hijri calendars while their arithmetic stays
/// distinct. The real accessor and resolver consume this closed selector;
/// the Type-II arithmetic leaf still accepts only Civil or Tbla.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TemporalIslamicEraCalendar {
    Tabular(TemporalIslamicCalendar),
    UmmAlQura,
}

impl TemporalIslamicEraCalendar {
    pub(crate) const fn calendar(self) -> TemporalCalendarId {
        match self {
            Self::Tabular(kind) => kind.calendar(),
            Self::UmmAlQura => TemporalCalendarId::IslamicUmalqura,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum TemporalCalendarCanonicalizationContext {
    PlainDateFamily,
    ZonedDateTime,
}

impl TemporalCalendarCanonicalizationContext {
    pub(in crate::builtins) const fn type_error_message(self) -> RuntimeErrorMessage {
        match self {
            Self::PlainDateFamily => {
                RuntimeErrorMessage::TEMPORAL_PLAINDATE_CALENDAR_MUST_BE_A_STRING
            }
            Self::ZonedDateTime => {
                RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_CALENDAR_MUST_BE_A_STRING
            }
        }
    }

    pub(in crate::builtins) const fn range_error_message(self) -> RuntimeErrorMessage {
        match self {
            Self::PlainDateFamily => RuntimeErrorMessage::INVALID_TEMPORAL_PLAINDATE_CALENDAR,
            Self::ZonedDateTime => RuntimeErrorMessage::INVALID_TEMPORAL_ZONEDDATETIME_CALENDAR,
        }
    }
}

impl TemporalCalendarId {
    /// Closed native policy key; it is never a String address.
    pub(crate) const fn runtime_code(self) -> i64 {
        self as i64
    }

    pub(crate) const fn arithmetic(self) -> TemporalCalendarArithmetic {
        match self {
            Self::Iso8601 | Self::Gregory | Self::Japanese => {
                TemporalCalendarArithmetic::ProlepticGregorian { year_offset: 0 }
            }
            Self::Buddhist => TemporalCalendarArithmetic::ProlepticGregorian { year_offset: 543 },
            Self::Roc => TemporalCalendarArithmetic::ProlepticGregorian { year_offset: -1911 },
            Self::Indian => TemporalCalendarArithmetic::IndianSolar,
            Self::Persian => TemporalCalendarArithmetic::PersianSolar,
            Self::Coptic => TemporalCalendarArithmetic::ThirteenMonthSolar(
                TemporalThirteenMonthCalendar::Coptic,
            ),
            Self::Ethiopic => TemporalCalendarArithmetic::ThirteenMonthSolar(
                TemporalThirteenMonthCalendar::Ethiopic,
            ),
            Self::Ethioaa => TemporalCalendarArithmetic::ThirteenMonthSolar(
                TemporalThirteenMonthCalendar::Ethioaa,
            ),
            Self::IslamicCivil => {
                TemporalCalendarArithmetic::TabularIslamic(TemporalIslamicCalendar::Civil)
            }
            Self::IslamicTbla => {
                TemporalCalendarArithmetic::TabularIslamic(TemporalIslamicCalendar::Tbla)
            }
            Self::IslamicUmalqura => TemporalCalendarArithmetic::UmmAlQura,
            Self::Hebrew => TemporalCalendarArithmetic::HebrewLunisolar,
            Self::Chinese => {
                TemporalCalendarArithmetic::EastAsianLunisolar(TemporalEastAsianCalendar::Chinese)
            }
            Self::Dangi => {
                TemporalCalendarArithmetic::EastAsianLunisolar(TemporalEastAsianCalendar::Dangi)
            }
        }
    }

    /// The source pool consumes the same closed code authority as resolution.
    pub(crate) fn month_code_spellings(self) -> impl Iterator<Item = &'static str> {
        let policy = self.arithmetic().month_arithmetic();
        TemporalCalendarMonthCode::ALL
            .into_iter()
            .filter(move |code| policy.accepts_code(*code))
            .map(TemporalCalendarMonthCode::spelling)
    }

    /// `ToTemporalCalendarIdentifier(undefined)`.
    pub(crate) const DEFAULT: Self = Self::Iso8601;

    /// The single spelling every `[[Calendar]]` slot stores and every
    /// `calendarId` reports. Canonicalisation happens once, in
    /// [`FunctionBuilder::emit_temporal_canonicalize_calendar`], so no code
    /// downstream of a slot has to case-fold or alias again.
    ///
    /// `Intl.DateTimeFormat` checks the canonical names of calendars it also
    /// supports; calendar arithmetic alone does not provide formatter data.
    pub(crate) const fn canonical(self) -> &'static str {
        match self {
            Self::Iso8601 => "iso8601",
            Self::Gregory => "gregory",
            Self::Buddhist => "buddhist",
            Self::Roc => "roc",
            Self::Japanese => "japanese",
            Self::Indian => "indian",
            Self::Persian => "persian",
            Self::Coptic => "coptic",
            Self::Ethiopic => "ethiopic",
            Self::Ethioaa => "ethioaa",
            Self::IslamicCivil => "islamic-civil",
            Self::IslamicTbla => "islamic-tbla",
            Self::IslamicUmalqura => "islamic-umalqura",
            Self::Hebrew => "hebrew",
            Self::Chinese => "chinese",
            Self::Dangi => "dangi",
        }
    }

    /// Every spelling `CanonicalizeCalendar` accepts, matched
    /// ASCII-case-insensitively. The canonical spelling is always one of them.
    pub(crate) const fn spellings(self) -> &'static [&'static str] {
        match self {
            Self::Iso8601 => &["iso8601"],
            // `gregorian` is the Unicode CLDR alias of `gregory`; CLDR's
            // alias table is normative for `CanonicalizeCalendar`, so both
            // spellings must resolve to the one canonical `gregory`.
            Self::Gregory => &["gregory", "gregorian"],
            Self::Buddhist => &["buddhist"],
            Self::Roc => &["roc"],
            Self::Japanese => &["japanese"],
            Self::Indian => &["indian"],
            Self::Persian => &["persian"],
            Self::Coptic => &["coptic"],
            Self::Ethiopic => &["ethiopic"],
            Self::Ethioaa => &["ethioaa", "ethiopic-amete-alem"],
            Self::IslamicCivil => &["islamic-civil", "islamicc"],
            Self::IslamicTbla => &["islamic-tbla"],
            Self::IslamicUmalqura => &["islamic-umalqura"],
            Self::Hebrew => &["hebrew"],
            Self::Chinese => &["chinese"],
            Self::Dangi => &["dangi"],
        }
    }

    /// Every [`Era`] this calendar recognises, in no observable order.
    ///
    /// Exhaustive with no catch-all, and that is the whole point: era *codes*
    /// are not globally unique, so "is this era valid" is only answerable per
    /// calendar. Test262's `harness/temporalHelpers.js` `CalendarEras` table
    /// shows `japanese` reusing `bce`/`ce` alongside `meiji`..`reiwa`, and
    /// `roc`'s `broc` counting backwards from a different epoch entirely. A
    /// third calendar therefore cannot compile until it states its own set,
    /// and cannot silently inherit `gregory`'s answers.
    ///
    /// An empty set is load-bearing rather than a degenerate case: it is
    /// simultaneously what makes `era`/`eraYear` report `undefined` and what
    /// makes the two property-bag keys go *unread*. Those are the same fact
    /// derived from one predicate, which matters because
    /// `TemporalHelpers.propertyBagObserver` is a Proxy that logs every `get`
    /// — an unconditional `fields.era` read is observable and would break all
    /// 63 `built-ins/Temporal/**/order-of-operations.js` files.
    pub(crate) const fn eras(self) -> &'static [Era] {
        match self {
            Self::Iso8601 | Self::Chinese | Self::Dangi => &[],
            Self::Gregory => Era::GREGORY,
            Self::Buddhist => &[Era::Buddhist],
            Self::Roc => Era::ROC,
            Self::Japanese => Era::JAPANESE,
            Self::Indian => &[Era::Indian],
            Self::Persian => &[Era::Persian],
            Self::Hebrew => &[Era::Hebrew],
            Self::Coptic => &[Era::Coptic],
            Self::Ethiopic => &[
                Era::Ethiopic(EthiopicEra::Am),
                Era::Ethiopic(EthiopicEra::Aa),
            ],
            Self::Ethioaa => &[Era::Ethioaa],
            Self::IslamicCivil => &[
                Era::Islamic(
                    TemporalIslamicEraCalendar::Tabular(TemporalIslamicCalendar::Civil),
                    EraDirection::Forward,
                ),
                Era::Islamic(
                    TemporalIslamicEraCalendar::Tabular(TemporalIslamicCalendar::Civil),
                    EraDirection::Backward,
                ),
            ],
            Self::IslamicTbla => &[
                Era::Islamic(
                    TemporalIslamicEraCalendar::Tabular(TemporalIslamicCalendar::Tbla),
                    EraDirection::Forward,
                ),
                Era::Islamic(
                    TemporalIslamicEraCalendar::Tabular(TemporalIslamicCalendar::Tbla),
                    EraDirection::Backward,
                ),
            ],
            Self::IslamicUmalqura => &[
                Era::Islamic(TemporalIslamicEraCalendar::UmmAlQura, EraDirection::Forward),
                Era::Islamic(
                    TemporalIslamicEraCalendar::UmmAlQura,
                    EraDirection::Backward,
                ),
            ],
        }
    }

    /// What a `Temporal.PlainMonthDay` property bag does with a supplied
    /// `year`. See [`MonthDayYearUse`].
    pub(crate) const fn month_day_year_use(self) -> MonthDayYearUse {
        match self {
            Self::Iso8601 => MonthDayYearUse::OverflowOnly,
            Self::Gregory => MonthDayYearUse::RangeChecked,
            Self::Buddhist => MonthDayYearUse::RangeChecked,
            Self::Roc => MonthDayYearUse::RangeChecked,
            Self::Japanese => MonthDayYearUse::RangeChecked,
            Self::Indian => MonthDayYearUse::RangeChecked,
            Self::Persian => MonthDayYearUse::RangeChecked,
            Self::Coptic
            | Self::Ethiopic
            | Self::Ethioaa
            | Self::IslamicCivil
            | Self::IslamicTbla
            | Self::IslamicUmalqura
            | Self::Hebrew
            | Self::Chinese
            | Self::Dangi => MonthDayYearUse::RangeChecked,
        }
    }
}

impl TemporalEastAsianCalendar {
    pub(crate) const fn calendar(self) -> TemporalCalendarId {
        match self {
            Self::Chinese => TemporalCalendarId::Chinese,
            Self::Dangi => TemporalCalendarId::Dangi,
        }
    }
}

/// Constant-time `&str` equality, because `str::eq` is not `const fn` and the
/// era tables below are checked at compile time.
const fn const_str_eq(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// [`Era::calendar`] must agree with [`TemporalCalendarId::eras`], so an era
/// filed under the wrong calendar is a build failure rather than a wrong
/// `RangeError` for a bag that named the other calendar.
///
/// Only one direction needs asserting because there is only one table.
/// [`TemporalCalendarId::eras`] is the single era list in the crate: the
/// resolver reads it, the accessors read it, and `data.rs` interns its
/// spellings by walking `TemporalCalendarId::ALL -> eras() -> spellings()`. An
/// earlier revision carried a second flat `Era::ALL` list for the string pool
/// and asserted membership both ways; that pair of assertions could not see
/// the case that actually breaks — a new calendar whose `eras()` is complete
/// but which nobody adds to the flat list — because a list that is never
/// consulted is trivially consistent with itself.
const _: () = {
    let mut calendar_index = 0;
    while calendar_index < TemporalCalendarId::ALL.len() {
        let calendar = TemporalCalendarId::ALL[calendar_index];
        let eras = calendar.eras();
        let mut era_index = 0;
        while era_index < eras.len() {
            assert!(
                eras[era_index].calendar() as u8 == calendar as u8,
                "an era listed in TemporalCalendarId::eras must report that calendar"
            );
            era_index += 1;
        }
        calendar_index += 1;
    }
};

/// No spelling may repeat inside one calendar: the resolver takes the first
/// match, so two eras sharing a spelling would make one of them unreachable
/// and the choice between them positional.
const _: () = {
    let mut calendar_index = 0;
    while calendar_index < TemporalCalendarId::ALL.len() {
        let eras = TemporalCalendarId::ALL[calendar_index].eras();
        let mut left = 0;
        while left < eras.len() {
            let left_spellings = eras[left].spellings();
            let mut left_spelling = 0;
            while left_spelling < left_spellings.len() {
                let mut right = 0;
                while right < eras.len() {
                    let right_spellings = eras[right].spellings();
                    let mut right_spelling = 0;
                    while right_spelling < right_spellings.len() {
                        assert!(
                            (left == right && left_spelling == right_spelling)
                                || !const_str_eq(
                                    left_spellings[left_spelling],
                                    right_spellings[right_spelling]
                                ),
                            "an era spelling is repeated inside one calendar"
                        );
                        right_spelling += 1;
                    }
                    right += 1;
                }
                left_spelling += 1;
            }
            left += 1;
        }
        calendar_index += 1;
    }
};

/// The complete PlainDate accessor domain consumed by the native entry match.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TemporalPlainDateField {
    CalendarId,
    Era,
    EraYear,
    Year,
    Month,
    MonthCode,
    Day,
    DayOfWeek,
    DayOfYear,
    WeekOfYear,
    YearOfWeek,
    DaysInWeek,
    DaysInMonth,
    DaysInYear,
    MonthsInYear,
    InLeapYear,
}

/// Which half of the `era`/`eraYear` accessor pair an emitter is producing.
///
/// One emitter serves both, so the pair cannot disagree about where the year-0
/// boundary falls; this names which answer the caller wants out of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TemporalEraField {
    Era,
    EraYear,
}

/// Which way an era's year counts relative to the calendar's arithmetic year.
///
/// Both variants are *involutions*, so one function serves both directions of
/// the conversion between an era year and a calendar arithmetic year. Calendar
/// year offsets are applied separately when crossing the ISO storage boundary.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum EraDirection {
    /// The era year is the ISO year: `ce` 1 is ISO 1.
    Forward,
    /// The era year counts backwards from ISO year 1: proleptic year 0 is
    /// `bce` 1, ISO -1 is `bce` 2.
    Backward,
}

impl EraDirection {
    pub(crate) const ALL: [Self; 2] = [Self::Forward, Self::Backward];

    /// The involution, as a value. `Forward` is the identity; `Backward` is
    /// `y |-> 1 - y`.
    ///
    /// Non-positive era years are *remapped*, never rejected:
    /// `intl402/Temporal/PlainDate/from/era-boundary-gregory.js` pins `ce` 0 to
    /// ISO 0 (reported back as `bce` 1) and `ce` -1 to ISO -1 (`bce` 2).
    pub(crate) const fn convert(self, year: i64) -> i64 {
        match self {
            Self::Forward => year,
            Self::Backward => 1 - year,
        }
    }
}

/// The two properties the era resolver relies on: `convert` is
/// affine (so its two coefficients determine it exactly), and it is an
/// involution (so one function serves both directions).
const _: () = {
    let mut index = 0;
    while index < EraDirection::ALL.len() {
        let direction = EraDirection::ALL[index];
        let constant = direction.convert(0);
        let slope = direction.convert(1) - direction.convert(0);
        let mut year = -4_i64;
        while year <= 4 {
            assert!(
                direction.convert(year) == constant + year * slope,
                "EraDirection::convert must be affine for the era resolver"
            );
            assert!(
                direction.convert(direction.convert(year)) == year,
                "EraDirection::convert must be an involution"
            );
            year += 1;
        }
        index += 1;
    }
};

/// `constant + slope * year_local` on the stack: the one affine year-map
/// emission consumed by the era resolver.
/// Both callers read their coefficients out of their model tables rather than
/// writing them here.
fn emit_affine_year_convert(
    constant: i64,
    slope: i64,
    year_local: I64Local,
    function: &mut Function,
) {
    function.instruction(&Instruction::I64Const(constant));
    year_local.load(function);
    function.instruction(&Instruction::I64Const(slope));
    function.instruction(&Instruction::I64Mul);
    function.instruction(&Instruction::I64Add);
}

/// The two eras of the proleptic Gregorian calendar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum GregoryEra {
    Ce,
    Bce,
}

impl GregoryEra {
    const fn direction(self) -> EraDirection {
        match self {
            Self::Ce => EraDirection::Forward,
            Self::Bce => EraDirection::Backward,
        }
    }

    /// Every spelling `CalendarResolveFields` accepts for this era, canonical
    /// first. `ad`/`bc` are the CLDR aliases of `ce`/`bce`, and
    /// `intl402/Temporal/PlainDate/from/canonicalize-era-codes.js` and its two
    /// siblings pin both.
    const fn spellings(self) -> &'static [&'static str] {
        match self {
            Self::Ce => &["ce", "ad"],
            Self::Bce => &["bce", "bc"],
        }
    }
}

/// The two eras of the ROC (Minguo) calendar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RocEra {
    Roc,
    Broc,
}

impl RocEra {
    const fn direction(self) -> EraDirection {
        match self {
            Self::Roc => EraDirection::Forward,
            Self::Broc => EraDirection::Backward,
        }
    }

    /// Every spelling `CalendarResolveFields` accepts for this era, canonical
    /// first. Neither CLDR nor the `CalendarEras` harness table lists aliases
    /// for these two codes.
    const fn spellings(self) -> &'static [&'static str] {
        match self {
            Self::Roc => &["roc"],
            Self::Broc => &["broc"],
        }
    }
}

/// The eras of the Japanese calendar: five imperial eras plus `ce`/`bce` for
/// dates outside the imperial range.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum JapaneseEra {
    Reiwa,
    Heisei,
    Showa,
    Taisho,
    Meiji,
    Ce,
    Bce,
}

impl JapaneseEra {
    /// Every era, most recent first. The accessor cascades this order: the
    /// first imperial era whose start the date reaches wins, and anything
    /// older than Meiji falls through to `ce`/`bce` by year sign.
    pub(crate) const ALL: [Self; 7] = [
        Self::Reiwa,
        Self::Heisei,
        Self::Showa,
        Self::Taisho,
        Self::Meiji,
        Self::Ce,
        Self::Bce,
    ];

    /// The imperial eras, most recent first: the prefix of [`Self::ALL`] the
    /// accessor's date cascade walks. Pinned to be exactly that prefix by the
    /// `const` assertion below, so an era added to one list but not the other
    /// is a compile error rather than an unreachable era or a cascade that
    /// stops early.
    pub(crate) const IMPERIAL: [Self; 5] = [
        Self::Reiwa,
        Self::Heisei,
        Self::Showa,
        Self::Taisho,
        Self::Meiji,
    ];

    /// Every spelling `CalendarResolveFields` accepts for this era, canonical
    /// first. `ce`/`bce` carry their CLDR aliases; the imperial eras have none
    /// in CLDR or the `CalendarEras` harness table.
    const fn spellings(self) -> &'static [&'static str] {
        match self {
            Self::Reiwa => &["reiwa"],
            Self::Heisei => &["heisei"],
            Self::Showa => &["showa"],
            Self::Taisho => &["taisho"],
            Self::Meiji => &["meiji"],
            Self::Ce => &["ce", "ad"],
            Self::Bce => &["bce", "bc"],
        }
    }

    /// The `(constant, slope)` of the property-bag resolution map
    /// `isoYear = constant + slope * eraYear`, read by both the resolver and
    /// the accessor's era-year arm from this one table.
    ///
    /// Resolution is purely affine in the era year: month and day never adjust
    /// it. `{ era: "reiwa", eraYear: 1, monthCode: "M04", day: 30 }` resolves
    /// to ISO 2019-04-30 and the *accessor* then reports `heisei` 31, and
    /// `{ era: "heisei", eraYear: 31, monthCode: "M05", day: 1 }` resolves to
    /// ISO 2019-05-01, reported as `reiwa` 1
    /// (`era-boundary-japanese.js`). Out-of-range era years remap the same
    /// way: `reiwa` 0 is ISO 2018, `heisei` -1 is ISO 1987.
    ///
    /// Note the deliberate asymmetry for `meiji`: resolution counts from ISO
    /// 1868 (`meiji` 1 is 1868, `meiji` 100 is 1967) while the accessor
    /// reports `ce` for every date before 1873 (`meiji1AfterStart`, `meiji5`,
    /// `japanese-pre-meiji.js`). The two tables disagree on purpose — do not
    /// "fix" one from the other.
    const fn resolve_coefficients(self) -> (i64, i64) {
        match self {
            Self::Reiwa => (2018, 1),
            Self::Heisei => (1988, 1),
            Self::Showa => (1925, 1),
            Self::Taisho => (1911, 1),
            Self::Meiji => (1867, 1),
            Self::Ce => (0, 1),
            Self::Bce => (1, -1),
        }
    }

    /// The first ISO date that reports this era, inclusive. `None` for
    /// `ce`/`bce`, which the accessor selects by year sign instead.
    ///
    /// These are the Test262-pinned Temporal boundaries, not ICU's: `meiji`
    /// starts at 1873-01-01 (`ce1873`, `japanese-pre-meiji.js`), while ICU's
    /// `MEIJI_START` is the historical 1868-10-23. The other four agree with
    /// ICU (`TAISHO_START` 1912-07-30, `SHOWA_START` 1926-12-25,
    /// `HEISEI_START` 1989-01-08, `REIWA_START` 2019-05-01).
    const fn start(self) -> Option<(i64, i64, i64)> {
        match self {
            Self::Reiwa => Some((2019, 5, 1)),
            Self::Heisei => Some((1989, 1, 8)),
            Self::Showa => Some((1926, 12, 25)),
            Self::Taisho => Some((1912, 7, 30)),
            Self::Meiji => Some((1873, 1, 1)),
            Self::Ce | Self::Bce => None,
        }
    }
}

/// `IMPERIAL` is exactly the dated prefix of `ALL`, in the same order.
const _: () = {
    let mut index = 0;
    while index < JapaneseEra::IMPERIAL.len() {
        let era = JapaneseEra::IMPERIAL[index];
        assert!(
            JapaneseEra::ALL[index] as u8 == era as u8,
            "JapaneseEra::IMPERIAL must be the prefix of JapaneseEra::ALL"
        );
        assert!(
            era.start().is_some(),
            "every imperial Japanese era needs an accessor start date"
        );
        assert!(
            era.resolve_coefficients().1 == 1,
            "imperial Japanese era years count forward from the resolve base"
        );
        index += 1;
    }
    let mut rest = JapaneseEra::IMPERIAL.len();
    while rest < JapaneseEra::ALL.len() {
        assert!(
            JapaneseEra::ALL[rest].start().is_none(),
            "only imperial Japanese eras take part in the date cascade"
        );
        rest += 1;
    }
};

/// Monotonic date key: `512 * year + 32 * month + day`.
///
/// Month is 1..=12 and day is 1..=31, so the month/day tail spans less than
/// 512 and the key orders dates exactly like the `(year, month, day)` tuple.
/// The accessor compares one key against the five imperial start keys instead
/// of nesting year/month/day compares per boundary.
const fn japanese_date_key(year: i64, month: i64, day: i64) -> i64 {
    year * 512 + month * 32 + day
}

/// Ethiopian reported eras; only these two can reach its era arm.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum EthiopicEra {
    Am,
    Aa,
}

/// One era of one calendar.
///
/// The calendar is part of the value because era codes are *not* globally
/// unique — `japanese` also has `ce`/`bce`, and `roc`'s `broc` counts backwards
/// on a different epoch — so an era only means something once you know which
/// calendar asked. Wrapping the flat per-calendar enum is what keeps a future
/// calendar from reusing `gregory`'s answers by accident.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Era {
    Gregory(GregoryEra),
    Buddhist,
    Roc(RocEra),
    Japanese(JapaneseEra),
    Indian,
    Persian,
    Coptic,
    Ethiopic(EthiopicEra),
    Ethioaa,
    Islamic(TemporalIslamicEraCalendar, EraDirection),
    Hebrew,
}

impl Era {
    /// The `gregory` era set, as [`TemporalCalendarId::eras`] hands it out.
    pub(crate) const GREGORY: &'static [Self] = &[
        Self::Gregory(GregoryEra::Ce),
        Self::Gregory(GregoryEra::Bce),
    ];

    /// The `roc` era set, as [`TemporalCalendarId::eras`] hands it out.
    pub(crate) const ROC: &'static [Self] = &[Self::Roc(RocEra::Roc), Self::Roc(RocEra::Broc)];

    /// The `japanese` era set, as [`TemporalCalendarId::eras`] hands it out.
    pub(crate) const JAPANESE: &'static [Self] = &[
        Self::Japanese(JapaneseEra::Reiwa),
        Self::Japanese(JapaneseEra::Heisei),
        Self::Japanese(JapaneseEra::Showa),
        Self::Japanese(JapaneseEra::Taisho),
        Self::Japanese(JapaneseEra::Meiji),
        Self::Japanese(JapaneseEra::Ce),
        Self::Japanese(JapaneseEra::Bce),
    ];

    /// The calendar this era belongs to. Pinned against
    /// [`TemporalCalendarId::eras`] by a `const` assertion above.
    pub(crate) const fn calendar(self) -> TemporalCalendarId {
        match self {
            Self::Gregory(_) => TemporalCalendarId::Gregory,
            Self::Buddhist => TemporalCalendarId::Buddhist,
            Self::Roc(_) => TemporalCalendarId::Roc,
            Self::Japanese(_) => TemporalCalendarId::Japanese,
            Self::Indian => TemporalCalendarId::Indian,
            Self::Persian => TemporalCalendarId::Persian,
            Self::Coptic => TemporalCalendarId::Coptic,
            Self::Ethiopic(_) => TemporalCalendarId::Ethiopic,
            Self::Ethioaa => TemporalCalendarId::Ethioaa,
            Self::Islamic(kind, _) => kind.calendar(),
            Self::Hebrew => TemporalCalendarId::Hebrew,
        }
    }

    /// Every accepted spelling, canonical first.
    pub(crate) const fn spellings(self) -> &'static [&'static str] {
        match self {
            Self::Gregory(era) => era.spellings(),
            Self::Buddhist => &["be"],
            Self::Roc(era) => era.spellings(),
            Self::Japanese(era) => era.spellings(),
            Self::Indian => &["shaka"],
            // Temporal uses signed Anno Persico, including zero, not ICU aliases.
            Self::Persian => &["ap"],
            Self::Coptic | Self::Ethiopic(EthiopicEra::Am) | Self::Hebrew => &["am"],
            Self::Ethiopic(EthiopicEra::Aa) | Self::Ethioaa => &["aa"],
            Self::Islamic(_, EraDirection::Forward) => &["ah"],
            Self::Islamic(_, EraDirection::Backward) => &["bh"],
        }
    }

    /// The identifier the `era` accessor reports: the canonical spelling, by
    /// definition rather than by a second table. Adding an alias to
    /// [`Self::spellings`] therefore cannot change what `era` reports, and
    /// cannot fail to be accepted by the resolver or interned by `data.rs`.
    pub(crate) fn code(self) -> &'static str {
        self.spellings()[0]
    }

    /// The `(constant, slope)` of the property-bag resolution map
    /// `calendarYear = constant + slope * eraYear`, the one function the
    /// resolver uses for every era of every calendar.
    ///
    /// The involutive eras read their coefficients out of
    /// [`EraDirection::convert`] rather than restating them, so the resolver
    /// cannot disagree with the accessor arms that share the direction. The
    /// imperial Japanese eras are not involutions — resolution counts from
    /// the era's start year while the accessor subtracts that base back out —
    /// so they state theirs in [`JapaneseEra::resolve_coefficients`], which
    /// the accessor's era-year arm reads as well.
    pub(crate) const fn resolve_coefficients(self) -> (i64, i64) {
        match self {
            Self::Gregory(era) => {
                let direction = era.direction();
                (
                    direction.convert(0),
                    direction.convert(1) - direction.convert(0),
                )
            }
            Self::Buddhist
            | Self::Indian
            | Self::Persian
            | Self::Coptic
            | Self::Ethiopic(EthiopicEra::Am)
            | Self::Ethioaa
            | Self::Hebrew => (0, 1),
            Self::Ethiopic(EthiopicEra::Aa) => (-5500, 1),
            Self::Roc(era) => {
                let direction = era.direction();
                (
                    direction.convert(0),
                    direction.convert(1) - direction.convert(0),
                )
            }
            Self::Japanese(era) => era.resolve_coefficients(),
            Self::Islamic(_, direction) => (
                direction.convert(0),
                direction.convert(1) - direction.convert(0),
            ),
        }
    }
}

// The ID-to-native-kind dispatch and kind-to-ID authority must agree before
// any catalog/model consumer can compile. A copied Chinese/Dangi mapping is a
// build error even if no runtime control reaches that branch.
const _: () = {
    let mut index = 0;
    while index < TemporalCalendarId::ALL.len() {
        let calendar = TemporalCalendarId::ALL[index];
        match calendar.arithmetic() {
            TemporalCalendarArithmetic::EastAsianLunisolar(kind) => {
                assert!(kind.calendar() as u8 == calendar as u8);
            }
            TemporalCalendarArithmetic::ProlepticGregorian { .. }
            | TemporalCalendarArithmetic::IndianSolar
            | TemporalCalendarArithmetic::PersianSolar
            | TemporalCalendarArithmetic::ThirteenMonthSolar(_)
            | TemporalCalendarArithmetic::TabularIslamic(_)
            | TemporalCalendarArithmetic::UmmAlQura
            | TemporalCalendarArithmetic::HebrewLunisolar => {}
        }
        index += 1;
    }
};

// Every closed code's spelling, numeric/leap projections and regular partner
// must agree. Reference Constrain consumes that partner; it cannot acquire a
// new leap code whose publication later panics at a missing regular spelling.
const _: () = {
    let mut index = 0;
    while index < TemporalCalendarMonthCode::ALL.len() {
        let code = TemporalCalendarMonthCode::ALL[index];
        let number = code.month_number();
        let bytes = code.spelling().as_bytes();
        assert!(number >= 1 && number <= 13);
        assert!(bytes.len() == if code.is_leap() { 4 } else { 3 });
        assert!(bytes[0] == b'M');
        assert!(bytes[1] == b'0' + (number / 10) as u8);
        assert!(bytes[2] == b'0' + (number % 10) as u8);
        if code.is_leap() {
            assert!(bytes[3] == b'L');
        }
        let mut regular_found = false;
        let mut candidate_index = 0;
        while candidate_index < TemporalCalendarMonthCode::ALL.len() {
            let candidate = TemporalCalendarMonthCode::ALL[candidate_index];
            if candidate.month_number() == number {
                if !candidate.is_leap() {
                    regular_found = true;
                }
                assert!(candidate_index == index || candidate.is_leap() != code.is_leap());
            }
            candidate_index += 1;
        }
        assert!(regular_found);
        index += 1;
    }
};

/// What a `Temporal.PlainMonthDay` property bag does with a supplied `year`.
///
/// A real fork, not a formality.
/// `built-ins/Temporal/PlainMonthDay/{from,prototype/with}/iso-year-used-only-for-overflow.js`
/// pin that `year: -999999` *succeeds* for `iso8601` — the year only decides
/// how 29 February constrains, and is never stored — while
/// `intl402/Temporal/PlainMonthDay/from/dont-calculate-month-info-for-out-of-range-year.js`
/// pins a RangeError for `gregory`. The predicate is "non-ISO", not "has eras":
/// `chinese` and `dangi` have no eras and still range-check, so a calendar
/// added later must re-decide this rather than inherit the exemption.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MonthDayYearUse {
    /// The year picks the overflow behaviour and is then discarded unchecked.
    OverflowOnly,
    /// Some date in the native year must lie inside the full ISO carrier
    /// before any month information is computed.
    RangeChecked,
}

/// Four reserved-but-unwritten locals for an `era`/`eraYear` pair.
///
/// The first link of a three-step chain that exists so a bag path cannot skip
/// era resolution and silently answer "fields require year":
///
/// 1. [`FunctionBuilder::reserve_temporal_era_slots`] mints this,
/// 2. [`FunctionBuilder::emit_temporal_read_era_fields`] consumes it *by value*
///    and returns [`TemporalEraLocals`],
/// 3. [`FunctionBuilder::emit_temporal_resolve_era_to_calendar_year`] consumes that by
///    value and returns [`TemporalResolvedCalendarYear`],
/// 4. every `*_resolve_fields` emitter takes a [`TemporalResolvedCalendarYear`] instead
///    of a bare `(year, year-present)` pair.
///
/// Nothing else accepts either type and neither is `Copy`, so a step skipped in
/// the middle is a type error at the *next* step, not a wrong answer at run
/// time.
///
/// The owner keeps complete era values and Function-owned scalar slots until
/// the alphabetical sweep resolves them. It is consumed by the next phase;
/// recovered compilation errors discard the entire unfinished Function.
#[must_use]
pub(crate) struct TemporalEraSlots {
    era: ValueLocals,
    era_present: I64Local,
    era_year: I64Local,
    era_year_present: I64Local,
}

#[must_use]
pub(crate) struct TemporalEraLocals {
    era: ValueLocals,
    era_present: I64Local,
    era_year: I64Local,
    era_year_present: I64Local,
}

impl TemporalEraLocals {
    pub(crate) const fn present_locals(&self) -> [I64Local; 2] {
        [self.era_present, self.era_year_present]
    }
}

/// A calendar `(year, year-present)` local pair that has been through
/// [`FunctionBuilder::emit_temporal_resolve_era_to_calendar_year`].
///
/// The fields are private and there is deliberately no second constructor, so
/// the resolver is the only thing in the crate that can mint one. Every
/// `*_resolve_fields` emitter takes this instead of two unchecked scalar slots, which is
/// what makes "read a bag and forgot the era half or calendar conversion"
/// fail to typecheck. An absent year may be filled from the receiver's calendar projection later.
#[must_use]
pub(crate) struct TemporalResolvedCalendarYear {
    calendar_id_local: I64Local,
    year_local: I64Local,
    year_present_local: I64Local,
}

impl TemporalResolvedCalendarYear {
    pub(crate) const fn calendar_id(&self) -> I64Local {
        self.calendar_id_local
    }
    pub(crate) const fn year_local(&self) -> I64Local {
        self.year_local
    }

    pub(crate) const fn year_present_local(&self) -> I64Local {
        self.year_present_local
    }
}

/// Diagnostics and admission at the actual month-field resolution boundaries.
#[derive(Clone, Copy)]
pub(crate) enum TemporalMonthFieldContext {
    PlainDate,
    PlainYearMonth,
    PlainMonthDay,
    PlainMonthDayToPlainDate,
    ZonedDateTime,
}

impl TemporalMonthFieldContext {
    const fn invalid_code(self) -> RuntimeErrorMessage {
        match self {
            Self::PlainDate => RuntimeErrorMessage::INVALID_TEMPORAL_PLAINDATE_MONTHCODE,
            Self::PlainYearMonth => RuntimeErrorMessage::INVALID_TEMPORAL_PLAINYEARMONTH_MONTHCODE,
            Self::PlainMonthDay | Self::PlainMonthDayToPlainDate => {
                RuntimeErrorMessage::INVALID_TEMPORAL_PLAINMONTHDAY_MONTHCODE
            }
            Self::ZonedDateTime => RuntimeErrorMessage::INVALID_TEMPORAL_ZONEDDATETIME_MONTHCODE,
        }
    }

    const fn year_outside_range(self) -> RuntimeErrorMessage {
        match self {
            Self::PlainDate | Self::PlainMonthDayToPlainDate => {
                RuntimeErrorMessage::TEMPORAL_PLAINDATE_IS_OUTSIDE_THE_SUPPORTED_DATE_RANGE
            }
            Self::PlainYearMonth => {
                RuntimeErrorMessage::TEMPORAL_PLAINYEARMONTH_IS_OUTSIDE_THE_SUPPORTED_RANGE
            }
            Self::PlainMonthDay => {
                RuntimeErrorMessage::TEMPORAL_PLAINMONTHDAY_YEAR_IS_OUTSIDE_THE_SUPPORTED_RANGE
            }
            Self::ZonedDateTime => {
                RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_PROPERTY_BAG_YEAR_IS_OUTSIDE_THE_SUPPORTED_INSTANT_RANGE
            }
        }
    }

    const fn disagreement(self) -> RuntimeErrorMessage {
        match self {
            Self::PlainDate => {
                RuntimeErrorMessage::TEMPORAL_PLAINDATE_MONTH_AND_MONTHCODE_MUST_AGREE
            }
            Self::PlainYearMonth => {
                RuntimeErrorMessage::TEMPORAL_PLAINYEARMONTH_MONTH_AND_MONTHCODE_MUST_AGREE
            }
            Self::PlainMonthDay | Self::PlainMonthDayToPlainDate => {
                RuntimeErrorMessage::TEMPORAL_PLAINMONTHDAY_MONTH_AND_MONTHCODE_MUST_AGREE
            }
            Self::ZonedDateTime => {
                RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_MONTH_AND_MONTHCODE_MUST_AGREE
            }
        }
    }
}

/// Calendar-valid original code and constrained supplied-year agreement.
///
/// Only `emit_temporal_resolve_calendar_month` constructs this non-Copy proof.
/// Date/partial conversion consumes it before retiring calendar coordinates.
/// The acquired code slot now contains the canonical encoded ORIGINAL code,
/// including M05L when agreement constrained its ordinal to common-year M06.
/// No extra persistent temporary crosses partial-reference allocation.
#[must_use]
pub(crate) struct TemporalResolvedCalendarMonth {
    resolved_year: TemporalResolvedCalendarYear,
    month_local: I64Local,
    month_code_payload_local: I64Local,
    month_code_present_local: I64Local,
}

impl TemporalResolvedCalendarMonth {
    pub(crate) const fn calendar_id(&self) -> I64Local {
        self.resolved_year.calendar_id()
    }
    pub(crate) const fn year_local(&self) -> I64Local {
        self.resolved_year.year_local()
    }
    pub(crate) const fn year_present_local(&self) -> I64Local {
        self.resolved_year.year_present_local()
    }
    pub(crate) const fn month_local(&self) -> I64Local {
        self.month_local
    }
    pub(crate) const fn month_code_payload_local(&self) -> I64Local {
        self.month_code_payload_local
    }
    pub(crate) const fn month_code_present_local(&self) -> I64Local {
        self.month_code_present_local
    }
    pub(crate) const fn resolved_year(&self) -> &TemporalResolvedCalendarYear {
        &self.resolved_year
    }
}

/// `ISODateToEpochDays` bounds from `ISODateWithinLimits`: noon on the day must
/// stay inside `nsMinInstant - nsPerDay` .. `nsMaxInstant + nsPerDay`, which
/// works out to one more day below the epoch-day limit than above it.
/// `-271821-04-19` and `+275760-09-13` are the exact endpoints Test262's
/// `PlainDate/limits.js` pins.
pub(super) const TEMPORAL_PLAIN_DATE_MINIMUM_EPOCH_DAY: i64 = -100_000_001;
pub(super) const TEMPORAL_PLAIN_DATE_MAXIMUM_EPOCH_DAY: i64 = 100_000_000;

/// The `DifferenceTemporal*` guard messages, as one closed domain.
///
/// `until`/`since` reject a calendar mismatch in all four families, and a
/// time-zone mismatch in `ZonedDateTime`, with a RangeError whose message is a
/// **pool string**: `StringPool::payload` looks the text up in a map built
/// before emission and *panics* — ``string `..` must exist in pool`` — rather
/// than degrading when it was never interned.
///
/// Batch 6 shipped exactly that. [`FunctionBuilder::emit_temporal_require_same_calendar`]
/// took its message as a bare `&str`, the `Temporal.ZonedDateTime` arithmetic
/// lane spelled two new literals at its call site, `data.rs` grew no matching
/// intern entry, and `cargo test -p scriptw-aot-wasm --lib` went **24 red** —
/// 24 rather than 2, because every test that emits a full bootstrap takes the
/// panic whatever that test is about. Nothing in the type system could see it:
/// a `&str` parameter and a runtime map lookup have nothing to disagree about
/// at compile time.
///
/// So a guard message is no longer spellable at a call site. A site names a
/// variant, [`Self::message`] is the only source of the text, and `data.rs`
/// interns by walking [`Self::ALL`] and asking each variant which builtins emit
/// it ([`Self::emitting_builtins`]) — the same shape as the
/// `TemporalCalendarId::ALL -> eras() -> spellings()` walk beside it. Both
/// matches are exhaustive with no `_` arm, so a fifth difference family cannot
/// compile without stating its message and its gate, and the pool then picks it
/// up with no edit in `data.rs` at all.
///
/// **What this does not enforce**, stated so it is not over-read: [`Self::ALL`]
/// is a hand-written array. The const assertion below rejects a duplicate, a
/// reordering, and a variant dropped from the middle, but a variant *appended*
/// to the enum and left off the end of `ALL` still compiles. Keep `ALL`
/// adjacent to the variant list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TemporalDifferenceGuard {
    /// `CalendarEquals` in `DifferenceTemporalPlainDate`.
    PlainDateSameCalendar,
    /// `CalendarEquals` in `DifferenceTemporalPlainDateTime`. Reached by
    /// `Temporal.ZonedDateTime.prototype.{until,since}` too, which delegate
    /// their arithmetic to this body — but through a runtime call, so the
    /// message it throws is this one and the ZonedDateTime guards below fire
    /// first, in the caller.
    PlainDateTimeSameCalendar,
    /// `CalendarEquals` in `DifferenceTemporalPlainYearMonth`.
    PlainYearMonthSameCalendar,
    /// `CalendarEquals` in `DifferenceTemporalZonedDateTime`.
    ZonedDateTimeSameCalendar,
    /// `TimeZoneEquals` in `DifferenceTemporalZonedDateTime`. Deliberately
    /// applied unconditionally rather than only for date `largestUnit`s; the
    /// emitter's own comment carries that choice and its cost.
    ZonedDateTimeSameTimeZone,
}

impl TemporalDifferenceGuard {
    /// The catalog message consumed by the RangeError emitter.
    pub(crate) const fn message(self) -> RuntimeErrorMessage {
        match self {
            Self::PlainDateSameCalendar => {
                RuntimeErrorMessage::TEMPORAL_PLAINDATE_UNTIL_AND_SINCE_REQUIRE_THE_SAME_CALENDAR
            }
            Self::PlainDateTimeSameCalendar => {
                RuntimeErrorMessage::TEMPORAL_PLAINDATETIME_UNTIL_AND_SINCE_REQUIRE_THE_SAME_CALENDAR
            }
            Self::PlainYearMonthSameCalendar => {
                RuntimeErrorMessage::TEMPORAL_PLAINYEARMONTH_UNTIL_AND_SINCE_REQUIRE_THE_SAME_CALENDAR
            }
            Self::ZonedDateTimeSameCalendar => {
                RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_UNTIL_AND_SINCE_REQUIRE_THE_SAME_CALENDAR
            }
            Self::ZonedDateTimeSameTimeZone => {
                RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_UNTIL_AND_SINCE_REQUIRE_THE_SAME_TIME_ZONE
            }
        }
    }
}

impl<'a> FunctionBuilder<'a> {
    /// The retained native year is mandatory for variable-month calendars.
    pub(super) fn emit_temporal_calendar_months_in_year_i64(
        &mut self,
        calendar: I64Local,
        native_year: I64Local,
        function: &mut Function,
    ) {
        let mut branches = 0;
        for id in TemporalCalendarId::ALL {
            let policy = id.arithmetic().month_arithmetic();
            match policy {
                TemporalCalendarMonthArithmetic::Twelve => continue,
                TemporalCalendarMonthArithmetic::Thirteen
                | TemporalCalendarMonthArithmetic::HebrewMetonic
                | TemporalCalendarMonthArithmetic::EastAsianLunisolar(_) => {}
            }
            (calendar).load(function);
            function.instruction(&Instruction::I64Const(id.runtime_code()));
            function.instruction(&Instruction::I64Eq);
            function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
            match policy {
                TemporalCalendarMonthArithmetic::Twelve => {
                    unreachable!("fixed twelve-month branch was excluded")
                }
                TemporalCalendarMonthArithmetic::Thirteen => {
                    function.instruction(&Instruction::I64Const(13));
                }
                TemporalCalendarMonthArithmetic::HebrewMetonic => {
                    let count = self.runtime_schema().reserve_i64_local(function);
                    self.emit_temporal_hebrew_months_in_year(native_year, count, function);
                    (count).load(function);
                    self.runtime_schema().release_i64_local(count, function);
                }
                TemporalCalendarMonthArithmetic::EastAsianLunisolar(kind) => {
                    let count = self.runtime_schema().reserve_i64_local(function);
                    self.emit_temporal_east_asian_months_in_year(
                        kind,
                        native_year,
                        count,
                        function,
                    );
                    (count).load(function);
                    self.runtime_schema().release_i64_local(count, function);
                }
            }
            function.instruction(&Instruction::Else);
            branches += 1;
        }
        function.instruction(&Instruction::I64Const(12));
        for _ in 0..branches {
            function.instruction(&Instruction::End);
        }
    }

    fn emit_temporal_calendar_accepts_month_code_i32(
        &self,
        calendar: I64Local,
        code: TemporalCalendarMonthCode,
        function: &mut Function,
    ) {
        if TemporalCalendarId::ALL
            .into_iter()
            .all(|id| id.arithmetic().month_arithmetic().accepts_code(code))
        {
            function.instruction(&Instruction::I32Const(1));
            return;
        }
        function.instruction(&Instruction::I32Const(0));
        for id in TemporalCalendarId::ALL {
            if id.arithmetic().month_arithmetic().accepts_code(code) {
                (calendar).load(function);
                function.instruction(&Instruction::I64Const(id.runtime_code()));
                function.instruction(&Instruction::I64Eq);
                function.instruction(&Instruction::I32Or);
            }
        }
    }

    /// Caller guards and the complete observable acquisition precede this
    /// factory. Suitability and constrained agreement precede requested overflow.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_temporal_resolve_calendar_month(
        &mut self,
        resolved: TemporalResolvedCalendarYear,
        month: I64Local,
        month_present: I64Local,
        acquired_code: &ValueLocals,
        code_payload: I64Local,
        code_present: I64Local,
        context: TemporalMonthFieldContext,
        function: &mut Function,
    ) -> Result<TemporalResolvedCalendarMonth, EmitError> {
        // An acquired arbitrary integer cannot reach year-dependent month
        // information before the wide arithmetic proof. Final carrier limits
        // remain in the actual date/reference owners after conversion.
        for calendar in TemporalCalendarId::ALL {
            if let TemporalCalendarArithmetic::EastAsianLunisolar(_) = calendar.arithmetic() {
                (resolved.calendar_id()).load(function);
                function.instruction(&Instruction::I64Const(calendar.runtime_code()));
                function.instruction(&Instruction::I64Eq);
                self.open_frame(ControlFrameKind::If, function);
                (resolved.year_local()).load(function);
                function.instruction(&Instruction::I64Const(-TEMPORAL_EAST_ASIAN_YEAR_ENVELOPE));
                function.instruction(&Instruction::I64LtS);
                (resolved.year_local()).load(function);
                function.instruction(&Instruction::I64Const(TEMPORAL_EAST_ASIAN_YEAR_ENVELOPE));
                function.instruction(&Instruction::I64GtS);
                function.instruction(&Instruction::I32Or);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_temporal_error_and_return(
                    scriptw_ir::NativeErrorKind::RangeError,
                    context.year_outside_range(),
                    function,
                )?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        // The sole month-proof constructor owns partial-year admission. A
        // MonthDay decoder cannot mint the proof while forgetting this phase;
        // toPlainDate keeps its separate full-date carrier check.
        match context {
            TemporalMonthFieldContext::PlainMonthDay => {
                self.emit_temporal_month_day_require_year_range(&resolved, function)?;
            }
            TemporalMonthFieldContext::PlainDate
            | TemporalMonthFieldContext::PlainYearMonth
            | TemporalMonthFieldContext::PlainMonthDayToPlainDate
            | TemporalMonthFieldContext::ZonedDateTime => {}
        }
        let from_code = self.runtime_schema().reserve_i64_local(function);
        (code_present).load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (from_code).store(function);
        let acquired = self.runtime_schema().reserve_gc_local(function).initialize(
            acquired_code.cast_reference::<StringValue>(self.runtime_schema(), function),
            function,
        );
        for code in TemporalCalendarMonthCode::ALL {
            let expected = self.runtime_schema().reserve_gc_local(function).initialize(
                self.emit_interned_string_reference(code.spelling(), function)?,
                function,
            );
            self.emit_string_payload_equality_i32(&acquired, &expected, function);
            self.emit_temporal_calendar_accepts_month_code_i32(
                resolved.calendar_id(),
                code,
                function,
            );
            function.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(code.encoding()));
            (from_code).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            expected.clear(function);
        }
        acquired.clear(function);
        (from_code).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            context.invalid_code(),
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // Only now retire the acquired String payload. The encoded original
        // code survives independently of the constrained comparison ordinal.
        (from_code).load(function);
        (code_payload).store(function);
        // Fixed-calendar codes identify an ordinal without a supplied year.
        // ISO MonthDay can carry both month representations without a year,
        // so their disagreement must still reject. Lunisolar calendars defer
        // a yearless ordinal until their code-specific reference-year phase.
        (resolved.year_present_local()).load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Const(0));
        for calendar in TemporalCalendarId::ALL {
            if calendar.arithmetic().month_arithmetic().is_year_sensitive() {
                (resolved.calendar_id()).load(function);
                function.instruction(&Instruction::I64Const(calendar.runtime_code()));
                function.instruction(&Instruction::I64Eq);
                function.instruction(&Instruction::I32Or);
            }
        }
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        (resolved.calendar_id()).load(function);
        function.instruction(&Instruction::I64Const(
            TemporalCalendarId::Hebrew.runtime_code(),
        ));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_hebrew_month_code_ordinal(
            resolved.year_local(),
            code_payload,
            from_code,
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for calendar in TemporalCalendarId::ALL {
            if let TemporalCalendarArithmetic::EastAsianLunisolar(kind) = calendar.arithmetic() {
                (resolved.calendar_id()).load(function);
                function.instruction(&Instruction::I64Const(calendar.runtime_code()));
                function.instruction(&Instruction::I64Eq);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_temporal_east_asian_month_code_ordinal(
                    kind,
                    resolved.year_local(),
                    code_payload,
                    from_code,
                    function,
                );
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        (month_present).load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        (month).load(function);
        (from_code).load(function);
        function.instruction(&Instruction::I64Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            context.disagreement(),
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (from_code).load(function);
        (month).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        (code_payload).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.runtime_schema().release_i64_local(from_code, function);
        Ok(TemporalResolvedCalendarMonth {
            resolved_year: resolved,
            month_local: month,
            month_code_payload_local: code_payload,
            month_code_present_local: code_present,
        })
    }

    /// `ToIntegerWithTruncation`: `ToNumber`, reject NaN and the infinities with
    /// a RangeError, then truncate toward zero.
    pub(crate) fn emit_temporal_to_integer_with_truncation(
        &mut self,
        input: &ValueLocals,
        output: I64Local,
        error_message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        self.emit_value_to_number_payload(input, &pending, function)?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        let number = pending.value().scalar();
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Abs);
        function.instruction(&Instruction::F64Const(Ieee64::from(f64::MAX)));
        function.instruction(&Instruction::F64Gt);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Ne);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            error_message,
            function,
        )?;

        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Trunc);
        function.instruction(&Instruction::I64TruncSatF64S);
        output.store(function);
        pending.clear(function);
        Ok(())
    }

    /// `ToTemporalCalendarIdentifier` step 1: an object that already carries a
    /// `[[Calendar]]` internal slot resolves to *that slot*, without any
    /// observable property access — no `calendar` / `calendarId` getter runs.
    ///
    /// It reads the object's own slot and takes no substitute payload on
    /// purpose. The previous shape accepted an `iso8601` payload and wrote it
    /// back for every matched brand; that was right only while `iso8601` was
    /// the only calendar, and it would have kept compiling — and kept
    /// answering `iso8601` for a `gregory` receiver — once a second calendar
    /// existed. Dropping the parameter is what turns "a caller assumed
    /// `iso8601`" into a build error instead of a wrong `calendarId`.
    ///
    /// Rewrites `calendar_*_local` in place to the `String`-tagged slot when
    /// the fast path applies, and leaves them untouched otherwise.

    /// `CanonicalizeCalendar`, shared by every constructor that takes a
    /// calendar argument.
    ///
    /// `undefined` defaults to [`TemporalCalendarId::DEFAULT`]; an object with
    /// a `[[Calendar]]` slot resolves to that slot; any other non-string is a
    /// TypeError; a string that is no spelling of any [`TemporalCalendarId`] is
    /// a RangeError. The RangeError must stay *after* the TypeError and must
    /// keep firing for `""`, `"notacal"`, `"11111111"`, `"1111-11-11"` and
    /// `"1997-12-04[u-ca=iso8601]"` — the five rows of
    /// `PlainDate/calendar-invalid-iso-string.js` and its seven siblings. This
    /// operation is *not* `ParseTemporalCalendarString`: an ISO date string is
    /// rejected here and accepted by
    /// [`Self::emit_temporal_to_temporal_calendar_identifier`].
    ///
    /// Exactly one place canonicalises, so every `[[Calendar]]` slot in the
    /// heap holds a pooled [`TemporalCalendarId::canonical`] payload and no
    /// reader downstream has to case-fold or resolve an alias again.

    /// `CanonicalizeCalendar` with the `Temporal.PlainDate` family's messages.
    /// Used by the `PlainDate`, `PlainDateTime`, `PlainYearMonth` and
    /// `PlainMonthDay` constructors, which all report the same two.

    /// Leaves an `i32` on the stack: 1 when the calendar payload is
    /// [`TemporalCalendarId::DEFAULT`].
    ///
    /// `FormatCalendarAnnotation` step 2, `TemporalYearMonthToString` step 4
    /// and `TemporalMonthDayToString` step 2 all ask exactly this question, so
    /// they all ask it here.
    pub(crate) fn emit_temporal_calendar_is_default_i32(
        &self,
        calendar_id: I64Local,
        function: &mut Function,
    ) {
        calendar_id.load(function);
        function.instruction(&Instruction::I64Const(
            TemporalCalendarId::DEFAULT.runtime_code(),
        ));
        function.instruction(&Instruction::I64Eq);
    }

    /// `CalendarEquals` as the difference operations use it: `until` and
    /// `since` throw a RangeError when the two receivers name different
    /// calendars, before any option is read.
    ///
    /// With one calendar this could never fire; with two it is the difference
    /// between `PlainDateTime/prototype/{until,since}/different-calendars-throws.js`
    /// passing because the feature works and passing because
    /// `new Temporal.PlainDateTime(..., "gregory")` threw first.
    ///
    /// The message arrives as a [`TemporalDifferenceGuard`] rather than as a
    /// `&str` because it is a pool string and an uninterned pool string is a
    /// compile-time panic in every full bootstrap, not a wrong answer in one
    /// case. That enum's doc carries the incident.
    pub(crate) fn emit_temporal_require_same_calendar(
        &mut self,
        left: I64Local,
        right: I64Local,
        guard: TemporalDifferenceGuard,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        left.load(function);
        right.load(function);
        function.instruction(&Instruction::I64Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            guard.message(),
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// Era output borrows the completed calendar projection. Every era-year
    /// inverse is derived from the same affine table as field input resolution.
    fn emit_temporal_report_era(
        &mut self,
        era: Era,
        year: I64Local,
        field: TemporalEraField,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        match field {
            TemporalEraField::Era => {
                let text = schema.reserve_gc_local(function).initialize(
                    self.emit_interned_string_reference(era.code(), function)?,
                    function,
                );
                output.set_reference(&text, schema, function);
                text.clear(function);
            }
            TemporalEraField::EraYear => {
                let number = schema.reserve_i64_local(function);
                let (constant, slope) = era.resolve_coefficients();
                emit_affine_year_convert(-constant * slope, slope, year, function);
                function.instruction(&Instruction::F64ConvertI64S);
                function.instruction(&Instruction::I64ReinterpretF64);
                number.store(function);
                output.set_number(number, function);
                schema.release_i64_local(number, function);
            }
        }
        Ok(())
    }

    fn emit_temporal_projected_era_field(
        &mut self,
        calendar: I64Local,
        year: I64Local,
        iso: [I64Local; 3],
        field: TemporalEraField,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        output.set_undefined(function);
        for id in TemporalCalendarId::ALL {
            match id {
                TemporalCalendarId::Iso8601
                | TemporalCalendarId::Chinese
                | TemporalCalendarId::Dangi => continue,
                TemporalCalendarId::Gregory
                | TemporalCalendarId::Buddhist
                | TemporalCalendarId::Roc
                | TemporalCalendarId::Japanese
                | TemporalCalendarId::Indian
                | TemporalCalendarId::Persian
                | TemporalCalendarId::Coptic
                | TemporalCalendarId::Ethiopic
                | TemporalCalendarId::Ethioaa
                | TemporalCalendarId::IslamicCivil
                | TemporalCalendarId::IslamicTbla
                | TemporalCalendarId::IslamicUmalqura
                | TemporalCalendarId::Hebrew => {}
            }
            calendar.load(function);
            function.instruction(&Instruction::I64Const(id.runtime_code()));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            match id {
                TemporalCalendarId::Iso8601
                | TemporalCalendarId::Chinese
                | TemporalCalendarId::Dangi => unreachable!("era-free calendar was excluded"),
                TemporalCalendarId::Japanese => {
                    let key = self.runtime_schema().reserve_i64_local(function);
                    iso[0].load(function);
                    function.instruction(&Instruction::I64Const(512));
                    function.instruction(&Instruction::I64Mul);
                    iso[1].load(function);
                    function.instruction(&Instruction::I64Const(32));
                    function.instruction(&Instruction::I64Mul);
                    function.instruction(&Instruction::I64Add);
                    iso[2].load(function);
                    function.instruction(&Instruction::I64Add);
                    key.store(function);
                    for era in JapaneseEra::IMPERIAL {
                        let (y, m, d) = era.start().expect("imperial era has a start");
                        key.load(function);
                        function.instruction(&Instruction::I64Const(japanese_date_key(y, m, d)));
                        function.instruction(&Instruction::I64GeS);
                        self.open_frame(ControlFrameKind::If, function);
                        self.emit_temporal_report_era(
                            Era::Japanese(era),
                            year,
                            field,
                            output,
                            function,
                        )?;
                        function.instruction(&Instruction::Else);
                    }
                    year.load(function);
                    function.instruction(&Instruction::I64Const(0));
                    function.instruction(&Instruction::I64GtS);
                    self.open_frame(ControlFrameKind::If, function);
                    self.emit_temporal_report_era(
                        Era::Japanese(JapaneseEra::Ce),
                        year,
                        field,
                        output,
                        function,
                    )?;
                    function.instruction(&Instruction::Else);
                    self.emit_temporal_report_era(
                        Era::Japanese(JapaneseEra::Bce),
                        year,
                        field,
                        output,
                        function,
                    )?;

                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                    for _ in JapaneseEra::IMPERIAL {
                        self.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                    }
                    self.runtime_schema().release_i64_local(key, function);
                }
                TemporalCalendarId::Gregory
                | TemporalCalendarId::Roc
                | TemporalCalendarId::Ethiopic
                | TemporalCalendarId::IslamicCivil
                | TemporalCalendarId::IslamicTbla
                | TemporalCalendarId::IslamicUmalqura => {
                    let (forward, backward) = match id {
                        TemporalCalendarId::Gregory => {
                            (Era::Gregory(GregoryEra::Ce), Era::Gregory(GregoryEra::Bce))
                        }
                        TemporalCalendarId::Roc => (Era::Roc(RocEra::Roc), Era::Roc(RocEra::Broc)),
                        TemporalCalendarId::Ethiopic => (
                            Era::Ethiopic(EthiopicEra::Am),
                            Era::Ethiopic(EthiopicEra::Aa),
                        ),
                        TemporalCalendarId::IslamicCivil => (
                            Era::Islamic(
                                TemporalIslamicEraCalendar::Tabular(TemporalIslamicCalendar::Civil),
                                EraDirection::Forward,
                            ),
                            Era::Islamic(
                                TemporalIslamicEraCalendar::Tabular(TemporalIslamicCalendar::Civil),
                                EraDirection::Backward,
                            ),
                        ),
                        TemporalCalendarId::IslamicTbla => (
                            Era::Islamic(
                                TemporalIslamicEraCalendar::Tabular(TemporalIslamicCalendar::Tbla),
                                EraDirection::Forward,
                            ),
                            Era::Islamic(
                                TemporalIslamicEraCalendar::Tabular(TemporalIslamicCalendar::Tbla),
                                EraDirection::Backward,
                            ),
                        ),
                        TemporalCalendarId::IslamicUmalqura => (
                            Era::Islamic(
                                TemporalIslamicEraCalendar::UmmAlQura,
                                EraDirection::Forward,
                            ),
                            Era::Islamic(
                                TemporalIslamicEraCalendar::UmmAlQura,
                                EraDirection::Backward,
                            ),
                        ),
                        // This local match shares the already selected sign-bearing policy.
                        _ => unreachable!("calendar has no sign-selected era"),
                    };
                    year.load(function);
                    function.instruction(&Instruction::I64Const(0));
                    function.instruction(&Instruction::I64GtS);
                    self.open_frame(ControlFrameKind::If, function);
                    self.emit_temporal_report_era(forward, year, field, output, function)?;
                    function.instruction(&Instruction::Else);
                    self.emit_temporal_report_era(backward, year, field, output, function)?;

                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                }
                TemporalCalendarId::Buddhist
                | TemporalCalendarId::Indian
                | TemporalCalendarId::Persian
                | TemporalCalendarId::Coptic
                | TemporalCalendarId::Ethioaa
                | TemporalCalendarId::Hebrew => {
                    self.emit_temporal_report_era(id.eras()[0], year, field, output, function)?;
                }
            }

            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        Ok(())
    }

    /// Leaves an `i32` on the stack: 1 when the calendar payload names a
    /// calendar with a non-empty [`TemporalCalendarId::eras`].
    ///
    /// A plain payload compare is enough with no case folding: every
    /// `[[Calendar]]` slot and every property-bag `calendar` value has already
    /// been through [`Self::emit_temporal_canonicalize_calendar`] exactly once,
    /// so only the canonical spelling can reach here.
    pub(crate) fn emit_temporal_calendar_has_eras_i32(
        &self,
        calendar_id: I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I32Const(0));
        for calendar in TemporalCalendarId::ALL {
            if !calendar.eras().is_empty() {
                calendar_id.load(function);
                function.instruction(&Instruction::I64Const(calendar.runtime_code()));
                function.instruction(&Instruction::I64Eq);
                function.instruction(&Instruction::I32Or);
            }
        }
    }

    /// Step 1 of the era chain: four locals, nothing emitted.
    ///
    /// Separate from the read so Function-owned era storage precedes scratch
    /// stack: a `PrepareCalendarFields` sweep reserves its own scratch locals
    /// first and releases them at the end, so an era bag reserved *inside* the
    /// sweep could not outlive it. Callers therefore reserve here, before their
    /// scratch, and read in the middle of the sweep.
    pub(crate) fn reserve_temporal_era_slots(&self, function: &mut Function) -> TemporalEraSlots {
        let schema = self.runtime_schema();
        let slots = TemporalEraSlots {
            era: schema.reserve_value_local(function),
            era_present: schema.reserve_i64_local(function),
            era_year: schema.reserve_i64_local(function),
            era_year_present: schema.reserve_i64_local(function),
        };
        slots.era.set_undefined(function);
        for local in [slots.era_present, slots.era_year, slots.era_year_present] {
            function.instruction(&Instruction::I64Const(0));
            local.store(function);
        }
        slots
    }

    /// Step 2: `PrepareCalendarFields`' `era` and `eraYear` rows, in that
    /// (alphabetical) order, and only for a calendar that has eras.
    ///
    /// The gate is load-bearing rather than an optimisation. The reads
    /// themselves are observable — `TemporalHelpers.propertyBagObserver` is a
    /// Proxy that logs every `get` — so an unconditional `fields.era` breaks
    /// all 63 `built-ins/Temporal/**/order-of-operations.js` files, and
    /// `built-ins/Temporal/PlainDate/prototype/with/time-units-ignored.js`
    /// hands `{ day: 30, era: 'BC' }` to an `iso8601` receiver and requires it
    /// ignored.
    ///
    /// `era` is `ToString`; `eraYear` is `ToIntegerWithTruncation`, which must
    /// accept `0` and negatives (they are remapped, not rejected) and must
    /// RangeError on the infinities after fetching the primitive — the call log
    /// `["get eraYear.valueOf", "call eraYear.valueOf"]` that the 13
    /// `infinity-throws-rangeerror.js` targets assert.
    pub(crate) fn emit_temporal_read_era_fields(
        &mut self,
        slots: TemporalEraSlots,
        input: &ValueLocals,
        calendar_id: I64Local,
        function: &mut Function,
    ) -> Result<TemporalEraLocals, EmitError> {
        let TemporalEraSlots {
            era,
            era_present,
            era_year,
            era_year_present,
        } = slots;
        self.emit_temporal_calendar_has_eras_i32(calendar_id, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_duration_option_get(input, "era", &era, function)?;
        era.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::I64ExtendI32U);
        era_present.store(function);
        self.emit_temporal_property_bag_string(&era, function)?;
        self.emit_temporal_property_bag_integer(
            input,
            "eraYear",
            era_year_present,
            era_year,
            0,
            RuntimeErrorMessage::TEMPORAL_ERAYEAR_MUST_BE_FINITE,
            function,
        )?;

        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(TemporalEraLocals {
            era,
            era_present,
            era_year,
            era_year_present,
        })
    }

    /// Step 3: the `era` half of `CalendarResolveFields`, and the only place in
    /// the backend where any era rule is decided.
    ///
    /// In order:
    ///
    /// * a calendar with no eras does nothing at all — the two locals are still
    ///   zero because the read above was skipped;
    /// * exactly one of the pair present is a **TypeError**
    ///   (`.../from/one-of-era-erayear-undefined.js` and the `with`
    ///   `mutually-exclusive-fields-gregory.js` files), and it must beat every
    ///   RangeError below, which is what
    ///   `PlainDate/prototype/with/calendarresolvefields-error-ordering-gregory.js`
    ///   asserts;
    /// * an era matching no spelling of any era *of this calendar* is a
    ///   **RangeError**, whether or not `year` is also present —
    ///   `PlainDate/from/calendar-invalid-era.js` supplies `year: 2025` beside
    ///   `era: "xyz"` and still wants one, while the three
    ///   `calendar-invalid-era-with-era-year.js` files omit `year` entirely;
    /// * the calendar year is `direction().convert(eraYear)`, and disagreeing
    ///   with an explicit `year` is a **RangeError**
    ///   (`PlainMonthDay/from/fields-overspecified.js`).
    ///
    /// Callers must place this *after* their overflow-option read:
    /// `built-ins/Temporal/PlainDate/from/options-read-before-algorithmic-validation.js`
    /// pins that every option is read and cast before any algorithmic
    /// validation throws.
    pub(crate) fn emit_temporal_resolve_era_to_calendar_year(
        &mut self,
        era: TemporalEraLocals,
        calendar_id: I64Local,
        year: I64Local,
        year_present: I64Local,
        function: &mut Function,
    ) -> Result<TemporalResolvedCalendarYear, EmitError> {
        let schema = self.runtime_schema();
        let TemporalEraLocals {
            era,
            era_present,
            era_year,
            era_year_present,
        } = era;
        let calendar_year = schema.reserve_i64_local(function);
        let matched = schema.reserve_i64_local(function);
        self.emit_temporal_calendar_has_eras_i32(calendar_id, function);
        self.open_frame(ControlFrameKind::If, function);
        era_present.load(function);
        era_year_present.load(function);
        function.instruction(&Instruction::I64Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::TEMPORAL_ERA_AND_ERAYEAR_MUST_BE_PROVIDED_TOGETHER,
            function,
        )?;

        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        era_present.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let era_string = schema.reserve_gc_local(function).initialize(
            era.cast_reference::<StringValue>(schema, function),
            function,
        );
        function.instruction(&Instruction::I64Const(0));
        matched.store(function);
        for calendar in TemporalCalendarId::ALL {
            if calendar.eras().is_empty() {
                continue;
            }
            calendar_id.load(function);
            function.instruction(&Instruction::I64Const(calendar.runtime_code()));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            for &candidate in calendar.eras() {
                for &spelling in candidate.spellings() {
                    self.emit_temporal_string_matches(&era_string, spelling, function)?;
                    self.open_frame(ControlFrameKind::If, function);
                    function.instruction(&Instruction::I64Const(1));
                    matched.store(function);
                    let (constant, slope) = candidate.resolve_coefficients();
                    emit_affine_year_convert(constant, slope, era_year, function);
                    calendar_year.store(function);

                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                }
            }

            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        matched.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::INVALID_TEMPORAL_ERA_FOR_THIS_CALENDAR,
            function,
        )?;

        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        year_present.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        year.load(function);
        calendar_year.load(function);
        function.instruction(&Instruction::I64Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::TEMPORAL_ERA_AND_YEAR_MUST_AGREE,
            function,
        )?;

        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        calendar_year.load(function);
        year.store(function);
        function.instruction(&Instruction::I64Const(1));
        year_present.store(function);
        era_string.clear(function);

        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for local in [
            matched,
            calendar_year,
            era_year_present,
            era_year,
            era_present,
        ] {
            schema.release_i64_local(local, function);
        }
        era.clear(function);
        Ok(TemporalResolvedCalendarYear {
            calendar_id_local: calendar_id,
            year_local: year,
            year_present_local: year_present,
        })
    }

    /// `CalendarMergeFields` for the year slot on the three `with` paths: when
    /// the bag resolved no year of its own, its projected calendar year stands in,
    /// and the merged bag always has one.
    ///
    /// This runs *after* [`Self::emit_temporal_resolve_era_to_calendar_year`] on
    /// purpose. `{ era, eraYear, year }` is one mutually-exclusive group in
    /// `NonISOFieldKeysToIgnore`, so a bag supplying the era pair must exclude
    /// the receiver's year rather than be checked against it — the "era and
    /// eraYear together exclude year" row of the three
    /// `with/mutually-exclusive-fields-gregory.js` files.
    pub(crate) fn emit_temporal_resolved_year_default_to(
        &mut self,
        resolved: &TemporalResolvedCalendarYear,
        receiver_year_local: I64Local,
        function: &mut Function,
    ) {
        (resolved.year_present_local()).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        (receiver_year_local).load(function);
        (resolved.year_local()).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(1));
        (resolved.year_present_local()).store(function);
    }

    /// `ToTemporalCalendarIdentifier` — the property-bag / `withCalendar` form,
    /// which runs `ParseTemporalCalendarString` on a string argument. A string
    /// is first tried as a `TemporalDateString` / `TemporalYearMonthString` /
    /// `TemporalMonthDayString` / `TemporalTimeString`, and only the bare
    /// `AnnotationValue` spelling falls through to `CanonicalizeCalendar`. So
    /// `{ calendar: "2020-01-01" }` resolves to `iso8601` rather than throwing.
    ///
    /// This is NOT the constructor form: `new Temporal.PlainDate(y, m, d, cal)`
    /// calls `CanonicalizeCalendar` directly, so `"1111-11-11"` and
    /// `"11111111"` must stay a RangeError there
    /// (`PlainDate/calendar-invalid-iso-string.js` and its four siblings).
    /// Keep using [`Self::emit_temporal_plain_date_calendar`] for the five
    /// constructors — switching them here is a net regression.
    ///
    /// The prefix (slot fast path, `undefined` default, non-string TypeError)
    /// is identical to `emit_temporal_plain_date_calendar`; only the string
    /// resolution differs, and that is outlined into the shared
    /// `ToTemporalCalendarIdentifier` helper so the ISO parser is inlined once
    /// per module rather than once per call site.

    /// Leaves an `i32` on the stack: 1 when the ISO year is a leap year.
    pub(crate) fn emit_temporal_iso_year_is_leap_i32(
        &mut self,
        year_local: I64Local,
        function: &mut Function,
    ) {
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64RemS);
        function.instruction(&Instruction::I64Eqz);
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(100));
        function.instruction(&Instruction::I64RemS);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(400));
        function.instruction(&Instruction::I64RemS);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Or);
    }

    /// `ISODaysInMonth` into `output_local`.
    pub(crate) fn emit_temporal_iso_days_in_month(
        &mut self,
        year_local: I64Local,
        month_local: I64Local,
        output_local: I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I64Const(31));
        (output_local).store(function);
        for month in [4_i64, 6, 9, 11] {
            (month_local).load(function);
            function.instruction(&Instruction::I64Const(month));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(30));
            (output_local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        (month_local).load(function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_iso_year_is_leap_i32(year_local, function);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(29));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(28));
        function.instruction(&Instruction::End);
        (output_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    /// `ISODateToEpochDays` into `days_local`, reserving and releasing the
    /// scratch locals `emit_temporal_days_from_civil` needs.
    pub(crate) fn emit_temporal_plain_date_epoch_days(
        &mut self,
        year_local: I64Local,
        month_local: I64Local,
        day_local: I64Local,
        days_local: I64Local,
        function: &mut Function,
    ) {
        let adjusted_year_local = self.runtime_schema().reserve_i64_local(function);
        let era_local = self.runtime_schema().reserve_i64_local(function);
        let month_index_local = self.runtime_schema().reserve_i64_local(function);
        self.emit_temporal_days_from_civil(
            year_local,
            month_local,
            day_local,
            adjusted_year_local,
            era_local,
            month_index_local,
            days_local,
            function,
        );
        self.runtime_schema()
            .release_i64_local(month_index_local, function);
        self.runtime_schema().release_i64_local(era_local, function);
        self.runtime_schema()
            .release_i64_local(adjusted_year_local, function);
    }

    /// `ISODateWithinLimits`, on its own, as a RangeError carrying `message`.
    ///
    /// Extracted from [`Self::emit_temporal_reject_iso_date`] rather than
    /// written again because `ToTemporalMonthDay` step (k) applies exactly this
    /// bound to a *parsed* year that the month-day record will never store, and
    /// a second copy of an epoch-day limit is a copy that drifts. It is
    /// deliberately not `ISOYearMonthWithinLimits`: that bound is a pair of year
    /// constants, and it answers wrongly on the two boundary days
    /// `-271821-04-19` and `+275760-09-13`, which are inside the date range and
    /// outside no year.
    ///
    /// The caller owns `days_local`; the shared check does not move the
    /// surrounding field reads, range validation or publication phases.
    pub(crate) fn emit_temporal_iso_date_within_limits(
        &mut self,
        year_local: I64Local,
        month_local: I64Local,
        day_local: I64Local,
        days_local: I64Local,
        message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_plain_date_epoch_days(
            year_local,
            month_local,
            day_local,
            days_local,
            function,
        );
        (days_local).load(function);
        function.instruction(&Instruction::I64Const(
            TEMPORAL_PLAIN_DATE_MINIMUM_EPOCH_DAY,
        ));
        function.instruction(&Instruction::I64LtS);
        (days_local).load(function);
        function.instruction(&Instruction::I64Const(
            TEMPORAL_PLAIN_DATE_MAXIMUM_EPOCH_DAY,
        ));
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            message,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// `RejectISODate` followed by the `ISODateWithinLimits` check that
    /// `CreateTemporalDate` performs. Both failures are RangeErrors, so the
    /// two are fused into one guard.
    ///
    /// The second half is [`Self::emit_temporal_iso_date_within_limits`], which
    /// `ToTemporalMonthDay` step (k) reaches without the `RejectISODate` half.
    pub(crate) fn emit_temporal_reject_iso_date(
        &mut self,
        year_local: I64Local,
        month_local: I64Local,
        day_local: I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let maximum_day_local = self.runtime_schema().reserve_i64_local(function);
        let days_local = self.runtime_schema().reserve_i64_local(function);

        self.emit_temporal_iso_days_in_month(year_local, month_local, maximum_day_local, function);
        (month_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64LtS);
        (month_local).load(function);
        function.instruction(&Instruction::I64Const(12));
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::I32Or);
        (day_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::I32Or);
        (day_local).load(function);
        (maximum_day_local).load(function);
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::TEMPORAL_PLAINDATE_IS_NOT_A_VALID_ISO_DATE,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.emit_temporal_iso_date_within_limits(
            year_local,
            month_local,
            day_local,
            days_local,
            RuntimeErrorMessage::TEMPORAL_PLAINDATE_IS_OUTSIDE_THE_SUPPORTED_DATE_RANGE,
            function,
        )?;

        self.runtime_schema()
            .release_i64_local(days_local, function);
        self.runtime_schema()
            .release_i64_local(maximum_day_local, function);
        Ok(())
    }

    pub(crate) fn emit_alloc_temporal_plain_date(
        &mut self,
        year: I64Local,
        month: I64Local,
        day: I64Local,
        calendar: &TemporalCalendarSlotLocals,
        prototype: TemporalPrototypeSource<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let fields = [year, month, day];
        let schema = self.runtime_schema();
        let scalars: [I32Local; 3] = std::array::from_fn(|index| {
            let local = schema.reserve_i32_local(function);
            fields[index].load(function);
            function.instruction(&Instruction::I32WrapI64);
            local.store(function);
            local
        });
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_temporal_object_header(
                TemporalIntrinsicFamily::PlainDate,
                prototype,
                function,
            )?,
            function,
        );
        let record = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<TemporalPlainDateObject>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::i32_local(scalars[0]),
                    GcOperand::i32_local(scalars[1]),
                    GcOperand::i32_local(scalars[2]),
                    GcOperand::reference(calendar.identifier(), schema),
                ),
                function,
            ),
            function,
        );
        self.completion()
            .value()
            .set_reference(&record, schema, function);
        self.completion()
            .set_normal(self.completion().value(), function);
        record.clear(function);
        header.clear(function);
        for scalar in scalars.into_iter().rev() {
            schema.release_i32_local(scalar, function);
        }
        Ok(())
    }

    /// The `[[InitializedTemporalDate]]` brand check. On failure it throws and
    /// returns, so callers may assume `record_local` is a live record after it.
    pub(crate) fn emit_temporal_plain_date_record_from_receiver(
        &mut self,
        function: &mut Function,
    ) -> Result<GcLocal<TemporalPlainDateObject>, EmitError> {
        self.emit_temporal_record_from_receiver(function)
    }

    pub(crate) fn emit_temporal_plain_date_load_record(
        &self,
        record: &GcLocal<TemporalPlainDateObject>,
        fields: &[I64Local; 3],
        calendar: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let scalar = schema.reserve_i32_local(function);
        for (field, output) in [
            TemporalPlainDateObjectSchema::ISO_YEAR,
            TemporalPlainDateObjectSchema::ISO_MONTH,
            TemporalPlainDateObjectSchema::ISO_DAY,
        ]
        .into_iter()
        .zip(fields)
        {
            schema
                .struct_type::<TemporalPlainDateObject>()
                .field(field)
                .read(record, schema, function)
                .store(scalar, function);
            scalar.load(function);
            function.instruction(&Instruction::I64ExtendI32S);
            output.store(function);
        }
        let identifier = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<TemporalPlainDateObject>()
                .field(TemporalPlainDateObjectSchema::CALENDAR)
                .read(record, schema, function)
                .reference(),
            function,
        );
        calendar.set_reference(&identifier, schema, function);
        identifier.clear(function);
        schema.release_i32_local(scalar, function);
    }

    /// Temporal proposal 3.1: `Temporal.PlainDate(isoYear, isoMonth, isoDay [, calendar])`.
    pub(crate) fn emit_temporal_plain_date_constructor(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_require_construct_call(
            RuntimeErrorMessage::TEMPORAL_PLAINDATE_CONSTRUCTOR_REQUIRES_NEW,
            function,
        )?;
        let schema = self.runtime_schema();
        let input = schema.reserve_value_local(function);
        let fields: [I64Local; 3] = std::array::from_fn(|_| schema.reserve_i64_local(function));
        for (index, message) in [
            RuntimeErrorMessage::TEMPORAL_PLAINDATE_YEAR_MUST_BE_AN_INTEGER,
            RuntimeErrorMessage::TEMPORAL_PLAINDATE_MONTH_MUST_BE_AN_INTEGER,
            RuntimeErrorMessage::TEMPORAL_PLAINDATE_DAY_MUST_BE_AN_INTEGER,
        ]
        .into_iter()
        .enumerate()
        {
            self.emit_builtin_arg_to_value(index, &input, function);
            self.emit_temporal_to_integer_with_truncation(
                &input,
                fields[index],
                message,
                function,
            )?;
        }
        self.emit_builtin_arg_to_value(3, &input, function);
        let calendar = self.emit_temporal_plain_date_calendar(&input, function)?;
        self.emit_temporal_reject_iso_date(fields[0], fields[1], fields[2], function)?;
        let prototype =
            self.emit_temporal_constructor_prototype(TemporalIntrinsicFamily::PlainDate, function)?;
        self.emit_alloc_temporal_plain_date(
            fields[0],
            fields[1],
            fields[2],
            &calendar,
            TemporalPrototypeSource::Constructor(&prototype),
            function,
        )?;
        prototype.release(function);
        calendar.release(self, function);
        input.clear(function);
        for field in fields.into_iter().rev() {
            schema.release_i64_local(field, function);
        }
        Ok(())
    }

    /// Every `Temporal.PlainDate.prototype` accessor. They all start from the
    /// same three ISO fields, so one emitter serves the family the way
    /// `emit_temporal_zoned_date_time_iso_field` does for ZonedDateTime.
    pub(crate) fn emit_temporal_plain_date_field(
        &mut self,
        field: TemporalPlainDateField,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let record = self.emit_temporal_plain_date_record_from_receiver(function)?;
        let fields = std::array::from_fn(|_| schema.reserve_i64_local(function));
        let calendar_value = schema.reserve_value_local(function);
        let output = schema.reserve_value_local(function);
        self.emit_temporal_plain_date_load_record(&record, &fields, &calendar_value, function);
        let identifier = schema.reserve_gc_local(function).initialize(
            calendar_value.cast_reference::<StringValue>(schema, function),
            function,
        );
        let calendar = self.emit_temporal_calendar_slot_from_identifier(&identifier, function)?;
        self.emit_temporal_date_field_value(field, &calendar, fields, &output, function)?;
        self.completion().set_normal(&output, function);
        calendar.release(self, function);
        identifier.clear(function);
        output.clear(function);
        calendar_value.clear(function);
        for local in fields.into_iter().rev() {
            schema.release_i64_local(local, function);
        }
        record.clear(function);
        Ok(())
    }

    /// The purely-numeric accessors. Split out of `emit_temporal_plain_date_field`
    /// so the calendar-arithmetic locals are only reserved when a caller needs
    /// them.
    /// Pure ISO/calendar projection shared by Date, DateTime and partial getters.
    pub(crate) fn emit_temporal_date_field_value(
        &mut self,
        field: TemporalPlainDateField,
        calendar: &TemporalCalendarSlotLocals,
        iso: [I64Local; 3],
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let date = self.emit_temporal_project_calendar_date(calendar.calendar_id(), iso, function);
        match field {
            TemporalPlainDateField::CalendarId => {
                output.set_reference(calendar.identifier(), self.runtime_schema(), function)
            }
            TemporalPlainDateField::Era => self.emit_temporal_projected_era_field(
                calendar.calendar_id(),
                date.year(),
                iso,
                TemporalEraField::Era,
                output,
                function,
            )?,
            TemporalPlainDateField::EraYear => self.emit_temporal_projected_era_field(
                calendar.calendar_id(),
                date.year(),
                iso,
                TemporalEraField::EraYear,
                output,
                function,
            )?,
            TemporalPlainDateField::MonthCode => {
                self.emit_temporal_calendar_month_code_payload(&date, output, function)?
            }
            TemporalPlainDateField::Year => {
                self.emit_temporal_integer_number(date.year(), output, function)
            }
            TemporalPlainDateField::Month => {
                self.emit_temporal_integer_number(date.month(), output, function)
            }
            TemporalPlainDateField::Day => {
                self.emit_temporal_integer_number(date.day(), output, function)
            }
            TemporalPlainDateField::DayOfYear => {
                self.emit_temporal_integer_number(date.day_of_year(), output, function)
            }
            TemporalPlainDateField::DaysInMonth => {
                self.emit_temporal_integer_number(date.days_in_month(), output, function)
            }
            TemporalPlainDateField::DaysInYear => {
                self.emit_temporal_integer_number(date.days_in_year(), output, function)
            }
            TemporalPlainDateField::MonthsInYear => {
                self.emit_temporal_integer_number(date.months_in_year(), output, function)
            }
            TemporalPlainDateField::DaysInWeek => {
                let value = self.runtime_schema().reserve_i64_local(function);
                function.instruction(&Instruction::I64Const(7));
                value.store(function);
                self.emit_temporal_integer_number(value, output, function);
                self.runtime_schema().release_i64_local(value, function);
            }
            TemporalPlainDateField::InLeapYear => {
                let value = self.runtime_schema().reserve_i32_local(function);
                date.leap().load(function);
                function.instruction(&Instruction::I32WrapI64);
                value.store(function);
                output.set_boolean(value, function);
                self.runtime_schema().release_i32_local(value, function);
            }
            TemporalPlainDateField::DayOfWeek => {
                let value = self.runtime_schema().reserve_i64_local(function);
                self.emit_temporal_plain_date_day_of_week(iso[0], iso[1], iso[2], value, function);
                self.emit_temporal_integer_number(value, output, function);
                self.runtime_schema().release_i64_local(value, function);
            }
            TemporalPlainDateField::WeekOfYear | TemporalPlainDateField::YearOfWeek => {
                let week = self.runtime_schema().reserve_i64_local(function);
                let year = self.runtime_schema().reserve_i64_local(function);
                self.emit_temporal_plain_date_iso_week(
                    iso[0], iso[1], iso[2], week, year, function,
                );
                self.emit_temporal_integer_number(
                    match field {
                        TemporalPlainDateField::WeekOfYear => week,
                        TemporalPlainDateField::YearOfWeek => year,
                        _ => unreachable!("week field already selected"),
                    },
                    output,
                    function,
                );
                self.emit_temporal_calendar_week_result(calendar.calendar_id(), output, function);
                self.runtime_schema().release_i64_local(year, function);
                self.runtime_schema().release_i64_local(week, function);
            }
        }
        date.release(self, function);
        Ok(())
    }

    pub(crate) fn emit_temporal_integer_number(
        &self,
        input: I64Local,
        output: &ValueLocals,
        function: &mut Function,
    ) {
        let bits = self.runtime_schema().reserve_i64_local(function);
        input.load(function);
        function.instruction(&Instruction::F64ConvertI64S);
        function.instruction(&Instruction::I64ReinterpretF64);
        bits.store(function);
        output.set_number(bits, function);
        self.runtime_schema().release_i64_local(bits, function);
    }

    /// ISO alone defines a week-numbering system. Called after the ISO numeric
    /// result is written by PlainDate, PlainDateTime and ZonedDateTime.
    pub(crate) fn emit_temporal_calendar_week_result(
        &self,
        calendar: I64Local,
        output: &ValueLocals,
        function: &mut Function,
    ) {
        calendar.load(function);
        function.instruction(&Instruction::I64Const(
            TemporalCalendarId::Iso8601.runtime_code(),
        ));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        output.set_undefined(function);
        function.instruction(&Instruction::End);
    }

    /// ISO weekday, 1 = Monday .. 7 = Sunday. Epoch day 0 is a Thursday, so the
    /// `+3` shift lands Monday on 0 before the floor-mod.
    fn emit_temporal_plain_date_day_of_week(
        &mut self,
        year_local: I64Local,
        month_local: I64Local,
        day_local: I64Local,
        output_local: I64Local,
        function: &mut Function,
    ) {
        let days_local = self.runtime_schema().reserve_i64_local(function);
        self.emit_temporal_plain_date_epoch_days(
            year_local,
            month_local,
            day_local,
            days_local,
            function,
        );
        (days_local).load(function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64Add);
        (days_local).store(function);
        // `I64RemS` truncates toward zero, so a pre-epoch date would yield a
        // negative remainder; the `+7 % 7` restores floor semantics.
        (days_local).load(function);
        function.instruction(&Instruction::I64Const(7));
        function.instruction(&Instruction::I64RemS);
        function.instruction(&Instruction::I64Const(7));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(7));
        function.instruction(&Instruction::I64RemS);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (output_local).store(function);
        self.runtime_schema()
            .release_i64_local(days_local, function);
    }

    pub(super) fn emit_temporal_plain_date_day_of_year(
        &mut self,
        year_local: I64Local,
        month_local: I64Local,
        day_local: I64Local,
        output_local: I64Local,
        function: &mut Function,
    ) {
        let days_local = self.runtime_schema().reserve_i64_local(function);
        let start_local = self.runtime_schema().reserve_i64_local(function);
        let one_local = self.runtime_schema().reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(1));
        (one_local).store(function);
        self.emit_temporal_plain_date_epoch_days(
            year_local,
            month_local,
            day_local,
            days_local,
            function,
        );
        self.emit_temporal_plain_date_epoch_days(
            year_local,
            one_local,
            one_local,
            start_local,
            function,
        );
        (days_local).load(function);
        (start_local).load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (output_local).store(function);
        self.runtime_schema().release_i64_local(one_local, function);
        self.runtime_schema()
            .release_i64_local(start_local, function);
        self.runtime_schema()
            .release_i64_local(days_local, function);
    }

    /// Leaves an `i32` on the stack: 1 when ISO year `year_local` has 53 weeks.
    /// A year is long when 1 January is a Thursday, or when it is a leap year
    /// starting on a Wednesday.
    fn emit_temporal_plain_date_year_is_long_i32(
        &mut self,
        year_local: I64Local,
        function: &mut Function,
    ) {
        let january_first_weekday_local = self.runtime_schema().reserve_i64_local(function);
        let one_local = self.runtime_schema().reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(1));
        (one_local).store(function);
        self.emit_temporal_plain_date_day_of_week(
            year_local,
            one_local,
            one_local,
            january_first_weekday_local,
            function,
        );
        (january_first_weekday_local).load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Eq);
        (january_first_weekday_local).load(function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64Eq);
        self.emit_temporal_iso_year_is_leap_i32(year_local, function);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        self.runtime_schema().release_i64_local(one_local, function);
        self.runtime_schema()
            .release_i64_local(january_first_weekday_local, function);
    }

    /// ISO 8601 week-of-year and its week-numbering year.
    fn emit_temporal_plain_date_iso_week(
        &mut self,
        year_local: I64Local,
        month_local: I64Local,
        day_local: I64Local,
        week_local: I64Local,
        year_of_week_local: I64Local,
        function: &mut Function,
    ) {
        let day_of_year_local = self.runtime_schema().reserve_i64_local(function);
        let day_of_week_local = self.runtime_schema().reserve_i64_local(function);
        let adjacent_year_local = self.runtime_schema().reserve_i64_local(function);
        let weeks_in_year_local = self.runtime_schema().reserve_i64_local(function);

        self.emit_temporal_plain_date_day_of_year(
            year_local,
            month_local,
            day_local,
            day_of_year_local,
            function,
        );
        self.emit_temporal_plain_date_day_of_week(
            year_local,
            month_local,
            day_local,
            day_of_week_local,
            function,
        );
        // `dayOfYear >= 1` and `dayOfWeek <= 7`, so the dividend is at least 4
        // and the truncating `I64DivS` already floors.
        (day_of_year_local).load(function);
        (day_of_week_local).load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(7));
        function.instruction(&Instruction::I64DivS);
        (week_local).store(function);
        (year_local).load(function);
        (year_of_week_local).store(function);

        (week_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, function);
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        (adjacent_year_local).store(function);
        (adjacent_year_local).load(function);
        (year_of_week_local).store(function);
        self.emit_temporal_plain_date_year_is_long_i32(adjacent_year_local, function);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(53));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(52));
        function.instruction(&Instruction::End);
        (week_local).store(function);
        function.instruction(&Instruction::Else);
        self.emit_temporal_plain_date_year_is_long_i32(year_local, function);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(53));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(52));
        function.instruction(&Instruction::End);
        (weeks_in_year_local).store(function);
        (week_local).load(function);
        (weeks_in_year_local).load(function);
        function.instruction(&Instruction::I64GtS);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (week_local).store(function);
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (year_of_week_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.runtime_schema()
            .release_i64_local(weeks_in_year_local, function);
        self.runtime_schema()
            .release_i64_local(adjacent_year_local, function);
        self.runtime_schema()
            .release_i64_local(day_of_week_local, function);
        self.runtime_schema()
            .release_i64_local(day_of_year_local, function);
    }
}
