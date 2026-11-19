// Source bindings spelled like compiler-private storage are ordinary
// bindings: they never alias an arrow's lexical this, arguments or
// [[HomeObject]], nor a destructuring loop head's temporary.
var results = [];
function check(label, actual, expected) {
  if (actual !== expected) throw new Error(label + ': ' + String(actual) + ' !== ' + String(expected));
  results.push(label);
}
var T = {v: 'this'};
function viaVar() { var $this = {v: 'var'}; return (() => this.v)() + $this.v; }
check('var $this', viaVar.call(T), 'thisvar');
function viaLet() { let $this = {v: 'let'}; return (() => this.v)() + $this.v; }
check('let $this', viaLet.call(T), 'thislet');
function viaParam($this) { return (() => this.v)() + $this.v; }
check('param $this', viaParam.call(T, {v: 'param'}), 'thisparam');
function viaCatch() { try { throw {v: 'catch'}; } catch ($this) { return (() => this.v)() + $this.v; } }
check('catch $this', viaCatch.call(T), 'thiscatch');
function viaFunction() { function $this() { return 'fn'; } return (() => this.v)() + $this(); }
check('function $this', viaFunction.call(T), 'thisfn');
function viaClass() { class $this { static get v() { return 'class'; } } return (() => this.v)() + $this.v; }
check('class $this', viaClass.call(T), 'thisclass');
function viaPattern() { var {v: $this} = {v: 'pattern'}; return (() => this.v)() + $this; }
check('pattern $this', viaPattern.call(T), 'thispattern');
function viaForOf() { var r = ''; for (var $this of ['loop']) r += (() => this.v)() + $this; return r; }
check('for-of $this', viaForOf.call(T), 'thisloop');
function innerArrow() { return (() => { var $this = {v: 'inner'}; return this.v + $this.v; })(); }
check('arrow-local $this', innerArrow.call(T), 'thisinner');
function unbound() { return (() => typeof $this)(); }
check('unbound $this', unbound.call(T), 'undefined');
function* generator() { var $this = 'gen'; yield (() => this.v)() + $this; }
check('generator $this', generator.call(T).next().value, 'thisgen');
var getterOwner = { v: 'getter', get g() { var $this = 'x'; return (() => this.v)() + $this; } };
check('getter $this', getterOwner.g, 'getterx');
var $this = 'outer';
class Fielded { v = 'field'; g = () => this.v + $this; }
check('class field $this', new Fielded().g(), 'fieldouter');
function viaArguments() { var $arguments = 3; return $arguments + arguments.length + (() => arguments[0])(); }
check('var $arguments', viaArguments(10, 20), 15);
function letArguments() { let $arguments = 'L'; return $arguments + arguments.length; }
check('let $arguments', letArguments(1), 'L1');
function paramArguments($arguments) { return $arguments + (() => arguments.length)(); }
check('param $arguments', paramArguments(5, 6, 7), 8);
class A { m() { return 'A.m'; } }
class B extends A { m() { var $homeObject = {}; return (() => super.m())() + typeof $homeObject; } }
check('var $homeObject', new B().m(), 'A.mobject');
class C extends A { m() { return (() => typeof $homeObject)(); } }
check('unbound $homeObject', new C().m(), 'undefined');
var literal = { m() { var $homeObject = 5; return (() => typeof super.toString)() + $homeObject; } };
check('object method $homeObject', literal.m(), 'function5');
function forOfTemp() { var seen = ''; for (var [a] of ['xy']) seen += a + typeof $forof0; return seen; }
check('for-of temporary', forOfTemp(), 'xundefined');
function forOfShadow() { var $forof0 = 10; for (let [a] of [[1], [2]]) { $forof0 += a; } return $forof0; }
check('for-of shadow', forOfShadow(), 13);
function forOfClosure() { var fs = []; var $forof0 = 10; for (let [a] of [[1], [2]]) fs.push(() => a + $forof0); return fs.map(f => f()).join(); }
check('for-of closure', forOfClosure(), '11,12');
function forInTemp() { var s = ''; var $forin0 = 'q', $forin1 = 'q'; for (var [a] in {xy: 1, zw: 2}) s += a + $forin0 + $forin1; return s; }
check('for-in temporary', forInTemp(), 'xqqzqq');
print('private binding names ok ' + results.length);
