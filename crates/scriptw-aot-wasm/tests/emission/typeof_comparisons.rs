//! Actual comparison routes omit intermediate Strings and retain generic typeof.
use crate::property_read_plans::Module;
use scriptw_aot_wasm::{emit, WasmRuntimeValueTag};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower, EqualityBinaryOp, ExprIr, SpecOperationIr, StatementIr};
use wasmparser::{Operator, Validator, WasmFeatures};

const SOURCE_FUNCTIONS: [&str; 8] = [
    "specStrict",
    "specLoose",
    "exprStrict",
    "exprLoose",
    "never",
    "dynamic",
    "bare",
    "unresolved",
];

fn calls(body: &[Operator<'_>], callee: u32) -> usize {
    body.iter()
        .filter(|op| matches!(op, Operator::Call { function_index } if *function_index == callee))
        .count()
}

fn string_values(body: &[Operator<'_>]) -> usize {
    // A String reference becomes a JavaScript Value through its tag/scalar
    // publication. A tag comparison alone cannot match this sequence.
    body.windows(4)
        .filter(|ops| {
            matches!(ops,
                [Operator::I32Const { value }, Operator::LocalSet { .. },
                 Operator::I64Const { value: 0 }, Operator::LocalSet { .. }]
                    if *value == WasmRuntimeValueTag::String as i32
            )
        })
        .count()
}

#[test]
fn literal_typeof_predicates_cover_spec_and_expression_comparisons_without_strings() {
    let (artifact, functions) = std::thread::Builder::new()
        .name("typeof-comparison-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let parsed = parse(r#"
                export function specStrict(value, unused, extra) { return typeof value === "number"; }
                export function specLoose(value, unused, extra) { return "object" == typeof value; }
                export function exprStrict(value, unused, extra) { return "function" !== typeof value; }
                export function exprLoose(value, unused, extra) { return typeof value != "undefined"; }
                export function never(value, unused, extra) { return typeof value === "not-a-type"; }
                export function dynamic(value, spelling, extra) { return typeof value === spelling; }
                export function bare(value, unused, extra) { return typeof value; }
                export function unresolved(value, unused, extra) { return typeof missingTypeofBinding === "undefined"; }
            "#, ParseOptions::module()).unwrap();
            let mut program = lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let mut functions = program.script.as_mut().unwrap().functions.iter_mut()
                .filter(|function| SOURCE_FUNCTIONS.contains(&function.name.as_str()))
                .collect::<Vec<_>>();
            for name in SOURCE_FUNCTIONS {
                assert!(functions.iter().any(|function| function.name == name), "source control {name} was lowered");
            }
            for function in &mut functions {
                let [StatementIr::Return(result)] = function.body.statements.as_mut_slice() else {
                    panic!("one source return in {}", function.name);
                };
                match function.name.as_str() {
                    "specStrict" | "specLoose" | "never" | "dynamic" | "unresolved" => {
                        assert!(matches!(&result.expr, ExprIr::SpecOperation {
                            operation: SpecOperationIr::StrictEqualityComparison | SpecOperationIr::IsLooselyEqual,
                            ..
                        }), "the source comparison reaches the actual SpecOperation route");
                    }
                    "exprStrict" | "exprLoose" => {
                        // Exercise the public ExprIr routes with the exact
                        // lowered operands, owner and Boolean value facts.
                        // Fold the source's outer negation into that variant.
                        let ExprIr::LogicalNot { expr } = &result.expr else {
                            panic!("source inequality retains its negation");
                        };
                        let ExprIr::SpecOperation { operation, operands } = &expr.expr else {
                            panic!("inequality negates its canonical comparison");
                        };
                        let [left, right] = operands.as_slice() else { panic!("two comparison operands"); };
                        result.expr = match operation {
                            SpecOperationIr::StrictEqualityComparison => ExprIr::StrictEquality {
                                op: EqualityBinaryOp::StrictNotEqual,
                                lhs: Box::new(left.clone()), rhs: Box::new(right.clone()),
                            },
                            SpecOperationIr::IsLooselyEqual => ExprIr::LooseEquality {
                                op: EqualityBinaryOp::LooseNotEqual,
                                lhs: Box::new(left.clone()), rhs: Box::new(right.clone()),
                            },
                            _ => panic!("only the source equality routes are adapted"),
                        };
                    }
                    "bare" => assert!(matches!(result.expr, ExprIr::TypeOf { .. })),
                    _ => panic!("unexpected source function"),
                }
            }
            let identities = functions.into_iter().map(|function|
                (function.name.clone(), format!("js::{}#{}", function.name, function.id))
            ).collect::<Vec<_>>();
            (emit(&program).expect("typeof comparisons emit"), identities)
        })
        .unwrap().join().unwrap();
    for bytes in [
        artifact.runtime().unwrap().bytes(),
        artifact.bytes.as_slice(),
    ] {
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(bytes)
            .expect("actual R/P validate");
    }
    let runtime = Module::read(artifact.runtime().unwrap().bytes(), false);
    let program = Module::read(&artifact.bytes, true);
    let (equality, _) = runtime.named("helper::string_equality");
    let equality = program.imported_runtime_function(&runtime, equality);
    for (name, identity) in functions {
        let (_, body) = program.named(&identity);
        match name.as_str() {
            "specStrict" | "specLoose" | "exprStrict" | "exprLoose" | "never" => {
                assert_eq!(
                    string_values(body),
                    0,
                    "{name} must not materialize typeof or literal String Values"
                );
                assert_eq!(
                    calls(body, equality),
                    0,
                    "{name} compares the type directly"
                );
            }
            "dynamic" | "unresolved" => {
                assert!(
                    string_values(body) > 0,
                    "{name} retains its complete typeof fallback"
                );
                assert!(
                    calls(body, equality) > 0,
                    "{name} retains real String comparison"
                );
            }
            "bare" => assert!(
                string_values(body) > 0,
                "a returned typeof still produces its observable String"
            ),
            _ => unreachable!(),
        }
    }
}
