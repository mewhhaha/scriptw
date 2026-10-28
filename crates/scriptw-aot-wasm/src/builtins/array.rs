//! Native Array algorithms share whole-value property and callable owners.
use super::super::*;
use crate::control_flow::SyncIteratorConsumer;
use crate::gc_types::{
    ArrayObject, ArrayObjectSchema, BindingCell, BindingCellSchema, CompletionLocals, GcLocal,
    GcOperand, I32Local, I64Local, ScalarValue, StoredValue, ValueLocals,
};
use scriptw_ir::{ArrayAccumulationElementIr, ArrayAccumulationIr, ArrayAccumulationTargetIr};

mod callback_iteration;
mod copy_within;
mod find_via_predicate;
mod flat;
mod literal;
mod packed_join;
mod sort;
mod string_buffer;
use callback_iteration::ArrayCallbackIterationKind;
use copy_within::ArrayCopyWithinDirection;

#[derive(Clone, Copy)]
enum ArraySortOutput {
    Receiver,
    Copy,
}
#[derive(Clone, Copy)]
enum ArraySpliceOutput {
    Receiver,
    Copy,
}
#[derive(Clone, Copy)]
enum ArrayStringOperation {
    Join,
    Locale,
}
enum ToLocaleStringReceiverKind {
    ArrayLike,
    TypedArray,
}
#[derive(Clone, Copy)]
enum ArrayCallbackReceiverKind {
    ArrayLike,
    TypedArray,
}
#[derive(Clone, Copy)]
enum ArrayReduceDirection {
    LeftToRight,
    RightToLeft,
}
impl ArrayReduceDirection {
    const fn typed_array_receiver_error(&self) -> RuntimeErrorMessage {
        match self {
            Self::LeftToRight => {
                RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_REDUCE_REQUIRES_A_TYPEDARRAY
            }
            Self::RightToLeft => {
                RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_REDUCERIGHT_REQUIRES_A_TYPEDARRAY
            }
        }
    }

    const fn callback_not_callable_error(
        &self,
        receiver_kind: &ArrayCallbackReceiverKind,
    ) -> RuntimeErrorMessage {
        match (receiver_kind, self) {
            (ArrayCallbackReceiverKind::ArrayLike, Self::LeftToRight) => {
                RuntimeErrorMessage::ARRAY_PROTOTYPE_REDUCE_CALLBACK_IS_NOT_CALLABLE
            }
            (ArrayCallbackReceiverKind::ArrayLike, Self::RightToLeft) => {
                RuntimeErrorMessage::ARRAY_PROTOTYPE_REDUCERIGHT_CALLBACK_IS_NOT_CALLABLE
            }
            (ArrayCallbackReceiverKind::TypedArray, Self::LeftToRight) => {
                RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_REDUCE_CALLBACK_IS_NOT_CALLABLE
            }
            (ArrayCallbackReceiverKind::TypedArray, Self::RightToLeft) => {
                RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_REDUCERIGHT_CALLBACK_IS_NOT_CALLABLE
            }
        }
    }
}
#[derive(Clone, Copy)]
enum TypedArrayQuantifierKind {
    Every,
    Some,
}
#[derive(Clone, Copy)]
enum TypedArraySearchKind {
    Includes,
    IndexOf,
    LastIndexOf,
}
#[derive(Clone, Copy)]
enum ArrayAtReceiverPolicy {
    GenericArrayLike,
    TypedArray,
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_array_prototype_map_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_callback_iteration(function, ArrayCallbackIterationKind::Map)
    }

    pub(crate) fn compile_array_prototype_every_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_callback_iteration(function, ArrayCallbackIterationKind::Every)
    }

    pub(crate) fn compile_array_prototype_some_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_callback_iteration(function, ArrayCallbackIterationKind::Some)
    }

    pub(crate) fn compile_array_prototype_filter_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_callback_iteration(function, ArrayCallbackIterationKind::Filter)
    }
}

