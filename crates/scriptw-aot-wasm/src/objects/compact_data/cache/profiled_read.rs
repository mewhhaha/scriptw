//! The inline proof an execution profile selects at a hot static read site.
//!
//! A site the profile marks hot with a dominant kind of answer proves that
//! one kind in P, ahead of exactly the code an unprofiled build emits: the
//! site's own inline hit, then the call of the R hit entry. The proof is the
//! hit entry's own for that kind, emitted by the entry's emitters, so the
//! two cannot drift: the same sealed casts admit the receiver's header and
//! storage, each record of the site's chain is asked in turn as the entry
//! asks it, and every re-proof on the live receiver and its live
//! [[Prototype]] is the one the entry makes. The guard only loads and tests;
//! when no record proves the kind, it falls through having observed nothing
//! and written no part of the read's value, and the branches to that
//! fallback are hinted unlikely. A wrong profile therefore costs speed only.
//!
//! As in the entry, a site's unvisited first record goes to the entry, which
//! sends it to the miss kernel; every answer the guard gives is one the
//! entry would give for the same receiver, from a record of the same chain.
use super::entry::{
    CompactLayout, CompactStorage, HolderStorage, ReadAnswer, ReceiverLayout, PRIMITIVE_RECEIVERS,
};
use super::holder::HolderReceiverKind;
#[cfg(test)]
use super::proof_trace::ProofStep;
use super::*;
use crate::emit::ControlTarget;
use crate::gc_types::JavaScriptObject;
use crate::profile::{CompactStorageLayout, PlaceHolder, ReadSpecialisation, ReceiverHeader};

fn compact_layout(layout: CompactStorageLayout) -> CompactLayout {
    match layout {
        CompactStorageLayout::Array => CompactLayout::Array,
        CompactStorageLayout::Inline => CompactLayout::Inline,
    }
}

/// The holder storages a place proof tests, in the entry's order.
fn holder_storages(holder: PlaceHolder) -> &'static [HolderStorage] {
    match holder {
        PlaceHolder::Intrinsic => &[HolderStorage::Intrinsic],
        PlaceHolder::Compact => &[
            HolderStorage::Compact(CompactLayout::Array),
            HolderStorage::Compact(CompactLayout::Inline),
        ],
        PlaceHolder::Dictionary => &[HolderStorage::Dictionary],
    }
}

pub(super) fn receiver_layout(header: ReceiverHeader) -> ReceiverLayout {
    match header {
        ReceiverHeader::Ordinary => ReceiverLayout::Ordinary,
        ReceiverHeader::Function => ReceiverLayout::Function,
        ReceiverHeader::Array => ReceiverLayout::Array,
        ReceiverHeader::PrimitiveBox => ReceiverLayout::PrimitiveBox,
        ReceiverHeader::Map => ReceiverLayout::Map,
        ReceiverHeader::WeakMap => ReceiverLayout::WeakMap,
        ReceiverHeader::Set => ReceiverLayout::Set,
        ReceiverHeader::WeakSet => ReceiverLayout::WeakSet,
        ReceiverHeader::Promise => ReceiverLayout::Promise,
    }
}

/// An actual OrdinaryObject receiver, its ordinary header and its compact
/// storage of the profiled layout.
struct OrdinaryCompactReceiver {
    object: GcLocal<OrdinaryObject>,
    storage: CompactStorage,
    shape: GcLocal<CompactDataShape>,
}

impl OrdinaryCompactReceiver {
    fn clear(self, f: &mut Function) {
        self.shape.clear(f);
        self.storage.clear(f);
        self.object.clear(f);
    }
}

