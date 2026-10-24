//! Rooted Main results decoded from the same typed schema used by codegen.

use super::*;
use scriptw_aot_wasm::{
    check_gc_main_completion, GcHostField, GcHostLayout, GcHostStorage, GcHostValue,
};
use wasmtime::{AsContextMut, EqRef, Rooted, StorageType, ValType};

pub(super) type GcMainCompletion = (i32, i64, Option<Rooted<EqRef>>, i32, i32);

pub(super) struct GcObservedCompletion {
    tag: WasmRuntimeValueTag,
    kind: WasmTopLevelCompletionKind,
    value: ObservedJsValue,
}

impl GcObservedCompletion {
    pub(super) fn into_parts(
        self,
    ) -> (
        WasmRuntimeValueTag,
        WasmTopLevelCompletionKind,
        ObservedJsValue,
    ) {
        (self.tag, self.kind, self.value)
    }
}

fn abi_error(message: impl std::fmt::Display) -> EngineError {
    EngineError::new(format!("invalid Wasm GC completion ABI: {message}"))
}

pub(super) fn observe(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    (tag, scalar, reference, kind, target): GcMainCompletion,
) -> Result<GcObservedCompletion, EngineError> {
    let kind = match check_gc_main_completion(kind, target).map_err(abi_error)? {
        CompletionKindIr::Normal => WasmTopLevelCompletionKind::Normal,
        CompletionKindIr::Throw => WasmTopLevelCompletionKind::Throw,
        CompletionKindIr::Return
        | CompletionKindIr::Break
        | CompletionKindIr::Continue
        | CompletionKindIr::Empty => {
            return Err(abi_error("unresolved local completion"));
        }
    };
    let checked = GcHostValue::check(tag, scalar, reference.is_some()).map_err(abi_error)?;
    let value = match checked.tag() {
        WasmRuntimeValueTag::Undefined => ObservedJsValue::Undefined,
        WasmRuntimeValueTag::Null => ObservedJsValue::Null,
        WasmRuntimeValueTag::Boolean => ObservedJsValue::Boolean(checked.scalar() == 1),
        WasmRuntimeValueTag::Number => {
            ObservedJsValue::Number(ObservedNumber::from_bits(checked.scalar() as u64))
        }
        WasmRuntimeValueTag::String => {
            let reference = reference.ok_or_else(|| abi_error("missing String root"))?;
            ObservedJsValue::String(string_units(store, reference)?)
        }
        WasmRuntimeValueTag::BigInt => {
            let reference = reference.ok_or_else(|| abi_error("missing BigInt root"))?;
            let decimal = bigint_decimal(store, reference)?;
            ObservedJsValue::BigInt(
                ObservedBigInt::parse_canonical_decimal(decimal.into_boxed_str())
                    .map_err(abi_error)?,
            )
        }
        WasmRuntimeValueTag::Symbol => ObservedJsValue::Symbol,
        WasmRuntimeValueTag::Object
        | WasmRuntimeValueTag::Array
        | WasmRuntimeValueTag::Function
        | WasmRuntimeValueTag::Arguments => ObservedJsValue::Object,
    };
    Ok(GcObservedCompletion {
        tag: checked.tag(),
        kind,
        value,
    })
}

pub(super) fn field(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    reference: Rooted<EqRef>,
    field: GcHostField,
) -> Result<WasmtimeVal, EngineError> {
    let definition = field.definition();
    let structure = reference
        .as_struct(&*store)
        .map_err(abi_error)?
        .ok_or_else(|| abi_error("value is not the required GC struct"))?;
    let ty = structure.ty(&*store).map_err(abi_error)?;
    if definition.layout().field_count() != Some(ty.fields().len() as u32) {
        return Err(abi_error(
            "struct field count disagrees with compiled schema",
        ));
    }
    let field_ty = ty
        .field(definition.index() as usize)
        .ok_or_else(|| abi_error("schema field is absent"))?;
    if (field_ty.mutability() == wasmtime::Mutability::Var) != definition.mutable()
        || !storage_matches(definition.storage(), field_ty.element_type())
    {
        return Err(abi_error(
            "struct field storage disagrees with compiled schema",
        ));
    }
    structure
        .field(&mut *store, definition.index() as usize)
        .map_err(abi_error)
}

