use super::*;
use scriptw_ir::foreign::{ForeignErrorTag, ForeignModuleDeclaration};
use std::borrow::Cow;
use wasm_encoder::{CustomSection, Module, TypeSection, ValType};

fn registry(error_tag: bool) -> ForeignModuleRegistry {
    let function = ForeignFunctionDeclaration::new(
        "add",
        "dd",
        "add",
        vec![ForeignValueType::Number; 2],
        ForeignResultType::Number,
    )
    .unwrap();
    let module = ForeignModuleDeclaration::new("dd:host", vec![function]).unwrap();
    let registry = ForeignModuleRegistry::new(vec![module]).unwrap();
    if error_tag {
        registry.with_error_tag(ForeignErrorTag::code_v1())
    } else {
        registry
    }
}

fn module(
    registry: Option<&ForeignModuleRegistry>,
    scalar: ValType,
    tag_payload: Option<ValType>,
    common_after_foreign: bool,
    duplicate_section: bool,
) -> Vec<u8> {
    let mut types = TypeSection::new();
    types.ty().function([scalar, scalar], [scalar]);
    if let Some(payload) = tag_payload {
        types.ty().function([payload], []);
    }
    let mut imports = ImportSection::new();
    imports.import("dd", "add", EntityType::Function(0));
    if common_after_foreign {
        imports.import(
            crate::RUNTIME_IMPORT_NAMESPACE,
            "f0",
            EntityType::Function(0),
        );
    }
    if tag_payload.is_some() {
        imports.import(
            "scriptw_foreign",
            "error_v1",
            EntityType::Tag(TagType {
                kind: TagKind::Exception,
                func_type_idx: 1,
            }),
        );
    }
    let mut module = Module::new();
    module.section(&types);
    module.section(&imports);
    if let Some(registry) = registry {
        let metadata = registry.to_normalized_json();
        let section = CustomSection {
            name: Cow::Borrowed(FOREIGN_MODULES_CUSTOM_SECTION),
            data: Cow::Borrowed(&metadata),
        };
        module.section(&section);
        if duplicate_section {
            module.section(&section);
        }
    }
    let bytes = module.finish();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&bytes)
        .expect("actual encoded fixture validates before metadata admission");
    bytes
}

#[test]
fn foreign_metadata_admits_exact_scalar_imports_and_optional_error_tag() {
    let ordinary = registry(false);
    let fallible = registry(true);
    assert_eq!(
        foreign_registry_from_module(&module(Some(&ordinary), ValType::F64, None, false, false))
            .unwrap(),
        ordinary
    );
    assert_eq!(
        foreign_registry_from_module(&module(
            Some(&fallible),
            ValType::F64,
            Some(ValType::I32),
            false,
            false
        ))
        .unwrap(),
        fallible
    );
    assert!(foreign_registry_from_module(&Module::new().finish())
        .unwrap()
        .is_empty());
}

#[test]
fn foreign_metadata_rejects_missing_authority_wrong_scalar_types_and_duplicate_sections() {
    let registry = registry(false);
    for bytes in [
        module(None, ValType::F64, None, false, false),
        module(Some(&registry), ValType::I64, None, false, false),
        module(Some(&registry), ValType::F64, None, false, true),
        module(Some(&registry), ValType::F64, None, true, false),
    ] {
        assert!(foreign_registry_from_module(&bytes).is_err());
    }
}

#[test]
fn foreign_metadata_rejects_tag_inventory_and_payload_mismatches() {
    let ordinary = registry(false);
    let fallible = registry(true);
    for bytes in [
        module(
            Some(&ordinary),
            ValType::F64,
            Some(ValType::I32),
            false,
            false,
        ),
        module(Some(&fallible), ValType::F64, None, false, false),
        module(
            Some(&fallible),
            ValType::F64,
            Some(ValType::F64),
            false,
            false,
        ),
    ] {
        assert!(foreign_registry_from_module(&bytes).is_err());
    }
}

#[test]
fn foreign_metadata_rejects_empty_or_unsupported_version_sections() {
    let mut module = Module::new();
    module.section(&CustomSection {
        name: Cow::Borrowed(FOREIGN_MODULES_CUSTOM_SECTION),
        data: Cow::Owned(ForeignModuleRegistry::default().to_normalized_json()),
    });
    assert!(foreign_registry_from_module(&module.finish()).is_err());
    let bytes = self::module(Some(&registry(false)), ValType::F64, None, false, false);
    let version = b"\"abi_version\":2";
    let mut changed = bytes.clone();
    let matches = bytes
        .windows(version.len())
        .enumerate()
        .filter(|(_, part)| *part == version)
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "canonical actual metadata has one ABI version"
    );
    changed[matches[0] + version.len() - 1] = b'1';
    assert!(foreign_registry_from_module(&changed).is_err());
}

