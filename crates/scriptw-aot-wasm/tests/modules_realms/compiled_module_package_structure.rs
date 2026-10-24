const MODULE_SOURCE: &str = include_str!("../../src/module.rs");
const OWNER_SOURCE: &str = include_str!("../../src/module/compiled_module_package.rs");
const EMIT_SOURCE: &str = include_str!("../../src/emit/module_assembly.rs");
const ENTRY_EXPORTS_SOURCE: &str =
    include_str!("../../src/emit/module_assembly/program_entries.rs");
const EMITTER_FAMILY: &str = concat!(
    include_str!("../../src/emit.rs"),
    include_str!("../../src/emit/body_compilation.rs"),
    include_str!("../../src/emit/body_entry.rs"),
    include_str!("../../src/emit/body_entry/literal_roots.rs"),
    include_str!("../../src/emit/module_assembly.rs"),
    include_str!("../../src/emit/module_assembly/program_entries.rs"),
    include_str!("../../src/emit/module_assembly/metadata.rs"),
);
const CODE_SOURCE: &str = include_str!("../../src/emitted_function.rs");
const GC_SOURCE: &str = include_str!("../../src/gc_types.rs");
const SNAPSHOT_SOURCE: &str = include_str!("../../src/gc_types/snapshot.rs");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end after {start}: {end}"))
        .0
}

