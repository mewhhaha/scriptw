//! Guarded source calls belong to the surrounding closed Number region.
//! The numeric tree, row projection and publication tail remain the ordinary
//! scalar-loop owners. This module authenticates callable/cell inputs only.
use super::dense_array::{ReadKey, RegionInputSource};
use super::*;
use scriptw_ir::{
    CapturedBindingIr, EnvironmentIdentifierOperationIr as EnvOp,
    EnvironmentIdentifierResolutionStart, FunctionIr, ProvenBindingCellOrigin,
};
use std::sync::Arc;

mod source;
pub(crate) use source::CapturedCallSource;
mod roots;
pub(super) use roots::CapturedCallRoots;

const MAX_CALLS: usize = 5;
const MAX_CANDIDATES: usize = 4;
const MAX_SLOTS: usize = 32;

enum Target {
    Input(RegionInputSource),
    CapturedArray {
        parent: usize,
        capture: CapturedBindingIr,
        index: u32,
    },
}

struct Case {
    source_id: i64,
    body: Vec<NumberStatement>,
    result: NumberExpression,
    captures: Vec<usize>,
}

struct Call {
    target: Target,
    cases: Vec<Case>,
}

enum Slot {
    Temporary,
    Capture {
        binding: CapturedBindingIr,
        written: bool,
    },
}

#[derive(Default)]
pub(super) struct CapturedCalls {
    calls: Vec<Call>,
    slots: Vec<Slot>,
    depth: u32,
}

enum Argument {
    Row,
    Number(NumberExpression),
}

#[derive(Clone)]
enum Binding {
    Row,
    Number(usize),
    Array(CapturedBindingIr),
}

struct SourceCompiler<'a, 'b, 'c> {
    proof: &'a mut LoopProof<'b, 'c>,
    source: Arc<CapturedCallSource>,
    call: usize,
    bindings: BTreeMap<String, Binding>,
    captures: Vec<usize>,
    depth: usize,
    in_branch: bool,
}

pub(super) fn is_call(value: &TypedExpr) -> bool {
    match &value.expr {
        ExprIr::CallIndirect { .. } => true,
        ExprIr::EnvironmentIdentifier(reference) => {
            matches!(reference.operation, EnvOp::Call { .. })
        }
        _ => false,
    }
}

fn arguments(value: &TypedExpr) -> Option<&[TypedExpr]> {
    match &value.expr {
        ExprIr::CallIndirect {
            args,
            direct_eval: None,
            ..
        } => Some(args),
        ExprIr::EnvironmentIdentifier(reference) => match &reference.operation {
            EnvOp::Call {
                args,
                direct_eval: None,
                ..
            } => Some(args),
            _ => None,
        },
        _ => None,
    }
}

// Actual argument evaluation stays ordered; the first region only substitutes
// private numeric reads/arithmetic and the separately guarded row reference.
fn pure_argument(value: &TypedExpr) -> bool {
    match &value.expr {
        ExprIr::Number(_) | ExprIr::Identifier(_) => true,
        ExprIr::UnaryPlus { expr } | ExprIr::UnaryMinusNumeric { expr } => pure_argument(expr),
        ExprIr::BinaryNumber { lhs, rhs, .. }
        | ExprIr::CoerciveBinaryNumber { lhs, rhs, .. }
        | ExprIr::CoerciveAdd { lhs, rhs }
        | ExprIr::BitwiseNumeric { lhs, rhs, .. } => pure_argument(lhs) && pure_argument(rhs),
        _ => false,
    }
}

pub(super) fn row_argument(value: &TypedExpr) -> Option<PropertyReadView<'_>> {
    let mut reads = arguments(value)?
        .iter()
        .filter_map(PropertyReadView::of)
        .filter(|read| matches!(read.key, ReadKey::Computed(_)));
    let read = reads.next()?;
    if reads.next().is_some() {
        return None;
    }
    Some(read)
}

