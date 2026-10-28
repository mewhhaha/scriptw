# Early-error diagnostics

`scriptw-front` owns `EarlyErrorCode`, the message-pattern table and parser
classification. `scriptw-ir/src/early_error_code.rs` reexports that domain and
exhaustively maps codes to rejection kind; phase and exception type derive from
the kind. There is no second diagnostic table in IR.

Every entry and dependency parse preserves its actual Script/Module goal,
strictness, callable kind and source context. Typed AST owners cover parser
boundary gaps such as late strict directives, parameter/body restrictions,
class constructors and private names. Shared predicates must not erase the
production that selects the diagnostic. A parsed early error must not be
relabelled as an execution failure or a missing backend capability.

Prepared eval/Function sources retain their parser diagnostic and phase through
lowering and runtime error construction. Unknown diagnostics are not silently
accepted, skipped or counted as successful conformance executions.

`ParseFailurePattern` uses intentional static-table value semantics: its rows
contain immutable references and are repeatedly read. The source guard records
`54/18/1` pattern ownership across six exhaustive consumers and 91 lexical mentions,
with no wildcard or `matches!` observer. Changes to that closed domain require
updating the real producers and consumers together, rather than adding a broad
message catch-all.

Focused structure checks live in
`scriptw-front/tests/parse_failure_pattern_structure.rs`; semantic parser,
module and prepared-source controls supply behavior evidence. Published
Test262 results retain their own exact source and execution identity.
