//! An `instanceof` site's record in P.
//!
//! Each source operator site keeps a record in a global of the module that
//! compiles it, made by R the first time the site runs, for the right-hand
//! side C it meets then. A usable record holds what stays true of C for as
//! long as two cheap facts hold: C's own "prototype" is a non-writable,
//! non-configurable data property holding an object, so its value is
//! permanent; and C's [[Prototype]] was a Realm's %Function.prototype%,
//! whose own @@hasInstance is the immutable default method, or a function
//! object, such as %TypedArray% or a parent class, whose [[Prototype]] was.
//! At each use the site re-proves that C is the record's function, that
//! those [[Prototype]] edges are unchanged and that @@hasInstance has never
//! been hashed, so that no dictionary, C's and the edge's included, holds
//! it; GetMethod(C, @@hasInstance) then finds the default method and the
//! result is OrdinaryHasInstance(C, V). The site
//! answers a primitive V and walks an ordinary, function or Array V's chain
//! inline. Everything else, a Proxy on the chain, another right-hand side, a
//! writable "prototype", a bound or non-callable C, goes on to the entry and
//! the HasInstance helper having observed nothing: every step only reads.
use super::*;
use crate::gc_types::{InstanceofSiteRecord, InstanceofSiteRecordSchema, SiteRecordGlobal};
use crate::runtime_helpers::{InstanceofRecordArguments, InstanceofRecordParameters};

