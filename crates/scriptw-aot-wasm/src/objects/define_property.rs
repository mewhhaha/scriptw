//! Typed descriptor compatibility and publication. The IR partial lattice is
//! retained through validation; the stored GC record is always complete.

use super::*;
use crate::gc_types::{I64Local, RuntimeSchema};

mod typed_array;

fn emit_presence<T>(presence: &Presence<T, I32Local>, function: &mut Function) {
    match presence {
        Presence::Absent => {
            function.instruction(&Instruction::I32Const(0));
        }
        Presence::Present(_) => {
            function.instruction(&Instruction::I32Const(1));
        }
        Presence::Runtime { present, .. } => present.load(function),
    }
}
fn emit_flag(flag: DescriptorFlag, function: &mut Function) {
    match flag {
        DescriptorFlag::Known(value) => {
            function.instruction(&Instruction::I32Const(i32::from(value)));
        }
        DescriptorFlag::BooleanPayload(value) => value.load(function),
    }
}
fn emit_mask(flags: I32Local, mask: DescriptorMask, function: &mut Function) {
    flags.load(function);
    function.instruction(&Instruction::I32Const(mask.as_i32()));
    function.instruction(&Instruction::I32And);
    function.instruction(&Instruction::I32Eqz);
    function.instruction(&Instruction::I32Eqz);
}
fn emit_terms(terms: KindTerms<WasmLocals<'_>>, function: &mut Function) {
    function.instruction(&Instruction::I32Const(i32::from(terms.statically_true)));
    for flag in terms.runtime_flags() {
        flag.load(function);
        function.instruction(&Instruction::I32Or);
    }
}

