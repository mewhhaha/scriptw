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

impl ProfileSites {
    /// The section of `program`, or `None` for a P not built to collect.
    pub(crate) fn of(program: &[u8]) -> Result<Option<Self>, EngineError> {
        let mut found = None;
        for payload in WasmParser::new(0).parse_all(program) {
            if let WasmPayload::CustomSection(section) =
                payload.map_err(|error| EngineError::new(error.to_string()))?
            {
                if section.name() == scriptw_aot_wasm::PROFILE_SITES_SECTION {
                    if found.is_some() {
                        return Err(EngineError::new("two execution profile sections"));
                    }
                    found = Some(Self(Arc::from(section.data())));
                }
            }
        }
        Ok(found)
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
            .map_err(|error| EngineError::new(error.to_string()))
    }
}

impl Engine {
    /// Verification only: a wrong profile of `source` compiled with
    /// `options`, every read site hot with a kind rotated from `seed`, for
    /// checking that a wrong profile costs speed only. It emits the
    /// unprofiled build once to learn its site map.
    #[doc(hidden)]
    pub fn adversarial_execution_profile(
        &self,
        source: &str,
        goal: EmbeddedModuleGoal,
        options: &CompileOptions,
        seed: usize,
    ) -> Result<ExecutionProfile, EngineError> {
        run_on_sized_stack(|| {
            let goal = match goal {
                EmbeddedModuleGoal::Script => ParseGoal::Script,
                EmbeddedModuleGoal::Module => ParseGoal::Module,
            };
            let options = CompileOptions {
                execution_profile: ExecutionProfileUse::None,
                ..options.clone()
            };
            let prepared = self.prepare_compilation(source, goal, &options)?;
            let program = ProgramKey::new(
                program_key_for_role(
                    source,
                    goal,
                    &options,
                    prepared.modules.graph(),
                    module_loader::EmbeddedSourceRole::Entry,
                    &ExecutionProfileUse::None,
                )
                .ok_or_else(|| EngineError::new("no compiler cache identity"))?,
            );
            let unit = self.compile_prepared_on_current_thread(prepared)?;
            let cache = program_wasm_cache();
            let runtime_cache = cache
                .as_deref()
                .map(wasm_runtime_link::RuntimeWasmCache::new);
            let artifact = scriptw_aot_wasm::emit_with_intl_profile_and_runtime_inputs(
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
            )
            .map_err(|error| EngineError::from_wasm_emit_error(&unit.ir, error))?;
            Ok(ExecutionProfile::adversarial(
                program,
                artifact.site_map(),
                "adversarial verification profile",
                seed,
            ))
        })
    }
}
