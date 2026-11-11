//! Registered module helpers borrow real records and return their declared domains.

use super::*;

#[derive(Clone, Copy)]
pub(crate) enum ModuleRuntimeOperation {
    Initialize,
    Evaluate,
    Ready,
    Gather,
    Execute,
    Fulfilled,
    Rejected,
    DeferredImport,
}
impl ModuleRuntimeOperation {
    pub(crate) const ALL: [Self; 8] = [
        Self::Initialize,
        Self::Evaluate,
        Self::Ready,
        Self::Gather,
        Self::Execute,
        Self::Fulfilled,
        Self::Rejected,
        Self::DeferredImport,
    ];
    pub(crate) const fn helper(self) -> RuntimeHelperId {
        match self {
            Self::Initialize => RuntimeHelperId::ModuleInitialize,
            Self::Evaluate => RuntimeHelperId::ModuleEvaluate,
            Self::Ready => RuntimeHelperId::ModuleReady,
            Self::Gather => RuntimeHelperId::ModuleGather,
            Self::Execute => RuntimeHelperId::ModuleExecute,
            Self::Fulfilled => RuntimeHelperId::ModuleFulfilled,
            Self::Rejected => RuntimeHelperId::ModuleRejected,
            Self::DeferredImport => RuntimeHelperId::ModuleDeferredImport,
        }
    }
}

/// A compiler List has a rooted backing array and an exact live prefix. Neither
/// component enters the JavaScript value or completion domain.
#[must_use]
pub(super) struct GatheredModules {
    pub(super) modules: GcLocal<ModuleRegistry>,
    pub(super) count: I64Local,
}
impl GatheredModules {
    pub(super) fn clear(self, schema: &RuntimeSchema, function: &mut Function) {
        schema.release_i64_local(self.count, function);
        self.modules.clear(function);
    }
}

