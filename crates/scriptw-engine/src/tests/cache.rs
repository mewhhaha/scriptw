use super::*;

#[test]
fn program_cache_key_tracks_source_goal_and_configuration() {
    let base = CompileOptions {
        filename: Some("case.js".to_string()),
        ..CompileOptions::default()
    };
    let key = program_wasm_cache_key("1 + 2", ParseGoal::Script, &base);
    let mut conformance = base.clone();
    conformance.intl_profile = IntlCompilationProfile::Conformance;
    assert_ne!(
        key,
        program_wasm_cache_key("1 + 2", ParseGoal::Script, &conformance)
    );
    assert_eq!(
        key,
        program_wasm_cache_key("1 + 2", ParseGoal::Script, &base)
    );
    assert_ne!(
        key,
        program_wasm_cache_key("1 + 3", ParseGoal::Script, &base)
    );
    assert_ne!(
        key,
        program_wasm_cache_key("1 + 2", ParseGoal::Module, &base)
    );
    let mut changed = base.clone();
    changed.optimize = !changed.optimize;
    assert_ne!(
        key,
        program_wasm_cache_key("1 + 2", ParseGoal::Script, &changed)
    );
    let mut changed_policy = base.clone();
    changed_policy.host_surface_policy = HostSurfacePolicy::Test262;
    assert_ne!(
        key,
        program_wasm_cache_key("1 + 2", ParseGoal::Script, &changed_policy)
    );
    let mut changed_rejection_policy = base.clone();
    changed_rejection_policy.promise_rejection_policy = PromiseRejectionPolicy::Ignore;
    assert_ne!(
        key,
        program_wasm_cache_key("1 + 2", ParseGoal::Script, &changed_rejection_policy)
    );
    let mut first_custom = base.clone();
    first_custom.intl_profile =
        IntlCompilationProfile::Custom(CustomProfileId::parse("profile-a").unwrap());
    let first_custom_key = program_wasm_cache_key("1 + 2", ParseGoal::Script, &first_custom);
    assert_ne!(key, first_custom_key);
    let mut second_custom = first_custom.clone();
    second_custom.intl_profile =
        IntlCompilationProfile::Custom(CustomProfileId::parse("profile-ab").unwrap());
    assert_ne!(
        first_custom_key,
        program_wasm_cache_key("1 + 2", ParseGoal::Script, &second_custom)
    );
    first_custom.filename = Some("bc".into());
    second_custom.filename = Some("c".into());
    first_custom.intl_profile =
        IntlCompilationProfile::Custom(CustomProfileId::parse("a").unwrap());
    second_custom.intl_profile =
        IntlCompilationProfile::Custom(CustomProfileId::parse("ab").unwrap());
    assert_ne!(
        program_wasm_cache_key("1 + 2", ParseGoal::Script, &first_custom),
        program_wasm_cache_key("1 + 2", ParseGoal::Script, &second_custom)
    );
    let projection = |locales: &[&str]| {
        IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::new(
                CustomProfileId::parse("same-id").unwrap(),
                Some(locales),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap(),
        )
    };
    first_custom.filename = None;
    second_custom.filename = None;
    first_custom.intl_profile = projection(&["es", "he"]);
    second_custom.intl_profile = projection(&["es", "ar"]);
    let projected_key = program_wasm_cache_key("1 + 2", ParseGoal::Script, &first_custom);
    assert_ne!(
        projected_key,
        program_wasm_cache_key("1 + 2", ParseGoal::Script, &second_custom)
    );
    second_custom.intl_profile = projection(&["he", "es"]);
    assert_eq!(
        projected_key,
        program_wasm_cache_key("1 + 2", ParseGoal::Script, &second_custom)
    );
    second_custom.intl_profile =
        IntlCompilationProfile::Custom(CustomProfileId::parse("same-id").unwrap());
    assert_ne!(
        projected_key,
        program_wasm_cache_key("1 + 2", ParseGoal::Script, &second_custom)
    );
    let mut changed_module_policy = base.clone();
    changed_module_policy.module_loading_policy = ModuleLoadingPolicy::RejectAll;
    assert_ne!(
        key,
        program_wasm_cache_key("1 + 2", ParseGoal::Script, &changed_module_policy)
    );
}

#[test]
fn currency_projection_cache_key_binds_presence_codes_and_canonical_order() {
    let options = |codes: &[&str]| CompileOptions {
        intl_profile: IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::for_currency_codes(
                CustomProfileId::parse("currency-cache").unwrap(),
                codes,
            )
            .unwrap(),
        ),
        ..CompileOptions::default()
    };
    let first = options(&["JPY", "EUR"]);
    let equivalent = options(&["eur", "jpy"]);
    let selected = options(&["EUR"]);
    let full = CompileOptions {
        intl_profile: IntlCompilationProfile::Custom(
            CustomProfileId::parse("currency-cache").unwrap(),
        ),
        ..CompileOptions::default()
    };
    let key = program_wasm_cache_key("262", ParseGoal::Script, &first);
    assert_eq!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &equivalent)
    );
    assert_ne!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &selected)
    );
    assert_ne!(key, program_wasm_cache_key("262", ParseGoal::Script, &full));
    let with_locales = CompileOptions {
        intl_profile: IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::new(
                CustomProfileId::parse("currency-cache").unwrap(),
                None,
                None,
                None,
                None,
                Some(&["fr"]),
                None,
                None,
                None,
            )
            .unwrap()
            .with_currency_codes(&["EUR", "JPY"])
            .unwrap(),
        ),
        ..CompileOptions::default()
    };
    assert_ne!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &with_locales)
    );
}

