//! A plain assignment to a computed key, `o[k] = v`, calls one R kernel with
//! the raw base, the raw key and the value. The kernel is PutValue's [[Set]]:
//! it answers packed numeric indices without converting them and prepares
//! other keys once in PutValue order before consulting live storage.
//!
//! - A Number key that is an array index on a concrete packed Array stores
//!   the element without a decimal key; an absent element is created only
//!   when no prototype has the index.
//! - An already-converted String key on an ordinary object replaces an
//!   existing writable own data property in place. An absent key is added as OrdinarySet's
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
use crate::intrinsics::{IntrinsicKey, IntrinsicPropertyLayoutId};

impl RuntimeSchema {
    /// Dynamic source operands are already evaluated. A concrete ordinary
    /// object and String need no coercion or observation before own lookup.
    pub(crate) fn emit_hinted_compact_own_key_from_values_or_branch(
        &self,
        target: &ValueLocals,
        key: &ValueLocals,
        miss: crate::LabelDepth,
        f: &mut Function,
    ) {
        key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        f.instruction(&Instruction::I32Ne);
        f.branch_if_to_label(miss);
        let key = self.reserve_non_null_gc_local(f).initialize(
            key.sealed_reference_or_branch::<StringValue>(self, miss, f),
            f,
        );
        let object = self.reserve_non_null_gc_local(f).initialize(
            target.sealed_reference_or_branch::<OrdinaryObject>(self, miss, f),
            f,
        );
        self.emit_hinted_compact_own_key_or_branch(&object, &key, miss, f);
        object.clear(f);
        key.clear(f);
    }

    pub(crate) fn emit_hinted_compact_own_key_or_branch(
        &self,
        object: &GcLocal<OrdinaryObject>,
        key: &GcLocal<StringValue>,
        miss: crate::LabelDepth,
        f: &mut Function,
    ) {
        let base = self.reserve_non_null_gc_local(f).initialize(
            self.field(OrdinaryObjectSchema::PROPERTIES)
                .read(object, self, f)
                .reference(),
            f,
        );
        let storage = self
            .reserve_non_null_gc_local(f)
            .initialize(self.emit_compact_storage_or_branch(&base, miss, f), f);
        base.clear(f);
        let index = self.reserve_i32_local(f);
        self.emit_hinted_compact_index_or_branch(&storage, key, index, miss, f);
        self.release_i32_local(index, f);
        storage.clear(f);
    }

