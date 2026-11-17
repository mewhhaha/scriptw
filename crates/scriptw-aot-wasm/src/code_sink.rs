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
    free_locals: Vec<u32>,
    operator_count: u32,
    literal_operands: Vec<crate::program_literal_operands::FunctionSite>,
    native_program_context: Option<crate::native_program_contexts::FunctionContext>,
    excluded_i32_value: Option<(u32, i32)>,
    checked_escape_frames: Vec<LabelDepth>,
    last_instruction_escapes: bool,
    /// Operator ordinal of each hinted branch, and whether it is likely taken.
    branch_hints: std::collections::BTreeMap<u32, bool>,
    /// Identities of the open labels that only rare paths branch to.
    rare_labels: Vec<u64>,
    /// Every function and global index immediate, in body order.
    index_operands: Vec<IndexOperand>,
    /// The branch that decides whether the next instruction runs, if one
    /// branch does.
    anchor: Option<Anchor>,
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
            excluded_i32_value: self.excluded_i32_value,
            checked_escape_frames: self.checked_escape_frames.clone(),
            last_instruction_escapes: self.last_instruction_escapes,
            branch_hints: self.branch_hints.clone(),
            rare_labels: self.rare_labels.clone(),
            index_operands: self.index_operands.clone(),
            anchor: self.anchor,
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
            free_locals: Vec::new(),
            operator_count: 0,
            literal_operands: Vec::new(),
            native_program_context: None,
            excluded_i32_value: None,
            checked_escape_frames: Vec::new(),
            last_instruction_escapes: false,
            branch_hints: std::collections::BTreeMap::new(),
            rare_labels: Vec::new(),
            index_operands: Vec::new(),
            anchor: None,
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

    pub(crate) fn reserve_typed_local(&mut self, ty: ValType) -> u32 {
        assert!(
            !self.frames.is_empty(),
            "cannot reserve a local after function publication"
        );
        if let Some(position) = self.free_locals.iter().position(|index| {
            self.locals.type_at(*index - self.parameters.len() as u32) == Some(ty)
        }) {
            let index = self.free_locals.swap_remove(position);
            self.forget_i32_local(index);
            return index;
        }
        self.reserve_fresh_typed_local(ty)
    }

    /// Append a fresh local declaration when no released slot can be reused.
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
        index
    }

    pub(crate) fn release_typed_local(&mut self, index: u32) {
        let base = u32::try_from(self.parameters.len()).expect("parameter count overflow");
        assert!(
            index >= base + self.initial_local_count && index < base + self.locals.count(),
            "only this body's dynamically reserved locals can be released"
        );
        assert!(!self.free_locals.contains(&index), "local released twice");
        self.forget_i32_local(index);
        self.free_locals.push(index);
    }

    fn forget_i32_local(&mut self, index: u32) {
        if self
            .excluded_i32_value
            .is_some_and(|(local, _)| local == index)
        {
            self.excluded_i32_value = None;
        }
    }

    /// The matching arm must leave this guard on every path. The sink owns
    /// the comparison and rejects branches into its continuation; the final
    /// instruction must also escape. Only the unmatched arm can fall through.
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
            !self.free_locals.contains(&local),
            "checked guard local is live"
        );
        if self.excluded_i32_value == Some((local, value)) {
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
        if !self.free_locals.contains(&local) {
            self.excluded_i32_value = Some((local, value));
        }
    }

    fn depth(&self) -> u32 {
        u32::try_from(self.frames.len()).expect("Wasm label depth exceeds u32")
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
        if self.frames.is_empty() {
            if matches!(instruction, Instruction::End) {
                panic!(
                    "wasm `end` with no open label: the emitter closed more frames than it opened"
                );
            }
            panic!("instruction emitted after the function body's final end");
        }

        match instruction {
            Instruction::LocalSet(local) | Instruction::LocalTee(local) => {
                self.forget_i32_local(*local);
            }
            Instruction::Block(_)
            | Instruction::Loop(_)
            | Instruction::If(_)
            | Instruction::Else
            | Instruction::End
            | Instruction::TryTable(_, _)
            | Instruction::Try(_)
            | Instruction::Catch(_)
            | Instruction::CatchAll
            | Instruction::Delegate(_)
            | Instruction::Br(_)
            | Instruction::BrIf(_)
            | Instruction::BrTable(_, _)
            | Instruction::BrOnNull(_)
            | Instruction::BrOnNonNull(_)
            | Instruction::BrOnCast { .. }
            | Instruction::BrOnCastFail { .. }
            | Instruction::Return
            | Instruction::ReturnCall(_)
            | Instruction::ReturnCallRef(_)
            | Instruction::ReturnCallIndirect { .. }
            | Instruction::Throw(_)
            | Instruction::ThrowRef
            | Instruction::Rethrow(_)
            | Instruction::Unreachable => self.excluded_i32_value = None,
            Instruction::Suspend(_)
            | Instruction::Resume { .. }
            | Instruction::ResumeThrow { .. }
            | Instruction::ResumeThrowRef { .. }
            | Instruction::Switch { .. } => {
                self.excluded_i32_value = None;
                assert!(
                    self.checked_escape_frames.is_empty(),
                    "stack switching cannot establish a checked escape fact"
                );
            }
            _ => {}
        }

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
            | Instruction::BrOnCastFail { relative_depth, .. } => {
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
            matches!(
                instruction,
                Instruction::If(_)
                    | Instruction::BrIf(_)
                    | Instruction::BrOnNull(_)
                    | Instruction::BrOnNonNull(_)
                    | Instruction::BrOnCast { .. }
                    | Instruction::BrOnCastFail { .. }
            ),
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

    /// The hinted branches of the body so far, in operator order.
    pub(crate) fn branch_hints(&self) -> Vec<crate::branch_hints::FunctionSite> {
        self.branch_hints
            .iter()
            .map(|(operator, likely)| crate::branch_hints::FunctionSite::new(*operator, *likely))
            .collect()
    }

    pub(crate) fn program_literal_count(&mut self, value: crate::gc_types::ProgramLiteralCount) {
        self.literal_operands
            .push(crate::program_literal_operands::FunctionSite::count(
                self.operator_count,
                value,
            ));
        self.instruction(&Instruction::I32Const(value.value() as i32));
    }

    pub(crate) fn program_literal_index(&mut self, value: crate::gc_types::ProgramLiteralIndex) {
        self.literal_operands
            .push(crate::program_literal_operands::FunctionSite::index(
                self.operator_count,
                value,
            ));
        self.instruction(&Instruction::I32Const(value.ordinal() as i32));
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

    /// The finished body and its index operands, at their offsets in it.
    pub(crate) fn into_body_and_index_operands(
        self,
        context: &dyn core::fmt::Display,
    ) -> (wasm_encoder::Function, Vec<IndexOperand>) {
        assert!(
            self.frames.is_empty(),
            "function body for {context} has an unclosed control frame: {} label(s) still open",
            self.frames.len()
        );
        let raw = self.body.into_raw_body();
        let mut body = self.locals.encode_body();
        // Locals declared after construction lengthen the declaration prefix.
        let prefix = u32::try_from(body.byte_len()).expect("Wasm body exceeds u32 bytes");
        let initial = u32::try_from(self.initial_prefix_len).expect("Wasm body exceeds u32 bytes");
        body.raw(raw[self.initial_prefix_len..].iter().copied());
        let mut operands = self.index_operands;
        for operand in &mut operands {
            operand.offset = operand.offset - initial + prefix;
        }
        (body, operands)
    }

    #[cfg(test)]
    pub(crate) fn into_body(self) -> wasm_encoder::Function {
        self.into_body_and_index_operands(&"an unnamed test body").0
    }
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
