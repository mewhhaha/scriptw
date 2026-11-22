# Performance deep dive against V8 (2026-11-22)

Six parallel investigations of where Speed-policy Wasm loses to V8, on main `6851e2848`
(RSC numbers from the equivalent `66d06f871` build). Investigation only: nothing below is
implemented, and every gain is an estimate from Callgrind instruction attribution (Ir) unless
it says measured. Ir is not time: host Rust code ran at about 7 Ir/ns here, JIT loops at
15-23 Ir/ns, and Callgrind counts each `rep movsb/stosb` byte as one Ir. V8 numbers are the
published 2026-11-17 panel; Node is not installed on the measuring machine.

## Corrections to published numbers

- React SSR is about 3.3x V8, not 11.6x: a fresh entry takes 2.08 ms against 0.632 ms
  (re-prepared with the panel's producer policy; Callgrind window 16.3 M Ir, against 66.7 M on
  2026-11-19, after static `arguments` resolution, RegExp runtime-compile removal and pooled heaps).
- `args-observed` is about 5x, not 29x: 13.4 us, the same fused loop as `args-unused`.
- Speed and SizeOptimized emit byte-identical native code (Cranelift reads `opt_level` only as
  None versus not None), so their measured differences are noise or allocator choice.
- These need a real republish (`scriptw test262` is unaffected); no README numbers were edited.

## Where the time goes

### Five cross-cutting causes

1. **Re-proving invariants at every use.** V8 caches facts behind shape checks, validity cells and
   protectors; Scriptw re-derives them at each site:
   - the `Object.prototype` absence tail in each property read hit (about 45 Ir);
   - each `instanceof` site's constructor, edge and `@@hasInstance` proof (about 112 Ir per site);
   - the packed `push` prototype walk (about 200 Ir per push);
   - the generic `length` Get in about 20 Array builtins (526 Ir per call);
   - `typeof`'s callable/IsHTMLDDA casts on primitives;
   - TDZ tests on `var` slots;
   - environment parent walks on every captured read;
   - `for-in`'s per-key enumerability check.
2. **The ABI and representation tax.**
   - Values travel as three lanes (tag, i64 payload, eqref) and completions as five.
   - JS bodies take about 15 parameters, so 11 words go on the stack.
   - Five callee-saved registers, with no shrink-wrapping.

   Prologue/epilogue plus stack arguments are about 8% of RSC JIT instructions. A guarded JS-to-JS
   call costs 170-210 Ir, against 20-25 for a minimal call.
3. **Backend code quality.**
   - Stack-slot traffic is 23% of RSC JIT Ir: spills 16%, stack-map slots 6%, vmctx homes 3%.
   - 32-bit zero-extensions sit on every GC pointer step (128K Ir of 32-bit moves).
   - Explicit null checks cost 87.7K Ir.
   - The hottest JS body is 27% spills. Most of them reload values that never change: a spilled
     zero, vmctx, the GC heap base.
4. **Numbers.** Int32 operators (`&`, `|`, `>>`, switch indices, array indices) run in binary64,
   with a ToInt32 and a convert-back around each one: 15-18 Ir per operator, plus a 30-cycle
   loop-carried chain in `frame-compact`.
5. **Fixed costs outside the JS body.**
   - The embedding call is 19.5K Ir for an empty `run(){}`, 94% of it host glue. This is
     `control-empty`'s 13x.
   - Fresh-entry Realm bootstrap is about 150K Ir.
   - SSR module init includes one minor GC (26% of the entry), program literal materialisation (10%)
     and a linear RegExp candidate-table scan (8.5%).

### Situations (measured)

