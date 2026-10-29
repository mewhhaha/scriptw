//! Canonical identity bytes without runtime sorting or formatting machinery.
use super::*;

const fn name_precedes(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut index = 0;
    while index < left.len() && index < right.len() {
        if left[index] != right[index] {
            return left[index] < right[index];
        }
        index += 1;
    }
    left.len() < right.len()
}

// Derive both wire orders from their closed domains, so an added or renamed
// member enters the canonical order without maintaining a second name list.
macro_rules! alphabetical_names {
    ($domain:ty) => {{
        let mut names = [(<$domain>::ALL[0], <$domain>::ALL[0].name()); <$domain>::ALL.len()];
        let mut index = 0;
        while index < names.len() {
            let member = <$domain>::ALL[index];
            let mut destination = index;
            while destination > 0 && name_precedes(member.name(), names[destination - 1].1) {
                names[destination] = names[destination - 1];
                destination -= 1;
            }
            names[destination] = (member, member.name());
            index += 1;
        }
        names
    }};
}

const SERVICES: [(IntlService, &str); IntlService::ALL.len()] = alphabetical_names!(IntlService);
const CAPABILITIES: [(IntlDataCapability, &str); IntlDataCapability::ALL.len()] =
    alphabetical_names!(IntlDataCapability);

fn names_length<T: Copy>(names: &[(T, &str)], selected: impl Fn(T) -> bool) -> usize {
    let mut length = 0;
    let mut first = true;
    for &(member, name) in names {
        if selected(member) {
            length += usize::from(!first) + name.len();
            first = false;
        }
    }
    length
}

fn append_names<T: Copy>(bytes: &mut Vec<u8>, names: &[(T, &str)], selected: impl Fn(T) -> bool) {
    let mut first = true;
    for &(member, name) in names {
        if selected(member) {
            if !first {
                bytes.push(b',');
            }
            bytes.extend_from_slice(name.as_bytes());
            first = false;
        }
    }
    bytes.push(b'\n');
}

fn decimal_length(mut value: u16) -> usize {
    let mut length = 1;
    while value >= 10 {
        value /= 10;
        length += 1;
    }
    length
}

fn append_decimal(bytes: &mut Vec<u8>, mut value: u16) {
    let mut digits = [0; 5];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    bytes.extend_from_slice(&digits[start..]);
    bytes.push(b'\n');
}

fn append_field(bytes: &mut Vec<u8>, key: &[u8], value: &[u8]) {
    bytes.extend_from_slice(key);
    bytes.extend_from_slice(value);
    bytes.push(b'\n');
}

