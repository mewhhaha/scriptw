//! A body `var` whose name the formal parameter list binds declares no new
//! binding. FunctionDeclarationInstantiation (ECMA-262 10.2.11) creates one
//! binding per name of `parameterNames`, the BoundNames of the whole list,
//! and step 27 instantiates a `var` only for a name outside it, so a name
//! that a binding pattern binds keeps its value like a name that an
//! identifier binds. A list that holds an expression gives the body its own
//! record (step 28), whose `var` starts with the parameter's value. A
//! function declaration of the name still replaces the value (step 36).
//! Expectations are Node 24's, taken by running each source as a sloppy
//! Script, as function code, as a strict Script and as a module, except
//! where a test says otherwise.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str, module: bool, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let options = CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    };
    let run = RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(60_000),
        ..RunOptions::default()
    };
    let observed = if module {
        engine.observe_module(source, options, run)
    } else {
        engine.observe_script(source, options, run)
    }
    .expect("the source executes compiled Wasm");
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

/// A sloppy Script and the same body as sloppy function code.
fn run_as_script_and_function(source: &str, expected: &str) {
    run(source, false, expected);
    run(&format!("(function () {{\n{source}}})();"), false, expected);
}

/// Sloppy as a Script and as function code, strict as a Script and as a
/// module. Each source observes only what both modes agree on.
fn run_in_every_goal_and_mode(source: &str, expected: &str) {
    run_as_script_and_function(source, expected);
    run(&format!("\"use strict\";\n{source}"), false, expected);
    run(source, true, expected);
}

