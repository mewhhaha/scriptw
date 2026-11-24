//! Runtime-to-program hooks.
//!
//! Each frame has an explicit ProgramInstance owner. Runtime code reads the
//! selected owner's immutable typed hook field and preserves its absent path.
//! A dispatch token roots that exact callee and borrows the same owner through
//! the registered hook call; another program's entry cannot replace either.

use super::*;
use crate::emit::{ControlFrameKind, FunctionBuilder};
use crate::gc_types::{
    FunctionLocal, FunctionStackReference, GcProgramHookRole, GcStackReference, NonNullable,
    ProgramInstance, ProgramInstanceLocals, RuntimeSchema,
};
use crate::runtime_helpers::ProgramHook;

/// The hooks a program has data for. Only these fields are non-null.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProvidedHooks(u32);

impl ProvidedHooks {
    pub(crate) fn of(functions: &FunctionMetaRegistry, strings: &StringPool) -> Self {
        let mut provided = 0;
        for hook in ProgramHook::ALL {
            let has_data = match hook {
                ProgramHook::PreparedDynamicFunction => {
                    !functions.prepared_dynamic_functions().is_empty()
                }
                ProgramHook::PreparedScript => functions
                    .prepared_scripts()
                    .iter()
                    .any(|script| !matches!(script.kind, PreparedScriptKind::DirectEval(_))),
                ProgramHook::RealmModuleImport => functions.module_graph().is_some(),
                ProgramHook::ModuleBodyReaction => functions.module_execution_record_count() > 0,
                ProgramHook::RegExpProgramCandidate => strings.runtime_regexp_table().is_some(),
                ProgramHook::ProgramLiteralResolve => strings.has_program_literals(),
            };
            if has_data {
                provided |= 1 << hook as u32;
            }
        }
        Self(provided)
    }

    pub(crate) const fn provides(self, hook: ProgramHook) -> bool {
        self.0 & (1 << hook as u32) != 0
    }
}

/// Minted only in the installed branch below. The private typed function root
/// and owner borrow cannot be fabricated from an enum or independent operand.
/// The token cannot escape its dispatch owner or be cloned across branches.
pub(crate) struct InstalledHook<'owner, R: GcProgramHookRole> {
    program: &'owner ProgramInstanceLocals,
    callee: FunctionLocal<R>,
}
impl<R: GcProgramHookRole> InstalledHook<'_, R> {
    pub(crate) fn program_reference(
        &self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<ProgramInstance> {
        self.program.reference(schema, function)
    }
    pub(crate) fn reference(&self, function: &mut Function) -> FunctionStackReference<R> {
        self.callee.load(function)
    }
    fn clear(self, function: &mut Function) {
        self.callee.clear(function);
    }
}

impl FunctionBuilder<'_> {
    /// The installed branch receives the exact non-null reference read from
    /// this frame's ProgramInstance. Both roots outlive the callback.
    pub(crate) fn emit_program_hook_dispatch<R: GcProgramHookRole>(
        &mut self,
        installed: impl FnOnce(&mut Self, &InstalledHook<'_, R>, &mut Function) -> Result<(), EmitError>,
        absent: impl FnOnce(&mut Self, &mut Function) -> Result<(), EmitError>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let program = self.capture_current_program(function);
        let result =
            self.emit_owned_program_hook_dispatch::<R>(&program, installed, absent, function);
        program.clear(function);
        result
    }

    /// Retained continuations select their own code/data owner, independently
    /// of the frame that drains the job. The token still binds the exact
    /// immutable typed hook and the same owner for the entire callback.
    pub(crate) fn emit_owned_program_hook_dispatch<R: GcProgramHookRole>(
        &mut self,
        owner: &ProgramInstanceLocals,
        installed: impl FnOnce(&mut Self, &InstalledHook<'_, R>, &mut Function) -> Result<(), EmitError>,
        absent: impl FnOnce(&mut Self, &mut Function) -> Result<(), EmitError>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        // Own a separate root so the token never borrows self across the
        // installed callback, which may reenter arbitrary JavaScript.
        let program =
            ProgramInstanceLocals::bind(schema, owner.reference(schema, function), function);
        let candidate = program.hook::<R>(schema, function);
        candidate.load(function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let callee = schema
            .reserve_function_local::<R, NonNullable>(function)
            .initialize(
                candidate.load(function).require_non_null(function),
                function,
            );
        let hook = InstalledHook {
            program: &program,
            callee,
        };
        installed(self, &hook, function)?;
        hook.clear(function);
        function.instruction(&Instruction::Else);
        absent(self, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        candidate.clear(function);
        program.clear(function);
        Ok(())
    }

    /// The body of a hook's program-owned helper.
    pub(crate) fn compile_program_hook(
        &mut self,
        hook: ProgramHook,
    ) -> Result<Function, EmitError> {
        match hook {
            ProgramHook::PreparedDynamicFunction => self.compile_prepared_dynamic_function_hook(),
            ProgramHook::PreparedScript => self.compile_prepared_script_hook(),
            ProgramHook::RealmModuleImport => self.compile_realm_module_import_hook(),
            ProgramHook::ModuleBodyReaction => self.compile_module_body_reaction_hook(),
            ProgramHook::RegExpProgramCandidate => self.compile_regexp_program_candidate_hook(),
            ProgramHook::ProgramLiteralResolve => self.compile_program_literal_resolve_hook(),
        }
    }
}
