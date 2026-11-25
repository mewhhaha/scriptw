use super::*;
use crate::{
    ArrayDestructuringElementIr, ArrayDestructuringPatternIr, ArrayPatternProtocol,
    FunctionTargetKnowledge, HeapShape, KindSet, ObjectShape, PrivateNameId, SpecOperationIr,
    Strictness,
};
use std::sync::Arc;

fn binding(name: &str, slot: u32) -> OwnedEnvBindingIr {
    OwnedEnvBindingIr {
        mutability: crate::EnvironmentBindingMutabilityIr::Mutable,
        name: name.into(),
        slot,
    }
}

fn read(binding: &OwnedEnvBindingIr, kind: ValueKind) -> TypedExpr {
    TypedExpr::from_info(
        ValueInfo::new(kind),
        ExprIr::Identifier(binding.name.clone()),
    )
}

fn key_literal() -> TypedExpr {
    TypedExpr::from_info(
        ValueInfo::new(ValueKind::String),
        ExprIr::String("same".into()),
    )
}

fn prepared_source(owned: &[OwnedEnvBindingIr]) -> ObjectDestructuringSourceIr {
    ObjectDestructuringSourceIr::prepare(key_literal(), owned[0].clone(), owned[1].clone(), owned)
        .unwrap()
        .1
}

#[test]
fn source_factory_publishes_raw_once_before_real_to_object_with_exact_boxed_union() {
    let owned = [binding("raw", 0), binding("boxed", 1)];
    let mut info = ValueInfo::new(ValueKind::Function);
    info.heap_shape = Some(Arc::new(HeapShape::Object(ObjectShape::default())));
    info.function_targets = FunctionTargetKnowledge::exact("original callable".into());
    let raw = TypedExpr::from_info(info, ExprIr::FunctionValue("original callable".into()));
    let (prefix, source) = ObjectDestructuringSourceIr::prepare(
        raw.clone(),
        owned[0].clone(),
        owned[1].clone(),
        &owned,
    )
    .unwrap();
    let [StatementIr::Lexical {
        mode: BindingMode::Let,
        name: raw_name,
        init: raw_init,
    }, StatementIr::Lexical {
        mode: BindingMode::Let,
        name: boxed_name,
        init: boxed_init,
    }] = prefix.as_slice()
    else {
        panic!("actual raw and boxed producers in order")
    };
    assert_eq!(raw_name, &owned[0].name);
    assert_eq!(
        raw_init, &raw,
        "only the first producer evaluates the original value"
    );
    assert_eq!(boxed_name, &owned[1].name);
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = &boxed_init.expr
    else {
        panic!("actual ToObject, not metadata over a raw read")
    };
    assert_eq!(*operation, SpecOperationIr::ToObject);
    assert_eq!(operands, &[source.raw_receiver().clone()]);
    assert_eq!(source.raw_receiver().kind, ValueKind::Function);
    assert_eq!(source.raw_receiver().function_targets, raw.function_targets);
    assert!(source.raw_receiver().heap_shape.is_none());
    let expected = KindSet::from_kind(ValueKind::Object)
        .union(KindSet::from_kind(ValueKind::Array))
        .union(KindSet::from_kind(ValueKind::Function))
        .union(KindSet::from_kind(ValueKind::Arguments));
    assert_eq!(boxed_init.kind, ValueKind::Dynamic);
    assert_eq!(boxed_init.possible_kinds, expected);
    assert_eq!(source.boxed.value_info(), boxed_init.value_info());
    assert_eq!(source.boxed.expr, ExprIr::Identifier(owned[1].name.clone()));
    assert!(source.boxed.heap_shape.is_none());
    assert_eq!(
        source.boxed.function_targets,
        FunctionTargetKnowledge::unknown()
    );
}

