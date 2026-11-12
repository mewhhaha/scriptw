//! Shared ordinary-header allocation with a closed prototype mutation policy.
use super::*;

enum ObjectPrototypeMutability {
    Mutable,
    Immutable,
}

impl ObjectPrototypeMutability {
    fn word(self) -> i32 {
        match self {
            Self::Mutable => 0,
            Self::Immutable => 1,
        }
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_alloc_plain_object_with_prototype(
        &self,
        prototype: Option<&ValueLocals>,
        function: &mut Function,
    ) -> Result<GcStackReference<OrdinaryObject>, EmitError> {
        self.emit_allocate_object_header(prototype, ObjectPrototypeMutability::Mutable, function)
    }

    /// A fresh plain ordinary object without own properties. It starts on the
    /// shared empty compact storage, so plain data adds keep it compact along
    /// canonical shapes. Exotic and composite headers keep the dictionary
    /// allocator: their own-key and indexed paths never consult a shape.
    pub(crate) fn emit_alloc_compact_object_with_prototype(
        &self,
        prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<GcStackReference<OrdinaryObject>, EmitError> {
        let schema = self.runtime_schema();
        Ok(schema.helper_reference_on_stack(schema.call_helper(
            crate::runtime_helpers::CompactObjectAllocateArguments::new(prototype),
            self.runtime_helper_base()?,
            function,
        )))
    }

    pub(crate) fn compile_compact_object_allocate_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::CompactObjectAllocate);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::CompactObjectAllocateParameters>(
                &mut function,
            );
        let schema = self.runtime_schema();
        let storage = schema.reserve_gc_local(&mut function).initialize(
            schema
                .empty_compact_storage(&mut function)
                .upcast::<crate::gc_types::CompactDataStorage>()
                .upcast(),
            &mut function,
        );
        let mutable = schema.reserve_i32_local(&mut function);
        mutable.set_constant(ObjectPrototypeMutability::Mutable.word(), &mut function);
        let object = self.emit_alloc_object_header_with_storage(
            &parameters.prototype,
            mutable,
            &storage,
            &mut function,
        );
        schema.release_i32_local(mutable, &mut function);
        storage.clear(&mut function);
        // Keep the actual header on the operand stack through root retirement.
        let _ = object;
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    pub(crate) fn emit_alloc_immutable_prototype_object(
        &mut self,
        realm: &GcLocal<crate::gc_types::RealmRecord>,
        function: &mut Function,
    ) -> Result<GcStackReference<OrdinaryObject>, EmitError> {
        let layout = crate::intrinsics::IntrinsicPropertyLayoutId::ObjectPrototype;
        if !self.runtime_bootstrap_plan.full_standard_globals
            && !self.strings.has_intrinsic_property_layout(layout)?
        {
            return self.emit_allocate_object_header(
                None,
                ObjectPrototypeMutability::Immutable,
                function,
            );
        }
        let schema = self.runtime_schema();
        let prototype = schema.reserve_value_local(function);
        prototype.set_scalar(ScalarValue::Null, function);
        let object = self.emit_alloc_intrinsic_layout_header_with_mutability(
            &prototype, layout, realm, true, function,
        )?;
        prototype.clear(function);
        Ok(object)
    }

    fn emit_allocate_object_header(
        &self,
        prototype: Option<&ValueLocals>,
        mutability: ObjectPrototypeMutability,
        function: &mut Function,
    ) -> Result<GcStackReference<OrdinaryObject>, EmitError> {
        let schema = self.runtime_schema();
        let absent_prototype = prototype
            .is_none()
            .then(|| schema.reserve_value_local(function));
        if let Some(absent_prototype) = &absent_prototype {
            absent_prototype.set_scalar(ScalarValue::Null, function);
        }
        let prototype = prototype
            .or(absent_prototype.as_ref())
            .expect("prototype value is initialized");
        let immutable_prototype = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(mutability.word()));
        immutable_prototype.store(function);
        let object = schema.helper_reference_on_stack(schema.call_helper(
            crate::runtime_helpers::OrdinaryObjectAllocateArguments::new(
                prototype,
                immutable_prototype,
            ),
            self.runtime_helper_base()?,
            function,
        ));
        schema.release_i32_local(immutable_prototype, function);
        if let Some(absent_prototype) = absent_prototype {
            absent_prototype.clear(function);
        }
        Ok(object)
    }

    pub(crate) fn compile_ordinary_object_allocate_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::OrdinaryObjectAllocate);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::OrdinaryObjectAllocateParameters>(
                &mut function,
            );
        let schema = self.runtime_schema();
        let object = schema.reserve_gc_local(&mut function).initialize(
            self.emit_alloc_object_header_kernel(
                &parameters.prototype,
                parameters.immutable_prototype,
                &mut function,
            )?,
            &mut function,
        );
        let _ = object.load(schema, &mut function);
        object.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_alloc_object_header_kernel(
        &self,
        prototype: &ValueLocals,
        immutable_prototype: I32Local,
        function: &mut Function,
    ) -> Result<GcStackReference<OrdinaryObject>, EmitError> {
        let schema = self.runtime_schema();
        let zero = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        zero.store(function);
        let entries = schema
            .reserve_gc_local(function)
            .initialize(schema.empty_property_table(function), function);
        let index = schema
            .reserve_gc_local(function)
            .initialize(schema.empty_property_index(function), function);
        let properties = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<OrdinaryPropertyStorage>().construct(
                (
                    GcOperand::reference(&entries, schema),
                    GcOperand::i32_local(zero),
                    GcOperand::reference(&index, schema),
                ),
                function,
            ),
            function,
        );
        let storage = schema
            .reserve_gc_local(function)
            .initialize(properties.load(schema, function).upcast(), function);
        let object = self.emit_alloc_object_header_with_storage(
            prototype,
            immutable_prototype,
            &storage,
            function,
        );
        storage.clear(function);
        properties.clear(function);
        index.clear(function);
        entries.clear(function);
        schema.release_i32_local(zero, function);
        Ok(object)
    }

    /// Both physical ordinary-storage states construct the same object header.
    pub(super) fn emit_alloc_object_header_with_storage(
        &self,
        prototype: &ValueLocals,
        immutable_prototype: I32Local,
        storage: &GcLocal<crate::gc_types::ObjectPropertyStorage>,
        function: &mut Function,
    ) -> GcStackReference<OrdinaryObject> {
        let schema = self.runtime_schema();
        let prototype_slot = schema.reserve_gc_local(function);
        let prototype_record =
            prototype_slot.initialize(schema.prototype_reference(prototype, function), function);
        let private_elements = schema
            .reserve_gc_local(function)
            .initialize(schema.empty_private_elements(function), function);
        let object = schema.struct_type::<OrdinaryObject>().construct(
            (
                GcOperand::reference(&prototype_record, schema),
                GcOperand::reference(storage, schema),
                GcOperand::reference(&private_elements, schema),
                GcOperand::boolean(true),
                GcOperand::boolean_local(immutable_prototype),
                GcOperand::i64(0),
            ),
            function,
        );
        private_elements.clear(function);
        prototype_record.clear(function);
        object
    }
}
