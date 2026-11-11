//! FromPropertyDescriptor is the sole fresh descriptor-object publisher.
//! Each selected field is a writable, enumerable, configurable data property.

use super::*;

#[derive(Debug, Clone, Copy)]
pub(crate) enum DescriptorFlag {
    Known(bool),
    BooleanPayload(I32Local),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DescriptorObjectLocals<'v>(core::marker::PhantomData<&'v ValueLocals>);
impl<'v> DescriptorCarrier for DescriptorObjectLocals<'v> {
    type Value = &'v ValueLocals;
    type Flag = DescriptorFlag;
    type RuntimeFlag = I32Local;
}
pub(crate) type DescriptorObjectFields<'v> = PartialDescriptor<DescriptorObjectLocals<'v>>;

pub(crate) enum DescriptorObjectPrototype<'v> {
    CurrentExecutionRealmObjectPrototype,
    ObjectPrototypeLocal(&'v ValueLocals),
    PrivateCarrier,
}

#[derive(Clone, Copy)]
enum DescriptorObjectFieldValue<'v> {
    Value(&'v ValueLocals),
    Flag(DescriptorFlag),
}
fn field_presence<'v, T: Copy + 'v>(
    value: &Presence<T, I32Local>,
    wrap: fn(T) -> DescriptorObjectFieldValue<'v>,
) -> Presence<DescriptorObjectFieldValue<'v>, I32Local> {
    match value {
        Presence::Absent => Presence::Absent,
        Presence::Present(value) => Presence::Present(wrap(*value)),
        Presence::Runtime { present, value } => Presence::Runtime {
            present: *present,
            value: wrap(*value),
        },
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_from_property_descriptor(
        &mut self,
        prototype: DescriptorObjectPrototype<'_>,
        fields: &DescriptorObjectFields<'_>,
        function: &mut Function,
    ) -> Result<GcStackReference<OrdinaryObject>, EmitError> {
        let schema = self.runtime_schema();
        let object = match prototype {
            DescriptorObjectPrototype::CurrentExecutionRealmObjectPrototype => {
                let realm = self.load_current_realm(function);
                let prototype = schema.reserve_value_local(function);
                self.emit_load_non_array_realm_intrinsic(
                    &realm,
                    crate::functions::NonArrayRealmIntrinsicSlot::ObjectPrototype,
                    &prototype,
                    function,
                );
                let object = schema.reserve_gc_local(function).initialize(
                    self.emit_alloc_plain_object_with_prototype(Some(&prototype), function)?,
                    function,
                );
                prototype.clear(function);
                realm.clear(function);
                object
            }
            DescriptorObjectPrototype::ObjectPrototypeLocal(prototype) => {
                schema.reserve_gc_local(function).initialize(
                    self.emit_alloc_plain_object_with_prototype(Some(prototype), function)?,
                    function,
                )
            }
            DescriptorObjectPrototype::PrivateCarrier => {
                schema.reserve_gc_local(function).initialize(
                    self.emit_alloc_plain_object_with_prototype(None, function)?,
                    function,
                )
            }
        };
        for field in DescriptorField::ALL {
            let presence = match field {
                DescriptorField::Value => {
                    field_presence(&fields.value, DescriptorObjectFieldValue::Value)
                }
                DescriptorField::Writable => {
                    field_presence(&fields.writable, DescriptorObjectFieldValue::Flag)
                }
                DescriptorField::Get => {
                    field_presence(&fields.get, DescriptorObjectFieldValue::Value)
                }
                DescriptorField::Set => {
                    field_presence(&fields.set, DescriptorObjectFieldValue::Value)
                }
                DescriptorField::Enumerable => {
                    field_presence(&fields.enumerable, DescriptorObjectFieldValue::Flag)
                }
                DescriptorField::Configurable => {
                    field_presence(&fields.configurable, DescriptorObjectFieldValue::Flag)
                }
            };
            match presence {
                Presence::Absent => {}
                Presence::Present(value) => {
                    self.emit_descriptor_object_field(&object, field, value, function)?
                }
                Presence::Runtime { present, value } => {
                    present.load(function);
                    self.open_frame(ControlFrameKind::If, function);
                    self.emit_descriptor_object_field(&object, field, value, function)?;
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                }
            }
        }
        let result = object.load(schema, function);
        object.clear(function);
        Ok(result)
    }

    fn emit_descriptor_object_field(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        field: DescriptorField,
        value: DescriptorObjectFieldValue<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(field.key(), function)?,
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &string, function);
        string.clear(function);
        let boolean = schema.reserve_value_local(function);
        let value = match value {
            DescriptorObjectFieldValue::Value(value) => value,
            DescriptorObjectFieldValue::Flag(flag) => {
                match flag {
                    DescriptorFlag::Known(value) => {
                        boolean.set_scalar(ScalarValue::Boolean(value), function)
                    }
                    DescriptorFlag::BooleanPayload(value) => boolean.set_boolean(value, function),
                }
                &boolean
            }
        };
        self.emit_object_append_data_property_with_flags(
            object, &key, value, true, true, true, function,
        )?;
        boolean.clear(function);
        key.clear(function);
        Ok(())
    }
}