#[test]
fn calendar_projection_cache_key_binds_requested_service_data_without_global_aliases() {
    let profile = |calendars: &[&str]| {
        IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::for_date_time_calendars(
                CustomProfileId::parse("calendar-cache").unwrap(),
                calendars,
            )
            .unwrap(),
        )
    };
    let first = CompileOptions {
        intl_profile: profile(&["chinese", "gregory"]),
        ..CompileOptions::default()
    };
    let ordered = CompileOptions {
        intl_profile: profile(&["gregory", "chinese"]),
        ..CompileOptions::default()
    };
    let fewer = CompileOptions {
        intl_profile: profile(&["chinese"]),
        ..CompileOptions::default()
    };
    let full = CompileOptions {
        intl_profile: IntlCompilationProfile::Custom(
            CustomProfileId::parse("calendar-cache").unwrap(),
        ),
        ..CompileOptions::default()
    };
    let key = program_wasm_cache_key("262", ParseGoal::Script, &first);
    assert_eq!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &ordered)
    );
    assert_ne!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &fewer)
    );
    assert_ne!(key, program_wasm_cache_key("262", ParseGoal::Script, &full));
    let coupled = CompileOptions {
        intl_profile: IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::for_date_time_calendars(
                CustomProfileId::parse("calendar-cache").unwrap(),
                &["chinese", "gregory"],
            )
            .unwrap()
            .with_currency_codes(&["EUR"])
            .unwrap(),
        ),
        ..CompileOptions::default()
    };
    assert_ne!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &coupled)
    );
}

#[test]
fn numbering_projection_cache_key_binds_requested_domains_and_composed_data() {
    let options = |systems: &[&str]| CompileOptions {
        intl_profile: IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::for_numbering_systems(
                CustomProfileId::parse("numbering-cache").unwrap(),
                systems,
            )
            .unwrap(),
        ),
        ..CompileOptions::default()
    };
    let first = options(&["deva", "latn"]);
    let equivalent = options(&["LATN", "DEVA"]);
    let fewer = options(&["deva"]);
    let full = CompileOptions {
        intl_profile: IntlCompilationProfile::Custom(
            CustomProfileId::parse("numbering-cache").unwrap(),
        ),
        ..CompileOptions::default()
    };
    let key = program_wasm_cache_key("262", ParseGoal::Script, &first);
    assert_eq!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &equivalent)
    );
    assert_ne!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &fewer)
    );
    assert_ne!(key, program_wasm_cache_key("262", ParseGoal::Script, &full));
    let composed = CompileOptions {
        intl_profile: IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::for_numbering_systems(
                CustomProfileId::parse("numbering-cache").unwrap(),
                &["deva", "latn"],
            )
            .unwrap()
            .with_date_time_calendars(&["chinese"])
            .unwrap()
            .with_currency_codes(&["EUR"])
            .unwrap(),
        ),
        ..CompileOptions::default()
    };
    assert_ne!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &composed)
    );
}

#[test]
fn named_zone_projection_cache_key_binds_primary_identity_and_independent_data_fields() {
    let options = |zones: &[&str]| CompileOptions {
        intl_profile: IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::for_named_time_zones(
                CustomProfileId::parse("named-zone-cache").unwrap(),
                zones,
            )
            .unwrap(),
        ),
        ..CompileOptions::default()
    };
    let first = options(&["US/Eastern", "Europe/London"]);
    let equivalent = options(&["europe/london", "America/New_York"]);
    let fewer = options(&["America/New_York"]);
    let full = CompileOptions {
        intl_profile: IntlCompilationProfile::Custom(
            CustomProfileId::parse("named-zone-cache").unwrap(),
        ),
        ..CompileOptions::default()
    };
    let key = program_wasm_cache_key("262", ParseGoal::Script, &first);
    assert_eq!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &equivalent)
    );
    assert_ne!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &fewer)
    );
    assert_ne!(key, program_wasm_cache_key("262", ParseGoal::Script, &full));
    let composed = CompileOptions {
        intl_profile: IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::for_named_time_zones(
                CustomProfileId::parse("named-zone-cache").unwrap(),
                &["Europe/London", "America/New_York"],
            )
            .unwrap()
            .with_numbering_systems(&["deva"])
            .unwrap(),
        ),
        ..CompileOptions::default()
    };
    assert_ne!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &composed)
    );
}

