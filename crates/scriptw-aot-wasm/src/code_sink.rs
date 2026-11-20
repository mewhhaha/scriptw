//! The only path from an emitter to a Wasm function body.
//!
//! Every raw structured-control instruction contributes to the real label
//! stack, including frames that the JS control-flow builder does not manage.
//! A branch target records both its stack position and its identity: a closed
//! frame must not become a valid target again when a sibling reuses its depth.
//!
//! These checks are unconditional, including in release builds used for
//! conformance runs. They validate emission structure, not Wasm operand types;
//! the Wasm validator remains responsible for the latter.

use std::sync::atomic::{AtomicU64, Ordering};

use wasm_encoder::{Catch, Instruction, ValType};

#[cfg(any(test, debug_assertions))]
mod gc_local_writes;

/// A live label's position and identity, not a relative branch immediate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LabelDepth {
    depth: u32,
    identity: u64,
}

impl LabelDepth {
    /// Synthetic positions for control-target tests, never emission handles.
    #[cfg(test)]
    pub(crate) const fn for_test(depth: u32) -> Self {
        Self { depth, identity: 0 }
    }
}

/// Constructed only after checking that the target label is still live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BranchDepth(u32);

impl BranchDepth {
    const fn immediate(self) -> u32 {
        self.0
    }
}

/// An open counted-call scope (see [`Function::open_counted_calls`]). Not
/// copyable: exactly one close consumes it.
#[must_use = "a counted-call scope must be closed"]
#[derive(Debug)]
pub(crate) struct CountedCalls {
    depth: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FrameKind {
    Function,
    Block,
    Loop,
    IfThen,
    IfElse,
    TryTable,
    LegacyTry,
    LegacyCatch,
    LegacyCatchAll,
}

/// The two-way branch that decides whether the code being emitted runs: the
/// place is reached exactly when that branch went this way, as far as the
/// sink can tell (it takes calls to return and loops to end).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Anchor {
    /// An arm of the `if` at this operator ordinal.
    Arm { opener: u32, alternative: bool },
    /// The fallthrough of the branch at this operator ordinal.
    Fallthrough(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Frame {
    identity: u64,
    kind: FrameKind,
    /// The ordinal of the operator that opened the frame.
    opener: u32,
    /// What decided that the frame's opener ran.
    entry_anchor: Option<Anchor>,
    /// Whether code inside the frame may leave it other than through its
    /// end: a branch to an outer label, a return, a throw.
    escaped: bool,
}

// Identities never enter the encoded module. Global allocation also rejects
// foreign-function handles and labels opened independently after cloning a
// partially emitted body. A clone intentionally retains its live prefix.
static NEXT_LABEL_IDENTITY: AtomicU64 = AtomicU64::new(1);

impl Frame {
    fn new(kind: FrameKind, opener: u32, entry_anchor: Option<Anchor>) -> Self {
        let identity = NEXT_LABEL_IDENTITY
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .expect("Wasm label identity space exhausted");
        Self {
            identity,
            kind,
            opener,
            entry_anchor,
            escaped: false,
        }
    }
}

/// What the sink knows of one i32 local at a program point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum I32Fact {
    /// The local holds this value: its last write stored this constant.
    Is(i32),
    /// The local does not hold this value: a checked escape left on it.
    IsNot(i32),
}

impl I32Fact {
    fn excludes(self, value: i32) -> bool {
        match self {
            Self::Is(known) => known != value,
            Self::IsNot(excluded) => excluded == value,
        }
    }

    /// What holds on both of two incoming edges.
    fn meet(self, other: Self) -> Option<Self> {
        match (self, other) {
            (Self::Is(a), Self::Is(b)) if a == b => Some(Self::Is(a)),
            (Self::IsNot(a), Self::IsNot(b)) if a == b => Some(Self::IsNot(a)),
            (Self::Is(known), Self::IsNot(excluded)) | (Self::IsNot(excluded), Self::Is(known))
                if known != excluded =>
            {
                Some(Self::IsNot(excluded))
            }
            _ => None,
        }
    }
}

/// The i32 local facts that hold on every path to a program point, through
/// the structured control flow the sink sees: a fact survives a join only
/// when every edge into it carries it. Only stores and tees change a local,
/// so calls keep every fact. A loop head and an exception handler entry
/// start without facts, since their incoming edges are not all known.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct I32Facts {
    facts: Vec<(u32, I32Fact)>,
}

impl I32Facts {
    /// Facts kept per program point; the oldest is forgotten beyond this.
    const LIMIT: usize = 32;

    fn get(&self, local: u32) -> Option<I32Fact> {
        self.facts
            .iter()
            .find(|(known, _)| *known == local)
            .map(|(_, fact)| *fact)
    }

    fn forget(&mut self, local: u32) {
        self.facts.retain(|(known, _)| *known != local);
    }

    fn set(&mut self, local: u32, fact: I32Fact) {
        self.forget(local);
        if self.facts.len() == Self::LIMIT {
            self.facts.remove(0);
        }
        self.facts.push((local, fact));
    }

    fn meet(&self, other: &Self) -> Self {
        Self {
            facts: self
                .facts
                .iter()
                .filter_map(|(local, fact)| {
                    other
                        .get(*local)
                        .and_then(|incoming| fact.meet(incoming))
                        .map(|fact| (*local, fact))
                })
                .collect(),
        }
    }
}

/// The facts at the current point, or none when no path reaches it.
type ReachingI32Facts = Option<I32Facts>;

fn meet_reaching(left: ReachingI32Facts, right: &ReachingI32Facts) -> ReachingI32Facts {
    match (left, right) {
        (None, right) => right.clone(),
        (left, None) => left,
        (Some(left), Some(right)) => Some(left.meet(right)),
    }
}

/// The facts of one open frame: those carried by the edges to its label so
/// far, and for an `if`, those at its opener for the arm that has not run.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct FrameI32Facts {
    label: ReachingI32Facts,
    other_arm: ReachingI32Facts,
}

/// The exact ordered local declaration consumed by one function body.
///
/// The body owns this declaration from construction onward. Typed allocation
/// appends exact scalar or reference types, and reuse accepts only the same
/// declared type; publication emits that complete declaration.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct LocalRun {
    end: u32,
    ty: ValType,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct LocalDeclarations {
    runs: Vec<LocalRun>,
}

