use std::borrow::Cow;
use std::cmp::Reverse;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use wasmtime::CacheStore;

#[path = "cache/native.rs"]
mod native;
pub(crate) use native::NativeFunctionCache;

pub(crate) const DEFAULT_CACHE_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;
pub(crate) const CACHE_PRUNE_PERCENT: u64 = 70;

/// Overrides the compiled-code storage budget for this process.
///
/// A full Test262 sweep compiles tens of thousands of programs. With the
/// default 1 GiB budget the Cranelift stencil cache sits at its cap for the
/// whole run, pruning to `CACHE_PRUNE_PERCENT` and then re-earning the evicted
/// 30% through repeated native compilation. That cache is keyed by Cranelift's
/// own stencil/target/flags rather than by `SCRIPTW_COMPILER_FINGERPRINT`, so
/// it survives compiler edits and is the most valuable warm-start asset the
/// project has — raising the budget for a sweep is worth real wall-clock.
const CACHE_LIMIT_ENV: &str = "SCRIPTW_CACHE_LIMIT_BYTES";

/// Per-tier overrides, for workloads whose tiers behave very differently.
///
/// Measured over a 300-case Test262 sample on 2026-07-30: the program-Wasm and
/// Wasmtime-module tiers grow by roughly 9 MiB and 17 MiB *per case*, and both
/// are keyed by source text. Every Test262 case is a distinct source, so across
/// a single sweep neither tier ever serves a hit — they are pure write and
/// prune churn, and holding the full suite would need on the order of 1.5 TiB.
/// The Cranelift stencil tier is the opposite: it is keyed per function, and
/// builtin bodies are shared by every case, so it converges and pays for itself.
///
/// A sweep therefore wants a large function tier and minimal program/module
/// tiers, which the single total-budget knob cannot express.
const FUNCTION_CACHE_LIMIT_ENV: &str = "SCRIPTW_FUNCTION_CACHE_LIMIT_BYTES";
const MODULE_CACHE_LIMIT_ENV: &str = "SCRIPTW_MODULE_CACHE_LIMIT_BYTES";
const PROGRAM_CACHE_LIMIT_ENV: &str = "SCRIPTW_PROGRAM_CACHE_LIMIT_BYTES";

/// The overall budget, and the default for the Cranelift stencil tier.
pub(crate) fn cache_limit_bytes() -> u64 {
    static LIMIT: OnceLock<u64> = OnceLock::new();
    *LIMIT.get_or_init(|| parse_cache_limit(std::env::var(CACHE_LIMIT_ENV).ok().as_deref()))
}

/// Budget for the Cranelift function-stencil tier.
pub(crate) fn function_cache_limit_bytes() -> u64 {
    static LIMIT: OnceLock<u64> = OnceLock::new();
    *LIMIT.get_or_init(|| tier_limit_bytes(FUNCTION_CACHE_LIMIT_ENV, cache_limit_bytes()))
}

/// Budget for the native-module tier, and separately for the build-only
/// Wasmtime module cache.
pub(crate) fn module_cache_limit_bytes() -> u64 {
    static LIMIT: OnceLock<u64> = OnceLock::new();
    *LIMIT.get_or_init(|| tier_limit_bytes(MODULE_CACHE_LIMIT_ENV, cache_limit_bytes() / 2))
}

/// Budget for the emitted program-Wasm tier.
pub(crate) fn program_cache_limit_bytes() -> u64 {
    static LIMIT: OnceLock<u64> = OnceLock::new();
    *LIMIT.get_or_init(|| tier_limit_bytes(PROGRAM_CACHE_LIMIT_ENV, cache_limit_bytes() / 2))
}

fn tier_limit_bytes(variable: &str, fallback: u64) -> u64 {
    parse_tier_limit(std::env::var(variable).ok().as_deref()).unwrap_or(fallback)
}

/// `None` when unset or unusable, so the caller can apply its own default
/// rather than collapsing every tier onto the global one.
fn parse_tier_limit(raw: Option<&str>) -> Option<u64> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|bytes| *bytes > 0)
}

/// Unset, blank, unparseable and zero all fall back to the default rather than
/// panicking: a typo in an environment variable must not take down a sweep that
/// has been running for hours.
fn parse_cache_limit(raw: Option<&str>) -> u64 {
    parse_tier_limit(raw).unwrap_or(DEFAULT_CACHE_LIMIT_BYTES)
}

