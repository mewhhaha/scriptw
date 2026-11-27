use super::finalization::cells::CellSelection;
use super::*;

// The runtime and all its programs share this private memory. This byte stays
// set once a successful constructor creates a resource in the Store.
const WEAK_ACTIVITY_ADDRESS: u32 = crate::heap::DEADLINE_POLL_ADDRESS + 1;
const _: () = assert!(WEAK_ACTIVITY_ADDRESS < crate::heap::STATIC_DATA_OFFSET);
const WEAK_ACTIVITY_BYTE: wasm_encoder::MemArg = wasm_encoder::MemArg {
    offset: WEAK_ACTIVITY_ADDRESS as u64,
    align: 0,
    // Runtime re-encoding remaps the emitter's private memory index.
    memory_index: 0,
};

impl FunctionBuilder<'_> {
    pub(crate) fn emit_record_weak_activity(&self, f: &mut Function) {
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Store8(WEAK_ACTIVITY_BYTE));
    }

    /// Called once after the script/module job and after each dequeued job.
    pub(crate) fn emit_finish_weak_job(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let exit = self.open_frame(ControlFrameKind::Block, f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Load8U(WEAK_ACTIVITY_BYTE));
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        self.weak_import(GcHostImport::WeakJobFinish)?
            .emit_call_instruction(f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        let next = s.reserve_gc_local(f).initialize(
            self.weak_import(GcHostImport::FinalizationNextRegistry)?
                .next_finalization_registry(s, f),
            f,
        );
        next.load(s, f).is_null(f);
        self.emit_branch_if_to_target(exit, f);
        let registry = s
            .reserve_gc_local(f)
            .initialize(next.load(s, f).require_non_null(f), f);
        s.field(FinalizationRegistryObjectSchema::CLEANUP_QUEUED)
            .read(&registry, s, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let cell =
            self.emit_find_finalization_cell(&registry, CellSelection::ClearedTarget, false, f)?;
        cell.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        s.field(FinalizationRegistryObjectSchema::CLEANUP_QUEUED)
            .write(&registry, GcOperand::constant(true), s, f);
        self.emit_enqueue_finalization_job(&registry, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cell.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        registry.clear(f);
        next.clear(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        Ok(())
    }
    pub(crate) fn emit_run_finalization_job(
        &mut self,
        registry: &GcLocal<FinalizationRegistryObject>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        s.field(FinalizationRegistryObjectSchema::CLEANUP_QUEUED)
            .write(registry, GcOperand::constant(false), s, f);
        let callback = s.reserve_value_local(f);
        let undefined = s.reserve_value_local(f);
        let holdings = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let stored = s.reserve_gc_local(f).initialize(
            s.field(FinalizationRegistryObjectSchema::CALLBACK)
                .read(registry, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&stored, &callback, s, f);
        stored.clear(f);
        undefined.set_undefined(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        // Remove one cell before its callback; re-read the list after user code.
        let selected =
            self.emit_find_finalization_cell(registry, CellSelection::ClearedTarget, true, f)?;
        selected.load(s, f).is_null(f);
        self.emit_branch_if_to_target(exit, f);
        let cell = s
            .reserve_gc_local(f)
            .initialize(selected.load(s, f).require_non_null(f), f);
        let holding = s.reserve_gc_local(f).initialize(
            s.field(FinalizationCellSchema::HOLDINGS)
                .read(&cell, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&holding, &holdings, s, f);
        holding.clear(f);
        cell.clear(f);
        selected.clear(f);
        let args = self.emit_pre_evaluated_arg_vector(&[&holdings], f);
        s.clear_throw_diagnostic(ThrowDiagnosticRole::Name, f);
        s.clear_throw_diagnostic(ThrowDiagnosticRole::Message, f);
        s.clear_throw_diagnostic(ThrowDiagnosticRole::ConstructorName, f);
        self.emit_function_or_proxy_call_with_argv(&callback, &undefined, &args, &pending, f)?;
        args.clear(f);
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let name = s.load_throw_diagnostic(ThrowDiagnosticRole::Name, f);
        let message = s.load_throw_diagnostic(ThrowDiagnosticRole::Message, f);
        self.weak_import(GcHostImport::ReportCleanupException)?
            .report_cleanup_exception(pending.value(), &name, &message, s, f);
        message.clear(f);
        name.clear(f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        holdings.set_undefined(f);
        pending.value().set_undefined(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        pending.clear(f);
        holdings.clear(f);
        undefined.clear(f);
        callback.clear(f);
        self.completion().initialize(f);
        Ok(())
    }
}
