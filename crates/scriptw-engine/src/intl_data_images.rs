//! Admit actual image consumers while an artifact is still inert bytes.

use super::{
    EngineError, IntlArtifactIdentityError, WASM_HOST_IMPORT_INTL_CALL, WASM_HOST_IMPORT_NAMESPACE,
    WASM_HOST_IMPORT_SYSTEM_TIME_ZONE,
};
use scriptw_intl::{
    embedded_collator_data_image, embedded_display_names_data_image, embedded_duration_data_image,
    embedded_list_data_image, embedded_locale_data_image, embedded_number_profiles_data_image,
    embedded_relative_time_data_image, embedded_segmenter_data_image, CollatorDataImage,
    DisplayNamesDataImage, DurationDataImage, EmbeddedIntlProvider, IntlKernel, IntlProvider,
    ListDataImage, LocaleDataImage, NumberProfilesDataImage, RelativeTimeDataImage,
    SegmenterDataImage, INTL_COLLATOR_DATA_CUSTOM_SECTION, INTL_DISPLAY_NAMES_DATA_CUSTOM_SECTION,
    INTL_DURATION_DATA_CUSTOM_SECTION, INTL_LIST_DATA_CUSTOM_SECTION,
    INTL_LOCALE_DATA_CUSTOM_SECTION, INTL_NUMBER_DATA_CUSTOM_SECTION,
    INTL_RELATIVE_TIME_DATA_CUSTOM_SECTION, INTL_SEGMENTER_DATA_CUSTOM_SECTION,
    MAX_INTL_COMPONENT_IMAGE_BYTES,
};
use scriptw_intl::{
    embedded_date_time_data_image, embedded_named_time_zone_data_image,
    embedded_native_locale_information_data_image, embedded_time_zone_names_data_image,
    DateTimeDataImage, NamedTimeZoneDataImage, NativeLocaleInformationDataImage,
    TimeZoneNamesDataImage, INTL_DATETIME_DATA_CUSTOM_SECTION,
    INTL_NAMED_TIME_ZONE_DATA_CUSTOM_SECTION, INTL_NATIVE_LOCALE_INFORMATION_CUSTOM_SECTION,
    INTL_TIME_ZONE_NAMES_DATA_CUSTOM_SECTION,
};
use scriptw_intl::{
    CheckedIntlServiceSelection, IntlDataComponent, SelectedIntlDataBundle,
    INTL_SERVICE_SELECTION_CUSTOM_SECTION,
};
use std::sync::Arc;
use wasmparser::{Parser, Payload};
mod deferred;
pub(super) use deferred::for_standalone as deferred_kernel_for_artifact;
pub(crate) use deferred::WasmIntlKernel;

#[derive(Default)]
struct ComponentSections<'a> {
    locale: Option<&'a [u8]>,
    lists: Option<&'a [u8]>,
    collators: Option<&'a [u8]>,
    numbers: Option<&'a [u8]>,
    segmenters: Option<&'a [u8]>,
    display_names: Option<&'a [u8]>,
    relative_times: Option<&'a [u8]>,
    durations: Option<&'a [u8]>,
    named_zones: Option<&'a [u8]>,
    date_time: Option<&'a [u8]>,
    time_zone_names: Option<&'a [u8]>,
    locale_information: Option<&'a [u8]>,
}
fn invalid_image() -> EngineError {
    EngineError::from_intl_artifact_identity(IntlArtifactIdentityError::InvalidDataImage)
}
fn retain_section<'a>(slot: &mut Option<&'a [u8]>, data: &'a [u8]) -> Result<(), EngineError> {
    if slot.replace(data).is_some() {
        return Err(invalid_image());
    }
    Ok(())
}

