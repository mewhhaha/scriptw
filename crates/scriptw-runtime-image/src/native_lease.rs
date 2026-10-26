//! Weak ownership shares concurrent loads without retaining decoded storage.
use super::NativeRuntimeBytes;

pub(super) enum Image {
    NativeHost,
    Portable,
}

impl Image {
    fn reader(&self) -> super::native_input::PackedReader {
        // SAFETY: these are exclusively Cargo's target-defined immutable arrays.
        let encoded = match self {
            Self::NativeHost => unsafe { &super::SCRIPTW_RUNTIME_NATIVE[..] },
            Self::Portable => unsafe { &super::SCRIPTW_RUNTIME_PORTABLE_NATIVE[..] },
        };
        super::native_input::PackedReader::new(encoded)
    }
}

pub(super) fn write(
    profile: Image,
    writer: &mut impl std::io::Write,
) -> Option<super::NativeRuntimeDigest> {
    super::native_storage::write_decoded(profile.reader(), writer)
}

pub(super) fn acquire(profile: Image) -> Option<std::sync::Arc<NativeRuntimeBytes>> {
    use std::sync::{Arc, Mutex, Weak};
    // Share only overlapping deserializations. A weak lease retains no
    // decoded bytes after the last load; each Module owns its native mapping.
    static NATIVE_HOST: Mutex<Result<Weak<NativeRuntimeBytes>, ()>> = Mutex::new(Ok(Weak::new()));
    static PORTABLE: Mutex<Result<Weak<NativeRuntimeBytes>, ()>> = Mutex::new(Ok(Weak::new()));
    let state = match profile {
        Image::NativeHost => &NATIVE_HOST,
        Image::Portable => &PORTABLE,
    };
    let mut state = state.lock().ok()?;
    let weak = state.as_mut().ok()?;
    if let Some(image) = weak.upgrade() {
        return Some(image);
    }
    // SAFETY: only Cargo's immutable target-defined encoded array enters
    // this private decoder. Exact framing, output/window bounds and digest
    // precede publication. No caller bytes or mutable aliases are accepted.
    let Some(decoded) = super::native_storage::decode_reader(profile.reader()) else {
        *state = Err(());
        return None;
    };
    let image = Arc::new(decoded);
    *weak = Arc::downgrade(&image);
    return Some(image);
}
