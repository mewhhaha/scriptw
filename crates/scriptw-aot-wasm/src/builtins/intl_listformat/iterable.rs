use super::*;
use crate::control_flow::SyncIteratorConsumer;
use crate::functions::ArgumentListConstruction;
/// Only the successful complete walk can mint a List containing exclusively Strings.
pub(super) struct ObservedStringListLocals {
    values: GcLocal<ValueArray>,
    count: I32Local,
}
impl ObservedStringListLocals {
    pub(super) fn values(&self) -> &GcLocal<ValueArray> {
        &self.values
    }
    pub(super) fn count(&self) -> I32Local {
        self.count
    }
    fn clear(self, schema: &RuntimeSchema, f: &mut Function) {
        schema.release_i32_local(self.count, f);
        self.values.clear(f);
    }
}
impl FunctionBuilder<'_> {
    fn emit_list_of_strings(
        &mut self,
        f: &mut Function,
    ) -> Result<ObservedStringListLocals, EmitError> {
        let schema = self.runtime_schema();
        let source = schema.reserve_value_local(f);
        let value = schema.reserve_value_local(f);
        let done = schema.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &source, f);
        let list = ArgumentListConstruction::new(self, f);
        emit_tag_is(&source, WasmRuntimeValueTag::Undefined, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let iterator = self.emit_get_sync_iterator(&source, SyncIteratorConsumer::ListFormat, f)?;
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        // Cached next/done/value abrupts propagate directly without Close.
        self.emit_sync_iterator_step_value(&iterator, done, &value, f)?;
        done.load(f);
        self.emit_branch_if_to_target(exit, f);
        emit_tag_is(&value, WasmRuntimeValueTag::String, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        self.emit_throw_current_function_realm_type_error(LF_STRING_ERROR, &pending, f)?;
        let stored = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<IteratorRecord>()
                .field(IteratorRecordSchema::ITERATOR)
                .read(iterator.record(), schema, f)
                .reference(),
            f,
        );
        let receiver = schema.reserve_value_local(f);
        self.emit_stored_value_to_locals(&stored, &receiver, f);
        let closed = schema.reserve_completion(f);
        closed.initialize(f);
        self.emit_iterator_close_with_completion(&receiver, &pending, &closed, f)?;
        self.completion().copy_from(&closed, f);
        closed.clear(f);
        receiver.clear(f);
        stored.clear(f);
        pending.clear(f);
        self.emit_propagate_current_throw_if_needed(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        list.append(&value, schema, f);
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        iterator.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let values = list.finish(self, f);
        let count = schema.reserve_i32_local(f);
        schema.array_type::<ValueArray>().length(&values, schema, f);
        count.store(f);
        schema.release_i32_local(done, f);
        value.clear(f);
        source.clear(f);
        Ok(ObservedStringListLocals { values, count })
    }
    pub(crate) fn emit_intl_list_format(
        &mut self,
        f: &mut Function,
        mode: ListFormatOutput,
    ) -> Result<(), EmitError> {
        let record = self.emit_list_record_from_receiver(f)?;
        let input = self.emit_list_of_strings(f)?;
        self.emit_list_format_partition(&record, &input, mode, f)?;
        input.clear(self.runtime_schema(), f);
        record.clear(f);
        Ok(())
    }
}
