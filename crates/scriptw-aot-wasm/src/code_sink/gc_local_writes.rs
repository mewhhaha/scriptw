//! A retained check that every holder of a pooled typed GC local writes it
//! before reading it.
//!
//! A released typed GC local is not cleared: storing null would retain
//! nothing less (Wasmtime finds stack roots through Cranelift stack maps, by
//! SSA liveness), so the release only returns the local to the pool. The next
//! holder of that local must therefore write it before it reads it, on every
//! path; a read that some path reaches without such a write would see the
//! previous holder's object, of the same Wasm type, and carry on with it.
//! Wasm validation does not catch this: a reference local is defaultable when
//! nullable, and a non-nullable one only needs *some* earlier set in an
//! enclosing block, whichever holder made it.
//!
//! The sink records the control structure, the reservations, releases, reads
//! and writes of every local declared with a concrete reference type, and the
//! operations that can throw into a handler. [`Trace::verify`] then runs a
//! must-be-written dataflow over that record when the body is finished:
//! joins intersect, and each `try_table` handler label receives the state at
//! every call and throw inside its body. A loop head holds what its entry
//! holds less every local the loop reserves or releases anywhere: a write
//! that survives the back edge was then made by a holder the body reserved,
//! not by one that held the local at the head, so the analysis needs one pass
//! and attributes a read at the head to the right holder. A read of a local
//! that is not written on every path from its reservation panics with the
//! function and the operator.
//!
//! The record exists in test and debug builds only.

