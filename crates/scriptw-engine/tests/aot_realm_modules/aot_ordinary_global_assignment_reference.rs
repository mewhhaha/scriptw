use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, ObservedCompletion,
    ObservedJsValue, RealmBuilder, RunOptions, WasmExecutionFailureKind,
};

fn engine() -> Engine {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    Engine::new(RealmBuilder::new().build())
}

fn options() -> CompileOptions {
    CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    }
}

fn execution() -> RunOptions {
    RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    }
}

fn succeeds(source: &str) {
    let observed = engine()
        .observe_script(source, options(), execution())
        .expect("ordinary global assignment compiles and executes");
    assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(
        observed.completion,
        ObservedCompletion::Normal(ObservedJsValue::Boolean(true)),
        "{source}\n{}",
        observed.note
    );
    assert!(observed.output_events.is_empty());
}

#[test]
fn unresolved_identifier_messages_retain_exact_read_and_write_names() {
    succeeds(
        r#"
const errors = [];
try { new MissingTextEncoder(); } catch (error) { errors.push(error); }
try { missing\u0052untime; } catch (error) { errors.push(error); }
try { café; } catch (error) { errors.push(error); }
function strictWrite() { 'use strict'; missingEndpoint = 1; }
try { strictWrite(); } catch (error) { errors.push(error); }
globalThis.deletedRuntime = 1;
function strictDelete() { 'use strict'; deletedRuntime = (delete globalThis.deletedRuntime, 1); }
try { strictDelete(); } catch (error) { errors.push(error); }
with ({}) {
  try { missingInsideWith; } catch (error) { errors.push(error); }
}
const expected = ['MissingTextEncoder', 'missingRuntime', 'café', 'missingEndpoint',
  'deletedRuntime', 'missingInsideWith'];
errors.length === expected.length && errors.every((error, index) => {
  const descriptor = Object.getOwnPropertyDescriptor(error, 'message');
  return error instanceof ReferenceError && error.message === expected[index] + ' is not defined'
    && descriptor.writable && descriptor.configurable && !descriptor.enumerable;
}) && typeof MissingTextEncoder === 'undefined' && typeof missingEndpoint === 'undefined';
"#,
    );
}

#[test]
fn uncaught_reference_diagnostics_name_missing_runtime_capabilities() {
    for (source, name) in [
        ("new MissingTextEncoder();", "MissingTextEncoder"),
        ("'use strict'; missingEndpoint = 1;", "missingEndpoint"),
    ] {
        let error = engine()
            .run_script(source, options(), execution())
            .expect_err("missing runtime capability must remain a JavaScript exception");
        assert_eq!(
            error.wasm_execution_failure_kind(),
            Some(WasmExecutionFailureKind::JavaScriptException)
        );
        assert_eq!(
            error.wasm_javascript_exception_constructor_name(),
            Some("ReferenceError")
        );
        assert!(
            error.message().contains(&format!("{name} is not defined")),
            "{error}"
        );
    }
}

#[test]
fn pinned_rhs_property_creation_cannot_resolve_an_earlier_strict_reference() {
    let source = concat!(
        "'use strict';\n",
        include_str!(
            "../../../../test262/vendor/test262/test/language/identifier-resolution/assign-to-global-undefined.js"
        )
    );
    let error = engine()
        .run_script(source, options(), execution())
        .expect_err("the original missing Reference remains unresolvable after the RHS");
    assert_eq!(
        error.wasm_execution_failure_kind(),
        Some(WasmExecutionFailureKind::JavaScriptException)
    );
    assert_eq!(
        error.wasm_javascript_exception_constructor_name(),
        Some("ReferenceError")
    );
}

