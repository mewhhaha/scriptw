use super::*;
use crate::emit::ControlTarget;
use crate::functions::NonArrayRealmIntrinsicSlot;
use crate::gc_types::{
    ArrayObject, ArrayObjectSchema, BoundFunction, BoundFunctionSchema, CompactDataStorage,
    CompletionLocals, ExecutableCodeKind, ExecutableCodeSchema, FunctionContextSchema,
    EqRefLocal, GcI32Constant, GcNullability, GcOperand, GcStackReference, GcSubtype,
    IntrinsicPropertyStorage, IntrinsicPropertyStorageSchema, IntrinsicPropertyValueCache,
    JavaScriptObject, JavaScriptReference, NonNullable, Nullable, OrdinaryObject,
    OrdinaryObjectSchema, OrdinaryPropertyStorage, OrdinaryPropertyStorageSchema,
    ObjectPropertyStorage, OwnDataReadCache, OwnDataReadCacheSchema, OwnDataReadCacheTable,
    PropertyDescriptor, PropertyDescriptorSchema, PropertyEntry, PropertyEntrySchema,
    PropertyTable, PropertyValue, RealmRecord, StoredValue, StoredValueSchema, SymbolValue,
    SymbolValueSchema,
};
use crate::runtime_helpers::{
    HasInstanceArguments, HasInstanceParameters, HelperParameters, InstanceofEntryArguments,
    InstanceofEntryParameters, ObjectGetPrototypeOfArguments, RuntimeHelperId,
};

mod site;

/// The site word of a request that has no operator site: the native method
/// and operators compiled outside source code. Site ids are never negative.
const NO_INSTANCEOF_SITE: i32 = -1;

/// GetMethod(C, @@hasInstance) is proved to reach the default method only
/// through at most this many function objects, C included.
const MAX_DEFAULT_HAS_INSTANCE_HOPS: i32 = 3;

/// These two entry algorithms have different hook and non-callable policies.
/// Borrowed whole values preserve the specification roles without raw slots.
enum HasInstanceRequest<'value> {
    InstanceofOperator {
        object: &'value ValueLocals,
        constructor: &'value ValueLocals,
        site: Option<crate::emit::ProgramSiteId>,
    },
    OrdinaryHasInstance {
        constructor: &'value ValueLocals,
        object: &'value ValueLocals,
    },
}

enum HasInstanceRuntimeState {
    InstanceofOperator,
    OrdinaryHasInstance,
}
impl HasInstanceRuntimeState {
    const fn runtime_code(&self) -> i32 {
        match self {
            Self::InstanceofOperator => 0,
            Self::OrdinaryHasInstance => 1,
        }
    }
}

/// An object on V's prototype chain, examined by its layout alone: V itself
/// as a value, or a [[Prototype]] edge.
#[derive(Clone, Copy)]
enum ChainObject<'a> {
    Value(&'a ValueLocals),
    Prototype(&'a GcLocal<JavaScriptObject, Nullable>),
}

impl ChainObject<'_> {
    fn has_layout<T: JavaScriptReference>(self, schema: &RuntimeSchema, f: &mut Function) {
        match self {
            Self::Value(value) => {
                value.reference().load(f);
                f.instruction(&Instruction::RefTestNonNull(
                    schema
                        .reference_type::<T>(GcNullability::NonNullable)
                        .heap_type,
                ));
            }
            Self::Prototype(reference) => reference.has_layout::<T>(schema, f),
        }
    }

    fn cast_layout<T: JavaScriptReference>(
        self,
        schema: &RuntimeSchema,
        f: &mut Function,
    ) -> GcStackReference<T> {
        match self {
            Self::Value(value) => value.cast_reference::<T>(schema, f),
            Self::Prototype(reference) => reference.cast_layout::<T>(schema, f),
        }
    }
}

/// An object on V's prototype chain as the entry sees it: V's reference
/// word, or a [[Prototype]] edge.
#[derive(Clone, Copy)]
enum CandidateObject<'a> {
    Reference(&'a EqRefLocal),
    Prototype(&'a GcLocal<JavaScriptObject, Nullable>),
}

impl CandidateObject<'_> {
    /// The object as the sealed layout `T`, after branching to `other` for
    /// every other layout and for null: one inline header test.
    fn sealed_layout_or_branch<T: JavaScriptReference + GcSubtype<JavaScriptObject>>(
        self,
        schema: &RuntimeSchema,
        other: crate::LabelDepth,
        f: &mut Function,
    ) -> GcStackReference<T> {
        match self {
            Self::Reference(reference) => reference.sealed_reference_or_branch::<T>(schema, other, f),
            Self::Prototype(reference) => reference.sealed_subtype_or_branch::<T>(schema, other, f),
        }
    }
}

/// The entry's answer word: the operator's Boolean, or no answer, when the
/// site goes on to the complete helper.
#[derive(Clone, Copy)]
enum EntryAnswer {
    False,
    True,
    Undecided,
}

