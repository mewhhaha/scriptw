# Short-circuit previous-expression exhaustiveness

Status: normative for the vendored Boa parser state that enforces ECMAScript's
unparenthesized logical/coalesce mixing restriction.

## Closed state

The private `PreviousExpr::{None, Logical, Coalesce}` domain records which
short-circuit operator family has already appeared in the current unparenthesized
expression. It retains `Clone` and `Copy` because the parser configuration and
loop legitimately project the small value repeatedly, but supports no equality
comparison.

The `&&`, `||` and `??` branches instead use three exhaustive operator matches.
The two logical branches accept `None | Logical` and reject `Coalesce`; the
coalesce branch accepts `None | Coalesce` and rejects `Logical`. Adding a fourth
state fails to compile at all three observers until its grammar relationship is
chosen explicitly. Equality would let those observers keep compiling while the
new state silently followed the accepting path.

## Guard and semantics

The Rust-lexical guard removes comments and all Rust string, byte-string,
C-string, raw-string, character and byte-character literals before checking the
exact private declaration, capability absence, 15-identifier census, three
exhaustive matches and wildcard absence. Its semantic witness rejects all four
logical/coalesce orderings under Script and Module goals while accepting
parenthesized mixtures and same-family chains.

This is source-equivalent parser hardening. It changes no token consumption,
AST, diagnostic wording, phase or span, and makes no parser-behavior or
conformance claim. The focused structure-and-semantics target passes `4/4`.
Broad front-end, IR, engine and Test262 verification remain deferred.
