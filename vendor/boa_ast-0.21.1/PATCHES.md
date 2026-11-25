# Patches to Boa AST 0.21.1 and Boa parser 0.21.1

Both crates are the product's front end as well as the oracle's, so a change
here reaches compiled programs.

## Class static block declared names

`vendor/boa_parser-0.21.1` reported `class C { static { var x; function x() {} } }`
("lexical name declared in var names") and two function declarations of one
name in a static block ("lexical name declared multiple times") as early
errors. Both are valid. The LexicallyDeclaredNames of a
ClassStaticBlockStatementList are the TopLevelLexicallyDeclaredNames of its
StatementList and its VarDeclaredNames the TopLevelVarDeclaredNames (ECMA-262
8.2.4 and 8.2.6), so a function declaration at the root of the block is var
scoped, as in a function body.

`src/operations/mod.rs` here exports `top_level_lexically_declared_names` and
`top_level_var_declared_names` over the existing top-level collectors, and the
parser's static block check
(`vendor/boa_parser-0.21.1/src/parser/statement/declaration/hoistable/class_decl/mod.rs`)
uses them. `let a; function a() {}` and `let a; var a;` stay early errors.

## Body variable record kept for direct eval

`src/scope.rs` and `src/scope_analyzer.rs` keep the body's own variable
environment record of a sloppy function whose parameter list holds an
expression when the function contains a direct eval. Only the oracle reads
that scope data; see the
[Boa Engine notes](../boa_engine-0.21.1/PATCHES.md).

## `let` and `async` ending a sloppy script

`vendor/boa_parser-0.21.1` reported "abrupt end" for a sloppy script whose
last token is the identifier `let` or `async` (`var let = 1; let`,
`var async = 1; async`, `async(1)`). ExpressionStatement only forbids the
lookaheads `let [` and `async [no LineTerminator here] function` (ECMA-262
14.5), so end of input is an ordinary IdentifierReference followed by
automatic semicolon insertion. The `let [` check in
`src/parser/statement/expression/mod.rs` and the async arrow detector in
`src/parser/expression/assignment/mod.rs` now treat a missing token as "not
the restricted form" instead of an error. Strict `let` and `async x` stay
SyntaxErrors. Test: `scriptw-front` `sloppy_let_and_async_identifiers_may_end_the_source`.

## Source nesting limit and operator chains

The recursive-descent parser recursed once per nesting level with no bound,
so deeply nested source (about 4,000 parentheses, brackets or blocks on the
64 MiB compiler stack) aborted the process with a native stack overflow.
`Cursor::nested` (`src/parser/cursor/mod.rs`) counts the recursive
productions that enclose the current token: AssignmentExpression, Statement
and FunctionStatementList are wrapped whole; the unary, prefix update,
`await` and `**` operands, the `new` callee, ClassHeritage and the two
binding patterns count only their recursive operand, so one source level
costs one count, and every grammar cycle of the parser passes a counted
production.

