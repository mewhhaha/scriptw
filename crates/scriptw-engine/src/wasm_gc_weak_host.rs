//! Per-Store collector handles and kept roots, without a second JS heap.
use super::*;
use crate::host_imports::FixedHostFunctions;
use scriptw_aot_wasm::GcHostImport;
use wasmtime::{AnyRef, EqRef, ExternRef, GcEphemeron, OwnedRooted, Rooted, WeakAnyRef};

mod collection_index;
mod collections;
mod finalization;
mod references;
mod reporting;

#[derive(Default)]
pub(super) struct WeakGcState {
    index: collection_index::Entries,
    kept: Vec<OwnedRooted<AnyRef>>,
    registries: Vec<WeakAnyRef>,
    poll_cursor: usize,
    last_swept_weak_retire_count: u64,
    sweeping_weak_retire_count: u64,
    finalization_dirty: bool,
}

fn required(reference: Option<Rooted<EqRef>>) -> wasmtime::Result<Rooted<AnyRef>> {
    reference
        .map(|r| r.to_anyref())
        .ok_or_else(|| wasmtime::Error::msg("required weak GC operand is null"))
}

fn resource<T: Send + Sync + 'static>(
    caller: &WasmtimeCaller<'_, WasmHostState>,
    reference: Rooted<ExternRef>,
) -> wasmtime::Result<Arc<T>> {
    reference
        .data(caller)?
        .and_then(|data| data.downcast_ref::<Arc<T>>())
        .cloned()
        .ok_or_else(|| wasmtime::Error::msg("weak resource has the wrong native brand"))
}

fn bind_error(error: wasmtime::Error) -> EngineError {
    EngineError::new(format!("wasmtime weak host linker setup failed: {error}"))
}

fn keep(
    caller: &mut WasmtimeCaller<'_, WasmHostState>,
    reference: Rooted<AnyRef>,
) -> wasmtime::Result<()> {
    for kept in &caller.data().weak.kept {
        if Rooted::ref_eq(&*caller, kept, &reference)? {
            return Ok(());
        }
    }
    caller
        .data_mut()
        .weak
        .kept
        .try_reserve(1)
        .map_err(wasmtime::Error::new)?;
    let root = reference.to_owned_rooted(&mut *caller)?;
    caller.data_mut().weak.kept.push(root);
    Ok(())
}

pub(super) fn link(linker: &mut FixedHostFunctions<'_, '_>) -> Result<(), EngineError> {
    collections::link(linker)?;
    references::link(linker)?;
    finalization::link(linker)?;
    reporting::link(linker)
}
