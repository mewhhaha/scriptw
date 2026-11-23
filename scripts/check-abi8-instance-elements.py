#!/usr/bin/env python3
"""Source-only ABI8 constructor-hook boundary checks; no compiler invocation."""
from __future__ import annotations
import re
import sys
from pathlib import Path

PREFIX = "crates/scriptw-aot-wasm/src/"
FILES = (
    "gc_types/layouts.rs", "gc_types/value/instance_elements.rs", "runtime_helpers.rs",
    "functions/instance_elements_hook.rs", "functions/function_object_allocation.rs",
    "functions/class_captures.rs", "functions.rs", "emit.rs", "emit/body_entry.rs",
    "emit/module_assembly.rs", "emit/module_assembly/program_helper_bodies.rs",
    "function_entry/executable_code.rs",
)


def compact(source: str) -> str:
    source = re.sub(r"//[^\n]*", "", source)
    return re.sub(r"\s+", "", source)


def body(source: str, name: str) -> str:
    start = source.index("fn " + name + "(")
    opening = source.index("{", start)
    depth = 1
    cursor = opening + 1
    while depth:
        depth += (source[cursor] == "{") - (source[cursor] == "}")
        cursor += 1
    return compact(source[opening + 1:cursor - 1])


def check(sources: dict[str, str]) -> list[str]:
    failures = []
    def require(ok: bool, reason: str) -> None:
        if not ok:
            failures.append(reason)
    c = {name: compact(text) for name, text in sources.items()}
    layout = c["gc_types/layouts.rs"]
    require("INSTANCE_ELEMENTS_INITIALIZER:GcFunctionRef<InitializeInstanceElementsHook>,Immutable,Nullable;" in layout,
            "constructor hook must be immutable nullable typed funcref")
    require("constSIGNATURE:GcFunctionSignature=GcFunctionSignature::ContextHook(crate::runtime_helpers::RuntimeHelperId::InitializeInstanceElements,);" in layout,
            "constructor hook role must retain its registered helper signature")
    derived = layout.split("structDirectEvalDerivedBindings=>DirectEvalDerivedBindingsSchema{")[1].split("}")[0]
    require(derived == "".join(name + ":GcRef<BindingCell>,Immutable,NonNullable;" for name in
            ("THIS_CELL", "THIS_STATUS_CELL", "NEW_TARGET_CELL", "ACTIVE_FUNCTION_CELL")),
            "derived binding record must retain all four immutable cell identities")
    registry = c["runtime_helpers.rs"]
    require('InitializeInstanceElements/InitializeInstanceElementsArguments/InitializeInstanceElementsParameters/"initialize_instance_elements"{caller_program:Program,caller_function_context:(RefFunctionContextNonNullable),receiver:Value}=>Completion;' in registry,
            "initializer ABI must be P!, constructor context!, receiver Value3 to Completion5")
    require("Self::InitializeInstanceElements=>HelperOwner::ContextHook," in registry and
            "Self::Runtime|Self::Hook(_)|Self::ContextHook=>HelperTypeRegion::Core," in registry and
            "Self::Program|Self::Hook(_)|Self::ContextHook=>HelperRegion::Program," in registry,
            "initializer body belongs to P and signature to core recursion group")
    hook = sources["functions/instance_elements_hook.rs"]
    imported = re.search(r"use crate::gc_types::\{(.*?)\};", hook, re.S)
    require(imported is not None and "ValueLocals" in imported.group(1).split(",")[-2].strip(),
            "receiver ValueLocals must be imported by the actual hook owner")
    domain = sources["runtime_helpers.rs"].split("runtime_helper_domain! {", 1)[1].split("\n}", 1)[0]
    names = re.findall(r"^    (\w+) /", domain, re.M)
    require(names[-2:] == ["JsonStringifyValue", "JsonStringifyPlainValue"] and names.index("InitializeInstanceElements") < len(names) - 2 and
            "const_:()=assert!(RuntimeHelperId::conditional_helpers_are_last());" in registry,
            "conditional helpers must remain a suffix with the compile-time assertion intact")
    work = body(hook, "has_instance_work")
    require("!plan.elements.is_empty()||!plan.private_method_brands.is_empty()" in work,
            "private brands/methods/accessors must install a hook even without fields")
    capture = body(hook, "emit_instance_elements_initializer_capture")
    require("has_instance_work(meta)" in capture,
            "allocation must base hook presence on the complete instance plan")
    selection = body(hook, "emit_initialize_super_instance_elements")
    require("Some(scriptw_ir::DirectEvalDerivedOwner::LocalFunction(owner))=>Some(owner.clone())," in selection and
            "Some(scriptw_ir::DirectEvalDerivedOwner::ExternalCaller)=>None," in selection and
            "ifletSome(owner)=local_owner{" in selection,
            "external owner must dispatch explicitly without foreign FunctionId lookup")
    active = body(hook, "emit_active_constructor_instance_elements")
    for token, reason in (
        ("self.emit_get_derived_active_function_to_locals(&active,function)?;", "initializer must read actual shared active constructor"),
        ("FunctionObjectSchema::CONTEXT", "initializer must use actual constructor context"),
        ("FunctionContextSchema::DEFINING_PROGRAM", "initializer must root actual constructor defining P"),
        ("FunctionContextSchema::INSTANCE_ELEMENTS_INITIALIZER", "initializer must invoke actual constructor hook"),
        ("self.emit_propagate_current_throw_if_needed(function);", "initializer throw completion must propagate"),
    ):
        require(token in active, reason)
    require("current_program(" not in active and "self.functions.get(" not in active,
            "external initialization cannot borrow ambient P or foreign local metadata")
    require("letdefining_program=ProgramInstanceLocals::bind(schema," in active and
            "schema.call_instance_elements_initializer(&callee,&defining_program,&context,receiver,function," in active,
            "typed call must retain the exact constructor defining P and context")
    require(all(token in active for token in ("schema.call_instance_elements_initializer(", "defining_program.clear(function);", "context.clear(function);")) and
            active.index("schema.call_instance_elements_initializer(") < active.index("defining_program.clear(function);") < active.index("context.clear(function);"),
            "constructor P/context and typed callee must remain rooted through the call")
    reference = body(sources["gc_types/value/instance_elements.rs"], "instance_elements_initializer_reference")
    require("ifpresent{" in reference and "RuntimeHelperId::InitializeInstanceElements.index(base)" in reference and
            "Instruction::RefNull(HeapType::Concrete(InitializeInstanceElementsHook::SIGNATURE.type_index(),))" in reference,
            "allocation must capture only the registered local hook or its exact typed null")
    typed = body(sources["gc_types/value/instance_elements.rs"], "call_instance_elements_initializer")
    require("defining_program.reference(self,function);constructor_context.load(self,function);receiver.emit(function);callee.load(function);function.instruction(&Instruction::CallRef(InitializeInstanceElementsHook::SIGNATURE.type_index(),));GcCallResult::emitted()" == typed,
            "typed call must preserve argument order and full Completion result")
    compile_hook = body(hook, "compile_initialize_instance_elements_hook")
    require("parameters.caller_program.load(schema,&mutfunction);" in compile_hook and
            "Instruction::RefEq" in compile_hook and "FunctionContextSchema::DEFINING_PROGRAM" in compile_hook,
            "helper must reject a mismatched P/context pair")
    require("FunctionContextSchema::ACTIVE_FUNCTION" in compile_hook and
            "read(&parameters.caller_function_context,schema,&mutfunction)" in compile_hook,
            "helper must dispatch the constructor retained by its exact context")
    require("ExecutableCodeSchema::SOURCE_ID" in compile_hook and
            "self.functions.values().filter(|meta|has_instance_work(meta))" in compile_hook and
            "Instruction::I64Const(meta.entry.source_id())" in compile_hook,
            "helper must dispatch actual immutable SOURCE_ID only against its own constructor plans")
    require(".field(ExecutableCodeSchema::SOURCE_ID).read(&code,schema,&mutfunction).store_i64(source_id,&mutfunction);" in compile_hook,
            "SOURCE_ID dispatch must use its declaration-owned I64 field-read API")
    require(compile_hook.count("Instruction::Unreachable") == 2 and
            "self.emit_branch_to_target(done,&mutfunction);" in compile_hook,
            "unknown SOURCE_ID must trap rather than complete as a silent no-op")
    allocation = body(sources["functions/function_object_allocation.rs"], "emit_complete_function_record")
    require("self.emit_instance_elements_initializer_capture(meta,function)?" in allocation and
            "program_operand,GcOperand::nullable_function_reference(&initializer)," in allocation,
            "every complete function record must install its explicit initializer slot")
    require("GcOperand::reference(&defining_program,schema),GcOperand::null_function()," in c["functions/class_captures.rs"],
            "synthetic static context must explicitly clear constructor-only hook")
    require("direct_eval_derived_constructor_owner" not in c["emit.rs"] and
            "context.derived_owner()" in c["emit.rs"], "super owner projection must preserve ExternalCaller")
    require("self.emit_initialize_super_instance_elements(result.value(),function)?;" in c["functions.rs"],
            "super must use explicit local/external initializer seam")
    require("RuntimeHelperId::InitializeInstanceElements" in c["emit/body_entry.rs"] and
            "RuntimeHelperId::NativeDirectEvalFactory" in c["emit/body_entry.rs"],
            "both program helpers must have explicit body-entry classification")
    helper_bodies = c["emit/module_assembly/program_helper_bodies.rs"]
    require("initializer.compile_initialize_instance_elements_hook()?" in helper_bodies and
            "factory.compile_native_direct_eval_factory()?" in helper_bodies and
            "program_helper_bodies::compile_program_helper_bodies(" in c["emit/module_assembly.rs"],
            "assembly must register both program helper bodies through its extracted owner")
    require(len(sources["emit/module_assembly.rs"].splitlines()) <= 1950,
            "existing module assembly line cap cannot rise")
    publication = c["function_entry/executable_code.rs"]
    require("source_id:self.source_id()," in publication and
            "fnsource_id(&self)->i64{i64::from(self.wasm_index)}" in publication,
            "publication and constructor dispatch must share one P-local source identity")
    return failures


def main() -> int:
    root = Path(sys.argv[1]) if len(sys.argv) == 2 else Path.cwd()
    sources = {name: (root / PREFIX / name).read_text() for name in FILES}
    failures = check(sources)
    for failure in failures:
        print(f"ABI8 instance elements: {failure}", file=sys.stderr)
    if not failures:
        print("ABI8 instance-elements source controls passed")
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
