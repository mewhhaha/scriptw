//! Static reads retain immutable key/shape/slot proofs across operations and
//! loop iterations. P reloads actual storage and its shape before each typed
//! load; only a miss enters the complete cached Get kernel in R.
use super::*;
use crate::gc_types::{
    CompactDataStorage, CompactDataStorageSchema, OwnDataReadCache, OwnDataReadCacheSchema,
};
use scriptw_ir::{HeapShape, HeapShapeProvenance, ObjectShapeProperty};

enum PropertyReadPlan {
    General,
    CachedStaticString,
}
impl PropertyReadPlan {
    fn for_read(target: &TypedExpr, key: &PropertyKeyIr) -> Self {
        match key {
            PropertyKeyIr::StaticString(name) => {
                if !target.possible_kinds.contains(ValueKind::Object) {
                    return Self::General;
                }
                match target.heap_shape.as_deref() {
                    Some(HeapShape::Object(shape)) => {
                        if shape.provenance == HeapShapeProvenance::Program
                            && matches!(
                                shape.properties.get(name),
                                Some(ObjectShapeProperty::Data(_))
                            )
                        {
                            Self::CachedStaticString
                        } else {
                            Self::General
                        }
                    }
                    Some(HeapShape::Array(_)) => Self::General,
                    None => Self::CachedStaticString,
                }
            }
            PropertyKeyIr::ArrayLength
            | PropertyKeyIr::ArrayIndex(_)
            | PropertyKeyIr::StringExpr(_) => Self::General,
        }
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_planned_property_read(
        &mut self,
        target: &TypedExpr,
        key: &PropertyKeyIr,
        lookup: &ValueLocals,
        receiver: &ValueLocals,
        key_value: &ValueLocals,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> Result<bool, EmitError> {
        if !self.has_source_execution_environment() {
            return Ok(false);
        }
        match PropertyReadPlan::for_read(target, key) {
            PropertyReadPlan::General => Ok(false),
            PropertyReadPlan::CachedStaticString => {
                let site = self.next_program_site(crate::emit::ProgramSiteKind::OwnDataRead)?;
                let schema = self.runtime_schema();
                let id = schema.reserve_i32_local(f);
                site.emit(id, f);
                // Selection admits only a compiler-owned static String key.
                // The helper accepts a completed key, never raw coercible input.
                let name = schema.reserve_gc_local(f).initialize(
                    key_value.cast_reference::<crate::gc_types::StringValue>(schema, f),
                    f,
                );
                let canonical_key = PropertyKeyLocals::from_string(schema, &name, f);
                name.clear(f);
                let found =
                    self.emit_guarded_cached_own_data_read(lookup, &canonical_key, id, result, f);
                found.load(f);
                f.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, f);
                schema
                    .call_helper(
                        crate::runtime_helpers::CachedPropertyReadArguments::new(
                            lookup,
                            receiver,
                            &canonical_key,
                            id,
                            self.current_environment(),
                            self.current_program(),
                        ),
                        self.runtime_helper_base()?,
                        f,
                    )
                    .store(result, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                schema.release_i32_local(found, f);
                canonical_key.clear(f);
                schema.release_i32_local(id, f);
                Ok(true)
            }
        }
    }

    fn emit_guarded_cached_own_data_read(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        site: I32Local,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> I32Local {
        let schema = self.runtime_schema();
        // This root belongs to this emitted source site for the entire Wasm
        // invocation. It retains metadata only, never a receiver or its values.
        let retained = schema.reserve_gc_frame_cache::<OwnDataReadCache>(f);
        let found = schema.reserve_i32_local(f);
        found.set_constant(0, f);
        let header = self.emit_simple_data_header(target, f);
        header.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let object = schema
            .reserve_gc_local(f)
            .initialize(header.load(schema, f).require_non_null(f), f);
        let base = schema.reserve_gc_local(f).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, schema, f)
                .reference(),
            f,
        );
        base.is_subtype::<CompactDataStorage>(schema, f);
        self.open_frame(ControlFrameKind::If, f);
        let storage = schema.reserve_gc_local(f).initialize(
            base.load(schema, f)
                .cast_subtype::<CompactDataStorage>(schema, f),
            f,
        );
        retained.load(schema, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let registered = self.emit_own_data_cache_site(site, key.value(), f);
        retained.replace(registered.load(schema, f), f);
        registered.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let cache = schema
            .reserve_gc_local(f)
            .initialize(retained.load(schema, f).require_non_null(f), f);
        let shape = schema.reserve_gc_local(f).initialize(
            schema
                .field(CompactDataStorageSchema::SHAPE)
                .read(&storage, schema, f)
                .reference(),
            f,
        );
        let cached_shape = schema.reserve_gc_local(f).initialize(
            schema
                .field(OwnDataReadCacheSchema::SHAPE)
                .read(&cache, schema, f)
                .reference(),
            f,
        );
        let cached_key = schema.reserve_gc_local(f).initialize(
            schema
                .field(OwnDataReadCacheSchema::KEY)
                .read(&cache, schema, f)
                .reference(),
            f,
        );
        shape.load(schema, f);
        cached_shape.load(schema, f);
        f.instruction(&Instruction::RefEq);
        cached_key.load(schema, f);
        key.value().reference().load(f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        // The immutable shape proves an own data slot. Load this object's live
        // scalar/reference value; a callback, promotion or other object can
        // never reuse a previously loaded value or storage reference.
        let slot = schema.reserve_i32_local(f);
        schema
            .field(OwnDataReadCacheSchema::SLOT)
            .read(&cache, schema, f)
            .store(slot, f);
        let value = schema.reserve_value_local(f);
        self.emit_compact_value_at(&storage, slot, &value, f);
        result.set_normal(&value, f);
        found.set_constant(1, f);
        value.clear(f);
        schema.release_i32_local(slot, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cached_key.clear(f);
        cached_shape.clear(f);
        shape.clear(f);
        cache.clear(f);
        storage.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        base.clear(f);
        object.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        header.clear(f);
        found
    }
}
