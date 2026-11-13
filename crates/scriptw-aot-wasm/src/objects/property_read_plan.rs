//! Static reads keep a per-site record in a P global: the site answers a
//! recorded own compact slot inline and otherwise calls the typed R hit
//! entry with the record. R owns registration and the complete Get fallback.
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
    ) -> Result<Option<crate::gc_types::SiteRecordGlobal>, EmitError> {
        if !self.has_source_execution_environment() {
            return Ok(None);
        }
        match PropertyReadPlan::for_write(target, key) {
            PropertyReadPlan::General => Ok(None),
            PropertyReadPlan::CachedStaticString => self.next_site_record().map(Some),
        }
    }

    /// Whether a read of `target.key` goes through a static read site.
    pub(crate) fn plans_static_read(&self, target: &TypedExpr, key: &PropertyKeyIr) -> bool {
        self.has_source_execution_environment()
            && matches!(
                PropertyReadPlan::for_read(target, key),
                PropertyReadPlan::CachedStaticString
            )
    }

    /// A planned static read of `target.name`: the site's record answers or
    /// the R hit entry does. A static read's raw base is both its lookup
    /// target and its receiver, and its compiler-owned String key needs no
    /// coercion, so the key is materialized only when R makes the site's
    /// record. Only a base that may be an object can be the actual
    /// OrdinaryObject the site's inline proof answers.
    pub(crate) fn emit_planned_property_read(
        &mut self,
        target: &TypedExpr,
        name: &str,
        base: &ValueLocals,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let may_be_ordinary = target.possible_kinds.contains(ValueKind::Object);
        self.emit_static_read_site(name, may_be_ordinary, base, result, f)
    }
}
