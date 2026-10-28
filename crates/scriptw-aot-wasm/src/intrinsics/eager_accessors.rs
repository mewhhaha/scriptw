//! An eager accessor keeps its original identity and source-materialization order.
use super::*;
use crate::data::{PooledLiteralDomain, PooledStringIndex, StringPool};
use crate::emitted_function::FunctionIdentity;
use crate::gc_types::*;

/// Allocation accepts only the same metadata and runtime-owned source that were
/// admitted together. Captured and canonical factories retain their own owners.
pub(crate) struct AccessorPlan {
    meta: WasmFunctionMeta,
    source: PooledStringIndex,
}

impl AccessorPlan {
    fn eligible(builtin: StandardBuiltinId, meta: &WasmFunctionMeta) -> Result<bool, EmitError> {
        if builtin.constructable()
            || crate::module::standard_builtin_function_realm_slot(builtin).is_some()
        {
            return Ok(false);
        }
        if meta.standard_builtin != Some(builtin)
            || !matches!(meta.entry.identity(), FunctionIdentity::StandardBuiltin(id) if id == builtin)
            || meta.entry.executable_kind()? != ExecutableCodeKind::StandardBuiltin
            || meta.host_builtin.is_some()
        {
            return Err(EmitError::unsupported(
                "eager intrinsic accessor metadata and native entry identity differ",
            ));
        }
        Ok(
            meta.protocol() == scriptw_ir::FunctionProtocolIr::OrdinaryCallOnly
                && meta.strict
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
                && meta.constant_leaf.is_none()
                && meta.numeric_leaf.is_none()
                && meta.async_generator_resume_environment_plan.is_none(),
        )
    }

    fn new(
        builtin: StandardBuiltinId,
        meta: WasmFunctionMeta,
        strings: &StringPool,
    ) -> Result<Option<Self>, EmitError> {
        if !Self::eligible(builtin, &meta)? {
            return Ok(None);
        }
        let index = strings.pooled_string_index(&meta.to_string_value)?;
        let PooledLiteralDomain::Runtime(source) = strings.pooled_string_domain(index)? else {
            // P's local literals belong to its ProgramInstance, not R's table.
            return Ok(None);
        };
        Ok(Some(Self { meta, source }))
    }

    pub(crate) fn metadata(&self) -> &WasmFunctionMeta {
        &self.meta
    }
    pub(crate) fn source(&self) -> PooledStringIndex {
        self.source
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_intrinsic_accessor_callable(
        &mut self,
        builtin: StandardBuiltinId,
        realm: &crate::functions::RealmFunctionMaterializationContext,
        f: &mut Function,
    ) -> Result<GcLocal<FunctionObject>, EmitError> {
        let meta = self
            .functions
            .get(&builtin.function_id())
            .cloned()
            .ok_or_else(|| {
                EmitError::unsupported(format!(
                    "missing planned intrinsic {}",
                    builtin.debug_name()
                ))
            })?;
        if self.uses_heap {
            if let Some(plan) = AccessorPlan::new(builtin, meta, self.strings)? {
                let schema = self.runtime_schema();
                return Ok(schema.reserve_gc_local(f).initialize(
                    self.emit_validated_intrinsic_accessor_record(&plan, realm, f)?,
                    f,
                ));
            }
        }
        // This retains canonical lookup/store and every nonuniform native path.
        self.emit_intrinsic_callable(builtin, realm, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordinary_meta() -> WasmFunctionMeta {
        let builtin = StandardBuiltinId::ArrayBufferPrototypeByteLengthGetter;
        WasmFunctionMeta {
            constant_leaf: None,
            numeric_leaf: None,
            async_generator_resume_environment_plan: None,
            template_source: None,
            name: "get byteLength".into(),
            to_string_value: "function get byteLength() { [native code] }".into(),
            standard_builtin: Some(builtin),
            host_builtin: None,
            length: 0,
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
    fn shared_accessor_record_requires_uniform_metadata_and_original_native_identity() {
        let builtin = StandardBuiltinId::ArrayBufferPrototypeByteLengthGetter;
        assert!(AccessorPlan::eligible(builtin, &ordinary_meta()).unwrap());
        for mutate in [
            (|m: &mut WasmFunctionMeta| m.strict = false) as fn(&mut WasmFunctionMeta),
            |m| m.uses_super = true,
            |m| m.this_before_super = true,
            |m| m.captures_private_environment = true,
            |m| m.needs_active_function_identity = true,
            |m| m.is_derived_constructor = true,
            |m| m.is_named_expression = true,
            |m| m.is_static_class_member = true,
        ] {
            let mut meta = ordinary_meta();
            mutate(&mut meta);
            assert!(!AccessorPlan::eligible(builtin, &meta).unwrap());
        }
        let mut meta = ordinary_meta();
        meta.entry =
            crate::function_entry::PlannedFunctionEntry::standard(StandardBuiltinId::MathAbs, 17);
        assert!(AccessorPlan::eligible(builtin, &meta).is_err());
        let mut meta = ordinary_meta();
        meta.standard_builtin = Some(StandardBuiltinId::MathAbs);
        assert!(AccessorPlan::eligible(builtin, &meta).is_err());
        assert!(!AccessorPlan::eligible(
            StandardBuiltinId::ArrayBufferConstructor,
            &ordinary_meta()
        )
        .unwrap());
        // A valid code plan cannot authorize an uncollected source ordinal.
        assert!(AccessorPlan::new(builtin, ordinary_meta(), &StringPool::default()).is_err());
    }
}
