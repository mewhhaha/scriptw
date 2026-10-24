//! Owned values at the foreign ABI. GC arrays never escape a callback.
use super::*;
use scriptw_ir::foreign::{
    ForeignFunctionDeclaration, ForeignFunctionMode, ForeignResultType, ForeignValueType,
};
use wasmtime::{ArrayRef, ArrayRefPre, ArrayType, Caller, ExternType, FuncType, Val};

/// A copied foreign value. UTF-16 preserves lone surrogates and embedded NULs.
#[derive(Debug, Clone, PartialEq)]
pub enum ForeignValue {
    Number(f64),
    StringUtf16(Vec<u16>),
    Bytes(Vec<u8>),
    Void,
}

impl ForeignValue {
    pub(super) fn matches(&self, expected: ForeignResultType) -> bool {
        matches!(
            (self, expected),
            (Self::Number(_), ForeignResultType::Number)
                | (Self::StringUtf16(_), ForeignResultType::StringUtf16)
                | (Self::Bytes(_), ForeignResultType::Bytes)
                | (Self::Void, ForeignResultType::Void)
        )
    }
}

pub(super) fn copy_array(
    store: &mut impl AsContextMut<Data = WasmStoreState>,
    value: &Val,
    units: bool,
) -> wasmtime::Result<ForeignValue> {
    let Val::AnyRef(Some(reference)) = value else {
        return Err(wasmtime::Error::msg(
            "foreign transfer value is not a GC array",
        ));
    };
    let array = reference
        .as_array(&*store)?
        .ok_or_else(|| wasmtime::Error::msg("foreign transfer value is not an array"))?;
    let length = usize::try_from(array.len(&*store)?)?;
    if units {
        let mut data = Vec::new();
        data.try_reserve_exact(length)?;
        for i in 0..u32::try_from(length)? {
            let Val::I32(unit) = array.get(&mut *store, i)? else {
                return Err(wasmtime::Error::msg(
                    "foreign UTF-16 array has wrong storage",
                ));
            };
            data.push(u16::try_from(unit)?);
        }
        Ok(ForeignValue::StringUtf16(data))
    } else {
        let mut data = Vec::new();
        data.try_reserve_exact(length)?;
        data.resize(length, 0);
        array.copy_to_i8_slice(&mut *store, &mut data)?;
        Ok(ForeignValue::Bytes(data))
    }
}

pub(super) fn allocate_array(
    store: &mut impl AsContextMut<Data = WasmStoreState>,
    ty: ArrayType,
    value: &ForeignValue,
) -> wasmtime::Result<Val> {
    let allocator = ArrayRefPre::new(&mut *store, ty);
    let array = match value {
        ForeignValue::StringUtf16(units) => {
            let length = u32::try_from(units.len())?;
            let array = ArrayRef::new(&mut *store, &allocator, &Val::I32(0), length)?;
            for (i, unit) in units.iter().enumerate() {
                array.set(&mut *store, u32::try_from(i)?, Val::I32(i32::from(*unit)))?;
            }
            array
        }
        ForeignValue::Bytes(bytes) => {
            u32::try_from(bytes.len())?;
            ArrayRef::new_from_i8_slice(&mut *store, &allocator, bytes)?
        }
        ForeignValue::Number(_) | ForeignValue::Void => {
            return Err(wasmtime::Error::msg("foreign scalar is not an array"));
        }
    };
    Ok(Val::AnyRef(Some(array.to_anyref())))
}

fn arguments(
    caller: &mut Caller<'_, WasmStoreState>,
    types: &[ForeignValueType],
    inputs: &[Val],
) -> wasmtime::Result<Vec<ForeignValue>> {
    let mut values = Vec::new();
    values.try_reserve_exact(types.len())?;
    for (ty, value) in types.iter().zip(inputs) {
        values.push(match ty {
            ForeignValueType::Number => {
                let Val::F64(bits) = value else {
                    return Err(wasmtime::Error::msg("foreign number has wrong storage"));
                };
                ForeignValue::Number(f64::from_bits(*bits))
            }
            ForeignValueType::StringUtf16 => copy_array(caller, value, true)?,
            ForeignValueType::Bytes => copy_array(caller, value, false)?,
        });
    }
    Ok(values)
}

impl EmbeddedRuntime {
    fn foreign_signature(
        &self,
        module: &str,
        name: &str,
        mode: ForeignFunctionMode,
    ) -> Result<(ForeignFunctionDeclaration, FuncType), EngineError> {
        let declaration = self
            .foreign
            .modules()
            .iter()
            .flat_map(|m| m.functions())
            .find(|f| f.import_module() == module && f.import_name() == name)
            .ok_or_else(|| EngineError::new("foreign function was not declared by this program"))?;
        if declaration.mode() != mode {
            return Err(EngineError::new(
                "foreign function registration mode differs from its declaration",
            ));
        }
        let ty = self
            .modules
            .program
            .imports()
            .find(|i| i.module() == module && i.name() == name)
            .ok_or_else(|| {
                EngineError::new("declared foreign function is not an actual program requirement")
            })?
            .ty();
        let ExternType::Func(ty) = ty else {
            return Err(EngineError::new("foreign import is not a function"));
        };
        Ok((declaration.clone(), ty))
    }