#[test]
fn a_body_var_of_a_pattern_bound_name_keeps_the_parameter_value() {
    // Object, array and nested patterns, rest elements and rest parameters,
    // initializers inside a pattern, functions and arrow functions. The `var`
    // is written with and without an initializer, before and after the first
    // use, twice, and inside every statement that can hold one. Closures and
    // inner functions share the binding across a collection. A function
    // declaration of the name wins. The surrounding code declares the same
    // names, as Script globals, as an enclosing function's variables and as
    // module variables, and a function expression binds its own name.
    run_in_every_goal_and_mode(
        r##"
var log = [];
function show(f) { try { return String(f()); } catch (e) { return e.constructor.name; } }
function churn() { var keep = []; for (var i = 0; i < 20000; i++) keep[i & 63] = { i: i, a: [i, i + 1] }; return keep.length; }
// The same names are declared by the code around the functions too.
var id = "outer-id", a = "outer-a", b = "outer-b", tail = "outer-tail", others = "outer-others";
function objectPattern({id}) { var id; return id; }
function arrayPattern([id]) { var id; return id; }
var arrow = ({id}) => { var id; return id; };
var arrowArray = ([a, b]) => { var b, a; return a + b; };
function nestedBlock({id}) { { var id; } return id; }
function simple(id) { var id; return id; }
function initialized({id}) { var id = 5; return id; }
function initializedFromItself([a]) { var a = a + 1; return a; }
function nested({a: {b, c: [d]}}) { var b, d; return b + d; }
function nestedInitialized({y: {a}}) { var a = a + 1; return a; }
function restList(...r) { var r; return r.join("|"); }
function restPatternList(...[p, q]) { var p, q; return p + q; }
function restLength(...{length}) { var length; return length; }
function objectRest({a, ...others}) { var others; return Object.keys(others).join("|"); }
function arrayRest([a, ...tail]) { var tail; return tail.join("|"); }
function patternDefault({a = 5}) { var a; return a; }
function arrayDefault([a = 6]) { var a; return a; }
function twoPatterns({a}, {b}) { var b, a; return a + b; }
function mixed(x, {a}, y) { var a, y, x; return x + a + y; }
function declaredTwice({a}) { var a; var a; { var a; } return a; }
function afterUse({a}) { var b = a; var a; return b; }
function afterReturn({a}) { return a; var a; }
function assigned({a}) { a = 37; var a; return a; }
function updated({a}) { var a; a++; return a; }
function typeOfVar({a}) { var a; return typeof a; }
function unrelated({a}) { var c; return typeof c + a; }
log.push("plain", objectPattern({id: 1}), arrayPattern([2]), arrow({id: 4}), arrowArray([3, 4]), nestedBlock({id: 6}), simple(3), initialized({id: 1}), initializedFromItself([7]), nested({a: {b: 4, c: [5]}}), nestedInitialized({y: {a: 76}}), restList(1, 2, 3), restPatternList(60, 5), restLength(6, 5, 1), objectRest({a: 1, b: 2, c: 3}), arrayRest([1, 2, 3]), patternDefault({}), patternDefault({a: 8}), arrayDefault([]), arrayDefault([9]), twoPatterns({a: 1}, {b: 2}), mixed(1, {a: 2}, 4), declaredTwice({a: 34}), afterUse({a: 36}), afterReturn({a: 35}), assigned({a: 36}), updated({a: 52}), typeOfVar({a: function () {}}), unrelated({a: 1}));
// A `var` anywhere in the body is the same declaration.
function inFor({a}) { for (var a; false;) {} return a; }
function inForInitialized({a}) { for (var a = 0; a < 3; a++); return a; }
function inForIn({a}) { for (var a in {}) {} return a; }
function inForInRuns({a}) { for (var a in {q: 1}) {} return a; }
function inForOf({a}) { for (var a of []) {} return a; }
function inForOfRuns({a}) { for (var a of [99]) {} return a; }
function inForOfPattern({a}) { for (var [a] of [[100]]) {} return a; }
function inIf({a}) { if (false) { var a = 1; } return a; }
function inTry({a}) { try { throw 1; } catch (e) { var a; } return a; }
function inFinally({a}) { try {} finally { var a; } return a; }
function inCatchParameter({a}) { try { throw 5; } catch (a) { var a = 7; } return a; }
function underCatchParameter({a}) { var a; try { throw 5; } catch (a) { a = 8; } return a; }
function inSwitch({a}) { switch (1) { case 1: var a; } return a; }
function inLabel({a}) { label: var a; return a; }
function inLoops({a}) { while (false) { var a; } do { var a; } while (false); return a; }
function varPattern({a}) { var {a} = {a: 22}; return a; }
function varArrayPattern({a}) { var [a] = [23]; return a; }
function lexicalShadow({a}) { var a; { let a = 1; a++; } return a; }
function lexicalUse({a}) { { let a = 1; var b = a; } var a; return a + b; }
log.push("statements", inFor({a: 19}), inForInitialized({a: 98}), inForIn({a: 17}), inForInRuns({a: 98}), inForOf({a: 18}), inForOfRuns({a: 98}), inForOfPattern({a: 98}), inIf({a: 21}), inTry({a: 20}), inFinally({a: 44}), inCatchParameter({a: 97}), underCatchParameter({a: 96}), inSwitch({a: 41}), inLabel({a: 40}), inLoops({a: 42}), varPattern({a: 21}), varArrayPattern({a: 21}), lexicalShadow({a: 94}), lexicalUse({a: 94}));
// Closures and inner functions see the one binding, across a collection.
function closureAfter({a}) { var a; return () => a; }
function closureWrite({a}) { var a; var g = () => a; a = 16; return g(); }
function closureBefore({a}) { var g = () => a; var a; return g; }
function innerFunction({a}) { var a; function g() { return a; } return g(); }
function innerWrite([a]) { var a; function set(v) { a = v; } set(a + 1); return a; }
function counter({count}) { var count; return { next: function () { return ++count; } }; }
var kept = closureAfter({a: {deep: 15}}), counting = counter({count: 10});
churn();
log.push("closures", kept().deep, closureWrite({a: 15}), closureBefore({a: 14})(), innerFunction({a: 24}), innerWrite([30]), counting.next(), counting.next());
// A function declaration of the name still replaces the value.
function functionWins({a}) { function a() {} return typeof a; }
function functionWinsVar({a}) { var a; function a() {} return typeof a; }
function functionWinsArray([a]) { return typeof a; function a() {} }
function blockFunction({a}) { { function a() {} } return typeof a; }
log.push("functions", functionWins({a: 1}), functionWinsVar({a: 1}), functionWinsArray([1]), blockFunction({a: 38}));
// Names that the enclosing code and the function itself also spell.
function outerObject({id}) { var id; return id + ":" + typeof id; }
function outerInitialized({id}) { var id = 5; return id; }
function outerArray([a, b]) { var a = a + 1; var b; return a + b; }
function outerRest({a, ...others}, [b, ...tail]) { var others, tail; return a + Object.keys(others).join("") + b + tail.join(""); }
function outerFunction({a}) { function a() {} return typeof a; }
function outerClosure({id}) { var id; return () => id; }
var named = function self({self}) { var self; return self; };
var namedUnrelated = function self({a}) { var self; return typeof self + a; };
var namedAssigned = function self({self}) { self = 70; var self; return self; };
var namedClosure = function self({self}) { var self; return () => self; };
var namedSimple = function self(self) { var self; return self; };
var namedAlone = function self() { var self; return typeof self; };
log.push("outer", outerObject({id: 47}), outerInitialized({id: 1}), outerArray([47, 1]), outerRest({a: 1, x: 2}, [3, 4, 5]), outerFunction({a: 1}), outerClosure({id: 49})(), named({self: 67}), namedUnrelated({a: 69}), namedAssigned({self: 66}), namedClosure({self: 72})(), namedSimple(68), namedAlone(), id, a, b, tail, others);
print(log.join());
"##,
        r##"plain,1,2,4,7,6,3,5,8,9,77,1|2|3,65,3,b|c,2|3,5,8,6,9,3,7,34,36,35,37,53,function,undefined1,statements,19,3,17,q,18,99,100,21,20,44,97,96,41,40,42,22,23,94,95,closures,15,16,14,24,31,11,12,functions,function,function,function,number,outer,47:number,5,49,1x345,function,49,67,undefined69,70,72,68,undefined,outer-id,outer-a,outer-b,outer-tail,outer-others"##,
    );
}