pub(super) fn storage_matches(expected: GcHostStorage, actual: &StorageType) -> bool {
    match (expected, actual) {
        (GcHostStorage::I32, StorageType::ValType(ValType::I32)) => true,
        (GcHostStorage::I64, StorageType::ValType(ValType::I64)) => true,
        (GcHostStorage::PackedU8, StorageType::I8) => true,
        (GcHostStorage::PackedU16, StorageType::I16) => true,
        (GcHostStorage::NullableEqRef, StorageType::ValType(ValType::Ref(reference))) => {
            reference.is_nullable() && reference.heap_type().matches(&wasmtime::HeapType::Eq)
        }
        (
            GcHostStorage::FunctionReference { nullable },
            StorageType::ValType(ValType::Ref(reference)),
        ) => {
            reference.is_nullable() == nullable
                && matches!(reference.heap_type(), wasmtime::HeapType::ConcreteFunc(_))
        }
        (
            GcHostStorage::Reference { layout, nullable },
            StorageType::ValType(ValType::Ref(reference)),
        ) => {
            reference.is_nullable() == nullable
                && match (layout.array_element(), reference.heap_type()) {
                    (Some(_), wasmtime::HeapType::ConcreteArray(_)) => true,
                    (None, wasmtime::HeapType::ConcreteStruct(_)) => true,
                    _ => false,
                }
        }
        (
            GcHostStorage::I32
            | GcHostStorage::I64
            | GcHostStorage::PackedU8
            | GcHostStorage::PackedU16
            | GcHostStorage::NullableEqRef
            | GcHostStorage::FunctionReference { .. }
            | GcHostStorage::Reference { .. },
            _,
        ) => false,
    }
}

fn array_field(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    reference: Rooted<EqRef>,
    field_name: GcHostField,
    layout: GcHostLayout,
) -> Result<Rooted<wasmtime::ArrayRef>, EngineError> {
    let value = field(store, reference, field_name)?;
    let WasmtimeVal::AnyRef(Some(reference)) = value else {
        return Err(abi_error(
            "required GC array field is null or has wrong type",
        ));
    };
    let array = reference
        .as_array(&*store)
        .map_err(abi_error)?
        .ok_or_else(|| abi_error("field is not a GC array"))?;
    let ty = array.ty(&*store).map_err(abi_error)?;
    let Some(element) = layout.array_element() else {
        return Err(abi_error("schema layout is not an array"));
    };
    if layout.array_mutable() != Some(ty.mutability() == wasmtime::Mutability::Var)
        || !storage_matches(element, &ty.element_type())
    {
        return Err(abi_error("GC array storage disagrees with compiled schema"));
    }
    Ok(array)
}

pub(super) fn string_units(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    reference: Rooted<EqRef>,
) -> Result<Box<[u16]>, EngineError> {
    let array = array_field(
        store,
        reference,
        GcHostField::StringCodeUnits,
        GcHostLayout::CodeUnits,
    )?;
    let length = array.len(&*store).map_err(abi_error)?;
    let mut units = Vec::new();
    units
        .try_reserve_exact(length as usize)
        .map_err(abi_error)?;
    for index in 0..length {
        let WasmtimeVal::I32(unit) = array.get(&mut *store, index).map_err(abi_error)? else {
            return Err(abi_error("packed code unit did not produce I32"));
        };
        units.push(u16::try_from(unit).map_err(abi_error)?);
    }
    Ok(units.into_boxed_slice())
}

