//! Inspect real dynamic callsites, canonical source bodies, and argv adapters.
use super::*;
use crate::gc_types::{
    ExecutableCode, GcFunctionRole, GcNullability, GlobalLedger, LocalArgumentsCallable,
    RuntimeModuleTypes, StoredValue, ValueArray, WideLocalArgumentsCallable,
};

fn declarations(artifact: &WasmArtifact) -> Vec<u32> {
    let mut result = Vec::new();
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        match payload.expect("P decodes") {
            Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    if let wasmparser::TypeRef::Func(index)
                    | wasmparser::TypeRef::FuncExact(index) = import.expect("import decodes").ty
                    {
                        result.push(index);
                    }
                }
            }
            Payload::FunctionSection(functions) => {
                result.extend(functions.into_iter().map(Result::unwrap));
            }
            _ => {}
        }
    }
    result
}

fn canonical_index(artifact: &WasmArtifact, name: &str) -> u32 {
    canonical_index_for(
        artifact,
        name,
        LocalArgumentsCallable::SIGNATURE.type_index(),
    )
}

fn canonical_index_for(artifact: &WasmArtifact, name: &str, signature: u32) -> u32 {
    let signatures = declarations(artifact);
    let matching: Vec<_> = function_names(artifact)
        .into_iter()
        .filter(|(index, actual)| {
            actual.contains(&format!("::{name}#")) && signatures[*index as usize] == signature
        })
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "one canonical source body for {name}: {matching:?}"
    );
    matching[0].0
}

fn inspect_dynamic_call(tail: bool, wide: bool) {
    let (actuals, formals, snapshot_count, signature) = if wide {
        (
            "row,1,2,3,4",
            "row,a,b,c,d",
            5,
            WideLocalArgumentsCallable::SIGNATURE.type_index(),
        )
    } else {
        (
            "row",
            "row",
            1,
            LocalArgumentsCallable::SIGNATURE.type_index(),
        )
    };
    let ending = if tail {
        format!("return steps[index & 1]({actuals});")
    } else {
        format!("var value = steps[index & 1]({actuals}); return value;")
    };
    let artifact = emit_script(&format!(
        "'use strict'; var steps; function prepare(seed) {{ var tick=seed; steps=[function({formals}){{tick++;return row.x+tick;}},function({formals}){{tick+=3;return row.y+tick;}}]; }} function frame(index,row) {{ {ending} }} prepare(7); frame(globalThis.index,globalThis.row);"
    ))
    .expect("capturing dynamic calls emit");
    expect_valid_module(&artifact, 4);
    let canonical = canonical_index(&artifact, "frame");
    let registered = RuntimeModuleTypes::register(crate::tests::standalone_kind())
        .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let concrete = |heap| {
        let wasm_encoder::HeapType::Concrete(index) = heap else {
            panic!("registered GC heap uses a concrete type");
        };
        index
    };
    let stored = concrete(
        schema
            .reference_type::<StoredValue>(GcNullability::NonNullable)
            .heap_type,
    );
    let vector = concrete(
        schema
            .reference_type::<ValueArray>(GcNullability::NonNullable)
            .heap_type,
    );
    let mut index = imported_function_count(&artifact);
    let mut inspected = false;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("P decodes") else {
            continue;
        };
        if index == canonical {
            let operators = body
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            let call = operators
                .iter()
                .position(|op| match op {
                    Operator::CallRef { type_index } if !tail => *type_index == signature,
                    Operator::ReturnCallRef { type_index } if tail => *type_index == signature,
                    _ => false,
                })
                .expect("actual dynamic call reaches the typed source body");
            assert!(
                operators[..call]
                    .iter()
                    .any(|op| matches!(op, Operator::RefIsNull)),
                "the actual callable's optional entry is checked before calling"
            );
            let miss_guard = operators
                .windows(3)
                .enumerate()
                .find_map(|(position, window)| {
                    let [Operator::LocalGet { local_index }, Operator::I32Eqz, Operator::If { .. }] = window else {
                        return None;
                    };
                    if position <= call {
                        return None;
                    }
                    operators[call + 1..position]
                        .windows(2)
                        .any(|window| matches!(window,
                            [Operator::I32Const { value: 1 }, Operator::LocalSet { local_index: handled }]
                            if handled == local_index))
                        .then_some(position)
                })
                .expect("typed hit sets the flag tested by the guard-miss branch");
            let mut depth = 0;
            let miss_end = operators[miss_guard + 3..]
                .iter()
                .position(|operator| match operator {
                    Operator::Block { .. }
                    | Operator::If { .. }
                    | Operator::Loop { .. }
                    | Operator::TryTable { .. } => {
                        depth += 1;
                        false
                    }
                    Operator::End if depth == 0 => true,
                    Operator::End => {
                        depth -= 1;
                        false
                    }
                    _ => false,
                })
                .map(|offset| miss_guard + 3 + offset)
                .expect("guard-miss branch is structurally complete");
            let mut snapshots = 0;
            let mut vectors = 0;
            for (position, op) in operators.iter().enumerate() {
                let allocation = match op {
                    Operator::StructNew { struct_type_index } if *struct_type_index == stored => {
                        snapshots += 1;
                        true
                    }
                    Operator::ArrayNewFixed {
                        array_type_index, ..
                    } if *array_type_index == vector => {
                        vectors += 1;
                        true
                    }
                    _ => false,
                };
                if allocation {
                    assert!(
                        position > call,
                        "argument allocation precedes the fast source call"
                    );
                    assert!(
                        position > miss_guard && position < miss_end,
                        "argument allocation belongs to the guard-miss fallback"
                    );
                }
            }
            assert_eq!(
                snapshots, snapshot_count,
                "each original actual needs exactly one fallback snapshot"
            );
            assert_eq!(vectors, 1, "the guard-miss branch owns one argv");
            inspected = true;
        }
        index += 1;
    }
    assert!(inspected);
}

