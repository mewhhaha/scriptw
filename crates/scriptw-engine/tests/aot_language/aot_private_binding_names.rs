//! Compiler-private binding storage is unspellable: source bindings named
//! like it (`$this`, `$arguments`, `$homeObject`, `$forof0`, ...) are
//! ordinary bindings in every declaration form, owner and source goal.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str, module: bool) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let compile = CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    };
    let options = RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    };
    let outcome = if module {
        engine.observe_module(source, compile, options)
    } else {
        engine.observe_script(source, compile, options)
    }
    .unwrap();
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "module={module}: {:?}",
        outcome.completion
    );
    assert_eq!(
        outcome.output_events,
        [HostOutputEvent::PrintLine("private binding names ok 23".into())],
        "module={module}"
    );
}

#[test]
fn private_storage_spellings_are_ordinary_bindings_in_scripts_and_modules() {
    let source = include_str!("../fixtures/private_binding_names.js");
    run(source, false);
    run(source, true);
}

#[test]
fn a_strict_destructuring_loop_head_temporary_is_not_an_assignment_target() {
    let source = r#"
"use strict";
var caught = false;
try { for (var [a] in {xy: 1}) { $forin0 = "INJ"; } } catch (error) { caught = error instanceof ReferenceError; }
if (!caught) throw new Error("an undeclared name stayed unresolvable");
print("private binding names ok 23");
"#;
    run(source, false);
}
