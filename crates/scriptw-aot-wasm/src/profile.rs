//! Execution profiles for profile-guided compilation of P.
//!
//! A profile changes emitted P only. R stays shared and profile-independent:
//! no runtime ABI, cache or object model depends on a profile. Every
//! specialisation a profile selects is a guarded prefix of pure loads and
//! tests that observes nothing and falls through to exactly the code an
//! unprofiled build emits, so a wrong profile costs speed only.
//!
//! Sites are named by [`SiteRef`]: the body of P a site is compiled in, the
//! kind of site and its ordinal among the sites of that kind in that body.
//! The emitter enumerates the [`SiteMap`] of a P in emission order, and
//! enumerates it identically whatever the profile mode; the map's digest
//! binds a profile to the build it was trained on.
//!
//! A profile is admitted once ([`AdmittedProfile::admit`]) against the
//! program it is applied to; emission then only looks sites up. The emitter
//! checks the one thing only emission can know, that the site map it
//! enumerates is the one the profile was trained on, when P is complete.
//!
//! A profile is recorded by an instrumented build of the same P
//! ([`ProfileMode::Collect`], see [`collect`]): its site map is the
//! unprofiled build's, its JavaScript semantics and what its site records
//! learn are the unprofiled build's, and only time changes. After a training
//! run, [`ExecutionProfile::from_collection`] turns the build's
//! `scriptw.profile.sites` section and its `scriptw.profile` memory into a
//! profile.
use crate::module::EmitError;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroU32;
use std::sync::Arc;

pub(crate) mod collect;
pub use collect::{
    PROFILE_DUMP_EXPORT, PROFILE_MEMORY_EXPORT, PROFILE_SITES_SECTION,
    PROFILE_SITES_SECTION_VERSION,
};

/// The magic that opens an encoded profile.
const MAGIC: &[u8; 6] = b"SWPROF";

/// The version of the encoding this compiler reads and writes. A profile of
/// any other version is rejected, never reinterpreted.
pub const PROFILE_FORMAT_VERSION: u16 = 1;

/// The executions per training request at which a static read site is hot
/// enough for its dominant kind to be proved inline.
pub const HOT_READ_EXECUTIONS_PER_REQUEST: u64 = 8;

/// How an emission uses an execution profile.
#[derive(Clone, Copy, Debug, Default)]
pub enum ProfileMode<'a> {
    /// No profile: P is exactly the unprofiled build.
    #[default]
    None,
    /// An instrumented build that records a profile of the program whose
    /// key is `program`, compiled by the compiler `compiler` names
    /// (informational). P counts every body entry and every static property
    /// site's executions in a memory of its own, exports a function that
    /// writes what each read site's records learned next to the counts, and
    /// describes both in a custom section; see [`collect`].
    Collect {
        program: ProgramKey,
        compiler: &'a str,
    },
    /// A build specialised by an admitted profile.
    Apply(&'a AdmittedProfile),
}

/// The P body a site is compiled in.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SiteBody {
    /// The program's main body.
    Main,
    /// A source function or prepared Script body, by its IR function id.
    Source(String),
}

/// The kinds of site a profile names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SiteKind {
    /// A static property read site (`base.name`) with its record global.
    PropertyRead,
    /// A static property write site (`base.name = value`) with its record
    /// global.
    PropertyWrite,
}

impl SiteKind {
    const fn code(self) -> u8 {
        match self {
            Self::PropertyRead => 0,
            Self::PropertyWrite => 1,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        [Self::PropertyRead, Self::PropertyWrite]
            .into_iter()
            .find(|kind| kind.code() == code)
    }
}

/// One site of a P: its body, kind and ordinal among the sites of that kind
/// in that body, in emission order. Only the emitter's site map and the
/// profile decoder make one.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SiteRef {
    body: SiteBody,
    kind: SiteKind,
    ordinal: u32,
}

impl SiteRef {
    pub fn body(&self) -> &SiteBody {
        &self.body
    }

    pub fn kind(&self) -> SiteKind {
        self.kind
    }

    pub fn ordinal(&self) -> u32 {
        self.ordinal
    }
}

/// The sites of one P, in the order the emitter met them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SiteMap {
    sites: Vec<SiteRef>,
}

impl SiteMap {
    pub fn sites(&self) -> &[SiteRef] {
        &self.sites
    }

    pub fn read_sites(&self) -> impl Iterator<Item = &SiteRef> {
        self.sites
            .iter()
            .filter(|site| site.kind == SiteKind::PropertyRead)
    }

