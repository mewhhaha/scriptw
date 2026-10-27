//! The native optional-alias cohort must share Realm bootstrap without moving
//! any emitted body across the Engine's whole-module size-optimization switch.

use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{Validator, WasmFeatures};

use crate::linked_bodies;
use linked_bodies::linked_bodies;

// This is the exact sloppy-mode source used by the Engine's strict/sloppy
// semantic control, with the same empty preamble and default host policy.
const SOURCE: &str = include_str!(
    "../../../scriptw-engine/tests/fixtures/shadow_realm/finite_candidates_receiver_aliases.js"
);

// Engine switches to SizeOptimized at >= 1 MiB for even one code body. This
// cohort previously emitted a 1,196,893-byte main body despite passing natively.
const SIZE_OPTIMIZED_MIN_BODY_BYTES: usize = 1024 * 1024;
const INITIALIZER: &str = "helper::realm_initialize_intrinsics";

#[test]
fn shadow_realm_bootstrap_is_shared_and_keeps_every_body_below_native_size_switch() {
    let artifact = std::thread::Builder::new()
        .name("shadow-realm-bootstrap-size".to_owned())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let parsed = parse(SOURCE, ParseOptions::script())
                .expect("native optional-alias fixture parses");
            let program = lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let artifact =
                scriptw_aot_wasm::emit(&program).expect("native optional-alias fixture emits");
            let mut features = WasmFeatures::default();
            for feature in [
                WasmFeatures::THREADS,
                WasmFeatures::MULTI_MEMORY,
                WasmFeatures::REFERENCE_TYPES,
                WasmFeatures::FUNCTION_REFERENCES,
                WasmFeatures::GC,
                WasmFeatures::EXCEPTIONS,
                WasmFeatures::TAIL_CALL,
            ] {
                features.set(feature, true);
            }
            for bytes in [
                &artifact.runtime().expect("heap program links R").bytes()[..],
                artifact.bytes.as_slice(),
            ] {
                Validator::new_with_features(features)
                    .validate_all(bytes)
                    .expect("shared Realm initialization and its callers validate");
            }
            artifact
        })
        .expect("compiler worker starts")
        .join()
        .expect("compiler worker completes");

    let bodies = linked_bodies(&artifact);
    let initializer = bodies.unique(INITIALIZER);
    let imports = linked_bodies::runtime_function_imports(
        &artifact.bytes,
        artifact.runtime().unwrap().bytes(),
    );
    assert!(
        bodies
            .runtime
            .iter()
            .any(|body| body.index == initializer.index),
        "the shared initializer is a runtime-module body"
    );
    eprintln!("{INITIALIZER}: {} bytes", initializer.bytes());
    // The callers' actual instructions establish that both bootstrap paths
    // consume the same helper: `main` in P, the ShadowRealm builtin in R.
    for (name, owner) in [
        ("scriptw::main", &bodies.program),
        ("builtin::ShadowRealm", &bodies.runtime),
    ] {
        let body = bodies.unique(name);
        assert!(
            owner.iter().any(|candidate| candidate.index == body.index),
            "{name} is in the wrong module"
        );
        eprintln!("{name}: {} bytes", body.bytes());
        assert!(
            body.calls().iter().any(|index| {
                if name == "scriptw::main" {
                    imports.get(index) == Some(&initializer.index)
                } else {
                    *index == initializer.index
                }
            }),
            "{name} must directly call the shared {INITIALIZER}"
        );
    }

    let oversized = bodies
        .all()
        .filter(|body| body.bytes() >= SIZE_OPTIMIZED_MIN_BODY_BYTES)
        .map(|body| (body.index, body.name.as_str(), body.bytes()))
        .collect::<Vec<_>>();
    assert!(
        oversized.is_empty(),
        "every body in R and P, including main, must stay below the \
         {SIZE_OPTIMIZED_MIN_BODY_BYTES}-byte whole-module SizeOptimized switch; \
         oversized bodies: {oversized:?}"
    );
}
