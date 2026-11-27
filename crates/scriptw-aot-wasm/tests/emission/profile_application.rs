//! Profile application over emitted P.
//!
//! A profile may only cost speed: every profile, empty, adversarial or
//! true, must keep the site map, and an empty one must leave P
//! byte-identical. Validation is all this crate can check; executing the
//! profiled programs against their unprofiled output happens in the engine.
//!
//! Three tests are inert unless their variable is set:
//! - `SCRIPTW_PGO_CAPTURE_OUT=<dir>`: the SHA-256 of R and of every CLI
//!   fixture's P, so a change to R's proof emitters can be shown to leave R
//!   and unprofiled P byte-identical (`diff -r` two captures).
//! - `SCRIPTW_PGO_CORPUS=1`: every CLI fixture under the empty profile, two
//!   adversarial profiles and each specialisation at every read site.
//! - `SCRIPTW_PGO_SIZE_SOURCE=<script>`: P size under synthetic profiles.
use scriptw_aot_wasm::{
    emit, AdmittedProfile, CompactStorageLayout, ExecutionProfile, PlaceHolder, ProfileMode,
    ProgramKey, PromiseRejectionPolicy, ReadSiteObservation, ReadSpecialisation, ReceiverHeader,
    RuntimeArtifactInputs, SiteBody, SiteMap, SiteRef, WasmArtifact,
};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower, ProgramIr};
use sha2::{Digest, Sha256};
use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

const CAPTURE_ENV: &str = "SCRIPTW_PGO_CAPTURE_OUT";
const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn fixture_paths() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../scriptw-cli/tests/fixtures")
        .canonicalize()
        .expect("CLI fixture corpus resolves");
    let mut paths = fs::read_dir(&root)
        .expect("CLI fixture corpus reads")
        .map(|entry| entry.expect("fixture entry reads").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "js"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

/// Runs `each` over every fixture on a few large-stack workers, in fixture
/// order.
fn over_fixtures<T: Send>(each: impl Fn(&str, &str) -> T + Sync) -> Vec<T> {
    let paths = fixture_paths();
    let workers = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(2)
        .min(paths.len());
    let each = &each;
    let mut results = std::thread::scope(|scope| {
        (0..workers)
            .map(|worker| {
                let paths = &paths;
                std::thread::Builder::new()
                    .stack_size(WORKER_STACK_BYTES)
                    .spawn_scoped(scope, move || {
                        paths
                            .iter()
                            .enumerate()
                            .filter(|(index, _)| index % workers == worker)
                            .map(|(index, path)| {
                                let name = path.file_name().unwrap().to_string_lossy();
                                let source = fs::read_to_string(path).unwrap();
                                (index, each(&name, &source))
                            })
                            .collect::<Vec<_>>()
                    })
                    .expect("fixture worker spawns")
            })
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|worker| worker.join().expect("fixture worker completes"))
            .collect::<Vec<_>>()
    });
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

#[test]
fn capture_profile_digests() {
    let Some(output) = std::env::var_os(CAPTURE_ENV) else {
        return;
    };
    let output = PathBuf::from(output);
    fs::create_dir_all(&output).unwrap();
    let lines = over_fixtures(|name, source| {
        let Ok(unit) = parse(source, ParseOptions::script()) else {
            return (format!("{name} parse-error\n"), None);
        };
        match emit(&lower(&unit)) {
            Ok(artifact) => (
                format!(
                    "{name} bytes={} sha256={}\n",
                    artifact.bytes.len(),
                    hex(&artifact.bytes)
                ),
                artifact.runtime().map(|runtime| hex(runtime.bytes())),
            ),
            Err(error) => (format!("{name} emit-error {error}\n"), None),
        }
    });
    let mut runtimes = lines
        .iter()
        .filter_map(|(_, runtime)| runtime.clone())
        .collect::<Vec<_>>();
    runtimes.sort();
    runtimes.dedup();
    fs::write(output.join("runtime.sha256"), runtimes.join("\n") + "\n").unwrap();
    fs::write(
        output.join("programs.txt"),
        lines.into_iter().map(|(line, _)| line).collect::<String>(),
    )
    .unwrap();
}

const PROGRAM: ProgramKey = ProgramKey::new([0x5c; 32]);

fn program(source: &str) -> Option<ProgramIr> {
    parse(source, ParseOptions::script())
        .ok()
        .map(|unit| lower(&unit))
}

fn emit_with(program: &ProgramIr, mode: ProfileMode<'_>) -> Result<WasmArtifact, String> {
    scriptw_aot_wasm::emit_with_intl_profile_and_runtime_inputs(
        program,
        PromiseRejectionPolicy::default(),
        &scriptw_intl::IntlCompilationProfile::default(),
        RuntimeArtifactInputs::new(None),
        mode,
    )
    .map_err(|error| error.to_string())
}

fn validate(artifact: &WasmArtifact, what: &str) {
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&artifact.bytes)
        .unwrap_or_else(|error| panic!("{what}: {error}"));
}

