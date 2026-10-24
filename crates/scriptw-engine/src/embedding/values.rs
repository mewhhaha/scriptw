//! Store-bound roots for exact JavaScript values.
use super::*;
use scriptw_aot_wasm::{check_gc_main_completion, GcHostValue};
use wasmtime::{EqRef, OwnedRooted, Rooted, Val};

pub use scriptw_aot_wasm::WasmRuntimeValueTag as JsValueKind;

/// An exact JavaScript value. Clones retain the same GC object, not a snapshot.
/// Reference values are rooted until the last handle is dropped.
#[derive(Clone, Debug)]
pub struct JsValue {
    pub(super) tag: JsValueKind,
    pub(super) scalar: i64,
    pub(super) reference: Option<OwnedRooted<EqRef>>,
    pub(super) store: Arc<()>,
}

/// JavaScript exceptions remain values; Wasmtime traps are `EngineError`s.
#[derive(Debug)]
pub enum JsCompletion {
    Normal(JsValue),
    Throw(JsValue),
}

impl JsCompletion {
    pub fn value(&self) -> &JsValue {
        match self {
            Self::Normal(value) | Self::Throw(value) => value,
        }
    }
    pub fn into_value(self) -> JsValue {
        match self {
            Self::Normal(value) | Self::Throw(value) => value,
        }
    }
}

/// Entry evaluation can suspend on host work without consuming the instance.
#[derive(Debug)]
pub enum EmbeddedEvaluation {
    Pending,
    Settled(JsCompletion),
}

#[derive(Debug)]
pub enum JsPromiseState {
    Pending,
    Fulfilled(JsValue),
    Rejected(JsValue),
}

impl JsValue {
    pub fn kind(&self) -> JsValueKind {
        self.tag
    }
    pub fn as_number(&self) -> Option<f64> {
        (self.tag == JsValueKind::Number).then(|| f64::from_bits(self.scalar as u64))
    }
    pub fn as_boolean(&self) -> Option<bool> {
        (self.tag == JsValueKind::Boolean).then_some(self.scalar == 1)
    }
    pub fn is_undefined(&self) -> bool {
        self.tag == JsValueKind::Undefined
    }

    pub(super) fn check(
        &self,
        store: &impl AsContext<Data = WasmStoreState>,
    ) -> Result<(), EngineError> {
        let context = store.as_context();
        let state = context
            .data()
            .embedding
            .as_ref()
            .ok_or_else(|| EngineError::new("value used outside its embedding Store"))?;
        if !Arc::ptr_eq(&self.store, &state.store) {
            return Err(EngineError::new(
                "JavaScript value belongs to a different Store",
            ));
        }
        Ok(())
    }

    pub fn to_utf16(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
    ) -> Result<Vec<u16>, EngineError> {
        self.check(store)?;
        if self.tag != JsValueKind::String {
            return Err(EngineError::new(
                "JavaScript value is not a primitive string",
            ));
        }
        let mut roots = wasmtime::RootScope::new(store);
        let reference = self
            .reference
            .as_ref()
            .ok_or_else(|| EngineError::new("String has no root"))?
            .to_rooted(&mut roots);
        Ok(wasm_gc_completion::string_units(&mut roots, reference)?.into_vec())
    }

    /// A diagnostic snapshot; objects remain exact retained values in `self`.
    pub fn observe(
        &self,
        store: &mut WasmtimeStore<WasmStoreState>,
    ) -> Result<ObservedJsValue, EngineError> {
        self.check(store)?;
        let mut roots = wasmtime::RootScope::new(store);
        let reference = self.reference.as_ref().map(|r| r.to_rooted(&mut roots));
        Ok(wasm_gc_completion::observe(
            &mut roots,
            (
                self.tag.tag(),
                self.scalar,
                reference,
                CompletionKindIr::Normal.abi_code() as i32,
                0,
            ),
        )?
        .into_parts()
        .2)
    }

    pub(super) fn arguments(
        &self,
        store: &mut impl AsContextMut<Data = WasmStoreState>,
    ) -> Result<[Val; 3], EngineError> {
        self.check(store)?;
        let reference = self
            .reference
            .as_ref()
            .map(|r| r.to_rooted(&mut *store).to_anyref());
        Ok([
            Val::I32(self.tag.tag()),
            Val::I64(self.scalar),
            Val::AnyRef(reference),
        ])
    }
}

pub(super) fn retained_value(
    store: &mut impl AsContextMut<Data = WasmStoreState>,
    tag: i32,
    scalar: i64,
    reference: Option<Rooted<EqRef>>,
) -> Result<JsValue, EngineError> {
    let checked = GcHostValue::check(tag, scalar, reference.is_some()).map_err(EngineError::new)?;
    let authority = Arc::clone(
        &store
            .as_context()
            .data()
            .embedding
            .as_ref()
            .ok_or_else(|| EngineError::new("value has no Store authority"))?
            .store,
    );
    let reference = reference
        .map(|r| r.to_owned_rooted(&mut *store))
        .transpose()
        .map_err(embedded_error)?;
    Ok(JsValue {
        tag: checked.tag(),
        scalar: checked.scalar(),
        reference,
        store: authority,
    })
}

pub(super) fn retained_completion(
    store: &mut impl AsContextMut<Data = WasmStoreState>,
    completion: GcMainCompletion,
) -> Result<JsCompletion, EngineError> {
    let (tag, scalar, reference, kind, target) = completion;
    let kind = check_gc_main_completion(kind, target).map_err(EngineError::new)?;
    let value = retained_value(store, tag, scalar, reference)?;
    match kind {
        CompletionKindIr::Normal => Ok(JsCompletion::Normal(value)),
        CompletionKindIr::Throw => Ok(JsCompletion::Throw(value)),
        _ => Err(EngineError::new(
            "embedding completion has unresolved local control",
        )),
    }
}

pub(super) fn dynamic_completion(
    store: &mut impl AsContextMut<Data = WasmStoreState>,
    results: &[Val],
) -> Result<JsCompletion, EngineError> {
    let [Val::I32(tag), Val::I64(scalar), Val::AnyRef(reference), Val::I32(kind), Val::I32(target)] =
        results
    else {
        return Err(EngineError::new(
            "embedding bridge completion has invalid Wasm storage",
        ));
    };
    let reference = reference
        .map(|r| {
            r.as_eqref(&mut *store)
                .map_err(embedded_error)?
                .ok_or_else(|| EngineError::new("embedding reference is not an eqref"))
        })
        .transpose()?;
    retained_completion(store, (*tag, *scalar, reference, *kind, *target))
}

pub(super) fn embedded_error(error: wasmtime::Error) -> EngineError {
    if let Some(rejection) = error.downcast_ref::<RuntimeSemanticRejection>() {
        return EngineError::from_runtime_semantic_rejection(*rejection);
    }
    if let Some(cause) = error.downcast_ref::<EngineError>() {
        let mut retained = cause.clone();
        retained.message = format!("embedded Wasm/host execution failed: {error:#}");
        return retained;
    }
    let kind = if is_wasm_epoch_interrupt(&error) {
        EngineExecutionFailure::Timeout
    } else {
        EngineExecutionFailure::Trap
    };
    EngineError::from_execution_failure(
        kind,
        format!("embedded Wasm/host execution failed: {error:#}"),
    )
}
