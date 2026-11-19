use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "scriptw-bytes-catalog-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn empty_catalog(directory: &Directory) -> Catalog {
    Catalog {
        root: directory.0.canonicalize().unwrap(),
        entry_path: directory.0.join("entry.js"),
        goal: EmbeddedModuleGoal::Script,
        texts: BTreeMap::new(),
        edges: BTreeMap::new(),
    }
}

#[test]
fn catalog_eligibility_preserves_javascript_trivia_and_escaped_requests() {
    let directory = Directory::new();
    let entry = directory.0.join("entry.js");
    fs::write(
        directory.0.join("target_FIXTURE.js"),
        "export const value = 42;",
    )
    .unwrap();
    let request =
        scriptw_engine::EmbeddedModuleRequest::try_new("./target_FIXTURE.js", vec![]).unwrap();
    for (source, realm) in [
        ("import/* comment */('./target_FIXTURE.js');", false),
        ("import\u{a0}('./target_FIXTURE.js');", false),
        (
            "realm.importValue /* comment */ ('./target_FIXTURE.js', 'value');",
            true,
        ),
        (
            "realm['importValue']('./target_FIXTURE.js', 'value');",
            true,
        ),
        (
            r"realm.\u0069mportValue('./target_FIXTURE.js', 'value');",
            true,
        ),
        ("eval('im' + 'port(\"./target_FIXTURE.js\")');", false),
    ] {
        fs::write(&entry, source).unwrap();
        let graph = declare(&entry, EmbeddedModuleGoal::Script, source, source)
            .unwrap()
            .unwrap()
            .graph;
        let referrer = if realm {
            EmbeddedModuleReferrer::Realm
        } else {
            EmbeddedModuleReferrer::Script(graph.entry().identity().into())
        };
        assert!(graph.resolve(&referrer, &request).is_some(), "{source}");
    }
    for source in [
        "// import('./target_FIXTURE.js')",
        "const text = 'importValue';",
        "// eval('im'+'port(...)')",
    ] {
        fs::write(&entry, source).unwrap();
        assert!(
            declare(&entry, EmbeddedModuleGoal::Script, source, source)
                .unwrap()
                .is_none(),
            "{source}"
        );
    }
}

#[test]
fn catalog_distinguishes_rejected_requests_from_failed_fixture_traversal() {
    let directory = Directory::new();
    let mut catalog = empty_catalog(&directory);
    assert!(catalog
        .resolve(&directory.0, "./missing.js", &[])
        .unwrap()
        .is_none());
    fs::write(directory.0.join("invalid_FIXTURE.js"), [255]).unwrap();
    assert!(catalog
        .resolve(&directory.0, "./invalid_FIXTURE.js", &[])
        .unwrap()
        .is_none());
    assert!(catalog
        .resolve(&directory.0, "./invalid_FIXTURE.js/child.js", &[])
        .unwrap()
        .is_none());
    assert!(catalog
        .resolve(&directory.0, "./invalid\0.js", &[])
        .unwrap()
        .is_none());
    let invalid_directory = directory.0.join("invalid_FIXTURE.js");
    let error = catalog
        .fixture_rows(&directory.0, &invalid_directory)
        .unwrap_err();
    assert!(error.contains("fixture"), "{error}");
    assert!(error.contains("invalid_FIXTURE.js"), "{error}");
}

#[cfg(unix)]
#[test]
fn catalog_reports_canonicalization_and_fixture_metadata_failures() {
    use std::os::unix::fs::symlink;
    let directory = Directory::new();
    let mut catalog = empty_catalog(&directory);
    symlink("loop_FIXTURE.js", directory.0.join("loop_FIXTURE.js")).unwrap();
    let error = catalog
        .resolve(&directory.0, "./loop_FIXTURE.js", &[])
        .unwrap_err();
    assert!(error.contains("cannot resolve module"), "{error}");
    assert!(error.contains("loop_FIXTURE.js"), "{error}");
    let error = catalog
        .fixture_rows(&directory.0, &directory.0)
        .unwrap_err();
    assert!(error.contains("cannot inspect fixture"), "{error}");
    assert!(error.contains("loop_FIXTURE.js"), "{error}");
}