    /// SHA-256 of the ordered site references.
    pub fn digest(&self) -> SiteMapDigest {
        let mut hasher = Sha256::new();
        let mut bytes = Vec::new();
        encode_u32(&mut bytes, self.sites.len() as u32);
        for site in &self.sites {
            encode_site(&mut bytes, site);
        }
        hasher.update(&bytes);
        SiteMapDigest(hasher.finalize().into())
    }
}

/// Enumerates a P's site map as its bodies are emitted. Each body's
/// ordinals count from zero per kind.
#[derive(Default)]
pub(crate) struct SiteMapBuilder {
    sites: Vec<SiteRef>,
    ordinals: HashMap<(SiteBody, SiteKind), u32>,
}

impl SiteMapBuilder {
    pub(crate) fn next(&mut self, body: SiteBody, kind: SiteKind) -> Result<SiteRef, EmitError> {
        let next = self.ordinals.entry((body.clone(), kind)).or_insert(0);
        let ordinal = *next;
        *next = next
            .checked_add(1)
            .ok_or_else(|| EmitError::unsupported("profile sites of one body exceed u32"))?;
        let site = SiteRef {
            body,
            kind,
            ordinal,
        };
        self.sites.push(site.clone());
        Ok(site)
    }

    pub(crate) fn finish(&self) -> SiteMap {
        SiteMap {
            sites: self.sites.clone(),
        }
    }
}

/// SHA-256 of a [`SiteMap`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SiteMapDigest([u8; 32]);

impl SiteMapDigest {
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// The identity of the program and compiler a profile was trained with:
/// the engine's program Wasm cache key computed with no profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProgramKey([u8; 32]);

impl ProgramKey {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// The two sealed compact storage layouts a read's receiver may have.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompactStorageLayout {
    Array,
    Inline,
}

/// The storage of the prototype that holds a read's place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaceHolder {
    /// An immutable intrinsic property layout, as an unmodified intrinsic
    /// prototype has.
    Intrinsic,
    /// Compact storage of either sealed layout.
    Compact,
    /// A dictionary.
    Dictionary,
}

/// The sealed object layouts whose ordinary header holds every named own
/// property a site record can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReceiverHeader {
    Ordinary,
    Function,
    Array,
    PrimitiveBox,
    Map,
    WeakMap,
    Set,
    WeakSet,
    Promise,
}

impl ReceiverHeader {
    pub const ALL: [Self; 9] = [
        Self::Ordinary,
        Self::Function,
        Self::Array,
        Self::PrimitiveBox,
        Self::Map,
        Self::WeakMap,
        Self::Set,
        Self::WeakSet,
        Self::Promise,
    ];
}

/// The one kind of answer a hot static read site proves inline, ahead of
/// the R hit entry it keeps calling for every other receiver. Each is a
/// proof the hit entry itself makes, emitted by the same emitters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReadSpecialisation {
    /// An actual OrdinaryObject on compact storage of this layout owns the
    /// key at a record's slot: the live shape is the record's.
    OwnCompact(CompactStorageLayout),
    /// An actual OrdinaryObject on compact storage of this layout lacks the
    /// key by a record's shape, and its live chain is the closed tail the
    /// record registered: the read is undefined.
    AbsentClosedTail(CompactStorageLayout),
    /// An actual OrdinaryObject on compact storage of this layout lacks the
    /// key by a record's receiver rule, and the record's place on its live
    /// [[Prototype]], whose storage is the holder's, holds it.
    ProtoPlaceCompact(CompactStorageLayout, PlaceHolder),
    /// A receiver with this header on a dictionary that holds no entry: the
    /// record's place on its live [[Prototype]], an intrinsic prototype
    /// such as %WeakMap.prototype% or %Function.prototype% on its intrinsic
    /// layout, holds the key.
    ProtoPlaceIntrinsic(ReceiverHeader),
    /// A String: the record's place on the current Realm's
    /// %String.prototype%, on its intrinsic layout, holds the key.
    StringPrototypePlace,
}

