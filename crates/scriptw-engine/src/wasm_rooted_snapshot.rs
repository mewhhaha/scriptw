//! The native graph adapter reads canonical GC records while the execution's
//! RootScope remains alive. Deferred intrinsic values resolve through an internal
//! Wasm helper under the existing Store limits; no JavaScript hook is called.
use super::*;
use scriptw_aot_wasm::{GcSnapshotField as F, GcSnapshotLayout as L};
use scriptw_runtime::rooted_snapshot::*;
use wasmtime::{AsContextMut, EqRef, HeapType, Instance, Rooted};
mod bigint;

#[derive(Clone)]
struct NativeValue {
    tag: i32,
    scalar: i64,
    reference: Option<Rooted<EqRef>>,
}

enum NativeCompactPayloads {
    Arrays {
        tags: Rooted<wasmtime::ArrayRef>,
        scalars: Option<Rooted<wasmtime::ArrayRef>>,
        references: Option<Rooted<wasmtime::ArrayRef>>,
    },
    Inline {
        packed_tags: u32,
        scalars: [i64; 2],
        references: [Option<Rooted<EqRef>>; 2],
    },
}

#[derive(Clone, Copy)]
enum NativeCompactStorageKind {
    Arrays,
    Inline,
}

fn invariant(detail: impl std::fmt::Display) -> SnapshotRejection {
    SnapshotRejection::BackendInvariant {
        detail: detail.to_string(),
    }
}
fn cap(dimension: SnapshotBudgetDimension) -> SnapshotRejection {
    SnapshotRejection::BudgetExceeded { dimension }
}

pub(super) fn observe(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    instance: &Instance,
    (tag, scalar, reference, kind, target): wasm_gc_completion::GcMainCompletion,
    limits: SnapshotLimits,
) -> Result<SnapshotCompletion, EngineError> {
    let kind = match scriptw_aot_wasm::check_gc_main_completion(kind, target)
        .map_err(|detail| EngineError::new(format!("invalid graph completion ABI: {detail}")))?
    {
        CompletionKindIr::Normal => SnapshotCompletionKind::Normal,
        CompletionKindIr::Throw => SnapshotCompletionKind::Throw,
        CompletionKindIr::Return
        | CompletionKindIr::Break
        | CompletionKindIr::Continue
        | CompletionKindIr::Empty => return Err(EngineError::new("unresolved graph completion")),
    };
    let value = NativeValue {
        tag,
        scalar,
        reference,
    };
    let outcome = match NativeSnapshot::new(store, instance, limits) {
        Ok(mut backend) => capture_snapshot(&mut backend, &value, limits),
        Err(reason) => SnapshotOutcome::Rejected { reason },
    };
    Ok(SnapshotCompletion { kind, outcome })
}

