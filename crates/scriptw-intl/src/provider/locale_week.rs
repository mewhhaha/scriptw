//! Pure pinned Locale week information. JavaScript objects remain in Wasm.

mod profile;
pub(crate) use profile::LocaleWeekProfile;
mod profile_identity;
mod record;

pub use profile_identity::{LOCALE_WEEK_DATA_SHA256, LOCALE_WEEK_KERNEL_SHA256};
pub use record::{IsoWeekday, LocaleWeekInfo, LocaleWeekRequest};

use core::fmt;

use super::language_domain::LocaleCanonicalizationData;
use super::region_preference::{region_preference, RegionPreferenceError};
use crate::{InvalidLocaleId, LocaleTransformError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocaleWeekProfileError {
    Encoding,
    Schema,
    Revision,
    Digest,
    Region,
    RegionOrder,
    WeekInfo,
    MissingWorldDefault,
}

#[derive(Debug)]
pub enum LocaleWeekError {
    UnavailableService(crate::IntlService),
    Profile(LocaleWeekProfileError),
    InvalidLocale(InvalidLocaleId),
    LocaleTransform(LocaleTransformError),
}

impl fmt::Display for LocaleWeekError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnavailableService(service) => crate::UnavailableIntlService(*service).fmt(f),
            Self::Profile(error) => write!(f, "invalid pinned Locale week profile: {error:?}"),
            Self::InvalidLocale(error) => error.fmt(f),
            Self::LocaleTransform(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for LocaleWeekError {}

pub(crate) fn resolve_locale_week(
    request: LocaleWeekRequest,
    profile: &LocaleWeekProfile,
    authority: &LocaleCanonicalizationData,
) -> Result<LocaleWeekInfo, LocaleWeekError> {
    let preference =
        region_preference(request.into_locale(), authority).map_err(|error| match error {
            RegionPreferenceError::InvalidLocale(error) => LocaleWeekError::InvalidLocale(error),
            RegionPreferenceError::LocaleTransform(error) => {
                LocaleWeekError::LocaleTransform(error)
            }
        })?;
    let regional = preference
        .region_override()
        .and_then(|region| profile.get(region))
        .or_else(|| profile.get(preference.region()))
        .unwrap_or_else(|| profile.world_default());
    let first_day = preference
        .unicode_keyword("fw")
        .as_deref()
        .and_then(IsoWeekday::from_unicode_value)
        .unwrap_or(regional.first_day());
    Ok(regional.with_first_day(first_day))
}

#[cfg(test)]
mod tests;
