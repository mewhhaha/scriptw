use super::*;
use wasm_encoder::{
    ConstExpr, DataSection, ExportSection, GlobalType, ImportSection, MemorySection, Module,
};

use crate::emit::MainFunctionCompilation;
use crate::emitted_function::ModuleFunctionTable;
use crate::gc_types::RuntimeModuleTypes;
pub(crate) use crate::gc_types::{FinalizedModuleGlobals, GlobalLedger};

mod typed_array_element_kind;
pub(crate) use typed_array_element_kind::{TypedArrayContentType, TypedArrayElementKind};

mod static_signature;
pub(crate) use static_signature::{StaticSignature, StaticSignatureDefinition, COMPLETION_TYPES};

mod compiled_module_package;
pub(crate) use compiled_module_package::{
    empty_direct_eval_module, primitive_direct_eval_module, ModuleAssemblySections,
    ModuleGlobalSectionBuilder, ModuleTypeRegistry,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThrowDiagnosticRole {
    Name,
    Message,
    ConstructorName,
}
impl ThrowDiagnosticRole {
    pub(crate) const ALL: &'static [Self] = &[Self::Name, Self::Message, Self::ConstructorName];
    pub(crate) const fn export_name(self) -> &'static str {
        match self {
            Self::Name => THROW_ERROR_NAME_EXPORT,
            Self::Message => THROW_ERROR_MESSAGE_EXPORT,
            Self::ConstructorName => THROW_ERROR_CONSTRUCTOR_NAME_EXPORT,
        }
    }
}
pub(crate) const THROW_ERROR_NAME_EXPORT: &str = "throw_error_name";
pub(crate) const THROW_ERROR_CONSTRUCTOR_NAME_EXPORT: &str = "throw_error_constructor_name";
/// Companion export to `THROW_ERROR_NAME_EXPORT`. The host reads both at an
/// uncaught throw so a failure detail can name the defect
/// (`TypeError: RegExp.prototype.exec unsupported pattern`) instead of printing
/// a raw linear-memory address (`TypeError: object(handle@5397552)`), which is
/// neither stable across builds nor resolvable to an allocation site.
pub(crate) const THROW_ERROR_MESSAGE_EXPORT: &str = "throw_error_message";

pub(crate) const HOST_IMPORT_MODULE: &str = "scriptw_host";
pub(crate) const HOST_IMPORT_AGENT_CAN_SUSPEND: &str = "agent_can_suspend";
pub(crate) const HOST_IMPORT_PRINT_LINE_UTF8: &str = "print_line_utf8";
pub(crate) const HOST_IMPORT_NUMBER_POW: &str = "number_pow";
pub(crate) const HOST_IMPORT_PRIVATE_MEMORY: &str = "private_memory";
pub(crate) const HOST_IMPORT_SHARED_MEMORY: &str = "shared_memory";
pub(crate) const HOST_IMPORT_WALL_CLOCK_MILLIS: &str = "wall_clock_millis";
pub(crate) const HOST_IMPORT_MONOTONIC_CLOCK_NANOS: &str = "monotonic_clock_nanos";
pub(crate) const HOST_IMPORT_SLEEP_NANOS: &str = "sleep_nanos";
pub(crate) const HOST_IMPORT_AGENT_CALL: &str = "agent_call";
pub(crate) const HOST_IMPORT_RANDOM_F64: &str = "random_f64";
pub(crate) const HOST_IMPORT_MATH_ACOS: &str = "math_acos";
pub(crate) const HOST_IMPORT_MATH_ACOSH: &str = "math_acosh";
pub(crate) const HOST_IMPORT_MATH_ASIN: &str = "math_asin";
pub(crate) const HOST_IMPORT_MATH_ASINH: &str = "math_asinh";
pub(crate) const HOST_IMPORT_MATH_ATAN: &str = "math_atan";
pub(crate) const HOST_IMPORT_MATH_ATANH: &str = "math_atanh";
pub(crate) const HOST_IMPORT_MATH_CBRT: &str = "math_cbrt";
pub(crate) const HOST_IMPORT_MATH_COS: &str = "math_cos";
pub(crate) const HOST_IMPORT_MATH_COSH: &str = "math_cosh";
pub(crate) const HOST_IMPORT_MATH_EXP: &str = "math_exp";
pub(crate) const HOST_IMPORT_MATH_EXPM1: &str = "math_expm1";
pub(crate) const HOST_IMPORT_MATH_LOG: &str = "math_log";
pub(crate) const HOST_IMPORT_MATH_LOG10: &str = "math_log10";
pub(crate) const HOST_IMPORT_MATH_LOG1P: &str = "math_log1p";
pub(crate) const HOST_IMPORT_MATH_LOG2: &str = "math_log2";
pub(crate) const HOST_IMPORT_MATH_SIN: &str = "math_sin";
pub(crate) const HOST_IMPORT_MATH_SINH: &str = "math_sinh";
pub(crate) const HOST_IMPORT_MATH_TAN: &str = "math_tan";
pub(crate) const HOST_IMPORT_MATH_TANH: &str = "math_tanh";
pub(crate) const HOST_IMPORT_MATH_ATAN2: &str = "math_atan2";
pub(crate) const HOST_IMPORT_REJECT_RUNTIME_SEMANTICS: &str = "reject_runtime_semantics";

/// Numeric process state precedes GC roots. Neither slot contains a JS reference.
/// The module once guards follow the roots (see `RuntimeSchema::module_unit_guard`).
pub(crate) const PRIVATE_BYTE_CURSOR_GLOBAL_INDEX: u32 = 0;
pub(crate) const MODULE_EVALUATION_STATUS_GLOBAL_INDEX: u32 = 1;
/// Globals ahead of the GC roots: the byte cursor and the module status.
pub(crate) const NUMERIC_STATE_GLOBAL_COUNT: u32 = 2;

pub(crate) const HOST_AGENT_CAN_SUSPEND_IMPORT_FUNCTION_INDEX: u32 = 0;
pub(crate) const HOST_PRINT_IMPORT_FUNCTION_INDEX: u32 = 1;

pub(crate) fn canonical_host_function_realm_slot(
    builtin: HostBuiltinId,
) -> Option<crate::functions::NonArrayRealmIntrinsicSlot> {
    use crate::functions::NonArrayRealmIntrinsicSlot;
    match builtin {
        HostBuiltinId::ParseInt => Some(NonArrayRealmIntrinsicSlot::ParseInt),
        HostBuiltinId::ParseFloat => Some(NonArrayRealmIntrinsicSlot::ParseFloat),
        HostBuiltinId::Print
        | HostBuiltinId::Gc
        | HostBuiltinId::AssertThrows
        | HostBuiltinId::IsConstructor
        | HostBuiltinId::CreateRealm
        | HostBuiltinId::RealmEvalScript
        | HostBuiltinId::CreateHTMLDDA
        | HostBuiltinId::GetAbstractModuleSource
        | HostBuiltinId::GeneratorFunctionConstructor
        | HostBuiltinId::AsyncFunctionConstructor
        | HostBuiltinId::AsyncGeneratorFunctionConstructor
        | HostBuiltinId::AsyncDisposableStackSyncDispose
        | HostBuiltinId::HTMLDDA
        | HostBuiltinId::DetachArrayBuffer
        | HostBuiltinId::AgentStart
        | HostBuiltinId::AgentBroadcast
        | HostBuiltinId::AgentReceiveBroadcast
        | HostBuiltinId::AgentReport
        | HostBuiltinId::AgentGetReport
        | HostBuiltinId::AgentSleep
        | HostBuiltinId::AgentMonotonicNow
        | HostBuiltinId::AgentLeaving => None,
    }
}
pub(crate) fn canonical_host_function_realm_slot_by_name(
    name: &str,
) -> Option<crate::functions::NonArrayRealmIntrinsicSlot> {
    HostBuiltinId::from_global_name(name).and_then(canonical_host_function_realm_slot)
}