| situation | gap | cause |
|---|---|---|
| control-empty | 13x | host embedding glue: `context()` field reads 7.1K Ir, untyped `Func::call` checks 4.8K, marshalling and host allocation 4.6K |
| global-bound-deep | 15x | 910 Ir per iteration: 20 captured reads at about 17 Ir each, plus 2 per parent hop, TDZ on `var`; the closed-loop optimizer refuses captured bindings |
| singleton/typeof-dispatch | 5-7x | the `values[(i+seed)&7]` fetch about 250 Ir (captured reads, f64 index round trip, boxed elements); typeof about 85 Ir |
| frame-branches32..512 | 4-9x | 1,331 Ir per entry: int32-in-f64, 2 calls at about 60 Ir fixed each, `steps[k]()` through `dynamic_property_read` (153 Ir); branches512 adds 14.5 I-cache misses per entry (1.8 KB native per `case`) |
| frame-compact | 3x | latency-bound f64/ToInt32 chain; stack-map stores for calls that never run |
| metadata-ordinary | 2x | f64 accumulators round-trip through stack slots in the closed loop (store-to-load forwarding) |
| args-unused/observed | 3-5x | 2.8 us host boundary, f64 loop counter, ToInt32 expansions |

### RSC (2.73 M Ir per sync request; 1.6x V8)

- **Property helpers: about 640K Ir (23%).**
  - `cached_property_read`: 188 Ir per call over 1,171 calls. Its hits are mostly absent keys
    with an `Object.prototype` tail, `Function.prototype.call`, prototype methods and
    String.prototype.
  - `computed_property_write`: 353 Ir per call.
  - `object_read`: 304 Ir per call.
  - Element boxing in `array_indexed_publish_default_data`.
  - `for_in_advance`.
- **Spills:** about 418K.
- **`instanceof` chain** in the serializer: about 70K.
- **`JSON.stringify`:** about 180K inclusive, with 537 Ir of fixed setup per recursive call.
- **WeakMap:** about 104K (host index plus crossings at about 130-157 Ir glue each, 200 crossings
  per request).
- **Minor GC:** 68K per request on the mean.

### Edge and runtime kernels

- **Edge kernels** still lose 1.24-1.44x with bootstrap removed. Main costs:
  - `JSON.stringify`: 7-32% of fresh Ir.
  - `String.prototype.split`: up to 21%.
  - Generic `length` Gets in Array builtins: 2-9%.
  - Array iterator results built by appends: 2-9%.
  - `JSON.parse`: up to 14%.
- **Small runtime kernels** lose mostly on fixed entry cost.
- **Packed `push`** re-walks prototypes on every append: 32% of `join-1024`.
- **Retained modules** lose the intrinsic proof for `Math.imul`: `integer.js` costs 0.42 ms per
  retained call, against about 0.005 ms as a script.

### Sloppy functions (not in RSC or SSR, whose hot code is strict)

- **The cost:** about 3,045 extra Ir per call, about 13x a strict call. That is a mapped
  ArgumentsObject built by three property appends, plus four own-property lookups for the legacy
  `caller`/`arguments` publish and restore.
- **Why it is forced:** the implementation-selected extension
  ([contract](contracts/active-legacy-function-reflection.md)). Pinned
  `staging/sm/extensions/function-properties.js` keeps the extension itself.

## Plan, ranked by gain per effort

Each item lists its layer, its exactness argument, and the Test262 areas at risk. Waves can run as
parallel batches with disjoint file ownership. Each wave ends with the usual verification (engine
and CLI suites, focused Test262 with `--reuse-case-workers`, RSC counts with exact output) and a
review agent.

### Wave 1: small, independent, high confidence

1. **Zero-extension after `i32` loads (vendor cranelift-codegen 0.134.0).**
   - Change: add `(rule (value32_zeros_upper32 (load _ _ _ _)) true)` in `isa/x64/inst.isle`. A plain
     i32 load already lowers to a zero-extending `mov r32,m32`; the existing rules cover
     iadd/band/... and an unreachable `uload32`.
   - Gain: 52-72K Ir (about 2-2.6% of RSC) plus a cycle on every pointer step.
   - Exactness: pure codegen.
