//! Final producer-P import compaction. R, types, bodies and operators are retained.
//! Structural references outside the code section use the encoder's exhaustive
//! index adapters; names and plain declarative ref.func lists do not keep
//! otherwise unused imports alive. Code bodies are not decoded again: the code
//! sink recorded every function and global index operand as it encoded it
//! ([`crate::code_sink::IndexOperand`]), so those operands are the bodies'
//! references and compaction rewrites exactly them.
use crate::{
    code_sink::{IndexOperand, IndexSpace},
    emitted_function::ModuleFunctionTable,
    module::EmitError,
};
use std::collections::BTreeSet;
use wasm_encoder::{
    reencode::{self, Reencode},
    Module,
};
use wasmparser::{ElementItems, ElementKind, Payload, TypeRef};
mod index_map;
mod names;
use index_map::{FunctionIndex, GlobalIndex, IndexMap};

fn user(message: impl Into<String>) -> reencode::Error<String> {
    reencode::Error::UserError(message.into())
}
fn invalid(message: impl std::fmt::Display) -> EmitError {
    EmitError::unsupported(format!(
        "compiler program import compaction failed: {message}"
    ))
}
fn parser() -> wasmparser::Parser {
    let mut parser = wasmparser::Parser::new(0);
    parser.set_features(crate::runtime_artifact::program_validation_features());
    parser
}
/// These producer sections contain no physical function/global indices. Native
/// context `ready`, payload sites and literal sites are OPERATOR ordinals;
/// body ordinals/types/fields also remain fixed when no body/operator is removed.
fn check_custom(section: &wasmparser::CustomSectionReader<'_>) -> Result<(), String> {
    match section.name() {
        "name" => Ok(()),
        crate::RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION
        | crate::FOREIGN_MODULES_CUSTOM_SECTION
        | crate::profile::PROFILE_SITES_SECTION => Ok(()),
        crate::native_program_contexts::SECTION_NAME => {
            crate::native_program_contexts::read(section.data())
                .map(|_| ())
                .map_err(str::to_owned)
        }
        crate::program_literal_operands::SECTION_NAME => {
            crate::program_literal_operands::read(section.data())
                .map(|_| ())
                .map_err(str::to_owned)
        }
        unknown => Err(format!("unreviewed compiler P custom section {unknown:?}")),
    }
}
#[derive(Default)]
struct References {
    functions: BTreeSet<u32>,
    globals: BTreeSet<u32>,
}
impl References {
    fn of_bodies(table: &ModuleFunctionTable) -> Self {
        let mut references = Self::default();
        for operand in table.index_operands().iter().flatten() {
            match operand.space() {
                IndexSpace::Function => references.functions.insert(operand.index()),
                IndexSpace::Global => references.globals.insert(operand.index()),
            };
        }
        references
    }
}
impl Reencode for References {
    type Error = String;
    /// Body references were taken from the recorded index operands.
    fn parse_code_section(
        &mut self,
        _: &mut wasm_encoder::CodeSection,
        _: wasmparser::CodeSectionReader<'_>,
    ) -> Result<(), reencode::Error<String>> {
        Ok(())
    }
    fn function_index(&mut self, index: u32) -> Result<u32, reencode::Error<String>> {
        self.functions.insert(index);
        Ok(index)
    }
    fn global_index(&mut self, index: u32) -> Result<u32, reencode::Error<String>> {
        self.globals.insert(index);
        Ok(index)
    }
    fn parse_custom_section(
        &mut self,
        _: &mut Module,
        section: wasmparser::CustomSectionReader<'_>,
    ) -> Result<(), reencode::Error<String>> {
        check_custom(&section).map_err(user)
    }
    fn parse_element(
        &mut self,
        elements: &mut wasm_encoder::ElementSection,
        element: wasmparser::Element<'_>,
    ) -> Result<(), reencode::Error<String>> {
        match (&element.kind, &element.items) {
            (ElementKind::Declared, ElementItems::Functions(_)) => Ok(()),
            _ => reencode::utils::parse_element(self, elements, element),
        }
    }
}
struct Maps {
    functions: IndexMap<FunctionIndex>,
    globals: IndexMap<GlobalIndex>,
}
impl Maps {
    fn collect(bytes: &[u8], refs: &References) -> Result<Self, EmitError> {
        let (mut funcs, mut globals) = (Vec::new(), Vec::new());
        let (mut defined_functions, mut defined_globals) = (0, 0);
        for payload in parser().parse_all(bytes) {
            match payload.map_err(invalid)? {
                Payload::ImportSection(reader) => {
                    for import in reader.into_imports() {
                        let import = import.map_err(invalid)?;
                        let runtime = import.module == crate::RUNTIME_IMPORT_NAMESPACE;
                        match import.ty {
                            TypeRef::Func(_) | TypeRef::FuncExact(_) => {
                                let index = u32::try_from(funcs.len()).map_err(invalid)?;
                                funcs.push(!runtime || refs.functions.contains(&index));
                            }
                            TypeRef::Global(_) => {
                                let index = u32::try_from(globals.len()).map_err(invalid)?;
                                globals.push(!runtime || refs.globals.contains(&index));
                            }
                            TypeRef::Memory(_) | TypeRef::Table(_) | TypeRef::Tag(_) => {}
                        }
                    }
                }
                Payload::FunctionSection(reader) => defined_functions = reader.count(),
                Payload::GlobalSection(reader) => defined_globals = reader.count(),
                _ => {}
            }
        }
        Ok(Self {
            functions: IndexMap::new(&funcs, defined_functions).map_err(invalid)?,
            globals: IndexMap::new(&globals, defined_globals).map_err(invalid)?,
        })
    }
}
struct Rewrite<'a> {
    maps: &'a Maps,
    bytes: &'a [u8],
    index_operands: &'a [Vec<IndexOperand>],
    /// Final size of each body rewritten so far, in code-section order.
    body_sizes: Vec<usize>,
    /// Per body rewritten so far: the offset of each operand whose encoding
    /// became shorter, and by how many bytes.
    removed: Vec<Vec<(u32, u32)>>,
    body: Vec<u8>,
}
impl Rewrite<'_> {
    /// `raw` (one body as emitted) with each index operand renumbered.
    fn rewrite_body(&mut self, raw: &[u8], operands: &[IndexOperand]) -> Result<(), String> {
        self.body.clear();
        let mut removed = Vec::new();
        let mut copied = 0;
        for operand in operands {
            let at = operand.offset() as usize;
            if at < copied || at >= raw.len() {
                return Err("index operands are not in body order".into());
            }
            let mut reader = wasmparser::BinaryReader::new(&raw[at..], 0);
            let encoded = reader.read_var_u32().map_err(|error| error.to_string())?;
            if encoded != operand.index() {
                return Err(format!(
                    "recorded index operand {} does not match the body's {encoded}",
                    operand.index()
                ));
            }
            let index = match operand.space() {
                IndexSpace::Function => self.maps.functions.require(FunctionIndex(encoded))?.0,
                IndexSpace::Global => self.maps.globals.require(GlobalIndex(encoded))?.0,
            };
            self.body.extend_from_slice(&raw[copied..at]);
            let written = self.body.len();
            wasm_encoder::Encode::encode(&index, &mut self.body);
            let width = reader.current_position();
            let shorter = width - (self.body.len() - written);
            if shorter != 0 {
                removed.push((operand.offset(), shorter as u32));
            }
            copied = at + width;
        }
        self.body.extend_from_slice(&raw[copied..]);
        self.removed.push(removed);
        Ok(())
    }
}
impl Reencode for Rewrite<'_> {
    type Error = String;
    fn parse_function_body(
        &mut self,
        code: &mut wasm_encoder::CodeSection,
        func: wasmparser::FunctionBody<'_>,
    ) -> Result<(), reencode::Error<String>> {
        let index_operands = self.index_operands;
        let operands = index_operands
            .get(self.body_sizes.len())
            .ok_or_else(|| user("a code body has no recorded index operands"))?;
        let bytes = self.bytes;
        self.rewrite_body(&bytes[func.range()], operands).map_err(user)?;
        code.raw(&self.body);
        self.body_sizes.push(self.body.len());
        Ok(())
    }
    fn function_index(&mut self, index: u32) -> Result<u32, reencode::Error<String>> {
        self.maps
            .functions
            .require(FunctionIndex(index))
            .map(|i| i.0)
            .map_err(user)
    }
    fn global_index(&mut self, index: u32) -> Result<u32, reencode::Error<String>> {
        self.maps
            .globals
            .require(GlobalIndex(index))
            .map(|i| i.0)
            .map_err(user)
    }
    fn parse_import_section(
        &mut self,
        imports: &mut wasm_encoder::ImportSection,
        reader: wasmparser::ImportSectionReader<'_>,
    ) -> Result<(), reencode::Error<String>> {
        let (mut functions, mut globals) = (0, 0);
        for import in reader.into_imports() {
            let import = import?;
            let keep = match import.ty {
                TypeRef::Func(_) | TypeRef::FuncExact(_) => {
                    let keep = self
                        .maps
                        .functions
                        .get(FunctionIndex(functions))
                        .map_err(user)?
                        .is_some();
                    functions += 1;
                    keep
                }
                TypeRef::Global(_) => {
                    let keep = self
                        .maps
                        .globals
                        .get(GlobalIndex(globals))
                        .map_err(user)?
                        .is_some();
                    globals += 1;
                    keep
                }
                TypeRef::Memory(_) | TypeRef::Table(_) | TypeRef::Tag(_) => true,
            };
            if keep {
                imports.import(import.module, import.name, self.entity_type(import.ty)?);
            }
        }
        Ok(())
    }
    fn parse_element(
        &mut self,
        elements: &mut wasm_encoder::ElementSection,
        element: wasmparser::Element<'_>,
    ) -> Result<(), reencode::Error<String>> {
        if matches!(element.kind, ElementKind::Declared) {
            if let ElementItems::Functions(indices) = element.items {
                let mut kept = Vec::new();
                for index in indices {
                    if let Some(index) = self
                        .maps
                        .functions
                        .get(FunctionIndex(index?))
                        .map_err(user)?
                    {
                        kept.push(index.0);
                    }
                }
                // Keep the segment, even when empty: element indices do not move.
                elements.declared(wasm_encoder::Elements::Functions(kept.into()));
                return Ok(());
            }
        }
        reencode::utils::parse_element(self, elements, element)
    }
    fn parse_custom_section(
        &mut self,
        module: &mut Module,
        section: wasmparser::CustomSectionReader<'_>,
    ) -> Result<(), reencode::Error<String>> {
        check_custom(&section).map_err(user)?;
        reencode::utils::parse_custom_section(self, module, section)
    }
    fn parse_custom_name_subsection(
        &mut self,
        names: &mut wasm_encoder::NameSection,
        name: wasmparser::Name<'_>,
    ) -> Result<(), reencode::Error<String>> {
        names::rewrite(self, names, name)
    }
}

