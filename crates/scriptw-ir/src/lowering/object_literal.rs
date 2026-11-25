//! Ordered object-literal properties, methods, name inference and tracked shapes.

use super::*;

/// Only computed-property NamedEvaluation can mint this completed class value.
/// Keeping its source property prevents a prepared class from being substituted
/// into another property or an unrelated generic async operand pin.
pub(super) struct PreparedComputedClassProperty<'ast> {
    property: &'ast PropertyDefinition,
    value: TypedExpr,
}

impl<'a> ScriptLowerer<'a> {
    fn lower_object_method_function(
        &mut self,
        method: &ObjectMethodDefinition,
        function_name: &str,
    ) -> Option<(ObjectMethodFunctionIr, ValueInfo)> {
        let Some(parameters) = self.lower_function_parameters(method.parameters(), function_name)
        else {
            return None;
        };
        match method.kind() {
            MethodDefinitionKind::Ordinary
            | MethodDefinitionKind::Generator
            | MethodDefinitionKind::Async
            | MethodDefinitionKind::AsyncGenerator => {}
            MethodDefinitionKind::Get => {
                if !parameters.as_ref().is_empty() {
                    self.unsupported_with_message(format!(
                        "unsupported in scriptw wasm-aot first slice: getter `{function_name}` must not declare parameters"
                    ));
                    return None;
                }
            }
            MethodDefinitionKind::Set => {
                if parameters.as_ref().len() != 1 {
                    self.unsupported_with_message(format!(
                        "unsupported in scriptw wasm-aot first slice: setter `{function_name}` must declare exactly one parameter"
                    ));
                    return None;
                }
                // PropertySetParameterList is one FormalParameter: a pattern
                // or default initializer is ordinary parameter binding (with
                // `length` 0 for a default); a rest parameter is an early
                // error the parser reports before lowering.
                let parameter = &parameters.as_ref()[0];
                if parameter.is_rest_param() {
                    self.unsupported_with_message(format!(
                        "unsupported in scriptw wasm-aot first slice: setter `{function_name}` declares a rest parameter"
                    ));
                    return None;
                }
            }
        }
        let key = object_method_key(method);
        let Some(function_id) = self.analysis.function_expr_ids.get(&key).cloned() else {
            self.unsupported_expr("object literal method");
            return None;
        };
        let info = self.function_value_info(&function_id);
        let function =
            ObjectMethodFunctionIr::new(function_id, object_method_protocol(method.kind()));
        Some((function, info))
    }