impl FunctionBuilder<'_> {
    /// Answers the read of the site whose record is `record` into `value`
    /// and branches to `done` when the proof `specialisation` names holds
    /// for `base`; otherwise falls through, having observed nothing and
    /// written no part of `value`.
    pub(super) fn emit_profiled_read(
        &mut self,
        specialisation: ReadSpecialisation,
        base: &ValueLocals,
        record: &GcLocal<OwnDataReadCache>,
        value: &ValueLocals,
        done: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        proof_step!(ProofStep::ProfiledRead);
        // The chain the guard walks is bounded by MAX_SITE_RECORDS.
        self.with_bounded_loops(|builder| {
            builder.emit_profiled_read_guard(specialisation, base, record, value, done, f)
        })?;
        proof_step!(ProofStep::ProfiledReadEnd);
        Ok(())
    }

    fn emit_profiled_read_guard(
        &mut self,
        specialisation: ReadSpecialisation,
        base: &ValueLocals,
        record: &GcLocal<OwnDataReadCache>,
        value: &ValueLocals,
        done: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let fallback = self.open_frame(ControlFrameKind::Block, f);
        f.rarely_taken(self.local_branch_label(fallback));
        // The entry sends a site's unvisited first record to the miss kernel.
        s.field(OwnDataReadCacheSchema::SLOT).read(record, s, f);
        f.instruction(&Instruction::I32Const(UNVISITED_SLOT));
        f.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(fallback, f);
        // The proof's value, written to the read's only once it holds: the
        // read's value may share its locals with `base`.
        let proved = s.reserve_value_local(f);
        let hit = self.open_frame(ControlFrameKind::Block, f);
        let answer = ReadAnswer::Branch(hit);
        match specialisation {
            ReadSpecialisation::OwnCompact(layout) => {
                let receiver =
                    self.emit_ordinary_compact_receiver(base, compact_layout(layout), fallback, f);
                self.emit_record_scan(record, f, |builder, candidate, next, f| {
                    let slot = builder.emit_recorded_shape_slot_or_branch(
                        candidate,
                        &receiver.shape,
                        next,
                        f,
                    );
                    builder.emit_recorded_own_slot_hit(&receiver.storage, slot, &proved, answer, f);
                    builder.runtime_schema().release_i32_local(slot, f);
                    Ok(())
                })?;
                receiver.clear(f);
            }
            ReadSpecialisation::AbsentClosedTail(layout) => {
                let receiver =
                    self.emit_ordinary_compact_receiver(base, compact_layout(layout), fallback, f);
                self.emit_record_scan(record, f, |builder, candidate, next, f| {
                    let slot = builder.emit_recorded_shape_slot_or_branch(
                        candidate,
                        &receiver.shape,
                        next,
                        f,
                    );
                    // A nonnegative slot is no closed tail's word: the
                    // tail's proof fails for it.
                    builder.emit_recorded_closed_tail_hit(
                        &receiver.object,
                        slot,
                        &proved,
                        next,
                        answer,
                        f,
                    )?;
                    builder.runtime_schema().release_i32_local(slot, f);
                    Ok(())
                })?;
                receiver.clear(f);
            }
            ReadSpecialisation::ProtoPlaceCompact(layout, holder) => {
                let receiver =
                    self.emit_ordinary_compact_receiver(base, compact_layout(layout), fallback, f);
                self.emit_profiled_place_scan(
                    record,
                    &receiver.object,
                    Some(&receiver.shape),
                    holder_storages(holder),
                    &proved,
                    answer,
                    f,
                )?;
                receiver.clear(f);
            }
            ReadSpecialisation::ProtoPlaceIntrinsic(header) => {
                let object = s.reserve_non_null_gc_local(f).initialize(
                    self.emit_layout_header_or_branch(receiver_layout(header), base, fallback, f),
                    f,
                );
                let storage = s.reserve_non_null_gc_local(f).initialize(
                    s.field(OrdinaryObjectSchema::PROPERTIES)
                        .read(&object, s, f)
                        .reference(),
                    f,
                );
                let dictionary = s.reserve_non_null_gc_local(f).initialize(
                    storage.sealed_subtype_or_branch::<OrdinaryPropertyStorage>(
                        s,
                        self.local_branch_label(fallback),
                        f,
                    ),
                    f,
                );
                storage.clear(f);
                // The entry asks this of every record; it holds of the
                // receiver alone.
                self.emit_empty_dictionary_or_branch(&dictionary, fallback, f);
                dictionary.clear(f);
                self.emit_profiled_place_scan(
                    record,
                    &object,
                    None,
                    holder_storages(PlaceHolder::Intrinsic),
                    &proved,
                    answer,
                    f,
                )?;
                object.clear(f);
            }
            ReadSpecialisation::StringPrototypePlace => {
                let (tag, kind, slot) = PRIMITIVE_RECEIVERS
                    .into_iter()
                    .find(|(tag, _, _)| *tag == WasmRuntimeValueTag::String)
                    .expect("a String's records name its Realm's prototype");
                base.tag().load(f);
                f.instruction(&Instruction::I32Const(tag as i32));
                f.instruction(&Instruction::I32Ne);
                self.emit_branch_if_to_target(fallback, f);
                // As in the entry: ToObject would box the String with the
                // current Realm's prototype, which holds the place.
                let holder =
                    self.emit_primitive_receiver_prototype_header_or_branch(slot, fallback, f);
                let (name, key) = self.emit_record_key(record, f);
                let found = s.reserve_i32_local(f);
                self.emit_record_scan(record, f, |builder, candidate, next, f| {
                    builder.emit_receiver_rule_or_branch(candidate, kind, None, next, f);
                    builder.emit_recorded_place_hit_on(
                        holder_storages(PlaceHolder::Intrinsic),
                        candidate,
                        &holder,
                        &key,
                        &proved,
                        found,
                        next,
                        answer,
                        f,
                    )
                })?;
                s.release_i32_local(found, f);
                key.clear(f);
                name.clear(f);
                holder.clear(f);
            }
        }
        // No record proves the kind for this receiver.
        self.emit_branch_to_target(fallback, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.copy_from(&proved, f);
        proved.reference().clear(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        proved.clear(f);
        Ok(())
    }

    /// The header and compact storage of `base` when it is an actual
    /// OrdinaryObject on compact storage of `layout`, the entry's casts for
    /// its object section; anything else branches to `other`.
    fn emit_ordinary_compact_receiver(
        &mut self,
        base: &ValueLocals,
        layout: CompactLayout,
        other: ControlTarget,
        f: &mut Function,
    ) -> OrdinaryCompactReceiver {
        let s = self.runtime_schema();
        let object = s.reserve_non_null_gc_local(f).initialize(
            self.emit_layout_header_or_branch(ReceiverLayout::Ordinary, base, other, f),
            f,
        );
        let properties = s.reserve_non_null_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, s, f)
                .reference(),
            f,
        );
        let storage = self.emit_compact_storage_or_branch(layout, &properties, other, f);
        properties.clear(f);
        let shape = s
            .reserve_non_null_gc_local(f)
            .initialize(storage.shape(s, f), f);
        OrdinaryCompactReceiver {
            object,
            storage,
            shape,
        }
    }

    /// Each record's place on the live [[Prototype]] of the receiver whose
    /// header is `object`, on a holder of one of `storages`, under the
    /// record's absence rule for an object: by the live compact `shape`, or
    /// with none for an empty dictionary.
    #[allow(clippy::too_many_arguments)]
    fn emit_profiled_place_scan(
        &mut self,
        record: &GcLocal<OwnDataReadCache>,
        object: &GcLocal<OrdinaryObject>,
        shape: Option<&GcLocal<CompactDataShape>>,
        storages: &[HolderStorage],
        proved: &ValueLocals,
        answer: ReadAnswer,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let (name, key) = self.emit_record_key(record, f);
        let prototype = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize_null(s, f);
        let found = s.reserve_i32_local(f);
        self.emit_record_scan(record, f, |builder, candidate, next, f| {
            builder.emit_receiver_rule_or_branch(
                candidate,
                HolderReceiverKind::Object,
                shape,
                next,
                f,
            );
            builder.emit_prototype_place_hit_on(
                storages, candidate, object, &prototype, &key, proved, found, next, answer, f,
            )
        })?;
        s.release_i32_local(found, f);
        prototype.clear(f);
        key.clear(f);
        name.clear(f);
        Ok(())
    }

    /// The site's key, as the entry takes it from the site's record.
    fn emit_record_key(
        &self,
        record: &GcLocal<OwnDataReadCache>,
        f: &mut Function,
    ) -> (GcLocal<StringValue>, PropertyKeyLocals) {
        let s = self.runtime_schema();
        let name = s.reserve_non_null_gc_local(f).initialize(
            s.field(OwnDataReadCacheSchema::KEY)
                .read(record, s, f)
                .reference(),
            f,
        );
        let key = PropertyKeyLocals::from_string(s, &name, f);
        (name, key)
    }
}

