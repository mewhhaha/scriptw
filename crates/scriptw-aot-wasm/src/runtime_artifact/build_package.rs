//! Build-owned raw R packages retain canonical identity and layout admission.
//!
//! The package is a candidate, never authority for unchecked layout or native
//! code. Its source identity deliberately excludes the eventual executable.
//! Safe candidates receive complete body validation. The explicitly unsafe
//! immutable Cargo boundary can reuse the producer's full body validation.

use super::validation::FunctionBodyValidation;
use super::{
    cache, emit_runtime_artifact, CurrentRuntimeLayoutVersion, RuntimeArtifact,
    RuntimeArtifactCache, RuntimeArtifactKey, RuntimeSourceCompilationProfile, LAYOUT_VERSION,
};
use crate::EmitError;
use sha2::{Digest, Sha256};
use std::sync::Arc;

mod envelope;
#[cfg(test)]
use envelope::decode_with_jobs;
use envelope::{decode, decode_immutable_build, encode};

const MAGIC: &[u8; 8] = b"SCRWRB01";
const DIGEST_BYTES: usize = 32;

/// An exact emitted runtime paired with its fully checked raw package. There
/// is no constructor accepting a caller-supplied layout or native image.
pub struct RuntimeBuildArtifact {
    runtime: Arc<RuntimeArtifact>,
    package: Vec<u8>,
}

impl RuntimeBuildArtifact {
    pub fn wasm(&self) -> &[u8] {
        self.runtime.bytes()
    }
    pub fn key(&self) -> RuntimeArtifactKey {
        self.runtime.key()
    }
    pub fn package_bytes(&self) -> &[u8] {
        &self.package
    }
}

/// Persist an already emitted portable R. This does not consult a cache or
/// emit a replacement. The selected Intl data and every Wasm body/layout are
/// checked by the same safe admission as a disk package.
pub fn encode_portable_runtime_package(
    runtime: &RuntimeArtifact,
    selection: &scriptw_intl::IntlDataSelection,
    compiler_source: &[u8; DIGEST_BYTES],
) -> Result<Vec<u8>, EmitError> {
    if runtime.source_compilation_profile() != RuntimeSourceCompilationProfile::Portable {
        return Err(EmitError::unsupported(
            "portable package requires a portable runtime",
        ));
    }
    let identity =
        cache::RuntimeIdentity::admit(selection, RuntimeSourceCompilationProfile::Portable)?;
    let package = encode(runtime, &identity, compiler_source);
    let admitted = admit_portable_runtime_package(&package, selection, compiler_source)?;
    if admitted.key() != runtime.key() || admitted.layout() != runtime.layout() {
        return Err(EmitError::unsupported(
            "portable package changed the runtime identity or layout",
        ));
    }
    Ok(package)
}

/// Strict safe raw R admission. Invalid, mismatched or missing bytes are an
/// error; this API has no emitter/cache fallback and grants no native compiler
/// or machine-code deserialization authority.
pub fn admit_portable_runtime_package(
    package: &[u8],
    selection: &scriptw_intl::IntlDataSelection,
    compiler_source: &[u8; DIGEST_BYTES],
) -> Result<Arc<RuntimeArtifact>, EmitError> {
    let identity =
        cache::RuntimeIdentity::admit(selection, RuntimeSourceCompilationProfile::Portable)?;
    decode(package, &identity, compiler_source)
        .map(Arc::new)
        .ok_or_else(|| {
            EmitError::unsupported(
                "portable runtime package failed source, Intl, checksum, Wasm or layout admission",
            )
        })
}

/// Emit and round-trip one raw R for a build producer before it precompiles
/// these exact Wasm bytes. The normal executable-bound cache is not consulted.
pub fn build_runtime_artifact(
    selection: &scriptw_intl::IntlDataSelection,
    compiler_source: &[u8; DIGEST_BYTES],
) -> Result<RuntimeBuildArtifact, EmitError> {
    build_runtime_artifact_with_source_compilation_profile(
        selection,
        compiler_source,
        RuntimeSourceCompilationProfile::Portable,
    )
}

/// A native-image producer explicitly selects native source compilation.
/// Portable artifacts use the ordinary checked wrapper above.
pub fn build_runtime_artifact_with_source_compilation_profile(
    selection: &scriptw_intl::IntlDataSelection,
    compiler_source: &[u8; DIGEST_BYTES],
    source_compilation_profile: RuntimeSourceCompilationProfile,
) -> Result<RuntimeBuildArtifact, EmitError> {
    let identity = cache::RuntimeIdentity::admit(selection, source_compilation_profile)?;
    let runtime = emit_runtime_artifact(selection, source_compilation_profile)?;
    let package = encode(&runtime, &identity, compiler_source);
    let admitted = decode(&package, &identity, compiler_source).ok_or_else(|| {
        let detail = super::validation::explain_wasm_validation(runtime.bytes()).err()
            .unwrap_or_else(|| "Wasm validates, but canonical package metadata/layout admission rejected it".to_owned());
        EmitError::unsupported(format!(
            "compiler invariant: emitted build runtime {source_compilation_profile:?} failed its canonical raw admission: {detail}"
        ))
    })?;
    if admitted.key() != runtime.key() || admitted.layout() != runtime.layout() {
        return Err(EmitError::unsupported(
            "compiler invariant: build runtime round-trip changed its key or typed layout",
        ));
    }
    Ok(RuntimeBuildArtifact { runtime, package })
}

