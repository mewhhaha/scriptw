# Contract: closed spec name domains — `NativeErrorKind` and `WellKnownSymbol`

## Canonical Symbol expressions — 2026-10-07

The current GC compiler uses `ExprIr::WellKnownSymbol(WellKnownSymbol)` for a
proved intrinsic Symbol read. The backend loads that identity from the original
agent-wide Symbol table. String literals remain strings even when their text
matches a Symbol description. Shape analysis, species descriptors and mutation
publication consume the enum directly. This supersedes every runtime
string-encoding claim in the historical domain audit below; descriptions and
shape namespace strings remain metadata only.

The old `ExprIr::String` payload with a Symbol kind hint produced real strings
after the GC cutover. An iterator's direct key reads could agree with its
mistaken string-keyed write while the native iterator protocol and dynamic
`Reflect.get` correctly looked for the actual Symbol and failed. The affected
IR controls now require the distinct variant. A finite native control covers
all fifteen identities through constant reads, dynamic reads, a created Realm
and simultaneous string properties with the same descriptions.
`tasks-symbol-progress1` passes the whole-workspace type check, all 20 selected
IR controls, native awaited iterator closing and the original mixed Array
lifecycle in strict and sloppy modes. The new catalog control incorrectly used
an unloaded `$262` harness and failed with an object throw; it now uses the actual
`__scriptwCreateRealm` hook. `tasks-constructor-progress1` passes the workspace
type check and the corrected catalog control, including all fifteen actual
Symbol identities and same-description string properties. Earlier domain
results below retain their original scope.

Area: *Closed spec name domains: NativeErrorKind and WellKnownSymbol*
Stage: FORMALIZER. This document is normative for the encoder and is the
oracle the dry-runner checks against. Source code is not edited in this stage.

> **Read §11 first.** A dry-run discrepancy pass amended this document after the
> encoding landed. §11 supersedes every claim it names — including parts of
> §4.3, §4.4, §6.2, §6.3, §10.2 and ledger entries R3/R5/R6. Do not cite
> §§1–10 without checking §11.

Owned files:

- `crates/scriptw-ir/src/native_error.rs` (new)
- `crates/scriptw-ir/src/well_known.rs` (new)
- `docs/rust-rewrite/contracts/closed-name-domains.md` (this file)

Every count in this document was produced by a command, not an estimate.
Where a count in the area brief disagreed with the repository, the measured
value is used and the correction is stated inline. Sections marked
**[dry-run obligation]** are claims the dry-runner must confirm or refute
before the encoder's work is accepted.

---

## 0. Corrections to the area brief, measured

The brief is right about the shape of the problem and wrong about five
numbers and one fact. The encoder must work from the numbers here.

| Brief says | Measured | Command |
|---|---|---|
| `test262/…/built-ins/Symbol/` contains "precisely those 15 subdirectories" | **18** subdirectories: the 15 well-known symbols **plus** `for`, `keyFor`, `prototype` | `ls -d test262/vendor/test262/test/built-ins/Symbol/*/ \| wc -l` |
| 69 `"Symbol.…"` sites in `lowering.rs` | **66** exact well-known-symbol string literals, **+2** bare `"Symbol."` prefix literals = 68 tokens matching `"Symbol\.[A-Za-z]*"` | see §6.3 |
| 334 literals / 36 files workspace-wide | **334** tokens matching `"Symbol\.[A-Za-z]*"`, of which **324** in **34** files are one of the 15 legal spellings; the other 10 are `"Symbol."`×7, `"Symbol.prototype"`, `"Symbol.for"`, `"Symbol.keyFor"` | `grep -rnoE '"Symbol\.(asyncIterator\|…)"' crates/ --include=*.rs \| wc -l` |
| `builtins/standard.rs` 28, `data.rs` 24 | `builtins/standard.rs` **22**, `data.rs` **18** | same |
| "all 16 construction sites in lowering.rs" | **14** `ExprIr::RuntimeThrow { … }` constructions; the other two `RuntimeThrow` tokens are the consumer match at 12106 and a `matches!(…, ExprIr::RuntimeThrow { .. })` at 12409 | `grep -n "RuntimeThrow" crates/scriptw-ir/src/lowering.rs` |
| "SuppressedError … the first `RuntimeThrow` with either name types as base `Error` and every downstream inference is silently wrong" | The defect is **latent and armed, not currently firing.** All 14 construction sites use only `TYPE_ERROR_NAME` (8), `REFERENCE_ERROR_NAME` (3), `RANGE_ERROR_NAME` (3) — all three are handled by the 6-arm match. The wrong arms (`AggregateError`, `SuppressedError`) have no producer yet. `ERROR_NAME` also falls through, but to `ErrorConstructor`, which is correct. | §5.3 |

Two facts the brief did not have, both of which strengthen the case:

- **There are two byte-identical hand-kept 15-element well-known-symbol
  whitelists in `lowering.rs`, not one** — at `34017-34034` (inside
  `try_well_known_symbol_key_name`) and at `28336-28352` (inside the
  property-key lowering path), each preceded by a byte-identical four-clause
  "is this the real builtin `Symbol`?" guard (`34000-34010` and
  `28324-28332`). This is exactly the `intl_object_value_info` /
  `init_intl_object` defect recorded at `names.rs:190-200`, alive today, for
  a different closed set. See §6.2.
- **`is_error_prototype_expr` (`lowering.rs:29157`) — one of the nine-row
  NativeError tables — has zero call sites workspace-wide.** It is one of the
  five hand-kept nine-row lists and it is unreachable from the product path.
  See §5.2 and §8.4.

The brief's claim that `crates/scriptw-aot-wasm/src/builtins/temporal.rs`
(batch-2 excluded) has exactly one `_ERROR_NAME` use is **confirmed**:
`RANGE_ERROR_NAME` at line 6629, and nothing else.

---

## 1. Spec basis

### 1.1 The error-name domain

ECMA-262 closes this domain in three separate clauses, and the distinction
between them is load-bearing for how the Rust type is named.

- **§20.5.5 *Native Error Types Used in This Standard*** enumerates exactly
  six: `EvalError`, `RangeError`, `ReferenceError`, `SyntaxError`,
  `TypeError`, `URIError`. §20.5.6 *NativeError Object Structure* then
  defines all six by a single parametric template: for each, there is a
  constructor `%NativeError%`, a prototype `%NativeError.prototype%`, and
  instances carrying `[[ErrorData]]`. The spec's word *NativeError* means
  these six and only these six.
