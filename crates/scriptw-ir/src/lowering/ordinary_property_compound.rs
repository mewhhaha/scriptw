use super::*;

pub(super) enum BuiltinGetterReceiverProvenance {
    ProvenNonProxy,
    MayBeProxy,
}

pub(super) struct OrdinaryPropertyReferenceMetadata {
    base_value_info: ValueInfo,
    base_binding_name: Option<String>,
    possible_receiver_values: Box<[ValueInfo]>,
    possible_mutation_authorities: BTreeSet<OrdinaryPropertyMutationAuthority>,
    base_evaluation_may_have_intervening_effects: bool,
    key_may_call_user_code: bool,
    key_evaluation_may_have_intervening_effects: bool,
    unknown_property_hooks_possible: bool,
    getter_may_dispatch_transitive_property_hooks: bool,
    possible_getters: PropertyHookTargets,
    possible_setters: PropertyHookTargets,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum OrdinaryPropertyMutationAuthority {
    GlobalObject,
    ArrayPrototype,
    NumberPrototype,
    BooleanPrototype,
    WellKnownSymbolPrototype,
}

impl OrdinaryPropertyMutationAuthority {
    const ALL: [Self; 5] = [
        Self::GlobalObject,
        Self::ArrayPrototype,
        Self::NumberPrototype,
        Self::BooleanPrototype,
        Self::WellKnownSymbolPrototype,
    ];
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ExpressionEffectAccounting {
    ProvenEffectFree,
    TrackedByLowering,
    UntrackedUserCodePossible,
}

impl ExpressionEffectAccounting {
    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::UntrackedUserCodePossible, _) | (_, Self::UntrackedUserCodePossible) => {
                Self::UntrackedUserCodePossible
            }
            (Self::TrackedByLowering, _) | (_, Self::TrackedByLowering) => Self::TrackedByLowering,
            (Self::ProvenEffectFree, Self::ProvenEffectFree) => Self::ProvenEffectFree,
        }
    }

    pub(super) fn intervening_effects_observed(
        self,
        before_effect_epoch: u64,
        after_effect_epoch: u64,
    ) -> bool {
        match self {
            Self::ProvenEffectFree => false,
            Self::TrackedByLowering => after_effect_epoch != before_effect_epoch,
            Self::UntrackedUserCodePossible => true,
        }
    }
}

#[must_use = "a possible post-Set fact must be published after alias invalidation"]
enum PendingOrdinaryPropertyPublication {
    CurrentThisShape {
        value_info: ValueInfo,
    },
    BindingShape {
        root_name: String,
        root_value_info: ValueInfo,
    },
    IntrinsicPrototypeShape {
        constructor_name: String,
        constructor_value_info: ValueInfo,
    },
    WellKnownSymbolPrototype {
        constructor_name: String,
        constructor_value_info: ValueInfo,
        primitive_receiver_kind: Option<ValueKind>,
        symbol: WellKnownSymbol,
        value: ValueInfo,
    },
}

impl<'a> ScriptLowerer<'a> {
    /// Lower the two evaluated operands and strictness which jointly identify
    /// one ordinary property Reference. The plan is non-cloneable; the cloned
    /// key is returned only for conservative shape invalidation.
    pub(super) fn lower_ordinary_property_reference_plan(
        &mut self,
        access: &boa_ast::expression::access::SimplePropertyAccess,
    ) -> (
        OrdinaryPropertyReferencePlan,
        PropertyKeyIr,
        OrdinaryPropertyReferenceMetadata,
    ) {
        let base_effect_accounting = self.prepare_potentially_effectful_expression(access.target());
        let before_base_effect_epoch = self.intervening_effect_epoch;
        let base_and_receiver = Box::new(self.lower_property_target(access.target()));
        let base_evaluation_may_have_intervening_effects = base_effect_accounting
            .intervening_effects_observed(before_base_effect_epoch, self.intervening_effect_epoch);
        if base_evaluation_may_have_intervening_effects {
            self.observe_all_planned_source_as_unknown_property_hooks();
            self.invalidate_unknown_user_code_effects();
        }
        let key_effect_accounting = match access.field() {
            PropertyAccessField::Const(_) => ExpressionEffectAccounting::ProvenEffectFree,
            PropertyAccessField::Expr(expression) => {
                self.prepare_potentially_effectful_expression(expression)
            }
        };
        let before_key_effect_epoch = self.intervening_effect_epoch;
        let referenced_name = match access.field() {
            PropertyAccessField::Const(name) => {
                let name = self.interner.resolve_expect(name.sym()).to_string();
                if base_and_receiver.kind == ValueKind::Array && name == "length" {
                    PropertyKeyIr::ArrayLength
                } else {
                    PropertyKeyIr::StaticString(name)
                }
            }
            PropertyAccessField::Expr(expression) => self
                .lower_static_property_key(expression)
                .unwrap_or_else(|| {
                    PropertyKeyIr::StringExpr(Box::new(self.lower_expression(expression)))
                }),
        };
        let key_evaluation_may_have_intervening_effects = key_effect_accounting
            .intervening_effects_observed(before_key_effect_epoch, self.intervening_effect_epoch);
        fn collect_property_accessors(
            property: Option<ObjectShapeProperty>,
            getters: &mut BTreeSet<FunctionId>,
            setters: &mut BTreeSet<FunctionId>,
        ) {
            let Some(ObjectShapeProperty::Accessor { getter, setter }) = property else {
                return;
            };
            if let Some(getter) = getter {
                getters.insert(getter.function_id);
            }
            if let Some(setter) = setter {
                setters.insert(setter.function_id);
            }
        }

        fn collect_receiver_accessors(
            lowerer: &ScriptLowerer<'_>,
            receiver: &TypedExpr,
            referenced_name: &PropertyKeyIr,
            getters: &mut BTreeSet<FunctionId>,
            setters: &mut BTreeSet<FunctionId>,
            possible_receiver_values: &mut Vec<ValueInfo>,
        ) -> bool {
            if let ExprIr::Conditional {
                then_expr,
                else_expr,
                ..
            } = &receiver.expr
            {
                let then_known = collect_receiver_accessors(
                    lowerer,
                    then_expr,
                    referenced_name,
                    getters,
                    setters,
                    possible_receiver_values,
                );
                let else_known = collect_receiver_accessors(
                    lowerer,
                    else_expr,
                    referenced_name,
                    getters,
                    setters,
                    possible_receiver_values,
                );
                return then_known && else_known;
            }

            possible_receiver_values.push(receiver.value_info());

            match referenced_name {
                PropertyKeyIr::StaticString(name) => {
                    let property = lowerer.read_current_object_shape_property(receiver, name);
                    let known = property.is_some();
                    collect_property_accessors(property, getters, setters);
                    known
                }
                PropertyKeyIr::StringExpr(key) if key.kind == ValueKind::Symbol => {
                    let property = match &key.expr {
                        ExprIr::WellKnownSymbol(symbol) => {
                            lowerer.read_current_object_symbol_shape_property(receiver, *symbol)
                        }
                        _ => None,
                    };
                    let known = property.is_some();
                    collect_property_accessors(property, getters, setters);
                    known
                }
                PropertyKeyIr::StringExpr(_) | PropertyKeyIr::ArrayIndex(_) => {
                    let (shape_getters, shape_setters) =
                        ScriptLowerer::possible_shape_accessors(receiver.heap_shape.as_deref());
                    getters.extend(shape_getters);
                    setters.extend(shape_setters);
                    // Snapshot accessors are candidates, not a complete
                    // description of a computed property's current hooks.
                    false
                }
                PropertyKeyIr::ArrayLength => true,
            }
        }

        let mut known_getters = BTreeSet::new();
        let mut known_setters = BTreeSet::new();
        let mut possible_receiver_values = Vec::new();
        let receiver_property_is_known = collect_receiver_accessors(
            self,
            &base_and_receiver,
            &referenced_name,
            &mut known_getters,
            &mut known_setters,
            &mut possible_receiver_values,
        );
        if matches!(
            &referenced_name,
            PropertyKeyIr::StaticString(name) if name == "__proto__"
        ) && Self::value_info_may_be_object(&base_and_receiver.value_info())
        {
            known_getters.insert(StandardBuiltinId::ObjectPrototypeProtoGetter.function_id());
            known_setters.insert(StandardBuiltinId::ObjectPrototypeProtoSetter.function_id());
        }
        let receiver_shapes_are_known = possible_receiver_values
            .iter()
            .all(|receiver| receiver.heap_shape.is_some());
        let receiver_provenance = if receiver_shapes_are_known {
            BuiltinGetterReceiverProvenance::ProvenNonProxy
        } else {
            BuiltinGetterReceiverProvenance::MayBeProxy
        };
        let mut possible_getters = PropertyHookTargets::from_known(known_getters);
        let mut possible_setters = PropertyHookTargets::from_known(known_setters);
        let base_value_info = base_and_receiver.value_info();
        let base_may_be_object = Self::value_info_may_be_object(&base_value_info);
        let possible_mutation_authorities =
            self.ordinary_property_mutation_authorities(&possible_receiver_values);
        let key_may_call_user_code = Self::property_key_may_call_user_code(&referenced_name);
        let prior_unknown_effects = self.unknown_user_code_effects_observed;
        if !receiver_property_is_known
            || base_evaluation_may_have_intervening_effects
            || key_may_call_user_code
            || key_evaluation_may_have_intervening_effects
            || prior_unknown_effects
        {
            let (unknown_getters, unknown_setters) = self.possible_unknown_accessor_functions();
            possible_getters.extend_targets(unknown_getters);
            possible_setters.extend_targets(unknown_setters);
        }

        // EvaluatePropertyAccessWithExpressionKey applies ToPropertyKey after
        // the raw key expression and before GetValue. Object coercion can run
        // arbitrary source code, so discard facts it may mutate before the
        // logical assignment captures its skipped-branch state.
        if key_may_call_user_code || key_evaluation_may_have_intervening_effects {
            self.observe_all_planned_source_as_unknown_property_hooks();
            self.invalidate_unknown_user_code_effects();
        }
        possible_getters.extend_known(self.dynamically_installed_getters.iter().cloned());
        possible_setters.extend_known(self.dynamically_installed_setters.iter().cloned());
        let getter_may_dispatch_transitive_property_hooks = possible_getters
            .iter()
            .filter_map(|getter| StandardBuiltinId::from_function_id(getter))
            .any(|getter| {
                Self::standard_builtin_getter_may_call_user_code(getter, &receiver_provenance)
            });
        if getter_may_dispatch_transitive_property_hooks {
            possible_getters
                .include_all_planned_source(self.analysis.planned_source_function_ids.clone());
        }

        let metadata = OrdinaryPropertyReferenceMetadata {
            base_value_info,
            base_binding_name: match Self::unwrap_parenthesized_expr(access.target()) {
                Expression::Identifier(identifier) => {
                    Some(self.interner.resolve_expect(identifier.sym()).to_string())
                }
                Expression::This(_) => Some(LEXICAL_THIS_NAME.to_string()),
                _ => None,
            },
            possible_receiver_values: possible_receiver_values.into_boxed_slice(),
            possible_mutation_authorities,
            base_evaluation_may_have_intervening_effects,
            key_may_call_user_code,
            key_evaluation_may_have_intervening_effects,
            unknown_property_hooks_possible: base_may_be_object
                && (!receiver_property_is_known
                    || base_evaluation_may_have_intervening_effects
                    || key_may_call_user_code
                    || key_evaluation_may_have_intervening_effects
                    || prior_unknown_effects),
            getter_may_dispatch_transitive_property_hooks,
            possible_getters,
            possible_setters,
        };
        let plan = OrdinaryPropertyReferencePlan::new(
            base_and_receiver,
            referenced_name.clone(),
            self.reference_strictness(),
        );
        (plan, referenced_name, metadata)
    }

