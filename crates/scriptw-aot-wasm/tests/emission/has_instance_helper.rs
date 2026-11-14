//! Operator sites and the native method share one whole-Completion R kernel;
//! a source operator site enters it through a call-free entry that tail-calls it.
use scriptw_aot_wasm::{WasmArtifact, emit};
use scriptw_front::{ParseOptions, parse};
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
    let entry = bodies.unique("helper::instanceof_entry");
    let entry_code = operators(runtime, entry.index);
    assert!(
        !entry_code.iter().any(|op| matches!(
            op,
            Operator::Call { .. }
                | Operator::CallRef { .. }
                | Operator::CallIndirect { .. }
                | Operator::ReturnCall { .. }
                | Operator::ReturnCallRef { .. }
                | Operator::ReturnCallIndirect { .. }
        )),
        "the entry makes no call, so it spills nothing at a safepoint"
    );
    let imports = runtime_function_imports(&artifact.bytes, runtime);
    let imported = |target: u32| {
        imports
            .iter()
            .filter_map(|(&source, &callee)| (callee == target).then_some(source))
            .collect::<Vec<_>>()
    };
    assert_eq!(imported(entry.index).len(), 1, "P imports the single R entry");
    let record = bodies.unique("helper::instanceof_record");
    assert_eq!(
        imported(record.index).len(),
        1,
        "P imports the single R record maker"
    );
    let record_signature = signature(runtime, record.index);
    assert_eq!(
        signature(&artifact.bytes, imported(record.index)[0]),
        record_signature
    );
    assert_eq!(
        record_signature.params()[0],
        ValType::Ref(wasmparser::RefType::EQREF),
        "the record is made from the right-hand side's reference alone"
    );
    assert_eq!(
        record_signature.params().len(),
        4,
        "then the Realm, the Environment and the program"
    );
    let [ValType::Ref(made)] = record_signature.results() else {
        panic!("the maker returns one record reference");
    };
    assert!(
        !made.is_nullable(),
        "R returns a record for every right-hand side, declined or usable"
    );
    let kernels = imported(helper.index);
    assert_eq!(kernels.len(), 1, "P imports the single actual R owner");
    let entry_signature = signature(runtime, entry.index);
    assert_eq!(
        signature(&artifact.bytes, imported(entry.index)[0]),
        entry_signature
    );
    assert_eq!(
        &entry_signature.params()[..4],
        [
            ValType::I32,
            ValType::Ref(wasmparser::RefType::EQREF),
            ValType::Ref(wasmparser::RefType::EQREF),
            ValType::I32
        ],
        "the candidate's tag and reference, the constructor's reference and the site"
    );
    assert_eq!(entry_signature.params().len(), 6, "then the Realm and the program");
    assert_eq!(
        entry_signature.results(),
        [ValType::I32],
        "false, true or no answer; the site then calls the owner"
    );
    let helper_signature = signature(runtime, helper.index);
    assert_eq!(signature(&artifact.bytes, kernels[0]), helper_signature);
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
    assert_eq!(
        helper_signature.params()[7],
        ValType::I32,
        "the operator's record site, or none"
    );
    assert_eq!(helper_signature.params().len(), 11);
    for (parameter, nullable) in helper_signature.params()[8..]
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
    let entry = bodies.unique("helper::instanceof_entry");
    let helper = bodies.unique("helper::has_instance");
    let prototype = bodies.unique("helper::object_get_prototype_of");
    let imports = runtime_function_imports(&artifact.bytes, runtime);
    let import_of = |target: u32| {
        *imports
            .iter()
            .find(|(_, callee)| **callee == target)
            .unwrap()
            .0
    };
    let (imported, kernel) = (import_of(entry.index), import_of(helper.index));
    let maker = import_of(bodies.unique("helper::instanceof_record").index);
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
        assert_eq!(
            call_count(&code, maker),
            sites,
            "each site asks R for its own record, kept in its own global"
        );
        let first_call = |target: u32| {
            code.iter()
                .position(|op| matches!(op, Operator::Call { function_index } if *function_index == target))
                .unwrap()
        };
        assert!(
            first_call(maker) < first_call(imported),
            "a site answers from its record before it asks the entry"
        );
        assert_eq!(
            call_count(&code, kernel),
            sites,
            "each site calls the owner when its entry has no answer"
        );
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
    // Each site also carries its record answer: the record's checks and an
    // inline walk of the candidate's chain, about 1 KiB. The entry and the
    // HasInstance state machine stay in R.
    assert!(
        growth < 7 * 1280,
        "seven operator sites must add bounded record answers and argument/result transport, not seven kernels: {sizes:?}"
    );
}
