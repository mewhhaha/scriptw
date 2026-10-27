//! Raw operands enter the exact R Wasm factory; JavaScript allocation stays in Wasm.
use super::*;
use scriptw_aot_wasm::{
    GcHostLayout, NATIVE_REGEXP_LITERAL_FACTORY_EXPORT, RUNTIME_IMPORT_NAMESPACE,
};
use wasmtime::{ArrayRef, ArrayRefPre, ArrayType, AsContextMut, ValType};

pub(super) fn execute(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    context: &NativeSourceCompilerContext,
    proof: &scriptw_ir::StaticRegExpRuntimeDirectEval<'_>,
    request: &ConvertedRequest,
    signature: &NativeDirectEvalSignature,
) -> wasmtime::Result<Option<[Val; 6]>> {
    context.check_budget()?;
    let factory = context
        .resolve_import(
            &mut *store,
            RUNTIME_IMPORT_NAMESPACE,
            NATIVE_REGEXP_LITERAL_FACTORY_EXPORT,
        )?
        .into_func()
        .ok_or_else(|| abi_error("exact R has no typed RegExp literal factory"))?;
    let ty = factory.ty(&*store);
    let arguments: Vec<_> = ty.params().collect();
    let results: Vec<_> = ty.results().collect();
    let [source, flags, compiled, environment, realm] = arguments.as_slice() else {
        return Err(abi_error("RegExp literal factory operand count differs"));
    };
    for operand in [source, flags] {
        let array = schema::array(operand, false, GcHostLayout::CodeUnits)?;
        if !ArrayType::eq(&array, &signature.units) {
            return Err(abi_error(
                "RegExp literal UTF16 array is not the caller's core type",
            ));
        }
    }
    let compiled = schema::array(compiled, false, GcHostLayout::ImmutableByteArray)?;
    if !ArrayType::eq(&compiled, &signature.descriptor) {
        return Err(abi_error(
            "RegExp compiled bytes differ from the caller's core type",
        ));
    }
    schema::peer(
        &schema::structure(environment, false, GcHostLayout::Environment)?,
        &signature.environment,
    )?;
    schema::peer(
        &schema::structure(realm, false, GcHostLayout::RealmRecord)?,
        &signature.realm,
    )?;
    if results.len() != 6
        || !results[..5]
            .iter()
            .zip(&signature.completion)
            .all(|(a, b)| ValType::eq(a, b))
        || !matches!(results[5], ValType::I32)
    {
        return Err(abi_error("RegExp factory Completion/status types differ"));
    }
    let units = ArrayRefPre::new(&mut *store, signature.units.clone());
    let source: Vec<_> = proof
        .source_units()
        .into_iter()
        .map(|u| Val::I32(i32::from(u)))
        .collect();
    let flags: Vec<_> = proof
        .flags_units()
        .into_iter()
        .map(|u| Val::I32(i32::from(u)))
        .collect();
    let source = ArrayRef::new_fixed(&mut *store, &units, &source)?;
    let flags = ArrayRef::new_fixed(&mut *store, &units, &flags)?;
    let bytes = ArrayRefPre::new(&mut *store, compiled);
    let compiled = ArrayRef::new_from_i8_slice(&mut *store, &bytes, proof.compiled_bytes())?;
    let operands = [
        Val::AnyRef(Some(source.to_anyref())),
        Val::AnyRef(Some(flags.to_anyref())),
        Val::AnyRef(Some(compiled.to_anyref())),
        request.operands[3],
        request.operands[2],
    ];
    let mut output = [Val::I32(0); 6];
    context.check_budget()?;
    factory.call(&mut *store, &operands, &mut output)?;
    context.check_budget()?;
    match output[5] {
        Val::I32(0) => Ok(None), // No JS work; preserve ordinary preparation.
        Val::I32(2) => Ok(Some(output)),
        _ => Err(abi_error("RegExp factory returned an invalid entry status")),
    }
}
