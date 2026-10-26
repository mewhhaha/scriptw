//! Integration-test projections of the public emitted host-schema authority.
use scriptw_aot_wasm::{GcHostField, GcHostLayout, GcHostStorage};
use wasmparser::{FieldType, RefType, StorageType, StructType, ValType};

// A new StoredValue field must update this complete named projection rather
// than silently weakening the test's notion of a real stored JavaScript value.
const STORED_FIELDS: [GcHostField;
    GcHostLayout::StoredValue
        .field_count()
        .expect("StoredValue is a struct") as usize] = [
    GcHostField::StoredTag,
    GcHostField::StoredScalar,
    GcHostField::StoredReference,
];

fn stored_field_storage(storage: GcHostStorage) -> StorageType {
    match storage {
        GcHostStorage::I32 => StorageType::Val(ValType::I32),
        GcHostStorage::I64 => StorageType::Val(ValType::I64),
        GcHostStorage::NullableEqRef => StorageType::Val(ValType::Ref(RefType::EQREF)),
        GcHostStorage::PackedU8
        | GcHostStorage::PackedU16
        | GcHostStorage::FunctionReference { .. }
        | GcHostStorage::Reference { .. } => {
            panic!("unexpected StoredValue host storage: {storage:?}")
        }
    }
}

pub(super) fn is_stored_value(actual: &StructType) -> bool {
    actual.fields.len() == STORED_FIELDS.len()
        && STORED_FIELDS.iter().all(|field| {
            let expected = field.definition();
            actual
                .fields
                .get(expected.index() as usize)
                .is_some_and(|actual| {
                    actual.mutable == expected.mutable()
                        && actual.element_type == stored_field_storage(expected.storage())
                })
        })
}

#[test]
fn stored_value_shape_uses_host_authority_and_rejects_malformed_controls() {
    let mut declarations = STORED_FIELDS.map(|field| {
        let definition = field.definition();
        (
            definition.index(),
            FieldType {
                element_type: stored_field_storage(definition.storage()),
                mutable: definition.mutable(),
            },
        )
    });
    declarations.sort_by_key(|(index, _)| *index);
    for (ordinal, (actual, _)) in declarations.iter().enumerate() {
        assert_eq!(
            *actual as usize, ordinal,
            "complete, disjoint host field ordinals"
        );
    }
    let valid = StructType {
        fields: declarations
            .into_iter()
            .map(|(_, field)| field)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    };
    assert!(is_stored_value(&valid));

    let tag = GcHostField::StoredTag.definition().index() as usize;
    let scalar = GcHostField::StoredScalar.definition().index() as usize;
    let reference = GcHostField::StoredReference.definition().index() as usize;
    let mut old_order = valid.clone();
    old_order.fields.swap(tag, scalar);
    assert!(
        !is_stored_value(&old_order),
        "tag/scalar transposition is not the current schema"
    );
    for field in STORED_FIELDS {
        let index = field.definition().index() as usize;
        let mut wrong_mutability = valid.clone();
        wrong_mutability.fields[index].mutable = !field.definition().mutable();
        assert!(
            !is_stored_value(&wrong_mutability),
            "{field:?} mutability is part of its schema"
        );
        let mut wrong_storage = valid.clone();
        wrong_storage.fields[index].element_type = StorageType::Val(ValType::F64);
        assert!(
            !is_stored_value(&wrong_storage),
            "{field:?} storage is part of its schema"
        );
    }
    for wrong_reference in [RefType::EQREF.as_non_null(), RefType::ANYREF] {
        let mut wrong = valid.clone();
        wrong.fields[reference].element_type = StorageType::Val(ValType::Ref(wrong_reference));
        assert!(
            !is_stored_value(&wrong),
            "reference heap type and nullability must both match"
        );
    }
    let mut missing = valid.fields.to_vec();
    missing.pop();
    assert!(!is_stored_value(&StructType {
        fields: missing.into_boxed_slice()
    }));
    let mut extra = valid.fields.to_vec();
    extra.push(valid.fields[tag]);
    assert!(!is_stored_value(&StructType {
        fields: extra.into_boxed_slice()
    }));
}
