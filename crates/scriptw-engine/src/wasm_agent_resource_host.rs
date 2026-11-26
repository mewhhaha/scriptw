//! Each agent message owns primitive ID data and a native shared byte resource.

use super::wasm_gc_host::TypedGcHostImport;
use super::*;
use scriptw_aot_wasm::{GcHostImport, GcHostLayout};
use wasmtime::{ArrayRef, ArrayRefPre, ArrayType, ExternRef, FuncType, Mutability, Val, ValType};

struct AgentResourceSignature {
    function: FuncType,
    bytes: ArrayType,
}

impl AgentResourceSignature {
    fn validate(function: FuncType, receive: bool) -> wasmtime::Result<Self> {
        let (resource, bytes) = {
            let mut params = function.params();
            let mut results = function.results();
            if receive {
                let (None, Some(resource), Some(ValType::I64), Some(bytes), None) = (
                    params.next(),
                    results.next(),
                    results.next(),
                    results.next(),
                    results.next(),
                ) else {
                    return Err(wasmtime::Error::msg(
                        "agent receive requires resource, Int32 carrier and BigInt bytes",
                    ));
                };
                (resource, bytes)
            } else {
                let (
                    Some(resource),
                    Some(ValType::I64),
                    Some(bytes),
                    None,
                    Some(ValType::I64),
                    None,
                ) = (
                    params.next(),
                    params.next(),
                    params.next(),
                    params.next(),
                    results.next(),
                    results.next(),
                )
                else {
                    return Err(wasmtime::Error::msg(
                        "agent broadcast requires resource, Int32 carrier and BigInt bytes",
                    ));
                };
                (resource, bytes)
            }
        };
        let resource = resource
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("agent resource must be an externref"))?;
        if resource.is_nullable() != receive
            || !matches!(resource.heap_type(), wasmtime::HeapType::Extern)
        {
            return Err(wasmtime::Error::msg(
                "agent resource has invalid heap type or nullability",
            ));
        }
        let bytes = bytes
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("agent BigInt bytes must be a reference"))?;
        if !bytes.is_nullable() {
            return Err(wasmtime::Error::msg(
                "agent BigInt bytes must admit the Int32 branch",
            ));
        }
        let array = bytes
            .heap_type()
            .as_concrete_array()
            .ok_or_else(|| wasmtime::Error::msg("agent BigInt bytes must be a concrete array"))?;
        let layout = GcHostLayout::ByteArray;
        if layout.array_mutable() != Some(array.mutability() == Mutability::Var)
            || !wasm_gc_completion::storage_matches(
                layout.array_element().expect("byte schema"),
                &array.element_type(),
            )
        {
            return Err(wasmtime::Error::msg(
                "agent bytes disagree with the emitted GC byte schema",
            ));
        }
        let bytes = array.clone();
        Ok(Self { function, bytes })
    }
}

fn signature(
    import: TypedGcHostImport<'_>,
    receive: bool,
) -> Result<Option<AgentResourceSignature>, EngineError> {
    let Some(function) = import.into_func(
        "duplicate agent resource import",
        "agent resource import is not a function",
    )?
    else {
        return Ok(None);
    };
    AgentResourceSignature::validate(function, receive)
        .map(Some)
        .map_err(|error| EngineError::new(format!("invalid agent resource import: {error}")))
}

fn decimal_id(bytes: Vec<u8>) -> wasmtime::Result<WasmAgentMessageId> {
    let decimal = String::from_utf8(bytes)
        .map_err(|error| wasmtime::Error::msg(format!("agent BigInt ID is not UTF-8: {error}")))?;
    ObservedBigInt::parse_canonical_decimal(decimal.into_boxed_str())
        .map(WasmAgentMessageId::BigInt)
        .map_err(wasmtime::Error::new)
}

