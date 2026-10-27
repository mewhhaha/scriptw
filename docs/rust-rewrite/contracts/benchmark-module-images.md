# Diagnostic module image ownership

The nondefault `benchmark-module-images` engine/CLI feature exposes address-only
observations of the actual immutable program (P) and optional runtime (R) modules
retained by `EmbeddedRuntime`. It uses Wasmtime's safe image/text range APIs.
The ranges are independent: text need not be contained in the image range.
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
Diagnostic success requires the validated host inventory to be retained as well
as all four cases' validated attribution files.
Shared pages yield nonexclusive symbol overlaps, never additive function RSS.
Names and permissions alone establish neither allocation ownership nor causality.

Verification is pending. Planned controls cover default feature exclusion,
actual P/R ranges and Store-drop lifetime, lazy-hook ordering, malformed ownership
metadata, present-bit bounds and adversarial ELF ranges. No native execution,
RSS reduction or memory-saving claim follows from source checks or static extents.
