use super::*;
use scriptw_aot_wasm::{
    RuntimeArtifact, RuntimeSourceCompilationProfile, RUNTIME_IMPORT_NAMESPACE,
};

pub(super) fn program(bytes: &[u8], linked: bool) -> Result<ForeignModuleRegistry, EngineError> {
    if RuntimeSourceCompilationProfile::from_module(bytes)
        .map_err(|error| EngineError::new(error.to_string()))?
        != RuntimeSourceCompilationProfile::Portable
    {
        return Err(EngineError::new(
            "precompiled embedding requires Portable Wasm",
        ));
    }
    let foreign = scriptw_aot_wasm::foreign_registry_from_module(bytes).map_err(|error| {
        EngineError::new(format!("precompiled foreign ABI admission failed: {error}"))
    })?;
    let mut imports_runtime = false;
    let mut has_intl = false;
    for payload in WasmParser::new(0).parse_all(bytes) {
        match payload.map_err(|error| EngineError::new(error.to_string()))? {
            WasmPayload::StartSection { .. } => {
                return Err(EngineError::new(
                    "embedding rejects implicit Wasm start functions",
                ))
            }
            WasmPayload::CustomSection(section) => {
                has_intl |= section.name().starts_with("scriptw.intl")
            }
            WasmPayload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.map_err(|error| EngineError::new(error.to_string()))?;
                    imports_runtime |= import.module == RUNTIME_IMPORT_NAMESPACE;
                    has_intl |= import.module == WASM_HOST_IMPORT_NAMESPACE
                        && matches!(
                            import.name,
                            WASM_HOST_IMPORT_INTL_CALL | WASM_HOST_IMPORT_SYSTEM_TIME_ZONE
                        );
                }
            }
            _ => {}
        }
    }
    if imports_runtime != linked {
        return Err(EngineError::new(
            "precompiled program/runtime linkage differs",
        ));
    }
    if !linked && has_intl {
        return Err(EngineError::new(
            "precompiled Intl requires the current Cargo Minimal Portable runtime",
        ));
    }
    Ok(foreign)
}

pub(super) fn cargo_runtime(key: &[u8; 32]) -> Result<Arc<RuntimeArtifact>, EngineError> {
    let inputs = embedded_runtime::inputs(None, RuntimeSourceCompilationProfile::Portable);
    let selection = inputs
        .intl_selection(&IntlCompilationProfile::Minimal)
        .map_err(|error| EngineError::new(error.to_string()))?;
    let runtime = inputs.admit_supplied_package(&selection).map_err(|error| {
        EngineError::new(format!("Cargo Portable runtime admission failed: {error}"))
    })?;
    if runtime.key().as_bytes() != key {
        return Err(EngineError::new(
            "precompiled program requires a different Cargo runtime key or Intl profile",
        ));
    }
    Ok(runtime)
}
