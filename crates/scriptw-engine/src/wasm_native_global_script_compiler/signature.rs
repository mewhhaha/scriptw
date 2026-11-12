use super::*;
use scriptw_aot_wasm::{GcHostField, GcHostLayout};
use wasmtime::{ArrayType, FieldType, FuncType, Mutability, StorageType, StructType, ValType};

pub(super) struct NativeGlobalScriptSignature {
    pub(super) function: FuncType,
    pub(super) arguments: Vec<ValType>,
    pub(super) completion: Vec<ValType>,
    pub(super) program: StructType,
    pub(super) realm: StructType,
    pub(super) string: StructType,
    pub(super) units: ArrayType,
}
impl NativeGlobalScriptSignature {
    pub(super) fn validate(function: FuncType) -> wasmtime::Result<Self> {
        let arguments: Vec<_> = function.params().collect();
        let results: Vec<_> = function.results().collect();
        let (
            [ValType::I32, source, realm, program],
            [ValType::I32, ValType::I64, value, ValType::I32, ValType::I32, ValType::I32, message],
        ) = (arguments.as_slice(), results.as_slice())
        else {
            return Err(abi_error(
                "expected four operands and Completion/status/syntax results",
            ));
        };
        if !matches!(value, ValType::Ref(r) if r.is_nullable() && matches!(r.heap_type(), wasmtime::HeapType::Eq))
        {
            return Err(abi_error(
                "Completion payload is not an actual nullable eqref",
            ));
        }
        let string = struct_reference(source, false, GcHostLayout::String)?;
        let message = struct_reference(message, true, GcHostLayout::String)?;
        if !StructType::eq(&string, &message) {
            return Err(abi_error(
                "source and syntax String types are not canonical peers",
            ));
        }
        let realm = struct_reference(realm, false, GcHostLayout::RealmRecord)?;
        let program = struct_reference(program, false, GcHostLayout::ProgramInstance)?;
        checked_field(&string, GcHostField::StringRawKeyHash)?;
        checked_field(&string, GcHostField::StringEntryHint)?;
        let field = checked_field(&string, GcHostField::StringCodeUnits)?;
        let StorageType::ValType(ValType::Ref(units)) = field.element_type() else {
            return Err(abi_error("source String has no typed code-unit array"));
        };
        if units.is_nullable() {
            return Err(abi_error("String code-unit array is nullable"));
        }
        let units = units
            .heap_type()
            .as_concrete_array()
            .ok_or_else(|| abi_error("String code units are not a concrete GC array"))?
            .clone();
        let layout = GcHostLayout::CodeUnits;
        if layout.array_mutable() != Some(units.mutability() == Mutability::Var)
            || !wasm_gc_completion::storage_matches(
                layout
                    .array_element()
                    .ok_or_else(|| abi_error("missing code-unit schema"))?,
                &units.element_type(),
            )
        {
            return Err(abi_error(
                "String code-unit storage/mutability differs from the declaration",
            ));
        }
        Ok(Self {
            function,
            arguments,
            completion: results[..5].to_vec(),
            program,
            realm,
            string,
            units,
        })
    }
}

fn struct_reference(
    value: &ValType,
    nullable: bool,
    layout: GcHostLayout,
) -> wasmtime::Result<StructType> {
    let ValType::Ref(reference) = value else {
        return Err(abi_error("operand is not a GC reference"));
    };
    if reference.is_nullable() != nullable {
        return Err(abi_error("wrong GC operand nullability"));
    }
    let ty = reference
        .heap_type()
        .as_concrete_struct()
        .ok_or_else(|| abi_error("operand is not a concrete core struct"))?;
    if layout.field_count() != Some(ty.fields().len() as u32) {
        return Err(abi_error(
            "operand differs from its declared core field count",
        ));
    }
    Ok(ty.clone())
}
fn checked_field(ty: &StructType, field: GcHostField) -> wasmtime::Result<FieldType> {
    let definition = field.definition();
    let actual = ty
        .field(definition.index() as usize)
        .ok_or_else(|| abi_error("missing declared field"))?;
    if definition.layout().field_count() != Some(ty.fields().len() as u32)
        || (actual.mutability() == Mutability::Var) != definition.mutable()
        || !wasm_gc_completion::storage_matches(definition.storage(), actual.element_type())
    {
        return Err(abi_error(
            "field storage/mutability differs from the core declaration",
        ));
    }
    Ok(actual)
}
