use super::*;
use std::io::Read;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Process endpoints only. This is neither a sampled maximum nor live JS data.
pub(super) fn snapshot() -> Result<Value, String> {
    let mut raw = String::new();
    fs::File::open("/proc/self/smaps_rollup")
        .map_err(|e| e.to_string())?
        .take(64 * 1024)
        .read_to_string(&mut raw)
        .map_err(|e| e.to_string())?;
    let field = |name: &str| -> Result<u64, String> {
        let mut matches = raw.lines().filter_map(|line| line.strip_prefix(name));
        let mut words = matches
            .next()
            .ok_or("missing process memory field")?
            .split_whitespace();
        let count: u64 = words
            .next()
            .ok_or("missing process memory count")?
            .parse()
            .map_err(|_| "invalid process memory count")?;
        if words.next() != Some("kB") || words.next().is_some() || matches.next().is_some() {
            return Err("ambiguous process memory field".into());
        }
        count
            .checked_mul(1024)
            .ok_or_else(|| "process memory count overflow".into())
    };
    Ok(
        json!({"rss_bytes": field("Rss:")?, "pss_bytes": field("Pss:")?,
        "private_dirty_bytes": field("Private_Dirty:")?,
        "scope": "linux-process-smaps-rollup-endpoint-not-peak-or-live-js"}),
    )
}

fn rss() -> Result<u64, String> {
    let mut raw = String::new();
    fs::File::open("/proc/self/status")
        .map_err(|e| e.to_string())?
        .take(64 * 1024)
        .read_to_string(&mut raw)
        .map_err(|e| e.to_string())?;
    let line = raw
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .ok_or("missing process RSS")?;
    let mut words = line.split_whitespace();
    let value: u64 = words
        .next()
        .ok_or("missing RSS count")?
        .parse()
        .map_err(|_| "invalid RSS count")?;
    if words.next() != Some("kB") || words.next().is_some() {
        return Err("invalid RSS unit".into());
    }
    value.checked_mul(1024).ok_or_else(|| "RSS overflow".into())
}

/// Retain the first R decode's temporary residency as sampled RSS, separately
/// from post-load endpoints. The producer/consumer process remains independent
/// of ordinary cohorts; a sampled maximum is not a whole-child high-water mark.
pub(super) fn measure_loading<T>(
    load: impl FnOnce() -> Result<T, String>,
) -> Result<(T, Value), String> {
    let started = Instant::now();
    let initial = rss()?;
    let (stop, stopped) = mpsc::channel();
    let sampler = std::thread::Builder::new()
        .name("precompiled-load-rss".into())
        .stack_size(256 * 1024)
        .spawn(move || -> Result<_, String> {
            let mut maximum = initial;
            let mut count = 1u64;
            loop {
                let finished = match stopped.recv_timeout(Duration::from_millis(10)) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => true,
                    Err(mpsc::RecvTimeoutError::Timeout) => false,
                };
                let current = rss()?;
                maximum = maximum.max(current);
                count = count.checked_add(1).ok_or("RSS sample counter overflow")?;
                if finished {
                    return Ok((current, maximum, count));
                }
            }
        })
        .map_err(|e| e.to_string())?;
    let result = load();
    let _ = stop.send(());
    let sampled = sampler.join().map_err(|_| "load RSS sampler panicked")?;
    let value = result?;
    let (final_rss, maximum, count) = sampled?;
    Ok((
        value,
        json!({"status": "sampled", "interval_ns": 10_000_000,
        "initial_rss_bytes": initial, "final_rss_bytes": final_rss,
        "maximum_rss_bytes": maximum, "sample_count": count,
        "elapsed_ns": started.elapsed().as_nanos(),
        "scope": "native-P-and-Cargo-R-admission-process-RSS-sampled-maximum-not-peak-or-live-js"}),
    ))
}