#[test]
fn service_cache_key_binds_requested_public_roots_and_not_only_shared_dependencies() {
    let options = |services: &[&str]| CompileOptions {
        intl_profile: IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::for_services(
                CustomProfileId::parse("service-cache").unwrap(),
                services,
            )
            .unwrap(),
        ),
        ..CompileOptions::default()
    };
    let first = options(&["NumberFormat", "PluralRules"]);
    let reordered = options(&["PluralRules", "NumberFormat"]);
    let number_only = options(&["NumberFormat"]);
    let plural_only = options(&["PluralRules"]);
    let key = program_wasm_cache_key("262", ParseGoal::Script, &first);
    assert_eq!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &reordered)
    );
    assert_ne!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &number_only)
    );
    assert_ne!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &plural_only)
    );
    let full = CompileOptions {
        intl_profile: IntlCompilationProfile::Custom(
            CustomProfileId::parse("service-cache").unwrap(),
        ),
        ..CompileOptions::default()
    };
    assert_ne!(key, program_wasm_cache_key("262", ParseGoal::Script, &full));
    let filtered = CompileOptions {
        intl_profile: IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::new(
                CustomProfileId::parse("service-cache").unwrap(),
                None,
                None,
                None,
                None,
                Some(&["fr"]),
                None,
                None,
                None,
            )
            .unwrap()
            .with_services(&["NumberFormat", "PluralRules"])
            .unwrap(),
        ),
        ..CompileOptions::default()
    };
    assert_ne!(
        key,
        program_wasm_cache_key("262", ParseGoal::Script, &filtered)
    );
}

