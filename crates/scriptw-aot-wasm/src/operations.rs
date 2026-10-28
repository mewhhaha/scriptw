#![deny(unused_must_use)]

use super::*;
use crate::gc_types::{
    BigIntLimbArray, BigIntValue, BigIntValueSchema, BoundFunction, CodeUnitArray,
    CompletionLocals, FunctionObject, FunctionObjectSchema, GcLocal, I32Local, I64Local,
    ProxyCallCapability, ProxyObject, ProxyObjectSchema, RuntimeSchema, StringValue,
    StringValueSchema, SymbolValue, ValueLocals,
};
use crate::runtime_helpers::RuntimeHelperPropertyKeyParameter;
use scriptw_ir::NativeErrorKind;
use scriptw_ir::StaticRegExpCompilation;

mod coercive_add;
mod has_instance;
mod intrinsic_installation_key;
pub(crate) use intrinsic_installation_key::IntrinsicInstallationKey;
mod number_remainder;
mod number_to_string;
mod property_key_hash;
mod relational_comparison;
mod scalar_number;
mod string_trim;
pub(crate) use string_trim::EcmaTrimMode;
mod to_object;

/// A rooted String or Symbol already accepted as an ECMAScript property key.
/// Computed raw values must pass through ToPropertyKey before this owner exists.
#[must_use = "a completed property key must be consumed or cleared"]
pub(crate) struct PropertyKeyLocals {
    value: ValueLocals,
}

impl PropertyKeyLocals {
    pub(crate) fn value(&self) -> &ValueLocals {
        &self.value
    }

    pub(crate) fn from_string(
        schema: &RuntimeSchema,
        string: &GcLocal<StringValue>,
        function: &mut Function,
    ) -> Self {
        let value = schema.reserve_value_local(function);
        value.set_reference(string, schema, function);
        Self { value }
    }

    pub(crate) fn from_symbol(
        schema: &RuntimeSchema,
        symbol: &GcLocal<SymbolValue>,
        function: &mut Function,
    ) -> Self {
        let value = schema.reserve_value_local(function);
        value.set_reference(symbol, schema, function);
        Self { value }
    }

    pub(crate) fn from_helper_parameter(parameter: RuntimeHelperPropertyKeyParameter) -> Self {
        Self {
            value: parameter.into_value(),
        }
    }

    fn from_converted_value(value: ValueLocals) -> Self {
        Self { value }
    }

    pub(crate) fn clear(self, function: &mut Function) {
        self.value.clear(function);
    }
}

/// Keys projected only from an admitted incremental destructuring operation.
/// Its remaining operands stay borrowed from that same opaque operation.
#[must_use = "prepared destructuring keys must reach their operation and be cleared"]
pub(crate) enum ObjectDestructuringOperationKeys<'operation> {
    GetV {
        raw_receiver: &'operation TypedExpr,
        boxed: &'operation TypedExpr,
        key: PropertyKeyLocals,
    },
    Rest {
        boxed: &'operation TypedExpr,
        excluded: Vec<PropertyKeyLocals>,
    },
    PutTarget {
        target: &'operation DestructuringTargetIr,
        value: &'operation TypedExpr,
    },
}

/// The three algorithms differ only in Number equality: NaN and signed zero
/// decisions are mandatory at the shared comparator.
enum NumberEquality {
    Strict,
    SameValue,
    SameValueZero,
}

/// Whether a Number primitive is admitted by a value-to-BigInt conversion.
///
/// `ToBigInt` rejects Number, while the `%BigInt%` function applies the
/// distinct `NumberToBigInt` operation. Keeping that choice in a closed domain
/// makes each caller name its specification policy and makes a new policy an
/// exhaustive-match compile error at the sole Number projection below.
pub(crate) enum BigIntNumberPolicy {
    RejectNumber,
    NumberToBigInt,
}

/// The complete abrupt-routing decision for one tagged `ToPrimitive` use.
///
/// The route is a required argument of the sole tagged emitter. A new caller
/// therefore cannot receive the primitive locals without explicitly choosing
/// what happens to a throw, and a new route must update the exhaustive match
/// in `finish_to_primitive_operation`.
pub(crate) enum ToPrimitiveAbruptRoute {
    /// Route the thrown value in the primitive output locals to the active
    /// in-function handler, or return it when there is no handler.
    ActiveHandler,
    /// Return the current function's completion tuple on throw.
    ReturnCurrentFunction,
}

/// Where the Symbol throw admitted by primitive `ToString` must go.
///
/// The route is required by the sole primitive-string emitter. A new consumer
/// cannot accidentally inherit a function return when it sits beneath an
/// in-function handler, and a new routing discipline must update the
/// exhaustive match in the whole-completion routing emitter.
pub(crate) enum PrimitiveToStringAbruptRoute {
    /// Route the freshly-created TypeError to the active handler, or return it
    /// when no handler exists.
    ActiveHandler,
    /// Return the current function's completion tuple on throw.
    ReturnCurrentFunction,
}

/// A primitive whose ToPrimitive errors belong to the current function Realm.
///
/// The private producer owns abrupt routing. Consumers either preserve the
/// primitive tag or perform primitive ToString with the same Realm policy;
/// both consume the token and release its locals.
#[must_use = "a current-function-realm primitive must be consumed by a tagged or ToString projection"]
pub(crate) struct CurrentFunctionRealmPrimitiveLocals {
    value: ValueLocals,
}

fn validate_spec_operation_operands(
    operation: SpecOperationIr,
    operand_count: usize,
) -> Result<(), EmitError> {
    if operation.accepts_operand_count(operand_count) {
        return Ok(());
    }

    Err(EmitError::unsupported(format!(
        "unsupported in scriptw wasm-aot first slice: {} expects {}, got {} operands",
        operation.name(),
        operation.operand_domain_description(),
        operand_count,
    )))
}

