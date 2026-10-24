use std::collections::{BTreeMap, BTreeSet};

use super::linked_bodies::module_bodies;
use scriptw_aot_wasm::{GcHostLayout, WasmArtifact, RUNTIME_IMPORT_NAMESPACE};
use scriptw_front::{parse, ParseOptions};
use wasmparser::{
    CompositeInnerType, ExternalKind, GlobalType, HeapType, Operator, Parser, Payload, StorageType,
    SubType, TypeRef, ValType, Validator, WasmFeatures,
};

#[derive(Default)]
struct ModuleShape {
    groups: Vec<Vec<SubType>>,
    functions: Vec<u32>,
    globals: Vec<GlobalType>,
    runtime_imports: BTreeMap<String, TypeRef>,
    exports: BTreeMap<String, (ExternalKind, u32)>,
    reference_calls: BTreeSet<u32>,
    global_reference_calls: Vec<(u32, u32)>,
    owner_reference_calls: Vec<(u32, u32, u32)>,
}

fn shape(bytes: &[u8]) -> ModuleShape {
    Validator::new_with_features(WasmFeatures::all())
        .validate_all(bytes)
        .expect("the actual R/P module validates");
    let mut shape = ModuleShape::default();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("module decodes") {
            Payload::TypeSection(reader) => {
                for group in reader {
                    shape.groups.push(group.unwrap().types().cloned().collect());
                }
            }
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.unwrap();
                    match import.ty {
                        TypeRef::Func(index) | TypeRef::FuncExact(index) => {
                            shape.functions.push(index)
                        }
                        TypeRef::Global(global) => shape.globals.push(global),
                        _ => {}
                    }
                    if import.module == RUNTIME_IMPORT_NAMESPACE {
                        assert!(shape
                            .runtime_imports
                            .insert(import.name.to_owned(), import.ty)
                            .is_none());
                    }
                }
            }
            Payload::FunctionSection(reader) => shape
                .functions
                .extend(reader.into_iter().map(Result::unwrap)),
            Payload::GlobalSection(reader) => shape
                .globals
                .extend(reader.into_iter().map(|global| global.unwrap().ty)),
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.unwrap();
                    shape
                        .exports
                        .insert(export.name.to_owned(), (export.kind, export.index));
                }
            }
            Payload::CodeSectionEntry(body) => {
                let mut previous = None;
                let mut reference_origin = None;
                let mut local_origins = BTreeMap::new();
                for operator in body.get_operators_reader().unwrap() {
                    let operator = operator.unwrap();
                    if let Operator::CallRef { type_index }
                    | Operator::ReturnCallRef { type_index } = &operator
                    {
                        shape.reference_calls.insert(*type_index);
                        if let Some((owner, field)) = reference_origin {
                            shape
                                .owner_reference_calls
                                .push((owner, field, *type_index));
                        }
                        if let Some(Operator::GlobalGet { global_index }) = previous {
                            shape
                                .global_reference_calls
                                .push((global_index, *type_index));
                        }
                    }
                    // Track only direct typed field/local moves. An unrelated
                    // instruction drops provenance rather than guessing a callee.
                    reference_origin = match &operator {
                        Operator::StructGet {
                            struct_type_index,
                            field_index,
                        } => Some((*struct_type_index, *field_index)),
                        Operator::LocalGet { local_index } => {
                            local_origins.get(local_index).copied()
                        }
                        Operator::LocalSet { local_index } | Operator::LocalTee { local_index } => {
                            if let Some(origin) = reference_origin {
                                local_origins.insert(*local_index, origin);
                            } else {
                                local_origins.remove(local_index);
                            }
                            if matches!(&operator, Operator::LocalTee { .. }) {
                                reference_origin
                            } else {
                                None
                            }
                        }
                        Operator::RefAsNonNull => reference_origin,
                        _ => None,
                    };
                    previous = Some(operator);
                }
            }
            _ => {}
        }
    }
    shape
}

fn emit(source: &str) -> WasmArtifact {
    let parsed = parse(source, ParseOptions::script()).unwrap();
    let program = scriptw_ir::lower(&parsed);
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    scriptw_aot_wasm::emit(&program).expect("source emits through the product compiler")
}

