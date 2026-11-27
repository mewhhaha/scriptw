//! The host side of module loading: `HostResolveImportedModule` and
//! `HostLoadImportedModule`.
//!
//! All loading happens at *compile* time, so the trait is synchronous. The
//! compiler reads the whole transitive closure of an entry module up front and
//! hands it to `scriptw-ir`, which performs no IO of its own.
//!
//! Nothing here bakes in a test262 path. The runner supplies the root, exactly
//! as any other embedder would.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use scriptw_front::{ParseGoal, ParsedModule, ParsedScript, ParsedSource};
use scriptw_ir::{ModuleGraphSources, ModuleSourceIr};
pub use scriptw_ir::{ModuleKey, ModuleKindIr, ModuleRecordKey, ModuleRequestKeyIr};
use scriptw_runtime::{
    EmbeddedModuleGoal, EmbeddedModuleGraph, EmbeddedModuleKind, EmbeddedModuleReferrer,
};

#[path = "module_paths.rs"]
mod module_paths;
use module_paths::normalize;

#[cfg(test)]
#[path = "module_loader_foreign_tests.rs"]
mod foreign_tests;

/// Root source belongs to the declared entry. Agent source has no graph locator
/// and can use only explicit Unlocated edges from that same immutable graph.
#[derive(Clone, Copy)]
pub(crate) enum EmbeddedSourceRole {
    Entry,
    UnlocatedScript,
}

impl EmbeddedSourceRole {
    fn parse_filename(
        self,
        graph: &EmbeddedModuleGraph,
        source: &str,
        goal: ParseGoal,
        filename: Option<&str>,
    ) -> Result<Option<String>, ModuleLoadError> {
        let valid = match self {
            Self::Entry => {
                let declared_goal = match graph.entry().goal() {
                    EmbeddedModuleGoal::Script => ParseGoal::Script,
                    EmbeddedModuleGoal::Module => ParseGoal::Module,
                };
                goal == declared_goal
                    && source == graph.entry().source()
                    && filename.is_none_or(|name| name == graph.entry().identity())
            }
            Self::UnlocatedScript => goal == ParseGoal::Script,
        };
        if !valid {
            return Err(ModuleLoadError::Denied {
                specifier: filename.unwrap_or("<entry>").to_owned(),
                reason: "source, goal or identity differs from embedded entry authority".to_owned(),
            });
        }
        Ok(match self {
            Self::Entry => Some(graph.entry().identity().to_owned()),
            Self::UnlocatedScript => None,
        })
    }
}

/// Admission and the one parse attempt mint the only entry the catalog
/// builder accepts; source/goal/identity cannot be paired again at load time.
pub(crate) struct ParsedEmbeddedEntry<'a> {
    graph: &'a EmbeddedModuleGraph,
    source: ParsedSource,
    role: EmbeddedSourceRole,
}

#[derive(Debug)]
pub(crate) enum EmbeddedEntryError {
    Authority(ModuleLoadError),
    Parse(scriptw_front::ParseError),
}

impl EmbeddedSourceRole {
    pub(crate) fn parse_entry<'a>(
        self,
        graph: &'a EmbeddedModuleGraph,
        source: Cow<'_, str>,
        goal: ParseGoal,
        filename: Option<&str>,
    ) -> Result<ParsedEmbeddedEntry<'a>, EmbeddedEntryError> {
        let filename = self
            .parse_filename(graph, source.as_ref(), goal, filename)
            .map_err(EmbeddedEntryError::Authority)?;
        let source = scriptw_front::parse(source, scriptw_front::ParseOptions { goal, filename })
            .map_err(EmbeddedEntryError::Parse)?;
        Ok(ParsedEmbeddedEntry {
            graph,
            source,
            role: self,
        })
    }
}

impl ParsedEmbeddedEntry<'_> {
    /// Consume all declared rows, including computed-only targets. Reuse this
    /// exact entry parse and parse each canonical dependency only once.
    pub(crate) fn into_graph(self) -> Result<(ParsedSource, ModuleGraphSources), ModuleLoadError> {
        let Self {
            graph,
            source,
            role,
        } = self;
        let sources = load_embedded_module_graph(graph, &source, role)?;
        Ok((source, sources))
    }
}

fn load_embedded_module_graph(
    graph: &EmbeddedModuleGraph,
    entry: &ParsedSource,
    role: EmbeddedSourceRole,
) -> Result<ModuleGraphSources, ModuleLoadError> {
    let key = ModuleKey::from_host(match role {
        EmbeddedSourceRole::Entry => graph.entry().identity(),
        EmbeddedSourceRole::UnlocatedScript => "<unlocated-script>",
    });
    let entry_module = match entry {
        ParsedSource::Script(source) => ModuleSourceIr::from_parsed_script(
            key,
            graph.entry().meta_url().to_owned(),
            source.clone(),
        ),
        ParsedSource::Module(source) => {
            ModuleSourceIr::from_parsed(key, graph.entry().meta_url().to_owned(), source.clone())
        }
    };
    let entry_referrer = match role {
        EmbeddedSourceRole::Entry => match graph.entry().goal() {
            EmbeddedModuleGoal::Script => {
                EmbeddedModuleReferrer::Script(graph.entry().identity().to_owned())
            }
            EmbeddedModuleGoal::Module => {
                EmbeddedModuleReferrer::Module(graph.entry().identity().to_owned())
            }
        },
        EmbeddedSourceRole::UnlocatedScript => EmbeddedModuleReferrer::Unlocated,
    };
    let entry_kind = entry_module.kind();
    let mut indices = BTreeMap::from([((entry_referrer, entry_kind), 0u32)]);
    let mut modules = vec![entry_module];
    for module in graph.modules() {
        let kind = match module.kind() {
            EmbeddedModuleKind::SourceText => ModuleKindIr::SourceText,
            EmbeddedModuleKind::Json => ModuleKindIr::Json,
            EmbeddedModuleKind::Bytes => ModuleKindIr::Bytes,
            EmbeddedModuleKind::Text => ModuleKindIr::Text,
            EmbeddedModuleKind::ModuleSource => ModuleKindIr::ModuleSource,
        };
        let referrer = (
            EmbeddedModuleReferrer::Module(module.identity().to_owned()),
            kind,
        );
        if indices.contains_key(&referrer) {
            continue;
        }
        let index = u32::try_from(modules.len()).map_err(|_| ModuleLoadError::Denied {
            specifier: module.identity().to_owned(),
            reason: "embedded catalog exceeds module source index space".to_owned(),
        })?;
        indices.insert(referrer, index);
        let key = ModuleKey::from_host(module.identity());
        let url = module.meta_url().to_owned();
        modules.push(match module.contents() {
            scriptw_runtime::EmbeddedModuleContents::SourceText(text) => {
                ModuleSourceIr::new(key, text.clone(), url)
            }
            scriptw_runtime::EmbeddedModuleContents::Json(text) => {
                ModuleSourceIr::json(key, text.clone(), url)
            }
            scriptw_runtime::EmbeddedModuleContents::Text(text) => {
                ModuleSourceIr::text(key, text.clone(), url)
            }
            scriptw_runtime::EmbeddedModuleContents::ModuleSource { source_tag } => {
                ModuleSourceIr::module_source(key, source_tag.clone(), url)
            }
            scriptw_runtime::EmbeddedModuleContents::Bytes(bytes) => {
                ModuleSourceIr::bytes(key, bytes.clone(), url)
            }
        });
    }
    let mut resolutions = Vec::new();
    let mut realm_requests = BTreeMap::new();
    for resolution in graph.resolutions() {
        let request = ModuleRequestKeyIr::try_new(
            resolution.request().specifier(),
            resolution
                .request()
                .attributes()
                .iter()
                .map(|(key, value)| scriptw_ir::ImportAttributeIr {
                    key: key.clone(),
                    value: value.clone(),
                }),
        )
        .expect("embedded request owner already validated unique attributes");
        let target = indices[&(
            EmbeddedModuleReferrer::Module(resolution.target().to_owned()),
            match resolution.target_kind() {
                EmbeddedModuleKind::SourceText => ModuleKindIr::SourceText,
                EmbeddedModuleKind::Json => ModuleKindIr::Json,
                EmbeddedModuleKind::Bytes => ModuleKindIr::Bytes,
                EmbeddedModuleKind::Text => ModuleKindIr::Text,
                EmbeddedModuleKind::ModuleSource => ModuleKindIr::ModuleSource,
            },
        )];
        if resolution.referrer() == &EmbeddedModuleReferrer::Realm {
            realm_requests.insert(request, scriptw_ir::RealmModuleResolutionIr::Loaded(target));
        } else if let Some(&referrer) = indices.get(&(
            resolution.referrer().clone(),
            match resolution.referrer() {
                EmbeddedModuleReferrer::Script(_) => ModuleKindIr::Script,
                EmbeddedModuleReferrer::Module(_) => ModuleKindIr::SourceText,
                EmbeddedModuleReferrer::Realm | EmbeddedModuleReferrer::Unlocated => entry_kind,
            },
        )) {
            resolutions.push((referrer, request, target));
        }
    }
    Ok(ModuleGraphSources {
        realm_requests,
        modules,
        entry: 0,
        resolutions,
    })
}

