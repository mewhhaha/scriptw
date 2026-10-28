use super::*;

/// An authority-bearing cache fixture, never an executable compiler result.
pub(super) fn portable_cache_fixture() -> Vec<u8> {
    let name = scriptw_aot_wasm::RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION.as_bytes();
    let payload_len = 1 + name.len() + 1;
    assert!(payload_len < 128);
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    module.extend_from_slice(&[0, payload_len as u8, name.len() as u8]);
    module.extend_from_slice(name);
    module.push(0); // Portable profile; no native compiler imports.
    wasmparser::Validator::new().validate_all(&module).unwrap();
    assert_eq!(
        scriptw_aot_wasm::RuntimeSourceCompilationProfile::from_module(&module).unwrap(),
        scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable
    );
    module
}

const TEST262_AGENT_PRELUDE: &str = r#"
var $262 = {
  agent: {
    start: function (source) { return __scriptwAgentStart(source); },
    broadcast: function (sab) { return __scriptwAgentBroadcast(sab); },
    receiveBroadcast: function (callback) {
      var message = __scriptwAgentReceiveBroadcast();
      return callback(message[0], message[1]);
    },
    report: function (value) { return __scriptwAgentReport(value); },
    getReport: function () { return __scriptwAgentGetReport(); },
    sleep: function (milliseconds) { return __scriptwAgentSleep(milliseconds); },
    monotonicNow: function () { return __scriptwAgentMonotonicNow(); },
    leaving: function () { return __scriptwAgentLeaving(); }
  }
};
"#;

fn test262_compile_options() -> CompileOptions {
    CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    }
}

#[test]
fn invalid_runtime_semantic_host_codes_remain_abi_errors() {
    for code in [i64::MIN, -1, 13, i64::MAX] {
        let error =
            wasm_reject_runtime_semantics(code).expect_err("an invalid host operation must fail");
        assert!(error.downcast_ref::<RuntimeSemanticRejection>().is_none());
        assert!(error.to_string().contains(&code.to_string()));
    }
}

#[test]
fn engine_error_wasmtime_capabilities_share_policy_authority() {
    let setup_error = EngineError::from_wasmtime_setup(WasmtimeEngineSetupError::new(
        WasmNativeCompilationMode::Fast,
        "test setup failure",
    ));
    assert_eq!(
        setup_error.wasm_gc_capability(),
        Some(PRODUCT_WASMTIME_POLICY.gc_capability())
    );
    assert_eq!(
        setup_error.wasm_weak_reachability_capability(),
        Some(PRODUCT_WASMTIME_POLICY.weak_reachability_capability())
    );

    let ordinary_error = EngineError::new("ordinary failure");
    assert_eq!(ordinary_error.wasm_gc_capability(), None);
    assert_eq!(ordinary_error.wasm_weak_reachability_capability(), None);
}

#[test]
fn product_wasmtime_profiles_require_copying_weak_capability() {
    for engine in [
        shared_wasm_engine().expect("fast required runtime should initialize"),
        shared_size_optimized_wasm_engine()
            .expect("size-optimized required runtime should initialize"),
    ] {
        assert_eq!(engine.get_max_wasm_stack(), WASM_MAX_STACK_SIZE);
        assert_eq!(engine.get_async_stack_size(), ENGINE_WORKER_STACK_SIZE);
        assert!(engine.get_max_wasm_stack() < engine.get_async_stack_size());
        assert_eq!(engine.get_collector(), Some(wasmtime::Collector::Copying));
        assert!(engine.get_wasm_features().threads());
        assert!(engine.get_shared_memory());
        let memory = WasmtimeSharedMemory::new(&engine, wasmtime::MemoryType::shared(1, 1))
            .expect("every product profile must allocate the required shared memory");
        assert_eq!(memory.size(), 1);
        PRODUCT_WASMTIME_POLICY
            .verify_engine(&engine)
            .expect("every product profile must preserve required runtime capabilities");
    }
    assert_eq!(
        PRODUCT_WASMTIME_POLICY.gc_capability(),
        WasmGcCapability::CopyingWithCycleCollection
    );
    assert_eq!(
        PRODUCT_WASMTIME_POLICY.weak_reachability_capability(),
        WasmWeakReachabilityCapability::CopyingCollector
    );
}

#[test]
fn required_runtime_rejects_disabled_gc_support() {
    let mut config = WasmtimeConfig::new();
    PRODUCT_WASMTIME_POLICY.configure(&mut config);
    config.gc_support(false);
    let engine = WasmtimeEngine::new(&config)
        .expect("upstream permits a configuration with its GC runtime disabled");
    assert!(!engine.get_wasm_features().gc_types());
    let failure = PRODUCT_WASMTIME_POLICY
        .verify_engine(&engine)
        .expect_err("Scriptw must reject the instantiated engine without its required GC runtime");
    assert_eq!(failure, "required Wasm GC runtime support is unavailable");
    let error = EngineError::from_wasmtime_setup(WasmtimeEngineSetupError::new(
        WasmNativeCompilationMode::Fast,
        failure,
    ));
    assert_eq!(
        error.wasm_gc_capability(),
        Some(WasmGcCapability::CopyingWithCycleCollection)
    );
    assert_eq!(
        error.wasm_weak_reachability_capability(),
        Some(WasmWeakReachabilityCapability::CopyingCollector)
    );
}

#[test]
fn required_runtime_refuses_implicit_collector_selection() {
    let mut config = WasmtimeConfig::new();
    PRODUCT_WASMTIME_POLICY.configure(&mut config);
    config.collector(wasmtime::Collector::Auto);
    let engine = WasmtimeEngine::new(&config)
        .expect("compiled copying support should permit an automatic upstream configuration");
    assert_eq!(
        PRODUCT_WASMTIME_POLICY.verify_engine(&engine),
        Err("required copying collector was not explicitly selected")
    );
}

