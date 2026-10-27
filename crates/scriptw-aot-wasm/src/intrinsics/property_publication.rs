//! Shared intrinsic descriptor construction. Installers retain evaluation and Realm ownership.
use super::*;
use crate::gc_types::*;
use crate::runtime_helpers::{
    HelperParameters, IntrinsicAccessorPublishParameters, IntrinsicDataPublishArguments,
    IntrinsicDataPublishParameters, RuntimeHelperId,
};

impl FunctionBuilder<'_> {
    pub(super) fn emit_intrinsic_data_property_publication(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        key: &crate::operations::PropertyKeyLocals,
        value: &ValueLocals,
        writable: bool,
        enumerable: bool,
        configurable: bool,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let property_value = s.reserve_gc_local(f).initialize(
            s.struct_type::<StoredValue>().from_value(value, f).upcast(),
            f,
        );
        let w = s.reserve_i32_local(f);
        w.set_constant(i32::from(writable), f);
        let e = s.reserve_i32_local(f);
        e.set_constant(i32::from(enumerable), f);
        let c = s.reserve_i32_local(f);
        c.set_constant(i32::from(configurable), f);
        s.call_helper(
            IntrinsicDataPublishArguments::new(object, key, &property_value, w, e, c),
            self.runtime_helper_base()?,
            f,
        );
        s.release_i32_local(c, f);
        s.release_i32_local(e, f);
        s.release_i32_local(w, f);
        property_value.clear(f);
        Ok(())
    }
    /// Fresh descriptors remain distinct mutable records. Only their immutable
    /// value and canonical absent accessors are shared by alias publications.
    pub(crate) fn compile_intrinsic_data_publish_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::IntrinsicDataPublish);
        let p = self.helper_parameters::<IntrinsicDataPublishParameters>(&mut f);
        let s = self.runtime_schema();
        // No caller can smuggle accessor/unknown bits through a raw flag word.
        // Normalize the three private boolean operands before building Data flags.
        let writable = s.reserve_i32_local(&mut f);
        p.writable.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32Eqz);
        writable.store(&mut f);
        let configurable = s.reserve_i32_local(&mut f);
        p.configurable.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32Eqz);
        configurable.store(&mut f);
        let enumerable = s.reserve_i32_local(&mut f);
        p.enumerable.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32Eqz);
        enumerable.store(&mut f);
        let absent = s
            .reserve_gc_local(&mut f)
            .initialize(s.stored_undefined(&mut f), &mut f);
        let descriptor = s.reserve_gc_local(&mut f).initialize(
            s.struct_type::<PropertyDescriptor>().construct(
                (
                    GcOperand::data_descriptor_flags(writable, enumerable, configurable),
                    GcOperand::reference(&p.value, s),
                    GcOperand::reference(&absent, s),
                    GcOperand::reference(&absent, s),
                ),
                &mut f,
            ),
            &mut f,
        );
        self.emit_ordinary_append_property_entry(&p.object, &p.key, &descriptor, &mut f)?;
        descriptor.clear(&mut f);
        absent.clear(&mut f);
        s.release_i32_local(enumerable, &mut f);
        s.release_i32_local(configurable, &mut f);
        s.release_i32_local(writable, &mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    pub(crate) fn compile_intrinsic_accessor_publish_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::IntrinsicAccessorPublish);
        let p = self.helper_parameters::<IntrinsicAccessorPublishParameters>(&mut f);
        let s = self.runtime_schema();
        let enumerable = s.reserve_i32_local(&mut f);
        p.enumerable.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32Eqz);
        enumerable.store(&mut f);
        let configurable = s.reserve_i32_local(&mut f);
        p.configurable.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32Eqz);
        configurable.store(&mut f);
        let absent = s
            .reserve_gc_local(&mut f)
            .initialize(s.stored_undefined(&mut f), &mut f);
        let descriptor = s.reserve_gc_local(&mut f).initialize(
            s.struct_type::<PropertyDescriptor>().construct(
                (
                    GcOperand::accessor_descriptor_flags(enumerable, configurable),
                    GcOperand::subtype_reference(&absent, s),
                    GcOperand::reference(&p.getter, s),
                    GcOperand::reference(&p.setter, s),
                ),
                &mut f,
            ),
            &mut f,
        );
        self.emit_ordinary_append_property_entry(&p.object, &p.key, &descriptor, &mut f)?;
        descriptor.clear(&mut f);
        absent.clear(&mut f);
        s.release_i32_local(configurable, &mut f);
        s.release_i32_local(enumerable, &mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
}
