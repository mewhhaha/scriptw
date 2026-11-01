//! Opt-in benchmark markers through the ordinary synchronous print hook.
//! The workload still enters through JS main in a fresh product Store.
use scriptw_engine::{
    HostHooks, WasmThreadCpuInstant, WasmThreadCpuObservation, WasmThreadCpuUnavailable,
};
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const MARKERS: [&str; 3] = [
    "scriptw:rows:construction:start",
    "scriptw:rows:traversal:start",
    "scriptw:rows:traversal:end",
];

type CpuReading = Result<WasmThreadCpuInstant, WasmThreadCpuUnavailable>;

struct MarkerTimes {
    entered: Instant,
    returned: Instant,
    entered_cpu: CpuReading,
    returned_cpu: CpuReading,
}

struct CpuPhases {
    construction: WasmThreadCpuObservation,
    traversal: WasmThreadCpuObservation,
    combined: WasmThreadCpuObservation,
    middle_callback: WasmThreadCpuObservation,
}

impl CpuPhases {
    fn new(start: &MarkerTimes, middle: &MarkerTimes, end: &MarkerTimes) -> Self {
        fn span(earlier: &CpuReading, later: &CpuReading) -> WasmThreadCpuObservation {
            match (earlier, later) {
                (Ok(earlier), Ok(later)) => later.duration_since(earlier),
                (Err(reason), _) | (_, Err(reason)) => {
                    WasmThreadCpuObservation::Unavailable(*reason)
                }
            }
        }
        Self {
            construction: span(&start.returned_cpu, &middle.entered_cpu),
            traversal: span(&middle.returned_cpu, &end.entered_cpu),
            combined: span(&start.returned_cpu, &end.entered_cpu),
            middle_callback: span(&middle.entered_cpu, &middle.returned_cpu),
        }
    }

    fn json(&self) -> Value {
        let scope = "marker-callback-worker-thread";
        json!({
            "construction": super::thread_cpu_json(&self.construction, scope),
            "traversal": super::thread_cpu_json(&self.traversal, scope),
            "combined": super::thread_cpu_json(&self.combined, scope),
            "middle_callback": super::thread_cpu_json(&self.middle_callback, scope),
        })
    }
}

#[derive(Clone, Copy)]
enum Region {
    Construction,
    Traversal,
    Combined,
}
impl Region {
    fn as_str(self) -> &'static str {
        match self {
            Self::Construction => "construction",
            Self::Traversal => "traversal",
            Self::Combined => "combined",
        }
    }
    fn boundaries(self) -> (usize, usize) {
        match self {
            Self::Construction => (0, 1),
            Self::Traversal => (1, 2),
            Self::Combined => (0, 2),
        }
    }
}

