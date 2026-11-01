//! CPU samples retain their thread identity; worker clocks cannot be combined.
use std::{thread::ThreadId, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmThreadCpuUnavailable {
    UnsupportedPlatform,
    ClockReadFailed(i32),
    InvalidClockValue,
    ClockWentBackwards,
    DifferentThread,
}

/// CPU consumed on one execution thread, including synchronous host/GC work.
/// Other threads, including agents, are excluded. This is not GC pause time or
/// an explanation of the difference between CPU time and wall time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmThreadCpuObservation {
    Sampled(Duration),
    Unavailable(WasmThreadCpuUnavailable),
}

/// An opaque clock endpoint that rejects subtraction across execution workers.
#[derive(Debug, Clone)]
pub struct WasmThreadCpuInstant {
    thread: ThreadId,
    value: Duration,
}

impl WasmThreadCpuInstant {
    pub fn now() -> Result<Self, WasmThreadCpuUnavailable> {
        let thread = std::thread::current().id();
        Ok(Self {
            thread,
            value: current_thread_cpu()?,
        })
    }

    pub fn duration_since(&self, earlier: &Self) -> WasmThreadCpuObservation {
        if self.thread != earlier.thread {
            return WasmThreadCpuObservation::Unavailable(
                WasmThreadCpuUnavailable::DifferentThread,
            );
        }
        match self.value.checked_sub(earlier.value) {
            Some(elapsed) => WasmThreadCpuObservation::Sampled(elapsed),
            None => {
                WasmThreadCpuObservation::Unavailable(WasmThreadCpuUnavailable::ClockWentBackwards)
            }
        }
    }

    pub fn elapsed(&self) -> WasmThreadCpuObservation {
        match Self::now() {
            Ok(end) => end.duration_since(self),
            Err(reason) => WasmThreadCpuObservation::Unavailable(reason),
        }
    }
}

#[cfg(target_os = "linux")]
fn current_thread_cpu() -> Result<Duration, WasmThreadCpuUnavailable> {
    use rustix::time::{clock_gettime_dynamic, ClockId, DynamicClockId};
    // The fallible entry handles restricted/unusual kernels without failing
    // JavaScript execution or disguising a missing clock as a zero duration.
    let time = clock_gettime_dynamic(DynamicClockId::Known(ClockId::ThreadCPUTime))
        .map_err(|error| WasmThreadCpuUnavailable::ClockReadFailed(error.raw_os_error()))?;
    clock_value(time.tv_sec, time.tv_nsec)
}

#[cfg(not(target_os = "linux"))]
fn current_thread_cpu() -> Result<Duration, WasmThreadCpuUnavailable> {
    Err(WasmThreadCpuUnavailable::UnsupportedPlatform)
}

#[cfg(any(target_os = "linux", test))]
fn clock_value(seconds: i64, nanoseconds: i64) -> Result<Duration, WasmThreadCpuUnavailable> {
    let seconds =
        u64::try_from(seconds).map_err(|_| WasmThreadCpuUnavailable::InvalidClockValue)?;
    let nanoseconds =
        u32::try_from(nanoseconds).map_err(|_| WasmThreadCpuUnavailable::InvalidClockValue)?;
    if nanoseconds >= 1_000_000_000 {
        return Err(WasmThreadCpuUnavailable::InvalidClockValue);
    }
    Ok(Duration::new(seconds, nanoseconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_from_different_workers_cannot_be_subtracted() {
        let earlier = WasmThreadCpuInstant {
            thread: std::thread::current().id(),
            value: Duration::from_secs(1),
        };
        let other = std::thread::spawn(|| WasmThreadCpuInstant {
            thread: std::thread::current().id(),
            value: Duration::from_secs(2),
        })
        .join()
        .unwrap();
        assert_eq!(
            other.duration_since(&earlier),
            WasmThreadCpuObservation::Unavailable(WasmThreadCpuUnavailable::DifferentThread)
        );
    }

    #[test]
    fn invalid_or_reversed_clock_values_are_unavailable() {
        for (seconds, nanos) in [(-1, 0), (0, -1), (0, 1_000_000_000)] {
            assert_eq!(
                clock_value(seconds, nanos),
                Err(WasmThreadCpuUnavailable::InvalidClockValue)
            );
        }
        assert_eq!(clock_value(1, 7), Ok(Duration::new(1, 7)));
        let earlier = WasmThreadCpuInstant {
            thread: std::thread::current().id(),
            value: Duration::from_secs(2),
        };
        let later = WasmThreadCpuInstant {
            thread: earlier.thread,
            value: Duration::from_secs(1),
        };
        assert_eq!(
            later.duration_since(&earlier),
            WasmThreadCpuObservation::Unavailable(WasmThreadCpuUnavailable::ClockWentBackwards)
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn current_worker_clock_records_cpu_work() {
        let start = WasmThreadCpuInstant::now().expect("Linux thread CPU clock");
        let mut value = 1_u64;
        for _ in 0..100_000 {
            value = std::hint::black_box(value.wrapping_mul(17).wrapping_add(3));
        }
        let WasmThreadCpuObservation::Sampled(elapsed) = start.elapsed() else {
            panic!("same worker has a monotonic CPU clock");
        };
        assert!(elapsed > Duration::ZERO);
    }
}
