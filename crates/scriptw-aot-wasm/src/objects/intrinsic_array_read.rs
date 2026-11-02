//! Static Array prototype DATA reads use the live, already-resolved group.
//! Every guard is reloaded before use; a miss retains the ordinary Get path.
use super::*;
use crate::data::RNamedDataProperty;
use crate::gc_types::{
    ArrayObject, ArrayObjectSchema, GcNullability, IntrinsicPropertyStorage,
    IntrinsicPropertyStorageSchema, IntrinsicPropertyValueCache,
};
use crate::intrinsics::IntrinsicPropertyLayoutId;

impl FunctionBuilder<'_> {
    fn intrinsic_array_read_plan(
        &self,
        target: &TypedExpr,
        key: &PropertyKeyIr,
    ) -> Option<RNamedDataProperty> {
        if !self.has_source_execution_environment()
            || !target.possible_kinds.contains(ValueKind::Array)
        {
            return None;
        }
        let PropertyKeyIr::StaticString(name) = key else {
            return None;
        };
        self.strings
            .intrinsic_named_data_property(IntrinsicPropertyLayoutId::ArrayPrototype, name)
    }

    pub(super) fn try_compile_intrinsic_array_property_read(
        &mut self,
        target: &TypedExpr,
        key: &PropertyKeyIr,
        receiver: &ValueLocals,
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(plan) = self.intrinsic_array_read_plan(target, key) else {
            return Ok(false);
        };
        self.compile_intrinsic_array_property_read(plan, target, key, receiver, output, f)?;
        Ok(true)
    }

    fn compile_intrinsic_array_property_read(
        &mut self,
        plan: RNamedDataProperty,
        target: &TypedExpr,
        key: &PropertyKeyIr,
        receiver: &ValueLocals,
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let handled = self.emit_intrinsic_array_data_read(plan, receiver, output, f);
        handled.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        // Only a literal String is deferred. Receiver effects have completed,
        // and callee lookup still finishes before any call arguments execute.
        self.compile_ordinary_property_read_from_locals(target, key, receiver, output, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.runtime_schema().release_i32_local(handled, f);
        Ok(())
    }

    pub(crate) fn try_compile_intrinsic_array_get_v(
        &mut self,
        operation: SpecOperationIr,
        operands: &[TypedExpr],
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<bool, EmitError> {
        if operation != SpecOperationIr::GetV {
            return Ok(false);
        }
        let [target, key] = operands else {
            return Ok(false);
        };
        let ExprIr::String(name) = &key.expr else {
            return Ok(false);
        };
        let key = PropertyKeyIr::StaticString(name.clone());
        let Some(plan) = self.intrinsic_array_read_plan(target, &key) else {
            return Ok(false);
        };
        let receiver = self.runtime_schema().reserve_value_local(f);
        self.compile_expr_to_value(target, &receiver, f)?;
        self.compile_intrinsic_array_property_read(plan, target, &key, &receiver, output, f)?;
        receiver.clear(f);
        Ok(true)
    }

    fn emit_intrinsic_array_data_read(
        &mut self,
        plan: RNamedDataProperty,
        receiver: &ValueLocals,
        output: &ValueLocals,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        handled.set_constant(0, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        receiver.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<ArrayObject>(s, f), f);
        let header = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&array, s, f)
                .reference(),
            f,
        );
        let own = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&header, s, f)
                .reference(),
            f,
        );
        own.is_subtype::<OrdinaryPropertyStorage>(s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        let ordinary = s.reserve_gc_local(f).initialize(
            own.load(s, f).cast_subtype::<OrdinaryPropertyStorage>(s, f),
            f,
        );
        let length = s.reserve_i32_local(f);
        s.field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&ordinary, s, f)
            .store(length, f);
        length.load(f);
        self.emit_branch_if_to_target(exit, f);
        let prototype = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, s, f)
                .reference(),
            f,
        );
        prototype.is_subtype::<ArrayObject>(s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        let prototype_array = s
            .reserve_gc_local(f)
            .initialize(prototype.load(s, f).cast_subtype::<ArrayObject>(s, f), f);
        let prototype_header = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&prototype_array, s, f)
                .reference(),
            f,
        );
        let base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&prototype_header, s, f)
                .reference(),
            f,
        );
        base.is_subtype::<IntrinsicPropertyStorage>(s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        let storage = s.reserve_gc_local(f).initialize(
            base.load(s, f)
                .cast_subtype::<IntrinsicPropertyStorage>(s, f),
            f,
        );
        let word = s.reserve_i32_local(f);
        s.field(IntrinsicPropertyStorageSchema::ADDRESS)
            .read(&storage, s, f)
            .store(word, f);
        word.load(f);
        f.instruction(&Instruction::I32Const(plan.span().address() as i32));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(exit, f);
        s.field(IntrinsicPropertyStorageSchema::COUNT)
            .read(&storage, s, f)
            .store(word, f);
        word.load(f);
        f.instruction(&Instruction::I32Const(plan.span().count() as i32));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(exit, f);
        let cache = s.reserve_gc_local(f).initialize(
            s.field(IntrinsicPropertyStorageSchema::CACHE)
                .read(&storage, s, f)
                .reference(),
            f,
        );
        cache.load(s, f).is_null(f);
        self.emit_branch_if_to_target(exit, f);
        let cache_ref = s
            .reserve_gc_local(f)
            .initialize(cache.load(s, f).require_non_null(f), f);
        word.set_constant(plan.group().index() as i32, f);
        let value = s.reserve_gc_local(f).initialize(
            s.array_type::<IntrinsicPropertyValueCache>()
                .read(&cache_ref, word, s, f)
                .reference(),
            f,
        );
        // A nonnull cache entry may still be a descriptor-observed recipe.
        value.is_subtype::<StoredValue>(s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        let stored = s
            .reserve_gc_local(f)
            .initialize(value.load(s, f).cast_subtype::<StoredValue>(s, f), f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, output, s, f);
        handled.set_constant(1, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // Clear even on a guard miss. No calls or allocations can observe a
        // partially initialized arm, and every reserved root is nullable.
        stored.clear(f);
        value.clear(f);
        cache_ref.clear(f);
        cache.clear(f);
        storage.clear(f);
        base.clear(f);
        prototype_header.clear(f);
        prototype_array.clear(f);
        prototype.clear(f);
        ordinary.clear(f);
        own.clear(f);
        header.clear(f);
        array.clear(f);
        s.release_i32_local(word, f);
        s.release_i32_local(length, f);
        handled
    }
}
