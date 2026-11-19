//! The instrumented build that records an execution profile.
//!
//! A Collect-mode P defines one linear memory of its own after every other
//! memory, exported as [`PROFILE_MEMORY_EXPORT`]. Its first part holds one
//! little-endian i64 execution counter per body (counted on entry) and per
//! static property site (counted where the site loads its record), each
//! incremented by the stack-neutral
//! `i32.const 0 i32.const 0 i64.load i64.const 1 i64.add i64.store` at the
//! counter's offset. Its second part holds one slot per static read site,
//! which the exported function [`PROFILE_DUMP_EXPORT`] fills from the site's
//! record chain when the host calls it after a training run (see
//! `objects::compact_data::cache::profile_classify`). Counters are the only
//! state the instrumentation adds; they live in memory no other code
//! reaches, so the program's semantics and what its site records learn are
//! the unprofiled build's.
//!
//! The custom section [`PROFILE_SITES_SECTION`] says where everything is:
//! little-endian throughout, it holds the section version, the program key,
//! the site-map digest, the compiler fingerprint, the bytes of the profile
//! memory it describes, the bodies with their entry counter offsets, and
//! every site of the site map, in order, with its counter offset and, for a
//! read site, its dump slot offset. It names no Wasm index, so the final P
//! import compaction leaves it unchanged. [`decode`] reads it with the
//! memory after a run, so the host needs nothing of the emitter but the two.
use super::{
    encode_body, encode_bytes, encode_site, encode_u32, ExecutionProfile, ProfileError, ProgramKey,
    ReadSiteObservation, Reader, SiteBody, SiteKind, SiteMap, SiteMapBuilder, SiteMapDigest,
    SiteRef,
};
use crate::code_sink::Function;
use crate::gc_types::SiteRecordGlobal;
use crate::module::EmitError;
use crate::objects::{observe_dump_slot, DUMP_SLOT_BYTES};
use std::collections::HashMap;
use std::num::NonZeroU32;
use wasm_encoder::{Instruction, MemArg};

/// The export name of a Collect-mode P's profile memory.
pub const PROFILE_MEMORY_EXPORT: &str = "scriptw.profile";
/// The export name of a Collect-mode P's `() -> ()` profile dump, which the
/// host calls once a training run has ended and before it reads the memory.
pub const PROFILE_DUMP_EXPORT: &str = "scriptw.profile.dump";
/// The custom section that describes a Collect-mode P's profile memory.
pub const PROFILE_SITES_SECTION: &str = "scriptw.profile.sites";
/// The encoding of [`PROFILE_SITES_SECTION`] this compiler writes and reads.
pub const PROFILE_SITES_SECTION_VERSION: u16 = 1;

const COUNTER_BYTES: u32 = 8;
const PAGE_BYTES: u64 = 65_536;

/// The program and compiler a collected profile belongs to.
#[derive(Clone, Debug)]
pub(crate) struct CollectionIdentity {
    program: ProgramKey,
    compiler: String,
}

impl CollectionIdentity {
    pub(crate) fn new(program: ProgramKey, compiler: &str) -> Self {
        Self {
            program,
            compiler: compiler.to_owned(),
        }
    }
}

/// The index of the profile memory: after every memory the module imports
/// or defines for the program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProfileMemory(u32);

impl ProfileMemory {
    pub(crate) const fn after(memories: u32) -> Self {
        Self(memories)
    }

    pub(crate) const fn index(self) -> u32 {
        self.0
    }
}

/// One i64 execution counter in the profile memory.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Counter {
    memory: ProfileMemory,
    offset: u32,
}

impl Counter {
    /// Adds one to the counter, leaving the operand stack as it was.
    pub(crate) fn emit_increment(self, f: &mut Function) {
        let at = MemArg {
            offset: u64::from(self.offset),
            align: 3,
            memory_index: self.memory.index(),
        };
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I64Load(at));
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I64Store(at));
    }
}

/// The counters and read sites of a Collect-mode P, as its bodies are
/// emitted.
#[derive(Debug)]
pub(crate) struct Collector {
    identity: CollectionIdentity,
    memory: ProfileMemory,
    counters: u32,
    bodies: Vec<(SiteBody, Counter)>,
    body_counters: HashMap<SiteBody, Counter>,
    sites: Vec<(SiteRef, Counter)>,
    record_counters: HashMap<u32, Counter>,
    reads: Vec<SiteRecordGlobal>,
}

