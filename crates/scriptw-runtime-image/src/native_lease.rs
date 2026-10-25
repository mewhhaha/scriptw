//! Weak ownership shares concurrent loads without retaining decoded storage.
use super::NativeRuntimeBytes;

pub(super) enum Image {
    NativeHost,
    Portable,
}

pub(super) fn acquire(profile: Image) -> Option<std::sync::Arc<NativeRuntimeBytes>> {
    use std::sync::{Arc, Mutex, Weak};
    // Share only overlapping deserializations. A weak lease retains no
    // decoded bytes after the last load; each Module owns its native mapping.
    static NATIVE_HOST: Mutex<Result<Weak<NativeRuntimeBytes>, ()>> = Mutex::new(Ok(Weak::new()));
    static PORTABLE: Mutex<Result<Weak<NativeRuntimeBytes>, ()>> = Mutex::new(Ok(Weak::new()));
    // SAFETY: both arrays have Cargo's exact target-defined immutable ABI.
    let (state, encoded) = match profile {
        Image::NativeHost => (&NATIVE_HOST, unsafe { &super::SCRIPTW_RUNTIME_NATIVE[..] }),
        Image::Portable => (&PORTABLE, unsafe {
            &super::SCRIPTW_RUNTIME_PORTABLE_NATIVE[..]
        }),
    };
    let mut state = state.lock().ok()?;
    let weak = state.as_mut().ok()?;
    if let Some(image) = weak.upgrade() {
        return Some(image);
    }
    // SAFETY: only Cargo's immutable target-defined encoded array enters
    // this private decoder. Exact framing, output/window bounds and digest
    // precede publication. No caller bytes or mutable aliases are accepted.
    let Some(decoded) = super::native_storage::decode(encoded) else {
        *state = Err(());
        return None;
    };
    let image = Arc::new(decoded);
    *weak = Arc::downgrade(&image);
    return Some(image);
}
