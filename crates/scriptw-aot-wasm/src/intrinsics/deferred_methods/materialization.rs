//! One closed ordinary-native construction tail, selected without JS execution.
use super::*;
use crate::emitted_function::FunctionIdentity;

/// Only the metadata proven uniform for deferred methods can enter the tree.
/// Future planner changes fail emission here, before any partial body escapes.
pub(super) struct MethodPlan {
    pub(super) method: DeferredIntrinsicMethod,
    meta: WasmFunctionMeta,
}
impl MethodPlan {
    pub(super) fn new(
        method: DeferredIntrinsicMethod,
        meta: WasmFunctionMeta,
    ) -> Result<Self, EmitError> {
        let valid = meta.standard_builtin == Some(method.0)
            && DeferredIntrinsicMethod::new(method.0).is_some()
            && matches!(meta.entry.identity(), FunctionIdentity::StandardBuiltin(id) if id == method.0)
            && meta.entry.executable_kind()? == ExecutableCodeKind::StandardBuiltin
            && meta.protocol() == scriptw_ir::FunctionProtocolIr::OrdinaryCallOnly
            && meta.host_builtin.is_none()
            && meta.strict
            && meta.length_name_configurable
            && matches!(
                meta.class_element_execution_kind,
                scriptw_ir::ClassElementExecutionKind::None
            )
            && matches!(
                meta.class_heritage_kind,
                scriptw_ir::ClassHeritageKind::None
            )
            && !meta.is_named_expression
            && !meta.is_static_class_member
            && !meta.is_derived_constructor
            && !meta.is_synthetic_default_derived_constructor
            && !meta.uses_super
            && !meta.this_before_super
            && !meta.captures_private_environment
            && !meta.needs_active_function_identity
            && meta.class_instance_element_plan.is_none()
            && meta.template_source.is_none()
            && meta.numeric_leaf.is_none()
            && meta.async_generator_resume_environment_plan.is_none();
        if !valid {
            return Err(EmitError::unsupported(format!(
                "deferred intrinsic {} metadata is outside the ordinary strict nonconstructor construction domain",
                method.0.debug_name()
            )));
        }
        Ok(Self { method, meta })
    }
}

