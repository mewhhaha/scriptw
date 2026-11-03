//! Complete fresh-Realm layouts. Native metadata is sealed from the actual code plan.
use super::*;

pub(super) fn populate(
    id: IntrinsicPropertyLayoutId,
    plan: &mut LayoutBuilder,
) -> Result<(), EmitError> {
    use IntrinsicPropertyLayoutId as Layout;
    use NonArrayRealmIntrinsicSlot as Slot;
    use StandardBuiltinId as Builtin;

    // Every constructor owns metadata first; Proxy alone has no public prototype.
    let constructor_prototype = match id {
        Layout::FunctionConstructor => {
            Some(IntrinsicPropertyRecipe::ready(Slot::FunctionPrototype)?)
        }
        Layout::ObjectConstructor => Some(IntrinsicPropertyRecipe::ready(Slot::ObjectPrototype)?),
        Layout::AbstractModuleSourceConstructor => Some(IntrinsicPropertyRecipe::ready(
            Slot::AbstractModuleSourcePrototype,
        )?),
        Layout::AbortControllerConstructor => Some(IntrinsicPropertyRecipe::ready(
            Slot::AbortControllerPrototype,
        )?),
        Layout::AbortSignalConstructor => {
            Some(IntrinsicPropertyRecipe::ready(Slot::AbortSignalPrototype)?)
        }
        Layout::EventTargetConstructor => {
            Some(IntrinsicPropertyRecipe::ready(Slot::EventTargetPrototype)?)
        }
        Layout::EventConstructor => Some(IntrinsicPropertyRecipe::ready(Slot::EventPrototype)?),
        Layout::DOMExceptionConstructor => {
            Some(IntrinsicPropertyRecipe::ready(Slot::DOMExceptionPrototype)?)
        }
        Layout::ReadableStreamConstructor => Some(IntrinsicPropertyRecipe::ready(
            Slot::ReadableStreamPrototype,
        )?),
        Layout::ReadableStreamDefaultReaderConstructor => Some(IntrinsicPropertyRecipe::ready(
            Slot::ReadableStreamDefaultReaderPrototype,
        )?),
        Layout::ReadableByteStreamControllerConstructor => Some(IntrinsicPropertyRecipe::ready(
            Slot::ReadableByteStreamControllerPrototype,
        )?),
        Layout::ShadowRealmConstructor => {
            Some(IntrinsicPropertyRecipe::ready(Slot::ShadowRealmPrototype)?)
        }
        Layout::IteratorConstructor => {
            Some(IntrinsicPropertyRecipe::ready(Slot::IteratorPrototype)?)
        }
        Layout::ArrayConstructor => Some(IntrinsicPropertyRecipe::ArrayPrototype),
        Layout::NumberConstructor => Some(IntrinsicPropertyRecipe::ready(Slot::NumberPrototype)?),
        Layout::StringConstructor => Some(IntrinsicPropertyRecipe::ready(Slot::StringPrototype)?),
        Layout::BooleanConstructor => Some(IntrinsicPropertyRecipe::ready(Slot::BooleanPrototype)?),
        Layout::PromiseConstructor => Some(IntrinsicPropertyRecipe::ready(Slot::PromisePrototype)?),
        Layout::MapConstructor => Some(IntrinsicPropertyRecipe::ready(Slot::MapPrototype)?),
        Layout::WeakMapConstructor => Some(IntrinsicPropertyRecipe::ready(Slot::WeakMapPrototype)?),
        Layout::WeakRefConstructor => Some(IntrinsicPropertyRecipe::ready(Slot::WeakRefPrototype)?),
        Layout::FinalizationRegistryConstructor => Some(IntrinsicPropertyRecipe::ready(
            Slot::FinalizationRegistryPrototype,
        )?),
        Layout::WeakSetConstructor => Some(IntrinsicPropertyRecipe::ready(Slot::WeakSetPrototype)?),
        Layout::AsyncDisposableStackConstructor => Some(IntrinsicPropertyRecipe::ready(
            Slot::AsyncDisposableStackPrototype,
        )?),
        Layout::DisposableStackConstructor => Some(IntrinsicPropertyRecipe::ready(
            Slot::DisposableStackPrototype,
        )?),
        Layout::SetConstructor => Some(IntrinsicPropertyRecipe::ready(Slot::SetPrototype)?),
        Layout::TextEncoderConstructor => {
            Some(IntrinsicPropertyRecipe::ready(Slot::TextEncoderPrototype)?)
        }
        Layout::TextDecoderConstructor => {
            Some(IntrinsicPropertyRecipe::ready(Slot::TextDecoderPrototype)?)
        }
        Layout::SymbolConstructor => Some(IntrinsicPropertyRecipe::ready(Slot::SymbolPrototype)?),
        Layout::BigIntConstructor => Some(IntrinsicPropertyRecipe::ready(Slot::BigIntPrototype)?),
        Layout::GeneratorFunctionConstructor => Some(IntrinsicPropertyRecipe::ready(
            Slot::GeneratorFunctionPrototype,
        )?),
        Layout::AsyncFunctionConstructor => Some(IntrinsicPropertyRecipe::ready(
            Slot::AsyncFunctionPrototype,
        )?),
        Layout::AsyncGeneratorFunctionConstructor => Some(IntrinsicPropertyRecipe::ready(
            Slot::AsyncGeneratorFunctionPrototype,
        )?),
        _ => None,
    };
    if let Some(prototype) = constructor_prototype {
        plan.native_metadata(id)?;
        plan.data(
            IntrinsicKey::Name("prototype"),
            prototype,
            false,
            false,
            false,
        );
    }

    match id {
        Layout::ReadableStreamConstructor => {}
        Layout::ReadableStreamPrototype => {
            plan.constructor(Slot::ReadableStreamConstructor)?;
            web_getter(
                plan,
                IntrinsicKey::Name("locked"),
                Builtin::ReadableStreamLocked,
            )?;
            plan.data(
                IntrinsicKey::Name("cancel"),
                IntrinsicPropertyRecipe::Method(Builtin::ReadableStreamCancel),
                true,
                true,
                true,
            );
            plan.data(
                IntrinsicKey::Name("getReader"),
                IntrinsicPropertyRecipe::Method(Builtin::ReadableStreamGetReader),
                true,
                true,
                true,
            );
            plan.tag("ReadableStream");
        }
        Layout::ReadableStreamDefaultReaderConstructor => {}
        Layout::ReadableStreamDefaultReaderPrototype => {
            plan.constructor(Slot::ReadableStreamDefaultReaderConstructor)?;
            plan.data(
                IntrinsicKey::Name("cancel"),
                IntrinsicPropertyRecipe::Method(Builtin::ReadableStreamDefaultReaderCancel),
                true,
                true,
                true,
            );
            plan.data(
                IntrinsicKey::Name("read"),
                IntrinsicPropertyRecipe::Method(Builtin::ReadableStreamDefaultReaderRead),
                true,
                true,
                true,
            );
            plan.data(
                IntrinsicKey::Name("releaseLock"),
                IntrinsicPropertyRecipe::Method(Builtin::ReadableStreamDefaultReaderReleaseLock),
                true,
                true,
                true,
            );
            web_getter(
                plan,
                IntrinsicKey::Name("closed"),
                Builtin::ReadableStreamDefaultReaderClosed,
            )?;
            plan.tag("ReadableStreamDefaultReader");
        }
        Layout::ReadableByteStreamControllerConstructor => {}
        Layout::ReadableByteStreamControllerPrototype => {
            plan.constructor(Slot::ReadableByteStreamControllerConstructor)?;
            web_getter(
                plan,
                IntrinsicKey::Name("desiredSize"),
                Builtin::ReadableByteStreamControllerDesiredSize,
            )?;
            plan.data(
                IntrinsicKey::Name("close"),
                IntrinsicPropertyRecipe::Method(Builtin::ReadableByteStreamControllerClose),
                true,
                true,
                true,
            );
            plan.data(
                IntrinsicKey::Name("enqueue"),
                IntrinsicPropertyRecipe::Method(Builtin::ReadableByteStreamControllerEnqueue),
                true,
                true,
                true,
            );
            plan.data(
                IntrinsicKey::Name("error"),
                IntrinsicPropertyRecipe::Method(Builtin::ReadableByteStreamControllerError),
                true,
                true,
                true,
            );
            plan.tag("ReadableByteStreamController");
        }
        Layout::ProxyConstructor => {
            plan.native_metadata(id)?;
            named_method(plan, "revocable", Builtin::ProxyRevocable);
        }
        Layout::ObjectConstructor => {
            plan.methods(IntrinsicMethodPlan::ObjectConstructorInitial)?;
            plan.methods(IntrinsicMethodPlan::ObjectConstructorRemaining)?;
        }
        Layout::IteratorConstructor => plan.methods(IntrinsicMethodPlan::IteratorConstructor)?,
        Layout::ArrayConstructor => {
            plan.methods(IntrinsicMethodPlan::ArrayConstructor)?;
            getter(
                plan,
                IntrinsicKey::Symbol(WellKnownSymbol::Species),
                Builtin::ArraySpeciesGetter,
            )?;
        }
        Layout::NumberConstructor => {
            for (name, value) in [
                ("NaN", f64::NAN),
                ("POSITIVE_INFINITY", f64::INFINITY),
                ("NEGATIVE_INFINITY", f64::NEG_INFINITY),
                ("MAX_VALUE", f64::MAX),
                ("MIN_VALUE", f64::from_bits(1)),
                ("EPSILON", f64::EPSILON),
                ("MAX_SAFE_INTEGER", 9007199254740991.0),
                ("MIN_SAFE_INTEGER", -9007199254740991.0),
            ] {
                plan.data(
                    IntrinsicKey::Name(name),
                    IntrinsicPropertyRecipe::Number(value.to_bits()),
                    false,
                    false,
                    false,
                );
            }
            plan.methods(IntrinsicMethodPlan::NumberConstructor)?;
            for (name, slot) in [
                ("parseInt", Slot::ParseInt),
                ("parseFloat", Slot::ParseFloat),
            ] {
                plan.data(
                    IntrinsicKey::Name(name),
                    IntrinsicPropertyRecipe::ready(slot)?,
                    true,
                    false,
                    true,
                );
            }
        }
        Layout::StringConstructor => {
            for (name, builtin) in [
                ("fromCharCode", Builtin::StringFromCharCode),
                ("fromCodePoint", Builtin::StringFromCodePoint),
                ("raw", Builtin::StringRaw),
            ] {
                named_method(plan, name, builtin);
            }
        }
        Layout::PromiseConstructor => {
            plan.methods(IntrinsicMethodPlan::PromiseConstructor)?;
            getter(
                plan,
                IntrinsicKey::Symbol(WellKnownSymbol::Species),
                Builtin::PromiseSpeciesGetter,
            )?;
        }
        Layout::MapConstructor => {
            named_method(plan, "groupBy", Builtin::MapGroupBy);
            getter(
                plan,
                IntrinsicKey::Symbol(WellKnownSymbol::Species),
                Builtin::MapSpeciesGetter,
            )?;
        }
        Layout::SetConstructor => getter(
            plan,
            IntrinsicKey::Symbol(WellKnownSymbol::Species),
            Builtin::SetSpeciesGetter,
        )?,
        Layout::SymbolConstructor => {
            for symbol in [
                WellKnownSymbol::Iterator,
                WellKnownSymbol::AsyncIterator,
                WellKnownSymbol::HasInstance,
                WellKnownSymbol::IsConcatSpreadable,
                WellKnownSymbol::Match,
                WellKnownSymbol::MatchAll,
                WellKnownSymbol::Replace,
                WellKnownSymbol::Search,
                WellKnownSymbol::Species,
                WellKnownSymbol::Split,
                WellKnownSymbol::ToPrimitive,
                WellKnownSymbol::ToStringTag,
                WellKnownSymbol::Unscopables,
                WellKnownSymbol::Dispose,
                WellKnownSymbol::AsyncDispose,
            ] {
                plan.data(
                    IntrinsicKey::Name(symbol.member_name()),
                    IntrinsicPropertyRecipe::WellKnownSymbol(symbol),
                    false,
                    false,
                    false,
                );
            }
            named_method(plan, "for", Builtin::SymbolFor);
            named_method(plan, "keyFor", Builtin::SymbolKeyFor);
        }
        Layout::BigIntConstructor => plan.methods(IntrinsicMethodPlan::BigIntConstructor)?,
        Layout::FunctionConstructor
        | Layout::AbstractModuleSourceConstructor
        | Layout::ShadowRealmConstructor
        | Layout::BooleanConstructor
        | Layout::WeakMapConstructor
        | Layout::WeakRefConstructor
        | Layout::FinalizationRegistryConstructor
        | Layout::WeakSetConstructor
        | Layout::AsyncDisposableStackConstructor
        | Layout::DisposableStackConstructor
        | Layout::TextEncoderConstructor
        | Layout::TextDecoderConstructor
        | Layout::GeneratorFunctionConstructor
        | Layout::AsyncFunctionConstructor
        | Layout::AsyncGeneratorFunctionConstructor => {}
        Layout::FunctionPrototype => {
            plan.native_metadata(id)?;
            plan.constructor(Slot::FunctionConstructor)?;
            plan.methods(IntrinsicMethodPlan::FunctionPrototype)?;
            plan.data(
                IntrinsicKey::Symbol(WellKnownSymbol::HasInstance),
                IntrinsicPropertyRecipe::Method(Builtin::FunctionPrototypeSymbolHasInstance),
                false,
                false,
                false,
            );
            for name in ["arguments", "caller"] {
                plan.accessor(
                    IntrinsicKey::Name(name),
                    IntrinsicCallableRecipe::ready(Slot::ThrowTypeError)?,
                    IntrinsicCallableRecipe::ready(Slot::ThrowTypeError)?,
                );
            }
        }
        Layout::NumberPrototype => {
            plan.constructor(Slot::NumberConstructor)?;
            plan.methods(IntrinsicMethodPlan::NumberPrototype)?;
        }
        Layout::BooleanPrototype => {
            plan.constructor(Slot::BooleanConstructor)?;
            plan.methods(IntrinsicMethodPlan::BooleanPrototype)?;
        }
        Layout::BigIntPrototype => {
            plan.constructor(Slot::BigIntConstructor)?;
            plan.tag("BigInt");
            plan.methods(IntrinsicMethodPlan::BigIntPrototype)?;
        }
        Layout::PromisePrototype => {
            plan.constructor(Slot::PromiseConstructor)?;
            plan.methods(IntrinsicMethodPlan::PromisePrototype)?;
            plan.tag("Promise");
        }
        Layout::MapPrototype => {
            plan.constructor(Slot::MapConstructor)?;
            plan.methods(IntrinsicMethodPlan::MapPrototype)?;
            plan.aliases(
                &[
                    IntrinsicKey::Name("entries"),
                    IntrinsicKey::Symbol(WellKnownSymbol::Iterator),
                ],
                IntrinsicPropertyRecipe::Method(Builtin::MapPrototypeEntries),
            );
            getter(
                plan,
                IntrinsicKey::Name("size"),
                Builtin::MapPrototypeSizeGetter,
            )?;
            plan.tag("Map");
        }
        Layout::WeakMapPrototype => {
            plan.constructor(Slot::WeakMapConstructor)?;
            plan.methods(IntrinsicMethodPlan::WeakMapPrototype)?;
            plan.tag("WeakMap");
        }
        Layout::WeakSetPrototype => {
            plan.constructor(Slot::WeakSetConstructor)?;
            plan.methods(IntrinsicMethodPlan::WeakSetPrototype)?;
            plan.tag("WeakSet");
        }
        Layout::WeakRefPrototype => {
            plan.constructor(Slot::WeakRefConstructor)?;
            named_method(plan, "deref", Builtin::WeakRefPrototypeDeref);
            plan.tag("WeakRef");
        }
        Layout::FinalizationRegistryPrototype => {
            plan.constructor(Slot::FinalizationRegistryConstructor)?;
            named_method(
                plan,
                "register",
                Builtin::FinalizationRegistryPrototypeRegister,
            );
            named_method(
                plan,
                "unregister",
                Builtin::FinalizationRegistryPrototypeUnregister,
            );
            plan.tag("FinalizationRegistry");
        }
        Layout::AsyncDisposableStackPrototype => {
            plan.constructor(Slot::AsyncDisposableStackConstructor)?;
            plan.methods(IntrinsicMethodPlan::AsyncDisposableStackPrototype)?;
            plan.aliases(
                &[
                    IntrinsicKey::Name("disposeAsync"),
                    IntrinsicKey::Symbol(WellKnownSymbol::AsyncDispose),
                ],
                IntrinsicPropertyRecipe::Method(Builtin::AsyncDisposableStackPrototypeDisposeAsync),
            );
            getter(
                plan,
                IntrinsicKey::Name("disposed"),
                Builtin::AsyncDisposableStackPrototypeDisposedGetter,
            )?;
            plan.tag("AsyncDisposableStack");
        }
        Layout::TextEncoderPrototype => {
            plan.constructor(Slot::TextEncoderConstructor)?;
            plan.data(
                IntrinsicKey::Name("encode"),
                IntrinsicPropertyRecipe::Method(Builtin::TextEncoderPrototypeEncode),
                true,
                true,
                true,
            );
            plan.data(
                IntrinsicKey::Name("encodeInto"),
                IntrinsicPropertyRecipe::Method(Builtin::TextEncoderPrototypeEncodeInto),
                true,
                true,
                true,
            );
            web_getter(
                plan,
                IntrinsicKey::Name("encoding"),
                Builtin::TextEncoderPrototypeEncodingGetter,
            )?;
            plan.tag("TextEncoder");
        }
        Layout::TextDecoderPrototype => {
            plan.constructor(Slot::TextDecoderConstructor)?;
            plan.data(
                IntrinsicKey::Name("decode"),
                IntrinsicPropertyRecipe::Method(Builtin::TextDecoderPrototypeDecode),
                true,
                true,
                true,
            );
            web_getter(
                plan,
                IntrinsicKey::Name("encoding"),
                Builtin::TextDecoderPrototypeEncodingGetter,
            )?;
            web_getter(
                plan,
                IntrinsicKey::Name("fatal"),
                Builtin::TextDecoderPrototypeFatalGetter,
            )?;
            web_getter(
                plan,
                IntrinsicKey::Name("ignoreBOM"),
                Builtin::TextDecoderPrototypeIgnoreBOMGetter,
            )?;
            plan.tag("TextDecoder");
        }
        Layout::DisposableStackPrototype => {
            plan.constructor(Slot::DisposableStackConstructor)?;
            plan.methods(IntrinsicMethodPlan::DisposableStackPrototype)?;
            plan.aliases(
                &[
                    IntrinsicKey::Name("dispose"),
                    IntrinsicKey::Symbol(WellKnownSymbol::Dispose),
                ],
                IntrinsicPropertyRecipe::Method(Builtin::DisposableStackPrototypeDispose),
            );
            getter(
                plan,
                IntrinsicKey::Name("disposed"),
                Builtin::DisposableStackPrototypeDisposedGetter,
            )?;
            plan.tag("DisposableStack");
        }
        Layout::SetPrototype => {
            plan.constructor(Slot::SetConstructor)?;
            plan.methods(IntrinsicMethodPlan::SetPrototype)?;
            plan.aliases(
                &[
                    IntrinsicKey::Name("values"),
                    IntrinsicKey::Name("keys"),
                    IntrinsicKey::Symbol(WellKnownSymbol::Iterator),
                ],
                IntrinsicPropertyRecipe::Method(Builtin::SetPrototypeValues),
            );
            named_method(plan, "entries", Builtin::SetPrototypeEntries);
            getter(
                plan,
                IntrinsicKey::Name("size"),
                Builtin::SetPrototypeSizeGetter,
            )?;
            plan.tag("Set");
        }
        Layout::SymbolPrototype => {
            plan.constructor(Slot::SymbolConstructor)?;
            plan.tag("Symbol");
            named_method(plan, "toString", Builtin::SymbolPrototypeToString);
            named_method(plan, "valueOf", Builtin::SymbolPrototypeValueOf);
            getter(
                plan,
                IntrinsicKey::Name("description"),
                Builtin::SymbolPrototypeDescriptionGetter,
            )?;
            plan.data(
                IntrinsicKey::Symbol(WellKnownSymbol::ToPrimitive),
                IntrinsicPropertyRecipe::Method(Builtin::SymbolPrototypeToPrimitive),
                false,
                false,
                true,
            );
        }
        Layout::IteratorPrototype => {
            plan.methods(IntrinsicMethodPlan::IteratorPrototype)?;
            plan.accessor(
                IntrinsicKey::Name("constructor"),
                IntrinsicCallableRecipe::method(Builtin::IteratorPrototypeConstructorGetter)?,
                IntrinsicCallableRecipe::method(Builtin::IteratorPrototypeConstructorSetter)?,
            );
            symbol_method(
                plan,
                WellKnownSymbol::Iterator,
                Builtin::ArrayIteratorIdentity,
            );
            symbol_method(
                plan,
                WellKnownSymbol::Dispose,
                Builtin::IteratorPrototypeSymbolDispose,
            );
            plan.accessor(
                IntrinsicKey::Symbol(WellKnownSymbol::ToStringTag),
                IntrinsicCallableRecipe::method(Builtin::IteratorPrototypeToStringTagGetter)?,
                IntrinsicCallableRecipe::method(Builtin::IteratorPrototypeToStringTagSetter)?,
            );
        }
        Layout::IteratorFromWrapperPrototype => {
            named_method(plan, "next", Builtin::IteratorFromWrapperNext);
            named_method(plan, "return", Builtin::IteratorFromWrapperReturn);
        }
        Layout::IteratorHelperPrototype => {
            named_method(plan, "next", Builtin::IteratorHelperNext);
            named_method(plan, "return", Builtin::IteratorHelperReturn);
            plan.tag("Iterator Helper");
        }
        Layout::AbstractModuleSourcePrototype => {
            plan.constructor(Slot::AbstractModuleSourceConstructor)?;
            getter(
                plan,
                IntrinsicKey::Symbol(WellKnownSymbol::ToStringTag),
                Builtin::AbstractModuleSourcePrototypeToStringTagGetter,
            )?;
        }
        Layout::AbortControllerConstructor => {}
        Layout::AbortControllerPrototype => {
            plan.constructor(Slot::AbortControllerConstructor)?;
            plan.tag("AbortController");
            web_method(plan, "abort", Builtin::AbortControllerPrototypeAbort);
            web_getter(
                plan,
                IntrinsicKey::Name("signal"),
                Builtin::AbortControllerPrototypeSignalGetter,
            )?;
        }
        Layout::AbortSignalConstructor => {
            web_method(plan, "abort", Builtin::AbortSignalAbort);
        }
        Layout::AbortSignalPrototype => {
            plan.constructor(Slot::AbortSignalConstructor)?;
            plan.tag("AbortSignal");
            web_method(
                plan,
                "throwIfAborted",
                Builtin::AbortSignalPrototypeThrowIfAborted,
            );
            web_getter(
                plan,
                IntrinsicKey::Name("aborted"),
                Builtin::AbortSignalPrototypeAbortedGetter,
            )?;
            web_getter(
                plan,
                IntrinsicKey::Name("reason"),
                Builtin::AbortSignalPrototypeReasonGetter,
            )?;
            plan.accessor_with_enumerability(
                IntrinsicKey::Name("onabort"),
                IntrinsicCallableRecipe::method(Builtin::AbortSignalPrototypeOnabortGetter)?,
                IntrinsicCallableRecipe::method(Builtin::AbortSignalPrototypeOnabortSetter)?,
                true,
            );
        }
        Layout::EventTargetConstructor => {}
        Layout::EventTargetPrototype => {
            plan.constructor(Slot::EventTargetConstructor)?;
            plan.tag("EventTarget");
            web_method(
                plan,
                "addEventListener",
                Builtin::EventTargetPrototypeAddEventListener,
            );
            web_method(
                plan,
                "removeEventListener",
                Builtin::EventTargetPrototypeRemoveEventListener,
            );
            web_method(
                plan,
                "dispatchEvent",
                Builtin::EventTargetPrototypeDispatchEvent,
            );
        }
        Layout::EventConstructor => {}
        Layout::EventPrototype => {
            plan.constructor(Slot::EventConstructor)?;
            plan.tag("Event");
            web_method(
                plan,
                "preventDefault",
                Builtin::EventPrototypePreventDefault,
            );
            web_method(
                plan,
                "stopPropagation",
                Builtin::EventPrototypeStopPropagation,
            );
            web_method(
                plan,
                "stopImmediatePropagation",
                Builtin::EventPrototypeStopImmediatePropagation,
            );
            web_getter(
                plan,
                IntrinsicKey::Name("type"),
                Builtin::EventPrototypeTypeGetter,
            )?;
            web_getter(
                plan,
                IntrinsicKey::Name("target"),
                Builtin::EventPrototypeTargetGetter,
            )?;
            web_getter(
                plan,
                IntrinsicKey::Name("currentTarget"),
                Builtin::EventPrototypeCurrentTargetGetter,
            )?;
            web_getter(
                plan,
                IntrinsicKey::Name("defaultPrevented"),
                Builtin::EventPrototypeDefaultPreventedGetter,
            )?;
            web_getter(
                plan,
                IntrinsicKey::Name("eventPhase"),
                Builtin::EventPrototypeEventPhaseGetter,
            )?;
            web_getter(
                plan,
                IntrinsicKey::Name("bubbles"),
                Builtin::EventPrototypeBubblesGetter,
            )?;
            web_getter(
                plan,
                IntrinsicKey::Name("cancelable"),
                Builtin::EventPrototypeCancelableGetter,
            )?;
            web_getter(
                plan,
                IntrinsicKey::Name("composed"),
                Builtin::EventPrototypeComposedGetter,
            )?;
            web_getter(
                plan,
                IntrinsicKey::Name("isTrusted"),
                Builtin::EventPrototypeIsTrustedGetter,
            )?;
        }
        Layout::DOMExceptionConstructor => {}
        Layout::DOMExceptionPrototype => {
            plan.constructor(Slot::DOMExceptionConstructor)?;
            plan.tag("DOMException");
            web_getter(
                plan,
                IntrinsicKey::Name("name"),
                Builtin::DOMExceptionPrototypeNameGetter,
            )?;
            web_getter(
                plan,
                IntrinsicKey::Name("message"),
                Builtin::DOMExceptionPrototypeMessageGetter,
            )?;
            web_getter(
                plan,
                IntrinsicKey::Name("code"),
                Builtin::DOMExceptionPrototypeCodeGetter,
            )?;
        }
        Layout::ShadowRealmPrototype => {
            plan.constructor(Slot::ShadowRealmConstructor)?;
            for &(name, builtin) in scriptw_ir::SHADOW_REALM_PROTOTYPE_METHODS {
                named_method(plan, name, builtin);
            }
            plan.tag("ShadowRealm");
        }
        Layout::ArrayIteratorPrototype => {
            named_method(plan, "next", Builtin::ArrayIteratorNext);
            symbol_method(
                plan,
                WellKnownSymbol::Iterator,
                Builtin::ArrayIteratorIdentity,
            );
            plan.tag("Array Iterator");
        }
        Layout::StringIteratorPrototype => {
            named_method(plan, "next", Builtin::StringIteratorNext);
            plan.tag("String Iterator");
        }
        Layout::RegExpStringIteratorPrototype => {
            named_method(plan, "next", Builtin::RegExpStringIteratorNext);
            plan.tag("RegExp String Iterator");
        }
        Layout::MapIteratorPrototype => {
            named_method(plan, "next", Builtin::MapIteratorNext);
            plan.tag("Map Iterator");
        }
        Layout::SetIteratorPrototype => {
            named_method(plan, "next", Builtin::SetIteratorNext);
            plan.tag("Set Iterator");
        }
        Layout::GeneratorPrototype => {
            generator_prototype(
                plan,
                Slot::GeneratorFunctionPrototype,
                [
                    Builtin::GeneratorPrototypeNext,
                    Builtin::GeneratorPrototypeReturn,
                    Builtin::GeneratorPrototypeThrow,
                ],
                "Generator",
            )?;
        }
        Layout::AsyncGeneratorPrototype => {
            generator_prototype(
                plan,
                Slot::AsyncGeneratorFunctionPrototype,
                [
                    Builtin::AsyncGeneratorPrototypeNext,
                    Builtin::AsyncGeneratorPrototypeReturn,
                    Builtin::AsyncGeneratorPrototypeThrow,
                ],
                "AsyncGenerator",
            )?;
        }
        Layout::AsyncIteratorPrototype => {
            symbol_method(
                plan,
                WellKnownSymbol::AsyncIterator,
                Builtin::AsyncIteratorIdentity,
            );
            symbol_method(
                plan,
                WellKnownSymbol::AsyncDispose,
                Builtin::AsyncIteratorPrototypeAsyncDispose,
            );
        }
        Layout::GeneratorFunctionPrototype => {
            function_prototype(
                plan,
                Some(Slot::GeneratorPrototype),
                Slot::GeneratorFunctionConstructor,
                "GeneratorFunction",
            )?;
        }
        Layout::AsyncFunctionPrototype => {
            function_prototype(plan, None, Slot::AsyncFunctionConstructor, "AsyncFunction")?;
        }
        Layout::AsyncGeneratorFunctionPrototype => {
            function_prototype(
                plan,
                Some(Slot::AsyncGeneratorPrototype),
                Slot::AsyncGeneratorFunctionConstructor,
                "AsyncGeneratorFunction",
            )?;
        }
        Layout::Reflect
        | Layout::Math
        | Layout::Json
        | Layout::Atomics
        | Layout::ObjectPrototype
        | Layout::ArrayPrototype
        | Layout::StringPrototype
        | Layout::ArrayUnscopables => {
            return Err(EmitError::unsupported(
                "existing intrinsic layout has its own complete owner",
            ));
        }
    }
    Ok(())
}

