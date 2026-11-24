//! Whole native modules, stored as Wasmtime serialized them and mapped back.
//!
//! This replaces Wasmtime's own module cache for product Engines. That cache
//! stores each module zstd-compressed, so every hit decompresses the whole
//! image into private anonymous memory. An entry here is the serialized
//! image itself: a hit maps the file, and its clean pages are page-cache
//! pages shared by every process that maps the same entry.

use crate::cache::{EntryPublication, FunctionCache};
use sha2::{Digest, Sha256};
use std::hash::{Hash, Hasher};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use wasmtime::{Engine, Module};

const KEY_DOMAIN: &[u8] = b"scriptw-native-module-v1";

#[derive(Debug)]
pub(crate) struct NativeModuleCache {
    store: FunctionCache,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl NativeModuleCache {
    pub(crate) fn new(directory: PathBuf, limit_bytes: u64) -> io::Result<Self> {
        Ok(Self {
            store: FunctionCache::new(directory, limit_bytes)?
                .with_publication(EntryPublication::SyncedBeforeRename),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        })
    }

    /// The module `compile` produces for `wasm` on `engine`, from this cache
    /// when it holds a usable entry.
    ///
    /// `compile` must compile exactly `wasm` with `engine`. An entry that
    /// cannot be loaded (absent, truncated, not this Engine's native code) is
    /// a miss, and the freshly compiled module replaces it. Failing to store
    /// the result never fails the compilation.
    pub(crate) fn load_or_compile<E>(
        &self,
        engine: &Engine,
        wasm: &[u8],
        compile: impl FnOnce() -> Result<Module, E>,
    ) -> Result<Module, E> {
        let key = entry_key(engine, wasm);
        if let Some(module) = self.load(engine, &key) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(module);
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        let module = compile()?;
        if let Ok(native) = module.serialize() {
            self.store.write(&key, native);
        }
        Ok(module)
    }

    fn load(&self, engine: &Engine, key: &[u8; 32]) -> Option<Module> {
        let file = self.store.open(key).ok()?;
        // SAFETY: the cache directory is trusted native-code storage, as it
        // was for Wasmtime's module cache. An entry is the output of
        // `Module::serialize`, published by one rename of an exclusively
        // created temporary file, and no Scriptw process writes to an entry
        // afterwards: replacing one renames over it and pruning unlinks it,
        // and either leaves this private mapping on the inode it opened.
        // Wasmtime checks the image's recorded target, compiler settings,
        // tunables, features and module version against `engine` before it
        // uses any of it.
        if let Ok(module) = unsafe { Module::deserialize_open_file(engine, file) } {
            return Some(module);
        }
        // A file mapping cannot be made executable when the cache directory
        // is on a `noexec` mount. Such an entry still loads from a private
        // copy; only a copy that also fails is a miss.
        let bytes = self.store.read(key)?;
        // SAFETY: the same entry and the same checks as above, on an owned
        // copy of its bytes.
        unsafe { Module::deserialize(engine, bytes) }.ok()
    }

    /// Process-wide `(hits, misses)` of `load_or_compile`.
    pub(crate) fn counters(&self) -> (u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
        )
    }
}

