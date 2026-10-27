//! Cross-route recovery must restore captures, repeat frames and both cursor units.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion,
    ObservedJsValue, RealmBuilder, RunOptions,
};

fn check(cases: &[(&str, &str, &str, &str)]) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let mut source = String::from("function require(v,m){if(!v)throw new Error(m);}\nfunction computed(units,flags){var s='';for(var i=0;i<units.length;i++)s+=String.fromCharCode(units[i]);return new RegExp(s,flags);}\n");
    for (case, (pattern, flags, input, expectation)) in cases.iter().enumerate() {
        let units: Vec<u16> = pattern.encode_utf16().collect();
        source.push_str(&format!(
            r#"
var expressions=[/{pattern}/{flags},computed({units:?},{flags:?})];
for(var round=0;round<2;round++){{
 var previous;
 for(var variant=0;variant<2;variant++){{
  var expression=expressions[variant];gc();
  var m=expression.exec({input:?});
  require(({expectation}),'case {case}: exact captures/indices');
  gc();
  var signature=JSON.stringify([m&&Array.from(m),m&&m.indices,m&&m.index,expression.lastIndex]);
  if(variant)require(signature===previous,'case {case}: literal/computed differential');
  previous=signature;
 }}
}}
"#
        ));
    }
    source.push_str("true;");
    for directive in ["", "'use strict';\n"] {
        let script = format!("{directive}{source}");
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &script,
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
            .unwrap();
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(ObservedJsValue::Boolean(true)),
            "{script}"
        );
    }
}

#[test]
fn alternative_backreference_and_assertion_failures_restore_capture_participation() {
    check(&[
        (r"^(?:(a)|(ab))c\2$", "d", "abcab", "m!==null&&m[0]==='abcab'&&m[1]===undefined&&m.indices[1]===undefined&&m[2]==='ab'&&m.indices[2][0]===0&&m.indices[2][1]===2"),
        (r"^((a)|(ab))+c$", "d", "aabc", "m!==null&&m[1]==='ab'&&m[2]===undefined&&m.indices[2]===undefined&&m[3]==='ab'&&m.indices[3][0]===1&&m.indices[3][1]===3"),
        (r"^(?!(a)b)(a|ac)c$", "d", "acc", "m!==null&&m[1]===undefined&&m.indices[1]===undefined&&m[2]==='ac'&&m.indices[2][0]===0&&m.indices[2][1]===2"),
        (r"^(a|ab)\1c$", "d", "ababc", "m!==null&&m[1]==='ab'&&m.indices[1][0]===0&&m.indices[1][1]===2"),
        (r"(a|ab)c\1", "d", "zabcab", "m!==null&&m.index===1&&m[1]==='ab'&&m.indices[1][0]===1&&m.indices[1][1]===3"),
        (r"^(?:(a)|(ab)){2,3}c\2$", "d", "ababx", "m===null"),
    ]);
}

#[test]
fn required_runs_and_lazy_empty_retries_restore_repeat_and_capture_frames() {
    check(&[
        (r"^((?:a|ab){2,3})bc$", "d", "aabbc", "m!==null&&m[0]==='aabbc'&&m[1]==='aab'&&m.indices[1][0]===0&&m.indices[1][1]===3"),
        (r"^((a?)*?)(a)b$", "d", "aab", "m!==null&&m[1]==='a'&&m[2]==='a'&&m[3]==='a'&&m.indices[2][0]===0&&m.indices[2][1]===1&&m.indices[3][0]===1"),
        (r"^((a?){2,4}?)(a)b$", "d", "aab", "m!==null&&m[1]==='a'&&m[2]===''&&m.indices[2][0]===1&&m.indices[2][1]===1&&m[3]==='a'&&m.indices[3][0]===1"),
        (r"^(?:(a)|()){2,4}?bc$", "d", "bc", "m!==null&&m[1]===undefined&&m.indices[1]===undefined&&m[2]===''&&m.indices[2][0]===0&&m.indices[2][1]===0"),
    ]);
}

#[test]
fn unicode_reverse_and_partial_surrogate_cursors_survive_recovery_and_gc() {
    check(&[
        (
            r"^((?:😀a|😀)+)a$",
            "du",
            "😀a",
            "m!==null&&m[1]==='😀'&&m.indices[1][0]===0&&m.indices[1][1]===2&&m.indices[0][1]===3",
        ),
        (
            r"(?<=((?:😀a|😀)+))a$",
            "du",
            "😀a",
            "m!==null&&m.index===2&&m[1]==='😀'&&m.indices[1][0]===0&&m.indices[1][1]===2",
        ),
        (
            r"^(😀|😀a)\1b$",
            "du",
            "😀a😀ab",
            "m!==null&&m[1]==='😀a'&&m.indices[1][1]===3&&m.indices[0][1]===7",
        ),
        (
            r"^(?:\ud83d\ude00|\ud83d)(\ude00)a$",
            "d",
            "😀a",
            r"m!==null&&m[1]==='\ude00'&&m.indices[1][0]===1&&m.indices[1][1]===2",
        ),
    ]);
}
