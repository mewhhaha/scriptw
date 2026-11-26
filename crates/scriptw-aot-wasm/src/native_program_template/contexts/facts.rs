use super::*;
use std::collections::BTreeSet;

/// Header facts collected by the same pass that constructs the native plan.
#[derive(Default)]
pub(in crate::native_program_template) struct Facts {
    pub(super) signatures: signatures::Signatures,
    payload_fields: BTreeMap<u32, wasmparser::FieldType>,
    immutable_bytes: BTreeSet<u32>,
}
impl Facts {
    pub(in crate::native_program_template) fn record_type(
        &mut self,
        index: u32,
        ty: &wasmparser::SubType,
    ) {
        self.signatures.record_type(ty);
        match &ty.composite_type.inner {
            CompositeInnerType::Struct(ty) => {
                let field = crate::gc_types::ProgramInstanceLocals::native_payload_field();
                if let Some(field) = ty.fields.get(field as usize) {
                    self.payload_fields.insert(index, *field);
                }
            }
            CompositeInnerType::Array(ty)
                if !ty.0.mutable && matches!(ty.0.element_type, StorageType::I8) =>
            {
                self.immutable_bytes.insert(index);
            }
            _ => {}
        }
    }
    pub(in crate::native_program_template) fn import_function(&mut self, ty: u32) {
        self.signatures.import_function(ty);
    }
    pub(in crate::native_program_template) fn define_function(&mut self, ty: u32) {
        self.signatures.define_function(ty);
    }
    pub(in crate::native_program_template) fn exports(
        &mut self,
        reader: wasmparser::ExportSectionReader<'_>,
    ) -> Result<(), NativeProgramTemplateError> {
        for export in reader {
            self.signatures.record_export(export.map_err(invalid)?)?;
        }
        Ok(())
    }
    pub(super) fn matches(&self, schema: Schema) -> bool {
        self.payload_fields
            .get(&schema.program_type)
            .is_some_and(|field| {
                !field.mutable
                    && matches!(field.element_type,
                    StorageType::Val(ValType::Ref(reference)) if reference.is_nullable()
                    && matches!(reference.heap_type(), wasmparser::HeapType::Concrete(index)
                        if index.as_module_index() == Some(schema.payload_type)))
            })
            && self.immutable_bytes.contains(&schema.payload_type)
    }
}