impl FunctionBuilder<'_> {
    /// A Realm can retain several independently installed P instances. Its
    /// cache list never interprets one program's record ordinals in another.
    pub(super) fn emit_module_graph_in_realm(
        &self,
        realm: &GcLocal<RealmRecord>,
        program: &ProgramInstanceLocals,
        function: &mut Function,
    ) -> GcLocal<ModuleGraph, Nullable> {
        let schema = self.runtime_schema();
        let graph = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<RealmRecord>()
                .field(RealmRecordSchema::MODULES)
                .read(realm, schema, function)
                .reference(),
            function,
        );
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        graph.load(schema, function).is_null(function);
        function.instruction(&Instruction::BrIf(1));
        schema
            .struct_type::<ModuleGraph>()
            .field(ModuleGraphSchema::PROGRAM)
            .read(&graph, schema, function)
            .reference();
        program.reference(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::BrIf(1));
        graph.replace(
            schema
                .struct_type::<ModuleGraph>()
                .field(ModuleGraphSchema::NEXT)
                .read(&graph, schema, function)
                .reference(),
            function,
        );
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        graph
    }

    /// Module jobs retain a record and therefore the graph's immutable P.
    /// Neither the queue drainer's frame nor the execution Realm selects it.
    pub(super) fn emit_module_record_program(
        &self,
        record: &GcLocal<ModuleRecord>,
        function: &mut Function,
    ) -> ProgramInstanceLocals {
        let schema = self.runtime_schema();
        let graph = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleRecord>()
                .field(ModuleRecordSchema::GRAPH)
                .read(record, schema, function)
                .reference(),
            function,
        );
        let program = ProgramInstanceLocals::bind(
            schema,
            schema
                .struct_type::<ModuleGraph>()
                .field(ModuleGraphSchema::PROGRAM)
                .read(&graph, schema, function)
                .reference(),
            function,
        );
        graph.clear(function);
        program
    }

    /// An existing catalog uses its retained P. Before its first load, the
    /// explicitly calling frame supplies P; initialization publishes that same
    /// owner in this Realm without replacing any earlier graph.
    pub(super) fn emit_realm_module_program(
        &self,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> ProgramInstanceLocals {
        let schema = self.runtime_schema();
        let program = schema
            .reserve_gc_local(function)
            .initialize(self.current_program().reference(schema, function), function);
        let graph = self.emit_module_graph_in_realm(realm, self.current_program(), function);
        graph.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        program.replace(
            schema
                .struct_type::<ModuleGraph>()
                .field(ModuleGraphSchema::PROGRAM)
                .read(&graph, schema, function)
                .reference(),
            function,
        );
        function.instruction(&Instruction::End);
        let owner = ProgramInstanceLocals::bind(schema, program.load(schema, function), function);
        graph.clear(function);
        program.clear(function);
        owner
    }

    pub(crate) fn compile_module_runtime_operation(
        &mut self,
        operation: ModuleRuntimeOperation,
    ) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(operation.helper());
        let schema = self.runtime_schema();
        macro_rules! completion_body {
            ($parameters:ident, $method:ident $(, $value:ident)?) => {{
                let parameters = self.helper_parameters::<$parameters>(&mut function);
                let result = schema.reserve_completion(&mut function); result.initialize(&mut function);
                self.$method(&parameters.module, $(&parameters.$value,)? &result, &mut function)?;
                result.emit(&mut function); result.clear(&mut function); parameters.release(&mut function);
            }};
        }
        match operation {
            ModuleRuntimeOperation::Initialize => {
                let parameters =
                    self.helper_parameters::<ModuleInitializeParameters>(&mut function);
                let result = schema.reserve_completion(&mut function);
                result.initialize(&mut function);
                self.emit_module_initialization_runtime(&parameters.realm, &result, &mut function)?;
                result.emit(&mut function);
                result.clear(&mut function);
                parameters.release(&mut function);
            }
            ModuleRuntimeOperation::Evaluate => {
                completion_body!(ModuleEvaluateParameters, emit_module_evaluation_runtime)
            }
            ModuleRuntimeOperation::Execute => {
                completion_body!(ModuleExecuteParameters, emit_module_execute_runtime)
            }
            ModuleRuntimeOperation::Fulfilled => completion_body!(
                ModuleFulfilledParameters,
                emit_module_fulfilled_runtime,
                value
            ),
            ModuleRuntimeOperation::Rejected => completion_body!(
                ModuleRejectedParameters,
                emit_module_rejected_runtime,
                reason
            ),
            ModuleRuntimeOperation::DeferredImport => completion_body!(
                ModuleDeferredImportParameters,
                emit_module_deferred_import_runtime
            ),
            ModuleRuntimeOperation::Ready => {
                let parameters = self.helper_parameters::<ModuleReadyParameters>(&mut function);
                let result = schema.reserve_i32_local(&mut function);
                self.emit_module_readiness_runtime(&parameters.module, result, &mut function)?;
                result.load(&mut function);
                schema.release_i32_local(result, &mut function);
                parameters.release(&mut function);
            }
            ModuleRuntimeOperation::Gather => {
                let parameters = self.helper_parameters::<ModuleGatherParameters>(&mut function);
                let result = self.emit_module_gather_runtime(&parameters.module, &mut function)?;
                result.modules.load(schema, &mut function);
                result.count.load(&mut function);
                result.clear(schema, &mut function);
                parameters.release(&mut function);
            }
        }
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    pub(super) fn emit_module_gather_call(
        &mut self,
        record: &GcLocal<ModuleRecord>,
        function: &mut Function,
    ) -> Result<GatheredModules, EmitError> {
        let schema = self.runtime_schema();
        let slot = schema.reserve_gc_local(function);
        let count = schema.reserve_i64_local(function);
        let program = self.emit_module_record_program(record, function);
        let modules = schema
            .call_helper(
                ModuleGatherArguments::new(record, &program),
                self.runtime_helper_base()?,
                function,
            )
            .bind(schema, slot, count, function);
        program.clear(function);
        Ok(GatheredModules { modules, count })
    }

    pub(super) fn emit_module_list_contains(
        &self,
        list: &GcLocal<ModuleRegistry>,
        count: I64Local,
        sought: &GcLocal<ModuleRecord>,
        found: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let index = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::I32Const(0));
        found.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        count.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        let entry = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ModuleRegistry>()
                .read(list, index, schema, function)
                .reference(),
            function,
        );
        entry.load(schema, function);
        sought.load(schema, function);
        function.instruction(&Instruction::RefEq);
        found.store(function);
        entry.clear(function);
        found.load(function);
        function.instruction(&Instruction::BrIf(1));
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i32_local(index, function);
    }

    pub(super) fn emit_module_bounded_append(
        &self,
        list: &GcLocal<ModuleRegistry>,
        count: I64Local,
        value: &GcLocal<ModuleRecord>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let index = schema.reserve_i32_local(function);
        count.load(function);
        schema
            .array_type::<ModuleRegistry>()
            .length(list, schema, function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        count.load(function);
        function.instruction(&Instruction::I32WrapI64);
        index.store(function);
        schema.array_type::<ModuleRegistry>().write(
            list,
            index,
            GcOperand::nullable_reference(value, schema),
            schema,
            function,
        );
        count.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        count.store(function);
        schema.release_i32_local(index, function);
    }

    pub(super) fn emit_module_graph_list(
        &self,
        graph: &GcLocal<ModuleGraph>,
        function: &mut Function,
    ) -> GcLocal<ModuleRegistry> {
        let schema = self.runtime_schema();
        let records = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleGraph>()
                .field(ModuleGraphSchema::MODULES)
                .read(graph, schema, function)
                .reference(),
            function,
        );
        let count = schema.reserve_i32_local(function);
        schema
            .array_type::<ModuleRegistry>()
            .length(&records, schema, function);
        count.store(function);
        let list = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ModuleRegistry>()
                .filled(GcOperand::null(schema), count, function),
            function,
        );
        schema.release_i32_local(count, function);
        records.clear(function);
        list
    }

    pub(super) fn emit_module_list_entry(
        &self,
        list: &GcLocal<ModuleRegistry>,
        index: I64Local,
        function: &mut Function,
    ) -> GcLocal<ModuleRecord> {
        let schema = self.runtime_schema();
        let position = schema.reserve_i32_local(function);
        index.load(function);
        function.instruction(&Instruction::I64Const(u32::MAX as i64));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        index.load(function);
        function.instruction(&Instruction::I32WrapI64);
        position.store(function);
        let entry = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ModuleRegistry>()
                .read(list, position, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        schema.release_i32_local(position, function);
        entry
    }

    pub(super) fn emit_module_read_settled_evaluation(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let state = schema.reserve_i32_local(function);
        schema
            .struct_type::<PromiseObject>()
            .field(PromiseObjectSchema::HANDLED)
            .write(promise, GcOperand::boolean(true), schema, function);
        self.emit_load_promise_state_strict(promise, state, function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseObject>()
                .field(PromiseObjectSchema::RESULT)
                .read(promise, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, result.value(), schema, function);
        state.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            PromiseState::Pending,
        )));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        state.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            PromiseState::Rejected,
        )));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        result.set_kind(CompletionKind::Throw, function);
        function.instruction(&Instruction::Else);
        result.set_kind(CompletionKind::Normal, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I32Const(0));
        result.target().store(function);
        stored.clear(function);
        schema.release_i32_local(state, function);
        Ok(())
    }
}
