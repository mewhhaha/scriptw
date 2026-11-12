use super::*;
use scriptw_aot_wasm::{GcHostField, GcHostLayout};
use wasmtime::{
    ArrayType, FieldType, FuncType, HeapType, Mutability, StorageType, StructType, ValType,
};

pub(super) struct NativeFunctionSignature {
    pub(super) function: FuncType,
    pub(super) arguments: Vec<ValType>,
    pub(super) completion: Vec<ValType>,
    pub(super) sources: ArrayType,
    pub(super) stored: StructType,
    pub(super) program: StructType,
    pub(super) string: StructType,
    pub(super) units: ArrayType,
}

impl NativeFunctionSignature {
    pub(super) fn validate(function: FuncType) -> wasmtime::Result<Self> {
        let arguments: Vec<_> = function.params().collect();
        let results: Vec<_> = function.results().collect();
        let (
            [ValType::I32, ValType::I32, ValType::I64, target, sources, context, environment],
            [ValType::I32, ValType::I64, value, ValType::I32, ValType::I32, message],
        ) = (arguments.as_slice(), results.as_slice())
        else {
            return Err(abi_error(
                "expected seven operands and Completion plus syntax message",
            ));
        };
        require_eq_reference(target)?;
        require_eq_reference(value)?;
        let sources = array_reference(sources, false, GcHostLayout::ValueArray)?;
        let stored = match sources.element_type() {
            StorageType::ValType(value) => {
                struct_reference(&value, false, GcHostLayout::StoredValue)?
            }
            _ => return Err(abi_error("source entries are not StoredValue references")),
        };
        for field in [
            GcHostField::StoredTag,
            GcHostField::StoredScalar,
            GcHostField::StoredReference,
        ] {
            checked_field(&stored, field)?;
        }
        let context = struct_reference(context, false, GcHostLayout::FunctionContext)?;
        let field = checked_field(&context, GcHostField::ContextDefiningProgram)?;
        let program = match field.element_type() {
            StorageType::ValType(value) => {
                struct_reference(value, true, GcHostLayout::ProgramInstance)?
            }
            _ => return Err(abi_error("constructor context has no typed program owner")),
        };
        struct_reference(environment, true, GcHostLayout::Environment)?;
        let string = struct_reference(message, true, GcHostLayout::String)?;
        checked_field(&string, GcHostField::StringRawKeyHash)?;
        checked_field(&string, GcHostField::StringEntryHint)?;
        let field = checked_field(&string, GcHostField::StringCodeUnits)?;
        let units = match field.element_type() {
            StorageType::ValType(value) => array_reference(value, false, GcHostLayout::CodeUnits)?,
            _ => return Err(abi_error("syntax message lacks canonical code units")),
        };
        let completion = results[..5].to_vec();
        Ok(Self {
            function,
            arguments,
            completion,
            sources,
            stored,
            program,
            string,
            units,
        })
    }
}

pub(super) fn require_eq_reference(value: &ValType) -> wasmtime::Result<()> {
    if !matches!(value, ValType::Ref(reference) if reference.is_nullable() && matches!(reference.heap_type(), HeapType::Eq))
    {
        return Err(abi_error("value payload must be an actual nullable eqref"));
    }
    Ok(())
}

pub(super) fn checked_field(ty: &StructType, field: GcHostField) -> wasmtime::Result<FieldType> {
    let definition = field.definition();
    if definition.layout().field_count() != Some(ty.fields().len() as u32) {
        return Err(abi_error(
            "struct disagrees with its declaration-owned field count",
        ));
    }
    let actual = ty
        .field(definition.index() as usize)
        .ok_or_else(|| abi_error("missing schema field"))?;
    if (actual.mutability() == Mutability::Var) != definition.mutable()
        || !wasm_gc_completion::storage_matches(definition.storage(), actual.element_type())
    {
        return Err(abi_error(
            "struct field storage or mutability disagrees with schema",
        ));
    }
    Ok(actual)
}

fn struct_reference(
    value: &ValType,
    nullable: bool,
    layout: GcHostLayout,
) -> wasmtime::Result<StructType> {
    let ValType::Ref(reference) = value else {
        return Err(abi_error("expected a concrete struct reference"));
    };
    if reference.is_nullable() != nullable {
        return Err(abi_error("wrong struct reference nullability"));
    }
    let ty = reference
        .heap_type()
        .as_concrete_struct()
        .ok_or_else(|| abi_error("reference is not a concrete struct"))?;
    if layout.field_count() != Some(ty.fields().len() as u32) {
        return Err(abi_error(
            "reference struct has the wrong declared field count",
        ));
    }
    Ok(ty.clone())
}

fn array_reference(
    value: &ValType,
    nullable: bool,
    layout: GcHostLayout,
) -> wasmtime::Result<ArrayType> {
    let ValType::Ref(reference) = value else {
        return Err(abi_error("expected a concrete array reference"));
    };
    if reference.is_nullable() != nullable {
        return Err(abi_error("wrong array reference nullability"));
    }
    let ty = reference
        .heap_type()
        .as_concrete_array()
        .ok_or_else(|| abi_error("reference is not a concrete array"))?;
    let element = layout
        .array_element()
        .ok_or_else(|| abi_error("schema is not an array"))?;
    if layout.array_mutable() != Some(ty.mutability() == Mutability::Var)
        || !wasm_gc_completion::storage_matches(element, &ty.element_type())
    {
        return Err(abi_error(
            "array storage or mutability disagrees with schema",
        ));
    }
    Ok(ty.clone())
}
