# Labelled-break iterator closing

The workspace pins Boa 0.21.1 as the spec-exec oracle. The local repair in
`src/bytecompiler/statement/break.rs` closes each skipped inner iterator loop
when a labelled break transfers to an outer loop or block. The target loop
retains its own close operation. This follows the pinned ECMA-262
ForIn/OfBodyEvaluation IteratorClose rule and mirrors the existing continue
action handling without changing the dependency version.

Strict/sloppy spec-exec controls cover local and outer breaks, outer continue,
finally, throwing close identity and asynchronous for-await closing. The
[exact repair evidence](../../docs/rust-rewrite/generator-oracle-repair-20261012.json)
retains the original differential requests and all subsequent verdicts. These
selected checks do not establish full upstream Boa or Test262 acceptance.

# Direct eval in a function body with its own variable record

A sloppy function whose parameter list contains an expression has three
records: the function environment, the record of the parameters, and the
body's variable environment record inside it (ECMA-262
FunctionDeclarationInstantiation steps 20 and 28). Boa treated the function
environment as the VariableEnvironment of every eval in the function. A sloppy
direct eval in the body therefore walked through the parameter record in
EvalDeclarationInstantiation step 3.d and threw `SyntaxError` for a `var`
named like a parameter, or for a name the body itself declares; its other
`var` and function names landed in the function environment, where closures
of the parameter list could see them. The scope analysis also dropped the
body record whenever the body's own code declared nothing in it.

The repair has two parts. In `boa_ast` (`src/scope.rs`,
`src/scope_analyzer.rs`, shared with the product front end, which reads none
of the changed scope data), a function that contains a direct eval keeps the
body record of a sloppy parameter-expression function at runtime. In this
crate, `PushVariableScope` pushes that record with a per-call copy of its
scope (`src/vm/opcode/push/environment.rs`,
`src/environments/runtime/{mod.rs,declarative/{mod.rs,lexical.rs}}`); direct
eval declares into the innermost such record or function environment
(`src/builtins/eval/mod.rs`), and a name lookup finds in the record the
names an eval added, not the ones the compiled body already resolved. The
bytecompiler takes the body record as the variable scope of body and eval
code (`src/bytecompiler/{env.rs,declarations.rs,mod.rs}`), so Annex B block
functions assign to the binding there. An eval in the parameter list still
declares into the function environment and still meets the parameters.

The spec-exec control is
`sloppy_direct_eval_declares_into_the_function_body_record` in
`crates/scriptw-spec-exec/src/lib.rs`. It is a selected check, not upstream
Boa or Test262 acceptance. The `flowgraph` feature is not built here and its
opcode table was edited without compiling it.
