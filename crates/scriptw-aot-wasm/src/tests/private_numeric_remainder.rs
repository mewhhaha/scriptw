//! Inspect both physical source variants, including the ordinary call boundary.
use super::*;

fn with_bodies(source: &str, name: &str, mut check: impl FnMut(&WasmArtifact, &[Operator<'_>])) {
    let artifact = emit_script(source).expect("private numeric control emits");
    expect_valid_module(&artifact, 1);
    let names = function_names(&artifact)
        .into_iter()
        .filter(|(_, actual)| actual.starts_with(&format!("js::{name}#")))
        .collect::<BTreeMap<_, _>>();
    assert!(!names.is_empty(), "the actual source body is named");
    let mut index = imported_function_count(&artifact);
    let mut inspected = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.expect("P parses") {
            if names.contains_key(&index) {
                let operators = body
                    .get_operators_reader()
                    .expect("source body parses")
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                    .expect("source operators decode");
                check(&artifact, &operators);
                inspected += 1;
            }
            index += 1;
        }
    }
    assert_eq!(inspected, names.len(), "every source variant is inspected");
}

fn runtime_helper_import(artifact: &WasmArtifact, requested: &str) -> u32 {
    let runtime = artifact.runtime().expect("ordinary operations retain R");
    let definition = function_names_in_bytes(runtime.bytes())
        .into_iter()
        .find_map(|(index, name)| (name == requested).then_some(index))
        .expect("the actual runtime helper is named");
    let mut exported = None;
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        if let Payload::ExportSection(exports) = payload.unwrap() {
            for export in exports {
                let export = export.unwrap();
                if export.kind == wasmparser::ExternalKind::Func && export.index == definition {
                    exported = Some(export.name);
                }
            }
        }
    }
    let exported = exported.expect("R exports the helper");
    let mut index = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::ImportSection(imports) = payload.unwrap() {
            for import in imports.into_imports() {
                let import = import.unwrap();
                if matches!(
                    import.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                ) {
                    if import.module == crate::RUNTIME_IMPORT_NAMESPACE && import.name == exported {
                        return index;
                    }
                    index += 1;
                }
            }
        }
    }
    panic!("P imports {requested}");
}

fn masked_remainders(operators: &[Operator<'_>]) -> usize {
    operators
        .windows(5)
        .filter(|ops| {
            matches!(
                ops,
                [
                    Operator::I32TruncF64U,
                    Operator::I32Const { value: 3 },
                    Operator::I32And,
                    Operator::F64ConvertI32U,
                    Operator::I64ReinterpretF64
                ]
            )
        })
        .count()
}

fn generic_remainders(operators: &[Operator<'_>]) -> usize {
    operators
        .iter()
        .filter(|op| matches!(op, Operator::I64RemS))
        .count()
}

#[test]
fn mixed_join_keeps_packed_publication_and_removes_general_remainder_kernels() {
    with_bodies(
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/join-mixed-1024.js"
        ),
        "workload",
        |artifact, operators| {
            assert_eq!(masked_remainders(operators), 3);
            assert_eq!(generic_remainders(operators), 0);
            let append = runtime_helper_import(artifact, "helper::array_packed_push1");
            assert!(operators.iter().any(
                |op| matches!(op, Operator::Call { function_index } if *function_index == append)
            ));
        },
    );
}

#[test]
fn nonpower_of_two_divisors_keep_unknown_calls_and_use_unsigned_remainder() {
    for divisor in [3_u32, u32::MAX] {
        with_bodies(
            &format!(
                "function varied(){{for(var i=0;i<16;i++)globalThis.consume(i%{divisor});}} varied();"
            ),
            "varied",
            |artifact, operators| {
                assert_eq!(generic_remainders(operators), 0);
                assert_eq!(
                    operators
                        .iter()
                        .filter(|op| matches!(op, Operator::I32RemU))
                        .count(),
                    1
                );
                let dispatch = runtime_helper_import(artifact, "helper::proxy_call");
                assert!(operators.iter().any(
                    |op| matches!(op, Operator::Call { function_index } if *function_index == dispatch)
                ));
            },
        );
    }
}

#[test]
fn induction_facts_end_with_the_ordinary_loop_body() {
    with_bodies(
        "function varied(){for(var i=0;i<16;i++)globalThis.consume(i%4);i=globalThis.input;return i%4;} varied();",
        "varied",
        |_, operators| {
            assert_eq!(masked_remainders(operators), 1);
            assert_eq!(generic_remainders(operators), 1);
        },
    );
}

#[test]
fn mutable_captured_signed_or_unbounded_counters_keep_general_remainder() {
    for source in [
        "function varied(){for(var i=-0;i<16;i++)globalThis.consume(i%4);} varied();",
        "function varied(){for(var i=-1;i<16;i++)globalThis.consume(i%4);} varied();",
        "function varied(){for(var i=0;i<4294967297;i++)globalThis.consume(i%4);} varied();",
        "function varied(){for(var i=0;i<16;i++){i=globalThis.input;globalThis.consume(i%4);}} varied();",
        "function varied(){for(var i=0;i<16;i++){i++;globalThis.consume(i%4);}} varied();",
        "function varied(){function change(){i=-1;}for(var i=0;i<16;i++){globalThis.consume(change);globalThis.consume(i%4);}} varied();",
        "function varied(i){for(var i=0;i<16;i++){arguments[0]=-1;globalThis.consume(i%4);}} varied(0);",
        "function varied(){for(var i=0;i<16;i++){let i=globalThis.input;globalThis.consume(i%4);}} varied();",
        "function* varied(){for(var i=0;i<16;i++){yield i;globalThis.consume(i%4);}} varied();",
    ] {
        let mut generic = 0;
        with_bodies(source, "varied", |_, operators| {
            assert_eq!(masked_remainders(operators), 0, "{source}");
            generic += generic_remainders(operators);
        });
        assert!(generic > 0, "{source}");
    }
}

#[test]
fn unsupported_divisors_keep_number_remainder_and_its_signed_zero_semantics() {
    for divisor in ["0", "-0", "-4", "1.5", "4294967296", "globalThis.divisor"] {
        with_bodies(
            &format!(
                "function varied(){{for(var i=0;i<16;i++)globalThis.consume(i%{divisor});}} varied();"
            ),
            "varied",
            |_, operators| {
                assert_eq!(masked_remainders(operators), 0, "{divisor}");
                assert!(generic_remainders(operators) > 0, "{divisor}");
            },
        );
    }
}
