//! Catalog expressions retain their kernels while publishing only their value output.
use crate::property_read_plans::Module;
use scriptw_aot_wasm::emit;
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::{
    AbruptCapability, ExprIr, NormalResult, SpecOperationIr, StatementIr, TypedExpr, ValueInfo,
    ValueKind, lower,
};
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
    let completion_tag = body
        .windows(10)
        .find_map(|ops| match ops {
            [
                Operator::I32Const { value: 0 },
                Operator::LocalSet { local_index },
                Operator::I64Const { value: 0 },
                Operator::LocalSet { .. },
                Operator::RefNull { .. },
                Operator::LocalSet { .. },
                Operator::I32Const { value: 0 },
                Operator::LocalSet { .. },
                Operator::I32Const { value: 0 },
                Operator::LocalSet { .. },
            ] => Some(*local_index),
            _ => None,
        })
        .expect("source body initializes its enclosing Completion");
    // Count publication into the enclosing Completion specifically. The
    // expression's output copy followed by kind/target normalization has the
    // same opcode shape but writes a different Value3 destination.
    body.windows(10)
        .filter(|ops| {
            matches!(ops, [
        Operator::LocalGet { local_index: source_tag },
        Operator::LocalSet { local_index: result_tag },
        Operator::LocalGet { .. }, Operator::LocalSet { .. },
        Operator::LocalGet { .. }, Operator::LocalSet { .. },
        Operator::I32Const { value: 0 }, Operator::LocalSet { .. },
        Operator::I32Const { value: 0 }, Operator::LocalSet { .. }
    ] if source_tag != result_tag && *result_tag == completion_tag)
        })
        .count()
}

#[test]
fn infallible_boolean_catalog_rows_omit_pending_completion_and_normal_payload_copies() {
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
            source.push_str("export function loose(left, right, unused) { return left == right; } export function nullish(value) { return value === null; } export function kind(value) { return typeof value === 'number'; }");
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
                0,
                "loose equality projects its normal value without a second Normal gate"
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
                0,
                "{name} leaves normal payload publication to its statement/return owner"
            );
            assert_eq!(calls(body, loose), 0);
            if matches!(name.as_str(), "sameValue" | "sameValueZero" | "strict") {
                assert_eq!(calls(body, strings), 1, "{name} retains String equality");
                assert_eq!(calls(body, bigints), 1, "{name} retains BigInt equality");
            }
        }
    }
    for name in ["js::nullish#", "js::kind#"] {
        let (_, body) = program.named(name);
        assert_eq!(
            guarded_value_copies(body),
            0,
            "{name} retains a direct scalar comparison"
        );
        assert_eq!(
            direct_normal_publications(body),
            0,
            "{name} uses the same output-only normal result contract"
        );
    }
}

#[test]
fn catalog_expression_results_publish_whole_throw_only_on_the_cold_edge() {
    std::thread::Builder::new()
        .name("catalog-expression-result".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let parsed = parse(
                "export function numeric(value, unused) { return value === unused; } export function call(value, receiver) { return value === receiver; }",
                ParseOptions::module(),
            ).unwrap();
            let mut ir = lower(&parsed);
            for function in &mut ir.script.as_mut().unwrap().functions {
                let operation = match function.name.as_str() {
                    "numeric" => SpecOperationIr::ToNumber,
                    "call" => SpecOperationIr::Call,
                    _ => continue,
                };
                let [StatementIr::Return(result)] = function.body.statements.as_mut_slice() else {
                    panic!("one source return");
                };
                let ExprIr::SpecOperation { operation: selected, operands } = &mut result.expr else {
                    panic!("source reaches the catalog owner");
                };
                *selected = operation;
                if operation == SpecOperationIr::ToNumber { operands.truncate(1); }
                let kind = match operation {
                    SpecOperationIr::ToNumber => ValueKind::Number,
                    SpecOperationIr::Call => ValueKind::Dynamic,
                    _ => unreachable!("the witness selects a numeric operation or source Call"),
                };
                let expression = result.expr.clone();
                *result = TypedExpr::from_info(ValueInfo::new(kind), expression);
                function.return_kind = kind;
                function.body.result_kind = kind;
                function.return_shape = None;
                function.return_targets = result.function_targets.clone();
            }
            let artifact = emit(&ir).unwrap();
            for bytes in [artifact.bytes.as_slice(), artifact.runtime().unwrap().bytes()] {
                Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
            }
            let program = Module::read(&artifact.bytes, true);
            let runtime = Module::read(artifact.runtime().unwrap().bytes(), false);
            let (conversion, _) = runtime.named("helper::value_to_number");
            let conversion = program.imported_runtime_function(&runtime, conversion);
            let (_, numeric) = program.named("js::numeric#");
            assert_eq!(calls(numeric, conversion), 1);
            assert_eq!(guarded_value_copies(numeric), 0,
                "the catalog result has no Normal-gated value transport");
            assert_eq!(direct_normal_publications(numeric), 0,
                "a normal expression payload is published only to its output and source return");
            let call = numeric.iter().position(|op| matches!(op,
                Operator::Call { function_index } if *function_index == conversion)).unwrap();
            let [Operator::LocalSet { .. }, Operator::LocalSet { local_index: kind },
                Operator::LocalSet { .. }, Operator::LocalSet { .. }, Operator::LocalSet { .. },
                after @ ..] = &numeric[call+1..] else {
                panic!("the complete helper result is stored whole");
            };
            // The call sits inside the inline Number/undefined conversion
            // arms; only the ends of those arms precede the kind test.
            let after = &after[after.iter().take_while(|op| matches!(op, Operator::End)).count()..];
            let [Operator::LocalGet { local_index: tested },
                Operator::I32Const { value: 1 }, Operator::I32Eq, Operator::If { .. }, ..] = after else {
                panic!("the complete helper result goes directly to one cold Throw edge");
            };
            assert_eq!(kind, tested, "the result's own kind controls its projection");
            let (_, call) = program.named("js::call#");
            assert_eq!(guarded_value_copies(call), 1,
                "source Call retains its existing complete returned-kind protocol");
        }).unwrap().join().unwrap();
}
