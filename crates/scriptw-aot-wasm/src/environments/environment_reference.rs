use super::global_environment::GlobalBindingFailure;
use super::named_environment::NamedEnvironmentKind;
use super::*;
use crate::gc_types::{
    NamedBinding, NamedBindingSchema, ObjectEnvironmentSchema, RealmRecordSchema,
};
use crate::operations::PropertyKeyLocals;
use crate::runtime_helpers::{
    ObjectHasPropertyArguments, ObjectReadArguments, OrdinarySetArguments,
};

mod captured_identifier_reference;
mod environment_identifier_get;
mod environment_identifier_put;
mod environment_identifier_read;
mod environment_identifier_resolve;
mod global_identifier_read;

#[derive(Clone, Copy)]
enum EnvironmentReferenceKind {
    Unresolvable,
    Declarative,
    GlobalLexical,
    WithObject,
    GlobalObject,
    DirectDeclarativeCell,
}
impl EnvironmentReferenceKind {
    const fn code(self) -> i32 {
        match self {
            Self::Unresolvable => 0,
            Self::Declarative => 1,
            Self::GlobalLexical => 2,
            Self::WithObject => 3,
            Self::GlobalObject => 4,
            Self::DirectDeclarativeCell => 5,
        }
    }
}

/// The selected record and source name survive RHS effects. Eval may change
/// this record's name table; PutValue re-resolves within this same record,
/// never through a newly changed parent chain.
#[must_use]
pub(crate) struct EnvironmentIdentifierReference {
    key: GcLocal<StringValue>,
    kind: I32Local,
    record: GcLocal<Environment, Nullable>,
    entry: GcLocal<NamedBinding, Nullable>,
    cell: GcLocal<BindingCell, Nullable>,
    base: ValueLocals,
    strictness: Strictness,
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_resolve_environment_identifier(
        &mut self,
        name: &GcLocal<StringValue>,
        strictness: Strictness,
        function: &mut Function,
    ) -> Result<EnvironmentIdentifierReference, EmitError> {
        let record = self.resolve_env_handle_local(0, function);
        self.emit_resolve_environment_identifier_from_record(name, strictness, record, function)
    }

    /// A statically located global fallback still performs ResolveBinding at
    /// runtime. Retain its Global Environment Record or unresolvable result
    /// before the RHS can change bindings. Global Get/Put selects that record's
    /// declarative or object delegate only when the operation actually runs.
    pub(crate) fn emit_resolve_global_identifier(
        &mut self,
        name: &GcLocal<StringValue>,
        strictness: Strictness,
        function: &mut Function,
    ) -> Result<EnvironmentIdentifierReference, EmitError> {
        let schema = self.runtime_schema();
        let global = self.emit_source_global_environment_to_local(function);
        let record = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize(global.load(schema, function).nullable(), function);
        global.clear(function);
        self.emit_resolve_environment_identifier_from_record(name, strictness, record, function)
    }

