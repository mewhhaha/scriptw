//! A saved activation's frame keeps every input rooted until its call returns.

use super::*;
use crate::gc_types::{
    AsyncActivation, AsyncActivationSchema, AsyncCallable, AsyncGeneratorActivation,
    AsyncGeneratorActivationSchema, AsyncGeneratorCallable, CompletionLocals, ExecutableCode,
    ExecutableCodeSchema, FunctionContext, FunctionContextSchema, FunctionObject,
    FunctionObjectSchema, GcLocal, GeneratorActivation, GeneratorActivationSchema,
    GeneratorCallable, InvocationFrame, InvocationFrameSchema, ProgramInstanceLocals, StoredValue,
};

macro_rules! saved_activation_call {
    ($method:ident, $activation:ident, $activation_schema:ident, $role:ident, $code_field:ident, $inputs:ident) => {
        impl FunctionBuilder<'_> {
            pub(crate) fn $method(
                &mut self,
                activation: &GcLocal<$activation>,
                result: &CompletionLocals,
                function: &mut Function,
            ) {
                let schema = self.runtime_schema();
                let frame = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<$activation>()
                        .field($activation_schema::FRAME)
                        .read(activation, schema, function)
                        .reference(),
                    function,
                );
                let callable = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<InvocationFrame>()
                        .field(InvocationFrameSchema::FUNCTION)
                        .read(&frame, schema, function)
                        .reference(),
                    function,
                );
                let executable = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<FunctionObject>()
                        .field(FunctionObjectSchema::CODE)
                        .read(&callable, schema, function)
                        .reference(),
                    function,
                );
                let context = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<FunctionObject>()
                        .field(FunctionObjectSchema::CONTEXT)
                        .read(&callable, schema, function)
                        .reference(),
                    function,
                );
                let realm = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<FunctionContext>()
                        .field(FunctionContextSchema::REALM)
                        .read(&context, schema, function)
                        .reference(),
                    function,
                );
                // Resumable roles are source entries. Their retained callable
                // already owns the defining P even after the scheduling frame
                // returned, so no ambient helper/caller owner is consulted.
                let defining_program = ProgramInstanceLocals::bind(
                    schema,
                    schema
                        .struct_type::<FunctionContext>()
                        .field(FunctionContextSchema::DEFINING_PROGRAM)
                        .read(&context, schema, function)
                        .reference()
                        .require_non_null(function),
                    function,
                );
                let caller_realm = self.load_current_realm(function);
                self.replace_current_realm(&realm, function);
                let code = schema
                    .reserve_function_local::<$role, _>(function)
                    .initialize(
                        schema
                            .struct_type::<ExecutableCode>()
                            .field(ExecutableCodeSchema::$code_field)
                            .read(&executable, schema, function)
                            .callable()
                            .require_non_null(function),
                        function,
                    );
                let stored_this = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<InvocationFrame>()
                        .field(InvocationFrameSchema::THIS_VALUE)
                        .read(&frame, schema, function)
                        .reference(),
                    function,
                );
                let this_value = schema.reserve_value_local(function);
                schema.struct_type::<StoredValue>().read_into(
                    &stored_this,
                    &this_value,
                    schema,
                    function,
                );
                let arguments = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<InvocationFrame>()
                        .field(InvocationFrameSchema::ARGUMENTS)
                        .read(&frame, schema, function)
                        .reference(),
                    function,
                );
                $inputs::new(
                    &this_value,
                    activation,
                    EntryArguments::new(&arguments),
                    &defining_program,
                )
                .emit(&callable, schema, function);
                result.store_call(schema.call_reference(&code, function), function);
                self.replace_current_realm(&caller_realm, function);
                caller_realm.clear(function);
                defining_program.clear(function);
                realm.clear(function);
                context.clear(function);
                arguments.clear(function);
                this_value.clear(function);
                stored_this.clear(function);
                code.clear(function);
                executable.clear(function);
                callable.clear(function);
                frame.clear(function);
            }
        }
    };
}
saved_activation_call!(
    emit_saved_generator_body_call,
    GeneratorActivation,
    GeneratorActivationSchema,
    GeneratorCallable,
    GENERATOR_ENTRY,
    GeneratorBodyInputs
);
saved_activation_call!(
    emit_saved_async_body_call,
    AsyncActivation,
    AsyncActivationSchema,
    AsyncCallable,
    ASYNC_ENTRY,
    AsyncBodyInputs
);
saved_activation_call!(
    emit_saved_async_generator_body_call,
    AsyncGeneratorActivation,
    AsyncGeneratorActivationSchema,
    AsyncGeneratorCallable,
    ASYNC_GENERATOR_ENTRY,
    AsyncGeneratorBodyInputs
);
