//! What an instrumented P reports of its static read sites' records, and
//! how a profile reads that report.
//!
//! The exported profile dump of a Collect-mode P walks every read site's
//! record chain and writes, per site, one slot of the profile memory: the
//! chain's length, whether the site spent its learning budget, the first
//! record's receiver-layout hint, and a facts word per record that says
//! which of the record's proofs are filled. The dump only reads records; it
//! runs after the training run and changes nothing the program observes.
//!
//! [`observe_dump_slot`] turns a slot back into what a profile keeps: the
//! chain length, the megamorphic flag and the one [`ReadSpecialisation`]
//! that every record of the chain proves, if there is one. The record facts
//! are named here, next to the fields and words they test, so the emitted
//! dump and the classification cannot disagree about a bit. The record
//! fields do not say which compact layout a receiver had; the hint, the
//! layout pair the miss kernel last met at the site, stands for it. A chain
//! whose receivers had several layouts may therefore be named by the last
//! one, which costs a specialised build speed only.
use super::entry::{ReceiverLayout, ReceiverStorage, RECEIVER_LAYOUTS, RECEIVER_STORAGES};
use super::holder::{HolderReceiverKind, NO_INTRINSIC_LAYOUT};
use super::profiled_read::receiver_layout;
use super::*;
use crate::gc_types::SiteRecordGlobal;
use crate::profile::{CompactStorageLayout, PlaceHolder, ReadSpecialisation, ReceiverHeader};

/// The bytes of one read site's slot in the profile memory:
/// `[records u8][flags u8][hint u8][0]`, then a little-endian u16 facts
/// word per record of the chain, in chain order, then zero padding.
pub(crate) const DUMP_SLOT_BYTES: u32 = 24;
const FACTS_AT: u32 = 4;
/// Slot flag: the site's learning budget is spent.
const FLAG_MEGAMORPHIC: u8 = 1;

const _: () = assert!(FACTS_AT + 2 * MAX_SITE_RECORDS as u32 <= DUMP_SLOT_BYTES);
const _: () = assert!(DUMP_SLOT_BYTES % 8 == 0);
// A hint word fits the slot's byte.
const _: () =
    assert!((RECEIVER_STORAGES.len() as u32) << ReceiverStorage::HINT_SHIFT <= u8::MAX as u32 + 1);

/// One bit of a record's facts word, each a test of one record field.
#[derive(Clone, Copy)]
enum Fact {
    /// SLOT is the unvisited word.
    Unvisited,
    /// SLOT is negative and not the unvisited word: a closed tail's word.
    NegativeSlot,
    /// SHAPE is set: an own compact slot or an absence by shape.
    OwnShape,
    /// DICTIONARY_POSITION names an own dictionary entry.
    OwnDictionary,
    /// LAYOUT names an own intrinsic row.
    OwnIntrinsic,
    /// RECEIVER_SHAPE is set: the receiver rule's compact shape.
    ReceiverShape,
    /// HOLDER_LAYOUT names an intrinsic holder layout.
    HolderIntrinsic,
    /// HOLDER_SHAPE is set: a compact holder.
    HolderShape,
    /// The record exists; every written facts word has it.
    Present,
}

impl Fact {
    const fn bit(self) -> u16 {
        1 << match self {
            Self::Unvisited => 0,
            Self::NegativeSlot => 1,
            Self::OwnShape => 2,
            Self::OwnDictionary => 3,
            Self::OwnIntrinsic => 4,
            Self::ReceiverShape => 8,
            Self::HolderIntrinsic => 9,
            Self::HolderShape => 10,
            Self::Present => 15,
        }
    }
}

/// RECEIVER_KIND's word sits in bits 5 to 7.
const RECEIVER_KIND_SHIFT: u32 = 5;
const RECEIVER_KIND_MASK: i32 = 0b111;

const fn receiver_kind_fits(kind: HolderReceiverKind) -> bool {
    kind.word() >= 0 && kind.word() <= RECEIVER_KIND_MASK
}
const _: () = assert!(
    receiver_kind_fits(HolderReceiverKind::None)
        && receiver_kind_fits(HolderReceiverKind::Object)
        && receiver_kind_fits(HolderReceiverKind::String)
        && receiver_kind_fits(HolderReceiverKind::Number)
        && receiver_kind_fits(HolderReceiverKind::Probed)
);