#[test]
fn embedded_authority_reaches_prepared_units_and_unlocated_agent_options() {
    let source = "var key = 'entryOnly'; import(key);";
    let graph = EmbeddedModuleGraph::try_new(
        EmbeddedModuleEntryInput {
            goal: EmbeddedModuleGoal::Script,
            identity: "entry.js".into(),
            source: source.into(),
            meta_url: "scriptw://entry.js".into(),
        },
        vec![EmbeddedModuleSourceInput {
            identity: "dep.js".into(),
            source: "export const value = 5;".into(),
            meta_url: "scriptw://dep.js".into(),
        }],
        vec![
            EmbeddedModuleResolutionInput {
                referrer: EmbeddedModuleReferrer::Script("entry.js".into()),
                specifier: "entryOnly".into(),
                attributes: vec![],
                target: "dep.js".into(),
            },
            EmbeddedModuleResolutionInput {
                referrer: EmbeddedModuleReferrer::Unlocated,
                specifier: "agentOnly".into(),
                attributes: vec![],
                target: "dep.js".into(),
            },
        ],
    )
    .expect("finite exact graph");
    let options = CompileOptions {
        module_loading_policy: ModuleLoadingPolicy::Embedded(Arc::clone(&graph)),
        ..CompileOptions::default()
    };
    let engine = Engine::new(RealmBuilder::new().build());
    let prepared = engine
        .prepare_compilation(source, ParseGoal::Script, &options)
        .unwrap();
    let PreparedModuleCompilation::Embedded {
        owner: prepared_graph,
        ..
    } = &prepared.modules
    else {
        panic!("prepared authority lost");
    };
    assert!(Arc::ptr_eq(&graph, prepared_graph));
    assert_eq!(
        prepared.source.source().filename.as_deref(),
        Some("entry.js")
    );
    assert_eq!(
        prepared.modules.graph().unwrap().resolutions[0]
            .1
            .specifier(),
        "entryOnly"
    );
    let unit = engine.compile_prepared_on_current_thread(prepared).unwrap();
    let ModuleLoadingPolicy::Embedded(unit_graph) = &unit.module_loading_policy else {
        panic!("unit authority lost");
    };
    assert!(Arc::ptr_eq(&graph, unit_graph));

    let worker_options = WasmAgentCompilePolicy::from_root(&options).worker_options();
    let ModuleLoadingPolicy::Embedded(worker_graph) = &worker_options.module_loading_policy else {
        panic!("worker authority lost");
    };
    assert!(Arc::ptr_eq(&graph, worker_graph));
    let worker_source = "var key = 'agentOnly'; import(key);";
    assert!(engine
        .prepare_compilation(worker_source, ParseGoal::Script, &worker_options)
        .is_err());
    let worker = engine
        .prepare_compilation_for_role(
            worker_source,
            ParseGoal::Script,
            &worker_options,
            module_loader::EmbeddedSourceRole::UnlocatedScript,
        )
        .unwrap();
    assert_eq!(worker.source.source().filename, None);
    let worker_sources = worker.modules.graph().unwrap();
    assert_eq!(worker_sources.resolutions.len(), 1);
    assert_eq!(worker_sources.resolutions[0].1.specifier(), "agentOnly");
    let worker_key = program_cache_key_for_role(
        worker_source,
        ParseGoal::Script,
        &worker_options,
        Some(worker_sources),
        module_loader::EmbeddedSourceRole::UnlocatedScript,
    );
    assert_ne!(
        worker_key,
        program_cache_key(
            worker_source,
            ParseGoal::Script,
            &worker_options,
            Some(worker_sources)
        )
    );
    let worker_unit = engine.compile_prepared_on_current_thread(worker).unwrap();
    let ModuleLoadingPolicy::Embedded(worker_unit_graph) = worker_unit.module_loading_policy else {
        panic!("worker unit authority lost");
    };
    assert!(Arc::ptr_eq(&graph, &worker_unit_graph));
}

#[test]
fn embedded_artifact_key_consumes_unused_source_and_url_authority() {
    let graph = |source: &str, url: &str| {
        EmbeddedModuleGraph::try_new(
            EmbeddedModuleEntryInput {
                goal: EmbeddedModuleGoal::Script,
                identity: "entry.js".into(),
                source: "262;".into(),
                meta_url: "scriptw://entry.js".into(),
            },
            vec![EmbeddedModuleSourceInput {
                identity: "unused.js".into(),
                source: source.into(),
                meta_url: url.into(),
            }],
            vec![],
        )
        .unwrap()
    };
    let key = |graph| {
        program_wasm_cache_key(
            "262;",
            ParseGoal::Script,
            &CompileOptions {
                module_loading_policy: ModuleLoadingPolicy::Embedded(graph),
                ..CompileOptions::default()
            },
        )
    };
    assert_ne!(
        key(graph("export {};", "scriptw://unused.js")),
        key(graph("export const value = 1;", "scriptw://unused.js"))
    );
    assert_ne!(
        key(graph("export {};", "scriptw://unused.js")),
        key(graph("export {};", "scriptw://other/unused.js"))
    );
}

fn push_wasm_u32(mut value: u32, bytes: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn push_wasm_name(name: &str, bytes: &mut Vec<u8>) {
    push_wasm_u32(name.len() as u32, bytes);
    bytes.extend_from_slice(name.as_bytes());
}

fn push_wasm_section(id: u8, payload: &[u8], bytes: &mut Vec<u8>) {
    bytes.push(id);
    push_wasm_u32(payload.len() as u32, bytes);
    bytes.extend_from_slice(payload);
}

fn inert_wasm_with_intl_identity(imports_intl_host: bool, identity_sections: &[&[u8]]) -> Vec<u8> {
    let mut bytes = b"\0asm\x01\0\0\0".to_vec();
    if imports_intl_host {
        let mut imports = Vec::new();
        push_wasm_u32(1, &mut imports);
        push_wasm_name(WASM_HOST_IMPORT_NAMESPACE, &mut imports);
        push_wasm_name(WASM_HOST_IMPORT_INTL_CALL, &mut imports);
        imports.push(0); // function import
        push_wasm_u32(0, &mut imports); // type index; parsing does not validate it
        push_wasm_section(2, &imports, &mut bytes);
    }
    for identity in identity_sections {
        let mut custom = Vec::new();
        push_wasm_name(INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION, &mut custom);
        custom.extend_from_slice(identity);
        push_wasm_section(0, &custom, &mut bytes);
    }
    bytes
}

// Agent.start compiles and readies a cold worker inside the root execution
// deadline. Match the existing worker-policy and structured-root fixtures.
const SHARED_AGENT_STARTUP_EXECUTION_TIMEOUT_MS: u64 = 120_000;

#[test]
fn wasm_agent_worker_loading_obeys_the_root_policy() {
    // The root execution deadline includes cold worker Wasmtime compilation
    // and readiness; match the existing bounded worker fixture budget.
    const WORKER_POLICY_TIMEOUT_MS: u64 = 120_000;

    let directory = std::env::current_dir()
        .expect("workspace directory should exist")
        .join("target")
        .join(format!(
            "scriptw-wasm-agent-module-policy-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
    let ambient_path = directory.join("ambient.mjs");
    let entry_path = directory.join("entry.js");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("ambient witness directory should exist");
    std::fs::write(&ambient_path, "export const value = 1;\n")
        .expect("ambient witness module should exist");

    let specifier = ambient_path.to_string_lossy();
    let worker_source = format!(
        "import({specifier:?}).then(function () {{ $262.agent.report(\"ambient-loaded\"); $262.agent.leaving(); }}, function () {{ $262.agent.report(\"module-rejected\"); $262.agent.leaving(); }});"
    );
    let source = format!(
        r#"{TEST262_AGENT_PRELUDE}
$262.agent.start({worker_source:?});
var report = null;
for (var attempt = 0; attempt < 10000 && report === null; attempt++) {{
  $262.agent.sleep(1);
  report = $262.agent.getReport();
}}
report;
"#
    );
    let options = |module_loading_policy| CompileOptions {
        filename: Some(entry_path.to_string_lossy().into_owned()),
        module_loading_policy,
        ..test262_compile_options()
    };

    let filesystem = Engine::new(RealmBuilder::new().build())
        .run_wasm_aot_script_with_agents(
            &source,
            options(ModuleLoadingPolicy::Filesystem),
            Some(WORKER_POLICY_TIMEOUT_MS),
            true,
            TEST262_AGENT_PRELUDE.to_string(),
        )
        .expect("the filesystem control should load the on-disk worker dependency");
    assert!(
        filesystem.note.contains("ambient-loaded"),
        "{}",
        filesystem.note
    );

    let reject_all = Engine::new(RealmBuilder::new().build())
        .run_wasm_aot_script_with_agents(
            &source,
            options(ModuleLoadingPolicy::RejectAll),
            Some(WORKER_POLICY_TIMEOUT_MS),
            true,
            TEST262_AGENT_PRELUDE.to_string(),
        )
        .expect("a reject-all root should settle the worker import through rejection");
    assert!(
        reject_all.note.contains("module-rejected"),
        "{}",
        reject_all.note
    );

    std::fs::remove_dir_all(&directory).expect("ambient witness directory should be removed");
}

fn push_unsigned_leb128(output: &mut Vec<u8>, mut value: usize) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            return;
        }
    }
}

