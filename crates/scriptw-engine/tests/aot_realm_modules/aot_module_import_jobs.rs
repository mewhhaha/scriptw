use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};
use scriptw_ir::{EarlyErrorCode, IrDiagnosticPhase, NativeErrorKind};

struct Modules(PathBuf);
impl Drop for Modules {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
impl Modules {
    fn new(files: &[(&str, &str)]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let fixture = Self(std::env::temp_dir().join(format!(
            "scriptw-module-import-jobs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        std::fs::create_dir_all(&fixture.0).expect("create fixture");
        for (name, source) in files {
            std::fs::write(fixture.0.join(name), source).expect("write fixture");
        }
        fixture
    }
    fn options(&self) -> CompileOptions {
        CompileOptions {
            filename: Some(self.0.join("entry.js").to_str().unwrap().into()),
            module_root: Some(self.0.to_str().unwrap().into()),
            host_surface_policy: HostSurfacePolicy::Test262,
            ..CompileOptions::default()
        }
    }
}
fn assert_modules(files: &[(&str, &str)], expected: &[&str]) {
    let fixture = Modules::new(files);
    scriptw_engine::configure_compilation_jobs(1).expect("bounded compiler");
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_module(
            files[0].1,
            fixture.options(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("AOT module graph executes");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        expected
            .iter()
            .map(|line| HostOutputEvent::PrintLine((*line).into()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn mixed_generator_import_completes_raw_operands_before_capability_and_module_job() {
    assert_modules(
        &[
            (
                "entry.js",
                r#"
import defer * as registered from './value.js';
var events=[],whole={tag:'whole'},calls=0;globalThis.mixedImportEvaluations=0;
function check(condition,label){if(!condition)throw label;}
var specifier={[Symbol.toPrimitive]:function(){events.push('specifier');return './value.js';}};
var options={get with(){events.push('options');return {};}};
async function* values(){const result=import(await(yield specifier),await(yield options));yield result;}
async function* failed(){try{yield import(await(yield specifier),await(yield options));}catch(error){check(error===whole,'whole-import-options-rejection');yield 'caught';}}
async function run(){
  var iterator=values();check((await iterator.next()).value===specifier&&events.length===0,'raw-specifier-first-yield');
  check((await iterator.next(specifier)).value===options&&events.length===0,'raw-options-complete-before-tostring-or-option-get');
  gc();var result=await iterator.next(options);
  check(result.value.value===59&&events.join(',')==='specifier,options'&&globalThis.mixedImportEvaluations===1,'original-capability-job-and-module-evaluation');
  check(registered.value===59&&globalThis.mixedImportEvaluations===1,'one-original-module-instance');await iterator.next();
  events=[];var rejected={get with(){events.push('rejected-options');throw whole;}};
  iterator=failed();await iterator.next();await iterator.next(specifier);
  check((await iterator.next(rejected)).value==='caught'&&events.join(',')==='specifier,rejected-options','original-import-rejects-after-completed-source-operands');await iterator.next();
}
run().then(function(){print('mixed-import:ok');},function(error){print(error);throw error;});
"#,
            ),
            (
                "value.js",
                "globalThis.mixedImportEvaluations++; export const value=59;",
            ),
        ],
        &["mixed-import:ok"],
    );
}

#[test]
fn ordinary_import_waits_for_its_job_and_evaluates_once() {
    assert_modules(
        &[
            (
                "entry.js",
                r#"
import defer * as deferred from './value.js';
globalThis.evaluations = 0;
if (false) import('./unreached.js');
const first = import('./value.js');
const second = import('./value.js');
if (first === second || globalThis.evaluations !== 0) throw 'call scheduling';
Promise.all([first, second]).then(values => {
  if (values[0] !== values[1] || values[0] === deferred) throw 'namespace identity';
  if (values[0].value !== 7 || deferred.value !== 7 || globalThis.evaluations !== 1) throw 'evaluate once';
  print('ok');
});
"#,
            ),
            (
                "value.js",
                "globalThis.evaluations++; export const value = 7;",
            ),
            ("unreached.js", "throw 'unreached import ran';"),
        ],
        &["ok"],
    );
}

#[test]
fn ordinary_generator_import_retains_raw_operands_then_yields_the_original_promise() {
    assert_modules(
        &[
            (
                "entry.js",
                r#"
import defer * as registered from './value.js';
var events=[];globalThis.ordinaryImportEvaluations=0;
function check(value,label){if(!value)throw label;}
var specifier={[Symbol.toPrimitive]:function(){events.push('specifier');return './value.js';}};
var options={get with(){events.push('options');return {};}};
function* values(){const result=import(yield specifier,yield options);yield result;}
async function run(){
  var iterator=values();check(iterator.next().value===specifier&&events.length===0,'raw-specifier');
  check(iterator.next(specifier).value===options&&events.length===0,'raw-options-before-conversion');
  gc();var promise=iterator.next(options).value;
  check(promise instanceof Promise&&events.join(',')==='specifier,options','ordinary-yield-retains-import-promise');
  var namespace=await promise;
  check(namespace.value===61&&registered.value===61&&globalThis.ordinaryImportEvaluations===1,'original-module-job-and-instance');
  check(iterator.next().done,'completed-ordinary-generator');
}
run().then(function(){print('ordinary-generator-import:ok');},function(error){print(error);throw error;});
"#,
            ),
            (
                "value.js",
                "globalThis.ordinaryImportEvaluations++;export const value=61;",
            ),
        ],
        &["ordinary-generator-import:ok"],
    );
}

#[test]
fn cached_evaluation_error_survives_both_deferred_namespace_orders() {
    for entry in [
        r#"
import defer * as ns from './throws.js';
print('entry');
import('./throws.js').then(() => { throw 'fulfilled'; }, error => {
  let again; try { ns.missing; } catch (caught) { again = caught; }
  if (error !== again || error.marker !== 7) throw 'cached abrupt identity';
  print('ok');
});
print('scheduled');
"#,
        r#"
print('entry');
import('./throws.js').then(() => { throw 'fulfilled'; }, error => {
  return import('./wrapper.js').then(wrapper => {
    let again; try { wrapper.ns.missing; } catch (caught) { again = caught; }
    if (error !== again || error.marker !== 7) throw 'cached abrupt identity';
    print('ok');
  });
});
print('scheduled');
"#,
    ] {
        assert_modules(
            &[
                ("entry.js", entry),
                ("throws.js", "throw { marker: 7 };"),
                (
                    "wrapper.js",
                    "import defer * as ns from './throws.js'; export { ns };",
                ),
            ],
            &["entry", "scheduled", "ok"],
        );
    }
}

#[test]
fn dynamic_load_and_link_errors_reject_without_running_invalid_closures() {
    assert_modules(
        &[
            (
                "entry.js",
                r#"
globalThis.sharedCount = 0;
globalThis.invalidBody = false;
const checks = [[import('./syntax.js'), SyntaxError], [import('./missing.js'), TypeError], [import('./export.js'), SyntaxError]].map(([promise, ErrorType]) => promise.then(() => { throw 'fulfilled invalid graph'; }, error => {
  if (!(error instanceof ErrorType)) throw 'wrong rejection type';
}));
checks.push(import('./valid.js').then(ns => {
  if (ns.value !== 7) throw 'valid shared dependency';
}));
Promise.all(checks).then(() => {
  if (globalThis.sharedCount !== 1 || globalThis.invalidBody) throw 'invalid graph evaluated';
  print('ok');
});
"#,
            ),
            (
                "syntax.js",
                "import defer * as ns from './invalid.js'; globalThis.invalidBody = true;",
            ),
            ("invalid.js", "invalid syntax!"),
            (
                "missing.js",
                "import './shared.js'; import defer * as ns from './absent.js'; globalThis.invalidBody = true;",
            ),
            (
                "export.js",
                "import { missing } from './shared.js'; globalThis.invalidBody = true;",
            ),
            ("valid.js", "export { value } from './shared.js';"),
            (
                "shared.js",
                "globalThis.sharedCount++; export const value = 7;",
            ),
        ],
        &["ok"],
    );
}

#[test]
fn direct_dynamic_parse_failure_is_a_promise_rejection() {
    assert_modules(
        &[
            (
                "entry.js",
                r#"
let promise;
try { promise = import.defer('./invalid.js'); } catch (_) { throw 'synchronous throw'; }
promise.then(() => { throw 'fulfilled'; }, error => {
  if (!(error instanceof SyntaxError)) throw 'wrong rejection';
  print('ok');
});
"#,
            ),
            ("invalid.js", "invalid syntax!"),
        ],
        &["ok"],
    );
}

#[test]
fn evaluation_fulfillment_and_abrupt_completion_use_the_second_reaction() {
    for target in [
        "globalThis.events.push('body'); export const value = 7;",
        "globalThis.events.push('body'); throw 7;",
    ] {
        assert_modules(
            &[
                (
                    "entry.js",
                    r#"
globalThis.events = [];
const result = import('./target.js');
events.push('entry');
Promise.resolve().then(() => events.push('tick1')).then(() => events.push('tick2')).then(() => {
  events.push('tick3');
  if (events.join(',') !== 'entry,body,tick1,tick2,settled,tick3') throw events.join(',');
  print('ok');
});
result.then(() => events.push('settled'), error => {
  if (error !== 7) throw 'wrong error';
  events.push('settled');
});
"#,
                ),
                ("target.js", target),
            ],
            &["ok"],
        );
    }
}

#[test]
fn synchronous_defer_settles_in_the_load_reaction_without_evaluation() {
    assert_modules(
        &[
            (
                "entry.js",
                r#"
globalThis.events = [];
const result = import.defer('./target.js');
events.push('entry');
Promise.resolve().then(() => events.push('tick1')).then(() => {
  events.push('tick2');
  if (events.join(',') !== 'entry,tick1,settled,body,tick2') throw events.join(',');
  print('ok');
});
result.then(ns => { events.push('settled'); ns.missing; });
"#,
            ),
            ("target.js", "globalThis.events.push('body');"),
        ],
        &["ok"],
    );
}

#[test]
fn import_uses_intrinsics_after_mutable_global_promise_and_reflection_changes() {
    assert_modules(
        &[
            (
                "entry.js",
                r#"
const OriginalPromise = Promise;
const originalThen = Promise.prototype.then;
const OriginalTypeError = TypeError;
const OriginalSyntaxError = SyntaxError;
Promise.prototype.then = function () { throw 'observable internal then'; };
globalThis.Promise = function () { throw 'observable global Promise'; };
globalThis.TypeError = function () { throw 'observable global TypeError'; };
globalThis.SyntaxError = function () { throw 'observable global SyntaxError'; };
Reflect.ownKeys = function () { throw 'observable Reflect.ownKeys'; };
Object.getOwnPropertyDescriptor = function () { throw 'observable descriptor lookup'; };
const result = import('./value.js', { with: {} });
if (!(result instanceof OriginalPromise)) throw 'wrong promise intrinsic';
let completed = 0;
function finish() { completed++; if (completed === 3) print('ok'); }
originalThen.call(result, ns => { if (ns.value !== 7) throw 'wrong value'; finish(); });
originalThen.call(import('./value.js', null), () => { throw 'bad options fulfilled'; }, error => {
  if (!(error instanceof OriginalTypeError)) throw 'wrong options error'; finish();
});
originalThen.call(import('./invalid.js'), () => { throw 'bad syntax fulfilled'; }, error => {
  if (!(error instanceof OriginalSyntaxError)) throw 'wrong syntax error'; finish();
});
"#,
            ),
            ("value.js", "export const value = 7;"),
            ("invalid.js", "invalid syntax!"),
        ],
        &["ok"],
    );
}

#[test]
fn operand_evaluation_and_conversion_precede_import_continuations() {
    assert_modules(
        &[
            (
                "entry.js",
                r#"
globalThis.events = [];
if (false) import('./value.js');
function specifier() { events.push('specifier'); return { toString() { events.push('toString'); return './value.js'; } }; }
function options() { events.push('options'); return { get with() { events.push('with'); return {}; } }; }
const result = import(specifier(), options());
events.push('returned');
if (events.join(',') !== 'specifier,options,toString,with,returned') throw 'conversion order';
result.then(ns => {
  if (ns.value !== 7 || events.join(',') !== 'specifier,options,toString,with,returned,body') throw 'job order';
  print('ok');
});
"#,
            ),
            (
                "value.js",
                "globalThis.events.push('body'); export const value = 7;",
            ),
        ],
        &["ok"],
    );
}

#[test]
fn static_deferred_dependency_syntax_fails_during_compile() {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let files = [
        (
            "entry.js",
            "import defer * as ns from './invalid.js'; print('unreachable');",
        ),
        ("invalid.js", "invalid syntax!"),
    ];
    let fixture = Modules::new(&files);
    let error = Engine::new(RealmBuilder::new().build())
        .compile_module(files[0].1, fixture.options())
        .expect_err("static dependency rejects before evaluation");
    let diagnostic = error.ir_diagnostic().expect("structured rejection");
    assert_eq!(diagnostic.code(), Some(EarlyErrorCode::ModuleSyntax));
    assert_eq!(diagnostic.phase(), IrDiagnosticPhase::Resolution);
    assert_eq!(diagnostic.error_type(), Some(NativeErrorKind::SyntaxError));
}

#[test]
fn dynamic_host_load_errors_precede_link_errors_and_preserve_source_phase_syntax() {
    assert_modules(
        &[
            (
                "entry.js",
                r#"
const NativeTypeError = TypeError, NativeSyntaxError = SyntaxError;
globalThis.invalidBody = false;
globalThis.TypeError = function () { throw 'public TypeError constructor observed'; };
globalThis.SyntaxError = function () { throw 'public SyntaxError constructor observed'; };
function rejectWith(promise, ErrorType) { return promise.then(
  () => { throw 'invalid graph fulfilled'; },
  error => { if (!(error instanceof ErrorType)) throw 'wrong intrinsic rejection'; }
); }
const checks = [
  rejectWith(import('./host.js'), NativeTypeError),
  rejectWith(import.source('./host.js'), NativeSyntaxError),
  rejectWith(import('./parse.js'), NativeSyntaxError),
  rejectWith(import('./link.js'), NativeSyntaxError)
];
Promise.all(checks).then(() => {
  if (globalThis.invalidBody) throw 'invalid closure evaluated';
  print('host-load-kind:ok');
});
"#,
            ),
            (
                "host.js",
                "import './ensure.js'; import source source from '<do not resolve>'; import source from from '<do not resolve>'; globalThis.invalidBody = true;",
            ),
            (
                "ensure.js",
                "import {missing} from './ensure.js'; globalThis.invalidBody = true;",
            ),
            ("parse.js", "invalid syntax!"),
            (
                "link.js",
                "import {missing} from './value.js'; globalThis.invalidBody = true;",
            ),
            (
                "value.js",
                "export const value = 1; globalThis.invalidBody = true;",
            ),
        ],
        &["host-load-kind:ok"],
    );
}
