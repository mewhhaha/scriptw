# Workspace inventory repairs and smaller native tests — 2026-10-11

All 301 focused native checks pass with zero failures or ignores. The joined
all-feature/all-target workspace type check, all twelve repository guards and
eight unchanged Intl reproduction checks pass. The
[receipt](workspace-inventory-repairs-20261011.json) retains exact test inventories,
executables, link events, source identity and logs.

The preceding all-feature workspace sweep completed 169 target scopes:
4,229 checks passed and four failed, with no ignored checks in completed scopes.
Its Engine library finished at 870 passes and one failure. The four failures
were stale exact inventories after weak and Error stack integration:

- the typed GC host import domain now has 34 entries;
- sixteen synchronous iterator policies own all 64 error projections;
- the Error stack intrinsic contributes paired typed accessor endpoints; and
- Engine's emitted-Wasm characterization includes all nineteen weak host imports.

The repairs preserve exact domain checks, producer censuses and actual execution
of the characterization's arithmetic, string, object and caught-throw programs.
The three complete affected AOT targets pass 291 checks; four Engine image and
characterization checks, one fresh-program runtime check and five CLI supervisor
checks complete the 301-pass selection.

`SCRIPTW_TEST_STRIP_DEBUG=1` opts into removing debug sections through the existing
Linux test linker. Stripping occurs before executable identity is sealed. Every
allocated ELF section must retain its bytes, flags, address and alignment. The
current Engine test executable shrinks from 368,369,160 to 186,450,960 bytes.
The independent copied-executable probe also preserves global symbols and
dynamic-link tags, then passes three runtime-image admission controls. Product
and doctest links keep their existing behavior. Rust backtraces lose DWARF source
lines; global symbols and compiled panic locations remain. See the
[workflow](../../CONTRIBUTING.md#verification).

The old broad sweep was deliberately stopped with watcher exit 143 after the
complete Engine library result. Async and builtin targets retain 159 and nine
partial pass markers, respectively; neither is a completed passing target.
Seventy-seven target scopes remained unstarted. The full repaired workspace,
default graph, release/campaign and pinned-suite acceptance remain open.

The unchanged five-second generated campaign remains red on CrossRealm seed
1270, before Temporal seed 3215 is reached. Its oracle completes the exact case;
Wasm AOT times out. A separate diagnostic trace locates runtime-data/package
admission and program native compilation costs, but an older CLI controller
rejects its newer worker identity, so that replay is not acceptance evidence.
No deadline or denominator was changed.
