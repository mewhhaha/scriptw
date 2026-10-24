//! Ordered collections retain actual GC entries and stable insertion ordinals.
use super::super::*;
use crate::control_flow::{OwnedSyncIterator, SyncIteratorConsumer};
use crate::functions::{NonArrayRealmIntrinsicSlot, OrdinaryDefaultPrototype};
use crate::gc_types::*;
use crate::objects::PropertyKeyLocals;
mod hashing;
mod iterable_algorithms;
mod iteration;
mod map_get_or_insert;
mod receiver;
mod set_operations;
mod storage;
use receiver::StrongCollectionReceiverKind;

#[derive(Clone, Copy)]
enum GroupByResult {
    Map,
    Object,
}

impl FunctionBuilder<'_> {
    fn emit_collection_key(
        &mut self,
        name: &str,
        f: &mut Function,
    ) -> Result<PropertyKeyLocals, EmitError> {
        let s = self.runtime_schema();
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference(name, f)?, f);
        let key = PropertyKeyLocals::from_string(s, &text, f);
        text.clear(f);
        Ok(key)
    }
    pub(super) fn emit_collection_get(
        &mut self,
        receiver: &ValueLocals,
        name: &str,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let key = self.emit_collection_key(name, f)?;
        self.emit_object_read(receiver, receiver, &key, result, f)?;
        key.clear(f);
        Ok(())
    }
    pub(super) fn emit_collection_propagate(
        &mut self,
        result: &CompletionLocals,
        f: &mut Function,
    ) {
        self.completion().copy_from(result, f);
        self.emit_propagate_current_throw_if_needed(f);
    }
    fn emit_collection_abrupt_exit(
        &mut self,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) {
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        output.copy_from(pending, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }
    pub(super) fn emit_collection_close_abrupt(
        &mut self,
        iterator: &OwnedSyncIterator,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_sync_iterator_close(iterator, pending, output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    pub(super) fn emit_collection_assert_callable(
        &mut self,
        value: &ValueLocals,
        message: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_is_callable_i32(value, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let failed = self.runtime_schema().reserve_completion(f);
        self.emit_throw_current_function_realm_type_error(message, &failed, f)?;
        self.completion().copy_from(&failed, f);
        failed.clear(f);
        self.emit_propagate_current_throw(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    fn emit_collection_normalize_zero(&self, value: &ValueLocals, f: &mut Function) {
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Empty));
        value.scalar().load(f);
        f.instruction(&Instruction::I64Const(i64::MAX));
        f.instruction(&Instruction::I64And);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        value.set_scalar(ScalarValue::NumberBits(0), f);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
    }
    pub(super) fn emit_collection_boolean(&self, boolean: I32Local, f: &mut Function) {
        let s = self.runtime_schema();
        let v = s.reserve_value_local(f);
        v.set_boolean(boolean, f);
        self.completion().set_normal(&v, f);
        v.clear(f);
    }
    fn emit_collection_number(&self, number: I64Local, f: &mut Function) {
        let s = self.runtime_schema();
        let v = s.reserve_value_local(f);
        number.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        v.scalar().store(f);
        v.set_number(v.scalar(), f);
        self.completion().set_normal(&v, f);
        v.clear(f);
    }
    fn emit_collection_header(
        &mut self,
        slot: NonArrayRealmIntrinsicSlot,
        f: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        let s = self.runtime_schema();
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let proto = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(&realm, slot, &proto, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&proto), f)?,
            f,
        );
        proto.clear(f);
        realm.clear(f);
        Ok(header)
    }
    fn emit_collection_map_receiver(
        &mut self,
        f: &mut Function,
    ) -> Result<GcLocal<MapObject>, EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        receiver.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<MapObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let failed = s.reserve_completion(f);
        self.emit_collection_receiver_type_error(
            &receiver,
            StrongCollectionReceiverKind::MapData,
            &failed,
            f,
        )?;
        self.completion().copy_from(&failed, f);
        failed.clear(f);
        self.emit_propagate_current_throw(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let record = s
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<MapObject>(s, f), f);
        receiver.clear(f);
        Ok(record)
    }
    pub(crate) fn emit_map_prototype_size_getter(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let record = self.emit_collection_map_receiver(f)?;
        let n = s.reserve_i64_local(f);
        s.struct_type::<MapObject>()
            .field(MapObjectSchema::LIVE_COUNT)
            .read(&record, s, f)
            .store_i64(n, f);
        self.emit_collection_number(n, f);
        s.release_i64_local(n, f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_map_prototype_has(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let record = self.emit_collection_map_receiver(f)?;
        let key = s.reserve_value_local(f);
        let index = s.reserve_i64_local(f);
        let found = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &key, f);
        self.emit_collection_find_map(&record, &key, index, found, f)?;
        self.emit_collection_boolean(found, f);
        s.release_i32_local(found, f);
        s.release_i64_local(index, f);
        key.clear(f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_map_prototype_delete(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let record = self.emit_collection_map_receiver(f)?;
        let key = s.reserve_value_local(f);
        let removed = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &key, f);
        self.emit_collection_delete_map(&record, &key, removed, f)?;
        self.emit_collection_boolean(removed, f);
        s.release_i32_local(removed, f);
        key.clear(f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_map_prototype_clear(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let record = self.emit_collection_map_receiver(f)?;
        self.emit_collection_clear_map(&record, f);
        let v = self.runtime_schema().reserve_value_local(f);
        v.set_undefined(f);
        self.completion().set_normal(&v, f);
        v.clear(f);
        record.clear(f);
        Ok(())
    }
    fn emit_collection_set_receiver(
        &mut self,
        f: &mut Function,
    ) -> Result<GcLocal<SetObject>, EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        receiver.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<SetObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let failed = s.reserve_completion(f);
        self.emit_collection_receiver_type_error(
            &receiver,
            StrongCollectionReceiverKind::SetData,
            &failed,
            f,
        )?;
        self.completion().copy_from(&failed, f);
        failed.clear(f);
        self.emit_propagate_current_throw(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let record = s
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<SetObject>(s, f), f);
        receiver.clear(f);
        Ok(record)
    }
    pub(crate) fn emit_set_prototype_size_getter(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let record = self.emit_collection_set_receiver(f)?;
        let n = s.reserve_i64_local(f);
        s.struct_type::<SetObject>()
            .field(SetObjectSchema::LIVE_COUNT)
            .read(&record, s, f)
            .store_i64(n, f);
        self.emit_collection_number(n, f);
        s.release_i64_local(n, f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_set_prototype_has(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let record = self.emit_collection_set_receiver(f)?;
        let key = s.reserve_value_local(f);
        let index = s.reserve_i64_local(f);
        let found = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &key, f);
        self.emit_collection_find_set(&record, &key, index, found, f)?;
        self.emit_collection_boolean(found, f);
        s.release_i32_local(found, f);
        s.release_i64_local(index, f);
        key.clear(f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_set_prototype_delete(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let record = self.emit_collection_set_receiver(f)?;
        let key = s.reserve_value_local(f);
        let removed = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &key, f);
        self.emit_collection_delete_set(&record, &key, removed, f)?;
        self.emit_collection_boolean(removed, f);
        s.release_i32_local(removed, f);
        key.clear(f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_set_prototype_clear(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let record = self.emit_collection_set_receiver(f)?;
        self.emit_collection_clear_set(&record, f);
        let v = self.runtime_schema().reserve_value_local(f);
        v.set_undefined(f);
        self.completion().set_normal(&v, f);
        v.clear(f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_map_prototype_get(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let map = self.emit_collection_map_receiver(f)?;
        let key = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let index = s.reserve_i64_local(f);
        let found = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &key, f);
        value.set_undefined(f);
        self.emit_collection_find_map(&map, &key, index, found, f)?;
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_collection_read_map(&map, index, &key, &value, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&value, f);
        s.release_i32_local(found, f);
        s.release_i64_local(index, f);
        value.clear(f);
        key.clear(f);
        map.clear(f);
        Ok(())
    }
    pub(crate) fn emit_map_prototype_set(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let map = self.emit_collection_map_receiver(f)?;
        let key = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let receiver = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &key, f);
        self.emit_builtin_arg_to_value(1, &value, f);
        self.emit_collection_normalize_zero(&key, f);
        self.emit_collection_put_map(&map, &key, &value, f)?;
        receiver.set_reference(&map, s, f);
        self.completion().set_normal(&receiver, f);
        receiver.clear(f);
        value.clear(f);
        key.clear(f);
        map.clear(f);
        Ok(())
    }
    pub(crate) fn emit_set_prototype_add(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let set = self.emit_collection_set_receiver(f)?;
        let value = s.reserve_value_local(f);
        let receiver = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &value, f);
        self.emit_collection_normalize_zero(&value, f);
        self.emit_collection_put_set(&set, &value, f)?;
        receiver.set_reference(&set, s, f);
        self.completion().set_normal(&receiver, f);
        receiver.clear(f);
        value.clear(f);
        set.clear(f);
        Ok(())
    }
}
