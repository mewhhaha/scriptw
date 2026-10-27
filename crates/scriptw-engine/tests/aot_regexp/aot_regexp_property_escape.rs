use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, ObservedJsValue, ObservedNumber, RealmBuilder, RunOptions,
};

const UNITS: &str = include_str!("../fixtures/regexp_runtime_gap/from_units.js");
const PROPERTY_HELPERS: &str = r#"
function textUnits(text) {
  var units = [];
  for (var i = 0; i < text.length; i++) units.push(text.charCodeAt(i));
  return units;
}
function propertyUnits(name, marker) {
  return [92, marker, 123].concat(textUnits(name), [125]);
}
function computed(units, flags) { return new RegExp(fromUnits(units), fromUnits(flags)); }
function property(name, marker, flags) {
  return computed([94].concat(propertyUnits(name, marker), [36]), flags);
}
function codePoint(value) {
  if (value <= 0xffff) return fromUnits([value]);
  value -= 0x10000;
  return fromUnits([0xd800 + (value >> 10), 0xdc00 + (value & 1023)]);
}
function rejects(units, flags) {
  var caught;
  try { computed(units, flags); } catch (error) { caught = error; }
  require(caught instanceof SyntaxError, 'property syntax is a catchable SyntaxError');
}
"#;

fn options() -> (CompileOptions, RunOptions) {
    (
        CompileOptions {
            host_surface_policy: HostSurfacePolicy::Test262,
            ..CompileOptions::default()
        },
        RunOptions {
            backend: ExecutionBackend::WasmAot,
            timeout_ms: Some(30_000),
            ..RunOptions::default()
        },
    )
}

pub(super) fn assert_normal(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("bounded compilation jobs");
    for directive in ["", "'use strict';\n"] {
        let script = format!("{directive}{UNITS}\n{PROPERTY_HELPERS}\n{source}");
        let (compile, run) = options();
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(&script, compile, run)
            .expect("computed property semantics execute through Wasm AOT");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(262.0))),
            "{script}"
        );
        assert_eq!(
            observed.output_events,
            vec![HostOutputEvent::PrintLine(expected.into())],
            "{script}"
        );
    }
}

#[test]
fn computed_property_aliases_and_unicode17_domains_have_real_membership() {
    assert_normal(
        include_str!("../fixtures/regexp_property_escape/domains.js"),
        "regexp-property-domains:ok",
    );
}

#[test]
fn property_complements_fold_per_operand_before_class_union_and_negation() {
    assert_normal(
        include_str!("../fixtures/regexp_property_escape/folding.js"),
        "regexp-property-folding:ok",
    );
}

#[test]
fn exact_syntax_repeated_descriptors_and_recompile_publication_are_preserved() {
    assert_normal(
        include_str!("../fixtures/regexp_property_escape/syntax_and_transactions.js"),
        "regexp-property-syntax:ok",
    );
}

#[test]
fn string_property_mode_and_negation_errors_remain_catchable() {
    assert_normal(
        include_str!("../fixtures/regexp_property_escape/string_syntax.js"),
        "regexp-property-strings:ok",
    );
}

#[test]
fn positive_v_string_properties_match_and_recompile_normally() {
    assert_normal(
        r#"
for (var row of [
  ['Basic_Emoji', [0x1f600]],
  ['Emoji_Keycap_Sequence', [0x31, 0xfe0f, 0x20e3]],
  ['RGI_Emoji_Modifier_Sequence', [0x1f44d, 0x1f3fd]],
  ['RGI_Emoji_Flag_Sequence', [0x1f1fa, 0x1f1f8]],
  ['RGI_Emoji_Tag_Sequence', [0x1f3f4, 0xe0067, 0xe0062, 0xe0065, 0xe006e, 0xe0067, 0xe007f]],
  ['RGI_Emoji_ZWJ_Sequence', [0x1f469, 0x200d, 0x1f4bb]],
  ['RGI_Emoji', [0x1f600]]
]) {
  var member = '';
  for (var point of row[1]) member += codePoint(point);
  var direct = property(row[0], 112, [118]);
  var bracketed = computed([94, 91].concat(propertyUnits(row[0], 112), [93, 36]), [118]);
  require(direct.test(member) && bracketed.test(member), 'complete computed string-property member');
  require(!direct.test('x') && !bracketed.test('x'), 'complete computed string-property nonmember');
}
var receiver = computed([111, 108, 100], [103]);
receiver.lastIndex = 7;
var source = fromUnits([94].concat(propertyUnits('RGI_Emoji', 112), [36]));
receiver.compile(source, fromUnits([118]));
require(receiver.source === source && receiver.flags === 'v' && receiver.lastIndex === 0, 'finite property recompile publishes slots');
require(receiver.test(codePoint(0x1f600)) && !receiver.test('x'), 'finite property recompile installs actual matcher');
print('regexp-property-finite:ok');
262;
"#,
        "regexp-property-finite:ok",
    );
}
