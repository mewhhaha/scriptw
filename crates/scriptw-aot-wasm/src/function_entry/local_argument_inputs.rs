//! Strict source Call consumes its bounded rooted value triples without an argv.
use super::*;
use crate::gc_types::{
    ExecutableCode, ExecutableCodeSchema, FunctionLocal, FunctionObject, GcCallResult,
    GcEntryFunctionRole, GcLocal, LocalArgumentsCallable, Nullable, ProgramInstanceLocals,
    RealmRecord, RuntimeSchema, ValueLocals, WideLocalArgumentsCallable,
};

/// Each typed role owns the matching physical entry field and argument extent.
pub(crate) trait LocalArgumentsCallableRole: GcEntryFunctionRole + Sized {
    const ARITY: LocalArgumentsArity;
    fn load_optional_entry(
        code: &GcLocal<ExecutableCode>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> FunctionLocal<Self, Nullable>;
}

macro_rules! local_arguments_role {
    ($role:ident, $arity:ident, $field:ident) => {
        impl LocalArgumentsCallableRole for $role {
            const ARITY: LocalArgumentsArity = LocalArgumentsArity::$arity;
            fn load_optional_entry(
                code: &GcLocal<ExecutableCode>,
                schema: &RuntimeSchema,
                function: &mut Function,
            ) -> FunctionLocal<Self, Nullable> {
                schema
                    .reserve_function_local::<Self, Nullable>(function)
                    .initialize(
                        schema
                            .field(ExecutableCodeSchema::$field)
                            .read(code, schema, function)
                            .callable(),
                        function,
                    )
            }
        }
    };
}
local_arguments_role!(LocalArgumentsCallable, Narrow, LOCAL_ARGUMENTS_ENTRY);
local_arguments_role!(WideLocalArgumentsCallable, Wide, WIDE_LOCAL_ARGUMENTS_ENTRY);

/// The published typed entry proves strict source semantics. Missing actuals
/// are padded by the caller; no arguments object can observe the omitted argc.
pub(crate) struct LocalArgumentCallInputs<'a, R: LocalArgumentsCallableRole> {
    receiver: &'a ValueLocals,
    arguments: &'a [ValueLocals],
    padding: &'a ValueLocals,
    caller_realm: &'a GcLocal<RealmRecord>,
    calling_program: &'a ProgramInstanceLocals,
    role: std::marker::PhantomData<R>,
}

impl<'a, R: LocalArgumentsCallableRole> LocalArgumentCallInputs<'a, R> {
    pub(crate) fn call(
        receiver: &'a ValueLocals,
        arguments: &'a [ValueLocals],
        padding: &'a ValueLocals,
        caller_realm: &'a GcLocal<RealmRecord>,
        calling_program: &'a ProgramInstanceLocals,
    ) -> Self {
        assert!(
            arguments.len() <= R::ARITY.slot_count(),
            "typed argument extent validated before emission"
        );
        Self {
            receiver,
            arguments,
            padding,
            caller_realm,
            calling_program,
            role: std::marker::PhantomData,
        }
    }

    pub(crate) fn emit_call(
        &self,
        callable: &GcLocal<FunctionObject>,
        entry: &FunctionLocal<R>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcCallResult {
        self.emit(callable, schema, function);
        schema.call_reference(entry, function)
    }

    pub(crate) fn emit_return_call(
        &self,
        callable: &GcLocal<FunctionObject>,
        entry: &FunctionLocal<R>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.emit(callable, schema, function);
        entry.load(function);
        function.instruction(&Instruction::ReturnCallRef(R::SIGNATURE.type_index()));
    }

    pub(crate) fn emit_planned_call(
        &self,
        callable: &GcLocal<FunctionObject>,
        entry: &PlannedLocalArgumentsEntry,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcCallResult {
        assert_eq!(
            entry.arity(),
            R::ARITY,
            "direct local entry has the validated argument extent"
        );
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
        assert_eq!(
            entry.arity(),
            R::ARITY,
            "direct local tail entry has the validated argument extent"
        );
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
        for index in 0..R::ARITY.slot_count() {
            self.arguments
                .get(index)
                .unwrap_or(self.padding)
                .emit(function);
        }
        self.caller_realm.load(schema, function);
        self.calling_program.reference(schema, function);
    }
}
