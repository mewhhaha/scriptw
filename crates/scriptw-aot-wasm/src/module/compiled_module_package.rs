use std::borrow::Cow;

use wasm_encoder::{ElementSection, Elements};

use super::*;
use crate::function_layout::ModuleEntryRole;
use crate::gc_types::FinalizedRuntimeModule;

/// The one type section and the typed indices assigned while constructing it.
pub(crate) struct ModuleTypeRegistry {
    registered: RuntimeModuleTypes,
}

impl ModuleTypeRegistry {
    pub(crate) fn new(kind: crate::runtime_artifact::ModuleKind<'_>) -> Self {
        Self {
            registered: RuntimeModuleTypes::register(kind),
        }
    }

    pub(crate) fn with_global_prelude(mut self, present: bool) -> Self {
        self.registered = self.registered.with_global_prelude(present);
        self
    }

    pub(crate) fn with_foreign_modules(
        mut self,
        registry: &scriptw_ir::foreign::ForeignModuleRegistry,
    ) -> Result<(Self, crate::foreign::ForeignImportTypes), EmitError> {
        let (registered, imports) = self.registered.with_foreign_modules(registry)?;
        self.registered = registered;
        Ok((self, imports))
    }

    /// Consumes every scalar/dynamic global and returns the sealed section with
    /// the runtime schema derived from its actual final scalar index.
    ///
    /// `snapshot_roots` declares the typed inventory and witness globals;
    /// `module_guard_count` once guards follow every root.
    pub(crate) fn finalize_globals(
        self,
        globals: ModuleGlobalSectionBuilder,
        snapshot_roots: bool,
        module_guard_count: u32,
    ) -> FinalizedModuleSections {
        FinalizedModuleSections {
            runtime: self.registered.finalize_globals(
                globals.section,
                snapshot_roots,
                module_guard_count,
            ),
        }
    }
}

/// The type and global sections finalized as one consume-once package.
///
/// Consuming [`ModuleTypeRegistry`] prevents a second global package from being
/// finalized against the same typed registry. The only next transition starts
/// package-owned code at the first defined function index.
pub(crate) struct FinalizedModuleSections {
    runtime: FinalizedRuntimeModule,
}

impl FinalizedModuleSections {
    pub(crate) fn begin(
        self,
        first_wasm_index: u32,
        entry_role: ModuleEntryRole,
    ) -> CompiledModulePackage {
        CompiledModulePackage {
            runtime: self.runtime,
            code: ModuleCode::new(first_wasm_index),
            main: None,
            program_initialize: None,
            program_functions: Vec::new(),
            main_emitted_local_count: 0,
            entry_role,
        }
    }
}

/// Types, rooted globals and paired function declarations/bodies, sealed
/// behind one assembly.
///
/// None of these Wasm sections has an independent append method. Bodies
/// extend package-owned code by mutable borrow, in planned index order (main
/// compiled against this exact root is one of them); only the final assembly
/// transition consumes the package. Normal Rust code therefore cannot combine
/// A's main with B's types or globals or reuse either package for another
/// module.
pub(crate) struct CompiledModulePackage {
    runtime: FinalizedRuntimeModule,
    code: ModuleCode,
    main: Option<EmittedFunction>,
    program_initialize: Option<(
        crate::function_layout::ProgramInitializeIndex,
        EmittedFunction,
    )>,
    program_functions: Vec<EmittedFunction>,
    main_emitted_local_count: u32,
    entry_role: ModuleEntryRole,
}

impl CompiledModulePackage {
    pub(crate) fn global_prelude_phase(&self) -> Option<crate::gc_types::GlobalScriptPreludePhase> {
        self.runtime.globals().global_prelude_phase()
    }

    pub(crate) fn schema(&self) -> &crate::gc_types::RuntimeSchema {
        self.runtime.globals().schema()
    }

    /// The types of the globals a program module imports from the runtime.
    pub(crate) fn runtime_global_types(&self) -> Vec<wasm_encoder::GlobalType> {
        self.runtime.globals().runtime_global_types()
    }
    /// Complete the pure P bootstrap against this same package's actual schema.
    pub(crate) fn compile_program_initialize(
        &mut self,
        compilation: crate::emit::ProgramInitializeCompilation<'_, '_>,
    ) -> Result<(), EmitError> {
        assert!(
            self.main.is_none() && self.program_initialize.is_none(),
            "P bootstrap precedes main and compiles once"
        );
        let completed = compilation.compile(self.runtime.globals())?;
        let index = completed.index();
        self.program_initialize = Some((index, EmittedFunction::program_initialize(completed)));
        Ok(())
    }

