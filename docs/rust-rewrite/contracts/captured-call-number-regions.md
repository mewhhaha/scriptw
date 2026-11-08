# Captured calls in Number regions

The existing scalar-loop owner may include bounded strict source calls. Its
normal Number expressions, predicates, row projection and publication tail
remain the only arithmetic implementation. There is no second heap or evaluator.

`CapturedCallSource` retains a bounded candidate body. Candidates are not facts
about the live callee: entry checks its actual FunctionObject, executable kind,
source ID and defining ProgramInstance. A single forwarding body may select a
bounded packed-array callable; each selected slot receives the same checks.
Source depth is checked for the complete call chain after positive-trip admission.

The complete test, body, update and substituted callees must be callback-free
and nonthrowing after admission. Only private Number operands, actual own-data
Number fields and initialized declarative Number cells enter the region.
Arguments observers, eval, suspension, object writes, unsupported coercions and
larger source trees retain the original loop. Arbitrary source functions are not
assumed pure merely because their result is inferred Number.

All fields in the repeated masked row domain are preflighted before any source
write. Entry failures execute the original loop without replaying a completed
prefix. The existing compact-row owner retains its original continuation route;
the captured-call route requires a complete projection, not streaming reads.

Captured roots are taken from each actual callable's lexical environment. Import
targets and uninitialized cells decline; written cells must be mutable. Entry
compares physical cell identities and assigns alias representatives. Deferred
scalar writes update all members of the alias group. The common exit publishes
each dirty representative once and clears every retained root. Distinct closure
instances with identical code therefore retain distinct state, while closures
sharing a binding observe one scalar history. No callback, throw or suspension
can observe a deferred write between entry and publication.

The unchanged captured-state fixtures must contain a projected loop with no
source calls, allocations or heap writes, alongside a real generic fallback.
Semantic coverage includes shared/distinct cells, call replacement, accessors,
Proxies, holes, non-Number values, retained state, GC, foreign Realms and depth.
