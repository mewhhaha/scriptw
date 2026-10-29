//! Inspect the actual dynamic addition and saved compound-assignment consumers.
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

fn addition_import(artifact: &WasmArtifact) -> u32 {
    let runtime = artifact.runtime().expect("addition links R");
    let definition = function_names_in_bytes(runtime.bytes())
        .into_iter()
        .find_map(|(index, actual)| (actual == "helper::coercive_add").then_some(index))
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
    let export_name = export_name.expect("the actual coercive owner is exported by R");
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
                        return index;
                    }
                    index += 1;
                }
            }
        }
    }
    panic!("P retains the complete coercive fallback import");
}

fn branches<'a>(
    operators: &'a [Operator<'a>],
    start: usize,
) -> (&'a [Operator<'a>], &'a [Operator<'a>]) {
    let mut depth = 0;
    let mut otherwise = None;
    for (index, op) in operators.iter().enumerate().skip(start + 1) {
        match op {
            Operator::Block { .. }
            | Operator::If { .. }
            | Operator::Loop { .. }
            | Operator::TryTable { .. } => depth += 1,
            Operator::Else if depth == 0 => otherwise = Some(index),
            Operator::End if depth == 0 => {
                let otherwise = otherwise.expect("Number guard retains its fallback arm");
                return (
                    &operators[start + 1..otherwise],
                    &operators[otherwise + 1..index],
                );
            }
            Operator::End => depth -= 1,
            _ => {}
        }
    }
    panic!("Number guard has a complete structured branch");
}

#[test]
fn dynamic_and_saved_compound_additions_use_actual_number_tags_before_the_runtime_fallback() {
    let artifact = emit_script(
        "function binary(left,right){return left+right;}\n\
         function compound(left,right){left+=right;return left;}\n[binary,compound];",
    )
    .expect("unspecialized additions emit");
    expect_valid_module(&artifact, 2);
    let add = addition_import(&artifact);
    for name in ["binary", "compound"] {
        let operators = source_body(&artifact, name);
        let guards = operators
            .windows(8)
            .enumerate()
            .filter_map(|(index, ops)| match ops {
                [Operator::LocalGet { .. }, Operator::I32Const { value: left }, Operator::I32Eq,
                 Operator::LocalGet { .. }, Operator::I32Const { value: right }, Operator::I32Eq,
                 Operator::I32And, Operator::If { .. }]
                    if *left == WasmRuntimeValueTag::Number as i32 && *right == *left => Some(index + 7),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(guards.len(), 1, "{name} checks both saved runtime tags");
        let (numeric, fallback) = branches(&operators, guards[0]);
        assert_eq!(
            numeric
                .iter()
                .filter(|op| matches!(op, Operator::F64Add))
                .count(),
            1,
            "the actual Number arm performs one direct binary64 addition"
        );
        assert!(numeric.iter().all(|op| matches!(op,
            Operator::LocalGet { .. } | Operator::LocalSet { .. } | Operator::I32Const { .. }
            | Operator::RefNull { .. } | Operator::F64ReinterpretI64
            | Operator::F64Add | Operator::I64ReinterpretF64)),
            "{name}'s Number arm has no runtime call, coercion, allocation or observable operation: {numeric:?}");
        assert!(
            numeric.windows(2).any(|ops| matches!(ops,
            [Operator::I32Const { value }, Operator::LocalSet { .. }]
            if *value == WasmRuntimeValueTag::Number as i32)),
            "the result publishes its actual Number tag"
        );
        assert!(
            numeric
                .iter()
                .any(|op| matches!(op, Operator::RefNull { .. })),
            "the Number result clears any previous strong reference"
        );
        assert!(
            matches!(&numeric[numeric.len() - 4..],
            [Operator::I32Const { value: normal }, Operator::LocalSet { .. },
             Operator::I32Const { value: 0 }, Operator::LocalSet { .. }]
            if *normal == crate::COMPLETION_KIND_NORMAL as i32),
            "the fast result resets both completion kind and target"
        );
        assert_eq!(
            fallback
                .iter()
                .filter(|op| matches!(op,
            Operator::Call { function_index } if *function_index == add))
                .count(),
            1,
            "each guard miss calls the original complete R coercion owner once"
        );
        assert_eq!(
            operators
                .iter()
                .filter(|op| matches!(op,
            Operator::Call { function_index } if *function_index == add))
                .count(),
            1,
            "the source consumer retains one physical fallback call"
        );
        assert!(
            !fallback.iter().any(|op| matches!(op, Operator::F64Add)),
            "non-Number operands retain complete runtime conversion and abrupt-completion handling"
        );
    }
}
