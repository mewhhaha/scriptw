//! Checked persistence of raw runtime Wasm, never native machine code.
//!
//! The storage owner supplies compiler identity and bounded atomic storage.
//! Function/global layout is reconstructed from validated Wasm. Only the pool
//! boundary is serialized, bound by the entry digest and checked against the
//! runtime's data segments and each newly collected program pool.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use scriptw_intl::{INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION, INTL_SERVICE_SELECTION_CUSTOM_SECTION};
use sha2::{Digest, Sha256};
use wasmparser::{
    DataKind, ExternalKind, Operator, Parser, Payload, TypeRef, ValidPayload, Validator,
};

use super::source_compilation::SourceCompilationProfileAdmission;
use super::{
    CurrentRuntimeLayoutVersion, RuntimeArtifact, RuntimeLayout, RuntimeSourceCompilationProfile,
    LAYOUT_VERSION, RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION,
};
use crate::data::PoolBoundary;
use crate::heap::STATIC_DATA_OFFSET;
use crate::module::HOST_IMPORT_MODULE;
use crate::EmitError;

const MAGIC: &[u8; 8] = b"SCRWRT01";
const DIGEST_BYTES: usize = 32;

/// A domain-separated key for one compiler build, runtime ABI and admitted Intl
/// selection. It cannot collide with the program/native cache key domains.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeArtifactCacheKey([u8; DIGEST_BYTES]);

impl RuntimeArtifactCacheKey {
    pub fn as_bytes(&self) -> &[u8; DIGEST_BYTES] {
        &self.0
    }
}

/// Optional storage for compiler-owned raw runtime artifacts.
///
/// The compiler identity must cover every source, data, configuration and
/// executable input capable of changing emitted Wasm. Storage must use the
/// caller's existing bounded cache policy. Cache failures are recoverable;
/// entries are fully checked before they can construct a runtime artifact.
pub trait RuntimeArtifactCache {
    fn compiler_identity(&self) -> &[u8; DIGEST_BYTES];
    fn load(&self, key: RuntimeArtifactCacheKey) -> Option<Vec<u8>>;
    fn store(&self, key: RuntimeArtifactCacheKey, entry: Vec<u8>) -> bool;
    fn remove(&self, key: RuntimeArtifactCacheKey);
}

#[derive(Clone)]
pub(super) struct RuntimeIdentity {
    sections: BTreeMap<&'static str, Arc<[u8]>>,
    digest: [u8; DIGEST_BYTES],
    source_compilation_profile: RuntimeSourceCompilationProfile,
}

impl RuntimeIdentity {
    pub(super) fn admit(
        selection: &scriptw_intl::IntlDataSelection,
        source_compilation_profile: RuntimeSourceCompilationProfile,
    ) -> Result<Self, EmitError> {
        Self::admit_with_jobs(
            selection,
            source_compilation_profile,
            std::num::NonZeroUsize::MIN,
        )
    }

    pub(super) fn admit_with_jobs(
        selection: &scriptw_intl::IntlDataSelection,
        source_compilation_profile: RuntimeSourceCompilationProfile,
        jobs: std::num::NonZeroUsize,
    ) -> Result<Self, EmitError> {
        let selected = selection.selected_with_jobs(jobs).map_err(|error| {
            EmitError::unsupported(format!("failed to select the Intl artifact data: {error}"))
        })?;
        if matches!(
            selected.identity().profile().profile(),
            scriptw_intl::IntlDataProfile::Minimal
        ) {
            // SelectedIntlDataBundle privately constructs this single immutable
            // process-wide bundle. Hash its complete physical frames once;
            // subsequent admissions retain those exact shared bytes and digest.
            static PORTABLE: OnceLock<RuntimeIdentity> = OnceLock::new();
            static NATIVE_HOST: OnceLock<RuntimeIdentity> = OnceLock::new();
            let minimal = match source_compilation_profile {
                RuntimeSourceCompilationProfile::Portable => &PORTABLE,
                RuntimeSourceCompilationProfile::NativeHost => &NATIVE_HOST,
            };
            return Ok(minimal
                .get_or_init(|| Self::from_selected(selected, source_compilation_profile))
                .clone());
        }
        // Custom profiles can reuse a name with different physical data. Their
        // complete section contents still determine every admission identity.
        Ok(Self::from_selected(selected, source_compilation_profile))
    }

