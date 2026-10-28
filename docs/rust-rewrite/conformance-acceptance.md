# Conformance acceptance

These are durable requirements for the compiler and conformance areas. They
state acceptance conditions, not a claim that any current source or run meets
them. The [ownership registry](conformance-ownership.md) identifies each area.
Detailed compiler/runtime invariants live beside their source contracts.

Wasm is the sole product compilation target. C output, native JavaScript
output and a REPL are outside project scope and are not acceptance criteria
or release blockers. Building the Rust compiler and Wasmtime compiling Wasm
to machine code remain part of the Wasm execution workflow.

Required final verification includes default and all-feature workspace tests,
ignored stress controls, documentation tests, debug and release differential
corpora/campaigns, both complete fake selections, product interpreter exclusion,
and the current-pin release gate's two fresh full Wasm-AOT families. Run these
under the applicable machine resource policy with their original deadlines.
Every receipt must bind the tested source, exact inventory and actual exit.
Supported-host reproducibility, real sharding/resume, and publication remain
separate evidence obligations. Publication uses the verified publisher and may
never turn incomplete or historical evidence into current pass counts.

<a id="t00"></a>

## T00 — Contribution protocol

- An unknown or inconsistent owner and a broken contract or acceptance link
  both fail the ownership validator with actionable messages.
- A normal checkout passes the validator.
- CI invokes the validator.
- The PR template clearly distinguishes fake fixture counts from pinned real Test262 counts.
- The workflow states that `Unsupported`, timeout, crash, and bug are all non-passing outcomes.
- An isolated guard regression proves that only the complete canonical
  Wasm-AOT JSON/text publication pair authorizes a generated README status
  change, apart from the exact historical T27/T29 identity/policy migration;
  node, aggregate, fake, oracle, single-file, partial/wrong identity and
  `wasm`-alias changes do not.
- No runtime/compiler behavior changes.

<a id="t01"></a>

## T01 — Baseline and failure evidence

- The `wasm-aot` backend has a complete verified snapshot for the current pin, or the PR documents a concrete infrastructure blocker with a reproducible failing node while still landing the deterministic backlog tooling. Any `spec-exec` oracle snapshot is stored and labeled separately from product data.
- Running the generator twice over the same snapshot produces byte-identical backlog output.
- Every failure is assigned to exactly one permanent owner code or the explicit
  unclassified closure bucket.
- The comparison command catches an intentionally injected regression and pin mismatch.
- README status is updated only through the normal publisher after the complete
  matrix is verified, with both canonical Wasm-AOT status artifacts committed
  beside the generated block.

<a id="t02"></a>

## T02 — IR and Wasm backend architecture

- Both giant `lib.rs` files become orchestration/re-export surfaces rather than implementation stores.
- Feature families have clear files that separate agents can own.
- There are no cyclic module dependencies or duplicate constant registries.
- Public APIs used by `scriptw-engine` remain coherent and documented.
- Representative emitted artifacts behave identically before and after extraction. If byte identity is not practical, compare imports, exports, validation, output, completion kind and thrown error class.
- Workspace compile time and binary size do not regress materially solely because of module movement.

<a id="t03"></a>

## T03 — Test262 harness integrity and host contract

- The harness contains no source regexes that emulate agent programs or expected assertions.
- Every `$262` method has a documented backend contract and direct tests.
- Realms are distinct or the operation fails explicitly; no same-global fallback.
- Buffer detachment and GC hooks either perform the requested operation or fail visibly.
- The runner correctly handles async pass, async rejection, timeout and duplicate `$DONE`.
- The generated shortcut inventory has an owner and removal task for every remaining item.
- Running an intentionally unsupported host case cannot be counted as success.

<a id="t04"></a>

## T04 — Shared ECMAScript operations and completion ABI

- There is one authoritative implementation for each listed operation or an explicit tracked gap.
- Side-effect/evaluation-order tests cover success and abrupt paths for every conversion family.
- Nested calls and builtins can propagate arbitrary thrown JavaScript values, not only error-name strings.
- `try/catch/finally`, proxy traps and cross-realm errors consume the same completion ABI.
- Representative Array, String, TypedArray, Date and Proxy tests use the shared operations rather than local coercion code.
- No operation silently maps unsupported object input to a primitive default.

