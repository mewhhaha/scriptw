mod callable_flow;
mod field_names;

use self::callable_flow::ClassConstructorInvocationRole;
use self::field_names::class_field_initializer_name;
use super::*;

impl<'a> ScriptLowerer<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_class_common_in_name_scope(
        &mut self,
        class_name: Option<String>,
        class_source: String,
        constructor_execution_key: String,
        heritage: Option<&Expression>,
        constructor: Option<&FunctionExpression>,
        elements: &[ClassElement],
        name_binding: Option<ClassNameBindingIr>,
        name_inference: ClassNameInferenceIr,
    ) -> TypedExpr {
        let entry_state = self.class_evaluation_state();
        let (heritage_prefix, heritage) = match heritage {
            Some(expression) => {
                let (prefix, value) = self.lower_class_evaluation_operand(expression);
                (prefix, Some(value))
            }
            None => (self.empty_class_evaluation_prefix(), None),
        };
        let mut heritage_kind = ClassHeritageKind::None;
        let mut heritage_function_id = None;
        if let Some(heritage) = heritage.as_ref() {
            if heritage.possible_kinds == KindSet::from_kind(ValueKind::Null) {
                heritage_kind = ClassHeritageKind::Null;
            } else {
                if let Some(function_id) = self.resolve_single_function_target(heritage) {
                    if self
                        .function_signatures
                        .get(&function_id)
                        .is_some_and(|signature| signature.protocol.is_constructable())
                    {
                        heritage_function_id = Some(function_id);
                    }
                }
                heritage_kind = ClassHeritageKind::Constructable;
                if self.class_heritage_prototype_get_may_call_user_code(heritage) {
                    self.invalidate_unknown_user_code_effects();
                }
            }
        }

        let constructor_invocation_role = match (heritage_kind, constructor.is_some()) {
            (ClassHeritageKind::None, _) => ClassConstructorInvocationRole::Base,
            (ClassHeritageKind::Constructable | ClassHeritageKind::Null, true) => {
                ClassConstructorInvocationRole::ExplicitDerived
            }
            (ClassHeritageKind::Constructable | ClassHeritageKind::Null, false) => {
                ClassConstructorInvocationRole::SyntheticDerived
            }
        };

        let constructor_id = self
            .analysis
            .class_execution_ids
            .get(&constructor_execution_key)
            .cloned()
            .unwrap_or_else(|| {
                panic!("class constructor execution `{constructor_execution_key}` must be planned")
            });
        let display_name = self
            .analysis
            .class_display_name(&constructor_id, &class_name);
        let class_private_environment_id = self
            .analysis
            .class_private_environment_ids
            .get(&constructor_execution_key)
            .copied();
        let class_body_private_environment_id =
            class_private_environment_id.or(self.private_environment_id);
        let private_name_ids = class_private_environment_id
            .map(|environment_id| {
                self.analysis.private_environment_plans[&environment_id]
                    .bindings
                    .clone()
            })
            .unwrap_or_default();

        #[derive(Clone)]
        struct PublicMethodPlan<'b> {
            key: PropertyKeyIr,
            key_prefix: Option<ClassEvaluationPrefixIr>,
            function_id: FunctionId,
            method: &'b ClassMethodDefinition,
            placement: ClassMethodPlacementIr,
            kind: ClassMethodKindIr,
            execution_kind: FunctionExecutionKind,
        }

        #[derive(Clone)]
        struct FieldPlan<'b> {
            key: ClassFieldKeyIr,
            computed_key: Option<PropertyKeyIr>,
            key_prefix: Option<ClassEvaluationPrefixIr>,
            init_function_id: Option<FunctionId>,
            initializer: Option<&'b Expression>,
            placement: ClassMethodPlacementIr,
        }

        #[derive(Clone)]
        struct PrivateMethodPlan<'b> {
            private_name_id: PrivateNameId,
            function_id: FunctionId,
            method: &'b ClassMethodDefinition,
            placement: ClassMethodPlacementIr,
            kind: ClassMethodKindIr,
            execution_kind: FunctionExecutionKind,
        }

        #[derive(Clone)]
        struct PrivateFieldPlan<'b> {
            private_name_id: PrivateNameId,
            init_function_id: Option<FunctionId>,
            initializer: Option<&'b Expression>,
            placement: ClassMethodPlacementIr,
        }

        #[derive(Clone)]
        struct AutoAccessorPlan<'b> {
            key: ClassFieldKeyIr,
            computed_key: Option<PropertyKeyIr>,
            key_prefix: Option<ClassEvaluationPrefixIr>,
            backing_name: AutoAccessorBackingNameIr,
            functions: AutoAccessorFunctionPairIr,
            init_function_id: Option<FunctionId>,
            initializer: Option<&'b Expression>,
            placement: ClassMethodPlacementIr,
        }

        #[derive(Clone, Copy)]
        enum ClassElementOrder {
            PublicMethod(usize),
            PrivateMethod(usize),
            PublicField(usize),
            PrivateField(usize),
            AutoAccessor(usize),
            StaticBlock(usize),
        }

        let mut public_methods = Vec::<PublicMethodPlan<'_>>::new();
        let mut private_methods = Vec::<PrivateMethodPlan<'_>>::new();
        let mut fields = Vec::<FieldPlan<'_>>::new();
        let mut private_fields = Vec::<PrivateFieldPlan<'_>>::new();
        let mut auto_accessors = Vec::<AutoAccessorPlan<'_>>::new();
        let mut static_blocks = Vec::<(FunctionId, &StaticBlockBody)>::new();
        let mut element_order = Vec::<ClassElementOrder>::new();
        let mut instance_private_method_brands = BTreeSet::<PrivateNameId>::new();
        let mut static_private_method_brands = BTreeSet::<PrivateNameId>::new();
        let mut computed_field_key_count = 0u32;

        let private_environment_plan = class_private_environment_id
            .map(|environment_id| self.analysis.private_environment_plans[&environment_id].clone());
        for (element_index, element) in elements.iter().enumerate() {
            match element {
                ClassElement::MethodDefinition(method) => {
                    let (kind, execution_kind) = match method.kind() {
                        MethodDefinitionKind::Ordinary => {
                            (ClassMethodKindIr::Method, FunctionExecutionKind::Ordinary)
                        }
                        MethodDefinitionKind::Get => {
                            (ClassMethodKindIr::Getter, FunctionExecutionKind::Ordinary)
                        }
                        MethodDefinitionKind::Set => {
                            (ClassMethodKindIr::Setter, FunctionExecutionKind::Ordinary)
                        }
                        MethodDefinitionKind::Generator
                            if generator_function_is_aot_supported(
                                method.body(),
                                method.parameters(),
                            ) =>
                        {
                            (ClassMethodKindIr::Method, FunctionExecutionKind::Generator)
                        }
                        MethodDefinitionKind::Async => {
                            (ClassMethodKindIr::Method, FunctionExecutionKind::Async)
                        }
                        MethodDefinitionKind::AsyncGenerator => (
                            ClassMethodKindIr::Method,
                            FunctionExecutionKind::AsyncGenerator,
                        ),
                        MethodDefinitionKind::Generator => {
                            return self.unsupported_expr("async or generator class element");
                        }
                    };
                    let placement = if method.is_static() {
                        ClassMethodPlacementIr::Static
                    } else {
                        ClassMethodPlacementIr::Instance
                    };
                    match method.name() {
                        ClassElementName::PropertyName(name) => {
                            let (key, key_prefix) = match name {
                                PropertyName::Literal(name) => (
                                    PropertyKeyIr::StaticString(
                                        self.interned_runtime_string(name.sym()),
                                    ),
                                    None,
                                ),
                                PropertyName::Computed(expr) => {
                                    let Some(key) = self.lower_class_property_key(
                                        expr,
                                        class_body_private_environment_id,
                                    ) else {
                                        return self.unsupported_expr("computed class method");
                                    };
                                    key
                                }
                            };
                            let method_index = public_methods.len();
                            public_methods.push(PublicMethodPlan {
                                key,
                                key_prefix,
                                function_id: self
                                    .analysis
                                    .class_execution_ids
                                    .get(&class_method_key(method))
                                    .cloned()
                                    .unwrap_or_else(|| {
                                        panic!(
                                            "class method execution `{}` must be planned",
                                            class_method_key(method)
                                        )
                                    }),
                                method,
                                placement,
                                kind,
                                execution_kind,
                            });
                            element_order.push(ClassElementOrder::PublicMethod(method_index));
                        }
                        ClassElementName::PrivateName(name) => {
                            let key = private_name_key(self.interner, *name);
                            let private_name_id =
                                private_name_ids.get(&key).copied().unwrap_or_else(|| {
                                    panic!("class private method `#{key}` must be planned")
                                });
                            if placement == ClassMethodPlacementIr::Static {
                                static_private_method_brands.insert(private_name_id);
                            } else {
                                instance_private_method_brands.insert(private_name_id);
                            }
                            let method_index = private_methods.len();
                            private_methods.push(PrivateMethodPlan {
                                private_name_id,
                                function_id: self
                                    .analysis
                                    .class_execution_ids
                                    .get(&class_method_key(method))
                                    .cloned()
                                    .unwrap_or_else(|| {
                                        panic!(
                                            "class method execution `{}` must be planned",
                                            class_method_key(method)
                                        )
                                    }),
                                method,
                                placement,
                                kind,
                                execution_kind,
                            });
                            element_order.push(ClassElementOrder::PrivateMethod(method_index));
                        }
                    }
                }
                ClassElement::FieldDefinition(field)
                | ClassElement::StaticFieldDefinition(field) => {
                    let (key, computed_key, key_prefix) = match field.name() {
                        PropertyName::Literal(name) => (
                            ClassFieldKeyIr::Public(self.interned_runtime_string(name.sym())),
                            None,
                            None,
                        ),
                        PropertyName::Computed(expr) => {
                            let Some((computed_key, key_prefix)) = self
                                .lower_class_property_key(expr, class_body_private_environment_id)
                            else {
                                return self.unsupported_expr("computed class field");
                            };
                            let slot = computed_field_key_count;
                            computed_field_key_count += 1;
                            (
                                ClassFieldKeyIr::ComputedPublic(slot),
                                Some(computed_key),
                                key_prefix,
                            )
                        }
                    };
                    let placement = if matches!(element, ClassElement::StaticFieldDefinition(_)) {
                        ClassMethodPlacementIr::Static
                    } else {
                        ClassMethodPlacementIr::Instance
                    };
                    let field_index = fields.len();
                    fields.push(FieldPlan {
                        key,
                        computed_key,
                        key_prefix,
                        init_function_id: field.initializer().map(|initializer| {
                            let execution_key = class_field_initializer_key(initializer);
                            self.analysis
                                .class_execution_ids
                                .get(&execution_key)
                                .cloned()
                                .unwrap_or_else(|| {
                                    panic!(
                                        "class field execution `{execution_key}` must be planned"
                                    )
                                })
                        }),
                        initializer: field.initializer(),
                        placement,
                    });
                    element_order.push(ClassElementOrder::PublicField(field_index));
                }
                ClassElement::PrivateFieldDefinition(field)
                | ClassElement::PrivateStaticFieldDefinition(field) => {
                    let key = private_name_key(self.interner, *field.name());
                    let private_name_id = private_name_ids
                        .get(&key)
                        .copied()
                        .unwrap_or_else(|| panic!("class private field `#{key}` must be planned"));
                    let placement =
                        if matches!(element, ClassElement::PrivateStaticFieldDefinition(_)) {
                            ClassMethodPlacementIr::Static
                        } else {
                            ClassMethodPlacementIr::Instance
                        };
                    let init_function_id = field.initializer().map(|initializer| {
                        let execution_key = class_field_initializer_key(initializer);
                        self.analysis
                            .class_execution_ids
                            .get(&execution_key)
                            .cloned()
                            .unwrap_or_else(|| {
                                panic!("class field execution `{execution_key}` must be planned")
                            })
                    });
                    if field.kind() == boa_ast::function::PrivateFieldDefinitionKind::AutoAccessor {
                        if !field.decorators().is_empty() {
                            return self.unsupported_expr("decorated auto-accessor class field");
                        }
                        let backing_name = private_environment_plan
                            .as_ref()
                            .and_then(|plan| {
                                plan.auto_accessor_backings.get(&element_index).copied()
                            })
                            .expect("private auto-accessor backing name must be planned");
                        let accessor_index = auto_accessors.len();
                        auto_accessors.push(AutoAccessorPlan {
                            key: ClassFieldKeyIr::Private(private_name_id),
                            computed_key: None,
                            key_prefix: None,
                            backing_name,
                            functions: AutoAccessorFunctionPairIr::new(
                                self.alloc_generated_function_id("auto-accessor.get"),
                                self.alloc_generated_function_id("auto-accessor.set"),
                            ),
                            init_function_id,
                            initializer: field.initializer(),
                            placement,
                        });
                        if placement == ClassMethodPlacementIr::Static {
                            static_private_method_brands.insert(private_name_id);
                        } else {
                            instance_private_method_brands.insert(private_name_id);
                        }
                        element_order.push(ClassElementOrder::AutoAccessor(accessor_index));
                    } else {
                        let field_index = private_fields.len();
                        private_fields.push(PrivateFieldPlan {
                            private_name_id,
                            init_function_id,
                            initializer: field.initializer(),
                            placement,
                        });
                        element_order.push(ClassElementOrder::PrivateField(field_index));
                    }
                }
                ClassElement::StaticBlock(block) => {
                    let execution_key = class_static_block_key(block);
                    let function_id = self
                        .analysis
                        .class_execution_ids
                        .get(&execution_key)
                        .cloned()
                        .unwrap_or_else(|| {
                            panic!("class static block execution `{execution_key}` must be planned")
                        });
                    let block_index = static_blocks.len();
                    static_blocks.push((function_id, block));
                    element_order.push(ClassElementOrder::StaticBlock(block_index));
                }
                ClassElement::AccessorFieldDefinition(field)
                | ClassElement::StaticAccessorFieldDefinition(field) => {
                    if !field.decorators().is_empty() {
                        return self.unsupported_expr("decorated auto-accessor class field");
                    }
                    let (key, computed_key, key_prefix) = match field.name() {
                        PropertyName::Literal(name) => (
                            ClassFieldKeyIr::Public(self.interned_runtime_string(name.sym())),
                            None,
                            None,
                        ),
                        PropertyName::Computed(expr) => {
                            let Some((computed_key, key_prefix)) = self
                                .lower_class_property_key(expr, class_body_private_environment_id)
                            else {
                                return self.unsupported_expr("computed auto-accessor class field");
                            };
                            let slot = computed_field_key_count;
                            computed_field_key_count += 1;
                            (
                                ClassFieldKeyIr::ComputedPublic(slot),
                                Some(computed_key),
                                key_prefix,
                            )
                        }
                    };
                    let placement =
                        if matches!(element, ClassElement::StaticAccessorFieldDefinition(_)) {
                            ClassMethodPlacementIr::Static
                        } else {
                            ClassMethodPlacementIr::Instance
                        };
                    let backing_name = private_environment_plan
                        .as_ref()
                        .and_then(|plan| plan.auto_accessor_backings.get(&element_index).copied())
                        .expect("public auto-accessor backing name must be planned");
                    let accessor_index = auto_accessors.len();
                    auto_accessors.push(AutoAccessorPlan {
                        key,
                        computed_key,
                        key_prefix,
                        backing_name,
                        functions: AutoAccessorFunctionPairIr::new(
                            self.alloc_generated_function_id("auto-accessor.get"),
                            self.alloc_generated_function_id("auto-accessor.set"),
                        ),
                        init_function_id: field.initializer().map(|initializer| {
                            let execution_key = class_field_initializer_key(initializer);
                            self.analysis
                                .class_execution_ids
                                .get(&execution_key)
                                .cloned()
                                .unwrap_or_else(|| {
                                    panic!(
                                        "class field execution `{execution_key}` must be planned"
                                    )
                                })
                        }),
                        initializer: field.initializer(),
                        placement,
                    });
                    element_order.push(ClassElementOrder::AutoAccessor(accessor_index));
                }
            }
        }

        let mut class_callable_ids = BTreeSet::from([constructor_id.clone()]);
        class_callable_ids.extend(
            public_methods
                .iter()
                .map(|method| method.function_id.clone()),
        );
        class_callable_ids.extend(
            private_methods
                .iter()
                .map(|method| method.function_id.clone()),
        );
        class_callable_ids.extend(
            fields
                .iter()
                .filter_map(|field| field.init_function_id.clone()),
        );
        class_callable_ids.extend(
            private_fields
                .iter()
                .filter_map(|field| field.init_function_id.clone()),
        );
        class_callable_ids.extend(
            static_blocks
                .iter()
                .map(|(function_id, _)| function_id.clone()),
        );
        for accessor in &auto_accessors {
            class_callable_ids.insert(accessor.functions.getter().clone());
            class_callable_ids.insert(accessor.functions.setter().clone());
            if let Some(init_function_id) = &accessor.init_function_id {
                class_callable_ids.insert(init_function_id.clone());
            }
        }
        let prior_class_callable_flow_effects = class_callable_ids
            .iter()
            .filter_map(|function_id| {
                self.function_signatures
                    .get(function_id)
                    .map(|signature| (function_id.clone(), signature.source_call_flow_effects))
            })
            .collect::<BTreeMap<_, _>>();
        for function_id in &class_callable_ids {
            if let Some(signature) = self.function_signatures.get_mut(function_id) {
                signature.source_call_flow_effects = SourceCallFlowEffects::unobserved();
            }
        }

        let heritage_prototype = if heritage_kind == ClassHeritageKind::Constructable {
            heritage
                .as_ref()
                .and_then(|heritage| self.read_current_object_shape_property(heritage, "prototype"))
                .and_then(|property| match property {
                    ObjectShapeProperty::Data(info) => info.heap_shape,
                    ObjectShapeProperty::Accessor { .. } => None,
                })
        } else {
            None
        };
        let mut prototype_shape = ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: heritage_prototype.clone(),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        };
        for method in &public_methods {
            if method.placement != ClassMethodPlacementIr::Instance {
                continue;
            }
            let Some(key) = method.key.static_name() else {
                continue;
            };
            Self::insert_class_method_shape(
                &mut prototype_shape.properties,
                key.to_string(),
                method.function_id.clone(),
                method.kind,
            );
        }
        for method in &private_methods {
            if method.placement != ClassMethodPlacementIr::Instance {
                continue;
            }
            let key = private_data_key(method.private_name_id);
            Self::insert_class_method_shape(
                &mut prototype_shape.properties,
                key,
                method.function_id.clone(),
                method.kind,
            );
        }
        for accessor in &auto_accessors {
            if accessor.placement != ClassMethodPlacementIr::Instance {
                continue;
            }
            let key = match &accessor.key {
                ClassFieldKeyIr::Public(key) => key.clone(),
                ClassFieldKeyIr::Private(private_name_id) => private_data_key(*private_name_id),
                ClassFieldKeyIr::ComputedPublic(_) => continue,
            };
            Self::insert_class_method_shape(
                &mut prototype_shape.properties,
                key.clone(),
                accessor.functions.getter().clone(),
                ClassMethodKindIr::Getter,
            );
            Self::insert_class_method_shape(
                &mut prototype_shape.properties,
                key,
                accessor.functions.setter().clone(),
                ClassMethodKindIr::Setter,
            );
        }

        let prototype_heap = Some(Arc::new(HeapShape::Object(prototype_shape.clone())));
        // A synthetic derived constructor delegates construction to its heritage
        // constructor. Preserve that constructor's statically known instance
        // kind/shape (notably Array) while layering this class's prototype.
        // Explicit constructors still start from the ordinary object shape and
        // refine their result through their own return analysis.
        let inherited_instance =
            if constructor.is_none() && heritage_kind == ClassHeritageKind::Constructable {
                heritage_function_id
                    .as_ref()
                    .and_then(|id| self.function_signatures.get(id))
                    .map(|signature| self.function_construct_instance_info(signature))
                    .filter(|info| info.possible_kinds != KindSet::EMPTY)
            } else {
                None
            };
        let mut instance_info = Self::with_instance_prototype(
            inherited_instance.unwrap_or_else(|| {
                Self::fresh_constructed_instance_with_private_brands(
                    instance_private_method_brands.clone(),
                )
            }),
            prototype_heap.clone(),
        );
        // ArrayShape has no dedicated private-brand set; represent brands as
        // hidden properties so shape propagation remains intact for Array
        // subclasses while constructor prefix writes still refine the shape.
        if let Some(HeapShape::Array(array)) = instance_info.heap_shape.as_mut().map(Arc::make_mut)
        {
            for private_name_id in &instance_private_method_brands {
                array.properties.insert(
                    private_brand_key(*private_name_id),
                    ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Boolean)),
                );
            }
        }
        let prototype_info = Self::value_info_from_shape(prototype_heap.clone());

        let mut class_properties = BTreeMap::new();
        class_properties.insert(
            "prototype".to_string(),
            ObjectShapeProperty::Data(prototype_info.clone()),
        );
        for method in &public_methods {
            if method.placement != ClassMethodPlacementIr::Static {
                continue;
            }
            let Some(key) = method.key.static_name() else {
                continue;
            };
            Self::insert_class_method_shape(
                &mut class_properties,
                key.to_string(),
                method.function_id.clone(),
                method.kind,
            );
        }
        for method in &private_methods {
            if method.placement != ClassMethodPlacementIr::Static {
                continue;
            }
            let key = private_data_key(method.private_name_id);
            Self::insert_class_method_shape(
                &mut class_properties,
                key,
                method.function_id.clone(),
                method.kind,
            );
        }
        for accessor in &auto_accessors {
            if accessor.placement != ClassMethodPlacementIr::Static {
                continue;
            }
            let key = match &accessor.key {
                ClassFieldKeyIr::Public(key) => key.clone(),
                ClassFieldKeyIr::Private(private_name_id) => private_data_key(*private_name_id),
                ClassFieldKeyIr::ComputedPublic(_) => continue,
            };
            Self::insert_class_method_shape(
                &mut class_properties,
                key.clone(),
                accessor.functions.getter().clone(),
                ClassMethodKindIr::Getter,
            );
            Self::insert_class_method_shape(
                &mut class_properties,
                key,
                accessor.functions.setter().clone(),
                ClassMethodKindIr::Setter,
            );
        }

        let mut class_info = ValueInfo {
            kind: ValueKind::Function,
            possible_kinds: KindSet::from_kind(ValueKind::Function),
            heap_shape: Some(Arc::new(HeapShape::Object(ObjectShape {
                provenance: HeapShapeProvenance::Program,
                prototype: heritage.as_ref().and_then(|info| info.heap_shape.clone()),
                properties: class_properties,
                private_brands: static_private_method_brands,
                boxed_primitive: None,
            }))),
            function_targets: FunctionTargetKnowledge::exact(constructor_id.clone()),
        };
        if let (Some(class_name), Some(_)) = (&class_name, &name_binding) {
            self.set_binding_value_info(class_name, class_info.clone())
                .expect("named class binding must remain in scope while its elements are lowered");
        }

        let heritage_prototype_shape = if heritage_kind == ClassHeritageKind::Constructable {
            heritage
                .as_ref()
                .and_then(|heritage| self.read_current_object_shape_property(heritage, "prototype"))
                .and_then(|property| match property {
                    ObjectShapeProperty::Data(info) => info.heap_shape,
                    ObjectShapeProperty::Accessor { .. } => None,
                })
        } else {
            None
        };

        let empty_parameters = FormalParameterList::default();
        for element in &element_order {
            match element {
                ClassElementOrder::PublicField(index) => {
                    let field = &fields[*index];
                    if field.placement != ClassMethodPlacementIr::Static {
                        continue;
                    }
                    let field_info = if let Some(init_function_id) = &field.init_function_id {
                        let output = self.lower_generated_expr_function(
                            init_function_id.clone(),
                            format!(
                                "{}.field.{}",
                                class_name.clone().unwrap_or_else(|| "<class>".to_string()),
                                class_field_debug_key(&field.key)
                            ),
                            CallableToStringRepresentation::NativeAnonymous,
                            field.initializer.expect("field initializer should exist"),
                            class_field_initializer_name(&field.key, &private_name_ids),
                            class_info.clone(),
                            ClassElementExecutionKind::StaticFieldInitializer,
                            ClassLoweringContext {
                                private_name_ids: private_name_ids.clone(),
                                heritage_kind,
                                super_base_shape: None,
                                super_constructor_target: None,
                                is_static: true,
                                is_derived_constructor: false,
                            },
                        );
                        if let Some(next_class_info) = output.construct_this_info {
                            class_info = next_class_info;
                        }
                        output.return_info
                    } else {
                        ValueInfo::undefined()
                    };
                    if let (Some(key), Some(HeapShape::Object(shape))) = (
                        field.key.static_name(),
                        class_info.heap_shape.as_mut().map(Arc::make_mut),
                    ) {
                        shape
                            .properties
                            .insert(key.to_string(), ObjectShapeProperty::Data(field_info));
                    }
                }
                ClassElementOrder::PrivateField(index) => {
                    let field = &private_fields[*index];
                    if field.placement != ClassMethodPlacementIr::Static {
                        continue;
                    }
                    let hidden_key = private_data_key(field.private_name_id);
                    let field_info = if let Some(init_function_id) = &field.init_function_id {
                        let output = self.lower_generated_expr_function(
                            init_function_id.clone(),
                            format!(
                                "{}.field.{}",
                                class_name.clone().unwrap_or_else(|| "<class>".to_string()),
                                hidden_key
                            ),
                            CallableToStringRepresentation::NativeAnonymous,
                            field.initializer.expect("field initializer should exist"),
                            class_field_initializer_name(
                                &ClassFieldKeyIr::Private(field.private_name_id),
                                &private_name_ids,
                            ),
                            class_info.clone(),
                            ClassElementExecutionKind::StaticFieldInitializer,
                            ClassLoweringContext {
                                private_name_ids: private_name_ids.clone(),
                                heritage_kind,
                                super_base_shape: None,
                                super_constructor_target: None,
                                is_static: true,
                                is_derived_constructor: false,
                            },
                        );
                        if let Some(next_class_info) = output.construct_this_info {
                            class_info = next_class_info;
                        }
                        output.return_info
                    } else {
                        ValueInfo::undefined()
                    };
                    if let Some(HeapShape::Object(shape)) =
                        class_info.heap_shape.as_mut().map(Arc::make_mut)
                    {
                        shape
                            .properties
                            .insert(hidden_key, ObjectShapeProperty::Data(field_info));
                        shape.private_brands.insert(field.private_name_id);
                    }
                }
                ClassElementOrder::AutoAccessor(index) => {
                    let accessor = &auto_accessors[*index];
                    if accessor.placement != ClassMethodPlacementIr::Static {
                        continue;
                    }
                    let backing_name_id = accessor.backing_name.private_name_id();
                    let hidden_key = private_data_key(backing_name_id);
                    let field_info = if let Some(init_function_id) = &accessor.init_function_id {
                        let output = self.lower_generated_expr_function(
                            init_function_id.clone(),
                            format!(
                                "{}.auto-accessor.{hidden_key}",
                                class_name.clone().unwrap_or_else(|| "<class>".to_string())
                            ),
                            CallableToStringRepresentation::NativeAnonymous,
                            accessor
                                .initializer
                                .expect("accessor initializer should exist"),
                            class_field_initializer_name(&accessor.key, &private_name_ids),
                            class_info.clone(),
                            ClassElementExecutionKind::StaticFieldInitializer,
                            ClassLoweringContext {
                                private_name_ids: private_name_ids.clone(),
                                heritage_kind,
                                super_base_shape: None,
                                super_constructor_target: None,
                                is_static: true,
                                is_derived_constructor: false,
                            },
                        );
                        if let Some(next_class_info) = output.construct_this_info {
                            class_info = next_class_info;
                        }
                        output.return_info
                    } else {
                        ValueInfo::undefined()
                    };
                    if let Some(HeapShape::Object(shape)) =
                        class_info.heap_shape.as_mut().map(Arc::make_mut)
                    {
                        shape
                            .properties
                            .insert(hidden_key, ObjectShapeProperty::Data(field_info));
                        shape.private_brands.insert(backing_name_id);
                    }
                }
                ClassElementOrder::StaticBlock(index) => {
                    let (block_id, block) = &static_blocks[*index];
                    let output = self.lower_generated_ast_function(
                        block_id.clone(),
                        format!(
                            "{}.<static>",
                            class_name.clone().unwrap_or_else(|| "<class>".to_string())
                        ),
                        CallableToStringRepresentation::NativeAnonymous,
                        &empty_parameters,
                        block.statements(),
                        FunctionProtocolIr::OrdinaryCallOnly,
                        true,
                        ClassElementExecutionKind::StaticBlock,
                        class_info.clone(),
                        Some(class_info.clone()),
                        None,
                        false,
                        private_name_ids.clone(),
                        ClassLoweringContext {
                            private_name_ids: private_name_ids.clone(),
                            heritage_kind,
                            super_base_shape: heritage
                                .as_ref()
                                .and_then(|info| info.heap_shape.clone()),
                            super_constructor_target: heritage_function_id.clone(),
                            is_static: true,
                            is_derived_constructor: false,
                        },
                        Vec::new(),
                    );
                    if let Some(next_class_info) = output.construct_this_info {
                        class_info = next_class_info;
                    }
                }
                ClassElementOrder::PublicMethod(_) | ClassElementOrder::PrivateMethod(_) => {
                    continue;
                }
            }
            if let (Some(class_name), Some(_)) = (&class_name, &name_binding) {
                self.set_binding_value_info(class_name, class_info.clone())
                    .expect(
                        "named class binding must remain in scope while its elements are lowered",
                    );
            }
        }

        for element in &element_order {
            match element {
                ClassElementOrder::PublicField(index) => {
                    let field = &fields[*index];
                    if field.placement != ClassMethodPlacementIr::Instance {
                        continue;
                    }
                    let field_info = if let Some(init_function_id) = &field.init_function_id {
                        self.lower_generated_expr_function(
                            init_function_id.clone(),
                            format!(
                                "{}.field.{}",
                                class_name.clone().unwrap_or_else(|| "<class>".to_string()),
                                class_field_debug_key(&field.key)
                            ),
                            CallableToStringRepresentation::NativeAnonymous,
                            field.initializer.expect("field initializer should exist"),
                            class_field_initializer_name(&field.key, &private_name_ids),
                            instance_info.clone(),
                            ClassElementExecutionKind::InstanceFieldInitializer,
                            ClassLoweringContext {
                                private_name_ids: private_name_ids.clone(),
                                heritage_kind,
                                super_base_shape: None,
                                super_constructor_target: None,
                                is_static: false,
                                is_derived_constructor: false,
                            },
                        )
                        .return_info
                    } else {
                        ValueInfo::undefined()
                    };
                    let Some(key) = field.key.static_name() else {
                        continue;
                    };
                    match instance_info.heap_shape.as_mut().map(Arc::make_mut) {
                        Some(HeapShape::Object(shape)) => {
                            shape
                                .properties
                                .insert(key.to_string(), ObjectShapeProperty::Data(field_info));
                        }
                        Some(HeapShape::Array(array)) => {
                            array
                                .properties
                                .insert(key.to_string(), ObjectShapeProperty::Data(field_info));
                        }
                        None => {}
                    }
                }
                ClassElementOrder::PrivateField(index) => {
                    let field = &private_fields[*index];
                    if field.placement != ClassMethodPlacementIr::Instance {
                        continue;
                    }
                    let hidden_key = private_data_key(field.private_name_id);
                    let field_info = if let Some(init_function_id) = &field.init_function_id {
                        self.lower_generated_expr_function(
                            init_function_id.clone(),
                            format!(
                                "{}.field.{}",
                                class_name.clone().unwrap_or_else(|| "<class>".to_string()),
                                hidden_key
                            ),
                            CallableToStringRepresentation::NativeAnonymous,
                            field.initializer.expect("field initializer should exist"),
                            class_field_initializer_name(
                                &ClassFieldKeyIr::Private(field.private_name_id),
                                &private_name_ids,
                            ),
                            instance_info.clone(),
                            ClassElementExecutionKind::InstanceFieldInitializer,
                            ClassLoweringContext {
                                private_name_ids: private_name_ids.clone(),
                                heritage_kind,
                                super_base_shape: None,
                                super_constructor_target: None,
                                is_static: false,
                                is_derived_constructor: false,
                            },
                        )
                        .return_info
                    } else {
                        ValueInfo::undefined()
                    };
                    match instance_info.heap_shape.as_mut().map(Arc::make_mut) {
                        Some(HeapShape::Object(shape)) => {
                            shape
                                .properties
                                .insert(hidden_key, ObjectShapeProperty::Data(field_info));
                            shape.private_brands.insert(field.private_name_id);
                        }
                        Some(HeapShape::Array(array)) => {
                            array
                                .properties
                                .insert(hidden_key, ObjectShapeProperty::Data(field_info));
                            array.properties.insert(
                                private_brand_key(field.private_name_id),
                                ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Boolean)),
                            );
                        }
                        None => {}
                    }
                }
                ClassElementOrder::AutoAccessor(index) => {
                    let accessor = &auto_accessors[*index];
                    if accessor.placement != ClassMethodPlacementIr::Instance {
                        continue;
                    }
                    let backing_name_id = accessor.backing_name.private_name_id();
                    let hidden_key = private_data_key(backing_name_id);
                    let field_info = if let Some(init_function_id) = &accessor.init_function_id {
                        self.lower_generated_expr_function(
                            init_function_id.clone(),
                            format!(
                                "{}.auto-accessor.{hidden_key}",
                                class_name.clone().unwrap_or_else(|| "<class>".to_string())
                            ),
                            CallableToStringRepresentation::NativeAnonymous,
                            accessor
                                .initializer
                                .expect("accessor initializer should exist"),
                            class_field_initializer_name(&accessor.key, &private_name_ids),
                            instance_info.clone(),
                            ClassElementExecutionKind::InstanceFieldInitializer,
                            ClassLoweringContext {
                                private_name_ids: private_name_ids.clone(),
                                heritage_kind,
                                super_base_shape: None,
                                super_constructor_target: None,
                                is_static: false,
                                is_derived_constructor: false,
                            },
                        )
                        .return_info
                    } else {
                        ValueInfo::undefined()
                    };
                    match instance_info.heap_shape.as_mut().map(Arc::make_mut) {
                        Some(HeapShape::Object(shape)) => {
                            shape
                                .properties
                                .insert(hidden_key, ObjectShapeProperty::Data(field_info));
                            shape.private_brands.insert(backing_name_id);
                        }
                        Some(HeapShape::Array(array)) => {
                            array
                                .properties
                                .insert(hidden_key, ObjectShapeProperty::Data(field_info));
                            array.properties.insert(
                                private_brand_key(backing_name_id),
                                ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Boolean)),
                            );
                        }
                        None => {}
                    }
                }
                ClassElementOrder::PublicMethod(_)
                | ClassElementOrder::PrivateMethod(_)
                | ClassElementOrder::StaticBlock(_) => continue,
            }
        }

        if let Some(HeapShape::Object(shape)) = class_info.heap_shape.as_mut().map(Arc::make_mut) {
            shape.properties.insert(
                "prototype".to_string(),
                ObjectShapeProperty::Data(prototype_info.clone()),
            );
        }

        let mut callable_instance_info = instance_info.clone();
        let mut callable_class_info = class_info.clone();
        // A method can run after a sibling callable has changed any private
        // field. Its receiver retains the class identities, but never a fresh
        // constructor's mutable field values. Initializers keep the fresh facts.
        for (private_name_id, placement) in private_fields
            .iter()
            .map(|field| (field.private_name_id, field.placement))
            .chain(
                auto_accessors
                    .iter()
                    .map(|accessor| (accessor.backing_name.private_name_id(), accessor.placement)),
            )
        {
            let receiver = match placement {
                ClassMethodPlacementIr::Instance => &mut callable_instance_info,
                ClassMethodPlacementIr::Static => &mut callable_class_info,
            };
            let properties = match receiver.heap_shape.as_mut().map(Arc::make_mut) {
                Some(HeapShape::Object(shape)) => &mut shape.properties,
                Some(HeapShape::Array(array)) => &mut array.properties,
                None => continue,
            };
            if let Some(ObjectShapeProperty::Data(value)) =
                properties.get_mut(&private_data_key(private_name_id))
            {
                *value = unknown_runtime_value_info();
            }
        }

        for method in &public_methods {
            let is_static = method.placement == ClassMethodPlacementIr::Static;
            let this_info = if is_static {
                callable_class_info.clone()
            } else {
                callable_instance_info.clone()
            };
            self.lower_generated_ast_function(
                method.function_id.clone(),
                format!(
                    "{}.{}",
                    class_name.clone().unwrap_or_else(|| "<class>".to_string()),
                    class_method_debug_key(&method.key)
                ),
                self.analysis
                    .module_execution
                    .class_method_to_string_representation(method.method, self.source_text),
                method.method.parameters(),
                method.method.body(),
                match method.kind {
                    ClassMethodKindIr::Method => {
                        FunctionProtocolIr::ClassMethod(method.execution_kind)
                    }
                    ClassMethodKindIr::Getter => FunctionProtocolIr::ClassGetter,
                    ClassMethodKindIr::Setter => FunctionProtocolIr::ClassSetter,
                },
                method.kind.is_method(),
                ClassElementExecutionKind::None,
                this_info,
                None,
                None,
                false,
                private_name_ids.clone(),
                ClassLoweringContext {
                    private_name_ids: private_name_ids.clone(),
                    heritage_kind,
                    super_base_shape: if is_static {
                        heritage.as_ref().and_then(|info| info.heap_shape.clone())
                    } else {
                        heritage_prototype_shape.clone()
                    },
                    super_constructor_target: heritage_function_id.clone(),
                    is_static,
                    is_derived_constructor: false,
                },
                Vec::new(),
            );
            if method.method.kind() == MethodDefinitionKind::Generator {
                self.mark_generated_function_as_generator(
                    &method.function_id,
                    linear_generator_plan(method.method.body())
                        .expect("registered generator method must have a linear plan"),
                );
            }
        }

        for method in &private_methods {
            let is_static = method.placement == ClassMethodPlacementIr::Static;
            let this_info = if is_static {
                callable_class_info.clone()
            } else {
                callable_instance_info.clone()
            };
            let ClassElementName::PrivateName(private_name) = method.method.name() else {
                unreachable!("private method plan must retain a private source name")
            };
            let private_name = private_name_key(self.interner, *private_name);
            let function_name = match method.kind {
                ClassMethodKindIr::Method => format!("#{private_name}"),
                ClassMethodKindIr::Getter => format!("get #{private_name}"),
                ClassMethodKindIr::Setter => format!("set #{private_name}"),
            };
            self.lower_generated_ast_function(
                method.function_id.clone(),
                function_name,
                self.analysis
                    .module_execution
                    .class_method_to_string_representation(method.method, self.source_text),
                method.method.parameters(),
                method.method.body(),
                match method.kind {
                    ClassMethodKindIr::Method => {
                        FunctionProtocolIr::ClassMethod(method.execution_kind)
                    }
                    ClassMethodKindIr::Getter => FunctionProtocolIr::ClassGetter,
                    ClassMethodKindIr::Setter => FunctionProtocolIr::ClassSetter,
                },
                method.kind.is_method(),
                ClassElementExecutionKind::None,
                this_info,
                None,
                None,
                heritage_kind != ClassHeritageKind::None,
                private_name_ids.clone(),
                ClassLoweringContext {
                    private_name_ids: private_name_ids.clone(),
                    heritage_kind,
                    super_base_shape: if is_static {
                        heritage.as_ref().and_then(|info| info.heap_shape.clone())
                    } else {
                        heritage_prototype_shape.clone()
                    },
                    super_constructor_target: heritage_function_id.clone(),
                    is_static,
                    is_derived_constructor: false,
                },
                Vec::new(),
            );
            if method.method.kind() == MethodDefinitionKind::Generator {
                self.mark_generated_function_as_generator(
                    &method.function_id,
                    linear_generator_plan(method.method.body())
                        .expect("registered generator method must have a linear plan"),
                );
            }
        }

        for accessor in &auto_accessors {
            let is_static = accessor.placement == ClassMethodPlacementIr::Static;
            let this_info = if is_static {
                callable_class_info.clone()
            } else {
                callable_instance_info.clone()
            };
            let exposed_name = match &accessor.key {
                ClassFieldKeyIr::Public(name) => name.clone(),
                ClassFieldKeyIr::ComputedPublic(_) => "<computed>".to_string(),
                ClassFieldKeyIr::Private(private_name_id) => private_name_ids
                    .iter()
                    .find_map(|(name, id)| (*id == *private_name_id).then(|| format!("#{name}")))
                    .expect("private auto-accessor source name must remain visible"),
            };
            self.lower_generated_auto_accessor_function(
                accessor.functions.getter().clone(),
                format!("get {exposed_name}"),
                FunctionProtocolIr::ClassGetter,
                accessor.backing_name,
                this_info.clone(),
                private_name_ids.clone(),
                heritage_kind,
                is_static,
            );
            self.lower_generated_auto_accessor_function(
                accessor.functions.setter().clone(),
                format!("set {exposed_name}"),
                FunctionProtocolIr::ClassSetter,
                accessor.backing_name,
                this_info,
                private_name_ids.clone(),
                heritage_kind,
                is_static,
            );
        }

        let instance_elements = element_order
            .iter()
            .filter_map(|element| match element {
                ClassElementOrder::PublicField(index) => {
                    let field = &fields[*index];
                    (field.placement == ClassMethodPlacementIr::Instance).then(|| {
                        ClassInstanceElementIr::Field(ClassFieldInitIr {
                            key: field.key.clone(),
                            init_function_id: field.init_function_id.clone(),
                        })
                    })
                }
                ClassElementOrder::PrivateField(index) => {
                    let field = &private_fields[*index];
                    (field.placement == ClassMethodPlacementIr::Instance).then(|| {
                        ClassInstanceElementIr::Field(ClassFieldInitIr {
                            key: ClassFieldKeyIr::Private(field.private_name_id),
                            init_function_id: field.init_function_id.clone(),
                        })
                    })
                }
                ClassElementOrder::AutoAccessor(index) => {
                    let accessor = &auto_accessors[*index];
                    (accessor.placement == ClassMethodPlacementIr::Instance).then(|| {
                        ClassInstanceElementIr::AutoAccessorBacking(
                            ClassAutoAccessorBackingInitIr {
                                backing_name: accessor.backing_name,
                                init_function_id: accessor.init_function_id.clone(),
                            },
                        )
                    })
                }
                ClassElementOrder::PublicMethod(_)
                | ClassElementOrder::PrivateMethod(_)
                | ClassElementOrder::StaticBlock(_) => None,
            })
            .collect::<Vec<_>>();
        let class_instance_element_plan = (!instance_private_method_brands.is_empty()
            || !instance_elements.is_empty())
        .then(|| ClassInstanceElementPlanIr {
            private_method_brands: instance_private_method_brands.iter().copied().collect(),
            elements: instance_elements,
        });

        let constructor_output = if let Some(constructor) = constructor {
            self.lower_generated_ast_function(
                constructor_id.clone(),
                display_name.unwrap_or_default().to_string(),
                CallableToStringRepresentation::ExactSource(class_source.clone()),
                constructor.parameters(),
                constructor.body(),
                FunctionProtocolIr::ClassConstructor,
                false,
                ClassElementExecutionKind::None,
                instance_info.clone(),
                Some(instance_info.clone()),
                None,
                false,
                private_name_ids.clone(),
                ClassLoweringContext {
                    private_name_ids: private_name_ids.clone(),
                    heritage_kind,
                    super_base_shape: heritage_prototype_shape.clone(),
                    super_constructor_target: heritage_function_id.clone(),
                    is_static: false,
                    is_derived_constructor: heritage_kind != ClassHeritageKind::None,
                },
                Vec::new(),
            )
        } else {
            let constructor_output = self.lower_generated_block_function(
                constructor_id.clone(),
                display_name.unwrap_or_default().to_string(),
                CallableToStringRepresentation::ExactSource(class_source.clone()),
                FunctionProtocolIr::ClassConstructor,
                false,
                ClassElementExecutionKind::None,
                instance_info.clone(),
                Some(instance_info.clone()),
                ClassLoweringContext {
                    private_name_ids: private_name_ids.clone(),
                    heritage_kind,
                    super_base_shape: heritage_prototype_shape.clone(),
                    super_constructor_target: heritage_function_id.clone(),
                    is_static: false,
                    is_derived_constructor: heritage_kind != ClassHeritageKind::None,
                },
                Vec::new(),
                Vec::new(),
            );
            if let Some(function_ir) = self
                .generated_functions
                .iter_mut()
                .find(|function| function.id == constructor_id)
            {
                function_ir.is_synthetic_default_derived_constructor =
                    heritage_kind != ClassHeritageKind::None;
            }
            constructor_output
        };

        self.finalize_class_callable_flow(
            prior_class_callable_flow_effects,
            &constructor_id,
            constructor_invocation_role,
            class_instance_element_plan.as_ref(),
        );

        if let Some(function_ir) = self
            .generated_functions
            .iter_mut()
            .find(|function| function.id == constructor_id)
        {
            function_ir.class_instance_element_plan = class_instance_element_plan;
        }

        if constructor_output.construct_this_info.is_some() {
            if let Some(HeapShape::Object(shape)) =
                class_info.heap_shape.as_mut().map(Arc::make_mut)
            {
                shape.properties.insert(
                    "prototype".to_string(),
                    ObjectShapeProperty::Data(prototype_info.clone()),
                );
            }
        }

        let mut element_prefixes = BTreeMap::new();
        let mut definitions = Vec::new();
        for element in &element_order {
            let mut key_prefix = None;
            let definition = match element {
                ClassElementOrder::PublicMethod(index) => {
                    let method = &public_methods[*index];
                    key_prefix = method.key_prefix.as_ref();
                    Some(ClassElementDefinitionIr::PublicMethod(
                        ClassPublicMethodIr {
                            key: method.key.clone(),
                            function_id: method.function_id.clone(),
                            placement: method.placement,
                            kind: method.kind,
                        },
                    ))
                }
                ClassElementOrder::PrivateMethod(index) => {
                    let method = &private_methods[*index];
                    Some(ClassElementDefinitionIr::PrivateMethod(
                        ClassPrivateMethodIr {
                            private_name_id: method.private_name_id,
                            function_id: method.function_id.clone(),
                            placement: method.placement,
                            kind: method.kind,
                        },
                    ))
                }
                ClassElementOrder::PublicField(index) => {
                    let field = &fields[*index];
                    key_prefix = field.key_prefix.as_ref();
                    field.computed_key.clone().map(|key| {
                        let ClassFieldKeyIr::ComputedPublic(slot) = &field.key else {
                            unreachable!("computed field key must use a cache slot")
                        };
                        ClassElementDefinitionIr::ComputedFieldKey { slot: *slot, key }
                    })
                }
                ClassElementOrder::AutoAccessor(index) => {
                    let accessor = &auto_accessors[*index];
                    key_prefix = accessor.key_prefix.as_ref();
                    Some(ClassElementDefinitionIr::AutoAccessor(
                        ClassAutoAccessorIr {
                            key: accessor.key.clone(),
                            computed_key: accessor.computed_key.clone(),
                            backing_name: accessor.backing_name,
                            functions: accessor.functions.clone(),
                            init_function_id: accessor.init_function_id.clone(),
                            placement: accessor.placement,
                        },
                    ))
                }
                ClassElementOrder::PrivateField(_) | ClassElementOrder::StaticBlock(_) => None,
            };
            if let Some(definition) = definition {
                if let Some(prefix) = key_prefix {
                    element_prefixes.insert(definitions.len(), prefix.clone());
                }
                definitions.push(definition);
            }
        }
        let expression = TypedExpr::from_info(
            class_info,
            ExprIr::ClassDefinition(Box::new(ClassDefinitionIr {
                name: display_name.map(str::to_string),
                name_binding,
                name_inference,
                constructor_function_id: constructor_id,
                explicit_constructor: constructor.is_some(),
                heritage_kind,
                heritage: heritage.map(Box::new),
                element_plan: ClassElementPlanIr {
                    definitions,
                    static_elements: element_order
                        .iter()
                        .filter_map(|element| match element {
                            ClassElementOrder::PublicField(index) => {
                                let field = &fields[*index];
                                (field.placement == ClassMethodPlacementIr::Static).then(|| {
                                    ClassStaticElementIr::Field(ClassFieldInitIr {
                                        key: field.key.clone(),
                                        init_function_id: field.init_function_id.clone(),
                                    })
                                })
                            }
                            ClassElementOrder::PrivateField(index) => {
                                let field = &private_fields[*index];
                                (field.placement == ClassMethodPlacementIr::Static).then(|| {
                                    ClassStaticElementIr::Field(ClassFieldInitIr {
                                        key: ClassFieldKeyIr::Private(field.private_name_id),
                                        init_function_id: field.init_function_id.clone(),
                                    })
                                })
                            }
                            ClassElementOrder::AutoAccessor(index) => {
                                let accessor = &auto_accessors[*index];
                                (accessor.placement == ClassMethodPlacementIr::Static).then(|| {
                                    ClassStaticElementIr::AutoAccessorBacking(
                                        ClassAutoAccessorBackingInitIr {
                                            backing_name: accessor.backing_name,
                                            init_function_id: accessor.init_function_id.clone(),
                                        },
                                    )
                                })
                            }
                            ClassElementOrder::StaticBlock(index) => {
                                Some(ClassStaticElementIr::Block(ClassStaticBlockIr {
                                    function_id: static_blocks[*index].0.clone(),
                                }))
                            }
                            ClassElementOrder::PublicMethod(_)
                            | ClassElementOrder::PrivateMethod(_) => None,
                        })
                        .collect(),
                },
                private_name_ids,
                private_environment: private_environment_plan
                    .as_ref()
                    .map(|plan| ClassPrivateEnvironmentIr::new(plan.id.0, plan.slot_count)),
            })),
        );
        self.finish_class_evaluation(expression, entry_state, heritage_prefix, element_prefixes)
    }
}
