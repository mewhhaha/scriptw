//! Static reads identify a ProgramInstance site and call the typed R cache kernel.
//! R owns registration, live storage guards, value loads and complete Get fallback.
//! Static assignments that would cache the same read get a write record site;
//! R likewise owns its guards and the complete Set fallback.
use super::*;
use scriptw_ir::{HeapShape, HeapShapeProvenance};

enum PropertyReadPlan {
    General,
    CachedStaticString,
}
impl PropertyReadPlan {
    /// A static read caches whenever its base may be an object or primitive
    /// that a record or the call-free walk answers for: they read primitives
    /// through their Realm's prototype without the wrapper ToObject would
    /// allocate. Nullish bases and arguments objects keep the general read.
    fn for_read(target: &TypedExpr, key: &PropertyKeyIr) -> Self {
        let general = scriptw_ir::KindSet::NULLISH
            .union(scriptw_ir::KindSet::from_kind(ValueKind::Arguments));
        if target.possible_kinds.is_subset_of(general) {
            return Self::General;
        }
        Self::for_static_key(target, key)
    }

    /// A write record serves only an ordinary object receiver.
    fn for_write(target: &TypedExpr, key: &PropertyKeyIr) -> Self {
        if !target.possible_kinds.contains(ValueKind::Object) {
            return Self::General;
        }
        Self::for_static_key(target, key)
    }

    fn for_static_key(target: &TypedExpr, key: &PropertyKeyIr) -> Self {
        match key {
            PropertyKeyIr::StaticString(_) => match target.heap_shape.as_deref() {
                Some(HeapShape::Object(shape)) => {
                    if shape.provenance == HeapShapeProvenance::Program {
                        Self::CachedStaticString
                    } else {
                        Self::General
                    }
                }
                Some(HeapShape::Array(_)) => Self::General,
                None => Self::CachedStaticString,
            },
            PropertyKeyIr::ArrayLength
            | PropertyKeyIr::ArrayIndex(_)
            | PropertyKeyIr::StringExpr(_) => Self::General,
        }
    }
}

impl FunctionBuilder<'_> {
    /// The write record site of an assignment to `target.key`, when the
    /// same reference read would be cached: a compiler-owned static String
    /// key on a base that can be an object.
    pub(crate) fn planned_property_write_site(
        &mut self,
        target: &TypedExpr,
        key: &PropertyKeyIr,
    ) -> Result<Option<crate::emit::ProgramSiteId>, EmitError> {
        if !self.has_source_execution_environment() {
            return Ok(None);
        }
        match PropertyReadPlan::for_write(target, key) {
            PropertyReadPlan::General => Ok(None),
            PropertyReadPlan::CachedStaticString => self
                .next_program_site(crate::emit::ProgramSiteKind::OwnDataWrite)
                .map(Some),
        }
    }

    /// A static read's raw base is both its lookup target and its receiver,
    /// so the helper takes it once.
    pub(crate) fn emit_planned_property_read(
        &mut self,
        target: &TypedExpr,
        key: &PropertyKeyIr,
        base: &ValueLocals,
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
                // The helper accepts that String, never raw coercible input.
                let name = schema.reserve_gc_local(f).initialize(
                    key_value.cast_reference::<crate::gc_types::StringValue>(schema, f),
                    f,
                );
                schema
                    .call_helper(
                        crate::runtime_helpers::CachedPropertyReadArguments::new(
                            base,
                            id,
                            &name,
                            self.current_environment(),
                            self.current_program(),
                        ),
                        self.runtime_helper_base()?,
                        f,
                    )
                    .store(result, f);
                name.clear(f);
                schema.release_i32_local(id, f);
                Ok(true)
            }
        }
    }
}
