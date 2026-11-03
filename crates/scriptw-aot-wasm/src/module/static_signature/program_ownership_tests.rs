use super::*;

#[test]
fn callable_entry_program_operands_are_nonnull_and_follow_existing_semantic_slots() {
    for (signature, program_position, context_position, context_layout) in [
        (
            StaticSignature::JavaScriptFunction,
            10,
            9,
            GcLayout::RealmRecord,
        ),
        (
            StaticSignature::LocalArgumentsFunction,
            14,
            13,
            GcLayout::RealmRecord,
        ),
        (
            StaticSignature::GeneratorFunction,
            7,
            4,
            GcLayout::GeneratorActivation,
        ),
        (
            StaticSignature::AsyncFunction,
            7,
            4,
            GcLayout::AsyncActivation,
        ),
        (
            StaticSignature::AsyncGeneratorFunction,
            7,
            4,
            GcLayout::AsyncGeneratorActivation,
        ),
        (
            StaticSignature::PreparedScript,
            12,
            11,
            GcLayout::DirectEvalExecutionContext,
        ),
    ] {
        let parameters = signature.parameter_types();
        assert_eq!(parameters.len(), program_position + 1);
        assert!(matches!(
            parameters.last(),
            Some(Gc(GcLayout::ProgramInstance, NonNullable))
        ));
        assert!(matches!(parameters[context_position], Gc(layout, _) if layout == context_layout));
        assert_eq!(signature.result_types(), &COMPLETION_TYPES);
    }
}

#[test]
fn program_publication_and_host_source_compilation_have_distinct_result_domains() {
    assert!(StaticSignature::ProgramInitialize
        .parameter_types()
        .is_empty());
    assert!(matches!(
        StaticSignature::ProgramInitialize.result_types(),
        [Gc(GcLayout::ProgramInstance, NonNullable)]
    ));
    assert!(StaticSignature::Main.parameter_types().is_empty());
    assert_eq!(StaticSignature::Main.result_types(), &COMPLETION_TYPES);
    let host = StaticSignature::HostCompileDynamicFunction;
    assert!(matches!(
        host.parameter_types(),
        [
            I32,
            I32,
            I64,
            EqRef,
            Gc(GcLayout::ValueArray, NonNullable),
            Gc(GcLayout::FunctionContext, NonNullable),
            Gc(GcLayout::Environment, Nullable)
        ]
    ));
    assert_eq!(&host.result_types()[..5], &COMPLETION_TYPES);
    assert!(matches!(
        host.result_types()[5],
        Gc(GcLayout::StringValue, Nullable)
    ));
    assert_eq!(host.result_types().len(), 6);
}
