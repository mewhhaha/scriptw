use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, ObservedJsValue, ObservedNumber, RealmBuilder, RunOptions,
};

fn assert_unicode_modes(source: &str, line: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{source}");
        let observed = Engine::new(RealmBuilder::new().build())
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
            .expect("Unicode native control compiles and executes through Wasm AOT");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(262.0))),
            "{source}\noutput: {:?}\nnote: {}",
            observed.output_events,
            observed.note,
        );
        assert_eq!(
            observed.output_events,
            vec![HostOutputEvent::PrintLine(line.into())],
            "{source}"
        );
    }
}

#[test]
fn gc_unicode_normalization_and_contextual_case_preserve_utf16() {
    assert_unicode_modes(
        r#"
function check(ok, label) { if (!ok) { print('check-failed: ' + label); throw new Error(label); } }
check('A\u030a'.normalize() === '\u00c5', 'default NFC');
check('\u00c5'.normalize('NFD') === 'A\u030a', 'canonical decomposition');
check('\ufb03'.normalize('NFKC') === 'ffi', 'compatibility composition');
check('\u1e9b\u0323'.normalize('NFKD') === 's\u0323\u0307', 'compatibility reorder');
check('\u1e9b\u0323'.normalize('NFKC') === '\u1e69', 'multi-stage composition');
check('\u1100\u1161\u11a8'.normalize('NFC') === '\uac01', 'Hangul LVT composition');
check('\uac01'.normalize('NFD') === '\u1100\u1161\u11a8', 'Hangul decomposition');
check('\u0301\u0323'.normalize('NFD') === '\u0323\u0301', 'leading nonstarter order');
check('A\u0305\u030a'.normalize('NFC') === 'A\u0305\u030a', 'equal-class blocking');
check('\ud800A\udc00\ud83d\ude00'.normalize('NFKD') === '\ud800A\udc00\ud83d\ude00', 'surrogates retained');
check(''.normalize('NFC') === '' && ''.toUpperCase() === '', 'empty roots');
check('A\u03a3\u0301'.toLowerCase() === 'a\u03c2\u0301', 'final sigma through ignorable');
check('A\u03a3\u0301B'.toLowerCase() === 'a\u03c3\u0301b', 'following cased defeats final sigma');
check('\u03a3'.toLowerCase() === '\u03c3', 'sigma needs preceding cased');
check('stra\u00dfe \ufb03'.toUpperCase() === 'STRASSE FFI', 'full multi-code-point uppercase');
check('\u0130'.toLowerCase() === 'i\u0307', 'default dotted I expansion');
check('\ud801\udc00'.toLowerCase() === '\ud801\udc28', 'astral lowercase');
check('\ud800a\udc00'.toUpperCase() === '\ud800A\udc00', 'lone surrogate case identity');
check('I\u0307 I \u0130 i'.toLocaleLowerCase('tr-TR-u-co-search') === 'i \u0131 i i', 'Turkish original contexts');
check('i\u0131'.toLocaleUpperCase('az') === '\u0130I', 'Azeri uppercase');
check('I\u0301\u0307'.toLocaleLowerCase('tr') === '\u0131\u0301\u0307', 'above mark blocks Turkic dot context');
check('I\u0301'.toLocaleLowerCase('lt') === 'i\u0307\u0301', 'Lithuanian more above');
check('\u00cd'.toLocaleLowerCase('lt-LT') === 'i\u0307\u0301', 'Lithuanian precomposed I');
check('i\u0323\u0307'.toLocaleUpperCase('lt') === 'I\u0323', 'soft dotted context crosses below mark');
check('i\u0301\u0307'.toLocaleUpperCase('lt') === 'I\u0301\u0307', 'above mark blocks soft dotted context');
check('I'.toLocaleLowerCase(['xx', 'tr']) === 'i', 'only first requested locale');
check('I'.toLocaleLowerCase([]) === 'I'.toLocaleLowerCase(), 'empty list uses same native default');
print('gc-string-unicode:ok');
262;
"#,
        "gc-string-unicode:ok",
    );
}

