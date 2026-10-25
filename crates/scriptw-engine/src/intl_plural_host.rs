use super::*;
#[cfg(test)]
use scriptw_intl::number_format::embedded_number_profiles_arc;
use scriptw_intl::NumberProfilesProvider;
use scriptw_intl::{
    IntlOperation, IntlOperationProvider, PluralCategory, PluralLocaleRequest,
    PluralRulesOperationError, PluralSupportedLocalesRequest, PluralSupportedLocalesResult,
    PluralWireError, ResolvePluralLocale, ResolvedPluralLocale, SelectPlural, SelectPluralRange,
    SupportedPluralLocales,
};
#[cfg(test)]
use scriptw_intl::{SelectPluralRangeRequest, SelectPluralRequest};

pub(super) trait PluralHostOperation:
    IntlOperation<Error = PluralRulesOperationError>
{
    fn decode_request(
        bytes: &[u8],
        kernel: &IntlKernel<impl NumberProfilesProvider>,
    ) -> Result<Self::Request, PluralWireError>;
    fn encode_response(response: &Self::Response) -> Result<Vec<u8>, PluralWireError>;
}
macro_rules! locale_operation {
    ($operation:ty, $request:ty, $response:ty) => {
        impl PluralHostOperation for $operation {
            fn decode_request(
                bytes: &[u8],
                _kernel: &IntlKernel<impl NumberProfilesProvider>,
            ) -> Result<Self::Request, PluralWireError> {
                <$request>::decode(bytes)
            }
            fn encode_response(response: &Self::Response) -> Result<Vec<u8>, PluralWireError> {
                <$response>::encode(response)
            }
        }
    };
}
locale_operation!(
    ResolvePluralLocale,
    PluralLocaleRequest,
    ResolvedPluralLocale
);
locale_operation!(
    SupportedPluralLocales,
    PluralSupportedLocalesRequest,
    PluralSupportedLocalesResult
);
impl PluralHostOperation for SelectPlural {
    fn decode_request(
        bytes: &[u8],
        kernel: &IntlKernel<impl NumberProfilesProvider>,
    ) -> Result<Self::Request, PluralWireError> {
        kernel.decode_plural_select_request(bytes)
    }
    fn encode_response(response: &Self::Response) -> Result<Vec<u8>, PluralWireError> {
        response.encode_scalar()
    }
}
impl PluralHostOperation for SelectPluralRange {
    fn decode_request(
        bytes: &[u8],
        kernel: &IntlKernel<impl NumberProfilesProvider>,
    ) -> Result<Self::Request, PluralWireError> {
        kernel.decode_plural_range_request(bytes)
    }
    fn encode_response(response: &Self::Response) -> Result<Vec<u8>, PluralWireError> {
        response.encode_range()
    }
}

pub(super) fn call<O>(
    kernel: &IntlKernel<impl NumberProfilesProvider + IntlOperationProvider<O>>,
    payload: &[u8],
) -> wasmtime::Result<Option<Vec<u8>>>
where
    O: PluralHostOperation,
{
    let request = O::decode_request(payload, kernel).map_err(|error| {
        wasmtime::Error::msg(format!(
            "invalid Intl {} request: {error}",
            O::HOST_OP.name()
        ))
    })?;
    let operation = kernel.operation::<O>().map_err(|error| {
        wasmtime::Error::msg(format!(
            "Intl {} capability mismatch: {error}",
            O::HOST_OP.name()
        ))
    })?;
    let response = match operation.execute(request) {
        Ok(response) => response,
        Err(PluralRulesOperationError::NaNRangeEndpoint) => return Ok(None),
        Err(
            error @ (PluralRulesOperationError::InvalidResolvedLocale
            | PluralRulesOperationError::InvalidCategory
            | PluralRulesOperationError::Data(_)
            | PluralRulesOperationError::Numeric(_)
            | PluralRulesOperationError::Rounding(_)
            | PluralRulesOperationError::UnavailableService(_)),
        ) => {
            return Err(wasmtime::Error::msg(format!(
                "Intl {} failed: {error}",
                O::HOST_OP.name()
            )))
        }
    };
    let bytes = O::encode_response(&response).map_err(|error| {
        wasmtime::Error::msg(format!(
            "Intl {} response failed: {error}",
            O::HOST_OP.name()
        ))
    })?;
    Ok(Some(bytes))
}
#[cfg(test)]
mod tests;
