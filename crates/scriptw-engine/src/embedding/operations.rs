//! Operations execute compiler-emitted bridges using the owning Realm/program.
use super::async_host::ForeignAsyncSettlement;
use super::foreign_host::{allocate_array, copy_array};
use super::values::{dynamic_completion, retained_completion, retained_value};
use super::*;
use scriptw_aot_wasm::GcHostField;
use wasmtime::{
    ArrayRef, ArrayRefPre, ArrayType, EqRef, Func, Global, OwnedRooted, Rooted, StorageType,
    StructRef, StructRefPre, TypedFunc, Val,
};

/// Store-local bridge handles. The typed call bridge and its GC allocators
/// are resolved on the first retained call, including a call by `pump` to
/// settle a foreign operation.
pub(super) struct Bridges {
    call: OnceLock<CallBridge>,
    promise: Option<(Func, usize)>,
    realm: Option<Global>,
    pub(super) module_status: Option<WasmtimeExtern>,
    pub(super) prelude_phase: Option<Global>,
}

/// `scriptw.embed.call`'s exact parameters: callee and receiver as
/// (tag, scalar, eqref) values, the canonical argument array, then the entry
/// Realm, its global Environment and the defining program.
type CallParameters = (
    i32,
    i64,
    Option<Rooted<EqRef>>,
    i32,
    i64,
    Option<Rooted<EqRef>>,
    Rooted<ArrayRef>,
    Rooted<EqRef>,
    Option<Rooted<EqRef>>,
    Rooted<EqRef>,
);

/// The call bridge, typed once so a call neither checks a `Val` slice
/// against the bridge's `FuncType` nor allocates parameter/result vectors.
struct CallBridge {
    function: TypedFunc<CallParameters, GcMainCompletion>,
    arguments: ArrayRefPre,
    stored: StructRefPre,
    /// One empty argument array allocated on the first zero-argument call.
    /// A zero-length Wasm GC array has no element a callee could write, and
    /// nothing compares argument arrays by identity. R itself shares one
    /// empty `ValueArray` for every argument-free entry.
    no_arguments: OnceLock<OwnedRooted<ArrayRef>>,
}

impl Bridges {
    pub(super) fn new(
        store: &mut impl AsContextMut<Data = WasmStoreState>,
        program: &Instance,
        roots: &Instance,
        modules: &wasm_runtime_link::WasmModules,
    ) -> Self {
        let promise = modules
            .roots_embed_promise_export()
            .and_then(|(export, results)| {
                roots
                    .get_module_export(&mut *store, export)
                    .and_then(WasmtimeExtern::into_func)
                    .map(|function| (function, results))
            });
        let realm = modules
            .roots_snapshot_entry_realm_export()
            .and_then(|export| roots.get_module_export(&mut *store, export))
            .and_then(WasmtimeExtern::into_global);
        let module_status = modules
            .program_module_status_export()
            .and_then(|export| program.get_module_export(&mut *store, export));
        let prelude_phase = modules
            .program_prelude_phase_export()
            .and_then(|export| program.get_module_export(&mut *store, export))
            .and_then(WasmtimeExtern::into_global);
        Bridges {
            call: OnceLock::new(),
            promise,
            realm,
            module_status,
            prelude_phase,
        }
    }

    /// Whether the call bridge has been typed and retained.
    #[cfg(test)]
    pub(super) fn has_typed_call(&self) -> bool {
        self.call.get().is_some()
    }
}

impl CallBridge {
    fn new(
        store: &mut impl AsContextMut<Data = WasmStoreState>,
        function: Func,
    ) -> Result<Self, EngineError> {
        let ty = function.ty(&*store);
        let arguments: ArrayType = ty
            .params()
            .nth(6)
            .and_then(|p| {
                p.as_ref()
                    .and_then(|r| r.heap_type().as_concrete_array())
                    .cloned()
            })
            .ok_or_else(|| EngineError::new("embedding call has no canonical argument array"))?;
        let StorageType::ValType(ValType::Ref(element)) = arguments.element_type() else {
            return Err(EngineError::new(
                "embedding argument array has wrong storage",
            ));
        };
        let stored = element
            .heap_type()
            .as_concrete_struct()
            .cloned()
            .ok_or_else(|| EngineError::new("embedding argument is not a StoredValue"))?;
        let function = function
            .typed::<CallParameters, GcMainCompletion>(&*store)
            .map_err(|error| EngineError::new(format!("embedding call signature: {error:#}")))?;
        Ok(CallBridge {
            function,
            arguments: ArrayRefPre::new(&mut *store, arguments),
            stored: StructRefPre::new(&mut *store, stored),
            no_arguments: OnceLock::new(),
        })
    }

