//! The per-site records live in one flat table that a hit indexes directly.
//! Only the miss kernel grows it, at least doubling, so registering every
//! site of a P costs amortized constant work; the hit entry never allocates.
use super::*;
use crate::gc_types::{GcNullability, GlobalLedger, OwnDataReadCacheTable, RuntimeModuleTypes};

#[test]
fn cached_read_grows_one_flat_record_table_and_its_entry_allocates_nothing() {
    let artifact = emit_script("function read(object){return object.value;} read({value:1});")
        .expect("record table control emits");
    expect_valid_module(&artifact, 1);
    let runtime = artifact.runtime().unwrap();
    let names = function_names_in_bytes(runtime.bytes());
    let index_of = |wanted: &str| {
        names
            .iter()
            .find_map(|(index, name)| (name == wanted).then_some(*index))
            .unwrap()
    };
    let entry = index_of("helper::cached_property_read");
    let miss = index_of("helper::cached_property_read_miss");
    let registered = RuntimeModuleTypes::register(crate::tests::standalone_kind())
        .finalize_globals(GlobalLedger::new(), false, 0);
    let wasm_encoder::HeapType::Concrete(table) = registered
        .globals()
        .schema()
        .reference_type::<OwnDataReadCacheTable>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("the record table is a declared GC array")
    };
    let mut index = 0;
    let mut checked = 0;
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
                if index == entry || index == miss {
                    let mut previous_constant = None;
                    let mut table_allocations = 0;
                    let mut other_allocations = 0;
                    let mut doublings = 0;
                    let mut minimum = 0;
                    let mut table_reads = 0;
                    for operator in body.get_operators_reader().unwrap() {
                        let operator = operator.unwrap();
                        match operator {
                            Operator::ArrayNew { array_type_index }
                                if array_type_index == table =>
                            {
                                table_allocations += 1
                            }
                            Operator::ArrayNew { .. }
                            | Operator::ArrayNewDefault { .. }
                            | Operator::ArrayNewFixed { .. }
                            | Operator::StructNew { .. }
                            | Operator::StructNewDefault { .. } => other_allocations += 1,
                            Operator::ArrayGet { array_type_index }
                                if array_type_index == table =>
                            {
                                table_reads += 1
                            }
                            Operator::I32Shl if previous_constant == Some(1) => doublings += 1,
                            Operator::I32Const { value: 64 } => minimum += 1,
                            _ => {}
                        }
                        previous_constant = match operator {
                            Operator::I32Const { value } => Some(value),
                            _ => None,
                        };
                    }
                    if index == entry {
                        assert_eq!(
                            (table_allocations, other_allocations, table_reads),
                            (0, 0, 1),
                            "a hit reads its record with one table read and allocates nothing"
                        );
                    } else {
                        assert_eq!(table_allocations, 1, "only the miss kernel grows the table");
                        assert!(
                            doublings >= 1 && minimum >= 1,
                            "growth doubles from a minimum"
                        );
                    }
                    checked += 1;
                }
                index += 1;
            }
            _ => {}
        }
    }
    assert_eq!(
        checked, 2,
        "the hit entry and the miss kernel are both present"
    );
}
