//! A validated literal plan owns creation of its fresh ordinary object.
//!
//! Static distinct data fields cannot reject definition on that object. Their
//! values still run through ordinary expression/throw emission in source order;
//! compact values or the shared append kernel own private publication.
use super::*;
use std::collections::BTreeSet;

const MAX_PLAIN_DATA_FIELDS: usize = 64;
pub(crate) const MAX_COMPACT_DATA_FIELDS: usize = 16;

struct PlainDataField<'ir> {
    key: &'ir str,
    value: &'ir TypedExpr,
}

struct PlainDataLiteral<'ir> {
    fields: Vec<PlainDataField<'ir>>,
}

impl<'ir> PlainDataLiteral<'ir> {
    fn validate(properties: &'ir [ObjectPropertyIr]) -> Option<Self> {
        if properties.len() > MAX_PLAIN_DATA_FIELDS {
            return None;
        }
        let mut keys = BTreeSet::new();
        let mut fields = Vec::with_capacity(properties.len());
        for property in properties {
            match property {
                ObjectPropertyIr::Data { key, value, .. } => {
                    if !keys.insert(key.as_str()) {
                        return None;
                    }
                    fields.push(PlainDataField { key, value });
                }
                ObjectPropertyIr::PrototypeSetter { .. }
                | ObjectPropertyIr::Spread { .. }
                | ObjectPropertyIr::NonEnumerableData { .. }
                | ObjectPropertyIr::ComputedData { .. }
                | ObjectPropertyIr::ComputedMethod { .. }
                | ObjectPropertyIr::ComputedGetter { .. }
                | ObjectPropertyIr::ComputedSetter { .. }
                | ObjectPropertyIr::Method { .. }
                | ObjectPropertyIr::Getter { .. }
                | ObjectPropertyIr::Setter { .. } => return None,
            }
        }
        Some(Self { fields })
    }