/// Selects an entry; it does not admit one. Wasmtime's own header check
/// decides whether the selected image belongs to `engine`.
///
/// The Engine part is Wasmtime's precompile compatibility hash: target, ISA
/// and compiler flags, tunables, features and the module version, which
/// product Engines set to the native compiler fingerprint. Two Engines that
/// agree on it accept each other's images.
fn entry_key(engine: &Engine, wasm: &[u8]) -> [u8; 32] {
    struct Sha256Hasher(Sha256);

    impl Hasher for Sha256Hasher {
        fn write(&mut self, bytes: &[u8]) {
            self.0.update(bytes);
        }

        fn finish(&self) -> u64 {
            unreachable!("the digest is taken from the SHA-256 state")
        }
    }

    let mut hasher = Sha256Hasher(Sha256::new());
    hasher.0.update(KEY_DOMAIN);
    hasher.0.update(Sha256::digest(wasm));
    // Last, because its length is whatever the `Hash` implementations write.
    engine.precompile_compatibility_hash().hash(&mut hasher);
    hasher.0.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use wasmtime::{Config, Instance, OptLevel, Store};

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "scriptw-native-module-cache-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            Self(path)
        }

        fn cache(&self) -> NativeModuleCache {
            NativeModuleCache::new(self.0.clone(), 64 * 1024 * 1024).unwrap()
        }

        fn only_entry(&self) -> PathBuf {
            let mut entries = fs::read_dir(&self.0)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>();
            assert_eq!(entries.len(), 1, "{entries:?}");
            entries.pop().unwrap()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn engine(version: &str, level: OptLevel) -> Engine {
        let mut config = Config::new();
        config.cranelift_opt_level(level);
        config
            .module_version(wasmtime::ModuleVersionStrategy::Custom(version.into()))
            .unwrap();
        Engine::new(&config).unwrap()
    }

    fn answer_wasm(value: u8) -> Vec<u8> {
        // (module (func (export "answer") (result i32) i32.const VALUE))
        vec![
            0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 2, 1, 0, 7, 10, 1, 6, 97, 110,
            115, 119, 101, 114, 0, 0, 10, 6, 1, 4, 0, 65, value, 11,
        ]
    }

    fn answer(engine: &Engine, module: &Module) -> i32 {
        let mut store = Store::new(engine, ());
        let instance = Instance::new(&mut store, module, &[]).unwrap();
        instance
            .get_typed_func::<(), i32>(&mut store, "answer")
            .unwrap()
            .call(&mut store, ())
            .unwrap()
    }

    /// Loads `wasm`, reporting whether it had to be compiled.
    fn load(cache: &NativeModuleCache, engine: &Engine, wasm: &[u8]) -> (Module, bool) {
        let mut compiled = false;
        let module = cache
            .load_or_compile(engine, wasm, || {
                compiled = true;
                Module::new(engine, wasm)
            })
            .unwrap();
        (module, compiled)
    }

    #[test]
    fn a_stored_module_is_mapped_by_another_cache_owner_and_runs() {
        let scratch = Scratch::new("round-trip");
        let engine = engine("a", OptLevel::None);
        let wasm = answer_wasm(42);

        let cold = scratch.cache();
        let (module, compiled) = load(&cold, &engine, &wasm);
        assert!(compiled);
        assert_eq!(answer(&engine, &module), 42);
        assert_eq!(cold.counters(), (0, 1));
        // The entry is the serialized image itself, published under its key.
        assert_eq!(
            fs::read(scratch.only_entry()).unwrap(),
            module.serialize().unwrap()
        );

        // A second owner stands for another process sharing the directory.
        // Its limit is below the entry's size: listing the directory would
        // prune the entry, and a hit must not list it.
        let warm = NativeModuleCache::new(scratch.0.clone(), 1).unwrap();
        let (module, compiled) = load(&warm, &engine, &wasm);
        assert!(!compiled);
        assert_eq!(answer(&engine, &module), 42);
        assert_eq!(warm.counters(), (1, 0));
        assert!(scratch.only_entry().is_file());

        // The mapping outlives its directory entry: pruning or replacing an
        // entry another process is running leaves that process's code intact.
        fs::remove_file(scratch.only_entry()).unwrap();
        assert_eq!(answer(&engine, &module), 42);
    }

    #[test]
    fn a_damaged_entry_is_a_miss_and_is_rewritten() {
        let scratch = Scratch::new("damaged");
        let engine = engine("a", OptLevel::None);
        let wasm = answer_wasm(7);
        let (module, _) = load(&scratch.cache(), &engine, &wasm);
        let entry = scratch.only_entry();
        let intact = module.serialize().unwrap();
        drop(module);

        let damaged: [(&str, Vec<u8>); 4] = [
            ("empty", Vec::new()),
            ("truncated", intact[..intact.len() / 2].to_vec()),
            ("not an image", b"not native code".repeat(64)),
            ("zero-filled", vec![0; intact.len()]),
        ];
        for (name, bytes) in damaged {
            fs::write(&entry, bytes).unwrap();
            let cache = scratch.cache();
            let (module, compiled) = load(&cache, &engine, &wasm);
            assert!(compiled, "{name}");
            assert_eq!(cache.counters(), (0, 1), "{name}");
            assert_eq!(answer(&engine, &module), 7, "{name}");
            assert_eq!(fs::read(&entry).unwrap(), intact, "{name}");
            assert!(!load(&scratch.cache(), &engine, &wasm).1, "{name}");
        }
    }

    #[test]
    fn the_key_follows_the_wasm_and_the_engine_compatibility_hash() {
        let engine_a = engine("a", OptLevel::None);
        let wasm = answer_wasm(1);
        let key = entry_key(&engine_a, &wasm);
        // Equal configuration is one key, whichever Engine object asks.
        assert_eq!(key, entry_key(&engine("a", OptLevel::None), &wasm));
        assert_ne!(key, entry_key(&engine_a, &answer_wasm(2)));
        // The module version carries the native compiler fingerprint.
        assert_ne!(key, entry_key(&engine("b", OptLevel::None), &wasm));
        assert_ne!(key, entry_key(&engine("a", OptLevel::Speed), &wasm));

        // Distinct keys are distinct entries: neither Engine nor Wasm reads
        // the other's module.
        let scratch = Scratch::new("keys");
        let cache = scratch.cache();
        assert!(load(&cache, &engine_a, &wasm).1);
        assert!(load(&cache, &engine("b", OptLevel::None), &wasm).1);
        assert!(load(&cache, &engine_a, &answer_wasm(2)).1);
        assert!(!load(&cache, &engine_a, &wasm).1);
        assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 3);
    }

    #[test]
    fn an_image_of_an_incompatible_engine_under_this_key_is_refused() {
        // The key only selects. Whatever sits under it, Wasmtime's header
        // check against the loading Engine decides.
        let scratch = Scratch::new("foreign");
        let ours = engine("a", OptLevel::None);
        let foreign = engine("b", OptLevel::None);
        let wasm = answer_wasm(9);
        load(&scratch.cache(), &ours, &wasm);
        let entry = scratch.only_entry();
        let foreign_image = Module::new(&foreign, answer_wasm(3))
            .unwrap()
            .serialize()
            .unwrap();
        fs::write(&entry, &foreign_image).unwrap();

        let (module, compiled) = load(&scratch.cache(), &ours, &wasm);
        assert!(compiled);
        assert_eq!(answer(&ours, &module), 9);
        assert_ne!(fs::read(&entry).unwrap(), foreign_image);
    }
}
