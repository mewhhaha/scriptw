use super::record::InvalidLocaleWeekInfo;
use super::*;
use crate::CanonicalLocaleId;
use sha2::{Digest as _, Sha256};

fn week(tag: &str) -> LocaleWeekInfo {
    crate::embedded_native_locale_information_data_image()
        .unwrap()
        .resolve_week(LocaleWeekRequest::new(
            CanonicalLocaleId::from_data(tag).unwrap(),
        ))
        .expect("canonical locale has week fallback")
}

fn numbers(info: &LocaleWeekInfo) -> (u8, Vec<u8>) {
    (
        info.first_day().iso_number(),
        info.weekend().iter().map(|day| day.iso_number()).collect(),
    )
}

#[test]
fn genuine_week_profile_admits_all_pinned_regions_and_world_default() {
    let image = crate::embedded_native_locale_information_data_image().unwrap();
    let profile = image.week_profile();
    for (region, expected) in [
        ("001", (1, vec![6, 7])),
        ("GB", (1, vec![6, 7])),
        ("US", (7, vec![6, 7])),
        ("AF", (6, vec![4, 5])),
        ("IR", (6, vec![5])),
        ("IN", (7, vec![7])),
        ("UG", (1, vec![7])),
        ("MV", (5, vec![6, 7])),
        ("AQ", (1, vec![6, 7])),
    ] {
        assert_eq!(numbers(profile.get(region).unwrap()), expected, "{region}");
    }
    assert!(profile.get("ZZ").is_none());
    assert!(profile.get("XY").is_none());
}

#[test]
fn explicit_region_precedes_sd_and_sd_precedes_likely_subtags() {
    assert_eq!(numbers(&week("en-US-u-sd-gbsct")), (7, vec![6, 7]));
    assert_eq!(numbers(&week("en-u-sd-gbsct")), (1, vec![6, 7]));
    assert_eq!(numbers(&week("en-u-sd-afbds")), (6, vec![4, 5]));
    assert_eq!(numbers(&week("en")), (7, vec![6, 7]));
}

#[test]
fn available_rg_overrides_region_and_sd() {
    assert_eq!(numbers(&week("en-US-u-rg-gbzzzz")), (1, vec![6, 7]));
    assert_eq!(numbers(&week("en-u-rg-afzzzz-sd-gbsct")), (6, vec![4, 5]));
    // AQ has no sparse week override; its inherited 001 data remain available.
    assert_eq!(numbers(&week("en-US-u-rg-aqzzzz")), (1, vec![6, 7]));
}

#[test]
fn unavailable_rg_falls_back_without_ignoring_the_original_region() {
    assert_eq!(numbers(&week("en-US-u-rg-xyzzzz")), (7, vec![6, 7]));
    assert_eq!(numbers(&week("en-US-u-rg-zzzzzz")), (7, vec![6, 7]));
    assert_eq!(numbers(&week("en-u-rg-xyzzzz-sd-afbds")), (6, vec![4, 5]));
}

#[test]
fn malformed_or_multi_subtag_subdivision_values_do_not_select_regions() {
    for suffix in [
        "rg-uszzzzz",
        "rg-uszzzz-abc",
        "rg-x1zzzz",
        "sd-gbzzzzz",
        "sd-gbzzzz-abc",
    ] {
        assert_eq!(
            numbers(&week(&format!("en-US-u-{suffix}"))),
            (7, vec![6, 7])
        );
    }
    // A valid but unavailable sd prevents likely-subtag region inference.
    assert_eq!(numbers(&week("en-u-sd-xyfoo")), (1, vec![6, 7]));
    // Syntax is the subdivision production, not a fixture-specific zzzz test.
    assert_eq!(numbers(&week("en-US-u-rg-foobar")), (1, vec![6, 7]));
}

#[test]
fn canonical_region_and_subdivision_aliases_use_the_genuine_owner() {
    assert_eq!(numbers(&week("en-u-sd-fi01")), numbers(&week("en-AX")));
    assert_eq!(numbers(&week("en-u-rg-fi01")), numbers(&week("en-AX")));
    assert_eq!(numbers(&week("en-u-rg-buzzzz")), numbers(&week("en-MM")));
}

#[test]
fn recognized_fw_overrides_only_first_day_after_rg_selection() {
    for (value, number) in [
        ("mon", 1),
        ("tue", 2),
        ("wed", 3),
        ("thu", 4),
        ("fri", 5),
        ("sat", 6),
        ("sun", 7),
    ] {
        let info = week(&format!("en-US-u-fw-{value}-rg-afzzzz"));
        assert_eq!(numbers(&info), (number, vec![4, 5]));
    }
}

#[test]
fn unknown_empty_and_multi_subtag_fw_keep_regional_first_day() {
    for tag in [
        "en-US-u-fw",
        "en-US-u-fw-primidi",
        "en-US-u-fw-mon-abc",
        "en-US-u-fw-false",
    ] {
        assert_eq!(numbers(&week(tag)), (7, vec![6, 7]));
    }
}