fn input_for_reference(
    builder: &FunctionBuilder<'_>,
    reference: &scriptw_ir::EnvironmentIdentifierIr,
) -> Option<RegionInputSource> {
    let EnvironmentIdentifierResolutionStart::Cell(proof) = reference.resolution_start() else {
        return None;
    };
    if !matches!(proof.origin(), ProvenBindingCellOrigin::Captured { .. }) {
        return None;
    }
    builder.module_cell_storage(&proof, &reference.name).ok()?;
    Some(RegionInputSource::Cell(proof))
}

pub(super) fn compile(
    proof: &mut LoopProof<'_, '_>,
    value: &TypedExpr,
    depth: usize,
) -> Option<NumberExpression> {
    let (target, hints, arguments) = match &value.expr {
        ExprIr::EnvironmentIdentifier(reference) => {
            let EnvOp::Call {
                args,
                direct_eval: None,
                callee_targets,
            } = &reference.operation
            else {
                return None;
            };
            (
                input_for_reference(proof.builder, reference)?,
                callee_targets,
                args,
            )
        }
        ExprIr::CallIndirect {
            callee,
            this_arg: None,
            args,
            direct_eval: None,
            ..
        } => (
            RegionInputSource::of(proof.builder, callee)?,
            &callee.function_targets,
            args,
        ),
        _ => return None,
    };
    let mut values = Vec::new();
    for argument in arguments {
        if matches!(&proof.invariant, Some(RegionInvariant::ArrayReads(array)) if array.is_row(proof.builder, argument))
        {
            values.push(Argument::Row);
        } else {
            if !pure_argument(argument) {
                return None;
            }
            values.push(Argument::Number(proof.expression(argument, depth)?));
        }
    }
    let candidates = candidates(proof.builder, values.len(), hints.known_targets())?;
    let call = compile_site(proof, Target::Input(target), &values, candidates, 1, depth)?;
    Some(NumberExpression::Call(call))
}

fn candidates(
    builder: &FunctionBuilder<'_>,
    arity: usize,
    hints: &BTreeSet<FunctionId>,
) -> Option<Vec<Arc<CapturedCallSource>>> {
    let sources: Vec<_> = builder
        .functions
        .metas()
        .iter()
        .filter_map(|(id, meta)| {
            if !hints.is_empty() && !hints.contains(id) {
                return None;
            }
            meta.captured_call
                .as_ref()
                .filter(|source| source.function.params.len() == arity)
                .cloned()
        })
        .collect();
    if sources.is_empty() || sources.len() > MAX_CANDIDATES {
        return None;
    }
    Some(sources)
}

fn compile_site(
    proof: &mut LoopProof<'_, '_>,
    target: Target,
    arguments: &[Argument],
    sources: Vec<Arc<CapturedCallSource>>,
    nesting: u32,
    depth: usize,
) -> Option<usize> {
    if proof.calls.calls.len() == MAX_CALLS || nesting > 2 {
        return None;
    }
    // Forwarding is admitted only for one guarded source; inactive alternative
    // contexts must never authorize preparation of nested callable roots.
    if sources.len() > 1 && sources.iter().any(|source| source.forwarding) {
        return None;
    }
    let call = proof.calls.calls.len();
    proof.calls.calls.push(Call {
        target,
        cases: Vec::new(),
    });
    proof.calls.depth = proof.calls.depth.max(nesting);
    for source in sources {
        if nesting == 2 && source.forwarding {
            return None;
        }
        let source_id = proof
            .builder
            .functions
            .get(&source.function.id)?
            .entry
            .source_id();
        let mut compiler = SourceCompiler {
            proof,
            source: source.clone(),
            call,
            bindings: BTreeMap::new(),
            captures: Vec::new(),
            depth,
            in_branch: false,
        };
        let mut body = Vec::new();
        for (parameter, argument) in source.function.params.iter().zip(arguments) {
            let binding = match argument {
                Argument::Row => Binding::Row,
                Argument::Number(value) => {
                    let slot = compiler.temporary()?;
                    body.push(NumberStatement::Initialize(
                        NumberExpression::AssignCallSlot(slot, Box::new(value.clone())),
                    ));
                    Binding::Number(slot)
                }
            };
            compiler.bindings.insert(parameter.name.clone(), binding);
        }
        let (last, statements) = source.function.body.statements.split_last()?;
        let StatementIr::Return(result) = last else {
            return None;
        };
        for statement in statements {
            compiler.statement(statement, &mut body, true)?;
        }
        let result = compiler.expression(result)?;
        let captures = compiler.captures;
        proof.calls.calls[call].cases.push(Case {
            source_id,
            body,
            result,
            captures,
        });
    }
    Some(call)
}