/// What one record of a site's chain proves, in the profile's vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecordClass {
    /// The miss kernel never visited it.
    Unvisited,
    /// It proves nothing yet.
    Blank,
    /// Its proof is the one this specialisation makes.
    Proves(ReadSpecialisation),
    /// Its proof is one no specialisation makes: an own dictionary entry or
    /// intrinsic row, a Number or probed-dictionary receiver, or a receiver
    /// layout no specialisation names.
    Unspecialised,
}

/// A read site as its dump slot reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DumpedReadSite {
    pub(crate) records: u8,
    pub(crate) megamorphic: bool,
    pub(crate) dominant: Option<ReadSpecialisation>,
}

/// The receiver layout pair of a hint word, when it names one.
fn hinted(hint: u8) -> (Option<ReceiverLayout>, Option<ReceiverStorage>) {
    let header = i32::from(hint) & ((1 << ReceiverStorage::HINT_SHIFT) - 1);
    let storage = i32::from(hint) >> ReceiverStorage::HINT_SHIFT;
    (
        RECEIVER_LAYOUTS
            .into_iter()
            .find(|layout| layout.word() == header),
        RECEIVER_STORAGES
            .into_iter()
            .find(|candidate| candidate.word() == storage),
    )
}

/// The inverse of `receiver_layout`: a layout without a profile header
/// fails to compile here.
fn header_of(layout: ReceiverLayout) -> ReceiverHeader {
    match layout {
        ReceiverLayout::Ordinary => ReceiverHeader::Ordinary,
        ReceiverLayout::Function => ReceiverHeader::Function,
        ReceiverLayout::Array => ReceiverHeader::Array,
        ReceiverLayout::PrimitiveBox => ReceiverHeader::PrimitiveBox,
        ReceiverLayout::Map => ReceiverHeader::Map,
        ReceiverLayout::WeakMap => ReceiverHeader::WeakMap,
        ReceiverLayout::Set => ReceiverHeader::Set,
        ReceiverLayout::WeakSet => ReceiverHeader::WeakSet,
        ReceiverLayout::Promise => ReceiverHeader::Promise,
    }
}

/// What a record whose facts word is `facts` proves, at a site whose hint
/// word is `hint`. The precedence is the entry's: own proofs before the
/// prototype place.
pub(crate) fn classify_record(facts: u16, hint: u8) -> RecordClass {
    let has = |fact: Fact| facts & fact.bit() != 0;
    let (header, storage) = hinted(hint);
    let ordinary_compact = match (header, storage) {
        (Some(ReceiverLayout::Ordinary), Some(ReceiverStorage::Array)) => {
            Some(CompactStorageLayout::Array)
        }
        (Some(ReceiverLayout::Ordinary), Some(ReceiverStorage::Inline)) => {
            Some(CompactStorageLayout::Inline)
        }
        _ => None,
    };
    let proves = |specialisation: Option<ReadSpecialisation>| {
        specialisation.map_or(RecordClass::Unspecialised, RecordClass::Proves)
    };
    if has(Fact::Unvisited) {
        return RecordClass::Unvisited;
    }
    if has(Fact::OwnShape) {
        return proves(ordinary_compact.map(|layout| {
            if has(Fact::NegativeSlot) {
                ReadSpecialisation::AbsentClosedTail(layout)
            } else {
                ReadSpecialisation::OwnCompact(layout)
            }
        }));
    }
    if has(Fact::OwnDictionary) || has(Fact::OwnIntrinsic) {
        return RecordClass::Unspecialised;
    }
    let holder = if has(Fact::HolderIntrinsic) {
        PlaceHolder::Intrinsic
    } else if has(Fact::HolderShape) {
        PlaceHolder::Compact
    } else {
        PlaceHolder::Dictionary
    };
    let kind = i32::from(facts >> RECEIVER_KIND_SHIFT) & RECEIVER_KIND_MASK;
    let is = |candidate: HolderReceiverKind| kind == candidate.word();
    if is(HolderReceiverKind::None) {
        RecordClass::Blank
    } else if is(HolderReceiverKind::Object) {
        if has(Fact::ReceiverShape) {
            proves(
                ordinary_compact
                    .map(|layout| ReadSpecialisation::ProtoPlaceCompact(layout, holder)),
            )
        } else {
            // An empty dictionary receiver: only an intrinsic holder is a
            // specialisation's place.
            proves(match (header, storage, holder) {
                (Some(layout), Some(ReceiverStorage::Dictionary), PlaceHolder::Intrinsic) => {
                    Some(ReadSpecialisation::ProtoPlaceIntrinsic(header_of(layout)))
                }
                _ => None,
            })
        }
    } else if is(HolderReceiverKind::String) {
        proves(
            (holder == PlaceHolder::Intrinsic).then_some(ReadSpecialisation::StringPrototypePlace),
        )
    } else {
        RecordClass::Unspecialised
    }
}

