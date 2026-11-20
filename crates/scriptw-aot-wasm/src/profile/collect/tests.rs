use super::*;
use crate::objects::dump_slot_for_test;
use crate::profile::{AdmittedProfile, CompactStorageLayout, ReadSpecialisation};

const PROGRAM: ProgramKey = ProgramKey::new([3; 32]);
const MEMORY: ProfileMemory = ProfileMemory::after(1);
const OWN_INLINE: ReadSpecialisation = ReadSpecialisation::OwnCompact(CompactStorageLayout::Inline);
const ABSENT_INLINE: ReadSpecialisation =
    ReadSpecialisation::AbsentClosedTail(CompactStorageLayout::Inline);

/// A P as its emitter meets it: main, a function `f` and four sites, the
/// write site between the reads.
struct Emitted {
    collector: Collector,
    site_map: SiteMap,
    main: Counter,
    f: Counter,
    sites: Vec<(SiteRef, Counter)>,
}

fn emitted() -> Emitted {
    let mut collector = Collector::new(CollectionIdentity::new(PROGRAM, "scriptw test"), MEMORY);
    let mut builder = SiteMapBuilder::default();
    let main = collector.body_counter(SiteBody::Main).unwrap();
    let mut sites = Vec::new();
    for (global, body, kind) in [
        (10, SiteBody::Main, SiteKind::PropertyRead),
        (11, SiteBody::Main, SiteKind::PropertyWrite),
        (12, SiteBody::Source("f".into()), SiteKind::PropertyRead),
        (13, SiteBody::Source("f".into()), SiteKind::PropertyRead),
    ] {
        let site = builder.next(body, kind).unwrap();
        let global = SiteRecordGlobal::for_test(global);
        collector.site(&site, global).unwrap();
        sites.push((site, collector.site_counter(global).unwrap()));
    }
    let f = collector
        .body_counter(SiteBody::Source("f".into()))
        .unwrap();
    // A body emitted again shares its counter.
    assert_eq!(
        collector.body_counter(SiteBody::Main).unwrap().offset,
        main.offset
    );
    Emitted {
        collector,
        site_map: builder.finish(),
        main,
        f,
        sites,
    }
}

