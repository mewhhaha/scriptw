use super::{WasmModuleMemoryCacheKey, WasmtimeModule};
mod native_templates;
#[cfg(feature = "native-source-compiler")]
pub(super) use native_templates::MemoryNativeInstanceCache;
use std::collections::VecDeque;
use std::num::NonZeroUsize;

/// These limits bound references retained by this cache, not active modules,
/// compilation work, Wasmtime metadata, JavaScript heaps or process RSS.
const DEFAULT_ENTRIES: NonZeroUsize = NonZeroUsize::new(64).unwrap();
const DEFAULT_LIMIT_BYTES: NonZeroUsize = NonZeroUsize::new(512 * 1024 * 1024).unwrap();
const ENTRIES_ENV: &str = "SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES";
const LIMIT_BYTES_ENV: &str = "SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES";

struct MemoryModuleCacheLimits {
    entries: NonZeroUsize,
    image_bytes: NonZeroUsize,
}

impl MemoryModuleCacheLimits {
    fn from_env() -> Self {
        Self {
            entries: parse_positive_limit(
                std::env::var(ENTRIES_ENV).ok().as_deref(),
                DEFAULT_ENTRIES,
            ),
            image_bytes: parse_positive_limit(
                std::env::var(LIMIT_BYTES_ENV).ok().as_deref(),
                DEFAULT_LIMIT_BYTES,
            ),
        }
    }

    fn has_image_room(&self, retained: usize, incoming: usize) -> bool {
        self.image_bytes
            .get()
            .checked_sub(incoming)
            .is_some_and(|remaining| retained <= remaining)
    }
}

fn parse_positive_limit(raw: Option<&str>, fallback: NonZeroUsize) -> NonZeroUsize {
    raw.map(str::trim)
        .and_then(|value| value.parse::<usize>().ok())
        .and_then(NonZeroUsize::new)
        .unwrap_or(fallback)
}

struct RetainedWasmModule {
    key: WasmModuleMemoryCacheKey,
    module: WasmtimeModule,
    image_bytes: usize,
}

/// The deque and its aggregate have one mutation owner. Entry admission always
/// measures the actual immutable Wasmtime compilation image; callers cannot
/// substitute an input-Wasm size or separately adjust accounting.
pub(super) struct MemoryWasmModuleCache {
    entries: VecDeque<RetainedWasmModule>,
    retained_image_bytes: usize,
    limits: MemoryModuleCacheLimits,
}

/// Actual limits and retained compilation-image accounting from the cache's
/// owner. These are not input-Wasm sizes, GC heap sizes or process RSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WasmRuntimeModuleCacheObservation {
    pub entry_limit: usize,
    pub image_byte_limit: usize,
    pub retained_entries: usize,
    pub retained_image_bytes: usize,
}

pub fn wasm_runtime_module_cache_observation() -> WasmRuntimeModuleCacheObservation {
    super::memory_wasm_modules()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .observation()
}

impl MemoryWasmModuleCache {
    fn observation(&self) -> WasmRuntimeModuleCacheObservation {
        WasmRuntimeModuleCacheObservation {
            entry_limit: self.limits.entries.get(),
            image_byte_limit: self.limits.image_bytes.get(),
            retained_entries: self.entries.len(),
            retained_image_bytes: self.retained_image_bytes,
        }
    }
    pub(super) fn from_env() -> Self {
        Self::new(MemoryModuleCacheLimits::from_env())
    }

    fn new(limits: MemoryModuleCacheLimits) -> Self {
        Self {
            entries: VecDeque::new(),
            retained_image_bytes: 0,
            limits,
        }
    }

    pub(super) fn get(&mut self, key: &WasmModuleMemoryCacheKey) -> Option<WasmtimeModule> {
        let index = self.entries.iter().position(|entry| entry.key == *key)?;
        let entry = self.entries.remove(index).expect("cache hit index exists");
        let module = entry.module.clone();
        // Promotion transfers the same measured entry, without changing bytes.
        self.entries.push_back(entry);
        Some(module)
    }

    pub(super) fn retain_compiled(
        &mut self,
        key: WasmModuleMemoryCacheKey,
        module: WasmtimeModule,
    ) -> WasmtimeModule {
        // Compilation happens outside the mutex. Another worker can have
        // admitted this key since the initial miss; keep only its hot entry.
        if let Some(existing) = self.get(&key) {
            return existing;
        }

        let Some(image_bytes) = module_image_bytes(&module) else {
            return module;
        };
        if image_bytes > self.limits.image_bytes.get() {
            // A valid oversized module still executes. It must not displace
            // retained modules when it cannot itself enter the cache.
            return module;
        }

        while self.entries.len() >= self.limits.entries.get()
            || !self
                .limits
                .has_image_room(self.retained_image_bytes, image_bytes)
        {
            let oldest = self.entries.pop_front().expect("full cache has an entry");
            self.retained_image_bytes = self
                .retained_image_bytes
                .checked_sub(oldest.image_bytes)
                .expect("retained entry bytes belong to the aggregate");
        }

        // The subtraction-based capacity check proves the exact sum fits both
        // the configured budget and usize, including a usize::MAX override.
        self.retained_image_bytes = self
            .retained_image_bytes
            .checked_add(image_bytes)
            .expect("admitted module image fits the byte budget");
        self.entries.push_back(RetainedWasmModule {
            key,
            module: module.clone(),
            image_bytes,
        });
        module
    }

