use scriptw_aot_wasm::{
    WasmModuleEvaluationStatus, WasmRuntimeValueTag, MODULE_EVALUATION_STATUS_EXPORT,
};
use scriptw_front::{parse, ParseDiagnostic, ParseGoal, ParseOptions, ParsedSource, SourceUnit};
use scriptw_intl::{
    CanonicalizeLocale, EmbeddedIntlProvider, IntlDataIdentity, IntlHostOp, IntlKernel, LocaleId,
    LocaleTransformError, LocaleTransformRequest, MaximizeLocale, MinimizeLocale,
    INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION,
};
use scriptw_ir::{
    script_has_dynamic_import, CompletionKindIr, DynamicSourceRuntimeOperation, IrDiagnostic,
    ProgramIr, RuntimeSemanticGap, RuntimeSemanticRejection, ValueKind,
};
pub use scriptw_runtime::rooted_snapshot::{SnapshotCompletion, SnapshotLimits, SnapshotOutcome};
use scriptw_runtime::AgentHostOperation;
use sha2::{Digest, Sha256};
use wasmparser::{Parser as WasmParser, Payload as WasmPayload};
use wasmtime::{
    Caller as WasmtimeCaller, Config as WasmtimeConfig, Engine as WasmtimeEngine,
    Extern as WasmtimeExtern, ExternType as WasmtimeExternType, Linker as WasmtimeLinker,
    Memory as WasmtimeMemory, Module as WasmtimeModule, SharedMemory as WasmtimeSharedMemory,
    Store as WasmtimeStore, StoreLimits as WasmtimeStoreLimits,
    StoreLimitsBuilder as WasmtimeStoreLimitsBuilder, Trap as WasmtimeTrap, Val as WasmtimeVal,
};

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(test)]
mod agent_failure_tests;
mod cache;
mod compilation_profile;
mod compiler_cache_identity;
mod compiler_identity;
mod compiler_inspection;
pub use compilation_profile::{
    ScriptCompilationProfile, WasmArtifactFootprint, WasmSectionFootprint,
};
pub use compiler_inspection::{CompilerInspectionFailure, CompilerInspectionStage};
mod runtime_profile;
pub use compiler_cache_identity::{
    compiler_cache_identity, CompilerCacheIdentity, CompilerCacheImageHashAlgorithm,
};
pub use compiler_identity::{
    CompilerCommitId, CompilerDigest, CompilerIdentity, CompilerSourceRevision,
};
pub use memory_module_cache::{
    wasm_runtime_module_cache_observation, WasmRuntimeModuleCacheObservation,
};
pub use runtime_profile::{
    wasm_runtime_profile_policy, WasmProcessMemoryObservation, WasmProcessMemorySample,
    WasmProcessMemoryScope, WasmProcessMemoryUnavailable, WasmProfileGcInitialCapacity,
    WasmProfileInlining, WasmProfileInliningLimits, WasmProfileNativeMode,
    WasmProfileRegisterAllocation, WasmProgramNativeCompilationProfile,
    WasmRuntimeEmbeddedImageTimings, WasmRuntimeHeapObservation, WasmRuntimeHostImportCacheOutcome,
    WasmRuntimeMatchedImageCacheOutcome, WasmRuntimeMetricUnavailable,
    WasmRuntimeModuleCacheOutcome, WasmRuntimeModuleLoading, WasmRuntimeProfile,
    WasmRuntimeProfileOptions, WasmRuntimeTimings,
};
mod embedding;
#[cfg(feature = "benchmark-module-images")]
pub use embedding::{
    BenchmarkModuleImage, BenchmarkModuleImages, BenchmarkModuleSection, BenchmarkModuleSections,
};
mod execution_failure;
pub use embedding::{
    configure_precompiled_wasm_runtime, configure_wasm_runtime, throw_foreign_error,
    EmbeddedEvaluation, EmbeddedInstance, EmbeddedRuntime, ForeignCompleter, ForeignValue,
    JsCompletion, JsPromiseState, JsValue, JsValueKind, PrecompiledEmbeddedProgram, WasmStoreState,
};
mod global_script_prelude;
mod graph_observation;
mod host_imports;
mod portable_bundle;
mod prepared_source_catalog;
pub use graph_observation::GraphRunOutcome;
mod wasm_rooted_snapshot;
mod wasm_runtime_link;
pub use execution_failure::WasmExecutionFailureKind;
use execution_failure::{finish_wasm_execution, EngineExecutionFailure};
pub use wasm_runtime_link::LinkedRuntime;
use wasm_runtime_link::WasmProgramRef;
mod intl_collator_host;
mod intl_data_images;
mod intl_datetime_host;
mod intl_display_names_host;
mod intl_duration_host;
#[cfg(test)]
mod intl_host_probe;
mod intl_list_host;
mod intl_locale_host;
mod intl_number_host;
mod intl_plural_host;
mod intl_relative_time_host;
mod intl_segmenter_host;
mod intl_time_zone_host;
mod memory_module_cache;
use memory_module_cache::MemoryWasmModuleCache;
mod embedded_runtime;
mod module_loader;
#[cfg(test)]
mod system_time_zone_host_tests;
mod wasm_shared_resource;
mod wasmtime_config;
mod wasmtime_policy;
use wasm_shared_resource::{
    WasmSharedBufferResource, WasmSharedMemoryBacking, WasmStoreAsyncWaiters,
};
mod wasm_agent_control;
mod wasm_agent_host;
mod wasm_agent_resource_host;
use wasm_agent_control::{AgentReceiveError, WasmAgentExecutionControl};
mod wasm_gc_byte_array_host;
#[cfg(test)]
use wasmtime_config::WASM_MAX_STACK_SIZE;
use wasmtime_config::{
    WasmNativeCompilationMode, ENGINE_WORKER_STACK_SIZE, WASM_STORE_MEMORY_CAP_BYTES,
};
#[cfg(test)]
#[path = "../tests/unit/native_compilation_dispatch.rs"]
mod native_compilation_dispatch_tests;
#[cfg(test)]
mod native_direct_eval_compiler_tests;
#[cfg(test)]
mod native_function_compiler_tests;
#[cfg(test)]
mod native_global_script_compiler_tests;
#[cfg(test)]
mod native_language_feature_policy_tests;
#[cfg(test)]
mod source_compilation_profile_tests;
mod wasm_compilation_policy;
mod wasm_gc_completion;
mod wasm_gc_host;
mod wasm_native_direct_eval_compiler;
mod wasm_native_function_compiler;
mod wasm_native_global_script_compiler;
mod wasm_native_program_template;
mod wasm_native_source_compiler;
pub use wasm_compilation_policy::WasmCompilationPolicy;
mod wasm_gc_intl_host;
mod wasm_gc_weak_host;
use wasm_gc_completion::GcMainCompletion;
use wasmtime::AsContext;

pub use cache::{cache_status, prune_caches, CacheDirectoryStatus, CachePruneReport, CacheStatus};
pub use module_loader::{
    load_module_graph, FilesystemModuleLoader, HostModuleLoader, LoadedModule, LoadedModuleKind,
    ModuleEntry, ModuleKey, ModuleLoadError, ModuleRequestKeyIr,
};
pub use scriptw_aot_wasm::PromiseRejectionPolicy;
pub use scriptw_intl::{
    CustomIntlProfile, CustomListProfile, CustomProfileId, IntlCompilationProfile,
    IntlDataSelection, SelectedIntlDataBundle,
};
pub use scriptw_ir::foreign;
pub use scriptw_ir::foreign::ForeignModuleRegistry;
pub use scriptw_ir::{
    ConformanceOwner, HostSurfacePolicy, LanguageFeaturePolicy, PrivateElementExtensibility,
};
pub use scriptw_ir::{RuntimeUnavailableCapability, WasmWeakReachabilityCapability};
pub use scriptw_runtime::OracleExceptionPhase;
pub use wasmtime_policy::WasmGcCapability;
use wasmtime_policy::{WasmtimeRuntimePolicy, PRODUCT_WASMTIME_POLICY};

const WASM_THROW_ERROR_NAME_EXPORT: &str = "throw_error_name";
const WASM_THROW_ERROR_MESSAGE_EXPORT: &str = "throw_error_message";
const WASM_THROW_ERROR_CONSTRUCTOR_NAME_EXPORT: &str = "throw_error_constructor_name";
const WASM_HOST_IMPORT_NAMESPACE: &str = "scriptw_host";
const WASM_HOST_IMPORT_AGENT_CAN_SUSPEND: &str = "agent_can_suspend";
const WASM_HOST_IMPORT_PRINT_LINE_UTF8: &str = "print_line_utf8";
const WASM_HOST_IMPORT_NUMBER_POW: &str = "number_pow";
const WASM_HOST_IMPORT_PRIVATE_MEMORY: &str = "private_memory";
const WASM_HOST_IMPORT_SHARED_MEMORY: &str = "shared_memory";
const WASM_HOST_IMPORT_AGENT_CALL: &str = "agent_call";
const WASM_HOST_IMPORT_INTL_CALL: &str = scriptw_aot_wasm::GcHostImport::IntlProviderCall.name();
const WASM_HOST_IMPORT_SYSTEM_TIME_ZONE: &str =
    scriptw_aot_wasm::GcHostImport::SystemTimeZoneSnapshot.name();
const WASM_HOST_IMPORT_RANDOM_F64: &str = "random_f64";
const WASM_HOST_IMPORT_MATH_ACOS: &str = "math_acos";
const WASM_HOST_IMPORT_MATH_ACOSH: &str = "math_acosh";
const WASM_HOST_IMPORT_MATH_ASIN: &str = "math_asin";
const WASM_HOST_IMPORT_MATH_ASINH: &str = "math_asinh";
const WASM_HOST_IMPORT_MATH_ATAN: &str = "math_atan";
const WASM_HOST_IMPORT_MATH_ATANH: &str = "math_atanh";
const WASM_HOST_IMPORT_MATH_CBRT: &str = "math_cbrt";
const WASM_HOST_IMPORT_MATH_COS: &str = "math_cos";
const WASM_HOST_IMPORT_MATH_COSH: &str = "math_cosh";
const WASM_HOST_IMPORT_MATH_EXP: &str = "math_exp";
const WASM_HOST_IMPORT_MATH_EXPM1: &str = "math_expm1";
const WASM_HOST_IMPORT_MATH_LOG: &str = "math_log";
const WASM_HOST_IMPORT_MATH_LOG10: &str = "math_log10";
const WASM_HOST_IMPORT_MATH_LOG1P: &str = "math_log1p";
const WASM_HOST_IMPORT_MATH_LOG2: &str = "math_log2";
const WASM_HOST_IMPORT_MATH_SIN: &str = "math_sin";
const WASM_HOST_IMPORT_MATH_SINH: &str = "math_sinh";
const WASM_HOST_IMPORT_MATH_TAN: &str = "math_tan";
const WASM_HOST_IMPORT_MATH_TANH: &str = "math_tanh";
const WASM_HOST_IMPORT_MATH_ATAN2: &str = "math_atan2";
const WASM_HOST_IMPORT_WALL_CLOCK_MILLIS: &str = "wall_clock_millis";
const WASM_HOST_IMPORT_MONOTONIC_CLOCK_NANOS: &str = "monotonic_clock_nanos";
const WASM_HOST_IMPORT_SLEEP_NANOS: &str = "sleep_nanos";
const WASM_HOST_IMPORT_REJECT_RUNTIME_SEMANTICS: &str = "reject_runtime_semantics";
/// Cut over before the multi-megabyte function bodies seen in slow Test262
/// artifacts can exhaust Cranelift's fast-compilation per-function limits.
/// This is a performance heuristic only; the normal compiler retains its
/// authoritative `CodeTooLarge` fallback below the cutoff.
const SIZE_OPTIMIZED_WASM_MIN_CODE_BODY_BYTES: usize = 1024 * 1024;
/// Cranelift's `CodegenError::CodeTooLarge` display text, verbatim
/// (`cranelift-codegen/src/result.rs`). Both the size-optimized retry and the
/// per-function attribution key on it, and a single constant keeps the two from
/// drifting apart.
const WASM_CODE_TOO_LARGE_MESSAGE: &str = "Code for function is too large";

fn number_is_odd_integer(value: f64) -> bool {
    value.is_finite()
        && value.trunc() == value
        && value.abs() <= 9_007_199_254_740_991.0
        && (value.abs() as u64) & 1 == 1
}

fn wasm_number_pow(base: f64, exponent: f64) -> f64 {
    if exponent.is_nan() {
        return f64::NAN;
    }
    if exponent == 0.0 {
        return 1.0;
    }
    if base.is_nan() {
        return f64::NAN;
    }
    if exponent.is_infinite() {
        return match base.abs().partial_cmp(&1.0) {
            Some(std::cmp::Ordering::Greater) if exponent.is_sign_positive() => f64::INFINITY,
            Some(std::cmp::Ordering::Greater) => 0.0,
            Some(std::cmp::Ordering::Less) if exponent.is_sign_positive() => 0.0,
            Some(std::cmp::Ordering::Less) => f64::INFINITY,
            _ => f64::NAN,
        };
    }
    if base == f64::INFINITY {
        return if exponent.is_sign_positive() {
            f64::INFINITY
        } else {
            0.0
        };
    }
    if base == f64::NEG_INFINITY {
        if exponent.is_sign_positive() {
            return if number_is_odd_integer(exponent) {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
        }
        return if number_is_odd_integer(exponent) {
            -0.0
        } else {
            0.0
        };
    }
    if base == 0.0 {
        let negative_zero = base.to_bits() == (-0.0f64).to_bits();
        if exponent.is_sign_positive() {
            return if negative_zero && number_is_odd_integer(exponent) {
                -0.0
            } else {
                0.0
            };
        }
        return if negative_zero && number_is_odd_integer(exponent) {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    if base < 0.0 && exponent.trunc() != exponent {
        return f64::NAN;
    }
    base.powf(exponent)
}

/// Host implementations of the Math transcendental imports. Each delegates
/// to the Rust `f64` method, whose IEEE-754 edge behavior (NaN propagation,
/// signed zeros, infinities, out-of-domain NaN) matches every ECMA-262 Math
/// special-case row. Atanh uses a stable log1p reduction to avoid cancellation
/// near the endpoints. The Wasm call sites additionally retain their exact
/// special-value guards.
fn wasm_math_acos(value: f64) -> f64 {
    value.acos()
}

fn wasm_math_acosh(value: f64) -> f64 {
    value.acosh()
}

fn wasm_math_asin(value: f64) -> f64 {
    value.asin()
}

fn wasm_math_asinh(value: f64) -> f64 {
    value.asinh()
}

fn wasm_math_atan(value: f64) -> f64 {
    value.atan()
}

fn wasm_math_atanh(value: f64) -> f64 {
    let magnitude = value.abs();
    // Below 2^-28, the cubic correction is smaller than half an ulp.
    // Returning the input also preserves signed zero and subnormal values.
    if magnitude < 2.0f64.powi(-28) {
        return value;
    }
    let twice = 2.0 * magnitude;
    let result = if magnitude < 0.5 {
        0.5 * (twice + twice * magnitude / (1.0 - magnitude)).ln_1p()
    } else {
        0.5 * (twice / (1.0 - magnitude)).ln_1p()
    };
    result.copysign(value)
}

fn wasm_math_cbrt(value: f64) -> f64 {
    value.cbrt()
}

fn wasm_math_cos(value: f64) -> f64 {
    value.cos()
}

fn wasm_math_cosh(value: f64) -> f64 {
    value.cosh()
}

fn wasm_math_exp(value: f64) -> f64 {
    value.exp()
}

fn wasm_math_expm1(value: f64) -> f64 {
    value.exp_m1()
}

fn wasm_math_log(value: f64) -> f64 {
    value.ln()
}

fn wasm_math_log10(value: f64) -> f64 {
    value.log10()
}

fn wasm_math_log1p(value: f64) -> f64 {
    value.ln_1p()
}

fn wasm_math_log2(value: f64) -> f64 {
    value.log2()
}

fn wasm_math_sin(value: f64) -> f64 {
    value.sin()
}

fn wasm_math_sinh(value: f64) -> f64 {
    value.sinh()
}

fn wasm_math_tan(value: f64) -> f64 {
    value.tan()
}

fn wasm_math_tanh(value: f64) -> f64 {
    value.tanh()
}

fn wasm_math_atan2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}

/// `(import name, implementation)` table for the nineteen unary Math
/// transcendental imports. Both linker sites iterate this one table so a new
/// builtin cannot be registered in one and forgotten in the other; the binary
/// `math_atan2` is registered alongside it at each site.
const WASM_MATH_UNARY_IMPORTS: [(&str, fn(f64) -> f64); 19] = [
    (WASM_HOST_IMPORT_MATH_ACOS, wasm_math_acos),
    (WASM_HOST_IMPORT_MATH_ACOSH, wasm_math_acosh),
    (WASM_HOST_IMPORT_MATH_ASIN, wasm_math_asin),
    (WASM_HOST_IMPORT_MATH_ASINH, wasm_math_asinh),
    (WASM_HOST_IMPORT_MATH_ATAN, wasm_math_atan),
    (WASM_HOST_IMPORT_MATH_ATANH, wasm_math_atanh),
    (WASM_HOST_IMPORT_MATH_CBRT, wasm_math_cbrt),
    (WASM_HOST_IMPORT_MATH_COS, wasm_math_cos),
    (WASM_HOST_IMPORT_MATH_COSH, wasm_math_cosh),
    (WASM_HOST_IMPORT_MATH_EXP, wasm_math_exp),
    (WASM_HOST_IMPORT_MATH_EXPM1, wasm_math_expm1),
    (WASM_HOST_IMPORT_MATH_LOG, wasm_math_log),
    (WASM_HOST_IMPORT_MATH_LOG10, wasm_math_log10),
    (WASM_HOST_IMPORT_MATH_LOG1P, wasm_math_log1p),
    (WASM_HOST_IMPORT_MATH_LOG2, wasm_math_log2),
    (WASM_HOST_IMPORT_MATH_SIN, wasm_math_sin),
    (WASM_HOST_IMPORT_MATH_SINH, wasm_math_sinh),
    (WASM_HOST_IMPORT_MATH_TAN, wasm_math_tan),
    (WASM_HOST_IMPORT_MATH_TANH, wasm_math_tanh),
];
thread_local! {
    static ENGINE_WORKER_STACK_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Admission is scoped to an owned worker or the documented current-thread API.
/// Restoring the previous state also handles nested entry and unwinding.
struct EngineWorkerStackGuard {
    previous: bool,
    thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl EngineWorkerStackGuard {
    fn enter() -> Self {
        Self {
            previous: ENGINE_WORKER_STACK_ACTIVE.replace(true),
            thread: std::marker::PhantomData,
        }
    }
}

impl Drop for EngineWorkerStackGuard {
    fn drop(&mut self) {
        ENGINE_WORKER_STACK_ACTIVE.set(self.previous);
    }
}

/// Runs `f` on a dedicated worker thread with `ENGINE_WORKER_STACK_SIZE`
/// bytes of stack, then joins and returns its result.
///
/// Uses `thread::scope` so `f` may borrow non-`'static` data (e.g. `&str`
/// source text, `&CompilationUnit`): the scope guarantees the worker
/// finishes before this function returns.
fn run_on_sized_stack<T, F>(f: F) -> T
where
    T: Send,
    F: FnOnce() -> T + Send,
{
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(ENGINE_WORKER_STACK_SIZE)
            .spawn_scoped(scope, move || {
                let _stack = EngineWorkerStackGuard::enter();
                f()
            })
            .expect("scriptw-engine worker thread should spawn")
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

/// Scriptw-owned Wasmtime whole-module cache. It is deliberately separate from
/// Wasmtime's global default cache so Scriptw can enforce its 1GiB/70% bounds
/// without changing another Wasmtime application's artifacts.
fn wasmtime_module_cache() -> Option<wasmtime::Cache> {
    static CACHE: OnceLock<Option<wasmtime::Cache>> = OnceLock::new();
    CACHE
        .get_or_init(|| match cache::module_cache() {
            Ok(cache) => Some(cache),
            Err(err) => {
                if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                    eprintln!(
                        "scriptw wasm trace: module cache unavailable, running \
                         uncached: {err}"
                    );
                }
                None
            }
        })
        .clone()
}

fn cranelift_function_cache() -> Option<Arc<cache::NativeFunctionCache>> {
    static CACHE: OnceLock<Option<Arc<cache::NativeFunctionCache>>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            match cache::FunctionCache::new(
                cache::function_cache_directory(),
                cache::function_cache_limit_bytes(),
            )
            .and_then(|cache| {
                cache::NativeFunctionCache::new(
                    Arc::new(cache),
                    env!("SCRIPTW_NATIVE_COMPILER_FINGERPRINT"),
                )
            }) {
                Ok(cache) => Some(Arc::new(cache)),
                Err(err) => {
                    if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                        eprintln!(
                            "scriptw wasm trace: function cache unavailable, running uncached: \
                             {err}"
                        );
                    }
                    None
                }
            }
        })
        .clone()
}

fn program_wasm_cache() -> Option<Arc<cache::FunctionCache>> {
    static CACHE: OnceLock<Option<Arc<cache::FunctionCache>>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            match cache::FunctionCache::new(
                cache::program_cache_directory(),
                cache::program_cache_limit_bytes(),
            ) {
                Ok(cache) => Some(Arc::new(cache)),
                Err(err) => {
                    if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                        eprintln!(
                            "scriptw wasm trace: program Wasm cache unavailable, running \
                             uncached: {err}"
                        );
                    }
                    None
                }
            }
        })
        .clone()
}

