use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, ObservedCompletion, RealmBuilder,
    RunOptions,
};

struct Modules(PathBuf);

impl Drop for Modules {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

enum EntryGoal {
    Module,
    Script,
}

#[test]
fn parsed_module_boundaries_preserve_regex_and_default_declaration_asi() {
    assert_modules(
        &[
            (
                "entry.js",
                "import fn from './function.js'; import C from './class.js'; print(fn() + ':' + C.value);",
            ),
            (
                "function.js",
                "if (true) {} /export/.test('export'); export default function /* original */ () { return 17; }\n[1].forEach(value => print('function:' + value));",
            ),
            (
                "class.js",
                "if (true) /}/.test('}'); export default class { static value = 23; }\n[2].forEach(value => print('class:' + value));",
            ),
        ],
        EntryGoal::Module,
        &["function:1", "class:2", "17:23"],
    );
}

#[test]
fn parser_dynamic_heads_keep_regex_and_import_methods_in_script_and_module() {
    for goal in [EntryGoal::Script, EntryGoal::Module] {
        assert_modules(
            &[
                (
                    "entry.js",
                    "if (true) {} /import/.test('import'); if (true) /}/.test('}'); const object = { import() { return 7; } }; import('./value.js').then(ns => print(ns.value + object.import()));",
                ),
                ("value.js", "export const value = 35;"),
            ],
            goal,
            &["42"],
        );
    }
}

#[test]
fn dynamic_heads_survive_default_declaration_terminator_insertion() {
    assert_modules(
        &[
            ("entry.js", "import fn from './fn.js'; print(fn());"),
            (
                "fn.js",
                "export default function () { return 4; }\nimport('./value.js').then(ns => print(ns.value));",
            ),
            ("value.js", "export const value = 9;"),
        ],
        EntryGoal::Module,
        &["4", "9"],
    );
}

fn assert_modules(files: &[(&str, &str)], goal: EntryGoal, expected: &[&str]) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let fixture = Modules(std::env::temp_dir().join(format!(
        "scriptw-default-export-names-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&fixture.0).unwrap();
    for (name, source) in files {
        std::fs::write(fixture.0.join(name), source).unwrap();
    }
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    let engine = Engine::new(RealmBuilder::new().build());
    let options = CompileOptions {
        filename: Some(fixture.0.join(files[0].0).to_str().unwrap().into()),
        module_root: Some(fixture.0.to_str().unwrap().into()),
        ..CompileOptions::default()
    };
    let run = RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    };
    let observed = match goal {
        EntryGoal::Module => engine.observe_module(files[0].1, options, run),
        EntryGoal::Script => engine.observe_script(files[0].1, options, run),
    }
    .expect("default export graph compiles and executes through Wasm");
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
fn default_class_name_exists_before_static_fields_and_blocks() {
    assert_modules(
        &[
            (
                "entry.js",
                "import first from './first.js'; import second from './second.js'; print(first.name + ':' + second.name);",
            ),
            (
                "first.js",
                "export default class { static field = print('field:' + this.name); static { print('block:' + this.name); let d = Object.getOwnPropertyDescriptor(this, 'name'); print(d.value + ':' + d.writable + ':' + d.enumerable + ':' + d.configurable); } } print('after class');",
            ),
            (
                "second.js",
                "export default (class { constructor() {} static { print('explicit:' + this.name); } });",
            ),
        ],
        EntryGoal::Module,
        &[
            "field:default",
            "block:default",
            "default:false:false:true",
            "after class",
            "explicit:default",
            "default:default",
        ],
    );
}

#[test]
fn anonymous_function_protocols_and_parenthesized_expressions_receive_default() {
    assert_modules(
        &[
            (
                "entry.js",
                "import a from './a.js'; import b from './b.js'; import c from './c.js'; import d from './d.js'; import e from './e.js'; import f from './f.js'; import g from './g.js'; print(a.name + ':' + b.name + ':' + c.name + ':' + d.name + ':' + e.name + ':' + f.name + ':' + g.name); print(a() + ':' + b().next().value + ':' + e() + ':' + g()); Promise.all([c(), d().next(), f()]).then(v => print(v[0] + ':' + v[1].value + ':' + v[2]));",
            ),
            ("a.js", "export default function () { return 1; }"),
            ("b.js", "export default function* () { yield 2; }"),
            ("c.js", "export default async function () { return 3; }"),
            ("d.js", "export default async function* () { yield 4; }"),
            ("e.js", "export default (() => 5);"),
            ("f.js", "export default (async () => 6);"),
            ("g.js", "export default (function () { return 7; });"),
        ],
        EntryGoal::Module,
        &[
            "default:default:default:default:default:default:default",
            "1:2:5:7",
            "3:4:6",
        ],
    );
}

#[test]
fn explicit_names_non_definitions_and_static_name_overrides_are_preserved() {
    assert_modules(
        &[
            (
                "entry.js",
                "import a from './a.js'; import b from './b.js'; import c from './c.js'; import d from './d.js'; import e from './e.js'; import f from './f.js'; import g from './g.js'; print(a.name + ':' + b.name + ':' + c.name + ':' + d.name + ':' + e.name()); print(f.name); print('constructed:[' + g.name + ']:' + new g().value); let bare = [class { static { print('array:[' + this.name + ']'); } }][0]; print('array descriptor:[' + Object.getOwnPropertyDescriptor(bare, 'name').value + ']');",
            ),
            (
                "a.js",
                "export default class Named { static { print(this.name); } }",
            ),
            ("b.js", "class Exported {} export { Exported as default };"),
            ("c.js", "export default (function Explicit() {});"),
            (
                "d.js",
                "export default (0, class { static { print('anonymous:[' + this.name + ']'); } });",
            ),
            (
                "e.js",
                "export default class { static name() { return 'override'; } static { print(this.name()); } }",
            ),
            ("f.js", "export default (function $d6$() {});"),
            (
                "g.js",
                "export default (0, class { constructor() { this.value = 5; } static { print('constructor:[' + this.name + ']'); } });",
            ),
        ],
        EntryGoal::Module,
        &[
            "Named",
            "anonymous:[]",
            "override",
            "constructor:[]",
            "Named:Exported:Explicit::override",
            "$d6$",
            "constructed:[]:5",
            "array:[]",
            "array descriptor:[]",
        ],
    );
}

#[test]
fn utf16_offsets_and_nested_same_spelled_bindings_keep_distinct_names() {
    assert_modules(
        &[
            (
                "entry.js",
                "// 🦀é\r\nimport value from './value.js'; print(value.name);",
            ),
            (
                "value.js",
                "const prefix = '🦀é';\r\nvoid import.meta;\r\nexport default class { static { let $d1$ = class { static { print('nested:' + this.name); } }; print(prefix + ':' + this.name + ':' + $d1$.name); } }print('after unicode class');",
            ),
        ],
        EntryGoal::Module,
        &[
            "nested:$d1$",
            "🦀é:default:$d1$",
            "after unicode class",
            "default",
        ],
    );
}

#[test]
fn asynchronous_module_wrapper_preserves_default_definition_identity() {
    assert_modules(
        &[
            (
                "entry.js",
                "import value from './value.js'; print(value.name);",
            ),
            (
                "value.js",
                "await 0; export default class { static { print('await:' + this.name); } }",
            ),
        ],
        EntryGoal::Module,
        &["await:default", "default"],
    );
}

#[test]
fn deferred_module_thunk_names_the_class_before_first_access_finishes() {
    assert_modules(
        &[
            (
                "entry.js",
                "import defer * as ns from './value.js'; print('before'); print(ns.default.name); print(ns.default.name);",
            ),
            (
                "value.js",
                "export default class { static { print('deferred:' + this.name); } }",
            ),
        ],
        EntryGoal::Module,
        &["before", "deferred:default", "default", "default"],
    );
}

#[test]
fn script_import_wrapper_and_length_changing_rewrites_preserve_names() {
    assert_modules(
        &[
            (
                "entry.js",
                "import('./value.js').then(ns => print('import:' + ns.default.name));",
            ),
            (
                "value.js",
                "void import('./other.js'); export default class { static { print('script:' + this.name); } }void import('./other.js');",
            ),
            ("other.js", "export const other = 1;"),
        ],
        EntryGoal::Script,
        &["script:default", "import:default"],
    );
}

#[test]
fn anonymous_default_declarations_are_callable_before_their_source_statement() {
    assert_modules(
        &[
            (
                "entry.js",
                "import './ordinary.js'; import './generator.js'; import './async.js'; import './async-generator.js'; print('complete');",
            ),
            (
                "ordinary.js",
                "import fOrdinary from './ordinary.js'; const beforeOrdinary = fOrdinary; beforeOrdinary.marker = 7; print('ordinary:' + fOrdinary() + ':' + fOrdinary.name); export default function /* original */ () { return 23; } print('ordinary identity:' + (beforeOrdinary === fOrdinary) + ':' + fOrdinary.marker + ':' + (Function.prototype.toString.call(fOrdinary) === 'function /* original */ () { return 23; }'));",
            ),
            (
                "generator.js",
                "import fGenerator from './generator.js'; const beforeGenerator = fGenerator; beforeGenerator.marker = 8; print('generator:' + fGenerator().next().value + ':' + fGenerator.name); export default function* /* original */ () { yield 24; } print('generator identity:' + (beforeGenerator === fGenerator) + ':' + fGenerator.marker + ':' + (Function.prototype.toString.call(fGenerator) === 'function* /* original */ () { yield 24; }'));",
            ),
            (
                "async.js",
                "import fAsync from './async.js'; const beforeAsync = fAsync; beforeAsync.marker = 9; fAsync().then(value => print('async:' + value)); export default async function /* original */ () { return 25; } print('async identity:' + (beforeAsync === fAsync) + ':' + fAsync.marker + ':' + fAsync.name + ':' + (Function.prototype.toString.call(fAsync) === 'async function /* original */ () { return 25; }'));",
            ),
            (
                "async-generator.js",
                "import fAsyncGenerator from './async-generator.js'; const beforeAsyncGenerator = fAsyncGenerator; beforeAsyncGenerator.marker = 10; fAsyncGenerator().next().then(result => print('async generator:' + result.value)); export default async function* /* original */ () { yield 26; } print('async generator identity:' + (beforeAsyncGenerator === fAsyncGenerator) + ':' + fAsyncGenerator.marker + ':' + fAsyncGenerator.name + ':' + (Function.prototype.toString.call(fAsyncGenerator) === 'async function* /* original */ () { yield 26; }'));",
            ),
        ],
        EntryGoal::Module,
        &[
            "ordinary:23:default",
            "ordinary identity:true:7:true",
            "generator:24:default",
            "generator identity:true:8:true",
            "async identity:true:9:default:true",
            "async generator identity:true:10:default:true",
            "complete",
            "async:25",
            "async generator:26",
        ],
    );
}

#[test]
fn cyclic_dependency_calls_the_default_declaration_before_exporter_evaluation() {
    assert_modules(
        &[
            (
                "entry.js",
                "import { original } from './dependency.js'; import f from './entry.js'; print('entry:' + (original === f) + ':' + f.marker); export default function () { return 23; } print('after:' + (original === f) + ':' + f.marker);",
            ),
            (
                "dependency.js",
                "import f from './entry.js'; print('dependency:' + f()); f.marker = 7; export const original = f;",
            ),
        ],
        EntryGoal::Module,
        &["dependency:23", "entry:true:7", "after:true:7"],
    );
}

#[test]
fn default_function_expressions_remain_uninitialized_until_evaluation() {
    assert_modules(
        &[
            (
                "entry.js",
                "import './ordinary.js'; import './async-generator.js'; import './comma.js'; import './generator-comma.js'; import './async-comma.js'; import './async-generator-comma.js'; print((function (callable) { return 'argument:[' + callable.name + ']'; })(function () {}));",
            ),
            (
                "ordinary.js",
                "import fOrdinary from './ordinary.js'; let caughtOrdinary = false; try { void fOrdinary; } catch (error) { caughtOrdinary = error instanceof ReferenceError; } print('ordinary TDZ:' + caughtOrdinary); export default (function () { return 27; }); print('ordinary:' + fOrdinary.name + ':' + fOrdinary());",
            ),
            (
                "async-generator.js",
                "import fAsyncGenerator from './async-generator.js'; let caughtAsyncGenerator = false; try { void fAsyncGenerator; } catch (error) { caughtAsyncGenerator = error instanceof ReferenceError; } print('async generator TDZ:' + caughtAsyncGenerator); export default (async function* () { yield 28; }); fAsyncGenerator().next().then(result => print('async generator:' + fAsyncGenerator.name + ':' + result.value));",
            ),
            (
                "comma.js",
                "import fComma from './comma.js'; let caughtComma = false; try { void fComma; } catch (error) { caughtComma = error instanceof ReferenceError; } print('comma TDZ:' + caughtComma); export default (0, function () { return 29; }); print('comma:[' + fComma.name + ']:' + fComma());",
            ),
            (
                "generator-comma.js",
                "import fGeneratorComma from './generator-comma.js'; export default (0, function* () { yield 30; }); print('generator comma:[' + fGeneratorComma.name + ']:' + (Function.prototype.toString.call(fGeneratorComma) === 'function* () { yield 30; }'));",
            ),
            (
                "async-comma.js",
                "import fAsyncComma from './async-comma.js'; export default (0, async function () { return 31; }); print('async comma:[' + fAsyncComma.name + ']:' + (Function.prototype.toString.call(fAsyncComma) === 'async function () { return 31; }'));",
            ),
            (
                "async-generator-comma.js",
                "import fAsyncGeneratorComma from './async-generator-comma.js'; export default (0, async function* () { yield 32; }); print('async generator comma:[' + fAsyncGeneratorComma.name + ']:' + (Function.prototype.toString.call(fAsyncGeneratorComma) === 'async function* () { yield 32; }'));",
            ),
        ],
        EntryGoal::Module,
        &[
            "ordinary TDZ:true",
            "ordinary:default:27",
            "async generator TDZ:true",
            "comma TDZ:true",
            "comma:[]:29",
            "generator comma:[]:true",
            "async comma:[]:true",
            "async generator comma:[]:true",
            "argument:[]",
            "async generator:default:28",
        ],
    );
}

#[test]
fn module_wrappers_initialize_default_declarations_in_their_owning_activation() {
    assert_modules(
        &[
            (
                "entry.js",
                "import f from './value.js'; print('entry:' + f.name + ':' + f());",
            ),
            (
                "value.js",
                "// 🦀é\r\nimport f from './value.js'; var answer = 31; print('before await:' + f()); await 0; export default function () { return answer; }",
            ),
        ],
        EntryGoal::Module,
        &["before await:31", "entry:default:31"],
    );
    assert_modules(
        &[
            (
                "entry.js",
                "import defer * as ns from './value.js'; print('before defer'); print('entry:' + ns.default.name + ':' + ns.default());",
            ),
            (
                "value.js",
                "let answer = 32; print('deferred body'); export default function () { return answer; }",
            ),
        ],
        EntryGoal::Module,
        &["before defer", "deferred body", "entry:default:32"],
    );
    assert_modules(
        &[
            (
                "entry.js",
                "import('./value.js').then(ns => print('entry:' + ns.default.name + ':' + ns.default()));",
            ),
            (
                "value.js",
                "import f from './value.js'; var answer = 33; print('script:' + f()); export default function () { return answer; }",
            ),
        ],
        EntryGoal::Script,
        &["script:33", "entry:default:33"],
    );
}
