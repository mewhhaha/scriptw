# Continue here

Handoff for the React Server Components (RSC) performance work, written 2026-11-15.
Delete this file when the work below has landed; the lasting record is
[benchmark details](docs/rust-rewrite/benchmarks.md) and
[current work](docs/rust-rewrite/current-inventory.md).

## Goal

Make the retained RSC request run at least as fast as V8 without its JIT
(Node 24 `--jitless`), for the sync and async cases, in medians and in means.

The rules for getting there:

- ECMAScript-exact semantics and byte-exact Flight output.
- No special cases for this benchmark.
- No heap sizing or collection-trigger changes made to dodge collections. The
  initial heap budget stays 16 MiB.
- Structural changes are allowed, including a rehaul of a subsystem.
- Verified work is fast-forwarded to `origin/main` as it lands.

## Where it stands

`main` holds everything verified so far. Per request, Speed policy, 100 measured
requests after 100 warm-ups, user-mode counts:

| | Scriptw sync | Node sync | Scriptw async | Node async |
| --- | ---: | ---: | ---: | ---: |
| Instructions, median | 5.01M | 4.37M | 5.49M | 4.62M |
| Instructions, mean | 5.50M | 4.41M | 6.13M | 4.64M |
| Cycles, median | 2.53M | 2.59M | not re-measured | 2.79M |
| Cycles, mean | 2.75M | 2.61M | not re-measured | 2.81M |

The work started at 20.12M instructions per sync request.

- Sync median is level with Node in cycles and 1.15 times Node in instructions.
- The sync mean is about 5% behind in cycles. The gap between Scriptw's median
  and mean is collection pauses, which the young generation below removes.
- Async is further behind, 1.19 times Node in median instructions.
- Cycle counts need a quiet machine. The async cycles of the current head have
  not been measured on one.

## In flight when this was written

### Young generation for the copying collector

Branch `claude/nursery-collector` on the remote: five commits on top of `main`,
**not yet verified on that base**, which is why they are not on `main`.

- 2 MiB nursery taken out of the 16 MiB initial heap, a write barrier in
  compiled code and in the host, a remembered buffer, and full collections under
  Appel's rule.
- Measured on its original base over 900 requests: sync mean 5.59M to 5.23M
  instructions, async mean 6.18M to 5.73M, medians up by under 1% from the
  barrier, worst request roughly halved.
- The last commit lets a limited heap grow to its limit when doubling is
  refused. It fixes `aot_gc_virtual_reservation`, which the nursery broke by
  taking 1 MiB from each semi-space.
- Verified on the original base `3dacf6024`: vendored collector tests, and the
  engine suite before the growth fix. Still running at handoff on that base: the
  engine suite with the growth fix, a 2 MiB-heap stress run, and Test262 for
  `WeakRef`, `WeakMap`, `WeakSet`, `FinalizationRegistry` and `built-ins/Array`.

To land it: run the engine suite and the RSC measurement on the branch head,
confirm the two patch logs (`vendor/*/PATCHES.md`,
`docs/rust-rewrite/vendor-patches.md`) describe it, then fast-forward `main`.

### Emitter round 6

Uncommitted and local only, in the sibling worktree `../scriptw-loadic`
(`arguments_protocol/strict_projection.rs`, `environments/arguments_projection.rs`
and a new engine test `aot_arguments_projection_reads.rs`). Nothing from it is
verified. The planned order:

1. **`arguments` object.** `createElement` only reads `arguments.length` and
   `arguments[i]`, yet a full object is built on every call (142 per request).
   Skip the object where every reference is one of those two reads, and build
   the real object on the spot for anything else.
2. **Read records.** Prototype hits with a builtin holder (`fn.call`,
   `str.indexOf`, `map.get`) still call the helper (17.6% of
   `cached_property_read`). Records also cannot hold a proven absence or more
   than one receiver shape, so `config.key` and `type.defaultProps` always take
   the walk (39.6%).
3. **Write records for non-ordinary receivers.** `r.status = …` and
   `r.value = …` on Promise objects always miss, about 2.7% of async.
4. **Dense `Array.prototype.splice`**, then eliding temporal-dead-zone checks
   by declaration kind.

## Further levers, not started

- Host side of async: growth of the weak-key table and `FuncRefTable::intern`.
- A duplicate `index < length` compare in array element access.
- Cranelift spills every GC reference that is live across a call and reloads it
  at each use. Spills and reloads are 35 to 41% of the cycle samples inside the
  two hottest compiled functions. This is structural; measure it in instructions
  before committing to a design.
- Function environments cost six objects each.
- About 600 test-then-cast pairs, the builtin call site (895 instructions
  against Node's 392) and the remainder of global reads (258 against 156).

## How to measure

Fixtures, the `perf` binary and the helper scripts live under `target/`, which
is git-ignored, so they exist only on the machine this work was done on.

```sh
# Build the retained host, check Flight output, print medians (CPU 14 here).
target/agents/tools/ic_measure.sh <repo> <cargo-target-dir> <tag> 14

# Medians and means from the same run.
python3 target/agents/tools/pmu_means.py <cargo-target-dir>/ic-measure/<tag>/pmu-{sync,async}

# Frame-pointer profile of 100 sync requests.
target/agents/tools/ic_profile.sh <host> <host's rsc-sync.json> <outdir> 14
```

- Fixtures: `target/application-benchmarks-20261102/generated-v2/{rsc.js,support.json}`.
- Node control: `target/agents/tools/node-jitless` passed as `--node` to
  `target/rsc-20261109/attribution/count_request_pmu.py --engine node`.
- Everything expensive runs through `python3 scripts/limited_verification.py --`.
  Pin each job with `taskset -c N`, or two wrapped jobs share CPU 0.
- Instructions are repeatable to about 0.1%. Use them to judge a change, and
  cycles only to compare against Node.

## Local state

- Worktrees `../scriptw-loadic` (emitter, target `target/agents/loadic`) and
  `../scriptw-integ` (host side, target `target/agents/integ`). Reuse them
  rather than adding a third: `/home` is btrfs with its metadata nearly full,
  and a new build tree fails with "No space left on device" while `df` still
  shows free space. `sudo btrfs balance start -dusage=50 /home` gives it room.
- To remove when the work is done: both worktrees, `target/agents/`, the stale
  worktree `target/performance-climb-20261103/env-prune-source`, and the local
  branches `worktree-agent-a24338a241a8153ce`, `worktree-agent-a297918255736d5f7`
  and `worktree-agent-a5cf54d0a36ac730a`.

## Failures that predate this work

- Four emitter `--lib` tests: `bootstrap_compact_metadata`,
  `error_demand_initialization`, `intrinsic_property_layout_allocation`,
  `ordered_bootstrap_allocation`.
- Five vendored `pagemap::tests::*`.
- Test262 `language/function-code/S10.2.1_A4_T2.js` crashes (listed in current
  work).
- In strict code, a write to a top-level `var` whose global property was deleted
  re-creates the property instead of throwing (listed in current work).

## Decisions left to the owner

- Whether Speed or Fast is the default policy. Every number here is Speed.
- The CPU baseline. It is x86-64 without SSE4.1 today.