#[cfg(test)]
#[path = "../tests/unit/local_declaration_index.rs"]
mod local_declaration_index_tests;

impl LocalDeclarations {
    pub(crate) fn from_types<L>(locals: L) -> Self
    where
        L: IntoIterator<Item = ValType>,
    {
        let mut result = Self { runs: Vec::new() };
        for ty in locals {
            result.push(ty);
        }
        result
    }

    /// Preserve deliberately ungrouped declarations used by decoder fixtures,
    /// including zero-length runs, without expanding their local count.
    #[cfg(test)]
    pub(crate) fn from_runs<L>(locals: L) -> Self
    where
        L: IntoIterator<Item = (u32, ValType)>,
    {
        let mut result = Self { runs: Vec::new() };
        for (count, ty) in locals {
            let end = result
                .count()
                .checked_add(count)
                .expect("Wasm local count exceeds u32");
            result.runs.push(LocalRun { end, ty });
        }
        result
    }

    fn count(&self) -> u32 {
        self.runs.last().map_or(0, |run| run.end)
    }

    fn push(&mut self, ty: ValType) {
        let end = self
            .count()
            .checked_add(1)
            .expect("Wasm local count exceeds u32");
        if let Some(last) = self.runs.last_mut() {
            if last.ty == ty {
                last.end = end;
                return;
            }
        }
        self.runs.push(LocalRun { end, ty });
    }

    fn type_at(&self, index: u32) -> Option<ValType> {
        self.runs
            .get(self.runs.partition_point(|run| run.end <= index))
            .map(|run| run.ty)
    }

    fn encode_body(&self) -> wasm_encoder::Function {
        wasm_encoder::Function::new(self.runs.iter().enumerate().map(|(index, run)| {
            let previous = index.checked_sub(1).map_or(0, |index| self.runs[index].end);
            (run.end - previous, run.ty)
        }))
    }

    /// Only cloning a complete body may fork its private declaration plan.
    fn fork_for_body(&self) -> Self {
        Self {
            runs: self.runs.clone(),
        }
    }
}

/// The index space an [`IndexOperand`] names.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum IndexSpace {
    Function,
    Global,
}

/// A function or global index immediate inside an encoded body.
///
/// Program import compaction renumbers both index spaces after every body is
/// emitted. The sink records each such immediate as it encodes it, so the
/// final body is these bytes with exactly these operands rewritten; no pass
/// has to decode the body's operators again to find them.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct IndexOperand {
    /// Byte offset of the operand's LEB128 encoding in the body.
    offset: u32,
    space: IndexSpace,
    index: u32,
}

impl IndexOperand {
    pub(crate) const fn offset(self) -> u32 {
        self.offset
    }
    pub(crate) const fn space(self) -> IndexSpace {
        self.space
    }
    pub(crate) const fn index(self) -> u32 {
        self.index
    }

    /// The function or global index an instruction carries. Each of these
    /// instructions encodes that index as its last immediate.
    fn of(instruction: &Instruction<'_>) -> Option<(IndexSpace, u32)> {
        match *instruction {
            Instruction::Call(index)
            | Instruction::ReturnCall(index)
            | Instruction::RefFunc(index) => Some((IndexSpace::Function, index)),
            Instruction::GlobalGet(index)
            | Instruction::GlobalSet(index)
            | Instruction::GlobalAtomicGet {
                global_index: index,
                ..
            }
            | Instruction::GlobalAtomicSet {
                global_index: index,
                ..
            }
            | Instruction::GlobalAtomicRmwAdd {
                global_index: index,
                ..
            }
            | Instruction::GlobalAtomicRmwSub {
                global_index: index,
                ..
            }
            | Instruction::GlobalAtomicRmwAnd {
                global_index: index,
                ..
            }
            | Instruction::GlobalAtomicRmwOr {
                global_index: index,
                ..
            }
            | Instruction::GlobalAtomicRmwXor {
                global_index: index,
                ..
            }
            | Instruction::GlobalAtomicRmwXchg {
                global_index: index,
                ..
            }
            | Instruction::GlobalAtomicRmwCmpxchg {
                global_index: index,
                ..
            } => Some((IndexSpace::Global, index)),
            _ => None,
        }
    }
}

fn leb128_len(value: u32) -> u32 {
    (32 - (value | 1).leading_zeros()).div_ceil(7)
}

/// Released locals awaiting reuse.
///
/// Local numbering reaches the emitted bytes, so the reuse order is the
/// contract: a reservation takes the earliest released local of its type in
/// `order`, and the most recently released local then moves into its place
/// (`Vec::swap_remove`). `positions` holds, per type, the positions in
/// `order` of that type's locals, so finding that local needs no scan;
/// `released` answers membership without one.
#[derive(Debug, Clone, Default)]
struct FreeLocals {
    order: Vec<(u32, ValType)>,
    positions: std::collections::BTreeMap<ValType, std::collections::BTreeSet<u32>>,
    released: Vec<bool>,
}

impl PartialEq for FreeLocals {
    fn eq(&self, other: &Self) -> bool {
        // `positions` and `released` are derived from `order`.
        self.order == other.order
    }
}
impl Eq for FreeLocals {}

impl FreeLocals {
    fn contains(&self, local: u32) -> bool {
        self.released.get(local as usize).copied().unwrap_or(false)
    }

    fn push(&mut self, local: u32, ty: ValType) {
        let position = u32::try_from(self.order.len()).expect("released local count exceeds u32");
        self.order.push((local, ty));
        self.positions.entry(ty).or_default().insert(position);
        let slot = local as usize;
        if self.released.len() <= slot {
            self.released.resize(slot + 1, false);
        }
        self.released[slot] = true;
    }

    fn take(&mut self, ty: ValType) -> Option<u32> {
        let position = self.positions.get_mut(&ty)?.pop_first()?;
        let (local, _) = self.order.swap_remove(position as usize);
        if let Some(&(_, moved)) = self.order.get(position as usize) {
            let last = u32::try_from(self.order.len()).expect("released local count exceeds u32");
            let moved = self
                .positions
                .get_mut(&moved)
                .expect("every released local's type has positions");
            moved.remove(&last);
            moved.insert(position);
        }
        self.released[local as usize] = false;
        Some(local)
    }
}

