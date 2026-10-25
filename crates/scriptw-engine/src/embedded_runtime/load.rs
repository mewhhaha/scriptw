//! Per-Engine native admission and a transient private Cargo-byte lease.
use super::*;

impl EmbeddedNativeRuntime {
    pub(super) fn accepts_native(&self, image: &scriptw_runtime_image::NativeRuntimeBytes) -> bool {
        image.digest() == &self.manifest.native_digest
    }

    pub(super) fn load(
        &self,
        engine: &Engine,
        runtime: &RuntimeArtifact,
        mode: WasmNativeCompilationMode,
        timings: &mut crate::WasmRuntimeEmbeddedImageTimings,
    ) -> Option<Module> {
        if !self.matches(runtime, mode) {
            return None;
        }
        // Reuse only immutable backing storage. Every Engine still creates and
        // admits its own Module; the existing module-cache budget is unchanged.
        if let Some(Some(backing)) = self.native_backing.get() {
            let started = std::time::Instant::now();
            // SAFETY: this owner admitted Cargo source/profile/target/digests
            // before construction, and matches() checked this actual R key/mode.
            if let Ok(module) = unsafe { backing.deserialize(engine) } {
                timings.deserialize = started.elapsed();
                return Some(module);
            }
        }
        // The weak lease shares only concurrent loads, never a mutable Store
        // or a Module. Its private constructor hashes the exact decoded Box;
        // the independent v5 manifest must admit that digest before deserialize.
        let lease_started = std::time::Instant::now();
        let lease = match self.images.profile {
            RuntimeSourceCompilationProfile::NativeHost => scriptw_runtime_image::native()?,
            RuntimeSourceCompilationProfile::Portable => scriptw_runtime_image::portable_native()?,
        };
        if !self.accepts_native(&lease) {
            return None;
        }
        timings.native_lease_decode_and_digest = lease_started.elapsed();
        let bytes = lease.bytes();
        // SAFETY: bytes are exclusively the immutable Cargo output of the
        // checked raw/native runtime producer. Each exact decoded digest is
        // independently admitted above. Source/target/config/modes/raw key have
        // already matched. No caller slice/path/cache blob enters this seam.
        // Wasmtime owns its file-backed or copied mapping. deserialize_raw,
        // which would borrow caller-owned executable memory, is not used.
        // Compatibility is checked by the actual destination Engine; no build
        // Engine or Module is cached here. Dropping the lease after this call
        // cannot invalidate the Module's separate Arc<CodeMemory> mapping.
        let started = std::time::Instant::now();
        let native_length = bytes.len();
        let backing = self
            .native_backing
            .get_or_init(|| native_backing::NativeBacking::create(bytes));
        let loaded = match backing {
            Some(backing) => {
                // SAFETY: exact admitted Cargo native bytes are privately
                // written once. No caller path/cache/native blob enters here.
                unsafe { backing.deserialize(engine) }
                    .or_else(|_| unsafe { Module::deserialize(engine, bytes) })
            }
            // Read-only or unsupported filesystems keep the same native module
            // and ordinary loader. No compiler or Wasm representation changes.
            None => unsafe { Module::deserialize(engine, bytes) },
        };
        timings.deserialize = started.elapsed();
        drop(lease);
        if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
            eprintln!(
                "scriptw wasm trace: runtime-native-bundle deserialization: {:?}",
                started.elapsed()
            );
        }
        match loaded {
            Ok(module) => {
                if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                    eprintln!(
                        "scriptw wasm trace: runtime-native-bundle hit: {} bytes (runtime: {}, execution: {}, source: {:?})",
                        native_length,
                        self.manifest.modes.runtime.as_str(),
                        mode.as_str(),
                        self.images.profile
                    );
                }
                Some(module)
            }
            Err(error) => {
                if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                    eprintln!("scriptw wasm trace: runtime-native-bundle incompatible: {error:#}; compiling normally");
                }
                None
            }
        }
    }
}