<a id="t05"></a>

## T05 — Value representation, heap, GC and weak reachability

- A written layout/GC design is checked in beside the implementation.
- Layout collisions and invalid pointer-field declarations fail tests.
- Allocation works across multiple `memory.grow` operations.
- Stress programs can allocate and release cyclic object graphs without exhausting memory at a fixed low threshold.
- Rooting tests cover closures, bound functions, pending promises, generators, exceptions and cross-realm objects.
- Weak collection/finalization primitives can be implemented without semantic hacks.
- Existing Wasm fixtures and representative real Test262 filters do not regress.

<a id="t06"></a>

## T06 — Realms, intrinsics and cross-realm semantics

- Two realms have distinct global objects and intrinsic identities.
- Cross-realm constructor/prototype fallback and thrown-error prototype tests pass without exact-test materialization.
- Builtin descriptors are generated from one registry and verified by unit tests.
- A function always retains the correct defining realm after binding, storage, proxy wrapping or cross-realm transfer.
- Realm destruction releases host resources only after JavaScript reachability allows it.
- No fallback returns the current realm when realm creation is unavailable; failures are explicit.

<a id="t07"></a>

## T07 — Parser boundary, grammar coverage and early errors

- Compilation parses each unit once.
- Parser panic cases become deterministic compiler failures and have minimized regression tests.
- All negative parse/early cases are classified at the required phase.
- Script/module goal differences are covered.
- Upgrading Boa does not require feature modules to depend directly on Boa AST internals.
- The parser/early-error buckets from T01 reach zero for the pinned suite, excluding only explicitly documented upstream parser defects with a vendored fix task.

<a id="t08"></a>

## T08 — Environments, references, control flow and abrupt completion

- Environment and Reference types are explicit in IR and do not depend on variable-name string conventions.
- Nested `try/finally` and labelled-loop tests pass without manual Wasm depth values.
- Closure mutation, per-iteration capture and TDZ tests pass.
- Destructuring abrupt-completion/evaluation-order cases pass across declarations, assignments and parameters.
- `with`/unscopables and global declaration instantiation are either implemented or left as explicit, owned failures—not approximated.
- Related `language/statements`, `language/expressions/assignment`, scope and global tests reach zero failures.

<a id="t09"></a>

## T09 — Functions, constructors, classes and private elements

- All function kinds share one coherent metadata/call protocol.
- Arbitrary thrown values propagate through calls/constructors.
- Bound, proxy-wrapped and cross-realm constructors preserve new-target behavior.
- Parameter/default/rest/arguments aliasing and evaluation-order tests pass.
- Class fields/private elements/static blocks pass brand, ordering, inheritance and abrupt-completion tests.
- Function `name`, `length`, prototype-property presence and descriptors match Test262.
- No function family is implemented by source-text pattern matching.

<a id="t10"></a>

## T10 — Object model, descriptors and exotic-object protocol

- All property operations route through the explicit internal-method API or a proven guarded fast path.
- Descriptor conversion and redefinition order tests pass with side-effecting/proxy descriptors.
- Own-key ordering is correct for numeric strings, ordinary strings and symbols.
- Object integrity methods handle primitives, proxies and exotics correctly.
- Prototype mutation/cycle and receiver-aware setter cases pass.
- Feature modules can add an exotic implementation without editing a giant central match.
- Object and descriptor Test262 subtrees reach zero failures before this task is closed.

<a id="t11"></a>

## T11 — Proxy and Reflect meta-object protocol

- The full pinned `built-ins/Proxy` and `built-ins/Reflect` trees pass.
- Every trap has unit tests for absent trap, successful trap, thrown trap and invariant violation.
- Revoked proxies fail consistently for every operation.
- Proxy-wrapped functions/classes preserve call/construct/new-target behavior.
- Proxy operations work against arrays, typed arrays and non-extensible targets.
- No property/call fast path skips a possible proxy trap without a proven non-proxy guard.
- Nested proxy and cross-realm handler tests pass without materialization.

<a id="t12"></a>