/// A function body and its open frames, including the implicit function label.
///
/// Re-exported as `Function` from the crate root so all emitters use the same
/// accounting. An empty frame stack means the final `end` has been emitted.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Function {
    body: wasm_encoder::Function,
    locals: LocalDeclarations,
    frames: Vec<Frame>,
    parameters: Vec<ValType>,
    initial_local_count: u32,
    initial_prefix_len: usize,
    free_locals: FreeLocals,
    operator_count: u32,
    literal_operands: Vec<crate::program_literal_operands::FunctionSite>,
    native_program_context: Option<crate::native_program_contexts::FunctionContext>,
    /// The i32 local facts at the current point.
    i32_facts: ReachingI32Facts,
    /// The facts of each open frame, parallel to `frames`.
    frame_i32_facts: Vec<FrameI32Facts>,
    /// The constant the previous instruction pushed, for the next store.
    pushed_i32_constant: Option<i32>,
    checked_escape_frames: Vec<LabelDepth>,
    /// Depth globals of the open counted-call scopes, innermost last.
    counted_calls: Vec<u32>,
    last_instruction_escapes: bool,
    /// Operator ordinal of each hinted branch, and whether it is likely taken.
    branch_hints: std::collections::BTreeMap<u32, bool>,
    /// Identities of the open labels that only rare paths branch to.
    rare_labels: Vec<u64>,
    /// Every function and global index immediate, in body order.
    index_operands: Vec<IndexOperand>,
    /// Operator ordinal and byte offset of every two-way branch, in body
    /// order: a hint may name a branch after it was emitted.
    two_way_branches: Vec<(u32, u32)>,
    /// The branch that decides whether the next instruction runs, if one
    /// branch does.
    anchor: Option<Anchor>,
    /// What the typed GC local write check needs of the body.
    #[cfg(any(test, debug_assertions))]
    gc_local_trace: gc_local_writes::Trace,
}

impl Clone for Function {
    fn clone(&self) -> Self {
        Self {
            body: self.body.clone(),
            locals: self.locals.fork_for_body(),
            frames: self.frames.clone(),
            parameters: self.parameters.clone(),
            initial_local_count: self.initial_local_count,
            initial_prefix_len: self.initial_prefix_len,
            free_locals: self.free_locals.clone(),
            operator_count: self.operator_count,
            literal_operands: self.literal_operands.clone(),
            native_program_context: self.native_program_context,
            i32_facts: self.i32_facts.clone(),
            frame_i32_facts: self.frame_i32_facts.clone(),
            pushed_i32_constant: self.pushed_i32_constant,
            checked_escape_frames: self.checked_escape_frames.clone(),
            counted_calls: self.counted_calls.clone(),
            last_instruction_escapes: self.last_instruction_escapes,
            branch_hints: self.branch_hints.clone(),
            rare_labels: self.rare_labels.clone(),
            index_operands: self.index_operands.clone(),
            two_way_branches: self.two_way_branches.clone(),
            anchor: self.anchor,
            #[cfg(any(test, debug_assertions))]
            gc_local_trace: self.gc_local_trace.clone(),
        }
    }
}

impl Function {
    pub(crate) fn new(locals: LocalDeclarations) -> Self {
        Self::with_parameters(locals, Vec::new())
    }

    pub(crate) fn with_parameters(locals: LocalDeclarations, parameters: Vec<ValType>) -> Self {
        let initial_local_count = locals.count();
        let body = locals.encode_body();
        let initial_prefix_len = body.byte_len();
        Self {
            body,
            locals,
            frames: vec![Frame::new(FrameKind::Function, 0, None)],
            parameters,
            initial_local_count,
            initial_prefix_len,
            free_locals: FreeLocals::default(),
            operator_count: 0,
            literal_operands: Vec::new(),
            native_program_context: None,
            i32_facts: Some(I32Facts::default()),
            frame_i32_facts: vec![FrameI32Facts::default()],
            pushed_i32_constant: None,
            checked_escape_frames: Vec::new(),
            counted_calls: Vec::new(),
            last_instruction_escapes: false,
            branch_hints: std::collections::BTreeMap::new(),
            rare_labels: Vec::new(),
            index_operands: Vec::new(),
            two_way_branches: Vec::new(),
            anchor: None,
            #[cfg(any(test, debug_assertions))]
            gc_local_trace: Default::default(),
        }
    }

    pub(crate) fn parameter_type(&self, index: u32) -> Option<ValType> {
        self.parameters.get(index as usize).copied()
    }
    pub(crate) fn parameter_types(&self) -> &[ValType] {
        &self.parameters
    }
    pub(crate) fn allocated_local_count(&self) -> u32 {
        self.locals.count()
    }

    /// Reports the caller's location to the GC local write check, which
    /// names the holder a failing read belongs to by it.
    #[track_caller]
    pub(crate) fn reserve_typed_local(&mut self, ty: ValType) -> u32 {
        assert!(
            !self.frames.is_empty(),
            "cannot reserve a local after function publication"
        );
        if let Some(index) = self.free_locals.take(ty) {
            self.forget_i32_local(index);
            #[cfg(any(test, debug_assertions))]
            self.gc_local_trace
                .reserve(index, ty, std::panic::Location::caller());
            return index;
        }
        self.reserve_fresh_typed_local(ty)
    }

    /// Append a fresh local declaration when no released slot can be reused.
    #[track_caller]
    pub(crate) fn reserve_fresh_typed_local(&mut self, ty: ValType) -> u32 {
        assert!(
            !self.frames.is_empty(),
            "cannot reserve a local after function publication"
        );
        let index = u32::try_from(self.parameters.len())
            .expect("parameter count overflow")
            .checked_add(self.locals.count())
            .expect("local index overflow");
        self.locals.push(ty);
        #[cfg(any(test, debug_assertions))]
        self.gc_local_trace
            .reserve(index, ty, std::panic::Location::caller());
        index
    }

