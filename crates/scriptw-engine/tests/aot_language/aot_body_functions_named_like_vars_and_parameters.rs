//! A function declaration at the root of a function body is a var-scoped
//! declaration. FunctionDeclarationInstantiation (ECMA-262 10.2.11) puts its
//! name in `varNames`, so the binding it writes is the one a `var` of the
//! name declares (step 27 or 28) or the one a parameter of the name already
//! owns, and step 36 sets that binding to the function object with
//! SetMutableBinding. The last declaration of a name is the one that stays,
//! and the binding is never uninitialized. Lowering used to spell the
//! declaration as a lexical `let`, and the emitter stopped where the storage
//! was a hoisted `var` local or a sealed parameter local, neither of which
//! has an initialization flag to clear.
//! Expectations are Node 24's, taken by running each source as a sloppy
//! Script and as function code, and as a strict Script and as a module
//! where the source is valid there.
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
fn a_root_function_declaration_replaces_a_var_or_a_parameter_of_its_name() {
    // No source here names `arguments` or calls eval, so every binding that
    // no closure captures is a Wasm local: the hoisted `var` local and the
    // parameter local are the storage the emitter used to refuse. The `var`
    // stands before and after the declaration, with and without an
    // initializer, and inside every statement that can hold one. The
    // parameter is an identifier, a rest parameter or a pattern's name. A
    // list with an expression gives the body its own record. Declarations
    // repeat, and closures move the binding into an environment cell. The
    // surrounding code declares the same names, as Script globals, as an
    // enclosing function's variables and as module variables.
    run_in_every_goal_and_mode(
        r##"
var log = [];
function show(f) { try { return String(f()); } catch (e) { return e.constructor.name; } }
function churn() { var keep = []; for (var i = 0; i < 20000; i++) keep[i & 63] = { i: i, a: [i, i + 1] }; return keep.length; }
// The code around the functions declares the same names.
var x = "outer-x", a = "outer-a", b = "outer-b";
// Over a `var`, wherever the `var` stands and whatever it holds.
function varThenFunction() { var x; function x() {} return typeof x; }
function functionThenVar() { function x() {} var x; return typeof x; }
function initializedVar() { var x = 1; function x() {} return typeof x; }
function initializedLater() { var before = typeof x; function x() {} var x = 1; return before + ":" + typeof x; }
function readFirst() { var before = x; var x; function x() { return 7; } return before === x && before(); }
function varTwice() { var x; var x; function x() {} { var x; } return typeof x; }
function inFor() { for (var x; false;) {} function x() {} return typeof x; }
function inForOf() { for (var x of []) {} function x() {} return typeof x; }
function inForOfRuns() { for (var x of [5]) {} function x() {} return x; }
function inForIn() { for (var x in {}) {} function x() {} return typeof x; }
function inBlock() { { var x = 1; } function x() {} return x; }
function inIf() { if (false) { var x = 1; } function x() {} return typeof x; }
function inCatch() { try { throw 1; } catch (e) { var x; } function x() {} return typeof x; }
function inSwitch() { switch (1) { case 1: var x; } function x() {} return typeof x; }
function inLabel() { label: var x; function x() {} return typeof x; }
function varPattern() { var {x} = {x: 1}; function x() {} return x; }
function varArrayPattern() { var [x] = [2]; function x() {} return x; }
function assigned() { var x; function x() {} x = 3; return x; }
function updated() { var x; function x() {} x++; return String(x); }
function called() { var x; function x(v) { return v + 1; } return x(4); }
function recursive(n) { var down; function down(k) { return k ? k + down(k - 1) : 0; } return down(n); }
function named() { var x; function x() {} return x.name + x.length; }
function unrelated() { var y; function x() {} return typeof y + typeof x; }
log.push("var", varThenFunction(), functionThenVar(), initializedVar(), initializedLater(), readFirst(), varTwice(), inFor(), inForOf(), inForOfRuns(), inForIn(), inBlock(), inIf(), inCatch(), inSwitch(), inLabel(), varPattern(), varArrayPattern(), assigned(), updated(), called(), recursive(4), named(), unrelated());
// Over a parameter that an identifier binds.
function overParameter(a) { function a() {} return typeof a; }
function overParameterAndVar(a) { var a; function a() {} return typeof a; }
function overInitializedVar(a) { var a = a; function a() {} return typeof a; }
function overSecond(a, b) { function b() {} return typeof a + typeof b; }
function overBoth(a, b) { function b() {} function a() {} return typeof a + typeof b; }
function overRest(...a) { function a() {} return typeof a; }
function overRestAndVar(b, ...a) { var a; function a() {} return typeof a + b; }
function overUnused(a) { function a() { return 9; } return a(); }
function parameterAssigned(a) { function a() {} a = 5; return a; }
function parameterBefore(a) { var before = a; function a() {} return typeof before; }
log.push("parameter", overParameter(1), overParameterAndVar(1), overInitializedVar(1), overSecond(1, 2), overBoth(1, 2), overRest(1), overRestAndVar(1, 2), overUnused(1), parameterAssigned(1), parameterBefore(1));
// Over a name that a pattern binds, with and without a `var`.
function overObjectPattern({a}) { function a() {} return typeof a; }
function overObjectPatternAndVar({a}) { var a; function a() {} return typeof a; }
function overArrayPattern([a]) { function a() {} return typeof a; }
function overArrayPatternAndVar([a]) { var a = 1; function a() {} return a; }
function overNested({p: [a, {b}]}) { function b() {} return a + typeof b; }
function overPatternRest({a, ...b}, [...x]) { function b() {} function x() {} return a + typeof b + typeof x; }
log.push("pattern", overObjectPattern({a: 1}), overObjectPatternAndVar({a: 1}), overArrayPattern([1]), overArrayPatternAndVar([2]), overNested({p: [1, {b: 2}]}), overPatternRest({a: 1, c: 2}, [3]));
// A list with an expression gives the body its own record (step 28): the
// declaration replaces the body's binding and a closure of the list keeps
// the parameter's.
function overDefault(a = 1) { function a() {} return typeof a; }
function overDefaultAndVar(a = 1) { var a; function a() {} return typeof a; }
function overDefaultPattern({a} = {a: 1}) { function a() {} return typeof a; }
function listClosure(a, g = () => typeof a) { function a() {} return typeof a + ":" + g(); }
function listClosureAndVar(a, g = () => typeof a) { var a; function a() {} return typeof a + ":" + g(); }
function listClosureWrites(a, set = (v) => { a = v; }, get = () => a) { function a() {} set(6); return typeof a + ":" + get(); }
function laterDefault(a, b = 1) { var x; function x() {} function a() {} return typeof x + typeof a + b; }
log.push("lists", overDefault(), overDefaultAndVar(2), overDefaultPattern(), listClosure(1), listClosureAndVar(1), listClosureWrites(1), laterDefault(1));
// The last declaration of a name is the one that stays.
function twice() { function x() { return 1; } function x() { return 2; } return x(); }
function twiceOverVar() { var x; function x() { return 1; } function x() { return 2; } return x(); }
function twiceOverParameter(a) { function a() { return 1; } function a() { return 2; } return a(); }
function twiceAround() { function x() { return 1; } var seen = x(); function x() { return 2; } return seen; }
function otherKinds(a) { var g; function* g() {} async function a() {} return Object.prototype.toString.call(g) + Object.prototype.toString.call(a); }
function mixedKinds() { var x; function x() {} async function* x() {} return Object.prototype.toString.call(x); }
log.push("last", twice(), twiceOverVar(), twiceOverParameter(0), twiceAround(), otherKinds(1), mixedKinds());
// A closure or an inner function moves the binding into an environment cell.
function closureOverVar() { var x; function x() {} return () => typeof x; }
function closureOverParameter(a) { function a() {} return () => typeof a; }
function closureWrites(a) { var x; function x() {} function a() {} var set = () => { x = 1; a = 2; }; set(); return x + a; }
function innerReads(a) { var x; function x() { return typeof a; } function a() { return typeof x; } return x() + a(); }
function counter(count) { function count() {} var calls = 0; return { next: function () { return typeof count + ++calls; } }; }
var kept = closureOverVar(), keptParameter = closureOverParameter({deep: 1}), counting = counter(10);
churn();
log.push("closures", kept(), keptParameter(), closureWrites(0), innerReads(0), counting.next(), counting.next());
// A function expression's own name, and names the enclosing code declares.
var self = function self() { function self() { return 1; } return self(); };
var selfVar = function self(self) { var self; function self() { return 2; } return self(); };
var selfKept = function self() { var x; function x() {} return typeof self + typeof x; };
function shadowsOuter() { function x() {} function a() {} return typeof x + typeof a + typeof b; }
log.push("names", self(), selfVar(0), selfKept(), shadowsOuter(), x, a, b);
print(log.join());
"##,
        r##"var,function,function,number,function:number,7,function,function,function,5,function,1,function,function,function,function,1,2,3,NaN,5,10,x0,undefinedfunction,parameter,function,function,function,numberfunction,functionfunction,function,function1,9,5,function,pattern,function,function,function,1,1function,1functionfunction,lists,function,function,function,function:number,function:number,function:6,functionfunction1,last,2,2,2,2,[object GeneratorFunction][object AsyncFunction],[object AsyncGeneratorFunction],closures,function,function,3,functionfunction,function1,function2,names,1,2,functionfunction,functionfunctionstring,outer-x,outer-a,outer-b"##,
    );
}