#[test]
fn asynchronous_metadata_requires_two_nonnull_eqref_resolvers_and_no_wasm_result() {
    let declaration = ForeignFunctionDeclaration::new_async(
        "later",
        "dd",
        "later",
        vec![ForeignValueType::Number],
        ForeignResultType::Number,
    )
    .unwrap();
    let registry = ForeignModuleRegistry::new(vec![ForeignModuleDeclaration::new(
        "dd:host",
        vec![declaration],
    )
    .unwrap()])
    .unwrap();
    let resolver = ValType::Ref(wasm_encoder::RefType {
        nullable: false,
        heap_type: wasm_encoder::HeapType::Abstract {
            shared: false,
            ty: wasm_encoder::AbstractHeapType::Eq,
        },
    });
    let any = ValType::Ref(wasm_encoder::RefType {
        nullable: false,
        heap_type: wasm_encoder::HeapType::Abstract {
            shared: false,
            ty: wasm_encoder::AbstractHeapType::Any,
        },
    });
    for (parameters, results, accepted) in [
        (vec![ValType::F64, resolver, resolver], vec![], true),
        (vec![ValType::F64, resolver], vec![], false),
        (
            vec![
                ValType::F64,
                ValType::Ref(wasm_encoder::RefType::EQREF),
                resolver,
            ],
            vec![],
            false,
        ),
        (vec![ValType::F64, any, resolver], vec![], false),
        (
            vec![ValType::F64, resolver, resolver],
            vec![ValType::F64],
            false,
        ),
    ] {
        let mut types = TypeSection::new();
        types.ty().function(parameters, results);
        let mut imports = ImportSection::new();
        imports.import("dd", "later", EntityType::Function(0));
        let mut module = Module::new();
        module.section(&types);
        module.section(&imports);
        module.section(&CustomSection {
            name: Cow::Borrowed(FOREIGN_MODULES_CUSTOM_SECTION),
            data: Cow::Owned(registry.to_normalized_json()),
        });
        let bytes = module.finish();
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
            .validate_all(&bytes)
            .unwrap();
        assert_eq!(foreign_registry_from_module(&bytes).is_ok(), accepted);
    }
}

#[test]
fn transfer_signatures_require_the_emitted_nonnull_canonical_array_types() {
    use crate::gc_types::RuntimeModuleTypes;
    use crate::module::GlobalLedger;
    use crate::runtime_artifact::ModuleKind;
    for (value, result) in [
        (
            ForeignValueType::StringUtf16,
            ForeignResultType::StringUtf16,
        ),
        (ForeignValueType::Bytes, ForeignResultType::Bytes),
    ] {
        let declaration =
            ForeignFunctionDeclaration::new("echo", "dd", "echo", vec![value], result).unwrap();
        let registry = ForeignModuleRegistry::new(vec![ForeignModuleDeclaration::new(
            "dd:host",
            vec![declaration.clone()],
        )
        .unwrap()])
        .unwrap();
        let (types, imports) = RuntimeModuleTypes::register(ModuleKind::Standalone)
            .with_foreign_modules(&registry)
            .unwrap();
        let (id, _) = registry.functions().next().unwrap();
        let index = imports.functions[&id].0;
        let runtime = types.finalize_globals(GlobalLedger::new(), false, 0);
        let mut imports = ImportSection::new();
        imports.import("dd", "echo", EntityType::Function(index));
        let mut module = Module::new();
        module.section(runtime.types());
        module.section(&imports);
        module.section(&CustomSection {
            name: Cow::Borrowed(FOREIGN_MODULES_CUSTOM_SECTION),
            data: Cow::Owned(registry.to_normalized_json()),
        });
        let bytes = module.finish();
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
            .validate_all(&bytes)
            .unwrap();
        assert_eq!(foreign_registry_from_module(&bytes).unwrap(), registry);
        let signature = wasmparser::Parser::new(0)
            .parse_all(&bytes)
            .find_map(|payload| {
                if let wasmparser::Payload::TypeSection(groups) = payload.unwrap() {
                    let subtypes = groups
                        .into_iter()
                        .flat_map(|group| group.unwrap().into_types())
                        .collect::<Vec<_>>();
                    let wasmparser::CompositeInnerType::Func(signature) =
                        &subtypes[index as usize].composite_type.inner
                    else {
                        panic!("foreign import uses a function type");
                    };
                    Some(signature.clone())
                } else {
                    None
                }
            })
            .unwrap();
        let wasmparser::ValType::Ref(reference) = signature.params()[0] else {
            panic!("transfer ABI requires a concrete array reference");
        };
        assert!(!reference.is_nullable());
        let nullable = wasmparser::ValType::Ref(
            wasmparser::RefType::new(true, reference.heap_type()).unwrap(),
        );
        for bad in [
            wasmparser::FuncType::new([nullable], signature.results().iter().copied()),
            wasmparser::FuncType::new(signature.params().iter().copied(), [nullable]),
            wasmparser::FuncType::new(
                [wasmparser::ValType::Ref(wasmparser::RefType::EQREF)],
                signature.results().iter().copied(),
            ),
        ] {
            assert!(require_foreign_signature(&bad, &declaration).is_err());
        }
        let wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(canonical)) =
            reference.heap_type()
        else {
            panic!("transfer array owns its exact module type index");
        };
        let unrelated = wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                false,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(canonical + 1)),
            )
            .unwrap(),
        );
        assert!(require_foreign_signature(
            &wasmparser::FuncType::new([unrelated], signature.results().iter().copied(),),
            &declaration
        )
        .is_err());
    }
}
