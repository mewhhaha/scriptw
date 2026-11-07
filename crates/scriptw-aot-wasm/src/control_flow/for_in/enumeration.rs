//! The sole native ForIn creation/advance algorithm, shared by eager and resumable consumers.

use super::*;
use crate::gc_types::{ForInEnumerationRecord, ForInEnumerationRecordSchema, SetObject};
use crate::runtime_helpers::{
    ForInAdvanceArguments, ForInAdvanceParameters, HelperParameters, RuntimeHelperId,
};

/// Only String keys enter this private, unpublished canonical Set. Its identity
/// stays rooted in the cursor while the collection owner grows its storage.
struct ForInVisitedKeys {
    set: GcLocal<SetObject>,
}

impl ForInVisitedKeys {
    fn new(builder: &mut FunctionBuilder<'_>, function: &mut Function) -> Result<Self, EmitError> {
        Ok(Self {
            set: builder.emit_private_string_set(function)?,
        })
    }

    fn contains(
        &self,
        key: &GcLocal<StringValue>,
        found: I32Local,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        builder.emit_private_string_set_has(&self.set, key, found, function)
    }

    fn insert(
        &self,
        key: &GcLocal<StringValue>,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        builder.emit_private_string_set_add(&self.set, key, function)
    }

    fn clear(self, function: &mut Function) {
        self.set.clear(function);
    }
}