    fn empty_arguments(
        &self,
        store: &mut impl AsContextMut<Data = WasmStoreState>,
    ) -> Result<Rooted<ArrayRef>, EngineError> {
        if let Some(empty) = self.no_arguments.get() {
            return Ok(empty.to_rooted(&mut *store));
        }
        let empty = ArrayRef::new_fixed(&mut *store, &self.arguments, &[])
            .map_err(embedded_error)?;
        let owned = empty
            .to_owned_rooted(&mut *store)
            .map_err(embedded_error)?;
        let _ = self.no_arguments.set(owned);
        Ok(empty)
    }
}

/// The entry Realm, its global Environment and its defining program, as
/// rooted in one operation's scope.
type EntryRoots = (Rooted<EqRef>, Option<Rooted<EqRef>>, Rooted<EqRef>);

/// The entry Realm, its global Environment and the defining program that
/// every retained operation enters, resolved once after `main` has run.
///
/// None of them can change afterwards. R publishes its entry Realm root only
/// while that root is still null (`retain_snapshot_realm`); CreateRealm
/// installs a Realm's global Environment once; and a Store admits exactly one
/// P/R pair (`StorePhase`), whose P installs its Module graph in the entry
/// Realm once (a second install traps) and, without a source compiler in an
/// embedding Store, no other program can prepend a graph. Re-reading them
/// per call through the host GC API re-derived every field's type and was
/// the largest part of an exported call. `OwnedRooted` keeps them alive and
/// current across moving collections and needs no Store to drop.
pub(super) struct EntryContext {
    realm: OwnedRooted<EqRef>,
    environment: Option<OwnedRooted<EqRef>>,
    program: OwnedRooted<EqRef>,
}

impl EntryContext {
    pub(super) fn resolve(
        bridges: &Bridges,
        store: &mut impl AsContextMut<Data = WasmStoreState>,
    ) -> Result<Self, EngineError> {
        let (realm, environment, program) = resolve_entry(bridges, store)?;
        let mut owned = |reference: Rooted<EqRef>| {
            reference
                .to_owned_rooted(&mut *store)
                .map_err(embedded_error)
        };
        Ok(EntryContext {
            realm: owned(realm)?,
            environment: environment.map(&mut owned).transpose()?,
            program: owned(program)?,
        })
    }

    fn to_rooted(&self, store: &mut impl AsContextMut<Data = WasmStoreState>) -> EntryRoots {
        (
            self.realm.to_rooted(&mut *store),
            self.environment
                .as_ref()
                .map(|environment| environment.to_rooted(&mut *store)),
            self.program.to_rooted(&mut *store),
        )
    }

    /// Whether a fresh read of the Realm root and its fields still yields
    /// exactly the retained objects.
    fn is_current(
        &self,
        bridges: &Bridges,
        store: &mut impl AsContextMut<Data = WasmStoreState>,
    ) -> Result<bool, EngineError> {
        let (realm, environment, program) = resolve_entry(bridges, store)?;
        let store = store.as_context();
        let same = |retained: &OwnedRooted<EqRef>, live: &Rooted<EqRef>| {
            Rooted::<EqRef>::ref_eq(&store, retained, live).map_err(embedded_error)
        };
        let environment = match (&self.environment, &environment) {
            (None, None) => true,
            (Some(retained), Some(live)) => same(retained, live)?,
            _ => false,
        };
        Ok(environment && same(&self.realm, &realm)? && same(&self.program, &program)?)
    }
}

