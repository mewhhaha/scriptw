//! The RegExp constructor recipe stays inert until its defining Realm is read.
use super::error_demand_initialization::{body, encode_realm_probe, named, type_index};
use super::*;
use crate::gc_types::{
    DeferredRegExpConstructor, FinalizedRuntimeModule, GcNullability, IntrinsicTable, NonNullable,
    RealmRecord, RealmRecordSchema,
};
use crate::heap::RegExpInitializationState;
use scriptw_ir::StandardBuiltinId;

fn call_count(operators: &[Operator<'_>], target: u32) -> usize {
    operators
        .iter()
        .filter(|operator| {
            matches!(operator, Operator::Call { function_index } if *function_index == target)
        })
        .count()
}

fn ref_func_count(operators: &[Operator<'_>], target: u32) -> usize {
    operators
        .iter()
        .filter(|operator| {
            matches!(operator, Operator::RefFunc { function_index } if *function_index == target)
        })
        .count()
}

fn struct_new_count(operators: &[Operator<'_>], target: u32) -> usize {
    operators
        .iter()
        .filter(|operator| {
            matches!(operator, Operator::StructNew { struct_type_index } if *struct_type_index == target)
        })
        .count()
}

fn regexp_state_field(runtime: &FinalizedRuntimeModule) -> u32 {
    let schema = runtime.globals().schema();
    let mut function = crate::code_sink::Function::with_parameters(
        crate::code_sink::LocalDeclarations::from_types([]),
        vec![wasm_encoder::ValType::Ref(
            schema.reference_type::<RealmRecord>(GcNullability::NonNullable),
        )],
    );
    let realm = schema.parameter_gc::<RealmRecord, NonNullable>(0, &mut function);
    let state = schema.reserve_i32_local(&mut function);
    schema
        .field(RealmRecordSchema::REGEXP_STATE)
        .read(&realm, schema, &mut function)
        .store(state, &mut function);
    realm.clear(&mut function);
    schema.release_i32_local(state, &mut function);
    function.instruction(&wasm_encoder::Instruction::End);
    let bytes = encode_realm_probe(runtime, function);
    let reads: Vec<_> = body(&bytes, 0)
        .into_iter()
        .filter_map(|operator| match operator {
            Operator::StructGet {
                struct_type_index,
                field_index,
            } if struct_type_index == type_index::<RealmRecord>(schema) => Some(field_index),
            _ => None,
        })
        .collect();
    assert_eq!(
        reads.len(),
        1,
        "the typed REGEXP_STATE read owns its ordinal"
    );
    reads[0]
}

#[test]
fn canonical_runtime_defers_regexp_and_builds_its_family_in_the_realm_owner() {
    let artifact = emit_script("this;").unwrap();
    expect_valid_module(&artifact, 0);
    let bytes = artifact.runtime().unwrap().bytes();
    let names = function_names_in_bytes(bytes);
    let initialize = body(bytes, named(&names, "helper::realm_initialize_intrinsics"));
    let ensure_index = named(&names, "helper::realm_ensure_regexp");
    let ensure = body(bytes, ensure_index);
    let resolve = body(bytes, named(&names, "helper::property_value_resolve"));
    let constructor_name = format!(
        "builtin::{}",
        StandardBuiltinId::RegExpConstructor.debug_name()
    );
    let constructor = named(&names, &constructor_name);

    assert_eq!(call_count(&initialize, ensure_index), 0);
    assert_eq!(call_count(&ensure, ensure_index), 0);
    assert_eq!(call_count(&resolve, ensure_index), 1);
    assert_eq!(ref_func_count(&initialize, constructor), 0);
    assert_eq!(ref_func_count(&ensure, constructor), 1);
    let regexp_builtins: Vec<_> = names
        .iter()
        .filter_map(|(&index, name)| name.starts_with("builtin::RegExp").then_some(index))
        .collect();
    assert!(!regexp_builtins.is_empty());
    assert!(regexp_builtins
        .iter()
        .all(|&index| ref_func_count(&initialize, index) == 0));

    let registered = crate::gc_types::RuntimeModuleTypes::register(
        crate::runtime_artifact::ModuleKind::Runtime(
            crate::RuntimeSourceCompilationProfile::Portable,
        ),
    )
    .finalize_globals(crate::gc_types::GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let recipe_type = type_index::<DeferredRegExpConstructor>(schema);
    assert_eq!(struct_new_count(&initialize, recipe_type), 0);
    for publisher in [
        format!("host::{}", scriptw_ir::HostBuiltinId::CreateRealm.as_str()),
        format!(
            "builtin::{}",
            StandardBuiltinId::ShadowRealmConstructor.debug_name()
        ),
    ] {
        let publication = body(bytes, named(&names, &publisher));
        assert_eq!(
            struct_new_count(&publication, recipe_type),
            1,
            "{publisher} installs one defining-Realm recipe",
        );
        assert_eq!(
            call_count(&publication, ensure_index),
            0,
            "global publication leaves RegExp dormant",
        );
    }

    let realm_type = type_index::<RealmRecord>(schema);
    let regexp_state_field = regexp_state_field(&registered);
    let state_reads = ensure
        .iter()
        .filter(|operator| {
            matches!(operator, Operator::StructGet { struct_type_index, field_index }
                if *struct_type_index == realm_type && *field_index == regexp_state_field)
        })
        .count();
    let state_writes = ensure
        .iter()
        .filter(|operator| {
            matches!(operator, Operator::StructSet { struct_type_index, field_index }
                if *struct_type_index == realm_type && *field_index == regexp_state_field)
        })
        .count();
    assert_eq!(
        state_reads, 1,
        "the ensure owner reads its typed Realm state"
    );
    assert_eq!(
        state_writes, 2,
        "Building then Ready bracket one complete graph"
    );
    let state_values = [
        RegExpInitializationState::Dormant.word(),
        RegExpInitializationState::Building.word(),
        RegExpInitializationState::Ready.word(),
    ];
    assert_eq!(state_values, [0, 1, 2]);

    for helper in [
        "helper::intrinsic_constructor_allocate",
        "helper::intrinsic_accessor_allocate",
        "helper::intrinsic_accessor_publish",
        "helper::deferred_intrinsic_install",
        "helper::deferred_intrinsic_install_run",
    ] {
        assert!(
            call_count(&ensure, named(&names, helper)) > 0,
            "RegExp family construction belongs to the demand owner: {helper}",
        );
    }

    let intrinsic_table = type_index::<IntrinsicTable>(schema);
    assert_eq!(
        ensure
            .iter()
            .filter(|operator| matches!(operator,
                Operator::ArraySet { array_type_index } if *array_type_index == intrinsic_table))
            .count(),
        5,
        "prototype, constructor, and the three canonical symbol methods are completed together",
    );
    assert_eq!(ref_func_count(&initialize, constructor), 0);
    assert_eq!(ref_func_count(&ensure, constructor), 1);
}