#[test]
fn catalog_reads_binary_fixtures_and_retains_mixed_records_at_one_url() {
    let directory = Directory::new();
    let source = "import value from './value_FIXTURE.js'; import raw from './value_FIXTURE.js' with {type:'bytes'}; const path = './raw_FIXTURE.png'; import(path, {with:{type:'bytes'}});";
    let entry_path = directory.0.join("entry.js");
    fs::write(&entry_path, source).unwrap();
    fs::write(directory.0.join("value_FIXTURE.js"), b"export default 42;").unwrap();
    fs::write(directory.0.join("raw_FIXTURE.png"), [0, 255, 128, 10]).unwrap();
    fs::write(directory.0.join("empty_FIXTURE.txt"), []).unwrap();
    let graph = declare(&entry_path, EmbeddedModuleGoal::Module, source, source)
        .unwrap()
        .unwrap()
        .graph;
    let referrer = EmbeddedModuleReferrer::Module(graph.entry().identity().into());
    let request = |specifier: &str, bytes: bool| {
        scriptw_engine::EmbeddedModuleRequest::try_new(
            specifier,
            if bytes {
                vec![("type".into(), "bytes".into())]
            } else {
                Vec::new()
            },
        )
        .unwrap()
    };
    let text = graph
        .resolve(&referrer, &request("./value_FIXTURE.js", false))
        .unwrap();
    let raw = graph
        .resolve(&referrer, &request("./value_FIXTURE.js", true))
        .unwrap();
    assert_eq!(text.kind(), EmbeddedModuleKind::SourceText);
    assert_eq!(raw.kind(), EmbeddedModuleKind::Bytes);
    assert_eq!(text.identity(), raw.identity());
    assert_eq!(text.meta_url(), raw.meta_url());
    assert_eq!(raw.payload_bytes(), b"export default 42;");
    let image = graph
        .resolve(&referrer, &request("./raw_FIXTURE.png", true))
        .unwrap();
    assert_eq!(image.payload_bytes(), &[0, 255, 128, 10]);
    assert!(graph
        .resolve(&referrer, &request("./raw_FIXTURE.png", false))
        .is_none());
    assert_eq!(
        graph
            .resolve(&referrer, &request("./empty_FIXTURE.txt", true))
            .unwrap()
            .payload_bytes(),
        b"" as &[u8]
    );
}

#[test]
fn text_catalog_keeps_self_import_raw_and_admits_computed_json_text() {
    let directory = Directory::new();
    let source = "import self from './entry.js' with {type:'text'}; const path = './raw_FIXTURE.json'; import(path, {with:{type:'text'}});";
    let entry = directory.0.join("entry.js");
    fs::write(&entry, source).unwrap();
    let raw = "\u{feff}{not JSON}\0𝄞";
    fs::write(directory.0.join("raw_FIXTURE.json"), raw).unwrap();
    fs::write(directory.0.join("invalid_FIXTURE.txt"), [255]).unwrap();
    let graph = declare(&entry, EmbeddedModuleGoal::Module, source, source)
        .unwrap()
        .unwrap()
        .graph;
    let referrer = EmbeddedModuleReferrer::Module(graph.entry().identity().into());
    let text_request = |specifier: &str| {
        scriptw_engine::EmbeddedModuleRequest::try_new(
            specifier,
            vec![("type".into(), "text".into())],
        )
        .unwrap()
    };
    let own = graph
        .resolve(&referrer, &text_request("./entry.js"))
        .unwrap();
    assert_eq!(own.kind(), EmbeddedModuleKind::Text);
    assert_eq!(own.source_text(), Some(source));
    assert!(graph
        .module(graph.entry().identity(), EmbeddedModuleKind::SourceText)
        .is_some());
    let data = graph
        .resolve(&referrer, &text_request("./raw_FIXTURE.json"))
        .unwrap();
    assert_eq!(data.kind(), EmbeddedModuleKind::Text);
    assert_eq!(data.source_text(), Some(raw));
    assert!(graph
        .resolve(&referrer, &text_request("./invalid_FIXTURE.txt"))
        .is_none());
}