#[test]
fn strict_and_sloppy_put_keep_pre_rhs_resolution_and_exact_rhs_completion() {
    succeeds(
        r#"
let effects = 0, strictError, deletionError, abruptError;
const marker = {};
function strictCreate() { 'use strict'; referenceCreated = (effects++, globalThis.referenceCreated = 5); }
try { strictCreate(); } catch (error) { strictError = error; }
globalThis.referenceDeleted = 1;
function strictDelete() { 'use strict'; referenceDeleted = (effects++, delete globalThis.referenceDeleted, 7); }
try { strictDelete(); } catch (error) { deletionError = error; }
function abrupt() { effects++; throw marker; }
function strictAbrupt() { 'use strict'; referenceAbsent = abrupt(); }
try { strictAbrupt(); } catch (error) { abruptError = error; }
const result = referenceSloppy = (globalThis.referenceSloppy = 3, 9);
globalThis.referenceRecreated = 1;
referenceRecreated = (delete globalThis.referenceRecreated, 11);
strictError instanceof ReferenceError && deletionError instanceof ReferenceError &&
  abruptError === marker && effects === 3 && globalThis.referenceCreated === 5 &&
  !Object.hasOwn(globalThis, 'referenceDeleted') && !Object.hasOwn(globalThis, 'referenceAbsent') &&
  result === 9 && globalThis.referenceSloppy === 9 && globalThis.referenceRecreated === 11;
"#,
    );
}

#[test]
fn script_global_var_uses_the_same_reference_recheck_after_rhs_deletion() {
    succeeds(
        r#"'use strict';
var parseInt;
const original = parseInt;
let caught, effects = 0;
try { parseInt = (effects++, delete globalThis.parseInt, 17); }
catch (error) { caught = error; }
const absent = !Object.hasOwn(globalThis, 'parseInt');
globalThis.parseInt = original;
caught instanceof ReferenceError && effects === 1 && absent && parseInt === original;
"#,
    );
}

