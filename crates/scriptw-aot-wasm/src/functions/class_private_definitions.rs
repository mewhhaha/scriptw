//! Private names are identities; complete definitions belong to class objects.

use super::*;
use crate::gc_types::{
    GcLocal, GcOperand, I32Local, NonNullable, Nullable, PrivateElement, PrivateElementKind,
    PrivateElementSchema, PrivateElementTable, PrivateEnvironment,
    PrivateName, PrivateNameTable, StoredValue, StringValue, ValueLocals,
};

#[derive(Clone, Copy)]
pub(super) enum ClassPrivateDefinition<'a> {
    Method(&'a ValueLocals),
    Getter(&'a ValueLocals),
    Setter(&'a ValueLocals),
    Accessor {
        getter: &'a ValueLocals,
        setter: &'a ValueLocals,
    },
}

pub(super) enum ClassMethodPlan<'a> {
    Single {
        function: &'a FunctionId,
        kind: ClassMethodKindIr,
    },
    Accessor(&'a scriptw_ir::AutoAccessorFunctionPairIr),
}

/// A completed accessor always owns both callable values.
pub(super) enum ClassMethodValues {
    Method(ValueLocals),
    Getter(ValueLocals),
    Setter(ValueLocals),
    Accessor {
        getter: ValueLocals,
        setter: ValueLocals,
    },
}

impl ClassMethodValues {
    pub(super) fn definition(&self) -> ClassPrivateDefinition<'_> {
        match self {
            Self::Method(value) => ClassPrivateDefinition::Method(value),
            Self::Getter(value) => ClassPrivateDefinition::Getter(value),
            Self::Setter(value) => ClassPrivateDefinition::Setter(value),
            Self::Accessor { getter, setter } => {
                ClassPrivateDefinition::Accessor { getter, setter }
            }
        }
    }
    pub(super) fn clear(self, function: &mut Function) {
        match self {
            Self::Method(value) | Self::Getter(value) | Self::Setter(value) => {
                value.clear(function)
            }
            Self::Accessor { getter, setter } => {
                getter.clear(function);
                setter.clear(function);
            }
        }
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_class_method_values(
        &mut self,
        plan: ClassMethodPlan<'_>,
        home: FunctionHomeObject<'_>,
        private_environment: &GcLocal<PrivateEnvironment, Nullable>,
        field_keys: &GcLocal<crate::gc_types::ValueArray, Nullable>,
        name: &crate::operations::PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<ClassMethodValues, EmitError> {
        match plan {
            ClassMethodPlan::Single { function: id, kind } => {
                let prefix = match kind {
                    ClassMethodKindIr::Method => FunctionNamePrefix::None,
                    ClassMethodKindIr::Getter => FunctionNamePrefix::Getter,
                    ClassMethodKindIr::Setter => FunctionNamePrefix::Setter,
                };
                let value = self.emit_defined_class_method_value(
                    id,
                    home,
                    private_environment,
                    field_keys,
                    name,
                    prefix,
                    function,
                )?;
                Ok(match kind {
                    ClassMethodKindIr::Method => ClassMethodValues::Method(value),
                    ClassMethodKindIr::Getter => ClassMethodValues::Getter(value),
                    ClassMethodKindIr::Setter => ClassMethodValues::Setter(value),
                })
            }
            ClassMethodPlan::Accessor(pair) => {
                let getter = self.emit_defined_class_method_value(
                    pair.getter(),
                    home,
                    private_environment,
                    field_keys,
                    name,
                    FunctionNamePrefix::Getter,
                    function,
                )?;
                let setter = self.emit_defined_class_method_value(
                    pair.setter(),
                    home,
                    private_environment,
                    field_keys,
                    name,
                    FunctionNamePrefix::Setter,
                    function,
                )?;
                Ok(ClassMethodValues::Accessor { getter, setter })
            }
        }
    }

    fn emit_defined_class_method_value(
        &mut self,
        id: &FunctionId,
        home: FunctionHomeObject<'_>,
        private_environment: &GcLocal<PrivateEnvironment, Nullable>,
        field_keys: &GcLocal<crate::gc_types::ValueArray, Nullable>,
        name: &crate::operations::PropertyKeyLocals,
        prefix: FunctionNamePrefix,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let schema = self.runtime_schema();
        let meta =
            self.functions.get(id).cloned().ok_or_else(|| {
                EmitError::unsupported("class member has no planned source callable")
            })?;
        let callable = schema.reserve_gc_local(function).initialize(
            self.emit_method_function_record(
                &meta,
                home,
                private_environment,
                field_keys,
                function,
            )?,
            function,
        );
        self.emit_set_function_name(&callable, name, prefix, function)?;
        let value = schema.reserve_value_local(function);
        value.set_reference(&callable, schema, function);
        callable.clear(function);
        Ok(value)
    }

    pub(super) fn emit_class_private_environment(
        &mut self,
        class: &ClassDefinitionIr,
        function: &mut Function,
    ) -> Result<GcLocal<PrivateEnvironment, Nullable>, EmitError> {
        let schema = self.runtime_schema();
        let parent = schema.reserve_gc_local(function).initialize(
            self.current_private_environment().load(schema, function),
            function,
        );
        let Some(plan) = class.private_environment else {
            return Ok(parent);
        };
        let descriptions = class
            .private_name_ids
            .iter()
            .map(|(text, id)| (id.name_ordinal(), text.as_str()))
            .collect::<BTreeMap<_, _>>();
        // Each fresh name is stored before the next allocation, so at most one
        // unpublished name is live at any allocation. Holding every name for one
        // `array.new_fixed` made each allocation a safepoint over all earlier
        // names: quadratic stack maps for classes with thousands of names.
        let mut table = None::<GcLocal<PrivateNameTable>>;
        let index = schema.reserve_i32_local(function);
        for ordinal in 0..plan.slot_count() {
            let description = schema
                .reserve_gc_local::<StringValue, Nullable>(function)
                .initialize_null(schema, function);
            if let Some(text) = descriptions.get(&ordinal) {
                description.replace(
                    // IR keys omit the sigil; PrivateIdentifier StringValue
                    // and the runtime private name's description include it.
                    self.emit_interned_string_reference(&format!("#{text}"), function)?
                        .nullable(),
                    function,
                );
            }
            let name = schema
                .reserve_gc_local::<PrivateName, NonNullable>(function)
                .initialize(
                    schema
                        .struct_type::<PrivateName>()
                        .construct((GcOperand::reference(&description, schema),), function),
                    function,
                );
            description.clear(function);
            let names = schema.array_type::<PrivateNameTable>();
            match &table {
                None => {
                    function.instruction(&Instruction::I32Const(plan.slot_count() as i32));
                    index.store(function);
                    table = Some(schema.reserve_gc_local(function).initialize(
                        names.filled(GcOperand::reference(&name, schema), index, function),
                        function,
                    ));
                }
                Some(table) => {
                    function.instruction(&Instruction::I32Const(ordinal as i32));
                    index.store(function);
                    names.write(
                        table,
                        index,
                        GcOperand::reference(&name, schema),
                        schema,
                        function,
                    );
                }
            }
            name.clear(function);
        }
        schema.release_i32_local(index, function);
        let table = match table {
            Some(table) => table,
            None => schema.reserve_gc_local(function).initialize(
                schema
                    .array_type::<PrivateNameTable>()
                    .fixed(std::iter::empty(), function),
                function,
            ),
        };
        let scope = schema.reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(plan.class_scope() as i64));
        scope.store(function);
        let environment = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PrivateEnvironment>()
                .construct(
                    (
                        GcOperand::reference(&parent, schema),
                        GcOperand::i64_local(scope),
                        GcOperand::reference(&table, schema),
                    ),
                    function,
                )
                .nullable(),
            function,
        );
        schema.release_i64_local(scope, function);
        table.clear(function);
        parent.clear(function);
        Ok(environment)
    }

    /// Class evaluation owns the table privately until all definitions finish.
    /// Accessor pairing replaces a complete immutable row, never its sides.
    pub(super) fn emit_class_private_definition(
        &mut self,
        table: &GcLocal<PrivateElementTable>,
        index: I32Local,
        name: &GcLocal<PrivateName>,
        definition: ClassPrivateDefinition<'_>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let method = schema
            .reserve_gc_local::<StoredValue, Nullable>(function)
            .initialize_null(schema, function);
        let getter = schema
            .reserve_gc_local::<StoredValue, Nullable>(function)
            .initialize_null(schema, function);
        let setter = schema
            .reserve_gc_local::<StoredValue, Nullable>(function)
            .initialize_null(schema, function);
        let kind = match definition {
            ClassPrivateDefinition::Method(value) => {
                method.replace(
                    schema
                        .struct_type::<StoredValue>()
                        .from_value(value, function)
                        .nullable(),
                    function,
                );
                PrivateElementKind::Method
            }
            ClassPrivateDefinition::Getter(value) => {
                getter.replace(
                    schema
                        .struct_type::<StoredValue>()
                        .from_value(value, function)
                        .nullable(),
                    function,
                );
                self.emit_class_existing_private_accessor_side(
                    table,
                    index,
                    PrivateElementSchema::SETTER,
                    &setter,
                    function,
                );
                PrivateElementKind::Accessor
            }
            ClassPrivateDefinition::Setter(value) => {
                setter.replace(
                    schema
                        .struct_type::<StoredValue>()
                        .from_value(value, function)
                        .nullable(),
                    function,
                );
                self.emit_class_existing_private_accessor_side(
                    table,
                    index,
                    PrivateElementSchema::GETTER,
                    &getter,
                    function,
                );
                PrivateElementKind::Accessor
            }
            ClassPrivateDefinition::Accessor {
                getter: get,
                setter: set,
            } => {
                getter.replace(
                    schema
                        .struct_type::<StoredValue>()
                        .from_value(get, function)
                        .nullable(),
                    function,
                );
                setter.replace(
                    schema
                        .struct_type::<StoredValue>()
                        .from_value(set, function)
                        .nullable(),
                    function,
                );
                PrivateElementKind::Accessor
            }
        };
        let row = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<PrivateElement>().construct(
                (
                    GcOperand::reference(name, schema),
                    GcOperand::constant(kind),
                    GcOperand::null(schema),
                    GcOperand::reference(&method, schema),
                    GcOperand::reference(&getter, schema),
                    GcOperand::reference(&setter, schema),
                ),
                function,
            ),
            function,
        );
        schema.array_type::<PrivateElementTable>().write(
            table,
            index,
            GcOperand::nullable_reference(&row, schema),
            schema,
            function,
        );
        row.clear(function);
        setter.clear(function);
        getter.clear(function);
        method.clear(function);
    }

    fn emit_class_existing_private_accessor_side(
        &mut self,
        table: &GcLocal<PrivateElementTable>,
        index: I32Local,
        side: crate::gc_types::GcField<
            PrivateElement,
            crate::gc_types::GcRef<StoredValue>,
            crate::gc_types::Immutable,
            Nullable,
        >,
        output: &GcLocal<StoredValue, Nullable>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let previous = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<PrivateElementTable>()
                .read(table, index, schema, function)
                .reference(),
            function,
        );
        previous.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let row = schema.reserve_gc_local(function).initialize(
            previous.load(schema, function).require_non_null(function),
            function,
        );
        output.replace(
            schema
                .struct_type::<PrivateElement>()
                .field(side)
                .read(&row, schema, function)
                .reference(),
            function,
        );
        row.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        previous.clear(function);
    }
}