#[test]
fn direct_array_program_helpers_preserve_runtime_bytes_types_imports_and_hook_call_abi() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let plain = emit("var record = { value: 1 }; record.value;");
            let small_arrays =
                emit("function build() { return [['slot',1],['other',2]]; } build();");
            // Each pure nested pair contributes four recipe nodes; 32 pairs plus
            // the outer Begin/End exceed the unchanged 128-node admission floor.
            let pairs = (0..32)
                .map(|index| match index {
                    0 => "['slot',1]".to_owned(),
                    1 => "['other',2]".to_owned(),
                    _ => format!("['slot',{}]", index + 1),
                })
                .collect::<Vec<_>>()
                .join(",");
            let arrays = emit(&format!(
                "function build() {{ return [{pairs}]; }} build();"
            ));
            let runtime = plain.runtime().expect("ordinary object uses linked R");
            let array_runtime = arrays.runtime().expect("array program uses linked R");
            let small_array_runtime = small_arrays.runtime().expect("small array uses linked R");
            assert_eq!(runtime.key(), small_array_runtime.key());
            assert_eq!(runtime.bytes(), small_array_runtime.bytes());
            assert_eq!(runtime.key(), array_runtime.key());
            assert_eq!(
                runtime.bytes(),
                array_runtime.bytes(),
                "using a direct program helper cannot change R bytes"
            );
            let core = shape(runtime.bytes());
            let core_type_count: usize = core.groups.iter().map(Vec::len).sum();
            assert!(!core.groups.is_empty());
            let recursive_groups = core
                .groups
                .iter()
                .enumerate()
                .filter_map(|(index, group)| (group.len() > 1).then_some(index))
                .collect::<Vec<_>>();
            assert_eq!(
                recursive_groups.len(),
                1,
                "R has one canonical GC recursion group"
            );
            let recursive_group = recursive_groups[0];
            assert!(recursive_group > 0, "scalar signatures precede GC types");
            assert!(
                recursive_group + 1 < core.groups.len(),
                "ordinary runtime signatures follow the GC group"
            );
            assert!(
                core.groups[..recursive_group]
                    .iter()
                    .chain(&core.groups[recursive_group + 1..])
                    .all(|group| group.len() == 1
                        && matches!(group[0].composite_type.inner, CompositeInnerType::Func(_))),
                "nonrecursive core signatures are standalone function types"
            );
            let recursive_start: usize = core.groups[..recursive_group].iter().map(Vec::len).sum();
            let recursive_types =
                recursive_start..recursive_start + core.groups[recursive_group].len();
            assert!(core.groups[recursive_group]
                .iter()
                .any(|ty| matches!(ty.composite_type.inner, CompositeInnerType::Struct(_))));
            assert!(core.groups[recursive_group]
                .iter()
                .any(|ty| matches!(ty.composite_type.inner, CompositeInnerType::Array(_))));
            assert!(
                !core.reference_calls.is_empty(),
                "actual R still invokes its typed hooks"
            );
            assert!(core
                .reference_calls
                .iter()
                .all(|index| (*index as usize) < core_type_count));
            let core_types = core.groups.iter().flatten().collect::<Vec<_>>();
            let global_function_references = core
                .globals
                .iter()
                .filter(|global| {
                    let ValType::Ref(reference) = global.content_type else {
                        return false;
                    };
                    let HeapType::Concrete(index) = reference.heap_type() else {
                        return false;
                    };
                    matches!(
                        &core_types[index.as_module_index().unwrap() as usize]
                            .composite_type
                            .inner,
                        CompositeInnerType::Func(_)
                    )
                })
                .count();
            assert_eq!(
                global_function_references, 0,
                "program hooks must not be stored in shared R globals"
            );
            assert!(
                core.global_reference_calls.is_empty(),
                "typed hook calls may not select a shared global owner"
            );
            let owner_field_count = GcHostLayout::ProgramInstance
                .field_count()
                .expect("the current ProgramInstance ABI is a struct")
                as usize;
            let mut owners = Vec::new();
            let mut group_base = 0u32;
            for group in &core.groups {
                for (ordinal, ty) in group.iter().enumerate() {
                    let CompositeInnerType::Struct(owner) = &ty.composite_type.inner else {
                        continue;
                    };
                    if owner.fields.len() != owner_field_count {
                        continue;
                    }
                    let mut hooks = BTreeMap::new();
                    for (field_index, field) in owner.fields.iter().take(5).enumerate() {
                        let StorageType::Val(ValType::Ref(reference)) = field.element_type else {
                            break;
                        };
                        let HeapType::Concrete(index) = reference.heap_type() else {
                            break;
                        };
                        let index = index
                            .as_module_index()
                            .or_else(|| index.as_rec_group_index().map(|index| group_base + index))
                            .expect("encoded core hook uses a module/rec-group index");
                        if !reference.is_nullable()
                            || !matches!(
                                &core_types[index as usize].composite_type.inner,
                                CompositeInnerType::Func(_)
                            )
                        {
                            break;
                        }
                        hooks.insert(field_index as u32, index);
                    }
                    if hooks.len() == 5 {
                        owners.push((group_base + ordinal as u32, hooks));
                    }
                }
                group_base += group.len() as u32;
            }
            assert_eq!(
                owners.len(),
                1,
                "one ProgramInstance owns all five typed nullable hook fields"
            );
            let (owner_type, hook_types) = owners.pop().unwrap();
            assert!(
                recursive_types.contains(&(owner_type as usize))
                    && hook_types
                        .values()
                        .all(|index| recursive_types.contains(&(*index as usize))),
                "the actual ProgramInstance and its five hook signatures remain in the GC group"
            );
            let CompositeInnerType::Struct(owner) =
                &core_types[owner_type as usize].composite_type.inner
            else {
                panic!("the selected ProgramInstance is an actual encoded struct");
            };
            // The two mutable metadata roots now select page directories.
            // Code/data and the original five hook fields remain immutable;
            // selecting an owner by requiring every field immutable loses P.
            let immutable_fields = owner_field_count
                .checked_sub(2)
                .expect("ProgramInstance retains its two metadata roots");
            assert!(immutable_fields >= 5);
            assert!(
                owner.fields[..immutable_fields]
                    .iter()
                    .all(|field| !field.mutable),
                "ProgramInstance hook and code/data ownership remain immutable"
            );
            let metadata_index = |reference: wasmparser::RefType| {
                let HeapType::Concrete(index) = reference.heap_type() else {
                    panic!("metadata cannot retain opaque or abstract callable references");
                };
                index
                    .as_module_index()
                    .or_else(|| {
                        index
                            .as_rec_group_index()
                            .map(|index| recursive_types.start as u32 + index)
                    })
                    .expect("metadata uses the actual module/recursion-group type")
            };
            let mut metadata_types = BTreeSet::new();
            for field in &owner.fields[immutable_fields..] {
                assert!(
                    field.mutable,
                    "only the appended metadata roots are mutable"
                );
                let StorageType::Val(ValType::Ref(reference)) = field.element_type else {
                    panic!("metadata roots remain typed GC references");
                };
                assert!(
                    reference.is_nullable(),
                    "metadata registration starts empty"
                );
                let root = metadata_index(reference);
                let mut index = root;
                for layer in ["directory", "leaf page"] {
                    let CompositeInnerType::Array(array) =
                        &core_types[index as usize].composite_type.inner
                    else {
                        panic!("metadata {layer} must be an indexed GC array");
                    };
                    assert!(array.0.mutable, "metadata {layer} admits lazy publication");
                    let StorageType::Val(ValType::Ref(element)) = array.0.element_type else {
                        panic!("metadata {layer} contains typed references");
                    };
                    assert!(element.is_nullable(), "unused metadata slots begin empty");
                    index = metadata_index(element);
                }
                assert!(
                    matches!(
                        &core_types[index as usize].composite_type.inner,
                        CompositeInnerType::Struct(_)
                    ),
                    "metadata directory and leaf arrays end in records, never callable hooks"
                );
                metadata_types.insert(root);
            }
            assert_eq!(
                metadata_types.len(),
                2,
                "shape and read-site cache metadata retain distinct typed roots"
            );
            // Follow all concrete metadata edges too: a record wrapping a
            // funcref, or an opaque StoredValue/receiver edge, is no substitute
            // for the original boundary against callable hook ownership.
            let mut pending = metadata_types.iter().copied().collect::<Vec<_>>();
            let mut visited = BTreeSet::new();
            while let Some(index) = pending.pop() {
                if !visited.insert(index) {
                    continue;
                }
                assert!(recursive_types.contains(&(index as usize)));
                let fields = match &core_types[index as usize].composite_type.inner {
                    CompositeInnerType::Struct(record) => record.fields.as_ref(),
                    CompositeInnerType::Array(array) => std::slice::from_ref(&array.0),
                    CompositeInnerType::Func(_) | CompositeInnerType::Cont(_) => {
                        panic!("metadata graph cannot own callable hook types")
                    }
                };
                for field in fields {
                    if let StorageType::Val(ValType::Ref(reference)) = field.element_type {
                        pending.push(metadata_index(reference));
                    }
                }
            }
            let mut called_fields = BTreeSet::new();
            for &(owner, field, type_index) in &core.owner_reference_calls {
                if owner != owner_type {
                    continue;
                }
                assert_eq!(
                    hook_types.get(&field),
                    Some(&type_index),
                    "the actual ProgramInstance field and call_ref retain the same hook ABI"
                );
                called_fields.insert(field);
            }
            assert_eq!(
                called_fields,
                BTreeSet::from([0, 1, 2, 3, 4]),
                "R actually calls each typed hook through its selected P owner"
            );
            assert!(
                module_bodies(runtime.bytes())
                    .iter()
                    .all(|body| body.name != "helper::compact_array_literal"),
                "the direct array helper body belongs only to P"
            );

            for artifact in [&plain, &small_arrays, &arrays] {
                let program = shape(&artifact.bytes);
                assert_eq!(
                    &program.groups[..core.groups.len()],
                    &core.groups,
                    "core canonical recursion groups must match before import linking"
                );
                assert!(program.groups.len() > core.groups.len());
                assert!(
                    program.groups[core.groups.len()..]
                        .iter()
                        .all(|group| group.len() == 1),
                    "program signatures are independent suffix types"
                );
                assert!(!program.runtime_imports.is_empty());
                assert!(
                    program.global_reference_calls.is_empty(),
                    "P must not recover hook authority from shared globals"
                );
                for (name, import) in &program.runtime_imports {
                    let &(kind, index) = core
                        .exports
                        .get(name)
                        .expect("P imports an actual R export");
                    match import {
                        TypeRef::Func(ty) | TypeRef::FuncExact(ty) => {
                            assert_eq!(kind, ExternalKind::Func);
                            assert_eq!(*ty, core.functions[index as usize], "{name} function ABI");
                            assert!((*ty as usize) < core_type_count);
                        }
                        TypeRef::Global(global) => {
                            assert_eq!(kind, ExternalKind::Global);
                            assert_eq!(*global, core.globals[index as usize], "{name} global ABI");
                        }
                        _ => panic!("unexpected runtime import {name}"),
                    }
                }
                let array_helper = module_bodies(&artifact.bytes)
                    .into_iter()
                    .find(|body| body.name == "helper::compact_array_literal")
                    .expect("P keeps its dense direct array helper slot");
                assert!(
                    (program.functions[array_helper.index as usize] as usize) >= core_type_count,
                    "the actual array helper declaration consumes a suffix signature"
                );
                assert!(
                    program
                        .reference_calls
                        .iter()
                        .all(|index| (*index as usize) < core_type_count),
                    "program call_ref roles and hooks consume stable core signatures"
                );
            }
            let plain_body = module_bodies(&plain.bytes)
                .into_iter()
                .find(|body| body.name == "helper::compact_array_literal")
                .unwrap()
                .bytes();
            let small_array_body = module_bodies(&small_arrays.bytes)
                .into_iter()
                .find(|body| body.name == "helper::compact_array_literal")
                .unwrap()
                .bytes();
            assert_eq!(
                small_array_body, plain_body,
                "the original two-pair input stays below recipe admission"
            );
            let array_body = module_bodies(&arrays.bytes)
                .into_iter()
                .find(|body| body.name == "helper::compact_array_literal")
                .unwrap()
                .bytes();
            assert_eq!(
                plain_body, 3,
                "without recipes the helper contains zero locals, unreachable and end"
            );
            assert!(
                array_body > plain_body,
                "the paired array program uses the real recipe constructor"
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