## T12 — Modules, linking, loading and namespace objects

- Static import/export, re-export, namespace import and side-effect-only import cases pass.
- Cycles, live bindings, star ambiguity, TDZ and evaluation order pass.
- Module namespace descriptor/internal-method tests pass.
- Top-level `this`, strictness, `import.meta` and host resolution behavior are correct.
- Parse/link/evaluate failures are classified at the right phase.
- Dynamic import is integrated with promises/jobs rather than synchronous source evaluation.
- The pinned `language/module-code` and related module builtins reach zero failures.

<a id="t13"></a>

## T13 — Dynamic source evaluation: `eval`, `Function` and realm evaluation

- The repository has one documented policy; no ambiguous fallback.
- Proven no-source `%eval%` retains runtime callee identity and evaluates every
  argument exactly once in source order. Textual invocations select a matching
  prepared unit or report their typed source/context gap.
- Finite Function candidates preserve live argument conversion, separate
  parameter/body grammars, fresh function identity and constructor realms;
  exceeding discovery bounds or missing the runtime tuple cannot create a pass.
- Any supported static direct-eval cases preserve lexical scope and abrupt completions.
- Any supported indirect/cross-realm evaluation never aliases the wrong global.
- Unsupported dynamic cases are classified consistently and remain in real-suite accounting.
- No source regex/materialization exists for known Test262 eval/Function cases.
- If a later architecture decision selects a host service, representative
  dynamic strings—not known at AOT time—pass scope, realm, constructor and
  error tests.
- The README/CLI clearly report artifact capability requirements.

<a id="t14"></a>

## T14 — Promise jobs, async functions and async iteration

- Promise state and resolution tests pass, including hostile thenables and side-effect ordering.
- Combinators pass iterator-close, species and subclassing tests.
- Async functions preserve environments and finally semantics across multiple awaits.
- Async Test262 cases pass/fail based on `$DONE` or returned async completion, with duplicate completion detected.
- Cross-realm promises and errors use correct intrinsics.
- No busy-loop polling is required for ordinary promise progression.
- The pinned Promise/async-function/async-iteration filters reach zero failures.

<a id="t15"></a>

## T15 — Generators, iterators, iterator helpers and resource management

- Generator protocol/state/re-entrancy tests pass.
- `yield*` handles sync and async delegates, return/throw absence and close precedence.
- All iterator consumers close iterators on the exact required abrupt paths.
- Iterator helpers are lazy and pass mutation/close/branding tests.
- Explicit resource management preserves suppression order and disposal timing.
- Generator/iterator objects remain valid across GC cycles.
- Pinned generator, iterator-helper and resource-management filters reach zero failures.

<a id="t16"></a>

## T16 — Array exotic semantics and complete Array API

- `built-ins/Array` and `built-ins/Array/prototype` are fully green for the pin.
- No path-specific materializer remains for covered Array tests.
- Array index/length descriptor edge cases pass.
- Sort is stable and obeys comparator/coercion/holes/undefined semantics.
- Sparse arrays do not cause loops proportional to impossible maximum lengths when the spec permits key-based optimization; observable access order remains correct.
- Species, proxy, inherited-index and cross-realm tests pass across creator methods.
- Adjacent TypedArray generic-borrow tests do not regress.

<a id="t17"></a>

## T17 — ArrayBuffer, DataView, TypedArray, SharedArrayBuffer and Atomics

- Complete pinned trees for ArrayBuffer, SharedArrayBuffer, DataView, TypedArray constructors/prototypes and Atomics are green.
- Integer-indexed exotic descriptor/key/proxy cases pass.
- Resizable/growable buffer tests pass before, during and after coercion/callback mutation.
- BigInt and Number typed arrays reject mixed values correctly.
- Real multi-agent wait/notify/report tests pass without source pattern matching.
- Detached/out-of-bounds checks occur at spec-required times.
- No data races or host panics under repeated agent stress tests.

<a id="t18"></a>

## T18 — Strings, Unicode and the complete String API

