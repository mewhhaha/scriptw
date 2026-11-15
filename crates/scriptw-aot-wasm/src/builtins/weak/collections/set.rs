use super::*;
impl FunctionBuilder<'_> {
    pub(in super::super) fn emit_weak_set_operation(
        &mut self,
        op: SetOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let record = self.emit_weak_receiver::<WeakSetObject>(WeakFamily::Set, f)?;
        let key = s.reserve_value_local(f);
        let result = s.reserve_value_local(f);
        let found = s.reserve_i32_local(f);
        let identities = WeakEntryIdentities {
            owner: s.reserve_i64_local(f),
            key: s.reserve_i64_local(f),
        };
        self.emit_builtin_arg_to_value(0, &key, f);
        found.set_constant(0, f);
        self.emit_whole_can_be_held_weakly_i32(&key, f);
        match op {
            SetOperation::Add => {
                f.instruction(&Instruction::I32Eqz);
                self.emit_weak_assert(
                    RuntimeErrorMessage::WEAKSET_VALUE_MUST_BE_AN_OBJECT_OR_UNREGISTERED_SYMBOL,
                    f,
                )?;
                let owner = s
                    .field(WeakSetObjectSchema::OBJECT)
                    .read(&record, s, f)
                    .reference();
                self.emit_weak_entry_identities(owner, &key, identities, true, f);
                self.weak_import(GcHostImport::WeakEntryPut)?
                    .put_weak_entry(&record, &key, None, identities, s, f);
                result.set_reference(&record, s, f);
                self.completion().set_normal(&result, f);
            }
            SetOperation::Has | SetOperation::Delete => {
                self.open_frame(ControlFrameKind::If, f);
                let import = match op {
                    SetOperation::Has => GcHostImport::WeakEntryHas,
                    SetOperation::Delete => GcHostImport::WeakEntryDelete,
                    SetOperation::Add => unreachable!(),
                };
                let owner = s
                    .field(WeakSetObjectSchema::OBJECT)
                    .read(&record, s, f)
                    .reference();
                self.emit_weak_entry_identities(owner, &key, identities, false, f);
                self.open_weak_entry_lookup(identities, f);
                self.weak_import(import)?
                    .weak_entry_membership(&record, &key, identities, found, s, f);
                for _ in 0..2 {
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                }
                self.emit_collection_boolean(found, f);
            }
        }
        s.release_i64_local(identities.key, f);
        s.release_i64_local(identities.owner, f);
        s.release_i32_local(found, f);
        result.clear(f);
        key.clear(f);
        record.clear(f);
        Ok(())
    }
}
