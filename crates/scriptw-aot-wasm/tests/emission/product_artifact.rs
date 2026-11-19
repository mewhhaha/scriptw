//! Product-artifact boundary for T27.
//!
//! Scriptw may retain source metadata for diagnostics, but emitted Wasm must not
//! carry user source as input to an evaluator. Validate the binary's types as
//! well as its imports and operators in both the program and its linked
//! runtime: successful decoding alone is not proof that either is valid Wasm.

use std::collections::BTreeMap;

use scriptw_aot_wasm::{emit, WasmArtifact, RUNTIME_IMPORT_NAMESPACE};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::types::{EntityType, TypesRef};
use wasmparser::{Operator, Parser, Payload, Validator, WasmFeatures};

use crate::product_programs;

const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;
const SOURCE_MARKER: &str = "SCRIPTW_SOURCE_MUST_NOT_FEED_A_RUNTIME_EVALUATOR_8A8D20E9";
const VALUE_MARKER: f64 = 424_242.0;

fn emit_program(name: &'static str, source: String) -> WasmArtifact {
    std::thread::Builder::new()
        .name(format!("product-artifact-{name}"))
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || {
            let parsed = parse(&source, ParseOptions::script()).expect("fixture should parse");
            emit(&lower(&parsed)).expect("fixture should compile directly to Wasm")
        })
        .expect("compiler worker should spawn")
        .join()
        .expect("compiler worker should not panic")
}

fn product_validator() -> Validator {
    let mut features = WasmFeatures::default();
    // The product deliberately targets these Wasm capabilities. Do not use
    // all(): that would silently allow unrelated experimental proposals.
    for feature in [
        WasmFeatures::THREADS,
        WasmFeatures::MULTI_MEMORY,
        WasmFeatures::REFERENCE_TYPES,
        WasmFeatures::FUNCTION_REFERENCES,
        WasmFeatures::GC,
        WasmFeatures::EXCEPTIONS,
        WasmFeatures::TAIL_CALL,
    ] {
        features.set(feature, true);
    }
    Validator::new_with_features(features)
}

fn assert_product_artifact(name: &str, artifact: &WasmArtifact, expected_value: Option<f64>) {
    let mut validator = product_validator();
    let runtime_types = artifact.runtime().map(|runtime| {
        let runtime_name = format!("{name} runtime");
        let types = validator
            .validate_all(runtime.bytes())
            .unwrap_or_else(|error| {
                panic!("{runtime_name}: emitted Wasm failed validation: {error}")
            });
        assert_product_module(&runtime_name, runtime.bytes(), types.as_ref(), None, None);
        // Preserve the canonical GC/function type context across both modules,
        // so import/export equality compares actual types, not module indices.
        validator.reset();
        types
    });
    let types = validator
        .validate_all(&artifact.bytes)
        .unwrap_or_else(|error| panic!("{name}: emitted Wasm failed validation: {error}"));
    assert_product_module(
        name,
        &artifact.bytes,
        types.as_ref(),
        runtime_types.as_ref().map(|types| types.as_ref()),
        expected_value,
    );
}

fn assert_product_module(
    name: &str,
    bytes: &[u8],
    types: TypesRef<'_>,
    runtime_types: Option<TypesRef<'_>>,
    expected_value: Option<f64>,
) {
    assert!(
        !bytes
            .windows(SOURCE_MARKER.len())
            .any(|window| window == SOURCE_MARKER.as_bytes()),
        "{name}: execution-irrelevant user source must not be embedded in the artifact"
    );

    let runtime_exports = runtime_types.map(|runtime| {
        runtime
            .core_exports()
            .expect("runtime is a core Wasm module")
            .collect::<BTreeMap<_, _>>()
    });
    for (module, import_name, ty) in types.core_imports().expect("product is a core Wasm module") {
        let boundary = format!("{module}.{import_name}").to_ascii_lowercase();
        for forbidden in [
            "eval",
            "interpreter",
            "javascript",
            "parse_source",
            "run_source",
        ] {
            assert!(
                !boundary.contains(forbidden),
                "{name}: product imports forbidden evaluator boundary {boundary}"
            );
        }
        if module == RUNTIME_IMPORT_NAMESPACE {
            let exports = runtime_exports
                .as_ref()
                .expect("only a linked program may import from its supplied runtime");
            assert_eq!(
                exports.get(import_name),
                Some(&ty),
                "{name}: {boundary} must match the supplied runtime export's kind and exact type"
            );
        } else if matches!(ty, EntityType::Func(_) | EntityType::FuncExact(_)) {
            assert_eq!(
                module, "scriptw_host",
                "{name}: function imports must cross the typed Scriptw host ABI"
            );
        }
    }

    let mut saw_compiled_value = false;
    let mut code_bodies = 0usize;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("emitted Wasm should parse") {
            Payload::CodeSectionEntry(body) => {
                code_bodies += 1;
                for operator in body
                    .get_operators_reader()
                    .expect("operator reader should construct")
                {
                    match operator.expect("operator should decode") {
                        Operator::F64Const { value } => {
                            if let Some(expected) = expected_value {
                                saw_compiled_value |= value.bits() == expected.to_bits();
                            }
                        }
                        Operator::I64Const { value } => {
                            if let Some(expected) = expected_value {
                                saw_compiled_value |=
                                    value == expected.to_bits() as i64 || value == expected as i64;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    assert!(
        code_bodies > 0,
        "{name}: product artifact must contain compiled code"
    );
    if expected_value.is_some() {
        assert!(
            saw_compiled_value,
            "{name}: the user program's numeric semantics must appear in emitted instructions"
        );
    }
}

#[test]
fn product_wasm_contains_compiled_semantics_without_a_source_evaluator() {
    let source = format!(
        "/* {SOURCE_MARKER} */ function answer() {{ return {VALUE_MARKER}; }} print(answer());"
    );
    let artifact = emit_program("numeric-marker", source);
    assert_product_artifact("numeric-marker", &artifact, Some(VALUE_MARKER));
}

#[test]
fn representative_language_families_produce_valid_aot_artifacts() {
    assert!(!product_programs::CASES.is_empty());
    for fixture in product_programs::CASES {
        assert!(
            !fixture.expected_stdout.is_empty(),
            "{}: a product fixture must declare its execution expectation",
            fixture.name
        );
        let artifact = emit_program(fixture.name, fixture.source.to_owned());
        assert_product_artifact(fixture.name, &artifact, None);
    }
}

#[test]
fn validator_rejects_a_parseable_function_result_type_mismatch() {
    // (module (func (result i32) i32.const 7))
    let valid = [
        0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 2, 1, 0, 10, 6, 1, 4, 0, 65, 7, 11,
    ];
    assert!(product_validator().validate_all(&valid).is_ok());
    let mut invalid = valid;
    assert_eq!(invalid[24], 0x41);
    invalid[24] = 0x42; // i64.const 7, while the function still promises i32.

    for payload in Parser::new(0).parse_all(&invalid) {
        if let Payload::CodeSectionEntry(body) = payload.expect("control should remain parseable") {
            for operator in body
                .get_operators_reader()
                .expect("control operators should decode")
            {
                operator.expect("control instruction should remain parseable");
            }
        }
    }
    assert!(
        product_validator().validate_all(&invalid).is_err(),
        "decodable but ill-typed Wasm must not pass the product artifact gate"
    );
}