- The complete pinned `built-ins/String` and String iterator trees are green.
- All visible lengths/indexes are correct for BMP, astral characters, combining sequences and lone surrogates.
- No String Test262 path/source-specific materialization remains for covered semantics.
- String wrapper descriptors and own-key ordering pass through the general exotic-object protocol.
- Case conversion and normalization pass exhaustive data-driven tests for the pinned Unicode version.
- RegExp-symbol delegation works with custom objects and proxies.
- Long-string benchmarks avoid accidental quadratic behavior.

<a id="t19"></a>

## T19 — Complete ECMAScript RegExp semantics

- Full pinned `built-ins/RegExp`, RegExp literal and String regex-method filters are green.
- Every flag and syntax feature has positive, negative and interaction tests.
- Matching uses ECMAScript UTF-16/Unicode semantics, including lone surrogates.
- Custom `exec`, species, subclassing, proxies and `lastIndex` property effects are observable in correct order.
- No exact pattern/test-path materializations remain.
- Adversarial patterns cannot panic the Rust host or corrupt Wasm memory.
- Timeout counts for RegExp subtrees are zero at the normal publication timeout.

<a id="t20"></a>

## T20 — Number, BigInt, Math and JSON

- Full pinned Number, BigInt, Math and JSON Test262 trees are green.
- BigInt supports values far beyond 64 bits in operators, formatting and typed-data integrations.
- Numeric string conversion and formatting pass exhaustive boundary/vector tests.
- Signed zero and NaN behavior is preserved through operators, Math, JSON and typed arrays where observable.
- JSON reviver/replacer/proxy/evaluation-order tests pass without static materialization.
- Platform-dependent Math functions have documented tolerances matching Test262 and produce no target-specific regressions.
- No numeric operation silently saturates/truncates outside the specification.

<a id="t21"></a>

## T21 — Symbols, collections, weak collections and weak references

- Full pinned Symbol, Map, Set, WeakMap, WeakSet, WeakRef and FinalizationRegistry trees are green.
- Symbol key ordering and coercion behavior pass across Object/Reflect/Proxy APIs.
- Map/Set iteration passes all mutation and reinsertion cases.
- Weak collections do not keep otherwise unreachable keys alive in GC stress tests.
- WeakRef keep-alive-within-job and finalization scheduling tests pass without deterministic shortcuts.
- Cross-realm well-known/registry/user-symbol identities are correct.
- No weak API is implemented by a permanent strong-reference table.

<a id="t22"></a>

## T22 — Date and Temporal

- Full pinned `built-ins/Date`, `built-ins/Temporal` and T22-owned `intl402/Temporal` consumer trees are green through Wasm-AOT.
- Runs are deterministic under an injected clock/time zone and reproducible across supported hosts.
- Date parsing, setters and formatting pass DST/extreme-range/coercion-order tests.
- Temporal option/property access order works with proxies/getters and abrupt completions.
- All Temporal classes enforce branding, descriptors, subclassing and cross-realm error behavior.
- Time-zone data version is pinned and surfaced in developer diagnostics; every named consumer retains zone identity through exact projection and inverse selection.
- Every conformance failure, including unresolved contextual rounding windows, retains an explicit owner and reason; a semantic gap never satisfies a negative JavaScript test.
- No exact Test262 date/time materialization remains.

<a id="t23"></a>

## T23 — Complete ECMA-402 Intl implementation

- The complete pinned `intl402` tree is green under Wasm-AOT.
- All locale-sensitive methods in core builtins pass their adjacent Test262 filters.
- Results are reproducible across supported hosts with the same pinned data.
- Locale/options access-order tests pass with proxies, accessors and thrown values.
- No service silently falls back to host locale formatting or an English-only handcrafted formatter.
- Constructors, bound functions, branding, descriptors and cross-realm tests pass.
- Data version and build profile are visible in diagnostic/artifact metadata.

<a id="t24"></a>

## T24 — Globals, native errors, Annex B and remaining host-visible builtins

- Full pinned native-error, global-function/global-value and Annex B trees are green.
- Error creation and throwing preserve exact realm/prototype/property order.
- URI functions pass malformed-surrogate, percent-escape and reserved-character cases.
- Annex B behavior is correctly gated by parse goal/strictness/context.
- IsHTMLDDA is implemented through one host-exotic contract, not operator-specific test hacks.
- Test262 shell globals are absent from ordinary product realms unless explicitly enabled.
- No focused static error/global/Annex B materialization remains for covered semantics.