/// A profile of one training request in which `kind` names each hot read
/// site's dominant kind.
fn profile(
    site_map: &SiteMap,
    mut kind: impl FnMut(usize, &SiteRef) -> Option<ReadSpecialisation>,
) -> AdmittedProfile {
    let mut profile = ExecutionProfile::new(PROGRAM, site_map, "synthetic", NonZeroU32::MIN);
    for (index, site) in site_map.read_sites().enumerate() {
        if let Some(dominant) = kind(index, site) {
            profile
                .observe_read(site, ReadSiteObservation::dominated(1 << 20, dominant))
                .unwrap();
        }
    }
    let decoded = ExecutionProfile::decode(&profile.encode()).unwrap();
    assert_eq!(decoded, profile);
    AdmittedProfile::admit(&decoded, &PROGRAM).unwrap()
}

/// Every read site hot, its kind rotating through every specialisation:
/// nearly every guard proves a kind its receivers never have.
fn adversarial(site_map: &SiteMap, seed: usize) -> AdmittedProfile {
    let kinds = ReadSpecialisation::ALL;
    profile(site_map, |index, _| {
        Some(kinds[(index + seed) % kinds.len()])
    })
}

fn uniform(site_map: &SiteMap, kind: ReadSpecialisation) -> AdmittedProfile {
    profile(site_map, |_, _| Some(kind))
}

fn empty(site_map: &SiteMap) -> AdmittedProfile {
    profile(site_map, |_, _| None)
}

/// Hand-written programs whose one read site in `probe` meets receivers of
/// one known kind: a true profile marks that site with it.
const TRUE_KIND_FIXTURES: &[(&str, &str, ReadSpecialisation)] = &[
    (
        "own inline",
        "function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) t += probe({ x: i }); t;",
        ReadSpecialisation::OwnCompact(CompactStorageLayout::Inline),
    ),
    (
        "own array",
        "function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) t += probe({ a: 1, b: 2, c: 3, x: i }); t;",
        ReadSpecialisation::OwnCompact(CompactStorageLayout::Array),
    ),
    (
        "absent inline",
        "function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) if (probe({ y: i }) === undefined) t++; t;",
        ReadSpecialisation::AbsentClosedTail(CompactStorageLayout::Inline),
    ),
    (
        "absent array",
        "function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) if (probe({ a: 1, b: 2, c: i }) === undefined) t++; t;",
        ReadSpecialisation::AbsentClosedTail(CompactStorageLayout::Array),
    ),
    (
        "prototype place",
        "function C() {} C.prototype.x = 3; function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) t += probe(new C()); t;",
        // A trained run proves this kind (the engine's execution_profile
        // tests): `new C()` has array-layout storage and C.prototype is a
        // dictionary.
        ReadSpecialisation::ProtoPlaceCompact(CompactStorageLayout::Array, PlaceHolder::Dictionary),
    ),
    (
        "object prototype",
        "function probe(o) { return o.hasOwnProperty; } var t = 0; for (var i = 0; i < 64; i++) if (probe({ y: i }) === Object.prototype.hasOwnProperty) t++; t;",
        ReadSpecialisation::ProtoPlaceCompact(CompactStorageLayout::Inline, PlaceHolder::Intrinsic),
    ),
    (
        "function prototype",
        "function g() {} function probe(o) { return o.call; } var t = 0; for (var i = 0; i < 64; i++) if (probe(g) === Function.prototype.call) t++; t;",
        ReadSpecialisation::ProtoPlaceIntrinsic(ReceiverHeader::Function),
    ),
    (
        "weak map prototype",
        "var w = new WeakMap(); function probe(o) { return o.get; } var t = 0; for (var i = 0; i < 64; i++) if (probe(w) === WeakMap.prototype.get) t++; t;",
        ReadSpecialisation::ProtoPlaceIntrinsic(ReceiverHeader::WeakMap),
    ),
    (
        "array prototype",
        "var a = [1, 2]; function probe(o) { return o.push; } var t = 0; for (var i = 0; i < 64; i++) if (probe(a) === Array.prototype.push) t++; t;",
        ReadSpecialisation::ProtoPlaceIntrinsic(ReceiverHeader::Array),
    ),
    (
        "string prototype",
        "function probe(o) { return o.slice; } var t = 0; for (var i = 0; i < 64; i++) if (probe('abc') === String.prototype.slice) t++; t;",
        ReadSpecialisation::StringPrototypePlace,
    ),
];

