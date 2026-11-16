use super::*;
use plan::Plan;
use std::borrow::Cow;
use wasm_encoder::{
    reencode::{self, Reencode},
    EntityType, GlobalType, HeapType, Instruction, RefType, ValType,
};
mod declarations;
mod names;
mod payload;
struct Encoder<'a> {
    plan: &'a Plan,
    body_index: u32,
}
impl Encoder<'_> {
    fn reference(index: u32) -> ValType {
        ValType::Ref(RefType {
            nullable: false,
            heap_type: HeapType::Concrete(index),
        })
    }
}
impl Reencode for Encoder<'_> {
    type Error = String;
    fn function_index(&mut self, index: u32) -> Result<u32, reencode::Error<String>> {
        self.plan.function(index).ok_or_else(|| {
            reencode::Error::UserError("removed function is still referenced".into())
        })
    }
    fn global_index(&mut self, index: u32) -> Result<u32, reencode::Error<String>> {
        Ok(
            if self.plan.contexts.is_some() || index < self.plan.imported_globals {
                index
            } else {
                index + self.plan.images.len() as u32 + self.plan.literal_operands.len() as u32
            },
        )
    }
    fn data_count(&mut self, _: u32) -> Result<u32, reencode::Error<String>> {
        Ok(0)
    }
    fn parse_data_section(
        &mut self,
        _: &mut wasm_encoder::DataSection,
        _: wasmparser::DataSectionReader<'_>,
    ) -> Result<(), reencode::Error<String>> {
        Ok(())
    }
    fn parse_custom_section(
        &mut self,
        module: &mut wasm_encoder::Module,
        section: wasmparser::CustomSectionReader<'_>,
    ) -> Result<(), reencode::Error<String>> {
        // The template's bodies are re-encoded with other operators, so the
        // hints, which name byte offsets of the original bodies, do not
        // describe them. They are advice on code placement only.
        if section.name() == crate::program_literal_operands::SECTION_NAME
            || section.name() == crate::native_program_contexts::SECTION_NAME
            || section.name() == crate::branch_hints::SECTION_NAME
        {
            return Ok(());
        }
        reencode::utils::parse_custom_section(self, module, section)
    }
    fn parse_type_section(
        &mut self,
        types: &mut wasm_encoder::TypeSection,
        reader: wasmparser::TypeSectionReader<'_>,
    ) -> Result<(), reencode::Error<String>> {
        self.type_declarations(types, reader)
    }
    fn parse_function_section(
        &mut self,
        functions: &mut wasm_encoder::FunctionSection,
        reader: wasmparser::FunctionSectionReader<'_>,
    ) -> Result<(), reencode::Error<String>> {
        self.function_declarations(functions, reader)
    }
    fn parse_import_section(
        &mut self,
        imports: &mut wasm_encoder::ImportSection,
        reader: wasmparser::ImportSectionReader<'_>,
    ) -> Result<(), reencode::Error<String>> {
        self.import_declarations(imports, reader)
    }
    fn parse_function_body(
        &mut self,
        code: &mut wasm_encoder::CodeSection,
        body: wasmparser::FunctionBody<'_>,
    ) -> Result<(), reencode::Error<String>> {
        let mut function = self.new_function_with_parsed_locals(&body)?;
        let bootstrap = self
            .plan
            .contexts
            .as_ref()
            .is_some_and(|contexts| contexts.bootstrap_body == self.body_index);
        let mut reader = body.get_operators_reader()?;
        while !reader.eof() {
            let offset = reader.original_position();
            let op = reader.read()?;
            if self.plan.suppressed.contains(&offset) {
                continue;
            }
            if self
                .plan
                .contexts
                .as_ref()
                .is_some_and(|contexts| contexts.payload_site == offset)
            {
                function.instruction(&Instruction::LocalGet(0));
            } else if let Some(index) = self
                .plan
                .literal_sites
                .get(&offset)
                .filter(|_| self.plan.contexts.is_some())
            {
                function.instruction(&Instruction::I32Const(*index as i32));
                self.program_payload(&mut function)?;
                function.instruction(&Instruction::Call(self.plan.literal_helper()));
            } else if let Some(index) = self.plan.literal_sites.get(&offset) {
                function.instruction(&Instruction::GlobalGet(
                    self.plan.imported_globals + self.plan.images.len() as u32 + index,
                ));
            } else if let Some(site) = self.plan.sites.get(&offset) {
                // Slice offset/length are already on the operand stack. Whole
                // construction derives the exact length from this data image.
                if self.plan.contexts.is_some() {
                    function.instruction(&Instruction::I32Const(site.image as i32));
                    self.program_payload(&mut function)?;
                } else {
                    function.instruction(&Instruction::GlobalGet(
                        self.plan.imported_globals + site.image,
                    ));
                }
                function.instruction(&Instruction::Call(
                    self.plan.retained_functions() + site.factory,
                ));
            } else {
                let instruction = match op {
                    wasmparser::Operator::LocalGet { local_index } if bootstrap => {
                        Instruction::LocalGet(local_index + 1)
                    }
                    wasmparser::Operator::LocalSet { local_index } if bootstrap => {
                        Instruction::LocalSet(local_index + 1)
                    }
                    wasmparser::Operator::LocalTee { local_index } if bootstrap => {
                        Instruction::LocalTee(local_index + 1)
                    }
                    other => self.instruction(other)?,
                };
                function.instruction(&instruction);
            }
        }
        code.function(&function);
        self.body_index += 1;
        if self.plan.contexts.is_some() && self.body_index == self.plan.defined_functions {
            crate::gc_types::native_payload::append_literal_helper(
                code,
                self.plan.contexts.as_ref().unwrap().schema,
            );
        }
        Ok(())
    }
    fn parse_element(
        &mut self,
        section: &mut wasm_encoder::ElementSection,
        element: wasmparser::Element<'_>,
    ) -> Result<(), reencode::Error<String>> {
        if matches!(&element.kind, wasmparser::ElementKind::Declared) {
            if let wasmparser::ElementItems::Functions(reader) = &element.items {
                let mut indices = Vec::new();
                for index in (*reader).clone() {
                    if let Some(index) = self.plan.function(index?) {
                        indices.push(index)
                    }
                }
                section.declared(wasm_encoder::Elements::Functions(Cow::Owned(indices)));
                return Ok(());
            }
        }
        reencode::utils::parse_element(self, section, element)
    }
    fn parse_custom_name_subsection(
        &mut self,
        names: &mut wasm_encoder::NameSection,
        name: wasmparser::Name<'_>,
    ) -> Result<(), reencode::Error<String>> {
        names::subsection(self, names, name)
    }
}
pub(super) fn encode(bytes: &[u8], plan: &Plan) -> Result<Vec<u8>, NativeProgramTemplateError> {
    let mut encoder = Encoder {
        plan,
        body_index: 0,
    };
    let mut module = wasm_encoder::Module::new();
    encoder
        .parse_core_module(&mut module, wasmparser::Parser::new(0), bytes)
        .map_err(invalid)?;
    Ok(module.finish())
}