/// Reads one read site's dump slot. A site has a dominant kind when it
/// still learns and every record of its chain that proves anything proves
/// that one kind.
pub(crate) fn observe_dump_slot(slot: &[u8]) -> Result<DumpedReadSite, &'static str> {
    let slot: &[u8; DUMP_SLOT_BYTES as usize] = slot
        .try_into()
        .map_err(|_| "a read site's dump slot has the wrong size")?;
    let [records, flags, hint, ..] = *slot;
    if usize::from(records) > MAX_SITE_RECORDS as usize {
        return Err("a read site's dump names more records than a chain holds");
    }
    if flags & !FLAG_MEGAMORPHIC != 0 {
        return Err("a read site's dump has unknown flags");
    }
    let megamorphic = flags & FLAG_MEGAMORPHIC != 0;
    let mut dominant = None;
    let mut mixed = false;
    for record in 0..usize::from(records) {
        let at = FACTS_AT as usize + 2 * record;
        let facts = u16::from_le_bytes([slot[at], slot[at + 1]]);
        if facts & Fact::Present.bit() == 0 {
            return Err("a read site's dump omits a record of its chain");
        }
        match classify_record(facts, hint) {
            RecordClass::Blank => {}
            RecordClass::Proves(kind) if dominant.is_none_or(|known| known == kind) => {
                dominant = Some(kind);
            }
            RecordClass::Proves(_) | RecordClass::Unvisited | RecordClass::Unspecialised => {
                mixed = true;
            }
        }
    }
    Ok(DumpedReadSite {
        records,
        megamorphic,
        dominant: dominant.filter(|_| !mixed && !megamorphic),
    })
}

/// The body of the exported profile dump: for the read site whose record
/// global is `reads[i]`, fills the slot at `dump_base + i * DUMP_SLOT_BYTES`
/// of linear memory `memory`. It reads records and writes only that memory.
pub(crate) fn emit_profile_dump(
    schema: &RuntimeSchema,
    reads: &[SiteRecordGlobal],
    memory: u32,
    dump_base: u32,
) -> Result<Function, EmitError> {
    let s = schema;
    let count = i32::try_from(reads.len())
        .map_err(|_| EmitError::unsupported("profiled read sites exceed i32"))?;
    let mut function = Function::new(crate::code_sink::LocalDeclarations::from_types(
        std::iter::empty(),
    ));
    let f = &mut function;
    if reads.is_empty() {
        f.instruction(&Instruction::End);
        return Ok(function);
    }
    let at = |offset: u32, align: u32| MemArg {
        offset: u64::from(dump_base) + u64::from(offset),
        align,
        memory_index: memory,
    };
    let index = s.reserve_i32_local(f);
    let address = s.reserve_i32_local(f);
    let chained = s.reserve_i32_local(f);
    let record = s
        .reserve_gc_local::<OwnDataReadCache, Nullable>(f)
        .initialize_null(s, f);
    // The records, gathered from their globals into one table.
    index.set_constant(count, f);
    let table = s.reserve_gc_local(f).initialize(
        s.array_type::<OwnDataReadCacheTable>()
            .filled(GcOperand::null(s), index, f),
        f,
    );
    for (position, global) in reads.iter().enumerate() {
        index.set_constant(position as i32, f);
        record.replace(global.load(s, f), f);
        s.array_type::<OwnDataReadCacheTable>().write(
            &table,
            index,
            GcOperand::reference(&record, s),
            s,
            f,
        );
    }
    index.set_constant(0, f);
    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));
    index.load(f);
    f.instruction(&Instruction::I32Const(count));
    f.instruction(&Instruction::I32GeU);
    f.instruction(&Instruction::BrIf(1));
    index.load(f);
    f.instruction(&Instruction::I32Const(DUMP_SLOT_BYTES as i32));
    f.instruction(&Instruction::I32Mul);
    address.store(f);
    // A dump may run more than once: every byte of the slot is written.
    for offset in (0..DUMP_SLOT_BYTES).step_by(8) {
        address.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64Store(at(offset, 3)));
    }
    record.replace(
        s.array_type::<OwnDataReadCacheTable>()
            .read(&table, index, s, f)
            .reference(),
        f,
    );
    // A site that never ran has no record.
    f.instruction(&Instruction::Block(BlockType::Empty));
    record.load(s, f).is_null(f);
    f.instruction(&Instruction::BrIf(0));
    address.load(f);
    s.field(OwnDataReadCacheSchema::HINT).read(&record, s, f);
    f.instruction(&Instruction::I32Store8(at(2, 0)));
    address.load(f);
    s.field(OwnDataReadCacheSchema::BUDGET).read(&record, s, f);
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::I32LeS);
    f.instruction(&Instruction::I32Const(i32::from(FLAG_MEGAMORPHIC)));
    f.instruction(&Instruction::I32Mul);
    f.instruction(&Instruction::I32Store8(at(1, 0)));
    chained.set_constant(0, f);
    f.instruction(&Instruction::Loop(BlockType::Empty));
    address.load(f);
    chained.load(f);
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Shl);
    f.instruction(&Instruction::I32Add);
    emit_record_facts(s, &record, f);
    f.instruction(&Instruction::I32Store16(at(FACTS_AT, 1)));
    chained.load(f);
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    chained.store(f);
    record.replace(
        s.field(OwnDataReadCacheSchema::NEXT)
            .read(&record, s, f)
            .reference(),
        f,
    );
    // Chains are bounded; the bound is checked all the same.
    record.load(s, f).is_null(f);
    f.instruction(&Instruction::I32Eqz);
    chained.load(f);
    f.instruction(&Instruction::I32Const(MAX_SITE_RECORDS));
    f.instruction(&Instruction::I32LtU);
    f.instruction(&Instruction::I32And);
    f.instruction(&Instruction::BrIf(0));
    f.instruction(&Instruction::End);
    address.load(f);
    chained.load(f);
    f.instruction(&Instruction::I32Store8(at(0, 0)));
    f.instruction(&Instruction::End);
    index.load(f);
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    index.store(f);
    f.instruction(&Instruction::Br(0));
    f.instruction(&Instruction::End);
    f.instruction(&Instruction::End);
    table.clear(f);
    record.clear(f);
    s.release_i32_local(chained, f);
    s.release_i32_local(address, f);
    s.release_i32_local(index, f);
    f.instruction(&Instruction::End);
    Ok(function)
}

