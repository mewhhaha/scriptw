//! Inert producer metadata selection and exact runtime identity admission.
use super::*;

impl RuntimeArtifactInputs<'_> {
    /// Resolve compilation metadata before constructing any operation providers.
    pub fn intl_selection(
        self,
        profile: &scriptw_intl::IntlCompilationProfile,
    ) -> Result<scriptw_intl::IntlDataSelection, EmitError> {
        if *profile == scriptw_intl::IntlCompilationProfile::Minimal {
            if let Some(Candidate::ImmutableMinimal(proof)) = self.package {
                let (_, expected_source, metadata, _) = proof.parts();
                return Ok(
                    scriptw_intl::IntlDataSelection::defer_minimal_compilation_data(move || {
                        let frames =
                            metadata::BuildMetadata::decode(proof, self.source_compilation_profile)
                                .map(|receipt| receipt.frames())
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

    pub(in crate::runtime_artifact) fn admit_identity(
        self,
        selection: &scriptw_intl::IntlDataSelection,
    ) -> Result<cache::RuntimeIdentity, EmitError> {
        if let Some(Candidate::ImmutableMinimal(proof)) = self.package {
            let receipt = metadata::BuildMetadata::decode(proof, self.source_compilation_profile)
                .ok_or_else(|| {
                EmitError::unsupported("immutable runtime receipt failed admission")
            })?;
            if let Some(identity) = receipt.identity(selection)? {
                return Ok(identity);
            }
        }
        cache::RuntimeIdentity::admit_with_jobs(
            selection,
            self.source_compilation_profile,
            self.admission_jobs,
        )
    }
}
