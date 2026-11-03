use super::*;
use crate::gc_types::{CompletionLocals, ValueLocals};
use crate::intrinsics::temporal::TemporalIntrinsicFamily;

/// The ordinary-object intrinsic prototypes selected by
/// `GetPrototypeFromConstructor` in constructor fallback paths.
///
/// `Generator` and `AsyncGenerator` are the defaults that generator body
/// evaluation passes to `OrdinaryCreateFromConstructor` with the generator
/// function itself as the constructor.
///
/// Array fallback reads its real Array-layout Realm root. Other defaults
/// select the closed ordinary intrinsic table; neither route exposes an index.
#[derive(Clone, Copy)]
pub(crate) enum OrdinaryDefaultPrototype {
    ReadableStream,
    ReadableStreamDefaultReader,
    AbortController,
    AbortSignal,
    EventTarget,
    Event,
    DOMException,
    ShadowRealm,
    Object,
    Array,
    Function(DynamicFunctionKind),
    MessageError(ErrorMessageConstructorKind),
    String,
    Number,
    Boolean,
    Date,
    Map,
    Set,
    WeakMap,
    WeakSet,
    WeakRef,
    FinalizationRegistry,
    ArrayBuffer,
    SharedArrayBuffer,
    DataView,
    TypedArray(TypedArrayElementKind),
    Temporal(TemporalIntrinsicFamily),
    Iterator,
    RegExp,
    IntlLocale,
    IntlDateTimeFormat,
    IntlNumberFormat,
    IntlPluralRules,
    IntlListFormat,
    IntlCollator,
    IntlDisplayNames,
    IntlRelativeTimeFormat,
    IntlSegmenter,
    IntlDurationFormat,
    Promise,
    DisposableStack,
    TextEncoder,
    TextDecoder,
    AsyncDisposableStack,
    AggregateError,
    SuppressedError,
    Generator,
    AsyncGenerator,
}

impl OrdinaryDefaultPrototype {
    const fn slot(self) -> Option<NonArrayRealmIntrinsicSlot> {
        Some(match self {
            Self::ReadableStream => NonArrayRealmIntrinsicSlot::ReadableStreamPrototype,
            Self::ReadableStreamDefaultReader => {
                NonArrayRealmIntrinsicSlot::ReadableStreamDefaultReaderPrototype
            }
            Self::AbortController => NonArrayRealmIntrinsicSlot::AbortControllerPrototype,
            Self::AbortSignal => NonArrayRealmIntrinsicSlot::AbortSignalPrototype,
            Self::EventTarget => NonArrayRealmIntrinsicSlot::EventTargetPrototype,
            Self::Event => NonArrayRealmIntrinsicSlot::EventPrototype,
            Self::DOMException => NonArrayRealmIntrinsicSlot::DOMExceptionPrototype,
            Self::ShadowRealm => NonArrayRealmIntrinsicSlot::ShadowRealmPrototype,
            Self::Array => return None,
            Self::MessageError(kind) => kind.prototype_slot(),
            Self::Temporal(family) => family.prototype_slot(),
            Self::Object => NonArrayRealmIntrinsicSlot::ObjectPrototype,
            Self::Function(kind) => match kind {
                DynamicFunctionKind::Ordinary => NonArrayRealmIntrinsicSlot::FunctionPrototype,
                DynamicFunctionKind::Generator => {
                    NonArrayRealmIntrinsicSlot::GeneratorFunctionPrototype
                }
                DynamicFunctionKind::Async => NonArrayRealmIntrinsicSlot::AsyncFunctionPrototype,
                DynamicFunctionKind::AsyncGenerator => {
                    NonArrayRealmIntrinsicSlot::AsyncGeneratorFunctionPrototype
                }
            },
            Self::String => NonArrayRealmIntrinsicSlot::StringPrototype,
            Self::Number => NonArrayRealmIntrinsicSlot::NumberPrototype,
            Self::Boolean => NonArrayRealmIntrinsicSlot::BooleanPrototype,
            Self::Date => NonArrayRealmIntrinsicSlot::DatePrototype,
            Self::Map => NonArrayRealmIntrinsicSlot::MapPrototype,
            Self::Set => NonArrayRealmIntrinsicSlot::SetPrototype,
            Self::WeakMap => NonArrayRealmIntrinsicSlot::WeakMapPrototype,
            Self::WeakSet => NonArrayRealmIntrinsicSlot::WeakSetPrototype,
            Self::WeakRef => NonArrayRealmIntrinsicSlot::WeakRefPrototype,
            Self::FinalizationRegistry => NonArrayRealmIntrinsicSlot::FinalizationRegistryPrototype,
            Self::ArrayBuffer => NonArrayRealmIntrinsicSlot::ArrayBufferPrototype,
            Self::SharedArrayBuffer => NonArrayRealmIntrinsicSlot::SharedArrayBufferPrototype,
            Self::DataView => NonArrayRealmIntrinsicSlot::DataViewPrototype,
            Self::TypedArray(kind) => NonArrayRealmIntrinsicSlot::prototype_identity(kind),
            Self::Iterator => NonArrayRealmIntrinsicSlot::IteratorPrototype,
            Self::RegExp => NonArrayRealmIntrinsicSlot::RegExpPrototype,
            Self::IntlLocale => NonArrayRealmIntrinsicSlot::IntlLocalePrototype,
            Self::IntlDateTimeFormat => NonArrayRealmIntrinsicSlot::IntlDateTimeFormatPrototype,
            Self::IntlNumberFormat => NonArrayRealmIntrinsicSlot::IntlNumberFormatPrototype,
            Self::IntlPluralRules => NonArrayRealmIntrinsicSlot::IntlPluralRulesPrototype,
            Self::IntlListFormat => NonArrayRealmIntrinsicSlot::IntlListFormatPrototype,
            Self::IntlCollator => NonArrayRealmIntrinsicSlot::IntlCollatorPrototype,
            Self::IntlDisplayNames => NonArrayRealmIntrinsicSlot::IntlDisplayNamesPrototype,
            Self::IntlRelativeTimeFormat => {
                NonArrayRealmIntrinsicSlot::IntlRelativeTimeFormatPrototype
            }
            Self::IntlSegmenter => NonArrayRealmIntrinsicSlot::IntlSegmenterPrototype,
            Self::IntlDurationFormat => NonArrayRealmIntrinsicSlot::IntlDurationFormatPrototype,
            Self::Promise => NonArrayRealmIntrinsicSlot::PromisePrototype,
            Self::DisposableStack => NonArrayRealmIntrinsicSlot::DisposableStackPrototype,
            Self::TextEncoder => NonArrayRealmIntrinsicSlot::TextEncoderPrototype,
            Self::TextDecoder => NonArrayRealmIntrinsicSlot::TextDecoderPrototype,
            Self::AsyncDisposableStack => NonArrayRealmIntrinsicSlot::AsyncDisposableStackPrototype,
            Self::AggregateError => NonArrayRealmIntrinsicSlot::AggregateErrorPrototype,
            Self::SuppressedError => NonArrayRealmIntrinsicSlot::SuppressedErrorPrototype,
            Self::Generator => NonArrayRealmIntrinsicSlot::GeneratorPrototype,
            Self::AsyncGenerator => NonArrayRealmIntrinsicSlot::AsyncGeneratorPrototype,
        })
    }
}

