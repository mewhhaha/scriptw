//! Static source read sites call a bounded cache kernel in optimized R. The
//! cache retains only primitive key/immutable shape metadata; live storage and
//! shape checks precede every direct value load, including reads after calls.
use super::*;
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
                canonical_key.clear(f);
                schema.release_i32_local(id, f);
                Ok(true)
            }
        }
    }
}
