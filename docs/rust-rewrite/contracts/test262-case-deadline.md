# Test262 whole-case process deadline

`SuiteConfig` can describe discovery without an executable. Actual case
execution validates a private `CaseExecutionDispatch` before scheduling and
journal admission. The default role is a supervisor with a selected executable;
an absent selection cannot authorize parsing or compilation. Only the exact
single-case worker entry constructs the production worker role. It rejects a
directory selector, resume/shard/matrix request, multiple cases or multiple
worker threads. Unit fixtures use a separate `cfg(test)` role.

The CLI always selects its own loaded executable. The hidden
`test262 __case-worker` route transports the exact execution ID, backend,
threads, timeout, snapshot selection, and original harness variant/file.
Library embedding executables must implement that route themselves; a different
executable cannot impersonate the running compiler's snapshot provenance.
Legacy FORCE/DISABLE environment variables do not change this policy.

`CaseDeadline` checks budget and monotonic-clock overflow, then records its
expiry before `Command::spawn`. The Wasm-AOT budget is the existing 300000 ms
compilation allowance plus `timeout_ms`; SpecExec uses `timeout_ms`. This covers
child admission, source materialization, parsing, lowering, emission, native
compilation and execution. The Store's [execution deadline](execution-deadlines.md)
remains an inner execution bound.
Direct `Engine::compile_script`/`compile_module` have no whole-process timeout
contract; embedding hosts supervise such requests themselves.

On supported Unix hosts each case gets its own process group. `CaseProcess`
checks the deadline before polling and again before accepting an exit. Every
success, timeout and polling failure retires the group, kills a remaining direct
child and polls reaping for at most two seconds. Failed retirement is reported
and Drop retries cleanup. A host without group retirement rejects supervised
execution rather than silently compiling in process. Group cleanup prevents
surviving descendants from writing the child snapshot after it is inspected.

Only a normally exited child can contribute its exact current-version snapshot.
Exit 1 must contain an exact failure; a crash, failed exit with a staged pass,
missing snapshot, malformed identity, timeout or cleanup error cannot become
success. Existing compiler-image, suite pin, backend, selected execution and
manifest checks remain the single snapshot authority. Journal and resume
retirement behavior is unchanged.

Publication wrappers reject `ISOLATE_CASES=0` and publication-session decoding
requires a literal `isolate_cases: true`. Schema-4 publication identity separately
records a literal boolean `reuse_case_workers`, defaulting to false. Explicit reuse
keeps mandatory supervision and original deadlines; a policy change cannot resume
the same family. Schema-2 closure evidence records the same selection for both
fresh full families. Existing resource/environment/locale,
source, compiler, suite and checkpoint bindings remain enforced. No timeout is a
skip or a pass, and a measured suite closes only with zero non-successes.

Finite controls retain all ten comparison witnesses, existing timeout,
resume, exact child identity, admission and cleanup controls; the real CLI
control adds default supervision and legacy-env refusal. Native process
controls cover pre-spawn expiry, overflow, direct reaping and descendant death.
Compilation, execution and full conformance acceptance remain pending.
