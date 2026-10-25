//! One admitted exact R/profile pair for repeated native-source emissions.
use super::{RuntimeArtifact, LAYOUT_VERSION};
use crate::EmitError;
use scriptw_intl::{IntlCompilationProfile, IntlDataSelection};
use std::sync::Arc;
/// Private fields bind the selected physical Intl frames to this exact Arc R.
/// This proof does not replace Wasm validation or canonical GC typed linking.
#[derive(Debug)]
pub struct ValidatedRuntimeArtifact {
    runtime: Arc<RuntimeArtifact>,
    selection: IntlDataSelection,
}
impl ValidatedRuntimeArtifact {
    pub(crate) fn require_linked_program(
        &self,
        emitted: &super::EmittedModule,
        diagnostic: &'static str,
    ) -> Result<(), EmitError> {
        if emitted.runtime_layout.is_some()
            || !emitted
                .artifact
                .runtime()
                .is_some_and(|actual| Arc::ptr_eq(actual, self.runtime()))
        {
            return Err(EmitError::unsupported(diagnostic));
        }
        Ok(())
    }

    pub fn new(
        runtime: Arc<RuntimeArtifact>,
        profile: IntlCompilationProfile,
    ) -> Result<Self, EmitError> {
        if !runtime
            .layout_admission
            .is_some_and(|version| version.0 == LAYOUT_VERSION)
        {
            return Err(EmitError::unsupported(
                "native source runtime has no checked current layout admission",
            ));
        }
        let selection = if profile == IntlCompilationProfile::Minimal {
            runtime
                .intl_compilation_data()
                .and_then(|data| IntlDataSelection::from_minimal_compilation_data(Arc::clone(data)))
        } else {
            None
        }
        .unwrap_or_else(|| IntlDataSelection::new(profile));
        runtime.require_intl_profile(&selection)?;
        Ok(Self { runtime, selection })
    }
    pub fn source_compilation_profile(&self) -> super::RuntimeSourceCompilationProfile {
        self.runtime.source_compilation_profile()
    }
    pub fn runtime(&self) -> &Arc<RuntimeArtifact> {
        &self.runtime
    }
    pub fn layout_version(&self) -> u32 {
        self.runtime
            .layout_admission
            .expect("validated runtime retains its immutable admission")
            .0
    }
    pub(crate) fn compiler_owned_pool(
        &self,
        functions: &std::collections::BTreeMap<scriptw_ir::FunctionId, crate::WasmFunctionMeta>,
        builtins: &[scriptw_ir::StandardBuiltinId],
        temporal: bool,
    ) -> Result<&Arc<crate::data::CompilerOwnedPool>, EmitError> {
        self.runtime
            .compiler_owned_pool
            .get_or_init(|| {
                crate::data::CompilerOwnedPool::for_runtime(
                    &self.runtime,
                    functions,
                    builtins,
                    temporal,
                    &self.selection,
                )
                .map(Arc::new)
            })
            .as_ref()
            .map_err(Clone::clone)
    }
    pub(crate) fn selection(&self) -> &IntlDataSelection {
        &self.selection
    }
}

#[cfg(test)]
mod pool_tests;
#[cfg(test)]
mod tests;