pub(crate) fn standard_builtin_constructor_realm_slot(
    builtin: StandardBuiltinId,
) -> Option<crate::functions::NonArrayRealmIntrinsicSlot> {
    match builtin {
        StandardBuiltinId::ReadableStreamConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::ReadableStreamConstructor)
        }
        StandardBuiltinId::ReadableStreamDefaultReaderConstructor => Some(
            crate::functions::NonArrayRealmIntrinsicSlot::ReadableStreamDefaultReaderConstructor,
        ),
        StandardBuiltinId::ReadableByteStreamControllerConstructor => Some(
            crate::functions::NonArrayRealmIntrinsicSlot::ReadableByteStreamControllerConstructor,
        ),
        StandardBuiltinId::ReadableStreamGetReader => None,
        StandardBuiltinId::ReadableStreamCancel => None,
        StandardBuiltinId::ReadableStreamLocked => None,
        StandardBuiltinId::ReadableStreamDefaultReaderRead => None,
        StandardBuiltinId::ReadableStreamDefaultReaderCancel => None,
        StandardBuiltinId::ReadableStreamDefaultReaderReleaseLock => None,
        StandardBuiltinId::ReadableStreamDefaultReaderClosed => None,
        StandardBuiltinId::ReadableByteStreamControllerEnqueue => None,
        StandardBuiltinId::ReadableByteStreamControllerClose => None,
        StandardBuiltinId::ReadableByteStreamControllerError => None,
        StandardBuiltinId::ReadableByteStreamControllerDesiredSize => None,
        StandardBuiltinId::ReadableStreamStartFulfilled => None,
        StandardBuiltinId::ReadableStreamStartRejected => None,
        StandardBuiltinId::ReadableStreamPullFulfilled => None,
        StandardBuiltinId::ReadableStreamPullRejected => None,
        StandardBuiltinId::ReadableStreamCancelFulfilled => None,
        StandardBuiltinId::ReadableStreamPullIfNeeded => None,
        StandardBuiltinId::AbortControllerConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::AbortControllerConstructor)
        }
        StandardBuiltinId::AbortSignalConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::AbortSignalConstructor)
        }
        StandardBuiltinId::EventTargetConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::EventTargetConstructor)
        }
        StandardBuiltinId::EventConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::EventConstructor)
        }
        StandardBuiltinId::DOMExceptionConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::DOMExceptionConstructor)
        }
        StandardBuiltinId::ShadowRealmConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::ShadowRealmConstructor)
        }
        StandardBuiltinId::AbortControllerPrototypeAbort
        | StandardBuiltinId::AbortControllerPrototypeSignalGetter
        | StandardBuiltinId::AbortSignalPrototypeThrowIfAborted
        | StandardBuiltinId::AbortSignalAbort
        | StandardBuiltinId::AbortSignalPrototypeAbortedGetter
        | StandardBuiltinId::AbortSignalPrototypeReasonGetter
        | StandardBuiltinId::AbortSignalPrototypeOnabortGetter
        | StandardBuiltinId::AbortSignalPrototypeOnabortSetter
        | StandardBuiltinId::EventTargetPrototypeAddEventListener
        | StandardBuiltinId::EventTargetPrototypeRemoveEventListener
        | StandardBuiltinId::EventTargetPrototypeDispatchEvent
        | StandardBuiltinId::EventPrototypePreventDefault
        | StandardBuiltinId::EventPrototypeStopPropagation
        | StandardBuiltinId::EventPrototypeStopImmediatePropagation
        | StandardBuiltinId::EventPrototypeTypeGetter
        | StandardBuiltinId::EventPrototypeTargetGetter
        | StandardBuiltinId::EventPrototypeCurrentTargetGetter
        | StandardBuiltinId::EventPrototypeDefaultPreventedGetter
        | StandardBuiltinId::EventPrototypeEventPhaseGetter
        | StandardBuiltinId::EventPrototypeBubblesGetter
        | StandardBuiltinId::EventPrototypeCancelableGetter
        | StandardBuiltinId::EventPrototypeComposedGetter
        | StandardBuiltinId::EventPrototypeIsTrustedGetter
        | StandardBuiltinId::DOMExceptionPrototypeNameGetter
        | StandardBuiltinId::DOMExceptionPrototypeMessageGetter
        | StandardBuiltinId::DOMExceptionPrototypeCodeGetter
        | StandardBuiltinId::ShadowRealmPrototypeEvaluate
        | StandardBuiltinId::ShadowRealmPrototypeImportValue
        | StandardBuiltinId::ShadowRealmWrappedFunctionCall
        | StandardBuiltinId::ShadowRealmImportFulfilled
        | StandardBuiltinId::ShadowRealmImportRejected => None,
        StandardBuiltinId::AbstractModuleSourceConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::AbstractModuleSourceConstructor)
        }
        StandardBuiltinId::FunctionConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::FunctionConstructor)
        }
        StandardBuiltinId::PromiseConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::PromiseConstructor)
        }
        StandardBuiltinId::MapConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::MapConstructor)
        }
        StandardBuiltinId::WeakMapConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::WeakMapConstructor)
        }
        StandardBuiltinId::WeakSetConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::WeakSetConstructor)
        }
        StandardBuiltinId::WeakRefConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::WeakRefConstructor)
        }
        StandardBuiltinId::FinalizationRegistryConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::FinalizationRegistryConstructor)
        }
        StandardBuiltinId::AsyncDisposableStackConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::AsyncDisposableStackConstructor)
        }
        StandardBuiltinId::DisposableStackConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::DisposableStackConstructor)
        }
        StandardBuiltinId::TextEncoderConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::TextEncoderConstructor)
        }
        StandardBuiltinId::TextDecoderConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::TextDecoderConstructor)
        }
        StandardBuiltinId::SetConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::SetConstructor)
        }
        StandardBuiltinId::AggregateErrorConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::AggregateErrorConstructor)
        }
        StandardBuiltinId::SuppressedErrorConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::SuppressedErrorConstructor)
        }
        StandardBuiltinId::ObjectConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::ObjectConstructor)
        }
        StandardBuiltinId::ProxyConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::ProxyConstructor)
        }
        StandardBuiltinId::IteratorConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::IteratorConstructor)
        }
        StandardBuiltinId::ArrayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::ArrayConstructor)
        }
        StandardBuiltinId::ArrayBufferConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::ArrayBufferConstructor)
        }
        StandardBuiltinId::SharedArrayBufferConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::SharedArrayBufferConstructor)
        }
        StandardBuiltinId::DataViewConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::DataViewConstructor)
        }
        StandardBuiltinId::TypedArrayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::TypedArrayConstructor)
        }
        StandardBuiltinId::DateConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::DateConstructor)
        }
        StandardBuiltinId::TemporalInstantConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::TemporalInstantConstructor)
        }
        StandardBuiltinId::TemporalPlainDateConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::TemporalPlainDateConstructor)
        }
        StandardBuiltinId::TemporalDurationConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::TemporalDurationConstructor)
        }
        StandardBuiltinId::TemporalPlainTimeConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::TemporalPlainTimeConstructor)
        }
        StandardBuiltinId::TemporalPlainDateTimeConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::TemporalPlainDateTimeConstructor)
        }
        StandardBuiltinId::TemporalPlainYearMonthConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::TemporalPlainYearMonthConstructor)
        }
        StandardBuiltinId::TemporalPlainMonthDayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::TemporalPlainMonthDayConstructor)
        }
        StandardBuiltinId::TemporalZonedDateTimeConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::TemporalZonedDateTimeConstructor)
        }
        StandardBuiltinId::IntlLocaleConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::IntlLocaleConstructor)
        }
        StandardBuiltinId::IntlDateTimeFormatConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::IntlDateTimeFormatConstructor)
        }
        StandardBuiltinId::IntlNumberFormatConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::IntlNumberFormatConstructor)
        }
        StandardBuiltinId::IntlPluralRulesConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::IntlPluralRulesConstructor)
        }
        StandardBuiltinId::IntlListFormatConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::IntlListFormatConstructor)
        }
        StandardBuiltinId::IntlCollatorConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::IntlCollatorConstructor)
        }
        StandardBuiltinId::IntlDisplayNamesConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::IntlDisplayNamesConstructor)
        }
        StandardBuiltinId::IntlRelativeTimeFormatConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::IntlRelativeTimeFormatConstructor)
        }
        StandardBuiltinId::IntlSegmenterConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::IntlSegmenterConstructor)
        }
        StandardBuiltinId::IntlDurationFormatConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::IntlDurationFormatConstructor)
        }
        StandardBuiltinId::RegExpConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::RegExpConstructor)
        }
        StandardBuiltinId::Float64ArrayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::Float64ArrayConstructor)
        }
        StandardBuiltinId::Float32ArrayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::Float32ArrayConstructor)
        }
        StandardBuiltinId::Float16ArrayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::Float16ArrayConstructor)
        }
        StandardBuiltinId::Int32ArrayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::Int32ArrayConstructor)
        }
        StandardBuiltinId::Int16ArrayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::Int16ArrayConstructor)
        }
        StandardBuiltinId::Int8ArrayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::Int8ArrayConstructor)
        }
        StandardBuiltinId::Uint32ArrayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::Uint32ArrayConstructor)
        }
        StandardBuiltinId::Uint16ArrayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::Uint16ArrayConstructor)
        }
        StandardBuiltinId::Uint8ArrayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::Uint8ArrayConstructor)
        }
        StandardBuiltinId::Uint8ClampedArrayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::Uint8ClampedArrayConstructor)
        }
        StandardBuiltinId::BigInt64ArrayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::BigInt64ArrayConstructor)
        }
        StandardBuiltinId::BigUint64ArrayConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::BigUint64ArrayConstructor)
        }
        StandardBuiltinId::BigIntConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::BigIntConstructor)
        }
        StandardBuiltinId::NumberConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::NumberConstructor)
        }
        StandardBuiltinId::StringConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::StringConstructor)
        }
        StandardBuiltinId::BooleanConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::BooleanConstructor)
        }
        StandardBuiltinId::SymbolConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::SymbolConstructor)
        }
        StandardBuiltinId::ErrorConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::ErrorConstructor)
        }
        StandardBuiltinId::EvalErrorConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::EvalErrorConstructor)
        }
        StandardBuiltinId::RangeErrorConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::RangeErrorConstructor)
        }
        StandardBuiltinId::SyntaxErrorConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::SyntaxErrorConstructor)
        }
        StandardBuiltinId::TypeErrorConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::TypeErrorConstructor)
        }
        StandardBuiltinId::URIErrorConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::URIErrorConstructor)
        }
        StandardBuiltinId::ReferenceErrorConstructor => {
            Some(crate::functions::NonArrayRealmIntrinsicSlot::ReferenceErrorConstructor)
        }
        StandardBuiltinId::FunctionPrototypeCall
        | StandardBuiltinId::FunctionPrototypeApply
        | StandardBuiltinId::FunctionPrototypeBind
        | StandardBuiltinId::FunctionPrototypeToString
        | StandardBuiltinId::RegExpPrototypeCompile
        | StandardBuiltinId::RegExpPrototypeExec
        | StandardBuiltinId::RegExpPrototypeTest
        | StandardBuiltinId::RegExpPrototypeToString
        | StandardBuiltinId::EvalFunction
        | StandardBuiltinId::StringFromCharCode
        | StandardBuiltinId::StringFromCodePoint
        | StandardBuiltinId::StringRaw
        | StandardBuiltinId::StringPrototypeToString
        | StandardBuiltinId::StringPrototypeValueOf
        | StandardBuiltinId::StringPrototypeCharAt
        | StandardBuiltinId::StringPrototypeConcat
        | StandardBuiltinId::StringPrototypeCharCodeAt
        | StandardBuiltinId::StringPrototypeCodePointAt
        | StandardBuiltinId::StringPrototypeAt
        | StandardBuiltinId::StringPrototypeAnchor
        | StandardBuiltinId::StringPrototypeBig
        | StandardBuiltinId::StringPrototypeBlink
        | StandardBuiltinId::StringPrototypeBold
        | StandardBuiltinId::StringPrototypeFixed
        | StandardBuiltinId::StringPrototypeFontcolor
        | StandardBuiltinId::StringPrototypeFontsize
        | StandardBuiltinId::StringPrototypeItalics
        | StandardBuiltinId::StringPrototypeLink
        | StandardBuiltinId::StringPrototypeSmall
        | StandardBuiltinId::StringPrototypeStrike
        | StandardBuiltinId::StringPrototypeSub
        | StandardBuiltinId::StringPrototypeSubstr
        | StandardBuiltinId::StringPrototypeSubstring
        | StandardBuiltinId::StringPrototypeSup
        | StandardBuiltinId::StringPrototypeMatch
        | StandardBuiltinId::StringPrototypeMatchAll
        | StandardBuiltinId::StringPrototypeReplace
        | StandardBuiltinId::StringPrototypeReplaceAll
        | StandardBuiltinId::StringPrototypeSearch
        | StandardBuiltinId::StringPrototypeIndexOf
        | StandardBuiltinId::StringPrototypeLastIndexOf
        | StandardBuiltinId::StringPrototypeSlice
        | StandardBuiltinId::StringPrototypeSplit
        | StandardBuiltinId::StringPrototypePadStart
        | StandardBuiltinId::StringPrototypePadEnd
        | StandardBuiltinId::StringPrototypeRepeat
        | StandardBuiltinId::StringPrototypeEndsWith
        | StandardBuiltinId::StringPrototypeIncludes
        | StandardBuiltinId::StringPrototypeStartsWith
        | StandardBuiltinId::StringPrototypeNormalize
        | StandardBuiltinId::StringPrototypeLocaleCompare
        | StandardBuiltinId::StringPrototypeIterator
        | StandardBuiltinId::StringPrototypeToLocaleLowerCase
        | StandardBuiltinId::StringPrototypeToLocaleUpperCase
        | StandardBuiltinId::StringPrototypeToLowerCase
        | StandardBuiltinId::StringPrototypeToUpperCase
        | StandardBuiltinId::StringPrototypeTrim
        | StandardBuiltinId::StringPrototypeTrimStart
        | StandardBuiltinId::StringPrototypeTrimEnd
        | StandardBuiltinId::StringPrototypeIsWellFormed
        | StandardBuiltinId::StringPrototypeToWellFormed
        | StandardBuiltinId::DateNow
        | StandardBuiltinId::DateParse
        | StandardBuiltinId::DateUtc
        | StandardBuiltinId::DatePrototypeGetTime
        | StandardBuiltinId::DatePrototypeSetTime
        | StandardBuiltinId::DatePrototypeValueOf
        | StandardBuiltinId::DatePrototypeGetFullYear
        | StandardBuiltinId::DatePrototypeGetUtcFullYear
        | StandardBuiltinId::DatePrototypeGetMonth
        | StandardBuiltinId::DatePrototypeGetUtcMonth
        | StandardBuiltinId::DatePrototypeGetDate
        | StandardBuiltinId::DatePrototypeGetUtcDate
        | StandardBuiltinId::DatePrototypeGetDay
        | StandardBuiltinId::DatePrototypeGetUtcDay
        | StandardBuiltinId::DatePrototypeGetHours
        | StandardBuiltinId::DatePrototypeGetUtcHours
        | StandardBuiltinId::DatePrototypeGetMinutes
        | StandardBuiltinId::DatePrototypeGetUtcMinutes
        | StandardBuiltinId::DatePrototypeGetSeconds
        | StandardBuiltinId::DatePrototypeGetUtcSeconds
        | StandardBuiltinId::DatePrototypeGetMilliseconds
        | StandardBuiltinId::DatePrototypeGetUtcMilliseconds
        | StandardBuiltinId::DatePrototypeGetTimezoneOffset
        | StandardBuiltinId::DatePrototypeGetYear
        | StandardBuiltinId::DatePrototypeSetYear
        | StandardBuiltinId::DatePrototypeSetFullYear
        | StandardBuiltinId::DatePrototypeSetUtcFullYear
        | StandardBuiltinId::DatePrototypeSetMonth
        | StandardBuiltinId::DatePrototypeSetUtcMonth
        | StandardBuiltinId::DatePrototypeSetDate
        | StandardBuiltinId::DatePrototypeSetUtcDate
        | StandardBuiltinId::DatePrototypeSetHours
        | StandardBuiltinId::DatePrototypeSetUtcHours
        | StandardBuiltinId::DatePrototypeSetMinutes
        | StandardBuiltinId::DatePrototypeSetUtcMinutes
        | StandardBuiltinId::DatePrototypeSetSeconds
        | StandardBuiltinId::DatePrototypeSetUtcSeconds
        | StandardBuiltinId::DatePrototypeSetMilliseconds
        | StandardBuiltinId::DatePrototypeSetUtcMilliseconds
        | StandardBuiltinId::DatePrototypeToIsoString
        | StandardBuiltinId::DatePrototypeToJson
        | StandardBuiltinId::DatePrototypeToPrimitive
        | StandardBuiltinId::DatePrototypeToDateString
        | StandardBuiltinId::DatePrototypeToLocaleDateString
        | StandardBuiltinId::DatePrototypeToLocaleString
        | StandardBuiltinId::DatePrototypeToLocaleTimeString
        | StandardBuiltinId::DatePrototypeToTemporalInstant
        | StandardBuiltinId::DatePrototypeToTimeString
        | StandardBuiltinId::DatePrototypeToString
        | StandardBuiltinId::DatePrototypeToUtcString
        | StandardBuiltinId::RegExpLegacyStaticGetter
        | StandardBuiltinId::RegExpLegacyStaticSetter
        | StandardBuiltinId::RegExpSpeciesGetter
        | StandardBuiltinId::RegExpPrototypeFlagsGetter
        | StandardBuiltinId::RegExpPrototypeSourceGetter
        | StandardBuiltinId::RegExpPrototypeHasIndicesGetter
        | StandardBuiltinId::RegExpPrototypeGlobalGetter
        | StandardBuiltinId::RegExpPrototypeIgnoreCaseGetter
        | StandardBuiltinId::RegExpPrototypeMultilineGetter
        | StandardBuiltinId::RegExpPrototypeDotAllGetter
        | StandardBuiltinId::RegExpPrototypeUnicodeGetter
        | StandardBuiltinId::RegExpPrototypeUnicodeSetsGetter
        | StandardBuiltinId::RegExpPrototypeStickyGetter
        | StandardBuiltinId::RegExpPrototypeSymbolMatch
        | StandardBuiltinId::RegExpPrototypeSymbolMatchAll
        | StandardBuiltinId::RegExpPrototypeSymbolReplace
        | StandardBuiltinId::RegExpPrototypeSymbolSearch
        | StandardBuiltinId::RegExpPrototypeSymbolSplit
        | StandardBuiltinId::RegExpEscape
        | StandardBuiltinId::ObjectCreate
        | StandardBuiltinId::ObjectGetPrototypeOf
        | StandardBuiltinId::ObjectSetPrototypeOf
        | StandardBuiltinId::ObjectDefineProperty
        | StandardBuiltinId::ObjectDefineProperties
        | StandardBuiltinId::ObjectGetOwnPropertyDescriptor
        | StandardBuiltinId::ObjectGetOwnPropertyDescriptors
        | StandardBuiltinId::ObjectAssign
        | StandardBuiltinId::ObjectGetOwnPropertyNames
        | StandardBuiltinId::ObjectGetOwnPropertySymbols
        | StandardBuiltinId::ObjectKeys
        | StandardBuiltinId::ObjectValues
        | StandardBuiltinId::ObjectEntries
        | StandardBuiltinId::ObjectHasOwn
        | StandardBuiltinId::ObjectIs
        | StandardBuiltinId::ObjectIsSealed
        | StandardBuiltinId::ObjectIsFrozen
        | StandardBuiltinId::ObjectSeal
        | StandardBuiltinId::ObjectFreeze
        | StandardBuiltinId::ObjectIsExtensible
        | StandardBuiltinId::ObjectPreventExtensions
        | StandardBuiltinId::ObjectPrototypeHasOwnProperty
        | StandardBuiltinId::ObjectPrototypeDefineGetter
        | StandardBuiltinId::ObjectPrototypeDefineSetter
        | StandardBuiltinId::ObjectPrototypeLookupGetter
        | StandardBuiltinId::ObjectPrototypeLookupSetter
        | StandardBuiltinId::ObjectPrototypeProtoGetter
        | StandardBuiltinId::ObjectPrototypeProtoSetter
        | StandardBuiltinId::ObjectPrototypePropertyIsEnumerable
        | StandardBuiltinId::ObjectPrototypeIsPrototypeOf
        | StandardBuiltinId::ObjectPrototypeToString
        | StandardBuiltinId::ObjectPrototypeToLocaleString
        | StandardBuiltinId::ObjectPrototypeValueOf
        | StandardBuiltinId::ProxyRevocable
        | StandardBuiltinId::ProxyRevoke
        | StandardBuiltinId::ReflectConstruct
        | StandardBuiltinId::ReflectApply
        | StandardBuiltinId::ReflectGet
        | StandardBuiltinId::ReflectGetPrototypeOf
        | StandardBuiltinId::ReflectGetOwnPropertyDescriptor
        | StandardBuiltinId::ReflectSet
        | StandardBuiltinId::ReflectHas
        | StandardBuiltinId::ReflectDefineProperty
        | StandardBuiltinId::ReflectDeleteProperty
        | StandardBuiltinId::ReflectIsExtensible
        | StandardBuiltinId::ReflectPreventExtensions
        | StandardBuiltinId::ReflectSetPrototypeOf
        | StandardBuiltinId::ReflectOwnKeys
        | StandardBuiltinId::ArrayFrom
        | StandardBuiltinId::ArrayFromAsync
        | StandardBuiltinId::ArrayFromAsyncFulfilled
        | StandardBuiltinId::ArrayFromAsyncRejected
        | StandardBuiltinId::ArrayOf
        | StandardBuiltinId::ArrayIsArray
        | StandardBuiltinId::ArraySpeciesGetter
        | StandardBuiltinId::TypedArraySpeciesGetter
        | StandardBuiltinId::ArrayPrototypeConcat
        | StandardBuiltinId::ArrayPrototypeJoin
        | StandardBuiltinId::ArrayPrototypeSlice
        | StandardBuiltinId::ArrayPrototypeSplice
        | StandardBuiltinId::ArrayPrototypeSort
        | StandardBuiltinId::ArrayPrototypeToLocaleString
        | StandardBuiltinId::ArrayPrototypeFlat
        | StandardBuiltinId::ArrayPrototypeFlatMap
        | StandardBuiltinId::ArrayPrototypeAt
        | StandardBuiltinId::ArrayPrototypeToReversed
        | StandardBuiltinId::ArrayPrototypeToSpliced
        | StandardBuiltinId::ArrayPrototypeToSorted
        | StandardBuiltinId::ArrayPrototypeWith
        | StandardBuiltinId::ArrayPrototypeReverse
        | StandardBuiltinId::ArrayPrototypeCopyWithin
        | StandardBuiltinId::ArrayPrototypeIncludes
        | StandardBuiltinId::ArrayPrototypeIndexOf
        | StandardBuiltinId::ArrayPrototypeLastIndexOf
        | StandardBuiltinId::ArrayPrototypeFind
        | StandardBuiltinId::ArrayPrototypeFindIndex
        | StandardBuiltinId::ArrayPrototypeFindLast
        | StandardBuiltinId::ArrayPrototypeFindLastIndex
        | StandardBuiltinId::ArrayPrototypeEvery
        | StandardBuiltinId::ArrayPrototypeSome
        | StandardBuiltinId::ArrayPrototypeForEach
        | StandardBuiltinId::ArrayPrototypeFilter
        | StandardBuiltinId::ArrayPrototypeMap
        | StandardBuiltinId::ArrayPrototypeReduce
        | StandardBuiltinId::ArrayPrototypeReduceRight
        | StandardBuiltinId::ArrayPrototypePop
        | StandardBuiltinId::ArrayPrototypePush
        | StandardBuiltinId::ArrayPrototypeShift
        | StandardBuiltinId::ArrayPrototypeUnshift
        | StandardBuiltinId::ArrayPrototypeFill
        | StandardBuiltinId::ArrayPrototypeKeys
        | StandardBuiltinId::ArrayPrototypeEntries
        | StandardBuiltinId::ArrayPrototypeValues
        | StandardBuiltinId::ArrayIteratorNext
        | StandardBuiltinId::ArrayIteratorIdentity
        | StandardBuiltinId::AsyncIteratorIdentity
        | StandardBuiltinId::StringIteratorNext
        | StandardBuiltinId::RegExpStringIteratorNext
        | StandardBuiltinId::GeneratorPrototypeNext
        | StandardBuiltinId::GeneratorPrototypeReturn
        | StandardBuiltinId::GeneratorPrototypeThrow
        | StandardBuiltinId::AsyncGeneratorPrototypeNext
        | StandardBuiltinId::AsyncGeneratorPrototypeReturn
        | StandardBuiltinId::AsyncGeneratorPrototypeThrow
        | StandardBuiltinId::AsyncIteratorPrototypeAsyncDispose
        | StandardBuiltinId::AsyncIteratorPrototypeAsyncDisposeFulfilled
        | StandardBuiltinId::IteratorFrom
        | StandardBuiltinId::IteratorConcat
        | StandardBuiltinId::IteratorConcatNext
        | StandardBuiltinId::IteratorConcatReturn
        | StandardBuiltinId::IteratorZip
        | StandardBuiltinId::IteratorZipKeyed
        | StandardBuiltinId::IteratorZipNext
        | StandardBuiltinId::IteratorZipReturn
        | StandardBuiltinId::IteratorHelperNext
        | StandardBuiltinId::IteratorHelperReturn
        | StandardBuiltinId::IteratorPrototypeToArray
        | StandardBuiltinId::IteratorPrototypeIncludes
        | StandardBuiltinId::IteratorPrototypeJoin
        | StandardBuiltinId::IteratorPrototypeForEach
        | StandardBuiltinId::IteratorPrototypeEvery
        | StandardBuiltinId::IteratorPrototypeSome
        | StandardBuiltinId::IteratorPrototypeFind
        | StandardBuiltinId::IteratorPrototypeReduce
        | StandardBuiltinId::IteratorPrototypeMap
        | StandardBuiltinId::IteratorMapNext
        | StandardBuiltinId::IteratorMapReturn
        | StandardBuiltinId::IteratorPrototypeFilter
        | StandardBuiltinId::IteratorFilterNext
        | StandardBuiltinId::IteratorFilterReturn
        | StandardBuiltinId::IteratorPrototypeFlatMap
        | StandardBuiltinId::IteratorFlatMapNext
        | StandardBuiltinId::IteratorFlatMapReturn
        | StandardBuiltinId::IteratorPrototypeTake
        | StandardBuiltinId::IteratorTakeNext
        | StandardBuiltinId::IteratorTakeReturn
        | StandardBuiltinId::IteratorPrototypeChunks
        | StandardBuiltinId::IteratorPrototypeWindows
        | StandardBuiltinId::IteratorBufferedNext
        | StandardBuiltinId::IteratorBufferedReturn
        | StandardBuiltinId::IteratorPrototypeDrop
        | StandardBuiltinId::IteratorDropNext
        | StandardBuiltinId::IteratorDropReturn
        | StandardBuiltinId::IteratorPrototypeConstructorGetter
        | StandardBuiltinId::IteratorPrototypeConstructorSetter
        | StandardBuiltinId::IteratorPrototypeSymbolDispose
        | StandardBuiltinId::IteratorPrototypeToStringTagGetter
        | StandardBuiltinId::IteratorPrototypeToStringTagSetter
        | StandardBuiltinId::IteratorFromWrapperNext
        | StandardBuiltinId::IteratorFromWrapperReturn
        | StandardBuiltinId::ArrayBufferIsView
        | StandardBuiltinId::BigIntAsIntN
        | StandardBuiltinId::BigIntAsUintN
        | StandardBuiltinId::BigIntPrototypeToString
        | StandardBuiltinId::BigIntPrototypeToLocaleString
        | StandardBuiltinId::BigIntPrototypeValueOf
        | StandardBuiltinId::NumberIsInteger
        | StandardBuiltinId::NumberIsSafeInteger
        | StandardBuiltinId::NumberIsFinite
        | StandardBuiltinId::NumberIsNaN
        | StandardBuiltinId::NumberPrototypeToExponential
        | StandardBuiltinId::NumberPrototypeToFixed
        | StandardBuiltinId::NumberPrototypeToPrecision
        | StandardBuiltinId::NumberPrototypeToString
        | StandardBuiltinId::NumberPrototypeToLocaleString
        | StandardBuiltinId::NumberPrototypeValueOf
        | StandardBuiltinId::BooleanPrototypeToString
        | StandardBuiltinId::BooleanPrototypeValueOf
        | StandardBuiltinId::GlobalIsFinite
        | StandardBuiltinId::GlobalIsNaN
        | StandardBuiltinId::MathAbs
        | StandardBuiltinId::MathAcos
        | StandardBuiltinId::MathAcosh
        | StandardBuiltinId::MathAsin
        | StandardBuiltinId::MathAsinh
        | StandardBuiltinId::MathAtan
        | StandardBuiltinId::MathAtan2
        | StandardBuiltinId::MathAtanh
        | StandardBuiltinId::MathCbrt
        | StandardBuiltinId::MathCeil
        | StandardBuiltinId::MathClz32
        | StandardBuiltinId::MathCos
        | StandardBuiltinId::MathCosh
        | StandardBuiltinId::MathExp
        | StandardBuiltinId::MathExpm1
        | StandardBuiltinId::MathF16Round
        | StandardBuiltinId::MathFloor
        | StandardBuiltinId::MathFround
        | StandardBuiltinId::MathHypot
        | StandardBuiltinId::MathImul
        | StandardBuiltinId::MathLog
        | StandardBuiltinId::MathLog10
        | StandardBuiltinId::MathLog1p
        | StandardBuiltinId::MathLog2
        | StandardBuiltinId::MathPow
        | StandardBuiltinId::MathRandom
        | StandardBuiltinId::MathRound
        | StandardBuiltinId::MathSign
        | StandardBuiltinId::MathSin
        | StandardBuiltinId::MathSinh
        | StandardBuiltinId::MathSqrt
        | StandardBuiltinId::MathSumPrecise
        | StandardBuiltinId::MathTan
        | StandardBuiltinId::MathTanh
        | StandardBuiltinId::MathTrunc
        | StandardBuiltinId::MathMin
        | StandardBuiltinId::MathMax
        | StandardBuiltinId::ErrorIsError
        | StandardBuiltinId::ArrayBufferSpeciesGetter
        | StandardBuiltinId::ArrayBufferPrototypeByteLengthGetter
        | StandardBuiltinId::SharedArrayBufferPrototypeByteLengthGetter
        | StandardBuiltinId::SharedArrayBufferPrototypeMaxByteLengthGetter
        | StandardBuiltinId::SharedArrayBufferPrototypeGrowableGetter
        | StandardBuiltinId::SharedArrayBufferPrototypeGrow
        | StandardBuiltinId::ArrayBufferPrototypeDetachedGetter
        | StandardBuiltinId::ArrayBufferPrototypeMaxByteLengthGetter
        | StandardBuiltinId::ArrayBufferPrototypeResizableGetter
        | StandardBuiltinId::ArrayBufferPrototypeImmutableGetter
        | StandardBuiltinId::ArrayBufferPrototypeResize
        | StandardBuiltinId::ArrayBufferPrototypeSlice
        | StandardBuiltinId::SharedArrayBufferPrototypeSlice
        | StandardBuiltinId::ArrayBufferPrototypeTransfer
        | StandardBuiltinId::ArrayBufferPrototypeTransferToFixedLength
        | StandardBuiltinId::ArrayBufferPrototypeTransferToImmutable
        | StandardBuiltinId::ArrayBufferPrototypeSliceToImmutable
        | StandardBuiltinId::DataViewPrototypeBufferGetter
        | StandardBuiltinId::DataViewPrototypeByteLengthGetter
        | StandardBuiltinId::DataViewPrototypeByteOffsetGetter
        | StandardBuiltinId::TypedArrayPrototypeBufferGetter
        | StandardBuiltinId::TypedArrayPrototypeByteLengthGetter
        | StandardBuiltinId::TypedArrayPrototypeByteOffsetGetter
        | StandardBuiltinId::TypedArrayPrototypeLengthGetter
        | StandardBuiltinId::TypedArrayPrototypeToStringTagGetter
        | StandardBuiltinId::TypedArrayPrototypeToString
        | StandardBuiltinId::Uint8ArrayFromBase64
        | StandardBuiltinId::Uint8ArrayFromHex
        | StandardBuiltinId::Uint8ArrayPrototypeSetFromBase64
        | StandardBuiltinId::Uint8ArrayPrototypeSetFromHex
        | StandardBuiltinId::Uint8ArrayPrototypeToBase64
        | StandardBuiltinId::Uint8ArrayPrototypeToHex
        | StandardBuiltinId::TypedArrayPrototypeAt
        | StandardBuiltinId::TypedArrayPrototypeIncludes
        | StandardBuiltinId::TypedArrayPrototypeIndexOf
        | StandardBuiltinId::TypedArrayPrototypeLastIndexOf
        | StandardBuiltinId::TypedArrayPrototypeFind
        | StandardBuiltinId::TypedArrayPrototypeFindIndex
        | StandardBuiltinId::TypedArrayPrototypeFindLast
        | StandardBuiltinId::TypedArrayPrototypeFindLastIndex
        | StandardBuiltinId::TypedArrayPrototypeEvery
        | StandardBuiltinId::TypedArrayPrototypeSome
        | StandardBuiltinId::TypedArrayPrototypeMap
        | StandardBuiltinId::TypedArrayPrototypeFilter
        | StandardBuiltinId::TypedArrayPrototypeForEach
        | StandardBuiltinId::TypedArrayPrototypeReduce
        | StandardBuiltinId::TypedArrayPrototypeReduceRight
        | StandardBuiltinId::TypedArrayPrototypeValues
        | StandardBuiltinId::TypedArrayPrototypeKeys
        | StandardBuiltinId::TypedArrayPrototypeEntries
        | StandardBuiltinId::TypedArrayPrototypeFill
        | StandardBuiltinId::TypedArrayPrototypeJoin
        | StandardBuiltinId::TypedArrayPrototypeToLocaleString
        | StandardBuiltinId::TypedArrayPrototypeSubarray
        | StandardBuiltinId::TypedArrayPrototypeSlice
        | StandardBuiltinId::TypedArrayPrototypeSet
        | StandardBuiltinId::TypedArrayPrototypeReverse
        | StandardBuiltinId::TypedArrayPrototypeCopyWithin
        | StandardBuiltinId::TypedArrayPrototypeSort
        | StandardBuiltinId::TypedArrayPrototypeToReversed
        | StandardBuiltinId::TypedArrayPrototypeToSorted
        | StandardBuiltinId::TypedArrayPrototypeWith
        | StandardBuiltinId::TypedArrayFrom
        | StandardBuiltinId::TypedArrayOf
        | StandardBuiltinId::DataViewPrototypeGetUint8
        | StandardBuiltinId::DataViewPrototypeSetUint8
        | StandardBuiltinId::DataViewPrototypeGetInt8
        | StandardBuiltinId::DataViewPrototypeSetInt8
        | StandardBuiltinId::DataViewPrototypeGetUint16
        | StandardBuiltinId::DataViewPrototypeSetUint16
        | StandardBuiltinId::DataViewPrototypeGetInt16
        | StandardBuiltinId::DataViewPrototypeSetInt16
        | StandardBuiltinId::DataViewPrototypeGetUint32
        | StandardBuiltinId::DataViewPrototypeSetUint32
        | StandardBuiltinId::DataViewPrototypeGetInt32
        | StandardBuiltinId::DataViewPrototypeSetInt32
        | StandardBuiltinId::DataViewPrototypeGetFloat16
        | StandardBuiltinId::DataViewPrototypeSetFloat16
        | StandardBuiltinId::DataViewPrototypeGetFloat32
        | StandardBuiltinId::DataViewPrototypeSetFloat32
        | StandardBuiltinId::DataViewPrototypeGetFloat64
        | StandardBuiltinId::DataViewPrototypeSetFloat64
        | StandardBuiltinId::DataViewPrototypeGetBigInt64
        | StandardBuiltinId::DataViewPrototypeSetBigInt64
        | StandardBuiltinId::DataViewPrototypeGetBigUint64
        | StandardBuiltinId::DataViewPrototypeSetBigUint64
        | StandardBuiltinId::ErrorPrototypeToString
        | StandardBuiltinId::ErrorPrototypeStackGetter
        | StandardBuiltinId::ErrorPrototypeStackSetter
        | StandardBuiltinId::ThrowTypeError
        | StandardBuiltinId::AbstractModuleSourcePrototypeToStringTagGetter
        | StandardBuiltinId::JsonParse
        | StandardBuiltinId::JsonStringify
        | StandardBuiltinId::JsonRawJson
        | StandardBuiltinId::JsonIsRawJson
        | StandardBuiltinId::AtomicsAdd
        | StandardBuiltinId::AtomicsAnd
        | StandardBuiltinId::AtomicsCompareExchange
        | StandardBuiltinId::AtomicsExchange
        | StandardBuiltinId::AtomicsLoad
        | StandardBuiltinId::AtomicsNotify
        | StandardBuiltinId::AtomicsOr
        | StandardBuiltinId::AtomicsPause
        | StandardBuiltinId::AtomicsSub
        | StandardBuiltinId::AtomicsStore
        | StandardBuiltinId::AtomicsWait
        | StandardBuiltinId::AtomicsWaitAsync
        | StandardBuiltinId::QueueMicrotask
        | StandardBuiltinId::SetTimeout
        | StandardBuiltinId::ClearTimeout
        | StandardBuiltinId::AtomicsXor
        | StandardBuiltinId::AtomicsIsLockFree
        | StandardBuiltinId::Escape
        | StandardBuiltinId::Unescape
        | StandardBuiltinId::EncodeUri
        | StandardBuiltinId::EncodeUriComponent
        | StandardBuiltinId::DecodeUri
        | StandardBuiltinId::DecodeUriComponent
        | StandardBuiltinId::SymbolFor
        | StandardBuiltinId::SymbolKeyFor
        | StandardBuiltinId::SymbolPrototypeDescriptionGetter
        | StandardBuiltinId::SymbolPrototypeToString
        | StandardBuiltinId::SymbolPrototypeValueOf
        | StandardBuiltinId::SymbolPrototypeToPrimitive
        | StandardBuiltinId::PromisePrototypeThen
        | StandardBuiltinId::PromisePrototypeCatch
        | StandardBuiltinId::PromisePrototypeFinally
        | StandardBuiltinId::PromiseThenFinally
        | StandardBuiltinId::PromiseCatchFinally
        | StandardBuiltinId::PromiseValueThunk
        | StandardBuiltinId::PromiseThrower
        | StandardBuiltinId::PromiseSpeciesGetter
        | StandardBuiltinId::MapSpeciesGetter
        | StandardBuiltinId::SetSpeciesGetter
        | StandardBuiltinId::PromiseResolve
        | StandardBuiltinId::PromiseWithResolvers
        | StandardBuiltinId::PromiseTry
        | StandardBuiltinId::PromiseReject
        | StandardBuiltinId::PromiseAll
        | StandardBuiltinId::PromiseAllSettled
        | StandardBuiltinId::PromiseAllKeyed
        | StandardBuiltinId::PromiseAllSettledKeyed
        | StandardBuiltinId::PromiseAny
        | StandardBuiltinId::PromiseRace
        | StandardBuiltinId::PromiseAllResolveElement
        | StandardBuiltinId::PromiseAllSettledResolveElement
        | StandardBuiltinId::PromiseAllSettledRejectElement
        | StandardBuiltinId::PromiseAnyRejectElement
        | StandardBuiltinId::PromiseAllKeyedResolveElement
        | StandardBuiltinId::PromiseAllSettledKeyedResolveElement
        | StandardBuiltinId::PromiseAllSettledKeyedRejectElement
        | StandardBuiltinId::PromiseCapabilityExecutor
        | StandardBuiltinId::PromiseResolveFunction
        | StandardBuiltinId::PromiseRejectFunction
        | StandardBuiltinId::MapGroupBy
        | StandardBuiltinId::ObjectGroupBy
        | StandardBuiltinId::ObjectFromEntries
        | StandardBuiltinId::MapPrototypeClear
        | StandardBuiltinId::MapPrototypeDelete
        | StandardBuiltinId::MapPrototypeForEach
        | StandardBuiltinId::MapPrototypeKeys
        | StandardBuiltinId::MapPrototypeValues
        | StandardBuiltinId::MapPrototypeEntries
        | StandardBuiltinId::MapIteratorNext
        | StandardBuiltinId::MapPrototypeGet
        | StandardBuiltinId::MapPrototypeGetOrInsert
        | StandardBuiltinId::MapPrototypeGetOrInsertComputed
        | StandardBuiltinId::MapPrototypeHas
        | StandardBuiltinId::MapPrototypeSet
        | StandardBuiltinId::MapPrototypeSizeGetter
        | StandardBuiltinId::WeakMapPrototypeDelete
        | StandardBuiltinId::WeakMapPrototypeGet
        | StandardBuiltinId::WeakMapPrototypeGetOrInsert
        | StandardBuiltinId::WeakMapPrototypeGetOrInsertComputed
        | StandardBuiltinId::WeakMapPrototypeHas
        | StandardBuiltinId::WeakMapPrototypeSet
        | StandardBuiltinId::WeakSetPrototypeAdd
        | StandardBuiltinId::WeakSetPrototypeDelete
        | StandardBuiltinId::WeakSetPrototypeHas
        | StandardBuiltinId::SetPrototypeAdd
        | StandardBuiltinId::SetPrototypeClear
        | StandardBuiltinId::SetPrototypeDelete
        | StandardBuiltinId::SetPrototypeDifference
        | StandardBuiltinId::SetPrototypeForEach
        | StandardBuiltinId::SetPrototypeIntersection
        | StandardBuiltinId::SetPrototypeIsDisjointFrom
        | StandardBuiltinId::SetPrototypeIsSubsetOf
        | StandardBuiltinId::SetPrototypeIsSupersetOf
        | StandardBuiltinId::SetPrototypeSymmetricDifference
        | StandardBuiltinId::SetPrototypeUnion
        | StandardBuiltinId::SetPrototypeValues
        | StandardBuiltinId::SetPrototypeEntries
        | StandardBuiltinId::SetIteratorNext
        | StandardBuiltinId::SetPrototypeHas
        | StandardBuiltinId::SetPrototypeSizeGetter
        | StandardBuiltinId::TemporalPlainDateFrom
        | StandardBuiltinId::TemporalPlainDateCompare
        | StandardBuiltinId::TemporalPlainDatePrototypeCalendarIdGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeEraGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeEraYearGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeYearGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeMonthGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeMonthCodeGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeDayGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeDayOfWeekGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeDayOfYearGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeWeekOfYearGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeYearOfWeekGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeDaysInWeekGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeDaysInMonthGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeDaysInYearGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeMonthsInYearGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeInLeapYearGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeWith
        | StandardBuiltinId::TemporalPlainDatePrototypeWithCalendar
        | StandardBuiltinId::TemporalPlainDatePrototypeEquals
        | StandardBuiltinId::TemporalPlainDatePrototypeToString
        | StandardBuiltinId::TemporalPlainDatePrototypeToJson
        | StandardBuiltinId::TemporalPlainDatePrototypeToLocaleString
        | StandardBuiltinId::TemporalPlainDatePrototypeValueOf
        | StandardBuiltinId::TemporalPlainDatePrototypeAdd
        | StandardBuiltinId::TemporalPlainDatePrototypeSubtract
        | StandardBuiltinId::TemporalPlainDatePrototypeUntil
        | StandardBuiltinId::TemporalPlainDatePrototypeSince
        | StandardBuiltinId::TemporalPlainDatePrototypeToPlainDateTime
        | StandardBuiltinId::TemporalPlainDatePrototypeToPlainYearMonth
        | StandardBuiltinId::TemporalPlainDatePrototypeToPlainMonthDay
        | StandardBuiltinId::TemporalPlainYearMonthFrom
        | StandardBuiltinId::TemporalPlainYearMonthCompare
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeCalendarIdGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeEraGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeEraYearGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeYearGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthCodeGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInYearGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInMonthGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthsInYearGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeInLeapYearGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeWith
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeAdd
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeSubtract
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeUntil
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeSince
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeEquals
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeToString
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeToJson
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeToLocaleString
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeValueOf
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeToPlainDate
        | StandardBuiltinId::TemporalPlainMonthDayFrom
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeCalendarIdGetter
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeMonthCodeGetter
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeDayGetter
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeWith
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeEquals
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeToString
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeToJson
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeToLocaleString
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeValueOf
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeToPlainDate
        | StandardBuiltinId::TemporalDurationFrom
        | StandardBuiltinId::TemporalDurationCompare
        | StandardBuiltinId::TemporalDurationPrototypeYearsGetter
        | StandardBuiltinId::TemporalDurationPrototypeMonthsGetter
        | StandardBuiltinId::TemporalDurationPrototypeWeeksGetter
        | StandardBuiltinId::TemporalDurationPrototypeDaysGetter
        | StandardBuiltinId::TemporalDurationPrototypeHoursGetter
        | StandardBuiltinId::TemporalDurationPrototypeMinutesGetter
        | StandardBuiltinId::TemporalDurationPrototypeSecondsGetter
        | StandardBuiltinId::TemporalDurationPrototypeMillisecondsGetter
        | StandardBuiltinId::TemporalDurationPrototypeMicrosecondsGetter
        | StandardBuiltinId::TemporalDurationPrototypeNanosecondsGetter
        | StandardBuiltinId::TemporalDurationPrototypeSignGetter
        | StandardBuiltinId::TemporalDurationPrototypeBlankGetter
        | StandardBuiltinId::TemporalDurationPrototypeWith
        | StandardBuiltinId::TemporalDurationPrototypeNegated
        | StandardBuiltinId::TemporalDurationPrototypeAbs
        | StandardBuiltinId::TemporalDurationPrototypeAdd
        | StandardBuiltinId::TemporalDurationPrototypeSubtract
        | StandardBuiltinId::TemporalDurationPrototypeRound
        | StandardBuiltinId::TemporalDurationPrototypeTotal
        | StandardBuiltinId::TemporalDurationPrototypeToString
        | StandardBuiltinId::TemporalDurationPrototypeToJson
        | StandardBuiltinId::TemporalDurationPrototypeToLocaleString
        | StandardBuiltinId::TemporalDurationPrototypeValueOf
        | StandardBuiltinId::TemporalPlainTimeFrom
        | StandardBuiltinId::TemporalPlainTimeCompare
        | StandardBuiltinId::TemporalPlainTimePrototypeHourGetter
        | StandardBuiltinId::TemporalPlainTimePrototypeMinuteGetter
        | StandardBuiltinId::TemporalPlainTimePrototypeSecondGetter
        | StandardBuiltinId::TemporalPlainTimePrototypeMillisecondGetter
        | StandardBuiltinId::TemporalPlainTimePrototypeMicrosecondGetter
        | StandardBuiltinId::TemporalPlainTimePrototypeNanosecondGetter
        | StandardBuiltinId::TemporalPlainTimePrototypeWith
        | StandardBuiltinId::TemporalPlainTimePrototypeAdd
        | StandardBuiltinId::TemporalPlainTimePrototypeSubtract
        | StandardBuiltinId::TemporalPlainTimePrototypeUntil
        | StandardBuiltinId::TemporalPlainTimePrototypeSince
        | StandardBuiltinId::TemporalPlainTimePrototypeRound
        | StandardBuiltinId::TemporalPlainTimePrototypeEquals
        | StandardBuiltinId::TemporalPlainTimePrototypeToString
        | StandardBuiltinId::TemporalPlainTimePrototypeToJson
        | StandardBuiltinId::TemporalPlainTimePrototypeToLocaleString
        | StandardBuiltinId::TemporalPlainTimePrototypeValueOf
        | StandardBuiltinId::TemporalPlainDateTimeFrom
        | StandardBuiltinId::TemporalPlainDateTimeCompare
        | StandardBuiltinId::TemporalPlainDateTimePrototypeCalendarIdGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeEraGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeEraYearGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeYearGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthCodeGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeDayGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeHourGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeMinuteGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeSecondGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeMillisecondGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeMicrosecondGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeNanosecondGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfWeekGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfYearGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeWeekOfYearGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeYearOfWeekGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInWeekGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInMonthGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInYearGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthsInYearGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeInLeapYearGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeWith
        | StandardBuiltinId::TemporalPlainDateTimePrototypeWithPlainTime
        | StandardBuiltinId::TemporalPlainDateTimePrototypeWithCalendar
        | StandardBuiltinId::TemporalPlainDateTimePrototypeAdd
        | StandardBuiltinId::TemporalPlainDateTimePrototypeSubtract
        | StandardBuiltinId::TemporalPlainDateTimePrototypeUntil
        | StandardBuiltinId::TemporalPlainDateTimePrototypeSince
        | StandardBuiltinId::TemporalPlainDateTimePrototypeRound
        | StandardBuiltinId::TemporalPlainDateTimePrototypeEquals
        | StandardBuiltinId::TemporalPlainDateTimePrototypeToString
        | StandardBuiltinId::TemporalPlainDateTimePrototypeToJson
        | StandardBuiltinId::TemporalPlainDateTimePrototypeToLocaleString
        | StandardBuiltinId::TemporalPlainDateTimePrototypeValueOf
        | StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainDate
        | StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainTime
        | StandardBuiltinId::TemporalPlainDateTimePrototypeToZonedDateTime
        | StandardBuiltinId::TemporalNowInstant
        | StandardBuiltinId::TemporalNowTimeZoneId
        | StandardBuiltinId::TemporalNowZonedDateTimeIso
        | StandardBuiltinId::TemporalNowPlainDateTimeIso
        | StandardBuiltinId::TemporalNowPlainDateIso
        | StandardBuiltinId::TemporalNowPlainTimeIso
        | StandardBuiltinId::TemporalInstantPrototypeEpochMillisecondsGetter
        | StandardBuiltinId::TemporalInstantPrototypeEpochNanosecondsGetter
        | StandardBuiltinId::TemporalInstantPrototypeAdd
        | StandardBuiltinId::TemporalInstantPrototypeSubtract
        | StandardBuiltinId::TemporalInstantPrototypeRound
        | StandardBuiltinId::TemporalInstantPrototypeUntil
        | StandardBuiltinId::TemporalInstantPrototypeSince
        | StandardBuiltinId::TemporalInstantPrototypeEquals
        | StandardBuiltinId::TemporalInstantFrom
        | StandardBuiltinId::TemporalInstantCompare
        | StandardBuiltinId::TemporalInstantFromEpochMilliseconds
        | StandardBuiltinId::TemporalInstantFromEpochNanoseconds
        | StandardBuiltinId::TemporalInstantPrototypeToString
        | StandardBuiltinId::TemporalInstantPrototypeToLocaleString
        | StandardBuiltinId::TemporalInstantPrototypeToJson
        | StandardBuiltinId::TemporalInstantPrototypeValueOf
        | StandardBuiltinId::TemporalInstantPrototypeToZonedDateTimeIso
        | StandardBuiltinId::TemporalZonedDateTimeFrom
        | StandardBuiltinId::TemporalZonedDateTimeCompare
        | StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfWeekGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfYearGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeWeekOfYearGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeYearOfWeekGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInWeekGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInMonthGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInYearGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthsInYearGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeInLeapYearGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeToString
        | StandardBuiltinId::TemporalZonedDateTimePrototypeToJson
        | StandardBuiltinId::TemporalZonedDateTimePrototypeValueOf
        | StandardBuiltinId::TemporalZonedDateTimePrototypeToLocaleString
        | StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainTime
        | StandardBuiltinId::TemporalZonedDateTimePrototypeWithPlainTime
        | StandardBuiltinId::TemporalZonedDateTimePrototypeWith
        | StandardBuiltinId::TemporalZonedDateTimePrototypeRound
        | StandardBuiltinId::TemporalZonedDateTimePrototypeGetTimeZoneTransition
        | StandardBuiltinId::TemporalZonedDateTimePrototypeHoursInDayGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeStartOfDay
        | StandardBuiltinId::TemporalPlainDatePrototypeToZonedDateTime
        | StandardBuiltinId::TemporalZonedDateTimePrototypeEpochMillisecondsGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeEpochNanosecondsGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetNanosecondsGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeTimeZoneIdGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeCalendarIdGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeEraGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeEraYearGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeYearGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthCodeGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeDayGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeHourGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeMinuteGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeSecondGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeMillisecondGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeMicrosecondGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeNanosecondGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeEquals
        | StandardBuiltinId::TemporalZonedDateTimePrototypeToInstant
        | StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDate
        | StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDateTime
        | StandardBuiltinId::TemporalZonedDateTimePrototypeWithTimeZone
        | StandardBuiltinId::TemporalZonedDateTimePrototypeWithCalendar
        | StandardBuiltinId::TemporalZonedDateTimePrototypeAdd
        | StandardBuiltinId::TemporalZonedDateTimePrototypeSubtract
        | StandardBuiltinId::TemporalZonedDateTimePrototypeUntil
        | StandardBuiltinId::TemporalZonedDateTimePrototypeSince
        | StandardBuiltinId::IntlSupportedValuesOf
        | StandardBuiltinId::IntlGetCanonicalLocales
        | StandardBuiltinId::IntlLocalePrototypeLanguageGetter
        | StandardBuiltinId::IntlLocalePrototypeScriptGetter
        | StandardBuiltinId::IntlLocalePrototypeRegionGetter
        | StandardBuiltinId::IntlLocalePrototypeBaseNameGetter
        | StandardBuiltinId::IntlLocalePrototypeCalendarGetter
        | StandardBuiltinId::IntlLocalePrototypeCollationGetter
        | StandardBuiltinId::IntlLocalePrototypeFirstDayOfWeekGetter
        | StandardBuiltinId::IntlLocalePrototypeHourCycleGetter
        | StandardBuiltinId::IntlLocalePrototypeCaseFirstGetter
        | StandardBuiltinId::IntlLocalePrototypeNumericGetter
        | StandardBuiltinId::IntlLocalePrototypeNumberingSystemGetter
        | StandardBuiltinId::IntlLocalePrototypeVariantsGetter
        | StandardBuiltinId::IntlLocalePrototypeToString
        | StandardBuiltinId::IntlLocalePrototypeMaximize
        | StandardBuiltinId::IntlLocalePrototypeMinimize
        | StandardBuiltinId::IntlLocalePrototypeGetWeekInfo
        | StandardBuiltinId::IntlLocalePrototypeGetCalendars
        | StandardBuiltinId::IntlLocalePrototypeGetCollations
        | StandardBuiltinId::IntlLocalePrototypeGetTimeZones
        | StandardBuiltinId::IntlLocalePrototypeGetNumberingSystems
        | StandardBuiltinId::IntlLocalePrototypeGetHourCycles
        | StandardBuiltinId::IntlLocalePrototypeGetTextInfo
        | StandardBuiltinId::IntlDateTimeFormatSupportedLocalesOf
        | StandardBuiltinId::IntlDateTimeFormatPrototypeResolvedOptions
        | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatGetter
        | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatToParts
        | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRange
        | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRangeToParts
        | StandardBuiltinId::IntlDateTimeFormatBoundFormat
        | StandardBuiltinId::IntlPluralRulesSupportedLocalesOf
        | StandardBuiltinId::IntlPluralRulesPrototypeResolvedOptions
        | StandardBuiltinId::IntlPluralRulesPrototypeSelect
        | StandardBuiltinId::IntlPluralRulesPrototypeSelectRange
        | StandardBuiltinId::IntlListFormatSupportedLocalesOf
        | StandardBuiltinId::IntlListFormatPrototypeResolvedOptions
        | StandardBuiltinId::IntlListFormatPrototypeFormat
        | StandardBuiltinId::IntlListFormatPrototypeFormatToParts
        | StandardBuiltinId::IntlCollatorSupportedLocalesOf
        | StandardBuiltinId::IntlCollatorPrototypeResolvedOptions
        | StandardBuiltinId::IntlCollatorPrototypeCompareGetter
        | StandardBuiltinId::IntlCollatorBoundCompare
        | StandardBuiltinId::IntlDisplayNamesSupportedLocalesOf
        | StandardBuiltinId::IntlDisplayNamesPrototypeResolvedOptions
        | StandardBuiltinId::IntlDisplayNamesPrototypeOf
        | StandardBuiltinId::IntlRelativeTimeFormatSupportedLocalesOf
        | StandardBuiltinId::IntlDurationFormatSupportedLocalesOf
        | StandardBuiltinId::IntlRelativeTimeFormatPrototypeResolvedOptions
        | StandardBuiltinId::IntlDurationFormatPrototypeResolvedOptions
        | StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormat
        | StandardBuiltinId::IntlDurationFormatPrototypeFormat
        | StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormatToParts
        | StandardBuiltinId::IntlDurationFormatPrototypeFormatToParts
        | StandardBuiltinId::IntlSegmenterSupportedLocalesOf
        | StandardBuiltinId::IntlSegmenterPrototypeSegment
        | StandardBuiltinId::IntlSegmenterPrototypeResolvedOptions
        | StandardBuiltinId::IntlSegmentsPrototypeContaining
        | StandardBuiltinId::IntlSegmentsPrototypeIterator
        | StandardBuiltinId::IntlSegmentIteratorPrototypeNext
        | StandardBuiltinId::IntlNumberFormatSupportedLocalesOf
        | StandardBuiltinId::IntlNumberFormatPrototypeResolvedOptions
        | StandardBuiltinId::IntlNumberFormatPrototypeFormatGetter
        | StandardBuiltinId::IntlNumberFormatPrototypeFormatToParts
        | StandardBuiltinId::IntlNumberFormatPrototypeFormatRange
        | StandardBuiltinId::IntlNumberFormatPrototypeFormatRangeToParts
        | StandardBuiltinId::IntlNumberFormatBoundFormat
        | StandardBuiltinId::WeakRefPrototypeDeref
        | StandardBuiltinId::FinalizationRegistryPrototypeRegister
        | StandardBuiltinId::FinalizationRegistryPrototypeUnregister
        | StandardBuiltinId::AsyncDisposableStackPrototypeUse
        | StandardBuiltinId::AsyncDisposableStackPrototypeAdopt
        | StandardBuiltinId::AsyncDisposableStackPrototypeDefer
        | StandardBuiltinId::AsyncDisposableStackPrototypeMove
        | StandardBuiltinId::AsyncDisposableStackPrototypeDisposeAsync
        | StandardBuiltinId::AsyncDisposableStackPrototypeDisposedGetter
        | StandardBuiltinId::AsyncDisposableStackDisposeAsyncFulfilled
        | StandardBuiltinId::AsyncDisposableStackDisposeAsyncRejected
        | StandardBuiltinId::DisposableStackPrototypeUse
        | StandardBuiltinId::DisposableStackPrototypeAdopt
        | StandardBuiltinId::DisposableStackPrototypeDefer
        | StandardBuiltinId::DisposableStackPrototypeMove
        | StandardBuiltinId::DisposableStackPrototypeDispose
        | StandardBuiltinId::DisposableStackPrototypeDisposedGetter
        | StandardBuiltinId::TextEncoderPrototypeEncode
        | StandardBuiltinId::TextEncoderPrototypeEncodeInto
        | StandardBuiltinId::TextEncoderPrototypeEncodingGetter
        | StandardBuiltinId::TextDecoderPrototypeDecode
        | StandardBuiltinId::TextDecoderPrototypeEncodingGetter
        | StandardBuiltinId::TextDecoderPrototypeFatalGetter
        | StandardBuiltinId::TextDecoderPrototypeIgnoreBOMGetter
        | StandardBuiltinId::FunctionPrototype
        | StandardBuiltinId::FunctionPrototypeSymbolHasInstance => None,
    }
}

