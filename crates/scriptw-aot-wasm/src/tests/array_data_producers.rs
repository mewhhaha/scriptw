//! Every producer calls the shared packed/default-data publication owner.
use super::*;
use crate::gc_types::{
    GcFunctionRole, GcNullability, GlobalLedger, LocalArgumentsCallable, NumberBitsI64Array,
    NumberPresenceU64Array, PackedValueTable, RuntimeModuleTypes, ValueArray,
    WideLocalArgumentsCallable,
};

fn named_body<'a>(bytes: &'a [u8], requested: &str) -> (u32, Vec<Operator<'a>>) {
    let names = function_names_in_bytes(bytes)
        .into_iter()
        .filter(|(_, name)| {
            if requested.ends_with('#') {
                assert!(requested.starts_with("js::"));
                name.starts_with(requested)
            } else {
                name.as_str() == requested
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        names.len(),
        1,
        "one actual body for {requested}; matches: {names:?}"
    );
    let named_index = names[0].0;
    let mut index = imported_function_count_in_bytes(bytes);
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.expect("producer module decodes") {
            if index == named_index {
                return (
                    index,
                    body.get_operators_reader()
                        .expect("producer body opens")
                        .into_iter()
                        .map(|operator| operator.expect("producer operator decodes"))
                        .collect(),
                );
            }
            index += 1;
        }
    }
    panic!("named producer has no actual body");
}

fn imported_runtime_definition(program: &[u8], runtime: &[u8], definition: u32) -> u32 {
    let mut exports = BTreeMap::new();
    for payload in Parser::new(0).parse_all(runtime) {
        if let Payload::ExportSection(section) = payload.expect("runtime decodes") {
            for export in section {
                let export = export.expect("runtime export decodes");
                if export.kind == wasmparser::ExternalKind::Func {
                    exports.insert(export.name.to_owned(), export.index);
                }
            }
        }
    }
    let mut matches = Vec::new();
    let mut index = 0;
    for payload in Parser::new(0).parse_all(program) {
        if let Payload::ImportSection(section) = payload.expect("program decodes") {
            for import in section.into_imports() {
                let import = import.expect("program import decodes");
                if matches!(
                    import.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                ) {
                    if import.module == crate::RUNTIME_IMPORT_NAMESPACE
                        && exports.get(import.name) == Some(&definition)
                    {
                        matches.push(index);
                    }
                    index += 1;
                }
            }
        }
    }
    assert_eq!(matches.len(), 1, "P imports the actual R publication owner");
    matches[0]
}

fn calls(body: &[Operator<'_>], target: u32) -> bool {
    body.iter().any(|operator| {
        matches!(operator,
        Operator::Call { function_index } | Operator::ReturnCall { function_index }
            if *function_index == target)
    })
}

fn encoded_types(bytes: &[u8]) -> Vec<wasmparser::SubType> {
    let mut types = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::TypeSection(section) = payload.expect("producer module decodes") {
            for group in section {
                types.extend(group.expect("type group decodes").into_types());
            }
        }
    }
    types
}

#[test]
fn fresh_array_producers_reach_number_storage_without_an_eager_descriptor() {
    let compact = (0..128)
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",");
    // Publishing make without directly calling it retains its generic body,
    // so the edge below cannot accidentally select a specialized clone.
    let artifact = emit_script(&format!(
        "function make(input) {{ return [input, 2]; }} var compact = [{compact}]; make;"
    ))
    .expect("both literal producer forms emit");
    expect_valid_module(&artifact, 1);
    let registered = RuntimeModuleTypes::register(crate::tests::standalone_kind())
        .finalize_globals(GlobalLedger::new(), false, 0);
    let wasm_encoder::HeapType::Concrete(number_bits) = registered
        .globals()
        .schema()
        .reference_type::<NumberBitsI64Array>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("Number backing must name a concrete registered GC type");
    };
    let runtime = artifact.runtime().expect("producer program links R");
    let runtime_types = encoded_types(runtime.bytes());
    let program_types = encoded_types(&artifact.bytes);
    assert!(program_types.len() >= runtime_types.len());
    assert_eq!(
        &program_types[..runtime_types.len()],
        runtime_types.as_slice(),
        "P and R retain the same actual core type indices"
    );
    let (publication, publication_body) = named_body(
        runtime.bytes(),
        "helper::array_indexed_publish_default_data",
    );
    assert!(
        publication_body.iter().any(|operator| matches!(operator,
        Operator::ArraySet { array_type_index } if *array_type_index == number_bits)),
        "the single shared publication body writes the actual Number backing"
    );
    let imported_publication =
        imported_runtime_definition(&artifact.bytes, runtime.bytes(), publication);
    for name in ["js::make#", "helper::compact_array_literal"] {
        let (_, body) = named_body(&artifact.bytes, name);
        assert!(
            calls(&body, imported_publication),
            "P producer {name} calls the actual shared Number publication owner"
        );
        assert!(
            !body.iter().any(|operator| matches!(operator,
            Operator::ArraySet { array_type_index } if *array_type_index == number_bits)),
            "P producer {name} does not duplicate physical Number publication"
        );
    }
    for name in [
        "builtin::JSON.parse",
        "builtin::Object.values",
        "builtin::RegExp.prototype.exec",
        "helper::proxy_call",
    ] {
        let (_, body) = named_body(runtime.bytes(), name);
        assert!(
            calls(&body, publication),
            "R producer {name} calls the actual shared Number publication owner"
        );
    }
}

