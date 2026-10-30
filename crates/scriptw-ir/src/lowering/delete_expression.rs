use super::*;

impl<'a> ScriptLowerer<'a> {
    pub(super) fn lower_delete(&mut self, target: &Expression) -> TypedExpr {
        let target = Self::unwrap_parenthesized_expr(target);
        if self.uses_runtime_identifier_environment() {
            if let Expression::Identifier(identifier) = target {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                if self.uses_runtime_identifier_environment_for(&name) {
                    return self
                        .environment_identifier(name, EnvironmentIdentifierOperationIr::Delete);
                }
            }
        }
        match target {
            Expression::Optional(optional) => self.lower_optional_delete_property(optional),
            Expression::PropertyAccess(PropertyAccess::Simple(access)) => {
                if self.is_constructor_prototype_expr(access.target(), ARRAY_NAME) {
                    self.array_prototype_mutated = true;
                }
                if self.is_number_prototype_property_expr(target, "toString") {
                    self.number_prototype_to_string_state = PrototypeToStringState::ObjectPrototype;
                }
                if self.is_boolean_prototype_property_expr(target, "toString") {
                    self.boolean_prototype_to_string_state =
                        PrototypeToStringState::ObjectPrototype;
                }
                let mut target = self.lower_property_target(access.target());
                if target.kind == ValueKind::Undefined
                    && self.is_current_param_expr(access.target())
                {
                    target.kind = ValueKind::Dynamic;
                    target.possible_kinds = KindSet::all_runtime_tags();
                    target.heap_shape = None;
                    target.function_targets.widen_for_possible_replacement();
                }
                let key = match access.field() {
                    PropertyAccessField::Const(name) => PropertyKeyIr::StaticString(
                        self.interner.resolve_expect(name.sym()).to_string(),
                    ),
                    PropertyAccessField::Expr(expr) => {
                        if let Some(key) = self.lower_static_property_key(expr) {
                            key
                        } else {
                            let mut lowered = self.lower_expression(expr);
                            if lowered.kind == ValueKind::Undefined
                                && self.is_current_param_expr(expr)
                            {
                                lowered.kind = ValueKind::Dynamic;
                                lowered.possible_kinds = KindSet::all_runtime_tags();
                                lowered.heap_shape = None;
                                lowered.function_targets.widen_for_possible_replacement();
                            }
                            if lowered.kind == ValueKind::Number {
                                PropertyKeyIr::ArrayIndex(Box::new(lowered))
                            } else if lowered.kind == ValueKind::String
                                || lowered.kind == ValueKind::Symbol
                                || lowered.possible_kinds.contains(ValueKind::String)
                                || lowered.possible_kinds.contains(ValueKind::Symbol)
                                || lowered
                                    .possible_kinds
                                    .is_subset_of(KindSet::PROPERTY_KEY_COERCIBLE)
                                || lowered.possible_kinds == KindSet::all_runtime_tags()
                            {
                                PropertyKeyIr::StringExpr(Box::new(lowered))
                            } else {
                                return self.unsupported_expr("unsupported unary operator");
                            }
                        }
                    }
                };
                if matches!(&target.expr, ExprIr::ExecutionGlobalObject) {
                    if let PropertyKeyIr::StaticString(name) = &key {
                        // This is a property Reference even when a same-named
                        // global lexical binding exists. Retain the evaluated
                        // receiver/key while clearing the actual-global fact.
                        self.record_global_property_delete(name);
                    }
                }
                self.update_well_known_symbol_prototype_property(access.target(), &key, None);
                self.lower_delete_property_from_evaluated(target, key)
            }
            Expression::PropertyAccess(PropertyAccess::Private(_)) => {
                self.unsupported_expr("unsupported unary operator")
            }
            Expression::PropertyAccess(PropertyAccess::Super(access)) => {
                if self.class_context.is_none() {
                    return self.unsupported_expr("object literal method");
                }
                let Some(key) = self.lower_super_property_key(access.field()) else {
                    return TypedExpr::undefined();
                };
                DeleteSuperReferencePlan::new(self.current_this_info(), key).into_reference_error()
            }
            Expression::Identifier(identifier) => {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                let reference = self.locate_identifier_reference(&name);
                let selected = self
                    .with_environment_chain
                    .select_preceding(reference.declarative_position());
                let fallback = if selected.is_some()
                    && matches!(reference, LocatedIdentifierReference::Unresolvable)
                {
                    // HasBinding/unscopables can create the global property
                    // before rejecting this object environment candidate.
                    self.lower_global_identifier_delete(name.clone())
                } else {
                    self.lower_identifier_delete_fallback(name.clone())
                };
                if let Some(objects) = selected {
                    self.invalidate_unknown_user_code_effects();
                    self.with_environment_reference_plan(name, objects)
                        .delete_binding(fallback)
                } else {
                    fallback
                }
            }
            _ => TypedExpr::from_info(
                ValueInfo::new(ValueKind::Boolean),
                ExprIr::DeleteValue {
                    expr: Box::new(self.lower_expression(target)),
                },
            ),
        }
    }

    /// Consume an already acquired raw property Reference without performing Get.
    pub(super) fn lower_delete_property_from_evaluated(
        &mut self,
        target: TypedExpr,
        key: PropertyKeyIr,
    ) -> TypedExpr {
        if target.heap_shape.is_none() || Self::property_key_may_call_user_code(&key) {
            // An unknown object can be a Proxy, and an object-valued
            // key can run arbitrary source code during ToPropertyKey.
            self.invalidate_unknown_user_code_effects();
        } else {
            // Delete may succeed and expose an inherited accessor, or
            // fail and preserve the own property. Until descriptor
            // attributes are tracked, neither outcome may retain an
            // exact target or enclosing-alias shape.
            self.invalidate_ordinary_property_shape_aliases(&target.value_info());
        }
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::DeleteProperty {
                target: Box::new(target),
                key,
                strictness: self.reference_strictness(),
            },
        )
    }

    fn lower_identifier_delete_fallback(&mut self, name: String) -> TypedExpr {
        if self.is_unshadowed_script_global_binding(&name) {
            return self.lower_global_identifier_delete(name);
        }
        if name == "undefined"
            || self.lookup_binding(&name).is_some()
            || (self.root_functions_need_body_initialization()
                && self.visible_function_names.contains_key(&name))
            || (name == "arguments" && self.lookup_binding(LEXICAL_ARGUMENTS_NAME).is_some())
            || self
                .lookup_global_property_info(&name)
                .is_some_and(|info| info.proven_present && !info.configurable)
        {
            return TypedExpr::from_info(
                ValueInfo::new(ValueKind::Boolean),
                ExprIr::DeleteIdentifier { name },
            );
        }
        self.lower_global_identifier_delete(name)
    }

    fn lower_global_identifier_delete(&mut self, name: String) -> TypedExpr {
        // A proven present root property ends HasBinding before prototype
        // hooks and DeleteBinding never invokes its getter. A dormant body
        // or uncertain property must account for an inherited Proxy first.
        if self.current_owner_id != SCRIPT_OWNER_ID
            || !self.global_property_is_proven_present(&name)
        {
            self.observe_all_planned_source_as_unknown_property_hooks();
            self.invalidate_unknown_user_code_effects();
        }
        self.record_global_property_delete(&name);
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::DeleteGlobalProperty {
                name,
                // Strict identifier deletion is an early SyntaxError.
                strictness: Strictness::Sloppy,
            },
        )
    }
}