#[test]
fn global_prototype_hasbinding_precedes_rhs_and_plain_assignment_does_not_get() {
    for prelude in ["", "'use strict';\n"] {
        succeeds(&format!(
            "{prelude}{}",
            r#"
const trace = [], marker = {};
let observed = 0, fail = false, reads = 0, effects = 0, caught;
const original = Object.getPrototypeOf(globalThis);
const target = { referenceProbe: 0 };
const prototype = new Proxy(target, {
  has(object, key) {
    if (key === 'referenceProbe') {
      trace.push('has');
      if (fail) throw marker;
      observed = 1;
    }
    return Reflect.has(object, key);
  },
  get(object, key, receiver) {
    if (key === 'referenceProbe') { reads++; throw 'assignment must not get'; }
    return Reflect.get(object, key, receiver);
  },
  set(object, key, value, receiver) {
    if (key === 'referenceProbe') trace.push('set:' + value);
    return Reflect.set(object, key, value, receiver);
  }
});
let first;
Object.setPrototypeOf(globalThis, prototype);
try {
  referenceProbe = (trace.push('rhs:' + observed), 23);
  first = trace.join(',');
  delete globalThis.referenceProbe;
  fail = true;
  try { referenceProbe = (effects++, 29); } catch (error) { caught = error; }
} finally { Object.setPrototypeOf(globalThis, original); }
if (first !== 'has,rhs:1,has,set:23' || trace.join(',') !== first + ',has' ||
    reads !== 0 || effects !== 0 || caught !== marker) throw 'plain assignment Reference';

// A getter deletes the own property. The inherited HasProperty trap must
// run at PutValue, after the conditional RHS, without resolving a second time.
var parseInt;
const global = globalThis;
const savedParseInt = Object.getOwnPropertyDescriptor(global, 'parseInt');
const logicalTrace = [];
let logicalOld, logicalEffect = 1;
const logicalPrototype = new Proxy({ parseInt: 0 }, {
  has(object, key) {
    if (key === 'parseInt') logicalTrace.push('has');
    return Reflect.has(object, key);
  },
  set(object, key, value, receiver) {
    if (key === 'parseInt') logicalTrace.push('set:' + value);
    return Reflect.set(object, key, value, receiver);
  }
});
function armLogical(value) {
  logicalOld = value;
  logicalEffect = 1;
  logicalTrace.length = 0;
  Object.defineProperty(global, 'parseInt', {
    configurable: true,
    get() {
      logicalTrace.push('get');
      delete global.parseInt;
      logicalEffect = 's';
      return logicalOld;
    }
  });
}
Object.setPrototypeOf(global, logicalPrototype);
try {
  armLogical(1);
  const andResult = parseInt &&= (logicalTrace.push('rhs:' + logicalEffect), 31);
  if (andResult !== 31 || logicalTrace.join(',') !== 'get,rhs:s,has,set:31') throw 'logical and Reference';
  armLogical(0);
  const orResult = parseInt ||= (logicalTrace.push('rhs'), 37);
  if (orResult !== 37 || logicalTrace.join(',') !== 'get,rhs,has,set:37') throw 'logical or Reference';
  armLogical(null);
  const nullishResult = parseInt ??= (logicalTrace.push('rhs'), 41);
  if (nullishResult !== 41 || logicalTrace.join(',') !== 'get,rhs,has,set:41') throw 'logical nullish Reference';
  armLogical(0);
  const skipped = parseInt &&= (effects++, 43);
  if (skipped !== 0 || effects !== 0 || logicalTrace.join(',') !== 'get') throw 'logical skipped Put';
  armLogical(1);
  let rhsError;
  try { parseInt &&= (() => { logicalTrace.push('rhs'); throw marker; })(); }
  catch (error) { rhsError = error; }
  if (rhsError !== marker || logicalTrace.join(',') !== 'get,rhs') throw 'logical abrupt RHS';
  logicalTrace.length = 0;
  Object.defineProperty(global, 'parseInt', {
    configurable: true,
    get() { logicalTrace.push('get'); throw marker; }
  });
  let getError;
  try { parseInt ||= (effects++, 47); } catch (error) { getError = error; }
  if (getError !== marker || effects !== 0 || logicalTrace.join(',') !== 'get') throw 'logical abrupt Get';
} finally {
  Object.setPrototypeOf(global, original);
  Object.defineProperty(global, 'parseInt', savedParseInt);
}
// A Global Reference can delegate to a lexical installed after static lowering.
__scriptwRealmEvalScript('let referenceEagerLate = 2; let referenceUpdateLate = 3;');
const eagerLateResult = referenceEagerLate += 5;
const updateLateOld = referenceUpdateLate++;
if (eagerLateResult !== 7 || updateLateOld !== 3 ||
    __scriptwRealmEvalScript('referenceEagerLate;') !== 7 ||
    __scriptwRealmEvalScript('referenceUpdateLate;') !== 4) throw 'existing global lexical mutation';

// PutValue rechecks the lexical delegate on the same retained Global Record.
global.referenceEagerRhs = 1;
const eagerRhsResult = referenceEagerRhs +=
    (__scriptwRealmEvalScript('let referenceEagerRhs = 40;'), 2);
if (eagerRhsResult !== 3 || global.referenceEagerRhs !== 1 ||
    __scriptwRealmEvalScript('referenceEagerRhs;') !== 3) throw 'eager RHS lexical mutation';
let updateCoercions = 0;
const numericOld = { valueOf() {
  updateCoercions++;
  __scriptwRealmEvalScript('let referenceUpdateCoercion = 20;');
  return 1;
} };
global.referenceUpdateCoercion = numericOld;
const coercionOldResult = referenceUpdateCoercion++;
if (coercionOldResult !== 1 || updateCoercions !== 1 ||
    global.referenceUpdateCoercion !== numericOld ||
    __scriptwRealmEvalScript('referenceUpdateCoercion;') !== 2) throw 'numeric coercion lexical mutation';

let eagerHasCount = 0, eagerGetCount = 0, numericHasCount = 0, numericGetCount = 0;
const mutationPrototype = new Proxy({ referenceEagerHas: 1 }, {
  has(object, key) {
    if (key === 'referenceEagerHas') {
      if (++eagerHasCount === 1) __scriptwRealmEvalScript('let referenceEagerHas = 10;');
    }
    if (key === 'parseInt') { numericHasCount++; return false; }
    return Reflect.has(object, key);
  },
  get(object, key, receiver) {
    if (key === 'referenceEagerHas') eagerGetCount++;
    if (key === 'parseInt') { numericGetCount++; return 1; }
    return Reflect.get(object, key, receiver);
  }
});
Object.setPrototypeOf(global, mutationPrototype);
try {
  const eagerHasResult = referenceEagerHas += 2;
  if (eagerHasResult !== 12 || eagerHasCount !== 1 || eagerGetCount !== 0 ||
      __scriptwRealmEvalScript('referenceEagerHas;') !== 12) throw 'eager Has lexical mutation';
  delete global.parseInt;
  let numericMissing;
  try { parseInt++; } catch (error) { numericMissing = error; }
  if (!(numericMissing instanceof ReferenceError) || numericHasCount !== 1 ||
      numericGetCount !== 0 || Object.hasOwn(global, 'parseInt')) throw 'numeric absent Reference';
} finally {
  Object.setPrototypeOf(global, original);
  Object.defineProperty(global, 'parseInt', savedParseInt);
}

true;
"#
        ));
    }
}