use wasm_encoder::{Catch, HeapType, Instruction, ValType};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Op {
    Block,
    Loop,
    If,
    Else,
    End,
    /// The catch clauses' label immediates, and whether one catches all.
    TryTable(Vec<u32>, bool),
    Br(u32),
    /// A branch that may fall through: `br_if` and the `br_on_*` forms.
    BrIf(u32),
    BrTable(Vec<u32>, u32),
    /// Control does not reach the next operator.
    Terminate,
    /// A call or throw: it can enter an enclosing `try_table` handler.
    MayThrow,
    /// A checked local, by its dense index.
    Read(usize),
    Write(usize),
    /// A new holder, and where the emitter reserved it.
    Reserve(usize, &'static std::panic::Location<'static>),
    Release(usize),
    /// An operator this check does not model (legacy exception handling,
    /// stack switching); the sink never emits one in a body it checks.
    Unmodelled(&'static str),
}

/// Whether locals of `ty` are checked: references to a concrete type, the
/// typed GC locals whose release does not clear them. Abstract references
/// (the value pool's `eqref`, which keeps its clear) are not.
pub(super) fn is_checked_type(ty: ValType) -> bool {
    matches!(
        ty,
        ValType::Ref(wasm_encoder::RefType {
            heap_type: HeapType::Concrete(_),
            ..
        })
    )
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Trace {
    ops: Vec<Op>,
    /// Dense index of each checked local, by Wasm local index.
    checked: std::collections::BTreeMap<u32, usize>,
}

impl Trace {
    pub(super) fn reserve(
        &mut self,
        local: u32,
        ty: ValType,
        at: &'static std::panic::Location<'static>,
    ) {
        if is_checked_type(ty) {
            let next = self.checked.len();
            let dense = *self.checked.entry(local).or_insert(next);
            self.ops.push(Op::Reserve(dense, at));
        }
    }

    pub(super) fn release(&mut self, local: u32) {
        if let Some(&dense) = self.checked.get(&local) {
            self.ops.push(Op::Release(dense));
        }
    }

    pub(super) fn instruction(&mut self, instruction: &Instruction<'_>) {
        let op = match instruction {
            Instruction::Block(_) => Op::Block,
            Instruction::Loop(_) => Op::Loop,
            Instruction::If(_) => Op::If,
            Instruction::Else => Op::Else,
            Instruction::End => Op::End,
            Instruction::TryTable(_, catches) => Op::TryTable(
                catches
                    .iter()
                    .map(|catch| match catch {
                        Catch::One { label, .. }
                        | Catch::OneRef { label, .. }
                        | Catch::All { label }
                        | Catch::AllRef { label } => *label,
                    })
                    .collect(),
                catches
                    .iter()
                    .any(|catch| matches!(catch, Catch::All { .. } | Catch::AllRef { .. })),
            ),
            Instruction::Br(label) => Op::Br(*label),
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
            } => Op::BrIf(*label),
            Instruction::BrTable(labels, default) => Op::BrTable(labels.to_vec(), *default),
            Instruction::Return
            | Instruction::ReturnCall(_)
            | Instruction::ReturnCallRef(_)
            | Instruction::ReturnCallIndirect { .. }
            | Instruction::Unreachable => Op::Terminate,
            Instruction::Throw(_) | Instruction::ThrowRef => {
                self.ops.push(Op::MayThrow);
                Op::Terminate
            }
            Instruction::Call(_) | Instruction::CallRef(_) | Instruction::CallIndirect { .. } => {
                Op::MayThrow
            }
            Instruction::LocalGet(local) => match self.checked.get(local) {
                Some(&dense) => Op::Read(dense),
                None => return,
            },
            Instruction::LocalSet(local) | Instruction::LocalTee(local) => {
                match self.checked.get(local) {
                    Some(&dense) => Op::Write(dense),
                    None => return,
                }
            }
            Instruction::Try(_) => Op::Unmodelled("legacy try"),
            Instruction::Catch(_) | Instruction::CatchAll => Op::Unmodelled("legacy catch"),
            Instruction::Delegate(_) => Op::Unmodelled("legacy delegate"),
            Instruction::Rethrow(_) => Op::Unmodelled("legacy rethrow"),
            Instruction::Suspend(_)
            | Instruction::Resume { .. }
            | Instruction::ResumeThrow { .. }
            | Instruction::ResumeThrowRef { .. }
            | Instruction::Switch { .. } => Op::Unmodelled("stack switching"),
            _ => return,
        };
        self.ops.push(op);
    }

    /// Panics, naming `context`, at the first read some path reaches
    /// without a write by the local's current holder.
    pub(super) fn verify(&self, context: &dyn core::fmt::Display) {
        if self.checked.is_empty() {
            return;
        }
        if let Some(Op::Unmodelled(what)) =
            self.ops.iter().find(|op| matches!(op, Op::Unmodelled(_)))
        {
            panic!("{context}: the GC local write check does not model {what}");
        }
        let violations = Flow::new(self).run();
        if !violations.is_empty() {
            let reads = violations
                .iter()
                .map(|&(operator, dense)| {
                    let local = self
                        .checked
                        .iter()
                        .find_map(|(local, index)| (*index == dense).then_some(*local))
                        .expect("a dense index names a checked local");
                    let holder = self.ops[..operator]
                        .iter()
                        .rev()
                        .find_map(|op| match op {
                            Op::Reserve(reserved, at) if *reserved == dense => Some(*at),
                            _ => None,
                        })
                        .map_or_else(|| "no reservation".to_owned(), ToString::to_string);
                    format!("local {local} (holder reserved at {holder})")
                })
                .collect::<Vec<_>>();
            panic!(
                "{context}: a typed GC local is read on a path where its current holder has \
                 not written it, so it would read a previous holder's reference: {}",
                reads.join("; ")
            );
        }
    }
}

/// The checked locals written since their reservation; `None` where no path
/// reaches.
type State = Option<Vec<u64>>;

fn meet(into: &mut State, incoming: &State) {
    match (into.as_mut(), incoming) {
        (_, None) => {}
        (None, Some(incoming)) => *into = Some(incoming.clone()),
        (Some(into), Some(incoming)) => {
            for (word, other) in into.iter_mut().zip(incoming) {
                *word &= other;
            }
        }
    }
}

fn bit(dense: usize) -> (usize, u64) {
    (dense / 64, 1 << (dense % 64))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Function,
    Block,
    Loop,
    If,
    Else,
    TryTable,
}

#[derive(Debug)]
struct Frame {
    kind: FrameKind,
    /// Edges to the label's end; a loop's label is its head, which takes
    /// its state from the loop's kill set instead.
    label: State,
    /// An `if`: the state at its opener, for the arm not yet run.
    other_arm: State,
    /// A `try_table`: the frame indices its catch clauses branch to, and
    /// whether one of them catches everything.
    handlers: Vec<usize>,
    catches_all: bool,
}

impl Frame {
    fn new(kind: FrameKind) -> Self {
        Self {
            kind,
            label: None,
            other_arm: None,
            handlers: Vec::new(),
            catches_all: false,
        }
    }
}

struct Flow<'a> {
    trace: &'a Trace,
    frames: Vec<Frame>,
    current: State,
    /// The locals each loop reserves or releases, by its opener's index.
    kills: std::collections::BTreeMap<usize, Vec<u64>>,
    violations: std::collections::BTreeSet<(usize, usize)>,
}

