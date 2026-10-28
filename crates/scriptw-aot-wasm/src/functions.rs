use super::*;
use crate::function_entry::{
    AsyncBodyInputs, AsyncGeneratorBodyInputs, EntryArguments, GeneratorBodyInputs,
    OrdinaryBodyInputs, RuntimeFunctionEntryDispatch,
};

use crate::module::TypedArrayElementKind;
use scriptw_ir::{ClassMethodKindIr, NativeErrorKind, StaticRegExpCompilation};

mod argument_vectors;
pub(crate) use argument_vectors::ArgumentListConstruction;
mod activation_allocation;
mod arguments_index_mapping;
mod arguments_object;
mod async_generator_body;
mod bound_function_allocation;
mod bound_function_record;
mod call_dispatch;
mod class_captures;
mod class_definition;
mod class_field_names;
mod class_members;
mod class_private_definitions;
mod class_scope;
mod constant_leaf_call;
pub(crate) use class_captures::FunctionHomeObject;
pub(crate) use constant_leaf_call::ConstantLeafPlan;
mod created_realm_array_prototype;
mod current_builtin_realm_closure;
mod current_function_realm_array_prototype;
mod current_function_realm_async_disposable_stack;
mod current_function_realm_disposable_stack;
mod current_function_realm_typed_array_constructor;
mod host_output;
pub(crate) use current_function_realm_typed_array_constructor::CurrentFunctionRealmTypedArrayConstructor;
mod direct_eval;
pub(crate) mod direct_eval_invocation;
mod eval_intrinsic;
mod function_name;
mod function_object_allocation;
mod native_direct_eval;
mod native_direct_eval_factory;
mod numeric_imul_call;
mod numeric_leaf_call;
pub(crate) use numeric_leaf_call::BinaryNumericLeafPlan;
pub(crate) use numeric_leaf_call::NumericLeafPlan;
mod binary_numeric_callback;
mod packed_join_call;
mod source_callable_factories;
use function_object_allocation::FunctionAllocationInputs;
mod function_realm;
mod generator_instance_prototype;
pub(crate) use function_name::FunctionNamePrefix;
use generator_instance_prototype::GeneratorInstanceFamily;
mod constructability;
mod indirect_call;
mod instance_elements_hook;
mod iterator_close;
mod legacy_activation;
pub(crate) use legacy_activation::SourceActivation;
mod intl_fallback_symbol;
mod proxy_creation_execution_realm;
mod proxy_execution_realm;
mod realm_intrinsic;
mod required_resolved_realm_ordinary_prototype;
mod throw_type_error;
pub(crate) use function_realm::FunctionRealmRevokedRoute;
use function_realm::ResolvedFunctionRealmLocal;
pub(crate) use proxy_creation_execution_realm::ProxyCreationExecutionRealm;
pub(crate) use required_resolved_realm_ordinary_prototype::OrdinaryDefaultPrototype;

#[derive(Clone, Copy)]
pub(crate) enum CallContinuation {
    Continue,
    Return,
}

/// The actual Realm and its completed callable Function prototype remain
/// rooted together throughout intrinsic bootstrap. Neither owner is copyable.
#[must_use]
pub(crate) struct RealmFunctionMaterializationContext {
    realm: crate::gc_types::GcLocal<crate::gc_types::RealmRecord>,
    function_prototype: crate::gc_types::GcLocal<crate::gc_types::FunctionObject>,
}

impl RealmFunctionMaterializationContext {
    pub(crate) fn realm(&self) -> &crate::gc_types::GcLocal<crate::gc_types::RealmRecord> {
        &self.realm
    }
    pub(crate) fn function_prototype(
        &self,
    ) -> &crate::gc_types::GcLocal<crate::gc_types::FunctionObject> {
        &self.function_prototype
    }
}

/// A realm-intrinsic slot whose representation is not constrained by the
/// `%Array.prototype%` typestate.
///
/// The Array slot is intentionally absent. Callers cannot manufacture this
/// fieldless enum from a raw offset, so adding Array to the generic writer
/// requires an explicit change to this closed domain and its exhaustive map.
#[derive(Clone, Copy)]
pub(crate) enum NonArrayRealmIntrinsicSlot {
    ShadowRealmConstructor,
    ShadowRealmPrototype,
    AbstractModuleSourceConstructor,
    AbstractModuleSourcePrototype,
    ModuleSourcePrototype,
    ArrayPrototypeValues,
    ThrowTypeError,
    TypeErrorPrototype,
    ErrorPrototype,
    EvalErrorPrototype,
    RangeErrorPrototype,
    ReferenceErrorPrototype,
    SyntaxErrorPrototype,
    URIErrorPrototype,
    AggregateErrorPrototype,
    SuppressedErrorPrototype,
    PromisePrototype,
    FunctionPrototype,
    PromiseConstructor,
    EvalFunction,
    AsyncDisposableStackPrototype,
    DisposableStackPrototype,
    GeneratorFunctionConstructor,
    AsyncFunctionConstructor,
    AsyncGeneratorFunctionConstructor,
    ObjectPrototype,
    ArrayIteratorPrototype,
    StringIteratorPrototype,
    RegExpStringIteratorPrototype,
    MapIteratorPrototype,
    SetIteratorPrototype,
    IteratorHelperPrototype,
    IteratorPrototype,
    IteratorFromWrapperPrototype,
    GeneratorPrototype,
    GeneratorFunctionPrototype,
    AsyncIteratorPrototype,
    AsyncFunctionPrototype,
    AsyncGeneratorPrototype,
    AsyncGeneratorFunctionPrototype,
    NumberPrototype,
    StringPrototype,
    BooleanPrototype,
    SymbolPrototype,
    BigIntPrototype,
    MapPrototype,
    SetPrototype,
    WeakMapPrototype,
    WeakSetPrototype,
    WeakRefPrototype,
    FinalizationRegistryPrototype,
    RegExpPrototype,
    DatePrototype,
    TemporalInstantPrototype,
    TemporalDurationPrototype,
    TemporalPlainDatePrototype,
    TemporalZonedDateTimePrototype,
    TemporalPlainTimePrototype,
    TemporalPlainDateTimePrototype,
    TemporalPlainYearMonthPrototype,
    TemporalPlainMonthDayPrototype,

