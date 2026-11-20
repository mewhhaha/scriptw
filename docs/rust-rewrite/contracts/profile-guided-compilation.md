# Profile-guided compilation

An execution profile records how one Wasm AOT program ran and lets a later
build of the same program emit cheaper code for what that run saw. It changes
the emitted program module (P) only. The shared runtime module (R), the runtime
ABI, the object model and every R cache stay profile-independent. Profiles are
off by default: `CompileOptions::execution_profile` is `ExecutionProfileUse::None`
and that build is byte-identical to a build without profile support.

## The rule: a wrong profile costs speed only

Every specialisation a profile selects is a guarded prefix of pure loads and
tests. The prefix observes nothing and writes no part of the result until its
proof succeeds; on failure it falls through to exactly the code an unprofiled
build emits (the site's inline hit, then the R entry, then the general path).
A profile of another input, a stale profile or a deliberately wrong one
(`ExecutionProfile::adversarial`, every read site hot with a rotated kind) can
make code slower and larger, never change a result, an exception, an output
event or evaluation order. The proofs reuse R's proof emitters, so P and R
cannot disagree about when a record proves a read.

A profile is never silently ignored. Decoding a profile of another format
version, admitting a profile of another program or compiler, and emitting with
a profile trained on a different site map are distinct errors
(`ProfileMismatch::{Version, Program, SiteMap}`); the compilation fails.

## Identity

- The **program key** is the program Wasm cache key of the unprofiled build:
  compiler fingerprint, target, goal, every compilation option, the entry source
  and the module-graph digest, in the role the entry was compiled in. Profiles
  name it, and admission requires it, so a profile applies only to the same
  source, options and compiler executable that recorded it.
- The **site map** is the ordered list of static property read and write sites
  of P, each a `SiteRef { body, kind, ordinal }`. The emitter enumerates it
  identically for unprofiled, collecting and applied builds; a profile carries
  its SHA-256 and emission rejects a mismatch.
- The **program cache key** distinguishes the three choices: `None` adds no
  field (unprofiled keys are unchanged), `Collect` adds tag 1, `Apply` adds tag
  2 and the SHA-256 of the profile's canonical `.swprof` encoding.

`.swprof` is magic `SWPROF`, a u16 format version, the program key, the
site-map digest, the compiler fingerprint (informational), the training
request count and the body, read-site and write-site tables, little-endian.
`ExecutionProfile::encode`/`decode` are the codec; decoding rejects
truncation, trailing bytes, duplicates and unknown codes.

## Collecting

`ExecutionProfileUse::Collect` builds an instrumented P: one i64 counter per
body entry and per static property site in a P-defined memory exported as
`scriptw.profile`, an exported `scriptw.profile.dump` that writes what each read
site's record chain learned (record kind, chain length, learning-budget-spent
flag), and a `scriptw.profile.sites` custom section that maps both to the site
map. Counters touch only that memory, so behaviour and what the site records
learn are the unprofiled build's.

After a training run of N requests, an embedder reads the profile with
`EmbeddedInstance::collect_profile(&mut store, N)`, which calls the dump and
decodes the memory against the section. A program that was not built with
`Collect` is refused with `ProfileError::NotCollecting`, and a malformed or
foreign memory or section with the decoder's typed `ProfileError`
(`EngineError::execution_profile_error`). `EmbeddedRuntime::precompile`
refuses a `Collect` build with `ProfileError::CollectingNotPrecompilable`,
because a precompiled image keeps no raw P to read the profile against. Counters count module initialization and every export
call as well as requests.

## Applying

`ExecutionProfileUse::Apply(Arc<ExecutionProfile>)` admits the profile against
the program key when the source is lowered. A static read site is specialised
when its records proved one kind (`ReadSpecialisation`, a closed enum of 20
kinds) and it ran at least `HOT_READ_EXECUTIONS_PER_REQUEST` (8) times per
training request (`ExecutionProfile::hot_reads`). Megamorphic and mixed-kind
sites are not specialised.