impl<'a> Flow<'a> {
    fn new(trace: &'a Trace) -> Self {
        let words = trace.checked.len().div_ceil(64);
        // Each loop's kill set: the reservations and releases anywhere in
        // its body, nested loops included.
        let mut kills = std::collections::BTreeMap::new();
        let mut open: Vec<Option<(usize, Vec<u64>)>> = Vec::new();
        for (index, op) in trace.ops.iter().enumerate() {
            match op {
                Op::Loop => open.push(Some((index, vec![0; words]))),
                Op::Block | Op::If | Op::TryTable(..) => open.push(None),
                Op::End => {
                    if let Some(Some((opener, killed))) = open.pop() {
                        if let Some((_, outer)) =
                            open.iter_mut().rev().find_map(|frame| frame.as_mut())
                        {
                            for (word, inner) in outer.iter_mut().zip(&killed) {
                                *word |= inner;
                            }
                        }
                        kills.insert(opener, killed);
                    }
                }
                Op::Reserve(dense, _) | Op::Release(dense) => {
                    if let Some((_, killed)) =
                        open.iter_mut().rev().find_map(|frame| frame.as_mut())
                    {
                        let (word, mask) = bit(*dense);
                        killed[word] |= mask;
                    }
                }
                _ => {}
            }
        }
        Self {
            trace,
            frames: vec![Frame::new(FrameKind::Function)],
            current: Some(vec![0; words]),
            kills,
            violations: Default::default(),
        }
    }

    fn target(&self, immediate: u32) -> usize {
        self.frames.len() - 1 - immediate as usize
    }

    /// An edge to a label: a block's end collects it; a loop head's state
    /// was fixed when the loop opened.
    fn branch(&mut self, immediate: u32) {
        let target = self.target(immediate);
        let Self {
            frames, current, ..
        } = self;
        if frames[target].kind != FrameKind::Loop {
            meet(&mut frames[target].label, current);
        }
    }

    /// A call or throw can enter the handlers of the innermost `try_table`
    /// around it, and those of outer ones unless an inner one catches all.
    fn may_throw(&mut self) {
        let Self {
            frames, current, ..
        } = self;
        if current.is_none() {
            return;
        }
        for frame in (0..frames.len()).rev() {
            if frames[frame].kind != FrameKind::TryTable {
                continue;
            }
            for handler in 0..frames[frame].handlers.len() {
                let target = frames[frame].handlers[handler];
                if frames[target].kind != FrameKind::Loop {
                    meet(&mut frames[target].label, current);
                }
            }
            if frames[frame].catches_all {
                break;
            }
        }
    }

