//! One execution's shared authority for real native source compilation.
use super::*;
use scriptw_aot_wasm::{RuntimeArtifact, ValidatedRuntimeArtifact};
mod trace;
use trace::TraceState;
pub(super) use trace::{CallbackTrace, Family, Phase};
use wasmtime::{AsContextMut, Extern, Instance};

/// The exact admitted R and already-linked Store execution define this authority.
/// Neither callback owns a replacement Realm, deadline or source/module cache.
pub(super) struct NativeSourceCompilerContext {
    pub(super) linker: WasmtimeLinker<WasmHostState>,
    runtime_instance: Instance,
    validated_runtime: ValidatedRuntimeArtifact,
    pub(super) policy: WasmCompilationPolicy,
    execution: Arc<WasmAgentExecutionControl>,
    trace: Option<TraceState>,
    pub(super) native_compilation_mode: WasmNativeCompilationMode,
    pub(super) instance_authority: Arc<()>,
    pub(super) empty_direct_eval_template:
        std::sync::OnceLock<scriptw_aot_wasm::NativeProgramTemplate>,
    pub(super) primitive_direct_eval_template:
        std::sync::OnceLock<scriptw_aot_wasm::NativeProgramTemplate>,
    pub(super) array_constructors:
        Mutex<crate::wasm_native_program_template::NativeArrayConstructorCache>,
    #[cfg(test)]
    pub(super) function_test_observer:
        Option<Arc<wasm_native_function_compiler::NativeFunctionCompilationObserver>>,
    #[cfg(test)]
    pub(super) global_script_test_observer:
        Option<Arc<wasm_native_global_script_compiler::NativeGlobalScriptCompilationObserver>>,
    #[cfg(test)]
    pub(super) direct_eval_test_observer:
        Option<Arc<wasm_native_direct_eval_compiler::NativeDirectEvalCompilationObserver>>,
}

impl NativeSourceCompilerContext {
    pub(super) fn new(
        linker: &WasmtimeLinker<WasmHostState>,
        runtime_instance: Instance,
        runtime: Arc<RuntimeArtifact>,
        policy: WasmCompilationPolicy,
        execution: Arc<WasmAgentExecutionControl>,
        native_compilation_mode: WasmNativeCompilationMode,
    ) -> Result<Self, EngineError> {
        if runtime.source_compilation_profile()
            != scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost
        {
            return Err(EngineError::new("native source runtime admission requires the NativeHost source-compilation profile"));
        }
        execution.remaining()?;
        let validated_runtime =
            ValidatedRuntimeArtifact::new(runtime, policy.intl_profile().clone());
        // Validation shares the already-armed execution deadline. Neither
        // admission nor callbacks acquire a fresh compilation budget.
        execution.remaining()?;
        let validated_runtime = validated_runtime.map_err(|error| {
            EngineError::new(format!("native source runtime admission failed: {error}"))
        })?;
        Ok(Self {
            linker: linker.clone(),
            runtime_instance,
            validated_runtime,
            policy,
            execution,
            native_compilation_mode,
            instance_authority: Arc::new(()),
            empty_direct_eval_template: std::sync::OnceLock::new(),
            primitive_direct_eval_template: std::sync::OnceLock::new(),
            trace: TraceState::from_environment(),
            array_constructors: Mutex::new(Default::default()),
            #[cfg(test)]
            function_test_observer: None,
            #[cfg(test)]
            global_script_test_observer: None,
            #[cfg(test)]
            direct_eval_test_observer: None,
        })
    }

    /// Native callbacks may request R exports absent from the original P's
    /// compact import set. Resolve from the same live R, never a replacement
    /// instance or a namespace-wide linker publication.
    pub(super) fn resolve_import(
        &self,
        store: &mut impl AsContextMut<Data = WasmHostState>,
        module: &str,
        name: &str,
    ) -> wasmtime::Result<Extern> {
        self.check_budget()?;
        if !WasmtimeEngine::same(store.as_context().engine(), self.linker.engine()) {
            return Err(wasmtime::Error::msg(
                "native import belongs to a different actual Engine",
            ));
        }
        let export = if module == scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE {
            self.runtime_instance
                .get_export(&mut *store, name)
                .ok_or_else(|| {
                    wasmtime::Error::msg(format!(
                        "native source lacks actual R export {module}.{name}"
                    ))
                })
        } else {
            self.linker.get(&mut *store, module, name)
        }?;
        self.check_budget()?;
        Ok(export)
    }

    pub(super) fn validated_runtime(&self) -> &ValidatedRuntimeArtifact {
        &self.validated_runtime
    }

    pub(super) fn runtime(&self) -> &Arc<RuntimeArtifact> {
        self.validated_runtime.runtime()
    }

    #[cfg(test)]
    pub(super) fn with_test_observers(
        mut self,
        function: Option<Arc<wasm_native_function_compiler::NativeFunctionCompilationObserver>>,
        global_script: Option<
            Arc<wasm_native_global_script_compiler::NativeGlobalScriptCompilationObserver>,
        >,
        direct_eval: Option<
            Arc<wasm_native_direct_eval_compiler::NativeDirectEvalCompilationObserver>,
        >,
    ) -> Self {
        self.function_test_observer = function;
        self.global_script_test_observer = global_script;
        self.direct_eval_test_observer = direct_eval;
        self
    }

    pub(super) fn begin_trace(&self, family: Family) -> Option<CallbackTrace<'_>> {
        self.trace.as_ref()?.begin(family)
    }

    pub(super) fn check_budget(&self) -> wasmtime::Result<()> {
        self.execution
            .remaining()
            .map(|_| ())
            .map_err(wasmtime::Error::new)
    }

    pub(super) fn execution(&self) -> &Arc<WasmAgentExecutionControl> {
        &self.execution
    }
}

#[cfg(test)]
mod tests;
