use super::*;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{
    lower_with_host_surface_policy, HostSurfacePolicy, ObjectAccessorShape, ObjectShape,
};

fn plan_metas(script: &ScriptIr) -> Result<BTreeMap<FunctionId, WasmFunctionMeta>, EmitError> {
    let source = SourceFunctions::partition(script);
    let layout = FunctionIndexLayout::new(
        0,
        source.runtime_owned.len(),
        0,
        0,
        0,
        source.program.len() + script.prepared_script_units().count(),
        source.binary_numeric_count(),
        0,
        crate::runtime_artifact::ModuleKind::Standalone,
        crate::function_layout::ModuleEntryRole::Main,
    );
    build_function_metas(&source, script.prepared_script_units(), &[], &[], &layout)
}

fn lower_script(source: &str) -> ScriptIr {
    let parsed = parse(source, ParseOptions::script()).expect("script should parse");
    lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262)
        .script
        .expect("script should lower")
}

#[test]
fn dynamic_property_keys_root_every_possible_shape_accessor() {
    let getter = "dynamic.getter".to_string();
    let setter = "dynamic.setter".to_string();
    let shape = HeapShape::Object(ObjectShape {
        properties: BTreeMap::from([(
            "p".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: getter.clone(),
                }),
                setter: Some(ObjectAccessorShape {
                    function_id: setter.clone(),
                }),
            },
        )]),
        ..ObjectShape::default()
    });
    let key = PropertyKeyIr::StringExpr(Box::new(TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::Identifier("key".to_string()),
    )));

    assert!(shape_accessor_references_function(
        Some(&shape),
        &key,
        &getter,
        ShapeAccessorReferenceSelection::Getter,
    ));
    assert!(shape_accessor_references_function(
        Some(&shape),
        &key,
        &setter,
        ShapeAccessorReferenceSelection::Setter,
    ));
    assert!(!shape_accessor_references_function(
        Some(&shape),
        &key,
        &getter,
        ShapeAccessorReferenceSelection::Setter,
    ));
    assert!(shape_accessor_references_function(
        Some(&shape),
        &key,
        &getter,
        ShapeAccessorReferenceSelection::GetterOrSetter,
    ));
}

#[test]
fn for_in_roots_internal_methods_and_primitive_prototype_installers() {
    for (source, constructor) in [
        (
            "for (var key in 7) {}",
            StandardBuiltinId::NumberConstructor,
        ),
        (
            "for (var key in false) {}",
            StandardBuiltinId::BooleanConstructor,
        ),
        (
            "for (var key in 7n) {}",
            StandardBuiltinId::BigIntConstructor,
        ),
        (
            "for (var key in 'text') {}",
            StandardBuiltinId::StringConstructor,
        ),
        (
            "for (var key in Symbol()) {}",
            StandardBuiltinId::SymbolConstructor,
        ),
    ] {
        let script = lower_script(source);
        for builtin in FOR_IN_INTRINSICS.into_iter().chain([constructor]) {
            assert!(
                script_references_standard_builtin(&script, builtin),
                "missing {builtin:?} dependency for {source}"
            );
        }
    }
}

#[test]
fn class_element_execution_metadata_requires_a_class_context() {
    let script = lower_script(
        "function ordinary() {} class C { instance = 1; static shared = 2; static {} method() {} }",
    );
    let metas = plan_metas(&script).expect("class callable identities are unique");
    let meta_named = |name: &str| {
        metas
            .values()
            .find(|meta| meta.name == name)
            .unwrap_or_else(|| panic!("function meta `{name}` should exist"))
    };

    assert_eq!(
        meta_named("C.field.instance").class_element_execution_kind,
        ClassElementExecutionKind::InstanceFieldInitializer
    );
    assert_eq!(
        meta_named("C.field.shared").class_element_execution_kind,
        ClassElementExecutionKind::StaticFieldInitializer
    );
    assert_eq!(
        meta_named("C.<static>").class_element_execution_kind,
        ClassElementExecutionKind::StaticBlock
    );
    assert!(meta_named("C.field.instance").has_class_execution_context());
    assert!(meta_named("C.field.shared").has_class_execution_context());
    assert!(meta_named("C.<static>").has_class_execution_context());
    assert!(meta_named("C.method").has_class_execution_context());
    assert!(meta_named("C").has_class_execution_context());
    assert!(!meta_named("ordinary").has_class_execution_context());
}

#[test]
fn duplicate_callable_ids_are_rejected_before_body_emission() {
    let mut script = lower_script("function repeated() {} repeated();");
    let duplicate = script.functions[0].clone();
    let duplicate_id = duplicate.id.clone();
    script.functions.push(duplicate);
    let error = plan_metas(&script).expect_err("one FunctionId cannot own two planned entries");
    assert!(error
        .to_string()
        .contains(&format!("duplicate function identity `{duplicate_id}`")));
}