    IntlLocalePrototype,
    IntlDateTimeFormatPrototype,
    IntlNumberFormatPrototype,
    IntlPluralRulesPrototype,
    IntlListFormatPrototype,
    IntlCollatorPrototype,
    IntlDisplayNamesPrototype,
    IntlRelativeTimeFormatPrototype,
    IntlSegmenterPrototype,
    IntlDurationFormatPrototype,
    IntlSegmentsPrototype,
    IntlSegmentIteratorPrototype,
    Float64ArrayPrototype,
    Float32ArrayPrototype,
    Float16ArrayPrototype,
    Int32ArrayPrototype,
    Int16ArrayPrototype,
    Int8ArrayPrototype,
    Uint32ArrayPrototype,
    Uint16ArrayPrototype,
    Uint8ArrayPrototype,
    Uint8ClampedArrayPrototype,
    BigInt64ArrayPrototype,
    BigUint64ArrayPrototype,
    Float64ArrayConstructor,
    Float32ArrayConstructor,
    Int8ArrayConstructor,
    Int16ArrayConstructor,
    Int32ArrayConstructor,
    Uint8ClampedArrayConstructor,
    Uint8ArrayConstructor,
    Uint16ArrayConstructor,
    Uint32ArrayConstructor,
    BigInt64ArrayConstructor,
    BigUint64ArrayConstructor,
    Float16ArrayConstructor,
    FunctionConstructor,
    MapConstructor,
    WeakMapConstructor,
    WeakSetConstructor,
    WeakRefConstructor,
    FinalizationRegistryConstructor,
    AsyncDisposableStackConstructor,
    DisposableStackConstructor,
    SetConstructor,
    AggregateErrorConstructor,
    SuppressedErrorConstructor,
    ObjectConstructor,
    ProxyConstructor,
    IteratorConstructor,
    ArrayConstructor,
    ArrayBufferConstructor,
    SharedArrayBufferConstructor,
    DataViewConstructor,
    TypedArrayConstructor,
    DateConstructor,
    TemporalInstantConstructor,
    TemporalPlainDateConstructor,
    TemporalDurationConstructor,
    TemporalPlainTimeConstructor,
    TemporalPlainDateTimeConstructor,
    TemporalPlainYearMonthConstructor,
    TemporalPlainMonthDayConstructor,
    TemporalZonedDateTimeConstructor,
    IntlLocaleConstructor,
    IntlDateTimeFormatConstructor,
    IntlNumberFormatConstructor,
    IntlPluralRulesConstructor,
    IntlListFormatConstructor,
    IntlCollatorConstructor,
    IntlDisplayNamesConstructor,
    IntlRelativeTimeFormatConstructor,
    IntlSegmenterConstructor,
    IntlDurationFormatConstructor,
    RegExpConstructor,
    BigIntConstructor,
    NumberConstructor,
    StringConstructor,
    BooleanConstructor,
    SymbolConstructor,
    ErrorConstructor,
    EvalErrorConstructor,
    RangeErrorConstructor,
    SyntaxErrorConstructor,
    TypeErrorConstructor,
    URIErrorConstructor,
    ReferenceErrorConstructor,
    RegExpPrototypeSymbolMatch,
    RegExpPrototypeSymbolMatchAll,
    RegExpPrototypeSymbolSearch,
    TypedArrayPrototypeToString,
    ParseInt,
    ParseFloat,
    ArrayBufferPrototype,
    SharedArrayBufferPrototype,
    DataViewPrototype,
    TypedArrayPrototype,
}

macro_rules! error_message_constructor_kinds {
    (
        $(
            $variant:ident => {
                native: $native:ident,
                constructor: $constructor:ident,
                prototype_slot: $prototype_slot:ident;
            };
        )+
    ) => {
        /// The seven Error-family constructors whose specification body is the
        /// shared `(message, options)` algorithm.
        ///
        /// AggregateError and SuppressedError are deliberately absent: their
        /// distinct argument processing remains owned by their dedicated
        /// constructor paths. Every identity needed by construction comes from
        /// this one row authority, so an active function cannot be paired with
        /// another Error family's default prototype.
        #[derive(Clone, Copy, Debug)]
        #[repr(usize)]
        pub(crate) enum ErrorMessageConstructorKind {
            $($variant,)+
        }

        impl ErrorMessageConstructorKind {
            pub(crate) const ALL: [Self; 7] = [$(Self::$variant,)+];

            pub(crate) const fn index(self) -> usize {
                self as usize
            }

            pub(crate) const fn from_native_error_kind(
                kind: NativeErrorKind,
            ) -> Option<Self> {
                match kind {
                    $(NativeErrorKind::$native => Some(Self::$variant),)+
                    NativeErrorKind::AggregateError | NativeErrorKind::SuppressedError => None,
                }
            }

            pub(crate) const fn native_error_kind(self) -> NativeErrorKind {
                match self {
                    $(Self::$variant => NativeErrorKind::$native,)+
                }
            }

            pub(crate) const fn constructor(self) -> StandardBuiltinId {
                match self {
                    $(Self::$variant => StandardBuiltinId::$constructor,)+
                }
            }

            pub(crate) const fn prototype_slot(self) -> NonArrayRealmIntrinsicSlot {
                match self {
                    $(Self::$variant => NonArrayRealmIntrinsicSlot::$prototype_slot,)+
                }
            }
        }
    };
}

