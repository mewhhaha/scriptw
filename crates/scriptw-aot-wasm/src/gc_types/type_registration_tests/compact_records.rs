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
    let stored_index = schema.struct_type::<StoredValue>().type_index().raw();
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
    for field in &descriptor.fields[1..] {
        assert_eq!(
            field.element_type,
            wasmparser::StorageType::Val(wasmparser::ValType::Ref(
                wasmparser::RefType::new(
                    false,
                    wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(stored_index))
                )
                .unwrap()
            ))
        );
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