    fn emit_resolve_environment_identifier_body_from_record(
        &mut self,
        name: &GcLocal<StringValue>,
        strictness: Strictness,
        record: GcLocal<Environment, Nullable>,
        function: &mut Function,
    ) -> Result<EnvironmentIdentifierReference, EmitError> {
        let schema = self.runtime_schema();
        let reference = EnvironmentIdentifierReference {
            key: schema
                .reserve_gc_local::<StringValue, NonNullable>(function)
                .initialize(name.load(schema, function), function),
            kind: schema.reserve_i32_local(function),
            record,
            entry: schema
                .reserve_gc_local::<NamedBinding, Nullable>(function)
                .initialize_null(schema, function),
            cell: schema
                .reserve_gc_local::<BindingCell, Nullable>(function)
                .initialize_null(schema, function),
            base: schema.reserve_value_local(function),
            strictness,
        };
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::Unresolvable.code(),
        ));
        reference.kind.store(function);
        reference.base.set_undefined(function);
        let parent = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize_null(schema, function);
        let kind = schema.reserve_i32_local(function);
        let present = schema.reserve_i32_local(function);
        let property_key = PropertyKeyLocals::from_string(schema, &reference.key, function);
        let helper_base = self.runtime_helper_base()?;
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        parent.replace(
            schema
                .struct_type::<Environment>()
                .field(EnvironmentSchema::PARENT)
                .read(&reference.record, schema, function)
                .reference(),
            function,
        );
        parent.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_find_own_named_binding(
            &reference.record,
            &reference.key,
            &reference.entry,
            function,
        );
        let realm = schema
            .reserve_gc_local::<RealmRecord, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::DEFINING_REALM)
                    .read(&reference.record, schema, function)
                    .reference()
                    .require_non_null(function),
                function,
            );
        let global = schema
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
        self.emit_stored_value_to_locals(&global, &reference.base, function);
        global.clear(function);
        realm.clear(function);
        reference.entry.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .call_helper(
                ObjectHasPropertyArguments::new(
                    &reference.base,
                    &property_key,
                    self.current_environment(),
                    self.current_program(),
                ),
                helper_base,
                function,
            )
            .store(self.completion(), function);
        self.emit_propagate_current_throw_if_needed(function);
        self.completion().value().scalar().load(function);
        function.instruction(&Instruction::I32WrapI64);
        present.store(function);
        present.load(function);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::GlobalObject.code(),
        ));
        reference.kind.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::GlobalLexical.code(),
        ));
        reference.kind.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Br(2));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .struct_type::<Environment>()
            .field(EnvironmentSchema::KIND)
            .read(&reference.record, schema, function)
            .store(kind, function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(
            NamedEnvironmentKind::WithObject.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let object_environment = schema
            .reserve_gc_local::<ObjectEnvironment, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::OBJECT)
                    .read(&reference.record, schema, function)
                    .reference()
                    .require_non_null(function),
                function,
            );
        let cell = schema
            .reserve_gc_local::<BindingCell, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<ObjectEnvironment>()
                    .field(ObjectEnvironmentSchema::BINDING_OBJECT_CELL)
                    .read(&object_environment, schema, function)
                    .reference(),
                function,
            );
        let initialized = self.emit_read_environment_cell(&cell, &reference.base, function);
        schema.release_i32_local(initialized, function);
        cell.clear(function);
        object_environment.clear(function);
        self.emit_with_environment_has_binding(&reference.base, &reference.key, present, function)?;
        present.load(function);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::WithObject.code(),
        ));
        reference.kind.store(function);
        function.instruction(&Instruction::Br(3));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        self.emit_find_own_named_binding(
            &reference.record,
            &reference.key,
            &reference.entry,
            function,
        );
        reference.entry.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::Declarative.code(),
        ));
        reference.kind.store(function);
        function.instruction(&Instruction::Br(3));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        reference
            .record
            .replace(parent.load(schema, function), function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        property_key.clear(function);
        schema.release_i32_local(present, function);
        schema.release_i32_local(kind, function);
        parent.clear(function);
        Ok(reference)
    }

    /// Reads a name that resolves to an own data property of an ordinary
    /// global object, and reports whether it did. No record before the Global
    /// Environment may bind the name or be a `with` object, and the global
    /// lexical record must not declare it: HasProperty and Get then observe
    /// nothing but that property. Every other case reports false and leaves
    /// the search to ResolveBinding.
    pub(super) fn emit_global_own_data_identifier_read_i32(
        &mut self,
        name: &GcLocal<StringValue>,
        record: &GcLocal<Environment, Nullable>,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let found = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        found.store(function);
        let cursor = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize(record.load(schema, function), function);
        let parent = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize_null(schema, function);
        let entry = schema
            .reserve_gc_local::<NamedBinding, Nullable>(function)
            .initialize_null(schema, function);
        let kind = schema.reserve_i32_local(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let walk = self.open_frame(ControlFrameKind::Loop, function);
        parent.replace(
            schema
                .struct_type::<Environment>()
                .field(EnvironmentSchema::PARENT)
                .read(&cursor, schema, function)
                .reference(),
            function,
        );
        parent.load(schema, function).is_null(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_find_own_named_binding(&cursor, name, &entry, function);
        entry.load(schema, function).is_null(function);
        self.open_frame(ControlFrameKind::If, function);
        let realm = schema
            .reserve_gc_local::<RealmRecord, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::DEFINING_REALM)
                    .read(&cursor, schema, function)
                    .reference()
                    .require_non_null(function),
                function,
            );
        let global = schema
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
        let base = schema.reserve_value_local(function);
        self.emit_stored_value_to_locals(&global, &base, function);
        let key = PropertyKeyLocals::from_string(schema, name, function);
        let own = self.emit_ordinary_own_data_read_i32(&base, &key, value, function)?;
        own.load(function);
        found.store(function);
        schema.release_i32_local(own, function);
        key.clear(function);
        base.clear(function);
        global.clear(function);
        realm.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .struct_type::<Environment>()
            .field(EnvironmentSchema::KIND)
            .read(&cursor, schema, function)
            .store(kind, function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(
            NamedEnvironmentKind::WithObject.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(done, function);
        self.emit_find_own_named_binding(&cursor, name, &entry, function);
        entry.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(done, function);
        cursor.replace(parent.load(schema, function), function);
        self.emit_branch_to_target(walk, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(kind, function);
        entry.clear(function);
        parent.clear(function);
        cursor.clear(function);
        Ok(found)
    }

    /// A resolved global Reference holds the Global Environment Record, not
    /// whichever subrecord answered its original HasBinding. RHS execution or
    /// a preceding HasBinding trap can create a lexical shadow. Refresh only
    /// delegation within the held record; an unresolvable Reference stays so.
    fn emit_refresh_global_identifier_entry(
        &mut self,
        reference: &EnvironmentIdentifierReference,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::GlobalLexical.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::GlobalObject.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_find_own_named_binding(
            &reference.record,
            &reference.key,
            &reference.entry,
            function,
        );
        reference.entry.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::GlobalObject.code(),
        ));
        reference.kind.store(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::GlobalLexical.code(),
        ));
        reference.kind.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    /// The ordered IR owner already selected this binding object. No
    /// HasBinding is repeated, even when RHS hooks later change unscopables.
    pub(crate) fn emit_selected_with_object_identifier_reference(
        &mut self,
        name: &GcLocal<StringValue>,
        object: &ValueLocals,
        strictness: Strictness,
        function: &mut Function,
    ) -> EnvironmentIdentifierReference {
        let schema = self.runtime_schema();
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
                .initialize_null(schema, function),
            base: schema.reserve_value_local(function),
            strictness,
        };
        reference.base.copy_from(object, function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::WithObject.code(),
        ));
        reference.kind.store(function);
        reference
    }

    /// The ordered IR selection is already complete. Undefined selects the
    /// global record (or unresolvable result) now, before any RHS effects;
    /// otherwise the selected object supplies the complete held Reference.
    pub(crate) fn emit_resolve_selected_with_global_fallback_identifier(
        &mut self,
        name: &GcLocal<StringValue>,
        selected: &ValueLocals,
        strictness: Strictness,
        function: &mut Function,
    ) -> Result<EnvironmentIdentifierReference, EmitError> {
        let schema = self.runtime_schema();
        let reference = self
            .emit_selected_with_object_identifier_reference(name, selected, strictness, function);
        selected.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let fallback = self.emit_resolve_global_identifier(name, strictness, function)?;
        fallback.kind.load(function);
        reference.kind.store(function);
        reference
            .record
            .replace(fallback.record.load(schema, function), function);
        reference
            .entry
            .replace(fallback.entry.load(schema, function), function);
        reference
            .cell
            .replace(fallback.cell.load(schema, function), function);
        reference.base.copy_from(&fallback.base, function);
        self.release_environment_identifier_reference(fallback, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(reference)
    }

    pub(crate) fn release_environment_identifier_reference(
        &mut self,
        reference: EnvironmentIdentifierReference,
        function: &mut Function,
    ) {
        reference.base.clear(function);
        reference.cell.clear(function);
        reference.entry.clear(function);
        reference.record.clear(function);
        self.runtime_schema()
            .release_i32_local(reference.kind, function);
        reference.key.clear(function);
    }

    pub(crate) fn emit_environment_identifier_call_base(
        &mut self,
        reference: &EnvironmentIdentifierReference,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::WithObject.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        value.copy_from(&reference.base, function);
        function.instruction(&Instruction::Else);
        value.set_undefined(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }
}

#[derive(Clone, Copy)]
pub(crate) enum EnvironmentIdentifierRead {
    Value,
    Typeof,
}

impl FunctionBuilder<'_> {
    fn emit_environment_identifier_get_body(
        &mut self,
        reference: &EnvironmentIdentifierReference,
        read: EnvironmentIdentifierRead,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let property_key = PropertyKeyLocals::from_string(schema, &reference.key, function);
        let base = self.runtime_helper_base()?;
        self.emit_refresh_global_identifier_entry(reference, function);
        self.open_frame(ControlFrameKind::Block, function);
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::Unresolvable.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        match read {
            EnvironmentIdentifierRead::Value => {
                self.emit_throw_global_binding_error(
                    GlobalBindingFailure::Unresolvable(&reference.key),
                    value,
                    function,
                )?;
                self.emit_propagate_current_throw_if_needed(function);
            }
            EnvironmentIdentifierRead::Typeof => value.set_undefined(function),
        }
        function.instruction(&Instruction::Br(1));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::DirectDeclarativeCell.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_direct_identifier_cell_get(reference, value, function)?;
        function.instruction(&Instruction::Br(1));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::Declarative.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::GlobalLexical.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_global_lexical_read(&reference.entry, value, function)?;
        function.instruction(&Instruction::Br(1));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .call_helper(
                ObjectHasPropertyArguments::new(
                    &reference.base,
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
        match reference.strictness {
            Strictness::Strict => {
                reference.kind.load(function);
                function.instruction(&Instruction::I32Const(
                    EnvironmentReferenceKind::WithObject.code(),
                ));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_environment_native_error(
                    NativeErrorKind::ReferenceError,
                    RuntimeErrorMessage::OBJECT_ENVIRONMENT_BINDING_MISSING,
                    value,
                    function,
                )?;
                function.instruction(&Instruction::Else);
                self.emit_throw_global_binding_error(
                    GlobalBindingFailure::Unresolvable(&reference.key),
                    value,
                    function,
                )?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                self.emit_propagate_current_throw_if_needed(function);
            }
            Strictness::Sloppy => value.set_undefined(function),
        }
        function.instruction(&Instruction::Br(1));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .call_helper(
                ObjectReadArguments::new(
                    &reference.base,
                    &reference.base,
                    &property_key,
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(self.completion(), function);
        self.emit_propagate_current_throw_if_needed(function);
        value.copy_from(self.completion().value(), function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        property_key.clear(function);
        Ok(())
    }

    fn emit_environment_identifier_put_body(
        &mut self,
        reference: &EnvironmentIdentifierReference,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let property_key = PropertyKeyLocals::from_string(schema, &reference.key, function);
        let base = self.runtime_helper_base()?;
        let error = schema.reserve_value_local(function);
        self.emit_refresh_global_identifier_entry(reference, function);
        self.open_frame(ControlFrameKind::Block, function);
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::DirectDeclarativeCell.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_direct_identifier_cell_put(reference, value, function)?;
        function.instruction(&Instruction::Br(1));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::Declarative.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_set_named_environment_binding(
            &reference.record,
            &reference.key,
            reference.strictness,
            value,
            function,
        )?;
        function.instruction(&Instruction::Br(1));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::GlobalLexical.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_global_lexical_write(&reference.entry, value, function)?;
        function.instruction(&Instruction::Br(1));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        if reference.strictness.throws_on_failed_set() {
            reference.kind.load(function);
            function.instruction(&Instruction::I32Const(
                EnvironmentReferenceKind::Unresolvable.code(),
            ));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_throw_global_binding_error(
                GlobalBindingFailure::Unresolvable(&reference.key),
                &error,
                function,
            )?;
            self.emit_propagate_current_throw_if_needed(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        // Object environments recheck their held object after RHS effects.
        // Unresolvable sloppy PutValue goes straight to the global object's Set.
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::Unresolvable.code(),
        ));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .call_helper(
                ObjectHasPropertyArguments::new(
                    &reference.base,
                    &property_key,
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(self.completion(), function);
        self.emit_propagate_current_throw_if_needed(function);
        if reference.strictness.throws_on_failed_set() {
            self.completion().value().scalar().load(function);
            function.instruction(&Instruction::I64Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_throw_global_binding_error(
                GlobalBindingFailure::Unresolvable(&reference.key),
                &error,
                function,
            )?;
            self.emit_propagate_current_throw_if_needed(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .call_helper(
                OrdinarySetArguments::new(
                    &reference.base,
                    &reference.base,
                    &property_key,
                    value,
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(self.completion(), function);
        self.emit_propagate_current_throw_if_needed(function);
        if reference.strictness.throws_on_failed_set() {
            self.completion().value().scalar().load(function);
            function.instruction(&Instruction::I64Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_environment_native_error(
                NativeErrorKind::TypeError,
                RuntimeErrorMessage::CANNOT_ASSIGN_TO_READ_ONLY_PROPERTY,
                &error,
                function,
            )?;
            self.emit_propagate_current_throw_if_needed(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        error.clear(function);
        property_key.clear(function);
        Ok(())
    }

    pub(crate) fn emit_environment_identifier_delete(
        &mut self,
        reference: &EnvironmentIdentifierReference,
        result: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.emit_refresh_global_identifier_entry(reference, function);
        function.instruction(&Instruction::I32Const(1));
        result.store(function);
        self.open_frame(ControlFrameKind::Block, function);
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::Unresolvable.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::BrIf(0));
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::GlobalLexical.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::DirectDeclarativeCell.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(0));
        result.store(function);
        function.instruction(&Instruction::Br(1));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        reference.kind.load(function);
        function.instruction(&Instruction::I32Const(
            EnvironmentReferenceKind::Declarative.code(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_find_own_named_binding(
            &reference.record,
            &reference.key,
            &reference.entry,
            function,
        );
        reference.entry.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::BrIf(1));
        schema
            .struct_type::<NamedBinding>()
            .field(NamedBindingSchema::DELETABLE)
            .read(&reference.entry, schema, function)
            .store(result, function);
        result.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        schema
            .struct_type::<NamedBinding>()
            .field(NamedBindingSchema::PRESENT)
            .write(
                &reference.entry,
                GcOperand::boolean(false),
                schema,
                function,
            );
        function.instruction(&Instruction::Br(1));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let key = PropertyKeyLocals::from_string(schema, &reference.key, function);
        self.emit_delete_environment_object_property(&reference.base, &key, result, function)?;
        key.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        Ok(())
    }
}
