use super::*;
use wasmtime_environ::{
    copying::CopyingTypeLayouts, EngineOrModuleTypeIndex, GcTypeLayouts, ModuleInternedTypeIndex,
    WasmFieldType, WasmHeapType, WasmRefType, WasmStorageType, WasmStructType, WasmValType,
};

#[test]
fn encoded_actual_array_fits_one_64_byte_copying_collector_record() {
    let (runtime, bytes) = encoded(ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ));
    let (types, _, _) = declarations(&bytes);
    let layouts = runtime.globals().schema().layouts();
    let array_index = layouts.layout_index(GcLayout::ArrayObject);
    let array = &types[array_index as usize];
    assert!(array.is_final);
    assert_eq!(
        array.supertype_idx.map(|index| index.unpack()),
        Some(wasmparser::UnpackedIndex::Module(
            layouts.layout_index(GcLayout::JavaScriptObject),
        )),
        "Array remains its actual JavaScript exotic, separate from OrdinaryObject"
    );
    let CompositeInnerType::Struct(array) = &array.composite_type.inner else {
        panic!("actual Array must be a GC struct");
    };
    // Feed the actual encoded field types to the pinned product collector's
    // layout implementation, retaining concrete heap kinds and nullability.
    let fields = array
        .fields
        .iter()
        .map(|field| {
            let element_type = match field.element_type {
                wasmparser::StorageType::Val(wasmparser::ValType::I32) => {
                    WasmStorageType::Val(WasmValType::I32)
                }
                wasmparser::StorageType::Val(wasmparser::ValType::I64) => {
                    WasmStorageType::Val(WasmValType::I64)
                }
                wasmparser::StorageType::Val(wasmparser::ValType::Ref(reference)) => {
                    let wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(index)) =
                        reference.heap_type()
                    else {
                        panic!("Array fields retain concrete reference types");
                    };
                    let index_type =
                        EngineOrModuleTypeIndex::Module(ModuleInternedTypeIndex::from_u32(index));
                    let heap_type = match &types[index as usize].composite_type.inner {
                        CompositeInnerType::Struct(_) => WasmHeapType::ConcreteStruct(index_type),
                        CompositeInnerType::Array(_) => WasmHeapType::ConcreteArray(index_type),
                        unexpected => panic!("unexpected Array edge type: {unexpected:?}"),
                    };
                    WasmStorageType::Val(WasmValType::Ref(WasmRefType {
                        nullable: reference.is_nullable(),
                        heap_type,
                    }))
                }
                unexpected => panic!("unexpected Array field: {unexpected:?}"),
            };
            WasmFieldType {
                element_type,
                mutable: field.mutable,
            }
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let layout = CopyingTypeLayouts
        .struct_layout(&WasmStructType { fields })
        .unwrap();
    assert_eq!(layout.size, 64);
    assert_eq!(layout.align, 16);
    assert_eq!(
        layout
            .fields
            .iter()
            .map(|field| field.offset)
            .collect::<Vec<_>>(),
        [16, 20, 24, 32, 40, 44, 48, 52, 56, 60],
    );
    assert_eq!(
        layout.fields.iter().filter(|field| field.is_gc_ref).count(),
        6
    );
    assert_eq!(
        112 - layout.size,
        48,
        "the prior 48-byte wrapper plus 64-byte indexed owner are replaced by one actual record"
    );
}
