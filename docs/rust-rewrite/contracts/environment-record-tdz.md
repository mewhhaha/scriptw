# Environment binding lifecycle

`scriptw-ir/src/binding_lifecycle.rs` places initialization state on each
`BindingInfo`. Creation, initialization and assignment are separate operations.
Both reads and writes of an uninitialized binding throw ReferenceError before
an immutable-write rule is considered.

An uninitialized binding distinguishes allocated storage from a placeholder.
The placeholder reserves resolution to the inner TDZ without claiming the
real storage name; initialization reuses the name selected for that binding.
`TdzPlaceholderName` closes the reserved spelling domain and does not itself
prove lifecycle state. Scope instantiation applies to function bodies and
nested statement lists as well as script/module roots.

Every `OwnedEnvBindingIr` carries `EnvironmentBindingMutabilityIr` from its
actual source record. Source const is immutable with strict=true; named function
expression self bindings are immutable with strict=false; generated suspension
storage is mutable. This policy is independent of Reference strictness and of
whether direct eval can see the binding.

Separate parameter/body records preserve their source modes. Default-created
closures retain the original named-self binding even when a body declaration
shadows its spelling; real parameter names retain their mutable policy.
Await/yield and per-iteration environments preserve the selected binding cell.

Object binding initialization retains the original InitializeBinding/var-write
policy, one acquired property value and the original primitive receiver.
See [object binding](object-binding-single-get.md),
[References](reference-records.md) and
[module namespaces](module-namespace-internal-methods.md).
