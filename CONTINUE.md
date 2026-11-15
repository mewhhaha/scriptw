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

`main` holds everything below. Per request, Speed policy, user-mode counts:

| | Scriptw sync | Node sync | Scriptw async | Node async |
| --- | ---: | ---: | ---: | ---: |
| Instructions, median | 4.87M | 4.37M | 5.34M | 4.62M |
| Instructions, mean | 4.95M | 4.41M | 5.44M | 4.64M |
| Cycles, median | 2.42M to 2.54M | 2.59M | 2.80M | 2.79M |
| Cycles, mean | 2.49M to 2.65M | 2.61M | 2.91M | 2.81M |

Scriptw counts are over 100 requests after 100 warm-ups. Instructions are from
the current head. The work started at 20.12M instructions per sync request.

- Sync is 1.11 times Node in instructions and about level with it in cycles.
  The two sync cycle figures are two runs: the lower before the `arguments`
  change on a quieter machine, the higher after it on a busier one. The async
  cycles are from before that change.
- On an unchanged base, the young generation cut the sync mean from 5.59M to
  5.23M instructions by removing most full collections. Its write barrier costs
  under 1% on the median.
- Async is level with Node in median cycles, about 4% behind in the mean, and
  1.16 times Node in median instructions.
- Cycle counts need a quiet machine. These were taken at a load average of 3
  to 4 and the Node figures in an earlier run, so re-measure both together
  before claiming the comparison.

## In flight when this was written

### Young generation for the copying collector

On `main` as the five commits ending "Let a limited GC heap grow to its limit".

- 2 MiB nursery taken out of the 16 MiB initial heap, a write barrier in
  compiled code and in the host, a remembered buffer, and full collections under
  Appel's rule.
- The last commit lets a limited heap grow to its limit when doubling is
  refused. It fixes `aot_gc_virtual_reservation`, which the nursery broke by
  taking 1 MiB from each semi-space.
- Verified on the exact tree of that last commit: engine suite 4,209 passed and
  0 failed, the vendored collector tests, and Test262 `WeakRef`, `WeakMap`,
  `WeakSet` and `FinalizationRegistry`. `built-ins/Array` passed 6,119 of 6,119
  across that tree and the one emitter commit before it.
- Not rerun on that tree: the source guards and the engine doctest, which
  passed before the growth fix.
- A minor collection costs about 0.47M instructions and runs every 4.3
  requests. About 0.11M of it is the roughly 460 WeakMap entries React Flight
  creates per nursery, most of them dead by the collection.

### Emitter round 6

Step 1 (strict `arguments` reads without the object) is finished: the WIP
commit plus the one after it, which admits loops, for-in, property writes,
declaration evaluation and bound method receivers in the closed walk, so
`createElement`, `cloneElement` and one argument-count check are admitted.
Retained RSC sync 5.054M -> 4.869M instructions (median), async 5.534M ->
5.336M; see benchmarks.md, "Arguments reads without the object".

The planned order:

1. Done: the `arguments` object (above).
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
Elsewhere, start from `scripts/benchmark-rsc.py` and
[CONTRIBUTING.md](CONTRIBUTING.md), which build the fixtures and report request
times; the commands below need the local helpers.

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
