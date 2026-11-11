use std::sync::Arc;

use scriptw_engine::{
    CompileOptions, EmbeddedModuleEntryInput, EmbeddedModuleGoal, EmbeddedModuleGraph,
    EmbeddedModuleReferrer, EmbeddedModuleResolutionInput, EmbeddedModuleSourceInput, Engine,
    ExecutionBackend, HostOutputEvent, HostSurfacePolicy, ModuleLoadingPolicy, ObservedCompletion,
    ObservedJsValue, ObservedNumber, RealmBuilder, RunOptions,
};

fn graph(
    goal: EmbeddedModuleGoal,
    source: &str,
    modules: &[(&str, &str, &str)],
    resolutions: Vec<EmbeddedModuleResolutionInput>,
) -> Arc<EmbeddedModuleGraph> {
    EmbeddedModuleGraph::try_new(
        EmbeddedModuleEntryInput {
            goal,
            identity: "entry.js".into(),
            source: source.into(),
            meta_url: "scriptw://observed/entry.js".into(),
        },
        modules
            .iter()
            .map(|(identity, source, url)| EmbeddedModuleSourceInput {
                identity: (*identity).into(),
                source: (*source).into(),
                meta_url: (*url).into(),
            })
            .collect(),
        resolutions,
    )
    .expect("finite graph is dependency sealed")
}

fn edge(
    specifier: &str,
    attributes: &[(&str, &str)],
    target: &str,
) -> EmbeddedModuleResolutionInput {
    EmbeddedModuleResolutionInput {
        referrer: EmbeddedModuleReferrer::Script("entry.js".into()),
        specifier: specifier.into(),
        attributes: attributes
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into()))
            .collect(),
        target: target.into(),
    }
}

