//! Inspect the actual source consumers and their saved-value fallback branches.
use super::*;

fn source_body<'a>(artifact: &'a WasmArtifact, name: &str) -> Vec<Operator<'a>> {
    let index = function_names(artifact)
        .into_iter()
        .find_map(|(index, actual)| actual.starts_with(&format!("js::{name}#")).then_some(index))
        .expect("actual named source function");
    let mut next = imported_function_count(artifact);
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
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
    panic!("named source body exists");
}

fn imported_helper(artifact: &WasmArtifact, requested: &str) -> Option<u32> {
    let runtime = artifact.runtime().expect("String consumer links R");
    let definition = function_names_in_bytes(runtime.bytes())
        .into_iter()
        .find_map(|(index, actual)| (actual == requested).then_some(index))
        .unwrap();
    let mut export_name = None;
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        if let Payload::ExportSection(exports) = payload.unwrap() {
            for export in exports {
                let export = export.unwrap();
                if export.kind == wasmparser::ExternalKind::Func && export.index == definition {
                    export_name = Some(export.name);
                }
            }
        }
    }
    let export_name = export_name.expect("registered helper is exported");
    let mut index = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::ImportSection(imports) = payload.unwrap() {
            for import in imports.into_imports() {
                let import = import.unwrap();
                if matches!(
                    import.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                ) {
                    if import.module == crate::RUNTIME_IMPORT_NAMESPACE
                        && import.name == export_name
                    {
                        return Some(index);
                    }
                    index += 1;
                }
            }
        }
    }
    None
}

