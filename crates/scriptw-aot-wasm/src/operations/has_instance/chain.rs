//! Share a receiver's prototype walk across guards in a statement list.
//!
//! Every path between participating sites either evaluates only inert guards
//! and guarded `instanceof` sites, or exits the run. A site's record can
//! answer its global read and `@@hasInstance` check without running user
//! code. Its slow path forgets the walk and shared proof before either read
//! can run user code. Ordinary objects, functions and Arrays have inert
//! [[GetPrototypeOf]]. Their edges stay stable through fast answers and
//! inert tests. Each entry to the statement list starts with no proof.
use super::*;
use crate::emit::BindingStorage;
use crate::gc_types::{
    Environment, EnvironmentSchema, InstanceofSiteRecord, InstanceofSiteRecordSchema,
};

/// The prefix of V's chain the first site keeps: an ordinary object, a
/// class instance and a subclass instance end within it at %Object.prototype%.
const CHAIN_SLOTS: usize = 3;

/// A statement-list prefix containing at least two participating sites.
struct InstanceofRun<'a> {
    statements: &'a [StatementIr],
    sites: Vec<*const TypedExpr>,
}

impl<'a> InstanceofRun<'a> {
    fn of_prefix(builder: &FunctionBuilder<'_>, statements: &'a [StatementIr]) -> Option<Self> {
        if !builder.has_source_execution_environment() {
            return None;
        }
        let mut scan = RunScan {
            builder,
            binding: None,
            sites: Vec::new(),
        };
        let mut length = 0;
        for statement in statements {
            let checkpoint = (scan.binding, scan.sites.len());
            if !scan.statement(statement, false) {
                scan.binding = checkpoint.0;
                scan.sites.truncate(checkpoint.1);
                // A final if or expression can end in arbitrary code along
                // a branch. No later site in this run can see its effects.
                scan.statement(statement, true);
                length += usize::from(scan.sites.len() > checkpoint.1);
                break;
            }
            length += 1;
            if scan.sites.is_empty() {
                return None;
            }
            if never_completes_normally(statement) {
                break;
            }
        }
        (scan.sites.len() >= 2).then(|| Self {
            statements: &statements[..length],
            sites: scan.sites,
        })
    }
}

struct RunScan<'a, 'source> {
    builder: &'a FunctionBuilder<'source>,
    binding: Option<BindingStorage>,
    sites: Vec<*const TypedExpr>,
}

