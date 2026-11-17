//! A strict function whose own arguments object only `.length` and reads
//! observe answers them from its argument vector and builds the real object
//! the first time a read needs it: an index at or past the count, a key
//! that is not a Number integer index, "callee" or a Symbol. Reads after it
//! go to the object, which an inherited getter may change. Sloppy functions
//! keep their mapped object. Expectations are Node 24's.
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
        "{:?}\n{source}",
        observed.completion
    );
    assert_eq!(
        observed.output_events,
        [HostOutputEvent::PrintLine(expected.into())],
        "{source}"
    );
}

fn run_at_top_level_and_in_a_function(source: &str, expected: &str) {
    run(source, expected);
    run(&format!("(function () {{\n{source}}})();"), expected);
}

#[test]
fn arguments_reads_answer_from_the_vector_until_the_object_is_needed() {
    // Parameters reassigned before the reads, fewer and more actuals than
    // formals, every kind of non-index key, a converting key object,
    // "callee", an inherited Object.prototype[5] getter that changes the
    // object, and createElement's loops.
    let source = r#"
var log = [];
function strictReads(a, b) {
  "use strict";
  a = "A"; b = "B";
  var out = [arguments.length];
  for (var i = 0; i < arguments.length + 2; i++) out.push(String(arguments[i]));
  out.push(String(arguments[-0]), String(arguments[1.5]), String(arguments[NaN]), String(arguments["0"]), String(arguments["length"]));
  out.push(arguments[arguments.length - 1]);
  out.push(typeof arguments[Symbol.iterator], arguments[Symbol.toStringTag] === undefined);
  return out.join("/");
}
function sloppyReads(a, b) {
  a = "A"; b = "B";
  var out = [arguments.length];
  for (var i = 0; i < arguments.length + 2; i++) out.push(String(arguments[i]));
  out.push(String(arguments[-0]), String(arguments[1.5]), String(arguments["0"]), String(arguments["length"]));
  out.push(arguments[arguments.length - 1]);
  return out.join("/");
}
log.push(strictReads(), strictReads(1), strictReads(1, 2, 3, 4), sloppyReads(), sloppyReads(1), sloppyReads(1, 2, 3, 4));
// A key object converts once per read, in order.
function strictKey(k) {
  "use strict";
  return [arguments[k], arguments[k]].join(",");
}
var conversions = 0;
var key = { toString: function () { conversions++; return "0"; } };
log.push(strictKey(key), conversions);
// "callee" throws in strict code.
function strictCallee() { "use strict"; return arguments.callee; }
try { strictCallee(1); log.push("no error"); } catch (e) { log.push(e.constructor === TypeError); }
// Inherited index properties: a getter sees one receiver per invocation and
// may change the object before later reads.
var receivers = [];
Object.defineProperty(Object.prototype, 5, { configurable: true, get: function () { receivers.push(this); this.length = 9; this[0] = "changed"; return "inherited"; } });
function strictInherited(a) {
  "use strict";
  var before = arguments.length, first = arguments[0];
  var five = arguments[5], again = arguments[5];
  return [before, first, five, again, arguments.length, arguments[0], receivers.length, receivers[0] === receivers[1]].join("/");
}
log.push(strictInherited("x"), strictInherited("y", 2, 3, 4, 5, 6));
receivers.length = 0;
log.push(strictInherited());
delete Object.prototype[5];
// Index loops and for-in, as createElement uses them.
function create(type, config, children) {
  "use strict";
  var props = {}, n = arguments.length - 2, k;
  for (k in config) props[k] = config[k];
  if (n === 1) props.children = children;
  else if (n > 1) { var c = Array(n); for (var i = 0; i < n; i++) c[i] = arguments[i + 2]; props.children = c; }
  return JSON.stringify(props);
}
log.push(create("div", { a: 1 }), create("div", null, "x"), create("div", { b: 2 }, "x", "y", "z"));
print(log.join(" | "));
"#;
    let expected = r#"0/undefined/undefined/undefined/undefined/undefined/undefined/0//function/true | 1/1/undefined/undefined/1/undefined/undefined/1/1/1/function/true | 4/1/2/3/4/undefined/undefined/1/undefined/undefined/1/4/4/function/true | 0/undefined/undefined/undefined/undefined/undefined/0/ | 1/A/undefined/undefined/A/undefined/A/1/A | 4/A/B/3/4/undefined/undefined/A/undefined/A/4/4 | 0,0 | 4 | true | 1/x/inherited/inherited/9/changed/2/true | 6/y/6/6/6/y/2/true | 0//inherited/inherited/9/changed/2/true | {"a":1} | {"children":"x"} | {"b":2,"children":["x","y","z"]}"#;
    run_at_top_level_and_in_a_function(source, expected);
}

