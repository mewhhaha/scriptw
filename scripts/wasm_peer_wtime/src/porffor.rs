use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

use wtime_runtime::{
    Engine, ErrorKind, External, FuncType, Instance, Linker, Module, Store, ValType, Value,
};

use crate::Error;

/// ADAPTER_WAT in wasm_peer_wasmtime.py plus two wrappers for the Rust clocks.
#[rustfmt::skip]
const ADAPTER: &[u8] = &[
    0x00, b'a', b's', b'm', 0x01, 0x00, 0x00, 0x00,
    // Types: (f64) -> (), () -> f64, and () -> ().
    0x01, 12, 3, 0x60, 1, 0x7c, 0, 0x60, 0, 1, 0x7c, 0x60, 0, 0,
    // Import Rust callbacks and declare the adapter functions.
    0x02, 44, 3,
    4, b'h', b'o', b's', b't', 5, b'p', b'r', b'i', b'n', b't', 0, 0,
    4, b'h', b'o', b's', b't', 4, b't', b'i', b'm', b'e', 0, 1,
    4, b'h', b'o', b's', b't', 10, b't', b'i', b'm', b'e', b'O', b'r', b'i', b'g', b'i', b'n', 0, 1,
    0x03, 6, 5, 0, 0, 2, 1, 1,
    // Four memory pages and two mutable i32 counters (length in bytes and numeric prints).
    0x05, 3, 1, 0, 4,
    0x06, 11, 2, 0x7f, 1, 0x41, 0, 0x0b, 0x7f, 1, 0x41, 0, 0x0b,
    // Exports.
    0x07, 74, 8,
    3, b'o', b'u', b't', 2, 0,
    6, b'l', b'e', b'n', b'g', b't', b'h', 3, 0,
    7, b'n', b'u', b'm', b'e', b'r', b'i', b'c', 3, 1,
    9, b'p', b'r', b'i', b'n', b't', b'C', b'h', b'a', b'r', 0, 3,
    5, b'p', b'r', b'i', b'n', b't', 0, 4,
    5, b'e', b'm', b'p', b't', b'y', 0, 5,
    4, b't', b'i', b'm', b'e', 0, 6,
    10, b't', b'i', b'm', b'e', b'O', b'r', b'i', b'g', b'i', b'n', 0, 7,
    0x0a, 60, 5,
    // printChar: trap if full, store one UTF-16 unit, advance length.
    29, 0,
    0x23, 0, 0x41, 0x80, 0x80, 0x10, 0x4f, 0x04, 0x40, 0x00, 0x0b,
    0x23, 0, 0x20, 0, 0xfc, 3, 0x3b, 1, 0,
    0x23, 0, 0x41, 2, 0x6a, 0x24, 0, 0x0b,
    // print: increment numeric, call host.print.
    13, 0, 0x23, 1, 0x41, 1, 0x6a, 0x24, 1, 0x20, 0, 0x10, 0, 0x0b,
    // empty.
    2, 0, 0x0b,
    // time and timeOrigin: nop precedes the host call so the register
    // executor can resume in decoded execution past the function's entry PC.
    5, 0, 0x01, 0x10, 1, 0x0b,
    5, 0, 0x01, 0x10, 2, 0x0b,
];

pub(crate) fn compile_adapter(engine: &Engine) -> Result<Module, Error> {
    Ok(engine.compile(ADAPTER)?)
}

#[derive(Default)]
struct Capture {
    numeric_text: Mutex<String>,
    clock_calls: AtomicU64,
}

pub(crate) struct Output {
    pub(crate) stdout: String,
    pub(crate) numeric_print_calls: u32,
    pub(crate) clock_calls: u64,
    pub(crate) mixed_output: bool,
}

pub(crate) struct Iteration {
    pub(crate) program: Instance,
    pub(crate) adapter: Instance,
    capture: Arc<Capture>,
}

pub(crate) fn instantiate(
    engine: &Engine,
    store: &mut Store,
    program: &Module,
    adapter: &Module,
    clock_origin: Instant,
    origin_ms: f64,
) -> Result<Iteration, Error> {
    let capture = Arc::new(Capture::default());
    let numeric_capture = Arc::clone(&capture);
    let print = store.create_host_func(FuncType::new(vec![ValType::F64], vec![]), move |args| {
        let [Value::F64(bits)] = args else {
            return Err(wtime_runtime::Error::new(
                ErrorKind::Host,
                "expected f64 in host.print",
            ));
        };
        let mut text = numeric_capture.numeric_text.lock().map_err(|error| {
            wtime_runtime::Error::new(ErrorKind::Host, format!("host.print: {error}"))
        })?;
        text.push_str(ryu_js::Buffer::new().format(f64::from_bits(*bits)));
        Ok(vec![])
    })?;
    let clock_type = FuncType::new(vec![], vec![ValType::F64]);
    let time_capture = Arc::clone(&capture);
    let time = store.create_host_func(clock_type.clone(), move |_| {
        time_capture.clock_calls.fetch_add(1, Ordering::Relaxed);
        Ok(vec![Value::F64(
            (clock_origin.elapsed().as_secs_f64() * 1e3).to_bits(),
        )])
    })?;
    let origin_capture = Arc::clone(&capture);
    let time_origin = store.create_host_func(clock_type, move |_| {
        origin_capture.clock_calls.fetch_add(1, Ordering::Relaxed);
        Ok(vec![Value::F64(origin_ms.to_bits())])
    })?;
    let mut linker = Linker::new(engine);
    linker.define("host", "print", External::Func(print))?;
    linker.define("host", "time", External::Func(time))?;
    linker.define("host", "timeOrigin", External::Func(time_origin))?;
    let adapter = linker.instantiate(store, adapter)?;

    linker.define("", "a", adapter.export(store, "print")?)?;
    linker.define("", "b", adapter.export(store, "printChar")?)?;
    linker.define("", "c", adapter.export(store, "time")?)?;
    linker.define("", "d", adapter.export(store, "timeOrigin")?)?;
    let program = linker.instantiate(store, program)?;
    Ok(Iteration {
        program,
        adapter,
        capture,
    })
}

impl Iteration {
    fn counter(&self, store: &Store, name: &str) -> Result<u32, Error> {
        if let External::Global(global) = self.adapter.export(store, name)?
            && let Value::I32(value) = store.get_global(global)?
        {
            return Ok(value as u32);
        }
        Err(format!("Porffor adapter export {name} must be an i32 global").into())
    }

    pub(crate) fn read_output(&self, store: &Store) -> Result<Output, Error> {
        let numeric_print_calls = self.counter(store, "numeric")?;
        let length = self.counter(store, "length")? as usize;
        let stdout = if numeric_print_calls != 0 {
            self.capture
                .numeric_text
                .lock()
                .map_err(|error| error.to_string())?
                .clone()
        } else {
            let External::Memory(memory) = self.adapter.export(store, "out")? else {
                return Err("Porffor adapter export out must be a memory".into());
            };
            let bytes = store.read_memory(memory, 0, length)?;
            let (pairs, _) = bytes.as_chunks::<2>();
            let units: Vec<_> = pairs.iter().copied().map(u16::from_le_bytes).collect();
            String::from_utf16(&units).map_err(|error| format!("Porffor stdout: {error}"))?
        };
        Ok(Output {
            stdout,
            numeric_print_calls,
            clock_calls: self.capture.clock_calls.load(Ordering::Relaxed),
            mixed_output: numeric_print_calls != 0 && length != 0,
        })
    }
}
