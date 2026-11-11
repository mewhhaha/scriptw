use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, ObservedJsValue, ObservedNumber, RealmBuilder, RunOptions,
};

struct Modules(PathBuf);

impl Drop for Modules {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_script(
    files: &[(&str, &str)],
    source: &str,
    expected: &[&str],
    completion: ObservedJsValue,
) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let fixture = Modules(std::env::temp_dir().join(format!(
        "scriptw-script-import-jobs-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&fixture.0).expect("create Script module fixture");
    for (name, text) in files {
        let path = fixture.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create Script fixture parent");
        std::fs::write(path, text).expect("write Script module fixture");
    }
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
            CompileOptions {
                filename: Some(fixture.0.join(files[0].0).to_str().unwrap().into()),
                module_root: Some(fixture.0.to_str().unwrap().into()),
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("Script dynamic-import graph compiles and executes through Wasm AOT");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(
        observed.completion,
        ObservedCompletion::Normal(completion),
        "{}; output events: {:?}",
        observed.note,
        observed.output_events
    );
    assert_eq!(
        observed.output_events,
        expected
            .iter()
            .map(|line| HostOutputEvent::PrintLine((*line).into()))
            .collect::<Vec<_>>()
    );
}

fn success(files: &[(&str, &str)], expected: &[&str]) {
    assert_script(files, files[0].1, expected, ObservedJsValue::Boolean(true));
}

#[test]
fn pinned_update_to_dynamic_import_keeps_nested_evaluation_lazy() {
    const SOURCE: &str = include_str!(
        "../../../../test262/vendor/test262/test/language/expressions/dynamic-import/update-to-dynamic-import.js"
    );
    const FIRST: &str = include_str!(
        "../../../../test262/vendor/test262/test/language/expressions/dynamic-import/update-to-dynamic-import_FIXTURE.js"
    );
    const OTHER: &str = include_str!(
        "../../../../test262/vendor/test262/test/language/expressions/dynamic-import/update-to-dynamic-import-other_FIXTURE.js"
    );
    const STA: &str = include_str!("../../../../test262/vendor/test262/harness/sta.js");
    const ASSERT: &str = include_str!("../../../../test262/vendor/test262/harness/assert.js");
    const DONE: &str =
        include_str!("../../../../test262/vendor/test262/harness/doneprintHandle.js");
    const ASYNC: &str = include_str!("../../../../test262/vendor/test262/harness/asyncHelpers.js");
    let files = [
        ("update-to-dynamic-import.js", SOURCE),
        ("update-to-dynamic-import_FIXTURE.js", FIRST),
        ("update-to-dynamic-import-other_FIXTURE.js", OTHER),
    ];
    for directive in ["", "\"use strict\";\n"] {
        let source = format!("{directive}{STA}\n{ASSERT}\n{DONE}\n{ASYNC}\n{SOURCE}");
        assert_script(
            &files,
            &source,
            &["Test262:AsyncTestComplete"],
            ObservedJsValue::Undefined,
        );
    }
}

#[test]
fn uncalled_import_branches_and_functions_never_evaluate_their_targets() {
    success(
        &[
            (
                "entry.js",
                include_str!("../fixtures/script_import_jobs/unreached/entry.js"),
            ),
            (
                "never.js",
                include_str!("../fixtures/script_import_jobs/unreached/never.js"),
            ),
            ("invalid.js", "invalid syntax!"),
        ],
        &["unreached targets stayed idle"],
    );
}

#[test]
fn sloppy_and_strict_script_globals_and_completion_survive_module_jobs() {
    const ENTRY: &str = include_str!("../fixtures/script_import_jobs/root_goal/entry.js");
    let files = [
        ("entry.js", ENTRY),
        (
            "target.js",
            include_str!("../fixtures/script_import_jobs/root_goal/target.js"),
        ),
    ];
    for directive in ["", "\"use strict\";\n"] {
        let source = format!("{directive}{ENTRY}");
        assert_script(
            &files,
            &source,
            &["Script globals ready", "module lexical this ready"],
            ObservedJsValue::Number(ObservedNumber::from_f64(73.0)),
        );
    }
}

#[test]
fn importing_the_script_filename_uses_a_distinct_cached_module_owner() {
    const ENTRY: &str = include_str!("../fixtures/script_import_jobs/self_import/entry.js");
    for directive in ["", "\"use strict\";\n"] {
        let source = format!("{directive}{ENTRY}");
        success(
            &[("entry.js", source.as_str())],
            &[
                "Script self import queued",
                "Module self import body",
                "distinct source goals",
            ],
        );
    }
}

#[test]
fn nested_dynamic_requests_evaluate_static_dependencies_before_updating_live_cells() {
    success(
        &[
            (
                "entry.js",
                include_str!("../fixtures/script_import_jobs/live_cells/entry.js"),
            ),
            (
                "first.js",
                include_str!("../fixtures/script_import_jobs/live_cells/first.js"),
            ),
            (
                "other.js",
                include_str!("../fixtures/script_import_jobs/live_cells/other.js"),
            ),
            (
                "dependency.js",
                include_str!("../fixtures/script_import_jobs/live_cells/dependency.js"),
            ),
        ],
        &["Script body", "first:first", "other:42:live"],
    );
}

#[test]
fn repeated_imports_keep_fresh_promises_one_evaluation_and_one_namespace() {
    success(
        &[
            (
                "entry.js",
                include_str!("../fixtures/script_import_jobs/repeated/entry.js"),
            ),
            (
                "target.js",
                include_str!("../fixtures/script_import_jobs/repeated/target.js"),
            ),
        ],
        &["imports queued", "one module namespace"],
    );
}

#[test]
fn ordinary_script_import_settles_after_evaluation_and_its_second_reaction() {
    const ENTRY: &str = include_str!("../fixtures/script_import_jobs/reactions/entry.js");
    for target in [
        "events.push('body'); export const value = 7;",
        "events.push('body'); throw 7;",
    ] {
        success(
            &[("entry.js", ENTRY), ("target.js", target)],
            &["second reaction"],
        );
    }
}

#[test]
fn cyclic_tla_dependencies_complete_before_script_import_fulfills() {
    success(
        &[
            (
                "entry.js",
                include_str!("../fixtures/script_import_jobs/cyclic_tla/entry.js"),
            ),
            (
                "a.js",
                include_str!("../fixtures/script_import_jobs/cyclic_tla/a.js"),
            ),
            (
                "b.js",
                include_str!("../fixtures/script_import_jobs/cyclic_tla/b.js"),
            ),
        ],
        &["cyclic TLA graph completed"],
    );
}

#[test]
fn cached_object_and_undefined_rejections_keep_identity_and_evaluate_once() {
    success(
        &[
            (
                "entry.js",
                include_str!("../fixtures/script_import_jobs/rejections/entry.js"),
            ),
            (
                "object.js",
                include_str!("../fixtures/script_import_jobs/rejections/object.js"),
            ),
            (
                "undefined.js",
                include_str!("../fixtures/script_import_jobs/rejections/undefined.js"),
            ),
        ],
        &["rejections queued", "cached arbitrary rejections"],
    );
}

#[test]
fn direct_syntax_and_transitive_link_errors_reject_at_their_owned_job_stages() {
    success(
        &[
            (
                "entry.js",
                include_str!("../fixtures/script_import_jobs/error_stages/entry.js"),
            ),
            ("invalid.js", "invalid syntax!"),
            (
                "transitive.js",
                "import './invalid.js'; globalThis.invalidBody = true;",
            ),
            (
                "link.js",
                "import { absent } from './shared.js'; globalThis.invalidBody = true;",
            ),
            (
                "missing.js",
                "import defer * as absent from './absent.js'; globalThis.invalidBody = true;",
            ),
            ("valid.js", "export { value } from './shared.js';"),
            (
                "shared.js",
                "globalThis.sharedCount++; export const value = 7;",
            ),
        ],
        &["load and dependency rejection stages"],
    );
}

#[test]
fn script_import_uses_its_realm_intrinsics_after_promise_and_reflection_replacement() {
    success(
        &[
            (
                "entry.js",
                include_str!("../fixtures/script_import_jobs/intrinsics/entry.js"),
            ),
            (
                "target.js",
                include_str!("../fixtures/script_import_jobs/intrinsics/target.js"),
            ),
            ("invalid.js", "invalid syntax!"),
        ],
        &["Script import realm intrinsics"],
    );
}

#[test]
fn with_environment_resolves_specifiers_without_observing_private_dispatchers() {
    success(
        &[
            (
                "entry.js",
                include_str!("../fixtures/script_import_jobs/with_lookup/entry.js"),
            ),
            (
                "value.js",
                include_str!("../fixtures/script_import_jobs/with_lookup/value.js"),
            ),
        ],
        &["with lookup preserves private dispatcher"],
    );
}

#[test]
fn script_and_module_callables_preserve_original_import_syntax_in_to_string() {
    const ENTRY: &str = include_str!("../fixtures/script_import_jobs/callable_source/entry.js");
    const TARGET: &str = include_str!("../fixtures/script_import_jobs/callable_source/target.js");
    for directive in ["", "\"use strict\";\n"] {
        // Real CRLF and Unicode line terminators exercise parser UTF-16 spans
        // independently of the byte-preserving export edits.
        let source = format!("{directive}/* 🟣 */\r\n// café\u{2028}{ENTRY}");
        assert_script(
            &[
                ("entry.js", ENTRY),
                ("target.js", TARGET),
                (
                    "other.js",
                    include_str!("../fixtures/script_import_jobs/callable_source/other.js"),
                ),
            ],
            &source,
            &["original callable sources"],
            ObservedJsValue::Boolean(true),
        );
    }
}

#[test]
fn script_source_jobs_reject_without_changing_global_scope_or_completion() {
    success(
        &[
            (
                "entry.js",
                r#"
var scriptMarker = 41;
const lexicalMarker = 7;
const request = import.source('./target.js');
if (!(request instanceof Promise) || this !== globalThis) throw 'Script import context';
request.then(() => { throw 'source fulfilled'; }, error => {
  if (!(error instanceof SyntaxError) || globalThis.scriptMarker !== 41 || lexicalMarker !== 7) throw 'Script source rejection';
  print('Script source target stayed idle');
});
true;
"#,
            ),
            (
                "target.js",
                "import './absent-child.js'; throw 'source target evaluated';",
            ),
        ],
        &["Script source target stayed idle"],
    );
}

#[test]
fn original_source_import_grammar_body_keeps_all_three_fixtures_in_both_script_modes() {
    const SOURCE: &str = include_str!(
        "../../../../test262/vendor/test262/test/language/module-code/source-phase-import/import-source.js"
    );
    const FIRST: &str = include_str!(
        "../../../../test262/vendor/test262/test/language/module-code/source-phase-import/import-source-binding-name_FIXTURE.js"
    );
    const SECOND: &str = include_str!(
        "../../../../test262/vendor/test262/test/language/module-code/source-phase-import/import-source-binding-name-2_FIXTURE.js"
    );
    const NEWLINES: &str = include_str!(
        "../../../../test262/vendor/test262/test/language/module-code/source-phase-import/import-source-newlines_FIXTURE.js"
    );
    const ENSURE: &str = include_str!(
        "../../../../test262/vendor/test262/test/language/module-code/source-phase-import/ensure-linking-error_FIXTURE.js"
    );
    const STA: &str = include_str!("../../../../test262/vendor/test262/harness/sta.js");
    const ASSERT: &str = include_str!("../../../../test262/vendor/test262/harness/assert.js");
    const DONE: &str =
        include_str!("../../../../test262/vendor/test262/harness/doneprintHandle.js");
    const ASYNC: &str = include_str!("../../../../test262/vendor/test262/harness/asyncHelpers.js");
    let files = [
        ("import-source.js", SOURCE),
        ("import-source-binding-name_FIXTURE.js", FIRST),
        ("import-source-binding-name-2_FIXTURE.js", SECOND),
        ("import-source-newlines_FIXTURE.js", NEWLINES),
        ("ensure-linking-error_FIXTURE.js", ENSURE),
    ];
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{STA}\n{ASSERT}\n{DONE}\n{ASYNC}\n{SOURCE}");
        assert_script(
            &files,
            &source,
            &["Test262:AsyncTestComplete"],
            ObservedJsValue::Undefined,
        );
    }
}

#[test]
fn contextual_source_bindings_keep_host_parse_source_errors_and_ordinary_imports_distinct() {
    success(
        &[
            (
                "entry.js",
                r#"
const NativeTypeError = TypeError, NativeSyntaxError = SyntaxError;
globalThis.invalidBody = false;
globalThis.TypeError = function() { throw 'mutable TypeError observed'; };
globalThis.SyntaxError = function() { throw 'mutable SyntaxError observed'; };
function rejects(promise, Type, message) { return promise.then(
  () => { throw 'invalid closure fulfilled'; },
  error => {
    if (!(error instanceof Type) || !error.message.includes(message)) throw 'wrong failure owner';
  }
); }
const checks = [
  rejects(import('./host.js'), NativeTypeError, '<do not resolve>'),
  rejects(import('./escaped-host.js'), NativeTypeError, '<do not resolve>'),
  rejects(import('./invalid.js'), NativeSyntaxError, 'from'),
  rejects(import('./resolved.js'), NativeSyntaxError, 'no source representation'),
  rejects(import.source('./value.js'), NativeSyntaxError, 'no source representation'),
  import('./default.js').then(ns => { if (ns.default !== 42) throw 'ordinary default source binding'; }),
  import('./escaped-default.js').then(ns => { if (ns.default !== 42) throw 'escaped ordinary default binding'; }),
  import('./mixed-default.js').then(ns => { if (ns.default !== 43) throw 'ordinary default/named list'; })
];
Promise.all(checks).then(() => {
  if (globalThis.invalidBody) throw 'invalid closure evaluated';
  print('contextual source grammar and error owners');
});
true;
"#,
            ),
            (
                "host.js",
                "import './ensure.js'; import source source from '<do not resolve>'; import source from from '<do not resolve>'; globalThis.invalidBody = true;",
            ),
            (
                "escaped-host.js",
                r"import source \u0066rom /* delimiter */ from '<do not resolve>'; globalThis.invalidBody = true;",
            ),
            (
                "ensure.js",
                "import {missing} from './ensure.js'; globalThis.invalidBody = true;",
            ),
            ("invalid.js", r"import source from fr\u006fm './value.js';"),
            (
                "resolved.js",
                "import source\r\nfrom\u{2028}from './value.js'; globalThis.invalidBody = true;",
            ),
            (
                "default.js",
                "import source from './value.js'; export default source;",
            ),
            (
                "escaped-default.js",
                r"import sour\u0063e from './value.js'; export default source;",
            ),
            (
                "mixed-default.js",
                "import source, {named as value} from './value.js'; export default source + value;",
            ),
            ("value.js", "export default 42; export const named = 1;"),
        ],
        &["contextual source grammar and error owners"],
    );
}

#[test]
fn pinned_import_usage_from_eval_preserves_both_original_modes() {
    const SOURCE: &str = include_str!(
        "../../../../test262/vendor/test262/test/language/expressions/dynamic-import/usage-from-eval.js"
    );
    const MODULE: &str = include_str!(
        "../../../../test262/vendor/test262/test/language/expressions/dynamic-import/module-code_FIXTURE.js"
    );
    const STA: &str = include_str!("../../../../test262/vendor/test262/harness/sta.js");
    const ASSERT: &str = include_str!("../../../../test262/vendor/test262/harness/assert.js");
    const DONE: &str =
        include_str!("../../../../test262/vendor/test262/harness/doneprintHandle.js");
    let files = [
        ("usage-from-eval.js", SOURCE),
        ("module-code_FIXTURE.js", MODULE),
    ];
    for directive in ["", "\"use strict\";\n"] {
        assert_script(
            &files,
            &format!("{directive}{STA}\n{ASSERT}\n{DONE}\n{SOURCE}"),
            &["Test262:AsyncTestComplete"],
            // The final .then($DONE, $DONE) expression completes with its Promise.
            ObservedJsValue::Object,
        );
    }
}

#[test]
fn nested_direct_eval_imports_keep_script_base_jobs_and_namespace_identity() {
    const ENTRY: &str = r#"
var evaluations = 0, order = [];
const first = eval('eval("import(\\"./target.js\\");")');
const second = eval('import("./target.js")');
if (first === second || Promise.resolve(first) !== first ||
    Object.getPrototypeOf(first) !== Promise.prototype) throw 'intrinsic fresh promise';
if (evaluations !== 0) throw 'eager prepared import';
order.push('entry');
Promise.all([first, second]).then(function (namespaces) {
    if (namespaces[0] !== namespaces[1] || namespaces[0].value !== 42 ||
        evaluations !== 1 || order.join(',') !== 'entry,target') throw 'canonical module jobs';
    print('prepared imports share one namespace');
});
print('imports queued');
true;
"#;
    for directive in ["", "\"use strict\";\n"] {
        let entry = format!("{directive}{ENTRY}");
        success(
            &[
                ("dir/entry.js", &entry),
                (
                    "dir/target.js",
                    "evaluations++; order.push('target'); export const value = 42;",
                ),
                ("target.js", "throw 'wrong Script base';"),
            ],
            &["imports queued", "prepared imports share one namespace"],
        );
    }
}

#[test]
fn prepared_eval_import_rejections_keep_cached_values_and_one_evaluation() {
    const ENTRY: &str = r#"
var attempts = 0, failure = { marker: 42 };
const first = eval('import("./fail.js")');
const second = eval('eval("import(\\"./fail.js\\");")');
function fulfilled() { throw 'rejected import fulfilled'; }
Promise.all([first.then(fulfilled, x => x), second.then(fulfilled, x => x)]).then(function (values) {
    if (values[0] !== failure || values[1] !== failure || attempts !== 1) throw 'cached module rejection';
    print('one exact module rejection');
});
if (attempts !== 0) throw 'synchronous target evaluation';
print('rejections queued');
true;
"#;
    for directive in ["", "\"use strict\";\n"] {
        let entry = format!("{directive}{ENTRY}");
        success(
            &[
                ("entry.js", &entry),
                ("fail.js", "attempts++; throw failure;"),
            ],
            &["rejections queued", "one exact module rejection"],
        );
    }
}

#[test]
fn unproved_eval_imports_keep_typed_capability_rejection_with_catalogued_targets() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let fixture = Modules(std::env::temp_dir().join(format!(
        "scriptw-prepared-import-bounds-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&fixture.0).unwrap();
    std::fs::write(fixture.0.join("target.js"), "export const value = 42;").unwrap();
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for (module, body) in [
        (
            false,
            r#"void import('./target.js'); var specifier = './target.js'; try { eval('import(specifier)/*' + Math.random() + '*/'); } catch (_) {} true;"#,
        ),
        (
            true,
            r#"void import('./target.js'); try { eval('import("./target.js")/*' + Math.random() + '*/'); } catch (_) {}"#,
        ),
    ] {
        // Merely authoring a nested import stays executable, and an abrupt
        // specifier operand keeps its exact caller throw before capability11.
        let idle = r#"void import('./target.js'); var marker = {}, side = 0;
var escaped = eval('({caller:marker,later:function(){return import("./target.js");}})/*' + Math.random() + '*/');
if (escaped.caller !== marker || typeof escaped.later !== 'function') throw 'unused import lost caller';
try { eval('import((function(){throw marker})(), side++)/*' + Math.random() + '*/'); throw 'missing operand throw'; }
catch (error) { if (error !== marker || side !== 0) throw 'operand effects preceded actual import boundary'; }
true;"#;
        std::fs::write(fixture.0.join("entry.js"), idle).unwrap();
        let engine = Engine::new(RealmBuilder::new().build());
        let idle_options = CompileOptions {
            filename: Some(fixture.0.join("entry.js").to_str().unwrap().into()),
            module_root: Some(fixture.0.to_str().unwrap().into()),
            ..CompileOptions::default()
        };
        let idle_run = RunOptions {
            backend: ExecutionBackend::WasmAot,
            timeout_ms: Some(30_000),
            ..RunOptions::default()
        };
        let idle_result = if module {
            engine.run_module(idle, idle_options, idle_run)
        } else {
            engine.run_script(idle, idle_options, idle_run)
        }
        .expect("unexecuted external import and actual operand throw preserve caller semantics");
        assert_eq!(idle_result.backend_used, ExecutionBackend::WasmAot);
        std::fs::write(fixture.0.join("entry.js"), body).unwrap();
        let engine = Engine::new(RealmBuilder::new().build());
        let options = CompileOptions {
            filename: Some(fixture.0.join("entry.js").to_str().unwrap().into()),
            module_root: Some(fixture.0.to_str().unwrap().into()),
            ..CompileOptions::default()
        };
        let run = RunOptions {
            backend: ExecutionBackend::WasmAot,
            timeout_ms: Some(30_000),
            ..RunOptions::default()
        };
        let result = if module {
            engine.run_module(body, options, run)
        } else {
            engine.run_script(body, options, run)
        };
        let error = result
            .expect_err("unknown import operands/origins cannot become catchable JS failures");
        assert!(
            error.runtime_dynamic_source_operations().is_empty(),
            "{error}"
        );
        assert_eq!(
            error.runtime_unavailable_capabilities(),
            vec![scriptw_ir::RuntimeUnavailableCapability::ExternalCallerImport],
            "{error}"
        );
        assert_eq!(
            error.wasm_execution_failure_kind(),
            Some(scriptw_engine::WasmExecutionFailureKind::UnavailableCapability)
        );
        assert_eq!(error.wasm_javascript_exception_constructor_name(), None);
        assert!(error.parse_diagnostic().is_none(), "{error}");
        assert!(error.ir_diagnostic().is_none(), "{error}");
    }
}

#[test]
fn prepared_import_options_keep_live_reentry_abrupt_values_and_private_helper_ownership() {
    const ENTRY: &str = r#"
var effects = [], evaluations = 0, inner, marker = { marker: 42 };
Object.defineProperty(globalThis, '$scriptw$module$importOptions', {
  get() { throw 'private validation helper used a global property'; }
});
var options = { get with() {
  effects.push('with');
  inner = eval('import("./target.js")');
  return {};
} };
var first = eval('import("./target.js", options)');
options = { get with() { effects.push('throw'); throw marker; } };
var rejected = eval('import("./target.js", options)');
if (first === inner || first === rejected ||
    Promise.resolve(first) !== first || Promise.resolve(rejected) !== rejected ||
    effects.join(',') !== 'with,throw' || evaluations !== 0) throw 'live options timing';
effects.push('entry');
Promise.all([first, inner, rejected.then(function () { throw 'abrupt options fulfilled'; }, x => x)])
  .then(function (values) {
    if (values[0] !== values[1] || values[0].value !== 42 ||
        values[2] !== marker || evaluations !== 1 ||
        effects.join(',') !== 'with,throw,entry,target') throw 'owned prepared import jobs';
    print('owned options and canonical import jobs');
  });
print('prepared options queued');
true;
"#;
    for directive in ["", "\"use strict\";\n"] {
        let entry = format!("{directive}{ENTRY}");
        success(
            &[
                ("entry.js", &entry),
                (
                    "target.js",
                    "evaluations++; effects.push('target'); export const value = 42;",
                ),
            ],
            &[
                "prepared options queued",
                "owned options and canonical import jobs",
            ],
        );
    }
}
