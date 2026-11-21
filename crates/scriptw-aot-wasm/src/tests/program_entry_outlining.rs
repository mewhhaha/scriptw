//! P shares one body for every rejected source entry, and keeps only the host
//! function imports its bodies call.
use super::*;

const MAX_ACTIVE_SOURCE_BODIES: i32 = 128;

fn bodies(bytes: &[u8]) -> Vec<(u32, Vec<Operator<'_>>)> {
    let mut index = imported_function_count_in_bytes(bytes);
    let mut result = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            let ops = crate::tests::emitted_operators(&body);
            result.push((index, ops));
            index += 1;
        }
    }
    result
}

/// The arm of each depth guard (the saved depth `global.get` kept in a local,
/// then `i32.const 128; i32.ge_u; if ... end`), without its `if` and `end`.
fn guard_arms<'a>(ops: &'a [Operator<'a>]) -> Vec<&'a [Operator<'a>]> {
    let mut arms = Vec::new();
    for test in 2..ops.len().saturating_sub(2) {
        let [Operator::I32Const { value }, Operator::I32GeU, Operator::If { .. }] =
            &ops[test..test + 3]
        else {
            continue;
        };
        // `local.set; local.get` of one local, or the `local.tee` of it.
        let saved = match &ops[..test] {
            [.., Operator::GlobalGet { .. }, Operator::LocalSet { local_index: saved }, Operator::LocalGet { local_index: read }]
                if saved == read =>
            {
                true
            }
            [.., Operator::GlobalGet { .. }, Operator::LocalTee { .. }] => true,
            _ => false,
        };
        if !saved || *value != MAX_ACTIVE_SOURCE_BODIES {
            continue;
        }
        let mut depth = 1;
        let arm = test + 3;
        let mut end = arm;
        while depth > 0 {
            match ops[end] {
                Operator::Block { .. }
                | Operator::Loop { .. }
                | Operator::If { .. }
                | Operator::TryTable { .. } => depth += 1,
                Operator::End => depth -= 1,
                _ => {}
            }
            end += 1;
        }
        arms.push(&ops[arm..end - 1]);
    }
    arms
}

#[test]
fn program_source_bodies_tail_call_one_overflow_helper_and_r_bodies_keep_theirs() {
    let artifact = emit_script(
        "function leaf(n) { return n + 1; }\n\
         function middle(n) { return leaf(n) * 2; }\n\
         var object = { method() { return middle(1); } };\n\
         print(object.method(), (() => middle(2))());",
    )
    .unwrap();
    expect_valid_module(&artifact, 0);
    let names = function_names_in_bytes(&artifact.bytes);
    let helpers = names
        .iter()
        .filter(|(_, name)| name.as_str() == "helper::source_depth_overflow")
        .map(|(index, _)| *index)
        .collect::<Vec<_>>();
    assert_eq!(helpers.len(), 1, "one P body builds every overflow Throw");
    let helper = helpers[0];
    let mut guards = 0;
    for (index, ops) in bodies(&artifact.bytes) {
        for arm in guard_arms(&ops) {
            guards += 1;
            assert!(
                matches!(arm.last(), Some(Operator::ReturnCall { function_index }) if *function_index == helper),
                "{}: the rejected entry ends in the helper's tail call: {arm:?}",
                names.get(&index).map_or("?", String::as_str)
            );
            assert!(
                !arm.iter()
                    .any(|op| matches!(op, Operator::Return | Operator::Call { .. })),
                "the arm only loads the execution Realm: {arm:?}"
            );
        }
    }
    assert!(guards >= 4, "every source body is guarded: {guards}");

    // R cannot reach a P helper: its own source bodies keep the inline Throw.
    let runtime = artifact.runtime().unwrap();
    let runtime_names = function_names_in_bytes(runtime.bytes());
    assert!(!runtime_names
        .values()
        .any(|name| name == "helper::source_depth_overflow"));
    let mut runtime_guards = 0;
    for (_, ops) in bodies(runtime.bytes()) {
        for arm in guard_arms(&ops) {
            runtime_guards += 1;
            assert!(matches!(arm.last(), Some(Operator::Return)), "{arm:?}");
            assert!(!arm
                .iter()
                .any(|op| matches!(op, Operator::ReturnCall { .. })));
        }
    }
    assert!(runtime_guards > 0, "R owns guarded source bodies");
}

#[test]
fn program_keeps_only_called_host_functions_and_every_compiler_authority_import() {
    let artifact = emit_script("print(Math.max(1, 2));").unwrap();
    expect_valid_module(&artifact, 0);
    let mut called = std::collections::BTreeSet::new();
    for (_, ops) in bodies(&artifact.bytes) {
        for op in ops {
            if let Operator::Call { function_index } | Operator::ReturnCall { function_index } = op
            {
                called.insert(function_index);
            }
        }
    }
    let mut function = 0u32;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::ImportSection(reader) = payload.unwrap() else {
            continue;
        };
        for import in reader.into_imports() {
            let import = import.unwrap();
            match import.ty {
                wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_) => {
                    let authority = [
                        crate::gc_types::GcHostImport::CompileDynamicFunction,
                        crate::gc_types::GcHostImport::CompileGlobalScript,
                        crate::gc_types::GcHostImport::CompileDirectEval,
                    ]
                    .iter()
                    .any(|row| row.name() == import.name);
                    if import.module == crate::module::HOST_IMPORT_MODULE {
                        assert!(
                            authority || called.contains(&function),
                            "unreferenced host function import {} kept",
                            import.name
                        );
                    }
                    function += 1;
                }
                _ => {}
            }
        }
    }
    assert!(function > 0, "the linked P still imports what it calls");
    // The kept compiler imports still state the P's compilation profile.
    crate::RuntimeSourceCompilationProfile::from_module(&artifact.bytes).unwrap();
}