    /// (operator, dense local) of every read a path reaches without a write.
    fn run(mut self) -> Vec<(usize, usize)> {
        for (index, op) in self.trace.ops.iter().enumerate() {
            match op {
                Op::Block => self.frames.push(Frame::new(FrameKind::Block)),
                Op::Loop => {
                    if let Some(head) = &mut self.current {
                        for (word, killed) in head.iter_mut().zip(&self.kills[&index]) {
                            *word &= !killed;
                        }
                    }
                    self.frames.push(Frame::new(FrameKind::Loop));
                }
                Op::If => {
                    let mut frame = Frame::new(FrameKind::If);
                    frame.other_arm = self.current.clone();
                    self.frames.push(frame);
                }
                Op::Else => {
                    let current = self.current.take();
                    let frame = self.frames.last_mut().expect("an open if");
                    meet(&mut frame.label, &current);
                    frame.kind = FrameKind::Else;
                    self.current = frame.other_arm.take();
                }
                Op::TryTable(labels, catches_all) => {
                    let mut frame = Frame::new(FrameKind::TryTable);
                    frame.handlers = labels.iter().map(|label| self.target(*label)).collect();
                    frame.catches_all = *catches_all;
                    self.frames.push(frame);
                }
                Op::End => {
                    let frame = self.frames.pop().expect("an open frame");
                    let current = self.current.take();
                    self.current = match frame.kind {
                        FrameKind::Loop | FrameKind::Function => current,
                        FrameKind::If => {
                            let mut joined = frame.label;
                            meet(&mut joined, &frame.other_arm);
                            meet(&mut joined, &current);
                            joined
                        }
                        FrameKind::Block | FrameKind::Else | FrameKind::TryTable => {
                            let mut joined = frame.label;
                            meet(&mut joined, &current);
                            joined
                        }
                    };
                }
                Op::Br(label) => {
                    self.branch(*label);
                    self.current = None;
                }
                Op::BrIf(label) => self.branch(*label),
                Op::BrTable(labels, default) => {
                    self.branch(*default);
                    for label in labels {
                        self.branch(*label);
                    }
                    self.current = None;
                }
                Op::Terminate => self.current = None,
                Op::MayThrow => self.may_throw(),
                Op::Read(dense) => {
                    let (word, mask) = bit(*dense);
                    if self
                        .current
                        .as_ref()
                        .is_some_and(|written| written[word] & mask == 0)
                    {
                        self.violations.insert((index, *dense));
                    }
                }
                Op::Write(dense) => {
                    let (word, mask) = bit(*dense);
                    if let Some(written) = &mut self.current {
                        written[word] |= mask;
                    }
                }
                Op::Reserve(dense, _) | Op::Release(dense) => {
                    let (word, mask) = bit(*dense);
                    if let Some(written) = &mut self.current {
                        written[word] &= !mask;
                    }
                }
                Op::Unmodelled(_) => unreachable!("refused before the flow runs"),
            }
        }
        self.violations.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_encoder::{BlockType, RefType};

    const TYPED: ValType = ValType::Ref(RefType {
        nullable: true,
        heap_type: HeapType::Concrete(0),
    });

    fn trace(build: impl FnOnce(&mut Trace)) -> Vec<(usize, usize)> {
        let mut trace = Trace::default();
        build(&mut trace);
        Flow::new(&trace).run()
    }

    #[test]
    fn a_holder_that_writes_first_on_every_path_passes() {
        assert!(trace(|trace| {
            trace.reserve(5, TYPED, std::panic::Location::caller());
            trace.instruction(&Instruction::If(BlockType::Empty));
            trace.instruction(&Instruction::LocalSet(5));
            trace.instruction(&Instruction::Else);
            trace.instruction(&Instruction::LocalSet(5));
            trace.instruction(&Instruction::End);
            trace.instruction(&Instruction::LocalGet(5));
            trace.release(5);
        })
        .is_empty());
    }

    #[test]
    fn a_read_after_a_join_with_an_unwritten_arm_is_found() {
        let violations = trace(|trace| {
            trace.reserve(5, TYPED, std::panic::Location::caller());
            trace.instruction(&Instruction::LocalSet(5));
            trace.release(5);
            // The next holder of local 5 writes it in one arm only.
            trace.reserve(5, TYPED, std::panic::Location::caller());
            trace.instruction(&Instruction::If(BlockType::Empty));
            trace.instruction(&Instruction::LocalSet(5));
            trace.instruction(&Instruction::End);
            trace.instruction(&Instruction::LocalGet(5));
        });
        assert_eq!(violations, [(7, 0)]);
    }

    #[test]
    fn a_release_on_a_back_edge_reaches_a_read_at_the_loop_head() {
        let violations = trace(|trace| {
            trace.reserve(3, TYPED, std::panic::Location::caller());
            trace.instruction(&Instruction::LocalSet(3));
            trace.instruction(&Instruction::Loop(BlockType::Empty));
            trace.instruction(&Instruction::LocalGet(3));
            trace.release(3);
            trace.reserve(3, TYPED, std::panic::Location::caller());
            trace.instruction(&Instruction::BrIf(0));
            trace.instruction(&Instruction::End);
        });
        assert_eq!(violations, [(3, 0)]);
    }

    #[test]
    fn a_write_by_a_holder_the_loop_reserved_does_not_cover_the_head() {
        // Local 3's holder before the loop reads it at the head; inside, the
        // loop releases it and a new holder writes it before the back edge.
        let violations = trace(|trace| {
            trace.reserve(3, TYPED, std::panic::Location::caller());
            trace.instruction(&Instruction::LocalSet(3));
            trace.instruction(&Instruction::Loop(BlockType::Empty));
            trace.instruction(&Instruction::LocalGet(3));
            trace.release(3);
            trace.reserve(3, TYPED, std::panic::Location::caller());
            trace.instruction(&Instruction::LocalSet(3));
            trace.instruction(&Instruction::BrIf(0));
            trace.instruction(&Instruction::End);
        });
        assert_eq!(violations, [(3, 0)]);
        // A loop that leaves the local alone keeps it written at the head.
        assert!(trace(|trace| {
            trace.reserve(3, TYPED, std::panic::Location::caller());
            trace.instruction(&Instruction::LocalSet(3));
            trace.instruction(&Instruction::Loop(BlockType::Empty));
            trace.instruction(&Instruction::LocalGet(3));
            trace.instruction(&Instruction::BrIf(0));
            trace.instruction(&Instruction::End);
            trace.instruction(&Instruction::LocalGet(3));
        })
        .is_empty());
    }

    #[test]
    fn a_handler_sees_the_state_at_every_call_in_its_body() {
        let violations = trace(|trace| {
            trace.instruction(&Instruction::Block(BlockType::Empty));
            trace.reserve(2, TYPED, std::panic::Location::caller());
            trace.instruction(&Instruction::TryTable(
                BlockType::Empty,
                std::borrow::Cow::Owned(vec![Catch::All { label: 0 }]),
            ));
            trace.instruction(&Instruction::Call(0));
            trace.instruction(&Instruction::LocalSet(2));
            trace.instruction(&Instruction::Call(1));
            trace.instruction(&Instruction::End);
            trace.instruction(&Instruction::LocalGet(2));
            trace.instruction(&Instruction::End);
            // After the block: the handler path joins without the write.
            trace.instruction(&Instruction::LocalGet(2));
        });
        assert_eq!(violations, [(9, 0)]);
    }

    #[test]
    fn untyped_and_unreserved_locals_are_not_checked() {
        assert!(trace(|trace| {
            trace.reserve(
                1,
                ValType::Ref(RefType::EQREF),
                std::panic::Location::caller(),
            );
            trace.instruction(&Instruction::LocalGet(1));
            trace.instruction(&Instruction::LocalGet(9));
        })
        .is_empty());
    }
}