#[test]
fn component_projection_cache_keys_bind_each_filter_presence_and_order_independently() {
    let profile = |id: &str, lists: Option<&[&str]>, relatives: Option<&[&str]>| {
        IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::new(
                CustomProfileId::parse(id).unwrap(),
                lists,
                relatives,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap(),
        )
    };
    let key = |profile| {
        let options = CompileOptions {
            intl_profile: profile,
            ..CompileOptions::default()
        };
        program_wasm_cache_key_with_compiler_fingerprint(
            "262;",
            ParseGoal::Script,
            &options,
            None,
            &[19; 32],
        )
    };
    let both = key(profile(
        "same-id",
        Some(&["fr", "pl"]),
        Some(&["ar", "en-US"]),
    ));
    assert_eq!(
        both,
        key(profile(
            "same-id",
            Some(&["pl", "fr"]),
            Some(&["en-US", "ar"])
        ))
    );
    let variants = [
        profile("same-id", Some(&["fr", "pl"]), None),
        profile("same-id", None, Some(&["ar", "en-US"])),
        profile("same-id", Some(&["ar", "en-US"]), Some(&["fr", "pl"])),
        profile("same-id", Some(&["fr"]), Some(&["pl", "ar", "en-US"])),
        profile("same-id", Some(&["fr", "pl"]), Some(&["ar"])),
        profile("other-id", Some(&["fr", "pl"]), Some(&["ar", "en-US"])),
        IntlCompilationProfile::Custom(CustomProfileId::parse("same-id").unwrap()),
        IntlCompilationProfile::Minimal,
    ];
    let mut keys = std::collections::BTreeSet::from([both]);
    for variant in variants {
        assert!(
            keys.insert(key(variant)),
            "distinct profile tuples must not reuse cached artifacts"
        );
    }
    assert_ne!(
        key(profile("a", Some(&["bcd"]), None)),
        key(profile("ab", Some(&["cd"]), None)),
    );
    assert_ne!(
        key(profile("same-id", Some(&["fr"]), None)),
        key(profile("same-id", None, Some(&["fr"]))),
    );
    assert!(
        CustomIntlProfile::new(
            CustomProfileId::parse("same-id").unwrap(),
            Some(&[]),
            Some(&["fr"]),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .is_err()
    );
}

#[test]
fn display_names_cache_filters_are_independent_of_other_components_and_input_order() {
    let profile =
        |id: &str, lists: Option<&[&str]>, relatives: Option<&[&str]>, display: Option<&[&str]>| {
            IntlCompilationProfile::CustomProjection(
                CustomIntlProfile::new(
                    CustomProfileId::parse(id).unwrap(),
                    lists,
                    relatives,
                    display,
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap(),
            )
        };
    let key = |intl_profile| {
        program_wasm_cache_key_with_compiler_fingerprint(
            "262;",
            ParseGoal::Script,
            &CompileOptions {
                intl_profile,
                ..CompileOptions::default()
            },
            None,
            &[19; 32],
        )
    };
    let all = key(profile(
        "same-id",
        Some(&["es", "he"]),
        Some(&["fr", "pl"]),
        Some(&["fr", "ja"]),
    ));
    assert_eq!(
        all,
        key(profile(
            "same-id",
            Some(&["he", "es"]),
            Some(&["pl", "fr"]),
            Some(&["ja", "fr"])
        ))
    );
    let variants = [
        profile("same-id", Some(&["es", "he"]), Some(&["fr", "pl"]), None),
        profile(
            "same-id",
            Some(&["es", "he"]),
            Some(&["fr", "pl"]),
            Some(&["fr"]),
        ),
        profile(
            "same-id",
            Some(&["es", "he"]),
            Some(&["fr", "pl"]),
            Some(&["de", "ja"]),
        ),
        profile("same-id", None, Some(&["fr", "pl"]), Some(&["fr", "ja"])),
        profile("same-id", Some(&["es", "he"]), None, Some(&["fr", "ja"])),
        profile("same-id", None, None, Some(&["fr", "ja"])),
        profile(
            "same-id",
            Some(&["fr", "ja"]),
            Some(&["fr", "pl"]),
            Some(&["es", "he"]),
        ),
        profile(
            "other-id",
            Some(&["es", "he"]),
            Some(&["fr", "pl"]),
            Some(&["fr", "ja"]),
        ),
        IntlCompilationProfile::Custom(CustomProfileId::parse("same-id").unwrap()),
    ];
    let mut keys = std::collections::BTreeSet::from([all]);
    for variant in variants {
        assert!(
            keys.insert(key(variant)),
            "different selected component inputs must not reuse an artifact"
        );
    }
    assert_ne!(
        key(profile("a", None, None, Some(&["bcd"]))),
        key(profile("ab", None, None, Some(&["cd"])))
    );
    assert_ne!(
        key(profile("same-id", None, Some(&["fr"]), None)),
        key(profile("same-id", None, None, Some(&["fr"])))
    );
}

#[test]
fn duration_cache_filters_bind_presence_and_rows_independently_of_other_components() {
    let profile = |id: &str,
                   lists: Option<&[&str]>,
                   relatives: Option<&[&str]>,
                   display: Option<&[&str]>,
                   duration: Option<&[&str]>| {
        IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::new(
                CustomProfileId::parse(id).unwrap(),
                lists,
                relatives,
                display,
                duration,
                None,
                None,
                None,
                None,
            )
            .unwrap(),
        )
    };
    let key = |intl_profile| {
        program_wasm_cache_key_with_compiler_fingerprint(
            "262;",
            ParseGoal::Script,
            &CompileOptions {
                intl_profile,
                ..CompileOptions::default()
            },
            None,
            &[19; 32],
        )
    };
    let all = key(profile(
        "same-id",
        Some(&["es", "he"]),
        Some(&["fr", "pl"]),
        Some(&["fr", "ja"]),
        Some(&["fr", "sr"]),
    ));
    assert_eq!(
        all,
        key(profile(
            "same-id",
            Some(&["he", "es"]),
            Some(&["pl", "fr"]),
            Some(&["ja", "fr"]),
            Some(&["sr", "fr"])
        ))
    );
    let variants = [
        profile(
            "same-id",
            Some(&["es", "he"]),
            Some(&["fr", "pl"]),
            Some(&["fr", "ja"]),
            None,
        ),
        profile(
            "same-id",
            Some(&["es", "he"]),
            Some(&["fr", "pl"]),
            Some(&["fr", "ja"]),
            Some(&["fr"]),
        ),
        profile(
            "same-id",
            Some(&["es", "he"]),
            Some(&["fr", "pl"]),
            Some(&["fr", "ja"]),
            Some(&["de", "sr"]),
        ),
        profile(
            "same-id",
            Some(&["fr", "sr"]),
            Some(&["fr", "pl"]),
            Some(&["fr", "ja"]),
            Some(&["es", "he"]),
        ),
        profile("same-id", None, None, None, Some(&["fr", "sr"])),
        profile(
            "other-id",
            Some(&["es", "he"]),
            Some(&["fr", "pl"]),
            Some(&["fr", "ja"]),
            Some(&["fr", "sr"]),
        ),
        IntlCompilationProfile::Custom(CustomProfileId::parse("same-id").unwrap()),
    ];
    let mut keys = std::collections::BTreeSet::from([all]);
    for variant in variants {
        assert!(
            keys.insert(key(variant)),
            "distinct Duration selections must not share an artifact"
        );
    }
    assert_ne!(
        key(profile("same-id", None, None, Some(&["fr"]), None)),
        key(profile("same-id", None, None, None, Some(&["fr"])))
    );
    assert_ne!(
        key(profile("a", None, None, None, Some(&["bcd"]))),
        key(profile("ab", None, None, None, Some(&["cd"])))
    );
}

#[test]
fn number_cache_filters_bind_the_coupled_public_domain_and_component_position() {
    let profile = |id: &str,
                   lists: Option<&[&str]>,
                   relatives: Option<&[&str]>,
                   display: Option<&[&str]>,
                   duration: Option<&[&str]>,
                   numbers: Option<&[&str]>| {
        IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::new(
                CustomProfileId::parse(id).unwrap(),
                lists,
                relatives,
                display,
                duration,
                numbers,
                None,
                None,
                None,
            )
            .unwrap(),
        )
    };
    let key = |intl_profile| {
        program_wasm_cache_key_with_compiler_fingerprint(
            "262;",
            ParseGoal::Script,
            &CompileOptions {
                intl_profile,
                ..CompileOptions::default()
            },
            None,
            &[19; 32],
        )
    };
    let all = key(profile(
        "same-id",
        Some(&["es", "he"]),
        Some(&["fr", "pl"]),
        Some(&["fr", "ja"]),
        Some(&["fr", "sr"]),
        Some(&["es", "pl"]),
    ));
    assert_eq!(
        all,
        key(profile(
            "same-id",
            Some(&["he", "es"]),
            Some(&["pl", "fr"]),
            Some(&["ja", "fr"]),
            Some(&["sr", "fr"]),
            Some(&["pl", "es"]),
        ))
    );
    let variants = [
        profile(
            "same-id",
            Some(&["es", "he"]),
            Some(&["fr", "pl"]),
            Some(&["fr", "ja"]),
            Some(&["fr", "sr"]),
            None,
        ),
        profile(
            "same-id",
            Some(&["es", "he"]),
            Some(&["fr", "pl"]),
            Some(&["fr", "ja"]),
            Some(&["fr", "sr"]),
            Some(&["es"]),
        ),
        profile(
            "same-id",
            Some(&["es", "he"]),
            Some(&["fr", "pl"]),
            Some(&["fr", "ja"]),
            Some(&["fr", "sr"]),
            Some(&["fr", "sr"]),
        ),
        profile("same-id", None, None, None, None, Some(&["es", "pl"])),
        profile(
            "other-id",
            Some(&["es", "he"]),
            Some(&["fr", "pl"]),
            Some(&["fr", "ja"]),
            Some(&["fr", "sr"]),
            Some(&["es", "pl"]),
        ),
        IntlCompilationProfile::Custom(CustomProfileId::parse("same-id").unwrap()),
    ];
    let mut keys = std::collections::BTreeSet::from([all]);
    for variant in variants {
        assert!(
            keys.insert(key(variant)),
            "different public Number inputs must not reuse an artifact"
        );
    }
    assert_ne!(
        key(profile("same-id", None, None, None, Some(&["fr"]), None)),
        key(profile("same-id", None, None, None, None, Some(&["fr"])))
    );
    assert_ne!(
        key(profile("a", None, None, None, None, Some(&["bcd"]))),
        key(profile("ab", None, None, None, None, Some(&["cd"])))
    );
}