fn named_method(plan: &mut LayoutBuilder, name: &'static str, builtin: StandardBuiltinId) {
    plan.data(
        IntrinsicKey::Name(name),
        IntrinsicPropertyRecipe::Method(builtin),
        true,
        false,
        true,
    );
}

fn symbol_method(plan: &mut LayoutBuilder, symbol: WellKnownSymbol, builtin: StandardBuiltinId) {
    plan.data(
        IntrinsicKey::Symbol(symbol),
        IntrinsicPropertyRecipe::Method(builtin),
        true,
        false,
        true,
    );
}

fn getter(
    plan: &mut LayoutBuilder,
    key: IntrinsicKey<'static>,
    builtin: StandardBuiltinId,
) -> Result<(), EmitError> {
    plan.accessor(
        key,
        IntrinsicCallableRecipe::method(builtin)?,
        IntrinsicCallableRecipe::Undefined,
    );
    Ok(())
}

fn web_method(plan: &mut LayoutBuilder, name: &'static str, builtin: StandardBuiltinId) {
    plan.data(
        IntrinsicKey::Name(name),
        IntrinsicPropertyRecipe::Method(builtin),
        true,
        true,
        true,
    );
}

fn web_getter(
    plan: &mut LayoutBuilder,
    key: IntrinsicKey<'static>,
    builtin: StandardBuiltinId,
) -> Result<(), EmitError> {
    plan.accessor_with_enumerability(
        key,
        IntrinsicCallableRecipe::method(builtin)?,
        IntrinsicCallableRecipe::Undefined,
        true,
    );
    Ok(())
}