/// Leaves the facts word of the non-null `record` on the stack.
fn emit_record_facts(
    s: &RuntimeSchema,
    record: &GcLocal<OwnDataReadCache, Nullable>,
    f: &mut Function,
) {
    let or_bit = |fact: Fact, f: &mut Function| {
        f.instruction(&Instruction::I32Const(fact.bit().trailing_zeros() as i32));
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::I32Or);
    };
    f.instruction(&Instruction::I32Const(i32::from(Fact::Present.bit())));
    s.field(OwnDataReadCacheSchema::SLOT).read(record, s, f);
    f.instruction(&Instruction::I32Const(UNVISITED_SLOT));
    f.instruction(&Instruction::I32Eq);
    or_bit(Fact::Unvisited, f);
    s.field(OwnDataReadCacheSchema::SLOT).read(record, s, f);
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::I32LtS);
    s.field(OwnDataReadCacheSchema::SLOT).read(record, s, f);
    f.instruction(&Instruction::I32Const(UNVISITED_SLOT));
    f.instruction(&Instruction::I32Ne);
    f.instruction(&Instruction::I32And);
    or_bit(Fact::NegativeSlot, f);
    s.field(OwnDataReadCacheSchema::SHAPE)
        .read(record, s, f)
        .reference()
        .is_null(f);
    f.instruction(&Instruction::I32Eqz);
    or_bit(Fact::OwnShape, f);
    s.field(OwnDataReadCacheSchema::DICTIONARY_POSITION)
        .read(record, s, f);
    f.instruction(&Instruction::I32Const(-1));
    f.instruction(&Instruction::I32Ne);
    or_bit(Fact::OwnDictionary, f);
    s.field(OwnDataReadCacheSchema::LAYOUT).read(record, s, f);
    f.instruction(&Instruction::I32Const(NO_INTRINSIC_LAYOUT));
    f.instruction(&Instruction::I32Ne);
    or_bit(Fact::OwnIntrinsic, f);
    s.field(OwnDataReadCacheSchema::RECEIVER_KIND)
        .read(record, s, f);
    f.instruction(&Instruction::I32Const(RECEIVER_KIND_MASK));
    f.instruction(&Instruction::I32And);
    f.instruction(&Instruction::I32Const(RECEIVER_KIND_SHIFT as i32));
    f.instruction(&Instruction::I32Shl);
    f.instruction(&Instruction::I32Or);
    s.field(OwnDataReadCacheSchema::RECEIVER_SHAPE)
        .read(record, s, f)
        .reference()
        .is_null(f);
    f.instruction(&Instruction::I32Eqz);
    or_bit(Fact::ReceiverShape, f);
    s.field(OwnDataReadCacheSchema::HOLDER_LAYOUT)
        .read(record, s, f);
    f.instruction(&Instruction::I32Const(NO_INTRINSIC_LAYOUT));
    f.instruction(&Instruction::I32Ne);
    or_bit(Fact::HolderIntrinsic, f);
    s.field(OwnDataReadCacheSchema::HOLDER_SHAPE)
        .read(record, s, f)
        .reference()
        .is_null(f);
    f.instruction(&Instruction::I32Eqz);
    or_bit(Fact::HolderShape, f);
}

