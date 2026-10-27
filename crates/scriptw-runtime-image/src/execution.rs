//! Bounded wire facts, never native-code or immutable-build authority.
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 8] = b"SCRWEX01";
const MAX_BYTES: usize = 8192;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionFrame {
    pub component: u16,
    pub start: u64,
    pub length: u64,
    pub digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionReceipt<'a> {
    pub layout: u32,
    pub profile: u8,
    pub source: [u8; 32],
    pub key: [u8; 32],
    pub package_length: u64,
    pub wasm_start: u64,
    pub wasm_length: u64,
    pub package_digest: [u8; 32],
    pub envelope_digest: [u8; 32],
    pub wasm_digest: [u8; 32],
    pub manifest_digest: [u8; 32],
    pub intl_digest: [u8; 32],
    pub intl_identity: &'a [u8],
    pub frames: Vec<ExecutionFrame>,
}

impl ExecutionReceipt<'_> {
    pub fn encode(&self) -> Option<Vec<u8>> {
        if self.intl_identity.len() > 4096 || self.frames.len() != 12 {
            return None;
        }
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&self.layout.to_le_bytes());
        out.push(self.profile);
        out.extend_from_slice(&self.source);
        out.extend_from_slice(&self.key);
        for n in [self.package_length, self.wasm_start, self.wasm_length] {
            out.extend_from_slice(&n.to_le_bytes());
        }
        for digest in [
            &self.package_digest,
            &self.envelope_digest,
            &self.wasm_digest,
            &self.manifest_digest,
            &self.intl_digest,
        ] {
            out.extend_from_slice(digest);
        }
        out.extend_from_slice(&(self.intl_identity.len() as u32).to_le_bytes());
        out.extend_from_slice(self.intl_identity);
        for frame in &self.frames {
            out.extend_from_slice(&frame.component.to_le_bytes());
            out.extend_from_slice(&frame.start.to_le_bytes());
            out.extend_from_slice(&frame.length.to_le_bytes());
            out.extend_from_slice(&frame.digest);
        }
        if out.len() + 32 > MAX_BYTES {
            return None;
        }
        out.extend_from_slice(&Sha256::digest(&out));
        Some(out)
    }
}

impl<'a> ExecutionReceipt<'a> {
    pub fn decode(bytes: &'a [u8]) -> Option<Self> {
        fn take<const N: usize>(bytes: &mut &[u8]) -> Option<[u8; N]> {
            let (head, tail) = bytes.split_at_checked(N)?;
            *bytes = tail;
            head.try_into().ok()
        }
        if bytes.len() > MAX_BYTES {
            return None;
        }
        let (mut body, digest) = bytes.split_at_checked(bytes.len().checked_sub(32)?)?;
        if Sha256::digest(body).as_slice() != digest || &take::<8>(&mut body)? != MAGIC {
            return None;
        }
        let layout = u32::from_le_bytes(take(&mut body)?);
        let profile = take::<1>(&mut body)?[0];
        let source = take(&mut body)?;
        let key = take(&mut body)?;
        let package_length = u64::from_le_bytes(take(&mut body)?);
        let wasm_start = u64::from_le_bytes(take(&mut body)?);
        let wasm_length = u64::from_le_bytes(take(&mut body)?);
        if wasm_length == 0
            || wasm_start.checked_add(wasm_length)? != package_length.checked_sub(32)?
        {
            return None;
        }
        let package_digest = take(&mut body)?;
        let envelope_digest = take(&mut body)?;
        let wasm_digest = take(&mut body)?;
        let manifest_digest = take(&mut body)?;
        let intl_digest = take(&mut body)?;
        let length = u32::from_le_bytes(take(&mut body)?) as usize;
        if length > 4096 {
            return None;
        }
        let (identity, rest) = body.split_at_checked(length)?;
        body = rest;
        let intl_identity = identity;
        let mut frames = Vec::with_capacity(12);
        for expected in 0..12 {
            let component = u16::from_le_bytes(take(&mut body)?);
            if component != expected {
                return None;
            }
            let start = u64::from_le_bytes(take(&mut body)?);
            let length = u64::from_le_bytes(take(&mut body)?);
            let digest = take(&mut body)?;
            if start < wasm_start
                || start.checked_add(length)? > wasm_start.checked_add(wasm_length)?
            {
                return None;
            }
            if frames.iter().any(|frame: &ExecutionFrame| {
                frame.component == component
                    || (length > 0
                        && frame.length > 0
                        && start < frame.start + frame.length
                        && frame.start < start + length)
            }) {
                return None;
            }
            frames.push(ExecutionFrame {
                component,
                start,
                length,
                digest,
            });
        }
        if !body.is_empty() {
            return None;
        }
        let receipt = Self {
            layout,
            profile,
            source,
            key,
            package_length,
            wasm_start,
            wasm_length,
            package_digest,
            envelope_digest,
            wasm_digest,
            manifest_digest,
            intl_digest,
            intl_identity,
            frames,
        };
        (receipt.encode()?.as_slice() == bytes).then_some(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> ExecutionReceipt<'static> {
        ExecutionReceipt {
            layout: 24,
            profile: 0,
            source: [1; 32],
            key: [2; 32],
            package_length: 16384 + 32,
            wasm_start: 256,
            wasm_length: 16384 - 256,
            package_digest: [3; 32],
            envelope_digest: [4; 32],
            wasm_digest: [5; 32],
            manifest_digest: [6; 32],
            intl_digest: [7; 32],
            intl_identity: b"identity",
            frames: (0..12)
                .map(|component| ExecutionFrame {
                    component,
                    start: 1024 + u64::from(component) * 512,
                    length: 128,
                    digest: [component as u8; 32],
                })
                .collect(),
        }
    }

    #[test]
    fn compact_receipt_exact_roundtrip_and_corruption_rejection() {
        let receipt = receipt();
        let bytes = receipt.encode().unwrap();
        assert_eq!(ExecutionReceipt::decode(&bytes), Some(receipt));
        for end in 0..bytes.len() {
            assert!(ExecutionReceipt::decode(&bytes[..end]).is_none());
        }
        for index in [0, 8, 13, 45, 109, bytes.len() - 1] {
            let mut corrupt = bytes.clone();
            corrupt[index] ^= 1;
            assert!(ExecutionReceipt::decode(&corrupt).is_none());
        }
        let mut trailing = bytes;
        trailing.insert(trailing.len() - 32, 0);
        let end = trailing.len() - 32;
        let digest = Sha256::digest(&trailing[..end]);
        trailing[end..].copy_from_slice(&digest);
        assert!(ExecutionReceipt::decode(&trailing).is_none());
    }

    #[test]
    fn compact_receipt_rejects_overlapping_foreign_and_overflowing_frame_ranges() {
        for invalid in 0..5 {
            let mut receipt = receipt();
            match invalid {
                0 => receipt.frames[1].start = receipt.frames[0].start + 1,
                1 => receipt.frames[0].start = 0,
                2 => receipt.frames[0].length = u64::MAX,
                3 => receipt.frames[0].component = 99,
                4 => receipt.wasm_length += 1,
                _ => unreachable!(),
            }
            assert!(ExecutionReceipt::decode(&receipt.encode().unwrap()).is_none());
        }
    }
}
