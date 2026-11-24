//! The entry of a function body whose builtin facts are proofs.
//!
//! The body was lowered from facts about builtin globals that any earlier
//! statement may have falsified (`scriptw_ir::BuiltinEntryRoleIr`). Before
//! its first effect the entry proves each fact from live state, by reads
//! alone, and on a miss tail-calls the proof-free body of the same source
//! with the parameters it received.
//!
//! A fact about the global binding `name` holds when
//!
//! - the Global Environment Record declares no such name and `name` is an
//!   own data property of its global object: the global identifier read
//!   site's own proof (`environments/global_read_site.rs`), with the names
//!   compared once per name table, so the value read is the one an
//!   identifier read observes;
//! - that value is the current Realm's own intrinsic: a function object
//!   whose immutable native code is the builtin, created for this Realm, or
//!   an ordinary object still holding this Realm's intrinsic rows of the
//!   namespace's layout, or the Realm's global object;
//! - the intrinsic objects the fact's shape reads through still hold their
//!   intrinsic rows, which every edit replaces
//!   (`objects/realm_protectors.rs`): the namespace itself, a constructor
//!   with a complete layout, the protected prototype the constructor's
//!   instances and primitives read (%String.prototype%, %Array.prototype%),
//!   and %Object.prototype% and %Function.prototype%, which every function
//!   fact inherits from.
//!
//! A value the Realm has not made yet is read first: that read of an own data
//! property observes nothing. Every other state, another Realm's intrinsic
//! included, is a miss.
//!
//! An entry that proved every fact keeps the whole proof in a
//! `BuiltinEntryProof`: the Environment and Realm it ran for, the
//! Environment's declarative name table and the builtin fact epoch. The next
//! entry compares those four and reads no fact while they are unchanged:
//! every other state the facts are read from is immutable or has a writer
//! that advances the epoch (`objects/builtin_fact_epoch.rs`). The facts are
//! read again only at the guard's first entry and after the epoch advanced,
//! so an entry costs the same however many facts the guard has.
use super::*;
use crate::emit::ControlTarget;
use crate::environments::environment_reference::EnvironmentIdentifierRead;
use crate::environments::GlobalDeclarativeNames;
use crate::function_entry::PlannedBodyEntry;
use crate::gc_types::{
    BuiltinEntryProof, BuiltinEntryProofSchema, Environment, EnvironmentSchema, ExecutableCodeKind,
    ExecutableCodeSchema, FunctionContextSchema, FunctionObject, FunctionObjectSchema,
    GcI32Constant, GcLocal, GcOperand, GlobalReadSiteRecord, GlobalReadSiteRecordSchema,
    IntrinsicPropertyStorage, IntrinsicPropertyStorageSchema, NamedBinding, NamedBindingTable,
    NonNullable, Nullable, ObjectPropertyStorage, OrdinaryObject, OrdinaryObjectSchema,
    PropertyEntry, PropertyEntrySchema, RealmRecord, RealmRecordSchema, RuntimeSchema,
    SiteRecordGlobal, StoredValue, StoredValueSchema, StringValue, ValueLocals,
};
use crate::intrinsics::IntrinsicPropertyLayoutId;
use crate::objects::ProtectedPrototypeRows;
use scriptw_ir::{BuiltinEntryFactIr, BuiltinNamespaceIr};

/// The guard of the body a builder compiles.
#[derive(Clone, Copy)]
pub(crate) struct BuiltinEntryGuard<'a> {
    facts: &'a [BuiltinEntryFactIr],
    fallback: &'a str,
}

impl<'a> BuiltinEntryGuard<'a> {
    pub(crate) fn admit(function: &'a FunctionIr) -> Option<Self> {
        let (facts, fallback) = function.builtin_entry_guard()?;
        Some(Self {
            facts,
            fallback: fallback.id.as_str(),
        })
    }
}

