//! Real module lowering, P import indices/wrappers, and the shared R boundary.
use scriptw_aot_wasm::{
    emit, foreign_registry_from_module, WasmArtifact, RUNTIME_IMPORT_NAMESPACE,
};
use scriptw_ir::foreign::{
    ForeignErrorTag, ForeignFunctionDeclaration, ForeignModuleDeclaration, ForeignResultType,
    ForeignValueType,
};
use scriptw_ir::{
    lower_module_graph, ModuleGraphSources, ModuleKey, ModuleRequestKeyIr, ModuleSourceIr,
};
use wasmparser::{
    CompositeInnerType, Operator, Parser, Payload, SubType, TypeRef, Validator, WasmFeatures,
};

fn compile(export: &str, native_name: &str, fallible: bool) -> WasmArtifact {
    let declaration = ForeignFunctionDeclaration::new(
        export,
        "dd",
        native_name,
        vec![ForeignValueType::Number; 2],
        ForeignResultType::Number,
    )
    .unwrap();
    compile_declaration(declaration, fallible)
}

fn compile_declaration(declaration: ForeignFunctionDeclaration, fallible: bool) -> WasmArtifact {
    let export = declaration.js_export().to_owned();
    let module = ForeignModuleDeclaration::new("dd:host", vec![declaration]).unwrap();
    let sources = ModuleGraphSources {
        realm_requests: Default::default(),
        entry: 0,
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("entry"),
                format!("import {{{export}}} from 'dd:host'; globalThis.retained=[{export}];"),
                "entry".into(),
            ),
            ModuleSourceIr::foreign(
                ModuleKey::from_host("native"),
                module,
                fallible.then_some(ForeignErrorTag::code_v1()),
            ),
        ],
        resolutions: vec![(0, ModuleRequestKeyIr::plain("dd:host"), 1)],
    };
    let program = lower_module_graph(&sources);
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    emit(&program).expect("declared imported wrapper emits real Wasm")
}

#[test]
fn asynchronous_foreign_wrapper_emits_intrinsic_capability_and_void_host_signature() {
    std::thread::Builder::new()
        .name("async-foreign-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let declaration = ForeignFunctionDeclaration::new_async(
                "later",
                "dd",
                "later",
                vec![
                    ForeignValueType::Number,
                    ForeignValueType::StringUtf16,
                    ForeignValueType::Bytes,
                ],
                ForeignResultType::Bytes,
            )
            .unwrap();
            let artifact = compile_declaration(declaration, true);
            let mut validator = Validator::new_with_features(WasmFeatures::all());
            validator
                .validate_all(&artifact.bytes)
                .expect("actual async wrapper and EH catch validate");
            let registry = foreign_registry_from_module(&artifact.bytes).unwrap();
            let (_, declaration) = registry.functions().next().unwrap();
            assert_eq!(
                declaration.mode(),
                scriptw_ir::foreign::ForeignFunctionMode::Asynchronous
            );
            assert_eq!(declaration.result(), ForeignResultType::Bytes);
            let types = types(&artifact.bytes);
            let mut foreign_index = None;
            let mut next = 0;
            for payload in Parser::new(0).parse_all(&artifact.bytes) {
                if let Payload::ImportSection(imports) = payload.unwrap() {
                    for import in imports.into_imports() {
                        let import = import.unwrap();
                        if let TypeRef::Func(index) | TypeRef::FuncExact(index) = import.ty {
                            if import.module == "dd" {
                                foreign_index = Some(next);
                                let CompositeInnerType::Func(signature) =
                                    &types[index as usize].composite_type.inner
                                else {
                                    panic!("host import must be function");
                                };
                                assert_eq!(signature.params().len(), 5);
                                assert_eq!(signature.params()[0], wasmparser::ValType::F64);
                                assert_eq!(
                                    &signature.params()[3..],
                                    [
                                        wasmparser::ValType::Ref(
                                            wasmparser::RefType::EQREF.as_non_null()
                                        ),
                                        wasmparser::ValType::Ref(
                                            wasmparser::RefType::EQREF.as_non_null()
                                        ),
                                    ]
                                );
                                assert!(
                                    signature.results().is_empty(),
                                    "fulfillment bytes travel through the owning Store checkpoint"
                                );
                            }
                            next += 1;
                        }
                    }
                }
            }
            let bodies = crate::linked_bodies::module_bodies(&artifact.bytes);
            let wrapper = bodies
                .iter()
                .find(|body| body.name.starts_with("js::later#"))
                .unwrap();
            assert!(wrapper.calls().contains(&foreign_index.unwrap()));
        })
        .unwrap()
        .join()
        .unwrap();
}

