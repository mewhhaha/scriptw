//! Complete raw build-package envelope admission; no native-code authority.
use super::*;
mod intl;
pub(super) use intl::inert_build_frames;

pub(super) fn encode(
    runtime: &RuntimeArtifact,
    identity: &cache::RuntimeIdentity,
    compiler_source: &[u8; DIGEST_BYTES],
) -> Vec<u8> {
    let mut package = Vec::with_capacity(runtime.bytes().len() + 168);
    package.extend_from_slice(MAGIC);
    package.extend_from_slice(&LAYOUT_VERSION.to_le_bytes());
    package.extend_from_slice(compiler_source);
    package.extend_from_slice(&identity.digest());
    package.extend_from_slice(runtime.key().as_bytes());
    cache::encode_payload(runtime, &mut package);
    let digest = Sha256::digest(&package);
    package.extend_from_slice(&digest);
    package
}

pub(super) fn decode(
    package: &[u8],
    identity: &cache::RuntimeIdentity,
    expected_source: &[u8; DIGEST_BYTES],
) -> Option<RuntimeArtifact> {
    decode_with_jobs(
        package,
        identity,
        expected_source,
        std::num::NonZeroUsize::MIN,
    )
}

pub(super) fn decode_with_jobs(
    package: &[u8],
    identity: &cache::RuntimeIdentity,
    expected_source: &[u8; DIGEST_BYTES],
    jobs: std::num::NonZeroUsize,
) -> Option<RuntimeArtifact> {
    let (payload, expected_runtime) = admit_envelope(
        package,
        identity,
        expected_source,
        jobs,
        FunctionBodyValidation::AtUse,
    )?;
    let runtime = payload.into_owned();
    (runtime.key().as_bytes() == &expected_runtime).then_some(runtime)
}

pub(super) fn decode_immutable_build(
    package: &'static [u8],
    identity: &cache::RuntimeIdentity,
    expected_source: &[u8; DIGEST_BYTES],
    jobs: std::num::NonZeroUsize,
) -> Option<RuntimeArtifact> {
    let (payload, expected_runtime) = admit_envelope(
        package,
        identity,
        expected_source,
        jobs,
        FunctionBodyValidation::ValidatedAtBuild,
    )?;
    let runtime = payload.into_immutable_build();
    (runtime.key().as_bytes() == &expected_runtime).then_some(runtime)
}

fn admit_envelope<'a>(
    package: &'a [u8],
    identity: &cache::RuntimeIdentity,
    expected_source: &[u8; DIGEST_BYTES],
    jobs: std::num::NonZeroUsize,
    body_validation: FunctionBodyValidation,
) -> Option<(cache::RuntimePayload<'a>, [u8; DIGEST_BYTES])> {
    let digest_offset = package.len().checked_sub(DIGEST_BYTES)?;
    let (framed, digest) = package.split_at(digest_offset);
    let mut payload = framed;
    // Reject unsupported source/profiles before hashing the large raw image.
    if &cache::take::<8>(&mut payload)? != MAGIC {
        return None;
    }
    let version =
        CurrentRuntimeLayoutVersion::admit(u32::from_le_bytes(cache::take(&mut payload)?))?;
    if &cache::take::<DIGEST_BYTES>(&mut payload)? != expected_source
        || cache::take::<DIGEST_BYTES>(&mut payload)? != identity.digest()
    {
        return None;
    }
    let expected_runtime = cache::take::<DIGEST_BYTES>(&mut payload)?;
    let actual_digest: [u8; DIGEST_BYTES] = Sha256::digest(framed).into();
    if actual_digest.as_slice() != digest {
        return None;
    }
    let admitted = cache::admit_payload(payload, identity, version, jobs, body_validation)?;
    Some((admitted, expected_runtime))
}
