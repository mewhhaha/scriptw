use super::*;
use crate::emit::ControlTarget;
use crate::functions::NonArrayRealmIntrinsicSlot;
use crate::gc_types::{
    ArrayObject, ArrayObjectSchema, BoundFunction, BoundFunctionSchema, CompactDataStorage,
    CompletionLocals, ExecutableCodeKind, ExecutableCodeSchema, FunctionContextSchema,
    GcI32Constant, GcNullability, GcOperand, GcStackReference, IntrinsicPropertyStorage,
    IntrinsicPropertyStorageSchema, JavaScriptObject, JavaScriptReference, NonNullable, Nullable,
    OrdinaryObject, OrdinaryObjectSchema, OrdinaryPropertyStorage, OwnDataReadCacheSchema,
    PropertyEntrySchema, StoredValue, StoredValueSchema,
};
use crate::runtime_helpers::{
    HasInstanceArguments, HasInstanceParameters, HelperParameters, ObjectGetPrototypeOfArguments,
    RuntimeHelperId,
};

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
        let (state, constructor, object, site) = match request {
            HasInstanceRequest::InstanceofOperator {
                object,
                constructor,
                site,
            } => (
                HasInstanceRuntimeState::InstanceofOperator,
                constructor,
                object,
                site,
            ),
            HasInstanceRequest::OrdinaryHasInstance {
                constructor,
                object,
            } => (
                HasInstanceRuntimeState::OrdinaryHasInstance,
                constructor,
                object,
                None,
            ),
        };
        let entry = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(state.runtime_code()));
        entry.store(function);
        let site_id = schema.reserve_i32_local(function);
        match site {
            Some(site) => site.emit(site_id, function),
            None => site_id.set_constant(NO_INSTANCEOF_SITE, function),
        }
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
