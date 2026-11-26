//! Realm protectors: facts about a Realm's intrinsic prototypes that a fast
//! path may assume instead of re-proving them at every use.
//!
//! When a Realm's intrinsics are complete, `emit_arm_realm_protectors`
//! records the Realm's %Object.prototype%, %Function.prototype%,
//! %Array.prototype% and %String.prototype% in an immutable
//! `RealmProtectors` record, each with the intrinsic storage (its *rows*)
//! its ordinary header held then. A protector is read only through the
//! accessors below, each named for the invariant it proves, and each
//! re-checks in a few loads the one live fact that can falsify it:
//!
//! - *rows unchanged*: the header still holds the recorded storage. An
//!   intrinsic layout's rows (ADDRESS, COUNT) are immutable, and every
//!   structural edit of such a header — a define, a delete, an attribute
//!   change, dictionary promotion — replaces its PROPERTIES with new storage
//!   (`emit_promote_intrinsic_properties`, the only writer of a header that
//!   holds intrinsic storage). A replacement therefore falsifies the
//!   protector by itself, for every Realm alike: the check needs no
//!   invalidation bit that mutating code, which may run in another Realm,
//!   would have to find and clear.
//! - *no indices on %Array.prototype%*: its live occupancy COUNT is zero.
//! - *%Array.prototype%'s [[Prototype]] is %Object.prototype%*: compared
//!   live, since SetPrototypeOf may change it.
//!
//! %Object.prototype%'s [[Prototype]] needs no re-check: its rows are
//! recorded only while it is the immutable null, and an immutable-prototype
//! header's [[Prototype]] is never written again (OrdinarySetPrototypeOf
//! declines before its write; IMMUTABLE_PROTOTYPE is an immutable field).
//!
//! Every accessor compares the live object with the *current* Realm's
//! record. An object of another Realm, a Realm whose record is not armed
//! yet, or a falsified fact branches to the caller's complete proof, so
//! each protected path proves a subset of what that proof proves.
use super::*;
use crate::emit::ControlTarget;
use crate::functions::NonArrayRealmIntrinsicSlot;
use crate::gc_types::{
    ArrayObject, ArrayObjectSchema, FunctionObject, FunctionObjectSchema, IntrinsicPropertyStorage,
    IntrinsicPropertyStorageSchema, JavaScriptObject, ObjectPropertyStorage, PrimitiveBox,
    PrimitiveBoxSchema, RealmProtectors, RealmProtectorsSchema, RealmRecord, RealmRecordSchema,
};
use crate::intrinsics::IntrinsicPropertyLayoutId;

/// A protected intrinsic prototype whose recorded rows a prototype place
/// on it may use: the invariant is "this header still holds its recorded
/// intrinsic storage".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtectedPrototype {
    Function,
    Object,
    Array,
}

/// The protected prototypes a caller needs still holding their recorded
/// rows, whatever object reads through them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProtectedPrototypeRows {
    pub(crate) object: bool,
    pub(crate) function: bool,
    pub(crate) array: bool,
    pub(crate) string: bool,
}

impl ProtectedPrototype {
    /// The order a holder's identity is compared in: `.call`/`.apply`
    /// places first, then Object.prototype's, then Array.prototype's.
    pub(crate) const HOLDER_ORDER: [Self; 3] = [Self::Function, Self::Object, Self::Array];
}

/// The header and recorded rows of a protected prototype whose header
/// still holds those rows.
pub(crate) struct ProtectedHolder {
    pub(crate) header: GcLocal<OrdinaryObject>,
    pub(crate) rows: GcLocal<IntrinsicPropertyStorage>,
}

impl ProtectedHolder {
    pub(crate) fn clear(self, f: &mut Function) {
        self.rows.clear(f);
        self.header.clear(f);
    }
}

