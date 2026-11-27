//! Typed weak host calls. References travel as references; the identities
//! beside a WeakSet entry's references only say where to look.
use super::*;
mod finalization;

fn resource_operand(resource: &GcLocal<HostResource>, s: &RuntimeSchema, f: &mut Function) {
    let _ = s.field(HostResourceSchema::RESOURCE).read(resource, s, f);
}

fn bind_resource(
    kind: HostResourceKind,
    s: &RuntimeSchema,
    f: &mut Function,
) -> GcStackReference<HostResource> {
    let native = NativeSharedBufferResult::bind(f);
    let resource = s.struct_type::<HostResource>().construct(
        (
            GcOperand::constant(kind),
            GcOperand {
                source: OperandSource::NonNullLocal(native.index),
                shape: PhantomData,
            },
        ),
        f,
    );
    native.clear(f);
    resource
}

/// The objects that own weak collection entries.
pub(crate) trait WeakCollectionObject: GcHeapType {}
impl WeakCollectionObject for WeakSetObject {}

/// The stable identities of a weak collection and of a key, in that order:
/// what the host hashes to find their entry.
#[derive(Clone, Copy)]
pub(crate) struct WeakEntryIdentities {
    pub(crate) owner: I64Local,
    pub(crate) key: I64Local,
}
impl WeakEntryIdentities {
    fn load(self, f: &mut Function) {
        self.owner.load(f);
        self.key.load(f);
    }
}

impl DeclaredGcHostImport {
    /// WeakSet's store. Both identities must already be minted.
    pub(crate) fn put_weak_entry<Owner: WeakCollectionObject>(
        self,
        owner: &GcLocal<Owner>,
        key: &ValueLocals,
        value: Option<&GcLocal<StoredValue>>,
        identities: WeakEntryIdentities,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        assert_eq!(self.import(), GcHostImport::WeakEntryPut);
        owner.load(s, f);
        key.reference().load(f);
        match value {
            Some(value) => {
                value.load(s, f);
            }
            None => {
                f.instruction(&Instruction::RefNull(wasm_encoder::HeapType::Abstract {
                    shared: false,
                    ty: wasm_encoder::AbstractHeapType::Eq,
                }));
            }
        }
        identities.load(f);
        self.emit_call_instruction(f);
    }
    /// Register the private Wasm array that holds interleaved WeakMap key/value pairs.
    pub(crate) fn register_weak_map_table(
        self,
        references: &GcLocal<CompactReferenceArray>,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        assert_eq!(self.import(), GcHostImport::WeakMapRegisterTable);
        references.load(s, f);
        self.emit_call_instruction(f);
    }
    /// Whether a weak collection has the key, for `has`, or had it before
    /// this call removed it, for `delete`.
    pub(crate) fn weak_entry_membership<Owner: WeakCollectionObject>(
        self,
        owner: &GcLocal<Owner>,
        key: &ValueLocals,
        identities: WeakEntryIdentities,
        result: I32Local,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        assert!(matches!(
            self.import(),
            GcHostImport::WeakEntryHas | GcHostImport::WeakEntryDelete
        ));
        owner.load(s, f);
        key.reference().load(f);
        identities.load(f);
        self.emit_call_instruction(f);
        result.store(f);
    }
    pub(crate) fn allocate_weak_ref(
        self,
        owner: &GcLocal<WeakRefObject>,
        target: &ValueLocals,
        s: &RuntimeSchema,
        f: &mut Function,
    ) -> GcStackReference<HostResource> {
        assert_eq!(self.import(), GcHostImport::WeakRefAllocate);
        owner.load(s, f);
        target.reference().load(f);
        self.emit_call_instruction(f);
        bind_resource(HostResourceKind::WeakRef, s, f)
    }
    pub(crate) fn deref_weak_ref(
        self,
        resource: &GcLocal<HostResource>,
        result: &ValueLocals,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        assert_eq!(self.import(), GcHostImport::WeakRefDeref);
        resource_operand(resource, s, f);
        self.emit_call_instruction(f);
        f.instruction(&Instruction::LocalSet(result.reference().index));
    }
}
