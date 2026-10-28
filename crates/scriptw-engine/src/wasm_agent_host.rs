use super::*;

pub(super) fn agent_call(
    mut caller: WasmtimeCaller<'_, WasmHostState>,
    operation: i64,
    first: i64,
    second: i64,
) -> wasmtime::Result<i64> {
    let group = caller.data().agent_group.clone();
    let private_memory = match caller.get_export("memory") {
        Some(WasmtimeExtern::Memory(memory)) => memory,
        _ => {
            return Err(wasmtime::Error::msg(
                "Test262 agent host call requires exported private memory",
            ));
        }
    };
    let read_bytes = |caller: &WasmtimeCaller<'_, WasmHostState>,
                      ptr: i64,
                      len: i64|
     -> wasmtime::Result<Vec<u8>> {
        let ptr = usize::try_from(ptr)
            .map_err(|_| wasmtime::Error::msg("Test262 agent pointer is negative"))?;
        let len = usize::try_from(len)
            .map_err(|_| wasmtime::Error::msg("Test262 agent length is negative"))?;
        let mut bytes = vec![0; len];
        private_memory
            .read(caller, ptr, &mut bytes)
            .map_err(|err| {
                wasmtime::Error::msg(format!(
                    "failed to read Test262 agent memory at {ptr} for {len} bytes: {err}"
                ))
            })?;
        Ok(bytes)
    };
    let write_bytes = |caller: &mut WasmtimeCaller<'_, WasmHostState>,
                       ptr: i64,
                       bytes: &[u8]|
     -> wasmtime::Result<()> {
        let ptr = usize::try_from(ptr)
            .map_err(|_| wasmtime::Error::msg("Test262 agent pointer is negative"))?;
        private_memory.write(caller, ptr, bytes).map_err(|err| {
            wasmtime::Error::msg(format!(
                "failed to write Test262 agent memory at {ptr} for {} bytes: {err}",
                bytes.len()
            ))
        })
    };

    let operation = AgentHostOperation::from_wire(operation).ok_or_else(|| {
        wasmtime::Error::msg(format!("unknown Test262 agent host operation {operation}"))
    })?;
    match operation {
        AgentHostOperation::Start => {
            #[cfg(not(feature = "native-source-compiler"))]
            {
                Err(wasmtime::Error::msg(
                    "agent.start source compilation is unavailable in this build; enable native-source-compiler",
                ))
            }
            #[cfg(feature = "native-source-compiler")]
            {
                let group = group.as_ref().ok_or_else(|| {
                    wasmtime::Error::msg("agent.start used without an active Test262 agent group")
                })?;
                let source = String::from_utf8(read_bytes(&caller, first, second)?).map_err(|err| {
                    wasmtime::Error::msg(format!("Test262 agent source is not UTF-8: {err}"))
                })?;
                group.start(source).map_err(|err| {
                    wasmtime::Error::new(err).context("failed to compile or start Test262 agent")
                })?;
                if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                    eprintln!("scriptw wasm trace: Test262 agent started");
                }
                Ok(0)
            }
        }
        AgentHostOperation::Report => {
            let group = group.as_ref().ok_or_else(|| {
                wasmtime::Error::msg("agent.report used without an active Test262 agent group")
            })?;
            let report = read_bytes(&caller, first, second)?;
            let trace_report = std::env::var_os("SCRIPTW_WASM_TRACE")
                .is_some()
                .then(|| String::from_utf8_lossy(&report).into_owned());
            group
                .reports
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push_back(report);
            if let Some(report) = trace_report {
                eprintln!("scriptw wasm trace: Test262 agent queued report {report:?}");
            }
            Ok(0)
        }
        AgentHostOperation::ReportLength => Ok(group.as_ref().map_or(-1, |group| {
            group
                .reports
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .front()
                .map_or(-1, |report| report.len() as i64)
        })),
        AgentHostOperation::ReportCopy => {
            let report = group
                .as_ref()
                .ok_or_else(|| {
                    wasmtime::Error::msg(
                        "agent.getReport used without an active Test262 agent group",
                    )
                })?
                .reports
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .pop_front()
                .ok_or_else(|| {
                    wasmtime::Error::msg("Test262 agent report queue changed before copy")
                })?;
            if report.len() != usize::try_from(second).unwrap_or(usize::MAX) {
                return Err(wasmtime::Error::msg(format!(
                    "Test262 agent report length changed: expected {second}, observed {}",
                    report.len()
                )));
            }
            write_bytes(&mut caller, first, &report)?;
            Ok(second)
        }
        AgentHostOperation::Sleep => {
            let milliseconds = f64::from_bits(first as u64);
            if let Some(group) = &group {
                group
                    .execution_control
                    .sleep(milliseconds)
                    .map_err(wasmtime::Error::new)?;
            } else if milliseconds.is_finite() && milliseconds > 0.0 {
                std::thread::sleep(std::time::Duration::from_secs_f64(milliseconds / 1000.0));
            }
            Ok(0)
        }
        AgentHostOperation::MonotonicNow => {
            let now = caller.data().realm.host_clock().monotonic_instant();
            let origin = group
                .as_ref()
                .map_or(caller.data().monotonic_clock_origin, |group| {
                    group.started_at
                });
            let elapsed = now.saturating_duration_since(origin);
            Ok(elapsed.as_milliseconds_f64().to_bits() as i64)
        }
        AgentHostOperation::Leaving => {
            if let Some(leaving) = &caller.data().agent_leaving {
                leaving.store(true, std::sync::atomic::Ordering::Release);
            }
            Ok(0)
        }
        AgentHostOperation::PollAsyncWaiter => Ok(caller.data().async_waiters.poll(first)),
        AgentHostOperation::CancelAsyncWaiter => Ok(caller.data().async_waiters.cancel(first)),
    }
}