#[test]
fn selected_with_fallback_and_foreign_realm_keep_the_original_missing_reference() {
    succeeds(
        r#"
let write, caught;
with ({}) {
  write = function () { 'use strict'; referenceWithMissing = (globalThis.referenceWithMissing = 31); };
}
try { write(); } catch (error) { caught = error; }
const realm = __scriptwCreateRealm();
const foreign = realm.evalScript("function write() { 'use strict'; foreignMissing = (globalThis.foreignMissing = 37); } write;");
const errorPrototype = realm.global.ReferenceError.prototype;
realm.global.ReferenceError = function () { throw 'public error constructor'; };
let foreignError;
try { foreign(); } catch (error) { foreignError = error; }
caught instanceof ReferenceError && globalThis.referenceWithMissing === 31 &&
  Object.getPrototypeOf(foreignError) === errorPrototype && realm.global.foreignMissing === 37 &&
  !Object.hasOwn(globalThis, 'foreignMissing');
"#,
    );
}

#[test]
fn with_plain_put_retains_global_and_object_references_across_rhs_changes() {
    for (directive, strict) in [("", false), ("'use strict';", true)] {
        let source = r#"
let write, caught, effects = 0;
const scope = {}, marker = {};
with (scope) write = function () { /* source strictness */ return withBaselineMissing = (effects++, 11); };
let baselineResult;
try { baselineResult = write(); } catch (error) { caught = error; }
if (strictMode ? !(caught instanceof ReferenceError) || Object.hasOwn(globalThis, 'withBaselineMissing')
               : caught !== undefined || baselineResult !== 11 || globalThis.withBaselineMissing !== 11) throw 'baseline strictness';

caught = undefined;
with (scope) write = function () { /* source strictness */ return withCreatedMissing = (effects++, globalThis.withCreatedMissing = 13, 17); };
let createdResult;
try { createdResult = write(); } catch (error) { caught = error; }
if (strictMode ? !(caught instanceof ReferenceError) || globalThis.withCreatedMissing !== 13
               : caught !== undefined || createdResult !== 17 || globalThis.withCreatedMissing !== 17) throw 'missing global Reference';

caught = undefined;
globalThis.withDeletedGlobal = 19;
with (scope) write = function () { /* source strictness */ return withDeletedGlobal = (effects++, delete globalThis.withDeletedGlobal, 23); };
let deletedResult;
try { deletedResult = write(); } catch (error) { caught = error; }
if (strictMode ? !(caught instanceof ReferenceError) || Object.hasOwn(globalThis, 'withDeletedGlobal')
               : caught !== undefined || deletedResult !== 23 || globalThis.withDeletedGlobal !== 23) throw 'deleted global Reference';

globalThis.withAddedObject = 29;
with (scope) write = function () { /* source strictness */ return withAddedObject = (effects++, scope.withAddedObject = 31, 37); };
if (write() !== 37 || globalThis.withAddedObject !== 37 || scope.withAddedObject !== 31) throw 'global fallback reselection';

caught = undefined;
scope.withDeletedObject = 41;
with (scope) write = function () { /* source strictness */ return withDeletedObject = (effects++, delete scope.withDeletedObject, globalThis.withDeletedObject = 43, 47); };
let objectResult;
try { objectResult = write(); } catch (error) { caught = error; }
if (strictMode ? !(caught instanceof ReferenceError) || Object.hasOwn(scope, 'withDeletedObject')
               : caught !== undefined || objectResult !== 47 || scope.withDeletedObject !== 47) throw 'selected object Reference';
if (globalThis.withDeletedObject !== 43) throw 'selected object switched to global';

scope.withUnscopableChanged = 53;
scope[Symbol.unscopables] = { withUnscopableChanged: false };
globalThis.withUnscopableChanged = 59;
with (scope) write = function () { /* source strictness */ return withUnscopableChanged = (effects++, scope[Symbol.unscopables].withUnscopableChanged = true, 61); };
if (write() !== 61 || scope.withUnscopableChanged !== 61 || globalThis.withUnscopableChanged !== 59) throw 'unscopables reselection';

caught = undefined;
with (scope) write = function () { /* source strictness */ withAbruptMissing = (() => { effects++; throw marker; })(); };
try { write(); } catch (error) { caught = error; }
if (caught !== marker || effects !== 7 || Object.hasOwn(globalThis, 'withAbruptMissing')) throw 'RHS completion';
true;
"#;
        succeeds(&format!(
            "const strictMode = {strict};\n{}",
            source.replace("/* source strictness */", directive)
        ));
    }
}

