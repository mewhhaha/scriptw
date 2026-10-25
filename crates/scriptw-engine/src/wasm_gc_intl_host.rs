//! Private Intl bytes cross Stores without JavaScript values or linear pointers.

use super::*;
use scriptw_aot_wasm::{GcHostImport, GcHostLayout};
use wasmtime::{ArrayRef, ArrayRefPre, ArrayType, ExternType, FuncType, Mutability, Val};

/// Preserve the module's canonical array type, including its recursive group.
/// Manufacturing an independent array type can produce an incompatible result.
struct IntlByteArraySignature {
    function: FuncType,
    array: ArrayType,
}

impl IntlByteArraySignature {
    fn validate(function: FuncType) -> wasmtime::Result<Self> {
        let params: Vec<_> = function.params().collect();
        let results: Vec<_> = function.results().collect();
        let ([parameter], [result]) = (params.as_slice(), results.as_slice()) else {
            return Err(wasmtime::Error::msg(
                "Intl GC import requires one input and one output",
            ));
        };
        let parameter = parameter
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("Intl GC input must be a reference"))?;
        let result = result
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("Intl GC output must be a reference"))?;
        if parameter.is_nullable() || !result.is_nullable() {
            return Err(wasmtime::Error::msg(
                "Intl GC import has invalid nullability",
            ));
        }
        let array = parameter
            .heap_type()
            .as_concrete_array()
            .ok_or_else(|| wasmtime::Error::msg("Intl GC input must be a concrete byte array"))?;
        let output = result
            .heap_type()
            .as_concrete_array()
            .ok_or_else(|| wasmtime::Error::msg("Intl GC output must be a concrete byte array"))?;
        if !array.matches(output) || !output.matches(array) {
            return Err(wasmtime::Error::msg(
                "Intl GC input and output have different canonical types",
            ));
        }
        let layout = GcHostLayout::ByteArray;
        let storage = layout
            .array_element()
            .ok_or_else(|| wasmtime::Error::msg("Intl byte schema must be an array"))?;
        if layout.array_mutable() != Some(array.mutability() == Mutability::Var)
            || !wasm_gc_completion::storage_matches(storage, &array.element_type())
        {
            return Err(wasmtime::Error::msg(
                "Intl GC array disagrees with emitted byte schema",
            ));
        }
        let array = array.clone();
        Ok(Self { function, array })
    }
}

fn link_intl(
    linker: &mut WasmtimeLinker<WasmHostState>,
    module: &WasmtimeModule,
) -> Result<(), EngineError> {
    let row = GcHostImport::IntlProviderCall;
    let mut imports = module
        .imports()
        .filter(|import| import.module() == row.module() && import.name() == row.name());
    let Some(import) = imports.next() else {
        return Ok(());
    };
    if imports.next().is_some() {
        return Err(EngineError::new("duplicate Intl GC host import"));
    }
    let ExternType::Func(function) = import.ty() else {
        return Err(EngineError::new("Intl GC host import is not a function"));
    };
    let signature = IntlByteArraySignature::validate(function)
        .map_err(|error| EngineError::new(format!("invalid Intl GC host import: {error}")))?;
    linker
        .func_new(
            row.module(),
            row.name(),
            signature.function,
            move |mut caller: WasmtimeCaller<'_, WasmHostState>,
                  inputs: &[Val],
                  outputs: &mut [Val]| {
                let [Val::AnyRef(Some(reference))] = inputs else {
                    return Err(wasmtime::Error::msg(
                        "Intl GC request has no byte-array root",
                    ));
                };
                let request = reference
                    .as_array(&caller)?
                    .ok_or_else(|| wasmtime::Error::msg("Intl GC request is not an array"))?;
                let length = usize::try_from(request.len(&caller)?)
                    .map_err(|_| wasmtime::Error::msg("Intl request exceeds host address space"))?;
                let mut bytes = Vec::new();
                bytes.try_reserve_exact(length).map_err(|error| {
                    wasmtime::Error::msg(format!("Intl request allocation failed: {error}"))
                })?;
                bytes.resize(length, 0);
                request.copy_to_i8_slice(&mut caller, &mut bytes)?;
                let request = scriptw_intl::IntlGcHostRequest::decode(&bytes).map_err(|error| {
                    wasmtime::Error::msg(format!("invalid Intl GC request: {error}"))
                })?;
                let kernel = caller.data().intl_kernel.clone();
                let response = execute(
                    kernel
                        .resolve()
                        .map_err(|error| wasmtime::Error::msg(error.to_string()))?,
                    request,
                )?;
                let [output] = outputs else {
                    return Err(wasmtime::Error::msg(
                        "Intl GC callback has invalid output arity",
                    ));
                };
                *output = match response {
                    None => Val::AnyRef(None),
                    Some(bytes) => {
                        u32::try_from(bytes.len()).map_err(|_| {
                            wasmtime::Error::msg("Intl response exceeds Wasm32 byte extent")
                        })?;
                        let allocator = ArrayRefPre::new(&mut caller, signature.array.clone());
                        let response =
                            ArrayRef::new_from_i8_slice(&mut caller, &allocator, &bytes)?;
                        Val::AnyRef(Some(response.to_anyref()))
                    }
                };
                Ok(())
            },
        )
        .map_err(|error| EngineError::new(format!("Intl GC linker setup failed: {error}")))?;
    Ok(())
}

