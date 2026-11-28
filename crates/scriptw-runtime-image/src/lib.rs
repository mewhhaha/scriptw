//! Immutable Cargo-produced runtime images, isolated from compiler metadata.
//!
//! Every image comes from the current checked raw runtime producer. Engine
//! independently admits source, target, modes, layout and digests before
//! deserializing native code. This crate accepts no caller bytes or paths.

pub mod execution;
mod metadata;
pub use metadata::{metadata, portable_metadata};
mod linked_native;
pub use linked_native::{native, portable_native, ExecutableImageRange, LinkedNativeImage};
#[cfg(all(target_os = "linux", any(scriptw_runtime_image, test)))]
mod native_input;

#[cfg(scriptw_runtime_image)]
include!(concat!(env!("OUT_DIR"), "/image_bindings.rs"));

/// Exact compact receipt of the checked Cargo producer, not caller authority.
#[inline(never)]
pub fn execution_metadata() -> &'static [u8] {
    #[cfg(scriptw_runtime_image)]
    return unsafe { &SCRIPTW_RUNTIME_EXECUTION };
    #[cfg(not(scriptw_runtime_image))]
    &[]
}
#[inline(never)]
pub fn portable_execution_metadata() -> &'static [u8] {
    #[cfg(all(scriptw_runtime_image, not(feature = "native-host-only")))]
    return unsafe { &SCRIPTW_RUNTIME_PORTABLE_EXECUTION };
    #[cfg(not(all(scriptw_runtime_image, not(feature = "native-host-only"))))]
    &[]
}

// The target Rust compiler defines each immutable data array with the same
// symbol and exact type as its generated declaration. The linked archive owns
// its storage for the process lifetime. No initializer enters crate metadata.
#[inline(never)]
pub fn intl_metadata() -> &'static [u8] {
    #[cfg(scriptw_runtime_image)]
    // SAFETY: same target-defined immutable array contract as package().
    return unsafe { &SCRIPTW_RUNTIME_INTL_METADATA };
    #[cfg(not(scriptw_runtime_image))]
    &[]
}

#[inline(never)]
pub fn package() -> &'static [u8] {
    #[cfg(scriptw_runtime_image)]
    // SAFETY: Cargo's checked producer and target compiler define this exact
    // immutable array in the linked data archive, with no mutable alias.
    return unsafe { &SCRIPTW_RUNTIME_PACKAGE };
    #[cfg(not(scriptw_runtime_image))]
    &[]
}

#[inline(never)]
pub fn manifest_bytes() -> &'static [u8] {
    #[cfg(scriptw_runtime_image)]
    // SAFETY: same target-defined immutable array contract as package().
    return unsafe { &SCRIPTW_RUNTIME_MANIFEST };
    #[cfg(not(scriptw_runtime_image))]
    &[]
}

#[inline(never)]
pub fn portable_package() -> &'static [u8] {
    #[cfg(all(scriptw_runtime_image, not(feature = "native-host-only")))]
    // SAFETY: Cargo's checked producer and target compiler define this exact
    // immutable array in the linked data archive, with no mutable alias.
    return unsafe { &SCRIPTW_RUNTIME_PORTABLE_PACKAGE };
    #[cfg(not(all(scriptw_runtime_image, not(feature = "native-host-only"))))]
    &[]
}

#[inline(never)]
pub fn portable_manifest_bytes() -> &'static [u8] {
    #[cfg(all(scriptw_runtime_image, not(feature = "native-host-only")))]
    // SAFETY: same target-defined immutable array contract as package().
    return unsafe { &SCRIPTW_RUNTIME_PORTABLE_MANIFEST };
    #[cfg(not(all(scriptw_runtime_image, not(feature = "native-host-only"))))]
    &[]
}

#[cfg(all(test, scriptw_runtime_image))]
mod linked_tests;
