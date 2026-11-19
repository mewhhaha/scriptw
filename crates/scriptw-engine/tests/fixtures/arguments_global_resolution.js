// A Script `var arguments` is an ordinary global binding. It never precedes a
// function's own implicit binding, and top-level arrows resolve it globally.
var out = [];
var arguments = 'global';
function f() { return arguments; }
out.push(typeof f() + ':' + f(1).length);
out.push(arguments);
var g = () => arguments;
out.push(g());
arguments = 'reassigned';
out.push(g());
out.push(typeof globalThis.arguments);
{ let arguments = 'blk'; out.push((() => arguments)()); }
function h() { return (() => typeof arguments)(); }
out.push(h());
var expected = 'object:1|global|global|reassigned|string|blk|object';
if (out.join('|') !== expected) throw new Error(out.join('|'));
print('global arguments ok');
