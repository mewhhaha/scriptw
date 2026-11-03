//! Suspended Identifier References remain native GC records, never JS values.

use super::*;
use crate::gc_types::{
    BindingCellTable, DeclarativeEnvironment, DeclarativeEnvironmentSchema,
    EnvironmentIdentifierReferenceRecord, EnvironmentIdentifierReferenceRecordSchema,
    InvocationFrame, InvocationFrameSchema,
};
use crate::WasmRuntimeValueTag;
use scriptw_ir::{
    CapturedIdentifierReferenceIr, IdentifierReferenceCaptureAccess,
    IdentifierReferenceCaptureDisposition, IdentifierReferenceCaptureIr,
    IdentifierReferenceFallbackDisposition,
};

/// Only a verified owned activation slot can retain a native Reference.
struct IdentifierReferenceCaptureSlot {
    slot: u32,
}

impl FunctionBuilder<'_> {
    /// Authenticate the finalized identity, then reuse the active storage's
    /// lexical hop adjustment. No value or initialization fact is inferred.
    fn module_cell_storage(
        &self,
        proof: &scriptw_ir::ProvenBindingCellIr,
        source_name: &str,
    ) -> Result<BindingStorage, EmitError> {
        let invalid = || {
            EmitError::unsupported(format!(
                "module cell proof for `{source_name}` ({}, slot {}) does not name this function's cell",
                proof.storage_name(), proof.slot()
            ))
        };
        if self.function_id.as_deref() != Some(proof.owner()) || proof.source_name() != source_name
        {
            return Err(invalid());
        }
        match proof.origin() {
            scriptw_ir::ProvenBindingCellOrigin::Owned => {
                if self
                    .captured_bindings
                    .iter()
                    .any(|binding| binding.name == proof.storage_name())
                {
                    return Err(invalid());
                }
            }
            scriptw_ir::ProvenBindingCellOrigin::Captured { entry_hops } => {
                if !self.captured_bindings.iter().any(|binding| {
                    binding.name == proof.storage_name()
                        && binding.source_name == source_name
                        && binding.slot == proof.slot()
                        && binding.hops == entry_hops
                }) {
                    return Err(invalid());
                }
            }
        }
        let storage = self
            .lookup_binding(proof.storage_name())
            .or_else(|| self.activation_owned_binding_storage(proof.storage_name()))
            .ok_or_else(invalid)?;
        match storage {
            BindingStorage::EnvSlot { slot, .. } if slot == proof.slot() => Ok(storage),
            _ => Err(invalid()),
        }
    }

    pub(crate) fn emit_read_module_cell_identifier(
        &mut self,
        proof: &scriptw_ir::ProvenBindingCellIr,
        source_name: &str,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let storage = self.module_cell_storage(proof, source_name)?;
        // The existing cell read follows TARGET and checks INITIALIZED. Only
        // its failing branch calls the shared lexical-TDZ error helper in R.
        self.read_binding_to_locals(storage, value, function)
    }

    pub(crate) fn emit_resolve_module_cell_identifier(
        &mut self,
        proof: &scriptw_ir::ProvenBindingCellIr,
        source_name: &str,
        name: &GcLocal<StringValue>,
        strictness: Strictness,
        function: &mut Function,
    ) -> Result<EnvironmentIdentifierReference, EmitError> {
        let storage = self.module_cell_storage(proof, source_name)?;
        self.emit_declarative_identifier_reference(name, strictness, storage, function)
    }

    fn identifier_reference_capture_slot(
        &self,
        reference: &CapturedIdentifierReferenceIr,
    ) -> Result<IdentifierReferenceCaptureSlot, EmitError> {
        let slot = self
            .owned_env_slot(reference.storage_name())
            .ok_or_else(|| {
                EmitError::unsupported(
                    "compiler invariant: Identifier Reference has no owned activation slot",
                )
            })?;
        if self
            .body_entry_locals()
            .and_then(|entry| entry.resume_frame())
            .is_none()
        {
            return Err(EmitError::unsupported(
                "compiler invariant: Identifier Reference has no resumable frame",
            ));
        }
        if !self.current_function_meta().is_some_and(|meta| {
            matches!(
                meta.protocol().execution_kind(),
                FunctionExecutionKind::Generator
                    | FunctionExecutionKind::Async
                    | FunctionExecutionKind::AsyncGenerator
            )
        }) {
            return Err(EmitError::unsupported(
                "compiler invariant: Identifier Reference capture requires a resumable function",
            ));
        }
        Ok(IdentifierReferenceCaptureSlot { slot })
    }

    fn identifier_reference_capture_cell(
        &mut self,
        capture: &IdentifierReferenceCaptureSlot,
        function: &mut Function,
    ) -> GcLocal<BindingCell> {
        let schema = self.runtime_schema();
        let frame = self
            .body_entry_locals()
            .and_then(|entry| entry.resume_frame())
            .expect("validated Reference capture owns its invocation frame");
        // The current lexical environment may be inside a suspended block.
        // This hidden cell belongs to the stable invocation environment.
        let environment = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<InvocationFrame>()
                .field(InvocationFrameSchema::INVOCATION_ENVIRONMENT)
                .read(frame, schema, function)
                .reference(),
            function,
        );
        let cell = self.emit_environment_cell_local(&environment, capture.slot, function);
        environment.clear(function);
        cell
    }

    pub(crate) fn emit_capture_identifier_reference(
        &mut self,
        capture: &IdentifierReferenceCaptureIr,
        name: &GcLocal<StringValue>,
        strictness: Strictness,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let slot = self.identifier_reference_capture_slot(capture.reference())?;
        match capture.disposition() {
            IdentifierReferenceCaptureDisposition::RuntimeEnvironment => {
                let reference =
                    self.emit_resolve_environment_identifier(name, strictness, function)?;
                self.emit_publish_identifier_reference(
                    &slot,
                    reference,
                    capture.access(),
                    value,
                    function,
                )?;
            }
            IdentifierReferenceCaptureDisposition::Located(fallback) => {
                let reference =
                    self.emit_located_identifier_reference(name, strictness, fallback, function)?;
                self.emit_publish_identifier_reference(
                    &slot,
                    reference,
                    capture.access(),
                    value,
                    function,
                )?;
            }
            IdentifierReferenceCaptureDisposition::WithObject {
                selection,
                fallback,
            } => {
                let selected = self.runtime_schema().reserve_value_local(function);
                self.compile_expr_to_value(selection, &selected, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                selected.tag().load(function);
                function.instruction(&Instruction::I32Const(
                    WasmRuntimeValueTag::Undefined as i32,
                ));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                let reference =
                    self.emit_located_identifier_reference(name, strictness, fallback, function)?;
                self.emit_publish_identifier_reference(
                    &slot,
                    reference,
                    capture.access(),
                    value,
                    function,
                )?;
                function.instruction(&Instruction::Else);
                let reference = self.emit_selected_with_object_identifier_reference(
                    name, &selected, strictness, function,
                );
                self.emit_publish_identifier_reference(
                    &slot,
                    reference,
                    capture.access(),
                    value,
                    function,
                )?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                selected.clear(function);
            }
        }
        Ok(())
    }

    fn emit_located_identifier_reference(
        &mut self,
        name: &GcLocal<StringValue>,
        strictness: Strictness,
        fallback: IdentifierReferenceFallbackDisposition<'_>,
        function: &mut Function,
    ) -> Result<EnvironmentIdentifierReference, EmitError> {
        match fallback {
            IdentifierReferenceFallbackDisposition::Global => {
                // Ordered with selection already ran. Begin the sole resolver
                // at the actual global record rather than traversing it again.
                self.emit_resolve_global_identifier(name, strictness, function)
            }
            IdentifierReferenceFallbackDisposition::Declarative { binding } => {
                let ExprIr::Identifier(storage_name) = &binding.expr else {
                    return Err(EmitError::unsupported(
                        "compiler invariant: captured declarative Reference has no binding identifier",
                    ));
                };
                let storage = self
                    .lookup_binding(storage_name)
                    .or_else(|| self.activation_owned_binding_storage(storage_name))
                    .ok_or_else(|| EmitError::unsupported(
                        "compiler invariant: suspended declarative Reference has no real BindingCell",
                    ))?;
                self.emit_declarative_identifier_reference(name, strictness, storage, function)
            }
        }
    }

    fn emit_declarative_identifier_reference(
        &mut self,
        name: &GcLocal<StringValue>,
        strictness: Strictness,
        storage: BindingStorage,
        function: &mut Function,
    ) -> Result<EnvironmentIdentifierReference, EmitError> {
        let BindingStorage::EnvSlot { slot, hops } = storage else {
            return Err(EmitError::unsupported(
                "compiler invariant: declarative Reference requires a real BindingCell",
            ));
        };
        let schema = self.runtime_schema();
        let environment = self.resolve_env_handle_local(hops, function);
        let actual = self.emit_environment_cell_local(&environment, slot, function);
        let reference = EnvironmentIdentifierReference {
            key: schema
                .reserve_gc_local::<StringValue, NonNullable>(function)
                .initialize(name.load(schema, function), function),
            kind: schema.reserve_i32_local(function),
            record: schema
                .reserve_gc_local::<Environment, Nullable>(function)
                .initialize_null(schema, function),
            entry: schema
                .reserve_gc_local::<NamedBinding, Nullable>(function)
                .initialize_null(schema, function),
            cell: schema
                .reserve_gc_local::<BindingCell, Nullable>(function)
                .initialize(actual.load(schema, function).nullable(), function),
            base: schema.reserve_value_local(function),
            strictness,
        };
        reference.base.set_undefined(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::DirectDeclarativeCell.code(),
        ));
        reference.kind.store(function);
        actual.clear(function);
        environment.clear(function);
        Ok(reference)
    }

    fn emit_publish_identifier_reference(
        &mut self,
        slot: &IdentifierReferenceCaptureSlot,
        reference: EnvironmentIdentifierReference,
        access: IdentifierReferenceCaptureAccess,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match access {
            IdentifierReferenceCaptureAccess::ReadBeforeRhs => {
                self.emit_environment_identifier_get(
                    &reference,
                    EnvironmentIdentifierRead::Value,
                    value,
                    function,
                )?;
            }
            IdentifierReferenceCaptureAccess::WriteOnly => value.set_undefined(function),
        }
        let schema = self.runtime_schema();
        let base = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(&reference.base, function),
                function,
            );
        let record = schema
            .reserve_gc_local::<EnvironmentIdentifierReferenceRecord, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<EnvironmentIdentifierReferenceRecord>()
                    .construct(
                        (
                            GcOperand::reference(&reference.key, schema),
                            GcOperand::i32_local(reference.kind),
                            GcOperand::reference(&reference.record, schema),
                            GcOperand::reference(&reference.entry, schema),
                            GcOperand::reference(&reference.cell, schema),
                            GcOperand::reference(&base, schema),
                        ),
                        function,
                    ),
                function,
            );
        let cell = self.identifier_reference_capture_cell(slot, function);
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::CAPTURED_IDENTIFIER_REFERENCE)
            .write(
                &cell,
                GcOperand::nullable_reference(&record, schema),
                schema,
                function,
            );
        cell.clear(function);
        record.clear(function);
        base.clear(function);
        self.release_environment_identifier_reference(reference, function);
        Ok(())
    }

    pub(crate) fn emit_take_captured_identifier_reference(
        &mut self,
        capture: &CapturedIdentifierReferenceIr,
        strictness: Strictness,
        function: &mut Function,
    ) -> Result<EnvironmentIdentifierReference, EmitError> {
        let slot = self.identifier_reference_capture_slot(capture)?;
        let schema = self.runtime_schema();
        let cell = self.identifier_reference_capture_cell(&slot, function);
        let record = schema
            .reserve_gc_local::<EnvironmentIdentifierReferenceRecord, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<BindingCell>()
                    .field(BindingCellSchema::CAPTURED_IDENTIFIER_REFERENCE)
                    .read(&cell, schema, function)
                    .reference()
                    .require_non_null(function),
                function,
            );
        // The restored fields remain rooted independently. Retire the saved
        // continuation before coercion or PutValue can throw or reenter.
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::CAPTURED_IDENTIFIER_REFERENCE)
            .write(&cell, GcOperand::null(schema), schema, function);
        cell.clear(function);
        let record_type = schema.struct_type::<EnvironmentIdentifierReferenceRecord>();
        let reference = EnvironmentIdentifierReference {
            key: schema.reserve_gc_local(function).initialize(
                record_type
                    .field(EnvironmentIdentifierReferenceRecordSchema::KEY)
                    .read(&record, schema, function)
                    .reference(),
                function,
            ),
            kind: schema.reserve_i32_local(function),
            record: schema.reserve_gc_local(function).initialize(
                record_type
                    .field(EnvironmentIdentifierReferenceRecordSchema::RECORD)
                    .read(&record, schema, function)
                    .reference(),
                function,
            ),
            entry: schema.reserve_gc_local(function).initialize(
                record_type
                    .field(EnvironmentIdentifierReferenceRecordSchema::ENTRY)
                    .read(&record, schema, function)
                    .reference(),
                function,
            ),
            cell: schema.reserve_gc_local(function).initialize(
                record_type
                    .field(EnvironmentIdentifierReferenceRecordSchema::CELL)
                    .read(&record, schema, function)
                    .reference(),
                function,
            ),
            base: schema.reserve_value_local(function),
            strictness,
        };
        record_type
            .field(EnvironmentIdentifierReferenceRecordSchema::KIND)
            .read(&record, schema, function)
            .store(reference.kind, function);
        let base = schema.reserve_gc_local(function).initialize(
            record_type
                .field(EnvironmentIdentifierReferenceRecordSchema::BASE)
                .read(&record, schema, function)
                .reference(),
            function,
        );
        self.emit_stored_value_to_locals(&base, &reference.base, function);
        base.clear(function);
        record.clear(function);
        Ok(reference)
    }

    pub(crate) fn emit_release_captured_identifier_reference(
        &mut self,
        capture: &CapturedIdentifierReferenceIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let slot = self.identifier_reference_capture_slot(capture)?;
        let schema = self.runtime_schema();
        let cell = self.identifier_reference_capture_cell(&slot, function);
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::CAPTURED_IDENTIFIER_REFERENCE)
            .write(&cell, GcOperand::null(schema), schema, function);
        cell.clear(function);
        Ok(())
    }

    fn identifier_reference_retirement_frame(&self) -> Option<&GcLocal<InvocationFrame>> {
        if self.owned_env_bindings.is_empty()
            || !self.current_function_meta().is_some_and(|meta| {
                matches!(
                    meta.protocol().execution_kind(),
                    FunctionExecutionKind::Generator
                        | FunctionExecutionKind::Async
                        | FunctionExecutionKind::AsyncGenerator
                )
            })
        {
            return None;
        }
        self.body_entry_locals()
            .and_then(|entry| entry.resume_frame())
    }

    pub(crate) fn emit_retire_abandoned_identifier_references_if_throw(
        &self,
        function: &mut Function,
    ) {
        if self.identifier_reference_retirement_frame().is_none() {
            return;
        }
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_retire_abandoned_identifier_references(function);
        function.instruction(&Instruction::End);
    }

    /// Source Throw/Return abandons every unfinished expression in this
    /// invocation. Clear only its private Reference edges before entering a
    /// handler or finalizer; ordinary Yield and pending Await never call this
    /// owner.
    pub(crate) fn emit_retire_abandoned_identifier_references(&self, function: &mut Function) {
        let Some(frame) = self.identifier_reference_retirement_frame() else {
            return;
        };
        let schema = self.runtime_schema();
        let environment = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<InvocationFrame>()
                    .field(InvocationFrameSchema::INVOCATION_ENVIRONMENT)
                    .read(frame, schema, function)
                    .reference(),
                function,
            );
        // Entry initialization can itself fail before it publishes the cells.
        environment.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let declarative = schema
            .reserve_gc_local::<DeclarativeEnvironment, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::DECLARATIVE)
                    .read(&environment, schema, function)
                    .reference(),
                function,
            );
        let cells = schema
            .reserve_gc_local::<BindingCellTable, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<DeclarativeEnvironment>()
                    .field(DeclarativeEnvironmentSchema::CELLS)
                    .read(&declarative, schema, function)
                    .reference(),
                function,
            );
        let index = schema.reserve_i32_local(function);
        let length = schema.reserve_i32_local(function);
        let table = schema.array_type::<BindingCellTable>();
        table.length(&cells, schema, function);
        length.store(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        let cell = schema
            .reserve_gc_local::<BindingCell, NonNullable>(function)
            .initialize(
                table.read(&cells, index, schema, function).reference(),
                function,
            );
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::CAPTURED_IDENTIFIER_REFERENCE)
            .write(&cell, GcOperand::null(schema), schema, function);
        cell.clear(function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i32_local(length, function);
        schema.release_i32_local(index, function);
        cells.clear(function);
        declarative.clear(function);
        function.instruction(&Instruction::End);
        environment.clear(function);
    }

    pub(super) fn emit_direct_identifier_cell_get(
        &mut self,
        reference: &EnvironmentIdentifierReference,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let cell = schema
            .reserve_gc_local::<BindingCell, NonNullable>(function)
            .initialize(
                reference
                    .cell
                    .load(schema, function)
                    .require_non_null(function),
                function,
            );
        let initialized = self.emit_read_environment_cell(&cell, value, function);
        initialized.load(function);
        schema.release_i32_local(initialized, function);
        cell.clear(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_global_binding_error(
            GlobalBindingFailure::BeforeInitialization,
            value,
            function,
        )?;
        self.emit_propagate_current_throw_if_needed(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(super) fn emit_direct_identifier_cell_put(
        &mut self,
        reference: &EnvironmentIdentifierReference,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let cell = schema
            .reserve_gc_local::<BindingCell, NonNullable>(function)
            .initialize(
                reference
                    .cell
                    .load(schema, function)
                    .require_non_null(function),
                function,
            );
        let error = schema.reserve_value_local(function);
        self.emit_check_environment_cell_initialized(&cell, &error, function)?;
        self.open_frame(ControlFrameKind::Block, function);
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::MUTABLE)
            .read(&cell, schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        if !reference.strictness.throws_on_failed_set() {
            schema
                .struct_type::<BindingCell>()
                .field(BindingCellSchema::IMMUTABLE_STRICT)
                .read(&cell, schema, function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::BrIf(1));
        }
        self.emit_environment_native_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::ASSIGNMENT_TO_CONSTANT_BINDING,
            &error,
            function,
        )?;
        self.emit_propagate_current_throw_if_needed(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_write_environment_cell(&cell, value, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        error.clear(function);
        cell.clear(function);
        Ok(())
    }
}
