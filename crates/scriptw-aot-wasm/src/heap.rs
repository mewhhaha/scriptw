//! Runtime scalar domains and private byte transport; semantic records use Wasm GC.

use super::*;

pub(crate) const WASM_PAGE_SIZE: u64 = 65_536;
pub(crate) const STATIC_DATA_OFFSET: u32 = 4096;
pub(crate) const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
pub(crate) const PROPERTY_KEY_SYMBOL_MARKER: u64 = 1 << 63;

/// Private linear-memory wire/scratch storage only. Semantic values use GC.
pub(crate) fn emit_transient_byte_alloc_helper_function() -> Function {
    const SIZE_LOCAL: u32 = 0;
    const ALLOC_LOCAL: u32 = 1;
    const END_LOCAL: u32 = 2;
    const ALIGNED_SIZE_LOCAL: u32 = 3;

    let mut function = Function::with_parameters(
        LocalDeclarations::from_types(std::iter::repeat_n(ValType::I64, 3)),
        vec![ValType::I64],
    );

    function.instruction(&Instruction::LocalGet(SIZE_LOCAL));
    function.instruction(&Instruction::I64Const(7));
    function.instruction(&Instruction::I64Add);
    function.instruction(&Instruction::LocalSet(ALIGNED_SIZE_LOCAL));

    function.instruction(&Instruction::LocalGet(ALIGNED_SIZE_LOCAL));
    function.instruction(&Instruction::LocalGet(SIZE_LOCAL));
    function.instruction(&Instruction::I64LtU);
    function.instruction(&Instruction::If(BlockType::Empty));
    function.instruction(&Instruction::Unreachable);
    function.instruction(&Instruction::End);

    function.instruction(&Instruction::LocalGet(ALIGNED_SIZE_LOCAL));
    function.instruction(&Instruction::I64Const(-8));
    function.instruction(&Instruction::I64And);
    function.instruction(&Instruction::LocalSet(ALIGNED_SIZE_LOCAL));

    function.instruction(&Instruction::GlobalGet(PRIVATE_BYTE_CURSOR_GLOBAL_INDEX));
    function.instruction(&Instruction::LocalSet(ALLOC_LOCAL));
    function.instruction(&Instruction::LocalGet(ALLOC_LOCAL));
    function.instruction(&Instruction::LocalGet(ALIGNED_SIZE_LOCAL));
    function.instruction(&Instruction::I64Add);
    function.instruction(&Instruction::LocalSet(END_LOCAL));

    function.instruction(&Instruction::LocalGet(END_LOCAL));
    function.instruction(&Instruction::LocalGet(ALLOC_LOCAL));
    function.instruction(&Instruction::I64LtU);
    function.instruction(&Instruction::If(BlockType::Empty));
    function.instruction(&Instruction::Unreachable);
    function.instruction(&Instruction::End);

    // The return value becomes a memory32 address at every consumer. The
    // one-past end may equal 2^32, but the returned allocation start may not.
    // Validate both before the page count is narrowed or the cursor commits.
    function.instruction(&Instruction::LocalGet(ALLOC_LOCAL));
    function.instruction(&Instruction::I64Const(u32::MAX as i64));
    function.instruction(&Instruction::I64GtU);
    function.instruction(&Instruction::If(BlockType::Empty));
    function.instruction(&Instruction::Unreachable);
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::LocalGet(END_LOCAL));
    function.instruction(&Instruction::I64Const(u32::MAX as i64 + 1));
    function.instruction(&Instruction::I64GtU);
    function.instruction(&Instruction::If(BlockType::Empty));
    function.instruction(&Instruction::Unreachable);
    function.instruction(&Instruction::End);

    function.instruction(&Instruction::LocalGet(END_LOCAL));
    function.instruction(&Instruction::MemorySize(0));
    function.instruction(&Instruction::I64ExtendI32U);
    function.instruction(&Instruction::I64Const(WASM_PAGE_SIZE as i64));
    function.instruction(&Instruction::I64Mul);
    function.instruction(&Instruction::I64GtU);
    function.instruction(&Instruction::If(BlockType::Empty));
    function.instruction(&Instruction::LocalGet(END_LOCAL));
    function.instruction(&Instruction::MemorySize(0));
    function.instruction(&Instruction::I64ExtendI32U);
    function.instruction(&Instruction::I64Const(WASM_PAGE_SIZE as i64));
    function.instruction(&Instruction::I64Mul);
    function.instruction(&Instruction::I64Sub);
    function.instruction(&Instruction::I64Const((WASM_PAGE_SIZE - 1) as i64));
    function.instruction(&Instruction::I64Add);
    function.instruction(&Instruction::I64Const(WASM_PAGE_SIZE as i64));
    function.instruction(&Instruction::I64DivU);
    function.instruction(&Instruction::I32WrapI64);
    function.instruction(&Instruction::MemoryGrow(0));
    function.instruction(&Instruction::I32Const(-1));
    function.instruction(&Instruction::I32Eq);
    function.instruction(&Instruction::If(BlockType::Empty));
    function.instruction(&Instruction::Unreachable);
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::End);

    function.instruction(&Instruction::LocalGet(END_LOCAL));
    function.instruction(&Instruction::GlobalSet(PRIVATE_BYTE_CURSOR_GLOBAL_INDEX));
    function.instruction(&Instruction::LocalGet(ALLOC_LOCAL));
    function.instruction(&Instruction::End);
    function
}

