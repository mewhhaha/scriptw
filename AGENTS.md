# AGENTS.md

## Product and compiler contract

- Scriptw is a greenfield Rust library and CLI rewrite until 1.0. Breaking changes are welcome when they improve the compiler architecture or ECMAScript correctness; do not preserve legacy APIs, layouts or behavior merely for compatibility.
- Wasm is the sole product compilation target. C/native JavaScript output and a REPL are outside scope.
- Compile JavaScript through parsing, early errors, spec IR, lowering IR and real Wasm codegen. Never ship an interpreter or VM compiled to Wasm, or emit an evaluator blob instead of the compiled program. An interpreter may exist only as a hidden debugging or differential-testing tool, never as the product or CLI execution path.
- Dynamic source evaluation (`eval`, `new Function`, cross-realm Function constructors) is explicitly unsupported in standalone Wasm when it would require bundling a parser, interpreter or VM. Report these cases; do not silently skip them.

## Runtime and correctness

- Experimental Wasmtime is the required backend baseline unless the task explicitly narrows it. The backend may require Wasm GC, reference types, typed function references, exception handling with `exnref`, and related experimental features.
- Maintain one backend and object model. Reject runtimes missing required capabilities clearly; do not add parallel heaps, closures, exception mechanisms or compatibility backends to emulate missing baseline features.
- Keep runtime assumptions explicit in code, tests and docs. Wasmtime support does not imply browser, wasm3, wasmi or other engine support.
- Target literal 100% ECMAScript and pinned Test262 conformance. Correctness comes before speed. Every failure needs an owner and reason; permanent skip lists, silent expected failures and regressions are unacceptable.
- Keep fake-suite results, subsets and full pinned Test262 results distinct. A green fixture suite is not 100% ECMAScript or Test262.

## Enforce invariants in code

- Validate once in a constructor and require the validated type at subsequent uses.
- Use enums for closed domains, newtypes for distinct identifiers, and exhaustive matches so missing cases fail to compile.
- Encode evaluation order and lifecycle requirements in types when this prevents a plausible mistake. Keep unreachable product paths out of the build rather than relying only on tests or public visibility to hide dead code.
- Avoid speculative abstractions: if the type prevents no plausible mistake, prefer a plain function.

## Development and verification

- Identify the whole coherent batch and its independent chunks before implementation. Implement independent chunks concurrently with subagents; coordinate ownership of shared files. Create only the smallest necessary seam when work is coupled.
- Finish code, tests, types and docs before expensive builds or broad suites. Run cheap source and structural checks first; compile early only to resolve an uncertainty or risky dependency. Batch commits so Git identity changes do not repeatedly invalidate builds.
- Compile the batch once, then run focused regressions and affected broader suites sequentially. Preserve passing scopes after a failure or test-only correction; rerun affected checks and continue unfinished scopes instead of restarting the batch. Reuse requires unchanged relevant inputs, with original source and binary identities retained. Report actual coverage and unverified work; prefer existing tooling over custom evidence frameworks.
- Do not add tests that only mirror reversible, low-impact edits. Prefer checks that enforce meaningful compiler or tooling behavior.
- Local verification uses `python3 scripts/limited_verification.py -- <command>`: at most 4 GiB, no swap, grouped OOM termination, serial CPU/workers and bounded retained-module caches. Do not run uncapped expensive verification.
- On managed cloud machines, use `--cloud` with the inherited finite cgroup budget. The opt-in `--cloud-cpus auto` mode respects affinity and CPU quotas; Cargo and libtest remain serial. Preserve explicit worker counts, cache bounds and deadlines.
- [CONTRIBUTING.md](CONTRIBUTING.md) owns checkout, build, verification and publication commands. [Current work](docs/rust-rewrite/current-inventory.md) owns the short unresolved queue. Keep completed narratives in Git, not new diaries.

## README and published status

- Update `README.md` with user-visible capability, conformance, CLI, architecture or workflow changes.
- When status changes, refresh generated counts, commands and date together. Never hand-edit the pinned Test262 status numbers.
- Publish with `scriptw test262 publish-status --execution-backend <spec-exec|wasm-aot>` or the equivalent `cargo run -p scriptw-cli -- ...` command. For low RAM, use `./scripts/publish-real-status-low-ram.sh <backend> <snapshot-name>`; it checkpoints one node per process and publishes only after verified completion.

## Workspace and retirement boundary

- Workspace: `crates/`; public API: `crates/scriptw-engine`; CLI: `crates/scriptw-cli` / `scriptw`; conformance tooling: `crates/scriptw-test262`.
- Keep packages, commands, configuration, caches and host interfaces under the canonical Scriptw identity.
- The retired JavaScript implementation exists only in Git at `2107dfe9ad58c730e3d19b0cc1c73ed4390602f8`. It is neither a development surface nor an oracle; do not restore or archive it in the working tree.
- Do not restore a JavaScript compiler/runtime, npm/JSR entrypoints, JavaScript Test262 runner or browser compiler playground. JavaScript may remain as pinned Test262 content, embedded harness data, Rust fixtures/reproducers or vendored dependencies, never as the product compiler/runtime.
