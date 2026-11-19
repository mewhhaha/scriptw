// Module (strict) owners keep their implicit arguments storage private
// unless a child arrow captures it: every observation still sees one object.
function same(actual, expected, label) {
  if (!Object.is(actual, expected)) throw new Error(label + ': ' + String(actual));
}
function escape(a, b) { return arguments; }
var escaped = escape(1, 2, 3);
same(escaped.length, 3, 'escape length');
same(escaped[2], 3, 'escape extra');
same(Object.prototype.toString.call(escaped), '[object Arguments]', 'escape tag');
same(Object.keys(escaped).join(), '0,1,2', 'escape keys');
var thrower = Object.getOwnPropertyDescriptor(escaped, 'callee').get;
same(typeof thrower, 'function', 'callee accessor');
var calleeThrew = false;
try { escaped.callee; } catch (error) { calleeThrew = error instanceof TypeError; }
same(calleeThrew, true, 'callee throws');
function arrow(a) { var read = () => arguments[0] + arguments.length; a = 10; return read(); }
same(arrow(5, 6), 7, 'arrow capture');
function arrowLater(a) { var self = arguments; return () => arguments === self && arguments[1]; }
same(arrowLater(1, 'second')(), 'second', 'arrow after return');
function nested(a) {
  function inner(b) { return arguments.length * 10 + arguments[0]; }
  return inner(7, 8, 9) + arguments.length;
}
same(nested(1, 2), 39, 'nested own arguments');
function spread(a) { return Math.max(...arguments); }
same(spread(3, 9, 4), 9, 'spread');
function iterate() { var sum = 0; for (var value of arguments) sum += value; return sum; }
same(iterate(1, 2, 3, 4), 10, 'iteration');
function mutate(a) { arguments[0] = 'changed'; return [a, arguments[0], arguments.length]; }
var mutated = mutate('original');
same(mutated[0], 'original', 'unmapped formal');
same(mutated[1], 'changed', 'mutated element');
function shrink(a, b) { arguments.length = 1; return Array.prototype.slice.call(arguments).length; }
same(shrink(1, 2), 1, 'length write');
function own(a) { return Object.prototype.hasOwnProperty.call(arguments, '1') + typeof arguments; }
same(own(1), 'falseobject', 'own and typeof');
same(own(1, 2), 'trueobject', 'own present');
function deleted(a) { delete arguments[0]; return arguments[0]; }
Object.prototype[0] = 'inherited';
same(deleted('own'), 'inherited', 'delete then inherited');
function absent() { return arguments[0]; }
same(absent(), 'inherited', 'absent inherited');
delete Object.prototype[0];
function twice(a) { var first = arguments; return first === arguments; }
same(twice(), true, 'same object');
function viaCall() { return Array.prototype.join.call(arguments, '-'); }
same(viaCall('a', 'b', 'c'), 'a-b-c', 'array method');
function recursive(n) { return n ? recursive(n - 1) + arguments.length : arguments.length; }
same(recursive(3), 4, 'recursive');
var object = { method(a) { return arguments.length; } };
same(object.method(1, 2), 2, 'method');
class Klass { constructor(a) { this.count = arguments.length; } }
same(new Klass(1, 2, 3).count, 3, 'class constructor');
function* generator() { yield arguments.length; yield arguments[1]; }
var iterator = generator('x', 'y');
same(iterator.next().value, 2, 'generator length');
same(iterator.next().value, 'y', 'generator index');
function captured(a) { var count = arguments.length; var get = function () { return count + a; }; return get() + arguments[1]; }
same(captured(1, 2), 5, 'closure beside arguments');
function block(a) { { let x = arguments[0]; var f = () => x; } return f() + arguments.length; }
same(block(4), 5, 'block env');
function tryCatch(a) { try { throw arguments; } catch (e) { return e[0] + e.length; } }
same(tryCatch(2, 0), 4, 'thrown arguments');
function loop() { var total = 0; for (var i = 0; i < arguments.length; i++) total += arguments[i]; return total; }
same(loop(1, 2, 3), 6, 'computed index loop');
function keyed(k) { return arguments[k]; }
same(keyed(1, 'one'), 'one', 'computed key');
same(keyed('length'), 1, 'length key');
function outer() { var inner = function () { return arguments.length; }; return inner(1) + arguments.length; }
same(outer(1, 2), 3, 'function expression own');
print('module arguments storage ok');