fn namespace_layout(namespace: BuiltinNamespaceIr) -> IntrinsicPropertyLayoutId {
    match namespace {
        BuiltinNamespaceIr::Math => IntrinsicPropertyLayoutId::Math,
        BuiltinNamespaceIr::Json => IntrinsicPropertyLayoutId::Json,
        BuiltinNamespaceIr::Reflect => IntrinsicPropertyLayoutId::Reflect,
        BuiltinNamespaceIr::Atomics => IntrinsicPropertyLayoutId::Atomics,
    }
}

impl FunctionBuilder<'_> {
    /// Tail-calls the guard's fallback unless every fact holds. Nothing
    /// before this point of the body is observable, and the parameters are
    /// still the caller's.
    pub(crate) fn emit_builtin_entry_guard(
        &mut self,
        guard: BuiltinEntryGuard<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let functions = self.functions;
        let fallback = functions.get(guard.fallback).ok_or_else(|| {
            EmitError::unsupported("compiler invariant: builtin entry guard without its fallback")
        })?;
        let own = self
            .planned_entry()
            .ok_or_else(|| {
                EmitError::unsupported(
                    "compiler invariant: builtin entry guard outside a source body",
                )
            })?
            .clone();
        if !matches!(own.body(), PlannedBodyEntry::Ordinary(_))
            || !matches!(fallback.entry.body(), PlannedBodyEntry::Ordinary(_))
        {
            return Err(EmitError::unsupported(
                "compiler invariant: builtin entry guard requires ordinary bodies",
            ));
        }
        let hold = self.open_frame(ControlFrameKind::Block, function);
        let miss = self.open_frame(ControlFrameKind::Block, function);
        self.emit_builtin_entry_facts_or_branch(guard.facts, miss, function)?;
        self.emit_branch_to_target(hold, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        // Only after an intrinsic was replaced or edited.
        function.rarely_reached();
        let parameters = u32::try_from(function.parameter_types().len())
            .map_err(|_| EmitError::unsupported("function parameters exceed u32"))?;
        for parameter in 0..parameters {
            function.instruction(&Instruction::LocalGet(parameter));
        }
        // Both bodies of a pair take one argument layout
        // (`LocalArgumentsPlan::for_function`).
        match (own.local_arguments(), fallback.entry.local_arguments()) {
            (Some(own), Some(other)) if own.slot_count() == other.slot_count() => {
                other.emit_return_call_instruction(function);
            }
            (None, None) => fallback.entry.emit_direct_return_call_instruction(function),
            _ => {
                return Err(EmitError::unsupported(
                    "compiler invariant: builtin entry guard pair disagrees on its argument layout",
                ));
            }
        }
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_builtin_entry_facts_or_branch(
        &mut self,
        facts: &[BuiltinEntryFactIr],
        miss: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let global = self.emit_source_global_environment_to_local(function);
        let environment = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize(global.load(schema, function).nullable(), function);
        let realm = self.load_current_realm(function);
        let whole = self.next_site_record::<BuiltinEntryProof>()?;
        let proved = self.open_frame(ControlFrameKind::Block, function);
        let unproved = self.open_frame(ControlFrameKind::Block, function);
        self.emit_builtin_entry_proof_current_or_branch(
            whole,
            &environment,
            &realm,
            unproved,
            function,
        );
        self.emit_branch_to_target(proved, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        // The guard's first use, and its first after the epoch advanced.
        self.emit_no_global_lexical_or_branch(facts, &environment, miss, function)?;
        let value = schema.reserve_value_local(function);
        let mut protected = ProtectedPrototypeRows::default();
        let mut sites = Vec::with_capacity(facts.len());
        for fact in facts {
            let name = fact.global_name();
            let site = self.next_site_record::<GlobalReadSiteRecord>()?;
            sites.push(site);
            // The value cell the binding held when this guard last proved
            // the fact. A value cell never changes, so while the binding
            // still holds that cell, in this Environment, it holds the
            // intrinsic; only the storage of the intrinsic's own objects is
            // read again below.
            let cell = self.next_site_record::<StoredValue>()?;
            let proved = self.open_frame(ControlFrameKind::Block, function);
            let unproved = self.open_frame(ControlFrameKind::Block, function);
            let kept = schema
                .reserve_gc_local::<StoredValue, Nullable>(function)
                .initialize(cell.load(schema, function), function);
            kept.load(schema, function).is_null(function);
            self.emit_branch_if_to_target(unproved, function);
            let record = schema
                .reserve_gc_local::<GlobalReadSiteRecord, Nullable>(function)
                .initialize(site.load(schema, function), function);
            record.load(schema, function).is_null(function);
            self.emit_branch_if_to_target(unproved, function);
            let data = self.emit_global_read_record_data_or_branch(
                &record,
                &environment,
                GlobalDeclarativeNames::ProvedOther,
                unproved,
                function,
            );
            record.clear(function);
            let _ = data.load(schema, function);
            let _ = kept.load(schema, function);
            function.instruction(&Instruction::RefEq);
            function.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(unproved, function);
            data.clear(function);
            schema
                .field(StoredValueSchema::REFERENCE)
                .read(&kept, schema, function)
                .store_eq(value.reference(), function);
            kept.clear(function);
            self.emit_branch_to_target(proved, function);
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
            // The guard's first use, and after the binding's value changed.
            function.rarely_reached();
            let found = self.emit_global_read_site_with_i32(
                site,
                &environment,
                name,
                EnvironmentIdentifierRead::Value,
                GlobalDeclarativeNames::ProvedOther,
                &value,
                function,
            )?;
            found.load(function);
            function.instruction(&Instruction::I32Eqz);
            schema.release_i32_local(found, function);
            self.open_frame(ControlFrameKind::If, function);
            // The site's record did not answer. It still proves the binding
            // an own data property of the global object, with no declarative
            // name to shadow it, when the value is one the Realm has not
            // made yet: reading such a property runs no source code and
            // cannot throw, so the complete read only makes the intrinsic.
            function.rarely_reached();
            let record = schema
                .reserve_gc_local::<GlobalReadSiteRecord, Nullable>(function)
                .initialize(site.load(schema, function), function);
            record.load(schema, function).is_null(function);
            self.emit_branch_if_to_target(miss, function);
            let data = self.emit_global_read_record_data_or_branch(
                &record,
                &environment,
                GlobalDeclarativeNames::ProvedOther,
                miss,
                function,
            );
            record.clear(function);
            data.load(schema, function).is_null(function);
            self.emit_branch_if_to_target(miss, function);
            data.clear(function);
            self.emit_global_identifier_read_at(
                site,
                name,
                EnvironmentIdentifierRead::Value,
                &value,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            match fact {
                BuiltinEntryFactIr::Function {
                    function: builtin, ..
                } => {
                    self.emit_realm_builtin_function_or_branch(
                        &value,
                        builtin,
                        &realm,
                        &mut protected,
                        miss,
                        function,
                    )?;
                }
                BuiltinEntryFactIr::Namespace(namespace) => {
                    let layout = namespace_layout(*namespace);
                    let header = schema
                        .reserve_gc_local::<OrdinaryObject, Nullable>(function)
                        .initialize(
                            value
                                .sealed_reference_or_branch::<OrdinaryObject>(
                                    schema,
                                    self.local_branch_label(miss),
                                    function,
                                )
                                .nullable(),
                            function,
                        );
                    self.emit_realm_intrinsic_rows_or_branch(
                        &header,
                        Some(layout),
                        &realm,
                        miss,
                        function,
                    )?;
                    header.clear(function);
                }
                BuiltinEntryFactIr::GlobalThis => {
                    let stored = schema
                        .struct_type::<RealmRecord>()
                        .field(RealmRecordSchema::GLOBAL_THIS)
                        .read(&realm, schema, function)
                        .reference();
                    schema
                        .field(StoredValueSchema::REFERENCE)
                        .read_top(stored, function);
                    value.reference().load(function);
                    function.instruction(&Instruction::RefEq);
                    function.instruction(&Instruction::I32Eqz);
                    self.emit_branch_if_to_target(miss, function);
                }
            }
            // Keep the cell that now holds the proved intrinsic.
            let record = schema
                .reserve_gc_local::<GlobalReadSiteRecord, Nullable>(function)
                .initialize(site.load(schema, function), function);
            record.load(schema, function).is_null(function);
            self.emit_branch_if_to_target(miss, function);
            let data = self.emit_global_read_record_data_or_branch(
                &record,
                &environment,
                GlobalDeclarativeNames::ProvedOther,
                miss,
                function,
            );
            record.clear(function);
            let stored = schema
                .reserve_gc_local::<StoredValue, NonNullable>(function)
                .initialize(
                    data.sealed_subtype_or_branch::<StoredValue>(
                        schema,
                        self.local_branch_label(miss),
                        function,
                    ),
                    function,
                );
            data.clear(function);
            // The cell must hold the value just proved.
            schema
                .field(StoredValueSchema::REFERENCE)
                .read(&stored, schema, function);
            value.reference().load(function);
            function.instruction(&Instruction::RefEq);
            function.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(miss, function);
            cell.store(stored.load(schema, function), function);
            stored.clear(function);
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
            // Live on every entry: the intrinsic's own rows.
            let header = match fact {
                BuiltinEntryFactIr::Function {
                    function: builtin, ..
                } if self.builtin_constructor_keeps_intrinsic_rows(builtin)? => Some(
                    schema
                        .reserve_gc_local::<OrdinaryObject, Nullable>(function)
                        .initialize(
                            {
                                let callable = value.sealed_reference_or_branch::<FunctionObject>(
                                    schema,
                                    self.local_branch_label(miss),
                                    function,
                                );
                                schema
                                    .field(FunctionObjectSchema::OBJECT)
                                    .read_top(callable, function)
                                    .reference()
                                    .nullable()
                            },
                            function,
                        ),
                ),
                BuiltinEntryFactIr::Namespace(_) => Some(
                    schema
                        .reserve_gc_local::<OrdinaryObject, Nullable>(function)
                        .initialize(
                            value
                                .sealed_reference_or_branch::<OrdinaryObject>(
                                    schema,
                                    self.local_branch_label(miss),
                                    function,
                                )
                                .nullable(),
                            function,
                        ),
                ),
                BuiltinEntryFactIr::Function { .. } | BuiltinEntryFactIr::GlobalThis => None,
            };
            if let Some(header) = header {
                let storage = schema
                    .reserve_gc_local::<ObjectPropertyStorage, Nullable>(function)
                    .initialize(
                        schema
                            .field(OrdinaryObjectSchema::PROPERTIES)
                            .read(&header, schema, function)
                            .reference()
                            .nullable(),
                        function,
                    );
                header.clear(function);
                storage.is_subtype::<IntrinsicPropertyStorage>(schema, function);
                function.instruction(&Instruction::I32Eqz);
                self.emit_branch_if_to_target(miss, function);
                storage.clear(function);
            }
        }
        value.clear(function);
        self.emit_protected_prototype_rows_unchanged_or_branch(protected, miss, function);
        self.emit_keep_builtin_entry_proof_or_branch(
            whole, &sites, &global, &realm, miss, function,
        );
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        realm.clear(function);
        environment.clear(function);
        global.clear(function);
        Ok(())
    }

    /// Falls through while the guard's last whole proof still stands: it
    /// was made for this Environment and Realm, the Environment still has
    /// the declarative name table it had then, and the builtin fact epoch
    /// has not advanced. Every state the facts below are read from either
    /// is one of those three or cannot change without advancing the epoch
    /// (`objects/builtin_fact_epoch.rs`), so each fact would be proved again.
    fn emit_builtin_entry_proof_current_or_branch(
        &mut self,
        whole: SiteRecordGlobal<BuiltinEntryProof>,
        environment: &GcLocal<Environment, Nullable>,
        realm: &GcLocal<RealmRecord>,
        unproved: ControlTarget,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        function.rarely_taken(self.local_branch_label(unproved));
        let proof = schema
            .reserve_gc_local::<BuiltinEntryProof, Nullable>(function)
            .initialize(whole.load(schema, function), function);
        proof.load(schema, function).is_null(function);
        self.emit_branch_if_to_target(unproved, function);
        schema
            .field(BuiltinEntryProofSchema::EPOCH)
            .read(&proof, schema, function);
        schema.emit_builtin_fact_epoch(function);
        function.instruction(&Instruction::I64Ne);
        self.emit_branch_if_to_target(unproved, function);
        let _ = schema
            .field(BuiltinEntryProofSchema::ENVIRONMENT)
            .read(&proof, schema, function)
            .reference();
        let _ = environment.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unproved, function);
        let _ = schema
            .field(BuiltinEntryProofSchema::REALM)
            .read(&proof, schema, function)
            .reference();
        let _ = realm.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unproved, function);
        let _ = schema
            .field(BuiltinEntryProofSchema::NAMES)
            .read(&proof, schema, function)
            .reference();
        let _ = schema
            .struct_type::<Environment>()
            .field(EnvironmentSchema::NAMED_BINDINGS)
            .read(environment, schema, function)
            .reference();
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(unproved, function);
        proof.clear(function);
    }

    /// Keeps the whole proof just made, for the current epoch, after marking
    /// the entry each fact was read from as one whose descriptor edits
    /// advance the epoch. Each site's record is the one its fact was just
    /// proved from, for this Environment. Nothing between the facts' reads
    /// and this point runs source code, so the epoch read here is the one
    /// the facts held in.
    fn emit_keep_builtin_entry_proof_or_branch(
        &mut self,
        whole: SiteRecordGlobal<BuiltinEntryProof>,
        sites: &[SiteRecordGlobal<GlobalReadSiteRecord>],
        environment: &GcLocal<Environment>,
        realm: &GcLocal<RealmRecord>,
        miss: ControlTarget,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        for site in sites {
            let record = schema
                .reserve_gc_local::<GlobalReadSiteRecord, Nullable>(function)
                .initialize(site.load(schema, function), function);
            record.load(schema, function).is_null(function);
            self.emit_branch_if_to_target(miss, function);
            let _ = schema
                .field(GlobalReadSiteRecordSchema::ENVIRONMENT)
                .read(&record, schema, function)
                .reference();
            let _ = environment.load(schema, function);
            function.instruction(&Instruction::RefEq);
            function.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(miss, function);
            // A record that names an Environment holds an entry.
            let entry = schema
                .reserve_gc_local::<PropertyEntry, Nullable>(function)
                .initialize(
                    schema
                        .field(GlobalReadSiteRecordSchema::ENTRY)
                        .read(&record, schema, function)
                        .reference(),
                    function,
                );
            record.clear(function);
            schema.field(PropertyEntrySchema::WATCHED).write(
                &entry,
                GcOperand::boolean(true),
                schema,
                function,
            );
            entry.clear(function);
        }
        let names = schema
            .reserve_gc_local::<NamedBindingTable, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::NAMED_BINDINGS)
                    .read(environment, schema, function)
                    .reference(),
                function,
            );
        let epoch = schema.reserve_i64_local(function);
        schema.emit_builtin_fact_epoch(function);
        epoch.store(function);
        whole.store(
            schema.struct_type::<BuiltinEntryProof>().construct(
                (
                    GcOperand::reference(environment, schema),
                    GcOperand::reference(realm, schema),
                    GcOperand::reference(&names, schema),
                    GcOperand::i64_local(epoch),
                ),
                function,
            ),
            function,
        );
        schema.release_i64_local(epoch, function);
        names.clear(function);
    }

    /// Falls through when the Global Environment Record `environment`
    /// declares none of the facts' names, so that each is the global object's
    /// property. The guard keeps the name table it last proved so: a table
    /// is replaced whenever a name joins it, and a name it holds stays
    /// declared, so the same table declares the same names.
    fn emit_no_global_lexical_or_branch(
        &mut self,
        facts: &[BuiltinEntryFactIr],
        environment: &GcLocal<Environment, Nullable>,
        miss: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let proved = self.next_site_record::<NamedBindingTable>()?;
        let names = schema
            .reserve_gc_local::<NamedBindingTable, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::NAMED_BINDINGS)
                    .read(environment, schema, function)
                    .reference(),
                function,
            );
        names.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let _ = names.load(schema, function);
        let _ = proved.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        // Once per table: a Script's global lexical declarations.
        function.rarely_reached();
        let entry = schema
            .reserve_gc_local::<NamedBinding, Nullable>(function)
            .initialize_null(schema, function);
        for fact in facts {
            let key = schema
                .reserve_gc_local::<StringValue, NonNullable>(function)
                .initialize(
                    self.emit_interned_string_reference(fact.global_name(), function)?,
                    function,
                );
            self.emit_find_own_named_binding(environment, &key, &entry, function);
            key.clear(function);
            entry.load(schema, function).is_null(function);
            function.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(miss, function);
        }
        entry.clear(function);
        proved.store(
            names.load(schema, function).require_non_null(function),
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        names.clear(function);
        Ok(())
    }

    /// Whether the builtin function `builtin` is a constructor the Realm
    /// allocates with a complete layout, so that its static properties stay
    /// in intrinsic rows until the first edit.
    fn builtin_constructor_keeps_intrinsic_rows(&self, builtin: &str) -> Result<bool, EmitError> {
        let Some(meta) = self.functions.get(builtin) else {
            return Ok(false);
        };
        if meta.standard_builtin.is_none() {
            return Ok(false);
        }
        match IntrinsicPropertyLayoutId::for_constructor(meta)? {
            Some(layout) => self.strings.has_intrinsic_property_layout(layout),
            None => Ok(false),
        }
    }

    /// Falls through when `value` is the function object the current Realm
    /// created for the builtin `builtin`, with every intrinsic object its
    /// fact reads through unedited; `protected` collects the prototypes the
    /// caller checks once.
    fn emit_realm_builtin_function_or_branch(
        &mut self,
        value: &ValueLocals,
        builtin: &str,
        realm: &GcLocal<RealmRecord>,
        protected: &mut ProtectedPrototypeRows,
        miss: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let functions = self.functions;
        let meta = functions.get(builtin).ok_or_else(|| {
            EmitError::unsupported("compiler invariant: builtin entry fact names no builtin")
        })?;
        let (kind, field, code) = match (meta.standard_builtin, meta.host_builtin) {
            (Some(standard), None) => (
                ExecutableCodeKind::StandardBuiltin,
                schema.field(ExecutableCodeSchema::STANDARD_BUILTIN),
                standard.encode(),
            ),
            (None, Some(host)) => {
                let field = schema.field(ExecutableCodeSchema::HOST_BUILTIN);
                let code = host.encode();
                let kind = ExecutableCodeKind::HostBuiltin;
                let callable = self.emit_native_code_identity_or_branch(
                    value,
                    realm,
                    miss,
                    function,
                    |code_local, schema, function| {
                        schema
                            .field(ExecutableCodeSchema::KIND)
                            .read(code_local, schema, function);
                        function.instruction(&Instruction::I32Const(kind.encode()));
                        function.instruction(&Instruction::I32Ne);
                        field.read(code_local, schema, function);
                        function.instruction(&Instruction::I32Const(code));
                        function.instruction(&Instruction::I32Ne);
                        function.instruction(&Instruction::I32Or);
                    },
                )?;
                callable.clear(function);
                protected.object = true;
                protected.function = true;
                return Ok(());
            }
            _ => {
                return Err(EmitError::unsupported(
                    "compiler invariant: builtin entry fact names no native builtin",
                ));
            }
        };
        let callable = self.emit_native_code_identity_or_branch(
            value,
            realm,
            miss,
            function,
            |code_local, schema, function| {
                schema
                    .field(ExecutableCodeSchema::KIND)
                    .read(code_local, schema, function);
                function.instruction(&Instruction::I32Const(kind.encode()));
                function.instruction(&Instruction::I32Ne);
                field.read(code_local, schema, function);
                function.instruction(&Instruction::I32Const(code));
                function.instruction(&Instruction::I32Ne);
                function.instruction(&Instruction::I32Or);
            },
        )?;
        // A constructor allocated with a complete layout keeps its static
        // properties in intrinsic rows.
        let layout = IntrinsicPropertyLayoutId::for_constructor(meta)?;
        if let Some(layout) = layout {
            if self.strings.has_intrinsic_property_layout(layout)? {
                let header = schema
                    .reserve_gc_local::<OrdinaryObject, Nullable>(function)
                    .initialize(
                        schema
                            .field(FunctionObjectSchema::OBJECT)
                            .read(&callable, schema, function)
                            .reference()
                            .nullable(),
                        function,
                    );
                self.emit_realm_intrinsic_rows_or_branch(&header, None, realm, miss, function)?;
                header.clear(function);
            }
        }
        callable.clear(function);
        protected.object = true;
        protected.function = true;
        match meta.standard_builtin {
            Some(StandardBuiltinId::StringConstructor) => protected.string = true,
            Some(StandardBuiltinId::ArrayConstructor) => protected.array = true,
            _ => {}
        }
        Ok(())
    }

    /// The function object `value` is, after branching to `miss` unless it
    /// is one whose context is `realm` and whose immutable code passes
    /// `differs`, which leaves on the stack whether the code is another's.
    fn emit_native_code_identity_or_branch(
        &mut self,
        value: &ValueLocals,
        realm: &GcLocal<RealmRecord>,
        miss: ControlTarget,
        function: &mut Function,
        differs: impl FnOnce(&GcLocal<crate::gc_types::ExecutableCode>, &RuntimeSchema, &mut Function),
    ) -> Result<GcLocal<FunctionObject>, EmitError> {
        let schema = self.runtime_schema();
        let callable = schema.reserve_non_null_gc_local(function).initialize(
            value.sealed_reference_or_branch::<FunctionObject>(
                schema,
                self.local_branch_label(miss),
                function,
            ),
            function,
        );
        let context = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CONTEXT)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        schema
            .field(FunctionContextSchema::REALM)
            .read(&context, schema, function);
        context.clear(function);
        let _ = realm.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, function);
        let code = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CODE)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        differs(&code, schema, function);
        code.clear(function);
        self.emit_branch_if_to_target(miss, function);
        Ok(callable)
    }

    /// Falls through while `header` still holds intrinsic rows made for
    /// `realm`, of `layout` when the header's identity is not already known.
    /// An intrinsic header never regains rows it lost, and no other header
    /// is allocated with a Realm's rows of that layout.
    fn emit_realm_intrinsic_rows_or_branch(
        &mut self,
        header: &GcLocal<OrdinaryObject, Nullable>,
        layout: Option<IntrinsicPropertyLayoutId>,
        realm: &GcLocal<RealmRecord>,
        miss: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let address = match layout {
            Some(layout) => {
                if !self.strings.has_intrinsic_property_layout(layout)? {
                    // No rows to compare with: the fact is never proved.
                    self.emit_branch_to_target(miss, function);
                    return Ok(());
                }
                Some(
                    self.strings
                        .intrinsic_property_layout_span(layout)?
                        .address(),
                )
            }
            None => None,
        };
        let storage = schema
            .reserve_gc_local::<ObjectPropertyStorage, Nullable>(function)
            .initialize(
                schema
                    .field(OrdinaryObjectSchema::PROPERTIES)
                    .read(header, schema, function)
                    .reference()
                    .nullable(),
                function,
            );
        let rows = schema
            .reserve_gc_local::<IntrinsicPropertyStorage, Nullable>(function)
            .initialize(
                storage
                    .sealed_subtype_or_branch::<IntrinsicPropertyStorage>(
                        schema,
                        self.local_branch_label(miss),
                        function,
                    )
                    .nullable(),
                function,
            );
        storage.clear(function);
        schema
            .field(IntrinsicPropertyStorageSchema::REALM)
            .read(&rows, schema, function);
        let _ = realm.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, function);
        if let Some(address) = address {
            schema
                .field(IntrinsicPropertyStorageSchema::ADDRESS)
                .read(&rows, schema, function);
            function.instruction(&Instruction::I32Const(address as i32));
            function.instruction(&Instruction::I32Ne);
            self.emit_branch_if_to_target(miss, function);
        }
        rows.clear(function);
        Ok(())
    }
}
