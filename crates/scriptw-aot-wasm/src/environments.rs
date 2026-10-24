use super::*;
use crate::functions::NonArrayRealmIntrinsicSlot;
use crate::gc_types::{
    ArgumentsObject, ArrayObject, BindingCell, BindingCellSchema, BindingCellTable,
    DeclarativeEnvironment, DeclarativeEnvironmentSchema, DirectEvalDerivedBindings,
    DirectEvalDerivedBindingsSchema, DirectEvalExecutionContext, DirectEvalExecutionContextSchema,
    Environment, EnvironmentSchema, FunctionObject, GcFieldNullability, GcLocal, GcOperand,
    I32Local, NamedBindingTable, NonNullable, Nullable, ObjectEnvironment, RealmRecord,
    ScalarValue, StoredValue, StoredValueSchema, StringValue, ValueArray, ValueLocals,
};
use scriptw_ir::{LexicalEnvironmentIr, NativeErrorKind, DIRECT_EVAL_EXECUTION_CONTEXT_NAME};

pub(crate) mod environment_reference;
mod eval_declaration_instantiation;
mod function_body_environment;
mod global_declaration_instantiation;
pub(crate) mod global_environment;
mod lexical_binding_uninitialized;
mod named_binding_mutation;
pub(crate) mod named_environment;
mod resumable_block;
mod retained_array_iterator;
mod retained_for_in_enumerator;
mod with_has_binding;
use global_environment::GlobalBindingFailure;