#[test]
fn gc_html_and_regexp_escape_publish_exact_units() {
    assert_unicode_modes(
        r#"
function check(ok, label) { if (!ok) { print('check-failed: ' + label); throw new Error(label); } }
var rows = [
  ['big', '<big>x</big>'], ['blink', '<blink>x</blink>'], ['bold', '<b>x</b>'],
  ['fixed', '<tt>x</tt>'], ['italics', '<i>x</i>'], ['small', '<small>x</small>'],
  ['strike', '<strike>x</strike>'], ['sub', '<sub>x</sub>'], ['sup', '<sup>x</sup>']
];
for (var i = 0; i < rows.length; ++i) check(String.prototype[rows[i][0]].call('x') === rows[i][1], rows[i][0]);
check('x'.anchor('a"&<') === '<a name="a&quot;&<">x</a>', 'only quote escaped in anchor');
check('x'.link('a"') === '<a href="a&quot;">x</a>', 'href');
check('x'.fontcolor('a"') === '<font color="a&quot;">x</font>', 'color');
check('x'.fontsize('a"') === '<font size="a&quot;">x</font>', 'size');
check('x'.anchor() === '<a name="undefined">x</a>', 'missing attribute coerced');
check('\ud800\ud83d\ude00'.bold() === '<b>\ud800\ud83d\ude00</b>', 'HTML preserves UTF16');
check(RegExp.escape('foo') === '\\x66oo', 'leading ASCII letter');
check(RegExp.escape('12') === '\\x312', 'leading digit');
check(RegExp.escape('^$\\.*+?()[]{}|/') === '\\^\\$\\\\\\.\\*\\+\\?\\(\\)\\[\\]\\{\\}\\|\\/', 'syntax punctuation');
check(RegExp.escape('\t\n\v\f\r -,#&/~') === '\\t\\n\\v\\f\\r\\x20\\x2d\\x2c\\x23\\x26\\/\\x7e', 'control and hex punctuation');
check(RegExp.escape('\ufeff\u2028\u2029\u0085') === '\\ufeff\\u2028\\u2029\u0085', 'ECMAScript whitespace only');
check(RegExp.escape('\ud800X\udc00\ud83d\ude00') === '\\ud800X\\udc00\ud83d\ude00', 'lone and paired surrogates');
check(RegExp.escape('') === '', 'empty escape');
var poison = { toString: function () { throw new Error('must not coerce escape'); } };
var caught;
try { RegExp.escape(poison); } catch (error) { caught = error; }
check(caught instanceof TypeError, 'escape rejects objects without coercion');
print('gc-string-html-escape:ok');
262;
"#,
        "gc-string-html-escape:ok",
    );
}

#[test]
fn gc_unicode_order_abrupt_identity_and_called_realms() {
    assert_unicode_modes(
        r#"
function check(ok, label) { if (!ok) { print('check-failed: ' + label); throw new Error(label); } }
var $262 = { createRealm: __scriptwCreateRealm };
var trace = [];
var receiver = { toString: function () { trace.push('receiver'); return 'A\u030a'; } };
var form = { toString: function () { trace.push('form'); return 'NFD'; } };
check(String.prototype.normalize.call(receiver, form) === 'A\u030a', 'borrowed normalization');
check(trace.join(',') === 'receiver,form', 'receiver before form');
trace = [];
var localeList = new Proxy({ length: 2, 0: 'tr', 1: 'en' }, {
  get: function (target, key) { trace.push('get:' + key); return target[key]; },
  has: function (target, key) { trace.push('has:' + key); return key in target; }
});
var lowerReceiver = { toString: function () { trace.push('receiver'); return 'I'; } };
check(String.prototype.toLocaleLowerCase.call(lowerReceiver, localeList) === '\u0131', 'first canonical locale');
check(trace.join(',') === 'receiver,get:length,has:0,get:0,has:1,get:1', 'complete locale acquisition after receiver');
var caught;
try { 'I'.toLocaleLowerCase(['tr', 'invalid_locale']); } catch (error) { caught = error; }
check(caught instanceof RangeError, 'later locale validated before selection');
trace = [];
var attr = { toString: function () { trace.push('attribute'); return '"'; } };
var htmlReceiver = { toString: function () { trace.push('receiver'); return 'x'; } };
check(String.prototype.anchor.call(htmlReceiver, attr) === '<a name="&quot;">x</a>', 'HTML conversion result');
check(trace.join(',') === 'receiver,attribute', 'HTML receiver then attribute');
var foreign = $262.createRealm();
var localString = String, localRegExp = RegExp, localRange = RangeError, localType = TypeError;
var foreignString = foreign.global.String, foreignRegExp = foreign.global.RegExp;
var foreignNormalize = foreignString.prototype.normalize, foreignLower = foreignString.prototype.toLocaleLowerCase;
var foreignEscape = foreignRegExp.escape, foreignType = foreign.global.TypeError, foreignRange = foreign.global.RangeError;
var localNormalize = String.prototype.normalize, localEscape = RegExp.escape;
var marker = foreign.evalScript('({ marker: 29 })');
var foreignBox = new foreignString('x');
var prior = 11;
try {
  String = function () { throw marker; }; RegExp = function () { throw marker; };
  foreign.global.String = function () { throw marker; }; foreign.global.RegExp = function () { throw marker; };
  caught = undefined;
  try { foreignNormalize.call('x', 'bad'); } catch (error) { caught = error; }
  check(Object.getPrototypeOf(caught) === foreignRange.prototype, 'foreign called RangeError');
  caught = undefined;
  try { localNormalize.call(foreignBox, 'bad'); } catch (error) { caught = error; }
  check(Object.getPrototypeOf(caught) === localRange.prototype, 'local called RangeError');
  caught = undefined;
  try { foreignEscape(17); } catch (error) { caught = error; }
  check(Object.getPrototypeOf(caught) === foreignType.prototype, 'foreign escape TypeError');
  caught = undefined;
  try { localEscape(foreign.evalScript('({})')); } catch (error) { caught = error; }
  check(Object.getPrototypeOf(caught) === localType.prototype, 'local escape TypeError');
  var unobserved = { get length() { throw marker; } };
  caught = undefined;
  try { foreignLower.call(null, unobserved); } catch (error) { caught = error; }
  check(Object.getPrototypeOf(caught) === foreignType.prototype, 'null receiver before locales');
  var explosiveForm = { toString: function () { throw marker; } };
  caught = undefined;
  try { prior = foreignNormalize.call('x', explosiveForm); } catch (error) { caught = error; } finally { trace.push('finally'); }
  check(caught === marker && prior === 11, 'whole foreign throw and assignment cutoff');
  var explosiveLocales = { get length() { throw marker; } };
  caught = undefined;
  try { foreignLower.call('I', explosiveLocales); } catch (error) { caught = error; }
  check(caught === marker, 'locale Get original throw');
} finally {
  String = localString; RegExp = localRegExp;
  foreign.global.String = foreignString; foreign.global.RegExp = foreignRegExp;
}
check(trace[trace.length - 1] === 'finally', 'original finally ran');
print('gc-string-unicode-realms:ok');
262;
"#,
        "gc-string-unicode-realms:ok",
    );
}

