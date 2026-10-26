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
    let mut expected = vec![stored_index, deferred_index];
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
