use super::*;

struct FinalizationCellResource {
    target: WeakAnyRef,
    token: Option<WeakAnyRef>,
}

pub(super) fn link(linker: &mut FixedHostFunctions<'_, '_>) -> Result<(), EngineError> {
    let row = GcHostImport::FinalizationTrack;
    linker
        .func_wrap(
            row.module(),
            row.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>,
             registry: Option<Rooted<EqRef>>|
             -> wasmtime::Result<()> {
                let registry = required(registry)?;
                caller
                    .data_mut()
                    .weak
                    .registries
                    .try_reserve(1)
                    .map_err(wasmtime::Error::new)?;
                let weak = WeakAnyRef::new(&mut caller, &registry, &registry)?;
                caller.data_mut().weak.registries.push(weak);
                Ok(())
            },
        )
        .map_err(bind_error)?;
    let row = GcHostImport::FinalizationNextRegistry;
    linker.func_wrap(row.module(), row.name(), |mut caller: WasmtimeCaller<'_, WasmHostState>| -> wasmtime::Result<Option<Rooted<EqRef>>> {
        loop {
            let cursor = caller.data().weak.poll_cursor;
            let Some(weak) = caller.data().weak.registries.get(cursor).copied() else { return Ok(None); };
            if let Some(registry) = weak.upgrade(&mut caller)? {
                caller.data_mut().weak.poll_cursor += 1;
                return registry.as_eqref(&mut caller);
            }
            caller.data_mut().weak.registries.swap_remove(cursor);
        }
    }).map_err(bind_error)?;
    let row = GcHostImport::FinalizationCellAllocate;
    linker
        .func_wrap(
            row.module(),
            row.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>,
             owner: Option<Rooted<EqRef>>,
             target: Option<Rooted<EqRef>>,
             token: Option<Rooted<EqRef>>|
             -> wasmtime::Result<Rooted<ExternRef>> {
                let owner = required(owner)?;
                let target = required(target)?;
                let target = WeakAnyRef::new(&mut caller, &owner, &target)?;
                let token = match token {
                    Some(token) => match WeakAnyRef::new(&mut caller, &owner, &token.to_anyref()) {
                        Ok(token) => Some(token),
                        Err(error) => {
                            target.remove(&mut caller)?;
                            return Err(error);
                        }
                    },
                    None => None,
                };
                ExternRef::new(
                    &mut caller,
                    Arc::new(FinalizationCellResource { target, token }),
                )
            },
        )
        .map_err(bind_error)?;
    let row = GcHostImport::FinalizationTargetAlive;
    linker
        .func_wrap(
            row.module(),
            row.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>,
             native: Rooted<ExternRef>|
             -> wasmtime::Result<i32> {
                let resource = resource::<FinalizationCellResource>(&caller, native)?;
                Ok(i32::from(resource.target.upgrade(&mut caller)?.is_some()))
            },
        )
        .map_err(bind_error)?;
    let row = GcHostImport::FinalizationTokenMatches;
    linker
        .func_wrap(
            row.module(),
            row.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>,
             native: Rooted<ExternRef>,
             token: Option<Rooted<EqRef>>|
             -> wasmtime::Result<i32> {
                let resource = resource::<FinalizationCellResource>(&caller, native)?;
                let Some(weak) = resource.token else {
                    return Ok(0);
                };
                let Some(actual) = weak.upgrade(&mut caller)? else {
                    return Ok(0);
                };
                Ok(i32::from(Rooted::ref_eq(
                    &caller,
                    &actual,
                    &required(token)?,
                )?))
            },
        )
        .map_err(bind_error)?;
    let row = GcHostImport::FinalizationCellRemove;
    linker
        .func_wrap(
            row.module(),
            row.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>,
             native: Rooted<ExternRef>|
             -> wasmtime::Result<()> {
                let resource = resource::<FinalizationCellResource>(&caller, native)?;
                resource.target.remove(&mut caller)?;
                if let Some(token) = resource.token {
                    token.remove(&mut caller)?;
                }
                Ok(())
            },
        )
        .map_err(bind_error)?;
    Ok(())
}