/// What module assembly adds to a Collect-mode P once every body is
/// emitted.
pub(crate) struct CollectedLayout {
    pub(crate) memory: ProfileMemory,
    /// The profile memory's size, in 64 KiB pages.
    pub(crate) pages: u64,
    /// Where the read sites' dump slots start.
    pub(crate) dump_base: u32,
    /// The record global of every read site, in site-map order.
    pub(crate) reads: Vec<SiteRecordGlobal>,
    /// The [`PROFILE_SITES_SECTION`] payload.
    pub(crate) section: Vec<u8>,
}

impl Collector {
    pub(crate) fn new(identity: CollectionIdentity, memory: ProfileMemory) -> Self {
        Self {
            identity,
            memory,
            counters: 0,
            bodies: Vec::new(),
            body_counters: HashMap::new(),
            sites: Vec::new(),
            record_counters: HashMap::new(),
            reads: Vec::new(),
        }
    }

    fn next_counter(&mut self) -> Result<Counter, EmitError> {
        let offset = self
            .counters
            .checked_mul(COUNTER_BYTES)
            .filter(|offset| offset.checked_add(COUNTER_BYTES).is_some())
            .ok_or_else(|| EmitError::unsupported("profile counters exceed the memory"))?;
        self.counters += 1;
        Ok(Counter {
            memory: self.memory,
            offset,
        })
    }

    /// Gives a new site of the site map its counter, which the site counts
    /// where it loads the record of `global`.
    pub(crate) fn site(
        &mut self,
        site: &SiteRef,
        global: SiteRecordGlobal,
    ) -> Result<(), EmitError> {
        if self.record_counters.contains_key(&global.identity()) {
            return Err(EmitError::unsupported(
                "compiler invariant: two profiled sites share a record global",
            ));
        }
        let counter = self.next_counter()?;
        self.record_counters.insert(global.identity(), counter);
        if site.kind() == SiteKind::PropertyRead {
            self.reads.push(global);
        }
        self.sites.push((site.clone(), counter));
        Ok(())
    }

    /// The counter of the site whose record is in `global`.
    pub(crate) fn site_counter(&self, global: SiteRecordGlobal) -> Option<Counter> {
        self.record_counters.get(&global.identity()).copied()
    }

    /// The entry counter of `body`; a body emitted twice shares it.
    pub(crate) fn body_counter(&mut self, body: SiteBody) -> Result<Counter, EmitError> {
        if let Some(counter) = self.body_counters.get(&body) {
            return Ok(*counter);
        }
        let counter = self.next_counter()?;
        self.body_counters.insert(body.clone(), counter);
        self.bodies.push((body, counter));
        Ok(counter)
    }

    /// The memory layout and section of the complete P, whose site map is
    /// `site_map`.
    pub(crate) fn finish(&self, site_map: &SiteMap) -> Result<CollectedLayout, EmitError> {
        if self.sites.len() != site_map.sites().len()
            || self
                .sites
                .iter()
                .zip(site_map.sites())
                .any(|((site, _), mapped)| site != mapped)
        {
            return Err(EmitError::unsupported(
                "compiler invariant: profile counters disagree with the site map",
            ));
        }
        let too_large = || EmitError::unsupported("the profile memory exceeds 4 GiB");
        let dump_base = self
            .counters
            .checked_mul(COUNTER_BYTES)
            .ok_or_else(too_large)?;
        let reads = u32::try_from(self.reads.len()).map_err(|_| too_large())?;
        let memory_bytes = reads
            .checked_mul(DUMP_SLOT_BYTES)
            .and_then(|dump| dump.checked_add(dump_base))
            .ok_or_else(too_large)?;
        let mut section = Vec::new();
        section.extend_from_slice(&PROFILE_SITES_SECTION_VERSION.to_le_bytes());
        section.extend_from_slice(self.identity.program.bytes());
        section.extend_from_slice(site_map.digest().bytes());
        encode_bytes(&mut section, self.identity.compiler.as_bytes());
        encode_u32(&mut section, memory_bytes);
        encode_u32(&mut section, self.bodies.len() as u32);
        for (body, counter) in &self.bodies {
            encode_body(&mut section, body);
            encode_u32(&mut section, counter.offset);
        }
        encode_u32(&mut section, self.sites.len() as u32);
        let mut slot = dump_base;
        for (site, counter) in &self.sites {
            encode_site(&mut section, site);
            encode_u32(&mut section, counter.offset);
            if site.kind() == SiteKind::PropertyRead {
                encode_u32(&mut section, slot);
                slot += DUMP_SLOT_BYTES;
            }
        }
        Ok(CollectedLayout {
            memory: self.memory,
            pages: u64::from(memory_bytes).div_ceil(PAGE_BYTES).max(1),
            dump_base,
            reads: self.reads.clone(),
            section,
        })
    }
}

