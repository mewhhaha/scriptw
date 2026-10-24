//! A validated literal plan owns creation of its fresh ordinary object.
//!
//! Static distinct data fields cannot reject definition on that object. Their
//! values still run through ordinary expression/throw emission in source order;
//! compact values or the shared append kernel own private publication.
use super::*;
use std::collections::BTreeSet;

const MAX_PLAIN_DATA_FIELDS: usize = 64;
const MAX_COMPACT_DATA_FIELDS: usize = 16;

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
        // no exposure before publication and every admitted field is new.
        let object = schema.reserve_gc_local(function).initialize(
            builder.emit_alloc_plain_object_with_prototype(Some(&prototype), function)?,
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
        let site = builder.next_program_site(crate::emit::ProgramSiteKind::CompactShape)?;
        let names: Vec<&str> = self.fields.iter().map(|field| field.key).collect();
        let shape = builder.emit_compact_literal_shape(&names, site, f)?;
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
        for (index, field) in self.fields.into_iter().enumerate() {
            builder.compile_expr_to_value(field.value, &value, f)?;
            builder.emit_propagate_current_throw_if_needed(f);
            f.instruction(&Instruction::I32Const(index as i32));
            slot.store(f);
            builder.emit_compact_store_at(&storage, slot, &value, f);
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
