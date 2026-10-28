use super::*;
use scriptw_front::{parse, ParseOptions};
use scriptw_intl::{IntlCompilationProfile, IntlDataSelection};
use wasm_encoder::{ConstExpr, DataSection, Module};

fn script(source: &str) -> ScriptIr {
    scriptw_ir::lower(&parse(source, ParseOptions::script()).unwrap())
        .script
        .unwrap()
}

fn prefix() -> CompilerOwnedPool {
    CompilerOwnedPool::collect(
        &BTreeMap::new(),
        &[],
        false,
        &IntlDataSelection::new(IntlCompilationProfile::Minimal),
    )
    .unwrap()
}

fn freeze(mut prefix: CompilerOwnedPool) -> Arc<CompilerOwnedPool> {
    prefix.0.bytes.freeze();
    prefix.0.pooled_code_unit_bytes.freeze();
    prefix.0.refs.freeze();
    Arc::new(prefix)
}

#[test]
fn program_appendices_share_the_exact_immutable_prefix_storage() {
    let prefix = freeze(prefix());
    let first = CompilerOwnedPool::collect_program(
        &prefix,
        &script("let first = 'first-local'; first;"),
        &BTreeMap::new(),
    )
    .unwrap();
    let second = CompilerOwnedPool::collect_program(
        &prefix,
        &script("let second = 'second-local'; second;"),
        &BTreeMap::new(),
    )
    .unwrap();
    let boundary = prefix.0.compiler_owned;
    for pool in [&first, &second] {
        assert!(Arc::ptr_eq(pool.shared_prefix.as_ref().unwrap(), &prefix));
        assert_eq!(
            pool.bytes[..boundary.static_bytes].as_ptr(),
            prefix.0.bytes[..].as_ptr()
        );
        assert_eq!(
            pool.pooled_code_unit_bytes[..boundary.code_unit_bytes].as_ptr(),
            prefix.0.pooled_code_unit_bytes[..].as_ptr()
        );
        assert!(std::ptr::eq(
            pool.refs.get("undefined").unwrap(),
            prefix.0.refs.get("undefined").unwrap()
        ));
        assert_eq!(pool.compiler_owned_boundary(), boundary);
        assert_eq!(
            pool.runtime_string_descriptors().unwrap(),
            prefix.0.runtime_string_descriptors().unwrap()
        );
    }
    assert_ne!(first.program_utf16_bytes(), second.program_utf16_bytes());
    assert!(first.pooled_string_index("second-local").is_err());
    assert!(second.pooled_string_index("first-local").is_err());
    assert!(prefix.0.pooled_string_index("first-local").is_err());
}

#[test]
fn regexp_literals_and_static_constructors_copy_an_r_descriptor_once_into_p() {
    let mut prefix = prefix();
    let program = RegExpProgram::compile("a", "").unwrap();
    let descriptor = ValidatedRegExpProgram::from_program(&program).unwrap();
    prefix.0.queue_regexp_program(&program);
    prefix.0.append_regexp_programs();
    let runtime_reference = prefix.0.regexp_program(&program);
    prefix.0.compiler_owned = prefix.0.boundary();
    let runtime_bytes = prefix.0.bytes.as_slice().to_vec();
    let prefix = freeze(prefix);
    for source in [
        "var literal = /a/; literal;",
        "new RegExp('a', '');",
        "var literal = /a/; new RegExp('a', ''); /a/;",
    ] {
        let pool =
            CompilerOwnedPool::collect_program(&prefix, &script(source), &BTreeMap::new()).unwrap();
        let reference = pool.regexp_program(&program);
        let span = reference
            .metadata_span(&pool)
            .expect("P-owned descriptor span");
        assert_ne!(reference.payload(), runtime_reference.payload());
        assert_eq!(
            &pool.program_metadata_bytes()
                [span.start() as usize..(span.start() + span.length()) as usize],
            descriptor.bytes(),
            "literal/static constructor bytes and ordering must match native validation"
        );
        assert_eq!(
            pool.program_metadata_bytes()
                .windows(descriptor.bytes().len())
                .filter(|bytes| *bytes == descriptor.bytes())
                .count(),
            1,
            "literal and constructor must share one P copy: {source}"
        );
        assert_eq!(prefix.0.bytes.as_slice(), runtime_bytes.as_slice());
        assert!(runtime_reference.metadata_span(&pool).is_err());
    }
}

