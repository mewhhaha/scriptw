//! The Cargo publisher supplements the compiler receipt with a body-size fact.
//! Both the original receipt and the extra build fact have framing checks.

use sha2::{Digest, Sha256};

const MAX_BYTES: usize = 8192 + 8 + 32;

#[allow(dead_code)] // Producer side of the shared format.
pub(super) fn encode(mut receipt: Vec<u8>, wasm: &[u8]) -> Option<Vec<u8>> {
    receipt.extend_from_slice(&largest_code_body_bytes(wasm)?.to_le_bytes());
    let checksum = Sha256::digest(&receipt);
    receipt.extend_from_slice(&checksum);
    (receipt.len() <= MAX_BYTES).then_some(receipt)
}

#[allow(dead_code)] // Engine side of the shared format.
pub(super) fn decode(bytes: &[u8]) -> Option<(&[u8], u64)> {
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let (body, checksum) = bytes.split_at_checked(bytes.len().checked_sub(32)?)?;
    if Sha256::digest(body).as_slice() != checksum {
        return None;
    }
    let (receipt, size) = body.split_at_checked(body.len().checked_sub(8)?)?;
    let size = u64::from_le_bytes(size.try_into().ok()?);
    (size != 0).then_some((receipt, size))
}

/// Build-only size inventory of already validated Wasm. Read the section and
/// body lengths without decoding instructions.
fn largest_code_body_bytes(wasm: &[u8]) -> Option<u64> {
    fn read_size(bytes: &mut &[u8]) -> Option<usize> {
        let mut size = 0_u32;
        for shift in (0..32).step_by(7) {
            let (&byte, rest) = bytes.split_first()?;
            *bytes = rest;
            if shift == 28 && byte > 0x0f {
                return None;
            }
            size |= u32::from(byte & 0x7f) << shift;
            if byte < 0x80 {
                return usize::try_from(size).ok();
            }
        }
        None
    }

    let mut sections = wasm.strip_prefix(b"\0asm\x01\0\0\0")?;
    while let Some((&id, rest)) = sections.split_first() {
        sections = rest;
        let len = read_size(&mut sections)?;
        let (mut section, next) = sections.split_at_checked(len)?;
        if id == 10 {
            let count = read_size(&mut section)?;
            let mut largest = 0;
            for _ in 0..count {
                let size = read_size(&mut section)?;
                largest = largest.max(size);
                section = section.get(size..)?;
            }
            return (section.is_empty() && largest > 0).then_some(largest as u64);
        }
        sections = next;
    }
    None
}
