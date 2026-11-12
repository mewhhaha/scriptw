//! Semantic call arguments. Count is always derived from the rooted vector.

use super::runtime_dispatch::NativeCallerBoundary;
use super::*;
use crate::gc_types::{
    AsyncActivation, AsyncGeneratorActivation, DirectEvalExecutionContext, Environment,
    FunctionObject, GcCallResult, GcLocal, GcNullability, GeneratorActivation, Nullable,
    PrivateEnvironment, ProgramInstanceLocals, RealmRecord, RuntimeSchema, ValueArray, ValueLocals,
};

pub(crate) struct EntryArguments<'a>(&'a GcLocal<ValueArray>);

impl<'a> EntryArguments<'a> {
    pub(crate) fn new(vector: &'a GcLocal<ValueArray>) -> Self {
        Self(vector)
    }

    pub(super) fn emit(&self, schema: &RuntimeSchema, function: &mut Function) {
        schema
            .array_type::<ValueArray>()
            .length(self.0, schema, function);
        function.instruction(&Instruction::I64ExtendI32U);
        self.0.load(schema, function);
    }
}

enum NewTargetInput<'a> {
    Call,
    Construct(&'a ValueLocals),
}

pub(crate) struct OrdinaryBodyInputs<'a> {
    this_value: &'a ValueLocals,
    new_target: NewTargetInput<'a>,
    arguments: EntryArguments<'a>,
    caller_realm: &'a GcLocal<RealmRecord>,
    calling_program: &'a ProgramInstanceLocals,
}

impl<'a> OrdinaryBodyInputs<'a> {
    pub(crate) fn call(
        this_value: &'a ValueLocals,
        arguments: EntryArguments<'a>,
        caller_realm: &'a GcLocal<RealmRecord>,
        calling_program: &'a ProgramInstanceLocals,
    ) -> Self {
        Self {
            this_value,
            new_target: NewTargetInput::Call,
            arguments,
            caller_realm,
            calling_program,
        }
    }

    pub(crate) fn construct(
        this_value: &'a ValueLocals,
        new_target: &'a ValueLocals,
        arguments: EntryArguments<'a>,
        caller_realm: &'a GcLocal<RealmRecord>,
        calling_program: &'a ProgramInstanceLocals,
    ) -> Self {
        Self {
            this_value,
            new_target: NewTargetInput::Construct(new_target),
            arguments,
            caller_realm,
            calling_program,
        }
    }

    pub(super) fn emit(
        &self,
        callable: &GcLocal<FunctionObject>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        callable.load(schema, function);
        self.this_value.emit(function);
        match &self.new_target {
            NewTargetInput::Call => {
                function.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
                function.instruction(&Instruction::I64Const(0));
                function.instruction(&Instruction::RefNull(wasm_encoder::HeapType::Abstract {
                    shared: false,
                    ty: wasm_encoder::AbstractHeapType::Eq,
                }));
            }
            NewTargetInput::Construct(value) => value.emit(function),
        }
        self.arguments.emit(schema, function);
        self.caller_realm.load(schema, function);
        self.calling_program.reference(schema, function);
    }
}

macro_rules! resumable_inputs {
    ($inputs:ident, $activation:ident) => {
        pub(crate) struct $inputs<'a> {
            this_value: &'a ValueLocals,
            activation: &'a GcLocal<$activation>,
            arguments: EntryArguments<'a>,
            calling_program: &'a ProgramInstanceLocals,
        }
        impl<'a> $inputs<'a> {
            pub(crate) fn new(
                this_value: &'a ValueLocals,
                activation: &'a GcLocal<$activation>,
                arguments: EntryArguments<'a>,
                calling_program: &'a ProgramInstanceLocals,
            ) -> Self {
                Self {
                    this_value,
                    activation,
                    arguments,
                    calling_program,
                }
            }

            pub(super) fn emit(
                &self,
                callable: &GcLocal<FunctionObject>,
                schema: &RuntimeSchema,
                function: &mut Function,
            ) {
                callable.load(schema, function);
                self.this_value.emit(function);
                self.activation.load(schema, function);
                self.arguments.emit(schema, function);
                self.calling_program.reference(schema, function);
            }
        }
    };
}
resumable_inputs!(GeneratorBodyInputs, GeneratorActivation);
resumable_inputs!(AsyncBodyInputs, AsyncActivation);
resumable_inputs!(AsyncGeneratorBodyInputs, AsyncGeneratorActivation);

