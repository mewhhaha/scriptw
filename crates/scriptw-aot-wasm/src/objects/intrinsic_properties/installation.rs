use crate::code_sink::Function;
#[cfg(test)]
use crate::code_sink::LocalDeclarations;
use crate::data::RPropertyLayoutSpan;
#[cfg(test)]
use crate::gc_types::{
    DeferredIntrinsicValue, GcHeapType, GcNullability, PropertyDescriptor, PropertyEntry,
    RuntimeModuleTypes,
};
use crate::gc_types::{
    GcLocal, GcOperand, IntrinsicPropertyStorage, OrdinaryObject, OrdinaryObjectSchema,
    RealmRecord, RuntimeSchema,
};
use crate::EmitError;
#[cfg(test)]
use wasm_encoder::{Encode, Instruction, ValType};
#[cfg(test)]
use wasmparser::{FunctionBody as ParsedFunctionBody, Operator};

/// Install one sealed layout by replacing only the header's storage reference.
/// Row recipes stay in R until observation; no per-property object is built here.
pub(crate) fn emit_intrinsic_layout_storage(
    span: RPropertyLayoutSpan,
    object: &GcLocal<OrdinaryObject>,
    realm: &GcLocal<RealmRecord>,
    schema: &RuntimeSchema,
    function: &mut Function,
) -> Result<(), EmitError> {
    let storage = schema.reserve_gc_local(function).initialize(
        schema.struct_type::<IntrinsicPropertyStorage>().construct(
            (
                GcOperand::reference(realm, schema),
                GcOperand::i32(span.address() as i32),
                GcOperand::i32(span.count() as i32),
                GcOperand::i32(span.group_count() as i32),
                GcOperand::i32(span.index_address() as i32),
                GcOperand::i32(span.index_capacity().saturating_sub(1) as i32),
                GcOperand::null(schema),
            ),
            function,
        ),
        function,
    );
    schema.field(OrdinaryObjectSchema::PROPERTIES).write(
        object,
        GcOperand::subtype_reference(&storage, schema),
        schema,
        function,
    );
    storage.clear(function);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::StringPool;
    use crate::intrinsics::IntrinsicPropertyLayoutId;
    use scriptw_front::{parse, ParseOptions};
    use std::collections::BTreeMap;

    fn type_index<T: GcHeapType>(schema: &RuntimeSchema) -> u32 {
        let wasm_encoder::HeapType::Concrete(index) = schema
            .reference_type::<T>(GcNullability::NonNullable)
            .heap_type
        else {
            panic!("registered GC type has a concrete index");
        };
        index
    }

    #[test]
    fn sealed_layout_install_fragment_allocates_only_one_storage_record() {
        let script = scriptw_ir::lower(&parse(";", ParseOptions::script()).unwrap())
            .script
            .unwrap();
        let intl =
            scriptw_intl::IntlDataSelection::new(scriptw_intl::IntlCompilationProfile::Minimal);
        let strings = StringPool::collect(&script, &BTreeMap::new(), &[], false, &intl).unwrap();
        let span = strings
            .intrinsic_property_layout_span(IntrinsicPropertyLayoutId::ArrayUnscopables)
            .unwrap();
        let registered =
            RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
                crate::RuntimeSourceCompilationProfile::Portable,
            ))
            .finalize_globals(crate::gc_types::GlobalLedger::new(), false, 0);
        let schema = registered.globals().schema();
        let mut function = Function::with_parameters(
            LocalDeclarations::from_types([]),
            vec![
                ValType::Ref(schema.reference_type::<OrdinaryObject>(GcNullability::NonNullable)),
                ValType::Ref(schema.reference_type::<RealmRecord>(GcNullability::NonNullable)),
            ],
        );
        let object =
            schema.parameter_gc::<OrdinaryObject, crate::gc_types::NonNullable>(0, &mut function);
        let realm =
            schema.parameter_gc::<RealmRecord, crate::gc_types::NonNullable>(1, &mut function);
        emit_intrinsic_layout_storage(span, &object, &realm, schema, &mut function).unwrap();
        object.clear(&mut function);
        realm.clear(&mut function);
        function.instruction(&Instruction::End);

        let mut body_bytes = Vec::new();
        function.into_body().encode(&mut body_bytes);
        let mut reader = wasmparser::BinaryReader::new(&body_bytes, 0);
        let length = reader.read_var_u32().unwrap() as usize;
        let bytes = reader.read_bytes(length).unwrap();
        let parsed = ParsedFunctionBody::new(wasmparser::BinaryReader::new(bytes, 0));
        let operators = parsed
            .get_operators_reader()
            .unwrap()
            .into_iter()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        let allocations = |ty| {
            operators
                .iter()
                .filter(|operator| matches!(operator, Operator::StructNew { struct_type_index } if *struct_type_index == ty))
                .count()
        };
        assert_eq!(
            allocations(type_index::<IntrinsicPropertyStorage>(schema)),
            1,
            "one complete layout installs one storage record"
        );
        for (name, count) in [
            (
                "PropertyEntry",
                allocations(type_index::<PropertyEntry>(schema)),
            ),
            (
                "PropertyDescriptor",
                allocations(type_index::<PropertyDescriptor>(schema)),
            ),
            (
                "DeferredIntrinsicValue",
                allocations(type_index::<DeferredIntrinsicValue>(schema)),
            ),
        ] {
            assert_eq!(count, 0, "layout installation allocates no {name}");
        }
        assert!(
            !operators.iter().any(|operator| matches!(
                operator,
                Operator::Call { .. } | Operator::CallRef { .. } | Operator::CallIndirect { .. }
            )),
            "installation stores the typed layout without calling a property installer"
        );
    }
}