/// What a successful load produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadedModuleKind {
    /// Explicit synchronous host capabilities, never discovered from files.
    Foreign {
        module: scriptw_ir::foreign::ForeignModuleDeclaration,
        error_tag: Option<scriptw_ir::foreign::ForeignErrorTag>,
    },
    /// A genuine opaque host module source, with no ECMAScript source body.
    ModuleSource { source_tag: String },
    /// A Source Text Module Record's source text.
    Source(String),
    /// JSON grammar and a genuine default-only synthetic module record.
    Json(String),
    /// Raw Bytes Module data, without UTF-8 decoding.
    Bytes(Vec<u8>),
    /// A host-decoded String, exported without parsing a source grammar.
    Text(String),
}

/// One loaded module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedModule {
    /// Key the module was loaded under.
    pub key: ModuleKey,
    /// The module's contents.
    pub kind: LoadedModuleKind,
    /// Value `import.meta.url` reports.
    pub meta_url: String,
}

/// Why a resolve or load failed.
///
/// Every variant becomes a `SyntaxError` at compile time, which is what
/// test262's `phase: resolution` negatives expect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleLoadError {
    /// No module answers this specifier.
    NotFound {
        /// The specifier that could not be resolved.
        specifier: String,
        /// Key of the module that requested it, if any.
        referrer: Option<ModuleKey>,
    },
    /// Resolution escaped the configured root.
    Denied {
        /// The rejected specifier.
        specifier: String,
        /// Why it was rejected.
        reason: String,
    },
    /// An import attribute the host does not implement.
    UnsupportedAttribute {
        /// Attribute key.
        key: String,
        /// Attribute value.
        value: String,
    },
    /// Reading the module failed.
    Io {
        /// Key being read.
        key: ModuleKey,
        /// Underlying error text.
        message: String,
    },
}

impl core::fmt::Display for ModuleLoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotFound { specifier, .. } => {
                write!(f, "unresolved module request: {specifier}")
            }
            Self::Denied { specifier, reason } => {
                write!(f, "module request denied: {specifier} ({reason})")
            }
            Self::UnsupportedAttribute { key, value } => {
                write!(f, "unsupported import attribute: {key}={value}")
            }
            Self::Io { key, message } => {
                write!(f, "failed to read module {}: {message}", key.as_str())
            }
        }
    }
}

/// The host hooks module loading needs.
pub trait HostModuleLoader: Send + Sync {
    /// `HostResolveImportedModule`: phase-free request key plus referrer to a
    /// stable module key.
    ///
    /// # Errors
    /// Returns an error when the specifier names nothing the host will serve.
    fn resolve(
        &self,
        referrer: Option<&ModuleKey>,
        request: &ModuleRequestKeyIr,
    ) -> Result<ModuleKey, ModuleLoadError>;

    /// `HostLoadImportedModule`: host locator and requested record kind to contents.
    ///
    /// # Errors
    /// Returns an error when the module cannot be read.
    fn load(&self, key: &ModuleRecordKey) -> Result<LoadedModule, ModuleLoadError>;

    /// Checks explicit declaration authority when a graph reuses a record or
    /// accepts a newly loaded one. This performs no loading or ambient lookup.
    fn validate_resolved_module(
        &self,
        _request: &ModuleRequestKeyIr,
        _source: &ModuleSourceIr,
    ) -> Result<(), ModuleLoadError> {
        Ok(())
    }

    /// Normalizes an entry locator the embedder supplied directly, without a
    /// referrer, and mints its stable [`ModuleKey`].
    ///
    /// The entry module arrives as a locator rather than a specifier, so it never
    /// passes through [`resolve`](Self::resolve) — and if it is not normalized
    /// the same way, a module that imports the entry back lands under a second
    /// key and the entry is loaded, and evaluated, twice. Unnormalizable keys
    /// are returned unchanged: an in-memory entry has no path to normalize.
    fn canonical_key(&self, key: &str) -> ModuleKey {
        ModuleKey::from_host(key)
    }
}

/// Resolves explicit foreign capabilities before consulting the fallback host
/// loader, including a RejectAll loader. No native names are discovered from
/// a filesystem or an ambient Linker.
pub struct ForeignModuleLoader {
    registry: scriptw_ir::foreign::ForeignModuleRegistry,
    fallback: Box<dyn HostModuleLoader>,
}
impl ForeignModuleLoader {
    pub fn new(
        registry: scriptw_ir::foreign::ForeignModuleRegistry,
        fallback: Box<dyn HostModuleLoader>,
    ) -> Self {
        Self { registry, fallback }
    }
    fn foreign_key(specifier: &str) -> ModuleKey {
        ModuleKey::from_host(format!("scriptw:foreign/{specifier}"))
    }
}
impl HostModuleLoader for ForeignModuleLoader {
    fn resolve(
        &self,
        referrer: Option<&ModuleKey>,
        request: &ModuleRequestKeyIr,
    ) -> Result<ModuleKey, ModuleLoadError> {
        if self.registry.module(request.specifier()).is_some() {
            if let Some(attribute) = request.attributes().first() {
                return Err(ModuleLoadError::UnsupportedAttribute {
                    key: attribute.key.clone(),
                    value: attribute.value.clone(),
                });
            }
            return Ok(Self::foreign_key(request.specifier()));
        }
        self.fallback.resolve(referrer, request)
    }
    fn load(&self, record: &ModuleRecordKey) -> Result<LoadedModule, ModuleLoadError> {
        if let Some(module) = self
            .registry
            .modules()
            .iter()
            .find(|module| Self::foreign_key(module.specifier()) == *record.locator())
        {
            if record.kind() != ModuleKindIr::SourceText {
                return Err(ModuleLoadError::Denied {
                    specifier: module.specifier().to_owned(),
                    reason:
                        "foreign modules accept evaluation/defer imports without attributes only"
                            .to_owned(),
                });
            }
            return Ok(LoadedModule {
                key: record.locator().clone(),
                meta_url: format!("scriptw:foreign/{}", module.specifier()),
                kind: LoadedModuleKind::Foreign {
                    module: module.clone(),
                    error_tag: self.registry.error_tag().copied(),
                },
            });
        }
        self.fallback.load(record)
    }
    fn canonical_key(&self, key: &str) -> ModuleKey {
        self.fallback.canonical_key(key)
    }
    fn validate_resolved_module(
        &self,
        request: &ModuleRequestKeyIr,
        source: &ModuleSourceIr,
    ) -> Result<(), ModuleLoadError> {
        if self.registry.module(request.specifier()).is_some() {
            validate_foreign_module_source(&self.registry, request, source)
        } else {
            self.fallback.validate_resolved_module(request, source)
        }
    }
}

