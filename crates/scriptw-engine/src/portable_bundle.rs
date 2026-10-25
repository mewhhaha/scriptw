//! Strict persistence of portable P plus its exact raw R. No source resolver,
//! emitter fallback, native compiler callback or native-image deserialization.
use super::*;
use scriptw_aot_wasm::{
    admit_portable_runtime_package, encode_portable_runtime_package,
    RuntimeSourceCompilationProfile, RUNTIME_IMPORT_NAMESPACE,
};

const MAGIC: &[u8; 8] = b"SCRWPB01";
const MAX_BYTES: usize = 256 * 1024 * 1024;
const DIGEST_BYTES: usize = 32;

fn source_identity() -> Result<CompilerDigest, EngineError> {
    compiler_identity::build_source_fingerprint().map_err(EngineError::new)
}

fn invalid() -> EngineError {
    EngineError::new("portable bundle failed framing, source, checksum or runtime admission")
}

fn take<'a>(bytes: &mut &'a [u8], count: usize) -> Result<&'a [u8], EngineError> {
    let (head, tail) = bytes.split_at_checked(count).ok_or_else(invalid)?;
    *bytes = tail;
    Ok(head)
}

fn length(bytes: &mut &[u8]) -> Result<usize, EngineError> {
    let raw: [u8; 8] = take(bytes, 8)?.try_into().map_err(|_| invalid())?;
    usize::try_from(u64::from_le_bytes(raw)).map_err(|_| invalid())
}

fn validate_program(bytes: &[u8], linked: bool) -> Result<(), EngineError> {
    // P owns its explicit native imports. Admit their persisted ABI metadata
    // and actual scalar/tag signatures independently of source compilation.
    scriptw_aot_wasm::foreign_registry_from_module(bytes).map_err(|error| {
        EngineError::new(format!("bundle foreign ABI admission failed: {error}"))
    })?;
    if RuntimeSourceCompilationProfile::from_module(bytes)
        .map_err(|error| EngineError::new(error.to_string()))?
        != RuntimeSourceCompilationProfile::Portable
    {
        return Err(EngineError::new("bundle requires portable compiled Wasm"));
    }
    let mut imports_runtime = false;
    for payload in WasmParser::new(0).parse_all(bytes) {
        if let WasmPayload::ImportSection(section) = payload.map_err(|_| invalid())? {
            for import in section.into_imports() {
                imports_runtime |=
                    import.map_err(|_| invalid())?.module == RUNTIME_IMPORT_NAMESPACE;
            }
        }
    }
    if imports_runtime != linked {
        return Err(EngineError::new("bundle program/runtime linkage differs"));
    }
    // Validate inert Wasm under the product capability set. This does not
    // compile JS or Wasm machine code, instantiate, or execute a start function.
    WasmtimeModule::validate(&shared_wasm_engine()?, bytes)
        .map_err(|error| EngineError::new(format!("bundle Wasm validation failed: {error}")))
}

impl Artifact {
    /// Encode raw portable Wasm with its matching runtime, without emitting
    /// either module. Only the two pinned Intl profiles are currently portable
    /// in this container format; custom projections need an explicit manifest.
    /// The result is raw Wasm, not a precompiled native-code image.
    pub fn encode_portable_bundle(
        &self,
        intl_profile: &IntlCompilationProfile,
    ) -> Result<Vec<u8>, EngineError> {
        run_on_sized_stack(|| self.encode_portable_bundle_inner(intl_profile))
    }