macro_rules! runtime_scalar_domain {
    ($name:ident, $first_word:literal, { $($variant:ident = $word:literal),+ $(,)? }) => {
        runtime_scalar_domain!(@define pub(crate), $name, $first_word, {
            $($variant = $word),+
        });
    };
    (@define $method_visibility:vis, $name:ident, $first_word:literal, { $($variant:ident = $word:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum $name {
            $($variant),+
        }

        impl $name {
            $method_visibility const ALL: [Self; runtime_scalar_domain!(@count $($variant),+)] =
                [$(Self::$variant),+];

            $method_visibility const fn word(self) -> u64 {
                match self {
                    $(Self::$variant => $word),+
                }
            }
        }

        const _: () = {
            let all = $name::ALL;
            let mut index = 0;
            while index < all.len() {
                assert!(all[index].word() == ($first_word as u64) + index as u64);
                index += 1;
            }
        };
    };
    (@count $($variant:ident),+) => {
        <[()]>::len(&[$(runtime_scalar_domain!(@unit $variant)),+])
    };
    (@unit $variant:ident) => { () };
}

// Initialization of one Realm's complete canonical Temporal graph.
runtime_scalar_domain!(TemporalInitializationState, 0, {
    Dormant = 0,
    Building = 1,
    Ready = 2,
});

// Initialization of one Realm's complete canonical Intl graph.
runtime_scalar_domain!(IntlInitializationState, 0, {
    Dormant = 0,
    Building = 1,
    Ready = 2,
});

// Initialization of one Realm's complete canonical binary-data graph.
runtime_scalar_domain!(BinaryDataInitializationState, 0, {
    Dormant = 0,
    Building = 1,
    Ready = 2,
});

// Only public constructors can inhabit a deferred global recipe. The abstract
// %TypedArray% remains privately rooted in the defining Realm's intrinsic table.
runtime_scalar_domain!(BinaryDataConstructor, 0, {
    ArrayBuffer = 0,
    SharedArrayBuffer = 1,
    DataView = 2,
    Float64Array = 3,
    Float32Array = 4,
    Float16Array = 5,
    Int32Array = 6,
    Int16Array = 7,
    Int8Array = 8,
    Uint32Array = 9,
    Uint16Array = 10,
    Uint8Array = 11,
    Uint8ClampedArray = 12,
    BigInt64Array = 13,
    BigUint64Array = 14,
});

impl BinaryDataConstructor {
    pub(crate) const fn from_builtin(builtin: scriptw_ir::StandardBuiltinId) -> Option<Self> {
        match builtin {
            scriptw_ir::StandardBuiltinId::ArrayBufferConstructor => Some(Self::ArrayBuffer),
            scriptw_ir::StandardBuiltinId::SharedArrayBufferConstructor => {
                Some(Self::SharedArrayBuffer)
            }
            scriptw_ir::StandardBuiltinId::DataViewConstructor => Some(Self::DataView),
            scriptw_ir::StandardBuiltinId::Float64ArrayConstructor => Some(Self::Float64Array),
            scriptw_ir::StandardBuiltinId::Float32ArrayConstructor => Some(Self::Float32Array),
            scriptw_ir::StandardBuiltinId::Float16ArrayConstructor => Some(Self::Float16Array),
            scriptw_ir::StandardBuiltinId::Int32ArrayConstructor => Some(Self::Int32Array),
            scriptw_ir::StandardBuiltinId::Int16ArrayConstructor => Some(Self::Int16Array),
            scriptw_ir::StandardBuiltinId::Int8ArrayConstructor => Some(Self::Int8Array),
            scriptw_ir::StandardBuiltinId::Uint32ArrayConstructor => Some(Self::Uint32Array),
            scriptw_ir::StandardBuiltinId::Uint16ArrayConstructor => Some(Self::Uint16Array),
            scriptw_ir::StandardBuiltinId::Uint8ArrayConstructor => Some(Self::Uint8Array),
            scriptw_ir::StandardBuiltinId::Uint8ClampedArrayConstructor => {
                Some(Self::Uint8ClampedArray)
            }
            scriptw_ir::StandardBuiltinId::BigInt64ArrayConstructor => Some(Self::BigInt64Array),
            scriptw_ir::StandardBuiltinId::BigUint64ArrayConstructor => Some(Self::BigUint64Array),
            _ => None,
        }
    }

    pub(crate) const fn builtin(self) -> scriptw_ir::StandardBuiltinId {
        match self {
            Self::ArrayBuffer => scriptw_ir::StandardBuiltinId::ArrayBufferConstructor,
            Self::SharedArrayBuffer => scriptw_ir::StandardBuiltinId::SharedArrayBufferConstructor,
            Self::DataView => scriptw_ir::StandardBuiltinId::DataViewConstructor,
            Self::Float64Array => scriptw_ir::StandardBuiltinId::Float64ArrayConstructor,
            Self::Float32Array => scriptw_ir::StandardBuiltinId::Float32ArrayConstructor,
            Self::Float16Array => scriptw_ir::StandardBuiltinId::Float16ArrayConstructor,
            Self::Int32Array => scriptw_ir::StandardBuiltinId::Int32ArrayConstructor,
            Self::Int16Array => scriptw_ir::StandardBuiltinId::Int16ArrayConstructor,
            Self::Int8Array => scriptw_ir::StandardBuiltinId::Int8ArrayConstructor,
            Self::Uint32Array => scriptw_ir::StandardBuiltinId::Uint32ArrayConstructor,
            Self::Uint16Array => scriptw_ir::StandardBuiltinId::Uint16ArrayConstructor,
            Self::Uint8Array => scriptw_ir::StandardBuiltinId::Uint8ArrayConstructor,
            Self::Uint8ClampedArray => scriptw_ir::StandardBuiltinId::Uint8ClampedArrayConstructor,
            Self::BigInt64Array => scriptw_ir::StandardBuiltinId::BigInt64ArrayConstructor,
            Self::BigUint64Array => scriptw_ir::StandardBuiltinId::BigUint64ArrayConstructor,
        }
    }

    pub(crate) const fn slot(self) -> crate::functions::NonArrayRealmIntrinsicSlot {
        match self {
            Self::ArrayBuffer => {
                crate::functions::NonArrayRealmIntrinsicSlot::ArrayBufferConstructor
            }
            Self::SharedArrayBuffer => {
                crate::functions::NonArrayRealmIntrinsicSlot::SharedArrayBufferConstructor
            }
            Self::DataView => crate::functions::NonArrayRealmIntrinsicSlot::DataViewConstructor,
            Self::Float64Array => {
                crate::functions::NonArrayRealmIntrinsicSlot::Float64ArrayConstructor
            }
            Self::Float32Array => {
                crate::functions::NonArrayRealmIntrinsicSlot::Float32ArrayConstructor
            }
            Self::Float16Array => {
                crate::functions::NonArrayRealmIntrinsicSlot::Float16ArrayConstructor
            }
            Self::Int32Array => crate::functions::NonArrayRealmIntrinsicSlot::Int32ArrayConstructor,
            Self::Int16Array => crate::functions::NonArrayRealmIntrinsicSlot::Int16ArrayConstructor,
            Self::Int8Array => crate::functions::NonArrayRealmIntrinsicSlot::Int8ArrayConstructor,
            Self::Uint32Array => {
                crate::functions::NonArrayRealmIntrinsicSlot::Uint32ArrayConstructor
            }
            Self::Uint16Array => {
                crate::functions::NonArrayRealmIntrinsicSlot::Uint16ArrayConstructor
            }
            Self::Uint8Array => crate::functions::NonArrayRealmIntrinsicSlot::Uint8ArrayConstructor,
            Self::Uint8ClampedArray => {
                crate::functions::NonArrayRealmIntrinsicSlot::Uint8ClampedArrayConstructor
            }
            Self::BigInt64Array => {
                crate::functions::NonArrayRealmIntrinsicSlot::BigInt64ArrayConstructor
            }
            Self::BigUint64Array => {
                crate::functions::NonArrayRealmIntrinsicSlot::BigUint64ArrayConstructor
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisposableStackState {
    Pending,
    Disposed,
}

impl DisposableStackState {
    pub(crate) const fn word(self) -> u64 {
        match self {
            Self::Pending => 0,
            Self::Disposed => 1,
        }
    }
}

/// The complete synchronous resource-entry domain. Nullish `use` values do
/// not create an entry, so the async stack's `Empty` kind has no sync analogue.
runtime_scalar_domain!(DisposableStackEntryKind, 0, {
    Use = 0,
    Adopt = 1,
    Defer = 2,
});

/// The only three call conventions a synchronous disposal entry can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisposableStackDisposeCall {
    ResourceReceiver,
    UndefinedReceiverWithResourceArgument,
    UndefinedReceiverNoArguments,
}

impl DisposableStackEntryKind {
    pub(crate) const fn dispose_call(self) -> DisposableStackDisposeCall {
        match self {
            Self::Use => DisposableStackDisposeCall::ResourceReceiver,
            Self::Adopt => DisposableStackDisposeCall::UndefinedReceiverWithResourceArgument,
            Self::Defer => DisposableStackDisposeCall::UndefinedReceiverNoArguments,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncDisposableStackState {
    Pending,
    Disposed,
}

impl AsyncDisposableStackState {
    /// The scalar code accepted by the GC state codec.
    pub(crate) const fn word(self) -> u64 {
        match self {
            Self::Pending => 0,
            Self::Disposed => 1,
        }
    }
}

// The shared declaration owns every entry and its code. Adding a new entry
// extends ALL and requires the exhaustive dispose_call policy below to change.
// Empty records request the final Await when no method Await has occurred.
runtime_scalar_domain!(AsyncDisposableStackEntryKind, 0, {
    Use = 0,
    Adopt = 1,
    Defer = 2,
    Empty = 3,
});

/// How the disposal walk calls one entry.
///
/// "No call at all" is spelled as the `None` of the [`Option`] returned by
/// [`AsyncDisposableStackEntryKind::dispose_call`], not as a variant here, so
/// the emitter's match over the shapes has no arm it must prove unreachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncDisposableStackDisposeCall {
    /// `Call(method, V)` — the resource is the receiver, no arguments.
    ResourceReceiver,
    /// `Call(onDisposeAsync, undefined, « V »)` — the resource is the sole
    /// argument.
    UndefinedReceiverWithResourceArgument,
    /// `Call(onDisposeAsync, undefined, « »)`.
    UndefinedReceiverNoArguments,
}

/// The private lifecycle of one activation-backed async DisposeCapability.
///
/// This domain is deliberately distinct from [`AsyncDisposableStackState`]: a
/// lexical `await using` scope must remain parked in `Disposing` across each
/// Await, while the user-visible stack only exposes pending/disposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivationAsyncDisposeCapabilityState {
    Pending,
    Disposing,
    Disposed,
}

impl ActivationAsyncDisposeCapabilityState {
    pub(crate) const fn word(self) -> u64 {
        match self {
            Self::Pending => 0,
            Self::Disposing => 1,
            Self::Disposed => 2,
        }
    }
}

/// Disposal methods retained in one async lexical resource capability.
///
/// `SyncFallbackMethod` cannot collapse into `AsyncMethod`: its unobservable
/// spec wrapper ignores a normal return (including a thenable) and converts a
/// synchronous throw into a rejected Promise before the disposal Await.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivationAsyncDisposeEntryKind {
    Empty,
    AsyncMethod,
    SyncFallbackMethod,
    /// A genuine `using` entry in a scope containing `await using`. Its return
    /// is ignored directly; it is not the await-using fallback wrapper.
    SyncMethod,
}

impl ActivationAsyncDisposeEntryKind {
    pub(crate) const ALL: [Self; 4] = [
        Self::Empty,
        Self::AsyncMethod,
        Self::SyncFallbackMethod,
        Self::SyncMethod,
    ];

    pub(crate) const fn word(self) -> u64 {
        match self {
            Self::Empty => 0,
            Self::AsyncMethod => 1,
            Self::SyncFallbackMethod => 2,
            Self::SyncMethod => 3,
        }
    }
}

impl AsyncDisposableStackEntryKind {
    /// The call convention, or no call for an empty async resource.
    pub(crate) const fn dispose_call(self) -> Option<AsyncDisposableStackDisposeCall> {
        match self {
            Self::Use => Some(AsyncDisposableStackDisposeCall::ResourceReceiver),
            Self::Adopt => {
                Some(AsyncDisposableStackDisposeCall::UndefinedReceiverWithResourceArgument)
            }
            Self::Defer => Some(AsyncDisposableStackDisposeCall::UndefinedReceiverNoArguments),
            Self::Empty => None,
        }
    }
}

// The closed domain stored in a Promise record's `[[PromiseState]]` word.
//
// GC publication accepts this closed state and its exhaustive field codec.
runtime_scalar_domain!(PromiseState, 0, {
    Pending = 0,
    Fulfilled = 1,
    Rejected = 2,
});

/// A terminal direction accepted by Promise settlement producers.
///
/// This is deliberately distinct from [`PromiseState`]: a caller settling a
/// Promise must choose fulfilment or rejection and cannot supply `Pending`.
/// It is also distinct from [`PromiseReactionType`], whose matching wire words
/// belong to a different specification record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromiseSettlement {
    Fulfill,
    Reject,
}

impl PromiseSettlement {
    pub(crate) const fn state(self) -> PromiseState {
        match self {
            Self::Fulfill => PromiseState::Fulfilled,
            Self::Reject => PromiseState::Rejected,
        }
    }

    pub(crate) const fn is_rejected(self) -> bool {
        match self {
            Self::Fulfill => false,
            Self::Reject => true,
        }
    }
}

// The closed domain stored in a Promise reaction record's `type` word.
//
// A reaction is selected for exactly one terminal Promise path. It cannot be
// pending, even though its stable wire words intentionally match the fulfilled
// and rejected Promise-state words. The job runner decodes this domain once;
// no callback shape may reinterpret the raw word independently.
runtime_scalar_domain!(PromiseReactionType, 1, {
    Fulfill = 1,
    Reject = 2,
});

impl PromiseReactionType {
    /// The normalized runtime branch consumed by every reaction callback.
    pub(crate) const fn is_rejected(self) -> bool {
        match self {
            Self::Fulfill => false,
            Self::Reject => true,
        }
    }
}

// The closed domain stored in a Promise reaction record's `callback_kind`
// word.
//
// Promise reactions use the default ECMAScript handler path or a closed
// async, module or Async-from-Sync continuation. Keeping the code and job-realm
// policy on one type means a new continuation cannot be initialized without
// also selecting how it runs and which realm a queued job carries.
runtime_scalar_domain!(PromiseReactionCallbackKind, 0, {
    Default = 0,
    AsyncFunction = 1,
    AsyncGeneratorAwaitReturn = 2,
    AsyncGeneratorAwait = 3,
    AsyncGeneratorYield = 4,
    AsyncGeneratorYieldReturn = 5,
    ModuleBody = 6,
    ModuleJoin = 7,
    AsyncFromSyncIterator = 8,
});

/// Where a Promise reaction job obtains its host job realm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromiseReactionRealmSource {
    /// `GetFunctionRealm(handler)` when the handler is callable, otherwise the
    /// null realm required for an empty Promise reaction handler.
    HandlerOrNull,
    /// The realm captured when an internal async continuation was created.
    Captured,
}

impl PromiseReactionCallbackKind {
    /// The complete realm-selection policy for this reaction shape.
    pub(crate) const fn realm_source(self) -> PromiseReactionRealmSource {
        match self {
            Self::Default => PromiseReactionRealmSource::HandlerOrNull,
            Self::AsyncFunction
            | Self::AsyncGeneratorAwaitReturn
            | Self::AsyncGeneratorAwait
            | Self::AsyncGeneratorYield
            | Self::AsyncGeneratorYieldReturn
            | Self::ModuleBody
            | Self::ModuleJoin
            | Self::AsyncFromSyncIterator => PromiseReactionRealmSource::Captured,
        }
    }
}

// The closed domain stored in a pending Promise job record's `kind` word.
//
// The job drain derives its emitted comparison chain from `ALL` and selects
// the implementation through an exhaustive Rust match. A new job kind
// therefore extends the run-time dispatch and fails to compile until its
// behavior is supplied, rather than falling through as a thenable job.
//
runtime_scalar_domain!(PromiseJobKind, 1, {
    Reaction = 1,
    ResolveThenable = 2,
    FinalizationCleanup = 3,
});

runtime_scalar_domain!(AsyncModuleEntryMode, 0, {
    Allocate = 0,
    Instantiate = 1,
    Execute = 2,
});

runtime_scalar_domain!(ModuleEvaluationState, 0, {
    Linked = 0,
    Evaluating = 1,
    EvaluatingAsync = 2,
    Evaluated = 3,
});

runtime_scalar_domain!(ModuleEvaluationCompletion, 0, {
    Empty = 0,
    Normal = 1,
    Throw = 2,
});

runtime_scalar_domain!(ModuleBodyState, 0, {
    NotStarted = 0,
    Executing = 1,
    Completed = 2,
});

runtime_scalar_domain!(ModuleActivationKind, 0, {
    Synchronous = 0,
    Async = 1,
});

runtime_scalar_domain!(ModuleRequestPhase, 0, {
    Evaluation = 0,
    Defer = 1,
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DescriptorBit {
    Accessor = 0,
    Configurable = 1,
    Writable = 2,
    Enumerable = 3,
}

impl DescriptorBit {
    pub(crate) const fn word(self) -> u32 {
        1u32 << (self as u32)
    }
}

/// Complete kind and attributes published to a GC descriptor.
///
/// The constructors are exactly the two 6.2.6.6 licenses, and `of_accessor`
/// takes no `writable` argument — so the bit pattern `ACCESSOR | WRITABLE`
/// (= 5), an accessor property carrying a stale writable bit that a later
/// accessor-to-data conversion reads back as `writable: true`, has **no
/// constructor**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DescriptorWord(u32);

impl DescriptorWord {
    /// 6.2.6.6 for a data property: `[[Writable]]`, `[[Enumerable]]`,
    /// `[[Configurable]]`, and the accessor bit clear.
    const fn of_data(writable: bool, enumerable: bool, configurable: bool) -> Self {
        let mut bits = 0u32;
        if writable {
            bits |= DescriptorBit::Writable.word();
        }
        if enumerable {
            bits |= DescriptorBit::Enumerable.word();
        }
        if configurable {
            bits |= DescriptorBit::Configurable.word();
        }
        Self(bits)
    }

    /// 6.2.6.6 for an accessor property. 10.1.6.3 steps 6.b and 7 say a
    /// conversion between kinds preserves only `[[Enumerable]]` and
    /// `[[Configurable]]`; there is no `writable` parameter because an accessor
    /// property has no `[[Writable]]` attribute to preserve.
    const fn of_accessor(enumerable: bool, configurable: bool) -> Self {
        let mut bits = DescriptorBit::Accessor.word();
        if enumerable {
            bits |= DescriptorBit::Enumerable.word();
        }
        if configurable {
            bits |= DescriptorBit::Configurable.word();
        }
        Self(bits)
    }

    pub(crate) const fn bits(self) -> u32 {
        self.0
    }

    pub(crate) const fn as_i32(self) -> i32 {
        self.0 as i32
    }
}

/// Static 6.2.6.6 attributes for a complete GC descriptor.
pub(crate) enum StoredPropertyAttributes {
    Data {
        writable: bool,
        enumerable: bool,
        configurable: bool,
    },
    Accessor {
        enumerable: bool,
        configurable: bool,
    },
}

impl StoredPropertyAttributes {
    pub(crate) const fn descriptor_word(self) -> DescriptorWord {
        match self {
            Self::Data {
                writable,
                enumerable,
                configurable,
            } => DescriptorWord::of_data(writable, enumerable, configurable),
            Self::Accessor {
                enumerable,
                configurable,
            } => DescriptorWord::of_accessor(enumerable, configurable),
        }
    }

    pub(crate) const fn descriptor_kind_bits(self) -> u32 {
        self.descriptor_word().bits()
    }
}

/// A **test** against a descriptor word.
///
/// Deliberately a different type from [`DescriptorWord`], with no conversion in
/// either direction: composites like `ACCESSOR | WRITABLE` are illegal as
/// stored values and *legal and needed* as masks — the one at
/// [`DescriptorMask::ACCESSOR_OR_WRITABLE`] asks "is the existing entry a data
/// property that is not writable" in a single `I32And`, and banning the bit
/// pattern outright would break correct code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DescriptorMask(u32);

impl DescriptorMask {
    pub(crate) const ACCESSOR: Self = Self(DescriptorBit::Accessor.word());
    pub(crate) const WRITABLE: Self = Self(DescriptorBit::Writable.word());
    pub(crate) const ENUMERABLE: Self = Self(DescriptorBit::Enumerable.word());
    pub(crate) const CONFIGURABLE: Self = Self(DescriptorBit::Configurable.word());
    /// "The existing entry is a data descriptor **and** is not writable", in
    /// one `I32And`. A legal mask; not a legal word.
    pub(crate) const ACCESSOR_OR_WRITABLE: Self =
        Self(DescriptorBit::Accessor.word() | DescriptorBit::Writable.word());
    /// Bits 0..3: the descriptor kind and the three attributes.
    pub(crate) const KIND_AND_ATTRIBUTES: Self = Self(
        DescriptorBit::Accessor.word()
            | DescriptorBit::Configurable.word()
            | DescriptorBit::Writable.word()
            | DescriptorBit::Enumerable.word(),
    );
    pub(crate) const fn of(bit: DescriptorBit) -> Self {
        Self(bit.word())
    }

    pub(crate) const fn bits(self) -> u32 {
        self.0
    }

    pub(crate) const fn as_i32(self) -> i32 {
        self.0 as i32
    }

    pub(crate) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// The closed `[[GeneratorState]]` domain persisted in a synchronous
/// generator record.
///
/// Its sole physical publication codec is the exhaustive GC field authority;
/// the Rust discriminant does not encode a transport word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeneratorState {
    SuspendedStart,
    Executing,
    Completed,
    SuspendedYield,
}

/// The closed completion kind supplied when a synchronous generator resumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeneratorResumeKind {
    Normal,
    Return,
    Throw,
}

/// The closed Completion Record subset persisted in an async-generator
/// request.
///
/// The semantic declaration order follows the specification operations. The
/// physical codes come from the exhaustive GC completion codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncGeneratorRequestCompletionKind {
    Normal,
    Return,
    Throw,
}

/// The closed `[[AsyncGeneratorState]]` lifecycle stored in an activation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncGeneratorExecutionState {
    SuspendedStart,
    SuspendedYield,
    Executing,
    DrainingQueue,
    Completed,
}

/// The closed backend status stored around an async-generator body invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncGeneratorBodyStatus {
    Idle,
    Running,
    Await,
    Yield,
    Complete,
    Throw,
}

/// The closed completion kind supplied when an async-generator body resumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncGeneratorResumeKind {
    Normal,
    Return,
    Throw,
    Fulfill,
    Reject,
}

