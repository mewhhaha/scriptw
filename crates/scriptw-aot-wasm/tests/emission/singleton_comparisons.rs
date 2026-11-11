//! Singleton syntax omits all-kind strict equality in both real emitter routes.
use crate::property_read_plans::Module;
use scriptw_aot_wasm::{WasmRuntimeValueTag, emit};
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::{EqualityBinaryOp, ExprIr, SpecOperationIr, StatementIr, lower};
use wasmparser::{Operator, Validator, WasmFeatures};

const SOURCE_FUNCTIONS: [&str; 9] = [
    "specNull",
    "specVoidRight",
    "specVoidLeft",
    "exprNull",
    "exprUndefined",
    "dynamic",
    "shadowed",
    "looseNull",
    "number",
];

fn calls(body: &[Operator<'_>], callee: u32) -> usize {
    body.iter()
        .filter(|op| matches!(op, Operator::Call { function_index } if *function_index == callee))
        .count()
}

fn compares_tag(body: &[Operator<'_>], tag: WasmRuntimeValueTag) -> bool {
    body.windows(3).any(|ops| {
        matches!(ops,
            [Operator::LocalGet { .. }, Operator::I32Const { value }, Operator::I32Eq]
                if *value == tag as i32
        )
    })
}

#[test]
fn strict_singletons_omit_all_kind_dispatch_and_keep_dynamic_and_loose_controls() {
    let (artifact, functions) = std::thread::Builder::new()
        .name("singleton-comparison-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let parsed = parse(r#"
                export function specNull(value, unused, extra) { return value === null; }
                export function specVoidRight(value, effect, extra) { return value === void effect(); }
                export function specVoidLeft(value, effect, extra) { return void effect() === value; }
                export function exprNull(value, unused, extra) { return null !== value; }
                export function exprUndefined(value, unused, extra) { return value !== void 0; }
                export function dynamic(value, other, extra) { return value === other; }
                export function shadowed(value, undefined, extra) { return value === undefined; }
                export function looseNull(value, unused, extra) { return value == null; }
                export function number(value, unused, extra) { return value === 0; }
            "#, ParseOptions::module()).unwrap();
            let mut program = lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let functions = program.script.as_mut().unwrap().functions.iter_mut()
                .filter(|function| SOURCE_FUNCTIONS.contains(&function.name.as_str()))
                .collect::<Vec<_>>();
            assert_eq!(functions.len(), SOURCE_FUNCTIONS.len());
            for name in SOURCE_FUNCTIONS {
                assert!(functions.iter().any(|function| function.name == name), "source control {name} was lowered");
            }
            let mut identities = Vec::new();
            for function in functions {
                let [StatementIr::Return(result)] = function.body.statements.as_mut_slice() else {
                    panic!("one source return in {}", function.name);
                };
                if matches!(function.name.as_str(), "exprNull" | "exprUndefined") {
                    // Exercise the public ExprIr route with the same lowered
                    // operands and owner. Fold the source negation into !==.
                    let ExprIr::LogicalNot { expr } = &result.expr else {
                        panic!("source inequality retains its negation");
                    };
                    let ExprIr::SpecOperation {
                        operation: SpecOperationIr::StrictEqualityComparison,
                        operands,
                    } = &expr.expr else {
                        panic!("inequality negates its canonical strict comparison");
                    };
                    let [left, right] = operands.as_slice() else { panic!("two strict operands"); };
                    let left = left.clone();
                    let mut right = right.clone();
                    if function.name == "exprUndefined" {
                        // The explicit Undefined IR form is equivalent to
                        // this effect-free void 0, with the same value facts.
                        let ExprIr::Void { expr } = &right.expr else { panic!("source void operand"); };
                        assert!(matches!(expr.expr, ExprIr::Number(bits) if bits == 0.0f64.to_bits()));
                        right.expr = ExprIr::Undefined;
                    } else {
                        assert!(matches!(left.expr, ExprIr::Null));
                    }
                    result.expr = ExprIr::StrictEquality {
                        op: EqualityBinaryOp::StrictNotEqual,
                        lhs: Box::new(left),
                        rhs: Box::new(right),
                    };
                } else {
                    let ExprIr::SpecOperation { operation, operands } = &result.expr else {
                        panic!("{} reaches the actual SpecOperation route", function.name);
                    };
                    let expected = if function.name == "looseNull" {
                        SpecOperationIr::IsLooselyEqual
                    } else {
                        SpecOperationIr::StrictEqualityComparison
                    };
                    assert_eq!(*operation, expected);
                    let [left, right] = operands.as_slice() else { panic!("two comparison operands"); };
                    match function.name.as_str() {
                        "specNull" | "looseNull" => assert!(matches!(right.expr, ExprIr::Null)),
                        "specVoidRight" => assert!(matches!(right.expr, ExprIr::Void { .. })),
                        "specVoidLeft" => assert!(matches!(left.expr, ExprIr::Void { .. })),
                        "shadowed" => assert!(!matches!(right.expr,
                            ExprIr::Null | ExprIr::Undefined | ExprIr::Void { .. }
                        ), "an identifier named undefined is not singleton syntax"),
                        "dynamic" | "number" => {},
                        _ => unreachable!(),
                    }
                }
                identities.push((function.name.clone(), format!("js::{}#{}", function.name, function.id)));
            }
            (emit(&program).expect("singleton comparisons emit"), identities)
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
    let helpers = [
        "helper::string_equality",
        "helper::bigint_equality",
        "helper::loose_equality",
    ]
    .map(|name| {
        let (definition, _) = runtime.named(name);
        program.imported_runtime_function(&runtime, definition)
    });
    let [string_equality, bigint_equality, loose_equality] = helpers;
    for (name, identity) in functions {
        let (_, body) = program.named(&identity);
        match name.as_str() {
            "specNull" | "specVoidRight" | "specVoidLeft" | "exprNull" | "exprUndefined" => {
                for helper in [string_equality, bigint_equality, loose_equality] {
                    assert_eq!(
                        calls(body, helper),
                        0,
                        "{name} needs no general equality kernel"
                    );
                }
                let tag = if matches!(name.as_str(), "specNull" | "exprNull") {
                    WasmRuntimeValueTag::Null
                } else {
                    WasmRuntimeValueTag::Undefined
                };
                assert!(
                    compares_tag(body, tag),
                    "{name} compares the exact singleton tag"
                );
            }
            "dynamic" | "shadowed" | "number" => {
                assert!(
                    calls(body, string_equality) > 0,
                    "{name} retains String equality"
                );
                assert!(
                    calls(body, bigint_equality) > 0,
                    "{name} retains BigInt equality"
                );
                assert_eq!(calls(body, loose_equality), 0);
            }
            "looseNull" => assert_eq!(
                calls(body, loose_equality),
                1,
                "loose null retains HTMLDDA semantics"
            ),
            _ => unreachable!(),
        }
    }
}