#[test]
fn with_global_fallback_runs_prototype_has_before_rhs_and_preserves_its_throw() {
    for (directive, strict) in [("", false), ("'use strict';", true)] {
        let source = r#"
const original = Object.getPrototypeOf(globalThis), marker = {};
const trace = [];
let effects = 0, gets = 0, write, abruptWrite;
const scope = new Proxy({}, {
  has(object, key) {
    if (key === 'withHasTarget') trace.push('with');
    if (key === 'withHasAbrupt') trace.push('with-abrupt');
    return Reflect.has(object, key);
  }
});
with (scope) {
  write = function () { /* source strictness */ return withHasTarget = (effects++, trace.push('rhs:' + effects), 71); };
  abruptWrite = function () { /* source strictness */ withHasAbrupt = (effects++, 73); };
}
const prototype = new Proxy({}, {
  has(object, key) {
    if (key === 'withHasTarget') { trace.push('global:' + effects); return true; }
    if (key === 'withHasAbrupt') { trace.push('global-abrupt:' + effects); throw marker; }
    return Reflect.has(object, key);
  },
  get(object, key, receiver) {
    if (key === 'withHasTarget') gets++;
    return Reflect.get(object, key, receiver);
  },
  set(object, key, value, receiver) {
    if (key === 'withHasTarget') trace.push('set:' + value);
    return Reflect.set(object, key, value, receiver);
  }
});
Object.setPrototypeOf(globalThis, prototype);
try {
  if (write() !== 71 || effects !== 1 || gets !== 0 || globalThis.withHasTarget !== 71 ||
      trace.join(',') !== 'with,global:0,rhs:1,global:1,set:71') throw 'pre-RHS global HasBinding';
  trace.length = 0;
  let caught;
  try { abruptWrite(); } catch (error) { caught = error; }
  if (caught !== marker || effects !== 1 || Object.hasOwn(globalThis, 'withHasAbrupt') ||
      trace.join(',') !== 'with-abrupt,global-abrupt:1') throw 'abrupt global HasBinding';
} finally {
  Object.setPrototypeOf(globalThis, original);
}
true;
"#;
        succeeds(&format!(
            "const strictMode = {strict};\n{}",
            source.replace("/* source strictness */", directive)
        ));
    }
}

#[test]
fn global_put_selects_the_current_lexical_delegate_of_its_held_record() {
    for prelude in ["", "'use strict';\n"] {
        succeeds(&format!(
            "{prelude}{}",
            r#"
globalThis.referenceAddedLet = 1;
const result = referenceAddedLet = (__scriptwRealmEvalScript('let referenceAddedLet = 2;'), 3);
if (result !== 3 || Reflect.get(globalThis, 'referenceAddedLet') !== 1) throw 'object delegate';
if (__scriptwRealmEvalScript('referenceAddedLet;') !== 3) throw 'lexical delegate';
referenceAddedLet = 4;
if (__scriptwRealmEvalScript('referenceAddedLet;') !== 4) throw 'existing lexical delegate';

globalThis.referenceAddedConst = 5;
let constantError;
try { referenceAddedConst = (__scriptwRealmEvalScript('const referenceAddedConst = 6;'), 7); }
catch (error) { constantError = error; }
if (!(constantError instanceof TypeError) || Reflect.get(globalThis, 'referenceAddedConst') !== 5) throw 'constant';
if (__scriptwRealmEvalScript('referenceAddedConst;') !== 6) throw 'constant value';

globalThis.referenceAddedTdz = 8;
function installTdz() {
  try { __scriptwRealmEvalScript("let referenceAddedTdz = (() => { throw 'install marker'; })();"); }
  catch (error) { if (error !== 'install marker') throw error; }
  return 9;
}
let tdzError;
try { referenceAddedTdz = installTdz(); } catch (error) { tdzError = error; }
tdzError instanceof ReferenceError && Reflect.get(globalThis, 'referenceAddedTdz') === 8;
"#
        ));
    }
}

