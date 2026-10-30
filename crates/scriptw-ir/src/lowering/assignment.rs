use super::*;

/// A saved primitive keeps its value domain while evaluating the right side.
/// Object conversion properties remain mutable, and a missing tracked shape
/// currently means ordinary String conversion to the coercion analyser. Use an
/// unknown value for heap-bearing operands rather than inventing that proof.
fn saved_compound_assignment_value_info(kind: ValueKind, possible_kinds: KindSet) -> ValueInfo {
    if !possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY) {
        return ValueInfo::new(ValueKind::Dynamic);
    }
    ValueInfo {
        kind,
        possible_kinds,
        heap_shape: None,
        function_targets: FunctionTargetKnowledge::none(),
    }
}

impl<'a> ScriptLowerer<'a> {
    /// PutValue of one iteration value into a bare `for (x in/of …)` head
    /// (14.7.5.7 step 6.g.i). The head is re-resolved every iteration exactly
    /// like `x = value`: eval-visible code goes through the runtime identifier
    /// environment, and static code pre-locates the Reference and lets a
    /// preceding `with` object intercept it.
    pub(super) fn lower_bare_iteration_head_write(
        &mut self,
        source_name: String,
        value: TypedExpr,
    ) -> TypedExpr {
        self.lower_bare_iteration_head_write_with_evidence(source_name, value)
            .value
    }

    pub(super) fn lower_bare_iteration_head_write_with_evidence(
        &mut self,
        source_name: String,
        value: TypedExpr,
    ) -> PreparedIdentifierWrite {
        if self.uses_runtime_identifier_environment_for(&source_name) {
            return PreparedIdentifierWrite::performed(self.environment_identifier(
                source_name,
                EnvironmentIdentifierOperationIr::Assign {
                    value: Box::new(value),
                },
            ));
        }
        let reference = self.locate_identifier_reference(&source_name);
        let selected = self
            .with_environment_chain
            .select_preceding(reference.declarative_position());
        if let Some(objects) = selected {
            self.lower_with_scoped_identifier_write_with_evidence(
                source_name,
                value,
                objects,
                reference,
            )
        } else {
            self.lower_located_identifier_assign_value_with_evidence(source_name, value, reference)
        }
    }