/// Reads `memory`, the profile memory of a Collect-mode P after a training
/// run of `training_requests` requests and a call of its dump, as the
/// [`PROFILE_SITES_SECTION`] payload `section` of that P describes it.
pub(super) fn decode(
    section: &[u8],
    memory: &[u8],
    training_requests: NonZeroU32,
) -> Result<ExecutionProfile, ProfileError> {
    let mut reader = Reader { bytes: section };
    if u16::from_le_bytes(reader.array()?) != PROFILE_SITES_SECTION_VERSION {
        return Err(ProfileError::Malformed(
            "profile sites section of another version",
        ));
    }
    let program = ProgramKey(reader.array()?);
    let digest = SiteMapDigest(reader.array()?);
    let compiler = String::from_utf8(reader.sized()?.to_vec())
        .map_err(|_| ProfileError::Malformed("compiler fingerprint is not UTF-8"))?;
    let memory_bytes = reader.u32()? as usize;
    let memory = memory.get(..memory_bytes).ok_or(ProfileError::Malformed(
        "the profile memory is smaller than its section says",
    ))?;
    let counter = |offset: u32| -> Result<u64, ProfileError> {
        let at = offset as usize;
        if offset % COUNTER_BYTES != 0 {
            return Err(ProfileError::Malformed("a profile counter is misaligned"));
        }
        let bytes = memory
            .get(at..at + COUNTER_BYTES as usize)
            .ok_or(ProfileError::Malformed(
                "a profile counter is out of bounds",
            ))?;
        Ok(u64::from_le_bytes(bytes.try_into().expect("eight bytes")))
    };
    let mut bodies = Vec::new();
    for _ in 0..reader.u32()? {
        let body = reader.body()?;
        bodies.push((body, counter(reader.u32()?)?));
    }
    let mut builder = SiteMapBuilder::default();
    let mut sites = Vec::new();
    for _ in 0..reader.u32()? {
        let site = reader.site()?;
        let ordinal = builder
            .next(site.body.clone(), site.kind)
            .map_err(|_| ProfileError::Malformed("too many sites"))?
            .ordinal;
        if ordinal != site.ordinal {
            return Err(ProfileError::Malformed(
                "a site's ordinal is not its place in the site map",
            ));
        }
        let executions = counter(reader.u32()?)?;
        let observation = match site.kind {
            SiteKind::PropertyRead => {
                let at = reader.u32()? as usize;
                let slot = memory
                    .get(at..at + DUMP_SLOT_BYTES as usize)
                    .ok_or(ProfileError::Malformed("a dump slot is out of bounds"))?;
                let dumped = observe_dump_slot(slot).map_err(ProfileError::Malformed)?;
                Some(ReadSiteObservation {
                    executions,
                    dominant: dumped.dominant,
                    records: dumped.records,
                    megamorphic: dumped.megamorphic,
                })
            }
            SiteKind::PropertyWrite => None,
        };
        sites.push((site, executions, observation));
    }
    if !reader.bytes.is_empty() {
        return Err(ProfileError::Malformed("trailing bytes"));
    }
    let site_map = builder.finish();
    if site_map.digest() != digest {
        return Err(ProfileError::Malformed(
            "the sites do not make the site map the section names",
        ));
    }
    let mut profile = ExecutionProfile::new(program, &site_map, compiler, training_requests);
    for (body, entries) in &bodies {
        if profile.bodies.insert(body.clone(), *entries).is_some() {
            return Err(ProfileError::Malformed("a body is counted twice"));
        }
    }
    for (site, executions, observation) in &sites {
        match observation {
            Some(observation) => profile.observe_read(site, *observation)?,
            None => profile.observe_write(site, *executions)?,
        }
    }
    Ok(profile)
}

#[cfg(test)]
mod tests;
