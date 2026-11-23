//! Compiler-cache identity of the loaded executable; full SHA-256 evidence is
//! separate (`CompilerIdentity`).
//!
//! A compiler-bound cache entry may be reused only by the compiler build that
//! wrote it. The key therefore binds the validated build-source fingerprint
//! and an identity of the image this process is executing. Three closed
//! cohorts supply that image identity, each in its own fingerprint domain:
//!
//! - `build-id` (the default when the image has one): the linker's own digest
//!   of the linked output, read from the loaded image without reading the
//!   executable file. See [`loaded_build_id`] for what it does and does not
//!   bind.
//! - `blake3` (the default otherwise) and `sha256`: a serial hash of the whole
//!   opened executable under before/after metadata checks.
//!
//! None of them is execution evidence, and the type has no conversion to
//! `CompilerDigest`: reports and publication use `CompilerIdentity`.
use crate::compiler_identity::{build_source_fingerprint, with_stable_loaded_executable};
use crate::{CompilerDigest, CompilerIdentity};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::io::{self, Read};
use std::sync::OnceLock;

mod loaded_build_id;
use loaded_build_id::LoadedBuildId;

const HASH_ENV: &str = "SCRIPTW_COMPILER_CACHE_IMAGE_HASH";

/// How a compiler-cache key binds the loaded executable. Execution-evidence
/// identity always remains a full SHA-256, whichever binding keys the caches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilerCacheImageBinding {
    /// SHA-256 of the whole executable file: the evidence digest itself.
    Sha256,
    /// BLAKE3 of the whole executable file.
    Blake3,
    /// The linker build ID in the loaded image. No executable file read.
    LinkerBuildId,
}

impl CompilerCacheImageBinding {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Blake3 => "blake3",
            Self::LinkerBuildId => "build-id",
        }
    }

    pub const fn fingerprint_domain(self) -> &'static str {
        match self {
            // Preserve the exact ordinary cache identity and existing entries.
            Self::Sha256 => "scriptw-program-cache-compiler-v4",
            Self::Blake3 => "scriptw-program-cache-compiler-v5-blake3",
            Self::LinkerBuildId => "scriptw-program-cache-compiler-v6-build-id",
        }
    }
}

/// The closed selection. Only an absent variable may choose between bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Requested {
    /// The build ID when the loaded image carries one, else full-image BLAKE3.
    Default,
    Explicit(CompilerCacheImageBinding),
}

impl Requested {
    fn parse(value: Option<&OsStr>) -> Result<Self, String> {
        let Some(value) = value else {
            return Ok(Self::Default);
        };
        [
            CompilerCacheImageBinding::Sha256,
            CompilerCacheImageBinding::Blake3,
            CompilerCacheImageBinding::LinkerBuildId,
        ]
        .into_iter()
        .find(|binding| value == OsStr::new(binding.as_str()))
        .map(Self::Explicit)
        .ok_or_else(|| {
            format!(
                "{HASH_ENV} must be exactly sha256, blake3 or build-id; compiler-bound caches are disabled"
            )
        })
    }
}

/// An observed image identity together with the binding that produced it, so
/// a digest of one cohort cannot be fingerprinted in another's domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CacheImage {
    Sha256([u8; 32]),
    Blake3([u8; 32]),
    LinkerBuildId(LoadedBuildId),
}

impl CacheImage {
    fn binding(&self) -> CompilerCacheImageBinding {
        match self {
            Self::Sha256(_) => CompilerCacheImageBinding::Sha256,
            Self::Blake3(_) => CompilerCacheImageBinding::Blake3,
            Self::LinkerBuildId(_) => CompilerCacheImageBinding::LinkerBuildId,
        }
    }

    fn bytes(&self) -> &[u8] {
        match self {
            Self::Sha256(digest) | Self::Blake3(digest) => digest,
            Self::LinkerBuildId(id) => id.as_bytes(),
        }
    }
}