#[test]
fn method_calls_keep_the_projection_unless_the_object_is_their_receiver() {
    // createElement's shape: an own-key test through a captured
    // `hasOwnProperty.call`, comma heads in for-in, and index reads into a
    // fresh Array. A method called on the arguments object, or an element
    // called as a function, has the real object as `this`.
    let source = r#"
var log = [];
var has = Object.prototype.hasOwnProperty;
function create(type, config, children) {
  "use strict";
  var props = {}, key = null, k;
  if (config != null) for (k in config.key !== void 0 && (key = "" + config.key), config) has.call(config, k) && k !== "key" && (props[k] = config[k]);
  var n = arguments.length - 2;
  if (n === 1) props.children = children;
  else if (1 < n) { for (var c = Array(n), i = 0; i < n; i++) c[i] = arguments[i + 2]; props.children = c; }
  if (type && type.defaultProps) for (k in n = type.defaultProps, n) props[k] === void 0 && (props[k] = n[k]);
  return JSON.stringify([type.name || type, key, props]);
}
function Widget() {}
Widget.defaultProps = { size: 3, a: "default" };
var inherited = Object.create({ skipped: 1 });
inherited.key = 7; inherited.a = 1;
log.push(create("div"), create("div", null), create("div", inherited), create(Widget, { b: 2 }, "x"), create(Widget, { a: 0, key: "k" }, "x", "y", "z"));
// A method called on the object itself has it as `this`; a method called on
// a passed value has that value.
function ownMethod(a) { "use strict"; return [arguments.hasOwnProperty(0), arguments.hasOwnProperty(1), arguments.length].join(","); }
function passedMethod(f) { "use strict"; return arguments[0].call("x", arguments.length); }
function elementCall(f) { "use strict"; return arguments[0](); }
function describe(n) { "use strict"; return Object.prototype.toString.call(this) + (this.length === undefined ? "" : this.length) + (n === undefined ? "" : n); }
log.push(ownMethod(), ownMethod(1), passedMethod(describe, 2, 3), elementCall(describe, 5));
// The receiver of a method call is evaluated once, before the arguments.
var order = [];
function receiver() { order.push("receiver"); return { m: function (a, b) { return a + b + order.join(">"); } }; }
function methodOrder(a) { "use strict"; return receiver().m((order.push("first"), arguments[0]), (order.push("second"), arguments.length)); }
log.push(methodOrder("v", 0, 0));
print(log.join(" | "));
"#;
    let expected = r#"["div",null,{}] | ["div",null,{}] | ["div","7",{"a":1}] | ["Widget",null,{"b":2,"children":"x","size":3,"a":"default"}] | ["Widget","k",{"a":0,"children":["x","y","z"],"size":3}] | false,false,0 | true,false,1 | [object String]13 | [object Arguments]2 | v3receiver>first>second"#;
    run_at_top_level_and_in_a_function(source, expected);
}

#[test]
fn one_unit_string_comparisons_read_through_the_projection() {
    // `arguments[i] === "c"` on either side, under `&&`, after a value
    // computed into a var, past the count where an inherited getter answers,
    // and Function.prototype.bind's bound and constructed calls as Test262's
    // 15.3.4.5-3-1 and 15.3.4.5.2-4-1/-2 make them.
    let source = r#"
var log = [];
function first(x) { "use strict"; return arguments[0] === "a"; }
function reversed(x) { "use strict"; return "a" === arguments[0]; }
function differs(x) { "use strict"; return arguments[0] !== "b"; }
function guarded(x) { "use strict"; return true && arguments[0] === "a"; }
function second(x) { "use strict"; return x === "a" && arguments[1] === "a"; }
function viaVar(x) { "use strict"; var y = x === "a"; return y && arguments[0] === "a"; }
function mixed(x) { "use strict"; return arguments[0] === "a" && arguments[0] === x && arguments.length; }
log.push([first("a"), first("b"), first(), first("ab"), first(new String("a"))].join(","));
log.push([reversed("a"), reversed("b"), differs("a"), differs("b"), differs()].join(","));
log.push([guarded("a"), second("a", "a"), second("a"), second("b", "a"), viaVar("a"), viaVar("b")].join(","));
log.push([mixed("a"), mixed("a", 2, 3), mixed("b")].join(","));
// An inherited index getter answers an index past the count.
Object.defineProperty(Object.prototype, 1, { configurable: true, get: function () { return "a"; } });
log.push([second("a"), second("a", "b")].join(","));
delete Object.prototype[1];
// Function.prototype.bind's bound and constructed calls, as Test262's
// 15.3.4.5-3-1 and 15.3.4.5.2-4-1/-2 use them.
var foo = function (x, y) {
  "use strict";
  return new Boolean((x + y) === "ab" && arguments[0] === "a" && arguments[1] === "b" && arguments.length === 2);
};
log.push(foo.bind({}, "a", "b")() == true, foo.bind({}, "a")("b") == true, foo.bind({}, "b", "a")() == true);
var func = function (x, y, z) {
  "use strict";
  var objResult = {};
  objResult.returnValue = x + y + z;
  objResult.returnVerifyResult = arguments[0] === "a" && arguments.length === 3;
  return objResult;
};
var a = new (Function.prototype.bind.call(func, {}, "a", "b", "c"))();
var b = new (Function.prototype.bind.call(func, {}))("a", "b", "c");
log.push(a.returnValue, a.returnVerifyResult, b.returnValue, b.returnVerifyResult);
print(log.join(" | "));
"#;
    let expected = r#"true,false,false,false,false | true,false,true,false,true | true,true,false,false,true,false | 1,3,false | true,false | true | true | false | abc | true | abc | true"#;
    run_at_top_level_and_in_a_function(source, expected);
}