#[derive(Clone)]
struct ProgramWasmCacheEntry {
    cache: Arc<cache::FunctionCache>,
    key: [u8; 32],
}

#[derive(Clone)]
struct ProgramWasmArtifact {
    /// The program module; standalone when `runtime` is `None`.
    bytes: Arc<[u8]>,
    runtime: Option<Arc<scriptw_aot_wasm::RuntimeArtifact>>,
    cached_entry: Option<ProgramWasmCacheEntry>,
    compilation_policy: WasmCompilationPolicy,
}

impl ProgramWasmArtifact {
    fn program(&self) -> WasmProgramRef<'_> {
        WasmProgramRef::new(&self.bytes, self.runtime.as_ref())
            .with_compilation_policy(&self.compilation_policy)
    }

    fn evict_if_invalid(&self, error: &EngineError) -> bool {
        let invalid_artifact = error
            .message()
            .starts_with("wasmtime module validation failed:")
            || error.message().starts_with("failed to inspect Wasm Intl ")
            || error.intl_artifact_identity_error().is_some();
        if !invalid_artifact {
            return false;
        }
        let Some(entry) = &self.cached_entry else {
            return false;
        };
        entry.cache.remove(&entry.key);
        true
    }
}

fn compiler_identity_cache_fingerprint(identity: &CompilerIdentity) -> [u8; 32] {
    compiler_cache_identity::cache_fingerprint(
        CompilerCacheImageHashAlgorithm::Sha256,
        identity.source_fingerprint(),
        identity.executable_sha256().as_bytes(),
    )
}

fn compiler_fingerprint() -> Option<&'static [u8; 32]> {
    // The selected algorithm admits actual loaded-image bytes. Full execution
    // evidence remains independently SHA-256; errors disable compiler caches.
    compiler_cache_identity()
        .ok()
        .map(CompilerCacheIdentity::fingerprint)
}

const PROGRAM_WASM_CACHE_KEY_DOMAIN: &[u8] = b"scriptw-program-wasm-cache-key-v20";

fn hash_program_cache_field(hash: &mut Sha256, bytes: &[u8]) {
    let length = u64::try_from(bytes.len()).expect("cache-key field length must fit in u64");
    hash.update(length.to_le_bytes());
    hash.update(bytes);
}

fn hash_optional_program_cache_field(hash: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hash.update([1]);
            hash_program_cache_field(hash, value.as_bytes());
        }
        None => hash.update([0]),
    }
}

fn hash_optional_program_cache_locales(
    hash: &mut Sha256,
    locales: Option<&[scriptw_intl::LocaleId]>,
) {
    match locales {
        Some(locales) => {
            hash.update([1]);
            let count = u64::try_from(locales.len()).expect("cache locale count must fit in u64");
            hash.update(count.to_le_bytes());
            for locale in locales {
                hash_program_cache_field(hash, locale.as_str().as_bytes());
            }
        }
        None => hash.update([0]),
    }
}

/// Hashes one injectively framed program-compilation tuple.
///
/// Fixed-width enum discriminators and presence-tagged, length-prefixed byte
/// fields prevent a suffix of one field from being reinterpreted as the next.
/// The domain changes when this framing grammar changes.
fn program_wasm_cache_key_with_compiler_fingerprint(
    source: &str,
    goal: ParseGoal,
    options: &CompileOptions,
    graph_digest: Option<&[u8; 32]>,
    compiler_fingerprint: &[u8; 32],
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(PROGRAM_WASM_CACHE_KEY_DOMAIN);
    if !options.foreign_modules.is_empty() {
        hash_program_cache_field(&mut hash, b"explicit-foreign-ABI-v1");
    }
    hash.update(compiler_fingerprint);
    hash_program_cache_field(&mut hash, std::env::consts::ARCH.as_bytes());
    hash.update([match goal {
        ParseGoal::Script => 0,
        ParseGoal::Module => 1,
    }]);
    hash.update([u8::from(options.optimize)]);
    hash.update([match options.host_surface_policy {
        HostSurfacePolicy::Product => 0,
        HostSurfacePolicy::Test262 => 1,
    }]);
    hash.update(options.language_features.wire_code().to_le_bytes());
    hash.update([match options.promise_rejection_policy {
        PromiseRejectionPolicy::FailRun => 0,
        PromiseRejectionPolicy::Ignore => 1,
    }]);
    match &options.intl_profile {
        IntlCompilationProfile::Minimal => hash.update([0]),
        IntlCompilationProfile::Conformance => hash.update([3]),
        IntlCompilationProfile::Custom(id) => {
            hash.update([1]);
            hash_program_cache_field(&mut hash, id.as_str().as_bytes());
        }
        IntlCompilationProfile::CustomProjection(selection) => {
            hash.update([2]);
            hash_program_cache_field(&mut hash, selection.id().as_str().as_bytes());
            hash_optional_program_cache_locales(
                &mut hash,
                selection
                    .list_projection()
                    .map(CustomListProfile::requested_locales),
            );
            hash_optional_program_cache_locales(&mut hash, selection.relative_time_locales());
            hash_optional_program_cache_locales(&mut hash, selection.display_names_locales());
            hash_optional_program_cache_locales(&mut hash, selection.duration_locales());
            hash_optional_program_cache_locales(&mut hash, selection.number_locales());
            hash_optional_program_cache_locales(&mut hash, selection.date_time_locales());
            hash_optional_program_cache_locales(&mut hash, selection.collator_locales());
            hash_optional_program_cache_locales(&mut hash, selection.segmenter_locales());
            match selection.currency_codes() {
                None => hash.update([0]),
                Some(codes) => {
                    hash.update([1]);
                    hash.update(
                        u64::try_from(codes.len())
                            .expect("checked currency count fits u64")
                            .to_le_bytes(),
                    );
                    for code in codes {
                        hash_program_cache_field(&mut hash, &code.clone().ascii());
                    }
                }
            }
            match selection.date_time_calendars() {
                None => hash.update([0]),
                Some(calendars) => {
                    hash.update([1]);
                    hash.update(
                        u64::try_from(calendars.len())
                            .expect("checked calendar count fits u64")
                            .to_le_bytes(),
                    );
                    for calendar in calendars {
                        hash_program_cache_field(&mut hash, calendar.as_str().as_bytes());
                    }
                }
            }
            match selection.numbering_systems() {
                None => hash.update([0]),
                Some(systems) => {
                    hash.update([1]);
                    hash.update(
                        u64::try_from(systems.len())
                            .expect("checked numbering-system count fits u64")
                            .to_le_bytes(),
                    );
                    for system in systems {
                        hash_program_cache_field(&mut hash, system.name().as_bytes());
                    }
                }
            }
            match selection.named_time_zones() {
                None => hash.update([0]),
                Some(zones) => {
                    hash.update([1]);
                    hash.update(
                        u64::try_from(zones.len())
                            .expect("checked named-zone count fits u64")
                            .to_le_bytes(),
                    );
                    for zone in zones {
                        hash_program_cache_field(&mut hash, zone.as_str().as_bytes());
                    }
                }
            }
            match selection.service_selection() {
                None => hash.update([0]),
                Some(services) => {
                    hash.update([1]);
                    hash.update(services.wire().to_le_bytes());
                }
            }
        }
    }
    hash_optional_program_cache_field(&mut hash, options.filename.as_deref());
    hash_optional_program_cache_field(&mut hash, options.module_root.as_deref());
    hash_optional_program_cache_field(&mut hash, options.module_prelude.as_deref());
    match &options.module_loading_policy {
        ModuleLoadingPolicy::Filesystem => hash.update([0]),
        ModuleLoadingPolicy::RejectAll => hash.update([1]),
        ModuleLoadingPolicy::Embedded(graph) => {
            hash.update([2]);
            hash.update(graph.fingerprint());
        }
    }
    if !options.foreign_modules.is_empty() {
        // Declarations authorize a compiler extension, never ambient discovery.
        // Preserve the existing empty-registry key and separate every complete
        // normalized ABI schema (including the expected-error policy).
        hash_program_cache_field(&mut hash, b"scriptw-foreign-declarations-v1");
        hash_program_cache_field(&mut hash, &options.foreign_modules.to_normalized_json());
    }
    hash_program_cache_field(&mut hash, source.as_bytes());
    match graph_digest {
        Some(digest) => {
            hash.update([1]);
            hash.update(digest);
        }
        None => hash.update([0]),
    }
    hash.finalize().into()
}

/// Digest of every module in a graph *other than* the entry, in graph order.
///
/// Without this a stale artifact survives an edit to an imported file: the
/// entry's own text is unchanged, so the key would be unchanged too.
fn module_graph_digest(sources: &scriptw_ir::ModuleGraphSources) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"scriptw-loaded-module-records-v3");
    hash.update(sources.modules.len().to_le_bytes());
    hash.update(sources.entry.to_le_bytes());
    for (index, module) in sources.modules.iter().enumerate() {
        if index == sources.entry as usize {
            continue;
        }
        hash.update([module.kind().code()]);
        hash.update(module.key().as_str().len().to_le_bytes());
        hash.update(module.key().as_str().as_bytes());
        hash.update(module.source_text().len().to_le_bytes());
        hash.update(module.source_text().as_bytes());
        hash.update(module.meta_url().len().to_le_bytes());
        hash.update(module.meta_url().as_bytes());
        if let Some(foreign) = module.foreign_modules() {
            hash_program_cache_field(&mut hash, b"foreign-module-record-v1");
            hash_program_cache_field(&mut hash, &foreign.to_normalized_json());
        }
    }
    hash.update(sources.resolutions.len().to_le_bytes());
    for (referrer, request, target) in &sources.resolutions {
        hash.update(referrer.to_le_bytes());
        hash.update(target.to_le_bytes());
        hash.update(request.specifier().len().to_le_bytes());
        hash.update(request.specifier().as_bytes());
        hash.update(request.attributes().len().to_le_bytes());
        for attribute in request.attributes() {
            hash.update(attribute.key.len().to_le_bytes());
            hash.update(attribute.key.as_bytes());
            hash.update(attribute.value.len().to_le_bytes());
            hash.update(attribute.value.as_bytes());
        }
    }
    hash.update(sources.realm_requests.len().to_le_bytes());
    for (request, resolution) in &sources.realm_requests {
        hash.update(request.specifier().len().to_le_bytes());
        hash.update(request.specifier().as_bytes());
        hash.update(request.attributes().len().to_le_bytes());
        for attribute in request.attributes() {
            hash.update(attribute.key.len().to_le_bytes());
            hash.update(attribute.key.as_bytes());
            hash.update(attribute.value.len().to_le_bytes());
            hash.update(attribute.value.as_bytes());
        }
        match resolution {
            scriptw_ir::RealmModuleResolutionIr::Loaded(target) => {
                hash.update([0]);
                hash.update(target.to_le_bytes());
            }
            scriptw_ir::RealmModuleResolutionIr::Rejected(message) => {
                hash.update([1]);
                hash.update(message.len().to_le_bytes());
                hash.update(message.as_bytes());
            }
        }
    }
    hash.finalize().into()
}

fn program_wasm_cache_key(
    source: &str,
    goal: ParseGoal,
    options: &CompileOptions,
) -> Option<[u8; 32]> {
    Some(program_wasm_cache_key_with_compiler_fingerprint(
        source,
        goal,
        options,
        None,
        compiler_fingerprint()?,
    ))
}

#[derive(Clone, Copy)]
enum AmbientModuleLoaderPolicy {
    Filesystem,
    RejectAll,
}

fn configured_module_loader(
    options: &CompileOptions,
    policy: AmbientModuleLoaderPolicy,
) -> Result<Box<dyn HostModuleLoader + 'static>, EngineError> {
    let fallback: Box<dyn HostModuleLoader> = match policy {
        AmbientModuleLoaderPolicy::Filesystem => Box::new(
            FilesystemModuleLoader::new(
                options.module_root.as_deref(),
                options.filename.as_deref(),
            )
            .map_err(|error| EngineError::new(error.to_string()))?,
        ),
        AmbientModuleLoaderPolicy::RejectAll => Box::new(module_loader::RejectAllModuleLoader),
    };
    Ok(if options.foreign_modules.is_empty() {
        fallback
    } else {
        Box::new(module_loader::ForeignModuleLoader::new(
            options.foreign_modules.clone(),
            fallback,
        ))
    })
}

/// Loads the module graph rooted at `source`, using the loader policy selected
/// by `options`.
///
/// `source` is the entry text as the caller supplied it, which may differ from
/// what is on disk (test262 prepends harness text). Resolution still keys off
/// `options.filename`, so relative specifiers inside the entry resolve against
/// the real directory.
fn module_entry_graph(
    source: &scriptw_front::ParsedModule,
    options: &CompileOptions,
    loader: &dyn HostModuleLoader,
) -> Option<scriptw_ir::ModuleGraphSources> {
    let entry_locator = options.filename.as_deref().unwrap_or("<entry>");
    module_loader::load_module_graph_from_parsed(entry_locator, source.clone(), loader).ok()
}

/// The module graph a Script's `import()` calls reach, or `None` when the Script
/// writes none.
///
/// A Script is not module code, but `import()` is legal in it (13.3.10 takes
/// `GetActiveScriptOrModule`, which a Script satisfies) and names a module, so
/// statically discoverable targets are compiled into the artifact exactly as
/// a module's `import()` targets are. A computed-only call still needs the
/// one-node graph's dispatcher: operand evaluation can throw, coercion can
/// reject, and a name outside the compiled registry rejects its Promise.
/// Discovery walks the retained Script AST without changing the parse goal,
/// so sloppy Script forms remain valid.
///
/// The retained AST decides whether a Script needs a graph, including computed
/// imports. Property and method names never become import requests, and this
/// decision does not repeat the parser's lexical analysis.
fn script_entry_graph(
    source: &scriptw_front::ParsedScript,
    options: &CompileOptions,
    loader: &dyn HostModuleLoader,
) -> Option<scriptw_ir::ModuleGraphSources> {
    if !script_has_dynamic_import(source)
        && scriptw_ir::scan_script_realm_module_requests(source).is_empty()
    {
        return None;
    }
    let entry_locator = options.filename.as_deref().unwrap_or("<entry>");
    let graph =
        module_loader::load_module_graph_from_parsed_script(entry_locator, source.clone(), loader)
            .ok()?;
    let entry = graph.modules.get(graph.entry as usize)?;
    if !entry.script_has_dynamic_import_sites()? {
        return None;
    }
    Some(graph)
}

fn parse_ambient_entry(
    source_text: &str,
    goal: ParseGoal,
    options: &CompileOptions,
    policy: AmbientModuleLoaderPolicy,
) -> Result<(ParsedSource, PreparedModuleCompilation), EngineError> {
    let parse_started = std::time::Instant::now();
    let source = parse(
        source_text,
        ParseOptions {
            goal,
            filename: options.filename.clone(),
        },
    )
    .map_err(EngineError::from_parse_error)?;
    if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
        eprintln!("scriptw wasm trace: parse: {:?}", parse_started.elapsed());
    }
    let loader = configured_module_loader(options, policy)?;
    let graph = match &source {
        ParsedSource::Script(source) => script_entry_graph(source, options, loader.as_ref()),
        ParsedSource::Module(source) => module_entry_graph(source, options, loader.as_ref()),
    };
    Ok((
        source,
        PreparedModuleCompilation::Ambient {
            policy,
            loader,
            graph,
        },
    ))
}

/// Cache key for a program, covering the whole module graph when there is one.
///
/// A module artifact embeds every module of its graph, so hashing the entry
/// text alone would leave a stale artifact live after an imported file is
/// edited. A Script that writes `import()` embeds its targets the same way, so
/// it is hashed the same way.
fn program_cache_key(
    source: &str,
    goal: ParseGoal,
    options: &CompileOptions,
    graph: Option<&scriptw_ir::ModuleGraphSources>,
) -> Option<[u8; 32]> {
    let digest = graph.map(module_graph_digest);
    Some(program_wasm_cache_key_with_compiler_fingerprint(
        source,
        goal,
        options,
        digest.as_ref(),
        compiler_fingerprint()?,
    ))
}

fn program_cache_key_for_role(
    source: &str,
    goal: ParseGoal,
    options: &CompileOptions,
    graph: Option<&scriptw_ir::ModuleGraphSources>,
    role: module_loader::EmbeddedSourceRole,
) -> Option<[u8; 32]> {
    let key = program_cache_key(source, goal, options, graph)?;
    Some(match (&options.module_loading_policy, role) {
        (ModuleLoadingPolicy::Embedded(_), module_loader::EmbeddedSourceRole::UnlocatedScript) => {
            let mut hash = Sha256::new();
            hash.update(b"scriptw-embedded-unlocated-program-v1");
            hash.update(key);
            hash.finalize().into()
        }
        (
            ModuleLoadingPolicy::Filesystem | ModuleLoadingPolicy::RejectAll,
            module_loader::EmbeddedSourceRole::Entry
            | module_loader::EmbeddedSourceRole::UnlocatedScript,
        )
        | (ModuleLoadingPolicy::Embedded(_), module_loader::EmbeddedSourceRole::Entry) => key,
    })
}

/// One parse/graph-discovery result consumed by one lowering attempt.
///
/// The cache key and the lowerer borrow this same graph. Previously each built
/// its own graph, which parsed every module twice on a cache miss even after the
/// front-end/lowering boundary itself became parse-once.
enum PreparedModuleCompilation {
    Ambient {
        policy: AmbientModuleLoaderPolicy,
        // A successful graph-free artifact has completed finite-source
        // discovery with an actual loader. No absent-loader success can
        // silently cache an unresolved filesystem dependency.
        loader: Box<dyn HostModuleLoader>,
        graph: Option<scriptw_ir::ModuleGraphSources>,
    },
    Embedded {
        owner: Arc<EmbeddedModuleGraph>,
        graph: scriptw_ir::ModuleGraphSources,
    },
}

impl PreparedModuleCompilation {
    fn graph(&self) -> Option<&scriptw_ir::ModuleGraphSources> {
        match self {
            Self::Ambient { graph, .. } => graph.as_ref(),
            Self::Embedded { graph, .. } => Some(graph),
        }
    }
}

/// Retained syntax and admitted module authority before lowering. A cache miss
/// consumes this owner once; cached programs retain no analysis or IR facts.
struct ParsedCompilation {
    source: ParsedSource,
    module_prelude: Option<scriptw_front::ParsedScript>,
    modules: PreparedModuleCompilation,
}