fn bigint_decimal(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    reference: Rooted<EqRef>,
) -> Result<String, EngineError> {
    let WasmtimeVal::I32(negative) = field(store, reference, GcHostField::BigIntNegative)? else {
        return Err(abi_error("BigInt sign is not I32"));
    };
    let negative = match negative {
        0 => false,
        1 => true,
        _ => return Err(abi_error("BigInt sign is not Boolean")),
    };
    let array = array_field(
        store,
        reference,
        GcHostField::BigIntLimbs,
        GcHostLayout::BigIntLimbs,
    )?;
    let length = array.len(&*store).map_err(abi_error)?;
    let mut limbs = Vec::new();
    limbs
        .try_reserve_exact(length as usize)
        .map_err(abi_error)?;
    for index in 0..length {
        let WasmtimeVal::I64(limb) = array.get(&mut *store, index).map_err(abi_error)? else {
            return Err(abi_error("BigInt limb is not I64"));
        };
        limbs.push(limb as u64);
    }
    magnitude_decimal(negative, limbs)
}

fn magnitude_decimal(negative: bool, mut limbs: Vec<u64>) -> Result<String, EngineError> {
    if limbs.is_empty() {
        return if negative {
            Err(abi_error("negative zero BigInt"))
        } else {
            Ok("0".to_string())
        };
    }
    if limbs.last() == Some(&0) {
        return Err(abi_error("BigInt magnitude has an unused high limb"));
    }
    let mut chunks = Vec::new();
    while !limbs.is_empty() {
        let mut remainder = 0_u128;
        for limb in limbs.iter_mut().rev() {
            let dividend = (remainder << 64) | u128::from(*limb);
            *limb = (dividend / 1_000_000_000) as u64;
            remainder = dividend % 1_000_000_000;
        }
        chunks.push(remainder as u32);
        while limbs.last() == Some(&0) {
            limbs.pop();
        }
    }
    let mut chunks = chunks.into_iter().rev();
    let first = chunks
        .next()
        .ok_or_else(|| abi_error("missing BigInt decimal magnitude"))?;
    let mut decimal = if negative {
        format!("-{first}")
    } else {
        first.to_string()
    };
    for chunk in chunks {
        use std::fmt::Write;
        write!(&mut decimal, "{chunk:09}").map_err(abi_error)?;
    }
    Ok(decimal)
}

pub(super) enum DiagnosticString {
    ErrorName,
    ErrorMessage,
    ErrorConstructorName,
}

impl DiagnosticString {
    fn export_name(self) -> &'static str {
        match self {
            Self::ErrorName => WASM_THROW_ERROR_NAME_EXPORT,
            Self::ErrorMessage => WASM_THROW_ERROR_MESSAGE_EXPORT,
            Self::ErrorConstructorName => WASM_THROW_ERROR_CONSTRUCTOR_NAME_EXPORT,
        }
    }
}

pub(super) fn diagnostic_string(
    instance: &wasmtime::Instance,
    store: &mut impl AsContextMut<Data = WasmHostState>,
    diagnostic: DiagnosticString,
) -> Result<Option<String>, EngineError> {
    let global_name = diagnostic.export_name();
    let Some(global) = instance.get_global(&mut *store, global_name) else {
        return Ok(None);
    };
    let WasmtimeVal::AnyRef(reference) = global.get(&mut *store) else {
        return Err(abi_error(format!(
            "{global_name} is not a reference global"
        )));
    };
    let Some(reference) = reference else {
        return Ok(None);
    };
    let reference = reference
        .as_eqref(&mut *store)
        .map_err(abi_error)?
        .ok_or_else(|| abi_error("diagnostic is not an EqRef"))?;
    let units = string_units(store, reference)?;
    Ok(Some(String::from_utf16_lossy(&units)))
}

