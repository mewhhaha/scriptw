const SOURCE: &str = include_str!("../../src/emit.rs");
const COMPILATION: &str = include_str!("../../src/emit/body_compilation.rs");
const ENTRY: &str = include_str!("../../src/emit/body_entry.rs");
const PROGRAM_INITIALIZE: &str =
    include_str!("../../src/emit/body_compilation/program_initialize.rs");
const MODULE_ASSEMBLY: &str = include_str!("../../src/emit/module_assembly.rs");
const SIGNATURES: &str = include_str!("../../src/module/static_signature.rs");
const SOURCE_FAMILY: &str = concat!(
    include_str!("../../src/emit.rs"),
    include_str!("../../src/emit/body_compilation.rs"),
    include_str!("../../src/emit/body_entry.rs"),
    include_str!("../../src/emit/body_entry/literal_roots.rs"),
    include_str!("../../src/emit/template_objects.rs"),
    include_str!("../../src/emit/body_compilation/program_initialize.rs"),
);

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn normalized_code(source: &str) -> String {
    let source = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut normalized = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for character in source.chars() {
        if in_string {
            normalized.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
            normalized.push(character);
        } else if !character.is_whitespace() {
            normalized.push(character);
        }
    }
    normalized
}

#[test]
fn function_module_state_is_the_exact_private_no_capability_domain() {
    let declaration = bounded(
        SOURCE,
        concat!(
            "pub(crate) enum ReturnAbi {\n",
            "    MainExport,\n",
            "    MultiValue,\n",
            "    ProgramInstance,\n",
            "}\n"
        ),
        "/// Only a builder owning the planned entry can finish this body.",
    );
    let state = normalized_code(bounded(
        declaration,
        "enum FunctionModuleState<'a> {",
        "\n}",
    ))
    .replace(",)", ")");
    assert_eq!(
        state,
        concat!(
            "ProgramInitialize,Main(&'aFinalizedModuleGlobals,PromiseRejectionPolicy),",
            "PreparedScript(&'aPreparedScriptUnit,&'acrate::function_entry::PlannedFunctionEntry),",
            "Internal(&'acrate::function_entry::PlannedFunctionEntry),RuntimeOperation(RuntimeHelperId),"
        )
    );
    let declaration = normalized_code(declaration);
    assert!(declaration.contains("Self::Main(_,_)=>StaticSignature::Main.parameter_count()"));
    assert!(declaration.contains(
        "Self::Internal(entry)=>entry.local_arguments().map_or_else(||entry.signature().parameter_count(),|local|local.signature().parameter_count(),)"
    ));
    assert!(
        declaration.contains("Self::PreparedScript(_,entry)=>entry.signature().parameter_count()")
    );
    assert!(declaration.contains("Self::RuntimeOperation(helper)=>helper.parameter_types().len()"));
    assert!(declaration
        .contains("Self::Internal(entry)|Self::PreparedScript(_,entry)=>Some(entry.body())"));
    assert!(declaration
        .contains("Self::Main(_,_)|Self::RuntimeOperation(_)|Self::ProgramInitialize=>None"));
    assert!(!declaration.contains("#["));
    let normalized_source = normalized_code(SOURCE_FAMILY);
    for capability in ["Clone", "Copy", "Debug", "PartialEq", "Eq", "Default"] {
        assert!(!normalized_source.contains(&format!("{capability}forFunctionModuleState")));
    }
    assert!(!SOURCE.contains("== FunctionModuleState::"));
    assert!(!SOURCE.contains("!= FunctionModuleState::"));
    assert!(!SOURCE.contains("matches!(module_state"));

    // Source-site ownership adds an exhaustive observation: one Main arm and
    // source/non-source arms for the other ordinary body kinds.
    assert_eq!(
        SOURCE_FAMILY.matches("FunctionModuleState::Main").count(),
        11
    );
    assert_eq!(
        SOURCE_FAMILY
            .matches("FunctionModuleState::PreparedScript")
            .count(),
        14
    );
    assert_eq!(
        SOURCE_FAMILY
            .matches("FunctionModuleState::Internal")
            .count(),
        14
    );
}

