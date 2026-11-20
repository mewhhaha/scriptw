//! Exactness of profile-guided compilation (design section 5): a read site
//! trained on one receiver shape keeps every answer once the receiver, its
//! prototype chain or a Realm changes. Each fixture trains its `probe` read
//! site for 64 iterations, then mutates. The unprofiled build, the
//! Collect-mode build, a true profile naming the trained kind at the probe
//! site, and three adversarial profiles must observe the same completion and
//! output.
use super::*;
use scriptw_aot_wasm::{
    CompactStorageLayout, PlaceHolder, ReadSiteObservation, ReadSpecialisation, ReceiverHeader,
    SiteBody,
};

const PROXY: &str = r#"
function probe(o) { return o.x; }
var out = [];
for (var i = 0; i < 64; i++) probe({ x: i });
out.push(probe({ x: 'own' }));
out.push(probe(new Proxy({ x: 1 }, { get: function (t, k) { return 'trap:' + String(k); } })));
out.push(probe(new Proxy({ x: 'target' }, {})));
print(out.join('|'));
out.join('|');
"#;

const PROTOTYPE_GETTER: &str = r#"
function C() {}
C.prototype.x = 'data';
function probe(o) { return o.x; }
var out = [];
for (var i = 0; i < 64; i++) probe(new C());
out.push(probe(new C()));
Object.defineProperty(C.prototype, 'x', { get: function () { return 'getter:' + (this instanceof C); }, configurable: true });
out.push(probe(new C()));
Object.defineProperty(C.prototype, 'x', { value: 'redefined', writable: false });
out.push(probe(new C()));
print(out.join('|'));
out.join('|');
"#;

const SET_PROTOTYPE_OF: &str = r#"
function C() {}
C.prototype.x = 'class';
function probe(o) { return o.x; }
var out = [];
for (var i = 0; i < 64; i++) probe(new C());
var o = new C();
Object.setPrototypeOf(o, { x: 'other' });
out.push(probe(o));
Object.setPrototypeOf(o, null);
out.push(String(probe(o)));
Object.setPrototypeOf(C.prototype, { x: 'grand' });
delete C.prototype.x;
out.push(probe(new C()));
print(out.join('|'));
out.join('|');
"#;

const DELETE: &str = r#"
function probe(o) { return o.x; }
var out = [];
for (var i = 0; i < 64; i++) probe({ x: i });
var o = { x: 1 };
delete o.x;
out.push(String(probe(o)));
var p = { x: 2, y: 3 };
delete p.y;
out.push(probe(p));
var q = { y: 1, x: 'after' };
delete q.y;
out.push(probe(q));
print(out.join('|'));
out.join('|');
"#;

const DEFINE_PROPERTY: &str = r#"
function probe(o) { return o.x; }
var out = [];
for (var i = 0; i < 64; i++) probe({ x: i });
var a = { x: 1 };
Object.defineProperty(a, 'x', { value: 'read-only', writable: false });
out.push(probe(a));
var b = { x: 1 };
Object.defineProperty(b, 'x', { get: function () { return 'accessor'; } });
out.push(probe(b));
var c = { x: 1 };
Object.defineProperty(c, 'x', { enumerable: false });
out.push(probe(c));
print(out.join('|'));
out.join('|');
"#;

const FREEZE_SEAL: &str = r#"
function probe(o) { return o.x; }
var out = [];
for (var i = 0; i < 64; i++) probe({ x: i });
out.push(probe(Object.freeze({ x: 'frozen' })));
out.push(probe(Object.seal({ x: 'sealed' })));
var e = { x: 'non-extensible' };
Object.preventExtensions(e);
out.push(probe(e));
print(out.join('|'));
out.join('|');
"#;

const OBJECT_PROTOTYPE_KEY: &str = r#"
function probe(o) { return o.x; }
var out = [];
for (var i = 0; i < 64; i++) probe({ y: i });
Object.prototype.x = 'inherited';
out.push(probe({ y: 1 }));
Object.defineProperty(Object.prototype, 'x', { get: function () { return 'getter'; }, configurable: true });
out.push(probe({ y: 2 }));
delete Object.prototype.x;
out.push(String(probe({ y: 3 })));
print(out.join('|'));
out.join('|');
"#;