const FUNCTION_CACHE_FORMAT: &str = "cranelift-functions-v1";
/// Whole native modules, mapped by the product (`NativeModuleCache`).
const NATIVE_MODULE_CACHE_DIR: &str = "native-modules-v1";
/// Wasmtime's own compressed module cache. Only the Cargo runtime-image
/// producer still configures it; no product Engine does.
const BUILD_MODULE_CACHE_DIR: &str = "wasmtime-modules-v1";
const PROGRAM_CACHE_DIR: &str = "program-wasm-v1";

// A process can own several cache instances for the same directory. Their
// temporary-file identities must not restart at zero independently.
static NEXT_CACHE_TEMP: AtomicU64 = AtomicU64::new(0);

/// When an entry's bytes reach the disk relative to its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryPublication {
    /// The name may survive a power loss without the bytes. A torn entry
    /// is at worst bad input to a reader that validates what it reads.
    Unsynced,
    /// The bytes are synced before the rename, so a name never points at
    /// a torn file. Entries that are mapped and executed without a digest
    /// (native modules) need this: a lost middle would otherwise fail every
    /// later run of that program instead of once.
    SyncedBeforeRename,
}

fn insert_atomic(
    path: &Path,
    value: &[u8],
    publication: EntryPublication,
    next: &AtomicU64,
) -> io::Result<()> {
    let (temp, mut file) = loop {
        let suffix = next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| io::Error::other("cache temporary-file identity space exhausted"))?;
        let mut temp_name: OsString = path.file_name().unwrap_or_default().to_os_string();
        temp_name.push(format!(".tmp-{}-{suffix}", std::process::id()));
        let temp = path.with_file_name(temp_name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(file) => break (temp, file),
            // A stale file from a reused PID, or a pre-existing symlink, is
            // never opened or truncated. Reserve another name instead.
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let written = file.write_all(value).and_then(|()| match publication {
        EntryPublication::Unsynced => Ok(()),
        EntryPublication::SyncedBeforeRename => file.sync_data(),
    });
    // Close before rename/removal, including on Windows. Publication is one
    // rename only after every byte has been written to our exclusively owned
    // temporary file (and, when synced, has reached the disk).
    drop(file);
    let result = written.and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheDirectoryStatus {
    pub path: PathBuf,
    pub bytes: u64,
    pub files: u64,
    pub limit_bytes: Option<u64>,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheStatus {
    pub function_cache: CacheDirectoryStatus,
    pub module_cache: CacheDirectoryStatus,
    pub build_module_cache: CacheDirectoryStatus,
    pub program_cache: CacheDirectoryStatus,
    pub legacy_wasmtime_cache: CacheDirectoryStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachePruneReport {
    pub scriptw_bytes_removed: u64,
    pub scriptw_files_removed: u64,
    pub legacy_bytes_removed: u64,
    pub legacy_files_removed: u64,
}

/// A flat, bounded directory of immutable entries named by their key.
///
/// Opening the store and reading from it never list the directory: a process
/// whose lookups all hit pays for those entries only, however many others the
/// directory holds. The size bound is enforced by writers. The first insert
/// of a process observes the directory once (pruning it if it is over its
/// limit, whoever grew it), and later inserts account for their own bytes.
#[derive(Debug)]
pub(crate) struct FunctionCache {
    directory: PathBuf,
    limit_bytes: u64,
    publication: EntryPublication,
    /// Unset until this process has listed the directory.
    observed_bytes: OnceLock<AtomicU64>,
    hits: AtomicU64,
    misses: AtomicU64,
    prune_lock: Mutex<()>,
}

impl FunctionCache {
    pub(crate) fn new(directory: PathBuf, limit_bytes: u64) -> io::Result<Self> {
        fs::create_dir_all(&directory)?;
        Ok(Self {
            directory,
            limit_bytes,
            publication: EntryPublication::Unsynced,
            observed_bytes: OnceLock::new(),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            prune_lock: Mutex::new(()),
        })
    }

    /// This store with `publication` for its later inserts.
    pub(crate) fn with_publication(mut self, publication: EntryPublication) -> Self {
        self.publication = publication;
        self
    }

    fn entry_path(&self, key: &[u8]) -> PathBuf {
        let mut name = String::with_capacity(key.len() * 2);
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in key {
            name.push(HEX[(byte >> 4) as usize] as char);
            name.push(HEX[(byte & 0x0f) as usize] as char);
        }
        self.directory.join(name)
    }

    fn prune_if_needed(&self) -> io::Result<()> {
        let _guard = self
            .prune_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let bytes = match prune_directory_to_limit(&self.directory, self.limit_bytes)? {
            CachePruneResult::WithinLimit { observed_bytes } => observed_bytes,
            // Keep the existing post-deletion accounting. Other processes can
            // also write this directory; a sum of our removed sizes is not a
            // replacement for observing the files that actually remain.
            CachePruneResult::Pruned => directory_usage(&self.directory)?.0,
        };
        // Set and replaced only under `prune_lock`.
        match self.observed_bytes.get() {
            Some(observed) => observed.store(bytes, Ordering::Relaxed),
            None => {
                let _ = self.observed_bytes.set(AtomicU64::new(bytes));
            }
        }
        Ok(())
    }

    /// Opens an entry and marks it recently used, even on noatime mounts.
    /// Failing to mark it must not lose an entry that can be read.
    ///
    /// Counts neither a hit nor a miss: a caller that maps the file instead
    /// of reading it decides whether the entry was usable.
    pub(crate) fn open(&self, key: &[u8]) -> io::Result<fs::File> {
        let path = self.entry_path(key);
        let file = fs::File::options()
            .read(true)
            .write(true)
            .open(&path)
            .or_else(|_| fs::File::open(&path))?;
        let _ = file.set_modified(SystemTime::now());
        Ok(file)
    }

    pub(crate) fn counters(&self) -> (u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
        )
    }

    pub(crate) fn contains(&self, key: &[u8]) -> bool {
        self.entry_path(key).is_file()
    }

    pub(crate) fn read(&self, key: &[u8]) -> Option<Vec<u8>> {
        <Self as CacheStore>::get(self, key).map(Cow::into_owned)
    }

    pub(crate) fn write(&self, key: &[u8], value: Vec<u8>) -> bool {
        <Self as CacheStore>::insert(self, key, value)
    }

    pub(crate) fn remove(&self, key: &[u8]) {
        let path = self.entry_path(key);
        if let Ok(metadata) = path.metadata() {
            if fs::remove_file(path).is_ok() {
                if let Some(observed) = self.observed_bytes.get() {
                    observed.fetch_sub(metadata.len(), Ordering::Relaxed);
                }
            }
        }
    }
}

impl CacheStore for FunctionCache {
    fn get(&self, key: &[u8]) -> Option<Cow<'_, [u8]>> {
        // Cranelift calls this trait method directly, so function and
        // program hits both refresh their recency here.
        let read = self.open(key).and_then(|mut file| {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        match read {
            Ok(bytes) => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some(Cow::Owned(bytes))
            }
            Err(_) => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    fn insert(&self, key: &[u8], value: Vec<u8>) -> bool {
        let path = self.entry_path(key);
        let old_len = path.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        if insert_atomic(&path, &value, self.publication, &NEXT_CACHE_TEMP).is_err() {
            return false;
        }
        let Some(observed) = self.observed_bytes.get() else {
            // This process's first insert. The listing includes the entry
            // just published and enforces the limit against everything other
            // processes left here.
            return self.prune_if_needed().is_ok();
        };
        let new_len = value.len() as u64;
        let retained = if new_len >= old_len {
            observed
                .fetch_add(new_len - old_len, Ordering::Relaxed)
                .saturating_add(new_len - old_len)
        } else {
            observed
                .fetch_sub(old_len - new_len, Ordering::Relaxed)
                .saturating_sub(old_len - new_len)
        };
        // The common path is O(1). The directory is listed on the first
        // insert and when the tracked total crosses the hard limit.
        retained <= self.limit_bytes || self.prune_if_needed().is_ok()
    }
}

pub(crate) fn function_cache_directory() -> PathBuf {
    scriptw_cache_root().join(FUNCTION_CACHE_FORMAT)
}

pub(crate) fn native_module_cache_directory() -> PathBuf {
    scriptw_cache_root().join(NATIVE_MODULE_CACHE_DIR)
}

/// Wasmtime's own compressed module cache, opened only by the Cargo
/// runtime-image producer (`embedded_runtime/build_cache.rs`).
pub(crate) fn build_module_cache_directory() -> PathBuf {
    scriptw_cache_root().join(BUILD_MODULE_CACHE_DIR)
}

pub(crate) fn program_cache_directory() -> PathBuf {
    scriptw_cache_root().join(PROGRAM_CACHE_DIR)
}

pub fn cache_status() -> io::Result<CacheStatus> {
    Ok(CacheStatus {
        function_cache: directory_status(
            &function_cache_directory(),
            Some(function_cache_limit_bytes()),
        )?,
        module_cache: directory_status(
            &native_module_cache_directory(),
            Some(module_cache_limit_bytes()),
        )?,
        build_module_cache: directory_status(
            &build_module_cache_directory(),
            Some(module_cache_limit_bytes()),
        )?,
        program_cache: directory_status(
            &program_cache_directory(),
            Some(program_cache_limit_bytes()),
        )?,
        legacy_wasmtime_cache: directory_status(&legacy_wasmtime_cache_directory(), None)?,
    })
}

pub fn prune_caches(include_legacy_wasmtime: bool) -> io::Result<CachePruneReport> {
    let function_path = function_cache_directory();
    let module_path = native_module_cache_directory();
    let build_module_path = build_module_cache_directory();
    let program_path = program_cache_directory();
    let function = remove_directory_contents(&function_path)?;
    let module = remove_directory_contents(&module_path)?;
    let build_module = remove_directory_contents(&build_module_path)?;
    let program = remove_directory_contents(&program_path)?;
    // A long-lived embedder may invoke the reusable command entrypoint again
    // after pruning. Keep initialized cache objects usable without requiring a
    // process restart.
    fs::create_dir_all(function_path)?;
    fs::create_dir_all(module_path)?;
    fs::create_dir_all(build_module_path)?;
    fs::create_dir_all(program_path)?;
    let legacy = if include_legacy_wasmtime {
        remove_directory_contents(&legacy_wasmtime_cache_directory())?
    } else {
        (0, 0)
    };
    Ok(CachePruneReport {
        scriptw_bytes_removed: function
            .0
            .saturating_add(module.0)
            .saturating_add(build_module.0)
            .saturating_add(program.0),
        scriptw_files_removed: function
            .1
            .saturating_add(module.1)
            .saturating_add(build_module.1)
            .saturating_add(program.1),
        legacy_bytes_removed: legacy.0,
        legacy_files_removed: legacy.1,
    })
}

fn scriptw_cache_root() -> PathBuf {
    if let Some(path) = std::env::var_os("SCRIPTW_CACHE_DIR") {
        return PathBuf::from(path);
    }
    platform_cache_root().join("scriptw")
}

fn legacy_wasmtime_cache_directory() -> PathBuf {
    platform_cache_root().join("wasmtime")
}

fn platform_cache_root() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(path);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".cache");
    }
    std::env::temp_dir()
}

fn directory_status(path: &Path, limit_bytes: Option<u64>) -> io::Result<CacheDirectoryStatus> {
    let exists = path.exists();
    let (bytes, files) = directory_usage(path)?;
    Ok(CacheDirectoryStatus {
        path: path.to_path_buf(),
        bytes,
        files,
        limit_bytes,
        exists,
    })
}

fn directory_usage(path: &Path) -> io::Result<(u64, u64)> {
    if !path.exists() {
        return Ok((0, 0));
    }
    let mut bytes = 0u64;
    let mut files = 0u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let Some(metadata) = existing_entry_metadata(&entry)? else {
                continue;
            };
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                bytes = bytes.saturating_add(metadata.len());
                files = files.saturating_add(1);
            }
        }
    }
    Ok((bytes, files))
}