/// Observed only from the loaded image: a hash of its opened file or the
/// linker build ID in its mapped headers. Private fields prevent requested
/// metadata or build-source identity from standing in for the image.
///
/// This is cache identity, not execution evidence. It exposes no
/// `CompilerDigest`; `CompilerIdentity` alone reports `executable_sha256`.
#[derive(Debug)]
pub struct CompilerCacheIdentity {
    image: CacheImage,
    fingerprint: [u8; 32],
}

impl CompilerCacheIdentity {
    pub fn binding(&self) -> CompilerCacheImageBinding {
        self.image.binding()
    }

    /// The binding is supplied separately; this must never be labeled SHA-256
    /// when the actual selected image identity is BLAKE3 or a build ID. A
    /// build ID has its linker's length (20 bytes for SHA-1), not 32.
    pub fn image_identity_hex(&self) -> String {
        self.image
            .bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    pub(crate) fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }

    fn capture(requested: Requested) -> Result<Self, String> {
        let source = build_source_fingerprint()?;
        let binding = match requested {
            Requested::Explicit(binding) => binding,
            Requested::Default if LoadedBuildId::current().is_some() => {
                CompilerCacheImageBinding::LinkerBuildId
            }
            Requested::Default => CompilerCacheImageBinding::Blake3,
        };
        let image = match binding {
            CompilerCacheImageBinding::Sha256 => {
                CacheImage::Sha256(*CompilerIdentity::current()?.executable_sha256().as_bytes())
            }
            CompilerCacheImageBinding::Blake3 => CacheImage::Blake3(
                with_stable_loaded_executable(|image| blake3_image_digest(image))
                    .map_err(
                    |error| format!("cannot bind the running compiler cache image: {error}"),
                )?,
            ),
            // An explicit request never falls back to hashing: the cohort a
            // caller asked for is the cohort it gets, or no compiler cache.
            CompilerCacheImageBinding::LinkerBuildId => {
                CacheImage::LinkerBuildId(*LoadedBuildId::current().ok_or_else(|| {
                    format!(
                        "{HASH_ENV}=build-id needs an admissible linker build ID in the loaded executable (a 64-bit little-endian Linux image with PT_PHDR, readable procfs and one well-formed 16-64 byte GNU build-ID note); compiler-bound caches are disabled"
                    )
                })?)
            }
        };
        Ok(Self {
            fingerprint: cache_fingerprint(source, &image),
            image,
        })
    }
}