#[test]
fn exactly_seven_constructors_choose_their_named_module_states() {
    for (start, end, expected) in [
        (
            "    fn new_main(",
            "    fn new_prepared_script(",
            "FunctionModuleState::Main(module_globals, promise_rejection_policy)",
        ),
        (
            "    fn new_prepared_script(",
            "    fn new_function(",
            "FunctionModuleState::PreparedScript(",
        ),
        (
            "    fn new_function(",
            "    fn new_host_builtin(",
            "FunctionModuleState::Internal",
        ),
        (
            "    fn new_host_builtin(",
            "    pub(super) fn new_program_initialize(",
            "FunctionModuleState::Internal",
        ),
        (
            "    pub(super) fn new_program_initialize(",
            "    fn new_runtime_operation_helper(",
            "FunctionModuleState::ProgramInitialize",
        ),
        (
            "    fn new_runtime_operation_helper(",
            "    fn new_standard_builtin(",
            "FunctionModuleState::RuntimeOperation",
        ),
        (
            "    fn new_standard_builtin(",
            "    fn new(\n        schema: &'a RuntimeSchema,\n        body: &'a BlockIr,",
            "FunctionModuleState::Internal",
        ),
    ] {
        let constructor = bounded(SOURCE, start, end);
        assert_eq!(constructor.matches(expected).count(), 1, "{start}");
        assert_eq!(
            constructor.matches("FunctionModuleState::").count(),
            1,
            "{start}"
        );
    }

    let builder = bounded(
        SOURCE,
        "pub(crate) struct FunctionBuilder<'a> {",
        "impl<'a> FunctionBuilder<'a> {",
    );
    assert_eq!(
        builder
            .matches("module_state: FunctionModuleState<'a>,")
            .count(),
        1
    );

    let shared_constructor = bounded(
        SOURCE,
        "    fn new(\n        schema: &'a RuntimeSchema,\n        body: &'a BlockIr,",
        "    pub(crate) fn emitted_local_count(",
    );
    assert_eq!(
        shared_constructor
            .matches("module_state: FunctionModuleState<'a>,")
            .count(),
        1
    );
    assert_eq!(
        shared_constructor
            .matches("            module_state,\n")
            .count(),
        1
    );
    assert!(
        shared_constructor
            .find("let return_abi = module_state.return_abi();")
            .unwrap()
            < shared_constructor
                .find("            module_state,\n")
                .unwrap()
    );
    assert!(
        shared_constructor
            .find("Function::with_parameters(")
            .unwrap()
            < shared_constructor
                .find("            module_state,\n")
                .unwrap()
    );
}

#[test]
fn module_state_storage_and_anchor_projections_are_borrowed_and_exhaustive() {
    assert_eq!(SOURCE.matches("match &module_state {").count(), 2);
    assert!(!SOURCE.contains("let FunctionModuleState::Main("));
    let shared_constructor = bounded(
        SOURCE,
        "    fn new(\n        schema: &'a RuntimeSchema,\n        body: &'a BlockIr,",
        "    pub(crate) fn emitted_local_count(",
    );
    let parameter_policy = bounded(
        shared_constructor,
        "        let parameter_types = match &module_state {",
        "        let return_abi = module_state.return_abi();",
    );
    assert_eq!(
        normalized_code(parameter_policy),
        concat!(
            "FunctionModuleState::Main(_,_)=>schema.signature(StaticSignature::Main),",
            "FunctionModuleState::ProgramInitialize=>{schema.signature(StaticSignature::ProgramInitialize)}",
            "FunctionModuleState::PreparedScript(_,entry)=>schema.signature(entry.signature()),",
            "FunctionModuleState::Internal(entry)=>schema.signature(entry.local_arguments().map_or_else(||entry.signature(),|local|local.signature()),),",
            "FunctionModuleState::RuntimeOperation(helper)=>helper.definition(schema.layouts()),}.parameters().to_vec();"
        )
    );
    let construction_policy = bounded(
        shared_constructor,
        "        let hoisted_vars = match &module_state {",
        "        let template_source_body = template_source;",
    );
    assert!(!construction_policy.contains("_ =>"));
    for role in [
        "Main(_, _)",
        "ProgramInitialize",
        "PreparedScript(unit, _)",
        "Internal(_)",
        "RuntimeOperation(_)",
        "PreparedScript(_, _)",
    ] {
        assert!(construction_policy.contains(&format!("FunctionModuleState::{role}")));
    }
    // Only the role's actual planned entry supplies lexical/private roots.
    let roots = bounded(
        shared_constructor,
        "        let current_environment = schema",
        "        Self {",
    );
    assert!(roots.contains("reserve_gc_local::<Environment, Nullable>"));
    assert!(roots.contains("reserve_gc_local::<PrivateEnvironment, Nullable>"));
    assert!(roots.contains(".planned_body_entry()"));
    assert!(roots.contains("entry.lexical_environment().load(schema, &mut owned_body)"));
    assert!(roots.contains("entry.private_environment().load(schema, &mut owned_body)"));
    let clear = bounded(
        ENTRY,
        "    pub(super) fn clear_body_entry_roots(",
        "    /// Starts the body of `helper`",
    );
    for retained_root in [
        "context.clear(function)",
        "self.clear_entry_program(function)",
        "entry.clear(self.schema, function)",
        "self.current_environment.set_null(self.schema, function)",
    ] {
        assert!(clear.contains(retained_root));
    }
    assert!(normalized_code(clear)
        .contains("self.current_private_environment.set_null(self.schema,function)"));
    assert_eq!(
        COMPILATION
            .matches("self.clear_body_entry_roots(&mut function);")
            .count(),
        2
    );
}