#[test]
fn literal_runtime_attributes_declare_only_that_locator_supported_record_variants() {
    let directory = Directory::new();
    let source =
        "var options = {with:{type:'json'}}; import('./plain.json', options); // import(ignored)";
    let entry = directory.0.join("entry.js");
    fs::write(&entry, source).unwrap();
    fs::write(directory.0.join("plain.json"), "262").unwrap();
    fs::write(directory.0.join("unrelated_FIXTURE.json"), "123").unwrap();
    for goal in [EmbeddedModuleGoal::Script, EmbeddedModuleGoal::Module] {
        let Some(catalog) = declare(&entry, goal, source, source).unwrap() else {
            assert_eq!(goal, EmbeddedModuleGoal::Script);
            continue; // No catalog leaves the host's RejectAll policy intact.
        };
        let graph = catalog.graph;
        let referrer = match goal {
            EmbeddedModuleGoal::Script => {
                EmbeddedModuleReferrer::Script(graph.entry().identity().into())
            }
            EmbeddedModuleGoal::Module => {
                EmbeddedModuleReferrer::Module(graph.entry().identity().into())
            }
        };
        for (name, kind) in [
            ("json", EmbeddedModuleKind::Json),
            ("bytes", EmbeddedModuleKind::Bytes),
            ("text", EmbeddedModuleKind::Text),
        ] {
            let request = scriptw_engine::EmbeddedModuleRequest::try_new(
                "./plain.json",
                vec![("type".into(), name.into())],
            )
            .unwrap();
            let target = graph.resolve(&referrer, &request).unwrap();
            assert_eq!(target.kind(), kind);
            assert_eq!(target.payload_bytes(), b"262");
        }
        assert_eq!(
            graph.dependency_modules().count(),
            3,
            "literal requests do not expand unrelated fixtures"
        );
        for attributes in [
            Vec::new(),
            vec![("type".into(), "css".into())],
            vec![
                ("type".into(), "json".into()),
                ("extra".into(), "no".into()),
            ],
        ] {
            let request =
                scriptw_engine::EmbeddedModuleRequest::try_new("./plain.json", attributes).unwrap();
            assert!(graph.resolve(&referrer, &request).is_none());
        }
    }
    let unsupported = "import('./plain.json', {with:{type:'css'}}); // import(ignored)";
    fs::write(&entry, unsupported).unwrap();
    let graph = declare(&entry, EmbeddedModuleGoal::Script, unsupported, unsupported)
        .unwrap()
        .unwrap()
        .graph;
    assert_eq!(
        graph.dependency_modules().count(),
        0,
        "known unsupported attributes do not become runtime or computed requests"
    );
    let computed = "const path = './plain.json'; import(path, {with:{type:'css'}});";
    fs::write(&entry, computed).unwrap();
    let graph = declare(&entry, EmbeddedModuleGoal::Script, computed, computed)
        .unwrap()
        .unwrap()
        .graph;
    assert_eq!(
        graph.dependency_modules().count(),
        0,
        "known computed attributes cannot select supported-kind fixture rows"
    );
}

