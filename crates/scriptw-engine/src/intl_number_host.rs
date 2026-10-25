use super::*;
use scriptw_intl::number_format::{
    NumberLocaleRequest, NumberSupportedLocalesRequest, RangeNumberPartition, ResolvedNumberLocale,
    ScalarNumberPartition,
};
use scriptw_intl::NumberProfilesProvider;
use scriptw_intl::NumberWireError;
use scriptw_intl::{
    FormatNumberParts, FormatNumberRangeParts, IntlOperation, IntlOperationProvider,
    ResolveNumberLocale, SupportedNumberLocales,
};
use scriptw_intl::{
    NumberFormatOperationError, NumberFormatRequest, NumberRangeFormatRequest,
    NumberSupportedLocalesResult,
};

/// One marker fixes the decoder, kernel request and response encoder together.
pub(super) trait NumberHostOperation:
    IntlOperation<Error = NumberFormatOperationError>
{
    fn decode_request(
        bytes: &[u8],
        kernel: &IntlKernel<impl NumberProfilesProvider>,
    ) -> Result<Self::Request, NumberWireError>;
    fn encode_response(response: &Self::Response) -> Result<Vec<u8>, NumberWireError>;
}

macro_rules! number_host_operation {
    ($operation:ty, $response:ty, $bytes:ident, $kernel:ident, $decode:expr) => {
        impl NumberHostOperation for $operation {
            fn decode_request(
                $bytes: &[u8],
                $kernel: &IntlKernel<impl NumberProfilesProvider>,
            ) -> Result<Self::Request, NumberWireError> {
                $decode
            }
            fn encode_response(response: &Self::Response) -> Result<Vec<u8>, NumberWireError> {
                <$response>::encode(response)
            }
        }
    };
}
number_host_operation!(
    ResolveNumberLocale,
    ResolvedNumberLocale,
    bytes,
    _kernel,
    NumberLocaleRequest::decode(bytes)
);
number_host_operation!(
    SupportedNumberLocales,
    NumberSupportedLocalesResult,
    bytes,
    _kernel,
    NumberSupportedLocalesRequest::decode(bytes)
);
number_host_operation!(
    FormatNumberParts,
    ScalarNumberPartition,
    bytes,
    kernel,
    kernel.decode_number_format_request(bytes)
);
number_host_operation!(
    FormatNumberRangeParts,
    RangeNumberPartition,
    bytes,
    kernel,
    kernel.decode_number_range_request(bytes)
);

pub(super) fn call<O>(
    kernel: &IntlKernel<impl NumberProfilesProvider + IntlOperationProvider<O>>,
    payload: &[u8],
) -> wasmtime::Result<Option<Vec<u8>>>
where
    O: NumberHostOperation,
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
        Err(NumberFormatOperationError::NaNRangeEndpoint) => {
            return Ok(None);
        }
        Err(
            error @ (NumberFormatOperationError::Kernel(_)
            | NumberFormatOperationError::Numeric(_)
            | NumberFormatOperationError::UnavailableService(_)),
        ) => {
            return Err(wasmtime::Error::msg(format!(
                "Intl {} failed: {error}",
                O::HOST_OP.name()
            )));
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

#[cfg(test)]
use scriptw_intl::number_format::embedded_number_profiles_arc;