pub(super) fn execute(
    kernel: &IntlKernel<EmbeddedIntlProvider>,
    request: scriptw_intl::IntlGcHostRequest<'_>,
) -> wasmtime::Result<Option<Vec<u8>>> {
    let operation = request.operation();
    let payload = request.payload();
    match operation {
        IntlHostOp::CanonicalizeLocale => {
            intl_locale_host::wasm_intl_locale_call::<CanonicalizeLocale>(kernel, payload)
        }
        IntlHostOp::MaximizeLocale => {
            intl_locale_host::wasm_intl_locale_call::<MaximizeLocale>(kernel, payload)
        }
        IntlHostOp::MinimizeLocale => {
            intl_locale_host::wasm_intl_locale_call::<MinimizeLocale>(kernel, payload)
        }
        IntlHostOp::LocaleCalendarsOperation => {
            intl_locale_host::wasm_intl_locale_calendars_call(kernel, payload)
        }
        IntlHostOp::LocaleCollationsOperation => {
            intl_locale_host::wasm_intl_locale_collations_call(kernel, payload)
        }
        IntlHostOp::LocaleTimeZonesOperation => {
            intl_locale_host::wasm_intl_locale_time_zones_call(kernel, payload)
        }
        IntlHostOp::LocaleNumberingSystemsOperation => {
            intl_locale_host::wasm_intl_locale_numbering_systems_call(kernel, payload)
        }
        IntlHostOp::LocaleHourCyclesOperation => {
            intl_locale_host::wasm_intl_locale_hour_cycles_call(kernel, payload)
        }
        IntlHostOp::LocaleTextInfoOperation => {
            intl_locale_host::wasm_intl_locale_text_info_call(kernel, payload)
        }
        IntlHostOp::LocaleWeekInfoOperation => {
            intl_locale_host::wasm_intl_locale_week_info_call(kernel, payload)
        }
        IntlHostOp::LookupNamedTimeZone => {
            intl_time_zone_host::lookup_named_time_zone(kernel, payload)
        }
        IntlHostOp::NamedTimeZoneOffset => {
            intl_time_zone_host::named_time_zone_offset(kernel, payload)
        }
        IntlHostOp::PossibleNamedTimeZoneEpochs => {
            intl_time_zone_host::possible_named_time_zone_epochs(kernel, payload)
        }
        IntlHostOp::FindNamedTimeZoneTransition => {
            intl_time_zone_host::find_named_time_zone_transition(kernel, payload)
        }
        IntlHostOp::ResolveTimeZone => intl_time_zone_host::resolve_time_zone(kernel, payload),
        IntlHostOp::ResolveDateTimeLocale => {
            intl_datetime_host::call::<scriptw_intl::ResolveDateTimeLocale>(kernel, payload)
        }
        IntlHostOp::SupportedDateTimeLocales => {
            intl_datetime_host::call::<scriptw_intl::SupportedDateTimeLocales>(kernel, payload)
        }
        IntlHostOp::SelectDateTimeFormat => {
            intl_datetime_host::call::<scriptw_intl::SelectDateTimeFormat>(kernel, payload)
        }
        IntlHostOp::FormatDateTimeParts => {
            intl_datetime_host::call::<scriptw_intl::FormatDateTimeParts>(kernel, payload)
        }
        IntlHostOp::FormatDateTimeRangeParts => {
            intl_datetime_host::call::<scriptw_intl::FormatDateTimeRangeParts>(kernel, payload)
        }
        IntlHostOp::ResolvePluralLocale => {
            intl_plural_host::call::<scriptw_intl::ResolvePluralLocale>(kernel, payload)
        }
        IntlHostOp::SupportedPluralLocales => {
            intl_plural_host::call::<scriptw_intl::SupportedPluralLocales>(kernel, payload)
        }
        IntlHostOp::SelectPlural => {
            intl_plural_host::call::<scriptw_intl::SelectPlural>(kernel, payload)
        }
        IntlHostOp::SelectPluralRange => {
            intl_plural_host::call::<scriptw_intl::SelectPluralRange>(kernel, payload)
        }
        IntlHostOp::ResolveCollatorLocale => {
            intl_collator_host::call::<scriptw_intl::ResolveCollatorLocale>(kernel, payload)
        }
        IntlHostOp::SupportedCollatorLocales => {
            intl_collator_host::call::<scriptw_intl::SupportedCollatorLocales>(kernel, payload)
        }
        IntlHostOp::CompareCollator => {
            intl_collator_host::call::<scriptw_intl::CompareCollator>(kernel, payload)
        }
        IntlHostOp::ResolveListLocale => {
            intl_list_host::call::<scriptw_intl::ResolveListLocale>(kernel, payload)
        }
        IntlHostOp::SupportedListLocales => {
            intl_list_host::call::<scriptw_intl::SupportedListLocales>(kernel, payload)
        }
        IntlHostOp::FormatListParts => {
            intl_list_host::call::<scriptw_intl::FormatListParts>(kernel, payload)
        }
        IntlHostOp::ResolveDisplayNamesLocale => intl_display_names_host::call(
            kernel,
            payload,
            scriptw_intl::DisplayNamesWireOperation::ResolveLocale,
        ),
        IntlHostOp::SupportedDisplayNamesLocales => intl_display_names_host::call(
            kernel,
            payload,
            scriptw_intl::DisplayNamesWireOperation::SupportedLocales,
        ),
        IntlHostOp::DisplayName => intl_display_names_host::call(
            kernel,
            payload,
            scriptw_intl::DisplayNamesWireOperation::DisplayName,
        ),
        IntlHostOp::ResolveRelativeTimeLocale => intl_relative_time_host::call(
            kernel,
            payload,
            scriptw_intl::RelativeHostOp::ResolveRelativeTimeLocale,
        ),
        IntlHostOp::SupportedRelativeTimeLocales => intl_relative_time_host::call(
            kernel,
            payload,
            scriptw_intl::RelativeHostOp::SupportedRelativeTimeLocales,
        ),
        IntlHostOp::FormatRelativeTimeParts => intl_relative_time_host::call(
            kernel,
            payload,
            scriptw_intl::RelativeHostOp::FormatRelativeTimeParts,
        ),
        IntlHostOp::ResolveSegmenterLocale => intl_segmenter_host::call(
            kernel,
            payload,
            scriptw_intl::SegmenterWireOperation::ResolveLocale,
        ),
        IntlHostOp::SupportedSegmenterLocales => intl_segmenter_host::call(
            kernel,
            payload,
            scriptw_intl::SegmenterWireOperation::SupportedLocales,
        ),
        IntlHostOp::SegmentUtf16 => intl_segmenter_host::call(
            kernel,
            payload,
            scriptw_intl::SegmenterWireOperation::SegmentUtf16,
        ),
        IntlHostOp::ResolveDurationFormatLocale => {
            intl_duration_host::call(kernel, payload, scriptw_intl::DurationHostOp::Resolve)
        }
        IntlHostOp::SupportedDurationFormatLocales => intl_duration_host::call(
            kernel,
            payload,
            scriptw_intl::DurationHostOp::SupportedLocales,
        ),
        IntlHostOp::PartitionDurationFormat => {
            intl_duration_host::call(kernel, payload, scriptw_intl::DurationHostOp::Parts)
        }
        IntlHostOp::ResolveNumberLocale => {
            intl_number_host::call::<scriptw_intl::ResolveNumberLocale>(kernel, payload)
        }
        IntlHostOp::SupportedNumberLocales => {
            intl_number_host::call::<scriptw_intl::SupportedNumberLocales>(kernel, payload)
        }
        IntlHostOp::FormatNumberParts => {
            intl_number_host::call::<scriptw_intl::FormatNumberParts>(kernel, payload)
        }
        IntlHostOp::FormatNumberRangeParts => {
            intl_number_host::call::<scriptw_intl::FormatNumberRangeParts>(kernel, payload)
        }
    }
}