#[cfg(test)]
thread_local! {
    static LOWERING_ATTEMPTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

struct PreparedCompilation {
    source: ParsedSource,
    module_prelude: Option<scriptw_front::ParsedScript>,
    modules: PreparedModuleCompilation,
    ir: ProgramIr,
    promise_rejection_policy: PromiseRejectionPolicy,
    intl_profile: IntlCompilationProfile,
    host_surface_policy: HostSurfacePolicy,
}

fn wasm_aot_program_is_cached(source: &str, goal: ParseGoal, options: &CompileOptions) -> bool {
    // This is only a scheduling hint. The execution path still reads and
    // validates the artifact, then rebuilds it if it disappeared or is corrupt.
    let Some(key) = program_wasm_cache_key(source, goal, options) else {
        return false;
    };
    program_wasm_cache().is_some_and(|cache| cache.contains(&key))
}

#[doc(hidden)]
pub fn wasm_aot_script_is_cached(source: &str, options: &CompileOptions) -> bool {
    wasm_aot_program_is_cached(source, ParseGoal::Script, options)
}

#[doc(hidden)]
pub fn wasm_aot_module_is_cached(source: &str, options: &CompileOptions) -> bool {
    wasm_aot_program_is_cached(source, ParseGoal::Module, options)
}

static COMPILATION_JOBS: OnceLock<usize> = OnceLock::new();
static EXPLICIT_COMPILATION_JOBS: OnceLock<usize> = OnceLock::new();
static COMPILATION_POOL: OnceLock<Result<rayon::ThreadPool, String>> = OnceLock::new();

/// Default Cranelift parallelism: half the logical CPUs, with a minimum of one.
/// Test262's `--threads` remains a separate case-execution setting.
pub fn default_compilation_jobs() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(2)
        .div_ceil(2)
        .max(1)
}

/// Selects the shared Cranelift compilation-pool size before the first Wasm
/// module is compiled. A process has one pool so concurrent Test262 workers
/// cannot each create an unbounded set of compiler threads.
pub fn configure_compilation_jobs(jobs: usize) -> Result<(), String> {
    if jobs == 0 {
        return Err("--jobs must be a positive integer".to_string());
    }
    let configured = COMPILATION_JOBS.get_or_init(|| jobs);
    if *configured == jobs {
        let _ = EXPLICIT_COMPILATION_JOBS.set(jobs);
        Ok(())
    } else {
        Err(format!(
            "Wasm compilation pool already configured for {configured} jobs"
        ))
    }
}

pub fn compilation_jobs() -> usize {
    *COMPILATION_JOBS.get_or_init(default_compilation_jobs)
}

/// Returns explicit configuration without choosing the CPU-derived default.
/// Isolated developer workers can therefore preserve their serial default.
#[doc(hidden)]
pub fn configured_compilation_jobs() -> Option<usize> {
    EXPLICIT_COMPILATION_JOBS.get().copied()
}

fn compilation_pool() -> Result<&'static rayon::ThreadPool, EngineError> {
    let result = COMPILATION_POOL.get_or_init(|| {
        let jobs = compilation_jobs();
        rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .thread_name(|index| format!("scriptw-cranelift-{index}"))
            .build()
            .map_err(|err| format!("failed to build {jobs}-thread compilation pool: {err}"))
    });
    result.as_ref().map_err(|err| EngineError::new(err.clone()))
}

fn on_native_compilation_thread<T: Send>(
    jobs: usize,
    compile: impl FnOnce() -> T + Send,
) -> Result<T, EngineError> {
    if jobs == 1 && ENGINE_WORKER_STACK_ACTIVE.get() {
        // The caller already owns the full compiler stack. Avoid creating an
        // idle Rayon worker whose backoff competes with emission on one CPU.
        Ok(compile())
    } else {
        // Unmarked callers may have a small stack; parallel compilation also
        // retains its process-wide, bounded Rayon registry.
        Ok(compilation_pool()?.install(compile))
    }
}

#[derive(Clone, Copy)]
enum WasmModuleMemoryCachePolicy {
    Retain,
    /// Retain immutable R modules, while unique per-case P modules bypass the LRU.
    RetainRuntime,
    BypassRetention,
}

#[derive(Clone, Copy)]
enum WasmModuleMemoryCacheOutcome {
    Hit,
    Miss,
    Bypassed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WasmNativeCompilationPlan {
    mode: WasmNativeCompilationMode,
    largest_code_body_bytes: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WasmModuleMemoryCacheKey {
    native_compilation_mode: WasmNativeCompilationMode,
    runtime_image_policy: WasmRuntimeImagePolicy,
    wasm_sha256: [u8; 32],
}

/// Image storage is an explicit experimental variable. Ordinary execution
/// admits its Cargo-produced image; all forced policy cohorts use the same
/// matching producer/retention path, including the 16 MiB control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WasmRuntimeImagePolicy {
    EmbeddedBuild,
    MatchedDiagnostic,
}

fn memory_wasm_modules() -> &'static Mutex<MemoryWasmModuleCache> {
    static MODULES: OnceLock<Mutex<MemoryWasmModuleCache>> = OnceLock::new();
    MODULES.get_or_init(|| Mutex::new(MemoryWasmModuleCache::from_env()))
}

fn plan_wasm_native_compilation(bytes: &[u8]) -> WasmNativeCompilationPlan {
    let largest_code_body_bytes = largest_wasm_code_body_size(bytes);
    let mode = if largest_code_body_bytes
        .is_some_and(|size| size >= SIZE_OPTIMIZED_WASM_MIN_CODE_BODY_BYTES)
    {
        WasmNativeCompilationMode::SizeOptimized
    } else {
        WasmNativeCompilationMode::Fast
    };
    WasmNativeCompilationPlan {
        mode,
        largest_code_body_bytes,
    }
}

/// One code-section body, as the attribution below ranks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WasmCodeBodyFacts {
    /// Index in the Wasm function index space, i.e. after the imports.
    function_index: u32,
    body_bytes: usize,
    /// `None` when the local declaration could not be decoded, which is
    /// reported as `?` rather than as a plausible number.
    declared_locals: Option<u64>,
}

impl core::fmt::Display for WasmCodeBodyFacts {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "wasm[0]::function[{}] ({} bytes, ",
            self.function_index, self.body_bytes
        )?;
        match self.declared_locals {
            Some(locals) => write!(f, "{locals} locals)"),
            None => write!(f, "? locals)"),
        }
    }
}

/// Total declared locals of one body, or `None` when the declaration does not
/// decode.
fn wasm_body_declared_locals(body: &wasmparser::FunctionBody<'_>) -> Option<u64> {
    let mut reader = body.get_locals_reader().ok()?;
    let mut total: u64 = 0;
    for _ in 0..reader.get_count() {
        let (count, _value_type) = reader.read().ok()?;
        total = total.checked_add(u64::from(count))?;
    }
    Some(total)
}

/// Attributes a `Code for function is too large` failure to the emitted
/// function bodies most likely to be responsible, **by Wasm function index**.
///
/// Cranelift's `CodegenError::CodeTooLarge` — raised when `VRegAllocator::alloc`
/// exhausts the 2,097,151 virtual registers `VReg::MAX_BITS = 21` allows — says
/// nothing about *which* function ran out. The module carries two candidate
/// answers and this reports both, because they are frequently different
/// functions:
///
/// * the largest code-section body, and
/// * the body declaring the most locals.
///
/// The second is the better predictor: the Cranelift Wasm frontend materialises
/// a value per live local at every control-flow join, so virtual-register
/// pressure tracks `locals × blocks` rather than encoded size. Reporting only
/// the byte-largest body names a shared builtin that is byte-identical in
/// hundreds of modules Cranelift compiles fine, which is a plausible-looking
/// wrong answer. Both are hints, and they are labelled as the two rankings they
/// are.
///
/// **Deliberately index-only: no symbol names appear here.** This string is
/// appended to the failure detail that `scriptw-test262` classifies with, and
/// both `classify_failure_origin` and `classify_failure_outcome` match
/// case-insensitive substrings over the *whole* detail. A symbol such as
/// `builtin::Intl.DateTimeFormat.prototype.format` would match `intl` and file
/// a Cranelift backend defect under `FailureOrigin::IcuIntl`; `builtin::JSON.parse`
/// would match `parse` and give `BoaParser`; `builtin_stub::…` would match
/// `stub` and demote the outcome from `Bug` to `NotImplemented`. The
/// index-to-symbol mapping lives in the module's `name` section — which is what
/// wasmtime itself symbolises `wasm[0]::function[N]::<name>` from — and in the
/// backend `debug_dump` size report (`SCRIPTW_EMIT_SIZE_REPORT`).
/// `attribution_is_inert_for_the_test262_failure_classifiers` guards this.
///
/// Returns `None` for a module that fails to parse or carries no code section —
/// in that case the caller reports the original error unchanged.
fn describe_largest_wasm_code_body(bytes: &[u8]) -> Option<String> {
    let mut imported_function_count: u32 = 0;
    let mut code_entry_ordinal: u32 = 0;
    let mut largest: Option<WasmCodeBodyFacts> = None;
    let mut most_locals: Option<WasmCodeBodyFacts> = None;

    for payload in WasmParser::new(0).parse_all(bytes) {
        match payload.ok()? {
            WasmPayload::ImportSection(reader) => {
                // `into_imports` rather than iterating the section directly:
                // one `Imports` entry can carry a whole compact-encoding group
                // (many names under one module, or many names under one type),
                // so counting section entries would undercount the imported
                // functions and shift every function index reported below.
                for import in reader.into_imports() {
                    let import = import.ok()?;
                    if matches!(
                        import.ty,
                        wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                    ) {
                        imported_function_count += 1;
                    }
                }
            }
            WasmPayload::CodeSectionEntry(body) => {
                let facts = WasmCodeBodyFacts {
                    function_index: imported_function_count + code_entry_ordinal,
                    body_bytes: body.range().len(),
                    declared_locals: wasm_body_declared_locals(&body),
                };
                code_entry_ordinal += 1;
                if largest.is_none_or(|current| facts.body_bytes > current.body_bytes) {
                    largest = Some(facts);
                }
                if most_locals.is_none_or(|current| {
                    facts.declared_locals.unwrap_or(0) > current.declared_locals.unwrap_or(0)
                }) {
                    most_locals = Some(facts);
                }
            }
            _ => {}
        }
    }

    let largest = largest?;
    let most_locals = most_locals?;
    Some(format!(
        "largest emitted function: {largest}; most locals: {most_locals}"
    ))
}

fn largest_wasm_code_body_size(bytes: &[u8]) -> Option<usize> {
    let mut largest_code_body_bytes = 0;
    for payload in WasmParser::new(0).parse_all(bytes) {
        let payload = payload.ok()?;
        if let WasmPayload::CodeSectionEntry(body) = payload {
            largest_code_body_bytes = largest_code_body_bytes.max(body.range().len());
        }
    }
    Some(largest_code_body_bytes)
}

#[derive(Clone, Copy)]
enum WasmModuleSource<'a> {
    Program(&'a [u8]),
    Runtime(&'a scriptw_aot_wasm::RuntimeArtifact),
    DiagnosticRuntime(&'a scriptw_aot_wasm::RuntimeArtifact),
}

impl<'a> WasmModuleSource<'a> {
    fn bytes(self) -> &'a [u8] {
        match self {
            Self::Program(bytes) => bytes,
            Self::Runtime(runtime) | Self::DiagnosticRuntime(runtime) => runtime.bytes(),
        }
    }

    fn runtime(
        runtime: &'a scriptw_aot_wasm::RuntimeArtifact,
        policy: WasmRuntimeImagePolicy,
    ) -> Self {
        match policy {
            WasmRuntimeImagePolicy::EmbeddedBuild => Self::Runtime(runtime),
            WasmRuntimeImagePolicy::MatchedDiagnostic => Self::DiagnosticRuntime(runtime),
        }
    }

    fn runtime_image_policy(self) -> WasmRuntimeImagePolicy {
        match self {
            Self::Program(_) | Self::Runtime(_) => WasmRuntimeImagePolicy::EmbeddedBuild,
            Self::DiagnosticRuntime(_) => WasmRuntimeImagePolicy::MatchedDiagnostic,
        }
    }
}

fn wasm_module_memory_cache_key(
    source: WasmModuleSource<'_>,
    native_compilation_mode: WasmNativeCompilationMode,
) -> WasmModuleMemoryCacheKey {
    WasmModuleMemoryCacheKey {
        native_compilation_mode,
        runtime_image_policy: source.runtime_image_policy(),
        wasm_sha256: match source {
            WasmModuleSource::Program(bytes) => Sha256::digest(bytes).into(),
            WasmModuleSource::Runtime(runtime) | WasmModuleSource::DiagnosticRuntime(runtime) => {
                runtime.wasm_sha256()
            }
        },
    }
}

fn compile_wasm_module(
    engine: &WasmtimeEngine,
    bytes: &[u8],
) -> Result<WasmtimeModule, EngineError> {
    on_native_compilation_thread(compilation_jobs(), || WasmtimeModule::new(engine, bytes))?
        .map_err(|err| {
            let message = format!("wasmtime module validation failed: {err:#}");
            // Cranelift's per-function limits are the one failure class whose
            // message names neither the function nor its size. Attach the two
            // index rankings, so the failure detail recorded by the Test262
            // runner points at a specific code-section body.
            //
            // This says nothing about the `[origin:…]` prefix that detail also
            // carries: that is a `scriptw-test262` `FailureOrigin` value, and
            // the attribution is index-only precisely so it cannot perturb it
            // (see `describe_largest_wasm_code_body`). The retry in
            // `run_with_wasm_aot_inner` matches on the `Code for function is
            // too large` substring, which this only ever appends to.
            if message.contains(WASM_CODE_TOO_LARGE_MESSAGE) {
                if let Some(attribution) = describe_largest_wasm_code_body(bytes) {
                    return EngineError::new(format!("{message}; {attribution}"));
                }
            }
            EngineError::new(message)
        })
}

fn memory_cached_wasm_module(
    engine: &WasmtimeEngine,
    source: WasmModuleSource<'_>,
    native_compilation_mode: WasmNativeCompilationMode,
    create: impl FnOnce() -> Result<WasmtimeModule, EngineError>,
) -> Result<(WasmtimeModule, WasmModuleMemoryCacheOutcome), EngineError> {
    let key = wasm_module_memory_cache_key(source, native_compilation_mode);
    let modules = memory_wasm_modules();
    {
        let mut modules = modules
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(module) = modules.get_for_engine(&key, engine) {
            return Ok((module, WasmModuleMemoryCacheOutcome::Hit));
        }
    }

    let module = create()?;
    let mut modules = modules
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let module = modules.retain_compiled_for_engine(key, module);
    Ok((module, WasmModuleMemoryCacheOutcome::Miss))
}

fn wasm_module_for_execution(
    engine: &WasmtimeEngine,
    bytes: &[u8],
    memory_cache_policy: WasmModuleMemoryCachePolicy,
    native_compilation_mode: WasmNativeCompilationMode,
) -> Result<(WasmtimeModule, WasmModuleMemoryCacheOutcome), EngineError> {
    wasm_module_for_execution_with_factory(
        engine,
        WasmModuleSource::Program(bytes),
        memory_cache_policy,
        native_compilation_mode,
        || compile_wasm_module(engine, bytes),
    )
}

fn wasm_module_for_execution_with_factory(
    engine: &WasmtimeEngine,
    source: WasmModuleSource<'_>,
    memory_cache_policy: WasmModuleMemoryCachePolicy,
    native_compilation_mode: WasmNativeCompilationMode,
    create: impl FnOnce() -> Result<WasmtimeModule, EngineError>,
) -> Result<(WasmtimeModule, WasmModuleMemoryCacheOutcome), EngineError> {
    match (memory_cache_policy, source) {
        (WasmModuleMemoryCachePolicy::Retain, _)
        | (WasmModuleMemoryCachePolicy::RetainRuntime, WasmModuleSource::Runtime(_))
        | (WasmModuleMemoryCachePolicy::RetainRuntime, WasmModuleSource::DiagnosticRuntime(_)) => {
            memory_cached_wasm_module(engine, source, native_compilation_mode, create)
        }
        (WasmModuleMemoryCachePolicy::RetainRuntime, WasmModuleSource::Program(_))
        | (WasmModuleMemoryCachePolicy::BypassRetention, _) => {
            Ok((create()?, WasmModuleMemoryCacheOutcome::Bypassed))
        }
    }
}

#[cfg(test)]
fn memory_wasm_module_is_cached(bytes: &[u8]) -> bool {
    let wasm_sha256: [u8; 32] = Sha256::digest(bytes).into();
    memory_wasm_modules()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains_wasm_sha256(&wasm_sha256)
}

/// Wall-clock tick between `Engine::increment_epoch()` calls made by
/// `ensure_wasm_epoch_ticker`. This is the granularity of Wasm-AOT execution
/// timeouts: a requested timeout is rounded up to the nearest multiple of
/// this duration. 100ms keeps the ticker thread cheap (a handful of wakeups
/// per second, process-wide, not per test) while staying far tighter than
/// the tens-of-seconds bounds test262 runs use in practice.
const WASM_EPOCH_TICK_MS: u64 = 100;

/// Retained, typed context for a product Wasmtime configuration failure.
#[derive(Debug, Clone)]
struct WasmtimeEngineSetupError {
    mode: WasmNativeCompilationMode,
    policy: WasmtimeRuntimePolicy,
    source: String,
}

impl WasmtimeEngineSetupError {
    fn new(mode: WasmNativeCompilationMode, source: impl Into<String>) -> Self {
        Self {
            mode,
            policy: PRODUCT_WASMTIME_POLICY,
            source: source.into(),
        }
    }
}

impl core::fmt::Display for WasmtimeEngineSetupError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "wasmtime {} engine setup failed under required runtime policy ({}): {}",
            self.mode.as_str(),
            self.policy.report(),
            self.source
        )
    }
}

/// Builds the sole product Wasmtime configuration.
///
/// Native compiler profiles differ only in optimization level (and the
/// fast-profile cache verifier). Proposal flags and collector selection come
/// from one typed policy, so adding another profile cannot weaken the runtime
/// capability boundary.
fn product_wasmtime_config(
    mode: WasmNativeCompilationMode,
) -> Result<WasmtimeConfig, WasmtimeEngineSetupError> {
    let mut config =
        wasmtime_config::base_config(mode, wasmtime::WasmBacktraceDetails::Environment);
    config.parallel_compilation(compilation_jobs() > 1);
    config.cache(wasmtime_module_cache());
    if let Some(function_cache) = cranelift_function_cache() {
        config
            .enable_incremental_compilation(function_cache)
            .map_err(|err| {
                WasmtimeEngineSetupError::new(
                    mode,
                    format!("Cranelift function-cache setup failed: {err}"),
                )
            })?;
    }
    if mode == WasmNativeCompilationMode::Fast
        && std::env::var_os("SCRIPTW_VERIFY_FUNCTION_CACHE").is_some()
    {
        // This is Cranelift's own diagnostic flag: it recompiles a sampled
        // cache hit and asserts that the native result is byte-for-byte
        // identical. The flag remains confined to the fast opt-in CI/debug
        // path because Cranelift marks stringly-typed compiler flags as an
        // unsafe API.
        unsafe {
            config.cranelift_flag_enable("enable_incremental_compilation_cache_checks");
        }
    }
    Ok(config)
}

fn product_wasmtime_engine(
    mode: WasmNativeCompilationMode,
) -> Result<WasmtimeEngine, WasmtimeEngineSetupError> {
    let config = product_wasmtime_config(mode)?;
    let engine = WasmtimeEngine::new(&config)
        .map_err(|err| WasmtimeEngineSetupError::new(mode, format!("{err:#}")))?;
    PRODUCT_WASMTIME_POLICY
        .verify_engine(&engine)
        .map_err(|message| WasmtimeEngineSetupError::new(mode, message))?;
    Ok(engine)
}

fn cached_product_wasmtime_engine(
    engine: &'static OnceLock<Result<WasmtimeEngine, WasmtimeEngineSetupError>>,
    mode: WasmNativeCompilationMode,
) -> Result<WasmtimeEngine, EngineError> {
    engine
        .get_or_init(|| product_wasmtime_engine(mode))
        .clone()
        .map_err(EngineError::from_wasmtime_setup)
}

