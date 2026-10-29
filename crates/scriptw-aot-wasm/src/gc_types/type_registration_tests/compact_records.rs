use super::*;
use crate::gc_types::host::GcHostField;
use crate::heap::StoredPropertyAttributes;

#[test]
fn compact_records_preserve_the_emitted_host_schema_and_scalar_widths() {
    let (runtime, bytes) = encoded(ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ));
    let (types, _, _) = declarations(&bytes);
    let schema = runtime.globals().schema();
    let value_index = schema.struct_type::<PropertyValue>().type_index().raw();
    let stored_index = schema.struct_type::<StoredValue>().type_index().raw();
    let deferred_index = schema
        .struct_type::<DeferredIntrinsicValue>()
        .type_index()
        .raw();
    let temporal_index = schema
        .struct_type::<DeferredTemporalNamespace>()
        .type_index()
        .raw();
    let intl_index = schema
        .struct_type::<DeferredIntlNamespace>()
        .type_index()
        .raw();
    let realm_index = schema.struct_type::<RealmRecord>().type_index().raw();
    let reference = |index, nullable| {
        wasmparser::StorageType::Val(wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                nullable,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(index)),
            )
            .unwrap(),
        ))
    };
    let base = &types[value_index as usize];
    assert!(!base.is_final);
    assert!(base.supertype_idx.is_none());
    let CompositeInnerType::Struct(base_fields) = &base.composite_type.inner else {
        panic!("PropertyValue must be the fieldless sum base");
    };
    assert!(base_fields.fields.is_empty());
    let mut alternatives = types
        .iter()
        .enumerate()
        .filter(|(_, ty)| {
            ty.supertype_idx.map(|parent| parent.unpack())
                == Some(wasmparser::UnpackedIndex::Module(value_index))
        })
        .map(|(index, ty)| {
            assert!(ty.is_final, "property value alternatives must be sealed");
            index as u32
        })
        .collect::<Vec<_>>();
    alternatives.sort_unstable();
    let mut expected = vec![stored_index, deferred_index, temporal_index, intl_index];
    expected.sort_unstable();
    assert_eq!(alternatives, expected);
    let CompositeInnerType::Struct(stored) = &types[stored_index as usize].composite_type.inner
    else {
        panic!("StoredValue must be the actual registered struct");
    };
    // The pinned copying collector has a 16-byte header/alignment and 4-byte
    // references. These declarations fit in 32 bytes without discarding bits.
    assert_eq!(
        stored
            .fields
            .iter()
            .map(|f| f.element_type)
            .collect::<Vec<_>>(),
        vec![
            wasmparser::StorageType::Val(wasmparser::ValType::I64),
            wasmparser::StorageType::Val(wasmparser::ValType::I32),
            wasmparser::StorageType::Val(wasmparser::ValType::Ref(wasmparser::RefType::EQREF)),
        ]
    );
    assert!(stored.fields.iter().all(|field| !field.mutable));
    assert_eq!(GcHostField::StoredScalar.definition().index(), 0);
    assert_eq!(GcHostField::StoredTag.definition().index(), 1);
    assert_eq!(GcHostField::StoredReference.definition().index(), 2);

    let CompositeInnerType::Struct(deferred) = &types[deferred_index as usize].composite_type.inner
    else {
        panic!("DeferredIntrinsicValue must be a registered struct");
    };
    assert_eq!(
        deferred
            .fields
            .iter()
            .map(|field| (field.element_type, field.mutable))
            .collect::<Vec<_>>(),
        vec![
            (reference(realm_index, false), false),
            (
                wasmparser::StorageType::Val(wasmparser::ValType::I32),
                false
            ),
            (reference(stored_index, true), true),
        ]
    );

    for namespace_index in [temporal_index, intl_index] {
        let CompositeInnerType::Struct(namespace) =
            &types[namespace_index as usize].composite_type.inner
        else {
            panic!("deferred namespace must be a registered struct");
        };
        assert_eq!(
            namespace
                .fields
                .iter()
                .map(|field| (field.element_type, field.mutable))
                .collect::<Vec<_>>(),
            vec![(reference(realm_index, false), false)],
            "the recipe retains only its defining Realm, which owns canonical identity"
        );
    }
    let CompositeInnerType::Struct(realm) = &types[realm_index as usize].composite_type.inner
    else {
        panic!("RealmRecord must be a registered struct");
    };
    for field in [
        RealmRecordSchema::TEMPORAL_STATE.ordinal(),
        RealmRecordSchema::INTL_STATE.ordinal(),
    ] {
        let state = &realm.fields[field.raw() as usize];
        assert!(state.mutable);
        assert_eq!(
            state.element_type,
            wasmparser::StorageType::Val(wasmparser::ValType::I32)
        );
    }
    for field in [
        RealmRecordSchema::TEMPORAL_NAMESPACE.ordinal(),
        RealmRecordSchema::INTL_NAMESPACE.ordinal(),
    ] {
        let namespace = &realm.fields[field.raw() as usize];
        assert!(namespace.mutable);
        assert_eq!(namespace.element_type, reference(stored_index, true));
    }

    let descriptor_index = schema
        .struct_type::<PropertyDescriptor>()
        .type_index()
        .raw();
    let CompositeInnerType::Struct(descriptor) =
        &types[descriptor_index as usize].composite_type.inner
    else {
        panic!("PropertyDescriptor must be the actual registered struct");
    };
    assert_eq!(descriptor.fields.len(), 4);
    assert!(descriptor.fields.iter().all(|field| field.mutable));
    assert_eq!(
        descriptor.fields[0].element_type,
        wasmparser::StorageType::Val(wasmparser::ValType::I32)
    );
    assert_eq!(
        descriptor.fields[1].element_type,
        reference(value_index, false)
    );
    for field in &descriptor.fields[2..] {
        assert_eq!(field.element_type, reference(stored_index, false));
    }
}