impl RunScan<'_, '_> {
    /// With `tail`, a branch may execute arbitrary code when exiting the run.
    fn statement(&mut self, statement: &StatementIr, tail: bool) -> bool {
        match statement {
            StatementIr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if !self.test(condition) {
                    if tail {
                        self.tail_expression(condition);
                    }
                    return tail;
                }
                self.statement(then_branch, tail)
                    && else_branch
                        .as_deref()
                        .is_none_or(|branch| self.statement(branch, tail))
            }
            StatementIr::Return(value) | StatementIr::Throw(value) => {
                self.tail_expression(value);
                true
            }
            StatementIr::Expression(value) if tail => {
                self.tail_expression(value);
                true
            }
            StatementIr::Expression(value) => self.test(value),
            StatementIr::Empty => true,
            _ => tail || never_completes_normally(statement),
        }
    }

    /// Roll back a rejected test so a sibling branch can keep scanning
    /// with the proof available when the branches diverged.
    fn test(&mut self, expression: &TypedExpr) -> bool {
        let checkpoint = (self.binding, self.sites.len());
        if self.guard(expression) {
            return true;
        }
        self.binding = checkpoint.0;
        self.sites.truncate(checkpoint.1);
        false
    }

    /// An arbitrary leaf is permitted once control cannot reach another
    /// participating site. Short-circuit and conditional arms retain their
    /// original evaluation and run only after an admitted prefix.
    fn tail_expression(&mut self, expression: &TypedExpr) {
        match &expression.expr {
            ExprIr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                if self.test(condition) {
                    self.tail_expression(then_expr);
                    self.tail_expression(else_expr);
                } else {
                    self.tail_expression(condition);
                }
            }
            ExprIr::LogicalShortCircuit { lhs, rhs, .. } | ExprIr::Comma { lhs, rhs } => {
                if self.test(lhs) {
                    self.tail_expression(rhs);
                } else {
                    self.tail_expression(lhs);
                }
            }
            _ => {
                self.test(expression);
            }
        }
    }

    fn guard(&mut self, expression: &TypedExpr) -> bool {
        match &expression.expr {
            ExprIr::InstanceOf { lhs, rhs } => {
                let ExprIr::Identifier(name) = &lhs.expr else {
                    return false;
                };
                let Some(binding) = self.builder.lookup_binding(name) else {
                    return false;
                };
                if GlobalReadOperand::of(rhs).is_none()
                    || self.binding.is_some_and(|previous| previous != binding)
                {
                    return false;
                }
                self.binding = Some(binding);
                self.sites.push(lhs.as_ref() as *const TypedExpr);
                true
            }
            ExprIr::Identifier(name) => self.builder.lookup_binding(name).is_some(),
            ExprIr::EnvironmentIdentifier(identifier) => {
                matches!(
                    identifier.resolution_start(),
                    scriptw_ir::EnvironmentIdentifierResolutionStart::Cell(_)
                ) && matches!(
                    identifier.operation,
                    scriptw_ir::EnvironmentIdentifierOperationIr::Read
                        | scriptw_ir::EnvironmentIdentifierOperationIr::Typeof
                )
            }
            ExprIr::Undefined
            | ExprIr::Null
            | ExprIr::Boolean(_)
            | ExprIr::Number(_)
            | ExprIr::String(_) => true,
            ExprIr::TypeOf { expr } | ExprIr::LogicalNot { expr } | ExprIr::Void { expr } => {
                self.guard(expr)
            }
            ExprIr::StrictEquality { lhs, rhs, .. }
            | ExprIr::LogicalShortCircuit { lhs, rhs, .. }
            | ExprIr::Comma { lhs, rhs } => self.guard(lhs) && self.guard(rhs),
            ExprIr::LooseEquality { lhs, rhs, .. } => {
                loose_guard_is_inert(lhs, rhs) && self.guard(lhs) && self.guard(rhs)
            }
            ExprIr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => self.guard(condition) && self.guard(then_expr) && self.guard(else_expr),
            ExprIr::SpecOperation {
                operation,
                operands,
            } => {
                let inert = match operation {
                    SpecOperationIr::IsLooselyEqual => {
                        matches!(operands.as_slice(), [lhs, rhs] if loose_guard_is_inert(lhs, rhs))
                    }
                    SpecOperationIr::StrictEqualityComparison
                    | SpecOperationIr::SameValue
                    | SpecOperationIr::SameValueZero
                    | SpecOperationIr::IsCallable
                    | SpecOperationIr::IsConstructor
                    | SpecOperationIr::IsPropertyKey
                    | SpecOperationIr::ToBoolean => true,
                    _ => false,
                };
                inert && operands.iter().all(|operand| self.guard(operand))
            }
            _ => false,
        }
    }
}

fn loose_guard_is_inert(lhs: &TypedExpr, rhs: &TypedExpr) -> bool {
    fn nullish(value: &TypedExpr) -> bool {
        matches!(
            value.expr,
            ExprIr::Null | ExprIr::Undefined | ExprIr::Void { .. }
        )
    }
    fn string(value: &TypedExpr) -> bool {
        matches!(value.expr, ExprIr::TypeOf { .. } | ExprIr::String(_))
    }
    // Nullish loose equality uses a tag/IsHTMLDDA test without coercion.
    nullish(lhs) || nullish(rhs) || (string(lhs) && string(rhs))
}