    #[cfg(test)]
    pub(super) fn contains_wasm_sha256(&self, wasm_sha256: &[u8; 32]) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.key.wasm_sha256 == *wasm_sha256)
    }
}

fn module_image_bytes(module: &WasmtimeModule) -> Option<usize> {
    let image = module.image_range();
    // Safe address observation only: neither pointer is dereferenced or
    // mutated, and the compilation image is never serialized for accounting.
    image.end.addr().checked_sub(image.start.addr())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WasmNativeCompilationMode, WasmtimeEngine};
    use std::sync::OnceLock;

    fn key(identity: u8) -> WasmModuleMemoryCacheKey {
        WasmModuleMemoryCacheKey {
            native_compilation_mode: WasmNativeCompilationMode::Fast,
            runtime_image_policy: crate::WasmRuntimeImagePolicy::EmbeddedBuild,
            wasm_sha256: [identity; 32],
        }
    }

    fn limits(entries: usize, image_bytes: usize) -> MemoryModuleCacheLimits {
        MemoryModuleCacheLimits {
            entries: NonZeroUsize::new(entries).expect("test entry budget is positive"),
            image_bytes: NonZeroUsize::new(image_bytes).expect("test byte budget is positive"),
        }
    }

    fn small_module() -> WasmtimeModule {
        static MODULE: OnceLock<WasmtimeModule> = OnceLock::new();
        MODULE
            .get_or_init(|| {
                WasmtimeModule::new(&WasmtimeEngine::default(), b"\0asm\x01\0\0\0")
                    .expect("empty binary module should compile")
            })
            .clone()
    }

    fn push_leb128(bytes: &mut Vec<u8>, mut value: usize) {
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            bytes.push(byte);
            if value == 0 {
                return;
            }
        }
    }

    fn large_module() -> WasmtimeModule {
        static MODULE: OnceLock<WasmtimeModule> = OnceLock::new();
        MODULE
            .get_or_init(|| {
                let data_bytes = module_image_bytes(&small_module())
                    .expect("small image range is ordered")
                    .checked_mul(4)
                    .expect("small image size can be multiplied");
                let mut data = vec![1, 1]; // One passive data segment.
                push_leb128(&mut data, data_bytes);
                data.extend(std::iter::repeat_n(0xa5, data_bytes));
                let mut wasm = b"\0asm\x01\0\0\0".to_vec();
                wasm.push(11); // Data section.
                push_leb128(&mut wasm, data.len());
                wasm.extend_from_slice(&data);
                WasmtimeModule::new(&WasmtimeEngine::default(), &wasm)
                    .expect("passive-data binary module should compile")
            })
            .clone()
    }

    fn image_bytes(module: &WasmtimeModule) -> usize {
        let bytes = module_image_bytes(module).expect("module image range is ordered");
        assert!(bytes > 0, "fixture compilation image must be nonempty");
        bytes
    }

    fn assert_state(cache: &MemoryWasmModuleCache, identities: &[u8], retained_bytes: usize) {
        let observed = cache.observation();
        assert_eq!(observed.entry_limit, cache.limits.entries.get());
        assert_eq!(observed.image_byte_limit, cache.limits.image_bytes.get());
        assert_eq!(observed.retained_entries, identities.len());
        assert_eq!(observed.retained_image_bytes, retained_bytes);
        let keys: Vec<_> = cache.entries.iter().map(|entry| entry.key).collect();
        assert_eq!(
            keys,
            identities.iter().copied().map(key).collect::<Vec<_>>()
        );
        assert_eq!(cache.retained_image_bytes, retained_bytes);
        assert_eq!(
            cache
                .entries
                .iter()
                .map(|entry| entry.image_bytes)
                .sum::<usize>(),
            retained_bytes
        );
        assert!(keys.len() <= cache.limits.entries.get());
        assert!(retained_bytes <= cache.limits.image_bytes.get());
    }

    #[test]
    fn memory_cache_limits_accept_only_positive_sizes_and_keep_defaults() {
        for invalid in [
            None,
            Some(""),
            Some(" \n "),
            Some("0"),
            Some("-1"),
            Some("bad"),
        ] {
            assert_eq!(
                parse_positive_limit(invalid, DEFAULT_ENTRIES),
                DEFAULT_ENTRIES
            );
            assert_eq!(
                parse_positive_limit(invalid, DEFAULT_LIMIT_BYTES),
                DEFAULT_LIMIT_BYTES
            );
        }
        let overflow = format!("{}0", usize::MAX);
        assert_eq!(
            parse_positive_limit(Some(&overflow), DEFAULT_LIMIT_BYTES),
            DEFAULT_LIMIT_BYTES
        );
        assert_eq!(
            parse_positive_limit(Some("  7  "), DEFAULT_ENTRIES).get(),
            7
        );
        assert_eq!(
            parse_positive_limit(Some("  1048576  "), DEFAULT_LIMIT_BYTES).get(),
            1048576
        );
        assert_eq!(
            parse_positive_limit(Some(&usize::MAX.to_string()), DEFAULT_LIMIT_BYTES).get(),
            usize::MAX
        );
    }

    #[test]
    fn memory_cache_capacity_handles_exact_limits_without_integer_overflow() {
        let maximum = limits(1, usize::MAX);
        assert!(maximum.has_image_room(usize::MAX - 7, 7));
        assert!(!maximum.has_image_room(usize::MAX - 7, 8));
        assert!(maximum.has_image_room(0, usize::MAX));
        assert!(!maximum.has_image_room(1, usize::MAX));
        assert!(!maximum.has_image_room(usize::MAX, 1));
        assert!(maximum.has_image_room(usize::MAX, 0));
        assert!(!limits(1, 1).has_image_room(0, usize::MAX));
    }

    #[test]
    fn memory_cache_entry_budget_evicts_the_oldest_without_byte_pressure() {
        let module = small_module();
        let bytes = image_bytes(&module);
        let mut cache = MemoryWasmModuleCache::new(limits(2, bytes * 3));
        for identity in [1, 2, 3] {
            cache.retain_compiled(key(identity), module.clone());
        }
        assert_state(&cache, &[2, 3], bytes * 2);
        assert!(cache.get(&key(1)).is_none());
    }

    #[test]
    fn memory_cache_byte_budget_evicts_multiple_entries_to_the_exact_boundary() {
        let small = small_module();
        let large = large_module();
        let small_bytes = image_bytes(&small);
        let large_bytes = image_bytes(&large);
        assert!(
            large_bytes >= small_bytes * 2,
            "data is part of the compilation image"
        );
        let budget = large_bytes + small_bytes;
        let mut cache = MemoryWasmModuleCache::new(limits(10, budget));
        for identity in [1, 2, 3] {
            cache.retain_compiled(key(identity), small.clone());
        }
        assert_state(&cache, &[1, 2, 3], small_bytes * 3);
        cache.retain_compiled(key(4), large);
        assert_state(&cache, &[3, 4], budget);
    }

    #[test]
    fn memory_cache_hit_promotes_without_changing_accounting() {
        let module = small_module();
        let bytes = image_bytes(&module);
        let mut cache = MemoryWasmModuleCache::new(limits(2, bytes * 2));
        cache.retain_compiled(key(1), module.clone());
        cache.retain_compiled(key(2), module.clone());
        let hit = cache.get(&key(1)).expect("first module should hit");
        assert!(WasmtimeModule::same(&hit, &module));
        assert_state(&cache, &[2, 1], bytes * 2);
        assert!(cache.get(&key(9)).is_none());
        assert_state(&cache, &[2, 1], bytes * 2);
        cache.retain_compiled(key(3), module);
        assert_state(&cache, &[1, 3], bytes * 2);
    }

    #[test]
    fn memory_cache_oversized_module_returns_for_execution_without_evicting_hot_entries() {
        let small = small_module();
        let large = large_module();
        let bytes = image_bytes(&small);
        assert!(image_bytes(&large) > bytes);
        let mut cache = MemoryWasmModuleCache::new(limits(2, bytes));
        cache.retain_compiled(key(1), small.clone());
        let execution_module = cache.retain_compiled(key(2), large.clone());
        assert!(WasmtimeModule::same(&execution_module, &large));
        assert_state(&cache, &[1], bytes);
        assert!(cache.get(&key(2)).is_none());
        assert!(cache.get(&key(1)).is_some());
        assert_state(&cache, &[1], bytes);
    }

    #[test]
    fn memory_cache_duplicate_after_an_unlocked_miss_reuses_and_promotes_the_winner() {
        let winner = small_module();
        let late_compilation = large_module();
        let bytes = image_bytes(&winner);
        let mut cache = MemoryWasmModuleCache::new(limits(2, bytes * 2));
        assert!(cache.get(&key(1)).is_none());
        cache.retain_compiled(key(1), winner.clone());
        cache.retain_compiled(key(2), winner.clone());

        let execution_module = cache.retain_compiled(key(1), late_compilation.clone());
        assert!(WasmtimeModule::same(&execution_module, &winner));
        assert!(!WasmtimeModule::same(&execution_module, &late_compilation));
        assert_state(&cache, &[2, 1], bytes * 2);
        cache.retain_compiled(key(3), winner);
        assert_state(&cache, &[1, 3], bytes * 2);
        assert!(cache.contains_wasm_sha256(&key(1).wasm_sha256));
        assert!(!cache.contains_wasm_sha256(&key(2).wasm_sha256));
    }
}