#[test]
fn literal_runtime_attribute_catalog_executes_live_options_and_proxy_enumeration() {
    use scriptw_engine::{
        CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
        ModuleLoadingPolicy, ObservedCompletion, RealmBuilder, RunOptions,
    };
    const BODY: &str = r#"
function require(value, label) { if (!value) throw 'catalog attribute invariant: ' + label; }
var trace = [], symbol = Symbol('ignored');
var proxy = new Proxy({}, {
  ownKeys() { trace.push('keys'); return ['type', symbol, 'hidden']; },
  getOwnPropertyDescriptor(target, key) { trace.push('desc:' + key); return {configurable:true, enumerable:key === 'type'}; },
  get(target, key) { trace.push('get:' + key); if (key !== 'type') throw 'non-enumerable or Symbol Get'; return 'json'; }
});
var options = {get with() { trace.push('with'); return proxy; }};
var checks = [import('./2nd-param_FIXTURE.json', options).then(ns => require(ns.default === 262, 'Proxy JSON'))];
require(trace.join(',') === 'with,keys,desc:type,get:type,desc:hidden', 'live enumeration order');
var plain = {with:{type:'json'}};
checks.push(import('./2nd-param_FIXTURE.json', plain).then(ns => require(ns.default === 262, 'options identifier')));
var text = {with:{type:'text'}}, bytes = {with:{type:'bytes'}};
checks.push(import('./2nd-param_FIXTURE.json', text).then(ns => require(ns.default === '262', 'text kind')));
checks.push(import('./2nd-param_FIXTURE.json', bytes).then(ns => require(ns.default.length === 3 && ns.default[0] === 50 && ns.default[2] === 50, 'bytes kind')));
function rejectsType(promise) { return promise.then(() => { throw 'bad attributes fulfilled'; }, error => require(error instanceof TypeError, 'attribute rejection type')); }
var hidden = {with:new Proxy({}, {
  ownKeys() { return ['type']; },
  getOwnPropertyDescriptor() { return {configurable:true, enumerable:false}; },
  get() { throw 'non-enumerable value observed'; }
})};
checks.push(rejectsType(import('./2nd-param_FIXTURE.json', hidden)));
var extra = {with:{type:'json',extra:'no'}}, wrong = {with:{type:1}};
checks.push(rejectsType(import('./2nd-param_FIXTURE.json', extra)));
checks.push(rejectsType(import('./2nd-param_FIXTURE.json', wrong)));
var marker = {}, abrupt = {get with() { throw marker; }};
checks.push(import('./2nd-param_FIXTURE.json', abrupt).then(() => { throw 'abrupt options fulfilled'; }, error => require(error === marker, 'abrupt identity')));
Promise.all(checks).then(() => print('literal-runtime-attributes:ok'));
"#;
    let directory = Directory::new();
    let entry = directory.0.join("entry.js");
    fs::write(directory.0.join("2nd-param_FIXTURE.json"), "262").unwrap();
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{BODY}");
        fs::write(&entry, &source).unwrap();
        let graph = declare(&entry, EmbeddedModuleGoal::Script, &source, &source)
            .unwrap()
            .unwrap()
            .graph;
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
                CompileOptions {
                    module_loading_policy: ModuleLoadingPolicy::Embedded(graph),
                    host_surface_policy: HostSurfacePolicy::Test262,
                    ..CompileOptions::default()
                },
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(30_000),
                    ..RunOptions::default()
                },
            )
            .unwrap();
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observed.completion, ObservedCompletion::Normal(_)),
            "{:?}",
            observed.completion
        );
        assert_eq!(
            observed.output_events,
            vec![HostOutputEvent::PrintLine(
                "literal-runtime-attributes:ok".into()
            )]
        );
    }
}

#[test]
fn finite_direct_eval_imports_declare_only_actual_script_referrer_edges() {
    let directory = Directory::new();
    let source = r#"eval('eval("import(\\"./target_FIXTURE.js\\");")');"#;
    let entry = directory.0.join("entry.js");
    fs::write(&entry, source).unwrap();
    fs::write(directory.0.join("target_FIXTURE.js"), "export default 42;").unwrap();
    fs::write(
        directory.0.join("unrelated_FIXTURE.js"),
        "throw 'undeclared fixture';",
    )
    .unwrap();
    let graph = declare(&entry, EmbeddedModuleGoal::Script, source, source)
        .unwrap()
        .unwrap()
        .graph;
    let request =
        scriptw_engine::EmbeddedModuleRequest::try_new("./target_FIXTURE.js", Vec::new()).unwrap();
    let referrer = EmbeddedModuleReferrer::Script(graph.entry().identity().into());
    assert_eq!(
        graph.resolve(&referrer, &request).unwrap().source_text(),
        Some("export default 42;")
    );
    assert!(graph
        .resolve(&EmbeddedModuleReferrer::Realm, &request)
        .is_none());
    assert!(graph
        .resolve(&EmbeddedModuleReferrer::Unlocated, &request)
        .is_none());
    let unrelated =
        scriptw_engine::EmbeddedModuleRequest::try_new("./unrelated_FIXTURE.js", Vec::new())
            .unwrap();
    assert!(graph.resolve(&referrer, &unrelated).is_none());
}

