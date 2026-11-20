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
tree, in units (`parser::MAX_NESTING_UNITS`): a counted production costs
2,048; an `else if` 1,365, two thirds of a level, so about 1,500 compile (V8:
about 3,000; it throws a RangeError at 10,000); a member, call, tagged
template or optional link 4,096, so a chain has about 500 links; and a
binary, logical, `??` or comma operator 1, or 1,024 inside a generator or
async function body, whose resumable lowering still recurses per operator.
The `else if` and member weights also bound native compilation, which grows
faster than those chains (1,000 member links took 1.5 GB, 2,000 `else if`
branches 2.2 GB). `Cursor::measured` returns the height of an operand parsed
by such a loop, and the loop adds its links (`Cursor::chain_height`) and
records the total for the enclosing node (`Cursor::record_height`). `else if`
recurses into `IfStatement` directly instead of through a whole Statement.
`a || b || c` is now parsed in the same loop as `&&` into a left-deep chain,
as ECMA-262 13.13's left-recursive LogicalORExpression has it; it recursed
once per `||` before, unbounded.

The cheap binary operators rest on the rest of the pipeline walking those
chains without recursion: in `boa_ast`, `Binary`'s `VisitWith` visits the
operands of a left-deep chain in a loop (a visitor that must see each inner
node sets `Visitor::VISITS_BINARY_CHAINS_ITERATIVELY` to `false`: the async
generator state planner and the oracle's optimizer), `Binary::span` walks to
the first operand, `Binary` clones a chain from its first operator outwards
and drops nested binary operands from an explicit stack, and
`CoverParenthesizedExpressionAndArrowParameterList` moves the parenthesized
expression instead of cloning it (which recursed through the whole subtree
once per enclosing parenthesis). Scriptw's lowering does the rest. About two million operators fit; a
chain of a million parses, compiles and runs. Tests: front
`source_nesting_limit_is_exact` and
`long_operator_chains_parse_without_recursing_per_operator`; engine
`source_nesting_beyond_the_limit_is_a_syntax_error_not_a_stack_overflow`,
`hundred_thousand_operator_chains_compile_and_run`,
`operator_chains_in_resumable_bodies_stay_within_the_nesting_limit` and
`else_if_and_member_chains_have_twice_the_nesting_limit`.

## Regular expression literal nesting

`src/lexer/regex.rs` validated every non-trivial literal with `regress`'s
recursive parser, which overflowed the stack on about 100,000 nested groups.
A literal whose groups (and, with the `v` flag, classes) nest more than
`lexer::regex::MAX_PATTERN_NESTING_DEPTH` (1,000) levels is now a SyntaxError
before it is parsed (V8 compiles about 10,000 and reports a SyntaxError at
20,000). Test: engine `regular_expression_nesting_is_bounded`.

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
