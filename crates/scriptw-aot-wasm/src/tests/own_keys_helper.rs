//! Real R/P signatures and call sites keep internal Lists out of public Arrays.
use super::*;
use crate::gc_types::{GcNullability, GlobalLedger, PropertyKeyTable, RuntimeModuleTypes};

fn named(bytes: &[u8], requested: &str) -> u32 {
    let matches = function_names_in_bytes(bytes)
        .into_iter()
        .filter_map(|(index, name)| (name == requested).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "one physical owner for {requested}");
    matches[0]
}

fn body(bytes: &[u8], wanted: u32) -> Vec<Operator<'_>> {
    let mut index = imported_function_count_in_bytes(bytes);
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if index == wanted {
                return body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .map(Result::unwrap)
                    .collect();
            }
            index += 1;
        }
    }
    panic!("named owner has a body");
}

fn signature(bytes: &[u8], function: u32) -> wasmparser::FuncType {
    let mut types = Vec::new();
    let mut declarations = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            Payload::TypeSection(groups) => {
                for group in groups {
                    types.extend(group.unwrap().into_types());
                }
            }
            Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    if let wasmparser::TypeRef::Func(index)
                    | wasmparser::TypeRef::FuncExact(index) = import.unwrap().ty
                    {
                        declarations.push(index);
                    }
                }
            }
            Payload::FunctionSection(functions) => {
                declarations.extend(functions.into_iter().map(Result::unwrap));
            }
            _ => {}
        }
    }
    let wasmparser::CompositeInnerType::Func(function) = &types
        [declarations[function as usize] as usize]
        .composite_type
        .inner
    else {
        panic!("actual function signature");
    };
    function.clone()
}

fn call_count(operators: &[Operator<'_>], callee: u32) -> usize {
    operators
        .iter()
        .filter(|operator| {
            matches!(operator, Operator::Call { function_index } if *function_index == callee)
        })
        .count()
}

#[test]
fn internal_own_keys_share_one_recursive_list_helper_with_whole_throw_results() {
    let artifact = emit_script(
        "function collect(source) { var out=[]; for (var key in source) out.push(key); return {...source}; } collect({a:1});",
    )
    .unwrap();
    expect_valid_module(&artifact, 1);
    let bytes = artifact.runtime().unwrap().bytes();
    Validator::new_with_features(WasmFeatures::all())
        .validate_all(bytes)
        .expect("every throw exit preserves the extended helper result shape");
    let helper = named(bytes, "helper::object_own_keys");
    let advance = named(bytes, "helper::for_in_advance");
    let reflect = named(bytes, "builtin::Reflect.ownKeys");
    let define = named(bytes, "helper::object_define_data");
    let helper_body = body(bytes, helper);
    let advance_body = body(bytes, advance);
    let public_body = body(bytes, reflect);
    assert!(
        call_count(&helper_body, helper) >= 2,
        "missing-trap fallback and target validation recurse through the typed boundary"
    );
    assert_eq!(call_count(&advance_body, helper), 1);
    assert_eq!(call_count(&public_body, helper), 1);
    assert!(call_count(&public_body, define) > 0);
    for operators in [&helper_body, &advance_body] {
        assert_eq!(
            call_count(operators, define),
            0,
            "private key Lists do not materialize public Array properties"
        );
        assert!(
            !operators.iter().any(|operator| matches!(operator,
            Operator::Call {function_index} | Operator::RefFunc {function_index}
                if *function_index == reflect)),
            "internal enumeration never materializes or invokes the public OwnKeys callable"
        );
    }

    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let wasm_encoder::HeapType::Concrete(table) = schema
        .reference_type::<PropertyKeyTable>(GcNullability::Nullable)
        .heap_type
    else {
        panic!("accepted keys use their actual GC table type");
    };
    let expected = wasmparser::ValType::Ref(
        wasmparser::RefType::new(
            true,
            wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(table)),
        )
        .unwrap(),
    );
    let signature = signature(bytes, helper);
    assert_eq!(
        signature.params().len(),
        6,
        "target triple, caller Realm, Environment and Program"
    );
    assert_eq!(
        signature.results(),
        [
            wasmparser::ValType::I32,
            wasmparser::ValType::I64,
            wasmparser::ValType::Ref(wasmparser::RefType::EQREF),
            wasmparser::ValType::I32,
            wasmparser::ValType::I32,
            expected,
        ],
        "the whole Completion precedes a separately typed nullable List"
    );
}

#[test]
fn copy_data_properties_imports_the_same_typed_own_keys_export() {
    let artifact =
        emit_script("function copy(source) { return {...source}; } copy({a:1});").unwrap();
    expect_valid_module(&artifact, 1);
    let runtime = artifact.runtime().unwrap().bytes();
    let helper = named(runtime, "helper::object_own_keys");
    let mut exported = None;
    for payload in Parser::new(0).parse_all(runtime) {
        if let Payload::ExportSection(exports) = payload.unwrap() {
            for export in exports {
                let export = export.unwrap();
                if export.kind == wasmparser::ExternalKind::Func && export.index == helper {
                    assert!(exported.replace(export.name.to_owned()).is_none());
                }
            }
        }
    }
    let exported = exported.expect("one shared R export");
    let mut index = 0;
    let mut imported = None;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::ImportSection(imports) = payload.unwrap() {
            for import in imports.into_imports() {
                let import = import.unwrap();
                if matches!(
                    import.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                ) {
                    if import.module == crate::RUNTIME_IMPORT_NAMESPACE && import.name == exported {
                        assert!(imported.replace(index).is_none());
                    }
                    index += 1;
                }
            }
        }
    }
    let imported = imported.expect("CopyDataProperties imports the accepted-list owner");
    assert_eq!(
        signature(&artifact.bytes, imported),
        signature(runtime, helper)
    );
    let copies = function_names(&artifact)
        .into_iter()
        .filter(|(_, name)| name.starts_with("js::copy#"))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert!(!copies.is_empty());
    for copy in copies {
        assert_eq!(call_count(&body(&artifact.bytes, copy), imported), 1);
    }
}
