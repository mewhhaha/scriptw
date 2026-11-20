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
decodes the memory against the section. It fails for a program that was not
built with `Collect` (and for programs loaded from a precompiled image, which
keep no raw P).

## Applying

`ExecutionProfileUse::Apply(Arc<ExecutionProfile>)` admits the profile against
the program key when the source is lowered. A static read site is specialised
when its records proved one kind (`ReadSpecialisation`, a closed enum of 20
kinds) and it ran at least `HOT_READ_EXECUTIONS_PER_REQUEST` (8) times per
training request (`ExecutionProfile::hot_reads`). Megamorphic and mixed-kind
sites are not specialised.

Entry points:

- Embedding: set `CompileOptions::execution_profile`, then `Engine::emit_wasm`
  and `EmbeddedRuntime::compile` as usual.
- CLI: `scriptw precompile --profile app.swprof app.js -o app.swep`. The
  profile must come from a training build with the same compilation options;
  the CLI compiles with `filename` set to the source path, so a profile
  collected by an embedder that left it unset is refused as another program.
- RSC host (excluded diagnostics, refused by `scripts/benchmark-rsc.py` for
  scored campaigns): `SCRIPTW_RSC_PROFILE_COLLECT=<path>` trains on the run's
  requests and writes the profile, `SCRIPTW_RSC_PROFILE_USE=<path>` builds with
  it, `SCRIPTW_RSC_PROFILE_ADVERSARIAL=<seed>` builds with a wrong profile. The
  exact Flight output check is unchanged.
- Test262 verification only: `SCRIPTW_TEST262_EXECUTION_PROFILE=collect` or
  `adversarial:<seed>` builds every Wasm AOT case collecting or with a wrong
  profile. Never set it for a published run.

## Validation

- `scriptw-aot-wasm --lib profile`: codec, admission, collector and classifier.
- `scriptw-aot-wasm --test emission profile_application`: site maps under every
  profile, true-kind specialisation, byte-identical empty profiles, Collect-mode
  instrumentation; `SCRIPTW_PGO_CORPUS=1` over the CLI fixtures.
- `scriptw-engine --lib embedding::tests::execution_profile`: executed counters
  (main 1, probe body 64, probe read 64), dump kinds, polymorphic and
  megamorphic flags, Collect and every applied profile (true and two
  adversarial seeds) observably identical to the unprofiled build, codec and
  admission round trip against the unprofiled key, distinct cache keys and
  refusals; a true profile's guards must spend less Wasm fuel than the
  unprofiled build, which shows they prove their kind.
- `scriptw-engine --lib tests::execution_profile`: the exactness fixtures. Each
  trains one site, then meets a Proxy receiver, a getter added to the
  prototype, `setPrototypeOf`, `delete`, `defineProperty` (non-writable,
  accessor, non-enumerable), `freeze`/`seal`/`preventExtensions`, a key or
  method added to `Object.prototype`, a `String.prototype` edit or a receiver
  from another Realm; unprofiled, Collect, true-kind and three adversarial
  builds observe the same completion and output.
- RSC host and Test262 subtrees with the hooks above against unprofiled runs.

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
build's P is 2.3% larger, an adversarial one's 20%. Native preparation showed
no measurable change (three alternating runs on a loaded machine: process
user CPU 8.47–8.67 s unprofiled, 8.51–8.76 s applied). A guard for an own
array slot is not emitted where the site's own inline hit already proves it.