pub(crate) fn typed_array_constructor_bytes_per_element_entries() -> [(StandardBuiltinId, u64); 12]
{
    TypedArrayElementKind::ALL.map(|kind| (kind.constructor(), kind.bytes_per_element()))
}

pub(crate) fn host_builtin_by_name(name: &str) -> Option<HostBuiltinId> {
    HostBuiltinId::from_global_name(name)
}

pub(crate) fn typed_array_bytes_per_element(builtin: StandardBuiltinId) -> u64 {
    TypedArrayElementKind::from_constructor(builtin)
        .expect("typed-array constructor must have an element width")
        .bytes_per_element()
}

pub(crate) fn is_typed_array_constructor(builtin: StandardBuiltinId) -> bool {
    TypedArrayElementKind::from_constructor(builtin).is_some()
}

pub(crate) fn standard_builtin_function_realm_slot(
    builtin: StandardBuiltinId,
) -> Option<crate::functions::NonArrayRealmIntrinsicSlot> {
    use crate::functions::NonArrayRealmIntrinsicSlot as Slot;
    match builtin {
        StandardBuiltinId::FunctionPrototype => Some(Slot::FunctionPrototype),
        StandardBuiltinId::ArrayPrototypeValues => Some(Slot::ArrayPrototypeValues),
        StandardBuiltinId::RegExpPrototypeSymbolMatch => Some(Slot::RegExpPrototypeSymbolMatch),
        StandardBuiltinId::RegExpPrototypeSymbolMatchAll => {
            Some(Slot::RegExpPrototypeSymbolMatchAll)
        }
        StandardBuiltinId::RegExpPrototypeSymbolSearch => Some(Slot::RegExpPrototypeSymbolSearch),
        StandardBuiltinId::TypedArrayPrototypeToString => Some(Slot::TypedArrayPrototypeToString),
        StandardBuiltinId::ThrowTypeError => Some(Slot::ThrowTypeError),
        _ => standard_builtin_constructor_realm_slot(builtin),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmArtifact {
    pub(crate) gc_host_imports: Vec<crate::gc_types::GcHostImport>,
    pub bytes: Vec<u8>,
    pub invariant_note: &'static str,
    pub debug_dump: String,
    /// Per-function attribution for every body in `bytes`, in code-section
    /// order, always populated.
    ///
    /// Typed rather than parsed back out of `debug_dump`: the dump's full
    /// report is opt-in behind `SCRIPTW_EMIT_SIZE_REPORT` and its only printer
    /// lives two crates away, so "how big is `js::probe#f0`?" was a question the
    /// compiler could answer but no test could ask. It is derived from the same
    /// single [`crate::emitted_function::ModuleFunctionTable::summaries`] call
    /// that renders the `largest emitted function:` line, so the two cannot
    /// disagree.
    pub function_sizes: Vec<EmittedFunctionSummary>,
    pub(crate) runtime: Option<std::sync::Arc<crate::RuntimeArtifact>>,
}

impl WasmArtifact {
    /// Exact imports selected by the same declaration plan that emitted bytes.
    pub fn gc_host_imports(&self) -> &[crate::gc_types::GcHostImport] {
        &self.gc_host_imports
    }

    /// The runtime this program module links against; `None` means `bytes`
    /// is a standalone module.
    pub fn runtime(&self) -> Option<&std::sync::Arc<crate::RuntimeArtifact>> {
        self.runtime.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitError {
    message: String,
}

impl EmitError {
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// A function body exceeded the configured per-function budget.
    ///
    /// The constructor takes the [`FunctionIdentity`] rather than a name or a
    /// bare size, so this diagnostic cannot be produced without knowing which
    /// function it is about — precisely what the `[origin:unknown] ... Code for
    /// function is too large` failure lacks. The budget arrives as a
    /// [`FunctionBodyBudget`], validated once at construction, so there is no
    /// bare `u32` threshold to mis-thread.
    pub(crate) fn function_too_large(
        identity: &FunctionIdentity,
        body_bytes: FunctionBodySize,
        budget: FunctionBodyBudget,
    ) -> Self {
        Self {
            message: format!(
                "emitted function body exceeds the configured budget: {} ({}) is {} against a budget of {}",
                identity.wasm_name(),
                identity.category(),
                body_bytes,
                budget
            ),
        }
    }
}

impl core::fmt::Display for EmitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EmitError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn finalized_runtime_sections_have_one_unsplittable_assembly_surface() {
        let module_source = include_str!("module.rs");
        let package_source = include_str!("module/compiled_module_package.rs");
        let emit_source = include_str!("emit/module_assembly.rs");
        let module_production = module_source
            .split_once("#[cfg(test)]")
            .expect("module test boundary")
            .0;

        assert_eq!(
            package_source
                .matches(concat!("pub(crate) fn append_", "to_module("))
                .count(),
            1,
            "the sealed package must have one consuming module-assembly transition"
        );
        for rejected_surface in [
            "push_main_to",
            "append_types_to",
            "append_globals_to",
            concat!("CompilingModule", "Package"),
            concat!("impl FnOnce(&Finalized", "ModuleGlobals)"),
        ] {
            assert!(
                !package_source.contains(rejected_surface),
                "split package surface returned: {rejected_surface}"
            );
        }
        assert!(package_source.contains("compilation.compile(self.runtime.globals())?"));
        assert!(package_source.contains("CompiledModulePackage::append_functions;"));
        assert!(!module_production.contains("struct CompiledModulePackage"));
        assert_eq!(
            emit_source
                .matches("module_package.append_to_module(")
                .count(),
            1,
            "the emitter must consume exactly one sealed package"
        );
        for raw_append in [
            "module.section(&types)",
            "module.section(&globals)",
            "module.section(runtime.types())",
            "module.section(runtime.globals())",
            "module.section(&code)",
        ] {
            assert!(
                !emit_source.contains(raw_append),
                "runtime section escaped its sealed package: {raw_append}"
            );
        }
    }

    #[test]
    fn iterator_concat_builtins_use_runtime_function_objects() {
        for builtin in [
            StandardBuiltinId::IteratorConcat,
            StandardBuiltinId::IteratorConcatNext,
            StandardBuiltinId::IteratorConcatReturn,
        ] {
            assert!(standard_builtin_constructor_realm_slot(builtin).is_none());
            assert!(standard_builtin_function_realm_slot(builtin).is_none());
        }
    }

    #[test]
    fn host_builtin_lookup_uses_the_catalog_global_surface() {
        for builtin in HostBuiltinId::ALL.iter().copied() {
            match builtin.global_name() {
                Some(name) => assert_eq!(host_builtin_by_name(name), Some(builtin)),
                None => assert_eq!(host_builtin_by_name(builtin.as_str()), None),
            }
        }
    }
}
