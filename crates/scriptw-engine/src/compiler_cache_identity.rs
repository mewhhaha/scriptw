//! Loaded-executable cache identity; full SHA-256 evidence is separate.
use crate::compiler_identity::{build_source_fingerprint, with_stable_loaded_executable};
use crate::{CompilerDigest, CompilerIdentity};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::io::{self, Read};
use std::sync::OnceLock;

const HASH_ENV: &str = "SCRIPTW_COMPILER_CACHE_IMAGE_HASH";

/// Both algorithms hash the actual loaded executable bytes serially. BLAKE3 is
/// the default; execution-evidence identity always remains SHA-256.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilerCacheImageHashAlgorithm {
    Sha256,
    Blake3,
}

impl CompilerCacheImageHashAlgorithm {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Blake3 => "blake3",
        }
    }

    pub const fn fingerprint_domain(self) -> &'static str {
        match self {
            // Preserve the exact ordinary cache identity and existing entries.
            Self::Sha256 => "scriptw-program-cache-compiler-v4",
            Self::Blake3 => "scriptw-program-cache-compiler-v5-blake3",
        }
    }

    fn requested(value: Option<&OsStr>) -> Result<Self, String> {
        match value {
            None => Ok(Self::Blake3),
            Some(value) if value == OsStr::new("sha256") => Ok(Self::Sha256),
            Some(value) if value == OsStr::new("blake3") => Ok(Self::Blake3),
            Some(_) => Err(format!(
                "{HASH_ENV} must be exactly sha256 or blake3; compiler-bound caches are disabled"
            )),
        }
    }
}

/// Observed only after hashing the opened loaded image. Private fields prevent
/// requested metadata or build-source identity from standing in for image bytes.
#[derive(Debug)]
pub struct CompilerCacheIdentity {
    algorithm: CompilerCacheImageHashAlgorithm,
    image_digest: [u8; 32],
    fingerprint: [u8; 32],
}

impl CompilerCacheIdentity {
    pub fn algorithm(&self) -> CompilerCacheImageHashAlgorithm {
        self.algorithm
    }

    /// The algorithm is supplied separately; this must never be labeled SHA-256
    /// when the actual selected image algorithm is BLAKE3.
    pub fn image_digest_hex(&self) -> String {
        self.image_digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    pub(crate) fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }

    fn capture(algorithm: CompilerCacheImageHashAlgorithm) -> Result<Self, String> {
        let source = build_source_fingerprint()?;
        let (image_digest, fingerprint) = match algorithm {
            CompilerCacheImageHashAlgorithm::Sha256 => {
                let identity = CompilerIdentity::current()?;
                (
                    *identity.executable_sha256().as_bytes(),
                    crate::compiler_identity_cache_fingerprint(identity),
                )
            }
            CompilerCacheImageHashAlgorithm::Blake3 => {
                let image = with_stable_loaded_executable(|image| image_digest(algorithm, image))
                    .map_err(|error| {
                    format!("cannot bind the running compiler cache image: {error}")
                })?;
                (image, cache_fingerprint(algorithm, source, &image))
            }
        };
        Ok(Self {
            algorithm,
            image_digest,
            fingerprint,
        })
    }
}