/// A dump slot whose chain has one record proving each of `records`, as a
/// training run of a site whose receivers had those kinds leaves it.
#[cfg(test)]
pub(crate) fn dump_slot_for_test(records: &[ReadSpecialisation], megamorphic: bool) -> Vec<u8> {
    let mut slot = vec![0; DUMP_SLOT_BYTES as usize];
    slot[0] = records.len() as u8;
    slot[1] = if megamorphic { FLAG_MEGAMORPHIC } else { 0 };
    for (position, kind) in records.iter().enumerate() {
        let (facts, hint) = tests::witness(*kind);
        slot[2] = hint;
        let at = FACTS_AT as usize + 2 * position;
        slot[at..at + 2].copy_from_slice(&facts.to_le_bytes());
    }
    slot
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(bits: &[Fact], kind: HolderReceiverKind) -> u16 {
        bits.iter()
            .fold(Fact::Present.bit(), |word, fact| word | fact.bit())
            | ((kind.word() as u16) << RECEIVER_KIND_SHIFT)
    }

    fn hint(layout: ReceiverLayout, storage: ReceiverStorage) -> u8 {
        (layout.word() | storage.word() << ReceiverStorage::HINT_SHIFT) as u8
    }

    fn storage_of(layout: CompactStorageLayout) -> ReceiverStorage {
        match layout {
            CompactStorageLayout::Array => ReceiverStorage::Array,
            CompactStorageLayout::Inline => ReceiverStorage::Inline,
        }
    }

    /// A record and hint that prove `specialisation`, as the miss kernel
    /// fills them.
    pub(super) fn witness(specialisation: ReadSpecialisation) -> (u16, u8) {
        let holder_fact = |holder: PlaceHolder| match holder {
            PlaceHolder::Intrinsic => vec![Fact::HolderIntrinsic],
            PlaceHolder::Compact => vec![Fact::HolderShape],
            PlaceHolder::Dictionary => vec![],
        };
        match specialisation {
            ReadSpecialisation::OwnCompact(layout) => (
                facts(&[Fact::OwnShape], HolderReceiverKind::None),
                hint(ReceiverLayout::Ordinary, storage_of(layout)),
            ),
            ReadSpecialisation::AbsentClosedTail(layout) => (
                facts(
                    &[Fact::OwnShape, Fact::NegativeSlot],
                    HolderReceiverKind::None,
                ),
                hint(ReceiverLayout::Ordinary, storage_of(layout)),
            ),
            ReadSpecialisation::ProtoPlaceCompact(layout, holder) => {
                let mut bits = holder_fact(holder);
                bits.push(Fact::ReceiverShape);
                (
                    facts(&bits, HolderReceiverKind::Object),
                    hint(ReceiverLayout::Ordinary, storage_of(layout)),
                )
            }
            ReadSpecialisation::ProtoPlaceIntrinsic(header) => (
                facts(&[Fact::HolderIntrinsic], HolderReceiverKind::Object),
                hint(receiver_layout(header), ReceiverStorage::Dictionary),
            ),
            ReadSpecialisation::StringPrototypePlace => (
                facts(&[Fact::HolderIntrinsic], HolderReceiverKind::String),
                0,
            ),
        }
    }

    fn slot(records: &[u16], flags: u8, hint: u8) -> Vec<u8> {
        let mut slot = vec![0; DUMP_SLOT_BYTES as usize];
        slot[0] = records.len() as u8;
        slot[1] = flags;
        slot[2] = hint;
        for (position, facts) in records.iter().enumerate() {
            let at = FACTS_AT as usize + 2 * position;
            slot[at..at + 2].copy_from_slice(&facts.to_le_bytes());
        }
        slot
    }

    #[test]
    fn every_specialisation_is_classified_from_its_record() {
        for specialisation in ReadSpecialisation::ALL {
            let (facts, hint) = witness(specialisation);
            assert_eq!(
                classify_record(facts, hint),
                RecordClass::Proves(specialisation),
                "{specialisation:?}"
            );
        }
    }

    #[test]
    fn records_no_specialisation_proves_are_not_named() {
        let ordinary_array = hint(ReceiverLayout::Ordinary, ReceiverStorage::Array);
        let unvisited = facts(
            &[Fact::Unvisited, Fact::NegativeSlot],
            HolderReceiverKind::None,
        );
        assert_eq!(
            classify_record(unvisited, ordinary_array),
            RecordClass::Unvisited
        );
        assert_eq!(
            classify_record(facts(&[], HolderReceiverKind::None), ordinary_array),
            RecordClass::Blank
        );
        for (bits, kind) in [
            (vec![Fact::OwnDictionary], HolderReceiverKind::None),
            (vec![Fact::OwnIntrinsic], HolderReceiverKind::None),
            (vec![Fact::HolderIntrinsic], HolderReceiverKind::Number),
            (vec![Fact::HolderIntrinsic], HolderReceiverKind::Probed),
            // A String whose prototype is no longer on its intrinsic layout.
            (vec![], HolderReceiverKind::String),
            // An empty dictionary receiver whose place is not intrinsic.
            (vec![Fact::HolderShape], HolderReceiverKind::Object),
        ] {
            assert_eq!(
                classify_record(facts(&bits, kind), ordinary_array),
                RecordClass::Unspecialised
            );
        }
        // An own compact slot of a function, or of a receiver whose layout
        // the hint does not name as compact.
        let own = facts(&[Fact::OwnShape], HolderReceiverKind::None);
        assert_eq!(
            classify_record(own, hint(ReceiverLayout::Function, ReceiverStorage::Array)),
            RecordClass::Unspecialised
        );
        assert_eq!(
            classify_record(
                own,
                hint(ReceiverLayout::Ordinary, ReceiverStorage::Dictionary)
            ),
            RecordClass::Unspecialised
        );
        assert_eq!(classify_record(own, u8::MAX), RecordClass::Unspecialised);
    }

    #[test]
    fn a_dump_slot_names_the_kind_every_record_proves() {
        let (own, inline) = witness(ReadSpecialisation::OwnCompact(CompactStorageLayout::Inline));
        let (absent, _) = witness(ReadSpecialisation::AbsentClosedTail(
            CompactStorageLayout::Inline,
        ));
        let blank = facts(&[], HolderReceiverKind::None);
        let dominated =
            |records: &[u16], flags: u8| observe_dump_slot(&slot(records, flags, inline)).unwrap();
        assert_eq!(
            dominated(&[own, blank, own], 0),
            DumpedReadSite {
                records: 3,
                megamorphic: false,
                dominant: Some(ReadSpecialisation::OwnCompact(CompactStorageLayout::Inline)),
            }
        );
        assert_eq!(dominated(&[own, absent], 0).dominant, None);
        assert_eq!(dominated(&[blank], 0).dominant, None);
        assert_eq!(dominated(&[], 0).records, 0);
        let megamorphic = dominated(&[own], FLAG_MEGAMORPHIC);
        assert!(megamorphic.megamorphic);
        assert_eq!(megamorphic.dominant, None);
        let unvisited = facts(&[Fact::Unvisited], HolderReceiverKind::None);
        assert_eq!(dominated(&[unvisited], 0).dominant, None);
    }

    #[test]
    fn malformed_dump_slots_are_rejected() {
        assert!(observe_dump_slot(&[0; 8]).is_err());
        let mut long = slot(&[], 0, 0);
        long[0] = MAX_SITE_RECORDS as u8 + 1;
        assert!(observe_dump_slot(&long).is_err());
        assert!(observe_dump_slot(&slot(&[], 2, 0)).is_err());
        // A chain of two whose second facts word is missing.
        let mut short = slot(&[Fact::Present.bit()], 0, 0);
        short[0] = 2;
        assert!(observe_dump_slot(&short).is_err());
    }
}