/// A `return` or `throw`, or a block that ends with one: every completion
/// of the statement is abrupt, so control never reaches the next statement
/// of the list through it.
fn never_completes_normally(statement: &StatementIr) -> bool {
    match statement {
        StatementIr::Return(_) | StatementIr::Throw(_) => true,
        StatementIr::Block(block) => block
            .statements
            .last()
            .is_some_and(never_completes_normally),
        _ => false,
    }
}

/// The owner is V, or undefined for an empty cache. The slots hold the
/// chain's first objects in order, with nulls after its end. A complete
/// chain reaches null or an object whose [[Prototype]] is immutably null.
pub(crate) struct WalkedChain {
    owner: ValueLocals,
    slots: [GcLocal<JavaScriptObject, Nullable>; CHAIN_SLOTS],
    complete: I32Local,
    shared_proof: I32Local,
}

impl WalkedChain {
    /// The next site must prove the shared facts again and walk its chain.
    pub(super) fn forget(&self, f: &mut Function) {
        self.owner.set_undefined(f);
        self.shared_proof.set_constant(0, f);
    }
}

/// The run whose statements are being compiled.
pub(crate) struct ActiveInstanceofRun {
    sites: Vec<*const TypedExpr>,
    chain: WalkedChain,
}

impl ActiveInstanceofRun {
    /// The role of the site whose left operand is `lhs`, when it is one of
    /// the run's: compared by node identity, so no other `instanceof`, a
    /// nested one in a site's S included, ever reads the slots.
    pub(super) fn site(&self, lhs: &TypedExpr) -> Option<RunSite<'_>> {
        let index = self
            .sites
            .iter()
            .position(|site| std::ptr::eq(*site, lhs))?;
        Some(RunSite {
            role: if index == 0 {
                RunRole::Walk
            } else {
                RunRole::Reuse
            },
            chain: &self.chain,
        })
    }
}

/// The first site starts with empty slots. Later sites check the owner before
/// deciding whether to reuse or fill them.
#[derive(Clone, Copy)]
enum RunRole {
    Walk,
    Reuse,
}

#[derive(Clone, Copy)]
pub(super) struct RunSite<'a> {
    role: RunRole,
    pub(super) chain: &'a WalkedChain,
}

/// What a usable record's answer hands to a run site: an Object-tagged V,
/// C's "prototype", the record, the walk's locals and its exits.
pub(super) struct ChainWalk<'a> {
    pub(super) object: &'a ValueLocals,
    pub(super) prototype: &'a GcLocal<JavaScriptObject, Nullable>,
    pub(super) record: &'a GcLocal<InstanceofSiteRecord, Nullable>,
    pub(super) next: &'a GcLocal<JavaScriptObject, Nullable>,
    pub(super) current: &'a GcLocal<JavaScriptObject, Nullable>,
    pub(super) answer: I32Local,
    pub(super) answered: ControlTarget,
    pub(super) slow: ControlTarget,
}

