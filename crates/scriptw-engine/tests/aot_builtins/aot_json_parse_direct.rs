//! JSON.parse without a reviver builds compact objects, numbers and strings
//! directly. Expected lines follow ECMA-262 (JSON.parse, CreateDataProperty,
//! OrdinaryOwnPropertyKeys, Number::fromString) and were cross-checked with
//! Node 22.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_json_parse_output(source: &str, expected: &[&str]) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compiler worker");
    // The indirect form reaches the runtime builtin through an ordinary call.
    let indirect = format!(
        "var __jsonParse = JSON.parse;\n{}",
        source.replace("JSON.parse(", "__jsonParse(")
    );
    for source in [source.to_owned(), indirect] {
        let observation = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
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
            .expect("JSON.parse must execute through Wasm AOT");
        assert_eq!(observation.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(observation.completion, ObservedCompletion::Normal(_)),
            "{:?}\n{source}",
            observation.completion
        );
        assert_eq!(
            observation.output_events,
            expected
                .iter()
                .map(|line| HostOutputEvent::PrintLine((*line).into()))
                .collect::<Vec<_>>(),
            "{source}"
        );
    }
}

/// Number tokens inside and outside the exact binary64 window round exactly as Number::fromString / RoundMVResult.
#[test]
fn numbers_are_correctly_rounded_from_code_units() {
    assert_json_parse_output(
        r#"
var tokens = ["0","-0","1","-1","123456789","9007199254740993","9007199254740992","9007199254740995","1.5","0.1","0.3","-0.0","1e22","1e23","1.7976931348623157e308","1.8e308","5e-324","2e-324","3e-324","123.456e-7","0.000001","1E5","1e+5","1e-5","12345678901234567890","0.1234567890123456789","4.35","1.0000000000000002","2.2250738585072011e-308","-1.5e-10","0e999999","-0e-999999","100000000000000000000000","1.00000000000000011102230246251565404236316680908203125","1.00000000000000011102230246251565404236316680908203126","9007199254740993.0","0.000000000000000000001","0.0000000000000000000001","0.00000000000000000000001","123e-20","8.98846567431158e307","-9007199254740992e22","4503599627370497.5","1e-22","7e22","9007199254740991e-22"];
var out = [];
for (var i = 0; i < tokens.length; i++) {
  var v = JSON.parse(tokens[i]);
  out.push(Object.is(v, -0) ? "-0" : String(v));
}
print(out.join(" "));
var nested = JSON.parse('[1.5, -0, {"n": 2.5e-3, "m": [1e400, -1e400]}]');
print(nested[0], Object.is(nested[1], -0), nested[2].n, nested[2].m[0], nested[2].m[1]);
"#,
        &[
            "0 -0 1 -1 123456789 9007199254740992 9007199254740992 9007199254740996 1.5 0.1 0.3 -0 1e+22 1e+23 1.7976931348623157e+308 Infinity 5e-324 0 5e-324 0.0000123456 0.000001 100000 100000 0.00001 12345678901234567000 0.12345678901234568 4.35 1.0000000000000002 2.225073858507201e-308 -1.5e-10 0 -0 1e+23 1 1.0000000000000002 9007199254740992 1e-21 1e-22 1e-23 1.23e-18 8.98846567431158e+307 -9.007199254740992e+37 4503599627370498 1e-22 7e+22 9.007199254740991e-7",
            "1.5 true 0.0025 Infinity -Infinity",
        ],
    );
}

/// Duplicate keys overwrite in place, index keys enumerate first, and __proto__ stays an own data key.
#[test]
fn fresh_objects_keep_create_data_property_order_duplicates_and_proto_keys() {
    assert_json_parse_output(
        r#"
var o = JSON.parse('{"b":1,"a":2,"b":3,"2":4,"1":5,"__proto__":6,"c":{"x":[1,2,{"y":null}]}}');
print(Object.keys(o).join(','), o.b, o.__proto__, Object.getPrototypeOf(o) === Object.prototype, Object.hasOwn(o, '__proto__'), JSON.stringify(o));
var d = JSON.parse('{"a":1,"b":2,"a":3}');
var desc = Object.getOwnPropertyDescriptor(d, 'a');
print(Object.keys(d).join(','), d.a, desc.writable, desc.enumerable, desc.configurable);
d.c = 4; delete d.a; d.a = 5;
print(Object.keys(d).join(','), JSON.stringify(d));
var p1 = JSON.parse('{"x":1,"y":"one"}'), p2 = JSON.parse('{"x":2,"y":"two"}');
p1.x = 10;
print(p1.x, p1.y, p2.x, p2.y, Object.keys(p2).join(','));
var parts = [];
for (var i = 0; i < 70; i++) parts.push('"k' + i + '":' + i);
parts.push('"k5":"dup"');
var wide = JSON.parse('{' + parts.join(',') + '}');
var keys = Object.keys(wide);
print(keys.length, keys[0], keys[5], keys[64], keys[69], wide.k5, wide.k69);
var mixed = JSON.parse('{"z":1,"10":2,"y":3,"4294967295":4,"4294967294":5,"01":6,"0":7}');
print(Object.keys(mixed).join(','));
var proto = JSON.parse('{"__proto__":{"inherited":true}}');
print(proto.inherited, typeof proto.__proto__, Object.getPrototypeOf(proto) === Object.prototype, Object.keys(proto).join(','));
var frozen = JSON.parse('{"a":{"b":1}}');
Object.freeze(frozen.a);
frozen.a.b = 2;
print(frozen.a.b, Object.isFrozen(frozen.a), Object.isExtensible(frozen));
var empty = JSON.parse(' { } ');
empty.q = 1;
print(Object.keys(empty).join(','), JSON.stringify(JSON.parse('[]')), JSON.stringify(JSON.parse('[[],{},[{}]]')));
var key = JSON.parse('{"ab":1,"a\\u0062":2,"":3}');
print(Object.keys(key).join('|'), key.ab, key['']);
"#,
        &[
            "1,2,b,a,__proto__,c 3 6 true true {\"1\":5,\"2\":4,\"b\":3,\"a\":2,\"__proto__\":6,\"c\":{\"x\":[1,2,{\"y\":null}]}}",
            "a,b 3 true true true",
            "b,c,a {\"b\":2,\"c\":4,\"a\":5}",
            "10 one 2 two x,y",
            "70 k0 k5 k64 k69 dup 69",
            "0,10,4294967294,z,y,4294967295,01",
            "undefined object true __proto__",
            "1 true true",
            "q [] [[],{},[{}]]",
            "ab| 2 3",
        ],
    );
}