impl IntlByteArraySignature {
    fn validate_system_zone(function: FuncType) -> wasmtime::Result<Self> {
        let params: Vec<_> = function.params().collect();
        let results: Vec<_> = function.results().collect();
        let ([], [result]) = (params.as_slice(), results.as_slice()) else {
            return Err(wasmtime::Error::msg(
                "system-zone GC import requires no inputs and one output",
            ));
        };
        let reference = result
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("system-zone GC result must be a reference"))?;
        if reference.is_nullable() {
            return Err(wasmtime::Error::msg("system-zone GC result cannot be null"));
        }
        let array = reference.heap_type().as_concrete_array().ok_or_else(|| {
            wasmtime::Error::msg("system-zone GC result must be a concrete byte array")
        })?;
        let layout = GcHostLayout::ByteArray;
        let storage = layout
            .array_element()
            .ok_or_else(|| wasmtime::Error::msg("system-zone byte schema must be an array"))?;
        if layout.array_mutable() != Some(array.mutability() == Mutability::Var)
            || !wasm_gc_completion::storage_matches(storage, &array.element_type())
        {
            return Err(wasmtime::Error::msg(
                "system-zone GC array disagrees with emitted byte schema",
            ));
        }
        let array = array.clone();
        Ok(Self { function, array })
    }
}