impl ReadSpecialisation {
    /// Every specialisation, in code order.
    pub const ALL: [Self; 20] = [
        Self::OwnCompact(CompactStorageLayout::Array),
        Self::OwnCompact(CompactStorageLayout::Inline),
        Self::AbsentClosedTail(CompactStorageLayout::Array),
        Self::AbsentClosedTail(CompactStorageLayout::Inline),
        Self::ProtoPlaceCompact(CompactStorageLayout::Array, PlaceHolder::Intrinsic),
        Self::ProtoPlaceCompact(CompactStorageLayout::Array, PlaceHolder::Compact),
        Self::ProtoPlaceCompact(CompactStorageLayout::Array, PlaceHolder::Dictionary),
        Self::ProtoPlaceCompact(CompactStorageLayout::Inline, PlaceHolder::Intrinsic),
        Self::ProtoPlaceCompact(CompactStorageLayout::Inline, PlaceHolder::Compact),
        Self::ProtoPlaceCompact(CompactStorageLayout::Inline, PlaceHolder::Dictionary),
        Self::ProtoPlaceIntrinsic(ReceiverHeader::Ordinary),
        Self::ProtoPlaceIntrinsic(ReceiverHeader::Function),
        Self::ProtoPlaceIntrinsic(ReceiverHeader::Array),
        Self::ProtoPlaceIntrinsic(ReceiverHeader::PrimitiveBox),
        Self::ProtoPlaceIntrinsic(ReceiverHeader::Map),
        Self::ProtoPlaceIntrinsic(ReceiverHeader::WeakMap),
        Self::ProtoPlaceIntrinsic(ReceiverHeader::Set),
        Self::ProtoPlaceIntrinsic(ReceiverHeader::WeakSet),
        Self::ProtoPlaceIntrinsic(ReceiverHeader::Promise),
        Self::StringPrototypePlace,
    ];

    pub(crate) const fn code(self) -> u8 {
        const fn layout(layout: CompactStorageLayout) -> u8 {
            match layout {
                CompactStorageLayout::Array => 0,
                CompactStorageLayout::Inline => 1,
            }
        }
        const fn header(header: ReceiverHeader) -> u8 {
            match header {
                ReceiverHeader::Ordinary => 0,
                ReceiverHeader::Function => 1,
                ReceiverHeader::Array => 2,
                ReceiverHeader::PrimitiveBox => 3,
                ReceiverHeader::Map => 4,
                ReceiverHeader::WeakMap => 5,
                ReceiverHeader::Set => 6,
                ReceiverHeader::WeakSet => 7,
                ReceiverHeader::Promise => 8,
            }
        }
        const fn holder(holder: PlaceHolder) -> u8 {
            match holder {
                PlaceHolder::Intrinsic => 0,
                PlaceHolder::Compact => 1,
                PlaceHolder::Dictionary => 2,
            }
        }
        match self {
            Self::OwnCompact(storage) => layout(storage),
            Self::AbsentClosedTail(storage) => 2 + layout(storage),
            Self::ProtoPlaceCompact(storage, place) => 4 + 3 * layout(storage) + holder(place),
            Self::ProtoPlaceIntrinsic(receiver) => 10 + header(receiver),
            Self::StringPrototypePlace => 19,
        }
    }

    pub(crate) fn from_code(code: u8) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|specialisation| specialisation.code() == code)
    }
}

/// What a training run saw at one static read site: how often it ran, the
/// kind of answer that dominated, if one did, and the shape of its record
/// chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadSiteObservation {
    pub executions: u64,
    /// The one kind of answer the site's records prove, when every record
    /// of its chain proves that kind and the site still learned.
    pub dominant: Option<ReadSpecialisation>,
    /// The records of the site's chain at the end of the run; zero for a
    /// site that never ran.
    pub records: u8,
    /// The site spent its learning budget: receivers its records cannot
    /// name were common enough that its entry answers them by walking.
    pub megamorphic: bool,
}

impl ReadSiteObservation {
    /// A site of `executions` whose one record proves `dominant`.
    pub fn dominated(executions: u64, dominant: ReadSpecialisation) -> Self {
        Self {
            executions,
            dominant: Some(dominant),
            records: 1,
            megamorphic: false,
        }
    }
}

/// A recorded execution profile, as written to and read from `.swprof`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionProfile {
    program: ProgramKey,
    site_map: SiteMapDigest,
    compiler: String,
    training_requests: NonZeroU32,
    /// How often each body was entered.
    bodies: BTreeMap<SiteBody, u64>,
    reads: BTreeMap<SiteRef, ReadSiteObservation>,
    /// How often each static write site ran.
    writes: BTreeMap<SiteRef, u64>,
}

/// Why a profile was not admitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileMismatch {
    /// Trained on another program or compiler.
    Program,
    /// The emitter enumerated another site map than the training build.
    SiteMap,
    /// Encoded in a version this compiler does not read.
    Version,
}