/// Returns the actual admitted cache-image cohort, or its explicit error. An
/// invalid selection/read failure disables caches; no other algorithm is chosen.
pub fn compiler_cache_identity() -> Result<&'static CompilerCacheIdentity, String> {
    static IDENTITY: OnceLock<Result<CompilerCacheIdentity, String>> = OnceLock::new();
    IDENTITY
        .get_or_init(|| {
            let requested = std::env::var_os(HASH_ENV);
            CompilerCacheIdentity::capture(CompilerCacheImageHashAlgorithm::requested(
                requested.as_deref(),
            )?)
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub(crate) fn cache_fingerprint(
    algorithm: CompilerCacheImageHashAlgorithm,
    source: CompilerDigest,
    image_digest: &[u8; 32],
) -> [u8; 32] {
    // The outer cache key remains SHA-256. Its explicit domain identifies the
    // loaded-image algorithm; neither source-only nor cross-algorithm reuse fits.
    let mut hash = Sha256::new();
    hash.update(algorithm.fingerprint_domain().as_bytes());
    hash.update(source.as_bytes());
    hash.update(image_digest);
    hash.finalize().into()
}

fn image_digest(
    algorithm: CompilerCacheImageHashAlgorithm,
    mut image: impl Read,
) -> io::Result<[u8; 32]> {
    enum Hasher {
        Sha256(Sha256),
        Blake3(blake3::Hasher),
    }
    let mut hash = match algorithm {
        CompilerCacheImageHashAlgorithm::Sha256 => Hasher::Sha256(Sha256::new()),
        CompilerCacheImageHashAlgorithm::Blake3 => Hasher::Blake3(blake3::Hasher::new()),
    };
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = image.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        match &mut hash {
            Hasher::Sha256(hash) => {
                hash.update(&buffer[..count]);
            }
            Hasher::Blake3(hash) => {
                hash.update(&buffer[..count]);
            }
        }
    }
    Ok(match hash {
        Hasher::Sha256(hash) => hash.finalize().into(),
        Hasher::Blake3(hash) => *hash.finalize().as_bytes(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_algorithm_selection_is_closed_and_invalid_inputs_do_not_fall_back() {
        assert_eq!(
            CompilerCacheImageHashAlgorithm::requested(None).unwrap(),
            CompilerCacheImageHashAlgorithm::Blake3
        );
        for algorithm in [
            CompilerCacheImageHashAlgorithm::Sha256,
            CompilerCacheImageHashAlgorithm::Blake3,
        ] {
            assert_eq!(
                CompilerCacheImageHashAlgorithm::requested(Some(OsStr::new(algorithm.as_str())))
                    .unwrap(),
                algorithm
            );
        }
        for invalid in ["", "SHA256", "blake2", " blake3", "blake3 "] {
            assert!(
                CompilerCacheImageHashAlgorithm::requested(Some(OsStr::new(invalid)))
                    .unwrap_err()
                    .contains("caches are disabled")
            );
        }
    }

    #[test]
    fn default_cache_identity_hashes_the_opened_loaded_image_and_keeps_sha_evidence_separate() {
        let default = CompilerCacheIdentity::capture(
            CompilerCacheImageHashAlgorithm::requested(None).unwrap(),
        )
        .unwrap();
        let actual = with_stable_loaded_executable(|image| {
            let mut hash = blake3::Hasher::new();
            hash.update_reader(image)?;
            Ok(*hash.finalize().as_bytes())
        })
        .unwrap();
        assert_eq!(default.algorithm(), CompilerCacheImageHashAlgorithm::Blake3);
        assert_eq!(default.image_digest, actual);
        assert_eq!(
            *default.fingerprint(),
            cache_fingerprint(
                CompilerCacheImageHashAlgorithm::Blake3,
                build_source_fingerprint().unwrap(),
                &actual,
            )
        );

        let sha = CompilerCacheIdentity::capture(CompilerCacheImageHashAlgorithm::Sha256).unwrap();
        let evidence = CompilerIdentity::current().unwrap();
        assert_eq!(sha.image_digest, *evidence.executable_sha256().as_bytes());
        assert_eq!(
            *sha.fingerprint(),
            crate::compiler_identity_cache_fingerprint(evidence)
        );
        assert_ne!(default.fingerprint(), sha.fingerprint());
    }

    #[test]
    fn actual_image_bytes_and_algorithm_both_determine_compiler_cache_identity() {
        let source = CompilerDigest::parse(&"11".repeat(32)).unwrap();
        let mut fingerprints = std::collections::BTreeSet::new();
        for algorithm in [
            CompilerCacheImageHashAlgorithm::Sha256,
            CompilerCacheImageHashAlgorithm::Blake3,
        ] {
            for executable in [
                b"loaded image one".as_slice(),
                b"loaded image two".as_slice(),
            ] {
                let actual = image_digest(algorithm, executable).unwrap();
                assert!(fingerprints.insert(cache_fingerprint(algorithm, source, &actual)));
                assert_ne!(
                    cache_fingerprint(algorithm, source, &actual),
                    cache_fingerprint(
                        algorithm,
                        CompilerDigest::parse(&"22".repeat(32)).unwrap(),
                        &actual
                    )
                );
            }
            let mut large_image = vec![0x5a; 200_000];
            let before = image_digest(algorithm, large_image.as_slice()).unwrap();
            large_image[199_999] ^= 1;
            let after = image_digest(algorithm, large_image.as_slice()).unwrap();
            assert_ne!(
                before, after,
                "a change after several read buffers must bind the actual image"
            );
            let expected = match algorithm {
                CompilerCacheImageHashAlgorithm::Sha256 => Sha256::digest(&large_image).into(),
                CompilerCacheImageHashAlgorithm::Blake3 => *blake3::hash(&large_image).as_bytes(),
            };
            assert_eq!(after, expected);
        }
        let sha = image_digest(CompilerCacheImageHashAlgorithm::Sha256, &b"abc"[..]).unwrap();
        assert_eq!(sha, <[u8; 32]>::from(Sha256::digest(b"abc")));
        let blake = image_digest(CompilerCacheImageHashAlgorithm::Blake3, &b"abc"[..]).unwrap();
        assert_eq!(blake, *blake3::hash(b"abc").as_bytes());
        assert_ne!(sha, blake);
        // Even equal supplied digest words cannot alias algorithm domains.
        assert_ne!(
            cache_fingerprint(CompilerCacheImageHashAlgorithm::Sha256, source, &sha),
            cache_fingerprint(CompilerCacheImageHashAlgorithm::Blake3, source, &sha)
        );
    }

    #[test]
    fn ordinary_sha_cache_domain_and_fingerprint_are_byte_identical_to_v4() {
        let source = CompilerDigest::parse(&"11".repeat(32)).unwrap();
        let image = [0x22; 32];
        let mut old = Sha256::new();
        old.update(b"scriptw-program-cache-compiler-v4");
        old.update(source.as_bytes());
        old.update(image);
        let expected: [u8; 32] = old.finalize().into();
        assert_eq!(
            cache_fingerprint(CompilerCacheImageHashAlgorithm::Sha256, source, &image),
            expected
        );
    }

    #[test]
    fn failed_image_reads_reject_both_algorithms_instead_of_hashing_an_empty_prefix() {
        struct Unreadable {
            delivered_prefix: bool,
        }
        impl Read for Unreadable {
            fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
                if !self.delivered_prefix {
                    output[..3].copy_from_slice(b"abc");
                    self.delivered_prefix = true;
                    return Ok(3);
                }
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "unreadable loaded image",
                ))
            }
        }
        for algorithm in [
            CompilerCacheImageHashAlgorithm::Sha256,
            CompilerCacheImageHashAlgorithm::Blake3,
        ] {
            assert_eq!(
                image_digest(
                    algorithm,
                    Unreadable {
                        delivered_prefix: false
                    }
                )
                .unwrap_err()
                .kind(),
                io::ErrorKind::PermissionDenied
            );
        }
    }
}
