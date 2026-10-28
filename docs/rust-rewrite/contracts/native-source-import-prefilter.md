# Native source import prefilter experiment

The GC host linker checks the actual immutable Module for the canonical
Function, global Script and direct-eval compiler import names and namespace
before entering their three binding routines. Portable R declares none of
these imports. If any name is present, every existing linker, duplicate/type/GC
ABI validator and compiler callback remains unchanged. Import kind is not part
of the prefilter: a matching name with a wrong kind must reach normal rejection.

This check does not inspect or cache a Store's compiler availability. NativeHost
execution still links before arming its native compiler context. Missing-context
errors, dynamic evaluation support, coercion/Realm/abrupt-completion behavior and
the original late-arming lifecycle are unchanged.

This is a bounded code-entry/page-locality experiment. Pre-change actual
Portable captures associate the three linker bodies with 3, 3 and 2 warm
host RX page overlaps after zero at transport. These are shared, nonexclusive
pages; they neither establish an eight-page saving nor explain the full
approximately 2.3-MiB difference. The existing linkers return before full
signature validation/capture for absent imports. This change avoids entering
their larger bodies when the immutable Module needs none of them.

The [verified checkpoint](../memory-calendar-rows-20261028.md) records the three selected import/ABI/late-arming controls within 1,766 native executions, plus all 17 matched workload checksums. No overall causal RSS saving is established.
