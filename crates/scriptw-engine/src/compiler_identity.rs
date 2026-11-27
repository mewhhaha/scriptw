//! Validated build-source and running-executable identity for execution evidence.

use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::sync::OnceLock;

/// A SHA-256 digest decoded once before evidence accepts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompilerDigest([u8; 32]);

impl CompilerDigest {
    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        if value.len() != 64 || !value.bytes().all(is_lower_hex) {
            return Err(
                "compiler SHA-256 must contain exactly 64 lowercase hexadecimal digits".into(),
            );
        }
        let mut bytes = [0; 32];
        for (destination, pair) in bytes.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
            *destination = (hex_value(pair[0]) << 4) | hex_value(pair[1]);
        }
        Ok(Self(bytes))
    }
}

impl fmt::Display for CompilerDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => unreachable!("the compiler digest constructor validates every digit"),
    }
}

/// A Git object identity, including repositories using SHA-256 object names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerCommitId(String);

impl CompilerCommitId {
    pub fn parse(value: &str) -> Result<Self, String> {
        if !matches!(value.len(), 40 | 64) || !value.bytes().all(is_lower_hex) {
            return Err(
                "compiler commit must contain 40 or 64 lowercase hexadecimal digits".into(),
            );
        }
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Archives retain the mandatory source fingerprint without claiming a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompilerSourceRevision {
    GitCommit(CompilerCommitId),
    UnversionedArchive,
}

/// Build metadata has its own validated authority. Source-only admission can
/// consume this fact without pretending it proves the loaded executable bytes:
/// no accessor of this type yields an executable digest, so it can neither key
/// a compiler-bound cache alone nor stand in for [`CompilerIdentity`] evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerBuildIdentity {
    source_fingerprint: CompilerDigest,
    source_revision: CompilerSourceRevision,
}

impl CompilerBuildIdentity {
    /// The build inputs Cargo embedded in this compiler. Reads no file.
    pub fn current() -> Result<&'static Self, String> {
        static BUILD: OnceLock<Result<CompilerBuildIdentity, String>> = OnceLock::new();
        BUILD
            .get_or_init(|| {
                let source_fingerprint =
                    CompilerDigest::parse(env!("SCRIPTW_COMPILER_FINGERPRINT"))?;
                let revision = env!("SCRIPTW_COMPILER_SOURCE_REVISION");
                let source_revision = match revision.strip_prefix("git:") {
                    Some(commit) => {
                        CompilerSourceRevision::GitCommit(CompilerCommitId::parse(commit)?)
                    }
                    None if revision == "unversioned-archive" => {
                        CompilerSourceRevision::UnversionedArchive
                    }
                    None => return Err("invalid embedded compiler source revision".into()),
                };
                Ok(CompilerBuildIdentity {
                    source_fingerprint,
                    source_revision,
                })
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    pub fn source_fingerprint(&self) -> CompilerDigest {
        self.source_fingerprint
    }
    pub fn source_revision(&self) -> &CompilerSourceRevision {
        &self.source_revision
    }
}

pub(crate) fn build_source_fingerprint() -> Result<CompilerDigest, String> {
    Ok(CompilerBuildIdentity::current()?.source_fingerprint)
}

/// Only checked source/executable parts can cross an execution-evidence boundary.
///
/// This is the evidence identity: `executable_sha256` is always SHA-256 of the
/// complete loaded image, read in full by the process that reports it (or
/// decoded from such a report). The cheaper compiler-cache identity
/// ([`crate::CompilerCacheIdentity`]) is a different type with no conversion
/// to this one, so a cache key can never be published as executable evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerIdentity {
    source_fingerprint: CompilerDigest,
    source_revision: CompilerSourceRevision,
    executable_sha256: CompilerDigest,
}

impl CompilerIdentity {
    pub const fn source_fingerprint_scheme() -> &'static str {
        env!("SCRIPTW_COMPILER_FINGERPRINT_SCHEME")
    }

    pub fn from_parts(
        source_fingerprint: CompilerDigest,
        source_revision: CompilerSourceRevision,
        executable_sha256: CompilerDigest,
    ) -> Self {
        Self {
            source_fingerprint,
            source_revision,
            executable_sha256,
        }
    }