#[test]
fn nested_private_functions_use_a_non_class_function_context() {
    let script = lower_script(
        "class Base { get value() { return 1; } }
             class C extends Base {
                 #value = 2;
                 method() {
                     function ordinary(receiver) { return receiver.#value; }
                     function middle() { return () => this.#value; }
                     const arrow = () => super.value + this.#value;
                     return [ordinary, middle, arrow];
                 }
             }",
    );
    let metas = plan_metas(&script).expect("class callable identities are unique");
    let ordinary = script
        .functions
        .iter()
        .find(|function| function.name == "ordinary")
        .expect("nested ordinary function should be lowered");
    let arrow = script
        .functions
        .iter()
        .find(|function| {
            function.protocol.flavor() == FunctionFlavor::Arrow
                && function.captures_private_environment
        })
        .expect("nested arrow function should be lowered");
    let middle = script
        .functions
        .iter()
        .find(|function| function.name == "middle")
        .expect("transitive private-environment function should be lowered");

    for function in [ordinary, middle, arrow] {
        assert!(function.captures_private_environment);
        let meta = &metas[&function.id];
        assert!(!meta.has_class_execution_context());
    }
}

#[test]
fn string_prototype_methods_root_the_constructor_bootstrap() {
    for builtin in [
        StandardBuiltinId::StringPrototypeToLocaleLowerCase,
        StandardBuiltinId::StringPrototypeToLocaleUpperCase,
        StandardBuiltinId::StringPrototypeToLowerCase,
        StandardBuiltinId::StringPrototypeToUpperCase,
        StandardBuiltinId::StringPrototypeSlice,
        StandardBuiltinId::StringPrototypeIncludes,
        StandardBuiltinId::StringPrototypeCharAt,
    ] {
        assert!(builtin.string_prototype_method_name().is_some());
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(builtin);
        assert!(plan.standard_roots.contains(&builtin));
        assert!(plan
            .standard_roots
            .contains(&StandardBuiltinId::StringConstructor));
    }
}

/// The rooting walk must terminate on a cyclic dependency, and must still
/// root both ends of the cycle.
///
/// `Temporal.PlainDateTime`'s arm requires `TemporalZonedDateTimeConstructor`
/// (its `toZonedDateTime` returns one) and `Temporal.ZonedDateTime`'s arm
/// requires `TemporalPlainDateTimeConstructor` (its `toPlainDateTime`
/// returns one). Before the `walked` guard this pair recursed until the
/// engine's 64 MiB worker stack was gone and the process died with SIGABRT
/// — from `print(typeof Temporal.ZonedDateTime)`, from
/// `print(typeof globalThis)`, and from any test262 case containing
/// `var global = this;`, which is how it took down a full-suite sweep and
/// aborted the whole `--test cli` process ten tests in.
///
/// A stack overflow aborts the test binary rather than failing one test, so
/// a regression here is loud: the whole `scriptw-aot-wasm` lib target dies.
/// That is the intended behaviour of this test, not a flaw in it — there is
/// no way to catch a blown guard page in-process.
///
/// Batch 6 added five more entry points, and it is worth being exact about
/// what they buy, because the comment here first claimed more. All 28
/// ZonedDateTime members share ONE or-pattern arm (`:2225`) whose body does
/// not branch per member, so entering at
/// `...PrototypeAdd` executes byte-identical code to entering at
/// `...PrototypeEraGetter` — the only difference is which id
/// `standard_roots.insert` receives first. **These are shape assertions
/// that the arithmetic members really are in that arm, not a longer walk.**
/// A member left out of the arm is already a compile error, since the
/// `match` over `StandardBuiltinId` is exhaustive; what these entries pin is
/// that nobody moves them into an arm that does not root both ends.
///
/// The entry point that would give genuinely new cycle coverage is a
/// `Temporal.Duration.prototype.round`/`total` member, on the day that arm
/// grows the `relativeTo` edge — see the note at the Duration root in the
/// ZonedDateTime arm. Non-termination shows up as a SIGABRT in a
/// whole-suite sweep, never as a red unit test.
///
/// `TemporalDurationConstructor` is deliberately **not** an entry point
/// here: its own arm is a leaf (it inserts its family directly into
/// `standard_roots` and requires nothing), so entering there roots neither
/// end of the cycle and the two assertions below would be false. That is
/// the current shape of the Duration arm, not an invariant — if it ever
/// grows a `require_standard_builtin(TemporalZonedDateTimeConstructor)` for
/// `relativeTo`, add it to this list at the same time.
#[test]
fn a_cyclic_rooting_dependency_terminates_and_roots_both_ends() {
    for entry in [
        StandardBuiltinId::TemporalZonedDateTimeConstructor,
        StandardBuiltinId::TemporalPlainDateTimeConstructor,
        StandardBuiltinId::TemporalZonedDateTimePrototypeEraGetter,
        StandardBuiltinId::TemporalZonedDateTimePrototypeEraYearGetter,
        StandardBuiltinId::TemporalZonedDateTimePrototypeWith,
        StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDate,
        StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDateTime,
        StandardBuiltinId::TemporalZonedDateTimePrototypeAdd,
        StandardBuiltinId::TemporalZonedDateTimePrototypeSubtract,
        StandardBuiltinId::TemporalZonedDateTimePrototypeUntil,
        StandardBuiltinId::TemporalZonedDateTimePrototypeSince,
        StandardBuiltinId::TemporalZonedDateTimePrototypeWithCalendar,
    ] {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(entry);

        assert!(
            plan.standard_roots.contains(&entry),
            "the entry point itself must be rooted: {entry:?}"
        );
        // Both ends of the cycle, whichever end we entered from. This is
        // the half that a naive `if !standard_roots.insert(..) { return }`
        // guard would have been free to drop.
        assert!(
            plan.standard_roots
                .contains(&StandardBuiltinId::TemporalZonedDateTimeConstructor),
            "entering at {entry:?} must still root the ZonedDateTime constructor"
        );
        assert!(
            plan.standard_roots
                .contains(&StandardBuiltinId::TemporalPlainDateTimeConstructor),
            "entering at {entry:?} must still root the PlainDateTime constructor"
        );
        assert!(
            plan.temporal_namespace_members().is_some(),
            "entering at {entry:?} must root the complete Temporal namespace"
        );
    }
}

/// What `zdt.add`/`subtract`/`until`/`since` must find already emitted.
///
/// **The Duration assertion is pre-satisfied and is deliberately not the
/// point of this test.** An earlier version of this doc claimed
/// `a_cyclic_rooting_dependency_terminates_and_roots_both_ends` "would still
/// pass with the Duration line removed — hence a second test". That is
/// false, and so is the implied "the Duration root has no other witness":
/// the ZonedDateTime arm's first statement requires
/// `TemporalPlainDateTimeConstructor`, whose arm's first statement requires
/// `TemporalDurationConstructor`, so Duration is rooted transitively for
/// every ZonedDateTime member with or without that line — and was before
/// batch 6. It is kept below as a statement of what these four bodies need,
/// not as coverage of a line that could regress.
///
/// The real coverage here is the delegation list: these bodies are
/// composition, they look their delegates up through `emit_direct_js_call`
/// on `self.functions`, and a delegate that is not rooted is an
/// `EmitError::unsupported` at emit time rather than a compile error. That
/// half is a genuine contract statement for a shape the type system does
/// not cover.
#[test]
fn zoned_date_time_arithmetic_roots_the_duration_family_it_allocates() {
    for entry in [
        StandardBuiltinId::TemporalZonedDateTimePrototypeAdd,
        StandardBuiltinId::TemporalZonedDateTimePrototypeSubtract,
        StandardBuiltinId::TemporalZonedDateTimePrototypeUntil,
        StandardBuiltinId::TemporalZonedDateTimePrototypeSince,
    ] {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(entry);

        assert!(
            plan.standard_roots
                .contains(&StandardBuiltinId::TemporalDurationConstructor),
            "{entry:?} allocates or reads a Temporal.Duration and must root its constructor \
                 (pre-satisfied transitively via TemporalPlainDateTimeConstructor; see this \
                 test's doc comment before treating a failure here as a Duration-line regression)"
        );
        // The bodies delegate to the PlainDateTime namesakes rather than
        // re-deriving the arithmetic, so those four have to be emitted, not
        // merely their constructor.
        for delegate in [
            StandardBuiltinId::TemporalPlainDateTimePrototypeAdd,
            StandardBuiltinId::TemporalPlainDateTimePrototypeSubtract,
            StandardBuiltinId::TemporalPlainDateTimePrototypeUntil,
            StandardBuiltinId::TemporalPlainDateTimePrototypeSince,
            StandardBuiltinId::TemporalPlainDateTimePrototypeToZonedDateTime,
            StandardBuiltinId::TemporalZonedDateTimePrototypeWith,
            StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDate,
            StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDateTime,
            StandardBuiltinId::TemporalZonedDateTimeFrom,
        ] {
            assert!(
                plan.standard_roots.contains(&delegate),
                "{entry:?} delegates to {delegate:?}, which must be rooted"
            );
        }
    }

    // `withCalendar` does *not* delegate — it rewrites the record in place
    // — so it is deliberately not in the loop above. It still needs the
    // ZonedDateTime prototype global, which its own arm roots.
    let mut plan = RuntimeBootstrapPlan::default();
    plan.require_standard_builtin(StandardBuiltinId::TemporalZonedDateTimePrototypeWithCalendar);
    assert!(plan
        .standard_roots
        .contains(&StandardBuiltinId::TemporalZonedDateTimeConstructor));
}

/// Requiring the same builtin twice must be a no-op the second time, and
/// must not shrink the answer.
///
/// `walked` makes the second call return early, so this pins the claim that
/// the arms are idempotent: walking once yields the same `standard_roots`
/// as walking repeatedly.
#[test]
fn requiring_a_builtin_twice_is_idempotent() {
    let mut once = RuntimeBootstrapPlan::default();
    once.require_standard_builtin(StandardBuiltinId::TemporalZonedDateTimeConstructor);

    let mut twice = RuntimeBootstrapPlan::default();
    twice.require_standard_builtin(StandardBuiltinId::TemporalZonedDateTimeConstructor);
    twice.require_standard_builtin(StandardBuiltinId::TemporalZonedDateTimeConstructor);

    assert_eq!(once.standard_roots, twice.standard_roots);
    assert_eq!(once.temporal, twice.temporal);
}

#[test]
fn string_call_result_chain_keeps_bootstrap_and_both_methods_live() {
    let mut plan = RuntimeBootstrapPlan::default();
    for builtin in [
        StandardBuiltinId::StringPrototypeToLocaleLowerCase,
        StandardBuiltinId::StringPrototypeToLocaleUpperCase,
        StandardBuiltinId::StringPrototypeToLowerCase,
        StandardBuiltinId::StringPrototypeToUpperCase,
        StandardBuiltinId::StringPrototypeSlice,
    ] {
        plan.require_standard_builtin(builtin);
    }

    assert!(plan
        .standard_roots
        .contains(&StandardBuiltinId::StringConstructor));
    assert!(plan
        .standard_roots
        .contains(&StandardBuiltinId::StringPrototypeToUpperCase));
    assert!(plan
        .standard_roots
        .contains(&StandardBuiltinId::StringPrototypeSlice));
}

#[test]
fn slice_method_references_array_and_string_builtins() {
    let key = PropertyKeyIr::StaticString("slice".to_string());
    for builtin in [
        StandardBuiltinId::ArrayPrototypeSlice,
        StandardBuiltinId::StringPrototypeSlice,
    ] {
        assert!(optimized_call_method_references_function(
            &key,
            &builtin.function_id()
        ));
    }
}

#[test]
fn concat_lookup_keeps_array_string_and_iterator_bodies_live() {
    let key = PropertyKeyIr::StaticString("concat".to_string());
    for builtin in [
        StandardBuiltinId::ArrayPrototypeConcat,
        StandardBuiltinId::StringPrototypeConcat,
        StandardBuiltinId::IteratorConcat,
    ] {
        assert!(optimized_call_method_references_function(
            &key,
            &builtin.function_id()
        ));
    }
    assert!(!optimized_call_method_references_function(
        &key,
        &StandardBuiltinId::ArrayPrototypeFlat.function_id()
    ));
}

#[test]
fn sort_method_references_and_roots_the_array_builtin() {
    let key = PropertyKeyIr::StaticString("sort".to_string());
    assert!(optimized_call_method_references_function(
        &key,
        &StandardBuiltinId::ArrayPrototypeSort.function_id()
    ));

    let mut plan = RuntimeBootstrapPlan::default();
    plan.require_standard_builtin(StandardBuiltinId::ArrayPrototypeSort);
    assert!(plan
        .standard_roots
        .contains(&StandardBuiltinId::ArrayPrototypeSort));
    assert!(plan
        .standard_roots
        .contains(&StandardBuiltinId::ArrayConstructor));
}

#[test]
fn dynamic_symbol_string_and_value_methods_are_referenced_and_bootstrapped() {
    for (name, builtin) in [
        ("toString", StandardBuiltinId::SymbolPrototypeToString),
        ("valueOf", StandardBuiltinId::SymbolPrototypeValueOf),
    ] {
        let key = PropertyKeyIr::StaticString(name.to_string());
        assert!(optimized_call_method_references_function(
            &key,
            &builtin.function_id()
        ));

        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(builtin);
        assert!(plan
            .standard_roots
            .contains(&StandardBuiltinId::SymbolConstructor));
    }
}

#[test]
fn nested_dynamic_array_method_get_roots_materialized_builtin() {
    let script = lower_script(
        "var values = []; var key = {}; key[Symbol.toPrimitive] = function (hint) { values.push(hint); return 0; };",
    );
    assert!(script_references_standard_builtin(
        &script,
        StandardBuiltinId::ArrayPrototypePush
    ));
    let mut plan = RuntimeBootstrapPlan::default();
    plan.require_standard_builtin(StandardBuiltinId::ArrayPrototypePush);
    assert!(plan
        .standard_roots
        .contains(&StandardBuiltinId::ArrayConstructor));
}

#[test]
fn iterator_prototype_members_root_the_constructor_bootstrap() {
    for builtin in [
        StandardBuiltinId::IteratorFrom,
        StandardBuiltinId::ArrayIteratorIdentity,
        StandardBuiltinId::IteratorPrototypeMap,
        StandardBuiltinId::IteratorPrototypeDrop,
        StandardBuiltinId::IteratorPrototypeSymbolDispose,
        StandardBuiltinId::IteratorPrototypeToStringTagGetter,
    ] {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(builtin);
        assert!(plan.standard_roots.contains(&builtin));
        assert!(plan
            .standard_roots
            .contains(&StandardBuiltinId::IteratorConstructor));
    }
}

#[test]
fn map_prototype_members_root_the_map_constructor_bootstrap() {
    for builtin in [
        StandardBuiltinId::MapGroupBy,
        StandardBuiltinId::MapPrototypeClear,
        StandardBuiltinId::MapPrototypeDelete,
        StandardBuiltinId::MapPrototypeForEach,
        StandardBuiltinId::MapPrototypeKeys,
        StandardBuiltinId::MapPrototypeValues,
        StandardBuiltinId::MapPrototypeEntries,
        StandardBuiltinId::MapIteratorNext,
        StandardBuiltinId::MapPrototypeGet,
        StandardBuiltinId::MapPrototypeGetOrInsert,
        StandardBuiltinId::MapPrototypeGetOrInsertComputed,
        StandardBuiltinId::MapPrototypeHas,
        StandardBuiltinId::MapPrototypeSet,
        StandardBuiltinId::MapPrototypeSizeGetter,
    ] {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(builtin);
        assert!(plan.standard_roots.contains(&builtin));
        assert!(plan
            .standard_roots
            .contains(&StandardBuiltinId::MapConstructor));
    }
}

#[test]
fn set_prototype_members_root_the_set_constructor_bootstrap() {
    for builtin in [
        StandardBuiltinId::SetPrototypeAdd,
        StandardBuiltinId::SetPrototypeClear,
        StandardBuiltinId::SetPrototypeDelete,
        StandardBuiltinId::SetPrototypeDifference,
        StandardBuiltinId::SetPrototypeForEach,
        StandardBuiltinId::SetPrototypeIntersection,
        StandardBuiltinId::SetPrototypeIsDisjointFrom,
        StandardBuiltinId::SetPrototypeIsSubsetOf,
        StandardBuiltinId::SetPrototypeIsSupersetOf,
        StandardBuiltinId::SetPrototypeSymmetricDifference,
        StandardBuiltinId::SetPrototypeUnion,
        StandardBuiltinId::SetPrototypeValues,
        StandardBuiltinId::SetPrototypeEntries,
        StandardBuiltinId::SetIteratorNext,
        StandardBuiltinId::SetPrototypeHas,
        StandardBuiltinId::SetPrototypeSizeGetter,
    ] {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(builtin);
        assert!(plan.standard_roots.contains(&builtin));
        assert!(plan
            .standard_roots
            .contains(&StandardBuiltinId::SetConstructor));
    }
}

#[test]
fn set_constructor_roots_sync_iterator_machinery() {
    let mut plan = RuntimeBootstrapPlan::default();
    plan.require_standard_builtin(StandardBuiltinId::SetConstructor);

    for builtin in [
        StandardBuiltinId::ArrayPrototypeValues,
        StandardBuiltinId::ArrayIteratorNext,
        StandardBuiltinId::ArrayIteratorIdentity,
        StandardBuiltinId::StringPrototypeIterator,
        StandardBuiltinId::StringIteratorNext,
    ] {
        assert!(plan.standard_roots.contains(&builtin));
    }
}

#[test]
fn map_constructor_roots_sync_iterator_machinery() {
    let mut plan = RuntimeBootstrapPlan::default();
    plan.require_standard_builtin(StandardBuiltinId::MapConstructor);

    assert!(plan.standard_roots.contains(&StandardBuiltinId::MapGroupBy));
    for builtin in [
        StandardBuiltinId::ArrayPrototypeValues,
        StandardBuiltinId::ArrayIteratorNext,
        StandardBuiltinId::ArrayIteratorIdentity,
        StandardBuiltinId::StringPrototypeIterator,
        StandardBuiltinId::StringIteratorNext,
    ] {
        assert!(plan.standard_roots.contains(&builtin));
    }
}

#[test]
fn math_sum_precise_roots_sync_iterator_machinery() {
    let mut plan = RuntimeBootstrapPlan::default();
    plan.require_standard_builtin(StandardBuiltinId::MathSumPrecise);

    for builtin in [
        StandardBuiltinId::ArrayPrototypeValues,
        StandardBuiltinId::ArrayIteratorNext,
        StandardBuiltinId::ArrayIteratorIdentity,
        StandardBuiltinId::StringPrototypeIterator,
        StandardBuiltinId::StringIteratorNext,
    ] {
        assert!(plan.standard_roots.contains(&builtin));
    }
}

#[test]
fn typed_array_constructors_root_array_iterator_machinery() {
    let mut plan = RuntimeBootstrapPlan::default();
    plan.require_standard_builtin(StandardBuiltinId::Uint8ArrayConstructor);

    for builtin in [
        StandardBuiltinId::ArrayPrototypeValues,
        StandardBuiltinId::TypedArrayPrototypeIncludes,
        StandardBuiltinId::TypedArrayPrototypeIndexOf,
        StandardBuiltinId::TypedArrayPrototypeLastIndexOf,
        StandardBuiltinId::TypedArrayPrototypeFind,
        StandardBuiltinId::TypedArrayPrototypeFindIndex,
        StandardBuiltinId::TypedArrayPrototypeFindLast,
        StandardBuiltinId::TypedArrayPrototypeFindLastIndex,
        StandardBuiltinId::TypedArrayPrototypeEvery,
        StandardBuiltinId::TypedArrayPrototypeSome,
        StandardBuiltinId::TypedArrayPrototypeMap,
        StandardBuiltinId::TypedArrayPrototypeFilter,
        StandardBuiltinId::TypedArrayPrototypeForEach,
        StandardBuiltinId::TypedArrayPrototypeReduce,
        StandardBuiltinId::TypedArrayPrototypeReduceRight,
        StandardBuiltinId::TypedArrayPrototypeValues,
        StandardBuiltinId::TypedArrayPrototypeKeys,
        StandardBuiltinId::TypedArrayPrototypeEntries,
        StandardBuiltinId::ArrayIteratorNext,
        StandardBuiltinId::ArrayIteratorIdentity,
    ] {
        assert!(plan.standard_roots.contains(&builtin));
    }
}

#[test]
fn every_typed_array_constructor_roots_the_array_buffer_prototype_installer() {
    for kind in TypedArrayElementKind::ALL {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(kind.constructor());

        assert!(
            plan.standard_roots
                .contains(&StandardBuiltinId::ArrayBufferConstructor),
            "{:?} exposes an ArrayBuffer whose prototype would be left uninstalled",
            kind.constructor()
        );
    }
}

#[test]
fn object_constructor_roots_group_by_and_property_key_bootstrap() {
    let mut plan = RuntimeBootstrapPlan::default();
    plan.require_standard_builtin(StandardBuiltinId::ObjectConstructor);

    assert!(plan
        .standard_roots
        .contains(&StandardBuiltinId::ObjectGroupBy));
    assert!(plan
        .standard_roots
        .contains(&StandardBuiltinId::SymbolPrototypeToPrimitive));
    assert!(plan
        .standard_roots
        .contains(&StandardBuiltinId::SymbolConstructor));
    for builtin in [
        StandardBuiltinId::ArrayPrototypeValues,
        StandardBuiltinId::ArrayIteratorNext,
        StandardBuiltinId::ArrayIteratorIdentity,
        StandardBuiltinId::StringPrototypeIterator,
        StandardBuiltinId::StringIteratorNext,
    ] {
        assert!(plan.standard_roots.contains(&builtin));
    }
}

#[test]
fn object_constructor_roots_proto_accessor_pair() {
    let mut plan = RuntimeBootstrapPlan::default();
    plan.require_standard_builtin(StandardBuiltinId::ObjectConstructor);

    assert!(plan
        .standard_roots
        .contains(&StandardBuiltinId::ObjectPrototypeProtoGetter));
    assert!(plan
        .standard_roots
        .contains(&StandardBuiltinId::ObjectPrototypeProtoSetter));
}

#[test]
fn object_locale_string_roots_primitive_to_string_methods() {
    let mut plan = RuntimeBootstrapPlan::default();
    plan.require_standard_builtin(StandardBuiltinId::ObjectPrototypeToLocaleString);

    for builtin in [
        StandardBuiltinId::StringPrototypeToString,
        StandardBuiltinId::NumberPrototypeToString,
        StandardBuiltinId::BooleanPrototypeToString,
        StandardBuiltinId::SymbolPrototypeToString,
        StandardBuiltinId::BigIntPrototypeToString,
    ] {
        assert!(plan.standard_roots.contains(&builtin));
    }
}

#[test]
fn accessor_definers_root_the_canonical_definition_path() {
    for builtin in [
        StandardBuiltinId::ObjectConstructor,
        StandardBuiltinId::ObjectPrototypeDefineGetter,
        StandardBuiltinId::ObjectPrototypeDefineSetter,
    ] {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(builtin);
        assert!(plan
            .standard_roots
            .contains(&StandardBuiltinId::ObjectDefineProperty));
        assert!(plan
            .standard_roots
            .contains(&StandardBuiltinId::ObjectGetOwnPropertyDescriptor));
    }
    let mut plan = RuntimeBootstrapPlan::default();
    plan.require_standard_builtin(StandardBuiltinId::ObjectConstructor);
    for builtin in [
        StandardBuiltinId::ObjectPrototypeDefineGetter,
        StandardBuiltinId::ObjectPrototypeDefineSetter,
    ] {
        assert!(plan.standard_roots.contains(&builtin));
        assert_eq!(standard_builtin_length(builtin), 2);
    }
}

#[test]
fn boolean_property_definitions_root_the_ordinary_definition_body() {
    for entry in [
        StandardBuiltinId::JsonParse,
        StandardBuiltinId::ReflectDefineProperty,
    ] {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(entry);
        assert!(plan
            .standard_roots
            .contains(&StandardBuiltinId::ReflectDefineProperty));
        assert!(plan
            .standard_roots
            .contains(&StandardBuiltinId::ObjectDefineProperty));
    }
}

#[test]
fn descriptor_entry_points_root_generic_descriptor_lookup() {
    for builtin in [
        StandardBuiltinId::ObjectDefineProperty,
        StandardBuiltinId::ReflectDefineProperty,
        StandardBuiltinId::ReflectGetOwnPropertyDescriptor,
        StandardBuiltinId::ObjectPrototypeHasOwnProperty,
        StandardBuiltinId::ObjectPrototypeLookupGetter,
        StandardBuiltinId::ObjectPrototypeLookupSetter,
        StandardBuiltinId::ObjectPrototypePropertyIsEnumerable,
    ] {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(builtin);

        assert!(plan.standard_roots.contains(&builtin));
        assert!(plan
            .standard_roots
            .contains(&StandardBuiltinId::ObjectGetOwnPropertyDescriptor));
    }
}

#[test]
fn reflected_call_entry_points_root_proxy_dispatch() {
    for builtin in [
        StandardBuiltinId::ReflectApply,
        StandardBuiltinId::ReflectConstruct,
        StandardBuiltinId::FunctionPrototypeApply,
    ] {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(builtin);

        assert!(plan.standard_roots.contains(&builtin));
        assert!(plan
            .standard_roots
            .contains(&StandardBuiltinId::ProxyConstructor));
    }
}

#[test]
fn object_has_own_roots_generic_descriptor_lookup() {
    let mut plan = RuntimeBootstrapPlan::default();
    plan.require_standard_builtin(StandardBuiltinId::ObjectHasOwn);

    for builtin in [
        StandardBuiltinId::ObjectHasOwn,
        StandardBuiltinId::ReflectGetOwnPropertyDescriptor,
        StandardBuiltinId::ObjectGetOwnPropertyDescriptor,
    ] {
        assert!(plan.standard_roots.contains(&builtin));
    }
}

#[test]
fn object_descriptor_map_builtins_root_reflective_operations() {
    for entry_point in [
        StandardBuiltinId::ObjectCreate,
        StandardBuiltinId::ObjectDefineProperties,
    ] {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(entry_point);

        for builtin in [
            entry_point,
            StandardBuiltinId::ObjectDefineProperties,
            StandardBuiltinId::ObjectDefineProperty,
            StandardBuiltinId::ReflectOwnKeys,
            StandardBuiltinId::ReflectGetOwnPropertyDescriptor,
            StandardBuiltinId::ReflectDefineProperty,
        ] {
            assert!(plan.standard_roots.contains(&builtin));
        }
    }
}

#[test]
fn object_integrity_builtins_root_reflective_property_operations() {
    for (integrity_builtin, dependencies) in [
        (
            StandardBuiltinId::ObjectSeal,
            &[
                StandardBuiltinId::ReflectOwnKeys,
                StandardBuiltinId::ReflectDefineProperty,
                StandardBuiltinId::ObjectGetOwnPropertyDescriptor,
            ][..],
        ),
        (
            StandardBuiltinId::ObjectFreeze,
            &[
                StandardBuiltinId::ReflectOwnKeys,
                StandardBuiltinId::ReflectGetOwnPropertyDescriptor,
                StandardBuiltinId::ReflectDefineProperty,
                StandardBuiltinId::ObjectGetOwnPropertyDescriptor,
            ][..],
        ),
        (
            StandardBuiltinId::ObjectIsSealed,
            &[
                StandardBuiltinId::ReflectOwnKeys,
                StandardBuiltinId::ReflectGetOwnPropertyDescriptor,
                StandardBuiltinId::ObjectGetOwnPropertyDescriptor,
            ][..],
        ),
        (
            StandardBuiltinId::ObjectIsFrozen,
            &[
                StandardBuiltinId::ReflectOwnKeys,
                StandardBuiltinId::ReflectGetOwnPropertyDescriptor,
                StandardBuiltinId::ObjectGetOwnPropertyDescriptor,
            ][..],
        ),
    ] {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(integrity_builtin);

        assert!(plan.standard_roots.contains(&integrity_builtin));
        for dependency in dependencies {
            assert!(plan.standard_roots.contains(dependency));
        }
    }
}

#[test]
fn enumerable_own_properties_root_reflective_property_operations() {
    for enumerable_own_properties_builtin in [
        StandardBuiltinId::ObjectKeys,
        StandardBuiltinId::ObjectEntries,
        StandardBuiltinId::ObjectValues,
    ] {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(enumerable_own_properties_builtin);

        for builtin in [
            enumerable_own_properties_builtin,
            StandardBuiltinId::ReflectOwnKeys,
            StandardBuiltinId::ReflectGetOwnPropertyDescriptor,
            StandardBuiltinId::ObjectGetOwnPropertyDescriptor,
        ] {
            assert!(plan.standard_roots.contains(&builtin));
        }
    }
}

#[test]
fn object_property_copy_builtins_root_reflective_property_operations() {
    for builtin in [
        StandardBuiltinId::ObjectAssign,
        StandardBuiltinId::ObjectGetOwnPropertyDescriptors,
    ] {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(builtin);

        for dependency in [
            builtin,
            StandardBuiltinId::ReflectOwnKeys,
            StandardBuiltinId::ReflectGetOwnPropertyDescriptor,
            StandardBuiltinId::ObjectGetOwnPropertyDescriptor,
        ] {
            assert!(plan.standard_roots.contains(&dependency));
        }

        if builtin == StandardBuiltinId::ObjectAssign {
            assert!(plan.standard_roots.contains(&StandardBuiltinId::ReflectSet));
        }
    }
}

#[test]
fn regexp_literal_roots_intrinsic_regexp_bootstrap() {
    let script = lower_script("/a/;");
    assert!(matches!(
        &script.body.statements[0],
        StatementIr::Expression(_)
    ));

    assert!(script_references_standard_builtin(
        &script,
        StandardBuiltinId::RegExpConstructor
    ));
}

#[test]
fn every_installed_intl_namespace_member_is_rooted_by_the_namespace_plan() {
    // Kept only for its message. The containment itself is now a `const _`
    // block beside `INTL_NAMESPACE_ROOTS`, so the drift this used to catch
    // at `cargo test` no longer compiles; do not read this test's presence
    // as meaning the check lives at test time.
    for (name, builtin) in INTL_NAMESPACE_CONSTRUCTORS {
        assert!(
            INTL_NAMESPACE_ROOTS.contains(builtin),
            "`Intl.{name}` is installed from `INTL_NAMESPACE_CONSTRUCTORS` but \
                 `INTL_NAMESPACE_ROOTS` never roots `{}`",
            builtin.debug_name()
        );
    }
}

#[test]
fn rooting_any_intl_builtin_installs_the_whole_namespace() {
    // Reaching one member of the family — say `Intl.DateTimeFormat` through
    // a folded member expression, with no bare `Intl` anywhere — must still
    // produce an `Intl` object carrying everything the shape declares.
    for entry_point in INTL_NAMESPACE_ROOTS {
        let mut plan = RuntimeBootstrapPlan::default();
        plan.require_standard_builtin(entry_point);

        assert!(
            plan.intl_namespace_members().is_some(),
            "rooting `{}` must install the `Intl` namespace object",
            entry_point.debug_name()
        );
        for builtin in INTL_NAMESPACE_ROOTS {
            assert!(
                plan.should_initialize_standard_builtin(builtin),
                "rooting `{}` must also root `{}`",
                entry_point.debug_name(),
                builtin.debug_name()
            );
        }
    }
}

/// No `Intl` builtin may exist outside the two namespace lists.
///
/// The three checks around this one partition as CONSTRUCTORS ⊆ ROOTS,
/// ROOTS ⊆ the `require_standard_builtin` match arm, and (at debug runtime
/// only) match arm ⊆ ROOTS. None of them can see the drift that actually
/// happens next: a *new* `Intl` builtin — `Intl.NumberFormat` is the
/// obvious follow-on — added to neither list. Its symptom is precisely what
/// this lane exists to prevent: a builtin reachable as a member expression
/// but never rooted from a bare `Intl`, so `intl402/**/prop-desc.js`-shaped
/// reflective reads find it missing.
///
/// `debug_name()` spells every `Intl` id with an `Intl.` prefix, which is
/// what makes the total direction checkable at all without a fourth list.
#[test]
fn every_intl_standard_builtin_is_in_the_namespace_root_list() {
    for builtin in StandardBuiltinId::all_functions() {
        if !builtin.debug_name().contains("Intl.") {
            continue;
        }
        assert!(
            INTL_NAMESPACE_ROOTS.contains(builtin),
            "`{}` is an `Intl` builtin but appears in no namespace list, so a bare \
                 `Intl` reference will not root it",
            builtin.debug_name()
        );
    }
}
