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
//! At each use the site re-proves that C is the record's function and that
//! those [[Prototype]] edges are unchanged. A single site or an entire run
//! also proves that @@hasInstance has never been hashed, so no dictionary
//! holds it. GetMethod(C, @@hasInstance) then finds the default method and
//! the result is OrdinaryHasInstance(C, V). The site
//! answers a primitive V and walks an ordinary, function or Array V's chain
//! inline. Everything else, a Proxy on the chain, another right-hand side, a
//! writable "prototype", a bound or non-callable C, goes on to the entry and
//! the HasInstance helper having observed nothing: every step only reads.
use super::chain::{ChainWalk, RunSite};
use super::*;
use crate::environments::GlobalDeclarativeNames;
use crate::gc_types::{
    Environment, GlobalReadSiteRecord, InstanceofSiteRecord, InstanceofSiteRecordSchema,
    SiteRecordGlobal,
};
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
        // Null only until the site's first use.
        f.likely(&Instruction::BrOnNonNull(0));
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
        // A declined record names no constructor, and a right-hand side
        // without a reference is not a function either.
        s.field(InstanceofSiteRecordSchema::CONSTRUCTOR)
            .read(&record, s, f);
        constructor.reference().load(f);
        f.instruction(&Instruction::RefEq);
        constructor.reference().load(f);
        f.instruction(&Instruction::RefIsNull);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(slow, f);
        let answer = s.reserve_i32_local(f);
        self.emit_instanceof_usable_record_answer(&record, object, answer, slow, None, f);
        record.clear(f);
        result.initialize(f);
        result.value().set_boolean(answer, f);
        s.release_i32_local(answer, f);
        Ok(())
    }

    /// Answers `V instanceof G` into `answer` for a right-hand side that is
    /// a read of the global binding G, without reading G's value, or
    /// branches to `slow` having observed nothing. The site keeps the value
    /// cell the global's property held when its `instanceof` record was made
    /// for that very value. A value cell never changes, so while the global
    /// read site's record still answers the read and the property's current
    /// cell is that one, the read's value is the record's function, and the
    /// record's own proof decides the rest.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_instanceof_global_answer(
        &mut self,
        read: SiteRecordGlobal<GlobalReadSiteRecord>,
        cell: SiteRecordGlobal<StoredValue>,
        instance: SiteRecordGlobal<InstanceofSiteRecord>,
        global: &GcLocal<Environment, Nullable>,
        object: &ValueLocals,
        answer: I32Local,
        slow: ControlTarget,
        chain: Option<RunSite<'_>>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let stored = s
            .reserve_gc_local::<StoredValue, Nullable>(f)
            .initialize(cell.load(s, f), f);
        stored.load(s, f).is_null(f);
        self.emit_branch_if_to_target(slow, f);
        let record = s
            .reserve_gc_local::<GlobalReadSiteRecord, Nullable>(f)
            .initialize(read.load(s, f), f);
        record.load(s, f).is_null(f);
        self.emit_branch_if_to_target(slow, f);
        let names = if let Some(site) = chain {
            self.emit_run_shared_proof_or_branch(site, global, slow, f);
            GlobalDeclarativeNames::ProvedOther
        } else {
            GlobalDeclarativeNames::Unknown
        };
        let data = self.emit_global_read_record_data_or_branch(&record, global, names, slow, f);
        record.clear(f);
        let _ = data.load(s, f);
        let _ = stored.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(slow, f);
        stored.clear(f);
        data.clear(f);
        // The cell is kept only beside a usable record for its value.
        let record = s
            .reserve_gc_local::<InstanceofSiteRecord, Nullable>(f)
            .initialize(instance.load(s, f), f);
        self.emit_instanceof_usable_record_answer(&record, object, answer, slow, chain, f);
        record.clear(f);
    }

    /// Keeps, once, the value cell that the global read site's record now
    /// answers with, when the `instanceof` site's usable record was made for
    /// that cell's value. Every step only reads.
    pub(super) fn emit_instanceof_global_cell(
        &mut self,
        read: SiteRecordGlobal<GlobalReadSiteRecord>,
        cell: SiteRecordGlobal<StoredValue>,
        instance: SiteRecordGlobal<InstanceofSiteRecord>,
        global: &GcLocal<Environment, Nullable>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        cell.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        // Until the cell is kept, on one of the site's first uses.
        f.rarely_reached();
        let skip = self.open_frame(ControlFrameKind::Block, f);
        let label = self.local_branch_label(skip);
        let made = s
            .reserve_gc_local::<InstanceofSiteRecord, Nullable>(f)
            .initialize(instance.load(s, f), f);
        made.load(s, f).is_null(f);
        self.emit_branch_if_to_target(skip, f);
        // A declined record names no constructor.
        s.field(InstanceofSiteRecordSchema::CONSTRUCTOR)
            .read(&made, s, f);
        f.instruction(&Instruction::RefIsNull);
        self.emit_branch_if_to_target(skip, f);
        let record = s
            .reserve_gc_local::<GlobalReadSiteRecord, Nullable>(f)
            .initialize(read.load(s, f), f);
        record.load(s, f).is_null(f);
        self.emit_branch_if_to_target(skip, f);
        let data = self.emit_global_read_record_data_or_branch(
            &record,
            global,
            GlobalDeclarativeNames::Unknown,
            skip,
            f,
        );
        record.clear(f);
        let stored = s
            .reserve_gc_local::<StoredValue, NonNullable>(f)
            .initialize(data.sealed_subtype_or_branch::<StoredValue>(s, label, f), f);
        data.clear(f);
        s.field(StoredValueSchema::REFERENCE).read(&stored, s, f);
        s.field(InstanceofSiteRecordSchema::CONSTRUCTOR)
            .read(&made, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(skip, f);
        made.clear(f);
        cell.store(stored.load(s, f), f);
        stored.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    /// Answers `V instanceof C` into `answer` from a usable record of C, or
    /// branches to `slow`, having observed nothing. A site of an
    /// `instanceof` run walks V's chain into the run's slots, or answers from
    /// the slots an earlier site walked for this very V.
    fn emit_instanceof_usable_record_answer(
        &mut self,
        record: &GcLocal<InstanceofSiteRecord, Nullable>,
        object: &ValueLocals,
        answer: I32Local,
        slow: ControlTarget,
        chain: Option<RunSite<'_>>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        // Each constructor must still reach its recorded %Function.prototype%
        // through the recorded edges. A run site already proved that no
        // dictionary holds @@hasInstance.
        let header = s
            .field(InstanceofSiteRecordSchema::HEADER)
            .read(record, s, f)
            .reference();
        s.field(OrdinaryObjectSchema::PROTOTYPE).read_top(header, f);
        s.field(InstanceofSiteRecordSchema::EDGE).read(record, s, f);
        f.instruction(&Instruction::RefEq);
        let edge = s
            .field(InstanceofSiteRecordSchema::EDGE_HEADER)
            .read(record, s, f)
            .reference();
        s.field(OrdinaryObjectSchema::PROTOTYPE).read_top(edge, f);
        s.field(InstanceofSiteRecordSchema::FUNCTION_PROTOTYPE)
            .read(record, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32And);
        if chain.is_none() {
            // Until the Symbol has a hash identity, no dictionary can hold
            // it as a key.
            let symbol = s
                .field(InstanceofSiteRecordSchema::HAS_INSTANCE)
                .read(record, s, f)
                .reference();
            s.field(SymbolValueSchema::KEY_HASH_ID).read_top(symbol, f);
            f.instruction(&Instruction::I64Eqz);
            f.instruction(&Instruction::I32And);
        }
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(slow, f);
        let answered = self.open_frame(ControlFrameKind::Block, f);
        answer.set_constant(0, f);
        // OrdinaryHasInstance step 3: a primitive V is false.
        emit_is_object_tag_i32(object.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(answered, f);
        let prototype = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize(
                s.field(InstanceofSiteRecordSchema::PROTOTYPE)
                    .read(record, s, f)
                    .reference(),
                f,
            );
        let next = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize_null(s, f);
        let current = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize_null(s, f);
        // Leaves with the next object of V's chain to examine in `next`.
        match chain {
            None => self.emit_chain_prototype_or_branch(
                CandidateObject::Reference(object.reference()),
                &next,
                slow,
                f,
            ),
            Some(site) => self.emit_run_chain_candidates(
                site,
                ChainWalk {
                    object,
                    prototype: &prototype,
                    record,
                    next: &next,
                    current: &current,
                    answer,
                    answered,
                    slow,
                },
                f,
            ),
        }
        // A completed shared walk has already answered. Only a remaining
        // walk needs this site's terminal proof.
        // The Realm's %Object.prototype%, when the record proved it: its
        // [[Prototype]] is null and immutable, so a chain reaching it ends.
        let terminal = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize(
                s.field(InstanceofSiteRecordSchema::TERMINAL)
                    .read(record, s, f)
                    .reference(),
                f,
            );
        let walk = self.open_frame(ControlFrameKind::Loop, f);
        let _ = next.load(s, f);
        let _ = prototype.load(s, f);
        f.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, f);
        answer.set_constant(1, f);
        self.emit_branch_to_target(answered, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        emit_chain_ends_i32(&next, &terminal, s, f);
        self.emit_branch_if_to_target(answered, f);
        current.replace(next.load(s, f), f);
        self.emit_chain_prototype_or_branch(CandidateObject::Prototype(&current), &next, slow, f);
        self.emit_branch_to_target(walk, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        current.clear(f);
        next.clear(f);
        terminal.clear(f);
        prototype.clear(f);
    }

    /// Makes an `instanceof` site's record for the right-hand side C it
    /// first meets: usable when C is a function object whose [[Prototype]]
    /// is this Realm's %Function.prototype%, or a function object whose
    /// [[Prototype]] is, @@hasInstance has never been
    /// hashed, and C's own "prototype" is a non-writable, non-configurable
    /// data property holding an object; declined, naming no constructor,
    /// for every other C. Only [[GetOwnProperty]] of a function object runs, which
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
        // A direct edge has no function object between: the record then
        // repeats C's own header.
        let edge_header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize(header.load(s, f), f);
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
        let symbol = s.reserve_gc_local::<SymbolValue, Nullable>(f).initialize(
            self.emit_well_known_symbol_reference(scriptw_ir::WellKnownSymbol::HasInstance, f)?
                .nullable(),
            f,
        );
        let terminal = self.emit_immutable_null_prototype_terminal(&p.caller_execution_realm, f);
        let _ = s.struct_type::<InstanceofSiteRecord>().construct(
            (
                GcOperand::eq_reference(&p.constructor),
                GcOperand::reference(&header, s),
                GcOperand::reference(&edge, s),
                GcOperand::reference(&edge_header, s),
                GcOperand::reference(&reached, s),
                GcOperand::reference(&prototype, s),
                GcOperand::reference(&symbol, s),
                GcOperand::reference(&terminal, s),
            ),
            f,
        );
        f.instruction(&Instruction::Return);
        terminal.clear(f);
        symbol.clear(f);
        prototype.clear(f);
        reached.clear(f);
        edge_header.clear(f);
        edge.clear(f);
        header.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let _ = s.struct_type::<InstanceofSiteRecord>().construct(
            (
                GcOperand::null_eq(),
                GcOperand::null(s),
                GcOperand::null(s),
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

    /// The Realm's %Object.prototype% as a record's TERMINAL, after reading
    /// it as an ordinary object whose [[Prototype]] is null and immutable;
    /// null otherwise. IMMUTABLE_PROTOTYPE never changes, and
    /// [[SetPrototypeOf]] of such an object (SetImmutablePrototype) writes
    /// nothing, so that [[Prototype]] stays null for good.
    fn emit_immutable_null_prototype_terminal(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        f: &mut Function,
    ) -> GcLocal<JavaScriptObject, Nullable> {
        let s = self.runtime_schema();
        let terminal = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize_null(s, f);
        let unproved = self.open_frame(ControlFrameKind::Block, f);
        let stored = self.emit_load_non_array_realm_intrinsic_stored(
            realm,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            f,
        );
        let value = s.reserve_value_local(f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, &value, s, f);
        stored.clear(f);
        let object = s
            .reserve_gc_local::<OrdinaryObject, NonNullable>(f)
            .initialize(
                value.sealed_reference_or_branch::<OrdinaryObject>(
                    s,
                    self.local_branch_label(unproved),
                    f,
                ),
                f,
            );
        s.field(OrdinaryObjectSchema::PROTOTYPE)
            .read(&object, s, f)
            .reference()
            .is_null(f);
        s.field(OrdinaryObjectSchema::IMMUTABLE_PROTOTYPE)
            .read(&object, s, f);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        object.clear(f);
        self.emit_branch_if_to_target(unproved, f);
        terminal.replace(s.prototype_reference(&value, f), f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.clear(f);
        terminal
    }
}
