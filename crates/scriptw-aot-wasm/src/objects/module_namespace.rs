//! Module namespace exotic methods retain private export readers and distinguish
//! presence from observable binding reads on the canonical GC namespace record.

use super::*;
use crate::gc_types::{
    FunctionObject, I32Local, ModuleExport, ModuleExportSchema, ModuleExportTable,
    ModuleNamespaceObject, ModuleNamespaceObjectSchema, ValueArray,
};
use scriptw_ir::ModuleNamespaceModeIr;

#[derive(Clone, Copy)]
pub(super) enum NamespaceBindingRead {
    Presence,
    Value,
}
#[derive(Clone, Copy)]
pub(crate) enum NamespaceOwnKeys {
    Strings,
    Symbols,
    All,
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_module_namespace(
        &mut self,
        mode: ModuleNamespaceModeIr,
        exports: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let ExprIr::ArrayLiteral(elements) = &exports.expr else {
            return Err(EmitError::unsupported(
                "namespace construction requires its private export table",
            ));
        };
        assert!(
            elements.len() % 2 == 1,
            "namespace table has evaluator and export pairs"
        );
        let schema = self.runtime_schema();
        let evaluator_value = schema.reserve_value_local(function);
        self.compile_expr_to_value(&elements[0], &evaluator_value, function)?;
        let evaluator = schema
            .reserve_gc_local::<FunctionObject, Nullable>(function)
            .initialize_null(schema, function);
        match mode {
            ModuleNamespaceModeIr::Eager => assert_eq!(elements[0].kind, ValueKind::Undefined),
            ModuleNamespaceModeIr::Deferred => {
                assert_eq!(elements[0].kind, ValueKind::Function);
                evaluator.replace(
                    evaluator_value
                        .cast_reference::<FunctionObject>(schema, function)
                        .nullable(),
                    function,
                );
            }
        }
        let mut rows = Vec::with_capacity(elements.len() / 2);
        for pair in elements[1..].chunks_exact(2) {
            let name_value = schema.reserve_value_local(function);
            let reader_value = schema.reserve_value_local(function);
            self.compile_expr_to_value(&pair[0], &name_value, function)?;
            self.compile_expr_to_value(&pair[1], &reader_value, function)?;
            let name = schema.reserve_gc_local(function).initialize(
                name_value.cast_reference::<crate::gc_types::StringValue>(schema, function),
                function,
            );
            let reader = schema.reserve_gc_local(function).initialize(
                reader_value.cast_reference::<FunctionObject>(schema, function),
                function,
            );
            let row = schema.reserve_gc_local(function).initialize(
                schema.struct_type::<ModuleExport>().construct(
                    (
                        GcOperand::reference(&name, schema),
                        GcOperand::reference(&reader, schema),
                    ),
                    function,
                ),
                function,
            );
            reader.clear(function);
            name.clear(function);
            reader_value.clear(function);
            name_value.clear(function);
            rows.push(row);
        }
        let table = schema.reserve_gc_local(function).initialize(
            schema.array_type::<ModuleExportTable>().fixed(
                rows.iter().map(|row| GcOperand::reference(row, schema)),
                function,
            ),
            function,
        );
        for row in rows.into_iter().rev() {
            row.clear(function);
        }
        self.emit_module_namespace_object(mode, &evaluator, &table, output, function)?;
        table.clear(function);
        evaluator.clear(function);
        evaluator_value.clear(function);
        Ok(())
    }

