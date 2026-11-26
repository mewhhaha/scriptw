//! CreateIteratorResultObject's `{ value, done }`: OrdinaryObjectCreate
//! followed by CreateDataPropertyOrThrow of "value" and then "done", both
//! default W/E/C data properties on a fresh extensible object. That is
//! exactly an inline compact storage over one shape with those two keys in
//! that order, which every result of this runtime shares. The shape retains
//! only its two primitive keys; each result owns its values.
use super::*;

impl FunctionBuilder<'_> {
    fn emit_iterator_result_shape(
        &mut self,
        f: &mut Function,
    ) -> Result<GcLocal<CompactDataShape>, EmitError> {
        let schema = self.runtime_schema();
        let result = schema
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize_null(schema, f);
        schema.load_iterator_result_shape(&result, f);
        result.load(schema, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let mut keys = Vec::with_capacity(2);
        for name in ["value", "done"] {
            let key = schema
                .reserve_gc_local(f)
                .initialize(self.emit_interned_string_reference(name, f)?, f);
            self.emit_prepare_compact_key_hash(&key, f);
            keys.push(key);
        }
        let key_table = schema.reserve_gc_local(f).initialize(
            schema
                .array_type::<CompactShapeKeys>()
                .fixed(keys.iter().map(|key| GcOperand::reference(key, schema)), f),
            f,
        );
        let shape = schema.reserve_gc_local(f).initialize(
            schema.struct_type::<CompactDataShape>().construct(
                (
                    GcOperand::reference(&key_table, schema),
                    GcOperand::descriptor_word(
                        crate::heap::StoredPropertyAttributes::Data {
                            writable: true,
                            enumerable: true,
                            configurable: true,
                        }
                        .descriptor_word(),
                    ),
                    // Neither key is an array index or "toJSON".
                    GcOperand::boolean(true),
                    GcOperand::boolean(true),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                f,
            ),
            f,
        );
        schema.publish_iterator_result_shape(&shape, f);
        result.replace(shape.load(schema, f).nullable(), f);
        shape.clear(f);
        key_table.clear(f);
        for key in keys {
            key.clear(f);
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let shape = schema
            .reserve_gc_local(f)
            .initialize(result.load(schema, f).require_non_null(f), f);
        result.clear(f);
        Ok(shape)
    }

    /// The inline storage is built holding both values, as the two compact
    /// stores into a fresh two-key storage would leave it: slot 0 `value`,
    /// slot 1 the Boolean `done`, each a complete scalar/tag/reference tuple.
    pub(crate) fn emit_alloc_iterator_result_object(
        &mut self,
        prototype: &ValueLocals,
        value: &ValueLocals,
        done: I32Local,
        f: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        let schema = self.runtime_schema();
        let shape = self.emit_iterator_result_shape(f)?;
        let tags = schema.reserve_i32_local(f);
        let undefined = WasmRuntimeValueTag::Undefined as i32;
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(255));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Const(
            ((WasmRuntimeValueTag::Boolean as i32) << 8)
                | (undefined << 16)
                | (undefined << 24),
        ));
        f.instruction(&Instruction::I32Or);
        tags.store(f);
        let boolean = schema.reserve_value_local(f);
        boolean.set_boolean(done, f);
        let inline = schema.reserve_gc_local(f).initialize(
            schema.struct_type::<InlineCompactDataStorage>().construct(
                (
                    GcOperand::reference(&shape, schema),
                    GcOperand::i32_local(tags),
                    GcOperand::i64_local(value.scalar()),
                    GcOperand::i64_local(boolean.scalar()),
                    GcOperand::i64(0),
                    GcOperand::i64(0),
                    GcOperand::eq_reference(value.reference()),
                    GcOperand::eq_reference(boolean.reference()),
                    GcOperand::null_eq(),
                    GcOperand::null_eq(),
                ),
                f,
            ),
            f,
        );
        let base = schema
            .reserve_gc_local::<ObjectPropertyStorage, NonNullable>(f)
            .initialize(
                inline
                    .load(schema, f)
                    .upcast::<CompactDataStorage>()
                    .upcast(),
                f,
            );
        let immutable = schema.reserve_i32_local(f);
        immutable.set_constant(0, f);
        let object = schema.reserve_gc_local(f).initialize(
            self.emit_alloc_object_header_with_storage(prototype, immutable, &base, f),
            f,
        );
        schema.release_i32_local(immutable, f);
        base.clear(f);
        inline.clear(f);
        boolean.clear(f);
        schema.release_i32_local(tags, f);
        shape.clear(f);
        Ok(object)
    }
}
