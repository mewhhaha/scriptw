//! Profile-guided compilation: what a compilation does with an execution
//! profile, the identity that binds a profile to its program, and reading the
//! profile a training run recorded.
//!
//! A profile changes emitted P only and only by guarded prefixes that fall
//! through to the unprofiled code, so a wrong profile costs speed, never
//! behaviour. The contract is `docs/rust-rewrite/contracts/profile-guided-compilation.md`.
use super::*;
use scriptw_aot_wasm::{AdmittedProfile, ProfileMode, ProgramKey};
pub use scriptw_aot_wasm::{ExecutionProfile, ProfileError, ProfileMismatch};

/// What one compilation does with an execution profile.
///
/// `Collect` and `Apply` need the compiler's cache identity, which names the
/// program a profile belongs to; a compilation that asks for either fails
/// when the identity is unavailable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ExecutionProfileUse {
    /// The unprofiled build.
    #[default]
    None,
    /// An instrumented build that records a profile while it runs: read it
    /// with [`EmbeddedInstance::collect_profile`] after the training run.
    /// Its JavaScript behaviour is the unprofiled build's.
    Collect,
    /// A build specialised by a profile of this same program, admitted when
    /// the source is compiled: a profile of another program, compiler or
    /// build is rejected with [`ProfileMismatch`], never ignored.
    Apply(Arc<ExecutionProfile>),
}

impl ExecutionProfileUse {
    /// Frames this choice into a program cache key. The unprofiled build
    /// adds nothing, so its key, which is also the [`ProgramKey`] its
    /// profiles name, is the key it had before profiles existed. An applied
    /// profile is named by the SHA-256 of its canonical encoding.
    pub(crate) fn hash_cache_key_field(&self, hash: &mut Sha256) {
        let tag = match self {
            Self::None => return,
            Self::Collect => 1,
            Self::Apply(_) => 2,
        };
        hash_program_cache_field(hash, b"scriptw-execution-profile-v1");
        hash.update([tag]);
        if let Self::Apply(profile) = self {
            hash.update(Sha256::digest(profile.encode()));
        }
    }
}

/// A compilation's profile use, resolved against its program once the
/// source is lowered: the only form emission takes.
#[derive(Clone, Debug, Default)]
pub(crate) enum ResolvedExecutionProfile {
    #[default]
    None,
    Collect {
        program: ProgramKey,
        compiler: String,
    },
    /// `admitted` is `profile` admitted for the program `profile` names.
    Apply {
        profile: Arc<ExecutionProfile>,
        admitted: AdmittedProfile,
    },
}

/// Two resolutions are equal when they emit the same program: an admitted
/// profile is a function of its profile, whose program it was admitted for.
impl PartialEq for ResolvedExecutionProfile {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::None, Self::None) => true,
            (
                Self::Collect { program, compiler },
                Self::Collect {
                    program: other_program,
                    compiler: other_compiler,
                },
            ) => program == other_program && compiler == other_compiler,
            (Self::Apply { profile, .. }, Self::Apply { profile: other, .. }) => profile == other,
            (Self::None | Self::Collect { .. } | Self::Apply { .. }, _) => false,
        }
    }
}

impl Eq for ResolvedExecutionProfile {}

impl ResolvedExecutionProfile {
    /// Resolves `profile_use` for the program whose unprofiled cache key
    /// `program_key` computes; it is computed only for a profiled build.
    pub(crate) fn resolve(
        profile_use: &ExecutionProfileUse,
        program_key: impl FnOnce() -> Option<[u8; 32]>,
    ) -> Result<Self, EngineError> {
        if *profile_use == ExecutionProfileUse::None {
            return Ok(Self::None);
        }
        let program = ProgramKey::new(program_key().ok_or_else(|| {
            EngineError::new(format!(
                "an execution profile needs the compiler cache identity, which names its program: {}",
                compiler_cache_identity()
                    .err()
                    .unwrap_or_else(|| "unavailable".into())
            ))
        })?);
        match profile_use {
            ExecutionProfileUse::None => Ok(Self::None),
            ExecutionProfileUse::Collect => Ok(Self::Collect {
                program,
                compiler: compiler_fingerprint()
                    .map(|bytes| bytes.iter().map(|byte| format!("{byte:02x}")).collect())
                    .unwrap_or_default(),
            }),
            ExecutionProfileUse::Apply(profile) => AdmittedProfile::admit(profile, &program)
                .map(|admitted| Self::Apply {
                    profile: Arc::clone(profile),
                    admitted,
                })
                .map_err(EngineError::from_execution_profile),
        }
    }

    pub(crate) fn mode(&self) -> ProfileMode<'_> {
        match self {
            Self::None => ProfileMode::None,
            Self::Collect { program, compiler } => ProfileMode::Collect {
                program: *program,
                compiler,
            },
            Self::Apply { admitted, .. } => ProfileMode::Apply(admitted),
        }
    }
}

/// The `scriptw.profile.sites` section of a Collect-mode P, which says where
/// its profile memory keeps what.
#[derive(Clone, Debug)]
pub(crate) struct ProfileSites(Arc<[u8]>);

/// Finds a P's profile sites section among its custom sections, while the
/// caller walks P for its own checks.
#[derive(Default)]
pub(crate) struct ProfileSitesScan(Option<ProfileSites>);

impl ProfileSitesScan {
    pub(crate) fn observe(
        &mut self,
        section: &wasmparser::CustomSectionReader<'_>,
    ) -> Result<(), EngineError> {
        if section.name() != scriptw_aot_wasm::PROFILE_SITES_SECTION {
            return Ok(());
        }
        if self.0.is_some() {
            return Err(EngineError::new("two execution profile sections"));
        }
        self.0 = Some(ProfileSites(Arc::from(section.data())));
        Ok(())
    }

