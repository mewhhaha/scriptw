//! Compare a resolved typeof result with a literal without making either String.
use super::*;

enum TypeofSpelling {
    Undefined,
    Boolean,
    Number,
    String,
    Symbol,
    BigInt,
    Object,
    Function,
    Never,
}

struct TypeofLiteralComparison<'a> {
    operand: &'a TypedExpr,
    spelling: TypeofSpelling,
}

impl<'a> TypeofLiteralComparison<'a> {
    fn for_operands(left: &'a TypedExpr, right: &'a TypedExpr) -> Option<Self> {
        let (operand, spelling) = match (&left.expr, &right.expr) {
            (ExprIr::TypeOf { expr }, ExprIr::String(spelling))
            | (ExprIr::String(spelling), ExprIr::TypeOf { expr }) => (expr, spelling),
            _ => return None,
        };
        let spelling = match spelling.as_str() {
            "undefined" => TypeofSpelling::Undefined,
            "boolean" => TypeofSpelling::Boolean,
            "number" => TypeofSpelling::Number,
            "string" => TypeofSpelling::String,
            "symbol" => TypeofSpelling::Symbol,
            "bigint" => TypeofSpelling::BigInt,
            "object" => TypeofSpelling::Object,
            "function" => TypeofSpelling::Function,
            _ => TypeofSpelling::Never,
        };
        Some(Self { operand, spelling })
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn try_compile_typeof_comparison_i32(
        &mut self,
        left: &TypedExpr,
        right: &TypedExpr,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(plan) = TypeofLiteralComparison::for_operands(left, right) else {
            return Ok(false);
        };
        // The literal is effect-free in either operand position. Keep the
        // complete operand evaluation, including TDZ and getter/call Throws.
        // Unresolved-identifier typeof retains its own reference semantics.
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        self.compile_expr_to_value(plan.operand, &value, function)?;
        let primitive = match plan.spelling {
            TypeofSpelling::Boolean => Some(WasmRuntimeValueTag::Boolean),
            TypeofSpelling::Number => Some(WasmRuntimeValueTag::Number),
            TypeofSpelling::String => Some(WasmRuntimeValueTag::String),
            TypeofSpelling::Symbol => Some(WasmRuntimeValueTag::Symbol),
            TypeofSpelling::BigInt => Some(WasmRuntimeValueTag::BigInt),
            TypeofSpelling::Undefined
            | TypeofSpelling::Object
            | TypeofSpelling::Function
            | TypeofSpelling::Never => None,
        };
        if let Some(tag) = primitive {
            value.tag().load(function);
            function.instruction(&Instruction::I32Const(tag.tag()));
            function.instruction(&Instruction::I32Eq);
        } else {
            match plan.spelling {
                TypeofSpelling::Undefined => {
                    value.tag().load(function);
                    function
                        .instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
                    function.instruction(&Instruction::I32Eq);
                    self.emit_is_htmldda_function_i32(&value, function)?;
                    function.instruction(&Instruction::I32Or);
                }
                TypeofSpelling::Object => {
                    self.emit_is_heap_object_like_tag_i32(value.tag(), function);
                    self.emit_is_callable_i32(&value, function)?;
                    function.instruction(&Instruction::I32Eqz);
                    function.instruction(&Instruction::I32And);
                    value.tag().load(function);
                    function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
                    function.instruction(&Instruction::I32Eq);
                    function.instruction(&Instruction::I32Or);
                }
                TypeofSpelling::Function => {
                    self.emit_is_callable_i32(&value, function)?;
                    self.emit_is_htmldda_function_i32(&value, function)?;
                    function.instruction(&Instruction::I32Eqz);
                    function.instruction(&Instruction::I32And);
                }
                TypeofSpelling::Never => {
                    function.instruction(&Instruction::I32Const(0));
                }
                TypeofSpelling::Boolean
                | TypeofSpelling::Number
                | TypeofSpelling::String
                | TypeofSpelling::Symbol
                | TypeofSpelling::BigInt => unreachable!("handled primitive spelling"),
            }
        }
        value.clear(function);
        Ok(true)
    }
}