#[test]
fn prepared_script_context_projections_keep_their_closed_role_policy() {
    assert_eq!(
        SOURCE_FAMILY.matches("match self.module_state {").count(),
        6
    );
    let projections = bounded(
        SOURCE,
        "    pub(crate) fn has_global_script_bindings(&self) -> bool {",
        "    pub(crate) const fn return_abi(&self) -> ReturnAbi {",
    );
    let projections = normalized_code(projections);
    assert!(projections.contains("FunctionModuleState::Main(_,_)=>true"));
    assert!(projections.contains("unit.has_global_variable_environment()"));
    assert!(projections.contains(
        "FunctionModuleState::Internal(_)|FunctionModuleState::RuntimeOperation(_)|FunctionModuleState::ProgramInitialize=>false"
    ));
    let owner = normalized_code(bounded(
        SOURCE,
        "    pub(crate) fn direct_eval_derived_owner(",
        "    pub(crate) fn direct_eval_execution_context_local(",
    ));
    assert!(owner.starts_with("&self)->Option<&scriptw_ir::DirectEvalDerivedOwner>{"));
    assert!(owner.contains("FunctionModuleState::ProgramInitialize=>None"));
    assert!(owner.contains(concat!(
        "FunctionModuleState::PreparedScript(unit,_)=>match&unit.kind{",
        "PreparedScriptKind::DirectEval(context)=>context.derived_owner(),",
        "PreparedScriptKind::RealmScript|PreparedScriptKind::IndirectEval|",
        "PreparedScriptKind::ShadowRealmEvaluate=>None,"
    )));
    assert_eq!(owner.matches("context.derived_owner()").count(), 2);
    assert!(owner.contains("ifunit.function_ids.contains(function_id)"));
    assert!(owner.contains("letPreparedScriptKind::DirectEval(context)=&unit.kindelse{"));
    assert!(!owner.contains("derived_constructor_owner("));
    assert!(projections.contains("self.direct_eval_context.as_ref()?;"));
    assert!(projections.contains("self.direct_eval_context.as_ref()"));
    assert!(normalized_code(SOURCE).contains("entry.private_environment()"));
    assert!(!projections.contains("Some(9)"));
    assert!(!projections.contains("Some(8)"));
    assert!(projections.contains("self.direct_eval_context"));
}

#[test]
fn prepared_script_role_controls_instantiation_and_script_completion() {
    let instantiation = bounded(
        COMPILATION,
        "        self.initialize_direct_eval_execution_context(&mut function)?;",
        "        self.init_runtime_roots(&mut function)?;",
    );
    assert_eq!(
        normalized_code(instantiation),
        concat!(
            "ifletFunctionModuleState::PreparedScript(unit,_)=self.module_state{",
            "self.emit_instantiate_prepared_script_declarations(unit,&mutfunction)?;}"
        )
    );
    let completion = bounded(
        COMPILATION,
        "        if matches!(self.return_abi(), ReturnAbi::MultiValue)",
        "        self.normalize_base_class_constructor_result(&mut function);",
    );
    assert_eq!(
        normalized_code(completion),
        concat!(
            "&&!matches!(self.module_state,FunctionModuleState::PreparedScript(_,_))",
            "&&!self.current_function_meta().is_some_and(|meta|",
            "meta.protocol().class_kind()==ClassFunctionKind::Constructor){",
            "self.completion.value().set_undefined(&mutfunction);}"
        )
    );
}