#[cfg(test)]
mod tests {
    use super::super::proof_trace::{capture, ProofStep};
    use super::{CompactLayout, ReceiverLayout};
    use crate::profile::{
        AdmittedProfile, CompactStorageLayout, ExecutionProfile, PlaceHolder, ProfileMode,
        ProgramKey, ReadSiteObservation, ReadSpecialisation, ReceiverHeader,
    };
    use crate::{PromiseRejectionPolicy, RuntimeArtifactInputs, WasmArtifact};

    /// One static read site, in `probe`.
    const PROBE: &str = "function probe(o) { return o.x; } probe({ x: 1 });";

    fn emit(mode: ProfileMode<'_>) -> WasmArtifact {
        let parsed = scriptw_front::parse(PROBE, scriptw_front::ParseOptions::script()).unwrap();
        crate::emit_with_intl_profile_and_runtime_inputs(
            &scriptw_ir::lower(&parsed),
            PromiseRejectionPolicy::default(),
            &scriptw_intl::IntlCompilationProfile::default(),
            RuntimeArtifactInputs::new(None),
            mode,
        )
        .unwrap()
    }

    fn marking_every_read(artifact: &WasmArtifact, kind: ReadSpecialisation) -> AdmittedProfile {
        let program = ProgramKey::new([0; 32]);
        let mut profile = ExecutionProfile::new(
            program,
            artifact.site_map(),
            "test",
            std::num::NonZeroU32::MIN,
        );
        for site in artifact.site_map().read_sites() {
            profile
                .observe_read(site, ReadSiteObservation::dominated(u64::MAX, kind))
                .unwrap();
        }
        AdmittedProfile::admit(&profile, &program).unwrap()
    }

