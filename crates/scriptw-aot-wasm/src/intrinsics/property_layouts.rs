//! Complete initial ordinary-property layouts. These are compiler metadata,
//! not JavaScript instructions; mutation leaves them for the ordinary dictionary.
use super::DeferredIntrinsicMethod;
use super::{IntrinsicKey, IntrinsicMethodPlan, IntrinsicMethodSpec};
use crate::emitted_function::FunctionIdentity;
use crate::functions::NonArrayRealmIntrinsicSlot;
use crate::heap::{DescriptorWord, StoredPropertyAttributes};
use crate::planning::WasmFunctionMeta;
use crate::EmitError;
use scriptw_ir::{FunctionId, HostBuiltinId, StandardBuiltinId, WellKnownSymbol};

mod eager;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub(crate) enum IntrinsicPropertyLayoutId {
    Reflect,
    Math,
    Json,
    Atomics,
    ObjectPrototype,
    ArrayPrototype,
    StringPrototype,
    ArrayUnscopables,
    FunctionConstructor,
    ObjectConstructor,
    AbstractModuleSourceConstructor,
    AbortControllerConstructor,
    AbortControllerPrototype,
    AbortSignalConstructor,
    AbortSignalPrototype,
    EventTargetConstructor,
    EventTargetPrototype,
    EventConstructor,
    EventPrototype,
    DOMExceptionConstructor,
    DOMExceptionPrototype,
    ReadableStreamConstructor,
    ReadableStreamPrototype,
    ReadableStreamDefaultReaderConstructor,
    ReadableStreamDefaultReaderPrototype,
    ReadableByteStreamControllerConstructor,
    ReadableByteStreamControllerPrototype,
    ShadowRealmConstructor,
    ProxyConstructor,
    IteratorConstructor,
    ArrayConstructor,
    NumberConstructor,
    StringConstructor,
    BooleanConstructor,
    PromiseConstructor,
    MapConstructor,
    WeakMapConstructor,
    WeakRefConstructor,
    FinalizationRegistryConstructor,
    WeakSetConstructor,
    AsyncDisposableStackConstructor,
    DisposableStackConstructor,
    TextEncoderConstructor,
    TextDecoderConstructor,
    TextEncoderPrototype,
    TextDecoderPrototype,
    SetConstructor,
    SymbolConstructor,
    BigIntConstructor,
    GeneratorFunctionConstructor,
    AsyncFunctionConstructor,
    AsyncGeneratorFunctionConstructor,
    FunctionPrototype,
    NumberPrototype,
    BooleanPrototype,
    BigIntPrototype,
    SymbolPrototype,
    PromisePrototype,
    MapPrototype,
    SetPrototype,
    WeakMapPrototype,
    WeakSetPrototype,
    WeakRefPrototype,
    FinalizationRegistryPrototype,
    DisposableStackPrototype,
    AsyncDisposableStackPrototype,
    IteratorPrototype,
    IteratorHelperPrototype,
    IteratorFromWrapperPrototype,
    ArrayIteratorPrototype,
    StringIteratorPrototype,
    RegExpStringIteratorPrototype,
    MapIteratorPrototype,
    SetIteratorPrototype,
    GeneratorPrototype,
    AsyncGeneratorPrototype,
    AsyncIteratorPrototype,
    GeneratorFunctionPrototype,
    AsyncFunctionPrototype,
    AsyncGeneratorFunctionPrototype,
    AbstractModuleSourcePrototype,
    ShadowRealmPrototype,
}

impl IntrinsicPropertyLayoutId {
    pub(crate) const ALL: [Self; 82] = [
        Self::Reflect,
        Self::Math,
        Self::Json,
        Self::Atomics,
        Self::ObjectPrototype,
        Self::ArrayPrototype,
        Self::StringPrototype,
        Self::ArrayUnscopables,
        Self::FunctionConstructor,
        Self::ObjectConstructor,
        Self::AbstractModuleSourceConstructor,
        Self::AbortControllerConstructor,
        Self::AbortControllerPrototype,
        Self::AbortSignalConstructor,
        Self::AbortSignalPrototype,
        Self::EventTargetConstructor,
        Self::EventTargetPrototype,
        Self::EventConstructor,
        Self::EventPrototype,
        Self::DOMExceptionConstructor,
        Self::DOMExceptionPrototype,
        Self::ReadableStreamConstructor,
        Self::ReadableStreamPrototype,
        Self::ReadableStreamDefaultReaderConstructor,
        Self::ReadableStreamDefaultReaderPrototype,
        Self::ReadableByteStreamControllerConstructor,
        Self::ReadableByteStreamControllerPrototype,
        Self::ShadowRealmConstructor,
        Self::ProxyConstructor,
        Self::IteratorConstructor,
        Self::ArrayConstructor,
        Self::NumberConstructor,
        Self::StringConstructor,
        Self::BooleanConstructor,
        Self::PromiseConstructor,
        Self::MapConstructor,
        Self::WeakMapConstructor,
        Self::WeakRefConstructor,
        Self::FinalizationRegistryConstructor,
        Self::WeakSetConstructor,
        Self::AsyncDisposableStackConstructor,
        Self::DisposableStackConstructor,
        Self::TextEncoderConstructor,
        Self::TextDecoderConstructor,
        Self::TextEncoderPrototype,
        Self::TextDecoderPrototype,
        Self::SetConstructor,
        Self::SymbolConstructor,
        Self::BigIntConstructor,
        Self::GeneratorFunctionConstructor,
        Self::AsyncFunctionConstructor,
        Self::AsyncGeneratorFunctionConstructor,
        Self::FunctionPrototype,
        Self::NumberPrototype,
        Self::BooleanPrototype,
        Self::BigIntPrototype,
        Self::SymbolPrototype,
        Self::PromisePrototype,
        Self::MapPrototype,
        Self::SetPrototype,
        Self::WeakMapPrototype,
        Self::WeakSetPrototype,
        Self::WeakRefPrototype,
        Self::FinalizationRegistryPrototype,
        Self::DisposableStackPrototype,
        Self::AsyncDisposableStackPrototype,
        Self::IteratorPrototype,
        Self::IteratorHelperPrototype,
        Self::IteratorFromWrapperPrototype,
        Self::ArrayIteratorPrototype,
        Self::StringIteratorPrototype,
        Self::RegExpStringIteratorPrototype,
        Self::MapIteratorPrototype,
        Self::SetIteratorPrototype,
        Self::GeneratorPrototype,
        Self::AsyncGeneratorPrototype,
        Self::AsyncIteratorPrototype,
        Self::GeneratorFunctionPrototype,
        Self::AsyncFunctionPrototype,
        Self::AsyncGeneratorFunctionPrototype,
        Self::AbstractModuleSourcePrototype,
        Self::ShadowRealmPrototype,
    ];