    fn from_selected(
        selected: &scriptw_intl::SelectedIntlDataBundle,
        source_compilation_profile: RuntimeSourceCompilationProfile,
    ) -> Self {
        let mut sections: BTreeMap<_, _> = selected.component_sections().into_iter().collect();
        sections.insert(
            INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION,
            Arc::from(selected.identity().artifact_identity().as_bytes()),
        );
        if let Some(services) = selected.service_selection() {
            sections.insert(
                INTL_SERVICE_SELECTION_CUSTOM_SECTION,
                Arc::from(services.wire().to_le_bytes()),
            );
        }
        sections.insert(
            RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION,
            Arc::from([source_compilation_profile.wire()]),
        );
        Self::from_sections(sections, source_compilation_profile)
    }

    fn from_sections(
        sections: BTreeMap<&'static str, Arc<[u8]>>,
        source_compilation_profile: RuntimeSourceCompilationProfile,
    ) -> Self {
        let mut hash = Sha256::new();
        hash.update(b"scriptw-runtime-intl-selection-v1");
        // Include the admitted physical images, with unambiguous boundaries.
        // A custom profile reusing a textual name cannot reuse old data.
        for (name, bytes) in &sections {
            hash_field(&mut hash, name.as_bytes());
            hash_field(&mut hash, bytes);
        }
        Self {
            sections,
            digest: hash.finalize().into(),
            source_compilation_profile,
        }
    }

    pub(super) fn digest(&self) -> [u8; DIGEST_BYTES] {
        self.digest
    }

    fn cache_key(&self, compiler: &[u8; DIGEST_BYTES]) -> RuntimeArtifactCacheKey {
        let mut hash = Sha256::new();
        hash.update(b"scriptw-raw-runtime-cache-v1");
        hash.update(LAYOUT_VERSION.to_le_bytes());
        hash.update(compiler);
        hash_field(&mut hash, std::env::consts::ARCH.as_bytes());
        hash.update(self.digest);
        RuntimeArtifactCacheKey(hash.finalize().into())
    }
}

fn hash_field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

pub(super) fn load_or_emit(
    identity: &RuntimeIdentity,
    cache: Option<&dyn RuntimeArtifactCache>,
    emit: impl FnOnce() -> Result<Arc<RuntimeArtifact>, EmitError>,
) -> Result<Arc<RuntimeArtifact>, EmitError> {
    load_or_emit_with_jobs(identity, cache, std::num::NonZeroUsize::MIN, emit)
}

pub(super) fn load_or_emit_with_jobs(
    identity: &RuntimeIdentity,
    cache: Option<&dyn RuntimeArtifactCache>,
    jobs: std::num::NonZeroUsize,
    emit: impl FnOnce() -> Result<Arc<RuntimeArtifact>, EmitError>,
) -> Result<Arc<RuntimeArtifact>, EmitError> {
    let Some(cache) = cache else {
        return emit();
    };
    let started = std::time::Instant::now();
    let trace = std::env::var_os("SCRIPTW_WASM_TRACE").is_some();
    let key = identity.cache_key(cache.compiler_identity());
    if let Some(entry) = cache.load(key) {
        let decoded = if jobs == std::num::NonZeroUsize::MIN {
            decode(&entry, key, identity)
        } else {
            decode_with_jobs(&entry, key, identity, jobs)
        };
        if let Some(runtime) = decoded {
            if trace {
                eprintln!(
                    "scriptw wasm trace: runtime-cache hit: {} bytes in {:?}",
                    runtime.bytes.len(),
                    started.elapsed()
                );
            }
            return Ok(Arc::new(runtime));
        }
        cache.remove(key);
        if trace {
            eprintln!("scriptw wasm trace: runtime-cache rejected invalid entry");
        }
    }
    if trace {
        eprintln!(
            "scriptw wasm trace: runtime-cache miss: {:?}",
            started.elapsed()
        );
    }
    let runtime = emit()?;
    if !cache.store(key, encode(&runtime, key)) && trace {
        eprintln!("scriptw wasm trace: runtime-cache write failed");
    }
    Ok(runtime)
}

fn encode(runtime: &RuntimeArtifact, key: RuntimeArtifactCacheKey) -> Vec<u8> {
    let mut entry = Vec::with_capacity(runtime.bytes.len() + 104);
    entry.extend_from_slice(MAGIC);
    entry.extend_from_slice(&LAYOUT_VERSION.to_le_bytes());
    entry.extend_from_slice(key.as_bytes());
    encode_payload(runtime, &mut entry);
    let digest = Sha256::digest(&entry);
    entry.extend_from_slice(&digest);
    entry
}

