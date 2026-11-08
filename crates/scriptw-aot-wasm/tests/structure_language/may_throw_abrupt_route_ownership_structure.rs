use std::fs;
use std::path::Path;

const OPERATIONS_SOURCE: &str = include_str!("../../src/operations.rs");
const MATH_SOURCE: &str = include_str!("../../src/builtins/math.rs");
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/may-throw-operation-abrupt-route-ownership.md"
);

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn normalized(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .flat_map(str::chars)
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn exact_identifier_count(source: &str, identifier: &str) -> usize {
    source
        .match_indices(identifier)
        .filter(|(offset, _)| {
            let before = source[..*offset].chars().next_back();
            let after = source[*offset + identifier.len()..].chars().next();
            [before, after].into_iter().all(|edge| {
                edge.map(|character| !character.is_alphanumeric() && character != '_')
                    .unwrap_or(true)
            })
        })
        .count()
}

fn recursive_rust_identifier_count(root: &Path, identifier: &str) -> usize {
    fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()))
        .map(|entry| entry.expect("failed to read Rust source entry").path())
        .map(|path| {
            if path.is_dir() {
                return recursive_rust_identifier_count(&path, identifier);
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                return 0;
            }
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            exact_identifier_count(&source, identifier)
        })
        .sum()
}

#[test]
fn generic_abrupt_route_authority_is_absent() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(
        recursive_rust_identifier_count(&source_root, "AbruptRoute"),
        0
    );
    assert_eq!(
        recursive_rust_identifier_count(&source_root, "finish_may_throw_operation"),
        0
    );
}

#[test]
fn get_v_owner_propagates_the_whole_abrupt_completion_before_publication() {
    let get_v = normalized(bounded(
        OPERATIONS_SOURCE,
        "pub(crate) fn compile_spec_operation_to_locals(",
        "fn emit_spec_operation_abrupt_exit(",
    ));
    assert!(get_v.contains("SpecOperationIr::GetV|SpecOperationIr::GetMethod=>"));
    assert!(get_v.contains("self.emit_value_to_object_locals(&inputs[0],&pending,function)?"));
    assert!(get_v.contains("result.project(self,output,inputs,function)"));
    let projection = normalized(bounded(
        OPERATIONS_SOURCE,
        "            Self::Expression(pending) => {",
        "            Self::Call(pending) => {",
    ));
    let throw = projection.find("CompletionKind::Throw.code()").unwrap();
    let copy = projection
        .find("builder.completion().copy_from(&pending,function)")
        .unwrap();
    let propagate = projection
        .find("builder.emit_propagate_current_throw(function)")
        .unwrap();
    let publish = projection
        .find("output.copy_from(pending.value(),function)")
        .unwrap();
    assert!(throw < copy && copy < propagate && propagate < publish);
    assert!(!projection.contains("CompletionKind::Normal.code()"));
    assert!(!get_v.contains("ReturnCurrentFunction"));
}

#[test]
fn builtin_to_number_owner_routes_throw_to_cleanup_before_reading_number_bits() {
    let to_number = normalized(bounded(
        MATH_SOURCE,
        "fn emit_math_coerce_number(",
        "fn emit_math_hypot_argument_reduction(",
    ));
    let convert = to_number
        .find("self.emit_value_to_number_payload(input,pending,function)?")
        .unwrap();
    let throw = to_number.find("CompletionKind::Throw.code()").unwrap();
    let copy = to_number
        .find("output.copy_from(pending,function)")
        .unwrap();
    let exit = to_number
        .find("self.emit_branch_to_target(exit,function)")
        .unwrap();
    let normal = to_number
        .find("pending.value().scalar().load(function)")
        .unwrap();
    let store = to_number.find("bits.store(function)").unwrap();
    assert!(convert < throw && throw < copy && copy < exit && exit < normal && normal < store);
    assert!(!to_number.contains("emit_propagate_current_throw"));
}

#[test]
fn contract_records_named_completion_ownership() {
    for evidence in [CONTRACT] {
        assert!(evidence.contains("generic `AbruptRoute` is gone"));
        assert!(evidence.contains("GetV"));
        assert!(evidence.contains("ToNumber"));
        assert!(evidence.contains("unrepresentable"));
    }
}