fn append_wasm_section(module: &mut Vec<u8>, section_id: u8, payload: &[u8]) {
    module.push(section_id);
    push_unsigned_leb128(module, payload.len());
    module.extend_from_slice(payload);
}

fn wasm_module_with_code_body_size(code_body_size: usize) -> Vec<u8> {
    assert!(code_body_size >= 2);
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    append_wasm_section(&mut module, 1, &[1, 0x60, 0, 0]);
    append_wasm_section(&mut module, 3, &[1, 0]);

    let mut code = Vec::with_capacity(code_body_size + 8);
    code.push(1);
    push_unsigned_leb128(&mut code, code_body_size);
    code.push(0);
    code.extend(std::iter::repeat_n(0x01, code_body_size - 2));
    code.push(0x0b);
    append_wasm_section(&mut module, 10, &code);
    module
}

#[test]
fn oversized_wasm_code_bodies_select_size_optimized_native_compilation() {
    let below_threshold =
        wasm_module_with_code_body_size(SIZE_OPTIMIZED_WASM_MIN_CODE_BODY_BYTES - 1);
    assert_eq!(
        plan_wasm_native_compilation(&below_threshold),
        WasmNativeCompilationPlan {
            mode: WasmNativeCompilationMode::Fast,
            largest_code_body_bytes: Some(SIZE_OPTIMIZED_WASM_MIN_CODE_BODY_BYTES - 1),
        }
    );

    let at_threshold = wasm_module_with_code_body_size(SIZE_OPTIMIZED_WASM_MIN_CODE_BODY_BYTES);
    assert_eq!(
        plan_wasm_native_compilation(&at_threshold),
        WasmNativeCompilationPlan {
            mode: WasmNativeCompilationMode::SizeOptimized,
            largest_code_body_bytes: Some(SIZE_OPTIMIZED_WASM_MIN_CODE_BODY_BYTES),
        }
    );
}

