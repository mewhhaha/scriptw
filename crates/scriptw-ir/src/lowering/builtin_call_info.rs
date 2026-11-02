use super::invocation_effects::StandardBuiltinCallAnalysis;
use super::promise_caller_flow::PromiseInvocationPolicy;
use super::*;

impl<'a> ScriptLowerer<'a> {
    pub(super) fn standard_builtin_call_info(
        &mut self,
        builtin: StandardBuiltinId,
        args: &[TypedExpr],
        context: BuiltinCallContext,
    ) -> Option<StandardBuiltinCallAnalysis> {
        if builtin.mutates_indexed_receiver() {
            self.record_caller_flow_invalidation();
        }
        self.record_boxed_builtin_invocation(builtin, &context);
        macro_rules! return_analysis {
            ($result:expr) => {
                return Some(StandardBuiltinCallAnalysis::Ordinary($result))
            };
        }
        let promise_invocation_policy = PromiseInvocationPolicy::for_call(builtin, args, &context);
        let result = match builtin {
            StandardBuiltinId::FunctionConstructor => Some(Self::empty_dynamic_function_info(
                DynamicFunctionKind::Ordinary,
            )),
            StandardBuiltinId::ShadowRealmConstructor => Some(Self::shadow_realm_instance_info()),
            StandardBuiltinId::ShadowRealmPrototypeImportValue => Some(
                Self::value_info_from_shape(Some(Self::promise_instance_shape())),
            ),
            StandardBuiltinId::ShadowRealmPrototypeEvaluate
            | StandardBuiltinId::ShadowRealmWrappedFunctionCall
            | StandardBuiltinId::ShadowRealmImportFulfilled
            | StandardBuiltinId::ShadowRealmImportRejected => Some(unknown_runtime_value_info()),
            StandardBuiltinId::EvalFunction => {
                unreachable!(
                    "dynamic-source builtins must consume their resolved disposition before builtin result analysis"
                )
            }
            StandardBuiltinId::FunctionPrototype | StandardBuiltinId::ErrorPrototypeStackSetter => {
                Some(ValueInfo::undefined())
            }
            StandardBuiltinId::ErrorPrototypeStackGetter => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::FunctionPrototypeSymbolHasInstance => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::PromiseConstructor => {
                if promise_invocation_policy.constructor_may_invoke_executor() {
                    if let Some(executor_id) = args
                        .first()
                        .and_then(|executor| self.resolve_single_function_target(executor))
                    {
                        self.merge_function_param_infos(
                            &executor_id,
                            &[
                                Self::standard_builtin_value_info(
                                    StandardBuiltinId::PromiseResolveFunction,
                                ),
                                Self::standard_builtin_value_info(
                                    StandardBuiltinId::PromiseRejectFunction,
                                ),
                            ],
                        );
                        self.merge_function_this_info(&executor_id, ValueInfo::undefined());
                    }
                }
                Some(Self::value_info_from_shape(Some(
                    Self::promise_instance_shape(),
                )))
            }
            StandardBuiltinId::PromiseResolve
            | StandardBuiltinId::PromiseTry
            | StandardBuiltinId::PromiseReject
            | StandardBuiltinId::PromiseAll
            | StandardBuiltinId::PromiseAllSettled
            | StandardBuiltinId::PromiseAllKeyed
            | StandardBuiltinId::PromiseAllSettledKeyed
            | StandardBuiltinId::PromiseAny
            | StandardBuiltinId::PromiseRace
            | StandardBuiltinId::ArrayFromAsync => Some(Self::value_info_from_shape(Some(
                Self::promise_instance_shape(),
            ))),
            StandardBuiltinId::PromiseWithResolvers => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::PromisePrototypeThen
            | StandardBuiltinId::PromisePrototypeCatch
            | StandardBuiltinId::PromisePrototypeFinally
            | StandardBuiltinId::PromiseThenFinally
            | StandardBuiltinId::PromiseCatchFinally
            | StandardBuiltinId::PromiseValueThunk
            | StandardBuiltinId::PromiseThrower
            | StandardBuiltinId::PromiseSpeciesGetter => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::PromiseCapabilityExecutor
            | StandardBuiltinId::PromiseAllResolveElement
            | StandardBuiltinId::PromiseAllSettledResolveElement
            | StandardBuiltinId::PromiseAllSettledRejectElement
            | StandardBuiltinId::PromiseAnyRejectElement
            | StandardBuiltinId::PromiseAllKeyedResolveElement
            | StandardBuiltinId::PromiseAllSettledKeyedResolveElement
            | StandardBuiltinId::PromiseAllSettledKeyedRejectElement
            | StandardBuiltinId::PromiseResolveFunction
            | StandardBuiltinId::PromiseRejectFunction
            | StandardBuiltinId::ArrayFromAsyncFulfilled
            | StandardBuiltinId::ArrayFromAsyncRejected => Some(ValueInfo::undefined()),
            StandardBuiltinId::MapConstructor => Some(Self::value_info_from_shape(Some(
                Self::map_instance_shape(),
            ))),
            StandardBuiltinId::MapSpeciesGetter => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::MapGroupBy => Some(Self::value_info_from_shape(Some(
                Self::map_instance_shape(),
            ))),
            StandardBuiltinId::ObjectGroupBy | StandardBuiltinId::ObjectFromEntries => {
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::MapPrototypeSet => Some(Self::value_info_from_shape(Some(
                Self::map_instance_shape(),
            ))),
            StandardBuiltinId::MapPrototypeClear | StandardBuiltinId::MapPrototypeForEach => {
                Some(ValueInfo::undefined())
            }
            StandardBuiltinId::MapPrototypeKeys
            | StandardBuiltinId::MapPrototypeValues
            | StandardBuiltinId::MapPrototypeEntries => Some(Self::value_info_from_shape(Some(
                Self::map_iterator_instance_shape(),
            ))),
            StandardBuiltinId::MapIteratorNext => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: Some(Box::new(Self::empty_object_shape())),
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::MapPrototypeDelete | StandardBuiltinId::MapPrototypeHas => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::MapPrototypeGet
            | StandardBuiltinId::MapPrototypeGetOrInsert
            | StandardBuiltinId::MapPrototypeGetOrInsertComputed => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::MapPrototypeSizeGetter => Some(ValueInfo::new(ValueKind::Number)),
            StandardBuiltinId::WeakMapConstructor => Some(Self::value_info_from_shape(Some(
                Self::weak_map_instance_shape(),
            ))),
            StandardBuiltinId::WeakMapPrototypeSet => Some(Self::value_info_from_shape(Some(
                Self::weak_map_instance_shape(),
            ))),
            StandardBuiltinId::WeakMapPrototypeDelete | StandardBuiltinId::WeakMapPrototypeHas => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::WeakMapPrototypeGet
            | StandardBuiltinId::WeakMapPrototypeGetOrInsert
            | StandardBuiltinId::WeakMapPrototypeGetOrInsertComputed => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::WeakSetConstructor => Some(Self::value_info_from_shape(Some(
                Self::weak_set_instance_shape(),
            ))),
            StandardBuiltinId::WeakSetPrototypeAdd => Some(Self::value_info_from_shape(Some(
                Self::weak_set_instance_shape(),
            ))),
            StandardBuiltinId::WeakSetPrototypeDelete | StandardBuiltinId::WeakSetPrototypeHas => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::WeakRefConstructor => Some(Self::value_info_from_shape(Some(
                Self::weak_ref_instance_shape(),
            ))),
            StandardBuiltinId::WeakRefPrototypeDeref => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::FinalizationRegistryConstructor => Some(
                Self::value_info_from_shape(Some(Self::finalization_registry_instance_shape())),
            ),
            StandardBuiltinId::FinalizationRegistryPrototypeRegister => {
                Some(ValueInfo::undefined())
            }
            StandardBuiltinId::FinalizationRegistryPrototypeUnregister => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            // Explicit-resource-management stack builtins. Every arm is
            // `Some`: `None` here is not "no static information", it is a
            // refusal — the three consumers read it as "this call does not
            // happen" and return `TypedExpr::undefined()` with the argument
            // vector replaced by `Vec::new()` (see the `let Some(...) else`
            // bindings at the `construct` and `call` sites). A `None` arm for a
            // builtin that must actually run silently drops the call.
            //
            // The kinds match `standard_builtin_signature` above; the
            // `IntlDateTimeFormatConstructor` arm is again the precedent for a
            // constructor with no instance shape.
            StandardBuiltinId::AsyncDisposableStackConstructor
            | StandardBuiltinId::DisposableStackConstructor
            | StandardBuiltinId::AsyncDisposableStackPrototypeMove
            | StandardBuiltinId::DisposableStackPrototypeMove
            | StandardBuiltinId::AsyncDisposableStackPrototypeDisposeAsync => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::AsyncDisposableStackPrototypeUse
            | StandardBuiltinId::AsyncDisposableStackPrototypeAdopt
            | StandardBuiltinId::DisposableStackPrototypeUse
            | StandardBuiltinId::DisposableStackPrototypeAdopt => {
                Some(ValueInfo::new(ValueKind::Dynamic))
            }
            StandardBuiltinId::AsyncDisposableStackPrototypeDefer
            | StandardBuiltinId::DisposableStackPrototypeDefer
            | StandardBuiltinId::DisposableStackPrototypeDispose
            | StandardBuiltinId::AsyncDisposableStackDisposeAsyncFulfilled
            | StandardBuiltinId::AsyncDisposableStackDisposeAsyncRejected => {
                Some(ValueInfo::undefined())
            }
            StandardBuiltinId::AsyncDisposableStackPrototypeDisposedGetter
            | StandardBuiltinId::DisposableStackPrototypeDisposedGetter => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::SetConstructor => Some(Self::value_info_from_shape(Some(
                Self::set_instance_shape(),
            ))),
            StandardBuiltinId::SetSpeciesGetter => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::SetPrototypeAdd => Some(Self::value_info_from_shape(Some(
                Self::set_instance_shape(),
            ))),
            StandardBuiltinId::SetPrototypeDifference
            | StandardBuiltinId::SetPrototypeIntersection
            | StandardBuiltinId::SetPrototypeSymmetricDifference
            | StandardBuiltinId::SetPrototypeUnion => Some(Self::value_info_from_shape(Some(
                Self::set_instance_shape(),
            ))),
            StandardBuiltinId::SetPrototypeIsDisjointFrom
            | StandardBuiltinId::SetPrototypeIsSubsetOf
            | StandardBuiltinId::SetPrototypeIsSupersetOf => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::SetPrototypeClear | StandardBuiltinId::SetPrototypeForEach => {
                Some(ValueInfo::undefined())
            }
            StandardBuiltinId::SetPrototypeValues | StandardBuiltinId::SetPrototypeEntries => Some(
                Self::value_info_from_shape(Some(Self::set_iterator_instance_shape())),
            ),
            StandardBuiltinId::SetIteratorNext => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: Some(Box::new(Self::empty_object_shape())),
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::SetPrototypeDelete | StandardBuiltinId::SetPrototypeHas => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::SetPrototypeSizeGetter => Some(ValueInfo::new(ValueKind::Number)),
            StandardBuiltinId::FunctionPrototypeCall => {
                if let Some(this_arg) = args.first() {
                    if this_arg
                        .possible_kinds
                        .is_subset_of(Self::boxed_primitive_kind_set())
                    {
                        self.boxed_receiver_adaptations += 1;
                    }
                }
                Some(ValueInfo {
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::all_runtime_tags(),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                })
            }
            StandardBuiltinId::FunctionPrototypeApply => {
                if let Some(this_arg) = args.first() {
                    if this_arg
                        .possible_kinds
                        .is_subset_of(Self::boxed_primitive_kind_set())
                    {
                        self.boxed_receiver_adaptations += 1;
                    }
                }
                Some(ValueInfo {
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::all_runtime_tags(),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                })
            }
            StandardBuiltinId::FunctionPrototypeBind => Some(ValueInfo::new(ValueKind::Function)),
            StandardBuiltinId::ObjectConstructor => {
                if let Some(arg) = args.first() {
                    let nullish = KindSet::from_kind(ValueKind::Undefined)
                        .union(KindSet::from_kind(ValueKind::Null));
                    if arg
                        .possible_kinds
                        .is_subset_of(Self::object_like_kind_set())
                    {
                        return_analysis!(arg.value_info());
                    }
                    if arg.possible_kinds.is_subset_of(nullish) {
                        return_analysis!(Self::fresh_constructed_instance_info());
                    }
                    if arg
                        .possible_kinds
                        .is_subset_of(Self::boxed_primitive_kind_set())
                    {
                        return_analysis!(Self::boxed_primitive_instance_info(arg.value_info()));
                    }
                    Some(Self::unknown_construct_result_info())
                } else {
                    Some(Self::fresh_constructed_instance_info())
                }
            }
            StandardBuiltinId::ObjectCreate => {
                if args.get(1).is_some_and(|properties| {
                    properties.possible_kinds != KindSet::from_kind(ValueKind::Undefined)
                }) {
                    self.invalidate_unknown_user_code_effects();
                    return_analysis!(ValueInfo {
                        kind: ValueKind::Object,
                        possible_kinds: KindSet::from_kind(ValueKind::Object),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    });
                }
                let Some(proto) = args.first() else {
                    return_analysis!(ValueInfo {
                        kind: ValueKind::Object,
                        possible_kinds: KindSet::from_kind(ValueKind::Object),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    });
                };
                let null_kind = KindSet::from_kind(ValueKind::Null);
                let allowed = Self::object_like_kind_set().union(null_kind);
                if !proto.possible_kinds.is_subset_of(allowed) {
                    return_analysis!(ValueInfo {
                        kind: ValueKind::Object,
                        possible_kinds: KindSet::from_kind(ValueKind::Object),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    });
                }
                if proto.possible_kinds == null_kind {
                    return_analysis!(Self::value_info_from_shape(Some(Box::new(
                        HeapShape::Object(ObjectShape {
                            provenance: HeapShapeProvenance::Program,
                            prototype: None,
                            properties: BTreeMap::new(),
                            private_brands: BTreeSet::new(),
                            boxed_primitive: None,
                        }),
                    ))));
                }
                if proto.possible_kinds.contains(ValueKind::Null) {
                    return_analysis!(ValueInfo {
                        kind: ValueKind::Object,
                        possible_kinds: KindSet::from_kind(ValueKind::Object),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    });
                }
                let Some(prototype) = proto.heap_shape.clone() else {
                    return_analysis!(ValueInfo {
                        kind: ValueKind::Object,
                        possible_kinds: KindSet::from_kind(ValueKind::Object),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    });
                };
                Some(Self::value_info_from_shape(Some(Box::new(
                    HeapShape::Object(ObjectShape {
                        provenance: HeapShapeProvenance::Program,
                        prototype: Some(prototype),
                        properties: BTreeMap::new(),
                        private_brands: BTreeSet::new(),
                        boxed_primitive: None,
                    }),
                ))))
            }
            StandardBuiltinId::ObjectGetPrototypeOf => {
                let unknown_prototype = || ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object)
                        .union(KindSet::from_kind(ValueKind::Null)),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                };
                let Some(target) = args.first() else {
                    return_analysis!(unknown_prototype());
                };
                if target
                    .function_targets
                    .exact_targets()
                    .is_some_and(|targets| {
                        !targets.is_empty()
                            && targets.iter().all(|function_id| {
                                StandardBuiltinId::from_function_id(function_id)
                                    .is_some_and(Self::is_typed_array_constructor)
                            })
                    })
                {
                    return_analysis!(Self::standard_builtin_value_info(
                        StandardBuiltinId::TypedArrayConstructor,
                    ));
                }
                if let Some(prototype) = target.heap_shape.as_deref().map(|shape| match shape {
                    HeapShape::Object(object) => object.prototype.clone(),
                    HeapShape::Array(array) => array
                        .prototype
                        .clone()
                        .or_else(|| Some(Self::array_prototype_shape())),
                }) {
                    return_analysis!(prototype.map_or_else(unknown_prototype, |prototype| {
                        Self::value_info_from_shape(Some(prototype))
                    },));
                }
                Some(unknown_prototype())
            }
            StandardBuiltinId::ObjectSetPrototypeOf => {
                let mut result = args
                    .first()
                    .map(TypedExpr::value_info)
                    .unwrap_or_else(ValueInfo::undefined);
                result.heap_shape = None;
                self.invalidate_unknown_user_code_effects();
                Some(result)
            }
            StandardBuiltinId::ObjectDefineProperty => {
                return Some(self.object_define_property_call_analysis(args));
            }
            StandardBuiltinId::ObjectDefineProperties => {
                if args.first().is_some_and(|target| {
                    self.is_builtin_property_expr(target, ARRAY_NAME, "prototype")
                }) {
                    self.array_prototype_mutated = true;
                }
                let mut result = args
                    .first()
                    .map(TypedExpr::value_info)
                    .unwrap_or_else(|| ValueInfo::new(ValueKind::Object));
                result.heap_shape = None;
                self.invalidate_unknown_user_code_effects();
                Some(result)
            }
            StandardBuiltinId::ObjectGetOwnPropertyDescriptor => {
                // Shape-specific results need a proven object target; any other
                // argument goes through the generic ToObject descriptor call.
                let proven_target = args.first().filter(|target| {
                    target.possible_kinds.is_subset_of(
                        Self::object_like_kind_set().union(KindSet::all_runtime_tags()),
                    )
                });
                if let Some(target) = proven_target {
                    if let Some(key_arg) = args.get(1) {
                        let key = match &key_arg.expr {
                            ExprIr::String(key) => Some(key.clone()),
                            ExprIr::WellKnownSymbol(symbol) => Some(shape_namespace_key(*symbol)),
                            _ => None,
                        };
                        if let Some(key) = key {
                            let is_species_symbol = matches!(
                                key_arg.expr,
                                ExprIr::WellKnownSymbol(WellKnownSymbol::Species)
                            );
                            let species_getter = if is_species_symbol
                                && target.function_targets.exact_single_target()
                                    == Some(&StandardBuiltinId::ArrayConstructor.function_id())
                            {
                                Some(StandardBuiltinId::ArraySpeciesGetter)
                            } else if is_species_symbol
                                && target.function_targets.exact_single_target()
                                    == Some(
                                        &StandardBuiltinId::ArrayBufferConstructor.function_id(),
                                    )
                            {
                                Some(StandardBuiltinId::ArrayBufferSpeciesGetter)
                            } else if is_species_symbol
                                && target.function_targets.exact_single_target()
                                    == Some(&StandardBuiltinId::RegExpConstructor.function_id())
                            {
                                Some(StandardBuiltinId::RegExpSpeciesGetter)
                            } else if is_species_symbol
                                && target.function_targets.exact_single_target()
                                    == Some(&StandardBuiltinId::PromiseConstructor.function_id())
                            {
                                Some(StandardBuiltinId::PromiseSpeciesGetter)
                            } else if is_species_symbol
                                && target.function_targets.exact_single_target()
                                    == Some(&StandardBuiltinId::MapConstructor.function_id())
                            {
                                Some(StandardBuiltinId::MapSpeciesGetter)
                            } else if is_species_symbol
                                && target.function_targets.exact_single_target()
                                    == Some(&StandardBuiltinId::SetConstructor.function_id())
                            {
                                Some(StandardBuiltinId::SetSpeciesGetter)
                            } else {
                                None
                            };
                            if let Some(species_getter) = species_getter {
                                return_analysis!(ValueInfo {
                                    kind: ValueKind::Object,
                                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                                    heap_shape: Some(Self::property_descriptor_shape(vec![
                                        ("get", Self::standard_builtin_value_info(species_getter)),
                                        ("set", ValueInfo::undefined()),
                                        ("enumerable", Self::boolean_value_info()),
                                        ("configurable", Self::boolean_value_info()),
                                    ])),
                                    function_targets: FunctionTargetKnowledge::none(),
                                });
                            }
                            if let Some(property) =
                                self.read_own_object_shape_property(target, &key)
                            {
                                let fields = match property {
                                    ObjectShapeProperty::Data(value) => vec![
                                        ("value", value),
                                        ("writable", Self::boolean_value_info()),
                                        ("enumerable", Self::boolean_value_info()),
                                        ("configurable", Self::boolean_value_info()),
                                    ],
                                    ObjectShapeProperty::Accessor { getter, setter } => vec![
                                        (
                                            "get",
                                            getter
                                                .map(|accessor| {
                                                    Self::function_value_info_with_constructable(
                                                        accessor.function_id,
                                                        false,
                                                    )
                                                })
                                                .unwrap_or_else(ValueInfo::undefined),
                                        ),
                                        (
                                            "set",
                                            setter
                                                .map(|accessor| {
                                                    Self::function_value_info_with_constructable(
                                                        accessor.function_id,
                                                        false,
                                                    )
                                                })
                                                .unwrap_or_else(ValueInfo::undefined),
                                        ),
                                        ("enumerable", Self::boolean_value_info()),
                                        ("configurable", Self::boolean_value_info()),
                                    ],
                                };
                                return_analysis!(ValueInfo {
                                    kind: ValueKind::Object,
                                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                                    heap_shape: Some(Self::property_descriptor_shape(fields)),
                                    function_targets: FunctionTargetKnowledge::none(),
                                });
                            }
                        }
                    }
                }
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object)
                        .union(KindSet::from_kind(ValueKind::Undefined)),
                    heap_shape: Some(Self::generic_property_descriptor_shape()),
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::ObjectAssign => {
                self.invalidate_unknown_user_code_effects();
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: Self::object_like_kind_set(),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                })
            }
            StandardBuiltinId::ObjectGetOwnPropertyDescriptors => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: Some(Box::new(Self::empty_object_shape())),
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::ObjectKeys => {
                let mut elements = Vec::new();
                if let Some(arg) = args.first() {
                    match arg.heap_shape.as_deref() {
                        Some(HeapShape::Array(shape)) => {
                            elements.extend((0..shape.elements.len()).map(|_| ValueInfo {
                                kind: ValueKind::String,
                                possible_kinds: KindSet::from_kind(ValueKind::String),
                                heap_shape: None,
                                function_targets: FunctionTargetKnowledge::none(),
                            }));
                            elements.extend(shape.properties.keys().map(|_| ValueInfo {
                                kind: ValueKind::String,
                                possible_kinds: KindSet::from_kind(ValueKind::String),
                                heap_shape: None,
                                function_targets: FunctionTargetKnowledge::none(),
                            }));
                        }
                        Some(HeapShape::Object(shape)) => {
                            elements.extend(shape.properties.keys().map(|_| ValueInfo {
                                kind: ValueKind::String,
                                possible_kinds: KindSet::from_kind(ValueKind::String),
                                heap_shape: None,
                                function_targets: FunctionTargetKnowledge::none(),
                            }));
                        }
                        _ => {}
                    }
                }
                Some(Self::array_value_info_from_elements(elements))
            }
            StandardBuiltinId::ObjectValues => {
                let mut elements = Vec::new();
                if let Some(arg) = args.first() {
                    match arg.heap_shape.as_deref() {
                        Some(HeapShape::Array(shape)) => {
                            elements.extend(shape.elements.iter().cloned());
                            elements.extend(shape.properties.values().filter_map(|property| {
                                match property {
                                    ObjectShapeProperty::Data(info) => Some(info.clone()),
                                    ObjectShapeProperty::Accessor { .. } => None,
                                }
                            }));
                        }
                        Some(HeapShape::Object(shape)) => {
                            elements.extend(shape.properties.values().filter_map(|property| {
                                match property {
                                    ObjectShapeProperty::Data(info) => Some(info.clone()),
                                    ObjectShapeProperty::Accessor { .. } => None,
                                }
                            }));
                        }
                        _ => {}
                    }
                }
                Some(Self::array_value_info_from_elements(elements))
            }
            StandardBuiltinId::ObjectEntries => {
                let shape = ArrayShape::default();
                Some(Self::value_info_from_shape(Some(Box::new(
                    HeapShape::Array(shape),
                ))))
            }
            StandardBuiltinId::ObjectIs
            | StandardBuiltinId::ObjectIsSealed
            | StandardBuiltinId::ObjectIsFrozen
            | StandardBuiltinId::ObjectIsExtensible => Some(ValueInfo::new(ValueKind::Boolean)),
            StandardBuiltinId::ObjectSeal
            | StandardBuiltinId::ObjectFreeze
            | StandardBuiltinId::ObjectPreventExtensions => {
                let mut result = args
                    .first()
                    .map(TypedExpr::value_info)
                    .unwrap_or_else(ValueInfo::undefined);
                result.heap_shape = None;
                self.invalidate_unknown_user_code_effects();
                Some(result)
            }
            StandardBuiltinId::ObjectPrototypeHasOwnProperty => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::ObjectPrototypeLookupGetter
            | StandardBuiltinId::ObjectPrototypeLookupSetter => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::Function)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::ObjectPrototypeProtoGetter => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object)
                    .union(KindSet::from_kind(ValueKind::Null)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::ObjectPrototypeDefineGetter
            | StandardBuiltinId::ObjectPrototypeDefineSetter
            | StandardBuiltinId::ObjectPrototypeProtoSetter => Some(ValueInfo::undefined()),
            StandardBuiltinId::ObjectPrototypePropertyIsEnumerable => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::ObjectPrototypeIsPrototypeOf => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::ObjectPrototypeValueOf => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: Self::object_like_kind_set(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::ErrorIsError => Some(ValueInfo::new(ValueKind::Boolean)),
            StandardBuiltinId::ObjectPrototypeToString => Some(ValueInfo::new(ValueKind::String)),
            StandardBuiltinId::ObjectPrototypeToLocaleString => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::ProxyConstructor => {
                if let (Some(target), Some(handler)) = (args.first(), args.get(1)) {
                    self.observe_proxy_handler_traps(target, handler);
                }
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                    // A Proxy is not an ordinary empty object: every property
                    // operation may dispatch a source trap. `None` preserves
                    // that exotic uncertainty through aliases and joins.
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                })
            }
            StandardBuiltinId::ProxyRevocable => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::ProxyRevoke => Some(ValueInfo::undefined()),
            StandardBuiltinId::ReflectConstruct => {
                self.invalidate_unknown_user_code_effects();
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                })
            }
            StandardBuiltinId::ReflectApply => {
                if let Some(target_builtin) = args
                    .first()
                    .filter(|target| {
                        matches!(
                            InvocationTargetProvenance::from(*target),
                            InvocationTargetProvenance::ProvenFunction(_)
                        )
                    })
                    .and_then(|target| self.resolve_single_function_target(target))
                    .and_then(|target| Self::define_property_builtin(&target))
                {
                    let forwarded_args = args.get(2).and_then(|array_like| {
                        matches!(&array_like.expr, ExprIr::ArrayLiteral(_))
                            .then(|| self.forwarded_apply_args(Some(array_like)))
                            .flatten()
                    });
                    return match forwarded_args {
                        Some(forwarded_args) => Some(self.forwarded_define_property_call_analysis(
                            target_builtin,
                            &forwarded_args,
                        )),
                        None => Some(
                            self.unknown_forwarded_define_property_call_analysis(target_builtin),
                        ),
                    };
                }
                let target_provenance = args
                    .first()
                    .map(InvocationTargetProvenance::from)
                    .unwrap_or(InvocationTargetProvenance::Erased);
                self.observe_unaccounted_invocation_effects(target_provenance);
                return Some(
                    StandardBuiltinCallAnalysis::with_accounted_invocation_effects(ValueInfo {
                        kind: ValueKind::Dynamic,
                        possible_kinds: KindSet::all_runtime_tags(),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::unknown(),
                    }),
                );
            }
            StandardBuiltinId::ReflectGet => {
                self.observe_all_planned_source_as_unknown_property_hooks();
                self.invalidate_unknown_user_code_effects();
                Some(ValueInfo {
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::all_runtime_tags(),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                })
            }
            StandardBuiltinId::ReflectGetPrototypeOf => {
                self.invalidate_unknown_user_code_effects();
                Some(ValueInfo {
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::from_kind(ValueKind::Object)
                        .union(KindSet::from_kind(ValueKind::Null)),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                })
            }
            StandardBuiltinId::ReflectGetOwnPropertyDescriptor => {
                self.observe_all_planned_source_as_unknown_property_hooks();
                self.invalidate_unknown_user_code_effects();
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object)
                        .union(KindSet::from_kind(ValueKind::Undefined)),
                    heap_shape: Some(Self::generic_property_descriptor_shape()),
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::ReflectDefineProperty => {
                return Some(self.reflect_define_property_call_analysis(args));
            }
            StandardBuiltinId::ReflectDeleteProperty => {
                if args.first().is_some_and(|target| {
                    self.is_builtin_property_expr(target, ARRAY_NAME, "prototype")
                }) {
                    self.array_prototype_mutated = true;
                }
                self.invalidate_unknown_user_code_effects();
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::ReflectSet => {
                if args.first().is_some_and(|target| {
                    self.is_builtin_property_expr(target, ARRAY_NAME, "prototype")
                }) {
                    self.array_prototype_mutated = true;
                }
                self.invalidate_unknown_user_code_effects();
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::ReflectHas => {
                self.observe_all_planned_source_as_unknown_property_hooks();
                self.invalidate_unknown_user_code_effects();
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::ReflectIsExtensible => Some(ValueInfo::new(ValueKind::Boolean)),
            StandardBuiltinId::ReflectPreventExtensions
            | StandardBuiltinId::ReflectSetPrototypeOf => {
                self.invalidate_unknown_user_code_effects();
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::ReflectOwnKeys => {
                let shape = ArrayShape::default();
                Some(Self::value_info_from_shape(Some(Box::new(
                    HeapShape::Array(shape),
                ))))
            }
            StandardBuiltinId::ArrayConstructor => {
                let mut shape = ArrayShape::default();
                if !(args.len() == 1
                    && args[0].possible_kinds == KindSet::from_kind(ValueKind::Number))
                {
                    shape
                        .elements
                        .extend(args.iter().map(TypedExpr::value_info));
                }
                Some(ValueInfo {
                    kind: ValueKind::Array,
                    possible_kinds: KindSet::from_kind(ValueKind::Array),
                    heap_shape: Some(Box::new(HeapShape::Array(shape))),
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::ArrayFrom => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: Self::object_like_kind_set(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::ArrayOf => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: Self::object_like_kind_set(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::IteratorFrom => {
                let base = args
                    .first()
                    .map(TypedExpr::value_info)
                    .unwrap_or(ValueInfo {
                        kind: ValueKind::Object,
                        possible_kinds: Self::object_like_kind_set(),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::unknown(),
                    });
                // GetIteratorFlattenable can invoke protocol getters/methods;
                // OrdinaryHasInstance can traverse a Proxy prototype even
                // when @@iterator and next are proven own data properties.
                // Keep the builtin's ordinary caller-effect accounting.
                Some(self.iterator_from_wrapper_value_info(base))
            }
            StandardBuiltinId::IteratorConcat => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: Some(Self::iterator_concat_helper_shape()),
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::IteratorZip | StandardBuiltinId::IteratorZipKeyed => {
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                    heap_shape: Some(Self::iterator_zip_helper_shape()),
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::ArrayIsArray => Some(ValueInfo {
                kind: ValueKind::Boolean,
                possible_kinds: KindSet::from_kind(ValueKind::Boolean),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::NumberIsInteger | StandardBuiltinId::NumberIsSafeInteger => {
                Some(ValueInfo {
                    kind: ValueKind::Boolean,
                    possible_kinds: KindSet::from_kind(ValueKind::Boolean),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::NumberIsNaN
            | StandardBuiltinId::NumberIsFinite
            | StandardBuiltinId::GlobalIsFinite
            | StandardBuiltinId::GlobalIsNaN => Some(ValueInfo {
                kind: ValueKind::Boolean,
                possible_kinds: KindSet::from_kind(ValueKind::Boolean),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::NumberPrototypeToExponential
            | StandardBuiltinId::NumberPrototypeToFixed
            | StandardBuiltinId::NumberPrototypeToPrecision
            | StandardBuiltinId::NumberPrototypeToString
            | StandardBuiltinId::NumberPrototypeToLocaleString => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::NumberPrototypeValueOf => Some(ValueInfo::new(ValueKind::Number)),
            StandardBuiltinId::BooleanPrototypeToString => Some(ValueInfo::new(ValueKind::String)),
            StandardBuiltinId::BooleanPrototypeValueOf => Some(ValueInfo::new(ValueKind::Boolean)),
            StandardBuiltinId::MathAbs
            | StandardBuiltinId::MathAcos
            | StandardBuiltinId::MathAcosh
            | StandardBuiltinId::MathAsin
            | StandardBuiltinId::MathAsinh
            | StandardBuiltinId::MathAtan
            | StandardBuiltinId::MathAtan2
            | StandardBuiltinId::MathAtanh
            | StandardBuiltinId::MathCbrt
            | StandardBuiltinId::MathCeil
            | StandardBuiltinId::MathClz32
            | StandardBuiltinId::MathCos
            | StandardBuiltinId::MathCosh
            | StandardBuiltinId::MathExp
            | StandardBuiltinId::MathExpm1
            | StandardBuiltinId::MathF16Round
            | StandardBuiltinId::MathFloor
            | StandardBuiltinId::MathFround
            | StandardBuiltinId::MathHypot
            | StandardBuiltinId::MathImul
            | StandardBuiltinId::MathLog
            | StandardBuiltinId::MathLog10
            | StandardBuiltinId::MathLog1p
            | StandardBuiltinId::MathLog2
            | StandardBuiltinId::MathPow
            | StandardBuiltinId::MathRandom
            | StandardBuiltinId::MathRound
            | StandardBuiltinId::MathSign
            | StandardBuiltinId::MathSin
            | StandardBuiltinId::MathSinh
            | StandardBuiltinId::MathSqrt
            | StandardBuiltinId::MathSumPrecise
            | StandardBuiltinId::MathTan
            | StandardBuiltinId::MathTanh
            | StandardBuiltinId::MathTrunc
            | StandardBuiltinId::MathMin
            | StandardBuiltinId::MathMax => Some(ValueInfo::new(ValueKind::Number)),
            StandardBuiltinId::ArrayPrototypeConcat
            | StandardBuiltinId::ArrayPrototypeSlice
            | StandardBuiltinId::ArrayPrototypeSplice => Some(Self::unshaped_array_result_info()),
            StandardBuiltinId::TypedArrayPrototypeFill
            | StandardBuiltinId::TypedArrayPrototypeReverse
            | StandardBuiltinId::TypedArrayPrototypeSort
            | StandardBuiltinId::TypedArrayPrototypeSubarray
            | StandardBuiltinId::TypedArrayPrototypeSlice
            | StandardBuiltinId::TypedArrayPrototypeToReversed
            | StandardBuiltinId::TypedArrayPrototypeToSorted
            | StandardBuiltinId::TypedArrayPrototypeWith
            | StandardBuiltinId::TypedArrayPrototypeCopyWithin => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::TypedArrayPrototypeSet => Some(ValueInfo::undefined()),
            StandardBuiltinId::TypedArrayPrototypeToString => Some(unknown_runtime_value_info()),
            StandardBuiltinId::ArrayPrototypeJoin
            | StandardBuiltinId::ArrayPrototypeToLocaleString
            | StandardBuiltinId::TypedArrayPrototypeJoin
            | StandardBuiltinId::Uint8ArrayPrototypeToBase64
            | StandardBuiltinId::Uint8ArrayPrototypeToHex
            | StandardBuiltinId::TypedArrayPrototypeToLocaleString => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::ArrayPrototypeFlat => Some(Self::unshaped_array_result_info()),
            StandardBuiltinId::ArrayPrototypeFlatMap => Some(Self::unshaped_array_result_info()),
            StandardBuiltinId::ArrayPrototypeAt | StandardBuiltinId::TypedArrayPrototypeAt => {
                Some(ValueInfo {
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::all_runtime_tags(),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                })
            }
            StandardBuiltinId::ArrayPrototypeToReversed => Some(Self::unshaped_array_result_info()),
            StandardBuiltinId::ArrayPrototypeWith => Some(Self::unshaped_array_result_info()),
            StandardBuiltinId::ArrayPrototypeToSpliced => Some(Self::unshaped_array_result_info()),
            StandardBuiltinId::ArrayPrototypeToSorted => Some(Self::unshaped_array_result_info()),
            StandardBuiltinId::ArrayPrototypeReverse
            | StandardBuiltinId::ArrayPrototypeCopyWithin => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: Self::object_like_kind_set(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::ArrayPrototypeIncludes
            | StandardBuiltinId::TypedArrayPrototypeIncludes => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::ArrayPrototypeIndexOf
            | StandardBuiltinId::TypedArrayPrototypeIndexOf => Some(ValueInfo {
                kind: ValueKind::Number,
                possible_kinds: KindSet::from_kind(ValueKind::Number),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::ArrayPrototypeLastIndexOf
            | StandardBuiltinId::TypedArrayPrototypeLastIndexOf => Some(ValueInfo {
                kind: ValueKind::Number,
                possible_kinds: KindSet::from_kind(ValueKind::Number),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::ArrayPrototypeFind | StandardBuiltinId::TypedArrayPrototypeFind => {
                Some(ValueInfo {
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::all_runtime_tags(),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                })
            }
            StandardBuiltinId::ArrayPrototypeFindIndex
            | StandardBuiltinId::TypedArrayPrototypeFindIndex => Some(ValueInfo {
                kind: ValueKind::Number,
                possible_kinds: KindSet::from_kind(ValueKind::Number),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::ArrayPrototypeFindLast
            | StandardBuiltinId::TypedArrayPrototypeFindLast => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::ArrayPrototypeFindLastIndex
            | StandardBuiltinId::TypedArrayPrototypeFindLastIndex => Some(ValueInfo {
                kind: ValueKind::Number,
                possible_kinds: KindSet::from_kind(ValueKind::Number),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::ArrayPrototypeEvery
            | StandardBuiltinId::TypedArrayPrototypeEvery => Some(ValueInfo {
                kind: ValueKind::Boolean,
                possible_kinds: KindSet::from_kind(ValueKind::Boolean),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::ArrayPrototypeSome | StandardBuiltinId::TypedArrayPrototypeSome => {
                Some(ValueInfo {
                    kind: ValueKind::Boolean,
                    possible_kinds: KindSet::from_kind(ValueKind::Boolean),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::ArrayPrototypeForEach
            | StandardBuiltinId::TypedArrayPrototypeForEach => Some(ValueInfo::undefined()),
            StandardBuiltinId::ArrayPrototypeFilter | StandardBuiltinId::ArrayPrototypeMap => {
                Some(Self::unshaped_array_result_info())
            }
            StandardBuiltinId::TypedArrayPrototypeMap
            | StandardBuiltinId::TypedArrayPrototypeFilter => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::ArrayPrototypeReduce
            | StandardBuiltinId::ArrayPrototypeReduceRight
            | StandardBuiltinId::TypedArrayPrototypeReduce
            | StandardBuiltinId::TypedArrayPrototypeReduceRight => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::ArrayPrototypePop => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::ArrayPrototypeShift => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::ArrayPrototypeFill | StandardBuiltinId::ArrayPrototypeSort => {
                Some(Self::unknown_construct_result_info())
            }
            StandardBuiltinId::ArrayPrototypePush => Some(ValueInfo {
                kind: ValueKind::Number,
                possible_kinds: KindSet::from_kind(ValueKind::Number),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::ArrayPrototypeUnshift => Some(ValueInfo {
                kind: ValueKind::Number,
                possible_kinds: KindSet::from_kind(ValueKind::Number),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::ArrayPrototypeKeys
            | StandardBuiltinId::ArrayPrototypeEntries
            | StandardBuiltinId::ArrayPrototypeValues
            | StandardBuiltinId::TypedArrayPrototypeKeys
            | StandardBuiltinId::TypedArrayPrototypeEntries
            | StandardBuiltinId::TypedArrayPrototypeValues
            | StandardBuiltinId::StringPrototypeIterator => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::IteratorConstructor => Some(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorConstructor,
            )),
            StandardBuiltinId::IteratorPrototypeToArray => Some(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeToArray,
            )),
            StandardBuiltinId::IteratorPrototypeForEach => Some(ValueInfo::undefined()),
            StandardBuiltinId::IteratorPrototypeIncludes => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::IteratorPrototypeJoin => Some(ValueInfo::new(ValueKind::String)),
            StandardBuiltinId::IteratorPrototypeEvery => Some(ValueInfo::new(ValueKind::Boolean)),
            StandardBuiltinId::IteratorPrototypeSome => Some(ValueInfo::new(ValueKind::Boolean)),
            StandardBuiltinId::IteratorPrototypeFind => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::IteratorPrototypeReduce => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::IteratorPrototypeMap => Some(Self::iterator_helper_info()),
            StandardBuiltinId::IteratorConcatNext
            | StandardBuiltinId::IteratorConcatReturn
            | StandardBuiltinId::IteratorZipNext
            | StandardBuiltinId::IteratorZipReturn
            | StandardBuiltinId::IteratorBufferedNext
            | StandardBuiltinId::IteratorBufferedReturn => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: Some(Box::new(Self::empty_object_shape())),
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::IteratorHelperNext | StandardBuiltinId::IteratorHelperReturn => {
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                    heap_shape: Some(Box::new(Self::empty_object_shape())),
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::IteratorPrototypeFilter => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: Some(Self::iterator_filter_helper_shape()),
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::IteratorPrototypeFlatMap => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: Some(Self::iterator_flat_map_helper_shape()),
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::IteratorPrototypeTake => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: Some(Self::iterator_take_helper_shape()),
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::IteratorPrototypeChunks
            | StandardBuiltinId::IteratorPrototypeWindows => Some(Self::iterator_helper_info()),
            StandardBuiltinId::IteratorPrototypeDrop => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: Some(Self::iterator_drop_helper_shape()),
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::IteratorTakeNext | StandardBuiltinId::IteratorTakeReturn => {
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                    heap_shape: Some(Box::new(Self::empty_object_shape())),
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::IteratorDropNext | StandardBuiltinId::IteratorDropReturn => {
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                    heap_shape: Some(Box::new(Self::empty_object_shape())),
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::IteratorMapNext | StandardBuiltinId::IteratorMapReturn => {
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                    heap_shape: Some(Box::new(Self::empty_object_shape())),
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::IteratorFilterNext | StandardBuiltinId::IteratorFilterReturn => {
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                    heap_shape: Some(Box::new(Self::empty_object_shape())),
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::IteratorFlatMapNext | StandardBuiltinId::IteratorFlatMapReturn => {
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                    heap_shape: Some(Box::new(Self::empty_object_shape())),
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::IteratorPrototypeConstructorGetter => {
                Some(Self::standard_builtin_value_info(
                    StandardBuiltinId::IteratorPrototypeConstructorGetter,
                ))
            }
            StandardBuiltinId::IteratorPrototypeConstructorSetter => {
                Some(Self::standard_builtin_value_info(
                    StandardBuiltinId::IteratorPrototypeConstructorSetter,
                ))
            }
            StandardBuiltinId::IteratorPrototypeSymbolDispose => {
                Some(Self::standard_builtin_value_info(
                    StandardBuiltinId::IteratorPrototypeSymbolDispose,
                ))
            }
            StandardBuiltinId::IteratorPrototypeToStringTagGetter => {
                Some(Self::standard_builtin_value_info(
                    StandardBuiltinId::IteratorPrototypeToStringTagGetter,
                ))
            }
            StandardBuiltinId::IteratorPrototypeToStringTagSetter => {
                Some(Self::standard_builtin_value_info(
                    StandardBuiltinId::IteratorPrototypeToStringTagSetter,
                ))
            }
            StandardBuiltinId::IteratorFromWrapperNext => Some(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorFromWrapperNext,
            )),
            StandardBuiltinId::IteratorFromWrapperReturn => Some(
                Self::standard_builtin_value_info(StandardBuiltinId::IteratorFromWrapperReturn),
            ),
            StandardBuiltinId::ArrayIteratorNext
            | StandardBuiltinId::StringIteratorNext
            | StandardBuiltinId::RegExpStringIteratorNext
            | StandardBuiltinId::GeneratorPrototypeNext
            | StandardBuiltinId::GeneratorPrototypeReturn
            | StandardBuiltinId::GeneratorPrototypeThrow => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: Some(Box::new(Self::empty_object_shape())),
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::AsyncGeneratorPrototypeNext
            | StandardBuiltinId::AsyncGeneratorPrototypeReturn
            | StandardBuiltinId::AsyncGeneratorPrototypeThrow
            | StandardBuiltinId::AsyncIteratorPrototypeAsyncDispose => Some(
                Self::value_info_from_shape(Some(Self::promise_instance_shape())),
            ),
            StandardBuiltinId::AsyncIteratorPrototypeAsyncDisposeFulfilled => {
                Some(ValueInfo::undefined())
            }
            StandardBuiltinId::ArrayIteratorIdentity | StandardBuiltinId::AsyncIteratorIdentity => {
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                    heap_shape: Some(Box::new(Self::empty_object_shape())),
                    function_targets: FunctionTargetKnowledge::unknown(),
                })
            }
            StandardBuiltinId::ArrayBufferConstructor => Some(Self::value_info_from_shape(Some(
                Self::array_buffer_instance_shape(),
            ))),
            StandardBuiltinId::SharedArrayBufferConstructor => Some(Self::value_info_from_shape(
                Some(Self::shared_array_buffer_instance_shape()),
            )),
            StandardBuiltinId::ArraySpeciesGetter
            | StandardBuiltinId::TypedArraySpeciesGetter
            | StandardBuiltinId::ArrayBufferSpeciesGetter
            | StandardBuiltinId::RegExpSpeciesGetter => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::ArrayBufferPrototypeByteLengthGetter => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::SharedArrayBufferPrototypeByteLengthGetter => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::SharedArrayBufferPrototypeMaxByteLengthGetter => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::SharedArrayBufferPrototypeGrowableGetter => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::SharedArrayBufferPrototypeGrow => Some(ValueInfo::undefined()),
            StandardBuiltinId::ArrayBufferPrototypeDetachedGetter => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::ArrayBufferPrototypeMaxByteLengthGetter => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::ArrayBufferPrototypeResizableGetter
            | StandardBuiltinId::ArrayBufferPrototypeImmutableGetter => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::ArrayBufferPrototypeResize => Some(ValueInfo::undefined()),
            StandardBuiltinId::ArrayBufferPrototypeSlice => Some(Self::value_info_from_shape(
                Some(Self::array_buffer_instance_shape()),
            )),
            StandardBuiltinId::SharedArrayBufferPrototypeSlice => Some(
                Self::value_info_from_shape(Some(Self::shared_array_buffer_instance_shape())),
            ),
            StandardBuiltinId::ArrayBufferPrototypeTransfer
            | StandardBuiltinId::ArrayBufferPrototypeTransferToFixedLength
            | StandardBuiltinId::ArrayBufferPrototypeTransferToImmutable
            | StandardBuiltinId::ArrayBufferPrototypeSliceToImmutable => Some(
                Self::value_info_from_shape(Some(Self::array_buffer_instance_shape())),
            ),
            StandardBuiltinId::ArrayBufferIsView => Some(ValueInfo {
                kind: ValueKind::Boolean,
                possible_kinds: KindSet::from_kind(ValueKind::Boolean),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::DataViewConstructor => Some(Self::value_info_from_shape(Some(
                Self::data_view_instance_shape(),
            ))),
            StandardBuiltinId::DataViewPrototypeBufferGetter => Some(Self::value_info_from_shape(
                Some(Self::array_buffer_instance_shape()),
            )),
            StandardBuiltinId::TypedArrayPrototypeBufferGetter => Some(
                Self::value_info_from_shape(Some(Self::array_buffer_instance_shape())),
            ),
            StandardBuiltinId::DataViewPrototypeByteLengthGetter
            | StandardBuiltinId::DataViewPrototypeByteOffsetGetter
            | StandardBuiltinId::TypedArrayPrototypeByteLengthGetter
            | StandardBuiltinId::TypedArrayPrototypeByteOffsetGetter
            | StandardBuiltinId::TypedArrayPrototypeLengthGetter => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::TypedArrayPrototypeToStringTagGetter => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::Undefined)
                    .union(KindSet::from_kind(ValueKind::String)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::TypedArrayFrom | StandardBuiltinId::TypedArrayOf => {
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::Uint8ArrayFromBase64 | StandardBuiltinId::Uint8ArrayFromHex => {
                Some(Self::value_info_from_shape(Some(
                    Self::typed_array_instance_shape_for_constructor(
                        StandardBuiltinId::Uint8ArrayConstructor,
                    ),
                )))
            }
            StandardBuiltinId::Uint8ArrayPrototypeSetFromBase64
            | StandardBuiltinId::Uint8ArrayPrototypeSetFromHex => Some(
                Self::value_info_from_shape(Some(Self::uint8_array_codec_result_shape())),
            ),
            StandardBuiltinId::Float64ArrayConstructor
            | StandardBuiltinId::Float32ArrayConstructor
            | StandardBuiltinId::Float16ArrayConstructor
            | StandardBuiltinId::Int32ArrayConstructor
            | StandardBuiltinId::Int16ArrayConstructor
            | StandardBuiltinId::Int8ArrayConstructor
            | StandardBuiltinId::Uint32ArrayConstructor
            | StandardBuiltinId::Uint16ArrayConstructor
            | StandardBuiltinId::Uint8ArrayConstructor
            | StandardBuiltinId::Uint8ClampedArrayConstructor
            | StandardBuiltinId::BigInt64ArrayConstructor
            | StandardBuiltinId::BigUint64ArrayConstructor => Some(Self::value_info_from_shape(
                Some(Self::typed_array_instance_shape_for_constructor(builtin)),
            )),
            StandardBuiltinId::DataViewPrototypeGetUint8
            | StandardBuiltinId::DataViewPrototypeGetInt8
            | StandardBuiltinId::DataViewPrototypeGetUint16
            | StandardBuiltinId::DataViewPrototypeGetInt16
            | StandardBuiltinId::DataViewPrototypeGetUint32
            | StandardBuiltinId::DataViewPrototypeGetInt32
            | StandardBuiltinId::DataViewPrototypeGetFloat16
            | StandardBuiltinId::DataViewPrototypeGetFloat32
            | StandardBuiltinId::DataViewPrototypeGetFloat64 => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::DataViewPrototypeGetBigInt64
            | StandardBuiltinId::DataViewPrototypeGetBigUint64 => {
                Some(ValueInfo::new(ValueKind::BigInt))
            }
            StandardBuiltinId::DataViewPrototypeSetUint8
            | StandardBuiltinId::DataViewPrototypeSetInt8
            | StandardBuiltinId::DataViewPrototypeSetUint16
            | StandardBuiltinId::DataViewPrototypeSetInt16
            | StandardBuiltinId::DataViewPrototypeSetUint32
            | StandardBuiltinId::DataViewPrototypeSetInt32
            | StandardBuiltinId::DataViewPrototypeSetFloat16
            | StandardBuiltinId::DataViewPrototypeSetFloat32
            | StandardBuiltinId::DataViewPrototypeSetFloat64
            | StandardBuiltinId::DataViewPrototypeSetBigInt64
            | StandardBuiltinId::DataViewPrototypeSetBigUint64 => Some(ValueInfo::undefined()),
            StandardBuiltinId::BigIntConstructor => {
                if context == BuiltinCallContext::Construct {
                    Some(ValueInfo {
                        kind: ValueKind::Dynamic,
                        possible_kinds: KindSet::all_runtime_tags(),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::unknown(),
                    })
                } else {
                    Some(ValueInfo::new(ValueKind::BigInt))
                }
            }
            StandardBuiltinId::BigIntAsIntN
            | StandardBuiltinId::BigIntAsUintN
            | StandardBuiltinId::BigIntPrototypeValueOf => Some(ValueInfo::new(ValueKind::BigInt)),
            StandardBuiltinId::BigIntPrototypeToString
            | StandardBuiltinId::BigIntPrototypeToLocaleString => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::NumberConstructor => {
                if context == BuiltinCallContext::Construct {
                    Some(Self::boxed_primitive_instance_info(ValueInfo::new(
                        ValueKind::Number,
                    )))
                } else {
                    Some(ValueInfo::new(ValueKind::Number))
                }
            }
            StandardBuiltinId::StringConstructor => {
                if context == BuiltinCallContext::Construct {
                    Some(Self::boxed_primitive_instance_info(ValueInfo::new(
                        ValueKind::String,
                    )))
                } else {
                    Some(ValueInfo::new(ValueKind::String))
                }
            }
            StandardBuiltinId::StringFromCharCode
            | StandardBuiltinId::StringFromCodePoint
            | StandardBuiltinId::StringRaw => Some(ValueInfo::new(ValueKind::String)),
            StandardBuiltinId::StringPrototypeToString
            | StandardBuiltinId::StringPrototypeValueOf => Some(ValueInfo::new(ValueKind::String)),
            StandardBuiltinId::StringPrototypeCharAt | StandardBuiltinId::StringPrototypeConcat => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::StringPrototypeCharCodeAt => Some(ValueInfo::new(ValueKind::Number)),
            StandardBuiltinId::StringPrototypeIndexOf
            | StandardBuiltinId::StringPrototypeLastIndexOf
            | StandardBuiltinId::StringPrototypeLocaleCompare
            | StandardBuiltinId::IntlCollatorBoundCompare => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::StringPrototypeCodePointAt => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::Number)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::StringPrototypeEndsWith
            | StandardBuiltinId::StringPrototypeIncludes
            | StandardBuiltinId::StringPrototypeStartsWith => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::RegExpPrototypeSymbolMatchAll => Some(Self::value_info_from_shape(
                Some(Self::regexp_string_iterator_instance_shape()),
            )),
            StandardBuiltinId::BooleanConstructor => {
                if context == BuiltinCallContext::Construct {
                    Some(Self::boxed_primitive_instance_info(ValueInfo::new(
                        ValueKind::Boolean,
                    )))
                } else {
                    Some(ValueInfo::new(ValueKind::Boolean))
                }
            }
            StandardBuiltinId::SymbolConstructor => Some(ValueInfo::new(ValueKind::Symbol)),
            StandardBuiltinId::SymbolFor => Some(ValueInfo::new(ValueKind::Symbol)),
            StandardBuiltinId::SymbolKeyFor => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::SymbolPrototypeDescriptionGetter => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::SymbolPrototypeToString => Some(ValueInfo::new(ValueKind::String)),
            StandardBuiltinId::SymbolPrototypeValueOf
            | StandardBuiltinId::SymbolPrototypeToPrimitive => {
                Some(ValueInfo::new(ValueKind::Symbol))
            }
            StandardBuiltinId::ErrorConstructor
            | StandardBuiltinId::EvalErrorConstructor
            | StandardBuiltinId::AggregateErrorConstructor
            | StandardBuiltinId::SuppressedErrorConstructor
            | StandardBuiltinId::RangeErrorConstructor
            | StandardBuiltinId::SyntaxErrorConstructor
            | StandardBuiltinId::TypeErrorConstructor
            | StandardBuiltinId::URIErrorConstructor
            | StandardBuiltinId::ReferenceErrorConstructor => {
                Some(self.standard_error_instance_info(builtin))
            }
            StandardBuiltinId::FunctionPrototypeToString
            | StandardBuiltinId::ErrorPrototypeToString
            | StandardBuiltinId::StringPrototypeAnchor
            | StandardBuiltinId::StringPrototypeBig
            | StandardBuiltinId::StringPrototypeBlink
            | StandardBuiltinId::StringPrototypeBold
            | StandardBuiltinId::StringPrototypeFixed
            | StandardBuiltinId::StringPrototypeFontcolor
            | StandardBuiltinId::StringPrototypeFontsize
            | StandardBuiltinId::StringPrototypeItalics
            | StandardBuiltinId::StringPrototypeLink
            | StandardBuiltinId::StringPrototypeSmall
            | StandardBuiltinId::StringPrototypeStrike
            | StandardBuiltinId::StringPrototypeSub
            | StandardBuiltinId::StringPrototypeSubstr
            | StandardBuiltinId::StringPrototypeSubstring
            | StandardBuiltinId::StringPrototypeSup
            | StandardBuiltinId::StringPrototypeSlice
            | StandardBuiltinId::StringPrototypePadStart
            | StandardBuiltinId::StringPrototypePadEnd
            | StandardBuiltinId::StringPrototypeRepeat
            | StandardBuiltinId::StringPrototypeNormalize
            | StandardBuiltinId::StringPrototypeToLocaleLowerCase
            | StandardBuiltinId::StringPrototypeToLocaleUpperCase
            | StandardBuiltinId::StringPrototypeToLowerCase
            | StandardBuiltinId::StringPrototypeToUpperCase
            | StandardBuiltinId::StringPrototypeTrim
            | StandardBuiltinId::StringPrototypeTrimStart
            | StandardBuiltinId::StringPrototypeTrimEnd
            | StandardBuiltinId::StringPrototypeToWellFormed
            | StandardBuiltinId::RegExpEscape
            | StandardBuiltinId::Escape
            | StandardBuiltinId::Unescape
            | StandardBuiltinId::EncodeUri
            | StandardBuiltinId::EncodeUriComponent
            | StandardBuiltinId::DecodeUri
            | StandardBuiltinId::DecodeUriComponent => Some(ValueInfo::new(ValueKind::String)),
            StandardBuiltinId::StringPrototypeAt => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::StringPrototypeIsWellFormed => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::RegExpPrototypeTest => Some(ValueInfo::new(ValueKind::Boolean)),
            StandardBuiltinId::StringPrototypeMatch
            | StandardBuiltinId::StringPrototypeMatchAll
            | StandardBuiltinId::StringPrototypeReplace
            | StandardBuiltinId::StringPrototypeReplaceAll
            | StandardBuiltinId::StringPrototypeSearch
            | StandardBuiltinId::StringPrototypeSplit
            | StandardBuiltinId::RegExpLegacyStaticGetter
            | StandardBuiltinId::RegExpPrototypeCompile
            | StandardBuiltinId::RegExpPrototypeExec
            | StandardBuiltinId::RegExpPrototypeToString
            | StandardBuiltinId::RegExpPrototypeSymbolMatch
            | StandardBuiltinId::RegExpPrototypeSymbolReplace
            | StandardBuiltinId::RegExpPrototypeSymbolSearch
            | StandardBuiltinId::RegExpPrototypeSymbolSplit => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::RegExpLegacyStaticSetter => {
                Some(ValueInfo::new(ValueKind::Undefined))
            }
            StandardBuiltinId::DateConstructor => Some(Self::value_info_from_shape(Some(
                Self::date_instance_shape(),
            ))),
            StandardBuiltinId::TemporalNowTimeZoneId => Some(ValueInfo::new(ValueKind::String)),
            StandardBuiltinId::TemporalPlainDateConstructor
            | StandardBuiltinId::TemporalPlainDateFrom
            | StandardBuiltinId::TemporalPlainDatePrototypeWith
            | StandardBuiltinId::TemporalPlainDatePrototypeWithCalendar
            | StandardBuiltinId::TemporalPlainDatePrototypeAdd
            | StandardBuiltinId::TemporalPlainDatePrototypeSubtract => Some(
                Self::value_info_from_shape(Some(Self::temporal_plain_date_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDatePrototypeUntil
            | StandardBuiltinId::TemporalPlainDatePrototypeSince => Some(
                Self::value_info_from_shape(Some(Self::temporal_duration_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDatePrototypeToPlainDateTime => Some(
                Self::value_info_from_shape(Some(Self::temporal_plain_date_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDatePrototypeToPlainYearMonth => Some(
                Self::value_info_from_shape(Some(Self::temporal_plain_year_month_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDatePrototypeToPlainMonthDay => Some(
                Self::value_info_from_shape(Some(Self::temporal_plain_month_day_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainYearMonthConstructor
            | StandardBuiltinId::TemporalPlainYearMonthFrom
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeWith
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeAdd
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeSubtract => Some(
                Self::value_info_from_shape(Some(Self::temporal_plain_year_month_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainMonthDayConstructor
            | StandardBuiltinId::TemporalPlainMonthDayFrom
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeWith => Some(
                Self::value_info_from_shape(Some(Self::temporal_plain_month_day_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainYearMonthPrototypeUntil
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeSince => Some(
                Self::value_info_from_shape(Some(Self::temporal_duration_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainYearMonthPrototypeToPlainDate
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeToPlainDate => Some(
                Self::value_info_from_shape(Some(Self::temporal_plain_date_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainYearMonthCompare
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeYearGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInYearGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInMonthGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthsInYearGetter
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeDayGetter => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeToString
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeToJson
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeToLocaleString
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeToString
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeToJson
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeToLocaleString => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeInLeapYearGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeEquals
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeEquals => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeEraGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeEraYearGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeValueOf
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeValueOf => {
                Some(ValueInfo::new(ValueKind::Undefined))
            }
            StandardBuiltinId::TemporalDurationConstructor
            | StandardBuiltinId::TemporalDurationFrom
            | StandardBuiltinId::TemporalDurationPrototypeWith
            | StandardBuiltinId::TemporalDurationPrototypeNegated
            | StandardBuiltinId::TemporalDurationPrototypeAbs
            | StandardBuiltinId::TemporalDurationPrototypeAdd
            | StandardBuiltinId::TemporalDurationPrototypeSubtract
            | StandardBuiltinId::TemporalDurationPrototypeRound => Some(
                Self::value_info_from_shape(Some(Self::temporal_duration_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainTimeConstructor
            | StandardBuiltinId::TemporalPlainTimeFrom
            | StandardBuiltinId::TemporalPlainTimePrototypeWith
            | StandardBuiltinId::TemporalPlainTimePrototypeAdd
            | StandardBuiltinId::TemporalPlainTimePrototypeSubtract
            | StandardBuiltinId::TemporalPlainTimePrototypeRound => Some(
                Self::value_info_from_shape(Some(Self::temporal_plain_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalInstantPrototypeUntil
            | StandardBuiltinId::TemporalInstantPrototypeSince
            | StandardBuiltinId::TemporalPlainTimePrototypeUntil
            | StandardBuiltinId::TemporalPlainTimePrototypeSince => Some(
                Self::value_info_from_shape(Some(Self::temporal_duration_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainTimeCompare
            | StandardBuiltinId::TemporalPlainTimePrototypeHourGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeMinuteGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeSecondGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeMillisecondGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeMicrosecondGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeNanosecondGetter => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::TemporalPlainTimePrototypeToString
            | StandardBuiltinId::TemporalPlainTimePrototypeToJson
            | StandardBuiltinId::TemporalPlainTimePrototypeToLocaleString => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::TemporalPlainTimePrototypeEquals => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::TemporalPlainTimePrototypeValueOf => {
                Some(ValueInfo::new(ValueKind::Undefined))
            }
            StandardBuiltinId::TemporalDurationCompare
            | StandardBuiltinId::TemporalDurationPrototypeYearsGetter
            | StandardBuiltinId::TemporalDurationPrototypeMonthsGetter
            | StandardBuiltinId::TemporalDurationPrototypeWeeksGetter
            | StandardBuiltinId::TemporalDurationPrototypeDaysGetter
            | StandardBuiltinId::TemporalDurationPrototypeHoursGetter
            | StandardBuiltinId::TemporalDurationPrototypeMinutesGetter
            | StandardBuiltinId::TemporalDurationPrototypeSecondsGetter
            | StandardBuiltinId::TemporalDurationPrototypeMillisecondsGetter
            | StandardBuiltinId::TemporalDurationPrototypeMicrosecondsGetter
            | StandardBuiltinId::TemporalDurationPrototypeNanosecondsGetter
            | StandardBuiltinId::TemporalDurationPrototypeSignGetter
            | StandardBuiltinId::TemporalDurationPrototypeTotal => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::TemporalDurationPrototypeToString
            | StandardBuiltinId::TemporalDurationPrototypeToJson
            | StandardBuiltinId::TemporalDurationPrototypeToLocaleString => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::TemporalDurationPrototypeBlankGetter => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::TemporalDurationPrototypeValueOf => {
                Some(ValueInfo::new(ValueKind::Undefined))
            }
            StandardBuiltinId::TemporalPlainDateCompare
            | StandardBuiltinId::TemporalPlainDatePrototypeYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeMonthGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDayGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDayOfWeekGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDayOfYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeWeekOfYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeYearOfWeekGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDaysInWeekGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDaysInMonthGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDaysInYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeMonthsInYearGetter => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::TemporalPlainDatePrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeToString
            | StandardBuiltinId::TemporalPlainDatePrototypeToJson
            | StandardBuiltinId::TemporalPlainDatePrototypeToLocaleString => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::TemporalPlainDatePrototypeInLeapYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeEquals => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::TemporalPlainDatePrototypeEraGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeValueOf => {
                Some(ValueInfo::new(ValueKind::Undefined))
            }
            StandardBuiltinId::TemporalPlainDateTimeConstructor
            | StandardBuiltinId::TemporalPlainDateTimeFrom
            | StandardBuiltinId::TemporalPlainDateTimePrototypeWith
            | StandardBuiltinId::TemporalPlainDateTimePrototypeWithPlainTime
            | StandardBuiltinId::TemporalPlainDateTimePrototypeWithCalendar
            | StandardBuiltinId::TemporalPlainDateTimePrototypeAdd
            | StandardBuiltinId::TemporalPlainDateTimePrototypeSubtract
            | StandardBuiltinId::TemporalPlainDateTimePrototypeRound => Some(
                Self::value_info_from_shape(Some(Self::temporal_plain_date_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDateTimePrototypeUntil
            | StandardBuiltinId::TemporalPlainDateTimePrototypeSince => Some(
                Self::value_info_from_shape(Some(Self::temporal_duration_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainDate => Some(
                Self::value_info_from_shape(Some(Self::temporal_plain_date_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainTime => Some(
                Self::value_info_from_shape(Some(Self::temporal_plain_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDateTimePrototypeToZonedDateTime => Some(
                Self::value_info_from_shape(Some(Self::temporal_zoned_date_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDateTimeCompare
            | StandardBuiltinId::TemporalPlainDateTimePrototypeYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDayGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeHourGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMinuteGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeSecondGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMillisecondGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMicrosecondGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeNanosecondGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfWeekGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeWeekOfYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeYearOfWeekGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInWeekGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInMonthGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthsInYearGetter => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeToString
            | StandardBuiltinId::TemporalPlainDateTimePrototypeToJson
            | StandardBuiltinId::TemporalPlainDateTimePrototypeToLocaleString => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeInLeapYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeEquals => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeEraGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeValueOf => {
                Some(ValueInfo::new(ValueKind::Undefined))
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeEraYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeEraYearGetter => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::Number)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::TemporalNowInstant
            | StandardBuiltinId::TemporalInstantConstructor
            | StandardBuiltinId::TemporalZonedDateTimePrototypeToInstant
            | StandardBuiltinId::TemporalInstantFrom
            | StandardBuiltinId::TemporalInstantFromEpochMilliseconds
            | StandardBuiltinId::TemporalInstantPrototypeAdd
            | StandardBuiltinId::TemporalInstantPrototypeSubtract
            | StandardBuiltinId::TemporalInstantPrototypeRound
            | StandardBuiltinId::TemporalInstantFromEpochNanoseconds => Some(
                Self::value_info_from_shape(Some(Self::temporal_instant_instance_shape())),
            ),
            StandardBuiltinId::TemporalInstantPrototypeToString
            | StandardBuiltinId::TemporalInstantPrototypeToLocaleString
            | StandardBuiltinId::TemporalInstantPrototypeToJson => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::TemporalInstantPrototypeValueOf => {
                Some(ValueInfo::new(ValueKind::Undefined))
            }
            StandardBuiltinId::IntlLocalePrototypeGetTextInfo
            | StandardBuiltinId::IntlLocalePrototypeGetWeekInfo => {
                Some(ValueInfo::new(ValueKind::Object))
            }
            StandardBuiltinId::IntlLocaleConstructor
            | StandardBuiltinId::IntlLocalePrototypeMaximize
            | StandardBuiltinId::IntlLocalePrototypeMinimize => Some(Self::value_info_from_shape(
                Some(Self::intl_locale_instance_shape()),
            )),
            StandardBuiltinId::IntlLocalePrototypeGetTimeZones => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::Array)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::IntlLocalePrototypeScriptGetter
            | StandardBuiltinId::IntlLocalePrototypeRegionGetter
            | StandardBuiltinId::IntlLocalePrototypeCalendarGetter
            | StandardBuiltinId::IntlLocalePrototypeCollationGetter
            | StandardBuiltinId::IntlLocalePrototypeFirstDayOfWeekGetter
            | StandardBuiltinId::IntlLocalePrototypeHourCycleGetter
            | StandardBuiltinId::IntlLocalePrototypeCaseFirstGetter
            | StandardBuiltinId::IntlLocalePrototypeNumberingSystemGetter
            | StandardBuiltinId::IntlLocalePrototypeVariantsGetter => None,
            StandardBuiltinId::IntlDateTimeFormatConstructor
            | StandardBuiltinId::IntlDateTimeFormatPrototypeResolvedOptions
            | StandardBuiltinId::IntlNumberFormatConstructor
            | StandardBuiltinId::IntlPluralRulesConstructor
            | StandardBuiltinId::IntlListFormatConstructor
            | StandardBuiltinId::IntlCollatorConstructor
            | StandardBuiltinId::IntlDisplayNamesConstructor
            | StandardBuiltinId::IntlRelativeTimeFormatConstructor
            | StandardBuiltinId::IntlDurationFormatConstructor
            | StandardBuiltinId::IntlSegmenterConstructor
            | StandardBuiltinId::IntlNumberFormatPrototypeResolvedOptions
            | StandardBuiltinId::IntlPluralRulesPrototypeResolvedOptions
            | StandardBuiltinId::IntlListFormatPrototypeResolvedOptions
            | StandardBuiltinId::IntlCollatorPrototypeResolvedOptions
            | StandardBuiltinId::IntlDisplayNamesPrototypeResolvedOptions
            | StandardBuiltinId::IntlRelativeTimeFormatPrototypeResolvedOptions
            | StandardBuiltinId::IntlDurationFormatPrototypeResolvedOptions
            | StandardBuiltinId::IntlSegmenterPrototypeSegment
            | StandardBuiltinId::IntlSegmenterPrototypeResolvedOptions
            | StandardBuiltinId::IntlSegmentsPrototypeIterator
            | StandardBuiltinId::IntlSegmentIteratorPrototypeNext => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::IntlSupportedValuesOf
            | StandardBuiltinId::IntlLocalePrototypeGetCalendars
            | StandardBuiltinId::IntlLocalePrototypeGetCollations
            | StandardBuiltinId::IntlLocalePrototypeGetNumberingSystems
            | StandardBuiltinId::IntlLocalePrototypeGetHourCycles
            | StandardBuiltinId::IntlGetCanonicalLocales
            | StandardBuiltinId::IntlDateTimeFormatSupportedLocalesOf
            | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRangeToParts
            | StandardBuiltinId::IntlNumberFormatSupportedLocalesOf
            | StandardBuiltinId::IntlPluralRulesSupportedLocalesOf
            | StandardBuiltinId::IntlListFormatSupportedLocalesOf
            | StandardBuiltinId::IntlCollatorSupportedLocalesOf
            | StandardBuiltinId::IntlDisplayNamesSupportedLocalesOf
            | StandardBuiltinId::IntlRelativeTimeFormatSupportedLocalesOf
            | StandardBuiltinId::IntlDurationFormatSupportedLocalesOf
            | StandardBuiltinId::IntlSegmenterSupportedLocalesOf
            | StandardBuiltinId::IntlListFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlDurationFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlNumberFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlNumberFormatPrototypeFormatRangeToParts => {
                Some(ValueInfo::new(ValueKind::Array))
            }
            StandardBuiltinId::IntlDisplayNamesPrototypeOf
            | StandardBuiltinId::IntlSegmentsPrototypeContaining => None,
            StandardBuiltinId::IntlLocalePrototypeNumericGetter => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::IntlDateTimeFormatPrototypeFormatGetter
            | StandardBuiltinId::IntlNumberFormatPrototypeFormatGetter
            | StandardBuiltinId::IntlCollatorPrototypeCompareGetter => {
                Some(ValueInfo::new(ValueKind::Function))
            }
            StandardBuiltinId::IntlLocalePrototypeLanguageGetter
            | StandardBuiltinId::IntlLocalePrototypeBaseNameGetter
            | StandardBuiltinId::IntlLocalePrototypeToString
            | StandardBuiltinId::IntlDateTimeFormatBoundFormat
            | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRange
            | StandardBuiltinId::IntlNumberFormatBoundFormat
            | StandardBuiltinId::IntlNumberFormatPrototypeFormatRange
            | StandardBuiltinId::IntlPluralRulesPrototypeSelect
            | StandardBuiltinId::IntlPluralRulesPrototypeSelectRange
            | StandardBuiltinId::IntlListFormatPrototypeFormat
            | StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormat
            | StandardBuiltinId::IntlDurationFormatPrototypeFormat => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeGetTimeZoneTransition => {
                Some(ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object)
                        .union(KindSet::from_kind(ValueKind::Null)),
                    heap_shape: Some(Self::temporal_zoned_date_time_instance_shape()),
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            StandardBuiltinId::TemporalNowZonedDateTimeIso
            | StandardBuiltinId::TemporalZonedDateTimeConstructor
            | StandardBuiltinId::TemporalZonedDateTimePrototypeWith
            | StandardBuiltinId::TemporalZonedDateTimePrototypeWithPlainTime
            | StandardBuiltinId::TemporalZonedDateTimePrototypeRound
            | StandardBuiltinId::TemporalZonedDateTimePrototypeStartOfDay
            | StandardBuiltinId::TemporalPlainDatePrototypeToZonedDateTime
            | StandardBuiltinId::TemporalInstantPrototypeToZonedDateTimeIso
            | StandardBuiltinId::TemporalZonedDateTimeFrom => Some(Self::value_info_from_shape(
                Some(Self::temporal_zoned_date_time_instance_shape()),
            )),
            StandardBuiltinId::TemporalNowPlainDateTimeIso => Some(Self::value_info_from_shape(
                Some(Self::temporal_plain_date_time_instance_shape()),
            )),
            StandardBuiltinId::TemporalNowPlainDateIso => Some(Self::value_info_from_shape(Some(
                Self::temporal_plain_date_instance_shape(),
            ))),
            StandardBuiltinId::TemporalNowPlainTimeIso => Some(Self::value_info_from_shape(Some(
                Self::temporal_plain_time_instance_shape(),
            ))),
            StandardBuiltinId::TemporalInstantPrototypeEpochNanosecondsGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeEpochNanosecondsGetter => {
                Some(ValueInfo::new(ValueKind::BigInt))
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeTimeZoneIdGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeToString
            | StandardBuiltinId::TemporalZonedDateTimePrototypeToJson
            | StandardBuiltinId::TemporalZonedDateTimePrototypeToLocaleString => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeValueOf => {
                Some(ValueInfo::new(ValueKind::Undefined))
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainTime => Some(
                Self::value_info_from_shape(Some(Self::temporal_plain_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalInstantCompare
            | StandardBuiltinId::TemporalInstantPrototypeEpochMillisecondsGetter
            | StandardBuiltinId::TemporalZonedDateTimeCompare
            | StandardBuiltinId::TemporalZonedDateTimePrototypeHoursInDayGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetNanosecondsGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeEpochMillisecondsGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfWeekGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInWeekGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInMonthGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthsInYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDayGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeHourGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMinuteGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeSecondGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMillisecondGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMicrosecondGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeNanosecondGetter => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::TemporalInstantPrototypeEquals
            | StandardBuiltinId::TemporalZonedDateTimePrototypeInLeapYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeEquals => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            // Same declaration as the PlainDate / PlainDateTime / PlainYearMonth
            // era pair; see the sibling comment on the kind table above.
            StandardBuiltinId::TemporalZonedDateTimePrototypeEraGetter => {
                Some(ValueInfo::new(ValueKind::Undefined))
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeWeekOfYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeYearOfWeekGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeEraYearGetter => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::Number)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            // Same grouping as the kind table above; the two tables must not be
            // able to disagree about which ZonedDateTime data methods return a
            // `Temporal.Duration` and which return a ZonedDateTime.
            StandardBuiltinId::TemporalZonedDateTimePrototypeWithTimeZone
            | StandardBuiltinId::TemporalZonedDateTimePrototypeWithCalendar
            | StandardBuiltinId::TemporalZonedDateTimePrototypeAdd
            | StandardBuiltinId::TemporalZonedDateTimePrototypeSubtract => Some(
                Self::value_info_from_shape(Some(Self::temporal_zoned_date_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeUntil
            | StandardBuiltinId::TemporalZonedDateTimePrototypeSince => Some(
                Self::value_info_from_shape(Some(Self::temporal_duration_instance_shape())),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDate => Some(
                Self::value_info_from_shape(Some(Self::temporal_plain_date_instance_shape())),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDateTime => Some(
                Self::value_info_from_shape(Some(Self::temporal_plain_date_time_instance_shape())),
            ),
            StandardBuiltinId::RegExpConstructor => {
                let object_kinds =
                    KindSet::HEAP_COERCIBLE_ONLY.union(KindSet::from_kind(ValueKind::Function));
                if context == BuiltinCallContext::Call
                    && args
                        .first()
                        .is_some_and(|pattern| pattern.possible_kinds.intersects(object_kinds))
                {
                    let possible_kinds = args[0]
                        .possible_kinds
                        .intersection(object_kinds)
                        .union(KindSet::from_kind(ValueKind::Object));
                    // Call may return the pattern object itself. Retain every
                    // known callable candidate so a later call still observes
                    // source parameters and prepares finite Function/eval code.
                    // A fresh RegExp remains possible, so this is never an
                    // exact callable result or an instance-shape proof.
                    let mut function_targets = args[0].function_targets.clone();
                    function_targets.widen_for_possible_replacement();
                    Some(ValueInfo {
                        kind: if possible_kinds == KindSet::from_kind(ValueKind::Object) {
                            ValueKind::Object
                        } else {
                            ValueKind::Dynamic
                        },
                        possible_kinds,
                        heap_shape: None,
                        function_targets,
                    })
                } else {
                    Some(Self::value_info_from_shape(Some(
                        Self::regexp_instance_shape(),
                    )))
                }
            }
            StandardBuiltinId::DateNow
            | StandardBuiltinId::DateParse
            | StandardBuiltinId::DateUtc
            | StandardBuiltinId::DatePrototypeGetTime
            | StandardBuiltinId::DatePrototypeSetTime
            | StandardBuiltinId::DatePrototypeValueOf
            | StandardBuiltinId::DatePrototypeGetFullYear
            | StandardBuiltinId::DatePrototypeGetUtcFullYear
            | StandardBuiltinId::DatePrototypeGetMonth
            | StandardBuiltinId::DatePrototypeGetUtcMonth
            | StandardBuiltinId::DatePrototypeGetDate
            | StandardBuiltinId::DatePrototypeGetUtcDate
            | StandardBuiltinId::DatePrototypeGetDay
            | StandardBuiltinId::DatePrototypeGetUtcDay
            | StandardBuiltinId::DatePrototypeGetHours
            | StandardBuiltinId::DatePrototypeGetUtcHours
            | StandardBuiltinId::DatePrototypeGetMinutes
            | StandardBuiltinId::DatePrototypeGetUtcMinutes
            | StandardBuiltinId::DatePrototypeGetSeconds
            | StandardBuiltinId::DatePrototypeGetUtcSeconds
            | StandardBuiltinId::DatePrototypeGetMilliseconds
            | StandardBuiltinId::DatePrototypeGetUtcMilliseconds
            | StandardBuiltinId::DatePrototypeGetTimezoneOffset
            | StandardBuiltinId::DatePrototypeGetYear
            | StandardBuiltinId::DatePrototypeSetYear
            | StandardBuiltinId::DatePrototypeSetFullYear
            | StandardBuiltinId::DatePrototypeSetUtcFullYear
            | StandardBuiltinId::DatePrototypeSetMonth
            | StandardBuiltinId::DatePrototypeSetUtcMonth
            | StandardBuiltinId::DatePrototypeSetDate
            | StandardBuiltinId::DatePrototypeSetUtcDate
            | StandardBuiltinId::DatePrototypeSetHours
            | StandardBuiltinId::DatePrototypeSetUtcHours
            | StandardBuiltinId::DatePrototypeSetMinutes
            | StandardBuiltinId::DatePrototypeSetUtcMinutes
            | StandardBuiltinId::DatePrototypeSetSeconds
            | StandardBuiltinId::DatePrototypeSetUtcSeconds
            | StandardBuiltinId::DatePrototypeSetMilliseconds
            | StandardBuiltinId::DatePrototypeSetUtcMilliseconds => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::DatePrototypeToJson => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::DatePrototypeToPrimitive => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::PRIMITIVE_ONLY,
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::DatePrototypeToTemporalInstant => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::DatePrototypeToIsoString
            | StandardBuiltinId::DatePrototypeToDateString
            | StandardBuiltinId::DatePrototypeToLocaleDateString
            | StandardBuiltinId::DatePrototypeToLocaleString
            | StandardBuiltinId::DatePrototypeToLocaleTimeString
            | StandardBuiltinId::DatePrototypeToTimeString
            | StandardBuiltinId::DatePrototypeToString
            | StandardBuiltinId::DatePrototypeToUtcString => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::AtomicsNotify => Some(ValueInfo::new(ValueKind::Number)),
            StandardBuiltinId::AtomicsWait => Some(ValueInfo::new(ValueKind::String)),
            StandardBuiltinId::AtomicsPause => Some(ValueInfo::new(ValueKind::Undefined)),
            StandardBuiltinId::AtomicsWaitAsync => Some(Self::value_info_from_shape(Some(
                Box::new(HeapShape::Object(ObjectShape {
                    provenance: HeapShapeProvenance::Program,
                    prototype: Some(Box::new(Self::empty_object_shape())),
                    properties: BTreeMap::from([
                        (
                            "async".to_string(),
                            ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Boolean)),
                        ),
                        (
                            "value".to_string(),
                            ObjectShapeProperty::Data(ValueInfo {
                                kind: ValueKind::Dynamic,
                                possible_kinds: KindSet::from_kind(ValueKind::String)
                                    .union(KindSet::from_kind(ValueKind::Object)),
                                heap_shape: None,
                                function_targets: FunctionTargetKnowledge::none(),
                            }),
                        ),
                    ]),
                    private_brands: BTreeSet::new(),
                    boxed_primitive: None,
                })),
            ))),
            StandardBuiltinId::ObjectHasOwn
            | StandardBuiltinId::JsonIsRawJson
            | StandardBuiltinId::AtomicsIsLockFree => Some(ValueInfo::new(ValueKind::Boolean)),
            StandardBuiltinId::AtomicsAdd
            | StandardBuiltinId::AtomicsAnd
            | StandardBuiltinId::AtomicsCompareExchange
            | StandardBuiltinId::AtomicsExchange
            | StandardBuiltinId::AtomicsLoad
            | StandardBuiltinId::AtomicsOr
            | StandardBuiltinId::AtomicsSub
            | StandardBuiltinId::AtomicsStore
            | StandardBuiltinId::AtomicsXor => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::Number)
                    .union(KindSet::from_kind(ValueKind::BigInt)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::ObjectGetOwnPropertyNames
            | StandardBuiltinId::ObjectGetOwnPropertySymbols => Some(Self::value_info_from_shape(
                Some(Box::new(HeapShape::Array(ArrayShape::default()))),
            )),
            StandardBuiltinId::JsonParse => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::JsonStringify => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            StandardBuiltinId::JsonRawJson => Some(Self::value_info_from_shape(Some(Box::new(
                Self::raw_json_object_shape(),
            )))),
            StandardBuiltinId::RegExpPrototypeFlagsGetter => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::RegExpPrototypeSourceGetter => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::RegExpPrototypeHasIndicesGetter
            | StandardBuiltinId::RegExpPrototypeGlobalGetter
            | StandardBuiltinId::RegExpPrototypeIgnoreCaseGetter
            | StandardBuiltinId::RegExpPrototypeMultilineGetter
            | StandardBuiltinId::RegExpPrototypeDotAllGetter
            | StandardBuiltinId::RegExpPrototypeUnicodeGetter
            | StandardBuiltinId::RegExpPrototypeUnicodeSetsGetter
            | StandardBuiltinId::RegExpPrototypeStickyGetter => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::ThrowTypeError
            | StandardBuiltinId::TypedArrayConstructor
            | StandardBuiltinId::AbstractModuleSourceConstructor
            | StandardBuiltinId::AbstractModuleSourcePrototypeToStringTagGetter => {
                Some(ValueInfo::undefined())
            }
        };
        let collection_constructor_cannot_call_user_code = matches!(
            builtin,
            StandardBuiltinId::MapConstructor
                | StandardBuiltinId::SetConstructor
                | StandardBuiltinId::WeakMapConstructor
                | StandardBuiltinId::WeakSetConstructor
        ) && (context
            != BuiltinCallContext::Construct
            || args
                .first()
                .is_none_or(|arg| arg.possible_kinds.is_subset_of(KindSet::NULLISH)));
        // Literal syntax allocates from this Realm's intrinsic with parser-owned
        // pattern/flag strings. It never reads the global RegExp binding or
        // coerces caller values, unlike either form of constructor invocation.
        let intrinsic_regexp_literal = builtin == StandardBuiltinId::RegExpConstructor
            && context == BuiltinCallContext::RegExpLiteral;
        if builtin.may_run_user_code_synchronously()
            && !collection_constructor_cannot_call_user_code
            && !intrinsic_regexp_literal
            && !promise_invocation_policy.bypasses_catalog_invalidation()
        {
            self.invalidate_unknown_user_code_effects();
        }
        result.map(StandardBuiltinCallAnalysis::Ordinary)
    }
}