#[test]
fn integer_decimal_helper_keeps_private_unsigned_domain_without_float_rounding() {
    let artifact = emit_script("var a=[]; a.push('x'); String(123);").unwrap();
    let runtime = artifact.runtime().unwrap();
    let (formatter, body) = named_body(runtime.bytes(), "helper::integer_decimal_string");
    assert!(body.iter().any(|op| matches!(op, Operator::I64DivU)));
    assert!(body.iter().any(|op| matches!(op, Operator::I64RemU)));
    assert!(
        !body.iter().any(|op| matches!(
            op,
            Operator::F64ConvertI64U
                | Operator::F64ConvertI64S
                | Operator::F64ReinterpretI64
                | Operator::I64TruncF64U
                | Operator::I64TruncF64S
                | Operator::Call { .. }
        )),
        "the complete u64 magnitude must never pass through binary64 or a coercion callback"
    );
    let (_, number) = named_body(runtime.bytes(), "helper::number_to_string");
    assert!(calls(&number, formatter));
    let bound = number
        .iter()
        .position(|op| {
            matches!(op, Operator::F64Const { value }
        if value.bits() == (9_007_199_254_740_991f64).to_bits())
        })
        .expect("safe-integer bound");
    let truncation = number
        .iter()
        .position(|op| matches!(op, Operator::I64TruncF64U))
        .expect("guarded magnitude conversion");
    assert!(bound < truncation);
    // Within the safe-integer bound the integrality test is the saturating
    // i64 truncation converted back; no rounding operator remains.
    assert!(number[bound..truncation]
        .iter()
        .any(|op| matches!(op, Operator::I64TruncSatF64S)));
    assert!(!number[bound..truncation]
        .iter()
        .any(|op| matches!(op, Operator::F64Floor | Operator::F64Trunc)));
    let (_, push) = named_body(runtime.bytes(), "builtin::Array.prototype.push");
    assert!(
        calls(&push, formatter),
        "private index publication shares the exact unsigned formatter"
    );
}

#[test]
fn array_callback_builtins_use_typed_entries_and_array_from_reserves_packed_storage() {
    let artifact = emit_script(
        "const a = Array.from({length: 129}, (_, i) => i); a.map(x => x); \
         a.find(x => x); a.findIndex(x => x);",
    )
    .unwrap();
    let bytes = artifact.runtime().unwrap().bytes();
    Validator::new_with_features(crate::runtime_artifact::program_validation_features())
        .validate_all(bytes)
        .expect("the runtime validates both packed and callback dispatch branches");
    let registered = RuntimeModuleTypes::register(crate::tests::standalone_kind())
        .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let concrete = |heap| {
        let wasm_encoder::HeapType::Concrete(index) = heap else {
            panic!("a typed backing must use a concrete GC type");
        };
        index
    };
    let vector = concrete(
        schema
            .reference_type::<ValueArray>(GcNullability::NonNullable)
            .heap_type,
    );
    for (name, arguments, mapper_sites) in [
        ("builtin::Array.from", 2, 2),
        ("builtin::Array.prototype.map", 3, 1),
        ("builtin::Array.prototype.find", 3, 1),
        ("builtin::Array.prototype.findIndex", 3, 1),
    ] {
        let (_, body) = named_body(bytes, name);
        let mut dispatch = body.split(|op| {
            matches!(op, Operator::ArrayNewFixed { array_type_index, array_size }
                if *array_type_index == vector && *array_size == arguments)
        });
        for _ in 0..mapper_sites {
            let before_fallback = dispatch.next().expect("callback argv fallback");
            for signature in [
                LocalArgumentsCallable::SIGNATURE.type_index(),
                WideLocalArgumentsCallable::SIGNATURE.type_index(),
            ] {
                assert!(
                    before_fallback.iter().any(|op| matches!(op,
                    Operator::CallRef { type_index } if *type_index == signature)),
                    "{name} must attempt the typed callback before allocating its argv"
                );
            }
        }
        assert!(
            dispatch.next().is_some(),
            "{name} retains its argv fallback"
        );
        assert!(dispatch.next().is_none(), "{name} emits one argv per miss");
    }
    let (_, array_from) = named_body(bytes, "builtin::Array.from");
    for heap in [
        schema
            .reference_type::<NumberBitsI64Array>(GcNullability::NonNullable)
            .heap_type,
        schema
            .reference_type::<NumberPresenceU64Array>(GcNullability::NonNullable)
            .heap_type,
        schema
            .reference_type::<PackedValueTable>(GcNullability::NonNullable)
            .heap_type,
    ] {
        let backing = concrete(heap);
        assert_eq!(
            array_from
                .iter()
                .filter(|op| matches!(op,
                    Operator::ArrayNew { array_type_index } if *array_type_index == backing))
                .count(),
            1,
            "Array.from reserves each possible backing at a single site"
        );
    }
}
