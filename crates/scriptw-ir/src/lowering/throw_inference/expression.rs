use super::*;

impl<'a> ScriptLowerer<'a> {
    /// The part of [`Self::infer_expr_throw_info`] that comes from the node's
    /// own operands. Recursive calls go back through the wrapper, so a nested
    /// strict Reference write contributes its TypeError too.
    pub(super) fn infer_expr_operand_throw_info(&self, expr: &TypedExpr) -> Option<ValueInfo> {
        match &expr.expr {
            ExprIr::ForeignCall(_) => Some(unknown_runtime_value_info()),
            // `import()` rejects rather than throws, and reading `import.meta`
            // or a namespace object cannot throw.
            ExprIr::ModuleEntryEvaluation(entry) => self.infer_expr_throw_info(entry.evaluation()),
            ExprIr::ModuleExecutionGraph(_)
            | ExprIr::ModuleBindingRead(_)
            | ExprIr::JsonModuleValue(_)
            | ExprIr::BytesModuleValue(_)
            | ExprIr::ModuleEvaluate(_)
            | ExprIr::DeferredModuleEvaluate(_)
            | ExprIr::ModuleHasAsyncDependencies(_)
            | ExprIr::ModuleDeferredImportEvaluate(_) => Some(unknown_runtime_value_info()),
            ExprIr::ModuleNamespacePublish { namespace, .. } => {
                self.infer_expr_throw_info(namespace)
            }
            ExprIr::DynamicImport { .. }
            | ExprIr::ImportMeta { .. }
            | ExprIr::ModuleNamespace { .. } => None,
            ExprIr::Undefined
            | ExprIr::ArrayHole
            | ExprIr::Null
            | ExprIr::Boolean(_)
            | ExprIr::Number(_)
            | ExprIr::BigInt(_)
            | ExprIr::Symbol { .. }
            | ExprIr::WellKnownSymbol(_)
            | ExprIr::String(_)
            | ExprIr::TemplateObject(_)
            | ExprIr::FunctionValue(_)
            | ExprIr::ClassFieldNamedFunctionValue { .. }
            | ExprIr::This
            | ExprIr::ExecutionGlobalObject
            | ExprIr::Arguments
            | ExprIr::Identifier(_)
            | ExprIr::UpdateIdentifier { .. }
            | ExprIr::NewTarget => None,
            ExprIr::RegExpLiteral {
                static_compilation, ..
            } => match static_compilation {
                // A statically rejected literal throws its SyntaxError on
                // evaluation, like a throwing construct; anything else builds
                // an object without user-observable failure.
                Some(StaticRegExpCompilation::InvalidSyntax { .. }) => {
                    Some(unknown_runtime_value_info())
                }
                Some(StaticRegExpCompilation::Program(_)) | None => None,
            },
            ExprIr::SuperPropertyRead { receiver, .. } => self.infer_expr_throw_info(receiver),
            ExprIr::SuperPropertyMutation(mutation) => {
                let mut info = self.infer_expr_throw_info(mutation.receiver());
                if let PropertyKeyIr::StringExpr(key) | PropertyKeyIr::ArrayIndex(key) =
                    mutation.referenced_name()
                {
                    info = self.merge_optional_value_info(info, self.infer_expr_throw_info(key));
                }
                match mutation.operation() {
                    SuperPropertyMutationOperationIr::NumericUpdate { .. } => info,
                    SuperPropertyMutationOperationIr::Capture(_) => {
                        Some(unknown_runtime_value_info())
                    }
                    SuperPropertyMutationOperationIr::PutCaptured { value, .. } => self
                        .merge_optional_value_info(
                            Some(unknown_runtime_value_info()),
                            self.infer_expr_throw_info(value),
                        ),
                    SuperPropertyMutationOperationIr::EagerCompound { result, .. } => {
                        self.merge_optional_value_info(info, self.infer_expr_throw_info(result))
                    }
                }
            }
            ExprIr::EnvironmentIdentifier(_) => Some(unknown_runtime_value_info()),
            // Global HasBinding/GetBindingValue can throw any value through
            // prototype traps or accessors; typeof exempts absence only. A
            // global the lowerer saw present may be deleted before the read.
            ExprIr::GlobalPropertyRead { .. }
            | ExprIr::GlobalIdentifierRead { .. }
            | ExprIr::TypeOfUnresolvedIdentifier { .. }
            | ExprIr::DeleteGlobalProperty { .. } => Some(unknown_runtime_value_info()),
            // No match here on purpose. `NativeErrorKind::constructor` is total
            // over the nine error intrinsics (20.5.1, 20.5.5, 20.5.7 and
            // Explicit Resource Management), so a tenth kind cannot be omitted
            // at this call site at all — the exhaustiveness obligation lives in
            // the one row list that generates it. The six-arm match this
            // replaced fell through to `ErrorConstructor` for `AggregateError`
            // and `SuppressedError`, which would have typed both as base
            // `Error` and made every downstream `instanceof` and shape
            // inference keyed on the result wrong.
            ExprIr::RuntimeThrow { name, .. } => {
                Some(self.standard_error_instance_info(name.constructor()))
            }
            ExprIr::ObjectLiteral(properties) => {
                let mut info = None;
                for property in properties {
                    info = self.merge_object_property_throw_info(info, property);
                }
                info
            }
            ExprIr::ObjectPropertyDefinition(definition) => {
                let info = self.infer_expr_throw_info(definition.target());
                self.merge_object_property_throw_info(info, definition.property())
            }
            ExprIr::ArrayLiteral(elements) => {
                let mut info = None;
                for element in elements {
                    info =
                        self.merge_optional_value_info(info, self.infer_expr_throw_info(element));
                }
                info
            }
            ExprIr::ArrayAccumulation(accumulation) => {
                let mut info = None;
                for element in accumulation.elements() {
                    let value = match element {
                        ArrayAccumulationElementIr::Elision => continue,
                        ArrayAccumulationElementIr::Value(value) => value,
                        ArrayAccumulationElementIr::Spread(spread) => {
                            // @@iterator lookup and iterator calls may throw an
                            // arbitrary language value supplied by user code.
                            info = self.merge_optional_value_info(
                                info,
                                Some(ValueInfo {
                                    kind: ValueKind::Dynamic,
                                    possible_kinds: KindSet::all_runtime_tags(),
                                    heap_shape: None,
                                    function_targets: FunctionTargetKnowledge::unknown(),
                                }),
                            );
                            &spread.value
                        }
                    };
                    info = self.merge_optional_value_info(info, self.infer_expr_throw_info(value));
                }
                info
            }
            ExprIr::CaptureOptionalCallReference(capture) => {
                let mut info = None;
                for operand in capture.operands() {
                    info =
                        self.merge_optional_value_info(info, self.infer_expr_throw_info(operand));
                }
                info
            }
            ExprIr::CaptureArgumentList(capture) => {
                let mut info = None;
                for argument in capture.arguments() {
                    info =
                        self.merge_optional_value_info(info, self.infer_expr_throw_info(argument));
                }
                // Spread iterator acquisition/stepping can throw any user value.
                if capture
                    .arguments()
                    .iter()
                    .any(|argument| matches!(argument.expr, ExprIr::SpreadArgument(_)))
                {
                    info = self.merge_optional_value_info(info, Some(unknown_runtime_value_info()));
                }
                info
            }
            ExprIr::CapturedArgumentList(list) => self.infer_expr_throw_info(list.binding()),
            ExprIr::AssignIdentifier { value, .. }
            | ExprIr::GlobalPropertyWrite { value, .. }
            | ExprIr::CompoundAssignIdentifier { value, .. }
            | ExprIr::SpreadArgument(SpreadArgumentIr { value, .. })
            | ExprIr::Void { expr: value }
            | ExprIr::DeleteValue { expr: value }
            | ExprIr::TypeOf { expr: value }
            | ExprIr::LogicalNot { expr: value }
            | ExprIr::UnaryPlus { expr: value }
            | ExprIr::UnaryMinusNumeric { expr: value }
            | ExprIr::UnaryBitwiseNumeric { expr: value, .. }
            | ExprIr::StringFromCharCode { code: value } => self.infer_expr_throw_info(value),
            ExprIr::SuperPropertyWrite {
                receiver, value, ..
            } => self.merge_optional_value_info(
                self.infer_expr_throw_info(receiver),
                self.infer_expr_throw_info(value),
            ),
            ExprIr::SpecOperation {
                operation,
                operands,
            } => {
                let mut info = None;
                for operand in operands {
                    info =
                        self.merge_optional_value_info(info, self.infer_expr_throw_info(operand));
                }
                if matches!(
                    operation,
                    SpecOperationIr::WithEnvironmentHasBinding
                        | SpecOperationIr::WithEnvironmentGetBindingValueSloppy
                        | SpecOperationIr::WithEnvironmentGetBindingValueStrict
                ) {
                    info = self.merge_optional_value_info(info, Some(unknown_runtime_value_info()));
                }
                if *operation == SpecOperationIr::HasProperty {
                    if let Some(target) = operands.first() {
                        let object_like = KindSet::from_kind(ValueKind::Object)
                            .union(KindSet::from_kind(ValueKind::Array))
                            .union(KindSet::from_kind(ValueKind::Arguments))
                            .union(KindSet::from_kind(ValueKind::Function));
                        if !target.possible_kinds.is_subset_of(object_like) {
                            info = self.merge_optional_value_info(
                                info,
                                Some(self.standard_error_instance_info(
                                    StandardBuiltinId::TypeErrorConstructor,
                                )),
                            );
                        }
                    }
                }
                info
            }
            ExprIr::DeleteIdentifier { .. } => None,
            ExprIr::PropertyRead { target, key } | ExprIr::DeleteProperty { target, key, .. } => {
                self.merge_optional_value_info(
                    self.infer_expr_throw_info(target),
                    self.infer_property_key_throw_info(key),
                )
            }
            ExprIr::OptionalPropertyChain { target, chain } => {
                let mut info = self.infer_expr_throw_info(target);
                for operation in chain {
                    match operation {
                        OptionalChainOperationIr::Property { key, .. } => {
                            info = self.merge_optional_value_info(
                                info,
                                self.infer_property_key_throw_info(key),
                            );
                        }
                        OptionalChainOperationIr::PrivateProperty { .. } => {
                            info = self.merge_optional_value_info(
                                info,
                                Some(ValueInfo {
                                    kind: ValueKind::Dynamic,
                                    possible_kinds: KindSet::all_runtime_tags(),
                                    heap_shape: None,
                                    function_targets: FunctionTargetKnowledge::unknown(),
                                }),
                            );
                        }
                        OptionalChainOperationIr::Call { args, .. } => {
                            info = self.merge_optional_value_info(
                                info,
                                Some(ValueInfo {
                                    kind: ValueKind::Dynamic,
                                    possible_kinds: KindSet::all_runtime_tags(),
                                    heap_shape: None,
                                    function_targets: FunctionTargetKnowledge::unknown(),
                                }),
                            );
                            for arg in args {
                                info = self.merge_optional_value_info(
                                    info,
                                    self.infer_expr_throw_info(arg),
                                );
                            }
                        }
                    }
                }
                info
            }
            ExprIr::DeleteOptionalPropertyChain(deletion) => {
                let target = deletion.target();
                let chain = deletion.prefix();
                let mut info = self.infer_expr_throw_info(target);
                for operation in chain {
                    match operation {
                        OptionalChainOperationIr::Property { key, .. } => {
                            info = self.merge_optional_value_info(
                                info,
                                self.infer_property_key_throw_info(key),
                            );
                        }
                        OptionalChainOperationIr::PrivateProperty { .. } => {
                            info = self.merge_optional_value_info(
                                info,
                                Some(ValueInfo {
                                    kind: ValueKind::Dynamic,
                                    possible_kinds: KindSet::all_runtime_tags(),
                                    heap_shape: None,
                                    function_targets: FunctionTargetKnowledge::unknown(),
                                }),
                            );
                        }
                        OptionalChainOperationIr::Call { args, .. } => {
                            info = self.merge_optional_value_info(
                                info,
                                Some(ValueInfo {
                                    kind: ValueKind::Dynamic,
                                    possible_kinds: KindSet::all_runtime_tags(),
                                    heap_shape: None,
                                    function_targets: FunctionTargetKnowledge::unknown(),
                                }),
                            );
                            for arg in args {
                                info = self.merge_optional_value_info(
                                    info,
                                    self.infer_expr_throw_info(arg),
                                );
                            }
                        }
                    }
                }
                self.merge_optional_value_info(
                    info,
                    self.infer_property_key_throw_info(deletion.key()),
                )
            }
            ExprIr::PropertyWrite {
                target, key, value, ..
            } => {
                let mut info = self.infer_expr_throw_info(target);
                info =
                    self.merge_optional_value_info(info, self.infer_property_key_throw_info(key));
                info = self.merge_optional_value_info(info, self.infer_expr_throw_info(value));
                info
            }
            ExprIr::OrdinaryPropertyAssignment(assignment) => {
                let mut info = self.infer_expr_throw_info(assignment.base_and_receiver());
                info = self.merge_optional_value_info(
                    info,
                    self.infer_property_key_throw_info(assignment.referenced_name()),
                );
                info = self.merge_optional_value_info(info, Some(unknown_runtime_value_info()));
                self.merge_optional_value_info(info, self.infer_expr_throw_info(assignment.rhs()))
            }
            ExprIr::OrdinaryPropertyLogicalAssignment(assignment) => {
                let mut info = self.infer_expr_throw_info(assignment.base_and_receiver());
                info = self.merge_optional_value_info(
                    info,
                    self.infer_property_key_throw_info(assignment.referenced_name()),
                );
                // ToPropertyKey, [[Get]], and the branch-local [[Set]] can
                // invoke source accessors or Proxy traps which throw any
                // language value, not merely a shaped strict-Set TypeError.
                info = self.merge_optional_value_info(info, Some(unknown_runtime_value_info()));
                self.merge_optional_value_info(info, self.infer_expr_throw_info(assignment.rhs()))
            }
            ExprIr::OrdinaryPropertyGetCapture(capture) => {
                let info = self.merge_optional_value_info(
                    self.infer_expr_throw_info(capture.base_and_receiver()),
                    self.infer_property_key_throw_info(capture.referenced_name()),
                );
                self.merge_optional_value_info(info, Some(unknown_runtime_value_info()))
            }
            ExprIr::CapturedOrdinaryPropertyWrite(write) => self.merge_optional_value_info(
                self.infer_expr_throw_info(write.rhs()),
                Some(unknown_runtime_value_info()),
            ),
            ExprIr::OrdinaryPropertyNumericUpdate(update) => {
                let mut info = self.infer_expr_throw_info(update.base_and_receiver());
                info = self.merge_optional_value_info(
                    info,
                    self.infer_property_key_throw_info(update.referenced_name()),
                );
                self.merge_optional_value_info(info, Some(unknown_runtime_value_info()))
            }
            ExprIr::OrdinaryPropertyEagerCompoundAssignment(assignment) => {
                let mut info = self.infer_expr_throw_info(assignment.base_and_receiver());
                info = self.merge_optional_value_info(
                    info,
                    self.infer_property_key_throw_info(assignment.referenced_name()),
                );
                // Ordinary [[Get]], conversion, and [[Set]] hooks can throw an
                // arbitrary ECMAScript value. This unknown contribution is
                // shared by every retained ordinary mutation carrier; strict
                // failed-Set TypeErrors are only one possible throw source.
                info = self.merge_optional_value_info(info, Some(unknown_runtime_value_info()));
                self.merge_optional_value_info(
                    info,
                    self.infer_expr_throw_info(assignment.result()),
                )
            }
            ExprIr::BinaryNumber { lhs, rhs, .. }
            | ExprIr::CoerciveAdd { lhs, rhs }
            | ExprIr::CoerciveBinaryNumber { lhs, rhs, .. }
            | ExprIr::BitwiseNumeric { lhs, rhs, .. }
            | ExprIr::StringConcat { lhs, rhs }
            | ExprIr::CompareNumber { lhs, rhs, .. }
            | ExprIr::CompareValue { lhs, rhs, .. }
            | ExprIr::StrictEquality { lhs, rhs, .. }
            | ExprIr::LooseEquality { lhs, rhs, .. }
            | ExprIr::AssertSameValue {
                actual: lhs,
                expected: rhs,
                ..
            }
            | ExprIr::LogicalShortCircuit { lhs, rhs, .. }
            | ExprIr::Comma { lhs, rhs }
            | ExprIr::InstanceOf { lhs, rhs } => self.merge_optional_value_info(
                self.infer_expr_throw_info(lhs),
                self.infer_expr_throw_info(rhs),
            ),
            ExprIr::MaterializeBinding { value, body, .. } => self.merge_optional_value_info(
                self.infer_expr_throw_info(value),
                self.infer_expr_throw_info(body),
            ),
            ExprIr::ArrayDestructure { value, pattern, .. } => {
                let mut info = self.infer_expr_throw_info(value);
                pattern.visit_expressions(&mut |expr| {
                    info = self
                        .merge_optional_value_info(info.clone(), self.infer_expr_throw_info(expr));
                });
                info
            }
            ExprIr::ObjectDestructure { value, pattern } => {
                let mut info = self.infer_expr_throw_info(value);
                pattern.visit_expressions(&mut |expr| {
                    info = self
                        .merge_optional_value_info(info.clone(), self.infer_expr_throw_info(expr));
                });
                info
            }
            ExprIr::ObjectDestructuringOperation(_) => Some(unknown_runtime_value_info()),
            ExprIr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                let mut info = self.infer_expr_throw_info(condition);
                info = self.merge_optional_value_info(info, self.infer_expr_throw_info(then_expr));
                self.merge_optional_value_info(info, self.infer_expr_throw_info(else_expr))
            }
            ExprIr::CallNamed { args, .. } | ExprIr::SuperConstruct { args } => {
                let mut info = None;
                for arg in args {
                    info = self.merge_optional_value_info(info, self.infer_expr_throw_info(arg));
                }
                info
            }
            ExprIr::SuperNewTarget | ExprIr::SuperConstructor => Some(unknown_runtime_value_info()),
            ExprIr::PreparedSuperConstruct(prepared) => {
                let mut info = Some(unknown_runtime_value_info());
                for operand in prepared.operands() {
                    info =
                        self.merge_optional_value_info(info, self.infer_expr_throw_info(operand));
                }
                info
            }
            ExprIr::CallIndirect {
                callee,
                this_arg,
                args,
                ..
            } => {
                let mut info = self.infer_expr_throw_info(callee);
                if matches!(
                    self.resolve_single_function_target(callee)
                        .and_then(|function_id| StandardBuiltinId::from_function_id(&function_id)),
                    Some(
                        StandardBuiltinId::ArrayBufferPrototypeResize
                            | StandardBuiltinId::ArrayBufferPrototypeTransfer
                            | StandardBuiltinId::ArrayBufferPrototypeTransferToFixedLength
                            | StandardBuiltinId::ArrayBufferPrototypeTransferToImmutable
                    )
                ) {
                    info =
                        self.merge_optional_value_info(
                            info,
                            Some(self.standard_error_instance_info(
                                StandardBuiltinId::TypeErrorConstructor,
                            )),
                        );
                }
                if let Some(this_arg) = this_arg {
                    info =
                        self.merge_optional_value_info(info, self.infer_expr_throw_info(this_arg));
                }
                for arg in args {
                    info = self.merge_optional_value_info(info, self.infer_expr_throw_info(arg));
                }
                info
            }
            // [[Construct]] can throw any value via its body, prototype lookup or Proxy construct trap,
            // independently of operands; later throws must not narrow away these catch values.
            ExprIr::Construct { .. } => Some(unknown_runtime_value_info()),
            ExprIr::CallMethod {
                receiver,
                key,
                args,
            } => {
                let mut info = self.infer_expr_throw_info(receiver);
                info =
                    self.merge_optional_value_info(info, self.infer_property_key_throw_info(key));
                for arg in args {
                    info = self.merge_optional_value_info(info, self.infer_expr_throw_info(arg));
                }
                info
            }
            ExprIr::ClassDefinition(class) => {
                let mut info = class
                    .heritage
                    .as_deref()
                    .and_then(|heritage| self.infer_expr_throw_info(heritage));
                for definition in &class.element_plan.definitions {
                    let key = match definition {
                        ClassElementDefinitionIr::PublicMethod(method) => Some(&method.key),
                        ClassElementDefinitionIr::AutoAccessor(accessor) => {
                            accessor.computed_key.as_ref()
                        }
                        ClassElementDefinitionIr::PrivateMethod(_)
                        | ClassElementDefinitionIr::ComputedFieldKey { .. } => None,
                    };
                    let Some(key) = key else { continue };
                    info = self
                        .merge_optional_value_info(info, self.infer_property_key_throw_info(key));
                }
                info
            }
            // PrivateGet/PrivateSet perform brand checks and may invoke an
            // accessor that throws any value, independently of their operands.
            ExprIr::PrivateRead { .. } | ExprIr::PrivateWrite { .. } => {
                Some(unknown_runtime_value_info())
            }
            ExprIr::PrivateIn { rhs, .. } => {
                let mut info = self.infer_expr_throw_info(rhs);
                if !matches!(rhs.expr, ExprIr::RuntimeThrow { .. })
                    && !rhs
                        .possible_kinds
                        .is_subset_of(Self::object_like_kind_set())
                {
                    info =
                        self.merge_optional_value_info(
                            info,
                            Some(self.standard_error_instance_info(
                                StandardBuiltinId::TypeErrorConstructor,
                            )),
                        );
                }
                info
            }
            ExprIr::In { lhs, rhs } => {
                let mut info = self.merge_optional_value_info(
                    self.infer_expr_throw_info(lhs),
                    self.infer_expr_throw_info(rhs),
                );
                let rhs_object_like = KindSet::from_kind(ValueKind::Object)
                    .union(KindSet::from_kind(ValueKind::Array))
                    .union(KindSet::from_kind(ValueKind::Arguments))
                    .union(KindSet::from_kind(ValueKind::Function));
                if !rhs.possible_kinds.is_subset_of(rhs_object_like) {
                    info =
                        self.merge_optional_value_info(
                            info,
                            Some(self.standard_error_instance_info(
                                StandardBuiltinId::TypeErrorConstructor,
                            )),
                        );
                }
                info
            }
        }
    }

    fn merge_object_property_throw_info(
        &self,
        mut info: Option<ValueInfo>,
        property: &ObjectPropertyIr,
    ) -> Option<ValueInfo> {
        match property {
            ObjectPropertyIr::PrototypeSetter { value }
            | ObjectPropertyIr::Spread { source: value }
            | ObjectPropertyIr::Data { value, .. }
            | ObjectPropertyIr::NonEnumerableData { value, .. } => {
                info = self.merge_optional_value_info(info, self.infer_expr_throw_info(value));
            }
            ObjectPropertyIr::ComputedData { key, value, .. } => {
                info = self.merge_optional_value_info(info, self.infer_expr_throw_info(key));
                info = self.merge_optional_value_info(info, self.infer_expr_throw_info(value));
            }
            ObjectPropertyIr::ComputedMethod { key, .. }
            | ObjectPropertyIr::ComputedGetter { key, .. }
            | ObjectPropertyIr::ComputedSetter { key, .. } => {
                info = self.merge_optional_value_info(info, self.infer_expr_throw_info(key));
            }
            ObjectPropertyIr::Method { .. }
            | ObjectPropertyIr::Getter { .. }
            | ObjectPropertyIr::Setter { .. } => {}
        }
        info
    }

    fn infer_property_key_throw_info(&self, key: &PropertyKeyIr) -> Option<ValueInfo> {
        match key {
            PropertyKeyIr::StaticString(_)
            | PropertyKeyIr::ArrayIndex(_)
            | PropertyKeyIr::ArrayLength => None,
            PropertyKeyIr::StringExpr(expr) => self.infer_expr_throw_info(expr),
        }
    }
}