    pub(crate) fn release_typed_local(&mut self, index: u32) {
        let base = u32::try_from(self.parameters.len()).expect("parameter count overflow");
        assert!(
            index >= base + self.initial_local_count && index < base + self.locals.count(),
            "only this body's dynamically reserved locals can be released"
        );
        assert!(!self.free_locals.contains(index), "local released twice");
        self.forget_i32_local(index);
        #[cfg(any(test, debug_assertions))]
        self.gc_local_trace.release(index);
        let ty = self
            .locals
            .type_at(index - base)
            .expect("a released local is declared");
        self.free_locals.push(index, ty);
    }

    fn forget_i32_local(&mut self, index: u32) {
        if let Some(facts) = &mut self.i32_facts {
            facts.forget(index);
        }
    }

    /// Joins `facts` into the edge to the label `immediate` frames out of
    /// a stack `depth` frames deep. A loop label is its head, whose facts
    /// were already given up when the loop opened.
    fn join_i32_facts_at(&mut self, depth: usize, immediate: u32, facts: &ReachingI32Facts) {
        let index = depth - 1 - immediate as usize;
        if self.frames[index].kind == FrameKind::Loop {
            return;
        }
        let label = self.frame_i32_facts[index].label.take();
        self.frame_i32_facts[index].label = meet_reaching(label, facts);
    }

    /// Updates the i32 local facts across an admitted `instruction`, after
    /// the frame stack changed for it: `enclosing_depth` frames were open
    /// before it, and `closing` is the kind of the frame it closed.
    fn flow_i32_facts(
        &mut self,
        instruction: &Instruction<'_>,
        enclosing_depth: usize,
        closing: Option<FrameKind>,
    ) {
        let pushed = self.pushed_i32_constant.take();
        match instruction {
            Instruction::I32Const(value) => self.pushed_i32_constant = Some(*value),
            Instruction::LocalSet(local) | Instruction::LocalTee(local) => {
                if let Some(facts) = &mut self.i32_facts {
                    facts.forget(*local);
                    if let Some(value) = pushed {
                        facts.set(*local, I32Fact::Is(value));
                    }
                }
            }
            Instruction::Block(_) => self.frame_i32_facts.push(FrameI32Facts::default()),
            Instruction::Loop(_) => {
                self.frame_i32_facts.push(FrameI32Facts::default());
                // Back edges are not emitted yet.
                if self.i32_facts.is_some() {
                    self.i32_facts = Some(I32Facts::default());
                }
            }
            Instruction::If(_) => self.frame_i32_facts.push(FrameI32Facts {
                label: None,
                other_arm: self.i32_facts.clone(),
            }),
            Instruction::TryTable(_, catches) => {
                // A handler can be entered from anywhere in the body.
                let anywhere = Some(I32Facts::default());
                for catch in catches.iter() {
                    let (Catch::One { label, .. }
                    | Catch::OneRef { label, .. }
                    | Catch::All { label }
                    | Catch::AllRef { label }) = catch;
                    self.join_i32_facts_at(enclosing_depth, *label, &anywhere);
                }
                self.frame_i32_facts.push(FrameI32Facts::default());
            }
            Instruction::Try(_) => self.frame_i32_facts.push(FrameI32Facts::default()),
            Instruction::Catch(_) | Instruction::CatchAll => {
                let current = self.i32_facts.take();
                let frame = self.frame_i32_facts.last_mut().expect("an open legacy try");
                let label = frame.label.take();
                frame.label = meet_reaching(label, &current);
                self.i32_facts = Some(I32Facts::default());
            }
            Instruction::Delegate(_) => {
                self.frame_i32_facts.pop();
                self.i32_facts = Some(I32Facts::default());
            }
            Instruction::Else => {
                let current = self.i32_facts.take();
                let frame = self.frame_i32_facts.last_mut().expect("an open if");
                let label = frame.label.take();
                frame.label = meet_reaching(label, &current);
                self.i32_facts = frame.other_arm.take();
            }
            Instruction::End => {
                let kind = closing.expect("an admitted end closed a frame");
                let frame = self.frame_i32_facts.pop().expect("facts of an open frame");
                let current = self.i32_facts.take();
                self.i32_facts = match kind {
                    // The label is the head; only the body's end falls out.
                    FrameKind::Loop | FrameKind::Function => current,
                    // The arm without code falls through from the opener.
                    FrameKind::IfThen => {
                        meet_reaching(meet_reaching(frame.label, &frame.other_arm), &current)
                    }
                    FrameKind::Block
                    | FrameKind::IfElse
                    | FrameKind::TryTable
                    | FrameKind::LegacyTry
                    | FrameKind::LegacyCatch
                    | FrameKind::LegacyCatchAll => meet_reaching(frame.label, &current),
                };
            }
            Instruction::Br(label) => {
                let current = self.i32_facts.take();
                self.join_i32_facts_at(enclosing_depth, *label, &current);
            }
            Instruction::BrIf(label)
            | Instruction::BrOnNull(label)
            | Instruction::BrOnNonNull(label)
            | Instruction::BrOnCast {
                relative_depth: label,
                ..
            }
            | Instruction::BrOnCastFail {
                relative_depth: label,
                ..
            }
            | Instruction::BrOnCastDescEq {
                relative_depth: label,
                ..
            }
            | Instruction::BrOnCastDescEqFail {
                relative_depth: label,
                ..
            } => {
                let current = self.i32_facts.clone();
                self.join_i32_facts_at(enclosing_depth, *label, &current);
            }
            Instruction::BrTable(labels, default) => {
                let current = self.i32_facts.take();
                self.join_i32_facts_at(enclosing_depth, *default, &current);
                for label in labels.iter() {
                    self.join_i32_facts_at(enclosing_depth, *label, &current);
                }
            }
            Instruction::Return
            | Instruction::ReturnCall(_)
            | Instruction::ReturnCallRef(_)
            | Instruction::ReturnCallIndirect { .. }
            | Instruction::Throw(_)
            | Instruction::ThrowRef
            | Instruction::Rethrow(_)
            | Instruction::Unreachable => self.i32_facts = None,
            Instruction::Suspend(_)
            | Instruction::Resume { .. }
            | Instruction::ResumeThrow { .. }
            | Instruction::ResumeThrowRef { .. }
            | Instruction::Switch { .. } => {
                // A resume table's handler labels would need the facts that
                // hold at every suspension, as `try_table` handlers get; no
                // emitter produces stack switching, so it is refused rather
                // than modelled.
                panic!("the i32 fact flow does not model stack switching")
            }
            // Every branching operator wasm-encoder 0.245 encodes is
            // matched above (the version is pinned exactly in Cargo.toml);
            // the rest fall through to the next operator.
            _ => {}
        }
    }

