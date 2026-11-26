//! Real independent global Script compilation inside the existing native Store.
use super::wasm_gc_host::TypedGcHostImport;
use super::*;
use crate::wasm_native_source_compiler::{Family, NativeSourceCompilerContext, Phase};
use scriptw_aot_wasm::GcHostImport;
use scriptw_ir::lower_runtime_global_script;
use wasmtime::{RootScope, Val};

mod factory;
#[cfg(test)]
mod observer;
mod outcome;
mod signature;
mod sources;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(super) use observer::{
    NativeGlobalScriptCompilationObserver, NativeGlobalScriptCompilationSnapshot,
};
use outcome::{CompileRejection, EscapedReply};
use signature::NativeGlobalScriptSignature;
use sources::ConvertedRequest;

fn abi_error(message: impl std::fmt::Display) -> wasmtime::Error {
    wasmtime::Error::new(EngineError::new(format!(
        "invalid native global Script compiler ABI: {message}"
    )))
}

pub(super) fn link(
    linker: &mut WasmtimeLinker<WasmHostState>,
    import: TypedGcHostImport<'_>,
) -> Result<(), EngineError> {
    let row = GcHostImport::CompileGlobalScript;
    let Some(function) = import.into_func(
        "duplicate native global Script compiler import",
        "native global Script compiler import is not a function",
    )?
    else {
        return Ok(());
    };
    let signature = NativeGlobalScriptSignature::validate(function)
        .map_err(|e| EngineError::new(format!("native global Script compiler signature: {e:#}")))?;
    linker
        .func_new(
            row.module(),
            row.name(),
            signature.function.clone(),
            move |caller, inputs, outputs| compile(caller, inputs, outputs, &signature),
        )
        .map_err(|e| EngineError::new(format!("native global Script compiler linkage: {e:#}")))?;
    Ok(())
}

fn compile(
    mut caller: WasmtimeCaller<'_, WasmHostState>,
    inputs: &[Val],
    outputs: &mut [Val],
    signature: &NativeGlobalScriptSignature,
) -> wasmtime::Result<()> {
    let context = caller
        .data()
        .native_source_compiler
        .clone()
        .ok_or_else(|| {
            abi_error("external artifact has no armed native source compiler context")
        })?;
    if !WasmtimeEngine::same(caller.engine(), context.linker.engine()) {
        return Err(abi_error("native source linker belongs to another Engine"));
    }
    context.check_budget()?;
    #[cfg(test)]
    if let Some(observer) = &context.global_script_test_observer {
        observer.request();
    }
    let trace = context.begin_trace(Family::GlobalScript);
    let engine = caller.engine().clone();
    let reply = {
        let mut roots = RootScope::new(&mut caller);
        let decode = trace.as_ref().map(|trace| trace.phase(Phase::Decode));
        let request = ConvertedRequest::decode(&mut roots, inputs, signature, context.execution())?;
        drop(decode);
        context.check_budget()?;
        let parse_lower = trace.as_ref().map(|trace| trace.phase(Phase::ParseLower));
        let lowered = lower_runtime_global_script(
            request.kind,
            &request.source,
            context.policy.host_surface_policy(),
        )
        .map(|program| program.with_language_features(context.policy.language_features()));
        drop(parse_lower);
        context.check_budget()?;
        match lowered {
            Ok(program) => {
                let completion = factory::instantiate_and_execute(
                    &mut roots,
                    &engine,
                    &context,
                    &request,
                    &program,
                    signature,
                    trace.as_ref(),
                )?;
                let reply = EscapedReply::entered(&mut roots, completion)?;
                #[cfg(test)]
                if let Some(observer) = &context.global_script_test_observer {
                    observer.completed_factory();
                }
                reply
            }
            Err(error) => match CompileRejection::classify(error)? {
                CompileRejection::Syntax(message) => {
                    let reply = EscapedReply::syntax(&mut roots, &message, signature)?;
                    context.check_budget()?;
                    #[cfg(test)]
                    if let Some(observer) = &context.global_script_test_observer {
                        observer.syntax_rejection();
                    }
                    reply
                }
            },
        }
    }; // Only owned outgoing roots escape this callback's temporary scope.
    context.check_budget()?;
    reply.write(&mut caller, outputs)
}
