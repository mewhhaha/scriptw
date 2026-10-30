function check(ok, message) { if (!ok) throw message; }
function throws(action, expected) {
  let caught = false;
  try { action(); } catch (error) { caught = error === expected; }
  check(caught, 'abrupt identity');
}

const empty = [], singleton = ['\0\ud800'], mixed = ['a', null, undefined, '\udfff'];
check(empty.join() === '' && singleton.join() === '\0\ud800' && mixed.join(':') === 'a:::\udfff',
  'real empty singleton and mixed results');
const separatorKinds = ['left', 'right'];
check(separatorKinds.join() === 'left,right' && separatorKinds.join(undefined) === 'left,right' &&
  separatorKinds.join('') === 'leftright' && separatorKinds.join(null) === 'leftnullright',
  'only omitted and undefined separators select the default comma');
let separatorArgumentCalls = 0;
function evaluatedSeparator(value) { separatorArgumentCalls++; gc(); return value; }
check(separatorKinds.join(evaluatedSeparator('|')) === 'left|right' && separatorArgumentCalls === 1,
  'explicit String separator argument is evaluated once before direct selection');
check(separatorKinds.join(evaluatedSeparator(undefined)) === 'left,right' && separatorArgumentCalls === 2,
  'undefined separator argument is evaluated once before default selection');
check(separatorKinds.join(evaluatedSeparator(null)) === 'leftnullright' && separatorArgumentCalls === 3,
  'null separator argument is preserved once for ordinary coercion');
const retained = mixed.join('\ud83d\ude00');
mixed[0] = 'changed';
gc();
check(retained === 'a\ud83d\ude00\ud83d\ude00\ud83d\ude00\udfff', 'immutable result survives mutation and collection');
const long = [];
for (let i = 0; i < 257; i++) long.push('\0\ud800\ud83d\ude00');
const longText = long.join('|');
gc();
check(longText.length === 1284 && longText.charCodeAt(0) === 0 && longText.charCodeAt(1) === 0xd800 &&
  longText.charCodeAt(1283) === 0xde00, 'long exact UTF16 result');

let argumentCalls = 0;
const changedLength = ['first', 'second'];
check(changedLength.join((argumentCalls++, changedLength.length = 1, gc(), ':')) === 'first' && argumentCalls === 1,
  'actual Array length follows source argument effects exactly once');
const selected = ['saved'];
let replacementCalls = 0;
function replacement(separator) { replacementCalls++; check(this === selected && separator === '!', 'replacement receiver and argument'); return 'patched'; }
check(selected.join((selected.join = replacement, '')) === 'saved' && replacementCalls === 0,
  'callee is captured before argument changes public join');
check(selected.join('!') === 'patched' && replacementCalls === 1, 'patched source join uses ordinary invocation');

let getterCalls = 0, getterArgumentCalls = 0;
const viaGetter = ['getter'];
Object.defineProperty(viaGetter, 'join', { configurable: true, get() {
  getterCalls++; gc(); return Array.prototype.join;
} });
check(viaGetter.join((getterArgumentCalls++, ':')) === 'getter' && getterCalls === 1 && getterArgumentCalls === 1,
  'join getter and argument are read once');
const marker = {};
throws(function() { viaGetter.join((getterArgumentCalls++, function() { throw marker; }())); }, marker);
check(getterCalls === 2 && getterArgumentCalls === 2, 'argument throw follows the once-read callee');

let proxyCalls = 0;
const viaProxy = ['proxy'];
viaProxy.join = new Proxy(Array.prototype.join, { apply(target, receiver, args) {
  proxyCalls++; check(receiver === viaProxy && args.length === 1 && args[0] === ':', 'proxy receives saved operands');
  gc(); return Reflect.apply(target, receiver, args);
} });
check(viaProxy.join(':') === 'proxy' && proxyCalls === 1, 'proxy callable cannot bypass its trap');
const viaBound = ['bound'];
viaBound.join = Array.prototype.join.bind(viaBound);
check(viaBound.join(':') === 'bound', 'bound native callable retains ordinary invocation');
const inherited = ['inherited'];
let inheritedCalls = 0;
Object.setPrototypeOf(inherited, { join(separator) { inheritedCalls++; check(this === inherited && separator === '|', 'inherited receiver'); return 'custom'; } });
check(inherited.join('|') === 'custom' && inheritedCalls === 1, 'inherited source method is not inferred native identity');

let coercions = 0;
const separatorEffect = ['old', 'tail'];
check(separatorEffect.join({ toString() { coercions++; separatorEffect[0] = 'new'; gc(); return ':'; } }) === 'new:tail' && coercions === 1,
  'unsupported separator uses one ordered native coercion');
throws(function() { [].join({ toString() { gc(); throw marker; } }); }, marker);
let symbolThrew = false;
try { [].join(Symbol('separator')); } catch (error) { symbolThrew = error instanceof TypeError; }
check(symbolThrew, 'empty join still converts unsupported Symbol separator');
symbolThrew = false;
try { ['prefix', Symbol('element')].join(':'); } catch (error) { symbolThrew = error instanceof TypeError; }
check(symbolThrew, 'unsupported Symbol element follows generic error path');

let indexGets = 0, conversions = 0;
const holes = ['first', , 'last'];
const holePrototype = Object.create(Array.prototype);
Object.defineProperty(holePrototype, '1', { configurable: true, get() { indexGets++; gc(); return 'inherited'; } });
Object.setPrototypeOf(holes, holePrototype);
check(holes.join(':') === 'first:inherited:last' && indexGets === 1, 'hole keeps inherited indexed getter');
const objectElements = [{ toString() { conversions++; objectElements[1] = 'updated'; gc(); return 'object'; } }, 'stale'];
check(objectElements.join(':') === 'object:updated' && conversions === 1, 'unsupported element preserves live reads and one conversion');
const generic = { join: Array.prototype.join, get length() { indexGets++; return 1; }, 0: 'generic' };
check(generic.join(':') === 'generic' && indexGets === 2, 'borrowed method keeps generic length getter');

let extraTrace = '';
const extra = ['extra'];
check(extra.join((extraTrace += 'first;', ':'), (extraTrace += 'ignored;', 'unused')) === 'extra' &&
  extraTrace === 'first;ignored;', 'extra arguments are evaluated before generic invocation');
let spreadTrace = '', nextIndex = 0;
const spread = { [Symbol.iterator]() {
  spreadTrace += 'iterator;';
  return { next() { spreadTrace += 'next;'; return nextIndex++ < 2 ? { value: ':', done: false } : { done: true }; } };
} };
check(['spread'].join(...spread) === 'spread' && spreadTrace === 'iterator;next;next;next;',
  'spread arguments retain original expansion exactly once');

const foreign = __scriptwCreateRealm().global;
const foreignJoin = ['foreign'];
foreignJoin.join = foreign.Array.prototype.join;
check(foreignJoin.join(':') === 'foreign', 'foreign defining Realm retains actual native invocation');
let foreignError;
try { foreignJoin.join(Symbol('separator')); } catch (error) { foreignError = error; }
check(Object.getPrototypeOf(foreignError) === foreign.TypeError.prototype, 'foreign native error keeps defining Realm');
print('array-packed-join-call:ok');
262;
