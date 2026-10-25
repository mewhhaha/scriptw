//! Inert frame views from the canonical raw envelope; no Wasm execution authority.
use super::*;

/// Only inert static frame views. This grants no Wasm layout, body or native
/// admission; the normal RuntimeIdentity and layout gates still follow.
pub(in crate::runtime_artifact::build_package) fn inert_build_frames(
    package: &'static [u8],
    source: &[u8; 32],
    profile: RuntimeSourceCompilationProfile,
) -> Option<Vec<(scriptw_intl::IntlDataComponent, &'static [u8])>> {
    use wasmparser::{Parser, Payload};
    let (body, checksum) = package.split_at_checked(package.len().checked_sub(32)?)?;
    let mut cursor = body;
    if &cache::take::<8>(&mut cursor)? != MAGIC
        || CurrentRuntimeLayoutVersion::admit(u32::from_le_bytes(cache::take(&mut cursor)?))
            .is_none()
        || cache::take::<32>(&mut cursor)? != *source
    {
        return None;
    }
    cache::take::<32>(&mut cursor)?; // Actual Intl identity is reconstructed from the frames.
    cache::take::<32>(&mut cursor)?; // The runtime key is checked by ordinary layout admission.
    cache::take::<8>(&mut cursor)?;
    cache::take::<8>(&mut cursor)?;
    cache::take::<4>(&mut cursor)?;
    let length = usize::try_from(u64::from_le_bytes(cache::take(&mut cursor)?)).ok()?;
    if cursor.len() != length || Sha256::digest(body).as_slice() != checksum {
        return None;
    }
    let mut frames = std::collections::BTreeMap::new();
    let mut actual_profile = None;
    for payload in Parser::new(0).parse_all(cursor) {
        if let Payload::CustomSection(section) = payload.ok()? {
            if section.name()
                == crate::runtime_artifact::RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION
            {
                if actual_profile.replace(section.data()).is_some() {
                    return None;
                }
            }
            if let Some(component) = scriptw_intl::IntlDataComponent::ALL
                .into_iter()
                .find(|c| c.section_name() == section.name())
            {
                if frames.insert(component, section.data()).is_some() {
                    return None;
                }
            }
        }
    }
    if actual_profile != Some([profile.wire()].as_slice())
        || frames.len() != scriptw_intl::IntlDataComponent::ALL.len()
    {
        return None;
    }
    Some(frames.into_iter().collect())
}
