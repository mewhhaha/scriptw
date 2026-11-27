//! A source call site with one known source target enters that ordinary body
//! by index once the actual callee is proven to be exactly that function.
//!
//! The guard reads only immutable identity fields, never a typed entry
//! reference. Every other callee, and a sloppy target whose receiver would
//! need ToObject, keeps the general Call helper, which enters a plain
//! FunctionObject itself; no effect precedes the choice.
use super::*;
use crate::function_entry::{EntryArguments, OrdinaryBodyInputs, PlannedBodyEntry};
use crate::gc_types::{
    CompletionLocals, ExecutableCode, ExecutableCodeKind, ExecutableCodeSchema,
    FunctionContextSchema, FunctionObject, FunctionObjectSchema, GcI32Constant, GcLocal,
    GcNullability, I32Local, ValueArray, ValueLocals,
};

/// A known source target whose ordinary body this site may call by index.
/// Its compile-time protocol already excludes the class-constructor error.
struct PlannedSourceCall<'a> {
    meta: &'a WasmFunctionMeta,
    /// OrdinaryCallBindThis would convert a non-object receiver.
    converts_receiver: bool,
}

impl<'a> PlannedSourceCall<'a> {
    fn admit(meta: &'a WasmFunctionMeta) -> Option<Self> {
        let protocol = meta.protocol();
        if !meta.entry.has_source_activation()
            || protocol == FunctionProtocolIr::ClassConstructor
            || !matches!(meta.entry.body(), PlannedBodyEntry::Ordinary(_))
        {
            return None;
        }
        Some(Self {
            meta,
            converts_receiver: !meta.strict && protocol != FunctionProtocolIr::Arrow,
        })
    }
}

/// A continuing call stores the whole completion and records entry; a
/// proper tail call leaves this body.
#[derive(Clone, Copy)]
enum SiteCallRoute<'a> {
    Continue {
        result: &'a CompletionLocals,
        entered: I32Local,
    },
    Return,
}