#[test]
fn global_get_refreshes_after_resolvebinding_and_preserves_object_record_get_order() {
    for (prelude, strict) in [("", false), ("'use strict';\n", true)] {
        succeeds(&format!(
            "{prelude}const strictMode = {strict};{}",
            r#"
const original = Object.getPrototypeOf(globalThis), marker = {};
let installed = false, shadowHas = 0, shadowGets = 0;
let absentHas = 0, absentGets = 0, abruptHas = 0, abruptGets = 0;
const target = { referenceGetShadow: 11, referenceGetAbsent: 12, referenceGetAbrupt: 13 };
const prototype = new Proxy(target, {
  has(object, key) {
    if (key === 'referenceGetShadow') {
      shadowHas++;
      if (!installed) {
        installed = true;
        __scriptwRealmEvalScript('let referenceGetShadow = 23;');
      }
    }
    if (key === 'referenceGetAbsent') return ++absentHas === 1;
    if (key === 'referenceGetAbrupt' && ++abruptHas === 2) throw marker;
    return Reflect.has(object, key);
  },
  get(object, key, receiver) {
    if (key === 'referenceGetShadow') shadowGets++;
    if (key === 'referenceGetAbsent') absentGets++;
    if (key === 'referenceGetAbrupt') abruptGets++;
    return Reflect.get(object, key, receiver);
  }
});
let shadow, absent = 99, absentError, abruptError;
Object.setPrototypeOf(globalThis, prototype);
try {
  shadow = referenceGetShadow;
  try { absent = referenceGetAbsent; } catch (error) { absentError = error; }
  try { referenceGetAbrupt; } catch (error) { abruptError = error; }
} finally { Object.setPrototypeOf(globalThis, original); }
const orderChecks = shadow === 23 && shadowHas === 1 && shadowGets === 0 &&
  absentHas === 2 && absentGets === 0 &&
  (strictMode ? absentError instanceof ReferenceError && absent === 99 : absentError === undefined && absent === undefined) &&
  abruptHas === 2 && abruptGets === 0 && abruptError === marker;

// typeof only suppresses an unresolvable name; a retained uninitialized global
// lexical binding still throws through the same complete read helper.
let missingError, tdzError;
try { referenceHelperMissing; } catch (error) { missingError = error; }
const missingType = typeof referenceHelperMissing;
try { __scriptwRealmEvalScript("let referenceHelperTdz = (() => { throw 'initialize marker'; })();"); }
catch (error) { if (error !== 'initialize marker') throw error; }
try { typeof referenceHelperTdz; } catch (error) { tdzError = error; }

// The acquired source callable owns its error Realm even when called here.
// Public error constructor replacement cannot change the canonical prototype.
const foreignRealm = __scriptwCreateRealm();
const foreignRead = foreignRealm.evalScript("(function () { return referenceForeignHelperMissing; });");
const foreignStrictRead = foreignRealm.evalScript("(function () { 'use strict'; return referenceForeignHelperMissing; });");
const foreignTypeof = foreignRealm.evalScript("(function () { return typeof referenceForeignHelperTdz; });");
const foreignStrictTypeof = foreignRealm.evalScript("(function () { 'use strict'; return typeof referenceForeignHelperTdz; });");
const foreignErrorPrototype = foreignRealm.global.ReferenceError.prototype;
try { foreignRealm.evalScript("let referenceForeignHelperTdz = (() => { throw 'foreign initialize marker'; })();"); }
catch (error) { if (error !== 'foreign initialize marker') throw error; }
foreignRealm.global.ReferenceError = function () { throw 'public ReferenceError'; };
let foreignChecks = 0;
for (const read of [foreignRead, foreignStrictRead, foreignTypeof, foreignStrictTypeof]) {
  try { read(); }
  catch (error) { if (Object.getPrototypeOf(error) === foreignErrorPrototype) foreignChecks++; }
}
orderChecks && missingError instanceof ReferenceError && missingType === 'undefined' &&
  tdzError instanceof ReferenceError && foreignChecks === 4;
"#
        ));
    }
}
