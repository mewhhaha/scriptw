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

/// What typeof inspects: an evaluated operand (a global identifier among them
/// keeps its Reference), or an identifier Reference that typeof reads itself.
/// Either way an unresolvable name reads as undefined.
enum TypeofOperand<'a> {
    Value(&'a TypedExpr),
    Reference(&'a TypedExpr),
}

impl<'a> TypeofOperand<'a> {
    fn of(typeof_expression: &'a TypedExpr) -> Option<Self> {
        match &typeof_expression.expr {
            ExprIr::TypeOf { expr } => Some(Self::Value(expr)),
            ExprIr::TypeOfUnresolvedIdentifier { .. } => Some(Self::Reference(typeof_expression)),
            ExprIr::EnvironmentIdentifier(identifier)
                if matches!(
                    identifier.operation,
                    scriptw_ir::EnvironmentIdentifierOperationIr::Typeof
                ) =>
            {
                Some(Self::Reference(typeof_expression))
            }
            _ => None,
        }
    }
}

struct TypeofLiteralComparison<'a> {
    operand: TypeofOperand<'a>,
    spelling: TypeofSpelling,
}

impl<'a> TypeofLiteralComparison<'a> {
    fn for_operands(left: &'a TypedExpr, right: &'a TypedExpr) -> Option<Self> {
        let (operand, spelling) = match (&left.expr, &right.expr) {
            (_, ExprIr::String(spelling)) => (TypeofOperand::of(left)?, spelling),
            (ExprIr::String(spelling), _) => (TypeofOperand::of(right)?, spelling),
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
        // An identifier Reference keeps typeof's own read, in which an
        // unresolvable name is undefined; only the String is omitted.
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        match plan.operand {
            TypeofOperand::Value(operand) => {
                self.compile_typeof_operand_to_value(operand, &value, function)?
            }
            TypeofOperand::Reference(reference) => {
                self.compile_typeof_reference_value(reference, &value, function)?
            }
        }
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
            // Classify from the tag first and decide each arm alone: only
            // the Function tag can carry [[IsHTMLDDA]], and only the Object
            // tag can be a callable Proxy (`emit_is_callable_i32`).
            match plan.spelling {
                TypeofSpelling::Undefined => {
                    value.tag().load(function);
                    function
                        .instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
                    function.instruction(&Instruction::I32Eq);
                    function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                    function.instruction(&Instruction::I32Const(1));
                    function.instruction(&Instruction::Else);
                    self.emit_is_htmldda_function_i32(&value, function)?;
                    function.instruction(&Instruction::End);
                }
                TypeofSpelling::Object => {
                    value.tag().load(function);
                    function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Object.tag()));
                    function.instruction(&Instruction::I32Eq);
                    function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                    self.emit_object_tag_proxy_call_capability_i32(&value, function);
                    function.instruction(&Instruction::I32Eqz);
                    function.instruction(&Instruction::Else);
                    // A Function-tagged value is callable or [[IsHTMLDDA]];
                    // either way its typeof is not "object".
                    crate::gc_types::emit_tag_in(
                        value.tag(),
                        crate::gc_types::TagSet::of(&[
                            WasmRuntimeValueTag::Null,
                            WasmRuntimeValueTag::Array,
                            WasmRuntimeValueTag::Arguments,
                        ]),
                        function,
                    );
                    function.instruction(&Instruction::End);
                }
                TypeofSpelling::Function => {
                    value.tag().load(function);
                    function
                        .instruction(&Instruction::I32Const(WasmRuntimeValueTag::Function.tag()));
                    function.instruction(&Instruction::I32Eq);
                    function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                    self.emit_function_tag_is_htmldda_i32(&value, function);
                    function.instruction(&Instruction::I32Eqz);
                    function.instruction(&Instruction::Else);
                    self.emit_object_tag_is_callable_proxy_i32(&value, function);
                    function.instruction(&Instruction::End);
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

#[cfg(test)]
mod tests {
    use crate::gc_types::GcLayout;
    use crate::WasmRuntimeValueTag;

    /// `emit_is_callable_i32`, `emit_is_htmldda_function_i32` and the literal
    /// typeof predicates classify from the tag first. That is exact only while
    /// the Function tag names exactly the FunctionObject and BoundFunction
    /// layouts and a Proxy, the one other callable, carries the Object tag.
    #[test]
    fn function_tag_names_exactly_the_function_layouts_and_proxies_are_objects() {
        for layout in GcLayout::ALL {
            assert_eq!(
                layout.javascript_tag() == Some(WasmRuntimeValueTag::Function),
                matches!(layout, GcLayout::FunctionObject | GcLayout::BoundFunction),
                "{layout:?}"
            );
        }
        assert_eq!(
            GcLayout::ProxyObject.javascript_tag(),
            Some(WasmRuntimeValueTag::Object)
        );
    }
}
