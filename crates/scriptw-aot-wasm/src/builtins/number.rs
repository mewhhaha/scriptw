//! Native Number entries preserve primitive identity and whole abrupt results.

use super::super::*;
use crate::functions::OrdinaryDefaultPrototype;
use crate::gc_types::{
    GcNullability, GcOperand, PrimitiveBox, PrimitiveBoxSchema, StoredValue,
};

#[derive(Clone, Copy)]
enum NumberPredicate {
    Integer,
    SafeInteger,
    Finite,
    NaN,
}
enum NumberPrototypeOperation {
    ToExponential,
    ToFixed,
    ToPrecision,
    ToString,
    ToLocaleString,
    ValueOf,
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_number_constructor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let argument = schema.reserve_value_local(function);
        let new_target = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        pending
            .value()
            .set_scalar(crate::gc_types::ScalarValue::NumberBits(0), function);
        let count = self
            .body_entry_locals()
            .ok_or_else(|| {
                EmitError::unsupported("Number constructor requires declared body entry")
            })?
            .argument_count();
        count.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_builtin_arg_to_value(0, &argument, function);
        self.emit_value_to_number_payload_allow_bigint(&argument, &pending, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let output = schema.reserve_completion(function);
        output.copy_from(&pending, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(exit, function);
        self.compile_new_target_to_locals(&new_target, function)?;
        new_target.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(exit, function);
        let prototype = schema.reserve_completion(function);
        self.emit_get_prototype_from_constructor(
            &new_target,
            OrdinaryDefaultPrototype::Number,
            &prototype,
            function,
        )?;
        output.copy_from(&prototype, function);
        prototype.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(prototype.value()), function)?,
            function,
        );
        let primitive = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(pending.value(), function),
            function,
        );
        let boxed = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<PrimitiveBox>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::reference(&primitive, schema),
                ),
                function,
            ),
            function,
        );
        let value = schema.reserve_value_local(function);
        value.set_reference(&boxed, schema, function);
        output.set_normal(&value, function);
        value.clear(function);
        boxed.clear(function);
        primitive.clear(function);
        header.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        prototype.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        output.clear(function);
        pending.clear(function);
        new_target.clear(function);
        argument.clear(function);
        Ok(())
    }

    fn emit_number_predicate(
        &mut self,
        predicate: NumberPredicate,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let argument = schema.reserve_value_local(function);
        let value = schema.reserve_value_local(function);
        let answer = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        answer.store(function);
        self.emit_builtin_arg_to_value(0, &argument, function);
        argument.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        match predicate {
            NumberPredicate::NaN => {
                argument.scalar().load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                argument.scalar().load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                function.instruction(&Instruction::F64Ne);
            }
            NumberPredicate::Finite | NumberPredicate::Integer | NumberPredicate::SafeInteger => {
                argument.scalar().load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
                function.instruction(&Instruction::F64Abs);
                function.instruction(&Instruction::F64Const(Ieee64::from(f64::INFINITY)));
                function.instruction(&Instruction::F64Lt);
                if matches!(
                    predicate,
                    NumberPredicate::Integer | NumberPredicate::SafeInteger
                ) {
                    argument.scalar().load(function);
                    function.instruction(&Instruction::F64ReinterpretI64);
                    function.instruction(&Instruction::F64Trunc);
                    argument.scalar().load(function);
                    function.instruction(&Instruction::F64ReinterpretI64);
                    function.instruction(&Instruction::F64Eq);
                    function.instruction(&Instruction::I32And);
                }
                if matches!(predicate, NumberPredicate::SafeInteger) {
                    argument.scalar().load(function);
                    function.instruction(&Instruction::F64ReinterpretI64);
                    function.instruction(&Instruction::F64Abs);
                    function.instruction(&Instruction::F64Const(Ieee64::from(
                        9_007_199_254_740_991.0,
                    )));
                    function.instruction(&Instruction::F64Le);
                    function.instruction(&Instruction::I32And);
                }
            }
        }
        answer.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        value.set_boolean(answer, function);
        self.completion().set_normal(&value, function);
        schema.release_i32_local(answer, function);
        value.clear(function);
        argument.clear(function);
        Ok(())
    }

    fn emit_number_prototype_builtin(
        &mut self,
        operation: NumberPrototypeOperation,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let number = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        self.compile_this_to_locals(&receiver, function)?;
        number.copy_from(&receiver, function);
        receiver.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<PrimitiveBox>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let boxed = schema.reserve_gc_local(function).initialize(
            receiver.cast_reference::<PrimitiveBox>(schema, function),
            function,
        );
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PrimitiveBox>()
                .field(PrimitiveBoxSchema::PRIMITIVE)
                .read(&boxed, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &number, schema, function);
        stored.clear(function);
        boxed.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        number.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::NUMBER_PROTOTYPE_METHOD_REQUIRES_A_NUMBER_RECEIVER,
            &pending,
            function,
        )?;
        function.instruction(&Instruction::Else);
        match operation {
            NumberPrototypeOperation::ValueOf => pending.set_normal(&number, function),
            NumberPrototypeOperation::ToFixed => {
                self.emit_number_to_fixed_payload(number.scalar(), &pending, function)?
            }
            NumberPrototypeOperation::ToExponential => {
                self.emit_number_to_exponential_payload(number.scalar(), &pending, function)?
            }
            NumberPrototypeOperation::ToPrecision => {
                self.emit_number_to_precision_payload(number.scalar(), &pending, function)?
            }
            NumberPrototypeOperation::ToString => {
                self.emit_number_to_string_with_radix_result(number.scalar(), &pending, function)?
            }
            NumberPrototypeOperation::ToLocaleString => {
                self.emit_intrinsic_number_locale_format(&number, function)?;
                pending.copy_from(self.completion(), function);
            }
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&pending, function);
        pending.clear(function);
        number.clear(function);
        receiver.clear(function);
        Ok(())
    }
    pub(super) fn emit_number_is_integer_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_number_predicate(NumberPredicate::Integer, function)
    }
    pub(super) fn emit_number_is_safe_integer_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_number_predicate(NumberPredicate::SafeInteger, function)
    }
    pub(super) fn emit_number_is_finite_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_number_predicate(NumberPredicate::Finite, function)
    }
    pub(super) fn emit_number_is_nan_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_number_predicate(NumberPredicate::NaN, function)
    }
    pub(super) fn emit_number_prototype_to_exponential_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_number_prototype_builtin(NumberPrototypeOperation::ToExponential, function)
    }
    pub(super) fn emit_number_prototype_to_fixed_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_number_prototype_builtin(NumberPrototypeOperation::ToFixed, function)
    }
    pub(super) fn emit_number_prototype_to_precision_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_number_prototype_builtin(NumberPrototypeOperation::ToPrecision, function)
    }
    pub(super) fn emit_number_prototype_to_string_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_number_prototype_builtin(NumberPrototypeOperation::ToString, function)
    }
    pub(super) fn emit_number_prototype_to_locale_string_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_number_prototype_builtin(NumberPrototypeOperation::ToLocaleString, function)
    }
    pub(super) fn emit_number_prototype_value_of_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_number_prototype_builtin(NumberPrototypeOperation::ValueOf, function)
    }
}
