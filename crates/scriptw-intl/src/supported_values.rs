//! Checked primitive enumeration for the currently consumed Intl provider.
//!
//! This is a compiler data seam, not a host operation. JavaScript ToString,
//! errors and fresh called-function-Realm arrays remain in Wasm. The current
//! sixteen-calendar/78-digit authority comes from its admitted kernel owners;
//! localized service selections may retain fewer associations. Currency values
//! are checked against genuine DisplayNames currency names.
//! Unused ICU pools never become public values.

use crate::number_format::options::LocaleMatcher;
use crate::{
    CheckedDisplayNamesConfiguration, DisplayNameRequest, DisplayNamesError, DisplayNamesFallback,
    DisplayNamesLocaleRequest, DisplayNamesSelection, DisplayNamesStyle, RelativeTimeError,
};
use core::fmt;
use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use crate::number_format::{options::SingleUnit, InvalidNumberProfile};
use crate::{
    DateTimeCalendar, DateTimeFormatError, EmbeddedIntlProvider, EmbeddedIntlProviderSetupError,
    IntlDataIdentity, IntlOperationProvider, IntlProvider, InvalidTimeZoneId,
    LookupNamedTimeZoneRequest, NamedTimeZoneLookupError, TimeZoneId,
};

/// The six exact, case-sensitive `Intl.supportedValuesOf` keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedValuesKey {
    Calendar,
    Collation,
    Currency,
    NumberingSystem,
    TimeZone,
    Unit,
}

impl SupportedValuesKey {
    pub(crate) const fn index(self) -> usize {
        self as usize
    }
    pub const ALL: [Self; 6] = [
        Self::Calendar,
        Self::Collation,
        Self::Currency,
        Self::NumberingSystem,
        Self::TimeZone,
        Self::Unit,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Calendar => "calendar",
            Self::Collation => "collation",
            Self::Currency => "currency",
            Self::NumberingSystem => "numberingSystem",
            Self::TimeZone => "timeZone",
            Self::Unit => "unit",
        }
    }
}

/// An immutable, sorted unique list bound to the provider that validated it.
/// No public constructor accepts arbitrary values or a caller-supplied count.
#[derive(Debug)]
pub struct SupportedValuesList {
    values: Box<[Box<str>]>,
    provider_identity: IntlDataIdentity,
}

impl SupportedValuesList {
    /// Called only after immutable producer metadata admission in compilation_data.
    pub(crate) fn from_build_catalogue(
        values: Box<[Box<str>]>,
        provider_identity: IntlDataIdentity,
    ) -> Self {
        Self {
            values,
            provider_identity,
        }
    }
    pub fn values(&self) -> &[Box<str>] {
        &self.values
    }

    pub const fn provider_identity(&self) -> &IntlDataIdentity {
        &self.provider_identity
    }

    fn checked(
        key: SupportedValuesKey,
        values: BTreeSet<Box<str>>,
        provider: &EmbeddedIntlProvider,
    ) -> Result<Self, SupportedValuesSetupError> {
        let identity = provider.identity();
        if values.is_empty() && key != SupportedValuesKey::Collation {
            return Err(SupportedValuesSetupError::InvalidCatalogue(
                "empty available domain",
            ));
        }
        for value in &values {
            let valid = match key {
                SupportedValuesKey::Calendar => DateTimeCalendar::parse(value).is_some(),
                SupportedValuesKey::Collation => provider
                    .collator_profiles()
                    .ok_or(SupportedValuesSetupError::UnavailableData(key))?
                    .available_collations()
                    .binary_search(value)
                    .is_ok(),
                SupportedValuesKey::Currency => {
                    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
                }
                SupportedValuesKey::NumberingSystem => {
                    (3..=8).contains(&value.len())
                        && value.bytes().all(|byte| byte.is_ascii_lowercase())
                }
                SupportedValuesKey::TimeZone => {
                    let identifier = TimeZoneId::parse(value.clone())
                        .map_err(SupportedValuesSetupError::TimeZoneIdentifier)?;
                    let selected = <EmbeddedIntlProvider as IntlOperationProvider<
                        crate::LookupNamedTimeZone,
                    >>::execute(
                        provider, LookupNamedTimeZoneRequest::new(identifier)
                    )
                    .map_err(SupportedValuesSetupError::TimeZoneLookup)?;
                    let resolved = selected.identity();
                    resolved.identifier() == value.as_ref()
                        && resolved.identifier() == resolved.primary_identifier()
                }
                SupportedValuesKey::Unit => SingleUnit::parse(value).is_some(),
            };
            if !valid {
                return Err(SupportedValuesSetupError::InvalidCatalogue(
                    "invalid available identifier",
                ));
            }
        }
        Ok(Self {
            values: values.into_iter().collect(),
            provider_identity: identity.clone(),
        })
    }
}