/// The only extraction path also refreshes the source-owned function table.
/// Compiler/native execution facts must consume these exact returned bytes.
pub(crate) fn compact(
    bytes: Vec<u8>,
    table: &mut ModuleFunctionTable,
) -> Result<Vec<u8>, EmitError> {
    let mut references = References::of_bodies(table);
    references
        .parse_core_module(&mut Module::new(), parser(), &bytes)
        .map_err(invalid)?;
    let maps = Maps::collect(&bytes, &references)?;
    if maps.functions.removed() == 0 && maps.globals.removed() == 0 {
        return Ok(bytes);
    }
    let mut module = Module::new();
    let mut rewrite = Rewrite {
        maps: &maps,
        bytes: &bytes,
        index_operands: table.index_operands(),
        body_sizes: Vec::new(),
        removed: Vec::new(),
        body: Vec::new(),
    };
    rewrite
        .parse_core_module(&mut module, parser(), &bytes)
        .map_err(invalid)?;
    let Rewrite {
        body_sizes,
        removed,
        ..
    } = rewrite;
    let output = module.finish();
    wasmparser::Validator::new_with_features(crate::runtime_artifact::program_validation_features()).validate_all(&output).map_err(invalid)?;
    table.rebind_compacted(&body_sizes, &removed, |index| {
        maps.functions
            .require(FunctionIndex(index))
            .map(|i| i.0)
            .map_err(invalid)
    })?;
    Ok(output)
}

#[cfg(test)]
mod tests;