/// The P bodies of the function named `name`: its own and any clone of it,
/// such as an exact-helper-context body.
fn bodies_of(artifact: &WasmArtifact, name: &str) -> Vec<SiteBody> {
    let prefix = format!("js::{name}#");
    let ids = artifact
        .function_sizes
        .iter()
        .filter_map(|body| body.name.strip_prefix(&prefix))
        .map(|id| SiteBody::Source(id.to_owned()))
        .collect::<Vec<_>>();
    assert!(!ids.is_empty(), "a body named {name}");
    ids
}

/// A true profile of each hand-written fixture specialises its probe site,
/// keeps the site map and validates; so does an adversarial one. An empty
/// profile changes no byte.
#[test]
fn true_kind_fixtures_specialise_their_probe_site() {
    for (name, source, kind) in TRUE_KIND_FIXTURES {
        let program = program(source).unwrap();
        let unprofiled = emit_with(&program, ProfileMode::None).unwrap();
        let site_map = unprofiled.site_map();
        let probe = bodies_of(&unprofiled, "probe");
        let probe_reads = site_map
            .read_sites()
            .filter(|site| probe.contains(site.body()))
            .count();
        assert_eq!(
            probe_reads,
            probe.len(),
            "{name}: each probe body has one read site"
        );

        let true_kind = profile(site_map, |_, site| {
            probe.contains(site.body()).then_some(*kind)
        });
        assert_eq!(true_kind.specialised_reads(), probe.len());
        let specialised = emit_with(&program, ProfileMode::Apply(&true_kind)).unwrap();
        validate(&specialised, name);
        assert_eq!(specialised.site_map(), site_map, "{name}");
        assert_eq!(specialised.site_map().digest(), site_map.digest(), "{name}");
        if matches!(
            kind,
            ReadSpecialisation::OwnCompact(_) | ReadSpecialisation::AbsentClosedTail(_)
        ) {
            // The probe's own inline hit already proves own compact slots and closed-tail absence.
            assert_eq!(specialised.bytes, unprofiled.bytes, "{name}");
        } else {
            assert!(specialised.bytes.len() > unprofiled.bytes.len(), "{name}");
        }

        for seed in 0..2 {
            let wrong =
                emit_with(&program, ProfileMode::Apply(&adversarial(site_map, seed))).unwrap();
            validate(&wrong, name);
            assert_eq!(wrong.site_map(), site_map, "{name}");
        }

        let unchanged = emit_with(&program, ProfileMode::Apply(&empty(site_map))).unwrap();
        assert_eq!(
            unchanged.bytes, unprofiled.bytes,
            "{name}: an empty profile changes P"
        );
    }
}

/// A profile trained on another build's site map is rejected, never
/// applied silently.
#[test]
fn mismatched_profiles_are_rejected() {
    let first = program(TRUE_KIND_FIXTURES[0].1).unwrap();
    let second = program(TRUE_KIND_FIXTURES[5].1).unwrap();
    let trained = emit_with(&first, ProfileMode::None).unwrap();
    assert_ne!(
        trained.site_map(),
        emit_with(&second, ProfileMode::None).unwrap().site_map()
    );
    let error = scriptw_aot_wasm::emit_with_intl_profile_and_runtime_inputs(
        &second,
        PromiseRejectionPolicy::default(),
        &scriptw_intl::IntlCompilationProfile::default(),
        RuntimeArtifactInputs::new(None),
        ProfileMode::Apply(&empty(trained.site_map())),
    )
    .unwrap_err();
    assert_eq!(
        error.profile_error(),
        Some(&scriptw_aot_wasm::ProfileError::Mismatch(
            scriptw_aot_wasm::ProfileMismatch::SiteMap
        )),
        "{error}"
    );
    let foreign = ExecutionProfile::new(
        ProgramKey::new([1; 32]),
        trained.site_map(),
        "synthetic",
        NonZeroU32::MIN,
    );
    assert_eq!(
        AdmittedProfile::admit(&foreign, &PROGRAM).unwrap_err(),
        scriptw_aot_wasm::ProfileError::Mismatch(scriptw_aot_wasm::ProfileMismatch::Program)
    );
}

