use super::*;

mod async_generator_structured_owner;
mod literal_roots;

impl<'a> FunctionBuilder<'a> {
    pub(super) fn clear_body_entry_roots(&mut self, function: &mut Function) {
        if let Some(activation) = self.source_activation.take() {
            activation.clear(function);
        }
        for binding in self.local_bindings.iter().flatten() {
            if let Some(list) = binding.private_argument_list() {
                list.set_null(self.schema, function);
            }
        }
        if let Some(context) = self.direct_eval_context.take() {
            context.clear(function);
        }
        if let Some(entry) = self.body_entry_locals.take() {
            entry.clear(self.schema, function);
        }
        self.clear_entry_program(function);
        self.current_private_environment
            .set_null(self.schema, function);
        self.current_environment.set_null(self.schema, function);
    }
    /// Starts the body of `helper` and, in the same step, switches that
    /// helper's own inline seam off for the rest of this builder's life.
    ///
    /// Consumes this registered helper's body and clears its own inline seam
    /// before any body code is emitted. The declaration, parameter owner and
    /// result authority all derive from the same exhaustive helper row.
    /// Deliberately recursive serializers retain their registered call route.
    pub(crate) fn begin_helper_body(&mut self, helper: RuntimeHelperId) -> Function {
        assert!(
            matches!(self.module_state, FunctionModuleState::RuntimeOperation(actual) if actual == helper),
            "runtime helper body must use its registered declaration"
        );
        match helper {
            RuntimeHelperId::StringEquality => self.outline_string_equality = false,
            RuntimeHelperId::NumberToString => self.outline_number_to_string = false,
            RuntimeHelperId::StringToNumber => self.outline_string_to_number = false,
            RuntimeHelperId::ValueToString => self.outline_value_to_string = false,
            RuntimeHelperId::ValueToNumber => self.outline_value_to_number = false,
            RuntimeHelperId::ValueToNumeric => self.outline_value_to_numeric = false,
            // Embedding entries are host boundaries, never an inline facade
            // of a JavaScript operation. Their declarations bind explicit
            // Realm/program operands; transfers and Promise inspection consume
            // only their declared GC values and require no callable context.
            RuntimeHelperId::EmbeddingExport
            | RuntimeHelperId::EmbeddingCall
            | RuntimeHelperId::EmbeddingRead
            | RuntimeHelperId::EmbeddingCheckpoint
            | RuntimeHelperId::EmbeddingString
            | RuntimeHelperId::EmbeddingBytes
            | RuntimeHelperId::EmbeddingBytesRead
            | RuntimeHelperId::EmbeddingPromise
            | RuntimeHelperId::EmbeddingCancel => {}
            // These storage kernels have no inline facade to disable. The
            // cached-read parameter owner binds its caller's ProgramInstance
            // and Environment before its body; allocation takes the source
            // prototype explicitly, and descriptor/storage kernels are pure.
            RuntimeHelperId::OrdinaryPropertyStorageEnsure
            | RuntimeHelperId::OrdinaryOwnDescriptor
            | RuntimeHelperId::CompactLiteralAllocate
            | RuntimeHelperId::NativeFunctionAllocate
            | RuntimeHelperId::ArgumentListAppend
            | RuntimeHelperId::CachedPropertyRead => {}
            // These bodies call their physical typed kernel directly. Their
            // public facades retain registered call boundaries for recursion.
            RuntimeHelperId::TransientByteAlloc
            | RuntimeHelperId::ArrayIndexedPublishDefaultData
            | RuntimeHelperId::ArrayIndexedPublish
            | RuntimeHelperId::ArrayIndexedDelete
            | RuntimeHelperId::ObjectRead
            | RuntimeHelperId::ObjectWrite
            | RuntimeHelperId::ObjectDefineData
            | RuntimeHelperId::OrdinaryPropertyFind
            | RuntimeHelperId::OrdinaryPropertyAppend
            | RuntimeHelperId::OrdinaryPropertyPrepare
            | RuntimeHelperId::CollectionPrepareMap
            | RuntimeHelperId::CollectionPrepareSet
            | RuntimeHelperId::CollectionNextMap
            | RuntimeHelperId::CollectionNextSet
            | RuntimeHelperId::ObjectHeaderProjection
            | RuntimeHelperId::OrdinaryObjectAllocate
            | RuntimeHelperId::ValueToObject
            | RuntimeHelperId::FunctionNameFromKey
            | RuntimeHelperId::RegExpLiteralAllocate
            | RuntimeHelperId::NativeRegExpLiteralFactory
            | RuntimeHelperId::PropertyValueResolve
            | RuntimeHelperId::FunctionMetadataPublish
            | RuntimeHelperId::RealmInitializeIntrinsics
            | RuntimeHelperId::FunctionCall
            | RuntimeHelperId::ProxyCall
            | RuntimeHelperId::ArrayIteratorStepValue
            | RuntimeHelperId::ArrayIteratorElision
            | RuntimeHelperId::ArrayIteratorClose
            | RuntimeHelperId::ArrayAccumulationStepValue
            | RuntimeHelperId::ProxyConstruct
            | RuntimeHelperId::ObjectGetPrototypeOf
            | RuntimeHelperId::ObjectSetPrototypeOf
            | RuntimeHelperId::ObjectDelete
            | RuntimeHelperId::ObjectIsExtensible
            | RuntimeHelperId::ObjectPreventExtensions
            | RuntimeHelperId::ObjectReadProxy
            | RuntimeHelperId::IndexedElementRead
            | RuntimeHelperId::IndexedElementWrite
            | RuntimeHelperId::ValueToPrimitiveDefault
            | RuntimeHelperId::ValueToPrimitiveNumber
            | RuntimeHelperId::ValueToPrimitiveString
            | RuntimeHelperId::ValueToPropertyKey
            | RuntimeHelperId::CoerciveAdd
            | RuntimeHelperId::LexicalBindingUninitialized
            | RuntimeHelperId::RelationalComparison
            | RuntimeHelperId::RegExpMatcher
            | RuntimeHelperId::RegExpCompiler
            | RuntimeHelperId::ModuleInitialize
            | RuntimeHelperId::ModuleEvaluate
            | RuntimeHelperId::ModuleReady
            | RuntimeHelperId::ModuleGather
            | RuntimeHelperId::ModuleExecute
            | RuntimeHelperId::ModuleFulfilled
            | RuntimeHelperId::ModuleRejected
            | RuntimeHelperId::ModuleDeferredImport
            | RuntimeHelperId::AsyncGeneratorStartBody
            | RuntimeHelperId::AsyncGeneratorDrainQueue
            | RuntimeHelperId::PromiseDrainJobs
            | RuntimeHelperId::AsyncAwaitReactions
            | RuntimeHelperId::AsyncGeneratorAwaitReactions
            | RuntimeHelperId::AsyncGeneratorYieldReactions
            | RuntimeHelperId::AsyncGeneratorYieldReturnReactions
            | RuntimeHelperId::AsyncGeneratorAwaitReturnReactions
            | RuntimeHelperId::DynamicPropertyRead
            | RuntimeHelperId::OrdinarySetDataOnReceiver
            | RuntimeHelperId::OrdinarySet
            | RuntimeHelperId::DecimalToBinary64
            | RuntimeHelperId::BigIntArithmetic
            | RuntimeHelperId::TemporalCalendarIsoDateProbe
            | RuntimeHelperId::TemporalCalendarIdentifier
            | RuntimeHelperId::TemporalZonedDateTimeConvert
            | RuntimeHelperId::TemporalPlainDateConvert
            | RuntimeHelperId::TemporalChineseYear
            | RuntimeHelperId::TemporalDangiYear
            | RuntimeHelperId::TemporalUmmAlQuraYear
            | RuntimeHelperId::TemporalUmmAlQuraEpoch
            | RuntimeHelperId::TemporalCalendarProjectDate
            | RuntimeHelperId::TemporalCalendarFieldsToIso
            | RuntimeHelperId::TemporalCalendarDaysInMonth
            | RuntimeHelperId::TemporalCalendarBalanceYearMonth
            | RuntimeHelperId::TemporalCalendarDifferenceDate
            | RuntimeHelperId::ObjectHasProperty
            | RuntimeHelperId::WithEnvironmentHasBinding
            | RuntimeHelperId::WithEnvironmentGetBindingValueSloppy
            | RuntimeHelperId::WithEnvironmentGetBindingValueStrict
            | RuntimeHelperId::EnvironmentIdentifierReadSloppy
            | RuntimeHelperId::EnvironmentIdentifierReadStrict
            | RuntimeHelperId::EnvironmentIdentifierReadTypeofSloppy
            | RuntimeHelperId::EnvironmentIdentifierReadTypeofStrict
            | RuntimeHelperId::EnvironmentIdentifierResolve
            | RuntimeHelperId::EnvironmentIdentifierGetSloppy
            | RuntimeHelperId::EnvironmentIdentifierGetStrict
            | RuntimeHelperId::EnvironmentIdentifierTypeofSloppy
            | RuntimeHelperId::EnvironmentIdentifierTypeofStrict
            | RuntimeHelperId::EnvironmentIdentifierPutSloppy
            | RuntimeHelperId::EnvironmentIdentifierPutStrict
            | RuntimeHelperId::GlobalIdentifierReadSloppy
            | RuntimeHelperId::GlobalIdentifierReadStrict
            | RuntimeHelperId::GlobalIdentifierTypeofSloppy
            | RuntimeHelperId::GlobalIdentifierTypeofStrict
            | RuntimeHelperId::PrivateElementAdd
            | RuntimeHelperId::PrivateFieldDefine
            | RuntimeHelperId::PreparedDynamicFunction
            | RuntimeHelperId::PreparedScript
            | RuntimeHelperId::RealmModuleImport
            | RuntimeHelperId::ModuleBodyReaction
            | RuntimeHelperId::RegExpProgramCandidate
            | RuntimeHelperId::RuntimeErrorObject
            | RuntimeHelperId::PooledStringResolve
            | RuntimeHelperId::IntegerDecimalString
            | RuntimeHelperId::PooledStringsInitialize
            | RuntimeHelperId::GlobalLexicalDeclarationCheck
            | RuntimeHelperId::GlobalVarDeclarationCheck
            | RuntimeHelperId::GlobalVarDeclarationInstall
            | RuntimeHelperId::JsonStringifyValue
            | RuntimeHelperId::CompactArrayLiteral
            | RuntimeHelperId::InitializeInstanceElements
            | RuntimeHelperId::NativeDirectEvalFactory => {}
        }
        self.take_owned_body()
    }

