//! ArrayBuffer backing allocation preserves the real collector's OOM result.

use super::wasm_gc_host::TypedGcHostImport;
use super::*;
use scriptw_aot_wasm::{GcHostImport, GcHostLayout};
use wasmtime::{ArrayRef, ArrayRefPre, ArrayType, FuncType, GcHeapOutOfMemory, Mutability, Val, ValType};

/// Use the import's canonical type rather than creating a parallel GC layout.
struct ByteArrayAllocationSignature {
    function: FuncType,
    array: ArrayType,
}

impl ByteArrayAllocationSignature {
    fn validate(function: FuncType) -> wasmtime::Result<Self> {
        let result = {
            let mut params = function.params();
            let mut results = function.results();
            let (Some(ValType::I32), None, Some(result), None) =
                (params.next(), params.next(), results.next(), results.next())
            else {
                return Err(wasmtime::Error::msg(
                    "GC byte allocation requires one I32 length and one result",
                ));
            };
            result
        };
        let result = result
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("GC byte allocation result must be a reference"))?;
        if !result.is_nullable() {
            return Err(wasmtime::Error::msg(
                "GC byte allocation result must admit OOM null",
            ));
        }
        let array = result.heap_type().as_concrete_array().ok_or_else(|| {
            wasmtime::Error::msg("GC byte allocation result must be a concrete array")
        })?;
        let layout = GcHostLayout::ByteArray;
        let storage = layout
            .array_element()
            .ok_or_else(|| wasmtime::Error::msg("GC byte allocation schema must be an array"))?;
        if layout.array_mutable() != Some(array.mutability() == Mutability::Var)
            || !wasm_gc_completion::storage_matches(storage, &array.element_type())
        {
            return Err(wasmtime::Error::msg(
                "GC byte allocation result disagrees with the emitted byte schema",
            ));
        }
        let array = array.clone();
        Ok(Self { function, array })
    }
}

pub(super) fn link(
    linker: &mut WasmtimeLinker<WasmHostState>,
    import: TypedGcHostImport<'_>,
) -> Result<(), EngineError> {
    let row = GcHostImport::ByteArrayAllocate;
    let Some(function) = import.into_func(
        "duplicate GC byte allocation import",
        "GC byte allocation import is not a function",
    )?
    else {
        return Ok(());
    };
    let signature = ByteArrayAllocationSignature::validate(function)
        .map_err(|error| EngineError::new(format!("invalid GC byte allocation import: {error}")))?;
    linker
        .func_new(
            row.module(),
            row.name(),
            signature.function,
            move |mut caller: WasmtimeCaller<'_, WasmHostState>,
                  inputs: &[Val],
                  outputs: &mut [Val]| {
                let ([Val::I32(length)], [output]) = (inputs, outputs) else {
                    return Err(wasmtime::Error::msg(
                        "GC byte allocation callback has invalid operands",
                    ));
                };
                let length = u32::try_from(*length)
                    .map_err(|_| wasmtime::Error::msg("GC byte allocation length is negative"))?;
                let allocator = ArrayRefPre::new(&mut caller, signature.array.clone());
                *output = match ArrayRef::new(&mut caller, &allocator, &Val::I32(0), length) {
                    Ok(bytes) => Val::AnyRef(Some(bytes.to_anyref())),
                    Err(error) if error.is::<GcHeapOutOfMemory<()>>() => Val::AnyRef(None),
                    Err(error) => return Err(error),
                };
                Ok(())
            },
        )
        .map_err(|error| {
            EngineError::new(format!("GC byte allocation linker setup failed: {error}"))
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmtime::{FieldType, HeapType, RefType, StorageType};

    #[test]
    fn byte_allocation_admits_the_canonical_schema_and_rejects_abi_lookalikes() {
        let engine = shared_wasm_engine().expect("required native GC target");
        let array = ArrayType::new(&engine, FieldType::new(Mutability::Var, StorageType::I8));
        let reference = ValType::Ref(RefType::new(true, HeapType::ConcreteArray(array.clone())));
        let signature = ByteArrayAllocationSignature::validate(FuncType::new(
            &engine,
            [ValType::I32],
            [reference.clone()],
        ))
        .expect("exact nullable byte-array result");
        assert!(signature.array.matches(&array) && array.matches(&signature.array));
        let nonnullable = ValType::Ref(RefType::new(false, HeapType::ConcreteArray(array)));
        let wide = ArrayType::new(&engine, FieldType::new(Mutability::Var, StorageType::I16));
        let immutable = ArrayType::new(&engine, FieldType::new(Mutability::Const, StorageType::I8));
        for (label, params, results) in [
            ("missing length", vec![], vec![reference.clone()]),
            (
                "extra length",
                vec![ValType::I32, ValType::I32],
                vec![reference.clone()],
            ),
            (
                "wrong length width",
                vec![ValType::I64],
                vec![reference.clone()],
            ),
            ("missing result", vec![ValType::I32], vec![]),
            (
                "extra result",
                vec![ValType::I32],
                vec![reference.clone(), reference],
            ),
            ("scalar result", vec![ValType::I32], vec![ValType::I32]),
            ("null forbidden", vec![ValType::I32], vec![nonnullable]),
            (
                "wide elements",
                vec![ValType::I32],
                vec![ValType::Ref(RefType::new(
                    true,
                    HeapType::ConcreteArray(wide),
                ))],
            ),
            (
                "immutable bytes",
                vec![ValType::I32],
                vec![ValType::Ref(RefType::new(
                    true,
                    HeapType::ConcreteArray(immutable),
                ))],
            ),
        ] {
            assert!(
                ByteArrayAllocationSignature::validate(FuncType::new(&engine, params, results))
                    .is_err(),
                "{label} must fail admission before allocating or exposing a root"
            );
        }
    }
}
