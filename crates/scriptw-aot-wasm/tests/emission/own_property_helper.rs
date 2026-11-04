//! Internal [[GetOwnProperty]] transports a private snapshot across actual R/P.
use crate::{linked_bodies::module_bodies, property_read_plans::Module};
use scriptw_aot_wasm::WasmArtifact;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower_with_host_surface_policy, HostSurfacePolicy};
use wasmparser::{Operator, ValType, Validator, WasmFeatures};

fn artifact() -> WasmArtifact {
    std::thread::Builder::new()
        .name("own-property-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let parsed = parse(
                "function copy(source, unused, extra) { return {...source}; } globalThis.copy = copy;",
                ParseOptions::script(),
            )
            .expect("dynamic descriptor consumer parses");
            scriptw_aot_wasm::emit(&lower_with_host_surface_policy(
                &parsed,
                HostSurfacePolicy::Test262,
            ))
            .expect("private descriptor owner emits without a callable context")
        })
        .unwrap()
        .join()
        .unwrap()
}

fn call_count(operators: &[Operator<'_>], callee: u32) -> usize {
    operators
        .iter()
        .filter(|operator| {
            matches!(operator, Operator::Call { function_index } if *function_index == callee)
        })
        .count()
}

#[test]
fn recursive_own_property_returns_whole_completion_and_actual_private_descriptor_type() {
    let artifact = artifact();
    for bytes in [
        artifact.runtime().unwrap().bytes(),
        artifact.bytes.as_slice(),
    ] {
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(bytes)
            .expect("normal, absent and whole-Throw exits validate as actual Wasm");
    }
    let runtime = Module::read(artifact.runtime().unwrap().bytes(), true);
    let (helper, kernel) = runtime.named("helper::object_get_own_property");
    let (ordinary, _) = runtime.named("helper::ordinary_own_descriptor");
    let expected = runtime.function_type(ordinary).results()[0];
    let ValType::Ref(reference) = expected else {
        panic!("the storage kernel returns a typed descriptor reference");
    };
    assert!(reference.is_nullable());
    assert!(matches!(
        reference.heap_type(),
        wasmparser::HeapType::Concrete(_)
    ));
    let signature = runtime.function_type(helper);
    assert_eq!(
        signature.params().len(),
        9,
        "target, key, Realm, Environment, Program"
    );
    assert_eq!(
        &signature.params()[..6],
        [
            ValType::I32,
            ValType::I64,
            ValType::Ref(wasmparser::RefType::EQREF),
            ValType::I32,
            ValType::I64,
            ValType::Ref(wasmparser::RefType::EQREF),
        ],
    );
    for (parameter, nullable) in signature.params()[6..].iter().zip([false, true, false]) {
        let ValType::Ref(reference) = parameter else {
            panic!("execution context operands are typed references");
        };
        assert_eq!(reference.is_nullable(), nullable);
        assert!(matches!(
            reference.heap_type(),
            wasmparser::HeapType::Concrete(_)
        ));
    }
    assert_eq!(
        signature.results(),
        [
            ValType::I32,
            ValType::I64,
            ValType::Ref(wasmparser::RefType::EQREF),
            ValType::I32,
            ValType::I32,
            expected,
        ],
        "whole Completion plus the storage model's nullable descriptor, not a JS Value",
    );
    assert_eq!(
        call_count(kernel, helper),
        2,
        "missing-trap forwarding and exotic target validation recurse through the same helper",
    );
    let (read, read_body) = runtime.named("helper::object_read");
    let (_, advance_body) = runtime.named("helper::for_in_advance");
    let (append, _) = runtime.named("helper::ordinary_property_append");
    assert_eq!(
        call_count(read_body, helper),
        2,
        "non-Proxy own lookup and the successful Proxy get-trap invariant check each retain one exotic descriptor fallback",
    );
    assert_eq!(call_count(advance_body, helper), 1);
    for name in [
        "builtin::Object.getOwnPropertyDescriptor",
        "builtin::Reflect.getOwnPropertyDescriptor",
        "builtin::Object.getOwnPropertyDescriptors",
    ] {
        let (public, body) = runtime.named(name);
        assert_eq!(
            call_count(body, helper),
            1,
            "{name} acquires a private snapshot"
        );
        assert!(
            call_count(body, append) > 0,
            "{name} publishes fresh observable fields"
        );
        for consumer in [kernel, read_body, advance_body] {
            assert!(
                !consumer.iter().any(|op| matches!(op,
                Operator::Call { function_index } | Operator::RefFunc { function_index }
                    if *function_index == public)),
                "internal consumers cannot construct or invoke a public descriptor callable"
            );
        }
    }
    assert!(
        call_count(kernel, read) > 0,
        "Proxy trap and descriptor conversion retain ordinary Get"
    );
    assert_eq!(
        call_count(kernel, append),
        0,
        "private results do not publish descriptor fields"
    );
}

#[test]
fn source_copy_imports_private_descriptor_helper_and_keeps_concrete_storage_paths() {
    let artifact = artifact();
    let runtime = Module::read(artifact.runtime().unwrap().bytes(), true);
    let program = Module::read(&artifact.bytes, true);
    let (helper, _) = runtime.named("helper::object_get_own_property");
    let (ordinary, _) = runtime.named("helper::ordinary_own_descriptor");
    let imported = program.imported_runtime_function(&runtime, helper);
    let ordinary_import = program.imported_runtime_function(&runtime, ordinary);
    assert_eq!(
        program.function_type(imported),
        runtime.function_type(helper)
    );
    let copies = module_bodies(&artifact.bytes)
        .into_iter()
        .filter(|body| body.name.starts_with("js::copy#"))
        .collect::<Vec<_>>();
    assert!(!copies.is_empty());
    for copy in copies {
        let (_, code) = program.named(&copy.name);
        assert_eq!(
            call_count(code, imported),
            1,
            "one exotic descriptor fallback"
        );
        assert!(
            call_count(code, ordinary_import) > 0,
            "actual ordinary/Array lookup stays direct"
        );
    }
}