/// Both disk and build envelopes carry this exact checked raw payload.
pub(super) fn encode_payload(runtime: &RuntimeArtifact, entry: &mut Vec<u8>) {
    entry.extend_from_slice(&(runtime.layout.pool.static_len() as u64).to_le_bytes());
    entry.extend_from_slice(&(runtime.layout.pool.code_unit_len() as u64).to_le_bytes());
    entry.extend_from_slice(&runtime.layout.pool.strings().to_le_bytes());
    entry.extend_from_slice(&(runtime.bytes.len() as u64).to_le_bytes());
    entry.extend_from_slice(&runtime.bytes);
}

pub(super) fn take<const N: usize>(bytes: &mut &[u8]) -> Option<[u8; N]> {
    let (value, rest) = bytes.split_at_checked(N)?;
    *bytes = rest;
    value.try_into().ok()
}

fn decode(
    entry: &[u8],
    key: RuntimeArtifactCacheKey,
    identity: &RuntimeIdentity,
) -> Option<RuntimeArtifact> {
    decode_with_jobs(entry, key, identity, std::num::NonZeroUsize::MIN)
}

fn decode_with_jobs(
    entry: &[u8],
    key: RuntimeArtifactCacheKey,
    identity: &RuntimeIdentity,
    jobs: std::num::NonZeroUsize,
) -> Option<RuntimeArtifact> {
    let digest_offset = entry.len().checked_sub(DIGEST_BYTES)?;
    let (mut payload, digest) = entry.split_at(digest_offset);
    let expected_digest: [u8; DIGEST_BYTES] = Sha256::digest(payload).into();
    if expected_digest.as_slice() != digest || &take::<8>(&mut payload)? != MAGIC {
        return None;
    }
    let version = CurrentRuntimeLayoutVersion::admit(u32::from_le_bytes(take(&mut payload)?))?;
    if &take::<DIGEST_BYTES>(&mut payload)? != key.as_bytes() {
        return None;
    }
    decode_payload(
        payload,
        identity,
        version,
        jobs,
        super::validation::FunctionBodyValidation::AtUse,
    )
}

pub(super) fn decode_payload(
    payload: &[u8],
    identity: &RuntimeIdentity,
    version: CurrentRuntimeLayoutVersion,
    jobs: std::num::NonZeroUsize,
    body_validation: super::validation::FunctionBodyValidation,
) -> Option<RuntimeArtifact> {
    Some(admit_payload(payload, identity, version, jobs, body_validation)?.into_owned())
}

/// Completed raw admission retains the payload lifetime until storage is
/// selected. A non-static package cannot become a borrowed runtime artifact.
pub(super) struct RuntimePayload<'a> {
    bytes: &'a [u8],
    layout: RuntimeLayout,
    version: CurrentRuntimeLayoutVersion,
    profile: RuntimeSourceCompilationProfile,
}

impl RuntimePayload<'_> {
    pub(super) fn into_owned(self) -> RuntimeArtifact {
        RuntimeArtifact::new(
            Cow::Owned(self.bytes.to_vec()),
            self.layout,
            self.version,
            self.profile,
        )
    }
}

impl RuntimePayload<'static> {
    pub(super) fn into_immutable_build(self) -> RuntimeArtifact {
        RuntimeArtifact::new(
            Cow::Borrowed(self.bytes),
            self.layout,
            self.version,
            self.profile,
        )
    }
}

pub(super) fn admit_payload<'a>(
    mut payload: &'a [u8],
    identity: &RuntimeIdentity,
    version: CurrentRuntimeLayoutVersion,
    jobs: std::num::NonZeroUsize,
    body_validation: super::validation::FunctionBodyValidation,
) -> Option<RuntimePayload<'a>> {
    let static_len = usize::try_from(u64::from_le_bytes(take(&mut payload)?)).ok()?;
    let code_unit_len = usize::try_from(u64::from_le_bytes(take(&mut payload)?)).ok()?;
    let strings = u32::from_le_bytes(take(&mut payload)?);
    let wasm_len = usize::try_from(u64::from_le_bytes(take(&mut payload)?)).ok()?;
    if payload.len() != wasm_len {
        return None;
    }
    let pool = PoolBoundary::from_runtime_cache(static_len, code_unit_len, strings)?;
    let layout = decode_layout(payload, pool, identity, jobs, body_validation)?;
    Some(RuntimePayload {
        bytes: payload,
        layout,
        version,
        profile: identity.source_compilation_profile,
    })
}