That bounds the parser's recursion, not the tree's height. Loops build some
trees deeper than the parser recursed: binary, logical, `??` and comma chains,
and member, call, tagged template and optional chains add one tree level per
operator, and every later phase walked them recursively (a 100,000-term `+`
chain overflowed the compiler stack). So the limit is on the height of the
tree, in units (`parser::MAX_NESTING_UNITS`), each charge a
`parser::NestingCharge` that the limit error names: a counted production
costs 2,048; an `else if` 1,365, two thirds of a level, so about 1,500
compile (V8: about 3,000; it throws a RangeError at 10,000); a member, call,
tagged template or optional link 4,096, so such a chain has about 500 links,
and each item of an optional chain is charged as the link it would be outside
one; the two links of a method call (`a.m(…)`, `a[k](…)`, `a?.m(…)`: a
property access whose next token is `(`, and a call of a property access)
1,024 each, so about 1,000 chained method calls fit (Node runs 3,000; with
the Speed policy and one compilation job, `SCRIPTW_JOBS=1`, the longest
admitted chain, 950 calls, compiled natively in
647 MB, or 680 MB for `Map#set`; native compilation memory follows the whole
function body, chained or not, which no nesting weight bounds); and a
binary, logical, `??` or comma operator 1. In a generator or async function
body, once a chain has parsed an `await` or `yield` (`Cursor::note_suspension`,
not counting those of functions inside it), every operator of the chain costs
1,024, the ones before the suspension included (`Cursor::binary_link` keeps
both heights): the resumable lowering and state planners recurse per operator
from the suspending operand on and test each such operator's operands for a
suspension. A chain without one costs what it costs anywhere. An ES module's
top level is charged as such a body too (`Cursor::module_body`), and eight
levels deep (`parser::MODULE_BODY_ENCLOSING_LEVELS`): Scriptw's module
lowering parses it again inside an async arrow function in generated
scaffolding, and a module that passed the Module parse at the light charge
or within eight levels of the limit failed only that reparse, with an error
positioned in the generated text. The forms that reparse rewrites are
charged what the rewrite adds: `export var` the Statement it is reached
through once `export` is deleted; an anonymous `export default function`
(and its generator and async forms), which becomes `var <name> = function …`,
a Statement and an AssignmentExpression; an anonymous `export default class`,
which becomes `let <name> = class …`, an AssignmentExpression; and every
`import(…)`, which becomes a call of a generated function, that call's link
(also in a Script, whose dynamic imports are rewritten the same way). A module
chain without a suspension is not limited to about 2,000 operators.
The `else if` and member weights were set for the native compilation cost of
one chain, which grows faster than its length: a property access chain's
native compilation memory is quadratic in its length (450 links took 0.5 GB
with the Fast policy). They bound one chain only: native compilation memory
follows the whole function body, so several chains in one body cost about
their sum, which no nesting weight bounds. `Cursor::measured` returns the height of an
operand parsed by such a loop, and the loop adds its links
(`Cursor::chain_height`) and records the total for the enclosing node
(`Cursor::record_height`). `else if` recurses into `IfStatement` directly
instead of through a whole Statement. `a || b || c` is now parsed in the same
loop as `&&` into a left-deep chain, as ECMA-262 13.13's left-recursive
LogicalORExpression has it; it recursed once per `||` before, unbounded.