const OBJECT_PROTOTYPE_METHOD: &str = r#"
function probe(o) { return o.hasOwnProperty; }
var out = [];
var original = Object.prototype.hasOwnProperty;
for (var i = 0; i < 64; i++) probe({ y: i });
Object.prototype.hasOwnProperty = function () { return 'patched'; };
out.push(probe({ y: 1 })());
out.push(probe({ y: 1, hasOwnProperty: function () { return 'own'; } })());
Object.prototype.hasOwnProperty = original;
out.push(probe({ y: 1 }) === original);
print(out.join('|'));
out.join('|');
"#;

const STRING_PROTOTYPE: &str = r#"
function probe(s) { return s.slice; }
var out = [];
var original = String.prototype.slice;
for (var i = 0; i < 64; i++) probe('abc');
String.prototype.slice = function () { return 'patched:' + this; };
out.push(probe('abc').call('abc'));
Object.defineProperty(String.prototype, 'slice', { get: function () { return function () { return 'getter'; }; }, configurable: true });
out.push(probe('abc')());
delete String.prototype.slice;
out.push(String(probe('abc')));
print(out.join('|'));
out.join('|');
"#;

const OTHER_REALM: &str = r#"
function probe(o) { return o.hasOwnProperty; }
var out = [];
for (var i = 0; i < 64; i++) probe({ y: i });
var other = __scriptwCreateRealm().global;
var foreign = new other.Object();
out.push(probe(foreign) === other.Object.prototype.hasOwnProperty);
out.push(probe(foreign) === Object.prototype.hasOwnProperty);
other.Object.prototype.hasOwnProperty = function () { return 'other realm'; };
out.push(probe(foreign)());
out.push(probe({ y: 1 }) === Object.prototype.hasOwnProperty);
print(out.join('|'));
out.join('|');
"#;

const OTHER_REALM_STRING_AND_WEAK_MAP: &str = r#"
function probe(o) { return o.get; }
function slice(s) { return s.slice; }
var out = [];
var w = new WeakMap();
for (var i = 0; i < 64; i++) { probe(w); slice('abc'); }
var other = __scriptwCreateRealm().global;
var foreign = new other.WeakMap();
out.push(probe(foreign) === other.WeakMap.prototype.get);
out.push(probe(foreign) === WeakMap.prototype.get);
other.String.prototype.slice = function () { return 'other realm'; };
out.push(slice('abc') === String.prototype.slice);
print(out.join('|'));
out.join('|');
"#;

/// Each fixture with the kind its probe site was trained on.
const EXACTNESS_FIXTURES: &[(&str, &str, ReadSpecialisation)] = &[
    (
        "Proxy receiver",
        PROXY,
        ReadSpecialisation::OwnCompact(CompactStorageLayout::Inline),
    ),
    (
        "getter added to the prototype",
        PROTOTYPE_GETTER,
        ReadSpecialisation::ProtoPlaceCompact(CompactStorageLayout::Inline, PlaceHolder::Compact),
    ),
    (
        "setPrototypeOf",
        SET_PROTOTYPE_OF,
        ReadSpecialisation::ProtoPlaceCompact(CompactStorageLayout::Inline, PlaceHolder::Compact),
    ),
    (
        "delete",
        DELETE,
        ReadSpecialisation::OwnCompact(CompactStorageLayout::Inline),
    ),
    (
        "defineProperty non-writable and accessor",
        DEFINE_PROPERTY,
        ReadSpecialisation::OwnCompact(CompactStorageLayout::Inline),
    ),
    (
        "freeze, seal and preventExtensions",
        FREEZE_SEAL,
        ReadSpecialisation::OwnCompact(CompactStorageLayout::Inline),
    ),
    (
        "key added to Object.prototype",
        OBJECT_PROTOTYPE_KEY,
        ReadSpecialisation::AbsentClosedTail(CompactStorageLayout::Inline),
    ),
    (
        "Object.prototype method replaced",
        OBJECT_PROTOTYPE_METHOD,
        ReadSpecialisation::ProtoPlaceCompact(CompactStorageLayout::Inline, PlaceHolder::Intrinsic),
    ),
    (
        "String.prototype edit",
        STRING_PROTOTYPE,
        ReadSpecialisation::StringPrototypePlace,
    ),
    (
        "other-Realm ordinary receiver",
        OTHER_REALM,
        ReadSpecialisation::ProtoPlaceCompact(CompactStorageLayout::Inline, PlaceHolder::Intrinsic),
    ),
    (
        "other-Realm WeakMap and String.prototype",
        OTHER_REALM_STRING_AND_WEAK_MAP,
        ReadSpecialisation::ProtoPlaceIntrinsic(ReceiverHeader::WeakMap),
    ),
];

