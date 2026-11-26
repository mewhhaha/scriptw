//! Native compilation of already-converted Function source Strings in one Store.
//! Entry/ABI and AOT factory consumers are required before this can be merged.

use super::wasm_gc_host::TypedGcHostImport;
use super::*;
use scriptw_aot_wasm::GcHostImport;
use scriptw_ir::{lower_runtime_function, DynamicFunctionKind};
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
pub(super) use observer::{NativeFunctionCompilationObserver, NativeFunctionCompilationSnapshot};
use outcome::{CompileRejection, EscapedReply};
use signature::NativeFunctionSignature;
use sources::ConvertedRequest;

// The Function and global Script callbacks consume the same armed execution authority.
use crate::wasm_native_source_compiler::{Family, NativeSourceCompilerContext, Phase};

fn abi_error(message: impl std::fmt::Display) -> wasmtime::Error {
    wasmtime::Error::new(EngineError::new(format!(
        "invalid native Function compiler ABI: {message}"
    )))
}

pub(super) fn link(
    linker: &mut WasmtimeLinker<WasmHostState>,
    import: TypedGcHostImport<'_>,
) -> Result<(), EngineError> {
    let row = GcHostImport::CompileDynamicFunction;
    let Some(function) = import.into_func(
        "duplicate native Function compiler import",
        "native Function compiler import is not a function",
    )?
    else {
        return Ok(());
    };
    let signature = NativeFunctionSignature::validate(function).map_err(|error| {
        EngineError::new(format!("native Function compiler signature: {error:#}"))
    })?;
    linker
        .func_new(
            row.module(),
            row.name(),
            signature.function.clone(),
            move |caller, inputs, outputs| compile(caller, inputs, outputs, &signature),
        )
        .map_err(|error| {
            EngineError::new(format!("native Function compiler linkage: {error:#}"))
        })?;
    Ok(())
}

fn compile(
    mut caller: WasmtimeCaller<'_, WasmHostState>,
    inputs: &[Val],
    outputs: &mut [Val],
    signature: &NativeFunctionSignature,
) -> wasmtime::Result<()> {
    let context = {
        caller
            .data()
            .native_source_compiler
            .clone()
            .ok_or_else(|| abi_error("linked execution has no armed native compiler context"))?
    }; // No State borrow crosses decoding, compilation or a Wasm invocation.
    if !WasmtimeEngine::same(caller.engine(), context.linker.engine()) {
        return Err(abi_error("compiler linker belongs to another Engine"));
    }
    context.check_budget()?;
    #[cfg(test)]
    if let Some(observer) = &context.function_test_observer {
        observer.request();
    }
    let trace = context.begin_trace(Family::Function);
    let engine = caller.engine().clone();
    let reply = {
        let mut roots = RootScope::new(&mut caller);
        let decode = trace.as_ref().map(|trace| trace.phase(Phase::Decode));
        let request = ConvertedRequest::decode(&mut roots, inputs, signature, context.execution())?;
        drop(decode);
        context.check_budget()?;
        let parse_lower = trace.as_ref().map(|trace| trace.phase(Phase::ParseLower));
        let lowered = lower_runtime_function(
            request.kind,
            &request.arguments,
            context.policy.host_surface_policy(),
        )
        .map(|program| program.with_language_features(context.policy.language_features()));
        drop(parse_lower);
        context.check_budget()?;
        match lowered {
            Ok(program) => {
                let completion = factory::instantiate_and_create(
                    &mut roots,
                    &engine,
                    &context,
                    &request,
                    &program,
                    signature,
                    trace.as_ref(),
                )?;
                let reply = EscapedReply::completion(&mut roots, completion)?;
                #[cfg(test)]
                if let Some(observer) = &context.function_test_observer {
                    observer.completed_factory();
                }
                reply
            }
            Err(error) => match CompileRejection::classify(error)? {
                CompileRejection::Syntax(message) => {
                    let reply = EscapedReply::syntax(&mut roots, &message, signature)?;
                    context.check_budget()?;
                    #[cfg(test)]
                    if let Some(observer) = &context.function_test_observer {
                        observer.syntax_rejection();
                    }
                    reply
                }
            },
        }
    }; // Only the actual outgoing reference roots escape the temporary scope.
    context.check_budget()?;
    reply.write(&mut caller, outputs)
}
