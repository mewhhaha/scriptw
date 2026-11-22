//! The Realm protector record: Realm initialization is its only constructor,
//! and the property-read entry and packed push consult it in front of, not
//! instead of, their complete proofs.
use super::*;
use crate::gc_types::{
    ArrayObject, GcHeapType, GcNullability, GlobalLedger, IntrinsicPropertyStorage, OrdinaryObject,
    RealmProtectors, RealmRecord, RuntimeModuleTypes, RuntimeSchema,
};
use wasmparser::{CompositeInnerType, StorageType, SubType, ValType};

fn type_index<T: GcHeapType>(schema: &RuntimeSchema) -> u32 {
    let wasm_encoder::HeapType::Concrete(index) = schema
        .reference_type::<T>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("registered GC type must have a concrete index");
    };
    index
}

/// The declared struct fields of type `index`.
fn struct_fields(types: &[SubType], index: u32) -> Vec<StorageType> {
    let CompositeInnerType::Struct(record) = &types[index as usize].composite_type.inner else {
        panic!("type {index} must be a GC struct");
    };
    record
        .fields
        .iter()
        .map(|field| field.element_type)
        .collect()
}

/// The one field of `fields` that refers to type `target`.
fn field_to(fields: &[StorageType], target: u32) -> u32 {
    let matches: Vec<_> = fields
        .iter()
        .enumerate()
        .filter(|(_, field)| {
            matches!(field, StorageType::Val(ValType::Ref(reference))
                if reference.heap_type()
                    == wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(target)))
        })
        .map(|(index, _)| index as u32)
        .collect();
    assert_eq!(matches.len(), 1, "one field refers to type {target}");
    matches[0]
}

#[test]
fn realm_initialization_alone_arms_the_protectors_that_reads_and_pushes_consult_first() {
    let artifact = emit_script(
        "var o = { a: 1 }; o.b; o.hasOwnProperty('a'); \
         var a = []; a.push(1); a.push(2, 3); ''.slice(0); (function () {}).call(null);",
    )
    .unwrap();
    expect_valid_module(&artifact, 1);
    let bytes = artifact.runtime().unwrap().bytes();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let mut types = Vec::new();
    let mut bodies = std::collections::BTreeMap::new();
    let mut index = imported_function_count_in_bytes(bytes);
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            Payload::TypeSection(groups) => {
                for group in groups {
                    types.extend(group.unwrap().types().cloned());
                }
            }
            Payload::CodeSectionEntry(body) => {
                bodies.insert(index, crate::tests::emitted_operators(&body));
                index += 1;
            }
            _ => {}
        }
    }
    let realm = type_index::<RealmRecord>(schema);
    let protectors = type_index::<RealmProtectors>(schema);
    let intrinsic = type_index::<IntrinsicPropertyStorage>(schema);
    let protectors_field = field_to(&struct_fields(&types, realm), protectors);
    let record = struct_fields(&types, protectors);
    let object_prototype = field_to(&record, type_index::<OrdinaryObject>(schema));
    let array_prototype = field_to(&record, type_index::<ArrayObject>(schema));
    // Each identity is followed by the rows its header held at arming.
    let object_rows = object_prototype + 1;
    assert!(
        matches!(record[object_rows as usize], StorageType::Val(ValType::Ref(rows))
        if rows.is_nullable() && rows.heap_type()
            == wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(intrinsic)))
    );

    let names = function_names_in_bytes(bytes);
    let named = |requested: &str| {
        let matches: Vec<_> = names
            .iter()
            .filter_map(|(&index, name)| (name == requested).then_some(index))
            .collect();
        assert_eq!(matches.len(), 1, "one physical R body for {requested}");
        matches[0]
    };
    let position = |operators: &[Operator<'_>], wanted: &dyn Fn(&Operator<'_>) -> bool| {
        operators.iter().position(wanted)
    };
    let reads = |owner: u32, field: u32| {
        move |op: &Operator<'_>| {
            matches!(op, Operator::StructGet { struct_type_index, field_index }
                if *struct_type_index == owner && *field_index == field)
        }
    };

    // Realm initialization is the record's only constructor and writer.
    let initialize = named("helper::realm_initialize_intrinsics");
    for (&function, operators) in &bodies {
        let constructions = operators
            .iter()
            .filter(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == protectors))
            .count();
        let writes = operators
            .iter()
            .filter(|op| {
                matches!(op, Operator::StructSet { struct_type_index, field_index }
                if *struct_type_index == realm && *field_index == protectors_field)
            })
            .count();
        let expected = usize::from(function == initialize);
        assert_eq!(
            constructions, expected,
            "RealmProtectors constructed only at arming"
        );
        assert_eq!(writes, expected, "PROTECTORS written only at arming");
    }

    // The read entry asks the protector first and keeps the complete
    // intrinsic-layout proof for every other prototype.
    let read = &bodies[&named("helper::cached_property_read")];
    let protected = position(read, &reads(realm, protectors_field))
        .expect("the read entry consults the current Realm's protectors");
    assert!(read.iter().any(reads(protectors, object_rows)));
    assert!(
        read[protected..].iter().any(|op| matches!(op,
            Operator::BrOnCast { to_ref_type, .. }
                if to_ref_type.heap_type()
                    == wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(intrinsic)))),
        "the complete intrinsic-layout proof remains after the protected path"
    );

    // Packed push asks the Array.prototype protector before the walk, which
    // still tests actual intrinsic storage.
    for helper in ["helper::array_packed_push1", "helper::array_packed_push2"] {
        let push = &bodies[&named(helper)];
        let protected = position(push, &reads(protectors, array_prototype))
            .unwrap_or_else(|| panic!("{helper} consults the Array.prototype protector"));
        let walk = position(push, &|op| {
            matches!(op, Operator::RefTestNonNull { hty }
                if *hty == wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(intrinsic)))
        })
        .unwrap_or_else(|| panic!("{helper} keeps the complete prototype walk"));
        assert!(protected < walk, "{helper} asks the protector first");
    }
}
