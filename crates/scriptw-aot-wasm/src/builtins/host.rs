//! Native host entries retain complete GC values through every observable call.
use super::super::*;
use crate::gc_types::*;
use crate::operations::PropertyKeyLocals;
use scriptw_ir::NativeErrorKind;

mod abstract_module_source;
mod agent;
mod assert_throws;
mod create_realm;
mod detach_array_buffer;
mod html_dda;
mod parse_float;
mod parse_int;
mod realm_eval_script;

impl FunctionBuilder<'_> {
    pub(crate) fn compile_host_print_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let entry = self
            .body_entry_locals()
            .expect("host callable owns its entry");
        let argc = entry.argument_count();
        let arguments = schema
            .reserve_gc_local(f)
            .initialize(entry.arguments().load(schema, f), f);
        let output = schema.reserve_completion(f);
        let pending = schema.reserve_completion(f);
        let argument = schema.reserve_value_local(f);
        let text = schema
            .reserve_gc_local::<StringValue, Nullable>(f)
            .initialize_null(schema, f);
        let index = schema.reserve_i32_local(f);
        output.initialize(f);
        let done = self.open_frame(ControlFrameKind::Block, f);

        argc.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        text.replace(self.emit_interned_string_reference("", f)?.nullable(), f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        index.set_constant(0, f);
        self.emit_argument_vector_entry_to_value(&arguments, index, &argument, f);
        self.emit_value_to_string_payload(&argument, &pending, f)?;
        self.emit_host_abrupt_exit(&pending, &output, done, f);
        text.replace(
            pending
                .value()
                .cast_reference::<StringValue>(schema, f)
                .nullable(),
            f,
        );
        // With one argument the converted String goes straight to the encoder.
        argc.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Eq);
        self.emit_branch_if_to_target(done, f);

        let space = schema
            .reserve_gc_local::<StringValue, NonNullable>(f)
            .initialize(self.emit_interned_string_reference(" ", f)?, f);
        index.set_constant(1, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        argc.load(f);
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_argument_vector_entry_to_value(&arguments, index, &argument, f);
        self.emit_value_to_string_payload(&argument, &pending, f)?;
        self.emit_host_abrupt_exit(&pending, &output, done, f);
        let converted = schema
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(schema, f), f);
        let prefix = schema
            .reserve_gc_local(f)
            .initialize(text.load(schema, f).require_non_null(f), f);
        prefix.replace(self.emit_concat_gc_strings(&prefix, &space, f), f);
        text.replace(
            self.emit_concat_gc_strings(&prefix, &converted, f)
                .nullable(),
            f,
        );
        prefix.clear(f);
        converted.clear(f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        space.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        output.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let printable = schema
            .reserve_gc_local(f)
            .initialize(text.load(schema, f).require_non_null(f), f);
        self.emit_host_print_string(&printable, f)?;
        printable.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        schema.release_i32_local(index, f);
        text.clear(f);
        argument.clear(f);
        pending.clear(f);
        output.clear(f);
        arguments.clear(f);
        Ok(())
    }

    pub(crate) fn compile_host_gc_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.functions
            .gc_host_imports()
            .get(GcHostImport::CollectGc)
            .ok_or_else(|| EmitError::unsupported("missing native collector import"))?
            .emit_call_instruction(f);
        self.completion().initialize(f);
        Ok(())
    }

    pub(crate) fn compile_host_is_constructor_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(f);
        let is_constructor = schema.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &value, f);
        self.emit_is_constructor_i32(&value, f);
        is_constructor.store(f);
        self.completion().initialize(f);
        self.completion().value().set_boolean(is_constructor, f);
        schema.release_i32_local(is_constructor, f);
        value.clear(f);
        Ok(())
    }

    pub(super) fn emit_host_abrupt_exit(
        &mut self,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) {
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        output.copy_from(pending, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }
}
