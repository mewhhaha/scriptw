use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use wasmparser::{Import, TypeRef};
use wtime_runtime::{
    Caller, Engine, Error as RuntimeError, ErrorKind, External, HeapType, Instance, Limits, Linker,
    Memory, MemoryType, Module, RefValue, RunOutcome, Store, Value,
};

use crate::Error;

const HOST: &str = "scriptw_host";
const RUNTIME: &str = "scriptw_runtime";
const SHARED_MEMORY: &str = "shared_memory";
const PRIVATE_MEMORY: &str = "private_memory";
const MAX_PAGES: u64 = 16_384;

pub(crate) struct MemoryImports {
    shared: Option<MemoryType>,
    program_uses_runtime_memory: bool,
}

impl MemoryImports {
    pub(crate) fn inspect(program: &[Import<'_>], runtime: &[Import<'_>]) -> Result<Self, Error> {
        let mut shared = None;
        for import in runtime.iter().chain(program) {
            if (import.module, import.name) != (HOST, SHARED_MEMORY) {
                continue;
            }
            let TypeRef::Memory(ty) = import.ty else {
                return Err("scriptw_host.shared_memory must import a memory".into());
            };
            let Some(maximum) = ty.maximum else {
                return Err("scriptw_host.shared_memory requires a maximum".into());
            };
            if !ty.shared || ty.memory64 || ty.page_size_log2.unwrap_or(16) != 16 {
                return Err(
                    "scriptw_host.shared_memory requires shared 32-bit 64 KiB pages".into(),
                );
            }
            let limits = shared.get_or_insert(Limits::new(1, Some(MAX_PAGES)));
            limits.min = limits.min.max(ty.initial);
            limits.max = Some(limits.max.unwrap_or(MAX_PAGES).min(maximum));
            if !limits.is_valid() {
                return Err("scriptw_host.shared_memory cannot fit within 1–16384 pages".into());
            }
        }
        Ok(Self {
            shared: shared.map(|limits| MemoryType {
                limits,
                shared: true,
                ..MemoryType::default()
            }),
            program_uses_runtime_memory: program.iter().any(|import| {
                (import.module, import.name) == (HOST, PRIVATE_MEMORY)
                    && matches!(import.ty, TypeRef::Memory(_))
            }),
        })
    }
}

struct HostState {
    output: Arc<Mutex<Vec<u8>>>,
    clock_origin: Instant,
    random: RandomState,
    random_index: AtomicU64,
}

pub(crate) fn instantiate(
    engine: &Engine,
    store: &mut Store,
    program: &Module,
    runtime: Option<&Module>,
    memory_imports: &MemoryImports,
    output: Arc<Mutex<Vec<u8>>>,
) -> Result<Instance, Error> {
    let state = Arc::new(HostState {
        output,
        clock_origin: Instant::now(),
        random: RandomState::new(),
        random_index: AtomicU64::new(0),
    });
    let shared = memory_imports
        .shared
        .map(|ty| store.create_memory(ty))
        .transpose()?;

    let mut program_linker = Linker::new(engine);
    let program_memory = Arc::new(OnceLock::new());
    bind_shared_memory(&mut program_linker, shared)?;
    bind_host_functions(&mut program_linker, store, program, &state, &program_memory)?;
    if let Some(runtime) = runtime {
        let mut runtime_linker = Linker::new(engine);
        let runtime_memory = Arc::new(OnceLock::new());
        bind_shared_memory(&mut runtime_linker, shared)?;
        bind_host_functions(&mut runtime_linker, store, runtime, &state, &runtime_memory)?;

        let instance = runtime_linker
            .instantiate(store, runtime)
            .map_err(|error| format!("instantiate Scriptw runtime: {error}"))?;
        let memory = export_memory(store, instance)?;
        runtime_memory
            .set(memory)
            .map_err(|_| "Scriptw runtime memory was already bound")?;
        if memory_imports.program_uses_runtime_memory {
            program_linker.define(HOST, PRIVATE_MEMORY, External::Memory(memory))?;
            program_memory
                .set(memory)
                .map_err(|_| "Scriptw program memory was already bound")?;
        }
        for (name, export) in instance.exports(store)? {
            program_linker.define(RUNTIME, &name, export)?;
        }
    }

    let instance = program_linker
        .instantiate(store, program)
        .map_err(|error| format!("instantiate Scriptw program: {error}"))?;
    match instance.export(store, "memory") {
        Ok(External::Memory(memory)) => {
            if let Some(bound) = program_memory.get() {
                if *bound != memory {
                    return Err(
                        "Scriptw program memory must re-export scriptw_host.private_memory".into(),
                    );
                }
            } else {
                program_memory
                    .set(memory)
                    .map_err(|_| "Scriptw program memory was already bound")?;
            }
        }
        _ if runtime.is_some() || memory_imports.program_uses_runtime_memory => {
            return Err("Scriptw linked program requires a linear memory export named 'memory'".into());
        }
        _ => {}
    }
    Ok(instance)
}