/// An error reading or admitting a profile. A profile is never silently
/// ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileError {
    Mismatch(ProfileMismatch),
    Malformed(&'static str),
    /// A read observation names a site of another kind.
    NotAReadSite,
    /// A write observation names a site of another kind.
    NotAWriteSite,
    /// The program was not built to collect a profile, so it has none to
    /// read.
    NotCollecting,
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mismatch(ProfileMismatch::Program) => {
                f.write_str("execution profile mismatch: trained on another program or compiler")
            }
            Self::Mismatch(ProfileMismatch::SiteMap) => f.write_str(
                "execution profile mismatch: the emitted site map is not the training build's",
            ),
            Self::Mismatch(ProfileMismatch::Version) => write!(
                f,
                "execution profile mismatch: not format version {PROFILE_FORMAT_VERSION}"
            ),
            Self::Malformed(what) => write!(f, "malformed execution profile: {what}"),
            Self::NotAReadSite => f.write_str("read observation for a site that is not a read"),
            Self::NotAWriteSite => f.write_str("write observation for a site that is not a write"),
            Self::NotCollecting => f.write_str(
                "this program records no execution profile: build it with profile collection",
            ),
        }
    }
}

impl std::error::Error for ProfileError {}

impl ProfileError {
    pub(crate) fn into_emit_error(self) -> EmitError {
        EmitError::profile(self)
    }
}

impl ExecutionProfile {
    /// An empty profile of `training_requests` requests of the program
    /// `program`, whose build enumerated `site_map`.
    pub fn new(
        program: ProgramKey,
        site_map: &SiteMap,
        compiler: impl Into<String>,
        training_requests: NonZeroU32,
    ) -> Self {
        Self {
            program,
            site_map: site_map.digest(),
            compiler: compiler.into(),
            training_requests,
            bodies: BTreeMap::new(),
            reads: BTreeMap::new(),
            writes: BTreeMap::new(),
        }
    }

    /// The profile a Collect-mode P recorded over `training_requests`
    /// requests: `section` is its `scriptw.profile.sites` custom section and
    /// `memory` its `scriptw.profile` memory, read after the run ended and
    /// the P's `scriptw.profile.dump` export was called. Every number is
    /// checked against the section; nothing is reinterpreted.
    pub fn from_collection(
        section: &[u8],
        memory: &[u8],
        training_requests: NonZeroU32,
    ) -> Result<Self, ProfileError> {
        collect::decode(section, memory, training_requests)
    }

    /// Records how often a body was entered; a later observation of the
    /// same body replaces it.
    pub fn observe_body(&mut self, body: &SiteBody, entries: u64) {
        self.bodies.insert(body.clone(), entries);
    }

    /// Records how often a static write site ran; a later observation of
    /// the same site replaces it.
    pub fn observe_write(&mut self, site: &SiteRef, executions: u64) -> Result<(), ProfileError> {
        if site.kind != SiteKind::PropertyWrite {
            return Err(ProfileError::NotAWriteSite);
        }
        self.writes.insert(site.clone(), executions);
        Ok(())
    }

    /// Records what a static read site saw; a later observation of the same
    /// site replaces it.
    pub fn observe_read(
        &mut self,
        site: &SiteRef,
        observation: ReadSiteObservation,
    ) -> Result<(), ProfileError> {
        if site.kind != SiteKind::PropertyRead {
            return Err(ProfileError::NotAReadSite);
        }
        self.reads.insert(site.clone(), observation);
        Ok(())
    }

    /// The read sites admission specialises, with their dominant kinds:
    /// those with a dominant kind that ran at least
    /// [`HOT_READ_EXECUTIONS_PER_REQUEST`] times per training request.
    pub fn hot_reads(&self) -> impl Iterator<Item = (&SiteRef, ReadSpecialisation)> {
        let threshold =
            HOT_READ_EXECUTIONS_PER_REQUEST.saturating_mul(u64::from(self.training_requests.get()));
        self.reads
            .iter()
            .filter(move |(_, observation)| observation.executions >= threshold)
            .filter_map(|(site, observation)| Some((site, observation.dominant?)))
    }

    pub fn program(&self) -> &ProgramKey {
        &self.program
    }

    pub fn site_map_digest(&self) -> &SiteMapDigest {
        &self.site_map
    }

    pub fn compiler(&self) -> &str {
        &self.compiler
    }

    pub fn training_requests(&self) -> NonZeroU32 {
        self.training_requests
    }

    pub fn bodies(&self) -> impl Iterator<Item = (&SiteBody, &u64)> {
        self.bodies.iter()
    }