#[test]
fn unproved_eval_requests_do_not_authorize_available_fixture_files() {
    let directory = Directory::new();
    let entry = directory.0.join("entry.js");
    fs::write(directory.0.join("target_FIXTURE.js"), "export default 42;").unwrap();
    for (goal, source) in [
        (EmbeddedModuleGoal::Script, r#"eval('import(specifier)');"#),
        (
            EmbeddedModuleGoal::Script,
            r#"(0, eval)('import("./target_FIXTURE.js")');"#,
        ),
        (
            EmbeddedModuleGoal::Module,
            r#"eval('import("./target_FIXTURE.js")');"#,
        ),
    ] {
        fs::write(&entry, source).unwrap();
        let Some(catalog) = declare(&entry, goal, source, source).unwrap() else {
            assert_eq!(goal, EmbeddedModuleGoal::Script);
            continue; // No catalog preserves the host's RejectAll policy.
        };
        let graph = catalog.graph;
        let request =
            scriptw_engine::EmbeddedModuleRequest::try_new("./target_FIXTURE.js", Vec::new())
                .unwrap();
        let referrer = match goal {
            EmbeddedModuleGoal::Script => {
                EmbeddedModuleReferrer::Script(graph.entry().identity().into())
            }
            EmbeddedModuleGoal::Module => {
                EmbeddedModuleReferrer::Module(graph.entry().identity().into())
            }
        };
        assert!(graph.resolve(&referrer, &request).is_none());
        assert!(graph
            .resolve(&EmbeddedModuleReferrer::Realm, &request)
            .is_none());
    }
}

#[test]
fn ordinary_script_import_discovery_skips_lowering_without_a_direct_eval_call() {
    for (label, text, retained_request) in [
        (
            "unicode-and-eval-text",
            r#"const frog = "🐸"; const text = "eval('import(\"./fake.js\")')"; import("./retained.js");"#,
            Some("./retained.js"),
        ),
        (
            "property-call",
            r#"object.eval('import("./target.js")');"#,
            None,
        ),
        (
            "computed-property-call",
            r#"object["eval"]('import("./target.js")');"#,
            None,
        ),
        (
            "indirect-call",
            r#"(0, eval)('import("./target.js")');"#,
            None,
        ),
        ("optional-call", r#"eval?.('import("./target.js")');"#, None),
        (
            "identifier-reference",
            r#"const alias = eval; alias('import("./target.js")');"#,
            None,
        ),
    ] {
        let scriptw_front::ParsedSource::Script(script) =
            scriptw_front::parse(text, scriptw_front::ParseOptions::script()).unwrap()
        else {
            panic!("not a Script: {label}");
        };
        let mut source = ModuleSourceIr::from_parsed_script(
            ModuleKey::from_host("entry"),
            String::new(),
            script,
        );
        include_prepared_script_imports_if_needed(&mut source, || {
            panic!("finite-source lowering reached an impossible direct eval: {label}")
        });
        let requests = source.module_loading_requests().unwrap();
        match retained_request {
            Some(specifier) => {
                assert_eq!(requests.len(), 1, "{label}");
                assert_eq!(requests[0].key(), &ModuleRequestKeyIr::plain(specifier));
            }
            None => assert!(requests.is_empty(), "{label}: {requests:?}"),
        }
    }
}

#[test]
fn prepared_script_discovery_covers_nested_escaped_and_parenthesized_direct_eval_calls() {
    for (label, text, literal_target) in [
        (
            "escaped-identifier",
            r#"\u0065val('import("./target.js")');"#,
            true,
        ),
        (
            "parenthesized-reference",
            r#"(((eval)))('import("./target.js")');"#,
            true,
        ),
        (
            "nested-functions",
            r#"function outer() { function inner() { eval('import("./target.js")'); } inner(); } outer();"#,
            true,
        ),
        (
            "object-method",
            r#"var object = { method() { eval('import("./target.js")'); } }; object.method();"#,
            true,
        ),
        (
            "class-method",
            r#"class C { method() { eval('import("./target.js")'); } } new C().method();"#,
            true,
        ),
        (
            "computed-import-remains-unproved",
            r#"eval('import(specifier)');"#,
            false,
        ),
        (
            "unknown-eval-source-remains-unproved",
            r#"eval(source);"#,
            false,
        ),
    ] {
        let scriptw_front::ParsedSource::Script(script) =
            scriptw_front::parse(text, scriptw_front::ParseOptions::script()).unwrap()
        else {
            panic!("not a Script: {label}");
        };
        let mut source = ModuleSourceIr::from_parsed_script(
            ModuleKey::from_host("entry"),
            String::new(),
            script.clone(),
        );
        let discovery_calls = std::cell::Cell::new(0);
        include_prepared_script_imports_if_needed(&mut source, || {
            discovery_calls.set(discovery_calls.get() + 1);
            let session = scriptw_ir::LoweringSession::default();
            session.lower_source(
                &scriptw_front::ParsedSource::Script(script),
                scriptw_ir::HostSurfacePolicy::Test262,
            );
            session.prepared_script_import_sites()
        });
        assert_eq!(discovery_calls.get(), 1, "missed source: {label}");
        let requests = source.module_loading_requests().unwrap();
        if literal_target {
            assert_eq!(requests.len(), 1, "{label}: {requests:?}");
            assert_eq!(requests[0].key(), &ModuleRequestKeyIr::plain("./target.js"));
        } else {
            assert!(requests.is_empty(), "unproved edge: {label}: {requests:?}");
        }
    }
}

#[test]
fn pinned_source_export_fixture_catalog_declares_one_shared_virtual_host_record() {
    const ENTRY: &str = include_str!(
        "../../../../test262/vendor/test262/test/language/module-code/ambiguous-export-bindings/namespace-unambiguous-if-import-source-and-export.js"
    );
    const ONE: &str = include_str!(
        "../../../../test262/vendor/test262/test/language/module-code/ambiguous-export-bindings/namespace-import-source-and-export-1_FIXTURE.js"
    );
    const TWO: &str = include_str!(
        "../../../../test262/vendor/test262/test/language/module-code/ambiguous-export-bindings/namespace-import-source-and-export-2_FIXTURE.js"
    );
    const STAR: &str = include_str!(
        "../../../../test262/vendor/test262/test/language/module-code/ambiguous-export-bindings/namespace-import-source-and-export-reexport_FIXTURE.js"
    );
    let directory = Directory::new();
    let entry = directory.0.join("entry.js");
    fs::write(&entry, ENTRY).unwrap();
    for (name, text) in [
        ("namespace-import-source-and-export-1_FIXTURE.js", ONE),
        ("namespace-import-source-and-export-2_FIXTURE.js", TWO),
        (
            "namespace-import-source-and-export-reexport_FIXTURE.js",
            STAR,
        ),
    ] {
        fs::write(directory.0.join(name), text).unwrap();
    }
    let graph = declare(&entry, EmbeddedModuleGoal::Module, ENTRY, ENTRY)
        .unwrap()
        .unwrap()
        .graph;
    let source_rows: Vec<_> = graph
        .resolutions()
        .iter()
        .filter(|row| row.request().specifier() == "<module source>")
        .collect();
    assert_eq!(source_rows.len(), 2);
    assert_eq!(source_rows[0].target(), source_rows[1].target());
    for row in source_rows {
        let target = graph.resolve(row.referrer(), row.request()).unwrap();
        assert_eq!(target.kind(), EmbeddedModuleKind::ModuleSource);
        assert_eq!(target.source_text(), None);
        assert!(
            matches!(target.contents(), EmbeddedModuleContents::ModuleSource { source_tag } if source_tag == "ModuleSource")
        );
    }
}

#[test]
fn virtual_source_host_contract_does_not_accept_other_names_or_data_attributes() {
    let directory = Directory::new();
    let source = "import source x from '<module source>'; import source y from '<module source>-other'; import z from '<module source>' with {type:'json'};";
    let entry = directory.0.join("entry.js");
    fs::write(&entry, source).unwrap();
    let graph = declare(&entry, EmbeddedModuleGoal::Module, source, source)
        .unwrap()
        .unwrap()
        .graph;
    assert_eq!(graph.resolutions().len(), 1);
    assert_eq!(
        graph.resolutions()[0].request().specifier(),
        "<module source>"
    );
    assert!(graph.resolutions()[0].request().attributes().is_empty());
}

#[test]
fn candidate_text_without_module_requests_needs_no_filesystem_identity() {
    let directory = Directory::new();
    let missing = directory.0.join("in-memory-script.js");
    for source in [
        "// import('./target.js')",
        "var value = 'eval';",
        r"var pattern = /\u0041/;",
        "eval('1 + 2')",
        "var holder = { invoke: eval }; holder.invoke('1 + 2');",
        "new ShadowRealm().evaluate('1 + 2');",
    ] {
        assert!(
            declare(&missing, EmbeddedModuleGoal::Script, source, source)
                .unwrap()
                .is_none(),
            "{source}"
        );
    }
    for (goal, source) in [
        (EmbeddedModuleGoal::Script, "import('./target.js')"),
        (
            EmbeddedModuleGoal::Script,
            "eval('im' + 'port(\"./target.js\")')",
        ),
        (EmbeddedModuleGoal::Module, "export const value = 1;"),
    ] {
        let error = match declare(&missing, goal, source, source) {
            Err(error) => error,
            Ok(_) => panic!("module-capable source lost its filesystem requirement: {source}"),
        };
        assert!(error.contains("cannot canonicalize"), "{error}");
    }
}