#[test]
fn dynamic_capturing_calls_allocate_argument_vectors_only_on_guard_miss() {
    inspect_dynamic_call(false, false);
}

#[test]
fn dynamic_capturing_tail_calls_reach_the_typed_entry_without_an_argument_vector() {
    inspect_dynamic_call(true, false);
}

#[test]
fn five_actual_dynamic_calls_allocate_five_snapshots_and_an_argv_only_on_guard_miss() {
    inspect_dynamic_call(false, true);
}

#[test]
fn five_actual_dynamic_tail_calls_use_the_wide_typed_entry_before_any_argv() {
    inspect_dynamic_call(true, true);
}

#[test]
fn arguments_observers_and_complex_parameters_retain_the_vector_body() {
    let artifact = emit_script(
        "'use strict'; function observed(a,b,c,d,e){return arguments.length;} function defaults(a,b,c,d,e=7){return e;} function rest(...a){return a[0];} observed(1); defaults(); rest(1);"
    ).expect("vector-only source functions emit");
    expect_valid_module(&artifact, 3);
    let signatures = declarations(&artifact);
    for (index, name) in function_names(&artifact) {
        if ["observed", "defaults", "rest"]
            .iter()
            .any(|source| name.contains(&format!("::{source}#")))
        {
            for signature in [
                LocalArgumentsCallable::SIGNATURE.type_index(),
                WideLocalArgumentsCallable::SIGNATURE.type_index(),
            ] {
                assert_ne!(
                    signatures[index as usize], signature,
                    "{name} needs the actual incoming argument list"
                );
            }
        }
    }
}

fn inspect_vector_adapter(wide: bool) {
    let (source, signature) = if wide {
        (
            "'use strict'; function make(a,b,c,d,e){return {a,b,c,d,e};} globalThis.saved=make;",
            WideLocalArgumentsCallable::SIGNATURE.type_index(),
        )
    } else {
        (
            "'use strict'; function make(value){return {value};} globalThis.saved=make;",
            LocalArgumentsCallable::SIGNATURE.type_index(),
        )
    };
    let artifact =
        emit_script(source).expect("escaping source callable emits both invocation entries");
    expect_valid_module(&artifact, 1);
    let canonical = canonical_index_for(&artifact, "make", signature);
    let mut adapters = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("P decodes") else {
            continue;
        };
        let operators = body
            .get_operators_reader()
            .unwrap()
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        if operators.iter().any(|op| {
            matches!(op, Operator::ReturnCall { function_index } if *function_index == canonical)
        }) {
            adapters += 1;
            assert!(operators.iter().all(|op| !matches!(op,
                Operator::StructNew { .. } | Operator::StructNewDefault { .. }
                | Operator::ArrayNew { .. } | Operator::ArrayNewDefault { .. }
                | Operator::ArrayNewFixed { .. } | Operator::GlobalSet { .. })),
                "the adapter neither allocates nor installs a second source activation");
        }
    }
    assert_eq!(
        adapters, 1,
        "one vector entry forwards to the canonical body"
    );
}

