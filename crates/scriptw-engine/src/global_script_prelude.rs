//! Read the declaration-owned P-local phase; no numeric GC offsets or shared R state.
use super::*;
use wasmtime::{AsContextMut, Instance as WasmtimeInstance};

pub(super) fn failed(
    instance: &WasmtimeInstance,
    store: &mut impl AsContextMut<Data = WasmHostState>,
) -> Result<bool, EngineError> {
    let global = instance.get_global(
        &mut *store,
        scriptw_aot_wasm::GLOBAL_SCRIPT_PRELUDE_PHASE_EXPORT,
    );
    failed_global(global, store)
}

pub(super) fn failed_global(
    global: Option<wasmtime::Global>,
    store: &mut impl AsContextMut<Data = WasmHostState>,
) -> Result<bool, EngineError> {
    let Some(global) = global else {
        return Ok(false);
    };
    match global.get(&mut *store) {
        WasmtimeVal::I32(0 | 2) => Ok(false),
        WasmtimeVal::I32(1) => Ok(true),
        _ => Err(EngineError::new(
            "invalid program global Script prelude phase",
        )),
    }
}

pub(super) fn completion_error(
    instance: &WasmtimeInstance,
    store: &mut impl AsContextMut<Data = WasmHostState>,
    completion: GcMainCompletion,
) -> Result<EngineError, EngineError> {
    let (tag, kind, value) = wasm_gc_completion::observe(&mut *store, completion)
        .map_err(EngineError::in_global_prelude)?
        .into_parts();
    let error = match kind {
        WasmTopLevelCompletionKind::Throw => {
            let text = ThrownErrorText::read(instance, &mut *store, tag.value_kind())
                .map_err(EngineError::in_global_prelude)?;
            let note = wasm_gc_completion::render_value(tag, &value, text.message());
            let prefix = text.name_prefix();
            EngineError::from_execution_failure(
                EngineExecutionFailure::JavaScriptException {
                    constructor_name: text.constructor_name,
                },
                format!("uncaught throw: {prefix}{note}"),
            )
        }
        WasmTopLevelCompletionKind::Normal => EngineError::from_execution_failure(
            EngineExecutionFailure::Trap,
            "global Script prelude returned without its completion checkpoint",
        ),
    };
    Ok(error.in_global_prelude())
}

#[cfg(test)]
mod tests;
