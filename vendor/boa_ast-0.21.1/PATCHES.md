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

## Source nesting limit

The recursive-descent parser recursed once per nesting level with no bound,
so deeply nested source (about 4,000 parentheses, brackets or blocks on the
64 MiB compiler stack) aborted the process with a native stack overflow.
`Cursor::nested` (`src/parser/cursor/mod.rs`) counts the recursive
productions that enclose the current token and reports a SyntaxError past
`parser::MAX_NESTING_DEPTH` (1,000). Every grammar cycle passes a counted
production: AssignmentExpression, Statement and FunctionStatementList are
wrapped whole; the unary, prefix update, `await` and `**` operands, the `new`
callee, ClassHeritage and the two binding patterns count only their recursive
operand, so one source level costs one count. Test: engine
`source_nesting_beyond_the_limit_is_a_syntax_error_not_a_stack_overflow`.
