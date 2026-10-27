//! Opt-in benchmark-only phase barriers. The launcher transfers one dedicated
//! connected UnixStream descriptor; no engine or scored-observation policy lives here.
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const LIMIT: usize = 256;
const TIMEOUT: Duration = Duration::from_secs(30);
#[derive(Clone, Copy)]
enum Phase {
    TransportDrop,
    Instantiate,
    Execution,
    StoreDrop,
}

impl Phase {
    const fn name(self) -> &'static str {
        match self {
            Self::TransportDrop => "after_transport_drop",
            Self::Instantiate => "after_instantiate",
            Self::Execution => "after_execution",
            Self::StoreDrop => "after_store_drop",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::TransportDrop => 0,
            Self::Instantiate => 1,
            Self::Execution => 2,
            Self::StoreDrop => 3,
        }
    }
}

const PHASES: [&str; 4] = [
    Phase::TransportDrop.name(),
    Phase::Instantiate.name(),
    Phase::Execution.name(),
    Phase::StoreDrop.name(),
];

struct Configuration {
    fd: i32,
    nonce: String,
    index: usize,
}

fn configuration(
    values: [Option<String>; 3],
    samples: usize,
    warmups: usize,
) -> Result<Option<Configuration>, String> {
    let [fd, nonce, index] = values;
    match (fd, nonce, index) {
        (None, None, None) => Ok(None),
        (Some(fd), Some(nonce), Some(index)) => {
            let decimal = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit());
            if !decimal(&fd)
                || !decimal(&index)
                || nonce.len() != 32
                || !nonce
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            {
                return Err("invalid precompiled phase barrier configuration".into());
            }
            let fd: i32 = fd.parse().map_err(|_| "invalid phase socket descriptor")?;
            let index: usize = index
                .parse()
                .map_err(|_| "invalid phase observation index")?;
            let total = samples
                .checked_add(warmups)
                .ok_or("phase observation count overflow")?;
            if fd < 3 || index < warmups || index >= total {
                return Err(
                    "phase socket must be dedicated and index must select a measured observation"
                        .into(),
                );
            }
            Ok(Some(Configuration { fd, nonce, index }))
        }
        _ => Err("all three SCRIPTW_PRECOMPILED_PHASE settings are required together".into()),
    }
}

pub(super) struct Barriers {
    active: Option<Active>,
}
struct Active {
    config: Configuration,
    completed: usize,
    #[cfg(target_os = "linux")]
    socket: std::os::unix::net::UnixStream,
}

/// Address records are constructed only after capture; roles cannot be supplied
/// as arbitrary strings by an adapter.
#[cfg(feature = "benchmark-module-images")]
enum ModuleRole {
    Program,
    Runtime,
}

#[cfg(feature = "benchmark-module-images")]
pub(super) struct ModuleImageAddresses {
    image: std::ops::Range<usize>,
    text: std::ops::Range<usize>,
}

#[cfg(feature = "benchmark-module-images")]
impl ModuleImageAddresses {
    pub(super) fn new(image: std::ops::Range<usize>, text: std::ops::Range<usize>) -> Self {
        Self { image, text }
    }

    fn value(self, role: ModuleRole) -> Value {
        let role = match role {
            ModuleRole::Program => "program",
            ModuleRole::Runtime => "runtime",
        };
        json!({"role":role,"image":{"start":self.image.start,"end":self.image.end},
            "text":{"start":self.text.start,"end":self.text.end}})
    }
}

#[cfg(feature = "benchmark-module-images")]
pub(super) struct ModuleImages {
    program: ModuleImageAddresses,
    runtime: Option<ModuleImageAddresses>,
}

#[cfg(feature = "benchmark-module-images")]
impl ModuleImages {
    pub(super) fn new(
        program: ModuleImageAddresses,
        runtime: Option<ModuleImageAddresses>,
    ) -> Self {
        Self { program, runtime }
    }
}