<a id="t25"></a>

## T25 — Differential testing, fuzzing and performance discipline

- Differential runs produce stable machine-readable mismatch signatures.
- The reducer turns seeded complex mismatches into meaningfully smaller reproductions.
- The regression corpus is deterministic and green on both debug and optimized builds where applicable.
- Parser/lowering/emitter fuzz targets run for a sustained campaign with zero unhandled panics or invalid-memory behavior.
- Performance dashboards identify compile/runtime/size regressions by subsystem and pin.
- Every enabled optimization has a test that forces both fast and fallback paths.
- Full-suite timeout counts trend as explicit performance debt and reach zero before T26 closes.

<a id="t26"></a>

## T26 — Conformance release gate

- The current pinned Wasm-AOT aggregate is complete and reports `passed == total`, `failed == 0`.
- Counts for every failure kind and non-success outcome are zero.
- Timeout, crash, unknown-origin, missing-case and duplicate-case counts are zero.
- Independent rerun/sharding/resume checks produce the same complete result set.
- Original failures and failed receipts remain visible; diagnostic retries, local
  fixture amendments and upstream proposals do not replace original-case results.
- Closure defaults to one case worker; explicit parallel scheduling stays within visible affinity/CPU quotas, keeps compiler jobs serial, and verifies the exact worker/timeout/reuse settings against each fresh node's snapshot-bound evidence.
- The integrity audit finds no test-specific semantic branch, fake standard builtin or silent host fallback.
- The T27 interpreter-quarantine audit is green: Wasm-AOT is the default backend everywhere, no interpreter engine crate is linked into product builds, and no emitted artifact embeds an interpreter or user source consumed by an evaluator.
- All workspace tests, fake suites, differential corpus and required stress checks are green.
- Published README/status artifacts are generated, current, internally consistent and tied to exact revisions.
- Any architecture limitation still present is stated separately and prevents a literal 100% claim until it is eliminated.

<a id="t27"></a>

## T27 — Interpreter quarantine and Wasm-AOT product default

- The default engine and CLI execution backend is Wasm-AOT everywhere; spec-exec requires an explicit internal flag.
- No code path silently falls back from Wasm-AOT to spec-exec.
- Product/release builds of the library and CLI link no interpreter engine crate, verified by an automated check that fails CI on reintroduction.
- The `build wasm` artifact audit test exists and passes for representative programs.
- Product status artifacts and the README conformance block contain only Wasm-AOT numbers; any oracle numbers present are clearly labeled non-product diagnostics.
- T25 differential runs can still invoke spec-exec through the developer-only configuration.

<a id="t28"></a>

## T28 — Legacy retirement boundary

- Every listed legacy surface is absent from the tracked working tree.
- Test262 local overlays are embedded Rust-crate assets with an explicit custom
  file option and no checkout-layout dependency.
- The Node discovery oracle and its CLI/API surfaces are absent.
- CI has no JavaScript toolchain or package-publication path and validates the
  Rust product on pull requests and `main`.
- A repository check rejects reintroduction of the retired product surfaces
  without flagging Test262, fixtures, reproducers, or vendored JavaScript.
- README, contribution guidance, task status, Cargo metadata, and the website
  consistently describe Scriptw as a Rust-only product.
- The generated README Test262 status block is byte-for-byte unchanged.

<a id="t29"></a>

## T29 — Canonical product identity

- A checked-in mapping accounts for every discovered identifier and persisted
  namespace before implementation begins.
- Workspace crates, Rust imports, CLI identity, environment variables, caches,
  diagnostics, host interfaces, status artifacts, docs, and CI use the selected
  canonical names consistently.
- Any compatibility alias has an owner, reason, warning behavior, and removal
  deadline; otherwise the migration is a clean break.
- Cache migration cannot delete unrelated data and handles existing entries
  deterministically.
- Product, fake-suite, focused real-Test262, and repository-contract checks pass
  after the rename.
- Repository-wide searches find no unexplained transitional identifier.