impl FunctionBuilder<'_> {
    /// Compiles an `instanceof` run at the start of `statements`, returning
    /// how many statements it took; none when they do not start one.
    pub(crate) fn try_compile_instanceof_run(
        &mut self,
        statements: &[StatementIr],
        f: &mut Function,
    ) -> Result<Option<usize>, EmitError> {
        let Some(run) = InstanceofRun::of_prefix(self, statements) else {
            return Ok(None);
        };
        let s = self.runtime_schema();
        let owner = s.reserve_value_local(f);
        owner.set_undefined(f);
        let slots = std::array::from_fn(|_| {
            s.reserve_gc_local::<JavaScriptObject, Nullable>(f)
                .initialize_null(s, f)
        });
        let complete = s.reserve_i32_local(f);
        complete.set_constant(0, f);
        let shared_proof = s.reserve_i32_local(f);
        shared_proof.set_constant(0, f);
        let previous = self.instanceof_run.replace(ActiveInstanceofRun {
            sites: run.sites,
            chain: WalkedChain {
                owner,
                slots,
                complete,
                shared_proof,
            },
        });
        let mut result = Ok(());
        for statement in run.statements {
            result = self.compile_statement(statement, f);
            if result.is_err() {
                break;
            }
        }
        if let Some(active) = std::mem::replace(&mut self.instanceof_run, previous) {
            let WalkedChain {
                owner,
                slots,
                complete,
                shared_proof,
            } = active.chain;
            for slot in slots {
                slot.clear(f);
            }
            s.release_i32_local(complete, f);
            s.release_i32_local(shared_proof, f);
            owner.clear(f);
        }
        result.map(|()| Some(run.statements.len()))
    }

    /// The source activation uses one Global Environment Record and one
    /// Agent-wide @@hasInstance throughout the run. Between fast answers
    /// no code can introduce a declarative name or hash that symbol.
    pub(super) fn emit_run_shared_proof_or_branch(
        &mut self,
        site: RunSite<'_>,
        global: &GcLocal<Environment, Nullable>,
        slow: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let proved = self.open_frame(ControlFrameKind::Block, f);
        if matches!(site.role, RunRole::Reuse) {
            site.chain.shared_proof.load(f);
            self.emit_branch_if_to_target(proved, f);
        }
        s.field(EnvironmentSchema::NAMED_BINDINGS)
            .read(global, s, f)
            .reference()
            .is_null(f);
        s.emit_well_known_symbol_never_hashed(scriptw_ir::WellKnownSymbol::HasInstance, f);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(slow, f);
        site.chain.shared_proof.set_constant(1, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
    }

    /// The first reached usable site fills the slots. Other sites reuse
    /// them for the same receiver, including after a conditional skip or
    /// a slow path that invalidated the earlier proof.
    pub(super) fn emit_run_chain_candidates(
        &mut self,
        site: RunSite<'_>,
        walk: ChainWalk<'_>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let chain = site.chain;
        if matches!(site.role, RunRole::Reuse) {
            chain.owner.reference().load(f);
            walk.object.reference().load(f);
            f.instruction(&Instruction::RefEq);
            f.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, f);
            f.rarely_reached();
        }
        self.emit_chain_prototype_or_branch(
            CandidateObject::Reference(walk.object.reference()),
            walk.next,
            walk.slow,
            f,
        );
        let (first, rest) = chain.slots.split_first().expect("the run keeps a slot");
        first.replace(walk.next.load(s, f), f);
        for slot in rest {
            slot.set_null(s, f);
        }
        let terminal = s.reserve_gc_local(f).initialize(
            s.field(InstanceofSiteRecordSchema::TERMINAL)
                .read(walk.record, s, f)
                .reference(),
            f,
        );
        chain.complete.set_constant(1, f);
        let filled = self.open_frame(ControlFrameKind::Block, f);
        for slot in rest {
            emit_chain_ends_i32(walk.next, &terminal, s, f);
            self.emit_branch_if_to_target(filled, f);
            walk.current.replace(walk.next.load(s, f), f);
            self.emit_chain_prototype_or_branch(
                CandidateObject::Prototype(walk.current),
                walk.next,
                walk.slow,
                f,
            );
            slot.replace(walk.next.load(s, f), f);
        }
        emit_chain_ends_i32(walk.next, &terminal, s, f);
        self.emit_branch_if_to_target(filled, f);
        chain.complete.set_constant(0, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        terminal.clear(f);
        chain.owner.copy_from(walk.object, f);
        if matches!(site.role, RunRole::Reuse) {
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        // Null slots never equal C's "prototype", an object.
        for (index, slot) in chain.slots.iter().enumerate() {
            let _ = slot.load(s, f);
            let _ = walk.prototype.load(s, f);
            f.instruction(&Instruction::RefEq);
            if index > 0 {
                f.instruction(&Instruction::I32Or);
            }
        }
        self.open_frame(ControlFrameKind::If, f);
        walk.answer.set_constant(1, f);
        self.emit_branch_to_target(walk.answered, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        chain.complete.load(f);
        self.emit_branch_if_to_target(walk.answered, f);
        let last = chain.slots.last().expect("the run keeps a slot");
        walk.next.replace(last.load(s, f), f);
    }
}
