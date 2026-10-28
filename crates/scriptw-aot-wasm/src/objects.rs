use super::*;
use crate::functions::FunctionNamePrefix;
use crate::gc_types::{
    CompletionLocals, GcLocal, GcOperand, GcStackReference, I32Local, I64Local, Nullable,
    OrdinaryObject, OrdinaryObjectSchema, OrdinaryPropertyIndex, OrdinaryPropertyStorage,
    OrdinaryPropertyStorageSchema, PrivateElementTable, PropertyDescriptor,
    PropertyDescriptorSchema, PropertyEntry, PropertyEntrySchema, PropertyTable, ProxyObject,
    ScalarValue, StoredValue, ValueLocals,
};
use crate::operations::BigIntNumberPolicy;
pub(crate) use crate::operations::PropertyKeyLocals;
use crate::runtime_helpers::HelperParameters;

use scriptw_ir::{ComputedPropertyNameInferenceIr, ObjectMethodFunctionIr};

// The Property Descriptor lattice follows ECMA-262 6.2.6.
use scriptw_ir::property_descriptor::{
    classify, DescriptorCarrier, DescriptorClassification, DescriptorField, DescriptorSide,
    KindTerms, KnownPresence, PartialDescriptor, Presence, PropertyDescriptorKind,
    ValidatedDescriptor, TO_PROPERTY_DESCRIPTOR_ORDER,
};

mod accessor_descriptor;
pub(crate) use accessor_descriptor::{
    AccessorDescriptor, AccessorDescriptorLocals, AccessorGetter, AccessorGetterLocals,
    AccessorSetter, AccessorSetterLocals,
};
mod allocation;
mod arguments_properties;
mod array_own_data_read;
mod array_own_indexed_data;
mod compact_data;
mod define_property;
mod dense_array_index;
mod descriptor_object;
mod has_property;
mod header_projection;
mod module_namespace;
mod object_literal_property;
mod packed_array_builtin;
mod packed_array_index;
mod plain_data_literal;
mod runtime_helpers;
mod simple_own_data_read;
mod sparse_array_index;

pub(crate) use descriptor_object::{
    DescriptorFlag, DescriptorObjectFields, DescriptorObjectPrototype,
};

use module_namespace::NamespaceBindingRead;
pub(crate) use module_namespace::NamespaceOwnKeys;
mod primitive_length;
mod private_elements;
mod property_read_plan;
mod property_storage;
mod proxy_own_keys_list;
mod regexp_literal;
pub(crate) use proxy_own_keys_list::CompletedOwnPropertyKeys;
mod proxy_target_descriptor;
pub(crate) use proxy_target_descriptor::CompletedProxyTargetDescriptor;
mod raw_numeric_array_read;
mod typed_array_elements;
mod typed_array_species_create;

pub(crate) use typed_array_species_create::TypedArraySpeciesLengthMethod;

#[must_use]
pub(crate) struct ProxySlotLocals {
    target: ValueLocals,
    handler: ValueLocals,
}

impl ProxySlotLocals {
    pub(crate) fn target(&self) -> &ValueLocals {
        &self.target
    }
    pub(crate) fn handler(&self) -> &ValueLocals {
        &self.handler
    }
    fn clear(self, function: &mut Function) {
        self.handler.clear(function);
        self.target.clear(function);
    }
}

/// A Proxy Get result whose abrupt completion still needs routing.
#[must_use = "a pending Proxy Get trap result must be normalized before inspection"]
struct PendingProxyGetTrapResultLocals<'value> {
    completion: &'value CompletionLocals,
    value: &'value ValueLocals,
}

/// A normal trap result can reach only the consuming descriptor invariant.
#[must_use = "a normal Proxy Get trap result must be consumed by its invariant"]
struct NormalProxyGetTrapResultLocals<'value>(&'value ValueLocals);

impl NormalProxyGetTrapResultLocals<'_> {
    fn value(&self) -> &ValueLocals {
        self.0
    }
}

/// Where a revoked-Proxy TypeError must leave the current emitter.
///
/// The slot reader owns the liveness check, so a caller cannot load a complete
/// `ProxySlotLocals` and then forget to reject the handler sentinel. The routes
/// distinguish both the error Realm authority and how the completion leaves
/// the current body.
pub(crate) enum ProxyRevocationRoute {
    CurrentFunctionRealm,
    ProxyExecutionRealmToActiveHandler,
    ObjectMutationRealmToActiveHandler,
}

#[derive(Clone, Copy)]
enum ProxyExtensibilityOperation {
    IsExtensible,
    PreventExtensions,
}

/// Declares the complete runtime order for object internal-method dispatch.
///
/// `[[HasProperty]]` and direct-target `[[GetOwnProperty]]` both consume this
/// order through exhaustive matches. Adding an exotic here without teaching
/// both operations its representation is therefore a compile error; there is
/// no second hand-maintained variant list that can quietly omit it. Function's
/// internal `prototype` slot belongs to the terminal Ordinary branch.
macro_rules! object_internal_method_branches {
    ($($branch:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum ObjectInternalMethodBranch {
            $($branch),+
        }

        impl ObjectInternalMethodBranch {
            const ORDER: &'static [Self] = &[$(Self::$branch),+];
        }
    };
}

object_internal_method_branches!(
    Proxy,
    Namespace,
    IntegerIndexed,
    Array,
    Arguments,
    BoxedString,
    Ordinary,
);

/// A borrowed partial descriptor retains whole JavaScript values and normalized
/// Boolean/presence locals. Only the conversion factory owns those roots.
pub(crate) type WasmLocals<'v> = descriptor_object::DescriptorObjectLocals<'v>;
pub(crate) type WasmPartialDescriptor<'v> = PartialDescriptor<WasmLocals<'v>>;
pub(crate) type WasmDescriptor<'v> = ValidatedDescriptor<WasmLocals<'v>>;

/// OrdinarySet can only update a value or create a complete data property.
/// Callers cannot put accessor fields behind the direct Array definition path.
enum SetReceiverDataDefinition<'v> {
    Update(&'v ValueLocals),
    Create(&'v ValueLocals),
}

impl<'v> SetReceiverDataDefinition<'v> {
    fn descriptor(self) -> WasmDescriptor<'v> {
        let fields = match self {
            Self::Update(value) => WasmPartialDescriptor {
                value: Presence::Present(value),
                ..WasmPartialDescriptor::empty()
            },
            Self::Create(value) => WasmPartialDescriptor {
                value: Presence::Present(value),
                writable: Presence::Present(DescriptorFlag::Known(true)),
                enumerable: Presence::Present(DescriptorFlag::Known(true)),
                configurable: Presence::Present(DescriptorFlag::Known(true)),
                ..WasmPartialDescriptor::empty()
            },
        };
        fields
            .validate()
            .expect("Set receiver definitions contain only normalized data fields")
    }
}

struct DescriptorFieldLocals {
    present: I32Local,
    value: ValueLocals,
}

#[must_use = "a converted descriptor owns rooted values until consumed"]
pub(crate) struct ReservedPropertyDescriptorLocals {
    fields: [DescriptorFieldLocals; 6],
    writable: I32Local,
    enumerable: I32Local,
    configurable: I32Local,
}
impl ReservedPropertyDescriptorLocals {
    fn field(&self, field: DescriptorField) -> &DescriptorFieldLocals {
        &self.fields[match field {
            DescriptorField::Value => 0,
            DescriptorField::Writable => 1,
            DescriptorField::Get => 2,
            DescriptorField::Set => 3,
            DescriptorField::Enumerable => 4,
            DescriptorField::Configurable => 5,
        }]
    }
    pub(crate) fn field_locals(&self, field: DescriptorField) -> (I32Local, &ValueLocals) {
        let field = self.field(field);
        (field.present, &field.value)
    }
    pub(crate) fn writable_flag(&self) -> I32Local {
        self.writable
    }
    pub(crate) fn enumerable_flag(&self) -> I32Local {
        self.enumerable
    }
    pub(crate) fn configurable_flag(&self) -> I32Local {
        self.configurable
    }
    pub(crate) fn object_fields(&self) -> DescriptorObjectFields<'_> {
        let value = |field| {
            let field = self.field(field);
            Presence::Runtime {
                present: field.present,
                value: &field.value,
            }
        };
        let flag = |field, local| Presence::Runtime {
            present: self.field(field).present,
            value: DescriptorFlag::BooleanPayload(local),
        };
        DescriptorObjectFields {
            value: value(DescriptorField::Value),
            writable: flag(DescriptorField::Writable, self.writable),
            get: value(DescriptorField::Get),
            set: value(DescriptorField::Set),
            enumerable: flag(DescriptorField::Enumerable, self.enumerable),
            configurable: flag(DescriptorField::Configurable, self.configurable),
        }
    }
    pub(crate) fn definition_descriptor(&self) -> WasmDescriptor<'_> {
        // The sole factory has already emitted callable and mixed-kind checks.
        self.object_fields().from_runtime_checked()
    }
    pub(crate) fn clear(self, schema: &crate::gc_types::RuntimeSchema, function: &mut Function) {
        schema.release_i32_local(self.configurable, function);
        schema.release_i32_local(self.enumerable, function);
        schema.release_i32_local(self.writable, function);
        for field in self.fields.into_iter().rev() {
            field.value.clear(function);
            schema.release_i32_local(field.present, function);
        }
    }
}