#[test]
fn malformed_wasm_remains_on_the_authoritative_validation_path() {
    let mut malformed = wasm_module_with_code_body_size(2);
    malformed.pop();

    assert_eq!(
        plan_wasm_native_compilation(&malformed),
        WasmNativeCompilationPlan {
            mode: WasmNativeCompilationMode::Fast,
            largest_code_body_bytes: None,
        }
    );
    let engine = shared_wasm_engine().expect("fast Wasmtime engine should initialize");
    assert!(
        WasmtimeModule::new(&engine, &malformed).is_err(),
        "malformed Wasm must still be rejected by Wasmtime"
    );
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
fn process_virtual_memory_bytes() -> u64 {
    let status =
        std::fs::read_to_string("/proc/self/status").expect("Linux process status should exist");
    let vm_size = status
        .lines()
        .find_map(|line| line.strip_prefix("VmSize:"))
        .expect("Linux process status should report VmSize");
    let mut fields = vm_size.split_ascii_whitespace();
    let kibibytes = fields
        .next()
        .expect("VmSize should contain a number")
        .parse::<u64>()
        .expect("VmSize should be numeric");
    assert_eq!(
        fields.next(),
        Some("kB"),
        "VmSize should be reported in kibibytes: {vm_size}"
    );
    kibibytes
        .checked_mul(1024)
        .expect("VmSize should fit in bytes")
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
#[test]
fn three_live_wasm_stores_reserve_less_than_four_gibibytes() {
    const EXPORTED_MEMORY_WASM: &[u8] = &[
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x05, 0x03, 0x01, 0x00, 0x01, 0x07, 0x0a,
        0x01, 0x06, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x02, 0x00,
    ];
    const MAXIMUM_ADDITIONAL_VIRTUAL_MEMORY_BYTES: u64 = 4 * 1024 * 1024 * 1024;

    let engines = [
        (
            "fast",
            shared_wasm_engine().expect("fast Wasmtime engine should initialize"),
        ),
        (
            "size-optimized",
            shared_size_optimized_wasm_engine()
                .expect("size-optimized Wasmtime engine should initialize"),
        ),
    ];
    for (engine_name, engine) in engines {
        let module = WasmtimeModule::new(&engine, EXPORTED_MEMORY_WASM)
            .expect("ordinary memory probe should compile");
        let virtual_memory_before = process_virtual_memory_bytes();
        let mut live_instances = Vec::with_capacity(3);
        for store_index in 0..3 {
            let mut store = WasmtimeStore::new(&engine, ());
            let instance =
                wasmtime::Instance::new(&mut store, &module, &[]).unwrap_or_else(|error| {
                    panic!("{engine_name} Wasmtime store {store_index} should instantiate: {error}")
                });
            live_instances.push((store, instance));
        }
        std::hint::black_box(&live_instances);
        let additional_virtual_memory =
            process_virtual_memory_bytes().saturating_sub(virtual_memory_before);

        assert!(
            additional_virtual_memory < MAXIMUM_ADDITIONAL_VIRTUAL_MEMORY_BYTES,
            "{engine_name} Wasmtime engine reserved {additional_virtual_memory} bytes for \
                 three live ordinary memories; expected less than \
                 {MAXIMUM_ADDITIONAL_VIRTUAL_MEMORY_BYTES}"
        );
    }
}

#[test]
fn shared_memory_reaches_but_cannot_exceed_the_one_gibibyte_cap() {
    const EXPORTED_SHARED_MEMORY_WASM: &[u8] = &[
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x05, 0x06, 0x01, 0x03, 0x01, 0x80, 0x80,
        0x04, 0x07, 0x0a, 0x01, 0x06, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x02, 0x00,
    ];
    const WASM_PAGE_BYTES: usize = 64 * 1024;
    const ONE_GIBIBYTE_PAGES: u64 = 16_384;

    assert_eq!(
        WASM_STORE_MEMORY_CAP_BYTES / WASM_PAGE_BYTES,
        ONE_GIBIBYTE_PAGES as usize,
        "the Wasm store memory cap must remain one GiB"
    );
    let engines = [
        (
            "fast",
            shared_wasm_engine().expect("fast Wasmtime engine should initialize"),
        ),
        (
            "size-optimized",
            shared_size_optimized_wasm_engine()
                .expect("size-optimized Wasmtime engine should initialize"),
        ),
    ];
    for (engine_name, engine) in engines {
        let module = WasmtimeModule::new(&engine, EXPORTED_SHARED_MEMORY_WASM)
            .expect("shared memory probe should compile");
        let limits = WasmtimeStoreLimitsBuilder::new()
            .memory_size(WASM_STORE_MEMORY_CAP_BYTES)
            .build();
        let mut store = WasmtimeStore::new(&engine, limits);
        store.limiter(|limits| limits);
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap_or_else(|error| {
            panic!("{engine_name} shared memory should instantiate: {error}")
        });
        let memory = instance
            .get_shared_memory(&mut store, "memory")
            .expect("shared memory probe should export memory");

        assert_eq!(memory.size(), 1);
        assert_eq!(
            memory.grow(ONE_GIBIBYTE_PAGES - 1).unwrap_or_else(|error| {
                panic!("{engine_name} shared memory should grow to one GiB: {error}")
            }),
            1
        );
        assert_eq!(memory.size(), ONE_GIBIBYTE_PAGES);
        assert!(
            memory.grow(1).is_err(),
            "{engine_name} shared memory must not grow beyond one GiB"
        );
    }
}

fn assert_epoch_ticker_advances(engine: &WasmtimeEngine) {
    const EXPORTED_NOOP_WASM: &[u8] = &[
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x03,
        0x02, 0x01, 0x00, 0x07, 0x08, 0x01, 0x04, 0x74, 0x69, 0x63, 0x6b, 0x00, 0x00, 0x0a, 0x04,
        0x01, 0x02, 0x00, 0x0b,
    ];

    ensure_wasm_epoch_ticker(engine);
    let module =
        WasmtimeModule::new(engine, EXPORTED_NOOP_WASM).expect("epoch probe module should compile");
    let mut store = WasmtimeStore::new(engine, ());
    let instance = wasmtime::Instance::new(&mut store, &module, &[])
        .expect("epoch probe module should instantiate");
    let tick = instance
        .get_typed_func::<(), ()>(&mut store, "tick")
        .expect("epoch probe function should exist");
    store.set_epoch_deadline(1);

    let timeout = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match tick.call(&mut store, ()) {
            Err(error) => {
                assert!(
                    is_wasm_epoch_interrupt(&error),
                    "epoch probe should stop with an interrupt trap: {error}"
                );
                return;
            }
            Ok(()) if std::time::Instant::now() < timeout => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Ok(()) => panic!("registered Wasmtime engine did not receive an epoch tick"),
        }
    }
}

#[test]
fn epoch_ticker_advances_both_native_compilation_engines() {
    let fast = shared_wasm_engine().expect("fast Wasmtime engine should initialize");
    assert_epoch_ticker_advances(&fast);

    let size_optimized = shared_size_optimized_wasm_engine()
        .expect("size-optimized Wasmtime engine should initialize");
    assert_epoch_ticker_advances(&size_optimized);
}

use std::sync::{atomic::AtomicU64, Arc, Mutex};
use wasmparser::{Imports, Parser, Payload};

#[derive(Debug)]
struct CapturingHostHooks {
    lines: Arc<Mutex<Vec<String>>>,
}

impl HostHooks for CapturingHostHooks {
    fn print_line(&self, text: &str) {
        self.lines
            .lock()
            .expect("capture mutex poisoned")
            .push(text.to_string());
    }
}

#[derive(Debug)]
struct DeterministicHostClock {
    wall_clock: UtcEpochMilliseconds,
    next_monotonic_nanoseconds: AtomicU64,
    monotonic_step_nanoseconds: u64,
}

impl HostClock for DeterministicHostClock {
    fn utc_epoch_milliseconds(&self) -> UtcEpochMilliseconds {
        self.wall_clock
    }

    fn monotonic_instant(&self) -> MonotonicClockInstant {
        MonotonicClockInstant::new(self.next_monotonic_nanoseconds.fetch_add(
            self.monotonic_step_nanoseconds,
            std::sync::atomic::Ordering::Relaxed,
        ))
    }
}

#[derive(Debug)]
struct DeterministicHostRandom {
    next_word: AtomicU64,
    step: u64,
}

impl HostRandom for DeterministicHostRandom {
    fn random_unit_interval(&self) -> Result<RandomUnitInterval, HostRandomError> {
        Ok(RandomUnitInterval::from_entropy_word(
            self.next_word
                .fetch_add(self.step, std::sync::atomic::Ordering::Relaxed),
        ))
    }
}

/// Engine-test harness authority. This module contains the historical
/// cross-realm/detach/agent fixture corpus; keeping its opt-in here makes
/// each invocation explicit without weakening `CompileOptions::default`.
/// Product-default behavior is covered with a raw `Engine` below.
struct TestEngine(Engine);

impl std::ops::Deref for TestEngine {
    type Target = Engine;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TestEngine {
    fn authorize_test262(mut options: CompileOptions) -> CompileOptions {
        options.host_surface_policy = HostSurfacePolicy::Test262;
        options
    }

    fn compile_script(
        &self,
        source: &str,
        options: CompileOptions,
    ) -> Result<CompilationUnit, EngineError> {
        self.0
            .compile_script(source, Self::authorize_test262(options))
    }

    fn compile_module(
        &self,
        source: &str,
        options: CompileOptions,
    ) -> Result<CompilationUnit, EngineError> {
        self.0
            .compile_module(source, Self::authorize_test262(options))
    }

    fn run_script(
        &self,
        source: &str,
        options: CompileOptions,
        run: RunOptions,
    ) -> Result<RunOutcome, EngineError> {
        self.0
            .run_script(source, Self::authorize_test262(options), run)
    }

    fn run_wasm_aot_script_on_current_thread(
        &self,
        source: &str,
        options: CompileOptions,
        timeout_ms: Option<u64>,
        can_block: bool,
    ) -> Result<RunOutcome, EngineError> {
        self.0.run_wasm_aot_script_on_current_thread(
            source,
            Self::authorize_test262(options),
            timeout_ms,
            can_block,
        )
    }

    fn run_wasm_aot_script_with_agents(
        &self,
        source: &str,
        options: CompileOptions,
        timeout_ms: Option<u64>,
        can_block: bool,
        agent_prelude: String,
    ) -> Result<RunOutcome, EngineError> {
        self.0.run_wasm_aot_script_with_agents(
            source,
            Self::authorize_test262(options),
            timeout_ms,
            can_block,
            agent_prelude,
        )
    }
}

fn engine() -> TestEngine {
    TestEngine(Engine::new(RealmBuilder::new().build()))
}

#[derive(Debug)]
struct NamedHostHooks;

impl HostHooks for NamedHostHooks {
    fn shell_name(&self) -> &'static str {
        "named-engine-shell"
    }
}

#[test]
fn engine_projects_the_realm_host_hook_shell_name() {
    let engine = Engine::new(
        RealmBuilder::new()
            .with_host_hooks(Box::new(NamedHostHooks))
            .build(),
    );

    assert_eq!(engine.shell_name(), "named-engine-shell");
}

fn engine_with_captured_prints(lines: Arc<Mutex<Vec<String>>>) -> TestEngine {
    TestEngine(Engine::new(
        RealmBuilder::new()
            .with_host_hooks(Box::new(CapturingHostHooks { lines }))
            .build(),
    ))
}

#[test]
fn wasm_memory_spans_bound_transient_print_bytes_before_allocation() {
    assert!(wasm_memory_span(8, usize::MAX, 32, "wasm print input").is_err());
    assert!(wasm_memory_span(31, 2, 32, "wasm print input").is_err());
    assert_eq!(
        wasm_memory_span(8, 24, 32, "wasm print input").unwrap(),
        8..32
    );
}

/// Recomputes the two rankings independently of
/// `describe_largest_wasm_code_body` and asserts the reported indices and
/// figures are the ones the module actually has.
///
/// Asserting only that the text *contains* `wasm[0]::function[` would pass
/// for any index the walk happened to produce, including one shifted by the
/// imported-function count — and a shifted index points at the wrong
/// function, which is the whole failure mode this attribution exists to
/// avoid.
#[test]
fn largest_wasm_code_body_is_attributed_by_index() {
    let engine = engine();
    let unit = engine
        .compile_script("var x = 1 + 1;", CompileOptions::default())
        .expect("script compile should succeed");
    let artifact = engine.emit_wasm(&unit).expect("wasm emit should succeed");

    let mut imported_function_count = 0u32;
    let mut bodies: Vec<WasmCodeBodyFacts> = Vec::new();
    for payload in WasmParser::new(0).parse_all(&artifact.bytes) {
        match payload.expect("emitted module should parse") {
            WasmPayload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.expect("import should parse");
                    if matches!(
                        import.ty,
                        wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                    ) {
                        imported_function_count += 1;
                    }
                }
            }
            WasmPayload::CodeSectionEntry(body) => {
                bodies.push(WasmCodeBodyFacts {
                    function_index: imported_function_count
                        + u32::try_from(bodies.len()).expect("code entry count fits u32"),
                    body_bytes: body.range().len(),
                    declared_locals: wasm_body_declared_locals(&body),
                });
            }
            _ => {}
        }
    }
    assert!(
        imported_function_count > 0,
        "module should import functions"
    );
    let largest = bodies
        .iter()
        .copied()
        .max_by_key(|facts| facts.body_bytes)
        .expect("an emitted module always has a code section");
    let most_locals = bodies
        .iter()
        .copied()
        .max_by_key(|facts| facts.declared_locals.unwrap_or(0))
        .expect("an emitted module always has a code section");
    assert!(
        most_locals.declared_locals.is_some_and(|locals| locals > 0),
        "the backend declares locals; a `?` here means the decode broke"
    );

    let description = describe_largest_wasm_code_body(&artifact.bytes)
        .expect("an emitted module always has a code section");
    assert_eq!(
        description,
        format!("largest emitted function: {largest}; most locals: {most_locals}"),
    );
}

