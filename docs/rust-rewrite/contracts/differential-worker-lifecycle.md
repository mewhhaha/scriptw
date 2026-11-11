# Differential worker lifecycle

Status: source drafted for the 2026-10-04 implementation batch. Compilation,
worker execution, source guards and pinned acceptance remain unverified.

## Source and process ownership

`DifferentialReplayInput` is a native-only corpus owner. Loading JSON checks the
closed protocol, metadata and graph wire without parsing JavaScript. Legacy
v1/v2/v3 Script closure admission occurs inside the selected worker; v4 uses the
same immutable embedded graph authority. `DifferentialWorkerRunner` is mandatory
for replay and every initial or reduced generated candidate. There is no parent
backend executor or current-test-executable heuristic.

The CLI defaults to its own real executable. Library embedders and Cargo tests
select a real worker with `--worker-bin PATH` or the runner constructor. The
named `scriptw-differential-worker` Cargo binary requires the oracle feature. The
CLI's private worker dispatch runs before Realm or Engine construction, with
one inherited job. Default builds return `OracleNotLinked` before spawning.
Unsupported process platforms fail explicitly; there is no in-process fallback.

## Attempt authority and bounds

Each backend gets a new process group and private mode-0700 staging directory.
Native request and journal files are bounded at 16 MiB; an individual journal
frame is bounded at 1 MiB. Raw standard input, output and error are null. The
first frame or journal budget failure terminates the worker before Engine output
capture can continue to grow. These IO bounds complement the enclosing 4096 MiB
aggregate verification cgroup, rather than providing a separate RSS limit.

The parent writes and flushes the bounded native request before starting the
attempt clock. That clock starts before process spawn and covers worker startup,
image hashing, JavaScript admission, Realm construction, parsing, compilation
and execution. It applies to both Wasm-AOT and spec-exec, with no startup
allowance. The Store's [execution deadline](execution-deadlines.md) remains an
additional child-side mechanism.
Parent JSON loading and request serialization are outside the attempt deadline.

The closed journal lifecycle is Header, Admitted, ordered PrintLine frames, then
one Terminal. An explicit AdmissionRejected terminal is a corpus error, distinct
from a missing or malformed execution result. The header binds the request token,
backend, case fingerprint, goal, filename, protocol and observation contract to
the compiler's embedded source/revision and the selected executable's SHA-256.
The graph digest is already part of v4 case identity. A mismatched image or
source, invalid phase, sequence, count, primitive domain or trailing frame cannot
mint the private non-cloneable completed attempt.

Normal exit, timeout, malformed observation and polling failure all retire the
process group, reap the direct child and remove staging before a completed
observation can publish. Direct-child cleanup has a bounded grace; any cleanup
failure remains red. Committed validated print frames and available provenance
survive incomplete execution. Missing capture is never an empty transcript.

## Reports, reduction and controls

Worker failures are a distinct execution and verdict domain. They take priority
over semantic comparison, stay red even when both workers fail identically, and
receive no mismatch signature. Completed JS throws, backend Engine failures and
unsupported Object/Symbol observations retain their existing meanings. Corpus
wire bytes and case/mismatch fingerprint domains remain unchanged.

A worker failure during the initial campaign or any reduction candidate returns
the actual rejected case and report immediately. It cannot be retained as a
semantic mismatch, reported as a reducer fixed point, or persisted as a corpus
case. The CLI prints the red report and fails without creating the output file.

Authored controls retain the three foundation, seven graph and three generated
or probe cohorts. They select Cargo's real worker explicitly and serialize
execution. Additional controls cover fresh state, selected-image provenance,
SpecExec nontermination with committed print prefix, worker-only admission,
wrong executable selection, process/staging retirement, reduction rejection and
nonpersistence. CLI controls join real worker dispatch and embedder selection to
those core boundaries. None has run against this draft.

This process boundary adds bounded isolation, not an object identity, descriptor,
prototype, error-Realm or arbitrary-side-effect comparator. Every green result
still reports `semantic_equivalence: not_established`. Full T25 generation,
performance/CI and conformance acceptance remain open.
