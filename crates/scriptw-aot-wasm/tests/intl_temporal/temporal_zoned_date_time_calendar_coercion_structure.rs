const TEMPORAL_SOURCE: &str = include_str!("../../src/builtins/temporal.rs");
const ZONED_CONVERSION_SOURCE: &str =
    include_str!("../../src/builtins/temporal/zoned_conversion.rs");
const EXACT_SOURCE: &str = include_str!("../../src/builtins/temporal_zone_provider/exact.rs");
const PROVIDER_SOURCE: &str = include_str!("../../src/builtins/temporal_zone_provider.rs");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end after {start}: {end}"))
        .0
}

#[test]
fn zoned_date_time_calendar_proof_is_sealed_at_the_allocator() {
    let calendar = bounded(
        EXACT_SOURCE,
        "pub(crate) struct TemporalCalendarSlotLocals {",
        "\n}\nimpl TemporalCalendarSlotLocals",
    );
    assert_eq!(
        calendar.split_whitespace().collect::<String>(),
        "identifier:GcLocal<StringValue>,calendar_id:I64Local,"
    );
    let allocation = bounded(
        PROVIDER_SOURCE,
        "pub(super) struct TemporalZonedAllocationInput<'a> {",
        "\n}\nimpl<'a> TemporalZonedAllocationInput<'a>",
    );
    assert_eq!(
        allocation.split_whitespace().collect::<String>(),
        "instant:&'aNormalizedTemporalInstantLocals,zone:&'aResolvedTemporalZoneLocals,calendar:&'aTemporalCalendarSlotLocals,"
    );
    let allocator = bounded(
        TEMPORAL_SOURCE,
        "pub(in crate::builtins) fn emit_alloc_temporal_zoned_date_time(",
        "pub(crate) fn emit_temporal_zoned_date_time_record_from_receiver(",
    );
    assert!(allocator.contains("input: TemporalZonedAllocationInput<'_>"));
    assert!(allocator.contains(".struct_type::<TemporalZonedDateTimeObject>()"));
    assert!(allocator.contains("GcOperand::reference(input.calendar().identifier(), schema)"));
    assert!(!allocator.contains("emit_temporal_canonicalize_calendar("));
    assert!(!allocator.contains("emit_temporal_to_temporal_calendar_identifier("));
    for source in [TEMPORAL_SOURCE, ZONED_CONVERSION_SOURCE] {
        assert!(!source.contains("ZonedDateTimeCalendarCoercion"));
        assert!(!source.contains("emit_temporal_zoned_date_time_calendar("));
        assert!(!source.contains("TemporalCalendarSlotLocals {"));
    }
}

#[test]
fn property_bag_and_constructor_retain_their_spec_operation_proofs() {
    let property_bag = bounded(
        ZONED_CONVERSION_SOURCE,
        "    fn emit_temporal_zoned_date_time_from_property_bag(",
        "\n}",
    );
    assert_eq!(
        property_bag
            .matches("emit_temporal_calendar_slot_from_value(")
            .count(),
        1
    );
    assert!(!property_bag.contains("emit_temporal_constructor_calendar_slot("));
    assert!(!property_bag.contains("emit_temporal_canonicalize_calendar("));
    assert!(!property_bag.contains("emit_temporal_plain_date_calendar("));
    let read = property_bag
        .find("emit_temporal_duration_option_get(input, \"calendar\", &value, f)?")
        .expect("calendar property read");
    let proof = property_bag
        .find("emit_temporal_calendar_slot_from_value(&value, f)?")
        .expect("fallible calendar identifier admission");
    let fields = property_bag
        .find("emit_temporal_property_bag_integer(input, \"day\"")
        .expect("first date field read");
    assert!(read < proof && proof < fields);

    let constructor = bounded(
        TEMPORAL_SOURCE,
        "    pub(crate) fn emit_temporal_zoned_date_time_constructor(",
        "    pub(in crate::builtins) fn emit_temporal_parse_time_zone_value_into(",
    );
    assert_eq!(
        constructor
            .matches("emit_temporal_constructor_calendar_slot(")
            .count(),
        1
    );
    assert!(!constructor.contains("emit_temporal_calendar_slot_from_value"));
    assert!(!constructor.contains("emit_temporal_canonicalize_calendar("));
    for body in [property_bag, constructor] {
        let allocation = "TemporalZonedAllocationInput::new(&instant, &zone, &calendar)";
        assert_eq!(body.matches(allocation).count(), 1);
        assert_eq!(body.matches("calendar.release(self, f)").count(), 1);
        assert!(
            body.find("let calendar =").unwrap() < body.find(allocation).unwrap()
                && body.find(allocation).unwrap() < body.find("calendar.release(self, f)").unwrap()
        );
    }
    assert!(property_bag.contains("TemporalPrototypeSource::Intrinsic"));
    assert!(constructor.contains("TemporalPrototypeSource::Constructor(&prototype)"));
}