    pub(super) fn standard_builtin_getter_may_call_user_code(
        builtin: StandardBuiltinId,
        receiver_provenance: &BuiltinGetterReceiverProvenance,
    ) -> bool {
        match builtin {
            StandardBuiltinId::MapPrototypeSizeGetter
            | StandardBuiltinId::SetPrototypeSizeGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetNanosecondsGetter => false,
            StandardBuiltinId::ObjectPrototypeProtoGetter => match receiver_provenance {
                BuiltinGetterReceiverProvenance::ProvenNonProxy => false,
                BuiltinGetterReceiverProvenance::MayBeProxy => true,
            },
            _ => true,
        }
    }

    pub(super) fn pre_write_global_property_value(
        &self,
        access: &boa_ast::expression::access::SimplePropertyAccess,
        referenced_name: &PropertyKeyIr,
    ) -> Option<PreWriteGlobalPropertyValue> {
        match referenced_name {
            PropertyKeyIr::StaticString(name) if self.is_global_this_expr(access.target()) => {
                Some(self.capture_pre_write_global_property_value(name))
            }
            PropertyKeyIr::StaticString(_)
            | PropertyKeyIr::StringExpr(_)
            | PropertyKeyIr::ArrayIndex(_)
            | PropertyKeyIr::ArrayLength => None,
        }
    }

    pub(super) fn possible_ordinary_property_setters(
        &self,
        metadata: &OrdinaryPropertyReferenceMetadata,
        intervening_user_code: bool,
    ) -> PropertyHookTargets {
        let mut setters = metadata.possible_setters.clone();
        if intervening_user_code || self.unknown_user_code_effects_observed {
            let (_, unknown_setters) = self.possible_unknown_accessor_functions();
            setters.extend_targets(unknown_setters);
        }
        setters.extend_known(self.dynamically_installed_setters.iter().cloned());
        setters
    }

    pub(super) fn possible_ordinary_property_getters(
        metadata: &OrdinaryPropertyReferenceMetadata,
    ) -> PropertyHookTargets {
        metadata.possible_getters.clone()
    }

    pub(super) fn ordinary_property_numeric_coercion_may_call_user_code(
        &self,
        metadata: &OrdinaryPropertyReferenceMetadata,
    ) -> bool {
        if metadata.unknown_property_hooks_possible
            || self.source_functions_may_run(&metadata.possible_getters)
            || metadata.possible_getters.is_empty()
        {
            return true;
        }
        metadata.possible_getters.iter().any(|getter| {
            !matches!(
                StandardBuiltinId::from_function_id(getter),
                Some(
                    StandardBuiltinId::MapPrototypeSizeGetter
                        | StandardBuiltinId::SetPrototypeSizeGetter
                )
            )
        })
    }

    /// A read/modify/write carrier always performs GetValue before deciding
    /// whether or how to write, so a statically known getter observes the
    /// original Reference receiver on every normal path.
    pub(super) fn record_ordinary_property_get(
        &mut self,
        metadata: &OrdinaryPropertyReferenceMetadata,
    ) {
        let mut receiver_info = metadata.base_value_info.clone();
        if metadata.base_evaluation_may_have_intervening_effects
            || metadata.key_may_call_user_code
            || metadata.key_evaluation_may_have_intervening_effects
        {
            // The Reference still owns the same receiver value, but
            // ToPropertyKey can mutate any reachable property before [[Get]].
            receiver_info.heap_shape = None;
        }
        for getter in metadata.possible_getters.iter() {
            if metadata.unknown_property_hooks_possible
                || metadata.getter_may_dispatch_transitive_property_hooks
            {
                self.observe_unknown_property_hook(getter);
            } else {
                self.observe_ordinary_property_hook_this(getter, receiver_info.clone());
                if let Some(signature) = self.function_signatures.get_mut(getter) {
                    Self::merge_omitted_signature_params_as_undefined(signature, 0);
                }
            }
        }
        if metadata.unknown_property_hooks_possible
            || metadata.getter_may_dispatch_transitive_property_hooks
            || self.source_functions_may_run(&metadata.possible_getters)
        {
            self.invalidate_unknown_user_code_effects();
        }
    }

    pub(super) fn ordinary_property_mutation_authorities(
        &self,
        possible_receivers: &[ValueInfo],
    ) -> BTreeSet<OrdinaryPropertyMutationAuthority> {
        let mut authorities = BTreeSet::new();
        for receiver in possible_receivers {
            if !Self::value_info_may_be_object(receiver) {
                continue;
            }
            let Some(base_shape) = receiver.heap_shape.as_deref() else {
                authorities.extend(OrdinaryPropertyMutationAuthority::ALL);
                break;
            };
            if self.global_this_info().heap_shape.as_deref() == Some(base_shape) {
                authorities.insert(OrdinaryPropertyMutationAuthority::GlobalObject);
            }
            for (constructor_name, authority) in [
                (
                    ARRAY_NAME,
                    OrdinaryPropertyMutationAuthority::ArrayPrototype,
                ),
                (
                    NUMBER_NAME,
                    OrdinaryPropertyMutationAuthority::NumberPrototype,
                ),
                (
                    BOOLEAN_NAME,
                    OrdinaryPropertyMutationAuthority::BooleanPrototype,
                ),
            ] {
                let prototype_shape = self
                    .lookup_global_property(constructor_name)
                    .and_then(|constructor| {
                        self.read_current_heap_shape_property(
                            constructor.heap_shape.as_deref()?,
                            "prototype",
                        )
                    })
                    .and_then(|prototype| match prototype {
                        ObjectShapeProperty::Data(prototype) => prototype.heap_shape,
                        ObjectShapeProperty::Accessor { .. } => None,
                    });
                if prototype_shape.as_deref() == Some(base_shape) {
                    authorities.insert(authority);
                }
            }
            let well_known_symbol_prototype_matches = self
                .well_known_symbol_prototype_properties
                .keys()
                .map(|(constructor_name, _)| constructor_name)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .any(|constructor_name| {
                    self.lookup_global_property(constructor_name)
                        .and_then(|constructor| {
                            self.read_current_heap_shape_property(
                                constructor.heap_shape.as_deref()?,
                                "prototype",
                            )
                        })
                        .and_then(|prototype| match prototype {
                            ObjectShapeProperty::Data(prototype) => prototype.heap_shape,
                            ObjectShapeProperty::Accessor { .. } => None,
                        })
                        .as_deref()
                        == Some(base_shape)
                });
            if well_known_symbol_prototype_matches {
                authorities.insert(OrdinaryPropertyMutationAuthority::WellKnownSymbolPrototype);
            }
        }
        authorities
    }

    pub(super) fn record_ordinary_property_mutation_authority_effects(
        &mut self,
        referenced_name: &PropertyKeyIr,
        possible_authorities: &BTreeSet<OrdinaryPropertyMutationAuthority>,
    ) {
        if possible_authorities.contains(&OrdinaryPropertyMutationAuthority::ArrayPrototype) {
            self.array_prototype_mutated = true;
        }

        match referenced_name {
            PropertyKeyIr::StaticString(name) => {
                if possible_authorities.contains(&OrdinaryPropertyMutationAuthority::GlobalObject) {
                    self.invalidate_possible_global_property_value_info(name);
                }
                match name.as_str() {
                    "toString" => {
                        if possible_authorities
                            .contains(&OrdinaryPropertyMutationAuthority::NumberPrototype)
                        {
                            self.number_prototype_to_string_state = PrototypeToStringState::Unknown;
                        }
                        if possible_authorities
                            .contains(&OrdinaryPropertyMutationAuthority::BooleanPrototype)
                        {
                            self.boolean_prototype_to_string_state =
                                PrototypeToStringState::Unknown;
                        }
                    }
                    _ => {}
                }
            }
            PropertyKeyIr::StringExpr(key) if key.kind == ValueKind::Symbol => {
                if possible_authorities
                    .contains(&OrdinaryPropertyMutationAuthority::WellKnownSymbolPrototype)
                {
                    self.well_known_symbol_prototype_properties.clear();
                }
            }
            PropertyKeyIr::StringExpr(_) | PropertyKeyIr::ArrayIndex(_) => {
                if possible_authorities.contains(&OrdinaryPropertyMutationAuthority::GlobalObject) {
                    self.invalidate_all_possible_global_property_value_infos();
                }
                if possible_authorities
                    .contains(&OrdinaryPropertyMutationAuthority::NumberPrototype)
                {
                    self.number_prototype_to_string_state = PrototypeToStringState::Unknown;
                }
                if possible_authorities
                    .contains(&OrdinaryPropertyMutationAuthority::BooleanPrototype)
                {
                    self.boolean_prototype_to_string_state = PrototypeToStringState::Unknown;
                }
                if possible_authorities
                    .contains(&OrdinaryPropertyMutationAuthority::WellKnownSymbolPrototype)
                {
                    self.well_known_symbol_prototype_properties.clear();
                }
            }
            PropertyKeyIr::ArrayLength => {}
        }
    }

    /// Record every conservative compiler-state effect shared by a possible
    /// write through an ordinary property Reference.
    pub(super) fn record_ordinary_property_possible_write(
        &mut self,
        referenced_name: &PropertyKeyIr,
        metadata: &OrdinaryPropertyReferenceMetadata,
        intervening_user_code: bool,
        written_value_info: ValueInfo,
    ) -> bool {
        self.record_caller_flow_invalidation();
        self.record_ordinary_property_mutation_authority_effects(
            referenced_name,
            &metadata.possible_mutation_authorities,
        );
        let possible_setters =
            self.possible_ordinary_property_setters(metadata, intervening_user_code);
        let mut receiver_info = metadata.base_value_info.clone();
        if metadata.key_may_call_user_code
            || metadata.base_evaluation_may_have_intervening_effects
            || metadata.key_evaluation_may_have_intervening_effects
            || intervening_user_code
        {
            // RHS evaluation and ToNumeric/ToPrimitive preserve the captured
            // receiver identity but can mutate its contents before [[Set]].
            receiver_info.heap_shape = None;
        }
        let unknown_setter_provenance = metadata.unknown_property_hooks_possible
            || intervening_user_code
            || self.unknown_user_code_effects_observed;
        for setter in possible_setters.iter() {
            if unknown_setter_provenance {
                self.observe_unknown_property_hook(setter);
            } else {
                self.observe_ordinary_property_hook_this(setter, receiver_info.clone());
                self.merge_function_param_infos(setter, std::slice::from_ref(&written_value_info));
                if let Some(signature) = self.function_signatures.get_mut(setter) {
                    Self::merge_omitted_signature_params_as_undefined(signature, 1);
                }
            }
        }
        let setter_may_call_user_code = metadata.unknown_property_hooks_possible
            || self.source_functions_may_run(&possible_setters);
        if setter_may_call_user_code {
            self.invalidate_unknown_user_code_effects();
        }
        if let Some(base_binding_name) = metadata.base_binding_name.as_deref() {
            self.invalidate_static_boolean_alias_shapes(base_binding_name);
        }
        for receiver in &metadata.possible_receiver_values {
            self.invalidate_ordinary_property_shape_aliases(receiver);
        }
        setter_may_call_user_code
    }

    fn observe_ordinary_property_hook_this(
        &mut self,
        function_id: &FunctionId,
        receiver_info: ValueInfo,
    ) {
        let fallback = self
            .function_signature_for_current_flow(function_id)
            .map(|signature| signature.this_info.clone())
            .unwrap_or_else(|| receiver_info.clone());
        let receiver = TypedExpr::from_info(receiver_info, ExprIr::Undefined);
        let this_info =
            self.explicit_this_info_for_function_target(function_id, &receiver, fallback);
        self.merge_function_this_info(function_id, this_info);
    }