    fn native_identity(self) -> Option<FunctionIdentity> {
        match self {
            Self::FunctionConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::FunctionConstructor,
            )),
            Self::ObjectConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::ObjectConstructor,
            )),
            Self::AbstractModuleSourceConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::AbstractModuleSourceConstructor,
            )),
            Self::AbortControllerConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::AbortControllerConstructor,
            )),
            Self::AbortSignalConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::AbortSignalConstructor,
            )),
            Self::EventTargetConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::EventTargetConstructor,
            )),
            Self::EventConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::EventConstructor,
            )),
            Self::DOMExceptionConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::DOMExceptionConstructor,
            )),
            Self::ReadableStreamConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::ReadableStreamConstructor,
            )),
            Self::ReadableStreamDefaultReaderConstructor => {
                Some(FunctionIdentity::StandardBuiltin(
                    StandardBuiltinId::ReadableStreamDefaultReaderConstructor,
                ))
            }
            Self::ReadableByteStreamControllerConstructor => {
                Some(FunctionIdentity::StandardBuiltin(
                    StandardBuiltinId::ReadableByteStreamControllerConstructor,
                ))
            }
            Self::ShadowRealmConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::ShadowRealmConstructor,
            )),
            Self::ProxyConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::ProxyConstructor,
            )),
            Self::IteratorConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::IteratorConstructor,
            )),
            Self::ArrayConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::ArrayConstructor,
            )),
            Self::NumberConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::NumberConstructor,
            )),
            Self::StringConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::StringConstructor,
            )),
            Self::BooleanConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::BooleanConstructor,
            )),
            Self::PromiseConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::PromiseConstructor,
            )),
            Self::MapConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::MapConstructor,
            )),
            Self::WeakMapConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::WeakMapConstructor,
            )),
            Self::WeakRefConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::WeakRefConstructor,
            )),
            Self::FinalizationRegistryConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::FinalizationRegistryConstructor,
            )),
            Self::WeakSetConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::WeakSetConstructor,
            )),
            Self::AsyncDisposableStackConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::AsyncDisposableStackConstructor,
            )),
            Self::DisposableStackConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::DisposableStackConstructor,
            )),
            Self::TextEncoderConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::TextEncoderConstructor,
            )),
            Self::TextDecoderConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::TextDecoderConstructor,
            )),
            Self::SetConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::SetConstructor,
            )),
            Self::SymbolConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::SymbolConstructor,
            )),
            Self::BigIntConstructor => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::BigIntConstructor,
            )),
            Self::GeneratorFunctionConstructor => Some(FunctionIdentity::HostBuiltin(
                HostBuiltinId::GeneratorFunctionConstructor,
            )),
            Self::AsyncFunctionConstructor => Some(FunctionIdentity::HostBuiltin(
                HostBuiltinId::AsyncFunctionConstructor,
            )),
            Self::AsyncGeneratorFunctionConstructor => Some(FunctionIdentity::HostBuiltin(
                HostBuiltinId::AsyncGeneratorFunctionConstructor,
            )),
            Self::FunctionPrototype => Some(FunctionIdentity::StandardBuiltin(
                StandardBuiltinId::FunctionPrototype,
            )),
            _ => None,
        }
    }

    pub(crate) fn native_function_id(self) -> Option<FunctionId> {
        match self.native_identity()? {
            FunctionIdentity::StandardBuiltin(builtin) => Some(builtin.function_id()),
            FunctionIdentity::HostBuiltin(builtin) => Some(builtin.function_id()),
            _ => unreachable!("native layout identity is a native builtin"),
        }
    }

    pub(crate) fn validate_native_metadata(self, meta: &WasmFunctionMeta) -> Result<(), EmitError> {
        let expected = self.native_identity().ok_or_else(|| {
            EmitError::unsupported("property layout does not own native metadata")
        })?;
        let metadata_matches = match &expected {
            FunctionIdentity::StandardBuiltin(builtin) => {
                meta.standard_builtin == Some(*builtin) && meta.host_builtin.is_none()
            }
            FunctionIdentity::HostBuiltin(builtin) => {
                meta.host_builtin == Some(*builtin) && meta.standard_builtin.is_none()
            }
            _ => unreachable!("closed native layout identity"),
        };
        if meta.entry.identity() != expected
            || !metadata_matches
            || !meta.strict
            || !meta.length_name_configurable
        {
            return Err(EmitError::unsupported(
                "complete native layout metadata differs from its admitted native identity",
            ));
        }
        Ok(())
    }

    pub(crate) fn for_constructor(meta: &WasmFunctionMeta) -> Result<Option<Self>, EmitError> {
        for id in Self::ALL {
            if id != Self::FunctionPrototype && id.native_identity() == Some(meta.entry.identity())
            {
                id.validate_native_metadata(meta)?;
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    pub(crate) fn for_prototype(slot: NonArrayRealmIntrinsicSlot) -> Option<Self> {
        eager::prototype_layout(slot)
    }

    pub(crate) fn layout(self) -> Result<ValidatedIntrinsicPropertyLayout, EmitError> {
        let mut plan = LayoutBuilder::default();
        match self {
            Self::Reflect => {
                plan.methods(IntrinsicMethodPlan::Reflect)?;
                plan.tag("Reflect");
            }
            Self::Math => {
                for (name, number) in [
                    ("E", std::f64::consts::E),
                    ("LN10", std::f64::consts::LN_10),
                    ("LN2", std::f64::consts::LN_2),
                    ("LOG10E", std::f64::consts::LOG10_E),
                    ("LOG2E", std::f64::consts::LOG2_E),
                    ("PI", std::f64::consts::PI),
                    ("SQRT1_2", std::f64::consts::FRAC_1_SQRT_2),
                    ("SQRT2", std::f64::consts::SQRT_2),
                ] {
                    plan.data(
                        IntrinsicKey::Name(name),
                        IntrinsicPropertyRecipe::Number(number.to_bits()),
                        false,
                        false,
                        false,
                    );
                }
                plan.methods(IntrinsicMethodPlan::Math)?;
                plan.tag("Math");
            }
            Self::Json => {
                plan.tag("JSON");
                plan.methods(IntrinsicMethodPlan::Json)?;
            }
            Self::Atomics => {
                plan.tag("Atomics");
                for builtin in crate::builtins::ATOMICS_PUBLICATION_ORDER {
                    let name = builtin.native_function_name().ok_or_else(|| {
                        EmitError::unsupported("Atomics layout method has no native name")
                    })?;
                    plan.method(IntrinsicMethodSpec {
                        key: IntrinsicKey::Name(name),
                        builtin,
                        writable: true,
                        configurable: true,
                    })?;
                }
            }
            Self::ObjectPrototype => {
                plan.constructor(NonArrayRealmIntrinsicSlot::ObjectConstructor)?;
                plan.methods(IntrinsicMethodPlan::ObjectPrototypeBeforeAccessor)?;
                plan.accessor(
                    IntrinsicKey::Name("__proto__"),
                    IntrinsicCallableRecipe::method(StandardBuiltinId::ObjectPrototypeProtoGetter)?,
                    IntrinsicCallableRecipe::method(StandardBuiltinId::ObjectPrototypeProtoSetter)?,
                );
                plan.methods(IntrinsicMethodPlan::ObjectPrototypeAfterAccessor)?;
            }
            Self::ArrayPrototype => {
                // Array length remains in the Array exotic storage.
                plan.constructor(NonArrayRealmIntrinsicSlot::ArrayConstructor)?;
                plan.methods(IntrinsicMethodPlan::ArrayPrototype)?;
                plan.aliases(
                    &[
                        IntrinsicKey::Name("values"),
                        IntrinsicKey::Symbol(WellKnownSymbol::Iterator),
                    ],
                    IntrinsicPropertyRecipe::ready(
                        NonArrayRealmIntrinsicSlot::ArrayPrototypeValues,
                    )?,
                );
                plan.data(
                    IntrinsicKey::Symbol(WellKnownSymbol::Unscopables),
                    IntrinsicPropertyRecipe::LayoutObject(Self::ArrayUnscopables),
                    false,
                    false,
                    true,
                );
            }
            Self::StringPrototype => {
                // The current boxed String prototype owns length before constructor.
                plan.data(
                    IntrinsicKey::Name("length"),
                    IntrinsicPropertyRecipe::Number(0.0f64.to_bits()),
                    false,
                    false,
                    false,
                );
                plan.constructor(NonArrayRealmIntrinsicSlot::StringConstructor)?;
                plan.methods(IntrinsicMethodPlan::StringPrototypeBeforeAliases)?;
                for (names, builtin) in [
                    (
                        ["trimStart", "trimLeft"],
                        StandardBuiltinId::StringPrototypeTrimStart,
                    ),
                    (
                        ["trimEnd", "trimRight"],
                        StandardBuiltinId::StringPrototypeTrimEnd,
                    ),
                ] {
                    plan.aliases(
                        &[IntrinsicKey::Name(names[0]), IntrinsicKey::Name(names[1])],
                        IntrinsicPropertyRecipe::Method(builtin),
                    );
                }
                plan.methods(IntrinsicMethodPlan::StringPrototypeAfterAliases)?;
            }
            Self::ArrayUnscopables => {
                for name in [
                    "at",
                    "copyWithin",
                    "entries",
                    "fill",
                    "find",
                    "findIndex",
                    "findLast",
                    "findLastIndex",
                    "flat",
                    "flatMap",
                    "includes",
                    "keys",
                    "toReversed",
                    "toSorted",
                    "toSpliced",
                    "values",
                ] {
                    plan.data(
                        IntrinsicKey::Name(name),
                        IntrinsicPropertyRecipe::Boolean(true),
                        true,
                        true,
                        true,
                    );
                }
            }
            Self::FunctionConstructor
            | Self::ObjectConstructor
            | Self::AbstractModuleSourceConstructor
            | Self::AbortControllerConstructor
            | Self::AbortControllerPrototype
            | Self::AbortSignalConstructor
            | Self::AbortSignalPrototype
            | Self::EventTargetConstructor
            | Self::EventTargetPrototype
            | Self::EventConstructor
            | Self::EventPrototype
            | Self::DOMExceptionConstructor
            | Self::DOMExceptionPrototype
            | Self::ShadowRealmConstructor
            | Self::ProxyConstructor
            | Self::IteratorConstructor
            | Self::ArrayConstructor
            | Self::NumberConstructor
            | Self::StringConstructor
            | Self::BooleanConstructor
            | Self::PromiseConstructor
            | Self::MapConstructor
            | Self::WeakMapConstructor
            | Self::WeakRefConstructor
            | Self::FinalizationRegistryConstructor
            | Self::WeakSetConstructor
            | Self::AsyncDisposableStackConstructor
            | Self::DisposableStackConstructor
            | Self::TextEncoderConstructor
            | Self::TextEncoderPrototype
            | Self::TextDecoderConstructor
            | Self::TextDecoderPrototype
            | Self::SetConstructor
            | Self::SymbolConstructor
            | Self::BigIntConstructor
            | Self::GeneratorFunctionConstructor
            | Self::AsyncFunctionConstructor
            | Self::AsyncGeneratorFunctionConstructor
            | Self::FunctionPrototype
            | Self::NumberPrototype
            | Self::BooleanPrototype
            | Self::BigIntPrototype
            | Self::SymbolPrototype
            | Self::PromisePrototype
            | Self::MapPrototype
            | Self::SetPrototype
            | Self::WeakMapPrototype
            | Self::WeakSetPrototype
            | Self::WeakRefPrototype
            | Self::FinalizationRegistryPrototype
            | Self::DisposableStackPrototype
            | Self::AsyncDisposableStackPrototype
            | Self::IteratorPrototype
            | Self::IteratorHelperPrototype
            | Self::IteratorFromWrapperPrototype
            | Self::ArrayIteratorPrototype
            | Self::StringIteratorPrototype
            | Self::RegExpStringIteratorPrototype
            | Self::MapIteratorPrototype
            | Self::SetIteratorPrototype
            | Self::GeneratorPrototype
            | Self::AsyncGeneratorPrototype
            | Self::AsyncIteratorPrototype
            | Self::GeneratorFunctionPrototype
            | Self::AsyncFunctionPrototype
            | Self::AsyncGeneratorFunctionPrototype
            | Self::AbstractModuleSourcePrototype
            | Self::ReadableStreamConstructor
            | Self::ReadableStreamPrototype
            | Self::ReadableStreamDefaultReaderConstructor
            | Self::ReadableStreamDefaultReaderPrototype
            | Self::ReadableByteStreamControllerConstructor
            | Self::ReadableByteStreamControllerPrototype
            | Self::ShadowRealmPrototype => eager::populate(self, &mut plan)?,
        }
        ValidatedIntrinsicPropertyLayout::new(plan.properties, plan.groups)
    }
}

/// Only eagerly completed canonical slots can be cached without a demand gate.
#[derive(Clone, Copy)]
pub(crate) struct ReadyRealmSlot(NonArrayRealmIntrinsicSlot);
impl ReadyRealmSlot {
    fn new(slot: NonArrayRealmIntrinsicSlot) -> Result<Self, EmitError> {
        use NonArrayRealmIntrinsicSlot as Slot;
        match slot {
            Slot::AbortControllerConstructor
            | Slot::AbortControllerPrototype
            | Slot::AbortSignalConstructor
            | Slot::AbortSignalPrototype
            | Slot::EventTargetConstructor
            | Slot::EventTargetPrototype
            | Slot::EventConstructor
            | Slot::EventPrototype
            | Slot::DOMExceptionConstructor
            | Slot::DOMExceptionPrototype
            | Slot::ReadableStreamConstructor
            | Slot::ReadableStreamPrototype
            | Slot::ReadableStreamDefaultReaderConstructor
            | Slot::ReadableStreamDefaultReaderPrototype
            | Slot::ReadableByteStreamControllerConstructor
            | Slot::ReadableByteStreamControllerPrototype
            | Slot::ShadowRealmConstructor
            | Slot::ShadowRealmPrototype
            | Slot::AbstractModuleSourceConstructor
            | Slot::AbstractModuleSourcePrototype
            | Slot::ModuleSourcePrototype
            | Slot::ArrayPrototypeValues
            | Slot::ThrowTypeError
            | Slot::PromisePrototype
            | Slot::FunctionPrototype
            | Slot::PromiseConstructor
            | Slot::AsyncDisposableStackPrototype
            | Slot::DisposableStackPrototype
            | Slot::TextEncoderPrototype
            | Slot::TextDecoderPrototype
            | Slot::GeneratorFunctionConstructor
            | Slot::AsyncFunctionConstructor
            | Slot::AsyncGeneratorFunctionConstructor
            | Slot::ObjectPrototype
            | Slot::ArrayIteratorPrototype
            | Slot::StringIteratorPrototype
            | Slot::RegExpStringIteratorPrototype
            | Slot::MapIteratorPrototype
            | Slot::SetIteratorPrototype
            | Slot::IteratorHelperPrototype
            | Slot::IteratorPrototype
            | Slot::IteratorFromWrapperPrototype
            | Slot::GeneratorPrototype
            | Slot::GeneratorFunctionPrototype
            | Slot::AsyncIteratorPrototype
            | Slot::AsyncFunctionPrototype
            | Slot::AsyncGeneratorPrototype
            | Slot::AsyncGeneratorFunctionPrototype
            | Slot::NumberPrototype
            | Slot::StringPrototype
            | Slot::BooleanPrototype
            | Slot::SymbolPrototype
            | Slot::BigIntPrototype
            | Slot::MapPrototype
            | Slot::SetPrototype
            | Slot::WeakMapPrototype
            | Slot::WeakSetPrototype
            | Slot::WeakRefPrototype
            | Slot::FinalizationRegistryPrototype
            | Slot::ObjectConstructor
            | Slot::FunctionConstructor
            | Slot::ProxyConstructor
            | Slot::IteratorConstructor
            | Slot::ArrayConstructor
            | Slot::NumberConstructor
            | Slot::StringConstructor
            | Slot::BooleanConstructor
            | Slot::SymbolConstructor
            | Slot::BigIntConstructor
            | Slot::MapConstructor
            | Slot::SetConstructor
            | Slot::WeakMapConstructor
            | Slot::WeakSetConstructor
            | Slot::WeakRefConstructor
            | Slot::FinalizationRegistryConstructor
            | Slot::AsyncDisposableStackConstructor
            | Slot::DisposableStackConstructor
            | Slot::TextEncoderConstructor
            | Slot::TextDecoderConstructor
            | Slot::TypedArrayPrototypeToString
            | Slot::ParseInt
            | Slot::ParseFloat => Ok(Self(slot)),
            _ => Err(EmitError::unsupported(
                "intrinsic layout requires an eagerly completed Realm slot",
            )),
        }
    }
    pub(crate) fn gc_index(self) -> u32 {
        self.0.gc_index()
    }
    pub(crate) fn function_id(self) -> Option<FunctionId> {
        use NonArrayRealmIntrinsicSlot as Slot;
        match self.0 {
            Slot::AbortControllerConstructor => {
                Some(StandardBuiltinId::AbortControllerConstructor.function_id())
            }
            Slot::AbortSignalConstructor => {
                Some(StandardBuiltinId::AbortSignalConstructor.function_id())
            }
            Slot::EventTargetConstructor => {
                Some(StandardBuiltinId::EventTargetConstructor.function_id())
            }
            Slot::EventConstructor => Some(StandardBuiltinId::EventConstructor.function_id()),
            Slot::DOMExceptionConstructor => {
                Some(StandardBuiltinId::DOMExceptionConstructor.function_id())
            }
            Slot::ReadableStreamConstructor => {
                Some(StandardBuiltinId::ReadableStreamConstructor.function_id())
            }
            Slot::ReadableStreamDefaultReaderConstructor => {
                Some(StandardBuiltinId::ReadableStreamDefaultReaderConstructor.function_id())
            }
            Slot::ReadableByteStreamControllerConstructor => {
                Some(StandardBuiltinId::ReadableByteStreamControllerConstructor.function_id())
            }
            Slot::ShadowRealmConstructor => {
                Some(StandardBuiltinId::ShadowRealmConstructor.function_id())
            }
            Slot::AbstractModuleSourceConstructor => {
                Some(StandardBuiltinId::AbstractModuleSourceConstructor.function_id())
            }
            Slot::PromiseConstructor => Some(StandardBuiltinId::PromiseConstructor.function_id()),
            Slot::GeneratorFunctionConstructor => {
                Some(HostBuiltinId::GeneratorFunctionConstructor.function_id())
            }
            Slot::AsyncFunctionConstructor => {
                Some(HostBuiltinId::AsyncFunctionConstructor.function_id())
            }
            Slot::AsyncGeneratorFunctionConstructor => {
                Some(HostBuiltinId::AsyncGeneratorFunctionConstructor.function_id())
            }
            Slot::ObjectConstructor => Some(StandardBuiltinId::ObjectConstructor.function_id()),
            Slot::FunctionConstructor => Some(StandardBuiltinId::FunctionConstructor.function_id()),
            Slot::ProxyConstructor => Some(StandardBuiltinId::ProxyConstructor.function_id()),
            Slot::IteratorConstructor => Some(StandardBuiltinId::IteratorConstructor.function_id()),
            Slot::ArrayConstructor => Some(StandardBuiltinId::ArrayConstructor.function_id()),
            Slot::NumberConstructor => Some(StandardBuiltinId::NumberConstructor.function_id()),
            Slot::StringConstructor => Some(StandardBuiltinId::StringConstructor.function_id()),
            Slot::BooleanConstructor => Some(StandardBuiltinId::BooleanConstructor.function_id()),
            Slot::SymbolConstructor => Some(StandardBuiltinId::SymbolConstructor.function_id()),
            Slot::BigIntConstructor => Some(StandardBuiltinId::BigIntConstructor.function_id()),
            Slot::MapConstructor => Some(StandardBuiltinId::MapConstructor.function_id()),
            Slot::SetConstructor => Some(StandardBuiltinId::SetConstructor.function_id()),
            Slot::WeakMapConstructor => Some(StandardBuiltinId::WeakMapConstructor.function_id()),
            Slot::WeakSetConstructor => Some(StandardBuiltinId::WeakSetConstructor.function_id()),
            Slot::WeakRefConstructor => Some(StandardBuiltinId::WeakRefConstructor.function_id()),
            Slot::FinalizationRegistryConstructor => {
                Some(StandardBuiltinId::FinalizationRegistryConstructor.function_id())
            }
            Slot::AsyncDisposableStackConstructor => {
                Some(StandardBuiltinId::AsyncDisposableStackConstructor.function_id())
            }
            Slot::DisposableStackConstructor => {
                Some(StandardBuiltinId::DisposableStackConstructor.function_id())
            }
            Slot::TextEncoderConstructor => {
                Some(StandardBuiltinId::TextEncoderConstructor.function_id())
            }
            Slot::TextDecoderConstructor => {
                Some(StandardBuiltinId::TextDecoderConstructor.function_id())
            }
            Slot::FunctionPrototype => Some(StandardBuiltinId::FunctionPrototype.function_id()),
            Slot::ArrayPrototypeValues => {
                Some(StandardBuiltinId::ArrayPrototypeValues.function_id())
            }
            Slot::ThrowTypeError => Some(StandardBuiltinId::ThrowTypeError.function_id()),
            Slot::TypedArrayPrototypeToString => {
                Some(StandardBuiltinId::TypedArrayPrototypeToString.function_id())
            }
            Slot::ParseInt => Some(HostBuiltinId::ParseInt.function_id()),
            Slot::ParseFloat => Some(HostBuiltinId::ParseFloat.function_id()),
            _ => None,
        }
    }
}

/// Accessor values additionally require a canonical callable, never an object.
#[derive(Clone, Copy)]
pub(crate) struct ReadyRealmCallableSlot(ReadyRealmSlot);
impl ReadyRealmCallableSlot {
    fn new(slot: NonArrayRealmIntrinsicSlot) -> Result<Self, EmitError> {
        use NonArrayRealmIntrinsicSlot as Slot;
        match slot {
            Slot::ShadowRealmConstructor
            | Slot::AbstractModuleSourceConstructor
            | Slot::PromiseConstructor
            | Slot::GeneratorFunctionConstructor
            | Slot::AsyncFunctionConstructor
            | Slot::AsyncGeneratorFunctionConstructor
            | Slot::ObjectConstructor
            | Slot::FunctionConstructor
            | Slot::ProxyConstructor
            | Slot::IteratorConstructor
            | Slot::ArrayConstructor
            | Slot::NumberConstructor
            | Slot::StringConstructor
            | Slot::BooleanConstructor
            | Slot::SymbolConstructor
            | Slot::BigIntConstructor
            | Slot::MapConstructor
            | Slot::SetConstructor
            | Slot::WeakMapConstructor
            | Slot::WeakSetConstructor
            | Slot::WeakRefConstructor
            | Slot::FinalizationRegistryConstructor
            | Slot::AsyncDisposableStackConstructor
            | Slot::DisposableStackConstructor
            | Slot::TextEncoderConstructor
            | Slot::TextDecoderConstructor
            | Slot::FunctionPrototype
            | Slot::ArrayPrototypeValues
            | Slot::ThrowTypeError
            | Slot::TypedArrayPrototypeToString
            | Slot::ParseInt
            | Slot::ParseFloat => Ok(Self(ReadyRealmSlot::new(slot)?)),
            _ => Err(EmitError::unsupported(
                "intrinsic accessor requires an eagerly completed callable slot",
            )),
        }
    }
    pub(crate) fn gc_index(self) -> u32 {
        self.0.gc_index()
    }
    pub(crate) fn function_id(self) -> FunctionId {
        self.0
            .function_id()
            .expect("callable ready slots own native function identities")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum IntrinsicCallableRecipeKind {
    Undefined,
    Method,
    Ready,
}
impl IntrinsicCallableRecipeKind {
    pub(crate) const ALL: [Self; 3] = [Self::Undefined, Self::Method, Self::Ready];
}

#[derive(Clone, Copy)]
pub(crate) enum IntrinsicCallableRecipe {
    Undefined,
    Method(DeferredIntrinsicMethod),
    Ready(ReadyRealmCallableSlot),
}
impl IntrinsicCallableRecipe {
    pub(crate) fn method(builtin: StandardBuiltinId) -> Result<Self, EmitError> {
        DeferredIntrinsicMethod::new(builtin)
            .map(Self::Method)
            .ok_or_else(|| {
                EmitError::unsupported("intrinsic accessor method must be a noncanonical native")
            })
    }
    pub(crate) fn ready(slot: NonArrayRealmIntrinsicSlot) -> Result<Self, EmitError> {
        Ok(Self::Ready(ReadyRealmCallableSlot::new(slot)?))
    }
    pub(crate) fn words(self) -> (IntrinsicCallableRecipeKind, u32) {
        match self {
            Self::Undefined => (IntrinsicCallableRecipeKind::Undefined, 0),
            Self::Method(method) => (IntrinsicCallableRecipeKind::Method, method.builtin() as u32),
            Self::Ready(slot) => (IntrinsicCallableRecipeKind::Ready, slot.gc_index()),
        }
    }
    fn identity(self) -> u64 {
        let (kind, payload) = self.words();
        (u64::from(kind as u32) << 32) | u64::from(payload)
    }
}

/// The wire discriminant is shared by row sealing and the exhaustive emitter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum IntrinsicPropertyRecipeKind {
    Number,
    Boolean,
    String,
    Method,
    ReadyRealmSlot,
    Accessor,
    LayoutObject,
    ArrayPrototype,
    WellKnownSymbol,
}
impl IntrinsicPropertyRecipeKind {
    pub(crate) const ALL: [Self; 9] = [
        Self::Number,
        Self::Boolean,
        Self::String,
        Self::Method,
        Self::ReadyRealmSlot,
        Self::Accessor,
        Self::LayoutObject,
        Self::ArrayPrototype,
        Self::WellKnownSymbol,
    ];
}

#[derive(Clone, Copy)]
pub(crate) enum IntrinsicPropertyRecipe {
    Number(u64),
    Boolean(bool),
    String(&'static str),
    Method(StandardBuiltinId),
    ReadyRealmSlot(ReadyRealmSlot),
    Accessor {
        getter: IntrinsicCallableRecipe,
        setter: IntrinsicCallableRecipe,
    },
    LayoutObject(IntrinsicPropertyLayoutId),
    ArrayPrototype,
    WellKnownSymbol(WellKnownSymbol),
    // Compiler-only metadata references seal to the existing Number/String rows.
    NativeLength(IntrinsicPropertyLayoutId),
    NativeName(IntrinsicPropertyLayoutId),
}
impl IntrinsicPropertyRecipe {
    pub(crate) fn ready(slot: NonArrayRealmIntrinsicSlot) -> Result<Self, EmitError> {
        Ok(Self::ReadyRealmSlot(ReadyRealmSlot::new(slot)?))
    }
    pub(crate) fn kind(self) -> IntrinsicPropertyRecipeKind {
        match self {
            Self::Number(_) | Self::NativeLength(_) => IntrinsicPropertyRecipeKind::Number,
            Self::Boolean(_) => IntrinsicPropertyRecipeKind::Boolean,
            Self::String(_) | Self::NativeName(_) => IntrinsicPropertyRecipeKind::String,
            Self::Method(_) => IntrinsicPropertyRecipeKind::Method,
            Self::ReadyRealmSlot(_) => IntrinsicPropertyRecipeKind::ReadyRealmSlot,
            Self::Accessor { .. } => IntrinsicPropertyRecipeKind::Accessor,
            Self::LayoutObject(_) => IntrinsicPropertyRecipeKind::LayoutObject,
            Self::ArrayPrototype => IntrinsicPropertyRecipeKind::ArrayPrototype,
            Self::WellKnownSymbol(_) => IntrinsicPropertyRecipeKind::WellKnownSymbol,
        }
    }
    fn identity(self) -> (u32, u64, u64) {
        let mut kind = self.kind() as u32;
        let (first, second) = match self {
            Self::Number(bits) => (bits, 0),
            Self::Boolean(value) => (u64::from(value), 0),
            Self::String(_) => (0, 0),
            Self::Method(builtin) => (builtin as u64, 0),
            Self::ReadyRealmSlot(slot) => (u64::from(slot.gc_index()), 0),
            Self::Accessor { getter, setter } => (getter.identity(), setter.identity()),
            Self::LayoutObject(layout) => (layout as u64, 0),
            Self::ArrayPrototype => (0, 0),
            Self::WellKnownSymbol(symbol) => (symbol as u64, 0),
            Self::NativeLength(layout) => {
                kind = u32::MAX;
                (layout as u64, 0)
            }
            Self::NativeName(layout) => {
                kind = u32::MAX - 1;
                (layout as u64, 0)
            }
        };
        (kind, first, second)
    }
}

/// A cache slot is local to one complete object's installation, never a builtin ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IntrinsicIdentityGroup(u32);
impl IntrinsicIdentityGroup {
    pub(crate) fn index(self) -> u32 {
        self.0
    }
}
#[derive(Clone, Copy)]
pub(crate) struct IntrinsicPropertySpec {
    pub(crate) key: IntrinsicKey<'static>,
    pub(crate) flags: DescriptorWord,
    pub(crate) group: IntrinsicIdentityGroup,
    pub(crate) recipe: IntrinsicPropertyRecipe,
}
/// Complete immutable named properties, with no canonical Array-index keys.
/// Packed append can therefore skip indexed descriptor probes for this storage.
pub(crate) struct ValidatedIntrinsicPropertyLayout {
    properties: Vec<IntrinsicPropertySpec>,
    group_count: u32,
}
impl ValidatedIntrinsicPropertyLayout {
    fn new(properties: Vec<IntrinsicPropertySpec>, group_count: u32) -> Result<Self, EmitError> {
        let error = || EmitError::unsupported("invalid complete intrinsic property layout");
        if properties.is_empty() || group_count == 0 {
            return Err(error());
        }
        let mut names = BTreeSet::new();
        let mut symbols = BTreeSet::new();
        let mut groups = BTreeMap::new();
        for property in &properties {
            if let IntrinsicPropertyRecipe::Method(builtin) = property.recipe {
                if DeferredIntrinsicMethod::new(builtin).is_none() {
                    return Err(EmitError::unsupported(format!(
                        "complete intrinsic layout method {} requires its canonical recipe",
                        builtin.debug_name(),
                    )));
                }
            }
            let unique = match property.key {
                IntrinsicKey::Name(name) => {
                    let units = scriptw_ir::decode_js_string_utf16(name);
                    if is_array_index(&units) {
                        return Err(error());
                    }
                    names.insert(units)
                }
                IntrinsicKey::Symbol(symbol) => symbols.insert(symbol),
            };
            if !unique || property.group.0 >= group_count {
                return Err(error());
            }
            if let Some(previous) = groups.insert(property.group.0, property.recipe) {
                if previous.identity() != property.recipe.identity()
                    || matches!((previous, property.recipe), (IntrinsicPropertyRecipe::String(a), IntrinsicPropertyRecipe::String(b)) if a != b)
                {
                    return Err(error());
                }
            }
            if let IntrinsicPropertyRecipe::Accessor { .. } = property.recipe {
                let setter = property.group.0.checked_add(1).ok_or_else(error)?;
                if setter >= group_count || groups.insert(setter, property.recipe).is_some() {
                    return Err(error());
                }
            }
        }
        if groups.len() != group_count as usize {
            return Err(error());
        }
        Ok(Self {
            properties,
            group_count,
        })
    }
    pub(crate) fn properties(&self) -> &[IntrinsicPropertySpec] {
        &self.properties
    }
    pub(crate) fn group_count(&self) -> u32 {
        self.group_count
    }
}

// Classify the semantic UTF-16 key, just as the runtime indexed-key owner does.
pub(crate) fn is_array_index(units: &[u16]) -> bool {
    if units.is_empty() || units.len() > 10 || (units.len() > 1 && units[0] == 48) {
        return false;
    }
    let value = units.iter().try_fold(0_u32, |value, &unit| {
        let digit = unit.checked_sub(48).filter(|digit| *digit < 10)?;
        value.checked_mul(10)?.checked_add(u32::from(digit))
    });
    matches!(value, Some(value) if value != u32::MAX)
}

#[derive(Default)]
struct LayoutBuilder {
    properties: Vec<IntrinsicPropertySpec>,
    groups: u32,
}
impl LayoutBuilder {
    fn data(
        &mut self,
        key: IntrinsicKey<'static>,
        recipe: IntrinsicPropertyRecipe,
        writable: bool,
        enumerable: bool,
        configurable: bool,
    ) {
        let group = IntrinsicIdentityGroup(self.groups);
        self.groups += 1;
        self.properties.push(IntrinsicPropertySpec {
            key,
            recipe,
            group,
            flags: StoredPropertyAttributes::Data {
                writable,
                enumerable,
                configurable,
            }
            .descriptor_word(),
        });
    }
    fn method(&mut self, method: IntrinsicMethodSpec) -> Result<(), EmitError> {
        let recipe = match crate::module::standard_builtin_function_realm_slot(method.builtin) {
            Some(slot) => IntrinsicPropertyRecipe::ready(slot)?,
            None => IntrinsicPropertyRecipe::Method(method.builtin),
        };
        self.data(
            method.key,
            recipe,
            method.writable,
            false,
            method.configurable,
        );
        Ok(())
    }
    fn methods(&mut self, plan: IntrinsicMethodPlan) -> Result<(), EmitError> {
        for &method in plan.methods()?.iter() {
            self.method(method)?;
        }
        Ok(())
    }
    fn tag(&mut self, text: &'static str) {
        self.data(
            IntrinsicKey::Symbol(WellKnownSymbol::ToStringTag),
            IntrinsicPropertyRecipe::String(text),
            false,
            false,
            true,
        );
    }
    fn constructor(&mut self, slot: NonArrayRealmIntrinsicSlot) -> Result<(), EmitError> {
        self.data(
            IntrinsicKey::Name("constructor"),
            IntrinsicPropertyRecipe::ready(slot)?,
            true,
            false,
            true,
        );
        Ok(())
    }
    fn native_metadata(&mut self, id: IntrinsicPropertyLayoutId) -> Result<(), EmitError> {
        if id.native_identity().is_none() {
            return Err(EmitError::unsupported(
                "native metadata requires its complete callable layout",
            ));
        }
        for (name, recipe) in [
            ("length", IntrinsicPropertyRecipe::NativeLength(id)),
            ("name", IntrinsicPropertyRecipe::NativeName(id)),
        ] {
            self.data(IntrinsicKey::Name(name), recipe, false, false, true);
        }
        Ok(())
    }
    fn aliases(&mut self, keys: &[IntrinsicKey<'static>], recipe: IntrinsicPropertyRecipe) {
        let group = IntrinsicIdentityGroup(self.groups);
        self.groups += 1;
        for &key in keys {
            self.properties.push(IntrinsicPropertySpec {
                key,
                recipe,
                group,
                flags: StoredPropertyAttributes::Data {
                    writable: true,
                    enumerable: false,
                    configurable: true,
                }
                .descriptor_word(),
            });
        }
    }
    fn accessor(
        &mut self,
        key: IntrinsicKey<'static>,
        getter: IntrinsicCallableRecipe,
        setter: IntrinsicCallableRecipe,
    ) {
        self.accessor_with_enumerability(key, getter, setter, false);
    }
    fn accessor_with_enumerability(
        &mut self,
        key: IntrinsicKey<'static>,
        getter: IntrinsicCallableRecipe,
        setter: IntrinsicCallableRecipe,
        enumerable: bool,
    ) {
        let group = IntrinsicIdentityGroup(self.groups);
        self.groups += 2;
        self.properties.push(IntrinsicPropertySpec {
            key,
            group,
            flags: StoredPropertyAttributes::Accessor {
                enumerable,
                configurable: true,
            }
            .descriptor_word(),
            recipe: IntrinsicPropertyRecipe::Accessor { getter, setter },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ready_recipes_require_eager_producers_and_accessors_require_callables() {
        use NonArrayRealmIntrinsicSlot as Slot;
        for slot in [
            Slot::RegExpConstructor,
            Slot::DateConstructor,
            Slot::TypeErrorPrototype,
            Slot::ArrayBufferConstructor,
            Slot::IntlLocaleConstructor,
            Slot::TemporalInstantConstructor,
        ] {
            assert!(IntrinsicPropertyRecipe::ready(slot).is_err());
            assert!(IntrinsicCallableRecipe::ready(slot).is_err());
        }
        assert!(IntrinsicPropertyRecipe::ready(Slot::ObjectPrototype).is_ok());
        assert!(IntrinsicCallableRecipe::ready(Slot::ObjectPrototype).is_err());
        assert!(IntrinsicCallableRecipe::ready(Slot::ThrowTypeError).is_ok());
        assert!(IntrinsicCallableRecipe::method(StandardBuiltinId::ThrowTypeError).is_err());
        assert!(IntrinsicCallableRecipe::method(StandardBuiltinId::MapSpeciesGetter).is_ok());
        assert_eq!(
            IntrinsicCallableRecipe::Undefined.words(),
            (IntrinsicCallableRecipeKind::Undefined, 0)
        );
        let (kind, slot) = IntrinsicCallableRecipe::ready(Slot::ThrowTypeError)
            .unwrap()
            .words();
        assert_eq!(kind, IntrinsicCallableRecipeKind::Ready);
        assert_eq!(slot, Slot::ThrowTypeError.gc_index());
    }

    #[test]
    fn complete_layouts_keep_seed_order_and_explicit_alias_identity() {
        for id in IntrinsicPropertyLayoutId::ALL {
            id.layout().unwrap();
        }
        let string = IntrinsicPropertyLayoutId::StringPrototype.layout().unwrap();
        assert!(matches!(
            string.properties()[0].key,
            IntrinsicKey::Name("length")
        ));
        assert!(matches!(
            string.properties()[1].key,
            IntrinsicKey::Name("constructor")
        ));
        let named = |name| {
            string
                .properties()
                .iter()
                .find(|p| matches!(p.key, IntrinsicKey::Name(key) if key == name))
                .unwrap()
        };
        assert_eq!(named("trimStart").group, named("trimLeft").group);
        assert_eq!(named("trimEnd").group, named("trimRight").group);
        assert_ne!(named("trimStart").group, named("trimEnd").group);
        let array = IntrinsicPropertyLayoutId::ArrayPrototype.layout().unwrap();
        let values = array
            .properties()
            .iter()
            .find(|p| matches!(p.key, IntrinsicKey::Name("values")))
            .unwrap();
        let iterator = array
            .properties()
            .iter()
            .find(|p| matches!(p.key, IntrinsicKey::Symbol(WellKnownSymbol::Iterator)))
            .unwrap();
        assert_eq!(values.group, iterator.group);
        assert!(
            matches!(values.recipe, IntrinsicPropertyRecipe::ReadyRealmSlot(slot) if slot.gc_index() == NonArrayRealmIntrinsicSlot::ArrayPrototypeValues.gc_index())
        );
    }
    #[test]
    fn validation_rejects_duplicate_keys_and_aliases_to_different_recipes() {
        let mut duplicate = LayoutBuilder::default();
        duplicate.tag("a");
        duplicate.tag("b");
        assert!(
            ValidatedIntrinsicPropertyLayout::new(duplicate.properties, duplicate.groups).is_err()
        );
        for (first, second) in [
            ("push", "push"),
            ("\u{F0000}", "\u{F0000}\u{F0000}"),
            ("\u{F0000}D800", "\u{F0000}d800"),
        ] {
            let mut duplicate = LayoutBuilder::default();
            for name in [first, second] {
                duplicate.data(
                    IntrinsicKey::Name(name),
                    IntrinsicPropertyRecipe::Number(0),
                    true,
                    true,
                    true,
                );
            }
            assert!(
                ValidatedIntrinsicPropertyLayout::new(duplicate.properties, duplicate.groups)
                    .is_err(),
                "duplicate decoded key: {first:?}, {second:?}",
            );
        }
        let mut aliases = LayoutBuilder::default();
        aliases.data(
            IntrinsicKey::Name("a"),
            IntrinsicPropertyRecipe::Number(0),
            true,
            false,
            true,
        );
        aliases.data(
            IntrinsicKey::Name("b"),
            IntrinsicPropertyRecipe::Boolean(false),
            true,
            false,
            true,
        );
        aliases.properties[1].group = aliases.properties[0].group;
        assert!(ValidatedIntrinsicPropertyLayout::new(aliases.properties, 1).is_err());
    }
    #[test]
    fn validation_excludes_only_canonical_array_index_strings() {
        for (names, valid) in [
            (&["0", "1", "9", "10", "4294967294"][..], false),
            (
                &[
                    "",
                    "00",
                    "01",
                    "-0",
                    "+1",
                    "1.0",
                    "1e0",
                    "4294967295",
                    "4294967296",
                    "١",
                    "\0",
                    "0\0",
                    "0\u{F0000}D800",
                    "\u{F0000}D800",
                    "\u{F0000}DC00",
                    "\u{F0000}DFFF",
                    "\u{10000}",
                    "\u{F0000}",
                    "\u{F0000}\u{F0000}",
                    r"\u0030",
                    r"\x30",
                    r"\u{30}",
                ][..],
                true,
            ),
        ] {
            for &name in names {
                let mut plan = LayoutBuilder::default();
                plan.data(
                    IntrinsicKey::Name(name),
                    IntrinsicPropertyRecipe::Number(0),
                    true,
                    true,
                    true,
                );
                assert_eq!(
                    ValidatedIntrinsicPropertyLayout::new(plan.properties, plan.groups).is_ok(),
                    valid,
                    "{name:?}",
                );
            }
        }
    }
}
