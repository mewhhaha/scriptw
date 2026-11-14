//! Identifier assignments to a statically proven cell read and write it
//! directly, and Number paths that need integer truncation avoid the
//! rounding operators. These scripts exercise every binding kind those
//! assignments meet and the edges of each truncation's range. Expected lines
//! are Node's.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let observed = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
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
        .expect("the script executes compiled Wasm");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(observed.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        [HostOutputEvent::PrintLine(expected.into())]
    );
}

#[test]
fn proven_cell_assignments_keep_tdz_immutability_and_order() {
    run(
        r#"
var log = [];
// Compound, update, logical and plain assignments to captured bindings of
// every kind: var, let in and out of its TDZ, const, a sloppy and a strict
// function-expression name, and a parameter.
(function () {
  var v = 1; let l = 2; const c = 3;
  function bump() { v += 10; v++; l *= 3; l--; v ||= 0; l &&= l + 1; return v + ":" + l; }
  log.push(bump(), bump());
  function setC() { "use strict"; c += 1; }
  try { setC(); } catch (e) { log.push(e instanceof TypeError, c); }
  function incC() { c++; }
  try { incC(); } catch (e) { log.push(e instanceof TypeError, c); }
  function orC() { c ||= 9; return c; }
  log.push(orC());
  function andC() { c &&= 9; }
  try { andC(); } catch (e) { log.push(e instanceof TypeError, c); }
  function early() { t += 1; }
  try { early(); } catch (e) { log.push(e instanceof ReferenceError); }
  function earlyRead() { return t++; }
  try { earlyRead(); } catch (e) { log.push(e instanceof ReferenceError); }
  let t = 5;
  log.push(early(), t, earlyRead(), t);
  var sloppyNamed = function self() { self += 1; self++; self = 0; return typeof self; };
  log.push(sloppyNamed());
  var strictNamed = function self() { "use strict"; try { self += 1; } catch (e) { return e instanceof TypeError; } };
  log.push(strictNamed());
  function param(p) { return function () { p += "!"; p ??= "x"; return p; }; }
  var g = param("a"); log.push(g(), g());
  var order = [];
  var w = { valueOf: function () { order.push("valueOf"); return 4; } };
  function rhsFirst() { v = (order.push("rhs"), w) ; v += (order.push("rhs2"), 1); return v; }
  log.push(rhsFirst(), order.join(""));
})();
print(log.join());
"#,
        "12:6,23:18,true,3,true,3,3,true,3,true,true,,6,6,7,function,true,a!,a!!,5,rhsrhs2valueOf",
    );
}

#[test]
fn integer_truncations_keep_their_exact_ranges() {
    run(
        r#"
var log = [];
// ToInt32/ToUint32 at the edges of the fast path, array indices with -0,
// fractions and large values, Number#toString(radix), Array(n), %, string
// positions and apply with fractional and huge lengths.
var xs = [0, -0, 1.5, -1.5, 2147483647, 2147483648, -2147483649, 4294967295, 4294967296, 4294967301.7, -4294967301.7, 9007199254740993, 9223372036854775807, -9223372036854775808, 9223372036854775808, 1e19, -1e19, 1e300, -1e300, NaN, Infinity, -Infinity, 0.9999999999999999, -0.5];
log.push(xs.map(function (x) { return (x | 0) + "/" + (x >>> 0) + "/" + (x & 63) + "/" + (~x); }).join(" "));
var arr = []; for (var i = 0; i < 70; i++) arr[i] = i;
var keys = [0, -0, 1, 1.5, 63, 69, 70, 4294967294, 4294967295, -1, NaN, 1e300];
log.push(keys.map(function (k) { return String(arr[k]); }).join(","));
var target = [0, 0, 0];
[-0, 1, 2.5, 4294967295, 2].forEach(function (k, n) { target[k] = n; });
log.push(Object.keys(target).join(","), target.length);
// Radix 16 is exact at every magnitude; other radixes approximate digits
// beyond binary64 precision, so radix 36 stays below 2^53.
log.push([255, 4096.5, 1e21, 2 ** 63, 2 ** 64, -255.25, 123456789012345680000].map(function (x) { return x.toString(16) + (Math.abs(x) < 2 ** 53 ? "|" + x.toString(36) : ""); }).join(" "));
[0, 3, 4294967295, 2.5, 4294967296, -1].forEach(function (n) { try { log.push(Array(n).length); } catch (e) { log.push(e.name); } });
log.push([7 % 3, -7 % 3, 7.5 % 2, 9007199254740991 % 2, -0 % 5, 5 % -0, 2 ** 53 % 3].join(","));
var s = "abcdefgh";
log.push([s.slice(1.9, 4.2), s.slice(-2.5), s.substring(1.5, 1e300), s.slice(-1e300, 2), s.substring(NaN, 3), s.indexOf("d", 2.9), s.charAt(2.7), s.at(-1.5)].join(","));
function count() { return arguments.length; }
log.push(count.apply(null, { length: 2.9 }), count.apply(null, { length: -3 }), count.apply(null, { length: 0.5 }));
log.push((255).toString(2), (-0).toString(16), (1e-7).toString(16).slice(0, 12), (0.5).toString(2));
print(log.join(" ; "));
"#,
        "0/0/0/-1 0/0/0/-1 1/1/1/-2 -1/4294967295/63/0 2147483647/2147483647/63/-2147483648 -2147483648/2147483648/0/2147483647 2147483647/2147483647/63/-2147483648 -1/4294967295/63/0 0/0/0/-1 5/5/5/-6 -5/4294967291/59/4 0/0/0/-1 0/0/0/-1 0/0/0/-1 0/0/0/-1 -1981284352/2313682944/0/1981284351 1981284352/1981284352/0/-1981284353 0/0/0/-1 0/0/0/-1 0/0/0/-1 0/0/0/-1 0/0/0/-1 0/0/0/-1 0/0/0/-1 ; 0,0,1,undefined,63,69,undefined,undefined,undefined,undefined,undefined,undefined ; 0,1,2,2.5,4294967295 ; 3 ; ff|73 1000.8|35s.i 3635c9adc5dea00000 8000000000000000 10000000000000000 -ff.4|-73.9 6b14e9f812f368000 ; 0 ; 3 ; 4294967295 ; RangeError ; RangeError ; RangeError ; 1,-1,1.5,1,0,NaN,2 ; bcd,gh,bcdefgh,ab,abc,3,c,h ; 2 ; 0 ; 0 ; 11111111 ; 0 ; 0.000001ad7f ; 0.1",
    );
}