impl SourceCompiler<'_, '_, '_> {
    fn visit(&mut self) -> Option<()> {
        self.proof.remaining = self.proof.remaining.checked_sub(1)?;
        Some(())
    }
    fn temporary(&mut self) -> Option<usize> {
        if self.proof.calls.slots.len() == MAX_SLOTS {
            return None;
        }
        let slot = self.proof.calls.slots.len();
        self.proof.calls.slots.push(Slot::Temporary);
        Some(slot)
    }
    fn capture_binding(
        &self,
        reference: &scriptw_ir::EnvironmentIdentifierIr,
    ) -> Option<CapturedBindingIr> {
        let EnvironmentIdentifierResolutionStart::Cell(proof) = reference.resolution_start() else {
            return None;
        };
        let ProvenBindingCellOrigin::Captured { entry_hops } = proof.origin() else {
            return None;
        };
        if proof.owner() != self.source.function.id || proof.source_name() != reference.name {
            return None;
        }
        self.source
            .function
            .captured_bindings
            .iter()
            .find(|binding| {
                binding.name == proof.storage_name()
                    && binding.source_name == reference.name
                    && binding.slot == proof.slot()
                    && binding.hops == entry_hops
            })
            .cloned()
    }
    fn capture(
        &mut self,
        reference: &scriptw_ir::EnvironmentIdentifierIr,
        written: bool,
    ) -> Option<usize> {
        let binding = self.capture_binding(reference)?;
        if let Some(index) = self.captures.iter().copied().find(|index| matches!(&self.proof.calls.slots[*index], Slot::Capture { binding: old, .. } if old == &binding)) {
            let Slot::Capture { written: old, .. } = &mut self.proof.calls.slots[index] else { unreachable!() };
            *old |= written;
            return Some(index);
        }
        if self.proof.calls.slots.len() == MAX_SLOTS {
            return None;
        }
        let index = self.proof.calls.slots.len();
        self.proof
            .calls
            .slots
            .push(Slot::Capture { binding, written });
        self.captures.push(index);
        Some(index)
    }
    fn assign(
        &mut self,
        name: &str,
        value: &TypedExpr,
        declaration: bool,
    ) -> Option<NumberExpression> {
        let expression = self.expression(value)?;
        let slot = match self.bindings.get(name) {
            Some(Binding::Number(index)) => *index,
            Some(_) | None
                if declaration
                    || (!self.in_branch
                        && self
                            .source
                            .function
                            .params
                            .iter()
                            .any(|parameter| parameter.name == name)) =>
            {
                let slot = self.temporary()?;
                self.bindings.insert(name.to_owned(), Binding::Number(slot));
                slot
            }
            _ => return None,
        };
        Some(NumberExpression::AssignCallSlot(slot, Box::new(expression)))
    }
    fn statement(
        &mut self,
        statement: &StatementIr,
        out: &mut Vec<NumberStatement>,
        declarations: bool,
    ) -> Option<()> {
        self.visit()?;
        match statement {
            StatementIr::Empty
            | StatementIr::Expression(TypedExpr {
                expr: ExprIr::String(_),
                ..
            }) => {}
            StatementIr::Expression(value) => {
                out.push(NumberStatement::Expression(self.expression(value)?))
            }
            StatementIr::Var(values) if declarations => {
                for value in values {
                    let init = value.init.as_ref()?;
                    if let ExprIr::Identifier(other) = &init.expr {
                        if let Some(Binding::Row) = self.bindings.get(other) {
                            self.bindings.insert(value.name.clone(), Binding::Row);
                            continue;
                        }
                    }
                    out.push(NumberStatement::Initialize(self.assign(
                        &value.name,
                        init,
                        true,
                    )?));
                }
            }
            StatementIr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let test = self.predicate(condition)?;
                let mut consequent = Vec::new();
                let mut alternate = Vec::new();
                let previous = self.in_branch;
                self.in_branch = true;
                self.statement(then_branch, &mut consequent, false)?;
                if let Some(branch) = else_branch {
                    self.statement(branch, &mut alternate, false)?;
                }
                self.in_branch = previous;
                out.push(NumberStatement::If {
                    test,
                    consequent,
                    alternate,
                });
            }
            StatementIr::Block(block) if block.lexical_environment.is_none() => {
                for statement in &block.statements {
                    self.statement(statement, out, declarations)?;
                }
            }
            _ => return None,
        }
        Some(())
    }
    fn predicate(&mut self, value: &TypedExpr) -> Option<NumberPredicate> {
        Some(match &value.expr {
            ExprIr::Boolean(value) => NumberPredicate::Boolean(*value),
            ExprIr::LogicalNot { expr } => NumberPredicate::Not(Box::new(self.predicate(expr)?)),
            ExprIr::CompareNumber { op, lhs, rhs } | ExprIr::CompareValue { op, lhs, rhs } => {
                NumberPredicate::Compare(*op, self.expression(lhs)?, self.expression(rhs)?)
            }
            _ => NumberPredicate::Truthy(self.expression(value)?),
        })
    }
    fn expression(&mut self, value: &TypedExpr) -> Option<NumberExpression> {
        self.visit()?;
        if let Some(read) = PropertyReadView::of(value) {
            let ExprIr::Identifier(target) = &read.target.expr else {
                return None;
            };
            if !matches!(self.bindings.get(target), Some(Binding::Row)) {
                return None;
            }
            let ReadKey::StaticString(name) = read.key else {
                return None;
            };
            let Some(RegionInvariant::ArrayReads(array)) = &mut self.proof.invariant else {
                return None;
            };
            return Some(NumberExpression::CompactPath(
                array.capture_field(self.proof.builder, name)?,
            ));
        }
        Some(match &value.expr {
            ExprIr::Number(bits) => NumberExpression::Constant(*bits),
            ExprIr::Identifier(name) => {
                let Binding::Number(index) = self.bindings.get(name)? else {
                    return None;
                };
                NumberExpression::CallSlot(*index)
            }
            ExprIr::EnvironmentIdentifier(reference) => match &reference.operation {
                EnvOp::Read => NumberExpression::CallSlot(self.capture(reference, false)?),
                EnvOp::Assign { value } => NumberExpression::AssignCallSlot(
                    self.capture(reference, true)?,
                    Box::new(self.expression(value)?),
                ),
                _ => return None,
            },
            ExprIr::AssignIdentifier { name, value } => return self.assign(name, value, false),
            ExprIr::CoerciveAdd { lhs, rhs } => NumberExpression::Arithmetic(
                NumberArithmetic::Add,
                Box::new(self.expression(lhs)?),
                Box::new(self.expression(rhs)?),
            ),
            ExprIr::BinaryNumber { op, lhs, rhs }
            | ExprIr::CoerciveBinaryNumber { op, lhs, rhs } => NumberExpression::Arithmetic(
                NumberArithmetic::new(*op)?,
                Box::new(self.expression(lhs)?),
                Box::new(self.expression(rhs)?),
            ),
            ExprIr::BitwiseNumeric { op, lhs, rhs } => NumberExpression::Bitwise(
                *op,
                Box::new(self.expression(lhs)?),
                Box::new(self.expression(rhs)?),
            ),
            ExprIr::UnaryPlus { expr } => return self.expression(expr),
            ExprIr::UnaryMinusNumeric { expr } => {
                NumberExpression::Negate(Box::new(self.expression(expr)?))
            }
            ExprIr::MaterializeBinding { name, value, body } if self.source.forwarding => {
                let ExprIr::EnvironmentIdentifier(reference) = &value.expr else {
                    return None;
                };
                if !matches!(reference.operation, EnvOp::Read) || self.bindings.contains_key(name) {
                    return None;
                }
                self.bindings.insert(
                    name.clone(),
                    Binding::Array(self.capture_binding(reference)?),
                );
                let result = self.expression(body)?;
                self.bindings.remove(name);
                result
            }
            ExprIr::CallIndirect {
                callee,
                args,
                this_arg,
                direct_eval: None,
                ..
            } if self.source.forwarding => {
                if let Some(receiver) = this_arg {
                    if receiver.expr != PropertyReadView::of(callee)?.target.expr {
                        return None;
                    }
                }
                self.forward(callee, args)?
            }
            _ => return None,
        })
    }
    fn forward(&mut self, callee: &TypedExpr, args: &[TypedExpr]) -> Option<NumberExpression> {
        let read = PropertyReadView::of(callee)?;
        let ExprIr::Identifier(name) = &read.target.expr else {
            return None;
        };
        let Binding::Array(binding) = self.bindings.get(name)?.clone() else {
            return None;
        };
        let ReadKey::Computed(index) = read.key else {
            return None;
        };
        let ExprIr::BitwiseNumeric {
            op: BitwiseBinaryOp::And,
            rhs,
            ..
        } = &index.expr
        else {
            return None;
        };
        let ExprIr::Number(bits) = rhs.expr else {
            return None;
        };
        let mask = super::chunked::bounded_counter_literal(bits)?;
        if mask > 3 || !(mask + 1).is_power_of_two() {
            return None;
        }
        let index = self.expression(index)?;
        let mut arguments = Vec::new();
        for argument in args {
            if matches!(&argument.expr, ExprIr::Identifier(name) if matches!(self.bindings.get(name), Some(Binding::Row)))
            {
                arguments.push(Argument::Row);
            } else {
                if !pure_argument(argument) {
                    return None;
                }
                arguments.push(Argument::Number(self.expression(argument)?));
            }
        }
        let sources = candidates(
            self.proof.builder,
            arguments.len(),
            callee.function_targets.known_targets(),
        )?;
        let mut sites = Vec::new();
        for index in 0..=mask {
            sites.push(compile_site(
                self.proof,
                Target::CapturedArray {
                    parent: self.call,
                    capture: binding.clone(),
                    index,
                },
                &arguments,
                sources.clone(),
                2,
                self.depth,
            )?);
        }
        Some(NumberExpression::SelectCall(Box::new(index), sites))
    }
}

