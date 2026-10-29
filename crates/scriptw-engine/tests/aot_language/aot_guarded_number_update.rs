use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, ObservedCompletion, RealmBuilder,
    RunOptions,
};

fn assert_guarded_update(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let observation = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("guarded numeric updates execute through Wasm AOT");
    assert!(
        matches!(observation.completion, ObservedCompletion::Normal(_)),
        "{:?}\n{source}",
        observation.completion
    );
    assert_eq!(
        observation.output_events,
        vec![HostOutputEvent::PrintLine("true".into())],
        "{source}"
    );
}

#[test]
fn runtime_number_updates_and_conversion_misses_keep_prefix_postfix_values() {
    assert_guarded_update(
        r#"
function sequence(value) {
  var old = value++, prefix = ++value, next = value--, last = --value;
  return [old, prefix, next, last, value];
}
function check(input, expected) {
  var actual = sequence(input);
  for (var i = 0; i < 5; i++) {
    if (!Object.is(actual[i], expected[i])) throw 'update result ' + i;
  }
}
check(-0, [-0, 2, 2, 0, 0]);
check(NaN, [NaN, NaN, NaN, NaN, NaN]);
check(Infinity, [Infinity, Infinity, Infinity, Infinity, Infinity]);
check(-Infinity, [-Infinity, -Infinity, -Infinity, -Infinity, -Infinity]);
check('41', [41, 43, 43, 41, 41]);
check(true, [1, 3, 3, 1, 1]);
check(false, [0, 2, 2, 0, 0]);
check(null, [0, 2, 2, 0, 0]);
check(undefined, [NaN, NaN, NaN, NaN, NaN]);
check(7n, [7n, 9n, 9n, 7n, 7n]);
check(9223372036854775807n,
  [9223372036854775807n, 9223372036854775809n, 9223372036854775809n,
   9223372036854775807n, 9223372036854775807n]);
var conversions = 0;
check({ [Symbol.toPrimitive](hint) {
  if (hint !== 'number') throw hint;
  conversions++;
  return '41';
}}, [41, 43, 43, 41, 41]);
check({ valueOf() { conversions++; return 7n; } }, [7n, 9n, 9n, 7n, 7n]);
if (conversions !== 2) throw 'repeat conversion';
function knownNumber() {
  let value = -0;
  let old = value++;
  if (!Object.is(old, -0) || value !== 1 || --value !== 0) throw 'known Number';
}
function knownPrimitive() {
  let value = '4';
  return value++ === 4 && value === 5;
}
knownNumber();
if (!knownPrimitive()) throw 'Number result kind still needs conversion';
function mixedLoop() {
  let index = 0;
  const values = [];
  while (index < 16) values.push(index++);
  return values.join(',');
}
if (mixedLoop() !== '0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15') throw 'mixed loop';
print(true);
"#,
    );
}

#[test]
fn numeric_updates_keep_one_saved_reference_across_get_and_conversion_mutations() {
    assert_guarded_update(
        r#"
var trace = '', stored = 0, current, key, replacement = {value: 100};
var original = {
  get value() {
    if (this !== original) throw 'getter receiver';
    trace += 'get;';
    current = replacement;
    return { [Symbol.toPrimitive](hint) {
      trace += hint + ';';
      current = replacement;
      key = 'other';
      stored = 100;
      return '4';
    }};
  },
  set value(value) {
    if (this !== original) throw 'setter receiver';
    trace += 'set;';
    stored = value;
  }
};
function base() { trace += 'base;'; return current; }
current = original;
key = { toString() { trace += 'key;'; return 'value'; } };
var old = base()[key]++;
if (old !== 4 || stored !== 5 || replacement.value !== 100 || current !== replacement ||
    trace !== 'base;key;get;number;set;') throw trace;

trace = '';
var proxy, proxyAlias, proxyStored;
proxy = new Proxy({}, {
  get(target, name, receiver) {
    if (name !== 'value' || receiver !== proxyAlias) throw 'proxy get reference';
    trace += 'get;';
    proxy = replacement;
    return -0;
  },
  set(target, name, value, receiver) {
    if (name !== 'value' || receiver !== proxyAlias) throw 'proxy set reference';
    trace += 'set;';
    proxyStored = value;
    return true;
  }
});
proxyAlias = proxy;
old = proxy.value++;
if (!Object.is(old, -0) || proxyStored !== 1 || proxy !== replacement ||
    trace !== 'get;set;') throw 'Number proxy update';

function mutatedBinding() {
  let value = { valueOf() { value = 'replacement'; return 7; } };
  const old = value++;
  return old === 7 && value === 8;
}
if (!mutatedBinding()) throw 'saved identifier value';
print(true);
"#,
    );
}

#[test]
fn numeric_update_throws_stop_before_store_and_keep_the_original_exception() {
    assert_guarded_update(
        r#"
var marker = {}, reads = 0, conversions = 0, writes = 0;
var received;
var getterThrows = {
  get value() { reads++; throw marker; },
  set value(value) { writes++; }
};
try { getterThrows.value++; } catch (error) { received = error; }
if (received !== marker || reads !== 1 || writes !== 0) throw 'getter throw';
var coercionThrows = {
  get value() { reads++; return { valueOf() { conversions++; throw marker; } }; },
  set value(value) { writes++; }
};
received = undefined;
try { ++coercionThrows.value; } catch (error) { received = error; }
if (received !== marker || reads !== 2 || conversions !== 1 || writes !== 0) throw 'coercion throw';
var setterThrows = {
  get value() { reads++; return -0; },
  set value(value) { writes++; if (value !== 1) throw 'new Number'; throw marker; }
};
received = undefined;
try { setterThrows.value++; } catch (error) { received = error; }
if (received !== marker || reads !== 3 || writes !== 1) throw 'Number setter throw';
var symbol = Symbol(), symbolValue = {
  get value() { reads++; return symbol; },
  set value(value) { writes++; }
};
received = undefined;
try { --symbolValue.value; } catch (error) { received = error; }
if (!(received instanceof TypeError) || reads !== 4 || writes !== 1) throw 'Symbol conversion';
var proxyThrows = new Proxy({}, {
  get() { reads++; return 1; },
  set() { writes++; throw marker; }
});
received = undefined;
try { ++proxyThrows.value; } catch (error) { received = error; }
if (received !== marker || reads !== 5 || writes !== 2) throw 'proxy setter throw';
function bindingThrow() {
  let value = { valueOf() { value = 'replacement'; throw marker; } };
  try { value--; } catch (error) { return error === marker && value === 'replacement'; }
  return false;
}
function immutableNumber() {
  const value = 1;
  try { value++; } catch (error) { return error instanceof TypeError && value === 1; }
  return false;
}
function knownSymbol() {
  let value = Symbol();
  const original = value;
  try { value++; } catch (error) { return error instanceof TypeError && value === original; }
  return false;
}
if (!bindingThrow() || !immutableNumber() || !knownSymbol()) throw 'binding abrupt completion';
print(true);
"#,
    );
}
