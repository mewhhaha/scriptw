//! A straight-line Number body reuses its actual local-argument source entry.
//! A failed preflight reaches unchanged FDI; successful writes use real cells.
use super::numeric_leaf_call::NumericLeafExpression;
use super::*;
use crate::gc_types::{
    BindingCell, BindingCellSchema, GcNullability, NonNullable, Nullable, OrdinaryObject,
    StoredValue,
};
use crate::operations::PropertyKeyLocals;
use scriptw_ir::{
    EnvironmentIdentifierOperationIr as EnvOp, EnvironmentIdentifierResolutionStart, PropertyKeyIr,
    ProvenBindingCellOrigin, SpecOperationIr,
};

#[derive(Clone, Copy)]
enum Binding {
    Argument(usize),
    Number(usize),
}

#[derive(Clone, PartialEq, Eq)]
enum Input {
    Argument(usize),
    OwnField { argument: usize, name: String },
    Capture { slot: u32, hops: u32 },
}

struct Assignment {
    destination: usize,
    value: NumericLeafExpression,
    capture: Option<usize>,
}

/// No source evaluation is replayed: only immutable entry arguments and proved
/// declarative cells may feed a complete, callback-free straight-line body.
pub(crate) struct NumericBodyPlan {
    source: FunctionId,
    arguments: usize,
    inputs: Vec<(Input, usize)>,
    assignments: Vec<Assignment>,
    slots: usize,
    result: NumericLeafExpression,
}

struct Planner<'a> {
    source: &'a FunctionIr,
    bindings: BTreeMap<String, Binding>,
    inputs: Vec<(Input, usize)>,
    assignments: Vec<Assignment>,
    slots: usize,
    remaining: usize,
}

impl NumericBodyPlan {
    pub(crate) fn admit(source: &FunctionIr) -> Option<Self> {
        crate::function_entry::LocalArgumentsPlan::for_function(source)?;
        if source.eval_environment.is_some()
            || !source.owned_env_bindings.is_empty()
            || source.body.lexical_environment.is_some()
            || source.is_named_expression
            || source.captures_private_environment
            || source.captures_lexical_this
            || source.uses_super
            || !source.private_name_ids.is_empty()
        {
            return None;
        }
        let (last, body) = source.body.statements.split_last()?;
        let StatementIr::Return(result) = last else {
            return None;
        };
        let mut planner = Planner {
            source,
            bindings: source
                .params
                .iter()
                .enumerate()
                .map(|(index, param)| (param.name.clone(), Binding::Argument(index)))
                .collect(),
            inputs: Vec::new(),
            assignments: Vec::new(),
            slots: 0,
            remaining: 64,
        };
        for statement in body {
            planner.remaining = planner.remaining.checked_sub(1)?;
            match statement {
                StatementIr::Empty => {}
                StatementIr::Var(declarations) => {
                    for declaration in declarations {
                        // Hoisted but unread variables may stay uninitialized here;
                        // a later read needs an actual dominating initialization.
                        if let Some(value) = &declaration.init {
                            planner.assign_local(&declaration.name, value)?;
                        }
                    }
                }
                StatementIr::Expression(value) => match &value.expr {
                    ExprIr::AssignIdentifier { name, value } => {
                        planner.assign_local(name, value)?
                    }
                    ExprIr::EnvironmentIdentifier(reference) => {
                        let EnvOp::Assign { value } = &reference.operation else {
                            return None;
                        };
                        let capture = planner.capture(reference)?;
                        let value = planner.expression(value)?;
                        let destination = planner.slot();
                        planner.assignments.push(Assignment {
                            destination,
                            value,
                            capture: Some(capture),
                        });
                        let binding = &source.captured_bindings[capture];
                        planner
                            .bindings
                            .insert(binding.name.clone(), Binding::Number(destination));
                    }
                    // Directive prologues retain only their inert String value.
                    ExprIr::String(_) => {}
                    _ => return None,
                },
                _ => return None,
            }
        }
        let result = planner.expression(result)?;
        // Pure leaves already have their own smaller owner. This discriminator
        // requires the live cell+own-field work that that owner cannot admit.
        if !planner
            .assignments
            .iter()
            .any(|step| step.capture.is_some())
            || !planner
                .inputs
                .iter()
                .any(|(input, _)| matches!(input, Input::OwnField { .. }))
        {
            return None;
        }
        Some(Self {
            source: source.id.clone(),
            arguments: source.params.len(),
            inputs: planner.inputs,
            assignments: planner.assignments,
            slots: planner.slots,
            result,
        })
    }
}