#[test]
fn preparation_rejects_absent_empty_duplicate_and_name_or_slot_aliased_allocations() {
    let raw = binding("raw", 0);
    let boxed = binding("boxed", 1);
    let prepare = |inventory: &[OwnedEnvBindingIr]| {
        ObjectDestructuringSourceIr::prepare(key_literal(), raw.clone(), boxed.clone(), inventory)
    };
    for missing in [&[][..], &[raw.clone()][..], &[boxed.clone()][..]] {
        assert_eq!(
            prepare(missing),
            Err(ObjectDestructuringPreparationError::MissingOwnedBinding)
        );
    }
    for alias in [raw.clone(), binding("raw", 7), binding("other", raw.slot)] {
        let inventory = [raw.clone(), boxed.clone(), alias];
        assert_eq!(
            prepare(&inventory),
            Err(ObjectDestructuringPreparationError::AliasedOwnedBinding)
        );
    }
    assert_eq!(
        ObjectDestructuringSourceIr::prepare(
            key_literal(),
            raw.clone(),
            raw.clone(),
            &[raw.clone()],
        ),
        Err(ObjectDestructuringPreparationError::AliasedOwnedBinding)
    );
    let empty = binding("", 5);
    assert_eq!(
        ObjectDestructuringSourceIr::prepare(
            key_literal(),
            empty.clone(),
            boxed.clone(),
            &[empty.clone(), boxed],
        ),
        Err(ObjectDestructuringPreparationError::MissingOwnedBinding)
    );
    assert_eq!(
        ObjectDestructuringKeyIr::prepare(key_literal(), empty.clone(), &[empty]),
        Err(ObjectDestructuringPreparationError::MissingOwnedBinding)
    );
    assert_eq!(
        ObjectDestructuringKeyIr::prepare(key_literal(), raw.clone(), &[]),
        Err(ObjectDestructuringPreparationError::MissingOwnedBinding)
    );
    for alias in [raw.clone(), binding("raw", 8), binding("other", raw.slot)] {
        assert_eq!(
            ObjectDestructuringKeyIr::prepare(key_literal(), raw.clone(), &[raw.clone(), alias]),
            Err(ObjectDestructuringPreparationError::AliasedOwnedBinding)
        );
    }
}