    // `compile_indexed_element_read_helper` and
    // `compile_indexed_element_write_helper` live in `objects.rs`, beside the
    // composites they outline, and are the only `compile_*_helper` pair that
    // does. That is not a filing preference: the bodies they emit are the two
    // largest inline expansions in the crate (72,635 and 174,558 bytes), and
    // keeping the compiler in the same module as the body is what lets the body
    // itself stay module-private with exactly two callers — its seam's fallback
    // arm and its helper compiler. A third caller elsewhere in the crate would
    // re-inline the composite and quietly undo the outlining; with the body
    // private, that call does not compile.

    pub(super) fn init_current_env(&mut self, function: &mut Function) -> Result<(), EmitError> {
        use crate::gc_types::{
            EnvironmentSchema, FunctionContext, FunctionContextSchema, InvocationFrame,
            InvocationFrameSchema, NonNullable, StoredValue,
        };
        let schema = self.runtime_schema();
        let frame = self
            .body_entry_locals()
            .and_then(|entry| entry.resume_frame())
            .map(|frame| {
                schema
                    .reserve_gc_local::<InvocationFrame, NonNullable>(function)
                    .initialize(frame.load(schema, function), function)
            });
        let initialized = frame.as_ref().map(|frame| {
            let initialized = schema.reserve_i32_local(function);
            schema
                .struct_type::<InvocationFrame>()
                .field(InvocationFrameSchema::INITIALIZED)
                .read(frame, schema, function)
                .store(initialized, function);
            initialized.load(function);
            function.instruction(&Instruction::If(BlockType::Empty));
            let environment_field = match self
                .body_entry_locals()
                .expect("resumable body owns its entry")
                .resume_activation()
                .expect("resumable body owns its activation")
            {
                crate::function_entry::ResumableEntryLocals::Generator(_)
                | crate::function_entry::ResumableEntryLocals::Async(_) => {
                    // Structured regions restore children from the saved deepest
                    // chain, starting with this invocation's original outer.
                    InvocationFrameSchema::INVOCATION_ENVIRONMENT
                }
                crate::function_entry::ResumableEntryLocals::AsyncGenerator(_) => {
                    // Linear, foreign and implicit finalizer states retain the
                    // original saved-chain entry unless the source proves this
                    // exact resume state starts at the invocation outer.
                    InvocationFrameSchema::LEXICAL_ENVIRONMENT
                }
            };
            self.replace_current_environment(
                schema
                    .struct_type::<InvocationFrame>()
                    .field(environment_field)
                    .read(frame, schema, function)
                    .reference(),
                function,
            );
            if matches!(
                self.body_entry_locals()
                    .and_then(|entry| entry.resume_activation()),
                Some(crate::function_entry::ResumableEntryLocals::AsyncGenerator(
                    _
                ))
            ) {
                if let Some(plan) = self
                    .async_generator_resume_environment_plan
                    .as_ref()
                    .filter(|plan| {
                        !plan.invocation_resume_states().is_empty()
                            || !plan.enclosing_scope_resume_states().is_empty()
                    })
                {
                    async_generator_structured_owner::emit_scope_outer_resume_test(
                        plan,
                        self.body_entry_locals()
                            .and_then(|entry| entry.resume_point())
                            .expect("a resumable entry owns its resume point"),
                        function,
                    );
                    function.instruction(&Instruction::If(BlockType::Empty));
                    self.replace_current_environment(
                        schema
                            .struct_type::<InvocationFrame>()
                            .field(InvocationFrameSchema::INVOCATION_ENVIRONMENT)
                            .read(frame, schema, function)
                            .reference(),
                        function,
                    );
                    function.instruction(&Instruction::End);
                }
            }
            function.instruction(&Instruction::Else);
            // A fresh frame has no environment fields yet. Its activation
            // must descend from the actual callable's defining environment.
            let context = schema
                .reserve_gc_local::<FunctionContext, NonNullable>(function)
                .initialize(
                    self.body_entry_locals()
                        .expect("resumable body owns its entry")
                        .function_context()
                        .expect("resumable callable owns its context")
                        .load(schema, function),
                    function,
                );
            self.replace_current_environment(
                schema
                    .struct_type::<FunctionContext>()
                    .field(FunctionContextSchema::LEXICAL_ENVIRONMENT)
                    .read(&context, schema, function)
                    .reference(),
                function,
            );
            context.clear(function);
            function.instruction(&Instruction::End);
            initialized
        });
        if self
            .captured_bindings
            .iter()
            .any(|binding| binding.name == scriptw_ir::DIRECT_EVAL_EXECUTION_CONTEXT_NAME)
        {
            let context = self
                .direct_eval_context
                .as_ref()
                .expect("captured direct-eval context owns a typed root");
            let environment = schema
                .reserve_gc_local::<Environment, Nullable>(function)
                .initialize(self.current_environment().load(schema, function), function);
            function.instruction(&Instruction::Block(BlockType::Empty));
            function.instruction(&Instruction::Loop(BlockType::Empty));
            environment.load(schema, function);
            function.instruction(&Instruction::RefIsNull);
            function.instruction(&Instruction::BrIf(1));
            context.replace(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::DIRECT_EVAL_CONTEXT)
                    .read(&environment, schema, function)
                    .reference(),
                function,
            );
            context.load(schema, function);
            function.instruction(&Instruction::RefIsNull);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::BrIf(1));
            environment.replace(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::PARENT)
                    .read(&environment, schema, function)
                    .reference(),
                function,
            );
            function.instruction(&Instruction::Br(0));
            function.instruction(&Instruction::End);
            function.instruction(&Instruction::End);
            environment.clear(function);
        }
        let has_function_body_environment = self.body.statements.iter().any(|statement| {
            matches!(statement, StatementIr::Block(body) if body.lexical_environment.as_ref().is_some_and(|environment|
                matches!(environment.initialization, scriptw_ir::LexicalEnvironmentInitializationIr::FunctionBody { .. })))
        });
        if self.owned_env_bindings.is_empty()
            && self.eval_environment.is_none()
            && !has_function_body_environment
            && frame.is_none()
        {
            return Ok(());
        }
        if let Some(initialized) = initialized {
            initialized.load(function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::If(BlockType::Empty));
        }
        if matches!(
            self.eval_environment,
            Some(scriptw_ir::EvalEnvironmentRoleIr::Declarative {
                kind: scriptw_ir::EvalDeclarativeEnvironmentKindIr::Parameters,
                ..
            })
        ) {
            self.emit_allocate_lexical_environment_record(
                &LexicalEnvironmentIr {
                    initialization: scriptw_ir::LexicalEnvironmentInitializationIr::Uninitialized,
                    eval_environment: Some(scriptw_ir::EvalEnvironmentRoleIr::Declarative {
                        kind: scriptw_ir::EvalDeclarativeEnvironmentKindIr::Variable,
                        bindings: Vec::new(),
                    }),
                    bindings: Vec::new(),
                },
                function,
            )?;
        }
        let parent = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize(self.current_environment().load(schema, function), function);
        let cells = self.emit_allocate_environment_cells(self.owned_env_bindings, function);
        let record = self.emit_initialize_named_environment_header(
            &parent,
            &cells,
            self.eval_environment,
            function,
        )?;
        self.replace_current_environment(record.load(schema, function).nullable(), function);
        if frame.is_some() {
            self.emit_prepare_resumable_function_body_environment(function)?;
        }
        if self.function_flavor == FunctionFlavor::Ordinary && !self.is_main() {
            if self.is_derived_constructor {
                let activation = self
                    .lexical_derived_activation
                    .expect("derived constructor must have activation metadata");
                self.initialize_derived_activation(activation, function)?;
            }
            let value = schema.reserve_value_local(function);
            if let Some(slot) = self.owned_env_slot(LEXICAL_THIS_NAME) {
                value.copy_from(
                    self.body_entry_locals()
                        .expect("callable owns its entry")
                        .this_value(),
                    function,
                );
                self.write_env_slot_from_locals(slot, 0, &value, function);
            }
            if let Some(slot) = self.owned_env_slot(LEXICAL_NEW_TARGET_NAME) {
                value.copy_from(
                    self.body_entry_locals()
                        .expect("callable owns its entry")
                        .new_target(),
                    function,
                );
                self.write_env_slot_from_locals(slot, 0, &value, function);
            }
            if let Some(slot) = self.owned_env_slot(LEXICAL_HOME_OBJECT_NAME) {
                let context = schema
                    .reserve_gc_local::<FunctionContext, NonNullable>(function)
                    .initialize(
                        self.body_entry_locals()
                            .expect("callable owns its entry")
                            .function_context()
                            .expect("source callable owns its context")
                            .load(schema, function),
                        function,
                    );
                let stored = schema
                    .reserve_gc_local::<StoredValue, NonNullable>(function)
                    .initialize(
                        schema
                            .struct_type::<FunctionContext>()
                            .field(FunctionContextSchema::HOME_OBJECT)
                            .read(&context, schema, function)
                            .reference(),
                        function,
                    );
                schema
                    .struct_type::<StoredValue>()
                    .read_into(&stored, &value, schema, function);
                stored.clear(function);
                context.clear(function);
                self.write_env_slot_from_locals(slot, 0, &value, function);
            }
            value.clear(function);
        }
        if let Some(frame) = &frame {
            for field in [
                InvocationFrameSchema::ENVIRONMENT,
                InvocationFrameSchema::INVOCATION_ENVIRONMENT,
                InvocationFrameSchema::LEXICAL_ENVIRONMENT,
            ] {
                schema.struct_type::<InvocationFrame>().field(field).write(
                    frame,
                    crate::gc_types::GcOperand::reference(self.current_environment(), schema),
                    schema,
                    function,
                );
            }
            function.instruction(&Instruction::End);
        }
        if let Some(initialized) = initialized {
            schema.release_i32_local(initialized, function);
        }
        record.clear(function);
        cells.clear(function);
        parent.clear(function);
        if let Some(frame) = frame {
            frame.clear(function);
        }
        Ok(())
    }

    /// Derived-constructor state belongs to this invocation's actual cells;
    /// immutable function context never stores a recursive invocation's `this`.
    fn initialize_derived_activation(
        &mut self,
        activation: &DerivedConstructorActivationIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let slot = |name: &str| {
            self.owned_env_slot(name).ok_or_else(|| {
                EmitError::unsupported(format!(
                    "derived constructor activation has no `{name}` cell"
                ))
            })
        };
        let this = slot(&activation.this_binding)?;
        let status = slot(&activation.this_status_binding)?;
        let new_target = slot(&activation.new_target_binding)?;
        let active_function = slot(&activation.active_function_binding)?;
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        value.set_undefined(function);
        self.write_env_slot_from_locals(this, 0, &value, function);
        value.set_scalar(crate::gc_types::ScalarValue::Boolean(false), function);
        self.write_env_slot_from_locals(status, 0, &value, function);
        value.copy_from(
            self.body_entry_locals()
                .expect("derived constructor owns an entry")
                .new_target(),
            function,
        );
        self.write_env_slot_from_locals(new_target, 0, &value, function);
        value.set_reference(
            self.body_entry_locals()
                .expect("derived constructor owns an entry")
                .function_object()
                .expect("derived constructor owns its function"),
            schema,
            function,
        );
        self.write_env_slot_from_locals(active_function, 0, &value, function);
        value.clear(function);
        Ok(())
    }
}
