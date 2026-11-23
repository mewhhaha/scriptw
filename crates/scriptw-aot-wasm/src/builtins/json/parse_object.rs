//! Fresh JSON objects grow along the compact shape transitions object literals use.
use super::*;

impl FunctionBuilder<'_> {
    /// A fresh extensible ordinary object inheriting %Object.prototype%,
    /// born on the shared empty compact storage like `{}`.
    pub(super) fn emit_json_fresh_object(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        f: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        let s = self.runtime_schema();
        let prototype = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &prototype,
            f,
        );
        let object = s.reserve_gc_local(f).initialize(
            self.emit_alloc_compact_object_with_prototype(&prototype, f)?,
            f,
        );
        prototype.clear(f);
        Ok(object)
    }

    /// CreateDataProperty(container, key, value) for the object the parser
    /// created with `emit_json_fresh_object` and still owns privately. Its
    /// [[DefineOwnProperty]] is the ordinary one, whose first step for this
    /// complete default data descriptor is the compact kernel: a present key
    /// keeps its slot (a duplicate overwrites in place, at its first
    /// position) and an absent non-index String key moves to the canonical
    /// child shape. The kernel declines without mutation otherwise (an array
    /// index, a shape bound, a dictionary), and the general definition then
    /// promotes to the dictionary that orders integer keys first. Every key
    /// is a String, so "__proto__" is an own data key like any other.
    pub(super) fn emit_json_object_member(
        &mut self,
        container: &ValueLocals,
        key: &GcLocal<StringValue>,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let object = s
            .reserve_gc_local(f)
            .initialize(container.cast_reference::<OrdinaryObject>(s, f), f);
        let property = PropertyKeyLocals::from_string(s, key, f);
        let handled = s.reserve_i32_local(f);
        s.call_helper(
            crate::runtime_helpers::CompactDataDefineArguments::new(&object, &property, value),
            self.runtime_helper_base()?,
            f,
        )
        .store(handled, f);
        property.clear(f);
        object.clear(f);
        handled.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_define(container, key, value, false, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(handled, f);
        Ok(())
    }
}