error_message_constructor_kinds! {
    Error => {
        native: Error,
        constructor: ErrorConstructor,
        prototype_slot: ErrorPrototype;
    };
    EvalError => {
        native: EvalError,
        constructor: EvalErrorConstructor,
        prototype_slot: EvalErrorPrototype;
    };
    RangeError => {
        native: RangeError,
        constructor: RangeErrorConstructor,
        prototype_slot: RangeErrorPrototype;
    };
    ReferenceError => {
        native: ReferenceError,
        constructor: ReferenceErrorConstructor,
        prototype_slot: ReferenceErrorPrototype;
    };
    SyntaxError => {
        native: SyntaxError,
        constructor: SyntaxErrorConstructor,
        prototype_slot: SyntaxErrorPrototype;
    };
    TypeError => {
        native: TypeError,
        constructor: TypeErrorConstructor,
        prototype_slot: TypeErrorPrototype;
    };
    URIError => {
        native: URIError,
        constructor: URIErrorConstructor,
        prototype_slot: URIErrorPrototype;
    };
}

/// The fallback selected after `Get(NewTarget, "prototype")` produces a
/// primitive. The closed variants retain optional realm-slot, required
/// resolved-realm, and undefined-NewTarget active-function semantics at each
/// actual caller.
impl NonArrayRealmIntrinsicSlot {
    pub(crate) const ARRAY_INDEX: u32 = 0;
    pub(crate) const TABLE_LENGTH: u32 = 159;

    pub(crate) const fn typed_array_constructor_identity(kind: TypedArrayElementKind) -> Self {
        match kind {
            TypedArrayElementKind::Float64 => Self::Float64ArrayConstructor,
            TypedArrayElementKind::Float32 => Self::Float32ArrayConstructor,
            TypedArrayElementKind::Float16 => Self::Float16ArrayConstructor,
            TypedArrayElementKind::Int32 => Self::Int32ArrayConstructor,
            TypedArrayElementKind::Int16 => Self::Int16ArrayConstructor,
            TypedArrayElementKind::Int8 => Self::Int8ArrayConstructor,
            TypedArrayElementKind::Uint32 => Self::Uint32ArrayConstructor,
            TypedArrayElementKind::Uint16 => Self::Uint16ArrayConstructor,
            TypedArrayElementKind::Uint8 => Self::Uint8ArrayConstructor,
            TypedArrayElementKind::Uint8Clamped => Self::Uint8ClampedArrayConstructor,
            TypedArrayElementKind::BigInt64 => Self::BigInt64ArrayConstructor,
            TypedArrayElementKind::BigUint64 => Self::BigUint64ArrayConstructor,
        }
    }

    pub(crate) const fn prototype_identity(kind: TypedArrayElementKind) -> Self {
        match kind {
            TypedArrayElementKind::Float64 => Self::Float64ArrayPrototype,
            TypedArrayElementKind::Float32 => Self::Float32ArrayPrototype,
            TypedArrayElementKind::Float16 => Self::Float16ArrayPrototype,
            TypedArrayElementKind::Int32 => Self::Int32ArrayPrototype,
            TypedArrayElementKind::Int16 => Self::Int16ArrayPrototype,
            TypedArrayElementKind::Int8 => Self::Int8ArrayPrototype,
            TypedArrayElementKind::Uint32 => Self::Uint32ArrayPrototype,
            TypedArrayElementKind::Uint16 => Self::Uint16ArrayPrototype,
            TypedArrayElementKind::Uint8 => Self::Uint8ArrayPrototype,
            TypedArrayElementKind::Uint8Clamped => Self::Uint8ClampedArrayPrototype,
            TypedArrayElementKind::BigInt64 => Self::BigInt64ArrayPrototype,
            TypedArrayElementKind::BigUint64 => Self::BigUint64ArrayPrototype,
        }
    }

