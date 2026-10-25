//! Lazy runtime inputs and the explicit immutable Cargo trust boundary.
use super::*;

#[derive(Clone, Copy)]
enum Candidate<'a> {
    External {
        bytes: &'a [u8],
        expected_source: &'a [u8; DIGEST_BYTES],
    },
    ImmutableBuild {
        bytes: &'static [u8],
        expected_source: &'static [u8; DIGEST_BYTES],
        intl_metadata: Option<&'static [u8]>,
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

    pub(in crate::runtime_artifact) const fn admission_jobs(self) -> std::num::NonZeroUsize {
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
            intl_metadata: None,
        });
        self
    }

    /// # Safety
    /// The package must satisfy `with_immutable_build_package` and metadata
    /// must be the exact `minimal_build_metadata` output of that same producer.
    pub const unsafe fn with_immutable_minimal_build(
        mut self,
        bytes: &'static [u8],
        expected_source: &'static [u8; DIGEST_BYTES],
        metadata: &'static [u8],
    ) -> Self {
        self.package = Some(Candidate::ImmutableBuild {
            bytes,
            expected_source,
            intl_metadata: Some(metadata),
        });
        self
    }

    /// Resolve compilation metadata before constructing any operation providers.
    pub fn intl_selection(
        self,
        profile: &scriptw_intl::IntlCompilationProfile,
    ) -> Result<scriptw_intl::IntlDataSelection, EmitError> {
        if *profile == scriptw_intl::IntlCompilationProfile::Minimal {
            if let Some(Candidate::ImmutableBuild {
                bytes,
                expected_source,
                intl_metadata: Some(metadata),
            }) = self.package
            {
                return Ok(
                    scriptw_intl::IntlDataSelection::defer_minimal_compilation_data(move || {
                        let frames = envelope::inert_build_frames(
                            bytes,
                            expected_source,
                            self.source_compilation_profile,
                        )
                        .ok_or_else(|| {
                            scriptw_intl::EmbeddedIntlProviderSetupError::DataImage(
                                scriptw_intl::IntlDataImageError::consumer(
                                    "immutable Intl build package failed envelope admission",
                                ),
                            )
                        })?;
                        // SAFETY: only the immutable Cargo candidate constructor can
                        // carry this authority. Frame/source/hash checks above and in
                        // the metadata reader bind the exact admitted producer outputs.
                        let build = unsafe {
                            scriptw_intl::ImmutableIntlBuildData::new(
                                metadata,
                                expected_source,
                                frames,
                            )
                        };
                        let selection =
                            scriptw_intl::IntlDataSelection::from_immutable_minimal_build(build)
                                .map_err(|error| {
                                    scriptw_intl::EmbeddedIntlProviderSetupError::DataImage(
                                        scriptw_intl::IntlDataImageError::consumer(error),
                                    )
                                })?;
                        selection
                            .compilation_data()
                            .map(Arc::clone)
                            .map_err(|error| {
                                scriptw_intl::EmbeddedIntlProviderSetupError::DataImage(
                                    scriptw_intl::IntlDataImageError::consumer(error),
                                )
                            })
                    }),
                );
            }
        }
        Ok(scriptw_intl::IntlDataSelection::new(profile.clone()))
    }

    pub(in crate::runtime_artifact) fn load_or_emit(
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
                    ..
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
