//! Physical Intl frames stay with the module that owns the host imports.
use super::*;
use scriptw_intl::INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION;
use std::borrow::Cow;

pub(super) fn append_sections(
    module: &mut wasm_encoder::Module,
    kind: ModuleKind<'_>,
    string_pool: &StringPool,
    intl_selection: &IntlDataSelection,
    uses_intl_host: bool,
    uses_system_time_zone: bool,
    debug_dump: &mut Vec<String>,
) -> Result<(), EmitError> {
    let uses_intl_catalogue = string_pool.check_intl_supported_values()?;
    if kind.owns_globals() && (uses_intl_host || uses_system_time_zone || uses_intl_catalogue) {
        let selected = intl_selection.compilation_data().map_err(|error| {
            EmitError::unsupported(format!("failed to select the Intl artifact data: {error}"))
        })?;
        let artifact_identity = selected.identity().artifact_identity();
        module.section(&wasm_encoder::CustomSection {
            name: Cow::Borrowed(INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION),
            data: Cow::Borrowed(artifact_identity.as_bytes()),
        });
        debug_dump.push(format!(
            "custom section: {INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION} ({} bytes)",
            artifact_identity.as_bytes().len()
        ));
        if let Some(services) = selected.service_selection() {
            let name = scriptw_intl::INTL_SERVICE_SELECTION_CUSTOM_SECTION;
            let wire = services.wire().to_le_bytes();
            module.section(&wasm_encoder::CustomSection {
                name: Cow::Borrowed(name),
                data: Cow::Borrowed(&wire),
            });
            debug_dump.push(format!("custom section: {name} ({} bytes)", wire.len()));
        }
        for (name, data) in selected.component_sections() {
            module.section(&wasm_encoder::CustomSection {
                name: Cow::Borrowed(name),
                data: Cow::Borrowed(data.as_ref()),
            });
            debug_dump.push(format!(
                "custom section: {name} ({} bytes)",
                data.as_ref().len()
            ));
        }
    }

    Ok(())
}