/// Returns the actual admitted cache-image cohort, or its explicit error. An
/// invalid selection/read failure disables caches; no other binding is chosen.
pub fn compiler_cache_identity() -> Result<&'static CompilerCacheIdentity, String> {
    static IDENTITY: OnceLock<Result<CompilerCacheIdentity, String>> = OnceLock::new();
    IDENTITY
        .get_or_init(|| {
            let started = std::time::Instant::now();
            let requested = std::env::var_os(HASH_ENV);
            let identity = CompilerCacheIdentity::capture(Requested::parse(requested.as_deref())?)?;
            if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                eprintln!(
                    "scriptw wasm trace: compiler-cache identity: {} in {:?}",
                    identity.binding().as_str(),
                    started.elapsed()
                );
            }
            Ok(identity)
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub(crate) fn cache_fingerprint(source: CompilerDigest, image: &CacheImage) -> [u8; 32] {
    // The outer cache key remains SHA-256. Its explicit domain identifies the
    // image binding; neither source-only nor cross-binding reuse fits.
    let mut hash = Sha256::new();
    hash.update(image.binding().fingerprint_domain().as_bytes());
    hash.update(source.as_bytes());
    match image {
        // Fixed-width digests keep the exact v4/v5 fingerprints.
        CacheImage::Sha256(digest) | CacheImage::Blake3(digest) => hash.update(digest),
        // Linkers choose the ID's length, so it is framed.
        CacheImage::LinkerBuildId(id) => {
            hash.update([id.as_bytes().len() as u8]);
            hash.update(id.as_bytes());
        }
    }
    hash.finalize().into()
}

/// BLAKE3 of every byte the reader yields; a read failure is never a digest.
/// The SHA-256 cohort reuses the evidence digest instead of hashing here.
fn blake3_image_digest(mut image: impl Read) -> io::Result<[u8; 32]> {
    let mut hash = blake3::Hasher::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = image.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(*hash.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BINDINGS: [CompilerCacheImageBinding; 3] = [
        CompilerCacheImageBinding::Sha256,
        CompilerCacheImageBinding::Blake3,
        CompilerCacheImageBinding::LinkerBuildId,
    ];

    #[test]
    fn cache_binding_selection_is_closed_and_invalid_inputs_do_not_fall_back() {
        assert_eq!(Requested::parse(None).unwrap(), Requested::Default);
        for binding in BINDINGS {
            assert_eq!(
                Requested::parse(Some(OsStr::new(binding.as_str()))).unwrap(),
                Requested::Explicit(binding)
            );
        }
        for invalid in [
            "", "SHA256", "blake2", " blake3", "blake3 ", "buildid", "default",
        ] {
            assert!(Requested::parse(Some(OsStr::new(invalid)))
                .unwrap_err()
                .contains("caches are disabled"));
        }
        let domains: std::collections::BTreeSet<_> = BINDINGS
            .iter()
            .map(|binding| binding.fingerprint_domain())
            .collect();
        assert_eq!(domains.len(), BINDINGS.len());
    }

    /// The build ID the linker wrote into the executable file, found through
    /// the file's own headers: independent of the loaded-memory reader.
    #[cfg(target_os = "linux")]
    fn build_id_in_executable_file() -> Option<Vec<u8>> {
        let file = std::fs::read("/proc/self/exe").unwrap();
        let u16_at = |at: usize| u16::from_le_bytes(file[at..at + 2].try_into().unwrap()) as usize;
        let u32_at = |at: usize| u32::from_le_bytes(file[at..at + 4].try_into().unwrap()) as usize;
        let u64_at = |at: usize| u64::from_le_bytes(file[at..at + 8].try_into().unwrap()) as usize;
        let (headers, count) = (u64_at(32), u16_at(56));
        for header in (0..count).map(|index| headers + index * 56) {
            if u32_at(header) != 4 {
                continue;
            }
            let mut note = u64_at(header + 8);
            let end = note + u64_at(header + 32);
            while note + 12 <= end {
                let (name, description) = (u32_at(note), u32_at(note + 4));
                let start = note + 12 + name.next_multiple_of(4);
                if u32_at(note + 8) == 3 && &file[note + 12..note + 12 + name] == b"GNU\0" {
                    return Some(file[start..start + description].to_vec());
                }
                note = start + description.next_multiple_of(4);
            }
        }
        None
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn default_cache_identity_is_the_loaded_images_linker_build_id_without_hashing_it() {
        let default = CompilerCacheIdentity::capture(Requested::Default).unwrap();
        let explicit = CompilerCacheIdentity::capture(Requested::Explicit(
            CompilerCacheImageBinding::LinkerBuildId,
        ));
        let Some(linked) = build_id_in_executable_file().filter(|id| id.len() >= 16) else {
            // A toolchain without build IDs keeps the full-image default, and
            // an explicit build-id request disables caches instead of hashing.
            assert_eq!(default.binding(), CompilerCacheImageBinding::Blake3);
            assert!(explicit.unwrap_err().contains("caches are disabled"));
            return;
        };
        // The mapped note is the file's note: this test executable's own link.
        assert_eq!(default.binding(), CompilerCacheImageBinding::LinkerBuildId);
        assert_eq!(default.image.bytes(), linked);
        assert_eq!(
            default.image_identity_hex(),
            linked
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        assert_eq!(
            *default.fingerprint(),
            cache_fingerprint(build_source_fingerprint().unwrap(), &default.image)
        );
        assert_eq!(explicit.unwrap().fingerprint(), default.fingerprint());
    }

    #[test]
    fn full_image_cohorts_hash_the_opened_loaded_image_and_keep_sha_evidence_separate() {
        let blake =
            CompilerCacheIdentity::capture(Requested::Explicit(CompilerCacheImageBinding::Blake3))
                .unwrap();
        let actual = with_stable_loaded_executable(|image| {
            let mut hash = blake3::Hasher::new();
            hash.update_reader(image)?;
            Ok(*hash.finalize().as_bytes())
        })
        .unwrap();
        assert_eq!(blake.binding(), CompilerCacheImageBinding::Blake3);
        assert_eq!(blake.image, CacheImage::Blake3(actual));
        assert_eq!(
            *blake.fingerprint(),
            cache_fingerprint(
                build_source_fingerprint().unwrap(),
                &CacheImage::Blake3(actual)
            )
        );

        let sha =
            CompilerCacheIdentity::capture(Requested::Explicit(CompilerCacheImageBinding::Sha256))
                .unwrap();
        let evidence = CompilerIdentity::current().unwrap();
        assert_eq!(
            sha.image,
            CacheImage::Sha256(*evidence.executable_sha256().as_bytes())
        );
        assert_eq!(
            *sha.fingerprint(),
            crate::compiler_identity_cache_fingerprint(evidence)
        );
        let default = CompilerCacheIdentity::capture(Requested::Default).unwrap();
        let fingerprints: std::collections::BTreeSet<_> = [&blake, &sha, &default]
            .map(|identity| *identity.fingerprint())
            .into();
        assert_eq!(
            fingerprints.len(),
            if default.binding() == CompilerCacheImageBinding::Blake3 {
                2
            } else {
                3
            }
        );
    }

    #[test]
    fn actual_image_identity_source_and_binding_all_determine_compiler_cache_identity() {
        let source = CompilerDigest::parse(&"11".repeat(32)).unwrap();
        let other_source = CompilerDigest::parse(&"22".repeat(32)).unwrap();
        let sha1: Vec<u8> = (0..20).collect();
        let mut relinked = sha1.clone();
        relinked[19] ^= 1;
        let longer = [&sha1[..], &[0]].concat();
        let mut images = Vec::new();
        for executable in [
            b"loaded image one".as_slice(),
            b"loaded image two".as_slice(),
        ] {
            images.push(CacheImage::Sha256(Sha256::digest(executable).into()));
            images.push(CacheImage::Blake3(blake3_image_digest(executable).unwrap()));
        }
        // Two links differ in their build ID, in any byte and in length.
        for id in [&sha1[..], &relinked, &sha1[..16], &longer] {
            images.push(CacheImage::LinkerBuildId(LoadedBuildId::for_test(id)));
        }
        let mut fingerprints = std::collections::BTreeSet::new();
        for image in &images {
            assert!(fingerprints.insert(cache_fingerprint(source, image)));
            assert!(fingerprints.insert(cache_fingerprint(other_source, image)));
        }

        let mut large_image = vec![0x5a; 200_000];
        let before = blake3_image_digest(large_image.as_slice()).unwrap();
        large_image[199_999] ^= 1;
        let after = blake3_image_digest(large_image.as_slice()).unwrap();
        assert_ne!(
            before, after,
            "a change after several read buffers must bind the actual image"
        );
        assert_eq!(after, *blake3::hash(&large_image).as_bytes());

        // Even equal identity bytes cannot alias binding domains.
        let bytes = [0x5a; 32];
        let aliases: std::collections::BTreeSet<_> = [
            CacheImage::Sha256(bytes),
            CacheImage::Blake3(bytes),
            CacheImage::LinkerBuildId(LoadedBuildId::for_test(&after)),
        ]
        .map(|image| cache_fingerprint(source, &image))
        .into();
        assert_eq!(aliases.len(), 3);
        assert_ne!(
            cache_fingerprint(source, &CacheImage::Blake3(after)),
            cache_fingerprint(
                source,
                &CacheImage::LinkerBuildId(LoadedBuildId::for_test(&after))
            )
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
            cache_fingerprint(source, &CacheImage::Sha256(image)),
            expected
        );
    }

    #[test]
    fn failed_image_reads_reject_the_cache_hash_instead_of_hashing_an_empty_prefix() {
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
        assert_eq!(
            blake3_image_digest(Unreadable {
                delivered_prefix: false
            })
            .unwrap_err()
            .kind(),
            io::ErrorKind::PermissionDenied
        );
    }
}