fn types(bytes: &[u8]) -> Vec<SubType> {
    let mut types = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::TypeSection(groups) = payload.unwrap() {
            for group in groups {
                types.extend(group.unwrap().into_types());
            }
        }
    }
    types
}

#[test]
fn foreign_wrappers_preserve_runtime_prefix_and_call_actual_declared_native_imports() {
    std::thread::Builder::new()
        .name("foreign-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(assert_foreign_wrappers)
        .unwrap()
        .join()
        .unwrap();
}

fn assert_foreign_wrappers() {
    let first = compile("add", "add", false);
    let second = compile("sum", "sum_other", true);
    let runtime = first.runtime().expect("foreign wrapper is linked P");
    let other = second
        .runtime()
        .expect("different declarations still use shared R");
    assert_eq!(runtime.key(), other.key());
    assert_eq!(
        runtime.bytes(),
        other.bytes(),
        "application declarations never alter R bytes or exports"
    );
    let mut validator = Validator::new_with_features(WasmFeatures::all());
    validator
        .validate_all(runtime.bytes())
        .expect("actual R validates");
    let runtime_types = types(runtime.bytes());
    for (artifact, name, native, fallible) in [
        (&first, "add", "add", false),
        (&second, "sum", "sum_other", true),
    ] {
        validator.reset();
        validator
            .validate_all(&artifact.bytes)
            .expect("actual P wrappers/imports/try_table validate");
        assert_eq!(
            &types(&artifact.bytes)[..runtime_types.len()],
            runtime_types.as_slice(),
            "P retains R's entire declared type prefix"
        );
        let admitted = foreign_registry_from_module(&artifact.bytes)
            .expect("persisted actual ABI admits without original source");
        assert_eq!(admitted.error_tag().is_some(), fallible);
        let mut functions = Vec::new();
        for payload in Parser::new(0).parse_all(&artifact.bytes) {
            if let Payload::ImportSection(imports) = payload.unwrap() {
                for import in imports.into_imports() {
                    let import = import.unwrap();
                    if let TypeRef::Func(index) | TypeRef::FuncExact(index) = import.ty {
                        functions.push((import.module.to_owned(), import.name.to_owned(), index));
                    }
                }
            }
        }
        let foreign_index = u32::try_from(functions.len() - 1).unwrap();
        assert_eq!(
            (
                &*functions[foreign_index as usize].0,
                &*functions[foreign_index as usize].1
            ),
            ("dd", native)
        );
        let runtime_imports = functions
            .iter()
            .enumerate()
            .filter(|(_, (module, _, _))| module == RUNTIME_IMPORT_NAMESPACE)
            .collect::<Vec<_>>();
        assert!(!runtime_imports.is_empty());
        for (index, (_, exported, _)) in runtime_imports {
            assert_eq!(exported, &format!("f{index}"));
        }
        let declared_types = types(&artifact.bytes);
        let CompositeInnerType::Func(signature) = &declared_types
            [functions[foreign_index as usize].2 as usize]
            .composite_type
            .inner
        else {
            panic!("foreign import must have scalar function type");
        };
        assert_eq!(signature.params(), [wasmparser::ValType::F64; 2]);
        assert_eq!(signature.results(), [wasmparser::ValType::F64]);
        let bodies = crate::linked_bodies::module_bodies(&artifact.bytes);
        let wrappers = bodies
            .iter()
            .filter(|body| body.name.starts_with(&format!("js::{name}#")))
            .collect::<Vec<_>>();
        assert_eq!(
            wrappers.len(),
            1,
            "retained first-class export has one unspecialized JS wrapper"
        );
        let wrapper = wrappers[0];
        assert!(wrapper.index > foreign_index);
        assert!(
            wrapper.calls().contains(&foreign_index),
            "actual JS wrapper must call its declared native import"
        );
        // Decode the body itself instead of asserting compiler source text.
        let mut next = functions.len() as u32;
        let mut actual_tag_catch = false;
        for payload in Parser::new(0).parse_all(&artifact.bytes) {
            if let Payload::CodeSectionEntry(body) = payload.unwrap() {
                if next == wrapper.index {
                    for operator in body.get_operators_reader().unwrap() {
                        if let Operator::TryTable { try_table } = operator.unwrap() {
                            assert_eq!(try_table.catches, vec![wasmparser::Catch::One { tag: 0, label: 0 }], "wrapper catches only the declared i32-code tag and never catch_all");
                            actual_tag_catch = true;
                        }
                    }
                }
                next += 1;
            }
        }
        assert_eq!(
            actual_tag_catch, fallible,
            "only an explicitly declared expected-error tag is caught"
        );
    }
}