impl FunctionBuilder<'_> {
    /// Records the Realm's protected prototypes once its intrinsics are
    /// complete; before this the Realm's PROTECTORS is null and every
    /// accessor declines. A prototype whose header does not hold intrinsic
    /// storage is recorded without rows, which no live storage equals.
    pub(crate) fn emit_arm_realm_protectors(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        object_prototype: &ValueLocals,
        function_prototype: &GcLocal<FunctionObject>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let skip = self.open_frame(ControlFrameKind::Block, f);
        let object = s.reserve_non_null_gc_local(f).initialize(
            object_prototype.sealed_reference_or_branch::<OrdinaryObject>(
                s,
                self.local_branch_label(skip),
                f,
            ),
            f,
        );
        let string_value = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::StringPrototype,
            &string_value,
            f,
        );
        let string = s.reserve_non_null_gc_local(f).initialize(
            string_value.sealed_reference_or_branch::<PrimitiveBox>(
                s,
                self.local_branch_label(skip),
                f,
            ),
            f,
        );
        string_value.clear(f);
        let array = s
            .reserve_non_null_gc_local(f)
            .initialize(self.emit_load_realm_array_prototype(realm, f), f);
        let object_rows = self.emit_object_prototype_rows_at_arming(&object, f)?;
        let function_header = s.reserve_non_null_gc_local(f).initialize(
            s.field(FunctionObjectSchema::OBJECT)
                .read(function_prototype, s, f)
                .reference(),
            f,
        );
        let function_rows = self.emit_header_rows_at_arming(&function_header, f);
        function_header.clear(f);
        let array_header = s.reserve_non_null_gc_local(f).initialize(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&array, s, f)
                .reference(),
            f,
        );
        let array_rows = self.emit_header_rows_at_arming(&array_header, f);
        array_header.clear(f);
        let string_header = s.reserve_non_null_gc_local(f).initialize(
            s.field(PrimitiveBoxSchema::OBJECT)
                .read(&string, s, f)
                .reference(),
            f,
        );
        let string_rows = self.emit_header_rows_at_arming(&string_header, f);
        string_header.clear(f);
        let protectors = s.reserve_non_null_gc_local(f).initialize(
            s.struct_type::<RealmProtectors>().construct(
                (
                    GcOperand::reference(&object, s),
                    GcOperand::reference(&object_rows, s),
                    GcOperand::reference(function_prototype, s),
                    GcOperand::reference(&function_rows, s),
                    GcOperand::reference(&array, s),
                    GcOperand::reference(&array_rows, s),
                    GcOperand::reference(&string, s),
                    GcOperand::reference(&string_rows, s),
                ),
                f,
            ),
            f,
        );
        s.field(RealmRecordSchema::PROTECTORS).write(
            realm,
            GcOperand::nullable_reference(&protectors, s),
            s,
            f,
        );
        protectors.clear(f);
        string_rows.clear(f);
        array_rows.clear(f);
        function_rows.clear(f);
        object_rows.clear(f);
        array.clear(f);
        string.clear(f);
        object.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        Ok(())
    }

    /// The header's intrinsic storage, or null for any other storage.
    fn emit_header_rows_at_arming(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        f: &mut Function,
    ) -> GcLocal<IntrinsicPropertyStorage, Nullable> {
        let s = self.runtime_schema();
        let rows = s
            .reserve_gc_local::<IntrinsicPropertyStorage, Nullable>(f)
            .initialize_null(s, f);
        let other = self.open_frame(ControlFrameKind::Block, f);
        let base = s
            .reserve_non_null_gc_local::<ObjectPropertyStorage>(f)
            .initialize(
                s.field(OrdinaryObjectSchema::PROPERTIES)
                    .read(header, s, f)
                    .reference(),
                f,
            );
        rows.replace(
            base.sealed_subtype_or_branch::<IntrinsicPropertyStorage>(
                s,
                self.local_branch_label(other),
                f,
            )
            .nullable(),
            f,
        );
        base.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        rows
    }

    /// %Object.prototype%'s rows when they are those of its layout and its
    /// [[Prototype]] is the immutable null — `emit_negative_cache_mode`'s
    /// closed-tail proof, run once — and null otherwise.
    fn emit_object_prototype_rows_at_arming(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        f: &mut Function,
    ) -> Result<GcLocal<IntrinsicPropertyStorage, Nullable>, EmitError> {
        let s = self.runtime_schema();
        if !self
            .strings
            .has_intrinsic_property_layout(IntrinsicPropertyLayoutId::ObjectPrototype)?
        {
            return Ok(s
                .reserve_gc_local::<IntrinsicPropertyStorage, Nullable>(f)
                .initialize_null(s, f));
        }
        let span = self
            .strings
            .intrinsic_property_layout_span(IntrinsicPropertyLayoutId::ObjectPrototype)?;
        let rows = self.emit_header_rows_at_arming(object, f);
        let keep = self.open_frame(ControlFrameKind::Block, f);
        let reject = self.open_frame(ControlFrameKind::Block, f);
        let storage = s.reserve_non_null_gc_local(f).initialize(
            rows.non_null_or_branch(self.local_branch_label(reject), f),
            f,
        );
        s.field(OrdinaryObjectSchema::PROTOTYPE)
            .read(object, s, f)
            .reference()
            .is_null(f);
        s.field(OrdinaryObjectSchema::IMMUTABLE_PROTOTYPE)
            .read(object, s, f);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(reject, f);
        for (field, expected) in [
            (IntrinsicPropertyStorageSchema::ADDRESS, span.address()),
            (IntrinsicPropertyStorageSchema::COUNT, span.count()),
        ] {
            s.field(field).read(&storage, s, f);
            f.instruction(&Instruction::I32Const(expected as i32));
            f.instruction(&Instruction::I32Ne);
            self.emit_branch_if_to_target(reject, f);
        }
        storage.clear(f);
        self.emit_branch_to_target(keep, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        rows.set_null(s, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        Ok(rows)
    }

    /// The current Realm's armed protector record, after branching to
    /// `disarmed` while it has none.
    fn emit_armed_realm_protectors_or_branch(
        &mut self,
        disarmed: ControlTarget,
        f: &mut Function,
    ) -> GcLocal<RealmProtectors> {
        let s = self.runtime_schema();
        let realm: GcLocal<RealmRecord> = self.load_current_realm(f);
        let protectors = s.reserve_non_null_gc_local(f).initialize(
            s.field(RealmRecordSchema::PROTECTORS)
                .read(&realm, s, f)
                .reference()
                .non_null_or_branch(self.local_branch_label(disarmed), f),
            f,
        );
        realm.clear(f);
        protectors
    }

    /// Leaves on the stack whether %Object.prototype%, read from
    /// `protectors`, still holds its recorded rows.
    fn emit_object_prototype_rows_unchanged_i32(
        &self,
        protectors: &GcLocal<RealmProtectors>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let object = s
            .field(RealmProtectorsSchema::OBJECT_PROTOTYPE)
            .read(protectors, s, f)
            .reference();
        s.field(OrdinaryObjectSchema::PROPERTIES)
            .read_top(object, f);
        s.field(RealmProtectorsSchema::OBJECT_PROTOTYPE_ROWS)
            .read(protectors, s, f);
        f.instruction(&Instruction::RefEq);
    }

    /// Falls through while the current Realm's %String.prototype%,
    /// %Array.prototype% and %Object.prototype% all hold their recorded
    /// untouched intrinsic rows, %String.prototype% and %Array.prototype%
    /// inherit directly from %Object.prototype%, and %Array.prototype% has no
    /// own indexed properties. Used by `.split(sep).join(rep)` fusion.
    pub(crate) fn emit_string_split_join_protectors_or_branch(
        &mut self,
        unprotected: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let protectors = self.emit_armed_realm_protectors_or_branch(unprotected, f);
        // %Object.prototype% rows unchanged (and immutable null [[Prototype]]).
        self.emit_object_prototype_rows_unchanged_i32(&protectors, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        // %String.prototype% rows unchanged and [[Prototype]] == %Object.prototype%.
        let string_proto = s.reserve_non_null_gc_local(f).initialize(
            s.field(RealmProtectorsSchema::STRING_PROTOTYPE)
                .read(&protectors, s, f)
                .reference(),
            f,
        );
        let string_header = s.reserve_non_null_gc_local(f).initialize(
            s.field(PrimitiveBoxSchema::OBJECT)
                .read(&string_proto, s, f)
                .reference(),
            f,
        );
        string_proto.clear(f);
        s.field(OrdinaryObjectSchema::PROPERTIES)
            .read(&string_header, s, f);
        s.field(RealmProtectorsSchema::STRING_PROTOTYPE_ROWS)
            .read(&protectors, s, f);
        f.instruction(&Instruction::RefEq);
        s.field(OrdinaryObjectSchema::PROTOTYPE)
            .read(&string_header, s, f);
        s.field(RealmProtectorsSchema::OBJECT_PROTOTYPE)
            .read(&protectors, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32And);
        string_header.clear(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        // %Array.prototype% rows unchanged, COUNT == 0, [[Prototype]] == %Object.prototype%.
        let array_proto = s.reserve_non_null_gc_local(f).initialize(
            s.field(RealmProtectorsSchema::ARRAY_PROTOTYPE)
                .read(&protectors, s, f)
                .reference(),
            f,
        );
        s.field(ArrayObjectSchema::COUNT)
            .read(&array_proto, s, f);
        f.instruction(&Instruction::I64Eqz);
        let array_header = s.reserve_non_null_gc_local(f).initialize(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&array_proto, s, f)
                .reference(),
            f,
        );
        array_proto.clear(f);
        s.field(OrdinaryObjectSchema::PROPERTIES)
            .read(&array_header, s, f);
        s.field(RealmProtectorsSchema::ARRAY_PROTOTYPE_ROWS)
            .read(&protectors, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32And);
        s.field(OrdinaryObjectSchema::PROTOTYPE)
            .read(&array_header, s, f);
        s.field(RealmProtectorsSchema::OBJECT_PROTOTYPE)
            .read(&protectors, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32And);
        array_header.clear(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        protectors.clear(f);
    }

    /// Falls through when %ArrayIteratorPrototype% -> %IteratorPrototype% ->
    /// %Object.prototype% all still hold their untouched intrinsic layouts
    /// (proving canonical `.next` and absence of `.return`). Anything else
    /// branches to `unprotected`.
    pub(crate) fn emit_array_iterator_prototype_chain_unchanged_or_branch(
        &mut self,
        unprotected: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        if !self
            .strings
            .has_intrinsic_property_layout(IntrinsicPropertyLayoutId::ArrayIteratorPrototype)?
            || !self
                .strings
                .has_intrinsic_property_layout(IntrinsicPropertyLayoutId::IteratorPrototype)?
        {
            self.emit_branch_to_target(unprotected, f);
            return Ok(());
        }
        let array_iter_span = self
            .strings
            .intrinsic_property_layout_span(IntrinsicPropertyLayoutId::ArrayIteratorPrototype)?;
        let iter_span = self
            .strings
            .intrinsic_property_layout_span(IntrinsicPropertyLayoutId::IteratorPrototype)?;
        let s = self.runtime_schema();
        let protectors = self.emit_armed_realm_protectors_or_branch(unprotected, f);
        self.emit_object_prototype_rows_unchanged_i32(&protectors, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        let realm = self.load_current_realm(f);
        let arr_iter_val = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::ArrayIteratorPrototype,
            &arr_iter_val,
            f,
        );
        let iter_val = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::IteratorPrototype,
            &iter_val,
            f,
        );
        realm.clear(f);
        let arr_iter = s.reserve_non_null_gc_local(f).initialize(
            arr_iter_val.sealed_reference_or_branch::<OrdinaryObject>(
                s,
                self.local_branch_label(unprotected),
                f,
            ),
            f,
        );
        arr_iter_val.clear(f);
        let iter_proto = s.reserve_non_null_gc_local(f).initialize(
            iter_val.sealed_reference_or_branch::<OrdinaryObject>(
                s,
                self.local_branch_label(unprotected),
                f,
            ),
            f,
        );
        iter_val.clear(f);
        // Check prototype links: ArrayIteratorPrototype -> IteratorPrototype -> Object.prototype.
        s.field(OrdinaryObjectSchema::PROTOTYPE)
            .read(&arr_iter, s, f);
        let _ = iter_proto.load(s, f);
        f.instruction(&Instruction::RefEq);
        s.field(OrdinaryObjectSchema::PROTOTYPE)
            .read(&iter_proto, s, f);
        s.field(RealmProtectorsSchema::OBJECT_PROTOTYPE)
            .read(&protectors, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32And);
        protectors.clear(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        // Check both headers still hold their untouched IntrinsicPropertyStorage.
        for (owner, expected_addr) in [
            (&arr_iter, array_iter_span.address()),
            (&iter_proto, iter_span.address()),
        ] {
            let base = s.reserve_non_null_gc_local(f).initialize(
                s.field(OrdinaryObjectSchema::PROPERTIES)
                    .read(owner, s, f)
                    .reference(),
                f,
            );
            let storage = s.reserve_non_null_gc_local(f).initialize(
                base.sealed_subtype_or_branch::<IntrinsicPropertyStorage>(
                    s,
                    self.local_branch_label(unprotected),
                    f,
                ),
                f,
            );
            base.clear(f);
            s.field(IntrinsicPropertyStorageSchema::ADDRESS)
                .read(&storage, s, f);
            storage.clear(f);
            f.instruction(&Instruction::I32Const(expected_addr as i32));
            f.instruction(&Instruction::I32Ne);
            self.emit_branch_if_to_target(unprotected, f);
        }
        iter_proto.clear(f);
        arr_iter.clear(f);
        Ok(())
    }

    /// Falls through when `array` is an actual Array whose own header holds an
    /// empty dictionary (no own `@@iterator`), whose [[Prototype]] is unchanged
    /// `%Array.prototype%` -> `%Object.prototype%`, and whose Realm's
    /// `%ArrayIteratorPrototype%` -> `%IteratorPrototype%` -> `%Object.prototype%`
    /// are all untouched. Anything else branches to `unprotected`.
    pub(crate) fn emit_fast_array_for_of_protectors_or_branch(
        &mut self,
        array: &GcLocal<ArrayObject>,
        unprotected: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let header = s.reserve_non_null_gc_local(f).initialize(
            s.field(ArrayObjectSchema::OBJECT).read(array, s, f).reference(),
            f,
        );
        // Empty ordinary storage proves no own [Symbol.iterator].
        let props = s.reserve_non_null_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&header, s, f)
                .reference(),
            f,
        );
        let dict = s.reserve_non_null_gc_local(f).initialize(
            props.sealed_subtype_or_branch::<OrdinaryPropertyStorage>(
                s,
                self.local_branch_label(unprotected),
                f,
            ),
            f,
        );
        props.clear(f);
        s.field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&dict, s, f);
        dict.clear(f);
        self.emit_branch_if_to_target(unprotected, f);
        let prototype = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize(
                s.field(OrdinaryObjectSchema::PROTOTYPE)
                    .read(&header, s, f)
                    .reference(),
                f,
            );
        header.clear(f);
        let protectors = self.emit_armed_realm_protectors_or_branch(unprotected, f);
        let array_proto = s.reserve_non_null_gc_local(f).initialize(
            s.field(RealmProtectorsSchema::ARRAY_PROTOTYPE)
                .read(&protectors, s, f)
                .reference(),
            f,
        );
        let _ = array_proto.load(s, f);
        let _ = prototype.load(s, f);
        f.instruction(&Instruction::RefEq);
        prototype.clear(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        let array_header = s.reserve_non_null_gc_local(f).initialize(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&array_proto, s, f)
                .reference(),
            f,
        );
        array_proto.clear(f);
        s.field(OrdinaryObjectSchema::PROPERTIES)
            .read(&array_header, s, f);
        s.field(RealmProtectorsSchema::ARRAY_PROTOTYPE_ROWS)
            .read(&protectors, s, f);
        f.instruction(&Instruction::RefEq);
        s.field(OrdinaryObjectSchema::PROTOTYPE)
            .read(&array_header, s, f);
        s.field(RealmProtectorsSchema::OBJECT_PROTOTYPE)
            .read(&protectors, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32And);
        array_header.clear(f);
        protectors.clear(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        self.emit_array_iterator_prototype_chain_unchanged_or_branch(unprotected, f)
    }

    /// Falls through when `prototype` is the current Realm's unchanged
    /// %Object.prototype% (with its immutable null [[Prototype]]) or
    /// unchanged %Array.prototype% (whose [[Prototype]] is unchanged
    /// %Object.prototype%): neither intrinsic layout defines "toJSON".
    /// Anything else branches to `unprotected`.
    pub(crate) fn emit_prototype_chain_lacks_to_json_or_branch(
        &mut self,
        prototype: &GcLocal<JavaScriptObject, Nullable>,
        unprotected: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let protectors = self.emit_armed_realm_protectors_or_branch(unprotected, f);
        let accepted = self.open_frame(ControlFrameKind::Block, f);
        // Commonest case: plain object inheriting directly from %Object.prototype%.
        s.field(RealmProtectorsSchema::OBJECT_PROTOTYPE)
            .read(&protectors, s, f);
        let _ = prototype.load(s, f);
        f.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_object_prototype_rows_unchanged_i32(&protectors, f);
        self.emit_branch_if_to_target(accepted, f);
        self.emit_branch_to_target(unprotected, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Second case: Array inheriting from unchanged %Array.prototype% -> %Object.prototype%.
        let array = s.reserve_non_null_gc_local(f).initialize(
            s.field(RealmProtectorsSchema::ARRAY_PROTOTYPE)
                .read(&protectors, s, f)
                .reference(),
            f,
        );
        let _ = array.load(s, f);
        let _ = prototype.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        let array_header = s.reserve_non_null_gc_local(f).initialize(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&array, s, f)
                .reference(),
            f,
        );
        array.clear(f);
        s.field(OrdinaryObjectSchema::PROPERTIES)
            .read(&array_header, s, f);
        s.field(RealmProtectorsSchema::ARRAY_PROTOTYPE_ROWS)
            .read(&protectors, s, f);
        f.instruction(&Instruction::RefEq);
        s.field(OrdinaryObjectSchema::PROTOTYPE)
            .read(&array_header, s, f);
        s.field(RealmProtectorsSchema::OBJECT_PROTOTYPE)
            .read(&protectors, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32And);
        array_header.clear(f);
        self.emit_object_prototype_rows_unchanged_i32(&protectors, f);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        protectors.clear(f);
    }

    /// Falls through when `prototype` is the current Realm's
    /// %Object.prototype% still holding the rows of its layout, with its
    /// immutable null [[Prototype]]: everything the
    /// `NegativeCacheMode::ObjectPrototype` closed-tail proof establishes of
    /// a [[Prototype]]. Anything else branches to `unprotected`.
    pub(crate) fn emit_unchanged_object_prototype_or_branch(
        &mut self,
        prototype: &GcLocal<JavaScriptObject, Nullable>,
        unprotected: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let protectors = self.emit_armed_realm_protectors_or_branch(unprotected, f);
        s.field(RealmProtectorsSchema::OBJECT_PROTOTYPE)
            .read(&protectors, s, f);
        let _ = prototype.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        self.emit_object_prototype_rows_unchanged_i32(&protectors, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        protectors.clear(f);
    }

    /// Falls through when `prototype` is the current Realm's
    /// %Array.prototype% with no own index, whose [[Prototype]] is
    /// %Object.prototype% still holding the rows of its layout (which have
    /// no Array-index key) with its immutable null [[Prototype]]: the chain
    /// the packed append's prototype walk admits in its first two steps.
    /// Anything else branches to `unprotected`.
    pub(crate) fn emit_array_prototype_chain_without_indices_or_branch(
        &mut self,
        prototype: &GcLocal<JavaScriptObject, Nullable>,
        unprotected: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let protectors = self.emit_armed_realm_protectors_or_branch(unprotected, f);
        let array = s.reserve_non_null_gc_local(f).initialize(
            s.field(RealmProtectorsSchema::ARRAY_PROTOTYPE)
                .read(&protectors, s, f)
                .reference(),
            f,
        );
        let _ = array.load(s, f);
        let _ = prototype.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        s.field(ArrayObjectSchema::COUNT).read(&array, s, f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        let header = s
            .field(ArrayObjectSchema::OBJECT)
            .read(&array, s, f)
            .reference();
        s.field(OrdinaryObjectSchema::PROTOTYPE).read_top(header, f);
        s.field(RealmProtectorsSchema::OBJECT_PROTOTYPE)
            .read(&protectors, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        array.clear(f);
        self.emit_object_prototype_rows_unchanged_i32(&protectors, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        protectors.clear(f);
    }

    /// The header and rows of the protected prototype in `candidates` that
    /// `prototype` is, while that header still holds its recorded rows;
    /// any other prototype branches to `unprotected`.
    pub(crate) fn emit_protected_prototype_holder_or_branch(
        &mut self,
        candidates: &[ProtectedPrototype],
        prototype: &GcLocal<JavaScriptObject, Nullable>,
        unprotected: ControlTarget,
        f: &mut Function,
    ) -> ProtectedHolder {
        let s = self.runtime_schema();
        let protectors = self.emit_armed_realm_protectors_or_branch(unprotected, f);
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let rows = s
            .reserve_gc_local::<IntrinsicPropertyStorage, Nullable>(f)
            .initialize_null(s, f);
        let found = self.open_frame(ControlFrameKind::Block, f);
        for candidate in candidates {
            self.emit_protected_identity(&protectors, *candidate, f);
            let _ = prototype.load(s, f);
            f.instruction(&Instruction::RefEq);
            self.open_frame(ControlFrameKind::If, f);
            header.replace(
                self.emit_protected_header(&protectors, *candidate, f)
                    .nullable(),
                f,
            );
            rows.replace(self.emit_protected_rows(&protectors, *candidate, f), f);
            self.emit_branch_to_target(found, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        self.emit_branch_to_target(unprotected, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        protectors.clear(f);
        let recorded = s.reserve_non_null_gc_local(f).initialize(
            rows.non_null_or_branch(self.local_branch_label(unprotected), f),
            f,
        );
        rows.clear(f);
        let live = s
            .reserve_non_null_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        header.clear(f);
        s.field(OrdinaryObjectSchema::PROPERTIES).read(&live, s, f);
        let _ = recorded.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unprotected, f);
        ProtectedHolder {
            header: live,
            rows: recorded,
        }
    }

    /// Falls through while each prototype `rows` names is, in the current
    /// Realm, still holding the intrinsic rows recorded for it: no property
    /// of it was defined, deleted or redefined since. A Realm without an
    /// armed record, a prototype recorded without rows and an edited one
    /// branch to `unprotected`.
    pub(crate) fn emit_protected_prototype_rows_unchanged_or_branch(
        &mut self,
        rows: ProtectedPrototypeRows,
        unprotected: ControlTarget,
        f: &mut Function,
    ) {
        if rows == ProtectedPrototypeRows::default() {
            return;
        }
        let s = self.runtime_schema();
        let protectors = self.emit_armed_realm_protectors_or_branch(unprotected, f);
        for (wanted, prototype) in [
            (rows.object, ProtectedPrototype::Object),
            (rows.function, ProtectedPrototype::Function),
            (rows.array, ProtectedPrototype::Array),
        ] {
            if !wanted {
                continue;
            }
            let header = self.emit_protected_header(&protectors, prototype, f);
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read_top(header, f);
            let _ = self.emit_protected_rows(&protectors, prototype, f);
            f.instruction(&Instruction::RefEq);
            f.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(unprotected, f);
        }
        if rows.string {
            let string = s
                .field(RealmProtectorsSchema::STRING_PROTOTYPE)
                .read(&protectors, s, f)
                .reference();
            let header = s
                .field(PrimitiveBoxSchema::OBJECT)
                .read_top(string, f)
                .reference();
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read_top(header, f);
            s.field(RealmProtectorsSchema::STRING_PROTOTYPE_ROWS)
                .read(&protectors, s, f);
            f.instruction(&Instruction::RefEq);
            f.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(unprotected, f);
        }
        protectors.clear(f);
    }

    /// The ordinary header of the current Realm's %String.prototype%, the
    /// object its intrinsic table holds, without reading that table; a
    /// Realm without an armed record branches to `disarmed`.
    pub(crate) fn emit_string_prototype_header_or_branch(
        &mut self,
        disarmed: ControlTarget,
        f: &mut Function,
    ) -> GcLocal<OrdinaryObject> {
        let s = self.runtime_schema();
        let protectors = self.emit_armed_realm_protectors_or_branch(disarmed, f);
        let string = s
            .field(RealmProtectorsSchema::STRING_PROTOTYPE)
            .read(&protectors, s, f)
            .reference();
        let header = s.reserve_non_null_gc_local(f).initialize(
            s.field(PrimitiveBoxSchema::OBJECT)
                .read_top(string, f)
                .reference(),
            f,
        );
        protectors.clear(f);
        header
    }

    fn emit_protected_identity(
        &self,
        protectors: &GcLocal<RealmProtectors>,
        prototype: ProtectedPrototype,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        match prototype {
            ProtectedPrototype::Function => {
                s.field(RealmProtectorsSchema::FUNCTION_PROTOTYPE)
                    .read(protectors, s, f);
            }
            ProtectedPrototype::Object => {
                s.field(RealmProtectorsSchema::OBJECT_PROTOTYPE)
                    .read(protectors, s, f);
            }
            ProtectedPrototype::Array => {
                s.field(RealmProtectorsSchema::ARRAY_PROTOTYPE)
                    .read(protectors, s, f);
            }
        }
    }

    fn emit_protected_header(
        &self,
        protectors: &GcLocal<RealmProtectors>,
        prototype: ProtectedPrototype,
        f: &mut Function,
    ) -> crate::gc_types::GcStackReference<OrdinaryObject> {
        let s = self.runtime_schema();
        match prototype {
            ProtectedPrototype::Function => {
                let callable = s
                    .field(RealmProtectorsSchema::FUNCTION_PROTOTYPE)
                    .read(protectors, s, f)
                    .reference();
                s.field(FunctionObjectSchema::OBJECT)
                    .read_top(callable, f)
                    .reference()
            }
            ProtectedPrototype::Object => s
                .field(RealmProtectorsSchema::OBJECT_PROTOTYPE)
                .read(protectors, s, f)
                .reference(),
            ProtectedPrototype::Array => {
                let array = s
                    .field(RealmProtectorsSchema::ARRAY_PROTOTYPE)
                    .read(protectors, s, f)
                    .reference();
                s.field(ArrayObjectSchema::OBJECT)
                    .read_top(array, f)
                    .reference()
            }
        }
    }

    fn emit_protected_rows(
        &self,
        protectors: &GcLocal<RealmProtectors>,
        prototype: ProtectedPrototype,
        f: &mut Function,
    ) -> crate::gc_types::GcStackReference<IntrinsicPropertyStorage, Nullable> {
        let s = self.runtime_schema();
        match prototype {
            ProtectedPrototype::Function => s
                .field(RealmProtectorsSchema::FUNCTION_PROTOTYPE_ROWS)
                .read(protectors, s, f)
                .reference(),
            ProtectedPrototype::Object => s
                .field(RealmProtectorsSchema::OBJECT_PROTOTYPE_ROWS)
                .read(protectors, s, f)
                .reference(),
            ProtectedPrototype::Array => s
                .field(RealmProtectorsSchema::ARRAY_PROTOTYPE_ROWS)
                .read(protectors, s, f)
                .reference(),
        }
    }
}