/// Read the entry Realm root, its Module graph's defining program and its
/// global Environment through the host GC API.
fn resolve_entry(
    bridges: &Bridges,
    store: &mut impl AsContextMut<Data = WasmStoreState>,
) -> Result<EntryRoots, EngineError> {
    let realm = bridges
        .realm
        .ok_or_else(|| EngineError::new("compiled program has no embedding Realm root"))?
        .get(&mut *store);
    let realm =
        eqref(store, realm)?.ok_or_else(|| EngineError::new("entry Realm was not initialized"))?;
    let graph_field = wasm_gc_completion::field(store, realm, GcHostField::RealmModules)?;
    let graph = eqref(store, graph_field)?
        .ok_or_else(|| EngineError::new("entry Realm has no Module graph"))?;
    let program_field = wasm_gc_completion::field(store, graph, GcHostField::ModuleGraphProgram)?;
    let program = eqref(store, program_field)?
        .ok_or_else(|| EngineError::new("Module graph has no defining program"))?;
    let environment_field =
        wasm_gc_completion::field(store, realm, GcHostField::RealmGlobalEnvironment)?;
    let environment = eqref(store, environment_field)?;
    Ok((realm, environment, program))
}

impl EmbeddedInstance {
    pub(super) fn check_store(
        &self,
        store: &WasmtimeStore<WasmStoreState>,
    ) -> Result<(), EngineError> {
        let state = store
            .data()
            .embedding
            .as_ref()
            .ok_or_else(|| EngineError::new("Store has no embedding authority"))?;
        if !Arc::ptr_eq(&state.store, &self.store) || state.phase != StorePhase::Evaluated {
            return Err(EngineError::new(
                "instance belongs to a different Store, has not initialized, or failed",
            ));
        }
        Ok(())
    }

    fn check_ready(&self, store: &mut WasmtimeStore<WasmStoreState>) -> Result<(), EngineError> {
        self.check_store(store)?;
        if matches!(
            module_entry_status(self.bridges.module_status.clone(), store)?,
            Some(WasmModuleEvaluationStatus::Pending | WasmModuleEvaluationStatus::NotStarted)
        ) {
            return Err(EngineError::new(
                "Module entry evaluation has not settled; pump host work before accessing its exports",
            ));
        }
        Ok(())
    }

    fn function(
        &self,
        store: &mut impl AsContextMut<Data = WasmStoreState>,
        name: &str,
        program: bool,
    ) -> Result<Func, EngineError> {
        let instance = if program { &self.program } else { &self.roots };
        instance
            .get_func(store, name)
            .ok_or_else(|| EngineError::new(format!("compiled embedding bridge is absent: {name}")))
    }

    /// The entry context retained at initialize, re-rooted in the caller's
    /// scope. Before initialize retains it (or when entry left it
    /// unresolvable), each operation reads it afresh and reports exactly what
    /// a fresh read finds.
    fn context(
        &self,
        store: &mut impl AsContextMut<Data = WasmStoreState>,
    ) -> Result<EntryRoots, EngineError> {
        let Some(entry) = &self.entry else {
            return resolve_entry(&self.bridges, store);
        };
        debug_assert!(
            matches!(entry.is_current(&self.bridges, store), Ok(true)),
            "retained embedding entry context went stale"
        );
        Ok(entry.to_rooted(store))
    }

    /// Whether the retained entry context still equals a fresh read; `None`
    /// when initialize retained none.
    #[cfg(test)]
    pub(super) fn entry_context_is_current(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
    ) -> Result<Option<bool>, EngineError> {
        let mut roots = wasmtime::RootScope::new(store);
        self.entry
            .as_ref()
            .map(|entry| entry.is_current(&self.bridges, &mut roots))
            .transpose()
    }

    fn execute(
        &self,
        store: &mut impl AsContextMut<Data = WasmStoreState>,
        function: Func,
        inputs: &[Val],
    ) -> Result<Vec<Val>, EngineError> {
        let results = function.ty(&*store).results().len();
        self.execute_with(store, function, results, inputs)
    }

