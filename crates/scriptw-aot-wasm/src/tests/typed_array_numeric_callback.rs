//! The real TypedArray merge reaches the typed entry and retains its fallback.
use super::*;

fn body<'a>(bytes: &'a [u8], index: u32) -> Vec<Operator<'a>> {
    let mut next = imported_function_count_in_bytes(bytes);
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if next == index {
                return body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .map(Result::unwrap)
                    .collect();
            }
            next += 1;
        }
    }
    panic!("actual registered function body");
}

fn branch_end(operators: &[Operator<'_>], start: usize) -> usize {
    let mut depth = 0;
    for (index, operator) in operators.iter().enumerate().skip(start) {
        match operator {
            Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::If { .. }
            | Operator::TryTable { .. } => depth += 1,
            Operator::End if depth == 0 => return index,
            Operator::End => depth -= 1,
            _ => {}
        }
    }
    panic!("structured branch has an end");
}

#[test]
fn actual_typed_array_sort_and_to_sorted_call_binary_entries_before_argv_fallback() {
    let artifact =
        emit_script("function compare(a,b){return a-b;} new Float64Array([3,1,2]).sort(compare);")
            .unwrap();
    expect_valid_module(&artifact, 1);
    let bytes = artifact.runtime().unwrap().bytes();
    Validator::new_with_features(crate::runtime_artifact::program_validation_features())
        .validate_all(bytes)
        .expect("the real shared TypedArray body has valid typed callback operands and results");
    let names = function_names_in_bytes(bytes);
    let find = |name: &str| {
        names
            .iter()
            .find_map(|(&index, actual)| (actual == name).then_some(index))
            .unwrap()
    };
    let binary_type = crate::module::StaticSignature::BinaryNumericCallback.type_index();
    let call = find("helper::function_call");
    for builtin in [
        StandardBuiltinId::TypedArrayPrototypeSort,
        StandardBuiltinId::TypedArrayPrototypeToSorted,
    ] {
        let operators = body(bytes, find(&format!("builtin::{}", builtin.debug_name())));
        let typed: Vec<_> = operators
            .iter()
            .enumerate()
            .filter_map(|(i, op)| {
                matches!(op, Operator::CallRef { type_index } if *type_index == binary_type)
                    .then_some(i)
            })
            .collect();
        assert_eq!(
            typed.len(),
            1,
            "one real binary callback attempt in the merge: {builtin:?}"
        );
        let typed = typed[0];
        let generic = operators
            .iter()
            .enumerate()
            .skip(typed + 1)
            .find_map(|(i, op)| {
                matches!(op, Operator::Call { function_index } if *function_index == call)
                    .then_some(i)
            })
            .expect("the original actual-function fallback remains after scalar dispatch");
        assert!(
            operators[typed + 1..generic]
                .iter()
                .any(|op| matches!(op, Operator::ArrayNewFixed { array_size: 2, .. })),
            "fresh generic argv is created only after the scalar attempt can decline"
        );
        let merge = operators
            .iter()
            .enumerate()
            .skip(generic + 1)
            .find_map(|(i, op)| matches!(op, Operator::F64Le).then_some(i))
            .expect("stable TypedArray merge retains F64Le");
        let nan_guard = operators[generic + 1..merge].windows(6).enumerate().find_map(|(offset, window)| match window {
            [Operator::LocalGet { local_index: left }, Operator::F64ReinterpretI64,
             Operator::LocalGet { local_index: right }, Operator::F64ReinterpretI64,
             Operator::F64Ne, Operator::If { .. }] if left == right => Some(generic + 1 + offset + 6),
            _ => None,
        }).expect("both routes retain NaN normalization before F64Le");
        let nan_end = branch_end(&operators, nan_guard);
        assert!(
            nan_end < merge
                && operators[nan_guard..nan_end]
                    .iter()
                    .any(|op| matches!(op, Operator::I64Const { value: 0 })),
            "NaN publishes positive zero before the stable left-selection comparison"
        );
        let bigint = operators[generic + 1..merge].windows(2).any(|window| matches!(window,
            [Operator::I32Const { value }, Operator::I32Eq] if *value == WasmRuntimeValueTag::BigInt as i32
        ));
        assert!(
            bigint,
            "the default exact BigInt comparison branch remains in the real builtin"
        );
    }
}

#[test]
fn ordinary_and_arrow_source_comparators_publish_real_binary_callback_bodies() {
    let artifact = emit_script(
        r#"
function ordinary(a,b){return a-b;}
var arrow=(a,b)=>b-a;
function generic(a,b){var unused=0;return a-b;}
new Float64Array([3,1,2]).toSorted(ordinary);
new Float64Array([3,1,2]).sort(arrow);
new Float64Array([3,1,2]).sort(generic);
"#,
    )
    .unwrap();
    expect_valid_module(&artifact, 3);
    let names = function_names(&artifact);
    let auxiliary: Vec<_> = names
        .iter()
        .filter(|(_, name)| name.starts_with("js-numeric::"))
        .collect();
    assert_eq!(auxiliary.len(), 2, "ordinary and Arrow leaves each publish one auxiliary body; generic retains its ordinary entry");
    assert!(auxiliary
        .iter()
        .any(|(_, name)| name.starts_with("js-numeric::ordinary#")));
    assert!(!auxiliary
        .iter()
        .any(|(_, name)| name.starts_with("js-numeric::generic#")));
    for (index, _) in auxiliary {
        let operators = body(&artifact.bytes, *index);
        assert!(operators.iter().any(|op| matches!(op, Operator::F64Sub)));
        assert!(
            !operators.iter().any(|op| matches!(
                op,
                Operator::ArrayNew { .. }
                    | Operator::ArrayNewFixed { .. }
                    | Operator::ArrayNewDefault { .. }
            )),
            "admitted comparator thunk needs no generic argv backing"
        );
    }
}