#[test]
fn a_list_with_an_expression_starts_the_body_var_with_the_parameter_value() {
    // Initializers of the list itself, a pattern with a computed key or an
    // initializer, and patterns after a parameter with an initializer. A
    // closure created in the list reads and writes the parameter's binding
    // while the body reads and writes its own. Names of the list stay
    // uninitialized until their own initialization, and getters and iterators
    // of the argument run once, before the body.
    run_in_every_goal_and_mode(
        r##"
var log = [];
function show(f) { try { return String(f()); } catch (e) { return e.constructor.name; } }
var a = "outer-a", b = "outer-b";
// A list with an expression: the body's `var` is a second binding that
// starts with the parameter's value, and a closure of the list keeps the
// parameter's.
function listDefault({a} = {a: 1}) { var a; return a; }
function listDefaultInitialized({a} = {a: 1}) { var a = 2; return a; }
function closureInList(a, g = () => a) { var a = 2; return a + ":" + g(); }
function closureInListUnwritten(a, g = () => a) { var a; return a + ":" + g(); }
function closureInListPattern({a}, g = () => a) { var a; var r = a; a = 3; return r + ":" + a + ":" + g(); }
function closureInListArray([a], g = () => a) { var a; a++; return a + ":" + g(); }
function closureWritesList({a}, set = (v) => { a = v; }, get = () => a) { var a; set(9); return a + ":" + get(); }
function laterPattern(b = 1, {a}) { var a; return a + b; }
function laterArray(b = 1, [a]) { var a; return a + b; }
function laterRest(b = 1, ...a) { var a; return a.join("|"); }
function laterRestPattern(b = 1, ...[a, c]) { var a, c; return a + c; }
function patternWithComputedKey({["a"]: a}) { var a; return a; }
function patternWithInitializer({a = 5, b}) { var a, b; return a + b; }
function arrayWithInitializer([a = 5, b]) { var b, a; return a + b; }
function simpleDefault(a = 1) { var a; return a; }
function bodyClosure({a} = {a: 1}) { var a; var g = () => a; a = 4; return g(); }
function defaultSeesParameter({a}, b = a) { var a = 5; return a + ":" + b; }
log.push("lists", listDefault(), listDefault({a: 2}), listDefaultInitialized(), closureInList(7), closureInListUnwritten(8), closureInListPattern({a: 8}), closureInListArray([46]), closureWritesList({a: 1}), laterPattern(undefined, {a: 62}), laterArray(undefined, [63]), laterRest(undefined, 6, 4), laterRestPattern(undefined, 6, 4), patternWithComputedKey({a: 64}), patternWithInitializer({b: 1}), arrayWithInitializer([undefined, 2]), simpleDefault(), simpleDefault(89), bodyClosure(), defaultSeesParameter({a: 33}));
// A function declaration of the name wins in the second binding only.
function functionWins({a} = {a: 93}) { function a() {} return typeof a; }
function functionWinsVar(a = 93) { var a; function a() {} return typeof a; }
function functionWinsClosure(a = 93, b = () => typeof a) { var a; function a() {} return typeof a + ":" + b(); }
function functionWinsPattern({a}, b = () => typeof a) { function a() {} return typeof a + ":" + b(); }
log.push("functions", functionWins(), functionWinsVar(), functionWinsClosure(), functionWinsPattern({a: 1}));
// The names of the list are uninitialized until their own initialization.
function early({a = b, b}) { var a, b; return a + b; }
function earlyInList(a = b, {b}) { var b; return a + b; }
function earlySelf({a = a}) { var a; return a; }
function earlyArray([a = b, b]) { var a, b; return a + b; }
function earlyClosure({a = () => b, b}) { var b; return a() + b; }
function earlyCall({a = (() => b)(), b}) { var b; return a + b; }
log.push("early", show(function () { return early({b: 1}); }), show(function () { return early({a: 1, b: 2}); }), show(function () { return earlyInList(undefined, {b: 1}); }), show(function () { return earlyInList(1, {b: 2}); }), show(function () { return earlySelf({}); }), show(function () { return earlySelf({a: 3}); }), show(function () { return earlyArray([]); }), show(function () { return earlyArray([1, 2]); }), show(function () { return earlyClosure({b: 2}); }), show(function () { return earlyCall({b: 2}); }), show(function () { return earlyCall({a: 1, b: 2}); }));
// A getter of the destructured value runs before the body, once.
var reads = [];
function fromGetter({a, b}) { var a; reads.push("body"); var b = b + 1; return a + b; }
log.push("getter", fromGetter({ get a() { reads.push("a"); return 1; }, get b() { reads.push("b"); return 2; } }), reads.join("|"));
function fromIterator([a, b]) { var a, b; reads.push("body"); return a + b; }
reads.length = 0;
log.push(fromIterator({ [Symbol.iterator]: function () { var n = 0; return { next: function () { reads.push("next"); return { value: ++n, done: n > 2 }; }, return: function () { reads.push("return"); return {}; } }; } }), reads.join("|"));
log.push(a, b);
print(log.join());
"##,
        r##"lists,1,2,2,2:7,8:8,8:3:8,47:46,1:9,63,64,6|4,10,64,6,7,1,89,4,5:33,functions,function,function,function:number,function:number,early,ReferenceError,3,ReferenceError,3,ReferenceError,3,ReferenceError,3,4,ReferenceError,3,getter,4,a|b|body,3,next|next|return|body,outer-a,outer-b"##,
    );
}