Entry points:

- Embedding: set `CompileOptions::execution_profile`, then `Engine::emit_wasm`
  and `EmbeddedRuntime::compile` as usual. The profile must come from a
  training build with the same compilation options.
- RSC host (excluded diagnostics, refused by `scripts/benchmark-rsc.py` for
  scored campaigns): `SCRIPTW_RSC_PROFILE_COLLECT=<path>` trains on the run's
  requests and writes the profile, `SCRIPTW_RSC_PROFILE_USE=<path>` builds with
  it. The exact Flight output check is unchanged.

Verification-only surface, compiled only with the non-default
`profile-experiments` feature of `scriptw-engine`, `scriptw-test262` and
`scriptw-cli` (never in a product build or a published Test262 run):

- `Engine::adversarial_execution_profile`: a wrong profile of a source (every
  read site hot, its kind rotated from a seed), `None` only when the program
  itself is rejected with a source diagnostic (parse, early, link or
  lowering); any other failure is an error.
- `SCRIPTW_RSC_PROFILE_ADVERSARIAL=<seed>` in the RSC host.
- `SCRIPTW_TEST262_EXECUTION_PROFILE=collect|adversarial:<seed>` with
  `SCRIPTW_TEST262_EXECUTION_PROFILE_LOG=<file>`: every Wasm AOT case is built
  collecting or with a wrong profile, and appends to the log `profiled`,
  `unprofiled-compile-negative` (a parse or early negative, whose build fails
  first) or `unprofiled-own-build-fails`. Any other failure of the hook fails
  the case. `$262.agent` worker programs stay unprofiled. A build without the
  feature fails every case while either variable is set, and `test262
  publish-status` refuses to run while the hook variable is set.
- `scriptw precompile --profile <swprof>`. The CLI compiles with `filename` set
  to the source path as typed, and has no way to collect, so only a profile
  an embedder collected under exactly those options is admitted.

## Validation

- `scriptw-aot-wasm --lib profile`: codec (including observations collection
  cannot make), admission, collector, classifier and the guards' proof steps.
- `scriptw-aot-wasm --test emission profile_application`: site maps under every
  profile, specialisation, byte-identical empty profiles, Collect-mode
  instrumentation; `SCRIPTW_PGO_CORPUS=1` over the CLI fixtures.
- `scriptw-engine --lib embedding::tests::execution_profile`, all executed in
  Wasmtime:
  - the emission fixtures trained for real: counters (main 1, probe body 64,
    probe read 64, except an Array receiver, whose read takes the general
    path its Array heap shape plans and never reaches the site record), the
    kind each decodes to, polymorphic and megamorphic
    flags, the codec and admission round trip against the unprofiled key,
    and a true profile's guards spending less Wasm fuel than the unprofiled
    build;
  - the exactness fixtures: each trains its probe site under Collect on
    `train()` alone, the true profile's guard must hit (less fuel for
    `train()`), and then `mutate()` meets a Proxy receiver, a getter or
    redefinition on the prototype, `setPrototypeOf` on the receiver and on the
    prototype, `delete`, `defineProperty` (non-writable, accessor,
    non-enumerable), `freeze`/`seal`/`preventExtensions`, a key or method on
    `Object.prototype`, a `String.prototype` edit, or receivers of another
    Realm. Unprofiled, Collect, true and three adversarial builds answer
    identically as Modules through embedding; the same sources as Scripts
    through the engine's run path are checked unprofiled, Collect and with
    three adversarial profiles, but with no true profile (a Module-trained
    profile names another program);
  - distinct cache keys and typed refusals.
- Retained RSC requests: Collect (sync), the true profile and adversarial seeds
  0 and 1 (sync and async) pass the host's exact Flight output check.
- Test262, wasm-aot, the same 15 subtrees unprofiled and with each hook
  (property-accessors, assignment, call, delete, compound-assignment and super
  expressions; class and for-in statements; Object, Proxy, Reflect, WeakMap,
  Array.prototype.push, Function.prototype.call, String.prototype); results
  in the verdict section.