pub(super) fn link(
    linker: &mut WasmtimeLinker<WasmHostState>,
    broadcast_import: TypedGcHostImport<'_>,
    receive_import: TypedGcHostImport<'_>,
) -> Result<(), EngineError> {
    let row = GcHostImport::AgentBroadcastResource;
    if let Some(signature) = signature(broadcast_import, false)? {
        linker
            .func_new(
                row.module(),
                row.name(),
                signature.function,
                |mut caller: WasmtimeCaller<'_, WasmHostState>,
                 inputs: &[Val],
                 outputs: &mut [Val]| {
                    let (
                        [Val::ExternRef(Some(reference)), Val::I64(integer), Val::AnyRef(bytes)],
                        [output],
                    ) = (inputs, outputs)
                    else {
                        return Err(wasmtime::Error::msg(
                            "agent broadcast callback has invalid operands",
                        ));
                    };
                    let resource = wasm_gc_host::resource(&caller, *reference)?;
                    let id = match bytes {
                        None => {
                            WasmAgentMessageId::Int32(i32::try_from(*integer).map_err(|_| {
                                wasmtime::Error::msg("agent Int32 ID exceeds its domain")
                            })?)
                        }
                        Some(bytes) => {
                            if *integer != 0 {
                                return Err(wasmtime::Error::msg(
                                    "agent BigInt ID has a stale Int32 carrier",
                                ));
                            }
                            let array = bytes.as_array(&caller)?.ok_or_else(|| {
                                wasmtime::Error::msg("agent BigInt ID is not an array")
                            })?;
                            let length = usize::try_from(array.len(&caller)?).map_err(|_| {
                                wasmtime::Error::msg("agent ID exceeds host extent")
                            })?;
                            let mut bytes = Vec::new();
                            bytes
                                .try_reserve_exact(length)
                                .map_err(wasmtime::Error::new)?;
                            bytes.resize(length, 0);
                            array.copy_to_i8_slice(&mut caller, &mut bytes)?;
                            decimal_id(bytes)?
                        }
                    };
                    let group = caller.data().agent_group.as_ref().ok_or_else(|| {
                        wasmtime::Error::msg("agent.broadcast requires an active agent group")
                    })?;
                    let sent = group
                        .broadcast(WasmAgentBroadcast { resource, id })
                        .map_err(wasmtime::Error::new)?;
                    *output = Val::I64(
                        i64::try_from(sent)
                            .map_err(|_| wasmtime::Error::msg("agent count exceeds ABI"))?,
                    );
                    Ok(())
                },
            )
            .map_err(|error| {
                EngineError::new(format!("agent broadcast linker setup failed: {error}"))
            })?;
    }
    let row = GcHostImport::AgentReceiveResource;
    if let Some(signature) = signature(receive_import, true)? {
        linker
            .func_new(
                row.module(),
                row.name(),
                signature.function,
                move |mut caller: WasmtimeCaller<'_, WasmHostState>,
                      inputs: &[Val],
                      outputs: &mut [Val]| {
                    let ([], [resource, integer, bytes]) = (inputs, outputs) else {
                        return Err(wasmtime::Error::msg(
                            "agent receive callback has invalid operands",
                        ));
                    };
                    let commands = caller.data().agent_commands.clone().ok_or_else(|| {
                        wasmtime::Error::msg("agent.receiveBroadcast may only run inside an agent")
                    })?;
                    let group = caller.data().agent_group.as_ref().ok_or_else(|| {
                        wasmtime::Error::msg(
                            "agent.receiveBroadcast requires an active agent group",
                        )
                    })?;
                    let receiver = commands.lock().unwrap_or_else(|error| error.into_inner());
                    let command = group
                        .execution_control
                        .receive(&receiver)
                        .map_err(|error| match error {
                            AgentReceiveError::Disconnected => {
                                wasmtime::Error::msg("agent broadcast channel closed")
                            }
                            AgentReceiveError::Execution(error) => wasmtime::Error::new(error),
                        })?;
                    drop(receiver);
                    match command {
                        WasmAgentCommand::Shutdown => {
                            *resource = Val::ExternRef(None);
                            *integer = Val::I64(0);
                            *bytes = Val::AnyRef(None);
                        }
                        WasmAgentCommand::Broadcast {
                            broadcast,
                            retrieved,
                        } => {
                            let backing =
                                caller
                                    .data()
                                    .shared_memory_backing
                                    .as_ref()
                                    .ok_or_else(|| {
                                        wasmtime::Error::msg(
                                            "agent resource requires imported shared memory",
                                        )
                                    })?;
                            backing.check_resource(&broadcast.resource)?;
                            // Both roots belong to the recipient's canonical GC types.
                            let backing = ExternRef::new(&mut caller, broadcast.resource)?;
                            let (id, decimal) = match broadcast.id {
                                WasmAgentMessageId::Int32(id) => (i64::from(id), None),
                                WasmAgentMessageId::BigInt(id) => {
                                    let allocator =
                                        ArrayRefPre::new(&mut caller, signature.bytes.clone());
                                    let bytes = ArrayRef::new_from_i8_slice(
                                        &mut caller,
                                        &allocator,
                                        id.as_str().as_bytes(),
                                    )?;
                                    (0, Some(bytes.to_anyref()))
                                }
                            };
                            *resource = Val::ExternRef(Some(backing));
                            *integer = Val::I64(id);
                            *bytes = Val::AnyRef(decimal);
                            let _ = retrieved.send(());
                        }
                    }
                    Ok(())
                },
            )
            .map_err(|error| {
                EngineError::new(format!("agent receive linker setup failed: {error}"))
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmtime::{FieldType, HeapType, RefType, StorageType};

    #[test]
    fn agent_bigint_ids_require_canonical_exact_decimal() {
        for decimal in [
            "0",
            "-1",
            "18446744073709551616",
            "-340282366920938463463374607431768211457",
        ] {
            let WasmAgentMessageId::BigInt(id) = decimal_id(decimal.as_bytes().to_vec()).unwrap()
            else {
                panic!("BigInt branch");
            };
            assert_eq!(id.as_str(), decimal);
        }
        for invalid in [
            b"".as_slice(),
            b"-0",
            b"01",
            b"+1",
            b"1.0",
            b" 1",
            b"1n",
            &[0xff],
        ] {
            assert!(decimal_id(invalid.to_vec()).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn agent_resource_admission_preserves_canonical_bytes_and_rejects_lookalikes() {
        let engine = shared_wasm_engine().expect("required GC engine");
        let byte_array = ArrayType::new(&engine, FieldType::new(Mutability::Var, StorageType::I8));
        let bytes = ValType::Ref(RefType::new(
            true,
            HeapType::ConcreteArray(byte_array.clone()),
        ));
        for receive in [false, true] {
            let resource = ValType::Ref(RefType::new(receive, HeapType::Extern));
            let operands = vec![resource.clone(), ValType::I64, bytes.clone()];
            let make = |operands: Vec<ValType>| {
                if receive {
                    FuncType::new(&engine, [], operands)
                } else {
                    FuncType::new(&engine, operands, [ValType::I64])
                }
            };
            let accepted = AgentResourceSignature::validate(make(operands.clone()), receive)
                .expect("exact schema");
            assert!(accepted.bytes.matches(&byte_array) && byte_array.matches(&accepted.bytes));
            for malformed in [
                vec![],
                vec![resource.clone(), ValType::I32, bytes.clone()],
                vec![
                    ValType::Ref(RefType::new(!receive, HeapType::Extern)),
                    ValType::I64,
                    bytes.clone(),
                ],
                vec![
                    ValType::Ref(RefType::new(receive, HeapType::Any)),
                    ValType::I64,
                    bytes.clone(),
                ],
                vec![resource.clone(), ValType::I64, ValType::I64],
                vec![
                    resource.clone(),
                    ValType::I64,
                    ValType::Ref(RefType::new(
                        false,
                        HeapType::ConcreteArray(byte_array.clone()),
                    )),
                ],
            ] {
                assert!(AgentResourceSignature::validate(make(malformed), receive).is_err());
            }
            for (mutable, storage) in [
                (Mutability::Var, StorageType::I16),
                (Mutability::Const, StorageType::I8),
            ] {
                let wrong = ArrayType::new(&engine, FieldType::new(mutable, storage));
                assert!(AgentResourceSignature::validate(
                    make(vec![
                        resource.clone(),
                        ValType::I64,
                        ValType::Ref(RefType::new(true, HeapType::ConcreteArray(wrong)))
                    ]),
                    receive
                )
                .is_err());
            }
        }
    }
}