/// Adds declared native targets to a complete embedded catalog without granting
/// filesystem authority. Conflicting explicit host resolutions are rejected.
pub(crate) fn include_foreign_module_registry(
    graph: &mut ModuleGraphSources,
    registry: &scriptw_ir::foreign::ForeignModuleRegistry,
) -> Result<(), ModuleLoadError> {
    include_foreign_module_registry_requests(graph, registry, &[])
}

/// The same closed catalog owner also admits prepared Realm requests. Keep
/// their loading and the final capability/conflict check in one operation.
pub(crate) fn include_foreign_module_registry_requests(
    graph: &mut ModuleGraphSources,
    registry: &scriptw_ir::foreign::ForeignModuleRegistry,
    requests_to_add: &[ModuleRequestKeyIr],
) -> Result<(), ModuleLoadError> {
    if registry.is_empty() {
        return Ok(());
    }
    validate_foreign_module_resolutions(graph, registry)?;
    // A complete catalog owns source records reached through computed import
    // resolution rows too. Literal-only discovery from entry cannot open their
    // declared native dependencies. Inspect every already-owned record while
    // keeping the fallback loader incapable of any ambient IO.
    let roots = (0..graph.modules.len()).collect();
    let loader = ForeignModuleLoader::new(registry.clone(), Box::new(RejectAllModuleLoader));
    extend_module_graph_requests_from_roots(graph, requests_to_add, &loader, roots, None)?;
    // Canonical host keys can collide even before a request has a resolution.
    // An existing source record must not silently replace the registered
    // capability when the loading driver reuses that key.
    validate_foreign_module_resolutions(graph, registry)
}

fn validate_foreign_module_resolutions(
    graph: &ModuleGraphSources,
    registry: &scriptw_ir::foreign::ForeignModuleRegistry,
) -> Result<(), ModuleLoadError> {
    for (_, request, target) in &graph.resolutions {
        validate_foreign_module_source(registry, request, &graph.modules[*target as usize])?;
    }
    for (request, resolution) in &graph.realm_requests {
        if let scriptw_ir::RealmModuleResolutionIr::Loaded(target) = resolution {
            validate_foreign_module_source(registry, request, &graph.modules[*target as usize])?;
        }
    }
    Ok(())
}

fn validate_foreign_module_source(
    registry: &scriptw_ir::foreign::ForeignModuleRegistry,
    request: &ModuleRequestKeyIr,
    source: &ModuleSourceIr,
) -> Result<(), ModuleLoadError> {
    if let Some(module) = registry.module(request.specifier()) {
        if source.foreign_modules().is_none_or(|foreign| {
            foreign.module(module.specifier()) != Some(module)
                || foreign.error_tag() != registry.error_tag()
        }) {
            return Err(ModuleLoadError::Denied {
                specifier: module.specifier().to_owned(),
                reason: "declared foreign module conflicts with the resolved module record"
                    .to_owned(),
            });
        }
    }
    Ok(())
}

/// A host loader that proves an execution cannot consult module state.
///
/// Differential protocols without an embedded graph use this rather than a
/// filesystem loader. Both operations fail before path normalization or IO.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RejectAllModuleLoader;

impl HostModuleLoader for RejectAllModuleLoader {
    fn resolve(
        &self,
        referrer: Option<&ModuleKey>,
        request: &ModuleRequestKeyIr,
    ) -> Result<ModuleKey, ModuleLoadError> {
        Err(ModuleLoadError::Denied {
            specifier: request.specifier().to_string(),
            reason: match referrer {
                Some(referrer) => format!(
                    "module loading disabled by host policy for referrer {}",
                    referrer.as_str()
                ),
                None => "module loading disabled by host policy".to_string(),
            },
        })
    }

    fn load(&self, key: &ModuleRecordKey) -> Result<LoadedModule, ModuleLoadError> {
        Err(ModuleLoadError::Denied {
            specifier: key.locator().as_str().to_string(),
            reason: "module loading disabled by host policy".to_string(),
        })
    }
}

/// The source authority for a module graph entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleEntry {
    /// Load the entry through [`HostModuleLoader::load`].
    HostLoad {
        /// Unnormalized locator the host turns into the entry's [`ModuleKey`].
        /// Relative specifiers inside the entry resolve against its directory.
        locator: String,
    },
    /// Use source text already supplied by the embedder.
    ///
    /// Load-bearing for test262: the harness prepends `assert.js` and `sta.js`
    /// to the entry module's text, but relative specifiers inside it must
    /// still resolve against the real on-disk directory that `locator` names.
    InMemory {
        /// Unnormalized locator the host turns into the entry's [`ModuleKey`].
        locator: String,
        /// Exact entry source to parse.
        source_text: String,
    },
}

impl ModuleEntry {
    fn locator(&self) -> &str {
        match self {
            Self::HostLoad { locator } | Self::InMemory { locator, .. } => locator,
        }
    }
}

/// A filesystem loader confined to a root directory.
#[derive(Debug, Clone)]
pub struct FilesystemModuleLoader {
    root: PathBuf,
    entry_dir: PathBuf,
}

impl FilesystemModuleLoader {
    /// Builds a loader rooted at `root`, or at `entry`'s parent directory when
    /// `root` is `None`.
    ///
    /// # Errors
    /// Returns an error when neither a root nor a usable working directory is
    /// available.
    pub fn new(root: Option<&str>, entry: Option<&str>) -> Result<Self, ModuleLoadError> {
        Ok(Self::from_paths(root, entry))
    }

    /// Ambient compilation owns the raw options. Resolve the directories once
    /// when graph discovery first needs to resolve or read a filesystem module.
    pub(super) fn deferred(root: Option<String>, entry: Option<String>) -> impl HostModuleLoader {
        LazyLock::new(move || Self::from_paths(root.as_deref(), entry.as_deref()))
    }

    fn from_paths(root: Option<&str>, entry: Option<&str>) -> Self {
        let entry_dir = entry
            .map(Path::new)
            .and_then(Path::parent)
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let root = root.map(PathBuf::from).unwrap_or_else(|| entry_dir.clone());
        let root = root.canonicalize().unwrap_or(root);
        let entry_dir = entry_dir.canonicalize().unwrap_or(entry_dir);
        Self { root, entry_dir }
    }

    fn canonical_entry_key(key: &str) -> ModuleKey {
        // Exactly what `resolve` produces, so an entry the graph reaches again
        // through a relative specifier lands on the key it already holds.
        // A key naming nothing on disk (an in-memory entry) stays as written.
        normalize(Path::new(key)).canonicalize().map_or_else(
            |_| ModuleKey::from_host(key),
            |resolved| ModuleKey::from_host(resolved.to_string_lossy().into_owned()),
        )
    }

    /// Rejects any path that escapes the root, before it is ever read.
    fn confine(&self, specifier: &str, candidate: &Path) -> Result<PathBuf, ModuleLoadError> {
        let normalized = normalize(candidate);
        let resolved = normalized.canonicalize().unwrap_or(normalized);
        if resolved.starts_with(&self.root) {
            return Ok(resolved);
        }
        Err(ModuleLoadError::Denied {
            specifier: specifier.to_string(),
            reason: format!("resolves outside module root {}", self.root.display()),
        })
    }
}