    fn encode_portable_bundle_inner(
        &self,
        intl_profile: &IntlCompilationProfile,
    ) -> Result<Vec<u8>, EngineError> {
        let profile = match intl_profile {
            IntlCompilationProfile::Minimal => 0,
            IntlCompilationProfile::Conformance => 1,
            IntlCompilationProfile::Custom(_) | IntlCompilationProfile::CustomProjection(_) => {
                return Err(EngineError::new(
                    "bundle does not encode custom Intl projections",
                ));
            }
        };
        validate_program(&self.bytes, self.runtime.is_some())?;
        let source = source_identity()?;
        let selection =
            super::embedded_runtime::inputs(None, RuntimeSourceCompilationProfile::Portable)
                .intl_selection(intl_profile)
                .map_err(|error| EngineError::new(error.to_string()))?;
        if self.runtime.is_none() {
            validate_standalone_intl(&self.bytes, &selection)?;
        }
        let runtime = self
            .runtime
            .as_ref()
            .map(|runtime| {
                encode_portable_runtime_package(&runtime.0, &selection, source.as_bytes())
                    .map_err(|error| EngineError::new(error.to_string()))
            })
            .transpose()?
            .unwrap_or_default();
        let size = 8usize + 1 + 32 + 8 + 8 + 32 + 32;
        let size = size
            .checked_add(self.bytes.len())
            .and_then(|n| n.checked_add(runtime.len()))
            .filter(|n| *n <= MAX_BYTES)
            .ok_or_else(invalid)?;
        let mut bundle = Vec::with_capacity(size);
        bundle.extend_from_slice(MAGIC);
        bundle.push(profile);
        bundle.extend_from_slice(source.as_bytes());
        bundle.extend_from_slice(&(self.bytes.len() as u64).to_le_bytes());
        bundle.extend_from_slice(&(runtime.len() as u64).to_le_bytes());
        bundle.extend_from_slice(
            &self
                .runtime
                .as_ref()
                .map(|r| *r.key().as_bytes())
                .unwrap_or([0; 32]),
        );
        bundle.extend_from_slice(&self.bytes);
        bundle.extend_from_slice(&runtime);
        let digest = Sha256::digest(&bundle);
        bundle.extend_from_slice(&digest);
        Ok(bundle)
    }
}

impl Engine {
    /// Admit an untrusted raw portable bundle. Invalid input is an error, never
    /// a request to parse/compile JavaScript or rebuild a missing runtime.
    /// Every call returns an inert artifact; execution still creates a fresh
    /// Store/Realm through the ordinary Wasm product path.
    pub fn load_portable_bundle(&self, bytes: &[u8]) -> Result<Artifact, EngineError> {
        run_on_sized_stack(|| self.load_portable_bundle_inner(bytes))
    }

    fn load_portable_bundle_inner(&self, bytes: &[u8]) -> Result<Artifact, EngineError> {
        if bytes.len() > MAX_BYTES {
            return Err(invalid());
        }
        let end = bytes.len().checked_sub(DIGEST_BYTES).ok_or_else(invalid)?;
        let (framed, digest) = bytes.split_at(end);
        let mut input = framed;
        if take(&mut input, 8)? != MAGIC {
            return Err(invalid());
        }
        let profile = match take(&mut input, 1)? {
            [0] => IntlCompilationProfile::Minimal,
            [1] => IntlCompilationProfile::Conformance,
            _ => return Err(invalid()),
        };
        if take(&mut input, 32)? != source_identity()?.as_bytes()
            || Sha256::digest(framed).as_slice() != digest
        {
            return Err(invalid());
        }
        let program_len = length(&mut input)?;
        let runtime_len = length(&mut input)?;
        let runtime_key = take(&mut input, 32)?;
        let program = take(&mut input, program_len)?;
        let package = take(&mut input, runtime_len)?;
        if !input.is_empty() {
            return Err(invalid());
        }
        validate_program(program, !package.is_empty())?;
        let selection =
            super::embedded_runtime::inputs(None, RuntimeSourceCompilationProfile::Portable)
                .intl_selection(&profile)
                .map_err(|error| EngineError::new(error.to_string()))?;
        let runtime = if package.is_empty() {
            if runtime_key != [0; 32] {
                return Err(invalid());
            }
            validate_standalone_intl(program, &selection)?;
            None
        } else {
            let runtime =
                admit_portable_runtime_package(package, &selection, source_identity()?.as_bytes())
                    .map_err(|error| EngineError::new(error.to_string()))?;
            if runtime.key().as_bytes() != runtime_key {
                return Err(invalid());
            }
            Some(LinkedRuntime(runtime))
        };
        Ok(Artifact {
            bytes: program.to_vec(),
            description: "admitted portable Wasm bundle".into(),
            debug_dump: String::new(),
            runtime,
            compilation_policy: None,
        })
    }
}

fn validate_standalone_intl(
    program: &[u8],
    selection: &IntlDataSelection,
) -> Result<(), EngineError> {
    let selected = selection
        .compilation_data()
        .map_err(|error| EngineError::new(error.to_string()))?;
    validate_wasm_intl_artifact_identity(
        program,
        selected.identity(),
        IntlArtifactIdentityRequirement::HostImports,
    )
}

#[cfg(test)]
mod tests;
