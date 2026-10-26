//! Native name/length properties use ordinary compact data storage. Only the
//! immutable primitive-key shape is shared; every callable owns its values.
use super::*;

impl FunctionBuilder<'_> {
    fn emit_native_function_metadata_shape(
        &mut self,
        configurable: I32Local,
        f: &mut Function,
    ) -> Result<GcLocal<CompactDataShape>, EmitError> {
        let schema = self.runtime_schema();
        let result = schema
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize_null(schema, f);
        schema.load_native_metadata_shape(configurable, &result, f);
        result.load(schema, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let mut keys = Vec::with_capacity(2);
        for name in ["length", "name"] {
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
        let zero = schema.reserve_i32_local(f);
        zero.set_constant(0, f);
        let shape = schema.reserve_gc_local(f).initialize(
            schema.struct_type::<CompactDataShape>().construct(
                (
                    GcOperand::reference(&key_table, schema),
                    GcOperand::data_descriptor_flags(zero, zero, configurable),
                ),
                f,
            ),
            f,
        );
        schema.publish_native_metadata_shape(configurable, &shape, f);
        result.replace(shape.load(schema, f).nullable(), f);
        shape.clear(f);
        schema.release_i32_local(zero, f);
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

    /// Called by the closed native-code allocation arm before publication.
    /// No dictionary descriptor/entry or boxed StoredValue is created for the
    /// two metadata fields. Structural changes use the normal one-way promotion.
    pub(crate) fn emit_alloc_native_function_header(
        &mut self,
        meta: &WasmFunctionMeta,
        prototype: &ValueLocals,
        f: &mut Function,
    ) -> Result<GcStackReference<OrdinaryObject>, EmitError> {
        let schema = self.runtime_schema();
        let configurable = schema.reserve_i32_local(f);
        configurable.set_constant(i32::from(meta.length_name_configurable), f);
        let length = schema.reserve_i64_local(f);
        f.instruction(&Instruction::I64Const((meta.length as f64).to_bits() as i64));
        length.store(f);
        let name = schema.reserve_gc_local(f).initialize(
            self.emit_interned_string_reference(meta.runtime_name(), f)?,
            f,
        );
        let reference = schema.helper_reference_on_stack(schema.call_helper(
            crate::runtime_helpers::NativeFunctionAllocateArguments::new(
                prototype,
                &name,
                length,
                configurable,
            ),
            self.runtime_helper_base()?,
            f,
        ));
        name.clear(f);
        schema.release_i64_local(length, f);
        schema.release_i32_local(configurable, f);
        Ok(reference)
    }

    pub(crate) fn compile_native_function_allocate_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut body = self.begin_helper_body(RuntimeHelperId::NativeFunctionAllocate);
        let p = self.helper_parameters::<crate::runtime_helpers::NativeFunctionAllocateParameters>(
            &mut body,
        );
        let f = &mut body;
        let schema = self.runtime_schema();
        let shape = self.emit_native_function_metadata_shape(p.configurable, f)?;
        let reference = schema.helper_reference_on_stack(schema.call_helper(
            crate::runtime_helpers::CompactLiteralAllocateArguments::new(&p.prototype, &shape),
            self.runtime_helper_base()?,
            f,
        ));
        let object = schema.reserve_gc_local(f).initialize(reference, f);
        let base = schema.reserve_gc_local(f).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, schema, f)
                .reference(),
            f,
        );
        let storage = schema.reserve_gc_local(f).initialize(
            base.load(schema, f)
                .cast_subtype::<CompactDataStorage>(schema, f),
            f,
        );
        let slot = schema.reserve_i32_local(f);
        let value = schema.reserve_value_local(f);
        slot.set_constant(0, f);
        value.set_number(p.length_bits, f);
        self.emit_compact_store_at(&storage, slot, &value, f);
        slot.set_constant(1, f);
        value.set_reference(&p.name, schema, f);
        self.emit_compact_store_at(&storage, slot, &value, f);
        value.clear(f);
        schema.release_i32_local(slot, f);
        storage.clear(f);
        base.clear(f);
        shape.clear(f);
        object.load(schema, f);
        object.clear(f);
        p.release(f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(body))
    }
}