impl HostModuleLoader for FilesystemModuleLoader {
    fn resolve(
        &self,
        referrer: Option<&ModuleKey>,
        request: &ModuleRequestKeyIr,
    ) -> Result<ModuleKey, ModuleLoadError> {
        let json = matches!(request.attributes(), [attribute]
            if attribute.key == "type" && attribute.value == "json");
        let bytes = matches!(request.attributes(), [attribute]
            if attribute.key == "type" && attribute.value == "bytes");
        let text = matches!(request.attributes(), [attribute]
            if attribute.key == "type" && attribute.value == "text");
        if !request.attributes().is_empty() && !json && !bytes && !text {
            let attribute = &request.attributes()[0];
            return Err(ModuleLoadError::UnsupportedAttribute {
                key: attribute.key.clone(),
                value: attribute.value.clone(),
            });
        }
        let specifier = request.specifier();
        let base = referrer
            .map(|key| PathBuf::from(key.as_str()))
            .as_deref()
            .and_then(Path::parent)
            .filter(|parent| !parent.as_os_str().is_empty())
            .map_or_else(|| self.entry_dir.clone(), Path::to_path_buf);
        let candidate = if Path::new(specifier).is_absolute() {
            PathBuf::from(specifier)
        } else if specifier.starts_with("./") || specifier.starts_with("../") {
            base.join(specifier)
        } else {
            // A bare specifier has no package resolution here; it is looked up
            // relative to the root, and nothing else.
            self.root.join(specifier)
        };
        let resolved = self.confine(specifier, &candidate)?;
        if !resolved.is_file() {
            return Err(ModuleLoadError::NotFound {
                specifier: specifier.to_string(),
                referrer: referrer.cloned(),
            });
        }
        if !bytes
            && !text
            && resolved
                .extension()
                .is_some_and(|extension| extension == "json")
                != json
        {
            return Err(ModuleLoadError::UnsupportedAttribute {
                key: "type".to_owned(),
                value: if json { "json" } else { "missing JSON type" }.to_owned(),
            });
        }
        Ok(ModuleKey::from_host(
            resolved.to_string_lossy().into_owned(),
        ))
    }

    fn canonical_key(&self, key: &str) -> ModuleKey {
        Self::canonical_entry_key(key)
    }

    fn load(&self, key: &ModuleRecordKey) -> Result<LoadedModule, ModuleLoadError> {
        // Entry loads and public trait calls need not pass through resolve.
        // Revalidate the path here as well: an earlier resolution is not an
        // enduring authority to read a path that now points outside the root.
        let locator = key.locator();
        let path = self.confine(locator.as_str(), Path::new(locator.as_str()))?;
        let io = |error: std::io::Error| ModuleLoadError::Io {
            key: locator.clone(),
            message: error.to_string(),
        };
        let kind = match key.kind() {
            ModuleKindIr::SourceText => {
                LoadedModuleKind::Source(std::fs::read_to_string(&path).map_err(io)?)
            }
            ModuleKindIr::Json => {
                LoadedModuleKind::Json(std::fs::read_to_string(&path).map_err(io)?)
            }
            ModuleKindIr::Bytes => LoadedModuleKind::Bytes(std::fs::read(&path).map_err(io)?),
            ModuleKindIr::Text => {
                // Match the host source decoding policy: reject malformed UTF-8,
                // preserve all decoded characters including a leading BOM.
                LoadedModuleKind::Text(std::fs::read_to_string(&path).map_err(io)?)
            }
            ModuleKindIr::ModuleSource => {
                return Err(ModuleLoadError::Denied {
                    specifier: locator.as_str().into(),
                    reason: "filesystem bytes do not supply a native ModuleSource record".into(),
                });
            }
            ModuleKindIr::Script => {
                return Err(ModuleLoadError::Denied {
                    specifier: locator.as_str().into(),
                    reason: "a Script is not an importable Module Record".into(),
                })
            }
        };
        Ok(LoadedModule {
            meta_url: format!("file://{}", path.display()),
            kind,
            key: locator.clone(),
        })
    }
}

impl<F> HostModuleLoader for LazyLock<FilesystemModuleLoader, F>
where
    F: FnOnce() -> FilesystemModuleLoader + Send + Sync,
{
    fn resolve(
        &self,
        referrer: Option<&ModuleKey>,
        request: &ModuleRequestKeyIr,
    ) -> Result<ModuleKey, ModuleLoadError> {
        LazyLock::force(self).resolve(referrer, request)
    }

    fn load(&self, key: &ModuleRecordKey) -> Result<LoadedModule, ModuleLoadError> {
        LazyLock::force(self).load(key)
    }

    fn canonical_key(&self, key: &str) -> ModuleKey {
        FilesystemModuleLoader::canonical_entry_key(key)
    }
}

/// Loads the transitive closure of `entry` and returns it in the shape
/// `scriptw-ir` consumes.
///
/// A request the loader rejects is simply left unresolved: `scriptw-ir` reports
/// it as a link diagnostic, which is where the `SyntaxError` a
/// `phase: resolution` negative expects comes from.
///
/// # Errors
/// Returns an error only when the *entry* module itself cannot be read.
pub fn load_module_graph(
    entry: &ModuleEntry,
    loader: &dyn HostModuleLoader,
) -> Result<ModuleGraphSources, ModuleLoadError> {
    // The entry never passes through `resolve`, so normalize its key here or a
    // module that imports the entry back gets a second copy of it.
    let entry_key = loader.canonical_key(entry.locator());
    let entry_module = match entry {
        ModuleEntry::InMemory { source_text, .. } => ModuleSourceIr::new(
            entry_key.clone(),
            source_text.clone(),
            format!("file://{}", entry_key.as_str()),
        ),
        ModuleEntry::HostLoad { .. } => {
            let loaded = loader.load(&ModuleRecordKey::new(
                entry_key.clone(),
                ModuleKindIr::SourceText,
            ))?;
            loaded_module_source(loaded)
        }
    };

    load_module_graph_from_entry(entry_module, loader, None)
}

/// [`load_module_graph`] with an entry already parsed by the compilation front
/// end. Dependencies enter through their declared Source Text/JSON factory; the
/// entry retains this exact syntax product instead of parsing its text again.
pub(crate) fn load_module_graph_from_parsed(
    entry_locator: &str,
    source: ParsedModule,
    loader: &dyn HostModuleLoader,
) -> Result<ModuleGraphSources, ModuleLoadError> {
    let entry_key = loader.canonical_key(entry_locator);
    let entry_module = ModuleSourceIr::from_parsed(
        entry_key.clone(),
        format!("file://{}", entry_key.as_str()),
        source,
    );
    load_module_graph_from_entry(entry_module, loader, None)
}

/// Script-entry counterpart of [`load_module_graph_from_parsed`]. The entry
/// keeps Script grammar and contributes only its `import()` requests; every
/// loaded target retains its declared Source Text or JSON grammar.
pub(crate) fn load_module_graph_from_parsed_script(
    entry_locator: &str,
    source: ParsedScript,
    loader: &dyn HostModuleLoader,
) -> Result<ModuleGraphSources, ModuleLoadError> {
    load_module_graph_from_discovered_script(entry_locator, source, loader, None)
}

type GraphLoadingRequests = (Vec<scriptw_ir::ModuleRequestIr>, Vec<ModuleRequestKeyIr>);

/// Test the retained Script before constructing its ambient loader or
/// canonicalizing its filename, then feed the same discovery to the driver.
pub(super) fn discover_module_graph_from_parsed_script(
    entry_locator: &str,
    source: &ParsedScript,
    loader: &mut Option<Box<dyn HostModuleLoader>>,
    configure_loader: impl FnOnce() -> Box<dyn HostModuleLoader>,
) -> Result<Option<ModuleGraphSources>, ModuleLoadError> {
    let Some(requests) = ModuleSourceIr::script_graph_loading_requests(source) else {
        return Ok(None);
    };
    load_module_graph_from_discovered_script(
        entry_locator,
        source.clone(),
        loader.get_or_insert_with(configure_loader).as_ref(),
        Some(requests),
    )
    .map(Some)
}

