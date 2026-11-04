//! Strict equality against syntax with a unique normal value needs only its tag.
use super::*;

enum StrictSingleton {
    Null,
    Undefined,
}

impl StrictSingleton {
    fn for_expression(expression: &TypedExpr) -> Option<Self> {
        match &expression.expr {
            ExprIr::Null => Some(Self::Null),
            ExprIr::Undefined | ExprIr::Void { .. } => Some(Self::Undefined),
            _ => None,
        }
    }

    fn tag(self) -> WasmRuntimeValueTag {
        match self {
            Self::Null => WasmRuntimeValueTag::Null,
            Self::Undefined => WasmRuntimeValueTag::Undefined,
        }
    }
}

enum StrictSingletonComparison {
    Left(StrictSingleton),
    Right(StrictSingleton),
}

impl StrictSingletonComparison {
    fn for_operands(left: &TypedExpr, right: &TypedExpr) -> Option<Self> {
        if let Some(singleton) = StrictSingleton::for_expression(left) {
            return Some(Self::Left(singleton));
        }
        StrictSingleton::for_expression(right).map(Self::Right)
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn try_compile_strict_singleton_comparison_i32(
        &mut self,
        left: &TypedExpr,
        right: &TypedExpr,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(plan) = StrictSingletonComparison::for_operands(left, right) else {
            return Ok(false);
        };
        let schema = self.runtime_schema();
        let left_value = schema.reserve_value_local(function);
        let right_value = schema.reserve_value_local(function);
        // A void operand may run arbitrary code or throw. Its singleton fact
        // describes only normal completion; retain both complete evaluations
        // in source order, including the usual abrupt-completion routing.
        self.compile_expr_to_value(left, &left_value, function)?;
        self.compile_expr_to_value(right, &right_value, function)?;
        let (singleton, value) = match plan {
            StrictSingletonComparison::Left(singleton) => (singleton, &right_value),
            StrictSingletonComparison::Right(singleton) => (singleton, &left_value),
        };
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(singleton.tag().tag()));
        function.instruction(&Instruction::I32Eq);
        right_value.clear(function);
        left_value.clear(function);
        Ok(true)
    }
}