    pub fn reads(&self) -> impl Iterator<Item = (&SiteRef, &ReadSiteObservation)> {
        self.reads.iter()
    }

    pub fn writes(&self) -> impl Iterator<Item = (&SiteRef, &u64)> {
        self.writes.iter()
    }

    /// The `.swprof` encoding: magic, u16 version, identity, training
    /// request count, then the body, read-site and write-site tables,
    /// little-endian throughout.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&PROFILE_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.program.0);
        bytes.extend_from_slice(&self.site_map.0);
        encode_bytes(&mut bytes, self.compiler.as_bytes());
        encode_u32(&mut bytes, self.training_requests.get());
        encode_u32(&mut bytes, self.bodies.len() as u32);
        for (body, entries) in &self.bodies {
            encode_body(&mut bytes, body);
            bytes.extend_from_slice(&entries.to_le_bytes());
        }
        encode_u32(&mut bytes, self.reads.len() as u32);
        for (site, observation) in &self.reads {
            encode_site(&mut bytes, site);
            bytes.extend_from_slice(&observation.executions.to_le_bytes());
            bytes.push(
                observation
                    .dominant
                    .map_or(NO_DOMINANT_KIND, ReadSpecialisation::code),
            );
            bytes.push(observation.records);
            bytes.push(u8::from(observation.megamorphic));
        }
        encode_u32(&mut bytes, self.writes.len() as u32);
        for (site, executions) in &self.writes {
            encode_site(&mut bytes, site);
            bytes.extend_from_slice(&executions.to_le_bytes());
        }
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProfileError> {
        let mut reader = Reader { bytes };
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(ProfileError::Malformed("not an execution profile"));
        }
        let version = u16::from_le_bytes(reader.array()?);
        if version != PROFILE_FORMAT_VERSION {
            return Err(ProfileError::Mismatch(ProfileMismatch::Version));
        }
        let program = ProgramKey(reader.array()?);
        let site_map = SiteMapDigest(reader.array()?);
        let compiler = String::from_utf8(reader.sized()?.to_vec())
            .map_err(|_| ProfileError::Malformed("compiler fingerprint is not UTF-8"))?;
        let training_requests =
            NonZeroU32::new(reader.u32()?).ok_or(ProfileError::Malformed("no training request"))?;
        let mut bodies = BTreeMap::new();
        for _ in 0..reader.u32()? {
            let body = reader.body()?;
            let entries = u64::from_le_bytes(reader.array()?);
            if bodies.insert(body, entries).is_some() {
                return Err(ProfileError::Malformed("a body is recorded twice"));
            }
        }
        let mut reads = BTreeMap::new();
        for _ in 0..reader.u32()? {
            let site = reader.site()?;
            if site.kind != SiteKind::PropertyRead {
                return Err(ProfileError::NotAReadSite);
            }
            let executions = u64::from_le_bytes(reader.array()?);
            let [dominant, records, megamorphic] = reader.array::<3>()?;
            let dominant = match dominant {
                NO_DOMINANT_KIND => None,
                code => Some(
                    ReadSpecialisation::from_code(code)
                        .ok_or(ProfileError::Malformed("unknown read specialisation"))?,
                ),
            };
            let megamorphic = match megamorphic {
                0 => false,
                1 => true,
                _ => return Err(ProfileError::Malformed("megamorphic flag is not 0 or 1")),
            };
            // Only states a collected observation can have: a chain of at
            // most a chain's records, and a dominant kind only at a site
            // that still learns and has a record proving it.
            if records > crate::objects::MAX_OBSERVED_RECORDS {
                return Err(ProfileError::Malformed(
                    "a read site names more records than a chain holds",
                ));
            }
            if dominant.is_some() && (records == 0 || megamorphic) {
                return Err(ProfileError::Malformed(
                    "a dominant kind at a site without records or that stopped learning",
                ));
            }
            if reads
                .insert(
                    site,
                    ReadSiteObservation {
                        executions,
                        dominant,
                        records,
                        megamorphic,
                    },
                )
                .is_some()
            {
                return Err(ProfileError::Malformed("a read site is recorded twice"));
            }
        }
        let mut writes = BTreeMap::new();
        for _ in 0..reader.u32()? {
            let site = reader.site()?;
            if site.kind != SiteKind::PropertyWrite {
                return Err(ProfileError::NotAWriteSite);
            }
            let executions = u64::from_le_bytes(reader.array()?);
            if writes.insert(site, executions).is_some() {
                return Err(ProfileError::Malformed("a write site is recorded twice"));
            }
        }
        if !reader.bytes.is_empty() {
            return Err(ProfileError::Malformed("trailing bytes"));
        }
        Ok(Self {
            program,
            site_map,
            compiler,
            training_requests,
            bodies,
            reads,
            writes,
        })
    }
}