#[test]
fn unicode_looking_private_use_is_not_a_week_override() {
    assert_eq!(numbers(&week("en-US-x-u-fw-mon")), (7, vec![6, 7]));
    assert_eq!(numbers(&week("en-US-x-u-rg-afzzzz")), (7, vec![6, 7]));
}

#[test]
fn reserved_language_preserves_unknown_language_and_world_fallback() {
    for tag in ["abcde", "abcdefgh", "abcde-u-fw-primidi"] {
        assert_eq!(numbers(&week(tag)), (1, vec![6, 7]));
    }
    assert_eq!(numbers(&week("abcde-u-rg-afzzzz-fw-mon")), (1, vec![4, 5]));
    assert_eq!(numbers(&week("abcde-u-sd-afbds")), (6, vec![4, 5]));
}

#[test]
fn every_valid_week_mask_roundtrips_with_numeric_sorted_weekend() {
    for first in 1..=7 {
        for mask in 1..=127 {
            let info = LocaleWeekInfo::from_iso_mask(first, mask).unwrap();
            assert_eq!(u32::from(info.first_day().iso_number()), first);
            assert_eq!(u32::from(info.weekend_mask()), mask);
            assert!(!info.weekend().is_empty());
            assert!(info.weekend().windows(2).all(|p| p[0] < p[1]));
        }
    }
    assert_eq!(
        LocaleWeekInfo::from_iso_mask(0, 1),
        Err(InvalidLocaleWeekInfo::FirstDay)
    );
    assert_eq!(
        LocaleWeekInfo::from_iso_mask(8, 1),
        Err(InvalidLocaleWeekInfo::FirstDay)
    );
    assert_eq!(
        LocaleWeekInfo::from_iso_mask(1, 0),
        Err(InvalidLocaleWeekInfo::EmptyWeekend)
    );
    assert_eq!(
        LocaleWeekInfo::from_iso_mask(1, 128),
        Err(InvalidLocaleWeekInfo::WeekendMask)
    );
}

#[test]
fn checked_result_rejects_duplicate_unsorted_and_empty_weekends() {
    use IsoWeekday::{Monday, Saturday, Sunday};
    assert_eq!(
        LocaleWeekInfo::new(Monday, Vec::new().into_boxed_slice()),
        Err(InvalidLocaleWeekInfo::EmptyWeekend)
    );
    assert_eq!(
        LocaleWeekInfo::new(Monday, vec![Sunday, Saturday].into_boxed_slice()),
        Err(InvalidLocaleWeekInfo::WeekendOrder)
    );
    assert_eq!(
        LocaleWeekInfo::new(Monday, vec![Saturday, Saturday].into_boxed_slice()),
        Err(InvalidLocaleWeekInfo::WeekendOrder)
    );
}

fn mutated_profile(
    change: impl FnOnce(&mut serde_json::Value),
) -> Result<profile::LocaleWeekProfile, LocaleWeekProfileError> {
    let mut raw: serde_json::Value = serde_json::from_str(include_str!(
        "../../../data/locale-week-cldr-47/profile.json"
    ))
    .unwrap();
    change(&mut raw);
    let bytes = serde_json::to_vec(&raw).unwrap();
    let digest = Sha256::digest(&bytes).into();
    profile::LocaleWeekProfile::from_bytes(&bytes, digest)
}

#[test]
fn profile_admission_rejects_missing_default_bad_rows_and_revision() {
    assert!(matches!(
        mutated_profile(|v| {
            v["regions"].as_array_mut().unwrap().remove(0);
        }),
        Err(LocaleWeekProfileError::MissingWorldDefault)
    ));
    assert!(matches!(
        mutated_profile(|v| v["schema"] = 2.into()),
        Err(LocaleWeekProfileError::Schema)
    ));
    assert!(matches!(
        mutated_profile(|v| v["regions"][0]["weekend_mask"] = 0.into()),
        Err(LocaleWeekProfileError::WeekInfo)
    ));
    assert!(matches!(
        mutated_profile(|v| v["regions"][0]["first_day"] = 8.into()),
        Err(LocaleWeekProfileError::WeekInfo)
    ));
    assert!(matches!(
        mutated_profile(|v| v["regions"][0]["region"] = "ZZ".into()),
        Err(LocaleWeekProfileError::Region)
    ));
    assert!(matches!(
        mutated_profile(|v| {
            v["regions"].as_array_mut().unwrap().swap(0, 1);
        }),
        Err(LocaleWeekProfileError::RegionOrder)
    ));
    assert!(matches!(
        mutated_profile(|v| v["cldr_release"] = "48.0.0".into()),
        Err(LocaleWeekProfileError::Revision)
    ));
}

#[test]
fn profile_admission_checks_exact_profile_digest_before_json_fields() {
    let bytes = include_bytes!("../../../data/locale-week-cldr-47/profile.json");
    assert!(matches!(
        profile::LocaleWeekProfile::from_bytes(bytes, [0; 32]),
        Err(LocaleWeekProfileError::Digest)
    ));
}
