# Retained embedding verification, 2026-10-25

Product source [`4ad59d1eb`](https://github.com/mewhhaha/scriptw/commit/4ad59d1ebbc64c9e3384aee9cefd19e3c8b8e92d), with the archived test-fixture correction below, passed **5,700 native checks across 23 complete targets**. Every accepted final inventory ran in full, with no failures, ignores or filtered cases. The preliminary 45 focused checks overlap these targets and are not added to the total.

The first `modules_realms` attempt passed 73 of 74 tests: one structural guard still expected the former `ProgramExports.append` argument list. Its assertion was corrected to include `runtime_helper_base`, after which the same complete 74-test inventory passed. A retained-export fixture was also corrected: module export names must be well-formed Unicode, so its export name now uses an astral pair while a separate object-property assertion checks a lone-surrogate key. The API doc comment was corrected accordingly. A third fixture updated the exact expected Wasm export list to include the intended private embedding bridges after its original 32-test batch passed 31 and failed that one stale assertion. The IR namespace fixture then passed 1,625 of 1,626 checks before its unobserved-entry assertion was corrected to use a Script entry; Module entries now have a host observer by design. The same complete 1,626-test inventory was required to pass after that cfg(test)-only correction. These four fixture changes and one documentation-only source edit leave runtime/compiler implementation unchanged from `4ad59d1eb`; the verification source tree is not described as completely clean. All four patch versions and original failed evidence are archived. Final `verification-fixture.patch` SHA-256: `6af747d35ab196853d7d8218b2a3a62da4ada2e7a11047432a262d1ef055a2d4`. Earlier failed/partial attempts are excluded from accepted totals.

The source-free Portable examples passed: the scalar example printed `42`; the retained example preserved NUL, lone UTF-16 surrogates and an astral pair, then called a retained default object's async method with its receiver and returned bytes `[42, 127, 128, 255]` after host settlement.

The selected pinned Test262 replay completed **984 exact execution IDs: 980 successes and the same four known runtime bugs**, with zero crashes, unsupported cases or timeouts. The actual suite exit remains **1**. This selected checkpoint leaves the full Test262 chart and historical V8 performance measurements unchanged.

The original whole-engine scope reached its 4-GiB cap and was terminated by the OOM policy; its exit137 transcript and systemd journal lines remain archived. It is not an accepted target result. The complete 1,029-test engine inventory was instead verified by a disjoint union of fresh bounded native batches, with no identities omitted or duplicated. This is explicitly an aggregate of actual process observations, not a synthetic single-process transcript.

Every accepted native/product/replay receipt establishes one CPU, the kernel-enforced 4-GiB cap and zero swap. Later receipts also observe grouped OOM termination and the one-entry/64-MiB retained module cache directly. The explicitly listed early receipts predate those fields: their group/cache provenance is the archived verified launcher policy, rather than invented kernel observations. No accepted run records an OOM event. The largest recorded cumulative cgroup peak was **4,294,967,296 bytes**; reaching the cap through reclaim is not counted as an OOM kill.

| Native target | Passed |
| --- | ---: |
| `scriptw-aot-wasm` library | 472 |
| `scriptw-aot-wasm` emission | 31 |
| `scriptw-aot-wasm` modules_realms | 74 |
| `scriptw-aot-wasm` runtime_link | 30 |
| `scriptw-aot-wasm` structure_async | 115 |
| `scriptw-aot-wasm` structure_builtins | 166 |
| `scriptw-aot-wasm` structure_language | 152 |
| `scriptw-engine` library | 1,029 |
| `scriptw-engine` aot_async | 248 |
| `scriptw-engine` aot_async_generator_returns | 2 |
| `scriptw-engine` aot_builtins | 541 |
| `scriptw-engine` aot_gc_entries | 193 |
| `scriptw-engine` aot_generators | 167 |
| `scriptw-engine` aot_language | 338 |
| `scriptw-engine` aot_native_program_templates | 21 |
| `scriptw-engine` aot_realm_modules | 428 |
| `scriptw-engine` runtime_cache | 4 |
| `scriptw-engine` structure | 36 |
| `scriptw-engine` weak_gc_facility | 14 |
| `scriptw-runtime-image` library | 3 |
| `scriptw-ir` library | 1,626 |
| `scriptw-cli` runtime_baseline | 7 |
| `scriptw-cli` runtime_boundaries | 3 |

[Machine-readable verification](embedding-bridge-20261024.json) records exact commands, target inventory hashes, executable identities, the selected execution IDs and the unchanged known failures. [The evidence archive](embedding-bridge-20261024.tar.gz) retains inventories, actual transcripts, resource receipts, native snapshots and runner scripts. Its SHA-256 is `118d5f2006f479380e7403818d337361f48c2e9f711486fee673891b8db020b5`. Executables and caches are excluded; the source commit above reproduces product code.

For reproduction, check out the source commit and apply the archived `verification-fixture.patch` for native verification. The implementation remains the source commit plus these exact fixture/documentation changes; the actual compiler source fingerprint and executable hash are retained rather than described as a clean-tree build. Build serially with `python3 -B scripts/limited_verification.py -- ...`, then use the archived `native_final.py` target inventory and replay commands. Copy `engine_batches.py`, `native_final.py` and `run_check.py` into a fresh target directory before rerunning: archived closed receipts are audit evidence, not a new execution. Run `engine_batches.py` first to create the exact engine union, then `native_final.py` for the complete target batch. The archive publisher validates every accepted target against its complete `--list --include-ignored` inventory before publishing; failed, stale and partial artifacts cannot produce a complete checkpoint. Run the examples with `cargo run --locked --release --jobs 1 -p scriptw-engine --example retained_embedding` and `--example foreign_modules` inside the same resource wrapper.
