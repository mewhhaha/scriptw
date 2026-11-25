//! R defines its private linear memory instead of importing it.
//!
//! The emitter numbers a module's private memory 0 and, when it has one, its
//! shared memory 1. The shared memory must be imported (every agent's Store
//! links the same one), and Wasm numbers imported memories before defined
//! ones, so R as emitted imports its private memory too. The host then
//! created that memory for every Store and Wasmtime copied R's active data
//! segments into it at instantiation: 47 pages written per Store, identical in
//! every Store, program and process.
//!
//! This pass re-encodes R so it defines the private memory, bounded by the
//! per-Store cap, as memory [`RUNTIME_PRIVATE_MEMORY_INDEX`] behind the
//! imported shared memory, now [`RUNTIME_SHARED_MEMORY_INDEX`]. Every memory
//! immediate (operators, active data segments, the `memory` export and memory
//! names) is renumbered and nothing else changes. A defined memory gets
//! Wasmtime's copy-on-write image of R's active data (`memory_init_cow`),
//! mapped from R's native image, so a Store shares those pages with every
//! other one until it writes a page. Wasmtime resets a pooled slot to that
//! image and maps it afresh into an on-demand memory, so every Store still
//! starts from R's pristine bytes.
//!
//! P keeps importing `scriptw_host.private_memory`: the host defines that
//! import as R's exported memory once R is instantiated, so R and P (and code
//! compiled at run time) still share one private memory per Store.
use crate::emitted_function::ModuleFunctionTable;
use crate::module::{
    EmitError, HOST_IMPORT_MODULE, HOST_IMPORT_PRIVATE_MEMORY, HOST_IMPORT_SHARED_MEMORY,
    STORE_MEMORY_CAP_PAGES,
};
use wasm_encoder::reencode::{self, Reencode};
use wasmparser::{Operator, TypeRef};

/// R's private memory as R defines it.
pub(crate) const RUNTIME_PRIVATE_MEMORY_INDEX: u32 = 1;
/// R's imported shared memory.
pub(crate) const RUNTIME_SHARED_MEMORY_INDEX: u32 = 0;

/// The emitter's numbering, which every module but R keeps.
const EMITTED_PRIVATE_MEMORY_INDEX: u32 = 0;
const EMITTED_SHARED_MEMORY_INDEX: u32 = 1;

fn user(message: impl Into<String>) -> reencode::Error<String> {
    reencode::Error::UserError(message.into())
}

fn invalid(message: impl std::fmt::Display) -> EmitError {
    EmitError::unsupported(format!(
        "compiler invariant: R private memory definition failed: {message}"
    ))
}

/// R's `bytes` with its private memory defined. `table` follows the bodies:
/// a memory immediate can change length, so body sizes and the branch-hint
/// offsets after it move. Runs before the branch hinting section is written.
pub(crate) fn define(
    bytes: Vec<u8>,
    table: &mut ModuleFunctionTable,
) -> Result<Vec<u8>, EmitError> {
    let mut parser = wasmparser::Parser::new(0);
    parser.set_features(crate::runtime_artifact::program_validation_features());
    let mut definition = Definition::default();
    let mut module = wasm_encoder::Module::new();
    definition
        .parse_core_module(&mut module, parser, &bytes)
        .map_err(invalid)?;
    if definition.memory_imports != 2 || !definition.defined {
        return Err(invalid(
            "R did not import exactly its private and shared memories",
        ));
    }
    let output = module.finish();
    wasmparser::Validator::new_with_features(
        crate::runtime_artifact::program_validation_features(),
    )
    .validate_all(&output)
    .map_err(invalid)?;
    table.rebind_reencoded(&definition.body_sizes, &definition.branches)?;
    Ok(output)
}

#[derive(Default)]
struct Definition {
    /// The private memory import's type, once read.
    private: Option<wasm_encoder::MemoryType>,
    memory_imports: u32,
    defined: bool,
    body_sizes: Vec<usize>,
    /// Per body: the old and new offset of every two-way branch.
    branches: Vec<Vec<(u32, u32)>>,
}

fn is_two_way_branch(operator: &Operator<'_>) -> bool {
    matches!(
        operator,
        Operator::If { .. }
            | Operator::BrIf { .. }
            | Operator::BrOnNull { .. }
            | Operator::BrOnNonNull { .. }
            | Operator::BrOnCast { .. }
            | Operator::BrOnCastFail { .. }
    )
}

