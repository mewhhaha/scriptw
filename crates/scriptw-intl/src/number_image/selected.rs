//! Reuse canonical Minimal Number consumers without caching rejected input.
use super::*;

pub(crate) fn selected_minimal_number_data_image(
    selection: &crate::IntlCompilationData,
    locale: &LocaleDataImage,
) -> Result<NumberProfilesDataImage, IntlDataImageError> {
    if locale.profile() != &IntlDataProfile::Minimal
        || locale.bytes().as_ref()
            != selection
                .minimal_frame(crate::IntlDataComponent::Locale)?
                .as_ref()
    {
        return Err(IntlDataImageError::consumer(
            "selected Number has a different Locale foundation",
        ));
    }
    selected_frame(
        &MINIMAL_IMAGE,
        selection.minimal_frame(crate::IntlDataComponent::Number)?,
        locale,
    )
}

fn selected_frame(
    cache: &OnceLock<Result<NumberProfilesDataImage, IntlDataImageError>>,
    bytes: crate::IntlCompilationBytes,
    locale: &LocaleDataImage,
) -> Result<NumberProfilesDataImage, IntlDataImageError> {
    let matches = |image: &NumberProfilesDataImage| {
        if image.profile() != &IntlDataProfile::Minimal
            || image.locale_digest() != locale.digest()
            || !image.uses_locale(locale)
            || image.bytes().as_ref() != bytes.as_ref()
        {
            return Err(IntlDataImageError::consumer(
                "selected Number differs from canonical Minimal data",
            ));
        }
        Ok(image.clone())
    };
    if let Some(image) = cache.get() {
        return matches(image.as_ref().map_err(Clone::clone)?);
    }
    let owned = match &bytes {
        crate::IntlCompilationBytes::Shared(bytes) => Arc::clone(bytes),
        crate::IntlCompilationBytes::Build(bytes) => Arc::from(*bytes),
    };
    let image = NumberProfilesDataImage::from_complete_frame(owned, locale)?;
    matches(&image)?;
    matches(
        cache
            .get_or_init(|| Ok(image))
            .as_ref()
            .map_err(Clone::clone)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_selected_number_admission_does_not_poison_canonical_publication() {
        let locale = crate::embedded_locale_data_image().unwrap();
        let cache = OnceLock::new();
        assert!(selected_frame(
            &cache,
            crate::IntlCompilationBytes::Build(b"invalid"),
            &locale
        )
        .is_err());
        assert!(cache.get().is_none());
        let custom_locale = LocaleDataImage::for_profile(IntlDataProfile::Custom(
            crate::CustomProfileId::parse("number-cache-negative").unwrap(),
        ))
        .unwrap();
        let custom =
            NumberProfilesDataImage::for_profile(custom_locale.profile().clone(), &custom_locale)
                .unwrap();
        assert!(selected_frame(
            &cache,
            crate::IntlCompilationBytes::Shared(custom.bytes()),
            &locale
        )
        .is_err());
        assert!(cache.get().is_none());
        let canonical = embedded_number_profiles_data_image().unwrap();
        let selected = selected_frame(
            &cache,
            crate::IntlCompilationBytes::Shared(canonical.bytes()),
            &locale,
        )
        .unwrap();
        assert_eq!(selected.bytes(), canonical.bytes());
        assert!(cache.get().unwrap().is_ok());
    }
}
