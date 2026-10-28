# Conformance ownership

Compiler diagnostics and conformance artifacts use permanent area codes
`T00` through `T29`. These codes identify technical ownership, independently
of temporary work plans or completion status. `T26-unclassified` is a separate
failure-classification bucket and is never a valid area code.

The closed `ConformanceOwner` domain in
[`ownership.rs`](../../crates/scriptw-ir/src/ownership.rs) is the registry authority.
Its parser, formatter, complete set and descriptive area names come from one
macro table. Unknown codes are rejected at admission. Existing serialized
codes retain their meaning across fresh runs and historical evidence.

Historical report fields such as `owner_task_id` and `summary_by_task`, and TSV
columns named `owner_task`, carry these same permanent area codes. Their wire
spelling is retained so existing source-bound evidence remains readable; they
do not refer to work-plan files. The host-ABI ledger admits semantic owners
T00–T26 and its explicit unclassified bucket; product-policy owners T27–T29
remain outside that ledger's semantic ownership domain.

Each area is accountable for its compiler/runtime semantics, relevant source
contracts, meaningful native regression controls, and failures attributed to
that area. An owner code does not indicate support or acceptance. Reports must
also retain the observed outcome, reason, source identity and evidence.

Specific invariants live in the [technical contracts](contracts/), with
verification commands and evidence policy in [contribution guide](../../CONTRIBUTING.md#verification).

<a id="t00"></a>

## T00 — Contribution protocol

[Acceptance criteria](conformance-acceptance.md#t00)

<a id="t01"></a>

## T01 — Baseline and failure evidence

[Acceptance criteria](conformance-acceptance.md#t01)

Technical contracts:

- [snapshot-compiler-provenance](contracts/snapshot-compiler-provenance.md)

<a id="t02"></a>

## T02 — IR and Wasm backend architecture

[Acceptance criteria](conformance-acceptance.md#t02)

Technical contracts:

- [scriptw-ir-module-budget-owner-splits](contracts/scriptw-ir-module-budget-owner-splits.md)
- [native-host-gc-values](contracts/native-host-gc-values.md)
- [data-view-access-owner](contracts/data-view-access-owner.md)
- [json-reviver-frame](contracts/json-reviver-frame.md)
- [module-memory-image-budget](contracts/module-memory-image-budget.md)
- [memory32-allocation-bound](contracts/memory32-allocation-bound.md)
- [object-own-descriptor-predicate-kind](contracts/object-own-descriptor-predicate-kind.md)
- [object-builtin-policy-domains](contracts/object-builtin-policy-domains.md)
- [object-get-own-property-descriptor-owner](contracts/object-get-own-property-descriptor-owner.md)
- [object-get-own-property-descriptors-owner](contracts/object-get-own-property-descriptors-owner.md)
- [object-assign-owner](contracts/object-assign-owner.md)
- [weak-ref-created-realm-publication](contracts/weak-ref-created-realm-publication.md)
- [atomics-builtin-dispatch-boundary](contracts/atomics-builtin-dispatch-boundary.md)
- [duplicate-named-group-pattern](contracts/duplicate-named-group-pattern.md)
- [global-ascii-class-quantifier](contracts/global-ascii-class-quantifier.md)
- [postal-code-match-result-shape](contracts/postal-code-match-result-shape.md)
- [string-literal-replacement-scope](contracts/string-literal-replacement-scope.md)
- [map-collection-weak-key-admission](contracts/map-collection-weak-key-admission.md)
- [host-import-function-indices-authority](contracts/host-import-function-indices-authority.md)
- [temporal-zoned-date-time-direction-dispatch](contracts/temporal-zoned-date-time-direction-dispatch.md)
- [intl-locale-string-slot-dispatch](contracts/intl-locale-string-slot-dispatch.md)
- [array-sort-output](contracts/native-array-gc-methods.md)
- [array-callback-receiver-kind](contracts/native-array-gc-methods.md)
- [array-find-via-predicate](contracts/array-find-via-predicate.md)
- [ecmascript-trim-mode](contracts/ecmascript-trim-mode.md)
- [obsolete-builtin-emitter-removal](contracts/obsolete-builtin-emitter-removal.md)
- [obsolete-core-backend-api-removal](contracts/obsolete-core-backend-api-removal.md)
- [obsolete-planning-analysis-removal](contracts/obsolete-planning-analysis-removal.md)
- [shadowed-property-read-arm-removal](contracts/shadowed-property-read-arm-removal.md)
- [obsolete-lowering-specialization-removal](contracts/obsolete-lowering-specialization-removal.md)
- [obsolete-static-generator-cache-removal](contracts/obsolete-static-generator-cache-removal.md)
- [plain-async-synchronous-for-of-lexical-pattern-heads](contracts/plain-async-synchronous-for-of-lexical-pattern-heads.md)
- [compiled-module-package-ownership](contracts/compiled-module-package-ownership.md)

<a id="t03"></a>

## T03 — Test262 harness integrity and host contract

[Acceptance criteria](conformance-acceptance.md#t03)

Technical contracts:

- [compiler-source-identity](contracts/compiler-source-identity.md)
- [test262-source-execution](contracts/test262-source-execution.md)
- [test262-case-deadline](contracts/test262-case-deadline.md)
- [snapshot-compiler-provenance](contracts/snapshot-compiler-provenance.md)
- [fake-suite-publication](contracts/fake-suite-publication.md)
- [test262-snapshot-use](contracts/test262-snapshot-use.md)
- [test262-wasm-aot-execution-stack](contracts/test262-wasm-aot-execution-stack.md)
- [test262-checkpoint-run-identity-admission](contracts/test262-checkpoint-run-identity-admission.md)
- [test262-attempt-journal-strike-state](contracts/test262-attempt-journal-strike-state.md)

<a id="t04"></a>

## T04 — Shared ECMAScript operations and completion ABI

[Acceptance criteria](conformance-acceptance.md#t04)

Technical contracts:

- [primitive-property-read-effects](contracts/primitive-property-read-effects.md)
- [numeric-native-caller-effects](contracts/numeric-native-caller-effects.md)
- [global-error-caller-effects](contracts/global-error-caller-effects.md)
- [reflect-to-property-descriptor-owner](contracts/reflect-to-property-descriptor-owner.md)
- [array-species-consumer-ownership](contracts/array-species-consumer-ownership.md)
- [numeric-conversion-realm-projection-capability](contracts/numeric-conversion-realm-projection-capability.md)
- [try-clause-empty-completion-seed](contracts/try-clause-empty-completion-seed.md)
- [async-generator-complete-step-kind](contracts/async-generator-complete-step-kind.md)
- [conversion-error-realm-source-lifecycle](contracts/conversion-error-realm-source-lifecycle.md)
- [wasm-top-level-completion-kind](contracts/wasm-top-level-completion-kind.md)
- [shape-accessor-reference-selection](contracts/shape-accessor-reference-selection.md)

<a id="t05"></a>

## T05 — Value representation, heap, GC and weak reachability

[Acceptance criteria](conformance-acceptance.md#t05)

Technical contracts:

- [collector-weak-edges](contracts/collector-weak-edges.md)
- [temporal-gc-values](contracts/temporal-gc-values.md)
- [native-host-gc-values](contracts/native-host-gc-values.md)
- [weak-unavailable-runtime-boundary](contracts/weak-unavailable-runtime-boundary.md)
- [memory32-allocation-bound](contracts/memory32-allocation-bound.md)
- [function-module-state](contracts/function-module-state.md)
- [heap-named-slot-storage](contracts/heap-named-slot-storage.md)
- [heap-side-storage-element](contracts/heap-side-storage-element.md)
- [temporal-instant-heap-slot-authority](contracts/temporal-instant-heap-slot-authority.md)
- [weak-ref-heap-slot-authority](contracts/weak-ref-heap-slot-authority.md)
- [private-environment-heap-slot-authority](contracts/private-environment-heap-slot-authority.md)
- [set-entry-heap-slot-authority](contracts/set-entry-heap-slot-authority.md)
- [weak-set-entry-heap-slot-authority](contracts/weak-set-entry-heap-slot-authority.md)
- [map-iterator-heap-slot-authority](contracts/map-iterator-heap-slot-authority.md)
- [set-iterator-heap-slot-authority](contracts/set-iterator-heap-slot-authority.md)
- [set-record-heap-slot-authority](contracts/set-record-heap-slot-authority.md)
- [weak-set-record-heap-slot-authority](contracts/weak-set-record-heap-slot-authority.md)
- [weak-map-record-heap-slot-authority](contracts/weak-map-record-heap-slot-authority.md)
- [map-record-heap-slot-authority](contracts/map-record-heap-slot-authority.md)
- [map-entry-heap-slot-authority](contracts/map-entry-heap-slot-authority.md)
- [weak-map-entry-heap-slot-authority](contracts/weak-map-entry-heap-slot-authority.md)
- [async-generator-object-heap-slot-authority](contracts/async-generator-object-heap-slot-authority.md)
- [environment-heap-slot-authority](contracts/environment-heap-slot-authority.md)
- [string-heap-slot-authority](contracts/string-heap-slot-authority.md)
- [bigint-heap-slot-authority](contracts/bigint-heap-slot-authority.md)
- [symbol-heap-slot-authority](contracts/symbol-heap-slot-authority.md)
- [typed-array-iterator-heap-slot-authority](contracts/typed-array-iterator-heap-slot-authority.md)
- [temporal-plain-date-heap-slot-authority](contracts/temporal-plain-date-heap-slot-authority.md)
- [disposable-stack-record-heap-slot-authority](contracts/disposable-stack-record-heap-slot-authority.md)
- [async-disposable-stack-record-heap-slot-authority](contracts/async-disposable-stack-record-heap-slot-authority.md)
- [disposable-stack-entry-heap-slot-authority](contracts/disposable-stack-entry-heap-slot-authority.md)
- [async-disposable-stack-entry-heap-slot-authority](contracts/async-disposable-stack-entry-heap-slot-authority.md)
- [pending-completion-heap-slot-authority](contracts/pending-completion-heap-slot-authority.md)
- [atomics-async-waiter-heap-slot-authority](contracts/atomics-async-waiter-heap-slot-authority.md)
- [bound-function-heap-slot-authority](contracts/bound-function-heap-slot-authority.md)
- [finalization-registry-record-heap-slot-authority](contracts/finalization-registry-record-heap-slot-authority.md)
- [finalization-registry-cell-heap-slot-authority](contracts/finalization-registry-cell-heap-slot-authority.md)
- [promise-capability-heap-slot-authority](contracts/promise-capability-heap-slot-authority.md)
- [pending-job-heap-slot-authority](contracts/pending-job-heap-slot-authority.md)
- [class-function-context-heap-slot-authority](contracts/class-function-context-heap-slot-authority.md)
- [private-element-entry-heap-slot-authority](contracts/private-element-entry-heap-slot-authority.md)
- [temporal-zoned-date-time-heap-slot-authority](contracts/temporal-zoned-date-time-heap-slot-authority.md)
- [promise-reaction-heap-slot-authority](contracts/promise-reaction-heap-slot-authority.md)
- [intl-locale-heap-slot-authority](contracts/intl-locale-heap-slot-authority.md)
- [object-entry-heap-slot-authority](contracts/object-entry-heap-slot-authority.md)
- [realm-record-heap-slot-authority](contracts/realm-record-heap-slot-authority.md)
- [temporal-plain-time-heap-slot-authority](contracts/temporal-plain-time-heap-slot-authority.md)
- [temporal-plain-date-time-heap-slot-authority](contracts/temporal-plain-date-time-heap-slot-authority.md)
- [temporal-duration-heap-slot-authority](contracts/temporal-duration-heap-slot-authority.md)
- [intl-date-time-format-heap-slot-authority](contracts/intl-date-time-format-heap-slot-authority.md)
- [heap-root-source-authority](contracts/heap-root-source-authority.md)
- [heap-collector-phase-authority](contracts/heap-collector-phase-authority.md)
- [heap-host-boundary-policy](contracts/heap-host-boundary-policy.md)
- [heap-value-encoding-authority](contracts/heap-value-encoding-authority.md)
- [heap-weak-edge-identity-authority](contracts/heap-weak-edge-identity-authority.md)
- [heap-collector-policy-authority](contracts/heap-collector-policy-authority.md)
- [local-declaration-authority](contracts/local-declaration-authority.md)

<a id="t06"></a>

## T06 — Realms, intrinsics and cross-realm semantics

[Acceptance criteria](conformance-acceptance.md#t06)

Technical contracts:

- [shadowrealm-implementation-sequence](contracts/shadowrealm-implementation-sequence.md)
- [template-site-source-ownership](contracts/template-site-source-ownership.md)
- [reflect-descriptor-object-realm](contracts/reflect-descriptor-object-realm.md)
- [numeric-conversion-realm-projection-capability](contracts/numeric-conversion-realm-projection-capability.md)
- [get-function-realm-outcome-code](contracts/get-function-realm-outcome-code.md)
- [promise-species-realm-context](contracts/promise-species-realm-context.md)
- [promise-prototype-receiver-error-realm](contracts/promise-prototype-receiver-error-realm.md)
- [array-from-async-result-definition-error-realm](contracts/array-from-async-result-definition-error-realm.md)
- [promise-resolve-realm-context](contracts/promise-resolve-realm-context.md)
- [array-from-async-promise-realm-context](contracts/array-from-async-promise-realm-context.md)
- [array-from-async-internal-callback-realm-context](contracts/array-from-async-internal-callback-realm-context.md)
- [promise-combinator-algorithm-error-realm](contracts/promise-combinator-algorithm-error-realm.md)
- [weak-ref-created-realm-publication](contracts/weak-ref-created-realm-publication.md)
- [finalization-registry-created-realm-publication](contracts/finalization-registry-created-realm-publication.md)
- [weak-collection-created-realm-publication](contracts/weak-collection-created-realm-publication.md)
- [realm-id-allocation](contracts/realm-id-allocation.md)

<a id="t07"></a>

## T07 — Parser boundary, grammar coverage and early errors

[Acceptance criteria](conformance-acceptance.md#t07)

Technical contracts:

- [short-circuit-previous-expression-exhaustiveness](contracts/short-circuit-previous-expression-exhaustiveness.md)
- [module-binding-names](contracts/module-binding-names.md)

<a id="t08"></a>

## T08 — Environments, references, control flow and abrupt completion

[Acceptance criteria](conformance-acceptance.md#t08)

Technical contracts:

- [object-binding-single-get](contracts/object-binding-single-get.md)
- [plain-generator-for-of-lexical-pattern-heads](contracts/plain-generator-for-of-lexical-pattern-heads.md)
- [plain-generator-synchronous-for-of-continuations](contracts/plain-generator-synchronous-for-of-continuations.md)
- [ordinary-property-logical-assignment-reference](contracts/ordinary-property-logical-assignment-reference.md)
- [async-for-of-array-walk-form-ownership](contracts/async-for-of-array-walk-form-ownership.md)
- [function-arguments-binding-ownership](contracts/function-arguments-binding-ownership.md)
- [fresh-script-restricted-global-lexicals](contracts/fresh-script-restricted-global-lexicals.md)

<a id="t09"></a>

## T09 — Functions, constructors, classes and private elements

[Acceptance criteria](conformance-acceptance.md#t09)

Technical contracts:

- [generator-object-patterns](contracts/generator-object-patterns.md)
- [class-name-source](contracts/class-name-source.md)
- [generator-optional-regions](contracts/generator-optional-regions.md)
- [remaining-invocation-reference-ownership](contracts/remaining-invocation-reference-ownership.md)
- [native-function-source-syntax](contracts/native-function-source-syntax.md)
- [class-auto-accessors](contracts/class-auto-accessors.md)
- [function-prototype-materialization](contracts/function-prototype-materialization.md)
- [class-field-named-evaluation](contracts/class-field-named-evaluation.md)

<a id="t10"></a>

## T10 — Object model, descriptors and exotic-object protocol

[Acceptance criteria](conformance-acceptance.md#t10)

Technical contracts:

- [reflect-to-property-descriptor-owner](contracts/reflect-to-property-descriptor-owner.md)
- [proxy-revocation-route-ownership](contracts/proxy-revocation-route-ownership.md)
- [object-get-own-property-descriptor-owner](contracts/object-get-own-property-descriptor-owner.md)
- [object-get-own-property-descriptors-owner](contracts/object-get-own-property-descriptors-owner.md)
- [object-assign-owner](contracts/object-assign-owner.md)
- [object-read-realm-projection-capability](contracts/object-read-realm-projection-capability.md)

<a id="t11"></a>

## T11 — Proxy and Reflect meta-object protocol

[Acceptance criteria](conformance-acceptance.md#t11)

Technical contracts:

- [object-define-property-entry-order](contracts/object-define-property-entry-order.md)
- [proxy-target-descriptor-completion](contracts/proxy-target-descriptor-completion.md)
- [proxy-own-keys-result-ownership](contracts/proxy-own-keys-result-ownership.md)
- [proxy-prevent-extensions-request](contracts/proxy-prevent-extensions-request.md)
- [reflect-to-property-descriptor-owner](contracts/reflect-to-property-descriptor-owner.md)

<a id="t12"></a>

## T12 — Modules, linking, loading and namespace objects

[Acceptance criteria](conformance-acceptance.md#t12)

Technical contracts:

- [module-async-lifecycle](contracts/module-async-lifecycle.md)
- [json-module-native-default-export](contracts/json-module-native-default-export.md)
- [source-text-module-source-phase-rejection](contracts/source-text-module-source-phase-rejection.md)
- [script-computed-import-admission](contracts/script-computed-import-admission.md)
- [script-dynamic-import-jobs](contracts/script-dynamic-import-jobs.md)
- [dynamic-import-dispatcher-reference-ownership](contracts/dynamic-import-dispatcher-reference-ownership.md)
- [module-binding-names](contracts/module-binding-names.md)

<a id="t13"></a>

## T13 — Dynamic source evaluation: `eval`, `Function` and realm evaluation

[Acceptance criteria](conformance-acceptance.md#t13)

Technical contracts:

- [shadowrealm-finite-evaluate](contracts/shadowrealm-finite-evaluate.md)
- [template-site-source-ownership](contracts/template-site-source-ownership.md)
- [prepared-dynamic-functions](contracts/prepared-dynamic-functions.md)
- [prepared-direct-eval](contracts/prepared-direct-eval.md)
- [direct-eval-environment-records](contracts/direct-eval-environment-records.md)
- [precompiled-realm-scripts](contracts/precompiled-realm-scripts.md)
- [dynamic-source-capability](contracts/dynamic-source-capability.md)

<a id="t14"></a>

## T14 — Promise jobs, async functions and async iteration

[Acceptance criteria](conformance-acceptance.md#t14)

Technical contracts:

- [shadowrealm-implementation-sequence](contracts/shadowrealm-implementation-sequence.md)
- [plain-async-classic-regions](contracts/plain-async-classic-regions.md)
- [plain-async-for-await-body-continuations](contracts/plain-async-for-await-body-continuations.md)
- [plain-async-switch-continuations](contracts/plain-async-switch-continuations.md)
- [async-for-of-continuations](contracts/async-for-of-continuations.md)
- [grouped-optional-reference-await-ownership](contracts/grouped-optional-reference-await-ownership.md)
- [grouped-optional-call-value-yield-ownership](contracts/grouped-optional-call-value-yield-ownership.md)
- [grouped-optional-call-value-await-ownership](contracts/grouped-optional-call-value-await-ownership.md)
- [plain-generator-value-branch-ownership](contracts/plain-generator-value-branch-ownership.md)
- [suspended-call-argument-ownership](contracts/suspended-call-argument-ownership.md)
- [conditional-await-expression-ownership](contracts/conditional-await-expression-ownership.md)
- [logical-await-expression-ownership](contracts/logical-await-expression-ownership.md)
- [optional-property-await-ownership](contracts/optional-property-await-ownership.md)
- [optional-call-await-ownership](contracts/optional-call-await-ownership.md)
- [plain-async-awaited-while-condition](contracts/plain-async-awaited-while-condition.md)
- [logical-assignment-await-reference-ownership](contracts/logical-assignment-await-reference-ownership.md)
- [plain-async-labelled-region](contracts/plain-async-labelled-region.md)
- [array-from-async-iterator-result-property-domain](contracts/array-from-async-iterator-result-property-domain.md)
- [array-from-async-result-definition-error-realm](contracts/array-from-async-result-definition-error-realm.md)
- [promise-job-to-enqueue-ownership](contracts/promise-job-to-enqueue-ownership.md)
- [promise-reaction-initialization](contracts/promise-reaction-initialization.md)
- [promise-species-realm-context](contracts/promise-species-realm-context.md)
- [promise-prototype-receiver-error-realm](contracts/promise-prototype-receiver-error-realm.md)
- [promise-resolve-realm-context](contracts/promise-resolve-realm-context.md)
- [promise-resolve-realm-authority-ownership](contracts/promise-resolve-realm-authority-ownership.md)
- [array-from-async-promise-realm-context](contracts/array-from-async-promise-realm-context.md)
- [array-from-async-internal-callback-realm-context](contracts/array-from-async-internal-callback-realm-context.md)
- [promise-combinator-algorithm-error-realm](contracts/promise-combinator-algorithm-error-realm.md)
- [promise-combinator-mode-domains](contracts/promise-combinator-mode-domains.md)

<a id="t15"></a>

## T15 — Generators, iterators, iterator helpers and resource management

[Acceptance criteria](conformance-acceptance.md#t15)

Technical contracts:

- [generator-object-patterns](contracts/generator-object-patterns.md)
- [class-name-source](contracts/class-name-source.md)
- [generator-optional-regions](contracts/generator-optional-regions.md)
- [object-binding-single-get](contracts/object-binding-single-get.md)
- [plain-generator-for-of-lexical-pattern-heads](contracts/plain-generator-for-of-lexical-pattern-heads.md)
- [plain-generator-synchronous-for-of-continuations](contracts/plain-generator-synchronous-for-of-continuations.md)
- [iterator-from-completed-record](contracts/iterator-from-completed-record.md)
- [aggregate-error-iterator-list](contracts/aggregate-error-iterator-list.md)
- [sync-iterator-consumer-capability](contracts/sync-iterator-consumer-capability.md)
- [plain-async-switch-continuations](contracts/plain-async-switch-continuations.md)
- [grouped-optional-call-value-yield-ownership](contracts/grouped-optional-call-value-yield-ownership.md)
- [suspended-call-argument-ownership](contracts/suspended-call-argument-ownership.md)
- [conditional-await-expression-ownership](contracts/conditional-await-expression-ownership.md)
- [plain-generator-value-branch-ownership](contracts/plain-generator-value-branch-ownership.md)
- [generator-optional-chain-yield-ownership](contracts/generator-optional-chain-yield-ownership.md)
- [remaining-invocation-reference-ownership](contracts/remaining-invocation-reference-ownership.md)
- [async-generator-complete-step-kind](contracts/async-generator-complete-step-kind.md)
- [generator-delegate-property-capability](contracts/generator-delegate-property-capability.md)
- [sync-dispose-completion-continuation](contracts/sync-dispose-completion-continuation.md)
- [prepared-destructuring-target](contracts/prepared-destructuring-target.md)
- [destructuring-iterator-locals-ownership](contracts/destructuring-iterator-locals-ownership.md)
- [sync-iterator-locals-release-ownership](contracts/sync-iterator-locals-release-ownership.md)
- [iterator-close-error-realm](contracts/iterator-close-error-realm.md)
- [direct-synchronous-for-of-protocol-error-realm](contracts/direct-synchronous-for-of-protocol-error-realm.md)
- [async-disposable-stack-dispose-completion-kind](contracts/async-disposable-stack-dispose-completion-kind.md)
- [synchronous-array-for-of-iterator-protocol](contracts/synchronous-array-for-of-iterator-protocol.md)
- [plain-async-synchronous-for-of-member-heads](contracts/plain-async-synchronous-for-of-member-heads.md)
- [plain-async-synchronous-for-of-nonlexical-pattern-heads](contracts/plain-async-synchronous-for-of-nonlexical-pattern-heads.md)
- [plain-async-synchronous-for-of-lexical-pattern-heads](contracts/plain-async-synchronous-for-of-lexical-pattern-heads.md)
- [gc-iterator-helper-execution](contracts/gc-iterator-helper-execution.md)

<a id="t16"></a>

## T16 — Array exotic semantics and complete Array API

[Acceptance criteria](conformance-acceptance.md#t16)

Technical contracts:

- [invocation-shortcut-retirement](contracts/invocation-shortcut-retirement.md)
- [indexed-collection-invocation-reference](contracts/indexed-collection-invocation-reference.md)
- [array-splice-algorithm-owner](contracts/native-array-gc-methods.md)
- [array-by-copy-realm](contracts/array-by-copy-realm.md)
- [array-sort-output](contracts/native-array-gc-methods.md)
- [array-species-consumer-ownership](contracts/array-species-consumer-ownership.md)
- [array-flat-forward-traversal](contracts/native-array-gc-methods.md)
- [array-from-async-result-definition-error-realm](contracts/array-from-async-result-definition-error-realm.md)
- [array-quantifier-entry-boundary](contracts/array-quantifier-entry-boundary.md)
- [array-callback-receiver-kind](contracts/native-array-gc-methods.md)
- [array-flat-map-typed-array-buffer-witness](contracts/native-array-gc-methods.md)
- [gc-array-from-async-execution](contracts/gc-array-from-async-execution.md)

<a id="t17"></a>

## T17 — ArrayBuffer, DataView, TypedArray, SharedArrayBuffer and Atomics

[Acceptance criteria](conformance-acceptance.md#t17)

Technical contracts:

- [native-host-gc-values](contracts/native-host-gc-values.md)
- [remaining-invocation-reference-ownership](contracts/remaining-invocation-reference-ownership.md)
- [indexed-collection-invocation-reference](contracts/indexed-collection-invocation-reference.md)
- [typed-array-create-same-type-ownership](contracts/typed-array-create-same-type-ownership.md)
- [typed-array-species-create-ownership](contracts/typed-array-species-create-ownership.md)
- [atomics-access-revalidation](contracts/atomics-access-revalidation.md)
- [data-view-access-owner](contracts/data-view-access-owner.md)
- [numeric-conversion-realm-projection-capability](contracts/numeric-conversion-realm-projection-capability.md)
- [typed-array-intrinsic-prototype-publication](contracts/typed-array-intrinsic-prototype-publication.md)
- [atomics-wait-outcome](contracts/atomics-wait-outcome.md)
- [atomics-integer-operation](contracts/atomics-integer-operation.md)
- [atomics-integer-element-kind-local](contracts/atomics-integer-element-kind-local.md)
- [typed-array-search-kind](contracts/gc-typed-array-native-methods.md)
- [array-callback-receiver-kind](contracts/native-array-gc-methods.md)
- [typed-array-integer-index-buffer-witness](contracts/typed-array-integer-index-buffer-witness.md)
- [array-buffer-slice-source-reobservation](contracts/array-buffer-slice-source-reobservation.md)
- [typed-array-accessor-buffer-witness](contracts/typed-array-accessor-buffer-witness.md)
- [typed-array-iterator-buffer-witness](contracts/typed-array-iterator-buffer-witness.md)
- [typed-array-join-buffer-witness](contracts/gc-typed-array-native-methods.md)
- [typed-array-reverse-family-buffer-witness](contracts/typed-array-reverse-family-buffer-witness.md)
- [typed-array-sort-family-buffer-witness](contracts/typed-array-sort-family-buffer-witness.md)
- [typed-array-find-family-buffer-witness](contracts/typed-array-find-family-buffer-witness.md)
- [typed-array-quantifier-family-buffer-witness](contracts/gc-typed-array-native-methods.md)
- [array-quantifier-entry-boundary](contracts/array-quantifier-entry-boundary.md)
- [typed-array-to-locale-string-buffer-witness](contracts/gc-typed-array-native-methods.md)
- [array-to-locale-string-typed-array-buffer-witness](contracts/native-array-gc-methods.md)
- [array-flat-map-typed-array-buffer-witness](contracts/native-array-gc-methods.md)
- [typed-array-map-filter-buffer-witness](contracts/gc-typed-array-native-methods.md)
- [atomics-typed-array-buffer-witness](contracts/atomics-typed-array-buffer-witness.md)
- [array-concat-typed-array-buffer-witness](contracts/native-array-gc-methods.md)
- [object-get-own-property-names-typed-array-buffer-witness](contracts/object-get-own-property-names-typed-array-buffer-witness.md)
- [typed-array-subarray-buffer-witness](contracts/typed-array-subarray-buffer-witness.md)
- [typed-array-subarray-species-argument-arity](contracts/typed-array-subarray-species-argument-arity.md)
- [atomics-builtin-dispatch-boundary](contracts/atomics-builtin-dispatch-boundary.md)
- [created-realm-data-view-publication-lifecycle](contracts/created-realm-data-view-publication-lifecycle.md)
- [array-buffer-flag-wire-domain](contracts/array-buffer-flag-wire-domain.md)
- [typed-array-length-mode-wire-domain](contracts/typed-array-length-mode-wire-domain.md)

<a id="t18"></a>

## T18 — Strings, Unicode and the complete String API

[Acceptance criteria](conformance-acceptance.md#t18)

Technical contracts:

- [string-invocation-family](contracts/string-invocation-family.md)
- [number-string-hook-and-dispatch-retirement](contracts/number-string-hook-and-dispatch-retirement.md)
- [invocation-shortcut-retirement](contracts/invocation-shortcut-retirement.md)
- [string-symbol-hook-operation](contracts/string-symbol-hook-operation.md)
- [string-concat-method-lookup](contracts/string-concat-method-lookup.md)
- [created-realm-iterator-next-publication](contracts/created-realm-iterator-next-publication.md)

<a id="t19"></a>

## T19 — Complete ECMAScript RegExp semantics

[Acceptance criteria](conformance-acceptance.md#t19)

Technical contracts:

- [regexp-independent-counted-repetition](contracts/regexp-independent-counted-repetition.md)
- [regexp-required-choice-runs](contracts/regexp-required-choice-runs.md)
- [regexp-exact-natural-counter](contracts/regexp-exact-natural-counter.md)
- [regexp-legacy-state](contracts/regexp-legacy-state.md)
- [string-invocation-family](contracts/string-invocation-family.md)
- [number-string-hook-and-dispatch-retirement](contracts/number-string-hook-and-dispatch-retirement.md)
- [string-symbol-hook-operation](contracts/string-symbol-hook-operation.md)
- [runtime-regexp-unicode-character-domain](contracts/runtime-regexp-unicode-character-domain.md)
- [runtime-regexp-braced-unicode-escape](contracts/runtime-regexp-braced-unicode-escape.md)
- [runtime-regexp-named-unicode-reference](contracts/runtime-regexp-named-unicode-reference.md)
- [regexp-native-unicode-property-alias](contracts/regexp-native-unicode-property-alias.md)
- [runtime-regexp-codepoint-property](contracts/runtime-regexp-codepoint-property.md)
- [runtime-regexp-computed-unicode-sets](contracts/runtime-regexp-computed-unicode-sets.md)
- [runtime-regexp-finite-string-catalog](contracts/runtime-regexp-finite-string-catalog.md)
- [regexp-program-boundary](contracts/regexp-program-boundary.md)
- [runtime-regexp-named-capture-inventory](contracts/runtime-regexp-named-capture-inventory.md)
- [runtime-regexp-lookbehind-direction](contracts/runtime-regexp-lookbehind-direction.md)
- [regexp-runtime-capability-rejection](contracts/regexp-runtime-capability-rejection.md)
- [regexp-constructor-realm-prototype](contracts/regexp-constructor-realm-prototype.md)
- [regexp-matcher-result-domain](contracts/regexp-matcher-result-domain.md)
- [runtime-regexp-entry-kind-capability](contracts/runtime-regexp-entry-kind-capability.md)
- [regexp-unicode-class-escape-grammar](contracts/regexp-unicode-class-escape-grammar.md)
- [regexp-legacy-direct-astral-quantifier](contracts/regexp-legacy-direct-astral-quantifier.md)
- [regexp-lookbehind-polarity](contracts/regexp-lookbehind-polarity.md)
- [regexp-unicode-set-expression-shape](contracts/regexp-unicode-set-expression-shape.md)
- [regexp-range-bound-domain](contracts/regexp-range-bound-domain.md)
- [regexp-substitution-kind](contracts/regexp-substitution-kind.md)
- [duplicate-named-group-pattern](contracts/duplicate-named-group-pattern.md)
- [regexp-exec-result-mode](contracts/regexp-exec-result-mode.md)
- [regexp-flag-getter](contracts/regexp-flag-getter.md)
- [regexp-modifier-override](contracts/regexp-modifier-override.md)

<a id="t20"></a>

## T20 — Number, BigInt, Math and JSON

[Acceptance criteria](conformance-acceptance.md#t20)

Technical contracts:

- [numeric-native-caller-effects](contracts/numeric-native-caller-effects.md)
- [remaining-invocation-reference-ownership](contracts/remaining-invocation-reference-ownership.md)
- [json-stringify-preparation-get](contracts/json-stringify-preparation-get.md)
- [json-reviver-frame](contracts/json-reviver-frame.md)
- [numeric-conversion-realm-projection-capability](contracts/numeric-conversion-realm-projection-capability.md)
- [arithmetic-number-conversion-order](contracts/arithmetic-number-conversion-order.md)
- [intl-numberformat-wasm](contracts/intl-numberformat-wasm.md)
- [bigint-prototype-result-policy](contracts/bigint-prototype-result-policy.md)
- [math-extremum-argument-reduction](contracts/math-extremum-argument-reduction.md)
- [math-hypot-argument-reduction](contracts/math-hypot-argument-reduction.md)
- [math-sum-precise-runtime](contracts/math-sum-precise-runtime.md)
- [math-sum-precise-limb-operation](contracts/math-sum-precise-limb-operation.md)
- [bigint-fixed-width-operation](contracts/bigint-fixed-width-operation.md)
- [bigint-helper-operation-runtime-code](contracts/bigint-helper-operation-runtime-code.md)
- [json-parse-frame-state](contracts/json-parse-frame-state.md)
- [global-numeric-call-coercion-ownership](contracts/global-numeric-call-coercion-ownership.md)

<a id="t21"></a>

## T21 — Symbols, collections, weak collections and weak references

[Acceptance criteria](conformance-acceptance.md#t21)

Technical contracts:

- [collector-weak-edges](contracts/collector-weak-edges.md)
- [primitive-property-read-effects](contracts/primitive-property-read-effects.md)
- [weak-unavailable-runtime-boundary](contracts/weak-unavailable-runtime-boundary.md)
- [completed-set-like-record](contracts/completed-set-like-record.md)
- [ordered-collection-cursors](contracts/ordered-collection-cursors.md)
- [collection-iterator-receiver-validation](contracts/collection-iterator-receiver-validation.md)
- [collection-data-receiver-validation](contracts/collection-data-receiver-validation.md)
- [collection-receiver-error-domain](contracts/collection-receiver-error-domain.md)
- [created-realm-iterator-next-publication](contracts/created-realm-iterator-next-publication.md)
- [collection-algorithm-error-realms](contracts/collection-algorithm-error-realms.md)
- [collection-prototype-to-string-tag](contracts/collection-prototype-to-string-tag.md)
- [set-collection-weak-value-admission](contracts/set-collection-weak-value-admission.md)
- [map-collection-weak-key-admission](contracts/map-collection-weak-key-admission.md)
- [symbol-receiver-operation-ownership](contracts/symbol-receiver-operation-ownership.md)
- [weak-ref-created-realm-publication](contracts/weak-ref-created-realm-publication.md)
- [finalization-registry-error-domain](contracts/finalization-registry-error-domain.md)
- [finalization-registry-cell-state](contracts/finalization-registry-cell-state.md)
- [finalization-registry-created-realm-publication](contracts/finalization-registry-created-realm-publication.md)
- [weak-collection-created-realm-publication](contracts/weak-collection-created-realm-publication.md)
- [group-by-result-kind](contracts/group-by-result-kind.md)
- [set-operation-domains](contracts/set-operation-domains.md)

<a id="t22"></a>

## T22 — Date and Temporal

[Acceptance criteria](conformance-acceptance.md#t22)

Technical contracts:

- [temporal-partial-date-reference-format](contracts/temporal-partial-date-reference-format.md)
- [temporal-east-asian-calendar-ownership](contracts/temporal-east-asian-calendar-ownership.md)
- [temporal-hebrew-calendar-ownership](contracts/temporal-hebrew-calendar-ownership.md)
- [temporal-umalqura-calendar-ownership](contracts/temporal-umalqura-calendar-ownership.md)
- [temporal-islamic-tabular-calendar-ownership](contracts/temporal-islamic-tabular-calendar-ownership.md)
- [temporal-thirteen-month-calendar-ownership](contracts/temporal-thirteen-month-calendar-ownership.md)
- [temporal-persian-calendar-ownership](contracts/temporal-persian-calendar-ownership.md)
- [date-system-time-zone](contracts/date-system-time-zone.md)
- [temporal-selected-rounding-window](contracts/temporal-selected-rounding-window.md)
- [temporal-named-zone-authority](contracts/temporal-named-zone-authority.md)
- [date-component-setter-operation](contracts/date-component-setter-operation.md)
- [temporal-plain-year-month-field-read-mode](contracts/temporal-plain-year-month-field-read-mode.md)
- [temporal-zoned-date-time-field-result](contracts/temporal-zoned-date-time-field-result.md)
- [temporal-zoned-date-time-property-bag-option-order](contracts/temporal-zoned-date-time-property-bag-option-order.md)
- [temporal-plain-time-field-authority](contracts/temporal-plain-time-field-authority.md)
- [temporal-zoned-date-time-direction-dispatch](contracts/temporal-zoned-date-time-direction-dispatch.md)
- [temporal-calendar-carrier-privacy](contracts/temporal-calendar-carrier-privacy.md)
- [temporal-plain-month-day-parsed-year-privacy](contracts/temporal-plain-month-day-parsed-year-privacy.md)
- [temporal-field-offset-table-privacy](contracts/temporal-field-offset-table-privacy.md)
- [temporal-instant-diagnostic-privacy](contracts/temporal-instant-diagnostic-privacy.md)
- [temporal-created-realm-completion](contracts/temporal-created-realm-completion.md)

<a id="t23"></a>

## T23 — Complete ECMA-402 Intl implementation

[Acceptance criteria](conformance-acceptance.md#t23)

Technical contracts:

- [intl-service-projection](contracts/intl-service-projection.md)
- [intl-bundle-export](contracts/intl-bundle-export.md)
- [intl-custom-manifest](contracts/intl-custom-manifest.md)
- [intl-currency-projection](contracts/intl-currency-projection.md)
- [intl-calendar-projection](contracts/intl-calendar-projection.md)
- [intl-numbering-projection](contracts/intl-numbering-projection.md)
- [intl-named-zone-projection](contracts/intl-named-zone-projection.md)
- [intl-conformance-data-image](contracts/intl-conformance-data-image.md)
- [intl-custom-projection-selection](contracts/intl-custom-projection-selection.md)
- [intl-numberformat-wasm](contracts/intl-numberformat-wasm.md)
- [intl-display-relative-wasm](contracts/intl-display-relative-wasm.md)
- [intl-pluralrules-wasm](contracts/intl-pluralrules-wasm.md)
- [intl-date-time-format-mode-capability](contracts/intl-date-time-format-mode-capability.md)
- [intl-date-time-format-receiver-operation](contracts/intl-date-time-format-receiver-operation.md)
- [intl-date-time-format-range-locals-ownership](contracts/intl-date-time-format-range-locals-ownership.md)
- [intl-locale-string-slot-dispatch](contracts/intl-locale-string-slot-dispatch.md)
- [intl-canonical-locale-tag-invocation-authority](contracts/intl-canonical-locale-tag-invocation-authority.md)
- [intl-date-time-format-option-privacy](contracts/intl-date-time-format-option-privacy.md)
- [intl-date-time-format-time-zone-name-style-privacy](contracts/intl-date-time-format-time-zone-name-style-privacy.md)
- [intl-date-time-format-temporal-kind-privacy](contracts/intl-date-time-format-temporal-kind-privacy.md)
- [intl-date-time-format-extension-key-privacy](contracts/intl-date-time-format-extension-key-privacy.md)
- [intl-date-time-format-time-zone-authority-privacy](contracts/intl-date-time-format-time-zone-authority-privacy.md)
- [intl-date-time-format-heap-slot-authority](contracts/intl-date-time-format-heap-slot-authority.md)
- [intl-segmenter-wasm](contracts/intl-segmenter-wasm.md)
- [intl-duration-format-wasm](contracts/intl-duration-format-wasm.md)

<a id="t24"></a>

## T24 — Globals, native errors, Annex B and remaining host-visible builtins

[Acceptance criteria](conformance-acceptance.md#t24)

Technical contracts:

- [error-stack-accessors](contracts/error-stack-accessors.md)
- [shadowrealm-implementation-sequence](contracts/shadowrealm-implementation-sequence.md)
- [shadowrealm-finite-evaluate](contracts/shadowrealm-finite-evaluate.md)
- [native-host-gc-values](contracts/native-host-gc-values.md)
- [global-error-caller-effects](contracts/global-error-caller-effects.md)
- [aggregate-error-iterator-list](contracts/aggregate-error-iterator-list.md)
- [runtime-error-message-domain](contracts/runtime-error-message-domain.md)
- [remaining-invocation-reference-ownership](contracts/remaining-invocation-reference-ownership.md)
- [aggregate-error-construction-phases](contracts/aggregate-error-construction-phases.md)
- [boolean-prototype-operation](contracts/boolean-prototype-operation.md)
- [global-numeric-builtin-capability](contracts/global-numeric-builtin-capability.md)
- [error-builtin-dispatch-ownership](contracts/error-builtin-dispatch-ownership.md)
- [thrown-error-diagnostic-kind-authority](contracts/thrown-error-diagnostic-kind-authority.md)

<a id="t25"></a>

## T25 — Differential testing, fuzzing and performance discipline

[Acceptance criteria](conformance-acceptance.md#t25)

Technical contracts:

- [runtime-artifact-persistence](contracts/runtime-artifact-persistence.md)
- [differential-robustness](contracts/differential-robustness.md)
- [verification-memory-budget](contracts/verification-memory-budget.md)
- [runtime-performance-profile](contracts/runtime-performance-profile.md)
- [differential-rooted-completion-snapshot](contracts/differential-rooted-completion-snapshot.md)
- [performance-evidence-aggregation](contracts/performance-evidence-aggregation.md)
- [performance-budget-checks](contracts/performance-budget-checks.md)
- [differential-generated-negative-source](contracts/differential-generated-negative-source.md)
- [differential-generated-scenarios](contracts/differential-generated-scenarios.md)
- [differential-generated-control-flow](contracts/differential-generated-control-flow.md)
- [differential-object-mutations](contracts/differential-object-mutations.md)
- [differential-generated-campaign](contracts/differential-generated-campaign.md)
- [differential-worker-lifecycle](contracts/differential-worker-lifecycle.md)
- [differential-whole-corpus](contracts/differential-whole-corpus.md)
- [performance-reporting](contracts/performance-reporting.md)
- [compiler-performance-profile](contracts/compiler-performance-profile.md)
- [differential-selected-object-probes](contracts/differential-selected-object-probes.md)
- [portable-performance-corpus](contracts/portable-performance-corpus.md)
- [differential-embedded-module-graph](contracts/differential-embedded-module-graph.md)
- [differential-integer-bitwise-grammar](contracts/differential-integer-bitwise-grammar.md)
- [differential-integer-product-grammar](contracts/differential-integer-product-grammar.md)
- [test262-admitted-case-ownership](contracts/test262-admitted-case-ownership.md)
- [differential-oracle-compile-boundary](contracts/differential-oracle-compile-boundary.md)

<a id="t26"></a>

## T26 — Conformance release gate

[Acceptance criteria](conformance-acceptance.md#t26)

Technical contracts:

- [conformance-closure](contracts/conformance-closure.md)
- [differential-worker-lifecycle](contracts/differential-worker-lifecycle.md)
- [plain-async-for-await-body-continuations](contracts/plain-async-for-await-body-continuations.md)
- [numeric-native-caller-effects](contracts/numeric-native-caller-effects.md)
- [object-binding-single-get](contracts/object-binding-single-get.md)
- [plain-generator-for-of-lexical-pattern-heads](contracts/plain-generator-for-of-lexical-pattern-heads.md)
- [snapshot-compiler-provenance](contracts/snapshot-compiler-provenance.md)
- [verification-memory-budget](contracts/verification-memory-budget.md)
- [global-error-caller-effects](contracts/global-error-caller-effects.md)
- [remaining-invocation-reference-ownership](contracts/remaining-invocation-reference-ownership.md)
- [number-string-hook-and-dispatch-retirement](contracts/number-string-hook-and-dispatch-retirement.md)
- [string-invocation-family](contracts/string-invocation-family.md)
- [invocation-shortcut-retirement](contracts/invocation-shortcut-retirement.md)
- [indexed-collection-invocation-reference](contracts/indexed-collection-invocation-reference.md)
- [plain-generator-synchronous-for-of-continuations](contracts/plain-generator-synchronous-for-of-continuations.md)
- [array-splice-algorithm-owner](contracts/native-array-gc-methods.md)
- [iterator-from-completed-record](contracts/iterator-from-completed-record.md)
- [aggregate-error-iterator-list](contracts/aggregate-error-iterator-list.md)
- [module-memory-image-budget](contracts/module-memory-image-budget.md)

<a id="t27"></a>

## T27 — Interpreter quarantine and Wasm-AOT product default

[Acceptance criteria](conformance-acceptance.md#t27)

Technical contracts:

- [execution-backend-routing](contracts/execution-backend-routing.md)

<a id="t28"></a>

## T28 — Legacy retirement boundary

[Acceptance criteria](conformance-acceptance.md#t28)

<a id="t29"></a>

## T29 — Canonical product identity

[Acceptance criteria](conformance-acceptance.md#t29)

