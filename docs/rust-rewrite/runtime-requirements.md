# Wasm runtime requirements

Scriptw's tested execution path is its Rust CLI/library with the project's
vendored **Wasmtime 47.0.0**. The host policy is defined in
[wasmtime_policy.rs](../../crates/scriptw-engine/src/wasmtime_policy.rs).
The same proposal and collector policy applies to both native compilation modes.

## Required feature set

| Wasm capability | Required by the backend |
| --- | --- |
| GC structs and arrays | JavaScript objects and the semantic heap |
| Reference types and typed function references | Values, closures and typed calls across modules |
| Exception handling | JavaScript abrupt completion and exception flow |
| Tail calls | Enabled as part of the required backend target |
| Multi-memory | Program/runtime memory arrangements |
| Threads and shared memory | Required proposal support and host shared-memory allocation |

The native adapter explicitly selects Wasmtime's copying collector and verifies
GC, reference types, typed function references, exceptions, threads and shared
memory before admitting its engine. Multi-memory and tail calls are enabled
in the same policy. This does not claim that Scriptw supplies browser Workers
or Node's worker APIs. SIMD is not an additional requirement of this policy.

## Host compatibility

An emitted program is linked to Scriptw's runtime module and host imports,
including the `scriptw_host` namespace. Matching runtime bytes, Intl data and
host adapters are part of the execution environment.

- **Scriptw CLI / Rust API:** the supported integration with vendored Wasmtime
  47.0.0, including the copying collector and project host bindings.
- **Browsers, Node's Wasm API and other engines:** no compatible Scriptw adapter
  or end-to-end portability result is established. An engine implementing the
  same Wasm proposals still needs the host ABI, data and collector capabilities.
- **Bare WASI / a standalone Wasmtime CLI:** WASI does not provide Scriptw's
  JavaScript runtime or its custom host imports. The tested path is Scriptw's
  embedding, not loading the emitted file as an arbitrary WASI command.

These are requirements of the current backend, not a promise of support for
every engine or a fallback implementation for engines missing Wasm GC.

## Dynamic source: eval and Function constructors

Dynamic code generation is **limited**, rather than wholly absent:

- The compiler can prepare finite known `eval` and Function-constructor sources
  ahead of time, with dispatch that checks the source and execution context.
- The native host can parse, lower and compile additional supported runtime
  strings into Wasm. See the actual
  [direct-eval compiler](../../crates/scriptw-engine/src/wasm_native_direct_eval_compiler.rs)
  and [Function compiler](../../crates/scriptw-engine/src/wasm_native_function_compiler.rs).
- A portable Wasm artifact does not bundle a parser, interpreter or JavaScript
  VM. Runtime strings outside its prepared sources require a suitable native
  compiler host; unsupported syntax, lowering or contexts remain capability gaps.

This does not establish full ECMAScript `eval`, `new Function` or cross-realm
dynamic-source compatibility. See the [current inventory](current-inventory.md)
for verification and remaining acceptance work.

## Out-of-scope tests

Standalone Wasm dynamic-source cases that would require bundling a JavaScript
parser, interpreter or VM into the emitted artifact are **outside product
scope**, rather than features planned for implementation. This includes such
uses of `eval`, `new Function` and cross-realm Function constructors.

The chart now classifies a conservative, audited subset by each test's primary
specification target: the two exact PerformEval ESIDs and eight individually
reviewed Function-constructor source-parser cases. This is **221 source files /
429 mode-specific executions** at the published suite pin: 413 eval and 16 Function
executions. It excludes the general dynamic-source promise of standalone Wasm,
even where finite prepared fixtures or the native compiler host can pass a test.
It does not remove supported prepared-source/native-host code or declare it broken.

The four direct/indirect non-string eval pass-through tests remain in scope: they
return their input without compiling source. Ordinary API-property tests, Function call/apply/bind, constructors used only to
set up another behavior, and incidental source mentions remain in the ordinary
measured categories. This is a known audited subset, not an exhaustive inventory
of every dynamic-source use. Parser/lowering failures in supported paths remain
failures; an Unsupported failure kind cannot decide scope. No excluded execution
is an existing failure or unimplemented result. The four remaining original
failures stay red.

[The original scope audit](test262-chart-reconciliation-20261021.json) binds every
selected source hash, execution ID, reason and ESID to the verified vendored tree.
The [current complete publication](test262-current.md) retains that scope inventory
and all original outcomes. Blue describes product scope; it does not claim those
native-host executions failed or remove them from the complete result.

## Refreshing the README chart

After a complete canonical publication,
`python3 -B scripts/generate-test-graph.py --refresh-evidence --full-evidence`
refreshes the README caption and [chart](assets/test262-status.svg) from its
portable native snapshots. `--check` verifies the evidence and generated outputs
without writing. The generator checks compiler/pins, exact disjoint execution IDs,
outcomes and the unchanged scope inventory. A scope decision cannot hide a
non-success; reviewed errata snapshots are never inputs.

Green is in-scope Success, red is Bug plus Crash, gray is NotImplemented, and blue
is the audited standalone dynamic-source scope subset. The graph uses the
[complete 2026-10-23 run](test262-current.md), with every execution bound to its
recorded compiler. Earlier baseline/replay reconciliation remains historical
evidence. A new compiler requires a new complete publication before its counts
can replace this result. Unrun tests do not become unimplemented tests, and no
scope decision silently skips execution.