fn generator_prototype(
    plan: &mut LayoutBuilder,
    constructor: NonArrayRealmIntrinsicSlot,
    methods: [StandardBuiltinId; 3],
    tag: &'static str,
) -> Result<(), EmitError> {
    plan.data(
        IntrinsicKey::Name("constructor"),
        IntrinsicPropertyRecipe::ready(constructor)?,
        false,
        false,
        true,
    );
    for (name, builtin) in ["next", "return", "throw"].into_iter().zip(methods) {
        named_method(plan, name, builtin);
    }
    plan.tag(tag);
    Ok(())
}

fn function_prototype(
    plan: &mut LayoutBuilder,
    prototype: Option<NonArrayRealmIntrinsicSlot>,
    constructor: NonArrayRealmIntrinsicSlot,
    tag: &'static str,
) -> Result<(), EmitError> {
    if let Some(prototype) = prototype {
        plan.data(
            IntrinsicKey::Name("prototype"),
            IntrinsicPropertyRecipe::ready(prototype)?,
            false,
            false,
            true,
        );
    }
    plan.data(
        IntrinsicKey::Name("constructor"),
        IntrinsicPropertyRecipe::ready(constructor)?,
        false,
        false,
        true,
    );
    plan.tag(tag);
    Ok(())
}

/// Only populated eager prototypes own a complete fresh-Realm layout here.
/// Empty and demand-owned prototypes retain their existing graph owners.
pub(super) fn prototype_layout(
    slot: NonArrayRealmIntrinsicSlot,
) -> Option<IntrinsicPropertyLayoutId> {
    use IntrinsicPropertyLayoutId as Layout;
    use NonArrayRealmIntrinsicSlot as Slot;
    match slot {
        Slot::FunctionPrototype => Some(Layout::FunctionPrototype),
        Slot::NumberPrototype => Some(Layout::NumberPrototype),
        Slot::BooleanPrototype => Some(Layout::BooleanPrototype),
        Slot::BigIntPrototype => Some(Layout::BigIntPrototype),
        Slot::PromisePrototype => Some(Layout::PromisePrototype),
        Slot::MapPrototype => Some(Layout::MapPrototype),
        Slot::WeakMapPrototype => Some(Layout::WeakMapPrototype),
        Slot::WeakSetPrototype => Some(Layout::WeakSetPrototype),
        Slot::WeakRefPrototype => Some(Layout::WeakRefPrototype),
        Slot::FinalizationRegistryPrototype => Some(Layout::FinalizationRegistryPrototype),
        Slot::AsyncDisposableStackPrototype => Some(Layout::AsyncDisposableStackPrototype),
        Slot::DisposableStackPrototype => Some(Layout::DisposableStackPrototype),
        Slot::TextEncoderPrototype => Some(Layout::TextEncoderPrototype),
        Slot::TextDecoderPrototype => Some(Layout::TextDecoderPrototype),
        Slot::SetPrototype => Some(Layout::SetPrototype),
        Slot::SymbolPrototype => Some(Layout::SymbolPrototype),
        Slot::IteratorPrototype => Some(Layout::IteratorPrototype),
        Slot::IteratorFromWrapperPrototype => Some(Layout::IteratorFromWrapperPrototype),
        Slot::IteratorHelperPrototype => Some(Layout::IteratorHelperPrototype),
        Slot::AbstractModuleSourcePrototype => Some(Layout::AbstractModuleSourcePrototype),
        Slot::AbortControllerConstructor => Some(Layout::AbortControllerConstructor),
        Slot::AbortControllerPrototype => Some(Layout::AbortControllerPrototype),
        Slot::AbortSignalConstructor => Some(Layout::AbortSignalConstructor),
        Slot::AbortSignalPrototype => Some(Layout::AbortSignalPrototype),
        Slot::EventTargetConstructor => Some(Layout::EventTargetConstructor),
        Slot::EventTargetPrototype => Some(Layout::EventTargetPrototype),
        Slot::EventConstructor => Some(Layout::EventConstructor),
        Slot::EventPrototype => Some(Layout::EventPrototype),
        Slot::DOMExceptionConstructor => Some(Layout::DOMExceptionConstructor),
        Slot::DOMExceptionPrototype => Some(Layout::DOMExceptionPrototype),
        Slot::ReadableStreamPrototype => Some(Layout::ReadableStreamPrototype),
        Slot::ReadableStreamDefaultReaderPrototype => {
            Some(Layout::ReadableStreamDefaultReaderPrototype)
        }
        Slot::ReadableByteStreamControllerPrototype => {
            Some(Layout::ReadableByteStreamControllerPrototype)
        }
        Slot::ShadowRealmPrototype => Some(Layout::ShadowRealmPrototype),
        Slot::ArrayIteratorPrototype => Some(Layout::ArrayIteratorPrototype),
        Slot::StringIteratorPrototype => Some(Layout::StringIteratorPrototype),
        Slot::RegExpStringIteratorPrototype => Some(Layout::RegExpStringIteratorPrototype),
        Slot::MapIteratorPrototype => Some(Layout::MapIteratorPrototype),
        Slot::SetIteratorPrototype => Some(Layout::SetIteratorPrototype),
        Slot::GeneratorPrototype => Some(Layout::GeneratorPrototype),
        Slot::AsyncGeneratorPrototype => Some(Layout::AsyncGeneratorPrototype),
        Slot::AsyncIteratorPrototype => Some(Layout::AsyncIteratorPrototype),
        Slot::GeneratorFunctionPrototype => Some(Layout::GeneratorFunctionPrototype),
        Slot::AsyncFunctionPrototype => Some(Layout::AsyncFunctionPrototype),
        Slot::AsyncGeneratorFunctionPrototype => Some(Layout::AsyncGeneratorFunctionPrototype),
        Slot::ObjectPrototype => Some(Layout::ObjectPrototype),
        Slot::StringPrototype => Some(Layout::StringPrototype),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONSTRUCTORS: [(IntrinsicPropertyLayoutId, usize); 24] = [
        (IntrinsicPropertyLayoutId::FunctionConstructor, 3),
        (IntrinsicPropertyLayoutId::ObjectConstructor, 26),
        (
            IntrinsicPropertyLayoutId::AbstractModuleSourceConstructor,
            3,
        ),
        (IntrinsicPropertyLayoutId::ShadowRealmConstructor, 3),
        (IntrinsicPropertyLayoutId::ProxyConstructor, 3),
        (IntrinsicPropertyLayoutId::IteratorConstructor, 7),
        (IntrinsicPropertyLayoutId::ArrayConstructor, 8),
        (IntrinsicPropertyLayoutId::NumberConstructor, 17),
        (IntrinsicPropertyLayoutId::StringConstructor, 6),
        (IntrinsicPropertyLayoutId::BooleanConstructor, 3),
        (IntrinsicPropertyLayoutId::PromiseConstructor, 14),
        (IntrinsicPropertyLayoutId::MapConstructor, 5),
        (IntrinsicPropertyLayoutId::WeakMapConstructor, 3),
        (IntrinsicPropertyLayoutId::WeakRefConstructor, 3),
        (
            IntrinsicPropertyLayoutId::FinalizationRegistryConstructor,
            3,
        ),
        (IntrinsicPropertyLayoutId::WeakSetConstructor, 3),
        (
            IntrinsicPropertyLayoutId::AsyncDisposableStackConstructor,
            3,
        ),
        (IntrinsicPropertyLayoutId::DisposableStackConstructor, 3),
        (IntrinsicPropertyLayoutId::SetConstructor, 4),
        (IntrinsicPropertyLayoutId::SymbolConstructor, 20),
        (IntrinsicPropertyLayoutId::BigIntConstructor, 5),
        (IntrinsicPropertyLayoutId::GeneratorFunctionConstructor, 3),
        (IntrinsicPropertyLayoutId::AsyncFunctionConstructor, 3),
        (
            IntrinsicPropertyLayoutId::AsyncGeneratorFunctionConstructor,
            3,
        ),
    ];
    const PROTOTYPES: [(IntrinsicPropertyLayoutId, usize); 30] = [
        (IntrinsicPropertyLayoutId::FunctionPrototype, 10),
        (IntrinsicPropertyLayoutId::NumberPrototype, 7),
        (IntrinsicPropertyLayoutId::BooleanPrototype, 3),
        (IntrinsicPropertyLayoutId::BigIntPrototype, 5),
        (IntrinsicPropertyLayoutId::PromisePrototype, 5),
        (IntrinsicPropertyLayoutId::MapPrototype, 15),
        (IntrinsicPropertyLayoutId::WeakMapPrototype, 8),
        (IntrinsicPropertyLayoutId::WeakSetPrototype, 5),
        (IntrinsicPropertyLayoutId::WeakRefPrototype, 3),
        (IntrinsicPropertyLayoutId::FinalizationRegistryPrototype, 4),
        (IntrinsicPropertyLayoutId::AsyncDisposableStackPrototype, 9),
        (IntrinsicPropertyLayoutId::DisposableStackPrototype, 9),
        (IntrinsicPropertyLayoutId::SetPrototype, 19),
        (IntrinsicPropertyLayoutId::SymbolPrototype, 6),
        (IntrinsicPropertyLayoutId::IteratorPrototype, 19),
        (IntrinsicPropertyLayoutId::IteratorFromWrapperPrototype, 2),
        (IntrinsicPropertyLayoutId::IteratorHelperPrototype, 3),
        (IntrinsicPropertyLayoutId::AbstractModuleSourcePrototype, 2),
        (IntrinsicPropertyLayoutId::ShadowRealmPrototype, 4),
        (IntrinsicPropertyLayoutId::ArrayIteratorPrototype, 3),
        (IntrinsicPropertyLayoutId::StringIteratorPrototype, 2),
        (IntrinsicPropertyLayoutId::RegExpStringIteratorPrototype, 2),
        (IntrinsicPropertyLayoutId::MapIteratorPrototype, 2),
        (IntrinsicPropertyLayoutId::SetIteratorPrototype, 2),
        (IntrinsicPropertyLayoutId::GeneratorPrototype, 5),
        (IntrinsicPropertyLayoutId::AsyncGeneratorPrototype, 5),
        (IntrinsicPropertyLayoutId::AsyncIteratorPrototype, 2),
        (IntrinsicPropertyLayoutId::GeneratorFunctionPrototype, 3),
        (IntrinsicPropertyLayoutId::AsyncFunctionPrototype, 2),
        (
            IntrinsicPropertyLayoutId::AsyncGeneratorFunctionPrototype,
            3,
        ),
    ];

    #[test]
    fn every_eager_object_has_its_complete_ordered_property_count() {
        let mut ids = BTreeSet::new();
        for (cases, expected_rows) in [(&CONSTRUCTORS[..], 154), (&PROTOTYPES[..], 169)] {
            let mut rows = 0;
            for &(id, expected) in cases {
                assert!(ids.insert(id), "duplicate complete layout {id:?}");
                let layout = id.layout().unwrap();
                assert_eq!(layout.properties().len(), expected, "{id:?}");
                rows += layout.properties().len();
            }
            assert_eq!(rows, expected_rows);
        }
        assert_eq!(ids.len(), 54);
    }

    fn named_index(layout: &ValidatedIntrinsicPropertyLayout, name: &str) -> usize {
        layout
            .properties()
            .iter()
            .position(|row| matches!(row.key, IntrinsicKey::Name(actual) if actual == name))
            .unwrap()
    }

    fn symbol_index(layout: &ValidatedIntrinsicPropertyLayout, symbol: WellKnownSymbol) -> usize {
        layout
            .properties()
            .iter()
            .position(|row| matches!(row.key, IntrinsicKey::Symbol(actual) if actual == symbol))
            .unwrap()
    }

    #[test]
    fn constructor_metadata_precedes_public_members_and_proxy_omits_prototype() {
        for (id, _) in CONSTRUCTORS {
            let layout = id.layout().unwrap();
            assert_eq!(named_index(&layout, "length"), 0, "{id:?}");
            assert_eq!(named_index(&layout, "name"), 1, "{id:?}");
            if id == IntrinsicPropertyLayoutId::ProxyConstructor {
                assert!(!layout
                    .properties()
                    .iter()
                    .any(|row| matches!(row.key, IntrinsicKey::Name("prototype"))));
            } else {
                assert_eq!(named_index(&layout, "prototype"), 2, "{id:?}");
            }
        }
    }

    #[test]
    fn collection_and_disposal_aliases_share_one_local_identity_group() {
        for (id, names, symbol) in [
            (
                IntrinsicPropertyLayoutId::MapPrototype,
                &["entries"][..],
                WellKnownSymbol::Iterator,
            ),
            (
                IntrinsicPropertyLayoutId::SetPrototype,
                &["values", "keys"][..],
                WellKnownSymbol::Iterator,
            ),
            (
                IntrinsicPropertyLayoutId::DisposableStackPrototype,
                &["dispose"][..],
                WellKnownSymbol::Dispose,
            ),
            (
                IntrinsicPropertyLayoutId::AsyncDisposableStackPrototype,
                &["disposeAsync"][..],
                WellKnownSymbol::AsyncDispose,
            ),
        ] {
            let layout = id.layout().unwrap();
            let group = layout.properties()[symbol_index(&layout, symbol)].group;
            for name in names {
                assert_eq!(
                    layout.properties()[named_index(&layout, name)].group,
                    group,
                    "{id:?}"
                );
            }
        }
    }

    #[test]
    fn prototype_membership_rejects_empty_and_demand_owned_families() {
        for slot in [
            NonArrayRealmIntrinsicSlot::ModuleSourcePrototype,
            NonArrayRealmIntrinsicSlot::RegExpPrototype,
            NonArrayRealmIntrinsicSlot::DatePrototype,
            NonArrayRealmIntrinsicSlot::ErrorPrototype,
            NonArrayRealmIntrinsicSlot::ArrayBufferPrototype,
            NonArrayRealmIntrinsicSlot::TypedArrayPrototype,
        ] {
            assert!(prototype_layout(slot).is_none());
        }
    }
}