#[test]
fn datetime_cache_filters_bind_ordered_rows_and_their_distinct_component_position() {
    let profile = |id: &str, numbers: Option<&[&str]>, dates: Option<&[&str]>| {
        IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::new(
                CustomProfileId::parse(id).unwrap(),
                None,
                None,
                None,
                None,
                numbers,
                dates,
                None,
                None,
            )
            .unwrap(),
        )
    };
    let key = |intl_profile| {
        program_wasm_cache_key_with_compiler_fingerprint(
            "262;",
            ParseGoal::Script,
            &CompileOptions {
                intl_profile,
                ..CompileOptions::default()
            },
            None,
            &[23; 32],
        )
    };
    let selected = key(profile("same-id", None, Some(&["ar-EG", "zh"])));
    assert_eq!(
        selected,
        key(profile("same-id", None, Some(&["zh", "ar-EG"])))
    );
    let variants = [
        profile("same-id", None, Some(&["ar-EG"])),
        profile("same-id", None, Some(&["en-US"])),
        profile("same-id", Some(&["ar-EG", "zh"]), None),
        profile("same-id", Some(&["es"]), Some(&["ar-EG", "zh"])),
        profile("other-id", None, Some(&["ar-EG", "zh"])),
        IntlCompilationProfile::Custom(CustomProfileId::parse("same-id").unwrap()),
    ];
    let mut keys = std::collections::BTreeSet::from([selected]);
    for variant in variants {
        assert!(
            keys.insert(key(variant)),
            "distinct DateTime selections cannot reuse an artifact"
        );
    }
    assert_ne!(
        key(profile("same-id", Some(&["fr"]), None)),
        key(profile("same-id", None, Some(&["fr"]))),
    );
    assert_ne!(
        key(profile("a", None, Some(&["bcd"]))),
        key(profile("ab", None, Some(&["cd"]))),
    );
}

#[test]
fn collator_cache_filter_has_its_own_position_and_exact_length_framing() {
    let profile = |id: &str, dates: Option<&[&str]>, collators: Option<&[&str]>| {
        IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::new(
                CustomProfileId::parse(id).unwrap(),
                None,
                None,
                None,
                None,
                None,
                dates,
                collators,
                None,
            )
            .unwrap(),
        )
    };
    let key = |intl_profile| {
        program_wasm_cache_key_with_compiler_fingerprint(
            "262;",
            ParseGoal::Script,
            &CompileOptions {
                intl_profile,
                ..CompileOptions::default()
            },
            None,
            &[31; 32],
        )
    };
    let selected = key(profile("same-id", None, Some(&["de-CH", "sv"])));
    assert_eq!(
        selected,
        key(profile("same-id", None, Some(&["sv", "de-CH"])))
    );
    for variant in [
        profile("same-id", None, Some(&["sv"])),
        profile("other-id", None, Some(&["de-CH", "sv"])),
        profile("same-id", Some(&["de-CH", "sv"]), None),
        profile("same-id", Some(&["zh"]), Some(&["de-CH", "sv"])),
        IntlCompilationProfile::Custom(CustomProfileId::parse("same-id").unwrap()),
    ] {
        assert_ne!(selected, key(variant));
    }
    assert_ne!(
        key(profile("a", None, Some(&["bcd"]))),
        key(profile("ab", None, Some(&["cd"])))
    );
}

#[test]
fn segmenter_cache_filter_has_independent_presence_order_and_length_framing() {
    let profile = |id: &str, collators: Option<&[&str]>, segmenters: Option<&[&str]>| {
        IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::new(
                CustomProfileId::parse(id).unwrap(),
                None,
                None,
                None,
                None,
                None,
                None,
                collators,
                segmenters,
            )
            .unwrap(),
        )
    };
    let key = |intl_profile| {
        program_wasm_cache_key_with_compiler_fingerprint(
            "262;",
            ParseGoal::Script,
            &CompileOptions {
                intl_profile,
                ..CompileOptions::default()
            },
            None,
            &[37; 32],
        )
    };
    let selected = key(profile("same-id", None, Some(&["sv", "el"])));
    assert_eq!(selected, key(profile("same-id", None, Some(&["el", "sv"]))));
    for different in [
        profile("same-id", None, Some(&["sv"])),
        profile("other-id", None, Some(&["sv", "el"])),
        profile("same-id", Some(&["sv", "el"]), None),
        profile("same-id", Some(&["de"]), Some(&["sv", "el"])),
        IntlCompilationProfile::Custom(CustomProfileId::parse("same-id").unwrap()),
    ] {
        assert_ne!(selected, key(different));
    }
    assert_ne!(
        key(profile("a", None, Some(&["bcd"]))),
        key(profile("ab", None, Some(&["cd"])))
    );
}