#[derive(Clone, Copy)]
enum Candidate<'a> {
    External {
        bytes: &'a [u8],
        expected_source: &'a [u8; DIGEST_BYTES],
    },
    ImmutableBuild {
        bytes: &'static [u8],
        expected_source: &'static [u8; DIGEST_BYTES],
    },
}

/// Optional inputs to the one lazy runtime resolver. A package can be supplied
/// by any safe caller: full identity/Wasm/layout admission precedes every use.
#[derive(Clone, Copy)]
pub struct RuntimeArtifactInputs<'a> {
    cache: Option<&'a dyn RuntimeArtifactCache>,
    package: Option<Candidate<'a>>,
    source_compilation_profile: RuntimeSourceCompilationProfile,
    admission_jobs: std::num::NonZeroUsize,
}

impl<'a> RuntimeArtifactInputs<'a> {
    pub const fn new(cache: Option<&'a dyn RuntimeArtifactCache>) -> Self {
        Self {
            cache,
            package: None,
            source_compilation_profile: RuntimeSourceCompilationProfile::Portable,
            admission_jobs: std::num::NonZeroUsize::MIN,
        }
    }

    pub const fn with_source_compilation_profile(
        mut self,
        profile: RuntimeSourceCompilationProfile,
    ) -> Self {
        self.source_compilation_profile = profile;
        self
    }

    pub const fn source_compilation_profile(self) -> RuntimeSourceCompilationProfile {
        self.source_compilation_profile
    }

    /// Bound concurrent admission work by this explicit limit and the visible
    /// CPU capacity. Every identity, Wasm body and layout check still runs.
    pub const fn with_admission_jobs(mut self, jobs: std::num::NonZeroUsize) -> Self {
        self.admission_jobs = jobs;
        self
    }

    pub(super) const fn admission_jobs(self) -> std::num::NonZeroUsize {
        self.admission_jobs
    }

    pub const fn with_build_package(
        mut self,
        bytes: &'a [u8],
        expected_source: &'a [u8; DIGEST_BYTES],
    ) -> Self {
        self.package = Some(Candidate::External {
            bytes,
            expected_source,
        });
        self
    }

    /// Reuse complete producer body validation for immutable build output.
    /// Identity, checksum, section, data and typed layout admission still run.
    /// Construction is lazy and does not admit or hash the package. After
    /// admission the runtime borrows this static payload rather than copying R.
    ///
    /// # Safety
    ///
    /// `bytes` must be the exact package returned by this compiler's
    /// `build_runtime_artifact_with_source_compilation_profile` (or its portable
    /// wrapper), whose successful round-trip fully validated every Wasm body.
    /// The package and `expected_source` must remain immutable for their entire
    /// static lifetimes. A static lifetime alone does not prove producer
    /// validation. Caller input, disk caches and re-encoded packages do not
    /// satisfy this contract. Use the safe `with_build_package` for those.
    pub const unsafe fn with_immutable_build_package(
        mut self,
        bytes: &'static [u8],
        expected_source: &'static [u8; DIGEST_BYTES],
    ) -> Self {
        self.package = Some(Candidate::ImmutableBuild {
            bytes,
            expected_source,
        });
        self
    }

    pub(super) fn load_or_emit(
        self,
        identity: &cache::RuntimeIdentity,
        emit: impl FnOnce() -> Result<Arc<RuntimeArtifact>, EmitError>,
    ) -> Result<Arc<RuntimeArtifact>, EmitError> {
        if let Some(candidate) = self.package {
            let started = std::time::Instant::now();
            let admitted = match candidate {
                Candidate::External {
                    bytes,
                    expected_source,
                } => envelope::decode_with_jobs(
                    bytes,
                    identity,
                    expected_source,
                    self.admission_jobs,
                ),
                Candidate::ImmutableBuild {
                    bytes,
                    expected_source,
                } => decode_immutable_build(bytes, identity, expected_source, self.admission_jobs),
            };
            if let Some(runtime) = admitted {
                if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                    eprintln!(
                        "scriptw wasm trace: runtime-build-package hit: {} bytes in {:?}",
                        runtime.bytes().len(),
                        started.elapsed()
                    );
                }
                return Ok(Arc::new(runtime));
            }
            if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                eprintln!("scriptw wasm trace: runtime-build-package miss: identity or layout rejected in {:?}", started.elapsed());
            }
        }
        if self.admission_jobs == std::num::NonZeroUsize::MIN {
            cache::load_or_emit(identity, self.cache, emit)
        } else {
            cache::load_or_emit_with_jobs(identity, self.cache, self.admission_jobs, emit)
        }
    }
}

#[cfg(test)]
mod tests;