## Measured verdict (2026-11-20)

S1, inline read proofs at hot single-kind sites, is **no-go** under its
design threshold (at least 80K Callgrind instructions saved per sync request
with P growth of at most 2%). Retained RSC requests, Speed policy, trained on
10 sync requests (33 of 1,384 read sites hot with one kind), 97 counted
requests each, `scripts/rsc_instruction_counts.py count`:

| | unprofiled | applied | change |
|---|---:|---:|---:|
| sync, Ir per request (median) | 2,791,401 | 2,753,746 | -37,655 (-1.35%) |
| async, Ir per request (median) | 3,021,140 | 2,982,630 | -38,510 (-1.27%) |
| `cached_property_read` calls, sync / async | 1,171 / 1,235 | 583 / 617 | -50% |
| P bytes | 4,304,878 | 4,313,471 | +8,593 (+0.20%) |

The guards convert half of the R read-entry calls, but in the hottest body
(`vn`) they cost about 145 instructions per converted call, nearly the R
proof they replace, so only the call overhead is saved. The remaining entry
calls are at hot sites whose records prove more than one kind. A Collect
build's P is 2.3% larger, an adversarial one's 20%. A guard for an own array
slot is not emitted where the site's own inline hit already proves it.

Test262 (pinned suite, wasm-aot, `--threads` 2–3, 60 s case timeout):

| run | subtrees | cases | pass |
|---|---|---:|---:|
| unprofiled | all 15 | 21,282 | 21,282 |
| adversarial seed 0 | all 15 | 21,282 | 21,282 |
| Collect | all 15 | 21,282 | 21,282 |

Per-case results are identical to the unprofiled run's in every case run
(`target/pgo-t262-compare.py`: 21,282 common execution ids, none differing,
none in one run only, for both comparisons).

These runs did not use the committed code. Every snapshot records source
revision `d63eac23f` with uncommitted profile edits: the unprofiled run used
executable `af022d5f…` (before the own-array guard skip), the adversarial
and Collect runs executable `11d48a60…` (with it). That hook still fell back
to an unprofiled build on any error and had no case log. Later commits
changed the hook (b3fcee87f, 658d90ec4) and, independently, arguments
resolution, `import.meta` source text and the code sink; the table has not
been rerun on them.

Not verified:

- Native preparation time: three alternating runs on a machine at load about
  10 gave process user CPU 8.47–8.67 s unprofiled and 8.51–8.76 s applied;
  the ranges overlap, so no change was measured reliably.
- Test262 `Function.prototype.apply` and `bind` and
  `annexB/built-ins/Object/prototype/__proto__` from the design's list were
  not run.
- The Test262 runs above predate the hook's case log, so how many cases were
  built profiled is not recorded. Cases whose own unprofiled build fails
  (parse and early-error negatives) run unprofiled by design; the adversarial
  build compiles the program twice and its P is about 20% larger, and the
  adversarial pass ran about twice as long, so the profile was applied. With
  the log, the property-accessors subtree builds 40 of 42 cases profiled and
  its 2 parse negatives unprofiled.
- `scriptw precompile --profile` end to end: only its argument parsing ran
  as a test. Refusals of a foreign or malformed profile are tested only
  through the engine API.
- The hook's case log, the label for build-failing cases and the
  feature-build `publish-status` refusal have not run in a Test262 pass;
  snapshots and checkpoints do not record a hook mode, so a feature build's
  profiled checkpoints could be resumed and published once the variable is
  unset. Owner: the profile-guided compilation stream; rerun the adversarial
  pass on a committed `profile-experiments` build, with the case log kept,
  before S2–S5 build on this.
- Spec-exec runs ignore the hook variables; the loud refusal covers only
  wasm-aot cases.
- A true profile on the Script goal through the engine's run path.
- Collect mode of the graph and rooted-snapshot emission, and `$262.agent`
  worker programs under any profile.
