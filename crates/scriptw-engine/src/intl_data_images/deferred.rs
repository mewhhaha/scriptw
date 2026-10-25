//! Identity admission precedes execution; provider construction waits for an
//! actual Intl host operation. Custom/external data keeps its eager admission.
use super::*;
use std::sync::OnceLock;

#[derive(Clone)]
pub(crate) enum WasmIntlKernel {
    Absent,
    Ready(Arc<IntlKernel<EmbeddedIntlProvider>>),
    Minimal(Arc<DeferredMinimal>),
}

pub(crate) struct DeferredMinimal {
    data: Arc<scriptw_intl::IntlCompilationData>,
    resolved: OnceLock<Result<Arc<IntlKernel<EmbeddedIntlProvider>>, String>>,
}

impl From<Arc<IntlKernel<EmbeddedIntlProvider>>> for WasmIntlKernel {
    fn from(kernel: Arc<IntlKernel<EmbeddedIntlProvider>>) -> Self {
        Self::Ready(kernel)
    }
}
impl WasmIntlKernel {
    pub(crate) fn minimal(data: Arc<scriptw_intl::IntlCompilationData>) -> Option<Self> {
        scriptw_intl::IntlDataSelection::from_minimal_compilation_data(Arc::clone(&data))?;
        Some(Self::Minimal(Arc::new(DeferredMinimal {
            data,
            resolved: OnceLock::new(),
        })))
    }
    pub(crate) fn identity(&self) -> Option<&scriptw_intl::IntlDataIdentity> {
        match self {
            Self::Absent => None,
            Self::Ready(kernel) => Some(kernel.identity()),
            Self::Minimal(kernel) => Some(kernel.data.identity()),
        }
    }
    pub(crate) fn minimal_data(&self) -> Option<&scriptw_intl::IntlCompilationData> {
        match self {
            Self::Minimal(kernel) => Some(&kernel.data),
            Self::Absent | Self::Ready(_) => None,
        }
    }
    pub(crate) fn resolve(&self) -> Result<&IntlKernel<EmbeddedIntlProvider>, EngineError> {
        match self {
            Self::Absent => Err(EngineError::new("artifact has no Intl host imports")),
            Self::Ready(kernel) => Ok(kernel),
            Self::Minimal(kernel) => kernel
                .resolved
                .get_or_init(|| {
                    let actual = super::super::shared_embedded_intl_kernel()
                        .map_err(|error| error.to_string())?;
                    if actual.identity() != kernel.data.identity() {
                        return Err(
                            "deferred Intl provider identity differs from admitted compiler data"
                                .into(),
                        );
                    }
                    Ok(actual)
                })
                .as_ref()
                .map(Arc::as_ref)
                .map_err(|error| EngineError::new(error.clone())),
        }
    }
}