    /// An already evaluated native host module owns an empty namespace without
    /// inventing an ECMAScript activation or a deferred evaluator function.
    pub(crate) fn emit_empty_module_namespace(
        &mut self,
        mode: ModuleNamespaceModeIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let evaluator = schema
            .reserve_gc_local::<FunctionObject, Nullable>(function)
            .initialize_null(schema, function);
        let table = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ModuleExportTable>()
                .fixed(std::iter::empty(), function),
            function,
        );
        self.emit_module_namespace_object(mode, &evaluator, &table, output, function)?;
        table.clear(function);
        evaluator.clear(function);
        Ok(())
    }

    fn emit_module_namespace_object(
        &mut self,
        mode: ModuleNamespaceModeIr,
        evaluator: &GcLocal<FunctionObject, Nullable>,
        table: &GcLocal<ModuleExportTable>,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(None, function)?,
            function,
        );
        schema
            .struct_type::<OrdinaryObject>()
            .field(OrdinaryObjectSchema::EXTENSIBLE)
            .write(&header, GcOperand::boolean(false), schema, function);
        let tag_value = schema.reserve_value_local(function);
        let tag_string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(
                match mode {
                    ModuleNamespaceModeIr::Eager => "Module",
                    ModuleNamespaceModeIr::Deferred => "Deferred Module",
                },
                function,
            )?,
            function,
        );
        tag_value.set_reference(&tag_string, schema, function);
        tag_string.clear(function);
        let tag_symbol = schema.reserve_gc_local(function).initialize(
            self.emit_well_known_symbol_reference(
                scriptw_ir::WellKnownSymbol::ToStringTag,
                function,
            )?,
            function,
        );
        let tag_key = PropertyKeyLocals::from_symbol(schema, &tag_symbol, function);
        tag_symbol.clear(function);
        self.emit_object_append_data_property_with_flags(
            &header, &tag_key, &tag_value, false, false, false, function,
        )?;
        tag_key.clear(function);
        tag_value.clear(function);
        let namespace = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<ModuleNamespaceObject>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::reference(evaluator, schema),
                    GcOperand::reference(table, schema),
                ),
                function,
            ),
            function,
        );
        output.set_reference(&namespace, schema, function);
        namespace.clear(function);
        header.clear(function);
        Ok(())
    }

    pub(crate) fn emit_is_module_namespace_i32(
        &self,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        value.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            self.runtime_schema()
                .reference_type::<ModuleNamespaceObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
    }

    fn emit_namespace_evaluate(
        &mut self,
        object: &GcLocal<ModuleNamespaceObject>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let evaluator = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleNamespaceObject>()
                .field(ModuleNamespaceObjectSchema::DEFERRED_EVALUATOR)
                .read(object, schema, function)
                .reference(),
            function,
        );
        result.initialize(function);
        evaluator.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let callable = schema.reserve_value_local(function);
        let non_null = schema.reserve_gc_local(function).initialize(
            evaluator.load(schema, function).require_non_null(function),
            function,
        );
        callable.set_reference(&non_null, schema, function);
        let arguments = self.emit_pre_evaluated_arg_vector(&[], function);
        self.emit_function_handle_call_with_argv_inner(
            &callable,
            None,
            &arguments,
            result,
            PropagateCallThrow::LeaveInCompletion,
            function,
        )?;
        arguments.clear(function);
        non_null.clear(function);
        callable.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        evaluator.clear(function);
        Ok(())
    }

    /// A presence request evaluates deferred modules but never calls an export
    /// reader. In particular, eager uninitialized exports remain present.
    pub(super) fn emit_namespace_property(
        &mut self,
        object: &GcLocal<ModuleNamespaceObject>,
        key: &PropertyKeyLocals,
        read: NamespaceBindingRead,
        present: I32Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        result.initialize(function);
        function.instruction(&Instruction::I32Const(0));
        present.store(function);
        let evaluator = schema
            .reserve_gc_local::<FunctionObject, Nullable>(function)
            .initialize_null(schema, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        key.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let header = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleNamespaceObject>()
                .field(ModuleNamespaceObjectSchema::OBJECT)
                .read(object, schema, function)
                .reference(),
            function,
        );
        let descriptor = self.emit_ordinary_own_descriptor_reference(&header, key, function)?;
        descriptor.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        present.store(function);
        if matches!(read, NamespaceBindingRead::Value) {
            present.load(function);
            self.open_frame(ControlFrameKind::If, function);
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<PropertyDescriptor>()
                    .field(PropertyDescriptorSchema::VALUE)
                    .read(&descriptor, schema, function)
                    .reference(),
                function,
            );
            self.emit_property_value_into(&stored, result.value(), function);
            stored.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        descriptor.clear(function);
        header.clear(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        evaluator.replace(
            schema
                .struct_type::<ModuleNamespaceObject>()
                .field(ModuleNamespaceObjectSchema::DEFERRED_EVALUATOR)
                .read(object, schema, function)
                .reference(),
            function,
        );
        evaluator.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let then_string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("then", function)?,
            function,
        );
        let key_string = schema.reserve_gc_local(function).initialize(
            key.value()
                .cast_reference::<crate::gc_types::StringValue>(schema, function),
            function,
        );
        self.emit_string_payload_equality_i32(&key_string, &then_string, function);
        then_string.clear(function);
        key_string.clear(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_namespace_evaluate(object, result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.initialize(function);
        let table = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleNamespaceObject>()
                .field(ModuleNamespaceObjectSchema::EXPORTS)
                .read(object, schema, function)
                .reference(),
            function,
        );
        let index = schema.reserve_i32_local(function);
        let length = schema.reserve_i32_local(function);
        schema
            .array_type::<ModuleExportTable>()
            .length(&table, schema, function);
        length.store(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let row = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ModuleExportTable>()
                .read(&table, index, schema, function)
                .reference(),
            function,
        );
        let name = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleExport>()
                .field(ModuleExportSchema::NAME)
                .read(&row, schema, function)
                .reference(),
            function,
        );
        let key_string = schema.reserve_gc_local(function).initialize(
            key.value()
                .cast_reference::<crate::gc_types::StringValue>(schema, function),
            function,
        );
        self.emit_string_payload_equality_i32(&name, &key_string, function);
        present.store(function);
        key_string.clear(function);
        name.clear(function);
        present.load(function);
        self.open_frame(ControlFrameKind::If, function);
        if matches!(read, NamespaceBindingRead::Value) {
            let reader = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<ModuleExport>()
                    .field(ModuleExportSchema::READER)
                    .read(&row, schema, function)
                    .reference(),
                function,
            );
            let callable = schema.reserve_value_local(function);
            callable.set_reference(&reader, schema, function);
            let arguments = self.emit_pre_evaluated_arg_vector(&[], function);
            self.emit_function_handle_call_with_argv_inner(
                &callable,
                None,
                &arguments,
                result,
                PropagateCallThrow::LeaveInCompletion,
                function,
            )?;
            arguments.clear(function);
            callable.clear(function);
            reader.clear(function);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        row.clear(function);
        present.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(length, function);
        schema.release_i32_local(index, function);
        table.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        evaluator.clear(function);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    /// Own-key enumeration returns the actual internal List; public Reflect and
    /// Object result creation is owned by their ArrayCreate consumer.
    pub(crate) fn emit_namespace_own_keys(
        &mut self,
        object: &GcLocal<ModuleNamespaceObject>,
        keys: NamespaceOwnKeys,
        function: &mut Function,
    ) -> Result<GcLocal<ValueArray>, EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        self.emit_namespace_evaluate(object, &pending, function)?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let table = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleNamespaceObject>()
                .field(ModuleNamespaceObjectSchema::EXPORTS)
                .read(object, schema, function)
                .reference(),
            function,
        );
        let length = schema.reserve_i32_local(function);
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let value = schema.reserve_value_local(function);
        schema
            .array_type::<ModuleExportTable>()
            .length(&table, schema, function);
        length.store(function);
        match keys {
            NamespaceOwnKeys::Strings => length.load(function),
            NamespaceOwnKeys::Symbols => {
                function.instruction(&Instruction::I32Const(1));
            }
            NamespaceOwnKeys::All => {
                length.load(function);
                function.instruction(&Instruction::I32Const(-1));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                function.instruction(&Instruction::Unreachable);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                length.load(function);
                function.instruction(&Instruction::I32Const(1));
                function.instruction(&Instruction::I32Add);
            }
        }
        count.store(function);
        value.set_undefined(function);
        let initial = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&value, function),
            function,
        );
        let list = schema.reserve_gc_local(function).initialize(
            schema.array_type::<ValueArray>().filled(
                GcOperand::reference(&initial, schema),
                count,
                function,
            ),
            function,
        );
        initial.clear(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        if !matches!(keys, NamespaceOwnKeys::Symbols) {
            let done = self.open_frame(ControlFrameKind::Block, function);
            let next = self.open_frame(ControlFrameKind::Loop, function);
            index.load(function);
            length.load(function);
            function.instruction(&Instruction::I32GeU);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_branch_to_target(done, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            let row = schema.reserve_gc_local(function).initialize(
                schema
                    .array_type::<ModuleExportTable>()
                    .read(&table, index, schema, function)
                    .reference(),
                function,
            );
            let name = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<ModuleExport>()
                    .field(ModuleExportSchema::NAME)
                    .read(&row, schema, function)
                    .reference(),
                function,
            );
            value.set_reference(&name, schema, function);
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(&value, function),
                function,
            );
            schema.array_type::<ValueArray>().write(
                &list,
                index,
                GcOperand::reference(&stored, schema),
                schema,
                function,
            );
            stored.clear(function);
            name.clear(function);
            row.clear(function);
            index.load(function);
            function.instruction(&Instruction::I32Const(1));
            function.instruction(&Instruction::I32Add);
            index.store(function);
            self.emit_branch_to_target(next, function);
            self.pop_control(ControlFrameKind::Loop);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
        }
        if !matches!(keys, NamespaceOwnKeys::Strings) {
            let symbol = schema.reserve_gc_local(function).initialize(
                self.emit_well_known_symbol_reference(
                    scriptw_ir::WellKnownSymbol::ToStringTag,
                    function,
                )?,
                function,
            );
            value.set_reference(&symbol, schema, function);
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(&value, function),
                function,
            );
            schema.array_type::<ValueArray>().write(
                &list,
                index,
                GcOperand::reference(&stored, schema),
                schema,
                function,
            );
            stored.clear(function);
            symbol.clear(function);
        }
        value.clear(function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        schema.release_i32_local(length, function);
        table.clear(function);
        pending.clear(function);
        Ok(list)
    }

    pub(crate) fn emit_namespace_own_descriptor(
        &mut self,
        object: &GcLocal<ModuleNamespaceObject>,
        key: &PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyDescriptor, Nullable>, EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        let present = schema.reserve_i32_local(function);
        let result = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(function)
            .initialize_null(schema, function);
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        self.emit_namespace_property(
            object,
            key,
            NamespaceBindingRead::Value,
            present,
            &pending,
            function,
        )?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        present.load(function);
        self.open_frame(ControlFrameKind::If, function);
        key.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let descriptor = self.emit_alloc_property_descriptor(
            StoredPropertyAttributes::Data {
                writable: false,
                enumerable: false,
                configurable: false,
            },
            pending.value(),
            &undefined,
            &undefined,
            function,
        );
        result.replace(descriptor.load(schema, function).nullable(), function);
        descriptor.clear(function);
        function.instruction(&Instruction::Else);
        let descriptor = self.emit_alloc_property_descriptor(
            StoredPropertyAttributes::Data {
                writable: true,
                enumerable: true,
                configurable: false,
            },
            pending.value(),
            &undefined,
            &undefined,
            function,
        );
        result.replace(descriptor.load(schema, function).nullable(), function);
        descriptor.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        undefined.clear(function);
        schema.release_i32_local(present, function);
        pending.clear(function);
        Ok(result)
    }
}
