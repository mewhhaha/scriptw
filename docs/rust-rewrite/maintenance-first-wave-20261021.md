# First maintenance batch — 2026-10-21

H001–H004, H011, H017, H024 and H091 are implemented. The other 92 hills remain
candidates. Implementation commits are `ab7093a46`, `30065bbb5` and `e7532de91`.
The [machine-readable record](maintenance-first-wave-20261021.json) binds source
maps, exact target inventories, artifact hashes, commands and observed outcomes.
Raw logs remain in `target/maintenance-first-wave-20261021` on the verification
machine; this report preserves their hashes without embedding binaries. The
closing commit updates documentation and a stale name in a Cargo comment only;
parsed Cargo settings are identical to the tested revision.

## Changes

- Removed the C/native product crates, obsolete engine API and CLI routes, and
  REPL placeholder. Wasm is the sole product target. Wasmtime native execution
  and shared native test runtime images remain in use. The program cache domain
  advances to v20 because the obsolete native-target field left its key.
- Made the workspace runner import-safe, with explicit parsing, validation,
  inventory planning and execution. Shared publication defaults and validation;
  made optional Cargo job arguments explicit without shell word splitting.
- Replaced duplicate LDML parsing with one implementation, including isolated
  NumberFormat generation. Regenerated source ownership identities; semantic
  generated data is unchanged.
- Shortened current guidance, moved historical documents without changing a
  byte, and corrected maintained repository URLs and project descriptions.
  Current status lives in the inventory; CONTINUE owns the next-action queue.

## Verification

| Check | Result | Scope / source |
| --- | --- | --- |
| Complete tooling checkpoint | 653 passed, zero skips; 172.214 s | Includes runner, publication, developer wrapper, generator and guard controls |
| Cargo check | Passed; Cargo reported 10 min 14 s | Locked workspace, all targets and all features |
| Engine library | 973 passed, zero failed/ignored | Complete final-source scope at `e7532de91` |
| CLI library | 82 passed, zero failed/ignored | Complete final-source scope |
| CLI integration | 856 passed, zero failed/ignored | Complete final-source inventory, four disjoint serial partitions; every ID once |
| Runtime-image anchor | Zero tests confirmed | Complete final-source auxiliary scope; adds no passes |
| Engine Intl integration | 299 passed, zero failed/ignored | Earlier frozen batch source; see source limits below |
| Focused engine and CLI controls | One pass each | Overlap the complete final scopes; not additional coverage counts |
| Full NumberFormat regeneration | 1,082 locales; generated semantic products byte-identical | Canonical parser copied into isolated staging; payload SHA-256 in JSON |
| Intl source identity recipes | All 15 pass | Provenance changes separated from semantic payloads |
| Default-feature CLI | Built in 14 min 6 s; normal dependency graph excludes `boa_engine` | Fresh ordinary compiler used for the original Test262 replays |
| Original 16-failure replay | 12 Success, 4 Bug; zero Crash/NotImplemented | Original pinned fixtures, one worker, 60,000 ms deadline |
| Four remaining failures plus ten controls | 10 Success, 4 Bug; zero Crash/NotImplemented | Same fresh ordinary compiler; four failures retained |

The Intl integration receipt precedes the fixture-only engine test correction
and removal of unrelated formatting changes in 12 CLI files. Their formatter
normalized contents were proven identical. The final source reruns the complete
engine and CLI scopes. The JSON keeps these receipts separate; none is presented
as a complete 109-scope workspace run. Cargo and libtest remain serial within
each process; the CLI partition run uses the machine's four-core quota. Timings
from different checks are not a speedup comparison.

## Failures found and resolved during the batch

The runner extraction initially left a workspace-members variable out of scope.
The build succeeded but receipt processing stopped before tests ran. Commit
`30065bbb5` restores that binding and adds an import/global-binding control, which
now catches this class of error before costly compilation.

The first complete native attempt recorded one engine fixture failure and 17
CLI setup failures. Both engine fixture defects predate the batch: the sorted
export expectation retained the old name's ordering, and a concatenation input
still formed the old project name while expecting the new name. The fixture now
uses `hello` and the actual sorted export list; every assertion is retained.

All 17 CLI failures were the same setup error: a four-job compilation pool had
already been initialized before tests explicitly requested one job. No artifact
comparison was reached. The final run uses the existing partition driver with
one CPU/job per child, without modifying test bodies, case-worker counts or
deadlines. Original failed receipts remain failures. Two stale source guards
also needed their predicates aligned with the already-existing global-prelude
repair; negative controls prove that referenced-only globals are rejected.

## Coverage and history accounting

The workspace inventory changes from 113 scopes to 109 solely by retiring the
C and native scaffold library/documentation scopes. Exact removed and surviving
identities are retained in JSON; surviving command plans are identical. Four
obsolete emitter checks and fourteen C/native rows in seven CLI policy matrices
are retired. Their containing tests and meaningful Wasm/oracle assertions remain,
and a new CLI regression rejects unsupported commands before source loading.

The dated [workflow](batch-workflow-history-20261021.md),
[inventory](inventory-history-20261021.md) and
[task history](https://github.com/mewhhaha/porffor/blob/59c2c777bcab11b7a1cd4122f8546048f48f7c7d/tasks/history-through-20261021.md) are byte-identical copies
of baseline `6b3d9b449`. Maintained links, the 100 stable hill IDs, source guards,
and unchanged generated README status/chart blocks are checked.

## Remaining work

The four original Test262 disagreements remain ordinary failures with their
[README explanations](../../README.md) and
[upstream tracking](../../test262/errata/proposals/upstream-status-20261021.md).
Original Test262 fixtures and canonical published totals are unchanged. Targeted
replays do not replace the complete pre-repair baseline.

The [current inventory](current-inventory.md) and
[acceptance criteria](conformance-acceptance.md) retain the broader native,
release, differential, reproducibility, performance and complete Test262 gates.
This batch does not complete those gates or authorize deleting CONTINUE/tasks.