#[test]
fn module_prelude_is_part_of_the_program_cache_key() {
    let options = CompileOptions::default();
    let key = program_wasm_cache_key("export {};", ParseGoal::Module, &options);
    let first = CompileOptions {
        module_prelude: Some("var value = 1;".into()),
        ..options.clone()
    };
    let second = CompileOptions {
        module_prelude: Some("var value = 2;".into()),
        ..options
    };
    let first_key = program_wasm_cache_key("export {};", ParseGoal::Module, &first);
    assert_ne!(key, first_key);
    assert_ne!(
        first_key,
        program_wasm_cache_key("export {};", ParseGoal::Module, &second)
    );
}

#[test]
fn program_cache_key_frames_the_adversarial_policy_filename_source_tuple() {
    let compiler = [0x5a; 32];
    let reject_all_source = "throw 1;//module-loading:filesystem\n42;";
    let reject_all = CompileOptions {
        filename: Some("case.js".to_string()),
        module_loading_policy: ModuleLoadingPolicy::RejectAll,
        ..CompileOptions::default()
    };
    let filesystem_source = "\n42;";
    let filesystem = CompileOptions {
        filename: Some("case.jsmodule-loading:reject-allthrow 1;//".to_string()),
        module_loading_policy: ModuleLoadingPolicy::Filesystem,
        ..CompileOptions::default()
    };

    // This is the exact byte collision in the retired unframed tail:
    // filename || module-loading-policy-string || source.
    assert_eq!(
        format!(
            "{}module-loading:reject-all{reject_all_source}",
            reject_all.filename.as_deref().unwrap()
        ),
        format!(
            "{}module-loading:filesystem{filesystem_source}",
            filesystem.filename.as_deref().unwrap()
        )
    );
    assert_ne!(
        program_wasm_cache_key_with_compiler_fingerprint(
            reject_all_source,
            ParseGoal::Script,
            &reject_all,
            None,
            &compiler,
        ),
        program_wasm_cache_key_with_compiler_fingerprint(
            filesystem_source,
            ParseGoal::Script,
            &filesystem,
            None,
            &compiler,
        )
    );
}

#[test]
fn program_cache_key_changes_when_compiler_artifact_changes() {
    let options = CompileOptions::default();
    let identity = |source: &str, executable: &str| {
        CompilerIdentity::from_parts(
            CompilerDigest::parse(&source.repeat(32)).unwrap(),
            CompilerSourceRevision::UnversionedArchive,
            CompilerDigest::parse(&executable.repeat(32)).unwrap(),
        )
    };
    let before = compiler_identity_cache_fingerprint(&identity("11", "22"));
    assert_eq!(
        before,
        compiler_identity_cache_fingerprint(&identity("11", "22"))
    );
    let key = |compiler: &[u8; 32]| {
        program_wasm_cache_key_with_compiler_fingerprint(
            "1 + 2",
            ParseGoal::Script,
            &options,
            None,
            compiler,
        )
    };
    for changed in [identity("11", "33"), identity("44", "22")] {
        let after = compiler_identity_cache_fingerprint(&changed);
        assert_ne!(before, after);
        assert_ne!(key(&before), key(&after));
    }
}

