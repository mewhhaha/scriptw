//! A plain assignment to a computed key, `o[k] = v`, calls one R kernel with
//! the raw base, the raw key and the value. The kernel is PutValue's [[Set]]:
//! it first answers the cases whose outcome the live storage proves, before
//! any key conversion, and otherwise runs the general steps in their order.
//!
//! - A Number key that is an array index on a concrete packed Array stores
//!   the element without a decimal key; an absent element is created only
//!   when no prototype has the index.
//! - A String key on an ordinary object replaces an existing writable own
//!   data property in place. An absent key is added as OrdinarySet's
//!   CreateDataProperty when the call-free walk proves that no object on the
//!   receiver's chain has it, so no setter, read-only property or Proxy can
//!   intervene; the compact define kernel adds it by shape transition or
//!   declines without effect.
//!
//! Everything else takes the general route: a nullish base throws, ToObject
//! boxes a primitive, ToPropertyKey converts the key (which may call user
//! code), and OrdinarySet runs with the original base as receiver. ToObject
//! of an object and ToPropertyKey of a String or Number observe nothing, so
//! answering those cases first changes no order.
use super::*;
use crate::gc_types::{ArrayObject, JavaScriptObject};

impl FunctionBuilder<'_> {
    pub(crate) fn compile_computed_property_write_helper(&mut self) -> Result<Function, EmitError> {
        let mut body = self.begin_helper_body(RuntimeHelperId::ComputedPropertyWrite);
        let p = self.helper_parameters::<crate::runtime_helpers::ComputedPropertyWriteParameters>(
            &mut body,
        );
        let f = &mut body;
        let s = self.runtime_schema();
        let result = s.reserve_completion(f);
        let succeeded = s.reserve_i32_local(f);
        succeeded.set_constant(1, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let general = self.open_frame(ControlFrameKind::Block, f);
        self.emit_computed_index_set_or_branch(&p.target, &p.key, &p.value, general, f)?;
        self.emit_object_boolean_result(succeeded, &result, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let general_set = self.open_frame(ControlFrameKind::Block, f);
        self.emit_computed_named_set_or_branch(&p.target, &p.key, &p.value, general_set, f)?;
        self.emit_object_boolean_result(succeeded, &result, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // PutValue: the base must be coercible, ToObject precedes the key's
        // conversion, and the Set keeps the original base as receiver.
        self.compile_nullish_tagged_i32(p.target.tag(), f)?;
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_CONVERT_UNDEFINED_OR_NULL_TO_OBJECT,
            &result,
            f,
        )?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let pending = s.reserve_completion(f);
        let lookup = s.reserve_value_local(f);
        self.emit_value_to_object_locals(&p.target, &pending, f)?;
        self.emit_object_operation_abrupt_exit(&pending, &result, exit, f);
        lookup.copy_from(pending.value(), f);
        pending.clear(f);
        let key = self.emit_value_to_property_key_locals_or_branch(&p.key, &result, exit, f)?;
        s.call_helper(
            crate::runtime_helpers::OrdinarySetArguments::new(
                &lookup,
                &p.target,
                &key,
                &p.value,
                self.current_environment(),
                self.current_program(),
            ),
            self.runtime_helper_base()?,
            f,
        )
        .store(&result, f);
        key.clear(f);
        lookup.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        result.emit(f);
        result.clear(f);
        s.release_i32_local(succeeded, f);
        p.release(f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(body))
    }

    /// A Number key that is an array index, on a concrete Array: the packed
    /// element Set. Falls through when the Set succeeded.
    fn emit_computed_index_set_or_branch(
        &mut self,
        target: &ValueLocals,
        key: &ValueLocals,
        value: &ValueLocals,
        general: crate::emit::ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(general, f);
        let array = s.reserve_gc_local(f).initialize(
            target.sealed_reference_or_branch::<ArrayObject>(
                s,
                self.local_branch_label(general),
                f,
            ),
            f,
        );
        // Ordered bounds reject NaN and infinities before the integer cast;
        // -0 names index 0. 2^32 - 1 is not an array index.
        key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Ge);
        key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(u32::MAX as f64)));
        f.instruction(&Instruction::F64Lt);
        f.instruction(&Instruction::I32And);
        key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        // An integral value equals its saturating i64 truncation converted
        // back; the bounds above make that exact, and no rounding operator
        // (a host call on the baseline x86-64 target) is needed.
        key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncSatF64S);
        f.instruction(&Instruction::F64ConvertI64S);
        f.instruction(&Instruction::F64Eq);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(general, f);
        let index = s.reserve_i64_local(f);
        key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncF64U);
        index.store(f);
        self.emit_packed_array_index_set_or_branch(&array, index, value, general, f)?;
        s.release_i64_local(index, f);
        array.clear(f);
        Ok(())
    }

    /// A String key on an actual OrdinaryObject. On compact storage one probe
    /// of the live shape decides: a present key takes the value when the
    /// shape's one attribute word is writable data; an absent key is added
    /// by the canonical child shape when the object is extensible, its shape
    /// holds default data attributes and the walk proves that no object on
    /// its chain has the key: CompactDataDefine's steps, which complete
    /// OrdinarySet's CreateDataProperty. A dictionary receiver takes the
    /// value into an existing writable data property. Falls through when the
    /// Set succeeded.
    fn emit_computed_named_set_or_branch(
        &mut self,
        target: &ValueLocals,
        key: &ValueLocals,
        value: &ValueLocals,
        general: crate::emit::ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(general, f);
        let object = s.reserve_gc_local(f).initialize(
            target.sealed_reference_or_branch::<OrdinaryObject>(
                s,
                self.local_branch_label(general),
                f,
            ),
            f,
        );
        let name = s
            .reserve_gc_local(f)
            .initialize(key.cast_reference::<StringValue>(s, f), f);
        let property = PropertyKeyLocals::from_string(s, &name, f);
        let set = self.open_frame(ControlFrameKind::Block, f);
        let base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, s, f)
                .reference(),
            f,
        );
        base.is_subtype::<CompactDataStorage>(s, f);
        self.open_frame(ControlFrameKind::If, f);
        let compact = s
            .reserve_gc_local(f)
            .initialize(base.load(s, f).cast_subtype::<CompactDataStorage>(s, f), f);
        let shape = s.reserve_gc_local(f).initialize(
            s.field(CompactDataStorageSchema::SHAPE)
                .read(&compact, s, f)
                .reference(),
            f,
        );
        let hash = s.reserve_i64_local(f);
        self.emit_string_key_hash(&name, hash, f);
        let slot = s.reserve_i32_local(f);
        let found = s.reserve_i32_local(f);
        self.emit_compact_shape_probe_prepared(&shape, &name, hash, slot, found, f);
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        // Existing own data: OrdinarySet's [[DefineOwnProperty]] with only
        // [[Value]] on a writable property replaces the value.
        s.field(CompactDataShapeSchema::ATTRIBUTES)
            .read(&shape, s, f);
        f.instruction(&Instruction::I32Const(DescriptorMask::WRITABLE.as_i32()));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(general, f);
        self.emit_compact_store_at(&compact, slot, value, f);
        self.emit_branch_to_target(set, f);
        f.instruction(&Instruction::Else);
        // An add: only an all-default shape takes a default data key, and
        // only an extensible object takes a new property.
        s.field(CompactDataShapeSchema::ATTRIBUTES)
            .read(&shape, s, f);
        f.instruction(&Instruction::I32Const(
            super::transition::default_data_attributes().as_i32(),
        ));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(general, f);
        s.field(OrdinaryObjectSchema::EXTENSIBLE)
            .read(&object, s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(general, f);
        self.emit_computed_chain_absence_or_branch(&object, &name, hash, general, f);
        let keys = s.reserve_gc_local(f).initialize(
            s.field(CompactDataShapeSchema::KEYS)
                .read(&shape, s, f)
                .reference(),
            f,
        );
        let count = s.reserve_i32_local(f);
        s.array_type::<CompactShapeKeys>().length(&keys, s, f);
        count.store(f);
        // The child refuses array-index keys and the transition bounds.
        let child =
            self.emit_compact_shape_child(&shape, &keys, count, &name, hash, &property, f)?;
        keys.clear(f);
        let next = s.reserve_gc_local(f).initialize(
            child.non_null_or_branch(self.local_branch_label(general), f),
            f,
        );
        child.clear(f);
        self.emit_compact_storage_extend(&object, &compact, &next, count, value, f);
        next.clear(f);
        s.release_i32_local(count, f);
        self.emit_branch_to_target(set, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(found, f);
        s.release_i32_local(slot, f);
        s.release_i64_local(hash, f);
        shape.clear(f);
        compact.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        base.clear(f);
        let dictionary =
            self.emit_dictionary_existing_data_write(target, target, &property, value, f)?;
        dictionary.load(f);
        f.instruction(&Instruction::I32Eqz);
        s.release_i32_local(dictionary, f);
        self.emit_branch_if_to_target(general, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        property.clear(f);
        name.clear(f);
        object.clear(f);
        Ok(())
    }

    /// Branches to `general` unless the walk proves that no object on the
    /// receiver's prototype chain has the key.
    fn emit_computed_chain_absence_or_branch(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        name: &GcLocal<StringValue>,
        hash: I64Local,
        general: crate::emit::ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let named = s.reserve_i32_local(f);
        self.emit_header_named_key_i32(name, hash, f);
        named.store(f);
        let prototype = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize(
                s.field(OrdinaryObjectSchema::PROTOTYPE)
                    .read(object, s, f)
                    .reference(),
                f,
            );
        prototype.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let first = self.emit_walk_prototype_header_or_branch(&prototype, named, general, f);
        let scratch = s.reserve_value_local(f);
        self.emit_chain_walk(
            &first,
            ChainKey::String { name, hash },
            named,
            ChainAnswer::Absence,
            &scratch,
            general,
            f,
        );
        scratch.clear(f);
        first.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        prototype.clear(f);
        s.release_i32_local(named, f);
    }
}