    /// Whether `local` is known not to hold `value` at the current point.
    fn excludes_i32_value(&self, local: u32, value: i32) -> bool {
        self.i32_facts
            .as_ref()
            .and_then(|facts| facts.get(local))
            .is_some_and(|fact| fact.excludes(value))
    }

    /// The matching arm must leave this guard on every path. The sink owns
    /// the comparison and rejects branches into its continuation; the final
    /// instruction must also escape. Only the unmatched arm can fall through.
    /// No guard is emitted where every path here already proves the local
    /// is not `value` (an earlier guard, or a stored other constant, with no
    /// store since on any path).
    pub(crate) fn if_i32_equal_escape(
        &mut self,
        local: u32,
        value: i32,
        escape: impl FnOnce(&mut Self),
    ) {
        let ty = self.parameter_type(local).or_else(|| {
            local
                .checked_sub(self.parameters.len() as u32)
                .and_then(|index| self.locals.type_at(index))
        });
        assert_eq!(
            ty,
            Some(ValType::I32),
            "checked guard requires an i32 local"
        );
        assert!(
            !self.free_locals.contains(local),
            "checked guard local is live"
        );
        if self.excludes_i32_value(local, value) {
            return;
        }
        self.instruction(&Instruction::LocalGet(local));
        self.instruction(&Instruction::I32Const(value));
        self.instruction(&Instruction::I32Eq);
        // The guarded value is the one the arm leaves on: an abrupt
        // completion, a failed admission.
        self.unlikely(&Instruction::If(wasm_encoder::BlockType::Empty));
        let guard = self.label_depth();
        self.checked_escape_frames.push(guard);
        escape(self);
        assert_eq!(
            self.label_depth(),
            guard,
            "checked escape must balance its frames"
        );
        assert_eq!(
            self.frames.last().unwrap().kind,
            FrameKind::IfThen,
            "checked escape cannot introduce a fallthrough else arm"
        );
        assert!(
            self.last_instruction_escapes,
            "checked arm must unconditionally escape"
        );
        assert_eq!(self.checked_escape_frames.pop(), Some(guard));
        self.instruction(&Instruction::End);
        if !self.free_locals.contains(local) {
            if let Some(facts) = &mut self.i32_facts {
                facts.set(local, I32Fact::IsNot(value));
            }
        }
    }

    fn depth(&self) -> u32 {
        u32::try_from(self.frames.len()).expect("Wasm label depth exceeds u32")
    }

    /// Opens a scope in which every call (`call`, `call_ref`,
    /// `call_indirect`) is bracketed by an increment and a decrement of the
    /// i32 global `depth`, so the global counts the scope's calls that are
    /// still executing. The bracket is the call itself: no branch, return or
    /// throw can come between the call and its decrement, so the count stays
    /// balanced however the surrounding code exits. Scopes nest; the
    /// innermost global counts.
    pub(crate) fn open_counted_calls(&mut self, depth: u32) -> CountedCalls {
        self.counted_calls.push(depth);
        CountedCalls { depth }
    }

    /// Closes the innermost counted-call scope.
    pub(crate) fn close_counted_calls(&mut self, scope: CountedCalls) {
        assert_eq!(
            self.counted_calls.pop(),
            Some(scope.depth),
            "counted-call scopes close innermost first"
        );
    }

    fn adjust_counted_calls(&mut self, depth: u32, delta: i32) {
        self.uncounted_instruction(&Instruction::GlobalGet(depth));
        self.uncounted_instruction(&Instruction::I32Const(delta));
        self.uncounted_instruction(&Instruction::I32Add);
        self.uncounted_instruction(&Instruction::GlobalSet(depth));
    }

    fn check_branch(&self, label: u32) {
        assert!(
            label < self.depth(),
            "branch immediate {label} is out of range at label depth {}",
            self.depth()
        );
        let target = self.frames[self.frames.len() - 1 - label as usize].identity;
        assert!(
            self.checked_escape_frames
                .iter()
                .all(|guard| guard.identity != target),
            "checked escape cannot branch to its own continuation"
        );
    }

    /// Account for an instruction before appending its bytes.
    pub(crate) fn instruction(&mut self, instruction: &Instruction<'_>) -> &mut Self {
        if let Some(&depth) = self.counted_calls.last() {
            if matches!(
                instruction,
                Instruction::Call(_) | Instruction::CallRef(_) | Instruction::CallIndirect { .. }
            ) {
                self.adjust_counted_calls(depth, 1);
                self.uncounted_instruction(instruction);
                self.adjust_counted_calls(depth, -1);
                return self;
            }
        }
        self.uncounted_instruction(instruction)
    }

