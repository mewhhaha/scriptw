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

/// typeof's answer as a closed class: TypeofSpelling without Never.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TypeofClass {
    Undefined,
    Boolean,
    Number,
    String,
    Symbol,
    BigInt,
    Object,
    Function,
}

impl TypeofClass {
    const ALL: [Self; 8] = [
        Self::Undefined,
        Self::Boolean,
        Self::Number,
        Self::String,
        Self::Symbol,
        Self::BigInt,
        Self::Object,
        Self::Function,
    ];

    /// The dispatch index: below 16, so a class fits a 4-bit table entry.
    const fn code(self) -> u32 {
        self as u32
    }

    /// typeof of a value with this tag, before the two exotic slots: an
    /// Object-tagged callable Proxy is "function" and a Function-tagged
    /// [[IsHTMLDDA]] object is "undefined"; no other tag has either.
    const fn of_tag(tag: WasmRuntimeValueTag) -> Self {
        match tag {
            WasmRuntimeValueTag::Undefined => Self::Undefined,
            WasmRuntimeValueTag::Boolean => Self::Boolean,
            WasmRuntimeValueTag::Number => Self::Number,
            WasmRuntimeValueTag::String => Self::String,
            WasmRuntimeValueTag::Symbol => Self::Symbol,
            WasmRuntimeValueTag::BigInt => Self::BigInt,
            WasmRuntimeValueTag::Null
            | WasmRuntimeValueTag::Object
            | WasmRuntimeValueTag::Array
            | WasmRuntimeValueTag::Arguments => Self::Object,
            WasmRuntimeValueTag::Function => Self::Function,
        }
    }

    /// `of_tag` for every tag as one 4-bit entry per tag value, which the
    /// classification shifts by the actual tag.
    fn tag_table() -> i64 {
        let mut table = 0u64;
        for tag in 0..16 {
            if let Some(tag) = WasmRuntimeValueTag::from_tag(tag) {
                table |= u64::from(Self::of_tag(tag).code()) << (4 * tag as u32);
            }
        }
        table as i64
    }
}

impl TypeofSpelling {
    fn class(&self) -> Option<TypeofClass> {
        match self {
            Self::Undefined => Some(TypeofClass::Undefined),
            Self::Boolean => Some(TypeofClass::Boolean),
            Self::Number => Some(TypeofClass::Number),
            Self::String => Some(TypeofClass::String),
            Self::Symbol => Some(TypeofClass::Symbol),
            Self::BigInt => Some(TypeofClass::BigInt),
            Self::Object => Some(TypeofClass::Object),
            Self::Function => Some(TypeofClass::Function),
            Self::Never => None,
        }
    }
}

/// One test of a typeof chain: `typeof x` compared with a literal, true
/// for one class (`==`/`===`) or for every other class (`!=`/`!==`).
struct TypeofChainTest {
    spelling: TypeofSpelling,
    negated: bool,
}

impl TypeofChainTest {
    fn holds_for(&self, class: TypeofClass) -> bool {
        (self.spelling.class() == Some(class)) != self.negated
    }
}

/// `if (T1) S1 else if (T2) S2 ... else E` in which every Ti compares
/// `typeof x`, for one private local x, with a literal. Each Ti reads x and
/// observes nothing else, and only a false Ti reaches T(i+1), so no code
/// runs between two tests and x holds one value for all of them: each Ti is
/// a function of typeof x, and the first true one is a function of that
/// class. The chain reads x once, classifies it once, and selects its arm
/// with one table branch.
struct TypeofIfChain<'a> {
    operand: &'a TypedExpr,
    arms: Vec<(TypeofChainTest, &'a StatementIr)>,
    otherwise: Option<&'a StatementIr>,
}

/// Fewer tests than this decide faster from the tag one by one.
const MIN_TYPEOF_CHAIN_TESTS: usize = 3;

impl<'a> TypeofIfChain<'a> {
    fn of(
        builder: &FunctionBuilder<'_>,
        condition: &'a TypedExpr,
        then_branch: &'a StatementIr,
        else_branch: Option<&'a StatementIr>,
    ) -> Option<Self> {
        let (operand, binding, test) = chain_test(builder, condition)?;
        let mut arms = vec![(test, then_branch)];
        let mut otherwise = else_branch;
        while let Some(StatementIr::If {
            condition,
            then_branch,
            else_branch,
        }) = otherwise
        {
            let Some((_, next, test)) = chain_test(builder, condition) else {
                break;
            };
            if next != binding {
                break;
            }
            arms.push((test, &**then_branch));
            otherwise = else_branch.as_deref();
        }
        (arms.len() >= MIN_TYPEOF_CHAIN_TESTS).then_some(Self {
            operand,
            arms,
            otherwise,
        })
    }