impl<'a> FunctionBuilder<'a> {
    /// Environment References already own an object base and canonical key.
    /// Retain the complete internal-method completion until the caller releases
    /// the Reference and routes any Throw to its active handler.
    pub(super) fn emit_delete_environment_object_property(
        &mut self,
        object: &ValueLocals,
        key: &crate::operations::PropertyKeyLocals,
        output: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        self.emit_object_delete(object, key, &pending, function)?;
        self.completion().copy_from(&pending, function);
        function.instruction(&Instruction::I32Const(0));
        output.store(function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        output.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        Ok(())
    }

    /// Native failures publish the complete Throw record. A value output is
    /// copied only for existing Reference consumers that retain the exception.
    fn emit_environment_native_error(
        &mut self,
        kind: NativeErrorKind,
        message: RuntimeErrorMessage,
        error: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let pending = self.runtime_schema().reserve_completion(function);
        pending.initialize(function);
        self.emit_throw_runtime_error(kind, message, &pending, function)?;
        error.copy_from(pending.value(), function);
        self.completion().copy_from(&pending, function);
        pending.clear(function);
        Ok(())
    }

    pub(crate) fn emit_enter_for_in_of_tdz_scope(
        &mut self,
        mode: BindingMode,
        environment: &ForInOfEnvironmentIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.push_scope();
        if let Some(runtime_environment) = &environment.tdz_environment {
            self.emit_enter_lexical_environment(runtime_environment, function)?;
        }
        for name in &environment.tdz_binding_names {
            let storage = self.lookup_current_scope_binding(name).unwrap_or_else(|| {
                self.allocate_binding(name.clone(), mode, ValueKind::Dynamic, function)
            });
            self.initialize_binding_uninitialized(storage, function);
        }
        Ok(())
    }

    pub(crate) fn emit_leave_for_in_of_tdz_scope(
        &mut self,
        environment: &ForInOfEnvironmentIr,
        function: &mut Function,
    ) {
        if environment.tdz_environment.is_some() {
            self.emit_leave_lexical_environment(function);
        }
        self.pop_scope();
    }

    pub(crate) fn emit_enter_lexical_environment(
        &mut self,
        environment: &LexicalEnvironmentIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_allocate_lexical_environment_record(environment, function)?;
        self.begin_existing_lexical_environment_scope(environment);
        Ok(())
    }

    /// Allocate one lexical Environment Record without changing the compiler's
    /// binding view. Resumable owners use this when a fresh runtime arm and a
    /// resumed runtime arm must converge on the same compile-time scope.
    pub(crate) fn emit_allocate_lexical_environment_record(
        &mut self,
        environment: &LexicalEnvironmentIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let parent = self.resolve_env_handle_local(0, function);
        let cells = self.emit_allocate_environment_cells(&environment.bindings, function);
        let record = self.emit_initialize_named_environment_header(
            &parent,
            &cells,
            environment.eval_environment.as_ref(),
            function,
        )?;
        self.emit_initialize_function_body_bindings(environment, &record, &parent, function);
        self.replace_current_environment(record.load(schema, function).nullable(), function);
        record.clear(function);
        cells.clear(function);
        parent.clear(function);
        Ok(())
    }

    /// Attaches the compiler's binding view to an environment record that is
    /// already current at run time.
    ///
    /// Resumable loop bodies use this on their resume path: function entry has
    /// reloaded the exact per-iteration record from the activation, so
    /// allocating another record here would give the resumed body a different
    /// cell from closures created before the suspension. The ordinary entry
    /// path still uses [`Self::emit_enter_lexical_environment`].
    pub(crate) fn begin_existing_lexical_environment_scope(
        &mut self,
        environment: &LexicalEnvironmentIr,
    ) {
        let outer_scope_count = self.binding_scopes.len().saturating_sub(1);
        for scope in &mut self.binding_scopes[..outer_scope_count] {
            for storage in scope.values_mut() {
                if let BindingStorage::EnvSlot { hops, .. } = storage {
                    *hops += 1;
                }
            }
        }
        let scope = self
            .binding_scopes
            .last_mut()
            .expect("block environment requires an active binding scope");
        for binding in &environment.bindings {
            scope.insert(
                binding.name.clone(),
                BindingStorage::EnvSlot {
                    slot: binding.slot,
                    hops: 0,
                },
            );
        }
        self.environment_depth += 1;
    }

    pub(crate) fn emit_leave_lexical_environment(&mut self, function: &mut Function) {
        let schema = self.runtime_schema();
        let parent = schema
            .struct_type::<Environment>()
            .field(EnvironmentSchema::PARENT)
            .read(self.current_environment(), schema, function)
            .reference();
        self.replace_current_environment(parent, function);
        self.end_lexical_environment_scope();
    }

    pub(crate) fn end_lexical_environment_scope(&mut self) {
        self.environment_depth = self
            .environment_depth
            .checked_sub(1)
            .expect("block environment depth must not underflow");
        let outer_scope_count = self.binding_scopes.len().saturating_sub(1);
        for scope in &mut self.binding_scopes[..outer_scope_count] {
            for storage in scope.values_mut() {
                if let BindingStorage::EnvSlot { hops, .. } = storage {
                    *hops = hops
                        .checked_sub(1)
                        .expect("outer environment binding must have at least one hop");
                }
            }
        }
    }

    pub(crate) fn emit_replace_lexical_environment(
        &mut self,
        environment: &ForLexicalEnvironmentIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if environment.per_iteration_slots.is_empty() {
            return Ok(());
        }
        let schema = self.runtime_schema();
        let previous = self.resolve_env_handle_local(0, function);
        let parent = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::PARENT)
                    .read(&previous, schema, function)
                    .reference(),
                function,
            );
        let cells = self.emit_allocate_environment_cells(&environment.bindings, function);
        let record = self.emit_initialize_named_environment_header(
            &parent,
            &cells,
            environment.eval_environment.as_ref(),
            function,
        )?;
        let value = schema.reserve_value_local(function);
        for slot in &environment.per_iteration_slots {
            let old_cell = self.emit_environment_cell_local(&previous, *slot, function);
            let initialized = self.emit_read_environment_cell(&old_cell, &value, function);
            let new_cell = self.emit_environment_cell_local(&record, *slot, function);
            self.emit_write_environment_cell(&new_cell, &value, function);
            schema
                .struct_type::<BindingCell>()
                .field(BindingCellSchema::INITIALIZED)
                .write(
                    &new_cell,
                    GcOperand::boolean_local(initialized),
                    schema,
                    function,
                );
            new_cell.clear(function);
            schema.release_i32_local(initialized, function);
            old_cell.clear(function);
        }
        value.clear(function);
        self.replace_current_environment(record.load(schema, function).nullable(), function);
        record.clear(function);
        cells.clear(function);
        parent.clear(function);
        previous.clear(function);
        Ok(())
    }

    pub(crate) fn resolve_env_handle_local(
        &mut self,
        hops: u32,
        function: &mut Function,
    ) -> GcLocal<Environment, Nullable> {
        let schema = self.runtime_schema();
        let environment = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize(self.current_environment().load(schema, function), function);
        for _ in 0..hops {
            environment.replace(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::PARENT)
                    .read(&environment, schema, function)
                    .reference(),
                function,
            );
        }
        environment
    }

    pub(crate) fn emit_environment_cell_local<N: GcFieldNullability>(
        &mut self,
        environment: &GcLocal<Environment, N>,
        slot: u32,
        function: &mut Function,
    ) -> GcLocal<BindingCell> {
        let schema = self.runtime_schema();
        let declarative = schema
            .reserve_gc_local::<DeclarativeEnvironment, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::DECLARATIVE)
                    .read(environment, schema, function)
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
        function.instruction(&Instruction::I32Const(slot as i32));
        index.store(function);
        let cell = schema
            .reserve_gc_local::<BindingCell, NonNullable>(function)
            .initialize(
                schema
                    .array_type::<BindingCellTable>()
                    .read(&cells, index, schema, function)
                    .reference(),
                function,
            );
        schema.release_i32_local(index, function);
        cells.clear(function);
        declarative.clear(function);
        cell
    }

    /// Link the already allocated importing cell to its ultimate exporter.
    /// The importing binding is initialized independently of exporter TDZ.
    pub(crate) fn emit_initialize_indirect_binding(
        &mut self,
        local_slot: u32,
        target: &GcLocal<BindingCell>,
        function: &mut Function,
    ) {
        let environment = self.resolve_env_handle_local(0, function);
        let cell = self.emit_environment_cell_local(&environment, local_slot, function);
        let schema = self.runtime_schema();
        let cell_type = schema.struct_type::<BindingCell>();
        cell_type.field(BindingCellSchema::TARGET).write(
            &cell,
            GcOperand::nullable_reference(target, schema),
            schema,
            function,
        );
        cell_type.field(BindingCellSchema::INITIALIZED).write(
            &cell,
            GcOperand::boolean(true),
            schema,
            function,
        );
        cell.clear(function);
        environment.clear(function);
    }

    /// Resolve the sole strong import target before GetBindingValue. The local
    /// importing cell keeps its own initialized and immutability state for writes.
    pub(crate) fn emit_read_environment_cell(
        &mut self,
        cell: &GcLocal<BindingCell>,
        value: &ValueLocals,
        function: &mut Function,
    ) -> I32Local {
        let schema = self.runtime_schema();
        let source = schema
            .reserve_gc_local::<BindingCell, Nullable>(function)
            .initialize(cell.load(schema, function).nullable(), function);
        let target = schema
            .reserve_gc_local::<BindingCell, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<BindingCell>()
                    .field(BindingCellSchema::TARGET)
                    .read(cell, schema, function)
                    .reference(),
                function,
            );
        target.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        source.replace(target.load(schema, function), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let initialized = schema.reserve_i32_local(function);
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::INITIALIZED)
            .read(&source, schema, function)
            .store(initialized, function);
        let stored = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<BindingCell>()
                    .field(BindingCellSchema::VALUE)
                    .read(&source, schema, function)
                    .reference(),
                function,
            );
        let stored_type = schema.struct_type::<StoredValue>();
        stored_type
            .field(StoredValueSchema::TAG)
            .read(&stored, schema, function)
            .store(value.tag(), function);
        stored_type
            .field(StoredValueSchema::SCALAR)
            .read(&stored, schema, function)
            .store_i64(value.scalar(), function);
        stored_type
            .field(StoredValueSchema::REFERENCE)
            .read(&stored, schema, function)
            .store_eq(value.reference(), function);
        stored.clear(function);
        target.clear(function);
        source.clear(function);
        initialized
    }

    pub(crate) fn emit_write_environment_cell(
        &mut self,
        cell: &GcLocal<BindingCell>,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let stored = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(value, function),
                function,
            );
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::VALUE)
            .write(
                cell,
                GcOperand::reference(&stored, schema),
                schema,
                function,
            );
        stored.clear(function);
    }

    pub(crate) fn emit_allocate_environment_cells(
        &mut self,
        bindings: &[OwnedEnvBindingIr],
        function: &mut Function,
    ) -> GcLocal<BindingCellTable> {
        let schema = self.runtime_schema();
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let mut cells = Vec::with_capacity(bindings.len());
        for slot in 0..bindings.len() {
            let binding = bindings
                .iter()
                .find(|binding| binding.slot as usize == slot)
                .expect("planned environment slots must be contiguous");
            let (mutable, immutable_strict) = match binding.mutability {
                scriptw_ir::EnvironmentBindingMutabilityIr::Mutable => (true, true),
                scriptw_ir::EnvironmentBindingMutabilityIr::Immutable { strict } => (false, strict),
            };
            cells.push(self.emit_allocate_environment_cell(
                &undefined,
                false,
                mutable,
                immutable_strict,
                function,
            ));
        }
        let array = schema
            .reserve_gc_local::<BindingCellTable, NonNullable>(function)
            .initialize(
                schema.array_type::<BindingCellTable>().fixed(
                    cells.iter().map(|cell| GcOperand::reference(cell, schema)),
                    function,
                ),
                function,
            );
        for cell in cells.into_iter().rev() {
            cell.clear(function);
        }
        undefined.clear(function);
        array
    }

    fn emit_environment_record(
        &mut self,
        parent: &GcLocal<Environment, Nullable>,
        cells: &GcLocal<BindingCellTable>,
        named_bindings: &GcLocal<NamedBindingTable, Nullable>,
        object: &GcLocal<ObjectEnvironment, Nullable>,
        kind: named_environment::NamedEnvironmentKind,
        function: &mut Function,
    ) -> GcLocal<Environment> {
        let schema = self.runtime_schema();
        let declarative = schema
            .reserve_gc_local::<DeclarativeEnvironment, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<DeclarativeEnvironment>()
                    .construct((GcOperand::reference(cells, schema),), function),
                function,
            );
        let environment = schema
            .reserve_gc_local::<Environment, NonNullable>(function)
            .initialize(
                schema.struct_type::<Environment>().construct(
                    (
                        GcOperand::<crate::gc_types::GcRef<RealmRecord>, Nullable>::null(schema),
                        GcOperand::reference(named_bindings, schema),
                        GcOperand::i32(kind.code() as i32),
                        GcOperand::reference(parent, schema),
                        GcOperand::null(schema),
                        GcOperand::nullable_reference(&declarative, schema),
                        GcOperand::reference(object, schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                    ),
                    function,
                ),
                function,
            );
        declarative.clear(function);
        environment
    }

    pub(crate) fn emit_allocate_environment_cell(
        &mut self,
        initial_value: &ValueLocals,
        initialized: bool,
        mutable: bool,
        immutable_strict: bool,
        function: &mut Function,
    ) -> GcLocal<BindingCell> {
        let schema = self.runtime_schema();
        let stored = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(initial_value, function),
                function,
            );
        let cell = schema
            .reserve_gc_local::<BindingCell, NonNullable>(function)
            .initialize(
                schema.struct_type::<BindingCell>().construct(
                    (
                        GcOperand::reference(&stored, schema),
                        GcOperand::boolean(initialized),
                        GcOperand::boolean(mutable),
                        GcOperand::boolean(immutable_strict),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::i64(0),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                    ),
                    function,
                ),
                function,
            );
        stored.clear(function);
        cell
    }

    pub(crate) fn emit_initialize_environment_cell(
        &mut self,
        cell: &GcLocal<BindingCell>,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        self.emit_write_environment_cell(cell, value, function);
        let schema = self.runtime_schema();
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::INITIALIZED)
            .write(cell, GcOperand::boolean(true), schema, function);
    }

    pub(crate) fn read_env_slot_to_locals(
        &mut self,
        slot: u32,
        hops: u32,
        value: &ValueLocals,
        function: &mut Function,
    ) -> I32Local {
        let environment = self.resolve_env_handle_local(hops, function);
        let cell = self.emit_environment_cell_local(&environment, slot, function);
        let initialized = self.emit_read_environment_cell(&cell, value, function);
        cell.clear(function);
        environment.clear(function);
        initialized
    }

    pub(crate) fn write_env_slot_from_locals(
        &mut self,
        slot: u32,
        hops: u32,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        let environment = self.resolve_env_handle_local(hops, function);
        let cell = self.emit_environment_cell_local(&environment, slot, function);
        self.emit_initialize_environment_cell(&cell, value, function);
        cell.clear(function);
        environment.clear(function);
    }

    /// FunctionDeclarationInstantiation reuses the initialized arguments value
    /// for a source `var arguments`; other newly created var cells start empty.
    pub(crate) fn initialize_function_var_binding(
        &mut self,
        name: &str,
        storage: BindingStorage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if name == "arguments" && self.function_arguments_protocol.present().is_some() {
            let arguments = self
                .lookup_binding(LEXICAL_ARGUMENTS_NAME)
                .expect("bound arguments protocol owns its initialized binding");
            let schema = self.runtime_schema();
            let value = schema.reserve_value_local(function);
            self.read_binding_to_locals(arguments, &value, function)?;
            self.write_binding_from_locals(storage, &value, function);
            value.clear(function);
        } else {
            self.initialize_binding_undefined(storage, function);
        }
        Ok(())
    }

    pub(crate) fn initialize_binding_undefined(
        &mut self,
        storage: BindingStorage,
        function: &mut Function,
    ) {
        match storage {
            BindingStorage::Local(id) => {
                let binding = self.local_binding(id);
                binding.value().set_undefined(function);
                binding.mark_initialized(function);
            }
            BindingStorage::EnvSlot { slot, hops } => {
                let schema = self.runtime_schema();
                let value = schema.reserve_value_local(function);
                value.set_undefined(function);
                self.write_env_slot_from_locals(slot, hops, &value, function);
                value.clear(function);
            }
        }
    }

    pub(crate) fn initialize_binding_uninitialized(
        &mut self,
        storage: BindingStorage,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        match storage {
            BindingStorage::Local(id) => {
                let binding = self.local_binding(id);
                binding.value().set_undefined(function);
                function.instruction(&Instruction::I32Const(0));
                binding
                    .initialization_check()
                    .expect("only checked lexical storage can be uninitialized")
                    .store(function);
            }
            BindingStorage::EnvSlot { slot, hops } => {
                let environment = self.resolve_env_handle_local(hops, function);
                let cell = self.emit_environment_cell_local(&environment, slot, function);
                let value = schema.reserve_value_local(function);
                value.set_undefined(function);
                self.emit_write_environment_cell(&cell, &value, function);
                schema
                    .struct_type::<BindingCell>()
                    .field(BindingCellSchema::INITIALIZED)
                    .write(&cell, GcOperand::boolean(false), schema, function);
                value.clear(function);
                cell.clear(function);
                environment.clear(function);
            }
        }
    }

    pub(crate) fn write_binding_from_locals(
        &mut self,
        storage: BindingStorage,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        match storage {
            BindingStorage::Local(id) => {
                let binding = self.local_binding(id);
                binding.value().copy_from(value, function);
                binding.mark_initialized(function);
            }
            BindingStorage::EnvSlot { slot, hops } => {
                self.write_env_slot_from_locals(slot, hops, value, function);
            }
        }
    }

    /// An assignment checks the own cell before any import TARGET read. The
    /// unchecked write remains solely for declaration/head initialization.
    pub(crate) fn write_binding_from_locals_checked(
        &mut self,
        storage: BindingStorage,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match storage {
            BindingStorage::Local(_) => {
                // Ordinary local TDZ/const and named-self failure/no-op prefixes
                // are already owned by the checked lowering assignment.
                self.write_binding_from_locals(storage, value, function);
            }
            BindingStorage::EnvSlot { slot, hops } => {
                let schema = self.runtime_schema();
                let environment = self.resolve_env_handle_local(hops, function);
                let cell = self.emit_environment_cell_local(&environment, slot, function);
                let error = schema.reserve_value_local(function);
                schema
                    .struct_type::<BindingCell>()
                    .field(BindingCellSchema::INITIALIZED)
                    .read(&cell, schema, function);
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_environment_native_error(
                    NativeErrorKind::ReferenceError,
                    RuntimeErrorMessage::LEXICAL_BINDING_ACCESSED_BEFORE_INITIALIZATION,
                    &error,
                    function,
                )?;
                self.emit_propagate_current_throw_if_needed(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                self.open_frame(ControlFrameKind::Block, function);
                schema
                    .struct_type::<BindingCell>()
                    .field(BindingCellSchema::MUTABLE)
                    .read(&cell, schema, function);
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                if !self.is_current_function_strict() {
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
                environment.clear(function);
            }
        }
        Ok(())
    }

    pub(crate) fn bind_captured_bindings(&mut self, function: &mut Function) {
        let schema = self.runtime_schema();
        for binding in self.captured_bindings {
            // The retained direct-eval context is a strong typed Environment
            // edge cached by the entry owner, never a JavaScript binding value.
            if binding.name == DIRECT_EVAL_EXECUTION_CONTEXT_NAME {
                continue;
            }
            let storage = if binding.name == LEXICAL_ARGUMENTS_NAME {
                let storage = self.allocate_local_binding(binding.mode, function);
                let value = schema.reserve_value_local(function);
                let initialized =
                    self.read_env_slot_to_locals(binding.slot, binding.hops, &value, function);
                self.write_binding_from_locals(storage, &value, function);
                if let BindingStorage::Local(id) = storage {
                    initialized.load(function);
                    self.local_binding(id)
                        .initialization_check()
                        .expect("captured arguments preserve their source initialization state")
                        .store(function);
                }
                schema.release_i32_local(initialized, function);
                value.clear(function);
                storage
            } else {
                BindingStorage::EnvSlot {
                    slot: binding.slot,
                    hops: binding.hops,
                }
            };
            self.binding_scopes
                .last_mut()
                .expect("binding scope stack must exist")
                .insert(binding.name.clone(), storage);
        }
    }

    pub(crate) fn bind_parameters(&mut self, function: &mut Function) -> Result<(), EmitError> {
        let parameter_initializers = self
            .body
            .statements
            .iter()
            .filter_map(|statement| {
                let StatementIr::ParameterInitialization {
                    parameter_index,
                    statements,
                } = statement
                else {
                    return None;
                };
                Some((*parameter_index, statements.clone()))
            })
            .collect::<Vec<_>>();
        let arguments_protocol = self.function_arguments_protocol.take_for_binding()?;
        if let Some(arguments_protocol) = arguments_protocol.into_present() {
            let arguments_storage =
                self.allocate_dynamic_binding_storage(LEXICAL_ARGUMENTS_NAME, function);
            self.binding_scopes
                .last_mut()
                .expect("binding scope stack must exist")
                .insert(LEXICAL_ARGUMENTS_NAME.to_string(), arguments_storage);
            self.initialize_arguments_binding(arguments_storage, arguments_protocol, function)?;
        }

        for param in self.params {
            let storage = self.allocate_dynamic_binding_storage(&param.name, function);
            self.binding_scopes
                .last_mut()
                .expect("binding scope stack must exist")
                .insert(param.name.clone(), storage);
        }

        for (index, param) in self.params.iter().enumerate() {
            let storage = self.lookup_binding(&param.name).ok_or_else(|| {
                EmitError::unsupported(format!(
                    "unsupported in scriptw wasm-aot first slice: missing parameter binding `{}`",
                    param.name
                ))
            })?;
            if param.is_rest {
                self.initialize_rest_parameter(index, storage, function)?;
            } else {
                self.initialize_parameter(index, param, storage, function)?;
            }
            if let Some((_, statements)) = parameter_initializers
                .iter()
                .find(|(parameter_index, _)| *parameter_index == index)
            {
                for statement in statements {
                    self.compile_statement(statement, function)?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn allocate_dynamic_binding_storage(
        &mut self,
        name: &str,
        function: &mut Function,
    ) -> BindingStorage {
        self.activation_owned_binding_storage(name)
            .unwrap_or_else(|| self.allocate_local_binding(BindingMode::Let, function))
    }

    pub(crate) fn initialize_arguments_binding(
        &mut self,
        storage: BindingStorage,
        protocol: PresentArgumentsObjectProtocol,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        let arguments = schema
            .reserve_gc_local::<ArgumentsObject, NonNullable>(function)
            .initialize(
                self.emit_arguments_object_payload(&protocol, function)?,
                function,
            );
        value.set_reference(&arguments, schema, function);
        // Retain the actual object before a parameter or source write can
        // replace the visible `arguments` binding.
        self.emit_publish_source_activation_arguments(&arguments, function);
        arguments.clear(function);
        self.write_binding_from_locals(storage, &value, function);
        value.clear(function);
        Ok(())
    }

    pub(crate) fn initialize_parameter(
        &mut self,
        index: usize,
        param: &FunctionParamIr,
        storage: BindingStorage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        self.read_argument_at_index(index, &value, function);
        if let Some(default_init) = &param.default_init {
            value.tag().load(function);
            function.instruction(&Instruction::I32Const(
                WasmRuntimeValueTag::Undefined as i32,
            ));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            self.compile_expr_to_value(default_init, &value, function)?;
            self.emit_propagate_current_throw_if_needed(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.write_binding_from_locals(storage, &value, function);
        value.clear(function);
        Ok(())
    }

    pub(crate) fn initialize_rest_parameter(
        &mut self,
        index: usize,
        storage: BindingStorage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        let array = schema
            .reserve_gc_local::<ArrayObject, NonNullable>(function)
            .initialize(self.emit_rest_array_payload(index, function)?, function);
        value.set_reference(&array, schema, function);
        array.clear(function);
        self.write_binding_from_locals(storage, &value, function);
        value.clear(function);
        Ok(())
    }

    pub(crate) fn bind_self_function(&mut self, function: &mut Function) -> Result<(), EmitError> {
        let Some(name) = self.self_binding_name.clone() else {
            return Ok(());
        };
        let storage = if let Some(slot) = self.owned_env_slot(&name) {
            BindingStorage::EnvSlot { slot, hops: 0 }
        } else {
            self.allocate_local_binding(BindingMode::Const, function)
        };
        self.binding_scopes
            .last_mut()
            .expect("binding scope stack must exist")
            .insert(name, storage);
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        value.set_reference(
            self.body_entry_locals()
                .expect("named callable owns an entry")
                .function_object()
                .expect("named callable owns a function object"),
            schema,
            function,
        );
        self.write_binding_from_locals(storage, &value, function);
        value.clear(function);
        Ok(())
    }

    pub(crate) fn emit_direct_eval_context_root(
        &self,
        function: &mut Function,
    ) -> Option<GcLocal<DirectEvalExecutionContext>> {
        let context = self.direct_eval_execution_context_local()?;
        let schema = self.runtime_schema();
        Some(
            schema
                .reserve_gc_local::<DirectEvalExecutionContext, NonNullable>(function)
                .initialize(
                    context.load(schema, function).require_non_null(function),
                    function,
                ),
        )
    }

    fn emit_direct_eval_derived_bindings(
        &self,
        context: &GcLocal<DirectEvalExecutionContext>,
        function: &mut Function,
    ) -> GcLocal<DirectEvalDerivedBindings> {
        let schema = self.runtime_schema();
        schema
            .reserve_gc_local::<DirectEvalDerivedBindings, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<DirectEvalExecutionContext>()
                    .field(DirectEvalExecutionContextSchema::DERIVED_BINDINGS)
                    .read(context, schema, function)
                    .reference()
                    .require_non_null(function),
                function,
            )
    }

    pub(crate) fn compile_this_to_locals(
        &mut self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        if let Some(context) = self.emit_direct_eval_context_root(function) {
            let derived = schema
                .reserve_gc_local::<DirectEvalDerivedBindings, Nullable>(function)
                .initialize(
                    schema
                        .struct_type::<DirectEvalExecutionContext>()
                        .field(DirectEvalExecutionContextSchema::DERIVED_BINDINGS)
                        .read(&context, schema, function)
                        .reference(),
                    function,
                );
            derived.load(schema, function);
            function.instruction(&Instruction::RefIsNull);
            self.open_frame(ControlFrameKind::If, function);
            let stored = schema
                .reserve_gc_local::<StoredValue, NonNullable>(function)
                .initialize(
                    schema
                        .struct_type::<DirectEvalExecutionContext>()
                        .field(DirectEvalExecutionContextSchema::THIS_SNAPSHOT)
                        .read(&context, schema, function)
                        .reference(),
                    function,
                );
            self.emit_stored_value_to_locals(&stored, value, function);
            stored.clear(function);
            function.instruction(&Instruction::Else);
            let status_cell = schema
                .reserve_gc_local::<BindingCell, NonNullable>(function)
                .initialize(
                    schema
                        .struct_type::<DirectEvalDerivedBindings>()
                        .field(DirectEvalDerivedBindingsSchema::THIS_STATUS_CELL)
                        .read(&derived, schema, function)
                        .reference(),
                    function,
                );
            let status = schema.reserve_value_local(function);
            let initialized = self.emit_read_environment_cell(&status_cell, &status, function);
            schema.release_i32_local(initialized, function);
            status_cell.clear(function);
            self.compile_truthy_tagged_i32(&status, function)?;
            status.clear(function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_derived_this_reference_error(
                RuntimeErrorMessage::MUST_CALL_SUPER_BEFORE_ACCESSING_THIS,
                value,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            let this_cell = schema
                .reserve_gc_local::<BindingCell, NonNullable>(function)
                .initialize(
                    schema
                        .struct_type::<DirectEvalDerivedBindings>()
                        .field(DirectEvalDerivedBindingsSchema::THIS_CELL)
                        .read(&derived, schema, function)
                        .reference(),
                    function,
                );
            let initialized = self.emit_read_environment_cell(&this_cell, value, function);
            schema.release_i32_local(initialized, function);
            this_cell.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            derived.clear(function);
            context.clear(function);
            return Ok(());
        }
        match self.function_flavor {
            FunctionFlavor::Ordinary => {
                if self.lexical_derived_activation.is_some() {
                    return self.emit_get_derived_this_to_locals(value, function);
                }
                if let Some(entry) = self.body_entry_locals() {
                    value.copy_from(entry.this_value(), function);
                } else if self.is_main() {
                    self.emit_execution_global_object_to_locals(value, function);
                } else {
                    return Err(EmitError::unsupported(
                        "callable this requires its declared entry",
                    ));
                }
            }
            FunctionFlavor::Arrow => {
                if self.lexical_derived_activation.is_some() {
                    self.emit_get_derived_this_to_locals(value, function)?;
                } else if let Some(storage) = self.lookup_binding(LEXICAL_THIS_NAME) {
                    self.read_binding_to_locals(storage, value, function)?;
                } else {
                    self.emit_execution_global_object_to_locals(value, function);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn emit_default_this(&mut self, value: &ValueLocals, function: &mut Function) {
        self.emit_execution_global_object_to_locals(value, function);
    }

    pub(crate) fn emit_default_this_for_known_strictness(
        &mut self,
        strict: bool,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        if strict {
            value.set_undefined(function);
        } else {
            self.emit_default_this(value, function);
        }
    }

    pub(crate) fn compile_new_target_to_locals(
        &mut self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        if let Some(context) = self.emit_direct_eval_context_root(function) {
            let stored = schema
                .reserve_gc_local::<StoredValue, NonNullable>(function)
                .initialize(
                    schema
                        .struct_type::<DirectEvalExecutionContext>()
                        .field(DirectEvalExecutionContextSchema::NEW_TARGET_SNAPSHOT)
                        .read(&context, schema, function)
                        .reference(),
                    function,
                );
            self.emit_stored_value_to_locals(&stored, value, function);
            stored.clear(function);
            context.clear(function);
            return Ok(());
        }
        if self.function_flavor == FunctionFlavor::Arrow {
            if let Some(storage) = self.lookup_binding(LEXICAL_NEW_TARGET_NAME) {
                self.read_binding_to_locals(storage, value, function)?;
                return Ok(());
            }
        }
        if let Some(entry) = self.body_entry_locals() {
            if entry.resume_activation().is_some() {
                value.set_undefined(function);
            } else {
                value.copy_from(entry.new_target(), function);
            }
        } else {
            value.set_undefined(function);
        }
        Ok(())
    }

    pub(crate) fn is_current_function_strict(&self) -> bool {
        self.function_id
            .as_ref()
            .and_then(|function_id| self.functions.get(function_id))
            .map_or(self.strict, |meta| meta.strict)
    }

    pub(crate) fn read_argument_at_index(
        &mut self,
        index: usize,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        self.emit_builtin_arg_to_value(index, value, function);
    }

    pub(crate) fn read_binding_to_locals(
        &mut self,
        storage: BindingStorage,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match storage {
            BindingStorage::Local(id) => {
                self.check_local_binding_initialized(id, function)?;
                value.copy_from(self.local_binding(id).value(), function);
            }
            BindingStorage::EnvSlot { slot, hops } => {
                let schema = self.runtime_schema();
                let initialized = self.read_env_slot_to_locals(slot, hops, value, function);
                initialized.load(function);
                schema.release_i32_local(initialized, function);
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_lexical_binding_uninitialized(value, function)?;
                if let Some(target) = self.active_throw_target() {
                    self.emit_branch_to_target(target, function);
                } else {
                    self.emit_return_current_completion(function);
                }
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        Ok(())
    }

    /// Both tagged reads and proven scalar reads share the same live lexical
    /// initialization check. The error value exists only on the abrupt edge.
    pub(crate) fn check_local_binding_initialized(
        &mut self,
        id: crate::emit::LocalBindingId,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if let Some(initialized) = self.local_binding(id).initialization_check() {
            initialized.load(function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            let error = self.runtime_schema().reserve_value_local(function);
            self.emit_lexical_binding_uninitialized(&error, function)?;
            self.emit_propagate_current_throw_if_needed(function);
            error.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        Ok(())
    }

    pub(crate) fn allocate_binding(
        &mut self,
        name: String,
        mode: BindingMode,
        _kind: ValueKind,
        function: &mut Function,
    ) -> BindingStorage {
        let storage = match mode {
            BindingMode::Let | BindingMode::Const if self.owned_env_slot(&name).is_some() => self
                .activation_owned_binding_storage(&name)
                .expect("owned env slot must exist"),
            BindingMode::Let | BindingMode::Const => self.allocate_local_binding(mode, function),
            BindingMode::Var => panic!("var bindings are hoisted"),
        };
        self.binding_scopes
            .last_mut()
            .expect("binding scope stack must exist")
            .insert(name, storage);
        storage
    }

    pub(crate) fn lookup_binding(&self, name: &str) -> Option<BindingStorage> {
        self.binding_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    pub(crate) fn lookup_current_scope_binding(&self, name: &str) -> Option<BindingStorage> {
        self.binding_scopes
            .last()
            .and_then(|scope| scope.get(name).copied())
    }

    pub(crate) fn lookup_owner_binding(&self, name: &str) -> Option<BindingStorage> {
        self.binding_scopes
            .first()
            .and_then(|scope| scope.get(name).copied())
    }

    pub(crate) fn push_scope(&mut self) {
        self.binding_scopes.push(BTreeMap::new());
    }

    pub(crate) fn pop_scope(&mut self) {
        self.binding_scopes.pop();
    }

    pub(crate) fn owned_env_slot(&self, name: &str) -> Option<u32> {
        self.owned_env_bindings
            .iter()
            .find(|binding| binding.name == name)
            .map(|binding| binding.slot)
    }

    /// Attach an already checked invocation cell to the current compiler scope.
    /// This publishes no runtime write and preserves the cell across suspension.
    pub(crate) fn bind_owned_environment_alias(&mut self, binding: &OwnedEnvBindingIr) {
        debug_assert!(self.owned_env_bindings.contains(binding));
        let storage = BindingStorage::EnvSlot {
            slot: binding.slot,
            hops: self.environment_depth,
        };
        self.binding_scopes
            .last_mut()
            .expect("a checked source owner has a binding scope")
            .insert(binding.name.clone(), storage);
    }

    /// Resolves a compiler-private derived-constructor activation binding.
    /// The owner has an owned slot; arrows reach the same slot through their
    /// recorded captured binding and hop count.
    pub(crate) fn derived_activation_storage(
        &self,
        name: &str,
    ) -> Result<BindingStorage, EmitError> {
        self.owned_env_slot(name)
            .map(|slot| BindingStorage::EnvSlot {
                slot,
                hops: self.environment_depth,
            })
            .or_else(|| self.lookup_binding(name))
            .ok_or_else(|| {
                EmitError::unsupported(format!(
                    "derived constructor activation is missing compiler-private binding `{name}`"
                ))
            })
    }

    fn emit_derived_this_reference_error(
        &mut self,
        message: RuntimeErrorMessage,
        error: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_environment_native_error(
            NativeErrorKind::ReferenceError,
            message,
            error,
            function,
        )?;
        if let Some(target) = self.active_throw_target() {
            self.emit_branch_to_target(target, function);
        } else {
            self.emit_return_current_completion(function);
        }
        Ok(())
    }

    /// Resolve the derived result while its own this/status cells are still
    /// live. Post-body errors belong to the restored caller context's Realm.
    pub(crate) fn emit_derived_constructor_body_result(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if !self.is_derived_constructor {
            return Ok(());
        }
        let activation = self
            .lexical_derived_activation
            .expect("derived constructor body must have activation metadata");
        let this_storage = self.derived_activation_storage(&activation.this_binding)?;
        let status_storage = self.derived_activation_storage(&activation.this_status_binding)?;
        let schema = self.runtime_schema();
        let caller_realm = schema.reserve_gc_local(function).initialize(
            self.body_entry_locals()
                .and_then(|entry| entry.caller_realm())
                .expect("ordinary constructor entry retains its actual caller Realm")
                .load(schema, function),
            function,
        );
        let value = schema.reserve_value_local(function);
        let prototype = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(COMPLETION_KIND_NORMAL as i32));
        function.instruction(&Instruction::I32Eq);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(COMPLETION_KIND_RETURN as i32));
        function.instruction(&Instruction::I32Eq);
        self.completion().value().tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.read_binding_to_locals(status_storage, &value, function)?;
        self.compile_truthy_tagged_i32(&value, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_load_non_array_realm_intrinsic(
            &caller_realm,
            NonArrayRealmIntrinsicSlot::ReferenceErrorPrototype,
            &prototype,
            function,
        );
        self.emit_throw_runtime_error_with_prototype(
            NativeErrorKind::ReferenceError,
            RuntimeErrorMessage::MUST_CALL_SUPER_BEFORE_ACCESSING_THIS,
            &prototype,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.read_binding_to_locals(this_storage, &value, function)?;
        self.completion().value().copy_from(&value, function);
        self.completion().set_kind(CompletionKind::Return, function);
        function.instruction(&Instruction::Else);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(COMPLETION_KIND_RETURN as i32));
        function.instruction(&Instruction::I32Eq);
        self.emit_is_heap_object_like_tag_i32(self.completion().value().tag(), function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_load_non_array_realm_intrinsic(
            &caller_realm,
            NonArrayRealmIntrinsicSlot::TypeErrorPrototype,
            &prototype,
            function,
        );
        self.emit_throw_runtime_error_with_prototype(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::DERIVED_CONSTRUCTOR_MAY_ONLY_RETURN_OBJECT_OR_UNDEFINED,
            &prototype,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.clear(function);
        prototype.clear(function);
        value.clear(function);
        caller_realm.clear(function);
        Ok(())
    }

    pub(crate) fn emit_get_derived_this_to_locals(
        &mut self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let activation = self.lexical_derived_activation.ok_or_else(|| {
            EmitError::unsupported("derived this requested without activation metadata")
        })?;
        let status_storage = self.derived_activation_storage(&activation.this_status_binding)?;
        let this_storage = self.derived_activation_storage(&activation.this_binding)?;
        let schema = self.runtime_schema();
        let status = schema.reserve_value_local(function);
        self.read_binding_to_locals(status_storage, &status, function)?;
        self.compile_truthy_tagged_i32(&status, function)?;
        status.clear(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_derived_this_reference_error(
            RuntimeErrorMessage::MUST_CALL_SUPER_BEFORE_ACCESSING_THIS,
            value,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.read_binding_to_locals(this_storage, value, function)
    }

    pub(crate) fn emit_get_derived_new_target_to_locals(
        &mut self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if let Some(context) = self.emit_direct_eval_context_root(function) {
            let schema = self.runtime_schema();
            let derived = self.emit_direct_eval_derived_bindings(&context, function);
            let cell = schema
                .reserve_gc_local::<BindingCell, NonNullable>(function)
                .initialize(
                    schema
                        .struct_type::<DirectEvalDerivedBindings>()
                        .field(DirectEvalDerivedBindingsSchema::NEW_TARGET_CELL)
                        .read(&derived, schema, function)
                        .reference(),
                    function,
                );
            let initialized = self.emit_read_environment_cell(&cell, value, function);
            schema.release_i32_local(initialized, function);
            cell.clear(function);
            derived.clear(function);
            context.clear(function);
            return Ok(());
        }
        let activation = self.lexical_derived_activation.ok_or_else(|| {
            EmitError::unsupported("derived new.target requested without activation metadata")
        })?;
        let storage = self.derived_activation_storage(&activation.new_target_binding)?;
        self.read_binding_to_locals(storage, value, function)
    }

    pub(crate) fn emit_get_derived_active_function_to_locals(
        &mut self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if let Some(context) = self.emit_direct_eval_context_root(function) {
            let schema = self.runtime_schema();
            let derived = self.emit_direct_eval_derived_bindings(&context, function);
            let cell = schema
                .reserve_gc_local::<BindingCell, NonNullable>(function)
                .initialize(
                    schema
                        .struct_type::<DirectEvalDerivedBindings>()
                        .field(DirectEvalDerivedBindingsSchema::ACTIVE_FUNCTION_CELL)
                        .read(&derived, schema, function)
                        .reference(),
                    function,
                );
            let initialized = self.emit_read_environment_cell(&cell, value, function);
            schema.release_i32_local(initialized, function);
            cell.clear(function);
            derived.clear(function);
            context.clear(function);
            return Ok(());
        }
        let activation = self.lexical_derived_activation.ok_or_else(|| {
            EmitError::unsupported("derived active function requested without activation metadata")
        })?;
        let storage = self.derived_activation_storage(&activation.active_function_binding)?;
        self.read_binding_to_locals(storage, value, function)
    }

    /// Initializes the same per-invocation this/status cells exactly once.
    /// Original derived status is separate from BindingCell INITIALIZED.
    pub(crate) fn emit_bind_derived_this_from_locals(
        &mut self,
        value: &ValueLocals,
        error: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let status = schema.reserve_value_local(function);
        if let Some(context) = self.emit_direct_eval_context_root(function) {
            let derived = self.emit_direct_eval_derived_bindings(&context, function);
            let this_cell = schema
                .reserve_gc_local::<BindingCell, NonNullable>(function)
                .initialize(
                    schema
                        .struct_type::<DirectEvalDerivedBindings>()
                        .field(DirectEvalDerivedBindingsSchema::THIS_CELL)
                        .read(&derived, schema, function)
                        .reference(),
                    function,
                );
            let status_cell = schema
                .reserve_gc_local::<BindingCell, NonNullable>(function)
                .initialize(
                    schema
                        .struct_type::<DirectEvalDerivedBindings>()
                        .field(DirectEvalDerivedBindingsSchema::THIS_STATUS_CELL)
                        .read(&derived, schema, function)
                        .reference(),
                    function,
                );
            let initialized = self.emit_read_environment_cell(&status_cell, &status, function);
            schema.release_i32_local(initialized, function);
            self.compile_truthy_tagged_i32(&status, function)?;
            self.open_frame(ControlFrameKind::If, function);
            self.emit_derived_this_reference_error(
                RuntimeErrorMessage::SUPER_CALLED_TWICE_IN_DERIVED_CONSTRUCTOR,
                error,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.emit_initialize_environment_cell(&this_cell, value, function);
            status.set_scalar(ScalarValue::Boolean(true), function);
            self.emit_initialize_environment_cell(&status_cell, &status, function);
            status_cell.clear(function);
            this_cell.clear(function);
            derived.clear(function);
            context.clear(function);
        } else {
            let activation = self.lexical_derived_activation.ok_or_else(|| {
                EmitError::unsupported("derived this bind requested without activation metadata")
            })?;
            let status_storage =
                self.derived_activation_storage(&activation.this_status_binding)?;
            let this_storage = self.derived_activation_storage(&activation.this_binding)?;
            self.read_binding_to_locals(status_storage, &status, function)?;
            self.compile_truthy_tagged_i32(&status, function)?;
            self.open_frame(ControlFrameKind::If, function);
            self.emit_derived_this_reference_error(
                RuntimeErrorMessage::SUPER_CALLED_TWICE_IN_DERIVED_CONSTRUCTOR,
                error,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.write_binding_from_locals(this_storage, value, function);
            status.set_scalar(ScalarValue::Boolean(true), function);
            self.write_binding_from_locals(status_storage, &status, function);
        }
        status.clear(function);
        Ok(())
    }

    pub(crate) fn emit_global_property_read(
        &mut self,
        name: &str,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let key = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(
                self.emit_interned_string_reference(name, function)?,
                function,
            );
        let entry = schema
            .reserve_gc_local::<crate::gc_types::NamedBinding, Nullable>(function)
            .initialize_null(schema, function);
        self.emit_global_lexical_entry_to_local(&key, &entry, function);
        entry.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        let object = schema.reserve_value_local(function);
        self.emit_execution_global_object_to_locals(&object, function);
        let property_key =
            crate::operations::PropertyKeyLocals::from_string(schema, &key, function);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectReadArguments::new(
                    &object,
                    &object,
                    &property_key,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(self.completion(), function);
        self.emit_propagate_current_throw_if_needed(function);
        value.copy_from(self.completion().value(), function);
        property_key.clear(function);
        object.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_global_lexical_read(&entry, value, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        entry.clear(function);
        key.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }

    pub(crate) fn emit_global_property_write(
        &mut self,
        name: &str,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let key = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(
                self.emit_interned_string_reference(name, function)?,
                function,
            );
        let entry = schema
            .reserve_gc_local::<crate::gc_types::NamedBinding, Nullable>(function)
            .initialize_null(schema, function);
        self.emit_global_lexical_entry_to_local(&key, &entry, function);
        entry.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        let object = schema.reserve_value_local(function);
        self.emit_execution_global_object_to_locals(&object, function);
        let property_key =
            crate::operations::PropertyKeyLocals::from_string(schema, &key, function);
        let strict = schema.reserve_i32_local(function);
        if let Some(reference_strictness) = self.object_write_strict_flag_local {
            reference_strictness.load(function);
        } else {
            function.instruction(&Instruction::I32Const(i32::from(
                self.is_current_function_strict(),
            )));
        }
        strict.store(function);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectWriteArguments::new(
                    &object,
                    &property_key,
                    value,
                    strict,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(self.completion(), function);
        self.emit_propagate_current_throw_if_needed(function);
        schema.release_i32_local(strict, function);
        property_key.clear(function);
        object.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_global_lexical_write(&entry, value, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        entry.clear(function);
        key.clear(function);
        Ok(())
    }

    /// Strictness belongs to this source Reference. A missing strict
    /// identifier rejects before Set; sloppy PutValue may create a property.
    pub(crate) fn emit_global_property_write_checked(
        &mut self,
        name: &str,
        value: &ValueLocals,
        strictness: Strictness,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if !strictness.throws_on_failed_set() {
            return self.emit_global_property_write(name, value, function);
        }
        let schema = self.runtime_schema();
        let key = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(
                self.emit_interned_string_reference(name, function)?,
                function,
            );
        let entry = schema
            .reserve_gc_local::<crate::gc_types::NamedBinding, Nullable>(function)
            .initialize_null(schema, function);
        self.emit_global_lexical_entry_to_local(&key, &entry, function);
        entry.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        let object = schema.reserve_value_local(function);
        self.emit_execution_global_object_to_locals(&object, function);
        let property_key =
            crate::operations::PropertyKeyLocals::from_string(schema, &key, function);
        let base = self.runtime_helper_base()?;
        schema
            .call_helper(
                crate::runtime_helpers::ObjectHasPropertyArguments::new(
                    &object,
                    &property_key,
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(self.completion(), function);
        self.emit_propagate_current_throw_if_needed(function);
        self.completion().value().scalar().load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let error = schema.reserve_value_local(function);
        self.emit_throw_global_binding_error(
            GlobalBindingFailure::UnresolvableAssignment,
            &error,
            function,
        )?;
        self.emit_propagate_current_throw_if_needed(function);
        error.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let strict = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(1));
        strict.store(function);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectWriteArguments::new(
                    &object,
                    &property_key,
                    value,
                    strict,
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(self.completion(), function);
        self.emit_propagate_current_throw_if_needed(function);
        schema.release_i32_local(strict, function);
        property_key.clear(function);
        object.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_global_lexical_write(&entry, value, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        entry.clear(function);
        key.clear(function);
        Ok(())
    }

    pub(crate) fn emit_global_identifier_delete(
        &mut self,
        name: &str,
        result: I32Local,
        strictness: Strictness,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let key = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(
                self.emit_interned_string_reference(name, function)?,
                function,
            );
        let reference = self.emit_resolve_global_identifier(&key, strictness, function)?;
        self.emit_environment_identifier_delete(&reference, result, function)?;
        self.release_environment_identifier_reference(reference, function);
        key.clear(function);
        Ok(())
    }

    pub(crate) fn mirror_binding_to_global_object(
        &mut self,
        name: &str,
        storage: BindingStorage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if !self.has_global_script_bindings() || !self.is_script_global_binding(name) {
            return Ok(());
        }
        if self
            .binding_scopes
            .first()
            .and_then(|scope| scope.get(name))
            .is_none_or(|global_storage| *global_storage != storage)
        {
            return Ok(());
        }
        let saved = self.save_statement_list_value(function);
        let value = self.runtime_schema().reserve_value_local(function);
        self.read_binding_to_locals(storage, &value, function)?;
        self.emit_global_property_write(name, &value, function)?;
        value.clear(function);
        self.restore_statement_list_value(saved, function)
    }
}