#[test]
fn vector_entry_is_a_thin_tail_adapter_to_the_single_canonical_body() {
    inspect_vector_adapter(false);
}

#[test]
fn five_formal_vector_entry_only_unpacks_and_tail_enters_one_wide_canonical_body() {
    inspect_vector_adapter(true);
}

fn inspect_known_source_call(tail: bool, wide: bool) {
    let (actuals, formals, target_signature) = if wide {
        (
            "row,1,2,3,4",
            "row,a,b,c,d",
            WideLocalArgumentsCallable::SIGNATURE.type_index(),
        )
    } else {
        ("row", "row", LocalArgumentsCallable::SIGNATURE.type_index())
    };
    let ending = if tail {
        format!("return frame({actuals});")
    } else {
        format!("var value=frame({actuals}); return value;")
    };
    let program = lower(
        &parse(
            &format!(
                "function frame({formals}){{return row.x;}} export function run(row){{{ending}}}"
            ),
            ParseOptions::module(),
        )
        .unwrap(),
    );
    let artifact = emit(&program).expect("known Module source calls emit");
    expect_valid_module(&artifact, 2);
    let signatures = declarations(&artifact);
    let names = function_names(&artifact);
    let mut targets = BTreeSet::new();
    let mut callers = BTreeSet::new();
    // The lowerer can create exact-context specializations with distinct IDs
    // and the same source name. Each IR identity still owns one canonical body.
    for source in &program
        .script
        .as_ref()
        .expect("Module has source IR")
        .functions
    {
        let destination = match source.name.as_str() {
            "frame" => &mut targets,
            "run" => &mut callers,
            _ => continue,
        };
        let expected = format!("js::{}#{}", source.name, source.id);
        let signature = if source.name == "frame" {
            target_signature
        } else {
            LocalArgumentsCallable::SIGNATURE.type_index()
        };
        let matching: Vec<_> = names
            .iter()
            .filter_map(|(index, actual)| {
                (*actual == expected && signatures[*index as usize] == signature).then_some(*index)
            })
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "one canonical body for IR identity {expected}"
        );
        destination.insert(matching[0]);
    }
    assert!(!targets.is_empty() && !callers.is_empty());
    let registered = RuntimeModuleTypes::register(crate::tests::standalone_kind())
        .finalize_globals(GlobalLedger::new(), false, 0);
    let wasm_encoder::HeapType::Concrete(code_type) = registered
        .globals()
        .schema()
        .reference_type::<ExecutableCode>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("ExecutableCode has its registered concrete type");
    };
    let mut index = imported_function_count(&artifact);
    let mut inspected = false;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("P decodes") else {
            continue;
        };
        if callers.contains(&index) {
            let operators = body
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            let direct = operators
                .iter()
                .position(|op| match op {
                    Operator::Call { function_index } if !tail => targets.contains(function_index),
                    Operator::ReturnCall { function_index } if tail => {
                        targets.contains(function_index)
                    }
                    _ => false,
                })
                .expect("known run to frame call directly invokes the existing canonical body");
            assert!(
                operators[..direct]
                    .iter()
                    .any(|op| matches!(op, Operator::I64Eq)),
                "actual source identity guards the direct arm"
            );
            // The encoded ExecutableCode ABI starts with immutable SOURCE_ID
            // and KIND. Inspect that prefix without exposing private field ordinals.
            let mut types = Vec::new();
            for payload in Parser::new(0).parse_all(&artifact.bytes) {
                if let Payload::TypeSection(groups) = payload.expect("P types decode") {
                    for group in groups {
                        types.extend(group.unwrap().types().cloned());
                    }
                }
            }
            let wasmparser::CompositeInnerType::Struct(code) =
                &types[code_type as usize].composite_type.inner
            else {
                panic!("actual ExecutableCode struct");
            };
            for (field, storage) in [(0, wasmparser::ValType::I64), (1, wasmparser::ValType::I32)] {
                assert!(!code.fields[field].mutable);
                assert_eq!(code.fields[field].element_type.unpack(), storage);
                let field = field as u32;
                assert!(
                    operators[..direct].iter().any(|op| matches!(op,
                    Operator::StructGet { struct_type_index, field_index }
                        if *struct_type_index == code_type && *field_index == field)),
                    "actual code metadata is read before a direct source call"
                );
            }
            assert!(
                operators[..direct]
                    .iter()
                    .any(|op| matches!(op, Operator::RefEq)),
                "actual defining program identity guards the direct arm"
            );
            assert!(
                operators[direct + 1..].iter().any(|op| match op {
                    Operator::CallRef { type_index } if !tail => *type_index == target_signature,
                    Operator::ReturnCallRef { type_index } if tail =>
                        *type_index == target_signature,
                    _ => false,
                }),
                "an identity miss still calls the actual typed source entry"
            );
            inspected = true;
        }
        index += 1;
    }
    assert!(inspected);
}

