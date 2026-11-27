//! Candidate locale inventory becomes AvailableLocales only after real admission.
use super::*;
use crate::collator_image::CollatorImageProvider;
use icu_collator::options::{AlternateHandling, CaseLevel, CollatorOptions, MaxVariable, Strength};
use icu_collator::preferences::{CollationCaseFirst, CollationNumericOrdering, CollationType};
use icu_collator::provider::*;
use icu_collator::{Collator, CollatorPreferences};
use icu_locale::{extensions::unicode, Locale};
use icu_normalizer::provider::{NormalizerNfdDataV1, NormalizerNfdTablesV1};
use icu_provider::marker::DataMarkerExt;
use icu_provider::prelude::*;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

mod preferences;
use preferences::LocaleCollationPreferences;

// Pinned ICU CollationMetadata bit assignments, independent of wire offsets.
const TAILORED: u32 = 1 << 3;
const TAILORED_DIACRITICS: u32 = 1 << 4;
const REORDERING: u32 = 1 << 5;

/// ECMA-402 BestAvailableLocale removes a trailing singleton with its suffix.
fn truncate_lookup(candidate: &str) -> Option<&str> {
    let shortened = &candidate[..candidate.rfind('-')?];
    if shortened
        .rsplit('-')
        .next()
        .is_some_and(|part| part.len() == 1)
    {
        Some(&shortened[..shortened.rfind('-')?])
    } else {
        Some(shortened)
    }
}

fn data_error(error: impl fmt::Display) -> CollatorOperationError {
    CollatorOperationError::Data(error.to_string().into_boxed_str())
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConsumedCollationLoad {
    marker: &'static str,
    requested: DataIdentifierCow<'static>,
    effective_locale: DataLocale,
    used_fallback: bool,
}
struct AdmissionProvider<'a> {
    data: &'a CollatorImageProvider,
    expected: &'a DataIdentifierCow<'static>,
    bits: u32,
    loads: RefCell<Vec<ConsumedCollationLoad>>,
}
#[derive(Clone, Copy)]
enum LoadDomain {
    Profile,
    Diacritics,
    Singleton,
}
impl AdmissionProvider<'_> {
    fn load<M: DataMarker>(
        &self,
        request: DataRequest,
        domain: LoadDomain,
        marker: &'static str,
    ) -> Result<DataResponse<M>, DataError>
    where
        CollatorImageProvider: DataProvider<M>,
    {
        let profile = request.id == self.expected.as_borrowed();
        let singleton = request.id.locale.is_unknown() && request.id.marker_attributes.is_empty();
        let admitted = match domain {
            LoadDomain::Profile => profile,
            LoadDomain::Diacritics if self.bits & TAILORED_DIACRITICS != 0 => profile,
            LoadDomain::Diacritics | LoadDomain::Singleton => singleton,
        };
        if !admitted {
            return Err(DataError::custom(
                "Collator attempted mismatched profile or missing-profile retry",
            )
            .with_req(M::INFO, request));
        }
        let response = DataProvider::<M>::load(self.data, request)?;
        self.loads.borrow_mut().push(ConsumedCollationLoad {
            marker,
            requested: DataIdentifierCow::from_owned(
                request.id.marker_attributes.to_owned(),
                request.id.locale.clone(),
            ),
            used_fallback: response
                .metadata
                .locale
                .as_ref()
                .is_some_and(|locale| locale != request.id.locale),
            effective_locale: response
                .metadata
                .locale
                .clone()
                .unwrap_or_else(|| request.id.locale.clone()),
        });
        Ok(response)
    }
}

#[cfg(test)]
mod locale_independent_sort_tests {
    use super::*;

    #[test]
    fn lookup_truncation_retains_complete_candidates_and_removes_singletons() {
        for (input, expected) in [
            ("en-US", Some("en")),
            ("zh-Hant-TW", Some("zh-Hant")),
            ("de-DE-u-co", Some("de-DE")),
            ("en-x-private", Some("en")),
            ("en-x-u-data", Some("en-x")),
            ("en", None),
            ("x-private", None),
        ] {
            assert_eq!(truncate_lookup(input), expected, "{input}");
        }
    }