    fn uncounted_instruction(&mut self, instruction: &Instruction<'_>) -> &mut Self {
        if self.frames.is_empty() {
            if matches!(instruction, Instruction::End) {
                panic!(
                    "wasm `end` with no open label: the emitter closed more frames than it opened"
                );
            }
            panic!("instruction emitted after the function body's final end");
        }

        if matches!(
            instruction,
            Instruction::Suspend(_)
                | Instruction::Resume { .. }
                | Instruction::ResumeThrow { .. }
                | Instruction::ResumeThrowRef { .. }
                | Instruction::Switch { .. }
        ) {
            assert!(
                self.checked_escape_frames.is_empty(),
                "stack switching cannot establish a checked escape fact"
            );
        }
        // Facts follow the instruction only once it is admitted, so a
        // rejected instruction changes nothing.
        let enclosing_depth = self.frames.len();
        let closing =
            matches!(instruction, Instruction::End | Instruction::Delegate(_)).then(|| {
                self.frames
                    .last()
                    .expect("an open frame was checked above")
                    .kind
            });

        match instruction {
            Instruction::Block(_) => self.frames.push(Frame::new(
                FrameKind::Block,
                self.operator_count,
                self.anchor,
            )),
            Instruction::Loop(_) => self.frames.push(Frame::new(
                FrameKind::Loop,
                self.operator_count,
                self.anchor,
            )),
            Instruction::If(_) => {
                self.frames.push(Frame::new(
                    FrameKind::IfThen,
                    self.operator_count,
                    self.anchor,
                ));
                self.anchor = Some(Anchor::Arm {
                    opener: self.operator_count,
                    alternative: false,
                });
            }
            Instruction::TryTable(_, catches) => {
                // Handler labels name enclosing frames, not the new try_table
                // frame. Check EVERY clause before changing either the frame
                // stack or encoded bytes, including clauses after catch_all.
                for catch in catches.iter() {
                    let label = match catch {
                        Catch::One { label, .. }
                        | Catch::OneRef { label, .. }
                        | Catch::All { label }
                        | Catch::AllRef { label } => *label,
                    };
                    self.check_branch(label);
                }
                self.frames.push(Frame::new(
                    FrameKind::TryTable,
                    self.operator_count,
                    self.anchor,
                ));
            }
            Instruction::Try(_) => self.frames.push(Frame::new(
                FrameKind::LegacyTry,
                self.operator_count,
                self.anchor,
            )),
            Instruction::Catch(_) | Instruction::CatchAll => {
                let frame = self
                    .frames
                    .last_mut()
                    .expect("an open frame was checked above");
                assert!(
                    matches!(frame.kind, FrameKind::LegacyTry | FrameKind::LegacyCatch),
                    "wasm `catch`/`catch_all` must belong to an open legacy try before catch_all"
                );
                // A handler replaces the arm, not its label identity. Multiple
                // tagged catches are legal; catch_all is necessarily last.
                frame.kind = if matches!(instruction, Instruction::CatchAll) {
                    FrameKind::LegacyCatchAll
                } else {
                    FrameKind::LegacyCatch
                };
                self.anchor = None;
            }
            Instruction::Delegate(label) => {
                assert_eq!(
                    self.frames
                        .last()
                        .expect("an open frame was checked above")
                        .kind,
                    FrameKind::LegacyTry,
                    "wasm `delegate` must terminate a legacy try body before any catch"
                );
                // Delegate substitutes for end. Its immediate is relative to
                // the enclosing stack AFTER removing this try. Ordinary block
                // and function labels are legal delegation destinations too.
                let enclosing_depth = self.depth() - 1;
                assert!(
                    *label < enclosing_depth,
                    "delegate immediate {label} is out of range at enclosing label depth {enclosing_depth}"
                );
                self.check_branch(*label + 1);
                self.frames.pop();
                self.anchor = None;
            }
            Instruction::Rethrow(label) => {
                self.check_branch(*label);
                let frame = &self.frames[self.frames.len() - 1 - *label as usize];
                assert!(
                    matches!(
                        frame.kind,
                        FrameKind::LegacyCatch | FrameKind::LegacyCatchAll
                    ),
                    "wasm `rethrow` must target a live legacy catch handler"
                );
                for frame in &mut self.frames {
                    frame.escaped = true;
                }
            }
            Instruction::End => {
                if let Some(closed) = self.frames.pop() {
                    self.rare_labels.retain(|rare| *rare != closed.identity);
                    // Whatever entered a frame that nothing leaves early
                    // reaches the code after it.
                    self.anchor = closed.entry_anchor.filter(|_| !closed.escaped);
                }
            }
            Instruction::Else => {
                let frame = self
                    .frames
                    .last_mut()
                    .expect("an open frame was checked above");
                assert_eq!(
                    frame.kind,
                    FrameKind::IfThen,
                    "wasm `else` must belong to an unmatched `if`"
                );
                // Both arms share a label; only its structural state changes.
                frame.kind = FrameKind::IfElse;
                self.anchor = Some(Anchor::Arm {
                    opener: frame.opener,
                    alternative: true,
                });
            }
            Instruction::Br(label) => {
                self.check_branch(*label);
                if self.branches_rarely_to_immediate(*label) {
                    // Only a rare path comes to this branch.
                    self.rarely_reached();
                }
                self.leaves_frames_inside(*label);
            }
            Instruction::BrIf(label)
            | Instruction::BrOnNull(label)
            | Instruction::BrOnNonNull(label) => {
                self.check_branch(*label);
                self.two_way_branch(*label);
            }
            Instruction::BrTable(labels, default) => {
                self.check_branch(*default);
                self.leaves_frames_inside(*default);
                for label in labels.iter() {
                    self.check_branch(*label);
                    self.leaves_frames_inside(*label);
                }
                self.anchor = None;
            }
            Instruction::BrOnCast { relative_depth, .. }
            | Instruction::BrOnCastFail { relative_depth, .. }
            | Instruction::BrOnCastDescEq { relative_depth, .. }
            | Instruction::BrOnCastDescEqFail { relative_depth, .. } => {
                self.check_branch(*relative_depth);
                self.two_way_branch(*relative_depth);
            }
            Instruction::Return
            | Instruction::ReturnCall(_)
            | Instruction::ReturnCallRef(_)
            | Instruction::ReturnCallIndirect { .. }
            | Instruction::Throw(_)
            | Instruction::ThrowRef => {
                for frame in &mut self.frames {
                    frame.escaped = true;
                }
            }
            _ => {}
        }
        self.flow_i32_facts(instruction, enclosing_depth, closing);
        #[cfg(any(test, debug_assertions))]
        self.gc_local_trace.instruction(instruction);
        self.last_instruction_escapes = matches!(
            instruction,
            Instruction::Br(_)
                | Instruction::Return
                | Instruction::ReturnCall(_)
                | Instruction::ReturnCallRef(_)
                | Instruction::ReturnCallIndirect { .. }
                | Instruction::Throw(_)
                | Instruction::ThrowRef
                | Instruction::Rethrow(_)
                | Instruction::Unreachable
        );
        if is_two_way_branch(instruction) {
            self.two_way_branches.push((
                self.operator_count,
                u32::try_from(self.body.byte_len()).expect("Wasm body exceeds u32 bytes"),
            ));
        }
        self.body.instruction(instruction);
        if let Some((space, index)) = IndexOperand::of(instruction) {
            let end = u32::try_from(self.body.byte_len()).expect("Wasm body exceeds u32 bytes");
            self.index_operands.push(IndexOperand {
                offset: end - leb128_len(index),
                space,
                index,
            });
        }
        self.operator_count = self
            .operator_count
            .checked_add(1)
            .expect("operator count overflow");
        self
    }

