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
any environment eval-visible. `AnalysisBuilder::finish` in `analysis.rs` makes
the whole Script eval-visible in exactly these cases: the source contains a
direct eval call; it contains an Annex B `function arguments` candidate; the
Script is itself a prepared direct-eval Script; it is a strict Script prepared
for indirect eval or `ShadowRealm.prototype.evaluate`; or the compilation
enables Realm-reusable module environments, which sets the flag after the
static module-binding decision has read it. `with` is not one of them. Inside
a `with` body the static result is the fallback only: lowering tests the Object Environment
Record first at run time, and the stable implicit-arguments cell is not used
there. (Before `c294028e7` a whole-Script scan made every activation
eval-visible, which sent every identifier in a classic Script through runtime
name lookup.)
Where the owner's activation has no source binding named `arguments`, lowering
declares that source name as an alias of the hidden implicit storage
(`LEXICAL_ARGUMENTS_NAME`), so reads, writes and closures share one cell.
Unreassigned reads keep `ExprIr::Arguments`; captured reads keep the
`StableImplicitArgumentsCell` fact. A simple source `var arguments` slot starts
with the protocol's initialized ArgumentsObject value rather than undefined;
all source accesses then share that published cell. Real parameters, root
function declarations and separate body records retain their existing ordering.

A function expression named `arguments` keeps that name only in the outer
record that holds its own name (`named_expression_binds_self_in_activation`
in `names.rs`). Scriptw binds other self names in the activation too, but the
function's own `arguments` binding shadows this one everywhere in its
parameters and body, so the activation holds the implicit binding and every
reference, arrow, nested eval and write resolves to it; strict code cannot
use the name. (Until the fix this resolved to the function itself:
`(function arguments(){ return typeof arguments })()` gave `function`.)
`arguments_named_function_expression.js` covers plain, generator, async,
method-valued and parameter-default forms, arrows, eval, `var` and parameter
shadows, writes, `callee`, strict inner functions and the strict early errors.

Controls cover own/global collision, var-without-initializer, reassignment and
escaping arrows, destructuring in async functions/generators, parameter and
lexical shadows, and with precedence. The original executions and every prior
passing inventory remain required.