const COLLECT: ProfileMode<'static> = ProfileMode::Collect {
    program: PROGRAM,
    compiler: "scriptw collect test",
};

/// What a Collect-mode P adds, read back from its bytes.
struct Collection {
    section: Vec<u8>,
    /// The profile memory's size in bytes.
    memory_bytes: usize,
    /// Counter increments outside the dump: `i64.load; i64.const 1;
    /// i64.add; i64.store` at one offset of the profile memory.
    increments: usize,
    /// Accesses of the profile memory outside the dump that are no
    /// increment.
    stray_accesses: usize,
}

fn collection(artifact: &WasmArtifact) -> Collection {
    use wasmparser::{ExternalKind, Operator, Payload, TypeRef};
    let mut imported_functions = 0;
    let mut memories = 0;
    let mut profile_memory = None;
    let mut dump = None;
    let mut section = None;
    let mut sizes = Vec::new();
    let mut bodies = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(&artifact.bytes) {
        match payload.unwrap() {
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    match import.unwrap().ty {
                        TypeRef::Func(_) | TypeRef::FuncExact(_) => imported_functions += 1,
                        TypeRef::Memory(memory) => {
                            sizes.push(memory.initial);
                            memories += 1;
                        }
                        _ => {}
                    }
                }
            }
            Payload::MemorySection(reader) => {
                for memory in reader {
                    sizes.push(memory.unwrap().initial);
                    memories += 1;
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.unwrap();
                    match (export.name, export.kind) {
                        (scriptw_aot_wasm::PROFILE_MEMORY_EXPORT, ExternalKind::Memory) => {
                            profile_memory = Some(export.index)
                        }
                        (scriptw_aot_wasm::PROFILE_DUMP_EXPORT, ExternalKind::Func) => {
                            dump = Some(export.index)
                        }
                        _ => {}
                    }
                }
            }
            Payload::CustomSection(reader)
                if reader.name() == scriptw_aot_wasm::PROFILE_SITES_SECTION =>
            {
                assert!(section.is_none(), "one profile sites section");
                section = Some(reader.data().to_vec());
            }
            Payload::CodeSectionEntry(body) => bodies.push(body),
            _ => {}
        }
    }
    let memory = profile_memory.expect("the profile memory is exported");
    assert_eq!(
        memory + 1,
        memories,
        "the profile memory is the last memory"
    );
    let dump = dump.expect("the profile dump is exported");
    assert_eq!(
        dump as usize,
        imported_functions + bodies.len() - 1,
        "the dump is the last function"
    );
    let (mut increments, mut stray_accesses) = (0, 0);
    for body in &bodies[..bodies.len() - 1] {
        let operators = crate::linked_bodies::emitted_operators(&body);
        for (at, operator) in operators.iter().enumerate() {
            let on_profile = |operator: &Operator| match operator {
                Operator::I64Load { memarg } | Operator::I64Store { memarg } => {
                    (memarg.memory == memory).then_some(memarg.offset)
                }
                _ => None,
            };
            let Some(offset) = on_profile(operator) else {
                continue;
            };
            if matches!(operator, Operator::I64Store { .. }) {
                continue;
            }
            let increment = matches!(
                operators.get(at + 1..at + 3),
                Some([Operator::I64Const { value: 1 }, Operator::I64Add])
            ) && operators.get(at + 3).and_then(|store| on_profile(store))
                == Some(offset)
                && matches!(operators[at + 3], Operator::I64Store { .. });
            if increment {
                increments += 1;
            } else {
                stray_accesses += 1;
            }
        }
    }
    Collection {
        section: section.expect("the profile sites section is present"),
        memory_bytes: sizes[memory as usize] as usize * 65_536,
        increments,
        stray_accesses,
    }
}

