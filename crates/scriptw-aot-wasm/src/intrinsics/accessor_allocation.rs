//! One construction tail for source-validated ordinary strict native accessors.
//! The caller allocates/roots the header and executable code first and publishes
//! metadata only after this helper closes the Context/FunctionObject cycle.
use super::*;
use crate::gc_types::*;
use crate::runtime_helpers::{
    HelperParameters, IntrinsicAccessorAllocateParameters, RuntimeHelperId,
};

impl FunctionBuilder<'_> {
    pub(crate) fn compile_intrinsic_accessor_allocate_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut body = self.begin_helper_body(RuntimeHelperId::IntrinsicAccessorAllocate);
        let p = self.helper_parameters::<IntrinsicAccessorAllocateParameters>(&mut body);
        let f = &mut body;
        let s = self.runtime_schema();
        let home = s.reserve_gc_local(f).initialize(s.stored_undefined(f), f);
        let context = s.reserve_gc_local(f).initialize(
            s.struct_type::<FunctionContext>().construct(
                (
                    GcOperand::reference(&p.realm, s),
                    GcOperand::null(s),
                    GcOperand::null(s),
                    GcOperand::null(s),
                    GcOperand::reference(&home, s),
                    GcOperand::null(s),
                    GcOperand::null(s),
                    GcOperand::null(s),
                    GcOperand::null(s),
                    GcOperand::null(s),
                    GcOperand::null_function(),
                ),
                f,
            ),
            f,
        );
        // Retain the producer-admitted ordinal. The first toString observation
        // resolves its StringValue through the existing runtime string cache.
        let public_prototype = s.reserve_gc_local(f).initialize(s.stored_undefined(f), f);
        let callable = s.reserve_gc_local(f).initialize(
            s.struct_type::<FunctionObject>().construct(
                (
                    GcOperand::reference(&p.header, s),
                    GcOperand::reference(&p.code, s),
                    GcOperand::reference(&context, s),
                    GcOperand::null(s),
                    GcOperand::reference(&public_prototype, s),
                    GcOperand::boolean(true),
                    GcOperand::constant(scriptw_ir::ClassHeritageKind::None),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::constant(None::<crate::module::TypedArrayElementKind>),
                    GcOperand::null(s),
                    GcOperand::null(s),
                    GcOperand::i32_local(p.source_ordinal),
                ),
                f,
            ),
            f,
        );
        s.field(FunctionContextSchema::ACTIVE_FUNCTION).write(
            &context,
            GcOperand::nullable_reference(&callable, s),
            s,
            f,
        );
        public_prototype.clear(f);
        home.clear(f);
        context.clear(f);
        callable.load(s, f);
        callable.clear(f);
        p.release(f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(body))
    }
}
