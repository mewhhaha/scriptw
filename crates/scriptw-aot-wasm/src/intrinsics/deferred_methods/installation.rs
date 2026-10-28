//! Shared recipe allocation and first publication; no installation record object.
use super::*;
use crate::operations::PropertyKeyLocals;
use crate::runtime_helpers::DeferredIntrinsicInstallParameters;

impl FunctionBuilder<'_> {
    pub(crate) fn compile_deferred_intrinsic_install_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::DeferredIntrinsicInstall);
        let p = self.helper_parameters::<DeferredIntrinsicInstallParameters>(&mut f);
        let s = self.runtime_schema();
        // Exactly the original one recipe per installation. No key or String
        // is resolved before it captures the defining Realm and empty cache.
        let recipe = s.reserve_gc_local(&mut f).initialize(
            s.struct_type::<DeferredIntrinsicValue>().construct(
                (
                    GcOperand::reference(&p.realm, s),
                    GcOperand::helper_standard_builtin(&p.builtin),
                    GcOperand::null(s),
                ),
                &mut f,
            ),
            &mut f,
        );
        let value = s
            .reserve_gc_local(&mut f)
            .initialize(recipe.load(s, &mut f).upcast(), &mut f);
        let enumerable = s.reserve_i32_local(&mut f);
        enumerable.set_constant(0, &mut f);
        let header = s.reserve_gc_local(&mut f).initialize(
            self.emit_object_header_projection(&p.target, &mut f),
            &mut f,
        );
        let key = PropertyKeyLocals::from_intrinsic_installation_parameter(
            &p.key,
            s,
            self.runtime_helper_base()?,
            &mut f,
        );
        s.call_helper(
            IntrinsicDataPublishArguments::new(
                &header,
                &key,
                &value,
                p.writable,
                enumerable,
                p.configurable,
            ),
            self.runtime_helper_base()?,
            &mut f,
        );
        key.clear(&mut f);
        header.clear(&mut f);
        s.release_i32_local(enumerable, &mut f);
        value.load(s, &mut f);
        value.clear(&mut f);
        recipe.clear(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
}
