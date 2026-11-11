const OBJECTS: &str = include_str!("../../src/objects.rs");
const HELPERS: &str = include_str!("../../src/runtime_helpers.rs");
const EMIT: &str = include_str!("../../src/emit.rs");
fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing {end}"))
        .0
}
fn normalized(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}
#[test]
fn indexed_get_forwards_one_typed_environment_and_whole_completion() {
    assert!(HELPERS.contains(r#"IndexedElementRead / IndexedElementReadArguments / IndexedElementReadParameters / "indexed_element_read" { target:Value,index:I64,caller_environment:(Ref Environment Nullable),caller_program:Program } => Completion;"#));
    let seam = normalized(bounded(
        OBJECTS,
        "pub(crate) fn emit_typed_array_or_object_index_read_from_locals(",
        "pub(crate) fn compile_indexed_element_read_helper(",
    ));
    assert_eq!(
        seam.matches("IndexedElementReadArguments::new(target,index,self.current_environment(),self.current_program(),)")
            .count(),
        1
    );
    assert!(seam.contains(".store(result,function)"));
    assert!(seam.contains("result:&CompletionLocals"));
    let parameters = normalized(bounded(
        EMIT,
        "pub(crate) fn helper_parameters<P:",
        "fn new_main(",
    ));
    assert_eq!(parameters.matches("parameters.caller_program()").count(), 1);
    assert!(parameters.contains(concat!(
        "ifletSome(program)=parameters.caller_program(){",
        "letowner=crate::gc_types::ProgramInstanceLocals::bind(",
        "self.schema,program.load(self.schema,function),function,);",
        "self.bind_entry_program(owner,function);}"
    )));
    assert!(
        parameters
            .find("self.bind_entry_program(owner,function)")
            .unwrap()
            < parameters.find("parameters.caller_environment()").unwrap()
    );
    let binding = normalized(bounded(
        EMIT,
        "pub(crate) fn bind_entry_program(",
        "pub(crate) fn capture_current_program(",
    ));
    assert!(binding.contains("self.body_entry_locals.is_none()&&self.entry_program.is_none()"));
    let mark = binding
        .find("program.mark_native_context(self.schema,function);")
        .unwrap();
    let retain = binding.find("self.entry_program=Some(program);").unwrap();
    assert!(mark < retain, "mark the selected owner before retaining it");
    let helpers = normalized(HELPERS);
    for typed_role in [
        "(@argumentProgram)=>{&'aProgramInstanceLocals}",
        "(@parameterProgram)=>{GcLocal<ProgramInstance>}",
        "(@abiProgram)=>{&[AbiType::Gc(GcLayout::ProgramInstance,GcNullability::NonNullable)]}",
        "(@acquire$schema:ident,$index:ident,$function:ident,Program)=>{$schema.parameter_gc::<ProgramInstance,NonNullable>($index,$function)}",
    ] {
        assert!(helpers.contains(typed_role), "lost {typed_role}");
    }
    assert!(!seam.contains("Instruction::Call("));
    assert!(!seam.contains("GlobalGet"));
}
#[test]
fn indexed_get_enters_its_typed_helper_domain_before_property_emission() {
    let body = bounded(
        OBJECTS,
        "pub(crate) fn compile_indexed_element_read_helper(",
        "pub(crate) fn compile_property_write_to_locals(",
    );
    let start = body
        .find("self.begin_helper_body(RuntimeHelperId::IndexedElementRead)")
        .unwrap();
    let bind = body
        .find("helper_parameters::<crate::runtime_helpers::IndexedElementReadParameters>")
        .unwrap();
    let read = body
        .find("self.emit_object_read_with_throw_routing(")
        .unwrap();
    let emit = body.find("result.emit(&mut function)").unwrap();
    assert!(start < bind && bind < read && read < emit);
    assert!(body.contains("AccessorThrowRouting::LeaveInCompletion"));
    assert_eq!(body.matches("&parameters.target,").count(), 2);
    let parameters = normalized(bounded(
        EMIT,
        "pub(crate) fn helper_parameters<P:",
        "\n    }",
    ));
    assert!(parameters.contains("parameters.caller_environment()"));
    assert!(parameters.contains(
        "self.current_environment.replace(environment.load(self.schema,function),function)"
    ));
}
