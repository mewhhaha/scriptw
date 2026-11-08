use super::*;
use scriptw_aot_wasm::GcHostField as Field;
use scriptw_ir::RuntimeDirectEvalDerivedOwner;

pub(super) fn validate(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    invocation: &Rooted<StructRef>,
    operands: &[Val],
    caller: &RuntimeDirectEvalCaller,
    signature: &NativeDirectEvalSignature,
    execution: &WasmAgentExecutionControl,
) -> wasmtime::Result<()> {
    execution.remaining().map_err(wasmtime::Error::new)?;
    for (name, words) in [
        (Field::DirectEvalThisSnapshot, &operands[6..9]),
        (Field::DirectEvalNewTargetSnapshot, &operands[9..12]),
    ] {
        let value = read(store, invocation, name)?;
        let stored = required_structure(store, &value, &signature.stored)?;
        let actual = stored_words(store, &stored)?;
        require_same_value(store, &actual, words)?;
    }
    let derived = read(store, invocation, Field::DirectEvalDerivedBindings)?;
    let derived = nullable_structure(store, &derived, &signature.derived_bindings)?;
    let expected =
        caller.descriptor().derived_owner() == RuntimeDirectEvalDerivedOwner::ExternalCaller;
    if derived.is_some() != expected {
        return Err(abi_error(
            "caller grammar and actual derived binding cells disagree",
        ));
    }
    if let Some(derived) = derived {
        for (name, words) in [
            (Field::DerivedThisCell, &operands[6..9]),
            (Field::DerivedNewTargetCell, &operands[9..12]),
        ] {
            let value = read(store, &derived, name)?;
            let cell = required_structure(store, &value, &signature.binding_cell)?;
            let actual = binding_words(store, &cell)?;
            require_same_value(store, &actual, words)?;
        }
        let status = read(store, &derived, Field::DerivedThisStatusCell)?;
        required_structure(store, &status, &signature.binding_cell)?;
        let active = read(store, &derived, Field::DerivedActiveFunctionCell)?;
        let cell = required_structure(store, &active, &signature.binding_cell)?;
        let [Val::I32(tag), _, reference] = binding_words(store, &cell)? else {
            unreachable!()
        };
        if tag != WasmRuntimeValueTag::Function.tag() {
            return Err(abi_error("derived active function cell is not a Function"));
        }
        let active = required_structure(store, &reference, &signature.function_object)?;
        if !matches!(
            read(store, &active, Field::FunctionObjectDerivedConstructor)?,
            Val::I32(1)
        ) {
            return Err(abi_error(
                "derived active function is not a derived constructor",
            ));
        }
        let context = read(store, &active, Field::FunctionObjectContext)?;
        let context = required_structure(store, &context, &signature.function_context)?;
        let defining = read(store, &context, Field::ContextDefiningProgram)?;
        // This may be an older P1 than the currently calling eval P2.
        required_structure(store, &defining, &signature.program)?;
    }
    execution.remaining().map_err(wasmtime::Error::new)?;
    Ok(())
}

fn read(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    owner: &Rooted<StructRef>,
    name: Field,
) -> wasmtime::Result<Val> {
    owner.field(store, name.definition().index() as usize)
}
fn stored_words(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    value: &Rooted<StructRef>,
) -> wasmtime::Result<[Val; 3]> {
    value_words(
        store,
        value,
        [
            Field::StoredTag,
            Field::StoredScalar,
            Field::StoredReference,
        ],
    )
}
fn binding_words(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    value: &Rooted<StructRef>,
) -> wasmtime::Result<[Val; 3]> {
    value_words(
        store,
        value,
        [
            Field::BindingCellTag,
            Field::BindingCellScalar,
            Field::BindingCellReference,
        ],
    )
}
fn value_words(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    value: &Rooted<StructRef>,
    fields: [Field; 3],
) -> wasmtime::Result<[Val; 3]> {
    let words = [
        read(store, value, fields[0])?,
        read(store, value, fields[1])?,
        read(store, value, fields[2])?,
    ];
    let [Val::I32(tag), Val::I64(scalar), Val::AnyRef(reference)] = words else {
        return Err(abi_error("stored caller value has invalid words"));
    };
    GcHostValue::check(tag, scalar, reference.is_some()).map_err(abi_error)?;
    Ok(words)
}
fn require_same_value(
    store: &impl wasmtime::AsContext<Data = WasmHostState>,
    actual: &[Val; 3],
    expected: &[Val],
) -> wasmtime::Result<()> {
    let ([Val::I32(a), Val::I64(b), Val::AnyRef(c)], [Val::I32(x), Val::I64(y), Val::AnyRef(z)]) =
        (actual, expected)
    else {
        return Err(abi_error("caller snapshot has invalid value operands"));
    };
    let same_reference = match (c, z) {
        (None, None) => true,
        (Some(c), Some(z)) => Rooted::ref_eq(store, c, z)?,
        (Some(_), None) | (None, Some(_)) => false,
    };
    if a != x || b != y || !same_reference {
        return Err(abi_error(
            "caller invocation snapshots differ from actual entry operands",
        ));
    }
    Ok(())
}
