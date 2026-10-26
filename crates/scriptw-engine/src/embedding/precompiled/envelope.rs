//! Bounded framing detects damage; it grants no native-code authority.
use super::*;
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 8] = b"SCRWEP01";
const MAX_BYTES: usize = 256 * 1024 * 1024;

pub(super) struct Frame {
    pub(super) program: Range<usize>,
    pub(super) native: Range<usize>,
    pub(super) runtime_key: Option<[u8; 32]>,
    pub(super) mode: WasmNativeCompilationMode,
}

fn invalid() -> EngineError {
    EngineError::new("precompiled image failed bounded framing, source or checksum admission")
}

fn take<'a>(bytes: &mut &'a [u8], count: usize) -> Result<&'a [u8], EngineError> {
    let (value, rest) = bytes.split_at_checked(count).ok_or_else(invalid)?;
    *bytes = rest;
    Ok(value)
}

pub(super) fn encode(
    program: &[u8],
    native: &[u8],
    runtime_key: Option<[u8; 32]>,
    mode: WasmNativeCompilationMode,
) -> Result<Vec<u8>, EngineError> {
    let length = program
        .len()
        .checked_add(native.len())
        .and_then(|n| n.checked_add(122))
        .ok_or_else(invalid)?;
    if length > MAX_BYTES {
        return Err(invalid());
    }
    let mut bytes = Vec::with_capacity(length);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(
        compiler_identity::build_source_fingerprint()
            .map_err(EngineError::new)?
            .as_bytes(),
    );
    bytes.push(u8::from(runtime_key.is_some()));
    bytes.extend_from_slice(&runtime_key.unwrap_or([0; 32]));
    bytes.push(match mode {
        WasmNativeCompilationMode::Fast => 0,
        WasmNativeCompilationMode::SizeOptimized => 1,
        WasmNativeCompilationMode::Speed => 2,
    });
    bytes.extend_from_slice(&(program.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&(native.len() as u64).to_le_bytes());
    bytes.extend_from_slice(program);
    bytes.extend_from_slice(native);
    bytes.extend_from_slice(&Sha256::digest(&bytes));
    Ok(bytes)
}

pub(super) fn decode(bytes: &[u8]) -> Result<Frame, EngineError> {
    if bytes.len() > MAX_BYTES || bytes.len() < 122 {
        return Err(invalid());
    }
    let body_end = bytes.len().checked_sub(32).ok_or_else(invalid)?;
    let (body, digest) = bytes.split_at(body_end);
    if Sha256::digest(body).as_slice() != digest {
        return Err(invalid());
    }
    let mut rest = body;
    if take(&mut rest, 8)? != MAGIC
        || take(&mut rest, 32)?
            != compiler_identity::build_source_fingerprint()
                .map_err(EngineError::new)?
                .as_bytes()
    {
        return Err(invalid());
    }
    let linked = take(&mut rest, 1)?[0];
    let key: [u8; 32] = take(&mut rest, 32)?.try_into().map_err(|_| invalid())?;
    let runtime_key = match linked {
        0 if key == [0; 32] => None,
        1 => Some(key),
        _ => return Err(invalid()),
    };
    let mode = match take(&mut rest, 1)?[0] {
        0 => WasmNativeCompilationMode::Fast,
        1 => WasmNativeCompilationMode::SizeOptimized,
        2 => WasmNativeCompilationMode::Speed,
        _ => return Err(invalid()),
    };
    let mut length = || {
        let word: [u8; 8] = take(&mut rest, 8)?.try_into().map_err(|_| invalid())?;
        usize::try_from(u64::from_le_bytes(word)).map_err(|_| invalid())
    };
    let program_length = length()?;
    let native_length = length()?;
    let start = body.len() - rest.len();
    take(&mut rest, program_length)?;
    take(&mut rest, native_length)?;
    if program_length == 0 || native_length == 0 || !rest.is_empty() {
        return Err(invalid());
    }
    Ok(Frame {
        program: start..start + program_length,
        native: start + program_length..body_end,
        runtime_key,
        mode,
    })
}
