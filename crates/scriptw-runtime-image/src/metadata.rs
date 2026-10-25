//! Compact receipts emitted by the fully checked immutable Cargo producer.
#[cfg(scriptw_runtime_image)]
use super::{SCRIPTW_RUNTIME_METADATA, SCRIPTW_RUNTIME_PORTABLE_METADATA};

#[inline(never)]
pub fn metadata() -> &'static [u8] {
    #[cfg(scriptw_runtime_image)]
    // SAFETY: the checked producer and target compiler define this array.
    return unsafe { &SCRIPTW_RUNTIME_METADATA };
    #[cfg(not(scriptw_runtime_image))]
    &[]
}

#[inline(never)]
pub fn portable_metadata() -> &'static [u8] {
    #[cfg(scriptw_runtime_image)]
    // SAFETY: the checked producer and target compiler define this array.
    return unsafe { &SCRIPTW_RUNTIME_PORTABLE_METADATA };
    #[cfg(not(scriptw_runtime_image))]
    &[]
}
