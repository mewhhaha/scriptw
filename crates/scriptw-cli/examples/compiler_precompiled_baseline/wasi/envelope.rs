//! Inert framing only. Successful decoding never grants native-code authority.
use super::*;
use std::ops::Range;

const MAGIC: &[u8; 8] = b"SCRWWA01";
const HEADER: usize = 56;
const DIGEST: usize = 32;
const MAX_IMAGE: usize = 256 * 1024 * 1024;

pub(super) struct Frame {
    pub(super) program: Range<usize>,
    pub(super) native: Range<usize>,
}

pub(crate) fn current_fingerprint() -> Result<[u8; 32], String> {
    let value = CompilerIdentity::current()?
        .source_fingerprint()
        .to_string();
    let mut fingerprint = [0; 32];
    if value.len() != 64 {
        return Err("invalid compiler fingerprint".into());
    }
    for (index, byte) in fingerprint.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| "invalid compiler fingerprint")?;
    }
    Ok(fingerprint)
}

pub(crate) fn encode(
    program: &[u8],
    native: &[u8],
    fingerprint: &[u8; 32],
) -> Result<Vec<u8>, String> {
    let length = HEADER
        .checked_add(program.len())
        .and_then(|n| n.checked_add(native.len()))
        .and_then(|n| n.checked_add(DIGEST))
        .ok_or("WASI image size overflow")?;
    if length > MAX_IMAGE || program.len() < 8 || native.is_empty() {
        return Err("WASI image lengths are outside bounds".into());
    }
    let mut bytes = Vec::with_capacity(length);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(fingerprint);
    bytes.extend_from_slice(&(program.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&(native.len() as u64).to_le_bytes());
    bytes.extend_from_slice(program);
    bytes.extend_from_slice(native);
    let checksum = Sha256::digest(&bytes);
    bytes.extend_from_slice(&checksum);
    Ok(bytes)
}

pub(super) fn decode(bytes: &[u8], fingerprint: &[u8; 32]) -> Result<Frame, String> {
    if !(HEADER + 8 + 1 + DIGEST..=MAX_IMAGE).contains(&bytes.len()) {
        return Err("WASI image size outside bounds".into());
    }
    if &bytes[..8] != MAGIC || &bytes[8..40] != fingerprint {
        return Err("WASI image magic or compiler identity mismatch".into());
    }
    let length = |offset| -> Result<usize, String> {
        let raw = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        usize::try_from(raw).map_err(|_| "WASI image length overflow".into())
    };
    let program_len = length(40)?;
    let native_len = length(48)?;
    let program_end = HEADER
        .checked_add(program_len)
        .ok_or("WASI program length overflow")?;
    let native_end = program_end
        .checked_add(native_len)
        .ok_or("WASI native length overflow")?;
    if program_len < 8 || native_len == 0 || native_end.checked_add(DIGEST) != Some(bytes.len()) {
        return Err("WASI image section lengths mismatch".into());
    }
    if Sha256::digest(&bytes[..native_end]).as_slice() != &bytes[native_end..] {
        return Err("WASI image checksum mismatch".into());
    }
    Ok(Frame {
        program: HEADER..program_end,
        native: program_end..native_end,
    })
}

fn section_length(bytes: &[u8], cursor: &mut usize) -> Result<usize, String> {
    let mut value = 0u32;
    for index in 0..5 {
        let byte = *bytes.get(*cursor).ok_or("truncated Wasm section length")?;
        *cursor += 1;
        if index == 4 && byte & 0xf0 != 0 {
            return Err("Wasm section length overflow".into());
        }
        value |= u32::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            return Ok(value as usize);
        }
    }
    Err("Wasm section length overflow".into())
}

/// A bounded section scan rejects implicit instantiation execution. Full Wasm
/// validation still belongs to Module::new/validate, before deserialization.
pub(crate) fn reject_implicit_start(bytes: &[u8]) -> Result<(), String> {
    if bytes.get(..8) != Some(b"\0asm\x01\0\0\0".as_slice()) {
        return Err("expected a core Wasm module".into());
    }
    let mut cursor = 8usize;
    while cursor < bytes.len() {
        let section = bytes[cursor];
        cursor += 1;
        let length = section_length(bytes, &mut cursor)?;
        let end = cursor
            .checked_add(length)
            .ok_or("Wasm section range overflow")?;
        if end > bytes.len() {
            return Err("truncated Wasm section".into());
        }
        if section == 8 {
            return Err("implicit Wasm start sections are forbidden".into());
        }
        cursor = end;
    }
    Ok(())
}

#[cfg(test)]
#[path = "envelope/tests.rs"]
mod tests;
