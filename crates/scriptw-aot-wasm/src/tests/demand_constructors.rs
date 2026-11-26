//! Realm creation leaves every demand constructor, demand prototype, boxed
//! Number/Boolean prototype, dynamic function constructor, parseInt/parseFloat,
//! and standard global namespace to its first canonical read.
use super::error_demand_initialization::{body, named};
use super::*;
use crate::heap::{DemandConstructor, DemandDynamicFunction, DemandHostFunction};

#[test]
fn realm_creation_allocates_no_demand_constructor_and_the_gate_builds_each_once() {
    let artifact = emit_script("this;").unwrap();
    expect_valid_module(&artifact, 0);
    let bytes = artifact.runtime().unwrap().bytes();
    let names = function_names_in_bytes(bytes);
    let initial = body(bytes, named(&names, "helper::realm_initialize_intrinsics"));
    let ensure = body(bytes, named(&names, "helper::realm_ensure_constructor"));
    let references = |ops: &[Operator<'_>], entry: u32| {
        ops.iter()
            .filter(|op| matches!(op, Operator::RefFunc { function_index } if *function_index == entry))
            .count()
    };
    for demand in DemandConstructor::ALL {
        let entry = named(
            &names,
            &format!("builtin::{}", demand.builtin().debug_name()),
        );
        assert_eq!(references(&initial, entry), 0, "startup builds {:?}", demand);
        assert_eq!(references(&ensure, entry), 1, "one canonical {:?}", demand);
    }
    for demand in DemandDynamicFunction::ALL {
        let entry = named(&names, &format!("host::{}", demand.builtin().as_str()));
        assert_eq!(references(&initial, entry), 0, "startup builds {:?}", demand);
        assert_eq!(references(&ensure, entry), 1, "one canonical {:?}", demand);
    }
    for demand in DemandHostFunction::ALL {
        let entry = named(&names, &format!("host::{}", demand.builtin().as_str()));
        assert_eq!(references(&initial, entry), 0, "startup builds {:?}", demand);
        assert_eq!(references(&ensure, entry), 1, "one canonical {:?}", demand);
    }
}