    /// Compiles and retains main against this package's own globals. Only final
    /// assembly can publish it, between the runtime and program functions.
    pub(crate) fn compile_main(
        &mut self,
        compilation: MainFunctionCompilation<'_>,
    ) -> Result<(), EmitError> {
        assert!(self.main.is_none(), "a module package compiles main once");
        assert_eq!(
            self.entry_role,
            ModuleEntryRole::Main,
            "only a main entry compiles JavaScript main"
        );
        let initializer = self.program_initialize.as_ref().map(|(index, _)| *index);
        let (main, emitted_local_count) =
            compilation.compile(self.runtime.globals(), initializer)?;
        self.main = Some(main);
        self.main_emitted_local_count = emitted_local_count;
        Ok(())
    }

    /// Runtime bodies precede main. Retain program bodies until the package
    /// publishes its own main, so no caller can separate main from its globals.
    pub(crate) fn append_functions(
        &mut self,
        runtime_functions: Vec<EmittedFunction>,
        program_functions: Vec<EmittedFunction>,
    ) {
        for function in runtime_functions {
            self.code.push(function);
        }
        self.program_functions.extend(program_functions);
    }

    pub(crate) const fn main_emitted_local_count(&self) -> u32 {
        self.main_emitted_local_count
    }

    pub(crate) fn append_to_module(
        self,
        module: &mut Module,
        sections: ModuleAssemblySections,
    ) -> ModuleFunctionTable {
        let Self {
            runtime,
            mut code,
            main,
            program_initialize,
            program_functions,
            main_emitted_local_count: _,
            entry_role,
        } = self;
        match entry_role {
            ModuleEntryRole::RuntimeOnly => assert!(
                main.is_none() && program_initialize.is_none(),
                "runtime modules have no P entry"
            ),
            ModuleEntryRole::Main => {
                assert!(main.is_some(), "main and its globals share their package")
            }
            ModuleEntryRole::NativeFactory => assert!(
                main.is_none() && program_initialize.is_some(),
                "native factories publish a P without JavaScript main"
            ),
        }
        if let Some(main) = main {
            code.push(main);
        }
        if let Some((_, initialize)) = program_initialize {
            code.push(initialize);
        }
        for function in program_functions {
            code.push(function);
        }
        let (functions, code, function_table) = code.finish();
        let ModuleAssemblySections {
            imports,
            callable_function_table,
            memories,
            exports,
            data,
            imported_globals,
            site_records,
        } = sections;
        let CallableFunctionTableSections { elements } = callable_function_table;

        module.section(runtime.types());
        module.section(&imports);
        module.section(&functions);
        if let Some(memories) = memories {
            module.section(&memories);
        }
        module.section(
            &runtime
                .globals()
                .defined_section(imported_globals, site_records),
        );
        module.section(&exports);
        module.section(&elements);
        if let Some(data) = &data {
            module.section(&wasm_encoder::DataCountSection { count: data.len() });
        }
        module.section(&code);
        if let Some(data) = data {
            module.section(&data);
        }

        function_table
    }
}

/// Declarative references for every defined function.
/// Calls use registered typed funcrefs; no dispatch table is allocated.
///
/// Keeping both encoder sections private prevents module assembly callers from
/// supplying an empty section or pairing entries from different ranges.
struct CallableFunctionTableSections {
    elements: ElementSection,
}

impl CallableFunctionTableSections {
    fn new(defined_functions: std::ops::Range<u32>) -> Self {
        let mut elements = ElementSection::new();
        let function_indexes = defined_functions.collect::<Vec<_>>();
        elements.declared(Elements::Functions(Cow::Owned(function_indexes)));

        Self { elements }
    }
}

/// The non-runtime core sections that must be interleaved with one compiled
/// runtime package according to Wasm's canonical section order.
pub(crate) struct ModuleAssemblySections {
    imports: ImportSection,
    callable_function_table: CallableFunctionTableSections,
    memories: Option<MemorySection>,
    exports: ExportSection,
    data: Option<DataSection>,
    /// Globals the module imports from the runtime; the global section
    /// declares only the ones after them.
    imported_globals: u32,
    /// The kinds of the site record globals the module's functions use, in
    /// order; they end the global section.
    site_records: Vec<crate::gc_types::SiteRecordKind>,
}

impl ModuleAssemblySections {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        imports: ImportSection,
        defined_functions: std::ops::Range<u32>,
        memories: Option<MemorySection>,
        exports: ExportSection,
        data: Option<DataSection>,
        imported_globals: u32,
        site_records: Vec<crate::gc_types::SiteRecordKind>,
    ) -> Self {
        Self {
            imports,
            callable_function_table: CallableFunctionTableSections::new(defined_functions),
            memories,
            exports,
            data,
            imported_globals,
            site_records,
        }
    }
}