#[test]
fn wasm_number_pow_matches_ecmascript_special_values() {
    assert_eq!(wasm_number_pow(f64::NAN, 0.0), 1.0);
    assert!(wasm_number_pow(1.0, f64::INFINITY).is_nan());
    assert!(wasm_number_pow(-1.0, f64::NEG_INFINITY).is_nan());
    assert_eq!(wasm_number_pow(f64::NEG_INFINITY, 3.0), f64::NEG_INFINITY);
    assert_eq!(
        wasm_number_pow(f64::NEG_INFINITY, -3.0).to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(wasm_number_pow(-0.0, -3.0), f64::NEG_INFINITY);
    assert_eq!(wasm_number_pow(-0.0, 3.0).to_bits(), (-0.0f64).to_bits());
    assert!(wasm_number_pow(-4.0, 0.5).is_nan());
}

#[test]
fn wasm_math_transcendentals_match_ecmascript_special_values() {
    // NaN propagates through every unary transcendental.
    for value in [
        wasm_math_acos(f64::NAN),
        wasm_math_acosh(f64::NAN),
        wasm_math_asin(f64::NAN),
        wasm_math_asinh(f64::NAN),
        wasm_math_atan(f64::NAN),
        wasm_math_atanh(f64::NAN),
        wasm_math_cbrt(f64::NAN),
        wasm_math_cos(f64::NAN),
        wasm_math_cosh(f64::NAN),
        wasm_math_exp(f64::NAN),
        wasm_math_expm1(f64::NAN),
        wasm_math_log(f64::NAN),
        wasm_math_log10(f64::NAN),
        wasm_math_log1p(f64::NAN),
        wasm_math_log2(f64::NAN),
        wasm_math_sin(f64::NAN),
        wasm_math_sinh(f64::NAN),
        wasm_math_tan(f64::NAN),
        wasm_math_tanh(f64::NAN),
    ] {
        assert!(value.is_nan());
    }
    assert!(wasm_math_atan2(f64::NAN, 1.0).is_nan());
    assert!(wasm_math_atan2(1.0, f64::NAN).is_nan());

    // Signed-zero rows are bit-exact: the sign is observable via
    // `Object.is` / division, so `assert_eq!` alone would miss it.
    for pair in [
        (wasm_math_asin(0.0), 0.0),
        (wasm_math_asin(-0.0), -0.0),
        (wasm_math_asinh(0.0), 0.0),
        (wasm_math_asinh(-0.0), -0.0),
        (wasm_math_atan(0.0), 0.0),
        (wasm_math_atan(-0.0), -0.0),
        (wasm_math_atanh(0.0), 0.0),
        (wasm_math_atanh(-0.0), -0.0),
        (wasm_math_cbrt(0.0), 0.0),
        (wasm_math_cbrt(-0.0), -0.0),
        (wasm_math_expm1(0.0), 0.0),
        (wasm_math_expm1(-0.0), -0.0),
        (wasm_math_log1p(0.0), 0.0),
        (wasm_math_log1p(-0.0), -0.0),
        (wasm_math_sin(0.0), 0.0),
        (wasm_math_sin(-0.0), -0.0),
        (wasm_math_sinh(0.0), 0.0),
        (wasm_math_sinh(-0.0), -0.0),
        (wasm_math_tan(0.0), 0.0),
        (wasm_math_tan(-0.0), -0.0),
        (wasm_math_tanh(0.0), 0.0),
        (wasm_math_tanh(-0.0), -0.0),
        (wasm_math_exp(f64::NEG_INFINITY), 0.0),
        (wasm_math_log(1.0), 0.0),
        (wasm_math_log10(1.0), 0.0),
        (wasm_math_log2(1.0), 0.0),
        (wasm_math_acos(1.0), 0.0),
        (wasm_math_acosh(1.0), 0.0),
        (wasm_math_atan2(0.0, 1.0), 0.0),
        (wasm_math_atan2(-0.0, 1.0), -0.0),
        (wasm_math_atan2(0.0, 0.0), 0.0),
        (wasm_math_atan2(-0.0, 0.0), -0.0),
        (wasm_math_atan2(1.0, f64::INFINITY), 0.0),
        (wasm_math_atan2(-1.0, f64::INFINITY), -0.0),
    ] {
        let (actual, expected): (f64, f64) = pair;
        assert_eq!(actual.to_bits(), expected.to_bits());
    }

    // Infinity and out-of-domain rows.
    assert_eq!(wasm_math_acos(-1.0), std::f64::consts::PI);
    assert!(wasm_math_acos(2.0).is_nan());
    assert!(wasm_math_acos(f64::INFINITY).is_nan());
    assert_eq!(wasm_math_acosh(f64::INFINITY), f64::INFINITY);
    assert!(wasm_math_acosh(0.0).is_nan());
    assert_eq!(wasm_math_asinh(f64::INFINITY), f64::INFINITY);
    assert_eq!(wasm_math_asinh(f64::NEG_INFINITY), f64::NEG_INFINITY);
    assert_eq!(wasm_math_atanh(1.0), f64::INFINITY);
    assert_eq!(wasm_math_atanh(-1.0), f64::NEG_INFINITY);
    assert!(wasm_math_atanh(2.0).is_nan());
    assert_eq!(wasm_math_cbrt(f64::INFINITY), f64::INFINITY);
    assert_eq!(wasm_math_cbrt(f64::NEG_INFINITY), f64::NEG_INFINITY);
    assert_eq!(wasm_math_cos(0.0), 1.0);
    assert_eq!(wasm_math_cos(-0.0), 1.0);
    assert!(wasm_math_cos(f64::INFINITY).is_nan());
    assert_eq!(wasm_math_cosh(0.0), 1.0);
    assert_eq!(wasm_math_cosh(f64::INFINITY), f64::INFINITY);
    assert_eq!(wasm_math_cosh(f64::NEG_INFINITY), f64::INFINITY);
    assert_eq!(wasm_math_exp(0.0), 1.0);
    assert_eq!(wasm_math_exp(f64::INFINITY), f64::INFINITY);
    assert_eq!(wasm_math_expm1(f64::INFINITY), f64::INFINITY);
    assert_eq!(wasm_math_expm1(f64::NEG_INFINITY), -1.0);
    assert_eq!(wasm_math_log(0.0), f64::NEG_INFINITY);
    assert_eq!(wasm_math_log(-0.0), f64::NEG_INFINITY);
    assert_eq!(wasm_math_log(f64::INFINITY), f64::INFINITY);
    assert!(wasm_math_log(-1.0).is_nan());
    assert_eq!(wasm_math_log10(0.0), f64::NEG_INFINITY);
    assert_eq!(wasm_math_log10(f64::INFINITY), f64::INFINITY);
    assert_eq!(wasm_math_log10(10.0), 1.0);
    assert!(wasm_math_log10(-1.0).is_nan());
    assert_eq!(wasm_math_log1p(-1.0), f64::NEG_INFINITY);
    assert_eq!(wasm_math_log1p(f64::INFINITY), f64::INFINITY);
    assert!(wasm_math_log1p(f64::NEG_INFINITY).is_nan());
    assert_eq!(wasm_math_log2(0.0), f64::NEG_INFINITY);
    assert_eq!(wasm_math_log2(f64::INFINITY), f64::INFINITY);
    assert_eq!(wasm_math_log2(2.0), 1.0);
    assert!(wasm_math_log2(-1.0).is_nan());
    assert!(wasm_math_sin(f64::INFINITY).is_nan());
    assert_eq!(wasm_math_sinh(f64::INFINITY), f64::INFINITY);
    assert_eq!(wasm_math_sinh(f64::NEG_INFINITY), f64::NEG_INFINITY);
    assert!(wasm_math_tan(f64::INFINITY).is_nan());
    assert_eq!(wasm_math_tanh(f64::INFINITY), 1.0);
    assert_eq!(wasm_math_tanh(f64::NEG_INFINITY), -1.0);

    // `atan2` quadrant rows are implementation-approximated multiples of
    // pi, so they pin a tight tolerance rather than exact bits.
    for (actual, expected) in [
        (wasm_math_atan2(0.0, -0.0), std::f64::consts::PI),
        (wasm_math_atan2(-0.0, -0.0), -std::f64::consts::PI),
        (
            wasm_math_atan2(f64::INFINITY, f64::INFINITY),
            std::f64::consts::FRAC_PI_4,
        ),
        (
            wasm_math_atan2(f64::INFINITY, f64::NEG_INFINITY),
            3.0 * std::f64::consts::FRAC_PI_4,
        ),
        (
            wasm_math_atan2(f64::NEG_INFINITY, f64::INFINITY),
            -std::f64::consts::FRAC_PI_4,
        ),
        (
            wasm_math_atan2(f64::NEG_INFINITY, f64::NEG_INFINITY),
            -3.0 * std::f64::consts::FRAC_PI_4,
        ),
        (wasm_math_atan2(1.0, 0.0), std::f64::consts::FRAC_PI_2),
        (wasm_math_atan2(-1.0, 0.0), -std::f64::consts::FRAC_PI_2),
        (
            wasm_math_atan2(f64::INFINITY, 1.0),
            std::f64::consts::FRAC_PI_2,
        ),
        (
            wasm_math_atan2(f64::NEG_INFINITY, 1.0),
            -std::f64::consts::FRAC_PI_2,
        ),
        (
            wasm_math_atan2(1.0, f64::NEG_INFINITY),
            std::f64::consts::PI,
        ),
        (
            wasm_math_atan2(-1.0, f64::NEG_INFINITY),
            -std::f64::consts::PI,
        ),
    ] {
        assert!(
            (actual - expected).abs() < 1e-15,
            "{actual} should approximate {expected}"
        );
    }
}

fn run_wasm_raw(source: &str) -> (i64, WasmRuntimeValueTag, i32, Option<Box<[u16]>>) {
    let engine = engine();
    let unit = engine
        .compile_script(source, CompileOptions::default())
        .expect("script compile should succeed");
    let artifact = engine.emit_wasm(&unit).expect("wasm emit should succeed");
    let program = artifact.program();
    let mode = wasm_runtime_link::plan_native_compilation(program).mode;
    let wasm_engine =
        shared_native_mode_wasm_engine(mode).expect("wasmtime engine should initialize");
    let modules = wasm_runtime_link::compile_modules(
        &wasm_engine,
        program,
        WasmModuleMemoryCachePolicy::BypassRetention,
        true,
        mode,
        WasmRuntimeImagePolicy::EmbeddedBuild,
    )
    .expect("modules should validate for the supported Wasmtime target");
    let shared_memory_backing = modules
        .host()
        .imports()
        .find_map(|import| {
            (import.module() == WASM_HOST_IMPORT_NAMESPACE
                && import.name() == WASM_HOST_IMPORT_SHARED_MEMORY)
                .then(|| match import.ty() {
                    WasmtimeExternType::Memory(memory_type) => Some(memory_type),
                    _ => None,
                })
                .flatten()
        })
        .map(|memory_type| {
            WasmSharedMemoryBacking::new(
                WasmtimeSharedMemory::new(&wasm_engine, memory_type)
                    .expect("shared memory should be created"),
            )
        });
    let mut store = WasmtimeStore::new(
        &wasm_engine,
        WasmHostState {
            user_data: None,
            embedding: None,
            weak: wasm_gc_weak_host::WeakGcState::default(),
            #[cfg(feature = "native-source-compiler")]
            native_source_compiler: None,
            #[cfg(feature = "native-source-compiler")]
            native_template_instances: Default::default(),
            realm: engine.realm.clone(),
            output_events: WasmOutputEvents::DelegateOnly,
            intl_kernel: shared_embedded_intl_kernel()
                .expect("embedded Intl kernel should initialize")
                .into(),
            can_block: true,
            monotonic_clock_origin: engine.realm.host_clock().monotonic_instant(),
            async_waiters: WasmStoreAsyncWaiters::new(shared_memory_backing.clone()),
            shared_memory_backing,
            agent_group: None,
            agent_commands: None,
            agent_leaving: None,
            limits: WasmtimeStoreLimitsBuilder::new()
                .memory_size(WASM_STORE_MEMORY_CAP_BYTES)
                .build(),
        },
    );
    store.limiter(|state| &mut state.limits);
    store.set_epoch_deadline(u64::MAX / 2);
    let mut linker = WasmtimeLinker::new(&wasm_engine);
    if let Some(private_memory_type) = modules.host().imports().find_map(|import| {
        (import.module() == WASM_HOST_IMPORT_NAMESPACE
            && import.name() == WASM_HOST_IMPORT_PRIVATE_MEMORY)
            .then(|| match import.ty() {
                WasmtimeExternType::Memory(memory_type) => Some(memory_type),
                _ => None,
            })
            .flatten()
    }) {
        let private_memory = WasmtimeMemory::new(&mut store, private_memory_type)
            .expect("private memory should be created");
        linker
            .define(
                &store,
                WASM_HOST_IMPORT_NAMESPACE,
                WASM_HOST_IMPORT_PRIVATE_MEMORY,
                private_memory,
            )
            .expect("private memory import should link");
    }
    if let Some(shared_memory) = store
        .data()
        .shared_memory_backing
        .as_ref()
        .map(|backing| backing.memory.clone())
    {
        linker
            .define(
                &store,
                WASM_HOST_IMPORT_NAMESPACE,
                WASM_HOST_IMPORT_SHARED_MEMORY,
                shared_memory,
            )
            .expect("shared memory import should link");
    }
    wasm_gc_host::link(&mut linker, modules.host())
        .expect("actual GC collector and resource imports should link");
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_REJECT_RUNTIME_SEMANTICS,
            wasm_reject_runtime_semantics,
        )
        .expect("dynamic source capability import should link");
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_AGENT_CAN_SUSPEND,
            |_caller: WasmtimeCaller<'_, WasmHostState>| -> i32 { 1 },
        )
        .expect("host agent capability import should link");
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_AGENT_CALL,
            wasm_agent_host::agent_call,
        )
        .expect("actual host agent dispatch should link");
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_MONOTONIC_CLOCK_NANOS,
            |caller: WasmtimeCaller<'_, WasmHostState>| -> i64 {
                caller
                    .data()
                    .realm
                    .host_clock()
                    .monotonic_instant()
                    .saturating_duration_since(caller.data().monotonic_clock_origin)
                    .as_i64_saturating()
            },
        )
        .expect("actual monotonic host clock should link");
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_SLEEP_NANOS,
            |_caller: WasmtimeCaller<'_, WasmHostState>, nanos: i64| {
                let Ok(nanos) = u64::try_from(nanos) else {
                    return;
                };
                std::thread::sleep(std::time::Duration::from_nanos(nanos.min(1_000_000)));
            },
        )
        .expect("actual host sleep operation should link");
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_PRINT_LINE_UTF8,
            |_caller: WasmtimeCaller<'_, WasmHostState>,
             _ptr: i32,
             _len: i32|
             -> wasmtime::Result<()> { Ok(()) },
        )
        .expect("host print import should link");
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_NUMBER_POW,
            wasm_number_pow,
        )
        .expect("host number pow import should link");
    for (name, func) in WASM_MATH_UNARY_IMPORTS {
        linker
            .func_wrap(WASM_HOST_IMPORT_NAMESPACE, name, func)
            .unwrap_or_else(|err| panic!("host math import {name} should link: {err}"));
    }
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_MATH_ATAN2,
            wasm_math_atan2,
        )
        .expect("host math atan2 import should link");
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_WALL_CLOCK_MILLIS,
            |caller: WasmtimeCaller<'_, WasmHostState>| -> f64 {
                caller
                    .data()
                    .realm
                    .host_clock()
                    .utc_epoch_milliseconds()
                    .get() as f64
            },
        )
        .expect("host wall clock import should link");
    linker
        .func_wrap(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_RANDOM_F64,
            wasm_random_f64,
        )
        .expect("host random import should link");
    let (instance, _) = wasm_runtime_link::instantiate(&mut linker, &mut store, &modules)
        .expect("instance should instantiate");
    let main = instance
        .get_typed_func::<(), GcMainCompletion>(&mut store, "main")
        .expect("main export should exist");
    let mut roots = wasmtime::RootScope::new(&mut store);
    let result @ (_, payload, _, completion_kind, _) =
        main.call(&mut roots, ()).expect("main should run");
    let (tag, _, value) = wasm_gc_completion::observe(&mut roots, result)
        .expect("Main result follows the actual GC ABI")
        .into_parts();
    let bytes = match value {
        ObservedJsValue::String(units) => Some(units),
        ObservedJsValue::Undefined
        | ObservedJsValue::Null
        | ObservedJsValue::Boolean(_)
        | ObservedJsValue::Number(_)
        | ObservedJsValue::BigInt(_)
        | ObservedJsValue::Symbol
        | ObservedJsValue::Object => None,
    };
    (payload, tag, completion_kind, bytes)
}

