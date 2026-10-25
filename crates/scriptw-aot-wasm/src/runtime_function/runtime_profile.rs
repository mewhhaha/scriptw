//! Match actual physical Intl frames in the supplied immutable runtime bytes.
//! Pool dimensions alone cannot distinguish same-name custom profiles.
use crate::EmitError;
use scriptw_intl::{
    IntlDataSelection, INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION, INTL_SERVICE_SELECTION_CUSTOM_SECTION,
};
use std::{collections::BTreeMap, sync::Arc};
use wasmparser::{Parser, Payload};

pub(crate) fn require_runtime_profile(
    bytes: &[u8],
    selection: &IntlDataSelection,
) -> Result<(), EmitError> {
    let selected = selection.compilation_data().map_err(|error| {
        EmitError::unsupported(format!("failed to select native source Intl data: {error}"))
    })?;
    let mut expected: BTreeMap<&str, scriptw_intl::IntlCompilationBytes> =
        selected.component_sections().into_iter().collect();
    expected.insert(
        INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION,
        scriptw_intl::IntlCompilationBytes::Shared(Arc::from(
            selected.identity().artifact_identity().as_bytes(),
        )),
    );
    if let Some(services) = selected.service_selection() {
        expected.insert(
            INTL_SERVICE_SELECTION_CUSTOM_SECTION,
            scriptw_intl::IntlCompilationBytes::Shared(Arc::from(services.wire().to_le_bytes())),
        );
    }
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|error| {
            EmitError::unsupported(format!("invalid current runtime image: {error}"))
        })?;
        if let Payload::CustomSection(section) = payload {
            if section.name().starts_with("scriptw.intl") {
                if !expected
                    .remove(section.name())
                    .is_some_and(|value| value.as_ref() == section.data())
                {
                    return Err(EmitError::unsupported(
                        "native source Intl data differs from the actual current runtime",
                    ));
                }
            }
        }
    }
    if !expected.is_empty() {
        return Err(EmitError::unsupported(
            "current runtime omits the selected native source Intl data",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