// These function-pointer assignments are compile-time lifecycle gates. Module
// assembly must consume its package state, while adding compiled bodies may
// only borrow the one compiled package. A future edit that weakens those
// ownership transitions no longer matches these function types and fails to
// compile.
const _: fn(FinalizedModuleSections, u32, ModuleEntryRole) -> CompiledModulePackage =
    FinalizedModuleSections::begin;
const _: fn(crate::runtime_artifact::ModuleKind<'_>) -> ModuleTypeRegistry =
    ModuleTypeRegistry::new;
const _: fn(
    ImportSection,
    std::ops::Range<u32>,
    Option<MemorySection>,
    ExportSection,
    Option<DataSection>,
    u32,
    Vec<crate::gc_types::SiteRecordKind>,
) -> ModuleAssemblySections = ModuleAssemblySections::new;
const _: fn(
    &mut CompiledModulePackage,
    crate::emit::ProgramInitializeCompilation<'_, '_>,
) -> Result<(), EmitError> = CompiledModulePackage::compile_program_initialize;
const _: fn(&mut CompiledModulePackage, MainFunctionCompilation<'_>) -> Result<(), EmitError> =
    CompiledModulePackage::compile_main;
const _: fn(&mut CompiledModulePackage, Vec<EmittedFunction>, Vec<EmittedFunction>) =
    CompiledModulePackage::append_functions;
const _: fn(CompiledModulePackage, &mut Module, ModuleAssemblySections) -> ModuleFunctionTable =
    CompiledModulePackage::append_to_module;

/// The sole construction path for the module global section.
///
/// The inner encoder is private and this wrapper is not a Wasm section. A
/// caller must finish it through the type registry before the result can be
/// attached to a module. Finalization both appends the GC root and creates the
/// only matching runtime schema, which makes omission or a separately planned
/// root index a compile error rather than an out-of-band convention.
pub(crate) struct ModuleGlobalSectionBuilder {
    section: GlobalLedger,
}

impl ModuleGlobalSectionBuilder {
    pub(crate) fn new() -> Self {
        Self {
            section: GlobalLedger::new(),
        }
    }

    pub(crate) fn global(&mut self, global_type: GlobalType, init_expr: &ConstExpr) -> &mut Self {
        self.section.global(global_type, init_expr);
        self
    }
}

/// Fixed stateless completion bytes are assembled beside the sole module owner.
/// The input proof permits no source, data, environment or P to escape.
pub(crate) fn empty_direct_eval_module(
    _proof: &scriptw_ir::EmptyRuntimeDirectEval<'_>,
    export: &str,
) -> Vec<u8> {
    stateless_direct_eval_module(export, DirectEvalCompletionOperands::Undefined)
}

pub(crate) fn primitive_direct_eval_module(
    _proof: &scriptw_ir::PrimitiveRuntimeDirectEval<'_>,
    export: &str,
) -> Vec<u8> {
    stateless_direct_eval_module(export, DirectEvalCompletionOperands::Primitive)
}

enum DirectEvalCompletionOperands {
    Undefined,
    Primitive,
}

fn stateless_direct_eval_module(export: &str, operands: DirectEvalCompletionOperands) -> Vec<u8> {
    use crate::WasmRuntimeValueTag;
    use scriptw_ir::CompletionKindIr;
    use wasm_encoder::{
        CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
        RefType, TypeSection, ValType,
    };
    let mut module = Module::new();
    let mut types = TypeSection::new();
    let parameters: &[ValType] = match operands {
        DirectEvalCompletionOperands::Undefined => &[],
        DirectEvalCompletionOperands::Primitive => &[ValType::I32, ValType::I64],
    };
    types.ty().function(
        parameters.iter().copied(),
        [
            ValType::I32,
            ValType::I64,
            ValType::Ref(RefType::EQREF),
            ValType::I32,
            ValType::I32,
            ValType::I32,
        ],
    );
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut exports = ExportSection::new();
    exports.export(export, ExportKind::Func, 0);
    module.section(&exports);
    let mut body = Function::new([]);
    match operands {
        DirectEvalCompletionOperands::Undefined => {
            body.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
            body.instruction(&Instruction::I64Const(0));
        }
        DirectEvalCompletionOperands::Primitive => {
            body.instruction(&Instruction::LocalGet(0));
            body.instruction(&Instruction::LocalGet(1));
        }
    }
    body.instruction(&Instruction::RefNull(RefType::EQREF.heap_type));
    body.instruction(&Instruction::I32Const(
        CompletionKindIr::Normal.abi_code() as i32
    ));
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::I32Const(2)); // Entered.
    body.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&body);
    module.section(&code);
    module.finish()
}
