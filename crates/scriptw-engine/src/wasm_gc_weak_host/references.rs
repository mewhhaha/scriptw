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
                let retire_count = caller.gc_weak_retire_count();
                let weak = &mut caller.data_mut().weak;
                weak.kept.clear();
                if weak.finalization_dirty || retire_count != weak.last_swept_weak_retire_count {
                    weak.poll_cursor = 0;
                    weak.sweeping_weak_retire_count = retire_count;
                    weak.finalization_dirty = true;
                } else {
                    weak.poll_cursor = weak.registries.len();
                }
            },
        )
        .map_err(bind_error)?;
    Ok(())
}