fn then_branch<'a>(operators: &'a [Operator<'a>], start: usize) -> &'a [Operator<'a>] {
    let mut depth = 0;
    for (offset, operator) in operators[start..].iter().enumerate() {
        match operator {
            Operator::Block { .. }
            | Operator::If { .. }
            | Operator::Loop { .. }
            | Operator::TryTable { .. } => depth += 1,
            Operator::End if depth == 0 => return &operators[start..start + offset],
            Operator::Else if depth == 0 => return &operators[start..start + offset],
            Operator::End => depth -= 1,
            _ => {}
        }
    }
    panic!("String guard has a complete structured branch");
}

fn primitive_length_branches<'a>(operators: &'a [Operator<'a>]) -> Vec<&'a [Operator<'a>]> {
    operators.windows(4).enumerate().filter_map(|(i, window)| match window {
        [Operator::LocalGet { .. }, Operator::I32Const { value }, Operator::I32Eq, Operator::If { .. }]
            if *value == WasmRuntimeValueTag::String as i32 => {
                let branch = then_branch(operators, i + 4);
                branch.iter().any(|op| matches!(op, Operator::ArrayLen)).then_some(branch)
            }
        _ => None,
    }).collect()
}

fn concat_length_branch<'a>(operators: &'a [Operator<'a>]) -> Option<&'a [Operator<'a>]> {
    operators.windows(8).enumerate().find_map(|(i, window)| match window {
        [Operator::LocalGet { .. }, Operator::I32Const { value: left }, Operator::I32Eq,
         Operator::LocalGet { .. }, Operator::I32Const { value: right }, Operator::I32Eq,
         Operator::I32And, Operator::If { .. }]
            if *left == WasmRuntimeValueTag::String as i32 && *right == *left => {
                let branch = then_branch(operators, i + 8);
                (branch.iter().filter(|op| matches!(op, Operator::ArrayLen)).count() == 2).then_some(branch)
            }
        _ => None,
    })
}

fn validate_program(artifact: &WasmArtifact) {
    Validator::new_with_features(crate::runtime_artifact::program_validation_features())
        .validate_all(&artifact.bytes)
        .expect("actual consumer has valid typed GC roots and operands");
}

#[test]
fn static_and_canonical_get_v_length_read_actual_strings_before_lookup_or_boxing() {
    for (source, generic) in [
        (
            "function read(value) { return value.length; } read(globalThis.value);",
            "helper::value_to_object",
        ),
        (
            "function read() { var value='a\\0\\ud800'; return value.length; } read();",
            "helper::dynamic_property_read",
        ),
    ] {
        let artifact = emit_script(source).unwrap();
        validate_program(&artifact);
        let operators = source_body(&artifact, "read");
        let branches = primitive_length_branches(&operators);
        assert!(
            !branches.is_empty(),
            "the real read consumer reaches an actual-tag String branch: {source}"
        );
        assert!(
            branches.iter().any(|branch| !branch
                .iter()
                .any(|op| matches!(op, Operator::Call { .. } | Operator::CallRef { .. }))),
            "a primitive length branch reads rooted code units without helper calls"
        );
        let fallback =
            imported_helper(&artifact, generic).expect("ordinary fallback remains imported");
        let first_length = operators
            .iter()
            .position(|op| matches!(op, Operator::ArrayLen))
            .unwrap();
        let generic_call = operators
            .iter()
            .position(
                |op| matches!(op, Operator::Call { function_index } if *function_index == fallback),
            )
            .unwrap();
        assert!(
            first_length < generic_call,
            "actual String length precedes boxing/lookup"
        );
    }
    for source in [
        "function read(value,key) { return value[key]; } read(globalThis.value,globalThis.key);",
        "function read(value) { return value.size; } read(globalThis.value);",
    ] {
        let artifact = emit_script(source).unwrap();
        validate_program(&artifact);
        assert!(
            primitive_length_branches(&source_body(&artifact, "read")).is_empty(),
            "computed or other keys cannot acquire a static length proof"
        );
    }
}

#[test]
fn concat_length_has_two_actual_tag_guards_no_allocation_and_one_saved_operand_fallback() {
    let artifact = emit_script(
        "function consume(a,b) { return (a+b).length; } consume(globalThis.a,globalThis.b);",
    )
    .unwrap();
    validate_program(&artifact);
    let operators = source_body(&artifact, "consume");
    let branch = concat_length_branch(&operators)
        .expect("actual canonical outer consumer reaches the fused branch");
    assert!(branch.iter().any(|op| matches!(op, Operator::I64Add)));
    assert!(
        !branch.iter().any(|op| matches!(
            op,
            Operator::Call { .. }
                | Operator::CallRef { .. }
                | Operator::StructNew { .. }
                | Operator::ArrayNew { .. }
                | Operator::ArrayNewDefault { .. }
                | Operator::ArrayNewFixed { .. }
        )),
        "no concatenated String is allocated or exposed by the admitted branch"
    );
    let bound = branch
        .windows(2)
        .find_map(|ops| match ops {
            [Operator::I64Const { value }, Operator::I64LeU] => Some(*value as u64),
            _ => None,
        })
        .expect("widened total is bounded before conversion");
    assert_eq!(
        bound,
        u32::MAX as u64,
        "retain the original concat representable-unit boundary"
    );
    for (left, right, admitted) in [
        (0, 0, true),
        (u32::MAX, 0, true),
        (u32::MAX - 1, 1, true),
        (u32::MAX, 1, false),
        (u32::MAX, u32::MAX, false),
    ] {
        assert_eq!(left as u64 + right as u64 <= bound, admitted);
    }
    let add = imported_helper(&artifact, "helper::coercive_add").unwrap();
    assert_eq!(
        operators
            .iter()
            .filter(|op| matches!(op, Operator::Call { function_index } if *function_index == add))
            .count(),
        1,
        "the miss retains exactly one call to the original coercion owner with saved operands"
    );
    assert!(
        !primitive_length_branches(&operators).is_empty(),
        "fallback length lookup still distinguishes an actual String result from Number/BigInt"
    );
}

#[test]
fn original_string_and_join_shapes_reach_real_primitive_consumers() {
    let dynamic = include_str!(
        "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/dynamic-string-4096.js"
    );
    let artifact = emit_script(dynamic).unwrap();
    validate_program(&artifact);
    assert!(
        concat_length_branch(&source_body(&artifact, "workload")).is_some(),
        "the unchanged dynamic-string workload reaches the fused outer consumer"
    );
    for source in [
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/folded-string-4096.js"
        ),
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/join-empty-1024.js"
        ),
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/join-singleton-1024.js"
        ),
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/join-small-1024.js"
        ),
    ] {
        let artifact = emit_script(source).unwrap();
        validate_program(&artifact);
        assert!(
            !primitive_length_branches(&source_body(&artifact, "workload")).is_empty(),
            "the unchanged call-result consumer reaches the primitive String branch"
        );
    }
}
