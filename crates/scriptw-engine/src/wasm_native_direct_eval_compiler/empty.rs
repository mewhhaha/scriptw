//! Execute only the source-independent code admitted by the empty IR proof.
use super::*;
use crate::wasm_native_source_compiler::CallbackTrace;
use wasmtime::{AsContextMut, ValType};

pub(super) fn execute(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    engine: &WasmtimeEngine,
    context: &NativeSourceCompilerContext,
    proof: &scriptw_ir::EmptyRuntimeDirectEval<'_>,
    signature: &NativeDirectEvalSignature,
    trace: Option<&CallbackTrace<'_>>,
) -> wasmtime::Result<[Val; 6]> {
    let runtime = context.validated_runtime()?;
    let template = context.empty_direct_eval_template.get_or_init(|| {
        scriptw_aot_wasm::NativeProgramTemplate::for_empty_direct_eval(proof, runtime)
    });
    let instance = crate::wasm_native_program_template::instantiate(
        &mut *store,
        engine,
        context,
        template,
        trace,
    )?;
    let factory = instance
        .get_func(
            &mut *store,
            template
                .empty_direct_eval_export()
                .ok_or_else(|| abi_error("empty kernel has no factory"))?,
        )
        .ok_or_else(|| abi_error("empty direct eval has no declared Wasm factory"))?;
    let ty = factory.ty(&*store);
    let results: Vec<_> = ty.results().collect();
    if ty.params().len() != 0
        || results.len() != 6
        || !results[..5]
            .iter()
            .zip(&signature.completion)
            .all(|(a, b)| ValType::eq(a, b))
        || !matches!(results[5], ValType::I32)
    {
        return Err(abi_error(
            "empty kernel differs from actual Completion/status types",
        ));
    }
    context.check_budget()?;
    let _invoke = trace.map(|trace| trace.phase(Phase::FactoryInvoke));
    let mut output = [Val::I32(0); 6];
    factory.call(&mut *store, &[], &mut output)?;
    context.check_budget()?;
    Ok(output)
}