#[test]
fn program_literal_descriptor_roots_preserve_empty_long_nul_and_surrogate_units() {
    let long = "x".repeat(16 * 1024);
    let source = format!(
        r#"
function check(value, label) {{ if (!value) throw label; }}
var empty = '', nul = '\u0000', pair = '\ud800\udc00', lone = '\ud800X\udc00';
var long = '{long}\u0000\ud800\udc00\ud800';
var saved = [empty, nul, pair, lone, long];
gc(); gc();
check(saved[0].length === 0 && saved[0] === String.fromCharCode(), 'empty root');
check(saved[1].length === 1 && saved[1].charCodeAt(0) === 0, 'NUL root');
check(saved[2].length === 2 && saved[2].codePointAt(0) === 0x10000, 'paired root');
check(saved[3].length === 3 && saved[3].charCodeAt(0) === 0xd800
    && saved[3].charCodeAt(2) === 0xdc00, 'isolated roots');
check(saved[4].length === 16388 && saved[4].charCodeAt(16384) === 0
    && saved[4].charCodeAt(16385) === 0xd800 && saved[4].charCodeAt(16387) === 0xd800, 'long root');
check(saved[4].slice(0, 16384) === 'x'.repeat(16384), 'full long prefix');
check(saved[3] === String.fromCharCode(0xd800, 88, 0xdc00), 'literal and dynamic unit equality');
check(saved[4].slice(16384) === String.fromCharCode(0, 0xd800, 0xdc00, 0xd800), 'exact terminal units');
var foreign = __scriptwCreateRealm().global;
check(foreign.String.prototype.slice.call(saved[3], 0) === saved[3], 'borrowed foreign string');
gc(); check(saved[4] === long && saved[0] === '', 'pool remains rooted after Realm creation');
print('program-literal-units:ok');
262;
"#
    );
    assert_unicode_modes(&source, "program-literal-units:ok");
}

