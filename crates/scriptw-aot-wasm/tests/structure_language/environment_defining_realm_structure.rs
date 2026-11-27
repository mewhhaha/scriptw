const ENVIRONMENTS: &str = include_str!("../../src/environments.rs");
const GLOBAL_ENVIRONMENT: &str = include_str!("../../src/environments/global_environment.rs");

fn normalized_body(source: &str, name: &str, next: &str) -> String {
    let (_, body) = source.split_once(name).expect("emitter exists");
    let (body, _) = body.split_once(next).expect("next emitter exists");
    body.chars().filter(|ch| !ch.is_whitespace()).collect()
}

#[test]
fn environment_construction_inherits_the_parent_realm() {
    let body = normalized_body(
        ENVIRONMENTS,
        "fn emit_environment_record(",
        "pub(crate) fn emit_allocate_environment_cell(",
    );
    assert!(body.contains(
        "reserve_gc_local::<RealmRecord,Nullable>(function).initialize_null(schema,function)"
    ));
    assert!(body.contains(
        "letparent_reference=parent.load(schema,function).non_null_or_branch(self.local_branch_label(parent_absent),function)"
    ));
    assert!(body.contains(
        "defining_realm.replace(schema.struct_type::<Environment>().field(EnvironmentSchema::DEFINING_REALM).read_top(parent_reference,function).reference(),function,);"
    ));
    assert!(body.contains(
        "struct_type::<Environment>().construct((GcOperand::reference(&defining_realm,schema),"
    ));
}

#[test]
fn source_lookups_use_the_inherited_realm_before_the_parent_walk() {
    let global = normalized_body(
        GLOBAL_ENVIRONMENT,
        "fn emit_global_environment_from(",
        "pub(crate) fn emit_source_execution_realm_to_local(",
    );
    let fast = global
        .find("field(RealmRecordSchema::GLOBAL_ENVIRONMENT).read_top(realm,function)")
        .expect("global Environment is read from the inherited Realm");
    let walk = global
        .find("field(EnvironmentSchema::PARENT)")
        .expect("parent walk remains available");
    assert!(fast < walk);

    let realm = normalized_body(
        GLOBAL_ENVIRONMENT,
        "fn emit_source_execution_realm_to_local(",
        "fn emit_execution_realm_to_local(",
    );
    assert!(realm.contains(
        "defining_realm.read(self.current_environment(),schema,function).reference();function.instruction(&Instruction::BrOnNonNull(0));letenvironment=self.emit_source_global_environment_to_local(function);"
    ));
}