/// Consume the initial Script discovery when the caller already used it to
/// decide whether the Script needs a graph or a host loader.
fn load_module_graph_from_discovered_script(
    entry_locator: &str,
    source: ParsedScript,
    loader: &dyn HostModuleLoader,
    entry_requests: Option<GraphLoadingRequests>,
) -> Result<ModuleGraphSources, ModuleLoadError> {
    let entry_key = loader.canonical_key(entry_locator);
    let entry_module = ModuleSourceIr::from_parsed_script(
        entry_key.clone(),
        format!("file://{}", entry_key.as_str()),
        source,
    );
    load_module_graph_from_entry(entry_module, loader, entry_requests)
}

fn load_module_graph_from_entry(
    entry_module: ModuleSourceIr,
    loader: &dyn HostModuleLoader,
    entry_requests: Option<GraphLoadingRequests>,
) -> Result<ModuleGraphSources, ModuleLoadError> {
    let mut graph = ModuleGraphSources {
        modules: vec![entry_module],
        entry: 0,
        resolutions: Vec::new(),
        realm_requests: BTreeMap::new(),
    };
    extend_module_graph_requests_from_roots(
        &mut graph,
        &[],
        loader,
        VecDeque::from([0]),
        entry_requests,
    )?;
    Ok(graph)
}

/// Adds requests discovered while compiling retained finite source. Existing
/// parsed records and host resolutions survive; each newly loaded record is
/// parsed once by its ModuleSourceIr constructor.
pub(crate) fn extend_module_graph_realm_requests(
    graph: &mut ModuleGraphSources,
    requests_to_add: &[ModuleRequestKeyIr],
    loader: &dyn HostModuleLoader,
) -> Result<(), ModuleLoadError> {
    let roots = VecDeque::from([graph.entry as usize]);
    extend_module_graph_requests_from_roots(graph, requests_to_add, loader, roots, None)
}

fn extend_module_graph_requests_from_roots(
    graph: &mut ModuleGraphSources,
    requests_to_add: &[ModuleRequestKeyIr],
    loader: &dyn HostModuleLoader,
    mut pending: VecDeque<usize>,
    mut entry_requests: Option<GraphLoadingRequests>,
) -> Result<(), ModuleLoadError> {
    let entry = graph.entry as usize;
    if entry >= graph.modules.len() {
        return Err(ModuleLoadError::Denied {
            specifier: String::new(),
            reason: "module graph has no entry source".into(),
        });
    }
    let modules = &mut graph.modules;
    let resolutions = &mut graph.resolutions;
    let realm_requests = &mut graph.realm_requests;
    // Both the key a request resolved to and the key its load reported map
    // here, so a loader that normalizes further on load still reads each
    // module once.
    let mut indices: BTreeMap<ModuleRecordKey, u32> = BTreeMap::new();
    // A Script Record is not a member of the host module map. Importing its
    // own URL loads that URL under Module grammar and owns a separate module
    // environment, even when the bytes happen to be identical.
    for (index, module) in modules.iter().enumerate() {
        if module.goal() == ParseGoal::Module {
            indices.insert(module.record_key(), index as u32);
        }
    }
    let mut expanded = BTreeSet::new();

    while let Some(cursor) = pending.pop_front() {
        if !expanded.insert(cursor) {
            continue;
        }
        let referrer = u32::try_from(cursor).unwrap_or(u32::MAX);
        let key = modules[cursor].key().clone();
        // A rejected module produces no requests; `scriptw-ir` reports the
        // retained parse failure itself without trying the parser again.
        let discovered = if cursor == entry {
            entry_requests.take()
        } else {
            None
        };
        let (requests, mut realm_candidates) =
            discovered.unwrap_or_else(|| modules[cursor].graph_loading_requests());
        let source_requests = requests
            .into_iter()
            .map(|request| (Some(referrer), request));
        if cursor == entry {
            realm_candidates.extend_from_slice(requests_to_add);
        }
        let realm_candidates = realm_candidates.into_iter().map(|request| {
            (
                None,
                scriptw_ir::ModuleRequestIr::from_key(
                    request,
                    scriptw_ir::ImportPhaseIr::Evaluation,
                ),
            )
        });
        for (origin, occurrence) in source_requests.chain(realm_candidates) {
            let recursive = occurrence.phase().loads_dependencies();
            let request = occurrence.key().clone();
            if origin.is_none() && realm_requests.contains_key(&request) {
                continue;
            }
            if let Some(owner) = origin {
                if let Some((_, _, target)) = resolutions
                    .iter()
                    .find(|(referrer, key, _)| *referrer == owner && key == &request)
                {
                    if recursive {
                        pending.push_back(*target as usize);
                    }
                    continue;
                }
            }
            // A request the loader rejects stays unresolved on purpose: the
            // link stage turns it into the SyntaxError the spec asks for,
            // instead of the whole compile failing here.
            let target_key = match loader.resolve(origin.map(|_| &key), &request) {
                Ok(key) => key,
                Err(error) => {
                    if origin.is_none() {
                        realm_requests.insert(
                            request,
                            scriptw_ir::RealmModuleResolutionIr::Rejected(error.to_string()),
                        );
                    }
                    continue;
                }
            };
            let target_record =
                ModuleRecordKey::new(target_key.clone(), ModuleKindIr::requested(&request));
            if let Some(index) = indices.get(&target_record).copied() {
                loader.validate_resolved_module(&request, &modules[index as usize])?;
                if !modules[index as usize].kind().matches_request(&request) {
                    if origin.is_none() {
                        realm_requests.insert(
                            request,
                            scriptw_ir::RealmModuleResolutionIr::Rejected(
                                "loaded module record kind does not match import attributes".into(),
                            ),
                        );
                    }
                    continue;
                }
                match origin {
                    Some(referrer) => resolutions.push((referrer, request, index)),
                    None => {
                        realm_requests
                            .insert(request, scriptw_ir::RealmModuleResolutionIr::Loaded(index));
                    }
                }
                if recursive {
                    pending.push_back(index as usize);
                }
                continue;
            }
            let loaded = match loader.load(&target_record) {
                Ok(loaded) => loaded,
                Err(error) => {
                    if origin.is_none() {
                        realm_requests.insert(
                            request,
                            scriptw_ir::RealmModuleResolutionIr::Rejected(error.to_string()),
                        );
                    }
                    continue;
                }
            };
            let source = loaded_module_source(loaded);
            loader.validate_resolved_module(&request, &source)?;
            let loaded_kind = source.kind();
            if !loaded_kind.matches_request(&request) {
                if origin.is_none() {
                    realm_requests.insert(
                        request,
                        scriptw_ir::RealmModuleResolutionIr::Rejected(
                            "loaded module record kind does not match import attributes".into(),
                        ),
                    );
                }
                continue;
            }
            // The load may report a key the graph already holds: module map
            // identity is keyed on the loaded key, not on the specifier that
            // reached it. Text that disagrees is pushed through as a duplicate
            // key so `build_graph` reports one `InconsistentLoad` rather than
            // silently running one of the two.
            let existing = indices.get(&source.record_key()).copied();
            let target = match existing {
                Some(index)
                    if modules[index as usize].payload_bytes() == source.payload_bytes()
                        && modules[index as usize].meta_url() == source.meta_url() =>
                {
                    index
                }
                _ => {
                    let index = u32::try_from(modules.len()).unwrap_or(u32::MAX);
                    let loaded_key = source.record_key();
                    modules.push(source);
                    if existing.is_none() {
                        indices.insert(loaded_key, index);
                    }
                    index
                }
            };
            indices.insert(target_record, target);
            match origin {
                Some(referrer) => resolutions.push((referrer, request, target)),
                None => {
                    realm_requests
                        .insert(request, scriptw_ir::RealmModuleResolutionIr::Loaded(target));
                }
            }
            // A source request loads/parses only this record. A later ordinary
            // or deferred request expands the same cached record exactly once.
            if recursive {
                pending.push_back(target as usize);
            }
        }
    }

    Ok(())
}

