//! The concrete callable owns its typed executable references.

use super::*;
use crate::gc_types::{
    AsyncCallable, AsyncGeneratorCallable, ExecutableCode, ExecutableCodeSchema, FunctionLocal,
    FunctionObject, FunctionObjectSchema, GcCallResult, GcI32Constant, GcLocal, GeneratorCallable,
    Nullable, OrdinaryCallable, RuntimeSchema,
};

pub(crate) struct RuntimeFunctionEntryDispatch {
    callable: GcLocal<FunctionObject>,
    code: GcLocal<ExecutableCode>,
}

macro_rules! runtime_entry {
    ($entry:ident, $role:ident, $inputs:ident $(; $boundary:ident)?) => {
        pub(crate) struct $entry {
            callable: GcLocal<FunctionObject>,
            code: FunctionLocal<$role>,
        }
        impl $entry {
            pub(crate) fn function_object(&self) -> &GcLocal<FunctionObject> {
                &self.callable
            }
            pub(crate) fn emit_call(
                &self,
                inputs: $inputs<'_>,
                schema: &RuntimeSchema,
                function: &mut Function,
            ) -> GcCallResult {
                $(let boundary = $boundary::enter(&self.callable, schema, function);)?
                inputs.emit(&self.callable, schema, function);
                let completion = schema.call_reference(&self.code, function);
                $($boundary::restore(boundary, schema, function);)?
                completion
            }
            fn clear(self, function: &mut Function) {
                self.code.clear(function);
                self.callable.clear(function);
            }
        }
    };
}
runtime_entry!(
    RuntimeOrdinaryBodyEntry,
    OrdinaryCallable,
    OrdinaryBodyInputs; NativeCallerBoundary
);
runtime_entry!(
    RuntimeGeneratorBodyEntry,
    GeneratorCallable,
    GeneratorBodyInputs
);
runtime_entry!(RuntimeAsyncBodyEntry, AsyncCallable, AsyncBodyInputs);
runtime_entry!(
    RuntimeAsyncGeneratorBodyEntry,
    AsyncGeneratorCallable,
    AsyncGeneratorBodyInputs
);

impl RuntimeOrdinaryBodyEntry {
    pub(crate) fn emit_return_call(
        &self,
        inputs: OrdinaryBodyInputs<'_>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        emit_opaque_native_caller(&self.callable, schema, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        // A native body can return abruptly at any internal leaf. Keep its
        // caller boundary around that whole body, then forward the entire
        // completion; no per-builtin return hook can omit a leaf.
        let boundary = NativeCallerBoundary::enter(&self.callable, schema, function);
        inputs.emit(&self.callable, schema, function);
        let completion = schema.reserve_completion(function);
        completion.store_call(schema.call_reference(&self.code, function), function);
        boundary.restore(schema, function);
        completion.emit(function);
        completion.clear(function);
        function.instruction(&Instruction::Return);
        function.instruction(&Instruction::End);
        inputs.emit(&self.callable, schema, function);
        schema.return_call_ordinary_reference(&self.code, function);
    }
}

/// Native callbacks are opaque callers. Only actual forwarding intrinsics
/// are transparent; changing a source binding or a public method cannot grant
/// another callable that intrinsic's identity. A JavaScript callee is never
/// opaque here: it installs its own source activation.
pub(super) fn emit_opaque_native_caller(
    callable: &GcLocal<FunctionObject>,
    schema: &RuntimeSchema,
    function: &mut Function,
) {
    let code = emit_callable_code(callable, schema, function);
    emit_native_code_i32(&code, schema, function);
    function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    emit_opaque_native_code_i32(&code, schema, function);
    function.instruction(&Instruction::Else);
    function.instruction(&Instruction::I32Const(0));
    function.instruction(&Instruction::End);
    code.clear(function);
}

fn emit_callable_code(
    callable: &GcLocal<FunctionObject>,
    schema: &RuntimeSchema,
    function: &mut Function,
) -> GcLocal<ExecutableCode> {
    schema.reserve_gc_local(function).initialize(
        schema
            .struct_type::<FunctionObject>()
            .field(FunctionObjectSchema::CODE)
            .read(callable, schema, function)
            .reference(),
        function,
    )
}

/// Whether the code is a native (standard or host) builtin rather than a
/// JavaScript source body.
fn emit_native_code_i32(
    code: &GcLocal<ExecutableCode>,
    schema: &RuntimeSchema,
    function: &mut Function,
) {
    schema
        .struct_type::<ExecutableCode>()
        .field(ExecutableCodeSchema::KIND)
        .read(code, schema, function);
    function.instruction(&Instruction::I32Const(
        crate::gc_types::ExecutableCodeKind::JavaScript.encode(),
    ));
    function.instruction(&Instruction::I32Ne);
}

