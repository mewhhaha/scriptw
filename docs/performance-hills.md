# Performance queue

Use current profiles to choose one coherent change. This is a short list of
remaining questions, not an implementation mandate or release checklist.
[Acceptance criteria](rust-rewrite/conformance-acceptance.md) own release gates;
[README](../README.md) owns published comparisons. Earlier P identifiers remain
traceable in Git and the source-bound benchmark records.

| Area | Question to answer before changing code |
| --- | --- |
| Array operations | Re-profile append prototype probes and String allocation/copying. The [dense packed-join borrow](rust-rewrite/join-dense-values-20261030.json) failed its balanced repeatability gate and was reverted; require a different measured hypothesis before repeating it. Preserve the storage-transition and ordered-fallback regressions. |
| Module locals | Uncaptured activation `var` bindings and simple parameters in strict ordinary functions now use private storage; actual captures and nonlocal names keep live lookup. Measure lexical locals, arrows and other function protocols before expanding this boundary. Direct eval/arguments and surrounding `with` retain the conservative plan. Preserve the slower ordinary controls in the [local diagnostic](rust-rewrite/module-local-performance-20261030.json). |
| Dense numeric arrays | Can a dense-prefix storage mode avoid a private presence bitmap and its read checks, creating a bitmap only when holes require it? Numbers are already unboxed; [numeric literals](../crates/scriptw-aot-wasm/src/builtins/array/literal/numeric.rs) still allocate 8,192 small presence arrays in retained objects. |
| P146 — resident memory | Which remaining runtime data, native code, stack maps, GC pages and host allocations account for the measured footprint? Continue from [runtime ownership](rust-rewrite/contracts/runtime-byte-ownership.md) and the [section diagnostic](rust-rewrite/memory-module-sections-20261028.md). |
| P148 — root lifetimes | Can a measured retention cost be reduced after the final semantic use while preserving closures, exceptions, weak reachability, resumptions and host reentry? |
| P149 — immutable ownership | Are any package/data copies still redundant on compile/load paths after the existing bounded-loading changes? Preserve admission, byte lifetimes and source/native identities. |
| P156/P157 — emitted size | Which remaining duplicated or unreachable bodies/data materially affect the footprint? Prove every export, prepared source and reflective reachability root before sharing or pruning. |
| P158 — fresh Realm setup | Which remaining eager allocations are costly enough to defer or share as immutable metadata? Mutable JavaScript state and defining-Realm identities must remain fresh. |

The [initial GC-capacity experiment](rust-rewrite/memory-gc-capacity-20261028.md)
already tests 8 MiB against the ordinary 16-MiB default. It did not establish a
memory or speed gain; retained objects grew back to 16 MiB and slowed down.
Do not repeat P147 without a different measured hypothesis.

For each selected change:

- Record exact source, compiler/runtime settings, workload checksums and raw
  observations. Use balanced controls when effects overlap observed variation.
- Keep startup, small/large workloads, retention and memory controls. Time
  compilation, setup and execution separately; preserve ordinary invocation scores.
- Distinguish post-execution RSS, sampled peak, GC capacity and live JavaScript
  reachability. Shared pages and cross-runner/configuration cohorts are not causal
  savings. Diagnostic observers stay outside scored measurements.
- Preserve semantic order, fresh Stores/Realms, runtime features and artifact
  admission. Verify the complete affected scopes and adjacent original Test262
  cases after the batch; publish broader claims only with broader evidence.
- Record rejected hypotheses and slower controls in existing evidence. Do not
  add another checklist to justify work the profile does not support.

See [Contributing](../CONTRIBUTING.md) for bounded iteration and
[the current queue](rust-rewrite/current-inventory.md) for open acceptance work.