    /// The section, or `None` for a P not built to collect.
    pub(crate) fn finish(self) -> Option<ProfileSites> {
        self.0
    }
}

impl ProfileSites {
    /// Whether `program` was built to collect a profile.
    pub(crate) fn in_program(program: &[u8]) -> Result<bool, EngineError> {
        let mut scan = ProfileSitesScan::default();
        for payload in WasmParser::new(0).parse_all(program) {
            if let WasmPayload::CustomSection(section) =
                payload.map_err(|error| EngineError::new(error.to_string()))?
            {
                scan.observe(&section)?;
            }
        }
        Ok(scan.finish().is_some())
    }

    /// Calls P's profile dump and reads its profile memory, after a
    /// training run of `training_requests` requests.
    pub(crate) fn collect(
        &self,
        program: &wasmtime::Instance,
        store: &mut WasmtimeStore<WasmStoreState>,
        training_requests: std::num::NonZeroU32,
    ) -> Result<ExecutionProfile, EngineError> {
        let dump = program
            .get_typed_func::<(), ()>(&mut *store, scriptw_aot_wasm::PROFILE_DUMP_EXPORT)
            .map_err(|error| EngineError::new(format!("execution profile dump: {error:#}")))?;
        let memory = program
            .get_memory(&mut *store, scriptw_aot_wasm::PROFILE_MEMORY_EXPORT)
            .ok_or_else(|| EngineError::new("the execution profile memory is not exported"))?;
        run_on_execution_stack(|| dump.call(&mut *store, ()))
            .map_err(|error| EngineError::new(format!("execution profile dump: {error:#}")))?;
        ExecutionProfile::from_collection(&self.0, memory.data(&*store), training_requests)
            .map_err(EngineError::from_execution_profile)
    }
}

#[cfg(any(test, feature = "profile-experiments"))]
impl Engine {
    /// Verification only (feature `profile-experiments`): a wrong profile of
    /// `source` compiled with `options`, for checking that a wrong profile
    /// costs speed only. Every read site is hot in one training request,
    /// its dominant kind rotating through every specialisation from
    /// `seed`, so nearly every guard proves a kind its receivers never
    /// have. It emits the unprofiled build once to learn the site map.
    /// `Ok(None)` means the program itself is rejected with a source
    /// diagnostic (parse, early, link or lowering), which its unprofiled
    /// build reports too; every other failure is an error.
    pub fn adversarial_execution_profile(
        &self,
        source: &str,
        goal: EmbeddedModuleGoal,
        options: &CompileOptions,
        seed: usize,
    ) -> Result<Option<ExecutionProfile>, EngineError> {
        run_on_sized_stack(|| {
            let goal = match goal {
                EmbeddedModuleGoal::Script => ParseGoal::Script,
                EmbeddedModuleGoal::Module => ParseGoal::Module,
            };
            let options = CompileOptions {
                execution_profile: ExecutionProfileUse::None,
                ..options.clone()
            };
            let own = |error: EngineError| {
                if error.parse_diagnostic.is_some() || error.ir_diagnostic.is_some() {
                    Ok(None)
                } else {
                    Err(error)
                }
            };
            let prepared = match self.prepare_compilation(source, goal, &options) {
                Ok(prepared) => prepared,
                Err(error) => return own(error),
            };
            let program = ProgramKey::new(
                program_key_for_role(
                    source,
                    goal,
                    &options,
                    prepared.modules.graph(),
                    module_loader::EmbeddedSourceRole::Entry,
                    &ExecutionProfileUse::None,
                )
                .ok_or_else(|| {
                    EngineError::new(format!(
                        "an adversarial profile needs the compiler cache identity: {}",
                        compiler_cache_identity()
                            .err()
                            .unwrap_or_else(|| "unavailable".into())
                    ))
                })?,
            );
            let unit = match self.compile_prepared_on_current_thread(prepared) {
                Ok(unit) => unit,
                Err(error) => return own(error),
            };
            let cache = program_wasm_cache();
            let runtime_cache = cache
                .as_deref()
                .map(wasm_runtime_link::RuntimeWasmCache::new);
            let artifact = match scriptw_aot_wasm::emit_with_intl_profile_and_runtime_inputs(
                &unit.ir,
                unit.promise_rejection_policy,
                &unit.intl_profile,
                embedded_runtime::inputs(
                    runtime_cache
                        .as_ref()
                        .map(|cache| cache as &dyn scriptw_aot_wasm::RuntimeArtifactCache),
                    scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost,
                ),
                ProfileMode::None,
            ) {
                Ok(artifact) => artifact,
                Err(error) => return own(EngineError::from_wasm_emit_error(&unit.ir, error)),
            };
            let site_map = artifact.site_map();
            let mut profile = ExecutionProfile::new(
                program,
                site_map,
                "adversarial verification profile",
                std::num::NonZeroU32::MIN,
            );
            // Rotate through the guarded specialisations so every adversarial
            // seed emits a real profiled guard even on one-site ordinary reads
            // (which already prove OwnCompact layouts inline without a guard).
            let kinds: Vec<_> = scriptw_aot_wasm::ReadSpecialisation::ALL
                .into_iter()
                .filter(|kind| {
                    !matches!(kind, scriptw_aot_wasm::ReadSpecialisation::OwnCompact(_))
                })
                .collect();
            for (index, site) in site_map.read_sites().enumerate() {
                profile
                    .observe_read(
                        site,
                        scriptw_aot_wasm::ReadSiteObservation::dominated(
                            u64::MAX,
                            kinds[(index + seed) % kinds.len()],
                        ),
                    )
                    .map_err(EngineError::from_execution_profile)?;
            }
            Ok(Some(profile))
        })
    }
}