impl FunctionBuilder<'_> {
    /// The site's record, from its global; the site's first use asks R to
    /// make it for C and keeps it in the global.
    fn emit_instanceof_site_record(
        &mut self,
        global: SiteRecordGlobal<InstanceofSiteRecord>,
        constructor: &ValueLocals,
        f: &mut Function,
    ) -> Result<GcLocal<InstanceofSiteRecord, Nullable>, EmitError> {
        let s = self.runtime_schema();
        f.instruction(&Instruction::Block(BlockType::Result(ValType::Ref(
            s.reference_type::<InstanceofSiteRecord>(GcNullability::NonNullable),
        ))));
        let _ = global.load(s, f);
        f.instruction(&Instruction::BrOnNonNull(0));
        let realm = self.emit_execution_realm(f);
        let made = s.helper_reference_on_stack(s.call_helper(
            InstanceofRecordArguments::new(
                constructor.reference(),
                &realm,
                self.current_environment(),
                self.current_program(),
            ),
            self.runtime_helper_base()?,
            f,
        ));
        realm.clear(f);
        global.store(made, f);
        let record = global.load(s, f).require_non_null(f);
        f.instruction(&Instruction::End);
        Ok(s.reserve_gc_local(f).initialize(record.nullable(), f))
    }

    /// Answers `V instanceof C` into `result` from the site's record when
    /// its proof holds; otherwise branches to `slow`, having observed
    /// nothing.
    pub(super) fn emit_instanceof_record_answer(
        &mut self,
        global: SiteRecordGlobal<InstanceofSiteRecord>,
        object: &ValueLocals,
        constructor: &ValueLocals,
        result: &CompletionLocals,
        slow: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let record = self.emit_instanceof_site_record(global, constructor, f)?;
        s.field(InstanceofSiteRecordSchema::CONSTRUCTOR)
            .read(&record, s, f);
        constructor.reference().load(f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(slow, f);
        // A declined record has no header.
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize(
                s.field(InstanceofSiteRecordSchema::HEADER)
                    .read(&record, s, f)
                    .reference(),
                f,
            );
        header.load(s, f).is_null(f);
        self.emit_branch_if_to_target(slow, f);
        s.field(OrdinaryObjectSchema::PROTOTYPE).read(&header, s, f);
        s.field(InstanceofSiteRecordSchema::EDGE)
            .read(&record, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(slow, f);
        header.clear(f);
        // An edge that is a function object still inherits from the
        // recorded %Function.prototype%.
        let edge = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize(
                s.field(InstanceofSiteRecordSchema::EDGE_HEADER)
                    .read(&record, s, f)
                    .reference(),
                f,
            );
        edge.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        s.field(OrdinaryObjectSchema::PROTOTYPE).read(&edge, s, f);
        s.field(InstanceofSiteRecordSchema::FUNCTION_PROTOTYPE)
            .read(&record, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(slow, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        edge.clear(f);
        self.emit_has_instance_never_hashed_or_branch(slow, f);
        let answered = self.open_frame(ControlFrameKind::Block, f);
        let answer = s.reserve_i32_local(f);
        answer.set_constant(0, f);
        // OrdinaryHasInstance step 3: a primitive V is false.
        emit_is_object_tag_i32(object.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(answered, f);
        let prototype = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize(
                s.field(InstanceofSiteRecordSchema::PROTOTYPE)
                    .read(&record, s, f)
                    .reference(),
                f,
            );
        record.clear(f);
        let next = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize_null(s, f);
        let current = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize_null(s, f);
        self.emit_chain_prototype_or_branch(
            CandidateObject::Reference(object.reference()),
            &next,
            slow,
            f,
        );
        let walk = self.open_frame(ControlFrameKind::Loop, f);
        next.load(s, f).is_null(f);
        self.emit_branch_if_to_target(answered, f);
        let _ = next.load(s, f);
        let _ = prototype.load(s, f);
        f.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, f);
        answer.set_constant(1, f);
        self.emit_branch_to_target(answered, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        current.replace(next.load(s, f), f);
        self.emit_chain_prototype_or_branch(CandidateObject::Prototype(&current), &next, slow, f);
        self.emit_branch_to_target(walk, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        current.clear(f);
        next.clear(f);
        prototype.clear(f);
        result.initialize(f);
        result.value().set_boolean(answer, f);
        s.release_i32_local(answer, f);
        Ok(())
    }

    /// Makes an `instanceof` site's record for the right-hand side C it
    /// first meets: usable when C is a function object whose [[Prototype]]
    /// is this Realm's %Function.prototype%, or a function object whose
    /// [[Prototype]] is, @@hasInstance has never been
    /// hashed, and C's own "prototype" is a non-writable, non-configurable
    /// data property holding an object; declined, with no header, for every
    /// other C. Only [[GetOwnProperty]] of a function object runs, which
    /// observes nothing.
    pub(crate) fn compile_instanceof_record_helper(&mut self) -> Result<Function, EmitError> {
        let mut body = self.begin_helper_body(RuntimeHelperId::InstanceofRecord);
        let p = self.helper_parameters::<InstanceofRecordParameters>(&mut body);
        let f = &mut body;
        let s = self.runtime_schema();
        let declined = self.open_frame(ControlFrameKind::Block, f);
        let label = self.local_branch_label(declined);
        let callable = s
            .reserve_gc_local::<FunctionObject, NonNullable>(f)
            .initialize(
                p.constructor
                    .sealed_reference_or_branch::<FunctionObject>(s, label, f),
                f,
            );
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize(
                s.field(FunctionObjectSchema::OBJECT)
                    .read(&callable, s, f)
                    .reference()
                    .nullable(),
                f,
            );
        let edge = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize(
                s.field(OrdinaryObjectSchema::PROTOTYPE)
                    .read(&header, s, f)
                    .reference(),
                f,
            );
        let function_prototype = self.emit_load_non_array_realm_intrinsic_stored(
            &p.caller_execution_realm,
            NonArrayRealmIntrinsicSlot::FunctionPrototype,
            f,
        );
        // The %Function.prototype% that C's chain reaches: C's own edge, or
        // the edge of the one function object between them.
        let reached = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize(edge.load(s, f), f);
        let edge_header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let direct = self.open_frame(ControlFrameKind::Block, f);
        let _ = edge.load(s, f);
        s.field(StoredValueSchema::REFERENCE)
            .read(&function_prototype, s, f);
        f.instruction(&Instruction::RefEq);
        self.emit_branch_if_to_target(direct, f);
        let middle = s
            .reserve_gc_local::<FunctionObject, Nullable>(f)
            .initialize(
                edge.sealed_subtype_or_branch::<FunctionObject>(s, label, f)
                    .nullable(),
                f,
            );
        edge_header.replace(
            s.field(FunctionObjectSchema::OBJECT)
                .read(&middle, s, f)
                .reference()
                .nullable(),
            f,
        );
        middle.clear(f);
        reached.replace(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&edge_header, s, f)
                .reference(),
            f,
        );
        let _ = reached.load(s, f);
        s.field(StoredValueSchema::REFERENCE)
            .read(&function_prototype, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(declined, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        function_prototype.clear(f);
        self.emit_has_instance_never_hashed_or_branch(declined, f);
        let target = s.reserve_value_local(f);
        target.set_reference(&callable, s, f);
        callable.clear(f);
        let name = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("prototype", f)?, f);
        let key = PropertyKeyLocals::from_string(s, &name, f);
        name.clear(f);
        let completion = s.reserve_completion(f);
        let descriptor = self.emit_object_get_own_property(&target, &key, &completion, f)?;
        key.clear(f);
        target.clear(f);
        completion.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(declined, f);
        completion.clear(f);
        let present = s
            .reserve_gc_local::<PropertyDescriptor, Nullable>(f)
            .initialize(descriptor.non_null_or_branch(label, f).nullable(), f);
        descriptor.clear(f);
        s.field(PropertyDescriptorSchema::FLAGS)
            .read(&present, s, f);
        f.instruction(&Instruction::I32Const(
            crate::heap::DescriptorMask::ACCESSOR
                .union(crate::heap::DescriptorMask::WRITABLE)
                .union(crate::heap::DescriptorMask::CONFIGURABLE)
                .as_i32(),
        ));
        f.instruction(&Instruction::I32And);
        self.emit_branch_if_to_target(declined, f);
        let data = s.reserve_gc_local::<PropertyValue, Nullable>(f).initialize(
            s.field(PropertyDescriptorSchema::VALUE)
                .read(&present, s, f)
                .reference()
                .nullable(),
            f,
        );
        present.clear(f);
        let stored = s.reserve_gc_local::<StoredValue, Nullable>(f).initialize(
            data.sealed_subtype_or_branch::<StoredValue>(s, label, f)
                .nullable(),
            f,
        );
        data.clear(f);
        let value = s.reserve_value_local(f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, &value, s, f);
        stored.clear(f);
        // A non-object "prototype" throws; the kernel owns that error.
        emit_is_object_tag_i32(value.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(declined, f);
        let prototype = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize(s.prototype_reference(&value, f), f);
        value.clear(f);
        let _ = s.struct_type::<InstanceofSiteRecord>().construct(
            (
                GcOperand::eq_reference(&p.constructor),
                GcOperand::reference(&header, s),
                GcOperand::reference(&edge, s),
                GcOperand::reference(&edge_header, s),
                GcOperand::reference(&reached, s),
                GcOperand::reference(&prototype, s),
            ),
            f,
        );
        f.instruction(&Instruction::Return);
        prototype.clear(f);
        reached.clear(f);
        edge_header.clear(f);
        edge.clear(f);
        header.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let _ = s.struct_type::<InstanceofSiteRecord>().construct(
            (
                GcOperand::eq_reference(&p.constructor),
                GcOperand::null(s),
                GcOperand::null(s),
                GcOperand::null(s),
                GcOperand::null(s),
                GcOperand::null(s),
            ),
            f,
        );
        self.clear_helper_execution_realm(f);
        p.release(f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(body))
    }
}