    /// The steps between the first `begin` and the next `end`.
    fn between(steps: &[ProofStep], begin: ProofStep, end: ProofStep) -> Vec<ProofStep> {
        let start = steps.iter().position(|step| *step == begin).unwrap() + 1;
        let len = steps[start..].iter().position(|step| *step == end).unwrap();
        steps[start..start + len].to_vec()
    }

    /// The steps of each profiled read guard, in emission order.
    fn guards(steps: &[ProofStep]) -> Vec<Vec<ProofStep>> {
        let mut guards = Vec::new();
        let mut rest = steps;
        while let Some(start) = rest
            .iter()
            .position(|step| *step == ProofStep::ProfiledRead)
        {
            let guard = between(
                &rest[start..],
                ProofStep::ProfiledRead,
                ProofStep::ProfiledReadEnd,
            );
            rest = &rest[start + guard.len() + 2..];
            guards.push(guard);
        }
        guards
    }

    fn expected(kind: ReadSpecialisation) -> Vec<ProofStep> {
        let layout = |layout| match layout {
            CompactStorageLayout::Array => CompactLayout::Array,
            CompactStorageLayout::Inline => CompactLayout::Inline,
        };
        // A place on a holder: its header, then the profiled storage of it.
        let place = |holder| {
            let mut steps = vec![ProofStep::RecordedPlace];
            for storage in super::holder_storages(holder) {
                steps.push(ProofStep::HolderStorage(*storage));
                if let super::HolderStorage::Compact(layout) = storage {
                    steps.push(ProofStep::CompactStorage(*layout));
                }
            }
            steps
        };
        let object_rule = |shape| ProofStep::ReceiverRule {
            kind: super::HolderReceiverKind::Object.word(),
            shape,
        };
        let receiver = |storage| {
            [
                ProofStep::LayoutHeader(ReceiverLayout::Ordinary),
                ProofStep::CompactStorage(layout(storage)),
                ProofStep::RecordScan,
            ]
        };
        match kind {
            ReadSpecialisation::OwnCompact(storage) => [
                &receiver(storage)[..],
                &[ProofStep::RecordedShape, ProofStep::RecordedOwnSlot],
            ]
            .concat(),
            ReadSpecialisation::AbsentClosedTail(storage) => [
                &receiver(storage)[..],
                &[ProofStep::RecordedShape, ProofStep::RecordedClosedTail],
            ]
            .concat(),
            ReadSpecialisation::ProtoPlaceCompact(storage, holder) => [
                &receiver(storage)[..],
                &[object_rule(true), ProofStep::PrototypePlace],
                &place(holder),
            ]
            .concat(),
            ReadSpecialisation::ProtoPlaceIntrinsic(header) => [
                &[
                    ProofStep::LayoutHeader(super::receiver_layout(header)),
                    ProofStep::EmptyDictionary,
                    ProofStep::RecordScan,
                    object_rule(false),
                    ProofStep::PrototypePlace,
                ][..],
                &place(PlaceHolder::Intrinsic),
            ]
            .concat(),
            ReadSpecialisation::StringPrototypePlace => [
                &[
                    ProofStep::RealmPrototypeHeader,
                    ProofStep::RecordScan,
                    ProofStep::ReceiverRule {
                        kind: super::HolderReceiverKind::String.word(),
                        shape: false,
                    },
                ][..],
                &place(PlaceHolder::Intrinsic),
            ]
            .concat(),
        }
    }

