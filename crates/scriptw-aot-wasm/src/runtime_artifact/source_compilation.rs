//! Source-compilation authority is explicit in the emitted module and identity.
use crate::gc_types::GcHostImport;
use crate::EmitError;
use wasmparser::{Import, Parser, Payload, TypeRef};

pub const RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION: &str =
    "scriptw.runtime.source-compilation.v1";

/// Portable artifacts contain compiled Wasm and no native compiler callback.
/// Native execution explicitly selects the host's JS-to-IR-to-Wasm compiler.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum RuntimeSourceCompilationProfile {
    #[default]
    Portable,
    NativeHost,
}

impl RuntimeSourceCompilationProfile {
    pub(crate) const fn wire(self) -> u8 {
        match self {
            Self::Portable => 0,
            Self::NativeHost => 1,
        }
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        match bytes {
            [0] => Some(Self::Portable),
            [1] => Some(Self::NativeHost),
            _ => None,
        }
    }

    /// Read one exact physical profile and its corresponding compiler imports.
    /// This checks authority metadata; ordinary Wasm/type validation is separate.
    pub fn from_module(bytes: &[u8]) -> Result<Self, EmitError> {
        let mut admission = SourceCompilationProfileAdmission::default();
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.map_err(|_| Self::malformed())? {
                Payload::ImportSection(section) => {
                    for import in section.into_imports() {
                        admission.import(&import.map_err(|_| Self::malformed())?)?;
                    }
                }
                Payload::CustomSection(section)
                    if section.name() == RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION =>
                {
                    admission.section(section.data())?;
                }
                _ => {}
            }
        }
        admission.finish()
    }

    fn malformed() -> EmitError {
        EmitError::unsupported("runtime source-compilation profile or compiler imports mismatch")
    }
}

/// The source-compiler imports' slots, one per
/// [`GcHostImport::SOURCE_COMPILERS`] row.
const COMPILER_IMPORTS: [GcHostImport; 3] = GcHostImport::SOURCE_COMPILERS;

fn compiler_import_slot(name: &str) -> Option<usize> {
    COMPILER_IMPORTS.iter().position(|row| row.name() == name)
}

#[derive(Default)]
pub(crate) struct SourceCompilationProfileAdmission {
    profile: Option<RuntimeSourceCompilationProfile>,
    imports: [bool; COMPILER_IMPORTS.len()],
}

impl SourceCompilationProfileAdmission {
    pub(crate) fn import(&mut self, import: &Import<'_>) -> Result<(), EmitError> {
        let Some(slot) = compiler_import_slot(import.name) else {
            return Ok(());
        };
        if import.module != COMPILER_IMPORTS[slot].module()
            || !matches!(import.ty, TypeRef::Func(_) | TypeRef::FuncExact(_))
            || self.imports[slot]
        {
            return Err(RuntimeSourceCompilationProfile::malformed());
        }
        self.imports[slot] = true;
        Ok(())
    }

    pub(crate) fn section(&mut self, bytes: &[u8]) -> Result<(), EmitError> {
        if self.profile.is_some() {
            return Err(RuntimeSourceCompilationProfile::malformed());
        }
        self.profile = Some(
            RuntimeSourceCompilationProfile::decode(bytes)
                .ok_or_else(RuntimeSourceCompilationProfile::malformed)?,
        );
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<RuntimeSourceCompilationProfile, EmitError> {
        let profile = self
            .profile
            .ok_or_else(RuntimeSourceCompilationProfile::malformed)?;
        let expected = match profile {
            RuntimeSourceCompilationProfile::Portable => [false; COMPILER_IMPORTS.len()],
            RuntimeSourceCompilationProfile::NativeHost => [true; COMPILER_IMPORTS.len()],
        };
        if self.imports != expected {
            return Err(RuntimeSourceCompilationProfile::malformed());
        }
        Ok(profile)
    }
}

#[cfg(test)]
mod tests;
