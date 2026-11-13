//! A static property site keeps its record in a global of P. R's record
//! helper alone allocates a record; the hit entry and the miss kernel take
//! the site's record and neither read nor grow a record table.
use super::*;
use crate::gc_types::{
    GcNullability, GlobalLedger, OwnDataReadCache, OwnDataReadCacheTable, RuntimeModuleTypes,
};

#[test]
fn static_sites_keep_records_in_program_globals_that_only_r_allocates() {
    let artifact = emit_script("function read(object){return object.value;} read({value:1});")
        .expect("record site control emits");
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
    let maker = index_of("helper::cached_property_record");
    let registered = RuntimeModuleTypes::register(crate::tests::standalone_kind())
        .finalize_globals(GlobalLedger::new(), false, 0);
    let concrete = |reference: wasm_encoder::RefType| {
        let wasm_encoder::HeapType::Concrete(index) = reference.heap_type else {
            panic!("records and their table are declared GC types")
        };
        index
    };
    let schema = registered.globals().schema();
    let table =
        concrete(schema.reference_type::<OwnDataReadCacheTable>(GcNullability::NonNullable));
    let record = concrete(schema.reference_type::<OwnDataReadCache>(GcNullability::NonNullable));
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
                if index == entry || index == miss || index == maker {
                    let mut table_uses = 0;
                    let mut record_allocations = 0;
                    let mut other_allocations = 0;
                    for operator in body.get_operators_reader().unwrap() {
                        match operator.unwrap() {
                            Operator::ArrayNew { array_type_index }
                            | Operator::ArrayGet { array_type_index }
                            | Operator::ArraySet { array_type_index }
                                if array_type_index == table =>
                            {
                                table_uses += 1
                            }
                            Operator::StructNew { struct_type_index }
                                if struct_type_index == record =>
                            {
                                record_allocations += 1
                            }
                            Operator::ArrayNew { .. }
                            | Operator::ArrayNewDefault { .. }
                            | Operator::ArrayNewFixed { .. }
                            | Operator::StructNew { .. }
                            | Operator::StructNewDefault { .. } => other_allocations += 1,
                            _ => {}
                        }
                    }
                    assert_eq!(table_uses, 0, "static sites never use the record table");
                    if index == maker {
                        assert_eq!(
                            (record_allocations, other_allocations),
                            (1, 0),
                            "the record helper allocates exactly the site's record"
                        );
                    } else if index == entry {
                        assert_eq!(
                            (record_allocations, other_allocations),
                            (0, 0),
                            "a hit allocates nothing"
                        );
                    } else {
                        assert_eq!(
                            record_allocations, 0,
                            "the miss kernel fills the record it is given"
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
        checked, 3,
        "the hit entry, the miss kernel and the record helper are present"
    );
    // Every record global belongs to exactly one emitted site: the site's
    // first use stores the record R made, and no other code stores it. A
    // body that is emitted twice (an exact helper-context variant, say) has
    // two sites.
    let mut record_globals = std::collections::BTreeSet::new();
    let mut imported_globals = 0;
    let mut stores = std::collections::BTreeMap::<u32, u32>::new();
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        match payload.unwrap() {
            Payload::ImportSection(imports) => {
                imported_globals += imports
                    .into_imports()
                    .map(Result::unwrap)
                    .filter(|import| matches!(import.ty, wasmparser::TypeRef::Global(_)))
                    .count() as u32;
            }
            Payload::GlobalSection(globals) => {
                for (position, global) in globals.into_iter().enumerate() {
                    let global = global.unwrap();
                    if let wasmparser::ValType::Ref(reference) = global.ty.content_type {
                        if reference.is_nullable()
                            && global.ty.mutable
                            && reference
                                .type_index()
                                .and_then(|index| index.as_module_index())
                                == Some(record)
                        {
                            record_globals.insert(imported_globals + position as u32);
                        }
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                for operator in body.get_operators_reader().unwrap() {
                    if let Operator::GlobalSet { global_index } = operator.unwrap() {
                        *stores.entry(global_index).or_default() += 1;
                    }
                }
            }
            _ => {}
        }
    }
    assert!(
        !record_globals.is_empty(),
        "P keeps the static read site's record in a global"
    );
    for global in &record_globals {
        assert_eq!(
            stores.get(global),
            Some(&1),
            "one site stores each record global, once"
        );
    }
}