#[test]
fn compact_descriptor_word_keeps_every_kind_and_attribute_without_unknown_bits() {
    for enumerable in [false, true] {
        for configurable in [false, true] {
            for writable in [false, true] {
                let word = StoredPropertyAttributes::Data {
                    writable,
                    enumerable,
                    configurable,
                }
                .descriptor_word();
                assert_eq!(
                    snapshot_descriptor_flags(word.bits()),
                    Some((false, writable, enumerable, configurable))
                );
            }
            let word = StoredPropertyAttributes::Accessor {
                enumerable,
                configurable,
            }
            .descriptor_word();
            assert_eq!(
                snapshot_descriptor_flags(word.bits()),
                Some((true, false, enumerable, configurable))
            );
        }
    }
    for invalid in [16, 1 << 31, u32::MAX] {
        assert_eq!(snapshot_descriptor_flags(invalid), None);
    }
}

#[test]
fn prototype_edges_accept_only_the_sealed_actual_object_payload_family() {
    let (runtime, bytes) = encoded(ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ));
    let (types, _, _) = declarations(&bytes);
    let schema = runtime.globals().schema();
    let base = schema.struct_type::<JavaScriptObject>().type_index().raw();
    let declaration = &types[base as usize];
    assert!(!declaration.is_final);
    assert!(declaration.supertype_idx.is_none());
    let CompositeInnerType::Struct(fields) = &declaration.composite_type.inner else {
        panic!("the actual object family has a fieldless struct base");
    };
    assert!(fields.fields.is_empty());
    let mut object_count = 0;
    for layout in GcLayout::ALL {
        let ty = &types[schema.layouts().layout_index(*layout) as usize];
        let object = layout.object_projection().is_some();
        let child = ty.supertype_idx.map(|index| index.unpack())
            == Some(wasmparser::UnpackedIndex::Module(base));
        assert_eq!(child, object, "{layout:?} must match its object projection");
        if object {
            object_count += 1;
            assert!(ty.is_final, "actual payload identity must stay sealed");
        }
    }
    assert!(object_count > 0);
    let index = schema.struct_type::<OrdinaryObject>().type_index().raw();
    let CompositeInnerType::Struct(object) = &types[index as usize].composite_type.inner else {
        panic!("ordinary object must retain its own header");
    };
    assert_eq!(
        object.fields[0].element_type,
        wasmparser::StorageType::Val(wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                true,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(base)),
            )
            .unwrap()
        ))
    );
    assert!(object.fields[0].mutable);
    let compact_index = schema
        .struct_type::<ArrayCompactDataStorage>()
        .type_index()
        .raw();
    let CompositeInnerType::Struct(compact) = &types[compact_index as usize].composite_type.inner
    else {
        panic!("array compact storage retains its backing fields");
    };
    let scalar_index = schema.array_type::<CompactScalarArray>().type_index().raw();
    assert_eq!(
        compact.fields[2].element_type,
        wasmparser::StorageType::Val(wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                true,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(scalar_index))
            )
            .unwrap()
        ))
    );
    assert!(
        compact.fields[2].mutable,
        "first scalar write publishes its backing"
    );
    let references_index = schema
        .array_type::<CompactReferenceArray>()
        .type_index()
        .raw();
    assert_eq!(
        compact.fields[3].element_type,
        wasmparser::StorageType::Val(wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                true,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(references_index))
            )
            .unwrap()
        ))
    );
    assert!(
        compact.fields[3].mutable,
        "first nonnull reference write publishes its backing"
    );
    let CompositeInnerType::Array(references) =
        &types[references_index as usize].composite_type.inner
    else {
        panic!("compact references must retain their exact registered array type");
    };
    assert_eq!(
        references.0.element_type,
        wasmparser::StorageType::Val(wasmparser::ValType::Ref(wasmparser::RefType::EQREF))
    );
    assert!(
        references.0.mutable,
        "null overwrites must retire strong edges"
    );
}