#[test]
fn every_callable_kind_keeps_a_pattern_bound_name_under_a_body_var() {
    // Methods, accessors, generator methods, async methods and private
    // methods of object literals and classes, base and derived constructors,
    // a constructor function, generators across their suspensions, and async
    // functions, arrows and generators across their awaits.
    run_in_every_goal_and_mode(
        r##"
var log = [];
var a = "outer-a", value = "outer-value";
var object = {
  method({a}) { var a; return a; },
  set value({a}) { var a; this.stored = a; },
  get value() { var value; return typeof value + this.stored; },
  *generator([a]) { var a; yield a; a++; yield a; },
  async asyncMethod({a}) { var a; await null; return a; },
  arrow: ([a], {b}) => { var a, b; return a + b; },
  ["computed"]({a: {b}}) { var b; return b; },
};
class Holder {
  constructor({a}, [b]) { var a, b; this.sum = a + b; }
  method({a}) { var a; return a; }
  static make({a}) { var a; return new Holder({a: a}, [a]); }
  get value() { var value; return typeof value + this.sum; }
  set value([a]) { var a; this.sum = a; }
  *generator({a}) { var a; yield a; }
  static async asyncMethod([a]) { var a; await null; return a; }
  #hidden({a}) { var a; return a; }
  callHidden(v) { return this.#hidden({a: v}); }
}
class Derived extends Holder {
  constructor({a}) { var a; super({a: a}, [1]); this.own = a; }
  method({a}) { var a; return super.method({a: a + 1}); }
}
function* generator({a}) { var a; yield a; a++; yield a; }
function* generatorClosure({a}) { var a; yield () => a; a = "later"; yield a; }
function* generatorInitialized([a]) { var a = a + 1; yield a; }
function Constructor({a}) { var a; this.a = a; }
log.push("methods", object.method({a: 26}), (object.value = {a: 27}, object.value), object.arrow([1], {b: 2}), object.computed({a: {b: 3}}));
var objectGenerator = object.generator([30]);
log.push(objectGenerator.next().value, objectGenerator.next().value, objectGenerator.next().done);
var holder = new Holder({a: 28}, [1]);
log.push("classes", holder.sum, holder.method({a: 29}), Holder.make({a: 4}).sum, holder.value, (holder.value = [31], holder.sum), holder.generator({a: 32}).next().value, holder.callHidden(33));
var derived = new Derived({a: 5});
log.push(derived.sum, derived.own, derived.method({a: 6}), new Constructor({a: 34}).a);
var running = generator({a: 30});
log.push("generators", running.next().value, running.next().value, running.next().done);
var closing = generatorClosure({a: "first"}), read = closing.next().value;
log.push(read(), closing.next().value, read(), generatorInitialized([7]).next().value);
async function asyncFunction({a}) { var a; await null; return a; }
async function asyncInitialized({a}) { await null; var a = a + 1; await null; return a; }
async function asyncClosure([a]) { var a; var g = () => a; await null; a++; return g(); }
async function* asyncGenerator([a]) { var a; yield a; await null; yield a + 1; }
var asyncArrow = async ({a}) => { var a; await null; return a; };
async function asyncDefault({a} = {a: 1}, g = () => a) { var a; await null; a++; return a + ":" + g(); }
(async function () {
  log.push("async", await asyncFunction({a: 78}), await asyncInitialized({a: 1}), await asyncClosure([5]), await asyncArrow({a: 79}), await object.asyncMethod({a: 80}), await Holder.asyncMethod([81]), await asyncDefault());
  var stream = asyncGenerator([40]);
  log.push((await stream.next()).value, (await stream.next()).value, (await stream.next()).done);
  log.push(a, value);
})().then(function () { print(log.join()); }, function (error) { print("rejected:" + error); });
"##,
        r##"methods,26,undefined27,3,3,30,31,true,classes,29,29,8,undefined29,31,32,33,6,5,7,34,generators,30,31,true,first,later,later,8,async,78,2,6,79,80,81,2:1,40,41,true,outer-a,outer-value"##,
    );
}

#[test]
fn sloppy_arguments_eval_and_with_share_the_parameter_binding() {
    // `var arguments` keeps the arguments object (`parameterBindings` holds
    // the implicit name) unless a formal binds `arguments`, whose value it
    // keeps instead. Direct eval reads, writes and redeclares the one binding,
    // and `with` and `delete` resolve it. None of this is module code.
    run_as_script_and_function(
        r##"
var log = [];
function show(f) { try { return String(f()); } catch (e) { return e.constructor.name; } }
var a = "outer-a";
// `arguments` as a body `var` keeps the arguments object, and a parameter of
// that name keeps its value.
function argumentsVar({a}) { var arguments; return typeof arguments + arguments.length + a; }
function argumentsVarSimple(a) { var arguments; return typeof arguments + arguments.length + a; }
function argumentsInitialized({a}) { var arguments = 5; return arguments + a; }
function argumentsAssigned({a}) { var arguments; arguments = 6; return arguments + a; }
function argumentsPattern({arguments}) { var arguments; return arguments; }
function argumentsArrayPattern([arguments]) { var arguments; return arguments; }
function argumentsPatternAlone({arguments}) { return arguments; }
function argumentsSimple(arguments) { var arguments; return arguments; }
function argumentsFunction({a}) { function arguments() {} return typeof arguments; }
function argumentsInList({a}, b = arguments.length) { var arguments; return typeof arguments + b; }
function argumentsWithDefault({a}, b = 1) { var arguments; return typeof arguments + arguments.length; }
function argumentsRead({a}) { var a; return arguments[0].a + a; }
function argumentsUnmapped({a}, b) { var a, b; b = 9; return arguments[1] + a + b; }
var argumentsArrow = ({a}) => { var arguments; return typeof arguments + a; };
function argumentsOfArrow() { var k = ({a}) => { var arguments; return typeof arguments + a; }; return k({a: 1}); }
log.push("arguments", argumentsVar({a: 1}), argumentsVarSimple(1), argumentsInitialized({a: 1}), argumentsAssigned({a: 1}), argumentsPattern({arguments: 13}), argumentsArrayPattern([14]), argumentsPatternAlone({arguments: 15}), argumentsSimple(14), argumentsFunction({a: 1}), argumentsInList({a: 1}), argumentsWithDefault({a: 1}), argumentsRead({a: 25}), argumentsUnmapped({a: 1}, 2), argumentsArrow({a: 1}), argumentsOfArrow(1, 2));
// Direct eval reads and writes the same binding, and declares into it.
function evalReads({a}) { var a; return eval("a"); }
function evalIsPresent({a}) { var a; eval(""); return a; }
function evalBefore({a}) { eval("var b = a"); var a; return b; }
function evalRedeclares({a}) { var a; return eval("var a; a"); }
function evalInitializes({a}) { eval("var a = 87"); return a; }
function evalFunction({a}) { eval("function a() {}"); return typeof a; }
function evalSimple(a) { eval("var a"); return a; }
function evalInList({a}, b = eval("a")) { var a; return a + b; }
function evalWrites([a]) { var a; eval("a = a + 1"); return a; }
function evalClosure({a}) { var a; return eval("(function () { return a; })"); }
log.push("eval", evalReads({a: 83}), evalIsPresent({a: 84}), evalBefore({a: 85}), evalRedeclares({a: 86}), evalInitializes({a: 1}), evalFunction({a: 1}), evalSimple(88), evalInList({a: 45}), evalWrites([4]), evalClosure({a: 6})());
log.push(show(function () { return (function ({a}, b = eval("var a = 3; a")) { var a; return a + b; })({a: 45}); }), show(function () { return (function (a, b = eval("var a = 3; a")) { return a + b; })(45); }));
// A function declaration over an identifier formal. Sloppy only: the strict
// form is the hoisted-function local-storage crash listed in current work.
function functionWinsSimple(a) { var a; function a() {} return typeof a; }
function functionWinsSimpleAlone(a, b) { function a() {} return typeof a + b; }
log.push("functions", functionWinsSimple(1), functionWinsSimpleAlone(1, 2));
// `with` and `delete` on the one binding.
function withBody({a}) { var a; with ({}) { return a; } }
function withShadow({a}) { with ({a: "object"}) { var a = "written"; } return a; }
function deleted({a}) { var a; return delete a; }
function strictInner() { "use strict"; return function ({a}, [b]) { var a, b; return a + b; }; }
log.push("with", withBody({a: 39}), withShadow({a: 40}), deleted({a: 1}), strictInner()({a: 1}, [2]), a);
print(log.join());
"##,
        r##"arguments,object11,object11,6,7,13,14,15,14,function,object1,object1,50,12,undefined1,undefined1,eval,83,84,85,86,87,function,88,90,5,6,SyntaxError,SyntaxError,functions,function,function2,with,39,40,false,3,outer-a"##,
    );
}

#[test]
fn a_direct_eval_var_of_a_parameter_name_follows_the_function_record() {
    // The one test here whose sloppy expectation is not Node's. Node 24
    // prints `undefined,undefined,13,undefined` for the sloppy runs: V8
    // gives every non-simple parameter list a separate body scope, so its
    // eval declares a fresh `a` there. In ECMA-262 only a list that holds
    // an expression has that second record (10.2.11 steps 27 and 28). For
    // the first two functions varEnv is the record that holds the
    // parameter, so EvalDeclarationInstantiation (19.2.1.3) finds `a` in it
    // and creates nothing, and the value stays. The last list holds an
    // expression, so its eval declares a new `a` in the body record, which
    // is undefined. Strict eval has its own record, and there Node agrees:
    // `11,12,13,14`.
    let source = r##"
var log = [];
function objectPattern({a}) { eval("var a"); return a; }
function arrayPattern([a]) { eval("var a"); return a; }
function simple(a) { eval("var a"); return a; }
function listExpression({a}, b = 1) { eval("var a"); return a; }
log.push(String(objectPattern({a: 11})), String(arrayPattern([12])), String(simple(13)), String(listExpression({a: 14})));
print(log.join());
"##;
    run_as_script_and_function(source, "11,12,13,undefined");
    run(&format!("\"use strict\";\n{source}"), false, "11,12,13,14");
    run(source, true, "11,12,13,14");
}
