//! A bounded pure Number leaf can consume rooted operands without a GC argv.
//! Target knowledge only selects a candidate; code and defining P prove it.
use super::*;
use crate::gc_types::{
    ExecutableCodeKind, ExecutableCodeSchema, FunctionContextSchema, FunctionObject,
    FunctionObjectSchema, GcI32Constant, GcNullability, I64Local, ValueLocals,
};

const MAX_LEAF_NODES: usize = 16;
const MAX_LEAF_ARGUMENTS: usize = 4;

#[derive(Clone, Copy, Debug)]
enum NumericLeafBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Debug)]
enum NumericLeafExpression {
    Parameter(usize),
    Constant(u64),
    Negate(Box<Self>),
    Binary(NumericLeafBinaryOp, Box<Self>, Box<Self>),
}

/// Construction admits no calls, mutable binding reads, coercions on the
/// guarded Number path, allocation, or activation/Realm observers.
#[derive(Clone, Debug)]
pub(crate) struct NumericLeafPlan {
    source: FunctionId,
    arity: usize,
    expression: NumericLeafExpression,
}

impl NumericLeafPlan {
    pub(crate) fn for_function(function: &FunctionIr) -> Option<Self> {
        if !matches!(
            function.protocol,
            FunctionProtocolIr::OrdinaryCallAndConstruct | FunctionProtocolIr::Arrow
        ) || function.class_element_execution_kind != ClassElementExecutionKind::None
            || function.class_instance_element_plan.is_some()
            || function.body.lexical_environment.is_some()
            || !function.captured_bindings.is_empty()
            || function.captures_private_environment
            || function.params.is_empty()
            || function.params.len() > MAX_LEAF_ARGUMENTS
            || function
                .params
                .iter()
                .any(|param| param.is_rest || param.default_init.is_some())
        {
            return None;
        }
        let names: BTreeSet<_> = function.params.iter().map(|param| &param.name).collect();
        if names.len() != function.params.len() {
            return None;
        }
        let [StatementIr::Return(value)] = function.body.statements.as_slice() else {
            return None;
        };
        let mut remaining = MAX_LEAF_NODES;
        Some(Self {
            source: function.id.clone(),
            arity: function.params.len(),
            expression: Self::expression(value, function, &mut remaining)?,
        })
    }

    fn expression(
        value: &TypedExpr,
        function: &FunctionIr,
        remaining: &mut usize,
    ) -> Option<NumericLeafExpression> {
        *remaining = remaining.checked_sub(1)?;
        Some(match &value.expr {
            ExprIr::Identifier(name) => NumericLeafExpression::Parameter(
                function
                    .params
                    .iter()
                    .position(|param| param.name == *name)?,
            ),
            ExprIr::Number(bits) => NumericLeafExpression::Constant(*bits),
            ExprIr::UnaryPlus { expr } => return Self::expression(expr, function, remaining),
            ExprIr::UnaryMinusNumeric { expr } => NumericLeafExpression::Negate(Box::new(
                Self::expression(expr, function, remaining)?,
            )),
            ExprIr::CoerciveAdd { lhs, rhs } => NumericLeafExpression::Binary(
                NumericLeafBinaryOp::Add,
                Box::new(Self::expression(lhs, function, remaining)?),
                Box::new(Self::expression(rhs, function, remaining)?),
            ),
            ExprIr::BinaryNumber { op, lhs, rhs }
            | ExprIr::CoerciveBinaryNumber { op, lhs, rhs } => {
                let op = match op {
                    ArithmeticBinaryOp::Add => NumericLeafBinaryOp::Add,
                    ArithmeticBinaryOp::Sub => NumericLeafBinaryOp::Sub,
                    ArithmeticBinaryOp::Mul => NumericLeafBinaryOp::Mul,
                    ArithmeticBinaryOp::Div => NumericLeafBinaryOp::Div,
                    ArithmeticBinaryOp::Mod | ArithmeticBinaryOp::Exp => return None,
                };
                NumericLeafExpression::Binary(
                    op,
                    Box::new(Self::expression(lhs, function, remaining)?),
                    Box::new(Self::expression(rhs, function, remaining)?),
                )
            }
            _ => return None,
        })
    }
}

