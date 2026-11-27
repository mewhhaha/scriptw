//! Read-only byte view of one immutable, bounded precompiled envelope.
use crate::EngineError;
use rustix::mm::{mmap, munmap, MapFlags, ProtFlags};
use std::fs::File;
use std::ptr::{self, NonNull};
use std::sync::Arc;

pub(super) struct FileBytes {
    file: Arc<File>,
    address: NonNull<u8>,
    len: usize,
}

// SAFETY: the mapping is immutable and only its owning FileBytes unmaps it.
unsafe impl Send for FileBytes {}
// SAFETY: shared access exposes only immutable bytes and a read-only file.
unsafe impl Sync for FileBytes {}

impl FileBytes {
    /// # Safety
    /// `len` must be a bounded, nonzero length within this file, and the
    /// opened inode must remain immutable for the lifetime of all its views.
    pub(super) unsafe fn map(file: Arc<File>, len: usize) -> Result<Self, EngineError> {
        // SAFETY: no fixed address is requested and the caller guarantees
        // the entire mapped range is within the immutable file.
        let raw = unsafe {
            mmap(
                ptr::null_mut(),
                len,
                ProtFlags::READ,
                MapFlags::PRIVATE,
                &*file,
                0,
            )
        }
        .map_err(|error| EngineError::new(format!("precompiled image mapping failed: {error}")))?;
        let Some(address) = NonNull::new(raw.cast()) else {
            // SAFETY: mmap succeeded and no references to the map exist.
            let _ = unsafe { munmap(raw, len) };
            return Err(EngineError::new("precompiled image mapped at address zero"));
        };
        Ok(Self { file, address, len })
    }

    pub(super) fn advise_sequential(&self) {
        // Advisory prefetch/reclaim hint for one-pass envelope verification.
        // SAFETY: the mapping is live for `self.len` bytes and owned by `self`.
        let _ = unsafe {
            rustix::mm::madvise(
                self.address.as_ptr().cast(),
                self.len,
                rustix::mm::Advice::Sequential,
            )
        };
    }

    pub(super) fn as_slice(&self) -> &[u8] {
        // SAFETY: the caller of map guarantees the contents remain immutable.
        // The returned borrow cannot outlive this owner or its mapping.
        unsafe { std::slice::from_raw_parts(self.address.as_ptr(), self.len) }
    }

    pub(super) fn file(&self) -> &Arc<File> {
        &self.file
    }
}

impl Drop for FileBytes {
    fn drop(&mut self) {
        // SAFETY: this is the original mapping, with all borrows ended.
        let _ = unsafe { munmap(self.address.as_ptr().cast(), self.len) };
    }
}