    /// Emit the conditional branch `instruction` and say that it is rarely
    /// taken: an `if` rarely runs its consequent, a `br_if` or `br_on_*`
    /// rarely leaves for its label. The backend lays out what only such
    /// branches reach behind the function's other code.
    ///
    /// A hint always names an instruction this sink emitted as a two-way
    /// branch: it is recorded here with the instruction, or later against
    /// the ordinal the sink kept for it ([`Function::rarely_reached`]).
    pub(crate) fn unlikely(&mut self, instruction: &Instruction<'_>) -> &mut Self {
        self.hinted(false, instruction)
    }

    /// Emit the conditional branch `instruction` and say that it is nearly
    /// always taken: an `if` rarely skips its consequent (or runs its
    /// `else`), a `br_if` or `br_on_*` rarely falls through.
    pub(crate) fn likely(&mut self, instruction: &Instruction<'_>) -> &mut Self {
        self.hinted(true, instruction)
    }

    fn hinted(&mut self, likely: bool, instruction: &Instruction<'_>) -> &mut Self {
        assert!(
            is_two_way_branch(instruction),
            "only a two-way branch takes a hint"
        );
        self.branch_hints.insert(self.operator_count, likely);
        self.instruction(instruction)
    }

    /// Say that the code about to be emitted runs rarely: a throw, a fault,
    /// a fallback. When one two-way branch decides whether control comes
    /// here, that branch is hinted to rarely go this way, unless its emitter
    /// already hinted it: the `if` whose arm leads here without a way out
    /// before this place, or the branch that falls through to it. Where
    /// several paths meet, or some leave before this place, nothing is
    /// known of any one branch and nothing is recorded.
    pub(crate) fn rarely_reached(&mut self) {
        match self.anchor {
            Some(Anchor::Arm {
                opener,
                alternative,
            }) => {
                self.branch_hints.entry(opener).or_insert(alternative);
            }
            Some(Anchor::Fallthrough(branch)) => {
                self.branch_hints.entry(branch).or_insert(true);
            }
            None => {}
        }
    }

    /// Say that only rare paths branch to `label`: a miss, a failed proof, a
    /// fallback. Every two-way branch to it emitted from here on is hinted
    /// unlikely taken, unless its emitter hints it itself; an `if` around an
    /// unconditional branch to it asks [`Function::branches_rarely_to`].
    pub(crate) fn rarely_taken(&mut self, label: LabelDepth) {
        // Checks that the label is open in this body.
        let _ = self.branch_depth_to(label);
        if !self.rare_labels.contains(&label.identity) {
            self.rare_labels.push(label.identity);
        }
    }

    /// Whether `label` was declared [`Function::rarely_taken`].
    pub(crate) fn branches_rarely_to(&self, label: LabelDepth) -> bool {
        self.rare_labels.contains(&label.identity)
    }

    fn branches_rarely_to_immediate(&self, immediate: u32) -> bool {
        !self.rare_labels.is_empty()
            && self
                .rare_labels
                .contains(&self.frames[self.frames.len() - 1 - immediate as usize].identity)
    }

    /// The frames strictly inside the target of a branch are left by it.
    fn leaves_frames_inside(&mut self, immediate: u32) {
        let inside = self.frames.len() - immediate as usize;
        for frame in &mut self.frames[inside..] {
            frame.escaped = true;
        }
    }

    /// Called with the operator count still that of the two-way branch
    /// being emitted.
    fn two_way_branch(&mut self, immediate: u32) {
        if self.branches_rarely_to_immediate(immediate) {
            self.branch_hints
                .entry(self.operator_count)
                .or_insert(false);
        }
        self.leaves_frames_inside(immediate);
        self.anchor = Some(Anchor::Fallthrough(self.operator_count));
    }

    /// The hinted branches of the body so far: operator ordinal and whether
    /// it is likely taken, in operator order.
    #[cfg(test)]
    pub(crate) fn hinted_operators(&self) -> Vec<(u32, bool)> {
        self.branch_hints
            .iter()
            .map(|(operator, likely)| (*operator, *likely))
            .collect()
    }

    pub(crate) fn program_literal_count(&mut self, value: crate::gc_types::ProgramLiteralCount) {
        self.literal_operands
            .push(crate::program_literal_operands::FunctionSite::count(
                self.operator_count,
                value,
            ));
        self.instruction(&Instruction::I32Const(value.value() as i32));
        // The immediate is relocated when the program is linked, so it is no
        // constant a following store could make a fact of.
        self.pushed_i32_constant = None;
    }

    pub(crate) fn program_literal_index(&mut self, value: crate::gc_types::ProgramLiteralIndex) {
        self.literal_operands
            .push(crate::program_literal_operands::FunctionSite::index(
                self.operator_count,
                value,
            ));
        self.instruction(&Instruction::I32Const(value.ordinal() as i32));
        // Relocated at link time, like the count above.
        self.pushed_i32_constant = None;
    }

    pub(crate) fn literal_operands(&self) -> &[crate::program_literal_operands::FunctionSite] {
        &self.literal_operands
    }

    pub(crate) fn native_program_context(
        &self,
    ) -> Option<crate::native_program_contexts::FunctionContext> {
        self.native_program_context
    }

    pub(crate) fn bind_native_program_context(
        &mut self,
        schema: crate::native_program_contexts::Schema,
        local: u32,
    ) {
        assert!(
            self.native_program_context.is_none(),
            "one effective program binding per body"
        );
        self.native_program_context =
            Some(crate::native_program_contexts::FunctionContext::Program {
                schema,
                local,
                ready: self.operator_count,
            });
    }

    pub(crate) fn bind_native_bootstrap_context(
        &mut self,
        schema: crate::native_program_contexts::Schema,
    ) {
        assert!(self.native_program_context.is_none());
        self.native_program_context =
            Some(crate::native_program_contexts::FunctionContext::Bootstrap {
                schema,
                payload_operator: None,
            });
    }

