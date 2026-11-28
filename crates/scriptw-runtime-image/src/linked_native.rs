//! Raw Cargo-linked native images and their place in the running executable.
//!
//! Admission by origin, not by re-hash. The checked producer's native image is
//! linked unchanged and page-aligned, so its bytes are pages of the running
//! executable: the same file, and the same kernel protection against writes,
//! as the code that would check a digest and as the manifest holding one.
//! Hashing them again at every start cannot detect anything a writer of that
//! file could not also defeat. What load-time admission can establish is
//! therefore origin and pairing, and both remain mandatory:
//!
//! - origin: only this module constructs a [`LinkedNativeImage`], and only
//!   from the two linked statics. An [`ExecutableImageRange`] is constructed
//!   only after `native_input` proves that the static's exact address range
//!   is a read-only private mapping of the kernel's `/proc/self/exe` file;
//! - pairing: `build_digest` is the SHA-256 the Cargo packaging step computed
//!   over the exact file it linked. The engine compares it with the native
//!   digest in the producer's independent manifest before any deserialization.
//!
//! No path, caller slice, cache entry or digest-selected file can become
//! either type. Evidence of the bytes on disk remains `CompilerIdentity`,
//! which hashes the whole executable, image included.
use std::fs::File;

/// One of the two native images linked into this build, with the digest the
/// build attested for it. Neither field is caller-supplied.
#[derive(Clone, Copy)]
pub struct LinkedNativeImage {
    bytes: &'static [u8],
    build_digest: &'static [u8; 32],
}

impl LinkedNativeImage {
    /// The linked image through its loaded address. Reading it faults in the
    /// executable's pages; prefer [`Self::executable_range`] for loading.
    pub fn bytes(&self) -> &'static [u8] {
        self.bytes
    }

    /// SHA-256 of the linked bytes as computed when they were linked. It is
    /// not recomputed at load.
    pub fn build_digest(&self) -> &'static [u8; 32] {
        self.build_digest
    }

    /// The image's own range of the running executable file, where the
    /// process can prove it. `None` leaves [`Self::bytes`].
    pub fn executable_range(&self) -> Option<ExecutableImageRange> {
        #[cfg(all(target_os = "linux", any(scriptw_runtime_image, test)))]
        {
            let (file, offset) = super::native_input::executable_range(self.bytes)?;
            Some(ExecutableImageRange {
                file,
                offset,
                length: self.bytes.len(),
            })
        }
        #[cfg(not(all(target_os = "linux", any(scriptw_runtime_image, test))))]
        None
    }
}

/// The NativeHost image this build linked, raw and page-aligned.
#[inline(never)]
pub fn native() -> Option<LinkedNativeImage> {
    #[cfg(scriptw_runtime_image)]
    // SAFETY: Cargo's checked producer, its packaging step and the target
    // compiler define these exact immutable statics, with no mutable alias.
    return Some(unsafe {
        LinkedNativeImage {
            bytes: &super::SCRIPTW_RUNTIME_NATIVE.0,
            build_digest: &super::SCRIPTW_RUNTIME_NATIVE_DIGEST,
        }
    });
    #[cfg(not(scriptw_runtime_image))]
    None
}

/// Portable-profile counterpart of native(), with the same source boundary.
#[inline(never)]
pub fn portable_native() -> Option<LinkedNativeImage> {
    #[cfg(all(scriptw_runtime_image, not(feature = "native-host-only")))]
    // SAFETY: same target-defined immutable static contract as native().
    return Some(unsafe {
        LinkedNativeImage {
            bytes: &super::SCRIPTW_RUNTIME_PORTABLE_NATIVE.0,
            build_digest: &super::SCRIPTW_RUNTIME_PORTABLE_NATIVE_DIGEST,
        }
    });
    #[cfg(not(all(scriptw_runtime_image, not(feature = "native-host-only"))))]
    None
}

/// Proof that `length` bytes at `offset` of the owned kernel executable handle
/// are a linked native image. The handle stays the running executable after a
/// rename or unlink; it is read-only and never exposed for writing.
pub struct ExecutableImageRange {
    file: File,
    offset: u64,
    length: usize,
}

impl ExecutableImageRange {
    /// Another handle to the same open executable, for one mapping owner.
    pub fn try_clone_file(&self) -> std::io::Result<File> {
        self.file.try_clone()
    }

    /// File offset of the image. The link aligns it to 64 KiB wherever the
    /// link's page size divides that; a mapper still checks its own page size.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn length(&self) -> usize {
        self.length
    }
}