struct NativeSnapshot<'a, S: AsContextMut<Data = WasmHostState>> {
    store: &'a mut S,
    witnesses: Vec<(L, HeapType)>,
    resolve_property_value: wasmtime::Func,
    entry: Rooted<EqRef>,
    realms: Vec<Rooted<EqRef>>,
    symbols: Rooted<EqRef>,
    inventory_work: usize,
}
impl<'a, S: AsContextMut<Data = WasmHostState>> NativeSnapshot<'a, S> {
    fn new(
        store: &'a mut S,
        instance: &Instance,
        limits: SnapshotLimits,
    ) -> Result<Self, SnapshotRejection> {
        let mut witnesses = Vec::with_capacity(L::ALL.len());
        for layout in L::ALL {
            let global = instance
                .get_global(&mut *store, layout.witness_export())
                .ok_or_else(|| invariant("snapshot type witness absent"))?;
            let ty = global.ty(&*store);
            if ty.mutability() != wasmtime::Mutability::Const {
                return Err(invariant("snapshot type witness is mutable"));
            }
            let reference = ty
                .content()
                .as_ref()
                .ok_or_else(|| invariant("snapshot witness is not a reference"))?;
            if !reference.is_nullable() {
                return Err(invariant("snapshot witness must be nullable"));
            }
            witnesses.push((*layout, reference.heap_type().clone()));
        }
        let entry = root_global(
            store,
            instance,
            scriptw_aot_wasm::SNAPSHOT_ENTRY_REALM_EXPORT,
        )?
        .ok_or_else(|| invariant("entry Realm absent"))?;
        let symbols = root_global(store, instance, scriptw_aot_wasm::SNAPSHOT_SYMBOLS_EXPORT)?
            .ok_or_else(|| invariant("well-known Symbol table absent"))?;
        let mut current = root_global(store, instance, scriptw_aot_wasm::SNAPSHOT_REALMS_EXPORT)?;
        let resolve_property_value = instance
            .get_func(
                &mut *store,
                scriptw_aot_wasm::SNAPSHOT_RESOLVE_PROPERTY_VALUE_EXPORT,
            )
            .ok_or_else(|| invariant("snapshot property resolver absent"))?;
        let mut backend = Self {
            store,
            resolve_property_value,
            witnesses,
            entry,
            realms: Vec::new(),
            symbols,
            inventory_work: 0,
        };
        let mut inventory_budget = SnapshotBudget::new(limits);
        backend.require(entry, L::Realm)?;
        backend.require(symbols, L::WellKnownSymbols)?;
        while let Some(node) = current {
            inventory_budget.work(2 + backend.realms.len())?;
            if backend.realms.len() >= limits.get(SnapshotBudgetDimension::Realms) as usize {
                return Err(cap(SnapshotBudgetDimension::Realms));
            }
            // Inventory extent is capped before retaining/copying its roots.
            let realm = backend.required_reference(node, F::InventoryRealm)?;
            backend.require(realm, L::Realm)?;
            for known in &backend.realms {
                if Rooted::ref_eq(&*backend.store, known, &realm).map_err(invariant)? {
                    return Err(invariant("duplicate/cyclic Realm inventory"));
                }
            }
            backend.realms.push(realm);
            current = backend.reference(node, F::InventoryPrevious)?;
        }
        let mut has_entry = false;
        for realm in &backend.realms {
            inventory_budget.work(1)?;
            has_entry |= Rooted::ref_eq(&*backend.store, realm, &entry).map_err(invariant)?;
        }
        if !has_entry {
            return Err(invariant("entry Realm missing from inventory"));
        }
        backend.inventory_work = (limits.get(SnapshotBudgetDimension::Work)
            - inventory_budget.remaining(SnapshotBudgetDimension::Work))
            as usize;
        Ok(backend)
    }
    fn require(&self, reference: Rooted<EqRef>, layout: L) -> Result<(), SnapshotRejection> {
        let actual = reference.ty(&*self.store).map_err(invariant)?;
        let expected = &self
            .witnesses
            .iter()
            .find(|(candidate, _)| *candidate == layout)
            .ok_or_else(|| invariant("unregistered snapshot layout"))?
            .1;
        if !HeapType::eq(&actual, expected) {
            return Err(invariant(format!("canonical {layout:?} type mismatch")));
        }
        Ok(())
    }
    fn has_layout(&self, reference: Rooted<EqRef>, layout: L) -> Result<bool, SnapshotRejection> {
        let actual = reference.ty(&*self.store).map_err(invariant)?;
        Ok(self
            .witnesses
            .iter()
            .any(|(candidate, expected)| *candidate == layout && HeapType::eq(&actual, expected)))
    }
    fn field(
        &mut self,
        reference: Rooted<EqRef>,
        field: F,
    ) -> Result<WasmtimeVal, SnapshotRejection> {
        self.require(reference, field.layout())?;
        reference
            .as_struct(&*self.store)
            .map_err(invariant)?
            .ok_or_else(|| invariant("expected struct"))?
            .field(&mut *self.store, field.index() as usize)
            .map_err(invariant)
    }
    fn integer(&mut self, reference: Rooted<EqRef>, field: F) -> Result<i64, SnapshotRejection> {
        match self.field(reference, field)? {
            WasmtimeVal::I32(value) => Ok(i64::from(value)),
            WasmtimeVal::I64(value) => Ok(value),
            _ => Err(invariant("expected integral field")),
        }
    }
    fn boolean(&mut self, reference: Rooted<EqRef>, field: F) -> Result<bool, SnapshotRejection> {
        match self.integer(reference, field)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invariant("noncanonical Boolean field")),
        }
    }
    fn reference(
        &mut self,
        reference: Rooted<EqRef>,
        field: F,
    ) -> Result<Option<Rooted<EqRef>>, SnapshotRejection> {
        let value = self.field(reference, field)?;
        eq_reference(self.store, value)
    }
    fn required_reference(
        &mut self,
        reference: Rooted<EqRef>,
        field: F,
    ) -> Result<Rooted<EqRef>, SnapshotRejection> {
        self.reference(reference, field)?
            .ok_or_else(|| invariant("required reference is null"))
    }
    fn array(
        &self,
        reference: Rooted<EqRef>,
        layout: L,
    ) -> Result<Rooted<wasmtime::ArrayRef>, SnapshotRejection> {
        self.require(reference, layout)?;
        reference
            .as_array(&*self.store)
            .map_err(invariant)?
            .ok_or_else(|| invariant("expected array"))
    }
    fn stored(&mut self, reference: Rooted<EqRef>) -> Result<NativeValue, SnapshotRejection> {
        let tag = self.integer(reference, F::StoredTag)? as i32;
        let scalar = self.integer(reference, F::StoredScalar)?;
        let reference = self.reference(reference, F::StoredRef)?;
        scriptw_aot_wasm::GcHostValue::check(tag, scalar, reference.is_some())
            .map_err(invariant)?;
        Ok(NativeValue {
            tag,
            scalar,
            reference,
        })
    }
    fn stored_field(
        &mut self,
        reference: Rooted<EqRef>,
        field: F,
    ) -> Result<NativeValue, SnapshotRejection> {
        let stored = self.required_reference(reference, field)?;
        self.stored(stored)
    }
    fn property_value(
        &mut self,
        reference: Rooted<EqRef>,
        budget: &mut SnapshotBudget,
    ) -> Result<NativeValue, SnapshotRejection> {
        if self.has_layout(reference, L::Stored)? {
            return self.stored(reference);
        }
        let expected = &self
            .witnesses
            .iter()
            .find(|(layout, _)| *layout == L::PropertyValue)
            .ok_or_else(|| invariant("property value witness absent"))?
            .1;
        if !reference
            .matches_ty(&*self.store, expected)
            .map_err(invariant)?
        {
            return Err(invariant("descriptor value is not a PropertyValue"));
        }
        // Charge before materialization, keeping the existing fuel/deadline and
        // graph limits. The helper allocates intrinsic records without JS calls.
        budget.work(1)?;
        let mut results = [WasmtimeVal::AnyRef(None)];
        self.resolve_property_value
            .call(
                &mut *self.store,
                &[WasmtimeVal::AnyRef(Some(reference.to_anyref()))],
                &mut results,
            )
            .map_err(invariant)?;
        let resolved = eq_reference(self.store, results[0].clone())?
            .ok_or_else(|| invariant("property value resolver returned null"))?;
        self.stored(resolved)
    }
    fn string(
        &mut self,
        reference: Rooted<EqRef>,
        budget: &mut SnapshotBudget,
    ) -> Result<SnapshotString, SnapshotRejection> {
        let units = self.required_reference(reference, F::StringUnits)?;
        let array = self.array(units, L::CodeUnits)?;
        let length = array.len(&*self.store).map_err(invariant)? as usize;
        budget.read_utf16(length, |index| {
            let WasmtimeVal::I32(unit) = array
                .get(&mut *self.store, index as u32)
                .map_err(invariant)?
            else {
                return Err(invariant("code unit is not I32"));
            };
            u16::try_from(unit).map_err(invariant)
        })
    }
    fn bigint(
        &mut self,
        reference: Rooted<EqRef>,
        budget: &mut SnapshotBudget,
    ) -> Result<SnapshotBigInt, SnapshotRejection> {
        let negative = self.boolean(reference, F::BigIntSign)?;
        let limbs = self.required_reference(reference, F::BigIntLimbs)?;
        let array = self.array(limbs, L::Limbs)?;
        let length = array.len(&*self.store).map_err(invariant)? as usize;
        budget.work(length)?;
        let max_digits = budget.remaining(SnapshotBudgetDimension::BigIntDigits) as usize;
        // Every nonzero high limb contributes at least one bit; avoid a
        // source-sized limb copy when even the minimum decimal size cannot fit.
        if length
            .saturating_sub(1)
            .saturating_mul(64)
            .saturating_mul(301)
            / 1000
            + 1
            > max_digits
        {
            return Err(cap(SnapshotBudgetDimension::BigIntDigits));
        }
        let mut limbs = Vec::with_capacity(length);
        for index in 0..length {
            let WasmtimeVal::I64(limb) = array
                .get(&mut *self.store, index as u32)
                .map_err(invariant)?
            else {
                return Err(invariant("BigInt limb is not I64"));
            };
            limbs.push(limb as u64);
        }
        bigint::decimal(negative, limbs, budget)
    }
    fn descriptor(
        &mut self,
        descriptor: Rooted<EqRef>,
        budget: &mut SnapshotBudget,
    ) -> Result<SnapshotRawDescriptor<NativeValue>, SnapshotRejection> {
        let word = u32::try_from(self.integer(descriptor, F::DescriptorFlags)?)
            .map_err(|_| invariant("descriptor flags exceed their unsigned word"))?;
        let (accessor, writable, enumerable, configurable) =
            scriptw_aot_wasm::snapshot_descriptor_flags(word)
                .ok_or_else(|| invariant("descriptor has unknown bits"))?;
        if accessor {
            if writable {
                return Err(invariant("accessor descriptor has writable bit"));
            }
            Ok(SnapshotRawDescriptor::Accessor {
                get: self.stored_field(descriptor, F::DescriptorGet)?,
                set: self.stored_field(descriptor, F::DescriptorSet)?,
                enumerable,
                configurable,
            })
        } else {
            let value = self.required_reference(descriptor, F::DescriptorValue)?;
            Ok(SnapshotRawDescriptor::Data {
                value: self.property_value(value, budget)?,
                writable,
                enumerable,
                configurable,
            })
        }
    }
    fn anchors(
        &mut self,
        object: Rooted<EqRef>,
        budget: &mut SnapshotBudget,
    ) -> Result<Vec<SnapshotRawAnchor<Rooted<EqRef>>>, SnapshotRejection> {
        budget.work(self.realms.len() * SnapshotIntrinsic::ALL.len())?;
        let mut anchors = Vec::new();
        for index in 0..self.realms.len() {
            let realm = self.realms[index];
            let table = self.required_reference(realm, F::RealmIntrinsics)?;
            let array = self.array(table, L::Intrinsics)?;
            for &intrinsic in SnapshotIntrinsic::ALL {
                let raw = array
                    .get(
                        &mut *self.store,
                        scriptw_aot_wasm::snapshot_intrinsic_index(intrinsic) as u32,
                    )
                    .map_err(invariant)?;
                let stored =
                    eq_reference(self.store, raw)?.ok_or_else(|| invariant("null intrinsic"))?;
                let value = self.stored(stored)?;
                if let Some(reference) = value.reference {
                    if Rooted::ref_eq(&*self.store, &object, &reference).map_err(invariant)? {
                        anchors.push(SnapshotRawAnchor { realm, intrinsic });
                    }
                }
            }
        }
        Ok(anchors)
    }
}

