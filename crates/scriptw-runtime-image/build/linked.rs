//! Attest both raw native images for direct linking. The checked producer's
//! bytes are linked unchanged, so a loaded image is its executable's own
//! pages. This step hashes exactly the file `objects` links; the engine
//! compares that digest with the producer's independent native manifest.
#[path = "../../scriptw-engine/src/embedded_runtime/output.rs"]
mod output;

use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) fn write(out: &Path) {
    let native_host_only = std::env::var_os("CARGO_FEATURE_NATIVE_HOST_ONLY").is_some();
    let prefixes: &[&str] = if native_host_only {
        &["scriptw-runtime"]
    } else {
        &["scriptw-runtime", "scriptw-runtime-portable"]
    };
    for &prefix in prefixes {
        let raw = std::fs::read(out.join(format!("{prefix}-native.bin")))
            .expect("the checked native producer published its image");
        // An unsupported target publishes empty images and links none.
        let digest = if raw.is_empty() {
            Vec::new()
        } else {
            println!("{prefix} native storage: {} raw bytes", raw.len());
            Sha256::digest(&raw).to_vec()
        };
        output::write(&out.join(format!("{prefix}-native.sha256")), digest)
            .expect("native image digest writes completely");
    }
}