impl Planner<'_> {
    fn slot(&mut self) -> usize {
        let slot = self.slots;
        self.slots += 1;
        slot
    }
    fn input(&mut self, input: Input) -> usize {
        if let Some((_, slot)) = self.inputs.iter().find(|(previous, _)| *previous == input) {
            return *slot;
        }
        let slot = self.slot();
        self.inputs.push((input, slot));
        slot
    }
    fn expression(&mut self, value: &TypedExpr) -> Option<NumericLeafExpression> {
        let mut remaining = self.remaining;
        let result = NumericLeafExpression::parse(value, &mut remaining, true, &mut |value| {
            self.number(value)
        });
        self.remaining = remaining;
        result
    }
    fn assign_local(&mut self, name: &str, value: &TypedExpr) -> Option<()> {
        self.remaining = self.remaining.checked_sub(1)?;
        // Local identifiers cannot name an owned/captured/environment record.
        if self
            .source
            .captured_bindings
            .iter()
            .any(|capture| capture.name == name)
        {
            return None;
        }
        if let ExprIr::Identifier(other) = &value.expr {
            if let Some(Binding::Argument(argument)) = self.bindings.get(other).copied() {
                self.bindings
                    .insert(name.to_owned(), Binding::Argument(argument));
                return Some(());
            }
        }
        let value = self.expression(value)?;
        let destination = self.slot();
        self.assignments.push(Assignment {
            destination,
            value,
            capture: None,
        });
        self.bindings
            .insert(name.to_owned(), Binding::Number(destination));
        Some(())
    }
    fn capture(&self, reference: &scriptw_ir::EnvironmentIdentifierIr) -> Option<usize> {
        let EnvironmentIdentifierResolutionStart::Cell(proof) = reference.resolution_start() else {
            return None;
        };
        if proof.owner() != self.source.id || proof.source_name() != reference.name {
            return None;
        }
        let ProvenBindingCellOrigin::Captured { entry_hops } = proof.origin() else {
            return None;
        };
        self.source.captured_bindings.iter().position(|capture| {
            capture.name == proof.storage_name()
                && capture.source_name == proof.source_name()
                && capture.slot == proof.slot()
                && capture.hops == entry_hops
                && capture.mode != BindingMode::Const
        })
    }
    fn number(&mut self, value: &TypedExpr) -> Option<usize> {
        match &value.expr {
            ExprIr::Identifier(name) => match self.bindings.get(name).copied()? {
                Binding::Argument(argument) => Some(self.input(Input::Argument(argument))),
                Binding::Number(slot) => Some(slot),
            },
            ExprIr::EnvironmentIdentifier(reference)
                if matches!(reference.operation, EnvOp::Read) =>
            {
                let index = self.capture(reference)?;
                let capture = &self.source.captured_bindings[index];
                if let Some(Binding::Number(slot)) = self.bindings.get(&capture.name) {
                    return Some(*slot);
                }
                Some(self.input(Input::Capture {
                    slot: capture.slot,
                    hops: capture.hops,
                }))
            }
            _ => {
                let (target, name) = match &value.expr {
                    ExprIr::PropertyRead {
                        target,
                        key: PropertyKeyIr::StaticString(name),
                    } => (target.as_ref(), name),
                    ExprIr::SpecOperation {
                        operation: SpecOperationIr::GetV,
                        operands,
                    } => {
                        let [target, key] = operands.as_slice() else {
                            return None;
                        };
                        let ExprIr::String(name) = &key.expr else {
                            return None;
                        };
                        (target, name)
                    }
                    _ => return None,
                };
                let ExprIr::Identifier(name_of_target) = &target.expr else {
                    return None;
                };
                let Binding::Argument(argument) = self.bindings.get(name_of_target).copied()?
                else {
                    return None;
                };
                Some(self.input(Input::OwnField {
                    argument,
                    name: name.clone(),
                }))
            }
        }
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_numeric_body_prefix(
        &mut self,
        plan: NumericBodyPlan,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        assert!(self
            .planned_entry()
            .is_some_and(|entry| entry.local_arguments().is_some()));
        assert_eq!(self.function_id.as_ref(), Some(&plan.source));
        let schema = self.runtime_schema();
        let arguments: Vec<_> = (0..plan.arguments)
            .map(|index| {
                let value = schema.reserve_value_local(function);
                assert!(self
                    .body_entry_locals()
                    .expect("source entry")
                    .copy_local_argument(index, &value, function));
                value
            })
            .collect();
        let numbers: Vec<_> = (0..plan.slots)
            .map(|_| schema.reserve_i64_local(function))
            .collect();
        // Every nullable root exists before a decline edge, including cells
        // that a later preflight would otherwise leave uninitialized.
        let cells: Vec<_> = self
            .captured_bindings
            .iter()
            .map(|_| {
                schema
                    .reserve_gc_local::<BindingCell, Nullable>(function)
                    .initialize_null(schema, function)
            })
            .collect();
        let pending = schema.reserve_value_local(function);
        let accepted = schema.reserve_i32_local(function);
        accepted.set_constant(0, function);
        let cell_flag = schema.reserve_i32_local(function);
        let cell_mutable = schema.reserve_i32_local(function);
        let declined = self.open_frame(ControlFrameKind::Block, function);
        // Authenticate every cell that will be written, even a write-only one.
        for (index, capture) in self.captured_bindings.iter().enumerate() {
            if !plan.assignments.iter().any(|step| step.capture == Some(index))
                && !plan.inputs.iter().any(|(input, _)| matches!(input, Input::Capture { slot, hops } if *slot == capture.slot && *hops == capture.hops)) { continue; }
            let environment = self.resolve_env_handle_local(capture.hops, function);
            let cell = self.emit_environment_cell_local(&environment, capture.slot, function);
            cells[index].replace(cell.load(schema, function).nullable(), function);
            environment.clear(function);
            schema
                .field(BindingCellSchema::INITIALIZED)
                .read(&cell, schema, function)
                .store(cell_flag, function);
            schema
                .field(BindingCellSchema::MUTABLE)
                .read(&cell, schema, function)
                .store(cell_mutable, function);
            cell_mutable.load(function);
            cell_flag.load(function);
            function.instruction(&Instruction::I32And);
            schema
                .field(BindingCellSchema::TARGET)
                .read(&cell, schema, function)
                .reference()
                .is_null(function);
            function.instruction(&Instruction::I32And);
            cell.clear(function);
            function.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(declined, function);
        }
        for (input, slot) in &plan.inputs {
            match input {
                Input::Argument(index) => pending.copy_from(&arguments[*index], function),
                Input::Capture { slot, hops } => {
                    let index = self
                        .captured_bindings
                        .iter()
                        .position(|capture| capture.slot == *slot && capture.hops == *hops)
                        .expect("validated capture");
                    let stored = schema
                        .reserve_gc_local::<StoredValue, NonNullable>(function)
                        .initialize(
                            schema
                                .field(BindingCellSchema::VALUE)
                                .read(&cells[index], schema, function)
                                .reference(),
                            function,
                        );
                    schema
                        .struct_type::<StoredValue>()
                        .read_into(&stored, &pending, schema, function);
                    stored.clear(function);
                }
                Input::OwnField { argument, name } => {
                    let target = &arguments[*argument];
                    target.tag().load(function);
                    function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Object.tag()));
                    function.instruction(&Instruction::I32Eq);
                    target.reference().load(function);
                    function.instruction(&Instruction::RefTestNonNull(
                        schema
                            .reference_type::<OrdinaryObject>(GcNullability::NonNullable)
                            .heap_type,
                    ));
                    function.instruction(&Instruction::I32And);
                    function.instruction(&Instruction::I32Eqz);
                    self.emit_branch_if_to_target(declined, function);
                    let text = schema.reserve_gc_local(function).initialize(
                        self.emit_interned_string_reference(name, function)?,
                        function,
                    );
                    let key = PropertyKeyLocals::from_string(schema, &text, function);
                    let object = schema.reserve_gc_local(function).initialize(
                        target.cast_reference::<OrdinaryObject>(schema, function),
                        function,
                    );
                    let found = schema.reserve_i32_local(function);
                    self.emit_ordinary_own_data_value(&object, &key, &pending, found, function)?;
                    object.clear(function);
                    key.clear(function);
                    text.clear(function);
                    found.load(function);
                    schema.release_i32_local(found, function);
                    function.instruction(&Instruction::I32Eqz);
                    self.emit_branch_if_to_target(declined, function);
                }
            }
            pending.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number.tag()));
            function.instruction(&Instruction::I32Ne);
            self.emit_branch_if_to_target(declined, function);
            pending.scalar().load(function);
            numbers[*slot].store(function);
        }
        accepted.set_constant(1, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        // No retained row/descriptor snapshot survives into either source arm.
        pending.clear(function);
        for argument in arguments {
            argument.clear(function);
        }
        accepted.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let value = schema.reserve_value_local(function);
        for assignment in &plan.assignments {
            assignment.value.emit_parameters(
                schema,
                &|index, function| {
                    numbers[index].load(function);
                    function.instruction(&Instruction::F64ReinterpretI64);
                },
                function,
            );
            function.instruction(&Instruction::I64ReinterpretF64);
            numbers[assignment.destination].store(function);
            if let Some(index) = assignment.capture {
                value.set_number(numbers[assignment.destination], function);
                let cell = schema.reserve_gc_local(function).initialize(
                    cells[index]
                        .load(schema, function)
                        .require_non_null(function),
                    function,
                );
                self.emit_write_environment_cell(&cell, &value, function);
                cell.clear(function);
            }
        }
        plan.result.emit_parameters(
            schema,
            &|index, function| {
                numbers[index].load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
            },
            function,
        );
        function.instruction(&Instruction::I64ReinterpretF64);
        value.scalar().store(function);
        value.set_number(value.scalar(), function);
        self.completion().set_normal(&value, function);
        value.clear(function);
        // Both arms retire roots before either shared source epilogue or FDI.
        for cell in &cells {
            cell.set_null(schema, function);
        }
        self.set_completion_kind(CompletionKind::Return, function);
        self.emit_return_current_completion(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for cell in cells {
            cell.clear(function);
        }
        schema.release_i32_local(cell_mutable, function);
        schema.release_i32_local(cell_flag, function);
        schema.release_i32_local(accepted, function);
        for number in numbers {
            schema.release_i64_local(number, function);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};

    fn plans(source: &str) -> Vec<NumericBodyPlan> {
        let ir = scriptw_ir::lower(&parse(source, ParseOptions::module()).unwrap());
        assert!(ir.is_wasm_supported(), "{:?}", ir.diagnostics);
        ir.script
            .as_ref()
            .unwrap()
            .functions
            .iter()
            .filter_map(NumericBodyPlan::admit)
            .collect()
    }

    #[test]
    fn unchanged_assigned_step_has_one_body_plan_and_frame_has_none() {
        let plans = plans(include_str!(
            "../../tests/emission/fixtures/numeric_body_args_unused.mjs"
        ));
        assert_eq!(plans.len(), 1);
        let plan = &plans[0];
        assert_eq!(plan.arguments, 2);
        assert_eq!(
            plan.assignments
                .iter()
                .filter(|step| step.capture.is_some())
                .count(),
            1
        );
        assert_eq!(
            plan.inputs
                .iter()
                .filter(|(input, _)| matches!(input, Input::OwnField { .. }))
                .count(),
            2
        );
        assert_eq!(plans_for_frame(), 0);
    }
    fn plans_for_frame() -> usize {
        plans(include_str!(
            "../../tests/emission/fixtures/numeric_body_frame_compact.mjs"
        ))
        .len()
    }
    #[test]
    fn actual_observed_and_unused_arguments_have_disjoint_entry_routes() {
        for (text, observed) in [
            (
                include_str!("../../tests/emission/fixtures/numeric_body_args_unused.mjs"),
                false,
            ),
            (
                include_str!("../../tests/fixtures/strict_arguments_observed.js"),
                true,
            ),
        ] {
            let ir = scriptw_ir::lower(&parse(text, ParseOptions::module()).unwrap());
            let functions: Vec<_> = ir
                .script
                .as_ref()
                .unwrap()
                .functions
                .iter()
                .filter(|function| {
                    function.params.len() == 2
                        && function
                            .captured_bindings
                            .iter()
                            .any(|capture| capture.source_name == "tick")
                })
                .collect();
            assert_eq!(functions.len(), 1, "the original assigned source step");
            let source = functions[0];
            assert_eq!(
                crate::arguments_protocol::StrictArgumentsProjection::admit(source).is_some(),
                observed
            );
            assert_eq!(
                crate::function_entry::LocalArgumentsPlan::for_function(source).is_some(),
                !observed
            );
            assert_eq!(NumericBodyPlan::admit(source).is_some(), !observed);
        }
    }

    #[test]
    fn complete_body_refuses_observers_calls_control_and_environment_storage() {
        for body in [
            "tick=tick+1; if(row.x) return tick; return row.y;",
            "tick=tick+1; return row.x + callback();",
            "tick=tick+1; return row.x + arguments.length;",
            "tick=tick+1; return row.x + eval('1');",
            "tick=tick+1; return row.x + (()=>tick)();",
            "tick=tick+1; row.x=2; return row.x;",
            "tick=tick+1; return row[key];",
            "let value=row.x; tick=tick+1; return value;",
        ] {
            assert!(
                plans(&format!(
                    "var tick=0; export function step(row,i){{{body}}}"
                ))
                .is_empty(),
                "{body}"
            );
        }
        assert!(
            plans("var tick=0; export function step(row,i=0){tick=tick+1;return row.x+i;}")
                .is_empty()
        );
        assert!(plans(
            "var tick=0; export function step(row,i,extra){tick=tick+1;return row.x+i;}"
        )
        .is_empty());
        assert!(
            plans("const tick=0; export function step(row,i){tick=1;return row.x+i;}").is_empty()
        );
    }
    #[test]
    fn aliases_snapshot_entry_values_and_numeric_copies_keep_source_order() {
        let plans=plans("var tick=0; export function step(row,i){var old=row; row=i+tick; var before=tick; tick=(tick+1)&255; var after=tick; return old.x+row+before+after;}");
        assert_eq!(plans.len(), 1);
        let plan = &plans[0];
        assert_eq!(plan.assignments.len(), 4);
        assert!(matches!(
            plan.inputs
                .iter()
                .find(|(input, _)| matches!(input, Input::OwnField { .. }))
                .unwrap()
                .0,
            Input::OwnField { argument: 0, .. }
        ));
        // The result reads distinct scalar destinations for before/after;
        // neither local aliases the mutable capture's storage.
        assert_ne!(
            plan.assignments[1].destination,
            plan.assignments[3].destination
        );
    }
}
