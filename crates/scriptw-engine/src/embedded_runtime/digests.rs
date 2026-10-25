//! Hash the same immutable Cargo images within the caller's CPU budget.

use sha2::{Digest, Sha256};

pub(super) fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
