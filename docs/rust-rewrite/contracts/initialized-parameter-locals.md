# Initialized parameter locals

An ordinary function reaches its body only after every parameter initializer
has completed normally. `bind_parameters` retains checked bindings throughout
argument/default/destructuring initialization, then creates an
`InitializedParameterLocal` proof for each parameter still held in local storage.
Only that producer can construct the proof. The local-binding owner consumes it
to retire the initialized flag and omit subsequent body TDZ branches.

The transition preserves the parameter's value and binding identity. Duplicate
sloppy parameters can seal the same binding repeatedly. Later writes, var
redeclarations and named function declarations keep that initialized binding;
nested lexical declarations own separate storage and retain their checks.

Environment cells, including captured and direct-eval-visible parameters, never
receive the proof. Generator, async and module activations retain their existing
frame initialization protocol. Default initializer failures and escaped closures
over unfinished parameter cells therefore retain their original TDZ behavior.

Emission coverage checks zero body TDZ branches for ordinary parameters and
a frontend-proven ready default. The retained-operand control still checks
source lexical reads. Runtime coverage includes default TDZ, captured/generator
controls, script/module initialization order, abrupt
destructuring, escaped cells after failed initialization, separate parameter/body
environments, eval, duplicate mappings and resumption. Native frame size and
request effects require matched measurements; removed cold branches alone do
not establish a speedup.
