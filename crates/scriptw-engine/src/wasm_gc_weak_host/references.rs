use super::*;

struct WeakRefResource(WeakAnyRef);

pub(super) fn link(linker: &mut FixedHostFunctions<'_, '_>) -> Result<(), EngineError> {
    let row = GcHostImport::WeakRefAllocate;
    linker
        .func_wrap(
            row.module(),
            row.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>,
             owner: Option<Rooted<EqRef>>,
             target: Option<Rooted<EqRef>>|
             -> wasmtime::Result<Rooted<ExternRef>> {
                let owner = required(owner)?;
                let target = required(target)?;
                let handle = WeakAnyRef::new(&mut caller, &owner, &target)?;
                keep(&mut caller, target)?;
                ExternRef::new(&mut caller, Arc::new(WeakRefResource(handle)))
            },
        )
        .map_err(bind_error)?;
    let row = GcHostImport::WeakRefDeref;
    linker
        .func_wrap(
            row.module(),
            row.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>,
             native: Rooted<ExternRef>|
             -> wasmtime::Result<Option<Rooted<EqRef>>> {
                let resource = resource::<WeakRefResource>(&caller, native)?;
                let Some(target) = resource.0.upgrade(&mut caller)? else {
                    return Ok(None);
                };
                keep(&mut caller, target)?;
                target.as_eqref(&mut caller)
            },
        )
        .map_err(bind_error)?;
    let row = GcHostImport::WeakJobFinish;
    linker
        .func_wrap(
            row.module(),
            row.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>| {
                caller.data_mut().weak.kept.clear();
                caller.data_mut().weak.poll_cursor = 0;
            },
        )
        .map_err(bind_error)?;
    Ok(())
}