/// For native code: whether it is not one of the forwarding intrinsics.
fn emit_opaque_native_code_i32(
    code: &GcLocal<ExecutableCode>,
    schema: &RuntimeSchema,
    function: &mut Function,
) {
    let builtin = schema.reserve_i32_local(function);
    schema
        .struct_type::<ExecutableCode>()
        .field(ExecutableCodeSchema::STANDARD_BUILTIN)
        .read(code, schema, function)
        .store(builtin, function);
    function.instruction(&Instruction::I32Const(1));
    for forwarding in [
        StandardBuiltinId::FunctionPrototypeCall,
        StandardBuiltinId::FunctionPrototypeApply,
        StandardBuiltinId::ReflectApply,
        StandardBuiltinId::EvalFunction,
    ] {
        builtin.load(function);
        function.instruction(&Instruction::I32Const(forwarding.encode()));
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::I32And);
    }
    schema.release_i32_local(builtin, function);
}

/// The caller half of a native invocation's legacy-caller barrier.
///
/// Only a native callee needs it. A JavaScript callee (ExecutableCodeKind::
/// JavaScript, exactly the planned User entries) installs its own source
/// activation, which saves the active source function at entry and restores
/// it on every body exit (`SourceActivation`); the proper-tail-call routes
/// already rely on that and reach such a callee with no boundary. A
/// caller-side save/restore around it would rewrite the value it already
/// holds, so it is emitted only for native code: an opaque native publishes
/// itself as the active function, and a forwarding intrinsic keeps the
/// caller's while its target runs.
pub(super) struct NativeCallerBoundary {
    native: crate::gc_types::I32Local,
    previous: GcLocal<FunctionObject, Nullable>,
}

impl NativeCallerBoundary {
    pub(super) fn enter(
        callable: &GcLocal<FunctionObject>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> Self {
        let previous = schema
            .reserve_gc_local::<FunctionObject, Nullable>(function)
            .initialize_null(schema, function);
        let native = schema.reserve_i32_local(function);
        let code = emit_callable_code(callable, schema, function);
        emit_native_code_i32(&code, schema, function);
        native.store(function);
        native.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        previous.replace(schema.load_active_source_function(function), function);
        emit_opaque_native_code_i32(&code, schema, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema.replace_active_source_function(callable, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        code.clear(function);
        Self { native, previous }
    }

    pub(super) fn restore(self, schema: &RuntimeSchema, function: &mut Function) {
        self.native.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema.replace_active_source_function(&self.previous, function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(self.native, function);
        self.previous.clear(function);
    }
}

impl RuntimeFunctionEntryDispatch {
    /// Called only after ValueLocals has provided a concrete FunctionObject.
    pub(crate) fn from_function(
        callable: &GcLocal<FunctionObject>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> Self {
        let callable = schema
            .reserve_gc_local(function)
            .initialize(callable.load(schema, function), function);
        let code = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionObject>()
                .field(FunctionObjectSchema::CODE)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        Self { callable, code }
    }

    pub(crate) fn release(self, function: &mut Function) {
        self.code.clear(function);
        self.callable.clear(function);
    }
}

macro_rules! runtime_family {
    ($method:ident, $entry:ident, $role:ident, $field:ident) => {
        impl RuntimeFunctionEntryDispatch {
            pub(crate) fn $method(
                &self,
                emitter: &mut FunctionBuilder<'_>,
                function: &mut Function,
                emit: impl FnOnce(
                    &$entry,
                    &mut FunctionBuilder<'_>,
                    &mut Function,
                ) -> Result<(), EmitError>,
            ) -> Result<(), EmitError> {
                let schema = emitter.runtime_schema();
                // The nullable code field is the branch predicate. A reference
                // becomes non-null only within the branch that proved it exists.
                // Each typed field read is a host lookup; read it exactly once.
                let optional = schema
                    .reserve_function_local::<$role, Nullable>(function)
                    .initialize(
                        schema
                            .struct_type::<ExecutableCode>()
                            .field(ExecutableCodeSchema::$field)
                            .read(&self.code, schema, function)
                            .callable(),
                        function,
                    );
                optional.load(function).is_null(function);
                function.instruction(&Instruction::I32Eqz);
                emitter.open_frame(ControlFrameKind::If, function);
                let code = schema
                    .reserve_function_local::<$role, _>(function)
                    .initialize(optional.load(function).require_non_null(function), function);
                let callable = schema
                    .reserve_gc_local(function)
                    .initialize(self.callable.load(schema, function), function);
                let entry = $entry { callable, code };
                emit(&entry, emitter, function)?;
                entry.clear(function);
                emitter.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                optional.clear(function);
                Ok(())
            }
        }
    };
}
runtime_family!(
    ordinary,
    RuntimeOrdinaryBodyEntry,
    OrdinaryCallable,
    ORDINARY_ENTRY
);
runtime_family!(
    generator,
    RuntimeGeneratorBodyEntry,
    GeneratorCallable,
    GENERATOR_ENTRY
);
runtime_family!(
    asynchronous,
    RuntimeAsyncBodyEntry,
    AsyncCallable,
    ASYNC_ENTRY
);
runtime_family!(
    async_generator,
    RuntimeAsyncGeneratorBodyEntry,
    AsyncGeneratorCallable,
    ASYNC_GENERATOR_ENTRY
);