/// A complete required prototype from a Realm already routed through
/// GetFunctionRealm. Copying its full value consumes the selected owner.
#[must_use]
pub(super) struct ResolvedRealmOrdinaryPrototypeLocal(ValueLocals);

impl FunctionBuilder<'_> {
    pub(super) fn emit_load_required_resolved_realm_ordinary_prototype(
        &mut self,
        realm: &ResolvedFunctionRealmLocal,
        intrinsic: OrdinaryDefaultPrototype,
        function: &mut Function,
    ) -> ResolvedRealmOrdinaryPrototypeLocal {
        let value = self.runtime_schema().reserve_value_local(function);
        match intrinsic.slot() {
            Some(slot) => {
                self.emit_load_non_array_realm_intrinsic(realm.realm(), slot, &value, function)
            }
            None => {
                let schema = self.runtime_schema();
                let array = schema.reserve_gc_local(function).initialize(
                    self.emit_load_realm_array_prototype(realm.realm(), function),
                    function,
                );
                value.set_reference(&array, schema, function);
                array.clear(function);
            }
        }
        ResolvedRealmOrdinaryPrototypeLocal(value)
    }

    /// GetPrototypeFromConstructor observes the acquired constructor's
    /// prototype once. The caller receives either the original whole Throw
    /// or the complete selected prototype and controls its own Realm lifetime.
    pub(crate) fn emit_get_prototype_from_constructor(
        &mut self,
        constructor: &ValueLocals,
        intrinsic: OrdinaryDefaultPrototype,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        let key = self.emit_function_string_key("prototype", function)?;
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_object_read_with_throw_routing(
            constructor,
            constructor,
            &key,
            &pending,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        result.copy_from(&pending, function);
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(COMPLETION_KIND_THROW as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_heap_object_like_tag_i32(result.value().tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_required_function_realm_ordinary_prototype_completion(
            constructor,
            intrinsic,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        key.clear(function);
        pending.clear(function);
        Ok(())
    }

    /// Required Realm selection retains abrupt completion inside its local
    /// boundary so a caller can restore its execution Realm before propagation.
    pub(super) fn emit_required_function_realm_ordinary_prototype_completion(
        &mut self,
        constructor: &ValueLocals,
        intrinsic: OrdinaryDefaultPrototype,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        result.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let realm_result = self.emit_get_function_realm(constructor, function);
        let realm = self.emit_route_function_realm_result(
            realm_result,
            FunctionRealmRevokedRoute::ThrowTypeErrorAndBranch {
                result,
                target: exit,
            },
            function,
        )?;
        let selected =
            self.emit_load_required_resolved_realm_ordinary_prototype(&realm, intrinsic, function);
        self.emit_install_resolved_realm_ordinary_prototype(selected, result.value(), function);
        self.release_resolved_function_realm_local(realm, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_required_new_target_realm_ordinary_prototype(
        &mut self,
        new_target: &ValueLocals,
        intrinsic: OrdinaryDefaultPrototype,
        prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let completion = self.runtime_schema().reserve_completion(function);
        self.emit_required_function_realm_ordinary_prototype_completion(
            new_target,
            intrinsic,
            &completion,
            function,
        )?;
        self.completion().copy_from(&completion, function);
        self.emit_propagate_current_throw_if_needed(function);
        prototype.copy_from(completion.value(), function);
        completion.clear(function);
        Ok(())
    }

    pub(super) fn emit_install_resolved_realm_ordinary_prototype(
        &mut self,
        prototype: ResolvedRealmOrdinaryPrototypeLocal,
        result: &ValueLocals,
        function: &mut Function,
    ) {
        result.copy_from(&prototype.0, function);
        prototype.0.clear(function);
    }
}