    pub(super) fn lower_object_literal(&mut self, object: &ObjectLiteral) -> TypedExpr {
        if self.object_literal_has_duplicate_proto_setter(object) {
            self.diagnostics.push(IrDiagnostic::rejected(
                EarlyErrorCode::ObjectDuplicateProto,
                "early error: duplicate __proto__ prototype setter in object literal",
                None,
            ));
        }

        let mut properties = Vec::with_capacity(object.properties().len());
        let mut shape = ObjectShape::default();
        let mut has_spread = false;
        // A computed key whose identity is not known statically can name *any*
        // property (including `valueOf`/`toString`), so the tracked shape can no
        // longer be treated as the object's complete property set.
        let mut has_unknown_key = false;
        for property in object.properties() {
            if let Err(error) = self.lower_object_literal_property_into(
                property,
                &mut properties,
                &mut shape,
                &mut has_spread,
                &mut has_unknown_key,
                None,
            ) {
                return error;
            }
        }
        TypedExpr::from_info(
            ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: (!has_spread && !has_unknown_key)
                    .then(|| Arc::new(HeapShape::Object(shape))),
                function_targets: FunctionTargetKnowledge::none(),
            },
            ExprIr::ObjectLiteral(properties),
        )
    }

    /// The ordinary and suspended owners consume the same source property.
    /// Borrowing the original AST preserves method/function analysis identities.
    pub(super) fn lower_object_literal_property(
        &mut self,
        property: &PropertyDefinition,
    ) -> Option<ObjectPropertyIr> {
        self.lower_one_object_literal_property(property, None)
    }

    pub(super) fn prepare_computed_object_class_property<'ast>(
        &mut self,
        property: &'ast PropertyDefinition,
        normalized_key_binding: &str,
        prefix: &mut Vec<StatementIr>,
    ) -> Option<PreparedComputedClassProperty<'ast>> {
        let PropertyDefinition::Property(PropertyName::Computed(_), source) = property else {
            return None;
        };
        if !source.is_anonymous_function_definition() {
            return None;
        }
        let Expression::ClassExpression(class) = Self::unwrap_parenthesized_expr(source) else {
            return None;
        };
        let enclosing_prefix = self.async_expression_prefix.replace(Vec::new());
        let value = self.lower_class_expression_with_inferred_name(
            class,
            ClassNameInferenceIr::PropertyKeyBinding(normalized_key_binding.to_owned()),
        );
        prefix.extend(
            std::mem::replace(&mut self.async_expression_prefix, enclosing_prefix)
                .expect("computed class NamedEvaluation owns its evaluation prefix"),
        );
        let mut info = value.value_info();
        info.heap_shape = None;
        let name = self.alloc_suspension_owned_binding("generator.object.class.", info);
        prefix.push(StatementIr::Lexical {
            mode: BindingMode::Let,
            name: name.clone(),
            init: value,
        });
        Some(PreparedComputedClassProperty {
            property,
            value: self.lower_identifier_name(name, false),
        })
    }

    pub(super) fn lower_prepared_object_class_property(
        &mut self,
        prepared: PreparedComputedClassProperty<'_>,
    ) -> Option<ObjectPropertyIr> {
        self.lower_one_object_literal_property(prepared.property, Some(prepared.value))
    }

    fn lower_one_object_literal_property(
        &mut self,
        property: &PropertyDefinition,
        prepared_class: Option<TypedExpr>,
    ) -> Option<ObjectPropertyIr> {
        let mut properties = Vec::with_capacity(1);
        self.lower_object_literal_property_into(
            property,
            &mut properties,
            &mut ObjectShape::default(),
            &mut false,
            &mut false,
            prepared_class,
        )
        .ok()?;
        properties.pop()
    }

    fn lower_object_literal_property_into(
        &mut self,
        property: &PropertyDefinition,
        properties: &mut Vec<ObjectPropertyIr>,
        shape: &mut ObjectShape,
        has_spread: &mut bool,
        has_unknown_key: &mut bool,
        prepared_class: Option<TypedExpr>,
    ) -> Result<(), TypedExpr> {
        match property {
            PropertyDefinition::Property(PropertyName::Literal(name), value) => {
                let key = self.interned_runtime_string(name.sym());
                self.observe_proxy_trap_value_hint(&key, value);
                let lowered = self.lower_expression(value);
                if key == "__proto__" {
                    if lowered
                        .possible_kinds
                        .is_subset_of(Self::object_like_kind_set())
                    {
                        shape.prototype = lowered.heap_shape.clone();
                    } else if lowered.possible_kinds == KindSet::from_kind(ValueKind::Null) {
                        shape.prototype = None;
                    }
                    properties.push(ObjectPropertyIr::PrototypeSetter { value: lowered });
                    return Ok(());
                }
                Self::insert_string_keyed_shape_property(
                    shape,
                    &key,
                    ObjectShapeProperty::Data(lowered.value_info()),
                );
                properties.push(ObjectPropertyIr::Data {
                    key,
                    value: lowered,
                    is_shorthand: false,
                });
            }
            PropertyDefinition::IdentifierReference(identifier) => {
                let key = self.interner.resolve_expect(identifier.sym()).to_string();
                let lowered = self.lower_expression(&Expression::Identifier(*identifier));
                Self::insert_string_keyed_shape_property(
                    shape,
                    &key,
                    ObjectShapeProperty::Data(lowered.value_info()),
                );
                properties.push(ObjectPropertyIr::Data {
                    key,
                    value: lowered,
                    is_shorthand: true,
                });
            }
            PropertyDefinition::MethodDefinition(method) => {
                let unsupported_generator = method.kind() == MethodDefinitionKind::Generator
                    && !generator_function_is_aot_supported(method.body(), method.parameters());
                if unsupported_generator {
                    return Err(self.unsupported_expr("object literal method"));
                }

                let static_key = match method.name() {
                    PropertyName::Literal(name) => Some(self.interned_runtime_string(name.sym())),
                    PropertyName::Computed(expr) => self.try_static_ordinary_property_key(expr),
                };

                if let Some(key) = static_key {
                    self.observe_proxy_trap_method_hint(&key, method);
                    let Some((function, function_info)) =
                        self.lower_object_method_function(method, &key)
                    else {
                        return Err(TypedExpr::undefined());
                    };
                    match method.kind() {
                        MethodDefinitionKind::Ordinary
                        | MethodDefinitionKind::Generator
                        | MethodDefinitionKind::Async
                        | MethodDefinitionKind::AsyncGenerator => {
                            Self::insert_string_keyed_shape_property(
                                shape,
                                &key,
                                ObjectShapeProperty::Data(function_info),
                            );
                            properties.push(ObjectPropertyIr::Method { key, function });
                        }
                        MethodDefinitionKind::Get => {
                            let function_id = function.function_id().clone();
                            let entry = shape.properties.remove(&key);
                            let setter = match entry {
                                Some(ObjectShapeProperty::Accessor { setter, .. }) => setter,
                                _ => None,
                            };
                            Self::insert_string_keyed_shape_property(
                                shape,
                                &key,
                                ObjectShapeProperty::Accessor {
                                    getter: Some(ObjectAccessorShape { function_id }),
                                    setter,
                                },
                            );
                            properties.push(ObjectPropertyIr::Getter { key, function });
                        }
                        MethodDefinitionKind::Set => {
                            let function_id = function.function_id().clone();
                            let entry = shape.properties.remove(&key);
                            let getter = match entry {
                                Some(ObjectShapeProperty::Accessor { getter, .. }) => getter,
                                _ => None,
                            };
                            Self::insert_string_keyed_shape_property(
                                shape,
                                &key,
                                ObjectShapeProperty::Accessor {
                                    getter,
                                    setter: Some(ObjectAccessorShape { function_id }),
                                },
                            );
                            properties.push(ObjectPropertyIr::Setter { key, function });
                        }
                    }
                    return Ok(());
                }

                let PropertyName::Computed(expr) = method.name() else {
                    return Err(self.unsupported_expr("computed object key"));
                };
                let well_known_key = self.try_well_known_symbol_key_name(expr);
                let key = self.lower_expression(expr);
                if !key
                    .possible_kinds
                    .is_subset_of(KindSet::PROPERTY_KEY_COERCIBLE)
                {
                    return Err(self.unsupported_expr("computed object key"));
                }
                // ToPropertyKey of an object key is user code, and it runs
                // before the method is created.
                self.record_possible_to_primitive_effects(&key.value_info());
                let key_may_be_string = Self::computed_key_may_be_string(&key);
                let Some((function, function_info)) =
                    self.lower_object_method_function(method, "<computed>")
                else {
                    return Err(TypedExpr::undefined());
                };
                match method.kind() {
                    MethodDefinitionKind::Ordinary
                    | MethodDefinitionKind::Generator
                    | MethodDefinitionKind::Async
                    | MethodDefinitionKind::AsyncGenerator => {
                        // See the computed data-property case: a
                        // well-known-symbol method is a statically known key,
                        // so it belongs in the tracked shape.
                        match well_known_key {
                            Some(symbol) => {
                                shape.properties.insert(
                                    shape_namespace_key(symbol),
                                    ObjectShapeProperty::Data(function_info),
                                );
                            }
                            None => *has_unknown_key |= key_may_be_string,
                        }
                        properties.push(ObjectPropertyIr::ComputedMethod { key, function });
                    }
                    MethodDefinitionKind::Get => {
                        let function_id = function.function_id().clone();
                        match well_known_key {
                            Some(symbol) => {
                                let property_name = shape_namespace_key(symbol);
                                let setter = match shape.properties.get(&property_name) {
                                    Some(ObjectShapeProperty::Accessor { setter, .. }) => {
                                        setter.clone()
                                    }
                                    _ => None,
                                };
                                shape.properties.insert(
                                    property_name,
                                    ObjectShapeProperty::Accessor {
                                        getter: Some(ObjectAccessorShape { function_id }),
                                        setter,
                                    },
                                );
                            }
                            None => *has_unknown_key |= key_may_be_string,
                        }
                        properties.push(ObjectPropertyIr::ComputedGetter { key, function });
                    }
                    MethodDefinitionKind::Set => {
                        let function_id = function.function_id().clone();
                        match well_known_key {
                            Some(symbol) => {
                                let property_name = shape_namespace_key(symbol);
                                let getter = match shape.properties.get(&property_name) {
                                    Some(ObjectShapeProperty::Accessor { getter, .. }) => {
                                        getter.clone()
                                    }
                                    _ => None,
                                };
                                shape.properties.insert(
                                    property_name,
                                    ObjectShapeProperty::Accessor {
                                        getter,
                                        setter: Some(ObjectAccessorShape { function_id }),
                                    },
                                );
                            }
                            None => *has_unknown_key |= key_may_be_string,
                        }
                        properties.push(ObjectPropertyIr::ComputedSetter { key, function });
                    }
                }
            }
            PropertyDefinition::CoverInitializedName(_identifier, _) => {
                return Err(self.unsupported_expr("object literal shorthand"));
            }
            PropertyDefinition::Property(PropertyName::Computed(expr), value) => {
                let anonymous = value.is_anonymous_function_definition();
                let static_key = self.try_static_ordinary_property_key(expr);
                if let Some(key) = static_key.as_ref().filter(|_| !anonymous) {
                    self.observe_proxy_trap_value_hint(&key, value);
                    let lowered = self.lower_expression(value);
                    Self::insert_string_keyed_shape_property(
                        shape,
                        &key,
                        ObjectShapeProperty::Data(lowered.value_info()),
                    );
                    properties.push(ObjectPropertyIr::Data {
                        key: key.clone(),
                        value: lowered,
                        is_shorthand: false,
                    });
                    return Ok(());
                }
                let well_known_key = self.try_well_known_symbol_key_name(expr);
                let mut key = match &static_key {
                    Some(key) => TypedExpr::from_info(
                        ValueInfo::new(ValueKind::String),
                        ExprIr::String(key.clone()),
                    ),
                    None => self.lower_expression(expr),
                };
                if !key
                    .possible_kinds
                    .is_subset_of(KindSet::PROPERTY_KEY_COERCIBLE)
                {
                    return Err(self.unsupported_expr("computed object key"));
                }
                // ToPropertyKey of an object key is user code, and it runs
                // before the value is evaluated.
                self.record_possible_to_primitive_effects(&key.value_info());
                let (lowered, name_inference) = if anonymous {
                    if let Expression::ClassExpression(class) =
                        Self::unwrap_parenthesized_expr(value)
                    {
                        if let Some(lowered) = prepared_class {
                            (lowered, ComputedPropertyNameInferenceIr::None)
                        } else {
                            let suspends = self.class_evaluation_state().is_some()
                                && (contains(value, ContainsSymbol::AwaitExpression)
                                    || contains(value, ContainsSymbol::YieldExpression));
                            let key_binding = if suspends {
                                let normalized = TypedExpr::spec_to_property_key(key);
                                let binding = self.alloc_suspension_owned_binding(
                                    "class.inferred.name.",
                                    normalized.value_info(),
                                );
                                self.async_expression_prefix
                                    .as_mut()
                                    .expect("suspending class owns an evaluation prefix")
                                    .push(StatementIr::Lexical {
                                        mode: BindingMode::Let,
                                        name: binding.clone(),
                                        init: normalized,
                                    });
                                key = self.lower_identifier_name(binding.clone(), false);
                                binding
                            } else {
                                self.alloc_temp_binding_name("class.inferred.name.")
                            };
                            let lowered = self.lower_class_expression_with_inferred_name(
                                class,
                                ClassNameInferenceIr::PropertyKeyBinding(key_binding.clone()),
                            );
                            let inference = if suspends {
                                ComputedPropertyNameInferenceIr::None
                            } else {
                                ComputedPropertyNameInferenceIr::Class { key_binding }
                            };
                            (lowered, inference)
                        }
                    } else {
                        (
                            self.lower_expression(value),
                            ComputedPropertyNameInferenceIr::Function,
                        )
                    }
                } else {
                    (
                        self.lower_expression(value),
                        ComputedPropertyNameInferenceIr::None,
                    )
                };
                // A well-known-symbol key (`{ [Symbol.toPrimitive]: fn }`) is
                // still a computed key at runtime, but its identity is known
                // statically. Record it in the tracked shape — under the
                // symbol namespace, so no string-keyed read can reach it —
                // so ToPrimitive inference sees the hook instead of
                // concluding the object has no hooks and always stringifies.
                match (static_key, well_known_key) {
                    (Some(key), _) => Self::insert_string_keyed_shape_property(
                        shape,
                        &key,
                        ObjectShapeProperty::Data(lowered.value_info()),
                    ),
                    (None, Some(symbol)) => {
                        shape.properties.insert(
                            shape_namespace_key(symbol),
                            ObjectShapeProperty::Data(lowered.value_info()),
                        );
                    }
                    (None, None) => *has_unknown_key |= Self::computed_key_may_be_string(&key),
                }
                properties.push(ObjectPropertyIr::ComputedData {
                    key,
                    value: lowered,
                    name_inference,
                });
            }
            PropertyDefinition::SpreadObject(source) => {
                *has_spread = true;
                let source = self.lower_expression(source);
                self.invalidate_unknown_user_code_effects();
                properties.push(ObjectPropertyIr::Spread { source });
            }
        }
        Ok(())
    }

    /// Whether a lowered computed key could name a string property. A key that
    /// is provably a Symbol can never collide with (or shadow) a string-keyed
    /// property, so a shape that omits it still describes every string key the
    /// object has; anything else may turn out to be `"valueOf"` at runtime and
    /// invalidates the tracked shape as a complete property set.
    fn computed_key_may_be_string(key: &TypedExpr) -> bool {
        !key.possible_kinds
            .is_subset_of(KindSet::from_kind(ValueKind::Symbol))
    }

    // The shape-map name for a well-known-symbol key used to be built here from
    // a `&str`, so any string at all could be given the symbol namespace. It is
    // now `well_known::shape_namespace_key`, which takes a `WellKnownSymbol`.

    /// Records a *string*-keyed property in a tracked shape. A key inside the
    /// symbol namespace is left untracked rather than written, so it can never
    /// be mistaken for a symbol-keyed entry; string-keyed reads of such a name
    /// do not consult the shape either, so omitting it changes no answer.
    fn insert_string_keyed_shape_property(
        shape: &mut ObjectShape,
        key: &str,
        property: ObjectShapeProperty,
    ) {
        if shape_property_name_is_symbol_keyed(key) {
            return;
        }
        shape.properties.insert(key.to_string(), property);
    }

    pub(super) fn object_literal_has_duplicate_proto_setter(&self, object: &ObjectLiteral) -> bool {
        let mut proto_setters = 0usize;
        for property in object.properties() {
            let PropertyDefinition::Property(PropertyName::Literal(name), _) = property else {
                continue;
            };
            if self.interned_runtime_string(name.sym()) == "__proto__" {
                proto_setters += 1;
                if proto_setters > 1 {
                    return true;
                }
            }
        }
        false
    }
}