    pub(crate) const fn gc_index(self) -> u32 {
        match self {
            Self::ShadowRealmConstructor => 156,
            Self::ShadowRealmPrototype => 157,
            Self::AbstractModuleSourceConstructor => 154,
            Self::AbstractModuleSourcePrototype => 155,
            Self::ModuleSourcePrototype => 158,
            Self::ArrayPrototypeValues => 1,
            Self::ThrowTypeError => 2,
            Self::TypeErrorPrototype => 3,
            Self::ErrorPrototype => 4,
            Self::EvalErrorPrototype => 5,
            Self::RangeErrorPrototype => 6,
            Self::ReferenceErrorPrototype => 7,
            Self::SyntaxErrorPrototype => 8,
            Self::URIErrorPrototype => 9,
            Self::AggregateErrorPrototype => 10,
            Self::SuppressedErrorPrototype => 11,
            Self::PromisePrototype => 12,
            Self::FunctionPrototype => 13,
            Self::PromiseConstructor => 14,
            Self::EvalFunction => 15,
            Self::AsyncDisposableStackPrototype => 16,
            Self::DisposableStackPrototype => 17,
            Self::GeneratorFunctionConstructor => 18,
            Self::AsyncFunctionConstructor => 19,
            Self::AsyncGeneratorFunctionConstructor => 20,
            Self::ObjectPrototype => 21,
            Self::ArrayIteratorPrototype => 22,
            Self::StringIteratorPrototype => 23,
            Self::RegExpStringIteratorPrototype => 24,
            Self::MapIteratorPrototype => 25,
            Self::SetIteratorPrototype => 26,
            Self::IteratorHelperPrototype => 27,
            Self::IteratorPrototype => 28,
            Self::IteratorFromWrapperPrototype => 29,
            Self::GeneratorPrototype => 30,
            Self::GeneratorFunctionPrototype => 31,
            Self::AsyncIteratorPrototype => 32,
            Self::AsyncFunctionPrototype => 33,
            Self::AsyncGeneratorPrototype => 34,
            Self::AsyncGeneratorFunctionPrototype => 35,
            Self::NumberPrototype => 36,
            Self::StringPrototype => 37,
            Self::BooleanPrototype => 38,
            Self::SymbolPrototype => 39,
            Self::BigIntPrototype => 40,
            Self::MapPrototype => 41,
            Self::SetPrototype => 42,
            Self::WeakMapPrototype => 43,
            Self::WeakSetPrototype => 44,
            Self::WeakRefPrototype => 45,
            Self::FinalizationRegistryPrototype => 46,
            Self::RegExpPrototype => 47,
            Self::DatePrototype => 48,
            Self::TemporalInstantPrototype => 49,
            Self::TemporalDurationPrototype => 50,
            Self::TemporalPlainDatePrototype => 51,
            Self::TemporalZonedDateTimePrototype => 52,
            Self::TemporalPlainTimePrototype => 53,
            Self::TemporalPlainDateTimePrototype => 54,
            Self::TemporalPlainYearMonthPrototype => 55,
            Self::TemporalPlainMonthDayPrototype => 56,
            Self::IntlLocalePrototype => 57,
            Self::IntlDateTimeFormatPrototype => 58,
            Self::IntlNumberFormatPrototype => 59,
            Self::IntlPluralRulesPrototype => 60,
            Self::IntlListFormatPrototype => 61,
            Self::IntlCollatorPrototype => 62,
            Self::IntlDisplayNamesPrototype => 63,
            Self::IntlRelativeTimeFormatPrototype => 64,
            Self::IntlSegmenterPrototype => 65,
            Self::IntlDurationFormatPrototype => 66,
            Self::IntlSegmentsPrototype => 67,
            Self::IntlSegmentIteratorPrototype => 68,
            Self::Float64ArrayPrototype => 69,
            Self::Float32ArrayPrototype => 70,
            Self::Float16ArrayPrototype => 71,
            Self::Int32ArrayPrototype => 72,
            Self::Int16ArrayPrototype => 73,
            Self::Int8ArrayPrototype => 74,
            Self::Uint32ArrayPrototype => 75,
            Self::Uint16ArrayPrototype => 76,
            Self::Uint8ArrayPrototype => 77,
            Self::Uint8ClampedArrayPrototype => 78,
            Self::BigInt64ArrayPrototype => 79,
            Self::BigUint64ArrayPrototype => 80,
            Self::Float64ArrayConstructor => 81,
            Self::Float32ArrayConstructor => 82,
            Self::Int8ArrayConstructor => 83,
            Self::Int16ArrayConstructor => 84,
            Self::Int32ArrayConstructor => 85,
            Self::Uint8ClampedArrayConstructor => 86,
            Self::Uint8ArrayConstructor => 87,
            Self::Uint16ArrayConstructor => 88,
            Self::Uint32ArrayConstructor => 89,
            Self::BigInt64ArrayConstructor => 90,
            Self::BigUint64ArrayConstructor => 91,
            Self::Float16ArrayConstructor => 92,
            Self::FunctionConstructor => 93,
            Self::MapConstructor => 94,
            Self::WeakMapConstructor => 95,
            Self::WeakSetConstructor => 96,
            Self::WeakRefConstructor => 97,
            Self::FinalizationRegistryConstructor => 98,
            Self::AsyncDisposableStackConstructor => 99,
            Self::DisposableStackConstructor => 100,
            Self::SetConstructor => 101,
            Self::AggregateErrorConstructor => 102,
            Self::SuppressedErrorConstructor => 103,
            Self::ObjectConstructor => 104,
            Self::ProxyConstructor => 105,
            Self::IteratorConstructor => 106,
            Self::ArrayConstructor => 107,
            Self::ArrayBufferConstructor => 108,
            Self::SharedArrayBufferConstructor => 109,
            Self::DataViewConstructor => 110,
            Self::TypedArrayConstructor => 111,
            Self::DateConstructor => 112,
            Self::TemporalInstantConstructor => 113,
            Self::TemporalPlainDateConstructor => 114,
            Self::TemporalDurationConstructor => 115,
            Self::TemporalPlainTimeConstructor => 116,
            Self::TemporalPlainDateTimeConstructor => 117,
            Self::TemporalPlainYearMonthConstructor => 118,
            Self::TemporalPlainMonthDayConstructor => 119,
            Self::TemporalZonedDateTimeConstructor => 120,
            Self::IntlLocaleConstructor => 121,
            Self::IntlDateTimeFormatConstructor => 122,
            Self::IntlNumberFormatConstructor => 123,
            Self::IntlPluralRulesConstructor => 124,
            Self::IntlListFormatConstructor => 125,
            Self::IntlCollatorConstructor => 126,
            Self::IntlDisplayNamesConstructor => 127,
            Self::IntlRelativeTimeFormatConstructor => 128,
            Self::IntlSegmenterConstructor => 129,
            Self::IntlDurationFormatConstructor => 130,
            Self::RegExpConstructor => 131,
            Self::BigIntConstructor => 132,
            Self::NumberConstructor => 133,
            Self::StringConstructor => 134,
            Self::BooleanConstructor => 135,
            Self::SymbolConstructor => 136,
            Self::ErrorConstructor => 137,
            Self::EvalErrorConstructor => 138,
            Self::RangeErrorConstructor => 139,
            Self::SyntaxErrorConstructor => 140,
            Self::TypeErrorConstructor => 141,
            Self::URIErrorConstructor => 142,
            Self::ReferenceErrorConstructor => 143,
            Self::RegExpPrototypeSymbolMatch => 144,
            Self::RegExpPrototypeSymbolMatchAll => 145,
            Self::RegExpPrototypeSymbolSearch => 146,
            Self::TypedArrayPrototypeToString => 147,
            Self::ParseInt => 148,
            Self::ParseFloat => 149,
            Self::ArrayBufferPrototype => 150,
            Self::SharedArrayBufferPrototype => 151,
            Self::DataViewPrototype => 152,
            Self::TypedArrayPrototype => 153,
        }
    }
}

