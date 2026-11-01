use super::*;

#[test]
fn every_catalog_row_matches_uncached_native_property_resolution() {
    let catalog = regexp_unicode_property_catalog();
    assert!(catalog
        .windows(2)
        .all(|rows| rows[0].name() < rows[1].name()));
    let mut string_names = BTreeSet::new();
    for row in catalog {
        match row.value() {
            RegExpUnicodePropertyCatalogValue::CodePoints(ranges) => {
                let native = unicode_property_ranges(row.name()).expect(row.name());
                assert_eq!(ranges, normalize_ranges(native), "{}", row.name());
            }
            RegExpUnicodePropertyCatalogValue::Strings(sequences) => {
                let property = unicode_string_property_from_str(row.name()).expect(row.name());
                assert_eq!(
                    sequences,
                    validated_unicode_string_property_sequences(property),
                    "{}",
                    row.name()
                );
                string_names.insert(row.name());
            }
        }
    }
    assert_eq!(string_names.len(), 7);
    assert_eq!(
        string_names,
        UnicodeStringProperty::ALL
            .iter()
            .map(|property| property.name())
            .collect()
    );

    let ranges = |name: &str| {
        let row = catalog.iter().find(|row| row.name() == name).expect(name);
        let RegExpUnicodePropertyCatalogValue::CodePoints(ranges) = row.value() else {
            panic!("{name} must be a code-point property");
        };
        ranges
    };
    // Shared ICU bases must retain the distinct Script/Extensions domains.
    assert!(!ranges_contain(ranges("sc=Hira"), 0x30fc));
    assert!(ranges_contain(ranges("scx=Hira"), 0x30fc));
    for (name, alias, witness) in [
        ("Arabic", "Arab", 0x088f), // Unicode 17 addition to an ICU Script.
        ("Beria_Erfe", "Berf", 0x16ea0),
        ("Sidetic", "Sidt", 0x10940),
        ("Tai_Yo", "Tayo", 0x1e6c0),
        ("Tolong_Siki", "Tols", 0x11db0),
    ] {
        for (family, short_family) in [("Script", "sc"), ("Script_Extensions", "scx")] {
            let canonical = format!("{family}={name}");
            let base = RegExpCodePointProperty::parse(&canonical)
                .unwrap()
                .icu_ranges();
            if name == "Arabic" {
                assert!(!ranges_contain(&base.unwrap(), witness));
            } else {
                assert!(base.is_none(), "{canonical} must use the Unicode 17 owner");
            }
            for spelling in [
                format!("{family}={alias}"),
                format!("{short_family}={name}"),
                format!("{short_family}={alias}"),
            ] {
                assert_eq!(ranges(&canonical), ranges(&spelling), "{spelling}");
            }
            assert!(ranges_contain(ranges(&canonical), witness), "{canonical}");
        }
    }
    let cased_base = RegExpCodePointProperty::parse("Cased")
        .unwrap()
        .icu_ranges()
        .unwrap();
    assert!(ranges_contain(&cased_base, 0x0295));
    assert!(!ranges_contain(ranges("Cased"), 0x0295));
}

#[test]
fn catalog_requests_each_distinct_icu_script_family_once() {
    let mut calls = BTreeMap::new();
    let catalog = build_unicode_property_catalog(|script, family| {
        *calls.entry((family, script)).or_insert(0usize) += 1;
        script_ranges(script, family)
    });
    let mut expected = BTreeSet::new();
    let mut script_aliases = 0;
    for row in &catalog {
        let Some(property) = RegExpCodePointProperty::parse(row.name()) else {
            continue; // Properties of strings have a separate native owner.
        };
        if let CodePointPropertyKind::Script {
            family,
            value: ScriptPropertyValue::PinnedIcu(script),
        } = property.kind
        {
            expected.insert((family, script));
            script_aliases += 1;
        }
    }
    assert!(script_aliases > expected.len() * 2);
    assert_eq!(calls.keys().copied().collect::<BTreeSet<_>>(), expected);
    assert!(calls.values().all(|&count| count == 1));
    let hira = PropertyParser::<Script>::new().get_strict("Hira").unwrap();
    assert!(calls.contains_key(&(ScriptPropertyFamily::Script, hira)));
    assert!(calls.contains_key(&(ScriptPropertyFamily::Extensions, hira)));
}
