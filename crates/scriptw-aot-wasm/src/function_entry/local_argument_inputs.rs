//! A strict source Call consumes two rooted value triples without an argv.
use super::*;
use crate::gc_types::{
    FunctionLocal, FunctionObject, GcCallResult, GcFunctionRole, GcLocal, LocalArgumentsCallable,
    ProgramInstanceLocals, RealmRecord, RuntimeSchema, ValueLocals,
};

/// The published typed entry proves strict source semantics. Missing actuals
/// are padded by the caller; no arguments object can observe the omitted argc.
pub(crate) struct LocalArgumentCallInputs<'a> {
    receiver: &'a ValueLocals,
    arguments: [&'a ValueLocals; 2],
    caller_realm: &'a GcLocal<RealmRecord>,
    calling_program: &'a ProgramInstanceLocals,
}

impl<'a> LocalArgumentCallInputs<'a> {
    pub(crate) fn call(
        receiver: &'a ValueLocals,
        arguments: [&'a ValueLocals; 2],
        caller_realm: &'a GcLocal<RealmRecord>,
        calling_program: &'a ProgramInstanceLocals,
    ) -> Self {
        Self {
            receiver,
            arguments,
            caller_realm,
            calling_program,
        }
    }

    pub(crate) fn emit_call(
        &self,
        callable: &GcLocal<FunctionObject>,
        entry: &FunctionLocal<LocalArgumentsCallable>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcCallResult {
        self.emit(callable, schema, function);
        schema.call_reference(entry, function)
    }

    pub(crate) fn emit_return_call(
        &self,
        callable: &GcLocal<FunctionObject>,
        entry: &FunctionLocal<LocalArgumentsCallable>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.emit(callable, schema, function);
        entry.load(function);
        function.instruction(&Instruction::ReturnCallRef(
            LocalArgumentsCallable::SIGNATURE.type_index(),
        ));
    }

    pub(crate) fn emit_planned_call(
        &self,
        callable: &GcLocal<FunctionObject>,
        entry: &PlannedLocalArgumentsEntry,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcCallResult {
        self.emit(callable, schema, function);
        schema.call_local_arguments_entry(entry, function)
    }

    pub(crate) fn emit_planned_return_call(
        &self,
        callable: &GcLocal<FunctionObject>,
        entry: &PlannedLocalArgumentsEntry,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.emit(callable, schema, function);
        entry.emit_return_call_instruction(function);
    }

    fn emit(
        &self,
        callable: &GcLocal<FunctionObject>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        callable.load(schema, function);
        self.receiver.emit(function);
        function.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::RefNull(wasm_encoder::HeapType::Abstract {
            shared: false,
            ty: wasm_encoder::AbstractHeapType::Eq,
        }));
        for argument in self.arguments {
            argument.emit(function);
        }
        self.caller_realm.load(schema, function);
        self.calling_program.reference(schema, function);
    }
}
