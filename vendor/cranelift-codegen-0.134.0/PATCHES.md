# No second zero-extension after 32-bit results (2026-11-22)

This directory vendors the crates.io `cranelift-codegen` 0.134.0 release,
retaining its Apache-2.0 WITH LLVM-exception license. The original crate
checksum is
`dce9c62e5e11d12f4a8f5f939f29899f32002d38193b47491e2539705a1d447e`.
Its recorded upstream commit is
`39d67168cdb6911ffdc961381314b6fed62e3c93`, path `cranelift/codegen`.
The workspace selects this copy through `[patch.crates-io]`; dependency
versions, features and target/ISA settings are unchanged. The registry
installation marker was omitted; all other release files are kept byte for
byte (checked against the extracted `.crate`), including the license's
trailing blank line and the trailing spaces on line 2 of
`src/dominator_tree/simple.rs`.

The build script generates the ISLE lowering code into `OUT_DIR` from the
`.isle` sources here, so an edit to them takes effect on the next build
(the build script tracks them). It also asks `git rev-parse HEAD` in the
crate directory for `cranelift_codegen::VERSION`, which inside this
repository becomes `0.134.0-<checkout commit>` instead of `0.134.0`. Nothing
in the dependency graph reads that constant (Wasmtime's module version and
Scriptw's native compiler fingerprint do not).

## The change

`src/isa/x64/inst.isle`, `value32_zeros_upper32`: the x64 backend lowers
`uextend.i64` of an `i32` value to nothing when the instruction that defines
the value already wrote a 32-bit result, which x86-64 zero-extends into the
whole register (Intel SDM vol. 1, 3.4.1.1). Upstream listed `iadd`, `isub`,
`imul`, `band`, `bor`, `bxor`, `ishl`, `ushr` and `uload32` (which never
produces an `i32`, so that rule never fires). Three opcodes are added, each
restricted to `$I32`:

- `load`: an `i32` load has exactly one lowering, `load_sub64_x64_movzx`
  (`x64_movzx (ExtMode.LQ)`, which is `movl` from memory). The other `load`
  rules are for XMM types, 64-bit types and `i128`; a big-endian `i32` load
  has no x64 lowering at all. A load sunk into another instruction has that
  instruction as its only use, so it is never also the source of a
  `uextend`; when the `uextend` itself is the only use, the load is now
  lowered on its own (`movl` from memory) instead of being sunk into the
  extension (the same `movl`).
- `sshr`: lowered only by `x64_sar $I32` (`sarl` by `%cl`, by an immediate
  or by one). A 32-bit shift writes its destination, zero-extended, even
  for a count of zero, which upstream already relies on for `ishl` and
  `ushr`.
- `select`: an integer `select` of a type that fits in 32 bits lowers only
  through `lower_select_gpr`, whose last instruction is a 32-bit `cmov`
  (`cmove` with `fits_in_32`); a 32-bit `cmov` clears the upper half even
  when its condition is false (SDM vol. 2, CMOVcc). The `And` form is
  swapped to `Or`, two 32-bit `cmov`s.

The predicate's only use is rule 2 of `extend_to_gpr`, which matches
`(value_type $I32)` to `$I64` with `ExtendKind.Zero`. Register moves are
64-bit (`movq`) and regalloc2 spills and reloads integer registers as
`I64`, so a zero upper half survives both.

Why: Wasmtime's GC references are `i32` heap indices, addressed as
`base + uextend(ref) + offset`. A reference loaded from a field, or reloaded
from a stack-map slot (an `i32` `load` from `stack_addr`), and used more
than once was followed by `movl %r, %r` before each address computation:
about 52-72K of the React Server Components program's 2.7M instructions
per request (target/deepdive/backend/REPORT.md B1), plus one cycle of
latency on every pointer step.

This changes native code by design (one `movl` register copy fewer at such
extensions; a load that only fed its extension is no longer sunk into it,
with the same instruction).

## Tests

`src/isa/x64/zero_extension_tests.rs` compiles small functions for x86-64
at `opt_level` `none` and `speed` and reads the VCode text:
`i32_load_is_not_zero_extended_again`, `i32_sshr_is_not_zero_extended_again`
(by a register and by an immediate) and
`i32_select_is_not_zero_extended_again` (one and two conditions) find no
`movl` between registers; `i32_argument_is_still_zero_extended` keeps the
one `movl` for an `i32` argument, whose upper half is unknown. Without the
patch the first three fail (for example `movl %edi, %edi` after the load).
All 188 crate unit tests pass.

The crate is not a workspace member, so its tests run out of tree:

```sh
cp -a vendor/cranelift-codegen-0.134.0 "$SCRATCH/cg"
cd "$SCRATCH/cg" && printf '\n[workspace]\n' >> Cargo.toml
cargo test -j 1 --lib
```

`--offline` does not resolve there, because the optional `capstone`
dependency is not in a registry cache that the workspace build fills.