fn options(execution_profile: ExecutionProfileUse) -> CompileOptions {
    CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        execution_profile,
        ..CompileOptions::default()
    }
}

fn observe(source: &str, execution_profile: ExecutionProfileUse) -> String {
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script(source, options(execution_profile), RunOptions::default())
        .unwrap_or_else(|error| panic!("{error}"));
    format!("{:?} {:?}", outcome.completion, outcome.output_events)
}

/// A profile of one training request in which only the read sites of the
/// `probe` bodies are hot, with the kind they were trained on.
fn true_kind_profile(source: &str, kind: ReadSpecialisation) -> ExecutionProfile {
    let engine = engine();
    let unit = engine
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
    let probe = artifact
        .function_sizes
        .iter()
        .filter_map(|body| body.name.strip_prefix("js::probe#"))
        .map(|id| SiteBody::Source(id.to_owned()))
        .collect::<Vec<_>>();
    assert!(!probe.is_empty(), "a body named probe");
    let key = program_wasm_cache_key(
        source,
        ParseGoal::Script,
        &options(ExecutionProfileUse::None),
    )
    .unwrap();
    let mut profile = ExecutionProfile::new(
        scriptw_aot_wasm::ProgramKey::new(key),
        artifact.site_map(),
        "exactness",
        std::num::NonZeroU32::MIN,
    );
    for site in artifact
        .site_map()
        .read_sites()
        .filter(|site| probe.contains(site.body()))
    {
        profile
            .observe_read(site, ReadSiteObservation::dominated(64, kind))
            .unwrap();
    }
    assert!(profile.hot_reads().count() >= 1);
    profile
}

#[test]
fn trained_sites_keep_every_answer_after_the_receiver_or_its_chain_changes() {
    for (name, source, kind) in EXACTNESS_FIXTURES {
        let expected = observe(source, ExecutionProfileUse::None);
        assert!(expected.contains("Normal"), "{name}: {expected}");
        assert_eq!(
            observe(source, ExecutionProfileUse::Collect),
            expected,
            "{name}: Collect"
        );
        let true_kind = true_kind_profile(source, *kind);
        assert_eq!(
            observe(source, ExecutionProfileUse::Apply(Arc::new(true_kind))),
            expected,
            "{name}: true kind"
        );
        for seed in 0..3 {
            let wrong = engine()
                .adversarial_execution_profile(
                    source,
                    EmbeddedModuleGoal::Script,
                    &options(ExecutionProfileUse::None),
                    seed,
                )
                .unwrap();
            assert_eq!(
                observe(source, ExecutionProfileUse::Apply(Arc::new(wrong))),
                expected,
                "{name}: adversarial {seed}"
            );
        }
    }
}

#[test]
fn a_profile_of_another_program_is_a_typed_refusal() {
    let profile = true_kind_profile(
        DELETE,
        ReadSpecialisation::OwnCompact(CompactStorageLayout::Inline),
    );
    let error = engine()
        .compile_script(
            FREEZE_SEAL,
            options(ExecutionProfileUse::Apply(Arc::new(profile.clone()))),
        )
        .unwrap_err();
    assert_eq!(
        error.execution_profile_error(),
        Some(&ProfileError::Mismatch(ProfileMismatch::Program)),
        "{error}"
    );
    let error = Engine::new(RealmBuilder::new().build())
        .observe_script(
            FREEZE_SEAL,
            options(ExecutionProfileUse::Apply(Arc::new(profile))),
            RunOptions::default(),
        )
        .unwrap_err();
    assert_eq!(
        error.execution_profile_error(),
        Some(&ProfileError::Mismatch(ProfileMismatch::Program)),
        "{error}"
    );
}
