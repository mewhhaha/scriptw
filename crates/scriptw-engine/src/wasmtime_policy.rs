use scriptw_ir::{WasmWeakReachabilityCapability, PRODUCT_WASM_WEAK_REACHABILITY};
use wasmtime::{Collector, Config, Engine};

/// The required collector capability of the pinned Wasmtime runtime.
///
/// Copying collects unreachable cycles in the semantic Wasm-GC graph. The
/// compiler's strong reference ABI and rooted host boundary use this required
/// collector; weak/ephemeron reachability remains a separate runtime capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmGcCapability {
    CopyingWithCycleCollection,
}

impl WasmGcCapability {
    pub const fn report(self) -> &'static str {
        match self {
            Self::CopyingWithCycleCollection => {
                "collector=copying cycle-collection=available js-semantic-heap=wasm-gc"
            }
        }
    }
}

/// Complete proposal/collector policy for every product Wasmtime engine.
///
/// Its field is private and the product constant below is the only value. A
/// second engine profile may tune native compilation, but it cannot silently
/// choose a different Wasm feature surface or collector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WasmtimeRuntimePolicy {
    gc: WasmGcCapability,
    weak_reachability: WasmWeakReachabilityCapability,
}

pub(crate) const PRODUCT_WASMTIME_POLICY: WasmtimeRuntimePolicy = WasmtimeRuntimePolicy {
    gc: WasmGcCapability::CopyingWithCycleCollection,
    weak_reachability: PRODUCT_WASM_WEAK_REACHABILITY,
};

impl WasmtimeRuntimePolicy {
    pub(crate) const fn gc_capability(self) -> WasmGcCapability {
        self.gc
    }

    pub(crate) const fn weak_reachability_capability(self) -> WasmWeakReachabilityCapability {
        self.weak_reachability
    }

    pub(crate) fn report(self) -> String {
        format!(
            "reference-types=required function-references=required gc=required exceptions=required threads=required shared-memory=required {} {}",
            self.gc.report(),
            self.weak_reachability.report(),
        )
    }

    /// Validate the instantiated engine before either native profile is cached.
    /// The public getters are the pinned47 authority, rather than compiler
    /// defaults or an inference from enabled Cargo feature names.
    pub(crate) fn verify_engine(self, engine: &Engine) -> Result<(), &'static str> {
        let features = engine.get_wasm_features();
        if !features.gc_types() {
            return Err("required Wasm GC runtime support is unavailable");
        }
        match self.gc {
            WasmGcCapability::CopyingWithCycleCollection => {
                if engine.get_collector() != Some(Collector::Copying) {
                    return Err("required copying collector was not explicitly selected");
                }
            }
        }
        if !features.reference_types() {
            return Err("required Wasm reference types are unavailable");
        }
        if !features.function_references() {
            return Err("required Wasm typed function references are unavailable");
        }
        if !features.gc() {
            return Err("required Wasm GC structs and arrays are unavailable");
        }
        if !features.exceptions() {
            return Err("required Wasm exceptions are unavailable");
        }
        if !features.threads() {
            return Err("required Wasm threads are unavailable");
        }
        if !engine.get_shared_memory() {
            return Err("required Wasmtime shared-memory allocation is unavailable");
        }
        match self.weak_reachability {
            WasmWeakReachabilityCapability::Unavailable => {}
            WasmWeakReachabilityCapability::CopyingCollector => {
                if engine.get_collector() != Some(Collector::Copying) {
                    return Err("weak reachability requires the patched copying collector");
                }
            }
        }
        Ok(())
    }

    pub(crate) fn configure(self, config: &mut Config) {
        config.gc_support(true);
        config.wasm_threads(true);
        // Wasmtime 47 gates host shared-memory allocation separately from
        // the Wasm threads proposal. Both are required by this product.
        config.shared_memory(true);
        config.wasm_multi_memory(true);
        config.wasm_reference_types(true);
        config.wasm_function_references(true);
        config.wasm_gc(true);
        config.wasm_exceptions(true);
        config.wasm_tail_call(true);
        // P and R name their unlikely branches in the branch hinting section;
        // the translation lays out what those reach behind each function's
        // other code. It changes no behavior, and an engine without it loads
        // the same native code.
        config.wasm_branch_hinting(true);

        match self.gc {
            WasmGcCapability::CopyingWithCycleCollection => {
                config.collector(Collector::Copying);
            }
        }

        match self.weak_reachability {
            WasmWeakReachabilityCapability::Unavailable => {}
            WasmWeakReachabilityCapability::CopyingCollector => {
                config.collector(Collector::Copying);
            }
        }
    }
}
