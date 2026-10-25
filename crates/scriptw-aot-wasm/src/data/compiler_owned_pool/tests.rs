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

#[test]
fn canonical_prefix_admission_checks_actual_data_bytes_not_just_the_boundary() {
    let prefix = prefix();
    let mut data = DataSection::new();
    prefix.0.append_runtime_data(&mut data);
    let mut exact = Module::new();
    exact.section(&data);
    prefix.require_runtime_data(&exact.finish()).unwrap();
    let mut corrupt = prefix.0.bytes[..].to_vec();
    let descriptors = prefix.0.runtime_string_descriptors().unwrap();
    corrupt[(descriptors.address() - STATIC_DATA_OFFSET) as usize] ^= 1;
    let mut data = DataSection::new();
    data.passive(prefix.0.pooled_code_unit_bytes[..].iter().copied());
    data.active(0, &ConstExpr::i32_const(STATIC_DATA_OFFSET as i32), corrupt);
    let mut wrong = Module::new();
    wrong.section(&data);
    assert!(prefix.require_runtime_data(&wrong.finish()).is_err());
    assert!(prefix
        .require_runtime_data(&Module::new().finish())
        .is_err());
}
