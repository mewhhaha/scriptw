//! One compiled loop installs each contiguous deferred run on a known target.
use super::*;
use crate::intrinsics::IntrinsicMethodPlan;
use crate::runtime_helpers::{
    DeferredIntrinsicInstallRunArguments, DeferredIntrinsicInstallRunParameters,
};

impl FunctionBuilder<'_> {
    pub(crate) fn emit_install_intrinsic_methods(
        &mut self,
        target: &ValueLocals,
        plan: IntrinsicMethodPlan,
        realm: &crate::functions::RealmFunctionMaterializationContext,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let methods = plan.methods()?;
        let mut start = 0;
        while start < methods.len() {
            let method = methods[start];
            if DeferredIntrinsicMethod::new(method.builtin).is_none() {
                // Canonical identities remain eager and are ordering barriers.
                self.emit_install_intrinsic_method(
                    target,
                    method.key,
                    method.builtin,
                    realm,
                    method.writable,
                    method.configurable,
                    f,
                )?;
                start += 1;
                continue;
            }
            let end = start
                + methods[start..]
                    .iter()
                    .take_while(|method| DeferredIntrinsicMethod::new(method.builtin).is_some())
                    .count();
            let span = self.strings.intrinsic_installation_span(plan, start..end)?;
            self.runtime_schema().call_helper(
                DeferredIntrinsicInstallRunArguments::new(target, realm.realm(), span),
                self.runtime_helper_base()?,
                f,
            );
            start = end;
        }
        Ok(())
    }

    pub(crate) fn compile_deferred_intrinsic_install_run_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::DeferredIntrinsicInstallRun);
        let p = self.helper_parameters::<DeferredIntrinsicInstallRunParameters>(&mut f);
        let s = self.runtime_schema();
        let base = self.runtime_helper_base()?;
        p.rows.emit_for_each(
            self.strings.runtime_intrinsic_installation_rows()?,
            s,
            &mut f,
            |row, f| {
                s.call_helper(
                    DeferredIntrinsicInstallArguments::new(
                        &p.target,
                        &p.realm,
                        row.builtin(),
                        row.key(),
                        row.writable,
                        row.configurable,
                    ),
                    base,
                    f,
                );
                // The property entry now owns the fresh recipe. No function
                // observation or extra packaging allocation occurs here.
                f.instruction(&Instruction::Drop);
            },
        );
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
}
