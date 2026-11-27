use super::*;
use crate::intrinsics::IntrinsicPropertyLayoutId;

#[must_use = "species context must be consumed"]
pub(super) struct PromiseSpeciesRealmContext {
    constructor: ValueLocals,
    type_error: ValueLocals,
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_promise_then_capability(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        receiver: &ValueLocals,
        pending: &CompletionLocals,
        function: &mut Function,
    ) -> Result<GcLocal<PromiseCapability>, EmitError> {
        let schema = self.runtime_schema();
        let selected = schema
            .reserve_gc_local::<PromiseCapability, Nullable>(function)
            .initialize_null(schema, function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let slow = self.open_frame(ControlFrameKind::Block, function);
        let realm = schema
            .reserve_gc_local(function)
            .initialize(self.emit_current_function_realm(function), function);
        self.emit_intrinsic_promise_species_or_branch(promise, &realm, slow, function)?;
        let intrinsic = self.emit_alloc_internal_intrinsic_promise_capability(&realm, function)?;
        selected.replace(intrinsic.load(schema, function).nullable(), function);
        intrinsic.clear(function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        let species = self.emit_current_function_promise_species_realm_context(function);
        self.emit_promise_species_constructor(species, receiver, pending, function)?;
        self.completion().copy_from(pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        let executor =
            self.emit_current_function_promise_internal_function_materialization_context(function);
        let capability =
            self.emit_new_internal_promise_capability(&executor, pending.value(), function)?;
        self.release_promise_internal_function_materialization_context(executor, function);
        selected.replace(capability.load(schema, function).nullable(), function);
        capability.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let capability = schema.reserve_gc_local(function).initialize(
            selected.load(schema, function).require_non_null(function),
            function,
        );
        realm.clear(function);
        selected.clear(function);
        Ok(capability)
    }

    /// The receiver lacks own properties and inherits directly from this
    /// Realm's Promise.prototype. Its untouched constructor row contains the
    /// Realm's intrinsic Promise constructor. Property edits promote the
    /// header away from intrinsic storage.
    pub(super) fn emit_intrinsic_promise_constructor_or_branch(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        realm: &GcLocal<RealmRecord>,
        slow: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let object = schema.reserve_gc_local(function).initialize(
            schema
                .field(PromiseObjectSchema::OBJECT)
                .read(promise, schema, function)
                .reference(),
            function,
        );
        let storage = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, schema, function)
                .reference(),
            function,
        );
        let dictionary = schema.reserve_gc_local(function).initialize(
            storage.sealed_subtype_or_branch::<OrdinaryPropertyStorage>(
                schema,
                self.local_branch_label(slow),
                function,
            ),
            function,
        );
        storage.clear(function);
        schema
            .field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&dictionary, schema, function);
        dictionary.clear(function);
        self.emit_branch_if_to_target(slow, function);

        let value = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::PromisePrototype,
            &value,
            function,
        );
        schema
            .field(OrdinaryObjectSchema::PROTOTYPE)
            .read(&object, schema, function);
        object.clear(function);
        value.reference().load(function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(slow, function);
        let prototype = schema.reserve_gc_local(function).initialize(
            value.sealed_reference_or_branch::<OrdinaryObject>(
                schema,
                self.local_branch_label(slow),
                function,
            ),
            function,
        );
        value.clear(function);
        self.emit_untouched_promise_intrinsic_layout_or_branch(
            &prototype,
            IntrinsicPropertyLayoutId::PromisePrototype,
            slow,
            function,
        )?;
        prototype.clear(function);
        Ok(())
    }