    const ARRAY_OWN_SLOT: ReadSpecialisation =
        ReadSpecialisation::OwnCompact(crate::profile::CompactStorageLayout::Array);

    /// Each kind's guard is made of the R read entry's own proof emitters,
    /// called as the entry calls them, and of nothing else.
    #[test]
    fn each_guard_runs_the_read_entry_proofs() {
        let parsed = scriptw_front::parse(";", scriptw_front::ParseOptions::script()).unwrap();
        let script = scriptw_ir::lower(&parsed).script.unwrap();
        let selection =
            scriptw_intl::IntlDataSelection::new(scriptw_intl::IntlCompilationProfile::Minimal);
        let (runtime, steps) = capture(|| {
            crate::emit::emit_runtime_module(
                &script,
                &selection,
                crate::RuntimeSourceCompilationProfile::Portable,
            )
        });
        runtime.unwrap();
        let entry = between(&steps, ProofStep::ReadEntry, ProofStep::ReadEntryEnd);
        assert!(!entry.contains(&ProofStep::ProfiledRead));

        let unprofiled = emit(ProfileMode::None);
        // `probe` and its exact-helper-context clone each have the site.
        let reads = unprofiled.site_map().read_sites().count();
        assert!(reads >= 1);
        for kind in ReadSpecialisation::ALL {
            let profile = marking_every_read(&unprofiled, kind);
            let (profiled, steps) = capture(|| emit(ProfileMode::Apply(&profile)));
            assert_eq!(profiled.site_map(), unprofiled.site_map());
            let guards = guards(&steps);
            // `probe`'s base may be ordinary, so its own inline hit already
            // proves an own compact slot of either layout and those kinds take no guard.
            let guarded = if matches!(kind, ReadSpecialisation::OwnCompact(_)) {
                0
            } else {
                reads
            };
            assert_eq!(guards.len(), guarded, "{kind:?}");
            for guard in guards {
                assert_eq!(guard, expected(kind), "{kind:?}");
                for step in &guard {
                    assert!(
                        entry.contains(step),
                        "{kind:?}: {step:?} is not a proof of the R read entry"
                    );
                }
            }
        }
    }

    /// Every specialisation compiles to a valid P, and only the profiled
    /// site's body changes.
    #[test]
    fn profiled_bodies_validate_and_only_the_site_body_grows() {
        let unprofiled = emit(ProfileMode::None);
        let size = |artifact: &WasmArtifact, prefix: &str| {
            artifact
                .function_sizes
                .iter()
                .filter(|body| body.name.starts_with(prefix))
                .map(|body| body.body_bytes.bytes())
                .sum::<u32>()
        };
        for kind in
            ReadSpecialisation::ALL
                .into_iter()
                .chain([ReadSpecialisation::ProtoPlaceIntrinsic(
                    ReceiverHeader::WeakMap,
                )])
        {
            let profile = marking_every_read(&unprofiled, kind);
            let profiled = emit(ProfileMode::Apply(&profile));
            wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
                .validate_all(&profiled.bytes)
                .unwrap_or_else(|error| panic!("{kind:?}: {error}"));
            if matches!(kind, ReadSpecialisation::OwnCompact(_)) {
                assert_eq!(profiled.bytes, unprofiled.bytes, "{kind:?}");
            } else {
                assert!(size(&profiled, "js::probe#") > size(&unprofiled, "js::probe#"));
            }
            assert_eq!(
                size(&profiled, "scriptw::main"),
                size(&unprofiled, "scriptw::main")
            );
        }
    }
}
