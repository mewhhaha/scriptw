use super::*;
use crate::runtime_artifact::ModuleKind;
use wasmparser::{CompositeInnerType, Parser, Payload, Validator, WasmFeatures};

fn declared_fields(
    kind: ModuleKind<'_>,
    layout: GcLayout,
    targets: &[GcLayout],
) -> (Vec<wasmparser::FieldType>, Vec<u32>) {
    let runtime =
        RuntimeModuleTypes::register(kind).finalize_globals(GlobalLedger::new(), false, 0);
    let layouts = runtime.globals().schema().layouts();
    let target = layouts.layout_index(layout) as usize;
    let indexes = targets
        .iter()
        .map(|layout| layouts.layout_index(*layout))
        .collect();
    let mut module = wasm_encoder::Module::new();
    module.section(runtime.types());
    module.section(&runtime.globals().defined_section(0));
    let bytes = module.finish();
    Validator::new_with_features(WasmFeatures::all())
        .validate_all(&bytes)
        .unwrap();
    let mut types = Vec::new();
    for payload in Parser::new(0).parse_all(&bytes) {
        if let Payload::TypeSection(groups) = payload.unwrap() {
            for group in groups {
                types.extend(group.unwrap().into_types());
            }
        }
    }
    let CompositeInnerType::Struct(record) = &types[target].composite_type.inner else {
        panic!("the source brand and cell owner must be real GC structs")
    };
    (record.fields.to_vec(), indexes)
}

fn assert_strong_immutable_reference(field: &wasmparser::FieldType, target: u32) {
    assert!(!field.mutable);
    let wasmparser::StorageType::Val(wasmparser::ValType::Ref(reference)) = field.element_type
    else {
        panic!("a source owner must remain a concrete GC reference")
    };
    assert!(!reference.is_nullable());
    let wasmparser::HeapType::Concrete(index) = reference.heap_type() else {
        panic!("a source owner cannot be an opaque anyref")
    };
    assert_eq!(index.as_module_index(), Some(target));
}

#[test]
fn module_source_gc_brand_keeps_actual_module_realm_and_string_references() {
    let targets = [
        GcLayout::OrdinaryObject,
        GcLayout::ModuleRecord,
        GcLayout::RealmRecord,
        GcLayout::StringValue,
    ];
    let (runtime, indexes) = declared_fields(
        ModuleKind::Runtime(crate::RuntimeSourceCompilationProfile::Portable),
        GcLayout::ModuleSourceObject,
        &targets,
    );
    let (program, program_indexes) = declared_fields(
        crate::tests::standalone_kind(),
        GcLayout::ModuleSourceObject,
        &targets,
    );
    assert_eq!(runtime, program, "R and P must share the exact brand type");
    assert_eq!(indexes, program_indexes);
    assert_eq!(runtime.len(), 4);
    for (field, target) in runtime.iter().zip(indexes) {
        assert_strong_immutable_reference(field, target);
    }
    assert_ne!(
        GcLayout::ModuleSourceObject,
        GcLayout::ModuleNamespaceObject
    );
    assert_eq!(
        GcLayout::ModuleSourceObject.javascript_tag(),
        Some(crate::WasmRuntimeValueTag::Object)
    );
}

#[test]
fn canonical_module_source_cell_is_a_distinct_strong_binding_cell_owner() {
    let (record, indexes) = declared_fields(
        ModuleKind::Runtime(crate::RuntimeSourceCompilationProfile::Portable),
        GcLayout::ModuleRecord,
        &[GcLayout::BindingCell],
    );
    let source = ModuleRecordSchema::SOURCE_CELL.ordinal().raw() as usize;
    assert_ne!(
        source,
        ModuleRecordSchema::NAMESPACE_CELL.ordinal().raw() as usize
    );
    assert_ne!(
        source,
        ModuleRecordSchema::DEFERRED_NAMESPACE_CELL.ordinal().raw() as usize
    );
    assert_strong_immutable_reference(&record[source], indexes[0]);
}