fn compact(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[test]
fn compiled_module_package_has_one_private_owner_and_narrow_reexport() {
    let module_production = MODULE_SOURCE
        .split_once("#[cfg(test)]")
        .expect("module test boundary")
        .0;
    assert_eq!(
        module_production
            .matches("\nmod compiled_module_package;\n")
            .count(),
        1
    );
    assert!(!module_production.contains("\npub mod compiled_module_package;\n"));
    assert!(!module_production.contains("\nmod compiled_module_package {\n"));
    assert!(OWNER_SOURCE.contains("\nuse super::*;\n"));

    let reexport = bounded(
        module_production,
        "pub(crate) use compiled_module_package::{",
        "};",
    );
    for surface in [
        "ModuleAssemblySections",
        "ModuleGlobalSectionBuilder",
        "ModuleTypeRegistry",
    ] {
        assert_eq!(reexport.matches(surface).count(), 1, "{surface}");
    }
    for private_state in [
        "FinalizedModuleSections",
        "CompiledModulePackage",
        "CallableFunctionTableSections",
    ] {
        assert!(!reexport.contains(private_state), "{private_state}");
        assert!(!module_production.contains(&format!("struct {private_state}")));
        assert_eq!(
            OWNER_SOURCE
                .matches(&format!("struct {private_state}"))
                .count(),
            1,
            "{private_state}"
        );
    }

    for sole_owner in [
        "struct ModuleTypeRegistry",
        "struct FinalizedModuleSections",
        "struct CompiledModulePackage",
        "struct CallableFunctionTableSections",
        "struct ModuleAssemblySections",
        "struct ModuleGlobalSectionBuilder",
    ] {
        assert_eq!(OWNER_SOURCE.matches(sole_owner).count(), 1, "{sole_owner}");
        assert!(!module_production.contains(sole_owner), "{sole_owner}");
    }
    assert!(!OWNER_SOURCE.contains("struct ModuleTypeSectionBuilder"));
}

#[test]
fn package_lifecycle_is_consume_once_and_compile_time_checked() {
    let package_finalizers = bounded(
        OWNER_SOURCE,
        "impl ModuleTypeRegistry {",
        "/// The type and global sections finalized as one consume-once package.",
    );
    let package_finalizers = compact(package_finalizers);
    assert_eq!(package_finalizers.matches("fnfinalize_globals(").count(), 1);
    assert!(package_finalizers.contains(concat!(
        "fnfinalize_globals(self,globals:ModuleGlobalSectionBuilder,",
        "snapshot_roots:bool,module_guard_count:u32,)->FinalizedModuleSections"
    )));
    assert!(package_finalizers.contains(concat!(
        "self.registered.finalize_globals(",
        "globals.section,snapshot_roots,module_guard_count,)"
    )));
    let root_finalizers = bounded(
        GC_SOURCE,
        "impl RuntimeGcTypes {",
        "/// Frozen type declarations and their exact assigned GC indices.",
    );
    assert_eq!(root_finalizers.matches("fn finalize_globals(").count(), 1);
    assert!(compact(root_finalizers).contains(concat!(
        "fnfinalize_globals(self,mutglobals:GlobalLedger,snapshot:bool,",
        "module_guard_count:u32,)->FinalizedModuleGlobals"
    )));
    assert!(root_finalizers.contains(
        "snapshot.then(|| snapshot::SnapshotRoots::declare(&self.layouts, &mut globals))"
    ));
    let root_finalizers = compact(root_finalizers);
    assert_eq!(
        root_finalizers
            .matches(concat!(
                "FinalizedModuleGlobals{global_prelude_phase:None,",
                "section:globals,runtime_schema,}"
            ))
            .count(),
        1,
        "one construction seals the actual section with its matching schema"
    );
    let snapshot_roots = bounded(
        SNAPSHOT_SOURCE,
        "pub(super) struct SnapshotRoots {",
        "impl SnapshotRoots {",
    );
    assert!(snapshot_roots.contains("entry: GcRootGlobal<RealmRecord>"));
    assert!(snapshot_roots.contains("inventory: GcRootGlobal<SnapshotRealmInventory>"));
    assert!(!snapshot_roots.contains("pub "));
    assert!(!snapshot_roots.contains("pub("));
    let globals_view = bounded(
        GC_SOURCE,
        "impl FinalizedModuleGlobals {",
        "fn emit_root_get",
    );
    assert!(globals_view.contains("fn defined_section(&self, imported: u32) -> impl Section + '_"));
    assert!(globals_view.contains("ledger: &'a GlobalLedger"));
    assert!(globals_view.contains("self.ledger.section_after(self.imported).encode(sink)"));
    assert!(globals_view.contains("ledger: &self.section"));
    assert!(!globals_view.contains("-> GlobalSection"));
    assert!(!globals_view.contains("-> &GlobalLedger"));
    assert!(!globals_view.contains("#[derive"));

    let package_fields = bounded(
        OWNER_SOURCE,
        "pub(crate) struct CompiledModulePackage {",
        "impl CompiledModulePackage {",
    );
    assert!(package_fields.contains("    main: Option<EmittedFunction>,"));
    assert!(package_fields.contains("    program_functions: Vec<EmittedFunction>,"));
    assert!(!package_fields.contains("pub "));
    assert!(!package_fields.contains("pub("));
    let compile_main = bounded(
        OWNER_SOURCE,
        "pub(crate) fn compile_main(",
        "pub(crate) fn append_functions(",
    );
    assert!(compact(compile_main).starts_with(concat!(
        "&mutself,compilation:MainFunctionCompilation<'_>,)->Result<(),EmitError>"
    )));
    assert!(compile_main.contains("assert!(self.main.is_none()"));
    assert!(compact(compile_main).contains(concat!(
        "letinitializer=self.program_initialize.as_ref().map(|(index,_)|*index);",
        "let(main,emitted_local_count)=compilation.compile(self.runtime.globals(),initializer)?;",
        "self.main=Some(main);self.main_emitted_local_count=emitted_local_count;"
    )));
    let main_plan = compact(bounded(
        EMITTER_FAMILY,
        "impl<'a> MainFunctionCompilation<'a> {",
        "pub(crate) enum CompletionKind",
    ));
    let compile_plan = main_plan.split_once("pub(crate)fncompile(").unwrap().1;
    assert!(compile_plan.starts_with(concat!(
        "self,module_globals:&FinalizedModuleGlobals,",
        "program_initialize:Option<crate::function_layout::ProgramInitializeIndex>,",
        ")->Result<(EmittedFunction,u32),EmitError>"
    )));
    assert_eq!(compile_plan.matches("module_globals,").count(), 1);
    assert_eq!(
        compile_plan
            .matches("builder.compile_main_with_program_initialize(program_initialize)?")
            .count(),
        1
    );
    assert!(compile_main.contains("self.main = Some(main);"));
    let append_bodies = bounded(
        OWNER_SOURCE,
        "pub(crate) fn append_functions(",
        "pub(crate) const fn main_emitted_local_count",
    );
    assert!(compact(append_bodies).contains(concat!(
        "forfunctioninruntime_functions{self.code.push(function);}",
        "self.program_functions.extend(program_functions);"
    )));
    let publish = bounded(
        OWNER_SOURCE,
        "pub(crate) fn append_to_module(",
        "/// Declarative references for every defined function.",
    );
    assert!(compact(publish).contains(concat!(
        "ifletSome(main)=main{code.push(main);}",
        "ifletSome((_,initialize))=program_initialize{code.push(initialize);}",
        "forfunctioninprogram_functions{code.push(function);}",
        "let(functions,code,function_table)=code.finish();"
    )));
    let roles = compact(bounded(
        publish,
        "match entry_role {",
        "if let Some(main) = main {",
    ));
    assert_eq!(roles.matches("ModuleEntryRole::").count(), 3);
    for role in [
        "ModuleEntryRole::RuntimeOnly=>assert!(main.is_none()&&program_initialize.is_none(),",
        "ModuleEntryRole::Main=>{assert!(main.is_some(),",
        "ModuleEntryRole::NativeFactory=>assert!(main.is_none()&&program_initialize.is_some(),",
    ] {
        assert!(
            roles.contains(role),
            "lost entry-role ownership guard: {role}"
        );
    }
    assert!(publish.contains("runtime.globals().defined_section(imported_globals)"));
    assert!(OWNER_SOURCE.contains("let (functions, code, function_table) = code.finish();"));

    let code_owner = bounded(
        CODE_SOURCE,
        "pub(crate) struct ModuleCode {",
        "/// Every emitted function, in code-section order",
    );
    assert!(code_owner.contains("    functions: FunctionSection,"));
    assert!(code_owner.contains("    section: CodeSection,"));
    assert!(!code_owner.contains("pub(crate) functions:"));
    assert!(!code_owner.contains("pub(crate) section:"));
    let push = bounded(
        code_owner,
        "pub(crate) fn push(&mut self, function: EmittedFunction)",
        "pub(crate) fn finish(self)",
    );
    let declaration = "self.functions.function(type_index);";
    let body = "self.section.raw(&function.raw_body);";
    assert!(push.contains("FunctionDeclaration::NonCallable => function.identity.type_index()"));
    assert!(push.contains("FunctionDeclaration::Planned(entry) =>"));
    assert!(
        push.find("entry.assert_body_index(wasm_index);").unwrap()
            < push.find(declaration).unwrap()
    );
    assert!(push.find("entry.signature().type_index()").unwrap() < push.find(declaration).unwrap());
    assert_eq!(push.matches(declaration).count(), 1);
    assert_eq!(push.matches(body).count(), 1);
    assert!(push.find(declaration).unwrap() < push.find(body).unwrap());
    assert!(code_owner
        .contains("fn finish(self) -> (FunctionSection, CodeSection, ModuleFunctionTable)"));

    for ownership_gate in [
        "FinalizedModuleSections::begin;",
        "CompiledModulePackage::compile_main;",
        "CompiledModulePackage::append_functions;",
        "CompiledModulePackage::append_to_module;",
    ] {
        assert_eq!(
            OWNER_SOURCE.matches(ownership_gate).count(),
            1,
            "{ownership_gate}"
        );
    }
    assert!(compact(OWNER_SOURCE).contains(concat!(
        "const_:fn(&mutCompiledModulePackage,MainFunctionCompilation<'_>)->Result<(),EmitError>=",
        "CompiledModulePackage::compile_main;"
    )));
    assert!(OWNER_SOURCE.contains(concat!(
        "const _: fn(&mut CompiledModulePackage, Vec<EmittedFunction>, Vec<EmittedFunction>)"
    )));
    assert!(OWNER_SOURCE
        .contains("const _: fn(CompiledModulePackage, &mut Module, ModuleAssemblySections)"));
}

#[test]
fn emitter_consumes_the_package_through_the_reviewed_lifecycle() {
    for (call, count) in [
        ("ModuleTypeRegistry::new(", 1),
        ("ModuleGlobalSectionBuilder::new(", 1),
        (
            "module_types.finalize_globals(globals, uses_heap, module_guard_count)",
            1,
        ),
        (
            "module_sections.begin(first_defined_function, layout.entry_role())",
            1,
        ),
        ("module_package.compile_main(", 1),
        ("module_package.compile_program_initialize(", 1),
        (
            "module_package.append_functions(compiled_functions, program_functions)",
            1,
        ),
        ("module_package.main_emitted_local_count()", 1),
        ("module_package.append_to_module(", 1),
        ("ModuleAssemblySections::new(", 1),
    ] {
        assert_eq!(EMIT_SOURCE.matches(call).count(), count, "{call}");
    }
    assert!(compact(EMIT_SOURCE).contains(
        "iflayout.main_index().is_some(){module_package.compile_main(MainFunctionCompilation::new("
    ));
    let initialize = EMIT_SOURCE
        .find("if let Some(index) = layout.program_initialize_index() {")
        .unwrap();
    let initialize_body = EMIT_SOURCE
        .find("module_package.compile_program_initialize(ProgramInitializeCompilation::new(")
        .unwrap();
    let main = EMIT_SOURCE
        .find("if layout.main_index().is_some() {")
        .unwrap();
    assert!(initialize < initialize_body && initialize_body < main);
    assert!(!EMIT_SOURCE.contains("let main_function ="));
    assert!(EMIT_SOURCE.contains("program_functions.extend(program_helpers);"));
    assert_eq!(
        EMIT_SOURCE
            .matches(
                "let entry_exports = ProgramExports::new(kind, &layout, factory, &function_metas)?;"
            )
            .count(),
        1,
    );
    assert!(compact(EMIT_SOURCE).contains(concat!(
        "entry_exports.append(module_package.schema(),",
        "module_package.global_prelude_phase(),",
        "script.module_entry_evaluation().is_some(),runtime_helper_base,&mutexports,);"
    )));
    let export_plan = bounded(
        ENTRY_EXPORTS_SOURCE,
        "pub(super) fn new(",
        "pub(super) fn append(",
    );
    assert_eq!(
        export_plan
            .matches("owns_globals: kind.owns_globals(),")
            .count(),
        1
    );
    let append_exports = ENTRY_EXPORTS_SOURCE
        .split_once("pub(super) fn append(")
        .expect("typed entry export consumer")
        .1;
    assert_eq!(
        append_exports
            .matches("schema.export_snapshot_roots(exports);")
            .count(),
        1
    );
    assert!(
        compact(append_exports).contains(concat!(
            "ifself.owns_globals{schema.export_throw_diagnostics(exports);",
            "schema.export_snapshot_roots(exports);}"
        )),
        "only the instance owning the supplied package schema exports its roots"
    );

    for escaped_append in [
        "module.section(&types)",
        "module.section(&globals)",
        "module.section(runtime.types())",
        "module.section(runtime.globals())",
        "module.section(&code)",
    ] {
        assert!(!EMITTER_FAMILY.contains(escaped_append), "{escaped_append}");
    }
    assert!(!EMITTER_FAMILY.contains("CompiledModulePackage"));
    assert!(!EMITTER_FAMILY.contains("FinalizedModuleSections"));
    assert!(!EMITTER_FAMILY.contains("FunctionSection"));
    let (package_owner, stateless_entries) = OWNER_SOURCE
        .split_once("pub(crate) fn empty_direct_eval_module(")
        .expect("stateless empty eval has its separate proof-gated entry");
    assert!(!package_owner.contains("FunctionSection"));
    let (empty_entry, primitive_entry) = stateless_entries
        .split_once("pub(crate) fn primitive_direct_eval_module(")
        .expect("scalar eval has a separate typed proof entry");
    let signature = compact(empty_entry.split_once(") -> Vec<u8> {").unwrap().0);
    assert_eq!(
        signature,
        "_proof:&scriptw_ir::EmptyRuntimeDirectEval<'_>,export:&str,"
    );
    assert!(compact(empty_entry)
        .contains("stateless_direct_eval_module(export,DirectEvalCompletionOperands::Undefined)"));
    let (primitive_entry, shared_factory) = primitive_entry
        .split_once("fn stateless_direct_eval_module(")
        .expect("stateless entries share only the typed completion assembler");
    assert_eq!(
        compact(primitive_entry.split_once(") -> Vec<u8> {").unwrap().0),
        "_proof:&scriptw_ir::PrimitiveRuntimeDirectEval<'_>,export:&str,"
    );
    assert!(compact(primitive_entry)
        .contains("stateless_direct_eval_module(export,DirectEvalCompletionOperands::Primitive)"));
    assert_eq!(shared_factory.matches("FunctionSection::new()").count(), 1);
    assert!(shared_factory.contains("let mut body = Function::new([]);"));
    assert!(shared_factory.contains("WasmRuntimeValueTag::Undefined.tag()"));
    assert!(shared_factory.contains("CompletionKindIr::Normal.abi_code()"));
    assert!(shared_factory.contains("Instruction::LocalGet(0)"));
    assert!(shared_factory.contains("Instruction::LocalGet(1)"));
    for stateful_section in [
        "ImportSection",
        "GlobalSection",
        "MemorySection",
        "DataSection",
    ] {
        assert!(
            !stateless_entries.contains(stateful_section),
            "{stateful_section}"
        );
    }
}
