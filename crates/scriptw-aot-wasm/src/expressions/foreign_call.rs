//! A program-owned JS wrapper calls only its declared foreign import.
use super::*;
use crate::gc_types::{
    ByteArray, CodeUnitArray, FunctionObject, I64Local, PromiseCapability, PromiseCapabilitySchema,
    RuntimeSchema, StoredValue,
};
use crate::heap::PromiseSettlement;
use scriptw_ir::foreign::{
    ForeignCallIr, ForeignFunctionMode, ForeignResultType, ForeignValueType,
};

/// Snapshot owners keep references rooted until the native call completes.
enum ForeignArgument {
    Number(I64Local),
    StringUtf16(GcLocal<CodeUnitArray>),
    Bytes(GcLocal<ByteArray>),
}

/// Both resolver references come from the same intrinsic capability. Their
/// concrete function types guarantee the private import parameters are present
/// and callable before control reaches a native host implementation.
struct AsyncForeignCapability {
    capability: GcLocal<PromiseCapability>,
    resolve: GcLocal<FunctionObject>,
    reject: GcLocal<FunctionObject>,
}
impl AsyncForeignCapability {
    fn clear(self, function: &mut Function) {
        self.reject.clear(function);
        self.resolve.clear(function);
        self.capability.clear(function);
    }
}
impl ForeignArgument {
    fn load(&self, schema: &RuntimeSchema, function: &mut Function) {
        match self {
            Self::Number(bits) => {
                bits.load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
            }
            Self::StringUtf16(units) => {
                units.load(schema, function);
            }
            Self::Bytes(bytes) => {
                bytes.load(schema, function);
            }
        }
    }
    fn clear(self, function: &mut Function) {
        match self {
            Self::Number(_) => {}
            Self::StringUtf16(units) => units.clear(function),
            Self::Bytes(bytes) => bytes.clear(function),
        }
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_foreign_call(
        &mut self,
        call: &ForeignCallIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let planned = self
            .functions
            .foreign_imports()
            .function(call.function())?
            .clone();
        if call.operands().len() != planned.parameters.len() {
            return Err(EmitError::unsupported(
                "foreign wrapper operand arity differs from its declared import",
            ));
        }
        let error_tag = self.functions.foreign_imports().error_tag();
        let schema = self.runtime_schema();
        let mut arguments = Vec::with_capacity(call.operands().len());
        for operand in call.operands() {
            let value = schema.reserve_value_local(function);
            self.compile_expr_to_value(operand, &value, function)?;
            self.emit_propagate_current_throw_if_needed(function);
            arguments.push(value);
        }
        // Ordinary Call evaluated every actual JS argument, including extras.
        // Marshalling never applies ToNumber or invokes a user coercion hook.
        let mut native_arguments = Vec::with_capacity(arguments.len());
        for (argument, parameter) in arguments.iter().zip(&planned.parameters) {
            let native = match parameter {
                ForeignValueType::Number => {
                    argument.tag().load(function);
                    function
                        .instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
                    function.instruction(&Instruction::I32Ne);
                    self.open_frame(ControlFrameKind::If, function);
                    self.emit_expression_native_error(
                        NativeErrorKind::TypeError,
                        RuntimeErrorMessage::FOREIGN_NUMBER_ARGUMENT_REQUIRED,
                        function,
                    )?;
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                    ForeignArgument::Number(argument.scalar())
                }
                ForeignValueType::StringUtf16 => ForeignArgument::StringUtf16(
                    self.emit_foreign_string_argument(argument, function)?,
                ),
                ForeignValueType::Bytes => {
                    ForeignArgument::Bytes(self.emit_foreign_bytes_argument(argument, function)?)
                }
            };
            native_arguments.push(native);
        }
        let asynchronous = match planned.mode {
            ForeignFunctionMode::Synchronous => None,
            ForeignFunctionMode::Asynchronous => {
                Some(self.emit_foreign_async_capability(function)?)
            }
        };
        let done = error_tag.map(|tag| {
            function.instruction(&Instruction::Block(BlockType::Empty));
            let done = function.label_depth();
            function.instruction(&Instruction::Block(BlockType::Result(ValType::I32)));
            // The immediately enclosing result block is this catch's label 0.
            // Only the explicitly declared i32-code tag reaches that block.
            function.instruction(&Instruction::TryTable(
                BlockType::Empty,
                vec![tag.catch(0)].into(),
            ));
            done
        });
        for argument in &native_arguments {
            argument.load(schema, function);
        }
        if let Some(asynchronous) = &asynchronous {
            asynchronous.resolve.load(schema, function);
            asynchronous.reject.load(schema, function);
            planned.index.emit_call(function);
        } else {
            match planned.result {
                ForeignResultType::Number => {
                    planned.index.emit_call(function);
                    let bits = schema.reserve_i64_local(function);
                    function.instruction(&Instruction::I64ReinterpretF64);
                    bits.store(function);
                    output.set_number(bits, function);
                    schema.release_i64_local(bits, function);
                }
                ForeignResultType::StringUtf16 => {
                    let units = schema
                        .reserve_gc_local(function)
                        .initialize(planned.call_string_utf16(function)?, function);
                    self.emit_foreign_string_result(&units, output, function)?;
                    units.clear(function);
                }
                ForeignResultType::Bytes => {
                    let bytes = schema
                        .reserve_gc_local(function)
                        .initialize(planned.call_bytes(function)?, function);
                    self.emit_foreign_bytes_result(&bytes, output, function)?;
                    bytes.clear(function);
                }
                ForeignResultType::Void => {
                    planned.index.emit_call(function);
                    output.set_undefined(function);
                }
            }
        }
        if let Some(done) = done {
            function.branch_to_label(done);
            function.instruction(&Instruction::End); // try_table
            function.instruction(&Instruction::Unreachable); // only catch supplies i32
            function.instruction(&Instruction::End); // catch result block
            let code = schema.reserve_i32_local(function);
            code.store(function);
            self.emit_foreign_error_code(
                code,
                asynchronous
                    .as_ref()
                    .map(|capability| &capability.capability),
                function,
            )?;
            schema.release_i32_local(code, function);
            function.instruction(&Instruction::End); // successful-call block
        }
        if let Some(asynchronous) = asynchronous {
            self.emit_read_promise_capability_promise(&asynchronous.capability, output, function);
            asynchronous.clear(function);
        }
        for argument in native_arguments {
            argument.clear(function);
        }
        for argument in arguments {
            argument.clear(function);
        }
        Ok(())
    }

    fn emit_foreign_async_capability(
        &mut self,
        function: &mut Function,
    ) -> Result<AsyncForeignCapability, EmitError> {
        let constructor = self.emit_current_function_realm_intrinsic_promise_constructor(function);
        let capability = self
            .emit_new_current_function_realm_intrinsic_promise_capability(constructor, function)?;
        let schema = self.runtime_schema();
        let mut resolver = |field| {
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<PromiseCapability>()
                    .field(field)
                    .read(&capability, schema, function)
                    .reference(),
                function,
            );
            let value = schema.reserve_value_local(function);
            schema
                .struct_type::<StoredValue>()
                .read_into(&stored, &value, schema, function);
            let result = schema.reserve_gc_local(function).initialize(
                value.cast_reference::<FunctionObject>(schema, function),
                function,
            );
            value.clear(function);
            stored.clear(function);
            result
        };
        let resolve = resolver(PromiseCapabilitySchema::RESOLVE);
        let reject = resolver(PromiseCapabilitySchema::REJECT);
        Ok(AsyncForeignCapability {
            capability,
            resolve,
            reject,
        })
    }