/// Whether function allocation also creates the default own `prototype`
/// property. This policy is deliberately separate from semantic
/// constructability; realm bootstrap supplies a few intrinsic prototypes.
pub(crate) enum FunctionPrototypeMaterialization {
    Automatic,
    BootstrapSupplied,
}

/// Private recursive object algorithms whose native code runs in the caller's
/// Realm rather than the compiler entry Realm.
#[derive(Clone, Copy)]
pub(crate) enum NativeObjectAlgorithm {
    GetOwnPropertyDescriptor,
    OwnKeys,
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_alloc_realm_record(
        &mut self,
        realm_id: u64,
        agent_id: u64,
        function: &mut Function,
    ) -> Result<crate::gc_types::GcStackReference<crate::gc_types::RealmRecord>, EmitError> {
        use crate::gc_types::{GcOperand, IntrinsicTable, RealmRecord, StoredValue};
        let schema = self.runtime_schema();
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let empty = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&undefined, function),
            function,
        );
        let count = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(
            (NonArrayRealmIntrinsicSlot::TABLE_LENGTH
                + intl_fallback_symbol::PRIVATE_REALM_INTRINSIC_COUNT) as i32,
        ));
        count.store(function);
        let intrinsics = schema.reserve_gc_local(function).initialize(
            schema.array_type::<IntrinsicTable>().filled(
                GcOperand::reference(&empty, schema),
                count,
                function,
            ),
            function,
        );
        let regexp_legacy = schema
            .reserve_gc_local(function)
            .initialize(self.emit_alloc_regexp_legacy_state(function), function);
        let realm = schema.struct_type::<RealmRecord>().construct(
            (
                GcOperand::i64(realm_id as i64),
                GcOperand::i64(agent_id as i64),
                GcOperand::null(schema),
                GcOperand::null(schema),
                GcOperand::null(schema),
                GcOperand::reference(&intrinsics, schema),
                GcOperand::null(schema),
                GcOperand::null(schema),
                GcOperand::null(schema),
                GcOperand::reference(&regexp_legacy, schema),
            ),
            function,
        );
        regexp_legacy.clear(function);
        intrinsics.clear(function);
        schema.release_i32_local(count, function);
        empty.clear(function);
        undefined.clear(function);
        Ok(schema.retain_snapshot_realm(realm, function))
    }

    pub(crate) fn emit_store_realm_message_error_prototype(
        &mut self,
        realm: &crate::gc_types::GcLocal<crate::gc_types::RealmRecord>,
        kind: ErrorMessageConstructorKind,
        prototype: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) {
        self.emit_store_non_array_realm_intrinsic(
            realm,
            kind.prototype_slot(),
            prototype,
            function,
        );
    }

    pub(crate) fn emit_store_current_realm_message_error_prototype(
        &mut self,
        kind: ErrorMessageConstructorKind,
        prototype: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) {
        self.emit_store_current_realm_intrinsic(kind.prototype_slot(), prototype, function);
    }

    pub(crate) fn emit_store_non_array_realm_intrinsic(
        &mut self,
        realm: &crate::gc_types::GcLocal<crate::gc_types::RealmRecord>,
        slot: NonArrayRealmIntrinsicSlot,
        value: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) {
        self.emit_store_realm_intrinsic_at_index(realm, slot.gc_index(), value, function);
    }

    pub(crate) fn emit_store_current_realm_intrinsic(
        &mut self,
        slot: NonArrayRealmIntrinsicSlot,
        value: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) {
        let realm = self.load_current_realm(function);
        self.emit_store_non_array_realm_intrinsic(&realm, slot, value, function);
        realm.clear(function);
    }

    pub(crate) fn emit_store_realm_array_prototype(
        &mut self,
        realm: &crate::gc_types::GcLocal<crate::gc_types::RealmRecord>,
        prototype: &crate::gc_types::GcLocal<crate::gc_types::ArrayObject>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        value.set_reference(prototype, schema, function);
        self.emit_store_realm_intrinsic_at_index(
            realm,
            NonArrayRealmIntrinsicSlot::ARRAY_INDEX,
            &value,
            function,
        );
        value.clear(function);
    }

    pub(crate) fn emit_function_handle_call_with_argv_inner(
        &mut self,
        callee: &crate::gc_types::ValueLocals,
        this_value: Option<&crate::gc_types::ValueLocals>,
        arguments: &crate::gc_types::GcLocal<crate::gc_types::ValueArray>,
        result: &crate::gc_types::CompletionLocals,
        route: PropagateCallThrow,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let undefined_this = schema.reserve_value_local(function);
        undefined_this.set_undefined(function);
        self.emit_function_or_proxy_call_with_argv(
            callee,
            this_value.unwrap_or(&undefined_this),
            arguments,
            result,
            function,
        )?;
        undefined_this.clear(function);
        match route {
            PropagateCallThrow::ToActiveHandler => {
                result.kind().load(function);
                function.instruction(&Instruction::I32Const(CompletionKind::Throw as i32));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                self.completion().copy_from(result, function);
                self.emit_propagate_current_throw(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            PropagateCallThrow::LeaveInCompletion => {}
        }
        Ok(())
    }

    pub(crate) fn emit_function_or_proxy_call_with_argv(
        &mut self,
        callee: &crate::gc_types::ValueLocals,
        this_value: &crate::gc_types::ValueLocals,
        arguments: &crate::gc_types::GcLocal<crate::gc_types::ValueArray>,
        result: &crate::gc_types::CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let caller_realm = self.load_current_realm(function);
        let base = self.runtime_helper_base()?;
        self.runtime_schema()
            .call_helper(
                crate::runtime_helpers::ProxyCallArguments::new(
                    callee,
                    this_value,
                    arguments,
                    &caller_realm,
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(result, function);
        self.replace_current_realm(&caller_realm, function);
        caller_realm.clear(function);
        // Every raw dispatch path forwards one whole Completion. A source
        // Return becomes a normal Call value here; native Normal values survive.
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Return as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.set_normal(result.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_prepare_super_construct_to_locals(
        &mut self,
        new_target: &crate::gc_types::ValueLocals,
        constructor: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_get_derived_new_target_to_locals(new_target, function)?;
        self.emit_get_super_constructor_to_locals(constructor, function)
    }

    pub(crate) fn emit_get_super_constructor_to_locals(
        &mut self,
        constructor: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let active = schema.reserve_value_local(function);
        self.emit_get_derived_active_function_to_locals(&active, function)?;
        let result = schema.reserve_completion(function);
        self.emit_object_get_prototype_of(&active, &result, function)?;
        self.completion().copy_from(&result, function);
        self.emit_propagate_current_throw_if_needed(function);
        constructor.copy_from(result.value(), function);
        result.clear(function);
        active.clear(function);
        Ok(())
    }

    fn emit_super_construct_completion(
        &mut self,
        constructor: &crate::gc_types::ValueLocals,
        new_target: &crate::gc_types::ValueLocals,
        arguments: &crate::gc_types::GcLocal<crate::gc_types::ValueArray>,
        result: &crate::gc_types::CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.emit_function_or_proxy_construct_with_argv(
            constructor,
            new_target,
            arguments,
            result,
            function,
        )?;
        self.completion().copy_from(result, function);
        self.emit_propagate_current_throw_if_needed(function);
        // BindThisValue follows Construct, including on a second super call.
        // Construction throws leave the original this/status cells untouched.
        let error = schema.reserve_value_local(function);
        self.emit_bind_derived_this_from_locals(result.value(), &error, function)?;
        error.clear(function);
        self.emit_initialize_super_instance_elements(result.value(), function)?;
        self.completion().initialize(function);
        result.set_normal(result.value(), function);
        Ok(())
    }

    pub(crate) fn emit_super_construct_with_prepared_arg_vector(
        &mut self,
        constructor: &crate::gc_types::ValueLocals,
        new_target: &crate::gc_types::ValueLocals,
        arguments: &crate::gc_types::GcLocal<crate::gc_types::ValueArray>,
        output: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let result = self.runtime_schema().reserve_completion(function);
        self.emit_super_construct_completion(
            constructor,
            new_target,
            arguments,
            &result,
            function,
        )?;
        output.copy_from(result.value(), function);
        result.clear(function);
        Ok(())
    }

    pub(crate) fn emit_super_construct_with_arg_vector(
        &mut self,
        arguments: &crate::gc_types::GcLocal<crate::gc_types::ValueArray>,
        result: &crate::gc_types::CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let constructor = schema.reserve_value_local(function);
        let new_target = schema.reserve_value_local(function);
        self.emit_prepare_super_construct_to_locals(&new_target, &constructor, function)?;
        self.emit_super_construct_completion(
            &constructor,
            &new_target,
            arguments,
            result,
            function,
        )?;
        new_target.clear(function);
        constructor.clear(function);
        Ok(())
    }

    pub(crate) fn current_function_meta(&self) -> Option<&WasmFunctionMeta> {
        self.function_id
            .as_ref()
            .and_then(|function_id| self.functions.get(function_id))
    }

    pub(crate) fn emit_load_super_base(
        &mut self,
        output: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        use crate::gc_types::{
            DirectEvalExecutionContext, DirectEvalExecutionContextSchema, FunctionContext,
            FunctionContextSchema, FunctionObject, FunctionObjectSchema, StoredValue,
        };
        let schema = self.runtime_schema();
        let home = schema.reserve_value_local(function);
        if let Some(context) = self.emit_direct_eval_context_root(function) {
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<DirectEvalExecutionContext>()
                    .field(DirectEvalExecutionContextSchema::HOME_OBJECT)
                    .read(&context, schema, function)
                    .reference(),
                function,
            );
            schema
                .struct_type::<StoredValue>()
                .read_into(&stored, &home, schema, function);
            stored.clear(function);
            context.clear(function);
        } else if self
            .current_function_meta()
            .is_some_and(WasmFunctionMeta::has_home_object_execution_context)
        {
            let context = self
                .body_entry_locals()
                .expect("method owns its entry")
                .function_context()
                .expect("method owns its home capture");
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<FunctionContext>()
                    .field(FunctionContextSchema::HOME_OBJECT)
                    .read(context, schema, function)
                    .reference(),
                function,
            );
            schema
                .struct_type::<StoredValue>()
                .read_into(&stored, &home, schema, function);
            stored.clear(function);
        } else if self.lexical_derived_activation.is_some() {
            let active = schema.reserve_value_local(function);
            self.emit_get_derived_active_function_to_locals(&active, function)?;
            let callable = schema.reserve_gc_local(function).initialize(
                active.cast_reference::<FunctionObject>(schema, function),
                function,
            );
            let context = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<FunctionObject>()
                    .field(FunctionObjectSchema::CONTEXT)
                    .read(&callable, schema, function)
                    .reference(),
                function,
            );
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<FunctionContext>()
                    .field(FunctionContextSchema::HOME_OBJECT)
                    .read(&context, schema, function)
                    .reference(),
                function,
            );
            schema
                .struct_type::<StoredValue>()
                .read_into(&stored, &home, schema, function);
            stored.clear(function);
            context.clear(function);
            callable.clear(function);
            active.clear(function);
        } else {
            let storage = self
                .lookup_binding(LEXICAL_HOME_OBJECT_NAME)
                .ok_or_else(|| {
                    EmitError::unsupported("super property requires its lexical home owner")
                })?;
            self.read_binding_to_locals(storage, &home, function)?;
        }
        let result = schema.reserve_completion(function);
        self.emit_object_get_prototype_of(&home, &result, function)?;
        self.completion().copy_from(&result, function);
        self.emit_propagate_current_throw_if_needed(function);
        output.copy_from(result.value(), function);
        result.clear(function);
        home.clear(function);
        Ok(())
    }

    pub(crate) fn emit_throw_if_null_super_base(
        &mut self,
        base: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        base.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let result = self.runtime_schema().reserve_completion(function);
        self.emit_throw_runtime_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::SUPER_PROPERTY_ACCESS_ON_NULL_BASE,
            &result,
            function,
        )?;
        self.completion().copy_from(&result, function);
        result.clear(function);
        self.emit_dispatch_current_completion(function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_call_args_vector(
        &mut self,
        arguments: &[TypedExpr],
        function: &mut Function,
    ) -> Result<crate::gc_types::GcLocal<crate::gc_types::ValueArray>, EmitError> {
        let schema = self.runtime_schema();
        if arguments.iter().all(|argument| {
            !matches!(
                argument.expr,
                ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
            )
        }) {
            if arguments.is_empty() {
                return Ok(self.emit_pre_evaluated_arg_vector(&[], function));
            }
            let value = schema.reserve_value_local(function);
            let mut stored = Vec::with_capacity(arguments.len());
            for argument in arguments {
                self.compile_expr_to_value(argument, &value, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                // The immutable snapshot roots this argument across every later
                // argument's calls/GC. Reuse only the evaluation scratch triple;
                // a later evaluation cannot overwrite an earlier argument.
                stored.push(
                    schema.reserve_gc_local(function).initialize(
                        schema
                            .struct_type::<crate::gc_types::StoredValue>()
                            .from_value(&value, function),
                        function,
                    ),
                );
                value.reference().clear(function);
            }
            value.clear(function);
            let list = self.emit_argument_vector_from_snapshots(&stored, function);
            for value in stored.into_iter().rev() {
                value.clear(function);
            }
            return Ok(list);
        }
        let construction = ArgumentListConstruction::new(self, function);
        for argument in arguments {
            match &argument.expr {
                ExprIr::SpreadArgument(source) => {
                    self.emit_spread_argument_into_list(&source.value, &construction, function)?
                }
                ExprIr::CapturedArgumentList(captured) => {
                    let list = self.emit_load_captured_argument_list(captured, function)?;
                    construction.append_list(&list, self, function);
                    list.clear(function);
                }
                _ => {
                    let value = schema.reserve_value_local(function);
                    self.compile_expr_to_value(argument, &value, function)?;
                    self.emit_propagate_current_throw_if_needed(function);
                    construction.append(&value, schema, function);
                    value.clear(function);
                }
            }
        }
        Ok(construction.finish(self, function))
    }

    /// Invoke a private object algorithm in the executing function's Realm.
    /// Its code identity is fixed by this closed domain, independently of any
    /// public property mutation. Internal recursion must retain the originating
    /// builtin Realm when it creates an error or a fresh result object.
    pub(crate) fn emit_native_object_algorithm_call(
        &mut self,
        algorithm: NativeObjectAlgorithm,
        args: &[&crate::gc_types::ValueLocals],
        result: &crate::gc_types::CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let builtin = match algorithm {
            NativeObjectAlgorithm::GetOwnPropertyDescriptor => {
                StandardBuiltinId::ObjectGetOwnPropertyDescriptor
            }
            NativeObjectAlgorithm::OwnKeys => StandardBuiltinId::ReflectOwnKeys,
        };
        let meta = self
            .functions
            .get(&builtin.function_id())
            .cloned()
            .ok_or_else(|| EmitError::unsupported("missing rooted native object algorithm"))?;
        let schema = self.runtime_schema();
        let capture = schema
            .reserve_gc_local::<crate::gc_types::BuiltinClosureCapture, crate::gc_types::Nullable>(
                function,
            )
            .initialize_null(schema, function);
        let realm = self.emit_execution_realm(function);
        let context = self.emit_realm_function_materialization_context_from_realm(&realm, function);
        let callable = schema.reserve_gc_local(function).initialize(
            self.emit_function_value_payload_in_realm_with_capture(
                &meta, &context, &capture, function,
            )?,
            function,
        );
        self.release_realm_function_materialization_context(context, function);
        realm.clear(function);
        let callee = schema.reserve_value_local(function);
        let receiver = schema.reserve_value_local(function);
        callee.set_reference(&callable, schema, function);
        receiver.set_undefined(function);
        let arguments = self.emit_pre_evaluated_arg_vector(args, function);
        self.emit_function_or_proxy_call_with_argv(
            &callee, &receiver, &arguments, result, function,
        )?;
        arguments.clear(function);
        receiver.clear(function);
        callee.clear(function);
        callable.clear(function);
        capture.clear(function);
        Ok(())
    }

    pub(crate) fn emit_direct_js_call(
        &mut self,
        meta: &WasmFunctionMeta,
        this_value: Option<&crate::gc_types::ValueLocals>,
        args: &[&crate::gc_types::ValueLocals],
        result: &crate::gc_types::CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let arguments = self.emit_pre_evaluated_arg_vector(args, function);
        self.emit_direct_js_call_with_argv(meta, this_value, &arguments, result, function)?;
        arguments.clear(function);
        Ok(())
    }

    pub(crate) fn emit_direct_js_call_with_argv(
        &mut self,
        meta: &WasmFunctionMeta,
        this_value: Option<&crate::gc_types::ValueLocals>,
        arguments: &crate::gc_types::GcLocal<crate::gc_types::ValueArray>,
        result: &crate::gc_types::CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let callable = schema
            .reserve_gc_local(function)
            .initialize(self.emit_function_value_payload(meta, function)?, function);
        let value = schema.reserve_value_local(function);
        value.set_reference(&callable, schema, function);
        self.emit_function_handle_call_with_argv_inner(
            &value,
            this_value,
            arguments,
            result,
            PropagateCallThrow::LeaveInCompletion,
            function,
        )?;
        value.clear(function);
        callable.clear(function);
        Ok(())
    }

    pub(crate) fn emit_indirect_call_from_locals(
        &mut self,
        callee: &crate::gc_types::ValueLocals,
        this_value: Option<&crate::gc_types::ValueLocals>,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        match this_value {
            Some(value) => receiver.copy_from(value, function),
            None => receiver.set_undefined(function),
        }
        let list = self.emit_call_args_vector(arguments, function)?;
        self.emit_indirect_call_with_argv_to_value(
            callee,
            &receiver,
            &list,
            continuation,
            output,
            function,
        )?;
        list.clear(function);
        receiver.clear(function);
        Ok(())
    }

    fn emit_indirect_call_with_argv_to_value(
        &mut self,
        callee: &crate::gc_types::ValueLocals,
        receiver: &crate::gc_types::ValueLocals,
        list: &crate::gc_types::GcLocal<crate::gc_types::ValueArray>,
        continuation: &CallContinuation,
        output: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let result = schema.reserve_completion(function);
        match continuation {
            CallContinuation::Continue => self
                .emit_function_or_proxy_call_with_argv(callee, receiver, list, &result, function)?,
            CallContinuation::Return => {
                self.emit_prepared_tail_call(callee, receiver, list, function)?;
            }
        }
        // A normal call supplies an expression value, not a new statement
        // completion. Only the abrupt path needs the whole completion tuple.
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&result, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_propagate_current_throw_if_needed(function);
        output.copy_from(result.value(), function);
        result.clear(function);
        Ok(())
    }

    pub(crate) fn emit_method_call(
        &mut self,
        receiver_expression: &TypedExpr,
        key: &PropertyKeyIr,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let callee = schema.reserve_value_local(function);
        self.compile_expr_to_value(receiver_expression, &receiver, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.compile_property_read_from_locals(
            receiver_expression,
            key,
            &receiver,
            &callee,
            function,
        )?;
        self.emit_propagate_current_throw_if_needed(function);
        self.emit_indirect_call_from_locals(
            &callee,
            Some(&receiver),
            arguments,
            continuation,
            output,
            function,
        )?;
        callee.clear(function);
        receiver.clear(function);
        Ok(())
    }

    pub(crate) fn emit_call(
        &mut self,
        name: &str,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let callee = schema.reserve_value_local(function);
        let receiver = schema.reserve_value_local(function);
        receiver.set_undefined(function);
        if let Some(storage) = self.lookup_binding(name) {
            self.read_binding_to_locals(storage, &callee, function)?;
        } else {
            let key = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference(name, function)?,
                function,
            );
            let reference = self.emit_resolve_environment_identifier(
                &key,
                if self.strict {
                    Strictness::Strict
                } else {
                    Strictness::Sloppy
                },
                function,
            )?;
            self.emit_environment_identifier_get(
                &reference,
                crate::environments::environment_reference::EnvironmentIdentifierRead::Value,
                &callee,
                function,
            )?;
            self.emit_environment_identifier_call_base(&reference, &receiver, function);
            self.release_environment_identifier_reference(reference, function);
            key.clear(function);
        }
        self.emit_propagate_current_throw_if_needed(function);
        self.emit_indirect_call_from_locals(
            &callee,
            Some(&receiver),
            arguments,
            continuation,
            output,
            function,
        )?;
        receiver.clear(function);
        callee.clear(function);
        Ok(())
    }
}