    /// `execute` for a bridge whose result count is already known.
    fn execute_with(
        &self,
        store: &mut impl AsContextMut<Data = WasmStoreState>,
        function: Func,
        results: usize,
        inputs: &[Val],
    ) -> Result<Vec<Val>, EngineError> {
        let mut results = vec![Val::I32(0); results];
        if let Err(error) = function.call(&mut *store, inputs, &mut results) {
            if let Some(state) = store.as_context_mut().data_mut().embedding.as_mut() {
                state.phase = StorePhase::Failed;
            }
            return Err(embedded_error(error));
        }
        Ok(results)
    }

    /// Copy host data into an ordinary JavaScript value in this instance.
    pub fn value(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
        value: ForeignValue,
    ) -> Result<JsValue, EngineError> {
        self.check_store(store)?;
        run_on_execution_stack(|| {
            let mut roots = wasmtime::RootScope::new(store);
            self.value_inner(&mut roots, value)
        })
    }

    fn value_inner(
        &self,
        store: &mut impl AsContextMut<Data = WasmStoreState>,
        value: ForeignValue,
    ) -> Result<JsValue, EngineError> {
        self.value_in_realm(store, value, None)
    }

    fn value_in_realm(
        &self,
        store: &mut impl AsContextMut<Data = WasmStoreState>,
        value: ForeignValue,
        realm: Option<Rooted<EqRef>>,
    ) -> Result<JsValue, EngineError> {
        match value {
            ForeignValue::Number(number) => retained_value(
                store,
                JsValueKind::Number.tag(),
                number.to_bits() as i64,
                None,
            ),
            ForeignValue::Void => retained_value(store, JsValueKind::Undefined.tag(), 0, None),
            ForeignValue::StringUtf16(_) | ForeignValue::Bytes(_) => {
                let is_string = matches!(value, ForeignValue::StringUtf16(_));
                let name = if is_string {
                    "scriptw.embed.string"
                } else {
                    "scriptw.embed.bytes"
                };
                let factory = self.function(store, name, false)?;
                let ty = factory
                    .ty(&*store)
                    .params()
                    .next()
                    .and_then(|p| {
                        p.as_ref()
                            .and_then(|r| r.heap_type().as_concrete_array())
                            .cloned()
                    })
                    .ok_or_else(|| {
                        EngineError::new("embedding factory has no canonical array parameter")
                    })?;
                let raw = allocate_array(store, ty, &value).map_err(embedded_error)?;
                let mut inputs = vec![raw];
                if !is_string {
                    let realm = match realm {
                        Some(realm) => realm,
                        None => self.context(store)?.0,
                    };
                    inputs.push(Val::AnyRef(Some(realm.to_anyref())));
                }
                let result = self.execute(store, factory, &inputs)?;
                match dynamic_completion(store, &result)? {
                    JsCompletion::Normal(value) => Ok(value),
                    JsCompletion::Throw(_) => Err(EngineError::new(
                        "embedding value factory unexpectedly threw",
                    )),
                }
            }
        }
    }

    /// Read a live module export after entry evaluation settles.
    pub fn get_export(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
        name: &str,
    ) -> Result<JsCompletion, EngineError> {
        self.get_export_utf16(store, &name.encode_utf16().collect::<Vec<_>>())
    }

    /// Look up a live module export using a UTF-16 key.
    pub fn get_export_utf16(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
        name: &[u16],
    ) -> Result<JsCompletion, EngineError> {
        self.check_ready(store)?;
        run_on_execution_stack(|| {
            let mut roots = wasmtime::RootScope::new(store);
            let key = self.value_inner(&mut roots, ForeignValue::StringUtf16(name.to_vec()))?;
            let (realm, _, program) = self.context(&mut roots)?;
            let function = self.function(&mut roots, "scriptw.embed.export", true)?;
            let key = key
                .reference
                .as_ref()
                .ok_or_else(|| EngineError::new("export name has no String root"))?
                .to_rooted(&mut roots);
            let results = self.execute(
                &mut roots,
                function,
                &[
                    Val::AnyRef(Some(key.to_anyref())),
                    Val::AnyRef(Some(realm.to_anyref())),
                    Val::AnyRef(Some(program.to_anyref())),
                ],
            )?;
            dynamic_completion(&mut roots, &results)
        })
    }

