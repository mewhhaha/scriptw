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
/// Array receiver never reaches the probe's site record: it takes the
/// general read its Array heap shape plans.
const TRUE_KIND_FIXTURES: &[(&str, &str, Option<ReadSpecialisation>, u64)] = &[
    (
        "own inline",
        "function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) t += probe({ x: i }); t;",
        Some(ReadSpecialisation::OwnCompact(CompactStorageLayout::Inline)),
        64,
    ),
    (
        "own array",
        "function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) t += probe({ a: 1, b: 2, c: 3, d: 4, x: i }); t;",
        Some(ReadSpecialisation::OwnCompact(CompactStorageLayout::Array)),
        64,
    ),
    (
        "absent inline",
        "function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) if (probe({ y: i }) === undefined) t++; t;",
        Some(ReadSpecialisation::AbsentClosedTail(
            CompactStorageLayout::Inline,
        )),
        64,
    ),
    (
        "absent array",
        "function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) if (probe({ a: 1, b: 2, c: 3, d: 4, e: i }) === undefined) t++; t;",
        Some(ReadSpecialisation::AbsentClosedTail(
            CompactStorageLayout::Array,
        )),
        64,
    ),
    (
        "prototype place",
        "function C() {} C.prototype.x = 3; function probe(o) { return o.x; } var t = 0; for (var i = 0; i < 64; i++) t += probe(new C()); t;",
        Some(ReadSpecialisation::ProtoPlaceCompact(
            CompactStorageLayout::Array,
            PlaceHolder::Dictionary,
        )),
        64,
    ),
    (
        "object prototype",
        "var m = Object.prototype.hasOwnProperty; function probe(o) { return o.hasOwnProperty; } var t = 0; for (var i = 0; i < 64; i++) if (probe({ y: i }) === m) t++; t;",
        Some(ReadSpecialisation::ProtoPlaceCompact(
            CompactStorageLayout::Inline,
            PlaceHolder::Intrinsic,
        )),
        64,
    ),
    (
        "function prototype",
        "var m = Function.prototype.call; function g() {} function probe(o) { return o.call; } var t = 0; for (var i = 0; i < 64; i++) if (probe(g) === m) t++; t;",
        None,
        64,
    ),
    (
        "weak map prototype",
        "var m = WeakMap.prototype.get; var w = new WeakMap(); function probe(o) { return o.get; } var t = 0; for (var i = 0; i < 64; i++) if (probe(w) === m) t++; t;",
        Some(ReadSpecialisation::ProtoPlaceIntrinsic(
            ReceiverHeader::WeakMap,
        )),
        64,
    ),
    (
        "array prototype",
        "var m = Array.prototype.push; var a = [1, 2]; function probe(o) { return o.push; } var t = 0; for (var i = 0; i < 64; i++) if (probe(a) === m) t++; t;",
        None,
        0,
    ),
    (
        "string prototype",
        "var m = String.prototype.slice; function probe(o) { return o.slice; } var t = 0; for (var i = 0; i < 64; i++) if (probe('abc') === m) t++; t;",
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
    let runtime = EmbeddedRuntime::compile(fuel_engine(), artifact).unwrap();
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
    // The probe's own inline hit already proves own compact slots and
    // closed-tail absence of either layout, so those kinds are not guarded again.
    let hot = reads
        .iter()
        .filter(|read| {
            read.executions >= 8
                && read.dominant.is_some_and(|kind| {
                    !matches!(
                        kind,
                        ReadSpecialisation::OwnCompact(_)
                            | ReadSpecialisation::AbsentClosedTail(_)
                    )
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
            .unwrap()
            .expect("the fixture compiles");
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
        assert_eq!(executed.is_empty(), *probe_reads == 0, "{name}");
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
    assert_eq!(
        error.execution_profile_error(),
        Some(&ProfileError::NotCollecting),
        "{error}"
    );

    // A precompiled image keeps no raw P, so it never holds a Collect build.
    let collecting = compile(own, ExecutionProfileUse::Collect).unwrap();
    let error = EmbeddedRuntime::precompile(&shared_wasm_engine().unwrap(), &collecting)
        .err()
        .expect("a Collect build is not precompiled");
    assert_eq!(
        error.execution_profile_error(),
        Some(&ProfileError::CollectingNotPrecompilable),
        "{error}"
    );

    let (_, trained) = train(own);
    let error = compile(other, ExecutionProfileUse::Apply(Arc::new(trained))).unwrap_err();
    assert_eq!(
        error.execution_profile_error(),
        Some(&ProfileError::Mismatch(ProfileMismatch::Program)),
        "a foreign profile: {error}"
    );
}

/// Exactness (design section 5): a read site trained on one receiver shape
/// keeps every answer once the receiver, its prototype chain or a Realm
/// changes. Each fixture defines `train`, which reads through `probe` 64
/// times, and `mutate`, which changes what the read must see and reads
/// again. A Collect build is trained on `train` alone, so its profile is
/// the one the mutation invalidates; that true profile must prove its kind
/// (less fuel for `train` than the unprofiled build) and then answer
/// exactly as the unprofiled, Collect and adversarial builds do. The same
/// sources also run as Scripts through the engine's own run path.
const EXACTNESS_FIXTURES: &[(&str, &str)] = &[
    (
        "Proxy receiver",
        r#"
function probe(o) { return o.x; }
function train() { var t = 0; for (var i = 0; i < 64; i++) t += probe({ x: i }); return t; }
function mutate() {
  var out = [probe({ x: 'own' })];
  out.push(probe(new Proxy({ x: 1 }, { get: function (t, k) { return 'trap:' + String(k); } })));
  out.push(probe(new Proxy({ x: 'target' }, {})));
  return out.join('|');
}
"#,
    ),
    (
        "getter added to the prototype",
        r#"
function C() {}
C.prototype.x = 'data';
function probe(o) { return o.x; }
function train() { var t = ''; for (var i = 0; i < 64; i++) t = probe(new C()); return t; }
function mutate() {
  var out = [probe(new C())];
  Object.defineProperty(C.prototype, 'x', { get: function () { return 'getter:' + (this instanceof C); }, configurable: true });
  out.push(probe(new C()));
  Object.defineProperty(C.prototype, 'x', { value: 'redefined', writable: false });
  out.push(probe(new C()));
  return out.join('|');
}
"#,
    ),
    (
        "setPrototypeOf",
        r#"
function C() {}
C.prototype.x = 'class';
function probe(o) { return o.x; }
function train() { var t = ''; for (var i = 0; i < 64; i++) t = probe(new C()); return t; }
function mutate() {
  var o = new C();
  Object.setPrototypeOf(o, { x: 'other' });
  var out = [probe(o)];
  Object.setPrototypeOf(o, null);
  out.push(String(probe(o)));
  Object.setPrototypeOf(C.prototype, { x: 'grand' });
  delete C.prototype.x;
  out.push(probe(new C()));
  return out.join('|');
}
"#,
    ),
    (
        "delete",
        r#"
function probe(o) { return o.x; }
function train() { var t = 0; for (var i = 0; i < 64; i++) t += probe({ x: i }); return t; }
function mutate() {
  var o = { x: 1 };
  delete o.x;
  var out = [String(probe(o))];
  var p = { x: 2, y: 3 };
  delete p.y;
  out.push(probe(p));
  var q = { y: 1, x: 'after' };
  delete q.y;
  out.push(probe(q));
  return out.join('|');
}
"#,
    ),
    (
        "defineProperty non-writable, accessor and non-enumerable",
        r#"
function probe(o) { return o.x; }
function train() { var t = 0; for (var i = 0; i < 64; i++) t += probe({ x: i }); return t; }
function mutate() {
  var a = { x: 1 };
  Object.defineProperty(a, 'x', { value: 'read-only', writable: false });
  var out = [probe(a)];
  var b = { x: 1 };
  Object.defineProperty(b, 'x', { get: function () { return 'accessor'; } });
  out.push(probe(b));
  var c = { x: 'hidden' };
  Object.defineProperty(c, 'x', { enumerable: false });
  out.push(probe(c));
  return out.join('|');
}
"#,
    ),
    (
        "freeze, seal and preventExtensions",
        r#"
function probe(o) { return o.x; }
function train() { var t = 0; for (var i = 0; i < 64; i++) t += probe({ x: i }); return t; }
function mutate() {
  var out = [probe(Object.freeze({ x: 'frozen' })), probe(Object.seal({ x: 'sealed' }))];
  var e = { x: 'non-extensible' };
  Object.preventExtensions(e);
  out.push(probe(e));
  return out.join('|');
}
"#,
    ),
    (
        "key added to Object.prototype",
        r#"
function probe(o) { return o.x; }
function train() { var t = 0; for (var i = 0; i < 64; i++) if (probe({ y: i }) === undefined) t++; return t; }
function mutate() {
  Object.prototype.x = 'inherited';
  var out = [probe({ y: 1 })];
  Object.defineProperty(Object.prototype, 'x', { get: function () { return 'getter'; }, configurable: true });
  out.push(probe({ y: 2 }));
  delete Object.prototype.x;
  out.push(String(probe({ y: 3 })));
  return out.join('|');
}
"#,
    ),
    (
        "Object.prototype method replaced",
        r#"
var original = Object.prototype.hasOwnProperty;
function probe(o) { return o.hasOwnProperty; }
function train() { var t = 0; for (var i = 0; i < 64; i++) if (probe({ y: i }) === original) t++; return t; }
function mutate() {
  Object.prototype.hasOwnProperty = function () { return 'patched'; };
  var out = [probe({ y: 1 })()];
  out.push(probe({ y: 1, hasOwnProperty: function () { return 'own'; } })());
  Object.prototype.hasOwnProperty = original;
  out.push(String(probe({ y: 1 }) === original));
  return out.join('|');
}
"#,
    ),
    (
        "String.prototype edit",
        r#"
var original = String.prototype.slice;
function probe(s) { return s.slice; }
function train() { var t = 0; for (var i = 0; i < 64; i++) if (probe('abc') === original) t++; return t; }
function mutate() {
  String.prototype.slice = function () { return 'patched:' + this; };
  var out = [probe('abc').call('abc')];
  Object.defineProperty(String.prototype, 'slice', { get: function () { return function () { return 'getter'; }; }, configurable: true });
  out.push(probe('abc')());
  delete String.prototype.slice;
  out.push(String(probe('abc')));
  return out.join('|');
}
"#,
    ),
];

/// The other-Realm fixtures need the Test262 host's `__scriptwCreateRealm`.
const OTHER_REALM_FIXTURES: &[(&str, &str)] = &[
    (
        "other-Realm ordinary receiver",
        r#"
var original = Object.prototype.hasOwnProperty;
function probe(o) { return o.hasOwnProperty; }
function train() { var t = 0; for (var i = 0; i < 64; i++) if (probe({ y: i }) === original) t++; return t; }
function mutate() {
  var other = __scriptwCreateRealm().global;
  var foreign = new other.Object();
  var out = [probe(foreign) === other.Object.prototype.hasOwnProperty, probe(foreign) === original];
  other.Object.prototype.hasOwnProperty = function () { return 'other realm'; };
  out.push(probe(foreign)());
  out.push(probe({ y: 1 }) === original);
  return out.join('|');
}
"#,
    ),
    (
        "other-Realm WeakMap",
        r#"
var w = new WeakMap();
var original = WeakMap.prototype.get;
function probe(o) { return o.get; }
function train() { var t = 0; for (var i = 0; i < 64; i++) if (probe(w) === original) t++; return t; }
function mutate() {
  var other = __scriptwCreateRealm().global;
  var foreign = new other.WeakMap();
  var out = [probe(foreign) === other.WeakMap.prototype.get, probe(foreign) === original];
  other.WeakMap.prototype.get = function () { return 'other realm'; };
  out.push(probe(foreign)());
  out.push(probe(w) === original);
  return out.join('|');
}
"#,
    ),
    (
        "String.prototype of another Realm",
        r#"
var original = String.prototype.slice;
function probe(s) { return s.slice; }
function train() { var t = 0; for (var i = 0; i < 64; i++) if (probe('abc') === original) t++; return t; }
function mutate() {
  var other = __scriptwCreateRealm().global;
  other.String.prototype.slice = function () { return 'other realm'; };
  var out = [probe('abc') === original, probe('abc').call('abc', 1)];
  out.push(probe(new other.String('xyz')).call('xyz', 1));
  return out.join('|');
}
"#,
    ),
];

fn exactness_options(
    surface: HostSurfacePolicy,
    execution_profile: ExecutionProfileUse,
) -> CompileOptions {
    CompileOptions {
        host_surface_policy: surface,
        execution_profile,
        ..CompileOptions::default()
    }
}

fn exactness_module(
    source: &str,
    surface: HostSurfacePolicy,
    execution_profile: ExecutionProfileUse,
) -> Artifact {
    configure_compilation_jobs(1).unwrap();
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_module(source, exactness_options(surface, execution_profile))
        .unwrap_or_else(|error| panic!("{error}"));
    compiler.emit_wasm(&unit).unwrap()
}

/// What `train` and `mutate` return and print, the fuel `train` spent, and
/// the profile a Collect build recorded during `train` alone.
fn run_exactness(artifact: &Artifact) -> (String, u64, Option<ExecutionProfile>) {
    let runtime = EmbeddedRuntime::compile(fuel_engine(), artifact).unwrap();
    let mut store = store(&runtime, 0.0);
    store.set_fuel(u64::MAX / 2).unwrap();
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert!(matches!(
        instance.initialize(&mut store).unwrap(),
        EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
    ));
    let call = |store: &mut WasmtimeStore<WasmStoreState>, name: &str| {
        let function = match instance.get_export(store, name).unwrap() {
            JsCompletion::Normal(function) => function,
            JsCompletion::Throw(error) => panic!("export {name} threw {error:?}"),
        };
        match instance.call(store, &function, None, &[]).unwrap() {
            JsCompletion::Normal(value) => format!("{name} {:?}", value.observe(store).unwrap()),
            JsCompletion::Throw(value) => {
                format!("{name} threw {:?}", value.observe(store).unwrap())
            }
        }
    };
    let before = store.get_fuel().unwrap();
    let trained = call(&mut store, "train");
    let train_fuel = before - store.get_fuel().unwrap();
    let profile = runtime.profile_sites.is_some().then(|| {
        instance
            .collect_profile(&mut store, NonZeroU32::MIN)
            .unwrap()
    });
    let mutated = call(&mut store, "mutate");
    let output = instance.take_output(&mut store).unwrap();
    (
        format!("{trained} {mutated} {output:?}"),
        train_fuel,
        profile,
    )
}

fn fuel_engine() -> &'static WasmtimeEngine {
    static ENGINE: std::sync::OnceLock<WasmtimeEngine> = std::sync::OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = WasmtimeConfig::new();
        configure_wasm_runtime(
            &mut config,
            NativeCompilationPolicy::Fast,
            crate::CompilationJobs::current(),
        );
        config.consume_fuel(true);
        WasmtimeEngine::new(&config).unwrap()
    })
}

fn module_probe_bodies(source: &str, surface: HostSurfacePolicy) -> Vec<SiteBody> {
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_module(
            source,
            exactness_options(surface, ExecutionProfileUse::None),
        )
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
    artifact
        .function_sizes
        .iter()
        .filter_map(|body| body.name.strip_prefix("js::probe#"))
        .map(|id| SiteBody::Source(id.to_owned()))
        .collect()
}

/// Checks one exactness fixture and returns what went wrong.
fn check_exactness(name: &str, body: &str, surface: HostSurfacePolicy) -> Vec<String> {
    let mut problems = Vec::new();
    let module = format!("{body}\nexport {{ train, mutate }};\n");
    let (expected, unprofiled_fuel, _) = run_exactness(&exactness_module(
        &module,
        surface,
        ExecutionProfileUse::None,
    ));
    if expected.contains("threw") {
        problems.push(format!("{name}: the unprofiled build throws: {expected}"));
    }
    let (collected, _, profile) = run_exactness(&exactness_module(
        &module,
        surface,
        ExecutionProfileUse::Collect,
    ));
    if collected != expected {
        problems.push(format!("{name}: Collect {collected} against {expected}"));
    }
    let profile = profile.expect("a Collect build records");
    let probe = module_probe_bodies(&module, surface);
    let trained = profile
        .reads()
        .filter(|(site, read)| probe.contains(site.body()) && read.executions > 0)
        .map(|(_, read)| *read)
        .collect::<Vec<_>>();
    let kind = match trained.as_slice() {
        [read] if read.executions == 64 && read.dominant.is_some() => read.dominant,
        _ => {
            problems.push(format!(
                "{name}: probe site not trained to one kind: {trained:?}"
            ));
            None
        }
    };
    let (applied, applied_fuel, _) = run_exactness(&exactness_module(
        &module,
        surface,
        ExecutionProfileUse::Apply(Arc::new(profile)),
    ));
    if applied != expected {
        problems.push(format!(
            "{name}: true profile ({kind:?}) {applied} against {expected}"
        ));
    }
    if !matches!(
        kind,
        Some(ReadSpecialisation::OwnCompact(_) | ReadSpecialisation::AbsentClosedTail(_))
    ) && applied_fuel >= unprofiled_fuel
    {
        problems.push(format!(
            "{name}: the trained {kind:?} guard does not hit before the mutation ({applied_fuel} fuel against {unprofiled_fuel})"
        ));
    }
    for seed in 0..3 {
        let wrong = Engine::new(RealmBuilder::new().build())
            .adversarial_execution_profile(
                &module,
                EmbeddedModuleGoal::Module,
                &exactness_options(surface, ExecutionProfileUse::None),
                seed,
            )
            .unwrap()
            .expect("the fixture compiles");
        let (wrongly, _, _) = run_exactness(&exactness_module(
            &module,
            surface,
            ExecutionProfileUse::Apply(Arc::new(wrong)),
        ));
        if wrongly != expected {
            problems.push(format!(
                "{name}: adversarial {seed} {wrongly} against {expected}"
            ));
        }
    }

    // The engine's own run path, as a Script.
    let script = format!(
        "{body}\nvar trained = train();\nvar mutated = mutate();\nprint(trained + ' ' + mutated);\nmutated;\n"
    );
    let observe = |execution_profile| {
        let outcome = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &script,
                exactness_options(surface, execution_profile),
                RunOptions::default(),
            )
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        format!("{:?} {:?}", outcome.completion, outcome.output_events)
    };
    let expected = observe(ExecutionProfileUse::None);
    let mut profiles = vec![("Collect".to_owned(), ExecutionProfileUse::Collect)];
    for seed in 0..3 {
        let wrong = Engine::new(RealmBuilder::new().build())
            .adversarial_execution_profile(
                &script,
                EmbeddedModuleGoal::Script,
                &exactness_options(surface, ExecutionProfileUse::None),
                seed,
            )
            .unwrap()
            .expect("the fixture compiles");
        profiles.push((
            format!("adversarial {seed}"),
            ExecutionProfileUse::Apply(Arc::new(wrong)),
        ));
    }
    for (what, profile) in profiles {
        let observed = observe(profile);
        if observed != expected {
            problems.push(format!(
                "{name}: run path {what} {observed} against {expected}"
            ));
        }
    }
    problems
}

#[test]
fn trained_sites_keep_every_answer_after_the_receiver_or_its_chain_changes() {
    let problems = EXACTNESS_FIXTURES
        .iter()
        .flat_map(|(name, body)| check_exactness(name, body, HostSurfacePolicy::Product))
        .collect::<Vec<_>>();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn trained_sites_keep_every_answer_for_receivers_of_another_realm() {
    let problems = OTHER_REALM_FIXTURES
        .iter()
        .flat_map(|(name, body)| check_exactness(name, body, HostSurfacePolicy::Test262))
        .collect::<Vec<_>>();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn a_profile_of_another_program_is_a_typed_refusal_on_the_run_path() {
    let (own, other) = (EXACTNESS_FIXTURES[3].1, EXACTNESS_FIXTURES[5].1);
    let own = format!("{own}\ntrain();\n");
    let wrong = Engine::new(RealmBuilder::new().build())
        .adversarial_execution_profile(
            &own,
            EmbeddedModuleGoal::Script,
            &CompileOptions::default(),
            0,
        )
        .unwrap()
        .expect("the fixture compiles");
    let error = Engine::new(RealmBuilder::new().build())
        .observe_script(
            &format!("{other}\ntrain();\n"),
            CompileOptions {
                execution_profile: ExecutionProfileUse::Apply(Arc::new(wrong)),
                ..CompileOptions::default()
            },
            RunOptions::default(),
        )
        .unwrap_err();
    assert_eq!(
        error.execution_profile_error(),
        Some(&ProfileError::Mismatch(ProfileMismatch::Program)),
        "{error}"
    );
}