2. **Typed, context-free embedding call (engine).**
   - Change: resolve Realm, program and global environment once at initialize; build a `TypedFunc`;
     use shared-empty argument arrays; skip the clock import with no timer pending; cache the
     memory export in `print`.
   - Gain: about 19.5K to 3K Ir per exported call. `control-empty` goes from about 13x to about 3x,
     and every situation drops about 2.3 us.
3. **Array `length` fast path in Array builtins and `%ArrayIteratorPrototype%.next`.**
   - Change: `ref.test` ArrayObject, then read LENGTH (`builtins/array.rs:792`).
   - Exactness: an Array's `length` is an own non-configurable data property; a Proxy keeps the trap
     path.
   - Test262 at risk: built-ins/Array/prototype/*/length-*.
4. **No TDZ test on `var`-like environment slots; `typeof` classified from the tag first and once
   per if-else chain.**
   - Exactness: var/function/simple-parameter bindings are initialized before any code can name
     them; typeof depends only on the type, [[Call]] and [[IsHTMLDDA]].
   - Test262 at risk: let/const/class TDZ, parameter TDZ, cyclic modules, expressions/typeof, annexB
     emulates-undefined.
5. **Constant block parameters (vendor cranelift-frontend `ssa.rs`).**
   - Change: when every predecessor passes the same constant, alias it to a fresh constant instead of
     keeping a block parameter, and keep that null out of stack maps.
   - Gain: 25-40K Ir on vn/ft alone.
   - Prerequisite: confirm with one CLIF dump of `vn#f133`.
6. **Null-check dedupe (translator `ProvenReferences`).**
   - Change: record `ref.is_null` facts on `br_if`/`if` arms; in trapping casts test i31 first and let
     the null guard fault on the header load, keeping the trap codes.
   - Gain: about 0.7% now, up to 2% with deferred `ref.as_non_null`.
7. **Type-ids array base and polled memory base as entry-block homes (translator).**
   - Gain: about 0.5-1%.
8. **Drop the caller-side `NativeCallerBoundary` for JS callees.**
   - Gain: about 25 Ir per published call.
   - Test262 at risk: call/apply/Reflect.apply forwarding, legacy caller.
9. **Constant `RegExp(literal)` sites.**
   - Change: bind them at compile time when `RegExp` is statically the intrinsic; hash-index the
     candidate table (`expressions/regexp_program/candidate.rs`).
   - Gain: SSR -8.5%.
10. **Retained-module intrinsic proofs.**
    - Problem: module global reads such as `Math` lose the script form's intrinsic call proof
      (`integer.js` is 80x slower).
    - Next step: find why the proof is not established for module code.

### Wave 2: Realm protectors, then cheaper inline caches

A protector is one bit or epoch per invariant, cleared at the only places that can break it,
as in V8.

- **Protectors to add:**
  - "Intrinsic prototype unchanged" for Object/Function/String/Array.prototype. It cuts the
    absent-key and intrinsic-hit proofs from about 45 to about 6 Ir: about 30K Ir in RSC alone.
  - "No indexed properties on %Array.prototype%/%Object.prototype%", for packed `push`.
  - "No `toJSON` key on the intrinsic prototypes", for `JSON.stringify`.
  - "Array iteration unmodified" (Array.prototype[@@iterator] and %ArrayIteratorPrototype%.next),
    for for-of and spread.
  - An `instanceof` epoch for intrinsic constructors.
- **Inline one-record guards:** with the protectors in place, guard each site in P on its main hit
  kind, with no record scan and no br_table. Worthwhile only if each guard is at most about 25 Ir;
  measure the absent-key kind (473 hits per request) first. Up to about 180K Ir per RSC request
  (6-7%).
- **Exactness:** each guard proves a subset of what the R entry proves today.
- **Test262 at risk:** Object.prototype mutation, annexB `__proto__`, Reflect,
  Function/prototype/Symbol.hasInstance.

### Wave 3: numbers, loops and builtins

- **Int32 domain for bitwise results**, and Signed32 captured slots in the closed-Number region.
  - Gain: frame-compact -40..60 Ir per entry plus the latency chain; frame-branches -60..90.
  - Exactness: ToInt32 is total and never produces -0 or NaN.
- **Closed loops read invariant captured bindings once.** global-bound-deep goes from about 936 to
  about 15 Ir per iteration (about 46 us to 5 us; V8 3.2 us).
- **Compute environment parent links and cell tables once per function.**
- **Member calls on a dense array use the inline element read** (`steps[k](row)`).
- **for-in shape cache with enum-index slot reads.**
- **Cheaper `instanceof` sequences** (stop at Object.prototype, share across consecutive sites).
- **JSON.stringify:**
  - a "JSON-plain" bit on strings;
  - a hot entry for plain objects and arrays with cold helpers;
  - iteration instead of native recursion.
- **`split`:** a single-unit separator uses `code_unit_find`; one presized result array.
- **JSON.parse without a reviver** builds objects directly.
- **Iterator results:** a realm-cached compact `{value, done}` shape.
- **Array.from:** integer-key define.
- **Small items:**
  - unroll the ASCII scan loops;
  - raise the `array.new` dynamic-zero threshold;
  - an integer `number_to_string` path.

### Wave 4: medium-to-large structural

- **GC:** a pretenure window for fresh Stores (SSR -26%); first try a 32 MiB initial heap, which
  gives a 4 MiB nursery.
- **Fresh entries:** lazy program literals and metadata; funcref initialisation that finds the
  module once per instance.
- **Vendored regalloc2:** a split-limit campaign (`MAX_SPLITS_PER_SPILLSET` 2 to 4/8/16, possibly
  R only), plausibly 1-4%. Watch compile time on R and on the 2.2 MB Test262 body.
- **Stack-map stores** placed before the safepoints, not at the definition.
- **Shared call-site machinery:** emit each member-call site's machinery once per function, which
  fixes frame-branches512's I-cache misses and RSC's I-cache pressure (140K misses against Node's
  8K).
- **Direct entries:** C1a direct JS entries with a per-site cached (code, entry) guard, and fewer
  parameters per JS body.
- **Sloppy calls:** a preshaped ArgumentsObject plus an "implementation-owned" bit instead of four
  lookups per sloppy call (3,045 to 400-600 Ir).
- **WeakMap in the GC heap** (C6).
- **ISA baseline:** x86-64-v2 for R and prepared images, rejecting older hosts clearly.

### Wave 5: object model and ABI (large)

- **Object model:** the prototype in the shape with validity cells; the shape and value lanes on the
  object; unboxed array elements.
- **Strings:** a Latin-1 representation.
- **Tail CC:** more argument registers (vendored Cranelift ABI).
- **Value representation:** revisit C2b (one lane for values known to be objects or Numbers)
  after wave 1 item 5. C2a NaN-boxing does not address the measured causes, because it keeps
  binary64 for int32 work.

## Expected effect (estimates)

- **RSC:**
  - Backend items: 8-12% of JIT Ir.
  - Wave 2: 7-8% more.
  - JSON, WeakMap and GC: about 7%.
  - Spill and constant fixes overlap the backend estimate.

  Together these could bring RSC from 1.6x to roughly 1.15-1.25x V8.
- **Beating V8 on RSC-like code** probably also needs wave 5's object model, or inline caching that
  approaches V8's 3-5 Ir monomorphic load. Today's inline hit is about 60 Ir.
- **Situations:**
  - control-empty, global-bound-deep, typeof/singleton-dispatch and frame-* each have a specific
    fix above that removes most of their gap.
  - The global-lookup, property and metadata situations where Speed already wins stay ahead.

## Also found (not performance)

- Recursion deeper than 128 active source bodies throws RangeError
  (`MAX_ACTIVE_SOURCE_BODIES`, gc_types/source_activation_depth.rs:17); V8 allows about 10K.
- The test262 runner pins every run to one CPU unless the wrapper gets `--cloud-worker-cpus auto`,
  and one-shot case workers are about 18x slower than `--reuse-case-workers`
  ([CONTRIBUTING](../../CONTRIBUTING.md#test262)).
