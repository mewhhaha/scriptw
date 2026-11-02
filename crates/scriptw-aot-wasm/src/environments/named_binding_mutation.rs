use super::global_environment::GlobalBindingFailure;
use super::named_environment::*;
use super::*;
use crate::gc_types::{NamedBinding, NamedBindingSchema, NamedBindingTable, StringValue};

impl FunctionBuilder<'_> {
    /// CreateMutableBinding(name, true) then InitializeBinding. The grown name
    /// table retains every prior cell, including deletion and import identity.
    pub(crate) fn emit_create_eval_variable_binding(
        &mut self,
        environment: &GcLocal<Environment, Nullable>,
        key: &GcLocal<StringValue>,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let entry = schema
            .reserve_gc_local::<NamedBinding, Nullable>(function)
            .initialize_null(schema, function);
        self.emit_find_own_named_binding(environment, key, &entry, function);
        entry.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        let old_entries = schema
            .reserve_gc_local::<NamedBindingTable, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::NAMED_BINDINGS)
                    .read(environment, schema, function)
                    .reference(),
                function,
            );
        let old_count = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        old_count.store(function);
        old_entries.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .array_type::<NamedBindingTable>()
            .length(&old_entries, schema, function);
        old_count.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let new_count = schema.reserve_i32_local(function);
        old_count.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        new_count.store(function);
        let new_entries = schema
            .reserve_gc_local::<NamedBindingTable, NonNullable>(function)
            .initialize(
                schema.array_type::<NamedBindingTable>().filled(
                    GcOperand::null(schema),
                    new_count,
                    function,
                ),
                function,
            );
        let index = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let previous_entry = schema
            .reserve_gc_local::<NamedBinding, Nullable>(function)
            .initialize_null(schema, function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        old_count.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        previous_entry.replace(
            schema
                .array_type::<NamedBindingTable>()
                .read(&old_entries, index, schema, function)
                .reference(),
            function,
        );
        schema.array_type::<NamedBindingTable>().write(
            &new_entries,
            index,
            GcOperand::reference(&previous_entry, schema),
            schema,
            function,
        );
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        previous_entry.clear(function);
        schema.release_i32_local(index, function);
        let cell = self.emit_allocate_environment_cell(value, true, true, false, function);
        let new_entry = schema
            .reserve_gc_local::<NamedBinding, NonNullable>(function)
            .initialize(
                schema.struct_type::<NamedBinding>().construct(
                    (
                        GcOperand::reference(key, schema),
                        GcOperand::reference(&cell, schema),
                        GcOperand::boolean(true),
                        GcOperand::boolean(true),
                        GcOperand::boolean(false),
                    ),
                    function,
                ),
                function,
            );
        cell.clear(function);
        schema.array_type::<NamedBindingTable>().write(
            &new_entries,
            old_count,
            GcOperand::nullable_reference(&new_entry, schema),
            schema,
            function,
        );
        new_entry.clear(function);
        schema
            .struct_type::<Environment>()
            .field(EnvironmentSchema::NAMED_BINDINGS)
            .write(
                environment,
                GcOperand::nullable_reference(&new_entries, schema),
                schema,
                function,
            );
        new_entries.clear(function);
        schema.release_i32_local(new_count, function);
        schema.release_i32_local(old_count, function);
        old_entries.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        entry.clear(function);
        Ok(())
    }

    pub(crate) fn emit_set_named_environment_binding(
        &mut self,
        environment: &GcLocal<Environment, Nullable>,
        key: &GcLocal<StringValue>,
        strictness: Strictness,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let entry = schema
            .reserve_gc_local::<NamedBinding, Nullable>(function)
            .initialize_null(schema, function);
        self.emit_find_own_named_binding(environment, key, &entry, function);
        self.open_frame(ControlFrameKind::Block, function);
        entry.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        match strictness {
            Strictness::Strict => {
                self.emit_throw_global_binding_error(
                    GlobalBindingFailure::Unresolvable(key),
                    value,
                    function,
                )?;
                self.emit_propagate_current_throw_if_needed(function);
            }
            Strictness::Sloppy => {
                self.emit_create_eval_variable_binding(environment, key, value, function)?
            }
        }
        function.instruction(&Instruction::Br(1));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_check_named_binding_initialized(&entry, value, function)?;
        let cell = schema
            .reserve_gc_local::<BindingCell, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<NamedBinding>()
                    .field(NamedBindingSchema::CELL)
                    .read(&entry, schema, function)
                    .reference(),
                function,
            );
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::MUTABLE)
            .read(&cell, schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        if strictness == Strictness::Sloppy {
            schema
                .struct_type::<BindingCell>()
                .field(BindingCellSchema::IMMUTABLE_STRICT)
                .read(&cell, schema, function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::BrIf(1));
        }
        self.emit_throw_global_binding_error(GlobalBindingFailure::Immutable, value, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_write_environment_cell(&cell, value, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        cell.clear(function);
        entry.clear(function);
        Ok(())
    }
}
