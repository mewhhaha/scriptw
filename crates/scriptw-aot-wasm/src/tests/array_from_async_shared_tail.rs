//! Real R code must share commit/drive across all fulfilled continuation stages.
use super::*;
use crate::gc_types::{
    ArrayFromAsyncStage, ArrayFromAsyncState, GcI32Constant, GcNullability, GlobalLedger,
    RuntimeModuleTypes,
};

#[test]
fn fulfilled_array_from_async_stages_share_one_physical_commit_and_drive() {
    let artifact = emit_script("Array.fromAsync({0:1,length:1},function(x){return x;});").unwrap();
    expect_valid_module(&artifact, 1);
    let runtime = artifact.runtime().unwrap();
    let bytes = runtime.bytes();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let wasm_encoder::HeapType::Concrete(state_type) = registered
        .globals()
        .schema()
        .reference_type::<ArrayFromAsyncState>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("actual continuation state is a registered concrete GC type");
    };
    let mut types = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::TypeSection(groups) = payload.unwrap() {
            for group in groups {
                types.extend(group.unwrap().into_types());
            }
        }
    }
    let wasmparser::CompositeInnerType::Struct(shape) =
        &types[state_type as usize].composite_type.inner
    else {
        panic!("continuation state must have its physical struct shape");
    };
    // Identify the mutable counter/stage from encoded field types, rather than
    // copying semantic field ordinals into this physical-code control.
    let field = |value_type| {
        let fields: Vec<_> = shape
            .fields
            .iter()
            .enumerate()
            .filter_map(|(index, field)| {
                (field.mutable && field.element_type == wasmparser::StorageType::Val(value_type))
                    .then_some(index as u32)
            })
            .collect();
        assert_eq!(fields.len(), 1, "unique mutable continuation scalar field");
        fields[0]
    };
    let index_field = field(wasmparser::ValType::I64);
    let stage_field = field(wasmparser::ValType::I32);
    let requested = format!(
        "builtin::{}",
        scriptw_ir::StandardBuiltinId::ArrayFromAsyncFulfilled.debug_name()
    );
    let names = function_names_in_bytes(bytes);
    let found: Vec<_> = names
        .iter()
        .filter_map(|(&index, name)| (name == &requested).then_some(index))
        .collect();
    assert_eq!(found.len(), 1, "one actual fulfilled continuation body");
    let mut next = imported_function_count_in_bytes(bytes);
    let mut checked = false;
    for payload in Parser::new(0).parse_all(bytes) {
        let Payload::CodeSectionEntry(body) = payload.unwrap() else {
            continue;
        };
        let index = next;
        next += 1;
        if index != found[0] {
            continue;
        }
        let ops: Vec<_> = body
            .get_operators_reader()
            .unwrap()
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let commits: Vec<_> = ops
            .iter()
            .enumerate()
            .filter_map(|(position, op)| {
                matches!(op,Operator::StructSet {struct_type_index,field_index}
                if *struct_type_index==state_type && *field_index==index_field)
                .then_some(position)
            })
            .collect();
        assert_eq!(
            commits.len(),
            1,
            "all fulfilled routes share one index publication"
        );
        let commit = commits[0];
        assert!(commit >= 7);
        assert!(
            matches!(&ops[commit-7..=commit], [
            Operator::LocalGet{local_index:a},Operator::I64Const{value:1},Operator::I64Add,
            Operator::LocalSet{local_index:b},Operator::LocalGet{..},Operator::RefAsNonNull,Operator::LocalGet{local_index:c},Operator::StructSet{..}
        ] if a==b && b==c),
            "the shared publication increments the same real index once"
        );
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op,
                    Operator::I64Const {
                        value: 9_007_199_254_740_991
                    }
                ))
                .count(),
            1,
            "one actual next-step drive retains the iterator index bound"
        );
        let stages: Vec<_> = ops
            .windows(2)
            .filter_map(|pair| match pair {
                [Operator::StructGet {
                    struct_type_index,
                    field_index,
                }, Operator::LocalSet { local_index }]
                    if *struct_type_index == state_type && *field_index == stage_field =>
                {
                    Some(*local_index)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            stages.len(),
            1,
            "dispatch reads the physical saved stage once"
        );
        let admitted: Vec<_> = ops[..commit].windows(4).filter_map(|window|match window {
            [Operator::LocalGet{local_index},Operator::I32Const{value},Operator::I32Eq,Operator::If{..}]
                if *local_index==stages[0]=>Some(*value),
            _=>None,
        }).collect();
        let expected = [
            ArrayFromAsyncStage::InputValue,
            ArrayFromAsyncStage::MappedValue,
            ArrayFromAsyncStage::AsyncIteratorResult,
            ArrayFromAsyncStage::AsyncCloseResult,
        ]
        .map(GcI32Constant::encode);
        assert_eq!(
            admitted, expected,
            "all original stages remain admitted before the common tail"
        );
        checked = true;
    }
    assert!(checked);
}