/// The fast-compilation `wasmtime::Engine` used for ordinary Wasm-AOT modules.
///
/// Built once rather than per run. Oversized modules use the companion
/// size-optimized engine below; both engines share immutable configuration and
/// compiled-code caches. An immutable `Module` may come from the in-memory LRU,
/// while each execution gets a fresh `Store` and `Instance`, so no JavaScript
/// state crosses runs.
fn shared_wasm_engine() -> Result<WasmtimeEngine, EngineError> {
    static ENGINE: OnceLock<Result<WasmtimeEngine, WasmtimeEngineSetupError>> = OnceLock::new();
    cached_product_wasmtime_engine(&ENGINE, WasmNativeCompilationMode::Fast)
}

fn shared_size_optimized_wasm_engine() -> Result<WasmtimeEngine, EngineError> {
    static ENGINE: OnceLock<Result<WasmtimeEngine, WasmtimeEngineSetupError>> = OnceLock::new();
    cached_product_wasmtime_engine(&ENGINE, WasmNativeCompilationMode::SizeOptimized)
}

fn shared_speed_wasm_engine() -> Result<WasmtimeEngine, EngineError> {
    static ENGINE: OnceLock<Result<WasmtimeEngine, WasmtimeEngineSetupError>> = OnceLock::new();
    cached_product_wasmtime_engine(&ENGINE, WasmNativeCompilationMode::Speed)
}

fn shared_native_mode_wasm_engine(
    mode: WasmNativeCompilationMode,
) -> Result<WasmtimeEngine, EngineError> {
    match mode {
        WasmNativeCompilationMode::Fast => shared_wasm_engine(),
        WasmNativeCompilationMode::SizeOptimized => shared_size_optimized_wasm_engine(),
        WasmNativeCompilationMode::Speed => shared_speed_wasm_engine(),
    }
}

/// Two hot diagnostic configurations may retain immutable Engines. Modules still
/// use the existing bounded image cache, checked against their actual Engine;
/// an evicted/recreated configuration cannot reuse a foreign Module. No Store,
/// JavaScript object, linker import or Realm is retained by this owner.
fn shared_profile_wasm_engine(
    options: WasmRuntimeProfileOptions,
) -> Result<WasmtimeEngine, EngineError> {
    const RETAINED_CONFIGURATIONS: usize = 2;
    static ENGINES: OnceLock<Mutex<VecDeque<(WasmRuntimeProfileOptions, WasmtimeEngine)>>> =
        OnceLock::new();
    let engines = ENGINES.get_or_init(Default::default);
    {
        let mut engines = engines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(position) = engines.iter().position(|(key, _)| *key == options) {
            let entry = engines.remove(position).expect("profile Engine key exists");
            let engine = entry.1.clone();
            engines.push_back(entry);
            return Ok(engine);
        }
    }
    let mode = options.native_mode().internal();
    let mut config = product_wasmtime_config(mode).map_err(EngineError::from_wasmtime_setup)?;
    config.gc_heap_initial_size(options.initial_gc_capacity().bytes() as u64);
    config.cranelift_regalloc_algorithm(options.register_allocation().algorithm());
    options.configure_native_inlining(&mut config);
    let engine = WasmtimeEngine::new(&config).map_err(|err| {
        EngineError::from_wasmtime_setup(WasmtimeEngineSetupError::new(mode, format!("{err:#}")))
    })?;
    PRODUCT_WASMTIME_POLICY
        .verify_engine(&engine)
        .map_err(|message| {
            EngineError::from_wasmtime_setup(WasmtimeEngineSetupError::new(mode, message))
        })?;
    let mut engines = engines
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // Engine construction runs outside the mutex, like module compilation.
    let engine = match engines.iter().position(|(key, _)| *key == options) {
        Some(position) => {
            engines
                .remove(position)
                .expect("profile Engine key exists")
                .1
        }
        None => engine,
    };
    if engines.len() == RETAINED_CONFIGURATIONS {
        engines.pop_front();
    }
    engines.push_back((options, engine.clone()));
    Ok(engine)
}

fn registered_wasm_epoch_engines() -> &'static Mutex<Vec<wasmtime::EngineWeak>> {
    static ENGINES: OnceLock<Mutex<Vec<wasmtime::EngineWeak>>> = OnceLock::new();
    ENGINES.get_or_init(|| Mutex::new(Vec::new()))
}

/// Registers an engine with the process-wide ticker that increments every
/// Wasm-AOT engine's epoch every `WASM_EPOCH_TICK_MS`. Every store created by
/// `run_with_wasm_aot_inner` sets its epoch deadline in units of this tick, so
/// both native-compilation engines must remain registered: otherwise a module
/// compiled by the engine that registered second could never time out.
fn ensure_wasm_epoch_ticker(engine: &WasmtimeEngine) {
    {
        let mut engines = registered_wasm_epoch_engines()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        engines.retain(|registered| registered.upgrade().is_some());
        let already_registered = engines.iter().any(|registered| {
            registered
                .upgrade()
                .is_some_and(|registered| WasmtimeEngine::same(&registered, engine))
        });
        if !already_registered {
            engines.push(engine.weak());
        }
    }

    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::Builder::new()
            .name("scriptw-wasm-epoch-ticker".to_string())
            .spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(WASM_EPOCH_TICK_MS));
                let mut engines = registered_wasm_epoch_engines()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                engines.retain(|registered| {
                    let Some(engine) = registered.upgrade() else {
                        return false;
                    };
                    engine.increment_epoch();
                    true
                });
            })
            .expect("scriptw wasm epoch ticker thread should spawn");
    });
}

/// True if `err` (from a trapped `wasmtime` call) is specifically the
/// epoch-interruption trap raised when a store's `set_epoch_deadline` bound
/// was exceeded, as opposed to any other kind of wasm trap (unreachable,
/// stack overflow, out-of-bounds access, ...).
fn is_wasm_epoch_interrupt(err: &wasmtime::Error) -> bool {
    err.downcast_ref::<WasmtimeTrap>() == Some(&WasmtimeTrap::Interrupt)
}

pub use scriptw_runtime::{
    AgentId, EmbeddedModuleBytesInput, EmbeddedModuleContents, EmbeddedModuleEntryInput,
    EmbeddedModuleGoal, EmbeddedModuleGraph, EmbeddedModuleGraphError, EmbeddedModuleInput,
    EmbeddedModuleKind, EmbeddedModuleReferrer, EmbeddedModuleRequest,
    EmbeddedModuleResolutionInput, EmbeddedModuleSourceInput, EmbeddedModuleSourceValueInput,
    GlobalEnvironmentId, HostClock, HostHooks, HostOutputEvent, HostRandom, HostRandomError,
    IntrinsicDescriptor, IntrinsicFunctionMetadata, IntrinsicId, IntrinsicKind, IntrinsicLink,
    IntrinsicPropertyAttributes, IntrinsicPropertyDescriptor, IntrinsicPropertyKey,
    IntrinsicPropertyValue, IntrinsicRole, InvalidObservedBigInt, ModuleLoadingPolicy,
    MonotonicClockDuration, MonotonicClockInstant, NullHostHooks, ObservedBigInt,
    ObservedCompletion, ObservedJsValue, ObservedNumber, RandomUnitInterval, Realm, RealmBuilder,
    RealmGlobal, RealmId, RealmIntrinsics, RealmObjectId, RealmObjectKind, SystemHostClock,
    SystemHostRandom, UtcEpochMilliseconds, INTRINSIC_DESCRIPTORS, INTRINSIC_PROPERTY_DESCRIPTORS,
};

/// The product execution backend selector.
///
/// `WasmAot` is the only backend whose results count as conformance and it is
/// the default everywhere (engine, CLI `run`/`build`/`test262`). `SpecExec`
/// wraps an interpreter (Boa) and exists solely as a hidden, developer-only
/// differential oracle for T25-style testing; per `AGENTS.md` it must never be
/// the product default, a silent fallback, or linked into product/release
/// builds. Its implementation is gated behind the `spec-exec-oracle` cargo
/// feature, which is off by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ExecutionBackend {
    SpecExec,
    #[default]
    WasmAot,
}

impl ExecutionBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            ExecutionBackend::SpecExec => "spec-exec",
            ExecutionBackend::WasmAot => "wasm-aot",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub bytes: Vec<u8>,
    pub description: String,
    /// The Wasm emitter's self-description for this artifact.
    ///
    /// This field exists because dropping it was a real defect, not a
    /// hypothetical one. `scriptw_aot_wasm::emit` returns a `debug_dump`
    /// carrying the emitted-size attribution (`largest emitted function: ...`
    /// and, under `SCRIPTW_EMIT_SIZE_REPORT`, one line per emitted body);
    /// `emit_wasm_on_current_thread` used to build an `Artifact` that had
    /// nowhere to put it and discarded it. The only printer of a dump,
    /// `SCRIPTW_WASM_TRACE_DUMP` in `run_with_wasm_aot_inner`, is reachable
    /// only from `run_compiled_unit`, while `scriptw build wasm` and the Test262
    /// wasm-aot backend both go through this path — so the variable named
    /// "trace dump" printed nothing on exactly the two commands anyone would
    /// use it from, and `SCRIPTW_EMIT_SIZE_REPORT` silently reported nothing
    /// for a whole batch of size work.
    pub debug_dump: String,
    /// The runtime module `bytes` links against. `bytes`
    /// alone is then only the small program module; `None` means `bytes` is a
    /// standalone module.
    pub runtime: Option<LinkedRuntime>,
    /// Original lowering policy for execution-time native source compilation.
    /// Raw externally constructed artifacts may leave this absent.
    pub compilation_policy: Option<WasmCompilationPolicy>,
}

impl Artifact {
    /// Validated native import requirements carried by P itself. This works
    /// after source-free persistence and never consults a host closure registry.
    pub fn foreign_modules(&self) -> Result<ForeignModuleRegistry, EngineError> {
        scriptw_aot_wasm::foreign_registry_from_module(&self.bytes)
            .map_err(|error| EngineError::new(format!("foreign Wasm admission failed: {error}")))
    }

    fn program(&self) -> WasmProgramRef<'_> {
        let program =
            WasmProgramRef::new(&self.bytes, self.runtime.as_ref().map(|runtime| &runtime.0));
        match &self.compilation_policy {
            Some(policy) => program.with_compilation_policy(policy),
            None => program,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileOptions {
    pub filename: Option<String>,
    /// Global Script evaluated before a Script or Module entry in the same realm.
    /// Parsed independently; neither source inherits the other's directives.
    /// The entry's declarations are instantiated only after this Script succeeds.
    pub module_prelude: Option<String>,
    pub optimize: bool,
    /// Directory the filesystem module loader is confined to.
    ///
    /// `None` means the entry file's parent directory. A specifier that
    /// resolves outside the root is rejected rather than read.
    pub module_root: Option<String>,
    /// Which host module loader compilation and oracle execution may use.
    ///
    /// `RejectAll` is load-bearing for source-only differential protocols: it
    /// prevents both AOT graph discovery and spec-exec dynamic source from
    /// consulting ambient files.
    pub module_loading_policy: ModuleLoadingPolicy,
    /// Explicit compile-time native module declarations. An empty registry
    /// exposes no foreign imports; names never discover ambient host functions.
    pub foreign_modules: ForeignModuleRegistry,
    /// Which closed set of host-backed globals lowering may expose.
    pub host_surface_policy: HostSurfacePolicy,
    /// Proposal semantics selected explicitly; default is pinned ECMA-262.
    pub language_features: LanguageFeaturePolicy,
    /// Host policy applied after Promise jobs drain; part of the emitted artifact.
    pub promise_rejection_policy: PromiseRejectionPolicy,
    /// Complete pinned Intl image selection retained by Wasm emission and execution.
    /// Named Custom profiles select the same pinned domains under their validated identity.
    pub intl_profile: IntlCompilationProfile,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            filename: None,
            module_prelude: None,
            optimize: true,
            module_root: None,
            module_loading_policy: ModuleLoadingPolicy::default(),
            foreign_modules: ForeignModuleRegistry::default(),
            host_surface_policy: HostSurfacePolicy::default(),
            language_features: LanguageFeaturePolicy::default(),
            promise_rejection_policy: PromiseRejectionPolicy::default(),
            intl_profile: IntlCompilationProfile::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    pub backend: ExecutionBackend,
    pub argv: Vec<String>,
    pub module_root: Option<String>,
    pub test_path: Option<String>,
    pub can_block: bool,
    /// Wall-clock bound for `ExecutionBackend::WasmAot` execution, enforced
    /// in-process via wasmtime epoch interruption (see
    /// `run_with_wasm_aot_inner`). `None` means "no caller-specified bound";
    /// execution still runs under epoch interruption but with an
    /// effectively-unbounded deadline, so it behaves as before this field
    /// existed. Ignored by other backends.
    pub timeout_ms: Option<u64>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            backend: ExecutionBackend::default(),
            argv: Vec::new(),
            module_root: None,
            test_path: None,
            can_block: true,
            timeout_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilationUnit {
    pub source: SourceUnit,
    pub ir: ProgramIr,
    module_prelude: Option<String>,
    module_loading_policy: ModuleLoadingPolicy,
    promise_rejection_policy: PromiseRejectionPolicy,
    intl_profile: IntlCompilationProfile,
    host_surface_policy: HostSurfacePolicy,
}

fn require_default_intl_profile(
    profile: &IntlCompilationProfile,
    backend: &str,
) -> Result<(), EngineError> {
    match profile {
        IntlCompilationProfile::Minimal => Ok(()),
        IntlCompilationProfile::Conformance => Err(EngineError::new(format!(
            "the Conformance Intl data producer is unsupported by {backend}; use Wasm AOT"
        ))),
        IntlCompilationProfile::Custom(id) => Err(EngineError::new(format!(
            "custom Intl profile '{}' is unsupported by {backend}; use Wasm AOT",
            id.as_str()
        ))),
        IntlCompilationProfile::CustomProjection(selection) => Err(EngineError::new(format!(
            "custom Intl projection '{}' is unsupported by {backend}; use Wasm AOT",
            selection.id().as_str()
        ))),
    }
}

fn validate_execution_language_features(
    policy: LanguageFeaturePolicy,
    backend: ExecutionBackend,
) -> Result<(), EngineError> {
    if backend == ExecutionBackend::SpecExec && policy != LanguageFeaturePolicy::default() {
        return Err(EngineError::new(
            "selected language proposals are unsupported by spec-exec; use Wasm AOT",
        ));
    }
    Ok(())
}

fn validate_foreign_oracle(
    registry: &ForeignModuleRegistry,
    backend: ExecutionBackend,
) -> Result<(), EngineError> {
    if backend == ExecutionBackend::SpecExec && !registry.is_empty() {
        return Err(EngineError::new(
            "declared foreign functions require emitted Wasm and an embedder-owned Linker; spec-exec does not bind native functions",
        ));
    }
    Ok(())
}

fn validate_execution_intl_profile(
    profile: &IntlCompilationProfile,
    backend: ExecutionBackend,
) -> Result<(), EngineError> {
    match backend {
        ExecutionBackend::WasmAot => Ok(()),
        ExecutionBackend::SpecExec => require_default_intl_profile(profile, "spec-exec"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    pub backend_used: ExecutionBackend,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedRunOutcome {
    pub backend_used: ExecutionBackend,
    pub completion: ObservedCompletion,
    pub output_events: Vec<HostOutputEvent>,
    /// Type-bounded backend diagnostic text for humans. It is not the
    /// structured completion value and never contains the private rendering
    /// retained for legacy adapters.
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectionReport {
    pub goal: ParseGoal,
    pub source_len: usize,
    pub stages: Vec<&'static str>,
    pub invariants: Vec<&'static str>,
    pub ir_summary: String,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineError {
    message: String,
    oracle_entry_syntax_rejection: bool,
    global_prelude_failure: bool,
    oracle_javascript_exception: Option<(
        scriptw_runtime::OracleExceptionPhase,
        scriptw_runtime::OracleExceptionType,
    )>,
    parse_diagnostic: Option<ParseDiagnostic>,
    ir_diagnostic: Option<IrDiagnostic>,
    intl_artifact_identity_error: Option<IntlArtifactIdentityError>,
    wasmtime_policy: Option<WasmtimeRuntimePolicy>,
    execution_failure: Option<EngineExecutionFailure>,
}

/// Closed pre-instantiation failures for the Wasm Intl artifact/provider
/// identity contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntlArtifactIdentityError {
    MissingSection,
    DuplicateSections,
    IdentityMismatch,
    UnexpectedSection,
    InvalidDataImage,
}

impl core::fmt::Display for IntlArtifactIdentityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingSection => write!(
                f,
                "Wasm artifact imports {WASM_HOST_IMPORT_NAMESPACE}.{WASM_HOST_IMPORT_INTL_CALL} or {WASM_HOST_IMPORT_NAMESPACE}.{WASM_HOST_IMPORT_SYSTEM_TIME_ZONE} but is missing the required {INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION} custom section"
            ),
            Self::DuplicateSections => write!(
                f,
                "Wasm artifact carries more than one {INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION} custom section"
            ),
            Self::IdentityMismatch => write!(
                f,
                "Wasm artifact {INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION} does not match the shared embedded Intl provider identity"
            ),
            Self::InvalidDataImage => f.write_str("Wasm Intl component images are missing, duplicated or incompatible with their selected provider"),
            Self::UnexpectedSection => write!(
                f,
                "Wasm artifact carries {INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION} but does not import {WASM_HOST_IMPORT_NAMESPACE}.{WASM_HOST_IMPORT_INTL_CALL} or {WASM_HOST_IMPORT_NAMESPACE}.{WASM_HOST_IMPORT_SYSTEM_TIME_ZONE}"
            ),
        }
    }
}

impl std::error::Error for IntlArtifactIdentityError {}

impl EngineError {
    pub const fn is_global_prelude_failure(&self) -> bool {
        self.global_prelude_failure
    }
    fn in_global_prelude(mut self) -> Self {
        self.global_prelude_failure = true;
        self
    }

    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            oracle_entry_syntax_rejection: false,
            global_prelude_failure: false,
            oracle_javascript_exception: None,
            parse_diagnostic: None,
            ir_diagnostic: None,
            intl_artifact_identity_error: None,
            wasmtime_policy: None,
            execution_failure: None,
        }
    }

    fn from_parse_error(err: scriptw_front::ParseError) -> Self {
        Self {
            message: err.to_string(),
            oracle_entry_syntax_rejection: false,
            global_prelude_failure: false,
            oracle_javascript_exception: None,
            parse_diagnostic: Some(err.diagnostic().clone()),
            ir_diagnostic: None,
            intl_artifact_identity_error: None,
            wasmtime_policy: None,
            execution_failure: None,
        }
    }

    fn from_ir_diagnostic(diagnostic: IrDiagnostic) -> Self {
        Self {
            message: diagnostic.message.clone(),
            oracle_entry_syntax_rejection: false,
            global_prelude_failure: diagnostic.is_global_prelude_failure(),
            oracle_javascript_exception: None,
            parse_diagnostic: None,
            ir_diagnostic: Some(diagnostic),
            intl_artifact_identity_error: None,
            wasmtime_policy: None,
            execution_failure: None,
        }
    }

    fn from_wasm_emit_error(program: &ProgramIr, err: impl core::fmt::Display) -> Self {
        if let Some(diagnostic) = program.wasm_blocking_diagnostic() {
            return Self::from_ir_diagnostic(diagnostic.clone());
        }
        Self::new(format!(
            "{}. Product invariant: compile JavaScript directly to Wasm; do not ship interpreter-in-Wasm.",
            err
        ))
    }

    fn from_wasmtime_setup(err: WasmtimeEngineSetupError) -> Self {
        Self {
            message: err.to_string(),
            oracle_entry_syntax_rejection: false,
            global_prelude_failure: false,
            oracle_javascript_exception: None,
            parse_diagnostic: None,
            ir_diagnostic: None,
            intl_artifact_identity_error: None,
            wasmtime_policy: Some(err.policy),
            execution_failure: None,
        }
    }

    fn from_intl_artifact_identity(error: IntlArtifactIdentityError) -> Self {
        Self {
            message: error.to_string(),
            oracle_entry_syntax_rejection: false,
            global_prelude_failure: false,
            oracle_javascript_exception: None,
            parse_diagnostic: None,
            ir_diagnostic: None,
            intl_artifact_identity_error: Some(error),
            wasmtime_policy: None,
            execution_failure: None,
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// The optional differential oracle rejected its entry in its native
    /// parser. It does not separately classify grammar and static semantics.
    pub const fn is_oracle_entry_syntax_rejection(&self) -> bool {
        self.oracle_entry_syntax_rejection
    }
    pub const fn oracle_javascript_exception_phase(
        &self,
    ) -> Option<scriptw_runtime::OracleExceptionPhase> {
        match self.oracle_javascript_exception {
            Some((phase, _)) => Some(phase),
            None => None,
        }
    }
    pub const fn oracle_javascript_exception_constructor_name(&self) -> Option<&'static str> {
        match self.oracle_javascript_exception {
            Some((_, kind)) => kind.constructor_name(),
            None => None,
        }
    }

    #[cfg(feature = "spec-exec-oracle")]
    fn from_spec_exec(error: scriptw_spec_exec::ExecutionError) -> Self {
        let mut result = Self::new(error.to_string());
        result.oracle_entry_syntax_rejection = error.is_entry_syntax_rejection();
        result.global_prelude_failure = error.is_global_prelude_failure();
        result.oracle_javascript_exception = error
            .javascript_exception_phase()
            .zip(error.javascript_exception_type());
        result
    }

    pub fn parse_diagnostic(&self) -> Option<&ParseDiagnostic> {
        self.parse_diagnostic.as_ref()
    }

    pub fn ir_diagnostic(&self) -> Option<&IrDiagnostic> {
        self.ir_diagnostic.as_ref()
    }

    /// Typed reason an Intl artifact/provider identity gate rejected the Wasm
    /// bytes before Wasmtime compilation or instantiation.
    pub const fn intl_artifact_identity_error(&self) -> Option<IntlArtifactIdentityError> {
        self.intl_artifact_identity_error
    }

    /// Required GC capability attached to a Wasmtime engine-construction
    /// failure. Other engine errors have no runtime-policy classification.
    pub const fn wasm_gc_capability(&self) -> Option<WasmGcCapability> {
        match self.wasmtime_policy {
            Some(policy) => Some(policy.gc_capability()),
            None => None,
        }
    }

    /// Required weak-reachability capability attached to a Wasmtime
    /// engine-construction failure. This is separate from
    /// [`Self::wasm_gc_capability`] because Wasm GC does not itself provide
    /// weak references or ephemerons.
    pub const fn wasm_weak_reachability_capability(
        &self,
    ) -> Option<WasmWeakReachabilityCapability> {
        match self.wasmtime_policy {
            Some(policy) => Some(policy.weak_reachability_capability()),
            None => None,
        }
    }
}

impl core::fmt::Display for EngineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EngineError {}

fn wasm_reject_runtime_semantics(operation_code: i64) -> wasmtime::Result<()> {
    let rejection = RuntimeSemanticRejection::from_abi_code(operation_code).ok_or_else(|| {
        wasmtime::Error::msg(format!(
            "invalid runtime semantic rejection code {operation_code}"
        ))
    })?;
    Err(wasmtime::Error::new(rejection))
}

pub struct Engine {
    realm: Realm,
    #[cfg(test)]
    native_function_compilation_observer:
        Option<Arc<wasm_native_function_compiler::NativeFunctionCompilationObserver>>,
    #[cfg(test)]
    native_global_script_compilation_observer:
        Option<Arc<wasm_native_global_script_compiler::NativeGlobalScriptCompilationObserver>>,
    #[cfg(test)]
    native_direct_eval_compilation_observer:
        Option<Arc<wasm_native_direct_eval_compiler::NativeDirectEvalCompilationObserver>>,
}

enum WasmExecutionMode {
    Legacy,
    Structured,
    Graph(SnapshotLimits),
}

enum WasmExecutionOutcome {
    Legacy(RunOutcome),
    Structured(ObservedRunOutcome),
    Graph(GraphRunOutcome),
}

impl WasmExecutionOutcome {
    fn into_legacy(self) -> Result<RunOutcome, EngineError> {
        match self {
            Self::Legacy(outcome) => Ok(outcome),
            Self::Structured(_) | Self::Graph(_) => Err(EngineError::new(
                "internal wasm execution mode mismatch: expected legacy outcome",
            )),
        }
    }

    fn into_structured(self) -> Result<ObservedRunOutcome, EngineError> {
        match self {
            Self::Structured(outcome) => Ok(outcome),
            Self::Legacy(_) | Self::Graph(_) => Err(EngineError::new(
                "internal wasm execution mode mismatch: expected structured outcome",
            )),
        }
    }
}

#[derive(Clone)]
enum WasmOutputEvents {
    DelegateOnly,
    Capture(Arc<Mutex<Vec<HostOutputEvent>>>),
}

impl WasmOutputEvents {
    fn for_mode(mode: &WasmExecutionMode) -> Self {
        match mode {
            WasmExecutionMode::Legacy => Self::DelegateOnly,
            WasmExecutionMode::Structured | WasmExecutionMode::Graph(_) => {
                Self::Capture(Arc::new(Mutex::new(Vec::new())))
            }
        }
    }

    fn record(&self, text: &str) {
        if let Self::Capture(events) = self {
            events
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(HostOutputEvent::PrintLine(text.to_string()));
        }
    }

    fn record_exception(&self, text: &str) {
        if let Self::Capture(events) = self {
            events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(HostOutputEvent::ReportedException(text.to_string()));
        }
    }

    fn take(&self) -> Vec<HostOutputEvent> {
        let Self::Capture(events) = self else {
            return Vec::new();
        };
        let mut events = events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::take(&mut *events)
    }
}

type WasmHostState = WasmStoreState;

fn shared_embedded_intl_kernel() -> Result<Arc<IntlKernel<EmbeddedIntlProvider>>, EngineError> {
    scriptw_intl::shared_embedded_intl_kernel().map_err(|error| EngineError::new(error.to_string()))
}

#[derive(Clone, Copy)]
enum IntlArtifactIdentityRequirement {
    HostImports,
    AdmittedComponents,
}

/// Enforces the selected data-to-provider identity relation while the artifact is
/// still inert bytes. The section is compared to `scriptw-intl`'s canonical
/// serialization directly; the engine deliberately owns no parallel decoder
/// or field vocabulary.
fn validate_wasm_intl_artifact_identity(
    bytes: &[u8],
    expected: &IntlDataIdentity,
    requirement: IntlArtifactIdentityRequirement,
) -> Result<(), EngineError> {
    let mut imports_intl_host = false;
    let mut section_count = 0usize;
    let mut identity_matches = false;
    let expected = expected.artifact_identity();

    for payload in WasmParser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|error| {
            EngineError::new(format!(
                "failed to inspect Wasm Intl artifact identity before instantiation: {error}"
            ))
        })?;
        match payload {
            WasmPayload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.map_err(|error| {
                        EngineError::new(format!(
                            "failed to inspect Wasm Intl imports before instantiation: {error}"
                        ))
                    })?;
                    if import.module == WASM_HOST_IMPORT_NAMESPACE
                        && matches!(
                            import.name,
                            WASM_HOST_IMPORT_INTL_CALL | WASM_HOST_IMPORT_SYSTEM_TIME_ZONE
                        )
                    {
                        imports_intl_host = true;
                    }
                }
            }
            WasmPayload::CustomSection(section)
                if section.name() == INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION =>
            {
                section_count += 1;
                identity_matches = section.data() == expected.as_bytes();
            }
            _ => {}
        }
    }

    let requires_identity = match requirement {
        IntlArtifactIdentityRequirement::HostImports => imports_intl_host,
        IntlArtifactIdentityRequirement::AdmittedComponents => true,
    };
    let error = match (requires_identity, section_count, identity_matches) {
        (_, 2.., _) => Some(IntlArtifactIdentityError::DuplicateSections),
        (true, 0, _) => Some(IntlArtifactIdentityError::MissingSection),
        (false, 1, _) => Some(IntlArtifactIdentityError::UnexpectedSection),
        (true, 1, false) => Some(IntlArtifactIdentityError::IdentityMismatch),
        (false, 0, _) | (true, 1, true) => None,
    };
    match error {
        Some(error) => Err(EngineError::from_intl_artifact_identity(error)),
        None => Ok(()),
    }
}

