//! Infallible queries retain their kernels and full Normal result without pending5.
use crate::property_read_plans::Module;
use scriptw_aot_wasm::emit;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower, AbruptCapability, ExprIr, NormalResult, SpecOperationIr, StatementIr};
use wasmparser::{Operator, Validator, WasmFeatures};

const QUERIES: [(&str, SpecOperationIr); 7] = [
    ("callable", SpecOperationIr::IsCallable),
    ("constructor", SpecOperationIr::IsConstructor),
    ("propertyKey", SpecOperationIr::IsPropertyKey),
    ("boolean", SpecOperationIr::ToBoolean),
    ("sameValue", SpecOperationIr::SameValue),
    ("sameValueZero", SpecOperationIr::SameValueZero),
    ("strict", SpecOperationIr::StrictEqualityComparison),
];

fn calls(body: &[Operator<'_>], callee: u32) -> usize {
    body.iter()
        .filter(|op| matches!(op, Operator::Call { function_index } if *function_index == callee))
        .count()
}

fn guarded_value_copies(body: &[Operator<'_>]) -> usize {
    // The generic dispatcher copies Value3 only when pending.kind is Normal.
    // Match that transport boundary, not arbitrary Ifs within the algorithms.
    body.windows(11)
        .filter(|ops| {
            matches!(
                ops,
                [
                    Operator::LocalGet { .. },
                    Operator::I32Const { value: 0 },
                    Operator::I32Eq,
                    Operator::If { .. },
                    Operator::LocalGet { .. },
                    Operator::LocalSet { .. },
                    Operator::LocalGet { .. },
                    Operator::LocalSet { .. },
                    Operator::LocalGet { .. },
                    Operator::LocalSet { .. },
                    Operator::End
                ]
            )
        })
        .count()
}

fn direct_normal_publications(body: &[Operator<'_>]) -> usize {
    // Value3 copied from distinct output storage, then Normal kind + empty
    // target. A pending.set_normal(pending.value()) self-copy cannot pass.
    body.windows(10)
        .filter(|ops| {
            matches!(ops, [
        Operator::LocalGet { local_index: source_tag },
        Operator::LocalSet { local_index: result_tag },
        Operator::LocalGet { .. }, Operator::LocalSet { .. },
        Operator::LocalGet { .. }, Operator::LocalSet { .. },
        Operator::I32Const { value: 0 }, Operator::LocalSet { .. },
        Operator::I32Const { value: 0 }, Operator::LocalSet { .. }
    ] if source_tag != result_tag)
        })
        .count()
}

#[test]
fn infallible_boolean_catalog_rows_omit_pending_completion_and_keep_whole_normal() {
    let catalog = SpecOperationIr::ALL
        .iter()
        .copied()
        .filter(|operation| {
            let descriptor = operation.descriptor();
            descriptor.normal_result() == NormalResult::Boolean
                && descriptor.abrupt() == AbruptCapability::Infallible
        })
        .collect::<Vec<_>>();
    assert_eq!(catalog, QUERIES.map(|(_, operation)| operation));
    let (artifact, functions) = std::thread::Builder::new()
        .name("infallible-boolean-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let mut source = String::new();
            for (name, _) in QUERIES {
                source.push_str(&format!(
                    "export function {name}(left, right, unused) {{ return left === right; }}\n"
                ));
            }
            source.push_str("export function loose(left, right, unused) { return left == right; }");
            let parsed = parse(&source, ParseOptions::module()).unwrap();
            let mut program = lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let mut identities = Vec::new();
            for function in &mut program.script.as_mut().unwrap().functions {
                let selected = QUERIES.iter().find(|(name, _)| *name == function.name);
                if selected.is_none() && function.name != "loose" {
                    continue;
                }
                let [StatementIr::Return(result)] = function.body.statements.as_mut_slice() else {
                    panic!("one lowered return in {}", function.name);
                };
                let ExprIr::SpecOperation {
                    operation,
                    operands,
                } = &mut result.expr
                else {
                    panic!("the source reaches the actual spec-operation dispatcher");
                };
                if let Some((_, selected)) = selected {
                    assert_eq!(*operation, SpecOperationIr::StrictEqualityComparison);
                    assert_eq!(operands.len(), 2);
                    // Retain actual parameter reads and their value facts. The
                    // first four catalog operations consume only the left Value.
                    if matches!(
                        selected,
                        SpecOperationIr::IsCallable
                            | SpecOperationIr::IsConstructor
                            | SpecOperationIr::IsPropertyKey
                            | SpecOperationIr::ToBoolean
                    ) {
                        operands.truncate(1);
                    }
                    *operation = *selected;
                } else {
                    assert_eq!(*operation, SpecOperationIr::IsLooselyEqual);
                }
                identities.push((
                    function.name.clone(),
                    format!("js::{}#{}", function.name, function.id),
                ));
            }
            assert_eq!(
                identities.len(),
                8,
                "every requested source function was selected"
            );
            for name in QUERIES.map(|(name, _)| name).into_iter().chain(["loose"]) {
                assert!(identities.iter().any(|(found, _)| found == name));
            }
            (emit(&program).expect("infallible queries emit"), identities)
        })
        .unwrap()
        .join()
        .unwrap();
    for bytes in [
        artifact.runtime().unwrap().bytes(),
        artifact.bytes.as_slice(),
    ] {
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(bytes)
            .unwrap();
    }
    let runtime = Module::read(artifact.runtime().unwrap().bytes(), false);
    let program = Module::read(&artifact.bytes, true);
    let [strings, bigints, loose] = [
        "helper::string_equality",
        "helper::bigint_equality",
        "helper::loose_equality",
    ]
    .map(|name| {
        let (definition, _) = runtime.named(name);
        program.imported_runtime_function(&runtime, definition)
    });
    for (name, identity) in functions {
        let (_, body) = program.named(&identity);
        if name == "loose" {
            assert_eq!(
                guarded_value_copies(body),
                1,
                "loose equality retains pending Completion"
            );
            assert_eq!(
                calls(body, loose),
                1,
                "coercion and HTMLDDA retain the loose kernel"
            );
        } else {
            assert_eq!(
                guarded_value_copies(body),
                0,
                "{name} has no pending Normal-copy gate"
            );
            assert_eq!(
                direct_normal_publications(body),
                1,
                "{name} still publishes full Normal"
            );
            assert_eq!(calls(body, loose), 0);
            if matches!(name.as_str(), "sameValue" | "sameValueZero" | "strict") {
                assert_eq!(calls(body, strings), 1, "{name} retains String equality");
                assert_eq!(calls(body, bigints), 1, "{name} retains BigInt equality");
            }
        }
    }
}
