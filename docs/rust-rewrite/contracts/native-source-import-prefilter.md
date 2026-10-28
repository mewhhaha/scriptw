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

This is a bounded code-entry/page-locality experiment. The latest actual
Portable captures associate the three linker bodies with 3, 3 and 2 warm
host RX page overlaps after zero at transport. These are shared, nonexclusive
pages; they neither establish an eight-page saving nor explain the full
approximately 2.3-MiB difference. The existing linkers return before full
signature validation/capture for absent imports. This change avoids entering
their larger bodies when the immutable Module needs none of them.

Compilation, focused native regressions and paired end-to-end RSS/page/noise
measurement are pending. A smaller body or a source guard pass is not a memory
result. Retire any performance claim if matched measurements show no gain.
