//! Scalar callback code is a real typed P body retained by the actual callable.
use super::*;
use crate::gc_types::{ExecutableCode, GcNullability, GlobalLedger, RuntimeModuleTypes};
use crate::module::StaticSignature;
use wasmparser::{CompositeInnerType, StorageType, SubType, TypeRef, UnpackedIndex};

fn types_and_declarations(bytes: &[u8]) -> (Vec<SubType>, Vec<u32>) {
    let mut types = Vec::new();
    let mut declarations = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("module decodes") {
            Payload::TypeSection(groups) => {
                for group in groups {
                    types.extend(group.expect("type group decodes").types().cloned());
                }
            }
            Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    if let TypeRef::Func(index) | TypeRef::FuncExact(index) =
                        import.expect("import decodes").ty
                    {
                        declarations.push(index);
                    }
                }
            }
            Payload::FunctionSection(functions) => {
                declarations.extend(functions.into_iter().map(Result::unwrap))
            }
            _ => {}
        }
    }
    (types, declarations)
}

fn thunk_indices(artifact: &WasmArtifact) -> BTreeSet<u32> {
    function_names(artifact)
        .into_iter()
        .filter_map(|(index, name)| name.starts_with("js-numeric::").then_some(index))
        .collect()
}

fn code_type_index() -> u32 {
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Standalone)
        .finalize_globals(GlobalLedger::new(), false, 0);
    let wasm_encoder::HeapType::Concrete(index) = registered
        .globals()
        .schema()
        .reference_type::<ExecutableCode>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("code uses a concrete registered type");
    };
    index
}

#[test]
fn ordinary_and_arrow_number_callbacks_publish_real_scalar_thunks() {
    let artifact = emit_script(
        "function ordinary(a,b){return a-b;} var arrow=(a,b)=>a-b; [3,1,2].sort(ordinary); [3,1,2].sort(arrow);"
    ).expect("binary callbacks emit");
    expect_valid_module(&artifact, 2);
    let thunks = thunk_indices(&artifact);
    assert_eq!(
        thunks.len(),
        2,
        "both ordinary and Arrow source proofs publish a thunk"
    );
    let (types, declarations) = types_and_declarations(&artifact.bytes);
    let signature = StaticSignature::BinaryNumericCallback.type_index();
    let CompositeInnerType::Func(function) = &types[signature as usize].composite_type.inner else {
        panic!("scalar callback signature is an actual function type");
    };
    assert_eq!(
        function.params(),
        &[wasmparser::ValType::F64, wasmparser::ValType::F64]
    );
    assert_eq!(
        function.results(),
        &[wasmparser::ValType::I32, wasmparser::ValType::F64]
    );
    let mut referenced = BTreeSet::new();
    let mut index = imported_function_count(&artifact);
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("P decodes") else {
            continue;
        };
        let operators = body
            .get_operators_reader()
            .expect("body opens")
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .expect("operators decode");
        for operator in &operators {
            if let Operator::RefFunc { function_index } = operator {
                referenced.insert(*function_index);
            }
        }
        if thunks.contains(&index) {
            assert_eq!(
                declarations[index as usize], signature,
                "physical declaration uses the dedicated scalar signature"
            );
            assert_eq!(
                body.get_locals_reader()
                    .unwrap()
                    .into_iter()
                    .map(Result::unwrap)
                    .map(|(count, _)| count)
                    .sum::<u32>(),
                0
            );
            assert_eq!(
                operators
                    .iter()
                    .filter(|op| matches!(op, Operator::F64Sub))
                    .count(),
                1
            );
            assert!(
                operators.windows(2).any(|pair| matches!(
                    pair,
                    [Operator::I32Const { value: 128 }, Operator::I32LtU]
                )),
                "the P thunk owns source-depth admission"
            );
            assert!(operators.iter().all(|op| matches!(op,
                Operator::GlobalGet { .. } | Operator::I32Const { .. } | Operator::I32LtU
                | Operator::If { .. } | Operator::LocalGet { .. } | Operator::F64Sub
                | Operator::F64Const { .. } | Operator::Return | Operator::End)),
                "a pure scalar body has no argv, allocation, source activation, completion or generic call: {operators:?}");
        }
        index += 1;
    }
    assert!(
        thunks.is_subset(&referenced),
        "the real function objects retain each actual typed P entry"
    );
}