    /// Register an owned-value callback with this program's canonical Wasm type.
    /// The callback runs synchronously in its caller's Store and may access its
    /// application data. Argument and result arrays are independent copies.
    pub fn register_foreign<F>(
        &self,
        linker: &mut WasmtimeLinker<WasmStoreState>,
        module: &str,
        name: &str,
        callback: F,
    ) -> Result<(), EngineError>
    where
        F: Fn(&mut Caller<'_, WasmStoreState>, Vec<ForeignValue>) -> wasmtime::Result<ForeignValue>
            + Send
            + Sync
            + 'static,
    {
        if !WasmtimeEngine::same(linker.engine(), &self.engine) {
            return Err(EngineError::new(
                "foreign Linker belongs to a different Engine",
            ));
        }
        let (declaration, signature) =
            self.foreign_signature(module, name, ForeignFunctionMode::Synchronous)?;
        let output_type = signature.results().next().and_then(|ty| {
            ty.as_ref()
                .and_then(|r| r.heap_type().as_concrete_array())
                .cloned()
        });
        let authority = Arc::clone(&self.authority);
        linker
            .func_new(
                module,
                name,
                signature,
                move |mut caller, inputs, outputs| {
                    check_foreign_caller(&caller, &authority)?;
                    let values = arguments(&mut caller, declaration.parameters(), inputs)?;
                    let value = callback(&mut caller, values)?;
                    if !value.matches(declaration.result()) {
                        return Err(wasmtime::Error::msg(
                            "foreign callback returned a value outside its declared result type",
                        ));
                    }
                    match value {
                        ForeignValue::Number(number) => outputs[0] = Val::F64(number.to_bits()),
                        ForeignValue::StringUtf16(_) | ForeignValue::Bytes(_) => {
                            outputs[0] = allocate_array(
                                &mut caller,
                                output_type.clone().ok_or_else(|| {
                                    wasmtime::Error::msg(
                                        "foreign result has no canonical array type",
                                    )
                                })?,
                                &value,
                            )?
                        }
                        ForeignValue::Void => {}
                    }
                    Ok(())
                },
            )
            .map_err(|e| EngineError::new(format!("foreign registration failed: {e:#}")))?;
        Ok(())
    }

    /// Start host work without borrowing the Store across an await. Completing
    /// or dropping the single-use handle queues a settlement for `pump`.
    pub fn register_async_foreign<F>(
        &self,
        linker: &mut WasmtimeLinker<WasmStoreState>,
        module: &str,
        name: &str,
        callback: F,
    ) -> Result<(), EngineError>
    where
        F: Fn(
                &mut Caller<'_, WasmStoreState>,
                Vec<ForeignValue>,
                ForeignCompleter,
            ) -> wasmtime::Result<()>
            + Send
            + Sync
            + 'static,
    {
        if !WasmtimeEngine::same(linker.engine(), &self.engine) {
            return Err(EngineError::new(
                "foreign Linker belongs to a different Engine",
            ));
        }
        let (declaration, signature) =
            self.foreign_signature(module, name, ForeignFunctionMode::Asynchronous)?;
        let authority = Arc::clone(&self.authority);
        linker
            .func_new(
                module,
                name,
                signature,
                move |mut caller, inputs, _outputs| {
                    check_foreign_caller(&caller, &authority)?;
                    let values = arguments(&mut caller, declaration.parameters(), inputs)?;
                    let n = declaration.parameters().len();
                    let resolve = resolver(&mut caller, &inputs[n])?;
                    let reject = resolver(&mut caller, &inputs[n + 1])?;
                    let completer = caller
                        .data_mut()
                        .embedding
                        .as_mut()
                        .unwrap()
                        .async_host
                        .register(resolve, reject, declaration.result())
                        .map_err(wasmtime::Error::new)?;
                    callback(&mut caller, values, completer)
                },
            )
            .map_err(|e| EngineError::new(format!("async foreign registration failed: {e:#}")))?;
        Ok(())
    }
}

fn resolver(
    caller: &mut Caller<'_, WasmStoreState>,
    value: &Val,
) -> wasmtime::Result<wasmtime::OwnedRooted<wasmtime::EqRef>> {
    let Val::AnyRef(Some(reference)) = value else {
        return Err(wasmtime::Error::msg("async foreign resolver is absent"));
    };
    reference
        .as_eqref(&mut *caller)?
        .ok_or_else(|| wasmtime::Error::msg("async foreign resolver is not an eqref"))?
        .to_owned_rooted(caller)
}

fn check_foreign_caller(
    caller: &Caller<'_, WasmStoreState>,
    authority: &Arc<()>,
) -> wasmtime::Result<()> {
    let state = caller
        .data()
        .embedding
        .as_ref()
        .ok_or_else(|| wasmtime::Error::msg("foreign callback has no embedding authority"))?;
    if !Arc::ptr_eq(&state.module, authority) {
        return Err(wasmtime::Error::msg(
            "foreign callback belongs to a different admitted program",
        ));
    }
    Ok(())
}
