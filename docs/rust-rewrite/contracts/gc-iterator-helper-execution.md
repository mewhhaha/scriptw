# Native GC iterator helpers

This is source authoring in the atomic GC draft. MAIN is unchanged.
Compilation, Wasm validation, fixture parsing and execution are pending.
All-task source comes first; later checks require the confirmed aggregate
4096 MiB cgroup cap, swap disabled and one worker.

Actual cached Iterator Records retain whole iterator/next values. Initial
callback/limit failures close before reading next; failed step/done/value
observations complete helpers without closing the failed record. Callback
failures close while preserving the original whole Throw. Drop omits skipped
value Gets; take closes at the next resume after its limit is exhausted.

Seven concrete helper layouts carry creation Realm, started/completed/running
state and their actual family data. Shared next/return validates concrete
brands and reentrancy, returns completed results in the called Realm, then
uses real internal callable entries in the captured Realm for active resumes.
The existing invocation owner restores Realm on every completion.
Suspended-start return completes before closing; active return retains
running state while closing. FlatMap closes active inner before outer and
excludes a failed inner step from closing.

Concat caches opening methods without opening iterators until requested.
Private entry array builders have no reference getter; publication consumes
their write capability. Zip acquisition, padding and all three modes use
the same cached whole protocol and explicit reverse close order; keyed
results retain validated property keys. Geometric Argument List buffer
construction owns terminal ToArray publication.

Algorithms follow the [ECMA-262 iterator operations](https://tc39.es/ecma262/multipage/control-abstraction-objects.html#sec-iterator-objects).
Finite paired controls cover observable method/callback ordering, cache
identity, Number counters, whole values, closing and error precedence,
reentrancy, suspended-start/active return, lazy concatenation, inner iteration,
terminal short-circuit and initial-value presence, and borrowed Realms.
Seventeen finite paired controls are authored and unrun. All thirty actual
StandardBuiltinId dispatch arms consume these owners. Whole-task source
readiness remains false while other compiler families are unfinished.