fn wasm_random_f64(caller: WasmtimeCaller<'_, WasmHostState>) -> wasmtime::Result<f64> {
    caller
        .data()
        .realm
        .host_random()
        .random_unit_interval()
        .map(RandomUnitInterval::get)
        .map_err(|error| {
            wasmtime::Error::msg(format!("wasmtime host random import failed: {error}"))
        })
}

/// Primitive message data may cross Stores; collector roots never do.
#[derive(Debug, Clone, PartialEq, Eq)]
enum WasmAgentMessageId {
    Int32(i32),
    BigInt(ObservedBigInt),
}

#[derive(Clone)]
struct WasmAgentBroadcast {
    resource: Arc<WasmSharedBufferResource>,
    id: WasmAgentMessageId,
}

enum WasmAgentCommand {
    Broadcast {
        broadcast: WasmAgentBroadcast,
        retrieved: std::sync::mpsc::Sender<()>,
    },
    Shutdown,
}

struct WasmAgentWorker {
    commands: std::sync::mpsc::Sender<WasmAgentCommand>,
    join: std::thread::JoinHandle<Result<(), EngineError>>,
}

/// The closed subset of root compilation authority inherited by an agent.
///
/// Keeping these policies in one value prevents either the harness-to-group
/// handoff or a worker-cache retry from rebuilding options with an ambient
/// default.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WasmAgentCompilePolicy {
    host_surface_policy: HostSurfacePolicy,
    language_features: LanguageFeaturePolicy,
    module_loading_policy: ModuleLoadingPolicy,
    promise_rejection_policy: PromiseRejectionPolicy,
    intl_profile: IntlCompilationProfile,
}

impl WasmAgentCompilePolicy {
    fn from_root(options: &CompileOptions) -> Self {
        Self {
            host_surface_policy: options.host_surface_policy,
            language_features: options.language_features,
            module_loading_policy: options.module_loading_policy.clone(),
            promise_rejection_policy: options.promise_rejection_policy,
            intl_profile: options.intl_profile.clone(),
        }
    }

    fn worker_options(&self) -> CompileOptions {
        CompileOptions {
            filename: Some("<test262-agent>".to_string()),
            host_surface_policy: self.host_surface_policy,
            language_features: self.language_features,
            module_loading_policy: self.module_loading_policy.clone(),
            promise_rejection_policy: self.promise_rejection_policy,
            intl_profile: self.intl_profile.clone(),
            ..CompileOptions::default()
        }
    }
}

struct WasmAgentGroup {
    engine: WasmtimeEngine,
    /// The native mode `engine` was built for.
    native_compilation_mode: WasmNativeCompilationMode,
    realm: Realm,
    shared_memory_backing: Arc<WasmSharedMemoryBacking>,
    prelude: Arc<str>,
    compile_policy: WasmAgentCompilePolicy,
    timeout_ms: Option<u64>,
    execution_control: Arc<WasmAgentExecutionControl>,
    started_at: MonotonicClockInstant,
    reports: Mutex<VecDeque<Vec<u8>>>,
    worker_artifacts: Mutex<HashMap<String, ProgramWasmArtifact>>,
    workers: Mutex<Vec<WasmAgentWorker>>,
}

struct WasmAgentHarness {
    prelude: Arc<str>,
    compile_policy: WasmAgentCompilePolicy,
}

struct WasmAgentExecution {
    group: Arc<WasmAgentGroup>,
    commands: Arc<Mutex<std::sync::mpsc::Receiver<WasmAgentCommand>>>,
    leaving: Arc<std::sync::atomic::AtomicBool>,
    ready: std::sync::mpsc::SyncSender<()>,
}

impl WasmAgentGroup {
    fn start(self: &Arc<Self>, source: String) -> Result<(), EngineError> {
        self.execution_control.remaining()?;
        let worker_source = source;
        let mut worker_options = self.compile_policy.worker_options();
        worker_options.module_prelude = Some(self.prelude.to_string());
        let mut worker_artifact = self
            .worker_artifacts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&worker_source)
            .cloned();
        if worker_artifact.is_none() {
            let worker_engine = Engine::new(self.realm.clone());
            let artifact = worker_engine.load_or_compile_program_wasm_for_role(
                &worker_source,
                ParseGoal::Script,
                worker_options.clone(),
                program_wasm_cache(),
                module_loader::EmbeddedSourceRole::UnlocatedScript,
            )?;
            self.worker_artifacts
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(worker_source.clone(), artifact.clone());
            worker_artifact = Some(artifact);
        }
        let mut worker_artifact = worker_artifact.expect("worker artifact was initialized");
        let mut may_retry_corrupt_cache = true;

        loop {
            // Host compilation cannot be interrupted by a Wasm epoch. Do not
            // spawn a worker if it consumed the remaining parent budget.
            self.execution_control.remaining()?;
            let (commands_tx, commands_rx) = std::sync::mpsc::channel();
            let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(0);
            let execution = WasmAgentExecution {
                group: Arc::clone(self),
                commands: Arc::new(Mutex::new(commands_rx)),
                leaving: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                ready: ready_tx,
            };
            let timeout_ms = self.timeout_ms;
            let worker_engine = Engine::new(self.realm.clone());
            let execution_artifact = worker_artifact.clone();
            let join = std::thread::Builder::new()
                .name("scriptw-test262-wasm-agent".to_string())
                .stack_size(ENGINE_WORKER_STACK_SIZE)
                .spawn(move || {
                    worker_engine
                        .run_with_wasm_bytes_inner_with_agents(
                            execution_artifact.program(),
                            timeout_ms,
                            true,
                            WasmModuleMemoryCachePolicy::BypassRetention,
                            None,
                            Some(execution),
                        )
                        .map(|_| ())
                })
                .map_err(|err| EngineError::new(format!("failed to spawn Test262 agent: {err}")))?;
            match self.execution_control.receive(&ready_rx) {
                Ok(()) => {
                    self.workers
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(WasmAgentWorker {
                            commands: commands_tx,
                            join,
                        });
                    self.execution_control.remaining()?;
                    return Ok(());
                }
                Err(AgentReceiveError::Execution(error)) => {
                    // Release a rendezvous sender even if readiness races
                    // expiry; finish still owns and joins this worker.
                    drop(ready_rx);
                    self.workers
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(WasmAgentWorker {
                            commands: commands_tx,
                            join,
                        });
                    return Err(error);
                }
                Err(AgentReceiveError::Disconnected) => {}
            }

            let error = match join.join() {
                Ok(Err(error)) => error,
                Ok(Ok(())) => EngineError::new("Test262 agent stopped before becoming ready"),
                Err(payload) => {
                    let message = payload
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| {
                            payload
                                .downcast_ref::<&str>()
                                .map(|value| (*value).to_string())
                        })
                        .unwrap_or_else(|| "non-string panic payload".to_string());
                    EngineError::from_execution_failure(
                        EngineExecutionFailure::Trap,
                        format!("Test262 agent panicked before becoming ready: {message}"),
                    )
                }
            };
            if !may_retry_corrupt_cache || !worker_artifact.evict_if_invalid(&error) {
                return Err(error);
            }

            may_retry_corrupt_cache = false;
            self.worker_artifacts
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&worker_source);
            let worker_engine = Engine::new(self.realm.clone());
            worker_artifact = worker_engine.compile_program_wasm_on_current_thread(
                &worker_source,
                ParseGoal::Script,
                worker_options.clone(),
                program_wasm_cache(),
            )?;
            self.worker_artifacts
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(worker_source.clone(), worker_artifact.clone());
        }
    }

    fn broadcast(&self, broadcast: WasmAgentBroadcast) -> Result<usize, EngineError> {
        let retrieved = {
            let workers = self
                .workers
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            // Queue every delivery before waiting, and release the worker
            // registry while recipients retrieve their native resources.
            // Disconnection must retain the join handle for finish's verdict.
            workers
                .iter()
                .filter_map(|worker| {
                    let (acknowledge, retrieved) = std::sync::mpsc::channel();
                    worker
                        .commands
                        .send(WasmAgentCommand::Broadcast {
                            broadcast: broadcast.clone(),
                            retrieved: acknowledge,
                        })
                        .ok()
                        .map(|()| retrieved)
                })
                .collect::<Vec<_>>()
        };
        let mut count = 0;
        for receipt in retrieved {
            match self.execution_control.receive(&receipt) {
                Ok(()) => count += 1,
                // The worker's join handle remains owned by finish.
                Err(AgentReceiveError::Disconnected) => {}
                Err(AgentReceiveError::Execution(error)) => return Err(error),
            }
        }
        Ok(count)
    }

    fn finish(&self) -> Result<(), EngineError> {
        self.execution_control.finish();
        self.shared_memory_backing.cancel_all_waiters();
        let workers = {
            let mut workers = self
                .workers
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            std::mem::take(&mut *workers)
        };
        for worker in &workers {
            let _ = worker.commands.send(WasmAgentCommand::Shutdown);
        }
        let mut failures = Vec::new();
        for worker in workers {
            match worker.join.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => failures.push(error),
                Err(payload) => {
                    let message = payload
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| {
                            payload
                                .downcast_ref::<&str>()
                                .map(|value| (*value).to_string())
                        })
                        .unwrap_or_else(|| "non-string panic payload".to_string());
                    failures.push(EngineError::from_execution_failure(
                        EngineExecutionFailure::Trap,
                        format!("Test262 agent panicked: {message}"),
                    ));
                }
            }
        }
        let mut failures = failures.into_iter();
        match failures.next() {
            None => Ok(()),
            Some(first) => Err(EngineError::from_execution_failures(
                first,
                failures.collect(),
            )),
        }
    }
}

impl Engine {
    pub fn new(realm: Realm) -> Self {
        Self {
            realm,
            #[cfg(test)]
            native_function_compilation_observer: None,
            #[cfg(test)]
            native_global_script_compilation_observer: None,
            #[cfg(test)]
            native_direct_eval_compilation_observer: None,
        }
    }

    pub fn shell_name(&self) -> &str {
        self.realm.shell_name()
    }

    pub fn compile_script(
        &self,
        source: &str,
        options: CompileOptions,
    ) -> Result<CompilationUnit, EngineError> {
        self.compile_on_sized_stack(source, ParseGoal::Script, options)
    }

    pub fn compile_module(
        &self,
        source: &str,
        options: CompileOptions,
    ) -> Result<CompilationUnit, EngineError> {
        self.compile_on_sized_stack(source, ParseGoal::Module, options)
    }

    pub fn run_script(
        &self,
        source: &str,
        options: CompileOptions,
        run: RunOptions,
    ) -> Result<RunOutcome, EngineError> {
        self.run_source(source, ParseGoal::Script, options, run)
    }

    /// Runs a Module through the selected backend. Wasm AOT reports an
    /// unsettled entry at host quiescence as
    /// [`WasmExecutionFailureKind::IncompleteModuleEvaluation`], not success
    /// or a JavaScript exception. This one-shot API does not retain a resumable Store.
    pub fn run_module(
        &self,
        source: &str,
        options: CompileOptions,
        run: RunOptions,
    ) -> Result<RunOutcome, EngineError> {
        self.run_source(source, ParseGoal::Module, options, run)
    }

    fn run_source(
        &self,
        source: &str,
        goal: ParseGoal,
        options: CompileOptions,
        run: RunOptions,
    ) -> Result<RunOutcome, EngineError> {
        validate_execution_intl_profile(&options.intl_profile, run.backend)?;
        validate_execution_language_features(options.language_features, run.backend)?;
        validate_foreign_oracle(&options.foreign_modules, run.backend)?;
        match run.backend {
            ExecutionBackend::SpecExec => self.run_with_spec_exec(
                source,
                options.filename.as_deref(),
                goal,
                options.module_loading_policy,
                options.module_prelude.as_deref(),
                run,
            ),
            ExecutionBackend::WasmAot => self.run_source_with_cached_wasm(
                source,
                goal,
                options,
                run.timeout_ms,
                run.can_block,
            ),
        }
    }

    /// Executes one Script while keeping ECMAScript abrupt completion distinct
    /// from compiler, host and backend failures.
    pub fn observe_script(
        &self,
        source: &str,
        options: CompileOptions,
        run: RunOptions,
    ) -> Result<ObservedRunOutcome, EngineError> {
        self.observe_source(source, ParseGoal::Script, options, run)
    }

