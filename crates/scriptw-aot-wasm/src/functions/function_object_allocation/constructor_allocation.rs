//! One allocation tail for source-validated eager native Realm constructors.
//!
//! The caller still allocates the native header and its exact typed executable
//! code. Only their uniform Context/FunctionObject allocation is shared; public
//! metadata, constructor inheritance and prototype links keep their old owners.
use super::*;
use crate::data::{PooledLiteralDomain, PooledStringIndex, StringPool};
use crate::emitted_function::FunctionIdentity;
use crate::gc_types::*;
use crate::runtime_helpers::{
    HelperParameters, IntrinsicConstructorAllocateArguments,
    IntrinsicConstructorAllocateParameters, RuntimeHelperId,
};

/// No caller can use the shared tail without admitting its exact native entry,
/// uniform capture/flag shape and still-unresolved runtime-owned source together.
struct ConstructorAllocationPlan<'a> {
    meta: &'a WasmFunctionMeta,
    source: PooledStringIndex,
}

impl<'a> ConstructorAllocationPlan<'a> {
    fn validate_metadata(meta: &WasmFunctionMeta) -> Result<(), EmitError> {
        let identity_matches = match meta.entry.identity() {
            FunctionIdentity::StandardBuiltin(builtin) => {
                meta.standard_builtin == Some(builtin)
                    && meta.host_builtin.is_none()
                    && meta.entry.executable_kind()? == ExecutableCodeKind::StandardBuiltin
            }
            FunctionIdentity::HostBuiltin(builtin) => {
                meta.host_builtin == Some(builtin)
                    && meta.standard_builtin.is_none()
                    && meta.entry.executable_kind()? == ExecutableCodeKind::HostBuiltin
            }
            FunctionIdentity::Main
            | FunctionIdentity::ProgramInitialize
            | FunctionIdentity::Script { .. }
            | FunctionIdentity::PreparedScript { .. }
            | FunctionIdentity::RuntimeHelper(_) => false,
        };
        if !identity_matches || !meta.protocol().is_constructable() {
            return Err(EmitError::unsupported(
                "intrinsic constructor publication requires planned native constructor metadata",
            ));
        }
        if !meta.strict
            || !matches!(
                meta.class_element_execution_kind,
                scriptw_ir::ClassElementExecutionKind::None
            )
            || !matches!(
                meta.class_heritage_kind,
                scriptw_ir::ClassHeritageKind::None
            )
            || meta.is_named_expression
            || meta.is_static_class_member
            || meta.is_derived_constructor
            || meta.is_synthetic_default_derived_constructor
            || meta.uses_super
            || meta.this_before_super
            || meta.captures_private_environment
            || meta.needs_active_function_identity
            || meta.class_instance_element_plan.is_some()
            || meta.template_source.is_some()
            || meta.constant_leaf.is_some()
            || meta.numeric_leaf.is_some()
            || meta.async_generator_resume_environment_plan.is_some()
        {
            return Err(EmitError::unsupported(
                "intrinsic constructor allocation requires its uniform native capture shape",
            ));
        }
        Ok(())
    }

