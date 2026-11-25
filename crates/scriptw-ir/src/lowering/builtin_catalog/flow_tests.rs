use super::*;

fn with_lowerer(policy: HostSurfacePolicy, check: impl FnOnce(&mut ScriptLowerer<'_>)) {
    let source = "";
    let mut interner = Interner::default();
    let scope = Scope::new_global();
    let script = Parser::new(Source::from_bytes(source.as_bytes()))
        .parse_script(&scope, &mut interner)
        .expect("empty flow fixture parses");
    let analysis = AnalysisBuilder::default().finish(&script, &interner, source);
    let mut lowerer = ScriptLowerer::new(
        &interner,
        &analysis,
        source.into(),
        RootThisBinding::GlobalObject,
        SCRIPT_OWNER_ID.to_string(),
        policy,
    );
    check(&mut lowerer);
}

#[test]
fn unchanged_global_branch_recipes_are_idempotent_and_remain_borrowed() {
    for policy in [HostSurfacePolicy::Product, HostSurfacePolicy::Test262] {
        with_lowerer(policy, |lowerer| {
            let left = global_properties(policy);
            let right = left.clone();
            let merged = lowerer.merge_global_properties(&left, &right);
            assert_eq!(left.changed_keys().count(), 0);
            assert_eq!(merged.changed_keys().count(), 0);
            assert!(merged.iter().eq(global_property_baseline(policy).iter()));
            // The sparse join relies on this algebraic property for the shared
            // catalog rows, rather than assuming all ValueInfo kinds are equal.
            for property in global_property_baseline(policy).values() {
                assert_eq!(
                    lowerer.merge_value_infos(
                        property.value_info.clone(),
                        property.value_info.clone(),
                    ),
                    property.value_info
                );
            }
        });
    }
}

#[test]
fn global_branch_join_retains_shadowing_tombstones_and_intersection_presence() {
    with_lowerer(HostSurfacePolicy::Product, |lowerer| {
        let mut left = global_properties(HostSurfacePolicy::Product);
        let mut right = left.clone();
        left.get_mut(MATH_NAME).unwrap().value_info = ValueInfo::new(ValueKind::Number);
        right.get_mut(MATH_NAME).unwrap().value_info = ValueInfo::new(ValueKind::String);
        left.remove("Infinity");
        let property = GlobalPropertyInfo {
            value_info: ValueInfo::new(ValueKind::Number),
            proven_present: true,
            configurable: false,
            source: GlobalPropertySource::GlobalWrite,
        };
        left.insert("oneSide".to_string(), property.clone());
        left.insert("shared".to_string(), property.clone());
        right.insert(
            "shared".to_string(),
            GlobalPropertyInfo {
                proven_present: false,
                configurable: true,
                ..property
            },
        );
        let joined = lowerer.merge_global_properties(&left, &right);
        let math = joined.get(MATH_NAME).unwrap();
        assert_eq!(
            math.value_info.possible_kinds,
            KindSet::from_kind(ValueKind::Number).union(KindSet::from_kind(ValueKind::String))
        );
        assert!(joined.get("Infinity").is_none());
        assert!(joined.get("oneSide").is_none());
        assert!(!joined.get("shared").unwrap().proven_present);
        assert!(!joined.get("shared").unwrap().configurable);
        assert_eq!(joined.changed_keys().count(), 3);
        let fresh = global_properties(HostSurfacePolicy::Product);
        assert!(fresh.get("Infinity").is_some());
        assert_eq!(
            fresh.get(MATH_NAME).unwrap().value_info.kind,
            ValueKind::Object
        );
    });
}

#[test]
fn present_shape_invalidation_owns_only_live_rows_and_preserves_fresh_returns() {
    with_lowerer(HostSurfacePolicy::Product, |lowerer| {
        let baseline = function_signature_baseline();
        let live = baseline
            .iter()
            .filter(|(_, signature)| signature.has_live_shape_roots())
            .map(|(id, _)| id.clone())
            .collect::<BTreeSet<_>>();
        assert!(live.len() < baseline.len());
        lowerer.function_signatures = function_signatures();
        lowerer.visit_live_heap_shape_roots(&DiscardHeapShapeFacts);
        assert_eq!(
            lowerer
                .function_signatures
                .changed_keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            live
        );
        for (id, signature) in baseline {
            let actual = lowerer.function_signatures.get(id).unwrap();
            if !live.contains(id) {
                assert!(std::ptr::eq(actual, signature));
            }
            if matches!(
                signature.return_shape,
                FunctionReturnShape::RecreatedPerCall { .. }
            ) {
                assert_eq!(actual.return_shape, signature.return_shape);
            }
        }
        assert!(function_signatures().iter().eq(baseline.iter()));
    });
}

#[test]
fn global_branch_join_with_an_empty_fact_domain_cannot_reintroduce_builtins() {
    with_lowerer(HostSurfacePolicy::Product, |lowerer| {
        let builtins = global_properties(HostSurfacePolicy::Product);
        let empty = GlobalProperties::default();
        for joined in [
            lowerer.merge_global_properties(&builtins, &empty),
            lowerer.merge_global_properties(&empty, &builtins),
        ] {
            assert_eq!(joined.keys().count(), 0);
        }
    });
}