- **§20.5.1–20.5.4** define `Error` itself. `Error` is **not** a NativeError:
  it is the shared superclass whose prototype the six inherit
  (§20.5.6.3: "The *NativeError* prototype object … has a [[Prototype]]
  internal slot whose value is %Error.prototype%").
- **§20.5.7 *AggregateError Objects*** defines `AggregateError` separately,
  with an extra `errors` argument and an `[[AggregateErrors]]`-shaped
  behaviour. It is not produced by the §20.5.6 template.
- **`SuppressedError`** arrives with Explicit Resource Management (ES2026
  numbering: §20.5.8). It, too, is defined by its own clause, with `error`
  and `suppressed` arguments, and is not a NativeError under §20.5.5.

**Closure argument.** The set is closed because §20.5.5 is an enumeration
rather than an extension point, and because every intrinsic that ECMA-262
defines with `[[ErrorData]]` is introduced by one of these four clauses. No
clause of ECMA-262 admits a further error intrinsic. Nine, exactly:

```
Error, EvalError, RangeError, ReferenceError, SyntaxError, TypeError,
URIError, AggregateError, SuppressedError
```

**Invariants this contract carries.**

- **E1 (closure).** The nine spellings above are the complete domain of error
  intrinsic names in this compiler. There is no tenth.
- **E2 (spelling identity).** The `name` observable on the prototype
  (`%NativeError.prototype%.name`, §20.5.6.3.2), the `Error.prototype.toString`
  prefix (§20.5.3.4 step 3), the global binding name (§19.3), and the internal
  identifier this compiler uses to select a prototype must all be the *same*
  string. The spec guarantees this by construction; an implementation that
  spells them separately can drift.
- **E3 (bijection to constructor).** Each of the nine names denotes exactly
  one intrinsic constructor, and each error constructor has exactly one name.
  In this repository the constructor side is `StandardBuiltinId::{Error,
  EvalError, AggregateError, SuppressedError, RangeError, SyntaxError,
  TypeError, URIError, ReferenceError}Constructor` — nine variants, verified
  at `crates/scriptw-ir/src/builtins.rs:1393-1402`.
- **E4 (prototype totality).** For every one of the nine, `%X.prototype%`
  exists and is distinct. Consequently a name-to-prototype map must be
  **total on the domain**. A partial map with a fallback is a spec violation
  whenever the fallback is taken: §20.5.6.2 requires a thrown *NativeError*
  instance's [[Prototype]] to be `%NativeError.prototype%`, so falling back to
  `%Object.prototype%` produces a value for which
  `e instanceof TypeError` is `false` and `e.message` is `undefined`.
- **E5 (subset predicate).** The six-element §20.5.5 subset is itself
  spec-meaningful: it is the set for which the §20.5.6 template applies
  uniformly (same constructor arity, same `[[Prototype]]` chain, no extra
  arguments). Code that generalises over "all native errors" must be able to
  say which six it means.

**Implementation latitude and the choice made.** ECMA-262 does not say how an
implementation names this nine-element union — it has no name for it, because
it has no need of one. This contract adopts the area's chosen name
`NativeErrorKind` for the nine, *and* carries E5 as an explicit
`is_native_error()` predicate that is true for exactly the six of §20.5.5.
The name is a mild abuse; the predicate is what stops it becoming a
misconception. The doc comment on the type states both facts.

### 1.2 The well-known-symbol domain

**§6.1.5 *The Symbol Type*, §6.1.5.1 *Well-Known Symbols*, Table 1.** The
spec's own words: "Well-known symbols are built-in Symbol values that are
explicitly referenced by algorithms of this specification. They are typically
used as the keys of properties whose values serve as extension points of a
specification algorithm." Table 1 lists thirteen rows in ES2024:

```
@@asyncIterator @@hasInstance @@isConcatSpreadable @@iterator @@match
@@matchAll @@replace @@search @@species @@split @@toPrimitive
@@toStringTag @@unscopables
```

Explicit Resource Management adds two more rows to the same table:
`@@dispose` and `@@asyncDispose`. Fifteen, exactly. (Recent editions spell
these `%Symbol.iterator%` rather than `@@iterator`; the two notations denote
the same values. This repository already uses `@@` as a *shape-map* prefix
for an unrelated purpose — see §6.4 — so this document uses `@@name` only
when quoting the spec, and never as an implementation spelling.)

Each row of Table 1 has two columns that matter here:

| Table 1 column | Example | This compiler's use |
|---|---|---|
| **Specification Name** | `@@iterator` | selects the variant |
| **[[Description]]** | `"Symbol.iterator"` | is the *runtime value encoding* — see §6.1 |

And a third string exists that Table 1 does not have a column for: the
**member name** on the `Symbol` intrinsic. §19.4.2 defines `Symbol.iterator`
as a property of the `Symbol` constructor whose key is the string
`"iterator"` and whose value is `@@iterator`. So each well-known symbol has
three distinct strings in this codebase, and confusing them is a defect
class in its own right (§6.4).

**Closure argument.** Table 1 is exhaustive by construction: §6.1.5.1's
normative content *is* the table, and every algorithm that reads a
well-known symbol names a Table 1 row. Registry symbols (`Symbol.for` /
`Symbol.keyFor`, §19.4.2.2/19.4.2.6) are a *different, open* domain — the
GlobalSymbolRegistry admits arbitrary string keys — and are explicitly
outside this contract. `Symbol.prototype` is a property of the constructor,
not a symbol. That is why `built-ins/Symbol/` has 18 subdirectories and not 15.

**Invariants this contract carries.**

- **S1 (closure).** Fifteen, exactly, partitioned as 13 (Table 1, ES2024) +
  2 (Explicit Resource Management).
- **S2 (three-string coherence).** For each row: `description ==
  "Symbol." ++ member_name`. This is not a coincidence of the spec text; it
  is stated by Table 1's [[Description]] column matching §19.4.2's property
  key for every row. A tool that spells them independently can drift.
- **S3 (producer/consumer identity).** The value a producer emits for a
  well-known symbol and the value a consumer compares against must be
  the *same* value. In a string encoding this is unenforced.
- **S4 (symbol keys are not string keys).** §6.1.5: symbols are a distinct
  primitive type; a property keyed by `@@iterator` is a different property
  from one keyed by the string `"Symbol.iterator"`. `Object.getOwnPropertyNames`
  never reports the former; `Object.getOwnPropertySymbols` never reports the
  latter. **This compiler encodes well-known symbol values as strings**
  (§6.1), which makes S4 an invariant the encoding must actively defend
  rather than one it gets for free.
- **S5 (extension-point totality).** Each well-known symbol is read by
  named abstract operations with fixed ordering obligations, and a
  specialization keyed on the wrong spelling silently disables the extension
  point rather than erroring:

  | Symbol | Read by | Consequence of a missed match |
  |---|---|---|
  | `@@toPrimitive` | §7.1.1 ToPrimitive step 2 (`GetMethod(input, @@toPrimitive)`) | falls to OrdinaryToPrimitive; user hook never runs |
  | `@@iterator` | §7.4.3 GetIterator (sync) | wrong or absent iteration protocol |
  | `@@asyncIterator` | §7.4.3 GetIterator (async), tried **before** `@@iterator` | ordering obligation: async first, then sync-wrapped |
  | `@@species` | §7.3 SpeciesConstructor | wrong constructor for derived objects |
  | `@@hasInstance` | §13.10.2 InstanceofOperator step 2 | `instanceof` bypasses the user hook |
  | `@@isConcatSpreadable` | §23.1.3.1.1 IsConcatSpreadable | `concat` spreads (or fails to) wrongly |
  | `@@match @@matchAll @@replace @@search @@split` | §22.1.3.x `String.prototype.*` | falls back to the non-RegExp path |
  | `@@toStringTag` | §20.1.3.6 `Object.prototype.toString` step 15 | `[object Object]` instead of the tag |
  | `@@unscopables` | §9.1.1.2.1 (object Environment Record `HasBinding`) | `with` resolves a binding it must not |
  | `@@dispose @@asyncDispose` | Explicit Resource Management, `using` / `await using` | resource not disposed |

  Every entry in that column is a **silent wrong answer**, never a
  diagnostic. That is the whole reason this area exists.

---

## 2. The two types

Both types live in new files in `crates/scriptw-ir`, are re-exported through
the crate's existing `pub use names::*;`-style surface, and are generated
from a **single row list each** by a declarative macro. The macro is not
decoration: it is what makes "add to one table and not the other"
*unrepresentable* rather than merely detectable, which is a strictly
stronger property than the `INTL_NAMESPACE_CONSTRUCTORS` const-slice pattern
at `names.rs:206` achieved.

### 2.1 `crates/scriptw-ir/src/native_error.rs`

```rust
//! The closed domain of ECMA-262 error intrinsic names.
//!
//! ECMA-262 §20.5.5 *Native Error Types Used in This Standard* enumerates
//! exactly six NativeErrors. `Error` (§20.5.1), `AggregateError` (§20.5.7)
//! and `SuppressedError` (Explicit Resource Management) are defined by their
//! own clauses and are NOT NativeErrors under §20.5.5. This enum names the
//! nine-element union, for which ECMA-262 has no name because it has no need
//! of one; `is_native_error()` recovers the spec-exact six.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum NativeErrorKind { /* generated */ }
```

Generated from one row list, in this order (E5 grouping is explicit):

| Variant | `as_str()` | `constructor()` | `is_native_error()` |
|---|---|---|---|
| `Error` | `"Error"` | `StandardBuiltinId::ErrorConstructor` | `false` |
| `EvalError` | `"EvalError"` | `EvalErrorConstructor` | `true` |
| `RangeError` | `"RangeError"` | `RangeErrorConstructor` | `true` |
| `ReferenceError` | `"ReferenceError"` | `ReferenceErrorConstructor` | `true` |
| `SyntaxError` | `"SyntaxError"` | `SyntaxErrorConstructor` | `true` |
| `TypeError` | `"TypeError"` | `TypeErrorConstructor` | `true` |
| `URIError` | `"URIError"` | `URIErrorConstructor` | `true` |
| `AggregateError` | `"AggregateError"` | `AggregateErrorConstructor` | `false` |
| `SuppressedError` | `"SuppressedError"` | `SuppressedErrorConstructor` | `false` |

Surface, all generated, all `const fn` where the body allows:

```rust
impl NativeErrorKind {
    pub const ALL: [NativeErrorKind; 9];
    /// §20.5.5 — the six for which the §20.5.6 template applies.
    pub const NATIVE_ERRORS: [NativeErrorKind; 6];

    /// The single spelling authority (invariant E2).
    pub const fn as_str(self) -> &'static str;
    /// The only parse. Total on the domain, `None` off it.
    pub const fn from_str(name: &str) -> Option<Self>;
    /// Invariant E3, forward direction.
    pub const fn constructor(self) -> StandardBuiltinId;
    /// Invariant E3, reverse direction.
    pub const fn from_constructor(id: StandardBuiltinId) -> Option<Self>;
    /// Invariant E5.
    pub const fn is_native_error(self) -> bool;
}
```

Deliberately absent, and the encoder must not add them:

- `impl Display`, `impl AsRef<str>`, `impl Deref<Target = str>`,
  `impl From<NativeErrorKind> for String`. A stringification must name
  `as_str()` at the call site. This is what stops `format!("{kind}")` from
  quietly reintroducing the `&str` domain.
- `impl FromStr`. `from_str` is a const inherent fn returning `Option`, not
  a trait method returning `Result<_, Infallible>`-shaped noise.
- Any `Default`. There is no default error kind.

Const assertions, all `const _: () = assert!(…);` at module scope:

| # | Assertion | Catches |
|---|---|---|
| N1 | `ALL.len() == 9` | the domain drifting from §20.5.5+20.5.1+20.5.7+ERM |
| N2 | `NATIVE_ERRORS.len() == 6` | E5 drifting |
| N3 | `∀ i: ALL[i] as u8 == i as u8` | `ALL` out of order or short |
| N4 | `∀ k ∈ ALL: from_str(k.as_str()) == Some(k)` | parse/print divergence |
| N5 | `∀ i<j: !str_eq(ALL[i].as_str(), ALL[j].as_str())` | two variants sharing a spelling |
| N6 | `∀ k ∈ ALL: from_constructor(k.constructor()) == Some(k)` | E3; round-trip proves `constructor` is injective on `ALL` |
| N7 | `(count of k ∈ ALL with is_native_error()) == 6` and `∀ k ∈ NATIVE_ERRORS: k.is_native_error()` | E5 |
| N8 | nine `str_eq(<X>_ERROR_NAME, NativeErrorKind::X.as_str())` ties — **only in the fallback of §4.2** | the deferred consumer lane drifting |

`str_eq` is a private `const fn` in `native_error.rs` using
`str::as_bytes()` and a `while` loop; both are const-stable. `well_known.rs`
gets its own copy rather than a shared `pub` helper, because a `pub const fn
str_eq` in a public module is exactly the kind of no-call-site-in-product
surface AGENTS.md warns about.

### 2.2 `crates/scriptw-ir/src/well_known.rs`

```rust
//! ECMA-262 §6.1.5.1 Table 1, *Well-Known Symbols*, plus the two rows added
//! by Explicit Resource Management. Thirteen plus two, exactly fifteen.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum WellKnownSymbol { /* generated */ }
```

Row list, in Table 1 order for the thirteen, then the two additions:

| Variant | `member_name()` | `description()` | group |
|---|---|---|---|
| `AsyncIterator` | `"asyncIterator"` | `"Symbol.asyncIterator"` | Table 1 |
| `HasInstance` | `"hasInstance"` | `"Symbol.hasInstance"` | Table 1 |
| `IsConcatSpreadable` | `"isConcatSpreadable"` | `"Symbol.isConcatSpreadable"` | Table 1 |
| `Iterator` | `"iterator"` | `"Symbol.iterator"` | Table 1 |
| `Match` | `"match"` | `"Symbol.match"` | Table 1 |
| `MatchAll` | `"matchAll"` | `"Symbol.matchAll"` | Table 1 |
| `Replace` | `"replace"` | `"Symbol.replace"` | Table 1 |
| `Search` | `"search"` | `"Symbol.search"` | Table 1 |
| `Species` | `"species"` | `"Symbol.species"` | Table 1 |
| `Split` | `"split"` | `"Symbol.split"` | Table 1 |
| `ToPrimitive` | `"toPrimitive"` | `"Symbol.toPrimitive"` | Table 1 |
| `ToStringTag` | `"toStringTag"` | `"Symbol.toStringTag"` | Table 1 |
| `Unscopables` | `"unscopables"` | `"Symbol.unscopables"` | Table 1 |
| `Dispose` | `"dispose"` | `"Symbol.dispose"` | ERM |
| `AsyncDispose` | `"asyncDispose"` | `"Symbol.asyncDispose"` | ERM |

Surface:

```rust
pub const SYMBOL_DESCRIPTION_PREFIX: &str = "Symbol.";

impl WellKnownSymbol {
    pub const ALL: [WellKnownSymbol; 15];
    /// §6.1.5.1 Table 1 as of ES2024.
    pub const TABLE_1: [WellKnownSymbol; 13];
    /// Explicit Resource Management's two additions to the same table.
    pub const EXPLICIT_RESOURCE_MANAGEMENT: [WellKnownSymbol; 2];

    /// The key of the corresponding property of the `Symbol` intrinsic
    /// (§19.4.2), e.g. `"iterator"` for `Symbol.iterator`.
    pub const fn member_name(self) -> &'static str;
    /// Table 1's [[Description]] column, e.g. `"Symbol.iterator"`. This is
    /// also this compiler's runtime string encoding of the symbol value —
    /// see the contract, §6.1.
    pub const fn description(self) -> &'static str;

    /// THE ONLY CONSTRUCTOR from source text. Replaces both hand-kept
    /// `matches!` whitelists.
    pub const fn from_member_name(name: &str) -> Option<Self>;
    /// Recovers the symbol from its runtime string encoding.
    pub const fn from_description(description: &str) -> Option<Self>;
}

/// The shape-map key for a symbol-keyed entry: `"@@" ++ description()`.
/// Not `const fn` — it allocates. See contract §6.4 for why this is a
/// separate name from `description()` and must stay one.
pub fn shape_namespace_key(symbol: WellKnownSymbol) -> String;

/// Whether `name` lies in this compiler's symbol-value string namespace.
/// OPEN predicate over an OPEN domain — see the runtime-checked ledger,
/// entry R2. Not derivable from `WellKnownSymbol`.
pub fn is_symbol_description(name: &str) -> bool;
```

Deliberately absent, same reasoning as §2.1: `Display`, `AsRef<str>`,
`Deref`, `FromStr`, `Default`, `From<WellKnownSymbol> for String`. With
three distinct strings per variant, an implicit stringification is not
merely loose, it is *ambiguous*; every call site must name which one.

Const assertions:

| # | Assertion | Catches |
|---|---|---|
| W1 | `ALL.len() == 15` | the closed set drifting |
| W2 | `TABLE_1.len() == 13 && EXPLICIT_RESOURCE_MANAGEMENT.len() == 2 && ALL.len() == TABLE_1.len() + EXPLICIT_RESOURCE_MANAGEMENT.len()` | the 13+2 partition, with the two extras named |
| W3 | `∀ i: ALL[i] as u8 == i as u8` | `ALL` out of order or short |
| W4 | `∀ k ∈ ALL: from_member_name(k.member_name()) == Some(k)` | parse/print divergence on the member name |
| W5 | `∀ k ∈ ALL: from_description(k.description()) == Some(k)` | parse/print divergence on the description |
| W6 | `∀ k ∈ ALL: str_eq(k.description(), concat!("Symbol.", k.member_name()))` — implemented as a const prefix check plus a const suffix compare against `member_name()` | **S2**: the two strings drifting apart |
| W7 | `∀ i<j: !str_eq(ALL[i].member_name(), ALL[j].member_name())` | two variants sharing a spelling |
| W8 | `∀ k ∈ ALL: is_symbol_description(k.description())` — as a `const fn` prefix check | the namespace predicate and the encoding disagreeing |

W6 is the one that would have caught the whole `"Symbol.toStringtag"` class
at its root, and it is checkable at compile time because both strings come
from the same row.

---

## 3. Type mapping: invariant → construct

| Invariant | Rust construct | Why it holds |
|---|---|---|
| **E1** closure at nine | `enum NativeErrorKind`, macro-generated from one row list | a tenth requires editing the row list, which regenerates every table at once |
| **E2** one spelling | `const fn as_str`, the only place a name literal appears in `scriptw-ir`; the nine `*_ERROR_NAME` consts defined *from* it (§4.2) | there is one literal per name in the crate |
| **E3** name↔constructor bijection | `constructor` / `from_constructor` + const assert **N6** | round-trip in `const` proves injectivity on `ALL` |
| **E4** prototype map totality | exhaustive `match` on `NativeErrorKind` with **no catch-all** at `lowering.rs:12106` | omitting an arm is `E0004` |
| **E5** the spec-exact six | `is_native_error()` + `NATIVE_ERRORS` + const asserts **N2/N7** | the six cannot silently become five or seven |
| **S1** closure at fifteen | `enum WellKnownSymbol`, one row list | as E1 |
| **S2** `description == "Symbol." ++ member_name` | const assert **W6** | drift is a build failure |
| **S3** producer/consumer identity | `try_well_known_symbol_key_name` returns `Option<WellKnownSymbol>`; consumers `match`/`==` on the enum | a consumer naming a value no producer emits is `E0599`/`E0433` |
| **S5** extension-point totality | consumer `match`es over `WellKnownSymbol` in the retrofit (§6.3) are exhaustive where the code genuinely covers the domain, and use a documented `_ =>` only where it does not | see runtime-checked ledger **R3** |
| 13+2 partition named | `TABLE_1`, `EXPLICIT_RESOURCE_MANAGEMENT`, const assert **W2** | the two ERM rows are named in the type, not in a comment |

### 3.1 Runtime-checked ledger

These are the only places a test remains load-bearing. Each entry states why
a type cannot carry the invariant. The encoder must not "fix" these by
inventing a type; the dry-runner must confirm each reason still holds.

| # | Invariant | Where | Why no type carries it | What must check it |
|---|---|---|---|---|
| **R1** | **S4**, symbol keys are not string keys. | `ObjectShape.properties: BTreeMap<String, ObjectShapeProperty>` (`ir.rs:440`), `ArrayShape.properties` (`ir.rs:448`) | The key domain is *all* JS property keys — an open, unbounded set of arbitrary strings. Well-known symbols are an encoded subset of it. Retyping the map key to a closed enum is false; retyping it to an `enum { String(String), WellKnown(WellKnownSymbol) }` is a distinct, much larger refactor with observable ordering consequences (`BTreeMap` iteration order feeds shape comparison), and is **out of scope**. The type carries *construction* (`description()`, `shape_namespace_key()`), not the key type. | existing lowering tests, notably `crates/scriptw-ir/src/lib.rs:4497-4695`, which assert that a JS object with the *string* key `"Symbol.iterator"` is not treated as symbol-keyed |
| **R2** | `is_symbol_description(&str)` is total over an open domain. | `lowering.rs:29262`, `lowering.rs:35151` | Both sites test whether an arbitrary lowered `ExprIr::String` lies in the symbol-value namespace. The input is a `String` from the shape/IR layer, not a `WellKnownSymbol`. A prefix test is the honest operation. It is *guarded* at both sites by `lowered.kind == ValueKind::Symbol`, which is the real invariant — but `ValueKind` is not parameterised by the string. | §7 trace T7; plus the `debug_assert!` specified in §6.5 |
| **R3** | Consumer `match`es over `WellKnownSymbol` that are genuinely partial. | `lowering.rs:20986-21004` (five specialization arms keyed on receiver kind) | This match answers "does this receiver+symbol pair have a static fast path?" Its domain is `WellKnownSymbol × ValueKind`, and most cells legitimately have no fast path. Forcing exhaustiveness here would mean writing ten `=> None` arms that carry no information and that a future symbol addition would mechanically extend without thought — decoration, by AGENTS.md's own test. It keeps a `_ => None` arm, **but** the arm gets a comment naming the symbols that deliberately have no fast path, and the `WellKnownSymbol` type ensures the listed arms name real variants. | rung 1 `cargo test -p scriptw-ir`; the CLI area suites for iterator/regexp |
| **R4 (discharged)** | The former `error_prototype_global_index` / `error_realm_prototype_offset` / `error_realm_prototype_entries` / `operations.rs` string tables in `crates/scriptw-aot-wasm`. | `module/error_prototype_location.rs`, `operations.rs`, `builtins/errors.rs`; see §13 | T24 replaced them with one exhaustive `NativeErrorKind -> ErrorPrototypeLocation` authority. Realm entries derive from `NativeErrorKind::ALL`; the `instanceof` fast path consumes the same typed mapping; raw emitter names are validated before either lookup. There is no `%Object.prototype%` or `None` catch-all left. | `cargo check -p scriptw-aot-wasm --lib`, then the focused all-nine realm/prototype contract named in §13. |

---

## 4. Retrofit map: `NativeErrorKind`

### 4.1 Order of operations

Strictly this order. Each step leaves the tree compiling, so
`cargo check -p scriptw-ir` after each is a real gate.

1. Add `crates/scriptw-ir/src/native_error.rs`; `mod native_error;` and
   `pub use native_error::*;` in `crates/scriptw-ir/src/lib.rs` alongside the
   existing `mod names;` / `pub use names::*;` (lib.rs:64, 109). Nothing
   consumes it yet. **`cargo check -p scriptw-ir` here validates every const
   assertion N1–N7 before any call site depends on the type.**
2. Redefine the nine `*_ERROR_NAME` consts (§4.2). Everything still compiles;
   this is the step whose risk is spelled out below.
3. Retype `ExprIr::RuntimeThrow.name` (§4.3) and fix all consumers in the
   same edit — this step does *not* leave the tree compiling in between.
4. Route the four complete nine-row lists in `lowering.rs` through
   `NativeErrorKind::ALL` (§4.4).
5. Delete `is_error_prototype_expr` (§4.4, item 3).

### 4.2 `names.rs:241-249` — kept, redefined

The nine consts stay, because `crates/scriptw-aot-wasm/src/builtins/temporal.rs`
is on the batch-2 exclusion list, uses `RANGE_ERROR_NAME` at line 6629, and
must continue to compile **with zero edits**. Eight other out-of-scope files
also import them (`module.rs`, `operations.rs`, `data.rs`, `lib.rs`,
`builtins/bootstrap.rs`, `builtins/host.rs` in `scriptw-aot-wasm`;
`builtins.rs` in `scriptw-ir`).

**Primary form** — makes the enum the single spelling authority even for the
deferred lane:

```rust
pub const ERROR_NAME: &str = NativeErrorKind::Error.as_str();
pub const EVAL_ERROR_NAME: &str = NativeErrorKind::EvalError.as_str();
// … nine total, replacing names.rs:241-249 verbatim in place
```

**The one risk in this whole area, stated plainly.** These consts are used as
**match patterns** — `module.rs:1731`, `operations.rs:142`,
`lowering.rs:10759`, `lowering.rs:20725`. Pattern legality for a `const`
item depends on its *type* being structural-match, not on how it was
computed, and `&'static str` is structural-match. The primary form is
therefore expected to work. It has **not** been compiled in this stage.

**Fallback, if `cargo check -p scriptw-aot-wasm` rejects the primary form
for any reason:** revert the nine consts to their literals exactly as they
stand today, and add the nine **N8** const assertions
`const _: () = assert!(str_eq(TYPE_ERROR_NAME, NativeErrorKind::TypeError.as_str()));`
in `native_error.rs`. The tie is then checked rather than structural — one
notch weaker, still a compile error on drift. Choose the fallback on the
first compiler complaint; do not spend effort defending the primary form.

**Do not** change any of the nine spellings, and do not reorder them.

### 4.3 `ir.rs:1560` — the retype

```rust
    RuntimeThrow {
        name: NativeErrorKind,
        message: &'static str,
    },
```

`NativeErrorKind` is a fieldless `#[derive(PartialEq, Eq)]` enum, so
`ExprIr::RuntimeThrow { name: NativeErrorKind::ReferenceError, .. }` is a
legal pattern. Every site below is mechanical.

**14 construction sites in `lowering.rs`** — `TYPE_ERROR_NAME` →
`NativeErrorKind::TypeError`, etc.:

| Line | Today | Becomes |
|---|---|---|
| 16465 | `TYPE_ERROR_NAME` | `NativeErrorKind::TypeError` |
| 16531 | `TYPE_ERROR_NAME` | `NativeErrorKind::TypeError` |
| 16965 | `REFERENCE_ERROR_NAME` | `NativeErrorKind::ReferenceError` |
| 17091 | `REFERENCE_ERROR_NAME` | `NativeErrorKind::ReferenceError` |
| 20521 | `RANGE_ERROR_NAME` | `NativeErrorKind::RangeError` |
| 20548 | `RANGE_ERROR_NAME` | `NativeErrorKind::RangeError` |
| 20606 | `RANGE_ERROR_NAME` | `NativeErrorKind::RangeError` |
| 21216 | `TYPE_ERROR_NAME` | `NativeErrorKind::TypeError` |
| 23938 | `TYPE_ERROR_NAME` | `NativeErrorKind::TypeError` |
| 23957 | `TYPE_ERROR_NAME` | `NativeErrorKind::TypeError` |
| 30899 | `REFERENCE_ERROR_NAME` | `NativeErrorKind::ReferenceError` |
| 30918 | `TYPE_ERROR_NAME` | `NativeErrorKind::TypeError` |
| 30945 | `TYPE_ERROR_NAME` | `NativeErrorKind::TypeError` |
| 32687 | `TYPE_ERROR_NAME` | `NativeErrorKind::TypeError` |

**The consumer that is the point of the exercise**, `lowering.rs:12106-12115`
inside `infer_expr_throw_info`. Today six arms and `_ =>
StandardBuiltinId::ErrorConstructor`. It becomes:

```rust
            ExprIr::RuntimeThrow { name, .. } => {
                Some(Self::standard_error_instance_info(name.constructor()))
            }
```

Note what this does: it does not *write* a nine-arm match, it *deletes* the
match and defers to `NativeErrorKind::constructor()`, which is the
macro-generated total function whose totality is guaranteed by E3/N6. That
is strictly better than a nine-arm match at this call site, because a
tenth error kind then cannot be omitted here at all. The exhaustiveness
obligation moves to the row list, where it belongs. `AggregateError` and
`SuppressedError` become correct by construction, and the `Error` case —
which fell through to `ErrorConstructor` and was *right* to — stays right.

**Consumers in `crates/scriptw-aot-wasm`.** The retype cannot be confined to
`scriptw-ir`: `ExprIr` is the backend's input. Three edits, none of them on
the batch-2 exclusion list (`intl_datetimeformat.rs`, `temporal*.rs`,
`emitted_function.rs`, `runtime_helpers.rs`):

| File:line | Today | Becomes |
|---|---|---|
| `data.rs:3198` | `self.intern_string(name);` | `self.intern_string(name.as_str());` |
| `expressions.rs:1256` | `self.emit_throw_runtime_error(name, …)` | `…(name.as_str(), …)` |
| `expressions.rs:3025` | `self.emit_throw_runtime_error(name, message, …)` | `…(name.as_str(), message, …)` |

`emit_throw_runtime_error` (`builtins/errors.rs:476`) keeps its `name: &str`
parameter — it is on the deferred-lane side of the boundary, and widening it
to `NativeErrorKind` would pull `error_prototype_global_index` in with it.
That was R4, deferred deliberately at this stage and discharged later by §13.

Untouched, because they bind `{ .. }`: `planning.rs:2938, 3503, 4882, 6044,
6707, 6888, 7920`; `early_errors.rs:267`; `ir.rs:3079`;
`lowering.rs:12409`; `lib.rs:2797, 10832`.

**12 test patterns in `crates/scriptw-ir/src/lib.rs`** — `lib.rs:5022, 7151,
7209, 7276, 8164, 8259, 8319, 9168, 10860, 10899, 10934` bind
`name: REFERENCE_ERROR_NAME` or `name: TYPE_ERROR_NAME` inside `matches!`;
each becomes `name: NativeErrorKind::ReferenceError` /
`NativeErrorKind::TypeError`. These are tests, so they are edits of
convenience; but note that this is exactly the case where a `&str` const in
a pattern silently matched and an enum variant in a pattern must be spelled
correctly or fail to compile.

### 4.4 The four remaining nine-row lists in `scriptw-ir`

The brief counted five NativeError tables (one in `lowering.rs`, four in
`scriptw-aot-wasm`). There are four *more*, all inside `lowering.rs`, all
currently complete, all hand-kept, and all of them route through
`NativeErrorKind::ALL` after this change:

1. **`lowering.rs:10758-10767`**, `for_in_known_empty_target` — a nine-arm
   `matches!` or-pattern over `name.as_str()`. Becomes
   `NativeErrorKind::from_str(name.as_str()).is_some()`.
2. **`lowering.rs:20724-20733`**, the `propertyIsEnumerable("prototype")`
   constant-fold guard — the same nine-arm or-pattern. Same replacement.
3. **`lowering.rs:29157-29171`**, `is_error_prototype_expr` — a nine-element
   array. **This function has zero call sites in the entire workspace**
   (`grep -rn is_error_prototype_expr crates/ --include=*.rs` → one hit, the
   definition). AGENTS.md: "If something is unreachable from the product
   path, that should fail to build." **Delete it.** Do not migrate it. If
   the encoder finds a caller this measurement missed, migrate it as item 4
   instead and record the correction. **[dry-run obligation D6]**
4. **`lowering.rs:29173-29187`**, `is_error_constructor_expr` — a
   nine-element array, one caller (`lowering.rs:29473`, the `.stack` path).
   Becomes
   `NativeErrorKind::ALL.iter().any(|k| self.is_builtin_reference_expr(expr, k.as_str()))`.

**Untouched in `crates/scriptw-ir/src/builtins.rs`.** The four
`StandardBuiltinId` → name mappings at `1497-1498`, `1796-1801`,
`3176-3183`, `8185-8192` are already exhaustive `match`es over
`StandardBuiltinId`; they carry their own compile-time obligation and are
keyed on the constructor, not the name. Leave them. They will collapse into
one row when the descriptor-table work in `batch-workflow.md` lands.

---

## 5. What the `NativeErrorKind` change does and does not fix

### 5.1 Fixed, at compile time

Adding a tenth error intrinsic is now a build failure in the row list's
generated `as_str`, `from_str`, `constructor`, `is_native_error` — and
therefore in every table derived from them — rather than a silent fall
through `_ => StandardBuiltinId::ErrorConstructor`.

### 5.2 Fixed, by deletion

`is_error_prototype_expr` stops being a nine-row table nobody reads.

### 5.3 The live gap, stated exactly

`infer_expr_throw_info` today matches six of nine and falls through. Of the
three that fall through, one (`Error`) falls through *correctly*.
`AggregateError` and `SuppressedError` fall through *incorrectly* — they
would be typed as base `Error`, and every downstream `instanceof` and shape
inference keyed on the result would be wrong.

**But no lowering site constructs either today.** All 14 `RuntimeThrow`
constructions use `TypeError` (8), `ReferenceError` (3), `RangeError` (3).
So the defect is **latent and armed**: it fires the first time any lowering
site emits `RuntimeThrow` with `AggregateError` or `SuppressedError`, which
is a plausible near-term edit because `using` declarations have already
landed and `SuppressedError` already exists as an intrinsic (`names.rs:244`,
`StandardBuiltinId::SuppressedErrorConstructor` at `builtins.rs:1397`).

The encoder and the dry-runner must both state it this way. Claiming a
currently-observable wrong answer would be false, and the contract is not
improved by overstating it: "a `_ =>` arm that is wrong for two of its
members and has no producer yet" is exactly the omission AGENTS.md describes
as shipped-here-before.

### 5.4 Consumer side fixed by the T24 follow-up

Section 13 discharges R4. A hand-written `&str` still exists at the broad
emitter API because hundreds of callers supply error-name constants, but it is
now validated exactly once into `NativeErrorKind` before any prototype lookup.
The lookup itself is total on that closed domain and has no fallback. An
unrecognised spelling therefore becomes an internal `EmitError`; it cannot
silently allocate an object with `%Object.prototype%`.

---

## 6. Retrofit map: `WellKnownSymbol`

### 6.1 The encoding, established by reading the code

This compiler represents a well-known symbol **value** as an
`ExprIr::String(description)` carrying `ValueKind::Symbol` — for example
`ExprIr::String("Symbol.iterator")` with `kind == ValueKind::Symbol`
(`lowering.rs:28353-28356`, `lowering.rs:17151-17152`). The `ValueKind` is
what distinguishes it from an ordinary string; the text is the
[[Description]] of §6.1.5.1 Table 1. `crates/scriptw-aot-wasm/src/data.rs`
interns that text.

The contract therefore does **not** replace the string encoding. It replaces
every place a *program* writes or reads that string by hand.

### 6.2 The two whitelists — the core defect

There are **two** hand-kept fifteen-element `matches!` whitelists over the
member names, byte-identical to each other, ~5,700 lines apart:

- `lowering.rs:34017-34034`, inside `try_well_known_symbol_key_name`
  (`fn` at 33994; the fifteen spellings at 34019-34033)
- `lowering.rs:28335-28352`, inside the property-key lowering path

Each is preceded by a **byte-identical four-clause guard** answering "is this
identifier the real builtin `Symbol`?" — `lowering.rs:34002-34012` and
`lowering.rs:28324-28332` — testing `target_name == "Symbol"`,
`active_with_objects.is_empty()`, `lookup_binding(..).is_none()`, and
`lookup_global_property_info(..)` proven-present-and-Builtin.

This is precisely the defect shape the repository already documented at
`names.rs:190-200`: two hand-maintained lists of one closed set, which had
already drifted (`DateTimeFormat` in the shape and not the installer), and
whose observable symptom was `intl402/DateTimeFormat/prop-desc.js`. The same
shape, for a different closed set, is live in `lowering.rs` today.

**Both are replaced by `WellKnownSymbol::from_member_name`, and the duplicated
guard becomes one private helper** — call it
`fn expression_is_builtin_symbol_intrinsic(&self, target_name: &str) -> bool`
in `lowering.rs` — used by both sites. Neither list survives.

Signature changes:

```rust
fn try_well_known_symbol_key_name(&self, expr: &Expression) -> Option<WellKnownSymbol>
fn lower_well_known_symbol_property_key(&mut self, expr: &Expression)
    -> Option<(WellKnownSymbol, PropertyKeyIr)>
fn optional_chain_well_known_symbol_property_info(
    &self, receiver: &ValueInfo, key: WellKnownSymbol) -> Option<ValueInfo>
```

The `format!("Symbol.{symbol_name}")` / `Option<String>` round trip
(`lowering.rs:34037`) disappears: `description()` is a `&'static str` and
allocates nothing. The per-query `String` allocation at every one of the
10 `try_well_known_symbol_key_name` call sites goes with it.

### 6.3 The 66 sites in `lowering.rs`, classified

Every one was read. Counts sum to 66.

| Class | Count | Lines | Becomes |
|---|---|---|---|
| **Shape-table producer** — `properties.insert("Symbol.X".to_string(), …)` into `BTreeMap<String, ObjectShapeProperty>` | 42 | 737, 802, 1022, 1206, 1243, 1291, 1559, 1636, 1756, 1956, 2085, 2171, 2272, 2469, 2632, 2725, 2732, 2739, 2746, 2753, 2835, 2905, 2922, 3007, 3219, 3270, 3483, 3507, 3556, 3946, 3974, 4212, 4390, 4396, 4402, 4503, 4602, 4831, 4861, 4943, 4971, 4992 | `WellKnownSymbol::ToStringTag.description().to_string()` |
| **Table-driven shape producer** — tuple in a `for … in [ … ]` slice | 2 | 1106, 1350 | the slice's first element becomes `WellKnownSymbol`; `.to_string()` at the insert becomes `.description().to_string()` |
| **Consumer** — `match` / `==` / `!=` against the description | 15 | 20986, 20989, 20992, 20995, 20998, 21001, 21004, 21402, 25604, 28463, 30028, 30156, 30248, 35962, 36166 | compare `WellKnownSymbol` values; see below |
| **Key array into a lookup helper** | 6 | 13251 (×2), 27467, 27528, 33879, 33881 | `WellKnownSymbol` values in the array; the helper takes `WellKnownSymbol` (13251, 27467, 27528) or a small key enum (33879/33881, see below) |
| **Runtime symbol-value producer** — `ExprIr::String(…)` with `ValueKind::Symbol` | 1 | 17152 | `ExprIr::String(WellKnownSymbol::Unscopables.description().into())` |

Notes on the awkward ones:

- **20986-21004** is `match symbol_name.as_str() { "Symbol.iterator" if receiver.kind == … }`.
  It becomes `match symbol {  WellKnownSymbol::Iterator if … => …, WellKnownSymbol::Match => …, … , _ => None }`.
  It keeps the `_ =>` arm — ledger entry **R3**.
- **25604** — `key == "Symbol.species"` on an `ExprIr::String(key)` guarded by
  `key_arg.kind == ValueKind::Symbol`. Becomes
  `WellKnownSymbol::from_description(key) == Some(WellKnownSymbol::Species)`.
- **30028 / 30156 / 30248 / 35962 / 36166** — the
  `try_well_known_symbol_key_name(expr).as_deref() == Some("Symbol.iterator")`
  family. Becomes `== Some(WellKnownSymbol::Iterator)`. **This is the class
  the enum kills outright:** five sites, 4,000 lines (34017 → 30028) and
  2,150 lines (34017 → 36166) from the whitelist that produces the value they
  compare against, each a bare string comparison whose misspelling compiles
  and silently disables a specialization.
- **33879-33881**, the ToPrimitive lookup order, is a `&[&str]` mixing
  `"Symbol.toPrimitive"` with the ordinary string keys `"toString"` and
  `"valueOf"`. §7.1.1's OrdinaryToPrimitive genuinely mixes a symbol key with
  two string keys, so a `&[&str]` is not obviously wrong — but the *order* is
  the spec obligation and the *kinds* differ. Introduce a two-variant private
  enum in `lowering.rs`:
  ```rust
  enum ToPrimitiveLookupKey { Symbol(WellKnownSymbol), Method(&'static str) }
  ```
  and make the two arrays `&[ToPrimitiveLookupKey; 3]`. The existing
  "accept either spelling" lookup at 33888-33891 then dispatches on the
  variant instead of trying both. **This is the row where this area touches
  the ToPrimitive catalog owned by area A; the vocabularies do not conflict —
  area A owns the *hint* (`ToPrimitiveHint`), this area owns the *key*.**

### 6.4 The `@@` prefix: a second, distinct string — do not conflate

`ir.rs:3249` defines `SYMBOL_SHAPE_PROPERTY_PREFIX = "@@"` and
`shape_property_name_is_symbol_keyed(name) = name.starts_with("@@")`;
`read_heap_shape_property` (`ir.rs:3257-3261`) refuses any `@@`-prefixed key,
so that a string-keyed read can never resolve a symbol-keyed shape entry.
`lowering.rs:28252-28254` builds those keys as
`format!("@@{symbol_name}")` where `symbol_name` is already
`"Symbol.iterator"` — i.e. the key is `"@@Symbol.iterator"`.

So there are three strings per symbol in this codebase, and the enum must
expose all three under distinct names, with no `Display` to blur them:

| String | Example | Produced by | Consumed by |
|---|---|---|---|
| member name | `"iterator"` | source text | `from_member_name` |
| description | `"Symbol.iterator"` | `description()` | intrinsic shape maps, `ExprIr::String`, `data.rs` interning |
| shape namespace key | `"@@Symbol.iterator"` | `shape_namespace_key()` | user object-literal shape maps |

**[dry-run obligation D5] — a latent defect this classification exposes.**
The intrinsic shape maps insert under the *bare description*
(`"Symbol.toStringTag"`, 42 sites in §6.3), which does **not** start with
`@@`, so `read_heap_shape_property` does not filter it. `lowering.rs:28802`,
`29058`, and `35032` call `read_heap_shape_property` with a general
user-derived `key`. A JS program doing `Array.prototype["Symbol.iterator"]`
or `ArrayBuffer.prototype["Symbol.toStringTag"]` — a *string* key — would
therefore appear to consult a symbol-keyed entry, which S4 forbids. The
comment at `lowering.rs:33885-33887` acknowledges the two spellings ("object
literals record it in the symbol namespace, while the intrinsic wrapper
shapes still use the bare name. Accept either spelling.") without treating
the asymmetry as a hazard.

The dry-runner must trace one such read symbolically and report whether it
resolves. **This contract does not fix it** — unifying the intrinsic shapes
onto `shape_namespace_key()` would change shape contents and therefore
emitted bytes, which is a behaviour change, not a refactor. Recording it,
with a name for each of the three strings so the choice is explicit at every
future call site, is this area's contribution. If the dry-runner confirms
the read resolves, file it as a defect for a follow-up lane and cite this
section.

### 6.5 `well_known_symbol_prototype_properties` — a producer/consumer join to close

`lowering.rs:519` declares
`well_known_symbol_prototype_properties: BTreeMap<(String, String), ValueInfo>`.
The second component is a description string. The producer is
`lowering.rs:35155-35169`; the consumer is `lowering.rs:28864-28867`, which
does `.get(&(constructor_name.to_string(), key.to_string()))` — the two
joined only by string equality, ~6,300 lines apart, with a `String`
allocated per lookup.

**Retype to `BTreeMap<(String, WellKnownSymbol), ValueInfo>.** The field is
private to `lowering.rs`; the change is contained. The five
`lowerer.well_known_symbol_prototype_properties = self.….clone()` propagation
sites (14300, 17639, 19630, 20051, plus the `BTreeMap::new()` at 7909) are
unaffected.

**One behaviour change, deliberate, must be declared.** Today the producer at
35155 accepts *any* symbol-kinded string starting with `"Symbol."` into the
map. After the retype it must call `WellKnownSymbol::from_description(..)`,
which returns `None` for a symbol-kinded description that is not one of the
fifteen; the `None` path takes the existing conservative
`retain(|(constructor_name, _), _| constructor_name != &root)` invalidation
branch. That is strictly more conservative, so it cannot produce a wrong
answer — but it *can* change emitted bytes and therefore shows up as a rung-G
golden diff.

**[dry-run obligation D8]** — the dry-runner must determine whether any
producer of a `ValueKind::Symbol`-carrying `ExprIr::String` can emit a
description outside the fifteen. Three producers were measured:
`lowering.rs:28353` (whitelisted, closed), `lowering.rs:17152`
(`"Symbol.unscopables"`, closed), and `lowering.rs:29276` via
`lower_well_known_symbol_property_key` (closed by the same whitelist). If the
dry-runner finds no fourth producer, the `None` branch is unreachable and the
rung-G diff must be **empty**; a non-empty diff is then a defect in the
encoding, not an expected consequence. If a fourth producer exists, keep the
key as `String` and record the reason as a new ledger entry.

Add, at the producer, `debug_assert!(is_symbol_description(text))` guarded by
the `ValueKind::Symbol` check — the runtime half of ledger entry **R2**.

### 6.6 Deliberately left as `&str`, and why that is the point

- **`crates/scriptw-ir/src/lib.rs`, 10 sites** — `4497, 4536, 4539, 4577,
  4599, 4610, 4643, 4655, 4672, 4695`. These are JS *source text* fixtures
  and assertions for tests that verify a program using the literal string key
  `"Symbol.iterator"` is **not** treated as symbol-keyed. They must stay
  strings. Their existence is the strongest evidence that S4 needs
  defending, and the new type sharpens rather than replaces them: after this
  change, `"Symbol.iterator"`-as-a-string-key and
  `WellKnownSymbol::Iterator` are different Rust types, so a future edit
  cannot confuse them by accident.
- **`crates/scriptw-ir/src/modules/namespace.rs:1046, 1052, 1391`** — generated
  JS source text (`Object.defineProperty($ns, Symbol.toStringTag, …)`) and
  assertions over it. Source text, not a domain value. Stays.
- **`lowering.rs:29262` and `lowering.rs:35151`** — `starts_with("Symbol.")`.
  Ledger entry **R2**; they become `is_symbol_description(..)`, which is a
  named function over the open domain rather than an inline literal.

### 6.7 Explicitly out of scope

The **247** exact-literal well-known-symbol sites outside `crates/scriptw-ir`
(324 total minus the 77 in `scriptw-ir`), across 31 files — `builtins/host.rs`
40, `builtins/standard.rs` 22, `builtins/bootstrap.rs` 21, `data.rs` 18,
`intrinsics/symbol.rs` 17, `builtins/string.rs` 16, `objects.rs` 15,
`builtins/array.rs` 13, and 23 more — are **not** migrated here. They are one
follow-up consumer lane, gated on batch 2 reporting done. Note
`crates/scriptw-aot-wasm/src/intrinsics/temporal.rs` (8 literals) is on the
batch-2 exclusion list and must not be touched by that lane until batch 2
reports done either.

Also out of scope, restated: `Symbol.for` / `Symbol.keyFor` registry symbols
(an open domain, §19.4.2.2/19.4.2.6); `Symbol.prototype`;
`crates/scriptw-runtime/src/lib.rs:147-150`'s
`IntrinsicPropertyKey::WellKnownSymbol(&'static str)`, which is a different
crate with no dependency edge to `scriptw-ir` — migrating it would either
invert the dependency or duplicate the enum, and neither is worth doing until
the consumer lane runs.

---

## 7. Dry-run corpus: what each trace must establish

The dry-runner executes these symbolically against the code, not by running
the suite. Each has a specific question.

| # | Corpus item | Question the trace must answer |
|---|---|---|
| **T1** | `intl402/DateTimeFormat/prop-desc.js` | Confirm the `names.rs:190-206` fix is the same pattern proposed here, and state in one sentence what would have caught the original drift. Then confirm the macro form of §2 is *strictly stronger* than the `INTL_NAMESPACE_CONSTRUCTORS` const slice: the slice makes drift *detectable by a reader*, the macro makes it *unrepresentable*. |
| **T2** | `built-ins/AggregateError/prop-desc.js` | Trace `infer_expr_throw_info` with `name = AGGREGATE_ERROR_NAME`. Record that it reaches `_ => StandardBuiltinId::ErrorConstructor` (`lowering.rs:12114`). Then confirm **no lowering site constructs it today** (§5.3) — the correction to the brief. |
| **T3** | `built-ins/SuppressedError/prop-desc.js` | Same trace. Additionally confirm `SuppressedError` is a real intrinsic here (`names.rs:244`, `builtins.rs:1397`) and that `using` has landed, i.e. that the latent defect is *armed*. |
| **T4** | `built-ins/NativeErrors/TypeError/prop-desc.js` | The control. Trace `TypeError` end to end through all five tables and record the correct shape the other eight must match. Note that `built-ins/NativeErrors/` contains exactly the six of §20.5.5 — confirming the spec partition of §1.1. |
| **T5** | `built-ins/Symbol/iterator/prop-desc.js` | Trace producer→consumer for `@@iterator`. `"Symbol.iterator"` occurs at **15** sites in `lowering.rs`, measured: 7 shape-table producers (1106, 1350, 2905, 3007, 3219, 4390, 4602), 1 lookup-array element (13251), 2 `match` arms (20986, 20989), and 5 `== Some("Symbol.iterator")` comparisons (30028, 30156, 30248, 35962, 36166). The enum collapses the last 7 into variant equality. Confirm the set. |
| **T6** | `built-ins/Symbol/toStringTag/prop-desc.js` | Deliberately misspell `toStringTag` as `toStringtag` in **one** of the two whitelists (§6.2) and confirm the failure is silent: it compiles, the specialization at the misspelt site stops firing, and the *other* whitelist still accepts the correct spelling — so the two lists disagree and nothing reports it. Then re-trace against `from_member_name` and confirm the whole class disappears because there is one list. |
| **T7** | `built-ins/Symbol/toPrimitive/prop-desc.js` | Trace the §6.3 `ToPrimitiveLookupKey` change through `lowering.rs:33879-33894`. Confirm the §7.1.1 lookup **order** is preserved exactly for both hints. Confirm this area's `WellKnownSymbol::ToPrimitive` and area A's `ToPrimitiveHint` do not collide in naming or ownership. |
| **T8** | `built-ins/Symbol/unscopables/prop-desc.js` | `@@unscopables` appears in the whitelist and at exactly one site (`lowering.rs:17152`, a producer). Determine whether **any consumer reads it** — i.e. whether §9.1.1.2.1's `with`-scope `HasBinding` filtering actually consults it. If no consumer exists, the producer emits a value nothing reads, and the contract must say so. |
| **T9** | `built-ins/Symbol/asyncDispose/prop-desc.js` | Confirm `@@asyncDispose` and `@@dispose` are the two rows that make the set 15 rather than 13, that both come from Explicit Resource Management and not from ES2024 Table 1, and that const assertion **W2** names them as a separate array rather than burying them in `ALL`. |
| **T10** | `built-ins/Symbol/isConcatSpreadable/prop-desc.js` | The longest spelling, and its consumers are in `crates/scriptw-aot-wasm/src/builtins/array.rs` (13 literals) — outside this lane. Trace it to fix the exact boundary: producer inside `scriptw-ir` becomes typed; consumer in `array.rs` stays `&str` and is joined to the producer only through `description()`. State what remains uncheckable across that seam until the consumer lane runs. |
| **T11** | **Adversarial, compile-time.** | (a) Add a 16th spelling `"unscopable"` to the whitelist at `lowering.rs:34017` and confirm **nothing fails** — it compiles, produces `"Symbol.unscopable"`, and no consumer reads it. (b) Re-trace against `WellKnownSymbol`: adding a 16th row makes `ALL.len() == 15` (**W1**) a build failure, and adding a variant *without* a row makes the generated exhaustive `member_name` match fail with `E0004`. (c) Remove `"species"` from the whitelist while leaving its 10 `lowering.rs` sites (7 producers: 2922, 3483, 3507, 3556, 3946, 3974, 4212; 3 consumers: 25604, 27467, 27528): today the parse stops recognising `Symbol.species` and all three consumers become silently dead fast paths, with the seven producers still writing the key. After the change this edit has **no analogue** — there is no whitelist separate from the row list. Deleting the *row* instead makes all 10 sites `error[E0599]`. State (c) precisely: the enum does not make "removing a spelling" an error, it makes the whitelist-versus-domain distinction *cease to exist*, which is the stronger result. |
| **T12** | **Adversarial, compile-time.** | Construct `ExprIr::RuntimeThrow { name: SUPPRESSED_ERROR_NAME, .. }` at a lowering site; trace to `lowering.rs:12106` and confirm it silently yields `StandardBuiltinId::ErrorConstructor`. Re-trace against `name.constructor()` (§4.3) and confirm it yields `SuppressedErrorConstructor`. Then verify the edit requires **zero** changes to `crates/scriptw-aot-wasm/src/builtins/temporal.rs` — batch-2 excluded, one `_ERROR_NAME` use at line 6629, which §4.2 keeps compiling. |

Additional dry-run obligations, not tied to a corpus file:

| # | Obligation |
|---|---|
| **D5** | §6.4 — does a string-keyed read reach a bare-description intrinsic shape entry? |
| **D6** | §4.4 item 3 — confirm `is_error_prototype_expr` has zero callers, and that deleting it is correct rather than that a caller was lost. |
| **D7** | §4.2 — confirm `pub const ERROR_NAME: &str = NativeErrorKind::Error.as_str();` remains legal in the four `match`-pattern positions (`module.rs:1731`, `operations.rs:142`, `lowering.rs:10759`, `lowering.rs:20725`). This is the only step whose failure is a compile error in files this campaign does not own. |
| **D8** | §6.5 — is there a fourth producer of a `ValueKind::Symbol`-carrying `ExprIr::String`? |

---

## 8. Mistake-class table

Each row: the mistake, what happens today, and the exact compile error after.

| # | Mistake | Today | After — the compile error, by name |
|---|---|---|---|
| **M1** | Misspell a well-known symbol in a **consumer** comparison, e.g. `… == Some("Symbol.toStringtag")` at one of the 15 consumer sites. | Compiles. Matches nothing. Silently disables a spec extension point (§1.2 S5 table). No test signal — the fallback path is a legal path. **324 well-known-symbol literals across 34 files; 66 in `lowering.rs` alone, against 15 legal spellings, in a 37,830-line file where a producer at line 737 and its whitelist at line 34017 are 33,280 lines apart.** | `error[E0599]: no variant or associated item named 'ToStringtag' found for enum 'WellKnownSymbol'` — the comparison is now `== Some(WellKnownSymbol::ToStringtag)`. |
| **M2** | Misspell a well-known symbol in a **shape-table producer**, e.g. `properties.insert("Symbol.toStringtag".to_string(), …)` at one of the 42 producer sites. | Compiles. Inserts a key nothing reads; the modelled shape and the emitted object disagree; `Object.prototype.toString` specialization and `getOwnPropertyDescriptor` give different answers. | `error[E0599]: no variant or associated item named 'ToStringtag' found for enum 'WellKnownSymbol'` — the insert is now `WellKnownSymbol::ToStringtag.description().to_string()`. |
| **M3** | Add a well-known symbol to one whitelist and not the other. **This repository has two, byte-identical, 5,700 lines apart (§6.2), and has already shipped this exact defect for a different closed set — `names.rs:190-200`, `intl402/DateTimeFormat/prop-desc.js`.** | Compiles. The two producers disagree; one lowering path recognises the symbol and the other does not, so behaviour depends on which syntactic form the program used. | **Unrepresentable.** Both whitelists are deleted; there is one `from_member_name`. Adding a row to the macro's list regenerates every table simultaneously. There is nothing to keep in sync. |
| **M4** | Add a 16th well-known symbol as an enum variant but forget the row list. | n/a — no enum today. | `error[E0004]: non-exhaustive patterns: 'WellKnownSymbol::NewOne' not covered` in the generated `member_name`/`description` match. |
| **M5** | Add a 16th row but leave `ALL` at fifteen (or hand-edit `ALL`). | n/a. | `error[E0080]: evaluation of constant value failed` on const assertion **W1** (`ALL.len() == 15`) or **W3** (`ALL[i] as u8 == i`). |
| **M6** | Let a member name and its description drift, e.g. `("ToStringTag", "toStringTag", "Symbol.toStringtag")`. | Compiles. The parse accepts source `Symbol.toStringTag` and emits a description no consumer matches — worse than M1, because it is *self*-consistent within the row. | `error[E0080]` on const assertion **W6** (`description == "Symbol." ++ member_name`). |
| **M7** | Add a NativeError to one of the former nine-row tables and not the others. | Compiled before the retrofit. | **Unrepresentable on both sides.** `scriptw-ir` derives from `NativeErrorKind::ALL` / `constructor()`. T24 replaced the four AOT copies with one exhaustive `ErrorPrototypeLocation` match and derived the realm entries and `instanceof` lookup from it (§13); a tenth kind is `E0004`. |
| **M8** | Add a tenth error intrinsic to the enum but not to the row list. | n/a. | `error[E0004]: non-exhaustive patterns` in the generated `as_str`, `constructor`, and `is_native_error` matches — three separate failures. |
| **M9** | Two error kinds mapped to the same `StandardBuiltinId` (e.g. copy-paste `SuppressedError => AggregateErrorConstructor`). | Compiles. Two distinct error types share a prototype; `e instanceof AggregateError` is true for a `SuppressedError`. | `error[E0080]` on const assertion **N6** — `from_constructor(constructor(k)) == Some(k)` cannot round-trip both. |
| **M10** | Two error kinds sharing an `as_str()`. | Compiles; the second is unreachable through `from_str`. | `error[E0080]` on const assertion **N5** (pairwise distinct). |
| **M11** | `is_native_error()` drifting from §20.5.5's six — e.g. someone "helpfully" makes `AggregateError` a native error. | n/a today (the distinction is not represented at all). | `error[E0080]` on const assertion **N7** (exactly six). |
| **M12** | An `ExprIr::RuntimeThrow` constructed with a name outside the nine, or a typo'd hand-written emitter name. | `name: "TyepError"` compiled and ultimately fell back to `OBJECT_PROTOTYPE_GLOBAL_INDEX`. | The IR producer is `E0308` because it requires `NativeErrorKind`. The remaining hand-written AOT string boundary returns an internal `EmitError` before lookup; the typed lookup cannot represent the fallback (§13). |
| **M13** | A consumer compares against a spelling no producer emits — dead code that looks live. Enabled today by `try_well_known_symbol_key_name` returning `Option<String>`. | Compiles, allocates a `String` per query, hands the consumer an unchecked value. | The producer returns `Option<WellKnownSymbol>`; a consumer naming a 16th value is `error[E0599]`. A consumer naming a *real* variant that no producer path reaches is still not caught — that is a reachability question, not a domain question, and is out of this area's reach. |
| **M14** | Confusing the three strings — writing `description()` where `member_name()` or `shape_namespace_key()` was meant (§6.4). | n/a today; the strings are written as literals and the confusion is *already present* between intrinsic shapes (bare description) and object-literal shapes (`@@` prefixed) — see obligation **D5**. | Not a compile error, and the contract says so honestly: all three are `&str`/`String`. What the type buys is that each has a *name at the call site*, and that `Display`/`AsRef`/`Deref` are absent so no site can stringify without choosing. Newtyping all three (`MemberName`, `Description`, `ShapeKey`) was considered and **rejected**: the map keys stay `String` (ledger **R1**), so the newtypes would be unwrapped at every boundary and would be decoration by AGENTS.md's own test. |

---

## 9. Summary of what stays untouched

- `crates/scriptw-aot-wasm/src/builtins/intl_datetimeformat.rs`,
  `temporal*.rs`, `emitted_function.rs`, `runtime_helpers.rs` — batch-2
  exclusion list. Zero edits. §4.2 exists to guarantee this for
  `temporal.rs`'s single `RANGE_ERROR_NAME` use.
- The four NativeError tables in `crates/scriptw-aot-wasm` were untouched by the
  original lane, then replaced by the T24 follow-up in §13.
- The 247 well-known-symbol literals outside `crates/scriptw-ir`, in 31 files
  — same lane.
- `crates/scriptw-runtime/src/lib.rs:147-150` — different crate, no
  dependency edge.
- The four `StandardBuiltinId` → name mappings in
  `crates/scriptw-ir/src/builtins.rs` — already exhaustive over the enum.
- `ObjectShape.properties` / `ArrayShape.properties` key type — ledger **R1**.
- `emit_throw_runtime_error`'s `name: &str` parameter
  (`builtins/errors.rs:476`) — the seam between this area and the deferred
  lane.
- The 10 JS-source-text symbol literals in `crates/scriptw-ir/src/lib.rs` and
  the 3 in `modules/namespace.rs` — source text, not domain values, §6.6.

### Verification ladder for the encoder

| Step | Command | Expected |
|---|---|---|
| after §4.1 step 1 | `cargo check -p scriptw-ir` | clean; **this is where N1–N7 and W1–W8 are proved** |
| after §4.1 step 2 | `cargo check -p scriptw-aot-wasm` | clean; **this is obligation D7** — on failure, take §4.2's fallback |
| after §4.1 steps 3–5 and §6 | `cargo check -p scriptw-ir && cargo check -p scriptw-aot-wasm` | clean |
| whole area | `cargo test -p scriptw-ir` (rung 1) | as baseline |
| whole area | rung G golden capture + `diff -r` | **empty**, given obligation **D8** resolves as expected (§6.5). A non-empty diff is a defect in the encoding, not an expected consequence. |

Rung G matters here specifically because every change in this contract is a
pure refactor except the one declared in §6.5, and `batch-workflow.md` names
rung G as the gate for exactly that case.

---

## 10. Encoder record

Stage: ENCODER, appended after implementation. Nothing above was rewritten;
this section states what was actually built, where it differs from §§2–8, and
which mistake classes came out weaker than promised.

No cargo or rustc command was run (batch 2 holds the build lock). The only
check performed was `rustfmt --edition 2021 --check` on copies of the touched
files in a scratchpad: they parse and are rustfmt-clean. That proves syntax and
proves nothing about types. Integration instructions, including the three
required `scriptw-aot-wasm` edits this lane did not make, are in
`target/lane-notes/Closed spec name domains: NativeErrorKind and WellKnownSymbol-theory-integration.md`.

### 10.1 Mistake classes discharged as compile errors

| # | Promised | Delivered |
|---|---|---|
| **M1** | `E0599` on a misspelt consumer comparison | as promised. The five `== Some("Symbol.iterator")` sites are `== Some(WellKnownSymbol::Iterator)`; the seven-arm receiver-specialization match, the two `== "Symbol.toPrimitive"` sites, the `Symbol.species` descriptor check and the two `try_static_array_iterator_override`-family sites all compare enum values. |
| **M2** | `E0599` on a misspelt shape producer | as promised. All 42 producers are `WellKnownSymbol::X.description().to_string()`. |
| **M3** | unrepresentable | as promised, and stronger than expected: both fifteen-element whitelists *and* both copies of the four-clause "is this the real builtin `Symbol`?" guard are gone. The guard is now one `expression_is_builtin_symbol_intrinsic`; the parse is one `WellKnownSymbol::from_member_name`. |
| **M4** | `E0004` on a variant without a row | **unrepresentable instead.** The variants *are* generated from the rows, so a variant without a row cannot be written. |
| **M7** | unrepresentable in `scriptw-ir`; originally open in `scriptw-aot-wasm` (R4) | The original lane collapsed the five `scriptw-ir` tables. The T24 follow-up subsequently discharged R4 with the exhaustive location authority in §13. |
| **M8** | `E0004` in three generated matches | **unrepresentable instead**, same reason as M4. |
| **M9** | `E0080` on N6 | as promised. `all_is_ordered_and_round_trips` includes the `from_constructor(constructor(k)) == Some(k)` round trip. |
| **M10** | `E0080` on N5 | as promised (`spellings_are_distinct`). |
| **M11** | `E0080` on N7 | as promised (`native_error_subset_agrees`), with the caveat in §10.3. |
| **M12** | `E0308` at the producer | as promised. `ExprIr::RuntimeThrow.name` is `NativeErrorKind`; all 14 constructions and 11 test patterns retyped. The T24 follow-up now validates the remaining AOT string boundary before typed lookup (§13). |

### 10.2 Mistake classes delivered with a different error, or as a stronger property

| # | Promised | Delivered | Why |
|---|---|---|---|
| **M5** | `E0080` on W1 (`ALL.len() == 15`) or W3 | `E0308`, plus `E0080` on W3 | `ALL`, `TABLE_1`, `EXPLICIT_RESOURCE_MANAGEMENT` and `NATIVE_ERRORS` declare their length in the *type* (`[WellKnownSymbol; 15]` etc.), so a sixteenth row is `error[E0308]: expected an array with a fixed size of 15 elements, found one with 16`. Given that, assertions **N1** and **W1** would have been tautologies, and AGENTS.md's test rejects a check that cannot fail. They were **not written**. The *content* half of **W2** — that `TABLE_1 ++ EXPLICIT_RESOURCE_MANAGEMENT` really is `ALL`, in order — is not implied by the lengths and **is** asserted. |
| **M6** | `E0080` on W6 (`description == "Symbol." ++ member_name`) | **unrepresentable** | `description()` is generated as `concat!("Symbol.", $member)` from the same row. A row cannot express a drifting description, so S2 is definitional rather than checked. **W6 was not written**; `SYMBOL_DESCRIPTION_PREFIX` is still tied to the generated descriptions by **W8**. |

Assertions actually emitted: `native_error.rs` — `all_is_ordered_and_round_trips`
(N3, N4, N6), `spellings_are_distinct` (N5), `native_error_subset_agrees`
(N2, N7). `well_known.rs` — `all_is_ordered_and_round_trips` (W3, W4, W5, W8),
`spellings_are_distinct` (W7, extended to descriptions as well as member
names), `partition_covers_all` (W2's content half). Not written, with reasons
above: N1, W1, W6.

### 10.3 Added to the runtime-checked ledger

| # | Invariant | Where | Why no type carries it | What must check it |
|---|---|---|---|---|
| **R5** | `NativeErrorKind::NATIVE_ERRORS`, `is_native_error()` and `from_constructor()` have **no product call site**. | `native_error.rs` | Their only consumers are the const assertions that tie them (N6, N7) — i.e. the mistake they make into a compile error is a mistake *in themselves*. Nothing in the compiler currently needs the §20.5.5 six, and nothing needs the reverse constructor map. By AGENTS.md's test this is the closest thing in this area to decoration, and saying so is better than pretending otherwise. They were kept because E5 is real ECMA-262 structure (§20.5.6's template applies to exactly six) that this codebase represents nowhere else, and because M9 and M11 are genuine defect shapes. **If a reviewer disagrees, deleting all three plus `native_error_subset_agrees` is self-contained and costs the product path nothing.** | nothing at runtime; this row exists so the next reader does not mistake them for load-bearing. |
| **R6** | A sixteenth well-known symbol silently gets no receiver-specialization fast path. | `lowering.rs`, the `match symbol` with `_ => None` (ledger **R3**'s site) | Unchanged from R3: the domain is `WellKnownSymbol × ValueKind` and most cells legitimately have no fast path. The arm now carries a comment naming all nine symbols that deliberately fall through, so the omission is at least written down. | rung 1 `cargo test -p scriptw-ir`; the CLI iterator/regexp area suites. |

`R1`, `R2` and `R3` remain unchanged. R4 was honoured by the original lane and
then discharged by the T24 follow-up (§13):
`ObjectShape::properties` keeps its `String` key (R1); the two
`starts_with("Symbol.")` sites became `is_symbol_description(..)` and the
producer gained the specified `debug_assert!` (R2); the specialization match
kept its `_ => None` with a naming comment (R3).

### 10.4 Deviations from the retrofit map

1. **§4.4 item 3 / obligation D6 — confirmed and executed.**
   `grep -rn is_error_prototype_expr crates/ --include=*.rs` returns one hit,
   the definition. It was deleted, not migrated, with a comment left at the
   site recording why.
2. **§6.3, the `Symbol.species` rows.** `read_object_shape` was **not** retyped
   to take a `WellKnownSymbol`: it is a general helper with 14 call sites over
   arbitrary string keys, so a closed key type there would be false. Both sites
   pass `WellKnownSymbol::Species.description()`. The literal is gone and the
   join to the producer runs through the enum, which is the property §6.3 was
   after. `optional_chain_well_known_symbol_property_info` — which genuinely
   only ever takes a well-known key — *was* retyped.
3. **§6.3, the two table-driven shape producers (`Map`/`Set` prototypes).** The
   slices mix ordinary string method names with one symbol key, so the first
   element cannot become `WellKnownSymbol`. The `("Symbol.iterator", …)` row was
   lifted out of each loop into its own `properties.insert`. `ObjectShape::properties`
   is a `BTreeMap`, so insertion order does not affect the resulting map; this
   is byte-neutral.
4. **§6.3, the ToPrimitive lookup.** Implemented as specified —
   `enum ToPrimitiveLookupKey { Symbol(WellKnownSymbol), Method(&'static str) }`,
   two `&[ToPrimitiveLookupKey; 3]` arrays, dispatch on the variant. One
   incidental consequence: the `Method` arm no longer also probes the `@@`
   namespace for `"toString"`/`"valueOf"`. Nothing writes `@@toString` — every
   `@@` key is built from a `description()`, which always begins `"Symbol."` —
   so this is behaviour-preserving. §7.1.1's lookup order is unchanged for both
   hints.
5. **§6.5.** Implemented as specified, including the declared behaviour change
   and the `debug_assert!`. Obligation **D8** stands: the rung-G diff must be
   empty, and a non-empty diff means a fourth `ValueKind::Symbol` string
   producer exists that this contract did not find.

---

# 11. Amendments from the dry-run discrepancy pass (applied)

**This section supersedes every claim it names.** The code changes it describes
are in the tree.

## 11.1 The tree did not build; §4.3's three call sites were never edited

`ExprIr::RuntimeThrow.name` was retyped to `NativeErrorKind` in `scriptw-ir`,
but the three `scriptw-aot-wasm` consumers §4.3 names were left alone, so
`scriptw-aot-wasm` could not compile and `cargo check` — this campaign's stated
main test — could not be run against the area's own work. The fix existed only
as prose in `target/lane-notes/`.

Applied, all byte-neutral (`as_str()` yields the same interned bytes):
`data.rs:3197` `self.intern_string(name)` → `self.intern_string(name.as_str())`;
`expressions.rs:1256` and `:3025` `emit_throw_runtime_error(name, …)` →
`(name.as_str(), …)`. Verified against `fn intern_string(&mut self, value: &str)`
and `fn emit_throw_runtime_error(&mut self, name: &str, …)`, and verified that
the other ten `RuntimeThrow` matches in the crate all bind `{ .. }`.

## 11.2 Obligation D5 is a confirmed live defect (recorded, not fixed here)

A string-keyed property read resolves an intrinsic-shape entry inserted under
the bare Table 1 `[[Description]]`, violating S4. Symbolic trace for
`[1,2,3]?.["Symbol.iterator"]`:

1. `lower_static_property_key` sees `is_symbol_description("Symbol.iterator")`,
   lowers the operand, finds `kind == ValueKind::String` (a string literal, not
   a symbol) and falls through to `PropertyKeyIr::StaticString`.
2. `optional_chain_static_property_info(receiver, "Symbol.iterator")`.
3. Receiver kind `Array` with `shape.prototype.is_none()` selects `ARRAY_NAME`,
   so `%Array.prototype%`'s modelled shape is read.
4. `ir.rs`'s filter rejects only `@@`-prefixed keys, and `"Symbol.iterator"` is
   not `@@`-prefixed, so it passes.
5. `properties.get("Symbol.iterator")` **hits** the entry inserted as
   `WellKnownSymbol::Iterator.description().to_string()`, yielding
   `StandardBuiltinId::ArrayPrototypeValues`.

Static inference concludes the read is a function with a known target; the spec
answer is `undefined`. The existing S4 tests exercise object-*literal* shapes,
which correctly use the `@@` namespace, so they do not cover this.

**Not fixed in this pass**, deliberately: the fix routes the 47 intrinsic-shape
inserts through `shape_namespace_key(sym)` and stops the two deliberate symbol
readers accepting the bare-description spelling, which changes emitted bytes and
therefore belongs to a lane with a rung-G budget. It is recorded here as the
area's largest open defect rather than left in a lane note.

## 11.3 The census regex could not see member names; two literals survived

§6.3's census regex `"Symbol\.[A-Za-z]*"` matches **descriptions** only, so
"all 66 literals gone" was an incomplete measurement of the domain rather than
of the regex. `symbol_well_known_value(&mut self, property: &str)` was called
with the member-name literals `"dispose"` and `"asyncDispose"`, and the
parameter was load-bearing twice — the shape lookup key and the emitted
`PropertyKeyIr::StaticString`. Misspelling it compiled, emitted a read of a
property that does not exist, and made `await using` silently never dispose:
S5's "resource not disposed" row. The shape half was masked (the modelled
`Symbol` constructor shape carries only `prototype`, `for` and `keyFor`); the
emitted-key half was not.

The parameter is now `symbol: WellKnownSymbol`, using `symbol.member_name()`,
with the two callers passing `Dispose` / `AsyncDispose`. Re-measured: only three
member-name literals ever existed in `lowering.rs`, and the third (`"matchAll"`)
is `String.prototype.matchAll`, an ordinary method name and correctly a string.

## 11.4 The two `NativeErrorKind::from_str` predicates were unguarded

§4.4's "same replacement" wording hid an asymmetry: §6.2 extracted
`expression_is_builtin_symbol_intrinsic` precisely because a spelling test is
not a resolution test, and the error-name path got no equivalent. 9.1.1 resolves
a name against the environment first, so:

- `function f(TypeError) { for (const k in TypeError) return k; } f({a:1})` —
  spec `"a"`; the compiler elided the loop and answered `undefined`;
- `function g(RangeError){ return RangeError.propertyIsEnumerable("prototype"); }`
  folded to `false` whatever the argument was.

Pre-existing rather than introduced (the old nine-arm `matches!` was equally
unguarded), but fixed here: `ScriptLowerer::identifier_is_builtin_native_error`
mirrors `expression_is_builtin_symbol_intrinsic`'s four clauses and both sites
now use it. Note `is_error_constructor_expr` inherits the same weakness through
`is_builtin_reference_expr`, which still matches on identifier text alone —
recorded as open.

## 11.5 Five receiver-specialization arms ignored the receiver

Ledger R3/R6 justified the match with "the domain here is
`WellKnownSymbol × ValueKind`", which was true only of the two `Iterator` arms.
The `Match`/`MatchAll`/`Replace`/`Search`/`Split` arms fired on the symbol
alone, so `({ [Symbol.match](s) { return 42; } })[Symbol.match]("x")` was typed
as `RegExp.prototype[@@match]`'s `Array | Null` rather than as `Number`. The
emitted IR is a generic `ExprIr::CallMethod` either way, so dispatch was still
right; the damage was confined to the inferred `ValueInfo` — which is exactly
invariant S5's subject.

The five arms now carry `if regexp_receiver`, computed by
`Self::receiver_shape_allows_regexp_symbol_protocol`: `true` when the receiver
carries the RegExp prototype shape (a literal or a `RegExpConstructor` result)
and `true` when nothing is known about its shape (the pre-existing inference,
kept so this is byte-neutral for unshaped receivers); `false` for any other
*known* shape, which is what an object literal with its own `[Symbol.match]`
has. The R3/R6 comment is rewritten to say what the code does.

## 11.6 §10.2's "tautological assertions were rejected" claim is withdrawn

`partition_covers_all` (W2) cannot fail: `ALL`, `TABLE_1` and
`EXPLICIT_RESOURCE_MANAGEMENT` are three expansions of the same two `$(...)+`
sequences in the same order, so the concatenation identity is definitional. The
same is true of the discriminant-order halves of N3/W3 (`ALL` is generated in
declaration order and `#[repr(u8)]` assigns discriminants in declaration order),
and N4/W4/W5 can only fail when N5/W7 already have.

The assertion is kept — as executable documentation of the partition, and as the
check that starts being able to fail the moment either group stops being a
direct expansion — and its doc comment now says so plainly. What is withdrawn is
§10.2's claim that checks which cannot fail were rejected on principle: the code
does not follow that principle, and pretending otherwise is worse than either
choice. The assertions that genuinely earn their place today are **N5/W7** (a
duplicate spelling is writable in a row list), **N6** (two rows can name the
same `$constructor`, which is otherwise only an `unreachable_patterns` warning)
and **W8** (a change to `SYMBOL_DESCRIPTION_PREFIX` is a real edit).

## 11.7 The parse side of the three-strings hazard is closed

M14 analysed only the *print* side. `from_member_name` and `from_description`
shared the signature `fn(&str) -> Option<Self>`, so handing one the other's
domain compiled and silently returned `None` — at `lowering.rs:35167` that
discards every `well_known_symbol_prototype_properties` fact and stops the
specialization firing, with no compile error and no test signal: the M1 failure
mode relocated to the parse boundary. Both live call sites were correct, so this
was latent, not live.

Closed with two zero-cost newtypes at the parse boundary only —
`SymbolMemberName<'a>` and `SymbolDescription<'a>`, no `Deref`, no
`From<&str>` — which M14's cost/benefit argument does not cover because they are
not unwrapped at any map boundary: the shape maps stay string-keyed.

## 11.8 Dead surface deleted

- `SYMBOL_DESCRIPTION_PREFIX` was crate-public with **zero** consumers outside
  the module defining it (measured: definition, `is_symbol_description`'s own
  body, and the `lib.rs` re-export). It is now module-private and dropped from
  the re-export. `is_symbol_description` is the intended public face and is
  genuinely used. Its doc now states that it answers `true` for `"Symbol."`,
  `"Symbol.for"`, `"Symbol.keyFor"` and `"Symbol.prototype"` — honest for a
  namespace test, misleading only if read as a well-known-symbol test.
- **Ledger R5 is corrected.** `from_constructor` is **not** dead: it is
  load-bearing inside `all_is_ordered_and_round_trips`, the only thing that
  turns two rows naming the same `$constructor` from an `unreachable_patterns`
  warning into `error[E0080]` (N6). R5 grouped it wrongly. What *was* dead is
  `NATIVE_ERRORS`, `is_native_error`, `native_errors_contains` and
  `native_error_subset_agrees` — zero consumers workspace-wide, and
  `is_native_error` additionally invited a silent wrong answer
  (`for k in ALL { if k.is_native_error() { … } }` reads as "every error kind"
  while skipping `Error`, `AggregateError` and `SuppressedError`). All four are
  deleted along with the row list's `native =` column; §20.5.5 membership is
  recorded in the per-variant doc comments. Reintroduce it spelled
  `is_20_5_5_native_error`, in the patch that gives it a consumer.

## 11.9 Two measurement corrections

- §4.3 says "12 test patterns in `crates/scriptw-ir/src/lib.rs`" and then lists
  eleven line numbers. Measured in the retrofitted tree: **11**
  (`ReferenceError` ×10, `TypeError` ×1). The encoder's 11 was right.
- §§4–7 cite **pre-retrofit line numbers** throughout (Species producers,
  consumers, the 14 `RuntimeThrow` sites, the 15 `Symbol.iterator` sites, the
  whitelist addresses). Every *count* re-measured correct; every *address* has
  moved. Treat §§4–7 addresses as historical and re-grep before using one.

## 11.10 Obligations that resolved in the contract's favour

- **R1** (shape key stays `String`) — confirmed; the key domain is all JS
  property keys, an open set, and a closed enum there would be false. This is
  also what makes 11.2 a *which-string* bug rather than a *which-type* bug.
- **R2** (`is_symbol_description` over an open domain) — confirmed, and the
  specified `debug_assert!` was delivered.
- **S2** (`description == "Symbol." ++ member_name`) — discharged
  definitionally by `concat!`, stronger than the proposed W6, which was
  correctly not written.
- **E4** (prototype-map totality) — discharged by type on the producer side:
  `infer_expr_throw_info` no longer enumerates at all. The later T24 follow-up
  also discharged the AOT consumer side; see §13.

## 12. Integration record — the compile gate

Integrator's section, written after running the gate. Commands: `cargo check -p
scriptw-ir --all-targets`, `cargo xc`, `cargo fmt --all -- --check`.

**Green, with zero integrator edits to this area.** The three
`scriptw-aot-wasm` call sites that §11.1 applied (`data.rs:3197`,
`expressions.rs:1256`, `expressions.rs:3025`) are the only reason this crate
compiles with `ExprIr::RuntimeThrow.name: NativeErrorKind`, and they do.

Two of this area's blind risks are now measured rather than argued:

- **D7 resolved in the primary form's favour.** The nine `*_ERROR_NAME` consts
  in `names.rs`, defined as `NativeErrorKind::X.as_str()`, are used in
  match-pattern position at `module.rs:1731`, `module.rs:1746` and
  `operations.rs:142` and compile. Pattern legality follows the const's *type*,
  not how the value was computed, exactly as the encoder argued. §4.2's
  literal-reverting fallback and the N8 assertion it would have needed are **not
  required** and should not be taken.
- **Every const assertion in `native_error.rs` and `well_known.rs` evaluated.**
  N2–N7 and W2–W5, W7, W8 are checked facts now, not intended ones. §11.6's
  withdrawal stands: several of them cannot fail today, and the doc comments say
  so.

No warning in either new module: everything in them is `pub` and reachable, and
nothing the retype touched decayed into dead code.

**At this historical gate**, D5, R4 and the unguarded-identifier weakness were
still open. The T24 follow-up in §13 subsequently discharged R4; D5 and the
identifier weakness are unchanged by that patch.

---

## INTEGRATOR stage — D7 discharged by the compiler

The lane note's §3 named **D7** as the one risk it could not discharge without a
compiler: `names.rs`'s nine `*_ERROR_NAME` consts are now
`NativeErrorKind::<V>.as_str()` rather than string literals, and they are used in
**match-pattern position** at `scriptw-aot-wasm/src/module.rs:1731`,
`module.rs:1746` and `operations.rs:142`.

`cargo check -p scriptw-aot-wasm` is clean, so the primary form compiles: a
`const` item is usable as a pattern when its *type* is structural-match, and
`&'static str` is, regardless of how the value was computed. **The §4.2 fallback
(reverting the nine consts to literals plus assertion N8) is not needed and
should not be taken.**

`cargo check -p scriptw-ir` is likewise clean, so assertions N2–N7 and W2–W5,
W7, W8 are evaluated and pass.

Steps 3 and 4 of the note's ladder (`cargo test -p scriptw-ir`, rung G) are test
and golden-capture rungs and were not run by this stage.

---

## 13. T24 follow-up — AOT error prototype locations are closed

T24 closes the consumer-side gap formerly recorded as R4.

- `error_prototype_location.rs` (no longer in the tree) defined one private
  `ErrorPrototypeLocation { global_index, realm_offset }`. One exhaustive
  `match NativeErrorKind`, with no catch-all, assigns both values together.
  `error_prototype_global_index` and `error_realm_prototype_offset` are total
  projections of that record; `error_realm_prototype_entries` maps
  `NativeErrorKind::ALL` rather than maintaining a fourth nine-row list.
- `crates/scriptw-aot-wasm/src/operations.rs` retains the non-error constructor
  cases in its static `instanceof` fast path, but parses error names with
  `NativeErrorKind::from_str` and consumes the same typed location authority.
- `crates/scriptw-aot-wasm/src/builtins/errors.rs` validates each remaining raw
  string entry point once, before selecting a prototype. Typed private helpers
  carry the kind through allocation, realm lookup and diagnostic emission.
  Invalid internal spellings return `EmitError` instead of falling back to
  `%Object.prototype%`.
- The grouped error-constructor emitter converts its already-closed
  `StandardBuiltinId` with `NativeErrorKind::from_constructor` before asking
  for the base Error prototype location. That gives the reverse mapping a real
  product consumer without changing constructor branching or evaluation order.

For every existing valid kind, the same global index, realm offset, emitted
`name` string and instruction order are retained. The deliberate behavior
change is confined to invalid compiler-internal spellings, which no longer
produce a plausible but non-Error JavaScript object.

The follow-up was dry-written and rustfmt-checked in its implementation lane;
Cargo verification is centralized at the batch checkpoint. The minimum gate is
`cargo check -p scriptw-aot-wasm --lib`, followed by
`tests::operations_instanceof_dynamic_rhs_guard_validates` and the engine's
`tests::wasm_backend_create_realm_uses_realm_local_error_constructors_and_prototypes`
contract, which covers all nine prototype rows. The cross-realm fallback
contract is
`tests::wasm_backend_cross_realm_error_constructor_fallback_uses_new_target_realm`.
