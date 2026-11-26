//! A saved actual native push consumes one to three rooted operands directly.
//! Argument storage and ordinary JS dispatch exist only on the decline arm.
use super::*;
use crate::gc_types::{
    ArrayIndexedStorageMode, ArrayObject, ArrayObjectSchema, GcOperand, Nullable,
    NumberBitsI64Array, NumberPresenceU64Array, OrdinaryObjectSchema, PackedValueTable,
    StoredValue, ValueLocals,
};
use crate::runtime_helpers::{
    ArrayPackedPush1Arguments, ArrayPackedPush2Arguments, ArrayPackedPush3Arguments,
};

impl FunctionBuilder<'_> {
    pub(super) fn try_emit_packed_array_push_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        // A tail continuation admits the same route: the guarded native push
        // runs no user code, so only the caller's earlier retirement (stack
        // depth) distinguishes it from the proper tail call its miss keeps.
        if !(1..=8).contains(&arguments.len())
            || arguments.iter().any(|argument| {
                matches!(
                    argument.expr,
                    ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
                )
            })
        {
            return Ok(false);
        }
        let s = self.runtime_schema();
        let mut saved = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let value = s.reserve_value_local(function);
            self.compile_expr_to_value(argument, &value, function)?;
            self.emit_propagate_current_throw_if_needed(function);
            saved.push(value);
        }
        let handled = s.reserve_i32_local(function);
        handled.set_constant(0, function);
        self.emit_with_current_native_builtin(
            callee,
            StandardBuiltinId::ArrayPrototypePush,
            function,
            |builder, function| {
                let pending = s.reserve_completion(function);
                let base = builder.runtime_helper_base()?;
                let emit_chunk = |chunk: &[ValueLocals], function: &mut Function| match chunk {
                    [first] => s
                        .call_helper(
                            ArrayPackedPush1Arguments::new(receiver, first),
                            base,
                            function,
                        )
                        .store(&pending, function),
                    [first, second] => s
                        .call_helper(
                            ArrayPackedPush2Arguments::new(receiver, first, second),
                            base,
                            function,
                        )
                        .store(&pending, function),
                    [first, second, third] => s
                        .call_helper(
                            ArrayPackedPush3Arguments::new(receiver, first, second, third),
                            base,
                            function,
                        )
                        .store(&pending, function),
                    _ => unreachable!("packed push chunks have arity 1..=3"),
                };
                if saved.len() <= 3 {
                    let inline_done = (saved.len() == 1)
                        .then(|| builder.open_frame(ControlFrameKind::Block, function));
                    if let (Some(inline_done), [first]) = (inline_done, saved.as_slice()) {
                        // Fast inline within-capacity append for 1-arg push:
                        // identical admission checks to ArrayPackedPush1, falling
                        // through to the R helper on any decline, growth, or
                        // PackedNumbers -> PackedValues promotion.
                        let inline_miss = builder.open_frame(ControlFrameKind::Block, function);
                        let array = s.reserve_gc_local(function).initialize(
                            receiver.sealed_reference_or_branch::<ArrayObject>(
                                s,
                                builder.local_branch_label(inline_miss),
                                function,
                            ),
                            function,
                        );
                        let captured_length = s.reserve_i64_local(function);
                        s.field(ArrayObjectSchema::LENGTH)
                            .read(&array, s, function)
                            .store_i64(captured_length, function);
                        captured_length.load(function);
                        function.instruction(&Instruction::I64Const(u32::MAX as i64));
                        function.instruction(&Instruction::I64GeU);
                        builder.emit_branch_if_to_target(inline_miss, function);
                        s.field(ArrayObjectSchema::LENGTH_WRITABLE)
                            .read(&array, s, function);
                        function.instruction(&Instruction::I32Eqz);
                        builder.emit_branch_if_to_target(inline_miss, function);
                        s.field(ArrayObjectSchema::COUNT)
                            .read(&array, s, function);
                        captured_length.load(function);
                        function.instruction(&Instruction::I64Ne);
                        builder.emit_branch_if_to_target(inline_miss, function);
                        let header = s.reserve_gc_local(function).initialize(
                            s.field(ArrayObjectSchema::OBJECT)
                                .read(&array, s, function)
                                .reference(),
                            function,
                        );
                        s.field(OrdinaryObjectSchema::EXTENSIBLE)
                            .read(&header, s, function);
                        function.instruction(&Instruction::I32Eqz);
                        builder.emit_branch_if_to_target(inline_miss, function);
                        let prototype = s.reserve_gc_local(function).initialize(
                            s.field(OrdinaryObjectSchema::PROTOTYPE)
                                .read(&header, s, function)
                                .reference(),
                            function,
                        );
                        header.clear(function);
                        builder.emit_array_prototype_chain_without_indices_or_branch(
                            &prototype,
                            inline_miss,
                            function,
                        );
                        prototype.clear(function);
                        let new_length = s.reserve_i64_local(function);
                        captured_length.load(function);
                        function.instruction(&Instruction::I64Const(1));
                        function.instruction(&Instruction::I64Add);
                        new_length.store(function);
                        let slot = s.reserve_i32_local(function);
                        captured_length.load(function);
                        function.instruction(&Instruction::I32WrapI64);
                        slot.store(function);
                        s.array_storage_mode_is(
                            &array,
                            ArrayIndexedStorageMode::PackedValues,
                            function,
                        );
                        builder.open_frame(ControlFrameKind::If, function);
                        let values = s.reserve_gc_local(function).initialize(
                            s.field(ArrayObjectSchema::VALUES)
                                .read(&array, s, function)
                                .reference()
                                .require_non_null(function),
                            function,
                        );
                        new_length.load(function);
                        s.array_type::<PackedValueTable>()
                            .length(&values, s, function);
                        function.instruction(&Instruction::I64ExtendI32U);
                        function.instruction(&Instruction::I64GtU);
                        builder.emit_branch_if_to_target(inline_miss, function);
                        first.tag().load(function);
                        function.instruction(&Instruction::I32Const(
                            ValueKind::Undefined.tag() as i32,
                        ));
                        function.instruction(&Instruction::I32Eq);
                        builder.open_frame(ControlFrameKind::If, function);
                        let stored = s
                            .reserve_gc_local::<StoredValue, Nullable>(function)
                            .initialize(s.stored_undefined(function).nullable(), function);
                        function.instruction(&Instruction::Else);
                        stored.replace(
                            s.struct_type::<StoredValue>()
                                .from_value(first, function)
                                .nullable(),
                            function,
                        );
                        builder.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                        s.array_type::<PackedValueTable>().write(
                            &values,
                            slot,
                            GcOperand::reference(&stored, s),
                            s,
                            function,
                        );
                        stored.clear(function);
                        values.clear(function);
                        function.instruction(&Instruction::Else);
                        s.array_storage_mode_is(
                            &array,
                            ArrayIndexedStorageMode::PackedNumbers,
                            function,
                        );
                        first.tag().load(function);
                        function.instruction(&Instruction::I32Const(
                            ValueKind::Number.tag() as i32,
                        ));
                        function.instruction(&Instruction::I32Eq);
                        function.instruction(&Instruction::I32And);
                        function.instruction(&Instruction::I32Eqz);
                        builder.emit_branch_if_to_target(inline_miss, function);
                        let numbers = s.reserve_gc_local(function).initialize(
                            s.field(ArrayObjectSchema::NUMBERS)
                                .read(&array, s, function)
                                .reference(),
                            function,
                        );
                        new_length.load(function);
                        s.array_type::<NumberBitsI64Array>()
                            .length(&numbers, s, function);
                        function.instruction(&Instruction::I64ExtendI32U);
                        function.instruction(&Instruction::I64GtU);
                        builder.emit_branch_if_to_target(inline_miss, function);
                        let presence = s.reserve_gc_local(function).initialize(
                            s.field(ArrayObjectSchema::NUMBER_PRESENT)
                                .read(&array, s, function)
                                .reference(),
                            function,
                        );
                        s.array_type::<NumberBitsI64Array>().write(
                            &numbers,
                            slot,
                            GcOperand::i64_local(first.scalar()),
                            s,
                            function,
                        );
                        let word = s.reserve_i32_local(function);
                        let bits = s.reserve_i64_local(function);
                        slot.load(function);
                        function.instruction(&Instruction::I32Const(6));
                        function.instruction(&Instruction::I32ShrU);
                        word.store(function);
                        s.array_type::<NumberPresenceU64Array>()
                            .read(&presence, word, s, function)
                            .store_i64(bits, function);
                        bits.load(function);
                        function.instruction(&Instruction::I64Const(1));
                        slot.load(function);
                        function.instruction(&Instruction::I32Const(63));
                        function.instruction(&Instruction::I32And);
                        function.instruction(&Instruction::I64ExtendI32U);
                        function.instruction(&Instruction::I64Shl);
                        function.instruction(&Instruction::I64Or);
                        bits.store(function);
                        s.array_type::<NumberPresenceU64Array>().write(
                            &presence,
                            word,
                            GcOperand::i64_local(bits),
                            s,
                            function,
                        );
                        s.release_i64_local(bits, function);
                        s.release_i32_local(word, function);
                        presence.clear(function);
                        numbers.clear(function);
                        builder.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                        s.field(ArrayObjectSchema::COUNT).write(
                            &array,
                            GcOperand::i64_local(new_length),
                            s,
                            function,
                        );
                        s.field(ArrayObjectSchema::LENGTH).write(
                            &array,
                            GcOperand::i64_local(new_length),
                            s,
                            function,
                        );
                        new_length.load(function);
                        function.instruction(&Instruction::F64ConvertI64U);
                        function.instruction(&Instruction::I64ReinterpretF64);
                        new_length.store(function);
                        output.set_number(new_length, function);
                        handled.set_constant(1, function);
                        s.release_i32_local(slot, function);
                        s.release_i64_local(new_length, function);
                        s.release_i64_local(captured_length, function);
                        array.clear(function);
                        builder.emit_branch_to_target(inline_done, function);
                        builder.pop_control(ControlFrameKind::Block);
                        function.instruction(&Instruction::End);
                    }
                    emit_chunk(&saved, function);
                    pending.kind().load(function);
                    function
                        .instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
                    function.instruction(&Instruction::I32Eq);
                    builder.open_frame(ControlFrameKind::If, function);
                    builder.completion().copy_from(&pending, function);
                    builder.emit_propagate_current_throw(function);
                    builder.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                    // A completed Number is the helper's publication capability;
                    // Undefined means a pure miss before any mutation or user code.
                    pending.value().tag().load(function);
                    function.instruction(&Instruction::I32Const(ValueKind::Number.tag() as i32));
                    function.instruction(&Instruction::I32Eq);
                    builder.open_frame(ControlFrameKind::If, function);
                    output.copy_from(pending.value(), function);
                    handled.set_constant(1, function);
                    builder.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                    if inline_done.is_some() {
                        builder.pop_control(ControlFrameKind::Block);
                        function.instruction(&Instruction::End);
                    }
                } else {
                    // For 4..=8 arguments, require an actual ArrayObject whose
                    // post-push length stays strictly within PACKED_INDEX_LIMIT
                    // and whose prototype chain is the untouched %Array.prototype%
                    // -> %Object.prototype% -> null (zero indexed descriptors at
                    // any index), so once chunk 1 validates storage/extensibility/
                    // length-writability and commits, all remaining chunks are
                    // guaranteed to succeed without partial commits.
                    let skip = builder.open_frame(ControlFrameKind::Block, function);
                    let arr = s.reserve_gc_local(function).initialize(
                        receiver.sealed_reference_or_branch::<crate::gc_types::ArrayObject>(
                            s,
                            builder.local_branch_label(skip),
                            function,
                        ),
                        function,
                    );
                    s.field(crate::gc_types::ArrayObjectSchema::LENGTH)
                        .read(&arr, s, function);
                    function.instruction(&Instruction::I64Const((1i64 << 20) - 8));
                    function.instruction(&Instruction::I64GtU);
                    builder.emit_branch_if_to_target(skip, function);
                    let header = s.reserve_gc_local(function).initialize(
                        s.field(crate::gc_types::ArrayObjectSchema::OBJECT)
                            .read(&arr, s, function)
                            .reference(),
                        function,
                    );
                    arr.clear(function);
                    let prototype = s.reserve_gc_local(function).initialize(
                        s.field(crate::gc_types::OrdinaryObjectSchema::PROTOTYPE)
                            .read(&header, s, function)
                            .reference(),
                        function,
                    );
                    header.clear(function);
                    builder.emit_array_prototype_chain_without_indices_or_branch(
                        &prototype,
                        skip,
                        function,
                    );
                    prototype.clear(function);
                    emit_chunk(&saved[..3], function);
                    pending.value().tag().load(function);
                    function.instruction(&Instruction::I32Const(ValueKind::Number.tag() as i32));
                    function.instruction(&Instruction::I32Ne);
                    builder.emit_branch_if_to_target(skip, function);
                    for chunk in saved[3..].chunks(3) {
                        emit_chunk(chunk, function);
                    }
                    output.copy_from(pending.value(), function);
                    handled.set_constant(1, function);
                    builder.pop_control(ControlFrameKind::Block);
                    function.instruction(&Instruction::End);
                }
                pending.clear(function);
                Ok(())
            },
        )?;
        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let references = saved.iter().collect::<Vec<_>>();
        let list = self.emit_pre_evaluated_arg_vector(&references, function);
        self.emit_indirect_call_with_argv_to_value(
            callee,
            receiver,
            &list,
            continuation,
            output,
            function,
        )?;
        list.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        s.release_i32_local(handled, function);
        for value in saved.into_iter().rev() {
            value.clear(function);
        }
        Ok(true)
    }
}