#[test]
fn raw_wasm_test_linker_supports_date_now_wall_clock_import() {
    let (payload, tag, completion_kind, _) = run_wasm_raw("Date.now();");
    let milliseconds = f64::from_bits(payload as u64);

    assert_eq!(tag.value_kind(), ValueKind::Number);
    assert_eq!(completion_kind, 0);
    assert!((946_684_800_000.0..=4_102_444_800_000.0).contains(&milliseconds));
}

fn wasm_import_export_names(bytes: &[u8]) -> (Vec<String>, Vec<String>) {
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("wasm parse should succeed") {
            Payload::ImportSection(reader) => {
                for import in reader {
                    match import.expect("import should decode") {
                        Imports::Single(_, import) => {
                            imports.push(format!("{}::{}", import.module, import.name));
                        }
                        Imports::Compact1 { module, items } => {
                            for item in items {
                                let item = item.expect("compact import should decode");
                                imports.push(format!("{module}::{}", item.name));
                            }
                        }
                        Imports::Compact2 { module, names, .. } => {
                            for name in names {
                                imports.push(format!(
                                    "{module}::{}",
                                    name.expect("compact import name should decode")
                                ));
                            }
                        }
                    }
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.expect("export should decode");
                    exports.push(export.name.to_string());
                }
            }
            _ => {}
        }
    }
    imports.sort();
    exports.sort();
    (imports, exports)
}

