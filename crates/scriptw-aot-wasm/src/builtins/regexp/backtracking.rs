//! One in-function choice recovery owner; requests never carry branch depths.
use super::*;
use crate::emit::{ControlFrameKind, ControlTarget};

/// Only this scope's mismatch block can receive a backtrack request.
pub(super) struct BacktrackRequest {
    target: ControlTarget,
}

/// New frames enclose the original instruction block/loop, preserving every
/// existing opcode-local branch. Candidate/reset work remains outside retry.
pub(super) struct BacktrackRequestScope {
    request: BacktrackRequest,
    retry: ControlTarget,
    exhausted: ControlTarget,
}

/// Available only after the request block closes. Consuming it publishes the
/// sole recovery body and closes the remaining retry/exhaustion frames.
pub(super) struct BacktrackRecoveryScope {
    retry: ControlTarget,
    exhausted: ControlTarget,
}

impl BacktrackRequestScope {
    pub(super) fn open(builder: &mut FunctionBuilder<'_>, f: &mut Function) -> Self {
        let exhausted = builder.open_frame(ControlFrameKind::Block, f);
        let retry = builder.open_frame(ControlFrameKind::Loop, f);
        let request = BacktrackRequest {
            target: builder.open_frame(ControlFrameKind::Block, f),
        };
        Self {
            request,
            retry,
            exhausted,
        }
    }

    pub(super) fn request(&self) -> &BacktrackRequest {
        &self.request
    }

    pub(super) fn close(
        self,
        builder: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) -> BacktrackRecoveryScope {
        // The original instruction pair's normal fallthrough exhausted this
        // candidate. It must skip recovery and retain candidate advancement.
        builder.emit_branch_to_target(self.exhausted, f);
        builder.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        BacktrackRecoveryScope {
            retry: self.retry,
            exhausted: self.exhausted,
        }
    }
}

impl FunctionBuilder<'_> {
    /// Consume the existing mismatch I32. No snapshots or state are changed on
    /// the false path, and all request sites reach the same recovery owner.
    pub(super) fn emit_regexp_backtrack_request_if(
        &self,
        request: &BacktrackRequest,
        f: &mut Function,
    ) {
        self.emit_branch_if_to_target(request.target, f);
    }

    pub(super) fn emit_regexp_backtrack_recovery(
        &mut self,
        workspace: &MatcherWorkspace,
        cursor: RegExpInputCursor,
        pc: I64Local,
        scope: BacktrackRecoveryScope,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let selected = s.reserve_i32_local(f);
        let choose = self.open_frame(ControlFrameKind::Loop, f);
        workspace.choices().is_empty(f);
        self.emit_branch_if_to_target(scope.exhausted, f);
        workspace.choices().with_top(self, f, |entry, builder, f| {
            entry.is_kind(ChoiceEntryKind::RequiredRun, f);
            builder.open_frame(ControlFrameKind::If, f);
            entry.restore_required_run(builder, pc, cursor, selected, f);
            selected.load(f);
            f.instruction(&Instruction::I32Eqz);
            builder.emit_branch_if_to_target(choose, f);
            f.instruction(&Instruction::Else);
            entry.is_kind(ChoiceEntryKind::LazyProgressAttempt, f);
            builder.open_frame(ControlFrameKind::If, f);
            entry.discard_through(f);
            builder.emit_branch_to_target(choose, f);
            builder.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            entry.is_kind(ChoiceEntryKind::LazyProgressChoice, f);
            builder.open_frame(ControlFrameKind::If, f);
            entry.activate_lazy_attempt(f);
            f.instruction(&Instruction::Else);
            entry.discard_through(f);
            builder.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            entry.restore_fallback(pc, f);
            entry.restore_cursor(cursor, f);
            entry.restore_state(f);
            builder.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            builder.emit_branch_to_target(scope.retry, f);
        });
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        // Every recovery outcome resumes dispatch, chooses another entry or
        // exhausts the candidate; no implicit fallthrough may select a route.
        f.instruction(&Instruction::Unreachable);
        s.release_i32_local(selected, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
    }
}