/// Script execution has environments, rather than a FunctionObject receiver.
pub(crate) struct PreparedScriptInputs<'a> {
    lexical_environment: &'a GcLocal<Environment, Nullable>,
    this_value: &'a ValueLocals,
    new_target: &'a ValueLocals,
    variable_environment: &'a GcLocal<Environment, Nullable>,
    private_environment: &'a GcLocal<PrivateEnvironment, Nullable>,
    direct_eval_context: &'a GcLocal<DirectEvalExecutionContext, Nullable>,
    executing_program: &'a ProgramInstanceLocals,
}

impl<'a> PreparedScriptInputs<'a> {
    pub(crate) fn new(
        lexical_environment: &'a GcLocal<Environment, Nullable>,
        this_value: &'a ValueLocals,
        new_target: &'a ValueLocals,
        variable_environment: &'a GcLocal<Environment, Nullable>,
        private_environment: &'a GcLocal<PrivateEnvironment, Nullable>,
        direct_eval_context: &'a GcLocal<DirectEvalExecutionContext, Nullable>,
        executing_program: &'a ProgramInstanceLocals,
    ) -> Self {
        Self {
            lexical_environment,
            this_value,
            new_target,
            variable_environment,
            private_environment,
            direct_eval_context,
            executing_program,
        }
    }

    fn emit(&self, schema: &RuntimeSchema, function: &mut Function) {
        self.lexical_environment.load(schema, function);
        self.this_value.emit(function);
        self.new_target.emit(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::RefNull(
            schema
                .reference_type::<ValueArray>(GcNullability::Nullable)
                .heap_type,
        ));
        self.variable_environment.load(schema, function);
        self.private_environment.load(schema, function);
        self.direct_eval_context.load(schema, function);
        self.executing_program.reference(schema, function);
    }
}

macro_rules! planned_callable_call {
    ($entry:ident, $inputs:ident $(; $boundary:ident)?) => {
        impl $entry<'_> {
            pub(crate) fn emit_call(
                &self,
                callable: &GcLocal<FunctionObject>,
                inputs: $inputs<'_>,
                schema: &RuntimeSchema,
                function: &mut Function,
            ) -> GcCallResult {
                $(let boundary = $boundary::enter(callable, schema, function);)?
                inputs.emit(callable, schema, function);
                let completion = schema.call_entry(self.0, function);
                $($boundary::restore(boundary, schema, function);)?
                completion
            }
        }
    };
}
planned_callable_call!(OrdinaryBodyEntry, OrdinaryBodyInputs; NativeCallerBoundary);

impl OrdinaryBodyEntry<'_> {
    /// A planned source body is never an opaque native caller. Like general
    /// dispatch, which tail-enters it, the call installs no native boundary.
    pub(crate) fn emit_source_call(
        &self,
        callable: &GcLocal<FunctionObject>,
        inputs: OrdinaryBodyInputs<'_>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcCallResult {
        assert!(
            self.0.has_source_activation(),
            "only a source body enters without a native caller boundary"
        );
        inputs.emit(callable, schema, function);
        schema.call_entry(self.0, function)
    }

    pub(crate) fn emit_source_return_call(
        &self,
        callable: &GcLocal<FunctionObject>,
        inputs: OrdinaryBodyInputs<'_>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        assert!(
            self.0.has_source_activation(),
            "only a source body enters without a native caller boundary"
        );
        inputs.emit(callable, schema, function);
        self.0.emit_direct_return_call_instruction(function);
    }
}
planned_callable_call!(GeneratorBodyEntry, GeneratorBodyInputs);
planned_callable_call!(AsyncBodyEntry, AsyncBodyInputs);
planned_callable_call!(AsyncGeneratorBodyEntry, AsyncGeneratorBodyInputs);

impl PreparedScriptEntry<'_> {
    pub(crate) fn emit_call(
        &self,
        inputs: PreparedScriptInputs<'_>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcCallResult {
        inputs.emit(schema, function);
        schema.call_entry(self.0, function)
    }
}
