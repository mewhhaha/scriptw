//! The native compilation settings shared by Cargo's producer and every
//! product Engine. Runtime adapters add cache/pool/diagnostic policy only.

use crate::wasmtime_policy::PRODUCT_WASMTIME_POLICY;
use wasmtime::{Config, ModuleVersionStrategy, OptLevel, RegallocAlgorithm, WasmBacktraceDetails};

pub(crate) const CONFIGURATION_SCHEMA: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WasmNativeCompilationMode {
    Fast,
    SizeOptimized,
    Speed,
}

impl WasmNativeCompilationMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::SizeOptimized => "size-optimized",
            Self::Speed => "speed",
        }
    }
}

/// Large AOT native frames use at most half the real compiler/execution stack.
pub(crate) const WASM_MAX_STACK_SIZE: usize = 32 * 1024 * 1024;
/// Both lowering/emission and synchronous Wasm need a real host stack of this size.
pub(crate) const ENGINE_WORKER_STACK_SIZE: usize = 64 * 1024 * 1024;
const _: () = assert!(WASM_MAX_STACK_SIZE > 0 && WASM_MAX_STACK_SIZE < ENGINE_WORKER_STACK_SIZE);

/// Per-Store cap on each memory and the GC heap (`StoreLimits`, a pool's
/// `max_memory_size`); embedding does not change it.
pub(crate) const WASM_STORE_MEMORY_CAP_BYTES: usize = 1024 * 1024 * 1024;
/// Reservation of every linear memory and the GC heap: equal, as a Store pool
/// requires, and at least the cap, so nothing moves.
pub(crate) const WASM_HEAP_RESERVATION_BYTES: u64 = 1 << 32;
const _: () = assert!(WASM_HEAP_RESERVATION_BYTES >= WASM_STORE_MEMORY_CAP_BYTES as u64);
pub(crate) const WASM_LINEAR_MEMORY_GUARD_BYTES: u64 = 32 * 1024 * 1024;
// The copying collector starts with a 4 MiB nursery and two nominal 14 MiB
// old semi-spaces. Pages commit on demand. The Store limit still caps growth.
pub(crate) const WASM_GC_HEAP_INITIAL_BYTES: usize = 32 * 1024 * 1024;
const _: () = assert!(WASM_GC_HEAP_INITIAL_BYTES < WASM_STORE_MEMORY_CAP_BYTES);

pub(crate) fn base_config(
    mode: WasmNativeCompilationMode,
    backtrace_details: WasmBacktraceDetails,
) -> Config {
    let mut config = Config::new();
    config.wasm_backtrace_details(backtrace_details);
    config.cranelift_opt_level(match mode {
        WasmNativeCompilationMode::Fast => OptLevel::None,
        WasmNativeCompilationMode::SizeOptimized => OptLevel::SpeedAndSize,
        WasmNativeCompilationMode::Speed => OptLevel::Speed,
    });
    config.cranelift_regalloc_algorithm(RegallocAlgorithm::SinglePass);
    config.gc_heap_initial_size(WASM_GC_HEAP_INITIAL_BYTES as u64);
    // Cover the 32-bit reference range: protected pages replace heap checks.
    config.gc_heap_reservation(WASM_HEAP_RESERVATION_BYTES);
    config.gc_heap_guard_size(WASM_LINEAR_MEMORY_GUARD_BYTES);
    config.gc_heap_reservation_for_growth(0);
    config.gc_heap_may_move(false);
    config.max_wasm_stack(WASM_MAX_STACK_SIZE);
    config.async_stack_size(ENGINE_WORKER_STACK_SIZE);
    // Equal to the GC heap's in every Engine (`pooled_stores`), so one native
    // image serves pooled and on-demand Engines. Shared memories ignore
    // StoreLimits: admission (`host_memory_imports`) refuses a shared import
    // above the 1 GiB cap and gives the private memory that maximum.
    config.memory_reservation(WASM_HEAP_RESERVATION_BYTES);
    config.memory_reservation_for_growth(0);
    // Growth never relocates; saying so lets a deadline publish the range.
    config.memory_may_move(false);
    config.memory_guard_size(WASM_LINEAR_MEMORY_GUARD_BYTES);
    config.guard_before_linear_memory(true);
    PRODUCT_WASMTIME_POLICY.configure(&mut config);
    // Compiled code carries no deadline checks: `execution_deadline` stops a
    // Store by removing access to its GC heap and private memory.
    config.epoch_interruption(false);
    // Engine's Cargo build supplies this identity. The runtime-image build
    // script computes and installs the same version explicitly before compiling.
    if let Some(identity) = option_env!("SCRIPTW_NATIVE_COMPILER_FINGERPRINT") {
        configure_native_version(&mut config, identity);
    }
    config
}

pub(crate) fn configure_native_version(config: &mut Config, identity: &str) {
    assert!(
        identity.len() == 64
            && identity
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    config
        .module_version(ModuleVersionStrategy::Custom(format!(
            "scriptw-native-v1-{identity}"
        )))
        .expect("canonical native compiler identity fits Wasmtime's version bound");
}