fn decode_layout(
    bytes: &[u8],
    pool: PoolBoundary,
    identity: &RuntimeIdentity,
    jobs: std::num::NonZeroUsize,
    body_validation: super::validation::FunctionBodyValidation,
) -> Option<RuntimeLayout> {
    let features = super::validation::required_features();
    let mut validator = Validator::new_with_features(features);
    let mut parser = Parser::new(0);
    parser.set_features(features);
    let mut functions = Vec::new();
    let mut validated_end = false;

    let mut imported_functions = 0u32;
    let mut function_types = Vec::new();
    let mut globals = Vec::new();
    let mut exports = BTreeMap::new();
    let mut sections = identity.sections.clone();
    let mut data_count = 0;
    let mut source_admission = SourceCompilationProfileAdmission::default();
    for payload in parser.parse_all(bytes) {
        let payload = payload.ok()?;
        match validator.payload(&payload).ok()? {
            ValidPayload::Func(function, body) => {
                if matches!(
                    body_validation,
                    super::validation::FunctionBodyValidation::AtUse
                ) {
                    functions.push((function, body));
                }
            }
            ValidPayload::End(_) => validated_end = true,
            ValidPayload::Ok => {}
            // A runtime artifact is a core module, never a component graph.
            ValidPayload::Parser(_) => return None,
        }
        match payload {
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.ok()?;
                    source_admission.import(&import).ok()?;
                    if import.module != HOST_IMPORT_MODULE {
                        return None;
                    }
                    match import.ty {
                        TypeRef::Func(_) | TypeRef::FuncExact(_) => {
                            imported_functions = imported_functions.checked_add(1)?;
                        }
                        TypeRef::Memory(_) => {}
                        // R defines every global; no foreign runtime layout can
                        // enter through globals, tables or exception tags.
                        TypeRef::Global(_) | TypeRef::Table(_) | TypeRef::Tag(_) => return None,
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                for ty in reader {
                    function_types.push(ty.ok()?);
                }
            }
            Payload::GlobalSection(reader) => {
                for global in reader {
                    globals.push(wasm_encoder::GlobalType::try_from(global.ok()?.ty).ok()?);
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.ok()?;
                    if exports
                        .insert(export.name, (export.kind, export.index))
                        .is_some()
                    {
                        return None;
                    }
                }
            }
            Payload::DataSection(reader) => {
                for data in reader {
                    let data = data.ok()?;
                    match (data_count, data.kind) {
                        (0, DataKind::Passive) if data.data.len() == pool.code_unit_len() => {}
                        (
                            1,
                            DataKind::Active {
                                memory_index: 0,
                                offset_expr,
                            },
                        ) if data.data.len() == pool.static_len() => {
                            let mut ops = offset_expr.get_operators_reader();
                            if !matches!(ops.read().ok()?, Operator::I32Const { value }
                                if value == STATIC_DATA_OFFSET as i32)
                                || !matches!(ops.read().ok()?, Operator::End)
                                || !ops.eof()
                            {
                                return None;
                            }
                        }
                        _ => return None,
                    }
                    data_count += 1;
                }
            }
            Payload::CustomSection(section)
                if section.name().starts_with("scriptw.intl")
                    || section.name() == RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION =>
            {
                if section.name() == RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION {
                    source_admission.section(section.data()).ok()?;
                }
                if sections.remove(section.name())?.as_ref() != section.data() {
                    return None;
                }
            }
            Payload::StartSection { .. } => return None,
            _ => {}
        }
    }
    if source_admission.finish().ok()? != identity.source_compilation_profile {
        return None;
    }
    if !validated_end
        || data_count != 2
        || !sections.is_empty()
        || function_types.is_empty()
        || globals.is_empty()
    {
        return None;
    }
    for position in 0..u32::try_from(function_types.len()).ok()? {
        let index = imported_functions.checked_add(position)?;
        if exports.get(super::function_export_name(index).as_str())
            != Some(&(ExternalKind::Func, index))
        {
            return None;
        }
    }
    for index in 0..u32::try_from(globals.len()).ok()? {
        if exports.get(super::global_export_name(index).as_str())
            != Some(&(ExternalKind::Global, index))
        {
            return None;
        }
    }
    match body_validation {
        super::validation::FunctionBodyValidation::AtUse => {
            super::validation::validate_all(functions, jobs)?;
        }
        super::validation::FunctionBodyValidation::ValidatedAtBuild => {
            // The unsafe package owner guarantees these exact immutable bodies
            // passed the producer's full validation. All section, metadata,
            // identity, data and layout checks above still run at use.
            debug_assert!(functions.is_empty());
            if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
                eprintln!("scriptw wasm trace: runtime-function bodies: reused immutable Cargo producer validation");
            }
        }
    }
    Some(RuntimeLayout::new(
        imported_functions,
        function_types,
        globals,
        pool,
    ))
}

#[cfg(test)]
mod tests;
