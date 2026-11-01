# Performance queue

Use current profiles to choose one coherent change. This is a short list of
remaining questions, not an implementation mandate or release checklist.
[Acceptance criteria](rust-rewrite/conformance-acceptance.md) own release gates;
[README](../README.md) owns published comparisons. Earlier P identifiers remain
traceable in Git and the source-bound benchmark records.

| Area | Question to answer before changing code |
| --- | --- |
| Array operations | Distinguish retired work from frontend/cache effects using the exact failing products in the [array-storage experiments](rust-rewrite/array-storage-experiments-20261030.json), before changing function placement or publication dispatch again. The [dense packed-join borrow](rust-rewrite/join-dense-values-20261030.json) also failed its gate. Preserve mutation, snapshot and ordered-fallback regressions. |
| Module locals | Uncaptured activation `var` bindings and simple parameters in strict ordinary functions now use private storage; actual captures and nonlocal names keep live lookup. Measure lexical locals, arrows and other function protocols before expanding this boundary. Direct eval/arguments and surrounding `with` retain the conservative plan. Preserve the slower ordinary controls in the [local diagnostic](rust-rewrite/module-local-performance-20261030.json). |
| Dense numeric arrays | A dense-prefix prototype removed 8,192 private bitmaps in retained objects but failed repeated control gates and was reverted. Require a different measured hypothesis before expanding storage modes; [numeric literals](../crates/scriptw-aot-wasm/src/builtins/array/literal/numeric.rs) retain their original bitmap storage. |
| P146 — resident memory | Array/index fusion removes 48 bytes and one allocation per Array; the [local cohorts](rust-rewrite/array-owner-fusion-20261031.json) reduce retained-object RSS by 0.58–0.77 MiB. Which remaining runtime data, native code, stack maps, GC pages and host allocations explain the gap? Continue from [runtime ownership](rust-rewrite/contracts/runtime-byte-ownership.md) and the exact native-page observations in that cohort. |
| P148 — root lifetimes | Can a measured retention cost be reduced after the final semantic use while preserving closures, exceptions, weak reachability, resumptions and host reentry? |
| P149 — immutable ownership | Are any package/data copies still redundant on compile/load paths after the existing bounded-loading changes? Preserve admission, byte lifetimes and source/native identities. |
| P156/P157 — emitted size | The [full](rust-rewrite/runtime-fold-20261031.json) and [string-only](rust-rewrite/runtime-strings-20261031.json) resolver folds reduced native bytes but failed controls and are reverted. Require a distinct measured caller/frame/placement hypothesis before revisiting them; smaller code does not establish lower RSS or faster cached calls. |
| P158 — fresh Realm setup | [Date graph deferral](rust-rewrite/runtime-date-demand-20261031.json) improves normal-cache startup 7–8% at both fresh boundaries. First-Date, retained-Date and large-join costs remain explicit; selected RSS rises 0.15–0.46 MiB. Its 203-record / 7,312-byte dormant method slice is a gross allocation bound. Keep the full tmpfs-fallback cohort separate from the eight-case normal-cache follow-up. |
| P159 — append-key allocation | Both the [eager classifier](rust-rewrite/append-key-summary-20261031.json) and [bounded lazy scan](rust-rewrite/append-key-lazy-summary-20261031.json) improved targets but failed repeated controls and are reverted. Lazy ordinary small joins repeated +8.00% median / +13.25% mean; retained empty joins repeated +4.67% / +6.86%. Require a distinct measured cost hypothesis before repeating either summary. |
| P160 — numeric presence bits | Both [i31 storage](rust-rewrite/numeric-presence-i31-20261031.json) and the [scalar-count alternative](rust-rewrite/numeric-presence-packed-count-20261031.json) removed small bitmap records but failed controls and are reverted. Scalar count retained a nominal field yet grew R by 133,368 bytes and ObjectRead's frame by 112 bytes; RSS rose slightly. Require a distinct measured caller/frame hypothesis before repeating presence encodings. The 256-KiB record bound did not establish a process saving. |
| P161 — fresh sorted results | Both [fresh sorted-buffer transfers](rust-rewrite/fresh-sorted-buffer-20261031.json) are rejected. Inline publication improved targets but slowed retained empty joins in both balanced groups; outlining restored the original caller frame but slowed fresh precompiled execution of the retained-object workload in both groups. Production retains the canonical publisher and ABI 42. Require a different measured mechanism before repeating this hypothesis. |
| P162 — scalar execution regions | The [dense Number region screen](rust-rewrite/numeric-array-region-screen-20261101.json) misses one reverse-128 target block. The separate [joined-length candidate](rust-rewrite/joined-length-zero-trip-20261101.json) improves three joins but slows a zero-trip control by about 30%. Both remain unpromoted. Measure integer-index/four-step regions, bounded exact sums, and an entry-test gate before join preflight; preserve every sequential Number operation and complete fallback. |
| P163 — final String construction | Can the existing safe-integer decimal writer write directly into a fresh concatenation buffer, avoiding the temporary numeric String in `'row-' + i`? Preserve operand/conversion order, `-0`, noninteger/exponent fallbacks and retained/tiny-string controls. |
| P164 — hot runtime frames | Profile the remaining dynamic ObjectRead and FunctionCall entries before separating cold proxy/accessor/error machinery. Measure actual native entry frames, code size and hot instructions; the historical 800-to-912-byte ObjectRead frame observation alone establishes no timing cause. |
| P165 — source-call arguments | Measure a bounded small-arity entry retaining the full source activation protocol while avoiding temporary argument boxes/vectors. Require actual code/Program/Realm/depth and unobservable-arguments proofs; preserve captures, reentry, legacy observers and the ordinary fallback. |
| P166 — virtual literals | Can an unobserved fresh literal remain in Wasm locals until identity or escape requires its canonical object? Begin with private literal/property consumers, reject unknown calls/captures/weak observations, and measure allocations plus materialization/compilation costs before broadening. |
| Integer recurrences | [Whole-trip composition](rust-rewrite/whole-trip-affine-20261031.json) improves original/ordinary integer execution while preserving live guards, common completion and all fallbacks. Derived retained calls and small-join ordinary variation remain visible. Measure new domains before broadening the literal positive i32 count proof; preserve timeout and counter/last-body rules. |
| Numeric property reads | The [pre-`ToObject` relocation](rust-rewrite/numeric-getv-probe-20261031.json) failed to demonstrate repeatable target benefit and is reverted. Require a different measured caller/dispatch hypothesis; preserve key effects, holes, proxies, primitive receivers and complete fallbacks. |
| Empty Array headers | The [combined shared-empty graph/code fold](rust-rewrite/runtime-ownership-20261031.json) was rejected after failing controls and raising total RSS. Require isolated evidence before repeating shared empty Array named storage. Preserve exotic own keys, exact snapshots and independent promotion; ordinary/native/frozen-template allocators retain their paths. |

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