#[test]
fn program_wasm_cache_reuses_emitted_artifact() {
    let cache_root = std::env::temp_dir().join(format!(
        "scriptw-engine-program-cache-reuse-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&cache_root);
    let cache = Arc::new(
        crate::cache::FunctionCache::new(cache_root.clone(), 64 * 1024 * 1024)
            .expect("program cache should initialize"),
    );
    let source = "true;";
    let options = CompileOptions {
        filename: Some("cache-reuse.js".to_string()),
        ..CompileOptions::default()
    };
    let key = program_wasm_cache_key(source, ParseGoal::Script, &options)
        .expect("loaded test compiler identity");
    let engine = engine();

    let first = run_on_sized_stack(|| {
        engine.load_or_compile_program_wasm_on_current_thread(
            source,
            ParseGoal::Script,
            options.clone(),
            Some(Arc::clone(&cache)),
        )
    })
    .expect("cache miss should emit Wasm");
    assert!(first.cached_entry.is_none());
    assert!(first.bytes.starts_with(b"\0asm"));
    assert!(cache.contains(&key));

    let second = engine
        .load_or_compile_program_wasm_on_current_thread(
            source,
            ParseGoal::Script,
            options,
            Some(Arc::clone(&cache)),
        )
        .expect("cache hit should load emitted Wasm");
    assert!(second.cached_entry.is_some());
    assert_eq!(second.bytes, first.bytes);

    let _ = std::fs::remove_dir_all(cache_root);
}

#[test]
fn program_wasm_cache_evicts_only_invalid_cached_artifacts() {
    let cache_root = std::env::temp_dir().join(format!(
        "scriptw-engine-program-cache-corruption-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&cache_root);
    let cache = Arc::new(
        crate::cache::FunctionCache::new(cache_root.clone(), 1024)
            .expect("program cache should initialize"),
    );
    // Cache admission now rejects malformed headers before an artifact exists.
    // Use an admitted empty module to test eviction's error classification;
    // these fixture bytes are never executed or counted as compiler acceptance.
    let source = "true;";
    let options = CompileOptions::default();
    let key = program_wasm_cache_key(source, ParseGoal::Script, &options)
        .expect("loaded test compiler identity");
    let malformed =
        wasm_runtime_link::encode_cache_entry(WasmProgramRef::standalone(&[0, 1, 2, 3]));
    assert!(wasm_runtime_link::decode_standalone_cache_entry(&malformed).is_none());
    let bytes = portable_cache_fixture();
    assert!(cache.write(
        &key,
        wasm_runtime_link::encode_cache_entry(WasmProgramRef::standalone(&bytes)),
    ));

    let artifact = engine()
        .load_or_compile_program_wasm_on_current_thread(
            source,
            ParseGoal::Script,
            options,
            Some(Arc::clone(&cache)),
        )
        .expect("cache hit should return the stored artifact");
    assert!(artifact.cached_entry.is_some());
    assert_eq!(artifact.bytes.as_ref(), bytes.as_slice());
    assert!(
        !artifact.evict_if_invalid(&EngineError::new("wasmtime execution trapped: unreachable"))
    );
    assert!(cache.contains(&key));
    assert!(artifact.evict_if_invalid(&EngineError::new(
        "wasmtime module validation failed: invalid magic"
    )));
    assert!(!cache.contains(&key));
    assert!(cache.write(
        &key,
        wasm_runtime_link::encode_cache_entry(WasmProgramRef::standalone(&bytes)),
    ));
    assert!(
        artifact.evict_if_invalid(&EngineError::from_intl_artifact_identity(
            IntlArtifactIdentityError::MissingSection,
        ))
    );
    assert!(!cache.contains(&key));

    let _ = std::fs::remove_dir_all(cache_root);
}

struct ModuleCacheFiles(std::path::PathBuf);

impl ModuleCacheFiles {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "scriptw-module-cache-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn options(&self) -> CompileOptions {
        CompileOptions {
            filename: Some(self.0.join("entry.js").to_str().unwrap().into()),
            module_root: Some(self.0.to_str().unwrap().into()),
            ..CompileOptions::default()
        }
    }

    fn write(&self, name: &str, source: &str) {
        std::fs::write(self.0.join(name), source).unwrap();
    }
}

impl Drop for ModuleCacheFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn closed_module_cache_hit_skips_lowering_and_rechecks_dependency_bytes() {
    run_on_sized_stack(|| {
        let files = ModuleCacheFiles::new("closed");
        let source = "import { value } from './dep.js'; export const doubled = value * 2;";
        files.write("dep.js", "export const value = 21;");
        let cache = Arc::new(
            crate::cache::FunctionCache::new(files.0.join("cache"), 256 * 1024 * 1024).unwrap(),
        );
        let engine = engine();
        let load = || {
            engine.load_or_compile_program_wasm_on_current_thread(
                source,
                ParseGoal::Module,
                files.options(),
                Some(Arc::clone(&cache)),
            )
        };
        let before = LOWERING_ATTEMPTS.with(|attempts| attempts.get());
        let first = load().expect("cold module should compile");
        assert!(first.cached_entry.is_none());
        let after_cold = LOWERING_ATTEMPTS.with(|attempts| attempts.get());
        assert_eq!(after_cold, before + 1);
        let warm = load().expect("closed graph should load from cache");
        assert!(warm.cached_entry.is_some());
        assert_eq!(first.bytes, warm.bytes);
        assert_eq!(
            LOWERING_ATTEMPTS.with(|attempts| attempts.get()),
            after_cold
        );

        files.write("dep.js", "export const value = 22;");
        let changed = load().expect("changed dependency should recompile");
        assert!(changed.cached_entry.is_none());
        assert_ne!(first.bytes, changed.bytes);
        assert_eq!(
            LOWERING_ATTEMPTS.with(|attempts| attempts.get()),
            after_cold + 1
        );

        files.write("dep.js", "export const value = ;");
        assert!(
            load().is_err(),
            "cache must not mask a dependency parse error"
        );
    });
}

#[test]
fn initial_module_cache_candidate_does_not_name_a_prepared_source_closure() {
    run_on_sized_stack(|| {
        let files = ModuleCacheFiles::new("prepared");
        let source = r#"new ShadowRealm().evaluate('void new ShadowRealm().importValue("./hidden.js", "value")'); export const value = 1;"#;
        files.write("hidden.js", "export const value = 42;");
        let options = files.options();
        let engine = engine();
        let role = module_loader::EmbeddedSourceRole::Entry;
        let parsed = engine
            .parse_compilation_for_role(source, ParseGoal::Module, &options, role)
            .unwrap();
        let candidate = program_cache_key_for_role(
            source,
            ParseGoal::Module,
            &options,
            parsed.modules.graph(),
            role,
        )
        .unwrap();
        let prepared = Engine::lower_parsed_compilation(parsed, &options).unwrap();
        let closed = program_cache_key_for_role(
            source,
            ParseGoal::Module,
            &options,
            prepared.modules.graph(),
            role,
        )
        .unwrap();
        assert_ne!(
            candidate, closed,
            "newly discovered authority must change the key"
        );
        assert!(
            prepared
                .modules
                .graph()
                .unwrap()
                .realm_requests
                .contains_key(&scriptw_ir::ModuleRequestKeyIr::plain("./hidden.js"))
        );
    });
}

#[test]
fn module_memory_cache_separates_native_compilation_modes() {
    let wasm = wasm_module_with_code_body_size(2);
    let source = WasmModuleSource::Program(&wasm);
    let fast_key = wasm_module_memory_cache_key(source, WasmNativeCompilationMode::Fast);
    let size_optimized_key =
        wasm_module_memory_cache_key(source, WasmNativeCompilationMode::SizeOptimized);

    assert_eq!(fast_key.wasm_sha256, size_optimized_key.wasm_sha256);
    assert_ne!(fast_key, size_optimized_key);
}

#[test]
fn current_thread_wasm_aot_entry_bypasses_memory_module_cache() {
    let source = "let currentThreadMemoryCacheBypass = 4815162342;";
    let unit = engine()
        .compile_script(source, CompileOptions::default())
        .expect("source should compile");
    let artifact = engine()
        .emit_wasm(&unit)
        .expect("compiled source should emit Wasm");
    assert!(
        !memory_wasm_module_is_cached(&artifact.bytes),
        "test artifact must not already be retained in the memory module cache"
    );

    run_on_sized_stack(|| {
        engine().run_wasm_aot_script_on_current_thread(
            source,
            CompileOptions::default(),
            None,
            true,
        )
    })
    .expect("current-thread Wasm-AOT run should succeed");

    assert!(
        !memory_wasm_module_is_cached(&artifact.bytes),
        "current-thread Wasm-AOT runs must not retain their module in the memory cache"
    );
}

#[test]
fn wasm_backend_caches_and_freezes_tagged_template_objects() {
    let outcome = engine()
        .run_script(
            r#"
"use strict";
let first;
let second;
function tag(template) {
  if (first === undefined) first = template;
  else second = template;
}
function useTemplate() {
  tag`cooked`;
}
useTemplate();
useTemplate();

let assignmentResult = "missing";
try {
  first.extra = true;
} catch (error) {
  assignmentResult = error.name;
}
let rawDescriptor = Object.getOwnPropertyDescriptor(first, "raw");
let indexDescriptor = Object.getOwnPropertyDescriptor(first, "0");
[
  first === second,
  assignmentResult,
  first[0],
  first.raw[0],
  Object.hasOwn(first, "0"),
  Reflect.ownKeys(first).includes("0"),
  Object.isFrozen(first),
  Object.isFrozen(first.raw),
  rawDescriptor.enumerable,
  rawDescriptor.writable,
  rawDescriptor.configurable,
  indexDescriptor.enumerable,
  indexDescriptor.writable,
  indexDescriptor.configurable
].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("tagged template objects should be cached and deeply frozen");
    assert!(
            outcome
                .note
                .contains("string(true|TypeError|cooked|cooked|true|true|true|true|false|false|false|true|false|false)"),
            "note: {}",
            outcome.note
        );
}

#[test]
fn wasm_backend_for_await_array_caches_array_iterator_prototype_next() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let calls = "";
            let source = [Promise.resolve(1), 2];
            let iteratorPrototype = Object.getPrototypeOf(source[Symbol.iterator]());
            let originalNext = iteratorPrototype.next;
            Object.defineProperty(iteratorPrototype, "next", {
                configurable: true,
                get: function () {
                    calls += "g";
                    return function () {
                        calls += "n";
                        return originalNext.call(this);
                    };
                }
            });
            async function collect() {
                let sum = 0;
                for await (const value of source) {
                    calls += value;
                    sum += value;
                }
                return sum;
            }
            collect().then(function (result) { print(result + ":" + calls); });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("array iterator next lookup should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["sync".to_string(), "3:gn1n2n".to_string()]
    );
}

#[test]
fn wasm_backend_for_await_sync_iterator_caches_next_and_awaits_values() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let source = r#"
            let nextGets = 0;
            let nextCalls = 0;
            let iterator = {};
            Object.defineProperty(iterator, "next", {
                get: function () {
                    nextGets += 1;
                    return function () {
                        nextCalls += 1;
                        if (nextCalls === 1) return { value: Promise.resolve(4), done: false };
                        if (nextCalls === 2) return { value: 5, done: false };
                        return { value: undefined, done: true };
                    };
                }
            });
            let iterable = {};
            iterable[Symbol.iterator] = function () { return iterator; };
            async function collect(iterable) {
                let total = 0;
                for await (const value of iterable) total += value;
                return total;
            }
            collect(iterable).then(function (value) {
                print(value + ":" + nextGets + ":" + nextCalls);
            });
            print("sync");
        "#;
    engine_with_captured_prints(Arc::clone(&lines))
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("sync iterator for-await-of should execute: {err:?}"));
    assert_eq!(
        lines.lock().expect("capture mutex poisoned").as_slice(),
        &["sync".to_string(), "9:1:3".to_string()]
    );
}
