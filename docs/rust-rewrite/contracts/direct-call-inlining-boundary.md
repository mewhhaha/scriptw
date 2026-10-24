# Bounded native inlining and ordinary call costs

General JS calls retain a real callable, receiver and immutable argument List.
The boxed fixed-arity path uses one reusable evaluation value, roots each
argument snapshot before evaluating the next argument, then constructs one
ValueArray. Zero arguments use the canonical empty vector. Spread/captured
Lists retain their ordered iterator and completion protocol. The new encoded
arity controls bound additional StoredValue construction by argument count and
final List construction by one; they do not require future implementations to
keep unnecessary allocations.

Static `FunctionTargetKnowledge` does not grant authority to replace a current
callee. The general source path still reads the real binding/property and receiver,
evaluates arguments, and dispatches through R's ProxyCall/FunctionCall kernels
to the concrete typed entry. The emitted FunctionCall kernel retains
call_ref/return_call_ref. A native inlining option cannot be assumed to remove
that dynamic reference boundary or cross independently compiled R/P modules.

An exact target alone also does not justify cloning its JS return expression.
`FunctionBuilder` owns one source entry, scope stack, completion and
SourceActivation. Calling a JS body additionally owns source-depth admission,
actual function identity, legacy caller/arguments publication and restoration,
defining Realm, parameter/default/mapped state, captured environments and the
normal/abrupt return protocol. The synchronous leaf proof only omits an
unobservable arguments object and mapping-only storage; it keeps those call
activation operations. That proof alone cannot authorize a JS body inliner.

A separate NumericLeafPlan admits only noncapturing ordinary functions with
1–4 unique simple parameters and a single return tree of at most 16 nodes:
parameter reads, Number constants, unary plus/minus and addition, subtraction,
multiplication or division. Exact arity, actual JavaScript code identity,
defining ProgramInstance identity, runtime Number tags and source-depth admission
guard the substitution. Arguments evaluate once into roots before admission;
fallback dispatch consumes those same values. The admitted expression allocates
neither StoredValue argument boxes nor argv and has no calls, coercion effects,
captures or activation/Realm observers. Other calls retain the ordinary ABI.

Pinned Wasmtime47 provides a separate native compiler inlining switch.
`Config::compiler_inlining` and the pinned tunable defaults select `No` unless
explicitly configured. Cranelift Speed alone does not enable it. The opt-in
`WasmProfileInlining::Bounded` cohort selects `Inlining::Yes`, while the existing
three-argument profile constructor and ordinary execution retain `Disabled`.
The profile observes the actual Engine mode; unsupported inlining modes reject
diagnostic admission.

The bounds record is explicitly a pinned source fact, not observed Engine
threshold data. Wasmtime47's default small-callee IR size is50 and its maximum
caller-plus-callee IR size is2000. The supported Config API exposes no threshold
overrides or getters. Its inliner also excludes recursive SCC edges and ABI
trampolines. These bounds limit native graph expansion, not total module
compilation memory or emitted machine-code bytes.

Serialized native modules require exactly matching inlining mode and threshold
tunables. R and P share one actual execution Engine, so a P-only executing
inlining cohort with unchanged No-inlining R is invalid. The explicit cohort
therefore uses the existing matching optimized-R producer, setting the observed
destination mode under the same pinned bound defaults. Its one private image
cache keys runtime source/profile/layout, GC capacity, inlining mode and pinned
bound tuple, retaining at most two images/256MiB. Existing Engine identity
checks prevent cross-configuration Module reuse. No external native bytes or
second runtime representation enter this path.

Native inlining operates on the already emitted Wasm activation operations and
does not broaden JS arguments-elision authority. Tiny native compatibility and
actual-mode controls plus synthetic matched-image cache controls run in focused
verification. A full R inlining compile is a separate supervised experiment:
Wasmtime keeps module intermediate functions and a call graph during inlining,
so its peak may exceed the existing4GiB limit. Automatic broad tests must not
compile full R with this experimental option. A cgroup rejection remains a
recorded failure and does not increase the budget.

The parent-owned arity/break-even workload compares compile latency, native
size and correctly scoped execution under a separately labelled whole-Engine
cohort. No native or JS inlining speedup is assumed. General source-body inlining
beyond the bounded numeric plan stays gated on measured residual call cost and
a separately designed consumed activation contract; target knowledge alone is
insufficient. See the [efficiency checkpoint](../compiler-efficiency-20261024.md)
for the numeric plan's verification and limits.