pub(super) fn kernel_for_artifact(
    bytes: &[u8],
) -> Result<Arc<IntlKernel<EmbeddedIntlProvider>>, EngineError> {
    let mut imports_intl = false;
    let mut sections = ComponentSections::default();
    let mut services = None;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|_| invalid_image())? {
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.map_err(|_| invalid_image())?;
                    if import.module == WASM_HOST_IMPORT_NAMESPACE
                        && matches!(
                            import.name,
                            WASM_HOST_IMPORT_INTL_CALL | WASM_HOST_IMPORT_SYSTEM_TIME_ZONE
                        )
                    {
                        imports_intl = true;
                    }
                }
            }
            Payload::CustomSection(section) => match section.name() {
                INTL_SERVICE_SELECTION_CUSTOM_SECTION => {
                    retain_section(&mut services, section.data())?
                }
                INTL_LOCALE_DATA_CUSTOM_SECTION => {
                    retain_section(&mut sections.locale, section.data())?
                }
                INTL_LIST_DATA_CUSTOM_SECTION => {
                    retain_section(&mut sections.lists, section.data())?
                }
                INTL_COLLATOR_DATA_CUSTOM_SECTION => {
                    retain_section(&mut sections.collators, section.data())?
                }
                INTL_NUMBER_DATA_CUSTOM_SECTION => {
                    retain_section(&mut sections.numbers, section.data())?
                }
                INTL_SEGMENTER_DATA_CUSTOM_SECTION => {
                    retain_section(&mut sections.segmenters, section.data())?
                }
                INTL_DISPLAY_NAMES_DATA_CUSTOM_SECTION => {
                    retain_section(&mut sections.display_names, section.data())?
                }
                INTL_RELATIVE_TIME_DATA_CUSTOM_SECTION => {
                    retain_section(&mut sections.relative_times, section.data())?
                }
                INTL_DURATION_DATA_CUSTOM_SECTION => {
                    retain_section(&mut sections.durations, section.data())?
                }
                INTL_NAMED_TIME_ZONE_DATA_CUSTOM_SECTION => {
                    retain_section(&mut sections.named_zones, section.data())?
                }
                INTL_DATETIME_DATA_CUSTOM_SECTION => {
                    retain_section(&mut sections.date_time, section.data())?
                }
                INTL_TIME_ZONE_NAMES_DATA_CUSTOM_SECTION => {
                    retain_section(&mut sections.time_zone_names, section.data())?
                }
                INTL_NATIVE_LOCALE_INFORMATION_CUSTOM_SECTION => {
                    retain_section(&mut sections.locale_information, section.data())?
                }
                _ => {}
            },
            _ => {}
        }
    }
    if let Some(bytes_of_selection) = services {
        let wire: [u8; 2] = bytes_of_selection.try_into().map_err(|_| invalid_image())?;
        let selection = CheckedIntlServiceSelection::from_wire(u16::from_le_bytes(wire))
            .map_err(|_| invalid_image())?;
        // Physical absence is meaningful. Pass only the actual frames through
        // the same exact dependency-closure constructor used by bundle import.
        let frames = [
            (IntlDataComponent::Locale, sections.locale),
            (IntlDataComponent::List, sections.lists),
            (IntlDataComponent::Collator, sections.collators),
            (IntlDataComponent::Number, sections.numbers),
            (IntlDataComponent::Segmenter, sections.segmenters),
            (IntlDataComponent::DisplayNames, sections.display_names),
            (IntlDataComponent::RelativeTime, sections.relative_times),
            (IntlDataComponent::Duration, sections.durations),
            (IntlDataComponent::NamedTimeZones, sections.named_zones),
            (IntlDataComponent::DateTime, sections.date_time),
            (IntlDataComponent::TimeZoneNames, sections.time_zone_names),
            (
                IntlDataComponent::LocaleInformation,
                sections.locale_information,
            ),
        ]
        .into_iter()
        .filter_map(|(component, data)| data.map(|data| (component, data)))
        .map(|(component, data)| {
            if data.len() > MAX_INTL_COMPONENT_IMAGE_BYTES {
                return Err(invalid_image());
            }
            Ok((component, Arc::<[u8]>::from(data)))
        })
        .collect::<Result<Vec<_>, EngineError>>()?;
        let provider = SelectedIntlDataBundle::from_component_sections(Some(selection), frames)
            .map_err(|_| invalid_image())?
            .into_provider();
        let kernel = Arc::new(
            IntlKernel::new(provider.identity().clone(), provider).map_err(|_| invalid_image())?,
        );
        super::validate_wasm_intl_artifact_identity(
            bytes,
            kernel.identity(),
            super::IntlArtifactIdentityRequirement::AdmittedComponents,
        )?;
        return Ok(kernel);
    }
    let ComponentSections {
        locale,
        lists,
        collators,
        numbers,
        segmenters,
        display_names,
        relative_times,
        durations,
        named_zones,
        date_time,
        time_zone_names,
        locale_information,
    } = sections;
    let has_components = locale.is_some()
        || lists.is_some()
        || collators.is_some()
        || numbers.is_some()
        || segmenters.is_some()
        || display_names.is_some()
        || relative_times.is_some()
        || durations.is_some()
        || named_zones.is_some()
        || date_time.is_some()
        || time_zone_names.is_some()
        || locale_information.is_some();
    if !imports_intl && !has_components {
        let kernel = super::shared_embedded_intl_kernel()?;
        super::validate_wasm_intl_artifact_identity(
            bytes,
            kernel.identity(),
            super::IntlArtifactIdentityRequirement::HostImports,
        )?;
        return Ok(kernel);
    }
    let (
        Some(locale),
        Some(lists),
        Some(collators),
        Some(numbers),
        Some(segmenters),
        Some(display_names),
        Some(relative_times),
        Some(durations),
        Some(named_zones),
        Some(date_time),
        Some(time_zone_names),
        Some(locale_information),
    ) = (
        locale,
        lists,
        collators,
        numbers,
        segmenters,
        display_names,
        relative_times,
        durations,
        named_zones,
        date_time,
        time_zone_names,
        locale_information,
    )
    else {
        return Err(invalid_image());
    };
    if [
        locale.len(),
        lists.len(),
        collators.len(),
        numbers.len(),
        segmenters.len(),
        display_names.len(),
        relative_times.len(),
        durations.len(),
        named_zones.len(),
        date_time.len(),
        time_zone_names.len(),
        locale_information.len(),
    ]
    .into_iter()
    .any(|length| length > MAX_INTL_COMPONENT_IMAGE_BYTES)
    {
        return Err(invalid_image());
    }
    let default_locale = embedded_locale_data_image().map_err(|_| invalid_image())?;
    let default_lists = embedded_list_data_image().map_err(|_| invalid_image())?;
    let default_collators = embedded_collator_data_image().map_err(|_| invalid_image())?;
    let default_numbers = embedded_number_profiles_data_image().map_err(|_| invalid_image())?;
    let default_segmenters = embedded_segmenter_data_image().map_err(|_| invalid_image())?;
    let default_display_names = embedded_display_names_data_image().map_err(|_| invalid_image())?;
    let default_relative_times =
        embedded_relative_time_data_image().map_err(|_| invalid_image())?;
    let default_durations = embedded_duration_data_image().map_err(|_| invalid_image())?;
    let default_named_zones = embedded_named_time_zone_data_image().map_err(|_| invalid_image())?;
    let default_date_time = embedded_date_time_data_image().map_err(|_| invalid_image())?;
    let default_time_zone_names =
        embedded_time_zone_names_data_image().map_err(|_| invalid_image())?;
    let default_locale_information =
        embedded_native_locale_information_data_image().map_err(|_| invalid_image())?;
    let kernel = if locale == default_locale.bytes().as_ref()
        && lists == default_lists.bytes().as_ref()
        && collators == default_collators.bytes().as_ref()
        && numbers == default_numbers.bytes().as_ref()
        && segmenters == default_segmenters.bytes().as_ref()
        && display_names == default_display_names.bytes().as_ref()
        && relative_times == default_relative_times.bytes().as_ref()
        && durations == default_durations.bytes().as_ref()
        && named_zones == default_named_zones.bytes().as_ref()
        && date_time == default_date_time.bytes().as_ref()
        && time_zone_names == default_time_zone_names.bytes().as_ref()
        && locale_information == default_locale_information.bytes().as_ref()
    {
        // Exact bytes reuse only the already-admitted provider for these images.
        super::shared_embedded_intl_kernel()?
    } else {
        let locale = LocaleDataImage::from_bytes(Arc::from(locale)).map_err(|_| invalid_image())?;
        let lists =
            ListDataImage::from_bytes(Arc::from(lists), &locale).map_err(|_| invalid_image())?;
        let collators = CollatorDataImage::from_bytes(Arc::from(collators), &locale)
            .map_err(|_| invalid_image())?;
        let numbers = NumberProfilesDataImage::from_bytes(Arc::from(numbers), &locale, &lists)
            .map_err(|_| invalid_image())?;
        let segmenters = SegmenterDataImage::from_bytes(Arc::from(segmenters), &locale)
            .map_err(|_| invalid_image())?;
        let display_names = DisplayNamesDataImage::from_bytes(Arc::from(display_names), &locale)
            .map_err(|_| invalid_image())?;
        let relative_times =
            RelativeTimeDataImage::from_bytes(Arc::from(relative_times), &locale, &numbers)
                .map_err(|_| invalid_image())?;
        let durations =
            DurationDataImage::from_bytes(Arc::from(durations), &locale, &numbers, &lists)
                .map_err(|_| invalid_image())?;
        let named_zones = NamedTimeZoneDataImage::from_bytes(Arc::from(named_zones))
            .map_err(|_| invalid_image())?;
        let date_time = DateTimeDataImage::from_bytes(Arc::from(date_time), &locale, &named_zones)
            .map_err(|_| invalid_image())?;
        let time_zone_names =
            TimeZoneNamesDataImage::from_bytes(Arc::from(time_zone_names), &named_zones)
                .map_err(|_| invalid_image())?;
        let locale_information = NativeLocaleInformationDataImage::from_bytes(
            Arc::from(locale_information),
            &locale,
            &date_time,
        )
        .map_err(|_| invalid_image())?;
        let provider = EmbeddedIntlProvider::with_data_images(
            locale,
            lists,
            collators,
            numbers,
            segmenters,
            display_names,
            relative_times,
            durations,
            named_zones,
            date_time,
            time_zone_names,
            locale_information,
        )
        .map_err(|_| invalid_image())?;
        let identity = provider.identity().clone();
        Arc::new(IntlKernel::new(identity, provider).map_err(|_| invalid_image())?)
    };
    // Complete identity, including every component digest, is checked before
    // native compilation, cache lookup or module start code can run.
    super::validate_wasm_intl_artifact_identity(
        bytes,
        kernel.identity(),
        super::IntlArtifactIdentityRequirement::AdmittedComponents,
    )?;
    Ok(kernel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptw_intl::{CustomProfileId, IntlDataProfile, INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION};

    fn u32_leb(mut value: u32, output: &mut Vec<u8>) {
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            output.push(byte);
            if value == 0 {
                return;
            }
        }
    }
    fn name(value: &str, output: &mut Vec<u8>) {
        u32_leb(u32::try_from(value.len()).unwrap(), output);
        output.extend_from_slice(value.as_bytes());
    }
    fn section(id: u8, data: &[u8], output: &mut Vec<u8>) {
        output.push(id);
        u32_leb(u32::try_from(data.len()).unwrap(), output);
        output.extend_from_slice(data);
    }
    fn artifact(import: bool, sections: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = b"\0asm\x01\0\0\0".to_vec();
        if import {
            let mut imports = vec![1];
            name(WASM_HOST_IMPORT_NAMESPACE, &mut imports);
            name(WASM_HOST_IMPORT_INTL_CALL, &mut imports);
            imports.extend_from_slice(&[0, 0]); // parsing remains inert, no module type validation
            section(2, &imports, &mut bytes);
        }
        for (key, data) in sections {
            let mut custom = Vec::new();
            name(key, &mut custom);
            custom.extend_from_slice(data);
            section(0, &custom, &mut bytes);
        }
        bytes
    }
    #[test]
    fn sparse_artifacts_bind_service_identity_and_require_the_exact_real_frames() {
        use scriptw_intl::{
            CustomIntlProfile, IntlCompilationProfile, IntlDataSelection, IntlService,
            IntlServiceSet,
        };
        let id = CustomProfileId::parse("sparse-artifact-control").unwrap();
        let owner = IntlDataSelection::new(IntlCompilationProfile::CustomProjection(
            CustomIntlProfile::for_services(id.clone(), &[IntlService::NumberFormat.name()])
                .unwrap(),
        ));
        let selected = owner.selected().unwrap();
        let frames = selected.component_sections();
        let wire = selected.service_selection().unwrap().wire().to_le_bytes();
        let identity = selected.identity().artifact_identity();
        let mut valid = vec![
            (INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION, identity.as_bytes()),
            (INTL_SERVICE_SELECTION_CUSTOM_SECTION, wire.as_slice()),
        ];
        valid.extend(frames.iter().map(|(name, data)| (*name, data.as_ref())));
        for imports in [false, true] {
            assert_eq!(
                kernel_for_artifact(&artifact(imports, &valid))
                    .unwrap()
                    .identity(),
                selected.identity()
            );
            for index in 1..valid.len() {
                let mut missing = valid.clone();
                missing.remove(index);
                let mut duplicate = valid.clone();
                duplicate.push(valid[index]);
                for invalid in [missing, duplicate] {
                    assert_eq!(
                        kernel_for_artifact(&artifact(imports, &invalid))
                            .unwrap_err()
                            .intl_artifact_identity_error(),
                        Some(IntlArtifactIdentityError::InvalidDataImage)
                    );
                }
            }
        }
        let full = IntlDataSelection::new(IntlCompilationProfile::Custom(id));
        let extra = full
            .selected()
            .unwrap()
            .component_sections()
            .into_iter()
            .find(|(name, _)| *name == INTL_COLLATOR_DATA_CUSTOM_SECTION)
            .unwrap();
        let mut extraneous = valid.clone();
        extraneous.push((extra.0, extra.1.as_ref()));
        assert_eq!(
            kernel_for_artifact(&artifact(true, &extraneous))
                .unwrap_err()
                .intl_artifact_identity_error(),
            Some(IntlArtifactIdentityError::InvalidDataImage)
        );
        for malformed in [
            &[][..],
            &[0][..],
            &[0, 0][..],
            &[255, 255][..],
            &[1, 0, 0][..],
        ] {
            let mut damaged = valid.clone();
            damaged[1].1 = malformed;
            assert_eq!(
                kernel_for_artifact(&artifact(true, &damaged))
                    .unwrap_err()
                    .intl_artifact_identity_error(),
                Some(IntlArtifactIdentityError::InvalidDataImage)
            );
        }
        // Equal physical dependencies cannot change public service authority.
        let other =
            CheckedIntlServiceSelection::new(IntlServiceSet::EMPTY.with(IntlService::PluralRules))
                .unwrap()
                .wire()
                .to_le_bytes();
        let mut rebound = valid;
        rebound[1].1 = &other;
        assert_eq!(
            kernel_for_artifact(&artifact(true, &rebound))
                .unwrap_err()
                .intl_artifact_identity_error(),
            Some(IntlArtifactIdentityError::IdentityMismatch)
        );
    }
    #[test]
    fn actual_component_gate_rejects_missing_duplicate_corrupt_and_unbound_images() {
        let locale = embedded_locale_data_image().unwrap().bytes();
        let lists = embedded_list_data_image().unwrap().bytes();
        let collators = embedded_collator_data_image().unwrap().bytes();
        let numbers = embedded_number_profiles_data_image().unwrap().bytes();
        let segmenters = embedded_segmenter_data_image().unwrap().bytes();
        let display_names = embedded_display_names_data_image().unwrap().bytes();
        let relative_times = embedded_relative_time_data_image().unwrap().bytes();
        let durations = embedded_duration_data_image().unwrap().bytes();
        let named_zones = embedded_named_time_zone_data_image().unwrap().bytes();
        let date_time = embedded_date_time_data_image().unwrap().bytes();
        let time_zone_names = embedded_time_zone_names_data_image().unwrap().bytes();
        let locale_information = embedded_native_locale_information_data_image()
            .unwrap()
            .bytes();
        let kernel = super::super::shared_embedded_intl_kernel().unwrap();
        let identity = kernel.identity().artifact_identity();
        let valid = [
            (INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION, identity.as_bytes()),
            (INTL_LOCALE_DATA_CUSTOM_SECTION, locale.as_ref()),
            (INTL_LIST_DATA_CUSTOM_SECTION, lists.as_ref()),
            (INTL_COLLATOR_DATA_CUSTOM_SECTION, collators.as_ref()),
            (INTL_NUMBER_DATA_CUSTOM_SECTION, numbers.as_ref()),
            (INTL_SEGMENTER_DATA_CUSTOM_SECTION, segmenters.as_ref()),
            (
                INTL_DISPLAY_NAMES_DATA_CUSTOM_SECTION,
                display_names.as_ref(),
            ),
            (
                INTL_RELATIVE_TIME_DATA_CUSTOM_SECTION,
                relative_times.as_ref(),
            ),
            (INTL_DURATION_DATA_CUSTOM_SECTION, durations.as_ref()),
            (
                INTL_NAMED_TIME_ZONE_DATA_CUSTOM_SECTION,
                named_zones.as_ref(),
            ),
            (INTL_DATETIME_DATA_CUSTOM_SECTION, date_time.as_ref()),
            (
                INTL_TIME_ZONE_NAMES_DATA_CUSTOM_SECTION,
                time_zone_names.as_ref(),
            ),
            (
                INTL_NATIVE_LOCALE_INFORMATION_CUSTOM_SECTION,
                locale_information.as_ref(),
            ),
        ];
        let admitted = kernel_for_artifact(&artifact(true, &valid)).unwrap();
        assert!(Arc::ptr_eq(&admitted, &kernel));
        let hostless = kernel_for_artifact(&artifact(false, &valid)).unwrap();
        assert!(Arc::ptr_eq(&hostless, &kernel));
        assert_eq!(
            kernel_for_artifact(&artifact(false, &valid[1..]))
                .unwrap_err()
                .intl_artifact_identity_error(),
            Some(IntlArtifactIdentityError::MissingSection),
        );
        // Every component is independently required and bound before code runs.
        for index in 1..valid.len() {
            let mut missing = valid.to_vec();
            missing.remove(index);
            let mut duplicate = valid.to_vec();
            duplicate.push(valid[index]);
            let mut corrupt = valid[index].1.to_vec();
            let last = corrupt.len() - 1;
            corrupt[last] ^= 1;
            let mut damaged = valid.to_vec();
            damaged[index] = (valid[index].0, &corrupt);
            for invalid in [
                artifact(true, &missing),
                artifact(true, &duplicate),
                artifact(true, &damaged),
                artifact(false, &missing),
                artifact(false, &duplicate),
                artifact(false, &damaged),
                artifact(false, &[valid[index]]),
            ] {
                let error = kernel_for_artifact(&invalid)
                    .expect_err("bad images must fail while bytes are inert");
                assert_eq!(
                    error.intl_artifact_identity_error(),
                    Some(IntlArtifactIdentityError::InvalidDataImage)
                );
            }
        }
        let mut wrong = valid;
        wrong[0] = (INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION, b"foreign identity");
        assert_eq!(
            kernel_for_artifact(&artifact(true, &wrong))
                .unwrap_err()
                .intl_artifact_identity_error(),
            Some(IntlArtifactIdentityError::IdentityMismatch)
        );
    }
    #[test]
    fn custom_named_pinned_components_select_an_actual_bound_kernel() {
        let profile =
            IntlDataProfile::Custom(CustomProfileId::parse("artifact-image-control").unwrap());
        let locale = LocaleDataImage::for_profile(profile.clone()).unwrap();
        let lists = ListDataImage::for_profile(profile.clone(), &locale).unwrap();
        let collators = CollatorDataImage::for_profile(profile.clone(), &locale).unwrap();
        let numbers = NumberProfilesDataImage::for_profile(profile.clone(), &locale).unwrap();
        let segmenters = SegmenterDataImage::for_profile(profile.clone(), &locale).unwrap();
        let display_names = DisplayNamesDataImage::for_profile(profile.clone(), &locale).unwrap();
        let relative_times =
            RelativeTimeDataImage::for_profile(profile.clone(), &locale, &numbers).unwrap();
        let durations =
            DurationDataImage::for_profile(profile.clone(), &locale, &numbers, &lists).unwrap();
        let named_zones = NamedTimeZoneDataImage::for_profile(profile.clone()).unwrap();
        let date_time =
            DateTimeDataImage::for_profile(profile.clone(), &locale, &named_zones).unwrap();
        let time_zone_names =
            TimeZoneNamesDataImage::for_profile(profile.clone(), &named_zones).unwrap();
        let locale_information =
            NativeLocaleInformationDataImage::for_profile(profile.clone(), &locale, &date_time)
                .unwrap();
        let provider = EmbeddedIntlProvider::with_data_images(
            locale.clone(),
            lists.clone(),
            collators.clone(),
            numbers.clone(),
            segmenters.clone(),
            display_names.clone(),
            relative_times.clone(),
            durations.clone(),
            named_zones.clone(),
            date_time.clone(),
            time_zone_names.clone(),
            locale_information.clone(),
        )
        .unwrap();
        let expected = provider.identity().clone();
        assert_eq!(expected.profile().profile(), &profile);
        let default = super::super::shared_embedded_intl_kernel().unwrap();
        assert_eq!(
            expected.profile().services(),
            default.identity().profile().services()
        );
        assert_eq!(
            expected.profile().capabilities(),
            default.identity().profile().capabilities()
        );
        assert_eq!(expected.versions(), default.identity().versions());
        let identity = expected.artifact_identity();
        let identity_text = std::str::from_utf8(identity.as_bytes()).unwrap();
        assert!(identity_text
            .lines()
            .any(|line| line == "profile=custom:artifact-image-control"));
        let bytes = artifact(
            true,
            &[
                (INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION, identity.as_bytes()),
                (INTL_LOCALE_DATA_CUSTOM_SECTION, locale.bytes().as_ref()),
                (INTL_LIST_DATA_CUSTOM_SECTION, lists.bytes().as_ref()),
                (
                    INTL_COLLATOR_DATA_CUSTOM_SECTION,
                    collators.bytes().as_ref(),
                ),
                (INTL_NUMBER_DATA_CUSTOM_SECTION, numbers.bytes().as_ref()),
                (
                    INTL_SEGMENTER_DATA_CUSTOM_SECTION,
                    segmenters.bytes().as_ref(),
                ),
                (
                    INTL_DISPLAY_NAMES_DATA_CUSTOM_SECTION,
                    display_names.bytes().as_ref(),
                ),
                (
                    INTL_RELATIVE_TIME_DATA_CUSTOM_SECTION,
                    relative_times.bytes().as_ref(),
                ),
                (
                    INTL_DURATION_DATA_CUSTOM_SECTION,
                    durations.bytes().as_ref(),
                ),
                (
                    INTL_NAMED_TIME_ZONE_DATA_CUSTOM_SECTION,
                    named_zones.bytes().as_ref(),
                ),
                (
                    INTL_DATETIME_DATA_CUSTOM_SECTION,
                    date_time.bytes().as_ref(),
                ),
                (
                    INTL_TIME_ZONE_NAMES_DATA_CUSTOM_SECTION,
                    time_zone_names.bytes().as_ref(),
                ),
                (
                    INTL_NATIVE_LOCALE_INFORMATION_CUSTOM_SECTION,
                    locale_information.bytes().as_ref(),
                ),
            ],
        );
        let selected = kernel_for_artifact(&bytes).unwrap();
        assert_eq!(selected.identity(), &expected);
        // Keep all admitted payloads, versions, capabilities and digests fixed;
        // changing only the profile field must reject this inert artifact.
        let mislabeled_identity = identity_text.replace(
            "profile=custom:artifact-image-control\n",
            "profile=minimal\n",
        );
        let mut mislabeled = b"\0asm\x01\0\0\0".to_vec();
        for payload in Parser::new(0).parse_all(&bytes) {
            let payload = payload.unwrap();
            if matches!(&payload, Payload::CustomSection(custom) if custom.name() == INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION)
            {
                let mut replacement = Vec::new();
                name(INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION, &mut replacement);
                replacement.extend_from_slice(mislabeled_identity.as_bytes());
                section(0, &replacement, &mut mislabeled);
            } else if let Some((id, range)) = payload.as_section() {
                section(id, &bytes[range], &mut mislabeled);
            }
        }
        assert_eq!(
            kernel_for_artifact(&mislabeled)
                .unwrap_err()
                .intl_artifact_identity_error(),
            Some(IntlArtifactIdentityError::IdentityMismatch),
        );
        assert_ne!(
            selected.identity().digest(),
            super::super::shared_embedded_intl_kernel()
                .unwrap()
                .identity()
                .digest()
        );
        let result = selected
            .operation::<scriptw_intl::CanonicalizeLocale>()
            .unwrap()
            .execute(scriptw_intl::LocaleTransformRequest::new(
                scriptw_intl::LocaleId::parse("iw-IL").unwrap(),
            ))
            .unwrap();
        assert_eq!(result.locale().as_str(), "he-IL");
    }
    #[test]
    fn actual_wasm_uses_custom_named_pinned_images_and_selected_host_decoders() {
        super::super::configure_compilation_jobs(1).unwrap();
        let engine = super::super::Engine::new(super::super::RealmBuilder::new().build());
        let profile = scriptw_intl::IntlCompilationProfile::Custom(
            CustomProfileId::parse("actual-artifact-image-control").unwrap(),
        );
        let unit = engine.compile_script(
            "Intl.getCanonicalLocales(['iw-IL'])[0] === 'he-IL' && new Intl.ListFormat('en').format(['A', 'B']) === 'A and B' && new Intl.Collator('en').compare('a', 'b') < 0 && new Intl.NumberFormat('en').format(1234) === '1,234' && new Intl.NumberFormat('en').formatRange(1, 2) === '1–2' && new Intl.PluralRules('en').select(1) === 'one' && new Intl.PluralRules('en').selectRange(1, 2) === 'other' && new Intl.RelativeTimeFormat('en', {numeric: 'auto'}).format(-1, 'day') === 'yesterday' && new Intl.RelativeTimeFormat('en').format(2, 'day') === 'in 2 days' && new Intl.DisplayNames('en', {type: 'region'}).of('US') === 'United States' && new Intl.DurationFormat('en', {style: 'long'}).format({years: 1, months: 2}) === '1 year, 2 months' && new Intl.Segmenter('en', {granularity: 'grapheme'}).segment('á').containing(1).segment === 'á' && Intl.getCanonicalLocales('en-u-ca-islamicc')[0] === 'en-u-ca-islamic-civil' && new Intl.DateTimeFormat('en-US', {timeZone: 'UTC', year: 'numeric'}).format(0) === '1970' && new Intl.DateTimeFormat('en-US', {timeZone: 'America/New_York', year: 'numeric'}).format(0) === '1969' && new Intl.Locale('en-US').getCalendars()[0] === 'gregory' && new Intl.Locale('en-US').getHourCycles()[0] === 'h12' && new Intl.Locale('en-US').getWeekInfo().firstDay === 7 && new Intl.Locale('en-US').getTextInfo().direction === 'ltr' && new Intl.Locale('en-US').getTimeZones().includes('America/New_York');",
            super::super::CompileOptions {
                intl_profile: profile.clone(),
                ..super::super::CompileOptions::default()
            },
        ).unwrap();
        let emitted = engine.emit_wasm(&unit).unwrap();
        let selection = scriptw_intl::IntlDataSelection::new(profile);
        let selected = selection.selected().unwrap();
        assert_eq!(
            kernel_for_artifact(
                emitted
                    .runtime
                    .as_ref()
                    .map_or(&emitted.bytes[..], |runtime| runtime.bytes())
            )
            .unwrap()
            .identity(),
            selected.identity()
        );
        let observed = super::super::run_on_sized_stack(|| {
            engine.execute_with_wasm_bytes_inner(
                emitted.program(),
                Some(30_000),
                true,
                super::super::WasmModuleMemoryCachePolicy::BypassRetention,
                &super::super::WasmExecutionMode::Structured,
            )
        })
        .unwrap()
        .into_structured()
        .unwrap();
        assert_eq!(
            observed.backend_used,
            super::super::ExecutionBackend::WasmAot
        );
        assert_eq!(
            observed.completion,
            super::super::ObservedCompletion::Normal(super::super::ObservedJsValue::Boolean(true))
        );
    }
}