    fn new(meta: &'a WasmFunctionMeta, strings: &StringPool) -> Result<Self, EmitError> {
        Self::validate_metadata(meta)?;
        let index = strings.pooled_string_index(&meta.to_string_value)?;
        let PooledLiteralDomain::Runtime(source) = strings.pooled_string_domain(index)? else {
            return Err(EmitError::unsupported(
                "intrinsic constructor source must belong to the runtime string table",
            ));
        };
        Ok(Self { meta, source })
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_validated_realm_constructor_record(
        &mut self,
        meta: &WasmFunctionMeta,
        realm: &RealmFunctionMaterializationContext,
        f: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        let plan = ConstructorAllocationPlan::new(meta, self.strings)?;
        let s = self.runtime_schema();
        let prototype = s.reserve_value_local(f);
        prototype.set_reference(realm.function_prototype(), s, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_native_function_header(plan.meta, &prototype, f)?,
            f,
        );
        let code = s.reserve_gc_local(f).initialize(
            plan.meta
                .entry
                .emit_executable_code(plan.meta.class_element_execution_kind, s, f)?,
            f,
        );
        let source_ordinal = s.reserve_i32_local(f);
        source_ordinal.set_constant(plan.source.ordinal() as i32, f);
        let callable = s.reserve_gc_local(f).initialize(
            s.helper_reference_on_stack(s.call_helper(
                IntrinsicConstructorAllocateArguments::new(
                    &header,
                    &code,
                    realm.realm(),
                    source_ordinal,
                ),
                self.runtime_helper_base()?,
                f,
            )),
            f,
        );
        s.release_i32_local(source_ordinal, f);
        // Metadata remains after the original record's complete context cycle.
        // Bootstrap still owns all public prototype links and canonical slots.
        self.emit_publish_function_metadata(
            plan.meta,
            realm.realm(),
            &callable,
            FunctionPrototypeMaterialization::BootstrapSupplied,
            f,
        )?;
        code.clear(f);
        header.clear(f);
        prototype.clear(f);
        let result = callable.load(s, f);
        callable.clear(f);
        Ok(result)
    }

    pub(crate) fn compile_intrinsic_constructor_allocate_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut body = self.begin_helper_body(RuntimeHelperId::IntrinsicConstructorAllocate);
        let p = self.helper_parameters::<IntrinsicConstructorAllocateParameters>(&mut body);
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
        // Keep source resolution after Context allocation. Passing an eager
        // String would change the old allocation/key-materialization order.
        let source = s.reserve_gc_local(f).initialize(
            s.resolve_pooled_string_reference(p.source_ordinal, self.runtime_helper_base()?, f),
            f,
        );
        let public_prototype = s.reserve_gc_local(f).initialize(s.stored_undefined(f), f);
        // The exact typed executable was already admitted by the producer.
        // Derive this closed field from that immutable code identity, never
        // accept an arbitrary I32 as a typed-array element kind.
        let builtin = s.reserve_i32_local(f);
        s.field(ExecutableCodeSchema::STANDARD_BUILTIN)
            .read(&p.code, s, f)
            .store(builtin, f);
        let element_kind = GcI32DomainLocal::new(s, None::<TypedArrayElementKind>, f);
        for kind in TypedArrayElementKind::ALL {
            builtin.load(f);
            f.instruction(&Instruction::I32Const(GcI32Constant::encode(Some(
                kind.constructor(),
            ))));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            element_kind.set_constant(Some(kind), f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        let callable = s.reserve_gc_local(f).initialize(
            s.struct_type::<FunctionObject>().construct(
                (
                    GcOperand::reference(&p.header, s),
                    GcOperand::reference(&p.code, s),
                    GcOperand::reference(&context, s),
                    GcOperand::reference(&source, s),
                    GcOperand::reference(&public_prototype, s),
                    GcOperand::boolean(true),
                    GcOperand::constant(scriptw_ir::ClassHeritageKind::None),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    element_kind.operand(),
                    GcOperand::null(s),
                    GcOperand::null(s),
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
        element_kind.clear(s, f);
        s.release_i32_local(builtin, f);
        public_prototype.clear(f);
        source.clear(f);
        context.clear(f);
        home.clear(f);
        callable.load(s, f);
        callable.clear(f);
        p.release(f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constructor_meta() -> WasmFunctionMeta {
        let builtin = StandardBuiltinId::ArrayConstructor;
        WasmFunctionMeta {
            constant_leaf: None,
            numeric_leaf: None,
            async_generator_resume_environment_plan: None,
            template_source: None,
            name: "Array".into(),
            to_string_value: "function Array() { [native code] }".into(),
            standard_builtin: Some(builtin),
            host_builtin: None,
            length: 1,
            length_name_configurable: true,
            entry: crate::function_entry::PlannedFunctionEntry::standard(builtin, 17),
            strict: true,
            is_named_expression: false,
            class_element_execution_kind: scriptw_ir::ClassElementExecutionKind::None,
            class_heritage_kind: scriptw_ir::ClassHeritageKind::None,
            is_static_class_member: false,
            is_derived_constructor: false,
            is_synthetic_default_derived_constructor: false,
            class_instance_element_plan: None,
            uses_super: false,
            this_before_super: false,
            captures_private_environment: false,
            needs_active_function_identity: false,
        }
    }

    #[test]
    fn shared_constructor_tail_requires_exact_native_entry_and_uniform_captures() {
        assert!(ConstructorAllocationPlan::validate_metadata(&constructor_meta()).is_ok());
        for mutate in [
            (|m: &mut WasmFunctionMeta| m.strict = false) as fn(&mut WasmFunctionMeta),
            |m| m.uses_super = true,
            |m| m.this_before_super = true,
            |m| m.captures_private_environment = true,
            |m| m.needs_active_function_identity = true,
            |m| m.is_derived_constructor = true,
            |m| m.is_named_expression = true,
            |m| m.is_static_class_member = true,
            |m| m.is_synthetic_default_derived_constructor = true,
            |m| m.standard_builtin = Some(StandardBuiltinId::MathAbs),
            |m| m.host_builtin = Some(HostBuiltinId::GeneratorFunctionConstructor),
            |m| {
                m.entry = crate::function_entry::PlannedFunctionEntry::standard(
                    StandardBuiltinId::MathAbs,
                    17,
                )
            },
        ] {
            let mut meta = constructor_meta();
            mutate(&mut meta);
            assert!(ConstructorAllocationPlan::validate_metadata(&meta).is_err());
        }
        assert!(
            ConstructorAllocationPlan::new(&constructor_meta(), &StringPool::default()).is_err()
        );
    }
}