impl Barriers {
    pub(super) fn from_environment(samples: usize, warmups: usize) -> Result<Self, String> {
        let mut values = [None, None, None];
        for (slot, name) in values.iter_mut().zip([
            "SCRIPTW_PRECOMPILED_PHASE_FD",
            "SCRIPTW_PRECOMPILED_PHASE_NONCE",
            "SCRIPTW_PRECOMPILED_PHASE_INDEX",
        ]) {
            *slot = match std::env::var(name) {
                Ok(value) => Some(value),
                Err(std::env::VarError::NotPresent) => None,
                Err(_) => return Err("non-Unicode phase configuration".into()),
            };
        }
        let Some(config) = configuration(values, samples, warmups)? else {
            return Ok(Self { active: None });
        };
        #[cfg(target_os = "linux")]
        {
            {
                use std::os::unix::fs::FileTypeExt;
                let metadata = std::fs::metadata(format!("/proc/self/fd/{}", config.fd))
                    .map_err(|e| format!("phase descriptor is not open: {e}"))?;
                if !metadata.file_type().is_socket() {
                    return Err("phase descriptor is not a socket".into());
                }
            }
            validate_socket(config.fd)?;
            use std::os::fd::FromRawFd;
            // SAFETY: the opt-in launcher contract transfers this dedicated,
            // open socket descriptor exactly once. It is never a product input.
            let socket = unsafe { std::os::unix::net::UnixStream::from_raw_fd(config.fd) };
            socket
                .peer_addr()
                .map_err(|e| format!("phase socket is not a connected UnixStream: {e}"))?;
            Ok(Self {
                active: Some(Active {
                    config,
                    completed: 0,
                    socket,
                }),
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = config;
            Err("precompiled phase barriers require Linux /proc and UnixStream support".into())
        }
    }

    pub(super) fn after_transport_drop(&mut self) -> Result<(), String> {
        self.arrive(Phase::TransportDrop, None)
    }
    pub(super) fn after_instantiate(&mut self, index: usize) -> Result<(), String> {
        self.arrive(Phase::Instantiate, Some(index))
    }
    pub(super) fn after_execution(&mut self, index: usize) -> Result<(), String> {
        self.arrive(Phase::Execution, Some(index))
    }
    pub(super) fn after_store_drop(&mut self, index: usize) -> Result<(), String> {
        self.arrive(Phase::StoreDrop, Some(index))
    }

    fn arrive(&mut self, phase: Phase, index: Option<usize>) -> Result<(), String> {
        let Some(active) = &mut self.active else {
            return Ok(());
        };
        if index.is_some_and(|index| index != active.config.index) {
            return Ok(());
        }
        if active.completed != phase.index() {
            return Err("out-of-order phase barrier".into());
        }
        let tuple = format!(
            "{} {} {} {}\n",
            active.config.nonce,
            phase.index() + 1,
            phase.name(),
            active.config.index
        );
        #[cfg(target_os = "linux")]
        exchange(
            &mut active.socket,
            &format!("SWPHASE1 {tuple}"),
            &format!("SWACK1 {tuple}"),
            TIMEOUT,
        )?;
        active.completed += 1;
        Ok(())
    }

    /// Never observe module metadata until every capture has been acknowledged.
    /// No active barrier means no call to the lazy collector and no new fields.
    #[cfg(feature = "benchmark-module-images")]
    pub(super) fn finish_with_module_images(
        self,
        report: &mut Value,
        collect: impl FnOnce() -> ModuleImages,
    ) -> Result<(), String> {
        let Some(active) = self.active.as_ref() else {
            return self.finish(report);
        };
        if active.completed != PHASES.len() {
            return Err("incomplete phase barrier sequence".into());
        }
        let nonce = active.config.nonce.clone();
        let observation_index = active.config.index;
        self.finish(report)?;
        let images = collect();
        let mut owners = vec![images.program.value(ModuleRole::Program)];
        if let Some(runtime) = images.runtime {
            owners.push(runtime.value(ModuleRole::Runtime));
        }
        report["phase_barriers"]["module_images"] = json!({
            "schema":"scriptw-precompiled-module-images-v1", "nonce":nonce,
            "observation_index":observation_index, "owners":owners});
        Ok(())
    }

    pub(super) fn finish(self, report: &mut Value) -> Result<(), String> {
        if let Some(active) = self.active {
            if active.completed != PHASES.len() {
                return Err("incomplete phase barrier sequence".into());
            }
            report["phase_barriers"] = json!({"schema":"scriptw-precompiled-phase-barriers-v1",
                "nonce":active.config.nonce,"observation_index":active.config.index,"completed_phases":PHASES});
            report["excluded_from_scored_cohorts"] = json!(true);
            report["diagnostic_total_timings_include_barrier_waits"] = json!(true);
        }
        Ok(())
    }
}

// Linux ABI checked against cached libc 0.2.184: unix/mod.rs getsockopt,
// linux_like/linux/mod.rs socklen_t, arch/{generic,powerpc,mips,sparc}
// socket options and each architecture's SOCK_STREAM constants.
#[cfg(target_os = "linux")]
fn validate_socket(fd: i32) -> Result<(), String> {
    use std::ffi::{c_int, c_void};
    type SockLen = u32;
    unsafe extern "C" {
        fn getsockopt(
            fd: c_int,
            level: c_int,
            option: c_int,
            value: *mut c_void,
            length: *mut SockLen,
        ) -> c_int;
    }
    const AF_UNIX: c_int = 1;
    #[cfg(any(
        target_arch = "mips",
        target_arch = "mips32r6",
        target_arch = "mips64",
        target_arch = "mips64r6",
        target_arch = "sparc",
        target_arch = "sparc64"
    ))]
    const OPTIONS: (c_int, c_int, c_int) = (0xffff, 0x1008, 0x1029);
    #[cfg(not(any(
        target_arch = "mips",
        target_arch = "mips32r6",
        target_arch = "mips64",
        target_arch = "mips64r6",
        target_arch = "sparc",
        target_arch = "sparc64"
    )))]
    const OPTIONS: (c_int, c_int, c_int) = (1, 3, 39);
    const SOL_SOCKET: c_int = OPTIONS.0;
    const SO_TYPE: c_int = OPTIONS.1;
    const SO_DOMAIN: c_int = OPTIONS.2;
    #[cfg(any(
        target_arch = "mips",
        target_arch = "mips32r6",
        target_arch = "mips64",
        target_arch = "mips64r6"
    ))]
    const SOCK_STREAM: c_int = 2;
    #[cfg(not(any(
        target_arch = "mips",
        target_arch = "mips32r6",
        target_arch = "mips64",
        target_arch = "mips64r6"
    )))]
    const SOCK_STREAM: c_int = 1;
    for (option, expected) in [(SO_TYPE, SOCK_STREAM), (SO_DOMAIN, AF_UNIX)] {
        let mut value: c_int = 0;
        let mut length = std::mem::size_of::<c_int>() as SockLen;
        // SAFETY: getsockopt only borrows the descriptor; both writable output
        // buffers have exactly the declared Linux ABI sizes and live through the call.
        let result = unsafe {
            getsockopt(
                fd,
                SOL_SOCKET,
                option,
                (&mut value as *mut c_int).cast(),
                &mut length,
            )
        };
        if result != 0 {
            return Err(format!(
                "phase socket option: {}",
                std::io::Error::last_os_error()
            ));
        }
        if length as usize != std::mem::size_of::<c_int>() || value != expected {
            return Err("phase descriptor must be an AF_UNIX SOCK_STREAM socket".into());
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn exchange(
    socket: &mut std::os::unix::net::UnixStream,
    event: &str,
    expected: &str,
    timeout: Duration,
) -> Result<(), String> {
    use std::io::{Read, Write};
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or("phase deadline overflow")?;
    let remaining = || {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or("phase barrier deadline exceeded")
    };
    let mut bytes = event.as_bytes();
    while !bytes.is_empty() {
        socket
            .set_write_timeout(Some(remaining()?))
            .map_err(|e| e.to_string())?;
        let n = socket
            .write(bytes)
            .map_err(|e| format!("phase event write: {e}"))?;
        if n == 0 {
            return Err("phase event socket closed".into());
        }
        bytes = &bytes[n..];
    }
    let mut line = Vec::with_capacity(LIMIT);
    loop {
        if line.len() == LIMIT {
            return Err("phase acknowledgement exceeds 256 bytes".into());
        }
        socket
            .set_read_timeout(Some(remaining()?))
            .map_err(|e| e.to_string())?;
        let mut byte = [0];
        if socket
            .read(&mut byte)
            .map_err(|e| format!("phase acknowledgement read: {e}"))?
            == 0
        {
            return Err("phase acknowledgement EOF".into());
        }
        line.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    if line != expected.as_bytes() {
        return Err("phase acknowledgement tuple mismatch".into());
    }
    remaining()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn values(fd: &str, nonce: &str, index: &str) -> [Option<String>; 3] {
        [Some(fd.into()), Some(nonce.into()), Some(index.into())]
    }
    const NONCE: &str = "0123456789abcdef0123456789abcdef";
    #[test]
    fn configuration_requires_dedicated_socket_and_measured_index() {
        assert!(configuration([None, None, None], 7, 10).unwrap().is_none());
        assert!(configuration(values("3", NONCE, "16"), 7, 10)
            .unwrap()
            .is_some());
        for v in [
            values("2", NONCE, "16"),
            values("3", NONCE, "9"),
            values("3", NONCE, "17"),
            values("3", "ABCDEF", "16"),
            [Some("3".into()), None, None],
        ] {
            assert!(configuration(v, 7, 10).is_err());
        }
    }
    #[cfg(feature = "benchmark-module-images")]
    #[test]
    fn module_addresses_are_not_collected_without_barriers() {
        let mut report = json!({"existing":true});
        Barriers { active: None }
            .finish_with_module_images(&mut report, || {
                panic!("inactive diagnostics must not inspect module metadata")
            })
            .unwrap();
        assert_eq!(report, json!({"existing":true}));
    }

    #[cfg(all(feature = "benchmark-module-images", target_os = "linux"))]
    #[test]
    fn module_addresses_require_the_fourth_acknowledgement() {
        let (socket, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let config = configuration(values("3", NONCE, "16"), 7, 10)
            .unwrap()
            .unwrap();
        let barriers = Barriers {
            active: Some(Active {
                config,
                completed: 3,
                socket,
            }),
        };
        let mut report = json!({});
        assert!(barriers
            .finish_with_module_images(&mut report, || {
                panic!("three captures cannot admit address observation")
            })
            .is_err());
        assert_eq!(report, json!({}));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn acknowledgement_requires_exact_tuple_and_bounded_line() {
        use std::io::{Read, Write};
        for reply in [
            b"SWACK1 nonce 1 after_transport_drop 16\n".to_vec(),
            b"SWACK1 nonce 2 after_transport_drop 16\n".to_vec(),
            vec![b'x'; 257],
            Vec::new(),
        ] {
            let (mut client, mut peer) = std::os::unix::net::UnixStream::pair().unwrap();
            let valid = reply.starts_with(b"SWACK1 nonce 1 ");
            let worker = std::thread::spawn(move || {
                let mut event = [0; 6];
                peer.read_exact(&mut event).unwrap();
                assert_eq!(&event, b"event\n");
                let _ = peer.write_all(&reply);
            });
            assert_eq!(
                exchange(
                    &mut client,
                    "event\n",
                    "SWACK1 nonce 1 after_transport_drop 16\n",
                    Duration::from_secs(1)
                )
                .is_ok(),
                valid
            );
            worker.join().unwrap();
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn selected_observation_requires_all_four_ordered_handshakes() {
        use std::io::{BufRead, BufReader, Write};
        let (socket, peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let worker = std::thread::spawn(move || {
            peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
            let mut peer = BufReader::new(peer);
            for (sequence, phase) in PHASES.iter().enumerate() {
                let mut event = String::new();
                peer.read_line(&mut event).unwrap();
                assert_eq!(
                    event,
                    format!("SWPHASE1 {NONCE} {} {phase} 16\n", sequence + 1)
                );
                peer.get_mut()
                    .write_all(event.replacen("SWPHASE1", "SWACK1", 1).as_bytes())
                    .unwrap();
            }
        });
        let config = configuration(values("3", NONCE, "16"), 7, 10)
            .unwrap()
            .unwrap();
        let mut barriers = Barriers {
            active: Some(Active {
                config,
                completed: 0,
                socket,
            }),
        };
        assert!(barriers.after_execution(16).is_err());
        barriers.after_transport_drop().unwrap();
        barriers.after_instantiate(15).unwrap();
        barriers.after_execution(15).unwrap();
        barriers.after_store_drop(15).unwrap();
        barriers.after_instantiate(16).unwrap();
        barriers.after_execution(16).unwrap();
        barriers.after_store_drop(16).unwrap();
        let mut report = json!({});
        #[cfg(feature = "benchmark-module-images")]
        {
            let calls = std::cell::Cell::new(0);
            barriers
                .finish_with_module_images(&mut report, || {
                    calls.set(calls.get() + 1);
                    // Independent ranges: the hook must not impose containment.
                    ModuleImages::new(ModuleImageAddresses::new(100..200, 300..400), None)
                })
                .unwrap();
            assert_eq!(calls.get(), 1);
            assert_eq!(
                report["phase_barriers"]["module_images"],
                json!({
                "schema":"scriptw-precompiled-module-images-v1", "nonce":NONCE,
                "observation_index":16,"owners":[{"role":"program",
                "image":{"start":100,"end":200},"text":{"start":300,"end":400}}]})
            );
        }
        #[cfg(not(feature = "benchmark-module-images"))]
        barriers.finish(&mut report).unwrap();
        assert_eq!(report["phase_barriers"]["completed_phases"], json!(PHASES));
        assert_eq!(report["excluded_from_scored_cohorts"], true);
        assert_eq!(
            report["diagnostic_total_timings_include_barrier_waits"],
            true
        );
        worker.join().unwrap();
        let mut unchanged = json!({"untouched":true});
        Barriers { active: None }.finish(&mut unchanged).unwrap();
        assert_eq!(unchanged, json!({"untouched":true}));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn socket_admission_rejects_datagrams_and_inet_streams_before_ownership() {
        use std::os::fd::AsRawFd;
        let (stream, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        assert!(validate_socket(stream.as_raw_fd()).is_ok());
        let (datagram, _peer) = std::os::unix::net::UnixDatagram::pair().unwrap();
        assert!(validate_socket(datagram.as_raw_fd()).is_err());
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        assert!(validate_socket(listener.as_raw_fd()).is_err());
        assert!(validate_socket(-1).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn missing_acknowledgement_times_out() {
        let (mut client, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        assert!(exchange(&mut client, "event\n", "ack\n", Duration::from_millis(10)).is_err());
    }
}
