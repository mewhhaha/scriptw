//! A native join observed only through length needs no temporary String.
use super::dense_array::ReadKey;
use super::invariant_imul::{emit_own_native_method_guard, CachedRuntimeName, PureNativeMethod};
use super::*;
use crate::operations::PropertyKeyLocals;

/// Bound speculation even when the original arithmetic loop is zero-trip.
const MAX_PREFLIGHT_ELEMENTS: i64 = 256;

#[derive(PartialEq, Eq)]
pub(super) struct JoinedStringArrayLength {
    receiver: LocalBindingId,
    separator_length: u32,
    key: CachedRuntimeName,
}

impl JoinedStringArrayLength {
    pub(super) fn of_length(value: &TypedExpr, builder: &FunctionBuilder<'_>) -> Option<Self> {
        let outer = PropertyReadView::of(value)?;
        if !matches!(outer.key, ReadKey::Length | ReadKey::StaticString("length")) {
            return None;
        }
        let ExprIr::MaterializeBinding { name, value, body } = &outer.target.expr else {
            return None;
        };
        let ExprIr::Identifier(receiver) = &value.expr else {
            return None;
        };
        let receiver = builder.private_local_binding(receiver)?;
        let ExprIr::CallIndirect {
            direct_eval: None,
            callee,
            this_arg: Some(this_arg),
            args,
            static_regexp_compilation: None,
        } = &body.expr
        else {
            return None;
        };
        let ExprIr::Identifier(this_name) = &this_arg.expr else {
            return None;
        };
        let callee = PropertyReadView::of(callee)?;
        let ExprIr::Identifier(target_name) = &callee.target.expr else {
            return None;
        };
        if name != this_name
            || name != target_name
            || !matches!(callee.key, ReadKey::StaticString("join"))
        {
            return None;
        }
        let [separator] = args.as_slice() else {
            return None;
        };
        let ExprIr::String(separator) = &separator.expr else {
            return None;
        };
        let separator_length =
            u32::try_from(scriptw_ir::decode_js_string_utf16(separator).len()).ok()?;
        Some(Self {
            receiver,
            separator_length,
            key: CachedRuntimeName::new("join", builder.strings)?,
        })
    }

    pub(super) fn receiver(&self) -> LocalBindingId {
        self.receiver
    }

