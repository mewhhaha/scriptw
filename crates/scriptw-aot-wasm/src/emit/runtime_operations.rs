//! Actual shared runtime-operation compiler bodies.

use super::*;
use crate::runtime_helpers::{HelperParameters, StringEqualityParameters};

impl<'a> FunctionBuilder<'a> {
    /// Compiles the registered typed Call kernel with whole values, a non-null argument list and the trusted caller Environment.
    pub(super) fn compile_function_call_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::FunctionCall);
        let parameters =
            self.helper_parameters::<crate::runtime_helpers::FunctionCallParameters>(&mut function);
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_plain_function_call_dispatch(
            &parameters.callee,
            &parameters.this_value,
            &parameters.arguments,
            &result,
            &mut function,
        )?;
        result.emit(&mut function);
        result.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Compiles the registered raw-key Get kernel; the actual key conversion and complete abrupt result remain in this body.
    pub(super) fn compile_dynamic_property_read_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::DynamicPropertyRead);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::DynamicPropertyReadParameters>(
                &mut function,
            );
        let result = self.runtime_schema().reserve_completion(&mut function);
        let exit = self.open_frame(ControlFrameKind::Block, &mut function);
        self.compile_nullish_tagged_i32(parameters.target.tag(), &mut function)?;
        self.open_frame(ControlFrameKind::If, &mut function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_READ_PROPERTIES_OF_NULL_OR_UNDEFINED,
            &result,
            &mut function,
        )?;
        self.emit_branch_to_target(exit, &mut function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let found = self.emit_raw_numeric_array_own_data_read(
            &parameters.target,
            &parameters.key,
            &result,
            &mut function,
        );
        found.load(&mut function);
        self.runtime_schema()
            .release_i32_local(found, &mut function);
        self.emit_branch_if_to_target(exit, &mut function);
        self.emit_value_to_property_key_completion(&parameters.key, &result, &mut function)?;
        result.kind().load(&mut function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, &mut function);
        self.emit_branch_to_target(exit, &mut function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // Only a normal String/Symbol conversion reaches the sole checked mint.
        let key = self.emit_value_to_property_key_locals(result.value(), &mut function)?;
        self.emit_dynamic_property_read_with_key_locals(
            &parameters.target,
            &parameters.receiver,
            &key,
            &result,
            &mut function,
        )?;
        key.clear(&mut function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        result.emit(&mut function);
        result.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Compiles the physical Proxy Call kernel once; recursive dispatch uses its registered declaration and preserves whole completion.
    pub(super) fn compile_proxy_call_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::ProxyCall);
        let parameters =
            self.helper_parameters::<crate::runtime_helpers::ProxyCallParameters>(&mut function);
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_proxy_call_dispatch(
            &parameters.callee,
            &parameters.this_value,
            &parameters.arguments,
            &result,
            &mut function,
        )?;
        result.emit(&mut function);
        result.clear(&mut function);
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Compiles the physical Proxy Construct kernel with whole callee/newTarget values and the actual argument list.
    pub(super) fn compile_proxy_construct_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::ProxyConstruct);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::ProxyConstructParameters>(&mut function);
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_proxy_construct_dispatch(
            &parameters.callee,
            &parameters.new_target,
            &parameters.arguments,
            &result,
            &mut function,
        )?;
        result.emit(&mut function);
        result.clear(&mut function);
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Compares rooted UTF-16 strings through the registered pure I32 helper
    /// signature. ASCII folding changes only A through Z code units.
    pub(super) fn compile_string_equality_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::StringEquality);
        let schema = self.runtime_schema();
        let parameters = self.helper_parameters::<StringEqualityParameters>(&mut function);
        let result = schema.reserve_i32_local(&mut function);
        self.emit_gc_string_equality(
            &parameters.left,
            &parameters.right,
            parameters.ascii_fold,
            result,
            &mut function,
        );
        result.load(&mut function);
        schema.release_i32_local(result, &mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Compiles the shared Number-to-String helper.
    pub(super) fn compile_number_to_string_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::NumberToString);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::NumberToStringParameters>(&mut function);
        let result = self.emit_number_to_string_payload(parameters.bits, &mut function)?;
        parameters.release(&mut function);
        let _result = result;
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Compiles the registered pure GC UTF-16 String-to-Number kernel and emits its sole Number-bits result.
    pub(super) fn compile_string_to_number_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::StringToNumber);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::StringToNumberParameters>(&mut function);
        let result = self.runtime_schema().reserve_i64_local(&mut function);
        self.emit_string_to_number_payload(&parameters.input, result, &mut function)?;
        result.load(&mut function);
        self.runtime_schema()
            .release_i64_local(result, &mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Compiles observable ToPrimitive followed by primitive ToString, returning the complete result or original Throw.
    pub(super) fn compile_value_to_string_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::ValueToString);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::ValueToStringParameters>(&mut function);
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_value_to_string_payload(&parameters.input, &result, &mut function)?;
        result.emit(&mut function);
        result.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Compiles whole-value ToNumber with its trusted caller Environment and complete conversion result.
    pub(super) fn compile_value_to_number_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::ValueToNumber);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::ValueToNumberParameters>(&mut function);
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_value_to_number_payload(&parameters.input, &result, &mut function)?;
        result.emit(&mut function);
        result.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Compiles whole-value ToNumeric, preserving the resulting Number/BigInt value and any original Throw.
    pub(super) fn compile_value_to_numeric_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::ValueToNumeric);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::ValueToNumericParameters>(&mut function);
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_value_to_numeric_locals(&parameters.input, &result, &mut function)?;
        result.emit(&mut function);
        result.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Three registered hint-specific entries consume their actual typed
    /// parameter owners and return the complete conversion completion.
    pub(super) fn compile_value_to_primitive_helper(
        &mut self,
        hint: ToPrimitiveHint,
    ) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::helper_for(hint));
        let schema = self.runtime_schema();
        let result = schema.reserve_completion(&mut function);
        match hint {
            ToPrimitiveHint::Default => {
                let parameters = self
                    .helper_parameters::<crate::runtime_helpers::ValueToPrimitiveDefaultParameters>(
                        &mut function,
                    );
                self.emit_tagged_to_primitive_locals_pending_inner(
                    hint,
                    &parameters.input,
                    &result,
                    &mut function,
                )?;
                result.emit(&mut function);
                parameters.release(&mut function);
            }
            ToPrimitiveHint::Number => {
                let parameters = self
                    .helper_parameters::<crate::runtime_helpers::ValueToPrimitiveNumberParameters>(
                        &mut function,
                    );
                self.emit_tagged_to_primitive_locals_pending_inner(
                    hint,
                    &parameters.input,
                    &result,
                    &mut function,
                )?;
                result.emit(&mut function);
                parameters.release(&mut function);
            }
            ToPrimitiveHint::String => {
                let parameters = self
                    .helper_parameters::<crate::runtime_helpers::ValueToPrimitiveStringParameters>(
                        &mut function,
                    );
                self.emit_tagged_to_primitive_locals_pending_inner(
                    hint,
                    &parameters.input,
                    &result,
                    &mut function,
                )?;
                result.emit(&mut function);
                parameters.release(&mut function);
            }
        }
        result.clear(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    pub(super) fn compile_value_to_property_key_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::ValueToPropertyKey);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::ValueToPropertyKeyParameters>(
                &mut function,
            );
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_value_to_property_key_completion_inner(
            &parameters.input,
            &result,
            &mut function,
        )?;
        result.emit(&mut function);
        result.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }
}