#[test]
fn key_factory_normalizes_once_and_rest_rejects_cell_aliases_without_rejecting_equal_keys() {
    let owned = [
        binding("raw", 0),
        binding("boxed", 1),
        binding("first key", 2),
        binding("second key", 3),
    ];
    let source = prepared_source(&owned);
    let raw_key = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Number),
        ExprIr::Number(17.0f64.to_bits()),
    );
    let (prefix, first) =
        ObjectDestructuringKeyIr::prepare(raw_key.clone(), owned[2].clone(), &owned).unwrap();
    let StatementIr::Lexical {
        mode: BindingMode::Let,
        name,
        init,
    } = prefix
    else {
        panic!("actual normalized-key publication")
    };
    assert_eq!(name, owned[2].name);
    let ExprIr::SpecOperation {
        operation,
        operands,
    } = init.expr
    else {
        panic!("actual ToPropertyKey")
    };
    assert_eq!(operation, SpecOperationIr::ToPropertyKey);
    assert_eq!(operands, [raw_key]);
    assert_eq!(
        first.normalized.possible_kinds,
        KindSet::from_kind(ValueKind::String).union(KindSet::from_kind(ValueKind::Symbol))
    );
    assert_eq!(first.normalized.kind, ValueKind::Dynamic);
    assert!(first.normalized.heap_shape.is_none());
    assert_eq!(
        first.normalized.function_targets,
        FunctionTargetKnowledge::none()
    );
    let get = ObjectDestructuringOperationIr::get_v(&source, &first).unwrap();
    let ObjectDestructuringOperationView::GetV {
        raw_receiver,
        boxed,
        key,
    } = get.use_view()
    else {
        panic!("GetV")
    };
    assert_eq!(raw_receiver, source.raw_receiver());
    assert_eq!(boxed, &source.boxed);
    assert_eq!(key, &first.normalized);
    assert_eq!(get.into_expr().kind, ValueKind::Dynamic);
    // Equal values are legal: each source property still has a different
    // captured cell, so later evaluation cannot replace an earlier exclusion.
    let first = ObjectDestructuringKeyIr::prepare(key_literal(), owned[2].clone(), &owned)
        .unwrap()
        .1;
    let second = ObjectDestructuringKeyIr::prepare(key_literal(), owned[3].clone(), &owned)
        .unwrap()
        .1;
    let rest =
        ObjectDestructuringOperationIr::rest(&source, &[first.clone(), second.clone()]).unwrap();
    let ObjectDestructuringOperationView::Rest { boxed, excluded } = rest.use_view() else {
        panic!("rest")
    };
    assert_eq!(boxed, &source.boxed);
    assert_eq!(
        excluded,
        &[first.normalized.clone(), second.normalized.clone()]
    );
    assert_eq!(rest.into_expr().kind, ValueKind::Object);
    for alias in [
        owned[2].clone(),
        binding(&owned[2].name, 8),
        binding("other key", owned[2].slot),
    ] {
        let replacement = ObjectDestructuringKeyIr::prepare(key_literal(), alias.clone(), &[alias])
            .unwrap()
            .1;
        assert_eq!(
            ObjectDestructuringOperationIr::rest(&source, &[first.clone(), replacement]),
            Err(ObjectDestructuringPreparationError::AliasedOwnedBinding)
        );
    }
    for binding in &owned[..2] {
        let alias = ObjectDestructuringKeyIr::prepare(key_literal(), binding.clone(), &owned)
            .unwrap()
            .1;
        assert_eq!(
            ObjectDestructuringOperationIr::get_v(&source, &alias),
            Err(ObjectDestructuringPreparationError::AliasedOwnedBinding)
        );
        assert_eq!(
            ObjectDestructuringOperationIr::rest(&source, &[alias]),
            Err(ObjectDestructuringPreparationError::AliasedOwnedBinding)
        );
    }
}

#[test]
fn target_factory_requires_retained_member_operands_and_refuses_var_targets() {
    let owned = [binding("member", 0), binding("member key", 1)];
    let property = |target, key| DestructuringTargetIr::AssignmentProperty {
        target,
        key,
        strictness: Strictness::Strict,
    };
    let assigned = TypedExpr::from_info(
        ValueInfo::new(ValueKind::Number),
        ExprIr::Number(7.0f64.to_bits()),
    );
    let target = property(
        read(&owned[0], ValueKind::Object),
        DestructuringPropertyKeyIr::Computed(read(&owned[1], ValueKind::Dynamic)),
    );
    let admitted =
        ObjectDestructuringOperationIr::put_target(target.clone(), assigned.clone(), &owned)
            .unwrap();
    let ObjectDestructuringOperationView::PutTarget {
        target: retained_target,
        value,
    } = admitted.use_view()
    else {
        panic!("PutTarget")
    };
    assert_eq!(retained_target, &target);
    assert_eq!(value, &assigned);
    assert_eq!(admitted.into_expr().value_info(), assigned.value_info());
    for target in [
        property(
            TypedExpr::undefined(),
            DestructuringPropertyKeyIr::Static("x".into()),
        ),
        property(
            read(&owned[0], ValueKind::Object),
            DestructuringPropertyKeyIr::Computed(key_literal()),
        ),
    ] {
        assert_eq!(
            ObjectDestructuringOperationIr::put_target(target, assigned.clone(), &owned),
            Err(ObjectDestructuringPreparationError::UncapturedTarget)
        );
    }
    let missing = binding("not retained", 2);
    for target in [
        property(
            read(&missing, ValueKind::Object),
            DestructuringPropertyKeyIr::Static("x".into()),
        ),
        property(
            read(&owned[0], ValueKind::Object),
            DestructuringPropertyKeyIr::Computed(read(&missing, ValueKind::Dynamic)),
        ),
        DestructuringTargetIr::AssignmentPrivate {
            target: read(&missing, ValueKind::Object),
            private_name_id: PrivateNameId::new(0, 0),
        },
    ] {
        assert_eq!(
            ObjectDestructuringOperationIr::put_target(target, assigned.clone(), &owned),
            Err(ObjectDestructuringPreparationError::MissingOwnedBinding)
        );
    }
    assert_eq!(
        ObjectDestructuringOperationIr::put_target(
            property(
                read(&owned[0], ValueKind::Object),
                DestructuringPropertyKeyIr::Computed(read(&owned[0], ValueKind::Dynamic))
            ),
            assigned.clone(),
            &owned
        ),
        Err(ObjectDestructuringPreparationError::AliasedOwnedBinding)
    );
    let private = DestructuringTargetIr::AssignmentPrivate {
        target: read(&owned[0], ValueKind::Object),
        private_name_id: PrivateNameId::new(0, 0),
    };
    assert!(ObjectDestructuringOperationIr::put_target(private, assigned.clone(), &owned).is_ok());
    assert_eq!(
        ObjectDestructuringOperationIr::put_target(
            DestructuringTargetIr::Binding {
                mode: BindingMode::Var,
                name: "x".into(),
            },
            assigned,
            &owned
        ),
        Err(ObjectDestructuringPreparationError::UncapturedTarget)
    );
}

