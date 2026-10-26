use super::*;

impl FunctionBuilder<'_> {
    /// One lazy shape root per literal site and physical ProgramInstance. Keys
    /// are primitive strings in source insertion order, never runtime values.
    pub(in crate::objects) fn emit_compact_literal_shape(
        &mut self,
        names: &[&str],
        site: crate::emit::ProgramSiteId,
        f: &mut Function,
    ) -> Result<GcLocal<CompactDataShape>, EmitError> {
        let schema = self.runtime_schema();
        let id = schema.reserve_i32_local(f);
        site.emit(id, f);
        let (table, slot) = self.current_program().compact_shapes(id, schema, f);
        let result = schema.reserve_gc_local(f).initialize(
            schema
                .array_type::<CompactShapeTable>()
                .read(&table, slot, schema, f)
                .reference(),
            f,
        );
        result.load(schema, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let mut keys = Vec::with_capacity(names.len());
        for name in names {
            keys.push(
                schema
                    .reserve_gc_local(f)
                    .initialize(self.emit_interned_string_reference(name, f)?, f),
            );
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
                        StoredPropertyAttributes::Data {
                            writable: true,
                            enumerable: true,
                            configurable: true,
                        }
                        .descriptor_word(),
                    ),
                ),
                f,
            ),
            f,
        );
        schema.array_type::<CompactShapeTable>().write(
            &table,
            slot,
            GcOperand::nullable_reference(&shape, schema),
            schema,
            f,
        );
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
        table.clear(f);
        schema.release_i32_local(slot, f);
        schema.release_i32_local(id, f);
        Ok(shape)
    }

    pub(crate) fn compile_compact_literal_allocate_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::CompactLiteralAllocate);
        let p = self
            .helper_parameters::<crate::runtime_helpers::CompactLiteralAllocateParameters>(&mut f);
        let schema = self.runtime_schema();
        let keys = schema.reserve_gc_local(&mut f).initialize(
            schema
                .field(CompactDataShapeSchema::KEYS)
                .read(&p.shape, schema, &mut f)
                .reference(),
            &mut f,
        );
        let length = schema.reserve_i32_local(&mut f);
        schema
            .array_type::<CompactShapeKeys>()
            .length(&keys, schema, &mut f);
        length.store(&mut f);
        let undefined = schema.reserve_value_local(&mut f);
        undefined.set_undefined(&mut f);
        let tags = schema.reserve_gc_local(&mut f).initialize(
            schema.array_type::<CompactTagArray>().filled(
                GcOperand::i32(WasmRuntimeValueTag::Undefined as i32),
                length,
                &mut f,
            ),
            &mut f,
        );
        let scalars = schema.reserve_gc_local(&mut f).initialize(
            schema
                .array_type::<CompactScalarArray>()
                .filled(GcOperand::i64(0), length, &mut f),
            &mut f,
        );
        let references = schema.reserve_gc_local(&mut f).initialize(
            schema.array_type::<CompactReferenceArray>().filled(
                GcOperand::eq_reference(undefined.reference()),
                length,
                &mut f,
            ),
            &mut f,
        );
        let compact = schema.reserve_gc_local(&mut f).initialize(
            schema.struct_type::<CompactDataStorage>().construct(
                (
                    GcOperand::reference(&p.shape, schema),
                    GcOperand::reference(&tags, schema),
                    GcOperand::reference(&scalars, schema),
                    GcOperand::reference(&references, schema),
                ),
                &mut f,
            ),
            &mut f,
        );
        let base = schema
            .reserve_gc_local(&mut f)
            .initialize(compact.load(schema, &mut f).upcast(), &mut f);
        let mutable = schema.reserve_i32_local(&mut f);
        f.instruction(&Instruction::I32Const(0));
        mutable.store(&mut f);
        let object =
            self.emit_alloc_object_header_with_storage(&p.prototype, mutable, &base, &mut f);
        base.clear(&mut f);
        compact.clear(&mut f);
        references.clear(&mut f);
        scalars.clear(&mut f);
        tags.clear(&mut f);
        undefined.clear(&mut f);
        keys.clear(&mut f);
        schema.release_i32_local(mutable, &mut f);
        schema.release_i32_local(length, &mut f);
        // Keep the actual header on the operand stack through root retirement.
        let _ = object;
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
}
