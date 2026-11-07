use super::*;
use crate::gc_types::{PropertyKeyTable, StringValue};

mod enumeration;

impl FunctionBuilder<'_> {
    pub(crate) fn compile_for_in(
        &mut self,
        mode: BindingMode,
        name: &str,
        target: &TypedExpr,
        body: &StatementIr,
        lexical_environment: Option<&ForInOfEnvironmentIr>,
        labels: &[String],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let source = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        if let Some(environment) = lexical_environment {
            self.emit_enter_for_in_of_tdz_scope(mode, environment, function)?;
        }
        self.compile_expr_to_value(target, &source, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        if let Some(environment) = lexical_environment {
            self.emit_leave_for_in_of_tdz_scope(environment, function);
        }
        self.push_scope();
        let storage_without_environment =
            if mode == BindingMode::Var {
                Some(self.lookup_binding(name).ok_or_else(|| {
                    EmitError::unsupported(format!("unbound for-in var `{name}`"))
                })?)
            } else if !iteration_environment_owns_binding(lexical_environment, name) {
                Some(self.allocate_binding(name.to_string(), mode, ValueKind::String, function))
            } else {
                None
            };
        if mode == BindingMode::Var {
            self.binding_scopes
                .last_mut()
                .expect("for-in binding scope exists")
                .insert(
                    name.to_string(),
                    storage_without_environment.expect("var storage exists"),
                );
        }
        self.emit_statement_result(function);
        let record = self.emit_create_for_in_enumerator(&source, function)?;
        self.emit_statement_result(function);
        let break_frame = self.open_frame(ControlFrameKind::Block, function);
        self.breakable_stack.push(break_frame);
        let enumeration_loop = self.open_frame(ControlFrameKind::Loop, function);
        self.emit_advance_for_in_enumerator(&record, &pending, function)?;
        pending.value().tag().load(function);
        function.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(break_frame, function);
        if let Some(environment) =
            lexical_environment.and_then(|e| e.iteration_environment.as_ref())
        {
            self.emit_enter_lexical_environment(environment, function)?;
        }
        let storage = self
            .lookup_current_scope_binding(name)
            .or(storage_without_environment)
            .expect("for-in binding is allocated before key publication");
        let saved = self.save_statement_list_value(function);
        self.write_binding_from_locals(storage, pending.value(), function);
        self.mirror_binding_to_global_object(name, storage, function)?;
        self.restore_statement_list_value(saved, function)?;
        let continue_frame = self.open_frame(ControlFrameKind::Block, function);
        self.loop_stack.push(LoopTargets { continue_frame });
        self.push_labels(labels, break_frame, Some(continue_frame));
        self.compile_statement(body, function)?;
        self.pop_labels(labels.len());
        self.loop_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if lexical_environment
            .and_then(|e| e.iteration_environment.as_ref())
            .is_some()
        {
            self.emit_leave_lexical_environment(function);
        }
        self.emit_branch_to_target(enumeration_loop, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.breakable_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        record.clear(function);
        self.pop_scope();
        pending.clear(function);
        source.clear(function);
        Ok(())
    }
}