fn set(memory: &mut [u8], counter: Counter, value: u64) {
    let at = counter.offset as usize;
    memory[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

/// The memory a training run of two requests leaves, after the dump:
/// main ran twice, `f` 40 times; the first read is hot and own-inline, the
/// write ran 5 times, `f`'s first read is hot but mixed and its second is
/// hot and megamorphic.
fn trained(emitted: &Emitted, layout: &CollectedLayout) -> Vec<u8> {
    let mut memory = vec![0xAA; (layout.pages * PAGE_BYTES) as usize];
    let dump_bytes = layout.reads.len() * DUMP_SLOT_BYTES as usize;
    memory[..layout.dump_base as usize + dump_bytes].fill(0);
    set(&mut memory, emitted.main, 2);
    set(&mut memory, emitted.f, 40);
    for ((_, counter), executions) in emitted.sites.iter().zip([64, 5, 80, 90]) {
        set(&mut memory, *counter, executions);
    }
    for (read, slot) in [
        dump_slot_for_test(&[OWN_INLINE, OWN_INLINE], false),
        dump_slot_for_test(&[OWN_INLINE, ABSENT_INLINE], false),
        dump_slot_for_test(&[OWN_INLINE], true),
    ]
    .into_iter()
    .enumerate()
    {
        let at = layout.dump_base as usize + read * DUMP_SLOT_BYTES as usize;
        memory[at..at + slot.len()].copy_from_slice(&slot);
    }
    memory
}

#[test]
fn the_section_lays_out_counters_then_dump_slots() {
    let emitted = emitted();
    let layout = emitted.collector.finish(&emitted.site_map).unwrap();
    assert_eq!(layout.memory, MEMORY);
    // Six counters: two bodies and four sites, eight bytes each.
    assert_eq!(layout.dump_base, 6 * COUNTER_BYTES);
    assert_eq!(layout.pages, 1);
    assert_eq!(
        layout
            .reads
            .iter()
            .map(|global| global.identity())
            .collect::<Vec<_>>(),
        [10, 12, 13]
    );
    let section = &layout.section;
    assert_eq!(section[..2], PROFILE_SITES_SECTION_VERSION.to_le_bytes());
    assert_eq!(&section[2..34], PROGRAM.bytes());
    assert_eq!(&section[34..66], emitted.site_map.digest().bytes());
    let compiler = b"scriptw test";
    assert_eq!(section[66..70], (compiler.len() as u32).to_le_bytes());
    assert_eq!(&section[70..70 + compiler.len()], compiler);
    let memory_bytes = 6 * COUNTER_BYTES + 3 * DUMP_SLOT_BYTES;
    assert_eq!(
        section[70 + compiler.len()..74 + compiler.len()],
        memory_bytes.to_le_bytes()
    );
    // Counter offsets are distinct, aligned and below the dump.
    let mut offsets = emitted
        .sites
        .iter()
        .map(|(_, counter)| counter.offset)
        .chain([emitted.main.offset, emitted.f.offset])
        .collect::<Vec<_>>();
    offsets.sort();
    offsets.dedup();
    assert_eq!(offsets.len(), 6);
    assert!(offsets
        .iter()
        .all(|offset| offset % COUNTER_BYTES == 0 && *offset < layout.dump_base));
}

#[test]
fn a_trained_memory_decodes_into_the_profile() {
    let emitted = emitted();
    let layout = emitted.collector.finish(&emitted.site_map).unwrap();
    let memory = trained(&emitted, &layout);
    let requests = NonZeroU32::new(2).unwrap();
    let profile = decode(&layout.section, &memory, requests).unwrap();
    assert_eq!(profile.program(), &PROGRAM);
    assert_eq!(profile.site_map_digest(), &emitted.site_map.digest());
    assert_eq!(profile.compiler(), "scriptw test");
    assert_eq!(profile.training_requests(), requests);
    assert_eq!(
        profile.bodies().collect::<Vec<_>>(),
        [(&SiteBody::Main, &2), (&SiteBody::Source("f".into()), &40)]
    );
    let reads = profile
        .reads()
        .map(|(site, observation)| (site.clone(), *observation))
        .collect::<Vec<_>>();
    let sites = &emitted.sites;
    assert_eq!(
        reads,
        [
            (
                sites[0].0.clone(),
                ReadSiteObservation {
                    executions: 64,
                    dominant: Some(OWN_INLINE),
                    records: 2,
                    megamorphic: false,
                }
            ),
            (
                sites[2].0.clone(),
                ReadSiteObservation {
                    executions: 80,
                    dominant: None,
                    records: 2,
                    megamorphic: false,
                }
            ),
            (
                sites[3].0.clone(),
                ReadSiteObservation {
                    executions: 90,
                    dominant: None,
                    records: 1,
                    megamorphic: true,
                }
            ),
        ]
    );
    assert_eq!(profile.writes().collect::<Vec<_>>(), [(&sites[1].0, &5)]);
    // The profile round-trips through .swprof and admits only the hot,
    // dominated read.
    assert_eq!(
        ExecutionProfile::decode(&profile.encode()),
        Ok(profile.clone())
    );
    let admitted = AdmittedProfile::admit(&profile, &PROGRAM).unwrap();
    assert_eq!(admitted.specialised_reads(), 1);
    assert_eq!(admitted.read_specialisation(&sites[0].0), Some(OWN_INLINE));
    assert!(admitted.require_site_map(&emitted.site_map).is_ok());
}

#[test]
fn an_untrained_memory_decodes_into_cold_sites() {
    let emitted = emitted();
    let layout = emitted.collector.finish(&emitted.site_map).unwrap();
    let memory = vec![0; (layout.pages * PAGE_BYTES) as usize];
    let profile = decode(&layout.section, &memory, NonZeroU32::MIN).unwrap();
    assert_eq!(profile.reads().count(), 3);
    assert!(profile
        .reads()
        .all(|(_, observation)| observation.executions == 0
            && observation.records == 0
            && observation.dominant.is_none()));
    assert_eq!(profile.writes().count(), 1);
}

#[test]
fn malformed_sections_and_memories_are_rejected() {
    let emitted = emitted();
    let layout = emitted.collector.finish(&emitted.site_map).unwrap();
    let memory = trained(&emitted, &layout);
    let decode = |section: &[u8], memory: &[u8]| decode(section, memory, NonZeroU32::MIN);
    assert!(decode(&layout.section, &memory).is_ok());
    let memory_bytes = (layout.dump_base + 3 * DUMP_SLOT_BYTES) as usize;
    assert!(decode(&layout.section, &memory[..memory_bytes]).is_ok());
    assert!(matches!(
        decode(&layout.section, &memory[..memory_bytes - 1]),
        Err(ProfileError::Malformed(_))
    ));
    let mut version = layout.section.clone();
    version[0] ^= 1;
    assert_eq!(
        decode(&version, &memory).unwrap_err(),
        ProfileError::Mismatch(super::ProfileMismatch::Version)
    );
    let mut digest = layout.section.clone();
    digest[40] ^= 1;
    assert_eq!(
        decode(&digest, &memory),
        Err(ProfileError::Malformed(
            "the sites do not make the site map the section names"
        ))
    );
    let mut trailing = layout.section.clone();
    trailing.push(0);
    assert_eq!(
        decode(&trailing, &memory),
        Err(ProfileError::Malformed("trailing bytes"))
    );
    for len in [0, 1, 40, layout.section.len() - 1] {
        assert!(decode(&layout.section[..len], &memory).is_err());
    }
    // A dump slot claiming more records than a chain holds.
    let mut slot = memory.clone();
    slot[layout.dump_base as usize] = 9;
    assert!(decode(&layout.section, &slot).is_err());
}

#[test]
fn collection_invariants_are_checked() {
    let mut emitted = emitted();
    let mut builder = SiteMapBuilder::default();
    let site = builder
        .next(SiteBody::Main, SiteKind::PropertyRead)
        .unwrap();
    // A record global names one site.
    assert!(emitted
        .collector
        .site(&site, SiteRecordGlobal::for_test(10))
        .is_err());
    // The counters must describe the site map that is finished.
    assert!(emitted.collector.finish(&builder.finish()).is_err());
    assert!(emitted
        .collector
        .site_counter(SiteRecordGlobal::for_test(99))
        .is_none());
}
