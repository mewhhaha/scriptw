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
type LookupArguments = [MaybeUninit<ValRaw>; 4];

#[inline]
fn operands(
    owner: ValRaw,
    key: ValRaw,
    owner_id: ValRaw,
    key_id: ValRaw,
) -> wasmtime::Result<Operands> {
    Operands::new(
        owner.get_anyref(),
        key.get_anyref(),
        owner_id.get_i64(),
        key_id.get_i64(),
    )
}

/// Check the lookup frame once for both the parameters and the result.
#[inline]
fn lookup_arguments(arguments: &mut Arguments) -> &mut LookupArguments {
    arguments
        .first_chunk_mut()
        .expect("weak lookup takes four parameters")
}

/// Read the two references and their identities.
///
/// # Safety
///
/// Wasmtime initialized the parameters of a lookup import.
#[inline]
unsafe fn lookup_operands(arguments: &LookupArguments) -> wasmtime::Result<Operands> {
    // SAFETY: Wasmtime initialized every parameter of the import's type.
    let [owner, key, owner_id, key_id] = arguments.map(|argument| unsafe { argument.assume_init() });
    operands(owner, key, owner_id, key_id)
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
    let arguments = arguments
        .first_chunk::<5>()
        .expect("weak put takes five parameters");
    // SAFETY: Wasmtime initialized every parameter of this import.
    let [owner, key, value, owner_id, key_id] =
        arguments.map(|argument| unsafe { argument.assume_init() });
    let at = operands(owner, key, owner_id, key_id)?;
    // SAFETY: compiled code passed these raw operands to this host call.
    unsafe { Entries::put(&mut caller, at, value.get_anyref()) }
}

/// `(owner, key, owner identity, key identity) -> value`, null without an
/// entry.
fn get(
    mut caller: WasmtimeCaller<'_, WasmHostState>,
    arguments: &mut Arguments,
) -> wasmtime::Result<()> {
    let arguments = lookup_arguments(arguments);
    // SAFETY: the import's type.
    let at = unsafe { lookup_operands(arguments)? };
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
    let arguments = lookup_arguments(arguments);
    // SAFETY: the import's type.
    let at = unsafe { lookup_operands(arguments)? };
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
    let arguments = lookup_arguments(arguments);
    // SAFETY: the import's type.
    let at = unsafe { lookup_operands(arguments)? };
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