impl CapturedCalls {
    pub(super) fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }
    pub(super) fn emit_call(
        &self,
        index: usize,
        roots: &CapturedCallRoots,
        builder: &mut FunctionBuilder<'_>,
        locals: &LoopLocals<'_>,
        function: &mut Function,
    ) {
        let result = builder.runtime_schema().reserve_f64_local(function);
        for (case_index, case) in self.calls[index].cases.iter().enumerate() {
            roots.selected(index, function);
            function.instruction(&Instruction::I32Const(case_index as i32));
            function.instruction(&Instruction::I32Eq);
            builder.open_frame(ControlFrameKind::If, function);
            NumberStatement::emit_list(&case.body, builder, locals, None, function);
            case.result.emit(builder, locals, function);
            result.store(function);
            builder.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        result.load(function);
        builder.runtime_schema().release_f64_local(result, function);
    }
    pub(super) fn emit_selection(
        &self,
        sites: &[usize],
        roots: &CapturedCallRoots,
        builder: &mut FunctionBuilder<'_>,
        locals: &LoopLocals<'_>,
        function: &mut Function,
    ) {
        let s = builder.runtime_schema();
        let index = s.reserve_i32_local(function);
        index.store(function);
        let result = s.reserve_f64_local(function);
        for (slot, site) in sites.iter().enumerate() {
            index.load(function);
            function.instruction(&Instruction::I32Const(slot as i32));
            function.instruction(&Instruction::I32Eq);
            builder.open_frame(ControlFrameKind::If, function);
            self.emit_call(*site, roots, builder, locals, function);
            result.store(function);
            builder.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        result.load(function);
        s.release_f64_local(result, function);
        s.release_i32_local(index, function);
    }
}