    /// Execute ordinary JavaScript Get, including getters and Proxy traps.
    pub fn get_property(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
        target: &JsValue,
        key: &[u16],
    ) -> Result<JsCompletion, EngineError> {
        self.check_ready(store)?;
        target.check(store)?;
        run_on_execution_stack(|| {
            let mut roots = wasmtime::RootScope::new(store);
            let key = self.value_inner(&mut roots, ForeignValue::StringUtf16(key.to_vec()))?;
            let mut inputs = target.arguments(&mut roots)?.to_vec();
            inputs.push(Val::AnyRef(Some(
                key.reference
                    .as_ref()
                    .unwrap()
                    .to_rooted(&mut roots)
                    .to_anyref(),
            )));
            let (realm, environment, program) = self.context(&mut roots)?;
            inputs.extend(context_arguments(realm, environment, program));
            let function = self.function(&mut roots, "scriptw.embed.read", false)?;
            let result = self.execute(&mut roots, function, &inputs)?;
            dynamic_completion(&mut roots, &result)
        })
    }

    /// Invoke a retained callable with an explicit receiver and exact JS args.
    /// Bound functions, proxies, strict `this`, throws and Realm entry use the
    /// same runtime dispatch as calls emitted from JavaScript source.
    /// Top-level execution and synchronous host callbacks use the caller's
    /// thread on a guarded stack. Reentry receives an independent stack.
    pub fn call(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
        callable: &JsValue,
        receiver: Option<&JsValue>,
        arguments: &[JsValue],
    ) -> Result<JsCompletion, EngineError> {
        self.check_ready(store)?;
        callable.check(store)?;
        if let Some(receiver) = receiver {
            receiver.check(store)?;
        }
        for argument in arguments {
            argument.check(store)?;
        }
        run_on_execution_stack(|| {
            let mut roots = wasmtime::RootScope::new(store);
            self.call_inner(&mut roots, callable, receiver, arguments)
        })
    }

    fn call_inner(
        &self,
        store: &mut impl AsContextMut<Data = WasmStoreState>,
        callable: &JsValue,
        receiver: Option<&JsValue>,
        arguments: &[JsValue],
    ) -> Result<JsCompletion, EngineError> {
        let bridge = match self.bridges.call.get() {
            Some(bridge) => bridge,
            None => {
                let function = self.function(store, "scriptw.embed.call", false)?;
                let bridge = CallBridge::new(store, function)?;
                self.bridges.call.get_or_init(|| bridge)
            }
        };
        let args = if arguments.is_empty() {
            bridge.empty_arguments(store)?
        } else {
            u32::try_from(arguments.len())
                .map_err(|_| EngineError::new("argument count exceeds Wasm32"))?;
            let mut elements = Vec::new();
            elements
                .try_reserve_exact(arguments.len())
                .map_err(|e| EngineError::new(e.to_string()))?;
            for argument in arguments {
                let [tag, scalar, reference] = argument.arguments(store)?;
                let mut fields = [Val::I32(0); 3];
                fields[GcHostField::StoredTag.definition().index() as usize] = tag;
                fields[GcHostField::StoredScalar.definition().index() as usize] = scalar;
                fields[GcHostField::StoredReference.definition().index() as usize] = reference;
                let value =
                    StructRef::new(&mut *store, &bridge.stored, &fields).map_err(embedded_error)?;
                elements.push(Val::AnyRef(Some(value.to_anyref())));
            }
            ArrayRef::new_fixed(&mut *store, &bridge.arguments, &elements)
                .map_err(embedded_error)?
        };
        let (callee_tag, callee_scalar, callee) = callable.parts(store)?;
        // An absent receiver is exactly the undefined value: tag, zero, no root.
        let (this_tag, this_scalar, this) = match receiver {
            Some(receiver) => receiver.parts(store)?,
            None => (JsValueKind::Undefined.tag(), 0, None),
        };
        let (realm, environment, program) = self.context(store)?;
        let completion = match bridge.function.call(
            &mut *store,
            (
                callee_tag,
                callee_scalar,
                callee,
                this_tag,
                this_scalar,
                this,
                args,
                realm,
                environment,
                program,
            ),
        ) {
            Ok(completion) => completion,
            Err(error) => {
                if let Some(state) = store.as_context_mut().data_mut().embedding.as_mut() {
                    state.phase = StorePhase::Failed;
                }
                return Err(embedded_error(error));
            }
        };
        retained_completion(store, completion)
    }