#[test]
fn wasm_atomics_wait_uses_host_agent_suspend_capability() {
    let source = "
            const view = new Int32Array(new SharedArrayBuffer(4));
            try {
                Atomics.wait(view, 0, 0, 0);
                false;
            } catch (error) {
                error instanceof TypeError;
            }
        ";
    let cannot_suspend = engine()
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                can_block: false,
                ..RunOptions::default()
            },
        )
        .expect("non-suspendable agent should run");
    assert!(
        cannot_suspend.note.contains("boolean(true)"),
        "{}",
        cannot_suspend.note
    );

    let can_suspend = engine()
        .run_script(
            "const view = new Int32Array(new SharedArrayBuffer(4));
                 Atomics.wait(view, 0, 0, 0) === 'timed-out';",
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                can_block: true,
                ..RunOptions::default()
            },
        )
        .expect("suspendable agent should run");
    assert!(
        can_suspend.note.contains("boolean(true)"),
        "{}",
        can_suspend.note
    );
}

#[test]
fn compile_script_marks_script_goal() {
    let unit = engine()
        .compile_script("let x = 1;", CompileOptions::default())
        .expect("script compile stub should succeed");
    assert_eq!(unit.source.goal, ParseGoal::Script);
    assert!(unit
        .ir
        .invariants
        .contains(&scriptw_ir::LoweringInvariant::DirectJsToWasm));
}