/// A Collect-mode P validates, keeps the unprofiled site map, counts every
/// site and body entry in its own memory, and describes that memory in a
/// section that decodes into a profile of every site. An untrained memory
/// decodes into a profile with nothing hot.
fn check_collection(name: &str, program: &ProgramIr, unprofiled: &WasmArtifact) -> usize {
    let collected = emit_with(program, COLLECT).unwrap_or_else(|error| panic!("{name}: {error}"));
    validate(&collected, name);
    assert_eq!(collected.site_map(), unprofiled.site_map(), "{name}");
    assert_ne!(collected.bytes, unprofiled.bytes, "{name}");
    let collection = collection(&collected);
    let site_map = unprofiled.site_map();
    assert_eq!(collection.stray_accesses, 0, "{name}");
    // One increment per read site and at least one per body; a planned
    // write site whose record the emission dropped counts nothing.
    assert!(
        collection.increments > site_map.read_sites().count(),
        "{name}: {} increments",
        collection.increments
    );
    let memory = vec![0; collection.memory_bytes];
    let profile = ExecutionProfile::from_collection(&collection.section, &memory, NonZeroU32::MIN)
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    assert_eq!(profile.program(), &PROGRAM, "{name}");
    assert_eq!(profile.site_map_digest(), &site_map.digest(), "{name}");
    assert_eq!(
        profile.reads().count(),
        site_map.read_sites().count(),
        "{name}"
    );
    assert_eq!(
        profile.reads().count() + profile.writes().count(),
        site_map.sites().len(),
        "{name}"
    );
    assert!(profile.bodies().count() >= 1, "{name}");
    assert_eq!(
        AdmittedProfile::admit(&profile, &PROGRAM)
            .unwrap()
            .specialised_reads(),
        0,
        "{name}"
    );
    collected.bytes.len()
}

#[test]
fn collect_mode_instruments_without_changing_the_site_map() {
    for (name, source, _) in TRUE_KIND_FIXTURES {
        let program = program(source).unwrap();
        let unprofiled = emit_with(&program, ProfileMode::None).unwrap();
        check_collection(name, &program, &unprofiled);
    }
}

/// What one fixture of the corpus showed under every synthetic profile.
#[derive(Default)]
struct CorpusCounts {
    fixtures: usize,
    emitted: usize,
    read_sites: usize,
    profiled_emissions: usize,
    collected: usize,
}

#[test]
fn corpus_keeps_site_maps_and_validates_under_every_profile() {
    if std::env::var_os("SCRIPTW_PGO_CORPUS").is_none() {
        return;
    }
    let results = over_fixtures(|name, source| {
        let mut counts = CorpusCounts {
            fixtures: 1,
            ..CorpusCounts::default()
        };
        let Some(program) = program(source) else {
            return counts;
        };
        let Ok(unprofiled) = emit_with(&program, ProfileMode::None) else {
            return counts;
        };
        counts.emitted = 1;
        check_collection(name, &program, &unprofiled);
        counts.collected = 1;
        let site_map = unprofiled.site_map();
        counts.read_sites = site_map.read_sites().count();
        let unchanged = emit_with(&program, ProfileMode::Apply(&empty(site_map)))
            .unwrap_or_else(|error| panic!("{name}: empty profile: {error}"));
        assert_eq!(
            unchanged.bytes, unprofiled.bytes,
            "{name}: an empty profile changes P"
        );
        if counts.read_sites == 0 {
            return counts;
        }
        let profiles = (0..2)
            .map(|seed| (format!("adversarial {seed}"), adversarial(site_map, seed)))
            .chain(
                ReadSpecialisation::ALL
                    .into_iter()
                    .map(|kind| (format!("{kind:?}"), uniform(site_map, kind))),
            );
        for (what, profile) in profiles {
            let profiled = emit_with(&program, ProfileMode::Apply(&profile))
                .unwrap_or_else(|error| panic!("{name}: {what}: {error}"));
            validate(&profiled, &format!("{name}: {what}"));
            assert_eq!(profiled.site_map(), site_map, "{name}: {what}");
            counts.profiled_emissions += 1;
        }
        counts
    });
    let total = results
        .into_iter()
        .fold(CorpusCounts::default(), |total, counts| CorpusCounts {
            fixtures: total.fixtures + counts.fixtures,
            emitted: total.emitted + counts.emitted,
            read_sites: total.read_sites + counts.read_sites,
            profiled_emissions: total.profiled_emissions + counts.profiled_emissions,
            collected: total.collected + counts.collected,
        });
    eprintln!(
        "pgo corpus: {} fixtures, {} emitted, {} read sites, {} profiled emissions validated, {} Collect-mode P validated and decoded",
        total.fixtures, total.emitted, total.read_sites, total.profiled_emissions, total.collected
    );
}

