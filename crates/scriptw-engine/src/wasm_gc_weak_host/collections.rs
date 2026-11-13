use super::*;

use super::collection_index::Entries;
use std::sync::MutexGuard;

struct WeakMapResource(Mutex<Entries>);
struct WeakSetResource(Mutex<Entries>);

fn entries(table: &Mutex<Entries>) -> MutexGuard<'_, Entries> {
    table.lock().unwrap_or_else(|e| e.into_inner())
}

/// Lookups borrow the resource from the Store instead of cloning its `Arc`:
/// they run entirely under a shared Store borrow.
fn borrowed_resource<'a, T: Send + Sync + 'static>(
    caller: &'a WasmtimeCaller<'_, WasmHostState>,
    reference: &Rooted<ExternRef>,
) -> wasmtime::Result<&'a T> {
    reference
        .data(caller)?
        .and_then(|data| data.downcast_ref::<Arc<T>>())
        .map(|resource| &**resource)
        .ok_or_else(|| wasmtime::Error::msg("weak resource has the wrong native brand"))
}

/// Shared membership for WeakMap and WeakSet `has` and `delete`.
fn membership<T: Send + Sync + 'static>(
    mut caller: WasmtimeCaller<'_, WasmHostState>,
    native: Rooted<ExternRef>,
    key: Option<Rooted<EqRef>>,
    table: fn(&T) -> &Mutex<Entries>,
    delete: bool,
) -> wasmtime::Result<i32> {
    if !delete {
        let resource = borrowed_resource::<T>(&caller, &native)?;
        let found = entries(table(resource)).lookup(&caller, &required(key)?)?;
        return Ok(i32::from(found.entry.is_some()));
    }
    let resource = resource::<T>(&caller, native)?;
    let mut guard = entries(table(&resource));
    let found = guard.lookup(&caller, &required(key)?)?;
    let present = found.entry.is_some();
    guard.remove(&mut caller, found)?;
    Ok(i32::from(present))
}

pub(super) fn link(linker: &mut WasmtimeLinker<WasmHostState>) -> Result<(), EngineError> {
    let row = GcHostImport::WeakMapAllocate;
    linker
        .func_wrap(
            row.module(),
            row.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>| -> wasmtime::Result<Rooted<ExternRef>> {
                ExternRef::new(
                    &mut caller,
                    Arc::new(WeakMapResource(Mutex::new(Entries::default()))),
                )
            },
        )
        .map_err(bind_error)?;
    let row = GcHostImport::WeakSetAllocate;
    linker
        .func_wrap(
            row.module(),
            row.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>| -> wasmtime::Result<Rooted<ExternRef>> {
                ExternRef::new(
                    &mut caller,
                    Arc::new(WeakSetResource(Mutex::new(Entries::default()))),
                )
            },
        )
        .map_err(bind_error)?;
    let row = GcHostImport::WeakMapSet;
    linker
        .func_wrap(
            row.module(),
            row.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>,
             owner: Option<Rooted<EqRef>>,
             native: Rooted<ExternRef>,
             key: Option<Rooted<EqRef>>,
             value: Option<Rooted<EqRef>>|
             -> wasmtime::Result<()> {
                let resource = resource::<WeakMapResource>(&caller, native)?;
                let mut guard = entries(&resource.0);
                guard.insert(
                    &mut caller,
                    &required(owner)?,
                    &required(key)?,
                    Some(&required(value)?),
                )
            },
        )
        .map_err(bind_error)?;
    let row = GcHostImport::WeakSetAdd;
    linker
        .func_wrap(
            row.module(),
            row.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>,
             owner: Option<Rooted<EqRef>>,
             native: Rooted<ExternRef>,
             key: Option<Rooted<EqRef>>|
             -> wasmtime::Result<()> {
                let resource = resource::<WeakSetResource>(&caller, native)?;
                let mut guard = entries(&resource.0);
                guard.insert(&mut caller, &required(owner)?, &required(key)?, None)
            },
        )
        .map_err(bind_error)?;
    let row = GcHostImport::WeakMapGet;
    linker
        .func_wrap(
            row.module(),
            row.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>,
             native: Rooted<ExternRef>,
             key: Option<Rooted<EqRef>>|
             -> wasmtime::Result<Option<Rooted<EqRef>>> {
                let found = {
                    let resource = borrowed_resource::<WeakMapResource>(&caller, &native)?;
                    entries(&resource.0).lookup(&caller, &required(key)?)?.entry
                };
                let Some(entry) = found else {
                    return Ok(None);
                };
                entry
                    .value(&mut caller)?
                    .map(|v| v.as_eqref(&mut caller))
                    .transpose()
                    .map(Option::flatten)
            },
        )
        .map_err(bind_error)?;
    for (row, delete) in [
        (GcHostImport::WeakMapHas, false),
        (GcHostImport::WeakMapDelete, true),
    ] {
        linker
            .func_wrap(
                row.module(),
                row.name(),
                move |caller: WasmtimeCaller<'_, WasmHostState>,
                      native: Rooted<ExternRef>,
                      key: Option<Rooted<EqRef>>|
                      -> wasmtime::Result<i32> {
                    membership(caller, native, key, |r: &WeakMapResource| &r.0, delete)
                },
            )
            .map_err(bind_error)?;
    }
    for (row, delete) in [
        (GcHostImport::WeakSetHas, false),
        (GcHostImport::WeakSetDelete, true),
    ] {
        linker
            .func_wrap(
                row.module(),
                row.name(),
                move |caller: WasmtimeCaller<'_, WasmHostState>,
                      native: Rooted<ExternRef>,
                      key: Option<Rooted<EqRef>>|
                      -> wasmtime::Result<i32> {
                    membership(caller, native, key, |r: &WeakSetResource| &r.0, delete)
                },
            )
            .map_err(bind_error)?;
    }
    Ok(())
}
