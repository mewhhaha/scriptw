//! Actual scalar/reference function signatures for the single GC backend.
//!
//! A row owns its parameter/result shape and registration order. Concrete
//! references resolve only through the matching recursive runtime registry.

use crate::gc_types::{AbiType, GcLayout, GcLayoutRegistry, GcNullability};
use wasm_encoder::ValType;

pub(crate) struct StaticSignatureDefinition {
    parameters: Vec<ValType>,
    results: Vec<ValType>,
}
impl StaticSignatureDefinition {
    pub(crate) fn from_abi(
        parameters: Vec<AbiType>,
        results: Vec<AbiType>,
        registry: &GcLayoutRegistry,
    ) -> Self {
        Self {
            parameters: parameters.iter().map(|ty| ty.resolve(registry)).collect(),
            results: results.iter().map(|ty| ty.resolve(registry)).collect(),
        }
    }
    pub(crate) fn parameters(&self) -> &[ValType] {
        &self.parameters
    }
    pub(crate) fn results(&self) -> &[ValType] {
        &self.results
    }
}

macro_rules! static_signature_domain {
    ($( $signature:ident => ($parameters:expr, $results:expr) ),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u32)]
        pub(crate) enum StaticSignature { $( $signature, )+ }
        impl StaticSignature {
            pub(crate) const ALL: &'static [Self] = &[ $( Self::$signature, )+ ];
            pub(crate) const fn type_index(self) -> u32 { self as u32 }
            pub(crate) const fn parameter_types(self) -> &'static [AbiType] {
                match self { $( Self::$signature => &$parameters, )+ }
            }
            pub(crate) const fn result_types(self) -> &'static [AbiType] {
                match self { $( Self::$signature => &$results, )+ }
            }
            pub(crate) const fn parameter_count(self) -> usize {
                self.parameter_types().len()
            }
            pub(crate) const fn requires_runtime_group(self) -> bool {
                has_runtime_reference(self.parameter_types())
                    || has_runtime_reference(self.result_types())
            }
            const fn singleton_signatures_are_first() -> bool {
                let mut recursive_seen = false;
                let mut index = 0;
                while index < Self::ALL.len() {
                    if Self::ALL[index].requires_runtime_group() {
                        recursive_seen = true;
                    } else if recursive_seen {
                        return false;
                    }
                    index += 1;
                }
                true
            }
            pub(crate) fn definition(self, registry: &GcLayoutRegistry) -> StaticSignatureDefinition {
                StaticSignatureDefinition {
                    parameters: self.parameter_types().iter().map(|ty| ty.resolve(registry)).collect(),
                    results: self.result_types().iter().map(|ty| ty.resolve(registry)).collect(),
                }
            }
        }
    };
}

use AbiType::{EqRef, ExternRef, Gc, F64, I32, I64};
use GcNullability::{NonNullable, Nullable};

const fn has_runtime_reference(types: &[AbiType]) -> bool {
    let mut index = 0;
    while index < types.len() {
        match types[index] {
            Gc(..) => return true,
            I32 | I64 | F64 | EqRef | ExternRef(_) => {}
        }
        index += 1;
    }
    false
}

/// The actual ordered completion ABI, shared by all JS callable roles and Main.
pub(crate) const COMPLETION_TYPES: [AbiType; 5] = [I32, I64, EqRef, I32, I32];

