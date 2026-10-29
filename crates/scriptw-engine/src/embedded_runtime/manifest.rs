//! Fixed manifest shared by the Cargo producer and the embedded-only loader.

use crate::wasmtime_config::WasmNativeCompilationMode;
const MAGIC: &[u8; 8] = b"SCRWNR01";
const VERSION: u32 = 5;

/// Only the immutable Cargo-produced image uses the optimizing allocator.
/// Its native stack maps are emitted by Wasmtime and remain admission-checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeRegisterAllocation {
    SinglePass,
    Backtracking,
}

impl RuntimeRegisterAllocation {
    pub(crate) fn algorithm(self) -> wasmtime::RegallocAlgorithm {
        match self {
            Self::SinglePass => wasmtime::RegallocAlgorithm::SinglePass,
            Self::Backtracking => wasmtime::RegallocAlgorithm::Backtracking,
        }
    }
}

/// Explicit execution authority, independent of the image's compiler settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionModes {
    Fast,
    SizeOptimized,
    FastAndSizeOptimized,
    FastSizeOptimizedAndSpeed,
}

impl ExecutionModes {
    pub(crate) fn contains(self, mode: WasmNativeCompilationMode) -> bool {
        match self {
            Self::Fast => mode == WasmNativeCompilationMode::Fast,
            Self::SizeOptimized => mode == WasmNativeCompilationMode::SizeOptimized,
            Self::FastAndSizeOptimized => matches!(
                mode,
                WasmNativeCompilationMode::Fast | WasmNativeCompilationMode::SizeOptimized
            ),
            Self::FastSizeOptimizedAndSpeed => match mode {
                WasmNativeCompilationMode::Fast
                | WasmNativeCompilationMode::SizeOptimized
                | WasmNativeCompilationMode::Speed => true,
            },
        }
    }
}

/// Image optimization is independent of the Engine that owns the loaded R.
/// Wasmtime still checks the actual destination Engine's compatibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CompilationModes {
    pub(crate) runtime: WasmNativeCompilationMode,
    pub(crate) execution: ExecutionModes,
    pub(crate) register_allocation: RuntimeRegisterAllocation,
}

pub(crate) const COMPILATION_MODES: CompilationModes = CompilationModes {
    runtime: WasmNativeCompilationMode::SizeOptimized,
    execution: ExecutionModes::FastSizeOptimizedAndSpeed,
    register_allocation: RuntimeRegisterAllocation::Backtracking,
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Manifest {
    pub(crate) source: [u8; 32],
    pub(crate) target: String,
    pub(crate) configuration: u32,
    pub(crate) modes: CompilationModes,
    pub(crate) runtime_key: [u8; 32],
    pub(crate) package_digest: [u8; 32],
    pub(crate) native_digest: [u8; 32],
}

impl Manifest {
    // This shared module is also compiled by the Cargo image producer.
    #[allow(dead_code)]
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.configuration.to_le_bytes());
        bytes.push(match self.modes.runtime {
            WasmNativeCompilationMode::Fast => 0,
            WasmNativeCompilationMode::SizeOptimized => 1,
            WasmNativeCompilationMode::Speed => 2,
        });
        bytes.push(match self.modes.execution {
            ExecutionModes::Fast => 0,
            ExecutionModes::SizeOptimized => 1,
            ExecutionModes::FastAndSizeOptimized => 2,
            ExecutionModes::FastSizeOptimizedAndSpeed => 3,
        });
        bytes.push(match self.modes.register_allocation {
            RuntimeRegisterAllocation::SinglePass => 0,
            RuntimeRegisterAllocation::Backtracking => 1,
        });
        bytes.extend_from_slice(
            &u16::try_from(self.target.len())
                .expect("target triple fits manifest")
                .to_le_bytes(),
        );
        bytes.extend_from_slice(self.target.as_bytes());
        for digest in [
            &self.source,
            &self.runtime_key,
            &self.package_digest,
            &self.native_digest,
        ] {
            bytes.extend_from_slice(digest);
        }
        bytes
    }

    pub(crate) fn decode(mut bytes: &[u8]) -> Option<Self> {
        fn take<const N: usize>(bytes: &mut &[u8]) -> Option<[u8; N]> {
            let (field, rest) = bytes.split_at_checked(N)?;
            *bytes = rest;
            field.try_into().ok()
        }
        if &take::<8>(&mut bytes)? != MAGIC || u32::from_le_bytes(take(&mut bytes)?) != VERSION {
            return None;
        }
        let configuration = u32::from_le_bytes(take(&mut bytes)?);
        fn mode(byte: u8) -> Option<WasmNativeCompilationMode> {
            match byte {
                0 => Some(WasmNativeCompilationMode::Fast),
                1 => Some(WasmNativeCompilationMode::SizeOptimized),
                2 => Some(WasmNativeCompilationMode::Speed),
                _ => None,
            }
        }
        let modes = CompilationModes {
            runtime: mode(take::<1>(&mut bytes)?[0])?,
            execution: match take::<1>(&mut bytes)?[0] {
                0 => ExecutionModes::Fast,
                1 => ExecutionModes::SizeOptimized,
                2 => ExecutionModes::FastAndSizeOptimized,
                3 => ExecutionModes::FastSizeOptimizedAndSpeed,
                _ => return None,
            },
            register_allocation: match take::<1>(&mut bytes)?[0] {
                0 => RuntimeRegisterAllocation::SinglePass,
                1 => RuntimeRegisterAllocation::Backtracking,
                _ => return None,
            },
        };
        let length = usize::from(u16::from_le_bytes(take(&mut bytes)?));
        if length > 256 {
            return None;
        }
        let (target, rest) = bytes.split_at_checked(length)?;
        bytes = rest;
        let result = Self {
            source: take(&mut bytes)?,
            target: std::str::from_utf8(target).ok()?.to_owned(),
            configuration,
            modes,
            runtime_key: take(&mut bytes)?,
            package_digest: take(&mut bytes)?,
            native_digest: take(&mut bytes)?,
        };
        // Fixed-width fields and exact enum tags have one encoding. The bounded
        // target length and strict UTF-8 preserve its exact bytes, so EOF proves
        // canonical framing without serializing the parsed manifest again.
        bytes.is_empty().then_some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_rejects_invalid_utf8_with_complete_fixed_fields() {
        let manifest = Manifest {
            source: [1; 32],
            target: "was".to_owned(),
            configuration: 1,
            modes: COMPILATION_MODES,
            runtime_key: [2; 32],
            package_digest: [3; 32],
            native_digest: [4; 32],
        };
        let encoded = manifest.encode();
        for target in [[0xff, b'a', b's'], [0xc0, 0xaf, b's'], [0xed, 0xa0, 0x80]] {
            let mut invalid = encoded.clone();
            invalid[21..24].copy_from_slice(&target);
            assert!(Manifest::decode(&invalid).is_none());
        }
    }
}

pub(crate) fn source_identity(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut result = [0; 32];
    for (output, pair) in result.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
        let high = char::from(pair[0]).to_digit(16)?;
        let low = char::from(pair[1]).to_digit(16)?;
        *output = ((high << 4) | low) as u8;
    }
    Some(result)
}