/// The complete runtime result domain of `ToNumeric` for unary bitwise
/// complement.
///
/// The emitted tag chooses one of these branches, while the exhaustive Rust
/// match below makes adding another numeric representation a compiler error
/// until its complement semantics are defined.
enum UnaryNumericKind {
    Number,
    BigInt,
}

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn compile_object_destructuring_operation_keys<'operation>(
        &mut self,
        operation: &'operation scriptw_ir::ObjectDestructuringOperationIr,
        function: &mut Function,
    ) -> Result<ObjectDestructuringOperationKeys<'operation>, EmitError> {
        use scriptw_ir::ObjectDestructuringOperationView;

        let schema = self.runtime_schema();
        match operation.use_view() {
            ObjectDestructuringOperationView::GetV {
                raw_receiver,
                boxed,
                key,
            } => {
                let retained = schema.reserve_value_local(function);
                self.compile_expr_to_value(key, &retained, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                Ok(ObjectDestructuringOperationKeys::GetV {
                    raw_receiver,
                    boxed,
                    key: PropertyKeyLocals::from_converted_value(retained),
                })
            }
            ObjectDestructuringOperationView::Rest { boxed, excluded } => {
                let mut keys = Vec::with_capacity(excluded.len());
                for key in excluded {
                    let retained = schema.reserve_value_local(function);
                    self.compile_expr_to_value(key, &retained, function)?;
                    self.emit_propagate_current_throw_if_needed(function);
                    keys.push(PropertyKeyLocals::from_converted_value(retained));
                }
                Ok(ObjectDestructuringOperationKeys::Rest {
                    boxed,
                    excluded: keys,
                })
            }
            ObjectDestructuringOperationView::PutTarget { target, value } => {
                Ok(ObjectDestructuringOperationKeys::PutTarget { target, value })
            }
        }
    }

    fn finish_to_primitive_operation(
        &mut self,
        route: ToPrimitiveAbruptRoute,
        result: &crate::gc_types::CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(result, function);
        match route {
            ToPrimitiveAbruptRoute::ActiveHandler => self.emit_propagate_current_throw(function),
            ToPrimitiveAbruptRoute::ReturnCurrentFunction => {
                self.emit_return_current_completion(function)
            }
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_construct(
        &mut self,
        callee: &TypedExpr,
        args: &[TypedExpr],
        static_regexp_compilation: Option<&StaticRegExpCompilation>,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let target = schema.reserve_value_local(function);
        let result = schema.reserve_completion(function);
        self.compile_expr_to_value(callee, &target, function)?;
        let arguments = self.emit_call_args_vector(args, function)?;
        // A retained compilation plan applies to the actual acquired native
        // function only. All operands run before either validation or Call.
        let native_regexp = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        native_regexp.store(function);
        if static_regexp_compilation.is_some() {
            target.reference().load(function);
            function.instruction(&Instruction::RefTestNonNull(
                schema
                    .reference_type::<FunctionObject>(crate::gc_types::GcNullability::NonNullable)
                    .heap_type,
            ));
            self.open_frame(ControlFrameKind::If, function);
            let callable = schema.reserve_gc_local(function).initialize(
                target.cast_reference::<FunctionObject>(schema, function),
                function,
            );
            let code = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<FunctionObject>()
                    .field(FunctionObjectSchema::CODE)
                    .read(&callable, schema, function)
                    .reference(),
                function,
            );
            let kind = schema.reserve_i32_local(function);
            let builtin = schema.reserve_i32_local(function);
            schema
                .struct_type::<crate::gc_types::ExecutableCode>()
                .field(crate::gc_types::ExecutableCodeSchema::KIND)
                .read(&code, schema, function)
                .store(kind, function);
            schema
                .struct_type::<crate::gc_types::ExecutableCode>()
                .field(crate::gc_types::ExecutableCodeSchema::STANDARD_BUILTIN)
                .read(&code, schema, function)
                .store(builtin, function);
            kind.load(function);
            function.instruction(&Instruction::I32Const(
                crate::gc_types::GcI32Constant::encode(
                    crate::gc_types::ExecutableCodeKind::StandardBuiltin,
                ),
            ));
            function.instruction(&Instruction::I32Eq);
            builtin.load(function);
            function.instruction(&Instruction::I32Const(
                crate::gc_types::GcI32Constant::encode(Some(StandardBuiltinId::RegExpConstructor)),
            ));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32And);
            native_regexp.store(function);
            schema.release_i32_local(builtin, function);
            schema.release_i32_local(kind, function);
            code.clear(function);
            callable.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        // InvalidSyntax is consumed by the actual native RegExp entry/cache
        // after its source/flags coercions, in the acquired callee's Realm.
        self.emit_function_or_proxy_construct_with_argv(
            &target, &target, &arguments, &result, function,
        )?;
        if let Some(StaticRegExpCompilation::Program(program)) = static_regexp_compilation {
            result.kind().load(function);
            function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
            function.instruction(&Instruction::I32Eq);
            native_regexp.load(function);
            function.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, function);
            let regexp = schema.reserve_gc_local(function).initialize(
                result
                    .value()
                    .cast_reference::<crate::gc_types::RegExpObject>(schema, function),
                function,
            );
            self.emit_regexp_program_slots(&regexp, program, function)?;
            regexp.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.completion().copy_from(&result, function);
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        output.copy_from(result.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(native_regexp, function);
        arguments.clear(function);
        result.clear(function);
        target.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }

    pub(crate) fn compile_truthy_i32(
        &mut self,
        expr: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if let ExprIr::CompareNumber { op, lhs, rhs } = &expr.expr {
            return self.compile_number_comparison_i32(*op, lhs, rhs, function);
        }
        if let ExprIr::CompareValue { op, lhs, rhs } = &expr.expr {
            // Ordinary source relations retain the general comparison IR,
            // even for primitive operands. Its existing consumer already
            // returns a canonical i32; no Boolean value is needed here.
            return self.compile_compare_value_i32(*op, lhs, rhs, function);
        }
        let input = self.runtime_schema().reserve_value_local(function);
        self.compile_expr_to_value(expr, &input, function)?;
        if expr.kind == ValueKind::Boolean {
            // The normal value is already Boolean. Retain expression effects
            // and abrupt completion, then consume its canonical scalar rather
            // than dispatching over runtime tags that cannot occur here.
            input.scalar().load(function);
            function.instruction(&Instruction::I64Eqz);
            function.instruction(&Instruction::I32Eqz);
        } else {
            self.compile_truthy_tagged_i32(&input, function)?;
        }
        input.clear(function);
        Ok(())
    }

    /// All GetValue operands are retained before any operation-specific
    /// coercion. Every branch publishes one whole completion.
    pub(crate) fn compile_spec_operation_to_locals(
        &mut self,
        operation: SpecOperationIr,
        operands: &[TypedExpr],
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        validate_spec_operation_operands(operation, operands.len())?;
        let schema = self.runtime_schema();
        let mut inputs = Vec::with_capacity(operands.len());
        for operand in operands {
            let value = schema.reserve_value_local(function);
            self.compile_expr_to_value(operand, &value, function)?;
            inputs.push(value);
        }
        let pending = schema.reserve_completion(function);
        let boolean = schema.reserve_i32_local(function);
        let scalar = schema.reserve_i64_local(function);
        pending.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        match operation {
            SpecOperationIr::IsCallable
            | SpecOperationIr::IsConstructor
            | SpecOperationIr::IsPropertyKey
            | SpecOperationIr::ToBoolean => {
                match operation {
                    SpecOperationIr::IsCallable => {
                        self.emit_is_callable_i32(&inputs[0], function)?
                    }
                    SpecOperationIr::IsConstructor => {
                        self.emit_is_constructor_i32(&inputs[0], function)
                    }
                    SpecOperationIr::IsPropertyKey => {
                        self.emit_is_property_key_i32(inputs[0].tag(), function)
                    }
                    SpecOperationIr::ToBoolean => {
                        self.compile_truthy_tagged_i32(&inputs[0], function)?
                    }
                    _ => unreachable!("closed Boolean query group"),
                }
                boolean.store(function);
                pending.value().set_boolean(boolean, function);
                pending.set_normal(pending.value(), function);
            }
            SpecOperationIr::ToPrimitive(hint) => {
                self.emit_tagged_to_primitive_locals_pending(hint, &inputs[0], &pending, function)?;
            }
            SpecOperationIr::ToNumeric => {
                self.emit_value_to_numeric_locals(&inputs[0], &pending, function)?
            }
            SpecOperationIr::ToNumber => {
                self.emit_value_to_number_payload(&inputs[0], &pending, function)?
            }
            SpecOperationIr::ToBigInt => self.emit_value_to_bigint_locals(
                &inputs[0],
                BigIntNumberPolicy::RejectNumber,
                &pending,
                function,
            )?,
            SpecOperationIr::ToString => {
                self.emit_value_to_string_payload(&inputs[0], &pending, function)?
            }
            SpecOperationIr::ToObject => {
                self.emit_value_to_object_locals(&inputs[0], &pending, function)?
            }
            SpecOperationIr::ToPropertyKey => {
                self.emit_value_to_property_key_completion(&inputs[0], &pending, function)?
            }
            SpecOperationIr::ToIntegerOrInfinity => {
                self.emit_value_to_number_payload(&inputs[0], &pending, function)?;
                self.emit_spec_operation_abrupt_exit(&pending, exit, function);
                self.emit_to_integer_or_infinity_number_payload_from_number_payload(
                    pending.value().scalar(),
                    scalar,
                    function,
                );
                pending.value().set_number(scalar, function);
                pending.set_normal(pending.value(), function);
            }
            SpecOperationIr::ToLength | SpecOperationIr::ToIndex => {
                if operation == SpecOperationIr::ToLength {
                    self.emit_to_length_i64_from_value_locals(
                        &inputs[0], scalar, &pending, function,
                    )?;
                } else {
                    self.emit_to_index_i64_from_value_locals(
                        &inputs[0],
                        scalar,
                        RuntimeErrorMessage::TOINDEX_OUT_OF_RANGE,
                        &pending,
                        function,
                    )?;
                }
                self.emit_spec_operation_abrupt_exit(&pending, exit, function);
                scalar.load(function);
                function.instruction(&Instruction::F64ConvertI64U);
                function.instruction(&Instruction::I64ReinterpretF64);
                scalar.store(function);
                pending.value().set_number(scalar, function);
                pending.set_normal(pending.value(), function);
            }
            SpecOperationIr::SameValue
            | SpecOperationIr::SameValueZero
            | SpecOperationIr::StrictEqualityComparison
            | SpecOperationIr::IsLooselyEqual => {
                match operation {
                    SpecOperationIr::SameValue => {
                        self.emit_tagged_payload_same_value_i32(&inputs[0], &inputs[1], function)?
                    }
                    SpecOperationIr::SameValueZero => self
                        .emit_tagged_payload_same_value_zero_i32(
                            &inputs[0], &inputs[1], function,
                        )?,
                    SpecOperationIr::StrictEqualityComparison => {
                        self.emit_tagged_payload_equality_i32(&inputs[0], &inputs[1], function)?
                    }
                    SpecOperationIr::IsLooselyEqual => {
                        self.emit_loose_tagged_equality_i32(&inputs[0], &inputs[1], function)?
                    }
                    _ => unreachable!("closed comparison group"),
                }
                boolean.store(function);
                pending.value().set_boolean(boolean, function);
                pending.set_normal(pending.value(), function);
            }
            SpecOperationIr::Call => {
                let arguments = self.emit_pre_evaluated_arg_vector(
                    &inputs[2..].iter().collect::<Vec<_>>(),
                    function,
                );
                self.emit_function_or_proxy_call_with_argv(
                    &inputs[0], &inputs[1], &arguments, &pending, function,
                )?;
                arguments.clear(function);
            }
            SpecOperationIr::Construct => {
                let arguments = self.emit_pre_evaluated_arg_vector(
                    &inputs[1..].iter().collect::<Vec<_>>(),
                    function,
                );
                self.emit_function_or_proxy_construct_with_argv(
                    &inputs[0], &inputs[0], &arguments, &pending, function,
                )?;
                arguments.clear(function);
            }
            SpecOperationIr::WithEnvironmentHasBinding => {
                if operands[1].kind != ValueKind::String {
                    return Err(EmitError::unsupported(
                        "WithEnvironmentHasBinding requires a String binding name",
                    ));
                }
                let name = schema.reserve_gc_local(function).initialize(
                    inputs[1].cast_reference::<StringValue>(schema, function),
                    function,
                );
                schema
                    .call_helper(
                        crate::runtime_helpers::WithEnvironmentHasBindingArguments::new(
                            &inputs[0],
                            &name,
                            self.current_environment(),
                            self.current_program(),
                        ),
                        self.runtime_helper_base()?,
                        function,
                    )
                    .store(&pending, function);
                name.clear(function);
            }
            SpecOperationIr::WithEnvironmentGetBindingValueSloppy
            | SpecOperationIr::WithEnvironmentGetBindingValueStrict => {
                if operands[1].kind != ValueKind::String {
                    return Err(EmitError::unsupported(
                        "With GetBindingValue requires a String binding name",
                    ));
                }
                let name = schema.reserve_gc_local(function).initialize(
                    inputs[1].cast_reference::<StringValue>(schema, function),
                    function,
                );
                let strictness = match operation {
                    SpecOperationIr::WithEnvironmentGetBindingValueSloppy => Strictness::Sloppy,
                    SpecOperationIr::WithEnvironmentGetBindingValueStrict => Strictness::Strict,
                    _ => unreachable!("closed With GetBindingValue group"),
                };
                self.emit_selected_with_get_binding_value(
                    &name, &inputs[0], strictness, &pending, function,
                )?;
                name.clear(function);
            }
            SpecOperationIr::Get
            | SpecOperationIr::GetV
            | SpecOperationIr::GetMethod
            | SpecOperationIr::HasProperty
            | SpecOperationIr::HasOwnProperty
            | SpecOperationIr::DeletePropertyOrThrow
            | SpecOperationIr::Set
            | SpecOperationIr::CreateDataPropertyOrThrow => {
                let lookup = schema.reserve_value_local(function);
                let key_value = schema.reserve_value_local(function);
                let key_result = schema.reserve_completion(function);
                let property_exit = self.open_frame(ControlFrameKind::Block, function);
                match operation {
                    SpecOperationIr::GetV | SpecOperationIr::GetMethod => {
                        self.emit_value_to_object_locals(&inputs[0], &pending, function)?;
                        self.emit_spec_operation_abrupt_exit(&pending, property_exit, function);
                        lookup.copy_from(pending.value(), function);
                    }
                    SpecOperationIr::Get
                    | SpecOperationIr::HasProperty
                    | SpecOperationIr::HasOwnProperty
                    | SpecOperationIr::DeletePropertyOrThrow
                    | SpecOperationIr::Set
                    | SpecOperationIr::CreateDataPropertyOrThrow => {
                        let message = match operation {
                            SpecOperationIr::Get => RuntimeErrorMessage::GET_TARGET_IS_NOT_AN_OBJECT,
                            SpecOperationIr::HasProperty | SpecOperationIr::HasOwnProperty => RuntimeErrorMessage::RIGHT_HAND_SIDE_OF_IN_IS_NOT_AN_OBJECT,
                            SpecOperationIr::DeletePropertyOrThrow => RuntimeErrorMessage::DELETEPROPERTYORTHROW_TARGET_IS_NOT_AN_OBJECT,
                            SpecOperationIr::Set => RuntimeErrorMessage::SET_TARGET_IS_NOT_AN_OBJECT,
                            SpecOperationIr::CreateDataPropertyOrThrow => RuntimeErrorMessage::CREATEDATAPROPERTYORTHROW_TARGET_IS_NOT_AN_OBJECT,
                            _ => unreachable!("closed Object target group"),
                        };
                        self.emit_is_heap_object_like_tag_i32(inputs[0].tag(), function);
                        function.instruction(&Instruction::I32Eqz);
                        self.open_frame(ControlFrameKind::If, function);
                        self.emit_throw_runtime_error(
                            NativeErrorKind::TypeError,
                            message,
                            &pending,
                            function,
                        )?;
                        self.emit_branch_to_target(property_exit, function);
                        self.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                        lookup.copy_from(&inputs[0], function);
                    }
                    _ => unreachable!("closed property group"),
                }
                self.emit_value_to_property_key_completion(&inputs[1], &key_result, function)?;
                key_result.kind().load(function);
                function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                pending.copy_from(&key_result, function);
                self.emit_branch_to_target(property_exit, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                key_value.copy_from(key_result.value(), function);
                let key = PropertyKeyLocals::from_converted_value(key_value);
                match operation {
                    SpecOperationIr::Get | SpecOperationIr::GetV | SpecOperationIr::GetMethod => {
                        // GetV boxes only the lookup target. Getter this is the
                        // original value, including a primitive receiver.
                        // Unknown static source reads lower to canonical GetV,
                        // so admit the same cache after the existing operand,
                        // ToObject and ToPropertyKey steps have completed.
                        let cached = if operation == SpecOperationIr::GetV {
                            if let ExprIr::String(name) = &operands[1].expr {
                                self.emit_planned_property_read(
                                    &operands[0],
                                    &PropertyKeyIr::StaticString(name.clone()),
                                    &lookup,
                                    &inputs[0],
                                    key.value(),
                                    &pending,
                                    function,
                                )?
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        if !cached {
                            self.emit_dynamic_property_read_with_key_locals(
                                &lookup, &inputs[0], &key, &pending, function,
                            )?;
                        }
                        self.emit_spec_operation_abrupt_exit(&pending, property_exit, function);
                        if operation == SpecOperationIr::GetMethod {
                            self.compile_nullish_tagged_i32(pending.value().tag(), function)?;
                            self.open_frame(ControlFrameKind::If, function);
                            pending.value().set_undefined(function);
                            pending.set_normal(pending.value(), function);
                            function.instruction(&Instruction::Else);
                            self.emit_is_callable_i32(pending.value(), function)?;
                            function.instruction(&Instruction::I32Eqz);
                            self.open_frame(ControlFrameKind::If, function);
                            self.emit_throw_runtime_error(
                                NativeErrorKind::TypeError,
                                RuntimeErrorMessage::GETMETHOD_TARGET_IS_NOT_CALLABLE,
                                &pending,
                                function,
                            )?;
                            self.pop_control(ControlFrameKind::If);
                            function.instruction(&Instruction::End);
                            self.pop_control(ControlFrameKind::If);
                            function.instruction(&Instruction::End);
                        }
                    }
                    SpecOperationIr::HasProperty => {
                        schema
                            .call_helper(
                                crate::runtime_helpers::ObjectHasPropertyArguments::new(
                                    &lookup,
                                    &key,
                                    self.current_environment(),
                                    self.current_program(),
                                ),
                                self.runtime_helper_base()?,
                                function,
                            )
                            .store(&pending, function);
                    }
                    SpecOperationIr::HasOwnProperty => {
                        let descriptor =
                            self.emit_direct_own_descriptor_fact(&lookup, &key, function)?;
                        descriptor.load(schema, function);
                        function.instruction(&Instruction::RefIsNull);
                        function.instruction(&Instruction::I32Eqz);
                        boolean.store(function);
                        pending.value().set_boolean(boolean, function);
                        pending.set_normal(pending.value(), function);
                        descriptor.clear(function);
                    }
                    SpecOperationIr::DeletePropertyOrThrow => {
                        self.emit_object_delete(&lookup, &key, &pending, function)?;
                        self.emit_spec_operation_abrupt_exit(&pending, property_exit, function);
                        self.compile_truthy_tagged_i32(pending.value(), function)?;
                        function.instruction(&Instruction::I32Eqz);
                        self.open_frame(ControlFrameKind::If, function);
                        self.emit_throw_runtime_error(
                            NativeErrorKind::TypeError,
                            RuntimeErrorMessage::CANNOT_DELETE_PROPERTY,
                            &pending,
                            function,
                        )?;
                        self.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                    }
                    SpecOperationIr::Set => {
                        schema
                            .call_helper(
                                crate::runtime_helpers::OrdinarySetArguments::new(
                                    &lookup,
                                    &inputs[0],
                                    &key,
                                    &inputs[2],
                                    self.current_environment(),
                                    self.current_program(),
                                ),
                                self.runtime_helper_base()?,
                                function,
                            )
                            .store(&pending, function);
                    }
                    SpecOperationIr::CreateDataPropertyOrThrow => {
                        self.emit_create_data_property_or_throw(
                            &lookup, &key, &inputs[2], &pending, function,
                        )?;
                        self.emit_spec_operation_abrupt_exit(&pending, property_exit, function);
                        pending.value().set_undefined(function);
                        pending.set_normal(pending.value(), function);
                    }
                    _ => unreachable!("closed property group"),
                }
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
                key_result.clear(function);
                key.clear(function);
                lookup.clear(function);
            }
            SpecOperationIr::CopyDataProperties => {
                self.emit_is_heap_object_like_tag_i32(inputs[0].tag(), function);
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_throw_runtime_error(
                    NativeErrorKind::TypeError,
                    RuntimeErrorMessage::CREATEDATAPROPERTYORTHROW_TARGET_IS_NOT_AN_OBJECT,
                    &pending,
                    function,
                )?;
                self.emit_branch_to_target(exit, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                self.emit_copy_data_properties_into(
                    &inputs[1],
                    &[],
                    &inputs[0],
                    &pending,
                    function,
                )?;
                self.emit_spec_operation_abrupt_exit(&pending, exit, function);
                pending.value().set_undefined(function);
                pending.set_normal(pending.value(), function);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&pending, function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        output.copy_from(pending.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i64_local(scalar, function);
        schema.release_i32_local(boolean, function);
        pending.clear(function);
        for input in inputs.into_iter().rev() {
            input.clear(function);
        }
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }

    fn emit_spec_operation_abrupt_exit(
        &mut self,
        pending: &CompletionLocals,
        exit: crate::emit::ControlTarget,
        function: &mut Function,
    ) {
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn emit_to_boolean_payload_from_expr(
        &mut self,
        expr: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_truthy_i32(expr, function)?;
        function.instruction(&Instruction::I64ExtendI32U);
        Ok(())
    }

    pub(crate) fn emit_to_boolean_payload_from_tagged_locals(
        &mut self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_truthy_tagged_i32(value, function)?;
        function.instruction(&Instruction::I64ExtendI32U);
        Ok(())
    }

    pub(crate) fn emit_is_htmldda_function_i32(
        &self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        // Bound functions and callable proxies cannot acquire the HTMLDDA
        // internal slot merely by being callable. Test the concrete record.
        value.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<FunctionObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        let callable_slot = schema.reserve_gc_local(function);
        let callable = callable_slot.initialize(
            value.cast_reference::<FunctionObject>(schema, function),
            function,
        );
        let is_htmldda = schema.reserve_i32_local(function);
        schema
            .struct_type::<FunctionObject>()
            .field(FunctionObjectSchema::IS_HTMLDDA)
            .read(&callable, schema, function)
            .store(is_htmldda, function);
        is_htmldda.load(function);
        schema.release_i32_local(is_htmldda, function);
        callable.clear(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_truthy_tagged_i32(
        &self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Boolean as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        value.scalar().load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::Else);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        let string_slot = schema.reserve_gc_local(function);
        let string = string_slot.initialize(
            value.cast_reference::<StringValue>(schema, function),
            function,
        );
        let units_slot = schema.reserve_gc_local(function);
        let units = units_slot.initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(&string, schema, function)
                .reference(),
            function,
        );
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Eqz);
        units.clear(function);
        string.clear(function);
        function.instruction(&Instruction::Else);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        value.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Eq);
        value.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        value.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Ne);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::Else);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::BigInt as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        let bigint_slot = schema.reserve_gc_local(function);
        let bigint = bigint_slot.initialize(
            value.cast_reference::<BigIntValue>(schema, function),
            function,
        );
        let limbs_slot = schema.reserve_gc_local(function);
        let limbs = limbs_slot.initialize(
            schema
                .struct_type::<BigIntValue>()
                .field(BigIntValueSchema::LIMBS)
                .read(&bigint, schema, function)
                .reference(),
            function,
        );
        // Canonical zero has no magnitude limbs; every nonzero value has a
        // nonzero final limb, independently of the sign.
        schema
            .array_type::<BigIntLimbArray>()
            .length(&limbs, schema, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Eqz);
        limbs.clear(function);
        bigint.clear(function);
        function.instruction(&Instruction::Else);
        self.compile_nullish_tagged_i32(value.tag(), function)?;
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::Else);
        self.emit_is_htmldda_function_i32(value, function)?;
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_nullish_tagged_i32(
        &self,
        tag: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        tag.load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        tag.load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        Ok(())
    }

    /// There used to be a `_with_extra_depth` twin of this, for call sites that
    /// sat some number of untracked wasm blocks deeper than
    /// `self.control_stack` reflected (e.g. already inside a manually-emitted
    /// `If`), so that the internal throw propagation's `Br` reached the correct
    /// active handler. Branch immediates come from the real label depth now, so
    /// there is nothing for such a caller to declare and one entry point does
    /// both jobs.
    pub(crate) fn compile_expr_to_primitive_locals(
        &mut self,
        expr: &TypedExpr,
        hint: ToPrimitiveHint,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let input = schema.reserve_value_local(function);
        let result = schema.reserve_completion(function);
        self.compile_expr_to_value(expr, &input, function)?;
        self.emit_tagged_to_primitive_locals(
            hint,
            &input,
            &result,
            ToPrimitiveAbruptRoute::ActiveHandler,
            function,
        )?;
        output.copy_from(result.value(), function);
        result.clear(function);
        input.clear(function);
        Ok(())
    }

    pub(crate) fn emit_tagged_to_primitive_locals(
        &mut self,
        hint: ToPrimitiveHint,
        input: &ValueLocals,
        result: &crate::gc_types::CompletionLocals,
        route: ToPrimitiveAbruptRoute,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_tagged_to_primitive_locals_pending(hint, input, result, function)?;
        self.finish_to_primitive_operation(route, result, function)
    }

    /// Every caller consumes the registered hint-specific conversion body and
    /// its complete result. The helper receives the original caller Environment.
    pub(crate) fn emit_tagged_to_primitive_locals_pending(
        &mut self,
        hint: ToPrimitiveHint,
        input: &ValueLocals,
        result: &crate::gc_types::CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let base = self.runtime_helper_base()?;
        match hint {
            ToPrimitiveHint::Default => schema
                .call_helper(
                    crate::runtime_helpers::ValueToPrimitiveDefaultArguments::new(
                        input,
                        self.current_environment(),
                        self.current_program(),
                    ),
                    base,
                    function,
                )
                .store(result, function),
            ToPrimitiveHint::Number => schema
                .call_helper(
                    crate::runtime_helpers::ValueToPrimitiveNumberArguments::new(
                        input,
                        self.current_environment(),
                        self.current_program(),
                    ),
                    base,
                    function,
                )
                .store(result, function),
            ToPrimitiveHint::String => schema
                .call_helper(
                    crate::runtime_helpers::ValueToPrimitiveStringArguments::new(
                        input,
                        self.current_environment(),
                        self.current_program(),
                    ),
                    base,
                    function,
                )
                .store(result, function),
        }
        Ok(())
    }

    /// Sole physical GetMethod/OrdinaryToPrimitive body, emitted only by the
    /// registered hint-specific helper compilers.
    pub(crate) fn emit_tagged_to_primitive_locals_pending_inner(
        &mut self,
        hint: ToPrimitiveHint,
        input: &ValueLocals,
        result: &crate::gc_types::CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let method = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        method.set_undefined(function);
        pending.initialize(function);
        result.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_is_primitive_tag_i32(input.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        result.set_normal(input, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        let symbol_slot = schema.reserve_gc_local(function);
        let symbol = symbol_slot.initialize(
            self.emit_well_known_symbol_reference(
                scriptw_ir::WellKnownSymbol::ToPrimitive,
                function,
            )?,
            function,
        );
        let key = PropertyKeyLocals::from_symbol(schema, &symbol, function);
        symbol.clear(function);
        self.emit_object_read_with_throw_routing(
            input,
            input,
            &key,
            &pending,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        key.clear(function);
        self.emit_conversion_abrupt_exit(&pending, result, exit, function);
        method.copy_from(pending.value(), function);
        self.compile_nullish_tagged_i32(method.tag(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_is_callable_i32(&method, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_CONVERT_OBJECT_TO_PRIMITIVE_VALUE,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let hint_slot = schema.reserve_gc_local(function);
        let hint_text = hint_slot.initialize(
            self.emit_interned_string_reference(
                match hint {
                    ToPrimitiveHint::Default => "default",
                    ToPrimitiveHint::Number => "number",
                    ToPrimitiveHint::String => "string",
                },
                function,
            )?,
            function,
        );
        let hint_value = schema.reserve_value_local(function);
        hint_value.set_reference(&hint_text, schema, function);
        let arguments = self.emit_pre_evaluated_arg_vector(&[&hint_value], function);
        self.emit_function_handle_call_with_argv_inner(
            &method,
            Some(input),
            &arguments,
            &pending,
            PropagateCallThrow::LeaveInCompletion,
            function,
        )?;
        arguments.clear(function);
        hint_value.clear(function);
        hint_text.clear(function);
        self.emit_conversion_abrupt_exit(&pending, result, exit, function);
        self.emit_is_primitive_tag_i32(pending.value().tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(&pending, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_CONVERT_OBJECT_TO_PRIMITIVE_VALUE,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.emit_ordinary_to_primitive_pending(hint, input, result, function)?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.clear(function);
        method.clear(function);
        Ok(())
    }

    /// OrdinaryToPrimitive deliberately skips the @@toPrimitive lookup. Its
    /// Date and generic ToPrimitive consumers share the same Get/Call order
    /// and retain the complete abrupt result.
    pub(crate) fn emit_ordinary_to_primitive_pending(
        &mut self,
        hint: ToPrimitiveHint,
        input: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let method = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        method.set_undefined(function);
        pending.initialize(function);
        result.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let names = match hint {
            ToPrimitiveHint::String => ["toString", "valueOf"],
            ToPrimitiveHint::Default | ToPrimitiveHint::Number => ["valueOf", "toString"],
        };
        for name in names {
            let name_slot = schema.reserve_gc_local(function);
            let name = name_slot.initialize(
                self.emit_interned_string_reference(name, function)?,
                function,
            );
            let key = PropertyKeyLocals::from_string(schema, &name, function);
            name.clear(function);
            self.emit_object_read_with_throw_routing(
                input,
                input,
                &key,
                &pending,
                AccessorThrowRouting::LeaveInCompletion,
                function,
            )?;
            key.clear(function);
            self.emit_conversion_abrupt_exit(&pending, result, exit, function);
            method.copy_from(pending.value(), function);
            self.emit_is_callable_i32(&method, function)?;
            self.open_frame(ControlFrameKind::If, function);
            let arguments = self.emit_pre_evaluated_arg_vector(&[], function);
            self.emit_function_handle_call_with_argv_inner(
                &method,
                Some(input),
                &arguments,
                &pending,
                PropagateCallThrow::LeaveInCompletion,
                function,
            )?;
            arguments.clear(function);
            self.emit_conversion_abrupt_exit(&pending, result, exit, function);
            self.emit_is_primitive_tag_i32(pending.value().tag(), function);
            self.open_frame(ControlFrameKind::If, function);
            result.copy_from(&pending, function);
            self.emit_branch_to_target(exit, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_CONVERT_OBJECT_TO_PRIMITIVE_VALUE,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.clear(function);
        method.clear(function);
        Ok(())
    }

    fn emit_conversion_abrupt_exit(
        &mut self,
        pending: &crate::gc_types::CompletionLocals,
        result: &crate::gc_types::CompletionLocals,
        exit: crate::emit::ControlTarget,
        function: &mut Function,
    ) {
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(pending, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn emit_tagged_to_primitive_locals_in_current_function_realm(
        &mut self,
        hint: ToPrimitiveHint,
        input: &ValueLocals,
        function: &mut Function,
    ) -> Result<CurrentFunctionRealmPrimitiveLocals, EmitError> {
        let schema = self.runtime_schema();
        let result = schema.reserve_completion(function);
        self.emit_tagged_to_primitive_locals(
            hint,
            input,
            &result,
            ToPrimitiveAbruptRoute::ReturnCurrentFunction,
            function,
        )?;
        let value = schema.reserve_value_local(function);
        value.copy_from(result.value(), function);
        result.clear(function);
        Ok(CurrentFunctionRealmPrimitiveLocals { value })
    }

    pub(crate) fn emit_current_function_realm_primitive_to_tagged_locals(
        &mut self,
        primitive: CurrentFunctionRealmPrimitiveLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) {
        output.copy_from(&primitive.value, function);
        primitive.value.clear(function);
    }

    pub(crate) fn emit_current_function_realm_primitive_to_string_local(
        &mut self,
        primitive: CurrentFunctionRealmPrimitiveLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let result = schema.reserve_completion(function);
        self.emit_value_to_string_payload(&primitive.value, &result, function)?;
        self.finish_to_primitive_operation(
            ToPrimitiveAbruptRoute::ReturnCurrentFunction,
            &result,
            function,
        )?;
        output.copy_from(result.value(), function);
        result.clear(function);
        primitive.value.clear(function);
        Ok(())
    }

    pub(crate) fn emit_is_primitive_tag_i32(&self, tag: I32Local, function: &mut Function) {
        tag.load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        for kind in [
            WasmRuntimeValueTag::Null,
            WasmRuntimeValueTag::Boolean,
            WasmRuntimeValueTag::Number,
            WasmRuntimeValueTag::BigInt,
            WasmRuntimeValueTag::Symbol,
            WasmRuntimeValueTag::String,
        ] {
            tag.load(function);
            function.instruction(&Instruction::I32Const(kind as i32));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32Or);
        }
    }

    pub(crate) fn emit_is_bigint_tag_i32(&self, tag: I32Local, function: &mut Function) {
        tag.load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::BigInt as i32));
        function.instruction(&Instruction::I32Eq);
    }

    pub(crate) fn emit_is_heap_object_like_tag_i32(&self, tag: I32Local, function: &mut Function) {
        tag.load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Object as i32));
        function.instruction(&Instruction::I32Eq);
        for kind in [
            WasmRuntimeValueTag::Array,
            WasmRuntimeValueTag::Function,
            WasmRuntimeValueTag::Arguments,
        ] {
            tag.load(function);
            function.instruction(&Instruction::I32Const(kind as i32));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32Or);
        }
    }

    fn emit_numeric_bigint_operation(
        &mut self,
        operation: BigIntHelperOp,
        left: &ValueLocals,
        right: &ValueLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let selector = schema.reserve_i32_local(function);
        let result = schema.reserve_completion(function);
        function.instruction(&Instruction::I32Const(operation.runtime_code() as i32));
        selector.store(function);
        schema
            .call_helper(
                crate::runtime_helpers::BigIntArithmeticArguments::new(left, right, selector),
                self.runtime_helper_base()?,
                function,
            )
            .store(&result, function);
        self.finish_to_primitive_operation(
            ToPrimitiveAbruptRoute::ActiveHandler,
            &result,
            function,
        )?;
        output.copy_from(result.value(), function);
        result.clear(function);
        schema.release_i32_local(selector, function);
        Ok(())
    }

    fn emit_numeric_pair_agreement(
        &mut self,
        left: &ValueLocals,
        right: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_is_bigint_tag_i32(left.tag(), function);
        self.emit_is_bigint_tag_i32(right.tag(), function);
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        let error = self.runtime_schema().reserve_completion(function);
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_MIX_BIGINT_AND_OTHER_TYPES,
            &error,
            function,
        )?;
        self.completion().copy_from(&error, function);
        error.clear(function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn compile_numeric_operand_pair(
        &mut self,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        left: &ValueLocals,
        right: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let pending = self.runtime_schema().reserve_completion(function);
        self.compile_expr_to_value(lhs, left, function)?;
        self.compile_expr_to_value(rhs, right, function)?;
        for value in [left, right] {
            self.emit_value_to_numeric_locals(value, &pending, function)?;
            self.finish_to_primitive_operation(
                ToPrimitiveAbruptRoute::ActiveHandler,
                &pending,
                function,
            )?;
            value.copy_from(pending.value(), function);
        }
        pending.clear(function);
        self.emit_numeric_pair_agreement(left, right, function)
    }

    pub(crate) fn compile_coercive_binary_number_to_locals(
        &mut self,
        op: ArithmeticBinaryOp,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if lhs.kind == ValueKind::Number && rhs.kind == ValueKind::Number {
            return self.compile_binary_number_to_value(op, lhs, rhs, output, function);
        }
        if matches!(op, ArithmeticBinaryOp::Add) {
            return self.compile_coercive_add_to_locals(lhs, rhs, output, function);
        }
        let schema = self.runtime_schema();
        let left = schema.reserve_value_local(function);
        let right = schema.reserve_value_local(function);
        self.compile_numeric_operand_pair(lhs, rhs, &left, &right, function)?;
        self.emit_is_bigint_tag_i32(left.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_numeric_bigint_operation(
            BigIntHelperOp::from_arithmetic(op),
            &left,
            &right,
            output,
            function,
        )?;
        function.instruction(&Instruction::Else);
        self.emit_number_arithmetic(op, left.scalar(), right.scalar(), output.scalar(), function)?;
        output.set_number(output.scalar(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        right.clear(function);
        left.clear(function);
        Ok(())
    }

    pub(crate) fn compile_binary_number_to_value(
        &mut self,
        op: ArithmeticBinaryOp,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_number_arithmetic_payload(op, lhs, rhs, output.scalar(), function)?;
        output.set_number(output.scalar(), function);
        Ok(())
    }

    fn emit_number_arithmetic(
        &mut self,
        op: ArithmeticBinaryOp,
        left: crate::gc_types::I64Local,
        right: crate::gc_types::I64Local,
        output: crate::gc_types::I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match op {
            ArithmeticBinaryOp::Mod => {
                self.emit_number_remainder_payload(left, right, output, function)
            }
            ArithmeticBinaryOp::Exp => {
                self.emit_number_pow_payload(left, right, output, function)?
            }
            ArithmeticBinaryOp::Add
            | ArithmeticBinaryOp::Sub
            | ArithmeticBinaryOp::Mul
            | ArithmeticBinaryOp::Div => {
                left.load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                right.load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                match op {
                    ArithmeticBinaryOp::Add => function.instruction(&Instruction::F64Add),
                    ArithmeticBinaryOp::Sub => function.instruction(&Instruction::F64Sub),
                    ArithmeticBinaryOp::Mul => function.instruction(&Instruction::F64Mul),
                    ArithmeticBinaryOp::Div => function.instruction(&Instruction::F64Div),
                    ArithmeticBinaryOp::Mod | ArithmeticBinaryOp::Exp => {
                        unreachable!("handled by exact numeric owner")
                    }
                };
                function.instruction(&Instruction::I64ReinterpretF64);
                output.store(function);
            }
        }
        Ok(())
    }

    pub(crate) fn compile_bitwise_numeric_to_locals(
        &mut self,
        op: BitwiseBinaryOp,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let left = schema.reserve_value_local(function);
        let right = schema.reserve_value_local(function);
        if lhs.kind == ValueKind::Number && rhs.kind == ValueKind::Number {
            self.compile_expr_to_value(lhs, &left, function)?;
            self.compile_expr_to_value(rhs, &right, function)?;
            self.emit_number_bitwise_to_locals(op, left.scalar(), right.scalar(), output, function);
            right.clear(function);
            left.clear(function);
            return Ok(());
        }
        self.compile_numeric_operand_pair(lhs, rhs, &left, &right, function)?;
        self.emit_is_bigint_tag_i32(left.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        if let Some(bigint_op) = op.bigint_op() {
            self.emit_numeric_bigint_operation(
                BigIntHelperOp::from_bitwise(bigint_op),
                &left,
                &right,
                output,
                function,
            )?;
        } else {
            let error = schema.reserve_completion(function);
            self.emit_throw_runtime_error(
                NativeErrorKind::TypeError,
                RuntimeErrorMessage::BIGINTS_DO_NOT_SUPPORT_UNSIGNED_RIGHT_SHIFT,
                &error,
                function,
            )?;
            self.completion().copy_from(&error, function);
            error.clear(function);
            self.emit_propagate_current_throw(function);
        }
        function.instruction(&Instruction::Else);
        self.emit_number_bitwise_to_locals(op, left.scalar(), right.scalar(), output, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        right.clear(function);
        left.clear(function);
        Ok(())
    }

    fn emit_number_bitwise_to_locals(
        &mut self,
        op: BitwiseBinaryOp,
        left: I64Local,
        right: I64Local,
        output: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let left_bits = schema.reserve_i64_local(function);
        let right_bits = schema.reserve_i64_local(function);
        self.emit_to_uint32_i64_from_number_payload(left, left_bits, function);
        self.emit_to_uint32_i64_from_number_payload(right, right_bits, function);
        left_bits.load(function);
        function.instruction(&Instruction::I32WrapI64);
        right_bits.load(function);
        function.instruction(&Instruction::I32WrapI64);
        match op {
            BitwiseBinaryOp::And => function.instruction(&Instruction::I32And),
            BitwiseBinaryOp::Or => function.instruction(&Instruction::I32Or),
            BitwiseBinaryOp::Xor => function.instruction(&Instruction::I32Xor),
            BitwiseBinaryOp::Shl => function.instruction(&Instruction::I32Shl),
            BitwiseBinaryOp::Shr => function.instruction(&Instruction::I32ShrS),
            BitwiseBinaryOp::UShr => function.instruction(&Instruction::I32ShrU),
        };
        function.instruction(&if matches!(op, BitwiseBinaryOp::UShr) {
            Instruction::F64ConvertI32U
        } else {
            Instruction::F64ConvertI32S
        });
        function.instruction(&Instruction::I64ReinterpretF64);
        output.scalar().store(function);
        output.set_number(output.scalar(), function);
        schema.release_i64_local(right_bits, function);
        schema.release_i64_local(left_bits, function);
    }

    pub(crate) fn compile_expr_to_object_locals(
        &mut self,
        expr: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let input = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        self.compile_expr_to_value(expr, &input, function)?;
        self.emit_value_to_object_locals(&input, &pending, function)?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        output.copy_from(pending.value(), function);
        function.instruction(&Instruction::Else);
        self.completion().copy_from(&pending, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        input.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }

    pub(crate) fn emit_to_integer_or_infinity_number_payload_from_number_payload(
        &self,
        number_payload_local: crate::gc_types::I64Local,
        out_payload_local: crate::gc_types::I64Local,
        function: &mut Function,
    ) {
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::I64ReinterpretF64);
        out_payload_local.store(function);
        function.instruction(&Instruction::Else);
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::I64ReinterpretF64);
        out_payload_local.store(function);
        function.instruction(&Instruction::Else);
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(f64::INFINITY)));
        function.instruction(&Instruction::F64Eq);
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(f64::NEG_INFINITY)));
        function.instruction(&Instruction::F64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        number_payload_local.load(function);
        out_payload_local.store(function);
        function.instruction(&Instruction::Else);
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Trunc);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Add);
        function.instruction(&Instruction::I64ReinterpretF64);
        out_payload_local.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn emit_value_to_numeric_locals(
        &mut self,
        input: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        if self.outline_value_to_numeric {
            schema
                .call_helper(
                    crate::runtime_helpers::ValueToNumericArguments::new(
                        input,
                        self.current_environment(),
                        self.current_program(),
                    ),
                    self.runtime_helper_base()?,
                    function,
                )
                .store(result, function);
            return Ok(());
        }
        let primitive = schema.reserve_completion(function);
        self.emit_tagged_to_primitive_locals_pending(
            ToPrimitiveHint::Number,
            input,
            &primitive,
            function,
        )?;
        result.copy_from(&primitive, function);
        primitive.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        primitive.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::BigInt as i32));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_primitive_to_number_completion(primitive.value(), result, false, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        primitive.clear(function);
        Ok(())
    }

    fn emit_bigint_unit_value(
        &self,
        negative: bool,
        output: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let sign = schema.reserve_i32_local(function);
        let limb = schema.reserve_i64_local(function);
        function.instruction(&Instruction::I32Const(1));
        length.store(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::I32Const(i32::from(negative)));
        sign.store(function);
        function.instruction(&Instruction::I64Const(1));
        limb.store(function);
        let construction = crate::gc_types::BigIntConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            length,
            function,
        );
        construction.write(index, limb, schema, function);
        let bigint = schema
            .reserve_gc_local(function)
            .initialize(construction.publish(sign, schema, function), function);
        output.set_reference(&bigint, schema, function);
        bigint.clear(function);
        schema.release_i64_local(limb, function);
        schema.release_i32_local(sign, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(length, function);
    }

    pub(crate) fn emit_numeric_update_to_locals(
        &mut self,
        op: NumericUpdateOp,
        value_kind: NumericUpdateValueKind,
        old: &ValueLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        // ToNumeric already completed. The closed IR kind is the authority for
        // static admission; only Dynamic needs to inspect the runtime tag.
        match value_kind {
            NumericUpdateValueKind::Number => {
                self.emit_number_update_from_numeric(op, old, output, function);
            }
            NumericUpdateValueKind::BigInt => {
                self.emit_bigint_update_from_numeric(op, old, output, function)?;
            }
            NumericUpdateValueKind::Dynamic => {
                self.emit_is_bigint_tag_i32(old.tag(), function);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_bigint_update_from_numeric(op, old, output, function)?;
                function.instruction(&Instruction::Else);
                self.emit_number_update_from_numeric(op, old, output, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        Ok(())
    }

    fn emit_bigint_update_from_numeric(
        &mut self,
        op: NumericUpdateOp,
        old: &ValueLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let unit = schema.reserve_value_local(function);
        self.emit_bigint_unit_value(false, &unit, function);
        self.emit_numeric_bigint_operation(
            match op {
                NumericUpdateOp::Increment => BigIntHelperOp::Add,
                NumericUpdateOp::Decrement => BigIntHelperOp::Sub,
            },
            old,
            &unit,
            output,
            function,
        )?;
        unit.clear(function);
        Ok(())
    }

    fn emit_number_update_from_numeric(
        &mut self,
        op: NumericUpdateOp,
        old: &ValueLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) {
        old.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
        match op {
            NumericUpdateOp::Increment => function.instruction(&Instruction::F64Add),
            NumericUpdateOp::Decrement => function.instruction(&Instruction::F64Sub),
        };
        function.instruction(&Instruction::I64ReinterpretF64);
        output.scalar().store(function);
        output.set_number(output.scalar(), function);
    }

    pub(crate) fn compile_unary_minus_numeric_to_locals(
        &mut self,
        expr: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        if expr.kind == ValueKind::Number {
            self.compile_number_payload(expr, output.scalar(), function)?;
            output.scalar().load(function);
            function.instruction(&Instruction::F64ReinterpretI64);
            function.instruction(&Instruction::F64Neg);
            function.instruction(&Instruction::I64ReinterpretF64);
            output.scalar().store(function);
            output.set_number(output.scalar(), function);
            return Ok(());
        }
        let value = schema.reserve_value_local(function);
        self.compile_expr_to_value(expr, &value, function)?;
        let pending = schema.reserve_completion(function);
        self.emit_value_to_numeric_locals(&value, &pending, function)?;
        self.finish_to_primitive_operation(
            ToPrimitiveAbruptRoute::ActiveHandler,
            &pending,
            function,
        )?;
        value.copy_from(pending.value(), function);
        self.emit_is_bigint_tag_i32(value.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_numeric_bigint_operation(
            BigIntHelperOp::Negate,
            &value,
            &value,
            output,
            function,
        )?;
        function.instruction(&Instruction::Else);
        value.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Neg);
        function.instruction(&Instruction::I64ReinterpretF64);
        output.scalar().store(function);
        output.set_number(output.scalar(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        value.clear(function);
        Ok(())
    }

    pub(crate) fn compile_unary_bitwise_numeric_to_locals(
        &mut self,
        op: UnaryBitwiseOp,
        expr: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        self.compile_expr_to_value(expr, &value, function)?;
        if expr.kind == ValueKind::Number {
            match op {
                UnaryBitwiseOp::Complement => {
                    self.emit_number_bitwise_complement(value.scalar(), output, function);
                }
            }
            value.clear(function);
            return Ok(());
        }
        let pending = schema.reserve_completion(function);
        self.emit_value_to_numeric_locals(&value, &pending, function)?;
        self.finish_to_primitive_operation(
            ToPrimitiveAbruptRoute::ActiveHandler,
            &pending,
            function,
        )?;
        value.copy_from(pending.value(), function);
        match op {
            UnaryBitwiseOp::Complement => {
                self.emit_is_bigint_tag_i32(value.tag(), function);
                self.open_frame(ControlFrameKind::If, function);
                let negative_one = schema.reserve_value_local(function);
                self.emit_bigint_unit_value(true, &negative_one, function);
                self.emit_numeric_bigint_operation(
                    BigIntHelperOp::BitXor,
                    &value,
                    &negative_one,
                    output,
                    function,
                )?;
                negative_one.clear(function);
                function.instruction(&Instruction::Else);
                self.emit_number_bitwise_complement(value.scalar(), output, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        pending.clear(function);
        value.clear(function);
        Ok(())
    }

    fn emit_number_bitwise_complement(
        &mut self,
        value: crate::gc_types::I64Local,
        output: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let bits = schema.reserve_i64_local(function);
        self.emit_to_uint32_i64_from_number_payload(value, bits, function);
        bits.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Const(-1));
        function.instruction(&Instruction::I32Xor);
        function.instruction(&Instruction::F64ConvertI32S);
        function.instruction(&Instruction::I64ReinterpretF64);
        output.scalar().store(function);
        output.set_number(output.scalar(), function);
        schema.release_i64_local(bits, function);
    }

    pub(crate) fn compile_expr_to_number_payload(
        &mut self,
        expr: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        self.compile_expr_to_value(expr, &value, function)?;
        self.emit_value_to_number_payload(&value, &pending, function)?;
        self.finish_to_primitive_operation(
            ToPrimitiveAbruptRoute::ActiveHandler,
            &pending,
            function,
        )?;
        pending.value().scalar().load(function);
        pending.clear(function);
        value.clear(function);
        Ok(())
    }

    pub(crate) fn emit_to_uint32_i64_from_number_payload(
        &mut self,
        number_payload_local: crate::gc_types::I64Local,
        out_local: crate::gc_types::I64Local,
        function: &mut Function,
    ) {
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Trunc);
        function.instruction(&Instruction::I64ReinterpretF64);
        out_local.store(function);

        out_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        out_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(4_294_967_296.0)));
        function.instruction(&Instruction::F64Div);
        function.instruction(&Instruction::F64Floor);
        function.instruction(&Instruction::F64Const(Ieee64::from(4_294_967_296.0)));
        function.instruction(&Instruction::F64Mul);
        function.instruction(&Instruction::F64Sub);
        function.instruction(&Instruction::I64TruncSatF64U);
        out_local.store(function);
    }

    /// Emits ECMA-262 ToUint16 steps 2-5 for a Number payload.
    ///
    /// ToUint16 is exactly the low half of ToUint32. Routing through the
    /// binary64 modulo authority above is important for magnitudes outside
    /// `i64`: truncating to an integer first would saturate and lose the low
    /// bits that String.fromCharCode and the integer conversion require.
    pub(crate) fn emit_to_uint16_i64_from_number_payload(
        &mut self,
        number_payload_local: crate::gc_types::I64Local,
        out_local: crate::gc_types::I64Local,
        function: &mut Function,
    ) {
        self.emit_to_uint32_i64_from_number_payload(number_payload_local, out_local, function);
        out_local.load(function);
        function.instruction(&Instruction::I64Const(0xffff));
        function.instruction(&Instruction::I64And);
        out_local.store(function);
    }

    pub(crate) fn emit_nan_payload(&self, function: &mut Function) {
        function.instruction(&Instruction::I64Const(f64::NAN.to_bits() as i64));
    }

    pub(crate) fn emit_number_pow_payload(
        &mut self,
        base: crate::gc_types::I64Local,
        exponent: crate::gc_types::I64Local,
        output: crate::gc_types::I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let function_index = self
            .functions
            .number_pow_import_function_index()
            .ok_or_else(|| EmitError::unsupported("missing planned binary64 pow import"))?;
        base.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        exponent.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::Call(function_index));
        function.instruction(&Instruction::I64ReinterpretF64);
        output.store(function);
        Ok(())
    }

    /// Apply `ToLength` with the ordinary callers' existing completion policy.
    /// The numeric output exists only on Normal; the native consumer owns any
    /// rejection, IteratorClose or active-handler continuation of the Throw.
    pub(crate) fn emit_to_length_i64_from_value_locals(
        &mut self,
        input: &ValueLocals,
        output: I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_value_to_number_payload(input, result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_to_length_i64_from_number_payload_local(
            result.value().scalar(),
            output,
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_to_length_i64_from_number_payload_local(
        &self,
        number: I64Local,
        output: I64Local,
        function: &mut Function,
    ) {
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Ne);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Le);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        output.store(function);
        function.instruction(&Instruction::Else);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(
            MAX_SAFE_INTEGER as f64,
        )));
        function.instruction(&Instruction::F64Gt);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(MAX_SAFE_INTEGER as i64));
        output.store(function);
        function.instruction(&Instruction::Else);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Trunc);
        function.instruction(&Instruction::I64TruncF64U);
        output.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn emit_to_index_i64_from_value_locals(
        &mut self,
        input: &ValueLocals,
        output: I64Local,
        error_message: RuntimeErrorMessage,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_value_to_number_payload(input, result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_to_index_from_number_payload(
            result.value().scalar(),
            output,
            error_message,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_to_index_from_number_payload(
        &mut self,
        number: I64Local,
        output: I64Local,
        error_message: RuntimeErrorMessage,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let integer = schema.reserve_i64_local(function);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            number, integer, function,
        );
        integer.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Lt);
        integer.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(
            MAX_SAFE_INTEGER as f64,
        )));
        function.instruction(&Instruction::F64Gt);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            NativeErrorKind::RangeError,
            error_message,
            result,
            function,
        )?;
        function.instruction(&Instruction::Else);
        integer.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::I64TruncF64U);
        output.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i64_local(integer, function);
        Ok(())
    }

    pub(crate) fn emit_value_to_number_payload(
        &mut self,
        input: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        if self.outline_value_to_number {
            schema
                .call_helper(
                    crate::runtime_helpers::ValueToNumberArguments::new(
                        input,
                        self.current_environment(),
                        self.current_program(),
                    ),
                    self.runtime_helper_base()?,
                    function,
                )
                .store(result, function);
            return Ok(());
        }
        self.emit_value_to_number_composite(input, result, false, function)
    }

    pub(crate) fn emit_value_to_number_payload_allow_bigint(
        &mut self,
        input: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_value_to_number_composite(input, result, true, function)
    }

    fn emit_value_to_number_composite(
        &mut self,
        input: &ValueLocals,
        result: &CompletionLocals,
        allow_bigint: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let primitive = schema.reserve_completion(function);
        self.emit_tagged_to_primitive_locals_pending(
            ToPrimitiveHint::Number,
            input,
            &primitive,
            function,
        )?;
        result.copy_from(&primitive, function);
        primitive.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_primitive_to_number_completion(
            primitive.value(),
            result,
            allow_bigint,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        primitive.clear(function);
        Ok(())
    }

    pub(crate) fn emit_primitive_to_number_payload(
        &mut self,
        input: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_primitive_to_number_completion(input, result, false, function)
    }

    fn emit_primitive_to_number_completion(
        &mut self,
        input: &ValueLocals,
        result: &CompletionLocals,
        allow_bigint: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        let bits = schema.reserve_i64_local(function);
        value.copy_from(input, function);
        result.initialize(function);
        self.emit_nan_payload(function);
        bits.store(function);
        for tag in [
            WasmRuntimeValueTag::Number,
            WasmRuntimeValueTag::Null,
            WasmRuntimeValueTag::Boolean,
            WasmRuntimeValueTag::String,
        ] {
            value.tag().load(function);
            function.instruction(&Instruction::I32Const(tag as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            match tag {
                WasmRuntimeValueTag::Number => {
                    value.scalar().load(function);
                    bits.store(function);
                }
                WasmRuntimeValueTag::Null => {
                    function.instruction(&Instruction::I64Const(0));
                    bits.store(function);
                }
                WasmRuntimeValueTag::Boolean => {
                    value.scalar().load(function);
                    function.instruction(&Instruction::F64ConvertI64U);
                    function.instruction(&Instruction::I64ReinterpretF64);
                    bits.store(function);
                }
                WasmRuntimeValueTag::String => {
                    let string = schema.reserve_gc_local(function).initialize(
                        value.cast_reference::<StringValue>(schema, function),
                        function,
                    );
                    self.emit_string_to_number_payload(&string, bits, function)?;
                    string.clear(function);
                }
                _ => unreachable!("the primitive numeric conversion loop is closed"),
            }
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::BigInt as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        if allow_bigint {
            let bigint = schema.reserve_gc_local(function).initialize(
                value.cast_reference::<BigIntValue>(schema, function),
                function,
            );
            let string = schema.reserve_gc_local(function).initialize(
                self.emit_bigint_value_to_string_payload(&bigint, function)?,
                function,
            );
            self.emit_string_to_number_payload(&string, bits, function)?;
            string.clear(function);
            bigint.clear(function);
        } else {
            self.emit_throw_runtime_error(
                NativeErrorKind::TypeError,
                RuntimeErrorMessage::CANNOT_CONVERT_BIGINT_TO_NUMBER,
                result,
                function,
            )?;
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_CONVERT_SYMBOL_TO_NUMBER,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.value().set_number(bits, function);
        result.set_normal(result.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i64_local(bits, function);
        value.clear(function);
        Ok(())
    }

    /// Sole semantic PropertyKey mint. Captured already-converted String or
    /// Symbol values take the same checked fast path without invoking hooks.
    pub(crate) fn emit_value_to_property_key_locals(
        &mut self,
        input: &ValueLocals,
        function: &mut Function,
    ) -> Result<PropertyKeyLocals, EmitError> {
        let schema = self.runtime_schema();
        let result = schema.reserve_completion(function);
        self.emit_value_to_property_key_completion(input, &result, function)?;
        self.finish_to_primitive_operation(
            ToPrimitiveAbruptRoute::ActiveHandler,
            &result,
            function,
        )?;
        let key = schema.reserve_value_local(function);
        key.copy_from(result.value(), function);
        result.clear(function);
        Ok(PropertyKeyLocals::from_converted_value(key))
    }

    pub(crate) fn emit_value_to_property_key_completion(
        &mut self,
        input: &ValueLocals,
        result: &crate::gc_types::CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.runtime_schema()
            .call_helper(
                crate::runtime_helpers::ValueToPropertyKeyArguments::new(
                    input,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        Ok(())
    }

    /// The registered PropertyKey body alone owns the original fast path and
    /// conversion sequence. Its ToPrimitive phase uses the shared String hint.
    pub(crate) fn emit_value_to_property_key_completion_inner(
        &mut self,
        input: &ValueLocals,
        result: &crate::gc_types::CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.emit_is_property_key_i32(input.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        result.set_normal(input, function);
        function.instruction(&Instruction::Else);
        self.emit_tagged_to_primitive_locals_pending(
            ToPrimitiveHint::String,
            input,
            result,
            function,
        )?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol as i32));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        let primitive = schema.reserve_value_local(function);
        primitive.copy_from(result.value(), function);
        let base = self
            .runtime_helper_base
            .ok_or_else(|| EmitError::unsupported("missing registered ValueToString helper"))?;
        schema
            .call_helper(
                crate::runtime_helpers::ValueToStringArguments::new(
                    &primitive,
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(result, function);
        primitive.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_is_property_key_i32(&self, tag: I32Local, function: &mut Function) {
        tag.load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        tag.load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
    }

    pub(crate) fn emit_value_to_bigint_locals(
        &mut self,
        input: &ValueLocals,
        number_policy: BigIntNumberPolicy,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let primitive = schema.reserve_completion(function);
        self.emit_tagged_to_primitive_locals_pending(
            ToPrimitiveHint::Number,
            input,
            &primitive,
            function,
        )?;
        result.copy_from(&primitive, function);
        primitive.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_primitive_to_bigint_locals(primitive.value(), number_policy, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        primitive.clear(function);
        Ok(())
    }

    fn emit_primitive_to_bigint_locals(
        &mut self,
        input: &ValueLocals,
        number_policy: BigIntNumberPolicy,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        result.initialize(function);
        input.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::BigInt as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.set_normal(input, function);
        function.instruction(&Instruction::Else);
        input.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Boolean as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let negative = schema.reserve_i32_local(function);
        let limb = schema.reserve_i64_local(function);
        function.instruction(&Instruction::I32Const(1));
        length.store(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::I32Const(0));
        negative.store(function);
        input.scalar().load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I64ExtendI32U);
        limb.store(function);
        let buffer = crate::gc_types::BigIntConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            length,
            function,
        );
        buffer.write(index, limb, schema, function);
        let value = schema
            .reserve_gc_local(function)
            .initialize(buffer.publish(negative, schema, function), function);
        result.value().set_reference(&value, schema, function);
        result.set_normal(result.value(), function);
        value.clear(function);
        schema.release_i64_local(limb, function);
        schema.release_i32_local(negative, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(length, function);
        function.instruction(&Instruction::Else);
        input.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        match number_policy {
            BigIntNumberPolicy::NumberToBigInt => {
                self.emit_number_to_bigint_locals(input.scalar(), result, function)?
            }
            BigIntNumberPolicy::RejectNumber => self.emit_throw_runtime_error(
                NativeErrorKind::TypeError,
                RuntimeErrorMessage::CANNOT_CONVERT_NUMBER_TO_BIGINT,
                result,
                function,
            )?,
        }
        function.instruction(&Instruction::Else);
        input.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let string = schema.reserve_gc_local(function).initialize(
            input.cast_reference::<StringValue>(schema, function),
            function,
        );
        let parsed = schema.reserve_value_local(function);
        let valid = schema.reserve_i32_local(function);
        self.emit_string_to_bigint_locals(&string, &parsed, valid, function)?;
        valid.load(function);
        self.open_frame(ControlFrameKind::If, function);
        result.set_normal(&parsed, function);
        function.instruction(&Instruction::Else);
        self.emit_throw_runtime_error(
            NativeErrorKind::SyntaxError,
            RuntimeErrorMessage::CANNOT_CONVERT_VALUE_TO_BIGINT,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(valid, function);
        parsed.clear(function);
        string.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_CONVERT_VALUE_TO_BIGINT,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// StringToBigInt has a nonthrowing failure result, consumed by ToBigInt,
    /// equality and relational comparison with their distinct continuations.
    pub(crate) fn emit_string_to_bigint_locals(
        &mut self,
        string: &GcLocal<StringValue>,
        output: &ValueLocals,
        valid: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        output.set_undefined(function);
        let start = schema.reserve_i64_local(function);
        let end = schema.reserve_i64_local(function);
        let cursor = schema.reserve_i64_local(function);
        let unit = schema.reserve_i64_local(function);
        let radix = schema.reserve_i64_local(function);
        let digit = schema.reserve_i64_local(function);
        let carry = schema.reserve_i64_local(function);
        let limb = schema.reserve_i64_local(function);
        let low = schema.reserve_i64_local(function);
        let high = schema.reserve_i64_local(function);
        let capacity = schema.reserve_i32_local(function);
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let negative = schema.reserve_i32_local(function);
        let signed = schema.reserve_i32_local(function);
        let empty = schema.reserve_i32_local(function);
        self.emit_gc_string_trim_range(string, start, end, function);
        function.instruction(&Instruction::I32Const(1));
        valid.store(function);
        function.instruction(&Instruction::I32Const(0));
        negative.store(function);
        function.instruction(&Instruction::I32Const(0));
        signed.store(function);
        start.load(function);
        end.load(function);
        function.instruction(&Instruction::I64Eq);
        empty.store(function);
        function.instruction(&Instruction::I64Const(10));
        radix.store(function);
        start.load(function);
        end.load(function);
        function.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_gc_string_code_unit_i32(string, start, function);
        function.instruction(&Instruction::I64ExtendI32U);
        unit.store(function);
        unit.load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        unit.load(function);
        function.instruction(&Instruction::I64Const(b'+' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(1));
        signed.store(function);
        unit.load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        negative.store(function);
        start.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        start.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        start.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        end.load(function);
        function.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_gc_string_code_unit_i32(string, start, function);
        function.instruction(&Instruction::I32Const(b'0' as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        start.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        cursor.store(function);
        self.emit_gc_string_code_unit_i32(string, cursor, function);
        function.instruction(&Instruction::I64ExtendI32U);
        unit.store(function);
        for (lower, upper, base) in [(b'x', b'X', 16), (b'o', b'O', 8), (b'b', b'B', 2)] {
            unit.load(function);
            function.instruction(&Instruction::I64Const(lower as i64));
            function.instruction(&Instruction::I64Eq);
            unit.load(function);
            function.instruction(&Instruction::I64Const(upper as i64));
            function.instruction(&Instruction::I64Eq);
            function.instruction(&Instruction::I32Or);
            self.open_frame(ControlFrameKind::If, function);
            signed.load(function);
            function.instruction(&Instruction::I32Eqz);
            valid.store(function);
            function.instruction(&Instruction::I64Const(base));
            radix.store(function);
            start.load(function);
            function.instruction(&Instruction::I64Const(2));
            function.instruction(&Instruction::I64Add);
            start.store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // Empty whitespace is zero; a sign or radix prefix must have a digit.
        start.load(function);
        end.load(function);
        function.instruction(&Instruction::I64Eq);
        empty.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(0));
        valid.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        end.load(function);
        start.load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(16));
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        capacity.store(function);
        let buffer = crate::gc_types::BigIntConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            capacity,
            function,
        );
        function.instruction(&Instruction::I32Const(1));
        count.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        valid.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        start.load(function);
        end.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_gc_string_code_unit_i32(string, start, function);
        function.instruction(&Instruction::I64ExtendI32U);
        unit.store(function);
        self.emit_numeric_ascii_digit_i64(unit, digit, function);
        digit.load(function);
        radix.load(function);
        function.instruction(&Instruction::I64LtU);
        valid.store(function);
        valid.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        digit.load(function);
        carry.store(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        buffer.read(index, limb, schema, function);
        limb.load(function);
        function.instruction(&Instruction::I64Const(0xffff_ffff));
        function.instruction(&Instruction::I64And);
        radix.load(function);
        function.instruction(&Instruction::I64Mul);
        carry.load(function);
        function.instruction(&Instruction::I64Add);
        low.store(function);
        limb.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        radix.load(function);
        function.instruction(&Instruction::I64Mul);
        low.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Add);
        high.store(function);
        low.load(function);
        function.instruction(&Instruction::I64Const(0xffff_ffff));
        function.instruction(&Instruction::I64And);
        high.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Or);
        limb.store(function);
        buffer.write(index, limb, schema, function);
        high.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        carry.store(function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        carry.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        buffer.write(count, carry, schema, function);
        count.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        count.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        start.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        start.store(function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let parsed = schema
            .reserve_gc_local(function)
            .initialize(buffer.publish(negative, schema, function), function);
        valid.load(function);
        self.open_frame(ControlFrameKind::If, function);
        output.set_reference(&parsed, schema, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        parsed.clear(function);
        for local in [empty, signed, negative, index, count, capacity] {
            schema.release_i32_local(local, function);
        }
        for local in [
            high, low, limb, carry, digit, radix, unit, cursor, end, start,
        ] {
            schema.release_i64_local(local, function);
        }
        Ok(())
    }

    pub(crate) fn emit_canonical_numeric_index_string(
        &mut self,
        string: &GcLocal<StringValue>,
        number: I64Local,
        canonical: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let minus_zero = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("-0", function)?,
            function,
        );
        self.emit_string_payload_equality_i32(string, &minus_zero, function);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const((-0.0_f64).to_bits() as i64));
        number.store(function);
        function.instruction(&Instruction::I32Const(1));
        canonical.store(function);
        function.instruction(&Instruction::Else);
        self.emit_string_to_number_payload(string, number, function)?;
        let converted = schema.reserve_gc_local(function).initialize(
            self.emit_number_to_string_payload(number, function)?,
            function,
        );
        self.emit_string_payload_equality_i32(string, &converted, function);
        canonical.store(function);
        converted.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        minus_zero.clear(function);
        Ok(())
    }

    pub(crate) fn emit_string_to_number_payload(
        &mut self,
        string: &GcLocal<StringValue>,
        output_local: crate::gc_types::I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        if self.outline_string_to_number {
            schema
                .call_helper(
                    crate::runtime_helpers::StringToNumberArguments::new(string),
                    self.runtime_helper_base()?,
                    function,
                )
                .store(output_local, function);
            return Ok(());
        }
        let start_local = schema.reserve_i64_local(function);
        let end_local = schema.reserve_i64_local(function);
        let index_local = schema.reserve_i64_local(function);
        let byte_local = schema.reserve_i64_local(function);
        let digit_local = schema.reserve_i64_local(function);
        let significand_local = schema.reserve_i64_local(function);
        let saw_digit_local = schema.reserve_i64_local(function);
        let dot_seen_local = schema.reserve_i64_local(function);
        let negative_local = schema.reserve_i64_local(function);
        let invalid_local = schema.reserve_i64_local(function);
        let exponent_saw_digit_local = schema.reserve_i64_local(function);
        let infinity_match_local = schema.reserve_i64_local(function);
        let decimal_start_local = schema.reserve_i64_local(function);
        let radix_local = schema.reserve_i64_local(function);
        let radix_scale_local = schema.reserve_i64_local(function);
        let discarded_nonzero_local = schema.reserve_i64_local(function);
        let read_index = schema.reserve_i64_local(function);
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(string, schema, function)
                .reference(),
            function,
        );
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, function);
        function.instruction(&Instruction::I64ExtendI32U);
        end_local.store(function);
        units.clear(function);
        function.instruction(&Instruction::I64Const(0));
        start_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        start_local.load(function);
        end_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_gc_string_code_unit_i32(string, start_local, function);
        function.instruction(&Instruction::I64ExtendI32U);
        byte_local.store(function);
        self.emit_ecmascript_whitespace_i32(byte_local, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        start_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        start_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        start_local.load(function);
        end_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        end_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        index_local.store(function);
        self.emit_gc_string_code_unit_i32(string, index_local, function);
        function.instruction(&Instruction::I64ExtendI32U);
        byte_local.store(function);
        self.emit_ecmascript_whitespace_i32(byte_local, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        index_local.load(function);
        end_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        start_local.load(function);
        end_local.load(function);
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::I64ReinterpretF64);
        output_local.store(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        radix_local.store(function);
        end_local.load(function);
        start_local.load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        start_local.load(function);
        read_index.store(function);
        self.emit_gc_string_code_unit_i32(string, read_index, function);
        function.instruction(&Instruction::I32Const(b'0' as i32));
        function.instruction(&Instruction::I32Eq);
        start_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        read_index.store(function);
        self.emit_gc_string_code_unit_i32(string, read_index, function);
        function.instruction(&Instruction::I64ExtendI32U);
        byte_local.store(function);
        for (lower, upper, radix) in [(b'b', b'B', 2), (b'o', b'O', 8), (b'x', b'X', 16)] {
            byte_local.load(function);
            function.instruction(&Instruction::I64Const(lower as i64));
            function.instruction(&Instruction::I64Eq);
            byte_local.load(function);
            function.instruction(&Instruction::I64Const(upper as i64));
            function.instruction(&Instruction::I64Eq);
            function.instruction(&Instruction::I32Or);
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::I64Const(radix));
            radix_local.store(function);
            function.instruction(&Instruction::End);
        }
        radix_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::If(BlockType::Empty));
        start_local.load(function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Add);
        start_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        significand_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        discarded_nonzero_local.store(function);
        function.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
        function.instruction(&Instruction::I64ReinterpretF64);
        radix_scale_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        saw_digit_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        invalid_local.store(function);
        start_local.load(function);
        index_local.store(function);

        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index_local.load(function);
        end_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        index_local.load(function);
        read_index.store(function);
        self.emit_gc_string_code_unit_i32(string, read_index, function);
        function.instruction(&Instruction::I64ExtendI32U);
        byte_local.store(function);
        self.emit_numeric_ascii_digit_i64(byte_local, digit_local, function);
        digit_local.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64GeS);
        digit_local.load(function);
        radix_local.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        saw_digit_local.store(function);
        // Appending at most four bits below this threshold cannot overflow u64.
        // Once retained, the integer has guard bits below binary64 precision;
        // discarded nonzero digits distinguish an exact tie from rounding up.
        significand_local.load(function);
        function.instruction(&Instruction::I64Const(1_i64 << 57));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        significand_local.load(function);
        radix_local.load(function);
        function.instruction(&Instruction::I64Mul);
        digit_local.load(function);
        function.instruction(&Instruction::I64Add);
        significand_local.store(function);
        function.instruction(&Instruction::Else);
        discarded_nonzero_local.load(function);
        digit_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Or);
        discarded_nonzero_local.store(function);
        radix_scale_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        radix_local.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::F64Mul);
        function.instruction(&Instruction::I64ReinterpretF64);
        radix_scale_local.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(1));
        invalid_local.store(function);
        end_local.load(function);
        index_local.store(function);
        function.instruction(&Instruction::End);
        index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        invalid_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        saw_digit_local.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        significand_local.load(function);
        discarded_nonzero_local.load(function);
        function.instruction(&Instruction::I64Or);
        function.instruction(&Instruction::F64ConvertI64U);
        radix_scale_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Mul);
        function.instruction(&Instruction::I64ReinterpretF64);
        output_local.store(function);
        function.instruction(&Instruction::Else);
        self.emit_nan_payload(function);
        output_local.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        start_local.load(function);
        decimal_start_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        negative_local.store(function);
        start_local.load(function);
        read_index.store(function);
        self.emit_gc_string_code_unit_i32(string, read_index, function);
        function.instruction(&Instruction::I64ExtendI32U);
        byte_local.store(function);
        byte_local.load(function);
        function.instruction(&Instruction::I64Const(b'+' as i64));
        function.instruction(&Instruction::I64Eq);
        byte_local.load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        byte_local.load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I64ExtendI32U);
        negative_local.store(function);
        start_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        start_local.store(function);
        function.instruction(&Instruction::End);

        function.instruction(&Instruction::I64Const(0));
        infinity_match_local.store(function);
        start_local.load(function);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64Add);
        end_local.load(function);
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        infinity_match_local.store(function);
        for (offset, byte) in b"Infinity".iter().copied().enumerate() {
            start_local.load(function);
            function.instruction(&Instruction::I64Const(offset as i64));
            function.instruction(&Instruction::I64Add);
            read_index.store(function);
            self.emit_gc_string_code_unit_i32(string, read_index, function);
            function.instruction(&Instruction::I64ExtendI32U);
            byte_local.store(function);
            infinity_match_local.load(function);
            byte_local.load(function);
            function.instruction(&Instruction::I64Const(byte as i64));
            function.instruction(&Instruction::I64Eq);
            function.instruction(&Instruction::I64ExtendI32U);
            function.instruction(&Instruction::I64And);
            infinity_match_local.store(function);
        }
        function.instruction(&Instruction::End);

        infinity_match_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        negative_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Result(ValType::F64)));
        function.instruction(&Instruction::F64Const(Ieee64::from(f64::NEG_INFINITY)));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::F64Const(Ieee64::from(f64::INFINITY)));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64ReinterpretF64);
        output_local.store(function);
        function.instruction(&Instruction::Else);

        function.instruction(&Instruction::I64Const(0));
        saw_digit_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        dot_seen_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        invalid_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        exponent_saw_digit_local.store(function);
        start_local.load(function);
        index_local.store(function);

        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index_local.load(function);
        end_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));

        index_local.load(function);
        read_index.store(function);
        self.emit_gc_string_code_unit_i32(string, read_index, function);
        function.instruction(&Instruction::I64ExtendI32U);
        byte_local.store(function);

        byte_local.load(function);
        function.instruction(&Instruction::I64Const(b'0' as i64));
        function.instruction(&Instruction::I64GeU);
        byte_local.load(function);
        function.instruction(&Instruction::I64Const(b'9' as i64));
        function.instruction(&Instruction::I64LeU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        saw_digit_local.store(function);
        function.instruction(&Instruction::Else);
        byte_local.load(function);
        function.instruction(&Instruction::I64Const(b'.' as i64));
        function.instruction(&Instruction::I64Eq);
        dot_seen_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        dot_seen_local.store(function);
        function.instruction(&Instruction::Else);
        byte_local.load(function);
        function.instruction(&Instruction::I64Const(b'e' as i64));
        function.instruction(&Instruction::I64Eq);
        byte_local.load(function);
        function.instruction(&Instruction::I64Const(b'E' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        saw_digit_local.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index_local.store(function);
        index_local.load(function);
        end_local.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        index_local.load(function);
        read_index.store(function);
        self.emit_gc_string_code_unit_i32(string, read_index, function);
        function.instruction(&Instruction::I64ExtendI32U);
        byte_local.store(function);
        byte_local.load(function);
        function.instruction(&Instruction::I64Const(b'+' as i64));
        function.instruction(&Instruction::I64Eq);
        byte_local.load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index_local.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index_local.load(function);
        end_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        index_local.load(function);
        read_index.store(function);
        self.emit_gc_string_code_unit_i32(string, read_index, function);
        function.instruction(&Instruction::I64ExtendI32U);
        byte_local.store(function);
        byte_local.load(function);
        function.instruction(&Instruction::I64Const(b'0' as i64));
        function.instruction(&Instruction::I64GeU);
        byte_local.load(function);
        function.instruction(&Instruction::I64Const(b'9' as i64));
        function.instruction(&Instruction::I64LeU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        exponent_saw_digit_local.store(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(1));
        invalid_local.store(function);
        end_local.load(function);
        index_local.store(function);
        function.instruction(&Instruction::End);
        index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        exponent_saw_digit_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(1));
        invalid_local.store(function);
        function.instruction(&Instruction::End);
        end_local.load(function);
        index_local.store(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(1));
        invalid_local.store(function);
        end_local.load(function);
        index_local.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        invalid_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        saw_digit_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        let decimal = schema.reserve_gc_local(function).initialize(
            self.emit_gc_string_slice(string, decimal_start_local, end_local, function),
            function,
        );
        schema
            .call_helper(
                crate::runtime_helpers::DecimalToBinary64Arguments::new(&decimal),
                self.runtime_helper_base()?,
                function,
            )
            .store(output_local, function);
        decimal.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_nan_payload(function);
        output_local.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        schema.release_i64_local(read_index, function);
        schema.release_i64_local(discarded_nonzero_local, function);
        schema.release_i64_local(radix_scale_local, function);
        schema.release_i64_local(radix_local, function);
        schema.release_i64_local(decimal_start_local, function);
        schema.release_i64_local(infinity_match_local, function);
        schema.release_i64_local(exponent_saw_digit_local, function);
        schema.release_i64_local(invalid_local, function);
        schema.release_i64_local(negative_local, function);
        schema.release_i64_local(dot_seen_local, function);
        schema.release_i64_local(saw_digit_local, function);
        schema.release_i64_local(significand_local, function);
        schema.release_i64_local(digit_local, function);
        schema.release_i64_local(byte_local, function);
        schema.release_i64_local(index_local, function);
        schema.release_i64_local(end_local, function);
        schema.release_i64_local(start_local, function);
        Ok(())
    }

    pub(crate) fn emit_gc_string_code_unit_i32(
        &self,
        string: &GcLocal<StringValue>,
        index: crate::gc_types::I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(string, schema, function)
                .reference(),
            function,
        );
        let ordinal = schema.reserve_i32_local(function);
        index.load(function);
        function.instruction(&Instruction::I32WrapI64);
        ordinal.store(function);
        let unit = schema.reserve_i32_local(function);
        schema
            .array_type::<CodeUnitArray>()
            .read(&units, ordinal, schema, function)
            .store(unit, function);
        unit.load(function);
        schema.release_i32_local(unit, function);
        schema.release_i32_local(ordinal, function);
        units.clear(function);
    }

    pub(crate) fn emit_ecmascript_whitespace_i32(
        &self,
        unit: crate::gc_types::I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I32Const(0));
        for code in [
            0x0009, 0x000a, 0x000b, 0x000c, 0x000d, 0x0020, 0x00a0, 0x1680, 0x2000, 0x2001, 0x2002,
            0x2003, 0x2004, 0x2005, 0x2006, 0x2007, 0x2008, 0x2009, 0x200a, 0x2028, 0x2029, 0x202f,
            0x205f, 0x3000, 0xfeff,
        ] {
            unit.load(function);
            function.instruction(&Instruction::I64Const(code));
            function.instruction(&Instruction::I64Eq);
            function.instruction(&Instruction::I32Or);
        }
    }

    fn emit_numeric_ascii_digit_i64(
        &self,
        unit: crate::gc_types::I64Local,
        digit: crate::gc_types::I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I64Const(-1));
        digit.store(function);
        for (first, last, adjustment) in [
            (b'0', b'9', i64::from(b'0')),
            (b'a', b'f', i64::from(b'a') - 10),
            (b'A', b'F', i64::from(b'A') - 10),
        ] {
            unit.load(function);
            function.instruction(&Instruction::I64Const(i64::from(first)));
            function.instruction(&Instruction::I64GeU);
            unit.load(function);
            function.instruction(&Instruction::I64Const(i64::from(last)));
            function.instruction(&Instruction::I64LeU);
            function.instruction(&Instruction::I32And);
            function.instruction(&Instruction::If(BlockType::Empty));
            unit.load(function);
            function.instruction(&Instruction::I64Const(adjustment));
            function.instruction(&Instruction::I64Sub);
            digit.store(function);
            function.instruction(&Instruction::End);
        }
    }

    /// Select an already-validated UTF-16 interval. A full-span result can
    /// reuse the immutable value; partial results own exact-size backing.
    pub(crate) fn emit_gc_string_slice(
        &self,
        string: &GcLocal<StringValue>,
        start: crate::gc_types::I64Local,
        end: crate::gc_types::I64Local,
        function: &mut Function,
    ) -> crate::gc_types::GcStackReference<StringValue> {
        let schema = self.runtime_schema();
        let result = schema
            .reserve_gc_local(function)
            .initialize(string.load(schema, function), function);
        let source = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(string, schema, function)
                .reference(),
            function,
        );
        start.load(function);
        function.instruction(&Instruction::I64Eqz);
        end.load(function);
        schema
            .array_type::<CodeUnitArray>()
            .length(&source, schema, function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let length = schema.reserve_i32_local(function);
        end.load(function);
        start.load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I32WrapI64);
        length.store(function);
        let construction = crate::gc_types::StringConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            length,
            function,
        );
        // Empty intervals previously skipped every source access. Keep that
        // behavior even if an internal caller supplies an out-of-range empty
        // interval; array.copy validates its source index even for zero bytes.
        length.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let zero = schema.reserve_i32_local(function);
        let source_index = schema.reserve_i32_local(function);
        zero.set_constant(0, function);
        start.load(function);
        function.instruction(&Instruction::I32WrapI64);
        source_index.store(function);
        construction.copy_units_range_from(zero, &source, source_index, length, schema, function);
        schema.release_i32_local(source_index, function);
        schema.release_i32_local(zero, function);
        function.instruction(&Instruction::End);
        result.replace(construction.publish(schema, function), function);
        schema.release_i32_local(length, function);
        function.instruction(&Instruction::End);
        source.clear(function);
        let selected = result.load(schema, function);
        result.clear(function);
        selected
    }

    pub(crate) fn compile_loose_equality_i32(
        &mut self,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let left = schema.reserve_value_local(function);
        let right = schema.reserve_value_local(function);
        self.compile_expr_to_value(lhs, &left, function)?;
        self.compile_expr_to_value(rhs, &right, function)?;
        self.emit_loose_tagged_equality_i32(&left, &right, function)?;
        right.clear(function);
        left.clear(function);
        Ok(())
    }

    pub(crate) fn emit_loose_tagged_equality_i32(
        &mut self,
        lhs: &ValueLocals,
        rhs: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let left = schema.reserve_value_local(function);
        let right = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let result = schema.reserve_i32_local(function);
        let comparison = schema.reserve_i32_local(function);
        let unordered = schema.reserve_i32_local(function);
        left.copy_from(lhs, function);
        right.copy_from(rhs, function);
        function.instruction(&Instruction::I32Const(0));
        result.store(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        left.tag().load(function);
        right.tag().load(function);
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_tagged_payload_equality_i32(&left, &right, function)?;
        result.store(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.compile_nullish_tagged_i32(left.tag(), function)?;
        self.compile_nullish_tagged_i32(right.tag(), function)?;
        function.instruction(&Instruction::I32And);
        self.compile_nullish_tagged_i32(left.tag(), function)?;
        self.emit_is_htmldda_function_i32(&right, function)?;
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        self.compile_nullish_tagged_i32(right.tag(), function)?;
        self.emit_is_htmldda_function_i32(&left, function)?;
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(1));
        result.store(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for value in [&left, &right] {
            self.emit_value_tag_is_i32(value, WasmRuntimeValueTag::Boolean, function);
            self.open_frame(ControlFrameKind::If, function);
            value.scalar().load(function);
            function.instruction(&Instruction::F64ConvertI64U);
            function.instruction(&Instruction::I64ReinterpretF64);
            value.scalar().store(function);
            value.set_number(value.scalar(), function);
            // The loop label remains outside the emitted If.
            function.instruction(&Instruction::Br(1));
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        for (string_side, other) in [(&left, &right), (&right, &left)] {
            self.emit_value_tag_is_i32(string_side, WasmRuntimeValueTag::String, function);
            self.emit_value_tag_is_i32(other, WasmRuntimeValueTag::Number, function);
            function.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, function);
            let string = schema.reserve_gc_local(function).initialize(
                string_side.cast_reference::<StringValue>(schema, function),
                function,
            );
            let number = schema.reserve_i64_local(function);
            self.emit_string_to_number_payload(&string, number, function)?;
            string_side.set_number(number, function);
            schema.release_i64_local(number, function);
            string.clear(function);
            function.instruction(&Instruction::Br(1));
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.emit_value_tag_is_i32(string_side, WasmRuntimeValueTag::String, function);
            self.emit_value_tag_is_i32(other, WasmRuntimeValueTag::BigInt, function);
            function.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, function);
            let string = schema.reserve_gc_local(function).initialize(
                string_side.cast_reference::<StringValue>(schema, function),
                function,
            );
            let parsed = schema.reserve_value_local(function);
            let valid = schema.reserve_i32_local(function);
            self.emit_string_to_bigint_locals(&string, &parsed, valid, function)?;
            string_side.copy_from(&parsed, function);
            valid.load(function);
            schema.release_i32_local(valid, function);
            parsed.clear(function);
            string.clear(function);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::Br(2));
            function.instruction(&Instruction::Else);
            self.emit_branch_to_target(exit, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        for (bigint_side, number_side) in [(&left, &right), (&right, &left)] {
            self.emit_value_tag_is_i32(bigint_side, WasmRuntimeValueTag::BigInt, function);
            self.emit_value_tag_is_i32(number_side, WasmRuntimeValueTag::Number, function);
            function.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, function);
            let bigint = schema.reserve_gc_local(function).initialize(
                bigint_side.cast_reference::<BigIntValue>(schema, function),
                function,
            );
            self.emit_bigint_number_compare(
                &bigint,
                number_side.scalar(),
                comparison,
                unordered,
                function,
            )?;
            comparison.load(function);
            function.instruction(&Instruction::I32Eqz);
            unordered.load(function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32And);
            result.store(function);
            bigint.clear(function);
            self.emit_branch_to_target(exit, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        for (object, primitive) in [(&left, &right), (&right, &left)] {
            self.emit_is_heap_object_like_tag_i32(object.tag(), function);
            self.emit_is_primitive_tag_i32(primitive.tag(), function);
            self.compile_nullish_tagged_i32(primitive.tag(), function)?;
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32And);
            function.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_tagged_to_primitive_locals_pending(
                ToPrimitiveHint::Default,
                object,
                &pending,
                function,
            )?;
            self.finish_to_primitive_operation(
                ToPrimitiveAbruptRoute::ActiveHandler,
                &pending,
                function,
            )?;
            object.copy_from(pending.value(), function);
            function.instruction(&Instruction::Br(1));
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        result.load(function);
        schema.release_i32_local(unordered, function);
        schema.release_i32_local(comparison, function);
        schema.release_i32_local(result, function);
        pending.clear(function);
        right.clear(function);
        left.clear(function);
        Ok(())
    }

    fn emit_value_tag_is_i32(
        &self,
        value: &ValueLocals,
        tag: WasmRuntimeValueTag,
        function: &mut Function,
    ) {
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(tag as i32));
        function.instruction(&Instruction::I32Eq);
    }

    pub(crate) fn emit_bigint_compare(
        &mut self,
        left: &GcLocal<BigIntValue>,
        right: &GcLocal<BigIntValue>,
        result: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let left_sign = schema.reserve_i32_local(function);
        let right_sign = schema.reserve_i32_local(function);
        let left_len = schema.reserve_i32_local(function);
        let right_len = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let left_limb = schema.reserve_i64_local(function);
        let right_limb = schema.reserve_i64_local(function);
        schema
            .struct_type::<BigIntValue>()
            .field(BigIntValueSchema::NEGATIVE)
            .read(left, schema, function)
            .store(left_sign, function);
        schema
            .struct_type::<BigIntValue>()
            .field(BigIntValueSchema::NEGATIVE)
            .read(right, schema, function)
            .store(right_sign, function);
        let left_limbs = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<BigIntValue>()
                .field(BigIntValueSchema::LIMBS)
                .read(left, schema, function)
                .reference(),
            function,
        );
        let right_limbs = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<BigIntValue>()
                .field(BigIntValueSchema::LIMBS)
                .read(right, schema, function)
                .reference(),
            function,
        );
        schema
            .array_type::<BigIntLimbArray>()
            .length(&left_limbs, schema, function);
        left_len.store(function);
        schema
            .array_type::<BigIntLimbArray>()
            .length(&right_limbs, schema, function);
        right_len.store(function);
        function.instruction(&Instruction::I32Const(0));
        result.store(function);
        left_sign.load(function);
        right_sign.load(function);
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        left_sign.load(function);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(-1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::End);
        result.store(function);
        function.instruction(&Instruction::Else);
        left_len.load(function);
        right_len.load(function);
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        left_len.load(function);
        right_len.load(function);
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(-1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::End);
        result.store(function);
        function.instruction(&Instruction::Else);
        left_len.load(function);
        index.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        index.store(function);
        schema
            .array_type::<BigIntLimbArray>()
            .read(&left_limbs, index, schema, function)
            .store_i64(left_limb, function);
        schema
            .array_type::<BigIntLimbArray>()
            .read(&right_limbs, index, schema, function)
            .store_i64(right_limb, function);
        left_limb.load(function);
        right_limb.load(function);
        function.instruction(&Instruction::I64Ne);
        self.open_frame(ControlFrameKind::If, function);
        left_limb.load(function);
        right_limb.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(-1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::End);
        result.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.load(function);
        function.instruction(&Instruction::BrIf(1));
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        left_sign.load(function);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(0));
        result.load(function);
        function.instruction(&Instruction::I32Sub);
        result.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        right_limbs.clear(function);
        left_limbs.clear(function);
        schema.release_i64_local(right_limb, function);
        schema.release_i64_local(left_limb, function);
        for local in [index, right_len, left_len, right_sign, left_sign] {
            schema.release_i32_local(local, function);
        }
    }

    /// Compare an exact integer against binary64 without rounding the integer.
    /// Only the finite Number's integral part is converted to canonical limbs.
    fn emit_bigint_number_compare(
        &mut self,
        bigint: &GcLocal<BigIntValue>,
        number: I64Local,
        result: I32Local,
        unordered: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let truncated = schema.reserve_i64_local(function);
        let pending = schema.reserve_completion(function);
        function.instruction(&Instruction::I32Const(0));
        unordered.store(function);
        function.instruction(&Instruction::I32Const(0));
        result.store(function);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Ne);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(1));
        unordered.store(function);
        function.instruction(&Instruction::Else);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Abs);
        function.instruction(&Instruction::F64Const(Ieee64::from(f64::INFINITY)));
        function.instruction(&Instruction::F64Eq);
        self.open_frame(ControlFrameKind::If, function);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Gt);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(-1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::End);
        result.store(function);
        function.instruction(&Instruction::Else);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Trunc);
        function.instruction(&Instruction::I64ReinterpretF64);
        truncated.store(function);
        self.emit_number_to_bigint_locals(truncated, &pending, function)?;
        let integer = schema.reserve_gc_local(function).initialize(
            pending
                .value()
                .cast_reference::<BigIntValue>(schema, function),
            function,
        );
        self.emit_bigint_compare(bigint, &integer, result, function);
        integer.clear(function);
        result.load(function);
        function.instruction(&Instruction::I32Eqz);
        number.load(function);
        truncated.load(function);
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Gt);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(-1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::End);
        result.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        schema.release_i64_local(truncated, function);
        Ok(())
    }

    fn emit_primitive_to_number_or_bigint(
        &mut self,
        input: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_value_tag_is_i32(input, WasmRuntimeValueTag::BigInt, function);
        self.open_frame(ControlFrameKind::If, function);
        result.set_normal(input, function);
        function.instruction(&Instruction::Else);
        self.emit_primitive_to_number_completion(input, result, false, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_string_payload_compare_i32(
        &mut self,
        op: RelationalBinaryOp,
        lhs: &GcLocal<StringValue>,
        rhs: &GcLocal<StringValue>,
        function: &mut Function,
    ) {
        self.emit_string_payload_utf16_compare_i32(lhs, rhs, function);
        function.instruction(&Instruction::I32Const(0));
        match op {
            RelationalBinaryOp::LessThan => function.instruction(&Instruction::I32LtS),
            RelationalBinaryOp::LessThanOrEqual => function.instruction(&Instruction::I32LeS),
            RelationalBinaryOp::GreaterThan => function.instruction(&Instruction::I32GtS),
            RelationalBinaryOp::GreaterThanOrEqual => function.instruction(&Instruction::I32GeS),
        };
    }

    pub(crate) fn emit_string_payload_utf16_compare_i32(
        &mut self,
        left: &GcLocal<StringValue>,
        right: &GcLocal<StringValue>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let index = schema.reserve_i32_local(function);
        let left_len = schema.reserve_i32_local(function);
        let right_len = schema.reserve_i32_local(function);
        let lhs = schema.reserve_i32_local(function);
        let rhs = schema.reserve_i32_local(function);
        let result = schema.reserve_i32_local(function);
        let l = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(left, schema, function)
                .reference(),
            function,
        );
        let r = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(right, schema, function)
                .reference(),
            function,
        );
        schema
            .array_type::<CodeUnitArray>()
            .length(&l, schema, function);
        left_len.store(function);
        schema
            .array_type::<CodeUnitArray>()
            .length(&r, schema, function);
        right_len.store(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::I32Const(0));
        result.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        left_len.load(function);
        function.instruction(&Instruction::I32GeU);
        index.load(function);
        right_len.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::BrIf(1));
        schema
            .array_type::<CodeUnitArray>()
            .read(&l, index, schema, function)
            .store(lhs, function);
        schema
            .array_type::<CodeUnitArray>()
            .read(&r, index, schema, function)
            .store(rhs, function);
        lhs.load(function);
        rhs.load(function);
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        lhs.load(function);
        rhs.load(function);
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(-1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::End);
        result.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.load(function);
        function.instruction(&Instruction::BrIf(1));
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        result.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        left_len.load(function);
        right_len.load(function);
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(-1));
        function.instruction(&Instruction::Else);
        left_len.load(function);
        right_len.load(function);
        function.instruction(&Instruction::I32GtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        result.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.load(function);
        r.clear(function);
        l.clear(function);
        for local in [result, rhs, lhs, right_len, left_len, index] {
            schema.release_i32_local(local, function);
        }
    }

    pub(crate) fn compile_typeof_payload(
        &mut self,
        expr: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let input = self.runtime_schema().reserve_value_local(function);
        self.compile_expr_to_value(expr, &input, function)?;
        self.emit_typeof_value(&input, output, function)?;
        input.clear(function);
        Ok(())
    }

    pub(crate) fn emit_typeof_value(
        &mut self,
        input: &ValueLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let object = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("object", function)?,
            function,
        );
        output.set_reference(&object, schema, function);
        object.clear(function);
        for (tag, spelling) in [
            (WasmRuntimeValueTag::Undefined, "undefined"),
            (WasmRuntimeValueTag::Boolean, "boolean"),
            (WasmRuntimeValueTag::Number, "number"),
            (WasmRuntimeValueTag::BigInt, "bigint"),
            (WasmRuntimeValueTag::Symbol, "symbol"),
            (WasmRuntimeValueTag::String, "string"),
        ] {
            input.tag().load(function);
            function.instruction(&Instruction::I32Const(tag as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            let string = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference(spelling, function)?,
                function,
            );
            output.set_reference(&string, schema, function);
            string.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.emit_is_callable_i32(input, function)?;
        self.open_frame(ControlFrameKind::If, function);
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("function", function)?,
            function,
        );
        output.set_reference(&string, schema, function);
        string.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_htmldda_function_i32(input, function)?;
        self.open_frame(ControlFrameKind::If, function);
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("undefined", function)?,
            function,
        );
        output.set_reference(&string, schema, function);
        string.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_proxy_target_is_callable_for_typeof_i32(
        &self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_is_callable_i32(value, function)
    }

    pub(crate) fn emit_is_callable_i32(
        &self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        value.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<FunctionObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        value.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<BoundFunction>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::Else);
        value.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        let proxy_slot = schema.reserve_gc_local(function);
        let proxy = proxy_slot.initialize(
            value.cast_reference::<ProxyObject>(schema, function),
            function,
        );
        let capability = schema.reserve_i32_local(function);
        schema
            .struct_type::<ProxyObject>()
            .field(ProxyObjectSchema::CALL_CAPABILITY)
            .read(&proxy, schema, function)
            .store(capability, function);
        capability.load(function);
        function.instruction(&Instruction::I32Const(
            crate::gc_types::GcI32Constant::encode(ProxyCallCapability::ObjectOnly),
        ));
        function.instruction(&Instruction::I32Ne);
        schema.release_i32_local(capability, function);
        proxy.clear(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_string_concat_payload(
        &mut self,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_coercive_add_to_locals(lhs, rhs, output, function)
    }

    pub(crate) fn emit_value_to_string_payload(
        &mut self,
        input: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        if self.outline_value_to_string {
            schema
                .call_helper(
                    crate::runtime_helpers::ValueToStringArguments::new(
                        input,
                        self.current_environment(),
                        self.current_program(),
                    ),
                    self.runtime_helper_base()?,
                    function,
                )
                .store(result, function);
            return Ok(());
        }
        let primitive = schema.reserve_completion(function);
        self.emit_tagged_to_primitive_locals_pending(
            ToPrimitiveHint::String,
            input,
            &primitive,
            function,
        )?;
        result.copy_from(&primitive, function);
        primitive.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_primitive_to_string_completion(primitive.value(), result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        primitive.clear(function);
        Ok(())
    }

    fn emit_primitive_to_string_completion(
        &mut self,
        input: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        value.copy_from(input, function);
        result.initialize(function);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.set_normal(&value, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for (tag, spelling) in [
            (WasmRuntimeValueTag::Undefined, "undefined"),
            (WasmRuntimeValueTag::Null, "null"),
        ] {
            value.tag().load(function);
            function.instruction(&Instruction::I32Const(tag as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            let string = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference(spelling, function)?,
                function,
            );
            result.value().set_reference(&string, schema, function);
            result.set_normal(result.value(), function);
            string.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Boolean as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        value.scalar().load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let false_string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("false", function)?,
            function,
        );
        result
            .value()
            .set_reference(&false_string, schema, function);
        false_string.clear(function);
        function.instruction(&Instruction::Else);
        let true_string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("true", function)?,
            function,
        );
        result.value().set_reference(&true_string, schema, function);
        true_string.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.set_normal(result.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_number_to_string_payload(value.scalar(), function)?,
            function,
        );
        result.value().set_reference(&string, schema, function);
        result.set_normal(result.value(), function);
        string.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::BigInt as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let bigint = schema.reserve_gc_local(function).initialize(
            value.cast_reference::<BigIntValue>(schema, function),
            function,
        );
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_bigint_value_to_string_payload(&bigint, function)?,
            function,
        );
        result.value().set_reference(&string, schema, function);
        result.set_normal(result.value(), function);
        string.clear(function);
        bigint.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_CONVERT_A_SYMBOL_VALUE_TO_A_STRING,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        value.clear(function);
        Ok(())
    }

    pub(crate) fn emit_primitive_to_string_payload(
        &mut self,
        input: &ValueLocals,
        result: &CompletionLocals,
        route: PrimitiveToStringAbruptRoute,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_primitive_to_string_completion(input, result, function)?;
        match route {
            PrimitiveToStringAbruptRoute::ActiveHandler => self.finish_to_primitive_operation(
                ToPrimitiveAbruptRoute::ActiveHandler,
                result,
                function,
            ),
            PrimitiveToStringAbruptRoute::ReturnCurrentFunction => self
                .finish_to_primitive_operation(
                    ToPrimitiveAbruptRoute::ReturnCurrentFunction,
                    result,
                    function,
                ),
        }
    }

    pub(crate) fn emit_bigint_to_radix_string_payload(
        &mut self,
        input: &ValueLocals,
        radix: crate::gc_types::I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let bigint = schema.reserve_gc_local(function).initialize(
            input.cast_reference::<BigIntValue>(schema, function),
            function,
        );
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_gc_bigint_radix_string(&bigint, radix, function)?,
            function,
        );
        result.value().set_reference(&string, schema, function);
        result.set_normal(result.value(), function);
        string.clear(function);
        bigint.clear(function);
        Ok(())
    }
    pub(crate) fn emit_bigint_value_to_string_payload(
        &mut self,
        bigint: &GcLocal<BigIntValue>,
        function: &mut Function,
    ) -> Result<crate::gc_types::GcStackReference<StringValue>, EmitError> {
        let schema = self.runtime_schema();
        let radix = schema.reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(10));
        radix.store(function);
        let result = self.emit_gc_bigint_radix_string(bigint, radix, function)?;
        schema.release_i64_local(radix, function);
        Ok(result)
    }

    /// Repeated division works on a private construction buffer. Published
    /// BigInt limbs stay immutable, unsigned, and canonical throughout.
    fn emit_gc_bigint_radix_string(
        &mut self,
        bigint: &GcLocal<BigIntValue>,
        radix: crate::gc_types::I64Local,
        function: &mut Function,
    ) -> Result<crate::gc_types::GcStackReference<StringValue>, EmitError> {
        let schema = self.runtime_schema();
        let output = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("0", function)?,
            function,
        );
        let limbs = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<BigIntValue>()
                .field(BigIntValueSchema::LIMBS)
                .read(bigint, schema, function)
                .reference(),
            function,
        );
        let size = schema.reserve_i32_local(function);
        let active = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let length = schema.reserve_i32_local(function);
        let negative = schema.reserve_i32_local(function);
        let digits = schema.reserve_i64_local(function);
        let limb = schema.reserve_i64_local(function);
        let remainder = schema.reserve_i64_local(function);
        let unit = schema.reserve_i32_local(function);
        schema
            .array_type::<BigIntLimbArray>()
            .length(&limbs, schema, function);
        size.store(function);
        schema
            .struct_type::<BigIntValue>()
            .field(BigIntValueSchema::NEGATIVE)
            .read(bigint, schema, function)
            .store(negative, function);
        size.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        radix.load(function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64LtU);
        radix.load(function);
        function.instruction(&Instruction::I64Const(36));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        let work = crate::gc_types::BigIntConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            size,
            function,
        );
        self.emit_copy_bigint_formatting_limbs(&limbs, &work, size, index, limb, function);
        size.load(function);
        active.store(function);
        function.instruction(&Instruction::I64Const(0));
        digits.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        active.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        self.emit_bigint_formatting_division(&work, radix, active, remainder, function);
        digits.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        digits.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        digits.load(function);
        negative.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Add);
        digits.store(function);
        digits.load(function);
        function.instruction(&Instruction::I64Const(i64::from(u32::MAX)));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        digits.load(function);
        function.instruction(&Instruction::I32WrapI64);
        length.store(function);
        let string = crate::gc_types::StringConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            length,
            function,
        );
        self.emit_copy_bigint_formatting_limbs(&limbs, &work, size, index, limb, function);
        size.load(function);
        active.store(function);
        length.load(function);
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        active.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        self.emit_bigint_formatting_division(&work, radix, active, remainder, function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        index.store(function);
        remainder.load(function);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        remainder.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Const(i32::from(b'0')));
        function.instruction(&Instruction::I32Add);
        function.instruction(&Instruction::Else);
        remainder.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Const(i32::from(b'a') - 10));
        function.instruction(&Instruction::I32Add);
        function.instruction(&Instruction::End);
        unit.store(function);
        string.write(index, unit, schema, function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        negative.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::I32Const(i32::from(b'-')));
        unit.store(function);
        string.write(index, unit, schema, function);
        function.instruction(&Instruction::End);
        output.replace(string.publish(schema, function), function);
        work.clear(function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(unit, function);
        schema.release_i64_local(remainder, function);
        schema.release_i64_local(limb, function);
        schema.release_i64_local(digits, function);
        schema.release_i32_local(negative, function);
        schema.release_i32_local(length, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(active, function);
        schema.release_i32_local(size, function);
        limbs.clear(function);
        let result = output.load(schema, function);
        output.clear(function);
        Ok(result)
    }

    fn emit_copy_bigint_formatting_limbs(
        &self,
        source: &GcLocal<BigIntLimbArray>,
        target: &crate::gc_types::BigIntConstruction,
        size: I32Local,
        index: I32Local,
        limb: crate::gc_types::I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        size.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        schema
            .array_type::<BigIntLimbArray>()
            .read(source, index, schema, function)
            .store_i64(limb, function);
        target.write(index, limb, schema, function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
    }

    fn emit_bigint_formatting_division(
        &self,
        work: &crate::gc_types::BigIntConstruction,
        radix: crate::gc_types::I64Local,
        active: I32Local,
        remainder: crate::gc_types::I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let index = schema.reserve_i32_local(function);
        let limb = schema.reserve_i64_local(function);
        let quotient = schema.reserve_i64_local(function);
        let combined = schema.reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(0));
        remainder.store(function);
        active.load(function);
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        index.store(function);
        work.read(index, limb, schema, function);
        remainder.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64Shl);
        limb.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Or);
        combined.store(function);
        combined.load(function);
        radix.load(function);
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64Shl);
        quotient.store(function);
        combined.load(function);
        radix.load(function);
        function.instruction(&Instruction::I64RemU);
        remainder.store(function);
        remainder.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64Shl);
        limb.load(function);
        function.instruction(&Instruction::I64Const(0xffff_ffff));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Or);
        combined.store(function);
        quotient.load(function);
        combined.load(function);
        radix.load(function);
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Or);
        quotient.store(function);
        combined.load(function);
        radix.load(function);
        function.instruction(&Instruction::I64RemU);
        remainder.store(function);
        work.write(index, quotient, schema, function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        active.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        active.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        index.store(function);
        work.read(index, limb, schema, function);
        limb.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        index.load(function);
        active.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i64_local(combined, function);
        schema.release_i64_local(quotient, function);
        schema.release_i64_local(limb, function);
        schema.release_i32_local(index, function);
    }

    pub(crate) fn emit_number_to_string_with_radix_result(
        &mut self,
        number: I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let argument = schema.reserve_value_local(function);
        let radix = schema.reserve_i64_local(function);
        self.emit_builtin_arg_to_value(0, &argument, function);
        function.instruction(&Instruction::I64Const(10));
        radix.store(function);
        result.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        argument.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_number_format_integer_argument(&argument, radix, result, function)?;
        self.emit_spec_operation_abrupt_exit(result, exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_number_format_range_or_throw(
            radix,
            2,
            36,
            RuntimeErrorMessage::NUMBER_PROTOTYPE_TOSTRING_RADIX_OUT_OF_RANGE,
            result,
            exit,
            function,
        )?;
        radix.load(function);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_number_format_default_string(number, result, function)?;
        function.instruction(&Instruction::Else);
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_number_to_radix_string_payload(number, radix, function)?,
            function,
        );
        result.value().set_reference(&string, schema, function);
        result.set_normal(result.value(), function);
        string.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i64_local(radix, function);
        argument.clear(function);
        Ok(())
    }

    pub(crate) fn emit_number_to_fixed_payload(
        &mut self,
        number: I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let argument = schema.reserve_value_local(function);
        let digits = schema.reserve_i64_local(function);
        self.emit_builtin_arg_to_value(0, &argument, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_number_format_integer_argument(&argument, digits, result, function)?;
        self.emit_spec_operation_abrupt_exit(result, exit, function);
        self.emit_number_format_range_or_throw(
            digits,
            0,
            100,
            RuntimeErrorMessage::NUMBER_PROTOTYPE_TOFIXED_FRACTION_DIGITS_OUT_OF_RANGE,
            result,
            exit,
            function,
        )?;
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Ne);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Abs);
        function.instruction(&Instruction::F64Const(Ieee64::from(1e21)));
        function.instruction(&Instruction::F64Ge);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_number_format_default_string(number, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_number_decimal_format_payload(
                number,
                number_to_string::NumberDecimalFormat::Fixed {
                    fraction_digits_local: digits,
                },
                function,
            )?,
            function,
        );
        result.value().set_reference(&string, schema, function);
        result.set_normal(result.value(), function);
        string.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i64_local(digits, function);
        argument.clear(function);
        Ok(())
    }

    pub(crate) fn emit_number_to_exponential_payload(
        &mut self,
        number: I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let argument = schema.reserve_value_local(function);
        let digits = schema.reserve_i64_local(function);
        let present = schema.reserve_i32_local(function);
        self.emit_builtin_arg_to_value(0, &argument, function);
        argument.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Ne);
        present.store(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_number_format_integer_argument(&argument, digits, result, function)?;
        self.emit_spec_operation_abrupt_exit(result, exit, function);
        self.emit_number_format_non_finite_i32(number, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_number_format_default_string(number, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        present.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_number_format_range_or_throw(
            digits,
            0,
            100,
            RuntimeErrorMessage::NUMBER_PROTOTYPE_TOEXPONENTIAL_FRACTION_DIGITS_OUT_OF_RANGE,
            result,
            exit,
            function,
        )?;
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_number_decimal_format_payload(
                number,
                number_to_string::NumberDecimalFormat::Exponential(
                    number_to_string::NumberExponentialFormat::FractionDigits {
                        fraction_digits_local: digits,
                    },
                ),
                function,
            )?,
            function,
        );
        result.value().set_reference(&string, schema, function);
        result.set_normal(result.value(), function);
        string.clear(function);
        function.instruction(&Instruction::Else);
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_number_decimal_format_payload(
                number,
                number_to_string::NumberDecimalFormat::Exponential(
                    number_to_string::NumberExponentialFormat::Shortest,
                ),
                function,
            )?,
            function,
        );
        result.value().set_reference(&string, schema, function);
        result.set_normal(result.value(), function);
        string.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(present, function);
        schema.release_i64_local(digits, function);
        argument.clear(function);
        Ok(())
    }

    pub(crate) fn emit_number_to_precision_payload(
        &mut self,
        number: I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let argument = schema.reserve_value_local(function);
        let digits = schema.reserve_i64_local(function);
        self.emit_builtin_arg_to_value(0, &argument, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        argument.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_number_format_default_string(number, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_number_format_integer_argument(&argument, digits, result, function)?;
        self.emit_spec_operation_abrupt_exit(result, exit, function);
        self.emit_number_format_non_finite_i32(number, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_number_format_default_string(number, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_number_format_range_or_throw(
            digits,
            1,
            100,
            RuntimeErrorMessage::NUMBER_PROTOTYPE_TOPRECISION_PRECISION_OUT_OF_RANGE,
            result,
            exit,
            function,
        )?;
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_number_decimal_format_payload(
                number,
                number_to_string::NumberDecimalFormat::Precision {
                    significant_digits_local: digits,
                },
                function,
            )?,
            function,
        );
        result.value().set_reference(&string, schema, function);
        result.set_normal(result.value(), function);
        string.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i64_local(digits, function);
        argument.clear(function);
        Ok(())
    }

    fn emit_number_format_integer_argument(
        &mut self,
        input: &ValueLocals,
        integer: I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_value_to_number_payload(input, result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            result.value().scalar(),
            integer,
            function,
        );
        integer.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::I64TruncSatF64S);
        integer.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }
    fn emit_number_format_non_finite_i32(&self, number: I64Local, function: &mut Function) {
        number.load(function);
        function.instruction(&Instruction::I64Const(i64::MAX));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Const(0x7ff0_0000_0000_0000));
        function.instruction(&Instruction::I64GeU);
    }
    fn emit_number_format_default_string(
        &mut self,
        number: I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_number_to_string_payload(number, function)?,
            function,
        );
        result.value().set_reference(&string, schema, function);
        result.set_normal(result.value(), function);
        string.clear(function);
        Ok(())
    }
    fn emit_number_format_range_or_throw(
        &mut self,
        digits: I64Local,
        minimum: i64,
        maximum: i64,
        message: RuntimeErrorMessage,
        result: &CompletionLocals,
        exit: crate::emit::ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        digits.load(function);
        function.instruction(&Instruction::I64Const(minimum));
        function.instruction(&Instruction::I64LtS);
        digits.load(function);
        function.instruction(&Instruction::I64Const(maximum));
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(NativeErrorKind::RangeError, message, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_number_to_radix_string_payload(
        &mut self,
        payload_local: I64Local,
        radix_local: I64Local,
        function: &mut Function,
    ) -> Result<crate::gc_types::GcStackReference<StringValue>, EmitError> {
        let schema = self.runtime_schema();
        let output_local = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("0", function)?,
            function,
        );
        let sign_local = schema.reserve_i64_local(function);
        let abs_local = schema.reserve_i64_local(function);
        let int_f_local = schema.reserve_i64_local(function);
        let frac_f_local = schema.reserve_i64_local(function);
        let int_u_local = schema.reserve_i64_local(function);
        let fits_u64_local = schema.reserve_i64_local(function);
        let int_digits_local = schema.reserve_i64_local(function);
        let frac_digits_local = schema.reserve_i64_local(function);
        let total_len_local = schema.reserve_i64_local(function);
        let dst_offset_local = number_to_string::FormattingBuffer::empty(schema, function);
        let digit_start_local = schema.reserve_i64_local(function);
        let frac_start_local = schema.reserve_i64_local(function);

        payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        output_local.replace(
            self.emit_interned_string_reference("NaN", function)?,
            function,
        );
        function.instruction(&Instruction::Else);
        payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Abs);
        function.instruction(&Instruction::I64ReinterpretF64);
        abs_local.store(function);
        abs_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(f64::INFINITY)));
        function.instruction(&Instruction::F64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Lt);
        function.instruction(&Instruction::If(BlockType::Empty));
        output_local.replace(
            self.emit_interned_string_reference("-Infinity", function)?,
            function,
        );
        function.instruction(&Instruction::Else);
        output_local.replace(
            self.emit_interned_string_reference("Infinity", function)?,
            function,
        );
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Lt);
        function.instruction(&Instruction::I64ExtendI32U);
        sign_local.store(function);

        // int_f = trunc(abs); frac_f = abs - int_f (in [0, 1)).
        abs_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Trunc);
        function.instruction(&Instruction::I64ReinterpretF64);
        int_f_local.store(function);
        abs_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        int_f_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Sub);
        function.instruction(&Instruction::I64ReinterpretF64);
        frac_f_local.store(function);

        // `i64.trunc_f64_u` traps outside [0, 2^64); values at or beyond that
        // (e.g. 1e21) must not reach it, so branch to a bounded
        // floating-point digit walk instead of trapping.
        int_f_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(18446744073709551616.0)));
        function.instruction(&Instruction::F64Lt);
        function.instruction(&Instruction::I64ExtendI32U);
        fits_u64_local.store(function);

        fits_u64_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        int_f_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::I64TruncF64U);
        int_u_local.store(function);
        self.emit_count_radix_digits_u64(int_u_local, radix_local, int_digits_local, function);
        function.instruction(&Instruction::Else);
        self.emit_count_radix_digits_f64_bounded(
            int_f_local,
            radix_local,
            int_digits_local,
            function,
        );
        function.instruction(&Instruction::End);

        self.emit_count_radix_fraction_digits(
            frac_f_local,
            radix_local,
            frac_digits_local,
            function,
        );

        sign_local.load(function);
        int_digits_local.load(function);
        function.instruction(&Instruction::I64Add);
        frac_digits_local.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::Else);
        frac_digits_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Add);
        total_len_local.store(function);
        dst_offset_local.resize(total_len_local, schema, function);
        function.instruction(&Instruction::I64Const(0));
        digit_start_local.store(function);
        sign_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.store_ascii_byte_i64(&dst_offset_local, digit_start_local, b'-', function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        digit_start_local.store(function);
        function.instruction(&Instruction::End);

        fits_u64_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_write_radix_u64(
            int_u_local,
            radix_local,
            digit_start_local,
            int_digits_local,
            &dst_offset_local,
            function,
        );
        function.instruction(&Instruction::Else);
        self.emit_write_radix_f64_bounded(
            int_f_local,
            radix_local,
            digit_start_local,
            int_digits_local,
            &dst_offset_local,
            function,
        );
        function.instruction(&Instruction::End);

        digit_start_local.load(function);
        int_digits_local.load(function);
        function.instruction(&Instruction::I64Add);
        frac_start_local.store(function);
        frac_digits_local.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.store_ascii_byte_i64(&dst_offset_local, frac_start_local, b'.', function);
        frac_start_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        frac_start_local.store(function);
        self.emit_write_radix_fraction_digits(
            frac_f_local,
            radix_local,
            frac_start_local,
            frac_digits_local,
            &dst_offset_local,
            function,
        );
        function.instruction(&Instruction::End);

        output_local.replace(dst_offset_local.publish_string(schema, function), function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        let output = output_local.load(schema, function);

        schema.release_i64_local(frac_start_local, function);
        schema.release_i64_local(digit_start_local, function);
        dst_offset_local.clear(function);
        schema.release_i64_local(total_len_local, function);
        schema.release_i64_local(frac_digits_local, function);
        schema.release_i64_local(int_digits_local, function);
        schema.release_i64_local(fits_u64_local, function);
        schema.release_i64_local(int_u_local, function);
        schema.release_i64_local(frac_f_local, function);
        schema.release_i64_local(int_f_local, function);
        schema.release_i64_local(abs_local, function);
        schema.release_i64_local(sign_local, function);
        output_local.clear(function);
        Ok(output)
    }

    pub(crate) fn emit_count_decimal_digits_u64(
        &mut self,
        value_local: I64Local,
        digits_local: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let temp_local = schema.reserve_i64_local(function);
        value_local.load(function);
        temp_local.store(function);
        function.instruction(&Instruction::I64Const(1));
        digits_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        temp_local.load(function);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::BrIf(1));
        temp_local.load(function);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64DivU);
        temp_local.store(function);
        digits_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        digits_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i64_local(temp_local, function);
    }

    pub(crate) fn emit_count_radix_digits_u64(
        &mut self,
        value_local: I64Local,
        radix_local: I64Local,
        digits_local: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let temp_local = schema.reserve_i64_local(function);
        value_local.load(function);
        temp_local.store(function);
        function.instruction(&Instruction::I64Const(1));
        digits_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        temp_local.load(function);
        radix_local.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::BrIf(1));
        temp_local.load(function);
        radix_local.load(function);
        function.instruction(&Instruction::I64DivU);
        temp_local.store(function);
        digits_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        digits_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i64_local(temp_local, function);
    }

    pub(crate) fn emit_write_decimal_u64(
        &mut self,
        value_local: I64Local,
        start_offset_local: I64Local,
        digits_local: I64Local,
        output_buffer: &number_to_string::FormattingBuffer,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let temp_local = schema.reserve_i64_local(function);
        let index_local = schema.reserve_i64_local(function);
        let pos_local = schema.reserve_i64_local(function);
        let digit_local = schema.reserve_i64_local(function);

        value_local.load(function);
        temp_local.store(function);
        start_offset_local.load(function);
        digits_local.load(function);
        function.instruction(&Instruction::I64Add);
        pos_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        index_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index_local.load(function);
        digits_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        pos_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        pos_local.store(function);
        temp_local.load(function);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64RemU);
        digit_local.store(function);
        pos_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        digit_local.load(function);
        function.instruction(&Instruction::I64Const(b'0' as i64));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        output_buffer.write_from_stack(schema, function);
        temp_local.load(function);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64DivU);
        temp_local.store(function);
        index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        schema.release_i64_local(digit_local, function);
        schema.release_i64_local(pos_local, function);
        schema.release_i64_local(index_local, function);
        schema.release_i64_local(temp_local, function);
    }

    pub(crate) fn emit_write_radix_u64(
        &mut self,
        value_local: I64Local,
        radix_local: I64Local,
        start_offset_local: I64Local,
        digits_local: I64Local,
        output_buffer: &number_to_string::FormattingBuffer,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let temp_local = schema.reserve_i64_local(function);
        let index_local = schema.reserve_i64_local(function);
        let pos_local = schema.reserve_i64_local(function);
        let digit_local = schema.reserve_i64_local(function);
        let char_local = schema.reserve_i64_local(function);

        value_local.load(function);
        temp_local.store(function);
        start_offset_local.load(function);
        digits_local.load(function);
        function.instruction(&Instruction::I64Add);
        pos_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        index_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index_local.load(function);
        digits_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        pos_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        pos_local.store(function);
        temp_local.load(function);
        radix_local.load(function);
        function.instruction(&Instruction::I64RemU);
        digit_local.store(function);
        digit_local.load(function);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        digit_local.load(function);
        function.instruction(&Instruction::I64Const(b'0' as i64));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::Else);
        digit_local.load(function);
        function.instruction(&Instruction::I64Const((b'a' - 10) as i64));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::End);
        char_local.store(function);
        pos_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        char_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        output_buffer.write_from_stack(schema, function);
        temp_local.load(function);
        radix_local.load(function);
        function.instruction(&Instruction::I64DivU);
        temp_local.store(function);
        index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        schema.release_i64_local(char_local, function);
        schema.release_i64_local(digit_local, function);
        schema.release_i64_local(pos_local, function);
        schema.release_i64_local(index_local, function);
        schema.release_i64_local(temp_local, function);
    }

    // Bounded, floating-point-based counterparts of `emit_count_radix_digits_u64`
    // / `emit_write_radix_u64` for integer magnitudes that do not fit in a u64
    // (so `i64.trunc_f64_u` would trap). Digit extraction stays in the f64
    // domain the whole time (never truncates the magnitude itself into an
    // integer type), so it can never trap; only the final small remainder
    // (always in `[0, radix)`) is ever truncated to i64. For power-of-two
    // radixes (2, 8, 16, 32) every division/multiplication by the radix is
    // exact in IEEE 754, so the produced digits are exact. For other radixes
    // at magnitudes this large, digits beyond the double's ~53 bits of
    // precision are not fully significant regardless of algorithm (the
    // double itself does not carry that information); the loop is bounded so
    // it always terminates instead of hanging or trapping.
    const MAX_RADIX_BIG_INTEGER_DIGITS: i64 = 1100;
    const MAX_RADIX_FRACTION_DIGITS: i64 = 1100;

    pub(crate) fn emit_count_radix_digits_f64_bounded(
        &mut self,
        value_local: I64Local,
        radix_local: I64Local,
        digits_local: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let temp_local = schema.reserve_i64_local(function);
        let radix_f_local = schema.reserve_i64_local(function);

        radix_local.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        radix_f_local.store(function);

        value_local.load(function);
        temp_local.store(function);
        function.instruction(&Instruction::I64Const(1));
        digits_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        temp_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        radix_f_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Lt);
        function.instruction(&Instruction::BrIf(1));
        temp_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        radix_f_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Div);
        function.instruction(&Instruction::F64Floor);
        function.instruction(&Instruction::I64ReinterpretF64);
        temp_local.store(function);
        digits_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        digits_local.store(function);
        digits_local.load(function);
        function.instruction(&Instruction::I64Const(Self::MAX_RADIX_BIG_INTEGER_DIGITS));
        function.instruction(&Instruction::I64GeS);
        function.instruction(&Instruction::BrIf(1));
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        schema.release_i64_local(radix_f_local, function);
        schema.release_i64_local(temp_local, function);
    }

    pub(crate) fn emit_write_radix_f64_bounded(
        &mut self,
        value_local: I64Local,
        radix_local: I64Local,
        start_offset_local: I64Local,
        digits_local: I64Local,
        output_buffer: &number_to_string::FormattingBuffer,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let temp_local = schema.reserve_i64_local(function);
        let radix_f_local = schema.reserve_i64_local(function);
        let index_local = schema.reserve_i64_local(function);
        let pos_local = schema.reserve_i64_local(function);
        let quotient_local = schema.reserve_i64_local(function);
        let digit_local = schema.reserve_i64_local(function);
        let char_local = schema.reserve_i64_local(function);

        radix_local.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        radix_f_local.store(function);

        value_local.load(function);
        temp_local.store(function);
        start_offset_local.load(function);
        digits_local.load(function);
        function.instruction(&Instruction::I64Add);
        pos_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        index_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index_local.load(function);
        digits_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        pos_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        pos_local.store(function);

        temp_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        radix_f_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Div);
        function.instruction(&Instruction::F64Floor);
        function.instruction(&Instruction::I64ReinterpretF64);
        quotient_local.store(function);

        temp_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        quotient_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        radix_f_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Mul);
        function.instruction(&Instruction::F64Sub);
        // For non-power-of-two radixes at magnitudes far beyond exact double
        // precision (e.g. Number.MAX_VALUE), `quotient * radix_f` is itself
        // only accurate to ~1e-16 relative error, but `quotient` there can be
        // ~1e300+, so the *absolute* error can be enormous — nowhere near
        // "infinitesimally outside [0, radix_f)". `temp - quotient * radix_f`
        // can land far negative or far positive. `i64.trunc_f64_u` traps on
        // any input outside `[0, 2^64)`, so clamp both bounds defensively
        // before truncating: never let floating-point noise crash the VM
        // (the digit itself is already best-effort/inexact at this
        // magnitude for non-power-of-two radixes; clamping just guarantees
        // it stays a valid digit character instead of trapping).
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Max);
        radix_f_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
        function.instruction(&Instruction::F64Sub);
        function.instruction(&Instruction::F64Min);
        function.instruction(&Instruction::I64TruncF64U);
        digit_local.store(function);

        digit_local.load(function);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        digit_local.load(function);
        function.instruction(&Instruction::I64Const(b'0' as i64));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::Else);
        digit_local.load(function);
        function.instruction(&Instruction::I64Const((b'a' - 10) as i64));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::End);
        char_local.store(function);
        pos_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        char_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        output_buffer.write_from_stack(schema, function);

        quotient_local.load(function);
        temp_local.store(function);
        index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index_local.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        schema.release_i64_local(char_local, function);
        schema.release_i64_local(digit_local, function);
        schema.release_i64_local(quotient_local, function);
        schema.release_i64_local(pos_local, function);
        schema.release_i64_local(index_local, function);
        schema.release_i64_local(radix_f_local, function);
        schema.release_i64_local(temp_local, function);
    }

    // Counts/writes the fractional digits of the Number::toString(radix)
    // representation. `frac_local` holds the f64 bit pattern of a value in
    // `[0, 1)` (`abs - trunc(abs)`). Bounded the same way as the integer
    // helpers above so a fraction that never exactly terminates in the
    // requested radix (e.g. any non-power-of-two radix applied to a value
    // whose exact binary fraction doesn't share the radix's prime factors)
    // still produces a finite string instead of looping forever.
    pub(crate) fn emit_count_radix_fraction_digits(
        &mut self,
        frac_local: I64Local,
        radix_local: I64Local,
        digits_local: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let temp_local = schema.reserve_i64_local(function);
        let radix_f_local = schema.reserve_i64_local(function);
        let digit_f_local = schema.reserve_i64_local(function);

        radix_local.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        radix_f_local.store(function);

        frac_local.load(function);
        temp_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        digits_local.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));

        temp_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Le);
        function.instruction(&Instruction::BrIf(1));

        temp_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        radix_f_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Mul);
        function.instruction(&Instruction::I64ReinterpretF64);
        temp_local.store(function);

        temp_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Floor);
        function.instruction(&Instruction::I64ReinterpretF64);
        digit_f_local.store(function);

        temp_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        digit_f_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Sub);
        function.instruction(&Instruction::I64ReinterpretF64);
        temp_local.store(function);

        digits_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        digits_local.store(function);

        digits_local.load(function);
        function.instruction(&Instruction::I64Const(Self::MAX_RADIX_FRACTION_DIGITS));
        function.instruction(&Instruction::I64GeS);
        function.instruction(&Instruction::BrIf(1));

        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        schema.release_i64_local(digit_f_local, function);
        schema.release_i64_local(radix_f_local, function);
        schema.release_i64_local(temp_local, function);
    }

    pub(crate) fn emit_write_radix_fraction_digits(
        &mut self,
        frac_local: I64Local,
        radix_local: I64Local,
        start_offset_local: I64Local,
        digits_local: I64Local,
        output_buffer: &number_to_string::FormattingBuffer,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let temp_local = schema.reserve_i64_local(function);
        let radix_f_local = schema.reserve_i64_local(function);
        let digit_f_local = schema.reserve_i64_local(function);
        let digit_local = schema.reserve_i64_local(function);
        let char_local = schema.reserve_i64_local(function);
        let pos_local = schema.reserve_i64_local(function);
        let index_local = schema.reserve_i64_local(function);

        radix_local.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        radix_f_local.store(function);

        frac_local.load(function);
        temp_local.store(function);
        start_offset_local.load(function);
        pos_local.store(function);
        function.instruction(&Instruction::I64Const(0));
        index_local.store(function);

        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index_local.load(function);
        digits_local.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));

        temp_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        radix_f_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Mul);
        function.instruction(&Instruction::I64ReinterpretF64);
        temp_local.store(function);

        temp_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Floor);
        function.instruction(&Instruction::I64ReinterpretF64);
        digit_f_local.store(function);

        digit_f_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::I64TruncF64U);
        digit_local.store(function);

        digit_local.load(function);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        digit_local.load(function);
        function.instruction(&Instruction::I64Const(b'0' as i64));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::Else);
        digit_local.load(function);
        function.instruction(&Instruction::I64Const((b'a' - 10) as i64));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::End);
        char_local.store(function);

        pos_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        char_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        output_buffer.write_from_stack(schema, function);

        temp_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        digit_f_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Sub);
        function.instruction(&Instruction::I64ReinterpretF64);
        temp_local.store(function);

        pos_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        pos_local.store(function);

        index_local.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index_local.store(function);

        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        schema.release_i64_local(index_local, function);
        schema.release_i64_local(pos_local, function);
        schema.release_i64_local(char_local, function);
        schema.release_i64_local(digit_local, function);
        schema.release_i64_local(digit_f_local, function);
        schema.release_i64_local(radix_f_local, function);
        schema.release_i64_local(temp_local, function);
    }

    pub(crate) fn store_ascii_byte_i64(
        &self,
        output_buffer: &number_to_string::FormattingBuffer,
        offset_local: I64Local,
        byte: u8,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        offset_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Const(i32::from(byte)));
        output_buffer.write_from_stack(schema, function);
    }

    fn emit_decimal_sign(
        &self,
        sign: I64Local,
        buffer: &number_to_string::FormattingBuffer,
        number_start: I64Local,
        function: &mut Function,
    ) {
        sign.load(function);
        number_start.store(function);
        sign.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Else);
        let zero = self.runtime_schema().reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(0));
        zero.store(function);
        self.store_ascii_byte_i64(buffer, zero, b'-', function);
        self.runtime_schema().release_i64_local(zero, function);
        function.instruction(&Instruction::End);
    }

    fn emit_repeated_ascii(
        &self,
        buffer: &number_to_string::FormattingBuffer,
        start: I64Local,
        count: I64Local,
        byte: u8,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let index = schema.reserve_i64_local(function);
        let offset = schema.reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        start.load(function);
        index.load(function);
        function.instruction(&Instruction::I64Add);
        offset.store(function);
        self.store_ascii_byte_i64(buffer, offset, byte, function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i64_local(offset, function);
        schema.release_i64_local(index, function);
    }

    pub(crate) fn emit_string_search_argument_is_regexp_to_local(
        &mut self,
        input: &ValueLocals,
        output: I32Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        function.instruction(&Instruction::I32Const(0));
        output.store(function);
        result.initialize(function);
        self.emit_is_heap_object_like_tag_i32(input.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        let symbol = schema.reserve_gc_local(function).initialize(
            self.emit_well_known_symbol_reference(scriptw_ir::WellKnownSymbol::Match, function)?,
            function,
        );
        let key = PropertyKeyLocals::from_symbol(schema, &symbol, function);
        symbol.clear(function);
        let pending = schema.reserve_completion(function);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectReadArguments::new(
                    input,
                    input,
                    &key,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(&pending, function);
        key.clear(function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(&pending, function);
        function.instruction(&Instruction::Else);
        pending.value().tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        output.store(function);
        function.instruction(&Instruction::Else);
        input.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::RegExpObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        output.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.value().set_boolean(output, function);
        result.set_normal(result.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_string_payload_equality_i32(
        &self,
        left: &GcLocal<StringValue>,
        right: &GcLocal<StringValue>,
        function: &mut Function,
    ) {
        self.emit_string_payload_equality_i32_with_ascii_case_folding(left, right, None, function);
    }

    pub(crate) fn emit_string_payload_equality_i32_with_ascii_case_folding(
        &self,
        left: &GcLocal<StringValue>,
        right: &GcLocal<StringValue>,
        ascii_case_folding: Option<I32Local>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let default_fold = ascii_case_folding
            .is_none()
            .then(|| schema.reserve_i32_local(function));
        let fold = ascii_case_folding
            .or(default_fold)
            .expect("folding operand is initialized");
        if default_fold.is_some() {
            function.instruction(&Instruction::I32Const(0));
            fold.store(function);
        }
        let result = schema.reserve_i32_local(function);
        if self.outline_string_equality {
            if let Some(base) = self.runtime_helper_base {
                schema
                    .call_helper(
                        crate::runtime_helpers::StringEqualityArguments::new(left, right, fold),
                        base,
                        function,
                    )
                    .store(result, function);
            } else {
                self.emit_gc_string_equality(left, right, fold, result, function);
            }
        } else {
            self.emit_gc_string_equality(left, right, fold, result, function);
        }
        result.load(function);
        schema.release_i32_local(result, function);
        if let Some(default_fold) = default_fold {
            schema.release_i32_local(default_fold, function);
        }
    }

    pub(crate) fn emit_gc_string_equality(
        &self,
        left: &GcLocal<StringValue>,
        right: &GcLocal<StringValue>,
        ascii_case_folding: I32Local,
        result: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let left_units_slot = schema.reserve_gc_local(function);
        let left_units = left_units_slot.initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(left, schema, function)
                .reference(),
            function,
        );
        let right_units_slot = schema.reserve_gc_local(function);
        let right_units = right_units_slot.initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(right, schema, function)
                .reference(),
            function,
        );
        let left_length = schema.reserve_i32_local(function);
        let right_length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let left_unit = schema.reserve_i32_local(function);
        let right_unit = schema.reserve_i32_local(function);
        let units = schema.array_type::<CodeUnitArray>();
        units.length(&left_units, schema, function);
        left_length.store(function);
        units.length(&right_units, schema, function);
        right_length.store(function);

        function.instruction(&Instruction::I32Const(1));
        result.store(function);
        left_length.load(function);
        right_length.load(function);
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I32Const(0));
        result.store(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        left_length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        units
            .read(&left_units, index, schema, function)
            .store(left_unit, function);
        units
            .read(&right_units, index, schema, function)
            .store(right_unit, function);
        ascii_case_folding.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        for unit in [left_unit, right_unit] {
            unit.load(function);
            function.instruction(&Instruction::I32Const(i32::from(b'A')));
            function.instruction(&Instruction::I32GeU);
            unit.load(function);
            function.instruction(&Instruction::I32Const(i32::from(b'Z')));
            function.instruction(&Instruction::I32LeU);
            function.instruction(&Instruction::I32And);
            function.instruction(&Instruction::If(BlockType::Empty));
            unit.load(function);
            function.instruction(&Instruction::I32Const(i32::from(b'a' - b'A')));
            function.instruction(&Instruction::I32Add);
            unit.store(function);
            function.instruction(&Instruction::End);
        }
        function.instruction(&Instruction::End);
        left_unit.load(function);
        right_unit.load(function);
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I32Const(0));
        result.store(function);
        function.instruction(&Instruction::Br(2));
        function.instruction(&Instruction::End);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);

        schema.release_i32_local(right_unit, function);
        schema.release_i32_local(left_unit, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(right_length, function);
        schema.release_i32_local(left_length, function);
        right_units.clear(function);
        left_units.clear(function);
    }

    pub(crate) fn compile_strict_equality_i32(
        &mut self,
        left: &TypedExpr,
        right: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let left_value = self.runtime_schema().reserve_value_local(function);
        let right_value = self.runtime_schema().reserve_value_local(function);
        self.compile_expr_to_value(left, &left_value, function)?;
        self.compile_expr_to_value(right, &right_value, function)?;
        self.emit_tagged_payload_equality_i32(&left_value, &right_value, function)?;
        right_value.clear(function);
        left_value.clear(function);
        self.set_completion_kind(CompletionKind::Normal, function);
        Ok(())
    }

    pub(crate) fn emit_assert_same_value(
        &mut self,
        actual: &TypedExpr,
        expected: &TypedExpr,
        message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_same_value_i32(actual, expected, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let error = self.runtime_schema().reserve_completion(function);
        self.emit_throw_runtime_error(NativeErrorKind::Error, message, &error, function)?;
        self.completion().copy_from(&error, function);
        error.clear(function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_same_value_i32(
        &mut self,
        left: &TypedExpr,
        right: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let left_value = self.runtime_schema().reserve_value_local(function);
        let right_value = self.runtime_schema().reserve_value_local(function);
        self.compile_expr_to_value(left, &left_value, function)?;
        self.compile_expr_to_value(right, &right_value, function)?;
        self.emit_tagged_payload_same_value_i32(&left_value, &right_value, function)?;
        right_value.clear(function);
        left_value.clear(function);
        Ok(())
    }

    pub(crate) fn compile_same_value_zero_i32(
        &mut self,
        left: &TypedExpr,
        right: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let left_value = self.runtime_schema().reserve_value_local(function);
        let right_value = self.runtime_schema().reserve_value_local(function);
        self.compile_expr_to_value(left, &left_value, function)?;
        self.compile_expr_to_value(right, &right_value, function)?;
        self.emit_tagged_payload_same_value_zero_i32(&left_value, &right_value, function)?;
        right_value.clear(function);
        left_value.clear(function);
        Ok(())
    }

    pub(crate) fn emit_tagged_payload_same_value_i32(
        &self,
        left: &ValueLocals,
        right: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_value_equality_i32(left, right, NumberEquality::SameValue, function)
    }

    pub(crate) fn emit_tagged_payload_same_value_zero_i32(
        &self,
        left: &ValueLocals,
        right: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_value_equality_i32(left, right, NumberEquality::SameValueZero, function)
    }

    pub(crate) fn emit_tagged_payload_equality_i32(
        &self,
        left: &ValueLocals,
        right: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_value_equality_i32(left, right, NumberEquality::Strict, function)
    }

    fn emit_value_equality_i32(
        &self,
        left: &ValueLocals,
        right: &ValueLocals,
        numbers: NumberEquality,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        left.tag().load(function);
        right.tag().load(function);
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        left.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        match numbers {
            NumberEquality::SameValue => {
                left.scalar().load(function);
                right.scalar().load(function);
                function.instruction(&Instruction::I64Eq);
            }
            NumberEquality::Strict | NumberEquality::SameValueZero => {
                left.scalar().load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                right.scalar().load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                function.instruction(&Instruction::F64Eq);
            }
        }
        match numbers {
            NumberEquality::Strict => {}
            NumberEquality::SameValue | NumberEquality::SameValueZero => {
                for value in [left, right] {
                    value.scalar().load(function);
                    function.instruction(&Instruction::F64ReinterpretI64);
                    value.scalar().load(function);
                    function.instruction(&Instruction::F64ReinterpretI64);
                    function.instruction(&Instruction::F64Ne);
                }
                function.instruction(&Instruction::I32And);
                function.instruction(&Instruction::I32Or);
            }
        }
        function.instruction(&Instruction::Else);
        left.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        // Pooled/interned strings are usually the same reference.
        left.reference().load(function);
        right.reference().load(function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::Else);
        let left_string_slot = schema.reserve_gc_local(function);
        let left_string = left_string_slot.initialize(
            left.cast_reference::<StringValue>(schema, function),
            function,
        );
        let right_string_slot = schema.reserve_gc_local(function);
        let right_string = right_string_slot.initialize(
            right.cast_reference::<StringValue>(schema, function),
            function,
        );
        self.emit_string_payload_equality_i32(&left_string, &right_string, function);
        right_string.clear(function);
        left_string.clear(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        left.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::BigInt as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        let left_bigint_slot = schema.reserve_gc_local(function);
        let left_bigint = left_bigint_slot.initialize(
            left.cast_reference::<BigIntValue>(schema, function),
            function,
        );
        let right_bigint_slot = schema.reserve_gc_local(function);
        let right_bigint = right_bigint_slot.initialize(
            right.cast_reference::<BigIntValue>(schema, function),
            function,
        );
        self.emit_bigint_equality_i32(&left_bigint, &right_bigint, function);
        right_bigint.clear(function);
        left_bigint.clear(function);
        function.instruction(&Instruction::Else);
        self.compile_nullish_tagged_i32(left.tag(), function)?;
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::Else);
        left.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Boolean as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        left.scalar().load(function);
        right.scalar().load(function);
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::Else);
        left.reference().load(function);
        right.reference().load(function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_bigint_equality_i32(
        &self,
        left: &GcLocal<BigIntValue>,
        right: &GcLocal<BigIntValue>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let left_sign = schema.reserve_i32_local(function);
        let right_sign = schema.reserve_i32_local(function);
        let bigint = schema.struct_type::<BigIntValue>();
        bigint
            .field(BigIntValueSchema::NEGATIVE)
            .read(left, schema, function)
            .store(left_sign, function);
        bigint
            .field(BigIntValueSchema::NEGATIVE)
            .read(right, schema, function)
            .store(right_sign, function);
        let left_limbs_slot = schema.reserve_gc_local(function);
        let left_limbs = left_limbs_slot.initialize(
            bigint
                .field(BigIntValueSchema::LIMBS)
                .read(left, schema, function)
                .reference(),
            function,
        );
        let right_limbs_slot = schema.reserve_gc_local(function);
        let right_limbs = right_limbs_slot.initialize(
            bigint
                .field(BigIntValueSchema::LIMBS)
                .read(right, schema, function)
                .reference(),
            function,
        );
        let left_length = schema.reserve_i32_local(function);
        let right_length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let equal = schema.reserve_i32_local(function);
        let left_limb = schema.reserve_i64_local(function);
        let right_limb = schema.reserve_i64_local(function);
        let limbs = schema.array_type::<BigIntLimbArray>();
        limbs.length(&left_limbs, schema, function);
        left_length.store(function);
        limbs.length(&right_limbs, schema, function);
        right_length.store(function);
        left_sign.load(function);
        right_sign.load(function);
        function.instruction(&Instruction::I32Eq);
        left_length.load(function);
        right_length.load(function);
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        equal.store(function);
        equal.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        left_length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        limbs
            .read(&left_limbs, index, schema, function)
            .store_i64(left_limb, function);
        limbs
            .read(&right_limbs, index, schema, function)
            .store_i64(right_limb, function);
        left_limb.load(function);
        right_limb.load(function);
        function.instruction(&Instruction::I64Eq);
        equal.store(function);
        equal.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        equal.load(function);
        schema.release_i64_local(right_limb, function);
        schema.release_i64_local(left_limb, function);
        schema.release_i32_local(equal, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(right_length, function);
        schema.release_i32_local(left_length, function);
        right_limbs.clear(function);
        left_limbs.clear(function);
        schema.release_i32_local(right_sign, function);
        schema.release_i32_local(left_sign, function);
    }
}
