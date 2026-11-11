use super::*;
use crate::wasmtime_config::{base_config, WasmNativeCompilationMode};
use wasmtime::{CacheConfig, WasmBacktraceDetails};

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "scriptw-runtime-image-cache-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        Self(path)
    }

    fn cache(&self) -> Cache {
        let mut config = CacheConfig::new();
        config
            .with_directory(self.0.clone())
            .with_files_total_size_soft_limit(64 * 1024 * 1024);
        Cache::new(config).unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn config(mode: WasmNativeCompilationMode, allocator: wasmtime::RegallocAlgorithm) -> Config {
    let mut config = base_config(mode, WasmBacktraceDetails::Disable);
    config.cranelift_regalloc_algorithm(allocator);
    config.target(env!("SCRIPTW_RUNTIME_BUILD_TARGET")).unwrap();
    config.parallel_compilation(false);
    config
}

fn answer_wasm(value: u8) -> Vec<u8> {
    // (module (func (export "answer") (result i32) i32.const VALUE))
    vec![
        0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 2, 1, 0, 7, 10, 1, 6, 97, 110, 115,
        119, 101, 114, 0, 0, 10, 6, 1, 4, 0, 65, value, 11,
    ]
}

fn execute(native: &[u8], value: i32) {
    let execution = Engine::new(&base_config(
        WasmNativeCompilationMode::Fast,
        WasmBacktraceDetails::Disable,
    ))
    .unwrap();
    // SAFETY: native was just produced by this test's pinned Wasmtime compiler
    // from answer_wasm, directly or through Wasmtime's own compilation cache.
    let module = unsafe { Module::deserialize(&execution, native) }.unwrap();
    let mut store = wasmtime::Store::new(&execution, ());
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    assert_eq!(
        instance
            .get_typed_func::<(), i32>(&mut store, "answer")
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        value
    );
}

#[test]
fn module_cache_cold_and_warm_images_equal_direct_precompile_and_execute() {
    let scratch = Scratch::new("parity");
    let compile = |cache| {
        RuntimeImageCompiler::new(
            config(
                WasmNativeCompilationMode::SizeOptimized,
                wasmtime::RegallocAlgorithm::Backtracking,
            ),
            cache,
            env!("SCRIPTW_NATIVE_COMPILER_FINGERPRINT"),
        )
        .unwrap()
    };
    let wasm = answer_wasm(7);
    let direct = compile(None);
    let expected = direct.compile_wasm(&wasm).unwrap();
    assert_eq!(direct.module_cache_counters(), None);
    let cold = compile(Some(scratch.cache()));
    let cold_image = cold.compile_wasm(&wasm).unwrap();
    assert_eq!(cold.module_cache_counters(), Some((0, 1)));
    assert_eq!(cold_image, expected);
    drop(cold);
    let warm = compile(Some(scratch.cache()));
    let warm_image = warm.compile_wasm(&wasm).unwrap();
    assert_eq!(warm.module_cache_counters(), Some((1, 0)));
    assert_eq!(warm_image, expected);
    execute(&cold_image, 7);
    execute(&warm_image, 7);
}

#[test]
fn module_cache_changes_raw_wasm_allocator_and_mode_without_stale_execution() {
    let scratch = Scratch::new("invalidation");
    let compile = |value, mode, allocator| {
        let compiler = RuntimeImageCompiler::new(
            config(mode, allocator),
            Some(scratch.cache()),
            env!("SCRIPTW_NATIVE_COMPILER_FINGERPRINT"),
        )
        .unwrap();
        let native = compiler.compile_wasm(&answer_wasm(value)).unwrap();
        assert_eq!(compiler.module_cache_counters(), Some((0, 1)));
        execute(&native, i32::from(value));
        native
    };
    let mode = WasmNativeCompilationMode::SizeOptimized;
    let allocator = wasmtime::RegallocAlgorithm::Backtracking;
    let original = compile(7, mode, allocator);
    let changed = compile(8, mode, allocator);
    assert_ne!(original, changed);
    compile(7, mode, wasmtime::RegallocAlgorithm::SinglePass);
    compile(
        7,
        WasmNativeCompilationMode::Fast,
        wasmtime::RegallocAlgorithm::SinglePass,
    );
}

#[test]
fn native_dependency_identity_invalidates_modules_and_rejects_incompatible_images() {
    let scratch = Scratch::new("native-dependencies");
    let identity = env!("SCRIPTW_NATIVE_COMPILER_FINGERPRINT");
    let mut changed = identity.as_bytes().to_vec();
    changed[0] = if changed[0] == b'0' { b'1' } else { b'0' };
    let changed = String::from_utf8(changed).unwrap();
    let compile = |identity| {
        RuntimeImageCompiler::new(
            config(
                WasmNativeCompilationMode::SizeOptimized,
                wasmtime::RegallocAlgorithm::Backtracking,
            ),
            Some(scratch.cache()),
            identity,
        )
        .unwrap()
    };
    let original = compile(identity);
    let first = original.compile_wasm(&answer_wasm(7)).unwrap();
    assert_eq!(original.module_cache_counters(), Some((0, 1)));
    let different = compile(&changed);
    let second = different.compile_wasm(&answer_wasm(7)).unwrap();
    assert_eq!(different.module_cache_counters(), Some((0, 1)));
    assert_ne!(first, second);
    assert_eq!(different.compile_wasm(&answer_wasm(7)).unwrap(), second);
    assert_eq!(different.module_cache_counters(), Some((1, 1)));
    let old_engine = Engine::new(&base_config(
        WasmNativeCompilationMode::Fast,
        WasmBacktraceDetails::Disable,
    ))
    .unwrap();
    // SAFETY: both images are produced above by the pinned Wasmtime compiler.
    assert!(unsafe { Module::deserialize(&old_engine, &second) }.is_err());
    let mut current_config = base_config(
        WasmNativeCompilationMode::Fast,
        WasmBacktraceDetails::Disable,
    );
    crate::wasmtime_config::configure_native_version(&mut current_config, &changed);
    let current_engine = Engine::new(&current_config).unwrap();
    assert!(unsafe { Module::deserialize(&current_engine, &first) }.is_err());
    assert!(unsafe { Module::deserialize(&current_engine, &second) }.is_ok());
}
