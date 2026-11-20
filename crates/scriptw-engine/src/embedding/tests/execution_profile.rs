//! Profile-guided compilation executed: Collect-mode counters count, the
//! dump decodes into what the site records learned, collection and every
//! applied profile, true or wrong, leave behaviour unchanged, and a profile
//! survives its codec and admission against the unprofiled program key.
use super::*;
use scriptw_aot_wasm::{
    AdmittedProfile, CompactStorageLayout, PlaceHolder, ReadSpecialisation, ReceiverHeader,
    SiteBody, SiteKind,
};
use std::num::NonZeroU32;

/// The emission crate's hand-written fixtures: one read site in `probe`
/// meets receivers of one kind 64 times. The kind is what a real training
/// run's records prove, which is not always the emission fixture's label:
/// `new C()` has array-layout storage and its prototype a dictionary, and a
/// function receiver's compact own storage is no S1 kind. A read of an
/// Array receiver reaches the probe's site record once, not 64 times.
const TRUE_KIND_FIXTURES: &[(&str, &str, Option<ReadSpecialisation>, u64)] = &[
    (
        "own inline",
        "function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) t += probe({ x: i }); t;",
        Some(ReadSpecialisation::OwnCompact(CompactStorageLayout::Inline)),
        64,
    ),
    (
        "own array",
        "function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) t += probe({ a: 1, b: 2, c: 3, x: i }); t;",
        Some(ReadSpecialisation::OwnCompact(CompactStorageLayout::Array)),
        64,
    ),
    (
        "absent inline",
        "function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) if (probe({ y: i }) === undefined) t++; t;",
        Some(ReadSpecialisation::AbsentClosedTail(CompactStorageLayout::Inline)),
        64,
    ),
    (
        "absent array",
        "function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) if (probe({ a: 1, b: 2, c: i }) === undefined) t++; t;",
        Some(ReadSpecialisation::AbsentClosedTail(CompactStorageLayout::Array)),
        64,
    ),
    (
        "prototype place",
        "function C() {} C.prototype.x = 3; function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) t += probe(new C()); t;",
        Some(ReadSpecialisation::ProtoPlaceCompact(CompactStorageLayout::Array, PlaceHolder::Dictionary)),
        64,
    ),
    (
        "object prototype",
        "function probe(o) { return o.hasOwnProperty; } var t = 0; for (var i = 0; i < 64; i++) if (probe({ y: i }) === Object.prototype.hasOwnProperty) t++; t;",
        Some(ReadSpecialisation::ProtoPlaceCompact(CompactStorageLayout::Inline, PlaceHolder::Intrinsic)),
        64,
    ),
    (
        "function prototype",
        "function g() {} function probe(o) { return o.call; } var t = 0; for (var i = 0; i < 64; i++) if (probe(g) === Function.prototype.call) t++; t;",
        None,
        64,
    ),
    (
        "weak map prototype",
        "var w = new WeakMap(); function probe(o) { return o.get; } var t = 0; for (var i = 0; i < 64; i++) if (probe(w) === WeakMap.prototype.get) t++; t;",
        Some(ReadSpecialisation::ProtoPlaceIntrinsic(ReceiverHeader::WeakMap)),
        64,
    ),
    (
        "array prototype",
        "var a = [1, 2]; function probe(o) { return o.push; } var t = 0; for (var i = 0; i < 64; i++) if (probe(a) === Array.prototype.push) t++; t;",
        Some(ReadSpecialisation::ProtoPlaceIntrinsic(ReceiverHeader::Array)),
        1,
    ),
    (
        "string prototype",
        "function probe(o) { return o.slice; } var t = 0; for (var i = 0; i < 64; i++) if (probe('abc') === String.prototype.slice) t++; t;",
        Some(ReadSpecialisation::StringPrototypePlace),
        64,
    ),
];

/// Two receiver shapes, both owning the key inline: one kind, two records.
const POLYMORPHIC: &str = "function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) t += probe(i % 2 ? { x: i } : { y: 0, x: i }); t;";

/// Thirty-two receiver shapes: more than a site's records can name.
const MEGAMORPHIC: &str = "function probe(o) { return o.x; } var shapes = []; for (var s = 0; s < 32; s++) { var o = {}; o['k' + s] = s; o.x = s; shapes.push(o); } var t = 0; for (var i = 0; i < 64; i++) t += probe(shapes[i % 32]); t;";