    fn emit(
        self,
        builder: &mut FunctionBuilder<'_>,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if !self.fields.is_empty()
            && self.fields.len() <= MAX_COMPACT_DATA_FIELDS
            && builder.has_source_execution_environment()
        {
            return self.emit_compact(builder, output, function);
        }
        let schema = builder.runtime_schema();
        let prototype = schema.reserve_value_local(function);
        builder.emit_source_literal_prototype_to_value(
            crate::environments::global_environment::SourceLiteralPrototype::Object,
            &prototype,
            function,
        );
        // No existing object can enter this path. The private fresh object has
        // no exposure before publication and every admitted field is new. An
        // empty literal starts compact so later plain data adds stay shaped.
        let object = schema.reserve_gc_local(function).initialize(
            if self.fields.is_empty() {
                builder.emit_alloc_compact_object_with_prototype(&prototype, function)?
            } else {
                builder.emit_alloc_plain_object_with_prototype(Some(&prototype), function)?
            },
            function,
        );
        let value = schema.reserve_value_local(function);
        for field in self.fields {
            let name = schema.reserve_gc_local(function).initialize(
                builder.emit_interned_string_reference(field.key, function)?,
                function,
            );
            let key = PropertyKeyLocals::from_string(schema, &name, function);
            name.clear(function);
            builder.compile_expr_to_value(field.value, &value, function)?;
            builder.emit_propagate_current_throw_if_needed(function);
            builder.emit_object_append_data_property_with_flags(
                &object, &key, &value, true, true, true, function,
            )?;
            key.clear(function);
        }
        output.set_reference(&object, schema, function);
        value.clear(function);
        object.clear(function);
        prototype.clear(function);
        Ok(())
    }
    fn emit_compact(
        self,
        builder: &mut FunctionBuilder<'_>,
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        use crate::gc_types::*;
        let schema = builder.runtime_schema();
        let names: Vec<&str> = self.fields.iter().map(|field| field.key).collect();
        let shape = builder.emit_compact_literal_shape(&names, f)?;
        if self.fields.len() <= super::compact_data::INLINE_COMPACT_FIELDS {
            // Evaluate every field expression in source order before allocating
            // the fresh inline record so all 1..=4 values are published in one
            // struct.new with zero R helper calls.
            let mut values = Vec::with_capacity(self.fields.len());
            for field in self.fields {
                let value = schema.reserve_value_local(f);
                builder.compile_expr_to_value(field.value, &value, f)?;
                builder.emit_propagate_current_throw_if_needed(f);
                values.push(value);
            }
            let prototype = schema.reserve_value_local(f);
            builder.emit_source_literal_prototype_to_value(
                crate::environments::global_environment::SourceLiteralPrototype::Object,
                &prototype,
                f,
            );
            let undefined = WasmRuntimeValueTag::Undefined as i32;
            let mut unused_tags = 0i32;
            for slot in values.len()..super::compact_data::INLINE_COMPACT_FIELDS {
                unused_tags |= undefined << (slot * 8);
            }
            let tags = schema.reserve_i32_local(f);
            f.instruction(&Instruction::I32Const(unused_tags));
            for (slot, value) in values.iter().enumerate() {
                value.tag().load(f);
                f.instruction(&Instruction::I32Const(255));
                f.instruction(&Instruction::I32And);
                if slot > 0 {
                    f.instruction(&Instruction::I32Const((slot * 8) as i32));
                    f.instruction(&Instruction::I32Shl);
                }
                f.instruction(&Instruction::I32Or);
            }
            tags.store(f);
            let scalar_operand = |index: usize| {
                values
                    .get(index)
                    .map_or(GcOperand::i64(0), |v| GcOperand::i64_local(v.scalar()))
            };
            let ref_operand = |index: usize| {
                values
                    .get(index)
                    .map_or(GcOperand::null_eq(), |v| GcOperand::eq_reference(v.reference()))
            };
            let inline = schema.reserve_gc_local(f).initialize(
                schema.struct_type::<InlineCompactDataStorage>().construct(
                    (
                        GcOperand::reference(&shape, schema),
                        GcOperand::i32_local(tags),
                        scalar_operand(0),
                        scalar_operand(1),
                        scalar_operand(2),
                        scalar_operand(3),
                        ref_operand(0),
                        ref_operand(1),
                        ref_operand(2),
                        ref_operand(3),
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
                builder.emit_alloc_object_header_with_storage(&prototype, immutable, &base, f),
                f,
            );
            output.set_reference(&object, schema, f);
            object.clear(f);
            schema.release_i32_local(immutable, f);
            base.clear(f);
            inline.clear(f);
            schema.release_i32_local(tags, f);
            prototype.clear(f);
            for value in values.into_iter().rev() {
                value.clear(f);
            }
            shape.clear(f);
            return Ok(());
        }
        let prototype = schema.reserve_value_local(f);
        builder.emit_source_literal_prototype_to_value(
            crate::environments::global_environment::SourceLiteralPrototype::Object,
            &prototype,
            f,
        );
        // The compact allocator owns the fresh header; no generic descriptor
        // operation can accidentally redefine an existing object's properties.
        let object_reference = schema.helper_reference_on_stack(schema.call_helper(
            crate::runtime_helpers::CompactLiteralAllocateArguments::new(&prototype, &shape),
            builder.runtime_helper_base()?,
            f,
        ));
        let object = schema.reserve_gc_local(f).initialize(object_reference, f);
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
        let value = schema.reserve_value_local(f);
        let slot = schema.reserve_i32_local(f);
        // The allocator chose the layout from this same key count, and the
        // fresh storage is reachable only from here until its last store. A
        // shape with arrays takes a value without a scalar word inline once
        // it has its reference backing; R makes every other store, and the
        // one that allocates a backing.
        let array = (self.fields.len() > super::compact_data::INLINE_COMPACT_FIELDS).then(|| {
            schema
                .reserve_gc_local::<ArrayCompactDataStorage, NonNullable>(f)
                .initialize(storage.load(schema, f).cast_subtype(schema, f), f)
        });
        for (index, field) in self.fields.into_iter().enumerate() {
            builder.compile_expr_to_value(field.value, &value, f)?;
            builder.emit_propagate_current_throw_if_needed(f);
            f.instruction(&Instruction::I32Const(index as i32));
            slot.store(f);
            let Some(array) = &array else {
                builder.emit_compact_store_at(&storage, slot, &value, f);
                continue;
            };
            f.instruction(&Instruction::Block(BlockType::Empty));
            let stored = f.label_depth();
            f.instruction(&Instruction::Block(BlockType::Empty));
            let general = f.label_depth();
            builder.emit_fresh_array_compact_store_or_branch(array, slot, &value, general, f);
            f.branch_to_label(stored);
            f.instruction(&Instruction::End);
            builder.emit_compact_store_at(&storage, slot, &value, f);
            f.instruction(&Instruction::End);
        }
        if let Some(array) = array {
            array.clear(f);
        }
        output.set_reference(&object, schema, f);
        schema.release_i32_local(slot, f);
        value.clear(f);
        storage.clear(f);
        base.clear(f);
        object.clear(f);
        prototype.clear(f);
        shape.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn try_compile_plain_data_literal(
        &mut self,
        properties: &[ObjectPropertyIr],
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(plan) = PlainDataLiteral::validate(properties) else {
            return Ok(false);
        };
        plan.emit(self, output, function)?;
        Ok(true)
    }
}