The cheap binary operators rest on the rest of the pipeline walking those
chains without recursion: in `boa_ast`, `Binary`'s `VisitWith` visits the
operands of a left-deep chain in a loop (a visitor that must see each inner
node sets `Visitor::VISITS_BINARY_CHAINS_ITERATIVELY` to `false`: the async
generator state planner and the oracle's optimizer), `Binary::span` walks to
the first operand, `Binary` clones a chain from its first operator outwards
and drops nested binary operands from an explicit stack, and
`CoverParenthesizedExpressionAndArrowParameterList` moves the parenthesized
expression instead of cloning it (which recursed through the whole subtree
once per enclosing parenthesis) and converts an arrow function's
parenthesized comma chain to parameters in a loop (it recursed once per
parameter). Scriptw's lowering does the rest, and bounds the IR those chains
become by the source's real nesting: it binds a chain's value to a temporary
every 256 operators and right after an operand whose own chains are deep, and
arranges the temporaries as a tree, so a chain adds about two IR levels above
its deepest operand when that operand dominates, and about log2(k) + 1 above k
comparably deep operands (nested chains were 255 IR levels per parenthesis,
and the emitter overflowed at about 230 such parentheses). Template literals
fold their parts the same way: the parser loops over the substitutions, and
each `${}` nested the IR one level deeper before, so a flat template of 30,000
substitutions overflowed the emitter (Node runs 100,000). Each substitution
still adds about 500 bytes to its function's Wasm body, so from about 14,000
substitutions in one function the body exceeds the Wasm body size limit, a
compile error the compile memory item of the inventory owns. About two million
operators fit the parser; a chain of a million parses, and one of 100,000
compiles and runs. Tests: front `source_nesting_limit_is_exact`,
`long_operator_chains_parse_without_recursing_per_operator`,
`operator_chains_in_resumable_bodies_pay_only_when_they_suspend`,
`module_bodies_are_charged_as_their_reparse_inside_the_module_scaffolding` and
`member_chains_are_charged_by_link_kind`; IR
`nested_operator_chains_lower_to_ir_as_deep_as_their_nesting` and
`template_literals_lower_to_bounded_ir`; engine
`source_nesting_beyond_the_limit_is_a_syntax_error_not_a_stack_overflow`,
`hundred_thousand_term_chains_compile_and_run`,
`template_bodies_past_the_wasm_body_size_limit_are_a_compile_error`,
`long_templates_keep_their_evaluation_order`,
`module_nesting_is_charged_as_its_scaffolded_reparse`,
`rewritten_module_forms_are_charged_as_their_reparse`,
`operator_chains_in_resumable_bodies_pay_only_when_they_suspend`,
`nested_chains_split_into_temporaries_keep_their_evaluation_order`,
`long_chains_in_class_keys_and_heritage_compile` and
`else_if_and_member_chains_are_bounded_by_their_compile_cost`.

## Formal parameter count

ECMA-262 sets no limit on formal parameters. A function with more than
65,535 (`parser::MAX_FORMAL_PARAMETERS`) is now a SyntaxError (Node 24's V8
accepts at most 65,525, although its message says 65,534),
from `FormalParameters` and from the arrow function cover grammar; 300,000
arrow parameters overflowed the stack before. Tests: front
`parameter_lists_are_bounded_and_converted_without_recursion`; engine
`arrow_parameter_lists_are_bounded`.

## Regular expression literal nesting

`src/lexer/regex.rs` validated every non-trivial literal with `regress`'s
recursive parser, which overflowed the stack on about 100,000 nested groups.
A literal whose groups (and, with the `v` flag, classes) nest more than
`lexer::regex::MAX_PATTERN_NESTING_DEPTH` (1,000) levels is now a SyntaxError
before it is parsed (V8 compiles about 10,000 and reports a SyntaxError at
20,000). The scan, `lexer::regex::pattern_nesting_exceeds`, is public:
Scriptw's own pattern compiler applies it to every pattern it is given,
whatever its source. Tests: engine `regular_expression_nesting_is_bounded`;
IR `regexp_patterns_nesting_past_the_limit_are_refused_before_parsing`.

## `export default async`

After `export default`, `src/parser/statement/declaration/export.rs` treated
every `async` token as the start of an async function or generator
declaration, so `export default async () => 1`, `export default async(1)`,
`export default async` (the identifier) and `export default async` followed by
a line break and a function declaration were SyntaxErrors. ECMA-262 16.2.3
reserves only `async [no LineTerminator here] function`; any other `async`
starts the AssignmentExpression. It now uses the same lookahead as the
statement-level detector. Test: front
`export_default_async_is_a_declaration_only_before_function_on_the_same_line`.

## Boxed for-in and for-of statements

`Statement::ForInLoop` and `Statement::ForOfLoop` (`src/statement/mod.rs`)
now box their loops, as `Declaration::ClassDeclaration` already did. Inline,
the two 456-byte loops made every `Statement` 464 bytes instead of 224, and
the parser boxes each statement of a list, so Test262's
`staging/sm/regress/regress-610026.js` (about four million `{}` blocks in one
`eval`) peaked at 2.2 GB, nearly all of it empty blocks. Matches that only
borrow the loop are unchanged; a raw pointer used as the loop's identity is
taken from the boxed loop, `&**for_of`. Test: IR `compile_memory`
`an_empty_block_statement_is_a_224_byte_node`.
