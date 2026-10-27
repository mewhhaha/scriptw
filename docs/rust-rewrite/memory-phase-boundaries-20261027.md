# Phase-aligned memory method checkpoint

Compiler `38c25942b63a752ce5db46b6cb42b46ed98f9a57`, CI [40540496469](https://github.com/mewhhaha/scriptw/actions/runs/40540496469), 2026-10-27.

The canonical paired cohort validates all 17 workloads in two rounds with compilation in separate producers and disabled in both consumers. The companion phase captures never enter scored reports or graphs. This checkpoint adds observation controls, not a compiler memory optimization.

13 Rust benchmark-example controls and 49 Python tooling tests passed. Exact inventories, streams, hashes and local resource counters are in the [verification record](memory-phase-boundaries-20261027.json) and its bounded lossless archive. Both clean/local compiler-input closures match e7, as do both locally owned raw runtime packages. The [e7 checkpoint](memory-regexp-unicode-data-20261027.md) passed 4,876 native cases, 2,365 original pinned executions and 16 disjoint Unicode executions; those product suites were not rerun here. No full pinned-suite status is updated.

Four cases (two adapters × integer/retained objects) each captured four strict nonce/sequence/index-bound phases: transport drop, instantiation, execution and Store drop. Raw map hashes, endpoint reports, executable admission checks and the analysis are retained. Observation 16 is selected after 16 earlier invocations; loading precedes all observations. The loading-to-warm difference therefore includes allocator and execution history. Diagnostic totals can include earlier barrier waits; no wait is subtracted and these timings are unscored.

Warm RX mapping differences are approximately 2.4–2.5 MiB for the exact host executable and roughly 1.5 MiB for other executable mappings. Much of the post-execution paired gap persists after Store drop. Permission/path grouping is descriptive: it does not prove heap ownership, a peak, allocation attribution or causal savings. Results from another runner are not a controlled causal comparison.

Local checks used one CPU, a 4-GiB process-tree cap, no swap and bounded retained-module caches. CI processing also records its actual resource counters. The archive excludes executables/native images and the duplicate GitHub download, preserving the 29 inert phase-evidence files and source-bound analysis.
