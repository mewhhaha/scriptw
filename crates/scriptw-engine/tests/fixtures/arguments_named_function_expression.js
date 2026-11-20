// A function expression named `arguments` binds that name only in the outer
// record that holds its own name. FunctionDeclarationInstantiation gives the
// function an own `arguments` binding (the arguments object, or the parameter
// or declaration that suppresses it), which shadows the name everywhere in
// its parameters and body, in every function kind and through arrows and eval.
var out = [];
function log(x) { out.push(String(x)); }
var f28 = function arguments() { return typeof arguments; };
log(f28(1, 2));
log((function arguments() { return arguments.length; })(1, 2));
log((function* arguments() { yield typeof arguments; })().next().value);
log((function arguments() { return (() => typeof arguments)(); })());
log(({ arguments: function arguments() { return arguments.length; } }).arguments(1, 2, 3));
log((function arguments() { return eval('typeof arguments'); })());
log((function arguments() { eval('var arguments = 7'); return arguments; })());
log((function arguments() { var arguments; return typeof arguments; })());
log((function arguments(arguments) { return arguments; })(5));
// A write reaches the arguments binding, not the immutable outer name.
log((function arguments() { arguments = 4; return arguments; })());
var callee = function arguments() { return arguments.callee === callee; };
log(callee());
log((function arguments() { return function () { return typeof arguments; }(); })());
// Strict functions and arrows inside a sloppy one named `arguments`.
log((function arguments(a) {
  return (function () { 'use strict'; return arguments.length; })(1, 2, 3) + ':' +
    (() => { 'use strict'; return arguments[0]; })();
})(9));
log((function arguments(a = typeof arguments) { return a; })());
log(f28.name + ',' + typeof f28);
var asyncResult = 'pending';
(async function arguments() { return typeof arguments + arguments.length; })(1)
  .then(function (value) { asyncResult = value; });
// Strict code cannot name a function expression `arguments`.
var strictErrors = [];
for (var source of ['"use strict"; (function arguments() {});',
                    '(function arguments() { "use strict"; });',
                    '(function* arguments() { "use strict"; });']) {
  try { (0, eval)(source); strictErrors.push('accepted'); } catch (e) { strictErrors.push(e.name); }
}
var expected = [
  'object', '2', 'object', 'object', '3', 'object', '7', 'object', '5', '4', 'true', 'object',
  '3:9', 'object', 'arguments,function',
].join('|');
Promise.resolve().then(function () {}).then(function () {
  var actual = out.join('|') + ';' + asyncResult + ';' + strictErrors.join();
  if (actual !== expected + ';object1;SyntaxError,SyntaxError,SyntaxError') throw new Error(actual);
  print('named arguments ok');
});
