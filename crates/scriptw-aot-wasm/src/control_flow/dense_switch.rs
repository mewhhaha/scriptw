use super::*;

#[derive(Clone, Copy)]
enum CaseEntry {
    Clause(usize),
    Exit,
}

/// Only effect-free integer literals may skip ordered CaseClause evaluation.
/// Entries preserve the first duplicate and the source position of default.
pub(super) struct DenseIntegerSwitch<'a> {
    cases: &'a [SwitchCaseIr],
    minimum: i32,
    entries: Vec<CaseEntry>,
    fallback: CaseEntry,
}

impl<'a> DenseIntegerSwitch<'a> {
    pub(super) fn analyze(
        builder: &FunctionBuilder<'_>,
        cases: &'a [SwitchCaseIr],
    ) -> Option<Self> {
        if cases.len() > 4096
            || builder.statement_list_value_context.is_some()
            || builder.current_function_meta().is_some_and(|meta| {
                meta.protocol().execution_kind() != FunctionExecutionKind::Ordinary
            })
        {
            return None;
        }
        let mut keys = BTreeMap::new();
        let mut default = None;
        for (index, case) in cases.iter().enumerate() {
            let Some(condition) = &case.condition else {
                if default.replace(index).is_some() {
                    return None;
                }
                continue;
            };
            let value = match &condition.expr {
                ExprIr::Number(bits) => f64::from_bits(*bits),
                ExprIr::UnaryMinusNumeric { expr } => {
                    let ExprIr::Number(bits) = &expr.expr else {
                        return None;
                    };
                    -f64::from_bits(*bits)
                }
                _ => return None,
            };
            if !value.is_finite()
                || value.fract() != 0.0
                || value < f64::from(i32::MIN)
                || value > f64::from(i32::MAX)
            {
                return None;
            }
            keys.entry(value as i32).or_insert(index);
        }
        let (&minimum, _) = keys.first_key_value()?;
        let (&maximum, _) = keys.last_key_value()?;
        let span = usize::try_from(i64::from(maximum) - i64::from(minimum) + 1).ok()?;
        if keys.len() < 4 || span > 4096 || span > keys.len() * 2 {
            return None;
        }
        let fallback = default.map(CaseEntry::Clause).unwrap_or(CaseEntry::Exit);
        let mut entries = vec![fallback; span];
        for (value, index) in keys {
            entries[(i64::from(value) - i64::from(minimum)) as usize] = CaseEntry::Clause(index);
        }
        Some(Self {
            cases,
            minimum,
            entries,
            fallback,
        })
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn compile_dense_integer_switch(
        &mut self,
        dispatch: &DenseIntegerSwitch<'_>,
        discriminant: &ValueLocals,
        labels: &[String],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let cases = dispatch.cases;
        let index = schema.reserve_i32_local(function);
        self.emit_statement_result(function);
        let break_frame = self.open_frame(ControlFrameKind::Block, function);
        self.breakable_stack.push(break_frame);
        self.push_labels(labels, break_frame, None);
        // Exiting the innermost block enters the first source body; each next
        // block ends after its predecessor, so ordinary execution falls through.
        let mut entries = Vec::with_capacity(cases.len());
        for _ in cases.iter().rev() {
            entries.push(self.open_frame(ControlFrameKind::Block, function));
        }
        entries.reverse();
        let target = |entry| match entry {
            CaseEntry::Clause(index) => entries[index],
            CaseEntry::Exit => break_frame,
        };

        discriminant.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::I32TruncSatF64S);
        index.store(function);
        discriminant.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
        function.instruction(&Instruction::I32Eq);
        discriminant.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        index.load(function);
        function.instruction(&Instruction::F64ConvertI32S);
        function.instruction(&Instruction::F64Eq);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        index.load(function);
        function.instruction(&Instruction::I32Const(dispatch.minimum));
        function.instruction(&Instruction::I32Sub);
        let fallback = target(dispatch.fallback);
        function.branch_table_to_labels(
            dispatch.entries.iter().map(|entry| target(*entry).label),
            fallback.label,
        );
        function.instruction(&Instruction::End);
        // No conversion, getter, or user code is allowed on a failed guard.
        function.branch_to_label(fallback.label);

        for case in cases {
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
            self.compile_switch_case_body(&case.body, function)?;
        }
        self.pop_labels(labels.len());
        self.breakable_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(index, function);
        Ok(())
    }
}
