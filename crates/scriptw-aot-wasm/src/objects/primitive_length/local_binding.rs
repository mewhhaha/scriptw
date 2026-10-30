//! A complete two-statement body proves one unobserved local String consumer.
use super::*;

struct LocalConcatLengthReturn<'a> {
    consumer: ConcatLengthConsumer<'a>,
}

impl<'a> LocalConcatLengthReturn<'a> {
    fn new(builder: &FunctionBuilder<'_>, block: &'a BlockIr) -> Option<Self> {
        if !builder.has_source_execution_environment()
            || !std::ptr::eq(block, builder.body)
            || block.lexical_environment.is_some()
            || !builder.throw_handler_stack.is_empty()
            || !builder.finally_stack.is_empty()
            || !builder.current_function_meta().is_some_and(|meta| {
                meta.protocol().execution_kind() == FunctionExecutionKind::Ordinary
            })
        {
            return None;
        }
        let [declaration, StatementIr::Return(returned)] = block.statements.as_slice() else {
            return None;
        };
        let StatementIr::Lexical {
            mode: BindingMode::Const | BindingMode::Let,
            name,
            init,
        } = lexical_declaration(declaration)
        else {
            return None;
        };
        let BindingStorage::Local(binding) = builder.lookup_binding(name)? else {
            // Captures and eval-visible bindings have real environment cells.
            return None;
        };
        let key = match &returned.expr {
            ExprIr::PropertyRead { target, key } if matches!(&target.expr, ExprIr::Identifier(actual) if actual == name) => {
                StaticLengthKey::for_property_key(key)?
            }
            ExprIr::SpecOperation {
                operation: SpecOperationIr::GetV,
                operands,
            } => {
                let [target, key] = operands.as_slice() else {
                    return None;
                };
                if !matches!(&target.expr, ExprIr::Identifier(actual) if actual == name) {
                    return None;
                }
                StaticLengthKey::for_get_v_operand(key)?
            }
            _ => return None,
        };
        let mut consumer = ConcatLengthConsumer::new(init, key)?;
        // This closed whole-body grammar proves exactly one read, dominance,
        // no capture/eval and no intervening observer without a general use
        // analysis. Object-valued parameters still use the ordinary ordered
        // ToPrimitive fallback. Self-reference retains the original TDZ path.
        if !direct_operand(consumer.left, name) || !direct_operand(consumer.right, name) {
            return None;
        }
        consumer.fallback_binding = Some(binding);
        Some(Self { consumer })
    }
}

fn lexical_declaration(mut statement: &StatementIr) -> &StatementIr {
    loop {
        statement = match statement {
            StatementIr::EmptyStatementCompletion(item) => item.statement(),
            StatementIr::LexicalBlock(statements) if statements.len() == 1 => &statements[0],
            _ => return statement,
        };
    }
}

fn direct_operand(value: &TypedExpr, binding: &str) -> bool {
    match &value.expr {
        ExprIr::Identifier(name) => name != binding,
        ExprIr::String(_)
        | ExprIr::Number(_)
        | ExprIr::Boolean(_)
        | ExprIr::Null
        | ExprIr::Undefined => true,
        _ => false,
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn try_compile_local_concat_length_return(
        &mut self,
        block: &BlockIr,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(plan) = LocalConcatLengthReturn::new(self, block) else {
            return Ok(false);
        };
        let output = self.runtime_schema().reserve_value_local(function);
        self.compile_concat_length_consumer(plan.consumer, &output, function)?;
        // The existing return owner keeps constructor and activation semantics.
        self.emit_return_position_value(&output, function)?;
        output.clear(function);
        Ok(true)
    }
}