#[test]
fn calendar_factories_perform_only_their_prescribed_operation() {
    let bag_factory = bounded(
        EXACT_SOURCE,
        "    pub(in crate::builtins) fn emit_temporal_calendar_slot_from_value(",
        "    pub(in crate::builtins) fn emit_temporal_constructor_calendar_slot(",
    );
    let constructor_factory = bounded(
        EXACT_SOURCE,
        "    pub(in crate::builtins) fn emit_temporal_constructor_calendar_slot(",
        "\n}\n\nimpl FunctionBuilder<'_>",
    );
    for (body, prescribed, other) in [
        (
            bag_factory,
            "emit_temporal_to_temporal_calendar_identifier(",
            "emit_temporal_canonicalize_calendar(",
        ),
        (
            constructor_factory,
            "emit_temporal_canonicalize_calendar(",
            "emit_temporal_to_temporal_calendar_identifier(",
        ),
    ] {
        assert!(body.contains("Result<TemporalCalendarSlotLocals, EmitError>"));
        assert_eq!(body.matches(prescribed).count(), 1);
        assert!(!body.contains(other));
        assert!(!body.contains(": bool"));
        assert!(!body.contains("TemporalCalendarSlotLocals {"));
    }
    assert!(constructor_factory.contains("TemporalCalendarCanonicalizationContext::ZonedDateTime"));
    assert!(bag_factory
        .contains("RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_CALENDAR_MUST_BE_A_STRING"));

    // The public operations return the sealed proof from its actual validator.
    // A parser Throw must be routed before its normal identifier is projected.
    let parsed = bounded(
        EXACT_SOURCE,
        "pub(crate) fn emit_temporal_to_temporal_calendar_identifier(",
        "\n}",
    );
    let call = parsed
        .find("TemporalCalendarIdentifierArguments::new(")
        .unwrap();
    let throw = parsed
        .find("self.emit_return_current_completion(f)")
        .unwrap();
    let normal = parsed
        .find("pending.value().cast_reference::<StringValue>")
        .unwrap();
    let validate = parsed
        .find("emit_temporal_calendar_slot_from_identifier(&canonical, f)?")
        .unwrap();
    assert!(call < throw && throw < normal && normal < validate);

    let canonicalize = bounded(
        EXACT_SOURCE,
        "pub(in crate::builtins) fn emit_temporal_canonicalize_calendar(",
        "pub(crate) fn emit_temporal_plain_date_calendar(",
    );
    assert!(canonicalize.contains("emit_temporal_calendar_canonical_string(&string, context, f)?"));
    assert!(!canonicalize.contains("TemporalCalendarIdentifierArguments"));
    let validator = bounded(
        EXACT_SOURCE,
        "    fn emit_temporal_calendar_canonical_string(",
        "    pub(in crate::builtins) fn emit_temporal_canonicalize_calendar(",
    );
    let rejected = validator.find("context.range_error_message()").unwrap();
    let admitted = validator.find("Ok(TemporalCalendarSlotLocals {").unwrap();
    assert!(rejected < admitted);
    assert!(validator.contains("calendar.runtime_code()"));
    assert!(validator.contains("calendar.canonical()"));
    assert!(!EXACT_SOURCE.contains("pub(crate) fn emit_temporal_calendar_canonical_string("));
    assert!(!EXACT_SOURCE
        .contains("pub(in crate::builtins) fn emit_temporal_calendar_canonical_string("));
}