#[test]
fn operation_keeps_value_identity_and_visits_value_before_nested_target_operands() {
    let owned = [binding("member", 0), binding("member key", 1)];
    let mut info = ValueInfo::new(ValueKind::Function);
    info.heap_shape = Some(Arc::new(HeapShape::Object(ObjectShape::default())));
    info.function_targets = FunctionTargetKnowledge::exact("returned callable".into());
    let value = TypedExpr::from_info(info, ExprIr::Identifier("assigned".into()));
    let nested = DestructuringTargetIr::NestedArray(Box::new(ArrayDestructuringPatternIr {
        elements: vec![ArrayDestructuringElementIr::Target {
            target: DestructuringTargetIr::AssignmentProperty {
                target: read(&owned[0], ValueKind::Object),
                key: DestructuringPropertyKeyIr::Computed(read(&owned[1], ValueKind::Dynamic)),
                strictness: Strictness::Sloppy,
            },
            default: Some(TypedExpr::from_info(
                ValueInfo::new(ValueKind::Dynamic),
                ExprIr::Identifier("nested default".into()),
            )),
        }],
        protocol: ArrayPatternProtocol::ARRAY_DESTRUCTURING,
    }));
    let operation =
        ObjectDestructuringOperationIr::put_target(nested, value.clone(), &owned).unwrap();
    let mut visited = Vec::new();
    operation.visit_expressions(&mut |expression| {
        let ExprIr::Identifier(name) = &expression.expr else {
            panic!("retained read")
        };
        visited.push(name.clone());
    });
    assert_eq!(
        visited,
        ["assigned", "member", "member key", "nested default"]
    );
    let result = operation.into_expr();
    assert_eq!(result.kind, value.kind);
    assert_eq!(result.possible_kinds, value.possible_kinds);
    assert_eq!(result.function_targets, value.function_targets);
    assert!(
        result.heap_shape.is_none(),
        "target/default effects invalidate the object shape"
    );
    let binding_target = DestructuringTargetIr::Binding {
        mode: BindingMode::Const,
        name: "actual lexical target".into(),
    };
    let operation = ObjectDestructuringOperationIr::put_target(binding_target, value, &[]).unwrap();
    let mut bindings = Vec::new();
    operation.visit_bindings(&mut |mode, name| bindings.push((mode, name.to_owned())));
    assert_eq!(
        bindings,
        [(BindingMode::Const, "actual lexical target".into())]
    );
}