#[test]
fn only_the_main_export_selects_the_host_rejection_policy() {
    let checkpoint = bounded(
        COMPILATION,
        "            let promise_rejection_policy = match self.module_state {",
        "            self.emit_capture_final_throw_constructor_name(&mut function)?;",
    );
    assert_eq!(
        normalized_code(checkpoint),
        concat!(
            "FunctionModuleState::Main(_,policy)=>policy,",
            "FunctionModuleState::PreparedScript(_,_)|FunctionModuleState::Internal(_)|FunctionModuleState::RuntimeOperation(_)|FunctionModuleState::ProgramInitialize=>{",
            "unreachable!(\"only the main export reports unhandled rejections\")}};",
            "self.emit_report_unhandled_rejection(promise_rejection_policy,&mutfunction)?;"
        )
    );
}

#[test]
fn program_initialize_has_a_distinct_nonnull_result_and_never_enters_javascript() {
    assert!(normalized_code(SIGNATURES)
        .contains("ProgramInitialize=>([],[Gc(GcLayout::ProgramInstance,NonNullable)])"));
    let constructor = bounded(
        SOURCE,
        "    pub(super) fn new_program_initialize(",
        "    fn new_runtime_operation_helper(",
    );
    assert!(constructor.contains("&EMPTY_BLOCK"));
    assert!(constructor.contains("FunctionModuleState::ProgramInitialize"));
    assert!(!constructor.contains("FunctionModuleState::Main"));
    let begin = bounded(
        SOURCE,
        "    pub(super) fn begin_program_initialize_body(",
        "    pub(crate) fn current_function_context(",
    );
    assert!(begin.contains("self.is_program_initialize()"));
    assert!(begin.contains("self.body_entry_locals.is_none()"));
    assert!(begin.contains("self.entry_program.is_none()"));
    assert!(begin.contains("body.parameter_types().is_empty()"));
    assert!(PROGRAM_INITIALIZE.contains("let result: GcStackReference<ProgramInstance>"));
    assert!(PROGRAM_INITIALIZE.contains("program.clear(&mut function)"));
    for forbidden in [
        "self.completion.emit",
        "builder.compile()?",
        "init_current_realm(",
        "compile_block_contents(",
        "emit_enter_source_activation(",
        "MemoryInit",
        "PRIVATE_BYTE_CURSOR_GLOBAL_INDEX",
    ] {
        assert!(!PROGRAM_INITIALIZE.contains(forbidden), "{forbidden}");
    }
    assert!(
        COMPILATION.contains("self.is_program_initialize() || (self.is_main() && self.uses_heap)")
    );
    let main = bounded(
        COMPILATION,
        "    pub(super) fn compile_main_with_program_initialize(",
        "    fn compile_javascript_body(",
    );
    assert!(
        normalized_code(main).contains("if!self.is_main()||self.uses_heap!=initializer.is_some()")
    );
    let heap_main = bounded(
        COMPILATION,
        "        if self.uses_heap {\n            if self.is_main() {",
        "            self.init_current_env(&mut function)?;",
    );
    let bind = heap_main
        .find("self.bind_entry_program(program, &mut function);")
        .unwrap();
    assert!(
        bind < heap_main
            .find("self.init_current_realm(&mut function)?;")
            .unwrap()
    );
}

#[test]
fn gc_program_publication_does_not_reinstall_metadata_or_reset_runtime_scratch() {
    for source in [COMPILATION, PROGRAM_INITIALIZE] {
        for forbidden in [
            "legacy_static_data",
            "install_program_static_data",
            "ensure_heap_ptr_after_static_data",
            "Instruction::MemoryInit",
            "PRIVATE_BYTE_CURSOR_GLOBAL_INDEX",
        ] {
            assert!(!source.contains(forbidden), "{forbidden}");
        }
    }
    let compilation = normalized_code(COMPILATION);
    assert!(compilation.contains(concat!(
        "letprogram=self.runtime_schema().call_program_initialize(index,&mutfunction);",
        "self.bind_entry_program(program,&mutfunction);}",
        "self.init_current_realm(&mutfunction)?;"
    )));
    let assembly = normalized_code(MODULE_ASSEMBLY);
    assert!(assembly.contains(concat!(
        "&ConstExpr::i64_const(align_heap_start(ifuses_heap{",
        "string_pool.compiler_owned_boundary().static_len()",
        "}else{string_pool.bytes.len()})asi64)"
    )));
    assert!(
        assembly.contains("ModuleKind::Runtime(_)=>string_pool.append_runtime_data(&mutsection)")
    );
    assert!(
        assembly.contains("ModuleKind::Program(_)=>string_pool.append_program_data(&mutsection)")
    );
}
