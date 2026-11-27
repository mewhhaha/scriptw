//! Inspect real dynamic callsites, canonical source bodies, and argv adapters.
use super::*;
use crate::gc_types::{
    ArgumentsObject, ExecutableCode, GcFunctionRole, GcNullability, GlobalLedger,
    LocalArgumentsCallable, RuntimeModuleTypes, StoredValue, ValueArray,
    WideLocalArgumentsCallable,
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
    let (actuals, observations, snapshot_count, signature) = if wide {
        (
            "row,1,2,3,4,5",
            "+arguments.length+arguments[5]",
            6,
            WideLocalArgumentsCallable::SIGNATURE.type_index(),
        )
    } else {
        ("row", "", 1, LocalArgumentsCallable::SIGNATURE.type_index())
    };
    let ending = if tail {
        format!("return steps[index & 1]({actuals});")
    } else {
        format!("var value = steps[index & 1]({actuals}); return value;")
    };
    let artifact = emit_script(&format!(
        "'use strict'; var steps; function prepare(seed) {{ var tick=seed; steps=[function(row){{tick++;return row.x+tick{observations};}},function(row){{tick+=3;return row.y+tick{observations};}}]; }} function frame(index,row) {{ {ending} }} prepare(7); frame(globalThis.index,globalThis.row);"
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
            let operators = crate::tests::emitted_operators(&body);
            let call = operators
                .iter()
                .position(|op| match op {
                    Operator::CallRef { type_index } if !tail => *type_index == signature,
                    Operator::ReturnCallRef { type_index } if tail => *type_index == signature,
                    _ => false,
                })
                .expect("actual dynamic call reaches the typed source body");
            if wide {
                assert!(
                    operators[..call].windows(2).any(|ops| matches!(
                        ops,
                        [Operator::I32Const { value: 6 }, Operator::RefNull { hty }]
                            if *hty == wasmparser::HeapType::Concrete(
                                wasmparser::UnpackedIndex::Module(vector)
                            )
                    )),
                    "the six-argument call passes the actual count and no argv"
                );
            }
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
fn six_actual_dynamic_calls_to_arguments_readers_allocate_argv_only_on_guard_miss() {
    inspect_dynamic_call(false, true);
}

#[test]
fn six_actual_dynamic_tail_calls_to_arguments_readers_use_the_wide_typed_entry() {
    inspect_dynamic_call(true, true);
}

#[test]
fn only_projected_strict_arguments_reads_get_counted_local_bodies() {
    let artifact = emit_script(
        r#"
        function projectedLength() { 'use strict'; return arguments.length; }
        function projectedIndex(index) { 'use strict'; return arguments[index]; }
        function escaped() { 'use strict'; return arguments; }
        function mutated(value) { 'use strict'; arguments[0] = value; return arguments.length; }
        function deleted() { 'use strict'; delete arguments[0]; return arguments.length; }
        function presence() { 'use strict'; return 0 in arguments; }
        function callee() { 'use strict'; return arguments.callee; }
        function iterator() { 'use strict'; return arguments[Symbol.iterator]; }
        function sloppy(value) { value = 7; return arguments[0]; }
        function directEval() { 'use strict'; return eval('arguments.length'); }
        function complexParameters() {
            'use strict';
            function defaults(value = 7) { return value; }
            function rest(...values) { return values[0]; }
            return [defaults, rest];
        }
        "#,
    )
    .expect("arguments observer controls emit");
    expect_valid_module(&artifact, 13);
    let wide = WideLocalArgumentsCallable::SIGNATURE.type_index();
    canonical_index_for(&artifact, "projectedLength", wide);
    canonical_index_for(&artifact, "projectedIndex", wide);
    let signatures = declarations(&artifact);
    let names = function_names(&artifact);
    for source in [
        "escaped",
        "mutated",
        "deleted",
        "presence",
        "callee",
        "iterator",
        "sloppy",
        "directEval",
        "defaults",
        "rest",
    ] {
        let (index, name) = names
            .iter()
            .find(|(_, name)| name.starts_with(&format!("js::{source}#")))
            .expect("vector-only control has a compiled body");
        assert_eq!(
            signatures[*index as usize],
            crate::StaticSignature::JavaScriptFunction.type_index(),
            "{name} needs the incoming argument vector"
        );
    }
}

#[test]
fn counted_reads_use_the_entry_count_and_build_arguments_after_a_missed_index() {
    let artifact = emit_script(
        "'use strict'; function argCount() { return arguments.length; } function read(index) { return arguments[index]; }",
    )
    .expect("counted read bodies emit");
    expect_valid_module(&artifact, 2);
    let wide = WideLocalArgumentsCallable::SIGNATURE.type_index();
    let count_body = canonical_index_for(&artifact, "argCount", wide);
    let read_body = canonical_index_for(&artifact, "read", wide);
    let registered = RuntimeModuleTypes::register(crate::tests::standalone_kind())
        .finalize_globals(GlobalLedger::new(), false, 0);
    let wasm_encoder::HeapType::Concrete(arguments_type) = registered
        .globals()
        .schema()
        .reference_type::<ArgumentsObject>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("ArgumentsObject has a registered concrete type");
    };
    let arguments_allocation = |op: &Operator<'_>| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == arguments_type);
    let mut index = imported_function_count(&artifact);
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("P decodes") else {
            continue;
        };
        if index == count_body {
            let operators = crate::tests::emitted_operators(&body);
            let conversion = operators
                .windows(2)
                .position(|ops| {
                    matches!(
                        ops,
                        [
                            Operator::LocalGet { local_index: 25 },
                            Operator::F64ConvertI32U
                        ]
                    )
                })
                .expect("the incoming count becomes a JavaScript Number");
            assert!(
                operators[..conversion]
                    .iter()
                    .all(|op| !arguments_allocation(op)),
                "the .length fast path reads its count before any arguments object allocation"
            );
        }
        if index == read_body {
            let operators = crate::tests::emitted_operators(&body);
            let conversion = operators
                .iter()
                .position(|op| matches!(op, Operator::I32TruncSatF64U))
                .expect("the index is tested before reading a lane");
            let allocation = operators
                .iter()
                .position(arguments_allocation)
                .expect("an absent own index can need the real object and its prototype");
            assert!(conversion < allocation, "the entry leaves the object lazy");
            assert!(
                operators[conversion..allocation]
                    .iter()
                    .any(|op| matches!(op, Operator::BrIf { .. })),
                "the numeric index check guards the local read and its fallback"
            );
            assert!(
                operators[conversion..allocation]
                    .iter()
                    .any(|op| matches!(op, Operator::BrTable { .. })),
                "an in-range computed index selects an original lane directly"
            );
        }
        index += 1;
    }
}

fn inspect_vector_adapter(wide: bool) {
    let (source, signature) = if wide {
        (
            "'use strict'; function make(index){return [arguments.length, arguments[index]];} function overflow(callback){return callback(6,1,2,3,4,5,6);} globalThis.saved=make; globalThis.overflow=overflow;",
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
    expect_valid_module(&artifact, if wide { 2 } else { 1 });
    let canonical = canonical_index_for(&artifact, "make", signature);
    let overflow = wide.then(|| canonical_index(&artifact, "overflow"));
    let mut adapters = 0;
    let mut index = imported_function_count(&artifact);
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("P decodes") else {
            continue;
        };
        let operators = crate::tests::emitted_operators(&body);
        if Some(index) == overflow {
            assert!(
                operators
                    .iter()
                    .any(|op| matches!(op, Operator::ArrayNewFixed { array_size: 7, .. })),
                "an over-width call keeps every actual in argv"
            );
            assert!(
                operators.iter().all(|op| !matches!(
                    op,
                    Operator::CallRef { type_index } | Operator::ReturnCallRef { type_index }
                        if *type_index == LocalArgumentsCallable::SIGNATURE.type_index()
                            || *type_index == WideLocalArgumentsCallable::SIGNATURE.type_index()
                )),
                "an over-width call enters through the ordinary argv signature"
            );
        }
        if operators.iter().any(|op| {
            matches!(op, Operator::ReturnCall { function_index } if *function_index == canonical)
        }) {
            adapters += 1;
            if wide {
                assert!(
                    operators.windows(3).any(|ops| matches!(
                        ops,
                        [
                            Operator::LocalGet { local_index: 7 },
                            Operator::I32WrapI64,
                            Operator::LocalGet { local_index: 8 },
                        ]
                    )),
                    "the adapter forwards the actual count and the original vector for overflow"
                );
            }
            assert!(operators.iter().all(|op| !matches!(op,
                Operator::StructNew { .. } | Operator::StructNewDefault { .. }
                | Operator::ArrayNew { .. } | Operator::ArrayNewDefault { .. }
                | Operator::ArrayNewFixed { .. } | Operator::GlobalSet { .. })),
                "the adapter neither allocates nor installs a second source activation");
        }
        index += 1;
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
fn counted_vector_entry_tail_enters_the_wide_body_with_its_original_arguments() {
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
    // The catch keeps the source body out of the small-function specializer
    // while admitting the canonical local-argument entry.
    let program = lower(
        &parse(
            &format!(
                "function frame({formals}){{try{{return row.x;}}catch(e){{throw e;}}}} export function run(row){{{ending}}}"
            ),
            ParseOptions::module(),
        )
        .unwrap(),
    );
    let artifact = emit(&program).expect("known Module source calls emit");
    expect_valid_module(&artifact, 2);
    let signatures = declarations(&artifact);
    let names = function_names(&artifact);
    let vector_adapters: BTreeSet<_> = names
        .iter()
        .filter_map(|(index, name)| {
            name.starts_with("js-arguments-adapter::frame#")
                .then_some(*index)
        })
        .collect();
    assert!(!vector_adapters.is_empty());
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
            let operators = crate::tests::emitted_operators(&body);
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
            // Direct source calls guard against either the P-private cached
            // ExecutableCode global (GlobalGet + RefEq) or explicit SOURCE_ID,
            // KIND, and DEFINING_PROGRAM checks.
            let uses_cached_code = operators[..direct].windows(4).any(|window| {
                matches!(
                    window,
                    [
                        Operator::LocalGet { .. },
                        Operator::RefAsNonNull,
                        Operator::GlobalGet { .. },
                        Operator::RefEq
                    ]
                )
            });
            if !uses_cached_code {
                assert!(
                    operators[..direct]
                        .iter()
                        .any(|op| matches!(op, Operator::I64Eq)),
                    "actual source identity guards the direct arm"
                );
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
                for (field, storage) in [(0, wasmparser::ValType::I64), (1, wasmparser::ValType::I32)]
                {
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
            }
            assert!(
                operators[..direct]
                    .iter()
                    .any(|op| matches!(op, Operator::RefEq)),
                "actual defining program identity guards the direct arm"
            );
            assert!(
                operators[direct + 1..].iter().any(|op| matches!(
                    op,
                    Operator::Call { .. } | Operator::ReturnCall { .. }
                )),
                "an identity miss falls back to generic argv dispatch without duplicating indirect call_ref entries"
            );
            assert!(
                !operators[direct + 1..].iter().any(|op| matches!(
                    op,
                    Operator::CallRef { .. } | Operator::ReturnCallRef { .. }
                )),
                "a planned direct call does not duplicate published call_ref dispatch inline"
            );
            assert!(
                !operators[direct + 1..].iter().any(|op| matches!(
                    op,
                    Operator::Call { function_index } | Operator::ReturnCall { function_index }
                        if vector_adapters.contains(function_index)
                )),
                "the missed identity guard does not retry the same target through its vector adapter"
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
            let operators = crate::tests::emitted_operators(&body);
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