impl EntryAnswer {
    const fn code(self) -> i32 {
        match self {
            Self::False => 0,
            Self::True => 1,
            Self::Undecided => 2,
        }
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_instanceof_i32(
        &mut self,
        left: &TypedExpr,
        right: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let object = schema.reserve_value_local(function);
        let constructor = schema.reserve_value_local(function);
        let result = schema.reserve_completion(function);
        self.compile_expr_to_value(left, &object, function)?;
        self.compile_expr_to_value(right, &constructor, function)?;
        self.emit_instanceof_operator_from_locals(&object, &constructor, &result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&result, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.value().scalar().load(function);
        function.instruction(&Instruction::I32WrapI64);
        result.clear(function);
        constructor.clear(function);
        object.clear(function);
        Ok(())
    }

    pub(crate) fn emit_instanceof_operator_from_locals(
        &mut self,
        object: &ValueLocals,
        constructor: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        // A source operator site keeps a record of where its constructor's
        // own "prototype" lives; operators outside source code have none.
        let site = if self.has_source_execution_environment() {
            Some(self.next_program_site(crate::emit::ProgramSiteKind::InstanceofPrototype)?)
        } else {
            None
        };
        self.emit_has_instance_request(
            HasInstanceRequest::InstanceofOperator {
                object,
                constructor,
                site,
            },
            result,
            function,
        )
    }

    pub(crate) fn emit_ordinary_has_instance_from_locals(
        &mut self,
        constructor: &ValueLocals,
        object: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_has_instance_request(
            HasInstanceRequest::OrdinaryHasInstance {
                constructor,
                object,
            },
            result,
            function,
        )
    }

    /// Abstract-operation errors belong to the executing context. A Script
    /// operator has no callable; the native method retains its actual callee
    /// Realm through the common error owner's FunctionContext projection.
    fn emit_has_instance_request(
        &mut self,
        request: HasInstanceRequest<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let (state, constructor, object) = match request {
            // A source operator answers from its own record first, then asks
            // its call-free entry, and calls the complete helper only when
            // the entry has no answer.
            HasInstanceRequest::InstanceofOperator {
                object,
                constructor,
                site: Some(site),
            } => {
                let global = self.next_site_record::<crate::gc_types::InstanceofSiteRecord>()?;
                let done = self.open_frame(ControlFrameKind::Block, function);
                let slow = self.open_frame(ControlFrameKind::Block, function);
                self.emit_instanceof_record_answer(
                    global,
                    object,
                    constructor,
                    result,
                    slow,
                    function,
                )?;
                self.emit_branch_to_target(done, function);
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
                let site_id = schema.reserve_i32_local(function);
                site.emit(site_id, function);
                let realm = self.emit_execution_realm(function);
                let answer = schema.reserve_i32_local(function);
                schema
                    .call_helper(
                        InstanceofEntryArguments::new(
                            object.tag(),
                            object.reference(),
                            constructor.reference(),
                            site_id,
                            &realm,
                            self.current_program(),
                        ),
                        self.runtime_helper_base()?,
                        function,
                    )
                    .store(answer, function);
                answer.load(function);
                function.instruction(&Instruction::I32Const(EntryAnswer::Undecided.code()));
                function.instruction(&Instruction::I32Eq);
                function.instruction(&Instruction::If(BlockType::Empty));
                let entry = schema.reserve_i32_local(function);
                entry.set_constant(
                    HasInstanceRuntimeState::InstanceofOperator.runtime_code(),
                    function,
                );
                schema
                    .call_helper(
                        HasInstanceArguments::new(
                            entry,
                            object,
                            constructor,
                            site_id,
                            &realm,
                            self.current_environment(),
                            self.current_program(),
                        ),
                        self.runtime_helper_base()?,
                        function,
                    )
                    .store(result, function);
                schema.release_i32_local(entry, function);
                function.instruction(&Instruction::Else);
                result.initialize(function);
                result.value().set_boolean(answer, function);
                function.instruction(&Instruction::End);
                schema.release_i32_local(answer, function);
                realm.clear(function);
                schema.release_i32_local(site_id, function);
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
                return Ok(());
            }
            HasInstanceRequest::InstanceofOperator {
                object,
                constructor,
                site: None,
            } => (
                HasInstanceRuntimeState::InstanceofOperator,
                constructor,
                object,
            ),
            HasInstanceRequest::OrdinaryHasInstance {
                constructor,
                object,
            } => (
                HasInstanceRuntimeState::OrdinaryHasInstance,
                constructor,
                object,
            ),
        };
        let entry = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(state.runtime_code()));
        entry.store(function);
        let site_id = schema.reserve_i32_local(function);
        site_id.set_constant(NO_INSTANCEOF_SITE, function);
        let realm = self.emit_execution_realm(function);
        schema
            .call_helper(
                HasInstanceArguments::new(
                    entry,
                    object,
                    constructor,
                    site_id,
                    &realm,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        realm.clear(function);
        schema.release_i32_local(site_id, function);
        schema.release_i32_local(entry, function);
        Ok(())
    }

    /// The operator's entry at a source site: the lean front's common case
    /// answered from the site's record, with no call, write or allocation.
    /// Every step only reads, so declining at any of them leaves nothing
    /// observed: the entry then answers Undecided and the site calls
    /// HasInstance, whose front records the site and whose state machine
    /// owns the complete algorithm. The entry takes only the words it
    /// reads, and returning instead of tail-calling leaves nothing to keep
    /// for the kernel.
    pub(crate) fn compile_instanceof_entry_helper(&mut self) -> Result<Function, EmitError> {
        let mut body = self.begin_helper_body(RuntimeHelperId::InstanceofEntry);
        let p = self.helper_parameters::<InstanceofEntryParameters>(&mut body);
        let f = &mut body;
        let miss = self.open_frame(ControlFrameKind::Block, f);
        self.emit_instanceof_entry_answer(
            p.object_tag,
            &p.object,
            &p.constructor,
            p.site,
            &p.caller_execution_realm,
            miss,
            f,
        )?;
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.clear_helper_execution_realm(f);
        p.release(f);
        f.instruction(&Instruction::I32Const(EntryAnswer::Undecided.code()));
        f.instruction(&Instruction::End);
        Ok(self.finish_function(body))
    }

    /// Returns the operator's result when the lean front's proof holds and
    /// the site's record locates C's "prototype"; otherwise branches to
    /// `miss`. C is a function object whose chain reaches this Realm's
    /// %Function.prototype% through function objects without an own
    /// @@hasInstance, so GetMethod finds that prototype's immutable default
    /// method and the result is OrdinaryHasInstance(C, V), in this Realm.
    /// A primitive V is false without the "prototype" Get, which the entry
    /// may still have read: every read here is unobservable. An object V's
    /// chain is walked through ordinary, function and Array headers, whose
    /// [[GetPrototypeOf]] observes nothing.
    ///
    /// Locals of proved non-null references stay nullable: reading a field
    /// checks for null once, where a non-null local's load checks again.
    #[allow(clippy::too_many_arguments)]
    fn emit_instanceof_entry_answer(
        &mut self,
        object_tag: crate::gc_types::I32Local,
        object: &EqRefLocal,
        constructor: &EqRefLocal,
        site: crate::gc_types::I32Local,
        realm: &GcLocal<RealmRecord>,
        miss: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let function_prototype_layout =
            crate::intrinsics::IntrinsicPropertyLayoutId::FunctionPrototype;
        if !self
            .strings
            .has_intrinsic_property_layout(function_prototype_layout)?
        {
            return Ok(());
        }
        let function_prototype_address = self
            .strings
            .intrinsic_property_layout_span(function_prototype_layout)?
            .address();
        let callable = s
            .reserve_gc_local::<FunctionObject, Nullable>(f)
            .initialize(
                constructor
                    .sealed_reference_or_branch::<FunctionObject>(
                        s,
                        self.local_branch_label(miss),
                        f,
                    )
                    .nullable(),
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
        callable.clear(f);
        let prototype = s.reserve_value_local(f);
        self.emit_own_prototype_or_branch(
            &header,
            site,
            function_prototype_address,
            &prototype,
            miss,
            f,
        );
        self.emit_inherited_default_has_instance_or_branch(
            &header,
            realm,
            function_prototype_address,
            miss,
            f,
        );
        header.clear(f);
        // OrdinaryHasInstance step 3: a primitive V is false whatever
        // "prototype" holds.
        emit_is_object_tag_i32(object_tag, f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        emit_return_answer(EntryAnswer::False, f);
        f.instruction(&Instruction::End);
        // A non-object "prototype" throws; the kernel owns that error.
        emit_is_object_tag_i32(prototype.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, f);
        let next = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize_null(s, f);
        let current = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize_null(s, f);
        self.emit_chain_prototype_or_branch(CandidateObject::Reference(object), &next, miss, f);
        let walk = self.open_frame(ControlFrameKind::Loop, f);
        next.load(s, f).is_null(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        emit_return_answer(EntryAnswer::False, f);
        f.instruction(&Instruction::End);
        let _ = next.load(s, f);
        prototype.reference().load(f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::If(BlockType::Empty));
        emit_return_answer(EntryAnswer::True, f);
        f.instruction(&Instruction::End);
        current.replace(next.load(s, f), f);
        self.emit_chain_prototype_or_branch(CandidateObject::Prototype(&current), &next, miss, f);
        self.emit_branch_to_target(walk, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        current.clear(f);
        next.clear(f);
        prototype.clear(f);
        Ok(())
    }

    /// C's own storage, read once for two facts: it holds no @@hasInstance,
    /// and its "prototype" data property is where the site's record says, as
    /// the front's record read finds it. A dictionary entry at the recorded
    /// position holds the record's key String itself; an intrinsic layout at
    /// the recorded address other than %Function.prototype%'s is immutable
    /// and keeps the row's materialized value in the recorded group. The
    /// record table is the program's own and its site index is this
    /// operator's alone, so the record was made by this site's front, for
    /// "prototype". A site without its record, a dictionary that might hold
    /// @@hasInstance, a value still held as a recipe, an accessor and every
    /// other storage branch to `miss`.
    fn emit_own_prototype_or_branch(
        &mut self,
        header: &GcLocal<OrdinaryObject, Nullable>,
        site: crate::gc_types::I32Local,
        function_prototype_address: u32,
        prototype: &ValueLocals,
        miss: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let label = self.local_branch_label(miss);
        let records = self.current_program().own_data_records(s, f);
        let table = s
            .reserve_gc_local::<OwnDataReadCacheTable, Nullable>(f)
            .initialize(records.non_null_or_branch(label, f).nullable(), f);
        records.clear(f);
        site.load(f);
        s.array_type::<OwnDataReadCacheTable>().length(&table, s, f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(miss, f);
        let slot = s.reserve_gc_local(f).initialize(
            s.array_type::<OwnDataReadCacheTable>()
                .read(&table, site, s, f)
                .reference(),
            f,
        );
        table.clear(f);
        let record = s
            .reserve_gc_local::<OwnDataReadCache, Nullable>(f)
            .initialize(slot.non_null_or_branch(label, f).nullable(), f);
        slot.clear(f);
        let storage = s
            .reserve_gc_local::<ObjectPropertyStorage, Nullable>(f)
            .initialize(
                s.field(OrdinaryObjectSchema::PROPERTIES)
                    .read(header, s, f)
                    .reference()
                    .nullable(),
                f,
            );
        let read = self.open_frame(ControlFrameKind::Block, f);
        let other = self.open_frame(ControlFrameKind::Block, f);
        let dictionary = s
            .reserve_gc_local::<OrdinaryPropertyStorage, Nullable>(f)
            .initialize(
                storage
                    .sealed_subtype_or_branch::<OrdinaryPropertyStorage>(
                        s,
                        self.local_branch_label(other),
                        f,
                    )
                    .nullable(),
                f,
            );
        self.emit_has_instance_never_hashed_or_branch(miss, f);
        // A dictionary entry keeps its position while it lives; a deleted
        // entry is a null slot.
        let position = s.reserve_i32_local(f);
        s.field(OwnDataReadCacheSchema::DICTIONARY_POSITION)
            .read(&record, s, f)
            .store(position, f);
        position.load(f);
        s.field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&dictionary, s, f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(miss, f);
        let entries = s
            .reserve_gc_local::<PropertyTable, Nullable>(f)
            .initialize(
                s.field(OrdinaryPropertyStorageSchema::ENTRIES)
                    .read(&dictionary, s, f)
                    .reference()
                    .nullable(),
                f,
            );
        dictionary.clear(f);
        position.load(f);
        s.array_type::<PropertyTable>().length(&entries, s, f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(miss, f);
        let slot = s.reserve_gc_local(f).initialize(
            s.array_type::<PropertyTable>()
                .read(&entries, position, s, f)
                .reference(),
            f,
        );
        entries.clear(f);
        s.release_i32_local(position, f);
        let entry = s
            .reserve_gc_local::<PropertyEntry, Nullable>(f)
            .initialize(slot.non_null_or_branch(label, f).nullable(), f);
        slot.clear(f);
        let stored_key = s
            .reserve_gc_local::<StoredValue, Nullable>(f)
            .initialize(
                s.field(PropertyEntrySchema::KEY)
                    .read(&entry, s, f)
                    .reference()
                    .nullable(),
                f,
            );
        s.field(StoredValueSchema::REFERENCE)
            .read(&stored_key, s, f);
        stored_key.clear(f);
        s.field(OwnDataReadCacheSchema::KEY).read(&record, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, f);
        let descriptor = s
            .reserve_gc_local::<PropertyDescriptor, Nullable>(f)
            .initialize(
                s.field(PropertyEntrySchema::DESCRIPTOR)
                    .read(&entry, s, f)
                    .reference()
                    .nullable(),
                f,
            );
        entry.clear(f);
        s.field(PropertyDescriptorSchema::FLAGS)
            .read(&descriptor, s, f);
        f.instruction(&Instruction::I32Const(
            crate::heap::DescriptorMask::ACCESSOR.as_i32(),
        ));
        f.instruction(&Instruction::I32And);
        self.emit_branch_if_to_target(miss, f);
        let data = s
            .reserve_gc_local::<PropertyValue, Nullable>(f)
            .initialize(
                s.field(PropertyDescriptorSchema::VALUE)
                    .read(&descriptor, s, f)
                    .reference()
                    .nullable(),
                f,
            );
        descriptor.clear(f);
        let stored = s.reserve_gc_local::<StoredValue, Nullable>(f).initialize(
            data.sealed_subtype_or_branch::<StoredValue>(s, label, f)
                .nullable(),
            f,
        );
        data.clear(f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, prototype, s, f);
        stored.clear(f);
        self.emit_branch_to_target(read, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let intrinsic = s
            .reserve_gc_local::<IntrinsicPropertyStorage, Nullable>(f)
            .initialize(
                storage
                    .sealed_subtype_or_branch::<IntrinsicPropertyStorage>(s, label, f)
                    .nullable(),
                f,
            );
        let address = s.reserve_i32_local(f);
        s.field(IntrinsicPropertyStorageSchema::ADDRESS)
            .read(&intrinsic, s, f)
            .store(address, f);
        address.load(f);
        f.instruction(&Instruction::I32Const(function_prototype_address as i32));
        f.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(miss, f);
        address.load(f);
        s.field(OwnDataReadCacheSchema::LAYOUT).read(&record, s, f);
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(miss, f);
        s.release_i32_local(address, f);
        let cache = s
            .reserve_gc_local::<IntrinsicPropertyValueCache, Nullable>(f)
            .initialize(
                s.field(IntrinsicPropertyStorageSchema::CACHE)
                    .read(&intrinsic, s, f)
                    .reference(),
                f,
            );
        intrinsic.clear(f);
        let groups = s
            .reserve_gc_local::<IntrinsicPropertyValueCache, Nullable>(f)
            .initialize(cache.non_null_or_branch(label, f).nullable(), f);
        cache.clear(f);
        let group = s.reserve_i32_local(f);
        s.field(OwnDataReadCacheSchema::GROUP)
            .read(&record, s, f)
            .store(group, f);
        group.load(f);
        s.array_type::<IntrinsicPropertyValueCache>()
            .length(&groups, s, f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(miss, f);
        let value = s
            .reserve_gc_local::<PropertyValue, Nullable>(f)
            .initialize(
                s.array_type::<IntrinsicPropertyValueCache>()
                    .read(&groups, group, s, f)
                    .reference(),
                f,
            );
        groups.clear(f);
        s.release_i32_local(group, f);
        let stored = s.reserve_gc_local::<StoredValue, Nullable>(f).initialize(
            value
                .sealed_subtype_or_branch::<StoredValue>(s, label, f)
                .nullable(),
            f,
        );
        value.clear(f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, prototype, s, f);
        stored.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        storage.clear(f);
        record.clear(f);
    }

    /// Branches to `miss` unless GetMethod(C, @@hasInstance) reaches this
    /// Realm's %Function.prototype% from C's [[Prototype]] through function
    /// objects without an own @@hasInstance, C and at most two more: the
    /// kernel's bound. C's own storage was proved separately.
    fn emit_inherited_default_has_instance_or_branch(
        &mut self,
        header: &GcLocal<OrdinaryObject, Nullable>,
        realm: &GcLocal<RealmRecord>,
        function_prototype_address: u32,
        miss: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let function_prototype = self.emit_load_non_array_realm_intrinsic_stored(
            realm,
            NonArrayRealmIntrinsicSlot::FunctionPrototype,
            f,
        );
        let target = s.reserve_value_local(f);
        s.field(StoredValueSchema::REFERENCE)
            .read(&function_prototype, s, f)
            .store_eq(target.reference(), f);
        function_prototype.clear(f);
        let edge = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize(
                s.field(OrdinaryObjectSchema::PROTOTYPE)
                    .read(header, s, f)
                    .reference(),
                f,
            );
        let holder = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let hops = s.reserve_i32_local(f);
        hops.set_constant(0, f);
        let inherits = self.open_frame(ControlFrameKind::Block, f);
        let hop = self.open_frame(ControlFrameKind::Loop, f);
        let _ = edge.load(s, f);
        target.reference().load(f);
        f.instruction(&Instruction::RefEq);
        self.emit_branch_if_to_target(inherits, f);
        hops.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        hops.store(f);
        hops.load(f);
        f.instruction(&Instruction::I32Const(MAX_DEFAULT_HAS_INSTANCE_HOPS));
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(miss, f);
        let callable = s
            .reserve_gc_local::<FunctionObject, Nullable>(f)
            .initialize(
                edge.sealed_subtype_or_branch::<FunctionObject>(
                    s,
                    self.local_branch_label(miss),
                    f,
                )
                .nullable(),
                f,
            );
        holder.replace(
            s.field(FunctionObjectSchema::OBJECT)
                .read(&callable, s, f)
                .reference()
                .nullable(),
            f,
        );
        callable.clear(f);
        self.emit_lacks_own_has_instance_or_branch(&holder, function_prototype_address, miss, f);
        edge.replace(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&holder, s, f)
                .reference(),
            f,
        );
        self.emit_branch_to_target(hop, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(hops, f);
        holder.clear(f);
        edge.clear(f);
        target.clear(f);
    }

    /// Branches to `miss` unless a function header provably lacks an own
    /// @@hasInstance without a call: a compact shape holds String keys only,
    /// an intrinsic layout other than %Function.prototype%'s has no such
    /// row, and a dictionary cannot hold the Symbol while it has never been
    /// hashed. A dictionary that might hold it is left to the front's index
    /// probe.
    fn emit_lacks_own_has_instance_or_branch(
        &mut self,
        header: &GcLocal<OrdinaryObject, Nullable>,
        function_prototype_address: u32,
        miss: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let storage = s
            .reserve_gc_local::<ObjectPropertyStorage, Nullable>(f)
            .initialize(
                s.field(OrdinaryObjectSchema::PROPERTIES)
                    .read(header, s, f)
                    .reference()
                    .nullable(),
                f,
            );
        let lacks = self.open_frame(ControlFrameKind::Block, f);
        let other = self.open_frame(ControlFrameKind::Block, f);
        let intrinsic = s
            .reserve_gc_local::<IntrinsicPropertyStorage, Nullable>(f)
            .initialize(
                storage
                    .sealed_subtype_or_branch::<IntrinsicPropertyStorage>(
                        s,
                        self.local_branch_label(other),
                        f,
                    )
                    .nullable(),
                f,
            );
        s.field(IntrinsicPropertyStorageSchema::ADDRESS)
            .read(&intrinsic, s, f);
        intrinsic.clear(f);
        f.instruction(&Instruction::I32Const(function_prototype_address as i32));
        f.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(miss, f);
        self.emit_branch_to_target(lacks, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        storage.is_subtype::<OrdinaryPropertyStorage>(s, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_has_instance_never_hashed_or_branch(miss, f);
        self.emit_branch_to_target(lacks, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        storage.is_subtype::<CompactDataStorage>(s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        storage.clear(f);
    }

    /// Branches to `miss` once @@hasInstance has been hashed. Every
    /// dictionary entry stores its key's hash, and a Symbol is given its
    /// hash identity the first time it is hashed, so until then no
    /// dictionary holds it.
    fn emit_has_instance_never_hashed_or_branch(&mut self, miss: ControlTarget, f: &mut Function) {
        let s = self.runtime_schema();
        let symbol = s.reserve_gc_local::<SymbolValue, Nullable>(f).initialize(
            s.well_known_symbol_reference(scriptw_ir::WellKnownSymbol::HasInstance, f)
                .nullable(),
            f,
        );
        s.field(SymbolValueSchema::KEY_HASH_ID).read(&symbol, s, f);
        symbol.clear(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, f);
    }

    /// One [[GetPrototypeOf]] step of an ordinary object, a function or an
    /// Array, none of which observes anything: the edge goes to `next`.
    /// Every other object, a Proxy included, branches to `miss`.
    fn emit_chain_prototype_or_branch(
        &mut self,
        subject: CandidateObject<'_>,
        next: &GcLocal<JavaScriptObject, Nullable>,
        miss: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let done = self.open_frame(ControlFrameKind::Block, f);
        let other = self.open_frame(ControlFrameKind::Block, f);
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize(
                subject
                    .sealed_layout_or_branch::<OrdinaryObject>(s, self.local_branch_label(other), f)
                    .nullable(),
                f,
            );
        next.replace(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, s, f)
                .reference(),
            f,
        );
        header.clear(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let other = self.open_frame(ControlFrameKind::Block, f);
        let callable = s
            .reserve_gc_local::<FunctionObject, Nullable>(f)
            .initialize(
                subject
                    .sealed_layout_or_branch::<FunctionObject>(s, self.local_branch_label(other), f)
                    .nullable(),
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
        callable.clear(f);
        next.replace(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, s, f)
                .reference(),
            f,
        );
        header.clear(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let array = s
            .reserve_gc_local::<ArrayObject, Nullable>(f)
            .initialize(
                subject
                    .sealed_layout_or_branch::<ArrayObject>(s, self.local_branch_label(miss), f)
                    .nullable(),
                f,
            );
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize(
                s.field(ArrayObjectSchema::OBJECT)
                    .read(&array, s, f)
                    .reference()
                    .nullable(),
                f,
            );
        array.clear(f);
        next.replace(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, s, f)
                .reference(),
            f,
        );
        header.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
    }

    pub(crate) fn compile_has_instance_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::HasInstance);
        let parameters = self.helper_parameters::<HasInstanceParameters>(&mut function);
        self.completion().initialize(&mut function);
        let schema = self.runtime_schema();
        let result = schema.reserve_completion(&mut function);
        let handled = schema.reserve_i32_local(&mut function);
        let boolean = schema.reserve_i32_local(&mut function);
        // Only the operator passes a site, so the front decides the
        // operator's result; anything it cannot prove goes to the kernel.
        self.emit_has_instance_lean_front(
            parameters.site,
            &parameters.object,
            &parameters.constructor,
            boolean,
            handled,
            &mut function,
        )?;
        handled.load(&mut function);
        self.open_frame(ControlFrameKind::If, &mut function);
        result.initialize(&mut function);
        result.value().set_boolean(boolean, &mut function);
        function.instruction(&Instruction::Else);
        self.emit_has_instance_kernel(
            parameters.entry,
            &parameters.object,
            &parameters.constructor,
            &result,
            &mut function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(boolean, &mut function);
        schema.release_i32_local(handled, &mut function);
        result.emit(&mut function);
        result.clear(&mut function);
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// The operator's common case, decided without the state machine. C is a
    /// function object (not bound, not a Proxy) whose chain reaches this
    /// Realm's %Function.prototype% through function objects, C included,
    /// none with an own @@hasInstance, so GetMethod finds that prototype's
    /// immutable default method and the result is OrdinaryHasInstance(C, V)
    /// in this Realm. C's own "prototype"
    /// is a data property holding an object, read through the site's record;
    /// V's chain is walked through ordinary, Array and function headers,
    /// whose [[GetPrototypeOf]] observes nothing. No step can run user code,
    /// so no caller boundary is observable. Anything else leaves `handled`
    /// clear; the kernel then starts over, as nothing here had an effect.
    fn emit_has_instance_lean_front(
        &mut self,
        site: crate::gc_types::I32Local,
        object: &ValueLocals,
        constructor: &ValueLocals,
        boolean: crate::gc_types::I32Local,
        handled: crate::gc_types::I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        handled.set_constant(0, function);
        boolean.set_constant(0, function);
        let function_prototype_layout =
            crate::intrinsics::IntrinsicPropertyLayoutId::FunctionPrototype;
        if !self
            .strings
            .has_intrinsic_property_layout(function_prototype_layout)?
        {
            return Ok(());
        }
        let function_prototype_address = self
            .strings
            .intrinsic_property_layout_span(function_prototype_layout)?
            .address();
        let lean = self.open_frame(ControlFrameKind::Block, function);
        site.load(function);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::I32LtS);
        self.emit_branch_if_to_target(lean, function);
        constructor.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<FunctionObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(lean, function);
        let callable = schema.reserve_gc_local(function).initialize(
            constructor.cast_reference::<FunctionObject>(schema, function),
            function,
        );
        let header = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::OBJECT)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        callable.clear(function);
        // GetMethod(C, @@hasInstance) reaches this Realm's
        // %Function.prototype%, whose own method is immutable, when C and the
        // function objects on its chain before that prototype lack an own
        // @@hasInstance. The kernel's bound on such objects applies.
        let realm = self.emit_execution_realm(function);
        let function_prototype = self.emit_load_non_array_realm_intrinsic_stored(
            &realm,
            NonArrayRealmIntrinsicSlot::FunctionPrototype,
            function,
        );
        realm.clear(function);
        let holder = schema
            .reserve_gc_local::<OrdinaryObject, NonNullable>(function)
            .initialize(header.load(schema, function), function);
        let edge = schema
            .reserve_gc_local::<JavaScriptObject, Nullable>(function)
            .initialize_null(schema, function);
        let hops = schema.reserve_i32_local(function);
        hops.set_constant(0, function);
        let inherits = self.open_frame(ControlFrameKind::Block, function);
        let hop = self.open_frame(ControlFrameKind::Loop, function);
        let lacks =
            self.emit_lacks_own_has_instance_i32(&holder, function_prototype_address, function)?;
        lacks.load(function);
        function.instruction(&Instruction::I32Eqz);
        schema.release_i32_local(lacks, function);
        self.emit_branch_if_to_target(lean, function);
        edge.replace(
            schema
                .field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&holder, schema, function)
                .reference(),
            function,
        );
        let _ = edge.load(schema, function);
        schema
            .field(StoredValueSchema::REFERENCE)
            .read(&function_prototype, schema, function);
        function.instruction(&Instruction::RefEq);
        self.emit_branch_if_to_target(inherits, function);
        hops.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        hops.store(function);
        hops.load(function);
        function.instruction(&Instruction::I32Const(MAX_DEFAULT_HAS_INSTANCE_HOPS));
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(lean, function);
        edge.has_layout::<FunctionObject>(schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(lean, function);
        let callable = schema.reserve_gc_local(function).initialize(
            edge.cast_layout::<FunctionObject>(schema, function),
            function,
        );
        holder.replace(
            schema
                .field(FunctionObjectSchema::OBJECT)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        callable.clear(function);
        self.emit_branch_to_target(hop, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(hops, function);
        edge.clear(function);
        holder.clear(function);
        function_prototype.clear(function);
        // OrdinaryHasInstance step 3 precedes the "prototype" Get.
        self.emit_is_heap_object_like_tag_i32(object.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        handled.set_constant(1, function);
        self.emit_branch_to_target(lean, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let prototype = schema.reserve_value_local(function);
        let found = self.emit_recorded_prototype_read_i32(&header, site, &prototype, function)?;
        header.clear(function);
        found.load(function);
        function.instruction(&Instruction::I32Eqz);
        schema.release_i32_local(found, function);
        self.emit_branch_if_to_target(lean, function);
        // A non-object "prototype" throws; the kernel owns that error.
        self.emit_is_heap_object_like_tag_i32(prototype.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(lean, function);
        let ordinary = schema.reserve_i32_local(function);
        let next = schema
            .reserve_gc_local::<JavaScriptObject, Nullable>(function)
            .initialize_null(schema, function);
        let current = schema
            .reserve_gc_local::<JavaScriptObject, Nullable>(function)
            .initialize_null(schema, function);
        self.emit_ordinary_chain_step(ChainObject::Value(object), &next, ordinary, function);
        let walk = self.open_frame(ControlFrameKind::Loop, function);
        ordinary.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(lean, function);
        next.load(schema, function).is_null(function);
        self.open_frame(ControlFrameKind::If, function);
        handled.set_constant(1, function);
        self.emit_branch_to_target(lean, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let _ = next.load(schema, function);
        prototype.reference().load(function);
        function.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, function);
        boolean.set_constant(1, function);
        handled.set_constant(1, function);
        self.emit_branch_to_target(lean, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        current.replace(next.load(schema, function), function);
        self.emit_ordinary_chain_step(ChainObject::Prototype(&current), &next, ordinary, function);
        self.emit_branch_to_target(walk, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        current.clear(function);
        next.clear(function);
        schema.release_i32_local(ordinary, function);
        prototype.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// True when a function header has no own @@hasInstance: a compact shape
    /// holds String keys only; an intrinsic layout other than
    /// %Function.prototype%'s has no such row; a dictionary cannot hold the
    /// Symbol while it has never been hashed, and otherwise its key index
    /// may prove the key's first bucket empty. Every other case reports false.
    fn emit_lacks_own_has_instance_i32(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        function_prototype_address: u32,
        function: &mut Function,
    ) -> Result<crate::gc_types::I32Local, EmitError> {
        let schema = self.runtime_schema();
        let lacks = schema.reserve_i32_local(function);
        let storage = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(header, schema, function)
                .reference(),
            function,
        );
        storage.is_subtype::<CompactDataStorage>(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        lacks.set_constant(1, function);
        function.instruction(&Instruction::Else);
        storage.is_subtype::<IntrinsicPropertyStorage>(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        let intrinsic = schema.reserve_gc_local(function).initialize(
            storage
                .load(schema, function)
                .cast_subtype::<IntrinsicPropertyStorage>(schema, function),
            function,
        );
        schema
            .field(IntrinsicPropertyStorageSchema::ADDRESS)
            .read(&intrinsic, schema, function)
            .store(lacks, function);
        intrinsic.clear(function);
        lacks.load(function);
        function.instruction(&Instruction::I32Const(function_prototype_address as i32));
        function.instruction(&Instruction::I32Ne);
        lacks.store(function);
        function.instruction(&Instruction::Else);
        let symbol = schema.reserve_gc_local(function).initialize(
            self.emit_well_known_symbol_reference(
                scriptw_ir::WellKnownSymbol::HasInstance,
                function,
            )?,
            function,
        );
        // Every dictionary entry stores its key's hash, and a Symbol is given
        // its hash identity the first time it is hashed.
        schema
            .field(crate::gc_types::SymbolValueSchema::KEY_HASH_ID)
            .read(&symbol, schema, function);
        function.instruction(&Instruction::I64Eqz);
        lacks.store(function);
        lacks.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let dictionary = schema.reserve_gc_local(function).initialize(
            storage
                .load(schema, function)
                .cast_subtype::<OrdinaryPropertyStorage>(schema, function),
            function,
        );
        let key = PropertyKeyLocals::from_symbol(schema, &symbol, function);
        self.emit_ordinary_property_index_lacks_key_i32(&dictionary, &key, function);
        lacks.store(function);
        key.clear(function);
        dictionary.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        symbol.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        storage.clear(function);
        Ok(lacks)
    }

    /// C's own "prototype" data property, through the operator site's
    /// record: a dictionary entry position that the entry's own key object
    /// confirms, or an immutable intrinsic layout's row group confirmed by
    /// the layout address. A miss looks the key up and records where it is.
    /// Absence, accessors and other storage report false.
    fn emit_recorded_prototype_read_i32(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        site: crate::gc_types::I32Local,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<crate::gc_types::I32Local, EmitError> {
        let schema = self.runtime_schema();
        let found = schema.reserve_i32_local(function);
        found.set_constant(0, function);
        let name = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("prototype", function)?,
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &name, function);
        name.clear(function);
        let cache = self.emit_own_data_cache_site(site, key.value(), function);
        let storage = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(header, schema, function)
                .reference(),
            function,
        );
        storage.is_subtype::<OrdinaryPropertyStorage>(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        let dictionary = schema.reserve_gc_local(function).initialize(
            storage
                .load(schema, function)
                .cast_subtype::<OrdinaryPropertyStorage>(schema, function),
            function,
        );
        let position = schema.reserve_i32_local(function);
        schema
            .field(OwnDataReadCacheSchema::DICTIONARY_POSITION)
            .read(&cache, schema, function)
            .store(position, function);
        self.emit_dictionary_position_read(&dictionary, position, &key, value, found, function);
        found.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let entry = self.emit_ordinary_property_entry(header, &key, function)?;
        entry.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let present = schema.reserve_gc_local(function).initialize(
            entry.load(schema, function).require_non_null(function),
            function,
        );
        schema
            .field(PropertyEntrySchema::POSITION)
            .read(&present, schema, function)
            .store(position, function);
        present.clear(function);
        schema
            .field(OwnDataReadCacheSchema::DICTIONARY_POSITION)
            .write(&cache, GcOperand::i32_local(position), schema, function);
        self.emit_dictionary_position_read(&dictionary, position, &key, value, found, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        entry.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(position, function);
        dictionary.clear(function);
        function.instruction(&Instruction::Else);
        storage.is_subtype::<IntrinsicPropertyStorage>(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        let intrinsic = schema.reserve_gc_local(function).initialize(
            storage
                .load(schema, function)
                .cast_subtype::<IntrinsicPropertyStorage>(schema, function),
            function,
        );
        self.emit_own_intrinsic_record_read(&intrinsic, &key, &cache, value, found, function)?;
        intrinsic.clear(function);
        function.instruction(&Instruction::Else);
        let own = self.emit_ordinary_header_own_data_read_i32(header, &key, value, function)?;
        own.load(function);
        found.store(function);
        schema.release_i32_local(own, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        storage.clear(function);
        cache.clear(function);
        key.clear(function);
        Ok(found)
    }

    /// One [[GetPrototypeOf]] step of an ordinary, Array or function object:
    /// sets `ordinary` and loads the edge into `next`. Any other layout, a
    /// Proxy included, clears `ordinary` and leaves `next` unchanged.
    fn emit_ordinary_chain_step(
        &mut self,
        subject: ChainObject<'_>,
        next: &GcLocal<JavaScriptObject, Nullable>,
        ordinary: crate::gc_types::I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        ordinary.set_constant(1, function);
        subject.has_layout::<OrdinaryObject>(schema, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let header = schema.reserve_gc_local(function).initialize(
            subject.cast_layout::<OrdinaryObject>(schema, function),
            function,
        );
        next.replace(
            schema
                .field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, schema, function)
                .reference(),
            function,
        );
        header.clear(function);
        function.instruction(&Instruction::Else);
        subject.has_layout::<ArrayObject>(schema, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let array = schema.reserve_gc_local(function).initialize(
            subject.cast_layout::<ArrayObject>(schema, function),
            function,
        );
        let header = schema.reserve_gc_local(function).initialize(
            schema
                .field(ArrayObjectSchema::OBJECT)
                .read(&array, schema, function)
                .reference(),
            function,
        );
        array.clear(function);
        next.replace(
            schema
                .field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, schema, function)
                .reference(),
            function,
        );
        header.clear(function);
        function.instruction(&Instruction::Else);
        subject.has_layout::<FunctionObject>(schema, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let callable = schema.reserve_gc_local(function).initialize(
            subject.cast_layout::<FunctionObject>(schema, function),
            function,
        );
        let header = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::OBJECT)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        callable.clear(function);
        next.replace(
            schema
                .field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, schema, function)
                .reference(),
            function,
        );
        header.clear(function);
        function.instruction(&Instruction::Else);
        ordinary.set_constant(0, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
    }

    /// One emitted state machine owns both entry modes and bound-target
    /// transitions. Its errors use the supplied execution Realm; this helper
    /// has no actual callable context of its own.
    fn emit_has_instance_kernel(
        &mut self,
        entry: crate::gc_types::I32Local,
        object: &ValueLocals,
        initial_constructor: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let state_local = schema.reserve_i32_local(function);
        let constructor = schema.reserve_value_local(function);
        let handler = schema.reserve_value_local(function);
        let prototype = schema.reserve_value_local(function);
        let search = schema.reserve_value_local(function);
        let call_result = schema.reserve_completion(function);
        let boolean = schema.reserve_i32_local(function);
        // Entering OrdinaryHasInstance without calling the default method is
        // exact only while no user code can run: a getter or trap would see
        // the operator, not the native method, as its caller. `direct` marks
        // such an entry: 1 before the method was looked up, 2 once the
        // lookup has produced it. The first step that could run user code
        // moves that word to `call_method` and restarts through the real
        // call; 2 keeps the resolved method so its lookup is not repeated.
        let direct = schema.reserve_i32_local(function);
        let call_method = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        direct.store(function);
        function.instruction(&Instruction::I32Const(0));
        call_method.store(function);
        constructor.copy_from(initial_constructor, function);
        entry.load(function);
        state_local.store(function);
        function.instruction(&Instruction::I32Const(0));
        boolean.store(function);
        handler.set_undefined(function);
        prototype.set_undefined(function);
        search.set_undefined(function);
        call_result.initialize(function);
        result.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let dispatch = self.open_frame(ControlFrameKind::Loop, function);
        state_local.load(function);
        function.instruction(&Instruction::I32Const(
            HasInstanceRuntimeState::InstanceofOperator.runtime_code(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        call_method.load(function);
        function.instruction(&Instruction::I32Const(2));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_is_heap_object_like_tag_i32(constructor.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::RIGHT_HAND_SIDE_OF_INSTANCEOF_IS_NOT_CALLABLE,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        let inherited = self.emit_inherits_default_has_instance_i32(&constructor, function)?;
        inherited.load(function);
        schema.release_i32_local(inherited, function);
        call_method.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(
            HasInstanceRuntimeState::OrdinaryHasInstance.runtime_code(),
        ));
        state_local.store(function);
        direct.set_constant(1, function);
        self.emit_branch_to_target(dispatch, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        let symbol_slot = schema.reserve_gc_local(function);
        let symbol = symbol_slot.initialize(
            self.emit_well_known_symbol_reference(
                scriptw_ir::WellKnownSymbol::HasInstance,
                function,
            )?,
            function,
        );
        let has_instance_key = PropertyKeyLocals::from_symbol(schema, &symbol, function);
        symbol.clear(function);
        self.emit_object_read_with_throw_routing(
            &constructor,
            &constructor,
            &has_instance_key,
            &call_result,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        has_instance_key.clear(function);
        self.emit_has_instance_abrupt_exit(&call_result, result, exit, function);
        handler.copy_from(call_result.value(), function);
        self.compile_nullish_tagged_i32(handler.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_is_callable_i32(&constructor, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::RIGHT_HAND_SIDE_OF_INSTANCEOF_IS_NOT_CALLABLE,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I32Const(
            HasInstanceRuntimeState::OrdinaryHasInstance.runtime_code(),
        ));
        state_local.store(function);
        self.emit_branch_to_target(dispatch, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_callable_i32(&handler, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::RIGHT_HAND_SIDE_OF_INSTANCEOF_IS_NOT_CALLABLE,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let default_method = self.emit_is_default_has_instance_method_i32(&handler, function);
        default_method.load(function);
        schema.release_i32_local(default_method, function);
        call_method.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(
            HasInstanceRuntimeState::OrdinaryHasInstance.runtime_code(),
        ));
        state_local.store(function);
        direct.set_constant(2, function);
        self.emit_branch_to_target(dispatch, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let handled = self.emit_local_argument_call_from_values(
            &handler,
            &constructor,
            &[object],
            &call_result,
            function,
        )?;
        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let arguments = self.emit_pre_evaluated_arg_vector(&[object], function);
        self.emit_function_handle_call_with_argv_inner(
            &handler,
            Some(&constructor),
            &arguments,
            &call_result,
            PropagateCallThrow::LeaveInCompletion,
            function,
        )?;
        arguments.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(handled, function);
        self.emit_has_instance_abrupt_exit(&call_result, result, exit, function);
        self.compile_truthy_tagged_i32(call_result.value(), function)?;
        boolean.store(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        // The ordinary entry returns false for non-callable C. A bound target
        // re-enters the operator and observes its own @@hasInstance exactly once.
        self.emit_is_callable_i32(&constructor, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, function);
        constructor.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<BoundFunction>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let bound_slot = schema.reserve_gc_local(function);
        let bound = bound_slot.initialize(
            constructor.cast_reference::<BoundFunction>(schema, function),
            function,
        );
        let target_slot = schema.reserve_gc_local(function);
        let target = target_slot.initialize(
            schema
                .struct_type::<BoundFunction>()
                .field(BoundFunctionSchema::TARGET)
                .read(&bound, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&target, &constructor, schema, function);
        target.clear(function);
        bound.clear(function);
        function.instruction(&Instruction::I32Const(
            HasInstanceRuntimeState::InstanceofOperator.runtime_code(),
        ));
        state_local.store(function);
        direct.set_constant(0, function);
        call_method.set_constant(0, function);
        self.emit_branch_to_target(dispatch, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_heap_object_like_tag_i32(object.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, function);
        let string_slot = schema.reserve_gc_local(function);
        let string = string_slot.initialize(
            self.emit_interned_string_reference("prototype", function)?,
            function,
        );
        let prototype_key = PropertyKeyLocals::from_string(schema, &string, function);
        string.clear(function);
        let own_data = self.emit_ordinary_own_data_read_i32(
            &constructor,
            &prototype_key,
            &prototype,
            function,
        )?;
        own_data.load(function);
        function.instruction(&Instruction::I32Eqz);
        schema.release_i32_local(own_data, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_restart_through_method_if_direct(
            direct,
            call_method,
            state_local,
            dispatch,
            function,
        );
        self.emit_object_read_with_throw_routing(
            &constructor,
            &constructor,
            &prototype_key,
            &call_result,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        self.emit_has_instance_abrupt_exit(&call_result, result, exit, function);
        prototype.copy_from(call_result.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        prototype_key.clear(function);
        self.emit_is_heap_object_like_tag_i32(prototype.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::FUNCTION_HAS_NON_OBJECT_PROTOTYPE_IN_INSTANCEOF_CHECK,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        search.copy_from(object, function);
        let walk = self.open_frame(ControlFrameKind::Loop, function);
        let base = self.runtime_helper_base()?;
        // An ordinary header's [[GetPrototypeOf]] observes nothing; composite
        // and exotic objects keep the shared owner.
        search.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<OrdinaryObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let header = schema.reserve_gc_local(function).initialize(
            search.cast_reference::<OrdinaryObject>(schema, function),
            function,
        );
        let next = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<OrdinaryObject>()
                .field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, schema, function)
                .reference(),
            function,
        );
        schema.prototype_reference_into(&next, call_result.value(), function);
        next.clear(function);
        header.clear(function);
        function.instruction(&Instruction::Else);
        search.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        self.emit_restart_through_method_if_direct(
            direct,
            call_method,
            state_local,
            dispatch,
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .call_helper(
                ObjectGetPrototypeOfArguments::new(
                    &search,
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(&call_result, function);
        self.emit_has_instance_abrupt_exit(&call_result, result, exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        call_result.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        function.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(exit, function);
        call_result.value().reference().load(function);
        prototype.reference().load(function);
        function.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(1));
        boolean.store(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        search.copy_from(call_result.value(), function);
        self.emit_branch_to_target(walk, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        result.value().set_boolean(boolean, function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(boolean, function);
        schema.release_i32_local(call_method, function);
        schema.release_i32_local(direct, function);
        call_result.clear(function);
        search.clear(function);
        prototype.clear(function);
        handler.clear(function);
        constructor.clear(function);
        schema.release_i32_local(state_local, function);
        Ok(())
    }

    /// True when GetMethod(C, @@hasInstance) can only reach this Realm's
    /// %Function.prototype%. Each object from C up to that prototype must be
    /// a function without an own @@hasInstance: a compact shape holds string
    /// keys only, an intrinsic layout other than a %Function.prototype% has no
    /// such row, and a dictionary is asked. That prototype's @@hasInstance is
    /// immutable, so the operator's result is OrdinaryHasInstance(C, O).
    fn emit_inherits_default_has_instance_i32(
        &mut self,
        constructor: &ValueLocals,
        function: &mut Function,
    ) -> Result<crate::gc_types::I32Local, EmitError> {
        let schema = self.runtime_schema();
        let inherits = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        inherits.store(function);
        let function_prototype_layout =
            crate::intrinsics::IntrinsicPropertyLayoutId::FunctionPrototype;
        if !self
            .strings
            .has_intrinsic_property_layout(function_prototype_layout)?
        {
            return Ok(inherits);
        }
        let function_prototype_address = self
            .strings
            .intrinsic_property_layout_span(function_prototype_layout)?
            .address();
        let cursor = schema.reserve_value_local(function);
        cursor.copy_from(constructor, function);
        let hops = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        hops.store(function);
        let absent = schema.reserve_i32_local(function);
        let realm = self.emit_execution_realm(function);
        let function_prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::FunctionPrototype,
            &function_prototype,
            function,
        );
        realm.clear(function);
        let symbol = schema.reserve_gc_local(function).initialize(
            self.emit_well_known_symbol_reference(
                scriptw_ir::WellKnownSymbol::HasInstance,
                function,
            )?,
            function,
        );
        let key = PropertyKeyLocals::from_symbol(schema, &symbol, function);
        // Every dictionary entry stores its key's hash, and a Symbol is given
        // its hash identity the first time it is hashed. While @@hasInstance
        // has none, no dictionary anywhere holds it as a key.
        let symbol_hash = schema.reserve_i64_local(function);
        schema
            .field(crate::gc_types::SymbolValueSchema::KEY_HASH_ID)
            .read(&symbol, schema, function)
            .store_i64(symbol_hash, function);
        symbol.clear(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let walk = self.open_frame(ControlFrameKind::Loop, function);
        cursor.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<FunctionObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(done, function);
        let callable = schema.reserve_gc_local(function).initialize(
            cursor.cast_reference::<FunctionObject>(schema, function),
            function,
        );
        let header = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::OBJECT)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        callable.clear(function);
        let storage = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(&header, schema, function)
                .reference(),
            function,
        );
        storage.is_subtype::<CompactDataStorage>(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        absent.set_constant(1, function);
        function.instruction(&Instruction::Else);
        storage.is_subtype::<IntrinsicPropertyStorage>(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        let intrinsic = schema.reserve_gc_local(function).initialize(
            storage
                .load(schema, function)
                .cast_subtype::<IntrinsicPropertyStorage>(schema, function),
            function,
        );
        schema
            .field(IntrinsicPropertyStorageSchema::ADDRESS)
            .read(&intrinsic, schema, function)
            .store(absent, function);
        intrinsic.clear(function);
        absent.load(function);
        function.instruction(&Instruction::I32Const(function_prototype_address as i32));
        function.instruction(&Instruction::I32Ne);
        absent.store(function);
        function.instruction(&Instruction::Else);
        symbol_hash.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        absent.set_constant(1, function);
        function.instruction(&Instruction::Else);
        let dictionary_absent =
            self.emit_ordinary_own_property_absent_i32(&cursor, &key, function)?;
        dictionary_absent.load(function);
        absent.store(function);
        schema.release_i32_local(dictionary_absent, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        storage.clear(function);
        absent.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(done, function);
        let prototype = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, schema, function)
                .reference(),
            function,
        );
        header.clear(function);
        let _ = prototype.load(schema, function);
        function_prototype.reference().load(function);
        function.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, function);
        inherits.set_constant(1, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        hops.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        hops.store(function);
        hops.load(function);
        function.instruction(&Instruction::I32Const(MAX_DEFAULT_HAS_INSTANCE_HOPS));
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, function);
        schema.prototype_reference_into(&prototype, &cursor, function);
        prototype.clear(function);
        self.emit_branch_to_target(walk, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        key.clear(function);
        function_prototype.clear(function);
        schema.release_i64_local(symbol_hash, function);
        schema.release_i32_local(absent, function);
        schema.release_i32_local(hops, function);
        cursor.clear(function);
        Ok(inherits)
    }

    /// True for this execution Realm's own Function.prototype[@@hasInstance].
    /// Calling it with receiver C is OrdinaryHasInstance(C, V), with errors in
    /// the same Realm, so the operator may continue there without the call.
    fn emit_is_default_has_instance_method_i32(
        &mut self,
        handler: &ValueLocals,
        function: &mut Function,
    ) -> crate::gc_types::I32Local {
        let schema = self.runtime_schema();
        let is_default = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        is_default.store(function);
        handler.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<FunctionObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let callable = schema.reserve_gc_local(function).initialize(
            handler.cast_reference::<FunctionObject>(schema, function),
            function,
        );
        let code = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CODE)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        let context = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CONTEXT)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        let handler_realm = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionContextSchema::REALM)
                .read(&context, schema, function)
                .reference(),
            function,
        );
        let kind = schema.reserve_i32_local(function);
        let builtin = schema.reserve_i32_local(function);
        schema
            .field(ExecutableCodeSchema::KIND)
            .read(&code, schema, function)
            .store(kind, function);
        schema
            .field(ExecutableCodeSchema::STANDARD_BUILTIN)
            .read(&code, schema, function)
            .store(builtin, function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(
            ExecutableCodeKind::StandardBuiltin.encode(),
        ));
        function.instruction(&Instruction::I32Eq);
        builtin.load(function);
        function.instruction(&Instruction::I32Const(
            scriptw_ir::StandardBuiltinId::FunctionPrototypeSymbolHasInstance.encode(),
        ));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        let realm = self.emit_execution_realm(function);
        let _ = handler_realm.load(schema, function);
        let _ = realm.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32And);
        is_default.store(function);
        realm.clear(function);
        schema.release_i32_local(builtin, function);
        schema.release_i32_local(kind, function);
        handler_realm.clear(function);
        context.clear(function);
        code.clear(function);
        callable.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        is_default
    }

    /// Leaves a direct OrdinaryHasInstance before a step that could run user
    /// code and restarts the operator so that it calls the method instead.
    /// Nothing observable has happened yet: the steps so far read own data
    /// and ordinary prototypes only.
    fn emit_restart_through_method_if_direct(
        &mut self,
        direct: crate::gc_types::I32Local,
        call_method: crate::gc_types::I32Local,
        state: crate::gc_types::I32Local,
        dispatch: ControlTarget,
        function: &mut Function,
    ) {
        direct.load(function);
        self.open_frame(ControlFrameKind::If, function);
        direct.load(function);
        call_method.store(function);
        direct.set_constant(0, function);
        function.instruction(&Instruction::I32Const(
            HasInstanceRuntimeState::InstanceofOperator.runtime_code(),
        ));
        state.store(function);
        self.emit_branch_to_target(dispatch, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    fn emit_has_instance_abrupt_exit(
        &mut self,
        pending: &CompletionLocals,
        result: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) {
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(pending, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }
}

/// Returns the entry's answer.
fn emit_return_answer(answer: EntryAnswer, f: &mut Function) {
    f.instruction(&Instruction::I32Const(answer.code()));
    f.instruction(&Instruction::Return);
}

/// The object tags are consecutive, so `emit_is_heap_object_like_tag_i32`'s
/// four comparisons are one unsigned range test.
const OBJECT_TAGS: [WasmRuntimeValueTag; 4] = [
    WasmRuntimeValueTag::Object,
    WasmRuntimeValueTag::Array,
    WasmRuntimeValueTag::Function,
    WasmRuntimeValueTag::Arguments,
];
const _: () = {
    let mut index = 0;
    while index < OBJECT_TAGS.len() {
        assert!(OBJECT_TAGS[index] as i32 == OBJECT_TAGS[0] as i32 + index as i32);
        index += 1;
    }
};

/// Whether `tag` is one of `OBJECT_TAGS`.
fn emit_is_object_tag_i32(tag: crate::gc_types::I32Local, f: &mut Function) {
    tag.load(f);
    f.instruction(&Instruction::I32Const(OBJECT_TAGS[0] as i32));
    f.instruction(&Instruction::I32Sub);
    f.instruction(&Instruction::I32Const(OBJECT_TAGS.len() as i32));
    f.instruction(&Instruction::I32LtU);
}