    /// Executes one Module while keeping ECMAScript rejection distinct from
    /// compiler, loader, host and backend failures. A pending entry after
    /// supported Wasm host work drains returns
    /// [`WasmExecutionFailureKind::IncompleteModuleEvaluation`] as an error;
    /// it never becomes an [`ObservedCompletion`].
    pub fn observe_module(
        &self,
        source: &str,
        options: CompileOptions,
        run: RunOptions,
    ) -> Result<ObservedRunOutcome, EngineError> {
        self.observe_source(source, ParseGoal::Module, options, run)
    }

    fn observe_source(
        &self,
        source: &str,
        goal: ParseGoal,
        options: CompileOptions,
        run: RunOptions,
    ) -> Result<ObservedRunOutcome, EngineError> {
        validate_execution_intl_profile(&options.intl_profile, run.backend)?;
        validate_execution_language_features(options.language_features, run.backend)?;
        validate_foreign_oracle(&options.foreign_modules, run.backend)?;
        match run.backend {
            ExecutionBackend::SpecExec => self.observe_with_spec_exec(
                source,
                options.filename.as_deref(),
                goal,
                options.module_loading_policy,
                options.module_prelude.as_deref(),
                run,
            ),
            ExecutionBackend::WasmAot => self.observe_source_with_cached_wasm(
                source,
                goal,
                options,
                run.timeout_ms,
                run.can_block,
            ),
        }
    }

    /// Runs a script through the Wasm-AOT backend on the calling thread.
    ///
    /// Callers must provide a thread with at least 64MiB of stack. This is
    /// for hosts that already own such a persistent worker; ordinary callers
    /// should use [`Engine::run_script`]. The persistent-worker path retains
    /// immutable runtime modules within the existing bounded module cache and
    /// bypasses per-case program retention. Every execution creates fresh
    /// instances and a fresh Store; the on-disk caches remain enabled.
    #[doc(hidden)]
    pub fn run_wasm_aot_script_on_current_thread(
        &self,
        source: &str,
        options: CompileOptions,
        timeout_ms: Option<u64>,
        can_block: bool,
    ) -> Result<RunOutcome, EngineError> {
        let _stack = EngineWorkerStackGuard::enter();
        self.run_source_with_cached_wasm_on_current_thread(
            source,
            ParseGoal::Script,
            options,
            timeout_ms,
            can_block,
            WasmModuleMemoryCachePolicy::RetainRuntime,
        )
    }

    #[doc(hidden)]
    pub fn run_wasm_aot_script_with_agents(
        &self,
        source: &str,
        options: CompileOptions,
        timeout_ms: Option<u64>,
        can_block: bool,
        agent_prelude: String,
    ) -> Result<RunOutcome, EngineError> {
        run_on_sized_stack(move || {
            self.run_wasm_aot_script_with_agents_on_current_thread(
                source,
                options,
                timeout_ms,
                can_block,
                agent_prelude,
            )
        })
    }

    /// The caller must provide at least 64MiB of stack, as for the script API.
    #[doc(hidden)]
    pub fn run_wasm_aot_script_with_agents_on_current_thread(
        &self,
        source: &str,
        options: CompileOptions,
        timeout_ms: Option<u64>,
        can_block: bool,
        agent_prelude: String,
    ) -> Result<RunOutcome, EngineError> {
        let _stack = EngineWorkerStackGuard::enter();
        let agent_compile_policy = WasmAgentCompilePolicy::from_root(&options);
        let cache = program_wasm_cache();
        let artifact = self.load_or_compile_program_wasm_on_current_thread(
            source,
            ParseGoal::Script,
            options.clone(),
            cache.clone(),
        )?;
        let agent_prelude: Arc<str> = Arc::from(agent_prelude);
        let agent_harness = || WasmAgentHarness {
            prelude: Arc::clone(&agent_prelude),
            compile_policy: agent_compile_policy.clone(),
        };
        let result = self.run_with_wasm_bytes_inner_with_agents(
            artifact.program(),
            timeout_ms,
            can_block,
            WasmModuleMemoryCachePolicy::BypassRetention,
            Some(agent_harness()),
            None,
        );
        let Err(error) = &result else {
            return result;
        };
        if !artifact.evict_if_invalid(error) {
            return result;
        }

        let artifact =
            self.compile_program_wasm_on_current_thread(source, ParseGoal::Script, options, cache)?;
        self.run_with_wasm_bytes_inner_with_agents(
            artifact.program(),
            timeout_ms,
            can_block,
            WasmModuleMemoryCachePolicy::BypassRetention,
            Some(agent_harness()),
            None,
        )
    }

    /// Runs a module through the Wasm-AOT backend on the calling thread.
    ///
    /// Callers must provide a thread with at least 64MiB of stack. This is
    /// for hosts that already own such a persistent worker; ordinary callers
    /// should use [`Engine::run_module`]. The persistent-worker path retains
    /// immutable runtime modules within the existing bounded module cache and
    /// bypasses per-case program retention. Every execution creates fresh
    /// instances and a fresh Store; the on-disk caches remain enabled.
    #[doc(hidden)]
    pub fn run_wasm_aot_module_on_current_thread(
        &self,
        source: &str,
        options: CompileOptions,
        timeout_ms: Option<u64>,
        can_block: bool,
    ) -> Result<RunOutcome, EngineError> {
        let _stack = EngineWorkerStackGuard::enter();
        self.run_source_with_cached_wasm_on_current_thread(
            source,
            ParseGoal::Module,
            options,
            timeout_ms,
            can_block,
            WasmModuleMemoryCachePolicy::RetainRuntime,
        )
    }

    fn run_source_with_cached_wasm(
        &self,
        source: &str,
        goal: ParseGoal,
        options: CompileOptions,
        timeout_ms: Option<u64>,
        can_block: bool,
    ) -> Result<RunOutcome, EngineError> {
        run_on_sized_stack(|| {
            self.execute_source_with_cached_wasm_on_current_thread(
                source,
                goal,
                options,
                timeout_ms,
                can_block,
                WasmModuleMemoryCachePolicy::Retain,
                &WasmExecutionMode::Legacy,
            )
            .and_then(WasmExecutionOutcome::into_legacy)
        })
    }

    fn observe_source_with_cached_wasm(
        &self,
        source: &str,
        goal: ParseGoal,
        options: CompileOptions,
        timeout_ms: Option<u64>,
        can_block: bool,
    ) -> Result<ObservedRunOutcome, EngineError> {
        run_on_sized_stack(|| {
            self.execute_source_with_cached_wasm_on_current_thread(
                source,
                goal,
                options,
                timeout_ms,
                can_block,
                WasmModuleMemoryCachePolicy::Retain,
                &WasmExecutionMode::Structured,
            )
            .and_then(WasmExecutionOutcome::into_structured)
        })
    }

    fn run_source_with_cached_wasm_on_current_thread(
        &self,
        source: &str,
        goal: ParseGoal,
        options: CompileOptions,
        timeout_ms: Option<u64>,
        can_block: bool,
        memory_cache_policy: WasmModuleMemoryCachePolicy,
    ) -> Result<RunOutcome, EngineError> {
        self.execute_source_with_cached_wasm_on_current_thread(
            source,
            goal,
            options,
            timeout_ms,
            can_block,
            memory_cache_policy,
            &WasmExecutionMode::Legacy,
        )
        .and_then(WasmExecutionOutcome::into_legacy)
    }

    fn execute_source_with_cached_wasm_on_current_thread(
        &self,
        source: &str,
        goal: ParseGoal,
        options: CompileOptions,
        timeout_ms: Option<u64>,
        can_block: bool,
        memory_cache_policy: WasmModuleMemoryCachePolicy,
        mode: &WasmExecutionMode,
    ) -> Result<WasmExecutionOutcome, EngineError> {
        let cache = program_wasm_cache();
        let artifact = self.load_or_compile_program_wasm_on_current_thread(
            source,
            goal,
            options.clone(),
            cache.clone(),
        )?;
        let result = self.execute_with_wasm_bytes_inner(
            artifact.program(),
            timeout_ms,
            can_block,
            memory_cache_policy,
            mode,
        );
        let Err(error) = &result else {
            return result;
        };
        if !artifact.evict_if_invalid(error) {
            return result;
        }

        let artifact = self.compile_program_wasm_on_current_thread(source, goal, options, cache)?;
        self.execute_with_wasm_bytes_inner(
            artifact.program(),
            timeout_ms,
            can_block,
            memory_cache_policy,
            mode,
        )
    }

    fn load_or_compile_program_wasm_on_current_thread(
        &self,
        source: &str,
        goal: ParseGoal,
        options: CompileOptions,
        cache: Option<Arc<cache::FunctionCache>>,
    ) -> Result<ProgramWasmArtifact, EngineError> {
        self.load_or_compile_program_wasm_for_role(
            source,
            goal,
            options,
            cache,
            module_loader::EmbeddedSourceRole::Entry,
        )
    }

    fn load_or_compile_program_wasm_for_role(
        &self,
        source: &str,
        goal: ParseGoal,
        options: CompileOptions,
        cache: Option<Arc<cache::FunctionCache>>,
        role: module_loader::EmbeddedSourceRole,
    ) -> Result<ProgramWasmArtifact, EngineError> {
        let parsed = self.parse_compilation_for_role(source, goal, &options, role)?;
        // A candidate is the exact graph just admitted by the current loader,
        // including dependency bytes, resolutions, options and compiler image.
        // Only successful *final* catalog preparation publishes an artifact.
        // If finite-source discovery extends this graph, its final digest is
        // different and cannot be reached through this candidate. A previously
        // closed static graph can therefore hit without rebuilding its IR.
        let candidate_key =
            program_cache_key_for_role(source, goal, &options, parsed.modules.graph(), role);
        if let (Some(cache), Some(key)) = (&cache, candidate_key) {
            if let Some(artifact) = Self::load_cached_program_wasm(cache, key, &options) {
                return Ok(artifact);
            }
        }

        let prepared = Self::lower_parsed_compilation(parsed, &options)?;
        let key =
            program_cache_key_for_role(source, goal, &options, prepared.modules.graph(), role);
        // A graph discovered by finite-source lowering gets its final digest;
        // a graph-free miss does not reread the same cache entry.
        if key != candidate_key {
            if let (Some(cache), Some(key)) = (&cache, key) {
                if let Some(artifact) = Self::load_cached_program_wasm(cache, key, &options) {
                    return Ok(artifact);
                }
            }
        }
        self.compile_program_wasm_with_key_on_current_thread(prepared, cache, key)
    }

