use scriptw_engine::{CompileOptions, Engine, ExecutionBackend, RealmBuilder, RunOptions};

fn run_boolean(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let engine = Engine::new(RealmBuilder::new().build());
    let outcome = engine
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect("callable lifecycle should execute through Wasm");
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn arguments_aliases_survive_descriptor_changes_and_retire_individually() {
    run_boolean(
        r#"
function frozen(a) {
  var args = arguments;
  a = 2;
  var live = Object.getOwnPropertyDescriptor(args, "0").value === 2;
  Object.defineProperty(args, "0", { writable: false, configurable: false });
  a = 3;
  var rejected = !Reflect.defineProperty(args, "0", { value: 4 });
  return live && rejected && args[0] === 2 && a === 3;
}
function configurableSnapshot(a) {
  var args = arguments;
  a = 2;
  Object.defineProperty(args, "0", { writable: false });
  a = 3;
  var retired = args[0] === 2 && a === 3;
  var changed = Reflect.defineProperty(args, "0", { value: 4 });
  var descriptor = Object.getOwnPropertyDescriptor(args, "0");
  return retired && changed && args[0] === 4 && a === 3 &&
    descriptor.configurable && !descriptor.writable;
}
function accessor(a) {
  var args = arguments;
  Object.defineProperty(args, "0", { get: function () { return 8; } });
  a = 9;
  return args[0] === 8 && a === 9;
}
function removed(a) {
  var args = arguments;
  delete args[0];
  a = 9;
  Object.defineProperty(args, "0", { value: 4, writable: true });
  args[0] = 5;
  return a === 9 && args[0] === 5;
}
function duplicate(a, a) {
  a = 7;
  return arguments[0] === 1 && arguments[1] === 7;
}
function missingDuplicate(a, a) {
  a = 7;
  return arguments[0] === 1 && !Object.hasOwn(arguments, "1");
}
frozen(1) && configurableSnapshot(1) && accessor(1) && removed(1) && duplicate(1, 2) && missingDuplicate(1);
"#,
    );
}

#[test]
fn unmapped_arguments_and_rest_keep_intrinsic_and_value_identities() {
    run_boolean(
        r#"
var originalValues = Array.prototype.values;
Array.prototype.values = function () { throw "mutated values"; };
function strict(a) { "use strict"; a = 9; return arguments; }
function defaults(a = 2) { a = 9; return arguments; }
function rest(first, ...tail) { return tail; }
var first = {}, symbol = Symbol("argument"), large = 12345678901234567890n;
var one = strict(first), two = strict(1), nonSimple = defaults(first);
var strictCallee = Object.getOwnPropertyDescriptor(one, "callee");
var otherCallee = Object.getOwnPropertyDescriptor(two, "callee");
var iterator = Object.getOwnPropertyDescriptor(one, Symbol.iterator);
var length = Object.getOwnPropertyDescriptor(one, "length");
var tail = rest(0, first, symbol, large), empty = rest();
one[0] === first && nonSimple[0] === first &&
strictCallee.get === strictCallee.set && strictCallee.get === otherCallee.get &&
!strictCallee.enumerable && !strictCallee.configurable &&
iterator.value === originalValues && iterator.writable && !iterator.enumerable && iterator.configurable &&
length.value === 1 && length.writable && !length.enumerable && length.configurable &&
Array.isArray(tail) && Object.getPrototypeOf(tail) === Array.prototype &&
tail.length === 3 && tail[0] === first && tail[1] === symbol && tail[2] === large &&
Array.isArray(empty) && empty.length === 0;
"#,
    );
}

#[test]
fn suspended_generator_arguments_retain_the_parameter_cell() {
    run_boolean(
        r#"
function* capture(a) {
  var args = arguments;
  yield args;
  yield a;
  a = 9;
  return args[0];
}

var iterator = capture(1);
var args = iterator.next().value;
args[0] = 7;
var read = iterator.next();
var terminal = iterator.next();
read.value === 7 && !read.done && terminal.value === 9 && terminal.done && args[0] === 9;
"#,
    );
}

#[test]
fn callee_get_precedes_arguments_and_retains_the_original_receiver() {
    run_boolean(
        r#"
var log = "";
var receiver = {
  get method() {
    log += "get;";
    return function (value) { log += "call;"; return this === receiver && value === 7; };
  }
};
function argument() { log += "arg;"; return 7; }
var called = receiver.method(argument());
var original = {};
Object.defineProperty(receiver, "method", { get: function () { throw original; } });
var thrown;
try { receiver.method(argument()); } catch (error) { thrown = error; }
called && log === "get;arg;call;" && thrown === original;
"#,
    );
}

#[test]
fn spread_captures_iterator_effects_once_and_preserves_reference_identity() {
    run_boolean(
        r#"
var first = {}, second = Symbol("identity"), third = 12345678901234567890n;
var iteratorReads = 0, nextReads = 0, steps = 0, calls = 0;
var iterable = {
  get [Symbol.iterator]() {
    iteratorReads++;
    return function () {
      var position = 0;
      return {
        get next() {
          nextReads++;
          return function () {
            steps++;
            position++;
            if (position === 1) return {done: false, value: first};
            if (position === 2) return {done: false, value: second};
            if (position === 3) return {done: false, value: third};
            return {done: true};
          };
        }
      };
    };
  }
};
function target(a, b, c) { calls++; return a === first && b === second && c === third; }
target(...iterable) && iteratorReads === 1 && nextReads === 1 && steps === 4 && calls === 1;
"#,
    );
}

#[test]
fn class_fields_precede_parameter_initializers_and_have_complete_private_methods() {
    run_boolean(
        r#"
var log = "";
class C {
  field = (log += "field;", this.#read());
  #read() { return 17; }
  constructor(value = (log += "parameter;", 3)) {
    log += "body;";
    this.parameter = value;
  }
}
var value = new C();
value.field === 17 && value.parameter === 3 && log === "field;parameter;body;";
"#,
    );
}

#[test]
fn private_methods_can_be_installed_on_a_nonextensible_receiver() {
    run_boolean(
        r#"
var receiver = Object.preventExtensions({});
class Base { constructor() { return receiver; } }
class Derived extends Base {
  #read() { return 29; }
  constructor() { super(); if (this.#read() !== 29) throw "missing private method"; }
}
new Derived() === receiver && Object.isExtensible(receiver) === false;
"#,
    );
}

#[test]
fn derived_fields_reach_the_actual_proxy_definition_dispatcher() {
    run_boolean(
        r#"
var log = "", backing = {};
var receiver = new Proxy(backing, {
  defineProperty: function (target, key, descriptor) {
    log += key + ";";
    if (target !== backing || descriptor.writable !== true
        || descriptor.enumerable !== true || descriptor.configurable !== true) throw "bad descriptor";
    return Reflect.defineProperty(target, key, descriptor);
  }
});
class Base { constructor() { return receiver; } }
class Derived extends Base { field = 31; }
var value = new Derived();
value === receiver && backing.field === 31 && log === "field;";
"#,
    );
}

#[test]
fn computed_class_names_are_converted_once_before_static_initializers() {
    run_boolean(
        r#"
var log = "", converted = 0, tdz = false;
var key = {
  toString: function () { converted++; log += "key;"; return "method"; }
};
var C = class Named {
  [(function () { try { Named; } catch (error) { tdz = error instanceof ReferenceError; } return key; })()]() {
    return 37;
  }
  static field = (log += "static;", Named);
};
converted === 1 && tdz && log === "key;static;" && C.field === C && new C().method() === 37;
"#,
    );
}

#[test]
fn method_home_objects_survive_detachment_and_static_super_calls() {
    run_boolean(
        r#"
class Base {
  method() { return this.value; }
  static method() { return this.value; }
}
class Derived extends Base {
  method() { return super.method() + 1; }
  static method() { return super.method() + 2; }
}
var detached = Derived.prototype.method;
var detachedStatic = Derived.method;
detached.call({value: 40}) === 41 && detachedStatic.call({value: 41}) === 43;
"#,
    );
}

#[test]
fn compiler_errors_have_error_brand_and_inherit_their_name() {
    run_boolean(
        r#"
var error;
try { (class C {})(); } catch (caught) { error = caught; }
var message = Object.getOwnPropertyDescriptor(error, "message");
Object.prototype.toString.call(error) === "[object Error]"
  && Object.getPrototypeOf(error) === TypeError.prototype
  && Object.prototype.hasOwnProperty.call(error, "name") === false
  && error.name === "TypeError"
  && typeof message.value === "string" && message.value.length > 0
  && message.writable === true && message.enumerable === false && message.configurable === true;
"#,
    );
}

#[test]
fn iterator_close_observes_return_and_preserves_an_incoming_throw() {
    run_boolean(
        r#"
var original = {}, closing = {}, gets = 0, calls = 0;
var iterator = { next: function () { return {done: false, value: 1}; } };
var iterable = { [Symbol.iterator]: function () { return iterator; } };
Object.defineProperty(iterator, "return", {
  configurable: true,
  get: function () { gets++; throw closing; }
});
var first;
try { for (var value of iterable) throw original; } catch (error) { first = error; }
Object.defineProperty(iterator, "return", {
  get: function () {
    gets++;
    return function () { calls++; if (this !== iterator) throw "wrong receiver"; throw closing; };
  }
});
var second;
try { for (var value of iterable) throw original; } catch (error) { second = error; }
first === original && second === original && gets === 2 && calls === 1;
"#,
    );
}

#[test]
fn iterator_close_rejects_a_primitive_result_after_a_normal_break() {
    run_boolean(
        r#"
var calls = 0;
var iterator = {
  next: function () { return {done: false, value: 1}; },
  return: function () { calls++; return 0; }
};
var iterable = { [Symbol.iterator]: function () { return iterator; } };
var error;
try { for (var value of iterable) break; } catch (caught) { error = caught; }
calls === 1 && error instanceof TypeError;
"#,
    );
}

#[test]
fn private_accessor_pairs_and_names_stay_distinct_across_class_evaluations() {
    run_boolean(
        r#"
function make(seed) {
  return class {
    #stored = seed;
    get #value() { return this.#stored; }
    set #value(value) { this.#stored = value; }
    static #identity() { return this; }
    read() { return this.#value; }
    write(value) { this.#value = value; }
    static identity() { return this.#identity(); }
  };
}
var A = make(47), B = make(53), a = new A(), b = new B();
a.write(59);
var wrongBrand;
try { A.prototype.read.call(b); } catch (error) { wrongBrand = error; }
a.read() === 59 && b.read() === 53 && A.identity() === A && B.identity() === B
  && wrongBrand instanceof TypeError;
"#,
    );
}

#[test]
fn public_accessor_definitions_merge_and_preserve_method_attributes() {
    run_boolean(
        r#"
class C {
  get value() { return this.stored; }
  set value(value) { this.stored = value; }
  method() { return 1; }
  method() { return 61; }
}
var value = new C();
value.value = 67;
var accessor = Object.getOwnPropertyDescriptor(C.prototype, "value");
var method = Object.getOwnPropertyDescriptor(C.prototype, "method");
value.value === 67 && value.method() === 61
  && accessor.get.name === "get value" && accessor.set.name === "set value"
  && accessor.enumerable === false && accessor.configurable === true
  && method.writable === true && method.enumerable === false && method.configurable === true;
"#,
    );
}

#[test]
fn class_heritage_prototype_get_precedes_computed_keys_and_static_fields() {
    run_boolean(
        r#"
var log = "";
function Base() {}
var Parent = new Proxy(Base, {
  get: function (target, key, receiver) {
    if (key === "prototype") log += "prototype;";
    return Reflect.get(target, key, receiver);
  }
});
var key = {toString: function () { log += "key;"; return "method"; }};
class C extends Parent {
  [key]() { return 71; }
  static value = (log += "static;", 73);
}
log === "prototype;key;static;" && new C().method() === 71 && C.value === 73
  && Object.getPrototypeOf(C) === Parent && Object.getPrototypeOf(C.prototype) === Base.prototype;
"#,
    );
}

#[test]
fn suspended_class_evaluation_retains_its_name_scope_and_prepared_constructor() {
    run_boolean(
        r#"
var tdz = false;
function Base() {}
function* build() {
  var C = class Inner extends (yield "heritage") {
    [((yield "key"), (function () {
      try { Inner; } catch (error) { tdz = error instanceof ReferenceError; }
      return "read";
    })())]() { return this.#value(); }
    #value() { return 79; }
    static self = Inner;
  };
  return C;
}
var iterator = build();
var first = iterator.next(), second = iterator.next(Base), third = iterator.next();
var C = third.value;
first.value === "heritage" && first.done === false
  && second.value === "key" && second.done === false && third.done === true
  && tdz && C.self === C && new C().read() === 79;
"#,
    );
}

#[test]
fn prepared_direct_eval_arrows_share_derived_this_and_home_owners() {
    run_boolean(
        r#"
class Base {
  constructor() { this.base = 5; }
  method() { return this.base + 2; }
}
class Derived extends Base {
  constructor() {
    var read = eval("(() => this)");
    var target = eval("(() => new.target)");
    var early;
    try { read(); } catch (error) { early = error instanceof ReferenceError; }
    eval("super()");
    this.read = read;
    this.target = target;
    this.method = eval("(() => super.method())");
    this.early = early;
  }
}
var instance = new Derived();
instance.early && instance.read() === instance && instance.target() === Derived &&
instance.method() === 7;
"#,
    );
}

#[test]
fn direct_eval_identity_is_checked_after_arguments_and_preserves_whole_throws() {
    run_boolean(
        r#"
var order = "";
var original = {};
function replacement(value) { order += "call;"; return value; }
function invoke(eval) {
  return eval((order += "arg;", original));
}
var returned = invoke(replacement);
function throwFromPreparedEval() { eval("throw original"); }
var thrown;
try { throwFromPreparedEval(); } catch (error) { thrown = error; }
returned === original && order === "arg;call;" && thrown === original;
"#,
    );
}

#[test]
fn dynamic_functions_use_global_scope_and_fresh_template_owners() {
    run_boolean(
        r#"
var scope = 5;
function make() {
  var scope = 9;
  return Function("return scope;");
}
var one = make(), two = make();
var templateOne = Function("return (function (site) { return site; })`identity`;");
var templateTwo = Function("return (function (site) { return site; })`identity`;");
var first = templateOne(), repeat = templateOne(), other = templateTwo();
one !== two && one() === 5 && two() === 5 && one.name === "anonymous" &&
one.prototype.constructor === one && first === repeat && first !== other &&
Object.isFrozen(first) && Object.isFrozen(first.raw);
"#,
    );
}

#[test]
fn dynamic_function_source_coercion_precedes_new_target_prototype_get() {
    run_boolean(
        r#"
var known = Function("value", "return value;");
var order = "";
var parameters = { toString() { order += "parameters;"; return "value"; } };
var body = { toString() { order += "body;"; return "return value;"; } };
var parent = {};
var target = new Proxy(function () {}, {
  get(object, key, receiver) {
    if (key === "prototype") { order += "prototype;"; return parent; }
    return Reflect.get(object, key, receiver);
  }
});
var created = Reflect.construct(Function, [parameters, body], target);
order === "parameters;body;prototype;" && Object.getPrototypeOf(created) === parent &&
created(7) === 7 && known(8) === 8 && created.prototype.constructor === created;
"#,
    );
}

#[test]
fn empty_and_prepared_generator_functions_publish_their_own_instance_prototypes() {
    run_boolean(
        r#"
var sample = function* () {};
var familyPrototype = Object.getPrototypeOf(sample);
var GeneratorFunction = familyPrototype.constructor;
var empty = GeneratorFunction();
var prepared = GeneratorFunction("yield 7; return 8;");
var iterator = prepared();
var first = iterator.next(), terminal = iterator.next(), emptyResult = empty().next();
var emptyOrdinary = Function();
Object.getPrototypeOf(empty) === familyPrototype &&
Object.getPrototypeOf(prepared) === familyPrototype && empty.prototype !== prepared.prototype &&
Object.getPrototypeOf(empty.prototype) === Object.getPrototypeOf(sample.prototype) &&
!Object.hasOwn(empty.prototype, "constructor") && first.value === 7 && !first.done &&
terminal.value === 8 && terminal.done && emptyResult.value === undefined && emptyResult.done &&
emptyOrdinary() === undefined && emptyOrdinary.prototype.constructor === emptyOrdinary;
"#,
    );
}

#[test]
fn prepared_direct_eval_context_survives_resumed_deeper_block() {
    run_boolean(
        r#"
function make() {
  var outer = 9;
  return eval("(function*() { { let inner = 4; yield 1; return eval('outer + inner'); } })");
}
var iterator = make()();
var first = iterator.next();
var last = iterator.next();
(first.value === 1 && !first.done && last.value === 13 && last.done);
"#,
    );
}

#[test]
fn retained_operands_survive_reentry_abrupt_paths_and_suspended_reference_owners() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let outcome = engine
        .run_script(
            include_str!("../fixtures/retained_operands.js"),
            CompileOptions {
                host_surface_policy: scriptw_engine::HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}
