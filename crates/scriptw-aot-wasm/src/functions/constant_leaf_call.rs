//! Immutable primitive returns can bypass an unobservable source activation.
//! Target knowledge selects a candidate; actual code, P and depth prove it.
use super::*;
use crate::gc_types::{
    ExecutableCodeKind, ExecutableCodeSchema, FunctionContextSchema, FunctionObject,
    FunctionObjectSchema, GcI32Constant, GcNullability, ScalarValue, StringValue, ValueLocals,
};

#[derive(Clone, Debug)]
enum ConstantLeafValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(u64),
    String(String),
}

/// A complete zero-formal-argument ordinary body with no activation observer.
/// The real callable and its construction/reflection entry remain unchanged.
#[derive(Clone, Debug)]
pub(crate) struct ConstantLeafPlan {
    value: ConstantLeafValue,
}

impl ConstantLeafPlan {
    pub(crate) fn for_function(function: &FunctionIr) -> Option<Self> {
        if function.protocol != FunctionProtocolIr::OrdinaryCallAndConstruct
            || !function.params.is_empty()
            || !function.captured_bindings.is_empty()
            || function.captures_private_environment
            || function.class_element_execution_kind != ClassElementExecutionKind::None
            || function.class_instance_element_plan.is_some()
            || function.body.lexical_environment.is_some()
        {
            return None;
        }
        let [StatementIr::Return(value)] = function.body.statements.as_slice() else {
            return None;
        };
        let value = match &value.expr {
            ExprIr::Undefined => ConstantLeafValue::Undefined,
            ExprIr::Null => ConstantLeafValue::Null,
            ExprIr::Boolean(value) => ConstantLeafValue::Boolean(*value),
            ExprIr::Number(bits) => ConstantLeafValue::Number(*bits),
            ExprIr::String(value) => ConstantLeafValue::String(value.clone()),
            // In particular, no mutable read, effect, coercion, allocation or
            // observer is hidden by the expression's inferred ValueKind.
            _ => return None,
        };
        Some(Self { value })
    }

    fn emit(
        &self,
        builder: &FunctionBuilder<'_>,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match &self.value {
            ConstantLeafValue::Undefined => output.set_undefined(function),
            ConstantLeafValue::Null => output.set_scalar(ScalarValue::Null, function),
            ConstantLeafValue::Boolean(value) => {
                output.set_scalar(ScalarValue::Boolean(*value), function);
            }
            ConstantLeafValue::Number(bits) => {
                output.set_scalar(ScalarValue::NumberBits(*bits as i64), function);
            }
            ConstantLeafValue::String(value) => {
                let schema = builder.runtime_schema();
                // The defining-P identity guard precedes this publication.
                // The existing literal owner chooses its exact P or R pool.
                let literal = schema
                    .reserve_gc_local::<StringValue, _>(function)
                    .initialize(
                        builder.emit_interned_string_reference(value, function)?,
                        function,
                    );
                output.set_reference(&literal, schema, function);
                literal.clear(function);
            }
        }
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn try_emit_constant_leaf_call(
        &mut self,
        meta: &WasmFunctionMeta,
        plan: &ConstantLeafPlan,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        if !matches!(continuation, CallContinuation::Continue) || !arguments.is_empty() {
            // Decline before any argument evaluation. An accepted candidate
            // owns both its guarded route and the once-evaluated fallback.
            return Ok(false);
        }
        let schema = self.runtime_schema();
        let handled = schema.reserve_i32_local(function);
        handled.set_constant(0, function);
        callee.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Function.tag()));
        function.instruction(&Instruction::I32Eq);
        callee.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<FunctionObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32And);
        // A call at the implementation's source-depth bound must enter its
        // ordinary checked boundary and produce the same catchable Throw.
        schema.emit_source_activation_admission_test(function);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        let callable = schema.reserve_gc_local(function).initialize(
            callee.cast_reference::<FunctionObject>(schema, function),
            function,
        );
        let code = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CODE)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        let context = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CONTEXT)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        let owner = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionContextSchema::DEFINING_PROGRAM)
                .read(&context, schema, function)
                .reference(),
            function,
        );
        let source = schema.reserve_i64_local(function);
        let kind = schema.reserve_i32_local(function);
        schema
            .field(ExecutableCodeSchema::SOURCE_ID)
            .read(&code, schema, function)
            .store_i64(source, function);
        schema
            .field(ExecutableCodeSchema::KIND)
            .read(&code, schema, function)
            .store(kind, function);
        source.load(function);
        function.instruction(&Instruction::I64Const(meta.entry.source_id()));
        function.instruction(&Instruction::I64Eq);
        kind.load(function);
        function.instruction(&Instruction::I32Const(
            ExecutableCodeKind::JavaScript.encode(),
        ));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        owner.load(schema, function);
        self.current_program().reference(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        plan.emit(self, output, function)?;
        handled.set_constant(1, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(kind, function);
        schema.release_i64_local(source, function);
        owner.clear(function);
        context.clear(function);
        code.clear(function);
        callable.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        // Proxy/bound/replaced/foreign-P callees retain their complete call
        // behavior using exactly the callee and receiver already evaluated.
        let list = self.emit_pre_evaluated_arg_vector(&[], function);
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
        schema.release_i32_local(handled, function);
        Ok(true)
    }
}
