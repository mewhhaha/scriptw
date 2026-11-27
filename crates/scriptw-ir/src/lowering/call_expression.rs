mod closed_local_call;
mod non_property_call;

use super::call_candidate_analysis::DynamicSourceCallAdmission;
use super::*;

impl<'a> ScriptLowerer<'a> {
    /// A ParenthesizedExpression evaluates to the Reference its operand
    /// evaluates to (ECMA-262 13.2.1.1), so `(o.m)()` calls with `this` = o
    /// and `(eval)(x)` is a direct eval (EvaluateCall reads the Reference).
    /// Only an identifier or an ordinary property access keeps its Reference
    /// through the parentheses: an optional chain `(a?.b)()` must still throw
    /// when `a` is nullish instead of short-circuiting the call, and every
    /// other operand already evaluates to a value.
    fn reference_preserving_callee(callee: &Expression) -> &Expression {
        let inner = Self::unwrap_parenthesized_expr(callee);
        match inner {
            Expression::Identifier(_) | Expression::PropertyAccess(_) => inner,
            _ => callee,
        }
    }

    fn record_builtin_receiver_mutation(&mut self, builtin: StandardBuiltinId) {
        if builtin.mutates_indexed_receiver() {
            self.record_caller_flow_invalidation();
        }
    }