    pub(super) fn emit_intrinsic_promise_species_or_branch(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        realm: &GcLocal<RealmRecord>,
        slow: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_intrinsic_promise_constructor_or_branch(promise, realm, slow, function)?;
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::PromiseConstructor,
            &value,
            function,
        );
        let constructor = schema.reserve_gc_local(function).initialize(
            value.sealed_reference_or_branch::<FunctionObject>(
                schema,
                self.local_branch_label(slow),
                function,
            ),
            function,
        );
        value.clear(function);
        let header = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::OBJECT)
                .read(&constructor, schema, function)
                .reference(),
            function,
        );
        constructor.clear(function);
        // The original @@species getter only returns its receiver.
        self.emit_untouched_promise_intrinsic_layout_or_branch(
            &header,
            IntrinsicPropertyLayoutId::PromiseConstructor,
            slow,
            function,
        )?;
        header.clear(function);
        Ok(())
    }

    fn emit_untouched_promise_intrinsic_layout_or_branch(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        layout: IntrinsicPropertyLayoutId,
        slow: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if !self.strings.has_intrinsic_property_layout(layout)? {
            self.emit_branch_to_target(slow, function);
            return Ok(());
        }
        let span = self.strings.intrinsic_property_layout_span(layout)?;
        let schema = self.runtime_schema();
        let properties = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(object, schema, function)
                .reference(),
            function,
        );
        let rows = schema.reserve_gc_local(function).initialize(
            properties.sealed_subtype_or_branch::<IntrinsicPropertyStorage>(
                schema,
                self.local_branch_label(slow),
                function,
            ),
            function,
        );
        properties.clear(function);
        for (field, expected) in [
            (IntrinsicPropertyStorageSchema::ADDRESS, span.address()),
            (IntrinsicPropertyStorageSchema::COUNT, span.count()),
        ] {
            schema.field(field).read(&rows, schema, function);
            function.instruction(&Instruction::I32Const(expected as i32));
            function.instruction(&Instruction::I32Ne);
            self.emit_branch_if_to_target(slow, function);
        }
        rows.clear(function);
        Ok(())
    }

    pub(super) fn emit_current_function_promise_species_realm_context(
        &mut self,
        function: &mut Function,
    ) -> PromiseSpeciesRealmContext {
        let schema = self.runtime_schema();
        let realm = schema
            .reserve_gc_local(function)
            .initialize(self.emit_current_function_realm(function), function);
        let constructor = schema.reserve_value_local(function);
        let type_error = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::PromiseConstructor,
            &constructor,
            function,
        );
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::TypeErrorPrototype,
            &type_error,
            function,
        );
        realm.clear(function);
        PromiseSpeciesRealmContext {
            constructor,
            type_error,
        }
    }
    pub(super) fn emit_promise_species_constructor(
        &mut self,
        context: PromiseSpeciesRealmContext,
        promise: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        result.set_normal(&context.constructor, function);
        let pending = schema.reserve_completion(function);
        let key_string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("constructor", function)?,
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &key_string, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_object_read_with_throw_routing(
            promise,
            promise,
            &key,
            &pending,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(&pending, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.value().tag().load(function);
        function.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_is_heap_object_like_tag_i32(pending.value().tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error_with_prototype(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::PROMISE_CONSTRUCTOR_PROPERTY_IS_NOT_AN_OBJECT,
            &context.type_error,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let constructor = schema.reserve_value_local(function);
        constructor.copy_from(pending.value(), function);
        let symbol = schema.reserve_gc_local(function).initialize(
            self.emit_well_known_symbol_reference(WellKnownSymbol::Species, function)?,
            function,
        );
        let species_key = PropertyKeyLocals::from_symbol(schema, &symbol, function);
        self.emit_object_read_with_throw_routing(
            &constructor,
            &constructor,
            &species_key,
            &pending,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(&pending, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.compile_nullish_tagged_i32(pending.value().tag(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_is_constructor_i32(pending.value(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error_with_prototype(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::PROMISE_SPECIES_IS_NOT_A_CONSTRUCTOR,
            &context.type_error,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.set_normal(pending.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        species_key.clear(function);
        symbol.clear(function);
        constructor.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        key.clear(function);
        key_string.clear(function);
        pending.clear(function);
        context.type_error.clear(function);
        context.constructor.clear(function);
        Ok(())
    }
}