fn bind_shared_memory(linker: &mut Linker, memory: Option<Memory>) -> Result<(), RuntimeError> {
    if let Some(memory) = memory {
        linker.define(HOST, SHARED_MEMORY, External::Memory(memory))?;
    }
    Ok(())
}

fn export_memory(store: &Store, instance: Instance) -> Result<Memory, Error> {
    match instance.export(store, "memory") {
        Ok(External::Memory(memory)) => Ok(memory),
        _ => Err("Scriptw instance requires a linear memory export named 'memory'".into()),
    }
}

fn bind_host_functions(
    linker: &mut Linker,
    store: &mut Store,
    module: &Module,
    state: &Arc<HostState>,
    memory: &Arc<OnceLock<Memory>>,
) -> Result<(), RuntimeError> {
    let mut imports = std::collections::HashSet::new();
    for (namespace, name, signature, _) in module.function_imports() {
        if namespace != HOST || !imports.insert(name) {
            continue;
        }
        let name = name.to_owned();
        let state = Arc::clone(state);
        let memory = Arc::clone(memory);
        let callback_name = name.clone();
        // Bind even unsupported imports with their original concrete GC types.
        // Only fail if the guest calls a host operation we cannot perform.
        let func = store.create_module_host_func(module, signature, move |caller, args| {
            call_host(&callback_name, caller, args, &state, &memory).map(RunOutcome::Returned)
        })?;
        linker.define(HOST, &name, External::Func(func))?;
    }
    Ok(())
}

fn call_host(
    name: &str,
    caller: &mut Caller<'_>,
    args: &[Value],
    state: &HostState,
    memory: &OnceLock<Memory>,
) -> wtime_runtime::Result<Vec<Value>> {
    let host_error =
        |detail: &str| RuntimeError::new(ErrorKind::Host, format!("{HOST}.{name}: {detail}"));
    let result = match (name, args) {
        ("print_line_utf8", [Value::I32(ptr), Value::I32(len)]) => {
            let memory = *memory
                .get()
                .ok_or_else(|| host_error("exported memory is unavailable"))?;
            let ptr = u64::try_from(*ptr).map_err(|_| host_error("negative pointer"))?;
            let len = usize::try_from(*len).map_err(|_| host_error("negative length"))?;
            let bytes = caller
                .store()
                .read_memory(memory, ptr, len)
                .map_err(|error| host_error(&error.to_string()))?;
            std::str::from_utf8(&bytes).map_err(|error| host_error(&error.to_string()))?;
            let mut output = state
                .output
                .lock()
                .map_err(|_| host_error("output lock poisoned"))?;
            const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
            if output.len().saturating_add(bytes.len()).saturating_add(1) > MAX_OUTPUT_BYTES {
                return Err(host_error("captured output exceeds 16 MiB bound"));
            }
            output.extend_from_slice(&bytes);
            output.push(b'\n');
            return Ok(Vec::new());
        }
        ("agent_can_suspend", []) => Value::I32(0),
        ("wall_clock_millis", []) => {
            f64_value(match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(duration) => duration.as_millis() as f64,
                Err(error) => -(error.duration().as_millis() as f64),
            })
        }
        ("monotonic_clock_nanos", []) => {
            Value::I64(i64::try_from(state.clock_origin.elapsed().as_nanos()).unwrap_or(i64::MAX))
        }
        ("sleep_nanos", [Value::I64(nanos)]) => {
            if let Ok(nanos) = u64::try_from(*nanos) {
                // Match Scriptw's clock polling sleep in the product host.
                std::thread::sleep(Duration::from_nanos(nanos.min(1_000_000)));
            }
            return Ok(Vec::new());
        }
        ("agent_call", [Value::I64(operation), Value::I64(first), Value::I64(_)]) => {
            Value::I64(agent_call(*operation, *first, state.clock_origin)?)
        }
        ("reject_runtime_semantics", [Value::I64(operation)]) => {
            return Err(host_error(&format!(
                "guest rejected runtime semantics (operation {operation})"
            )));
        }
        ("random_f64", []) => {
            let counter = state.random_index.fetch_add(1, Ordering::Relaxed);
            let bits = state.random.hash_one(counter) >> 11;
            f64_value(bits as f64 / (1u64 << 53) as f64)
        }
        ("collect_gc", []) => {
            caller
                .collect()
                .map_err(|error| host_error(&error.to_string()))?;
            return Ok(Vec::new());
        }
        // Weak resources are unsupported, so the host's weak job is empty.
        ("weak_job_finish", []) => return Ok(Vec::new()),
        ("finalization_next_registry", []) => Value::Ref(RefValue::Null(HeapType::Eq)),
        ("number_pow", [Value::F64(base), Value::F64(exponent)]) => {
            f64_value(number_pow(f64::from_bits(*base), f64::from_bits(*exponent)))
        }
        ("math_atan2", [Value::F64(y), Value::F64(x)]) => {
            f64_value(f64::from_bits(*y).atan2(f64::from_bits(*x)))
        }
        (_, [Value::F64(value)]) => {
            let math = unary_math(name).ok_or_else(|| host_error("unsupported host callback"))?;
            f64_value(math(f64::from_bits(*value)))
        }
        _ => {
            return Err(host_error(
                "unsupported host callback or incompatible operands",
            ));
        }
    };
    Ok(vec![result])
}

