use super::*;

#[test]
fn for_in_cursor_keeps_the_same_strong_set_edge_in_runtime_and_program_types() {
    let (runtime, runtime_bytes) = encoded(ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ));
    let (_, program_bytes) = encoded(crate::tests::standalone_kind());
    let (runtime_types, _, _) = declarations(&runtime_bytes);
    let (program_types, _, _) = declarations(&program_bytes);
    let schema = runtime.globals().schema();
    let cursor = schema
        .layouts()
        .layout_index(GcLayout::ForInEnumerationRecord) as usize;
    let set = schema.layouts().layout_index(GcLayout::SetObject);
    assert_eq!(runtime_types[cursor], program_types[cursor]);
    assert_eq!(runtime_types[set as usize], program_types[set as usize]);
    let CompositeInnerType::Struct(record) = &runtime_types[cursor].composite_type.inner else {
        panic!("ForIn retains its actual four-field GC record");
    };
    assert_eq!(record.fields.len(), 4);
    let ordinal = ForInEnumerationRecordSchema::VISITED_SET.ordinal().raw() as usize;
    assert_eq!(ordinal, 3);
    let field = record.fields[ordinal];
    assert!(!field.mutable, "the cursor cannot replace its Set identity");
    assert_eq!(
        field.element_type,
        wasmparser::StorageType::Val(wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                false,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(set)),
            )
            .unwrap(),
        )),
        "the root names canonical SetObject, without a second storage layout",
    );
    assert_eq!(
        &runtime_types[..],
        &program_types[..runtime_types.len()],
        "the complete recursive type prefix agrees, including the forward Set edge",
    );
}