/// References outlive every selection branch and every allocation in the tail.
/// Null is only an internal unselected state; every invalid ID traps in the tree.
pub(super) struct Selection {
    code: GcLocal<ExecutableCode, Nullable>,
    name: GcLocal<StringValue, Nullable>,
    source: GcLocal<StringValue, Nullable>,
    length: I64Local,
}
impl Selection {
    fn reserve(s: &RuntimeSchema, f: &mut Function) -> Self {
        Self {
            code: s.reserve_gc_local(f).initialize_null(s, f),
            name: s.reserve_gc_local(f).initialize_null(s, f),
            source: s.reserve_gc_local(f).initialize_null(s, f),
            length: s.reserve_i64_local(f),
        }
    }
    fn release(self, s: &RuntimeSchema, f: &mut Function) {
        self.source.clear(f);
        self.name.clear(f);
        self.code.clear(f);
        s.release_i64_local(self.length, f);
    }
}
/// Produced only after the complete exact-ID tree has been emitted. The
/// construction tail cannot accept pending, merely reserved selection locals.
pub(super) struct SelectedMethod(Selection);
impl SelectedMethod {
    pub(super) fn release(self, s: &RuntimeSchema, f: &mut Function) {
        self.0.release(s, f);
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_deferred_method_selection(
        &mut self,
        methods: &[MethodPlan],
        builtin: I32Local,
        f: &mut Function,
    ) -> Result<SelectedMethod, EmitError> {
        let selected = Selection::reserve(self.runtime_schema(), f);
        self.emit_deferred_intrinsic_dispatch(methods, builtin, &selected, f)?;
        Ok(SelectedMethod(selected))
    }

    pub(super) fn emit_select_deferred_method(
        &mut self,
        plan: &MethodPlan,
        selected: &Selection,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        // Keep the existing declared entry/role/metadata validation together.
        let code = s.reserve_gc_local(f).initialize(
            plan.meta
                .entry
                .emit_executable_code(plan.meta.class_element_execution_kind, s, f)?,
            f,
        );
        selected.code.replace(code.load(s, f).nullable(), f);
        code.clear(f);
        let name = s.reserve_gc_local(f).initialize(
            self.emit_interned_string_reference(plan.meta.runtime_name(), f)?,
            f,
        );
        selected.name.replace(name.load(s, f).nullable(), f);
        name.clear(f);
        let source = s.reserve_gc_local(f).initialize(
            self.emit_interned_string_reference(&plan.meta.to_string_value, f)?,
            f,
        );
        selected.source.replace(source.load(s, f).nullable(), f);
        source.clear(f);
        f.instruction(&Instruction::I64Const(
            (plan.meta.length as f64).to_bits() as i64
        ));
        selected.length.store(f);
        Ok(())
    }

    pub(super) fn emit_materialize_selected_method(
        &mut self,
        selected: &SelectedMethod,
        realm: &crate::functions::RealmFunctionMaterializationContext,
        f: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        let selected = &selected.0;
        let s = self.runtime_schema();
        let code = s
            .reserve_gc_local(f)
            .initialize(selected.code.load(s, f).require_non_null(f), f);
        let name = s
            .reserve_gc_local(f)
            .initialize(selected.name.load(s, f).require_non_null(f), f);
        let source = s
            .reserve_gc_local(f)
            .initialize(selected.source.load(s, f).require_non_null(f), f);
        let prototype = s.reserve_value_local(f);
        prototype.set_reference(realm.function_prototype(), s, f);
        let yes = s.reserve_i32_local(f);
        yes.set_constant(1, f);
        let no = s.reserve_i32_local(f);
        no.set_constant(0, f);
        let object = s.reserve_gc_local(f).initialize(
            s.helper_reference_on_stack(s.call_helper(
                crate::runtime_helpers::NativeFunctionAllocateArguments::new(
                    &prototype,
                    &name,
                    selected.length,
                    yes,
                ),
                self.runtime_helper_base()?,
                f,
            )),
            f,
        );
        let undefined = s.reserve_gc_local(f).initialize(s.stored_undefined(f), f);
        let context = s.reserve_gc_local(f).initialize(
            s.struct_type::<FunctionContext>().construct(
                (
                    GcOperand::reference(realm.realm(), s),
                    GcOperand::null(s),
                    GcOperand::null(s),
                    GcOperand::null(s),
                    GcOperand::reference(&undefined, s),
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
        let callable = s.reserve_gc_local(f).initialize(
            s.struct_type::<FunctionObject>().construct(
                (
                    GcOperand::reference(&object, s),
                    GcOperand::reference(&code, s),
                    GcOperand::reference(&context, s),
                    GcOperand::reference(&source, s),
                    GcOperand::reference(&undefined, s),
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
                ),
                f,
            ),
            f,
        );
        // Close the same sole allocation cycle before metadata or cache publication.
        s.field(FunctionContextSchema::ACTIVE_FUNCTION).write(
            &context,
            GcOperand::nullable_reference(&callable, s),
            s,
            f,
        );
        let parent = s.reserve_value_local(f);
        parent.set_undefined(f);
        s.call_helper(
            crate::runtime_helpers::FunctionMetadataPublishArguments::new(
                &callable,
                selected.length,
                &name,
                yes,
                no,
                no,
                yes,
                &parent,
            ),
            self.runtime_helper_base()?,
            f,
        );
        parent.clear(f);
        undefined.clear(f);
        context.clear(f);
        object.clear(f);
        prototype.clear(f);
        source.clear(f);
        name.clear(f);
        code.clear(f);
        s.release_i32_local(no, f);
        s.release_i32_local(yes, f);
        let result = callable.load(s, f);
        callable.clear(f);
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