    /// Copy an attached, in-bounds Uint8Array view into host-owned bytes.
    pub fn to_bytes(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
        value: &JsValue,
    ) -> Result<Vec<u8>, EngineError> {
        self.check_store(store)?;
        value.check(store)?;
        run_on_execution_stack(|| {
            let mut roots = wasmtime::RootScope::new(store);
            let inputs = value.arguments(&mut roots)?;
            let function = self.function(&mut roots, "scriptw.embed.bytes.read", false)?;
            let results = self.execute(&mut roots, function, &inputs)?;
            let [value @ Val::AnyRef(Some(_))] = results.as_slice() else {
                return Err(EngineError::new(
                    "value is not an attached, in-bounds Uint8Array",
                ));
            };
            let ForeignValue::Bytes(bytes) =
                copy_array(&mut roots, value, false).map_err(embedded_error)?
            else {
                unreachable!()
            };
            Ok(bytes)
        })
    }

    /// Adopt a Promise for the host and inspect its current settled value.
    pub fn promise_state(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
        value: &JsValue,
    ) -> Result<JsPromiseState, EngineError> {
        self.check_store(store)?;
        value.check(store)?;
        run_on_execution_stack(|| {
            let mut roots = wasmtime::RootScope::new(store);
            let inputs = value.arguments(&mut roots)?;
            let results = match self.bridges.promise {
                Some((function, results)) => {
                    self.execute_with(&mut roots, function, results, &inputs)?
                }
                None => {
                    let function = self.function(&mut roots, "scriptw.embed.promise", false)?;
                    self.execute(&mut roots, function, &inputs)?
                }
            };
            let Some(Val::I32(state)) = results.get(5) else {
                return Err(EngineError::new("Promise bridge has wrong status storage"));
            };
            match *state {
                0 => Ok(JsPromiseState::Pending),
                1 => Ok(JsPromiseState::Fulfilled(
                    dynamic_completion(&mut roots, &results[..5])?.into_value(),
                )),
                2 => Ok(JsPromiseState::Rejected(
                    dynamic_completion(&mut roots, &results[..5])?.into_value(),
                )),
                -1 => Err(EngineError::new("JavaScript value is not a Promise")),
                _ => Err(EngineError::new("Promise bridge returned an invalid state")),
            }
        })
    }

    pub fn pending_host_operations(
        &self,
        store: &WasmtimeStore<WasmStoreState>,
    ) -> Result<usize, EngineError> {
        self.check_store(store)?;
        Ok(store
            .data()
            .embedding
            .as_ref()
            .unwrap()
            .async_host
            .pending_count())
    }