#[test]
fn optional_callback_field_is_typed_immutable_and_native_code_publishes_null() {
    let artifact = emit_script("function excluded(a,b,c){return a-b;} [3,1].sort(excluded);")
        .expect("excluded callback emits");
    expect_valid_module(&artifact, 1);
    assert!(thunk_indices(&artifact).is_empty());
    let (types, _) = types_and_declarations(&artifact.bytes);
    let code = code_type_index();
    let CompositeInnerType::Struct(record) = &types[code as usize].composite_type.inner else {
        panic!("actual ExecutableCode record");
    };
    let callback = record.fields.last().expect("optional scalar entry");
    assert!(!callback.mutable);
    let heap = wasmparser::HeapType::Concrete(UnpackedIndex::Module(
        StaticSignature::BinaryNumericCallback.type_index(),
    ));
    assert_eq!(
        callback.element_type,
        StorageType::Val(wasmparser::ValType::Ref(
            wasmparser::RefType::new(true, heap).unwrap()
        ))
    );
    let has_null_publication = |bytes: &[u8]| {
        Parser::new(0).parse_all(bytes).any(|payload| {
            let Payload::CodeSectionEntry(body) = payload.expect("module decodes") else {
                return false;
            };
            let operators = body
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            operators.windows(2).any(|pair| {
                matches!(pair,
                [Operator::RefNull { hty }, Operator::StructNew { struct_type_index }]
                if *hty == heap && *struct_type_index == code)
            })
        })
    };
    assert!(
        has_null_publication(&artifact.bytes),
        "excluded source initializes the field to typed null"
    );
    let runtime = artifact.runtime().expect("callback program links R");
    assert!(
        has_null_publication(runtime.bytes()),
        "native code factories initialize the field to typed null"
    );
    let (_, runtime_declarations) = types_and_declarations(runtime.bytes());
    assert!(
        !runtime_declarations.contains(&StaticSignature::BinaryNumericCallback.type_index()),
        "R uses actual P entries and defines no source callback thunk"
    );
}

#[test]
fn callback_entry_requires_exact_binary_pure_body_and_retains_the_node_budget() {
    for source in [
        "function compare(a){return a-1;} [2,1].sort(compare);",
        "function compare(a,b,c){return a-b;} [2,1].sort(compare);",
        "function compare(a,a){return a-1;} [2,1].sort(compare);",
        "function compare(a,b=1){return a-b;} [2,1].sort(compare);",
        "function compare(a,...b){return a-b[0];} [2,1].sort(compare);",
        "function compare(a,b){return arguments[0]-b;} [2,1].sort(compare);",
        "function compare(a,b){return globalThis.observe(a,b);} [2,1].sort(compare);",
        "function outer(){var offset=1;return function compare(a,b){return a-b+offset;};} [2,1].sort(outer());",
        "async function compare(a,b){return a-b;} [2,1].sort(compare);",
        "function* compare(a,b){return a-b;} [2,1].sort(compare);",
        "class Owner {compare(a,b){return a-b;}} [2,1].sort(new Owner().compare);",
    ] {
        let artifact = emit_script(source).expect("excluded source emits its ordinary route");
        expect_valid_module(&artifact, 1);
        assert!(thunk_indices(&artifact).is_empty(), "{source}");
    }
    for (leaves, expected) in [(8, 1), (9, 0)] {
        let expression = vec!["a"; leaves].join("*");
        let source = format!("function compare(a,b){{return {expression};}} [2,1].sort(compare);");
        let artifact = emit_script(&source).expect("bounded callback emits");
        expect_valid_module(&artifact, 1);
        assert_eq!(thunk_indices(&artifact).len(), expected, "{source}");
    }
}

#[test]
fn real_runtime_comparators_call_the_optional_typed_entry() {
    let artifact = emit_script("[3,1,2].sort((a,b)=>a-b);").expect("sort callback emits");
    let runtime = artifact.runtime().expect("sort uses R");
    let mut typed_calls = 0;
    let signature = StaticSignature::BinaryNumericCallback.type_index();
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        if let Payload::CodeSectionEntry(body) = payload.expect("R decodes") {
            typed_calls += body.get_operators_reader().unwrap().into_iter().filter(|operator| {
                matches!(operator.as_ref().unwrap(), Operator::CallRef { type_index } if *type_index == signature)
            }).count();
        }
    }
    assert!(
        typed_calls >= 2,
        "Array and TypedArray comparison kernels invoke their actual P scalar entry"
    );
}
