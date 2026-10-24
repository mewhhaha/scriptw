//! The encoded shared kernel bounds leaf allocation even for dormant sites.
use super::*;
use crate::gc_types::{GcNullability, GlobalLedger, OwnDataReadCacheTable, RuntimeModuleTypes};

#[test]
fn cached_read_allocates_fixed_pages_and_indexes_the_directory_by_page() {
    let artifact = emit_script("function read(object){return object.value;} read({value:1});")
        .expect("paged cache control emits");
    expect_valid_module(&artifact, 1);
    let runtime = artifact.runtime().unwrap();
    let target = function_names_in_bytes(runtime.bytes())
        .into_iter()
        .find_map(|(index, name)| (name == "helper::cached_property_read").then_some(index))
        .unwrap();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Standalone)
        .finalize_globals(GlobalLedger::new(), false, 0);
    let wasm_encoder::HeapType::Concrete(leaf) = registered
        .globals()
        .schema()
        .reference_type::<OwnDataReadCacheTable>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("cache leaf is a declared GC array")
    };
    let mut index = 0;
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        match payload.unwrap() {
            Payload::ImportSection(imports) => {
                index += imports
                    .into_imports()
                    .map(Result::unwrap)
                    .filter(|import| {
                        matches!(
                            import.ty,
                            wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                        )
                    })
                    .count() as u32;
            }
            Payload::CodeSectionEntry(body) => {
                if index == target {
                    let mut previous_constant = None;
                    let mut previous_local = None;
                    let mut constants = BTreeMap::new();
                    let mut leaf_allocations = 0;
                    let mut shifts = 0;
                    let mut masks = 0;
                    for operator in body.get_operators_reader().unwrap() {
                        let operator = operator.unwrap();
                        match operator {
                            Operator::LocalSet { local_index } => {
                                if let Some(value) = previous_constant {
                                    constants.insert(local_index, value);
                                } else {
                                    constants.remove(&local_index);
                                }
                            }
                            Operator::ArrayNew { array_type_index } if array_type_index == leaf => {
                                leaf_allocations += 1;
                                assert_eq!(
                                    previous_local.and_then(|local| constants.get(&local).copied()),
                                    Some(64)
                                );
                            }
                            Operator::I32ShrU if previous_constant == Some(6) => shifts += 1,
                            Operator::I32And if previous_constant == Some(63) => masks += 1,
                            _ => {}
                        }
                        previous_constant = match operator {
                            Operator::I32Const { value } => Some(value),
                            _ => None,
                        };
                        previous_local = match operator {
                            Operator::LocalGet { local_index } => Some(local_index),
                            _ => None,
                        };
                    }
                    assert_eq!((leaf_allocations, shifts, masks), (1, 1, 1));
                    return;
                }
                index += 1;
            }
            _ => {}
        }
    }
    panic!("shared cache kernel is absent");
}