impl FunctionBuilder<'_> {
    pub(crate) const fn memarg64(offset: u64) -> MemArg {
        Self::memarg64_in(0, offset)
    }

    pub(crate) const fn memarg64_in(memory_index: u32, offset: u64) -> MemArg {
        MemArg {
            offset,
            align: 3,
            memory_index,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promise_reaction_wire_domains_and_realm_policies_are_stable() {
        assert_eq!(
            PromiseReactionType::ALL.map(PromiseReactionType::word),
            [1, 2]
        );
        assert_eq!(
            PromiseReactionType::ALL.map(PromiseReactionType::is_rejected),
            [false, true]
        );
        assert_eq!(
            PromiseReactionCallbackKind::ALL.map(PromiseReactionCallbackKind::word),
            [0, 1, 2, 3, 4, 5, 6, 7, 8]
        );
        assert_eq!(
            PromiseReactionCallbackKind::ALL.map(PromiseReactionCallbackKind::realm_source),
            [
                PromiseReactionRealmSource::HandlerOrNull,
                PromiseReactionRealmSource::Captured,
                PromiseReactionRealmSource::Captured,
                PromiseReactionRealmSource::Captured,
                PromiseReactionRealmSource::Captured,
                PromiseReactionRealmSource::Captured,
                PromiseReactionRealmSource::Captured,
                PromiseReactionRealmSource::Captured,
                PromiseReactionRealmSource::Captured,
            ]
        );
    }

    #[test]
    fn promise_lifecycle_wire_domain_is_closed() {
        assert_eq!(PromiseState::ALL.map(PromiseState::word), [0, 1, 2]);
        assert_eq!(
            [PromiseSettlement::Fulfill, PromiseSettlement::Reject].map(PromiseSettlement::state),
            [PromiseState::Fulfilled, PromiseState::Rejected]
        );
        assert_eq!(
            [PromiseSettlement::Fulfill, PromiseSettlement::Reject]
                .map(PromiseSettlement::is_rejected),
            [false, true]
        );
    }
}