static_signature_domain! {
    Main => ([], COMPLETION_TYPES),
    TransientByteAlloc => ([I64], [I64]),
    HostPrint => ([I32, I32], []),
    HostNumberPow => ([F64, F64], [F64]),
    HostAgentCanSuspend => ([], [I32]),
    HostMonotonicClockNanos => ([], [I64]),
    HostSleepNanos => ([I64], []),
    HostAgentCall => ([I64, I64, I64], [I64]),
    HostWallClockMillis => ([], [F64]),
    HostMathUnary => ([F64], [F64]),
    HostCollectGc => ([], []),
    HostSharedBufferAllocate => ([I64, I64, I32], [ExternRef(Nullable)]),
    HostSharedBufferBase => ([ExternRef(NonNullable)], [I64]),
    HostSharedBufferLength => ([ExternRef(NonNullable)], [I64]),
    HostSharedBufferMaximum => ([ExternRef(NonNullable)], [I64]),
    HostSharedBufferGrowable => ([ExternRef(NonNullable)], [I32]),
    HostSharedBufferGrow => ([ExternRef(NonNullable), I64], [I32]),
    // Under one host lock, compare the expected word before publishing a waiter.
    // Zero means NotEqual; positive values are owned native waiter identities.
    // The final argument is the same store-relative deadline retained by GC.
    HostRegisterAsyncWaiter => ([ExternRef(NonNullable), I64, I32, I64, I64], [I64]),
    HostNotifyAsyncWaiters => ([ExternRef(NonNullable), I64, I32, I64], [I64]),
    // One native FIFO owns blocking and asynchronous waiters on each byte resource.
    HostSharedBufferWait => ([ExternRef(NonNullable), I64, I32, I64, I64], [I32]),

    // Weak operands cross as real rooted eqrefs and native slot resources.
    HostWeakMapAllocate => ([], [ExternRef(NonNullable)]),
    HostWeakSetAllocate => ([], [ExternRef(NonNullable)]),
    HostWeakMapSet => ([EqRef, ExternRef(NonNullable), EqRef, EqRef], []),
    HostWeakMapGet => ([ExternRef(NonNullable), EqRef], [EqRef]),
    HostWeakMapHas => ([ExternRef(NonNullable), EqRef], [I32]),
    HostWeakMapDelete => ([ExternRef(NonNullable), EqRef], [I32]),
    HostWeakSetAdd => ([EqRef, ExternRef(NonNullable), EqRef], []),
    HostWeakSetHas => ([ExternRef(NonNullable), EqRef], [I32]),
    HostWeakSetDelete => ([ExternRef(NonNullable), EqRef], [I32]),
    HostWeakRefAllocate => ([EqRef, EqRef], [ExternRef(NonNullable)]),
    HostWeakRefDeref => ([ExternRef(NonNullable)], [EqRef]),
    HostWeakJobFinish => ([], []),
    HostFinalizationTrack => ([EqRef], []),
    HostFinalizationNextRegistry => ([], [EqRef]),
    HostFinalizationCellAllocate => ([EqRef, EqRef, EqRef], [ExternRef(NonNullable)]),
    HostFinalizationTargetAlive => ([ExternRef(NonNullable)], [I32]),
    HostFinalizationTokenMatches => ([ExternRef(NonNullable), EqRef], [I32]),
    HostFinalizationCellRemove => ([ExternRef(NonNullable)], []),
    HostReportCleanupException => ([I32, I64, EqRef, EqRef, EqRef], []),

    // Pure source callbacks retain their own scalar result and decline bit.
    BinaryNumericCallback => ([F64, F64], [I32, F64]),

    // Only signatures with concrete runtime references share its recursion group.
    // Independent host signatures above must canonicalize as singleton types.
    JavaScriptFunction => ([Gc(GcLayout::FunctionObject, NonNullable), I32, I64, EqRef,
        I32, I64, EqRef, I64, Gc(GcLayout::ValueArray, Nullable),
        Gc(GcLayout::RealmRecord, NonNullable),
        Gc(GcLayout::ProgramInstance, NonNullable)], COMPLETION_TYPES),
    GeneratorFunction => ([Gc(GcLayout::FunctionObject, NonNullable), I32, I64, EqRef,
        Gc(GcLayout::GeneratorActivation, NonNullable), I64, Gc(GcLayout::ValueArray, Nullable),
        Gc(GcLayout::ProgramInstance, NonNullable)], COMPLETION_TYPES),
    AsyncFunction => ([Gc(GcLayout::FunctionObject, NonNullable), I32, I64, EqRef,
        Gc(GcLayout::AsyncActivation, NonNullable), I64, Gc(GcLayout::ValueArray, Nullable),
        Gc(GcLayout::ProgramInstance, NonNullable)], COMPLETION_TYPES),
    AsyncGeneratorFunction => ([Gc(GcLayout::FunctionObject, NonNullable), I32, I64, EqRef,
        Gc(GcLayout::AsyncGeneratorActivation, NonNullable), I64, Gc(GcLayout::ValueArray, Nullable),
        Gc(GcLayout::ProgramInstance, NonNullable)], COMPLETION_TYPES),
    PreparedScript => ([Gc(GcLayout::Environment, Nullable), I32, I64, EqRef,
        I32, I64, EqRef, I64, Gc(GcLayout::ValueArray, Nullable),
        Gc(GcLayout::Environment, Nullable), Gc(GcLayout::PrivateEnvironment, Nullable),
        Gc(GcLayout::DirectEvalExecutionContext, Nullable),
        Gc(GcLayout::ProgramInstance, NonNullable)], COMPLETION_TYPES),
    // P publication has no JavaScript entry/Completion or Realm prologue.
    ProgramInitialize => ([], [Gc(GcLayout::ProgramInstance, NonNullable)]),
    // Native same-Store compilation returns either factory Completion or a
    // genuine source SyntaxError message owned by the actual constructor Realm.
    HostCompileDynamicFunction => ([I32, I32, I64, EqRef,
        Gc(GcLayout::ValueArray, NonNullable),
        Gc(GcLayout::FunctionContext, NonNullable), Gc(GcLayout::Environment, Nullable)],
        [I32, I64, EqRef, I32, I32, Gc(GcLayout::StringValue, Nullable)]),
    // Independent global source compilation cannot borrow direct-eval context.
    HostCompileGlobalScript => ([I32, Gc(GcLayout::StringValue, NonNullable),
        Gc(GcLayout::RealmRecord, NonNullable), Gc(GcLayout::ProgramInstance, NonNullable)],
        [I32, I64, EqRef, I32, I32, I32, Gc(GcLayout::StringValue, Nullable)]),
    HostCompileDirectEval => ([Gc(GcLayout::StringValue, NonNullable),
        Gc(GcLayout::ImmutableByteArray, NonNullable), Gc(GcLayout::RealmRecord, NonNullable),
        Gc(GcLayout::Environment, Nullable), Gc(GcLayout::Environment, Nullable),
        Gc(GcLayout::PrivateEnvironment, Nullable), I32, I64, EqRef, I32, I64, EqRef,
        Gc(GcLayout::DirectEvalExecutionContext, NonNullable), Gc(GcLayout::ProgramInstance, NonNullable)],
        [I32, I64, EqRef, I32, I32, I32, Gc(GcLayout::StringValue, Nullable)]),
    HostByteArrayAllocate => ([I32], [Gc(GcLayout::ByteArray, Nullable)]),
    HostIntlProviderCall => ([Gc(GcLayout::ByteArray, NonNullable)], [Gc(GcLayout::ByteArray, Nullable)]),
    HostSystemTimeZoneSnapshot => ([], [Gc(GcLayout::ByteArray, NonNullable)]),
    // Null bytes select ToInt32; canonical decimal bytes select an exact BigInt.
    HostAgentBroadcastResource => ([ExternRef(NonNullable), I64, Gc(GcLayout::ByteArray, Nullable)], [I64]),
    HostAgentReceiveResource => ([], [ExternRef(Nullable), I64, Gc(GcLayout::ByteArray, Nullable)]),
}

// Registration emits the independent prefix before the mutually recursive tail.
// Deriving membership from actual operands prevents a new signature from
// silently acquiring a different canonical type or shifting its declared index.
const _: () = assert!(StaticSignature::singleton_signatures_are_first());

#[cfg(test)]
mod program_ownership_tests;

#[cfg(test)]
mod native_global_script_tests;