/// A profile admitted for one program: its identity checked, and its hot
/// read sites chosen. Emission takes only this.
#[derive(Clone, Debug)]
pub struct AdmittedProfile(Arc<Admitted>);

#[derive(Debug)]
struct Admitted {
    site_map: SiteMapDigest,
    reads: HashMap<SiteRef, ReadSpecialisation>,
}

impl AdmittedProfile {
    /// Admits `profile` for the program whose key is `program`. A read site
    /// is specialised when it has a dominant kind and ran at least
    /// [`HOT_READ_EXECUTIONS_PER_REQUEST`] times per training request.
    pub fn admit(profile: &ExecutionProfile, program: &ProgramKey) -> Result<Self, ProfileError> {
        if profile.program != *program {
            return Err(ProfileError::Mismatch(ProfileMismatch::Program));
        }
        let reads = profile
            .hot_reads()
            .map(|(site, dominant)| (site.clone(), dominant))
            .collect();
        Ok(Self(Arc::new(Admitted {
            site_map: profile.site_map,
            reads,
        })))
    }

    pub fn site_map_digest(&self) -> &SiteMapDigest {
        &self.0.site_map
    }

    /// How many read sites this profile specialises.
    pub fn specialised_reads(&self) -> usize {
        self.0.reads.len()
    }

    pub(crate) fn read_specialisation(&self, site: &SiteRef) -> Option<ReadSpecialisation> {
        self.0.reads.get(site).copied()
    }

    /// Fails unless the site map the emitter enumerated is the one this
    /// profile was trained on.
    pub(crate) fn require_site_map(&self, site_map: &SiteMap) -> Result<(), ProfileError> {
        if site_map.digest() == self.0.site_map {
            Ok(())
        } else {
            Err(ProfileError::Mismatch(ProfileMismatch::SiteMap))
        }
    }
}

/// What an emission does with its profile, owned past the entry point.
#[derive(Clone, Debug, Default)]
pub(crate) enum ProfileUse {
    #[default]
    None,
    Collect(collect::CollectionIdentity),
    Apply(AdmittedProfile),
}

impl ProfileUse {
    pub(crate) fn of(mode: ProfileMode<'_>) -> Self {
        match mode {
            ProfileMode::None => Self::None,
            ProfileMode::Collect { program, compiler } => {
                Self::Collect(collect::CollectionIdentity::new(program, compiler))
            }
            ProfileMode::Apply(profile) => Self::Apply(profile.clone()),
        }
    }
}

/// Encoding of a read observation without a dominant kind.
const NO_DOMINANT_KIND: u8 = u8::MAX;

fn encode_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn encode_bytes(bytes: &mut Vec<u8>, value: &[u8]) {
    encode_u32(bytes, value.len() as u32);
    bytes.extend_from_slice(value);
}

fn encode_body(bytes: &mut Vec<u8>, body: &SiteBody) {
    match body {
        SiteBody::Main => bytes.push(0),
        SiteBody::Source(id) => {
            bytes.push(1);
            encode_bytes(bytes, id.as_bytes());
        }
    }
}