    pub fn source_fingerprint(&self) -> CompilerDigest {
        self.source_fingerprint
    }
    pub fn source_revision(&self) -> &CompilerSourceRevision {
        &self.source_revision
    }
    pub fn executable_sha256(&self) -> CompilerDigest {
        self.executable_sha256
    }

    /// Bind once to embedded build inputs and the actual executing image.
    /// Unreadable or changing image bytes reject evidence; no empty-image hash
    /// or current-checkout substitution can stand in for this compiler.
    ///
    /// The first call reads and hashes the whole executable (about 0.2 s for a
    /// 200 MiB image). Only evidence producers call it: the `compiler-identity`
    /// command, Test262 and benchmark reports (except a benchmark host run
    /// with `SCRIPTW_BENCHMARK_REPORT_IDENTITY=build-source`). Compilation, compiler-bound
    /// caches and precompiled-image admission never do, unless the SHA-256
    /// cache cohort is selected explicitly.
    pub fn current() -> Result<&'static Self, String> {
        static IDENTITY: OnceLock<Result<CompilerIdentity, String>> = OnceLock::new();
        IDENTITY
            .get_or_init(Self::capture)
            .as_ref()
            .map_err(Clone::clone)
    }

    fn capture() -> Result<Self, String> {
        let build = CompilerBuildIdentity::current()?;
        let executable_sha256 = loaded_executable_digest()
            .map_err(|error| format!("cannot bind the running compiler executable: {error}"))?;
        Ok(Self::from_parts(
            build.source_fingerprint,
            build.source_revision.clone(),
            executable_sha256,
        ))
    }
}

fn loaded_executable_digest() -> io::Result<CompilerDigest> {
    with_stable_loaded_executable(|image| executable_digest(image))
}

/// Both cache-image algorithms and full SHA evidence hash the same opened
/// loaded-image authority, with identical before/after metadata admission.
pub(crate) fn with_stable_loaded_executable<T>(
    hash: impl FnOnce(&mut File) -> io::Result<T>,
) -> io::Result<T> {
    // Linux's loaded image survives renaming/replacement of its public path.
    // Opening current_exe's pathname could hash a different replacement image.
    #[cfg(target_os = "linux")]
    let mut image = File::open("/proc/self/exe")?;
    #[cfg(not(target_os = "linux"))]
    let mut image = File::open(std::env::current_exe()?)?;
    let before = image.metadata()?;
    if before.len() == 0 {
        return Err(io::Error::other("loaded compiler executable is empty"));
    }
    let digest = hash(&mut image)?;
    let after = image.metadata()?;
    if before.len() != after.len() || before.modified()? != after.modified()? {
        return Err(io::Error::other(
            "compiler executable changed while being hashed",
        ));
    }
    Ok(digest)
}

fn executable_digest(mut image: impl Read) -> io::Result<CompilerDigest> {
    let mut hash = Sha256::new();
    let mut buffer = [0; 8 * 1024];
    loop {
        let count = image.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(CompilerDigest(hash.finalize().into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiler_evidence_rejects_truncated_noncanonical_and_non_hex_identities() {
        for value in [
            "",
            "0",
            &"0".repeat(63),
            &"0".repeat(65),
            &"A".repeat(64),
            &"g".repeat(64),
        ] {
            assert!(CompilerDigest::parse(value).is_err());
        }
        for value in ["", &"0".repeat(39), &"0".repeat(41), &"A".repeat(40)] {
            assert!(CompilerCommitId::parse(value).is_err());
        }
        assert!(CompilerCommitId::parse(&"0".repeat(40)).is_ok());
        assert!(CompilerCommitId::parse(&"0".repeat(64)).is_ok());
    }

    #[test]
    fn executable_hash_is_standard_sha256_and_read_failures_cannot_become_evidence() {
        let digest = executable_digest(&b"abc"[..]).unwrap();
        assert_eq!(
            digest.to_string(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        struct Unreadable;
        impl Read for Unreadable {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "unreadable image",
                ))
            }
        }
        assert_eq!(
            executable_digest(Unreadable).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
}
