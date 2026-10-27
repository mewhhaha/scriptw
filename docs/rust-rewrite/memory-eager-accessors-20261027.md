# Eager accessor construction checkpoint

Verified compiler `00fcf97fbffde8137a67d6ed7ed4803e86384ef1`; matched CI compiler `a960f50acea8bd3a1de0a2c391bbb09b88f2ee4d`, [run 40552965169](https://github.com/mewhhaha/scriptw/actions/runs/40552965169). Runtime layout 28.

Strict ordinary noncanonical standard accessors retain eager fresh function construction. A checked plan proves entry identity, metadata defaults and runtime-owned source ordinal. Header and ExecutableCode allocation precede the shared tail; Context allocation still precedes source-string resolution, FunctionObject allocation and cycle closure. Existing metadata publication remains after construction. Canonical, captured and nonuniform factories retain their existing paths.

The [verification record](memory-eager-accessors-20261027.json) retains exact inventories: 4,881 native cases across 19 targets, 2,365 original pinned executions and 16 disjoint Unicode executions passed. This is a selected conformance checkpoint, not a full pinned-suite refresh. Original failed test-only compilation and the subsequent 503-pass/1-fail inspection-test attempt remain retained. Eighteen unchanged native binaries were reused with frozen hashes; the repaired AOT target was rebuilt. No missing after counters are synthesized. The excluded inspection-test repair and clean/local compiler-input fingerprints bind the verified and measured sources. Local verification used one CPU, a 4-GiB process-tree cap, no swap and bounded caches; selected cases ran in fresh batches of at most 16.

The canonical paired cohort passes original checksums on all 17 workloads, with two rounds, ten warmups and seven measured samples per round. Both consumers disable compilation and use the same Wasmtime executable and settings. Exact scored values are in [the paired report](precompiled-comparison.json) and canonical graphs. Comparing runs on different machines cannot establish causal memory savings. Scriptc parity remains open.

Static inspection of the owned Cargo Portable R artifacts ([source-bound facts and retained symbol streams](memory-eager-accessors-20261027.json)) records RealmInitializeIntrinsics at 888,703 → 765,194 native bytes (−123,509), with the new shared helper at 783 bytes. The raw package decreased by 32,919 bytes and the native ELF by 157,152 bytes. These are static artifact/code extents, not RSS, execution costs or evidence of causal resident-memory savings.

Any paused lifecycle captures retained with this batch are unscored; the [phase methodology](memory-phase-boundaries-20261027.md) remains separate from canonical graphs. Code/path mapping groups do not prove heap ownership, peak memory or causes.

The bounded lossless verification archive excludes executables and serialized native images. Its exact member inventory, byte lengths, SHA-256 hashes and roundtrip limits are recorded beside the source-bound reports.