fn assert_script(graph: Arc<EmbeddedModuleGraph>, output: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("bounded compiler jobs");
    let result = Engine::new(RealmBuilder::new().build())
        .observe_script(
            graph.entry().source(),
            CompileOptions {
                module_loading_policy: ModuleLoadingPolicy::Embedded(Arc::clone(&graph)),
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("embedded source executes through the actual Wasm AOT path");
    assert_eq!(result.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(
        result.completion,
        ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(262.0)))
    );
    assert_eq!(
        result.output_events,
        vec![HostOutputEvent::PrintLine(output.into())]
    );
}

#[test]
fn multiple_importers_keep_referrer_identity_and_synchronous_attribute_traps() {
    for directive in ["", "'use strict';\n"] {
        let source = format!(
            r#"{directive}
var events = [], marker = {{}};
function options(label) {{
  return {{ get with() {{
    events.push('with-' + label);
    return new Proxy({{ z: 'Z', a: 'A' }}, {{
      ownKeys(target) {{ events.push('keys-' + label); return ['z', Symbol(), 'a']; }},
      getOwnPropertyDescriptor(target, key) {{ events.push('desc-' + label + '-' + key); return Object.getOwnPropertyDescriptor(target, key); }},
      get(target, key) {{ events.push('get-' + label + '-' + key); return target[key]; }}
    }});
  }} }};
}}
Promise.all([import('caller-a'), import('caller-b')]).then(async function(callers) {{
  var first = callers[0].load(options('a'));
  if (events.join(',') !== 'with-a,keys-a,desc-a-z,get-a-z,desc-a-a,get-a-a') throw 'late attribute validation';
  var a = await first;
  var b = await callers[1].load(options('b'));
  if (a.value !== 41 || b.value !== 73 || a === b) throw 'lost referrer';
  var rejected = callers[0].load({{ get with() {{ events.push('abrupt'); throw marker; }} }});
  if (!(rejected instanceof Promise) || events[events.length - 1] !== 'abrupt') throw 'synchronous throw or late get';
  try {{ await rejected; throw 'fulfilled'; }} catch (error) {{ if (error !== marker) throw 'lost abrupt value'; }}
  print('imports:' + a.value + ',' + b.value + ':' + events.join(','));
}});
262;
"#
        );
        let module_edge = |referrer: &str, target: &str| EmbeddedModuleResolutionInput {
            referrer: EmbeddedModuleReferrer::Module(referrer.into()),
            specifier: "chosen".into(),
            attributes: vec![("a".into(), "A".into()), ("z".into(), "Z".into())],
            target: target.into(),
        };
        assert_script(
            graph(
                EmbeddedModuleGoal::Script,
                &source,
                &[
                    (
                        "a.js",
                        "export function load(options) { return import('chosen', options); }",
                        "scriptw://a.js",
                    ),
                    (
                        "b.js",
                        "export function load(options) { return import('chosen', options); }",
                        "scriptw://b.js",
                    ),
                    (
                        "value-a.js",
                        "export const value = 41;",
                        "scriptw://value-a.js",
                    ),
                    (
                        "value-b.js",
                        "export const value = 73;",
                        "scriptw://value-b.js",
                    ),
                ],
                vec![
                    edge("caller-a", &[], "a.js"),
                    edge("caller-b", &[], "b.js"),
                    module_edge("a.js", "value-a.js"),
                    module_edge("b.js", "value-b.js"),
                ],
            ),
            "imports:41,73:with-a,keys-a,desc-a-z,get-a-z,desc-a-a,get-a-a,with-b,keys-b,desc-b-z,get-b-z,desc-b-a,get-b-a,abrupt",
        );
    }
}

#[test]
fn computed_exact_attributes_keep_canonical_identity_and_lazy_catalog_bodies() {
    for directive in ["", "'use strict';\n"] {
        let source = format!(
            r#"{directive}
globalThis.loads = 0;
function require(value) {{ if (!value) throw 'embedded value invariant'; }}
var name = 'chosen';
var options = {{ with: {{ '\uE000': 'bmp', '\u{{10000}}': 'astral', flavour: 'blue' }} }};
Promise.all([import(name, options), import(name, options), import('self')]).then(function (values) {{
  require(values[0] === values[1] && values[0].value === 42 && loads === 1);
  require(values[0].url === 'scriptw://independent/dep.js');
  require(values[2].value === 7 && values[2].moduleThis === 'undefined');
  require(this === undefined || this === globalThis);
  return import(name, {{ with: {{ flavour: 'blue' }} }}).then(function () {{ throw 'dropped attrs matched'; }}, function (error) {{
    require(error instanceof TypeError && loads === 1); print('embedded-values:ok');
  }});
}});
262;
"#
        );
        assert_script(
            graph(
                EmbeddedModuleGoal::Script,
                &source,
                &[
                    (
                        "dep.js",
                        "globalThis.loads += 1; export const value = 42; export const url = import.meta.url;",
                        "scriptw://independent/dep.js",
                    ),
                    (
                        "red.js",
                        "throw 'unselected attribute body'; export const value = 13;",
                        "scriptw://red.js",
                    ),
                    (
                        "entry.js",
                        "export const value = 7; export const moduleThis = typeof this;",
                        "scriptw://module/entry.js",
                    ),
                    (
                        "unused.js",
                        "throw 'unreferenced body'; export {};",
                        "scriptw://unused.js",
                    ),
                    (
                        "unused-invalid.js",
                        "export const =",
                        "scriptw://unused-invalid.js",
                    ),
                ],
                vec![
                    edge(
                        "chosen",
                        &[
                            ("flavour", "blue"),
                            ("\u{10000}", "astral"),
                            ("\u{e000}", "bmp"),
                        ],
                        "dep.js",
                    ),
                    edge("chosen", &[("flavour", "red")], "red.js"),
                    edge("self", &[], "entry.js"),
                ],
            ),
            "embedded-values:ok",
        );
    }
}

#[test]
fn computed_rejections_and_source_defer_phases_keep_original_failure_cells() {
    for directive in ["", "'use strict';\n"] {
        let source = format!(
            r#"{directive}
globalThis.phaseLoads = 0;
function require(value) {{ if (!value) throw 'embedded phase invariant'; }}
var bad = 'bad'; var cold = 'cold'; var missing = 'absent';
var failures = 0;
Promise.all([
  import(bad).then(function () {{ throw 'malformed fulfilled'; }}, function (error) {{ require(error instanceof SyntaxError); failures++; }}),
  import.source(bad).then(function () {{ throw 'malformed source fulfilled'; }}, function (error) {{ require(error instanceof SyntaxError); failures++; }}),
  import.source(cold).then(function () {{ throw 'source fulfilled'; }}, function (error) {{ require(error instanceof SyntaxError); failures++; }}),
  import(missing).then(function () {{ throw 'missing fulfilled'; }}, function (error) {{ require(error instanceof TypeError); failures++; }})
]).then(function () {{
  require(failures === 4 && phaseLoads === 0);
  return import.defer(cold).then(function (namespace) {{
    require(phaseLoads === 0 && Object.keys(namespace).join() === 'value');
    require(namespace.value === 55 && phaseLoads === 1);
    return import(cold).then(function (eager) {{ require(eager.value === 55 && phaseLoads === 1); print('embedded-phases:ok'); }});
  }});
}});
262;
"#
        );
        assert_script(
            graph(
                EmbeddedModuleGoal::Script,
                &source,
                &[
                    ("bad.js", "export const =", "scriptw://bad.js"),
                    (
                        "cold.js",
                        "globalThis.phaseLoads += 1; export const value = 55;",
                        "scriptw://cold.js",
                    ),
                ],
                vec![edge("bad", &[], "bad.js"), edge("cold", &[], "cold.js")],
            ),
            "embedded-phases:ok",
        );
    }
}

#[test]
fn non_scalar_dynamic_strings_never_alias_declared_backslash_requests_or_attributes() {
    for directive in ["", "'use strict';\n"] {
        let source = format!(
            r#"{directive}
function require(value) {{ if (!value) throw 'embedded exact string invariant'; }}
var rejected = 0;
Promise.all([
  import('\uD800').then(function () {{ throw 'lone specifier matched'; }}, function (error) {{ require(error instanceof TypeError); rejected++; }}),
  import('attrs', {{ with: {{ '\uD800': 'yes' }} }}).then(function () {{ throw 'lone key matched'; }}, function (error) {{ require(error instanceof TypeError); rejected++; }}),
  import('value', {{ with: {{ key: '\uD800' }} }}).then(function () {{ throw 'lone value matched'; }}, function (error) {{ require(error instanceof TypeError); rejected++; }}),
  import('\\uD800'), import('attrs', {{ with: {{ '\\uD800': 'yes' }} }}),
  import('value', {{ with: {{ key: '\\uD800' }} }}),
  import('overwritten', {{ with: {{ key: '\uD800', key: 'yes' }} }})
]).then(function (values) {{
  require(rejected === 3 && values[3] === values[4] && values[4] === values[5] && values[6] === values[3]);
  require(values[3].value === 88); print('embedded-strings:ok');
}});
262;
"#
        );
        assert_script(
            graph(
                EmbeddedModuleGoal::Script,
                &source,
                &[("exact.js", "export const value = 88;", "scriptw://exact.js")],
                vec![
                    edge("\\uD800", &[], "exact.js"),
                    edge("attrs", &[("\\uD800", "yes")], "exact.js"),
                    edge("value", &[("key", "\\uD800")], "exact.js"),
                    edge("overwritten", &[("key", "yes")], "exact.js"),
                ],
            ),
            "embedded-strings:ok",
        );
    }
}

#[test]
fn module_entry_cycle_reuses_the_declared_entry_and_observes_independent_urls() {
    scriptw_engine::configure_compilation_jobs(1).expect("bounded compiler jobs");
    let source = "import { read } from 'dep'; export const value = 31; if (read() !== 31 || import.meta.url !== 'scriptw://observed/entry.js') throw 'entry cycle'; print('embedded-cycle:ok');";
    let graph = graph(
        EmbeddedModuleGoal::Module,
        source,
        &[(
            "dep.js",
            "import { value } from 'back'; export function read() { return value; }",
            "scriptw://different/dep.js",
        )],
        vec![
            EmbeddedModuleResolutionInput {
                referrer: EmbeddedModuleReferrer::Module("entry.js".into()),
                specifier: "dep".into(),
                attributes: vec![],
                target: "dep.js".into(),
            },
            EmbeddedModuleResolutionInput {
                referrer: EmbeddedModuleReferrer::Module("dep.js".into()),
                specifier: "back".into(),
                attributes: vec![],
                target: "entry.js".into(),
            },
        ],
    );
    let result = Engine::new(RealmBuilder::new().build())
        .observe_module(
            graph.entry().source(),
            CompileOptions {
                module_loading_policy: ModuleLoadingPolicy::Embedded(Arc::clone(&graph)),
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("canonical Module entry cycle executes");
    assert_eq!(result.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(
        result.completion,
        ObservedCompletion::Normal(ObservedJsValue::Undefined)
    );
    assert_eq!(
        result.output_events,
        vec![HostOutputEvent::PrintLine("embedded-cycle:ok".into())]
    );
}
