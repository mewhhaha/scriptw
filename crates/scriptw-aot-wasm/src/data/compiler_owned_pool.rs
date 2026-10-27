//! Canonical immutable compiler data, admitted once by the exact R owner.
use super::*;
use std::sync::Arc;

impl CompilerOwnedPool {
    pub(crate) fn for_runtime(
        runtime: &crate::RuntimeArtifact,
        functions: &BTreeMap<FunctionId, WasmFunctionMeta>,
        builtins: &[StandardBuiltinId],
        temporal: bool,
        selection: &scriptw_intl::IntlDataSelection,
    ) -> Result<Self, EmitError> {
        let mut prefix = Self::collect(functions, builtins, temporal, selection)?;
        runtime.layout().require_pool(&prefix.0)?;
        if let Some(matches) = runtime.admitted_compiler_pool_matches(
            &prefix.0.pooled_code_unit_bytes[..],
            &prefix.0.bytes[..],
        ) {
            if !matches {
                return Err(EmitError::unsupported(
                    "canonical compiler pool differs from its immutable producer receipt",
                ));
            }
        } else {
            prefix.require_runtime_data(runtime.bytes())?;
        }
        prefix.0.bytes.freeze();
        prefix.0.pooled_code_unit_bytes.freeze();
        prefix.0.refs.freeze();
        Ok(prefix)
    }

    // Matching lengths alone cannot admit another compiler's same-sized data.
    // Compare the canonical passive UTF-16 and active static images once,
    // before this opaque prefix enters the R-owned OnceLock.
    fn require_runtime_data(&self, bytes: &[u8]) -> Result<(), EmitError> {
        use wasmparser::{DataKind, Operator, Parser, Payload};
        let error = || {
            EmitError::unsupported(
                "canonical compiler pool does not match its exact runtime data images",
            )
        };
        let mut found = false;
        for payload in Parser::new(0).parse_all(bytes) {
            if let Payload::DataSection(section) = payload.map_err(|_| error())? {
                if found || section.count() != 4 {
                    return Err(error());
                }
                found = true;
                let region = self.0.normalization_image_region();
                let span = region.offsets();
                let expected = [
                    (&self.0.pooled_code_unit_bytes[..], None),
                    (&self.0.bytes[span.clone()], None),
                    (&self.0.bytes[..span.start], Some(STATIC_DATA_OFFSET)),
                    (&self.0.bytes[span.end..], Some(region.end())),
                ];
                for (row, (bytes, address)) in section.into_iter().zip(expected) {
                    let row = row.map_err(|_| error())?;
                    if row.data != bytes {
                        return Err(error());
                    }
                    match (row.kind, address) {
                        (DataKind::Passive, None) => {}
                        (
                            DataKind::Active {
                                memory_index: 0,
                                offset_expr,
                            },
                            Some(address),
                        ) => {
                            let mut ops = offset_expr.get_operators_reader();
                            if !matches!(ops.read().map_err(|_| error())?, Operator::I32Const { value } if value == address as i32)
                                || !matches!(ops.read().map_err(|_| error())?, Operator::End)
                                || !ops.eof()
                            {
                                return Err(error());
                            }
                        }
                        _ => return Err(error()),
                    }
                }
            }
        }
        if found {
            Ok(())
        } else {
            Err(error())
        }
    }

    pub(crate) fn collect_program(
        prefix: &Arc<Self>,
        script: &ScriptIr,
        functions: &BTreeMap<FunctionId, WasmFunctionMeta>,
    ) -> Result<StringPool, EmitError> {
        Self(StringPool::from_shared_prefix(prefix)).collect_script(script, functions)
    }
}

impl StringPool {
    fn from_shared_prefix(prefix: &Arc<CompilerOwnedPool>) -> Self {
        let base = &prefix.0;
        Self {
            compiler_owned: base.compiler_owned,
            shared_prefix: Some(Arc::clone(prefix)),
            bytes: base.bytes.program_overlay(),
            pooled_code_unit_bytes: base.pooled_code_unit_bytes.program_overlay(),
            refs: base.refs.program_overlay(),
            uses_heap: base.uses_heap,
            lowercase_mapping_table_ptr: base.lowercase_mapping_table_ptr,
            lowercase_mapping_count: base.lowercase_mapping_count,
            uppercase_mapping_table_ptr: base.uppercase_mapping_table_ptr,
            uppercase_mapping_count: base.uppercase_mapping_count,
            cased_range_table_ptr: base.cased_range_table_ptr,
            cased_range_count: base.cased_range_count,
            case_ignorable_range_table_ptr: base.case_ignorable_range_table_ptr,
            case_ignorable_range_count: base.case_ignorable_range_count,
            normalization_region: base.normalization_region,
            canonical_decomposition_table_ptr: base.canonical_decomposition_table_ptr,
            canonical_decomposition_count: base.canonical_decomposition_count,
            compatibility_decomposition_table_ptr: base.compatibility_decomposition_table_ptr,
            compatibility_decomposition_count: base.compatibility_decomposition_count,
            combining_class_table_ptr: base.combining_class_table_ptr,
            combining_class_count: base.combining_class_count,
            composition_table_ptr: base.composition_table_ptr,
            composition_count: base.composition_count,
            ..Self::default()
        }
    }

    pub(crate) fn collect_with_validated_runtime(
        script: &ScriptIr,
        functions: &BTreeMap<FunctionId, WasmFunctionMeta>,
        builtins: &[StandardBuiltinId],
        temporal: bool,
        validated: &crate::ValidatedRuntimeArtifact,
    ) -> Result<Self, EmitError> {
        let prefix = validated.compiler_owned_pool(functions, builtins, temporal)?;
        CompilerOwnedPool::collect_program(prefix, script, functions)
    }

    pub(super) fn runtime_prefix(&self) -> Option<&Self> {
        self.shared_prefix.as_ref().map(|prefix| &prefix.0)
    }

    /// Borrowed native-template data contains only this P's appended bytes.
    pub(crate) fn program_utf16_bytes(&self) -> &[u8] {
        &self.pooled_code_unit_bytes[self.compiler_owned.code_unit_bytes..]
    }
    pub(crate) fn program_metadata_bytes(&self) -> &[u8] {
        &self.bytes[self.compiler_owned.static_bytes..]
    }
    pub(crate) fn program_recipe_bytes(&self) -> &[u8] {
        self.compact_array_recipes.bytes()
    }
}

#[cfg(test)]
mod tests;