pub(crate) enum CachePruneResult {
    WithinLimit { observed_bytes: u64 },
    Pruned,
}

pub(crate) fn prune_directory_to_limit(
    path: &Path,
    limit_bytes: u64,
) -> io::Result<CachePruneResult> {
    let mut entries = cache_files(path)?;
    let total = entries.iter().map(|entry| entry.1).sum::<u64>();
    if total <= limit_bytes {
        return Ok(CachePruneResult::WithinLimit {
            observed_bytes: total,
        });
    }
    let target = limit_bytes.saturating_mul(CACHE_PRUNE_PERCENT) / 100;
    entries.sort_by_key(|entry| Reverse(entry.2));
    let mut retained = total;
    while retained > target {
        let Some((path, bytes, _)) = entries.pop() else {
            break;
        };
        match fs::remove_file(path) {
            Ok(()) => retained = retained.saturating_sub(bytes),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                retained = retained.saturating_sub(bytes)
            }
            Err(err) => return Err(err),
        }
    }
    Ok(CachePruneResult::Pruned)
}

fn cache_files(path: &Path) -> io::Result<Vec<(PathBuf, u64, SystemTime)>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let Some(metadata) = existing_entry_metadata(&entry)? else {
                continue;
            };
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                let accessed = metadata.accessed().unwrap_or(SystemTime::UNIX_EPOCH);
                let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let used = accessed.max(modified);
                files.push((entry.path(), metadata.len(), used));
            }
        }
    }
    Ok(files)
}

