//! Native buffers and views publish complete concrete GC records.
use super::super::*;
use crate::functions::OrdinaryDefaultPrototype;
use crate::gc_types::*;
use crate::operations::PropertyKeyLocals;

mod array_buffer;
mod backing;
mod data_view;
mod float16;
mod foreign_value;
mod module_value;
mod typed_array;

pub(in crate::builtins) use backing::BufferAccess;

#[derive(Clone, Copy)]
pub(in crate::builtins) enum BufferConstructorKind {
    ArrayBuffer,
    SharedArrayBuffer,
}
#[derive(Clone, Copy)]
pub(in crate::builtins) enum ArrayBufferAccessor {
    ByteLength,
    MaxByteLength,
    Resizable,
    Immutable,
    Detached,
}
#[derive(Clone, Copy)]
pub(in crate::builtins) enum SharedArrayBufferAccessor {
    ByteLength,
    MaxByteLength,
    Growable,
}
#[derive(Clone, Copy)]
pub(in crate::builtins) enum BufferSliceKind {
    ArrayBuffer,
    SharedArrayBuffer,
    Immutable,
}
#[derive(Clone, Copy)]
pub(in crate::builtins) enum BufferTransferKind {
    PreserveResizable,
    FixedLength,
    Immutable,
}
#[derive(Clone, Copy)]
pub(in crate::builtins) enum DataViewAccessor {
    Buffer,
    ByteLength,
    ByteOffset,
}
/// The `accessMode` of TypedArrayCreateFromConstructor's ValidateTypedArray:
/// a result that the caller only reads (`subarray`) may be backed by an
/// Immutable ArrayBuffer, while one the caller writes into may not.
#[derive(Clone, Copy)]
pub(in crate::builtins) enum TypedArrayCreateAccess {
    Read,
    ReadWrite,
}
#[derive(Clone, Copy)]
pub(in crate::builtins) enum TypedArrayAccessorKind {
    ByteLength,
    ByteOffset,
    Length,
}

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn emit_binary_abrupt_exit(
        &mut self,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) {
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(COMPLETION_KIND_THROW as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        output.copy_from(pending, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    pub(in crate::builtins) fn emit_binary_type_error(
        &mut self,
        message: RuntimeErrorMessage,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_throw_runtime_error(scriptw_ir::NativeErrorKind::TypeError, message, output, f)?;
        self.emit_branch_to_target(exit, f);
        Ok(())
    }
    pub(in crate::builtins) fn emit_binary_range_error(
        &mut self,
        message: RuntimeErrorMessage,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_throw_runtime_error(scriptw_ir::NativeErrorKind::RangeError, message, output, f)?;
        self.emit_branch_to_target(exit, f);
        Ok(())
    }

    pub(in crate::builtins) fn emit_binary_require_ref<
        T: JavaScriptReference + GcStructHeapType,
    >(
        &mut self,
        input: &ValueLocals,
        message: RuntimeErrorMessage,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<T>, EmitError> {
        let s = self.runtime_schema();
        input.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<T>(GcNullability::NonNullable).heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(message, output, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(s.reserve_gc_local(f)
            .initialize(input.cast_reference::<T>(s, f), f))
    }

    fn emit_binary_require_new_target(
        &mut self,
        new_target: &ValueLocals,
        message: RuntimeErrorMessage,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        new_target.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(message, output, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    pub(in crate::builtins) fn emit_binary_string_key(
        &mut self,
        spelling: &str,
        f: &mut Function,
    ) -> Result<PropertyKeyLocals, EmitError> {
        let s = self.runtime_schema();
        let string = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference(spelling, f)?, f);
        let key = PropertyKeyLocals::from_string(s, &string, f);
        string.clear(f);
        Ok(key)
    }

    /// Relative bounds retain ToIntegerOrInfinity as Number bits until their
    /// finite clamping is complete. No trapping cast precedes the clamp.
    pub(in crate::builtins) fn emit_binary_relative_index(
        &mut self,
        input: &ValueLocals,
        length: I64Local,
        default_length: bool,
        index: I64Local,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        input.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        if default_length {
            length.load(f);
        } else {
            f.instruction(&Instruction::I64Const(0));
        }
        index.store(f);
        f.instruction(&Instruction::Else);
        self.emit_value_to_number_payload(input, pending, f)?;
        self.emit_binary_abrupt_exit(pending, output, exit, f);
        let integer = s.reserve_i64_local(f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            integer,
            f,
        );
        integer.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Lt);
        self.open_frame(ControlFrameKind::If, f);
        integer.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::F64Add);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Max);
        f.instruction(&Instruction::I64TruncSatF64U);
        index.store(f);
        f.instruction(&Instruction::Else);
        integer.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::F64Min);
        f.instruction(&Instruction::I64TruncSatF64U);
        index.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(integer, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    pub(in crate::builtins) fn emit_detach_array_buffer(
        &mut self,
        input: &ValueLocals,
        key: &ValueLocals,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        result.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let buffer = self.emit_binary_require_ref::<ArrayBuffer>(
            input,
            RuntimeErrorMessage::DETACHARRAYBUFFER_EXPECTS_AN_ARRAYBUFFER,
            result,
            exit,
            f,
        )?;
        let stored = s.reserve_gc_local(f).initialize(
            s.field(ArrayBufferSchema::DETACH_KEY)
                .read(&buffer, s, f)
                .reference(),
            f,
        );
        let expected = s.reserve_value_local(f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, &expected, s, f);
        self.emit_tagged_payload_same_value_i32(&expected, key, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::DETACHARRAYBUFFER_KEY_DOES_NOT_MATCH_THE_ARRAYBUFFER_DETACH_KEY,
            result,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.field(ArrayBufferSchema::BYTES)
            .write(&buffer, GcOperand::null(s), s, f);
        s.field(ArrayBufferSchema::BYTE_LENGTH)
            .write(&buffer, GcOperand::i64(0), s, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        expected.clear(f);
        stored.clear(f);
        buffer.clear(f);
        Ok(())
    }
}