    /// The String's mutable ordinal is a hint. Bounds and exact reference
    /// equality prove which immutable key occupies that slot in live storage.
    /// A stale hint or an equal String with another identity is a miss.
    pub(super) fn emit_hinted_compact_index_or_branch(
        &self,
        storage: &GcLocal<CompactDataStorage>,
        key: &GcLocal<StringValue>,
        index: I32Local,
        miss: crate::LabelDepth,
        f: &mut Function,
    ) {
        self.field(StringValueSchema::ENTRY_HINT)
            .read(key, self, f)
            .store(index, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32LtS);
        f.branch_if_to_label(miss);
        let shape = self
            .field(CompactDataStorageSchema::SHAPE)
            .read(storage, self, f)
            .reference();
        let keys = self.reserve_non_null_gc_local(f).initialize(
            self.field(CompactDataShapeSchema::KEYS)
                .read_top(shape, f)
                .reference(),
            f,
        );
        index.load(f);
        self.array_type::<CompactShapeKeys>().length(&keys, self, f);
        f.instruction(&Instruction::I32GeU);
        f.branch_if_to_label(miss);
        self.array_type::<CompactShapeKeys>()
            .read(&keys, index, self, f)
            .reference();
        key.load(self, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        f.branch_if_to_label(miss);
        keys.clear(f);
    }
}

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
        self.emit_is_heap_object_like_tag_i32(p.target.tag(), f);
        self.open_frame(ControlFrameKind::If, f);
        lookup.copy_from(&p.target, f);
        f.instruction(&Instruction::Else);
        self.emit_value_to_object_locals(&p.target, &pending, f)?;
        self.emit_object_operation_abrupt_exit(&pending, &result, exit, f);
        lookup.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        pending.clear(f);
        let key = self.emit_value_to_property_key_locals_or_branch(&p.key, &result, exit, f)?;
        let general_set = self.open_frame(ControlFrameKind::Block, f);
        self.emit_computed_named_set_or_branch(&p.target, &key, &p.value, general_set, f)?;
        self.emit_object_boolean_result(succeeded, &result, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
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
        // The unsigned range and round trip admit exactly array indices.
        // NaN, infinities and fractions fail; -0 names index 0.
        let index = s.reserve_i64_local(f);
        key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncSatF64S);
        index.store(f);
        index.load(f);
        f.instruction(&Instruction::I64Const(i64::from(u32::MAX)));
        f.instruction(&Instruction::I64LtU);
        key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        index.load(f);
        f.instruction(&Instruction::F64ConvertI64S);
        f.instruction(&Instruction::F64Eq);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(general, f);
        self.emit_packed_array_index_set_or_branch(&array, index, value, general, f)?;
        s.release_i64_local(index, f);
        array.clear(f);
        Ok(())
    }

    /// A String key on an actual OrdinaryObject. A matching leading shape
    /// transition proves the key is absent from the live parent shape. Other
    /// keys probe the shape or dictionary for an existing writable own data
    /// property. Compact adds require extensibility and a live absence proof
    /// along the entire prototype chain.
    fn emit_computed_named_set_or_branch(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        general: crate::emit::ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        key.value().tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(general, f);
        let object = s.reserve_non_null_gc_local(f).initialize(
            target.sealed_reference_or_branch::<OrdinaryObject>(
                s,
                self.local_branch_label(general),
                f,
            ),
            f,
        );
        let name = s
            .reserve_non_null_gc_local(f)
            .initialize(key.value().cast_reference::<StringValue>(s, f), f);
        let set = self.open_frame(ControlFrameKind::Block, f);
        let base = s.reserve_non_null_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, s, f)
                .reference(),
            f,
        );
        let not_compact = self.open_frame(ControlFrameKind::Block, f);
        let compact = s.reserve_non_null_gc_local(f).initialize(
            s.emit_compact_storage_or_branch(&base, self.local_branch_label(not_compact), f),
            f,
        );
        let shape = s.reserve_non_null_gc_local(f).initialize(
            s.field(CompactDataStorageSchema::SHAPE)
                .read(&compact, s, f)
                .reference(),
            f,
        );
        // Check `StringValueSchema::ENTRY_HINT` before hashing and shape or
        // transition probing. Bounds and exact String identity prove the slot.
        let hint_miss = self.open_frame(ControlFrameKind::Block, f);
        let hinted_slot = s.reserve_i32_local(f);
        s.emit_hinted_compact_index_or_branch(
            &compact,
            &name,
            hinted_slot,
            self.local_branch_label(hint_miss),
            f,
        );
        s.field(CompactDataShapeSchema::ATTRIBUTES)
            .read(&shape, s, f);
        f.instruction(&Instruction::I32Const(DescriptorMask::WRITABLE.as_i32()));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(general, f);
        self.emit_direct_compact_store_at(&compact, hinted_slot, value, f);
        s.release_i32_local(hinted_slot, f);
        self.emit_branch_to_target(set, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);

        let hash = s.reserve_i64_local(f);
        self.emit_string_key_hash(&name, hash, f);
        let count = s.reserve_i32_local(f);
        count.set_constant(0, f);
        let child = self.emit_computed_leading_transition(&shape, &name, hash, count, f);

        child.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let slot = s.reserve_i32_local(f);
        let found = s.reserve_i32_local(f);
        self.emit_compact_shape_probe_prepared(&shape, &name, hash, slot, found, f);
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(CompactDataShapeSchema::ATTRIBUTES)
            .read(&shape, s, f);
        f.instruction(&Instruction::I32Const(DescriptorMask::WRITABLE.as_i32()));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(general, f);
        s.field(StringValueSchema::ENTRY_HINT)
            .write(&name, GcOperand::i32_local(slot), s, f);
        self.emit_direct_compact_store_at(&compact, slot, value, f);
        self.emit_branch_to_target(set, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(found, f);
        s.release_i32_local(slot, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

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
        self.emit_computed_chain_absence_or_branch(&object, &name, hash, general, f)?;
        let define = self.open_frame(ControlFrameKind::Block, f);
        let next = s.reserve_gc_local(f).initialize(
            child.non_null_or_branch(self.local_branch_label(define), f),
            f,
        );
        s.return_call_helper(
            crate::runtime_helpers::CachedPropertyAddArguments::new(
                &object, &compact, &next, count, value,
            ),
            self.runtime_helper_base()?,
            f,
        );
        next.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        child.clear(f);

        // The live shape probe already established own absence. Look up or
        // create its canonical child with the same converted key and hash.
        let keys = s.reserve_non_null_gc_local(f).initialize(
            s.field(CompactDataShapeSchema::KEYS)
                .read(&shape, s, f)
                .reference(),
            f,
        );
        s.array_type::<CompactShapeKeys>().length(&keys, s, f);
        count.store(f);
        let child = self.emit_compact_shape_child(&shape, &keys, count, &name, hash, key, f)?;
        keys.clear(f);
        let next = s.reserve_non_null_gc_local(f).initialize(
            child.non_null_or_branch(self.local_branch_label(general), f),
            f,
        );
        child.clear(f);
        s.return_call_helper(
            crate::runtime_helpers::CachedPropertyAddArguments::new(
                &object, &compact, &next, count, value,
            ),
            self.runtime_helper_base()?,
            f,
        );
        next.clear(f);
        s.release_i32_local(count, f);
        s.release_i64_local(hash, f);
        shape.clear(f);
        compact.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        base.clear(f);
        let dictionary = self.emit_dictionary_existing_data_write(target, target, key, value, f)?;
        dictionary.load(f);
        f.instruction(&Instruction::I32Eqz);
        s.release_i32_local(dictionary, f);
        self.emit_branch_if_to_target(general, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        name.clear(f);
        object.clear(f);
        Ok(())
    }

    /// Writes `value` into a proved slot of `compact` directly through its
    /// concrete sealed layout, avoiding a `helper::compact_data_store` call.
    fn emit_direct_compact_store_at(
        &mut self,
        compact: &GcLocal<CompactDataStorage>,
        slot: I32Local,
        value: &ValueLocals,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let done = self.open_frame(ControlFrameKind::Block, f);
        let array_path = self.open_frame(ControlFrameKind::Block, f);
        let inline = s.reserve_non_null_gc_local(f).initialize(
            compact.sealed_subtype_or_branch::<InlineCompactDataStorage>(
                s,
                self.local_branch_label(array_path),
                f,
            ),
            f,
        );
        self.emit_inline_compact_recorded_store_at(&inline, slot, value, f);
        inline.clear(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let array = s.reserve_non_null_gc_local(f).initialize(
            compact
                .load(s, f)
                .cast_subtype::<ArrayCompactDataStorage>(s, f),
            f,
        );
        self.emit_array_compact_store_at(&array, slot, value, f);
        array.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
    }

    /// Transition tables put frequently used children first. The child's
    /// first added key is at the live parent's key count, and its presence
    /// proves that the parent lacks this exact String key.
    fn emit_computed_leading_transition(
        &mut self,
        shape: &GcLocal<CompactDataShape>,
        name: &GcLocal<StringValue>,
        hash: I64Local,
        count: I32Local,
        f: &mut Function,
    ) -> GcLocal<CompactDataShape, Nullable> {
        let s = self.runtime_schema();
        let child = s
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize_null(s, f);
        let miss = self.open_frame(ControlFrameKind::Block, f);
        let table = s.reserve_gc_local(f).initialize(
            s.field(CompactDataShapeSchema::TRANSITIONS)
                .read(shape, s, f)
                .reference()
                .non_null_or_branch(self.local_branch_label(miss), f),
            f,
        );
        let first = s.reserve_i32_local(f);
        first.set_constant(0, f);
        // Published transition tables have at least their initial capacity.
        let candidate = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactShapeTable>()
                .read(&table, first, s, f)
                .reference()
                .non_null_or_branch(self.local_branch_label(miss), f),
            f,
        );
        s.release_i32_local(first, f);
        table.clear(f);
        let parent_keys = s.reserve_gc_local(f).initialize(
            s.field(CompactDataShapeSchema::KEYS)
                .read(shape, s, f)
                .reference(),
            f,
        );
        s.array_type::<CompactShapeKeys>()
            .length(&parent_keys, s, f);
        count.store(f);
        parent_keys.clear(f);
        let keys = s.reserve_gc_local(f).initialize(
            s.field(CompactDataShapeSchema::KEYS)
                .read(&candidate, s, f)
                .reference(),
            f,
        );
        let candidate_key = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactShapeKeys>()
                .read(&keys, count, s, f)
                .reference(),
            f,
        );
        self.emit_compact_key_matches_i32(&candidate_key, name, hash, f);
        self.open_frame(ControlFrameKind::If, f);
        child.replace(candidate.load(s, f).nullable(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        candidate_key.clear(f);
        keys.clear(f);
        candidate.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        child
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
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        // Derive the filter from the same validated rows used to publish
        // %Object.prototype%. A hash bucket with no row proves absence,
        // while collisions still go through the complete chain check.
        let layout = IntrinsicPropertyLayoutId::ObjectPrototype.layout()?;
        let mut name_buckets = 0u64;
        for property in layout.properties() {
            if let IntrinsicKey::Name(name) = property.key {
                if !name.is_ascii() {
                    name_buckets = u64::MAX;
                    break;
                }
                name_buckets |= 1 << ((super::chain::raw_ascii_hash(name) as u64) >> 58);
            }
        }
        let prototype = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize(
                s.field(OrdinaryObjectSchema::PROTOTYPE)
                    .read(object, s, f)
                    .reference(),
                f,
            );
        let absent = self.open_frame(ControlFrameKind::Block, f);
        prototype.load(s, f).is_null(f);
        self.emit_branch_if_to_target(absent, f);
        let walk = self.open_frame(ControlFrameKind::Block, f);
        f.instruction(&Instruction::I64Const(1));
        hash.load(f);
        f.instruction(&Instruction::I64Const(58));
        f.instruction(&Instruction::I64ShrU);
        f.instruction(&Instruction::I64Rotl);
        f.instruction(&Instruction::I64Const(name_buckets as i64));
        f.instruction(&Instruction::I64And);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(walk, f);
        self.emit_unchanged_object_prototype_or_branch(&prototype, walk, f);
        self.emit_branch_to_target(absent, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let named = s.reserve_i32_local(f);
        let first = self.open_reference_block(s, f);
        let classify = self.open_frame(ControlFrameKind::Block, f);
        let terminal = s.reserve_non_null_gc_local(f).initialize(
            prototype.sealed_subtype_or_branch::<OrdinaryObject>(
                s,
                self.local_branch_label(classify),
                f,
            ),
            f,
        );
        s.field(OrdinaryObjectSchema::PROTOTYPE)
            .read(&terminal, s, f)
            .reference()
            .is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(classify, f);
        // A single ordinary prototype stores every key in its header.
        // Its lookup needs no admission test for Array or String indices.
        named.set_constant(0, f);
        first.branch_with(terminal.load(s, f), f);
        terminal.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_header_named_key_i32(name, hash, f);
        named.store(f);
        let header = self.emit_walk_prototype_header_or_branch(&prototype, named, general, f);
        first.branch_with(header.load(s, f), f);
        header.clear(f);
        let first = s.reserve_non_null_gc_local(f).initialize(first.close(f), f);
        self.pop_control(ControlFrameKind::Block);
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
        s.release_i32_local(named, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        prototype.clear(f);
        Ok(())
    }
}
