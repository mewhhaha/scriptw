//! Build-only storage: Wasmtime's own module cache, with exact native admission.
//!
//! Only the Cargo runtime-image producer opens it. Product Engines map whole
//! modules from Scriptw's native-module tier instead.
use std::{fs, io, path::PathBuf};
use wasmtime::{Cache, CacheConfig};

pub(super) fn module_cache() -> io::Result<Cache> {
    let directory = std::env::var_os("SCRIPTW_BUILD_MODULE_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(crate::cache::build_module_cache_directory);
    module_cache_at(directory, crate::cache::module_cache_limit_bytes())
}

pub(super) fn module_cache_at(directory: PathBuf, limit_bytes: u64) -> io::Result<Cache> {
    fs::create_dir_all(&directory)?;
    // Wasmtime prunes on a background worker, which a build script can exit
    // before, so enforce the same bound synchronously whenever the producer
    // opens the cache. This lists the directory.
    crate::cache::prune_directory_to_limit(&directory, limit_bytes)?;
    let percent = crate::cache::CACHE_PRUNE_PERCENT as u8;
    let mut config = CacheConfig::new();
    config
        .with_directory(directory)
        .with_files_total_size_soft_limit(limit_bytes)
        .with_files_total_size_limit_percent_if_deleting(percent)
        .with_file_count_limit_percent_if_deleting(percent)
        // Ask the long-lived process worker to check after every update too.
        .with_cleanup_interval(std::time::Duration::ZERO);
    Cache::new(config).map_err(io::Error::other)
}