    #[test]
    fn exact_consumed_loads_reject_duplicates_order_and_request_drift() {
        let original = required(CollationType::Emoji);
        let baseline = original.loads.to_vec();
        require_consumed_loads(&baseline, &baseline, false).unwrap();
        let mut duplicate = baseline.clone();
        duplicate.push(baseline[0].clone());
        assert!(require_consumed_loads(&baseline, &duplicate, false).is_err());
        let mut reversed = baseline.clone();
        reversed.swap(0, 1);
        assert!(require_consumed_loads(&baseline, &reversed, false).is_err());
        let mut drift = baseline.clone();
        drift[0].used_fallback = !drift[0].used_fallback;
        assert!(require_consumed_loads(&baseline, &drift, false).is_err());
        let mut drift = baseline.clone();
        drift[0].requested.locale = "en".parse().unwrap();
        assert!(require_consumed_loads(&baseline, &drift, false).is_err());
        let mut drift = baseline.clone();
        drift[0].requested.marker_attributes = std::borrow::Cow::Borrowed(
            icu_provider::DataMarkerAttributes::from_str_or_panic("search"),
        );
        assert!(require_consumed_loads(&baseline, &drift, false).is_err());
        let mut drift = baseline.clone();
        drift[0].effective_locale = "en".parse().unwrap();
        assert!(require_consumed_loads(&baseline, &drift, false).is_err());
        let mut conditional = baseline.clone();
        conditional.push(ConsumedCollationLoad {
            marker: "CollationSpecialPrimariesV1",
            requested: DataIdentifierCow::default(),
            effective_locale: DataLocale::default(),
            used_fallback: false,
        });
        require_consumed_loads(&baseline, &conditional, true).unwrap();
        assert!(require_consumed_loads(&baseline, &conditional, false).is_err());
        conditional.last_mut().unwrap().used_fallback = true;
        assert!(require_consumed_loads(&baseline, &conditional, true).is_err());
    }