fn agent_call(operation: i64, first: i64, origin: Instant) -> Result<i64, RuntimeError> {
    // Discriminants are Scriptw's AgentHostOperation wire ABI.
    const REPORT_LENGTH: i64 = 5;
    const SLEEP: i64 = 7;
    const MONOTONIC_NOW: i64 = 8;
    const LEAVING: i64 = 9;
    const POLL_ASYNC_WAITER: i64 = 11;
    const CANCEL_ASYNC_WAITER: i64 = 13;
    match operation {
        REPORT_LENGTH | POLL_ASYNC_WAITER | CANCEL_ASYNC_WAITER => Ok(-1),
        SLEEP => {
            let milliseconds = f64::from_bits(first as u64);
            if milliseconds.is_finite() && milliseconds > 0.0 {
                let duration =
                    Duration::try_from_secs_f64(milliseconds / 1000.0).map_err(|error| {
                        RuntimeError::new(
                            ErrorKind::Host,
                            format!("{HOST}.agent_call sleep: {error}"),
                        )
                    })?;
                std::thread::sleep(duration);
            }
            Ok(0)
        }
        MONOTONIC_NOW => Ok((origin.elapsed().as_secs_f64() * 1000.0).to_bits() as i64),
        LEAVING => Ok(0),
        _ => Err(RuntimeError::new(
            ErrorKind::Host,
            format!("{HOST}.agent_call: unsupported agent operation {operation}"),
        )),
    }
}

fn f64_value(value: f64) -> Value {
    Value::F64(value.to_bits())
}

fn number_pow(base: f64, exponent: f64) -> f64 {
    if exponent.is_nan() || (base.abs() == 1.0 && exponent.is_infinite()) {
        f64::NAN
    } else {
        base.powf(exponent)
    }
}

fn unary_math(name: &str) -> Option<fn(f64) -> f64> {
    Some(match name {
        "math_acos" => f64::acos,
        "math_acosh" => f64::acosh,
        "math_asin" => f64::asin,
        "math_asinh" => f64::asinh,
        "math_atan" => f64::atan,
        "math_atanh" => math_atanh,
        "math_cbrt" => f64::cbrt,
        "math_cos" => f64::cos,
        "math_cosh" => f64::cosh,
        "math_exp" => f64::exp,
        "math_expm1" => f64::exp_m1,
        "math_log" => f64::ln,
        "math_log10" => f64::log10,
        "math_log1p" => f64::ln_1p,
        "math_log2" => f64::log2,
        "math_sin" => f64::sin,
        "math_sinh" => f64::sinh,
        "math_tan" => f64::tan,
        "math_tanh" => f64::tanh,
        _ => return None,
    })
}

fn math_atanh(value: f64) -> f64 {
    let magnitude = value.abs();
    // Preserve subnormal values and signed zero, as in Scriptw's host.
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

pub(crate) fn completion_kind(results: &[Value]) -> Result<i32, Error> {
    let [
        Value::I32(_),
        Value::I64(_),
        Value::Ref(_),
        Value::I32(kind),
        Value::I32(target),
    ] = results
    else {
        return Err("Scriptw main returned an invalid completion tuple".into());
    };
    if *target != 0 {
        return Err(
            format!("Scriptw main returned an unresolved completion target {target}").into(),
        );
    }
    Ok(*kind)
}
