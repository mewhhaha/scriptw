use super::storage::CollectionCursorLocals;
use super::*;
#[must_use]
struct CompletedSetLikeRecord {
    object: ValueLocals,
    size: ValueLocals,
    has: ValueLocals,
    keys: ValueLocals,
}
impl CompletedSetLikeRecord {
    fn clear(self, f: &mut Function) {
        self.keys.clear(f);
        self.has.clear(f);
        self.size.clear(f);
        self.object.clear(f);
    }
}
#[derive(Clone, Copy)]
enum SetOperation {
    Difference,
    Intersection,
    SymmetricDifference,
    Union,
    Disjoint,
    Subset,
    Superset,
}
// A receiver walk cannot be supplied a keys-only operation, or vice versa.
#[derive(Clone, Copy)]
enum SetReceiverOperation {
    Difference,
    Intersection,
    Disjoint,
    Subset,
}
#[derive(Clone, Copy)]
enum SetKeysOperation {
    Difference,
    Intersection,
    SymmetricDifference,
    Union,
    Disjoint,
    Superset,
}
impl FunctionBuilder<'_> {
    fn emit_collection_set_like(
        &mut self,
        f: &mut Function,
    ) -> Result<CompletedSetLikeRecord, EmitError> {
        let s = self.runtime_schema();
        let object = s.reserve_value_local(f);
        let size = s.reserve_value_local(f);
        let has = s.reserve_value_local(f);
        let keys = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        self.emit_builtin_arg_to_value(0, &object, f);
        self.emit_is_heap_object_like_tag_i32(object.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::SET_METHOD_ARGUMENT_IS_NOT_A_SET_LIKE_OBJECT,
            &pending,
            f,
        )?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_collection_get(&object, "size", &pending, f)?;
        self.emit_collection_propagate(&pending, f);
        size.copy_from(pending.value(), f);
        self.emit_value_to_number_payload(&size, &pending, f)?;
        self.emit_collection_propagate(&pending, f);
        size.copy_from(pending.value(), f);
        size.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        size.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::SET_LIKE_SIZE_IS_NAN,
            &pending,
            f,
        )?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        size.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Trunc);
        f.instruction(&Instruction::I64ReinterpretF64);
        size.scalar().store(f);
        size.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(0.0.into()));
        f.instruction(&Instruction::F64Lt);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::SET_LIKE_SIZE_IS_NEGATIVE,
            &pending,
            f,
        )?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_collection_get(&object, "has", &pending, f)?;
        self.emit_collection_propagate(&pending, f);
        has.copy_from(pending.value(), f);
        self.emit_collection_assert_callable(
            &has,
            RuntimeErrorMessage::SET_LIKE_HAS_METHOD_IS_NOT_CALLABLE,
            f,
        )?;
        self.emit_collection_get(&object, "keys", &pending, f)?;
        self.emit_collection_propagate(&pending, f);
        keys.copy_from(pending.value(), f);
        self.emit_collection_assert_callable(
            &keys,
            RuntimeErrorMessage::SET_LIKE_KEYS_METHOD_IS_NOT_CALLABLE,
            f,
        )?;
        pending.clear(f);
        Ok(CompletedSetLikeRecord {
            object,
            size,
            has,
            keys,
        })
    }
    fn emit_collection_other_keys(
        &mut self,
        other: &CompletedSetLikeRecord,
        f: &mut Function,
    ) -> Result<OwnedSyncIterator, EmitError> {
        let pending = self.runtime_schema().reserve_completion(f);
        let args = self.emit_pre_evaluated_arg_vector(&[], f);
        self.emit_function_or_proxy_call_with_argv(&other.keys, &other.object, &args, &pending, f)?;
        args.clear(f);
        self.emit_collection_propagate(&pending, f);
        let iterator =
            self.emit_get_sync_iterator_direct(pending.value(), SyncIteratorConsumer::SetLike, f)?;
        pending.clear(f);
        Ok(iterator)
    }
    fn emit_collection_copy_set(
        &mut self,
        source: &GcLocal<SetObject>,
        target: &GcLocal<SetObject>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let cursor = CollectionCursorLocals::reserve(s, f);
        let found = s.reserve_i32_local(f);
        let value = s.reserve_value_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_collection_next_set(source, &cursor, found, f)?;
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        self.emit_collection_read_set(source, cursor.index, &value, f);
        self.emit_increment_local(cursor.index, 1, f);
        self.emit_collection_put_set(target, &value, f)?;
        value.set_undefined(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.clear(f);
        s.release_i32_local(found, f);
        cursor.release(s, f);
        Ok(())
    }
    fn emit_collection_set_receiver_walk(
        &mut self,
        source: &GcLocal<SetObject>,
        result: &GcLocal<SetObject>,
        other: &CompletedSetLikeRecord,
        operation: SetReceiverOperation,
        boolean: I32Local,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let cursor = CollectionCursorLocals::reserve(s, f);
        let found = s.reserve_i32_local(f);
        let contained = s.reserve_i32_local(f);
        let value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        // Difference walks its private initial copy; other has callbacks cannot
        // enlarge it. Intersection/predicates walk the live original owner.
        let walked = match operation {
            SetReceiverOperation::Difference => result,
            SetReceiverOperation::Intersection
            | SetReceiverOperation::Disjoint
            | SetReceiverOperation::Subset => source,
        };
        self.emit_collection_next_set(walked, &cursor, found, f)?;
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(done, f);
        self.emit_collection_read_set(walked, cursor.index, &value, f);
        self.emit_increment_local(cursor.index, 1, f);
        let args = self.emit_pre_evaluated_arg_vector(&[&value], f);
        self.emit_function_or_proxy_call_with_argv(&other.has, &other.object, &args, &pending, f)?;
        args.clear(f);
        self.emit_collection_propagate(&pending, f);
        self.compile_truthy_tagged_i32(pending.value(), f)?;
        contained.store(f);
        match operation {
            SetReceiverOperation::Difference => {
                contained.load(f);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_collection_delete_set(result, &value, found, f)?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
            SetReceiverOperation::Intersection => {
                contained.load(f);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_collection_put_set(result, &value, f)?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
            SetReceiverOperation::Disjoint | SetReceiverOperation::Subset => {
                contained.load(f);
                if matches!(operation, SetReceiverOperation::Subset) {
                    f.instruction(&Instruction::I32Eqz);
                }
                self.open_frame(ControlFrameKind::If, f);
                f.instruction(&Instruction::I32Const(0));
                boolean.store(f);
                self.emit_branch_to_target(exit, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        value.set_undefined(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        pending.clear(f);
        value.clear(f);
        s.release_i32_local(contained, f);
        s.release_i32_local(found, f);
        cursor.release(s, f);
        Ok(())
    }
    fn emit_collection_set_keys_walk(
        &mut self,
        source: &GcLocal<SetObject>,
        result: &GcLocal<SetObject>,
        iterator: &OwnedSyncIterator,
        operation: SetKeysOperation,
        boolean: I32Local,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let index = s.reserve_i64_local(f);
        let found = s.reserve_i32_local(f);
        let done = s.reserve_i32_local(f);
        let value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let closed = s.reserve_completion(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_sync_iterator_step_value(iterator, done, &value, f)?;
        done.load(f);
        self.emit_branch_if_to_target(finished, f);
        self.emit_collection_normalize_zero(&value, f);
        match operation {
            SetKeysOperation::Difference => {
                self.emit_collection_delete_set(result, &value, found, f)?
            }
            SetKeysOperation::Union => self.emit_collection_put_set(result, &value, f)?,
            SetKeysOperation::Intersection => {
                let prepared = self.emit_collection_prepare_key(&value, f);
                self.emit_collection_find_prepared_set(source, &prepared, index, found, f)?;
                found.load(f);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_collection_put_prepared_set(result, &prepared, f)?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                prepared.release(self, f);
            }
            SetKeysOperation::SymmetricDifference => {
                // Membership consults the current receiver, not the copied result.
                // Duplicate iterator keys therefore do not repeatedly toggle.
                let prepared = self.emit_collection_prepare_key(&value, f);
                self.emit_collection_find_prepared_set(source, &prepared, index, found, f)?;
                found.load(f);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_collection_delete_prepared_set(result, &prepared, found, f)?;
                f.instruction(&Instruction::Else);
                self.emit_collection_put_prepared_set(result, &prepared, f)?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                prepared.release(self, f);
            }
            SetKeysOperation::Disjoint | SetKeysOperation::Superset => {
                self.emit_collection_find_set(source, &value, index, found, f)?;
                found.load(f);
                if matches!(operation, SetKeysOperation::Superset) {
                    f.instruction(&Instruction::I32Eqz);
                }
                self.open_frame(ControlFrameKind::If, f);
                pending.initialize(f);
                self.emit_sync_iterator_close(iterator, &pending, &closed, f)?;
                self.emit_collection_propagate(&closed, f);
                f.instruction(&Instruction::I32Const(0));
                boolean.store(f);
                self.emit_branch_to_target(exit, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        value.set_undefined(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        closed.clear(f);
        pending.clear(f);
        value.clear(f);
        s.release_i32_local(done, f);
        s.release_i32_local(found, f);
        s.release_i64_local(index, f);
        Ok(())
    }
    fn emit_collection_set_operation(
        &mut self,
        operation: SetOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let source = self.emit_collection_set_receiver(f)?;
        let other = self.emit_collection_set_like(f)?;
        let live = s.reserve_i64_local(f);
        let boolean = s.reserve_i32_local(f);
        let header = self.emit_collection_header(NonArrayRealmIntrinsicSlot::SetPrototype, f)?;
        let result = self.emit_collection_alloc_set(&header, f);
        s.struct_type::<SetObject>()
            .field(SetObjectSchema::LIVE_COUNT)
            .read(&source, s, f)
            .store_i64(live, f);
        f.instruction(&Instruction::I32Const(1));
        boolean.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        match operation {
            SetOperation::Subset => {
                live.load(f);
                f.instruction(&Instruction::F64ConvertI64U);
                other.size.scalar().load(f);
                f.instruction(&Instruction::F64ReinterpretI64);
                f.instruction(&Instruction::F64Gt);
                self.open_frame(ControlFrameKind::If, f);
                f.instruction(&Instruction::I32Const(0));
                boolean.store(f);
                self.emit_branch_to_target(exit, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                self.emit_collection_set_receiver_walk(
                    &source,
                    &result,
                    &other,
                    SetReceiverOperation::Subset,
                    boolean,
                    exit,
                    f,
                )?;
            }
            SetOperation::Superset => {
                live.load(f);
                f.instruction(&Instruction::F64ConvertI64U);
                other.size.scalar().load(f);
                f.instruction(&Instruction::F64ReinterpretI64);
                f.instruction(&Instruction::F64Lt);
                self.open_frame(ControlFrameKind::If, f);
                f.instruction(&Instruction::I32Const(0));
                boolean.store(f);
                self.emit_branch_to_target(exit, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                let iterator = self.emit_collection_other_keys(&other, f)?;
                self.emit_collection_set_keys_walk(
                    &source,
                    &result,
                    &iterator,
                    SetKeysOperation::Superset,
                    boolean,
                    exit,
                    f,
                )?;
                iterator.clear(f);
            }
            SetOperation::Union => {
                let iterator = self.emit_collection_other_keys(&other, f)?;
                self.emit_collection_copy_set(&source, &result, f)?;
                self.emit_collection_set_keys_walk(
                    &source,
                    &result,
                    &iterator,
                    SetKeysOperation::Union,
                    boolean,
                    exit,
                    f,
                )?;
                iterator.clear(f);
            }
            SetOperation::SymmetricDifference => {
                let iterator = self.emit_collection_other_keys(&other, f)?;
                self.emit_collection_copy_set(&source, &result, f)?;
                self.emit_collection_set_keys_walk(
                    &source,
                    &result,
                    &iterator,
                    SetKeysOperation::SymmetricDifference,
                    boolean,
                    exit,
                    f,
                )?;
                iterator.clear(f);
            }
            SetOperation::Difference => {
                self.emit_collection_copy_set(&source, &result, f)?;
                live.load(f);
                f.instruction(&Instruction::F64ConvertI64U);
                other.size.scalar().load(f);
                f.instruction(&Instruction::F64ReinterpretI64);
                f.instruction(&Instruction::F64Le);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_collection_set_receiver_walk(
                    &source,
                    &result,
                    &other,
                    SetReceiverOperation::Difference,
                    boolean,
                    exit,
                    f,
                )?;
                f.instruction(&Instruction::Else);
                let iterator = self.emit_collection_other_keys(&other, f)?;
                self.emit_collection_set_keys_walk(
                    &source,
                    &result,
                    &iterator,
                    SetKeysOperation::Difference,
                    boolean,
                    exit,
                    f,
                )?;
                iterator.clear(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
            SetOperation::Intersection => {
                live.load(f);
                f.instruction(&Instruction::F64ConvertI64U);
                other.size.scalar().load(f);
                f.instruction(&Instruction::F64ReinterpretI64);
                f.instruction(&Instruction::F64Le);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_collection_set_receiver_walk(
                    &source,
                    &result,
                    &other,
                    SetReceiverOperation::Intersection,
                    boolean,
                    exit,
                    f,
                )?;
                f.instruction(&Instruction::Else);
                let iterator = self.emit_collection_other_keys(&other, f)?;
                self.emit_collection_set_keys_walk(
                    &source,
                    &result,
                    &iterator,
                    SetKeysOperation::Intersection,
                    boolean,
                    exit,
                    f,
                )?;
                iterator.clear(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
            SetOperation::Disjoint => {
                live.load(f);
                f.instruction(&Instruction::F64ConvertI64U);
                other.size.scalar().load(f);
                f.instruction(&Instruction::F64ReinterpretI64);
                f.instruction(&Instruction::F64Le);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_collection_set_receiver_walk(
                    &source,
                    &result,
                    &other,
                    SetReceiverOperation::Disjoint,
                    boolean,
                    exit,
                    f,
                )?;
                f.instruction(&Instruction::Else);
                let iterator = self.emit_collection_other_keys(&other, f)?;
                self.emit_collection_set_keys_walk(
                    &source,
                    &result,
                    &iterator,
                    SetKeysOperation::Disjoint,
                    boolean,
                    exit,
                    f,
                )?;
                iterator.clear(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        match operation {
            SetOperation::Disjoint | SetOperation::Subset | SetOperation::Superset => {
                self.emit_collection_boolean(boolean, f)
            }
            SetOperation::Difference
            | SetOperation::Intersection
            | SetOperation::SymmetricDifference
            | SetOperation::Union => {
                let value = s.reserve_value_local(f);
                value.set_reference(&result, s, f);
                self.completion().set_normal(&value, f);
                value.clear(f);
            }
        }
        result.clear(f);
        header.clear(f);
        s.release_i32_local(boolean, f);
        s.release_i64_local(live, f);
        other.clear(f);
        source.clear(f);
        Ok(())
    }
    pub(crate) fn emit_set_prototype_difference(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_collection_set_operation(SetOperation::Difference, f)
    }
    pub(crate) fn emit_set_prototype_intersection(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_collection_set_operation(SetOperation::Intersection, f)
    }
    pub(crate) fn emit_set_prototype_symmetric_difference(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_collection_set_operation(SetOperation::SymmetricDifference, f)
    }
    pub(crate) fn emit_set_prototype_union(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_collection_set_operation(SetOperation::Union, f)
    }
    pub(crate) fn emit_set_prototype_is_disjoint_from(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_collection_set_operation(SetOperation::Disjoint, f)
    }
    pub(crate) fn emit_set_prototype_is_subset_of(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_collection_set_operation(SetOperation::Subset, f)
    }
    pub(crate) fn emit_set_prototype_is_superset_of(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_collection_set_operation(SetOperation::Superset, f)
    }
}