/// Whether a section with this id comes after the memory section.
fn follows_memory_section(id: wasm_encoder::SectionId) -> bool {
    use wasm_encoder::SectionId::*;
    matches!(
        id,
        Tag | Global | Export | Start | Element | DataCount | Code | Data
    )
}

impl Reencode for Definition {
    type Error = String;

    fn memory_index(&mut self, memory: u32) -> Result<u32, reencode::Error<String>> {
        match memory {
            EMITTED_PRIVATE_MEMORY_INDEX => Ok(RUNTIME_PRIVATE_MEMORY_INDEX),
            EMITTED_SHARED_MEMORY_INDEX => Ok(RUNTIME_SHARED_MEMORY_INDEX),
            other => Err(user(format!(
                "R names memory {other}, neither its private nor its shared memory"
            ))),
        }
    }

    fn parse_imports(
        &mut self,
        section: &mut wasm_encoder::ImportSection,
        imports: wasmparser::Imports<'_>,
    ) -> Result<(), reencode::Error<String>> {
        let wasmparser::Imports::Single(_, import) = &imports else {
            return Err(user("R's imports are not single imports"));
        };
        if let TypeRef::Memory(ty) = import.ty {
            let ordinal = self.memory_imports;
            self.memory_imports += 1;
            match (ordinal, import.module, import.name) {
                (EMITTED_PRIVATE_MEMORY_INDEX, HOST_IMPORT_MODULE, HOST_IMPORT_PRIVATE_MEMORY)
                    if !ty.shared && !ty.memory64 && ty.page_size_log2.is_none() =>
                {
                    self.private = Some(wasm_encoder::MemoryType {
                        minimum: ty.initial,
                        maximum: Some(ty.maximum.map_or(STORE_MEMORY_CAP_PAGES, |maximum| {
                            maximum.min(STORE_MEMORY_CAP_PAGES)
                        })),
                        memory64: false,
                        shared: false,
                        page_size_log2: None,
                    });
                    return Ok(());
                }
                (EMITTED_SHARED_MEMORY_INDEX, HOST_IMPORT_MODULE, HOST_IMPORT_SHARED_MEMORY)
                    if ty.shared => {}
                _ => {
                    return Err(user(
                        "R's memory imports are not its private and then its shared memory",
                    ))
                }
            }
        }
        reencode::utils::parse_imports(self, section, imports)
    }

    fn parse_memory_section(
        &mut self,
        _: &mut wasm_encoder::MemorySection,
        _: wasmparser::MemorySectionReader<'_>,
    ) -> Result<(), reencode::Error<String>> {
        Err(user("R as emitted defines no memory"))
    }

    fn intersperse_section_hook(
        &mut self,
        module: &mut wasm_encoder::Module,
        _after: Option<wasm_encoder::SectionId>,
        before: Option<wasm_encoder::SectionId>,
    ) -> Result<(), reencode::Error<String>> {
        if self.defined || !before.is_none_or(follows_memory_section) {
            return Ok(());
        }
        let private = self
            .private
            .ok_or_else(|| user("R does not import its private memory"))?;
        let mut memories = wasm_encoder::MemorySection::new();
        memories.memory(private);
        module.section(&memories);
        self.defined = true;
        Ok(())
    }

    fn parse_custom_section(
        &mut self,
        module: &mut wasm_encoder::Module,
        section: wasmparser::CustomSectionReader<'_>,
    ) -> Result<(), reencode::Error<String>> {
        if section.name() == crate::branch_hints::SECTION_NAME {
            return Err(user("branch hints are written after this pass"));
        }
        reencode::utils::parse_custom_section(self, module, section)
    }

    fn parse_function_body(
        &mut self,
        code: &mut wasm_encoder::CodeSection,
        func: wasmparser::FunctionBody<'_>,
    ) -> Result<(), reencode::Error<String>> {
        let start = func.range().start;
        let mut function = self.new_function_with_parsed_locals(&func)?;
        let mut reader = func.get_operators_reader()?;
        let mut branches = Vec::new();
        while !reader.eof() {
            let (operator, offset) = reader.read_with_offset()?;
            if is_two_way_branch(&operator) {
                let old = u32::try_from(offset - start).map_err(|_| user("body exceeds u32"))?;
                let new =
                    u32::try_from(function.byte_len()).map_err(|_| user("body exceeds u32"))?;
                branches.push((old, new));
            }
            function.instruction(&self.instruction(operator)?);
        }
        self.body_sizes.push(function.byte_len());
        self.branches.push(branches);
        code.function(&function);
        Ok(())
    }
}