pub(super) fn render_value(
    tag: WasmRuntimeValueTag,
    value: &ObservedJsValue,
    message: Option<&str>,
) -> String {
    if matches!(value, ObservedJsValue::Object | ObservedJsValue::Symbol) {
        let category = tag.value_kind().as_str();
        return match message {
            Some(message) => format!("wasm-aot completion: {category}: {message}"),
            None => format!("wasm-aot completion: {category}"),
        };
    }
    let value = match value {
        ObservedJsValue::Undefined => "undefined".to_string(),
        ObservedJsValue::Null => "null".to_string(),
        ObservedJsValue::Boolean(value) => value.to_string(),
        ObservedJsValue::Number(value) => f64::from_bits(value.bits()).to_string(),
        ObservedJsValue::String(units) => String::from_utf16_lossy(units),
        ObservedJsValue::BigInt(value) => format!("{}n", value.as_str()),
        ObservedJsValue::Symbol => "symbol".to_string(),
        ObservedJsValue::Object => match message {
            Some(message) => format!("object: {message}"),
            None => "object".to_string(),
        },
    };
    format!(
        "wasm-aot completion: {}({value})",
        tag.value_kind().as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmtime::{
        ArrayRef, ArrayRefPre, ArrayType, FieldType, HeapType, Mutability, RefType, StructRef,
        StructRefPre, StructType,
    };

    fn host_store() -> WasmtimeStore<WasmHostState> {
        let engine = shared_wasm_engine().expect("required copying collector");
        let realm = RealmBuilder::new().build();
        let mut store = WasmtimeStore::new(
            &engine,
            WasmHostState {
                user_data: None,
                embedding: None,
                weak: wasm_gc_weak_host::WeakGcState::default(),
                native_source_compiler: None,
                native_template_instances: Default::default(),
                monotonic_clock_origin: realm.host_clock().monotonic_instant(),
                realm,
                output_events: WasmOutputEvents::DelegateOnly,
                intl_kernel: shared_embedded_intl_kernel().expect("embedded native Intl"),
                can_block: true,
                shared_memory_backing: None,
                async_waiters: WasmStoreAsyncWaiters::new(None),
                agent_group: None,
                agent_commands: None,
                agent_leaving: None,
                limits: WasmtimeStoreLimitsBuilder::new()
                    .memory_size(WASM_STORE_MEMORY_CAP_BYTES)
                    .build(),
            },
        );
        store.limiter(|state| &mut state.limits);
        store.set_epoch_deadline(u64::MAX / 2);
        store
    }

    fn string(
        store: &mut impl AsContextMut<Data = WasmHostState>,
        units: &[u16],
        storage: StorageType,
    ) -> Rooted<EqRef> {
        let engine = store.as_context().engine().clone();
        let array_ty = ArrayType::new(&engine, FieldType::new(Mutability::Var, storage));
        let array_pre = ArrayRefPre::new(&mut *store, array_ty.clone());
        let values: Vec<_> = units
            .iter()
            .map(|unit| WasmtimeVal::I32(i32::from(*unit)))
            .collect();
        let array =
            ArrayRef::new_fixed(&mut *store, &array_pre, &values).expect("native code-unit array");
        let structure_ty = StructType::new(
            &engine,
            [
                FieldType::new(
                    Mutability::Const,
                    StorageType::ValType(ValType::Ref(RefType::new(
                        false,
                        HeapType::ConcreteArray(array_ty),
                    ))),
                ),
                FieldType::new(Mutability::Var, StorageType::ValType(ValType::I64)),
            ],
        )
        .expect("native String schema shape");
        let pre = StructRefPre::new(&mut *store, structure_ty);
        StructRef::new(
            &mut *store,
            &pre,
            &[WasmtimeVal::from(array), WasmtimeVal::I64(0)],
        )
        .expect("native String record")
        .to_eqref()
    }

    #[test]
    fn rooted_string_survives_collection_and_preserves_every_utf16_code_unit() {
        let mut store = host_store();
        let mut roots = wasmtime::RootScope::new(&mut store);
        let units = [0, 0x0061, 0xd800, 0x0062, 0xdc00, 0xd83d, 0xde00, 0xffff];
        let reference = string(&mut roots, &units, StorageType::I16);
        roots
            .as_context_mut()
            .gc(None)
            .expect("collect with live String root");
        let result = observe(
            &mut roots,
            (WasmRuntimeValueTag::String as i32, 0, Some(reference), 1, 0),
        )
        .expect("throw retains original rooted String");
        let (_, kind, value) = result.into_parts();
        assert!(matches!(kind, WasmTopLevelCompletionKind::Throw));
        assert_eq!(
            value,
            ObservedJsValue::String(units.to_vec().into_boxed_slice())
        );
    }

    #[test]
    fn completion_admission_refuses_stale_scalars_missing_roots_and_local_targets() {
        let mut store = host_store();
        let mut roots = wasmtime::RootScope::new(&mut store);
        for input in [
            (i32::MAX, 0, None, 0, 0),
            (WasmRuntimeValueTag::Undefined as i32, 1, None, 0, 0),
            (WasmRuntimeValueTag::Boolean as i32, 2, None, 0, 0),
            (WasmRuntimeValueTag::String as i32, 0, None, 0, 0),
            (WasmRuntimeValueTag::BigInt as i32, 0, None, 0, 0),
            (WasmRuntimeValueTag::Object as i32, 0, None, 0, 0),
            (WasmRuntimeValueTag::Undefined as i32, 0, None, 0, 1),
            (WasmRuntimeValueTag::Undefined as i32, 0, None, 2, 0),
            (WasmRuntimeValueTag::Undefined as i32, 0, None, 3, 0),
            (WasmRuntimeValueTag::Undefined as i32, 0, None, 4, 0),
            (WasmRuntimeValueTag::Undefined as i32, 0, None, 5, 0),
        ] {
            assert!(observe(&mut roots, input).is_err());
        }
        let reference = string(&mut roots, &[97], StorageType::I16);
        assert!(observe(
            &mut roots,
            (WasmRuntimeValueTag::Number as i32, 0, Some(reference), 0, 0)
        )
        .is_err());
        assert!(observe(
            &mut roots,
            (WasmRuntimeValueTag::String as i32, 1, Some(reference), 0, 0)
        )
        .is_err());
        let wrong = string(&mut roots, &[97], StorageType::ValType(ValType::I32));
        assert!(observe(
            &mut roots,
            (WasmRuntimeValueTag::String as i32, 0, Some(wrong), 0, 0)
        )
        .is_err());
    }

    #[test]
    fn unsigned_64_bit_magnitudes_render_canonical_decimal_without_host_handles() {
        for (negative, limbs, expected) in [
            (false, vec![], "0"),
            (false, vec![u64::MAX], "18446744073709551615"),
            (true, vec![0, 1], "-18446744073709551616"),
            (
                false,
                vec![u64::MAX, u64::MAX],
                "340282366920938463463374607431768211455",
            ),
            (
                false,
                vec![1, 0, 1],
                "340282366920938463463374607431768211457",
            ),
        ] {
            assert_eq!(magnitude_decimal(negative, limbs).unwrap(), expected);
        }
        assert!(magnitude_decimal(true, vec![]).is_err());
        assert!(magnitude_decimal(false, vec![1, 0]).is_err());
    }

    fn bigint(
        store: &mut impl AsContextMut<Data = WasmHostState>,
        negative: i32,
        limbs: &[u64],
    ) -> Rooted<EqRef> {
        let engine = store.as_context().engine().clone();
        let array_ty = ArrayType::new(
            &engine,
            FieldType::new(Mutability::Var, StorageType::ValType(ValType::I64)),
        );
        let array_pre = ArrayRefPre::new(&mut *store, array_ty.clone());
        let values: Vec<_> = limbs
            .iter()
            .map(|limb| WasmtimeVal::I64(*limb as i64))
            .collect();
        let array = ArrayRef::new_fixed(&mut *store, &array_pre, &values)
            .expect("native unsigned BigInt limbs");
        let structure_ty = StructType::new(
            &engine,
            [
                FieldType::new(Mutability::Const, StorageType::ValType(ValType::I32)),
                FieldType::new(
                    Mutability::Const,
                    StorageType::ValType(ValType::Ref(RefType::new(
                        false,
                        HeapType::ConcreteArray(array_ty),
                    ))),
                ),
            ],
        )
        .expect("native BigInt schema shape");
        let pre = StructRefPre::new(&mut *store, structure_ty);
        StructRef::new(
            &mut *store,
            &pre,
            &[WasmtimeVal::I32(negative), WasmtimeVal::from(array)],
        )
        .expect("native BigInt record")
        .to_eqref()
    }

    #[test]
    fn rooted_bigint_keeps_unsigned_limbs_and_original_throw_across_collection() {
        let mut store = host_store();
        let mut roots = wasmtime::RootScope::new(&mut store);
        for (negative, limbs, expected, kind) in [
            (0, vec![], "0", 0),
            (
                0,
                vec![u64::MAX, u64::MAX],
                "340282366920938463463374607431768211455",
                0,
            ),
            (1, vec![0, 1], "-18446744073709551616", 1),
        ] {
            let reference = bigint(&mut roots, negative, &limbs);
            roots
                .as_context_mut()
                .gc(None)
                .expect("collect with live BigInt root");
            let (_, completion_kind, value) = observe(
                &mut roots,
                (
                    WasmRuntimeValueTag::BigInt as i32,
                    0,
                    Some(reference),
                    kind,
                    0,
                ),
            )
            .expect("canonical GC BigInt completion")
            .into_parts();
            assert!(matches!(
                (kind, completion_kind),
                (0, WasmTopLevelCompletionKind::Normal) | (1, WasmTopLevelCompletionKind::Throw)
            ));
            let ObservedJsValue::BigInt(value) = value else {
                panic!("GC BigInt remains a primitive")
            };
            assert_eq!(value.as_str(), expected);
        }
        for (negative, limbs) in [(2, vec![1]), (1, vec![]), (0, vec![1, 0])] {
            let reference = bigint(&mut roots, negative, &limbs);
            assert!(observe(
                &mut roots,
                (WasmRuntimeValueTag::BigInt as i32, 0, Some(reference), 0, 0)
            )
            .is_err());
        }
    }

    #[test]
    fn collector_release_of_native_resource_returns_its_cleared_byte_range() {
        let mut store = host_store();
        let memory =
            WasmtimeSharedMemory::new(store.engine(), wasmtime::MemoryType::shared(1, 1)).unwrap();
        let backing = WasmSharedMemoryBacking::new(memory);
        let resource = backing.allocate(8, 8, false).unwrap().unwrap();
        let base = resource.base();
        let weak = Arc::downgrade(&resource);
        {
            let mut roots = wasmtime::RootScope::new(&mut store);
            let _reference = wasmtime::ExternRef::new(&mut roots, resource).unwrap();
            roots.as_context_mut().gc(None).unwrap();
            assert!(
                weak.upgrade().is_some(),
                "live Wasm root retains native byte owner"
            );
        }
        store.gc(None).unwrap();
        assert!(
            weak.upgrade().is_none(),
            "collector releases unreachable native bytes"
        );
        let replacement = backing.allocate(8, 8, false).unwrap().unwrap();
        assert_eq!(replacement.base(), base);
    }

    #[test]
    fn legacy_category_text_retains_throw_message_and_never_uses_an_address() {
        assert_eq!(
            render_value(WasmRuntimeValueTag::Object, &ObservedJsValue::Object, None),
            "wasm-aot completion: object"
        );
        assert_eq!(
            render_value(
                WasmRuntimeValueTag::Object,
                &ObservedJsValue::Object,
                Some("marker")
            ),
            "wasm-aot completion: object: marker"
        );
        assert_eq!(
            render_value(
                WasmRuntimeValueTag::Function,
                &ObservedJsValue::Object,
                None
            ),
            "wasm-aot completion: function"
        );
        assert_eq!(
            render_value(WasmRuntimeValueTag::Symbol, &ObservedJsValue::Symbol, None),
            "wasm-aot completion: symbol"
        );
    }
}