struct PerfControl {
    control: File,
    acknowledgement: File,
    region: Region,
    active: bool,
}
impl PerfControl {
    fn from_environment() -> Result<Option<Self>, String> {
        let values = [
            "SCRIPTW_ROW_PERF_CONTROL",
            "SCRIPTW_ROW_PERF_ACK",
            "SCRIPTW_ROW_PERF_REGION",
        ]
        .map(std::env::var_os);
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        let [Some(control), Some(ack), Some(region)] = values else {
            return Err(
                "row perf control, acknowledgement and region must be supplied together".into(),
            );
        };
        let region = match region.to_str() {
            Some("construction") => Region::Construction,
            Some("traversal") => Region::Traversal,
            Some("combined") => Region::Combined,
            _ => return Err("row perf region must be construction, traversal or combined".into()),
        };
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::{fs::FileTypeExt, fs::OpenOptionsExt};
            // Linux O_NONBLOCK. Keep this diagnostic free of a new dependency.
            const NONBLOCK: i32 = 0x800;
            let open = |path| -> Result<File, String> {
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .custom_flags(NONBLOCK)
                    .open(path)
                    .map_err(|e| format!("open perf FIFO: {e}"))?;
                if !file
                    .metadata()
                    .map_err(|e| e.to_string())?
                    .file_type()
                    .is_fifo()
                {
                    return Err("perf control paths must be FIFOs".into());
                }
                Ok(file)
            };
            Ok(Some(Self {
                control: open(control)?,
                acknowledgement: open(ack)?,
                region,
                active: false,
            }))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (control, ack, region);
            Err("row perf FIFO control requires Linux".into())
        }
    }

    fn command(&mut self, enable: bool) -> Result<Duration, String> {
        let started = Instant::now();
        let command: &[u8] = if enable { b"enable\n" } else { b"disable\n" };
        self.control
            .write_all(command)
            .map_err(|e| format!("write perf control: {e}"))?;
        // Until disable is acknowledged, assume events may still be enabled.
        if enable {
            self.active = true;
        }
        let mut answer = Vec::new();
        while started.elapsed() < Duration::from_secs(5) {
            let mut bytes = [0; 8];
            match self.acknowledgement.read(&mut bytes) {
                Ok(count) if count > 0 => {
                    answer.extend_from_slice(&bytes[..count]);
                    // perf writes the terminating NUL as part of its FIFO reply.
                    if answer == b"ack\n\0" {
                        self.active = enable;
                        return Ok(started.elapsed());
                    }
                    if !b"ack\n\0".starts_with(&answer) {
                        return Err(format!("unexpected perf acknowledgement: {answer:?}"));
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(format!("read perf acknowledgement: {error}")),
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err("perf acknowledgement exceeded five seconds".into())
    }
}
impl Drop for PerfControl {
    fn drop(&mut self) {
        if self.active {
            // Error cleanup must not block. A failed diagnostic is never accepted.
            let _ = self.control.write_all(b"disable\n");
        }
    }
}

#[derive(Default)]
struct State {
    active_invocation: bool,
    measured: bool,
    times: Vec<MarkerTimes>,
    error: Option<String>,
    control: Option<PerfControl>,
    control_spans: Vec<(bool, Duration)>,
}

#[derive(Clone)]
pub(super) struct Observer(Arc<Mutex<State>>);
impl Observer {
    pub(super) fn new() -> Result<Self, String> {
        Ok(Self(Arc::new(Mutex::new(State {
            control: PerfControl::from_environment()?,
            ..State::default()
        }))))
    }

    pub(super) fn begin(&self, measured: bool) -> Result<(), String> {
        let mut state = self.0.lock().map_err(|_| "row observer mutex poisoned")?;
        if state.active_invocation || state.control.as_ref().is_some_and(|control| control.active) {
            return Err("row observer's previous invocation is unfinished".into());
        }
        state.active_invocation = true;
        state.measured = measured;
        state.times.clear();
        state.control_spans.clear();
        state.error = None;
        Ok(())
    }

    pub(super) fn finish(&self) -> Result<Value, String> {
        let mut state = self.0.lock().map_err(|_| "row observer mutex poisoned")?;
        if let Some(error) = &state.error {
            return Err(error.clone());
        }
        if !state.active_invocation || state.times.len() != 3 {
            return Err("row profile requires exactly three ordered markers per invocation".into());
        }
        if state.control.as_ref().is_some_and(|control| control.active) {
            return Err("row profile completed while perf sampling remained active".into());
        }
        state.active_invocation = false;
        let t = &state.times;
        let cpu = CpuPhases::new(&t[0], &t[1], &t[2]);
        Ok(json!({
            "construction_ns": t[1].entered.duration_since(t[0].returned).as_nanos(),
            "traversal_ns": t[2].entered.duration_since(t[1].returned).as_nanos(),
            "combined_ns": t[2].entered.duration_since(t[0].returned).as_nanos(),
            "middle_callback_ns": t[1].returned.duration_since(t[1].entered).as_nanos(),
            "thread_cpu": cpu.json(),
            "perf_sampled": state.measured && state.control.is_some(),
            "perf_region": state.control.as_ref().map(|control| control.region.as_str()),
            "perf_control_spans": state.control_spans.iter().map(|(enable, elapsed)|
                json!({"enable":enable,"elapsed_ns":elapsed.as_nanos()})).collect::<Vec<_>>(),
            "boundary": "Host-hook wall time; marker return/next marker entry includes guest print work. Combined includes middle callback. Perf acknowledgements occur inside callbacks, outside the two individual phase spans. CPU scope and clock-read overhead follow the report's thread_cpu_observation_policy."
        }))
    }
}

impl HostHooks for Observer {
    fn print_line(&self, text: &str) {
        if !text.starts_with("scriptw:rows:") {
            return;
        }
        let entered = Instant::now();
        let entered_cpu = WasmThreadCpuInstant::now();
        let Ok(mut state) = self.0.lock() else {
            return;
        };
        if state.error.is_some() {
            return;
        }
        let index = state.times.len();
        if !state.active_invocation || MARKERS.get(index).copied() != Some(text) {
            state.error = Some(format!("unexpected row marker at position {index}: {text}"));
            return;
        }
        if state.measured {
            if let Some(control) = &mut state.control {
                let (start, end) = control.region.boundaries();
                if index == start || index == end {
                    match control.command(index == start) {
                        Ok(elapsed) => state.control_spans.push((index == start, elapsed)),
                        Err(error) => {
                            state.error = Some(error);
                            return;
                        }
                    }
                }
            }
        }
        let returned_cpu = WasmThreadCpuInstant::now();
        let returned = Instant::now();
        state.times.push(MarkerTimes {
            entered,
            returned,
            entered_cpu,
            returned_cpu,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observer() -> Observer {
        Observer(Arc::new(Mutex::new(State::default())))
    }
    #[test]
    fn phase_spans_keep_the_middle_callback_explicit() {
        let observer = observer();
        for measured in [false, true] {
            observer.begin(measured).unwrap();
            for marker in MARKERS {
                observer.print_line(marker);
            }
            let result = observer.finish().unwrap();
            assert_eq!(
                result["combined_ns"].as_u64().unwrap(),
                result["construction_ns"].as_u64().unwrap()
                    + result["traversal_ns"].as_u64().unwrap()
                    + result["middle_callback_ns"].as_u64().unwrap()
            );
            assert_eq!(result["perf_sampled"], false);
        }
    }
    #[test]
    fn missing_repeated_and_out_of_order_markers_fail() {
        for markers in [
            vec![MARKERS[0]],
            vec![MARKERS[0], MARKERS[0]],
            vec![MARKERS[1]],
        ] {
            let observer = observer();
            observer.begin(true).unwrap();
            observer.print_line("ordinary checksum output");
            for marker in markers {
                observer.print_line(marker);
            }
            assert!(observer.finish().is_err());
        }
    }
    #[test]
    fn unfinished_observations_cannot_be_overwritten() {
        let observer = observer();
        observer.begin(false).unwrap();
        assert!(observer.begin(true).is_err());
        assert!(observer.finish().is_err());
    }
    #[cfg(target_os = "linux")]
    fn cpu_phases(observer: &Observer) -> CpuPhases {
        let state = observer.0.lock().unwrap();
        let [start, middle, end] = state.times.as_slice() else {
            panic!("all marker endpoints are required");
        };
        CpuPhases::new(start, middle, end)
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn cpu_phases_belong_to_the_marker_worker_and_keep_callback_time() {
        let observer = observer();
        observer.begin(true).unwrap();
        let worker = observer.clone();
        std::thread::spawn(move || {
            for marker in MARKERS {
                worker.print_line(marker);
            }
        })
        .join()
        .unwrap();
        // Subtract the captured worker endpoints on this coordinating thread;
        // finish must never sample the coordinator's own CPU counter.
        observer.finish().unwrap();
        let CpuPhases {
            construction: WasmThreadCpuObservation::Sampled(construction),
            traversal: WasmThreadCpuObservation::Sampled(traversal),
            combined: WasmThreadCpuObservation::Sampled(combined),
            middle_callback: WasmThreadCpuObservation::Sampled(callback),
        } = cpu_phases(&observer)
        else {
            panic!("Linux worker thread CPU endpoints must be sampled");
        };
        assert_eq!(combined, construction + callback + traversal);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn cpu_phases_reject_cross_thread_frontiers_without_losing_valid_spans() {
        let observer = observer();
        observer.begin(false).unwrap();
        observer.print_line(MARKERS[0]);
        let worker = observer.clone();
        std::thread::spawn(move || worker.print_line(MARKERS[1]))
            .join()
            .unwrap();
        observer.print_line(MARKERS[2]);
        observer.finish().unwrap();
        let cpu = cpu_phases(&observer);
        assert!(matches!(
            cpu.construction,
            WasmThreadCpuObservation::Unavailable(WasmThreadCpuUnavailable::DifferentThread)
        ));
        assert!(matches!(
            cpu.traversal,
            WasmThreadCpuObservation::Unavailable(WasmThreadCpuUnavailable::DifferentThread)
        ));
        assert!(matches!(cpu.combined, WasmThreadCpuObservation::Sampled(_)));
        assert!(matches!(
            cpu.middle_callback,
            WasmThreadCpuObservation::Sampled(_)
        ));
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn perf_commands_require_acknowledgement_and_disable_after_the_region() {
        use std::os::{fd::OwnedFd, unix::net::UnixStream};
        let (control, mut peer_control) = UnixStream::pair().unwrap();
        let (ack, mut peer_ack) = UnixStream::pair().unwrap();
        ack.set_nonblocking(true).unwrap();
        let control: OwnedFd = control.into();
        let ack: OwnedFd = ack.into();
        let mut perf = PerfControl {
            control: File::from(control),
            acknowledgement: File::from(ack),
            region: Region::Traversal,
            active: false,
        };
        let peer = std::thread::spawn(move || {
            for expected in [b"enable\n".as_slice(), b"disable\n".as_slice()] {
                let mut command = vec![0; expected.len()];
                peer_control.read_exact(&mut command).unwrap();
                assert_eq!(command, expected);
                peer_ack.write_all(b"ack\n\0").unwrap();
            }
        });
        perf.command(true).unwrap();
        assert!(perf.active);
        perf.command(false).unwrap();
        assert!(!perf.active);
        peer.join().unwrap();
    }
}