    fn load_cached_program_wasm(
        cache: &Arc<cache::FunctionCache>,
        key: [u8; 32],
        options: &CompileOptions,
    ) -> Option<ProgramWasmArtifact> {
        let started = std::time::Instant::now();
        let admitted = cache.read(&key).and_then(|entry| {
            wasm_runtime_link::decode_cache_entry(&entry, &options.intl_profile, cache)
        });
        if let Some((bytes, runtime)) = admitted {
            let required = scriptw_aot_wasm::foreign_registry_from_module(&bytes).ok()?;
            if required
                .modules()
                .iter()
                .any(|module| options.foreign_modules.module(module.specifier()) != Some(module))
                || (!required.is_empty()
                    && required.error_tag() != options.foreign_modules.error_tag())
            {
                return None;
            }
            if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                eprintln!(
                    "scriptw wasm trace: program-cache hit: {} bytes in {:?}",
                    bytes.len(),
                    started.elapsed()
                );
            }
            Some(ProgramWasmArtifact {
                bytes,
                runtime,
                compilation_policy: WasmCompilationPolicy::from_options(options),
                cached_entry: Some(ProgramWasmCacheEntry {
                    cache: Arc::clone(cache),
                    key,
                }),
            })
        } else {
            if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                eprintln!(
                    "scriptw wasm trace: program-cache miss: {:?}",
                    started.elapsed()
                );
            }
            None
        }
    }

    fn compile_program_wasm_on_current_thread(
        &self,
        source: &str,
        goal: ParseGoal,
        options: CompileOptions,
        cache: Option<Arc<cache::FunctionCache>>,
    ) -> Result<ProgramWasmArtifact, EngineError> {
        let prepared = self.prepare_compilation(source, goal, &options)?;
        let key = program_cache_key(source, goal, &options, prepared.modules.graph());
        self.compile_program_wasm_with_key_on_current_thread(prepared, cache, key)
    }

    fn compile_program_wasm_with_key_on_current_thread(
        &self,
        prepared: PreparedCompilation,
        cache: Option<Arc<cache::FunctionCache>>,
        key: Option<[u8; 32]>,
    ) -> Result<ProgramWasmArtifact, EngineError> {
        let unit = self.compile_prepared_on_current_thread(prepared)?;
        let emit_started = std::time::Instant::now();
        let artifact = self.emit_wasm_with_cache_on_current_thread(
            &unit,
            cache.as_deref(),
            scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost,
        )?;
        if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
            eprintln!(
                "scriptw wasm trace: emit: {:?} ({} bytes)",
                emit_started.elapsed(),
                artifact.bytes.len()
            );
        }
        if let (Some(cache), Some(key)) = (cache, key) {
            let cache_started = std::time::Instant::now();
            let runtime = artifact.runtime.as_ref().map(|runtime| &runtime.0);
            if !cache.write(
                &key,
                wasm_runtime_link::encode_cache_entry(WasmProgramRef::new(
                    &artifact.bytes,
                    runtime,
                )),
            ) && std::env::var_os("SCRIPTW_WASM_TRACE").is_some()
            {
                eprintln!("scriptw wasm trace: program-cache write failed");
            } else if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                eprintln!(
                    "scriptw wasm trace: program-cache write: {:?}",
                    cache_started.elapsed()
                );
            }
        }
        Ok(ProgramWasmArtifact {
            bytes: Arc::from(artifact.bytes),
            runtime: artifact.runtime.map(|runtime| runtime.0),
            cached_entry: None,
            compilation_policy: WasmCompilationPolicy::from_unit(&unit),
        })
    }

    pub fn emit_wasm(&self, unit: &CompilationUnit) -> Result<Artifact, EngineError> {
        run_on_sized_stack(|| self.emit_wasm_on_current_thread(unit))
    }

    fn emit_wasm_on_current_thread(&self, unit: &CompilationUnit) -> Result<Artifact, EngineError> {
        let cache = program_wasm_cache();
        self.emit_wasm_with_cache_on_current_thread(
            unit,
            cache.as_deref(),
            scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable,
        )
    }

    fn emit_wasm_with_cache_on_current_thread(
        &self,
        unit: &CompilationUnit,
        cache: Option<&cache::FunctionCache>,
        source_compilation_profile: scriptw_aot_wasm::RuntimeSourceCompilationProfile,
    ) -> Result<Artifact, EngineError> {
        let runtime_cache = cache.and_then(wasm_runtime_link::RuntimeWasmCache::new);
        match scriptw_aot_wasm::emit_with_intl_profile_and_runtime_inputs(
            &unit.ir,
            unit.promise_rejection_policy,
            &unit.intl_profile,
            embedded_runtime::inputs(
                runtime_cache
                    .as_ref()
                    .map(|cache| cache as &dyn scriptw_aot_wasm::RuntimeArtifactCache),
                source_compilation_profile,
            ),
        ) {
            Ok(wasm) => {
                // `scriptw build wasm` and the Test262 wasm-aot backend both reach
                // emission through here and never through
                // `run_with_wasm_aot_inner`, so this is where
                // `SCRIPTW_WASM_TRACE_DUMP` has to be honoured for the variable
                // to mean what its name says.
                if std::env::var_os("SCRIPTW_WASM_TRACE_DUMP").is_some() {
                    eprintln!("scriptw wasm trace: artifact debug:\n{}", wasm.debug_dump);
                }
                Ok(Artifact {
                    runtime: wasm.runtime().cloned().map(LinkedRuntime),
                    compilation_policy: Some(WasmCompilationPolicy::from_unit(unit)),
                    bytes: wasm.bytes,
                    description: wasm.invariant_note.to_string(),
                    debug_dump: wasm.debug_dump,
                })
            }
            Err(err) => Err(EngineError::from_wasm_emit_error(&unit.ir, err)),
        }
    }

    pub fn inspect(&self, unit: &CompilationUnit) -> InspectionReport {
        InspectionReport {
            goal: unit.source.goal,
            source_len: unit.ir.source_len,
            stages: unit
                .ir
                .stages
                .iter()
                .map(|stage| match stage {
                    scriptw_ir::LoweringStage::ParsedSource => "parsed-source",
                    scriptw_ir::LoweringStage::ModuleGraphLoaded => "module-graph-loaded",
                    scriptw_ir::LoweringStage::ModuleGraphLinked => "module-graph-linked",
                    scriptw_ir::LoweringStage::ScriptIrBuilt => "script-ir-built",
                    scriptw_ir::LoweringStage::UnsupportedFeaturesRecorded => {
                        "unsupported-features-recorded"
                    }
                    scriptw_ir::LoweringStage::WasmReady => "wasm-ready",
                })
                .collect(),
            invariants: unit
                .ir
                .invariants
                .iter()
                .map(|invariant| invariant.as_str())
                .collect(),
            ir_summary: unit.ir.ir_summary(),
            diagnostics: unit
                .ir
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.clone())
                .collect(),
        }
    }

    fn compile_on_sized_stack(
        &self,
        source: &str,
        goal: ParseGoal,
        options: CompileOptions,
    ) -> Result<CompilationUnit, EngineError> {
        run_on_sized_stack(move || self.compile_on_current_thread(source, goal, options))
    }

    fn compile_on_current_thread(
        &self,
        source: &str,
        goal: ParseGoal,
        options: CompileOptions,
    ) -> Result<CompilationUnit, EngineError> {
        let prepared = self.prepare_compilation(source, goal, &options)?;
        self.compile_prepared_on_current_thread(prepared)
    }

    fn prepare_compilation(
        &self,
        source_text: &str,
        goal: ParseGoal,
        options: &CompileOptions,
    ) -> Result<PreparedCompilation, EngineError> {
        self.prepare_compilation_for_role(
            source_text,
            goal,
            options,
            module_loader::EmbeddedSourceRole::Entry,
        )
    }

    fn prepare_compilation_for_role(
        &self,
        source_text: &str,
        goal: ParseGoal,
        options: &CompileOptions,
        role: module_loader::EmbeddedSourceRole,
    ) -> Result<PreparedCompilation, EngineError> {
        let parsed = self.parse_compilation_for_role(source_text, goal, options, role)?;
        Self::lower_parsed_compilation(parsed, options)
    }

    fn parse_compilation_for_role(
        &self,
        source_text: &str,
        goal: ParseGoal,
        options: &CompileOptions,
        role: module_loader::EmbeddedSourceRole,
    ) -> Result<ParsedCompilation, EngineError> {
        let trace = std::env::var_os("SCRIPTW_WASM_TRACE").is_some();
        let parse_started = std::time::Instant::now();
        let (source, modules) = match &options.module_loading_policy {
            ModuleLoadingPolicy::Embedded(graph) => {
                let admitted = role
                    .parse_entry(graph, source_text, goal, options.filename.as_deref())
                    .map_err(|error| match error {
                        module_loader::EmbeddedEntryError::Authority(error) => {
                            EngineError::new(error.to_string())
                        }
                        module_loader::EmbeddedEntryError::Parse(error) => {
                            EngineError::from_parse_error(error)
                        }
                    })?;
                if trace {
                    eprintln!("scriptw wasm trace: parse: {:?}", parse_started.elapsed());
                }
                let (source, mut catalog) = admitted
                    .into_graph()
                    .map_err(|error| EngineError::new(error.to_string()))?;
                module_loader::include_foreign_module_registry(
                    &mut catalog,
                    &options.foreign_modules,
                )
                .map_err(|error| EngineError::new(error.to_string()))?;
                (
                    source,
                    PreparedModuleCompilation::Embedded {
                        owner: Arc::clone(graph),
                        graph: catalog,
                    },
                )
            }
            ModuleLoadingPolicy::Filesystem => parse_ambient_entry(
                source_text,
                goal,
                options,
                AmbientModuleLoaderPolicy::Filesystem,
            )?,
            ModuleLoadingPolicy::RejectAll => parse_ambient_entry(
                source_text,
                goal,
                options,
                AmbientModuleLoaderPolicy::RejectAll,
            )?,
        };
        let module_prelude = options
            .module_prelude
            .as_ref()
            .map(|prelude| {
                parse(prelude.as_str(), ParseOptions::script())
                    .map_err(|error| EngineError::from_parse_error(error).in_global_prelude())
                    .map(|parsed| match parsed {
                        ParsedSource::Script(script) => script,
                        ParsedSource::Module(_) => unreachable!("Script goal produces a Script"),
                    })
            })
            .transpose()?;
        Ok(ParsedCompilation {
            source,
            module_prelude,
            modules,
        })
    }

    fn lower_parsed_compilation(
        parsed: ParsedCompilation,
        options: &CompileOptions,
    ) -> Result<PreparedCompilation, EngineError> {
        #[cfg(test)]
        LOWERING_ATTEMPTS.with(|attempts| attempts.set(attempts.get() + 1));
        let ParsedCompilation {
            source,
            module_prelude,
            mut modules,
        } = parsed;
        let lower_started = std::time::Instant::now();
        let ir = prepared_source_catalog::prepare_source_catalog(
            &source,
            module_prelude.as_ref(),
            &mut modules,
            options,
        )?;
        if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
            eprintln!("scriptw wasm trace: lower: {:?}", lower_started.elapsed());
        }
        Ok(PreparedCompilation {
            source,
            module_prelude,
            modules,
            ir,
            promise_rejection_policy: options.promise_rejection_policy,
            intl_profile: options.intl_profile.clone(),
            host_surface_policy: options.host_surface_policy,
        })
    }

    fn compile_prepared_on_current_thread(
        &self,
        prepared: PreparedCompilation,
    ) -> Result<CompilationUnit, EngineError> {
        let PreparedCompilation {
            source,
            module_prelude,
            modules,
            ir,
            promise_rejection_policy,
            intl_profile,
            host_surface_policy,
        } = prepared;
        let module_loading_policy = match modules {
            PreparedModuleCompilation::Embedded { owner, .. } => {
                ModuleLoadingPolicy::Embedded(owner)
            }
            PreparedModuleCompilation::Ambient { policy, .. } => match policy {
                AmbientModuleLoaderPolicy::Filesystem => ModuleLoadingPolicy::Filesystem,
                AmbientModuleLoaderPolicy::RejectAll => ModuleLoadingPolicy::RejectAll,
            },
        };
        // Every *coded* diagnostic is a pre-evaluation rejection: `code()` is
        // `Some` exactly for `IrDiagnostic::rejected`, whose kind is derived by
        // `rejection_kind`. Matching on `kind == EarlyError` instead dropped the
        // `LinkError` half on the floor — a link failure then fell through to
        // `scriptw_aot_wasm::emit`, was flattened to `EmitError::unsupported`,
        // and arrived with `ir_diagnostic: None`, so the conformance matcher's
        // `IrDiagnosticPhase::Resolution` arm was unreachable. This is the same
        // predicate `scriptw-test262` uses (`diagnostic.code().is_some()`), and
        // the one that survives folding the code into `IrDiagnosticKind`.
        if let Some(diagnostic) = ir
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code().is_some())
        {
            return Err(EngineError::from_ir_diagnostic(diagnostic.clone()));
        }
        Ok(CompilationUnit {
            source: source.source().clone(),
            ir,
            module_prelude: module_prelude.map(|prelude| prelude.source_text.to_string()),
            module_loading_policy,
            promise_rejection_policy,
            intl_profile,
            host_surface_policy,
        })
    }

    pub fn run_compiled_unit(
        &self,
        unit: &CompilationUnit,
        source: &str,
        run: RunOptions,
    ) -> Result<RunOutcome, EngineError> {
        validate_execution_intl_profile(&unit.intl_profile, run.backend)?;
        validate_execution_language_features(unit.ir.language_features(), run.backend)?;
        if let Some(script) = &unit.ir.script {
            validate_foreign_oracle(&script.foreign_modules, run.backend)?;
        }
        match run.backend {
            ExecutionBackend::SpecExec => self.run_with_spec_exec(
                source,
                unit.source.filename.as_deref(),
                unit.source.goal,
                unit.module_loading_policy.clone(),
                unit.module_prelude.as_deref(),
                run,
            ),
            ExecutionBackend::WasmAot => {
                self.run_with_wasm_aot(unit, run.timeout_ms, run.can_block)
            }
        }
    }

    /// Developer-only differential oracle path (Boa interpreter). Never the
    /// product/default execution path — see `ExecutionBackend` docs. Real
    /// implementation only exists when the `spec-exec-oracle` cargo feature
    /// is enabled, keeping `boa_engine` out of product/release builds.
    #[cfg(feature = "spec-exec-oracle")]
    fn run_with_spec_exec(
        &self,
        source: &str,
        filename: Option<&str>,
        goal: ParseGoal,
        module_loading_policy: ModuleLoadingPolicy,
        module_prelude: Option<&str>,
        run: RunOptions,
    ) -> Result<RunOutcome, EngineError> {
        run_on_sized_stack(move || {
            let outcome = match goal {
                ParseGoal::Module => scriptw_spec_exec::execute_module(
                    source,
                    filename,
                    scriptw_spec_exec::ModuleHostConfig {
                        module_root: run.module_root.clone().map(Into::into),
                        test_path: run.test_path.clone().map(Into::into),
                        module_loading_policy,
                        prelude: module_prelude.map(str::to_owned),
                    },
                    &run.argv,
                    run.can_block,
                ),
                ParseGoal::Script => scriptw_spec_exec::execute_script_with_prelude(
                    source,
                    filename,
                    module_loading_policy,
                    module_prelude,
                    &run.argv,
                    run.can_block,
                ),
            }
            .map_err(EngineError::from_spec_exec)?;

            Ok(RunOutcome {
                backend_used: ExecutionBackend::SpecExec,
                note: outcome.note,
            })
        })
    }

    #[cfg(feature = "spec-exec-oracle")]
    fn observe_with_spec_exec(
        &self,
        source: &str,
        filename: Option<&str>,
        goal: ParseGoal,
        module_loading_policy: ModuleLoadingPolicy,
        module_prelude: Option<&str>,
        run: RunOptions,
    ) -> Result<ObservedRunOutcome, EngineError> {
        let host_hooks = self.realm.host_hooks();
        run_on_sized_stack(move || {
            let outcome = match goal {
                ParseGoal::Module => scriptw_spec_exec::observe_module(
                    source,
                    filename,
                    scriptw_spec_exec::ModuleHostConfig {
                        module_root: run.module_root.clone().map(Into::into),
                        test_path: run.test_path.clone().map(Into::into),
                        module_loading_policy,
                        prelude: module_prelude.map(str::to_owned),
                    },
                    &run.argv,
                    run.can_block,
                    host_hooks,
                ),
                ParseGoal::Script => scriptw_spec_exec::observe_script_with_prelude(
                    source,
                    filename,
                    module_loading_policy,
                    module_prelude,
                    &run.argv,
                    run.can_block,
                    host_hooks,
                ),
            }
            .map_err(EngineError::from_spec_exec)?;

            Ok(ObservedRunOutcome {
                backend_used: ExecutionBackend::SpecExec,
                completion: outcome.completion,
                output_events: outcome.output_events,
                note: outcome.note,
            })
        })
    }

    #[cfg(not(feature = "spec-exec-oracle"))]
    fn run_with_spec_exec(
        &self,
        _source: &str,
        _filename: Option<&str>,
        _goal: ParseGoal,
        _module_loading_policy: ModuleLoadingPolicy,
        _module_prelude: Option<&str>,
        _run: RunOptions,
    ) -> Result<RunOutcome, EngineError> {
        Err(EngineError::new(
            "spec-exec is a developer-only differential oracle backend and is not linked into \
             this build; rebuild with `--features spec-exec-oracle` (scriptw-engine/scriptw-cli) \
             to use it for differential testing only, never as the product execution backend",
        ))
    }

    #[cfg(not(feature = "spec-exec-oracle"))]
    fn observe_with_spec_exec(
        &self,
        _source: &str,
        _filename: Option<&str>,
        _goal: ParseGoal,
        _module_loading_policy: ModuleLoadingPolicy,
        _module_prelude: Option<&str>,
        _run: RunOptions,
    ) -> Result<ObservedRunOutcome, EngineError> {
        Err(EngineError::new(
            "spec-exec is a developer-only differential oracle backend and is not linked into \
             this build; rebuild with `--features spec-exec-oracle` (scriptw-engine/scriptw-cli) \
             to use it for differential testing only, never as the product execution backend",
        ))
    }

    fn run_with_wasm_aot(
        &self,
        unit: &CompilationUnit,
        timeout_ms: Option<u64>,
        can_block: bool,
    ) -> Result<RunOutcome, EngineError> {
        run_on_sized_stack(|| self.run_with_wasm_aot_inner(unit, timeout_ms, can_block))
    }

    fn run_with_wasm_aot_inner(
        &self,
        unit: &CompilationUnit,
        timeout_ms: Option<u64>,
        can_block: bool,
    ) -> Result<RunOutcome, EngineError> {
        let trace_wasm = std::env::var_os("SCRIPTW_WASM_TRACE").is_some();
        let trace_start = std::time::Instant::now();
        let trace_phase = |phase: &str, started: std::time::Instant| {
            if trace_wasm {
                eprintln!(
                    "scriptw wasm trace: {phase}: {:?} (total {:?})",
                    started.elapsed(),
                    trace_start.elapsed(),
                );
            }
        };

        let emit_started = std::time::Instant::now();
        let cache = program_wasm_cache();
        let runtime_cache = cache
            .as_deref()
            .and_then(wasm_runtime_link::RuntimeWasmCache::new);
        let artifact = scriptw_aot_wasm::emit_with_intl_profile_and_runtime_inputs(
            &unit.ir,
            unit.promise_rejection_policy,
            &unit.intl_profile,
            embedded_runtime::inputs(
                runtime_cache
                    .as_ref()
                    .map(|cache| cache as &dyn scriptw_aot_wasm::RuntimeArtifactCache),
                scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost,
            ),
        )
        .map_err(|err| EngineError::from_wasm_emit_error(&unit.ir, err))?;
        if std::env::var_os("SCRIPTW_WASM_TRACE_DUMP").is_some() {
            eprintln!(
                "scriptw wasm trace: artifact debug:\n{}",
                artifact.debug_dump
            );
        }
        trace_phase("emit", emit_started);
        let compilation_policy = WasmCompilationPolicy::from_unit(unit);
        self.run_with_wasm_bytes_inner(
            WasmProgramRef::new(&artifact.bytes, artifact.runtime())
                .with_compilation_policy(&compilation_policy),
            timeout_ms,
            can_block,
            WasmModuleMemoryCachePolicy::Retain,
        )
    }

    fn run_with_wasm_bytes_inner(
        &self,
        program: WasmProgramRef<'_>,
        timeout_ms: Option<u64>,
        can_block: bool,
        memory_cache_policy: WasmModuleMemoryCachePolicy,
    ) -> Result<RunOutcome, EngineError> {
        self.execute_with_wasm_bytes_inner(
            program,
            timeout_ms,
            can_block,
            memory_cache_policy,
            &WasmExecutionMode::Legacy,
        )
        .and_then(WasmExecutionOutcome::into_legacy)
    }

    fn execute_with_wasm_bytes_inner(
        &self,
        program: WasmProgramRef<'_>,
        timeout_ms: Option<u64>,
        can_block: bool,
        memory_cache_policy: WasmModuleMemoryCachePolicy,
        mode: &WasmExecutionMode,
    ) -> Result<WasmExecutionOutcome, EngineError> {
        self.execute_with_wasm_bytes_inner_with_agents(
            program,
            timeout_ms,
            can_block,
            memory_cache_policy,
            None,
            None,
            mode,
        )
    }

    fn run_with_wasm_bytes_inner_with_agents(
        &self,
        program: WasmProgramRef<'_>,
        timeout_ms: Option<u64>,
        can_block: bool,
        memory_cache_policy: WasmModuleMemoryCachePolicy,
        agent_harness: Option<WasmAgentHarness>,
        agent_execution: Option<WasmAgentExecution>,
    ) -> Result<RunOutcome, EngineError> {
        self.execute_with_wasm_bytes_inner_with_agents(
            program,
            timeout_ms,
            can_block,
            memory_cache_policy,
            agent_harness,
            agent_execution,
            &WasmExecutionMode::Legacy,
        )
        .and_then(WasmExecutionOutcome::into_legacy)
    }

    fn execute_with_wasm_bytes_inner_with_agents(
        &self,
        program: WasmProgramRef<'_>,
        timeout_ms: Option<u64>,
        can_block: bool,
        memory_cache_policy: WasmModuleMemoryCachePolicy,
        agent_harness: Option<WasmAgentHarness>,
        agent_execution: Option<WasmAgentExecution>,
        mode: &WasmExecutionMode,
    ) -> Result<WasmExecutionOutcome, EngineError> {
        self.execute_with_wasm_bytes_profiled(
            program,
            timeout_ms,
            can_block,
            memory_cache_policy,
            agent_harness,
            agent_execution,
            mode,
            None,
        )
    }

    fn execute_with_wasm_bytes_profiled(
        &self,
        program: WasmProgramRef<'_>,
        timeout_ms: Option<u64>,
        can_block: bool,
        memory_cache_policy: WasmModuleMemoryCachePolicy,
        agent_harness: Option<WasmAgentHarness>,
        agent_execution: Option<WasmAgentExecution>,
        mode: &WasmExecutionMode,
        profile: Option<&mut runtime_profile::RuntimeObservation>,
    ) -> Result<WasmExecutionOutcome, EngineError> {
        let trace_wasm = std::env::var_os("SCRIPTW_WASM_TRACE").is_some();
        let trace_start = std::time::Instant::now();
        // Only opt-in profiling owns a sampler. Its Drop also joins on every
        // admission/setup error before execution or on a host unwind.
        let process_memory_sampler = profile
            .as_ref()
            .map(|_| runtime_profile::ProcessMemorySampler::start());
        let trace_phase = |phase: &str, started: std::time::Instant| {
            if trace_wasm {
                eprintln!(
                    "scriptw wasm trace: {phase}: {:?} (total {:?})",
                    started.elapsed(),
                    trace_start.elapsed(),
                );
            }
        };
        if trace_wasm {
            eprintln!(
                "scriptw wasm trace: artifact bytes: {}",
                program.bytes.len()
            );
            if let Some(runtime) = program.runtime {
                eprintln!(
                    "scriptw wasm trace: runtime bytes: {}",
                    runtime.bytes().len()
                );
            }
            eprintln!(
                "scriptw wasm trace: runtime policy: {}",
                PRODUCT_WASMTIME_POLICY.report()
            );
        }

        // The custom-section identity is part of the cached Wasm bytes, so the
        // same gate covers fresh and cached artifacts. Validate while the
        // artifact is inert, before either Wasmtime compilation or instance
        // construction can run module start code.
        let admission_started = std::time::Instant::now();
        let intl_kernel = wasm_runtime_link::intl_kernel_for(program)?;
        if intl_kernel
            .identity()
            .is_some_and(|identity| !self.realm.system_time_zone().is_compatible_with(identity))
        {
            return Err(EngineError::new(
                "configured system time zone does not match the executing Intl provider",
            ));
        }
        let admission_elapsed = if profile.is_some() {
            admission_started.elapsed()
        } else {
            Default::default()
        };
        trace_phase("artifact + Intl admission", admission_started);
        let requested_options = profile.as_ref().and_then(|p| p.requested_options);
        let mut native_compilation_plan = wasm_runtime_link::plan_native_compilation(program);
        if let Some(options) = requested_options {
            native_compilation_plan.mode = options.native_mode().internal();
        }
        if trace_wasm {
            match native_compilation_plan.largest_code_body_bytes {
                Some(largest_code_body_bytes) => eprintln!(
                    "scriptw wasm trace: native compiler: {} (largest code body: {} bytes, size-optimized threshold: {} bytes)",
                    native_compilation_plan.mode.as_str(),
                    largest_code_body_bytes,
                    SIZE_OPTIMIZED_WASM_MIN_CODE_BODY_BYTES
                ),
                None => eprintln!(
                    "scriptw wasm trace: native compiler: {} (code-body scan could not parse artifact; Wasmtime validation remains authoritative)",
                    native_compilation_plan.mode.as_str()
                ),
            }
        }
        let engine_started = std::time::Instant::now();
        let mut engine = if let Some(agent_execution) = &agent_execution {
            agent_execution.group.engine.clone()
        } else if let Some(options) = requested_options {
            shared_profile_wasm_engine(options)?
        } else {
            shared_native_mode_wasm_engine(native_compilation_plan.mode)?
        };
        trace_phase("engine", engine_started);
        let engine_elapsed = if profile.is_some() {
            engine_started.elapsed()
        } else {
            Default::default()
        };
        ensure_wasm_epoch_ticker(&engine);
        let module_started = std::time::Instant::now();
        let function_cache_before = trace_wasm
            .then(cranelift_function_cache)
            .flatten()
            .map(|cache| cache.counters());
        let module_cache_before =
            wasmtime_module_cache().map(|cache| (cache.cache_hits(), cache.cache_misses()));
        // Agent workers share their group's engine, so their runtime module
        // must be compiled for the mode that engine was built for.
        let mut native_mode = agent_execution
            .as_ref()
            .map_or(native_compilation_plan.mode, |execution| {
                execution.group.native_compilation_mode
            });
        let runtime_image_policy = if requested_options.is_some() {
            WasmRuntimeImagePolicy::MatchedDiagnostic
        } else {
            WasmRuntimeImagePolicy::EmbeddedBuild
        };
        let compiled_module = wasm_runtime_link::compile_modules(
            &engine,
            program,
            memory_cache_policy,
            agent_execution.is_some(),
            native_mode,
            runtime_image_policy,
        );
        let modules = match compiled_module {
            Ok(compiled) => compiled,
            Err(error)
                if native_mode == WasmNativeCompilationMode::Fast
                    && requested_options.is_none()
                    && agent_execution.is_none()
                    && error.to_string().contains(WASM_CODE_TOO_LARGE_MESSAGE) =>
            {
                // This changes only the native compiler profile. Both engines
                // are constructed through the same required runtime policy,
                // so the retry cannot become a GC/feature capability fallback.
                if trace_wasm {
                    eprintln!(
                        "scriptw wasm trace: native compiler fallback: size-optimized after fast compiler exceeded its per-function limit"
                    );
                }
                engine = shared_size_optimized_wasm_engine()?;
                native_mode = WasmNativeCompilationMode::SizeOptimized;
                ensure_wasm_epoch_ticker(&engine);
                wasm_runtime_link::compile_modules(
                    &engine,
                    program,
                    memory_cache_policy,
                    true,
                    native_mode,
                    WasmRuntimeImagePolicy::EmbeddedBuild,
                )?
            }
            Err(error) => return Err(error),
        };
        // Read the final executing Engine, including an ordinary CodeTooLarge
        // retry's replacement. Requested options never supply observed policy.
        let actual_profile_options = if profile.is_some() {
            let observed = WasmRuntimeProfileOptions::observe_engine(&engine)?;
            if requested_options.is_some_and(|requested| requested != observed) {
                return Err(EngineError::new(format!(
                    "runtime profile requested {requested_options:?}, but the executing Engine has {observed:?}"
                )));
            }
            Some(observed)
        } else {
            None
        };
        let memory_cache_outcome = modules.memory_cache_outcome;
        let matched_runtime_image_cache = modules.matched_runtime_image_cache;
        let module_loading = modules.loading.clone();
        let module_elapsed = module_started.elapsed();
        if trace_wasm {
            eprintln!(
                "scriptw wasm trace: program-module-memory-cache {} during module loading: {:?}",
                match memory_cache_outcome {
                    WasmModuleMemoryCacheOutcome::Hit => "hit",
                    WasmModuleMemoryCacheOutcome::Miss => "miss",
                    WasmModuleMemoryCacheOutcome::Bypassed => "bypass",
                },
                module_elapsed
            );
            if let Some(before) = function_cache_before {
                if let Some(after) = cranelift_function_cache().map(|cache| cache.counters()) {
                    eprintln!(
                        "scriptw wasm trace: function-cache hits={} misses={} during {:?}",
                        after.0.saturating_sub(before.0),
                        after.1.saturating_sub(before.1),
                        module_elapsed
                    );
                }
            }
            let module_cache_after =
                wasmtime_module_cache().map(|cache| (cache.cache_hits(), cache.cache_misses()));
            match (module_cache_before, module_cache_after) {
                (Some(before), Some(after)) => eprintln!(
                    "scriptw wasm trace: module-cache hits={} misses={} during {:?}",
                    after.0.saturating_sub(before.0),
                    after.1.saturating_sub(before.1),
                    module_elapsed
                ),
                _ => eprintln!(
                    "scriptw wasm trace: module-cache result unavailable: {:?}",
                    module_elapsed
                ),
            }
        }
        trace_phase("module", module_started);
        let store_started = std::time::Instant::now();
        // Memory imports and the GC host imports live in the module that owns
        // the runtime: `R` when linked, the standalone module otherwise.
        let shared_memory_type = modules.host().imports().find_map(|import| {
            (import.module() == WASM_HOST_IMPORT_NAMESPACE
                && import.name() == WASM_HOST_IMPORT_SHARED_MEMORY)
                .then(|| match import.ty() {
                    WasmtimeExternType::Memory(memory_type) => Some(memory_type),
                    _ => None,
                })
                .flatten()
        });
        let shared_memory_backing = if let Some(agent_execution) = &agent_execution {
            Some(Arc::clone(&agent_execution.group.shared_memory_backing))
        } else {
            shared_memory_type
                .map(|memory_type| {
                    WasmtimeSharedMemory::new(&engine, memory_type)
                        .map(|memory| WasmSharedMemoryBacking::new(memory))
                        .map_err(|err| {
                            EngineError::new(format!("wasmtime shared-memory setup failed: {err}"))
                        })
                })
                .transpose()?
        };
        // This same authority is armed at the original execution-start point.
        // Agent workers inherit it; native source compilation cannot reset it.
        let execution_control = agent_execution
            .as_ref()
            .map(|execution| Arc::clone(&execution.group.execution_control))
            .unwrap_or_else(|| Arc::new(WasmAgentExecutionControl::new()));
        let root_agent_group = match (agent_harness, shared_memory_backing.as_ref()) {
            (Some(agent_harness), Some(shared_memory_backing)) => Some(Arc::new(WasmAgentGroup {
                engine: engine.clone(),
                native_compilation_mode: native_mode,
                realm: self.realm.clone(),
                shared_memory_backing: Arc::clone(shared_memory_backing),
                prelude: agent_harness.prelude,
                compile_policy: agent_harness.compile_policy,
                timeout_ms,
                execution_control: Arc::clone(&execution_control),
                started_at: self.realm.host_clock().monotonic_instant(),
                reports: Mutex::new(VecDeque::new()),
                worker_artifacts: Mutex::new(HashMap::new()),
                workers: Mutex::new(Vec::new()),
            })),
            (Some(_), None) => {
                return Err(EngineError::new(
                    "Test262 agent execution requires an imported shared memory",
                ));
            }
            (None, _) => None,
        };
        let agent_group = agent_execution
            .as_ref()
            .map(|execution| Arc::clone(&execution.group))
            .or_else(|| root_agent_group.as_ref().map(Arc::clone));
        let mut store = WasmtimeStore::new(
            &engine,
            WasmHostState {
                user_data: None,
                embedding: None,
                weak: wasm_gc_weak_host::WeakGcState::default(),
                native_source_compiler: None,
                native_template_instances: Default::default(),
                realm: self.realm.clone(),
                output_events: WasmOutputEvents::for_mode(mode),
                intl_kernel,
                can_block,
                monotonic_clock_origin: self.realm.host_clock().monotonic_instant(),
                async_waiters: WasmStoreAsyncWaiters::new(
                    shared_memory_backing.as_ref().map(Arc::clone),
                ),
                shared_memory_backing,
                agent_group,
                agent_commands: agent_execution
                    .as_ref()
                    .map(|execution| Arc::clone(&execution.commands)),
                agent_leaving: agent_execution
                    .as_ref()
                    .map(|execution| Arc::clone(&execution.leaving)),
                limits: WasmtimeStoreLimitsBuilder::new()
                    .memory_size(WASM_STORE_MEMORY_CAP_BYTES)
                    .build(),
            },
        );
        store.limiter(|state| &mut state.limits);
        // The engine has epoch interruption enabled process-wide (see
        // `shared_wasm_engine`); every store must set an explicit deadline or
        // it traps immediately (deadline defaults to epoch 0, which has
        // already "elapsed"). Instantiation and export lookup are setup, not
        // user-program execution, so keep them outside the execution bound.
        store.set_epoch_deadline(u64::MAX / 2);
        let host_imports_started = std::time::Instant::now();
        let host_source = match program.runtime {
            Some(runtime) => WasmModuleSource::runtime(runtime, runtime_image_policy),
            None => WasmModuleSource::Program(program.bytes),
        };
        let host_key = wasm_module_memory_cache_key(host_source, native_mode);
        let (mut linker, host_import_cache) =
            host_imports::linker(&engine, modules.host(), host_key)?;
        let host_imports_elapsed = if profile.is_some() {
            host_imports_started.elapsed()
        } else {
            Default::default()
        };
        trace_phase(
            "host import registration (within store + linker)",
            host_imports_started,
        );
        if let Some(private_memory_type) = modules.host().imports().find_map(|import| {
            (import.module() == WASM_HOST_IMPORT_NAMESPACE
                && import.name() == WASM_HOST_IMPORT_PRIVATE_MEMORY)
                .then(|| match import.ty() {
                    WasmtimeExternType::Memory(memory_type) => Some(memory_type),
                    _ => None,
                })
                .flatten()
        }) {
            let private_memory =
                WasmtimeMemory::new(&mut store, private_memory_type).map_err(|err| {
                    EngineError::new(format!("wasmtime private-memory setup failed: {err}"))
                })?;
            linker
                .define(
                    &store,
                    WASM_HOST_IMPORT_NAMESPACE,
                    WASM_HOST_IMPORT_PRIVATE_MEMORY,
                    private_memory,
                )
                .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
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
                .map_err(|err| EngineError::new(format!("wasmtime linker setup failed: {err}")))?;
        }
        trace_phase("store + linker", store_started);
        let store_elapsed = if profile.is_some() {
            store_started.elapsed()
        } else {
            Default::default()
        };
        let instantiate_started = std::time::Instant::now();
        // `observed_instance` owns the diagnostic globals and snapshot roots:
        // the runtime instance when linked. The program instance exports the
        // module status of a module entry.
        let (instance, observed_instance) =
            wasm_runtime_link::instantiate(&mut linker, &mut store, &modules)?;
        trace_phase("instantiate", instantiate_started);
        let instantiate_elapsed = if profile.is_some() {
            instantiate_started.elapsed()
        } else {
            Default::default()
        };
        let lookup_started = std::time::Instant::now();
        let main = instance
            .get_typed_func::<(), GcMainCompletion>(&mut store, "main")
            .map_err(|err| EngineError::new(format!("wasmtime export lookup failed: {err}")))?;
        trace_phase("lookup main", lookup_started);
        let lookup_elapsed = if profile.is_some() {
            lookup_started.elapsed()
        } else {
            Default::default()
        };
        let execution_started = std::time::Instant::now();
        if agent_execution.is_none() {
            execution_control.arm(execution_started, timeout_ms);
        }
        if let (Some(runtime), Some(policy)) = (program.runtime, program.compilation_policy) {
            if runtime.source_compilation_profile()
                == scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost
            {
                let context = wasm_native_source_compiler::NativeSourceCompilerContext::new(
                    &linker,
                    observed_instance,
                    Arc::clone(runtime),
                    policy.clone(),
                    Arc::clone(&execution_control),
                    native_mode,
                )?;
                #[cfg(test)]
                let context = context.with_test_observers(
                    self.native_function_compilation_observer.clone(),
                    self.native_global_script_compilation_observer.clone(),
                    self.native_direct_eval_compilation_observer.clone(),
                );
                store.data_mut().native_source_compiler = Some(Arc::new(context));
            }
        }
        let execution_budget = execution_control.remaining()?;
        let epoch_deadline_ticks = match execution_budget {
            // The ticker's phase is independent of this Store. One guard
            // tick ensures the deadline cannot expire before the requested
            // wall time when arming happens just before a process-wide tick.
            Some(remaining) => {
                (remaining.as_millis() as u64)
                    .div_ceil(WASM_EPOCH_TICK_MS)
                    .max(1)
                    + 1
            }
            None => u64::MAX / 2,
        };
        store.set_epoch_deadline(epoch_deadline_ticks);
        if let Some(agent_execution) = &agent_execution {
            agent_execution.ready.send(()).map_err(|_| {
                EngineError::new("Test262 agent owner stopped before the agent became ready")
            })?;
        }
        let gc_heap_before = profile.as_ref().map(|_| store.gc_heap_capacity());
        let mut roots = wasmtime::RootScope::new(&mut store);
        let execution_result = main.call(&mut roots, ()).map_err(|err| {
            if let Some(rejection) = err.downcast_ref::<RuntimeSemanticRejection>() {
                EngineError::from_runtime_semantic_rejection(*rejection)
            } else if let Some(cause) = err.downcast_ref::<EngineError>() {
                let mut error = cause.clone();
                error.message = format!("wasmtime host execution failed: {err:#}");
                error
            } else if is_wasm_epoch_interrupt(&err) {
                // Distinguish this from other traps with the same "timeout
                // exceeded" phrasing the child-process/elapsed-time timeout
                // path already uses (see
                // `scriptw_test262::run_one_case_in_child_process` and
                // `run_one_case`), so both paths classify identically as
                // timeouts downstream (FailureKind::Runtime,
                // OutcomeKind timeout bucketing, `RunSummary::timeouts`).
                EngineError::from_execution_failure(
                    EngineExecutionFailure::Timeout,
                    format!(
                        "timeout exceeded after {}ms (wasm epoch interrupt, bound {}ms)",
                        execution_started.elapsed().as_millis(),
                        timeout_ms.unwrap_or(0)
                    ),
                )
            } else {
                EngineError::from_execution_failure(
                    EngineExecutionFailure::Trap,
                    format!("wasmtime execution trapped: {err:?}"),
                )
            }
        });
        let global_prelude_failed = global_script_prelude::failed(&instance, &mut roots)?;
        let execution_result = execution_result.map_err(|error| {
            if global_prelude_failed {
                error.in_global_prelude()
            } else {
                error
            }
        });
        let agent_result = root_agent_group
            .as_ref()
            .map_or(Ok(()), |group| group.finish());
        let execution_result = execution_result.and_then(|completion| {
            if global_prelude_failed {
                return Err(global_script_prelude::completion_error(&observed_instance, &mut roots, completion)?);
            }
            trace_phase("execution", execution_started);
            let module_status = read_module_entry_status(&instance, &mut roots)?;
            match module_status {
                Some(WasmModuleEvaluationStatus::Pending) => {
                    return Err(EngineError::from_execution_failure(
                        EngineExecutionFailure::IncompleteModuleEvaluation,
                        "module entry evaluation remained pending after supported host work became quiescent",
                    ));
                }
                Some(WasmModuleEvaluationStatus::NotStarted) => {
                    return Err(EngineError::from_execution_failure(
                        EngineExecutionFailure::Trap,
                        "Module entry returned without reaching its completion checkpoint",
                    ));
                }
                Some(WasmModuleEvaluationStatus::Settled) | None => {}
            }
            if let WasmExecutionMode::Graph(limits) = mode {
                let completion = wasm_rooted_snapshot::observe(&mut roots, &observed_instance, completion, *limits)?;
                return Ok(WasmExecutionOutcome::Graph(GraphRunOutcome {
                    backend_used: ExecutionBackend::WasmAot, completion,
                    output_events: roots.as_context().data().output_events.take(),
                    note: "wasm-aot rooted completion snapshot after quiescence".into(),
                }));
            }
            let (result_tag, completion_kind, value) =
                wasm_gc_completion::observe(&mut roots, completion)?.into_parts();
            let result_kind = result_tag.value_kind();
            match mode {
                WasmExecutionMode::Legacy => {
                    // Diagnostic GC globals are read only for human throw text.
                    // Structured Object/Symbol observations remain category-only;
                    // no backend address escapes the rooted completion boundary.
                    let thrown_error = match &completion_kind {
                        WasmTopLevelCompletionKind::Normal => ThrownErrorText::NONE,
                        WasmTopLevelCompletionKind::Throw => {
                            ThrownErrorText::read(&observed_instance, &mut roots, result_kind)?
                        }
                    };
                    let note = wasm_gc_completion::render_value(
                        result_tag,
                        &value,
                        thrown_error.message(),
                    );
                    match &completion_kind {
                        WasmTopLevelCompletionKind::Normal => {
                            Ok(WasmExecutionOutcome::Legacy(RunOutcome {
                                backend_used: ExecutionBackend::WasmAot,
                                note,
                            }))
                        }
                        WasmTopLevelCompletionKind::Throw => {
                            let prefix = thrown_error.name_prefix();
                            Err(EngineError::from_execution_failure(
                                EngineExecutionFailure::JavaScriptException {
                                    constructor_name: thrown_error.constructor_name,
                                },
                                format!("uncaught throw: {prefix}{note}"),
                            ))
                        }
                    }
                }
                WasmExecutionMode::Structured => {
                    let completion = match completion_kind {
                        WasmTopLevelCompletionKind::Normal => ObservedCompletion::Normal(value),
                        WasmTopLevelCompletionKind::Throw => ObservedCompletion::Throw(value),
                    };
                    let note = match &completion {
                        ObservedCompletion::Normal(value) => {
                            format!("wasm-aot normal completion ({})", value.type_name())
                        }
                        ObservedCompletion::Throw(value) => {
                            format!("uncaught ECMAScript throw ({})", value.type_name())
                        }
                    };
                    Ok(WasmExecutionOutcome::Structured(ObservedRunOutcome {
                        backend_used: ExecutionBackend::WasmAot,
                        completion,
                        output_events: roots.as_context().data().output_events.take(),
                        note,
                    }))
                }
                WasmExecutionMode::Graph(_) => unreachable!("graph mode returned before scalar decoding"),
            }
        });
        let result = finish_wasm_execution(execution_result, agent_result);
        let execution_elapsed = if profile.is_some() {
            execution_started.elapsed()
        } else {
            Default::default()
        };
        drop(roots);
        if let Some(profile) = profile {
            let actual_options = actual_profile_options
                .expect("profile requested its final Engine configuration observation");
            let process_memory = process_memory_sampler
                .expect("profile requested its invocation sampler")
                .finish();
            *profile = runtime_profile::RuntimeObservation {
                requested_options,
                actual_native_mode: Some(actual_options.native_mode()),
                actual_options: Some(actual_options),
                host_import_cache: Some(host_import_cache),
                matched_runtime_image_cache,
                timings: WasmRuntimeTimings {
                    admission: admission_elapsed,
                    engine: engine_elapsed,
                    module: module_elapsed,
                    store_and_linker: store_elapsed,
                    host_import_registration: host_imports_elapsed,
                    instantiate: instantiate_elapsed,
                    export_lookup: lookup_elapsed,
                    execution_and_completion: execution_elapsed,
                    total: trace_start.elapsed(),
                },
                module_cache: memory_cache_outcome.into(),
                module_loading,
                heap: WasmRuntimeHeapObservation {
                    capacity_before_execution_bytes: gc_heap_before
                        .expect("profile requested its first snapshot"),
                    capacity_after_execution_bytes: store.gc_heap_capacity(),
                },
                process_memory: Some(process_memory),
            };
        }
        result
    }
}