    fn emit_foreign_error_code(
        &mut self,
        code: crate::gc_types::I32Local,
        asynchronous: Option<&GcLocal<PromiseCapability>>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        self.emit_throw_runtime_error(
            NativeErrorKind::Error,
            RuntimeErrorMessage::FOREIGN_HOST_OPERATION_FAILED,
            &pending,
            function,
        )?;
        let code_value = schema.reserve_value_local(function);
        let bits = schema.reserve_i64_local(function);
        code.load(function);
        function.instruction(&Instruction::F64ConvertI32S);
        function.instruction(&Instruction::I64ReinterpretF64);
        bits.store(function);
        code_value.set_number(bits, function);
        schema.release_i64_local(bits, function);
        let text = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("code", function)?,
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &text, function);
        let definition = schema.reserve_completion(function);
        definition.initialize(function);
        self.emit_create_data_property_or_throw(
            pending.value(),
            &key,
            &code_value,
            &definition,
            function,
        )?;
        self.completion().copy_from(&definition, function);
        self.emit_propagate_current_throw_if_needed(function);
        key.clear(function);
        text.clear(function);
        code_value.clear(function);
        definition.clear(function);
        if let Some(capability) = asynchronous {
            // A declared operation failure during async startup rejects the
            // same Promise returned on successful startup. Unrelated traps
            // never reach this catch and remain fatal.
            let rejected = schema.reserve_completion(function);
            rejected.initialize(function);
            self.emit_call_promise_capability(
                capability,
                PromiseSettlement::Reject,
                pending.value(),
                &rejected,
                function,
            )?;
            self.completion().copy_from(&rejected, function);
            rejected.clear(function);
        } else {
            self.completion().copy_from(&pending, function);
        }
        pending.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }
}
