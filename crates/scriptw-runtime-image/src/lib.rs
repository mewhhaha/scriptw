//! Immutable Cargo-produced runtime images, isolated from compiler metadata.
//!
//! Every image comes from the current checked raw runtime producer. Engine
//! independently admits source, target, modes, layout and byte digests before
//! deserializing native code. This crate accepts no caller bytes or paths.

mod metadata;
pub use metadata::{metadata, portable_metadata};
mod native_storage;
pub use native_storage::NativeRuntimeBytes;
#[cfg(scriptw_runtime_image)]
mod native_lease;

#[cfg(scriptw_runtime_image)]
include!(concat!(env!("OUT_DIR"), "/image_bindings.rs"));

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
pub fn native() -> Option<std::sync::Arc<NativeRuntimeBytes>> {
    #[cfg(scriptw_runtime_image)]
    return native_lease::acquire(native_lease::Image::NativeHost);
    #[cfg(not(scriptw_runtime_image))]
    None
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
    #[cfg(scriptw_runtime_image)]
    // SAFETY: Cargo's checked producer and target compiler define this exact
    // immutable array in the linked data archive, with no mutable alias.
    return unsafe { &SCRIPTW_RUNTIME_PORTABLE_PACKAGE };
    #[cfg(not(scriptw_runtime_image))]
    &[]
}

#[inline(never)]
pub fn portable_native() -> Option<std::sync::Arc<NativeRuntimeBytes>> {
    #[cfg(scriptw_runtime_image)]
    return native_lease::acquire(native_lease::Image::Portable);
    #[cfg(not(scriptw_runtime_image))]
    None
}

#[inline(never)]
pub fn portable_manifest_bytes() -> &'static [u8] {
    #[cfg(scriptw_runtime_image)]
    // SAFETY: same target-defined immutable array contract as package().
    return unsafe { &SCRIPTW_RUNTIME_PORTABLE_MANIFEST };
    #[cfg(not(scriptw_runtime_image))]
    &[]
}

#[cfg(all(test, scriptw_runtime_image))]
mod lease_tests;