fn encode_site(bytes: &mut Vec<u8>, site: &SiteRef) {
    encode_body(bytes, &site.body);
    bytes.push(site.kind.code());
    encode_u32(bytes, site.ordinal);
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], ProfileError> {
        if self.bytes.len() < len {
            return Err(ProfileError::Malformed("truncated"));
        }
        let (taken, rest) = self.bytes.split_at(len);
        self.bytes = rest;
        Ok(taken)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ProfileError> {
        Ok(self.take(N)?.try_into().expect("took N bytes"))
    }

    fn u32(&mut self) -> Result<u32, ProfileError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn sized(&mut self) -> Result<&'a [u8], ProfileError> {
        let len = self.u32()? as usize;
        self.take(len)
    }

    fn body(&mut self) -> Result<SiteBody, ProfileError> {
        Ok(match self.array::<1>()?[0] {
            0 => SiteBody::Main,
            1 => SiteBody::Source(
                String::from_utf8(self.sized()?.to_vec())
                    .map_err(|_| ProfileError::Malformed("body id is not UTF-8"))?,
            ),
            _ => return Err(ProfileError::Malformed("unknown site body")),
        })
    }

    fn site(&mut self) -> Result<SiteRef, ProfileError> {
        let body = self.body()?;
        let kind = SiteKind::from_code(self.array::<1>()?[0])
            .ok_or(ProfileError::Malformed("unknown site kind"))?;
        let ordinal = self.u32()?;
        Ok(SiteRef {
            body,
            kind,
            ordinal,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site_map() -> SiteMap {
        let mut builder = SiteMapBuilder::default();
        for (body, kind) in [
            (SiteBody::Main, SiteKind::PropertyRead),
            (SiteBody::Source("f1".into()), SiteKind::PropertyRead),
            (SiteBody::Main, SiteKind::PropertyWrite),
            (SiteBody::Main, SiteKind::PropertyRead),
            (SiteBody::Source("f1".into()), SiteKind::PropertyRead),
        ] {
            builder.next(body, kind).unwrap();
        }
        builder.finish()
    }

    fn profile(map: &SiteMap) -> ExecutionProfile {
        let mut profile = ExecutionProfile::new(
            ProgramKey::new([7; 32]),
            map,
            "scriptw test",
            NonZeroU32::new(2).unwrap(),
        );
        for (site, (executions, dominant)) in map.read_sites().zip(
            [16, 15, 400, 0]
                .into_iter()
                .zip(ReadSpecialisation::ALL.into_iter().rev()),
        ) {
            profile
                .observe_read(site, ReadSiteObservation::dominated(executions, dominant))
                .unwrap();
        }
        profile
    }

    #[test]
    fn site_map_counts_ordinals_per_body_and_kind() {
        let ordinals = site_map()
            .sites()
            .iter()
            .map(|site| (site.body().clone(), site.kind(), site.ordinal()))
            .collect::<Vec<_>>();
        assert_eq!(
            ordinals,
            [
                (SiteBody::Main, SiteKind::PropertyRead, 0),
                (SiteBody::Source("f1".into()), SiteKind::PropertyRead, 0),
                (SiteBody::Main, SiteKind::PropertyWrite, 0),
                (SiteBody::Main, SiteKind::PropertyRead, 1),
                (SiteBody::Source("f1".into()), SiteKind::PropertyRead, 1),
            ]
        );
        let mut reordered = site_map();
        reordered.sites.swap(0, 1);
        assert_ne!(reordered.digest(), site_map().digest());
    }

    #[test]
    fn every_specialisation_has_its_own_code() {
        for specialisation in ReadSpecialisation::ALL {
            assert_eq!(
                ReadSpecialisation::from_code(specialisation.code()),
                Some(specialisation)
            );
        }
        for (code, specialisation) in ReadSpecialisation::ALL.into_iter().enumerate() {
            assert_eq!(usize::from(specialisation.code()), code);
        }
        assert_eq!(ReadSpecialisation::from_code(20), None);
    }

    #[test]
    fn codec_round_trips() {
        let map = site_map();
        let profile = profile(&map);
        let bytes = profile.encode();
        assert_eq!(&bytes[..6], b"SWPROF");
        assert_eq!(ExecutionProfile::decode(&bytes), Ok(profile.clone()));
        assert_eq!(profile.reads().count(), 4);
        assert_eq!(profile.site_map_digest(), &map.digest());
    }

    #[test]
    fn decoding_rejects_other_versions_and_malformed_bytes() {
        let bytes = profile(&site_map()).encode();
        let mut other_version = bytes.clone();
        other_version[6..8].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(
            ExecutionProfile::decode(&other_version),
            Err(ProfileError::Mismatch(ProfileMismatch::Version))
        );
        let mut magic = bytes.clone();
        magic[0] = b'X';
        assert!(matches!(
            ExecutionProfile::decode(&magic),
            Err(ProfileError::Malformed(_))
        ));
        for len in [0, 7, 40, bytes.len() - 1] {
            assert!(matches!(
                ExecutionProfile::decode(&bytes[..len]),
                Err(ProfileError::Malformed(_))
            ));
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            ExecutionProfile::decode(&trailing),
            Err(ProfileError::Malformed("trailing bytes"))
        );
        // The last read's kind, flag and chain length precede the empty
        // write table's count.
        let mut flag = bytes.clone();
        let at = flag.len() - 5;
        flag[at] = 2;
        assert_eq!(
            ExecutionProfile::decode(&flag),
            Err(ProfileError::Malformed("megamorphic flag is not 0 or 1"))
        );
        let mut kind = bytes;
        let at = kind.len() - 7;
        kind[at] = 99;
        assert_eq!(
            ExecutionProfile::decode(&kind),
            Err(ProfileError::Malformed("unknown read specialisation"))
        );
    }

    #[test]
    fn admission_checks_the_program_and_keeps_hot_reads() {
        let map = site_map();
        let profile = profile(&map);
        assert_eq!(
            AdmittedProfile::admit(&profile, &ProgramKey::new([8; 32])).unwrap_err(),
            ProfileError::Mismatch(ProfileMismatch::Program)
        );
        let admitted = AdmittedProfile::admit(&profile, &ProgramKey::new([7; 32])).unwrap();
        // Two training requests: 16 and 400 executions qualify, 15 and 0 do not.
        assert_eq!(admitted.specialised_reads(), 2);
        let reads = map.read_sites().collect::<Vec<_>>();
        assert_eq!(
            admitted.read_specialisation(reads[0]),
            Some(ReadSpecialisation::StringPrototypePlace)
        );
        assert_eq!(admitted.read_specialisation(reads[1]), None);
        assert!(admitted.require_site_map(&map).is_ok());
        assert_eq!(
            admitted.require_site_map(&SiteMap::default()),
            Err(ProfileError::Mismatch(ProfileMismatch::SiteMap))
        );
    }

    #[test]
    fn write_sites_take_no_read_observation() {
        let map = site_map();
        let mut profile = profile(&map);
        let write = map
            .sites()
            .iter()
            .find(|site| site.kind() == SiteKind::PropertyWrite)
            .unwrap();
        assert_eq!(
            profile.observe_read(
                write,
                ReadSiteObservation::dominated(1, ReadSpecialisation::StringPrototypePlace),
            ),
            Err(ProfileError::NotAReadSite)
        );
        let read = map.read_sites().next().unwrap();
        assert_eq!(
            profile.observe_write(read, 1),
            Err(ProfileError::NotAWriteSite)
        );
    }

    #[test]
    fn collection_is_a_profile_use() {
        assert!(matches!(
            ProfileUse::of(ProfileMode::Collect {
                program: ProgramKey::new([1; 32]),
                compiler: "test",
            }),
            ProfileUse::Collect(_)
        ));
        assert!(matches!(
            ProfileUse::of(ProfileMode::None),
            ProfileUse::None
        ));
    }

    #[test]
    fn codec_round_trips_bodies_writes_and_undominated_reads() {
        let map = site_map();
        let mut profile = profile(&map);
        let reads = map.read_sites().collect::<Vec<_>>();
        profile
            .observe_read(
                reads[1],
                ReadSiteObservation {
                    executions: 9,
                    dominant: None,
                    records: 8,
                    megamorphic: true,
                },
            )
            .unwrap();
        let write = map
            .sites()
            .iter()
            .find(|site| site.kind() == SiteKind::PropertyWrite)
            .unwrap();
        profile.observe_write(write, 77).unwrap();
        profile.observe_body(&SiteBody::Main, 1);
        profile.observe_body(&SiteBody::Source("f1".into()), 300);
        let decoded = ExecutionProfile::decode(&profile.encode()).unwrap();
        assert_eq!(decoded, profile);
        assert_eq!(decoded.bodies().count(), 2);
        assert_eq!(decoded.writes().collect::<Vec<_>>(), [(write, &77)]);
        // An undominated read is never specialised, however hot.
        let admitted = AdmittedProfile::admit(&decoded, &ProgramKey::new([7; 32])).unwrap();
        assert_eq!(admitted.read_specialisation(reads[1]), None);
    }

    /// A decoded observation holds only what a collected one can.
    #[test]
    fn decoding_rejects_observations_collection_cannot_make() {
        let map = site_map();
        let read = map.read_sites().next().unwrap();
        let kind = ReadSpecialisation::OwnCompact(CompactStorageLayout::Inline);
        for (dominant, records, megamorphic) in [
            (Some(kind), 0, false),
            (Some(kind), 2, true),
            (None, 9, false),
        ] {
            let mut profile = profile(&map);
            profile
                .observe_read(
                    read,
                    ReadSiteObservation {
                        executions: 64,
                        dominant,
                        records,
                        megamorphic,
                    },
                )
                .unwrap();
            assert!(
                matches!(
                    ExecutionProfile::decode(&profile.encode()),
                    Err(ProfileError::Malformed(_))
                ),
                "{dominant:?} {records} {megamorphic}"
            );
        }
    }
}
