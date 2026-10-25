//! NumberFormat/PluralRules consumers share the selected Locale foundation.
use super::MinimalLocaleProvider;
use crate::number_format::{
    filter_number_locales, resolve_number_locale, NumberLocaleRequest, NumberProfiles,
    NumberSupportedLocalesRequest, PartitionLimits, RangeNumberPartition, ResolvedNumberLocale,
    ScalarNumberPartition,
};
use crate::number_operation::{format_number_parts_operation, format_number_range_parts_operation};
use crate::plural_rules::{
    resolve_plural_locale, select_plural_operation, select_plural_range_operation,
    supported_plural_locales,
};
use crate::{
    EmbeddedIntlProvider, FormatNumberParts, FormatNumberRangeParts, IntlCompilationData,
    IntlDataIdentity, IntlDataImageError, IntlKernel, IntlOperationProvider, IntlProvider,
    IntlService, NumberFormatOperationError, NumberFormatRequest, NumberProfilesDataImage,
    NumberRangeFormatRequest, NumberSupportedLocalesResult, PluralCategory, PluralLocaleRequest,
    PluralRulesOperationError, PluralSupportedLocalesRequest, PluralSupportedLocalesResult,
    ResolveNumberLocale, ResolvePluralLocale, ResolvedPluralLocale, SelectPlural,
    SelectPluralRange, SelectPluralRangeRequest, SelectPluralRequest, SupportedNumberLocales,
    SupportedPluralLocales,
};
use std::sync::Arc;

/// NumberFormat and PluralRules share one actual Number frame and its Locale
/// foundation. A formatter needing another component cannot use this kernel.
///
/// ```compile_fail
/// use scriptw_intl::{FormatDateTimeParts, IntlKernel, MinimalNumberProvider};
/// fn wrong_component(kernel: &IntlKernel<MinimalNumberProvider>) {
///     kernel.operation::<FormatDateTimeParts>().unwrap();
/// }
/// ```
pub struct MinimalNumberProvider {
    identity: IntlDataIdentity,
    _locale: Arc<IntlKernel<MinimalLocaleProvider>>,
    image: NumberProfilesDataImage,
}

impl MinimalNumberProvider {
    pub(super) fn new(
        selection: &IntlCompilationData,
        locale: Arc<IntlKernel<MinimalLocaleProvider>>,
    ) -> Result<Self, IntlDataImageError> {
        if locale.identity() != selection.identity() {
            return Err(IntlDataImageError::consumer(
                "selected Number and Locale identities differ",
            ));
        }
        let image = crate::number_image::selected_minimal_number_data_image(
            selection,
            &locale.provider().image,
        )?;
        Ok(Self {
            identity: locale.identity().clone(),
            _locale: locale,
            image,
        })
    }

    #[cfg(test)]
    pub(super) fn locale_kernel(&self) -> &Arc<IntlKernel<MinimalLocaleProvider>> {
        &self._locale
    }
}

impl core::fmt::Debug for MinimalNumberProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MinimalNumberProvider")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl IntlProvider for MinimalNumberProvider {
    fn identity(&self) -> &IntlDataIdentity {
        &self.identity
    }
}
mod sealed {
    pub trait Sealed {}
}
/// Profile-aware Number/Plural wire decoding retains the exact consuming owner.
/// Implementations are limited to constructor-admitted providers in this crate.
pub trait NumberProfilesProvider: IntlProvider + sealed::Sealed {
    fn number_profiles(&self) -> Option<&Arc<NumberProfiles>>;
    fn permits_number_service(&self, service: IntlService) -> bool;
}
impl sealed::Sealed for EmbeddedIntlProvider {}
impl NumberProfilesProvider for EmbeddedIntlProvider {
    fn number_profiles(&self) -> Option<&Arc<NumberProfiles>> {
        EmbeddedIntlProvider::number_profiles(self)
    }
    fn permits_number_service(&self, service: IntlService) -> bool {
        self.permits_service(service)
    }
}
impl sealed::Sealed for MinimalNumberProvider {}
impl NumberProfilesProvider for MinimalNumberProvider {
    fn number_profiles(&self) -> Option<&Arc<NumberProfiles>> {
        Some(self.image.profiles_arc_ref())
    }
    fn permits_number_service(&self, service: IntlService) -> bool {
        matches!(
            service,
            IntlService::NumberFormat | IntlService::PluralRules
        )
    }
}

impl IntlOperationProvider<ResolveNumberLocale> for MinimalNumberProvider {
    fn execute(
        &self,
        request: NumberLocaleRequest,
    ) -> Result<ResolvedNumberLocale, NumberFormatOperationError> {
        Ok(resolve_number_locale(
            &request,
            self.image.profiles_arc_ref(),
        )?)
    }
}
impl IntlOperationProvider<SupportedNumberLocales> for MinimalNumberProvider {
    fn execute(
        &self,
        request: NumberSupportedLocalesRequest,
    ) -> Result<NumberSupportedLocalesResult, NumberFormatOperationError> {
        Ok(NumberSupportedLocalesResult {
            locales: filter_number_locales(&request, self.image.profiles_arc_ref())?,
        })
    }
}
impl IntlOperationProvider<FormatNumberParts> for MinimalNumberProvider {
    fn execute(
        &self,
        request: NumberFormatRequest,
    ) -> Result<ScalarNumberPartition, NumberFormatOperationError> {
        format_number_parts_operation(
            request,
            self.image.profiles_arc_ref(),
            &PartitionLimits::HOST_ABI,
        )
    }
}
impl IntlOperationProvider<FormatNumberRangeParts> for MinimalNumberProvider {
    fn execute(
        &self,
        request: NumberRangeFormatRequest,
    ) -> Result<RangeNumberPartition, NumberFormatOperationError> {
        format_number_range_parts_operation(
            request,
            self.image.profiles_arc_ref(),
            &PartitionLimits::HOST_ABI,
        )
    }
}
impl IntlOperationProvider<ResolvePluralLocale> for MinimalNumberProvider {
    fn execute(
        &self,
        request: PluralLocaleRequest,
    ) -> Result<ResolvedPluralLocale, PluralRulesOperationError> {
        resolve_plural_locale(
            &request,
            self.image.profiles_arc_ref(),
            &PartitionLimits::HOST_ABI,
        )
    }
}
impl IntlOperationProvider<SupportedPluralLocales> for MinimalNumberProvider {
    fn execute(
        &self,
        request: PluralSupportedLocalesRequest,
    ) -> Result<PluralSupportedLocalesResult, PluralRulesOperationError> {
        supported_plural_locales(request, self.image.profiles_arc_ref())
    }
}
impl IntlOperationProvider<SelectPlural> for MinimalNumberProvider {
    fn execute(
        &self,
        request: SelectPluralRequest,
    ) -> Result<PluralCategory, PluralRulesOperationError> {
        select_plural_operation(
            request,
            self.image.profiles_arc_ref(),
            &crate::number_format::numeric::NumericLimits::HOST_ABI,
        )
    }
}
impl IntlOperationProvider<SelectPluralRange> for MinimalNumberProvider {
    fn execute(
        &self,
        request: SelectPluralRangeRequest,
    ) -> Result<PluralCategory, PluralRulesOperationError> {
        select_plural_range_operation(
            request,
            self.image.profiles_arc_ref(),
            &crate::number_format::numeric::NumericLimits::HOST_ABI,
        )
    }
}
