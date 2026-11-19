//! Realm creation leaves every demand constructor to its first canonical read.
use super::error_demand_initialization::{body, named};
use super::*;
use crate::heap::DemandConstructor;

#[test]
fn realm_creation_allocates_no_demand_constructor_and_the_gate_builds_each_once() {
    let artifact = emit_script("this;").unwrap();
    expect_valid_module(&artifact, 0);
    let bytes = artifact.runtime().unwrap().bytes();
    let names = function_names_in_bytes(bytes);
    let initial = body(bytes, named(&names, "helper::realm_initialize_intrinsics"));
    let ensure = body(bytes, named(&names, "helper::realm_ensure_constructor"));
    for demand in DemandConstructor::ALL {
        let entry = named(
            &names,
            &format!("builtin::{}", demand.builtin().debug_name()),
        );
        let references = |ops: &[Operator<'_>]| {
            ops.iter()
                .filter(|op| matches!(op, Operator::RefFunc { function_index } if *function_index == entry))
                .count()
        };
        assert_eq!(references(&initial), 0, "startup builds {:?}", demand);
        assert_eq!(references(&ensure), 1, "one canonical {:?}", demand);
    }
}