fn options(execution_profile: ExecutionProfileUse) -> CompileOptions {
    CompileOptions {
        execution_profile,
        ..CompileOptions::default()
    }
}

fn compile(source: &str, execution_profile: ExecutionProfileUse) -> Result<Artifact, EngineError> {
    configure_compilation_jobs(1).unwrap();
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler.compile_script(source, options(execution_profile))?;
    compiler.emit_wasm(&unit)
}

/// The P bodies of `probe` (its own and any clone) and the unprofiled site
/// map's identity, from the emitter's own names.
fn probe_bodies(source: &str) -> Vec<SiteBody> {
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_script(source, CompileOptions::default())
        .unwrap();
    let artifact = run_on_sized_stack(|| {
        scriptw_aot_wasm::emit_with_intl_profile_and_runtime_inputs(
            &unit.ir,
            unit.promise_rejection_policy,
            &unit.intl_profile,
            scriptw_aot_wasm::RuntimeArtifactInputs::new(None),
            scriptw_aot_wasm::ProfileMode::None,
        )
        .unwrap()
    });
    let bodies = artifact
        .function_sizes
        .iter()
        .filter_map(|body| body.name.strip_prefix("js::probe#"))
        .map(|id| SiteBody::Source(id.to_owned()))
        .collect::<Vec<_>>();
    assert!(!bodies.is_empty(), "a body named probe");
    bodies
}

/// The program's observable behaviour, and the profile it recorded when it
/// was built to collect one.
fn execute(artifact: &Artifact) -> (String, Option<Result<ExecutionProfile, EngineError>>) {
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), artifact).unwrap();
    let mut store = store(&runtime, 0.0);
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    let observed = match instance.initialize(&mut store).unwrap() {
        EmbeddedEvaluation::Settled(JsCompletion::Normal(value)) => {
            format!("normal {:?}", value.observe(&mut store).unwrap())
        }
        EmbeddedEvaluation::Settled(JsCompletion::Throw(value)) => {
            format!("throw {:?}", value.observe(&mut store).unwrap())
        }
        EmbeddedEvaluation::Pending => "pending".into(),
    };
    let output = instance.take_output(&mut store).unwrap();
    let profile = runtime
        .profile_sites
        .is_some()
        .then(|| instance.collect_profile(&mut store, NonZeroU32::MIN));
    (format!("{observed} {output:?}"), profile)
}

