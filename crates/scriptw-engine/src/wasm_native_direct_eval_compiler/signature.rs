use super::*;
use scriptw_aot_wasm::{GcHostField as Field, GcHostLayout as Layout};
use wasmtime::{ArrayType, FuncType, StorageType, StructType, ValType};

pub(super) struct NativeDirectEvalSignature {
    pub(super) function: FuncType,
    pub(super) arguments: Vec<ValType>,
    pub(super) completion: Vec<ValType>,
    pub(super) program: StructType,
    pub(super) realm: StructType,
    pub(super) string: StructType,
    pub(super) units: ArrayType,
    pub(super) descriptor: ArrayType,
    pub(super) environment: StructType,
    pub(super) private_environment: StructType,
    pub(super) private_names: ArrayType,
    pub(super) invocation: StructType,
    pub(super) derived_bindings: StructType,
    pub(super) stored: StructType,
    pub(super) binding_cell: StructType,
    pub(super) function_object: StructType,
    pub(super) function_context: StructType,
}
impl NativeDirectEvalSignature {
    pub(super) fn validate(function: FuncType) -> wasmtime::Result<Self> {
        let arguments: Vec<_> = function.params().collect();
        let results: Vec<_> = function.results().collect();
        let (
            [source, descriptor, realm, lexical, variable, private, ValType::I32, ValType::I64, this_ref, ValType::I32, ValType::I64, target_ref, invocation, program],
            [ValType::I32, ValType::I64, value, ValType::I32, ValType::I32, ValType::I32, message],
        ) = (arguments.as_slice(), results.as_slice())
        else {
            return Err(abi_error(
                "expected fourteen caller operands and Completion/status/syntax results",
            ));
        };
        for value in [this_ref, target_ref, value] {
            schema::eq_reference(value)?;
        }
        let string = schema::structure(source, false, Layout::String)?;
        schema::peer(&schema::structure(message, true, Layout::String)?, &string)?;
        let units = schema::field_array(&string, Field::StringCodeUnits, false, Layout::CodeUnits)?;
        schema::field(&string, Field::StringRawKeyHash)?;
        let descriptor = schema::array(descriptor, false, Layout::ImmutableByteArray)?;
        let realm = schema::structure(realm, false, Layout::RealmRecord)?;
        let program = schema::structure(program, false, Layout::ProgramInstance)?;
        let environment = schema::structure(lexical, true, Layout::Environment)?;
        schema::peer(
            &schema::structure(variable, true, Layout::Environment)?,
            &environment,
        )?;
        let private_environment = schema::structure(private, true, Layout::PrivateEnvironment)?;
        schema::peer(
            &schema::field_structure(
                &private_environment,
                Field::PrivateEnvironmentParent,
                true,
                Layout::PrivateEnvironment,
            )?,
            &private_environment,
        )?;
        schema::field(&private_environment, Field::PrivateEnvironmentClassScope)?;
        let private_names = schema::field_array(
            &private_environment,
            Field::PrivateEnvironmentNames,
            false,
            Layout::PrivateNameTable,
        )?;
        let StorageType::ValType(element) = private_names.element_type() else {
            return Err(abi_error("private table has no typed name element"));
        };
        schema::structure(&element, false, Layout::PrivateName)?;
        let invocation = schema::structure(invocation, false, Layout::DirectEvalExecutionContext)?;
        let function_context = schema::field_structure(
            &invocation,
            Field::DirectEvalClassContext,
            true,
            Layout::FunctionContext,
        )?;
        schema::field(&function_context, Field::ContextInstanceElementsInitializer)?;
        schema::peer(
            &schema::field_structure(
                &function_context,
                Field::ContextDefiningProgram,
                true,
                Layout::ProgramInstance,
            )?,
            &program,
        )?;
        let function_object = schema::field_structure(
            &function_context,
            Field::ContextActiveFunction,
            true,
            Layout::FunctionObject,
        )?;
        schema::peer(
            &schema::field_structure(
                &function_object,
                Field::FunctionObjectContext,
                false,
                Layout::FunctionContext,
            )?,
            &function_context,
        )?;
        schema::field(&function_object, Field::FunctionObjectDerivedConstructor)?;
        let stored = schema::field_structure(
            &invocation,
            Field::DirectEvalHomeObject,
            false,
            Layout::StoredValue,
        )?;
        for name in [
            Field::StoredTag,
            Field::StoredScalar,
            Field::StoredReference,
        ] {
            schema::field(&stored, name)?;
        }
        for name in [
            Field::DirectEvalThisSnapshot,
            Field::DirectEvalNewTargetSnapshot,
        ] {
            schema::peer(
                &schema::field_structure(&invocation, name, false, Layout::StoredValue)?,
                &stored,
            )?;
        }
        let derived_bindings = schema::field_structure(
            &invocation,
            Field::DirectEvalDerivedBindings,
            true,
            Layout::DirectEvalDerivedBindings,
        )?;
        let binding_cell = schema::field_structure(
            &derived_bindings,
            Field::DerivedThisCell,
            false,
            Layout::BindingCell,
        )?;
        for name in [
            Field::DerivedThisStatusCell,
            Field::DerivedNewTargetCell,
            Field::DerivedActiveFunctionCell,
        ] {
            schema::peer(
                &schema::field_structure(&derived_bindings, name, false, Layout::BindingCell)?,
                &binding_cell,
            )?;
        }
        for name in [
            Field::BindingCellTag,
            Field::BindingCellScalar,
            Field::BindingCellReference,
        ] {
            schema::field(&binding_cell, name)?;
        }
        Ok(Self {
            function,
            arguments,
            completion: results[..5].to_vec(),
            program,
            realm,
            string,
            units,
            descriptor,
            environment,
            private_environment,
            private_names,
            invocation,
            derived_bindings,
            stored,
            binding_cell,
            function_object,
            function_context,
        })
    }
}
