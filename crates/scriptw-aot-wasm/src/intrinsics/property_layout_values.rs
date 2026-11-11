//! Value recipes capture the defining Realm only when their identity is needed.
use super::*;
use crate::gc_types::*;
use crate::intrinsics::property_layouts::{
    IntrinsicCallableRecipeKind, IntrinsicPropertyLayoutId, IntrinsicPropertyRecipeKind,
};
use crate::runtime_helpers::RuntimeHelperPropertyLayoutRow;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_intrinsic_layout_recipe_value(
        &mut self,
        row: &RuntimeHelperPropertyLayoutRow,
        realm: &GcLocal<RealmRecord>,
        setter: bool,
        f: &mut Function,
    ) -> Result<GcLocal<PropertyValue>, EmitError> {
        let s = self.runtime_schema();
        let output = s
            .reserve_gc_local::<PropertyValue, Nullable>(f)
            .initialize_null(s, f);
        if setter {
            row.kind().load(f);
            f.instruction(&Instruction::I32Const(
                IntrinsicPropertyRecipeKind::Accessor as i32,
            ));
            f.instruction(&Instruction::I32Ne);
            self.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::Unreachable);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        for kind in IntrinsicPropertyRecipeKind::ALL {
            if setter && !matches!(kind, IntrinsicPropertyRecipeKind::Accessor) {
                continue;
            }
            row.kind().load(f);
            f.instruction(&Instruction::I32Const(kind as i32));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            match kind {
                IntrinsicPropertyRecipeKind::Method => {
                    let recipe = self.emit_layout_method_recipe(row, false, realm, f);
                    output.replace(recipe.load(s, f).nullable(), f);
                    recipe.clear(f);
                }
                IntrinsicPropertyRecipeKind::Accessor => {
                    for side_kind in IntrinsicCallableRecipeKind::ALL {
                        row.emit_callable_kind(setter, f);
                        f.instruction(&Instruction::I32Const(side_kind as i32));
                        f.instruction(&Instruction::I32Eq);
                        self.open_frame(ControlFrameKind::If, f);
                        match side_kind {
                            IntrinsicCallableRecipeKind::Undefined => {
                                let value =
                                    s.reserve_gc_local(f).initialize(s.stored_undefined(f), f);
                                output.replace(value.load(s, f).upcast().nullable(), f);
                                value.clear(f);
                            }
                            IntrinsicCallableRecipeKind::Method => {
                                let recipe = self.emit_layout_method_recipe(row, setter, realm, f);
                                output.replace(recipe.load(s, f).nullable(), f);
                                recipe.clear(f);
                            }
                            IntrinsicCallableRecipeKind::Ready => {
                                let mut index = row.payload0();
                                if setter {
                                    index = row.payload1();
                                }
                                let value = self.emit_layout_ready_slot(realm, index, true, f);
                                output.replace(value.load(s, f).upcast().nullable(), f);
                                value.clear(f);
                            }
                        }
                        self.pop_control(ControlFrameKind::If);
                        f.instruction(&Instruction::End);
                    }
                }
                IntrinsicPropertyRecipeKind::ReadyRealmSlot => {
                    let value = self.emit_layout_ready_slot(realm, row.payload0(), false, f);
                    output.replace(value.load(s, f).upcast().nullable(), f);
                    value.clear(f);
                }
                IntrinsicPropertyRecipeKind::Number
                | IntrinsicPropertyRecipeKind::Boolean
                | IntrinsicPropertyRecipeKind::String
                | IntrinsicPropertyRecipeKind::LayoutObject
                | IntrinsicPropertyRecipeKind::ArrayPrototype
                | IntrinsicPropertyRecipeKind::WellKnownSymbol => {
                    let value = s.reserve_value_local(f);
                    match kind {
                        IntrinsicPropertyRecipeKind::Number => {
                            let bits = s.reserve_i64_local(f);
                            row.payload0().load(f);
                            f.instruction(&Instruction::I64ExtendI32U);
                            row.payload1().load(f);
                            f.instruction(&Instruction::I64ExtendI32U);
                            f.instruction(&Instruction::I64Const(32));
                            f.instruction(&Instruction::I64Shl);
                            f.instruction(&Instruction::I64Or);
                            bits.store(f);
                            value.set_number(bits, f);
                            s.release_i64_local(bits, f);
                        }
                        IntrinsicPropertyRecipeKind::Boolean => {
                            value.set_boolean(row.payload0(), f)
                        }
                        IntrinsicPropertyRecipeKind::String => {
                            let text = s.reserve_gc_local(f).initialize(
                                s.resolve_pooled_string_reference(
                                    row.payload0(),
                                    self.runtime_helper_base()?,
                                    f,
                                ),
                                f,
                            );
                            value.set_reference(&text, s, f);
                            text.clear(f);
                        }
                        IntrinsicPropertyRecipeKind::ArrayPrototype => {
                            let array = s
                                .reserve_gc_local(f)
                                .initialize(self.emit_load_realm_array_prototype(realm, f), f);
                            value.set_reference(&array, s, f);
                            array.clear(f);
                        }
                        IntrinsicPropertyRecipeKind::WellKnownSymbol => {
                            let symbol = s
                                .reserve_gc_local(f)
                                .initialize(s.property_layout_symbol_reference(row, f), f);
                            value.set_reference(&symbol, s, f);
                            symbol.clear(f);
                        }
                        IntrinsicPropertyRecipeKind::LayoutObject => {
                            row.payload0().load(f);
                            f.instruction(&Instruction::I32Const(
                                IntrinsicPropertyLayoutId::ArrayUnscopables as i32,
                            ));
                            f.instruction(&Instruction::I32Ne);
                            self.open_frame(ControlFrameKind::If, f);
                            f.instruction(&Instruction::Unreachable);
                            self.pop_control(ControlFrameKind::If);
                            f.instruction(&Instruction::End);
                            let object = self.emit_intrinsic_unscopables(f)?;
                            value.set_reference(&object, s, f);
                            object.clear(f);
                        }
                        IntrinsicPropertyRecipeKind::Method
                        | IntrinsicPropertyRecipeKind::Accessor
                        | IntrinsicPropertyRecipeKind::ReadyRealmSlot => {
                            unreachable!("non-scalar recipe handled above")
                        }
                    }
                    let stored = s
                        .reserve_gc_local(f)
                        .initialize(s.struct_type::<StoredValue>().from_value(&value, f), f);
                    output.replace(stored.load(s, f).upcast().nullable(), f);
                    stored.clear(f);
                    value.clear(f);
                }
            }
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        let result = s
            .reserve_gc_local(f)
            .initialize(output.load(s, f).require_non_null(f), f);
        output.clear(f);
        Ok(result)
    }

    fn emit_layout_method_recipe(
        &self,
        row: &RuntimeHelperPropertyLayoutRow,
        setter: bool,
        realm: &GcLocal<RealmRecord>,
        f: &mut Function,
    ) -> GcLocal<PropertyValue> {
        let s = self.runtime_schema();
        let builtin = row.method_builtin(setter, f);
        let recipe = s.reserve_gc_local(f).initialize(
            s.struct_type::<DeferredIntrinsicValue>().construct(
                (
                    GcOperand::reference(realm, s),
                    GcOperand::helper_standard_builtin(&builtin),
                    GcOperand::null(s),
                ),
                f,
            ),
            f,
        );
        let value = s
            .reserve_gc_local(f)
            .initialize(recipe.load(s, f).upcast(), f);
        recipe.clear(f);
        value
    }

    /// The sealed row admits only ready producers. Reject an incomplete slot
    /// before its Undefined placeholder can poison a property identity cache.
    fn emit_layout_ready_slot(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        index: I32Local,
        callable: bool,
        f: &mut Function,
    ) -> GcLocal<StoredValue> {
        let s = self.runtime_schema();
        let table = s.reserve_gc_local(f).initialize(
            s.field(RealmRecordSchema::INTRINSICS)
                .read(realm, s, f)
                .reference(),
            f,
        );
        let stored = s.reserve_gc_local(f).initialize(
            s.array_type::<IntrinsicTable>()
                .read(&table, index, s, f)
                .reference(),
            f,
        );
        let value = s.reserve_value_local(f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, &value, s, f);
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Function as i32));
        f.instruction(&Instruction::I32Eq);
        if !callable {
            for tag in [
                WasmRuntimeValueTag::Object,
                WasmRuntimeValueTag::Array,
                WasmRuntimeValueTag::Arguments,
            ] {
                value.tag().load(f);
                f.instruction(&Instruction::I32Const(tag as i32));
                f.instruction(&Instruction::I32Eq);
                f.instruction(&Instruction::I32Or);
            }
        }
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.clear(f);
        table.clear(f);
        stored
    }

    /// This data-only object must not keep an otherwise unreachable Realm alive.
    /// Its immutable shape and scalar backing use the ordinary compact storage.
    fn emit_intrinsic_unscopables(
        &mut self,
        f: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        use crate::intrinsics::property_layouts::IntrinsicPropertyRecipe;
        let plan = IntrinsicPropertyLayoutId::ArrayUnscopables.layout()?;
        if plan.properties().len() > crate::objects::MAX_COMPACT_DATA_FIELDS {
            return Err(EmitError::unsupported(
                "unscopables exceeds compact property capacity",
            ));
        }
        let extent = i32::try_from(plan.properties().len())
            .map_err(|_| EmitError::unsupported("unscopables property count exceeds i32"))?;
        let attributes = crate::heap::StoredPropertyAttributes::Data {
            writable: true,
            enumerable: true,
            configurable: true,
        }
        .descriptor_word();
        let s = self.runtime_schema();
        let mut keys = Vec::with_capacity(plan.properties().len());
        for property in plan.properties() {
            let IntrinsicKey::Name(name) = property.key else {
                return Err(EmitError::unsupported(
                    "unscopables requires String data keys",
                ));
            };
            if !matches!(property.recipe, IntrinsicPropertyRecipe::Boolean(true))
                || property.flags != attributes
            {
                return Err(EmitError::unsupported(
                    "unscopables requires ordinary true data properties",
                ));
            }
            keys.push(
                s.reserve_gc_local(f)
                    .initialize(self.emit_interned_string_reference(name, f)?, f),
            );
        }
        let key_table = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactShapeKeys>()
                .fixed(keys.iter().map(|key| GcOperand::reference(key, s)), f),
            f,
        );
        let shape = s.reserve_gc_local(f).initialize(
            s.struct_type::<CompactDataShape>().construct(
                (
                    GcOperand::reference(&key_table, s),
                    GcOperand::descriptor_word(attributes),
                    GcOperand::boolean(true),
                ),
                f,
            ),
            f,
        );
        let count = s.reserve_i32_local(f);
        count.set_constant(extent, f);
        let tags = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactTagArray>().filled(
                GcOperand::i32(WasmRuntimeValueTag::Boolean.tag()),
                count,
                f,
            ),
            f,
        );
        let scalars = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactScalarArray>()
                .filled(GcOperand::i64(1), count, f),
            f,
        );
        let storage = s.reserve_gc_local(f).initialize(
            s.struct_type::<ArrayCompactDataStorage>()
                .construct(
                    (
                        GcOperand::reference(&shape, s),
                        GcOperand::reference(&tags, s),
                        GcOperand::nullable_reference(&scalars, s),
                        GcOperand::null(s),
                    ),
                    f,
                )
                .upcast::<CompactDataStorage>(),
            f,
        );
        let object = s
            .reserve_gc_local(f)
            .initialize(self.emit_alloc_plain_object_with_prototype(None, f)?, f);
        s.field(OrdinaryObjectSchema::PROPERTIES).write(
            &object,
            GcOperand::subtype_reference(&storage, s),
            s,
            f,
        );
        storage.clear(f);
        scalars.clear(f);
        tags.clear(f);
        s.release_i32_local(count, f);
        shape.clear(f);
        key_table.clear(f);
        for key in keys {
            key.clear(f);
        }
        Ok(object)
    }
}