    /// Deliver a finite batch of completed/cancelled host work, then drain JS
    /// jobs and checkpoint top-level await. This never waits for host I/O.
    pub fn pump(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
    ) -> Result<EmbeddedEvaluation, EngineError> {
        self.check_store(store)?;
        let result = run_on_execution_stack(|| {
            let mut roots = wasmtime::RootScope::new(&mut *store);
            let ready = roots
                .as_context_mut()
                .data_mut()
                .embedding
                .as_mut()
                .unwrap()
                .async_host
                .take_ready();
            for settlement in ready {
                // Release transfer arrays and argument temporaries per request;
                // only real Promise/JS roots survive across the batch.
                let mut roots = wasmtime::RootScope::new(&mut roots);
                let (resolver, data) = match settlement.settlement {
                    ForeignAsyncSettlement::Fulfill(value) => (settlement.resolve, Some(value)),
                    ForeignAsyncSettlement::Reject(value) => (settlement.reject, Some(value)),
                    ForeignAsyncSettlement::Cancelled => (settlement.reject, None),
                };
                let reference = resolver.to_rooted(&mut roots);
                let context_field = wasm_gc_completion::field(
                    &mut roots,
                    reference,
                    GcHostField::FunctionObjectContext,
                )?;
                let context = eqref(&mut roots, context_field)?
                    .ok_or_else(|| EngineError::new("foreign resolver has no FunctionContext"))?;
                let realm_field =
                    wasm_gc_completion::field(&mut roots, context, GcHostField::ContextRealm)?;
                let operation_realm = eqref(&mut roots, realm_field)?
                    .ok_or_else(|| EngineError::new("foreign resolver has no defining Realm"))?;
                let argument = match data {
                    Some(value) => self.value_in_realm(&mut roots, value, Some(operation_realm))?,
                    None => {
                        let (_, _, program) = self.context(&mut roots)?;
                        let factory = self.function(&mut roots, "scriptw.embed.cancel", false)?;
                        let results = self.execute(
                            &mut roots,
                            factory,
                            &[
                                Val::AnyRef(Some(operation_realm.to_anyref())),
                                Val::AnyRef(Some(program.to_anyref())),
                            ],
                        )?;
                        dynamic_completion(&mut roots, &results)?.into_value()
                    }
                };
                let callable =
                    retained_value(&mut roots, JsValueKind::Function.tag(), 0, Some(reference))?;
                match self.call_inner(&mut roots, &callable, None, &[argument])? {
                    JsCompletion::Normal(_) => {}
                    JsCompletion::Throw(value) => {
                        roots
                            .as_context_mut()
                            .data_mut()
                            .embedding
                            .as_mut()
                            .unwrap()
                            .phase = StorePhase::Failed;
                        return Ok(EmbeddedEvaluation::Settled(JsCompletion::Throw(value)));
                    }
                }
            }
            let (realm, _, program) = self.context(&mut roots)?;
            let function = self.function(&mut roots, "scriptw.embed.checkpoint", true)?;
            let results = self.execute(
                &mut roots,
                function,
                &[
                    Val::AnyRef(Some(realm.to_anyref())),
                    Val::AnyRef(Some(program.to_anyref())),
                ],
            )?;
            if matches!(
                module_entry_status(self.bridges.module_status.clone(), &mut roots)?,
                Some(WasmModuleEvaluationStatus::Pending)
            ) {
                return Ok(EmbeddedEvaluation::Pending);
            }
            Ok(EmbeddedEvaluation::Settled(dynamic_completion(
                &mut roots, &results,
            )?))
        });
        // A failed conversion may have detached an entire ready batch. Do not
        // leave a reusable Store whose unresolved Promises lost their roots.
        if result.is_err() {
            store.data_mut().embedding.as_mut().unwrap().phase = StorePhase::Failed;
        }
        result
    }

    pub fn take_output(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
    ) -> Result<Vec<HostOutputEvent>, EngineError> {
        self.check_store(store)?;
        Ok(store.data().output_events.take())
    }
}

fn context_arguments(
    realm: Rooted<EqRef>,
    environment: Option<Rooted<EqRef>>,
    program: Rooted<EqRef>,
) -> [Val; 3] {
    [
        Val::AnyRef(Some(realm.to_anyref())),
        Val::AnyRef(environment.map(|r| r.to_anyref())),
        Val::AnyRef(Some(program.to_anyref())),
    ]
}

fn eqref(
    store: &mut impl AsContextMut<Data = WasmStoreState>,
    value: Val,
) -> Result<Option<Rooted<EqRef>>, EngineError> {
    let Val::AnyRef(reference) = value else {
        return Err(EngineError::new(
            "embedding context field is not a GC reference",
        ));
    };
    reference
        .map(|r| {
            r.as_eqref(&mut *store)
                .map_err(embedded_error)?
                .ok_or_else(|| EngineError::new("embedding context reference is not eqref"))
        })
        .transpose()
}