/// The specialisations in the mix the retained RSC request showed at its
/// R read entry: absence before a closed tail, prototype places on compact
/// receivers, intrinsic prototype places of functions and WeakMaps, String
/// methods and own inline slots.
const RSC_MIX: [ReadSpecialisation; 10] = [
    ReadSpecialisation::AbsentClosedTail(CompactStorageLayout::Inline),
    ReadSpecialisation::ProtoPlaceIntrinsic(ReceiverHeader::WeakMap),
    ReadSpecialisation::ProtoPlaceCompact(CompactStorageLayout::Inline, PlaceHolder::Compact),
    ReadSpecialisation::AbsentClosedTail(CompactStorageLayout::Inline),
    ReadSpecialisation::ProtoPlaceIntrinsic(ReceiverHeader::Function),
    ReadSpecialisation::StringPrototypePlace,
    ReadSpecialisation::AbsentClosedTail(CompactStorageLayout::Inline),
    ReadSpecialisation::ProtoPlaceIntrinsic(ReceiverHeader::WeakMap),
    ReadSpecialisation::ProtoPlaceCompact(CompactStorageLayout::Inline, PlaceHolder::Dictionary),
    ReadSpecialisation::OwnCompact(CompactStorageLayout::Inline),
];

#[test]
fn report_profiled_program_size() {
    let Some(path) = std::env::var_os("SCRIPTW_PGO_SIZE_SOURCE") else {
        return;
    };
    let source = fs::read_to_string(path).unwrap();
    let report = std::thread::Builder::new()
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || {
            let program = program(&source).expect("size source parses");
            let unprofiled = emit_with(&program, ProfileMode::None).unwrap();
            let site_map = unprofiled.site_map();
            let reads = site_map.read_sites().collect::<Vec<_>>();
            // Without counts, the hottest-looking sites are those of the
            // bodies with the most read sites.
            let mut per_body = std::collections::HashMap::<&SiteBody, usize>::new();
            for site in &reads {
                *per_body.entry(site.body()).or_default() += 1;
            }
            let mut ranked = reads.clone();
            ranked.sort_by_key(|site| {
                (
                    std::cmp::Reverse(per_body[site.body()]),
                    site.body().clone(),
                    site.ordinal(),
                )
            });
            let base = unprofiled.bytes.len();
            let mut lines = vec![format!(
                "unprofiled P {base} bytes, {} read sites in {} bodies",
                reads.len(),
                per_body.len()
            )];
            let mut measure = |what: String, profile: AdmittedProfile| {
                let profiled = emit_with(&program, ProfileMode::Apply(&profile)).unwrap();
                validate(&profiled, &what);
                let growth = profiled.bytes.len() - base;
                lines.push(format!(
                    "{what}: {} sites, P {} bytes, +{growth} (+{:.2}%), {:.0} bytes/site",
                    profile.specialised_reads(),
                    profiled.bytes.len(),
                    growth as f64 * 100.0 / base as f64,
                    growth as f64 / profile.specialised_reads().max(1) as f64
                ));
            };
            for hot in [25, 50, 100, 200] {
                let chosen = ranked
                    .iter()
                    .take(hot)
                    .map(|site| (*site).clone())
                    .collect::<std::collections::HashSet<_>>();
                let mut next = 0;
                measure(
                    format!("RSC mix at the {hot} hottest-looking sites"),
                    profile(site_map, |_, site| {
                        chosen.contains(site).then(|| {
                            next += 1;
                            RSC_MIX[(next - 1) % RSC_MIX.len()]
                        })
                    }),
                );
            }
            for kind in ReadSpecialisation::ALL {
                measure(format!("{kind:?} at every site"), uniform(site_map, kind));
            }
            let collected = check_collection("size source", &program, &unprofiled);
            let growth = collected - base;
            lines.push(format!(
                "Collect mode: P {collected} bytes, +{growth} (+{:.2}%), {} sites",
                growth as f64 * 100.0 / base as f64,
                site_map.sites().len()
            ));
            lines.join("\n")
        })
        .unwrap()
        .join()
        .unwrap();
    eprintln!("{report}");
    if let Some(output) = std::env::var_os("SCRIPTW_PGO_SIZE_OUT") {
        fs::write(output, report + "\n").unwrap();
    }
}