    pub(super) fn lower_call(&mut self, callee: &Expression, args: &[Expression]) -> TypedExpr {
        let callee = Self::reference_preserving_callee(callee);
        self.register_array_callback_source_candidates(callee, args);
        self.register_dynamic_source_candidates(callee, args);
        // A call nested in a computed property key can mutate the already
        // captured base even when its result is a primitive key. The epoch is
        // only an ordering signal; the call's normal effect analysis still
        // decides which flow facts must actually be discarded.
        self.intervening_effect_epoch = self.intervening_effect_epoch.saturating_add(1);
        // Resolve identifier references before intrinsic folds: environment
        // bindings can shadow builtins and supply the call's receiver.
        if let Some(call) = self.lower_environment_identifier_call(callee, args) {
            return call;
        }
        if let Some(call) = self.lower_with_environment_identifier_call(callee, args) {
            return call;
        }
        if let Some(call) = self.lower_direct_eval_call(callee, args) {
            return call;
        }

        if let Expression::Identifier(identifier) = callee {
            let name = self.interner.resolve_expect(identifier.sym()).to_string();
            let callee_is_intrinsic_global = self.identifier_resolves_to_intrinsic_global(&name);
            if name == IS_CONSTRUCTOR_NAME
                && args.len() == 1
                && !matches!(args[0], Expression::Spread(_))
                && callee_is_intrinsic_global
            {
                return TypedExpr::spec_is_constructor(self.lower_expression(&args[0]));
            }
            // These single-argument primitive conversions retain the complete
            // operand IR. Calls with spreads or extra arguments go through the
            // ordinary call owner so every argument is evaluated before any
            // builtin conversion begins.
            if name == NUMBER_NAME
                && callee_is_intrinsic_global
                && args.len() == 1
                && [
                    ValueKind::BigInt,
                    ValueKind::Object,
                    ValueKind::Array,
                    ValueKind::Arguments,
                    ValueKind::Function,
                ]
                .into_iter()
                .all(|kind| self.expression_cannot_be_kind(&args[0], kind))
            {
                let value = self.lower_expression(&args[0]);
                self.record_possible_to_primitive_effects(&value.value_info());
                return TypedExpr::spec_to_number(value);
            }
            if name == STRING_NAME
                && callee_is_intrinsic_global
                && args.len() == 1
                && self.expression_cannot_be_symbol(&args[0])
            {
                let value = self.lower_expression(&args[0]);
                self.record_possible_to_primitive_effects(&value.value_info());
                return TypedExpr::spec_to_string(value);
            }
            if name == BOOLEAN_NAME
                && callee_is_intrinsic_global
                && args.len() == 1
                && !matches!(args[0], Expression::Spread(_))
            {
                return TypedExpr::spec_to_boolean(self.lower_expression(&args[0]));
            }
        }

        if let Expression::PropertyAccess(access) = callee {
            match access {
                PropertyAccess::Simple(access) => {
                    let mut receiver = self.lower_property_target(access.target());
                    let receiver_capture_epoch = self.intervening_effect_epoch;
                    // The receiver's value, not its spelling: `String` and its
                    // `fromCodePoint` are both writable.
                    let string_from_code_point_apply_call =
                        receiver.function_targets.exact_single_target()
                            == Some(&StandardBuiltinId::StringFromCodePoint.function_id());
                    if let PropertyAccessField::Const(field) = access.field() {
                        let field_name = self.interner.resolve_expect(field.sym()).to_string();
                        // `Array.prototype.forEach` reaches only an Array
                        // receiver whose prototype is still tracked as intrinsic.
                        let receiver_uses_array_for_each = field_name == "forEach"
                            && !self.array_prototype_mutated
                            && receiver
                                .possible_kinds
                                .is_subset_of(KindSet::from_kind(ValueKind::Array))
                            && !Self::array_shape_has_custom_prototype(&receiver);
                        // A callback-taking iterator helper needs both the
                        // receiver's chain to name the `%Iterator.prototype%`
                        // builtin and that prototype to still hold it.
                        let receiver_uses_iterator_method = self
                            .shaped_receiver_intrinsic_method(
                                &receiver,
                                IntrinsicPrototype::Iterator,
                                &field_name,
                            )
                            .is_some_and(|method| {
                                matches!(
                                    method.builtin(),
                                    StandardBuiltinId::IteratorPrototypeForEach
                                        | StandardBuiltinId::IteratorPrototypeEvery
                                        | StandardBuiltinId::IteratorPrototypeSome
                                        | StandardBuiltinId::IteratorPrototypeFind
                                        | StandardBuiltinId::IteratorPrototypeReduce
                                        | StandardBuiltinId::IteratorPrototypeMap
                                        | StandardBuiltinId::IteratorPrototypeFilter
                                        | StandardBuiltinId::IteratorPrototypeFlatMap
                                        | StandardBuiltinId::IteratorPrototypeTake
                                        | StandardBuiltinId::IteratorPrototypeDrop
                                        | StandardBuiltinId::IteratorPrototypeChunks
                                        | StandardBuiltinId::IteratorPrototypeWindows
                                )
                            });
                        if receiver_uses_array_for_each || receiver_uses_iterator_method {
                            let args = self
                                .lower_call_args_expanding_spread(args)
                                .into_arguments_after_expression(&mut receiver);
                            if !matches!(
                                field_name.as_str(),
                                "take" | "drop" | "chunks" | "windows"
                            ) {
                                if let Some(callback) = args.first() {
                                    if let Some(callback_id) =
                                        self.resolve_single_function_target(callback)
                                    {
                                        let dynamic_value = ValueInfo {
                                            kind: ValueKind::Dynamic,
                                            possible_kinds: KindSet::all_runtime_tags(),
                                            heap_shape: None,
                                            function_targets: FunctionTargetKnowledge::unknown(),
                                        };
                                        if field_name == "reduce" || field_name == "reduceRight" {
                                            self.merge_function_param_infos(
                                                &callback_id,
                                                &[
                                                    dynamic_value.clone(),
                                                    dynamic_value.clone(),
                                                    ValueInfo::new(ValueKind::Number),
                                                    dynamic_value,
                                                ],
                                            );
                                        } else {
                                            self.merge_function_param_infos(
                                                &callback_id,
                                                &[dynamic_value, ValueInfo::new(ValueKind::Number)],
                                            );
                                        }
                                        self.merge_function_this_info(
                                            &callback_id,
                                            ValueInfo::undefined(),
                                        );
                                    }
                                }
                            }
                            let result_info = match field_name.as_str() {
                                "every" | "some" => ValueInfo::new(ValueKind::Boolean),
                                "find" | "reduce" | "reduceRight" => ValueInfo {
                                    kind: ValueKind::Dynamic,
                                    possible_kinds: KindSet::all_runtime_tags(),
                                    heap_shape: None,
                                    function_targets: FunctionTargetKnowledge::unknown(),
                                },
                                "map" | "filter" | "flatMap" | "take" | "drop" | "chunks"
                                | "windows" => ValueInfo {
                                    kind: ValueKind::Object,
                                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                                    heap_shape: Some(match field_name.as_str() {
                                        "map" => Self::iterator_map_helper_shape(),
                                        "filter" => Self::iterator_filter_helper_shape(),
                                        "flatMap" => Self::iterator_flat_map_helper_shape(),
                                        "take" => Self::iterator_take_helper_shape(),
                                        "chunks" | "windows" => {
                                            Self::iterator_helper_instance_shape()
                                        }
                                        _ => Self::iterator_drop_helper_shape(),
                                    }),
                                    function_targets: FunctionTargetKnowledge::none(),
                                },
                                _ => ValueInfo::undefined(),
                            };
                            let callback_runs_synchronously = matches!(
                                field_name.as_str(),
                                "forEach" | "every" | "some" | "find" | "reduce" | "reduceRight"
                            );
                            let result = TypedExpr::from_info(
                                result_info,
                                ExprIr::CallMethod {
                                    receiver: Box::new(receiver),
                                    key: PropertyKeyIr::StaticString(field_name),
                                    args,
                                },
                            );
                            if callback_runs_synchronously {
                                self.invalidate_unknown_user_code_effects();
                            }
                            return result;
                        }
                    }
                    // Acquire every property before lowering arguments. Live intrinsic
                    // and own-property proofs belong to the shared Get owner below;
                    // receiver kind and a protocol-key spelling cannot license a callee.
                    let receiver_has_known_own_property = match access.field() {
                        PropertyAccessField::Const(field) => {
                            let field_name = self.interner.resolve_expect(field.sym()).to_string();
                            self.read_own_object_shape_property(&receiver, &field_name)
                                .is_some()
                        }
                        PropertyAccessField::Expr(_) => false,
                    };
                    let mut callee = match receiver.kind {
                        ValueKind::Object | ValueKind::Function => {
                            self.lower_object_property_key(receiver.clone(), access.field())
                        }
                        ValueKind::String => self.lower_primitive_property_key(
                            IntrinsicPrototype::String,
                            receiver.clone(),
                            access.field(),
                        ),
                        ValueKind::Number => self.lower_primitive_property_key(
                            IntrinsicPrototype::Number,
                            receiver.clone(),
                            access.field(),
                        ),
                        ValueKind::Boolean => self.lower_primitive_property_key(
                            IntrinsicPrototype::Boolean,
                            receiver.clone(),
                            access.field(),
                        ),
                        ValueKind::BigInt => self.lower_primitive_property_key(
                            IntrinsicPrototype::BigInt,
                            receiver.clone(),
                            access.field(),
                        ),
                        ValueKind::Symbol => self.lower_primitive_property_key(
                            IntrinsicPrototype::Symbol,
                            receiver.clone(),
                            access.field(),
                        ),
                        ValueKind::Array
                            if self.array_prototype_mutated
                                || Self::array_shape_has_custom_prototype(&receiver)
                                || receiver_has_known_own_property =>
                        {
                            self.lower_object_property_key(receiver.clone(), access.field())
                        }
                        ValueKind::Array => {
                            if let PropertyAccessField::Const(field) = access.field() {
                                let field_name =
                                    self.interner.resolve_expect(field.sym()).to_string();
                                let builtin = match field_name.as_str() {
                                    "pop" => Some(StandardBuiltinId::ArrayPrototypePop),
                                    "push" => Some(StandardBuiltinId::ArrayPrototypePush),
                                    "shift" => Some(StandardBuiltinId::ArrayPrototypeShift),
                                    "unshift" => Some(StandardBuiltinId::ArrayPrototypeUnshift),
                                    "fill" => Some(StandardBuiltinId::ArrayPrototypeFill),
                                    "sort" => Some(StandardBuiltinId::ArrayPrototypeSort),
                                    "keys" => Some(StandardBuiltinId::ArrayPrototypeKeys),
                                    "entries" => Some(StandardBuiltinId::ArrayPrototypeEntries),
                                    "values" => Some(StandardBuiltinId::ArrayPrototypeValues),
                                    "concat" => Some(StandardBuiltinId::ArrayPrototypeConcat),
                                    "join" => Some(StandardBuiltinId::ArrayPrototypeJoin),
                                    "slice" => Some(StandardBuiltinId::ArrayPrototypeSlice),
                                    "splice" => Some(StandardBuiltinId::ArrayPrototypeSplice),
                                    "toString" => {
                                        Some(StandardBuiltinId::TypedArrayPrototypeToString)
                                    }
                                    "toLocaleString" => {
                                        Some(StandardBuiltinId::ArrayPrototypeToLocaleString)
                                    }
                                    "flat" => Some(StandardBuiltinId::ArrayPrototypeFlat),
                                    "flatMap" => Some(StandardBuiltinId::ArrayPrototypeFlatMap),
                                    "at" => Some(StandardBuiltinId::ArrayPrototypeAt),
                                    "toReversed" => {
                                        Some(StandardBuiltinId::ArrayPrototypeToReversed)
                                    }
                                    "toSpliced" => Some(StandardBuiltinId::ArrayPrototypeToSpliced),
                                    "toSorted" => Some(StandardBuiltinId::ArrayPrototypeToSorted),
                                    "with" => Some(StandardBuiltinId::ArrayPrototypeWith),
                                    "reverse" => Some(StandardBuiltinId::ArrayPrototypeReverse),
                                    "copyWithin" => {
                                        Some(StandardBuiltinId::ArrayPrototypeCopyWithin)
                                    }
                                    "includes" => Some(StandardBuiltinId::ArrayPrototypeIncludes),
                                    "indexOf" => Some(StandardBuiltinId::ArrayPrototypeIndexOf),
                                    "lastIndexOf" => {
                                        Some(StandardBuiltinId::ArrayPrototypeLastIndexOf)
                                    }
                                    "find" => Some(StandardBuiltinId::ArrayPrototypeFind),
                                    "findIndex" => Some(StandardBuiltinId::ArrayPrototypeFindIndex),
                                    "findLast" => Some(StandardBuiltinId::ArrayPrototypeFindLast),
                                    "findLastIndex" => {
                                        Some(StandardBuiltinId::ArrayPrototypeFindLastIndex)
                                    }
                                    "every" => Some(StandardBuiltinId::ArrayPrototypeEvery),
                                    "some" => Some(StandardBuiltinId::ArrayPrototypeSome),
                                    "forEach" => Some(StandardBuiltinId::ArrayPrototypeForEach),
                                    "filter" => Some(StandardBuiltinId::ArrayPrototypeFilter),
                                    "map" => Some(StandardBuiltinId::ArrayPrototypeMap),
                                    "reduce" => Some(StandardBuiltinId::ArrayPrototypeReduce),
                                    "reduceRight" => {
                                        Some(StandardBuiltinId::ArrayPrototypeReduceRight)
                                    }
                                    _ => None,
                                };
                                if field_name == "forEach" {
                                    let args = self
                                        .lower_call_args_expanding_spread(args)
                                        .into_arguments_after_expression(&mut receiver);
                                    if let Some(callback) = args.first() {
                                        if let Some(function_id) =
                                            self.resolve_single_function_target(callback)
                                        {
                                            let callback_arg_infos = [
                                                ValueInfo {
                                                    kind: ValueKind::Dynamic,
                                                    possible_kinds: KindSet::all_runtime_tags(),
                                                    heap_shape: None,
                                                    function_targets:
                                                        FunctionTargetKnowledge::unknown(),
                                                },
                                                ValueInfo::new(ValueKind::Number),
                                                receiver.value_info(),
                                            ];
                                            let callback_this_info = args
                                                .get(1)
                                                .map(TypedExpr::value_info)
                                                .unwrap_or_else(ValueInfo::undefined);
                                            self.merge_function_param_infos(
                                                &function_id,
                                                &callback_arg_infos,
                                            );
                                            self.merge_function_this_info(
                                                &function_id,
                                                callback_this_info.clone(),
                                            );
                                            let original_function_id =
                                                self.original_exact_function_id(&function_id);
                                            if let Some(helper_context_id) = self
                                                .exact_context_callback_targets
                                                .get(&original_function_id)
                                                .cloned()
                                            {
                                                self.observe_exact_callback_param_infos(
                                                    &original_function_id,
                                                    &helper_context_id,
                                                    &callback_arg_infos,
                                                );
                                                self.observe_exact_callback_this_info(
                                                    &original_function_id,
                                                    &helper_context_id,
                                                    callback_this_info,
                                                );
                                            }
                                        }
                                    }
                                    let result = TypedExpr::from_info(
                                        ValueInfo::undefined(),
                                        ExprIr::CallMethod {
                                            receiver: Box::new(receiver),
                                            key: PropertyKeyIr::StaticString(field_name),
                                            args,
                                        },
                                    );
                                    self.invalidate_unknown_user_code_effects();
                                    return result;
                                }
                                if let Some(builtin) = builtin {
                                    TypedExpr::from_info(
                                        Self::standard_builtin_value_info(builtin),
                                        ExprIr::PropertyRead {
                                            target: Box::new(receiver.clone()),
                                            key: PropertyKeyIr::StaticString(field_name),
                                        },
                                    )
                                } else {
                                    self.lower_array_index_key(receiver.clone(), access.field())
                                }
                            } else {
                                self.lower_array_index_key(receiver.clone(), access.field())
                            }
                        }
                        ValueKind::Arguments => {
                            self.lower_arguments_index_key(receiver.clone(), access.field())
                        }
                        ValueKind::Dynamic
                            if receiver.heap_shape.is_some()
                                && matches!(access.field(), PropertyAccessField::Const(_)) =>
                        {
                            self.lower_object_property_key(receiver.clone(), access.field())
                        }
                        // An unshaped receiver that is an Array (or nullish, which
                        // throws before the call) reaches `%Array.prototype%`
                        // under the same policy as the `ValueKind::Array` arm. A
                        // receiver that may also be a function, string or other
                        // object may resolve the name elsewhere, so nothing here
                        // may claim a builtin for it.
                        ValueKind::Dynamic
                            if !self.array_prototype_mutated
                                && receiver.possible_kinds.contains(ValueKind::Array)
                                && receiver.possible_kinds.is_subset_of(
                                    KindSet::from_kind(ValueKind::Array)
                                        .union(KindSet::from_kind(ValueKind::Undefined))
                                        .union(KindSet::from_kind(ValueKind::Null)),
                                ) =>
                        {
                            if let PropertyAccessField::Const(field) = access.field() {
                                let field_name =
                                    self.interner.resolve_expect(field.sym()).to_string();
                                if field_name == "forEach" {
                                    let args = self
                                        .lower_call_args_expanding_spread(args)
                                        .into_arguments_after_expression(&mut receiver);
                                    if let Some(callback) = args.first() {
                                        if let Some(function_id) =
                                            self.resolve_single_function_target(callback)
                                        {
                                            self.merge_function_param_infos(
                                                &function_id,
                                                &[
                                                    ValueInfo {
                                                        kind: ValueKind::Dynamic,
                                                        possible_kinds: KindSet::all_runtime_tags(),
                                                        heap_shape: None,
                                                        function_targets:
                                                            FunctionTargetKnowledge::unknown(),
                                                    },
                                                    ValueInfo::new(ValueKind::Number),
                                                ],
                                            );
                                        }
                                    }
                                    let result = TypedExpr::from_info(
                                        ValueInfo::undefined(),
                                        ExprIr::CallMethod {
                                            receiver: Box::new(receiver),
                                            key: PropertyKeyIr::StaticString(field_name),
                                            args,
                                        },
                                    );
                                    self.invalidate_unknown_user_code_effects();
                                    return result;
                                }
                                let builtin = match field_name.as_str() {
                                    "pop" => Some(StandardBuiltinId::ArrayPrototypePop),
                                    "push" => Some(StandardBuiltinId::ArrayPrototypePush),
                                    "shift" => Some(StandardBuiltinId::ArrayPrototypeShift),
                                    "unshift" => Some(StandardBuiltinId::ArrayPrototypeUnshift),
                                    "fill" => Some(StandardBuiltinId::ArrayPrototypeFill),
                                    "sort" => Some(StandardBuiltinId::ArrayPrototypeSort),
                                    "keys" => Some(StandardBuiltinId::ArrayPrototypeKeys),
                                    "entries" => Some(StandardBuiltinId::ArrayPrototypeEntries),
                                    "values" => Some(StandardBuiltinId::ArrayPrototypeValues),
                                    "concat" => Some(StandardBuiltinId::ArrayPrototypeConcat),
                                    "join" => Some(StandardBuiltinId::ArrayPrototypeJoin),
                                    "slice" => Some(StandardBuiltinId::ArrayPrototypeSlice),
                                    "splice" => Some(StandardBuiltinId::ArrayPrototypeSplice),
                                    "toString" => {
                                        Some(StandardBuiltinId::TypedArrayPrototypeToString)
                                    }
                                    "toLocaleString" => {
                                        Some(StandardBuiltinId::ArrayPrototypeToLocaleString)
                                    }
                                    "flat" => Some(StandardBuiltinId::ArrayPrototypeFlat),
                                    "flatMap" => Some(StandardBuiltinId::ArrayPrototypeFlatMap),
                                    "at" => Some(StandardBuiltinId::ArrayPrototypeAt),
                                    "toReversed" => {
                                        Some(StandardBuiltinId::ArrayPrototypeToReversed)
                                    }
                                    "toSpliced" => Some(StandardBuiltinId::ArrayPrototypeToSpliced),
                                    "toSorted" => Some(StandardBuiltinId::ArrayPrototypeToSorted),
                                    "with" => Some(StandardBuiltinId::ArrayPrototypeWith),
                                    "reverse" => Some(StandardBuiltinId::ArrayPrototypeReverse),
                                    "copyWithin" => {
                                        Some(StandardBuiltinId::ArrayPrototypeCopyWithin)
                                    }
                                    "includes" => Some(StandardBuiltinId::ArrayPrototypeIncludes),
                                    "indexOf" => Some(StandardBuiltinId::ArrayPrototypeIndexOf),
                                    "lastIndexOf" => {
                                        Some(StandardBuiltinId::ArrayPrototypeLastIndexOf)
                                    }
                                    "find" => Some(StandardBuiltinId::ArrayPrototypeFind),
                                    "findIndex" => Some(StandardBuiltinId::ArrayPrototypeFindIndex),
                                    "findLast" => Some(StandardBuiltinId::ArrayPrototypeFindLast),
                                    "findLastIndex" => {
                                        Some(StandardBuiltinId::ArrayPrototypeFindLastIndex)
                                    }
                                    "every" => Some(StandardBuiltinId::ArrayPrototypeEvery),
                                    "some" => Some(StandardBuiltinId::ArrayPrototypeSome),
                                    "forEach" => Some(StandardBuiltinId::ArrayPrototypeForEach),
                                    "filter" => Some(StandardBuiltinId::ArrayPrototypeFilter),
                                    "map" => Some(StandardBuiltinId::ArrayPrototypeMap),
                                    "reduce" => Some(StandardBuiltinId::ArrayPrototypeReduce),
                                    "reduceRight" => {
                                        Some(StandardBuiltinId::ArrayPrototypeReduceRight)
                                    }
                                    _ => None,
                                };
                                if let Some(builtin) = builtin {
                                    TypedExpr::from_info(
                                        Self::standard_builtin_value_info(builtin),
                                        ExprIr::PropertyRead {
                                            target: Box::new(receiver.clone()),
                                            key: PropertyKeyIr::StaticString(field_name),
                                        },
                                    )
                                } else {
                                    return self.unsupported_expr(
                                        "indirect call: unsupported dynamic array property",
                                    );
                                }
                            } else {
                                self.lower_object_property_key(receiver.clone(), access.field())
                            }
                        }
                        ValueKind::Dynamic => {
                            self.lower_object_property_key(receiver.clone(), access.field())
                        }
                        _ => {
                            let dynamic_receiver = TypedExpr::from_info(
                                ValueInfo {
                                    kind: ValueKind::Dynamic,
                                    possible_kinds: KindSet::all_runtime_tags(),
                                    heap_shape: receiver.heap_shape.clone(),
                                    function_targets: FunctionTargetKnowledge::unknown(),
                                },
                                receiver.expr.clone(),
                            );
                            self.lower_object_property_key(dynamic_receiver, access.field())
                        }
                    };
                    if self.intervening_effect_epoch != receiver_capture_epoch {
                        receiver.heap_shape = None;
                    }
                    let Some(function_id) = self.resolve_single_function_target(&callee) else {
                        let source_args = args;
                        let args = self
                            .lower_call_args_expanding_spread(source_args)
                            .into_arguments_after_two_expressions(&mut callee, &mut receiver);
                        let analysis = self.analyze_known_call_candidates(
                            &callee.value_info(),
                            Some(&receiver.value_info()),
                            &args,
                            CallCandidateSource::IndirectSyntax(source_args),
                        );
                        let CallCandidateAnalysis::Accepted { result, effects } = analysis else {
                            return TypedExpr::undefined();
                        };
                        return self.lower_indirect_method_call(
                            result, callee, receiver, args, None, effects,
                        );
                    };
                    let Some(signature) = self.function_signatures.get(&function_id) else {
                        return self
                            .unsupported_expr("indirect call: missing property target signature");
                    };
                    if !signature.callable
                        && signature.protocol.class_kind() != ClassFunctionKind::Constructor
                    {
                        return self.unsupported_expr("indirect call");
                    }
                    self.mark_host_builtin_from_function_id(&function_id);
                    self.host_builtin_calls +=
                        usize::from(HostBuiltinId::from_function_id(&function_id).is_some());
                    if let Some((array_builtin, info)) =
                        StandardBuiltinId::from_function_id(&function_id).and_then(|builtin| {
                            Self::inferred_indexed_collection_result_info(builtin)
                                .map(|info| (builtin, info))
                        })
                    {
                        let args = self
                            .lower_call_args_expanding_spread(args)
                            .into_arguments_after_two_expressions(&mut callee, &mut receiver);
                        // An Array.prototype method can be copied onto an
                        // arbitrary object after the prototype was mutated.
                        // Its inferred builtin target is not enough to prove
                        // that the receiver satisfies the Array fast path.
                        if self.array_prototype_mutated
                            && !receiver
                                .possible_kinds
                                .is_subset_of(KindSet::from_kind(ValueKind::Array))
                        {
                            self.observe_unaccounted_invocation_effects(
                                InvocationTargetProvenance::from(&callee),
                            );
                            return self.lower_indirect_method_call(
                                ValueInfo::new(ValueKind::Dynamic),
                                callee,
                                receiver,
                                args,
                                None,
                                AnalyzedInvocationEffects::already_applied(),
                            );
                        }
                        self.record_builtin_receiver_mutation(array_builtin);
                        if array_builtin == StandardBuiltinId::ArrayPrototypePush {
                            if let Some(base_len) = Self::static_array_shape_len(&receiver) {
                                for (arg_offset, arg) in args.iter().enumerate() {
                                    let index = base_len + arg_offset;
                                    if index > MAX_STATIC_ARRAY_SHAPE_INDEX {
                                        self.clear_binding_shape(access.target());
                                        break;
                                    }
                                    let key =
                                        PropertyKeyIr::ArrayIndex(Box::new(TypedExpr::from_info(
                                            ValueInfo::new(ValueKind::Number),
                                            ExprIr::Number((index as f64).to_bits()),
                                        )));
                                    self.update_written_shape(
                                        access.target(),
                                        &key,
                                        &arg.value_info(),
                                    );
                                }
                            } else {
                                self.clear_binding_shape(access.target());
                            }
                        } else if matches!(
                            array_builtin,
                            StandardBuiltinId::ArrayPrototypeShift
                                | StandardBuiltinId::ArrayPrototypeUnshift
                        ) {
                            self.clear_binding_shape(access.target());
                        }
                        if matches!(
                            array_builtin,
                            StandardBuiltinId::ArrayPrototypeConcat
                                | StandardBuiltinId::ArrayPrototypeSlice
                                | StandardBuiltinId::ArrayPrototypeSplice
                                | StandardBuiltinId::ArrayPrototypeFlat
                                | StandardBuiltinId::ArrayPrototypeFlatMap
                                | StandardBuiltinId::ArrayPrototypeFilter
                                | StandardBuiltinId::ArrayPrototypeMap
                        ) {
                            self.merge_array_species_constructor_this_info(&receiver);
                        }
                        if matches!(
                            array_builtin,
                            StandardBuiltinId::ArrayPrototypeReduce
                                | StandardBuiltinId::ArrayPrototypeReduceRight
                                | StandardBuiltinId::TypedArrayPrototypeReduce
                                | StandardBuiltinId::TypedArrayPrototypeReduceRight
                        ) {
                            if let Some(callback) = args.first() {
                                if let Some(callback_id) =
                                    self.resolve_single_function_target(callback)
                                {
                                    self.merge_function_param_infos(
                                        &callback_id,
                                        &[
                                            ValueInfo {
                                                kind: ValueKind::Dynamic,
                                                possible_kinds: KindSet::all_runtime_tags(),
                                                heap_shape: None,
                                                function_targets: FunctionTargetKnowledge::unknown(
                                                ),
                                            },
                                            ValueInfo {
                                                kind: ValueKind::Dynamic,
                                                possible_kinds: KindSet::all_runtime_tags(),
                                                heap_shape: None,
                                                function_targets: FunctionTargetKnowledge::unknown(
                                                ),
                                            },
                                            ValueInfo::new(ValueKind::Number),
                                            receiver.value_info(),
                                        ],
                                    );
                                    self.merge_function_this_info(
                                        &callback_id,
                                        ValueInfo::undefined(),
                                    );
                                }
                            }
                        } else if matches!(
                            array_builtin,
                            StandardBuiltinId::ArrayPrototypeFlatMap
                                | StandardBuiltinId::ArrayPrototypeFind
                                | StandardBuiltinId::ArrayPrototypeFindIndex
                                | StandardBuiltinId::ArrayPrototypeFindLast
                                | StandardBuiltinId::ArrayPrototypeFindLastIndex
                                | StandardBuiltinId::ArrayPrototypeEvery
                                | StandardBuiltinId::ArrayPrototypeSome
                                | StandardBuiltinId::TypedArrayPrototypeEvery
                                | StandardBuiltinId::TypedArrayPrototypeSome
                                | StandardBuiltinId::TypedArrayPrototypeFind
                                | StandardBuiltinId::TypedArrayPrototypeFindIndex
                                | StandardBuiltinId::TypedArrayPrototypeFindLast
                                | StandardBuiltinId::TypedArrayPrototypeFindLastIndex
                                | StandardBuiltinId::TypedArrayPrototypeMap
                                | StandardBuiltinId::TypedArrayPrototypeFilter
                                | StandardBuiltinId::TypedArrayPrototypeForEach
                                | StandardBuiltinId::ArrayPrototypeForEach
                                | StandardBuiltinId::ArrayPrototypeFilter
                                | StandardBuiltinId::ArrayPrototypeMap
                        ) {
                            if let Some(callback) = args.first() {
                                if let Some(callback_id) =
                                    self.resolve_single_function_target(callback)
                                {
                                    let callback_arg_infos = [
                                        ValueInfo {
                                            kind: ValueKind::Dynamic,
                                            possible_kinds: KindSet::all_runtime_tags(),
                                            heap_shape: None,
                                            function_targets: FunctionTargetKnowledge::unknown(),
                                        },
                                        ValueInfo::new(ValueKind::Number),
                                        receiver.value_info(),
                                    ];
                                    let callback_this_info = args
                                        .get(1)
                                        .map(TypedExpr::value_info)
                                        .unwrap_or_else(ValueInfo::undefined);
                                    self.merge_function_param_infos(
                                        &callback_id,
                                        &callback_arg_infos,
                                    );
                                    self.merge_function_this_info(
                                        &callback_id,
                                        callback_this_info.clone(),
                                    );
                                    let original_callback_id =
                                        self.original_exact_function_id(&callback_id);
                                    if let Some(helper_context_id) = self
                                        .exact_context_callback_targets
                                        .get(&original_callback_id)
                                        .cloned()
                                    {
                                        self.observe_exact_callback_param_infos(
                                            &original_callback_id,
                                            &helper_context_id,
                                            &callback_arg_infos,
                                        );
                                        self.observe_exact_callback_this_info(
                                            &original_callback_id,
                                            &helper_context_id,
                                            callback_this_info,
                                        );
                                    }
                                }
                            }
                        }
                        if matches!(
                            array_builtin,
                            StandardBuiltinId::ArrayPrototypeSort
                                | StandardBuiltinId::ArrayPrototypeToSorted
                        ) {
                            if let Some(callback_id) = args
                                .first()
                                .and_then(|callback| self.resolve_single_function_target(callback))
                            {
                                self.merge_function_param_infos(
                                    &callback_id,
                                    &[
                                        ValueInfo {
                                            kind: ValueKind::Dynamic,
                                            possible_kinds: KindSet::all_runtime_tags(),
                                            heap_shape: None,
                                            function_targets: FunctionTargetKnowledge::unknown(),
                                        },
                                        ValueInfo {
                                            kind: ValueKind::Dynamic,
                                            possible_kinds: KindSet::all_runtime_tags(),
                                            heap_shape: None,
                                            function_targets: FunctionTargetKnowledge::unknown(),
                                        },
                                    ],
                                );
                                self.merge_function_this_info(&callback_id, ValueInfo::undefined());
                            }
                        }
                        // Even methods without a callback can observe getters,
                        // coercions or species. The catalog flags alone do not
                        // prove that these captured flow facts survive the call.
                        self.invalidate_unknown_user_code_effects();
                        return self.lower_indirect_method_call(
                            info,
                            callee,
                            receiver,
                            args,
                            None,
                            AnalyzedInvocationEffects::already_applied(),
                        );
                    }
                    let source_arguments = args;
                    let (args, mut info, mut invocation_effects) = self.lower_call_args(
                        &function_id,
                        source_arguments,
                        InvocationThisObservation::ExplicitMethod {
                            receiver: &mut receiver,
                            callee: &mut callee,
                        },
                    );
                    if StandardBuiltinId::from_function_id(&function_id)
                        == Some(StandardBuiltinId::StringPrototypeConcat)
                    {
                        // A known String builtin can be stored under any
                        // property name and borrowed by any receiver. The
                        // acquired callee and its original base define Call.
                        return self.lower_indirect_method_call(
                            info,
                            callee,
                            receiver,
                            args,
                            None,
                            invocation_effects,
                        );
                    }
                    if let Some(method) = NonGenericBuiltinMethod::from_function_id(&function_id) {
                        match method {
                            NonGenericBuiltinMethod::BooleanToString
                            | NonGenericBuiltinMethod::BooleanValueOf
                            | NonGenericBuiltinMethod::NumberToExponential
                            | NonGenericBuiltinMethod::NumberToFixed
                            | NonGenericBuiltinMethod::NumberToLocaleString
                            | NonGenericBuiltinMethod::NumberToPrecision
                            | NonGenericBuiltinMethod::NumberToString
                            | NonGenericBuiltinMethod::NumberValueOf
                            | NonGenericBuiltinMethod::BigIntToString
                            | NonGenericBuiltinMethod::BigIntToLocaleString
                            | NonGenericBuiltinMethod::BigIntValueOf
                            | NonGenericBuiltinMethod::StringToString
                            | NonGenericBuiltinMethod::StringValueOf => {
                                // Target inference identifies the function
                                // that was acquired, not the receiver's
                                // primitive brand. Keep both Reference
                                // components so a transferred method reaches
                                // its own closed receiver check.
                                return self.lower_indirect_method_call(
                                    info,
                                    callee,
                                    receiver,
                                    args,
                                    None,
                                    invocation_effects,
                                );
                            }
                        }
                    }
                    if matches!(
                        StandardBuiltinId::from_function_id(&function_id),
                        Some(
                            StandardBuiltinId::FunctionPrototypeCall
                                | StandardBuiltinId::FunctionPrototypeApply
                                | StandardBuiltinId::FunctionPrototypeBind
                        )
                    ) {
                        let forwarded_dynamic_source_result = if matches!(
                            StandardBuiltinId::from_function_id(&function_id),
                            Some(StandardBuiltinId::FunctionPrototypeCall)
                        ) && !Self::call_args_have_spread(
                            &args,
                        ) {
                            match self.preflight_function_prototype_call_dynamic_source(
                                &receiver.value_info(),
                                source_arguments,
                                &args,
                            ) {
                                DynamicSourceCallAdmission::Admitted(admission) => self
                                    .consume_forwarded_dynamic_source_admission(
                                        &receiver.value_info(),
                                        admission,
                                    ),
                                DynamicSourceCallAdmission::Rejected => {
                                    return TypedExpr::undefined();
                                }
                            }
                        } else {
                            None
                        };
                        if let Some(target_function_id) = matches!(
                            InvocationTargetProvenance::from(&receiver),
                            InvocationTargetProvenance::ProvenFunction(_)
                        )
                        .then(|| self.resolve_single_function_target(&receiver))
                        .flatten()
                        {
                            if let Some(signature) =
                                self.function_signature_for_current_flow(&target_function_id)
                            {
                                match StandardBuiltinId::from_function_id(&function_id) {
                                    Some(StandardBuiltinId::FunctionPrototypeCall)
                                    | Some(StandardBuiltinId::FunctionPrototypeApply) => {
                                        if signature.protocol.flavor() != FunctionFlavor::Arrow {
                                            let this_info = match args.first() {
                                                Some(this_arg) => self
                                                    .explicit_this_info_for_function_target(
                                                        &target_function_id,
                                                        this_arg,
                                                        signature.this_info.clone(),
                                                    ),
                                                None => self.default_this_info_for_function_target(
                                                    &target_function_id,
                                                ),
                                            };
                                            self.merge_function_this_info(
                                                &target_function_id,
                                                this_info,
                                            );
                                            let forwarded_args = if matches!(
                                                StandardBuiltinId::from_function_id(&function_id),
                                                Some(StandardBuiltinId::FunctionPrototypeCall)
                                            ) {
                                                Some(
                                                    args.iter()
                                                        .skip(1)
                                                        .map(TypedExpr::value_info)
                                                        .collect::<Vec<_>>(),
                                                )
                                            } else {
                                                self.forwarded_apply_args(args.get(1)).map(
                                                    |forwarded_args| {
                                                        forwarded_args
                                                            .iter()
                                                            .map(TypedExpr::value_info)
                                                            .collect()
                                                    },
                                                )
                                            };
                                            if let Some(forwarded_args) = forwarded_args {
                                                self.merge_function_param_infos(
                                                    &target_function_id,
                                                    &forwarded_args,
                                                );
                                            }
                                            if matches!(
                                                StandardBuiltinId::from_function_id(
                                                    &target_function_id
                                                ),
                                                Some(StandardBuiltinId::ArrayPrototypeSort)
                                            ) && matches!(
                                                StandardBuiltinId::from_function_id(&function_id),
                                                Some(StandardBuiltinId::FunctionPrototypeCall)
                                            ) {
                                                if let Some(callback_id) =
                                                    args.get(1).and_then(|callback| {
                                                        self.resolve_single_function_target(
                                                            callback,
                                                        )
                                                    })
                                                {
                                                    self.merge_function_param_infos(
                                                        &callback_id,
                                                        &[
                                                            ValueInfo {
                                                                kind: ValueKind::Dynamic,
                                                                possible_kinds:
                                                                    KindSet::all_runtime_tags(),
                                                                heap_shape: None,
                                                                function_targets: FunctionTargetKnowledge::unknown(),
                                                            },
                                                            ValueInfo {
                                                                kind: ValueKind::Dynamic,
                                                                possible_kinds:
                                                                    KindSet::all_runtime_tags(),
                                                                heap_shape: None,
                                                                function_targets: FunctionTargetKnowledge::unknown(),
                                                            },
                                                        ],
                                                    );
                                                    self.merge_function_this_info(
                                                        &callback_id,
                                                        ValueInfo::undefined(),
                                                    );
                                                }
                                            }
                                        }
                                        if signature.protocol.class_kind()
                                            != ClassFunctionKind::Constructor
                                        {
                                            info = self.function_call_return_info(&signature);
                                        }
                                        if let Some(target_builtin) =
                                            Self::define_property_builtin(&target_function_id)
                                        {
                                            info = match StandardBuiltinId::from_function_id(
                                                &function_id,
                                            ) {
                                                Some(StandardBuiltinId::FunctionPrototypeCall)
                                                    if !Self::call_args_have_spread(&args) =>
                                                {
                                                    let analysis = self
                                                        .forwarded_define_property_call_analysis(
                                                            target_builtin,
                                                            args.get(1..).unwrap_or_default(),
                                                        );
                                                    let (result, effects) = analysis.into_parts();
                                                    invocation_effects = effects;
                                                    result
                                                }
                                                Some(StandardBuiltinId::FunctionPrototypeApply)
                                                    if !Self::call_args_have_spread(&args) =>
                                                {
                                                    match self.forwarded_apply_args(args.get(1)) {
                                                        Some(forwarded_args) => {
                                                            let analysis = self
                                                                .forwarded_define_property_call_analysis(
                                                                    target_builtin,
                                                                    &forwarded_args,
                                                                );
                                                            let (result, effects) =
                                                                analysis.into_parts();
                                                            invocation_effects = effects;
                                                            result
                                                        }
                                                        None => {
                                                            let (result, effects) = self
                                                                .unknown_forwarded_define_property_call_analysis(
                                                                    target_builtin,
                                                                )
                                                                .into_parts();
                                                            invocation_effects = effects;
                                                            result
                                                        }
                                                    }
                                                }
                                                Some(
                                                    StandardBuiltinId::FunctionPrototypeCall
                                                    | StandardBuiltinId::FunctionPrototypeApply,
                                                ) => {
                                                    let (result, effects) = self
                                                        .unknown_forwarded_define_property_call_analysis(
                                                            target_builtin,
                                                        )
                                                        .into_parts();
                                                    invocation_effects = effects;
                                                    result
                                                }
                                                _ => unreachable!(
                                                    "defineProperty forwarding only applies to call/apply"
                                                ),
                                            };
                                        }
                                    }
                                    Some(StandardBuiltinId::FunctionPrototypeBind) => {
                                        if signature.protocol.flavor() != FunctionFlavor::Arrow
                                            && !signature.protocol.is_constructable()
                                        {
                                            if let Some(this_arg) = args.first() {
                                                self.merge_function_this_info(
                                                    &target_function_id,
                                                    this_arg.value_info(),
                                                );
                                            }
                                        }
                                        let bound_arg_infos = args
                                            .iter()
                                            .skip(1)
                                            .map(TypedExpr::value_info)
                                            .collect::<Vec<_>>();
                                        if !bound_arg_infos.is_empty() {
                                            self.merge_function_param_infos(
                                                &target_function_id,
                                                &bound_arg_infos,
                                            );
                                        }
                                        info = ValueInfo {
                                            kind: ValueKind::Function,
                                            possible_kinds: KindSet::from_kind(ValueKind::Function),
                                            heap_shape: Some(Self::function_heap_shape(
                                                signature.protocol.is_constructable(),
                                            )),
                                            function_targets: FunctionTargetKnowledge::unknown(),
                                        };
                                        self.bound_functions += 1;
                                        self.bound_function_constructs +=
                                            usize::from(signature.protocol.is_constructable());
                                    }
                                    _ => {}
                                }
                            }
                        }
                        if matches!(
                            StandardBuiltinId::from_function_id(&function_id),
                            Some(
                                StandardBuiltinId::FunctionPrototypeCall
                                    | StandardBuiltinId::FunctionPrototypeApply
                            )
                        ) && forwarded_dynamic_source_result.is_none()
                        {
                            self.consume_forwarded_call_flow_effects(&receiver);
                        }
                        if let Some(result) = forwarded_dynamic_source_result {
                            info = result;
                        }
                    }
                    if string_from_code_point_apply_call
                        && matches!(
                            StandardBuiltinId::from_function_id(&function_id),
                            Some(StandardBuiltinId::FunctionPrototypeApply)
                        )
                    {
                        info = ValueInfo::new(ValueKind::String);
                    }
                    let static_regexp_compilation = self.static_regexp_compilation_for_direct_call(
                        &callee,
                        &function_id,
                        &args,
                    );
                    return self.lower_indirect_method_call(
                        info,
                        callee,
                        receiver,
                        args,
                        static_regexp_compilation,
                        invocation_effects,
                    );
                }
                PropertyAccess::Private(access) => {
                    let Some(private_name_id) = self.current_private_name_id(access.field()) else {
                        return self.unsupported_expr("private class element");
                    };
                    let mut receiver = self.lower_property_target(access.target());
                    let mut receiver_info = receiver.value_info();
                    let receiver_storage_name =
                        self.alloc_temp_binding_name("private.call.receiver.");
                    let mut materialized_receiver = TypedExpr::from_info(
                        receiver_info.clone(),
                        ExprIr::Identifier(receiver_storage_name.clone()),
                    );
                    let mut callee =
                        self.lower_private_get_value(&mut materialized_receiver, private_name_id);
                    receiver_info = materialized_receiver.value_info();
                    let function_id = matches!(
                        InvocationTargetProvenance::from(&callee),
                        InvocationTargetProvenance::ProvenFunction(_)
                    )
                    .then(|| self.resolve_single_function_target(&callee))
                    .flatten();
                    let (args, info, invocation_effects) = if let Some(function_id) = function_id {
                        let Some(signature) = self.function_signatures.get(&function_id) else {
                            return self.unsupported_expr("indirect call");
                        };
                        if !signature.callable
                            && signature.protocol.class_kind() != ClassFunctionKind::Constructor
                        {
                            return self.unsupported_expr("indirect call");
                        }
                        self.lower_call_args(
                            &function_id,
                            args,
                            InvocationThisObservation::ExplicitValueMethod {
                                receiver: &mut receiver_info,
                                callee: &mut callee,
                            },
                        )
                    } else {
                        let source_args = args;
                        let args = self
                            .lower_call_args_expanding_spread(source_args)
                            .into_arguments_after_expression_and_value(
                                &mut callee,
                                &mut receiver_info,
                            );
                        let analysis = self.analyze_known_call_candidates(
                            &callee.value_info(),
                            Some(&receiver_info),
                            &args,
                            CallCandidateSource::IndirectSyntax(source_args),
                        );
                        let CallCandidateAnalysis::Accepted { result, effects } = analysis else {
                            return TypedExpr::undefined();
                        };
                        (args, result, effects)
                    };
                    receiver.heap_shape = receiver_info.heap_shape.clone();
                    materialized_receiver.heap_shape = receiver_info.heap_shape.clone();
                    let call = invocation_effects.attach_to_emitted_call(TypedExpr::from_info(
                        info.clone(),
                        ExprIr::CallIndirect {
                            direct_eval: None,
                            callee: Box::new(callee),
                            this_arg: Some(Box::new(materialized_receiver)),
                            args,
                            static_regexp_compilation: None,
                        },
                    ));
                    return TypedExpr::from_info(
                        info,
                        ExprIr::MaterializeBinding {
                            name: receiver_storage_name,
                            value: Box::new(receiver),
                            body: Box::new(call),
                        },
                    );
                }
                PropertyAccess::Super(access) => {
                    let mut callee = self.lower_super_property_access(access);
                    let Some(function_id) = matches!(
                        InvocationTargetProvenance::from(&callee),
                        InvocationTargetProvenance::ProvenFunction(_)
                    )
                    .then(|| self.resolve_single_function_target(&callee))
                    .flatten() else {
                        let source_args = args;
                        let args = self
                            .lower_call_args_expanding_spread(source_args)
                            .into_arguments_after_expression(&mut callee);
                        let this_value = self.current_this_info();
                        let analysis = self.analyze_known_call_candidates(
                            &callee.value_info(),
                            Some(&this_value),
                            &args,
                            CallCandidateSource::IndirectSyntax(source_args),
                        );
                        let CallCandidateAnalysis::Accepted { result, effects } = analysis else {
                            return TypedExpr::undefined();
                        };
                        let call = TypedExpr::from_info(
                            result,
                            ExprIr::CallIndirect {
                                direct_eval: None,
                                callee: Box::new(callee),
                                this_arg: Some(Box::new(TypedExpr::from_info(
                                    this_value,
                                    ExprIr::This,
                                ))),
                                args,
                                static_regexp_compilation: None,
                            },
                        );
                        return effects.attach_to_emitted_call(call);
                    };
                    let Some(signature) = self.function_signatures.get(&function_id) else {
                        return self.unsupported_expr("indirect call");
                    };
                    if !signature.callable
                        && signature.protocol.class_kind() != ClassFunctionKind::Constructor
                    {
                        return self.unsupported_expr("indirect call");
                    }
                    let (args, info, invocation_effects) = self.lower_call_args(
                        &function_id,
                        args,
                        InvocationThisObservation::Current,
                    );
                    let call = TypedExpr::from_info(
                        info,
                        ExprIr::CallIndirect {
                            direct_eval: None,
                            callee: Box::new(callee),
                            this_arg: Some(Box::new(TypedExpr::from_info(
                                self.current_this_info(),
                                ExprIr::This,
                            ))),
                            args,
                            static_regexp_compilation: None,
                        },
                    );
                    return invocation_effects.attach_to_emitted_call(call);
                }
            }
        }
        self.lower_non_property_call(callee, args)
    }
}