impl<S: AsContextMut<Data = WasmHostState>> SnapshotBackend for NativeSnapshot<'_, S> {
    type Value = NativeValue;
    type Object = Rooted<EqRef>;
    type Symbol = Rooted<EqRef>;
    type Realm = Rooted<EqRef>;
    fn entry_realm(&self) -> Self::Realm {
        self.entry
    }
    fn same_object(
        &mut self,
        left: &Self::Object,
        right: &Self::Object,
    ) -> Result<bool, SnapshotRejection> {
        Rooted::ref_eq(&*self.store, left, right).map_err(invariant)
    }
    fn same_symbol(
        &mut self,
        left: &Self::Symbol,
        right: &Self::Symbol,
    ) -> Result<bool, SnapshotRejection> {
        Rooted::ref_eq(&*self.store, left, right).map_err(invariant)
    }
    fn same_realm(
        &mut self,
        left: &Self::Realm,
        right: &Self::Realm,
    ) -> Result<bool, SnapshotRejection> {
        Rooted::ref_eq(&*self.store, left, right).map_err(invariant)
    }
    fn value(
        &mut self,
        value: &NativeValue,
        budget: &mut SnapshotBudget,
    ) -> Result<SnapshotRawValue<Self::Object, Self::Symbol>, SnapshotRejection> {
        budget.work(std::mem::take(&mut self.inventory_work))?;
        let checked = scriptw_aot_wasm::GcHostValue::check(
            value.tag,
            value.scalar,
            value.reference.is_some(),
        )
        .map_err(invariant)?;
        Ok(match checked.tag() {
            WasmRuntimeValueTag::Undefined => SnapshotRawValue::Undefined,
            WasmRuntimeValueTag::Null => SnapshotRawValue::Null,
            WasmRuntimeValueTag::Boolean => SnapshotRawValue::Boolean(value.scalar == 1),
            WasmRuntimeValueTag::Number => SnapshotRawValue::Number(value.scalar as u64),
            WasmRuntimeValueTag::String => {
                SnapshotRawValue::String(self.string(value.reference.unwrap(), budget)?)
            }
            WasmRuntimeValueTag::BigInt => {
                SnapshotRawValue::BigInt(self.bigint(value.reference.unwrap(), budget)?)
            }
            WasmRuntimeValueTag::Symbol => {
                let reference = value.reference.unwrap();
                self.require(reference, L::Symbol)?;
                SnapshotRawValue::Symbol(reference)
            }
            WasmRuntimeValueTag::Arguments => {
                return Err(SnapshotRejection::UnsupportedExotic {
                    exotic: SnapshotExotic::Arguments,
                });
            }
            WasmRuntimeValueTag::Array => {
                let reference = value.reference.unwrap();
                self.require(reference, L::Array)?;
                SnapshotRawValue::Object(reference)
            }
            WasmRuntimeValueTag::Function | WasmRuntimeValueTag::Object => {
                SnapshotRawValue::Object(value.reference.unwrap())
            }
        })
    }
    fn symbol(
        &mut self,
        symbol: &Self::Symbol,
        budget: &mut SnapshotBudget,
    ) -> Result<SnapshotSymbolData, SnapshotRejection> {
        let description = self
            .reference(*symbol, F::SymbolDescription)?
            .map(|reference| self.string(reference, budget))
            .transpose()?;
        let registry = self.reference(*symbol, F::SymbolRegistry)?;
        let table = self.array(self.symbols, L::WellKnownSymbols)?;
        budget.work(SnapshotWellKnownSymbol::ALL.len())?;
        for &name in SnapshotWellKnownSymbol::ALL {
            let raw = table
                .get(
                    &mut *self.store,
                    scriptw_aot_wasm::snapshot_symbol_index(name) as u32,
                )
                .map_err(invariant)?;
            let candidate = eq_reference(self.store, raw)?
                .ok_or_else(|| invariant("null well-known Symbol"))?;
            self.require(candidate, L::Symbol)?;
            if Rooted::ref_eq(&*self.store, symbol, &candidate).map_err(invariant)? {
                if registry.is_some() {
                    return Err(invariant("registered well-known Symbol"));
                }
                return Ok(SnapshotSymbolData {
                    description,
                    origin: SnapshotSymbolOrigin::WellKnown { name },
                });
            }
        }
        let origin = match registry {
            Some(key) => SnapshotSymbolOrigin::Registry {
                key: self.string(key, budget)?,
            },
            None => SnapshotSymbolOrigin::Local {},
        };
        Ok(SnapshotSymbolData {
            description,
            origin,
        })
    }
    fn object(
        &mut self,
        object: &Self::Object,
        budget: &mut SnapshotBudget,
    ) -> Result<SnapshotObjectData<NativeValue, Self::Symbol, Self::Realm>, SnapshotRejection> {
        let object = *object;
        let (kind, header, array) = if self.has_layout(object, L::Object)? {
            (SnapshotRawObjectKind::Ordinary, object, None)
        } else if self.has_layout(object, L::Error)? {
            (
                SnapshotRawObjectKind::Ordinary,
                self.required_reference(object, F::ErrorObject)?,
                None,
            )
        } else if self.has_layout(object, L::Array)? {
            (
                SnapshotRawObjectKind::Array,
                self.required_reference(object, F::ArrayObject)?,
                Some(object),
            )
        } else if self.has_layout(object, L::Function)? {
            if self.boolean(object, F::FunctionHtmlDda)? {
                return Err(SnapshotRejection::UnsupportedExotic {
                    exotic: SnapshotExotic::Other,
                });
            }
            let code = self.required_reference(object, F::FunctionCode)?;
            let protocol = self.integer(code, F::CodeProtocol)? as i32;
            let (execution, constructable) = scriptw_aot_wasm::snapshot_function_protocol(protocol)
                .ok_or_else(|| invariant("invalid function protocol"))?;
            match execution {
                scriptw_ir::FunctionExecutionKind::Ordinary => {}
                scriptw_ir::FunctionExecutionKind::Generator => {
                    return Err(SnapshotRejection::UnsupportedExotic {
                        exotic: SnapshotExotic::GeneratorFunction,
                    });
                }
                scriptw_ir::FunctionExecutionKind::Async
                | scriptw_ir::FunctionExecutionKind::AsyncGenerator => {
                    return Err(SnapshotRejection::UnsupportedExotic {
                        exotic: SnapshotExotic::AsyncFunction,
                    });
                }
            }
            let context = self.required_reference(object, F::FunctionContext)?;
            let realm = self.required_reference(context, F::ContextRealm)?;
            self.require(realm, L::Realm)?;
            (
                SnapshotRawObjectKind::Function {
                    constructable,
                    realm,
                },
                self.required_reference(object, F::FunctionObject)?,
                None,
            )
        } else {
            let exotic = if self.has_layout(object, L::Proxy)? {
                SnapshotExotic::Proxy
            } else if self.has_layout(object, L::BoundFunction)? {
                SnapshotExotic::BoundFunction
            } else if self.has_layout(object, L::Boxed)? {
                SnapshotExotic::BoxedPrimitive
            } else if self.has_layout(object, L::Promise)? {
                SnapshotExotic::Promise
            } else {
                SnapshotExotic::Other
            };
            return Err(SnapshotRejection::UnsupportedExotic { exotic });
        };
        let private = self.required_reference(header, F::ObjectPrivate)?;
        let private = self.array(private, L::PrivateElements)?;
        budget.work(private.len(&*self.store).map_err(invariant)? as usize)?;
        // A private element has observable brand/field semantics beyond this
        // initial ordinary property graph domain, even on an ordinary header.
        for index in 0..private.len(&*self.store).map_err(invariant)? {
            let value = private.get(&mut *self.store, index).map_err(invariant)?;
            if eq_reference(self.store, value)?.is_some() {
                return Err(SnapshotRejection::UnsupportedExotic {
                    exotic: SnapshotExotic::Other,
                });
            }
        }
        let extensible = self.boolean(header, F::ObjectExtensible)?;
        // The field retains the actual object payload, including exotic wrappers.
        let prototype_reference = self.reference(header, F::ObjectPrototype)?;
        let prototype_tag = if let Some(reference) = prototype_reference {
            if self.has_layout(reference, L::Array)? {
                WasmRuntimeValueTag::Array
            } else if self.has_layout(reference, L::Function)?
                || self.has_layout(reference, L::BoundFunction)?
            {
                WasmRuntimeValueTag::Function
            } else if self.has_layout(reference, L::Arguments)? {
                WasmRuntimeValueTag::Arguments
            } else {
                WasmRuntimeValueTag::Object
            }
        } else {
            WasmRuntimeValueTag::Null
        };
        let prototype = NativeValue {
            tag: prototype_tag as i32,
            scalar: 0,
            reference: prototype_reference,
        };
        let anchors = self.anchors(object, budget)?;
        let mut ordered = Vec::<(
            u8,
            u32,
            usize,
            SnapshotRawProperty<NativeValue, Rooted<EqRef>>,
        )>::new();
        let remaining = budget.remaining(SnapshotBudgetDimension::Properties) as usize;
        if let Some(array_object) = array {
            let length = self.integer(array_object, F::ArrayLength)?;
            if !(0..=u32::MAX as i64).contains(&length) {
                return Err(invariant("invalid Array length"));
            }
            let storage = self.required_reference(array_object, F::ArrayStorage)?;
            let count = self.integer(storage, F::ArrayCount)?;
            if count < 0 || count as usize >= remaining {
                return Err(cap(SnapshotBudgetDimension::Properties));
            }
            let buckets = self.required_reference(storage, F::ArrayBuckets)?;
            let buckets = self.array(buckets, L::ArrayBuckets)?;
            let bucket_count = buckets.len(&*self.store).map_err(invariant)?;
            budget.work(bucket_count as usize)?;
            let dense = self.required_reference(storage, F::ArrayDense)?;
            let dense = self.array(dense, L::ArrayDense)?;
            let dense_capacity = dense.len(&*self.store).map_err(invariant)?;
            let numbers = self.required_reference(storage, F::ArrayNumbers)?;
            let numbers = self.array(numbers, L::ArrayNumbers)?;
            let number_capacity = numbers.len(&*self.store).map_err(invariant)?;
            let presence = self.required_reference(storage, F::ArrayNumberPresent)?;
            let presence = self.array(presence, L::ArrayNumberPresence)?;
            let presence_words = presence.len(&*self.store).map_err(invariant)?;
            let values = self
                .reference(storage, F::ArrayValues)?
                .map(|value| self.array(value, L::ArrayValues))
                .transpose()?;
            let value_capacity = values
                .as_ref()
                .map(|values| values.len(&*self.store).map_err(invariant))
                .transpose()?
                .unwrap_or(0);
            let mode = match self.field(storage, F::ArrayMode)? {
                WasmtimeVal::I32(code) => scriptw_aot_wasm::snapshot_array_storage_mode(code)
                    .ok_or_else(|| invariant("unknown Array storage mode"))?,
                _ => return Err(invariant("Array storage mode is not I32")),
            };
            use scriptw_aot_wasm::ArrayIndexedStorageMode;
            match mode {
                ArrayIndexedStorageMode::PackedNumbers => {
                    if bucket_count != 0
                        || dense_capacity != 0
                        || values.is_some()
                        || number_capacity > (1 << 20)
                        || (number_capacity != 0 && !number_capacity.is_power_of_two())
                        || presence_words != number_capacity.div_ceil(64)
                        || count as u64 > u64::from(number_capacity)
                    {
                        return Err(invariant("invalid packed Number Array capacity/count"));
                    }
                    budget.work(count as usize + presence_words as usize)?;
                    for word in 0..presence_words {
                        let WasmtimeVal::I64(raw) =
                            presence.get(&mut *self.store, word).map_err(invariant)?
                        else {
                            return Err(invariant("Array presence word is not I64"));
                        };
                        let mut bits = raw as u64;
                        if word + 1 == presence_words
                            && number_capacity % 64 != 0
                            && bits >> (number_capacity % 64) != 0
                        {
                            return Err(invariant("Array presence has bits beyond capacity"));
                        }
                        while bits != 0 {
                            let index = word * 64 + bits.trailing_zeros();
                            bits &= bits - 1;
                            if i64::from(index) >= length || ordered.len() >= count as usize {
                                return Err(invariant("invalid occupied packed Array index/count"));
                            }
                            let WasmtimeVal::I64(scalar) =
                                numbers.get(&mut *self.store, index).map_err(invariant)?
                            else {
                                return Err(invariant("Array Number bits are not I64"));
                            };
                            let key = budget.collect_utf16(
                                index
                                    .to_string()
                                    .encode_utf16()
                                    .collect::<Vec<_>>()
                                    .into_iter(),
                            )?;
                            ordered.push((
                                0,
                                index,
                                ordered.len(),
                                SnapshotRawProperty {
                                    key: SnapshotRawKey::String(key),
                                    descriptor: SnapshotRawDescriptor::Data {
                                        value: NativeValue {
                                            tag: WasmRuntimeValueTag::Number as i32,
                                            scalar,
                                            reference: None,
                                        },
                                        writable: true,
                                        enumerable: true,
                                        configurable: true,
                                    },
                                },
                            ));
                        }
                    }
                }
                ArrayIndexedStorageMode::PackedValues => {
                    if bucket_count != 0
                        || dense_capacity != 0
                        || number_capacity != 0
                        || presence_words != 0
                        || value_capacity > (1 << 20)
                        || (value_capacity != 0 && !value_capacity.is_power_of_two())
                        || count as u64 > u64::from(value_capacity)
                    {
                        return Err(invariant("invalid packed value Array capacity/count"));
                    }
                    budget.work(value_capacity as usize)?;
                    if let Some(values) = &values {
                        for index in 0..value_capacity {
                            let raw = values.get(&mut *self.store, index).map_err(invariant)?;
                            let Some(stored) = eq_reference(self.store, raw)? else {
                                continue;
                            };
                            if i64::from(index) >= length || ordered.len() >= count as usize {
                                return Err(invariant(
                                    "invalid occupied packed value Array index/count",
                                ));
                            }
                            self.require(stored, L::Stored)?;
                            let value = self.stored(stored)?;
                            let key = budget.collect_utf16(
                                index
                                    .to_string()
                                    .encode_utf16()
                                    .collect::<Vec<_>>()
                                    .into_iter(),
                            )?;
                            ordered.push((
                                0,
                                index,
                                ordered.len(),
                                SnapshotRawProperty {
                                    key: SnapshotRawKey::String(key),
                                    descriptor: SnapshotRawDescriptor::Data {
                                        value,
                                        writable: true,
                                        enumerable: true,
                                        configurable: true,
                                    },
                                },
                            ));
                        }
                    }
                }
                ArrayIndexedStorageMode::DenseDescriptors => {
                    if bucket_count != 0
                        || number_capacity != 0
                        || presence_words != 0
                        || values.is_some()
                    {
                        return Err(invariant("inactive Array backing is not empty"));
                    }
                    if dense_capacity > (1 << 20)
                        || (dense_capacity != 0 && !dense_capacity.is_power_of_two())
                        || count as u64 > u64::from(dense_capacity)
                    {
                        return Err(invariant("invalid dense Array capacity/count"));
                    }
                    budget.work(dense_capacity as usize)?;
                    for index in 0..dense_capacity {
                        let raw = dense.get(&mut *self.store, index).map_err(invariant)?;
                        let Some(descriptor) = eq_reference(self.store, raw)? else {
                            continue;
                        };
                        if i64::from(index) >= length || ordered.len() >= count as usize {
                            return Err(invariant("invalid occupied dense Array index/count"));
                        }
                        let key = budget.collect_utf16(
                            index
                                .to_string()
                                .encode_utf16()
                                .collect::<Vec<_>>()
                                .into_iter(),
                        )?;
                        let property = SnapshotRawProperty {
                            key: SnapshotRawKey::String(key),
                            descriptor: self.descriptor(descriptor, budget)?,
                        };
                        ordered.push((0, index, ordered.len(), property));
                    }
                }
                ArrayIndexedStorageMode::SparseDescriptors => {
                    if dense_capacity != 0
                        || number_capacity != 0
                        || presence_words != 0
                        || values.is_some()
                        || (bucket_count == 0 && count != 0)
                        || (bucket_count != 0 && !bucket_count.is_power_of_two())
                    {
                        return Err(invariant("invalid sparse Array storage mode"));
                    }
                    for bucket in 0..bucket_count {
                        let raw = buckets.get(&mut *self.store, bucket).map_err(invariant)?;
                        let mut entry = eq_reference(self.store, raw)?;
                        while let Some(current) = entry {
                            budget.work(1)?;
                            if ordered.len() >= count as usize {
                                return Err(invariant("Array index count/chain mismatch"));
                            }
                            let index = self.integer(current, F::ArrayIndex)?;
                            if index < 0 || index >= length || index >= u32::MAX as i64 {
                                return Err(invariant("invalid occupied Array index"));
                            }
                            let descriptor =
                                self.required_reference(current, F::ArrayDescriptor)?;
                            let key = budget.collect_utf16(
                                (index as u32)
                                    .to_string()
                                    .encode_utf16()
                                    .collect::<Vec<_>>()
                                    .into_iter(),
                            )?;
                            let property = SnapshotRawProperty {
                                key: SnapshotRawKey::String(key),
                                descriptor: self.descriptor(descriptor, budget)?,
                            };
                            ordered.push((0, index as u32, ordered.len(), property));
                            entry = self.reference(current, F::ArrayNext)?;
                        }
                    }
                }
            }
            if ordered.len() != count as usize {
                return Err(invariant("Array index count mismatch"));
            }
            ordered.sort_by_key(|entry| entry.1);
            if ordered.windows(2).any(|pair| pair[0].1 == pair[1].1) {
                return Err(invariant("duplicate Array index"));
            }
            let key = budget.copy_utf16(&[108, 101, 110, 103, 116, 104])?;
            let writable = self.boolean(array_object, F::ArrayLengthWritable)?;
            ordered.push((
                1,
                0,
                0,
                SnapshotRawProperty {
                    key: SnapshotRawKey::String(key),
                    descriptor: SnapshotRawDescriptor::Data {
                        value: NativeValue {
                            tag: WasmRuntimeValueTag::Number as i32,
                            scalar: (length as f64).to_bits() as i64,
                            reference: None,
                        },
                        writable,
                        enumerable: false,
                        configurable: false,
                    },
                },
            ));
        }
        let storage = self.required_reference(header, F::ObjectProperties)?;
        let compact_kind = if self.has_layout(storage, L::ArrayCompactStorage)? {
            Some(NativeCompactStorageKind::Arrays)
        } else if self.has_layout(storage, L::InlineCompactStorage)? {
            Some(NativeCompactStorageKind::Inline)
        } else {
            None
        };
        if let Some(kind) = compact_kind {
            if array.is_some() {
                return Err(invariant("compact literal storage on Array header"));
            }
            let shape_field = match kind {
                NativeCompactStorageKind::Arrays => F::ArrayCompactShape,
                NativeCompactStorageKind::Inline => F::InlineCompactShape,
            };
            let shape = self.required_reference(storage, shape_field)?;
            let keys = self.required_reference(shape, F::CompactKeys)?;
            let attributes = u32::try_from(self.integer(shape, F::CompactAttributes)?)
                .map_err(|_| invariant("compact attributes exceed their unsigned word"))?;
            let (accessor, writable, enumerable, configurable) =
                scriptw_aot_wasm::snapshot_descriptor_flags(attributes)
                    .ok_or_else(|| invariant("compact attributes have unknown bits"))?;
            if accessor {
                return Err(invariant("compact data shape has accessor attributes"));
            }
            let keys = self.array(keys, L::CompactKeys)?;
            let length = keys.len(&*self.store).map_err(invariant)?;
            let payloads = match kind {
                NativeCompactStorageKind::Arrays => {
                    let tags = self.required_reference(storage, F::CompactTags)?;
                    let scalars = self.reference(storage, F::CompactScalars)?;
                    let references = self.reference(storage, F::CompactReferences)?;
                    let tags = self.array(tags, L::CompactTags)?;
                    let scalars = scalars
                        .map(|value| self.array(value, L::CompactScalars))
                        .transpose()?;
                    let references = references
                        .map(|value| self.array(value, L::CompactReferences))
                        .transpose()?;
                    if length > 16
                        || tags.len(&*self.store).map_err(invariant)? != length
                        || scalars
                            .as_ref()
                            .map(|values| values.len(&*self.store).map_err(invariant))
                            .transpose()?
                            .is_some_and(|count| count != length)
                        || references
                            .as_ref()
                            .map(|values| values.len(&*self.store).map_err(invariant))
                            .transpose()?
                            .is_some_and(|count| count != length)
                    {
                        return Err(invariant("invalid array compact property extent"));
                    }
                    NativeCompactPayloads::Arrays {
                        tags,
                        scalars,
                        references,
                    }
                }
                NativeCompactStorageKind::Inline => {
                    if !matches!(length, 1 | 2) {
                        return Err(invariant("invalid inline compact property extent"));
                    }
                    NativeCompactPayloads::Inline {
                        packed_tags: self.integer(storage, F::InlinePackedTags)? as u32,
                        scalars: [
                            self.integer(storage, F::InlineScalar0)?,
                            self.integer(storage, F::InlineScalar1)?,
                        ],
                        references: [
                            self.reference(storage, F::InlineReference0)?,
                            self.reference(storage, F::InlineReference1)?,
                        ],
                    }
                }
            };
            budget.work(length as usize)?;
            for ordinal in 0..length {
                if ordered.len() >= remaining {
                    return Err(cap(SnapshotBudgetDimension::Properties));
                }
                let raw = keys.get(&mut *self.store, ordinal).map_err(invariant)?;
                let key =
                    eq_reference(self.store, raw)?.ok_or_else(|| invariant("null compact key"))?;
                self.require(key, L::String)?;
                let key = self.string(key, budget)?;
                let index = array_index(key.units());
                let (tag, scalar, reference) = match &payloads {
                    NativeCompactPayloads::Arrays {
                        tags,
                        scalars,
                        references,
                    } => {
                        let WasmtimeVal::I32(tag) =
                            tags.get(&mut *self.store, ordinal).map_err(invariant)?
                        else {
                            return Err(invariant("invalid compact value tag"));
                        };
                        let scalar = match scalars {
                            Some(scalars) => {
                                let WasmtimeVal::I64(value) =
                                    scalars.get(&mut *self.store, ordinal).map_err(invariant)?
                                else {
                                    return Err(invariant("invalid compact value scalar"));
                                };
                                value
                            }
                            None => 0,
                        };
                        let reference = match references {
                            Some(references) => {
                                let raw = references
                                    .get(&mut *self.store, ordinal)
                                    .map_err(invariant)?;
                                eq_reference(self.store, raw)?
                            }
                            None => None,
                        };
                        (tag, scalar, reference)
                    }
                    NativeCompactPayloads::Inline {
                        packed_tags,
                        scalars,
                        references,
                    } => {
                        let slot = ordinal as usize;
                        (
                            ((packed_tags >> (ordinal * 8)) & 0xff) as i32,
                            scalars[slot],
                            references[slot],
                        )
                    }
                };
                scriptw_aot_wasm::GcHostValue::check(tag, scalar, reference.is_some())
                    .map_err(invariant)?;
                ordered.push((
                    if index.is_some() { 0 } else { 1 },
                    index.unwrap_or(0),
                    ordinal as usize,
                    SnapshotRawProperty {
                        key: SnapshotRawKey::String(key),
                        descriptor: SnapshotRawDescriptor::Data {
                            value: NativeValue {
                                tag,
                                scalar,
                                reference,
                            },
                            writable,
                            enumerable,
                            configurable,
                        },
                    },
                ));
            }
        } else {
            let length = match self.field(storage, F::PropertiesLength)? {
                WasmtimeVal::I32(extent) => extent as u32,
                _ => return Err(invariant("invalid ordinary property extent")),
            };
            let properties = self.required_reference(storage, F::PropertiesEntries)?;
            let properties = self.array(properties, L::Properties)?;
            if length > properties.len(&*self.store).map_err(invariant)? {
                return Err(invariant("ordinary property extent exceeds capacity"));
            }
            budget.work(length as usize)?;
            for ordinal in 0..length {
                let raw = properties
                    .get(&mut *self.store, ordinal)
                    .map_err(invariant)?;
                let Some(entry) = eq_reference(self.store, raw)? else {
                    continue;
                };
                if ordered.len() >= remaining {
                    return Err(cap(SnapshotBudgetDimension::Properties));
                }
                let key = self.stored_field(entry, F::PropertyKey)?;
                let (group, index, key) = match WasmRuntimeValueTag::from_tag(key.tag) {
                    Some(WasmRuntimeValueTag::String) => {
                        let key = self.string(
                            key.reference
                                .ok_or_else(|| invariant("missing property key"))?,
                            budget,
                        )?;
                        let index = array_index(key.units());
                        if array.is_some()
                            && (index.is_some() || key.units() == [108, 101, 110, 103, 116, 104])
                        {
                            return Err(invariant(
                                "Array virtual property duplicated in ordinary table",
                            ));
                        }
                        (
                            if index.is_some() { 0 } else { 1 },
                            index.unwrap_or(0),
                            SnapshotRawKey::String(key),
                        )
                    }
                    Some(WasmRuntimeValueTag::Symbol) => {
                        let key = key
                            .reference
                            .ok_or_else(|| invariant("missing Symbol key"))?;
                        self.require(key, L::Symbol)?;
                        (2, 0, SnapshotRawKey::Symbol(key))
                    }
                    _ => return Err(invariant("invalid ordinary property key")),
                };
                let descriptor = self.required_reference(entry, F::PropertyDescriptor)?;
                let property = SnapshotRawProperty {
                    key,
                    descriptor: self.descriptor(descriptor, budget)?,
                };
                ordered.push((
                    group,
                    index,
                    ordinal as usize + usize::from(array.is_some()),
                    property,
                ));
            }
        }
        ordered.sort_by_key(|(group, index, ordinal, _)| (*group, *index, *ordinal));
        let mut properties = SnapshotProperties::default();
        for (_, _, _, property) in ordered {
            properties.push(budget, property)?;
        }
        Ok(SnapshotObjectData {
            kind,
            extensible,
            prototype,
            anchors,
            properties,
        })
    }
}

