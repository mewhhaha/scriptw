use super::super::*;
use crate::control_flow::SyncIteratorConsumer;
use crate::functions::{ErrorMessageConstructorKind, OrdinaryDefaultPrototype};
use crate::gc_types::*;
use crate::operations::PropertyKeyLocals;
pub(crate) use runtime_error::ActiveBuiltinRealmPrototype;
use scriptw_ir::NativeErrorKind;

mod aggregate_error_preparation;
mod constructor;
mod promise_any;
mod prototype_to_string;
mod runtime_error;
mod stack;

enum ErrorBuiltin {
    IsError,
    Constructor(NativeErrorKind),
    PrototypeToString,
}

enum ErrorCauseOptionsArgument {
    MessageError,
    AggregateError,
}
impl ErrorCauseOptionsArgument {
    const fn index(self) -> usize {
        match self {
            Self::MessageError => 1,
            Self::AggregateError => 2,
        }
    }
}

impl FunctionBuilder<'_> {
    fn emit_error_builtin(
        &mut self,
        builtin: ErrorBuiltin,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let result = schema.reserve_completion(function);
        result.initialize(function);
        match builtin {
            ErrorBuiltin::IsError => {
                let argument = schema.reserve_value_local(function);
                let is_error = schema.reserve_i32_local(function);
                self.emit_builtin_arg_to_value(0, &argument, function);
                // ECMAScript errors and Web IDL DOMExceptions carry ErrorData.
                // Proxy and ordinary objects cannot impersonate either brand.
                argument.reference().load(function);
                function.instruction(&Instruction::RefTestNonNull(
                    schema
                        .reference_type::<NativeErrorObject>(GcNullability::NonNullable)
                        .heap_type,
                ));
                argument.reference().load(function);
                function.instruction(&Instruction::RefTestNonNull(
                    schema
                        .reference_type::<DOMExceptionObject>(GcNullability::NonNullable)
                        .heap_type,
                ));
                function.instruction(&Instruction::I32Or);
                is_error.store(function);
                result.value().set_boolean(is_error, function);
                schema.release_i32_local(is_error, function);
                argument.clear(function);
            }
            ErrorBuiltin::Constructor(kind) => match kind {
                NativeErrorKind::AggregateError => {
                    let source = schema.reserve_value_local(function);
                    let message = schema.reserve_value_local(function);
                    self.emit_builtin_arg_to_value(0, &source, function);
                    self.emit_builtin_arg_to_value(1, &message, function);
                    let prototype = self.emit_error_constructor_prototype(
                        OrdinaryDefaultPrototype::AggregateError,
                        function,
                    )?;
                    let prepared = self.emit_prepare_aggregate_error_instance(
                        prototype.value(),
                        &message,
                        function,
                    )?;
                    let errors = self.emit_aggregate_error_iterable_to_list(&source, function)?;
                    let errors_value = schema.reserve_value_local(function);
                    errors_value.set_reference(&errors, schema, function);
                    self.emit_finish_aggregate_error_instance(
                        prepared,
                        &errors_value,
                        &result,
                        function,
                    )?;
                    errors_value.clear(function);
                    errors.clear(function);
                    prototype.clear(function);
                    message.clear(function);
                    source.clear(function);
                }
                NativeErrorKind::SuppressedError => {
                    let error = schema.reserve_value_local(function);
                    let suppressed = schema.reserve_value_local(function);
                    let message = schema.reserve_value_local(function);
                    self.emit_builtin_arg_to_value(0, &error, function);
                    self.emit_builtin_arg_to_value(1, &suppressed, function);
                    self.emit_builtin_arg_to_value(2, &message, function);
                    let prototype = self.emit_error_constructor_prototype(
                        OrdinaryDefaultPrototype::SuppressedError,
                        function,
                    )?;
                    let instance = self.emit_prepare_native_error_instance(
                        prototype.value(),
                        NativeErrorKind::SuppressedError,
                        function,
                    )?;
                    let converted = self.emit_install_optional_error_message(
                        instance.header(),
                        &message,
                        function,
                    )?;
                    self.emit_append_error_data(instance.header(), "error", &error, function)?;
                    self.emit_append_error_data(
                        instance.header(),
                        "suppressed",
                        &suppressed,
                        function,
                    )?;
                    self.emit_capture_error_stack_from_message(&instance, &converted, function)?;
                    converted.clear(function);
                    instance.publish(self, &result, function);
                    prototype.clear(function);
                    message.clear(function);
                    suppressed.clear(function);
                    error.clear(function);
                }
                NativeErrorKind::Error
                | NativeErrorKind::EvalError
                | NativeErrorKind::RangeError
                | NativeErrorKind::ReferenceError
                | NativeErrorKind::SyntaxError
                | NativeErrorKind::TypeError
                | NativeErrorKind::URIError => {
                    let kind = ErrorMessageConstructorKind::from_native_error_kind(kind)
                        .expect("the message constructor arm excludes distinct constructors");
                    self.emit_error_message_constructor(kind, &result, function)?;
                }
            },
            ErrorBuiltin::PrototypeToString => {
                self.emit_error_prototype_to_string(&result, function)?;
            }
        }
        self.completion().copy_from(&result, function);
        result.clear(function);
        Ok(())
    }

    pub(super) fn emit_error_constructor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_error_builtin(ErrorBuiltin::Constructor(NativeErrorKind::Error), function)
    }

    pub(super) fn emit_error_is_error_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_error_builtin(ErrorBuiltin::IsError, function)
    }

    pub(super) fn emit_eval_error_constructor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_error_builtin(
            ErrorBuiltin::Constructor(NativeErrorKind::EvalError),
            function,
        )
    }

    pub(super) fn emit_aggregate_error_constructor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_error_builtin(
            ErrorBuiltin::Constructor(NativeErrorKind::AggregateError),
            function,
        )
    }

    pub(super) fn emit_suppressed_error_constructor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_error_builtin(
            ErrorBuiltin::Constructor(NativeErrorKind::SuppressedError),
            function,
        )
    }

    pub(super) fn emit_range_error_constructor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_error_builtin(
            ErrorBuiltin::Constructor(NativeErrorKind::RangeError),
            function,
        )
    }

    pub(super) fn emit_syntax_error_constructor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_error_builtin(
            ErrorBuiltin::Constructor(NativeErrorKind::SyntaxError),
            function,
        )
    }

    pub(super) fn emit_type_error_constructor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_error_builtin(
            ErrorBuiltin::Constructor(NativeErrorKind::TypeError),
            function,
        )
    }

    pub(super) fn emit_uri_error_constructor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_error_builtin(
            ErrorBuiltin::Constructor(NativeErrorKind::URIError),
            function,
        )
    }

    pub(super) fn emit_reference_error_constructor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_error_builtin(
            ErrorBuiltin::Constructor(NativeErrorKind::ReferenceError),
            function,
        )
    }

    pub(super) fn emit_error_prototype_to_string_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_error_builtin(ErrorBuiltin::PrototypeToString, function)
    }

    fn emit_install_error_cause_from_arg(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        options_argument: ErrorCauseOptionsArgument,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let options = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(options_argument.index(), &options, function);
        self.emit_is_heap_object_like_tag_i32(options.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        let key = self.emit_error_property_key("cause", function)?;
        let has_cause = schema.reserve_i32_local(function);
        self.emit_object_has_property_i32(&options, &key, has_cause, function)?;
        has_cause.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let pending = schema.reserve_completion(function);
        self.emit_object_read_with_throw_routing(
            &options,
            &options,
            &key,
            &pending,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        self.emit_object_append_data_property_with_flags(
            header,
            &key,
            pending.value(),
            true,
            false,
            true,
            function,
        )?;
        pending.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(has_cause, function);
        key.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        options.clear(function);
        Ok(())
    }

    /// Resource disposal supplies already converted message and complete
    /// error/suppressed values. This allocator has no observable coercion.
    pub(crate) fn emit_alloc_suppressed_error_instance(
        &mut self,
        message: Option<&GcLocal<StringValue>>,
        error: &ValueLocals,
        suppressed: &ValueLocals,
        prototype: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let instance = self.emit_prepare_native_error_instance(
            prototype,
            NativeErrorKind::SuppressedError,
            function,
        )?;
        if let Some(message) = message {
            let value = schema.reserve_value_local(function);
            value.set_reference(message, schema, function);
            self.emit_append_error_data(instance.header(), "message", &value, function)?;
            self.emit_capture_error_stack_from_message(&instance, message, function)?;
            value.clear(function);
        }
        self.emit_append_error_data(instance.header(), "error", error, function)?;
        self.emit_append_error_data(instance.header(), "suppressed", suppressed, function)?;
        instance.publish(self, result, function);
        Ok(())
    }

    fn emit_error_property_key(
        &mut self,
        name: &str,
        function: &mut Function,
    ) -> Result<PropertyKeyLocals, EmitError> {
        let schema = self.runtime_schema();
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(name, function)?,
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &string, function);
        string.clear(function);
        Ok(key)
    }

    fn emit_append_error_data(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        name: &str,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let key = self.emit_error_property_key(name, function)?;
        self.emit_object_append_data_property_with_flags(
            header, &key, value, true, false, true, function,
        )?;
        key.clear(function);
        Ok(())
    }
}
