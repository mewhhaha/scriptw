# Runtime and program helper type ownership

Layout ABI 4 introduced separate helper signature and helper body ownership.
The one `RuntimeHelperId` registry still owns every helper's typed operands,
results, and declaration order. Body/function indices keep their existing
Runtime versus Program/Hook ranks. Type indices use a separate closed region:

- Runtime helpers and program hooks belong to the core type region. The shared
  runtime calls hooks through immutable typed reference fields of the selected
  ProgramInstance, so their
  signatures remain part of the runtime contract even though P owns the bodies.
- Direct Program helpers belong to a suffix after the complete core GC graph.
  Runtime modules omit those signatures and bodies. Program and standalone
  modules append them as singleton function types referring back to core types.

The static signature prefix and the core recursion group have identical type
indices and exact type declarations in R and P. The assigned GC base depends
only on static signatures and core helper signatures. Adding or editing a
direct Program helper therefore cannot enlarge the core recursion group or
renumber its GC layouts. Adding or editing a hook signature is an actual runtime
ABI change and must invalidate the runtime.

Registration takes the actual module kind, consumes the private type section,
and finalizes the matching root schema as before. No caller supplies type
counts, helper-index lists, or a mutable section. Both signature declaration and
index use derive from the same exhaustive ownership match and registry rows.

The ABI 3 to 4 migration reindexed the existing core once. Layout version 4
invalidated old raw caches, build packages, and runtime keys coherently. Existing
package, Intl profile, pool, Wasm validation, source and native admission checks
remain authoritative. Tests decode real R/P bytes, compare core groups and
import signatures, validate every actual helper row, and retain hook field
and `call_ref` signatures in the core.

That ownership split does not narrow compiler source fingerprints or introduce a
new trusted native input. Source/build-package identity remains conservative over
the compiler inputs. Exact-Wasm native caches may reuse unchanged R after this
migration; no timing or cache-hit measurement is asserted without a fresh owned
verification run. Extracting a runtime emitter dependency boundary is a
separate change if fresh raw emission and package rewrites need to be avoided.

## ABI 5 legacy-reflection layout

Layout ABI 5 accompanies the legacy-reflection ownership repair. Two appended
FunctionObject fields and the new LegacyReflectionPropertyState record change
the shared GC recursion group. Both R and P must use that same current graph.
Registry-derived type indices, typed constructors, field ordinals and
host/snapshot projections already follow the declarations; no numeric layout
census needs adjustment.

The version participates in raw-cache keys and headers, build-package headers,
and the runtime key over the exact raw R bytes. Old unchanged cache entries and
packages therefore miss before reuse. The full compiler source fingerprint also
covers the changed declarations and new ownership module. Engine disk keys bind
that source identity and the actual loaded executable digest; cached P records
the exact R key. The Cargo producer emits current R before native compilation,
and embedded native admission binds current source, target, configuration,
compilation modes, raw runtime key, package digest and native-image digest.
Wasmtime's own native cache uses exact Wasm and native compilation configuration.

Raw admission checks Wasm validity, exports, data boundaries and admitted Intl
sections; it does not compare every decoded GC type against today's registry.
Re-signing old bytes with forged current identity fields is not an old-schema
rejection proof. Independently, R exports and P imports carry concrete Wasm
types from their GC graphs. Incompatible old-R/new-P function or global types
fail Wasm validation or typed import linking before P's entry executes; native
module loading does not bypass Wasmtime's import checks.

Envelope controls preserve the valid current payload and repair the digest
after changing only the layout header to 4. They require raw-cache eviction and
replacement, build-package fallback, and successful current-entry admission
without another emission. The build-package control reuses a freshly emitted
real R. Required verification also includes exact current R/P type registration,
fresh emission parity, source fingerprinting, native compatibility and actual
mixed-schema typed-link rejection. Forged identity labels do not establish
schema compatibility or rejection.