impl FunctionBuilder<'_> {
    fn emit_array_native_key(
        &mut self,
        name: &str,
        f: &mut Function,
    ) -> Result<crate::operations::PropertyKeyLocals, EmitError> {
        let s = self.runtime_schema();
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference(name, f)?, f);
        let key = crate::operations::PropertyKeyLocals::from_string(s, &text, f);
        text.clear(f);
        Ok(key)
    }

    pub(in crate::builtins) fn emit_array_native_propagate(
        &mut self,
        pending: &crate::gc_types::CompletionLocals,
        f: &mut Function,
    ) {
        self.completion().copy_from(pending, f);
        self.emit_propagate_current_throw_if_needed(f);
    }
    pub(in crate::builtins) fn emit_array_native_get(
        &mut self,
        source: &crate::gc_types::ValueLocals,
        name: &str,
        result: &crate::gc_types::CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let key = self.emit_array_native_key(name, f)?;
        self.emit_object_read(source, source, &key, result, f)?;
        key.clear(f);
        Ok(())
    }
    fn emit_array_native_create(
        &mut self,
        length: crate::gc_types::I64Local,
        result: &crate::gc_types::ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let proto = s
            .reserve_gc_local(f)
            .initialize(self.emit_load_realm_array_prototype(&realm, f), f);
        let value = s.reserve_value_local(f);
        value.set_reference(&proto, s, f);
        let array = s.reserve_gc_local(f).initialize(
            self.emit_alloc_array_payload_with_length_and_prototype(length, &value, f)?,
            f,
        );
        result.set_reference(&array, s, f);
        array.clear(f);
        value.clear(f);
        proto.clear(f);
        realm.clear(f);
        Ok(())
    }
    fn emit_array_native_target(
        &mut self,
        constructor: &crate::gc_types::ValueLocals,
        length: Option<crate::gc_types::I64Local>,
        target: &crate::gc_types::ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let pending = s.reserve_completion(f);
        let n = s.reserve_value_local(f);
        let zero = s.reserve_i64_local(f);
        f.instruction(&Instruction::I64Const(0));
        zero.store(f);
        self.emit_is_constructor_i32(constructor, f);
        self.open_frame(ControlFrameKind::If, f);
        if let Some(length) = length {
            length.load(f);
            f.instruction(&Instruction::F64ConvertI64U);
            f.instruction(&Instruction::I64ReinterpretF64);
            n.scalar().store(f);
            n.set_number(n.scalar(), f);
        }
        let length_arguments = [&n];
        let argv = self.emit_pre_evaluated_arg_vector(
            if length.is_some() {
                &length_arguments
            } else {
                &[]
            },
            f,
        );
        self.emit_function_or_proxy_construct_with_argv(
            constructor,
            constructor,
            &argv,
            &pending,
            f,
        )?;
        argv.clear(f);
        self.emit_array_native_propagate(&pending, f);
        target.copy_from(pending.value(), f);
        f.instruction(&Instruction::Else);
        self.emit_array_native_create(length.unwrap_or(zero), target, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(zero, f);
        n.clear(f);
        pending.clear(f);
        Ok(())
    }
    fn emit_array_native_set_length(
        &mut self,
        target: &crate::gc_types::ValueLocals,
        length: crate::gc_types::I64Local,
        pending: &crate::gc_types::CompletionLocals,
        message: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let value = s.reserve_value_local(f);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        value.scalar().store(f);
        value.set_number(value.scalar(), f);
        let key = self.emit_array_native_key("length", f)?;
        s.call_helper(
            crate::runtime_helpers::OrdinarySetArguments::new(
                target,
                target,
                &key,
                &value,
                self.current_environment(),
                self.current_program(),
            ),
            self.runtime_helper_base()?,
            f,
        )
        .store(pending, f);
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.compile_truthy_tagged_i32(pending.value(), f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(message, pending, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        key.clear(f);
        value.clear(f);
        Ok(())
    }
    pub(crate) fn emit_array_is_array_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let yes = s.reserve_i32_local(f);
        self.emit_builtin_arg_to_value(0, &value, f);
        self.emit_is_array_i32(&value, yes, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        value.set_boolean(yes, f);
        self.completion().set_normal(&value, f);
        s.release_i32_local(yes, f);
        pending.clear(f);
        value.clear(f);
        Ok(())
    }
    pub(crate) fn emit_array_constructor_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let constructor = s.reserve_value_local(f);
        let first = s.reserve_value_local(f);
        let target = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let count = s.reserve_i64_local(f);
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i32_local(f);
        let numeric_length = s.reserve_i32_local(f);
        let number = s.reserve_f64_local(f);
        let entry = self
            .body_entry_locals()
            .expect("Array constructor owns its native entry");
        constructor.copy_from(entry.new_target(), f);
        entry.argument_count().load(f);
        count.store(f);
        let argv = s
            .reserve_gc_local(f)
            .initialize(entry.arguments().load(s, f), f);
        constructor.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        constructor.set_reference(
            self.body_entry_locals()
                .and_then(|entry| entry.function_object())
                .expect("native Array entry owns its active Function"),
            s,
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_get_prototype_from_constructor(
            &constructor,
            crate::functions::OrdinaryDefaultPrototype::Array,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_builtin_arg_to_value(0, &first, f);
        count.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Eq);
        first.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        numeric_length.store(f);
        count.load(f);
        length.store(f);
        numeric_length.load(f);
        self.open_frame(ControlFrameKind::If, f);
        first.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        number.store(f);
        number.load(f);
        f.instruction(&Instruction::F64Const(0.0.into()));
        f.instruction(&Instruction::F64Ge);
        number.load(f);
        f.instruction(&Instruction::F64Const((u32::MAX as f64).into()));
        f.instruction(&Instruction::F64Le);
        f.instruction(&Instruction::I32And);
        number.load(f);
        number.load(f);
        f.instruction(&Instruction::F64Floor);
        f.instruction(&Instruction::F64Eq);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::INVALID_ARRAY_LENGTH,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        number.load(f);
        f.instruction(&Instruction::I64TruncF64U);
        length.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let array = s.reserve_gc_local(f).initialize(
            self.emit_alloc_array_payload_with_length_and_prototype(length, pending.value(), f)?,
            f,
        );
        target.set_reference(&array, s, f);
        array.clear(f);
        numeric_length.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I32Const(0));
        index.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, f);
        self.emit_argument_vector_entry_to_value(&argv, index, &first, f);
        index.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.store(f);
        let key = self.emit_array_native_index_key(length, f)?;
        self.emit_create_data_property_or_throw(&target, &key, &first, &pending, f)?;
        key.clear(f);
        self.emit_array_native_propagate(&pending, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&target, f);
        argv.clear(f);
        s.release_f64_local(number, f);
        s.release_i32_local(numeric_length, f);
        s.release_i32_local(index, f);
        s.release_i64_local(length, f);
        s.release_i64_local(count, f);
        pending.clear(f);
        target.clear(f);
        first.clear(f);
        constructor.clear(f);
        Ok(())
    }
    pub(crate) fn emit_array_of_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let constructor = s.reserve_value_local(f);
        let target = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i32_local(f);
        let key_index = s.reserve_i64_local(f);
        self.compile_this_to_locals(&constructor, f)?;
        let entry = self
            .body_entry_locals()
            .expect("Array.of owns its native entry");
        entry.argument_count().load(f);
        length.store(f);
        let argv = s
            .reserve_gc_local(f)
            .initialize(entry.arguments().load(s, f), f);
        self.emit_array_native_target(&constructor, Some(length), &target, f)?;
        f.instruction(&Instruction::I32Const(0));
        index.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, f);
        self.emit_argument_vector_entry_to_value(&argv, index, &value, f);
        index.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        key_index.store(f);
        let key = self.emit_array_native_index_key(key_index, f)?;
        self.emit_create_data_property_or_throw(&target, &key, &value, &pending, f)?;
        key.clear(f);
        self.emit_array_native_propagate(&pending, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_array_native_set_length(
            &target,
            length,
            &pending,
            RuntimeErrorMessage::ARRAY_OF_TARGET_IS_NOT_EXTENSIBLE,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.completion().set_normal(&target, f);
        argv.clear(f);
        s.release_i64_local(key_index, f);
        s.release_i32_local(index, f);
        s.release_i64_local(length, f);
        pending.clear(f);
        value.clear(f);
        target.clear(f);
        constructor.clear(f);
        Ok(())
    }
    fn emit_array_from_close_if_abrupt(
        &mut self,
        iterator: &crate::control_flow::OwnedSyncIterator,
        pending: &crate::gc_types::CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let closed = self.runtime_schema().reserve_completion(f);
        self.emit_sync_iterator_close(iterator, pending, &closed, f)?;
        self.completion().copy_from(&closed, f);
        closed.clear(f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    fn emit_array_from_mapper(
        &mut self,
        mapping: crate::gc_types::I32Local,
        mapper: &crate::gc_types::ValueLocals,
        this_arg: &crate::gc_types::ValueLocals,
        index: crate::gc_types::I64Local,
        value: &crate::gc_types::ValueLocals,
        pending: &crate::gc_types::CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        pending.set_normal(value, f);
        mapping.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let number = s.reserve_value_local(f);
        index.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        number.scalar().store(f);
        number.set_number(number.scalar(), f);
        let argv = self.emit_pre_evaluated_arg_vector(&[value, &number], f);
        self.emit_function_or_proxy_call_with_argv(mapper, this_arg, &argv, pending, f)?;
        argv.clear(f);
        number.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    pub(crate) fn emit_array_from_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let constructor = s.reserve_value_local(f);
        let items = s.reserve_value_local(f);
        let mapper = s.reserve_value_local(f);
        let this_arg = s.reserve_value_local(f);
        let method = s.reserve_value_local(f);
        let target = s.reserve_value_local(f);
        let source = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let index = s.reserve_i64_local(f);
        let length = s.reserve_i64_local(f);
        let mapping = s.reserve_i32_local(f);
        let done = s.reserve_i32_local(f);
        self.compile_this_to_locals(&constructor, f)?;
        self.emit_builtin_arg_to_value(0, &items, f);
        self.emit_builtin_arg_to_value(1, &mapper, f);
        self.emit_builtin_arg_to_value(2, &this_arg, f);
        mapper.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        mapping.store(f);
        mapping.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_callable_i32(&mapper, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::ARRAY_FROM_MAPPER_IS_NOT_CALLABLE,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let symbol = s.reserve_gc_local(f).initialize(
            self.emit_well_known_symbol_reference(scriptw_ir::WellKnownSymbol::Iterator, f)?,
            f,
        );
        let iterator_key = crate::operations::PropertyKeyLocals::from_symbol(s, &symbol, f);
        self.emit_object_read(&items, &items, &iterator_key, &pending, f)?;
        iterator_key.clear(f);
        symbol.clear(f);
        self.emit_array_native_propagate(&pending, f);
        method.copy_from(pending.value(), f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        method.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        method.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_callable_i32(&method, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::ARRAY_FROM_ITERATOR_METHOD_MUST_BE_CALLABLE,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_array_native_target(&constructor, None, &target, f)?;
        let argv = self.emit_pre_evaluated_arg_vector(&[], f);
        self.emit_function_or_proxy_call_with_argv(&method, &items, &argv, &pending, f)?;
        argv.clear(f);
        self.emit_array_native_propagate(&pending, f);
        let iterator = self.emit_get_sync_iterator_direct(
            pending.value(),
            SyncIteratorConsumer::ArrayFrom,
            f,
        )?;
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        f.instruction(&Instruction::I32Const(0));
        done.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(9_007_199_254_740_991));
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::ARRAY_FROM_ITERATOR_PRODUCED_TOO_MANY_VALUES,
            &pending,
            f,
        )?;
        self.emit_array_from_close_if_abrupt(&iterator, &pending, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_sync_iterator_step_value(&iterator, done, &value, f)?;
        done.load(f);
        self.emit_branch_if_to_target(finished, f);
        self.emit_array_from_mapper(mapping, &mapper, &this_arg, index, &value, &pending, f)?;
        self.emit_array_from_close_if_abrupt(&iterator, &pending, exit, f)?;
        value.copy_from(pending.value(), f);
        let key = self.emit_array_native_index_key(index, f)?;
        self.emit_create_data_property_or_throw(&target, &key, &value, &pending, f)?;
        key.clear(f);
        self.emit_array_from_close_if_abrupt(&iterator, &pending, exit, f)?;
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        iterator.clear(f);
        self.emit_array_native_set_length(
            &target,
            index,
            &pending,
            RuntimeErrorMessage::ARRAY_FROM_TARGET_IS_NOT_EXTENSIBLE,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.completion().set_normal(&target, f);
        self.emit_branch_to_target(exit, f);
        f.instruction(&Instruction::Else);
        self.emit_value_to_object_locals(&items, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        source.copy_from(pending.value(), f);
        self.emit_array_native_get(&source, "length", &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        value.copy_from(pending.value(), f);
        self.emit_to_length_i64_from_value_locals(&value, length, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_array_native_target(&constructor, Some(length), &target, f)?;
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(finished, f);
        self.emit_typed_array_or_object_index_read_from_locals(&source, index, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        value.copy_from(pending.value(), f);
        self.emit_array_from_mapper(mapping, &mapper, &this_arg, index, &value, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        value.copy_from(pending.value(), f);
        let key = self.emit_array_native_index_key(index, f)?;
        self.emit_create_data_property_or_throw(&target, &key, &value, &pending, f)?;
        key.clear(f);
        self.emit_array_native_propagate(&pending, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_array_native_set_length(
            &target,
            length,
            &pending,
            RuntimeErrorMessage::ARRAY_FROM_TARGET_IS_NOT_EXTENSIBLE,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.completion().set_normal(&target, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(done, f);
        s.release_i32_local(mapping, f);
        s.release_i64_local(length, f);
        s.release_i64_local(index, f);
        pending.clear(f);
        value.clear(f);
        source.clear(f);
        target.clear(f);
        method.clear(f);
        this_arg.clear(f);
        mapper.clear(f);
        items.clear(f);
        constructor.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    /// One ToObject and one observable LengthOfArrayLike snapshot.
    pub(crate) fn emit_array_like_length_snapshot(
        &mut self,
        receiver: &ValueLocals,
        length: I64Local,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_value_to_object_locals(receiver, pending, f)?;
        self.emit_array_native_propagate(pending, f);
        receiver.copy_from(pending.value(), f);
        self.emit_array_native_get(receiver, "length", pending, f)?;
        self.emit_array_native_propagate(pending, f);
        let length_value = self.runtime_schema().reserve_value_local(f);
        length_value.copy_from(pending.value(), f);
        self.emit_to_length_i64_from_value_locals(&length_value, length, pending, f)?;
        length_value.clear(f);
        self.emit_array_native_propagate(pending, f);
        Ok(())
    }

    fn emit_array_native_number(&self, integer: I64Local, value: &ValueLocals, f: &mut Function) {
        integer.load(f);
        f.instruction(&Instruction::F64ConvertI64S);
        f.instruction(&Instruction::I64ReinterpretF64);
        value.scalar().store(f);
        value.set_number(value.scalar(), f);
    }

    fn emit_array_native_set_index(
        &mut self,
        target: &ValueLocals,
        index: I64Local,
        value: &ValueLocals,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let key = self.emit_array_native_index_key(index, f)?;
        self.runtime_schema()
            .call_helper(
                crate::runtime_helpers::OrdinarySetArguments::new(
                    target,
                    target,
                    &key,
                    value,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                f,
            )
            .store(pending, f);
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.compile_truthy_tagged_i32(pending.value(), f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_ASSIGN_TO_READ_ONLY_PROPERTY,
            pending,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        key.clear(f);
        self.emit_array_native_propagate(pending, f);
        Ok(())
    }

    fn emit_array_native_delete_index(
        &mut self,
        target: &ValueLocals,
        index: I64Local,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let key = self.emit_array_native_index_key(index, f)?;
        self.emit_object_delete(target, &key, pending, f)?;
        key.clear(f);
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.compile_truthy_tagged_i32(pending.value(), f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_DELETE_PROPERTY,
            pending,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_array_native_propagate(pending, f);
        Ok(())
    }

    fn emit_array_native_has_index(
        &mut self,
        source: &ValueLocals,
        index: I64Local,
        present: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        present.set_constant(0, f);
        source.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ArrayObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let array = schema
            .reserve_gc_local(f)
            .initialize(source.cast_reference::<ArrayObject>(schema, f), f);
        let value = schema.reserve_value_local(f);
        // A live own data slot proves HasProperty without boxing packed
        // numbers or constructing a string key. Holes, accessors and Proxy
        // receivers retain the full observable HasProperty operation below.
        self.emit_array_own_indexed_value(&array, index, &value, present, f);
        value.clear(f);
        array.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        present.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let key = self.emit_array_native_index_key(index, f)?;
        self.emit_object_has_property_i32(source, &key, present, f)?;
        key.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    /// ArraySpeciesCreate preserves the full constructor and prototype identities.
    pub(crate) fn emit_array_species_create(
        &mut self,
        source: &ValueLocals,
        length: I64Local,
        target: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let pending = s.reserve_completion(f);
        let constructor = s.reserve_value_local(f);
        let canonical = s.reserve_value_local(f);
        let number = s.reserve_value_local(f);
        let is_array = s.reserve_i32_local(f);
        constructor.set_undefined(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_is_array_i32(source, is_array, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        is_array.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_array_native_get(source, "constructor", &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        constructor.copy_from(pending.value(), f);
        self.emit_is_constructor_i32(&constructor, f);
        self.open_frame(ControlFrameKind::If, f);
        let realm_result = self.emit_get_function_realm(&constructor, f);
        let realm = self.emit_route_function_realm_result(
            realm_result,
            crate::functions::FunctionRealmRevokedRoute::ThrowTypeErrorAndBranch {
                result: &pending,
                target: exit,
            },
            f,
        )?;
        let current = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        realm.realm().load(s, f);
        current.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_load_non_array_realm_intrinsic(
            realm.realm(),
            crate::functions::NonArrayRealmIntrinsicSlot::ArrayConstructor,
            &canonical,
            f,
        );
        self.emit_tagged_payload_same_value_i32(&constructor, &canonical, f)?;
        self.open_frame(ControlFrameKind::If, f);
        constructor.set_undefined(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        current.clear(f);
        self.release_resolved_function_realm_local(realm, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_is_heap_object_like_tag_i32(constructor.tag(), f);
        self.open_frame(ControlFrameKind::If, f);
        let symbol = s.reserve_gc_local(f).initialize(
            self.emit_well_known_symbol_reference(scriptw_ir::WellKnownSymbol::Species, f)?,
            f,
        );
        let key = crate::operations::PropertyKeyLocals::from_symbol(s, &symbol, f);
        self.emit_object_read(&constructor, &constructor, &key, &pending, f)?;
        key.clear(f);
        symbol.clear(f);
        self.emit_array_native_propagate(&pending, f);
        constructor.copy_from(pending.value(), f);
        constructor.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        constructor.set_undefined(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        constructor.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_array_native_create(length, target, f)?;
        f.instruction(&Instruction::Else);
        self.emit_is_constructor_i32(&constructor, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::ARRAY_SPECIES_CONSTRUCTOR_IS_NOT_A_CONSTRUCTOR,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_array_native_number(length, &number, f);
        let argv = self.emit_pre_evaluated_arg_vector(&[&number], f);
        self.emit_function_or_proxy_construct_with_argv(
            &constructor,
            &constructor,
            &argv,
            &pending,
            f,
        )?;
        argv.clear(f);
        self.emit_array_native_propagate(&pending, f);
        target.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        pending.set_normal(target, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_array_native_propagate(&pending, f);
        s.release_i32_local(is_array, f);
        number.clear(f);
        canonical.clear(f);
        constructor.clear(f);
        pending.clear(f);
        Ok(())
    }

    /// Source ArrayCreate owns the actual source Realm prototype for both
    /// fixed literals and their observable spread/accumulation path.
    fn emit_array_literal_object(
        &mut self,
        length: I64Local,
        f: &mut Function,
    ) -> Result<GcLocal<ArrayObject>, EmitError> {
        let s = self.runtime_schema();
        let prototype = s.reserve_value_local(f);
        self.emit_source_literal_prototype_to_value(
            crate::environments::global_environment::SourceLiteralPrototype::Array,
            &prototype,
            f,
        );
        let array = s.reserve_gc_local(f).initialize(
            self.emit_alloc_array_payload_with_length_and_prototype(length, &prototype, f)?,
            f,
        );
        prototype.clear(f);
        Ok(array)
    }

    fn emit_array_literal_create(
        &mut self,
        length: I64Local,
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let array = self.emit_array_literal_object(length, f)?;
        output.set_reference(&array, self.runtime_schema(), f);
        array.clear(f);
        Ok(())
    }

    fn emit_array_accumulation_append(
        &mut self,
        array: &ValueLocals,
        index: I64Local,
        value: &ValueLocals,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        index.load(f);
        f.instruction(&Instruction::I64Const(-1));
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_runtime_error(
            NativeErrorKind::RangeError,
            RuntimeErrorMessage::ARRAY_ACCUMULATION_INDEX_EXCEEDS_EXACT_BACKEND_RANGE,
            pending,
            f,
        )?;
        self.emit_array_native_propagate(pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let key = self.emit_array_native_index_key(index, f)?;
        self.emit_create_data_property_or_throw(array, &key, value, pending, f)?;
        key.clear(f);
        self.emit_array_native_propagate(pending, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        Ok(())
    }

    pub(crate) fn compile_array_accumulation_payload(
        &mut self,
        accumulation: &ArrayAccumulationIr,
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let array = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let index = s.reserve_i64_local(f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let cell = match accumulation.target() {
            ArrayAccumulationTargetIr::Fresh => {
                self.emit_array_literal_create(index, &array, f)?;
                None
            }
            ArrayAccumulationTargetIr::SuspensionOwned(slots) => {
                let array_binding =
                    self.lookup_binding(slots.array().as_str()).ok_or_else(|| {
                        EmitError::unsupported("missing suspension-owned Array accumulator")
                    })?;
                self.read_binding_to_locals(array_binding, &array, f)?;
                let storage = self
                    .lookup_binding(slots.next_index().as_str())
                    .ok_or_else(|| {
                        EmitError::unsupported("missing private Array accumulation index")
                    })?;
                let BindingStorage::EnvSlot { slot, hops } = storage else {
                    return Err(EmitError::unsupported(
                        "private Array accumulation index requires its suspension-owned cell",
                    ));
                };
                let environment = self.resolve_env_handle_local(hops, f);
                let cell = self.emit_environment_cell_local(&environment, slot, f);
                environment.clear(f);
                s.struct_type::<BindingCell>()
                    .field(BindingCellSchema::ARRAY_ACCUMULATION_INDEX)
                    .read(&cell, s, f)
                    .store_i64(index, f);
                Some(cell)
            }
        };
        for element in accumulation.elements() {
            match element {
                ArrayAccumulationElementIr::Elision => {
                    index.load(f);
                    f.instruction(&Instruction::I64Const(u32::MAX as i64));
                    f.instruction(&Instruction::I64GeU);
                    self.open_frame(ControlFrameKind::If, f);
                    self.emit_throw_runtime_error(
                        NativeErrorKind::RangeError,
                        RuntimeErrorMessage::INVALID_ARRAY_LENGTH,
                        &pending,
                        f,
                    )?;
                    self.emit_array_native_propagate(&pending, f);
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    index.load(f);
                    f.instruction(&Instruction::I64Const(1));
                    f.instruction(&Instruction::I64Add);
                    index.store(f);
                    let object = s
                        .reserve_gc_local(f)
                        .initialize(array.cast_reference::<ArrayObject>(s, f), f);
                    s.struct_type::<ArrayObject>()
                        .field(ArrayObjectSchema::LENGTH)
                        .write(&object, GcOperand::i64_local(index), s, f);
                    object.clear(f);
                }
                ArrayAccumulationElementIr::Value(expression) => {
                    self.compile_expr_to_value(expression, &value, f)?;
                    self.emit_propagate_current_throw_if_needed(f);
                    self.emit_array_accumulation_append(&array, index, &value, &pending, f)?;
                }
                ArrayAccumulationElementIr::Spread(spread) => {
                    self.compile_expr_to_value(&spread.value, &value, f)?;
                    self.emit_propagate_current_throw_if_needed(f);
                    let iterator = self.emit_get_sync_iterator(
                        &value,
                        SyncIteratorConsumer::ArrayAccumulation,
                        f,
                    )?;
                    let done = s.reserve_i32_local(f);
                    f.instruction(&Instruction::I32Const(0));
                    done.store(f);
                    let exit = self.open_frame(ControlFrameKind::Block, f);
                    let next = self.open_frame(ControlFrameKind::Loop, f);
                    self.emit_sync_iterator_step_value(&iterator, done, &value, f)?;
                    done.load(f);
                    self.emit_branch_if_to_target(exit, f);
                    self.emit_array_accumulation_append(&array, index, &value, &pending, f)?;
                    self.emit_branch_to_target(next, f);
                    self.pop_control(ControlFrameKind::Loop);
                    f.instruction(&Instruction::End);
                    self.pop_control(ControlFrameKind::Block);
                    f.instruction(&Instruction::End);
                    s.release_i32_local(done, f);
                    iterator.clear(f);
                }
            }
        }
        if let Some(cell) = cell {
            s.struct_type::<BindingCell>()
                .field(BindingCellSchema::ARRAY_ACCUMULATION_INDEX)
                .write(&cell, GcOperand::i64_local(index), s, f);
            cell.clear(f);
        }
        output.copy_from(&array, f);
        s.release_i64_local(index, f);
        pending.clear(f);
        value.clear(f);
        array.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn emit_array_native_index_key(
        &mut self,
        index: I64Local,
        f: &mut Function,
    ) -> Result<crate::operations::PropertyKeyLocals, EmitError> {
        // Exact decimal formatting also serves the private u64 accumulator.
        // No Number rounding may alter its mathematical property key.
        let s = self.runtime_schema();
        let negative = s.reserve_i32_local(f);
        negative.set_constant(0, f);
        let slot = s.reserve_gc_local(f);
        let text = s
            .call_helper(
                crate::runtime_helpers::IntegerDecimalStringArguments::new(index, negative),
                self.runtime_helper_base()?,
                f,
            )
            .bind(s, slot, f);
        let key = crate::operations::PropertyKeyLocals::from_string(s, &text, f);
        text.clear(f);
        s.release_i32_local(negative, f);
        Ok(key)
    }
}

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn emit_array_native_typed_receiver(
        &mut self,
        value: &ValueLocals,
        length: I64Local,
        message: RuntimeErrorMessage,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<crate::gc_types::GcLocal<crate::gc_types::TypedArrayObject>, EmitError> {
        let s = self.runtime_schema();
        value.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<crate::gc_types::TypedArrayObject>(
                crate::gc_types::GcNullability::NonNullable,
            )
            .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(message, pending, f)?;
        self.emit_array_native_propagate(pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let array = s.reserve_gc_local(f).initialize(
            value.cast_reference::<crate::gc_types::TypedArrayObject>(s, f),
            f,
        );
        self.emit_validate_typed_array_view(&array, length, pending, f)?;
        self.emit_array_native_propagate(pending, f);
        Ok(array)
    }
}
impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn emit_array_slice_clamped_index(
        &mut self,
        relative: I64Local,
        length: I64Local,
        index: I64Local,
        f: &mut Function,
    ) {
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Lt);
        self.open_frame(ControlFrameKind::If, f);
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::F64Neg);
        f.instruction(&Instruction::F64Lt);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        f.instruction(&Instruction::Else);
        length.load(f);
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncF64S);
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::F64Ge);
        self.open_frame(ControlFrameKind::If, f);
        length.load(f);
        index.store(f);
        f.instruction(&Instruction::Else);
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncF64U);
        index.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    pub(in crate::builtins) fn emit_array_native_typed_brand(
        &mut self,
        value: &ValueLocals,
        message: RuntimeErrorMessage,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<crate::gc_types::GcLocal<crate::gc_types::TypedArrayObject>, EmitError> {
        let s = self.runtime_schema();
        value.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<crate::gc_types::TypedArrayObject>(
                crate::gc_types::GcNullability::NonNullable,
            )
            .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(message, pending, f)?;
        self.emit_array_native_propagate(pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(s.reserve_gc_local(f).initialize(
            value.cast_reference::<crate::gc_types::TypedArrayObject>(s, f),
            f,
        ))
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_array_prototype_includes_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_array_search_native(
            ArrayCallbackReceiverKind::ArrayLike,
            TypedArraySearchKind::Includes,
            f,
        )
    }
    pub(crate) fn compile_array_prototype_index_of_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_array_search_native(
            ArrayCallbackReceiverKind::ArrayLike,
            TypedArraySearchKind::IndexOf,
            f,
        )
    }
    pub(crate) fn compile_array_prototype_last_index_of_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_array_search_native(
            ArrayCallbackReceiverKind::ArrayLike,
            TypedArraySearchKind::LastIndexOf,
            f,
        )
    }
    pub(crate) fn compile_typed_array_prototype_includes_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_array_search_native(
            ArrayCallbackReceiverKind::TypedArray,
            TypedArraySearchKind::Includes,
            f,
        )
    }
    pub(crate) fn compile_typed_array_prototype_index_of_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_array_search_native(
            ArrayCallbackReceiverKind::TypedArray,
            TypedArraySearchKind::IndexOf,
            f,
        )
    }
    pub(crate) fn compile_typed_array_prototype_last_index_of_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_array_search_native(
            ArrayCallbackReceiverKind::TypedArray,
            TypedArraySearchKind::LastIndexOf,
            f,
        )
    }
    fn emit_array_search_native(
        &mut self,
        receiver_kind: ArrayCallbackReceiverKind,
        method: TypedArraySearchKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let search = s.reserve_value_local(f);
        let from = s.reserve_value_local(f);
        let result = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let relative = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let present = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        match receiver_kind {
            ArrayCallbackReceiverKind::ArrayLike => {
                self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?
            }
            ArrayCallbackReceiverKind::TypedArray => {
                let message = match method {
                    TypedArraySearchKind::Includes => {
                        RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_INCLUDES_REQUIRES_A_TYPEDARRAY
                    }
                    TypedArraySearchKind::IndexOf => {
                        RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_INDEXOF_REQUIRES_A_TYPEDARRAY
                    }
                    TypedArraySearchKind::LastIndexOf => {
                        RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_LASTINDEXOF_REQUIRES_A_TYPEDARRAY
                    }
                };
                let array =
                    self.emit_array_native_typed_receiver(&receiver, length, message, &pending, f)?;
                array.clear(f);
            }
        }
        self.emit_builtin_arg_to_value(0, &search, f);
        match method {
            TypedArraySearchKind::Includes => result.set_scalar(ScalarValue::Boolean(false), f),
            TypedArraySearchKind::IndexOf | TypedArraySearchKind::LastIndexOf => {
                result.set_scalar(ScalarValue::NumberBits((-1.0f64).to_bits() as i64), f)
            }
        }
        let finished = self.open_frame(ControlFrameKind::Block, f);
        length.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(finished, f);
        // An empty receiver never coerces fromIndex. Omission remains distinct
        // from an explicitly supplied undefined for lastIndexOf.
        match method {
            TypedArraySearchKind::Includes | TypedArraySearchKind::IndexOf => {
                self.emit_builtin_arg_to_value(1, &from, f);
                self.emit_value_to_number_payload(&from, &pending, f)?;
                self.emit_array_native_propagate(&pending, f);
                self.emit_to_integer_or_infinity_number_payload_from_number_payload(
                    pending.value().scalar(),
                    relative,
                    f,
                );
                self.emit_array_slice_clamped_index(relative, length, index, f);
            }
            TypedArraySearchKind::LastIndexOf => {
                length.load(f);
                f.instruction(&Instruction::I64Const(1));
                f.instruction(&Instruction::I64Sub);
                index.store(f);
                self.emit_builtin_arg_is_present_i32(1, f);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_builtin_arg_to_value(1, &from, f);
                self.emit_value_to_number_payload(&from, &pending, f)?;
                self.emit_array_native_propagate(&pending, f);
                self.emit_to_integer_or_infinity_number_payload_from_number_payload(
                    pending.value().scalar(),
                    relative,
                    f,
                );
                relative.load(f);
                f.instruction(&Instruction::F64ReinterpretI64);
                f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
                f.instruction(&Instruction::F64Lt);
                self.open_frame(ControlFrameKind::If, f);
                relative.load(f);
                f.instruction(&Instruction::F64ReinterpretI64);
                length.load(f);
                f.instruction(&Instruction::F64ConvertI64U);
                f.instruction(&Instruction::F64Neg);
                f.instruction(&Instruction::F64Lt);
                self.emit_branch_if_to_target(finished, f);
                relative.load(f);
                f.instruction(&Instruction::F64ReinterpretI64);
                f.instruction(&Instruction::I64TruncF64S);
                length.load(f);
                f.instruction(&Instruction::I64Add);
                index.store(f);
                f.instruction(&Instruction::Else);
                relative.load(f);
                f.instruction(&Instruction::F64ReinterpretI64);
                index.load(f);
                f.instruction(&Instruction::F64ConvertI64U);
                f.instruction(&Instruction::F64Lt);
                self.open_frame(ControlFrameKind::If, f);
                relative.load(f);
                f.instruction(&Instruction::F64ReinterpretI64);
                f.instruction(&Instruction::I64TruncF64U);
                index.store(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        let next = self.open_frame(ControlFrameKind::Loop, f);
        match method {
            TypedArraySearchKind::Includes | TypedArraySearchKind::IndexOf => {
                index.load(f);
                length.load(f);
                f.instruction(&Instruction::I64GeU);
            }
            TypedArraySearchKind::LastIndexOf => {
                index.load(f);
                f.instruction(&Instruction::I64Const(0));
                f.instruction(&Instruction::I64LtS);
            }
        }
        self.emit_branch_if_to_target(finished, f);
        match method {
            TypedArraySearchKind::IndexOf | TypedArraySearchKind::LastIndexOf => {
                self.emit_array_native_has_index(&receiver, index, present, f)?
            }
            TypedArraySearchKind::Includes => {
                f.instruction(&Instruction::I32Const(1));
                present.store(f);
            }
        }
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_typed_array_or_object_index_read_from_locals(&receiver, index, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        match method {
            TypedArraySearchKind::Includes => {
                self.emit_tagged_payload_same_value_zero_i32(pending.value(), &search, f)?
            }
            TypedArraySearchKind::IndexOf | TypedArraySearchKind::LastIndexOf => {
                self.emit_tagged_payload_equality_i32(pending.value(), &search, f)?
            }
        }
        self.open_frame(ControlFrameKind::If, f);
        match method {
            TypedArraySearchKind::Includes => result.set_scalar(ScalarValue::Boolean(true), f),
            TypedArraySearchKind::IndexOf | TypedArraySearchKind::LastIndexOf => {
                self.emit_array_native_number(index, &result, f)
            }
        }
        self.emit_branch_to_target(finished, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_increment_local(
            index,
            match method {
                TypedArraySearchKind::LastIndexOf => -1,
                TypedArraySearchKind::Includes | TypedArraySearchKind::IndexOf => 1,
            },
            f,
        );
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&result, f);
        s.release_i32_local(present, f);
        s.release_i64_local(index, f);
        s.release_i64_local(relative, f);
        s.release_i64_local(length, f);
        pending.clear(f);
        result.clear(f);
        from.clear(f);
        search.clear(f);
        receiver.clear(f);
        Ok(())
    }
    pub(super) fn compile_array_prototype_at_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_like_at_builtin(ArrayAtReceiverPolicy::GenericArrayLike, f)
    }
    pub(super) fn compile_typed_array_prototype_at_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_like_at_builtin(ArrayAtReceiverPolicy::TypedArray, f)
    }
    fn compile_array_like_at_builtin(
        &mut self,
        policy: ArrayAtReceiverPolicy,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let argument = s.reserve_value_local(f);
        let result = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let relative = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        match policy {
            ArrayAtReceiverPolicy::GenericArrayLike => {
                self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?
            }
            ArrayAtReceiverPolicy::TypedArray => {
                let array = self.emit_array_native_typed_receiver(
                    &receiver,
                    length,
                    RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_AT_CALLED_ON_INCOMPATIBLE_RECEIVER,
                    &pending,
                    f,
                )?;
                array.clear(f);
            }
        }
        self.emit_builtin_arg_to_value(0, &argument, f);
        self.emit_value_to_number_payload(&argument, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            relative,
            f,
        );
        result.set_undefined(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Lt);
        self.open_frame(ControlFrameKind::If, f);
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::F64Neg);
        f.instruction(&Instruction::F64Lt);
        self.emit_branch_if_to_target(finished, f);
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncF64S);
        length.load(f);
        f.instruction(&Instruction::I64Add);
        index.store(f);
        f.instruction(&Instruction::Else);
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::F64Ge);
        self.emit_branch_if_to_target(finished, f);
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncF64U);
        index.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_typed_array_or_object_index_read_from_locals(&receiver, index, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        result.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&result, f);
        s.release_i64_local(index, f);
        s.release_i64_local(relative, f);
        s.release_i64_local(length, f);
        pending.clear(f);
        result.clear(f);
        argument.clear(f);
        receiver.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_array_prototype_fill_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let argument = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let relative = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let end = s.reserve_i64_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?;
        self.emit_builtin_arg_to_value(0, &value, f);
        self.emit_builtin_arg_to_value(1, &argument, f);
        self.emit_value_to_number_payload(&argument, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            relative,
            f,
        );
        self.emit_array_slice_clamped_index(relative, length, index, f);
        length.load(f);
        end.store(f);
        self.emit_builtin_arg_to_value(2, &argument, f);
        argument.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_number_payload(&argument, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            relative,
            f,
        );
        self.emit_array_slice_clamped_index(relative, length, end, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        end.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(finished, f);
        self.emit_array_native_set_index(&receiver, index, &value, &pending, f)?;
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&receiver, f);
        for local in [end, index, relative, length] {
            s.release_i64_local(local, f);
        }
        pending.clear(f);
        argument.clear(f);
        value.clear(f);
        receiver.clear(f);
        Ok(())
    }
    pub(crate) fn compile_array_prototype_slice_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let target = s.reserve_value_local(f);
        let argument = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let relative = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let end = s.reserve_i64_local(f);
        let count = s.reserve_i64_local(f);
        let output = s.reserve_i64_local(f);
        let present = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?;
        self.emit_builtin_arg_to_value(0, &argument, f);
        self.emit_value_to_number_payload(&argument, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            relative,
            f,
        );
        self.emit_array_slice_clamped_index(relative, length, index, f);
        length.load(f);
        end.store(f);
        self.emit_builtin_arg_to_value(1, &argument, f);
        argument.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_number_payload(&argument, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            relative,
            f,
        );
        self.emit_array_slice_clamped_index(relative, length, end, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I64Const(0));
        count.store(f);
        end.load(f);
        index.load(f);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        end.load(f);
        index.load(f);
        f.instruction(&Instruction::I64Sub);
        count.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_array_species_create(&receiver, count, &target, f)?;
        f.instruction(&Instruction::I64Const(0));
        output.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        end.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(finished, f);
        self.emit_array_native_has_index(&receiver, index, present, f)?;
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_typed_array_or_object_index_read_from_locals(&receiver, index, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        argument.copy_from(pending.value(), f);
        let key = self.emit_array_native_index_key(output, f)?;
        self.emit_create_data_property_or_throw(&target, &key, &argument, &pending, f)?;
        key.clear(f);
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_increment_local(output, 1, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_array_native_set_length(
            &target,
            output,
            &pending,
            RuntimeErrorMessage::CANNOT_ASSIGN_TO_READ_ONLY_PROPERTY,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.completion().set_normal(&target, f);
        s.release_i32_local(present, f);
        for local in [output, count, end, index, relative, length] {
            s.release_i64_local(local, f);
        }
        pending.clear(f);
        argument.clear(f);
        target.clear(f);
        receiver.clear(f);
        Ok(())
    }
    pub(crate) fn compile_typed_array_prototype_slice_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let argument = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let relative = s.reserve_i64_local(f);
        let start = s.reserve_i64_local(f);
        let end = s.reserve_i64_local(f);
        let count = s.reserve_i64_local(f);
        let current = s.reserve_i64_local(f);
        let output = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let source = self.emit_typed_array_species_length_source(
            crate::objects::TypedArraySpeciesLengthMethod::Slice,
            &receiver,
            f,
        )?;
        self.emit_builtin_arg_to_value(0, &argument, f);
        self.emit_value_to_number_payload(&argument, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            relative,
            f,
        );
        self.emit_array_slice_clamped_index(relative, source.length_local(), start, f);
        source.length_local().load(f);
        end.store(f);
        self.emit_builtin_arg_to_value(1, &argument, f);
        argument.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_number_payload(&argument, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            relative,
            f,
        );
        self.emit_array_slice_clamped_index(relative, source.length_local(), end, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I64Const(0));
        count.store(f);
        end.load(f);
        start.load(f);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        end.load(f);
        start.load(f);
        f.instruction(&Instruction::I64Sub);
        count.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let target = self.emit_typed_array_species_create_with_length(&source, count, f)?;
        count.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        // Species construction may detach or resize the source. Zero count
        // bypasses this second source validation, while output write admission
        // belongs to the species producer even for an empty result.
        self.emit_validate_typed_array_view(source.array(), current, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        current.load(f);
        end.load(f);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        current.load(f);
        end.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        source.element_kind_local().load(f);
        target.element_kind_local().load(f);
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let from = s.reserve_i64_local(f);
        let to = s.reserve_i64_local(f);
        let width = s.reserve_i64_local(f);
        let target_width = s.reserve_i64_local(f);
        let bytes = s.reserve_i64_local(f);
        let kind = s.reserve_i32_local(f);
        let source_access =
            self.emit_native_typed_array_byte_access(source.array(), from, width, kind, f);
        let target_access =
            self.emit_native_typed_array_byte_access(target.array(), to, target_width, kind, f);
        start.load(f);
        width.load(f);
        f.instruction(&Instruction::I64Mul);
        from.load(f);
        f.instruction(&Instruction::I64Add);
        from.store(f);
        f.instruction(&Instruction::I64Const(0));
        bytes.store(f);
        end.load(f);
        start.load(f);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        end.load(f);
        start.load(f);
        f.instruction(&Instruction::I64Sub);
        width.load(f);
        f.instruction(&Instruction::I64Mul);
        bytes.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_binary_copy_bytes(&source_access, from, &target_access, to, bytes, f)?;
        target_access.clear(s, f);
        source_access.clear(s, f);
        s.release_i32_local(kind, f);
        for local in [bytes, target_width, width, to, from] {
            s.release_i64_local(local, f);
        }
        f.instruction(&Instruction::Else);
        start.load(f);
        index.store(f);
        f.instruction(&Instruction::I64Const(0));
        output.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        end.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(finished, f);
        self.emit_typed_array_element_read_from_locals(source.array(), index, &argument, f)?;
        self.emit_typed_array_species_element_write(&target, output, &argument, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_increment_local(index, 1, f);
        self.emit_increment_local(output, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_publish_typed_array_species_result(target, self.completion(), f);
        source.clear(s, f);
        for local in [index, output, current, count, end, start, relative] {
            s.release_i64_local(local, f);
        }
        pending.clear(f);
        argument.clear(f);
        receiver.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn compile_array_prototype_for_each_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_like_for_each_builtin(f, ArrayCallbackReceiverKind::ArrayLike)
    }
    pub(super) fn compile_typed_array_prototype_for_each_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_like_for_each_builtin(f, ArrayCallbackReceiverKind::TypedArray)
    }
    fn compile_array_like_for_each_builtin(
        &mut self,
        f: &mut Function,
        kind: ArrayCallbackReceiverKind,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let callback = s.reserve_value_local(f);
        let this_arg = s.reserve_value_local(f);
        let element = s.reserve_value_local(f);
        let number = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let present = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        match kind {
            ArrayCallbackReceiverKind::ArrayLike => {
                self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?
            }
            ArrayCallbackReceiverKind::TypedArray => {
                let array = self.emit_array_native_typed_receiver(
                    &receiver,
                    length,
                    RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_FOREACH_REQUIRES_A_TYPEDARRAY,
                    &pending,
                    f,
                )?;
                array.clear(f);
            }
        }
        self.emit_builtin_arg_to_value(0, &callback, f);
        self.emit_is_callable_i32(&callback, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            match kind {
                ArrayCallbackReceiverKind::ArrayLike => {
                    RuntimeErrorMessage::ARRAY_PROTOTYPE_FOREACH_CALLBACK_IS_NOT_CALLABLE
                }
                ArrayCallbackReceiverKind::TypedArray => {
                    RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_FOREACH_CALLBACK_IS_NOT_CALLABLE
                }
            },
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_builtin_arg_to_value(1, &this_arg, f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(finished, f);
        match kind {
            ArrayCallbackReceiverKind::ArrayLike => {
                self.emit_array_native_has_index(&receiver, index, present, f)?
            }
            ArrayCallbackReceiverKind::TypedArray => {
                f.instruction(&Instruction::I32Const(1));
                present.store(f);
            }
        }
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_typed_array_or_object_index_read_from_locals(&receiver, index, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        element.copy_from(pending.value(), f);
        self.emit_array_native_number(index, &number, f);
        let argv = self.emit_pre_evaluated_arg_vector(&[&element, &number, &receiver], f);
        self.emit_function_or_proxy_call_with_argv(&callback, &this_arg, &argv, &pending, f)?;
        argv.clear(f);
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        element.set_undefined(f);
        self.completion().set_normal(&element, f);
        s.release_i32_local(present, f);
        s.release_i64_local(index, f);
        s.release_i64_local(length, f);
        pending.clear(f);
        number.clear(f);
        element.clear(f);
        this_arg.clear(f);
        callback.clear(f);
        receiver.clear(f);
        Ok(())
    }
    pub(super) fn compile_array_reduce_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_like_reduce_builtin(
            ArrayCallbackReceiverKind::ArrayLike,
            ArrayReduceDirection::LeftToRight,
            f,
        )
    }
    pub(super) fn compile_array_reduce_right_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_like_reduce_builtin(
            ArrayCallbackReceiverKind::ArrayLike,
            ArrayReduceDirection::RightToLeft,
            f,
        )
    }
    pub(super) fn compile_typed_array_reduce_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_like_reduce_builtin(
            ArrayCallbackReceiverKind::TypedArray,
            ArrayReduceDirection::LeftToRight,
            f,
        )
    }
    pub(super) fn compile_typed_array_reduce_right_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_like_reduce_builtin(
            ArrayCallbackReceiverKind::TypedArray,
            ArrayReduceDirection::RightToLeft,
            f,
        )
    }
    fn compile_array_like_reduce_builtin(
        &mut self,
        kind: ArrayCallbackReceiverKind,
        direction: ArrayReduceDirection,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let callback = s.reserve_value_local(f);
        let accumulator = s.reserve_value_local(f);
        let element = s.reserve_value_local(f);
        let number = s.reserve_value_local(f);
        let undefined = s.reserve_value_local(f);
        undefined.set_undefined(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let present = s.reserve_i32_local(f);
        let initialized = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        match kind {
            ArrayCallbackReceiverKind::ArrayLike => {
                self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?
            }
            ArrayCallbackReceiverKind::TypedArray => {
                let array = self.emit_array_native_typed_receiver(
                    &receiver,
                    length,
                    direction.typed_array_receiver_error(),
                    &pending,
                    f,
                )?;
                array.clear(f);
            }
        }
        self.emit_builtin_arg_to_value(0, &callback, f);
        self.emit_is_callable_i32(&callback, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            direction.callback_not_callable_error(&kind),
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_builtin_arg_is_present_i32(1, f);
        initialized.store(f);
        self.emit_builtin_arg_to_value(1, &accumulator, f);
        match direction {
            ArrayReduceDirection::LeftToRight => f.instruction(&Instruction::I64Const(0)),
            ArrayReduceDirection::RightToLeft => {
                length.load(f);
                f.instruction(&Instruction::I64Const(1));
                f.instruction(&Instruction::I64Sub)
            }
        };
        index.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        match direction {
            ArrayReduceDirection::LeftToRight => {
                index.load(f);
                length.load(f);
                f.instruction(&Instruction::I64GeU);
            }
            ArrayReduceDirection::RightToLeft => {
                index.load(f);
                f.instruction(&Instruction::I64Const(0));
                f.instruction(&Instruction::I64LtS);
            }
        }
        self.emit_branch_if_to_target(finished, f);
        match kind {
            ArrayCallbackReceiverKind::ArrayLike => {
                self.emit_array_native_has_index(&receiver, index, present, f)?
            }
            ArrayCallbackReceiverKind::TypedArray => {
                f.instruction(&Instruction::I32Const(1));
                present.store(f);
            }
        }
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_typed_array_or_object_index_read_from_locals(&receiver, index, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        element.copy_from(pending.value(), f);
        initialized.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_array_native_number(index, &number, f);
        let argv =
            self.emit_pre_evaluated_arg_vector(&[&accumulator, &element, &number, &receiver], f);
        self.emit_function_or_proxy_call_with_argv(&callback, &undefined, &argv, &pending, f)?;
        argv.clear(f);
        self.emit_array_native_propagate(&pending, f);
        accumulator.copy_from(pending.value(), f);
        f.instruction(&Instruction::Else);
        accumulator.copy_from(&element, f);
        f.instruction(&Instruction::I32Const(1));
        initialized.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_increment_local(
            index,
            match direction {
                ArrayReduceDirection::LeftToRight => 1,
                ArrayReduceDirection::RightToLeft => -1,
            },
            f,
        );
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        initialized.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            match (&kind, &direction) {
                (ArrayCallbackReceiverKind::ArrayLike, ArrayReduceDirection::LeftToRight) => {
                    RuntimeErrorMessage::REDUCE_OF_EMPTY_ARRAY_WITH_NO_INITIAL_VALUE
                }
                (ArrayCallbackReceiverKind::ArrayLike, ArrayReduceDirection::RightToLeft) => {
                    RuntimeErrorMessage::REDUCE_OF_EMPTY_ARRAY_WITH_NO_INITIAL_VALUE
                }
                (ArrayCallbackReceiverKind::TypedArray, ArrayReduceDirection::LeftToRight) => {
                    RuntimeErrorMessage::REDUCE_OF_EMPTY_TYPED_ARRAY_WITH_NO_INITIAL_VALUE
                }
                (ArrayCallbackReceiverKind::TypedArray, ArrayReduceDirection::RightToLeft) => {
                    RuntimeErrorMessage::REDUCE_OF_EMPTY_TYPED_ARRAY_WITH_NO_INITIAL_VALUE
                }
            },
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&accumulator, f);
        s.release_i32_local(initialized, f);
        s.release_i32_local(present, f);
        s.release_i64_local(index, f);
        s.release_i64_local(length, f);
        pending.clear(f);
        undefined.clear(f);
        number.clear(f);
        element.clear(f);
        accumulator.clear(f);
        callback.clear(f);
        receiver.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_typed_array_prototype_map_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let callback = s.reserve_value_local(f);
        let this_arg = s.reserve_value_local(f);
        let element = s.reserve_value_local(f);
        let number = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let index = s.reserve_i64_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let source = self.emit_typed_array_species_length_source(
            crate::objects::TypedArraySpeciesLengthMethod::Map,
            &receiver,
            f,
        )?;
        self.emit_builtin_arg_to_value(0, &callback, f);
        self.emit_is_callable_i32(&callback, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_MAP_CALLBACK_IS_NOT_CALLABLE,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_builtin_arg_to_value(1, &this_arg, f);
        let target =
            self.emit_typed_array_species_create_with_length(&source, source.length_local(), f)?;
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        source.length_local().load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(finished, f);
        self.emit_typed_array_element_read_from_locals(source.array(), index, &element, f)?;
        self.emit_array_native_number(index, &number, f);
        let argv = self.emit_pre_evaluated_arg_vector(&[&element, &number, &receiver], f);
        self.emit_function_or_proxy_call_with_argv(&callback, &this_arg, &argv, &pending, f)?;
        argv.clear(f);
        self.emit_array_native_propagate(&pending, f);
        element.copy_from(pending.value(), f);
        self.emit_typed_array_species_element_write(&target, index, &element, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_publish_typed_array_species_result(target, self.completion(), f);
        source.clear(s, f);
        s.release_i64_local(index, f);
        pending.clear(f);
        number.clear(f);
        element.clear(f);
        this_arg.clear(f);
        callback.clear(f);
        receiver.clear(f);
        Ok(())
    }
    pub(crate) fn compile_typed_array_prototype_filter_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let callback = s.reserve_value_local(f);
        let this_arg = s.reserve_value_local(f);
        let element = s.reserve_value_local(f);
        let number = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let index = s.reserve_i64_local(f);
        let count = s.reserve_i64_local(f);
        let slot = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let source = self.emit_typed_array_species_length_source(
            crate::objects::TypedArraySpeciesLengthMethod::Filter,
            &receiver,
            f,
        )?;
        self.emit_builtin_arg_to_value(0, &callback, f);
        self.emit_is_callable_i32(&callback, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_FILTER_CALLBACK_IS_NOT_CALLABLE,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_builtin_arg_to_value(1, &this_arg, f);
        let kept = crate::functions::ArgumentListConstruction::new(self, f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        f.instruction(&Instruction::I64Const(0));
        count.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        source.length_local().load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(finished, f);
        self.emit_typed_array_element_read_from_locals(source.array(), index, &element, f)?;
        self.emit_array_native_number(index, &number, f);
        let argv = self.emit_pre_evaluated_arg_vector(&[&element, &number, &receiver], f);
        self.emit_function_or_proxy_call_with_argv(&callback, &this_arg, &argv, &pending, f)?;
        argv.clear(f);
        self.emit_array_native_propagate(&pending, f);
        self.compile_truthy_tagged_i32(pending.value(), f)?;
        self.open_frame(ControlFrameKind::If, f);
        kept.append(&element, s, f);
        self.emit_increment_local(count, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // Species access follows every callback. The private List owns the
        // original elements, even when callbacks mutate the source.
        let kept = kept.finish(self, f);
        let target = self.emit_typed_array_species_create_with_length(&source, count, f)?;
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(finished, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        self.emit_argument_vector_entry_to_value(&kept, slot, &element, f);
        self.emit_typed_array_species_element_write(&target, index, &element, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_publish_typed_array_species_result(target, self.completion(), f);
        kept.clear(f);
        source.clear(s, f);
        s.release_i32_local(slot, f);
        s.release_i64_local(count, f);
        s.release_i64_local(index, f);
        pending.clear(f);
        number.clear(f);
        element.clear(f);
        this_arg.clear(f);
        callback.clear(f);
        receiver.clear(f);
        Ok(())
    }
    pub(crate) fn compile_typed_array_prototype_every_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_typed_array_quantifier_builtin(TypedArrayQuantifierKind::Every, f)
    }
    pub(crate) fn compile_typed_array_prototype_some_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_typed_array_quantifier_builtin(TypedArrayQuantifierKind::Some, f)
    }
    fn compile_typed_array_quantifier_builtin(
        &mut self,
        kind: TypedArrayQuantifierKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let callback = s.reserve_value_local(f);
        let this_arg = s.reserve_value_local(f);
        let element = s.reserve_value_local(f);
        let number = s.reserve_value_local(f);
        let answer = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let truth = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let array = self.emit_array_native_typed_receiver(
            &receiver,
            length,
            match kind {
                TypedArrayQuantifierKind::Every => {
                    RuntimeErrorMessage::TYPEDARRAY_EVERY_METHOD_REQUIRES_A_TYPEDARRAY
                }
                TypedArrayQuantifierKind::Some => {
                    RuntimeErrorMessage::TYPEDARRAY_SOME_METHOD_REQUIRES_A_TYPEDARRAY
                }
            },
            &pending,
            f,
        )?;
        self.emit_builtin_arg_to_value(0, &callback, f);
        self.emit_is_callable_i32(&callback, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            match kind {
                TypedArrayQuantifierKind::Every => {
                    RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_EVERY_CALLBACK_IS_NOT_CALLABLE
                }
                TypedArrayQuantifierKind::Some => {
                    RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SOME_CALLBACK_IS_NOT_CALLABLE
                }
            },
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_builtin_arg_to_value(1, &this_arg, f);
        answer.set_scalar(
            ScalarValue::Boolean(matches!(kind, TypedArrayQuantifierKind::Every)),
            f,
        );
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(finished, f);
        self.emit_typed_array_element_read_from_locals(&array, index, &element, f)?;
        self.emit_array_native_number(index, &number, f);
        let argv = self.emit_pre_evaluated_arg_vector(&[&element, &number, &receiver], f);
        self.emit_function_or_proxy_call_with_argv(&callback, &this_arg, &argv, &pending, f)?;
        argv.clear(f);
        self.emit_array_native_propagate(&pending, f);
        self.compile_truthy_tagged_i32(pending.value(), f)?;
        truth.store(f);
        truth.load(f);
        if matches!(kind, TypedArrayQuantifierKind::Every) {
            f.instruction(&Instruction::I32Eqz);
        }
        self.open_frame(ControlFrameKind::If, f);
        answer.set_boolean(truth, f);
        self.emit_branch_to_target(finished, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&answer, f);
        array.clear(f);
        s.release_i32_local(truth, f);
        s.release_i64_local(index, f);
        s.release_i64_local(length, f);
        pending.clear(f);
        answer.clear(f);
        number.clear(f);
        element.clear(f);
        this_arg.clear(f);
        callback.clear(f);
        receiver.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_array_prototype_reverse_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let lower_value = s.reserve_value_local(f);
        let upper_value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let lower = s.reserve_i64_local(f);
        let upper = s.reserve_i64_local(f);
        let middle = s.reserve_i64_local(f);
        let lower_exists = s.reserve_i32_local(f);
        let upper_exists = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?;
        length.load(f);
        f.instruction(&Instruction::I64Const(2));
        f.instruction(&Instruction::I64DivU);
        middle.store(f);
        f.instruction(&Instruction::I64Const(0));
        lower.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        lower.load(f);
        middle.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(finished, f);
        length.load(f);
        lower.load(f);
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        upper.store(f);
        self.emit_array_native_has_index(&receiver, lower, lower_exists, f)?;
        lower_exists.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_typed_array_or_object_index_read_from_locals(&receiver, lower, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        lower_value.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_array_native_has_index(&receiver, upper, upper_exists, f)?;
        upper_exists.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_typed_array_or_object_index_read_from_locals(&receiver, upper, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        upper_value.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        lower_exists.load(f);
        self.open_frame(ControlFrameKind::If, f);
        upper_exists.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_array_native_set_index(&receiver, lower, &upper_value, &pending, f)?;
        self.emit_array_native_set_index(&receiver, upper, &lower_value, &pending, f)?;
        f.instruction(&Instruction::Else);
        self.emit_array_native_delete_index(&receiver, lower, &pending, f)?;
        self.emit_array_native_set_index(&receiver, upper, &lower_value, &pending, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        upper_exists.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_array_native_set_index(&receiver, lower, &upper_value, &pending, f)?;
        self.emit_array_native_delete_index(&receiver, upper, &pending, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_increment_local(lower, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&receiver, f);
        s.release_i32_local(upper_exists, f);
        s.release_i32_local(lower_exists, f);
        for local in [middle, upper, lower, length] {
            s.release_i64_local(local, f);
        }
        pending.clear(f);
        upper_value.clear(f);
        lower_value.clear(f);
        receiver.clear(f);
        Ok(())
    }
    pub(crate) fn compile_array_prototype_copy_within_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let argument = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let relative = s.reserve_i64_local(f);
        let from = s.reserve_i64_local(f);
        let to = s.reserve_i64_local(f);
        let end = s.reserve_i64_local(f);
        let count = s.reserve_i64_local(f);
        let direction = s.reserve_i64_local(f);
        let present = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?;
        for (argument_index, output) in [(0, to), (1, from)] {
            self.emit_builtin_arg_to_value(argument_index, &argument, f);
            self.emit_value_to_number_payload(&argument, &pending, f)?;
            self.emit_array_native_propagate(&pending, f);
            self.emit_to_integer_or_infinity_number_payload_from_number_payload(
                pending.value().scalar(),
                relative,
                f,
            );
            self.emit_array_slice_clamped_index(relative, length, output, f);
        }
        length.load(f);
        end.store(f);
        self.emit_builtin_arg_to_value(2, &argument, f);
        argument.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_number_payload(&argument, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            relative,
            f,
        );
        self.emit_array_slice_clamped_index(relative, length, end, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I64Const(0));
        count.store(f);
        end.load(f);
        from.load(f);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        end.load(f);
        from.load(f);
        f.instruction(&Instruction::I64Sub);
        count.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        length.load(f);
        to.load(f);
        f.instruction(&Instruction::I64Sub);
        relative.store(f);
        relative.load(f);
        count.load(f);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        relative.load(f);
        count.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_array_copy_within_traversal_start(
            ArrayCopyWithinDirection::Forward,
            from,
            to,
            count,
            direction,
            f,
        );
        from.load(f);
        to.load(f);
        f.instruction(&Instruction::I64LtU);
        to.load(f);
        from.load(f);
        count.load(f);
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_array_copy_within_traversal_start(
            ArrayCopyWithinDirection::Backward,
            from,
            to,
            count,
            direction,
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        count.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(finished, f);
        self.emit_array_native_has_index(&receiver, from, present, f)?;
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_typed_array_or_object_index_read_from_locals(&receiver, from, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        argument.copy_from(pending.value(), f);
        self.emit_array_native_set_index(&receiver, to, &argument, &pending, f)?;
        f.instruction(&Instruction::Else);
        self.emit_array_native_delete_index(&receiver, to, &pending, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        from.load(f);
        direction.load(f);
        f.instruction(&Instruction::I64Add);
        from.store(f);
        to.load(f);
        direction.load(f);
        f.instruction(&Instruction::I64Add);
        to.store(f);
        self.emit_increment_local(count, -1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&receiver, f);
        s.release_i32_local(present, f);
        for local in [direction, count, end, to, from, relative, length] {
            s.release_i64_local(local, f);
        }
        pending.clear(f);
        argument.clear(f);
        receiver.clear(f);
        Ok(())
    }
    pub(crate) fn compile_array_prototype_to_reversed_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let target = s.reserve_value_local(f);
        let element = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let source_index = s.reserve_i64_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?;
        self.emit_array_native_create(length, &target, f)?;
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(finished, f);
        length.load(f);
        index.load(f);
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        source_index.store(f);
        self.emit_typed_array_or_object_index_read_from_locals(
            &receiver,
            source_index,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        element.copy_from(pending.value(), f);
        let key = self.emit_array_native_index_key(index, f)?;
        self.emit_create_data_property_or_throw(&target, &key, &element, &pending, f)?;
        key.clear(f);
        self.emit_array_native_propagate(&pending, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&target, f);
        for local in [source_index, index, length] {
            s.release_i64_local(local, f);
        }
        pending.clear(f);
        element.clear(f);
        target.clear(f);
        receiver.clear(f);
        Ok(())
    }
    pub(crate) fn compile_array_prototype_with_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let target = s.reserve_value_local(f);
        let replacement = s.reserve_value_local(f);
        let element = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let selected = s.reserve_i64_local(f);
        let relative = s.reserve_i64_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?;
        self.emit_builtin_arg_to_value(0, &element, f);
        self.emit_value_to_number_payload(&element, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            relative,
            f,
        );
        // Compare before integer conversion so either infinity follows the
        // ordinary called-Realm RangeError route instead of a Wasm trap.
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::F64Ge);
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::F64Neg);
        f.instruction(&Instruction::F64Lt);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_range_error(
            RuntimeErrorMessage::ARRAY_PROTOTYPE_WITH_INDEX_OUT_OF_RANGE,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncF64S);
        selected.store(f);
        selected.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, f);
        selected.load(f);
        length.load(f);
        f.instruction(&Instruction::I64Add);
        selected.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_builtin_arg_to_value(1, &replacement, f);
        self.emit_array_native_create(length, &target, f)?;
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(finished, f);
        index.load(f);
        selected.load(f);
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        element.copy_from(&replacement, f);
        f.instruction(&Instruction::Else);
        self.emit_typed_array_or_object_index_read_from_locals(&receiver, index, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        element.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let key = self.emit_array_native_index_key(index, f)?;
        self.emit_create_data_property_or_throw(&target, &key, &element, &pending, f)?;
        key.clear(f);
        self.emit_array_native_propagate(&pending, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&target, f);
        for local in [relative, selected, index, length] {
            s.release_i64_local(local, f);
        }
        pending.clear(f);
        element.clear(f);
        replacement.clear(f);
        target.clear(f);
        receiver.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_array_prototype_sort_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_sort_with_output(ArraySortOutput::Receiver, f)
    }
    pub(crate) fn compile_array_prototype_to_sorted_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_sort_with_output(ArraySortOutput::Copy, f)
    }
    fn compile_array_sort_with_output(
        &mut self,
        output: ArraySortOutput,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let comparator = s.reserve_value_local(f);
        let element = s.reserve_value_local(f);
        let target = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let count = s.reserve_i64_local(f);
        let slot = s.reserve_i32_local(f);
        let present = s.reserve_i32_local(f);
        // The compare function is admitted before ToObject and length access.
        self.emit_builtin_arg_to_value(0, &comparator, f);
        comparator.tag().load(f);
        f.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_callable_i32(&comparator, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::VALUE_IS_NOT_CALLABLE,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?;
        match output {
            ArraySortOutput::Receiver => target.copy_from(&receiver, f),
            ArraySortOutput::Copy => self.emit_array_native_create(length, &target, f)?,
        }
        let collected = crate::functions::ArgumentListConstruction::new(self, f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        f.instruction(&Instruction::I64Const(0));
        count.store(f);
        let gathered = self.open_frame(ControlFrameKind::Block, f);
        let gather = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(gathered, f);
        match output {
            ArraySortOutput::Receiver => {
                self.emit_array_native_has_index(&receiver, index, present, f)?
            }
            ArraySortOutput::Copy => {
                f.instruction(&Instruction::I32Const(1));
                present.store(f);
            }
        }
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_typed_array_or_object_index_read_from_locals(&receiver, index, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        element.copy_from(pending.value(), f);
        collected.append(&element, s, f);
        self.emit_increment_local(count, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(gather, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let list = collected.finish(self, f);
        let list = self.emit_array_private_values_sort(list, count, &comparator, &pending, f)?;
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let written = self.open_frame(ControlFrameKind::Block, f);
        let write = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(written, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        self.emit_argument_vector_entry_to_value(&list, slot, &element, f);
        match output {
            ArraySortOutput::Receiver => {
                self.emit_array_native_set_index(&target, index, &element, &pending, f)?
            }
            ArraySortOutput::Copy => {
                let key = self.emit_array_native_index_key(index, f)?;
                self.emit_create_data_property_or_throw(&target, &key, &element, &pending, f)?;
                key.clear(f);
                self.emit_array_native_propagate(&pending, f);
            }
        }
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(write, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        if matches!(output, ArraySortOutput::Receiver) {
            let deleted = self.open_frame(ControlFrameKind::Block, f);
            let delete = self.open_frame(ControlFrameKind::Loop, f);
            index.load(f);
            length.load(f);
            f.instruction(&Instruction::I64GeU);
            self.emit_branch_if_to_target(deleted, f);
            self.emit_array_native_delete_index(&target, index, &pending, f)?;
            self.emit_increment_local(index, 1, f);
            self.emit_branch_to_target(delete, f);
            self.pop_control(ControlFrameKind::Loop);
            f.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        }
        self.completion().set_normal(&target, f);
        list.clear(f);
        s.release_i32_local(present, f);
        s.release_i32_local(slot, f);
        s.release_i64_local(count, f);
        s.release_i64_local(index, f);
        s.release_i64_local(length, f);
        pending.clear(f);
        target.clear(f);
        element.clear(f);
        comparator.clear(f);
        receiver.clear(f);
        Ok(())
    }
    fn emit_array_sort_greater(
        &mut self,
        left_snapshot: &GcLocal<StoredValue>,
        right_snapshot: &GcLocal<StoredValue>,
        comparator: &ValueLocals,
        greater: I32Local,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let left = s.reserve_value_local(f);
        let right = s.reserve_value_local(f);
        self.emit_stored_value_to_locals(left_snapshot, &left, f);
        self.emit_stored_value_to_locals(right_snapshot, &right, f);
        let comparison = s.reserve_completion(f);
        let undefined = s.reserve_value_local(f);
        let numeric = s.reserve_value_local(f);
        undefined.set_undefined(f);
        left.tag().load(f);
        f.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        right.tag().load(f);
        f.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        f.instruction(&Instruction::I32Ne);
        greater.store(f);
        f.instruction(&Instruction::Else);
        right.tag().load(f);
        f.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I32Const(0));
        greater.store(f);
        f.instruction(&Instruction::Else);
        comparator.tag().load(f);
        f.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        // Snapshots are immutable and already rooted by the private sort.
        // Keep the argument vector fresh: callbacks can retain or mutate their
        // arguments and reenter sorting without changing the source snapshots.
        let argv = self.emit_argument_vector_from_snapshots([left_snapshot, right_snapshot], f);
        self.emit_function_or_proxy_call_with_argv(comparator, &undefined, &argv, &comparison, f)?;
        argv.clear(f);
        self.emit_array_native_propagate(&comparison, f);
        numeric.copy_from(comparison.value(), f);
        self.emit_value_to_number_payload(&numeric, pending, f)?;
        self.emit_array_native_propagate(pending, f);
        pending.value().scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Gt);
        greater.store(f);
        f.instruction(&Instruction::Else);
        self.emit_value_to_string_payload(&left, &comparison, f)?;
        self.emit_array_native_propagate(&comparison, f);
        let l = s.reserve_gc_local(f).initialize(
            comparison
                .value()
                .cast_reference::<crate::gc_types::StringValue>(s, f),
            f,
        );
        self.emit_value_to_string_payload(&right, &comparison, f)?;
        self.emit_array_native_propagate(&comparison, f);
        let r = s.reserve_gc_local(f).initialize(
            comparison
                .value()
                .cast_reference::<crate::gc_types::StringValue>(s, f),
            f,
        );
        self.emit_string_payload_utf16_compare_i32(&l, &r, f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GtS);
        greater.store(f);
        r.clear(f);
        l.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        numeric.clear(f);
        undefined.clear(f);
        comparison.clear(f);
        right.clear(f);
        left.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_array_prototype_concat_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let item = s.reserve_value_local(f);
        let target = s.reserve_value_local(f);
        let element = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let count = s.reserve_i64_local(f);
        let argument = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let length = s.reserve_i64_local(f);
        let output = s.reserve_i64_local(f);
        let slot = s.reserve_i32_local(f);
        let spread = s.reserve_i32_local(f);
        let present = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_value_to_object_locals(&receiver, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        receiver.copy_from(pending.value(), f);
        f.instruction(&Instruction::I64Const(0));
        output.store(f);
        self.emit_array_species_create(&receiver, output, &target, f)?;
        let entry = self
            .body_entry_locals()
            .expect("concat owns a native entry");
        entry.argument_count().load(f);
        count.store(f);
        let argv = s
            .reserve_gc_local(f)
            .initialize(entry.arguments().load(s, f), f);
        let symbol = s.reserve_gc_local(f).initialize(
            self.emit_well_known_symbol_reference(
                scriptw_ir::WellKnownSymbol::IsConcatSpreadable,
                f,
            )?,
            f,
        );
        let spread_key = crate::operations::PropertyKeyLocals::from_symbol(s, &symbol, f);
        symbol.clear(f);
        f.instruction(&Instruction::I64Const(0));
        argument.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next_item = self.open_frame(ControlFrameKind::Loop, f);
        argument.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GtU);
        self.emit_branch_if_to_target(finished, f);
        argument.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        item.copy_from(&receiver, f);
        f.instruction(&Instruction::Else);
        argument.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        self.emit_argument_vector_entry_to_value(&argv, slot, &item, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I32Const(0));
        spread.store(f);
        self.emit_is_heap_object_like_tag_i32(item.tag(), f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_object_read(&item, &item, &spread_key, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        pending.value().tag().load(f);
        f.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_array_i32(&item, spread, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        f.instruction(&Instruction::Else);
        self.compile_truthy_tagged_i32(pending.value(), f)?;
        spread.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        spread.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_array_like_length_snapshot(&item, length, &pending, f)?;
        length.load(f);
        f.instruction(&Instruction::I64Const(9_007_199_254_740_991));
        output.load(f);
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::ARRAY_PROTOTYPE_CONCAT_RESULT_EXCEEDS_MAXIMUM_SAFE_LENGTH,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let copied = self.open_frame(ControlFrameKind::Block, f);
        let copy = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(copied, f);
        self.emit_array_native_has_index(&item, index, present, f)?;
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_typed_array_or_object_index_read_from_locals(&item, index, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        element.copy_from(pending.value(), f);
        let key = self.emit_array_native_index_key(output, f)?;
        self.emit_create_data_property_or_throw(&target, &key, &element, &pending, f)?;
        key.clear(f);
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_increment_local(index, 1, f);
        self.emit_increment_local(output, 1, f);
        self.emit_branch_to_target(copy, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        output.load(f);
        f.instruction(&Instruction::I64Const(9_007_199_254_740_991));
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::ARRAY_PROTOTYPE_CONCAT_RESULT_EXCEEDS_MAXIMUM_SAFE_LENGTH,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let key = self.emit_array_native_index_key(output, f)?;
        self.emit_create_data_property_or_throw(&target, &key, &item, &pending, f)?;
        key.clear(f);
        self.emit_array_native_propagate(&pending, f);
        self.emit_increment_local(output, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_increment_local(argument, 1, f);
        self.emit_branch_to_target(next_item, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_array_native_set_length(
            &target,
            output,
            &pending,
            RuntimeErrorMessage::CANNOT_ASSIGN_TO_READ_ONLY_PROPERTY,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.completion().set_normal(&target, f);
        spread_key.clear(f);
        argv.clear(f);
        s.release_i32_local(present, f);
        s.release_i32_local(spread, f);
        s.release_i32_local(slot, f);
        s.release_i64_local(output, f);
        s.release_i64_local(length, f);
        s.release_i64_local(index, f);
        s.release_i64_local(argument, f);
        s.release_i64_local(count, f);
        pending.clear(f);
        element.clear(f);
        target.clear(f);
        item.clear(f);
        receiver.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_array_prototype_splice_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_splice_output(ArraySpliceOutput::Receiver, f)
    }
    pub(crate) fn compile_array_prototype_to_spliced_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_splice_output(ArraySpliceOutput::Copy, f)
    }
    fn compile_array_splice_output(
        &mut self,
        mode: ArraySpliceOutput,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let argument = s.reserve_value_local(f);
        let element = s.reserve_value_local(f);
        let target = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let argc = s.reserve_i64_local(f);
        let start = s.reserve_i64_local(f);
        let delete_count = s.reserve_i64_local(f);
        let item_count = s.reserve_i64_local(f);
        let new_length = s.reserve_i64_local(f);
        let available = s.reserve_i64_local(f);
        let relative = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let from = s.reserve_i64_local(f);
        let to = s.reserve_i64_local(f);
        let output = s.reserve_i64_local(f);
        let slot = s.reserve_i32_local(f);
        let present = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?;
        self.emit_builtin_arg_to_value(0, &argument, f);
        self.emit_value_to_number_payload(&argument, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            relative,
            f,
        );
        self.emit_array_slice_clamped_index(relative, length, start, f);
        let entry = self
            .body_entry_locals()
            .expect("splice owns a native entry");
        entry.argument_count().load(f);
        argc.store(f);
        let argv = s
            .reserve_gc_local(f)
            .initialize(entry.arguments().load(s, f), f);
        length.load(f);
        start.load(f);
        f.instruction(&Instruction::I64Sub);
        available.store(f);
        f.instruction(&Instruction::I64Const(0));
        item_count.store(f);
        f.instruction(&Instruction::I64Const(0));
        delete_count.store(f);
        argc.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        available.load(f);
        delete_count.store(f);
        f.instruction(&Instruction::Else);
        argc.load(f);
        f.instruction(&Instruction::I64Const(2));
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        argc.load(f);
        f.instruction(&Instruction::I64Const(2));
        f.instruction(&Instruction::I64Sub);
        item_count.store(f);
        self.emit_builtin_arg_to_value(1, &argument, f);
        self.emit_value_to_number_payload(&argument, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            relative,
            f,
        );
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Gt);
        self.open_frame(ControlFrameKind::If, f);
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        available.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::F64Ge);
        self.open_frame(ControlFrameKind::If, f);
        available.load(f);
        delete_count.store(f);
        f.instruction(&Instruction::Else);
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncF64U);
        delete_count.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        length.load(f);
        delete_count.load(f);
        f.instruction(&Instruction::I64Sub);
        new_length.store(f);
        item_count.load(f);
        f.instruction(&Instruction::I64Const(9_007_199_254_740_991));
        new_length.load(f);
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(match mode { ArraySpliceOutput::Receiver => RuntimeErrorMessage::ARRAY_PROTOTYPE_PUSH_LENGTH_EXCEEDS_SAFE_INTEGER, ArraySpliceOutput::Copy => RuntimeErrorMessage::ARRAY_PROTOTYPE_TOSPLICED_RESULT_EXCEEDS_MAXIMUM_SAFE_LENGTH }, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        new_length.load(f);
        item_count.load(f);
        f.instruction(&Instruction::I64Add);
        new_length.store(f);
        match mode {
            ArraySpliceOutput::Receiver => {
                self.emit_array_species_create(&receiver, delete_count, &target, f)?;
                f.instruction(&Instruction::I64Const(0));
                index.store(f);
                let copied = self.open_frame(ControlFrameKind::Block, f);
                let copy = self.open_frame(ControlFrameKind::Loop, f);
                index.load(f);
                delete_count.load(f);
                f.instruction(&Instruction::I64GeU);
                self.emit_branch_if_to_target(copied, f);
                start.load(f);
                index.load(f);
                f.instruction(&Instruction::I64Add);
                from.store(f);
                self.emit_array_native_has_index(&receiver, from, present, f)?;
                present.load(f);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_typed_array_or_object_index_read_from_locals(
                    &receiver, from, &pending, f,
                )?;
                self.emit_array_native_propagate(&pending, f);
                element.copy_from(pending.value(), f);
                let key = self.emit_array_native_index_key(index, f)?;
                self.emit_create_data_property_or_throw(&target, &key, &element, &pending, f)?;
                key.clear(f);
                self.emit_array_native_propagate(&pending, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                self.emit_increment_local(index, 1, f);
                self.emit_branch_to_target(copy, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                self.emit_array_native_set_length(
                    &target,
                    delete_count,
                    &pending,
                    RuntimeErrorMessage::CANNOT_ASSIGN_TO_READ_ONLY_PROPERTY,
                    f,
                )?;
                self.emit_array_native_propagate(&pending, f);
                item_count.load(f);
                delete_count.load(f);
                f.instruction(&Instruction::I64LtU);
                self.open_frame(ControlFrameKind::If, f);
                start.load(f);
                index.store(f);
                let shifted = self.open_frame(ControlFrameKind::Block, f);
                let shift = self.open_frame(ControlFrameKind::Loop, f);
                index.load(f);
                length.load(f);
                delete_count.load(f);
                f.instruction(&Instruction::I64Sub);
                f.instruction(&Instruction::I64GeU);
                self.emit_branch_if_to_target(shifted, f);
                index.load(f);
                delete_count.load(f);
                f.instruction(&Instruction::I64Add);
                from.store(f);
                index.load(f);
                item_count.load(f);
                f.instruction(&Instruction::I64Add);
                to.store(f);
                self.emit_array_splice_move_property(
                    &receiver, from, to, present, &element, &pending, f,
                )?;
                self.emit_increment_local(index, 1, f);
                self.emit_branch_to_target(shift, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                length.load(f);
                index.store(f);
                let deleted = self.open_frame(ControlFrameKind::Block, f);
                let delete = self.open_frame(ControlFrameKind::Loop, f);
                index.load(f);
                new_length.load(f);
                f.instruction(&Instruction::I64LeU);
                self.emit_branch_if_to_target(deleted, f);
                self.emit_increment_local(index, -1, f);
                self.emit_array_native_delete_index(&receiver, index, &pending, f)?;
                self.emit_branch_to_target(delete, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                f.instruction(&Instruction::Else);
                item_count.load(f);
                delete_count.load(f);
                f.instruction(&Instruction::I64GtU);
                self.open_frame(ControlFrameKind::If, f);
                length.load(f);
                delete_count.load(f);
                f.instruction(&Instruction::I64Sub);
                index.store(f);
                let shifted = self.open_frame(ControlFrameKind::Block, f);
                let shift = self.open_frame(ControlFrameKind::Loop, f);
                index.load(f);
                start.load(f);
                f.instruction(&Instruction::I64LeU);
                self.emit_branch_if_to_target(shifted, f);
                self.emit_increment_local(index, -1, f);
                index.load(f);
                delete_count.load(f);
                f.instruction(&Instruction::I64Add);
                from.store(f);
                index.load(f);
                item_count.load(f);
                f.instruction(&Instruction::I64Add);
                to.store(f);
                self.emit_array_splice_move_property(
                    &receiver, from, to, present, &element, &pending, f,
                )?;
                self.emit_branch_to_target(shift, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                start.load(f);
                output.store(f);
            }
            ArraySpliceOutput::Copy => {
                self.emit_array_native_create(new_length, &target, f)?;
                f.instruction(&Instruction::I64Const(0));
                index.store(f);
                f.instruction(&Instruction::I64Const(0));
                output.store(f);
                let copied = self.open_frame(ControlFrameKind::Block, f);
                let copy = self.open_frame(ControlFrameKind::Loop, f);
                index.load(f);
                start.load(f);
                f.instruction(&Instruction::I64GeU);
                self.emit_branch_if_to_target(copied, f);
                self.emit_array_splice_copy_value(
                    &receiver, index, &target, output, &element, &pending, f,
                )?;
                self.emit_increment_local(index, 1, f);
                self.emit_increment_local(output, 1, f);
                self.emit_branch_to_target(copy, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
            }
        }
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let inserted = self.open_frame(ControlFrameKind::Block, f);
        let insert = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        item_count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(inserted, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(2));
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        self.emit_argument_vector_entry_to_value(&argv, slot, &element, f);
        match mode {
            ArraySpliceOutput::Receiver => {
                self.emit_array_native_set_index(&receiver, output, &element, &pending, f)?
            }
            ArraySpliceOutput::Copy => {
                let key = self.emit_array_native_index_key(output, f)?;
                self.emit_create_data_property_or_throw(&target, &key, &element, &pending, f)?;
                key.clear(f);
                self.emit_array_native_propagate(&pending, f);
            }
        }
        self.emit_increment_local(index, 1, f);
        self.emit_increment_local(output, 1, f);
        self.emit_branch_to_target(insert, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        match mode {
            ArraySpliceOutput::Receiver => {
                self.emit_array_native_set_length(
                    &receiver,
                    new_length,
                    &pending,
                    RuntimeErrorMessage::CANNOT_ASSIGN_TO_READ_ONLY_PROPERTY,
                    f,
                )?;
                self.emit_array_native_propagate(&pending, f);
            }
            ArraySpliceOutput::Copy => {
                start.load(f);
                delete_count.load(f);
                f.instruction(&Instruction::I64Add);
                index.store(f);
                let copied = self.open_frame(ControlFrameKind::Block, f);
                let copy = self.open_frame(ControlFrameKind::Loop, f);
                index.load(f);
                length.load(f);
                f.instruction(&Instruction::I64GeU);
                self.emit_branch_if_to_target(copied, f);
                self.emit_array_splice_copy_value(
                    &receiver, index, &target, output, &element, &pending, f,
                )?;
                self.emit_increment_local(index, 1, f);
                self.emit_increment_local(output, 1, f);
                self.emit_branch_to_target(copy, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
            }
        }
        self.completion().set_normal(&target, f);
        argv.clear(f);
        s.release_i32_local(present, f);
        s.release_i32_local(slot, f);
        s.release_i64_local(output, f);
        s.release_i64_local(to, f);
        s.release_i64_local(from, f);
        s.release_i64_local(index, f);
        s.release_i64_local(relative, f);
        s.release_i64_local(available, f);
        s.release_i64_local(new_length, f);
        s.release_i64_local(item_count, f);
        s.release_i64_local(delete_count, f);
        s.release_i64_local(start, f);
        s.release_i64_local(argc, f);
        s.release_i64_local(length, f);
        pending.clear(f);
        target.clear(f);
        element.clear(f);
        argument.clear(f);
        receiver.clear(f);
        Ok(())
    }
    fn emit_array_splice_move_property(
        &mut self,
        receiver: &ValueLocals,
        from: I64Local,
        to: I64Local,
        present: I32Local,
        element: &ValueLocals,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_array_native_has_index(receiver, from, present, f)?;
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_typed_array_or_object_index_read_from_locals(receiver, from, pending, f)?;
        self.emit_array_native_propagate(pending, f);
        element.copy_from(pending.value(), f);
        self.emit_array_native_set_index(receiver, to, element, pending, f)?;
        f.instruction(&Instruction::Else);
        self.emit_array_native_delete_index(receiver, to, pending, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    fn emit_array_splice_copy_value(
        &mut self,
        receiver: &ValueLocals,
        from: I64Local,
        target: &ValueLocals,
        to: I64Local,
        element: &ValueLocals,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_typed_array_or_object_index_read_from_locals(receiver, from, pending, f)?;
        self.emit_array_native_propagate(pending, f);
        element.copy_from(pending.value(), f);
        let key = self.emit_array_native_index_key(to, f)?;
        self.emit_create_data_property_or_throw(target, &key, element, pending, f)?;
        key.clear(f);
        self.emit_array_native_propagate(pending, f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_array_prototype_join_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_string_operation(
            ToLocaleStringReceiverKind::ArrayLike,
            ArrayStringOperation::Join,
            f,
        )
    }
    pub(crate) fn compile_typed_array_prototype_join_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_string_operation(
            ToLocaleStringReceiverKind::TypedArray,
            ArrayStringOperation::Join,
            f,
        )
    }
    pub(crate) fn compile_array_prototype_to_locale_string_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_string_operation(
            ToLocaleStringReceiverKind::ArrayLike,
            ArrayStringOperation::Locale,
            f,
        )
    }
    pub(crate) fn compile_typed_array_prototype_to_locale_string_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_array_string_operation(
            ToLocaleStringReceiverKind::TypedArray,
            ArrayStringOperation::Locale,
            f,
        )
    }
    fn compile_array_string_operation(
        &mut self,
        receiver_kind: ToLocaleStringReceiverKind,
        operation: ArrayStringOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let argument = s.reserve_value_local(f);
        let element = s.reserve_value_local(f);
        let method = s.reserve_value_local(f);
        let options = s.reserve_value_local(f);
        let result = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        match &receiver_kind {
            ToLocaleStringReceiverKind::ArrayLike => {
                self.emit_array_like_length_snapshot(&receiver, length, &pending, f)?
            }
            ToLocaleStringReceiverKind::TypedArray => {
                let array = self.emit_array_native_typed_receiver(&receiver, length, match operation { ArrayStringOperation::Join => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_JOIN_REQUIRES_A_TYPEDARRAY, ArrayStringOperation::Locale => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_TOLOCALESTRING_REQUIRES_TYPEDARRAY }, &pending, f)?;
                array.clear(f);
            }
        }
        let separator = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference(",", f)?, f);
        match operation {
            ArrayStringOperation::Join => {
                self.emit_builtin_arg_to_value(0, &argument, f);
                argument.tag().load(f);
                f.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
                f.instruction(&Instruction::I32Ne);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_value_to_string_payload(&argument, &pending, f)?;
                self.emit_array_native_propagate(&pending, f);
                separator.replace(
                    pending
                        .value()
                        .cast_reference::<crate::gc_types::StringValue>(s, f),
                    f,
                );
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
            ArrayStringOperation::Locale => {
                self.emit_builtin_arg_to_value(0, &argument, f);
                self.emit_builtin_arg_to_value(1, &options, f);
            }
        }
        let joined = self.open_frame(ControlFrameKind::Block, f);
        if matches!(operation, ArrayStringOperation::Join) {
            // The captured zero length needs no indexed reads even if separator
            // conversion changed the receiver. Conversion has already finished.
            length.load(f);
            f.instruction(&Instruction::I64Eqz);
            self.open_frame(ControlFrameKind::If, f);
            let empty = s
                .reserve_gc_local(f)
                .initialize(self.emit_interned_string_reference("", f)?, f);
            result.set_reference(&empty, s, f);
            empty.clear(f);
            self.emit_branch_to_target(joined, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        if matches!(operation, ArrayStringOperation::Join) {
            match &receiver_kind {
                ToLocaleStringReceiverKind::ArrayLike => {
                    let handled =
                        self.emit_try_packed_array_join(&receiver, length, &separator, &result, f)?;
                    handled.load(f);
                    s.release_i32_local(handled, f);
                    self.emit_branch_if_to_target(joined, f);
                }
                ToLocaleStringReceiverKind::TypedArray => {}
            }
        }
        let text = string_buffer::ArrayStringBuffer::new(s, f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(finished, f);
        index.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        text.append(&separator, s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_typed_array_or_object_index_read_from_locals(&receiver, index, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        element.copy_from(pending.value(), f);
        element.tag().load(f);
        f.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        f.instruction(&Instruction::I32Ne);
        element.tag().load(f);
        f.instruction(&Instruction::I32Const(ValueKind::Null.tag() as i32));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        match operation {
            ArrayStringOperation::Join => {
                self.emit_value_to_string_payload(&element, &pending, f)?
            }
            ArrayStringOperation::Locale => {
                // GetV keeps the element as the getter and method receiver.
                self.emit_array_native_get(&element, "toLocaleString", &pending, f)?;
                self.emit_array_native_propagate(&pending, f);
                method.copy_from(pending.value(), f);
                self.emit_is_callable_i32(&method, f)?;
                f.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_throw_current_function_realm_type_error(match receiver_kind { ToLocaleStringReceiverKind::ArrayLike => RuntimeErrorMessage::ARRAY_PROTOTYPE_TOLOCALESTRING_ELEMENT_METHOD_IS_NOT_CALLABLE, ToLocaleStringReceiverKind::TypedArray => RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_TOLOCALESTRING_ELEMENT_METHOD_IS_NOT_CALLABLE }, &pending, f)?;
                self.emit_array_native_propagate(&pending, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                let argv = self.emit_pre_evaluated_arg_vector(&[&argument, &options], f);
                self.emit_function_or_proxy_call_with_argv(&method, &element, &argv, &pending, f)?;
                argv.clear(f);
                self.emit_array_native_propagate(&pending, f);
                result.copy_from(pending.value(), f);
                self.emit_value_to_string_payload(&result, &pending, f)?;
            }
        }
        self.emit_array_native_propagate(&pending, f);
        let part = s.reserve_gc_local(f).initialize(
            pending
                .value()
                .cast_reference::<crate::gc_types::StringValue>(s, f),
            f,
        );
        text.append(&part, s, f);
        part.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let text = text.finish(s, f);
        result.set_reference(&text, s, f);
        text.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().set_normal(&result, f);
        separator.clear(f);
        s.release_i64_local(index, f);
        s.release_i64_local(length, f);
        pending.clear(f);
        result.clear(f);
        options.clear(f);
        method.clear(f);
        element.clear(f);
        argument.clear(f);
        receiver.clear(f);
        Ok(())
    }
    /// Array.prototype.toString and %TypedArray%.prototype.toString share this
    /// generic native entry and their same per-Realm callable identity.
    pub(crate) fn compile_typed_array_prototype_to_string_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let method = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_value_to_object_locals(&receiver, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        receiver.copy_from(pending.value(), f);
        self.emit_array_native_get(&receiver, "join", &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        method.copy_from(pending.value(), f);
        self.emit_is_callable_i32(&method, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let meta = self
            .functions
            .get(&StandardBuiltinId::ObjectPrototypeToString.function_id())
            .cloned()
            .ok_or_else(|| {
                EmitError::unsupported("Object.prototype.toString fallback was not planned")
            })?;
        let fallback = s
            .reserve_gc_local(f)
            .initialize(self.emit_function_value_payload(&meta, f)?, f);
        method.set_reference(&fallback, s, f);
        fallback.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let argv = self.emit_pre_evaluated_arg_vector(&[], f);
        self.emit_function_or_proxy_call_with_argv(&method, &receiver, &argv, &pending, f)?;
        argv.clear(f);
        self.completion().copy_from(&pending, f);
        pending.clear(f);
        method.clear(f);
        receiver.clear(f);
        Ok(())
    }
}