/// The Wasm fuel a program's evaluation consumes: a guard that proves its
/// kind skips the R read entry, so a true profile spends less than the
/// unprofiled build and a guard that never proves spends more.
fn fuel(artifact: &Artifact) -> u64 {
    static ENGINE: std::sync::OnceLock<WasmtimeEngine> = std::sync::OnceLock::new();
    let engine = ENGINE.get_or_init(|| {
        let mut config = WasmtimeConfig::new();
        configure_wasm_runtime(
            &mut config,
            NativeCompilationPolicy::Fast,
            crate::CompilationJobs::current(),
        );
        config.consume_fuel(true);
        WasmtimeEngine::new(&config).unwrap()
    });
    let runtime = EmbeddedRuntime::compile(engine, artifact).unwrap();
    let mut store = store(&runtime, 0.0);
    store.set_fuel(u64::MAX / 2).unwrap();
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    let before = store.get_fuel().unwrap();
    assert!(matches!(
        instance.initialize(&mut store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    before - store.get_fuel().unwrap()
}

fn train(source: &str) -> (String, ExecutionProfile) {
    let (observed, profile) = execute(&compile(source, ExecutionProfileUse::Collect).unwrap());
    (observed, profile.expect("a Collect build records").unwrap())
}

fn unprofiled_key(source: &str) -> [u8; 32] {
    program_wasm_cache_key(source, ParseGoal::Script, &CompileOptions::default())
        .expect("the compiler cache identity is available")
}

fn probe_reads(
    profile: &ExecutionProfile,
    probe: &[SiteBody],
) -> Vec<scriptw_aot_wasm::ReadSiteObservation> {
    profile
        .reads()
        .filter(|(site, _)| probe.contains(site.body()))
        .map(|(_, observation)| *observation)
        .collect()
}

/// What training one fixture counted and what its true profile cost.
struct Trained {
    main_entries: u64,
    probe_entries: u64,
    probe_read_executions: u64,
    reads: Vec<scriptw_aot_wasm::ReadSiteObservation>,
    /// Fuel of the true-profile build and of the unprofiled build, when the
    /// profile guards a hot probe site.
    fuel: Option<(u64, u64)>,
}

impl Trained {
    /// Check 1 and the guard's hit: `main` entered once, the probe body and
    /// its read site 64 times, and a guarded probe site spending less fuel.
    fn mismatches(&self, name: &str, probe_reads: u64) -> Vec<String> {
        let mut mismatches = Vec::new();
        if (
            self.main_entries,
            self.probe_entries,
            self.probe_read_executions,
        ) != (1, 64, probe_reads)
        {
            mismatches.push(format!(
                "{name}: main {} probe body {} probe read {} (expected 1, 64, {probe_reads})",
                self.main_entries, self.probe_entries, self.probe_read_executions
            ));
        }
        if let Some((applied, unprofiled)) = self.fuel {
            if applied >= unprofiled {
                mismatches.push(format!(
                    "{name}: the trained guard spends {applied} fuel against {unprofiled} unprofiled"
                ));
            }
        }
        mismatches
    }
}

/// Checks 1 to 5 of the design on one fixture. Behaviour, identity and the
/// codec are asserted here; counts are returned for the caller.
fn check_fixture(name: &str, source: &str) -> Trained {
    let probe = probe_bodies(source);
    let unprofiled = compile(source, ExecutionProfileUse::None).unwrap();
    let (expected, none_profile) = execute(&unprofiled);
    assert!(
        none_profile.is_none(),
        "{name}: an unprofiled build records"
    );
    assert!(expected.starts_with("normal"), "{name}: {expected}");

    // Collection changes no behaviour, and its counters count.
    let (observed, profile) = train(source);
    assert_eq!(observed, expected, "{name}: Collect changes behaviour");
    let entries = |body: &SiteBody| {
        profile
            .bodies()
            .find(|(counted, _)| *counted == body)
            .map_or(0, |(_, entries)| *entries)
    };
    let main_entries = entries(&SiteBody::Main);
    let probe_entries = probe.iter().map(entries).sum::<u64>();
    let reads = probe_reads(&profile, &probe);
    let probe_read_executions = reads.iter().map(|read| read.executions).sum::<u64>();
    assert_eq!(
        profile
            .reads()
            .map(|(site, _)| site.kind())
            .chain(profile.writes().map(|(site, _)| site.kind()))
            .filter(|kind| *kind == SiteKind::PropertyRead)
            .count(),
        profile.reads().count()
    );

    // The profile names the unprofiled program and survives its codec.
    let key = unprofiled_key(source);
    assert_eq!(profile.program().bytes(), &key, "{name}: program key");
    let encoded = profile.encode();
    let decoded = ExecutionProfile::decode(&encoded).unwrap();
    assert_eq!(decoded, profile, "{name}: codec");
    assert_eq!(decoded.encode(), encoded, "{name}: canonical encoding");
    let admitted = AdmittedProfile::admit(&decoded, &scriptw_aot_wasm::ProgramKey::new(key))
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    // The probe's own inline hit already proves an own array slot, so
    // that kind is not guarded again.
    let hot = reads
        .iter()
        .filter(|read| {
            read.executions >= 8
                && read.dominant.is_some_and(|kind| {
                    kind != ReadSpecialisation::OwnCompact(CompactStorageLayout::Array)
                })
        })
        .count();

    // A true profile and two wrong ones change speed only.
    let applied = compile(source, ExecutionProfileUse::Apply(Arc::new(decoded))).unwrap();
    assert!(applied.bytes.len() >= unprofiled.bytes.len(), "{name}");
    if hot > 0 {
        assert!(admitted.specialised_reads() >= hot, "{name}");
        assert!(
            applied.bytes.len() > unprofiled.bytes.len(),
            "{name}: a hot probe site is specialised"
        );
    }
    assert_eq!(execute(&applied).0, expected, "{name}: true profile");
    let fuel = (hot > 0).then(|| (fuel(&applied), fuel(&unprofiled)));
    for seed in 0..2 {
        let wrong = Engine::new(RealmBuilder::new().build())
            .adversarial_execution_profile(
                source,
                EmbeddedModuleGoal::Script,
                &CompileOptions::default(),
                seed,
            )
            .unwrap();
        assert_eq!(wrong.program().bytes(), &key, "{name}");
        let applied = compile(source, ExecutionProfileUse::Apply(Arc::new(wrong))).unwrap();
        assert!(applied.bytes.len() > unprofiled.bytes.len(), "{name}");
        assert_eq!(
            execute(&applied).0,
            expected,
            "{name}: adversarial profile {seed}"
        );
    }
    Trained {
        main_entries,
        probe_entries,
        probe_read_executions,
        reads,
        fuel,
    }
}

#[test]
fn trained_fixtures_count_decode_their_kind_and_behave_identically_under_every_profile() {
    let mut mismatches = Vec::new();
    for (name, source, kind, probe_reads) in TRUE_KIND_FIXTURES {
        let trained = check_fixture(name, source);
        mismatches.extend(trained.mismatches(name, *probe_reads));
        let executed = trained
            .reads
            .iter()
            .filter(|read| read.executions > 0)
            .collect::<Vec<_>>();
        assert!(!executed.is_empty(), "{name}");
        for read in executed {
            if read.dominant != *kind || read.megamorphic || read.records != 1 {
                mismatches.push(format!(
                    "{name}: expected one record of {kind:?}, trained {read:?}"
                ));
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn polymorphic_and_megamorphic_sites_are_flagged() {
    let polymorphic = check_fixture("polymorphic", POLYMORPHIC);
    let mismatches = polymorphic.mismatches("polymorphic", 64);
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    let read = polymorphic
        .reads
        .iter()
        .find(|read| read.executions > 0)
        .expect("the probe read ran");
    assert!(read.records >= 2, "polymorphic: {read:?}");
    assert!(!read.megamorphic, "polymorphic: {read:?}");
    assert_eq!(
        read.dominant,
        Some(ReadSpecialisation::OwnCompact(CompactStorageLayout::Inline)),
        "polymorphic: {read:?}"
    );

    let megamorphic = check_fixture("megamorphic", MEGAMORPHIC);
    let mismatches = megamorphic.mismatches("megamorphic", 64);
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    let read = megamorphic
        .reads
        .iter()
        .find(|read| read.executions > 0)
        .expect("the probe read ran");
    assert!(read.megamorphic, "megamorphic: {read:?}");
    assert_eq!(read.dominant, None, "megamorphic: {read:?}");
}

#[test]
fn profile_choices_have_distinct_program_cache_keys() {
    let source = TRUE_KIND_FIXTURES[0].1;
    let key = |profile| program_wasm_cache_key(source, ParseGoal::Script, &options(profile));
    let none = key(ExecutionProfileUse::None).unwrap();
    let collect = key(ExecutionProfileUse::Collect).unwrap();
    let (_, trained) = train(source);
    let apply = key(ExecutionProfileUse::Apply(Arc::new(trained.clone()))).unwrap();
    let mut retrained = trained;
    retrained.observe_body(&SiteBody::Main, 2);
    let other = key(ExecutionProfileUse::Apply(Arc::new(retrained))).unwrap();
    assert_eq!(
        none,
        unprofiled_key(source),
        "the unprofiled key is unchanged"
    );
    let keys = [none, collect, apply, other];
    for (index, first) in keys.iter().enumerate() {
        for second in &keys[index + 1..] {
            assert_ne!(first, second);
        }
    }
}

#[test]
fn profiles_are_never_silently_ignored() {
    let (own, other) = (TRUE_KIND_FIXTURES[0].1, TRUE_KIND_FIXTURES[5].1);
    let runtime = EmbeddedRuntime::compile(
        &shared_wasm_engine().unwrap(),
        &compile(own, ExecutionProfileUse::None).unwrap(),
    )
    .unwrap();
    let mut store = store(&runtime, 0.0);
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    instance.initialize(&mut store).unwrap();
    let error = instance
        .collect_profile(&mut store, NonZeroU32::MIN)
        .unwrap_err();
    assert!(error.to_string().contains("Collect"), "{error}");

    let (_, trained) = train(own);
    let error = compile(other, ExecutionProfileUse::Apply(Arc::new(trained))).unwrap_err();
    assert_eq!(
        error.execution_profile_error(),
        Some(&ProfileError::Mismatch(ProfileMismatch::Program)),
        "a foreign profile: {error}"
    );
}