    /// The arm the first true test selects for `class`; `None` is the else.
    fn arm_for(&self, class: TypeofClass) -> Option<usize> {
        self.arms.iter().position(|(test, _)| test.holds_for(class))
    }
}

/// A chain test's operand, the private local it reads and its comparison.
fn chain_test<'a>(
    builder: &FunctionBuilder<'_>,
    condition: &'a TypedExpr,
) -> Option<(&'a TypedExpr, crate::emit::LocalBindingId, TypeofChainTest)> {
    // `!==` and `!=` are the negated comparisons.
    let (comparison, negated) = match &condition.expr {
        ExprIr::LogicalNot { expr } => (&**expr, true),
        _ => (condition, false),
    };
    let (left, right, negated) = match &comparison.expr {
        ExprIr::SpecOperation {
            operation: SpecOperationIr::StrictEqualityComparison | SpecOperationIr::IsLooselyEqual,
            operands,
        } => match &operands[..] {
            [left, right] => (left, right, negated),
            _ => return None,
        },
        ExprIr::StrictEquality { op, lhs, rhs } => (
            &**lhs,
            &**rhs,
            negated != (*op == EqualityBinaryOp::StrictNotEqual),
        ),
        ExprIr::LooseEquality { op, lhs, rhs } => (
            &**lhs,
            &**rhs,
            negated != (*op == EqualityBinaryOp::LooseNotEqual),
        ),
        _ => return None,
    };
    let TypeofLiteralComparison {
        operand: TypeofOperand::Value(operand),
        spelling,
    } = TypeofLiteralComparison::for_operands(left, right)?
    else {
        return None;
    };
    let ExprIr::Identifier(name) = &operand.expr else {
        return None;
    };
    let binding = builder.private_local_binding(name)?;
    Some((operand, binding, TypeofChainTest { spelling, negated }))
}

impl FunctionBuilder<'_> {
    /// Compiles `if (...) ... else ...` as a typeof chain when it is one,
    /// with the If statement's own order: x's read (and its TDZ Throw), the
    /// statement's undefined result, then the selected arm.
    pub(crate) fn try_compile_typeof_if_chain(
        &mut self,
        condition: &TypedExpr,
        then_branch: &StatementIr,
        else_branch: Option<&StatementIr>,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(chain) = TypeofIfChain::of(self, condition, then_branch, else_branch) else {
            return Ok(false);
        };
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        self.compile_typeof_operand_to_value(chain.operand, &value, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        let class = schema.reserve_i32_local(function);
        self.emit_typeof_class_i32(&value, function);
        class.store(function);
        value.clear(function);
        self.emit_statement_result(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let otherwise = self.open_frame(ControlFrameKind::Block, function);
        // Leaving the innermost block enters the first arm.
        let mut arms = Vec::with_capacity(chain.arms.len());
        for _ in &chain.arms {
            arms.push(self.open_frame(ControlFrameKind::Block, function));
        }
        arms.reverse();
        let target = |arm: Option<usize>| arm.map_or(otherwise, |arm| arms[arm]);
        let labels = TypeofClass::ALL
            .iter()
            .map(|class| self.local_branch_label(target(chain.arm_for(*class))))
            .collect::<Vec<_>>();
        class.load(function);
        schema.release_i32_local(class, function);
        let default = self.local_branch_label(otherwise);
        function.branch_table_to_labels(labels, default);
        for (_, arm) in &chain.arms {
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
            self.compile_statement(arm, function)?;
            self.emit_branch_to_target(exit, function);
        }
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if let Some(otherwise) = chain.otherwise {
            self.compile_statement(otherwise, function)?;
        }
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        Ok(true)
    }

    /// typeof of `value` as a TypeofClass code: the tag's class from one
    /// table, then the Object tag's callable Proxy and the Function tag's
    /// [[IsHTMLDDA]] object.
    fn emit_typeof_class_i32(&self, value: &ValueLocals, function: &mut Function) {
        let schema = self.runtime_schema();
        let class = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I64Const(TypeofClass::tag_table()));
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(2));
        function.instruction(&Instruction::I32Shl);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32Const(0xf));
        function.instruction(&Instruction::I32And);
        class.store(function);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Object.tag()));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_object_tag_proxy_call_capability_i32(value, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        class.set_constant(TypeofClass::Function.code() as i32, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Function.tag()));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_function_tag_is_htmldda_i32(value, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        class.set_constant(TypeofClass::Undefined.code() as i32, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        class.load(function);
        schema.release_i32_local(class, function);
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
