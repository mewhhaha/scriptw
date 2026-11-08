# Expression operation results

`compile_spec_operation_to_locals` is reached only through the expression
emitter, whose normal payload is the explicit output `ValueLocals`. Its
`SpecOperationResultLocals` constructor separates catalog value-or-throw
operations from `Call`/`Construct`, whose source return-kind protocol remains
unchanged. The operation catalog owns the exhaustive abrupt domain; statement
Return/Break/Continue are not catalog expression results.

Every non-call arm writes its complete result before reaching its operation
exit. This removes a redundant pending-completion initialization. The result
owner copies the whole completion into the enclosing function only on its
Throw edge, retires operand/result roots, then branches to the existing active
handler or source epilogue. Normal fallthrough supplies the output directly and
normalizes the enclosing kind/target without publishing an intermediate
expression value as the enclosing StatementList payload.

Infallible Boolean rows and direct comparison fast paths use that same
output-only contract. They retain operand evaluation and their scalar kernels,
then normalize only the enclosing kind/target; they allocate no pending
completion and copy no normal payload into it or the StatementList.

Statement/return owners publish their explicit expression output. Declaration,
loop-condition and finalizer owners retain their existing StatementList
save/restore rules. Direct helper consumers that intentionally read the current
completion, including ordinary property references, are outside this change.
The five-lane helper ABI, thrown payload identity, Realm selection and
statement/finalizer/suspension completion model are unchanged. On the measured
x86-64 Wasmtime 47 Tail ABI those five integer-class lanes fit in return
registers; the record does not automatically imply a hidden sret pointer.
Copies, branches, live ranges and spills need native measurement.

Emission coverage checks actual ToNumber transport and a source-Call control.
Execution coverage retains ordinary/strict declaration, loop, catch/finally and
source-return completion values, alongside existing arbitrary-throw and GC
continuation regressions. Native code size and runtime effects require a
paired measurement; fewer emitted copies alone are not a timing result.
