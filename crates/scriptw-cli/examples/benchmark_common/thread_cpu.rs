//! Same-thread CPU timestamps shared by benchmark-only CLI examples.
use scriptw_engine::{WasmThreadCpuInstant, WasmThreadCpuObservation, WasmThreadCpuUnavailable};
use serde_json::{json, Value};

pub(crate) type Start = Result<WasmThreadCpuInstant, WasmThreadCpuUnavailable>;

pub(crate) const EXECUTION_SCOPE: &str = "execution-worker-thread";
pub(crate) const POLICY: &str = "Observation only; no acceptance gate or runtime-policy change. CPU includes guest and synchronous host/GC work, including nested work on the same worker; it excludes descheduled time and other threads, including agents. Paired CPU/wall reads have boundary skew. Row-marker CPU reads occur inside wall callback endpoints and their overhead remains in middle_callback_ns. CPU/wall differences do not identify allocation, GC, scheduler or other causes. Unavailable clock reads are reported explicitly.";

pub(crate) fn start() -> Start {
    WasmThreadCpuInstant::now()
}

pub(crate) fn finish(start: Start) -> WasmThreadCpuObservation {
    match start {
        Ok(start) => start.elapsed(),
        Err(reason) => WasmThreadCpuObservation::Unavailable(reason),
    }
}

pub(crate) fn serialize(observation: &WasmThreadCpuObservation, scope: &str) -> Value {
    match observation {
        WasmThreadCpuObservation::Sampled(elapsed) => json!({
            "status": "sampled", "elapsed_ns": elapsed.as_nanos(), "scope": scope,
        }),
        WasmThreadCpuObservation::Unavailable(reason) => {
            let (kind, os_error) = match reason {
                WasmThreadCpuUnavailable::UnsupportedPlatform => ("unsupported_platform", None),
                WasmThreadCpuUnavailable::ClockReadFailed(code) => {
                    ("clock_read_failed", Some(*code))
                }
                WasmThreadCpuUnavailable::InvalidClockValue => ("invalid_clock_value", None),
                WasmThreadCpuUnavailable::ClockWentBackwards => ("clock_went_backwards", None),
                WasmThreadCpuUnavailable::DifferentThread => ("different_thread", None),
            };
            let mut result = json!({"status": "unavailable", "reason": kind, "scope": scope});
            if let Some(code) = os_error {
                result["os_error"] = json!(code);
            }
            result
        }
    }
}