#[test]
fn shared_program_images_and_ordinals_match_fresh_collection_with_exact_utf16() {
    let prefix = freeze(prefix());
    for source in [
        "let local = 'undefined'; local;",
        "let local = '\\ud800\\u0000\\udfff'; local;",
        "let local = ['local-a', ['local-b', 'undefined']]; local;",
        "let local = /a(b)\\1/i; local;",
    ] {
        let script = script(source);
        let shared =
            CompilerOwnedPool::collect_program(&prefix, &script, &BTreeMap::new()).unwrap();
        let fresh = StringPool::collect(
            &script,
            &BTreeMap::new(),
            &[],
            false,
            &IntlDataSelection::new(IntlCompilationProfile::Minimal),
        )
        .unwrap();
        assert_eq!(
            shared.compiler_owned_boundary(),
            fresh.compiler_owned_boundary()
        );
        assert_eq!(shared.pooled_string_count(), fresh.pooled_string_count());
        assert_eq!(
            shared.runtime_string_descriptors().unwrap(),
            fresh.runtime_string_descriptors().unwrap()
        );
        assert_eq!(shared.program_utf16_bytes(), fresh.program_utf16_bytes());
        assert_eq!(
            shared.program_metadata_bytes(),
            fresh.program_metadata_bytes()
        );
        assert_eq!(shared.program_recipe_bytes(), fresh.program_recipe_bytes());
        let mut shared_data = DataSection::new();
        let mut fresh_data = DataSection::new();
        shared.append_program_data(&mut shared_data);
        fresh.append_program_data(&mut fresh_data);
        let mut shared_module = Module::new();
        let mut fresh_module = Module::new();
        shared_module.section(&shared_data);
        fresh_module.section(&fresh_data);
        assert_eq!(shared_module.finish(), fresh_module.finish());
        for name in ["undefined", "local", "local-a", "local-b"] {
            if let Ok(index) = fresh.pooled_string_index(name) {
                assert_eq!(
                    shared.pooled_string_index(name).unwrap().ordinal(),
                    index.ordinal()
                );
                assert_eq!(shared.payload(name), fresh.payload(name));
            }
        }
    }
}

fn exact_bytes_for_corruption(prefix: &CompilerOwnedPool) -> Vec<u8> {
    let mut data = DataSection::new();
    prefix.0.append_runtime_data(&mut data);
    let mut module = Module::new();
    module.section(&data);
    module.finish()
}

fn descriptor_payload_byte(bytes: &[u8], address: u32) -> usize {
    use wasmparser::{DataKind, Operator, Parser, Payload};
    let mut found = None;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::DataSection(section) = payload.unwrap() {
            for row in section {
                let row = row.unwrap();
                if let DataKind::Active {
                    memory_index: 0,
                    offset_expr,
                } = row.kind
                {
                    let mut operators = offset_expr.get_operators_reader();
                    let Operator::I32Const { value } = operators.read().unwrap() else {
                        panic!("constant R address");
                    };
                    assert!(matches!(operators.read().unwrap(), Operator::End) && operators.eof());
                    if let Some(displacement) = address
                        .checked_sub(value as u32)
                        .map(|n| n as usize)
                        .filter(|n| *n < row.data.len())
                    {
                        let payload_start = row.range.end - row.data.len();
                        assert!(
                            found.replace(payload_start + displacement).is_none(),
                            "descriptor has one actual active owner"
                        );
                    }
                }
            }
        }
    }
    found.expect("actual encoded R descriptor payload")
}

#[test]
fn canonical_prefix_admission_checks_actual_data_bytes_not_just_the_boundary() {
    let prefix = prefix();
    let mut data = DataSection::new();
    prefix.0.append_runtime_data(&mut data);
    let mut exact = Module::new();
    exact.section(&data);
    prefix.require_runtime_data(&exact.finish()).unwrap();
    let mut wrong_bytes = exact_bytes_for_corruption(&prefix);
    // Retain the exact descriptor-byte corruption control across segment layouts.
    // Locate its actual encoded payload instead of rebuilding a parallel layout.
    let descriptors = prefix.0.runtime_string_descriptors().unwrap();
    let descriptor_byte = descriptor_payload_byte(&wrong_bytes, descriptors.address());
    wrong_bytes[descriptor_byte] ^= 1;
    assert!(prefix.require_runtime_data(&wrong_bytes).is_err());
    assert!(prefix
        .require_runtime_data(&Module::new().finish())
        .is_err());
}

