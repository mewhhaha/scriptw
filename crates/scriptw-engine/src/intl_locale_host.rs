use super::*;
use scriptw_intl::{IntlOperation, IntlOperationProvider, LocaleTransformResult};

pub(super) fn wasm_intl_locale_call<O>(
    kernel: &IntlKernel<impl IntlOperationProvider<O>>,
    payload: &[u8],
) -> wasmtime::Result<Option<Vec<u8>>>
where
    O: IntlOperation<
        Request = LocaleTransformRequest,
        Response = LocaleTransformResult,
        Error = LocaleTransformError,
    >,
{
    let request_text = core::str::from_utf8(payload).map_err(|error| {
        wasmtime::Error::msg(format!("Intl locale request is not UTF-8: {error}"))
    })?;
    let Ok(locale) = LocaleId::parse(request_text) else {
        return Ok(None);
    };
    let handle = kernel.operation::<O>().map_err(|error| {
        wasmtime::Error::msg(format!("Intl locale kernel capability mismatch: {error}"))
    })?;
    let result = match handle.execute(LocaleTransformRequest::new(locale)) {
        Ok(result) => result,
        Err(LocaleTransformError::Unsupported(_)) => {
            return Ok(None);
        }
        Err(LocaleTransformError::InvalidProviderResult(error)) => {
            return Err(wasmtime::Error::msg(format!(
                "Intl locale provider returned invalid canonical data: {error}"
            )));
        }
    };
    Ok(Some(result.locale().as_str().as_bytes().to_vec()))
}

/// Return pinned week data after the compiler has checked the receiver brand.
pub(super) fn wasm_intl_locale_week_info_call(
    kernel: &IntlKernel<EmbeddedIntlProvider>,
    payload: &[u8],
) -> wasmtime::Result<Option<Vec<u8>>> {
    let text = core::str::from_utf8(payload).map_err(|error| {
        wasmtime::Error::msg(format!("Intl week-info request is not UTF-8: {error}"))
    })?;
    let locale = scriptw_intl::CanonicalLocaleId::from_data(text.to_owned()).map_err(|error| {
        wasmtime::Error::msg(format!(
            "Intl week-info request lacks a canonical locale: {error}"
        ))
    })?;
    let operation = kernel
        .operation::<scriptw_intl::LocaleWeekInfoOperation>()
        .map_err(|error| {
            wasmtime::Error::msg(format!("Intl week-info capability mismatch: {error}"))
        })?;
    let info = operation
        .execute(scriptw_intl::LocaleWeekRequest::new(locale))
        .map_err(|error| {
            wasmtime::Error::msg(format!("Intl week-info provider failed: {error}"))
        })?;
    let wire = scriptw_intl::LocaleWeekWire::from_info(&info).encode();
    Ok(Some(wire.to_vec()))
}

/// Return pinned text direction after the compiler has checked the receiver brand.
pub(super) fn wasm_intl_locale_text_info_call(
    kernel: &IntlKernel<EmbeddedIntlProvider>,
    payload: &[u8],
) -> wasmtime::Result<Option<Vec<u8>>> {
    let text = core::str::from_utf8(payload).map_err(|error| {
        wasmtime::Error::msg(format!("Intl text-info request is not UTF-8: {error}"))
    })?;
    let locale = scriptw_intl::CanonicalLocaleId::from_data(text.to_owned()).map_err(|error| {
        wasmtime::Error::msg(format!(
            "Intl text-info request lacks a canonical locale: {error}"
        ))
    })?;
    let operation = kernel
        .operation::<scriptw_intl::LocaleTextInfoOperation>()
        .map_err(|error| {
            wasmtime::Error::msg(format!("Intl text-info capability mismatch: {error}"))
        })?;
    let info = operation
        .execute(scriptw_intl::LocaleTextInfoRequest::new(locale))
        .map_err(|error| {
            wasmtime::Error::msg(format!("Intl text-info provider failed: {error}"))
        })?;
    let wire = scriptw_intl::LocaleTextWire::from_info(info).encode();
    Ok(Some(wire.to_vec()))
}

/// Return pinned hour-cycle preferences after the compiler has checked the receiver brand.
pub(super) fn wasm_intl_locale_hour_cycles_call(
    kernel: &IntlKernel<EmbeddedIntlProvider>,
    payload: &[u8],
) -> wasmtime::Result<Option<Vec<u8>>> {
    let text = core::str::from_utf8(payload).map_err(|error| {
        wasmtime::Error::msg(format!("Intl hour-cycles request is not UTF-8: {error}"))
    })?;
    let locale = scriptw_intl::CanonicalLocaleId::from_data(text.to_owned()).map_err(|error| {
        wasmtime::Error::msg(format!(
            "Intl hour-cycles request lacks a canonical locale: {error}"
        ))
    })?;
    let operation = kernel
        .operation::<scriptw_intl::LocaleHourCyclesOperation>()
        .map_err(|error| {
            wasmtime::Error::msg(format!("Intl hour-cycles capability mismatch: {error}"))
        })?;
    let info = operation
        .execute(
            scriptw_intl::LocaleHourCyclesRequest::new(locale).map_err(|error| {
                wasmtime::Error::msg(format!(
                    "Intl default hour-cycle request contains an explicit slot: {error}"
                ))
            })?,
        )
        .map_err(|error| {
            wasmtime::Error::msg(format!("Intl hour-cycles provider failed: {error}"))
        })?;
    let wire = scriptw_intl::LocaleHourCyclesWire::from_info(&info).encode();
    Ok(Some(wire.to_vec()))
}

