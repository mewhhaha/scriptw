//! Inspect actual P predicates, their tag admission and the single R fallback.
use super::*;

fn source_body<'a>(artifact: &'a WasmArtifact, name: &str) -> Vec<Operator<'a>> {
    let target = function_names(artifact)
        .into_iter()
        .find_map(|(index, actual)| actual.starts_with(&format!("js::{name}#")).then_some(index))
        .expect("actual named source body");
    let mut index = imported_function_count(artifact);
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if index == target {
                return body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .map(Result::unwrap)
                    .collect();
            }
            index += 1;
        }
    }
    panic!("named body is emitted");
}

fn relational_helper(artifact: &WasmArtifact) -> u32 {
    let runtime = artifact.runtime().expect("dynamic predicates retain R");
    let definition = function_names_in_bytes(runtime.bytes())
        .into_iter()
        .find_map(|(index, name)| (name == "helper::relational_comparison").then_some(index))
        .unwrap();
    let export = Parser::new(0)
        .parse_all(runtime.bytes())
        .find_map(|payload| {
            if let Payload::ExportSection(exports) = payload.unwrap() {
                return exports
                    .into_iter()
                    .map(Result::unwrap)
                    .find(|export| {
                        export.kind == wasmparser::ExternalKind::Func && export.index == definition
                    })
                    .map(|export| export.name);
            }
            None
        })
        .unwrap();
    let mut index = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::ImportSection(imports) = payload.unwrap() {
            for import in imports.into_imports() {
                let import = import.unwrap();
                if matches!(
                    import.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                ) {
                    if import.module == crate::RUNTIME_IMPORT_NAMESPACE && import.name == export {
                        return index;
                    }
                    index += 1;
                }
            }
        }
    }
    panic!("P imports its original relational fallback");
}

fn arms<'a>(
    operators: &'a [Operator<'a>],
    start: usize,
) -> (&'a [Operator<'a>], &'a [Operator<'a>]) {
    let (mut depth, mut alternative) = (0, None);
    for index in start..operators.len() {
        match operators[index] {
            Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::If { .. }
            | Operator::TryTable { .. } => depth += 1,
            Operator::Else if depth == 0 => alternative = Some(index),
            Operator::End if depth == 0 => {
                let alternative = alternative.expect("tag guard retains an else fallback");
                return (
                    &operators[start..alternative],
                    &operators[alternative + 1..index],
                );
            }
            Operator::End => depth -= 1,
            _ => {}
        }
    }
    panic!("complete actual-tag branch");
}

#[test]
fn identifiers_use_two_actual_tags_and_a_bounded_call_free_number_predicate() {
    let artifact = emit_script(
        "function lt(a,b){return a<b;} function le(a,b){return a<=b;} function gt(a,b){return a>b;} function ge(a,b){return a>=b;} function hot(){let i=0;while(i<4096)i++;return i;} print(lt(globalThis.a,globalThis.b),le(1,2),gt(2,1),ge(2,2),hot());"
    ).unwrap();
    Validator::new_with_features(crate::runtime_artifact::program_validation_features())
        .validate_all(&artifact.bytes)
        .expect("guarded P validates");
    let helper = relational_helper(&artifact);
    for (name, operation) in [("lt", 0), ("le", 1), ("gt", 2), ("ge", 3), ("hot", 0)] {
        let operators = source_body(&artifact, name);
        let matches = |operator: &Operator<'_>| {
            matches!(
                (operation, operator),
                (0, Operator::F64Lt)
                    | (1, Operator::F64Le)
                    | (2, Operator::F64Gt)
                    | (3, Operator::F64Ge)
            )
        };
        let (fast, fallback) = operators
            .windows(8)
            .enumerate()
            .find_map(|(index, window)| {
                if matches!(window,
                [Operator::LocalGet { .. }, Operator::I32Const { value: left }, Operator::I32Eq,
                 Operator::LocalGet { .. }, Operator::I32Const { value: right }, Operator::I32Eq,
                 Operator::I32And, Operator::If { .. }]
                if *left == WasmRuntimeValueTag::Number as i32 && *right == *left)
                {
                    let branches = arms(&operators, index + 8);
                    return branches.0.iter().any(&matches).then_some(branches);
                }
                None
            })
            .expect("both evaluated Number tags admit the native comparison");
        assert_eq!(fast.iter().filter(|operator| matches(operator)).count(), 1);
        assert!(
            fast.len() <= 8
                && fast.iter().all(|operator| matches!(
                    operator,
                    Operator::LocalGet { .. }
                        | Operator::F64ReinterpretI64
                        | Operator::F64Lt
                        | Operator::F64Le
                        | Operator::F64Gt
                        | Operator::F64Ge
                        | Operator::LocalSet { .. }
                )),
            "the hot path contains only scalar loads, one comparison and its predicate result"
        );
        assert_eq!(
            fallback
                .iter()
                .filter(|operator| matches!(operator,
            Operator::Call { function_index } if *function_index == helper))
                .count(),
            1,
            "the non-Number branch calls the existing complete R coercion owner once"
        );
    }
}