fn read_module_entry_status(
    instance: &wasmtime::Instance,
    store: &mut impl wasmtime::AsContextMut<Data = WasmHostState>,
) -> Result<Option<WasmModuleEvaluationStatus>, EngineError> {
    let Some(exported) = instance.get_export(&mut *store, MODULE_EVALUATION_STATUS_EXPORT) else {
        return Ok(None);
    };
    let WasmtimeExtern::Global(exported) = exported else {
        return Err(EngineError::from_execution_failure(
            EngineExecutionFailure::Trap,
            "Module evaluation status export must be a global",
        ));
    };
    let WasmtimeVal::I64(word) = exported.get(&mut *store) else {
        return Err(EngineError::from_execution_failure(
            EngineExecutionFailure::Trap,
            "Module evaluation status export must be i64",
        ));
    };
    WasmModuleEvaluationStatus::from_word(word)
        .map(Some)
        .ok_or_else(|| {
            EngineError::from_execution_failure(
                EngineExecutionFailure::Trap,
                format!("invalid Module evaluation status word {word}"),
            )
        })
}

enum WasmTopLevelCompletionKind {
    Normal,
    Throw,
}

fn wasm_memory_span(
    offset: usize,
    len: usize,
    memory_byte_len: usize,
    label: &str,
) -> Result<core::ops::Range<usize>, EngineError> {
    let end = offset
        .checked_add(len)
        .filter(|end| *end <= memory_byte_len)
        .ok_or_else(|| EngineError::new(format!("{label} points outside exported memory")))?;
    Ok(offset..end)
}

fn read_wasmtime_shared_memory(
    memory: &wasmtime::SharedMemory,
    offset: usize,
    bytes: &mut [u8],
) -> Result<(), EngineError> {
    let end = offset
        .checked_add(bytes.len())
        .filter(|end| *end <= memory.data_size())
        .ok_or_else(|| EngineError::new("failed to read wasm memory: out of bounds"))?;
    for (source, destination) in memory.data()[offset..end].iter().zip(bytes) {
        // Wasmtime exposes shared bytes as UnsafeCell and requires host reads
        // to use atomics because another Wasm thread may modify them.
        *destination = unsafe { AtomicU8::from_ptr(source.get()) }.load(Ordering::Relaxed);
    }
    Ok(())
}

/// What an uncaught throw carried out of the module, as the module itself
/// reported it: the error's diagnostic `name`, `message`, and final
/// `constructor.name` for the conformance consumer.
///
/// Diagnostic text is published at throw sites; the constructor name is
/// captured from the final root completion after jobs and finalizers. These
/// globals are read together only for a thrown heap object: a heap-less module
/// aliases their exports onto one slot and cannot produce an object completion.
struct ThrownErrorText {
    name: Option<String>,
    message: Option<String>,
    constructor_name: Option<String>,
}

impl ThrownErrorText {
    const NONE: Self = Self {
        name: None,
        message: None,
        constructor_name: None,
    };

    fn read(
        instance: &wasmtime::Instance,
        store: &mut impl wasmtime::AsContextMut<Data = WasmHostState>,
        result_kind: ValueKind,
    ) -> Result<Self, EngineError> {
        if !matches!(
            result_kind,
            ValueKind::Object | ValueKind::Array | ValueKind::Function | ValueKind::Arguments
        ) {
            return Ok(Self::NONE);
        }
        let name = wasm_gc_completion::diagnostic_string(
            instance,
            store,
            wasm_gc_completion::DiagnosticString::ErrorName,
        )?;
        let message = wasm_gc_completion::diagnostic_string(
            instance,
            store,
            wasm_gc_completion::DiagnosticString::ErrorMessage,
        )?;
        let constructor_name = wasm_gc_completion::diagnostic_string(
            instance,
            store,
            wasm_gc_completion::DiagnosticString::ErrorConstructorName,
        )?;
        Ok(Self {
            name: name.filter(|value| !value.is_empty()),
            message: message.filter(|value| !value.is_empty()),
            constructor_name,
        })
    }

    fn name_prefix(&self) -> String {
        self.name
            .as_ref()
            .map(|name| format!("{name}: "))
            .unwrap_or_default()
    }

    fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod regexp_program_boundary_tests;

#[cfg(test)]
mod runtime_regexp_compiler_tests;

#[cfg(test)]
mod array_present_index_tests;

#[cfg(test)]
mod module_async_runtime_tests;