    pub(super) fn lower_assign(
        &mut self,
        op: AssignOp,
        lhs: &AssignTarget,
        rhs: &Expression,
    ) -> TypedExpr {
        if let AssignTarget::WebCompatCall(call) = lhs {
            return self.lower_web_compat_call_assignment_target(call);
        }
        if op == AssignOp::Assign {
            if let AssignTarget::Identifier(identifier) = lhs {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                self.register_finite_source_binding_assignment(&name, rhs);
            }
            if let AssignTarget::Pattern(pattern) = lhs {
                if self.plain_async_entry_state().is_some()
                    && contains(pattern, ContainsSymbol::AwaitExpression)
                {
                    if self.async_expression_prefix.is_none() {
                        return self.unsupported_expr(
                            "async pattern assignment requires its expression prefix owner",
                        );
                    }
                    let Some((prefix, value)) =
                        self.lower_staged_async_pattern_assignment(pattern, rhs)
                    else {
                        return self
                            .unsupported_expr("async pattern assignment continuation ownership");
                    };
                    self.async_expression_prefix
                        .as_mut()
                        .expect("checked async pattern prefix")
                        .extend(prefix);
                    return value;
                }
            }
        }
        if self.async_expression_prefix.is_some()
            && matches!(
                op,
                AssignOp::BoolAnd | AssignOp::BoolOr | AssignOp::Coalesce
            )
            && (contains(lhs, ContainsSymbol::AwaitExpression)
                || contains(rhs, ContainsSymbol::AwaitExpression))
        {
            return match AwaitedLogicalAssignmentSource::new(self, op, lhs, rhs) {
                Some(source) => self.lower_logical_assignment_await_value(source),
                None => self.unsupported_expr("awaited logical assignment Reference ownership"),
            };
        }
        if self.plain_async_entry_state().is_some()
            && self.async_expression_prefix.is_some()
            && (contains(lhs, ContainsSymbol::AwaitExpression)
                || contains(rhs, ContainsSymbol::AwaitExpression))
        {
            if let AssignTarget::Identifier(identifier) = lhs {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                if let Some(value) = self.lower_async_identifier_assignment(op, name, rhs) {
                    return value;
                }
            }
            if let AssignTarget::Access(access) = lhs {
                let staged = match (op, access) {
                    (AssignOp::Assign, PropertyAccess::Simple(access)) => {
                        self.lower_staged_generator_property_assignment(access, rhs)
                    }
                    _ => self.lower_resumable_property_assignment_parts(op, lhs, rhs),
                };
                let Some((prefix, value)) = staged else {
                    return self
                        .unsupported_expr("async assignment Reference continuation ownership");
                };
                self.async_expression_prefix
                    .as_mut()
                    .expect("async assignment prefix")
                    .extend(prefix);
                return value;
            }
        }
        if self.uses_runtime_identifier_environment() {
            if let AssignTarget::Identifier(identifier) = lhs {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                if self.uses_runtime_identifier_environment_for(&name) {
                    return self.lower_environment_identifier_assignment(name, op, rhs);
                }
            }
        }
        match op {
            AssignOp::Assign => match lhs {
                AssignTarget::Identifier(identifier) => {
                    let name = self.interner.resolve_expect(identifier.sym()).to_string();
                    // ResolveBinding is the LHS evaluation. Locate its
                    // declarative/global fallback before lowering the RHS, and
                    // carry that same value through Object-ER selection and
                    // the eventual write.
                    let reference = self.locate_identifier_reference(&name);
                    let objects = self
                        .with_environment_chain
                        .select_preceding(reference.declarative_position());
                    let retained_own_global = self.is_unshadowed_script_global_binding(&name)
                        && self
                            .lookup_global_property_info(&name)
                            .is_some_and(|property| {
                                property.proven_present && !property.configurable
                            });
                    if objects.is_some()
                        || (!retained_own_global
                            && (matches!(reference, LocatedIdentifierReference::Unresolvable)
                                || self.is_unshadowed_script_global_binding(&name)))
                    {
                        // HasBinding can invoke a With Proxy or a Proxy in the
                        // global object's prototype chain. Its effects precede
                        // the RHS, even though plain assignment performs no Get.
                        // A retained non-configurable own global property ends
                        // HasProperty before that prototype chain is visited.
                        self.observe_all_planned_source_as_unknown_property_hooks();
                        self.invalidate_unknown_user_code_effects();
                    }
                    let static_to_string_regexp_object =
                        self.static_to_string_returns_regexp_object_expr(rhs);
                    let value = self.lower_expression(rhs);
                    if let Some(objects) = objects {
                        return self
                            .lower_with_scoped_identifier_write(name, value, objects, reference);
                    }
                    let result =
                        self.lower_located_identifier_assign_value(name.clone(), value, reference);
                    if static_to_string_regexp_object {
                        self.static_to_string_regexp_object_bindings.insert(name);
                    } else {
                        self.static_to_string_regexp_object_bindings.remove(&name);
                    }
                    result
                }
                AssignTarget::Access(access) => match access {
                    PropertyAccess::Simple(access) => {
                        self.register_finite_source_property_assignment(access, rhs);
                        self.lower_ordinary_property_plain_assignment(access, rhs)
                    }
                    PropertyAccess::Private(_) | PropertyAccess::Super(_) => {
                        self.lower_property_assign(access, rhs)
                    }
                },
                AssignTarget::Pattern(pattern) => self.lower_pattern_assign(pattern, rhs),
                // Spelled out rather than `_`. `AssignTarget` is a closed
                // 4-variant boa enum (`boa_ast-0.21.1/src/expression/operator/
                // assign/mod.rs:126`) and this is invariant I7's AST half: a
                // fifth production that yields a Reference must be decided
                // here, as `error[E0004]`, not swallowed as an unsupported
                // expression with nothing to compile-error about.
                //
                // `WebCompatCall` is handled by the early return at the top of
                // this function — Annex B `f() = v` is a runtime
                // ReferenceError, not a compiler gap — so it is unreachable
                // here; it is still named, because `unreachable!()` in its
                // place would reintroduce a catch-all by another spelling.
                AssignTarget::WebCompatCall(call) => {
                    self.lower_web_compat_call_assignment_target(call)
                }
            },
            AssignOp::Add
            | AssignOp::Sub
            | AssignOp::Mul
            | AssignOp::Div
            | AssignOp::Mod
            | AssignOp::Exp => {
                let arithmetic = match op {
                    AssignOp::Add => ArithmeticOp::Add,
                    AssignOp::Sub => ArithmeticOp::Sub,
                    AssignOp::Mul => ArithmeticOp::Mul,
                    AssignOp::Div => ArithmeticOp::Div,
                    AssignOp::Mod => ArithmeticOp::Mod,
                    AssignOp::Exp => ArithmeticOp::Exp,
                    AssignOp::Assign
                    | AssignOp::BoolAnd
                    | AssignOp::BoolOr
                    | AssignOp::Coalesce
                    | AssignOp::And
                    | AssignOp::Or
                    | AssignOp::Xor
                    | AssignOp::Shl
                    | AssignOp::Shr
                    | AssignOp::Ushr => {
                        unreachable!("this match arm covers only the arithmetic operators")
                    }
                };
                if let AssignTarget::Access(access) = lhs {
                    return match access {
                        PropertyAccess::Simple(access) => self
                            .lower_ordinary_property_eager_compound_assignment(
                                access,
                                EagerCompoundAssignmentOp::Arithmetic(arithmetic),
                                rhs,
                            ),
                        PropertyAccess::Super(access) => self
                            .lower_super_property_eager_compound_assignment(
                                access,
                                EagerCompoundAssignmentOp::Arithmetic(arithmetic),
                                rhs,
                            ),
                        PropertyAccess::Private(_) => self.lower_property_reference_update(
                            access,
                            PropertyUpdateOp::Arithmetic(arithmetic),
                            rhs,
                        ),
                    };
                }
                let AssignTarget::Identifier(identifier) = lhs else {
                    return self.unsupported_expr("unsupported property assignment operator");
                };

                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                let reference = self.locate_identifier_reference(&name);
                let selected = self
                    .with_environment_chain
                    .select_preceding(reference.declarative_position());
                if let Some(objects) = selected {
                    self.observe_all_planned_source_as_unknown_property_hooks();
                    self.invalidate_unknown_user_code_effects();
                    let value = self.lower_expression(rhs);
                    return self.lower_with_scoped_identifier_eager_compound_assignment(
                        name,
                        EagerCompoundAssignmentOp::Arithmetic(arithmetic),
                        value,
                        objects,
                        reference,
                    );
                }
                if self.is_unshadowed_script_global_binding(&name)
                    || matches!(&reference, LocatedIdentifierReference::Unresolvable)
                {
                    // ResolveBinding/GetBindingValue precede RHS evaluation.
                    // Their hooks may replace captured values and callees.
                    self.observe_all_planned_source_as_unknown_property_hooks();
                    self.invalidate_unknown_user_code_effects();
                    let value = self.lower_expression(rhs);
                    return self.lower_global_identifier_eager_compound_assignment(
                        name,
                        EagerCompoundAssignmentOp::Arithmetic(arithmetic),
                        value,
                    );
                }
                // GetValue of the already-located declarative Reference
                // precedes RHS evaluation. Consume its initialization witness
                // now; initialized const bindings still fail only at PutValue.
                let binding = match reference {
                    LocatedIdentifierReference::Declarative {
                        resolution: BindingResolution::Uninitialized(violation),
                        ..
                    } => return violation.into_throw(),
                    LocatedIdentifierReference::Declarative {
                        resolution: BindingResolution::Initialized(binding),
                        ..
                    } => binding,
                    LocatedIdentifierReference::Declarative {
                        resolution: BindingResolution::Unresolvable,
                        ..
                    } => unreachable!("a declarative location cannot be unresolvable"),
                    LocatedIdentifierReference::Unresolvable => {
                        unreachable!("global compounds retain their Environment Reference")
                    }
                };
                let lhs_info =
                    saved_compound_assignment_value_info(binding.kind, binding.possible_kinds);
                let value = self.lower_expression(rhs);
                let storage_name = binding.storage_name;
                if binding.mode == BindingMode::Const {
                    // GetValue and the operator precede the immutable PutValue.
                    let lhs_read =
                        TypedExpr::from_info(lhs_info, ExprIr::Identifier(storage_name.clone()));
                    let applied = self.combine_arithmetic(arithmetic, lhs_read, value);
                    return self.immutable_binding_write(&storage_name, applied);
                }
                let string_kind = KindSet::from_kind(ValueKind::String);
                let string_add = matches!(op, AssignOp::Add)
                    && (value.possible_kinds.is_subset_of(string_kind)
                        || lhs_info.possible_kinds.is_subset_of(string_kind));
                let coercive_add = matches!(op, AssignOp::Add)
                    && !string_add
                    && (lhs_info.kind != ValueKind::Number || value.kind != ValueKind::Number);
                if coercive_add {
                    let lhs =
                        TypedExpr::from_info(lhs_info, ExprIr::Identifier(storage_name.clone()));
                    let possible_kinds = KindSet::from_kind(ValueKind::String)
                        .union(KindSet::from_kind(ValueKind::Number))
                        .union(KindSet::from_kind(ValueKind::BigInt));
                    let result_info = ValueInfo {
                        kind: possible_kinds.as_value_kind(),
                        possible_kinds,
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    };
                    let result = TypedExpr::from_info(
                        result_info.clone(),
                        ExprIr::CoerciveAdd {
                            lhs: Box::new(lhs),
                            rhs: Box::new(value),
                        },
                    );
                    self.set_binding_value_info(&name, result_info.clone());
                    return TypedExpr::from_info(
                        result_info,
                        ExprIr::AssignIdentifier {
                            name: storage_name,
                            value: Box::new(result),
                        },
                    );
                }
                let needs_general_form = !string_add
                    && (value.kind != ValueKind::Number || lhs_info.kind != ValueKind::Number);
                if needs_general_form {
                    return self.lower_identifier_arithmetic_general(
                        &name,
                        storage_name,
                        lhs_info,
                        arithmetic,
                        value,
                    );
                }
                let result_info = ValueInfo::new(if string_add {
                    ValueKind::String
                } else {
                    ValueKind::Number
                });
                self.set_binding_value_info(&name, result_info.clone());
                let op = match op {
                    AssignOp::Add => ArithmeticBinaryOp::Add,
                    AssignOp::Sub => ArithmeticBinaryOp::Sub,
                    AssignOp::Mul => ArithmeticBinaryOp::Mul,
                    AssignOp::Div => ArithmeticBinaryOp::Div,
                    AssignOp::Mod => ArithmeticBinaryOp::Mod,
                    AssignOp::Exp => ArithmeticBinaryOp::Exp,
                    _ => unreachable!(),
                };
                TypedExpr::from_info(
                    result_info,
                    ExprIr::CompoundAssignIdentifier {
                        name: storage_name,
                        op,
                        value: Box::new(value),
                    },
                )
            }
            AssignOp::BoolAnd | AssignOp::BoolOr | AssignOp::Coalesce => {
                let logical_op = match op {
                    AssignOp::BoolAnd => LogicalBinaryOp::And,
                    AssignOp::BoolOr => LogicalBinaryOp::Or,
                    AssignOp::Coalesce => LogicalBinaryOp::Coalesce,
                    AssignOp::Assign
                    | AssignOp::Add
                    | AssignOp::Sub
                    | AssignOp::Mul
                    | AssignOp::Div
                    | AssignOp::Mod
                    | AssignOp::Exp
                    | AssignOp::And
                    | AssignOp::Or
                    | AssignOp::Xor
                    | AssignOp::Shl
                    | AssignOp::Shr
                    | AssignOp::Ushr => {
                        unreachable!("this match arm covers only the logical operators")
                    }
                };
                let AssignTarget::Identifier(identifier) = lhs else {
                    if let AssignTarget::Access(access) = lhs {
                        return match access {
                            PropertyAccess::Simple(access) => self
                                .lower_ordinary_property_logical_assignment(
                                    access, logical_op, rhs,
                                ),
                            PropertyAccess::Super(_) | PropertyAccess::Private(_) => self
                                .lower_property_reference_update(
                                    access,
                                    PropertyUpdateOp::Logical(logical_op),
                                    rhs,
                                ),
                        };
                    }
                    return self.unsupported_expr("logical assignment");
                };
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                let reference = self.locate_identifier_logical_assignment(&name);
                let selected = self
                    .with_environment_chain
                    .select_preceding(reference.declarative_position());
                let reference = match selected.is_none() {
                    true => match reference.reject_definite_tdz() {
                        Ok(reference) => reference,
                        Err(error) => return error,
                    },
                    false => reference,
                };
                // ResolveBinding and GetValue precede the RHS. Global and
                // with-object hooks can change any captured/global value here.
                if selected.is_some()
                    || !reference.is_declarative()
                    || self.is_unshadowed_script_global_binding(&name)
                {
                    self.observe_all_planned_source_as_unknown_property_hooks();
                    self.invalidate_unknown_user_code_effects();
                }
                let rhs_value = self.lower_conditionally_reached_expression(rhs);
                if let Some(objects) = selected {
                    let plan = self.with_environment_reference_plan(name.clone(), objects);
                    let fallback = self.lower_located_identifier_logical_assignment(
                        name,
                        logical_op,
                        rhs_value.clone(),
                        reference,
                        LogicalAssignmentReachability::WithEnvironmentFallback,
                    );
                    return plan.logical_assignment(logical_op, rhs_value, fallback);
                }
                self.lower_located_identifier_logical_assignment(
                    name,
                    logical_op,
                    rhs_value,
                    reference,
                    LogicalAssignmentReachability::Definite,
                )
            }
            AssignOp::And
            | AssignOp::Or
            | AssignOp::Xor
            | AssignOp::Shl
            | AssignOp::Shr
            | AssignOp::Ushr => {
                let AssignTarget::Identifier(identifier) = lhs else {
                    if let AssignTarget::Access(access) = lhs {
                        let bitwise = match op {
                            AssignOp::And => BitwiseOp::And,
                            AssignOp::Or => BitwiseOp::Or,
                            AssignOp::Xor => BitwiseOp::Xor,
                            AssignOp::Shl => BitwiseOp::Shl,
                            AssignOp::Shr => BitwiseOp::Shr,
                            AssignOp::Ushr => BitwiseOp::UShr,
                            AssignOp::Assign
                            | AssignOp::Add
                            | AssignOp::Sub
                            | AssignOp::Mul
                            | AssignOp::Div
                            | AssignOp::Mod
                            | AssignOp::Exp
                            | AssignOp::BoolAnd
                            | AssignOp::BoolOr
                            | AssignOp::Coalesce => {
                                unreachable!("this match arm covers only the bitwise operators")
                            }
                        };
                        return match access {
                            PropertyAccess::Simple(access) => self
                                .lower_ordinary_property_eager_compound_assignment(
                                    access,
                                    EagerCompoundAssignmentOp::Bitwise(bitwise),
                                    rhs,
                                ),
                            PropertyAccess::Super(access) => self
                                .lower_super_property_eager_compound_assignment(
                                    access,
                                    EagerCompoundAssignmentOp::Bitwise(bitwise),
                                    rhs,
                                ),
                            PropertyAccess::Private(_) => self.lower_property_reference_update(
                                access,
                                PropertyUpdateOp::Bitwise(bitwise),
                                rhs,
                            ),
                        };
                    }
                    return self.unsupported_expr("unsupported property assignment operator");
                };
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                let bitwise = match op {
                    AssignOp::And => BitwiseOp::And,
                    AssignOp::Or => BitwiseOp::Or,
                    AssignOp::Xor => BitwiseOp::Xor,
                    AssignOp::Shl => BitwiseOp::Shl,
                    AssignOp::Shr => BitwiseOp::Shr,
                    AssignOp::Ushr => BitwiseOp::UShr,
                    AssignOp::Assign
                    | AssignOp::Add
                    | AssignOp::Sub
                    | AssignOp::Mul
                    | AssignOp::Div
                    | AssignOp::Mod
                    | AssignOp::Exp
                    | AssignOp::BoolAnd
                    | AssignOp::BoolOr
                    | AssignOp::Coalesce => {
                        unreachable!("this match arm covers only the bitwise operators")
                    }
                };
                let reference = self.locate_identifier_reference(&name);
                let selected = self
                    .with_environment_chain
                    .select_preceding(reference.declarative_position());
                if let Some(objects) = selected {
                    self.observe_all_planned_source_as_unknown_property_hooks();
                    self.invalidate_unknown_user_code_effects();
                    let value = self.lower_expression(rhs);
                    return self.lower_with_scoped_identifier_eager_compound_assignment(
                        name,
                        EagerCompoundAssignmentOp::Bitwise(bitwise),
                        value,
                        objects,
                        reference,
                    );
                }
                if self.is_unshadowed_script_global_binding(&name)
                    || matches!(&reference, LocatedIdentifierReference::Unresolvable)
                {
                    // ResolveBinding/GetBindingValue precede RHS evaluation.
                    // Their hooks may replace captured values and callees.
                    self.observe_all_planned_source_as_unknown_property_hooks();
                    self.invalidate_unknown_user_code_effects();
                    let value = self.lower_expression(rhs);
                    return self.lower_global_identifier_eager_compound_assignment(
                        name,
                        EagerCompoundAssignmentOp::Bitwise(bitwise),
                        value,
                    );
                }
                // 13.15.3 / 13.15.4: GetValue then PutValue, so 9.1.1.1.6 step 2
                // and 9.1.1.1.5 step 3 both apply — and step 3 precedes the
                // immutability test below, which is the only test this arm used
                // to make. The RHS is not lowered yet here, so the throw is in
                // the right place.
                match self.resolve_binding_reference(&name) {
                    BindingResolution::Uninitialized(violation) => return violation.into_throw(),
                    BindingResolution::Initialized(_) | BindingResolution::Unresolvable => {}
                }
                let binding = self.lookup_binding(&name);
                let binding_storage_name =
                    binding.as_ref().map(|binding| binding.storage_name.clone());
                let global_info = self.lookup_global_property_info(&name).cloned();
                // As for the arithmetic forms: 13.15.2 applies the operator
                // before PutValue, so a `const` target still coerces both
                // operands and only then throws.
                let const_storage_name = binding.as_ref().and_then(|binding| {
                    (binding.mode == BindingMode::Const).then(|| binding.storage_name.clone())
                });
                if binding.is_none()
                    && !global_info.as_ref().is_some_and(|info| info.proven_present)
                {
                    self.unsupported_with_message(format!(
                        "unsupported in scriptw wasm-aot first slice: unbound identifier `{name}`"
                    ));
                    return TypedExpr::undefined();
                }
                let op = match op {
                    AssignOp::And => BitwiseBinaryOp::And,
                    AssignOp::Or => BitwiseBinaryOp::Or,
                    AssignOp::Xor => BitwiseBinaryOp::Xor,
                    AssignOp::Shl => BitwiseBinaryOp::Shl,
                    AssignOp::Shr => BitwiseBinaryOp::Shr,
                    AssignOp::Ushr => BitwiseBinaryOp::UShr,
                    _ => unreachable!(),
                };
                let lhs_value = TypedExpr::from_info(
                    binding
                        .as_ref()
                        .map(|binding| {
                            saved_compound_assignment_value_info(
                                binding.kind,
                                binding.possible_kinds,
                            )
                        })
                        .or_else(|| {
                            global_info.as_ref().map(|info| {
                                saved_compound_assignment_value_info(
                                    info.value_info.kind,
                                    info.value_info.possible_kinds,
                                )
                            })
                        })
                        .unwrap_or_else(|| ValueInfo::new(ValueKind::Dynamic)),
                    if binding.is_some() {
                        ExprIr::Identifier(binding_storage_name.clone().expect("binding storage"))
                    } else {
                        ExprIr::GlobalPropertyRead { name: name.clone() }
                    },
                );
                let rhs = self.lower_expression(rhs);
                let value = self.combine_bitwise(op, lhs_value, rhs);
                if let Some(storage_name) = const_storage_name {
                    return self.immutable_binding_write(&storage_name, value);
                }
                if let Some(storage_name) = binding_storage_name {
                    self.set_binding_value_info(&name, value.value_info());
                    TypedExpr::from_info(
                        value.value_info(),
                        ExprIr::AssignIdentifier {
                            name: storage_name,
                            value: Box::new(value),
                        },
                    )
                } else {
                    self.set_global_property_value_info(name.clone(), value.value_info());
                    let strictness = self.reference_strictness();
                    TypedExpr::from_info(
                        value.value_info(),
                        ExprIr::GlobalPropertyWrite {
                            name,
                            value: Box::new(value),
                            implicit: false,
                            strictness,
                        },
                    )
                }
            }
        }
    }
}
