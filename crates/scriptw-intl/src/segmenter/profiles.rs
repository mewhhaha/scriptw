use super::*;
use crate::segmenter_image::SegmenterImageProvider;
use icu_locale::LanguageIdentifier;
use icu_provider::prelude::*;
use icu_segmenter::options::{SentenceBreakOptions, WordBreakOptions};
use icu_segmenter::provider::{
    SegmenterBreakGraphemeClusterV1, SegmenterBreakSentenceOverrideV1, SegmenterBreakSentenceV1,
    SegmenterBreakWordOverrideV1, SegmenterBreakWordV1, SegmenterDictionaryAutoV1,
    SegmenterLstmAutoV1,
};
use icu_segmenter::{GraphemeClusterSegmenter, SentenceSegmenter, WordSegmenter};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[path = "profile_identity.rs"]
mod identity;
pub use identity::SEGMENTER_DATA_SHA256;
#[cfg(test)]
const SEGMENTER_PROFILE: &[u8] = include_bytes!("profile.json");
const LOCALES: [&str; 17] = [
    "ar",
    "ar-EG",
    "de",
    "el",
    "en",
    "en-US",
    "fi",
    "fr",
    "hi",
    "it",
    "ja",
    "ko",
    "sr",
    "sv",
    "zh",
    "zh-Hans",
    "zh-Hans-CN",
];
const DEFAULT_LOCALE: &str = "en-US";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfile {
    schema_version: u8,
    algorithm_version: String,
    data_version: String,
    unicode_version: String,
    source_manifest_sha256: String,
    default_locale: String,
    locales: Vec<String>,
}

/// A successful load for every required model is recorded before any locale
/// association exists. ICU's optional missing-model path cannot certify this
/// owner. These are exactly the auto constructor's five complex-script inputs.
#[derive(Debug)]
struct DataCertificate {
    grapheme: GraphemeClusterSegmenter,
}
fn certify<P>(provider: &P) -> Result<DataCertificate, SegmenterError>
where
    P: DataProvider<SegmenterBreakGraphemeClusterV1>
        + DataProvider<SegmenterBreakWordV1>
        + DataProvider<SegmenterBreakSentenceV1>
        + DataProvider<SegmenterLstmAutoV1>
        + DataProvider<SegmenterDictionaryAutoV1>
        + DataProvider<SegmenterBreakWordOverrideV1>
        + DataProvider<SegmenterBreakSentenceOverrideV1>,
{
    certify_for_locales(provider, &LOCALES)
}
fn certify_for_locales<P>(provider: &P, locales: &[&str]) -> Result<DataCertificate, SegmenterError>
where
    P: DataProvider<SegmenterBreakGraphemeClusterV1>
        + DataProvider<SegmenterBreakWordV1>
        + DataProvider<SegmenterBreakSentenceV1>
        + DataProvider<SegmenterLstmAutoV1>
        + DataProvider<SegmenterDictionaryAutoV1>
        + DataProvider<SegmenterBreakWordOverrideV1>
        + DataProvider<SegmenterBreakSentenceOverrideV1>,
{
    DataProvider::<SegmenterBreakGraphemeClusterV1>::load(provider, Default::default())
        .map_err(data_error)?;
    DataProvider::<SegmenterBreakWordV1>::load(provider, Default::default()).map_err(data_error)?;
    DataProvider::<SegmenterBreakSentenceV1>::load(provider, Default::default())
        .map_err(data_error)?;
    for attribute in ["Burmese_", "Khmer_", "Lao_", "Thai_"] {
        let req = DataRequest {
            id: DataIdentifierBorrowed::for_marker_attributes(
                DataMarkerAttributes::from_str_or_panic(attribute),
            ),
            metadata: {
                // The pinned ICU constructor requests genuine suffixed LSTM
                // model identifiers through these script prefixes.
                let mut metadata = DataRequestMetadata::default();
                metadata.attributes_prefix_match = true;
                metadata
            },
        };
        DataProvider::<SegmenterLstmAutoV1>::load(provider, req).map_err(data_error)?;
    }
    let req = DataRequest {
        id: DataIdentifierBorrowed::for_marker_attributes(DataMarkerAttributes::from_str_or_panic(
            "cjdict",
        )),
        metadata: Default::default(),
    };
    DataProvider::<SegmenterDictionaryAutoV1>::load(provider, req).map_err(data_error)?;
    // Optional locale word-break overrides (empty in ICU4X 2.3 / CLDR 47+).
    for language in ["fi", "sv"] {
        if !locales.contains(&language) {
            continue;
        }
        let language: LanguageIdentifier = language.parse().map_err(data_error)?;
        let locale = DataLocale::from(&language);
        match DataProvider::<SegmenterBreakWordOverrideV1>::load(
            provider,
            DataRequest {
                id: DataIdentifierBorrowed::for_locale(&locale),
                metadata: Default::default(),
            },
        ) {
            Ok(_) => {}
            Err(err) if err.kind == DataErrorKind::IdentifierNotFound => {}
            Err(err) => return Err(data_error(err)),
        }
    }
    if locales.contains(&"el") {
        let language: LanguageIdentifier = "el".parse().map_err(data_error)?;
        let locale = DataLocale::from(&language);
        DataProvider::<SegmenterBreakSentenceOverrideV1>::load(
            provider,
            DataRequest {
                id: DataIdentifierBorrowed::for_locale(&locale),
                metadata: Default::default(),
            },
        )
        .map_err(data_error)?;
    }
    Ok(DataCertificate {
        grapheme: GraphemeClusterSegmenter::try_new_unstable(provider).map_err(data_error)?,
    })
}