impl FunctionBuilder<'_> {
    fn emit_descriptor_value(
        &self,
        current: &GcLocal<PropertyDescriptor>,
        field: DescriptorField,
        output: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        if field == DescriptorField::Value {
            let value = schema.reserve_gc_local(function).initialize(
                schema
                    .field(PropertyDescriptorSchema::VALUE)
                    .read(current, schema, function)
                    .reference(),
                function,
            );
            self.emit_property_value_into(&value, output, function);
            value.clear(function);
            return;
        }
        let field = match field {
            DescriptorField::Value => unreachable!("data value handled above"),
            DescriptorField::Get => PropertyDescriptorSchema::GETTER,
            DescriptorField::Set => PropertyDescriptorSchema::SETTER,
            DescriptorField::Writable
            | DescriptorField::Enumerable
            | DescriptorField::Configurable => {
                unreachable!("Boolean attributes are normalized separately")
            }
        };
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PropertyDescriptor>()
                .field(field)
                .read(current, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, output, schema, function);
        stored.clear(function);
    }

    /// ValidateAndApplyPropertyDescriptor's immutable compatibility half. The
    /// caller handles absent current descriptors with its extensibility result.
    pub(crate) fn emit_validate_stored_descriptor(
        &mut self,
        current: &GcLocal<PropertyDescriptor>,
        incoming: &WasmDescriptor<'_>,
        valid: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let flags = schema.reserve_i32_local(function);
        let current_value = schema.reserve_value_local(function);
        let classification = classify(incoming);
        let data_terms = classification.terms(DescriptorSide::Data);
        let accessor_terms = classification.terms(DescriptorSide::Accessor);
        let descriptor = incoming.as_partial();
        function.instruction(&Instruction::I32Const(1));
        valid.store(function);
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .read(current, schema, function)
            .store(flags, function);
        emit_mask(flags, DescriptorMask::CONFIGURABLE, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        if let Some(flag) = descriptor.configurable.value() {
            emit_presence(&descriptor.configurable, function);
            emit_flag(*flag, function);
            function.instruction(&Instruction::I32And);
            self.emit_descriptor_reject_if_true(valid, function);
        }
        if let Some(flag) = descriptor.enumerable.value() {
            emit_presence(&descriptor.enumerable, function);
            emit_flag(*flag, function);
            emit_mask(flags, DescriptorMask::ENUMERABLE, function);
            function.instruction(&Instruction::I32Ne);
            function.instruction(&Instruction::I32And);
            self.emit_descriptor_reject_if_true(valid, function);
        }
        emit_terms(accessor_terms, function);
        emit_mask(flags, DescriptorMask::ACCESSOR, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        emit_terms(data_terms, function);
        emit_mask(flags, DescriptorMask::ACCESSOR, function);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        self.emit_descriptor_reject_if_true(valid, function);
        emit_mask(flags, DescriptorMask::ACCESSOR, function);
        self.open_frame(ControlFrameKind::If, function);
        for (field, presence) in [
            (DescriptorField::Get, &descriptor.get),
            (DescriptorField::Set, &descriptor.set),
        ] {
            if let Some(incoming_value) = presence.value() {
                emit_presence(presence, function);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_descriptor_value(current, field, &current_value, function);
                self.emit_tagged_payload_same_value_i32(incoming_value, &current_value, function)?;
                function.instruction(&Instruction::I32Eqz);
                self.emit_descriptor_reject_if_true(valid, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        function.instruction(&Instruction::Else);
        emit_mask(flags, DescriptorMask::WRITABLE, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        if let Some(flag) = descriptor.writable.value() {
            emit_presence(&descriptor.writable, function);
            emit_flag(*flag, function);
            function.instruction(&Instruction::I32And);
            self.emit_descriptor_reject_if_true(valid, function);
        }
        if let Some(incoming_value) = descriptor.value.value() {
            emit_presence(&descriptor.value, function);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_descriptor_value(current, DescriptorField::Value, &current_value, function);
            self.emit_tagged_payload_same_value_i32(incoming_value, &current_value, function)?;
            function.instruction(&Instruction::I32Eqz);
            self.emit_descriptor_reject_if_true(valid, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        current_value.clear(function);
        schema.release_i32_local(flags, function);
        Ok(())
    }

    fn emit_descriptor_reject_if_true(&mut self, valid: I32Local, function: &mut Function) {
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(0));
        valid.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    /// Complete a new descriptor or merge a compatible partial descriptor with
    /// the current one. Kind conversion preserves only E/C and resets the new
    /// side's values and writable before applying supplied fields.
    fn emit_merge_property_descriptor(
        &mut self,
        current: &GcLocal<PropertyDescriptor, Nullable>,
        incoming: &WasmDescriptor<'_>,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyDescriptor>, EmitError> {
        let schema = self.runtime_schema();
        let output_slot = schema.reserve_gc_local(function);
        let value = schema.reserve_gc_local(function).initialize(
            schema
                .stored_undefined(function)
                .upcast::<crate::gc_types::PropertyValue>(),
            function,
        );
        let getter = schema.reserve_value_local(function);
        let setter = schema.reserve_value_local(function);
        let accessor = schema.reserve_i32_local(function);
        let writable = schema.reserve_i32_local(function);
        let enumerable = schema.reserve_i32_local(function);
        let configurable = schema.reserve_i32_local(function);
        let requested_data = schema.reserve_i32_local(function);
        let requested_accessor = schema.reserve_i32_local(function);
        let flags = schema.reserve_i32_local(function);
        getter.set_undefined(function);
        setter.set_undefined(function);
        for flag in [accessor, writable, enumerable, configurable] {
            function.instruction(&Instruction::I32Const(0));
            flag.store(function);
        }
        let classification = classify(incoming);
        emit_terms(classification.terms(DescriptorSide::Data), function);
        requested_data.store(function);
        emit_terms(classification.terms(DescriptorSide::Accessor), function);
        requested_accessor.store(function);
        current.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let record = schema.reserve_gc_local(function).initialize(
            current.load(schema, function).require_non_null(function),
            function,
        );
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&record, schema, function)
            .store(flags, function);
        for (output, mask) in [
            (accessor, DescriptorMask::ACCESSOR),
            (writable, DescriptorMask::WRITABLE),
            (enumerable, DescriptorMask::ENUMERABLE),
            (configurable, DescriptorMask::CONFIGURABLE),
        ] {
            emit_mask(flags, mask, function);
            output.store(function);
        }
        // An attribute-only definition preserves the internal value edge.
        // It need not construct an unobserved intrinsic or copy a snapshot.
        value.replace(
            schema
                .field(PropertyDescriptorSchema::VALUE)
                .read(&record, schema, function)
                .reference(),
            function,
        );
        self.emit_descriptor_value(&record, DescriptorField::Get, &getter, function);
        self.emit_descriptor_value(&record, DescriptorField::Set, &setter, function);
        record.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        requested_accessor.load(function);
        accessor.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        requested_data.load(function);
        accessor.load(function);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        requested_accessor.load(function);
        accessor.store(function);
        value.replace(schema.stored_undefined(function).upcast(), function);
        getter.set_undefined(function);
        setter.set_undefined(function);
        function.instruction(&Instruction::I32Const(0));
        writable.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let descriptor = incoming.as_partial();
        if let Some(input) = descriptor.value.value() {
            emit_presence(&descriptor.value, function);
            self.open_frame(ControlFrameKind::If, function);
            value.replace(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(input, function)
                    .upcast(),
                function,
            );
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        for (presence, output) in [(&descriptor.get, &getter), (&descriptor.set, &setter)] {
            if let Some(input) = presence.value() {
                emit_presence(presence, function);
                self.open_frame(ControlFrameKind::If, function);
                output.copy_from(input, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        for (presence, output) in [
            (&descriptor.writable, writable),
            (&descriptor.enumerable, enumerable),
            (&descriptor.configurable, configurable),
        ] {
            if let Some(input) = presence.value() {
                emit_presence(presence, function);
                self.open_frame(ControlFrameKind::If, function);
                emit_flag(*input, function);
                output.store(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }

        let stored_getter = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&getter, function),
            function,
        );
        let stored_setter = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&setter, function),
            function,
        );
        accessor.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let output = output_slot.initialize(
            schema.struct_type::<PropertyDescriptor>().construct(
                (
                    GcOperand::accessor_descriptor_flags(enumerable, configurable),
                    GcOperand::reference(&value, schema),
                    GcOperand::reference(&stored_getter, schema),
                    GcOperand::reference(&stored_setter, schema),
                ),
                function,
            ),
            function,
        );
        function.instruction(&Instruction::Else);
        output.replace(
            schema.struct_type::<PropertyDescriptor>().construct(
                (
                    GcOperand::data_descriptor_flags(writable, enumerable, configurable),
                    GcOperand::reference(&value, schema),
                    GcOperand::reference(&stored_getter, schema),
                    GcOperand::reference(&stored_setter, schema),
                ),
                function,
            ),
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        stored_setter.clear(function);
        stored_getter.clear(function);
        schema.release_i32_local(flags, function);
        schema.release_i32_local(requested_accessor, function);
        schema.release_i32_local(requested_data, function);
        schema.release_i32_local(configurable, function);
        schema.release_i32_local(enumerable, function);
        schema.release_i32_local(writable, function);
        schema.release_i32_local(accessor, function);
        setter.clear(function);
        getter.clear(function);
        value.clear(function);
        Ok(output)
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_object_define_data_with_flag_locals(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        writable: I32Local,
        enumerable: I32Local,
        configurable: I32Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.runtime_schema()
            .call_helper(
                crate::runtime_helpers::ObjectDefineDataArguments::new(
                    target,
                    key,
                    value,
                    writable,
                    enumerable,
                    configurable,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        Ok(())
    }

    /// Only this registered compiler can enter the physical descriptor kernel.
    pub(crate) fn compile_object_define_data_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::ObjectDefineData);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::ObjectDefineDataParameters>(&mut function);
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_object_define_data_body(
            &parameters.target,
            &parameters.key,
            &parameters.value,
            parameters.writable,
            parameters.enumerable,
            parameters.configurable,
            &result,
            &mut function,
        )?;
        result.emit(&mut function);
        result.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_object_define_data_body(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        writable: I32Local,
        enumerable: I32Local,
        configurable: I32Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let fields = DescriptorObjectFields {
            value: Presence::Present(value),
            writable: Presence::Present(DescriptorFlag::BooleanPayload(writable)),
            enumerable: Presence::Present(DescriptorFlag::BooleanPayload(enumerable)),
            configurable: Presence::Present(DescriptorFlag::BooleanPayload(configurable)),
            ..DescriptorObjectFields::empty()
        };
        self.emit_define_property_or_throw(target, key, &fields, result, function)
    }
    pub(crate) fn emit_object_define_data_with_configurable(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        writable: bool,
        enumerable: bool,
        configurable: bool,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let writable_flag = schema.reserve_i32_local(function);
        let enumerable_flag = schema.reserve_i32_local(function);
        let configurable_flag = schema.reserve_i32_local(function);
        writable_flag.set_constant(i32::from(writable), function);
        enumerable_flag.set_constant(i32::from(enumerable), function);
        configurable_flag.set_constant(i32::from(configurable), function);
        self.emit_object_define_data_with_flag_locals(
            target,
            key,
            value,
            writable_flag,
            enumerable_flag,
            configurable_flag,
            result,
            function,
        )?;
        schema.release_i32_local(configurable_flag, function);
        schema.release_i32_local(enumerable_flag, function);
        schema.release_i32_local(writable_flag, function);
        Ok(())
    }
    pub(crate) fn emit_object_define_data(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_object_define_data_with_configurable(
            target, key, value, true, false, true, result, function,
        )
    }
    pub(crate) fn emit_object_define_enumerable_data(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_object_define_data_with_configurable(
            target, key, value, true, true, true, result, function,
        )
    }
    pub(crate) fn emit_create_data_property_or_throw(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_object_define_enumerable_data(target, key, value, result, function)
    }
    pub(crate) fn emit_object_create_data_property_silent(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let fields = DescriptorObjectFields {
            value: Presence::Present(value),
            writable: Presence::Present(DescriptorFlag::Known(true)),
            enumerable: Presence::Present(DescriptorFlag::Known(true)),
            configurable: Presence::Present(DescriptorFlag::Known(true)),
            ..DescriptorObjectFields::empty()
        };
        self.emit_define_own_property_from_fields(target, key, &fields, result, function)
    }
    pub(crate) fn emit_object_define_accessor_with_flag_local(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        accessors: AccessorDescriptorLocals<'_>,
        enumerable: I32Local,
        configurable: I32Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let (get, set) = accessors.into_fields();
        let fields = DescriptorObjectFields {
            get,
            set,
            enumerable: Presence::Present(DescriptorFlag::BooleanPayload(enumerable)),
            configurable: Presence::Present(DescriptorFlag::BooleanPayload(configurable)),
            ..DescriptorObjectFields::empty()
        };
        self.emit_define_property_or_throw(target, key, &fields, result, function)
    }
    fn emit_define_property_or_throw(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        fields: &DescriptorObjectFields<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_define_own_property_from_fields(target, key, fields, result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.compile_truthy_tagged_i32(result.value(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        // A failed internal definition synthesizes its TypeError in the
        // executing operation's Realm. Preserve the no-message error shape;
        // a proxy's own abrupt completion above remains untouched.
        self.emit_throw_runtime_type_error_without_message(result, function)?;
        function.instruction(&Instruction::Else);
        result.initialize(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_ordinary_define_own_property(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        descriptor: &WasmDescriptor<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let lookup = self.emit_prepare_ordinary_definition(header, key, function)?;
        let current = lookup.current();
        let valid = schema.reserve_i32_local(function);
        current.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .struct_type::<OrdinaryObject>()
            .field(OrdinaryObjectSchema::EXTENSIBLE)
            .read(header, schema, function)
            .store(valid, function);
        function.instruction(&Instruction::Else);
        let record = schema.reserve_gc_local(function).initialize(
            current.load(schema, function).require_non_null(function),
            function,
        );
        self.emit_validate_stored_descriptor(&record, descriptor, valid, function)?;
        record.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        valid.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let merged = self.emit_merge_property_descriptor(current, descriptor, function)?;
        self.emit_publish_ordinary_definition(&lookup, &merged, function);
        merged.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_object_boolean_result(valid, result, function);
        schema.release_i32_local(valid, function);
        lookup.release(schema, function);
        Ok(())
    }

    pub(super) fn emit_copy_complete_descriptor(
        &self,
        destination: &GcLocal<PropertyDescriptor, Nullable>,
        source: &GcLocal<PropertyDescriptor>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let flags = schema.reserve_i32_local(function);
        let writable = schema.reserve_i32_local(function);
        let enumerable = schema.reserve_i32_local(function);
        let configurable = schema.reserve_i32_local(function);
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .read(source, schema, function)
            .store(flags, function);
        for (out, mask) in [
            (writable, DescriptorMask::WRITABLE),
            (enumerable, DescriptorMask::ENUMERABLE),
            (configurable, DescriptorMask::CONFIGURABLE),
        ] {
            emit_mask(flags, mask, function);
            out.store(function);
        }
        emit_mask(flags, DescriptorMask::ACCESSOR, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .write(
                destination,
                GcOperand::accessor_descriptor_flags(enumerable, configurable),
                schema,
                function,
            );
        function.instruction(&Instruction::Else);
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .write(
                destination,
                GcOperand::data_descriptor_flags(writable, enumerable, configurable),
                schema,
                function,
            );
        function.instruction(&Instruction::End);
        let value = schema.reserve_gc_local(function).initialize(
            schema
                .field(PropertyDescriptorSchema::VALUE)
                .read(source, schema, function)
                .reference(),
            function,
        );
        schema.field(PropertyDescriptorSchema::VALUE).write(
            destination,
            GcOperand::reference(&value, schema),
            schema,
            function,
        );
        value.clear(function);
        for field in [
            PropertyDescriptorSchema::GETTER,
            PropertyDescriptorSchema::SETTER,
        ] {
            let value = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<PropertyDescriptor>()
                    .field(field)
                    .read(source, schema, function)
                    .reference(),
                function,
            );
            schema
                .struct_type::<PropertyDescriptor>()
                .field(field)
                .write(
                    destination,
                    GcOperand::reference(&value, schema),
                    schema,
                    function,
                );
            value.clear(function);
        }
        schema.release_i32_local(configurable, function);
        schema.release_i32_local(enumerable, function);
        schema.release_i32_local(writable, function);
        schema.release_i32_local(flags, function);
    }

    fn emit_proxy_define_own_property(
        &mut self,
        slots: &ProxySlotLocals,
        key: &PropertyKeyLocals,
        incoming: &WasmDescriptor<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let method = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let valid = schema.reserve_i32_local(function);
        let setting_non_configurable = schema.reserve_i32_local(function);
        let extensible = schema.reserve_i32_local(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_proxy_named_method(
            slots,
            "defineProperty",
            &method,
            &pending,
            result,
            exit,
            function,
        )?;
        self.compile_nullish_tagged_i32(method.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_define_own_property_from_fields(
            slots.target(),
            key,
            incoming.as_partial(),
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_proxy_method_callable_check(
            &method,
            RuntimeErrorMessage::PROXY_DEFINEPROPERTY_TRAP_IS_NOT_CALLABLE,
            result,
            exit,
            function,
        )?;
        let carrier = schema.reserve_gc_local(function).initialize(
            self.emit_from_property_descriptor(
                DescriptorObjectPrototype::CurrentExecutionRealmObjectPrototype,
                incoming.as_partial(),
                function,
            )?,
            function,
        );
        let descriptor_value = schema.reserve_value_local(function);
        descriptor_value.set_reference(&carrier, schema, function);
        let args = self.emit_pre_evaluated_arg_vector(
            &[slots.target(), key.value(), &descriptor_value],
            function,
        );
        self.emit_function_handle_call_with_argv_inner(
            &method,
            Some(slots.handler()),
            &args,
            &pending,
            PropagateCallThrow::LeaveInCompletion,
            function,
        )?;
        args.clear(function);
        descriptor_value.clear(function);
        carrier.clear(function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        valid.store(function);
        valid.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(valid, result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // Target descriptor is acquired before target extensibility, including
        // the absent-descriptor case. A false outer trap skips both operations.
        let target_descriptor =
            self.emit_proxy_target_own_descriptor(slots.target(), key, function)?;
        let current = target_descriptor.into_descriptor();
        schema
            .call_helper(
                crate::runtime_helpers::ObjectIsExtensibleArguments::new(
                    slots.target(),
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(&pending, function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        extensible.store(function);
        let requested = incoming.as_partial();
        function.instruction(&Instruction::I32Const(0));
        setting_non_configurable.store(function);
        if let Some(flag) = requested.configurable.value() {
            emit_presence(&requested.configurable, function);
            emit_flag(*flag, function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32And);
            setting_non_configurable.store(function);
        }
        current.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        extensible.load(function);
        function.instruction(&Instruction::I32Eqz);
        setting_non_configurable.load(function);
        function.instruction(&Instruction::I32Or);
        self.emit_descriptor_reject_if_true(valid, function);
        function.instruction(&Instruction::Else);
        let record = schema.reserve_gc_local(function).initialize(
            current.load(schema, function).require_non_null(function),
            function,
        );
        self.emit_validate_stored_descriptor(&record, incoming, valid, function)?;
        let flags = schema.reserve_i32_local(function);
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&record, schema, function)
            .store(flags, function);
        setting_non_configurable.load(function);
        emit_mask(flags, DescriptorMask::CONFIGURABLE, function);
        function.instruction(&Instruction::I32And);
        self.emit_descriptor_reject_if_true(valid, function);
        if let Some(flag) = requested.writable.value() {
            emit_presence(&requested.writable, function);
            emit_flag(*flag, function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32And);
            emit_mask(flags, DescriptorMask::WRITABLE, function);
            function.instruction(&Instruction::I32And);
            emit_mask(flags, DescriptorMask::CONFIGURABLE, function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32And);
            emit_mask(flags, DescriptorMask::ACCESSOR, function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32And);
            self.emit_descriptor_reject_if_true(valid, function);
        }
        schema.release_i32_local(flags, function);
        record.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        valid.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(valid, result, function);
        function.instruction(&Instruction::Else);
        self.emit_proxy_execution_realm_type_error(RuntimeErrorMessage::PROXY_DEFINEPROPERTY_TRAP_RESULT_IS_INCOMPATIBLE_WITH_TARGET_DESCRIPTOR, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        current.clear(function);
        schema.release_i32_local(extensible, function);
        schema.release_i32_local(setting_non_configurable, function);
        schema.release_i32_local(valid, function);
        pending.clear(function);
        method.clear(function);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    fn emit_indexed_descriptor(
        &self,
        table: &GcLocal<crate::gc_types::IndexedTable>,
        index: I32Local,
        function: &mut Function,
    ) -> GcLocal<PropertyDescriptor, Nullable> {
        let schema = self.runtime_schema();
        let output = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(function)
            .initialize_null(schema, function);
        index.load(function);
        schema
            .array_type::<crate::gc_types::IndexedTable>()
            .length(table, schema, function);
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        output.replace(
            schema
                .array_type::<crate::gc_types::IndexedTable>()
                .read(table, index, schema, function)
                .reference(),
            function,
        );
        function.instruction(&Instruction::End);
        output
    }

    fn emit_grow_indexed_table(
        &self,
        table: &GcLocal<crate::gc_types::IndexedTable>,
        index: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let length = schema.reserve_i32_local(function);
        let required = schema.reserve_i32_local(function);
        let replacement_length = schema.reserve_i32_local(function);
        let array = schema.array_type::<crate::gc_types::IndexedTable>();
        array.length(table, schema, function);
        length.store(function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        required.store(function);
        self.emit_property_table_growth_capacity(length, required, replacement_length, function);
        let replacement = schema.reserve_gc_local(function).initialize(
            array.filled(GcOperand::null(schema), replacement_length, function),
            function,
        );
        array.copy_prefix_from(&replacement, table, length, schema, function);
        table.replace(replacement.load(schema, function), function);
        replacement.clear(function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(replacement_length, function);
        schema.release_i32_local(required, function);
        schema.release_i32_local(length, function);
    }

    fn emit_define_indexed_descriptor(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        table: &GcLocal<crate::gc_types::IndexedTable>,
        index: I32Local,
        incoming: &WasmDescriptor<'_>,
        valid: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let current = self.emit_indexed_descriptor(table, index, function);
        current.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .struct_type::<OrdinaryObject>()
            .field(OrdinaryObjectSchema::EXTENSIBLE)
            .read(header, schema, function)
            .store(valid, function);
        function.instruction(&Instruction::Else);
        let record = schema.reserve_gc_local(function).initialize(
            current.load(schema, function).require_non_null(function),
            function,
        );
        self.emit_validate_stored_descriptor(&record, incoming, valid, function)?;
        record.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        valid.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let merged = self.emit_merge_property_descriptor(&current, incoming, function)?;
        self.emit_grow_indexed_table(table, index, function);
        schema.array_type::<crate::gc_types::IndexedTable>().write(
            table,
            index,
            GcOperand::nullable_reference(&merged, schema),
            schema,
            function,
        );
        merged.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        current.clear(function);
        Ok(())
    }

    fn emit_define_array_indexed_descriptor(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        array: &GcLocal<crate::gc_types::ArrayObject>,
        index: I64Local,
        incoming: &WasmDescriptor<'_>,
        valid: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let current = self.emit_array_indexed_descriptor(array, index, function);
        current.load(schema, function).is_null(function);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .struct_type::<OrdinaryObject>()
            .field(OrdinaryObjectSchema::EXTENSIBLE)
            .read(header, schema, function)
            .store(valid, function);
        function.instruction(&Instruction::Else);
        let record = schema.reserve_gc_local(function).initialize(
            current.load(schema, function).require_non_null(function),
            function,
        );
        self.emit_validate_stored_descriptor(&record, incoming, valid, function)?;
        record.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        valid.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let merged = self.emit_merge_property_descriptor(&current, incoming, function)?;
        self.emit_array_indexed_publish_descriptor(array, index, &merged, function)?;
        merged.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        current.clear(function);
        Ok(())
    }

    fn emit_array_define_own_property(
        &mut self,
        array: &GcLocal<crate::gc_types::ArrayObject>,
        key: &PropertyKeyLocals,
        incoming: &WasmDescriptor<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let header = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<crate::gc_types::ArrayObject>()
                .field(crate::gc_types::ArrayObjectSchema::OBJECT)
                .read(array, schema, function)
                .reference(),
            function,
        );
        let length_name = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("length", function)?,
            function,
        );
        let length_key = PropertyKeyLocals::from_string(schema, &length_name, function);
        self.emit_tagged_payload_same_value_i32(key.value(), length_key.value(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_array_set_length_descriptor(array, incoming, result, function)?;
        function.instruction(&Instruction::Else);
        let index = schema.reserve_i32_local(function);
        let indexed = schema.reserve_i32_local(function);
        self.emit_property_key_array_index(key, index, indexed, function)?;
        indexed.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let length = schema.reserve_i64_local(function);
        let writable = schema.reserve_i32_local(function);
        let valid = schema.reserve_i32_local(function);
        schema
            .struct_type::<crate::gc_types::ArrayObject>()
            .field(crate::gc_types::ArrayObjectSchema::LENGTH)
            .read(array, schema, function)
            .store_i64(length, function);
        schema
            .struct_type::<crate::gc_types::ArrayObject>()
            .field(crate::gc_types::ArrayObjectSchema::LENGTH_WRITABLE)
            .read(array, schema, function)
            .store(writable, function);
        function.instruction(&Instruction::I32Const(0));
        valid.store(function);
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        length.load(function);
        function.instruction(&Instruction::I64LtU);
        writable.load(function);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        let index64 = schema.reserve_i64_local(function);
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        index64.store(function);
        self.emit_define_array_indexed_descriptor(
            &header, array, index64, incoming, valid, function,
        )?;
        valid.load(function);
        self.open_frame(ControlFrameKind::If, function);
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        length.load(function);
        function.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, function);
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        length.store(function);
        schema
            .struct_type::<crate::gc_types::ArrayObject>()
            .field(crate::gc_types::ArrayObjectSchema::LENGTH)
            .write(array, GcOperand::i64_local(length), schema, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i64_local(index64, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_object_boolean_result(valid, result, function);
        schema.release_i32_local(valid, function);
        schema.release_i32_local(writable, function);
        schema.release_i64_local(length, function);
        function.instruction(&Instruction::Else);
        self.emit_ordinary_define_own_property(&header, key, incoming, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(indexed, function);
        schema.release_i32_local(index, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        length_key.clear(function);
        length_name.clear(function);
        header.clear(function);
        Ok(())
    }

    /// ArraySetLength retains both observable numeric conversions before
    /// acquiring the current length descriptor. Deletion descends, preserves
    /// already removed elements, and restores the failing index plus one.
    pub(crate) fn emit_array_set_length_descriptor(
        &mut self,
        array: &GcLocal<crate::gc_types::ArrayObject>,
        incoming: &WasmDescriptor<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let old_length = schema.reserve_i64_local(function);
        let new_length = schema.reserve_i64_local(function);
        let cursor = schema.reserve_i64_local(function);
        let writable = schema.reserve_i32_local(function);
        let final_writable = schema.reserve_i32_local(function);
        let valid = schema.reserve_i32_local(function);
        let zero = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        zero.store(function);
        let normalized = schema.reserve_value_local(function);
        let old_value = schema.reserve_value_local(function);
        let undefined = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let current_slot = schema.reserve_gc_local::<PropertyDescriptor, Nullable>(function);
        let current = current_slot.initialize_null(schema, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let partial = incoming.as_partial();
        if let Some(value) = partial.value.value() {
            emit_presence(&partial.value, function);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_value_to_number_payload(value, &pending, function)?;
            self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
            self.emit_to_uint32_i64_from_number_payload(
                pending.value().scalar(),
                new_length,
                function,
            );
            self.emit_value_to_number_payload(value, &pending, function)?;
            self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
            new_length.load(function);
            function.instruction(&Instruction::F64ConvertI64U);
            pending.value().scalar().load(function);
            function.instruction(&Instruction::F64ReinterpretI64);
            function.instruction(&Instruction::F64Ne);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_throw_runtime_error(
                scriptw_ir::NativeErrorKind::RangeError,
                RuntimeErrorMessage::INVALID_ARRAY_LENGTH,
                result,
                function,
            )?;
            self.emit_branch_to_target(exit, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            new_length.load(function);
            function.instruction(&Instruction::F64ConvertI64U);
            function.instruction(&Instruction::I64ReinterpretF64);
            normalized.scalar().store(function);
            normalized.set_number(normalized.scalar(), function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        let normalized_partial: WasmPartialDescriptor<'_> = PartialDescriptor {
            value: match partial.value {
                Presence::Present(_) => Presence::Present(&normalized),
                Presence::Runtime { present, .. } => Presence::Runtime {
                    present,
                    value: &normalized,
                },
                Presence::Absent => Presence::Absent,
            },
            writable: partial.writable,
            get: partial.get,
            set: partial.set,
            enumerable: partial.enumerable,
            configurable: partial.configurable,
        };
        schema
            .struct_type::<crate::gc_types::ArrayObject>()
            .field(crate::gc_types::ArrayObjectSchema::LENGTH)
            .read(array, schema, function)
            .store_i64(old_length, function);
        schema
            .struct_type::<crate::gc_types::ArrayObject>()
            .field(crate::gc_types::ArrayObjectSchema::LENGTH_WRITABLE)
            .read(array, schema, function)
            .store(writable, function);
        old_length.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        old_value.scalar().store(function);
        old_value.set_number(old_value.scalar(), function);
        undefined.set_undefined(function);
        let record = self.emit_alloc_property_descriptor(
            StoredPropertyAttributes::Data {
                writable: true,
                enumerable: false,
                configurable: false,
            },
            &old_value,
            &undefined,
            &undefined,
            function,
        );
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .write(
                &record,
                GcOperand::data_descriptor_flags(writable, zero, zero),
                schema,
                function,
            );
        current.replace(record.load(schema, function).nullable(), function);
        let normalized_descriptor = normalized_partial.from_runtime_checked();
        self.emit_validate_stored_descriptor(&record, &normalized_descriptor, valid, function)?;
        record.clear(function);
        valid.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(valid, result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        writable.load(function);
        final_writable.store(function);
        if let Some(flag) = partial.writable.value() {
            emit_presence(&partial.writable, function);
            self.open_frame(ControlFrameKind::If, function);
            emit_flag(*flag, function);
            final_writable.store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        emit_presence(&partial.value, function);
        self.open_frame(ControlFrameKind::If, function);
        old_length.load(function);
        cursor.store(function);
        schema
            .struct_type::<crate::gc_types::ArrayObject>()
            .field(crate::gc_types::ArrayObjectSchema::LENGTH)
            .write(array, GcOperand::i64_local(new_length), schema, function);
        // ArraySetLength step 13: a non-shrinking length is an ordinary
        // define, so only a shrink snapshots indices (push must stay O(1)).
        new_length.load(function);
        old_length.load(function);
        function.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, function);
        // Snapshot only occupied indices, then visit them in descending order.
        // Sparse holes consume no scanning work.
        let keys = self.emit_array_indexed_keys(array, function);
        let remaining = schema.reserve_i32_local(function);
        let slot = schema.reserve_i32_local(function);
        let flags = schema.reserve_i32_local(function);
        schema
            .array_type::<crate::gc_types::ArrayIndexKeyTable>()
            .length(&keys, schema, function);
        remaining.store(function);
        let loop_exit = self.open_frame(ControlFrameKind::Block, function);
        let loop_target = self.open_frame(ControlFrameKind::Loop, function);
        remaining.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(loop_exit, function);
        remaining.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        remaining.store(function);
        remaining.load(function);
        slot.store(function);
        schema
            .array_type::<crate::gc_types::ArrayIndexKeyTable>()
            .read(&keys, slot, schema, function)
            .store_i64(cursor, function);
        cursor.load(function);
        new_length.load(function);
        function.instruction(&Instruction::I64LtU);
        self.emit_branch_if_to_target(loop_exit, function);
        let descriptor = self.emit_array_indexed_descriptor(array, cursor, function);
        descriptor.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&descriptor, schema, function)
            .store(flags, function);
        emit_mask(flags, DescriptorMask::CONFIGURABLE, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_array_indexed_delete(array, cursor, function)?;
        function.instruction(&Instruction::Else);
        cursor.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        new_length.store(function);
        schema
            .struct_type::<crate::gc_types::ArrayObject>()
            .field(crate::gc_types::ArrayObjectSchema::LENGTH)
            .write(array, GcOperand::i64_local(new_length), schema, function);
        function.instruction(&Instruction::I32Const(0));
        valid.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        descriptor.clear(function);
        valid.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(loop_exit, function);
        self.emit_branch_to_target(loop_target, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(flags, function);
        schema.release_i32_local(slot, function);
        schema.release_i32_local(remaining, function);
        keys.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .struct_type::<crate::gc_types::ArrayObject>()
            .field(crate::gc_types::ArrayObjectSchema::LENGTH_WRITABLE)
            .write(
                array,
                GcOperand::boolean_local(final_writable),
                schema,
                function,
            );
        self.emit_object_boolean_result(valid, result, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        current.clear(function);
        pending.clear(function);
        undefined.clear(function);
        old_value.clear(function);
        normalized.clear(function);
        schema.release_i32_local(zero, function);
        schema.release_i32_local(valid, function);
        schema.release_i32_local(final_writable, function);
        schema.release_i32_local(writable, function);
        schema.release_i64_local(cursor, function);
        schema.release_i64_local(new_length, function);
        schema.release_i64_local(old_length, function);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    fn emit_array_length_descriptor(
        &mut self,
        array: &GcLocal<crate::gc_types::ArrayObject>,
        function: &mut Function,
    ) -> GcLocal<PropertyDescriptor> {
        let schema = self.runtime_schema();
        let slot = schema.reserve_gc_local(function);
        let value = schema.reserve_value_local(function);
        let undefined = schema.reserve_value_local(function);
        let writable = schema.reserve_i32_local(function);
        let zero = schema.reserve_i32_local(function);
        schema
            .struct_type::<crate::gc_types::ArrayObject>()
            .field(crate::gc_types::ArrayObjectSchema::LENGTH)
            .read(array, schema, function)
            .store_i64(value.scalar(), function);
        value.scalar().load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        value.scalar().store(function);
        value.set_number(value.scalar(), function);
        schema
            .struct_type::<crate::gc_types::ArrayObject>()
            .field(crate::gc_types::ArrayObjectSchema::LENGTH_WRITABLE)
            .read(array, schema, function)
            .store(writable, function);
        function.instruction(&Instruction::I32Const(0));
        zero.store(function);
        undefined.set_undefined(function);
        let descriptor = self.emit_alloc_property_descriptor(
            StoredPropertyAttributes::Data {
                writable: true,
                enumerable: false,
                configurable: false,
            },
            &value,
            &undefined,
            &undefined,
            function,
        );
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .write(
                &descriptor,
                GcOperand::data_descriptor_flags(writable, zero, zero),
                schema,
                function,
            );
        let result = slot.initialize(descriptor.load(schema, function), function);
        descriptor.clear(function);
        schema.release_i32_local(zero, function);
        schema.release_i32_local(writable, function);
        undefined.clear(function);
        value.clear(function);
        result
    }

    fn emit_string_index_descriptor(
        &mut self,
        primitive: &ValueLocals,
        key: &PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyDescriptor, Nullable>, EmitError> {
        let schema = self.runtime_schema();
        let result = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(function)
            .initialize_null(schema, function);
        let index = schema.reserve_i32_local(function);
        let valid = schema.reserve_i32_local(function);
        primitive.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_property_key_array_index(key, index, valid, function)?;
        let string = schema.reserve_gc_local(function).initialize(
            primitive.cast_reference::<crate::gc_types::StringValue>(schema, function),
            function,
        );
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<crate::gc_types::StringValue>()
                .field(crate::gc_types::StringValueSchema::CODE_UNITS)
                .read(&string, schema, function)
                .reference(),
            function,
        );
        index.load(function);
        schema
            .array_type::<crate::gc_types::CodeUnitArray>()
            .length(&units, schema, function);
        function.instruction(&Instruction::I32LtU);
        valid.load(function);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        let length = schema.reserve_i32_local(function);
        let zero = schema.reserve_i32_local(function);
        let unit = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(1));
        length.store(function);
        function.instruction(&Instruction::I32Const(0));
        zero.store(function);
        schema
            .array_type::<crate::gc_types::CodeUnitArray>()
            .read(&units, index, schema, function)
            .store(unit, function);
        let construction = crate::gc_types::StringConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            length,
            function,
        );
        construction.write(zero, unit, schema, function);
        let character = schema
            .reserve_gc_local(function)
            .initialize(construction.publish(schema, function), function);
        let value = schema.reserve_value_local(function);
        value.set_reference(&character, schema, function);
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let descriptor = self.emit_alloc_property_descriptor(
            StoredPropertyAttributes::Data {
                writable: false,
                enumerable: true,
                configurable: false,
            },
            &value,
            &undefined,
            &undefined,
            function,
        );
        result.replace(descriptor.load(schema, function).nullable(), function);
        descriptor.clear(function);
        undefined.clear(function);
        value.clear(function);
        character.clear(function);
        schema.release_i32_local(unit, function);
        schema.release_i32_local(zero, function);
        schema.release_i32_local(length, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        units.clear(function);
        string.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(valid, function);
        schema.release_i32_local(index, function);
        Ok(result)
    }

    /// Actual non-Proxy storage descriptor dispatcher. Public recursive queries
    /// remain with the real GPD algorithm; this kernel owns only concrete
    /// ordinary/exotic storage and never reinterprets a reference as an address.
    pub(crate) fn emit_non_proxy_own_descriptor(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyDescriptor, Nullable>, EmitError> {
        let schema = self.runtime_schema();
        let result = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(function)
            .initialize_null(schema, function);
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_object_header_projection(target, function),
            function,
        );
        let ordinary = self.emit_ordinary_own_descriptor_reference(&header, key, function)?;
        result.replace(ordinary.load(schema, function), function);
        ordinary.clear(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        result.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, function);
        self.emit_is_module_namespace_i32(target, function);
        self.open_frame(ControlFrameKind::If, function);
        let namespace = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<crate::gc_types::ModuleNamespaceObject>(schema, function),
            function,
        );
        let descriptor = self.emit_namespace_own_descriptor(&namespace, key, function)?;
        result.replace(descriptor.load(schema, function), function);
        descriptor.clear(function);
        namespace.clear(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let index = schema.reserve_i32_local(function);
        let indexed = schema.reserve_i32_local(function);
        self.emit_property_key_array_index(key, index, indexed, function)?;
        target.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Array as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let array = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<crate::gc_types::ArrayObject>(schema, function),
            function,
        );
        indexed.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let index64 = schema.reserve_i64_local(function);
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        index64.store(function);
        let descriptor = self.emit_array_indexed_descriptor(&array, index64, function);
        result.replace(descriptor.load(schema, function), function);
        descriptor.clear(function);
        schema.release_i64_local(index64, function);
        function.instruction(&Instruction::Else);
        let name = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("length", function)?,
            function,
        );
        let length_key = PropertyKeyLocals::from_string(schema, &name, function);
        self.emit_tagged_payload_same_value_i32(key.value(), length_key.value(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        let descriptor = self.emit_array_length_descriptor(&array, function);
        result.replace(descriptor.load(schema, function).nullable(), function);
        descriptor.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        length_key.clear(function);
        name.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        array.clear(function);
        function.instruction(&Instruction::Else);
        target.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Arguments as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        indexed.load(function);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        let arguments = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<crate::gc_types::ArgumentsObject>(schema, function),
            function,
        );
        let table = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<crate::gc_types::ArgumentsObject>()
                .field(crate::gc_types::ArgumentsObjectSchema::INDEXED)
                .read(&arguments, schema, function)
                .reference(),
            function,
        );
        let descriptor = self.emit_indexed_descriptor(&table, index, function);
        descriptor.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let empty = WasmPartialDescriptor::empty()
            .validate()
            .expect("empty descriptor is generic");
        let completed = self.emit_merge_property_descriptor(&descriptor, &empty, function)?;
        let mapping = self.emit_arguments_index_mapping(&arguments, index, function);
        let value = schema.reserve_value_local(function);
        self.emit_descriptor_value(&completed, DescriptorField::Value, &value, function);
        self.emit_arguments_parameter_map_read(&mapping, &value, function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&value, function),
            function,
        );
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::VALUE)
            .write(
                &completed,
                GcOperand::subtype_reference(&stored, schema),
                schema,
                function,
            );
        result.replace(completed.load(schema, function).nullable(), function);
        stored.clear(function);
        value.clear(function);
        mapping.clear(function);
        completed.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        descriptor.clear(function);
        table.clear(function);
        arguments.clear(function);
        function.instruction(&Instruction::Else);
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::PrimitiveBox>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let boxed = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<crate::gc_types::PrimitiveBox>(schema, function),
            function,
        );
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<crate::gc_types::PrimitiveBox>()
                .field(crate::gc_types::PrimitiveBoxSchema::PRIMITIVE)
                .read(&boxed, schema, function)
                .reference(),
            function,
        );
        let primitive = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &primitive, schema, function);
        let descriptor = self.emit_string_index_descriptor(&primitive, key, function)?;
        result.replace(descriptor.load(schema, function), function);
        descriptor.clear(function);
        primitive.clear(function);
        stored.clear(function);
        boxed.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_typed_array_own_descriptor(target, key, &result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(indexed, function);
        schema.release_i32_local(index, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        header.clear(function);
        Ok(result)
    }
}

impl FunctionBuilder<'_> {
    fn emit_arguments_remove_index_mapping(
        &self,
        arguments: &GcLocal<crate::gc_types::ArgumentsObject>,
        index: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let map = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<crate::gc_types::ArgumentsObject>()
                .field(crate::gc_types::ArgumentsObjectSchema::PARAMETER_MAP)
                .read(arguments, schema, function)
                .reference(),
            function,
        );
        map.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        index.load(function);
        schema
            .array_type::<crate::gc_types::ArgumentsParameterMap>()
            .length(&map, schema, function);
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema
            .array_type::<crate::gc_types::ArgumentsParameterMap>()
            .write(&map, index, GcOperand::null(schema), schema, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        map.clear(function);
    }

    fn emit_arguments_define_own_property(
        &mut self,
        arguments: &GcLocal<crate::gc_types::ArgumentsObject>,
        key: &PropertyKeyLocals,
        incoming: &WasmDescriptor<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let header = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<crate::gc_types::ArgumentsObject>()
                .field(crate::gc_types::ArgumentsObjectSchema::OBJECT)
                .read(arguments, schema, function)
                .reference(),
            function,
        );
        let index = schema.reserve_i32_local(function);
        let indexed = schema.reserve_i32_local(function);
        self.emit_property_key_array_index(key, index, indexed, function)?;
        indexed.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let table = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<crate::gc_types::ArgumentsObject>()
                .field(crate::gc_types::ArgumentsObjectSchema::INDEXED)
                .read(arguments, schema, function)
                .reference(),
            function,
        );
        let mapping = self.emit_arguments_index_mapping(arguments, index, function);
        let mapped_value = schema.reserve_value_local(function);
        let snapshot = schema.reserve_i32_local(function);
        let valid = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        snapshot.store(function);
        mapped_value.set_undefined(function);
        let partial = incoming.as_partial();
        if let Some(flag) = partial.writable.value() {
            mapping.load(schema, function);
            function.instruction(&Instruction::RefIsNull);
            function.instruction(&Instruction::I32Eqz);
            emit_presence(&partial.value, function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32And);
            emit_presence(&partial.writable, function);
            emit_flag(*flag, function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32And);
            function.instruction(&Instruction::I32And);
            snapshot.store(function);
        }
        if let Some(value) = partial.value.value() {
            emit_presence(&partial.value, function);
            self.open_frame(ControlFrameKind::If, function);
            mapped_value.copy_from(value, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        snapshot.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_arguments_parameter_map_read(&mapping, &mapped_value, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        emit_presence(&partial.value, function);
        snapshot.load(function);
        function.instruction(&Instruction::I32Or);
        snapshot.store(function);
        let effective: WasmPartialDescriptor<'_> = PartialDescriptor {
            value: Presence::Runtime {
                present: snapshot,
                value: &mapped_value,
            },
            writable: partial.writable,
            get: partial.get,
            set: partial.set,
            enumerable: partial.enumerable,
            configurable: partial.configurable,
        };
        let effective = effective.from_runtime_checked();
        self.emit_define_indexed_descriptor(&header, &table, index, &effective, valid, function)?;
        valid.load(function);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .struct_type::<crate::gc_types::ArgumentsObject>()
            .field(crate::gc_types::ArgumentsObjectSchema::INDEXED)
            .write(
                arguments,
                GcOperand::reference(&table, schema),
                schema,
                function,
            );
        emit_terms(classify(incoming).terms(DescriptorSide::Accessor), function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_arguments_remove_index_mapping(arguments, index, function);
        function.instruction(&Instruction::Else);
        if let Some(value) = partial.value.value() {
            emit_presence(&partial.value, function);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_arguments_parameter_map_write(&mapping, value, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        if let Some(flag) = partial.writable.value() {
            emit_presence(&partial.writable, function);
            emit_flag(*flag, function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_arguments_remove_index_mapping(arguments, index, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_object_boolean_result(valid, result, function);
        schema.release_i32_local(valid, function);
        schema.release_i32_local(snapshot, function);
        mapped_value.clear(function);
        mapping.clear(function);
        table.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_ordinary_define_own_property(&header, key, incoming, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(indexed, function);
        schema.release_i32_local(index, function);
        header.clear(function);
        Ok(())
    }

    /// The actual Object/Reflect definition consumer calls this dispatcher
    /// after ToPropertyDescriptor. It does not call the public builtin facade
    /// on its own target, so recursive Proxy target acquisition remains finite.
    pub(crate) fn emit_object_define_entry_validated(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        incoming: &WasmDescriptor<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let proxy = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<ProxyObject>(schema, function),
            function,
        );
        self.emit_load_live_proxy_slots(
            &proxy,
            ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler,
            result,
            function,
            |builder, slots, function| {
                builder.emit_proxy_define_own_property(slots, key, incoming, result, function)
            },
        )?;
        proxy.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_non_proxy_define_own_property(target, key, incoming, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_non_proxy_define_own_property(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        incoming: &WasmDescriptor<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        target.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Array as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let array = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<crate::gc_types::ArrayObject>(schema, function),
            function,
        );
        self.emit_array_define_own_property(&array, key, incoming, result, function)?;
        array.clear(function);
        function.instruction(&Instruction::Else);
        target.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Arguments as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let arguments = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<crate::gc_types::ArgumentsObject>(schema, function),
            function,
        );
        self.emit_arguments_define_own_property(&arguments, key, incoming, result, function)?;
        arguments.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_is_module_namespace_i32(target, function);
        self.open_frame(ControlFrameKind::If, function);
        let namespace = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<crate::gc_types::ModuleNamespaceObject>(schema, function),
            function,
        );
        key.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_object_header_projection(target, function),
            function,
        );
        self.emit_ordinary_define_own_property(&header, key, incoming, result, function)?;
        header.clear(function);
        function.instruction(&Instruction::Else);
        let current = self.emit_namespace_own_descriptor(&namespace, key, function)?;
        let valid = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        valid.store(function);
        current.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let record = schema.reserve_gc_local(function).initialize(
            current.load(schema, function).require_non_null(function),
            function,
        );
        self.emit_validate_stored_descriptor(&record, incoming, valid, function)?;
        // Namespace data values cannot change, even while the exposed binding
        // descriptor reports writable:true.
        if let Some(value) = incoming.as_partial().value.value() {
            emit_presence(&incoming.as_partial().value, function);
            self.open_frame(ControlFrameKind::If, function);
            let current_value = schema.reserve_value_local(function);
            self.emit_descriptor_value(&record, DescriptorField::Value, &current_value, function);
            self.emit_tagged_payload_same_value_i32(value, &current_value, function)?;
            function.instruction(&Instruction::I32Eqz);
            self.emit_descriptor_reject_if_true(valid, function);
            current_value.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        // A writable:false request also violates namespace [[DefineOwnProperty]].
        if let Some(flag) = incoming.as_partial().writable.value() {
            emit_presence(&incoming.as_partial().writable, function);
            emit_flag(*flag, function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32And);
            self.emit_descriptor_reject_if_true(valid, function);
        }
        record.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_object_boolean_result(valid, result, function);
        schema.release_i32_local(valid, function);
        current.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        namespace.clear(function);
        function.instruction(&Instruction::Else);
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::PrimitiveBox>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let boxed = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<crate::gc_types::PrimitiveBox>(schema, function),
            function,
        );
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<crate::gc_types::PrimitiveBox>()
                .field(crate::gc_types::PrimitiveBoxSchema::PRIMITIVE)
                .read(&boxed, schema, function)
                .reference(),
            function,
        );
        let primitive = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &primitive, schema, function);
        let current = self.emit_string_index_descriptor(&primitive, key, function)?;
        current.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let record = schema.reserve_gc_local(function).initialize(
            current.load(schema, function).require_non_null(function),
            function,
        );
        let valid = schema.reserve_i32_local(function);
        self.emit_validate_stored_descriptor(&record, incoming, valid, function)?;
        self.emit_object_boolean_result(valid, result, function);
        schema.release_i32_local(valid, function);
        record.clear(function);
        function.instruction(&Instruction::Else);
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_object_header_projection(target, function),
            function,
        );
        self.emit_ordinary_define_own_property(&header, key, incoming, result, function)?;
        header.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        current.clear(function);
        primitive.clear(function);
        stored.clear(function);
        boxed.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_typed_array_define_own_property(target, key, incoming, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }
}