    pub(crate) fn native_program_payload_slot(&mut self) {
        if self.native_program_context.is_none() {
            return;
        }
        let Some(crate::native_program_contexts::FunctionContext::Bootstrap {
            payload_operator,
            ..
        }) = &mut self.native_program_context
        else {
            panic!("source payload construction belongs to the explicit program bootstrap");
        };
        assert!(
            payload_operator.is_none(),
            "one original source payload slot per bootstrap"
        );
        *payload_operator = Some(self.operator_count);
    }

    /// Capture immediately after opening the frame that will be targeted.
    pub(crate) fn label_depth(&self) -> LabelDepth {
        let frame = self
            .frames
            .last()
            .expect("a finished body has no live label");
        LabelDepth {
            depth: self.depth(),
            identity: frame.identity,
        }
    }

    /// Resolve a live target in this body to a relative branch immediate.
    ///
    /// Testing depth alone misses a closed block followed by a sibling block
    /// at the same depth. Identity must be checked at the recorded position.
    pub(crate) fn branch_depth_to(&self, label: LabelDepth) -> BranchDepth {
        let frame = label
            .depth
            .checked_sub(1)
            .and_then(|index| self.frames.get(index as usize));
        assert!(
            frame.is_some_and(|frame| frame.identity == label.identity),
            "branch target label is not open at this point: its frame was closed or belongs to another body"
        );
        BranchDepth(self.depth() - label.depth)
    }

    pub(crate) fn branch_to_label(&mut self, label: LabelDepth) {
        let depth = self.branch_depth_to(label);
        self.instruction(&Instruction::Br(depth.immediate()));
    }

    pub(crate) fn branch_if_to_label(&mut self, label: LabelDepth) {
        let depth = self.branch_depth_to(label);
        self.instruction(&Instruction::BrIf(depth.immediate()));
    }

    /// Branches to `label` with the operand, of type `from`, unless it is a
    /// `to`; otherwise leaves it on the stack as a `to`.
    pub(crate) fn branch_on_cast_fail_to_label(
        &mut self,
        label: LabelDepth,
        from: wasm_encoder::RefType,
        to: wasm_encoder::RefType,
    ) {
        let depth = self.branch_depth_to(label);
        self.instruction(&Instruction::BrOnCastFail {
            relative_depth: depth.immediate(),
            from_ref_type: from,
            to_ref_type: to,
        });
    }

    /// Pops a null reference and branches to `label`; leaves any other
    /// reference on the stack, refined to non-null.
    pub(crate) fn branch_on_null_to_label(&mut self, label: LabelDepth) {
        let depth = self.branch_depth_to(label);
        self.instruction(&Instruction::BrOnNull(depth.immediate()));
    }

    pub(crate) fn branch_table_to_labels(
        &mut self,
        labels: impl IntoIterator<Item = LabelDepth>,
        default: LabelDepth,
    ) {
        let depths = labels
            .into_iter()
            .map(|label| self.branch_depth_to(label).immediate())
            .collect::<Vec<_>>();
        let default = self.branch_depth_to(default).immediate();
        self.instruction(&Instruction::BrTable(depths.into(), default));
    }

    #[cfg(test)]
    pub(crate) fn byte_len(&self) -> usize {
        self.body.byte_len()
    }

    /// The finished body with its index operands and hinted branches, at
    /// their offsets in it.
    pub(crate) fn finish_named(self, context: &dyn core::fmt::Display) -> FinishedBody {
        assert!(
            self.frames.is_empty(),
            "function body for {context} has an unclosed control frame: {} label(s) still open",
            self.frames.len()
        );
        #[cfg(any(test, debug_assertions))]
        self.gc_local_trace.verify(context);
        let raw = self.body.into_raw_body();
        let mut body = self.locals.encode_body();
        // Locals declared after construction lengthen the declaration prefix.
        let prefix = u32::try_from(body.byte_len()).expect("Wasm body exceeds u32 bytes");
        let initial = u32::try_from(self.initial_prefix_len).expect("Wasm body exceeds u32 bytes");
        body.raw(raw[self.initial_prefix_len..].iter().copied());
        let mut index_operands = self.index_operands;
        for operand in &mut index_operands {
            operand.offset = operand.offset - initial + prefix;
        }
        let branches = self.two_way_branches;
        let branch_hints = self
            .branch_hints
            .iter()
            .map(|(operator, likely)| {
                let at = branches
                    .binary_search_by_key(operator, |(ordinal, _)| *ordinal)
                    .unwrap_or_else(|_| {
                        panic!("a hint in {context} names operator {operator}, which is no two-way branch")
                    });
                crate::branch_hints::FunctionSite::new(branches[at].1 - initial + prefix, *likely)
            })
            .collect();
        FinishedBody {
            body,
            index_operands,
            branch_hints,
        }
    }

    #[cfg(test)]
    pub(crate) fn into_body(self) -> wasm_encoder::Function {
        self.finish_named(&"an unnamed test body").body
    }
}

/// A closed body and what the module needs to know of its bytes.
pub(crate) struct FinishedBody {
    pub(crate) body: wasm_encoder::Function,
    /// Every function and global index immediate, at its offset in `body`.
    pub(crate) index_operands: Vec<IndexOperand>,
    /// Every hinted branch, at its offset in `body`, in body order.
    pub(crate) branch_hints: Vec<crate::branch_hints::FunctionSite>,
}

/// The branches whose direction a branch hint can describe.
fn is_two_way_branch(instruction: &Instruction<'_>) -> bool {
    matches!(
        instruction,
        Instruction::If(_)
            | Instruction::BrIf(_)
            | Instruction::BrOnNull(_)
            | Instruction::BrOnNonNull(_)
            | Instruction::BrOnCast { .. }
            | Instruction::BrOnCastFail { .. }
            | Instruction::BrOnCastDescEq { .. }
            | Instruction::BrOnCastDescEqFail { .. }
    )
}

// The complete declaration enters the same owner as its emitted body.
const _: fn(LocalDeclarations) -> Function = Function::new;

// Standalone Wasm fixtures live outside the product module-assembly boundary.
#[cfg(test)]
#[path = "../tests/unit/code_sink.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/code_sink_exceptions.rs"]
mod exception_tests;

#[cfg(test)]
#[path = "../tests/unit/gc_local_types.rs"]
mod gc_local_type_tests;

#[cfg(test)]
#[path = "../tests/unit/throw_guard_fact.rs"]
mod throw_guard_fact_tests;
