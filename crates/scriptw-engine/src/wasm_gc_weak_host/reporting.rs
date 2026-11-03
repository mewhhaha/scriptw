use super::*;

fn text(
    caller: &mut WasmtimeCaller<'_, WasmHostState>,
    reference: Option<Rooted<EqRef>>,
) -> wasmtime::Result<Option<String>> {
    reference
        .map(|r| wasm_gc_completion::string_units(caller, r).map(|s| String::from_utf16_lossy(&s)))
        .transpose()
        .map_err(|e| wasmtime::Error::msg(e.to_string()))
}

pub(super) fn link(linker: &mut WasmtimeLinker<WasmHostState>) -> Result<(), EngineError> {
    let row = GcHostImport::ReportCleanupException;
    linker
        .func_wrap(
            row.module(),
            row.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>,
             tag: i32,
             scalar: i64,
             reference: Option<Rooted<EqRef>>,
             name: Option<Rooted<EqRef>>,
             message: Option<Rooted<EqRef>>|
             -> wasmtime::Result<()> {
                let observed = wasm_gc_completion::observe(
                    &mut caller,
                    (
                        tag,
                        scalar,
                        reference,
                        scriptw_ir::CompletionKindIr::Throw.abi_code() as i32,
                        0,
                    ),
                )
                .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
                let (tag, _, value) = observed.into_parts();
                let name = text(&mut caller, name)?;
                let message = text(&mut caller, message)?;
                let detail = match (name, message) {
                    (Some(name), Some(message)) => Some(format!("{name}: {message}")),
                    (Some(detail), None) | (None, Some(detail)) => Some(detail),
                    (None, None) => None,
                };
                let text = format!(
                    "Uncaught exception: {}",
                    wasm_gc_completion::render_value(tag, &value, detail.as_deref())
                );
                caller.data().output_events.record_exception(&text);
                caller.data().realm.host_hooks().report_exception(&text);
                Ok(())
            },
        )
        .map_err(bind_error)?;
    Ok(())
}
