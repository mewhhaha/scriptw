//! String-only access to canonical Set storage without JavaScript observation.
use super::*;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_private_string_set(
        &mut self,
        f: &mut Function,
    ) -> Result<GcLocal<SetObject>, EmitError> {
        let s = self.runtime_schema();
        // A fresh null-prototype header belongs only to this unpublished Set.
        // No source object header or defining-Realm intrinsic is borrowed.
        let header = s
            .reserve_gc_local(f)
            .initialize(self.emit_alloc_plain_object_with_prototype(None, f)?, f);
        let set = self.emit_collection_alloc_set(&header, f);
        header.clear(f);
        Ok(set)
    }

    pub(crate) fn emit_private_string_set_has(
        &mut self,
        set: &GcLocal<SetObject>,
        string: &GcLocal<StringValue>,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let key = s.reserve_value_local(f);
        key.set_reference(string, s, f);
        let index = s.reserve_i64_local(f);
        self.emit_collection_find_set(set, &key, index, found, f)?;
        s.release_i64_local(index, f);
        key.clear(f);
        Ok(())
    }

    pub(crate) fn emit_private_string_set_add(
        &mut self,
        set: &GcLocal<SetObject>,
        string: &GcLocal<StringValue>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let key = s.reserve_value_local(f);
        key.set_reference(string, s, f);
        // Reacquire current buckets after any descriptor trap. No entry/slot
        // from the preceding membership query crosses callback-capable work.
        self.emit_collection_put_set(set, &key, f)?;
        key.clear(f);
        Ok(())
    }
}