    /// Merge an unknown receiver and arguments into `function_id`: the edit
    /// [`IdempotentEdit::UnknownPropertyHook`] names.
    fn observe_unknown_property_hook(&mut self, function_id: &FunctionId) {
        if self
            .function_signatures
            .is_settled(IdempotentEdit::UnknownPropertyHook, function_id)
        {
            return;
        }
        let unknown = unknown_runtime_value_info();
        self.merge_function_this_info(function_id, unknown.clone());
        self.merge_function_param_infos(
            function_id,
            &[unknown.clone(), unknown.clone(), unknown.clone(), unknown],
        );
        if let Some(signature) = self.function_signatures.get_mut(function_id) {
            Self::merge_omitted_signature_params_as_undefined(signature, 4);
        }
    }

    /// Merge an unknown hook call into every planned source function.
    ///
    /// The merge is idempotent per signature and touches no other entry, so
    /// once applied it only needs repeating for signatures mutated since.
    pub(super) fn observe_all_planned_source_as_unknown_property_hooks(&mut self) {
        let targets = std::sync::Arc::clone(&self.analysis.planned_source_function_ids);
        let edit = IdempotentEdit::UnknownPropertyHook;
        for target in self.function_signatures.unsettled_keys(edit, &targets) {
            self.observe_unknown_property_hook(&target);
        }
        self.function_signatures.settle(edit, targets);
    }

    pub(super) fn expression_effect_accounting(
        &self,
        expression: &Expression,
    ) -> ExpressionEffectAccounting {
        match Self::unwrap_parenthesized_expr(expression) {
            Expression::Literal(_)
            | Expression::FunctionExpression(_)
            | Expression::GeneratorExpression(_)
            | Expression::AsyncFunctionExpression(_)
            | Expression::AsyncGeneratorExpression(_)
            | Expression::ArrowFunction(_)
            | Expression::AsyncArrowFunction(_)
            | Expression::RegExpLiteral(_)
            | Expression::This(_)
            | Expression::NewTarget(_) => ExpressionEffectAccounting::ProvenEffectFree,
            Expression::Identifier(identifier) => {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                if self.with_environment_chain.is_empty()
                    && (self.lookup_binding(&name).is_some()
                        || self
                            .lookup_global_property_info(&name)
                            .is_some_and(|property| {
                                property.proven_present && !self.unknown_user_code_effects_observed
                            }))
                {
                    ExpressionEffectAccounting::ProvenEffectFree
                } else {
                    ExpressionEffectAccounting::UntrackedUserCodePossible
                }
            }
            Expression::PropertyAccess(PropertyAccess::Simple(_)) | Expression::Call(_) => {
                ExpressionEffectAccounting::TrackedByLowering
            }
            Expression::Parenthesized(_) => {
                unreachable!("parenthesized expressions are unwrapped before classification")
            }
            Expression::Conditional(conditional) => self
                .expression_effect_accounting(conditional.condition())
                .merge(self.expression_effect_accounting(conditional.if_true()))
                .merge(self.expression_effect_accounting(conditional.if_false())),
            Expression::ArrayLiteral(_)
            | Expression::ObjectLiteral(_)
            | Expression::Unary(_)
            | Expression::Binary(_)
            | Expression::BinaryInPrivate(_)
            | Expression::Assign(_)
            | Expression::ClassExpression(_)
            | Expression::New(_)
            | Expression::Optional(_)
            | Expression::SuperCall(_)
            | Expression::PropertyAccess(_)
            | Expression::Await(_)
            | Expression::Yield(_)
            | Expression::ImportCall(_)
            | Expression::Spread(_)
            | Expression::ImportMeta(_)
            | Expression::FormalParameterList(_)
            | Expression::Debugger
            | Expression::TemplateLiteral(_)
            | Expression::TaggedTemplate(_)
            | Expression::Update(_) => ExpressionEffectAccounting::UntrackedUserCodePossible,
        }
    }

    pub(super) fn prepare_potentially_effectful_expression(
        &mut self,
        expression: &Expression,
    ) -> ExpressionEffectAccounting {
        let effect_accounting = self.expression_effect_accounting(expression);
        match effect_accounting {
            ExpressionEffectAccounting::ProvenEffectFree
            | ExpressionEffectAccounting::TrackedByLowering => {}
            ExpressionEffectAccounting::UntrackedUserCodePossible => {
                self.observe_all_planned_source_as_unknown_property_hooks();
                self.invalidate_unknown_user_code_effects();
            }
        }
        effect_accounting
    }

