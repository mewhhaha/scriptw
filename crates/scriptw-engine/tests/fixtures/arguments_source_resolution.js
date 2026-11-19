// Every source `arguments` reference resolves statically: the nearest
// declaration of that name, else the nearest ordinary function's implicit
// binding. Reads, writes, destructuring and closures share that one binding.
var out = [];
function log(x) { out.push(String(x)); }
function f1() { return typeof arguments; var arguments = 1; }
log(f1(1, 2));
function f2(a) { arguments = 7; var g = () => arguments; return [arguments, g()].join(); }
log(f2(1));
function f3(a) { var g = () => { arguments = 9; }; g(); return arguments; }
log(f3(1));
function f4(a) { var arguments = 5; return arguments + (() => arguments)(); }
log(f4(1));
function f5(a) { var arguments; return typeof arguments + arguments.length; }
log(f5(1, 2, 3));
function f6(arguments) { return [arguments, (() => arguments)()].join(); }
log(f6(42));
function f7() { function arguments() { return 'fn'; } return typeof arguments + (() => typeof arguments)(); }
log(f7());
function f8() { { let arguments = 'blk'; return [arguments, (() => arguments)()].join(); } }
log(f8(1));
function f9(a, b) { arguments[0] = 'm'; b = 'n'; return [a, arguments[1], arguments.length].join(); }
log(f9(1, 2));
function f10(a) { 'use strict'; arguments[0] = 'm'; return [a, arguments[0]].join(); }
log(f10(1));
function f11(a = arguments.length, b = () => arguments[0]) { var arguments; return [a, b(), typeof arguments].join(); }
log(f11(undefined, undefined, 3));
function f12(a = () => arguments) { var arguments = 'body'; return [typeof a(), arguments].join(); }
log(f12());
function f13() { [arguments] = [3]; return arguments; }
log(f13(1));
function f14() { ({ x: arguments } = { x: 4 }); return (() => arguments)(); }
log(f14(1));
function f15() { arguments += 'z'; return arguments; }
log(f15(1));
function f16() { arguments = 1; arguments++; return arguments; }
log(f16());
function f17() { for (arguments of [8]); return arguments; }
log(f17());
function f18() { for (arguments in { k: 1 }); return arguments; }
log(f18());
function f19() { return delete arguments; }
log(f19());
function f20(a) { return (() => () => arguments[0])()(); }
log(f20('deep'));
function f21() { function inner() { return arguments.length; } return inner(1, 2, 3, 4) + ':' + arguments.length; }
log(f21(1));
function f22() { arguments = function () { return 'called'; }; return arguments(); }
log(f22());
function f23() { try { throw 'c'; } catch (arguments) { return arguments + (() => arguments)(); } }
log(f23());
function f24() { var o = { arguments: 'with' }; with (o) { return arguments; } }
log(f24());
function f25() { var o = {}; with (o) { return typeof arguments; } }
log(f25());
function f26() { var g = () => arguments; return g() === arguments; }
log(f26());
var object = { m() { return arguments.length; }, get g() { return arguments.length; } };
log(object.m(1, 2) + ',' + object.g);
function* generator() { yield arguments.length; arguments = 'gw'; yield arguments; }
var iterator = generator(1, 2);
log(iterator.next().value + ',' + iterator.next().value);
class C { constructor() { this.n = arguments.length; } m() { return (() => arguments.length)(); } }
log(new C(1, 2, 3).n + ',' + new C().m(1));
function f27(x) { switch (x) { case 1: let arguments = 'sw'; return arguments; } }
log(f27(1));
function f29(a = typeof arguments) { return a; }
log(f29());
function f30() { return arguments.callee === f30; }
log(f30());
var expected = [
  'object', '7,7', '9', '10', 'object3', '42,42', 'functionfunction', 'blk,blk', 'm,n,2', '1,m',
  '3,,object', 'object,body', '3', '4', '[object Arguments]z', '2', '8', 'k', 'false', 'deep',
  '4:1', 'called', 'cc', 'with', 'object', 'true', '2,0', '2,gw', '3,1', 'sw', 'object', 'true',
].join('|');
if (out.join('|') !== expected) throw new Error(out.join('|'));
print('arguments resolution ok');