impl FunctionBuilder<'_> {
    /// Box a nonnull source once; nullish sources publish a completed cursor.
    /// No prototype-level OwnPropertyKeys observation occurs during creation.
    pub(crate) fn emit_create_for_in_enumerator(
        &mut self,
        source: &ValueLocals,
        function: &mut Function,
    ) -> Result<GcLocal<ForInEnumerationRecord>, EmitError> {
        let schema = self.runtime_schema();
        let current = schema.reserve_value_local(function);
        current.set_scalar(ScalarValue::Null, function);
        let visited = ForInVisitedKeys::new(self, function)?;
        self.compile_nullish_tagged_i32(source.tag(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let boxed = schema.reserve_completion(function);
        self.emit_value_to_object_locals(source, &boxed, function)?;
        self.completion().copy_from(&boxed, function);
        current.copy_from(boxed.value(), function);
        boxed.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&current, function),
            function,
        );
        let record = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<ForInEnumerationRecord>().construct(
                (
                    GcOperand::reference(&stored, schema),
                    GcOperand::null(schema),
                    GcOperand::i32(0),
                    GcOperand::reference(&visited.set, schema),
                ),
                function,
            ),
            function,
        );
        stored.clear(function);
        current.clear(function);
        visited.clear(function);
        Ok(record)
    }

    /// Source consumers retain their StatementList completion while R advances
    /// the cursor. Only an abrupt result replaces the caller's completion.
    pub(crate) fn emit_advance_for_in_enumerator(
        &mut self,
        record: &GcLocal<ForInEnumerationRecord>,
        output: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let realm = self.emit_execution_realm(function);
        schema
            .call_helper(
                ForInAdvanceArguments::new(
                    record,
                    &realm,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(output, function);
        realm.clear(function);
        output.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(output, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_for_in_advance_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::ForInAdvance);
        let parameters = self.helper_parameters::<ForInAdvanceParameters>(&mut function);
        self.completion().initialize(&mut function);
        let output = self.runtime_schema().reserve_completion(&mut function);
        self.emit_advance_for_in_enumerator_inner(&parameters.record, &output, &mut function)?;
        output.emit(&mut function);
        output.clear(&mut function);
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// The sole native enumerator advance. Normal Undefined means completion;
    /// Normal String is the next key. User observations keep their whole Throw.
    fn emit_advance_for_in_enumerator_inner(
        &mut self,
        record: &GcLocal<ForInEnumerationRecord>,
        output: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let current = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let keys_length = schema.reserve_i32_local(function);
        let key_index = schema.reserve_i32_local(function);
        let found = schema.reserve_i32_local(function);
        let should_yield = schema.reserve_i32_local(function);
        output.initialize(function);
        let visited = ForInVisitedKeys {
            set: schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<ForInEnumerationRecord>()
                    .field(ForInEnumerationRecordSchema::VISITED_SET)
                    .read(record, schema, function)
                    .reference(),
                function,
            ),
        };
        let complete = self.open_frame(ControlFrameKind::Block, function);
        let prototype_loop = self.open_frame(ControlFrameKind::Loop, function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ForInEnumerationRecord>()
                .field(ForInEnumerationRecordSchema::CURRENT)
                .read(record, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &current, schema, function);
        stored.clear(function);
        current.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        function.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(complete, function);

        let nullable_keys = schema
            .reserve_gc_local::<PropertyKeyTable, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<ForInEnumerationRecord>()
                    .field(ForInEnumerationRecordSchema::REMAINING_KEYS)
                    .read(record, schema, function)
                    .reference(),
                function,
            );
        nullable_keys.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        let saved = self.save_statement_list_value(function);
        let keys = self
            .emit_object_own_property_keys(&current, function)?
            .into_table();
        schema
            .struct_type::<ForInEnumerationRecord>()
            .field(ForInEnumerationRecordSchema::REMAINING_KEYS)
            .write(
                record,
                GcOperand::nullable_reference(&keys, schema),
                schema,
                function,
            );
        nullable_keys.replace(keys.load(schema, function).nullable(), function);
        keys.clear(function);
        schema
            .struct_type::<ForInEnumerationRecord>()
            .field(ForInEnumerationRecordSchema::NEXT_KEY)
            .write(record, GcOperand::i32(0), schema, function);
        self.restore_statement_list_value(saved, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let keys = schema.reserve_gc_local(function).initialize(
            nullable_keys
                .load(schema, function)
                .require_non_null(function),
            function,
        );
        nullable_keys.clear(function);
        schema
            .array_type::<PropertyKeyTable>()
            .length(&keys, schema, function);
        keys_length.store(function);
        schema
            .struct_type::<ForInEnumerationRecord>()
            .field(ForInEnumerationRecordSchema::NEXT_KEY)
            .read(record, schema, function)
            .store(key_index, function);
        let keys_exhausted = self.open_frame(ControlFrameKind::Block, function);
        let key_loop = self.open_frame(ControlFrameKind::Loop, function);
        key_index.load(function);
        keys_length.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(keys_exhausted, function);
        let key = self.emit_property_key_table_entry(&keys, key_index, function)?;
        key_index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        key_index.store(function);
        // Commit advancement before descriptor traps or a selected body can run.
        schema
            .struct_type::<ForInEnumerationRecord>()
            .field(ForInEnumerationRecordSchema::NEXT_KEY)
            .write(record, GcOperand::i32_local(key_index), schema, function);
        function.instruction(&Instruction::I32Const(0));
        should_yield.store(function);
        let saved = self.save_statement_list_value(function);
        let next_property = self.open_frame(ControlFrameKind::Block, function);
        key.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(next_property, function);
        let string = schema.reserve_gc_local(function).initialize(
            key.value().cast_reference::<StringValue>(schema, function),
            function,
        );
        visited.contains(&string, found, self, function)?;
        found.load(function);
        self.emit_branch_if_to_target(next_property, function);
        self.emit_own_property_enumerability(&current, &key, found, should_yield, function)?;
        found.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(next_property, function);
        // Missing properties do not shadow a prototype; nonenumerable existing
        // properties do. Their getters never run during this observation.
        visited.insert(&string, self, function)?;
        string.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.restore_statement_list_value(saved, function)?;
        should_yield.load(function);
        self.open_frame(ControlFrameKind::If, function);
        output.set_normal(key.value(), function);
        self.emit_branch_to_target(complete, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        key.clear(function);
        self.emit_branch_to_target(key_loop, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        keys.clear(function);

        let saved = self.save_statement_list_value(function);
        self.emit_object_get_prototype_of(&current, &pending, function)?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.restore_statement_list_value(saved, function)?;
        let next = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(pending.value(), function),
            function,
        );
        schema
            .struct_type::<ForInEnumerationRecord>()
            .field(ForInEnumerationRecordSchema::CURRENT)
            .write(
                record,
                GcOperand::reference(&next, schema),
                schema,
                function,
            );
        next.clear(function);
        schema
            .struct_type::<ForInEnumerationRecord>()
            .field(ForInEnumerationRecordSchema::REMAINING_KEYS)
            .write(record, GcOperand::null(schema), schema, function);
        schema
            .struct_type::<ForInEnumerationRecord>()
            .field(ForInEnumerationRecordSchema::NEXT_KEY)
            .write(record, GcOperand::i32(0), schema, function);
        self.emit_branch_to_target(prototype_loop, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        visited.clear(function);
        schema.release_i32_local(should_yield, function);
        schema.release_i32_local(found, function);
        schema.release_i32_local(key_index, function);
        schema.release_i32_local(keys_length, function);
        pending.clear(function);
        current.clear(function);
        Ok(())
    }
}