pub(super) fn link(
    linker: &mut WasmtimeLinker<WasmHostState>,
    module: &WasmtimeModule,
) -> Result<(), EngineError> {
    link_intl(linker, module)?;
    link_system_zone(linker, module)
}

fn link_system_zone(
    linker: &mut WasmtimeLinker<WasmHostState>,
    module: &WasmtimeModule,
) -> Result<(), EngineError> {
    let row = GcHostImport::SystemTimeZoneSnapshot;
    let mut imports = module
        .imports()
        .filter(|import| import.module() == row.module() && import.name() == row.name());
    let Some(import) = imports.next() else {
        return Ok(());
    };
    if imports.next().is_some() {
        return Err(EngineError::new("duplicate system-zone GC import"));
    }
    let ExternType::Func(function) = import.ty() else {
        return Err(EngineError::new(
            "system-zone GC host import is not a function",
        ));
    };
    let signature = IntlByteArraySignature::validate_system_zone(function).map_err(|error| {
        EngineError::new(format!("invalid system-zone GC host import: {error}"))
    })?;
    linker
        .func_new(
            row.module(),
            row.name(),
            signature.function,
            move |mut caller: WasmtimeCaller<'_, WasmHostState>,
                  inputs: &[Val],
                  outputs: &mut [Val]| {
                if !inputs.is_empty() {
                    return Err(wasmtime::Error::msg(
                        "system-zone GC callback received arguments",
                    ));
                }
                let [output] = outputs else {
                    return Err(wasmtime::Error::msg(
                        "system-zone GC callback has invalid output arity",
                    ));
                };
                let bytes = caller.data().realm.system_time_zone().encode();
                let allocator = ArrayRefPre::new(&mut caller, signature.array.clone());
                let response = ArrayRef::new_from_i8_slice(&mut caller, &allocator, &bytes)?;
                *output = Val::AnyRef(Some(response.to_anyref()));
                Ok(())
            },
        )
        .map_err(|error| {
            EngineError::new(format!("system-zone GC linker setup failed: {error}"))
        })?;
    Ok(())
}
