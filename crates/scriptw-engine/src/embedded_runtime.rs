//! Native R authority is limited to this Cargo build's embedded output.
//!
//! Raw package bytes retain complete identity, metadata, data and layout
//! admission. Immutable Cargo output reuses the producer's full body validation.
//! No native constructor accepts a caller slice/path/cache blob, and no Module
//! is retained here.

#[cfg(test)]
#[path = "embedded_runtime/compile.rs"]
mod build_compile;
mod digests;
mod images;
mod load;
mod manifest;
mod matched_capacity;
mod native_backing;
use crate::wasmtime_config::{WasmNativeCompilationMode, CONFIGURATION_SCHEMA};
pub(super) use matched_capacity::load as load_capacity_matched_native;
use scriptw_aot_wasm::{
    RuntimeArtifact, RuntimeArtifactCache, RuntimeArtifactInputs, RuntimeSourceCompilationProfile,
};
use std::sync::OnceLock;
use wasmtime::{Engine, Module};

// The leaf crate owns the large initializers. Only non-inline accessor calls
// enter Engine metadata; source and native admission remain local here.
use images::Images;

fn expected_source() -> &'static [u8; 32] {
    static SOURCE: OnceLock<[u8; 32]> = OnceLock::new();
    SOURCE.get_or_init(|| {
        manifest::source_identity(env!("SCRIPTW_COMPILER_FINGERPRINT"))
            .expect("Cargo supplied a SHA-256 compiler source identity")
    })
}

/// Construct only a lazy raw candidate. Scalar-only programs do not hash/admit
/// the package or native image and do not resolve any Intl component here.
pub(super) fn inputs(
    cache: Option<&dyn RuntimeArtifactCache>,
    source_compilation_profile: RuntimeSourceCompilationProfile,
) -> RuntimeArtifactInputs<'_> {
    let inputs = RuntimeArtifactInputs::new(cache)
        .with_source_compilation_profile(source_compilation_profile)
        .with_admission_jobs(
            std::num::NonZeroUsize::new(crate::configured_compilation_jobs().unwrap_or(1))
                .expect("explicit compilation jobs are positive"),
        );
    let package = Images::package_for_profile(source_compilation_profile);
    if package.is_empty() {
        inputs
    } else {
        // SAFETY: The selected package is the immutable Cargo-owned array from
        // scriptw-runtime-image. Its sole producer round-trips these exact package
        // bytes through complete AOT body/layout validation before native
        // compilation and publication. expected_source() is immutable and
        // build-bound. No external slice, path or cache reaches this boundary.
        // AOT still checks source/profile/Intl identity, checksums, sections,
        // data and the reconstructed layout before using the candidate.
        unsafe {
            inputs.with_immutable_minimal_build(
                package,
                expected_source(),
                scriptw_runtime_image::intl_metadata(),
            )
        }
    }
}

struct EmbeddedNativeRuntime {
    manifest: manifest::Manifest,
    images: Images,
    native_backing: OnceLock<Option<native_backing::NativeBacking>>,
}

impl EmbeddedNativeRuntime {
    fn admit(images: Images) -> Option<Self> {
        Self::admit_manifest(images, manifest::Manifest::decode(images.manifest)?)
    }

    fn admit_manifest(images: Images, manifest: manifest::Manifest) -> Option<Self> {
        if &manifest.source != expected_source()
            || manifest.target != env!("SCRIPTW_RUNTIME_BUILD_TARGET")
            || manifest.configuration != CONFIGURATION_SCHEMA
            || manifest.modes != manifest::COMPILATION_MODES
        {
            return None;
        }
        let started = std::env::var_os("SCRIPTW_WASM_TRACE").map(|_| std::time::Instant::now());
        // Both native images are bounded compressed storage. Their exact
        // decoded digest is independently admitted on the first actual load.
        let package_digest = digests::digest(images.package);
        if let Some(started) = started {
            eprintln!(
                "scriptw wasm trace: runtime-native-bundle digest admission: {:?}",
                started.elapsed()
            );
        }
        if manifest.package_digest != package_digest {
            return None;
        }
        Some(Self {
            manifest,
            images,
            native_backing: OnceLock::new(),
        })
    }

    fn matches(&self, runtime: &RuntimeArtifact, mode: WasmNativeCompilationMode) -> bool {
        self.images.profile == runtime.source_compilation_profile()
            && self.manifest.modes.execution.contains(mode)
            && runtime.key().as_bytes() == &self.manifest.runtime_key
    }
}

/// Called only by the ordinary shared module-cache factory on an R miss or a
/// bypass. Every caller receives its own Wasmtime mapping; retained mappings
/// are owned/accounted solely by the existing bounded R/P cache. Its mode key
/// remains the execution Engine's mode, never the R image's optimization mode.
pub(super) fn load_native(
    engine: &Engine,
    runtime: &RuntimeArtifact,
    mode: WasmNativeCompilationMode,
    timings: &mut crate::WasmRuntimeEmbeddedImageTimings,
) -> Option<Module> {
    let admission_started = std::time::Instant::now();
    // Unadmitted modes and different raw R keys do not touch the large
    // embedded images. This cheap metadata check can only reject a candidate;
    // the private authority still validates all manifest fields and digests.
    let images = Images::for_profile(runtime.source_compilation_profile());
    let candidate = manifest::Manifest::decode(images.manifest)?;
    if !candidate.modes.execution.contains(mode)
        || runtime.key().as_bytes() != &candidate.runtime_key
    {
        return None;
    }
    static NATIVE_HOST: OnceLock<Option<EmbeddedNativeRuntime>> = OnceLock::new();
    static PORTABLE: OnceLock<Option<EmbeddedNativeRuntime>> = OnceLock::new();
    let bundle = match images.profile {
        RuntimeSourceCompilationProfile::NativeHost => &NATIVE_HOST,
        RuntimeSourceCompilationProfile::Portable => &PORTABLE,
    };
    let admitted = bundle
        .get_or_init(|| EmbeddedNativeRuntime::admit(images))
        .as_ref()?;
    timings.admission = admission_started.elapsed();
    admitted.load(engine, runtime, mode, timings)
}

#[cfg(test)]
mod tests;
