# Arguments source binding resolution

Authority: ECMA-262 `5345883164f463e87f8b40aca4956157ecba8783`,
FunctionDeclarationInstantiation, ResolveBinding, Declarative Environment Records
and destructuring assignment evaluation. An ordinary function's implicit
arguments binding precedes an outer/global binding with that name. A simple
function's `var arguments` reuses the initialized arguments value. Parameter
expressions keep their separate parameter/body environments. Arrows observe
the mutable lexical binding; they do not retain a copy of its old value.

The historical actual `aee6bc109` compiler records Bugs in sloppy modes of
`language/statements/function/S13_A15_T4.js` and `S13_A15_T5.js`, plus three
`language/statements/for-await-of/` destructuring cases targeting `arguments`.
Read fallback selects an ArgumentsObject while writes resolve a different
source/global slot, and an outer global can precede the own-arguments fallback.

Analysis resolves every `arguments` reference statically
(`analysis/arguments_reference.rs`): walking the lexical chain it finds a
source binding named `arguments`, the nearest ordinary function's implicit
binding, or, outside any function, an ordinary global reference. Reads,
writes, destructuring and update targets, for-in/of heads, shorthand
properties and arrow captures all use that one result, so a Script global can
no longer precede a function's own `arguments`. A reference no longer makes
any environment eval-visible; only direct eval and the Annex B
`function arguments` form still do. (Before `c294028e7` a whole-Script scan
made every activation eval-visible, which sent every identifier in a classic
Script through runtime name lookup.)
The existing hidden arguments slot is exported under its source name when no
real source declaration masks it. A simple source `var arguments` slot starts
with the protocol's initialized ArgumentsObject value rather than undefined;
all source accesses then share that published cell. Real parameters, root
function declarations and separate body records retain their existing ordering.
This deliberately uses the complete named-record path instead of adding a
partial hidden-slot alias with stale capture/value metadata.

Controls cover own/global collision, var-without-initializer, reassignment and
escaping arrows, destructuring in async functions/generators, parameter and
lexical shadows, and with precedence. The original executions and every prior
passing inventory remain required. This is prepared source; the wider named
environment transition requires complete affected native verification.
