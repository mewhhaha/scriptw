# Owned runtime section diagnostic

This unscored diagnostic comes from CI40611737589, source `45f26184b16ba0841346c537e517112d08aa74ab`, using feature `1fd6940fb3db57b999dc535595f5a185870cae69`. All 68 scored attempts and 17 workload checksums were admitted as context; the published [matched Core cohort](memory-ordered-bootstrap-20261028.md) remains the scored reference. The README comparison, V8/status and graphs are unchanged.

Six exact engine controls and all four source/nonce/index-bound v2 ownership captures validate. The Portable R layout29 content key matches the prior Core cohort; this does not assert native byte equality across runners. Raw CLI example-test job logs are unavailable, so no exact CLI example count is claimed.

At the retained warm endpoint (after_store_drop, observation16 after 16 earlier calls):

| Case | R image pages | Text | Other image pages | Mixed/boundary pages | Uncovered | Header/table overlap |
|---|---:|---:|---:|---:|---:|---:|
| integer.js | 680 | 461 | 219 | 7/7 | 0 | 1/1 |
| retained-objects.js | 968 | 557 | 411 | 8/8 | 0 | 1/1 |

| Case | Nontext section | Section bytes | Present-page overlaps | Full present pages |
|---|---|---:|---:|---:|
| integer.js | .wasmtime.engine | 1102 | 1 | 0 |
| integer.js | .wasmtime.bti | 1 | 1 | 0 |
| integer.js | .eh_frame | 107316 | 27 | 26 |
| integer.js | .wasmtime.addrmap | 7182758 | 8 | 7 |
| integer.js | .wasmtime.stackmap | 4294388 | 0 | 0 |
| integer.js | .wasmtime.traps | 2112068 | 0 | 0 |
| integer.js | .wasmtime.exceptions | 25544 | 5 | 4 |
| integer.js | .rodata.wasm | 2039616 | 147 | 145 |
| integer.js | .name.wasm | 38327 | 10 | 8 |
| integer.js | .wasmtime.info | 61968 | 16 | 14 |
| integer.js | .symtab | 54072 | 2 | 1 |
| integer.js | .strtab | 111597 | 8 | 7 |
| integer.js | .shstrtab | 187 | 1 | 0 |
| retained-objects.js | .wasmtime.engine | 1102 | 1 | 0 |
| retained-objects.js | .wasmtime.bti | 1 | 1 | 0 |
| retained-objects.js | .eh_frame | 107316 | 27 | 26 |
| retained-objects.js | .wasmtime.addrmap | 7182758 | 10 | 8 |
| retained-objects.js | .wasmtime.stackmap | 4294388 | 191 | 190 |
| retained-objects.js | .wasmtime.traps | 2112068 | 0 | 0 |
| retained-objects.js | .wasmtime.exceptions | 25544 | 5 | 4 |
| retained-objects.js | .rodata.wasm | 2039616 | 147 | 145 |
| retained-objects.js | .name.wasm | 38327 | 10 | 8 |
| retained-objects.js | .wasmtime.info | 61968 | 16 | 14 |
| retained-objects.js | .symtab | 54072 | 2 | 1 |
| retained-objects.js | .strtab | 111597 | 8 | 7 |
| retained-objects.js | .shstrtab | 187 | 1 | 0 |

The measured leaders differ: `.rodata.wasm` has 145 full present pages in both cases; `.wasmtime.stackmap` has 190 in retained objects and zero in integer. These identify data and GC-metadata origins to inspect before a matched intervention. They do not justify removing required data or stack maps. Overlap counts share boundary pages and must not be summed or converted into additive RSS.

The inert archive retains the final validator, raw scored/diagnostic reports and observations, six control streams, exact parsers/source instrumentation, and the first validator’s failed section-leader assertion. That failure and its authentic counters remain incomplete. No native payload is retained. The validator reads fixture text and metadata and executes no workload; no causal saving, parity or project-completion claim follows.