fn existing_entry_metadata(entry: &fs::DirEntry) -> io::Result<Option<fs::Metadata>> {
    match entry.metadata() {
        Ok(metadata) => Ok(Some(metadata)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn remove_directory_contents(path: &Path) -> io::Result<(u64, u64)> {
    let usage = directory_usage(path)?;
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(usage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn temp_cache(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "scriptw-cache-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    #[test]
    fn tier_limit_override_falls_back_to_the_tier_default_not_the_global_one() {
        assert_eq!(parse_tier_limit(Some("104857600")), Some(104857600));
        assert_eq!(parse_tier_limit(Some("  104857600 ")), Some(104857600));

        // `None` rather than a value, so each tier keeps its own default instead
        // of every tier collapsing onto the global budget.
        for rejected in [
            None,
            Some(""),
            Some("   "),
            Some("100MB"),
            Some("-1"),
            Some("0"),
        ] {
            assert_eq!(
                parse_tier_limit(rejected),
                None,
                "expected no override for {rejected:?}"
            );
        }
    }

    #[test]
    fn cache_limit_override_parses_and_falls_back_instead_of_panicking() {
        assert_eq!(parse_cache_limit(Some("8589934592")), 8589934592);
        assert_eq!(parse_cache_limit(Some("  8589934592  ")), 8589934592);

        // A typo in the environment must not take down a multi-hour sweep.
        for rejected in [
            None,
            Some(""),
            Some("   "),
            Some("8GiB"),
            Some("-1"),
            Some("0"),
        ] {
            assert_eq!(
                parse_cache_limit(rejected),
                DEFAULT_CACHE_LIMIT_BYTES,
                "expected fallback for {rejected:?}"
            );
        }
    }

    #[test]
    fn function_cache_round_trips_and_overwrites_corruption() {
        let root = temp_cache("round-trip");
        let _ = fs::remove_dir_all(&root);
        let cache = FunctionCache::new(root.clone(), 1024).expect("cache should initialize");
        assert!(cache.get(b"key").is_none());
        assert!(cache.insert(b"key", vec![1, 2, 3]));
        assert_eq!(cache.get(b"key").as_deref(), Some(&[1, 2, 3][..]));
        assert!(cache.insert(b"key", vec![9]));
        assert_eq!(cache.get(b"key").as_deref(), Some(&[9][..]));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cache_reads_refresh_recency_when_access_times_do_not_change() {
        for direct_function_read in [true, false] {
            let root = temp_cache(if direct_function_read {
                "function-read-recency"
            } else {
                "program-read-recency"
            });
            let _ = fs::remove_dir_all(&root);
            let cache = FunctionCache::new(root.clone(), 100).expect("cache should initialize");
            assert!(cache.insert(b"reused", vec![1; 30]));
            assert!(cache.insert(b"unused", vec![2; 40]));
            let old = fs::FileTimes::new()
                .set_accessed(SystemTime::UNIX_EPOCH)
                .set_modified(SystemTime::UNIX_EPOCH);
            fs::File::options()
                .write(true)
                .open(cache.entry_path(b"reused"))
                .unwrap()
                .set_times(old)
                .unwrap();
            fs::File::options()
                .write(true)
                .open(cache.entry_path(b"unused"))
                .unwrap()
                .set_times(
                    fs::FileTimes::new()
                        .set_accessed(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1))
                        .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1)),
                )
                .unwrap();

            let bytes = if direct_function_read {
                <FunctionCache as CacheStore>::get(&cache, b"reused").map(Cow::into_owned)
            } else {
                cache.read(b"reused")
            };
            assert_eq!(bytes.as_deref(), Some(&[1; 30][..]));
            // Model noatime even when this fixture runs on a filesystem that
            // updated atime during the read. Keep the refreshed mtime intact.
            fs::File::options()
                .write(true)
                .open(cache.entry_path(b"reused"))
                .unwrap()
                .set_times(fs::FileTimes::new().set_accessed(SystemTime::UNIX_EPOCH))
                .unwrap();
            assert!(cache.insert(b"new", vec![3; 31]));

            assert!(cache.contains(b"reused"));
            assert!(!cache.contains(b"unused"));
            assert!(cache.contains(b"new"));
            assert_eq!(cache.counters(), (1, 0));
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn function_cache_contains_checks_entry_metadata_without_counting_a_read() {
        let root = temp_cache("contains");
        let _ = fs::remove_dir_all(&root);
        let cache = FunctionCache::new(root.clone(), 1024).expect("cache should initialize");

        assert!(!cache.contains(b"key"));
        assert!(cache.insert(b"key", vec![1, 2, 3]));
        assert!(cache.contains(b"key"));
        assert_eq!(cache.counters(), (0, 0));

        cache.remove(b"key");
        assert!(!cache.contains(b"key"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn function_cache_is_safe_under_concurrent_atomic_writes() {
        let root = temp_cache("concurrent");
        let _ = fs::remove_dir_all(&root);
        let cache = Arc::new(FunctionCache::new(root.clone(), 4096).unwrap());
        let threads = (0..8)
            .map(|value| {
                let cache = cache.clone();
                std::thread::spawn(move || cache.insert(b"same-key", vec![value; 64]))
            })
            .collect::<Vec<_>>();
        for thread in threads {
            assert!(thread.join().unwrap());
        }
        let value = cache.get(b"same-key").expect("entry should exist");
        assert_eq!(value.len(), 64);
        assert!(value.iter().all(|byte| *byte == value[0]));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn function_cache_prunes_to_seventy_percent_after_limit() {
        let root = temp_cache("prune");
        let _ = fs::remove_dir_all(&root);
        let cache = FunctionCache::new(root.clone(), 100).unwrap();
        assert!(cache.insert(b"one", vec![1; 60]));
        assert!(cache.insert(b"two", vec![2; 60]));
        let (bytes, _) = directory_usage(&root).unwrap();
        assert!(bytes <= 70, "cache retained {bytes} bytes");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn opening_and_reading_an_over_limit_store_lists_nothing_until_an_insert() {
        let root = temp_cache("lazy-observation");
        let _ = fs::remove_dir_all(&root);
        let writer = FunctionCache::new(root.clone(), 1024).unwrap();
        assert!(writer.insert(b"older", vec![1; 60]));
        assert!(writer.insert(b"newer", vec![2; 60]));

        // Another process's view of the same directory, now over its limit.
        let cache = FunctionCache::new(root.clone(), 100).unwrap();
        assert_eq!(cache.get(b"older").as_deref(), Some(&[1; 60][..]));
        assert!(cache.get(b"absent").is_none());
        assert!(cache.contains(b"newer"));
        assert!(cache.open(b"newer").is_ok());
        // A listing would have recorded its total and pruned to 70 bytes.
        assert!(cache.observed_bytes.get().is_none());
        assert_eq!(directory_usage(&root).unwrap(), (120, 2));

        // The first insert observes what every process left and enforces
        // the bound.
        assert!(cache.insert(b"mine", vec![3; 5]));
        assert!(cache.observed_bytes.get().is_some());
        let (bytes, _) = directory_usage(&root).unwrap();
        assert!(bytes <= 70, "cache retained {bytes} bytes");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cache_scan_skips_an_entry_removed_before_metadata_lookup() {
        let root = temp_cache("removed-entry");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("vanished"), [1]).unwrap();
        let entry = fs::read_dir(&root)
            .unwrap()
            .next()
            .expect("cache entry should exist")
            .unwrap();
        fs::remove_file(entry.path()).unwrap();

        assert!(existing_entry_metadata(&entry).unwrap().is_none());
        let _ = fs::remove_dir_all(root);
    }
}

#[cfg(test)]
#[path = "../tests/unit/cache_atomic.rs"]
mod atomic_tests;
