//! WeakMap and WeakSet entries: collector ephemerons behind raw host calls.
//!
//! These imports take their references unrooted. A lookup allocates nothing
//! and cannot collect, and a store only registers a collector edge, so the
//! raw references compiled code passes stay valid for the whole call; no
//! operand is rooted, type-checked again or upgraded.
use super::*;

use super::collection_index::{Entries, Operands};
use std::mem::MaybeUninit;
use wasmtime::{FuncType, ValRaw, ValType};

type Arguments = [MaybeUninit<ValRaw>];

/// The collection and key in the first two parameters and their identities
/// in the two starting at `identities`.
///
/// # Safety
///
/// The import's type gives those four parameters these types.
unsafe fn operands(arguments: &Arguments, identities: usize) -> wasmtime::Result<Operands> {
    // SAFETY: Wasmtime initialized every parameter of the import's type.
    unsafe {
        Operands::new(
            arguments[0].assume_init().get_anyref(),
            arguments[1].assume_init().get_anyref(),
            arguments[identities].assume_init().get_i64(),
            arguments[identities + 1].assume_init().get_i64(),
        )
    }
}

/// Defines `row` with exactly the type the emitter declares for it, which
/// instantiation checks against the module's import.
///
/// # Safety
///
/// `operation` reads its parameters and writes its results as `parameters`
/// and `results` type them.
unsafe fn define(
    linker: &mut FixedHostFunctions<'_, '_>,
    row: GcHostImport,
    parameters: &[ValType],
    results: &[ValType],
    operation: impl Fn(WasmtimeCaller<'_, WasmHostState>, &mut Arguments) -> wasmtime::Result<()>
        + Send
        + Sync
        + 'static,
) -> Result<(), EngineError> {
    if !linker.includes(row.name()) {
        return Ok(());
    }
    let linker = &mut *linker.linker;
    let ty = FuncType::new(
        linker.engine(),
        parameters.iter().cloned(),
        results.iter().cloned(),
    );
    // SAFETY: this function's contract.
    unsafe { linker.func_new_unchecked(row.module(), row.name(), ty, operation) }
        .map(drop)
        .map_err(bind_error)
}

/// `(owner, key, value, owner identity, key identity)`: stores the value, or
/// with a null value a WeakSet's membership.
fn put(
    mut caller: WasmtimeCaller<'_, WasmHostState>,
    arguments: &mut Arguments,
) -> wasmtime::Result<()> {
    // SAFETY: the import's type, and references passed by the compiled code
    // that is calling.
    unsafe {
        let at = operands(arguments, 3)?;
        let value = arguments[2].assume_init().get_anyref();
        Entries::put(&mut caller, at, value)
    }
}

/// `(owner, key, owner identity, key identity) -> value`, null without an
/// entry.
fn get(
    mut caller: WasmtimeCaller<'_, WasmHostState>,
    arguments: &mut Arguments,
) -> wasmtime::Result<()> {
    // SAFETY: the import's type.
    let at = unsafe { operands(arguments, 2)? };
    let value = Entries::get(&mut caller, at)?;
    arguments[0].write(ValRaw::anyref(value));
    Ok(())
}

/// `(owner, key, owner identity, key identity) -> i32`: whether the entry
/// exists.
fn has(
    caller: WasmtimeCaller<'_, WasmHostState>,
    arguments: &mut Arguments,
) -> wasmtime::Result<()> {
    // SAFETY: the import's type.
    let at = unsafe { operands(arguments, 2)? };
    let present = caller.data().weak.index.find(&caller, at)?.is_some();
    arguments[0].write(ValRaw::i32(i32::from(present)));
    Ok(())
}

/// `(owner, key, owner identity, key identity) -> i32`: removes the entry
/// and says whether it existed.
fn delete(
    mut caller: WasmtimeCaller<'_, WasmHostState>,
    arguments: &mut Arguments,
) -> wasmtime::Result<()> {
    // SAFETY: the import's type.
    let at = unsafe { operands(arguments, 2)? };
    let entry = caller.data().weak.index.find(&caller, at)?;
    if let Some(entry) = entry {
        // The index keeps the retired handle until it next rebuilds; it can
        // match nothing.
        entry.remove(&mut caller)?;
    }
    arguments[0].write(ValRaw::i32(i32::from(entry.is_some())));
    Ok(())
}

pub(super) fn link(linker: &mut FixedHostFunctions<'_, '_>) -> Result<(), EngineError> {
    const EQ: ValType = ValType::EQREF;
    const LOOKUP: [ValType; 4] = [EQ, EQ, ValType::I64, ValType::I64];
    // SAFETY: each operation reads the parameters and writes the result its
    // documentation lists, which are the types given here.
    unsafe {
        define(
            linker,
            GcHostImport::WeakEntryPut,
            &[EQ, EQ, EQ, ValType::I64, ValType::I64],
            &[],
            put,
        )?;
        define(linker, GcHostImport::WeakEntryGet, &LOOKUP, &[EQ], get)?;
        define(
            linker,
            GcHostImport::WeakEntryHas,
            &LOOKUP,
            &[ValType::I32],
            has,
        )?;
        define(
            linker,
            GcHostImport::WeakEntryDelete,
            &LOOKUP,
            &[ValType::I32],
            delete,
        )
    }
}