fn loaded_module_source(loaded: LoadedModule) -> ModuleSourceIr {
    match loaded.kind {
        LoadedModuleKind::Foreign { module, error_tag } => {
            ModuleSourceIr::foreign(loaded.key, module, error_tag)
        }
        LoadedModuleKind::Source(text) => ModuleSourceIr::new(loaded.key, text, loaded.meta_url),
        LoadedModuleKind::ModuleSource { source_tag } => {
            ModuleSourceIr::module_source(loaded.key, source_tag, loaded.meta_url)
        }
        LoadedModuleKind::Json(text) => ModuleSourceIr::json(loaded.key, text, loaded.meta_url),
        LoadedModuleKind::Bytes(bytes) => ModuleSourceIr::bytes(loaded.key, bytes, loaded.meta_url),
        LoadedModuleKind::Text(text) => ModuleSourceIr::text(loaded.key, text, loaded.meta_url),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptw_ir::ImportAttributeIr;
    use std::fs;

    /// `base/root` is the module root; `base` itself is off-limits, which is
    /// what the traversal tests reach for.
    fn temp_base(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "scriptw-modules-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("root")).unwrap();
        base
    }

    fn write_tree(root: &Path, files: &[(&str, &str)]) {
        for (name, text) in files {
            let path = root.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, text).unwrap();
        }
    }

    fn loader_at(root: &Path) -> FilesystemModuleLoader {
        let root = root.to_string_lossy().into_owned();
        FilesystemModuleLoader::new(Some(root.as_str()), None).unwrap()
    }

    fn entry_at(path: &Path) -> ModuleEntry {
        ModuleEntry::HostLoad {
            locator: path.to_string_lossy().into_owned(),
        }
    }

    #[test]
    fn a_host_entry_loads_its_source_through_the_loader() {
        let base = temp_base("host-entry");
        let root = base.join("root");
        write_tree(&root, &[("entry.js", "export const origin = 'host';")]);
        let loader = loader_at(&root);

        let sources = load_module_graph(&entry_at(&root.join("entry.js")), &loader).unwrap();
        assert_eq!(
            sources.modules[0].source_text(),
            "export const origin = 'host';"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_script_importing_its_url_loads_a_separate_module_record() {
        let base = temp_base("script-module-same-url");
        let root = base.join("root");
        write_tree(
            &root,
            &[("entry.js", "import './entry.js'; export const value = 42;")],
        );
        let loader = loader_at(&root);
        let scriptw_front::ParsedSource::Script(script) = scriptw_front::parse(
            "import('./entry.js'); import('./entry.js');",
            scriptw_front::ParseOptions::script(),
        )
        .unwrap() else {
            panic!("Script fixture parses with Script grammar")
        };
        let sources = load_module_graph_from_parsed_script(
            &root.join("entry.js").to_string_lossy(),
            script,
            &loader,
        )
        .unwrap();
        assert_eq!(sources.modules.len(), 2);
        assert_eq!(sources.modules[0].goal(), ParseGoal::Script);
        assert_eq!(sources.modules[1].goal(), ParseGoal::Module);
        assert_eq!(sources.modules[0].key(), sources.modules[1].key());
        assert!(sources
            .resolutions
            .iter()
            .all(|(_, _, target)| *target == 1));
        assert!(sources
            .resolutions
            .iter()
            .any(|(referrer, _, _)| *referrer == 0));
        assert!(sources
            .resolutions
            .iter()
            .any(|(referrer, _, _)| *referrer == 1));
        let program = scriptw_ir::lower_script_graph(&sources);
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let graph = program.modules.expect("distinct Script and Module units");
        assert_eq!(graph.units.len(), 2);
        assert_eq!(graph.entry, 0);
        assert_eq!(graph.keys[&sources.modules[1].record_key()], 1);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn ambient_script_discovery_distinguishes_requests_from_method_names() {
        let parse_script = |source| {
            let ParsedSource::Script(script) =
                scriptw_front::parse(source, scriptw_front::ParseOptions::script()).unwrap()
            else {
                unreachable!()
            };
            script
        };
        let mut loader = None;
        let graph_free = parse_script("({ import() {}, importValue() {} });");
        assert!(discover_module_graph_from_parsed_script(
            "entry.js",
            &graph_free,
            &mut loader,
            || { panic!("a graph-free Script must not construct an ambient loader") }
        )
        .unwrap()
        .is_none());
        assert!(loader.is_none());

        let base = temp_base("script-discovery");
        write_tree(&base, &[("source.js", "export const value = 1;")]);
        let computed = parse_script("function get(name) { return import(name); }");
        let graph =
            discover_module_graph_from_parsed_script("entry.js", &computed, &mut loader, || {
                Box::new(loader_at(&base))
            })
            .unwrap()
            .expect("computed import needs a Script graph");
        assert_eq!(graph.modules.len(), 1);
        assert!(graph.resolutions.is_empty());

        let both =
            parse_script(r#"import('source.js'); realm['import\u0056alue']('realm', 'value');"#);
        let graph =
            discover_module_graph_from_parsed_script("entry.js", &both, &mut loader, || {
                panic!("an installed loader must be reused")
            })
            .unwrap()
            .expect("literal requests need a Script graph");
        assert_eq!(
            graph.resolutions,
            [(0, ModuleRequestKeyIr::plain("source.js"), 1)]
        );
        assert!(matches!(
            graph
                .realm_requests
                .get(&ModuleRequestKeyIr::plain("realm")),
            Some(scriptw_ir::RealmModuleResolutionIr::Rejected(_))
        ));
        assert_eq!(graph.realm_requests.len(), 1);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn an_in_memory_entry_does_not_ask_the_host_to_load_it() {
        let entry = ModuleEntry::InMemory {
            locator: "virtual/entry.js".to_string(),
            source_text: "export const origin = 'memory';".to_string(),
        };

        let sources = load_module_graph(&entry, &RejectAllModuleLoader).unwrap();
        assert_eq!(sources.modules[0].key().as_str(), "virtual/entry.js");
        assert_eq!(
            sources.modules[0].source_text(),
            "export const origin = 'memory';"
        );
    }

    #[test]
    fn reject_all_loader_never_resolves_or_loads() {
        let loader = RejectAllModuleLoader;
        let referrer = ModuleKey::from_host("entry.js");
        let request = ModuleRequestKeyIr::plain("./ambient.js");
        assert!(matches!(
            loader.resolve(Some(&referrer), &request),
            Err(ModuleLoadError::Denied { .. })
        ));
        assert!(matches!(
            loader.load(&ModuleRecordKey::new(
                ModuleKey::from_host("ambient.js"),
                ModuleKindIr::SourceText
            )),
            Err(ModuleLoadError::Denied { .. })
        ));
    }

    #[test]
    fn discovered_realm_requests_keep_host_origin_and_closed_cached_outcomes() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct RealmLoader {
            loads: AtomicUsize,
        }
        impl HostModuleLoader for RealmLoader {
            fn resolve(
                &self,
                referrer: Option<&ModuleKey>,
                request: &ModuleRequestKeyIr,
            ) -> Result<ModuleKey, ModuleLoadError> {
                assert!(
                    referrer.is_none(),
                    "Realm requests do not borrow entry or caller source identity"
                );
                Ok(ModuleKey::from_host(request.specifier()))
            }
            fn load(&self, record: &ModuleRecordKey) -> Result<LoadedModule, ModuleLoadError> {
                let key = record.locator();
                self.loads.fetch_add(1, Ordering::SeqCst);
                let kind = match key.as_str() {
                    "value" => LoadedModuleKind::Source("export const value = 7;".into()),
                    "json" => LoadedModuleKind::Json("7".into()),
                    _ => {
                        return Err(ModuleLoadError::Io {
                            key: key.clone(),
                            message: "original host failure".into(),
                        })
                    }
                };
                Ok(LoadedModule {
                    key: key.clone(),
                    kind,
                    meta_url: format!("scriptw://{}", key.as_str()),
                })
            }
        }
        let ParsedSource::Script(parsed) =
            scriptw_front::parse("0;", scriptw_front::ParseOptions::script()).unwrap()
        else {
            unreachable!()
        };
        let loader = RealmLoader {
            loads: AtomicUsize::new(0),
        };
        let mut graph = load_module_graph_from_parsed_script("entry.js", parsed, &loader).unwrap();
        let requests = ["value", "json", "absent"].map(ModuleRequestKeyIr::plain);
        extend_module_graph_realm_requests(&mut graph, &requests, &loader).unwrap();
        assert!(matches!(
            graph.realm_requests[&requests[0]],
            scriptw_ir::RealmModuleResolutionIr::Loaded(1)
        ));
        for request in &requests[1..] {
            assert!(
                matches!(&graph.realm_requests[request], scriptw_ir::RealmModuleResolutionIr::Rejected(message) if !message.is_empty())
            );
        }
        assert!(
            matches!(&graph.realm_requests[&requests[2]], scriptw_ir::RealmModuleResolutionIr::Rejected(message) if message.contains("original host failure"))
        );
        let retained = graph.modules.clone();
        extend_module_graph_realm_requests(&mut graph, &requests, &loader).unwrap();
        assert_eq!(
            loader.loads.load(Ordering::SeqCst),
            3,
            "every request has a cached success or rejection"
        );
        assert_eq!(
            graph.modules, retained,
            "extension retains the original parsed records"
        );
        assert!(
            graph.resolutions.is_empty(),
            "Realm requests never manufacture source edges"
        );
    }

    #[test]
    fn parent_traversal_out_of_the_root_is_denied() {
        let base = temp_base("traversal");
        let root = base.join("root");
        write_tree(&base, &[("outside.js", "export const secret = 1;")]);
        write_tree(&root, &[("entry.js", "import '../outside.js';")]);
        let loader = loader_at(&root);

        let referrer = ModuleKey::from_host(root.join("entry.js").to_string_lossy().into_owned());
        let denied = loader.resolve(Some(&referrer), &ModuleRequestKeyIr::plain("../outside.js"));
        assert!(
            matches!(denied, Err(ModuleLoadError::Denied { .. })),
            "{denied:?}"
        );

        // The graph still loads: the rejected request is simply unresolved, and
        // becomes a link error rather than a failed compile.
        let sources = load_module_graph(&entry_at(&root.join("entry.js")), &loader).unwrap();
        assert_eq!(sources.modules.len(), 1);
        assert!(sources.resolutions.is_empty());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn an_absolute_specifier_outside_the_root_is_denied() {
        let base = temp_base("absolute");
        let root = base.join("root");
        write_tree(&base, &[("outside.js", "export const secret = 1;")]);
        write_tree(&root, &[("entry.js", "")]);
        let loader = loader_at(&root);

        let outside = base.join("outside.js").to_string_lossy().into_owned();
        let referrer = ModuleKey::from_host(root.join("entry.js").to_string_lossy().into_owned());
        let denied = loader.resolve(Some(&referrer), &ModuleRequestKeyIr::plain(outside));
        assert!(
            matches!(denied, Err(ModuleLoadError::Denied { .. })),
            "{denied:?}"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_pointing_out_of_the_root_is_denied() {
        let base = temp_base("symlink");
        let root = base.join("root");
        write_tree(&base, &[("outside.js", "export const secret = 1;")]);
        write_tree(&root, &[("entry.js", "")]);
        std::os::unix::fs::symlink(base.join("outside.js"), root.join("link.js")).unwrap();
        let loader = loader_at(&root);

        let referrer = ModuleKey::from_host(root.join("entry.js").to_string_lossy().into_owned());
        let denied = loader.resolve(Some(&referrer), &ModuleRequestKeyIr::plain("./link.js"));
        assert!(
            matches!(denied, Err(ModuleLoadError::Denied { .. })),
            "{denied:?}"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn parent_components_never_pop_the_filesystem_root() {
        // Without the floor, this would normalize to the *relative* `etc`,
        // which the confinement check would then re-anchor at the root it is
        // supposed to be guarding.
        assert_eq!(
            normalize(Path::new("/a/../../etc/passwd")),
            PathBuf::from("/etc/passwd")
        );
        assert_eq!(normalize(Path::new("/a/b/../c")), PathBuf::from("/a/c"));
    }

    #[test]
    fn two_specifiers_naming_one_file_load_one_module() {
        let base = temp_base("module-map");
        let root = base.join("root");
        write_tree(
            &root,
            &[
                (
                    "entry.js",
                    "import { x } from './dep.js';\nimport * as ns from './sub/../dep.js';\nx; ns;",
                ),
                ("dep.js", "export const x = 1;"),
            ],
        );
        let loader = loader_at(&root);

        let sources = load_module_graph(&entry_at(&root.join("entry.js")), &loader).unwrap();
        assert_eq!(sources.modules.len(), 2);
        assert_eq!(sources.resolutions.len(), 2);
        assert!(sources
            .resolutions
            .iter()
            .all(|(referrer, _, target)| *referrer == 0 && *target == 1));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn phase_variants_share_one_filesystem_resolution() {
        let base = temp_base("phase-free-resolution");
        let root = base.join("root");
        write_tree(
            &root,
            &[
                (
                    "entry.js",
                    "import './dep.js';\n\
                     import defer * as deferred from './dep.js';\n\
                     import source artifact from './dep.js';\n\
                     deferred; artifact;",
                ),
                ("dep.js", "export const value = 1;"),
            ],
        );
        let loader = loader_at(&root);

        let sources = load_module_graph(&entry_at(&root.join("entry.js")), &loader).unwrap();
        assert_eq!(sources.modules.len(), 2);
        assert_eq!(
            sources.resolutions,
            vec![(0, ModuleRequestKeyIr::plain("./dep.js"), 1)]
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn source_loading_parses_one_record_without_loading_its_dependencies() {
        let base = temp_base("source-single-record");
        write_tree(
            &base,
            &[
                ("entry.js", "import.source('./source.js');"),
                (
                    "source.js",
                    "import './invalid.js'; export const value = 1;",
                ),
                ("invalid.js", "invalid syntax!"),
            ],
        );
        let sources =
            load_module_graph(&entry_at(&base.join("entry.js")), &loader_at(&base)).unwrap();
        assert_eq!(sources.modules.len(), 2);
        assert_eq!(sources.resolutions.len(), 1);
        let program = scriptw_ir::lower_module_graph(&sources);
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        assert_eq!(program.modules.as_ref().unwrap().units.len(), 1);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_later_non_source_request_expands_the_same_cached_record() {
        for request in ["import('./source.js');", "import.defer('./source.js');"] {
            let base = temp_base("source-promotion");
            write_tree(
                &base,
                &[
                    (
                        "entry.js",
                        "import.source('./source.js'); import './other.js';",
                    ),
                    (
                        "source.js",
                        "import './dependency.js'; export const value = 1;",
                    ),
                    ("other.js", request),
                    ("dependency.js", "export const value = 2;"),
                ],
            );
            let sources =
                load_module_graph(&entry_at(&base.join("entry.js")), &loader_at(&base)).unwrap();
            assert_eq!(sources.modules.len(), 4);
            assert_eq!(
                sources
                    .modules
                    .iter()
                    .filter(|source| source.key().as_str().ends_with("source.js"))
                    .count(),
                1
            );
            assert!(sources
                .resolutions
                .iter()
                .any(|(_, request, _)| request.specifier() == "./dependency.js"));
            let _ = fs::remove_dir_all(&base);
        }
    }

    #[test]
    fn an_entry_that_imports_itself_stays_one_module() {
        let base = temp_base("self-import");
        let root = base.join("root");
        write_tree(
            &root,
            // The self-import has to be aliased. Importing your own export
            // under its own name declares it twice in one module environment -
            // the import binding and the `export const` - so
            // `import { x } from './entry.js'; export const x = 1;` is a
            // SyntaxError (16.2.1.2) rather than a graph shape, and the parse
            // fails before any request is scanned.
            &[(
                "entry.js",
                "import { x as y } from './entry.js';\nexport const x = 1;\ny;",
            )],
        );
        let loader = loader_at(&root);

        // A key spelled differently from what `resolve` produces: normalizing
        // it is what keeps the entry from being loaded a second time.
        let entry = entry_at(&root.join(".").join("entry.js"));
        let sources = load_module_graph(&entry, &loader).unwrap();
        assert_eq!(sources.modules.len(), 1);
        assert_eq!(sources.resolutions.len(), 1);
        assert_eq!(sources.resolutions[0].2, 0);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_missing_module_is_left_unresolved() {
        let base = temp_base("missing");
        let root = base.join("root");
        write_tree(&root, &[("entry.js", "import { x } from './nope.js';\nx;")]);
        let loader = loader_at(&root);

        let referrer = ModuleKey::from_host(root.join("entry.js").to_string_lossy().into_owned());
        let missing = loader.resolve(Some(&referrer), &ModuleRequestKeyIr::plain("./nope.js"));
        assert!(
            matches!(missing, Err(ModuleLoadError::NotFound { .. })),
            "{missing:?}"
        );

        let sources = load_module_graph(&entry_at(&root.join("entry.js")), &loader).unwrap();
        assert_eq!(sources.modules.len(), 1);
        assert!(sources.resolutions.is_empty());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn an_unsupported_import_attribute_is_rejected() {
        let base = temp_base("attributes");
        let root = base.join("root");
        write_tree(&root, &[("dep.js", "export const x = 1;")]);
        let loader = loader_at(&root);

        let request = ModuleRequestKeyIr::try_new(
            "./dep.js",
            vec![ImportAttributeIr {
                key: "type".to_string(),
                value: "css".to_string(),
            }],
        )
        .expect("the test attribute key is unique");
        let referrer = ModuleKey::from_host(root.join("dep.js").to_string_lossy().into_owned());
        let rejected = loader.resolve(Some(&referrer), &request);
        assert!(
            matches!(rejected, Err(ModuleLoadError::UnsupportedAttribute { .. })),
            "{rejected:?}"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_json_module_is_loaded_as_a_checked_distinct_kind() {
        let base = temp_base("json");
        let root = base.join("root");
        write_tree(&root, &[("data.json", "{\"a\": 1}")]);
        let loader = loader_at(&root);

        // JSON's declared type selects its own native parse owner.
        let request = ModuleRequestKeyIr::try_new(
            "./data.json",
            vec![ImportAttributeIr {
                key: "type".to_string(),
                value: "json".to_string(),
            }],
        )
        .expect("the test attribute key is unique");
        let referrer = ModuleKey::from_host(root.join("entry.js").to_string_lossy().into_owned());
        let key = loader.resolve(Some(&referrer), &request).unwrap();
        let loaded = loader
            .load(&ModuleRecordKey::new(key.clone(), ModuleKindIr::Json))
            .unwrap();
        assert_eq!(loaded.kind, LoadedModuleKind::Json("{\"a\": 1}".to_owned()));
        let source = loaded_module_source(loaded);
        assert_eq!(source.kind(), scriptw_ir::ModuleKindIr::Json);
        assert_eq!(source.module_requests(), Some(Vec::new()));
        assert!(matches!(
            loader.resolve(Some(&referrer), &ModuleRequestKeyIr::plain("./data.json")),
            Err(ModuleLoadError::UnsupportedAttribute { .. })
        ));
        write_tree(
            &root,
            &[(
                "entry.js",
                "import value from './data.json' with { type: 'json' }; value;",
            )],
        );
        let graph = load_module_graph(&entry_at(&root.join("entry.js")), &loader).unwrap();
        assert_eq!(graph.modules[1].kind(), scriptw_ir::ModuleKindIr::Json);
        assert!(scriptw_ir::lower_module_graph(&graph).is_wasm_supported());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_transitive_closure_loads_every_module_once() {
        let base = temp_base("closure");
        let root = base.join("root");
        write_tree(
            &root,
            &[
                ("entry.js", "import './a.js';\nimport './b.js';"),
                ("a.js", "import './shared.js';"),
                ("b.js", "import './shared.js';"),
                ("shared.js", "export const x = 1;"),
            ],
        );
        let loader = loader_at(&root);

        let sources = load_module_graph(&entry_at(&root.join("entry.js")), &loader).unwrap();
        assert_eq!(sources.modules.len(), 4);
        let mut keys: Vec<&str> = sources
            .modules
            .iter()
            .map(|source| source.key().as_str())
            .collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), 4);
        assert_eq!(sources.resolutions.len(), 4);
        let _ = fs::remove_dir_all(&base);
    }
}

#[cfg(test)]
#[path = "../tests/unit/module_load_confinement.rs"]
mod confinement_tests;

#[cfg(test)]
mod text_module_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "scriptw-text-loader-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn text_host_loading_preserves_decoding_and_mixed_self_record_identity() {
        let directory = Directory::new();
        let source = "import raw from './entry.js' with {type:'text'}; export default raw;";
        let entry_path = directory.0.join("entry.js");
        std::fs::write(&entry_path, source).unwrap();
        let root = directory.0.to_str().unwrap();
        let loader =
            FilesystemModuleLoader::new(Some(root), Some(entry_path.to_str().unwrap())).unwrap();
        let graph = load_module_graph(
            &ModuleEntry::HostLoad {
                locator: entry_path.to_string_lossy().into_owned(),
            },
            &loader,
        )
        .unwrap();
        assert_eq!(graph.modules.len(), 2);
        assert_eq!(graph.modules[0].key(), graph.modules[1].key());
        assert_eq!(graph.modules[0].kind(), ModuleKindIr::SourceText);
        assert_eq!(graph.modules[1].kind(), ModuleKindIr::Text);
        assert_eq!(graph.modules[1].source_text(), source);
        assert!(graph.modules[1].module_requests().unwrap().is_empty());
        assert!(scriptw_ir::lower_module_graph(&graph).is_wasm_supported());
        let text = "\u{feff}Ā\0𝄞\n{not JSON}";
        std::fs::write(directory.0.join("raw.json"), text).unwrap();
        let referrer = graph.modules[0].key();
        let request = ModuleRequestKeyIr::try_new(
            "./raw.json",
            [scriptw_ir::ImportAttributeIr {
                key: "type".into(),
                value: "text".into(),
            }],
        )
        .unwrap();
        let key = loader.resolve(Some(referrer), &request).unwrap();
        let loaded = loader
            .load(&ModuleRecordKey::new(key.clone(), ModuleKindIr::Text))
            .unwrap();
        assert_eq!(loaded.kind, LoadedModuleKind::Text(text.into()));
        assert_eq!(loaded_module_source(loaded).source_text(), text);
        std::fs::write(directory.0.join("raw.json"), [0xff, 0x80]).unwrap();
        assert!(matches!(
            loader.load(&ModuleRecordKey::new(key, ModuleKindIr::Text)),
            Err(ModuleLoadError::Io { .. })
        ));
        let invalid = ModuleRequestKeyIr::try_new(
            "./raw.json",
            [
                scriptw_ir::ImportAttributeIr {
                    key: "type".into(),
                    value: "text".into(),
                },
                scriptw_ir::ImportAttributeIr {
                    key: "extra".into(),
                    value: "no".into(),
                },
            ],
        )
        .unwrap();
        assert!(matches!(
            loader.resolve(Some(referrer), &invalid),
            Err(ModuleLoadError::UnsupportedAttribute { .. })
        ));
    }
}
