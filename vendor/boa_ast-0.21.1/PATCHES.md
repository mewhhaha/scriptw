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
