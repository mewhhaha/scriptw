//! Typed consumers for the selected complete Minimal Locale and Number frames.
//! Their identity remains the admitted complete selection. Other formatter
//! providers are neither decoded nor advertised by these concrete Rust types.
use crate::provider::{LikelySubtags, LocaleCanonicalizationData};
use crate::{
    CanonicalizeLocale, IntlCompilationBytes, IntlCompilationData, IntlDataComponent,
    IntlDataIdentity, IntlDataImageError, IntlDataProfile, IntlKernel, IntlOperationProvider,
    IntlProvider, IntlServiceSet, LocaleDataImage, LocaleTransformError, LocaleTransformRequest,
    LocaleTransformResult, MaximizeLocale, MinimizeLocale,
};
use std::sync::{Arc, OnceLock};
mod number;
pub use number::{MinimalNumberProvider, NumberProfilesProvider};

#[derive(Debug, Default)]
pub(crate) struct MinimalConsumers {
    locale: OnceLock<Result<Arc<IntlKernel<MinimalLocaleProvider>>, IntlDataImageError>>,
    number: OnceLock<Result<Arc<IntlKernel<MinimalNumberProvider>>, IntlDataImageError>>,
}

/// Only Locale transform operations are implemented by this admitted owner.
pub struct MinimalLocaleProvider {
    identity: IntlDataIdentity,
    image: LocaleDataImage,
    locale: LocaleCanonicalizationData,
}

impl core::fmt::Debug for MinimalLocaleProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MinimalLocaleProvider")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}
impl IntlCompilationData {
    pub(crate) fn minimal_frame(
        &self,
        component: IntlDataComponent,
    ) -> Result<IntlCompilationBytes, IntlDataImageError> {
        if self.identity().profile().profile() != &IntlDataProfile::Minimal
            || self.identity().profile().services() != IntlServiceSet::ALL
            || self.service_selection().is_some()
        {
            return Err(IntlDataImageError::consumer(
                "typed Minimal consumers require the admitted complete Minimal selection",
            ));
        }
        let bytes = self
            .component_sections()
            .into_iter()
            .find_map(|(name, bytes)| (name == component.section_name()).then_some(bytes))
            .ok_or_else(|| IntlDataImageError::consumer("selected Minimal component is absent"))?;
        Ok(bytes)
    }

    pub fn minimal_locale_kernel(
        &self,
    ) -> Result<Arc<IntlKernel<MinimalLocaleProvider>>, IntlDataImageError> {
        self.minimal_consumers
            .locale
            .get_or_init(|| {
                let image = crate::locale_image::selected_minimal_locale_data_image(self)?;
                if image.profile() != self.identity().profile().profile() {
                    return Err(IntlDataImageError::consumer(
                        "selected Locale consumer profile differs",
                    ));
                }
                let locale = LocaleCanonicalizationData::from_image(&image)
                    .map_err(IntlDataImageError::consumer)?;
                let provider = MinimalLocaleProvider {
                    identity: self.identity().clone(),
                    image,
                    locale,
                };
                IntlKernel::new(self.identity().clone(), provider)
                    .map(Arc::new)
                    .map_err(IntlDataImageError::consumer)
            })
            .clone()
    }

    pub fn minimal_number_kernel(
        &self,
    ) -> Result<Arc<IntlKernel<MinimalNumberProvider>>, IntlDataImageError> {
        self.minimal_consumers
            .number
            .get_or_init(|| {
                let locale = self.minimal_locale_kernel()?;
                let provider = MinimalNumberProvider::new(self, locale)?;
                IntlKernel::new(self.identity().clone(), provider)
                    .map(Arc::new)
                    .map_err(IntlDataImageError::consumer)
            })
            .clone()
    }
}

impl IntlProvider for MinimalLocaleProvider {
    fn identity(&self) -> &IntlDataIdentity {
        &self.identity
    }
}
impl IntlOperationProvider<CanonicalizeLocale> for MinimalLocaleProvider {
    fn execute(
        &self,
        request: LocaleTransformRequest,
    ) -> Result<LocaleTransformResult, LocaleTransformError> {
        self.locale
            .canonicalize(&request.into_locale())
            .map(LocaleTransformResult::new)
    }
}
impl IntlOperationProvider<MaximizeLocale> for MinimalLocaleProvider {
    fn execute(
        &self,
        request: LocaleTransformRequest,
    ) -> Result<LocaleTransformResult, LocaleTransformError> {
        self.locale
            .apply_likely_subtags(&request.into_locale(), LikelySubtags::Maximize)
            .map(LocaleTransformResult::new)
    }
}
impl IntlOperationProvider<MinimizeLocale> for MinimalLocaleProvider {
    fn execute(
        &self,
        request: LocaleTransformRequest,
    ) -> Result<LocaleTransformResult, LocaleTransformError> {
        self.locale
            .apply_likely_subtags(&request.into_locale(), LikelySubtags::Minimize)
            .map(LocaleTransformResult::new)
    }
}

#[cfg(test)]
mod tests;