#[test]
fn normalization_region_is_passive_and_active_fragments_preserve_exact_virtual_addresses() {
    use wasmparser::{DataKind, Operator, Parser, Payload};
    for temporal in [false, true] {
        for normalization in [false, true] {
            let builtins = if normalization {
                &[StandardBuiltinId::StringPrototypeNormalize][..]
            } else {
                &[]
            };
            let prefix = CompilerOwnedPool::collect(
                &BTreeMap::new(),
                builtins,
                temporal,
                &IntlDataSelection::new(IntlCompilationProfile::Minimal),
            )
            .unwrap();
            let region = prefix.0.normalization_image_region();
            let span = region.offsets();
            assert_eq!(region.length() > 0, normalization);
            let mut data = DataSection::new();
            prefix.0.append_runtime_data(&mut data);
            let mut module = Module::new();
            module.section(&data);
            let bytes = module.finish();
            let mut rows = Vec::new();
            for payload in Parser::new(0).parse_all(&bytes) {
                if let Payload::DataSection(section) = payload.unwrap() {
                    rows.extend(section.into_iter().map(Result::unwrap));
                }
            }
            assert_eq!(rows.len(), RUNTIME_DATA_SEGMENT_COUNT);
            let years = prefix.0.temporal_east_asian_year_region();
            let year = years.offsets();
            assert_eq!(year.len(), if temporal { 133_712 } else { 0 });
            assert!(matches!(
                rows[years.segment().index() as usize].kind,
                DataKind::Passive
            ));
            assert_eq!(
                rows[years.segment().index() as usize].data,
                &prefix.0.bytes[year.clone()]
            );
            assert!(matches!(
                &rows[PooledCodeUnits::DATA_SEGMENT.index() as usize].kind,
                DataKind::Passive
            ));
            assert!(matches!(
                &rows[region.segment().index() as usize].kind,
                DataKind::Passive
            ));
            assert_eq!(
                rows[region.segment().index() as usize].data,
                &prefix.0.bytes[span.clone()]
            );
            let mut restored = vec![0u8; prefix.0.bytes.len()];
            let mut active_bytes = 0;
            for row in &rows {
                if let DataKind::Active {
                    memory_index: 0,
                    offset_expr,
                } = &row.kind
                {
                    let Operator::I32Const { value } =
                        offset_expr.get_operators_reader().read().unwrap()
                    else {
                        panic!("constant address");
                    };
                    let offset = value as usize - STATIC_DATA_OFFSET as usize;
                    let end = offset + row.data.len();
                    assert!(
                        (end <= year.start || offset >= year.end)
                            && (end <= span.start || offset >= span.end)
                            && {
                                let property = prefix.0.regexp_unicode_property_region().offsets();
                                end <= property.start || offset >= property.end
                            },
                        "no active copy may touch the deferred span, including zero placeholders"
                    );
                    restored[offset..end].copy_from_slice(row.data);
                    active_bytes += row.data.len();
                }
            }
            let properties = prefix.0.regexp_unicode_property_region();
            let property = properties.offsets();
            assert!(property.len() > 0);
            assert!(matches!(
                rows[properties.segment().index() as usize].kind,
                DataKind::Passive
            ));
            assert_eq!(
                rows[properties.segment().index() as usize].data,
                &prefix.0.bytes[property.clone()]
            );
            assert_eq!(
                active_bytes + span.len() + property.len() + year.len(),
                restored.len()
            );
            restored[year.clone()].copy_from_slice(rows[years.segment().index() as usize].data);
            restored[property.clone()]
                .copy_from_slice(rows[properties.segment().index() as usize].data);
            restored[span.clone()].copy_from_slice(rows[region.segment().index() as usize].data);
            assert_eq!(restored, &prefix.0.bytes[..]);
            prefix.require_runtime_data(&bytes).unwrap();
            if temporal {
                let mut corrupted = bytes.clone();
                let row = &rows[years.segment().index() as usize];
                let calendar_byte = row.range.end - row.data.len();
                corrupted[calendar_byte] ^= 1;
                assert!(
                    prefix.require_runtime_data(&corrupted).is_err(),
                    "calendar bytes must be admitted exactly"
                );
            }
            assert_eq!(
                initial_memory_pages(prefix.0.bytes.len(), true),
                (align_heap_start(prefix.0.bytes.len()) + WASM_PAGE_SIZE).div_ceil(WASM_PAGE_SIZE)
            );
        }
    }
}