fn array_index(units: &[u16]) -> Option<u32> {
    if units.is_empty() || units.len() > 10 || (units.len() > 1 && units[0] == b'0' as u16) {
        return None;
    }
    let mut value = 0u32;
    for unit in units {
        let digit = unit.checked_sub(b'0' as u16).filter(|digit| *digit <= 9)?;
        value = value.checked_mul(10)?.checked_add(u32::from(digit))?;
    }
    (value != u32::MAX).then_some(value)
}

fn root_global(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    instance: &Instance,
    name: &str,
) -> Result<Option<Rooted<EqRef>>, SnapshotRejection> {
    let global = instance
        .get_global(&mut *store, name)
        .ok_or_else(|| invariant(format!("missing {name}")))?;
    let value = global.get(&mut *store);
    eq_reference(store, value)
}
fn eq_reference(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    value: WasmtimeVal,
) -> Result<Option<Rooted<EqRef>>, SnapshotRejection> {
    match value {
        WasmtimeVal::AnyRef(Some(reference)) => reference
            .as_eqref(&mut *store)
            .map_err(invariant)?
            .map(Some)
            .ok_or_else(|| invariant("reference is not EqRef")),
        WasmtimeVal::AnyRef(None) => Ok(None),
        _ => Err(invariant("field is not a GC reference")),
    }
}
