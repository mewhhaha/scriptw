use super::*;

impl FunctionBuilder<'_> {
    pub(crate) fn compile_object_destructuring_operation_to_value(
        &mut self,
        operation: &scriptw_ir::ObjectDestructuringOperationIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        use crate::operations::ObjectDestructuringOperationKeys;

        let schema = self.runtime_schema();
        match self.compile_object_destructuring_operation_keys(operation, function)? {
            ObjectDestructuringOperationKeys::GetV {
                raw_receiver,
                boxed,
                key,
            } => {
                let raw = schema.reserve_value_local(function);
                let object = schema.reserve_value_local(function);
                let pending = schema.reserve_completion(function);
                self.compile_expr_to_value(raw_receiver, &raw, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                self.compile_expr_to_value(boxed, &object, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                self.emit_object_destructuring_get_v(
                    &object, &raw, &key, &pending, output, function,
                )?;
                pending.clear(function);
                object.clear(function);
                raw.clear(function);
                key.clear(function);
            }
            ObjectDestructuringOperationKeys::Rest { boxed, excluded } => {
                let object = schema.reserve_value_local(function);
                self.compile_expr_to_value(boxed, &object, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                let keys: Vec<_> = excluded.iter().collect();
                self.emit_copy_data_properties_rest(&object, &keys, output, function)?;
                object.clear(function);
                for key in excluded.into_iter().rev() {
                    key.clear(function);
                }
            }
            ObjectDestructuringOperationKeys::PutTarget { target, value } => {
                let assigned = schema.reserve_value_local(function);
                self.compile_expr_to_value(value, &assigned, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                // The checked target contains retained raw operands, not a replayed Reference.
                let prepared = self.prepare_destructuring_target(target, function)?;
                self.put_destructuring_target(prepared, &assigned, function)?;
                output.copy_from(&assigned, function);
                assigned.clear(function);
            }
        }
        Ok(())
    }

    pub(crate) fn compile_object_destructure_to_locals(
        &mut self,
        expression: &TypedExpr,
        pattern: &ObjectDestructuringPatternIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let source = self.runtime_schema().reserve_value_local(function);
        self.compile_expr_to_value(expression, &source, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.compile_object_destructure_from_value_locals(&source, pattern, output, function)?;
        source.clear(function);
        Ok(())
    }

    pub(super) fn compile_object_destructure_from_value_locals(
        &mut self,
        source: &ValueLocals,
        pattern: &ObjectDestructuringPatternIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.compile_nullish_tagged_i32(source.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        let failure = schema.reserve_completion(function);
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_DESTRUCTURE_UNDEFINED_OR_NULL,
            &failure,
            function,
        )?;
        self.completion().copy_from(&failure, function);
        failure.clear(function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let boxed = schema.reserve_completion(function);
        self.emit_value_to_object_locals(source, &boxed, function)?;
        self.completion().copy_from(&boxed, function);
        self.emit_propagate_current_throw_if_needed(function);
        let value = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let mut excluded_keys = Vec::with_capacity(pattern.properties.len());
        for property in &pattern.properties {
            let key = match &property.key {
                DestructuringPropertyKeyIr::Static(name) => {
                    self.emit_control_flow_string_key(name, function)?
                }
                DestructuringPropertyKeyIr::Computed(expression) => {
                    let raw = schema.reserve_value_local(function);
                    self.compile_expr_to_value(expression, &raw, function)?;
                    self.emit_propagate_current_throw_if_needed(function);
                    let key = self.emit_value_to_property_key_locals(&raw, function)?;
                    raw.clear(function);
                    key
                }
            };
            let target = self.prepare_destructuring_target(&property.target, function)?;
            self.emit_object_destructuring_get_v(
                boxed.value(),
                source,
                &key,
                &pending,
                &value,
                function,
            )?;
            if let Some(default) = &property.default {
                value.tag().load(function);
                function.instruction(&Instruction::I32Const(
                    WasmRuntimeValueTag::Undefined as i32,
                ));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                self.compile_expr_to_value(default, &value, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            self.put_destructuring_target(target, &value, function)?;
            excluded_keys.push(key);
        }
        if let Some(rest) = &pattern.rest {
            let target = self.prepare_destructuring_target(rest, function)?;
            let excluded: Vec<_> = excluded_keys.iter().collect();
            self.emit_copy_data_properties_rest(boxed.value(), &excluded, &value, function)?;
            self.put_destructuring_target(target, &value, function)?;
        }
        output.copy_from(source, function);
        for key in excluded_keys.into_iter().rev() {
            key.clear(function);
        }
        pending.clear(function);
        value.clear(function);
        boxed.clear(function);
        Ok(())
    }

    fn emit_copy_data_properties_rest(
        &mut self,
        source: &ValueLocals,
        excluded_keys: &[&crate::operations::PropertyKeyLocals],
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let realm = self.emit_execution_realm(function);
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            crate::functions::NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &prototype,
            function,
        );
        let object = schema
            .reserve_gc_local::<OrdinaryObject, NonNullable>(function)
            .initialize(
                self.emit_alloc_compact_object_with_prototype(&prototype, function)?,
                function,
            );
        prototype.clear(function);
        realm.clear(function);
        output.set_reference(&object, schema, function);
        let pending = schema.reserve_completion(function);
        self.emit_copy_data_properties_into(source, excluded_keys, output, &pending, function)?;
        self.completion().copy_from(&pending, function);
        pending.clear(function);
        object.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }

    fn emit_object_destructuring_get_v(
        &mut self,
        boxed: &ValueLocals,
        raw_receiver: &ValueLocals,
        key: &crate::operations::PropertyKeyLocals,
        pending: &CompletionLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        // GetV boxes the lookup base and retains the original primitive receiver.
        self.emit_object_read(boxed, raw_receiver, key, pending, function)?;
        self.completion().copy_from(pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        output.copy_from(pending.value(), function);
        Ok(())
    }
}
