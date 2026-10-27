//! Publish each complete Cargo-owned runtime bundle with its own v5 manifest.

#[path = "output.rs"]
mod output;

use scriptw_aot_wasm::RuntimeSourceCompilationProfile;
use std::path::Path;

pub(super) fn publish_intl_metadata(out: &Path, metadata: &[u8]) {
    output::write(&out.join("scriptw-runtime-intl-metadata.bin"), metadata)
        .expect("checked Intl build metadata writes completely");
}

pub(super) fn publish(
    out: &Path,
    profile: RuntimeSourceCompilationProfile,
    package: &[u8],
    native: &[u8],
    manifest: &[u8],
    metadata: &[u8],
    execution: &[u8],
) {
    let prefix = match profile {
        RuntimeSourceCompilationProfile::NativeHost => "scriptw-runtime",
        RuntimeSourceCompilationProfile::Portable => "scriptw-runtime-portable",
    };
    // Cargo admits the producer only after both complete package groups are written.
    for (suffix, bytes) in [
        ("package", package),
        ("native", native),
        ("manifest", manifest),
        ("metadata", metadata),
        ("execution", execution),
    ] {
        output::write(&out.join(format!("{prefix}-{suffix}.bin")), bytes)
            .expect("build runtime output writes completely");
    }
}
