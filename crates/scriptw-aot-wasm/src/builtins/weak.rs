//! Weak families use the real copying collector through typed host resources.
use super::super::*;
use crate::functions::OrdinaryDefaultPrototype;
use crate::gc_types::*;
mod collections;
mod constructors;
mod finalization;
mod jobs;
mod references;

pub(super) enum WeakBuiltin {
    WeakMapConstructor,
    WeakMapPrototypeDelete,
    WeakMapPrototypeGet,
    WeakMapPrototypeGetOrInsert,
    WeakMapPrototypeGetOrInsertComputed,
    WeakMapPrototypeHas,
    WeakMapPrototypeSet,
    WeakSetConstructor,
    WeakSetPrototypeAdd,
    WeakSetPrototypeDelete,
    WeakSetPrototypeHas,
    WeakRefConstructor,
    WeakRefPrototypeDeref,
    FinalizationRegistryConstructor,
    FinalizationRegistryPrototypeRegister,
    FinalizationRegistryPrototypeUnregister,
}

#[derive(Clone, Copy)]
pub(super) enum WeakFamily {
    Map,
    Set,
    Ref,
    Registry,
}
impl WeakFamily {
    fn requires_new(self) -> RuntimeErrorMessage {
        match self {
            Self::Map => RuntimeErrorMessage::WEAKMAP_CONSTRUCTOR_REQUIRES_NEW,
            Self::Set => RuntimeErrorMessage::WEAKSET_CONSTRUCTOR_REQUIRES_NEW,
            Self::Ref => RuntimeErrorMessage::WEAKREF_CONSTRUCTOR_REQUIRES_NEW,
            Self::Registry => RuntimeErrorMessage::FINALIZATIONREGISTRY_CONSTRUCTOR_REQUIRES_NEW,
        }
    }
    fn prototype(self) -> OrdinaryDefaultPrototype {
        match self {
            Self::Map => OrdinaryDefaultPrototype::WeakMap,
            Self::Set => OrdinaryDefaultPrototype::WeakSet,
            Self::Ref => OrdinaryDefaultPrototype::WeakRef,
            Self::Registry => OrdinaryDefaultPrototype::FinalizationRegistry,
        }
    }
    fn primitive_receiver_error(self) -> Option<RuntimeErrorMessage> {
        match self {
            Self::Map => Some(RuntimeErrorMessage::WEAKMAP_METHOD_RECEIVER_IS_NOT_AN_OBJECT),
            Self::Set => Some(RuntimeErrorMessage::WEAKSET_METHOD_RECEIVER_IS_NOT_AN_OBJECT),
            Self::Ref | Self::Registry => None,
        }
    }
    fn receiver_error(self) -> RuntimeErrorMessage {
        match self {
            Self::Map => RuntimeErrorMessage::WEAKMAP_METHOD_RECEIVER_DOES_NOT_HAVE_WEAKMAPDATA,
            Self::Set => RuntimeErrorMessage::WEAKSET_METHOD_RECEIVER_DOES_NOT_HAVE_WEAKSETDATA,
            Self::Ref => {
                RuntimeErrorMessage::WEAKREF_PROTOTYPE_DEREF_RECEIVER_DOES_NOT_HAVE_WEAKREFTARGET
            }
            Self::Registry => {
                RuntimeErrorMessage::FINALIZATIONREGISTRY_METHOD_RECEIVER_DOES_NOT_HAVE_CELLS
            }
        }
    }
}
impl FunctionBuilder<'_> {
    fn weak_import(&self, import: GcHostImport) -> Result<DeclaredGcHostImport, EmitError> {
        self.functions.gc_host_imports().get(import).ok_or_else(|| {
            EmitError::unsupported("weak operation requires its collector host import")
        })
    }
    pub(super) fn emit_weak_builtin(
        &mut self,
        builtin: WeakBuiltin,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        use WeakBuiltin::*;
        match builtin {
            WeakMapConstructor => self.emit_weak_map_constructor(f),
            WeakSetConstructor => self.emit_weak_set_constructor(f),
            WeakMapPrototypeGet => self.emit_weak_map_operation(collections::MapOperation::Get, f),
            WeakMapPrototypeHas => self.emit_weak_map_operation(collections::MapOperation::Has, f),
            WeakMapPrototypeDelete => {
                self.emit_weak_map_operation(collections::MapOperation::Delete, f)
            }
            WeakMapPrototypeSet => self.emit_weak_map_operation(collections::MapOperation::Set, f),
            WeakMapPrototypeGetOrInsert => {
                self.emit_weak_map_operation(collections::MapOperation::GetOrInsert, f)
            }
            WeakMapPrototypeGetOrInsertComputed => {
                self.emit_weak_map_operation(collections::MapOperation::GetOrInsertComputed, f)
            }
            WeakSetPrototypeAdd => self.emit_weak_set_operation(collections::SetOperation::Add, f),
            WeakSetPrototypeHas => self.emit_weak_set_operation(collections::SetOperation::Has, f),
            WeakSetPrototypeDelete => {
                self.emit_weak_set_operation(collections::SetOperation::Delete, f)
            }
            WeakRefConstructor => self.emit_weak_ref_constructor(f),
            WeakRefPrototypeDeref => self.emit_weak_ref_deref(f),
            FinalizationRegistryConstructor => self.emit_finalization_constructor(f),
            FinalizationRegistryPrototypeRegister => self.emit_finalization_register(f),
            FinalizationRegistryPrototypeUnregister => self.emit_finalization_unregister(f),
        }
    }
    fn emit_weak_assert(
        &mut self,
        message: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.open_frame(ControlFrameKind::If, f);
        let failed = self.runtime_schema().reserve_completion(f);
        self.emit_throw_current_function_realm_type_error(message, &failed, f)?;
        self.completion().copy_from(&failed, f);
        failed.clear(f);
        self.emit_propagate_current_throw(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    fn emit_weak_receiver<T: JavaScriptReference>(
        &mut self,
        family: WeakFamily,
        f: &mut Function,
    ) -> Result<GcLocal<T>, EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        if let Some(message) = family.primitive_receiver_error() {
            self.emit_is_heap_object_like_tag_i32(receiver.tag(), f);
            f.instruction(&Instruction::I32Eqz);
            self.emit_weak_assert(message, f)?;
        }
        receiver.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<T>(GcNullability::NonNullable).heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.emit_weak_assert(family.receiver_error(), f)?;
        let record = s
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<T>(s, f), f);
        receiver.clear(f);
        Ok(record)
    }
    fn emit_weak_header(
        &mut self,
        family: WeakFamily,
        argument: Option<&ValueLocals>,
        f: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        let s = self.runtime_schema();
        let new_target = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        self.compile_new_target_to_locals(&new_target, f)?;
        new_target.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.emit_weak_assert(family.requires_new(), f)?;
        match family {
            WeakFamily::Map | WeakFamily::Set => {}
            WeakFamily::Ref => {
                self.emit_whole_can_be_held_weakly_i32(argument.expect("target"), f);
                f.instruction(&Instruction::I32Eqz);
                self.emit_weak_assert(
                    RuntimeErrorMessage::WEAKREF_TARGET_CANNOT_BE_HELD_WEAKLY,
                    f,
                )?;
            }
            WeakFamily::Registry => {
                self.emit_is_callable_i32(argument.expect("callback"), f)?;
                f.instruction(&Instruction::I32Eqz);
                self.emit_weak_assert(
                    RuntimeErrorMessage::FINALIZATIONREGISTRY_CLEANUP_CALLBACK_IS_NOT_CALLABLE,
                    f,
                )?;
            }
        }
        self.emit_get_prototype_from_constructor(&new_target, family.prototype(), &pending, f)?;
        self.emit_collection_propagate(&pending, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(pending.value()), f)?,
            f,
        );
        pending.clear(f);
        new_target.clear(f);
        Ok(header)
    }
    /// Leaves the stable identities of a weak collection and of a key that
    /// can be held weakly in `identities`. With `mint`, an object or Symbol
    /// without one gets it now, as a store needs; otherwise its identity
    /// reads as zero, and no weak collection has an entry for it or owned by
    /// it: every store mints both first, and an identity never changes.
    fn emit_weak_entry_identities(
        &mut self,
        owner: GcStackReference<OrdinaryObject>,
        key: &ValueLocals,
        identities: WeakEntryIdentities,
        mint: bool,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let header = s.reserve_gc_local(f).initialize(owner, f);
        self.emit_ordinary_collection_identity(&header, identities.owner, mint, f);
        header.clear(f);
        self.emit_weak_key_identity(key, identities.key, mint, f);
    }
    /// The stable identity of an eligible key, or zero if none has been minted.
    fn emit_weak_key_identity(
        &mut self,
        key: &ValueLocals,
        identity: I64Local,
        mint: bool,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let symbol = s
            .reserve_gc_local(f)
            .initialize(key.cast_reference::<SymbolValue>(s, f), f);
        if mint {
            self.emit_symbol_key_hash(&symbol, identity, f);
        } else {
            s.field(SymbolValueSchema::KEY_HASH_ID)
                .read(&symbol, s, f)
                .store_i64(identity, f);
        }
        symbol.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_object_collection_identity(key, identity, mint, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }
    /// Opens the frame in which a lookup asks the host: both identities are
    /// minted. Outside it the answer is "no entry".
    fn open_weak_entry_lookup(&mut self, identities: WeakEntryIdentities, f: &mut Function) {
        identities.owner.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64Ne);
        identities.key.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64Ne);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
    }
    fn emit_whole_can_be_held_weakly_i32(&mut self, value: &ValueLocals, f: &mut Function) {
        let s = self.runtime_schema();
        let accepted = s.reserve_i32_local(f);
        self.emit_is_heap_object_like_tag_i32(value.tag(), f);
        accepted.store(f);
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let symbol = s
            .reserve_gc_local(f)
            .initialize(value.cast_reference::<SymbolValue>(s, f), f);
        s.struct_type::<SymbolValue>()
            .field(SymbolValueSchema::REGISTRY_KEY)
            .read(&symbol, s, f)
            .reference()
            .is_null(f);
        accepted.store(f);
        symbol.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        accepted.load(f);
        s.release_i32_local(accepted, f);
    }
}
