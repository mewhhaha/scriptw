use super::*;
use crate::gc_types::{
    NamedBinding, NamedBindingSchema, NamedBindingTable, NativeErrorObject, RealmRecordSchema,
    StringValue,
};
use crate::runtime_helpers::RuntimeErrorForThrowArguments;

// A Global Environment ends the lexical chain and roots its defining Realm.
// Declarative names retain the same BindingCell identities as captured reads.
pub(crate) enum GlobalBindingFailure<'a> {
    BeforeInitialization,
    Immutable,
    Unresolvable(&'a GcLocal<StringValue>),
}

pub(crate) enum SourceLiteralPrototype {
    Object,
    Array,
    RegExp,
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_alloc_realm_global_environment(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> Result<GcLocal<Environment>, EmitError> {
        let schema = self.runtime_schema();
        let environment = schema
            .reserve_gc_local::<Environment, NonNullable>(function)
            .initialize(
                schema.struct_type::<Environment>().construct(
                    (
                        GcOperand::nullable_reference(realm, schema),
                        GcOperand::null(schema),
                        GcOperand::i32(
                            named_environment::NamedEnvironmentKind::GlobalLexical.code(),
                        ),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                    ),
                    function,
                ),
                function,
            );
        schema
            .struct_type::<RealmRecord>()
            .field(RealmRecordSchema::GLOBAL_ENVIRONMENT)
            .write(
                realm,
                GcOperand::nullable_reference(&environment, schema),
                schema,
                function,
            );
        Ok(environment)
    }

    pub(crate) fn emit_source_global_environment_to_local(
        &mut self,
        function: &mut Function,
    ) -> GcLocal<Environment> {
        assert!(self.has_source_execution_environment());
        let schema = self.runtime_schema();
        // A source body's environments share one root, so the walk runs at
        // most once per invocation; later uses read the memoized root.
        function.instruction(&Instruction::Block(BlockType::Result(ValType::Ref(
            schema.reference_type::<Environment>(crate::gc_types::GcNullability::NonNullable),
        ))));
        let _ = self
            .source_global_environment_cache()
            .load(schema, function);
        function.instruction(&Instruction::BrOnNonNull(0));
        let environment = schema
            .reserve_gc_local::<Environment, NonNullable>(function)
            .initialize(
                self.current_environment()
                    .load(schema, function)
                    .require_non_null(function),
                function,
            );
        let root = self.emit_global_environment_from(&environment, function);
        environment.clear(function);
        self.source_global_environment_cache()
            .replace(root.load(schema, function).nullable(), function);
        let result = root.load(schema, function);
        root.clear(function);
        function.instruction(&Instruction::End);
        schema
            .reserve_gc_local(function)
            .initialize(result, function)
    }

    /// Walk the explicitly supplied source chain, never the active Realm fallback.
    pub(crate) fn emit_global_environment_from(
        &mut self,
        source: &GcLocal<Environment>,
        function: &mut Function,
    ) -> GcLocal<Environment> {
        let schema = self.runtime_schema();
        let environment = schema
            .reserve_gc_local(function)
            .initialize(source.load(schema, function), function);
        let parent = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize_null(schema, function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        parent.replace(
            schema
                .struct_type::<Environment>()
                .field(EnvironmentSchema::PARENT)
                .read(&environment, schema, function)
                .reference(),
            function,
        );
        parent.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::BrIf(1));
        environment.replace(
            parent.load(schema, function).require_non_null(function),
            function,
        );
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        parent.clear(function);
        environment
    }

    pub(crate) fn emit_source_execution_realm_to_local(
        &mut self,
        function: &mut Function,
    ) -> GcLocal<RealmRecord> {
        let schema = self.runtime_schema();
        let environment = self.emit_source_global_environment_to_local(function);
        let realm = schema
            .reserve_gc_local::<RealmRecord, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::DEFINING_REALM)
                    .read(&environment, schema, function)
                    .reference()
                    .require_non_null(function),
                function,
            );
        environment.clear(function);
        realm
    }

    fn emit_execution_realm_to_local(&mut self, function: &mut Function) -> GcLocal<RealmRecord> {
        if self.has_source_execution_environment() {
            self.emit_source_execution_realm_to_local(function)
        } else {
            self.load_current_realm(function)
        }
    }

    pub(crate) fn emit_execution_global_object_to_locals(
        &mut self,
        result: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let realm = self.emit_execution_realm_to_local(function);
        let stored = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<RealmRecord>()
                    .field(RealmRecordSchema::GLOBAL_OBJECT)
                    .read(&realm, schema, function)
                    .reference()
                    .require_non_null(function),
                function,
            );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, result, schema, function);
        stored.clear(function);
        realm.clear(function);
    }

    pub(crate) fn emit_source_literal_prototype_to_value(
        &mut self,
        prototype: SourceLiteralPrototype,
        result: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let realm = self.emit_execution_realm_to_local(function);
        match prototype {
            SourceLiteralPrototype::Object => self.emit_load_non_array_realm_intrinsic(
                &realm,
                NonArrayRealmIntrinsicSlot::ObjectPrototype,
                result,
                function,
            ),
            SourceLiteralPrototype::RegExp => self.emit_load_non_array_realm_intrinsic(
                &realm,
                NonArrayRealmIntrinsicSlot::RegExpPrototype,
                result,
                function,
            ),
            SourceLiteralPrototype::Array => {
                let array = schema
                    .reserve_gc_local::<ArrayObject, NonNullable>(function)
                    .initialize(
                        self.emit_load_realm_array_prototype(&realm, function),
                        function,
                    );
                result.set_reference(&array, schema, function);
                array.clear(function);
            }
        }
        realm.clear(function);
    }

    pub(crate) fn emit_initialize_main_global_lexicals(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if !self.is_main() {
            return Ok(());
        }
        self.emit_validate_main_global_lexical_declarations(function)?;
        let plan = self
            .script_global_bindings
            .expect("main global binding plan");
        let schema = self.runtime_schema();
        let global = self.emit_source_global_environment_to_local(function);
        let cells = self.resolve_env_handle_local(0, function);
        self.emit_append_global_lexical_cells(
            plan,
            self.owned_env_bindings,
            &global,
            &cells,
            function,
        )?;
        cells.clear(function);
        global.clear(function);
        Ok(())
    }

    /// Publish names that retain the already allocated static cells. Growing
    /// this table never replaces a lexical cell held by an escaping closure.
    pub(super) fn emit_append_global_lexical_cells<N: GcFieldNullability>(
        &mut self,
        plan: &scriptw_ir::GlobalBindingPlan,
        bindings: &[OwnedEnvBindingIr],
        global: &GcLocal<Environment, N>,
        cells: &GcLocal<Environment, Nullable>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if plan.lexical_bindings().is_empty() {
            return Ok(());
        }
        let schema = self.runtime_schema();
        let old_entries = schema
            .reserve_gc_local::<NamedBindingTable, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::NAMED_BINDINGS)
                    .read(global, schema, function)
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
        function.instruction(&Instruction::I32Const(
            i32::try_from(plan.lexical_bindings().len())
                .expect("planned global name table fits Wasm array indices"),
        ));
        function.instruction(&Instruction::I32Add);
        new_count.store(function);
        let entries = schema
            .reserve_gc_local::<NamedBindingTable, NonNullable>(function)
            .initialize(
                schema.array_type::<NamedBindingTable>().filled(
                    GcOperand::null(schema),
                    new_count,
                    function,
                ),
                function,
            );
        let cursor = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        cursor.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        cursor.load(function);
        old_count.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        let entry = schema
            .reserve_gc_local::<NamedBinding, Nullable>(function)
            .initialize(
                schema
                    .array_type::<NamedBindingTable>()
                    .read(&old_entries, cursor, schema, function)
                    .reference(),
                function,
            );
        schema.array_type::<NamedBindingTable>().write(
            &entries,
            cursor,
            GcOperand::reference(&entry, schema),
            schema,
            function,
        );
        entry.clear(function);
        cursor.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        cursor.store(function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        for name in plan.lexical_bindings().keys() {
            let slot = bindings
                .iter()
                .find(|binding| &binding.name == name)
                .unwrap_or_else(|| panic!("global lexical `{name}` must own its analyzed cell"))
                .slot;
            let key = schema
                .reserve_gc_local::<StringValue, NonNullable>(function)
                .initialize(
                    self.emit_interned_string_reference(name, function)?,
                    function,
                );
            let cell = self.emit_environment_cell_local(cells, slot, function);
            let entry = schema
                .reserve_gc_local::<NamedBinding, NonNullable>(function)
                .initialize(
                    schema.struct_type::<NamedBinding>().construct(
                        (
                            GcOperand::reference(&key, schema),
                            GcOperand::reference(&cell, schema),
                            GcOperand::boolean(false),
                            GcOperand::boolean(true),
                            GcOperand::boolean(true),
                        ),
                        function,
                    ),
                    function,
                );
            schema.array_type::<NamedBindingTable>().write(
                &entries,
                cursor,
                GcOperand::nullable_reference(&entry, schema),
                schema,
                function,
            );
            entry.clear(function);
            cell.clear(function);
            key.clear(function);
            cursor.load(function);
            function.instruction(&Instruction::I32Const(1));
            function.instruction(&Instruction::I32Add);
            cursor.store(function);
        }
        schema
            .struct_type::<Environment>()
            .field(EnvironmentSchema::NAMED_BINDINGS)
            .write(
                global,
                GcOperand::nullable_reference(&entries, schema),
                schema,
                function,
            );
        schema.release_i32_local(cursor, function);
        entries.clear(function);
        schema.release_i32_local(new_count, function);
        schema.release_i32_local(old_count, function);
        old_entries.clear(function);
        Ok(())
    }

    pub(crate) fn emit_global_lexical_entry_to_local(
        &mut self,
        key: &GcLocal<StringValue>,
        entry: &GcLocal<NamedBinding, Nullable>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        entry.set_null(schema, function);
        if !self.has_source_execution_environment() {
            return;
        }
        let environment = self.emit_source_global_environment_to_local(function);
        self.emit_find_own_named_binding(&environment, key, entry, function);
        environment.clear(function);
    }

    pub(crate) fn emit_global_lexical_read(
        &mut self,
        entry: &GcLocal<NamedBinding, Nullable>,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let cell = schema
            .reserve_gc_local::<BindingCell, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<NamedBinding>()
                    .field(NamedBindingSchema::CELL)
                    .read(entry, schema, function)
                    .reference(),
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

    pub(crate) fn emit_check_environment_cell_initialized(
        &mut self,
        cell: &GcLocal<BindingCell>,
        error: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::INITIALIZED)
            .read(cell, schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_global_binding_error(
            GlobalBindingFailure::BeforeInitialization,
            error,
            function,
        )?;
        self.emit_propagate_current_throw_if_needed(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// SetMutableBinding checks the own importing cell before immutability;
    /// it never observes an uninitialized exporter while rejecting an import write.
    pub(crate) fn emit_check_named_binding_initialized(
        &mut self,
        entry: &GcLocal<NamedBinding, Nullable>,
        error: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let cell = schema
            .reserve_gc_local::<BindingCell, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<NamedBinding>()
                    .field(NamedBindingSchema::CELL)
                    .read(entry, schema, function)
                    .reference(),
                function,
            );
        self.emit_check_environment_cell_initialized(&cell, error, function)?;
        cell.clear(function);
        Ok(())
    }

    pub(crate) fn emit_global_lexical_write(
        &mut self,
        entry: &GcLocal<NamedBinding, Nullable>,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let cell = schema
            .reserve_gc_local::<BindingCell, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<NamedBinding>()
                    .field(NamedBindingSchema::CELL)
                    .read(entry, schema, function)
                    .reference(),
                function,
            );
        self.emit_check_environment_cell_initialized(&cell, value, function)?;
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::MUTABLE)
            .read(&cell, schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_global_binding_error(GlobalBindingFailure::Immutable, value, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_write_environment_cell(&cell, value, function);
        cell.clear(function);
        Ok(())
    }

    pub(crate) fn emit_throw_global_binding_error(
        &mut self,
        failure: GlobalBindingFailure<'_>,
        error: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let (name, message) = match failure {
            GlobalBindingFailure::BeforeInitialization => (
                NativeErrorKind::ReferenceError,
                RuntimeErrorMessage::LEXICAL_BINDING_ACCESSED_BEFORE_INITIALIZATION,
            ),
            GlobalBindingFailure::Immutable => (
                NativeErrorKind::TypeError,
                RuntimeErrorMessage::ASSIGNMENT_TO_CONSTANT_BINDING,
            ),
            GlobalBindingFailure::Unresolvable(name) => {
                let schema = self.runtime_schema();
                let suffix = schema.reserve_gc_local(function).initialize(
                    self.emit_runtime_error_message_reference(
                        RuntimeErrorMessage::IDENTIFIER_IS_NOT_DEFINED_SUFFIX,
                        function,
                    )?,
                    function,
                );
                let message = schema.reserve_gc_local(function).initialize(
                    self.emit_concat_gc_strings(name, &suffix, function),
                    function,
                );
                let realm = self.emit_execution_realm(function);
                let prototype = schema.reserve_value_local(function);
                self.emit_load_non_array_realm_intrinsic(
                    &realm,
                    NonArrayRealmIntrinsicSlot::ReferenceErrorPrototype,
                    &prototype,
                    function,
                );
                let thrown = schema
                    .call_helper(
                        RuntimeErrorForThrowArguments::new(
                            NativeErrorKind::ReferenceError,
                            &prototype,
                            &message,
                        ),
                        self.runtime_helper_base()?,
                        function,
                    )
                    .bind(
                        schema,
                        schema.reserve_gc_local::<NativeErrorObject, _>(function),
                        function,
                    );
                error.set_reference(&thrown, schema, function);
                self.completion().set_throw(error, function);
                thrown.clear(function);
                prototype.clear(function);
                realm.clear(function);
                message.clear(function);
                suffix.clear(function);
                return Ok(());
            }
        };
        self.emit_environment_native_error(name, message, error, function)
    }
}