#[test]
fn known_source_calls_guard_direct_wasm_calls_to_the_canonical_body() {
    inspect_known_source_call(false, false);
}

#[test]
fn known_source_tail_calls_guard_direct_wasm_tail_calls_to_the_canonical_body() {
    inspect_known_source_call(true, false);
}

#[test]
fn known_five_argument_calls_guard_source_kind_and_defining_program_before_the_wide_body() {
    inspect_known_source_call(false, true);
}

#[test]
fn known_five_argument_tail_calls_retain_the_actual_wide_entry_on_identity_miss() {
    inspect_known_source_call(true, true);
}

#[test]
fn module_held_reference_numeric_leaf_calls_inline_guarded_arithmetic_into_the_caller() {
    let program = lower(&parse(
        "function arithmetic(a,b){return a*b+1;} export function run(a,b){var value=arithmetic(a,b);return value;}",
        ParseOptions::module(),
    ).unwrap());
    let artifact = emit(&program).expect("Module held Reference numeric leaf emits");
    expect_valid_module(&artifact, 2);
    let caller = canonical_index(&artifact, "run");
    let registered = RuntimeModuleTypes::register(crate::tests::standalone_kind())
        .finalize_globals(GlobalLedger::new(), false, 0);
    let wasm_encoder::HeapType::Concrete(stored) = registered
        .globals()
        .schema()
        .reference_type::<StoredValue>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("argument snapshots use a registered concrete type");
    };
    let mut index = imported_function_count(&artifact);
    let mut inspected = false;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("P decodes") else {
            continue;
        };
        if index == caller {
            let operators = body
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            let arithmetic: Vec<_> = operators
                .iter()
                .enumerate()
                .filter_map(|(position, op)| {
                    matches!(op, Operator::F64Mul | Operator::F64Add).then_some(position)
                })
                .collect();
            assert_eq!(
                arithmetic.len(),
                2,
                "the actual run body contains the leaf's multiply and add"
            );
            let first = arithmetic[0];
            assert!(
                operators[..first]
                    .iter()
                    .any(|op| matches!(op, Operator::RefEq)),
                "defining program identity guards the inlined source expression"
            );
            assert!(
                operators[..first]
                    .iter()
                    .any(|op| matches!(op, Operator::I64Eq)),
                "actual source identity guards the inlined source expression"
            );
            let fallback = operators[arithmetic[1] + 1..].iter().position(|op| {
                matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == stored)
            }).expect("guard miss retains rooted argument snapshots for ordinary dispatch");
            assert!(
                operators[arithmetic[1] + 1 + fallback..]
                    .iter()
                    .any(|op| matches!(op, Operator::Call { .. })),
                "guard miss invokes the actual callee through existing compiled dispatch"
            );
            inspected = true;
        }
        index += 1;
    }
    assert!(inspected);
}
