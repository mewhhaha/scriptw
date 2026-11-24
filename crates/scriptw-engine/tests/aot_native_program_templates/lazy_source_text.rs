//! A source function's text stays its defining P's literal until toString asks
//! for it. R resolves it through that P's own resolver, whichever P or Realm
//! calls toString, and the result is the same String any read of that literal
//! returns.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run_true(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
            source,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(60_000),
                ..RunOptions::default()
            },
        )
        .expect("lazy source text fixture executes through Wasm");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn functions_of_runtime_compiled_programs_resolve_their_own_source_text() {
    run_true(
        r#"
var token = 'lazy_' + Date.now();
var expected = 'function ' + token + '(a) { return "' + token + '"; }';
var make = Function('return ' + expected + ';');
var inner = make();
gc();
var texts = [
  Function.prototype.toString.call(inner),
  inner.toString(),
  Function.prototype.toString.call(make),
];
var evaluated = eval('(function ' + token + '_eval() { return 7; })');
gc();
texts[0] === expected && texts[1] === expected && inner(0) === token
  && texts[2] === 'function anonymous(\n) {\nreturn ' + expected + ';\n}'
  && evaluated.toString() === 'function ' + token + '_eval() { return 7; }'
  && evaluated() === 7;
"#,
    );
}

#[test]
fn source_text_that_is_also_a_string_literal_keeps_one_value() {
    run_true(
        r#"
function twin(a) { return a; }
function later() {}
// The literal is read before toString resolves `twin`, and after toString
// resolved `later`; either order yields one equal String.
var early = "function twin(a) { return a; }";
var twinText = twin.toString();
var laterText = later.toString();
var late = "function later() {}";
var keys = new Map([[twinText, 1], [late, 2]]);
gc();
twinText === early && Object.is(twinText, early) && laterText === late
  && keys.get(early) === 1 && keys.get(laterText) === 2
  && Function.prototype.toString.call(twin) === twinText
  && twin.toString() === twin.toString();
"#,
    );
}

#[test]
fn source_text_resolves_through_its_defining_program_from_another_realm() {
    run_true(
        r#"
var token = 'realm_' + Date.now();
var realm = __scriptwCreateRealm();
var other = realm.global;
var expected = 'function ' + token + '(x) { return x + 1; }';
// The other Realm compiles this source at run time into its own program.
var foreign = realm.evalScript('(' + expected + ')');
var constant = realm.evalScript('(function fixed() { return "fixed"; })');
function local() { return 'local'; }
gc();
Function.prototype.toString.call(foreign) === expected
  && other.Function.prototype.toString.call(foreign) === expected
  && other.Function.prototype.toString.call(constant) === 'function fixed() { return "fixed"; }'
  && other.Function.prototype.toString.call(local) === "function local() { return 'local'; }"
  && foreign(1) === 2 && constant() === 'fixed';
"#,
    );
}

#[test]
fn functions_exported_by_another_module_keep_their_source_text() {
    let directory =
        std::env::temp_dir().join(format!("scriptw-lazy-source-text-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create module fixture");
    let entry = r#"
import { exported, Shape } from './dependency.js';
const namespace = await import('./dependency.js');
const local = "export function exported(value) { return value * 2; }".slice(7);
if (exported.toString() !== local) throw new Error('exported source text');
if (Function.prototype.toString.call(Shape) !== 'class Shape { area() { return 1; } }')
  throw new Error('class source text');
if (Shape.prototype.area.toString() !== 'area() { return 1; }') throw new Error('method');
if (namespace.exported !== exported || exported(2) !== 4) throw new Error('binding');
print(exported.toString());
"#;
    std::fs::write(directory.join("entry.js"), entry).expect("write entry module");
    std::fs::write(
        directory.join("dependency.js"),
        "export function exported(value) { return value * 2; }\n\
         export class Shape { area() { return 1; } }\n",
    )
    .expect("write dependency module");
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_module(
            entry,
            CompileOptions {
                filename: Some(directory.join("entry.js").to_str().unwrap().into()),
                module_root: Some(directory.to_str().unwrap().into()),
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(60_000),
                ..RunOptions::default()
            },
        )
        .expect("module fixture compiles and executes through Wasm");
    std::fs::remove_dir_all(&directory).ok();
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        vec![HostOutputEvent::PrintLine(
            "function exported(value) { return value * 2; }".into()
        )]
    );
}