impl NumericLeafExpression {
    /// The whole expression is allocation-free and cannot throw. Its bounded
    /// tree may keep primitive payloads on the Wasm operand stack unchanged.
    fn emit(&self, arguments: &[ValueLocals], function: &mut Function) {
        self.emit_parameters(
            &|index, function| {
                arguments[index].scalar().load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
            },
            function,
        );
    }

    fn emit_parameters(&self, parameter: &impl Fn(usize, &mut Function), function: &mut Function) {
        match self {
            Self::Parameter(index) => parameter(*index, function),
            Self::Constant(bits) => {
                function.instruction(&Instruction::I64Const(*bits as i64));
                function.instruction(&Instruction::F64ReinterpretI64);
            }
            Self::Negate(value) => {
                value.emit_parameters(parameter, function);
                function.instruction(&Instruction::F64Neg);
            }
            Self::Binary(op, lhs, rhs) => {
                lhs.emit_parameters(parameter, function);
                rhs.emit_parameters(parameter, function);
                function.instruction(&match op {
                    NumericLeafBinaryOp::Add => Instruction::F64Add,
                    NumericLeafBinaryOp::Sub => Instruction::F64Sub,
                    NumericLeafBinaryOp::Mul => Instruction::F64Mul,
                    NumericLeafBinaryOp::Div => Instruction::F64Div,
                });
            }
        }
    }
}

/// Only the already validated complete Number leaf can mint a binary thunk.
/// Its proof is shared by direct specialization, layout and code publication.
#[derive(Clone, Debug)]
pub(crate) struct BinaryNumericLeafPlan(NumericLeafPlan);

impl NumericLeafPlan {
    pub(crate) fn binary_callback(&self) -> Option<BinaryNumericLeafPlan> {
        (self.arity == 2).then(|| BinaryNumericLeafPlan(self.clone()))
    }
}

impl BinaryNumericLeafPlan {
    pub(crate) fn source(&self) -> &FunctionId {
        &self.0.source
    }
    pub(crate) fn compile_body(&self, schema: &crate::gc_types::RuntimeSchema) -> Function {
        let signature = schema.signature(crate::module::StaticSignature::BinaryNumericCallback);
        let mut function = Function::with_parameters(
            crate::code_sink::LocalDeclarations::from_types([]),
            signature.parameters().to_vec(),
        );
        schema.emit_source_activation_admission_test(&mut function);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I32Const(1));
        self.0.expression.emit_parameters(
            &|index, function| {
                schema.parameter_f64(index as u32, function).load(function);
            },
            &mut function,
        );
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        function.instruction(&Instruction::End);
        function
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn try_emit_numeric_leaf_call(
        &mut self,
        meta: &WasmFunctionMeta,
        plan: &NumericLeafPlan,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        if !matches!(continuation, CallContinuation::Continue)
            || arguments.len() != plan.arity
            || arguments.iter().any(|argument| {
                // Kind knowledge may decline a route, never replace its
                // runtime Number guards or admit a coercion-free execution.
                matches!(
                    argument.expr,
                    ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
                ) || !argument.possible_kinds.contains(ValueKind::Number)
            })
        {
            return Ok(false);
        }
        let schema = self.runtime_schema();
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let value = schema.reserve_value_local(function);
            self.compile_expr_to_value(argument, &value, function)?;
            self.emit_propagate_current_throw_if_needed(function);
            // Every earlier operand remains a strong root across later calls.
            values.push(value);
        }
        let handled = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        handled.store(function);
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
        for value in &values {
            value.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32And);
        }
        // Exhausted source depth must still enter the ordinary call boundary
        // and produce its catchable Throw, even for an otherwise pure leaf.
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
        let source: I64Local = schema.reserve_i64_local(function);
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
        plan.expression.emit(&values, function);
        function.instruction(&Instruction::I64ReinterpretF64);
        output.scalar().store(function);
        output.set_number(output.scalar(), function);
        function.instruction(&Instruction::I32Const(1));
        handled.store(function);
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
        let references: Vec<_> = values.iter().collect();
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
        schema.release_i32_local(handled, function);
        for value in values.into_iter().rev() {
            value.clear(function);
        }
        Ok(true)
    }
}
