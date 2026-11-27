use super::*;
mod table;

impl FunctionBuilder<'_> {
    pub(in super::super) fn emit_weak_map_operation(
        &mut self,
        op: MapOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let record = self.emit_weak_receiver::<WeakMapObject>(WeakFamily::Map, f)?;
        let key = s.reserve_value_local(f);
        let argument = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let present = s.reserve_i32_local(f);
        let identity = s.reserve_i64_local(f);
        let lookup_only = matches!(op, MapOperation::Get | MapOperation::Has | MapOperation::Delete);
        self.emit_builtin_arg_to_value(0, &key, f);
        if !lookup_only {
            self.emit_builtin_arg_to_value(1, &argument, f);
        }
        value.set_undefined(f);
        present.set_constant(0, f);
        self.emit_whole_can_be_held_weakly_i32(&key, f);
        if lookup_only {
            self.open_frame(ControlFrameKind::If, f);
        } else {
            f.instruction(&Instruction::I32Eqz);
            self.emit_weak_assert(
                RuntimeErrorMessage::WEAKMAP_KEY_MUST_BE_AN_OBJECT_OR_UNREGISTERED_SYMBOL,
                f,
            )?;
        }

        if matches!(op, MapOperation::GetOrInsertComputed) {
            self.emit_collection_assert_callable(
                &argument,
                RuntimeErrorMessage::WEAKMAP_PROTOTYPE_GETORINSERTCOMPUTED_CALLBACK_MUST_BE_CALLABLE,
                f,
            )?;
        }
        self.emit_weak_key_identity(&key, identity, !lookup_only, f);
        match op {
            MapOperation::Set => {
                self.emit_weak_map_put(&record, &key, identity, &argument, f)?;
                value.set_reference(&record, s, f);
            }
            MapOperation::Get | MapOperation::Has | MapOperation::Delete => {
                self.emit_weak_map_lookup(op, &record, &key, identity, &value, present, f);
            }
            MapOperation::GetOrInsert | MapOperation::GetOrInsertComputed => {
                self.emit_weak_map_lookup(op, &record, &key, identity, &value, present, f);
                present.load(f);
                f.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, f);
                match op {
                    MapOperation::GetOrInsert => value.copy_from(&argument, f),
                    MapOperation::GetOrInsertComputed => {
                        let undefined = s.reserve_value_local(f);
                        undefined.set_undefined(f);
                        let pending = s.reserve_completion(f);
                        let args = self.emit_pre_evaluated_arg_vector(&[&key], f);
                        self.emit_function_or_proxy_call_with_argv(
                            &argument, &undefined, &args, &pending, f,
                        )?;
                        args.clear(f);
                        self.emit_collection_propagate(&pending, f);
                        value.copy_from(pending.value(), f);
                        pending.clear(f);
                        undefined.clear(f);
                    }
                    MapOperation::Get
                    | MapOperation::Has
                    | MapOperation::Delete
                    | MapOperation::Set => unreachable!(),
                }
                // The callback can resize the table or change this entry. Use
                // only the still-rooted key/identity and probe the current table.
                self.emit_weak_map_put(&record, &key, identity, &value, f)?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        if lookup_only {
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        match op {
            MapOperation::Has | MapOperation::Delete => self.emit_collection_boolean(present, f),
            MapOperation::Get
            | MapOperation::Set
            | MapOperation::GetOrInsert
            | MapOperation::GetOrInsertComputed => self.completion().set_normal(&value, f),
        }
        s.release_i64_local(identity, f);
        s.release_i32_local(present, f);
        value.clear(f);
        argument.clear(f);
        key.clear(f);
        record.clear(f);
        Ok(())
    }
}
