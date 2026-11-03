use super::*;
use crate::gc_types::{
    BindingCell, BindingCellTable, DeclarativeEnvironment, Environment, GcNullability,
    GlobalLedger, NamedBinding, NamedBindingTable, RuntimeModuleTypes,
};

fn with_storage_counts(source: &str, name: &str, mut check: impl FnMut(&[usize])) {
    let program = lower(&parse(source, ParseOptions::module()).unwrap());
    let artifact = emit(&program).expect("Module storage regression emits");
    expect_valid_module(&artifact, 1);
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Program(
        artifact.runtime().unwrap(),
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let types = [
        schema.reference_type::<BindingCell>(GcNullability::NonNullable),
        schema.reference_type::<Environment>(GcNullability::NonNullable),
        schema.reference_type::<DeclarativeEnvironment>(GcNullability::NonNullable),
        schema.reference_type::<NamedBinding>(GcNullability::NonNullable),
        schema.reference_type::<BindingCellTable>(GcNullability::NonNullable),
        schema.reference_type::<NamedBindingTable>(GcNullability::NonNullable),
    ]
    .map(|reference| match reference.heap_type {
        wasm_encoder::HeapType::Concrete(index) => index,
        _ => panic!("environment storage has concrete GC types"),
    });
    let names = function_names(&artifact)
        .into_iter()
        .filter(|(_, actual)| actual.starts_with(&format!("js::{name}#")))
        .collect::<BTreeMap<_, _>>();
    assert!(!names.is_empty(), "named source bodies exist");
    let mut index = imported_function_count(&artifact);
    let mut inspected = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if names.contains_key(&index) {
                let mut allocations = [0; 6];
                for operator in body.get_operators_reader().unwrap() {
                    let allocated = match operator.unwrap() {
                        Operator::StructNew { struct_type_index }
                        | Operator::StructNewDefault { struct_type_index } => {
                            Some(struct_type_index)
                        }
                        Operator::ArrayNew { array_type_index }
                        | Operator::ArrayNewDefault { array_type_index }
                        | Operator::ArrayNewFixed {
                            array_type_index, ..
                        } => Some(array_type_index),
                        _ => None,
                    };
                    if let Some(allocated) = allocated {
                        for (count, expected) in allocations.iter_mut().zip(types) {
                            *count += usize::from(allocated == expected);
                        }
                    }
                }
                check(&allocations);
                inspected += 1;
            }
            index += 1;
        }
    }
    assert_eq!(
        inspected,
        names.len(),
        "all physical source variants inspected"
    );
}

#[test]
fn module_entry_values_and_unobserved_arguments_allocate_no_environment_storage() {
    for source in [
        "export function make(row) { return row.x; }",
        "export function make() { return [this, new.target]; }",
    ] {
        with_storage_counts(source, "make", |counts| {
            assert_eq!(counts, [0; 6], "{source}")
        });
    }
}

#[test]
fn observed_and_arrow_captured_module_context_keeps_real_cells() {
    with_storage_counts(
        "export function make() { return arguments[0]; }",
        "make",
        |counts| {
            assert_eq!(counts[0], 1, "only observed arguments needs a BindingCell");
            assert_eq!(counts[1], 1, "its activation record remains");
        },
    );
    with_storage_counts(
        "export function make(value) { return () => [this, new.target, arguments[0], value]; }",
        "make",
        |counts| {
            assert_eq!(
                counts[0], 4,
                "each captured context and parameter needs a real cell"
            );
            assert_eq!(counts[1], 1, "the shared captured activation remains");
        },
    );
}
