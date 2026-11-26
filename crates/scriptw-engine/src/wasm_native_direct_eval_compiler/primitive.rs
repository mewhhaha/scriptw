//! Proven literals execute as actual Wasm; each call supplies its own operands.
use super::*;
use crate::wasm_native_source_compiler::CallbackTrace;
use scriptw_aot_wasm::WasmRuntimeValueTag;
use scriptw_ir::RuntimeDirectEvalPrimitive;
use wasmtime::{AsContextMut, ValType};

pub(super) fn execute(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    engine: &WasmtimeEngine,
    context: &NativeSourceCompilerContext,
    proof: &scriptw_ir::PrimitiveRuntimeDirectEval<'_>,
    signature: &NativeDirectEvalSignature,
    trace: Option<&CallbackTrace<'_>>,
) -> wasmtime::Result<[Val; 6]> {
    let runtime = context.validated_runtime()?;
    let template = context.primitive_direct_eval_template.get_or_init(|| {
        scriptw_aot_wasm::NativeProgramTemplate::for_primitive_direct_eval(proof, runtime)
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
                .primitive_direct_eval_export()
                .ok_or_else(|| abi_error("primitive kernel has no factory"))?,
        )
        .ok_or_else(|| abi_error("primitive eval has no declared Wasm factory"))?;
    let ty = factory.ty(&*store);
    let parameters = ty.params().collect::<Vec<_>>();
    let results = ty.results().collect::<Vec<_>>();
    if !matches!(parameters.as_slice(), [ValType::I32, ValType::I64])
        || results.len() != 6
        || !results[..5]
            .iter()
            .zip(&signature.completion)
            .all(|(actual, expected)| ValType::eq(actual, expected))
        || !matches!(results[5], ValType::I32)
    {
        return Err(abi_error(
            "primitive kernel differs from actual Completion/status types",
        ));
    }
    let (tag, bits) = match proof.value() {
        RuntimeDirectEvalPrimitive::Undefined => (WasmRuntimeValueTag::Undefined, 0),
        RuntimeDirectEvalPrimitive::Null => (WasmRuntimeValueTag::Null, 0),
        RuntimeDirectEvalPrimitive::Boolean(value) => {
            (WasmRuntimeValueTag::Boolean, u64::from(value))
        }
        RuntimeDirectEvalPrimitive::Number(bits) => (WasmRuntimeValueTag::Number, bits),
    };
    let arguments = [Val::I32(tag.tag()), Val::I64(bits as i64)];
    context.check_budget()?;
    let _invoke = trace.map(|trace| trace.phase(Phase::FactoryInvoke));
    let mut output = [Val::I32(0); 6];
    factory.call(&mut *store, &arguments, &mut output)?;
    context.check_budget()?;
    Ok(output)
}