/// Unescaped runs, escapes at either end, lone surrogates and long tokens decode exactly.
#[test]
fn strings_copy_unescaped_runs_and_decode_escapes() {
    assert_json_parse_output(
        r#"
function codes(s) { var r = []; for (var i = 0; i < s.length; i++) r.push(s.charCodeAt(i).toString(16)); return r.join('.'); }
var inputs = ['""', '"plain"', '"a\\nb"', '"\\u0041\\u00e9\\ud83d\\ude00x"', '"\\"\\\\\\/\\b\\f\\n\\r\\t"', '"\\ud800"', '"\\u00"+"00"', '"é€😀"', '"\\\\"', '"x\\\\"', '"\\u0000"', '"\u007f\u0080"'];
var out = [];
for (var i = 0; i < inputs.length; i++) {
  try { var v = JSON.parse(inputs[i]); out.push(v.length + ':' + codes(v)); } catch (e) { out.push(e.constructor.name); }
}
print(out.join(' '));
var long = '';
for (var j = 0; j < 300; j++) long += 'abc';
var tail = JSON.parse('"' + long + '\\n"'), head = JSON.parse('"\\t' + long + '"'), middle = JSON.parse('"' + long + '\\u0041' + long + '"');
print(tail.length, tail.charCodeAt(900), head.length, head.charCodeAt(0), head.slice(1) === long, middle.length, middle.charAt(900), middle === long + 'A' + long);
var arr = JSON.parse('["a\\"b", "c", "d\\\\", "", "\\/"]');
print(arr.length, arr.join('|'));
"#,
        &[
            "0: 5:70.6c.61.69.6e 3:61.a.62 5:41.e9.d83d.de00.78 8:22.5c.2f.8.c.a.d.9 1:d800 SyntaxError 4:e9.20ac.d83d.de00 1:5c 2:78.5c 1:0 2:7f.80",
            "901 10 901 9 true 1801 A true",
            "5 a\"b|c|d\\||/",
        ],
    );
}

/// Every malformed token or structure is a SyntaxError of the current realm.
#[test]
fn malformed_text_still_throws_syntax_error() {
    assert_json_parse_output(
        r#"
var bad = ['{"a" 1}', '{"a":1,}', '[1,]', '"\\x"', '"abc', '"a\u0001b"', '01', '1.', '-', '1e', '1e+', '-a', '{"a":1 "b":2}', '{,}', '[', '', ' ', '"\\u12G4"', '{"a"', '{"a":}', 'tru', '1 2', '{"a":1}}', '[1 2]', '"\\', '"\\u', '{1:2}', '+1', '.5', '1.e5', '"a\nb"', '{"a":1,"b"}', '[1,2', 'nul', '"\\ud800\\"'];
var out = [];
for (var i = 0; i < bad.length; i++) {
  try { JSON.parse(bad[i]); out.push('parsed'); } catch (e) { out.push(e instanceof SyntaxError && e.constructor === SyntaxError ? 'S' : 'other'); }
}
print(out.join(''));
var good = [' [ 1 , { "a" : [ ] } ] ', '\t\r\n"x"\n', '{"a":{"b":{"c":{}}}}', 'true', 'null', 'false', '-0.5E-2'];
print(good.map(function (t) { return JSON.stringify(JSON.parse(t)); }).join(' '));
"#,
        &[
            "SSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSS",
            "[1,{\"a\":[]}] \"x\" {\"a\":{\"b\":{\"c\":{}}}} true null false -0.005",
        ],
    );
}

/// A reviver sees the same objects, the CreateDataProperty order and each primitive's source text.
#[test]
fn reviver_walks_compact_objects_and_sees_exact_sources() {
    assert_json_parse_output(
        r#"
var seen = [];
var r = JSON.parse('{"b":[1,{"c":2.5}],"a":"x","2":true,"b":[3],"__proto__":null}', function (key, value) {
  seen.push(key + '=' + (typeof value === 'object' && value !== null ? (Array.isArray(value) ? 'array' : 'object') : String(value)));
  if (key === 'a') return undefined;
  if (key === '2') { this.late = 9; return 'two'; }
  return value;
});
print(seen.join(','));
print(Object.keys(r).join(','), r[2], r.late, r.a, JSON.stringify(r));
var sources = [];
JSON.parse('[1.50, "s\\u0041", {"k": -0e1}]', function (key, value, context) {
  if (context && 'source' in context) sources.push(key + ':' + context.source);
  return value;
});
print(sources.join(' '));
"#,
        &[
            "2=true,0=3,b=array,a=x,__proto__=null,=object",
            "2,b,__proto__,late two 9 undefined {\"2\":\"two\",\"b\":[3],\"__proto__\":null,\"late\":9}",
            "0:1.50 1:\"s\\u0041\" k:-0e1",
        ],
    );
}