/// Return a profile default only after the compiler excludes an explicit nu slot.
pub(super) fn wasm_intl_locale_numbering_systems_call(
    kernel: &IntlKernel<EmbeddedIntlProvider>,
    payload: &[u8],
) -> wasmtime::Result<Option<Vec<u8>>> {
    let text = core::str::from_utf8(payload).map_err(|error| {
        wasmtime::Error::msg(format!(
            "Intl numbering-systems request is not UTF-8: {error}"
        ))
    })?;
    let locale = scriptw_intl::CanonicalLocaleId::from_data(text.to_owned()).map_err(|error| {
        wasmtime::Error::msg(format!(
            "Intl numbering-systems request lacks a canonical locale: {error}"
        ))
    })?;
    let request = scriptw_intl::LocaleNumberingSystemsRequest::new(locale).map_err(|error| {
        wasmtime::Error::msg(format!(
            "Intl default numbering-system request contains an explicit slot: {error}"
        ))
    })?;
    let operation = kernel
        .operation::<scriptw_intl::LocaleNumberingSystemsOperation>()
        .map_err(|error| {
            wasmtime::Error::msg(format!(
                "Intl numbering-systems capability mismatch: {error}"
            ))
        })?;
    let info = operation.execute(request).map_err(|error| {
        wasmtime::Error::msg(format!("Intl numbering-systems provider failed: {error}"))
    })?;
    Ok(Some(info.name().as_bytes().to_vec()))
}

/// A closed framed request and typed native list; JS values stay in compiled Wasm.
pub(super) fn wasm_intl_locale_calendars_call(
    kernel: &IntlKernel<EmbeddedIntlProvider>,
    payload: &[u8],
) -> wasmtime::Result<Option<Vec<u8>>> {
    let locale = scriptw_intl::decode_locale_calendars_request(payload)
        .map_err(|error| wasmtime::Error::msg(format!("Intl calendars request: {error}")))?;
    let request = scriptw_intl::LocaleCalendarsRequest::new(locale)
        .map_err(|error| wasmtime::Error::msg(format!("Intl calendars request domain: {error}")))?;
    let operation = kernel
        .operation::<scriptw_intl::LocaleCalendarsOperation>()
        .map_err(|error| wasmtime::Error::msg(format!("Intl calendars capability: {error}")))?;
    let info = operation
        .execute(request)
        .map_err(|error| wasmtime::Error::msg(format!("Intl calendars provider: {error}")))?;
    let wire = scriptw_intl::LocaleInformationResponse::Calendars(&info)
        .encode()
        .map_err(|error| wasmtime::Error::msg(format!("Intl calendars response: {error}")))?;
    Ok(Some(wire))
}

/// A closed framed request and typed native list; JS values stay in compiled Wasm.
pub(super) fn wasm_intl_locale_collations_call(
    kernel: &IntlKernel<EmbeddedIntlProvider>,
    payload: &[u8],
) -> wasmtime::Result<Option<Vec<u8>>> {
    let locale = scriptw_intl::decode_locale_collations_request(payload)
        .map_err(|error| wasmtime::Error::msg(format!("Intl collations request: {error}")))?;
    let request = scriptw_intl::LocaleCollationsRequest::new(locale).map_err(|error| {
        wasmtime::Error::msg(format!("Intl collations request domain: {error}"))
    })?;
    let operation = kernel
        .operation::<scriptw_intl::LocaleCollationsOperation>()
        .map_err(|error| wasmtime::Error::msg(format!("Intl collations capability: {error}")))?;
    let info = operation
        .execute(request)
        .map_err(|error| wasmtime::Error::msg(format!("Intl collations provider: {error}")))?;
    let wire = scriptw_intl::LocaleInformationResponse::Collations(&info)
        .encode()
        .map_err(|error| wasmtime::Error::msg(format!("Intl collations response: {error}")))?;
    Ok(Some(wire))
}

/// A closed framed request and typed native list; JS values stay in compiled Wasm.
pub(super) fn wasm_intl_locale_time_zones_call(
    kernel: &IntlKernel<EmbeddedIntlProvider>,
    payload: &[u8],
) -> wasmtime::Result<Option<Vec<u8>>> {
    let locale = scriptw_intl::decode_locale_time_zones_request(payload)
        .map_err(|error| wasmtime::Error::msg(format!("Intl time_zones request: {error}")))?;
    let request = scriptw_intl::LocaleTimeZonesRequest::new(locale).map_err(|error| {
        wasmtime::Error::msg(format!("Intl time_zones request domain: {error}"))
    })?;
    let operation = kernel
        .operation::<scriptw_intl::LocaleTimeZonesOperation>()
        .map_err(|error| wasmtime::Error::msg(format!("Intl time_zones capability: {error}")))?;
    let info = operation
        .execute(request)
        .map_err(|error| wasmtime::Error::msg(format!("Intl time_zones provider: {error}")))?;
    let wire = scriptw_intl::LocaleInformationResponse::TimeZones(&info)
        .encode()
        .map_err(|error| wasmtime::Error::msg(format!("Intl time_zones response: {error}")))?;
    Ok(Some(wire))
}