    #[test]
    fn all_pinned_metadata_profiles_admit_all_49_required_configurations() {
        let profiles = embedded_collator_profiles().unwrap();
        let mut configurations = 0;
        assert_eq!(profiles.data.metadata_ids().len(), 144);
        for id in profiles.data.metadata_ids() {
            let locale: Locale = id.locale.to_string().parse().unwrap();
            let kind = if id.marker_attributes.is_empty() {
                None
            } else {
                Some(collation_type(id.marker_attributes.as_str()).unwrap())
            };
            let admitted = profile(
                &profiles.data,
                &locale,
                kind,
                kind.map(|value| value.as_str().into()),
            )
            .unwrap()
            .unwrap();
            configurations += 1; // profile() executed the actual default constructor.
            for numeric in [false, true] {
                for case_first in [
                    CollatorCaseFirst::False,
                    CollatorCaseFirst::Lower,
                    CollatorCaseFirst::Upper,
                ] {
                    for sensitivity in CollatorSensitivity::ALL {
                        for ignore in [false, true] {
                            admitted
                                .construct(numeric, case_first, *sensitivity, ignore)
                                .unwrap();
                            configurations += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(configurations, 7_056);
    }

    fn required(kind: CollationType) -> Arc<CollationProfile> {
        embedded_collator_profiles()
            .unwrap()
            .locale_independent
            .0
            .iter()
            .find(|profile| profile.kind == Some(kind))
            .cloned()
            .unwrap()
    }

    #[test]
    fn both_real_root_profiles_are_required_before_preferences_publish() {
        assert!(LocaleIndependentSortPreferences::checked(
            None,
            Some(required(CollationType::Eor)),
        )
        .is_err());
        assert!(LocaleIndependentSortPreferences::checked(
            Some(required(CollationType::Emoji)),
            None,
        )
        .is_err());
    }

    #[test]
    fn duplicate_or_reversed_root_kinds_cannot_mint_preferences() {
        assert!(LocaleIndependentSortPreferences::checked(
            Some(required(CollationType::Emoji)),
            Some(required(CollationType::Emoji)),
        )
        .is_err());
        assert!(LocaleIndependentSortPreferences::checked(
            Some(required(CollationType::Eor)),
            Some(required(CollationType::Emoji)),
        )
        .is_err());
    }

    #[test]
    fn missing_consumed_metadata_or_wrong_public_name_rejects_root_owner() {
        for bad_name in [false, true] {
            let original = required(CollationType::Emoji);
            let corrupt = Arc::new(CollationProfile {
                data: original.data.clone(),
                id: original.id.clone(),
                locale: original.locale.clone(),
                kind: original.kind,
                public_collation: if bad_name {
                    Some("search".into())
                } else {
                    original.public_collation.clone()
                },
                bits: original.bits,
                case_first: original.case_first,
                ignore: original.ignore,
                loads: if bad_name {
                    original.loads.clone()
                } else {
                    Box::new([])
                },
            });
            assert!(LocaleIndependentSortPreferences::checked(
                Some(corrupt),
                Some(required(CollationType::Eor)),
            )
            .is_err());
        }
    }

    #[test]
    fn unmatched_query_consumes_root_owner_without_mutating_formatter_catalogues() {
        let profiles = embedded_collator_profiles().unwrap();
        let owner_names: Vec<_> = profiles
            .locale_independent
            .0
            .iter()
            .map(|profile| profile.public_collation().unwrap())
            .collect();
        assert_eq!(owner_names, ["emoji", "eor"]);
        assert!(profiles.locale_independent.0.iter().all(|profile| {
            profile.id.locale.is_unknown()
                && profile.loads.iter().any(|load| {
                    load.marker == "CollationMetadataV1" && load.effective_locale.is_unknown()
                })
        }));
        let locale_names: Vec<_> = profiles.available_locales().cloned().collect();
        let global = profiles.available_collations().to_vec();
        let result = profiles
            .locale_sort_collations(&CanonicalLocaleId::from_data("qaa-US").unwrap())
            .unwrap();
        assert_eq!(
            result.iter().map(|name| name.as_ref()).collect::<Vec<_>>(),
            owner_names
        );
        assert_eq!(
            profiles.available_locales().cloned().collect::<Vec<_>>(),
            locale_names
        );
        assert_eq!(profiles.available_collations(), global.as_slice());
    }
}
macro_rules! admit_marker {
    ($marker:ident, $domain:expr) => {
        impl DataProvider<$marker> for AdmissionProvider<'_> {
            fn load(&self, request: DataRequest) -> Result<DataResponse<$marker>, DataError> {
                self.load::<$marker>(request, $domain, stringify!($marker))
            }
        }
    };
}
macro_rules! admission_markers {
    ($( $variant:ident, $marker:ident, $wire:literal, $domain:ident; )*) => { $( admit_marker!($marker, LoadDomain::$domain); )* };
}
crate::collator_markers::for_each!(admission_markers);

fn purpose(kind: CollationType) -> Result<bool, CollatorOperationError> {
    Ok(match kind {
        // The pinned keyword documentation identifies these as search purposes.
        CollationType::Search | CollationType::Searchjl | CollationType::Standard => false,
        CollationType::Compat
        | CollationType::Dict
        | CollationType::Ducet
        | CollationType::Emoji
        | CollationType::Eor
        | CollationType::Phonebk
        | CollationType::Phonetic
        | CollationType::Pinyin
        | CollationType::Stroke
        | CollationType::Trad
        | CollationType::Unihan
        | CollationType::Zhuyin => true,
        _ => return Err(data_error("unknown pinned CollationType purpose")),
    })
}
fn collation_type(value: &str) -> Result<CollationType, CollatorOperationError> {
    let value = unicode::Value::try_from_str(value).map_err(data_error)?;
    CollationType::try_from(&value).map_err(data_error)
}
fn defaults_case_first(
    value: CollationCaseFirst,
) -> Result<CollatorCaseFirst, CollatorOperationError> {
    Ok(match value {
        CollationCaseFirst::False => CollatorCaseFirst::False,
        CollationCaseFirst::Lower => CollatorCaseFirst::Lower,
        CollationCaseFirst::Upper => CollatorCaseFirst::Upper,
        _ => return Err(data_error("unknown pinned caseFirst default")),
    })
}
#[derive(Debug)]
pub(super) struct CollationProfile {
    data: Arc<CollatorImageProvider>,
    id: DataIdentifierCow<'static>,
    locale: Locale,
    kind: Option<CollationType>,
    public_collation: Option<Box<str>>,
    bits: u32,
    case_first: CollatorCaseFirst,
    ignore: bool,
    loads: Box<[ConsumedCollationLoad]>,
}
impl CollationProfile {
    pub(super) fn public_collation(&self) -> Option<&str> {
        self.public_collation.as_deref()
    }
    pub(super) fn default_ignore_punctuation(&self) -> bool {
        self.ignore
    }
    fn preferences(&self, numeric: bool, case_first: CollatorCaseFirst) -> CollatorPreferences {
        let mut prefs = CollatorPreferences::default();
        // Retain the parsed admitted DataLocale, including its ICU transform.
        prefs.locale_preferences = (&self.locale).into();
        prefs.collation_type = self.kind;
        prefs.numeric_ordering = Some(if numeric {
            CollationNumericOrdering::True
        } else {
            CollationNumericOrdering::False
        });
        prefs.case_first = Some(match case_first {
            CollatorCaseFirst::False => CollationCaseFirst::False,
            CollatorCaseFirst::Lower => CollationCaseFirst::Lower,
            CollatorCaseFirst::Upper => CollationCaseFirst::Upper,
        });
        prefs
    }
    pub(super) fn construct(
        &self,
        numeric: bool,
        case_first: CollatorCaseFirst,
        sensitivity: CollatorSensitivity,
        ignore: bool,
    ) -> Result<Collator, CollatorOperationError> {
        let (strength, case_level) = match sensitivity {
            CollatorSensitivity::Base => (Strength::Primary, CaseLevel::Off),
            CollatorSensitivity::Accent => (Strength::Secondary, CaseLevel::Off),
            CollatorSensitivity::Case => (Strength::Primary, CaseLevel::On),
            CollatorSensitivity::Variant => (Strength::Tertiary, CaseLevel::Off),
        };
        let mut options = CollatorOptions::default();
        options.strength = Some(strength);
        options.case_level = Some(case_level);
        options.alternate_handling = Some(if ignore {
            AlternateHandling::Shifted
        } else {
            AlternateHandling::NonIgnorable
        });
        options.max_variable = Some(MaxVariable::Punctuation);
        let provider = AdmissionProvider {
            data: &self.data,
            expected: &self.id,
            bits: self.bits,
            loads: RefCell::new(Vec::new()),
        };
        let collator =
            Collator::try_new_unstable(&provider, self.preferences(numeric, case_first), options)
                .map_err(data_error)?;
        require_consumed_loads(&self.loads, &provider.loads.into_inner(), numeric || ignore)?;
        Ok(collator)
    }
}
fn require_consumed_loads(
    baseline: &[ConsumedCollationLoad],
    actual: &[ConsumedCollationLoad],
    _special: bool,
) -> Result<(), CollatorOperationError> {
    if actual != baseline {
        return Err(data_error(
            "Collator consumed load order or profile association drift",
        ));
    }
    Ok(())
}

fn require_baseline_loads(
    loads: &[ConsumedCollationLoad],
    bits: u32,
    special: bool,
) -> Result<(), CollatorOperationError> {
    let mut markers = vec![
        "CollationRootV1",
        "NormalizerNfdDataV1",
        "NormalizerNfdTablesV1",
        "CollationJamoV1",
        "CollationSpecialPrimariesV1",
        "CollationMetadataV1",
    ];
    if bits & TAILORED != 0 {
        markers.push("CollationTailoringV1");
    }
    if bits & REORDERING != 0 {
        markers.push("CollationReorderingV1");
    }
    markers.push("CollationDiacriticsV1");
    let baseline = loads
        .get(..markers.len())
        .ok_or_else(|| data_error("incomplete Collator baseline load inventory"))?;
    if !baseline.iter().map(|load| load.marker).eq(markers) {
        return Err(data_error("Collator baseline marker order drift"));
    }
    let special_load = &baseline[4];
    if !special_load.requested.locale.is_unknown()
        || !special_load.requested.marker_attributes.is_empty()
        || !special_load.effective_locale.is_unknown()
        || special_load.used_fallback
    {
        return Err(data_error("Collator conditional singleton load drift"));
    }
    require_consumed_loads(baseline, loads, special)
}

#[derive(Debug)]
struct LocaleProfile {
    locale: CanonicalLocaleId,
    sort: Arc<CollationProfile>,
    search: Arc<CollationProfile>,
    collations: BTreeMap<Box<str>, Arc<CollationProfile>>,
}
/// The prescribed unmatched pair is publishable only with both real root profiles.
#[derive(Debug)]
struct LocaleIndependentSortPreferences([Arc<CollationProfile>; 2]);
impl LocaleIndependentSortPreferences {
    fn checked(
        emoji: Option<Arc<CollationProfile>>,
        eor: Option<Arc<CollationProfile>>,
    ) -> Result<Self, CollatorOperationError> {
        let profiles = [
            emoji.ok_or_else(|| data_error("missing locale-independent emoji profile"))?,
            eor.ok_or_else(|| data_error("missing locale-independent eor profile"))?,
        ];
        for (profile, expected) in profiles
            .iter()
            .zip([CollationType::Emoji, CollationType::Eor])
        {
            if profile.kind != Some(expected)
                || profile.public_collation() != Some(expected.as_str())
                || !profile.id.locale.is_unknown()
                || profile.id.marker_attributes.as_str() != expected.as_str()
                || !profile.loads.iter().any(|load| {
                    load.marker == "CollationMetadataV1" && load.effective_locale.is_unknown()
                })
            {
                return Err(data_error("invalid locale-independent root sort profile"));
            }
        }
        Ok(Self(profiles))
    }

    fn from_image(data: &Arc<CollatorImageProvider>) -> Result<Self, CollatorOperationError> {
        let root: Locale = "und".parse().map_err(data_error)?;
        Self::checked(
            profile(
                data,
                &root,
                Some(CollationType::Emoji),
                Some("emoji".into()),
            )?,
            profile(data, &root, Some(CollationType::Eor), Some("eor".into()))?,
        )
    }
}
#[derive(Debug)]
pub struct CollatorProfiles {
    data: Arc<CollatorImageProvider>,
    locales: Box<[LocaleProfile]>,
    public_collations: Box<[Box<str>]>,
    locale_independent: LocaleIndependentSortPreferences,
    locale_preferences: LocaleCollationPreferences,
}
static EMBEDDED: OnceLock<Result<Arc<CollatorProfiles>, CollatorOperationError>> = OnceLock::new();
pub fn embedded_collator_profiles() -> Result<&'static CollatorProfiles, CollatorOperationError> {
    EMBEDDED
        .get_or_init(|| {
            crate::embedded_collator_data_image()
                .map(|image| image.profiles())
                .map_err(data_error)
        })
        .as_ref()
        .map(Arc::as_ref)
        .map_err(Clone::clone)
}
fn profile(
    data: &Arc<CollatorImageProvider>,
    locale: &Locale,
    kind: Option<CollationType>,
    public_collation: Option<Box<str>>,
) -> Result<Option<Arc<CollationProfile>>, CollatorOperationError> {
    let mut prefs = CollatorPreferences::from(locale.clone());
    prefs.collation_type = kind;
    let data_locale = CollationTailoringV1::make_locale(prefs.locale_preferences);
    let attributes = kind.map_or("", |kind| kind.as_str());
    let attrs = DataMarkerAttributes::try_from_str(attributes)
        .map_err(|error| data_error(format!("marker attributes: {error:?}")))?;
    let id = DataIdentifierCow::from_owned(attrs.to_owned(), data_locale);
    // The immutable provider enumerates its exact metadata rows. Avoid actual
    // missing-data loads for impossible locale/type pairs while preserving full
    // constructor admission and consumed-row recording for every possible pair.
    if !data.supports_metadata_id(&id) {
        return Ok(None);
    }
    let metadata = match DataProvider::<CollationMetadataV1>::load(
        data.as_ref(),
        DataRequest {
            id: id.as_borrowed(),
            metadata: Default::default(),
        },
    ) {
        Ok(response) => response,
        Err(error) if error.kind == DataErrorKind::IdentifierNotFound => return Ok(None),
        Err(error) => return Err(data_error(error)),
    };
    let bits = metadata.payload.get().bits;
    let provider = AdmissionProvider {
        data,
        expected: &id,
        bits,
        loads: RefCell::new(Vec::new()),
    };
    let collator = Collator::try_new_unstable(&provider, prefs, CollatorOptions::default())
        .map_err(data_error)?;
    let resolved = collator.as_borrowed().resolved_options();
    if resolved.strength != Strength::Tertiary
        || resolved.case_level != CaseLevel::Off
        || resolved.numeric != CollationNumericOrdering::False
        || resolved.max_variable != MaxVariable::Punctuation
    {
        return Err(data_error("unexpected pinned Collator defaults"));
    }
    let case_first = defaults_case_first(resolved.case_first)?;
    let ignore = match resolved.alternate_handling {
        AlternateHandling::Shifted => true,
        AlternateHandling::NonIgnorable => false,
        _ => return Err(data_error("unknown pinned punctuation default")),
    };
    let loads = provider.loads.into_inner();
    // The admitted immutable provider owns locale fallback for all CLDR request
    // locales, including those without an exact metadata ID. Capture that real
    // association once; every later option configuration must reproduce it.
    require_baseline_loads(&loads, bits, ignore)?;
    let admitted_locale: Locale = id.locale.to_string().parse().map_err(data_error)?;
    let profile = Arc::new(CollationProfile {
        data: data.clone(),
        id,
        locale: admitted_locale,
        kind,
        public_collation,
        bits,
        case_first,
        ignore,
        loads: loads.into_boxed_slice(),
    });
    // Exercise the conditional singleton before publishing the profile. This
    // consumes the genuine SpecialPrimaries payload used by numeric/shifted
    // comparisons as well as the nonconditional baseline associations.
    profile.construct(true, case_first, CollatorSensitivity::Variant, true)?;
    Ok(Some(profile))
}
impl CollatorProfiles {
    pub(crate) fn from_image(
        data: Arc<CollatorImageProvider>,
    ) -> Result<Self, CollatorOperationError> {
        // This is a candidate CLDR locale inventory only. Each actual pair must load
        // complete sort-default and genuine-search profiles before publication.
        let canonicalizer = data.locale_image().canonicalizer();
        let mut names = BTreeSet::new();
        for name in data.candidate_locales() {
            let mut locale: Locale = name.parse().map_err(data_error)?;
            canonicalizer.canonicalize(&mut locale);
            names.insert(locale.to_string());
        }
        names.insert("en-US".to_owned());
        Self::from_names(data, names, None)
    }
    pub(crate) fn from_projection(
        data: Arc<CollatorImageProvider>,
        catalogue: &crate::collator_image::CollatorCatalogue,
    ) -> Result<Self, CollatorOperationError> {
        let names = catalogue
            .public_locales()
            .iter()
            .map(|name| name.as_str().to_owned())
            .collect();
        Self::from_names(
            data,
            names,
            Some(LocaleCollationPreferences::from_projection(catalogue)),
        )
    }
    fn from_names(
        data: Arc<CollatorImageProvider>,
        names: BTreeSet<String>,
        preferences: Option<LocaleCollationPreferences>,
    ) -> Result<Self, CollatorOperationError> {
        let locale_independent = LocaleIndependentSortPreferences::from_image(&data)?;
        let mut types = BTreeSet::new();
        for id in data.metadata_ids() {
            if !id.marker_attributes.is_empty() {
                let value = id.marker_attributes.as_str();
                if purpose(collation_type(value)?)? {
                    types.insert(value.to_owned());
                }
            }
        }
        let mut locales = Vec::new();
        locales
            .try_reserve_exact(names.len())
            .map_err(|_| CollatorOperationError::Resource("locale catalogue allocation"))?;
        let mut public = BTreeSet::new();
        for name in names {
            let parsed: Locale = name.parse().map_err(data_error)?;
            let sort = profile(&data, &parsed, None, None)?
                .ok_or_else(|| data_error(format!("missing default-sort profile {name}")))?;
            let search = profile(&data, &parsed, Some(CollationType::Search), None)?
                .unwrap_or_else(|| Arc::clone(&sort));
            let mut collations = BTreeMap::new();
            for value in &types {
                if let Some(admitted) = profile(
                    &data,
                    &parsed,
                    Some(collation_type(value)?),
                    Some(value.clone().into_boxed_str()),
                )? {
                    public.insert(value.clone());
                    collations.insert(value.clone().into_boxed_str(), admitted);
                }
            }
            locales.push(LocaleProfile {
                locale: CanonicalLocaleId::from_data(name).map_err(data_error)?,
                sort,
                search,
                collations,
            });
        }
        if locales.is_empty() {
            return Err(data_error("empty admitted Collator locale catalogue"));
        }
        let locale_preferences =
            preferences.unwrap_or_else(|| LocaleCollationPreferences::from_profiles(&locales));
        Ok(Self {
            data,
            locales: locales.into_boxed_slice(),
            public_collations: public.into_iter().map(String::into_boxed_str).collect(),
            locale_independent,
            locale_preferences,
        })
    }
    pub(crate) fn preference_rows(
        &self,
    ) -> impl Iterator<Item = (&CanonicalLocaleId, &[Box<str>])> {
        self.locale_preferences.rows()
    }
    /// A wire/configuration owner resolved by another image cannot cross the
    /// selected provider boundary, even when its public locale text matches.
    pub(crate) fn compare(
        &self,
        request: CompareCollatorRequest,
    ) -> Result<CollatorOrdering, CollatorOperationError> {
        if !Arc::ptr_eq(&self.data, &request.configuration.locale.profile.data) {
            return Err(CollatorOperationError::InvalidConfiguration);
        }
        super::compare_collator(request)
    }
    pub fn available_locales(&self) -> impl ExactSizeIterator<Item = &CanonicalLocaleId> {
        self.locales.iter().map(|profile| &profile.locale)
    }
    /// Actual admitted sort-purpose union for supportedValuesOf, not raw attributes.
    /// Locale information instead uses the matched profile's sort names below.
    pub fn available_collations(&self) -> &[Box<str>] {
        &self.public_collations
    }
    #[cfg(test)]
    pub(super) fn has_admitted_sort_collation(&self, value: &str) -> bool {
        self.locales
            .iter()
            .any(|locale| locale.collations.contains_key(value))
    }
    fn matching(&self, requested: &str, matcher: LocaleMatcher) -> Option<usize> {
        match matcher {
            LocaleMatcher::Lookup | LocaleMatcher::BestFit => {}
        }
        let mut parsed: Locale = requested.parse().ok()?;
        parsed.extensions.unicode = Default::default();
        let name = parsed.to_string();
        let mut candidate = name.as_str();
        loop {
            if let Ok(index) = self
                .locales
                .binary_search_by(|profile| profile.locale.as_str().cmp(candidate))
            {
                return Some(index);
            }
            candidate = truncate_lookup(candidate)?;
        }
    }
    /// Locale information uses only the matched locale's admitted sort profiles.
    /// An unmatched locale has the two locale-independent collation preferences,
    /// without borrowing the formatting DefaultLocale or the global union.
    pub(crate) fn locale_sort_collations(
        &self,
        locale: &CanonicalLocaleId,
    ) -> Result<Box<[Box<str>]>, CollatorOperationError> {
        self.locale_preferences
            .resolve(locale, &self.locale_independent)
    }
    pub(crate) fn resolve(
        &self,
        request: CollatorLocaleRequest,
    ) -> Result<ResolvedCollatorLocale, CollatorOperationError> {
        let matched = request.requested.iter().find_map(|locale| {
            self.matching(locale.as_str(), request.matcher)
                .map(|index| (index, locale))
        });
        let index = matched.map_or_else(
            || {
                self.locales
                    .binary_search_by(|profile| profile.locale.as_str().cmp("en-US"))
                    .expect("admitted DefaultLocale")
            },
            |(index, _)| index,
        );
        let base = &self.locales[index];
        let requested: Option<Locale> = matched.map(|(_, locale)| {
            let mut parsed: Locale = locale.as_str().parse().expect("canonical request locale");
            parsed.extensions.unicode.keywords =
                crate::provider::collator_locale_keywords(locale.as_str());
            parsed
        });
        let keyword = |key| {
            requested
                .as_ref()
                .and_then(|locale| locale.extensions.unicode.keywords.get(&key))
                .map(ToString::to_string)
        };
        let mut co = None;
        let mut co_addition = None;
        if request.usage == CollatorUsage::Sort {
            if let Some(value) = keyword(unicode::key!("co")) {
                if base.collations.contains_key(value.as_str()) {
                    co_addition = Some(value.clone());
                    co = Some(value);
                }
            }
            if let Some(option) = request.collation {
                let value = crate::provider::canonical_collator_keyword(
                    option.as_str(),
                    self.data.locale_image().keyword_aliases().as_ref(),
                )?;
                if base.collations.contains_key(value.as_ref()) {
                    if co.as_deref() != Some(value.as_ref()) {
                        co_addition = None;
                    }
                    co = Some(value.into_string());
                }
            }
        }
        let profile = match request.usage {
            CollatorUsage::Sort => co.as_ref().map_or_else(
                || Arc::clone(&base.sort),
                |value| Arc::clone(&base.collations[value.as_str()]),
            ),
            CollatorUsage::Search => Arc::clone(&base.search),
        };
        let mut numeric = false;
        let mut kn_addition = None;
        if let Some(value) = keyword(unicode::key!("kn")) {
            match value.as_str() {
                "" | "true" => {
                    numeric = true;
                    kn_addition = Some("kn".to_owned());
                }
                "false" => {
                    kn_addition = Some("kn-false".to_owned());
                }
                _ => {}
            }
        }
        if let Some(option) = request.numeric {
            if numeric != option {
                kn_addition = None;
            }
            numeric = option;
        }
        let mut case_first = profile.case_first;
        let mut kf_addition = None;
        if let Some(value) = keyword(unicode::key!("kf")) {
            if let Some(kind) = CollatorCaseFirst::ALL
                .iter()
                .copied()
                .find(|kind| kind.name() == value)
            {
                case_first = kind;
                kf_addition = Some(value);
            }
        }
        if let Some(option) = request.case_first {
            if case_first != option {
                kf_addition = None;
            }
            case_first = option;
        }
        let mut additions = Vec::new();
        if let Some(value) = co_addition {
            additions.push(format!("co-{value}"));
        }
        if let Some(value) = kf_addition {
            additions.push(format!("kf-{value}"));
        }
        if let Some(value) = kn_addition {
            additions.push(value);
        }
        let mut resolved = base.locale.as_str().to_owned();
        if !additions.is_empty() {
            resolved.push_str("-u-");
            resolved.push_str(&additions.join("-"));
        }
        Ok(ResolvedCollatorLocale {
            resolved: CanonicalLocaleId::from_data(resolved).map_err(data_error)?,
            usage: request.usage,
            profile,
            numeric,
            case_first,
        })
    }
    pub(crate) fn supported(
        &self,
        request: CollatorSupportedLocalesRequest,
    ) -> Result<CollatorSupportedLocalesResult, CollatorOperationError> {
        let mut locales = Vec::new();
        locales
            .try_reserve_exact(request.requested.len())
            .map_err(|_| CollatorOperationError::Resource("supported locales allocation"))?;
        for locale in request.requested {
            if self.matching(locale.as_str(), request.matcher).is_some() {
                locales.push(locale);
            }
        }
        Ok(CollatorSupportedLocalesResult {
            locales: locales.into_boxed_slice(),
        })
    }
    pub(crate) fn admit(
        &self,
        resolved: CanonicalLocaleId,
        usage: CollatorUsage,
        collation: Option<&str>,
        numeric: bool,
        case_first: CollatorCaseFirst,
    ) -> Result<ResolvedCollatorLocale, CollatorOperationError> {
        let mut parsed: Locale = resolved.as_str().parse().map_err(data_error)?;
        parsed.extensions.unicode.keywords =
            crate::provider::collator_locale_keywords(resolved.as_str());
        let unicode = core::mem::take(&mut parsed.extensions.unicode);
        if !unicode.attributes.is_empty() {
            return Err(CollatorOperationError::InvalidConfiguration);
        }
        for (key, value) in unicode.keywords.iter() {
            let matches = match key.as_str() {
                "co" => {
                    usage == CollatorUsage::Sort && collation == Some(value.to_string().as_str())
                }
                "kf" => value.to_string() == case_first.name(),
                "kn" => {
                    if numeric {
                        value.to_string().is_empty() || value.to_string() == "true"
                    } else {
                        value.to_string() == "false"
                    }
                }
                _ => false,
            };
            if !matches {
                return Err(CollatorOperationError::InvalidConfiguration);
            }
        }
        let name = parsed.to_string();
        let index = self
            .locales
            .binary_search_by(|profile| profile.locale.as_str().cmp(&name))
            .map_err(|_| CollatorOperationError::InvalidConfiguration)?;
        let locale = &self.locales[index];
        let profile = match (usage, collation) {
            (CollatorUsage::Sort, None) => Arc::clone(&locale.sort),
            (CollatorUsage::Sort, Some(value)) => Arc::clone(
                locale
                    .collations
                    .get(value)
                    .ok_or(CollatorOperationError::InvalidConfiguration)?,
            ),
            (CollatorUsage::Search, None) => Arc::clone(&locale.search),
            (CollatorUsage::Search, Some(_)) => {
                return Err(CollatorOperationError::InvalidConfiguration);
            }
        };
        Ok(ResolvedCollatorLocale {
            resolved,
            usage,
            profile,
            numeric,
            case_first,
        })
    }
}
