# Intl intrinsics in created Realms

## Current atomic GC source — 2026-10-05

The common GC bootstrap publishes each represented Intl namespace member for
both entry and created Realms through `IntlNamespaceMembers` and the shared
intrinsic definitions. NativeHost's separate `created_realm_intl_intrinsics.rs`
installer and `created_realm_intl_structure` raw publication mirror are
retired. The old three-member list and pointer-offset capture below describe
the earlier publication surface.

Current native Intl families consume actual GC formatter/Locale records,
immutable FunctionContext and BuiltinClosureCapture roots, and the private
validated ByteArray provider protocol. Existing created-Realm, bound-format,
constructor/getter and abrupt-Realm semantic targets remain. The maintained
`intl_supported_values_structure` guard retains checked native catalog/provider
identity, once-only coercion, List-to-Array publication and namespace admission.
These publication owners do not establish complete locale-data or Test262
acceptance. See [NativeHost GC values](native-host-gc-values.md).

All source, types and controls for the atomic batch remain uncompiled and
unexecuted. Final representation/helper/guard composition also remains pending.
Earlier verification commands and results below retain their original source
scope; they are historical records, not instructions to run during the full-task
dry-source pass. Later verification follows the [batch workflow](../../../CONTRIBUTING.md#verification)
with a confirmed aggregate 4096 MiB cap, swap zero and serial execution.

## Historical predecessor record

The AOT host creates an `Intl` namespace with the same represented members as
the entry Realm: `getCanonicalLocales`, `Locale`, and `DateTimeFormat`. Namespace
membership comes from `IntlNamespaceMembers`. Constructor and prototype
properties share one ordered definition between both installation paths; a new
namespace constructor without an intrinsic definition is an emission error.
Each namespace object and prototype is fresh, and each builtin function has the
created Realm's `%Function.prototype%` and defining Realm. Getter and constructor
coercions retain the active builtin environment.

`Intl.Locale.prototype` and `Intl.DateTimeFormat.prototype` have pointer-marked
slots in the Realm intrinsic record. Entry bootstrap and created-Realm bootstrap
populate those slots. A primitive `NewTarget.prototype` uses `GetFunctionRealm`
and the required ordinary-prototype lookup; it does not read mutable public
globals. An explicit object prototype retains its representation tag and avoids
the fallback Realm lookup. DateTimeFormat's plain-call path uses the active
builtin's Realm.

Frozen batch5 evidence is recorded in
`target/watched/completed-baseline-intl-locale-new-target-realm-detail-batch5.log`.
The guarded probe showed an absent foreign `Intl`, a valid bound foreign Array,
and a successfully constructed Locale with the entry Realm's prototype.
The first probe's property-read TypeError therefore did not invalidate the
constructor reproduction.

Focused verification targets are `aot_intl_created_realm`,
`aot_intl_locale_constructor`, `created_realm_intl_structure`,
`required_resolved_realm_ordinary_prototype_structure`, and
`intl_date_time_format_construction_order_structure`. They cover reflected
publication, getter identities and error Realms, primitive and explicit
new-target prototypes, proxy revocation, property access order, stored intrinsics
after public-global replacement, and metadata roots.

This changes Realm publication and constructor prototype selection. It does not
expand DateTimeFormat's existing locale-data or formatting domain. Verification
must run through Wasm AOT; source checks alone are not a native result.

The DateTimeFormat `format` getter materializes its cached function in the
active getter's Realm, including when that getter is borrowed onto a formatter
from another Realm. Subsequent getter calls reuse the existing function and
its original Realm. The canonical builtin closure capture slot retains the
formatter, while the environment handle retains the callable identity for
coercion and generated-error Realm lookup. Realm and `%Function.prototype%`
are selected together from the stored intrinsic record; public namespace or
constructor replacement and getter prototype mutation do not affect selection.

Additional verification targets are `aot_intl_bound_format_realm` and
`intl_bound_format_realm_structure`. They cover both borrowed-getter directions,
first-getter cache identity, function descriptors, ignored call receivers,
generated TypeError/RangeError prototypes, and identity of abrupt coercion.
This uses the existing function-object capture layout and AOT materializer.