#[must_use = "a completed descriptor owns its converted roots"]
pub(crate) struct CompletedPropertyDescriptorLocals {
    fields: ReservedPropertyDescriptorLocals,
    accessor: I32Local,
    data: I32Local,
}
impl CompletedPropertyDescriptorLocals {
    pub(crate) fn emit_accessor_i32(&self, function: &mut Function) {
        self.accessor.load(function);
    }
    pub(crate) fn emit_writable_i32(&self, function: &mut Function) {
        self.fields.writable.load(function);
    }
    pub(crate) fn emit_enumerable_i32(&self, function: &mut Function) {
        self.fields.enumerable.load(function);
    }
    pub(crate) fn emit_configurable_i32(&self, function: &mut Function) {
        self.fields.configurable.load(function);
    }
    pub(crate) fn value(&self) -> &ValueLocals {
        &self.fields.field(DescriptorField::Value).value
    }
    pub(crate) fn getter(&self) -> &ValueLocals {
        &self.fields.field(DescriptorField::Get).value
    }
    pub(crate) fn setter(&self) -> &ValueLocals {
        &self.fields.field(DescriptorField::Set).value
    }
    pub(crate) fn object_fields(&self) -> DescriptorObjectFields<'_> {
        DescriptorObjectFields {
            value: Presence::Runtime {
                present: self.data,
                value: self.value(),
            },
            writable: Presence::Runtime {
                present: self.data,
                value: DescriptorFlag::BooleanPayload(self.fields.writable),
            },
            get: Presence::Runtime {
                present: self.accessor,
                value: self.getter(),
            },
            set: Presence::Runtime {
                present: self.accessor,
                value: self.setter(),
            },
            enumerable: Presence::Present(DescriptorFlag::BooleanPayload(self.fields.enumerable)),
            configurable: Presence::Present(DescriptorFlag::BooleanPayload(
                self.fields.configurable,
            )),
        }
    }
    pub(crate) fn clear(self, schema: &crate::gc_types::RuntimeSchema, function: &mut Function) {
        schema.release_i32_local(self.data, function);
        schema.release_i32_local(self.accessor, function);
        self.fields.clear(schema, function);
    }
}

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn emit_is_array_i32(
        &mut self,
        value: &ValueLocals,
        output: I32Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let current = schema.reserve_value_local(function);
        current.copy_from(value, function);
        result.initialize(function);
        function.instruction(&Instruction::I32Const(0));
        output.store(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        current.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::ArrayObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(1));
        output.store(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        current.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, function);
        let proxy = schema.reserve_gc_local(function).initialize(
            current.cast_reference::<ProxyObject>(schema, function),
            function,
        );
        self.emit_load_live_proxy_slots(
            &proxy,
            ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler,
            result,
            function,
            |_, slots, function| {
                current.copy_from(slots.target(), function);
                Ok(())
            },
        )?;
        proxy.clear(function);
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(exit, function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(output, result, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        current.clear(function);
        Ok(())
    }

    pub(crate) fn emit_stored_value_to_locals(
        &self,
        source: &GcLocal<StoredValue>,
        destination: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        schema
            .struct_type::<StoredValue>()
            .read_into(source, destination, schema, function);
    }

    /// Resolves a key through the auxiliary index without invoking getters.
    pub(crate) fn emit_ordinary_own_descriptor_reference(
        &self,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyDescriptor, Nullable>, EmitError> {
        let schema = self.runtime_schema();
        let reference = schema.helper_reference_on_stack(schema.call_helper(
            crate::runtime_helpers::OrdinaryOwnDescriptorArguments::new(object, key),
            self.runtime_helper_base()?,
            function,
        ));
        Ok(schema
            .reserve_gc_local(function)
            .initialize(reference, function))
    }

    fn emit_ordinary_entry_descriptor_reference(
        &self,
        entry: &GcLocal<PropertyEntry, Nullable>,
        function: &mut Function,
    ) -> GcLocal<PropertyDescriptor, Nullable> {
        let schema = self.runtime_schema();
        let result = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(function)
            .initialize_null(schema, function);
        entry.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let present = schema.reserve_gc_local(function).initialize(
            entry.load(schema, function).require_non_null(function),
            function,
        );
        result.replace(
            schema
                .field(PropertyEntrySchema::DESCRIPTOR)
                .read(&present, schema, function)
                .reference()
                .nullable(),
            function,
        );
        present.clear(function);
        function.instruction(&Instruction::End);
        result
    }

    pub(crate) fn emit_alloc_proxy_with_slots(
        &mut self,
        target: &ValueLocals,
        handler: &ValueLocals,
        realm: &crate::functions::ProxyCreationExecutionRealm,
        function: &mut Function,
    ) -> Result<GcStackReference<crate::gc_types::ProxyObject>, EmitError> {
        for (value, message) in [
            (target, RuntimeErrorMessage::PROXY_TARGET_MUST_BE_OBJECT),
            (handler, RuntimeErrorMessage::PROXY_HANDLER_MUST_BE_OBJECT),
        ] {
            self.emit_is_heap_object_like_tag_i32(value.tag(), function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::If(BlockType::Empty));
            self.emit_throw_proxy_creation_type_error(realm, message, function)?;
            self.emit_return_current_completion(function);
            function.instruction(&Instruction::End);
        }
        let schema = self.runtime_schema();
        let target_slot = schema.reserve_gc_local(function);
        let target_record = target_slot.initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(target, function),
            function,
        );
        let handler_slot = schema.reserve_gc_local(function);
        let handler_record = handler_slot.initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(handler, function),
            function,
        );
        // Proxy internal methods use TARGET, not this otherwise empty header's
        // prototype. The header remains the actual shared property record.
        let header_slot = schema.reserve_gc_local(function);
        let header = header_slot.initialize(
            self.emit_alloc_plain_object_with_prototype(None, function)?,
            function,
        );
        let proxy = schema
            .reserve_gc_local::<crate::gc_types::ProxyObject, Nullable>(function)
            .initialize_null(schema, function);
        self.emit_is_callable_i32(target, function)?;
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_is_constructor_i32(target, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        proxy.replace(
            self.emit_alloc_proxy_record(
                &header,
                &target_record,
                &handler_record,
                crate::gc_types::ProxyCallCapability::CallAndConstruct,
                function,
            )
            .nullable(),
            function,
        );
        function.instruction(&Instruction::Else);
        proxy.replace(
            self.emit_alloc_proxy_record(
                &header,
                &target_record,
                &handler_record,
                crate::gc_types::ProxyCallCapability::CallOnly,
                function,
            )
            .nullable(),
            function,
        );
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        proxy.replace(
            self.emit_alloc_proxy_record(
                &header,
                &target_record,
                &handler_record,
                crate::gc_types::ProxyCallCapability::ObjectOnly,
                function,
            )
            .nullable(),
            function,
        );
        function.instruction(&Instruction::End);
        let result = proxy.load(schema, function).require_non_null(function);
        proxy.clear(function);
        header.clear(function);
        handler_record.clear(function);
        target_record.clear(function);
        Ok(result)
    }

    fn emit_alloc_proxy_record(
        &self,
        header: &GcLocal<OrdinaryObject>,
        target: &GcLocal<StoredValue>,
        handler: &GcLocal<StoredValue>,
        capability: crate::gc_types::ProxyCallCapability,
        function: &mut Function,
    ) -> GcStackReference<crate::gc_types::ProxyObject> {
        let schema = self.runtime_schema();
        schema
            .struct_type::<crate::gc_types::ProxyObject>()
            .construct(
                (
                    GcOperand::reference(header, schema),
                    GcOperand::reference(target, schema),
                    GcOperand::reference(handler, schema),
                    GcOperand::constant(capability),
                ),
                function,
            )
    }

    /// Validates the actual revocation field before exposing either slot.
    /// Both the successful continuation and the error retire their roots here.
    pub(crate) fn emit_load_live_proxy_slots(
        &mut self,
        proxy: &GcLocal<crate::gc_types::ProxyObject>,
        route: ProxyRevocationRoute,
        result: &CompletionLocals,
        function: &mut Function,
        consume: impl FnOnce(&mut Self, &ProxySlotLocals, &mut Function) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let target = schema.reserve_value_local(function);
        let handler = schema.reserve_value_local(function);
        for (field, value) in [
            (crate::gc_types::ProxyObjectSchema::TARGET, &target),
            (crate::gc_types::ProxyObjectSchema::HANDLER, &handler),
        ] {
            let record_slot = schema.reserve_gc_local(function);
            let record = record_slot.initialize(
                schema
                    .struct_type::<crate::gc_types::ProxyObject>()
                    .field(field)
                    .read(proxy, schema, function)
                    .reference(),
                function,
            );
            schema
                .struct_type::<StoredValue>()
                .read_into(&record, value, schema, function);
            record.clear(function);
        }
        let slots = ProxySlotLocals { target, handler };
        result.initialize(function);
        slots.handler().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        match route {
            ProxyRevocationRoute::CurrentFunctionRealm => self
                .emit_throw_current_function_realm_type_error(
                    RuntimeErrorMessage::PROXY_HANDLER_IS_NULL,
                    result,
                    function,
                )?,
            ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler => self
                .emit_proxy_execution_realm_type_error(
                    RuntimeErrorMessage::PROXY_HANDLER_IS_NULL,
                    result,
                    function,
                )?,
            ProxyRevocationRoute::ObjectMutationRealmToActiveHandler => self
                .emit_throw_runtime_error(
                    scriptw_ir::NativeErrorKind::TypeError,
                    RuntimeErrorMessage::PROXY_HANDLER_IS_NULL,
                    result,
                    function,
                )?,
        }
        function.instruction(&Instruction::Else);
        consume(self, &slots, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        slots.clear(function);
        Ok(())
    }

    pub(crate) fn emit_object_append_data_property_with_flags(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        writable: bool,
        enumerable: bool,
        configurable: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let descriptor = self.emit_alloc_property_descriptor(
            StoredPropertyAttributes::Data {
                writable,
                enumerable,
                configurable,
            },
            value,
            &undefined,
            &undefined,
            function,
        );
        self.emit_ordinary_append_property_entry(object, key, &descriptor, function)?;
        descriptor.clear(function);
        undefined.clear(function);
        Ok(())
    }

    /// Runtime flags still construct the same complete GC data descriptor.
    /// The metadata publisher supplies closed booleans from its typed ABI.
    pub(crate) fn emit_object_append_data_property_with_runtime_flags(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        writable: I32Local,
        enumerable: I32Local,
        configurable: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let descriptor = self.emit_alloc_data_property_descriptor_with_word(
            GcOperand::data_descriptor_flags(writable, enumerable, configurable),
            value,
            function,
        );
        self.emit_ordinary_append_property_entry(object, key, &descriptor, function)?;
        descriptor.clear(function);
        Ok(())
    }

    pub(crate) fn emit_object_append_accessor_property_with_flags(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        accessors: AccessorDescriptorLocals<'_>,
        enumerable: bool,
        configurable: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let (getter, setter) = accessors.into_fields::<core::convert::Infallible>();
        let descriptor = self.emit_alloc_property_descriptor(
            StoredPropertyAttributes::Accessor {
                enumerable,
                configurable,
            },
            &undefined,
            getter.into_static_value().unwrap_or(&undefined),
            setter.into_static_value().unwrap_or(&undefined),
            function,
        );
        self.emit_ordinary_append_property_entry(object, key, &descriptor, function)?;
        descriptor.clear(function);
        undefined.clear(function);
        Ok(())
    }

    /// ArrayCreate publishes one actual exotic with its explicit prototype;
    /// no consumer repairs the header after allocating the visible object.
    pub(crate) fn emit_alloc_array_payload_with_length_and_prototype(
        &mut self,
        length: crate::gc_types::I64Local,
        prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<GcStackReference<crate::gc_types::ArrayObject>, EmitError> {
        let schema = self.runtime_schema();
        length.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        length.load(function);
        function.instruction(&Instruction::I64Const(u32::MAX as i64));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        let pending = schema.reserve_completion(function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::INVALID_ARRAY_LENGTH,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        pending.clear(function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_alloc_array_payload_with_valid_length_and_prototype(length, prototype, function)
    }

    /// Bootstrap's Array.prototype has a statically valid zero length. It must
    /// not emit a JavaScript completion return into a reference-result helper.
    pub(crate) fn emit_alloc_empty_array_with_prototype(
        &mut self,
        prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<GcStackReference<crate::gc_types::ArrayObject>, EmitError> {
        let schema = self.runtime_schema();
        let length = schema.reserve_i64_local(function);
        length.set_constant(0, function);
        let array = self.emit_alloc_array_payload_with_valid_length_and_prototype(
            length, prototype, function,
        )?;
        schema.release_i64_local(length, function);
        Ok(array)
    }

    fn emit_alloc_array_payload_with_valid_length_and_prototype(
        &mut self,
        length: I64Local,
        prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<GcStackReference<crate::gc_types::ArrayObject>, EmitError> {
        let schema = self.runtime_schema();
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(prototype), function)?,
            function,
        );
        let elements = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<crate::gc_types::ArrayIndexStorage>()
                .empty(schema, function),
            function,
        );
        let array = schema
            .struct_type::<crate::gc_types::ArrayObject>()
            .construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::reference(&elements, schema),
                    GcOperand::i64_local(length),
                    GcOperand::boolean(true),
                ),
                function,
            );
        elements.clear(function);
        header.clear(function);
        Ok(array)
    }

    fn emit_alloc_property_descriptor(
        &self,
        attributes: StoredPropertyAttributes,
        value: &ValueLocals,
        getter: &ValueLocals,
        setter: &ValueLocals,
        function: &mut Function,
    ) -> GcLocal<PropertyDescriptor> {
        // Data descriptors cannot expose getter/setter fields. Their absent
        // values need no per-property immutable snapshots; later redefinition
        // replaces fields on the fresh descriptor, leaving this root unchanged.
        if matches!(&attributes, StoredPropertyAttributes::Data { .. }) {
            return self.emit_alloc_data_property_descriptor_with_word(
                GcOperand::descriptor_word(attributes.descriptor_word()),
                value,
                function,
            );
        }
        self.emit_alloc_property_descriptor_with_word(
            GcOperand::descriptor_word(attributes.descriptor_word()),
            value,
            getter,
            setter,
            function,
        )
    }

    /// The caller proves Data even when its attribute bits are runtime operands.
    /// Only the value needs a snapshot; absent accessors share immutable undefined.
    fn emit_alloc_data_property_descriptor_with_word(
        &self,
        attributes: GcOperand<'_, crate::heap::DescriptorWord, crate::gc_types::NonNullable>,
        value: &ValueLocals,
        function: &mut Function,
    ) -> GcLocal<PropertyDescriptor> {
        let schema = self.runtime_schema();
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(value, function),
            function,
        );
        let absent = schema
            .reserve_gc_local(function)
            .initialize(schema.stored_undefined(function), function);
        let descriptor = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<PropertyDescriptor>().construct(
                (
                    attributes,
                    GcOperand::subtype_reference(&stored, schema),
                    GcOperand::reference(&absent, schema),
                    GcOperand::reference(&absent, schema),
                ),
                function,
            ),
            function,
        );
        absent.clear(function);
        stored.clear(function);
        descriptor
    }

    fn emit_alloc_property_descriptor_with_word(
        &self,
        attributes: GcOperand<'_, crate::heap::DescriptorWord, crate::gc_types::NonNullable>,
        value: &ValueLocals,
        getter: &ValueLocals,
        setter: &ValueLocals,
        function: &mut Function,
    ) -> GcLocal<PropertyDescriptor> {
        let schema = self.runtime_schema();
        let stored_value_slot = schema.reserve_gc_local(function);
        let stored_value = stored_value_slot.initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(value, function),
            function,
        );
        let stored_getter_slot = schema.reserve_gc_local(function);
        let stored_getter = stored_getter_slot.initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(getter, function),
            function,
        );
        // Identical compiler operands need one immutable snapshot. Ordinary
        // data descriptors pass the same undefined local for both absent
        // accessors. The descriptor fields remain independently replaceable.
        // This is a compile-time alias proof, with no JS equality or branches.
        let stored_setter = (!std::ptr::eq(getter, setter)).then(|| {
            schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(setter, function),
                function,
            )
        });
        let setter_snapshot = stored_setter.as_ref().unwrap_or(&stored_getter);
        let descriptor_slot = schema.reserve_gc_local(function);
        let descriptor = descriptor_slot.initialize(
            schema.struct_type::<PropertyDescriptor>().construct(
                (
                    attributes,
                    GcOperand::subtype_reference(&stored_value, schema),
                    GcOperand::reference(&stored_getter, schema),
                    GcOperand::reference(setter_snapshot, schema),
                ),
                function,
            ),
            function,
        );
        if let Some(stored_setter) = stored_setter {
            stored_setter.clear(function);
        }
        stored_getter.clear(function);
        stored_value.clear(function);
        descriptor
    }

    fn emit_object_method_value_to_locals(
        &mut self,
        method: &ObjectMethodFunctionIr,
        home: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let meta = self
            .functions
            .get(method.function_id())
            .cloned()
            .ok_or_else(|| {
                EmitError::unsupported(format!("missing object method `{}`", method.function_id()))
            })?;
        if meta.protocol() != method.protocol() {
            return Err(EmitError::unsupported(
                "object method protocol disagrees with its planned code",
            ));
        }
        let schema = self.runtime_schema();
        let private_environment = schema.reserve_gc_local(function).initialize(
            self.current_private_environment().load(schema, function),
            function,
        );
        let field_keys = schema
            .reserve_gc_local::<crate::gc_types::ValueArray, Nullable>(function)
            .initialize_null(schema, function);
        let callable = schema.reserve_gc_local(function).initialize(
            self.emit_method_function_record(
                &meta,
                crate::functions::FunctionHomeObject::Object(home),
                &private_environment,
                &field_keys,
                function,
            )?,
            function,
        );
        let prefix = match method.protocol() {
            FunctionProtocolIr::ObjectMethod(_) => FunctionNamePrefix::None,
            FunctionProtocolIr::ObjectGetter => FunctionNamePrefix::Getter,
            FunctionProtocolIr::ObjectSetter => FunctionNamePrefix::Setter,
            FunctionProtocolIr::OrdinaryCallOnly
            | FunctionProtocolIr::OrdinaryCallAndConstruct
            | FunctionProtocolIr::Arrow
            | FunctionProtocolIr::Generator
            | FunctionProtocolIr::Async
            | FunctionProtocolIr::AsyncArrow
            | FunctionProtocolIr::AsyncGenerator
            | FunctionProtocolIr::ModuleActivation
            | FunctionProtocolIr::AsyncModuleActivation
            | FunctionProtocolIr::ClassConstructor
            | FunctionProtocolIr::ClassMethod(_)
            | FunctionProtocolIr::ClassGetter
            | FunctionProtocolIr::ClassSetter => {
                unreachable!("ObjectMethodFunctionIr owns an object method protocol")
            }
        };
        self.emit_set_function_name(&callable, key, prefix, function)?;
        output.set_reference(&callable, schema, function);
        callable.clear(function);
        field_keys.clear(function);
        private_environment.clear(function);
        Ok(())
    }

    pub(crate) fn compile_object_literal_payload(
        &mut self,
        properties: &[ObjectPropertyIr],
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if self.try_compile_plain_data_literal(properties, output, function)? {
            return Ok(());
        }
        let schema = self.runtime_schema();
        let prototype = schema.reserve_value_local(function);
        self.emit_source_literal_prototype_to_value(
            crate::environments::global_environment::SourceLiteralPrototype::Object,
            &prototype,
            function,
        );
        let object = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), function)?,
            function,
        );
        let target = schema.reserve_value_local(function);
        target.set_reference(&object, schema, function);
        let pending = schema.reserve_completion(function);
        let value = schema.reserve_value_local(function);
        for property in properties {
            self.emit_object_literal_property(
                property, &object, &target, &value, &pending, function,
            )?;
        }
        output.set_reference(&object, schema, function);
        value.clear(function);
        pending.clear(function);
        target.clear(function);
        object.clear(function);
        prototype.clear(function);
        Ok(())
    }

    fn emit_literal_property_abrupt_exit(
        &mut self,
        pending: &CompletionLocals,
        function: &mut Function,
    ) {
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn compile_property_read_to_locals(
        &mut self,
        target: &TypedExpr,
        key: &PropertyKeyIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if self.try_compile_concat_length_property_read(target, key, output, function)? {
            return Ok(());
        }
        let receiver = self.runtime_schema().reserve_value_local(function);
        self.compile_expr_to_value(target, &receiver, function)?;
        self.compile_property_read_from_locals(target, key, &receiver, output, function)?;
        receiver.clear(function);
        Ok(())
    }

    /// Evaluate the computed key expression before RequireObjectCoercible;
    /// ToPropertyKey follows that check. The raw receiver remains unchanged.
    pub(crate) fn compile_property_read_from_locals(
        &mut self,
        target: &TypedExpr,
        key: &PropertyKeyIr,
        receiver: &ValueLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if self
            .try_compile_primitive_string_static_length(target, key, receiver, output, function)?
        {
            return Ok(());
        }
        self.compile_ordinary_property_read_from_locals(target, key, receiver, output, function)
    }

    fn compile_ordinary_property_read_from_locals(
        &mut self,
        target: &TypedExpr,
        key: &PropertyKeyIr,
        receiver: &ValueLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let key_value = schema.reserve_value_local(function);
        self.compile_property_key_operand(key, &key_value, function)?;
        let pending = schema.reserve_completion(function);
        if !self.emit_planned_property_read(
            target, key, receiver, receiver, &key_value, &pending, function,
        )? {
            schema
                .call_helper(
                    crate::runtime_helpers::DynamicPropertyReadArguments::new(
                        receiver,
                        receiver,
                        &key_value,
                        self.current_environment(),
                        self.current_program(),
                    ),
                    self.runtime_helper_base()?,
                    function,
                )
                .store(&pending, function);
        }
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&pending, function);
        function.instruction(&Instruction::Else);
        output.copy_from(pending.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        key_value.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }

    pub(crate) fn emit_dynamic_property_read_with_key_locals(
        &mut self,
        target: &ValueLocals,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        schema
            .call_helper(
                crate::runtime_helpers::ObjectReadArguments::new(
                    target,
                    receiver,
                    key,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        Ok(())
    }

    fn compile_property_key_operand(
        &mut self,
        key: &PropertyKeyIr,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match key {
            PropertyKeyIr::StaticString(text) => {
                let schema = self.runtime_schema();
                let string = schema.reserve_gc_local(function).initialize(
                    self.emit_interned_string_reference(text, function)?,
                    function,
                );
                value.set_reference(&string, schema, function);
                string.clear(function);
            }
            PropertyKeyIr::ArrayLength => {
                let schema = self.runtime_schema();
                let string = schema.reserve_gc_local(function).initialize(
                    self.emit_interned_string_reference("length", function)?,
                    function,
                );
                value.set_reference(&string, schema, function);
                string.clear(function);
            }
            PropertyKeyIr::StringExpr(expr) | PropertyKeyIr::ArrayIndex(expr) => {
                self.compile_expr_to_value(expr, value, function)?;
            }
        }
        Ok(())
    }

    pub(crate) fn emit_typed_array_or_object_index_read_from_locals(
        &mut self,
        target: &ValueLocals,
        index: crate::gc_types::I64Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.runtime_schema()
            .call_helper(
                crate::runtime_helpers::IndexedElementReadArguments::new(
                    target,
                    index,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        Ok(())
    }

    pub(crate) fn compile_indexed_element_read_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::IndexedElementRead);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::IndexedElementReadParameters>(
                &mut function,
            );
        let schema = self.runtime_schema();
        let result = schema.reserve_completion(&mut function);
        // Probe the numeric own-data domain before allocating a decimal key.
        // A hole or accessor must still run ordinary Get, and Proxy wrappers
        // never satisfy the concrete Array reference test.
        let found = schema.reserve_i32_local(&mut function);
        found.set_constant(0, &mut function);
        parameters.target.reference().load(&mut function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::ArrayObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        parameters.index.load(&mut function);
        function.instruction(&Instruction::I64Const(u32::MAX as i64));
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, &mut function);
        let array = schema.reserve_gc_local(&mut function).initialize(
            parameters
                .target
                .cast_reference::<crate::gc_types::ArrayObject>(schema, &mut function),
            &mut function,
        );
        let value = schema.reserve_value_local(&mut function);
        self.emit_array_own_indexed_value(&array, parameters.index, &value, found, &mut function);
        found.load(&mut function);
        self.open_frame(ControlFrameKind::If, &mut function);
        result.set_normal(&value, &mut function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        value.clear(&mut function);
        array.clear(&mut function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        found.load(&mut function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut function);
        let number = schema.reserve_i64_local(&mut function);
        parameters.index.load(&mut function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        number.store(&mut function);
        let string_slot = schema.reserve_gc_local(&mut function);
        let string = schema
            .call_helper(
                crate::runtime_helpers::NumberToStringArguments::new(number),
                self.runtime_helper_base()?,
                &mut function,
            )
            .bind(schema, string_slot, &mut function);
        let key = PropertyKeyLocals::from_string(schema, &string, &mut function);
        self.emit_object_read_with_throw_routing(
            &parameters.target,
            &parameters.target,
            &key,
            &result,
            AccessorThrowRouting::LeaveInCompletion,
            &mut function,
        )?;
        key.clear(&mut function);
        string.clear(&mut function);
        schema.release_i64_local(number, &mut function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(found, &mut function);
        result.emit(&mut function);
        result.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    pub(crate) fn emit_typed_array_or_object_index_write_from_locals(
        &mut self,
        target: &ValueLocals,
        index: crate::gc_types::I64Local,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        strict: I32Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.runtime_schema()
            .call_helper(
                crate::runtime_helpers::IndexedElementWriteArguments::new(
                    target,
                    index,
                    key,
                    value,
                    strict,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        Ok(())
    }

    pub(crate) fn compile_indexed_element_write_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::IndexedElementWrite);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::IndexedElementWriteParameters>(
                &mut function,
            );
        let result = self.runtime_schema().reserve_completion(&mut function);
        // The retained String/Symbol key is the actual Reference name. The
        // complete Set owner selects integer-indexed behavior from that key.
        self.emit_object_write(
            &parameters.target,
            &parameters.key,
            &parameters.value,
            parameters.strict,
            &result,
            &mut function,
        )?;
        result.emit(&mut function);
        result.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    pub(crate) fn compile_property_write_to_locals(
        &mut self,
        target: &TypedExpr,
        key: &PropertyKeyIr,
        value: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let lookup = schema.reserve_value_local(function);
        let raw_key = schema.reserve_value_local(function);
        let incoming = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let strict = schema.reserve_i32_local(function);
        self.compile_expr_to_value(target, &receiver, function)?;
        self.compile_property_key_operand(key, &raw_key, function)?;
        self.compile_expr_to_value(value, &incoming, function)?;
        match self.object_write_strict_flag_local {
            Some(carried) => carried.load(function),
            None => {
                function.instruction(&Instruction::I32Const(i32::from(self.strict)));
            }
        }
        strict.store(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        // PutValue boxes the base and only then coerces the retained name,
        // after evaluating the RHS. The Set receiver remains the raw base.
        self.emit_value_to_object_locals(&receiver, &pending, function)?;
        self.emit_object_operation_abrupt_exit(&pending, &pending, exit, function);
        lookup.copy_from(pending.value(), function);
        self.emit_value_to_property_key_completion(&raw_key, &pending, function)?;
        self.emit_object_operation_abrupt_exit(&pending, &pending, exit, function);
        let completed_key = self.emit_value_to_property_key_locals(pending.value(), function)?;
        self.emit_object_write_with_receiver(
            &lookup,
            &receiver,
            &completed_key,
            &incoming,
            strict,
            &pending,
            function,
        )?;
        completed_key.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&pending, function);
        function.instruction(&Instruction::Else);
        output.copy_from(&incoming, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(strict, function);
        pending.clear(function);
        incoming.clear(function);
        raw_key.clear(function);
        lookup.clear(function);
        receiver.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }

    /// Delete preserves Reference evaluation and delays ToPropertyKey until
    /// after ToObject. A failed strict delete produces a whole native Throw.
    pub(crate) fn compile_delete_property_i32(
        &mut self,
        target: &TypedExpr,
        key: &PropertyKeyIr,
        strictness: Strictness,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let raw_key = schema.reserve_value_local(function);
        let object = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let boolean = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        boolean.store(function);
        self.compile_expr_to_value(target, &receiver, function)?;
        self.compile_property_key_operand(key, &raw_key, function)?;
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_value_to_object_locals(&receiver, &pending, function)?;
        self.emit_object_operation_abrupt_exit(&pending, &pending, exit, function);
        object.copy_from(pending.value(), function);
        self.emit_value_to_property_key_completion(&raw_key, &pending, function)?;
        self.emit_object_operation_abrupt_exit(&pending, &pending, exit, function);
        let completed_key = self.emit_value_to_property_key_locals(pending.value(), function)?;
        self.emit_object_delete(&object, &completed_key, &pending, function)?;
        completed_key.clear(function);
        self.emit_object_operation_abrupt_exit(&pending, &pending, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        boolean.store(function);
        if strictness.throws_on_failed_set() {
            boolean.load(function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_throw_runtime_error(
                scriptw_ir::NativeErrorKind::TypeError,
                RuntimeErrorMessage::CANNOT_DELETE_PROPERTY,
                &pending,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&pending, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        object.clear(function);
        raw_key.clear(function);
        receiver.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        boolean.load(function);
        schema.release_i32_local(boolean, function);
        Ok(())
    }

    pub(crate) fn emit_in_i32(
        &mut self,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let name = schema.reserve_value_local(function);
        let object = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let boolean = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        boolean.store(function);
        // Both GetValue operations precede the object test and name coercion.
        self.compile_expr_to_value(lhs, &name, function)?;
        self.compile_expr_to_value(rhs, &object, function)?;
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_is_heap_object_like_tag_i32(object.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::RIGHT_HAND_SIDE_OF_IN_IS_NOT_AN_OBJECT,
            &pending,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_value_to_property_key_completion(&name, &pending, function)?;
        self.emit_object_operation_abrupt_exit(&pending, &pending, exit, function);
        let completed_key = self.emit_value_to_property_key_locals(pending.value(), function)?;
        schema
            .call_helper(
                crate::runtime_helpers::ObjectHasPropertyArguments::new(
                    &object,
                    &completed_key,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(&pending, function);
        completed_key.clear(function);
        self.emit_object_operation_abrupt_exit(&pending, &pending, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        boolean.store(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&pending, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        object.clear(function);
        name.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        boolean.load(function);
        schema.release_i32_local(boolean, function);
        Ok(())
    }

    pub(crate) fn compile_object_key_to_locals(
        &mut self,
        key: &PropertyKeyIr,
        function: &mut Function,
    ) -> Result<PropertyKeyLocals, EmitError> {
        let value = self.runtime_schema().reserve_value_local(function);
        self.compile_property_key_operand(key, &value, function)?;
        let completed = self.emit_value_to_property_key_locals(&value, function)?;
        value.clear(function);
        Ok(completed)
    }

    pub(crate) fn emit_direct_own_descriptor_fact(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyDescriptor, Nullable>, EmitError> {
        Ok(self
            .emit_proxy_target_descriptor_fact(target, key, function)?
            .into_descriptor())
    }

    /// Private descriptor-result objects are read through their own data
    /// entries; this projection neither invokes accessors nor walks prototypes.
    pub(crate) fn emit_object_own_data_field_read(
        &mut self,
        object: &ValueLocals,
        key: &PropertyKeyLocals,
        present: crate::gc_types::I32Local,
        result: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        function.instruction(&Instruction::I32Const(0));
        present.store(function);
        result.set_undefined(function);
        self.emit_is_heap_object_like_tag_i32(object.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_object_header_projection(object, function),
            function,
        );
        self.emit_ordinary_own_data_value(&header, key, result, present, function)?;
        header.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// ToPropertyDescriptor performs HasProperty then Get exactly once per
    /// field, in the normative order, retaining original abrupt completions.
    pub(crate) fn emit_to_property_descriptor(
        &mut self,
        source: &ValueLocals,
        type_error_message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<ReservedPropertyDescriptorLocals, EmitError> {
        let schema = self.runtime_schema();
        let fields = core::array::from_fn(|_| {
            let present = schema.reserve_i32_local(function);
            function.instruction(&Instruction::I32Const(0));
            present.store(function);
            let value = schema.reserve_value_local(function);
            value.set_undefined(function);
            DescriptorFieldLocals { present, value }
        });
        let writable = schema.reserve_i32_local(function);
        let enumerable = schema.reserve_i32_local(function);
        let configurable = schema.reserve_i32_local(function);
        for flag in [writable, enumerable, configurable] {
            function.instruction(&Instruction::I32Const(0));
            flag.store(function);
        }
        let converted = ReservedPropertyDescriptorLocals {
            fields,
            writable,
            enumerable,
            configurable,
        };
        let pending = schema.reserve_completion(function);
        self.emit_is_heap_object_like_tag_i32(source.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            type_error_message,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for field in TO_PROPERTY_DESCRIPTOR_ORDER {
            let output = converted.field(field);
            let name = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference(field.key(), function)?,
                function,
            );
            let key = PropertyKeyLocals::from_string(schema, &name, function);
            name.clear(function);
            schema
                .call_helper(
                    crate::runtime_helpers::ObjectHasPropertyArguments::new(
                        source,
                        &key,
                        self.current_environment(),
                        self.current_program(),
                    ),
                    self.runtime_helper_base()?,
                    function,
                )
                .store(&pending, function);
            self.emit_descriptor_conversion_abrupt_exit(&pending, function);
            self.compile_truthy_tagged_i32(pending.value(), function)?;
            output.present.store(function);
            output.present.load(function);
            self.open_frame(ControlFrameKind::If, function);
            schema
                .call_helper(
                    crate::runtime_helpers::ObjectReadArguments::new(
                        source,
                        source,
                        &key,
                        self.current_environment(),
                        self.current_program(),
                    ),
                    self.runtime_helper_base()?,
                    function,
                )
                .store(&pending, function);
            self.emit_descriptor_conversion_abrupt_exit(&pending, function);
            output.value.copy_from(pending.value(), function);
            match field {
                DescriptorField::Writable
                | DescriptorField::Enumerable
                | DescriptorField::Configurable => {
                    let flag = match field {
                        DescriptorField::Writable => writable,
                        DescriptorField::Enumerable => enumerable,
                        DescriptorField::Configurable => configurable,
                        DescriptorField::Value | DescriptorField::Get | DescriptorField::Set => {
                            unreachable!()
                        }
                    };
                    self.compile_truthy_tagged_i32(&output.value, function)?;
                    flag.store(function);
                    output.value.set_boolean(flag, function);
                }
                DescriptorField::Get | DescriptorField::Set => {
                    output.value.tag().load(function);
                    function.instruction(&Instruction::I32Const(
                        WasmRuntimeValueTag::Undefined as i32,
                    ));
                    function.instruction(&Instruction::I32Ne);
                    self.emit_is_callable_i32(&output.value, function)?;
                    function.instruction(&Instruction::I32Eqz);
                    function.instruction(&Instruction::I32And);
                    self.open_frame(ControlFrameKind::If, function);
                    self.emit_throw_runtime_error(scriptw_ir::NativeErrorKind::TypeError,
                        RuntimeErrorMessage::PROPERTY_DESCRIPTOR_GETTER_SETTER_MUST_BE_CALLABLE_OR_UNDEFINED, &pending, function)?;
                    self.completion().copy_from(&pending, function);
                    self.emit_propagate_current_throw(function);
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                }
                DescriptorField::Value => {}
            }
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            key.clear(function);
        }
        converted.field(DescriptorField::Get).present.load(function);
        converted.field(DescriptorField::Set).present.load(function);
        function.instruction(&Instruction::I32Or);
        converted
            .field(DescriptorField::Value)
            .present
            .load(function);
        converted
            .field(DescriptorField::Writable)
            .present
            .load(function);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::PROPERTY_DESCRIPTOR_CANNOT_BE_BOTH_ACCESSOR_AND_DATA,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        Ok(converted)
    }

    fn emit_descriptor_conversion_abrupt_exit(
        &mut self,
        pending: &CompletionLocals,
        function: &mut Function,
    ) {
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn emit_from_present_property_descriptor(
        &mut self,
        descriptor: ReservedPropertyDescriptorLocals,
        result: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let object = schema.reserve_gc_local(function).initialize(
            self.emit_from_property_descriptor(
                DescriptorObjectPrototype::CurrentExecutionRealmObjectPrototype,
                &descriptor.object_fields(),
                function,
            )?,
            function,
        );
        result.set_reference(&object, schema, function);
        object.clear(function);
        descriptor.clear(schema, function);
        Ok(())
    }

    pub(crate) fn release_reserved_property_descriptor(
        &mut self,
        descriptor: ReservedPropertyDescriptorLocals,
        function: &mut Function,
    ) {
        descriptor.clear(self.runtime_schema(), function);
    }

    /// Generic completes to data; the side is derived from the same validated
    /// lattice used by definition and FromPropertyDescriptor.
    pub(crate) fn emit_complete_property_descriptor(
        &mut self,
        descriptor: ReservedPropertyDescriptorLocals,
        function: &mut Function,
    ) -> CompletedPropertyDescriptorLocals {
        let schema = self.runtime_schema();
        let accessor = schema.reserve_i32_local(function);
        let data = schema.reserve_i32_local(function);
        match classify(&descriptor.definition_descriptor()) {
            DescriptorClassification::Static(kind) => {
                function.instruction(&Instruction::I32Const(i32::from(
                    kind == PropertyDescriptorKind::Accessor,
                )));
            }
            DescriptorClassification::Dynamic { accessor_terms, .. } => {
                function.instruction(&Instruction::I32Const(i32::from(
                    accessor_terms.statically_true,
                )));
                for flag in accessor_terms.runtime_flags() {
                    flag.load(function);
                    function.instruction(&Instruction::I32Or);
                }
            }
        }
        accessor.store(function);
        accessor.load(function);
        function.instruction(&Instruction::I32Eqz);
        data.store(function);
        // Factory initialized absent values to Undefined and flags to false;
        // those roots are already the exact CompletePropertyDescriptor defaults.
        CompletedPropertyDescriptorLocals {
            fields: descriptor,
            accessor,
            data,
        }
    }

    pub(crate) fn release_completed_property_descriptor(
        &mut self,
        descriptor: CompletedPropertyDescriptorLocals,
        function: &mut Function,
    ) {
        descriptor.clear(self.runtime_schema(), function);
    }

    // Only the registered ObjectRead compiler in the private child module
    // enters this kernel; every product Get retains the typed call boundary.
    fn emit_object_read_kernel(
        &mut self,
        target: &ValueLocals,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let lookup = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let string_length =
            self.emit_primitive_string_length_read(target, key, result, function)?;
        string_length.load(function);
        self.emit_branch_if_to_target(exit, function);
        self.emit_value_to_object_locals(target, &pending, function)?;
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        lookup.copy_from(pending.value(), function);
        lookup.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::ProxyObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let proxy = schema.reserve_gc_local(function).initialize(
            lookup.cast_reference::<crate::gc_types::ProxyObject>(schema, function),
            function,
        );
        self.emit_load_live_proxy_slots(
            &proxy,
            ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler,
            result,
            function,
            |builder, slots, function| {
                builder.emit_proxy_get(slots, receiver, key, result, function)
            },
        )?;
        proxy.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_non_proxy_object_get(&lookup, receiver, key, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(string_length, function);
        pending.clear(function);
        lookup.clear(function);
        Ok(())
    }

    fn emit_non_proxy_object_get(
        &mut self,
        target: &ValueLocals,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let ordinary = self.emit_ordinary_own_data_read(target, key, result, function)?;
        ordinary.load(function);
        self.emit_branch_if_to_target(exit, function);
        let handled = self.emit_array_own_data_read(target, key, result, function)?;
        handled.load(function);
        self.emit_branch_if_to_target(exit, function);
        let descriptor = self.emit_proxy_target_own_descriptor(target, key, function)?;
        let value = schema.reserve_value_local(function);
        descriptor.emit_found_i32(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        descriptor.emit_accessor_i32(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        descriptor.read_getter(&value, schema, function);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.initialize(function);
        function.instruction(&Instruction::Else);
        let arguments = self.emit_pre_evaluated_arg_vector(&[], function);
        self.emit_function_handle_call_with_argv_inner(
            &value,
            Some(receiver),
            &arguments,
            result,
            PropagateCallThrow::LeaveInCompletion,
            function,
        )?;
        arguments.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        descriptor.read_value(&value, self, function);
        result.set_normal(&value, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        // Integer-indexed exotic [[Get]] never consults its prototype for a
        // canonical numeric key, including NaN, fractions and detached misses.
        let terminal = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        terminal.store(function);
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::TypedArrayObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        self.emit_canonical_numeric_property_key_i32(key, function)?;
        terminal.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        terminal.load(function);
        self.open_frame(ControlFrameKind::If, function);
        result.initialize(function);
        function.instruction(&Instruction::Else);
        self.emit_ordinary_get_prototype_of(target, &value, function);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.initialize(function);
        function.instruction(&Instruction::Else);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectReadArguments::new(
                    &value,
                    receiver,
                    key,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(terminal, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        value.clear(function);
        descriptor.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(handled, function);
        schema.release_i32_local(ordinary, function);
        Ok(())
    }

    pub(crate) fn emit_canonical_numeric_property_key_i32(
        &mut self,
        key: &PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let result = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        result.store(function);
        key.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let string = schema.reserve_gc_local(function).initialize(
            key.value()
                .cast_reference::<crate::gc_types::StringValue>(schema, function),
            function,
        );
        let minus_zero = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("-0", function)?,
            function,
        );
        self.emit_string_payload_equality_i32(&string, &minus_zero, function);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(1));
        result.store(function);
        function.instruction(&Instruction::Else);
        let number = schema.reserve_i64_local(function);
        let base = self.runtime_helper_base()?;
        schema
            .call_helper(
                crate::runtime_helpers::StringToNumberArguments::new(&string),
                base,
                function,
            )
            .store(number, function);
        let canonical_slot = schema.reserve_gc_local(function);
        let canonical = schema
            .call_helper(
                crate::runtime_helpers::NumberToStringArguments::new(number),
                base,
                function,
            )
            .bind(schema, canonical_slot, function);
        self.emit_string_payload_equality_i32(&string, &canonical, function);
        result.store(function);
        canonical.clear(function);
        schema.release_i64_local(number, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        minus_zero.clear(function);
        string.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.load(function);
        schema.release_i32_local(result, function);
        Ok(())
    }

    fn emit_proxy_get(
        &mut self,
        slots: &ProxySlotLocals,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let method = schema.reserve_value_local(function);
        let trap_result = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_proxy_named_method(slots, "get", &method, &pending, result, exit, function)?;
        self.compile_nullish_tagged_i32(method.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectReadArguments::new(
                    slots.target(),
                    receiver,
                    key,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_proxy_method_callable_check(
            &method,
            RuntimeErrorMessage::PROXY_GET_TRAP_IS_NOT_CALLABLE,
            result,
            exit,
            function,
        )?;
        let arguments =
            self.emit_pre_evaluated_arg_vector(&[slots.target(), key.value(), receiver], function);
        self.emit_function_handle_call_with_argv_inner(
            &method,
            Some(slots.handler()),
            &arguments,
            &pending,
            PropagateCallThrow::LeaveInCompletion,
            function,
        )?;
        arguments.clear(function);
        let normal = self.emit_normal_proxy_get_trap_result(
            PendingProxyGetTrapResultLocals {
                completion: &pending,
                value: &trap_result,
            },
            result,
            exit,
            function,
        );
        self.emit_proxy_get_invariant_check(slots.target(), key, normal, result, function)?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.clear(function);
        trap_result.clear(function);
        method.clear(function);
        Ok(())
    }

    fn emit_normal_proxy_get_trap_result<'value>(
        &mut self,
        pending: PendingProxyGetTrapResultLocals<'value>,
        result: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) -> NormalProxyGetTrapResultLocals<'value> {
        let PendingProxyGetTrapResultLocals { completion, value } = pending;
        self.emit_object_operation_abrupt_exit(completion, result, exit, function);
        value.copy_from(completion.value(), function);
        NormalProxyGetTrapResultLocals(value)
    }

    fn emit_proxy_get_invariant_check(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        trap_result: NormalProxyGetTrapResultLocals<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let descriptor = self.emit_direct_own_descriptor_for_proxy_get(target, key, function)?;
        let invariant_value = schema.reserve_value_local(function);
        result.set_normal(trap_result.value(), function);
        descriptor.emit_found_i32(schema, function);
        descriptor.emit_configurable_i32(schema, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        descriptor.emit_accessor_i32(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        descriptor.read_getter(&invariant_value, schema, function);
        invariant_value.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        trap_result.value().tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(
            RuntimeErrorMessage::PROXY_GET_TRAP_RETURNED_VALUE_FOR_ACCESSOR_WITHOUT_GETTER,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        descriptor.emit_writable_i32(schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        descriptor.read_value(&invariant_value, self, function);
        self.emit_tagged_payload_same_value_i32(trap_result.value(), &invariant_value, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(
            RuntimeErrorMessage::PROXY_GET_TRAP_RETURNED_INCONSISTENT_FROZEN_DATA_PROPERTY,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        invariant_value.clear(function);
        descriptor.clear(function);
        Ok(())
    }

    /// Set retains the raw Reference receiver while boxing only its lookup base.
    pub(crate) fn emit_object_write(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        strict: I32Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.runtime_schema()
            .call_helper(
                crate::runtime_helpers::ObjectWriteArguments::new(
                    target,
                    key,
                    value,
                    strict,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        Ok(())
    }

    /// The ObjectWrite body applies strict rejection around the shared [[Set]].
    pub(crate) fn compile_object_write_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::ObjectWrite);
        let parameters =
            self.helper_parameters::<crate::runtime_helpers::ObjectWriteParameters>(&mut function);
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_object_write_with_receiver(
            &parameters.target,
            &parameters.target,
            &parameters.key,
            &parameters.value,
            parameters.strict,
            &result,
            &mut function,
        )?;
        result.emit(&mut function);
        result.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_object_write_with_receiver(
        &mut self,
        target: &ValueLocals,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        strict: I32Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        self.emit_ordinary_set_result(target, receiver, key, value, &pending, function)?;
        result.copy_from(&pending, function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        function.instruction(&Instruction::I32Eqz);
        strict.load(function);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_ASSIGN_TO_PROPERTY,
            result,
            function,
        )?;
        function.instruction(&Instruction::Else);
        result.initialize(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        Ok(())
    }

    pub(crate) fn emit_object_write_strict(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let strict = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(1));
        strict.store(function);
        self.emit_object_write(target, key, value, strict, result, function)?;
        schema.release_i32_local(strict, function);
        Ok(())
    }

    /// Calls the shared [[Set]] dispatcher with its original receiver and
    /// whole completion. Only its registered compiler emits the physical body.
    pub(crate) fn emit_ordinary_set_result(
        &mut self,
        target: &ValueLocals,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.runtime_schema()
            .call_helper(
                crate::runtime_helpers::OrdinarySetArguments::new(
                    target,
                    receiver,
                    key,
                    value,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        Ok(())
    }

    pub(crate) fn compile_ordinary_set_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::OrdinarySet);
        let parameters =
            self.helper_parameters::<crate::runtime_helpers::OrdinarySetParameters>(&mut function);
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_ordinary_set_body(
            &parameters.target,
            &parameters.receiver,
            &parameters.key,
            &parameters.value,
            &result,
            &mut function,
        )?;
        result.emit(&mut function);
        result.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Prototype and Proxy recursion call the existing runtime declaration;
    /// they never re-enter this private physical emitter during compilation.
    fn emit_ordinary_set_body(
        &mut self,
        target: &ValueLocals,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let lookup = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_value_to_object_locals(target, &pending, function)?;
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        lookup.copy_from(pending.value(), function);
        lookup.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let proxy = schema.reserve_gc_local(function).initialize(
            lookup.cast_reference::<ProxyObject>(schema, function),
            function,
        );
        self.emit_load_live_proxy_slots(
            &proxy,
            ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler,
            result,
            function,
            |builder, slots, function| {
                builder.emit_proxy_set(slots, receiver, key, value, result, function)
            },
        )?;
        proxy.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_non_proxy_set(&lookup, receiver, key, value, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.clear(function);
        lookup.clear(function);
        Ok(())
    }

    fn emit_non_proxy_set(
        &mut self,
        target: &ValueLocals,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let boolean = schema.reserve_i32_local(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        // Both probes accept only an existing writable own data property on
        // the actual receiver. New properties, inherited setters, descriptors
        // and exotic receivers retain the general Set algorithm below.
        let compact =
            self.emit_compact_existing_data_write(target, receiver, key, value, function)?;
        compact.load(function);
        self.open_frame(ControlFrameKind::If, function);
        boolean.set_constant(1, function);
        self.emit_object_boolean_result(boolean, result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(compact, function);
        let packed =
            self.emit_packed_array_existing_data_write(target, receiver, key, value, function)?;
        packed.load(function);
        self.open_frame(ControlFrameKind::If, function);
        boolean.set_constant(1, function);
        self.emit_object_boolean_result(boolean, result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(packed, function);
        function.instruction(&Instruction::I32Const(0));
        boolean.store(function);
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::ModuleNamespaceObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(boolean, result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_typed_array_set_if_handled(target, receiver, key, value, result, function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let descriptor = self.emit_proxy_target_own_descriptor(target, key, function)?;
        descriptor.emit_found_i32(schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let prototype = schema.reserve_value_local(function);
        self.emit_ordinary_get_prototype_of(target, &prototype, function);
        prototype.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .call_helper(
                crate::runtime_helpers::OrdinarySetArguments::new(
                    &prototype,
                    receiver,
                    key,
                    value,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        function.instruction(&Instruction::Else);
        self.emit_ordinary_set_data_on_receiver_result(receiver, key, value, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        prototype.clear(function);
        function.instruction(&Instruction::Else);
        descriptor.emit_accessor_i32(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        let setter = schema.reserve_value_local(function);
        descriptor.read_setter(&setter, schema, function);
        setter.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(boolean, result, function);
        function.instruction(&Instruction::Else);
        let args = self.emit_pre_evaluated_arg_vector(&[value], function);
        self.emit_function_handle_call_with_argv_inner(
            &setter,
            Some(receiver),
            &args,
            result,
            PropagateCallThrow::LeaveInCompletion,
            function,
        )?;
        args.clear(function);
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(1));
        boolean.store(function);
        self.emit_object_boolean_result(boolean, result, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        setter.clear(function);
        function.instruction(&Instruction::Else);
        descriptor.emit_writable_i32(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_ordinary_set_data_on_receiver_result(receiver, key, value, result, function)?;
        function.instruction(&Instruction::Else);
        self.emit_object_boolean_result(boolean, result, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        descriptor.clear(function);
        schema.release_i32_local(boolean, function);
        Ok(())
    }

    /// Shares receiver-side [[GetOwnProperty]] and [[DefineOwnProperty]] with
    /// the original receiver, key and value rather than copying their kernels.
    fn emit_ordinary_set_data_on_receiver_result(
        &mut self,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.runtime_schema()
            .call_helper(
                crate::runtime_helpers::OrdinarySetDataOnReceiverArguments::new(
                    receiver,
                    key,
                    value,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        Ok(())
    }

    pub(crate) fn compile_ordinary_set_data_on_receiver_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::OrdinarySetDataOnReceiver);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::OrdinarySetDataOnReceiverParameters>(
                &mut function,
            );
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_ordinary_set_data_on_receiver_body(
            &parameters.receiver,
            &parameters.key,
            &parameters.value,
            &result,
            &mut function,
        )?;
        result.emit(&mut function);
        result.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// The receiver kernel retains Proxy and Array/Arguments exotic dispatch.
    fn emit_ordinary_set_data_on_receiver_body(
        &mut self,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let boolean = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        boolean.store(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_is_heap_object_like_tag_i32(receiver.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(boolean, result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let descriptor = self.emit_proxy_target_own_descriptor(receiver, key, function)?;
        descriptor.emit_found_i32(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        descriptor.emit_accessor_i32(schema, function);
        descriptor.emit_writable_i32(schema, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(boolean, result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_set_receiver_data_definition(
            receiver,
            key,
            SetReceiverDataDefinition::Update(value),
            result,
            function,
        )?;
        function.instruction(&Instruction::Else);
        self.emit_set_receiver_data_definition(
            receiver,
            key,
            SetReceiverDataDefinition::Create(value),
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        descriptor.clear(function);
        schema.release_i32_local(boolean, function);
        Ok(())
    }

    /// A concrete Array consumes normalized data fields directly. Other receiver
    /// kinds retain their existing ToPropertyDescriptor/native dispatch boundary.
    fn emit_set_receiver_data_definition(
        &mut self,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        definition: SetReceiverDataDefinition<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let incoming = definition.descriptor();
        let schema = self.runtime_schema();
        receiver.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::ArrayObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let array = schema.reserve_gc_local(function).initialize(
            receiver.cast_reference::<crate::gc_types::ArrayObject>(schema, function),
            function,
        );
        self.emit_array_define_own_property(&array, key, &incoming, result, function)?;
        array.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_define_own_property_from_fields(
            receiver,
            key,
            incoming.as_partial(),
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_define_own_property_from_fields(
        &mut self,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        fields: &DescriptorObjectFields<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let meta = self
            .functions
            .get(&StandardBuiltinId::ReflectDefineProperty.function_id())
            .cloned()
            .expect("Reflect.defineProperty dependency is rooted before emission");
        let schema = self.runtime_schema();
        let carrier = schema.reserve_gc_local(function).initialize(
            self.emit_from_property_descriptor(
                DescriptorObjectPrototype::PrivateCarrier,
                fields,
                function,
            )?,
            function,
        );
        let descriptor = schema.reserve_value_local(function);
        descriptor.set_reference(&carrier, schema, function);
        self.emit_direct_js_call(
            &meta,
            None,
            &[receiver, key.value(), &descriptor],
            result,
            function,
        )?;
        descriptor.clear(function);
        carrier.clear(function);
        Ok(())
    }

    fn emit_proxy_set(
        &mut self,
        slots: &ProxySlotLocals,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let method = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let boolean = schema.reserve_i32_local(function);
        result.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_proxy_named_method(slots, "set", &method, &pending, result, exit, function)?;
        self.compile_nullish_tagged_i32(method.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        schema
            .call_helper(
                crate::runtime_helpers::OrdinarySetArguments::new(
                    slots.target(),
                    receiver,
                    key,
                    value,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_proxy_method_callable_check(
            &method,
            RuntimeErrorMessage::PROXY_SET_TRAP_IS_NOT_CALLABLE,
            result,
            exit,
            function,
        )?;
        let args = self.emit_pre_evaluated_arg_vector(
            &[slots.target(), key.value(), value, receiver],
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
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        boolean.store(function);
        boolean.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_set_invariant_check(slots.target(), key, value, result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_object_boolean_result(boolean, result, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(boolean, function);
        pending.clear(function);
        method.clear(function);
        Ok(())
    }

    fn emit_integer_index_or_invalid(
        &self,
        number: crate::gc_types::I64Local,
        index: crate::gc_types::I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I64Const(-1));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        number.load(function);
        function.instruction(&Instruction::I64Const(i64::MIN));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::BrIf(0));
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Trunc);
        function.instruction(&Instruction::F64Ne);
        function.instruction(&Instruction::BrIf(0));
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::F64Lt);
        function.instruction(&Instruction::BrIf(0));
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(
            18_446_744_073_709_551_616.0,
        )));
        function.instruction(&Instruction::F64Ge);
        function.instruction(&Instruction::BrIf(0));
        number.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::I64TruncF64U);
        index.store(function);
        function.instruction(&Instruction::End);
    }

    fn emit_object_delete_kernel(
        &mut self,
        object: &ValueLocals,
        key: &PropertyKeyLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        object.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let proxy = schema.reserve_gc_local(function).initialize(
            object.cast_reference::<ProxyObject>(schema, function),
            function,
        );
        self.emit_load_live_proxy_slots(
            &proxy,
            ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler,
            result,
            function,
            |builder, slots, function| builder.emit_proxy_delete(slots, key, result, function),
        )?;
        proxy.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_delete_ordinary_by_tag(object, key, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_proxy_delete(
        &mut self,
        slots: &ProxySlotLocals,
        key: &PropertyKeyLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let method = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let boolean = schema.reserve_i32_local(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_proxy_named_method(
            slots,
            "deleteProperty",
            &method,
            &pending,
            result,
            exit,
            function,
        )?;
        self.compile_nullish_tagged_i32(method.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectDeleteArguments::new(
                    slots.target(),
                    key,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_proxy_method_callable_check(
            &method,
            RuntimeErrorMessage::PROXY_DELETEPROPERTY_TRAP_IS_NOT_CALLABLE,
            result,
            exit,
            function,
        )?;
        let args = self.emit_pre_evaluated_arg_vector(&[slots.target(), key.value()], function);
        self.emit_function_handle_call_with_argv_inner(
            &method,
            Some(slots.handler()),
            &args,
            &pending,
            PropagateCallThrow::LeaveInCompletion,
            function,
        )?;
        args.clear(function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        boolean.store(function);
        boolean.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(boolean, result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let descriptor = self.emit_proxy_target_descriptor_fact(slots.target(), key, function)?;
        descriptor.emit_found_i32(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        descriptor.emit_configurable_i32(schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(RuntimeErrorMessage::PROXY_DELETEPROPERTY_TRAP_RETURNED_TRUE_FOR_NON_CONFIGURABLE_TARGET_PROPERTY, result, function)?;
        function.instruction(&Instruction::Else);
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
        // Preserve the target operation's original Throw before inspecting its
        // Boolean. An absent descriptor skips this extensibility operation.
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(&pending, function);
        function.instruction(&Instruction::Else);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(boolean, result, function);
        function.instruction(&Instruction::Else);
        self.emit_proxy_execution_realm_type_error(RuntimeErrorMessage::PROXY_DELETEPROPERTY_TRAP_RETURNED_TRUE_FOR_NON_EXTENSIBLE_TARGET_PROPERTY, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        self.emit_object_boolean_result(boolean, result, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        descriptor.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(boolean, function);
        pending.clear(function);
        method.clear(function);
        Ok(())
    }

    /// Array indices are exactly "0" or a nonzero ASCII decimal string whose
    /// value is below 2^32-1. The at-most-ten-digit domain needs neither a
    /// floating-point parse nor Number-to-string allocation/roundtrip.
    pub(crate) fn emit_property_key_array_index(
        &mut self,
        key: &PropertyKeyLocals,
        index: I32Local,
        valid: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        valid.set_constant(0, function);
        key.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let string = schema.reserve_gc_local(function).initialize(
            key.value()
                .cast_reference::<crate::gc_types::StringValue>(schema, function),
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
        let length = schema.reserve_i32_local(function);
        let cursor = schema.reserve_i32_local(function);
        let digit = schema.reserve_i32_local(function);
        let number = schema.reserve_i64_local(function);
        schema
            .array_type::<crate::gc_types::CodeUnitArray>()
            .length(&units, schema, function);
        length.store(function);
        let finished = self.open_frame(ControlFrameKind::Block, function);
        length.load(function);
        function.instruction(&Instruction::I32Eqz);
        length.load(function);
        function.instruction(&Instruction::I32Const(10));
        function.instruction(&Instruction::I32GtU);
        function.instruction(&Instruction::I32Or);
        self.emit_branch_if_to_target(finished, function);
        cursor.set_constant(0, function);
        schema
            .array_type::<crate::gc_types::CodeUnitArray>()
            .read(&units, cursor, schema, function)
            .store(digit, function);
        digit.load(function);
        function.instruction(&Instruction::I32Const(48));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        // Only the one-unit spelling is canonical when the first digit is 0.
        index.set_constant(0, function);
        length.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Eq);
        valid.store(function);
        self.emit_branch_to_target(finished, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(0));
        number.store(function);
        let digits = self.open_frame(ControlFrameKind::Loop, function);
        schema
            .array_type::<crate::gc_types::CodeUnitArray>()
            .read(&units, cursor, schema, function)
            .store(digit, function);
        digit.load(function);
        function.instruction(&Instruction::I32Const(48));
        function.instruction(&Instruction::I32Sub);
        digit.store(function);
        digit.load(function);
        function.instruction(&Instruction::I32Const(9));
        function.instruction(&Instruction::I32GtU);
        self.emit_branch_if_to_target(finished, function);
        // Ten decimal digits fit in u64, so no intermediate can overflow.
        number.load(function);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64Mul);
        digit.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Add);
        number.store(function);
        cursor.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        cursor.store(function);
        cursor.load(function);
        length.load(function);
        function.instruction(&Instruction::I32LtU);
        self.emit_branch_if_to_target(digits, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        number.load(function);
        function.instruction(&Instruction::I64Const(u32::MAX as i64));
        function.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, function);
        number.load(function);
        function.instruction(&Instruction::I32WrapI64);
        index.store(function);
        valid.set_constant(1, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i64_local(number, function);
        for local in [digit, cursor, length] {
            schema.release_i32_local(local, function);
        }
        units.clear(function);
        string.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_delete_ordinary_by_tag(
        &mut self,
        object: &ValueLocals,
        key: &PropertyKeyLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let boolean = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(1));
        boolean.store(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_is_module_namespace_i32(object, function);
        self.open_frame(ControlFrameKind::If, function);
        let namespace = schema.reserve_gc_local(function).initialize(
            object.cast_reference::<crate::gc_types::ModuleNamespaceObject>(schema, function),
            function,
        );
        let present = schema.reserve_i32_local(function);
        let pending = schema.reserve_completion(function);
        self.emit_namespace_property(
            &namespace,
            key,
            NamespaceBindingRead::Presence,
            present,
            &pending,
            function,
        )?;
        present.load(function);
        function.instruction(&Instruction::I32Eqz);
        boolean.store(function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(&pending, function);
        function.instruction(&Instruction::Else);
        self.emit_object_boolean_result(boolean, result, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        schema.release_i32_local(present, function);
        namespace.clear(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        object.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::TypedArrayObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let typed = schema.reserve_gc_local(function).initialize(
            object.cast_reference::<crate::gc_types::TypedArrayObject>(schema, function),
            function,
        );
        let number = schema.reserve_i64_local(function);
        let integer = schema.reserve_i64_local(function);
        let handled = schema.reserve_i32_local(function);
        let valid = schema.reserve_i32_local(function);
        self.emit_typed_array_canonical_numeric_index_i32(object, key, number, handled, function)?;
        handled.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_typed_array_valid_integer_index_i32(&typed, number, integer, valid, function)?;
        valid.load(function);
        function.instruction(&Instruction::I32Eqz);
        boolean.store(function);
        self.emit_object_boolean_result(boolean, result, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        handled.load(function);
        schema.release_i32_local(valid, function);
        schema.release_i32_local(handled, function);
        schema.release_i64_local(integer, function);
        schema.release_i64_local(number, function);
        typed.clear(function);
        // The decision is retained on the stack while its private roots retire.
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // Packed Number entries are always configurable. Avoid asking for a
        // mutable descriptor (and promoting the backing) merely to delete one.
        let packed = self.emit_packed_array_delete_property(object, key, function)?;
        packed.load(function);
        self.open_frame(ControlFrameKind::If, function);
        boolean.set_constant(1, function);
        self.emit_object_boolean_result(boolean, result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(packed, function);
        let descriptor = self.emit_direct_own_descriptor_fact(object, key, function)?;
        descriptor.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(boolean, result, function);
        function.instruction(&Instruction::Else);
        let flags = schema.reserve_i32_local(function);
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&descriptor, schema, function)
            .store(flags, function);
        flags.load(function);
        function.instruction(&Instruction::I32Const(
            DescriptorMask::CONFIGURABLE.as_i32(),
        ));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(0));
        boolean.store(function);
        function.instruction(&Instruction::Else);
        let index = schema.reserve_i32_local(function);
        let indexed = schema.reserve_i32_local(function);
        self.emit_property_key_array_index(key, index, indexed, function)?;
        object.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Array as i32));
        function.instruction(&Instruction::I32Eq);
        indexed.load(function);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        let array = schema.reserve_gc_local(function).initialize(
            object.cast_reference::<crate::gc_types::ArrayObject>(schema, function),
            function,
        );
        let index64 = schema.reserve_i64_local(function);
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        index64.store(function);
        self.emit_array_indexed_delete(&array, index64, function)?;
        schema.release_i64_local(index64, function);
        array.clear(function);
        function.instruction(&Instruction::Else);
        object.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Arguments as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        indexed.load(function);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_arguments_delete_index(object, index, function);
        function.instruction(&Instruction::Else);
        self.emit_object_delete_ordinary(object, key, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(indexed, function);
        schema.release_i32_local(index, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_object_boolean_result(boolean, result, function);
        schema.release_i32_local(flags, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        descriptor.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(boolean, function);
        Ok(())
    }

    fn emit_object_delete_ordinary(
        &mut self,
        object: &ValueLocals,
        key: &PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_object_header_projection(object, function),
            function,
        );
        let candidate = self.emit_ordinary_property_entry(&header, key, function)?;
        candidate.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let entry = schema.reserve_gc_local(function).initialize(
            candidate.load(schema, function).require_non_null(function),
            function,
        );
        let storage = self.emit_dictionary_property_storage(&header, function);
        let table = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryPropertyStorageSchema::ENTRIES)
                .read(&storage, schema, function)
                .reference(),
            function,
        );
        let position = schema.reserve_i32_local(function);
        schema
            .field(PropertyEntrySchema::POSITION)
            .read(&entry, schema, function)
            .store(position, function);
        // Retain the bucket as a tombstone: colliding keys still probe past it.
        schema.array_type::<PropertyTable>().write(
            &table,
            position,
            GcOperand::null(schema),
            schema,
            function,
        );
        schema.release_i32_local(position, function);
        table.clear(function);
        storage.clear(function);
        entry.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        candidate.clear(function);
        header.clear(function);
        Ok(())
    }

    pub(crate) fn emit_proxy_set_invariant_check(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        incoming: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let descriptor = self.emit_direct_own_descriptor_for_proxy_set(target, key, function)?;
        let field = schema.reserve_value_local(function);
        descriptor.emit_found_i32(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        descriptor.emit_configurable_i32(schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        descriptor.emit_accessor_i32(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        descriptor.read_setter(&field, schema, function);
        field.tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(
            RuntimeErrorMessage::PROXY_SET_TRAP_RESULT_IS_INCOMPATIBLE_WITH_TARGET_DESCRIPTOR,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        descriptor.emit_writable_i32(schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        descriptor.read_value(&field, self, function);
        self.emit_tagged_payload_same_value_i32(incoming, &field, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(
            RuntimeErrorMessage::PROXY_SET_TRAP_RESULT_IS_INCOMPATIBLE_WITH_TARGET_DESCRIPTOR,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        field.clear(function);
        descriptor.clear(function);
        Ok(())
    }

    fn emit_object_get_prototype_of_kernel(
        &mut self,
        object: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        object.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::ProxyObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let proxy_slot = schema.reserve_gc_local(function);
        let proxy = proxy_slot.initialize(
            object.cast_reference::<crate::gc_types::ProxyObject>(schema, function),
            function,
        );
        self.emit_load_live_proxy_slots(
            &proxy,
            ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler,
            result,
            function,
            |builder, slots, function| builder.emit_proxy_get_prototype_of(slots, result, function),
        )?;
        proxy.clear(function);
        function.instruction(&Instruction::Else);
        let prototype = schema.reserve_value_local(function);
        self.emit_ordinary_get_prototype_of(object, &prototype, function);
        result.set_normal(&prototype, function);
        prototype.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_proxy_get_prototype_of(
        &mut self,
        slots: &ProxySlotLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let method = schema.reserve_value_local(function);
        let trap_result = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        method.set_undefined(function);
        trap_result.set_undefined(function);
        pending.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_proxy_named_method(
            slots,
            "getPrototypeOf",
            &method,
            &pending,
            result,
            exit,
            function,
        )?;
        self.compile_nullish_tagged_i32(method.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        let base = self.runtime_helper_base()?;
        schema
            .call_helper(
                crate::runtime_helpers::ObjectGetPrototypeOfArguments::new(
                    slots.target(),
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_proxy_method_callable_check(
            &method,
            RuntimeErrorMessage::PROXY_GETPROTOTYPEOF_TRAP_IS_NOT_CALLABLE,
            result,
            exit,
            function,
        )?;
        let arguments = self.emit_pre_evaluated_arg_vector(&[slots.target()], function);
        self.emit_function_handle_call_with_argv_inner(
            &method,
            Some(slots.handler()),
            &arguments,
            &pending,
            PropagateCallThrow::LeaveInCompletion,
            function,
        )?;
        arguments.clear(function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        trap_result.copy_from(pending.value(), function);
        self.emit_is_heap_object_like_tag_i32(trap_result.tag(), function);
        trap_result.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(
            RuntimeErrorMessage::PROXY_GETPROTOTYPEOF_TRAP_RESULT_MUST_BE_OBJECT_OR_NULL,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectIsExtensibleArguments::new(
                    slots.target(),
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(&pending, function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        result.set_normal(&trap_result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectGetPrototypeOfArguments::new(
                    slots.target(),
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(&pending, function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        self.emit_tagged_payload_same_value_i32(&trap_result, pending.value(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(
            RuntimeErrorMessage::PROXY_GETPROTOTYPEOF_TRAP_RESULT_DOES_NOT_MATCH_TARGET,
            result,
            function,
        )?;
        function.instruction(&Instruction::Else);
        result.set_normal(&trap_result, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.clear(function);
        trap_result.clear(function);
        method.clear(function);
        Ok(())
    }

    fn emit_proxy_named_method(
        &mut self,
        slots: &ProxySlotLocals,
        name: &str,
        method: &ValueLocals,
        pending: &CompletionLocals,
        result: &CompletionLocals,
        exit: crate::emit::ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let name_slot = schema.reserve_gc_local(function);
        let name = name_slot.initialize(
            self.emit_interned_string_reference(name, function)?,
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &name, function);
        name.clear(function);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectReadArguments::new(
                    slots.handler(),
                    slots.handler(),
                    &key,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(pending, function);
        key.clear(function);
        self.emit_object_operation_abrupt_exit(pending, result, exit, function);
        method.copy_from(pending.value(), function);
        Ok(())
    }

    fn emit_proxy_method_callable_check(
        &mut self,
        method: &ValueLocals,
        message: RuntimeErrorMessage,
        result: &CompletionLocals,
        exit: crate::emit::ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_is_callable_i32(method, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(message, result, function)?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_object_operation_abrupt_exit(
        &mut self,
        pending: &CompletionLocals,
        result: &CompletionLocals,
        exit: crate::emit::ControlTarget,
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

    fn emit_object_boolean_result(
        &self,
        boolean: crate::gc_types::I32Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) {
        let value = self.runtime_schema().reserve_value_local(function);
        value.set_boolean(boolean, function);
        result.set_normal(&value, function);
        value.clear(function);
    }

    pub(crate) fn emit_ordinary_get_prototype_of(
        &self,
        object: &ValueLocals,
        result: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let header_slot = schema.reserve_gc_local(function);
        let header = header_slot.initialize(
            self.emit_object_header_projection(object, function),
            function,
        );
        let prototype_slot = schema.reserve_gc_local(function);
        let prototype = prototype_slot.initialize(
            schema
                .struct_type::<OrdinaryObject>()
                .field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, schema, function)
                .reference(),
            function,
        );
        schema.prototype_reference_into(&prototype, result, function);
        prototype.clear(function);
        header.clear(function);
    }

    fn emit_object_set_prototype_of_kernel(
        &mut self,
        object: &ValueLocals,
        prototype: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        object.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let proxy = schema.reserve_gc_local(function).initialize(
            object.cast_reference::<ProxyObject>(schema, function),
            function,
        );
        self.emit_load_live_proxy_slots(
            &proxy,
            ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler,
            result,
            function,
            |builder, slots, function| {
                builder.emit_proxy_set_prototype_of(slots, prototype, result, function)
            },
        )?;
        proxy.clear(function);
        function.instruction(&Instruction::Else);
        let boolean = schema.reserve_i32_local(function);
        self.emit_ordinary_set_prototype_of_i32(object, prototype, boolean, function)?;
        self.emit_object_boolean_result(boolean, result, function);
        schema.release_i32_local(boolean, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_proxy_set_prototype_of(
        &mut self,
        slots: &ProxySlotLocals,
        prototype: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let method = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let boolean = schema.reserve_i32_local(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_proxy_named_method(
            slots,
            "setPrototypeOf",
            &method,
            &pending,
            result,
            exit,
            function,
        )?;
        self.compile_nullish_tagged_i32(method.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        // Proxy targets may be Proxies too. Emit the recursive internal
        // operation as a runtime call rather than expanding its Rust emitter.
        schema
            .call_helper(
                crate::runtime_helpers::ObjectSetPrototypeOfArguments::new(
                    slots.target(),
                    prototype,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_proxy_method_callable_check(
            &method,
            RuntimeErrorMessage::PROXY_SETPROTOTYPEOF_TRAP_IS_NOT_CALLABLE,
            result,
            exit,
            function,
        )?;
        let args = self.emit_pre_evaluated_arg_vector(&[slots.target(), prototype], function);
        self.emit_function_handle_call_with_argv_inner(
            &method,
            Some(slots.handler()),
            &args,
            &pending,
            PropagateCallThrow::LeaveInCompletion,
            function,
        )?;
        args.clear(function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        boolean.store(function);
        boolean.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(boolean, result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let base = self.runtime_helper_base()?;
        schema
            .call_helper(
                crate::runtime_helpers::ObjectIsExtensibleArguments::new(
                    slots.target(),
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(&pending, function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(boolean, result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectGetPrototypeOfArguments::new(
                    slots.target(),
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(&pending, function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        self.emit_tagged_payload_same_value_i32(prototype, pending.value(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(RuntimeErrorMessage::PROXY_SETPROTOTYPEOF_TRAP_RESULT_INCOMPATIBLE_WITH_NON_EXTENSIBLE_TARGET, result, function)?;
        function.instruction(&Instruction::Else);
        self.emit_object_boolean_result(boolean, result, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(boolean, function);
        pending.clear(function);
        method.clear(function);
        Ok(())
    }

    pub(crate) fn emit_ordinary_set_prototype_of_i32(
        &mut self,
        object: &ValueLocals,
        prototype: &ValueLocals,
        result: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let current = schema.reserve_value_local(function);
        let immutable = schema.reserve_i32_local(function);
        let extensible = schema.reserve_i32_local(function);
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_object_header_projection(object, function),
            function,
        );
        function.instruction(&Instruction::I32Const(0));
        result.store(function);
        self.emit_ordinary_get_prototype_of(object, &current, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_tagged_payload_same_value_i32(&current, prototype, function)?;
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(1));
        result.store(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .struct_type::<OrdinaryObject>()
            .field(OrdinaryObjectSchema::IMMUTABLE_PROTOTYPE)
            .read(&header, schema, function)
            .store(immutable, function);
        immutable.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .struct_type::<OrdinaryObject>()
            .field(OrdinaryObjectSchema::EXTENSIBLE)
            .read(&header, schema, function)
            .store(extensible, function);
        extensible.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        current.copy_from(prototype, function);
        let cycle_exit = self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        current.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(cycle_exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_tagged_payload_same_value_i32(&current, object, function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // A Proxy owns a different [[GetPrototypeOf]] method. The ordinary
        // cycle walk stops there and never invokes that object's trap.
        current.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(cycle_exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_ordinary_get_prototype_of(&current, &current, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let stored = schema
            .reserve_gc_local(function)
            .initialize(schema.prototype_reference(prototype, function), function);
        schema
            .struct_type::<OrdinaryObject>()
            .field(OrdinaryObjectSchema::PROTOTYPE)
            .write(
                &header,
                GcOperand::reference(&stored, schema),
                schema,
                function,
            );
        stored.clear(function);
        function.instruction(&Instruction::I32Const(1));
        result.store(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        header.clear(function);
        schema.release_i32_local(extensible, function);
        schema.release_i32_local(immutable, function);
        current.clear(function);
        Ok(())
    }

    fn emit_object_prevent_extensions_kernel(
        &mut self,
        object: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        object.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::ProxyObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let proxy_slot = schema.reserve_gc_local(function);
        let proxy = proxy_slot.initialize(
            object.cast_reference::<crate::gc_types::ProxyObject>(schema, function),
            function,
        );
        self.emit_load_live_proxy_slots(
            &proxy,
            ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler,
            result,
            function,
            |builder, slots, function| {
                builder.emit_proxy_extensibility_operation(
                    slots,
                    ProxyExtensibilityOperation::PreventExtensions,
                    result,
                    function,
                )
            },
        )?;
        proxy.clear(function);
        function.instruction(&Instruction::Else);
        let header_slot = schema.reserve_gc_local(function);
        let header = header_slot.initialize(
            self.emit_object_header_projection(object, function),
            function,
        );
        let boolean = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(1));
        boolean.store(function);
        object.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::TypedArrayObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let array = schema.reserve_gc_local(function).initialize(
            object.cast_reference::<crate::gc_types::TypedArrayObject>(schema, function),
            function,
        );
        self.emit_typed_array_fixed_length_i32(&array, function);
        boolean.store(function);
        array.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        boolean.load(function);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .struct_type::<OrdinaryObject>()
            .field(OrdinaryObjectSchema::EXTENSIBLE)
            .write(&header, GcOperand::boolean(false), schema, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_object_boolean_result(boolean, result, function);
        schema.release_i32_local(boolean, function);
        header.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_object_is_extensible_kernel(
        &mut self,
        object: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        object.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::ProxyObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let proxy_slot = schema.reserve_gc_local(function);
        let proxy = proxy_slot.initialize(
            object.cast_reference::<crate::gc_types::ProxyObject>(schema, function),
            function,
        );
        self.emit_load_live_proxy_slots(
            &proxy,
            ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler,
            result,
            function,
            |builder, slots, function| {
                builder.emit_proxy_extensibility_operation(
                    slots,
                    ProxyExtensibilityOperation::IsExtensible,
                    result,
                    function,
                )
            },
        )?;
        proxy.clear(function);
        function.instruction(&Instruction::Else);
        let header_slot = schema.reserve_gc_local(function);
        let header = header_slot.initialize(
            self.emit_object_header_projection(object, function),
            function,
        );
        let boolean = schema.reserve_i32_local(function);
        schema
            .struct_type::<OrdinaryObject>()
            .field(OrdinaryObjectSchema::EXTENSIBLE)
            .read(&header, schema, function)
            .store(boolean, function);
        self.emit_object_boolean_result(boolean, result, function);
        schema.release_i32_local(boolean, function);
        header.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_proxy_extensibility_operation(
        &mut self,
        slots: &ProxySlotLocals,
        operation: ProxyExtensibilityOperation,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let method = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let boolean = schema.reserve_i32_local(function);
        method.set_undefined(function);
        pending.initialize(function);
        function.instruction(&Instruction::I32Const(0));
        boolean.store(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let (name, noncallable) = match operation {
            ProxyExtensibilityOperation::IsExtensible => (
                "isExtensible",
                RuntimeErrorMessage::PROXY_ISEXTENSIBLE_TRAP_IS_NOT_CALLABLE,
            ),
            ProxyExtensibilityOperation::PreventExtensions => (
                "preventExtensions",
                RuntimeErrorMessage::PROXY_PREVENTEXTENSIONS_TRAP_IS_NOT_CALLABLE,
            ),
        };
        self.emit_proxy_named_method(slots, name, &method, &pending, result, exit, function)?;
        self.compile_nullish_tagged_i32(method.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        let base = self.runtime_helper_base()?;
        match operation {
            ProxyExtensibilityOperation::IsExtensible => schema
                .call_helper(
                    crate::runtime_helpers::ObjectIsExtensibleArguments::new(
                        slots.target(),
                        self.current_environment(),
                        self.current_program(),
                    ),
                    base,
                    function,
                )
                .store(result, function),
            ProxyExtensibilityOperation::PreventExtensions => schema
                .call_helper(
                    crate::runtime_helpers::ObjectPreventExtensionsArguments::new(
                        slots.target(),
                        self.current_environment(),
                        self.current_program(),
                    ),
                    base,
                    function,
                )
                .store(result, function),
        }
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_proxy_method_callable_check(&method, noncallable, result, exit, function)?;
        let arguments = self.emit_pre_evaluated_arg_vector(&[slots.target()], function);
        self.emit_function_handle_call_with_argv_inner(
            &method,
            Some(slots.handler()),
            &arguments,
            &pending,
            PropagateCallThrow::LeaveInCompletion,
            function,
        )?;
        arguments.clear(function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        boolean.store(function);
        if matches!(operation, ProxyExtensibilityOperation::PreventExtensions) {
            boolean.load(function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_object_boolean_result(boolean, result, function);
            self.emit_branch_to_target(exit, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        schema
            .call_helper(
                crate::runtime_helpers::ObjectIsExtensibleArguments::new(
                    slots.target(),
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(&pending, function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        match operation {
            ProxyExtensibilityOperation::IsExtensible => {
                boolean.load(function);
                function.instruction(&Instruction::I32Ne);
            }
            ProxyExtensibilityOperation::PreventExtensions => {}
        }
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(match operation {
            ProxyExtensibilityOperation::IsExtensible => RuntimeErrorMessage::PROXY_ISEXTENSIBLE_TRAP_RESULT_DOES_NOT_MATCH_TARGET,
            ProxyExtensibilityOperation::PreventExtensions => RuntimeErrorMessage::PROXY_PREVENTEXTENSIONS_TRAP_RETURNED_TRUE_FOR_EXTENSIBLE_TARGET,
        }, result, function)?;
        function.instruction(&Instruction::Else);
        self.emit_object_boolean_result(boolean, result, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(boolean, function);
        pending.clear(function);
        method.clear(function);
        Ok(())
    }

    fn emit_typed_array_canonical_numeric_index_i32(
        &mut self,
        object: &ValueLocals,
        key: &PropertyKeyLocals,
        number: I64Local,
        handled: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        function.instruction(&Instruction::I32Const(0));
        handled.store(function);
        object.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::TypedArrayObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        key.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        let string = schema.reserve_gc_local(function).initialize(
            key.value()
                .cast_reference::<crate::gc_types::StringValue>(schema, function),
            function,
        );
        self.emit_canonical_numeric_index_string(&string, number, handled, function)?;
        string.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// Exception metadata reads observe only ordinary stored data. They stop
    /// at an accessor or Proxy and never modify the original completion.
    pub(crate) fn emit_data_property_read_no_call(
        &mut self,
        object: &ValueLocals,
        key: &PropertyKeyLocals,
        result: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let current = schema.reserve_value_local(function);
        current.copy_from(object, function);
        result.set_undefined(function);
        let done = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        done.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        self.emit_is_heap_object_like_tag_i32(current.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        current.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::ProxyObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        function.instruction(&Instruction::BrIf(1));
        let header_slot = schema.reserve_gc_local(function);
        let header = header_slot.initialize(
            self.emit_object_header_projection(&current, function),
            function,
        );
        let descriptor = self.emit_ordinary_own_descriptor_reference(&header, key, function)?;
        descriptor.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::If(BlockType::Empty));
        let prototype_slot = schema.reserve_gc_local(function);
        let prototype = prototype_slot.initialize(
            schema
                .struct_type::<OrdinaryObject>()
                .field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, schema, function)
                .reference(),
            function,
        );
        schema.prototype_reference_into(&prototype, &current, function);
        prototype.clear(function);
        function.instruction(&Instruction::Else);
        let present_slot = schema.reserve_gc_local(function);
        let present = present_slot.initialize(
            descriptor.load(schema, function).require_non_null(function),
            function,
        );
        let flags = schema.reserve_i32_local(function);
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&present, schema, function)
            .store(flags, function);
        flags.load(function);
        function.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let value_slot = schema.reserve_gc_local(function);
        let value = value_slot.initialize(
            schema
                .struct_type::<PropertyDescriptor>()
                .field(PropertyDescriptorSchema::VALUE)
                .read(&present, schema, function)
                .reference(),
            function,
        );
        self.emit_property_value_into(&value, result, function);
        value.clear(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I32Const(1));
        done.store(function);
        schema.release_i32_local(flags, function);
        present.clear(function);
        function.instruction(&Instruction::End);
        descriptor.clear(function);
        header.clear(function);
        done.load(function);
        function.instruction(&Instruction::BrIf(1));
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i32_local(done, function);
        current.clear(function);
        Ok(())
    }
}