#[test]
fn many_program_literal_roots_preserve_runtime_pool_names_and_intrinsics() {
    let values = (0..512)
        .map(|index| format!("'program_literal_{index:04}\\u0000\\uD800'"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        r#"
function check(value, label) {{ if (!value) throw label; }}
var values = [{values}];
var shared = ['', 'name', 'String', 'Object', 'TypeError', 'parse', 'string'];
var foreign = __scriptwCreateRealm().global;
gc();
for (var i = 0; i < values.length; i++) {{
  var expected = 'program_literal_' + ('0000' + i).slice(-4) + String.fromCharCode(0, 0xd800);
  check(values[i] === expected && values[i].length === 22, 'program row ' + i);
}}
check(shared[2] === String.name && shared[3] === Object.name, 'runtime constructor name roots');
check(shared[4] === foreign.TypeError.name && shared[5] === Date.parse.name, 'cross-Realm builtin name roots');
check(shared[6] === typeof shared[0] && Object.getOwnPropertyDescriptor(Object, shared[1]).value === shared[3], 'runtime primitive and descriptor names');
check(new foreign.String(values[511]).valueOf() === values[511], 'foreign boxing of last source row');
gc(); check(values[0].charCodeAt(21) === 0xd800 && shared[0] === '', 'all namespaces stay rooted');
print('program-literal-many:ok');
262;
"#
    );
    assert_unicode_modes(&source, "program-literal-many:ok");
}

#[test]
fn late_canonical_strings_preserve_symbols_errors_and_cross_realm_mutation() {
    assert_unicode_modes(
        r#"
function check(value, label) { if (!value) throw label; }
check(Symbol.iterator.description === 'Symbol.iterator', 'bootstrap symbol description');
check(Symbol.toStringTag.description === 'Symbol.toStringTag', 'second symbol description');
for (var i = 0; i < 20; i++) { String(i).repeat(1000); gc(); }
var foreign = __scriptwCreateRealm().global;
check(foreign.Symbol.iterator === Symbol.iterator, 'shared well-known identity');
var original = String.prototype.normalize;
String.prototype.normalize = function() { return 'replacement'; };
check(foreign.String.prototype.normalize.call('e\u0301', 'NFC') === '\u00e9', 'late canonical normalization across Realm');
var first, second;
try { foreign.String.prototype.normalize.call('x', 'not-a-form'); } catch (error) { first = error; }
gc();
try { foreign.String.prototype.normalize.call('x', 'not-a-form'); } catch (error) { second = error; }
check(first instanceof foreign.RangeError && second instanceof foreign.RangeError, 'late error Realm');
check(typeof first.message === 'string' && first.message.length > 0 && first.message === second.message, 'cached diagnostic units');
check(String.prototype.normalize() === 'replacement', 'constant resolution cannot reset mutated intrinsic');
String.prototype.normalize = original;
check(original.call('\uD800\0', 'NFC') === '\uD800\0', 'lone surrogate and NUL');
print('canonical-late:ok');
262;
"#,
        "canonical-late:ok",
    );
}

#[test]
fn deferred_normalization_tables_survive_first_use_coercion_reentrancy_and_fresh_stores() {
    let source = r#"
function check(v,m){if(!v)throw m;}
var events=[],caught=false;
try{String.prototype.normalize.call({toString(){events.push('receiver-invalid');return 'x';}},
 {toString(){events.push('form-invalid');return 'invalid';}});}catch(e){caught=e instanceof RangeError;}
check(caught && events.join(',')==='receiver-invalid,form-invalid','invalid form before any normalization');
var input={toString(){events.push('receiver');gc();return '\u1e9b\u0323';}};
var form={toString(){events.push('form');check('A\u030a'.normalize('NFC')==='\u00c5','first nested initialization');gc();return 'NFKD';}};
check(String.prototype.normalize.call(input,form)==='s\u0323\u0307','outer first use after recursive coercion');
check(events.join(',')==='receiver-invalid,form-invalid,receiver,form','coercion order');
var foreign=__scriptwCreateRealm().global;
for(var i=0;i<3;i++){
 gc();
 check('A\u030a'.normalize('NFC')==='\u00c5','NFC');
 check('\u00c5'.normalize('NFD')==='A\u030a','NFD');
 check('\ufb03'.normalize('NFKC')==='ffi','NFKC');
 check(foreign.String.prototype.normalize.call('\u1e9b\u0323','NFKD')==='s\u0323\u0307','NFKD same R after Realm creation');
 check('\ud800\0\udc00'.normalize('NFC')==='\ud800\0\udc00','exact units');
}
print('normalization-deferred:ok');
262;
"#;
    // assert_unicode_modes creates a new Store for each invocation. Repeat the
    // same retained runtime profile so readiness cannot leak across Stores.
    assert_unicode_modes(source, "normalization-deferred:ok");
    let casing_first = format!(
        r#"
if('I\u0307'.toLocaleLowerCase('tr')!=='i')throw 'Turkic first table consumer';
if('I\u0301'.toLocaleLowerCase('lt')!=='i\u0307\u0301')throw 'Lithuanian combining classes';
{source}"#
    );
    assert_unicode_modes(&casing_first, "normalization-deferred:ok");
}