#[derive(Debug)]
pub enum SupportedValuesSetupError {
    Provider(EmbeddedIntlProviderSetupError),
    NumberProfile(InvalidNumberProfile),
    DateTime(DateTimeFormatError),
    TimeZoneIdentifier(InvalidTimeZoneId),
    TimeZoneLookup(NamedTimeZoneLookupError),
    UnavailableData(SupportedValuesKey),
    InvalidCatalogue(&'static str),
    DisplayNames(DisplayNamesError),
    RelativeTime(RelativeTimeError),
    Segmenter(crate::SegmenterError),
    Duration(crate::DurationError),
}

impl fmt::Display for SupportedValuesSetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(error) => error.fmt(f),
            Self::NumberProfile(error) => error.fmt(f),
            Self::DateTime(error) => error.fmt(f),
            Self::TimeZoneIdentifier(error) => error.fmt(f),
            Self::TimeZoneLookup(error) => error.fmt(f),
            Self::UnavailableData(key) => write!(
                f,
                "Intl supportedValuesOf({:?}) data is unavailable in the selected Custom data",
                key.as_str()
            ),
            Self::InvalidCatalogue(reason) => {
                write!(f, "invalid supported-values catalogue: {reason}")
            }
            Self::DisplayNames(error) => error.fmt(f),
            Self::RelativeTime(error) => error.fmt(f),
            Self::Segmenter(error) => error.fmt(f),
            Self::Duration(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SupportedValuesSetupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Provider(error) => Some(error),
            Self::NumberProfile(error) => Some(error),
            Self::DateTime(error) => Some(error),
            Self::TimeZoneIdentifier(error) => Some(error),
            Self::TimeZoneLookup(error) => Some(error),
            Self::InvalidCatalogue(_) | Self::UnavailableData(_) => None,
            Self::DisplayNames(error) => Some(error),
            Self::RelativeTime(error) => Some(error),
            Self::Segmenter(error) => Some(error),
            Self::Duration(error) => Some(error),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SupportedValuesCatalogue {
    calendar: Arc<SupportedValuesList>,
    collation: Arc<SupportedValuesList>,
    currency: Arc<SupportedValuesList>,
    numbering_system: Arc<SupportedValuesList>,
    time_zone: Arc<SupportedValuesList>,
    unit: Arc<SupportedValuesList>,
}

impl SupportedValuesCatalogue {
    fn list(&self, key: SupportedValuesKey) -> &SupportedValuesList {
        match key {
            SupportedValuesKey::Calendar => &self.calendar,
            SupportedValuesKey::Collation => &self.collation,
            SupportedValuesKey::Currency => &self.currency,
            SupportedValuesKey::NumberingSystem => &self.numbering_system,
            SupportedValuesKey::TimeZone => &self.time_zone,
            SupportedValuesKey::Unit => &self.unit,
        }
    }

    pub(crate) fn from_consumed_provider(
        provider: &EmbeddedIntlProvider,
    ) -> Result<Self, SupportedValuesSetupError> {
        Ok(Self {
            calendar: Arc::new(Self::from_consumed_provider_key(
                provider,
                SupportedValuesKey::Calendar,
            )?),
            collation: Arc::new(Self::from_consumed_provider_key(
                provider,
                SupportedValuesKey::Collation,
            )?),
            currency: Arc::new(Self::from_consumed_provider_key(
                provider,
                SupportedValuesKey::Currency,
            )?),
            numbering_system: Arc::new(Self::from_consumed_provider_key(
                provider,
                SupportedValuesKey::NumberingSystem,
            )?),
            time_zone: Arc::new(Self::from_consumed_provider_key(
                provider,
                SupportedValuesKey::TimeZone,
            )?),
            unit: Arc::new(Self::from_consumed_provider_key(
                provider,
                SupportedValuesKey::Unit,
            )?),
        })
    }

    pub(crate) fn from_consumed_provider_key(
        provider: &EmbeddedIntlProvider,
        key: SupportedValuesKey,
    ) -> Result<SupportedValuesList, SupportedValuesSetupError> {
        let unavailable = || SupportedValuesSetupError::UnavailableData(key);
        let values = match key {
            SupportedValuesKey::Calendar => provider
                .available_calendar_kernels()
                .ok_or_else(unavailable)?
                .into_iter()
                .map(|calendar| calendar.as_str().into())
                .collect(),
            SupportedValuesKey::Collation => provider
                .collator_profiles()
                .ok_or_else(unavailable)?
                .available_collations()
                .iter()
                .cloned()
                .collect(),
            SupportedValuesKey::Currency => {
                let numbers = provider.number_profiles().ok_or_else(unavailable)?;
                let values: BTreeSet<Box<str>> = numbers
                    .reachable_currency_codes()
                    .map(|code| {
                        code.into_iter()
                            .map(char::from)
                            .collect::<String>()
                            .into_boxed_str()
                    })
                    .collect();
                if let Some(displays) = provider.display_names_profiles() {
                    let locale = displays
                        .resolve(&DisplayNamesLocaleRequest {
                            requested: vec![provider.identity().default_locale().clone()]
                                .into_boxed_slice(),
                            matcher: LocaleMatcher::Lookup,
                        })
                        .map_err(SupportedValuesSetupError::DisplayNames)?;
                    let configuration = CheckedDisplayNamesConfiguration::new(
                        locale,
                        DisplayNamesSelection::Currency,
                        DisplayNamesStyle::Long,
                        DisplayNamesFallback::None,
                    );
                    for code in &values {
                        let request = DisplayNameRequest::new(
                            configuration.clone(),
                            code.encode_utf16().collect(),
                        )
                        .map_err(SupportedValuesSetupError::DisplayNames)?;
                        // The configuration owns an actual admitted Display row.
                        // Validation does not invoke an omitted public formatter.
                        let name = crate::display_names::display_name(&request, provider)
                            .map_err(SupportedValuesSetupError::DisplayNames)?;
                        if name.name().is_none() {
                            return Err(SupportedValuesSetupError::InvalidCatalogue(
                                "currency lacks genuine DisplayNames field",
                            ));
                        }
                    }
                }
                values
            }
            SupportedValuesKey::NumberingSystem => {
                let number = provider.number_profiles().map(|profiles| {
                    profiles
                        .numbering_systems()
                        .iter()
                        .cloned()
                        .collect::<BTreeSet<Box<str>>>()
                });
                let datetime = provider
                    .available_numbering_system_kernels()
                    .map(|systems| systems.into_iter().collect::<BTreeSet<Box<str>>>());
                match (number, datetime) {
                    (Some(number), Some(datetime)) => {
                        if number != datetime {
                            return Err(SupportedValuesSetupError::InvalidCatalogue(
                                "Number and DateTime global numbering kernels differ",
                            ));
                        }
                        number
                    }
                    (Some(number), None) => number,
                    (None, Some(datetime)) => datetime,
                    (None, None) => return Err(unavailable()),
                }
            }
            SupportedValuesKey::TimeZone => {
                let mut values = BTreeSet::new();
                for source in provider
                    .named_time_zone_identifiers()
                    .ok_or_else(unavailable)?
                {
                    let identifier = TimeZoneId::parse(source)
                        .map_err(SupportedValuesSetupError::TimeZoneIdentifier)?;
                    let selected = <EmbeddedIntlProvider as IntlOperationProvider<
                        crate::LookupNamedTimeZone,
                    >>::execute(
                        provider, LookupNamedTimeZoneRequest::new(identifier)
                    )
                    .map_err(SupportedValuesSetupError::TimeZoneLookup)?;
                    let resolved = selected.identity();
                    if resolved.identifier() == resolved.primary_identifier() {
                        values.insert(resolved.identifier().into());
                    }
                }
                values
            }
            SupportedValuesKey::Unit => {
                provider.number_profiles().ok_or_else(unavailable)?;
                SingleUnit::ALL
                    .iter()
                    .map(|unit| unit.name().into())
                    .collect()
            }
        };
        SupportedValuesList::checked(key, values, provider)
    }
}

/// Lists remain cached and validated against the actual complete provider.
pub fn embedded_supported_values(
    key: SupportedValuesKey,
) -> Result<&'static SupportedValuesList, &'static SupportedValuesSetupError> {
    static CATALOGUE: OnceLock<Result<SupportedValuesCatalogue, SupportedValuesSetupError>> =
        OnceLock::new();
    CATALOGUE
        .get_or_init(|| {
            let provider =
                EmbeddedIntlProvider::new().map_err(SupportedValuesSetupError::Provider)?;
            SupportedValuesCatalogue::from_consumed_provider(&provider)
        })
        .as_ref()
        .map(|catalogue| catalogue.list(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn values(key: SupportedValuesKey) -> &'static [Box<str>] {
        embedded_supported_values(key)
            .expect("actual consumed provider validates")
            .values()
    }

    #[test]
    fn primary_time_zone_enumeration_keeps_terminal_and_noncontinental_identities() {
        let provider = EmbeddedIntlProvider::new().unwrap();
        let identifiers = provider
            .named_time_zone_identifiers()
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(identifiers.len(), 598);
        for name in ["UTC", "Europe/Paris", "Etc/UTC", "US/Eastern"] {
            assert!(
                identifiers.contains(&name),
                "catalogue lost spelling {name}"
            );
        }
        let catalogue = SupportedValuesCatalogue::from_consumed_provider(&provider).unwrap();
        let zones = catalogue.list(SupportedValuesKey::TimeZone).values();
        for name in [
            "UTC",
            "Europe/Paris",
            "America/New_York",
            "Etc/GMT+1",
            "Etc/GMT-14",
        ] {
            assert!(zones.iter().any(|zone| zone.as_ref() == name), "{name}");
        }
        for alias in ["Etc/UTC", "US/Eastern", "+01:00"] {
            assert!(!zones.iter().any(|zone| zone.as_ref() == alias), "{alias}");
        }
    }

    #[test]
    fn currency_enumeration_uses_reachable_labels_instead_of_code_fallback() {
        let currencies = values(SupportedValuesKey::Currency);
        for code in ["USD", "EUR", "JPY", "ADP"] {
            assert!(
                currencies.iter().any(|value| value.as_ref() == code),
                "{code}"
            );
        }
        assert!(crate::number_format::options::CurrencyCode::parse("ZZZ").is_ok());
        assert!(!currencies.iter().any(|value| value.as_ref() == "ZZZ"));
    }

    #[test]
    fn current_calendar_numbering_and_admitted_collator_domains_are_exact() {
        let provider = EmbeddedIntlProvider::new().unwrap();
        let catalogue = SupportedValuesCatalogue::from_consumed_provider(&provider).unwrap();
        let calendars = catalogue.list(SupportedValuesKey::Calendar).values();
        assert_eq!(
            calendars.iter().map(Box::as_ref).collect::<Vec<_>>(),
            [
                "buddhist",
                "chinese",
                "coptic",
                "dangi",
                "ethioaa",
                "ethiopic",
                "gregory",
                "hebrew",
                "indian",
                "islamic-civil",
                "islamic-tbla",
                "islamic-umalqura",
                "iso8601",
                "japanese",
                "persian",
                "roc",
            ]
        );
        assert_eq!(
            catalogue.list(SupportedValuesKey::Collation).values(),
            provider.collator_profiles().unwrap().available_collations()
        );
        assert!(catalogue
            .list(SupportedValuesKey::Collation)
            .values()
            .iter()
            .any(|co| co.as_ref() == "phonebk"));
        assert!(catalogue
            .list(SupportedValuesKey::Collation)
            .values()
            .iter()
            .all(|co| !["standard", "search", "searchjl"].contains(&co.as_ref())));
        let systems = catalogue.list(SupportedValuesKey::NumberingSystem).values();
        assert!(systems.iter().any(|value| value.as_ref() == "hanidec"));
        assert!(systems.iter().any(|value| value.as_ref() == "tols"));
    }

    #[test]
    fn checked_values_remain_cached_sorted_unique_and_provider_bound() {
        let identity = crate::embedded_intl_data_identity().unwrap();
        for key in SupportedValuesKey::ALL {
            let list = embedded_supported_values(key).unwrap();
            assert!(std::ptr::eq(list, embedded_supported_values(key).unwrap()));
            assert_eq!(list.provider_identity(), &identity);
            assert!(list.values().windows(2).all(|pair| pair[0] < pair[1]));
        }
    }

    #[test]
    fn malformed_source_domains_cannot_mint_enumeration_proofs() {
        let provider = EmbeddedIntlProvider::new().unwrap();
        for (key, source) in [
            (SupportedValuesKey::Currency, "usd"),
            (SupportedValuesKey::Unit, "horsepower"),
            (SupportedValuesKey::Calendar, "gregorian"),
            (SupportedValuesKey::Collation, "searchjl"),
            (SupportedValuesKey::TimeZone, "Etc/UTC"),
            (SupportedValuesKey::TimeZone, "Unknown/Zone"),
            (SupportedValuesKey::TimeZone, "utc"),
        ] {
            assert!(SupportedValuesList::checked(key, [source.into()].into(), &provider).is_err());
        }
        assert!(SupportedValuesList::checked(
            SupportedValuesKey::Collation,
            ["phonebk".into()].into(),
            &provider
        )
        .is_ok());
        assert!(SupportedValuesList::checked(
            SupportedValuesKey::TimeZone,
            ["UTC".into()].into(),
            &provider,
        )
        .is_ok());
        assert!(
            SupportedValuesList::checked(SupportedValuesKey::Unit, BTreeSet::new(), &provider)
                .is_err()
        );
    }
}