    pub(super) fn emit_guard(
        &self,
        builder: &mut FunctionBuilder<'_>,
        accepted: I32Local,
        function: &mut Function,
    ) -> Result<JoinedLengthFact, EmitError> {
        let s = builder.runtime_schema();
        let fact = JoinedLengthFact {
            length: s.reserve_f64_local(function),
        };
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        fact.length.store(function);
        // Every decline and the skipped enclosing Number guard share cleanup.
        let array = s
            .reserve_gc_local::<ArrayObject, Nullable>(function)
            .initialize_null(s, function);
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(function)
            .initialize_null(s, function);
        let properties = s
            .reserve_gc_local::<ObjectPropertyStorage, Nullable>(function)
            .initialize_null(s, function);
        let dictionary = s
            .reserve_gc_local::<OrdinaryPropertyStorage, Nullable>(function)
            .initialize_null(s, function);
        let prototype = s
            .reserve_gc_local::<ArrayObject, Nullable>(function)
            .initialize_null(s, function);
        let prototype_header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(function)
            .initialize_null(s, function);
        let values = s
            .reserve_gc_local::<PackedValueTable, Nullable>(function)
            .initialize_null(s, function);
        let stored = s
            .reserve_gc_local::<StoredValue, Nullable>(function)
            .initialize_null(s, function);
        let string = s
            .reserve_gc_local::<StringValue, Nullable>(function)
            .initialize_null(s, function);
        let units = s
            .reserve_gc_local::<CodeUnitArray, Nullable>(function)
            .initialize_null(s, function);
        let projection = s.reserve_value_local(function);
        let element = s.reserve_value_local(function);
        projection.set_undefined(function);
        element.set_undefined(function);
        let length = s.reserve_i64_local(function);
        let total = s.reserve_i64_local(function);
        let mode = s.reserve_i32_local(function);
        let index = s.reserve_i32_local(function);
        accepted.load(function);
        builder.open_frame(ControlFrameKind::If, function);
        accepted.set_constant(0, function);
        let decline = builder.open_frame(ControlFrameKind::Block, function);
        if let Some(initialized) = builder.local_binding(self.receiver).initialization_check() {
            initialized.load(function);
            function.instruction(&Instruction::I32Eqz);
            builder.emit_branch_if_to_target(decline, function);
        }
        builder
            .local_binding(self.receiver)
            .value()
            .tag()
            .load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Array.tag()));
        function.instruction(&Instruction::I32Ne);
        builder.emit_branch_if_to_target(decline, function);
        builder
            .local_binding(self.receiver)
            .value()
            .reference()
            .load(function);
        function.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(decline, function);
        array.replace(
            builder
                .local_binding(self.receiver)
                .value()
                .cast_reference::<ArrayObject>(s, function)
                .nullable(),
            function,
        );
        header.replace(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&array, s, function)
                .reference()
                .nullable(),
            function,
        );
        properties.replace(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&header, s, function)
                .reference()
                .nullable(),
            function,
        );
        properties.is_subtype::<OrdinaryPropertyStorage>(s, function);
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(decline, function);
        dictionary.replace(
            properties
                .load(s, function)
                .cast_subtype::<OrdinaryPropertyStorage>(s, function)
                .nullable(),
            function,
        );
        s.field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&dictionary, s, function);
        // An empty dictionary proves absence of an own join override. History
        // after deletion conservatively retains the ordinary call path.
        builder.emit_branch_if_to_target(decline, function);
        let realm = builder.load_current_realm(function);
        prototype.replace(
            builder
                .emit_load_realm_array_prototype(&realm, function)
                .nullable(),
            function,
        );
        realm.clear(function);
        s.field(OrdinaryObjectSchema::PROTOTYPE)
            .read(&header, s, function);
        prototype.load(s, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(decline, function);
        prototype_header.replace(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&prototype, s, function)
                .reference()
                .nullable(),
            function,
        );
        let projected = s.reserve_gc_local(function).initialize(
            prototype_header
                .load(s, function)
                .require_non_null(function),
            function,
        );
        projection.set_reference(&projected, s, function);
        projected.clear(function);
        let key = self.key.resolve(builder, function)?;
        let property = PropertyKeyLocals::from_string(s, &key, function);
        emit_own_native_method_guard(
            builder,
            &projection,
            &property,
            PureNativeMethod::ArrayJoin,
            accepted,
            function,
        )?;
        property.clear(function);
        key.clear(function);
        accepted.load(function);
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(decline, function);
        accepted.set_constant(0, function);
        s.field(ArrayObjectSchema::LENGTH)
            .read(&array, s, function)
            .store_i64(length, function);
        length.load(function);
        function.instruction(&Instruction::I64Const(MAX_PREFLIGHT_ELEMENTS));
        function.instruction(&Instruction::I64GtU);
        builder.emit_branch_if_to_target(decline, function);
        s.field(ArrayObjectSchema::COUNT).read(&array, s, function);
        length.load(function);
        function.instruction(&Instruction::I64Ne);
        builder.emit_branch_if_to_target(decline, function);
        s.field(ArrayObjectSchema::MODE)
            .read(&array, s, function)
            .store(mode, function);
        total.set_constant(0, function);
        length.load(function);
        function.instruction(&Instruction::I64Eqz);
        builder.open_frame(ControlFrameKind::If, function);
        mode.load(function);
        function.instruction(&Instruction::I32Const(
            ArrayIndexedStorageMode::PackedNumbers.encode(),
        ));
        function.instruction(&Instruction::I32Eq);
        mode.load(function);
        function.instruction(&Instruction::I32Const(
            ArrayIndexedStorageMode::PackedValues.encode(),
        ));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(decline, function);
        function.instruction(&Instruction::Else);
        mode.load(function);
        function.instruction(&Instruction::I32Const(
            ArrayIndexedStorageMode::PackedValues.encode(),
        ));
        function.instruction(&Instruction::I32Ne);
        builder.emit_branch_if_to_target(decline, function);
        values.replace(
            s.field(ArrayObjectSchema::VALUES)
                .read(&array, s, function)
                .reference(),
            function,
        );
        values.load(s, function).is_null(function);
        builder.emit_branch_if_to_target(decline, function);
        s.array_type::<PackedValueTable>()
            .length(&values, s, function);
        function.instruction(&Instruction::I64ExtendI32U);
        length.load(function);
        function.instruction(&Instruction::I64LtU);
        builder.emit_branch_if_to_target(decline, function);
        length.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(i64::from(self.separator_length)));
        function.instruction(&Instruction::I64Mul);
        total.store(function);
        index.set_constant(0, function);
        let complete = builder.open_frame(ControlFrameKind::Block, function);
        let scan = builder.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32GeU);
        builder.emit_branch_if_to_target(complete, function);
        stored.replace(
            s.array_type::<PackedValueTable>()
                .read(&values, index, s, function)
                .reference(),
            function,
        );
        stored.load(s, function).is_null(function);
        builder.emit_branch_if_to_target(decline, function);
        s.field(StoredValueSchema::TAG)
            .read(&stored, s, function)
            .store(element.tag(), function);
        s.field(StoredValueSchema::REFERENCE)
            .read(&stored, s, function)
            .store_eq(element.reference(), function);
        element.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        function.instruction(&Instruction::I32Ne);
        builder.emit_branch_if_to_target(decline, function);
        element.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<StringValue>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(decline, function);
        string.replace(
            element
                .cast_reference::<StringValue>(s, function)
                .nullable(),
            function,
        );
        units.replace(
            s.field(StringValueSchema::CODE_UNITS)
                .read(&string, s, function)
                .reference()
                .nullable(),
            function,
        );
        total.load(function);
        s.array_type::<CodeUnitArray>().length(&units, s, function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Add);
        total.store(function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        builder.emit_branch_to_target(scan, function);
        builder.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        builder.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // 256 rows and u32 code-unit/separator lengths cannot overflow i64.
        // Preserve the original join's resource owner outside its u32 domain.
        total.load(function);
        function.instruction(&Instruction::I64Const(u32::MAX as i64));
        function.instruction(&Instruction::I64GtU);
        builder.emit_branch_if_to_target(decline, function);
        total.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        fact.length.store(function);
        accepted.set_constant(1, function);
        builder.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // No GC root escapes the preflight, including partially admitted rows.
        element.clear(function);
        projection.clear(function);
        units.clear(function);
        string.clear(function);
        stored.clear(function);
        values.clear(function);
        prototype_header.clear(function);
        prototype.clear(function);
        dictionary.clear(function);
        properties.clear(function);
        header.clear(function);
        array.clear(function);
        s.release_i32_local(index, function);
        s.release_i32_local(mode, function);
        s.release_i64_local(total, function);
        s.release_i64_local(length, function);
        Ok(fact)
    }
}

/// Only the guarded constructor can supply this invariant primitive value.
pub(super) struct JoinedLengthFact {
    length: F64Local,
}

impl JoinedLengthFact {
    pub(super) fn emit(&self, function: &mut Function) {
        self.length.load(function);
    }
    pub(super) fn release(self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        builder
            .runtime_schema()
            .release_f64_local(self.length, function);
    }
}