    pub(super) fn invalidate_ordinary_property_shape_aliases(&mut self, base: &ValueInfo) {
        self.record_caller_flow_invalidation();
        if base.heap_shape.is_none() {
            return;
        }

        fn canonical_function_target<'a>(
            target: &'a FunctionId,
            canonical_targets: &'a BTreeMap<FunctionId, FunctionId>,
        ) -> &'a FunctionId {
            canonical_targets.get(target).unwrap_or(target)
        }

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum ExactFunctionTargetRelation {
            Unknown,
            Disjoint,
            Overlapping,
        }

        fn exact_function_target_relation(
            left_kinds: KindSet,
            left_targets: &FunctionTargetKnowledge,
            right: &ValueInfo,
            canonical_targets: &BTreeMap<FunctionId, FunctionId>,
        ) -> ExactFunctionTargetRelation {
            let function_kind = KindSet::from_kind(ValueKind::Function);
            if left_kinds != function_kind || right.possible_kinds != function_kind {
                return ExactFunctionTargetRelation::Unknown;
            }
            let (Some(left_targets), Some(right_targets)) = (
                left_targets.exact_targets(),
                right.function_targets.exact_targets(),
            ) else {
                return ExactFunctionTargetRelation::Unknown;
            };
            if left_targets.is_empty() || right_targets.is_empty() {
                return ExactFunctionTargetRelation::Unknown;
            }
            if left_targets.iter().any(|left| {
                right_targets.iter().any(|right| {
                    canonical_function_target(left, canonical_targets)
                        == canonical_function_target(right, canonical_targets)
                })
            }) {
                ExactFunctionTargetRelation::Overlapping
            } else {
                ExactFunctionTargetRelation::Disjoint
            }
        }

        /// Whether invalidation left a shape's prototype chain unknown.
        ///
        /// A shape with `prototype: None` reads as inheriting
        /// `%Object.prototype%`, so cutting the link to the empty
        /// `%Object.prototype%` placeholder keeps its meaning. Cutting any other
        /// link would make the rest of the chain read as `%Object.prototype%`
        /// too — a TypeError whose `%Error.prototype%` was written would then
        /// resolve `toString` to `Object.prototype.toString` — so every shape
        /// whose chain passes through that link has to be dropped instead.
        #[must_use]
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum PrototypeChain {
            Described,
            Unknown,
        }

        fn invalidate_prototype_link(
            prototype: &mut Option<Arc<HeapShape>>,
            alias: &ValueInfo,
            canonical_targets: &BTreeMap<FunctionId, FunctionId>,
        ) -> PrototypeChain {
            let Some(linked) = prototype.as_mut().map(Arc::make_mut) else {
                return PrototypeChain::Described;
            };
            if alias.heap_shape.as_deref() != Some(&*linked) {
                return invalidate_nested_aliases(linked, alias, canonical_targets);
            }
            let chain = if *linked == ScriptLowerer::empty_object_shape() {
                PrototypeChain::Described
            } else {
                PrototypeChain::Unknown
            };
            *prototype = None;
            chain
        }

        fn invalidate_nested_aliases(
            shape: &mut HeapShape,
            alias: &ValueInfo,
            canonical_targets: &BTreeMap<FunctionId, FunctionId>,
        ) -> PrototypeChain {
            match shape {
                HeapShape::Object(shape) => {
                    for property in shape.properties.values_mut() {
                        if let ObjectShapeProperty::Data(value) = property {
                            invalidate_value_alias(value, alias, canonical_targets);
                        }
                    }
                    if let Some(boxed_primitive) = shape.boxed_primitive.as_deref_mut() {
                        invalidate_value_alias(boxed_primitive, alias, canonical_targets);
                    }
                    invalidate_prototype_link(&mut shape.prototype, alias, canonical_targets)
                }
                HeapShape::Array(shape) => {
                    for property in shape.properties.values_mut() {
                        if let ObjectShapeProperty::Data(value) = property {
                            invalidate_value_alias(value, alias, canonical_targets);
                        }
                    }
                    for element in &mut shape.elements {
                        invalidate_value_alias(element, alias, canonical_targets);
                    }
                    invalidate_prototype_link(&mut shape.prototype, alias, canonical_targets)
                }
            }
        }

        fn invalidate_value_alias(
            value: &mut ValueInfo,
            alias: &ValueInfo,
            canonical_targets: &BTreeMap<FunctionId, FunctionId>,
        ) {
            let Some(shape) = value.heap_shape.as_deref() else {
                return;
            };
            let function_target_relation = exact_function_target_relation(
                value.possible_kinds,
                &value.function_targets,
                alias,
                canonical_targets,
            );
            let direct_alias = value.possible_kinds.intersects(alias.possible_kinds)
                && (function_target_relation == ExactFunctionTargetRelation::Overlapping
                    || (function_target_relation == ExactFunctionTargetRelation::Unknown
                        && alias.heap_shape.as_deref() == Some(shape)));
            if direct_alias {
                value.heap_shape = None;
                return;
            }
            let shape = value
                .heap_shape
                .as_mut()
                .map(Arc::make_mut)
                .expect("checked nested alias shape must still exist");
            match invalidate_nested_aliases(shape, alias, canonical_targets) {
                PrototypeChain::Described => {}
                PrototypeChain::Unknown => value.heap_shape = None,
            }
        }

        let canonical_targets = self
            .function_signatures
            .iter()
            .map(|(target, signature)| (target.clone(), signature.id.clone()))
            .collect::<BTreeMap<_, _>>();
        let clear_if_alias_is_reachable =
            |possible_kinds: KindSet,
             function_targets: &FunctionTargetKnowledge,
             root_shape: &mut HeapShape| {
                let function_target_relation = exact_function_target_relation(
                    possible_kinds,
                    function_targets,
                    base,
                    &canonical_targets,
                );
                let direct_alias = possible_kinds.intersects(base.possible_kinds)
                    && (function_target_relation == ExactFunctionTargetRelation::Overlapping
                        || (function_target_relation == ExactFunctionTargetRelation::Unknown
                            && base.heap_shape.as_deref() == Some(&*root_shape)));
                if direct_alias {
                    return false;
                }
                match invalidate_nested_aliases(root_shape, base, &canonical_targets) {
                    PrototypeChain::Described => true,
                    PrototypeChain::Unknown => false,
                }
            };
        self.visit_live_heap_shape_roots(clear_if_alias_is_reachable);
        self.static_to_string_regexp_object_bindings.clear();
    }

    fn possible_shape_accessors(
        shape: Option<&HeapShape>,
    ) -> (BTreeSet<FunctionId>, BTreeSet<FunctionId>) {
        fn collect(
            shape: &HeapShape,
            getters: &mut BTreeSet<FunctionId>,
            setters: &mut BTreeSet<FunctionId>,
        ) {
            let (properties, prototype) = match shape {
                HeapShape::Object(shape) => (&shape.properties, shape.prototype.as_deref()),
                HeapShape::Array(shape) => (&shape.properties, shape.prototype.as_deref()),
            };
            for property in properties.values() {
                if let ObjectShapeProperty::Accessor { getter, setter } = property {
                    if let Some(getter) = getter {
                        getters.insert(getter.function_id.clone());
                    }
                    if let Some(setter) = setter {
                        setters.insert(setter.function_id.clone());
                    }
                }
            }
            if let Some(prototype) = prototype {
                collect(prototype, getters, setters);
            }
        }

        let mut getters = BTreeSet::new();
        let mut setters = BTreeSet::new();
        if let Some(shape) = shape {
            collect(shape, &mut getters, &mut setters);
        }
        (getters, setters)
    }

    pub(super) fn possible_unknown_accessor_functions(
        &self,
    ) -> (PropertyHookTargets, PropertyHookTargets) {
        let mut known_getters =
            BTreeSet::from([StandardBuiltinId::ObjectPrototypeProtoGetter.function_id()]);
        let mut known_setters =
            BTreeSet::from([StandardBuiltinId::ObjectPrototypeProtoSetter.function_id()]);

        let mut collect = |info: &ValueInfo| {
            let (shape_getters, shape_setters) =
                Self::possible_shape_accessors(info.heap_shape.as_deref());
            known_getters.extend(shape_getters);
            known_setters.extend(shape_setters);
        };
        for property in self.global_properties.values() {
            collect(&property.value_info);
        }
        for scope in &self.scopes {
            for binding in scope.values() {
                collect(&ValueInfo {
                    kind: binding.kind,
                    possible_kinds: binding.possible_kinds,
                    heap_shape: binding.heap_shape.clone(),
                    function_targets: binding.function_targets.clone(),
                });
            }
        }
        for binding in self.var_bindings.values() {
            collect(&ValueInfo {
                kind: binding.kind,
                possible_kinds: binding.possible_kinds,
                heap_shape: binding.heap_shape.clone(),
                function_targets: binding.function_targets.clone(),
            });
        }
        if let CurrentThisBinding::Activation(info) = &self.current_this_binding {
            collect(info.as_ref());
        }
        if let Some(info) = &self.current_construct_this_info {
            collect(info);
        }

        let mut getters = PropertyHookTargets::from_known(known_getters);
        let mut setters = PropertyHookTargets::from_known(known_setters);
        getters.include_all_planned_source(self.analysis.planned_source_function_ids.clone());
        setters.include_all_planned_source(self.analysis.planned_source_function_ids.clone());
        (getters, setters)
    }

    fn source_functions_may_run(&self, functions: &PropertyHookTargets) -> bool {
        functions.includes_all_planned_source()
            || functions
                .iter()
                .any(|function| self.function_may_run_user_code_synchronously(function))
    }

    pub(super) fn property_key_may_call_user_code(key: &PropertyKeyIr) -> bool {
        match key {
            PropertyKeyIr::StringExpr(expr) | PropertyKeyIr::ArrayIndex(expr) => {
                Self::value_info_may_be_object(&expr.value_info())
            }
            PropertyKeyIr::StaticString(_) | PropertyKeyIr::ArrayLength => false,
        }
    }

    fn value_info_may_be_object(info: &ValueInfo) -> bool {
        [
            ValueKind::Object,
            ValueKind::Array,
            ValueKind::Function,
            ValueKind::Arguments,
        ]
        .into_iter()
        .any(|kind| info.possible_kinds.contains(kind))
    }

    fn possible_post_set_data_value(
        &self,
        referenced_name: &PropertyKeyIr,
        metadata: &OrdinaryPropertyReferenceMetadata,
        rhs_value_info: ValueInfo,
        possible_setters: &PropertyHookTargets,
        rhs_may_have_intervening_effects: bool,
    ) -> Option<ValueInfo> {
        if metadata.base_value_info.heap_shape.is_none()
            || metadata.base_evaluation_may_have_intervening_effects
            || metadata.key_may_call_user_code
            || metadata.key_evaluation_may_have_intervening_effects
            || metadata.unknown_property_hooks_possible
            || rhs_may_have_intervening_effects
            || self.unknown_user_code_effects_observed
            || !possible_setters.is_empty()
        {
            return None;
        }

        let prior_property = |name: &str| {
            self.read_current_heap_shape_property(
                metadata.base_value_info.heap_shape.as_deref()?,
                name,
            )
        };

        match referenced_name {
            PropertyKeyIr::StaticString(name) => match prior_property(name) {
                Some(ObjectShapeProperty::Data(previous)) => {
                    Some(self.merge_value_infos(previous, rhs_value_info))
                }
                Some(ObjectShapeProperty::Accessor { .. }) => None,
                // Absent own data does not prove a successful write: an
                // inherited non-writable descriptor may retain its old value.
                None => None,
            },
            PropertyKeyIr::StringExpr(key) if key.kind == ValueKind::Symbol => {
                let ExprIr::WellKnownSymbol(symbol) = &key.expr else {
                    return None;
                };
                let previous = prior_property(&shape_namespace_key(*symbol));
                match previous {
                    Some(ObjectShapeProperty::Data(previous)) => {
                        Some(self.merge_value_infos(previous, rhs_value_info))
                    }
                    Some(ObjectShapeProperty::Accessor { .. }) => None,
                    None => None,
                }
            }
            PropertyKeyIr::StringExpr(_)
            | PropertyKeyIr::ArrayIndex(_)
            | PropertyKeyIr::ArrayLength => None,
        }
    }

    fn pending_ordinary_property_publication(
        &mut self,
        target: &Expression,
        referenced_name: &PropertyKeyIr,
        possible_post_set_value: ValueInfo,
        possible_mutation_authorities: &BTreeSet<OrdinaryPropertyMutationAuthority>,
    ) -> Option<PendingOrdinaryPropertyPublication> {
        let (root_name, mut path) = self.binding_shape_path(target)?;

        if let PropertyKeyIr::StringExpr(key) = referenced_name {
            let ExprIr::WellKnownSymbol(symbol) = &key.expr else {
                return None;
            };
            let symbol = *symbol;
            if path.as_slice() != [PropertyKeyIr::StaticString("prototype".to_string())]
                || self.lookup_binding(&root_name).is_some()
            {
                return None;
            }
            let constructor = self.lookup_global_property_info(&root_name)?;
            if !constructor.proven_present || constructor.source != GlobalPropertySource::Builtin {
                return None;
            }
            let mut constructor_value_info = constructor.value_info.clone();
            let HeapShape::Object(constructor_shape) = constructor_value_info
                .heap_shape
                .as_mut()
                .map(Arc::make_mut)?
            else {
                return None;
            };
            let Some(ObjectShapeProperty::Data(prototype)) =
                constructor_shape.properties.get_mut("prototype")
            else {
                return None;
            };
            let prototype_properties = match prototype.heap_shape.as_mut().map(Arc::make_mut)? {
                HeapShape::Object(shape) => &mut shape.properties,
                HeapShape::Array(shape) => &mut shape.properties,
            };
            let property_name = shape_namespace_key(symbol);
            if !matches!(
                prototype_properties.get(&property_name),
                Some(ObjectShapeProperty::Accessor { .. })
            ) {
                prototype_properties.insert(
                    property_name,
                    ObjectShapeProperty::Data(possible_post_set_value.clone()),
                );
            }
            let primitive_receiver_kind = match root_name.as_str() {
                STRING_NAME => Some(ValueKind::String),
                NUMBER_NAME => Some(ValueKind::Number),
                BOOLEAN_NAME => Some(ValueKind::Boolean),
                BIGINT_NAME => Some(ValueKind::BigInt),
                SYMBOL_NAME => Some(ValueKind::Symbol),
                _ => None,
            };
            return Some(
                PendingOrdinaryPropertyPublication::WellKnownSymbolPrototype {
                    constructor_name: root_name,
                    constructor_value_info,
                    primitive_receiver_kind,
                    symbol,
                    value: possible_post_set_value,
                },
            );
        }

        let PropertyKeyIr::StaticString(_) = referenced_name else {
            return None;
        };
        if root_name == LEXICAL_THIS_NAME {
            path.push(referenced_name.clone());
            let value_info = Self::apply_shape_write(
                self.current_construct_this_info.clone()?,
                path.as_slice(),
                possible_post_set_value,
            );
            value_info.heap_shape.as_ref()?;
            return Some(PendingOrdinaryPropertyPublication::CurrentThisShape { value_info });
        }
        let targets_intrinsic_prototype = path.as_slice()
            == [PropertyKeyIr::StaticString("prototype".to_string())]
            && self.lookup_binding(&root_name).is_none()
            && self
                .lookup_global_property_info(&root_name)
                .is_some_and(|property| {
                    property.proven_present && property.source == GlobalPropertySource::Builtin
                });
        if !path.is_empty() && !targets_intrinsic_prototype {
            return None;
        }
        if path.is_empty() && !possible_mutation_authorities.is_empty() {
            return None;
        }
        path.push(referenced_name.clone());

        let root_value_info = self
            .lookup_binding(&root_name)
            .map(|binding| ValueInfo {
                kind: binding.kind,
                possible_kinds: binding.possible_kinds,
                heap_shape: binding.heap_shape.clone(),
                function_targets: binding.function_targets.clone(),
            })
            .or_else(|| {
                self.var_bindings.get(&root_name).map(|binding| ValueInfo {
                    kind: binding.kind,
                    possible_kinds: binding.possible_kinds,
                    heap_shape: binding.heap_shape.clone(),
                    function_targets: binding.function_targets.clone(),
                })
            })
            .or_else(|| self.lookup_global_property(&root_name))?;
        let root_value_info =
            Self::apply_shape_write(root_value_info, path.as_slice(), possible_post_set_value);
        root_value_info.heap_shape.as_ref()?;

        if targets_intrinsic_prototype {
            Some(
                PendingOrdinaryPropertyPublication::IntrinsicPrototypeShape {
                    constructor_name: root_name,
                    constructor_value_info: root_value_info,
                },
            )
        } else {
            Some(PendingOrdinaryPropertyPublication::BindingShape {
                root_name,
                root_value_info,
            })
        }
    }

    fn publish_ordinary_property_fact(&mut self, publication: PendingOrdinaryPropertyPublication) {
        let (root_name, root_value_info) = match publication {
            PendingOrdinaryPropertyPublication::CurrentThisShape { value_info } => {
                self.current_construct_this_info = Some(value_info.clone());
                if let CurrentThisBinding::Activation(current) = &mut self.current_this_binding {
                    *current = std::sync::Arc::new(value_info);
                }
                return;
            }
            PendingOrdinaryPropertyPublication::BindingShape {
                root_name,
                root_value_info,
            } => (root_name, root_value_info),
            PendingOrdinaryPropertyPublication::IntrinsicPrototypeShape {
                constructor_name,
                constructor_value_info,
            } => {
                if let Some(constructor) = self.global_properties.get_mut(&constructor_name) {
                    constructor.value_info = constructor_value_info;
                    constructor.proven_present = true;
                }
                return;
            }
            PendingOrdinaryPropertyPublication::WellKnownSymbolPrototype {
                constructor_name,
                constructor_value_info,
                primitive_receiver_kind,
                symbol,
                value,
            } => {
                if let Some(constructor) = self.global_properties.get_mut(&constructor_name) {
                    constructor.value_info = constructor_value_info;
                    constructor.proven_present = true;
                }
                let function_targets = value
                    .function_targets
                    .known_targets()
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>();
                self.well_known_symbol_prototype_properties
                    .insert((constructor_name, symbol), value);
                if let Some(receiver_kind) = primitive_receiver_kind {
                    let receiver =
                        TypedExpr::from_info(ValueInfo::new(receiver_kind), ExprIr::Undefined);
                    for function_id in function_targets {
                        let fallback = self
                            .function_signature_for_current_flow(&function_id)
                            .map(|signature| signature.this_info.clone())
                            .unwrap_or_else(|| receiver.value_info());
                        let this_info = self.explicit_this_info_for_function_target(
                            &function_id,
                            &receiver,
                            fallback,
                        );
                        self.merge_function_this_info(&function_id, this_info);
                    }
                }
                return;
            }
        };
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(&root_name) {
                binding.kind = root_value_info.kind;
                binding.possible_kinds = root_value_info.possible_kinds;
                binding.heap_shape = root_value_info.heap_shape;
                binding.function_targets = root_value_info.function_targets;
                return;
            }
        }

        let is_script_global = self
            .var_bindings
            .get(&root_name)
            .is_some_and(|binding| binding.is_script_global);
        if let Some(binding) = self.var_bindings.get_mut(&root_name) {
            binding.kind = root_value_info.kind;
            binding.possible_kinds = root_value_info.possible_kinds;
            binding.heap_shape = root_value_info.heap_shape.clone();
            binding.function_targets = root_value_info.function_targets.clone();
            if !is_script_global {
                return;
            }
        }
        if let Some(property) = self.global_properties.get_mut(&root_name) {
            property.value_info = root_value_info;
            property.proven_present = true;
        }
    }

    /// Lower a source-level plain assignment into one retained ordinary
    /// property Reference. Base and raw key are lowered before the RHS; the
    /// carrier leaves ToObject, ToPropertyKey, and Set to its backend consumer.
    pub(super) fn lower_ordinary_property_plain_assignment(
        &mut self,
        access: &boa_ast::expression::access::SimplePropertyAccess,
        rhs: &Expression,
    ) -> TypedExpr {
        let (plan, referenced_name, metadata) = self.lower_ordinary_property_reference_plan(access);
        let rhs_effect_accounting = self.prepare_potentially_effectful_expression(rhs);
        let before_rhs_effect_epoch = self.intervening_effect_epoch;
        let rhs_value = self.lower_expression(rhs);
        let rhs_may_have_intervening_effects = rhs_effect_accounting
            .intervening_effects_observed(before_rhs_effect_epoch, self.intervening_effect_epoch);
        if rhs_may_have_intervening_effects {
            self.observe_all_planned_source_as_unknown_property_hooks();
            self.invalidate_unknown_user_code_effects();
        }
        self.complete_ordinary_property_plain_assignment(
            access,
            plan,
            referenced_name,
            metadata,
            rhs_value,
            rhs_may_have_intervening_effects,
        )
    }

    /// Complete a prepared Reference with its actual RHS value. The expression
    /// producer accounts for RHS effects; an iterator-head sink is an eager,
    /// effect-free value already observed by IteratorValue.
    pub(super) fn complete_ordinary_property_plain_assignment(
        &mut self,
        access: &boa_ast::expression::access::SimplePropertyAccess,
        plan: OrdinaryPropertyReferencePlan,
        referenced_name: PropertyKeyIr,
        metadata: OrdinaryPropertyReferenceMetadata,
        rhs_value: TypedExpr,
        rhs_may_have_intervening_effects: bool,
    ) -> TypedExpr {
        let written_value_info = rhs_value.value_info();
        let possible_setters =
            self.possible_ordinary_property_setters(&metadata, rhs_may_have_intervening_effects);
        let possible_post_set_value = self.possible_post_set_data_value(
            &referenced_name,
            &metadata,
            written_value_info.clone(),
            &possible_setters,
            rhs_may_have_intervening_effects,
        );
        let pending_publication = possible_post_set_value.clone().and_then(|value| {
            self.pending_ordinary_property_publication(
                access.target(),
                &referenced_name,
                value,
                &metadata.possible_mutation_authorities,
            )
        });

        self.record_ordinary_property_possible_write(
            &referenced_name,
            &metadata,
            rhs_may_have_intervening_effects,
            written_value_info,
        );
        if let Some(publication) = pending_publication {
            self.publish_ordinary_property_fact(publication);
        } else if let Some(value) = possible_post_set_value {
            self.update_well_known_symbol_prototype_property(
                access.target(),
                &referenced_name,
                Some(&value),
            );
        }
        plan.plain_assignment(rhs_value, possible_setters)
    }

    /// Lower one ordinary property Reference directly into its fused eager
    /// mutation carrier. The base and raw computed-key expression are lowered
    /// before the RHS; their runtime GetValue/PutValue staging remains owned by
    /// the carrier's single backend consumer.
    pub(super) fn lower_ordinary_property_eager_compound_assignment(
        &mut self,
        access: &boa_ast::expression::access::SimplePropertyAccess,
        op: EagerCompoundAssignmentOp,
        rhs: &Expression,
    ) -> TypedExpr {
        let (plan, referenced_name, metadata) = self.lower_ordinary_property_reference_plan(access);
        self.record_ordinary_property_get(&metadata);
        let possible_getters = Self::possible_ordinary_property_getters(&metadata);
        let rhs_effect_accounting = self.prepare_potentially_effectful_expression(rhs);
        let before_rhs_effect_epoch = self.intervening_effect_epoch;
        let rhs = self.lower_expression(rhs);
        let rhs_may_have_intervening_effects = rhs_effect_accounting
            .intervening_effects_observed(before_rhs_effect_epoch, self.intervening_effect_epoch);
        if rhs_may_have_intervening_effects {
            self.observe_all_planned_source_as_unknown_property_hooks();
            self.invalidate_unknown_user_code_effects();
        }
        // The old property value is coerced only after the RHS has been
        // evaluated. Exact numeric builtin getters discharge that effect;
        // every other result still admits source `valueOf` or @@toPrimitive.
        let coercion_may_call_user_code =
            self.ordinary_property_numeric_coercion_may_call_user_code(&metadata);
        let possible_setters =
            self.possible_ordinary_property_setters(&metadata, coercion_may_call_user_code);
        let old_value_binding = self.alloc_temp_binding_name("ordinary.property.compound.old.");
        let result = plan.eager_compound_assignment(
            old_value_binding,
            op,
            rhs,
            possible_getters,
            possible_setters,
        );
        self.record_ordinary_property_possible_write(
            &referenced_name,
            &metadata,
            coercion_may_call_user_code,
            result.value_info(),
        );
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};

    fn lower(source: &str) -> ProgramIr {
        let source = parse(source, ParseOptions::script()).expect("script should parse");
        crate::lower(&source)
    }

    fn assert_last_expression_is_coercive_add(source: &str, failure_message: &str) {
        let program = lower(source);
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "{failure_message}: {:?}",
            result.expr
        );
    }

    fn returned_assignment<'a>(
        script: &'a ScriptIr,
        function_name: &str,
    ) -> &'a OrdinaryPropertyEagerCompoundAssignmentIr {
        let function = script
            .functions
            .iter()
            .find(|function| function.name == function_name)
            .unwrap_or_else(|| panic!("missing function {function_name}"));
        let StatementIr::Return(value) = function
            .body
            .statements
            .iter()
            .find(|statement| matches!(statement, StatementIr::Return(_)))
            .expect("function should return its assignment")
        else {
            unreachable!("selected statement is a return")
        };
        let ExprIr::OrdinaryPropertyEagerCompoundAssignment(assignment) = &value.expr else {
            panic!(
                "expected fused ordinary property assignment, got {:?}",
                value.expr
            );
        };
        assignment
    }

    fn returned_plain_assignment<'a>(
        script: &'a ScriptIr,
        function_name: &str,
    ) -> &'a OrdinaryPropertyAssignmentIr {
        let function = script
            .functions
            .iter()
            .find(|function| function.name == function_name)
            .unwrap_or_else(|| panic!("missing function {function_name}"));
        let StatementIr::Return(value) = function
            .body
            .statements
            .iter()
            .find(|statement| matches!(statement, StatementIr::Return(_)))
            .expect("function should return its assignment")
        else {
            unreachable!("selected statement is a return")
        };
        let ExprIr::OrdinaryPropertyAssignment(assignment) = &value.expr else {
            panic!(
                "expected fused ordinary property plain assignment, got {:?}",
                value.expr
            );
        };
        assignment
    }

    fn applied_lhs(result: &TypedExpr) -> &TypedExpr {
        match &result.expr {
            ExprIr::CoerciveAdd { lhs, .. }
            | ExprIr::CoerciveBinaryNumber { lhs, .. }
            | ExprIr::BitwiseNumeric { lhs, .. } => lhs,
            other => panic!("unexpected eager operation {other:?}"),
        }
    }

    #[test]
    fn intrinsic_property_installation_retains_the_strict_this_operand() {
        let program = lower(
            "String.prototype.q = function stringQ() { 'use strict'; return this === 'z'; }; 'z'?.q();",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let function = script
            .functions
            .iter()
            .find(|function| function.name == "stringQ")
            .expect("installed function should be lowered");
        let this_operand = function.body.statements.iter().find_map(|statement| {
            let StatementIr::Return(TypedExpr {
                expr:
                    ExprIr::SpecOperation {
                        operation: SpecOperationIr::StrictEqualityComparison,
                        operands,
                    },
                ..
            }) = statement
            else {
                return None;
            };
            operands
                .iter()
                .find(|operand| matches!(operand.expr, ExprIr::This))
        });

        assert!(function.strict);
        let this_operand = this_operand.expect("strict comparison retains its this operand");
        // A new property can reach an inherited setter. Its possible calls
        // widen the function signature, but never replace the strict This
        // operand with a primitive wrapper or an assumed receiver.
        assert!(this_operand.possible_kinds.contains(ValueKind::String));
        let StatementIr::Expression(TypedExpr {
            expr: ExprIr::OrdinaryPropertyAssignment(assignment),
            ..
        }) = &script.body.statements[0]
        else {
            panic!("method installation must retain its property Reference");
        };
        assert_eq!(
            assignment.rhs().function_targets.exact_single_target(),
            Some(&function.id)
        );
    }

    #[test]
    fn intrinsic_method_transfer_retains_the_property_read_after_observable_put() {
        let program = lower(
            "var value = new Object(); value.transferred = Boolean.prototype.toString; value.transferred();",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(expression) = script.body.statements.last().unwrap() else {
            panic!("expected transferred method call");
        };
        let ExprIr::MaterializeBinding { body: call, .. } = &expression.expr else {
            panic!(
                "expected acquired-callee materialization: {:?}",
                expression.expr
            );
        };
        let ExprIr::CallIndirect {
            callee,
            this_arg: Some(receiver),
            ..
        } = &call.expr
        else {
            panic!("expected indirect transferred method call: {:?}", call.expr);
        };

        // PutValue can reach a prototype setter. The later property read must
        // remain live rather than assume it still returns the earlier method.
        assert_eq!(call.kind, ValueKind::Dynamic);
        assert_eq!(call.possible_kinds, KindSet::all_runtime_tags());
        assert!(matches!(
            callee.function_targets,
            FunctionTargetKnowledge::Open(_)
        ));
        assert!(matches!(&callee.expr, ExprIr::SpecOperation {
            operation: SpecOperationIr::GetV, operands
        } if matches!(operands.as_slice(), [target, key] if target.expr == receiver.expr && matches!(&key.expr, ExprIr::String(key) if key == "transferred"))));
    }

    #[test]
    fn ordinary_property_eager_compound_assignment_owns_one_reference() {
        let program = lower(
            r#"
            function add(base, key, rhs) { "use strict"; return base[key] += rhs; }
            function multiply(base, key, rhs) { "use strict"; return base[key] *= rhs; }
            function xor(base, key, rhs) { "use strict"; return base[key] ^= rhs; }
            function exponentiate(base, key, rhs) { "use strict"; return base[key] **= rhs; }
            "#,
        );
        let script = program.script.as_ref().expect("script IR should exist");

        for name in ["add", "multiply", "xor", "exponentiate"] {
            let assignment = returned_assignment(&script, name);
            assert!(matches!(
                &assignment.base_and_receiver().expr,
                ExprIr::Identifier(_)
            ));
            assert!(matches!(
                assignment.referenced_name(),
                PropertyKeyIr::StringExpr(key)
                    if matches!(&key.expr, ExprIr::Identifier(_))
            ));
            assert_eq!(assignment.strictness(), Strictness::Strict);
            assert!(assignment
                .old_value_binding()
                .starts_with("$ordinary.property.compound.old."));
            assert!(matches!(
                &applied_lhs(assignment.result()).expr,
                ExprIr::Identifier(binding) if binding == assignment.old_value_binding()
            ));
        }

        assert!(matches!(
            &returned_assignment(&script, "add").result().expr,
            ExprIr::CoerciveAdd { .. }
        ));
        assert!(matches!(
            &returned_assignment(&script, "multiply").result().expr,
            ExprIr::CoerciveBinaryNumber {
                op: ArithmeticBinaryOp::Mul,
                ..
            }
        ));
        assert!(matches!(
            &returned_assignment(&script, "xor").result().expr,
            ExprIr::BitwiseNumeric {
                op: BitwiseBinaryOp::Xor,
                ..
            }
        ));
        assert!(matches!(
            &returned_assignment(&script, "exponentiate").result().expr,
            ExprIr::CoerciveBinaryNumber {
                op: ArithmeticBinaryOp::Exp,
                ..
            }
        ));
    }

    #[test]
    fn ordinary_property_plain_assignment_retains_base_key_rhs_and_strictness() {
        let program = lower(
            r#"
            function computed(base, key, rhs) {
                "use strict";
                return base()[key()] = rhs();
            }
            function named(base, rhs) {
                return base.prop = rhs();
            }
            function nullBase(rhs) {
                let base = null;
                return base.prop = rhs();
            }
            function undefinedBase(rhs) {
                let base = undefined;
                return base.prop = rhs();
            }
            "#,
        );
        let script = program.script.as_ref().expect("script IR should exist");

        let computed = returned_plain_assignment(script, "computed");
        assert!(matches!(
            &computed.base_and_receiver().expr,
            ExprIr::CallIndirect { .. }
        ));
        assert!(matches!(
            computed.referenced_name(),
            PropertyKeyIr::StringExpr(key)
                if matches!(&key.expr, ExprIr::CallIndirect { .. })
        ));
        assert!(matches!(&computed.rhs().expr, ExprIr::CallIndirect { .. }));
        assert_eq!(computed.strictness(), Strictness::Strict);

        let named = returned_plain_assignment(script, "named");
        assert!(matches!(
            named.referenced_name(),
            PropertyKeyIr::StaticString(name) if name == "prop"
        ));
        assert!(matches!(&named.rhs().expr, ExprIr::CallIndirect { .. }));
        assert_eq!(named.strictness(), Strictness::Sloppy);

        let null_base = returned_plain_assignment(script, "nullBase");
        assert_eq!(null_base.base_and_receiver().kind, ValueKind::Null);
        assert!(matches!(&null_base.rhs().expr, ExprIr::CallIndirect { .. }));

        let undefined_base = returned_plain_assignment(script, "undefinedBase");
        assert_eq!(
            undefined_base.base_and_receiver().kind,
            ValueKind::Undefined
        );
        assert!(matches!(
            &undefined_base.rhs().expr,
            ExprIr::CallIndirect { .. }
        ));
    }

    #[test]
    fn plain_assignment_does_not_assume_a_sloppy_set_succeeds() {
        let program = lower(
            "globalThis.plainWritableFact = 1; Object.defineProperty(globalThis, 'plainWritableFact', { value: 1, writable: false }); globalThis.plainWritableFact = 's'; globalThis.plainWritableFact + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "a possibly failed sloppy Set cannot publish the RHS as a fact: {:?}",
            result.expr
        );
    }

    #[test]
    fn new_function_properties_do_not_prove_effect_free_inherited_writes() {
        let program = lower(
            "function changed() {} function preserved() {} changed.value = 1; preserved.value = 1; changed.marker = 0; preserved.value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        let ExprIr::CoerciveAdd { lhs, rhs } = &result.expr else {
            panic!("inherited setters can change either function's property: {result:?}");
        };
        assert!(matches!(rhs.expr, ExprIr::Number(bits) if bits == 1.0_f64.to_bits()));
        let ExprIr::SpecOperation {
            operation: SpecOperationIr::GetV,
            operands,
        } = &lhs.expr
        else {
            panic!("the preserved function's value must still be read at runtime: {lhs:?}");
        };
        assert!(matches!(operands.as_slice(), [target, key]
            if matches!(&target.expr, ExprIr::GlobalIdentifierRead { name } if name == "preserved")
                && matches!(&key.expr, ExprIr::String(name) if name == "value")));
    }

    #[test]
    fn function_property_write_invalidates_a_true_alias_shape() {
        let program = lower(
            "function target() {} target.value = 1; let alias = target; target.marker = 0; alias.value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "a true function alias must not retain the pre-write numeric shape: {:?}",
            result.expr
        );
    }

    #[test]
    fn ordinary_property_write_invalidates_nested_object_and_array_alias_shapes() {
        for source in [
            "let target = { p: 0 }; let holder = { alias: target }; target.p ||= 's'; holder.alias.p + 1;",
            "let target = { p: 0 }; let holder = [target]; target.p ||= 's'; holder[0].p + 1;",
        ] {
            let program = lower(source);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let script = program.script.as_ref().expect("script IR should exist");
            let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
                panic!("expected follow-up addition");
            };
            assert!(
                matches!(result.expr, ExprIr::CoerciveAdd { .. }),
                "a nested alias must not retain the pre-write Number shape: {:?}",
                result.expr
            );
        }
    }

    #[test]
    fn nested_conditional_property_write_invalidates_every_receiver_shape() {
        let program = lower(
            "function write(first, second) { let left = { p: 1, leftOnly: 0 }; let middle = { p: 1, middleOnly: 0 }; let right = { p: 1, rightOnly: 0 }; (first ? left : second ? middle : right).p = 's'; left.p + 1; middle.p + 1; return right.p + 1; } write(true, false);",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let function = script
            .functions
            .iter()
            .find(|function| function.name == "write")
            .expect("write function should be lowered");

        for statement in function.body.statements.iter().rev().take(3) {
            let result = match statement {
                StatementIr::Expression(result) | StatementIr::Return(result) => result,
                _ => panic!("expected follow-up addition"),
            };
            assert!(
                matches!(result.expr, ExprIr::CoerciveAdd { .. }),
                "every possible receiver must lose its pre-write Number shape: {:?}",
                result.expr
            );
        }
    }

    #[test]
    fn conditional_logical_write_invalidates_both_receiver_shapes() {
        let program = lower(
            "function write(flag) { let left = { p: 0, leftOnly: 0 }; let right = { p: 0, rightOnly: 0 }; (flag ? left : right).p ||= 's'; left.p + 1; return right.p + 1; } write(true);",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let function = script
            .functions
            .iter()
            .find(|function| function.name == "write")
            .expect("write function should be lowered");

        for statement in function.body.statements.iter().rev().take(2) {
            let result = match statement {
                StatementIr::Expression(result) | StatementIr::Return(result) => result,
                _ => panic!("expected follow-up addition"),
            };
            assert!(
                matches!(result.expr, ExprIr::CoerciveAdd { .. }),
                "both possible receivers must lose their pre-write Number shape: {:?}",
                result.expr
            );
        }
    }

    #[test]
    fn conditional_ordinary_object_write_preserves_number_prototype_fact() {
        let program = lower(
            "function write(flag) { let left = { p: 0, leftOnly: 0 }; let right = { p: 0, rightOnly: 0 }; (flag ? left : right).p = 's'; return (1).toString(); } write(true);",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let function = script
            .functions
            .iter()
            .find(|function| function.name == "write")
            .expect("write function should be lowered");
        let StatementIr::Return(result) = function.body.statements.last().unwrap() else {
            panic!("expected return addition");
        };

        let ExprIr::MaterializeBinding { value, body, .. } = &result.expr else {
            panic!("the Number receiver must remain evaluated once: {result:?}");
        };
        assert!(matches!(value.expr, ExprIr::Number(bits) if bits == 1.0_f64.to_bits()));
        let ExprIr::CallIndirect {
            callee,
            this_arg: Some(receiver),
            args,
            ..
        } = &body.expr
        else {
            panic!("the actual acquired Number method must be called: {body:?}");
        };
        assert_eq!(body.kind, ValueKind::String);
        assert_eq!(
            callee.function_targets.exact_single_target(),
            Some(&StandardBuiltinId::NumberPrototypeToString.function_id())
        );
        assert!(matches!(&callee.expr, ExprIr::PropertyRead { target, key }
            if target.expr == receiver.expr
                && matches!(key, PropertyKeyIr::StaticString(name) if name == "toString")));
        assert_eq!(receiver.kind, ValueKind::Number);
        assert!(args.is_empty());
    }

    #[test]
    fn conditional_prototype_write_invalidates_both_inheriting_shapes() {
        let program = lower(
            "function write(flag) { let leftPrototype = { p: 1, leftOnly: 0 }; let rightPrototype = { p: 1, rightOnly: 0 }; let left = Object.create(leftPrototype); let right = Object.create(rightPrototype); (flag ? leftPrototype : rightPrototype).p = 's'; left.p + 1; return right.p + 1; } write(true);",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let function = script
            .functions
            .iter()
            .find(|function| function.name == "write")
            .expect("write function should be lowered");

        for statement in function.body.statements.iter().rev().take(2) {
            let result = match statement {
                StatementIr::Expression(result) | StatementIr::Return(result) => result,
                _ => panic!("expected follow-up addition"),
            };
            assert!(
                matches!(result.expr, ExprIr::CoerciveAdd { .. }),
                "an inherited lookup must not keep a stale prototype property shape: {:?}",
                result.expr
            );
        }
    }

    #[test]
    fn nested_property_write_does_not_restore_a_stale_sibling_alias() {
        let program = lower(
            "let target = { p: 1 }; let root = { a: target, b: target }; root.a.p = 's'; root.b.p + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "publishing the written path must not restore a sibling alias: {:?}",
            result.expr
        );
    }

    #[test]
    fn well_known_symbol_setter_is_observed_before_plain_assignment_publication() {
        let program = lower(
            "let outcome = 1; let target = { set [Symbol.iterator](value) { outcome = 'setter'; } }; target[Symbol.iterator] = 0; outcome + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let setter = script
            .functions
            .iter()
            .find(|function| function.protocol == FunctionProtocolIr::ObjectSetter)
            .expect("computed setter should be lowered");
        let StatementIr::Expression(write) = &script.body.statements[2] else {
            panic!("expected property assignment");
        };
        let ExprIr::OrdinaryPropertyAssignment(assignment) = &write.expr else {
            panic!("expected ordinary property assignment: {write:?}");
        };
        assert!(
            assignment.possible_setters().contains(&setter.id),
            "the exact computed Symbol setter must be retained on the Reference: {assignment:?}"
        );
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "a computed Symbol setter must invalidate captured binding facts: {:?}",
            result.expr
        );
    }

    #[test]
    fn well_known_symbol_assignment_retains_a_non_writable_intrinsic_value_as_possible() {
        let program = lower(
            "Symbol.prototype[Symbol.toPrimitive] = function replacement() { return 1; }; Symbol.prototype[Symbol.toPrimitive];",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up property read");
        };
        assert!(
            result
                .function_targets
                .exact_targets()
                .is_some_and(|targets| {
                    targets.contains(&StandardBuiltinId::SymbolPrototypeToPrimitive.function_id())
                }),
            "a possibly failed sloppy Set must retain the old intrinsic target: {:?}",
            result.function_targets
        );
        assert_eq!(
            result
                .function_targets
                .exact_targets()
                .expect("both retained targets must be exhaustive")
                .len(),
            2,
            "the possible successful Set must retain the replacement target too"
        );
    }

    #[test]
    fn ordinary_property_mutation_hooks_contribute_arbitrary_catch_values() {
        let program = lower(
            r#"
            var target = {
                get p() { throw 'getter'; },
                set p(value) { throw 'setter'; }
            };
            function catchPlain() {
                "use strict";
                try { target.p = 1; } catch (error) { return typeof error; }
            }
            function catchEager() {
                "use strict";
                try { target.p += 1; } catch (error) { return typeof error; }
            }
            function catchNumeric() {
                "use strict";
                try { target.p++; } catch (error) { return typeof error; }
            }
            "#,
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");

        for function_name in ["catchPlain", "catchEager", "catchNumeric"] {
            let function = script
                .functions
                .iter()
                .find(|function| function.name == function_name)
                .unwrap_or_else(|| panic!("missing function {function_name}"));
            let catch_block = function
                .body
                .statements
                .iter()
                .find_map(|statement| match statement {
                    StatementIr::TryCatch { catch_block, .. } => Some(catch_block),
                    _ => None,
                })
                .expect("expected try/catch");
            let StatementIr::Return(result) = &catch_block.statements[0] else {
                panic!("catch should return typeof error");
            };
            let ExprIr::TypeOf { expr: caught } = &result.expr else {
                panic!(
                    "arbitrary thrown values must prevent typeof folding: {:?}",
                    result.expr
                );
            };
            assert_eq!(caught.kind, ValueKind::Dynamic);
            assert_eq!(caught.possible_kinds, KindSet::all_runtime_tags());
        }
    }

    #[test]
    fn ordinary_setter_observes_the_published_value_after_a_direct_call() {
        let program = lower(
            r#"
            let prototype = {
                set value(value) { return value + 1; }
            };
            let setter = Object.getOwnPropertyDescriptor(prototype, 'value').set;
            setter(1);
            let target = { __proto__: prototype };
            target.value = 'published';
            "#,
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let setters = script
            .functions
            .iter()
            .filter(|function| function.name.contains("value") && function.params.len() == 1)
            .collect::<Vec<_>>();
        assert!(
            setters
                .iter()
                .any(|setter| setter.params[0].kind == ValueKind::Dynamic),
            "setter signatures: {:?}",
            setters
                .iter()
                .map(|setter| (&setter.id, setter.params[0].kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn lost_proxy_hook_observation_stays_unknown_after_a_direct_call() {
        let program = lower(
            r#"
            function trap(target, key, value, receiver) { return target; }
            let handler = globalThis.choose ? { set: trap } : {};
            let proxy = new Proxy({}, handler);
            proxy.value = 'published';
            trap(1, 2, 3, 4);
            "#,
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let traps = script
            .functions
            .iter()
            .filter(|function| function.name == "trap")
            .collect::<Vec<_>>();
        assert!(
            traps.iter().any(|trap| {
                trap.params.len() == 4
                    && trap
                        .params
                        .iter()
                        .all(|param| param.kind == ValueKind::Dynamic)
            }),
            "trap signatures: {:?}",
            traps
                .iter()
                .map(|trap| {
                    (
                        &trap.id,
                        trap.params
                            .iter()
                            .map(|param| param.kind)
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn sloppy_primitive_property_hooks_observe_boxed_receivers() {
        for (source, function_name) in [
            (
                "Object.defineProperty(Number.prototype, 'compoundPrimitiveGetter', { configurable: true, get: function sloppyPrimitiveGetter() { return this + 1; } }); (1).compoundPrimitiveGetter += 2;",
                "sloppyPrimitiveGetter",
            ),
            (
                "Object.defineProperty(Number.prototype, 'compoundPrimitiveSetter', { configurable: true, set: function sloppyPrimitiveSetter(value) { return this + value; } }); (1).compoundPrimitiveSetter = 2;",
                "sloppyPrimitiveSetter",
            ),
        ] {
            let program = lower(source);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let script = program.script.as_ref().expect("script IR should exist");
            let hook = script
                .functions
                .iter()
                .find(|function| function.name == function_name)
                .unwrap_or_else(|| panic!("missing {function_name}"));
            let StatementIr::Return(result) = hook
                .body
                .statements
                .iter()
                .find(|statement| matches!(statement, StatementIr::Return(_)))
                .expect("hook should return a value")
            else {
                unreachable!("selected statement is a return")
            };
            assert!(
                matches!(result.expr, ExprIr::CoerciveAdd { .. }),
                "sloppy primitive hook receiver must be boxed: {:?}",
                result.expr
            );
        }
    }

    #[test]
    fn property_delete_invalidates_a_cached_function_return_shape() {
        let program = lower(
            "let target = { value: 1 }; function readTarget() { return target; } readTarget(); delete target.value; readTarget().value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "delete must prevent a cached return shape from restoring the property: {:?}",
            result.expr
        );
    }

    #[test]
    fn called_callback_invalidates_later_function_return_shape_in_the_same_body() {
        let program = lower(
            "let target = { value: 1 }; \
             function readTarget() { return target; } \
             function invoke(callback) { callback(); return readTarget().value + 1; } \
             invoke(function () { delete target.value; });",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let invoke = script
            .functions
            .iter()
            .find(|function| function.name == "invoke")
            .expect("invoke should be lowered");
        let StatementIr::Return(result) = invoke
            .body
            .statements
            .iter()
            .find(|statement| matches!(statement, StatementIr::Return(_)))
            .expect("invoke should return the addition")
        else {
            unreachable!("selected statement is a return")
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "the callback must prevent a cached return shape from proving a numeric property: {:?}",
            result.expr
        );
    }

    #[test]
    fn static_class_block_invalidates_later_function_return_shape() {
        let program = lower(
            "let target = { value: 1 }; \
             function readTarget() { return target; } \
             class Example { static { delete target.value; } } \
             readTarget().value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "the static block must prevent a cached return shape from proving a numeric property: {:?}",
            result.expr
        );
    }

    #[test]
    fn static_class_block_invalidates_a_captured_binding_shape() {
        let program = lower(
            "let target = { value: 1 }; \
             class Example { static { delete target.value; } } \
             target.value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "the static block must invalidate the captured property shape: {:?}",
            result.expr
        );
    }

    #[test]
    fn static_field_initializer_invalidates_a_captured_binding_shape() {
        let program = lower(
            "let target = { value: 1 }; \
             class Example { static deleted = delete target.value; } \
             target.value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "the static initializer must invalidate the captured property shape: {:?}",
            result.expr
        );
    }

    #[test]
    fn called_class_constructor_invalidates_later_function_return_shape() {
        let program = lower(
            "let target = { value: 1 }; \
             function readTarget() { return target; } \
             class Example { constructor() { delete target.value; } } \
             new Example(); \
             readTarget().value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "the constructor call must invalidate the cached return shape: {:?}",
            result.expr
        );
    }

    #[test]
    fn constructed_instance_initializer_invalidates_a_captured_binding_shape() {
        let program = lower(
            "let target = { value: 1 }; \
             class Example { field = delete target.value; } \
             new Example(); \
             target.value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "construction must account for the instance initializer effect: {:?}",
            result.expr
        );
    }

    #[test]
    fn constructed_literal_instance_initializer_preserves_an_unrelated_binding_shape() {
        let program = lower(
            "let target = { value: 1 }; \
             class Example { field = 1; } \
             new Example(); \
             target.value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(
                result.expr,
                ExprIr::CoerciveBinaryNumber {
                    op: ArithmeticBinaryOp::Add,
                    ..
                } | ExprIr::BinaryNumber {
                    op: ArithmeticBinaryOp::Add,
                    ..
                }
            ),
            "a literal instance initializer must preserve an unrelated property shape: {:?}",
            result.expr
        );
    }

    #[test]
    fn synthetic_derived_constructor_accounts_for_the_base_constructor_effect() {
        assert_last_expression_is_coercive_add(
            "let target = { value: 1 }; \
             class Base { constructor() { delete target.value; } } \
             class Derived extends Base {} \
             new Derived(); \
             target.value + 1;",
            "a synthetic derived constructor must account for its implicit super call",
        );
    }

    #[test]
    fn called_class_method_invalidates_a_captured_binding_shape() {
        let program = lower(
            "class Example { erase() { delete target.value; } } \
             let example = new Example(); \
             let target = { value: 1 }; \
             example.erase(); \
             target.value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "the method call must invalidate the captured property shape: {:?}",
            result.expr
        );
    }

    #[test]
    fn optional_source_call_invalidates_later_function_return_shape() {
        let program = lower(
            "let target = { value: 1 }; \
             function readTarget() { return target; } \
             function erase() { delete target.value; } \
             erase?.(); \
             readTarget().value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "the optional call must invalidate the cached return shape: {:?}",
            result.expr
        );
    }

    #[test]
    fn uncalled_class_method_does_not_invalidate_a_captured_binding_shape() {
        let program = lower(
            "let target = { value: 1 }; \
             class Example { erase() { delete target.value; } } \
             target.value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(
                result.expr,
                ExprIr::CoerciveBinaryNumber {
                    op: ArithmeticBinaryOp::Add,
                    ..
                } | ExprIr::BinaryNumber {
                    op: ArithmeticBinaryOp::Add,
                    ..
                }
            ),
            "an uncalled method must not publish its captured mutation: {:?}",
            result.expr
        );
    }

    #[test]
    fn uncalled_class_constructor_does_not_invalidate_a_captured_binding_shape() {
        let program = lower(
            "let target = { value: 1 }; \
             class Example { constructor() { delete target.value; } } \
             target.value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(
                result.expr,
                ExprIr::CoerciveBinaryNumber {
                    op: ArithmeticBinaryOp::Add,
                    ..
                } | ExprIr::BinaryNumber {
                    op: ArithmeticBinaryOp::Add,
                    ..
                }
            ),
            "an uncalled constructor must not publish its captured mutation: {:?}",
            result.expr
        );
    }

    #[test]
    fn unconstructed_instance_initializer_does_not_invalidate_a_captured_binding_shape() {
        let program = lower(
            "let target = { value: 1 }; \
             class Example { field = delete target.value; } \
             target.value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(
                result.expr,
                ExprIr::CoerciveBinaryNumber {
                    op: ArithmeticBinaryOp::Add,
                    ..
                } | ExprIr::BinaryNumber {
                    op: ArithmeticBinaryOp::Add,
                    ..
                }
            ),
            "an unconstructed initializer must not publish its captured mutation: {:?}",
            result.expr
        );
    }

    #[test]
    fn array_callback_invalidates_later_function_return_shape() {
        let program = lower(
            "let target = { value: 1 }; \
             function readTarget() { return target; } \
             [0].forEach(function () { delete target.value; }); \
             readTarget().value + 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let StatementIr::Expression(result) = script.body.statements.last().unwrap() else {
            panic!("expected follow-up addition");
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "the callback call must invalidate the cached return shape: {:?}",
            result.expr
        );
    }

    #[test]
    fn source_function_array_mutation_invalidates_the_caller_shape() {
        assert_last_expression_is_coercive_add(
            "const values = [1]; function replace() { values.fill('x'); } replace(); values[0] + 1;",
            "the source function's array mutation must invalidate the caller shape",
        );
    }

    #[test]
    fn tagged_template_call_invalidates_later_function_return_shape() {
        assert_last_expression_is_coercive_add(
            "let target = { value: 1 }; \
             function readTarget() { return target; } \
             function tag() { delete target.value; } \
             tag`value`; \
             readTarget().value + 1;",
            "the tag call must invalidate the cached return shape",
        );
    }

    #[test]
    fn object_exec_method_does_not_use_a_pure_regexp_effect_boundary() {
        assert_last_expression_is_coercive_add(
            "let target = { value: 1 }; \
             function readTarget() { return target; } \
             let object = { exec() { delete target.value; } }; \
             object.exec('value'); \
             readTarget().value + 1;",
            "an ordinary exec method must invalidate the cached return shape",
        );
    }

    #[test]
    fn optional_getter_invalidates_later_function_return_shape() {
        assert_last_expression_is_coercive_add(
            "let target = { value: 1 }; \
             function readTarget() { return target; } \
             let object = { get value() { delete target.value; return 1; } }; \
             object?.value; \
             readTarget().value + 1;",
            "the optional getter must invalidate the cached return shape",
        );
    }

    #[test]
    fn spread_iteration_invalidates_later_function_return_shape() {
        assert_last_expression_is_coercive_add(
            "let target = { value: 1 }; \
             function readTarget() { return target; } \
             function* values() { delete target.value; yield 1; } \
             Math.max(...values()); \
             readTarget().value + 1;",
            "spread iteration must invalidate the cached return shape",
        );
    }

    #[test]
    fn generator_resume_invalidates_a_shape_created_after_generator_creation() {
        assert_last_expression_is_coercive_add(
            "let target; \
             function* values() { delete target.value; } \
             let iterator = values(); \
             target = { value: 1 }; \
             iterator.next(); \
             target.value + 1;",
            "generator resumption must invalidate the later property shape",
        );
    }

    #[test]
    fn multi_target_construction_invalidates_an_instance_initializer_capture() {
        assert_last_expression_is_coercive_add(
            "let target = { value: 1 }; \
             class First { field = delete target.value; } \
             class Second { field = delete target.value; } \
             let selected = Math.random() < 0.5 ? First : Second; \
             new selected(); \
             target.value + 1;",
            "multi-target construction must account for instance initializers",
        );
    }

    #[test]
    fn with_environment_call_invalidates_later_function_return_shape() {
        assert_last_expression_is_coercive_add(
            "let target = { value: 1 }; \
             function readTarget() { return target; } \
             function invoke() {} \
             let scope = new Proxy({}, { has() { delete target.value; return false; } }); \
             with (scope) { invoke(); } \
             readTarget().value + 1;",
            "with-environment selection must invalidate the cached return shape",
        );
    }

    #[test]
    fn pure_static_initializer_does_not_replay_an_earlier_unknown_effect() {
        let program = lower(
            "function inspect(callback) { \
             callback(); \
             let fresh = { value: 1 }; \
             class Example { static value = 0; } \
             return fresh.value + 1; \
             } \
             inspect(function () {});",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let inspect = script
            .functions
            .iter()
            .find(|function| function.name == "inspect")
            .expect("inspect should be lowered");
        let StatementIr::Return(result) = inspect
            .body
            .statements
            .iter()
            .find(|statement| matches!(statement, StatementIr::Return(_)))
            .expect("inspect should return the addition")
        else {
            unreachable!("selected statement is a return")
        };
        assert!(
            matches!(
                result.expr,
                ExprIr::CoerciveBinaryNumber {
                    op: ArithmeticBinaryOp::Add,
                    ..
                } | ExprIr::BinaryNumber {
                    op: ArithmeticBinaryOp::Add,
                    ..
                }
            ),
            "a pure static initializer must preserve facts established after the callback: {:?}",
            result.expr
        );
    }

    #[test]
    fn computed_key_effects_do_not_restore_the_captured_base_shape() {
        for (setup, key) in [
            ("", "(base.marker = 's', 'value')"),
            (
                "",
                "(Object.defineProperty(base, 'marker', { value: 's' }), 'value')",
            ),
            (
                "function key() { base.marker = 's'; return 'value'; }",
                "key()",
            ),
        ] {
            let program = lower(&format!(
                "let base = {{ marker: 1, get value() {{ return this.marker + 1; }} }}; {setup} base[{key}] ||= 2;"
            ));
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let script = program.script.as_ref().expect("script IR should exist");
            let getter = script
                .functions
                .iter()
                .find(|function| function.name.contains("value") && function.params.is_empty())
                .expect("object-literal getter should be lowered");
            let StatementIr::Return(result) = getter
                .body
                .statements
                .iter()
                .find(|statement| matches!(statement, StatementIr::Return(_)))
                .expect("getter should return a value")
            else {
                unreachable!("selected statement is a return")
            };
            assert!(
                matches!(result.expr, ExprIr::CoerciveAdd { .. }),
                "key evaluation must invalidate the captured Number shape: {:?}",
                result.expr
            );
        }
    }

    #[test]
    fn reference_operand_effects_precede_a_plain_assignment_rhs() {
        for source in [
            "let x = 1; function make() { x = 's'; return { p: 0 }; } function run() { return make().p = x + 1; }",
            "let x = 1; const key = { get value() { x = 's'; return 'p'; } }; const base = {}; function run() { return base[key.value] = x + 1; }",
            "let x = 1; const key = { valueOf() { x = 's'; return 1; } }; const base = {}; function run() { return base[key + 0] = x + 1; }",
        ] {
            let program = lower(source);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let script = program.script.as_ref().expect("script IR should exist");
            let assignment = returned_plain_assignment(script, "run");
            assert!(
                matches!(assignment.rhs().expr, ExprIr::CoerciveAdd { .. }),
                "base/raw-key effects must invalidate x before the RHS: {:?}",
                assignment.rhs().expr
            );
        }
    }

    #[test]
    fn implicit_hooks_widen_later_subexpressions_within_reference_stages() {
        let base_program = lower(
            "function outer() { let x = 'u'; const key = { get p() { x = 's'; return 0; } }; return (x = 1, key.p, { value: 0 }).value = x + 1; }",
        );
        let base_script = base_program
            .script
            .as_ref()
            .expect("script IR should exist");
        assert!(
            matches!(
                returned_plain_assignment(base_script, "outer").rhs().expr,
                ExprIr::CoerciveAdd { .. }
            ),
            "an implicit hook inside the base must precede the RHS"
        );

        let key_program = lower(
            "function outer() { let x = 'u'; const key = { get p() { x = 's'; return 0; } }; const base = {}; return base[(x = 1, key.p, x + 1)] = 2; }",
        );
        let key_script = key_program.script.as_ref().expect("script IR should exist");
        let PropertyKeyIr::StringExpr(key) =
            returned_plain_assignment(key_script, "outer").referenced_name()
        else {
            panic!("computed key should remain carried");
        };
        assert!(
            matches!(
                &key.expr,
                ExprIr::Comma { rhs, .. } if matches!(rhs.expr, ExprIr::CoerciveAdd { .. })
            ),
            "an implicit hook must widen later raw-key subexpressions: {:?}",
            key.expr
        );

        let rhs_program = lower(
            "function outer() { let x = 'u'; const key = { get p() { x = 's'; return 0; } }; const base = {}; return base.value = (x = 1, key.p, x + 1); }",
        );
        let rhs_script = rhs_program.script.as_ref().expect("script IR should exist");
        let rhs = returned_plain_assignment(rhs_script, "outer").rhs();
        assert!(
            matches!(
                &rhs.expr,
                ExprIr::Comma { rhs, .. } if matches!(rhs.expr, ExprIr::CoerciveAdd { .. })
            ),
            "an implicit hook must widen later RHS subexpressions: {:?}",
            rhs.expr
        );

        let call_program = lower(
            "let x = 'u'; function mutate() { x = 's'; } function outer() { const base = {}; return base.value = (x = 1, mutate(), x + 1); }",
        );
        let call_script = call_program
            .script
            .as_ref()
            .expect("script IR should exist");
        let rhs = returned_plain_assignment(call_script, "outer").rhs();
        assert!(
            matches!(
                &rhs.expr,
                ExprIr::Comma { rhs, .. } if matches!(rhs.expr, ExprIr::CoerciveAdd { .. })
            ),
            "a source call must widen later RHS subexpressions: {:?}",
            rhs.expr
        );
    }

    #[test]
    fn unknown_proxy_hooks_join_omitted_formals_with_undefined() {
        let program = lower(
            "function hook(a, b, c, d, e) { return e + 1; } hook(0, 0, 0, 0, 1); const handler = { set: hook }; const proxy = new Proxy({}, handler); proxy.x = 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let hook = script
            .functions
            .iter()
            .find(|function| function.name == "hook")
            .expect("hook should be lowered");
        let StatementIr::Return(result) = hook
            .body
            .statements
            .iter()
            .find(|statement| matches!(statement, StatementIr::Return(_)))
            .expect("hook should return")
        else {
            unreachable!("selected statement is a return")
        };
        let ExprIr::CoerciveBinaryNumber { lhs, .. } = &result.expr else {
            panic!(
                "Number-or-undefined addition should remain numeric: {:?}",
                result.expr
            );
        };
        assert_eq!(
            lhs.possible_kinds,
            KindSet::from_kind(ValueKind::Number).union(KindSet::from_kind(ValueKind::Undefined)),
            "the fifth Proxy trap formal must include its omitted undefined argument"
        );
    }

    #[test]
    fn eager_old_value_coercion_invalidates_followup_flow_facts() {
        let program = lower(
            "function outer() { let x = 1; const value = { valueOf() { x = 's'; return 1; } }; const base = [value]; base[0] += 1; return x + 1; }",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let outer = script
            .functions
            .iter()
            .find(|function| function.name == "outer")
            .expect("outer should be lowered");
        let StatementIr::Return(result) = outer
            .body
            .statements
            .iter()
            .find(|statement| matches!(statement, StatementIr::Return(_)))
            .expect("outer should return")
        else {
            unreachable!("selected statement is a return")
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "deferred old-value coercion must widen later facts: {:?}",
            result.expr
        );
    }

    #[test]
    fn primitive_base_key_coercion_does_not_box_the_key_hook_receiver() {
        let program = lower(
            "let out = 0; const key = { get length() { out = 's'; return 'v'; }, toString() { return this.length + 1; } }; ('x')[key] = 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let coercion = script
            .functions
            .iter()
            .find(|function| function.name == "toString")
            .expect("key coercion method should be lowered");
        let StatementIr::Return(result) = coercion
            .body
            .statements
            .iter()
            .find(|statement| matches!(statement, StatementIr::Return(_)))
            .expect("key coercion method should return")
        else {
            unreachable!("selected statement is a return")
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "the key object, not the primitive property base, is the coercion receiver: {:?}",
            result.expr
        );
    }

    #[test]
    fn eager_old_value_coercion_observes_unknown_hook_receivers() {
        let program = lower(
            "const value = { get length() { return 's'; }, valueOf() { return this.length + 1; } }; const base = [value]; base[0] += 1;",
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.as_ref().expect("script IR should exist");
        let coercion = script
            .functions
            .iter()
            .find(|function| function.name == "valueOf")
            .expect("old-value coercion method should be lowered");
        let StatementIr::Return(result) = coercion
            .body
            .statements
            .iter()
            .find(|statement| matches!(statement, StatementIr::Return(_)))
            .expect("old-value coercion method should return")
        else {
            unreachable!("selected statement is a return")
        };
        assert!(
            matches!(result.expr, ExprIr::CoerciveAdd { .. }),
            "deferred ToPrimitive must not reuse the property base as this: {:?}",
            result.expr
        );
    }
}