pub(super) fn encode(identity: &IntlDataIdentity) -> IntlArtifactIdentity {
    let services = identity.profile.services();
    let capabilities = identity.profile.capabilities();
    let (profile, custom) = match identity.profile.profile() {
        IntlDataProfile::Conformance => ("conformance", ""),
        IntlDataProfile::Minimal => ("minimal", ""),
        IntlDataProfile::Custom(id) => ("custom:", id.as_str()),
    };
    let placement = match identity.placement {
        IntlDataPlacement::Embedded => b"embedded",
        IntlDataPlacement::External => b"external",
    };
    let versions = identity.versions;
    let version_fields: [(&[u8], &str); 6] = [
        (b"icu4x=", versions.icu4x),
        (b"cldr=", versions.cldr),
        (b"unicode=", versions.unicode),
        (b"icu-data-tag=", versions.icu_data_tag),
        (b"segmenter-lstm=", versions.segmenter_lstm),
        (b"tzdb=", versions.tzdb),
    ];
    let supplement = services.contains(IntlService::Locale)
        || services.contains(IntlService::NumberFormat)
        || services.contains(IntlService::DateTimeFormat)
        || services.contains(IntlService::RelativeTimeFormat)
        || services.contains(IntlService::DurationFormat);
    let supplement_bytes = number_format::NUMBERING_SUPPLEMENT_PROVENANCE.as_bytes();
    // Include every key/newline and variable field once, keeping the final
    // Box conversion from requiring a second allocation or a shrink.
    let capacity = b"schema=\nhost-call-abi=\nprofile=\nservices=\ncapabilities=\ndefault-locale=\nplacement=\ndigest=\nicu4x=\ncldr=\nunicode=\nicu-data-tag=\nsegmenter-lstm=\ntzdb=\n".len()
        + decimal_length(identity.schema.get())
        + decimal_length(INTL_HOST_CALL_ABI_VERSION)
        + profile.len()
        + custom.len()
        + names_length(&SERVICES, |member| services.contains(member))
        + names_length(&CAPABILITIES, |member| capabilities.contains(member))
        + identity.default_locale.as_str().len()
        + placement.len()
        + 64
        + version_fields.iter().map(|(_, value)| value.len()).sum::<usize>()
        + if supplement { b"numbering-supplement=\n".len() + supplement_bytes.len() } else { 0 };
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(b"schema=");
    append_decimal(&mut bytes, identity.schema.get());
    bytes.extend_from_slice(b"host-call-abi=");
    append_decimal(&mut bytes, INTL_HOST_CALL_ABI_VERSION);
    bytes.extend_from_slice(b"profile=");
    bytes.extend_from_slice(profile.as_bytes());
    bytes.extend_from_slice(custom.as_bytes());
    bytes.push(b'\n');
    bytes.extend_from_slice(b"services=");
    append_names(&mut bytes, &SERVICES, |member| services.contains(member));
    bytes.extend_from_slice(b"capabilities=");
    append_names(&mut bytes, &CAPABILITIES, |member| {
        capabilities.contains(member)
    });
    append_field(
        &mut bytes,
        b"default-locale=",
        identity.default_locale.as_str().as_bytes(),
    );
    append_field(&mut bytes, b"placement=", placement);
    bytes.extend_from_slice(b"digest=");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &byte in identity.digest.as_bytes() {
        bytes.push(HEX[usize::from(byte >> 4)]);
        bytes.push(HEX[usize::from(byte & 15)]);
    }
    bytes.push(b'\n');
    for (key, value) in version_fields {
        append_field(&mut bytes, key, value.as_bytes());
    }
    if supplement {
        append_field(&mut bytes, b"numbering-supplement=", supplement_bytes);
    }
    IntlArtifactIdentity(bytes.into_boxed_slice())
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write as _;

    // Preserve the previous general serializer as a wire-format oracle only.
    fn reference(identity: &IntlDataIdentity) -> IntlArtifactIdentity {
        let mut services = identity
            .profile
            .services()
            .iter()
            .map(IntlService::name)
            .collect::<Vec<_>>();
        services.sort_unstable();
        let mut capabilities = identity
            .profile
            .capabilities()
            .iter()
            .map(IntlDataCapability::name)
            .collect::<Vec<_>>();
        capabilities.sort_unstable();
        let profile = match identity.profile.profile() {
            IntlDataProfile::Conformance => "conformance".to_string(),
            IntlDataProfile::Minimal => "minimal".to_string(),
            IntlDataProfile::Custom(id) => format!("custom:{}", id.as_str()),
        };
        let placement = match identity.placement {
            IntlDataPlacement::Embedded => "embedded",
            IntlDataPlacement::External => "external",
        };
        let mut digest = String::with_capacity(64);
        for byte in identity.digest.as_bytes() {
            write!(digest, "{byte:02x}").expect("writing to a String cannot fail");
        }
        let versions = identity.versions;
        let mut bytes = format!(
            concat!(
                "schema={}\n",
                "host-call-abi={}\n",
                "profile={}\n",
                "services={}\n",
                "capabilities={}\n",
                "default-locale={}\n",
                "placement={}\n",
                "digest={}\n",
                "icu4x={}\n",
                "cldr={}\n",
                "unicode={}\n",
                "icu-data-tag={}\n",
                "segmenter-lstm={}\n",
                "tzdb={}\n",
            ),
            identity.schema.get(),
            INTL_HOST_CALL_ABI_VERSION,
            profile,
            services.join(","),
            capabilities.join(","),
            identity.default_locale.as_str(),
            placement,
            digest,
            versions.icu4x,
            versions.cldr,
            versions.unicode,
            versions.icu_data_tag,
            versions.segmenter_lstm,
            versions.tzdb,
        )
        .into_bytes();
        if identity.profile.services().contains(IntlService::Locale)
            || identity
                .profile
                .services()
                .contains(IntlService::NumberFormat)
            || identity
                .profile
                .services()
                .contains(IntlService::DateTimeFormat)
            || identity
                .profile
                .services()
                .contains(IntlService::RelativeTimeFormat)
            || identity
                .profile
                .services()
                .contains(IntlService::DurationFormat)
        {
            bytes.extend_from_slice(b"numbering-supplement=");
            bytes.extend_from_slice(number_format::NUMBERING_SUPPLEMENT_PROVENANCE.as_bytes());
            bytes.push(b'\n');
        }
        IntlArtifactIdentity(bytes.into_boxed_slice())
    }

    fn identity(plan: IntlProfilePlan, placement: IntlDataPlacement) -> IntlDataIdentity {
        IntlDataIdentity::new(
            plan,
            CanonicalLocaleId::from_data("en-US").unwrap(),
            placement,
            IntlDataDigest::from_sha256([0x5a; 32]),
        )
    }

    fn assert_reference(identity: &IntlDataIdentity) {
        assert_eq!(identity.artifact_identity(), reference(identity));
    }

    #[test]
    fn every_service_mask_preserves_minimal_and_custom_wire_bytes() {
        for mask in 1..(1u16 << IntlService::ALL.len()) {
            let services = IntlServiceSet(mask);
            for plan in [
                IntlProfilePlan::minimal(services).unwrap(),
                IntlProfilePlan::custom(
                    CustomProfileId::parse("Wire.identity-0_9").unwrap(),
                    services,
                )
                .unwrap(),
            ] {
                for placement in [IntlDataPlacement::Embedded, IntlDataPlacement::External] {
                    assert_reference(&identity(plan.clone(), placement));
                }
            }
        }
    }

    #[test]
    fn singleton_and_combined_capabilities_preserve_alphabetic_wire_order() {
        // Exercise the serializer's complete capability vocabulary separately
        // from whichever closures current service and operation plans require.
        let mut sets = vec![IntlCapabilitySet::EMPTY, IntlCapabilitySet::ALL];
        let mut combined = IntlCapabilitySet::EMPTY;
        for &capability in IntlDataCapability::ALL {
            sets.push(IntlCapabilitySet::EMPTY.with(capability));
            combined = combined.with(capability);
            sets.push(combined);
        }
        for capabilities in sets {
            for service in [IntlService::Collator, IntlService::Locale] {
                let plan = IntlProfilePlan {
                    profile: IntlDataProfile::Minimal,
                    services: IntlServiceSet::EMPTY.with(service),
                    capabilities,
                };
                assert_reference(&identity(plan, IntlDataPlacement::External));
            }
        }
    }

    #[test]
    fn digest_locale_versions_and_numeric_boundaries_preserve_wire_bytes() {
        for byte in 0..=u8::MAX {
            let mut current = identity(IntlProfilePlan::conformance(), IntlDataPlacement::Embedded);
            current.digest = IntlDataDigest::from_sha256([byte; 32]);
            assert_reference(&current);
        }
        for locale in ["en", "zh-Hant-TW-u-nu-hanidec", "de-DE-x-private", "x-test"] {
            for placement in [IntlDataPlacement::Embedded, IntlDataPlacement::External] {
                let mut current = identity(IntlProfilePlan::conformance(), placement);
                current.default_locale = CanonicalLocaleId::from_data(locale).unwrap();
                current.versions = IntlDataVersions {
                    icu4x: "20.10",
                    cldr: "999.0.1",
                    unicode: "1",
                    icu_data_tag: "future/版本",
                    segmenter_lstm: "v9-beta",
                    tzdb: "9999z",
                };
                for schema in [0, 1, 9, 10, 99, 100, 999, 1000, 9999, 10000, u16::MAX] {
                    current.schema = IntlDataSchemaVersion(schema);
                    assert_reference(&current);
                }
            }
        }
        for custom in ["a".to_owned(), "9".to_owned(), "A".repeat(64)] {
            let plan = IntlProfilePlan::custom(
                CustomProfileId::parse(custom).unwrap(),
                IntlServiceSet::EMPTY.with(IntlService::Segmenter),
            )
            .unwrap();
            assert_reference(&identity(plan, IntlDataPlacement::Embedded));
        }
    }
}