#[test]
fn compact_storage_has_a_shape_only_base_and_two_sealed_physical_variants() {
    let (runtime, bytes) = encoded(ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ));
    let (types, _, _) = declarations(&bytes);
    let schema = runtime.globals().schema();
    let index = |layout| schema.layouts().layout_index(layout);
    let fields = |layout| match &types[index(layout) as usize].composite_type.inner {
        CompositeInnerType::Struct(record) => record.fields.as_ref(),
        _ => panic!("compact storage is a GC struct"),
    };
    let reference = |layout, nullable| {
        wasmparser::StorageType::Val(wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                nullable,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(index(layout))),
            )
            .unwrap(),
        ))
    };
    let base = index(GcLayout::CompactDataStorage);
    assert!(!types[base as usize].is_final);
    let base_fields = fields(GcLayout::CompactDataStorage);
    assert_eq!(base_fields.len(), 1);
    assert_eq!(
        base_fields[0].element_type,
        reference(GcLayout::CompactDataShape, false)
    );
    assert!(!base_fields[0].mutable);
    let mut alternatives = types
        .iter()
        .enumerate()
        .filter_map(|(ordinal, ty)| {
            (ty.supertype_idx.map(|parent| parent.unpack())
                == Some(wasmparser::UnpackedIndex::Module(base)))
            .then(|| {
                assert!(ty.is_final, "physical storage alternatives are sealed");
                ordinal as u32
            })
        })
        .collect::<Vec<_>>();
    alternatives.sort_unstable();
    let mut expected = vec![
        index(GcLayout::ArrayCompactDataStorage),
        index(GcLayout::InlineCompactDataStorage),
    ];
    expected.sort_unstable();
    assert_eq!(alternatives, expected);
    for layout in [
        GcLayout::ArrayCompactDataStorage,
        GcLayout::InlineCompactDataStorage,
    ] {
        assert_eq!(
            fields(layout)[0],
            base_fields[0],
            "{layout:?} repeats the inherited SHAPE prefix"
        );
    }
    let array = fields(GcLayout::ArrayCompactDataStorage);
    assert_eq!(array.len(), 4);
    assert_eq!(
        array[1].element_type,
        reference(GcLayout::CompactTagArray, false)
    );
    assert!(!array[1].mutable);
    assert_eq!(
        array[2].element_type,
        reference(GcLayout::CompactScalarArray, true)
    );
    assert_eq!(
        array[3].element_type,
        reference(GcLayout::CompactReferenceArray, true)
    );
    assert!(array[2..].iter().all(|field| field.mutable));
    let inline = fields(GcLayout::InlineCompactDataStorage);
    assert_eq!(inline.len(), 6);
    assert_eq!(
        inline[1].element_type,
        wasmparser::StorageType::Val(wasmparser::ValType::I32)
    );
    for field in &inline[2..4] {
        assert_eq!(
            field.element_type,
            wasmparser::StorageType::Val(wasmparser::ValType::I64)
        );
    }
    for field in &inline[4..] {
        assert_eq!(
            field.element_type,
            wasmparser::StorageType::Val(wasmparser::ValType::Ref(wasmparser::RefType::EQREF))
        );
    }
    assert!(inline[1..].iter().all(|field| field.mutable));
    assert_eq!(CompactDataStorageSchema::SHAPE.ordinal().raw(), 0);
    assert_eq!(ArrayCompactDataStorageSchema::SHAPE.ordinal().raw(), 0);
    assert_eq!(ArrayCompactDataStorageSchema::TAGS.ordinal().raw(), 1);
    assert_eq!(ArrayCompactDataStorageSchema::SCALARS.ordinal().raw(), 2);
    assert_eq!(ArrayCompactDataStorageSchema::REFERENCES.ordinal().raw(), 3);
    assert_eq!(InlineCompactDataStorageSchema::SHAPE.ordinal().raw(), 0);
    assert_eq!(
        InlineCompactDataStorageSchema::PACKED_TAGS.ordinal().raw(),
        1
    );
    assert_eq!(InlineCompactDataStorageSchema::SCALAR0.ordinal().raw(), 2);
    assert_eq!(InlineCompactDataStorageSchema::SCALAR1.ordinal().raw(), 3);
    assert_eq!(
        InlineCompactDataStorageSchema::REFERENCE0.ordinal().raw(),
        4
    );
    assert_eq!(
        InlineCompactDataStorageSchema::REFERENCE1.ordinal().raw(),
        5
    );
}