pub(crate) fn for_standalone(bytes: &[u8]) -> Result<WasmIntlKernel, EngineError> {
    let mut has_intl = false;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|_| invalid_image())? {
            Payload::CustomSection(section) => {
                has_intl |= section.name().starts_with("scriptw.intl")
            }
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.map_err(|_| invalid_image())?;
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
    if !has_intl {
        return Ok(WasmIntlKernel::Absent);
    }
    let selection = super::super::embedded_runtime::inputs(
        None,
        scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable,
    )
    .intl_selection(&scriptw_intl::IntlCompilationProfile::Minimal)
    .map_err(|error| EngineError::new(error.to_string()))?;
    let data = selection
        .compilation_data()
        .map_err(|error| EngineError::new(error.to_string()))?;
    let mut expected: std::collections::BTreeMap<_, _> =
        data.component_sections().into_iter().collect();
    let mut has_component = false;
    let mut imports_intl = false;
    let mut matches = true;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|_| invalid_image())? {
            Payload::CustomSection(section) if section.name().starts_with("scriptw.intl") => {
                if section.name() != scriptw_intl::INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION {
                    has_component = true;
                    matches &= expected
                        .remove(section.name())
                        .is_some_and(|frame| frame.as_ref() == section.data());
                }
            }
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.map_err(|_| invalid_image())?;
                    imports_intl |= import.module == WASM_HOST_IMPORT_NAMESPACE
                        && matches!(
                            import.name,
                            WASM_HOST_IMPORT_INTL_CALL | WASM_HOST_IMPORT_SYSTEM_TIME_ZONE
                        );
                }
            }
            _ => {}
        }
    }
    if matches && ((!has_component && !imports_intl) || expected.is_empty()) {
        super::super::validate_wasm_intl_artifact_identity(
            bytes,
            data.identity(),
            if has_component {
                super::super::IntlArtifactIdentityRequirement::AdmittedComponents
            } else {
                super::super::IntlArtifactIdentityRequirement::HostImports
            },
        )?;
        return WasmIntlKernel::minimal(Arc::clone(data)).ok_or_else(invalid_image);
    }
    Ok(super::kernel_for_artifact(bytes)?.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostless_module_has_no_provider_or_fabricated_identity() {
        let kernel = for_standalone(b"\0asm\x01\0\0\0").unwrap();
        assert!(matches!(kernel, WasmIntlKernel::Absent));
        assert!(kernel.identity().is_none());
        assert!(kernel.resolve().is_err());
    }

    #[test]
    fn actual_locale_number_and_plural_calls_leave_the_complete_provider_unresolved() {
        use scriptw_intl::number_format::options::LocaleMatcher;
        let selection = crate::embedded_runtime::inputs(
            None,
            scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable,
        )
        .intl_selection(&scriptw_intl::IntlCompilationProfile::Minimal)
        .unwrap();
        let data = Arc::clone(selection.compilation_data().unwrap());
        let kernel = WasmIntlKernel::minimal(Arc::clone(&data)).unwrap();
        let WasmIntlKernel::Minimal(owner) = &kernel else {
            unreachable!()
        };
        let call = |operation: scriptw_intl::IntlHostOp, payload: &[u8]| {
            let mut bytes = operation.wire().to_le_bytes().to_vec();
            bytes.extend_from_slice(payload);
            super::super::super::wasm_gc_intl_host::execute_deferred(
                &kernel,
                scriptw_intl::IntlGcHostRequest::decode(&bytes).unwrap(),
            )
            .unwrap()
            .unwrap()
        };
        assert_eq!(
            call(scriptw_intl::IntlHostOp::CanonicalizeLocale, b"EN-us").as_slice(),
            b"en-US"
        );
        assert!(owner.resolved.get().is_none());
        let locale = scriptw_intl::CanonicalLocaleId::from_data("en-US").unwrap();
        let number = scriptw_intl::number_format::NumberLocaleRequest {
            requested: vec![locale.clone()].into_boxed_slice(),
            matcher: LocaleMatcher::Lookup,
            numbering_system: None,
        }
        .encode()
        .unwrap();
        assert!(!call(scriptw_intl::IntlHostOp::ResolveNumberLocale, &number).is_empty());
        let plural = scriptw_intl::PluralLocaleRequest {
            requested: vec![locale].into_boxed_slice(),
            matcher: LocaleMatcher::Lookup,
        }
        .encode()
        .unwrap();
        assert!(!call(scriptw_intl::IntlHostOp::ResolvePluralLocale, &plural).is_empty());
        assert_eq!(
            data.minimal_number_kernel().unwrap().identity(),
            data.identity()
        );
        assert!(owner.resolved.get().is_none(), "first canonicalization/Number/Plural must not load DateTime, Segmenter or other complete consumers");
    }

    #[test]
    fn metadata_and_fixed_zones_stay_deferred_until_shared_first_use() {
        let selection = crate::embedded_runtime::inputs(
            None,
            scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable,
        )
        .intl_selection(&scriptw_intl::IntlCompilationProfile::Minimal)
        .unwrap();
        let data = Arc::clone(selection.compilation_data().unwrap());
        let kernel = WasmIntlKernel::minimal(Arc::clone(&data)).unwrap();
        let WasmIntlKernel::Minimal(owner) = &kernel else {
            unreachable!()
        };
        assert!(owner.resolved.get().is_none());
        for zone in [
            scriptw_intl::ConfiguredSystemTimeZone::utc(),
            scriptw_intl::ConfiguredSystemTimeZone::resolve("+05:30").unwrap(),
        ] {
            assert!(zone.is_compatible_with(kernel.identity().unwrap()));
        }
        assert_eq!(kernel.identity(), Some(data.identity()));
        assert!(owner.resolved.get().is_none());
        std::thread::scope(|scope| {
            let workers = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        let actual = kernel.resolve().unwrap();
                        assert_eq!(actual.identity(), data.identity());
                        actual as *const _ as usize
                    })
                })
                .collect::<Vec<_>>();
            let pointers = workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .collect::<Vec<_>>();
            assert!(pointers.windows(2).all(|pair| pair[0] == pair[1]));
        });
        assert!(owner.resolved.get().unwrap().is_ok());
    }
}
