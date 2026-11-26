//! Consecutive `instanceof` sites on one binding walk its chain once.
//!
//! An `instanceof` run is a prefix of a statement list in which every
//! statement is `if (x instanceof G) S`, with no else, the same declarative
//! binding x (a Wasm local or an environment cell: reading either runs no
//! code), a read of a global binding G, and an S that never completes
//! normally.
//! A later site is then reached only from the previous site's false answer:
//! S leaves the list, no other statement runs between the sites, and a
//! site's record path reads x, G's records and C's edges without observing
//! anything. The sites share the proof that their Global Environment
//! Record has no declarative names and that @@hasInstance has no hash.
//! The run's first site, once its record's proof holds, walks V's
//! [[Prototype]] chain into the run's slots; a later site, after its own
//! record's proof, answers from those slots when it meets that very V.
//! [[GetPrototypeOf]] of the ordinary, function and Array objects the walk
//! admits observes nothing and their edges change only through code that
//! no fast path runs. Every site's slow path invalidates both the shared
//! proof and the slots before it may run user code. The run starts with
//! neither on every entry to its statement list.
use super::*;
use crate::emit::BindingStorage;
use crate::gc_types::{Environment, EnvironmentSchema};

/// The prefix of V's chain the first site keeps: an ordinary object, a
/// class instance and a subclass instance end within it at %Object.prototype%.
const CHAIN_SLOTS: usize = 3;

/// A run of at least two `instanceof` statements.
struct InstanceofRun<'a> {
    statements: &'a [StatementIr],
    sites: Vec<*const TypedExpr>,
}

impl<'a> InstanceofRun<'a> {
    fn of_prefix(builder: &FunctionBuilder<'_>, statements: &'a [StatementIr]) -> Option<Self> {
        if !builder.has_source_execution_environment() {
            return None;
        }
        let mut local = None;
        let mut sites = Vec::new();
        for statement in statements {
            let Some((lhs, id)) = run_site(builder, statement) else {
                break;
            };
            if *local.get_or_insert(id) != id {
                break;
            }
            sites.push(lhs as *const TypedExpr);
        }
        (sites.len() >= 2).then(|| Self {
            statements: &statements[..sites.len()],
            sites,
        })
    }
}

/// `if (x instanceof G) S` with a declarative binding x, a global read G and
/// an S that never completes normally: x's operand node and binding. A site
/// answers from the slots only for the very V they were walked for, so x's
/// identity matters only in that its read runs no code.
fn run_site<'a>(
    builder: &FunctionBuilder<'_>,
    statement: &'a StatementIr,
) -> Option<(&'a TypedExpr, BindingStorage)> {
    let StatementIr::If {
        condition,
        then_branch,
        else_branch: None,
    } = statement
    else {
        return None;
    };
    let ExprIr::InstanceOf { lhs, rhs } = &condition.expr else {
        return None;
    };
    let ExprIr::Identifier(name) = &lhs.expr else {
        return None;
    };
    let id = builder.lookup_binding(name)?;
    GlobalReadOperand::of(rhs)?;
    never_completes_normally(then_branch).then_some((lhs, id))
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

/// V's chain as the run's first site walked it: OWNER is that V (undefined
/// while nothing is kept), the slots hold the chain's first objects in order
/// and null after its end, and COMPLETE says that the chain ends within them,
/// at null or at an object whose [[Prototype]] is immutably null.
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

/// The first site walks V's chain into the slots; every later site reuses
/// them for the same V.
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
/// C's "prototype", the record's TERMINAL, the walk's locals and its exits.
pub(super) struct ChainWalk<'a> {
    pub(super) object: &'a ValueLocals,
    pub(super) prototype: &'a GcLocal<JavaScriptObject, Nullable>,
    pub(super) terminal: &'a GcLocal<JavaScriptObject, Nullable>,
    pub(super) next: &'a GcLocal<JavaScriptObject, Nullable>,
    pub(super) current: &'a GcLocal<JavaScriptObject, Nullable>,
    pub(super) answer: I32Local,
    pub(super) answered: ControlTarget,
    pub(super) candidates: ControlTarget,
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

    /// The candidates of a run site's walk: the first site walks V's chain
    /// into the slots; a later site that meets the slots' V reads them, and
    /// one that meets another V walks as a lone site would. A slot holding
    /// C's "prototype" answers true, and a chain that ends within the slots
    /// without it answers false. Otherwise the walk goes on from the last
    /// slot. Every step only reads, and any other layout on the chain goes
    /// to the slow path, which forgets the slots.
    pub(super) fn emit_run_chain_candidates(
        &mut self,
        site: RunSite<'_>,
        walk: ChainWalk<'_>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let chain = site.chain;
        match site.role {
            RunRole::Walk => {
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
                chain.complete.set_constant(1, f);
                let filled = self.open_frame(ControlFrameKind::Block, f);
                for slot in rest {
                    emit_chain_ends_i32(walk.next, walk.terminal, s, f);
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
                emit_chain_ends_i32(walk.next, walk.terminal, s, f);
                self.emit_branch_if_to_target(filled, f);
                chain.complete.set_constant(0, f);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                chain.owner.copy_from(walk.object, f);
            }
            RunRole::Reuse => {
                chain.owner.reference().load(f);
                walk.object.reference().load(f);
                f.instruction(&Instruction::RefEq);
                f.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_chain_prototype_or_branch(
                    CandidateObject::Reference(walk.object.reference()),
                    walk.next,
                    walk.slow,
                    f,
                );
                self.emit_branch_to_target(walk.candidates, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
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