#[test]
fn every_callable_kind_instantiates_its_root_function_declarations() {
    // Arrow functions, methods, accessors, generator and async methods,
    // private methods and field initializers, base and derived constructors,
    // a static block, a constructor function, generators across their
    // suspensions, and async functions, arrows and generators across their
    // awaits. A resumable body keeps its bindings in activation cells, which
    // the declaration must fill once, on the first entry only.
    run_in_every_goal_and_mode(
        r##"
var log = [];
var x = "outer-x", a = "outer-a";
var arrow = (a) => { var x; function x() {} function a() {} return typeof a + typeof x; };
var arrowRest = (...a) => { function a() {} return typeof a; };
var arrowPattern = ({a}, [x]) => { function x() {} function a() {} return typeof a + typeof x; };
var object = {
  method(a) { var x; function x() {} function a() {} return typeof a + typeof x; },
  get value() { var x; function x() {} return typeof x + this.stored; },
  set value(a) { var x; function x() {} function a() {} this.stored = typeof a + typeof x; },
  *generator(a) { var x; function x() {} function a() {} yield typeof a; yield typeof x; },
  async asyncMethod(a) { var x; function x() {} function a() {} await null; return typeof a + typeof x; },
  ["computed"](a) { function a() { return 3; } return a(); },
};
class Holder {
  constructor(a) { var x; function x() {} function a() {} this.kinds = typeof a + typeof x; }
  method(a) { var x; function x() {} function a() {} return typeof a + typeof x; }
  static make(a) { function a() {} return typeof a; }
  get value() { var x; function x() {} return typeof x; }
  set value(a) { function a() {} this.kinds = typeof a; }
  *generator(a) { var x; function x() {} function a() {} yield typeof a + typeof x; }
  static async asyncMethod(a) { var x; function x() {} function a() {} await null; return typeof a + typeof x; }
  #hidden(a) { var x; function x() {} function a() {} return typeof a + typeof x; }
  callHidden() { return this.#hidden(1); }
  field = (a) => { var x; function x() {} function a() {} return typeof a + typeof x; };
  static block = "unset";
  static { function local() { return "block"; } var y; Holder.block = local() + typeof y; }
}
class Derived extends Holder {
  constructor(a) { var x; function x() {} function a() {} super(1); this.own = typeof a + typeof x; }
  method(a) { function a() {} return super.method(1) + typeof a; }
}
function* generator(a) { var x; function x() {} function a() {} yield typeof a; x = 1; yield typeof x; yield typeof a; }
function* generatorKeeps(a) { var x; function x() { return a; } function a() { return 4; } yield 0; yield x()(); }
function Constructor(a) { var x; function x() {} function a() {} this.kinds = typeof a + typeof x; }
log.push("arrows", arrow(1), arrowRest(1), arrowPattern({a: 1}, [2]));
log.push("methods", object.method(1), (object.value = 1, object.value), object.computed(1));
var objectGenerator = object.generator(1);
log.push(objectGenerator.next().value, objectGenerator.next().value, objectGenerator.next().done);
var holder = new Holder(1);
log.push("classes", holder.kinds, holder.method(1), Holder.make(1), holder.value, (holder.value = 1, holder.kinds), holder.generator(1).next().value, holder.callHidden(), holder.field(1), Holder.block);
var derived = new Derived(1);
log.push(derived.kinds, derived.own, derived.method(1), new Constructor(1).kinds);
var running = generator(1);
log.push("generators", running.next().value, running.next().value, running.next().value, running.next().done);
var keeping = generatorKeeps(1);
keeping.next();
log.push(keeping.next().value);
async function asyncFunction(a) { var x; function x() {} function a() {} await null; return typeof a + typeof x; }
async function asyncBefore(a) { function a() {} var before = typeof a; await null; a = 1; await null; return before + typeof a; }
async function* asyncGenerator(a) { var x; function x() {} function a() {} yield typeof a; await null; yield typeof x; }
var asyncArrow = async (a) => { var x; function x() {} function a() {} await null; return typeof a + typeof x; };
async function asyncDefault(a = 1, g = () => typeof a) { function a() {} await null; return typeof a + ":" + g(); }
(async function () {
  log.push("async", await asyncFunction(1), await asyncBefore(1), await asyncArrow(1), await object.asyncMethod(1), await Holder.asyncMethod(1), await asyncDefault());
  var stream = asyncGenerator(1);
  log.push((await stream.next()).value, (await stream.next()).value, (await stream.next()).done);
  log.push(x, a);
})().then(function () { print(log.join()); }, function (error) { print("rejected:" + error); });
"##,
        r##"arrows,functionfunction,function,functionfunction,methods,functionfunction,functionfunctionfunction,3,function,function,true,classes,functionfunction,functionfunction,function,function,function,functionfunction,functionfunction,functionfunction,blockundefined,functionfunction,functionfunction,functionfunctionfunction,functionfunction,generators,function,number,function,true,4,async,functionfunction,functionnumber,functionfunction,functionfunction,functionfunction,function:number,function,function,true,outer-x,outer-a"##,
    );
}

#[test]
fn sloppy_labelled_and_block_functions_share_the_root_declaration_binding() {
    // Sloppy code only. A labelled function declaration at the root is
    // hoisted like a direct one. A block-level function of the same name is
    // a different mechanism, the Annex B.3.2.1 copy into the var binding
    // when the block is evaluated: it gave the name hoisted `var` storage,
    // which the root declaration then met.
    run_as_script_and_function(
        r##"
var log = [];
var x = "outer-x";
// A labelled function declaration is hoisted like a direct one.
function labelled() { var x; label: function x() {} return typeof x; }
function labelledOverParameter(a) { label: function a() {} return typeof a; }
// A block's function of the name is copied over the binding when the block
// runs (Annex B.3.2.1), after the root declaration has filled it.
function blockThenRoot() { { function x() { return 1; } } function x() { return 2; } return x(); }
function rootThenBlock() { function x() { return 2; } var before = x(); { function x() { return 1; } } return before + ":" + x(); }
function blockNotRun() { if (false) { function x() { return 1; } } function x() { return 2; } return x(); }
function blockOverVar() { var x; { function x() {} } return typeof x; }
function blockUnderParameter(x) { { function x() {} } return typeof x; }
function blockRootAndVar() { var x = 0; { function x() { return 1; } } function x() { return 2; } return x(); }
function inSwitch() { switch (1) { case 1: function x() { return 1; } } function x() { return 2; } return x(); }
// Sloppy parameters named twice bind once.
function duplicateParameters(a, a) { function a() {} return typeof a; }
function deleted(a) { var x; function x() {} function a() {} return delete x || delete a; }
log.push(labelled(), labelledOverParameter(1), blockThenRoot(), rootThenBlock(), blockNotRun(), blockOverVar(), blockUnderParameter(1), blockRootAndVar(), inSwitch(), duplicateParameters(1, 2), deleted(1), x);
print(log.join());
"##,
        r##"function,function,1,2:1,2,function,number,1,1,function,false,outer-x"##,
    );
}

#[test]
fn sloppy_arguments_eval_and_with_see_the_root_declaration_binding() {
    // A source that names `arguments` or calls eval keeps every binding in
    // environment cells, the storage that worked before. A function named
    // `arguments` leaves no arguments object unless the list holds an
    // expression (step 18), and one over a parameter shows through the
    // mapped arguments object. None of this is module code.
    run_as_script_and_function(
        r##"
var log = [];
var a = "outer-a";
// A function named `arguments` replaces the arguments object, which then is
// never created (step 18), and one over a parameter shows through the mapped
// arguments object.
function namedArguments() { function arguments() {} return typeof arguments; }
function namedArgumentsOverVar() { var arguments; function arguments() {} return typeof arguments; }
function namedArgumentsOverParameter(arguments) { function arguments() {} return typeof arguments; }
function namedArgumentsInList(a = typeof arguments) { function arguments() {} return a + typeof arguments; }
var arrowArguments = () => { var arguments; function arguments() {} return typeof arguments; };
function mapped(a) { function a() {} return typeof arguments[0] + arguments.length; }
function mappedWrites(a) { function a() {} arguments[0] = 5; return a; }
function unmapped(a, ...rest) { function a() {} return typeof arguments[0]; }
function strictUnmapped(a) { "use strict"; function a() {} return typeof arguments[0]; }
log.push("arguments", namedArguments(1), namedArgumentsOverVar(1), namedArgumentsOverParameter(1), namedArgumentsInList(), arrowArguments(), mapped(1), mappedWrites(1), unmapped(1), strictUnmapped(1));
// Direct eval sees, writes and redeclares the one binding.
function evalReads(a) { var x; function x() {} function a() {} return eval("typeof x + typeof a"); }
function evalWrites(a) { var x; function x() {} function a() {} eval("x = 1; a = 2"); return x + a; }
function evalDeclaresVar(a) { function a() {} eval("var a"); return typeof a; }
function evalDeclaresFunction(a) { var x = 1; eval("function x() {} function a() {}"); return typeof x + typeof a; }
function evalBoth() { eval("var x; function x() {}"); return typeof x; }
function evalStrict() { return eval("'use strict'; var x; function x() {} typeof x") + typeof x; }
function evalClosure(a) { function a() {} return eval("(function () { return typeof a; })"); }
log.push("eval", evalReads(1), evalWrites(1), evalDeclaresVar(1), evalDeclaresFunction(1), evalBoth(), evalStrict(), evalClosure(1)());
// `with` resolves the name to the function's binding.
function withBody(a) { var x; function x() {} function a() {} with ({}) { return typeof x + typeof a; } }
function withShadow(a) { function a() {} with ({a: "object"}) { a = "written"; } return typeof a; }
log.push("with", withBody(1), withShadow(1), a);
print(log.join());
"##,
        r##"arguments,function,function,function,objectfunction,function,function1,5,number,number,eval,functionfunction,3,function,functionfunction,function,functionundefined,function,with,functionfunction,function,outer-a"##,
    );
}

#[test]
fn eval_and_dynamic_function_code_instantiate_their_root_declarations() {
    // Eval code is var-scoped the same way (EvalDeclarationInstantiation,
    // 19.2.1.3): sloppy direct eval declares into the caller's variable
    // environment, and strict or indirect strict eval into a record of its
    // own. Function code that eval or the Function constructor creates is
    // function code like any other.
    run_in_every_goal_and_mode(
        r##"
var log = [];
// Eval code with a variable environment of its own, and function code that
// dynamic source creates.
log.push(eval("var x; function x() {} typeof x"));
log.push(eval("function y() { return 1; } function y() { return 2; } var y; y()"));
log.push((0, eval)("'use strict'; var x; function x() {} typeof x"));
log.push(eval("(function (a) { var x; function x() {} function a() {} return typeof a + typeof x; })")(1));
log.push((0, eval)("(function (a) { 'use strict'; var x; function x() {} function a() {} return typeof a + typeof x; })")(1));
log.push(new Function("a", "var x; function x() {} function a() {} return typeof a + typeof x;")(1));
log.push(new Function("a", "'use strict'; var x; function x() {} function a() {} return typeof a + typeof x;")(1));
print(log.join());
"##,
        r##"function,2,function,functionfunction,functionfunction,functionfunction,functionfunction"##,
    );
}

#[test]
fn a_class_static_block_declares_its_functions_as_a_function_body_does() {
    // The LexicallyDeclaredNames of a ClassStaticBlockStatementList are the
    // TopLevelLexicallyDeclaredNames of its StatementList (ECMA-262 8.2.4),
    // so a function declaration at the root of the block is var scoped: it
    // may share its name with a `var` or with another function declaration
    // of the block, and it writes that one binding. The vendored parser
    // used to report both as early errors. A block-level function inside
    // the static block stays in its block, because class code is strict.
    // The conflicts that remain early errors (`let a; function a() {}`,
    // `let a; var a;`) are Test262's `static-init-invalid-lex-dup.js` and
    // `static-init-invalid-lex-var.js`.
    run_in_every_goal_and_mode(
        r##"
var log = [];
class A { static { var x; function x() { return 1; } log.push(typeof x + x()); } }
class B { static { function y() { return 1; } function y() { return 2; } log.push(y()); } }
class C { static { log.push(typeof z); var z = 3; function z() {} log.push(typeof z); } }
class D { static { { function w() { return 4; } } log.push(typeof w); } }
class E { static { var q = 5; { let q = 6; log.push(q); } function q() {} log.push(q); } }
class F { static { var c; function c() { return 7; } F.get = () => c; } }
log.push(F.get()());
class G { static { var g; function* g() { yield 8; } async function h() {} var h; log.push(g().next().value + typeof h); } }
var H = class { static { var k; log.push(typeof k); function k() { return 9; } } static { function k() { return 10; } log.push(k()); } };
class I { static { function i() { return 11; } log.push(i()); } static m() { var i; function i() { return 12; } return i(); } }
log.push(I.m());
print(log.join());
"##,
        "function1,2,function,number,undefined,6,5,7,8function,function,10,11,12",
    );
}
