//! Operator sites and the native method share one whole-Completion R kernel.
use scriptw_aot_wasm::{emit, WasmArtifact};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{FuncType, Operator, Parser, Payload, TypeRef, ValType, Validator, WasmFeatures};

use crate::linked_bodies::{linked_bodies, runtime_function_imports};

fn artifact() -> WasmArtifact {
    std::thread::Builder::new()
        .name("has-instance-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            // Six formals keep these bodies independent of the local-argument
            // entry optimization. Every operator retains unknown runtime inputs.
            let source = format!(
                "function single(object, constructor, unused1, unused2, unused3, unused4) {{
                    return object instanceof constructor;
                 }}
                 function repeated(object, constructor, unused1, unused2, unused3, unused4) {{
                    {}
                    return object instanceof constructor;
                 }}
                 globalThis.retained = [single, repeated];",
                "object instanceof constructor;".repeat(7),
            );
            let parsed = parse(&source, ParseOptions::script()).expect("dynamic operators parse");
            let lowered = lower(&parsed);
            assert!(lowered.is_wasm_supported(), "{:?}", lowered.diagnostics);
            emit(&lowered).expect("dynamic operators emit real R/P modules")
        })
        .expect("compiler worker starts")
        .join()
        .expect("helper emission has no callable-Realm assumption")
}

fn signature(bytes: &[u8], function: u32) -> FuncType {
    let mut types = Vec::new();
    let mut functions = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("module decodes") {
            Payload::TypeSection(groups) => {
                for group in groups {
                    types.extend(group.unwrap().into_types());
                }
            }
            Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    if let TypeRef::Func(index) | TypeRef::FuncExact(index) = import.unwrap().ty {
                        functions.push(index);
                    }
                }
            }
            Payload::FunctionSection(declarations) => {
                functions.extend(declarations.into_iter().map(Result::unwrap));
            }
            _ => {}
        }
    }
    let wasmparser::CompositeInnerType::Func(signature) = &types
        [functions[function as usize] as usize]
        .composite_type
        .inner
    else {
        panic!("function has an actual function signature");
    };
    signature.clone()
}

fn operators(bytes: &[u8], wanted: u32) -> Vec<Operator<'_>> {
    let mut index = 0;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("module decodes") {
            Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    if matches!(import.unwrap().ty, TypeRef::Func(_) | TypeRef::FuncExact(_)) {
                        index += 1;
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                if index == wanted {
                    return body
                        .get_operators_reader()
                        .unwrap()
                        .into_iter()
                        .map(Result::unwrap)
                        .collect();
                }
                index += 1;
            }
            _ => {}
        }
    }
    panic!("named function has an encoded body");
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
fn both_has_instance_entries_share_one_typed_whole_completion_kernel() {
    let artifact = artifact();
    let runtime = artifact.runtime().expect("HasInstance requires R").bytes();
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
    for bytes in [runtime, artifact.bytes.as_slice()] {
        Validator::new_with_features(features)
            .validate_all(bytes)
            .expect("normal and abrupt helper exits validate as actual Wasm");
    }
    let bodies = linked_bodies(&artifact);
    let helper = bodies.unique("helper::has_instance");
    let method = bodies.unique("builtin::Function.prototype[Symbol.hasInstance]");
    let prototype = bodies.unique("helper::object_get_prototype_of");
    assert_eq!(
        call_count(&operators(runtime, method.index), helper.index),
        1,
        "the native OrdinaryHasInstance entry uses the same R kernel"
    );
    let kernel = operators(runtime, helper.index);
    assert_eq!(call_count(&kernel, prototype.index), 1);
    assert!(
        kernel
            .iter()
            .filter(|op| matches!(op, Operator::Loop { .. }))
            .count()
            >= 2,
        "bound-target transitions and the prototype walk stay inside the kernel"
    );
    let imports = runtime_function_imports(&artifact.bytes, runtime);
    let imported = imports
        .iter()
        .filter_map(|(&source, &target)| (target == helper.index).then_some(source))
        .collect::<Vec<_>>();
    assert_eq!(imported.len(), 1, "P imports the single actual R owner");
    let helper_signature = signature(runtime, helper.index);
    assert_eq!(signature(&artifact.bytes, imported[0]), helper_signature);
    assert_eq!(
        &helper_signature.params()[..7],
        [
            ValType::I32,
            ValType::I32,
            ValType::I64,
            ValType::Ref(wasmparser::RefType::EQREF),
            ValType::I32,
            ValType::I64,
            ValType::Ref(wasmparser::RefType::EQREF)
        ],
        "entry mode and two complete held Values"
    );
    assert_eq!(helper_signature.params().len(), 10);
    for (parameter, nullable) in helper_signature.params()[7..]
        .iter()
        .zip([false, true, false])
    {
        let ValType::Ref(reference) = parameter else {
            panic!("execution Realm, Environment and Program are typed GC references");
        };
        assert_eq!(reference.is_nullable(), nullable);
        assert!(matches!(
            reference.heap_type(),
            wasmparser::HeapType::Concrete(_)
        ));
    }
    assert_eq!(
        helper_signature.results(),
        [
            ValType::I32,
            ValType::I64,
            ValType::Ref(wasmparser::RefType::EQREF),
            ValType::I32,
            ValType::I32
        ],
        "the boundary preserves the whole Completion, including Throw"
    );
}

#[test]
fn repeated_instanceof_sites_do_not_expand_the_state_machine_in_p() {
    let artifact = artifact();
    let runtime = artifact.runtime().unwrap().bytes();
    let bodies = linked_bodies(&artifact);
    let helper = bodies.unique("helper::has_instance");
    let prototype = bodies.unique("helper::object_get_prototype_of");
    let imports = runtime_function_imports(&artifact.bytes, runtime);
    let imported = *imports
        .iter()
        .find(|(_, target)| **target == helper.index)
        .unwrap()
        .0;
    let mut sizes = Vec::new();
    for (name, sites) in [("single", 1), ("repeated", 8)] {
        let matched = bodies
            .program
            .iter()
            .filter(|body| body.name.starts_with(&format!("js::{name}#")))
            .collect::<Vec<_>>();
        assert_eq!(matched.len(), 1, "one canonical body for {name}");
        let body = matched[0];
        let code = operators(&artifact.bytes, body.index);
        assert_eq!(call_count(&code, imported), sites);
        assert!(
            body.calls()
                .iter()
                .all(|callee| imports.get(callee) != Some(&prototype.index)),
            "source sites cannot retain an inlined prototype walk"
        );
        sizes.push(body.bytes());
    }
    let growth = sizes[1]
        .checked_sub(sizes[0])
        .expect("seven more dynamic operations");
    assert!(
        growth < 7 * 1024,
        "seven operator sites must add bounded argument/result transport, not seven kernels: {sizes:?}"
    );
}
