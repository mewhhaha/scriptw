//! Per-Engine native admission and deserialization of the linked Cargo image.
use super::native_backing::NativeLoadPath;
use super::*;

impl EmbeddedNativeRuntime {
    /// Pairing admission: the digest the Cargo packaging step attested for
    /// the bytes it linked must be the producer's manifest digest. The bytes
    /// are not hashed again here; `scriptw_runtime_image::linked_native`
    /// states why origin and pairing are what a load can establish.
    pub(super) fn accepts_native(&self, image: &scriptw_runtime_image::LinkedNativeImage) -> bool {
        image.build_digest() == &self.manifest.native_digest
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
        self.load_admitted(engine, mode, timings)
    }

    pub(super) fn load_admitted(
        &self,
        engine: &Engine,
        mode: WasmNativeCompilationMode,
        timings: &mut crate::WasmRuntimeEmbeddedImageTimings,
    ) -> Option<Module> {
        let image = match self.images.profile {
            RuntimeSourceCompilationProfile::NativeHost => scriptw_runtime_image::native()?,
            RuntimeSourceCompilationProfile::Portable => scriptw_runtime_image::portable_native()?,
        };
        if !self.accepts_native(&image) {
            return None;
        }
        // Reuse only the proved executable range. Every Engine still creates and
        // admits its own Module; the existing module-cache budget is unchanged.
        let backing_started = std::time::Instant::now();
        let backing = self
            .native_backing
            .get_or_init(|| native_backing::NativeBacking::from_cargo(&image));
        timings.native_backing_preparation = backing_started.elapsed();
        if let Some(backing) = backing {
            let started = std::time::Instant::now();
            // SAFETY: this owner admitted Cargo source/profile/target/digests
            // before construction, matches() checked this actual R key/mode,
            // and accepts_native() paired this image with the manifest.
            let loaded = unsafe { backing.deserialize(engine) };
            timings.deserialize += started.elapsed();
            match loaded {
                Ok(module) => {
                    self.trace_hit(backing.bytes(), NativeLoadPath::ExecutableMapping, mode);
                    return Some(module);
                }
                Err(error) => {
                    if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                        eprintln!(
                            "scriptw wasm trace: runtime-native-bundle executable mapping unavailable: {error:#}"
                        );
                    }
                }
            }
        }
        let bytes = image.bytes();
        // SAFETY: bytes are exclusively the immutable linked Cargo output of
        // the checked raw/native runtime producer, paired with the manifest
        // above. Source/target/config/modes/raw key have already matched. No
        // caller slice/path/cache blob enters this seam. Wasmtime copies them
        // into its own mapping. deserialize_raw, which would borrow memory
        // Wasmtime cannot make executable, is not used. Compatibility is
        // checked by the actual destination Engine; no build Engine or Module
        // is cached here.
        let started = std::time::Instant::now();
        let native_length = bytes.len();
        let loaded = unsafe { Module::deserialize(engine, bytes) };
        timings.deserialize += started.elapsed();
        if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
            eprintln!(
                "scriptw wasm trace: runtime-native-bundle deserialization: {:?}",
                started.elapsed()
            );
        }
        match loaded {
            Ok(module) => {
                self.trace_hit(native_length as u64, NativeLoadPath::Copied, mode);
                Some(module)
            }
            Err(error) => {
                if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                    eprintln!("scriptw wasm trace: runtime-native-bundle incompatible: {error:#}");
                }
                None
            }
        }
    }

    /// Both admitted load paths report the same hit line; only `backing`
    /// differs.
    fn trace_hit(
        &self,
        native_bytes: u64,
        backing: NativeLoadPath,
        mode: WasmNativeCompilationMode,
    ) {
        if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
            eprintln!(
                "scriptw wasm trace: runtime-native-bundle hit: {} bytes, backing: {} (runtime: {}, execution: {}, source: {:?})",
                native_bytes,
                backing.as_str(),
                self.manifest.modes.runtime.as_str(),
                mode.as_str(),
                self.images.profile
            );
        }
    }
}
