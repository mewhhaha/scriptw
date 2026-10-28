# Diagnostic module image ownership

The nondefault `benchmark-module-images` engine/CLI feature exposes address-only
observations of the actual immutable program (P) and optional runtime (R) modules
retained by `EmbeddedRuntime`. It uses Wasmtime's safe image/text range APIs.
Section attribution requires checked correspondence between the image buffer,
its serialized length and the actual native text section.
Addresses remain valid while the embedding owner lives, including after its
Stores drop. Returned ranges do not retain a module and must not be dereferenced.
Default builds expose neither the observation method nor its exported types.

Only an active, fully acknowledged four-phase diagnostic invokes the lazy
observation hook. Both adapters emit this metadata after the final ACK; scored
runs and absent-barrier paths omit it. There are no new JavaScript operations,
compiler representations, profiling CLI or changes to scored RSS/timing values.
The complete invocation remains unscored and canonical readers reject its
markers. Address observation does not establish which functions executed.

The Linux parent captures bounded present-bit-only pagemap evidence for readable
VMAs at each existing pause, capped at 4,096 mappings and 131,072 base
pages; PFNs are never retained. After child exit it joins
source/configuration identities and exact nonce/index/four-phase sequence with
closed P/R owner roles. Invalid, overlapping or uncovered ranges and incomplete
presence evidence fail explicit diagnostic validation. Host ELF load/section and
function-symbol bounds are separately validated against the actual host file.
ELF input is capped at 512 MiB, 4,096 sections, 256 program headers and
200,000 symbol entries. Each retained symbol-metadata/overlap file is capped at
16 MiB; cold function counts are retained without copying their code.
ELF names require strict UTF-8 and an in-table NUL terminator. Section names are
capped at 2,048 bytes and function-symbol names at 256 KiB, with the combined
decoded-name budget still 32 MiB. Rust generic symbols can exceed 128 KiB.
Diagnostic success requires the validated host inventory to be retained as well
as all four cases' validated attribution files.
Shared pages yield nonexclusive symbol overlaps, never additive function RSS.
Names and permissions alone establish neither allocation ownership nor causality.

The [baseline checkpoint](../memory-module-ownership-20261027.md) records 31 native
diagnostic test executions, 63 Python tests and the default-feature compile-fail check.
They cover actual P/R owners and Store-drop lifetime, lazy-hook ordering,
malformed ownership metadata, present-bit bounds and adversarial ELF ranges.
The current matched cohort passes all 17 workload checksums; its four additional
ownership captures remain unscored. No RSS improvement follows from this feature.

## Actual owned image sections

The [section-attribution checkpoint](../memory-module-sections-20261028.md)
verifies the extension against six native controls and four actual ownership
captures. It uses the
same lazy hook, exclusively after all four captures are acknowledged. Pinned
Wasmtime 47 serializes the same immutable image buffer exposed by `image_range`.
The observer safely serializes the actual retained P and optional R; the WASI
adapter observes its actual P through the same constructor. It retains only
the digest, byte length and bounded ELF64 section metadata. It never retains
native image bytes or dereferences observed addresses.

Serialization copies and touches the image, so every phase capture precedes
that copy. Later memory readings cannot enter these observations. Ordinary
and scored invocations never call the hook. Collection failure is an explicit
unscored diagnostic error after the captures; it cannot become a scored row.

Each image is capped at 128 MiB before copying. ELF headers, file-backed section
extents, UTF-8 names and the section table are bounded and validated. Images
must match their live range length and uniquely anchor their executable `.text`
section to `Module::text`. The P digest must match its admitted native producer.
Wasmtime artifacts may be relocatable ELF files without program headers;
host executable PT_LOAD translation does not apply to them.

The v2 ownership endpoint carries the source/nonce/index-bound v1 section
metadata. Parent processing reports section/page overlaps and complete pages
separately, with mixed-section boundaries, headers and uncovered image pages
explicit. Shared boundary counts are nonadditive. It does not prorate RSS,
claim that a present section executed, or infer original Wasm data-segment
boundaries from Wasmtime's merged native sections. This instrumentation grants
no native-code provenance independently of the existing producer boundary.