#[derive(Debug)]
pub(super) struct LocaleProfile {
    pub(super) locale: CanonicalLocaleId,
    pub(super) word: WordSegmenter,
    pub(super) sentence: SentenceSegmenter,
    // Keeping the private certificate with every shared profile makes its
    // admission proof survive a dropped outer catalogue owner.
    _certificate: Arc<DataCertificate>,
}
impl LocaleProfile {
    pub(super) fn grapheme(&self) -> &GraphemeClusterSegmenter {
        &self._certificate.grapheme
    }
}
#[derive(Debug, Clone)]
pub struct SegmenterProfiles {
    profiles: Box<[Arc<LocaleProfile>]>,
    // Retain the admitted typed provider and its actual Locale fallback owner.
    _data: Arc<SegmenterImageProvider>,
}
impl SegmenterProfiles {
    pub fn load() -> Result<Self, SegmenterError> {
        crate::segmenter_image::embedded_segmenter_data_image_ref()
            .map(|image| image.profiles_ref().clone())
            .map_err(data_error)
    }
    fn parse_profile(bytes: &[u8]) -> Result<RawProfile, SegmenterError> {
        let raw: RawProfile = serde_json::from_slice(bytes).map_err(data_error)?;
        if raw.schema_version != 1
            || raw.algorithm_version != "2.0.1"
            || raw.data_version != "2.0.0"
            || raw.unicode_version != "16.0.0"
            || raw.source_manifest_sha256 != identity::SEGMENTER_SOURCE_MANIFEST_SHA256
            || raw.default_locale != DEFAULT_LOCALE
            || raw.locales.iter().map(String::as_str).ne(LOCALES)
        {
            return Err(data_error("source identity or exact finite catalogue"));
        }
        Ok(raw)
    }
    pub(crate) fn from_image_data(
        data: Arc<SegmenterImageProvider>,
        descriptor: &[u8],
    ) -> Result<Self, SegmenterError> {
        let raw = Self::parse_profile(descriptor)?;
        if format!("{:x}", Sha256::digest(descriptor)) != SEGMENTER_DATA_SHA256 {
            return Err(data_error("profile digest"));
        }
        // Exact full source validation precedes either complete or projected
        // publication; projected callers cannot supply arbitrary locale rows.
        let certificate = Arc::new(certify(data.as_ref())?);
        let names = raw
            .locales
            .into_iter()
            .map(|name| CanonicalLocaleId::from_data(name).map_err(data_error))
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_names(data, &names, certificate)
    }
    pub(crate) fn from_projection(
        data: Arc<SegmenterImageProvider>,
        catalogue: &crate::segmenter_image::SegmenterCatalogue,
    ) -> Result<Self, SegmenterError> {
        let names = catalogue.public_locales();
        let locales = names
            .iter()
            .map(CanonicalLocaleId::as_str)
            .collect::<Vec<_>>();
        let certificate = Arc::new(certify_for_locales(data.as_ref(), &locales)?);
        Self::from_names(data, names, certificate)
    }
    fn from_names(
        data: Arc<SegmenterImageProvider>,
        names: &[CanonicalLocaleId],
        certificate: Arc<DataCertificate>,
    ) -> Result<Self, SegmenterError> {
        let mut profiles = Vec::new();
        for name in names {
            let name = name.as_str();
            let language: LanguageIdentifier = name.parse().map_err(data_error)?;
            let mut word = WordBreakOptions::default();
            word.content_locale = Some(&language);
            let mut sentence = SentenceBreakOptions::default();
            sentence.content_locale = Some(&language);
            profiles.push(Arc::new(LocaleProfile {
                locale: CanonicalLocaleId::from_data(name.to_owned()).map_err(data_error)?,
                word: WordSegmenter::try_new_auto_unstable(data.as_ref(), word)
                    .map_err(data_error)?,
                sentence: SentenceSegmenter::try_new_unstable(data.as_ref(), sentence)
                    .map_err(data_error)?,
                _certificate: Arc::clone(&certificate),
            }));
        }
        Ok(Self {
            profiles: profiles.into_boxed_slice(),
            _data: data,
        })
    }
    pub(crate) fn segment(
        &self,
        request: &SegmentUtf16Request,
    ) -> Result<SegmenterResult, SegmenterError> {
        let requested = &request.configuration().locale().profile;
        let index = self
            .profiles
            .binary_search_by(|profile| profile.locale.as_str().cmp(requested.locale.as_str()))
            .map_err(|_| SegmenterError::InvalidResolvedLocale)?;
        if !Arc::ptr_eq(&self.profiles[index], requested) {
            return Err(SegmenterError::InvalidResolvedLocale);
        }
        segment_utf16(request)
    }
    pub fn available_locales(&self) -> impl ExactSizeIterator<Item = &CanonicalLocaleId> {
        self.profiles.iter().map(|p| &p.locale)
    }
    pub fn admit(
        &self,
        locale: CanonicalLocaleId,
    ) -> Result<ResolvedSegmenterLocale, SegmenterError> {
        let index = self
            .profiles
            .binary_search_by(|p| p.locale.as_str().cmp(locale.as_str()))
            .map_err(|_| SegmenterError::InvalidResolvedLocale)?;
        Ok(ResolvedSegmenterLocale {
            profile: Arc::clone(&self.profiles[index]),
        })
    }
    fn matching(&self, requested: &str, matcher: LocaleMatcher) -> Option<usize> {
        // ECMA402 permits lookup for best fit. The finite catalogue is the
        // complete service domain; no other valid tag implies data admission.
        match matcher {
            LocaleMatcher::Lookup | LocaleMatcher::BestFit => {}
        }
        let mut candidate = requested;
        loop {
            if let Ok(index) = self
                .profiles
                .binary_search_by(|p| p.locale.as_str().cmp(candidate))
            {
                return Some(index);
            }
            let position = candidate.rfind('-')?;
            candidate = &candidate[..position];
            if candidate
                .rsplit('-')
                .next()
                .is_some_and(|part| part.len() == 1)
            {
                candidate = &candidate[..candidate.rfind('-')?];
            }
        }
    }
    pub fn resolve(
        &self,
        request: &SegmenterLocaleRequest,
    ) -> Result<ResolvedSegmenterLocale, SegmenterError> {
        for locale in &request.requested {
            if let Some(index) = self.matching(locale.as_str(), request.matcher) {
                return Ok(ResolvedSegmenterLocale {
                    profile: Arc::clone(&self.profiles[index]),
                });
            }
        }
        self.admit(CanonicalLocaleId::from_data(DEFAULT_LOCALE).map_err(data_error)?)
    }
    pub fn supported_locales(
        &self,
        request: &SegmenterLocaleRequest,
    ) -> SegmenterSupportedLocalesResult {
        let mut output = Vec::new();
        for locale in &request.requested {
            if self.matching(locale.as_str(), request.matcher).is_some() && !output.contains(locale)
            {
                output.push(locale.clone());
            }
        }
        SegmenterSupportedLocalesResult {
            locales: output.into_boxed_slice(),
        }
    }
}

#[cfg(test)]
mod tests;
