//! The actual callable's published typed entry is the guard, including across P.
//! A planned source target's code identity proves its entry without reading it.
use super::*;
use crate::function_entry::{
    LocalArgumentCallInputs, LocalArgumentsArity, LocalArgumentsCallableRole,
    PlannedLocalArgumentsEntry,
};
use crate::gc_types::{
    CompletionLocals, ExecutableCode, FunctionContextSchema, FunctionLocal, FunctionObject,
    FunctionObjectSchema, GcI32Constant, GcLocal, I32Local, LocalArgumentsCallable, ValueLocals,
    WideLocalArgumentsCallable,
};

impl FunctionBuilder<'_> {
    pub(super) fn try_emit_local_argument_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        known_target: Option<&WasmFunctionMeta>,
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        // An ineligible source hint does not justify another inline guard.
        // Full dispatch still handles an eligible replacement correctly.
        let known_entry = known_target.and_then(|meta| meta.entry.local_arguments());
        if known_target.is_some() && known_entry.is_none() {
            return Ok(false);
        }
        if arguments.iter().any(|argument| {
            matches!(
                argument.expr,
                ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
            )
        }) {
            return Ok(false);
        }
        if LocalArgumentsArity::for_actuals(arguments.len()).is_none() {
            return Ok(false);
        }

        let schema = self.runtime_schema();
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let value = schema.reserve_value_local(function);
            self.compile_expr_to_value(argument, &value, function)?;
            self.emit_propagate_current_throw_if_needed(function);
            // A later operand can call, allocate, or replace the source binding.
            // Keep both the actual callee and all preceding arguments rooted.
            values.push(value);
        }
        let result = schema.reserve_completion(function);
        let references: Vec<_> = values.iter().collect();
        if let Some((meta, planned)) =
            known_target.zip(known_entry.filter(|entry| entry.is_source()))
        {
            self.emit_planned_local_argument_call(
                callee,
                receiver,
                &references,
                meta,
                planned,
                continuation,
                &result,
                function,
            )?;
        } else {
            let handled = self.emit_local_argument_call_values_for_count(
                callee,
                receiver,
                &references,
                continuation,
                &result,
                function,
            )?;
            handled.load(function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_local_argument_call_fallback(
                callee,
                receiver,
                &references,
                continuation,
                &result,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            schema.release_i32_local(handled, function);
        }

        if matches!(continuation, CallContinuation::Continue) {
            self.emit_propagate_pending_throw(&result, &[], function);
            output.copy_from(result.value(), function);
        } else {
            // Both the typed entry and the argv fallback tail-call.
            function.instruction(&Instruction::Unreachable);
        }
        result.clear(function);
        for value in values.into_iter().rev() {
            value.clear(function);
        }
        Ok(true)
    }

    fn emit_planned_local_argument_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[&ValueLocals],
        meta: &WasmFunctionMeta,
        planned: &PlannedLocalArgumentsEntry,
        continuation: &CallContinuation,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let done = self.open_frame(ControlFrameKind::Block, function);
        let fallback = self.open_frame(ControlFrameKind::Block, function);
        self.rarely_taken(fallback, function);
        let callable = self.emit_local_argument_callable_or_branch(callee, fallback, function);
        let code = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CODE)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        self.emit_source_code_identity_test(meta, &callable, &code, function)?;
        code.clear(function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(fallback, function);
        // A narrow source body cannot observe surplus lanes. Every operand
        // was evaluated already, and the fallback gets the full argument list.
        let target = LocalCallTarget {
            callable: &callable,
            receiver,
            arguments: &arguments[..arguments.len().min(planned.arity().slot_count())],
        };
        match planned.arity() {
            LocalArgumentsArity::Narrow => self
                .emit_entered_local_argument_call::<LocalArgumentsCallable>(
                    &target,
                    LocalCallEntry::Planned(planned),
                    continuation,
                    result,
                    function,
                ),
            LocalArgumentsArity::Wide => self
                .emit_entered_local_argument_call::<WideLocalArgumentsCallable>(
                    &target,
                    LocalCallEntry::Planned(planned),
                    continuation,
                    result,
                    function,
                ),
        }
        callable.clear(function);
        if matches!(continuation, CallContinuation::Continue) {
            self.emit_branch_to_target(done, function);
        }
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        // This guard and the vector entry use the same source identity. A
        // miss goes straight to general dispatch with the evaluated callable.
        function.rarely_reached();
        self.emit_local_argument_call_fallback(
            callee,
            receiver,
            arguments,
            continuation,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_local_argument_call_fallback(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[&ValueLocals],
        continuation: &CallContinuation,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let list = self.emit_pre_evaluated_arg_vector(arguments, function);
        match continuation {
            CallContinuation::Continue => self
                .emit_function_or_proxy_call_with_argv(callee, receiver, &list, result, function)?,
            CallContinuation::Return => {
                self.emit_prepared_tail_call(callee, receiver, &list, function)?;
            }
        }
        list.clear(function);
        Ok(())
    }

    /// Borrow once-evaluated values. The owner chooses fallback allocation and
    /// abrupt routing after this guard has preserved the whole call result.
    pub(crate) fn emit_local_argument_call_from_values(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[&ValueLocals],
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<I32Local, EmitError> {
        self.emit_local_argument_call_values_for_count(
            callee,
            receiver,
            arguments,
            &CallContinuation::Continue,
            result,
            function,
        )
    }

    /// The argument count selects the typed role; over-width calls use argv.
    pub(super) fn emit_local_argument_call_values_for_count(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[&ValueLocals],
        continuation: &CallContinuation,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<I32Local, EmitError> {
        match LocalArgumentsArity::for_actuals(arguments.len()) {
            Some(LocalArgumentsArity::Narrow) => self
                .emit_local_argument_call_values::<LocalArgumentsCallable>(
                    callee,
                    receiver,
                    arguments,
                    continuation,
                    result,
                    function,
                ),
            Some(LocalArgumentsArity::Wide) => self
                .emit_local_argument_call_values::<WideLocalArgumentsCallable>(
                    callee,
                    receiver,
                    arguments,
                    continuation,
                    result,
                    function,
                ),
            None => {
                let handled = self.runtime_schema().reserve_i32_local(function);
                handled.set_constant(0, function);
                Ok(handled)
            }
        }
    }

    fn emit_local_argument_call_values<R: LocalArgumentsCallableRole>(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[&ValueLocals],
        continuation: &CallContinuation,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let handled = schema.reserve_i32_local(function);
        handled.set_constant(0, function);
        let fallback = self.open_frame(ControlFrameKind::Block, function);
        let callable = self.emit_local_argument_callable_or_branch(callee, fallback, function);
        let code = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CODE)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        let target = LocalCallTarget {
            callable: &callable,
            receiver,
            arguments,
        };
        self.emit_published_local_argument_call::<R>(
            &target,
            &code,
            handled,
            continuation,
            result,
            function,
        );
        code.clear(function);
        callable.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        Ok(handled)
    }

    fn emit_local_argument_callable_or_branch(
        &self,
        callee: &ValueLocals,
        fallback: ControlTarget,
        function: &mut Function,
    ) -> GcLocal<FunctionObject> {
        let schema = self.runtime_schema();
        callee.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Function.tag()));
        function.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(fallback, function);
        schema.reserve_non_null_gc_local(function).initialize(
            callee.sealed_reference_or_branch::<FunctionObject>(
                schema,
                self.local_branch_label(fallback),
                function,
            ),
            function,
        )
    }

    /// The published typed entry is the guard for an unplanned actual callee.
    fn emit_published_local_argument_call<R: LocalArgumentsCallableRole>(
        &mut self,
        target: &LocalCallTarget<'_>,
        code: &GcLocal<ExecutableCode>,
        handled: I32Local,
        continuation: &CallContinuation,
        result: &CompletionLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let optional = R::load_optional_entry(code, schema, function);
        optional.load(function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let entry = schema
            .reserve_function_local::<R, _>(function)
            .initialize(optional.load(function).require_non_null(function), function);
        self.emit_entered_local_argument_call::<R>(
            target,
            LocalCallEntry::Published(&entry),
            continuation,
            result,
            function,
        );
        handled.set_constant(1, function);
        entry.clear(function);
        if R::ARITY == LocalArgumentsArity::Wide {
            // A narrow local-arguments entry proves args >= 2 are unobservable,
            // so a 3..=6 argument call site (e.g. Array.prototype.map/filter/
            // find/reduce) can invoke it directly with the first two arguments.
            function.instruction(&Instruction::Else);
            let narrow_optional =
                LocalArgumentsCallable::load_optional_entry(code, schema, function);
            narrow_optional.load(function).is_null(function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            let narrow_entry = schema
                .reserve_function_local::<LocalArgumentsCallable, _>(function)
                .initialize(
                    narrow_optional.load(function).require_non_null(function),
                    function,
                );
            let narrow_target = LocalCallTarget {
                callable: target.callable,
                receiver: target.receiver,
                arguments: &target.arguments[..target
                    .arguments
                    .len()
                    .min(LocalArgumentsArity::Narrow.slot_count())],
            };
            self.emit_entered_local_argument_call::<LocalArgumentsCallable>(
                &narrow_target,
                LocalCallEntry::Published(&narrow_entry),
                continuation,
                result,
                function,
            );
            handled.set_constant(1, function);
            narrow_entry.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            narrow_optional.clear(function);
        } else {
            // A wide local-arguments entry pads omitted arguments with
            // undefined and carries the actual count, so a 0..=2 argument call
            // site can invoke it directly without allocating a ValueArray.
            function.instruction(&Instruction::Else);
            let wide_optional =
                WideLocalArgumentsCallable::load_optional_entry(code, schema, function);
            wide_optional.load(function).is_null(function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            let wide_entry = schema
                .reserve_function_local::<WideLocalArgumentsCallable, _>(function)
                .initialize(
                    wide_optional.load(function).require_non_null(function),
                    function,
                );
            self.emit_entered_local_argument_call::<WideLocalArgumentsCallable>(
                target,
                LocalCallEntry::Published(&wide_entry),
                continuation,
                result,
                function,
            );
            handled.set_constant(1, function);
            wide_entry.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            wide_optional.clear(function);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        optional.clear(function);
    }

    fn emit_entered_local_argument_call<R: LocalArgumentsCallableRole>(
        &mut self,
        target: &LocalCallTarget<'_>,
        entry: LocalCallEntry<'_, R>,
        continuation: &CallContinuation,
        result: &CompletionLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let context = schema
            .field(FunctionObjectSchema::CONTEXT)
            .read(target.callable, schema, function)
            .reference();
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionContextSchema::REALM)
                .read_top(context, function)
                .reference(),
            function,
        );
        let caller_realm = self.load_current_realm(function);
        // The actual closure's Realm determines whether a direct source
        // call can enter with the caller's ambient Realm already installed.
        let realm_changed = if matches!(&entry, LocalCallEntry::Planned(_)) {
            let changed = schema.reserve_i32_local(function);
            caller_realm.load(schema, function);
            realm.load(schema, function);
            function.instruction(&Instruction::RefEq);
            function.instruction(&Instruction::I32Eqz);
            changed.store(function);
            Some(changed)
        } else {
            None
        };
        match continuation {
            CallContinuation::Continue => {
                self.emit_local_call_realm(&realm, realm_changed, function);
                let inputs = LocalArgumentCallInputs::<R>::call(
                    target.receiver,
                    target.arguments,
                    &caller_realm,
                    self.current_program(),
                );
                result.store_call(
                    match entry {
                        LocalCallEntry::Planned(planned) => {
                            inputs.emit_planned_call(target.callable, planned, schema, function)
                        }
                        LocalCallEntry::Published(published) => {
                            inputs.emit_call(target.callable, published, schema, function)
                        }
                    },
                    function,
                );
                // A strict same-Realm callee can tail-call a foreign-Realm
                // function and return directly here, so recheck the live
                // ambient Realm before skipping the restore write.
                if realm_changed.is_some() {
                    let live_realm = self.load_current_realm(function);
                    live_realm.load(schema, function);
                    caller_realm.load(schema, function);
                    function.instruction(&Instruction::RefEq);
                    function.instruction(&Instruction::I32Eqz);
                    function.instruction(&Instruction::If(BlockType::Empty));
                    function.rarely_reached();
                    self.replace_current_realm(&caller_realm, function);
                    function.instruction(&Instruction::End);
                    live_realm.clear(function);
                } else {
                    self.replace_current_realm(&caller_realm, function);
                }
            }
            CallContinuation::Return => {
                // The frame-local program owner survives lexical unwind.
                // Operand evaluation and the entry guard are complete.
                self.emit_retire_tail_call_activation(function);
                self.emit_local_call_realm(&realm, realm_changed, function);
                let inputs = LocalArgumentCallInputs::<R>::call(
                    target.receiver,
                    target.arguments,
                    &caller_realm,
                    self.current_program(),
                );
                match entry {
                    LocalCallEntry::Planned(planned) => {
                        inputs.emit_planned_return_call(target.callable, planned, schema, function)
                    }
                    LocalCallEntry::Published(published) => {
                        inputs.emit_return_call(target.callable, published, schema, function)
                    }
                }
            }
        }
        if let Some(changed) = realm_changed {
            schema.release_i32_local(changed, function);
        }
        caller_realm.clear(function);
        realm.clear(function);
    }

    fn emit_local_call_realm(
        &self,
        realm: &GcLocal<crate::gc_types::RealmRecord>,
        changed: Option<I32Local>,
        function: &mut Function,
    ) {
        if let Some(changed) = changed {
            changed.load(function);
            function.instruction(&Instruction::If(BlockType::Empty));
            function.rarely_reached();
        }
        self.replace_current_realm(realm, function);
        if changed.is_some() {
            function.instruction(&Instruction::End);
        }
    }
}

/// The once-evaluated actual callee and operands of one guarded local call.
struct LocalCallTarget<'a> {
    callable: &'a GcLocal<FunctionObject>,
    receiver: &'a ValueLocals,
    arguments: &'a [&'a ValueLocals],
}

enum LocalCallEntry<'a, R: LocalArgumentsCallableRole> {
    /// Proven by the actual code's source identity and defining program.
    Planned(&'a PlannedLocalArgumentsEntry),
    /// Read from the actual code record and checked non-null.
    Published(&'a FunctionLocal<R>),
}
