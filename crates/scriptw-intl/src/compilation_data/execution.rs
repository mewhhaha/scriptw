//! Identity-only admission shares the full decoder's exact prefix/plan.
use super::*;

pub(super) fn identity_prefix<'a>(
    metadata: &'a [u8],
    source: &[u8; 32],
) -> Result<(IntlDataIdentity, Reader<'a>), IntlBundleExportError> {
    if metadata.len() > MAX_METADATA || metadata.len() < 32 {
        return Err(IntlBundleExportError::TooLarge);
    }
    let (body, checksum) = metadata.split_at(metadata.len() - 32);
    if Sha256::digest(body).as_slice() != checksum {
        return Err(IntlBundleExportError::Identity);
    }
    let mut r = Reader(body);
    if r.take(8)? != MAGIC || r.take(32)? != source {
        return Err(IntlBundleExportError::Identity);
    }
    let digest = IntlDataDigest::from_sha256(r.take(32)?.try_into().unwrap());
    let locale =
        CanonicalLocaleId::from_data(r.text()?).map_err(|_| IntlBundleExportError::Identity)?;
    let plan = IntlProfilePlan::minimal(IntlServiceSet::ALL)
        .map_err(|_| IntlBundleExportError::Identity)?
        .with_operation::<crate::LookupNamedTimeZone>()
        .with_operation::<crate::ResolveTimeZone>()
        .with_operation::<crate::NamedTimeZoneOffset>()
        .with_operation::<crate::PossibleNamedTimeZoneEpochs>()
        .with_operation::<crate::FindNamedTimeZoneTransition>();
    let identity = IntlDataIdentity::new(plan, locale, IntlDataPlacement::Embedded, digest);
    if r.field()? != identity.artifact_identity().as_bytes() {
        return Err(IntlBundleExportError::Identity);
    }
    Ok((identity, r))
}

impl IntlCompilationData {
    /// Parse bounded identity facts only. This grants no frame, catalogue or
    /// native authority; complete data admission still needs its own owner.
    pub fn build_metadata_identity(
        metadata: &[u8],
        source: &[u8; 32],
        frames: &[(IntlDataComponent, usize, [u8; 32])],
    ) -> Result<IntlDataIdentity, IntlBundleExportError> {
        let (identity, mut r) = identity_prefix(metadata, source)?;
        if frames.len() != IntlDataComponent::ALL.len() {
            return Err(IntlBundleExportError::Identity);
        }
        for (expected, (component, length, digest)) in
            IntlDataComponent::ALL.into_iter().zip(frames)
        {
            if expected != *component
                || r.take(2)? != component.code().to_le_bytes()
                || r.u32()? != *length
                || r.take(32)? != digest
            {
                return Err(IntlBundleExportError::Identity);
            }
        }
        Ok(identity)
    }
}
