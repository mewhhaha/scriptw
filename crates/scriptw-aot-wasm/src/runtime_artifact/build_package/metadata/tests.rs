use super::super::tests::{fixture as producer, SOURCE};
use super::*;
use scriptw_intl::{IntlCompilationProfile, IntlDataSelection};
use std::sync::OnceLock;

struct Fixture {
    metadata: Vec<u8>,
    intl: &'static [u8],
}
fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let selection = IntlDataSelection::new(IntlCompilationProfile::Minimal);
        let intl = Box::leak(
            selection
                .selected()
                .unwrap()
                .minimal_build_metadata(&SOURCE)
                .unwrap()
                .into_boxed_slice(),
        );
        Fixture {
            metadata: producer()
                .immutable_build_metadata(&selection, &SOURCE, intl)
                .unwrap(),
            intl,
        }
    })
}
fn decode(bytes: &[u8]) -> Option<BuildMetadata> {
    let f = fixture();
    assert!(std::ptr::eq(bytes, &f.metadata[..]));
    // SAFETY: the complete fixture producer owns these unchanged outputs.
    let inputs = unsafe {
        RuntimeArtifactInputs::new(None).with_immutable_minimal_build(
            producer().package_bytes(),
            &SOURCE,
            f.intl,
            &f.metadata,
        )
    };
    BuildMetadata::decode(
        inputs.immutable_metadata().unwrap(),
        RuntimeSourceCompilationProfile::Portable,
    )
}
fn rejects(bytes: &[u8]) -> bool {
    BuildMetadata::parser_rejects(
        bytes,
        producer().package_bytes(),
        &SOURCE,
        RuntimeSourceCompilationProfile::Portable,
        fixture().intl,
    )
}
fn resign(mut bytes: Vec<u8>) -> Vec<u8> {
    let end = bytes.len() - 32;
    let digest = Sha256::digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
    bytes
}

#[test]
fn receipt_metadata_preserves_complete_raw_layout_keys_and_actual_frame_views() {
    let f = fixture();
    let receipt = decode(&f.metadata).unwrap();
    assert_eq!(receipt.layout, *producer().runtime.layout());
    assert_eq!(receipt.wasm_digest, producer().runtime.wasm_sha256());
    for ((component, bytes), (name, expected)) in receipt.frames().into_iter().zip(
        IntlDataSelection::new(IntlCompilationProfile::Minimal)
            .compilation_data()
            .unwrap()
            .component_sections(),
    ) {
        assert_eq!(component.section_name(), name);
        assert_eq!(bytes, expected.as_ref());
    }
    let selection = IntlDataSelection::new(IntlCompilationProfile::Minimal);
    let identity = receipt.identity(&selection).unwrap().unwrap();
    assert_eq!(
        identity.digest(),
        cache::RuntimeIdentity::admit(&selection, RuntimeSourceCompilationProfile::Portable)
            .unwrap()
            .digest()
    );
    let runtime = receipt.into_runtime(&identity).unwrap();
    assert_eq!(runtime.bytes(), producer().wasm());
    assert_eq!(runtime.key(), producer().key());
    assert_eq!(
        runtime.immutable_build_package_digest(producer().package_bytes()),
        Some(Sha256::digest(producer().package_bytes()).into())
    );
    let copied = producer().package_bytes().to_vec();
    assert!(runtime.immutable_build_package_digest(&copied).is_none());
    assert_eq!(
        runtime.immutable_build_largest_body_bytes(),
        Some(receipt_largest_body())
    );
}
fn receipt_largest_body() -> usize {
    wasmparser::Parser::new(0)
        .parse_all(producer().wasm())
        .filter_map(|payload| match payload.unwrap() {
            wasmparser::Payload::CodeSectionEntry(body) => Some(body.range().len()),
            _ => None,
        })
        .max()
        .unwrap()
}

#[test]
fn receipt_metadata_binds_canonical_pool_contents_not_only_dimensions() {
    let receipt = decode(&fixture().metadata).unwrap();
    let utf16 = &receipt.package[receipt.data[0].span.clone()];
    let data = &receipt.package[receipt.data[1].span.clone()];
    let selection = IntlDataSelection::new(IntlCompilationProfile::Minimal);
    let identity = receipt.identity(&selection).unwrap().unwrap();
    let runtime = receipt.into_runtime(&identity).unwrap();
    assert_eq!(
        runtime.admitted_compiler_pool_matches(utf16, data),
        Some(true)
    );
    let mut wrong = data.to_vec();
    wrong[0] ^= 1;
    assert_eq!(
        runtime.admitted_compiler_pool_matches(utf16, &wrong),
        Some(false)
    );
    assert_eq!(
        runtime.admitted_compiler_pool_matches(&utf16[..utf16.len() - 1], data),
        Some(false)
    );
}

#[test]
fn receipt_metadata_rejects_truncation_tampering_source_profile_and_overlapping_spans() {
    let f = fixture();
    for end in [0, 7, 8, 11, 44, f.metadata.len() - 33, f.metadata.len() - 1] {
        assert!(rejects(&f.metadata[..end]));
    }
    let mut wrong = f.metadata.clone();
    wrong[12] ^= 1;
    assert!(rejects(&wrong));
    assert!(rejects(&resign(wrong)));
    assert!(BuildMetadata::parser_rejects(
        &f.metadata,
        producer().package_bytes(),
        &SOURCE,
        RuntimeSourceCompilationProfile::NativeHost,
        f.intl
    ));
    assert!(BuildMetadata::parser_rejects(
        &f.metadata,
        producer().package_bytes(),
        &SOURCE,
        RuntimeSourceCompilationProfile::Portable,
        b"other metadata"
    ));
    let mut wrong = f.metadata.clone();
    let last_frame_start = wrong.len() - 32 - 50 + 2;
    wrong[last_frame_start..last_frame_start + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(rejects(&resign(wrong)));
    let mut wrong = f.metadata.clone();
    let first_frame_start = wrong.len() - 32 - 12 * 50 + 2;
    let first = wrong[first_frame_start..first_frame_start + 8].to_vec();
    wrong[last_frame_start..last_frame_start + 8].copy_from_slice(&first);
    assert!(rejects(&resign(wrong)));
}

#[test]
fn receipt_metadata_is_reused_only_through_the_unsafe_exact_producer_candidate() {
    let f = fixture();
    let metadata: &'static [u8] = &f.metadata;
    // SAFETY: all three arrays are unchanged outputs of the complete static
    // producer fixture; no caller/cache bytes or synthesized layout enter it.
    let inputs = unsafe {
        RuntimeArtifactInputs::new(None).with_immutable_minimal_build(
            producer().package_bytes(),
            &SOURCE,
            f.intl,
            metadata,
        )
    };
    let selection = inputs
        .intl_selection(&IntlCompilationProfile::Minimal)
        .unwrap();
    let runtime = inputs.admit_supplied_package(&selection).unwrap();
    assert!(runtime.build_admission.is_some());
    assert_eq!(runtime.key(), producer().key());
    let validated =
        crate::ValidatedRuntimeArtifact::new(runtime, IntlCompilationProfile::Minimal).unwrap();
    assert!(Arc::ptr_eq(
        validated.selection().compilation_data().unwrap(),
        selection.compilation_data().unwrap()
    ));
    let custom = IntlDataSelection::new(IntlCompilationProfile::Custom(
        scriptw_intl::CustomProfileId::parse("receipt-other-profile").unwrap(),
    ));
    assert!(decode(metadata)
        .unwrap()
        .identity(&custom)
        .unwrap()
        .is_none());
}
