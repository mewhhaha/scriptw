use super::*;
use wasmtime::{Linker, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{p1, p1::WasiP1Ctx, p2::pipe::MemoryOutputPipe, I32Exit, WasiCtxBuilder};

struct State {
    wasi: WasiP1Ctx,
    limits: StoreLimits,
}

pub(super) fn observe(
    engine: &Engine,
    module: &Module,
    expected: &str,
    samples: usize,
    warmups: usize,
    barriers: &mut phase_barriers::Barriers,
) -> Result<Vec<Value>, String> {
    let mut observations = Vec::new();
    for index in 0..samples + warmups {
        let before_store = residency::snapshot()?;
        let total = Instant::now();
        let started = Instant::now();
        let stdout = MemoryOutputPipe::new(1024 * 1024);
        let stderr = MemoryOutputPipe::new(1024 * 1024);
        let wasi = WasiCtxBuilder::new()
            .arg("benchmark")
            .stdout(stdout.clone())
            .stderr(stderr.clone())
            .build_p1();
        let state = State {
            wasi,
            limits: StoreLimitsBuilder::new()
                .memory_size(1024 * 1024 * 1024)
                .build(),
        };
        let mut store = Store::new(engine, state);
        store.limiter(|state| &mut state.limits);
        store.set_epoch_deadline(u64::MAX / 2);
        let mut linker: Linker<State> = Linker::new(engine);
        let state_linker_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        p1::add_to_linker_sync(&mut linker, |state| &mut state.wasi).map_err(|e| e.to_string())?;
        let register_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        let instance = linker
            .instantiate(&mut store, module)
            .map_err(|e| e.to_string())?;
        let start = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .map_err(|e| e.to_string())?;
        let instantiate_ns = started.elapsed().as_nanos();
        let after_instantiate = residency::snapshot()?;
        barriers.after_instantiate(index)?;
        let gc_before = store.gc_heap_capacity();
        let memory_before = instance
            .get_memory(&mut store, "memory")
            .map(|m| m.data_size(&store));
        let started = Instant::now();
        let result = start.call(&mut store, ());
        let output = stdout.contents();
        let errors = stderr.contents();
        let execution_ns = started.elapsed().as_nanos();
        let gc_after = store.gc_heap_capacity();
        let memory_after = instance
            .get_memory(&mut store, "memory")
            .map(|m| m.data_size(&store));
        let after_execution = residency::snapshot()?;
        let total_ns = total.elapsed().as_nanos();
        barriers.after_execution(index)?;
        // Like the Scriptw adapter, output flattening/completion validation is
        // outside execution_ns and follows the after_execution RSS endpoint.
        match result {
            Ok(()) => (),
            Err(error)
                if error
                    .downcast_ref::<I32Exit>()
                    .is_some_and(|exit| exit.0 == 0) =>
            {
                ()
            }
            Err(error) => return Err(format!("WASI execution failed: {error:#}")),
        }
        let output = std::str::from_utf8(&output)
            .map_err(|_| "non-UTF8 WASI stdout")?
            .to_owned();
        let errors = std::str::from_utf8(&errors)
            .map_err(|_| "non-UTF8 WASI stderr")?
            .to_owned();
        if output != expected || !errors.is_empty() {
            return Err(format!(
                "unexpected WASI output: stdout={output:?}; stderr={errors:?}"
            ));
        }
        drop(linker);
        drop(store);
        drop(stdout);
        drop(stderr);
        let after_store_drop = residency::snapshot()?;
        barriers.after_store_drop(index)?;
        observations.push(json!({"index":index, "warmup":index < warmups,
            "completion":"normal", "stdout":output, "stderr":errors, "matched":true,
            "phases_ns":{"state_linker":state_linker_ns, "register":register_ns,
                "instantiate":instantiate_ns, "execution":execution_ns, "total":total_ns},
            "gc_capacity_before_bytes":gc_before, "gc_capacity_after_bytes":gc_after,
            "exported_linear_memory_before_bytes":memory_before,
            "exported_linear_memory_after_bytes":memory_after,
            "process_snapshots":{"before_store":before_store,"after_instantiate":after_instantiate,
                "after_execution":after_execution,"after_store_drop":after_store_drop}}));
    }
    Ok(observations)
}