#[test]
fn compile_options_authorize_test262_host_globals_explicitly() {
    let source = "gc; parseInt; __scriptwCreateRealm;";
    let engine = Engine::new(RealmBuilder::new().build());
    let product = engine
        .compile_script(source, CompileOptions::default())
        .expect("product source should lower");
    let product_hosts = &product
        .ir
        .script
        .expect("script ir should exist")
        .host_builtins;
    assert!(product_hosts.contains(&scriptw_ir::HostBuiltinId::Gc));
    assert!(product_hosts.contains(&scriptw_ir::HostBuiltinId::ParseInt));
    assert!(!product_hosts.contains(&scriptw_ir::HostBuiltinId::CreateRealm));

    let test262 = engine
        .compile_script(source, test262_compile_options())
        .expect("Test262-authorized source should lower");
    assert!(test262
        .ir
        .script
        .expect("script ir should exist")
        .host_builtins
        .contains(&scriptw_ir::HostBuiltinId::CreateRealm));
}

#[test]
fn compile_script_preserves_structured_parse_diagnostic() {
    let err = engine()
        .compile_script("let x = ;", CompileOptions::default())
        .expect_err("malformed script should fail during parse");
    let diagnostic = err
        .parse_diagnostic()
        .expect("engine error should retain parse diagnostic");
    assert_eq!(
        diagnostic.kind(),
        scriptw_front::ParseDiagnosticKind::MalformedJavaScript
    );
    assert_eq!(
        diagnostic.phase(),
        scriptw_front::ParseDiagnosticPhase::Parse
    );
    assert_eq!(diagnostic.error_type(), Some("SyntaxError"));
    assert_eq!(diagnostic.code, scriptw_front::ParseCode::Malformed);
    assert!(diagnostic.span.is_some());
}

#[test]
fn engine_error_preserves_structured_ir_early_error_diagnostic() {
    // The code is a real inhabitant of the domain. It used to be
    // `"E_TEST_EARLY"`, a code no producer emits, which a `&'static str`
    // field happily accepted — the mistake class this type exists to make
    // unrepresentable.
    let source_diagnostic = IrDiagnostic::rejected(
        scriptw_ir::EarlyErrorCode::ObjectDuplicateProto,
        "early error: test",
        None,
    );
    let err = EngineError::from_ir_diagnostic(source_diagnostic.clone());

    assert_eq!(err.message(), source_diagnostic.message);
    assert_eq!(err.ir_diagnostic(), Some(&source_diagnostic));
    assert!(err.parse_diagnostic().is_none());
}

#[test]
fn compile_script_reports_front_end_early_error_diagnostic() {
    let err = engine()
        .compile_script(
            "({ __proto__: null, __proto__: {} });",
            CompileOptions::default(),
        )
        .expect_err("duplicate __proto__ prototype setters should be early error");
    let diagnostic = err
        .parse_diagnostic()
        .expect("engine error should retain front-end diagnostic");
    assert_eq!(
        diagnostic.code,
        scriptw_front::ParseCode::Early(scriptw_front::ParseClassified::from_parse_table(
            scriptw_front::EarlyErrorCode::ObjectDuplicateProto
        ))
    );
    assert_eq!(
        diagnostic.phase(),
        scriptw_front::ParseDiagnosticPhase::Early
    );
    assert_eq!(diagnostic.error_type(), Some("SyntaxError"));
    assert!(err.ir_diagnostic().is_none());
}

#[test]
fn wasm_emit_reports_unsupported_slice_precisely() {
    // Destructured parameters and textual eval now compile. A prepared
    // Realm Script still cannot import through an unknown module specifier
    // without a defining-module bridge; retain its precise typed gap.
    let source = format!(
        "{REALM_EVAL}(\"import(globalThis.moduleSpecifier);\");",
        REALM_EVAL = scriptw_ir::REALM_EVAL_SCRIPT_NAME
    );
    let unit = engine()
        .compile_script(&source, test262_compile_options())
        .expect("script compile should succeed");
    let err = engine()
        .emit_wasm(&unit)
        .err()
        .expect("actual remaining source gap must be rejected");
    assert!(
        err.message()
            .contains("unsupported in scriptw wasm-aot first slice"),
        "err: {}",
        err.message()
    );
    assert_eq!(
        err.ir_diagnostic()
            .and_then(IrDiagnostic::unsupported_feature),
        Some(scriptw_ir::UnsupportedFeature::DynamicSource(
            scriptw_ir::DynamicSourceGap::aot_known_source(
                scriptw_ir::DynamicSourceKind::RealmEvalScript
            ),
        )),
    );
}

#[test]
fn run_defaults_to_wasm_aot() {
    // Product invariant: Wasm-AOT is the default execution backend
    // everywhere. Selecting spec-exec (the hidden differential oracle)
    // requires an explicit `RunOptions::backend` override.
    assert_eq!(ExecutionBackend::default(), ExecutionBackend::WasmAot);
    assert_eq!(RunOptions::default().backend, ExecutionBackend::WasmAot);
    let outcome = engine()
        .run_script("1 + 1;", CompileOptions::default(), RunOptions::default())
        .expect("wasm-aot should run a simple script by default");
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
}

#[path = "tests/arrays.rs"]
mod arrays;
#[path = "tests/cache.rs"]
mod cache;
#[path = "tests/control_flow.rs"]
mod control_flow;
#[path = "tests/execution.rs"]
mod execution;
#[path = "tests/foreign_capabilities.rs"]
mod foreign_capabilities;
#[path = "tests/functions.rs"]
mod functions;
#[path = "tests/intl.rs"]
mod intl;
#[path = "tests/modules.rs"]
mod modules;
#[path = "tests/objects.rs"]
mod objects;
#[path = "tests/strings.rs"]
mod strings;