/// How an evaluated argument vector reaches its callee.
#[derive(Clone, Copy)]
pub(super) enum ArgvDispatch<'a> {
    /// A guarded leaf or packed fast path already rejected this callee.
    General,
    /// A source call site, with its optional single known target.
    Site(Option<&'a WasmFunctionMeta>),
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_dispatched_call_with_argv(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        dispatch: ArgvDispatch<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match dispatch {
            ArgvDispatch::General => self.emit_function_or_proxy_call_with_argv(
                callee, receiver, arguments, result, function,
            ),
            ArgvDispatch::Site(known_target) => self.emit_site_call_with_argv(
                callee,
                receiver,
                arguments,
                known_target,
                result,
                function,
            ),
        }
    }

    pub(super) fn emit_dispatched_tail_call_with_argv(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        dispatch: ArgvDispatch<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match dispatch {
            ArgvDispatch::General => {
                self.emit_prepared_tail_call(callee, receiver, arguments, function)
            }
            ArgvDispatch::Site(known_target) => self.emit_site_tail_call_with_argv(
                callee,
                receiver,
                arguments,
                known_target,
                function,
            ),
        }
    }

    /// Call(F, thisArgument, argumentsList) after every operand is evaluated.
    /// Only Throw or the value leaves this private scratch result, so an
    /// entered body's Return kind is not rewritten to Normal here.
    pub(super) fn emit_site_call_with_argv(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        known_target: Option<&WasmFunctionMeta>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let Some(planned) = known_target.and_then(PlannedSourceCall::admit) else {
            return self.emit_function_or_proxy_call_with_argv(
                callee, receiver, arguments, result, function,
            );
        };
        let schema = self.runtime_schema();
        let entered = schema.reserve_i32_local(function);
        entered.set_constant(0, function);
        self.emit_planned_source_entry(
            callee,
            receiver,
            arguments,
            &planned,
            SiteCallRoute::Continue { result, entered },
            function,
        )?;
        entered.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_function_or_proxy_call_with_argv(callee, receiver, arguments, result, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(entered, function);
        Ok(())
    }

    /// The proper tail call form: the entered branch leaves this body.
    pub(super) fn emit_site_tail_call_with_argv(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        known_target: Option<&WasmFunctionMeta>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if let Some(planned) = known_target.and_then(PlannedSourceCall::admit) {
            self.emit_planned_source_entry(
                callee,
                receiver,
                arguments,
                &planned,
                SiteCallRoute::Return,
                function,
            )?;
        }
        self.emit_prepared_tail_call(callee, receiver, arguments, function)
    }

    fn emit_planned_source_entry(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        planned: &PlannedSourceCall<'_>,
        route: SiteCallRoute<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        callee.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Function.tag()));
        function.instruction(&Instruction::I32Eq);
        callee.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<FunctionObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        let callable = schema.reserve_gc_local(function).initialize(
            callee.cast_reference::<FunctionObject>(schema, function),
            function,
        );
        let code = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CODE)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        self.emit_source_code_identity_test(planned.meta, &callable, &code, function)?;
        code.clear(function);
        if planned.converts_receiver {
            // ToObject(object) is identity; undefined/null become the callee
            // Realm's globalThis inline. Other primitives keep the general helper.
            self.emit_object_value_i32(receiver, function);
            receiver.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32Or);
            receiver.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32Or);
            function.instruction(&Instruction::I32And);
        }
        self.open_frame(ControlFrameKind::If, function);
        let PlannedBodyEntry::Ordinary(entry) = planned.meta.entry.body() else {
            unreachable!("planned source call admits only an ordinary body");
        };
        // PrepareForOrdinaryCall's Realm switch around the entered body.
        let context = schema
            .field(FunctionObjectSchema::CONTEXT)
            .read(&callable, schema, function)
            .reference();
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionContextSchema::REALM)
                .read_top(context, function)
                .reference(),
            function,
        );
        let bound_receiver = planned.converts_receiver.then(|| {
            let bound = schema.reserve_value_local(function);
            receiver.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
            function.instruction(&Instruction::I32Eq);
            receiver.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32Or);
            function.instruction(&Instruction::If(BlockType::Empty));
            let global_this = schema
                .reserve_gc_local::<crate::gc_types::StoredValue, crate::gc_types::Nullable>(
                    function,
                )
                .initialize(
                    schema
                        .struct_type::<crate::gc_types::RealmRecord>()
                        .field(crate::gc_types::RealmRecordSchema::GLOBAL_THIS)
                        .read(&realm, schema, function)
                        .reference(),
                    function,
                );
            global_this.load(schema, function).is_null(function);
            function.instruction(&Instruction::If(BlockType::Empty));
            bound.copy_from(receiver, function);
            function.instruction(&Instruction::Else);
            let present = schema.reserve_gc_local(function).initialize(
                global_this
                    .load(schema, function)
                    .require_non_null(function),
                function,
            );
            schema
                .struct_type::<crate::gc_types::StoredValue>()
                .read_into(&present, &bound, schema, function);
            present.clear(function);
            function.instruction(&Instruction::End);
            global_this.clear(function);
            function.instruction(&Instruction::Else);
            bound.copy_from(receiver, function);
            function.instruction(&Instruction::End);
            bound
        });
        let effective_receiver = bound_receiver.as_ref().unwrap_or(receiver);
        let caller_realm = self.load_current_realm(function);
        match route {
            SiteCallRoute::Continue { result, entered } => {
                entered.set_constant(1, function);
                self.replace_current_realm(&realm, function);
                result.store_call(
                    entry.emit_source_call(
                        &callable,
                        OrdinaryBodyInputs::call(
                            effective_receiver,
                            EntryArguments::new(arguments),
                            &caller_realm,
                            self.current_program(),
                        ),
                        schema,
                        function,
                    ),
                    function,
                );
                self.replace_current_realm(&caller_realm, function);
            }
            SiteCallRoute::Return => {
                // The P owner and Realm survive lexical unwind. Operand work
                // and the callee guard precede caller retirement; the caller
                // of this body restores its own Realm.
                let calling_program = self.capture_current_program(function);
                self.emit_retire_tail_call_activation(function);
                self.replace_current_realm(&realm, function);
                entry.emit_source_return_call(
                    &callable,
                    OrdinaryBodyInputs::call(
                        effective_receiver,
                        EntryArguments::new(arguments),
                        &caller_realm,
                        &calling_program,
                    ),
                    schema,
                    function,
                );
                calling_program.clear(function);
            }
        }
        caller_realm.clear(function);
        if let Some(bound) = bound_receiver {
            bound.clear(function);
        }
        realm.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        callable.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// Emits the identity predicate shared by planned local and vector calls.
    /// The checked executable still enters with this actual closure's context.
    pub(super) fn emit_source_code_identity_test(
        &mut self,
        meta: &WasmFunctionMeta,
        callable: &GcLocal<FunctionObject>,
        code: &GcLocal<ExecutableCode>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        if let Some(cached_global) = self.cached_executable_code_global(meta)? {
            // The P-private immutable code object names its source and P.
            code.load(schema, function);
            cached_global.load(schema, function);
            function.instruction(&Instruction::RefEq);
        } else {
            // Source IDs can collide across P or with native code.
            schema
                .field(ExecutableCodeSchema::SOURCE_ID)
                .read(code, schema, function);
            function.instruction(&Instruction::I64Const(meta.entry.source_id()));
            function.instruction(&Instruction::I64Eq);
            schema
                .field(ExecutableCodeSchema::KIND)
                .read(code, schema, function);
            function.instruction(&Instruction::I32Const(
                ExecutableCodeKind::JavaScript.encode(),
            ));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32And);
            let context = schema
                .field(FunctionObjectSchema::CONTEXT)
                .read(callable, schema, function)
                .reference();
            schema
                .field(FunctionContextSchema::DEFINING_PROGRAM)
                .read_top(context, function);
            self.current_program().reference(schema, function);
            function.instruction(&Instruction::RefEq);
            function.instruction(&Instruction::I32And);
        }
        Ok(())
    }
}
