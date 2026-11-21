//! Side-effect-free iterative walks borrow canonical module records.

use super::runtime::GatheredModules;
use super::*;

#[derive(Clone, Copy)]
enum ModuleTraversal<'a> {
    Readiness(I32Local),
    AsyncDependencies {
        output: &'a GcLocal<ModuleRegistry>,
        count: I64Local,
    },
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_module_scc_evaluated(
        &self,
        record: &GcLocal<ModuleRecord>,
        evaluated: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let root = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleRecord>()
                .field(ModuleRecordSchema::CYCLE_ROOT)
                .read(record, schema, function)
                .reference(),
            function,
        );
        root.load(schema, function).is_null(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        root.replace(record.load(schema, function).nullable(), function);
        function.instruction(&Instruction::End);
        let selected = schema.reserve_gc_local(function).initialize(
            root.load(schema, function).require_non_null(function),
            function,
        );
        self.emit_load_module_state_strict(&selected, evaluated, function);
        evaluated.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            ModuleEvaluationState::Evaluated,
        )));
        function.instruction(&Instruction::I32Eq);
        evaluated.store(function);
        selected.clear(function);
        root.clear(function);
    }

    pub(super) fn emit_module_readiness_runtime(
        &mut self,
        record: &GcLocal<ModuleRecord>,
        ready: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        function.instruction(&Instruction::I32Const(1));
        ready.store(function);
        self.emit_module_traversal(record, ModuleTraversal::Readiness(ready), function)
    }

    pub(super) fn emit_module_gather_runtime(
        &mut self,
        record: &GcLocal<ModuleRecord>,
        function: &mut Function,
    ) -> Result<GatheredModules, EmitError> {
        let schema = self.runtime_schema();
        let graph = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleRecord>()
                .field(ModuleRecordSchema::GRAPH)
                .read(record, schema, function)
                .reference(),
            function,
        );
        let modules = self.emit_module_graph_list(&graph, function);
        let count = schema.reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(0));
        count.store(function);
        self.emit_module_traversal(
            record,
            ModuleTraversal::AsyncDependencies {
                output: &modules,
                count,
            },
            function,
        )?;
        graph.clear(function);
        Ok(GatheredModules { modules, count })
    }

    fn emit_module_traversal(
        &mut self,
        origin: &GcLocal<ModuleRecord>,
        purpose: ModuleTraversal<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let graph = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleRecord>()
                .field(ModuleRecordSchema::GRAPH)
                .read(origin, schema, function)
                .reference(),
            function,
        );
        let records = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleGraph>()
                .field(ModuleGraphSchema::MODULES)
                .read(&graph, schema, function)
                .reference(),
            function,
        );
        let limit = schema.reserve_i32_local(function);
        schema
            .array_type::<ModuleRegistry>()
            .length(&records, schema, function);
        limit.store(function);
        let frames = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ModuleDfsStack>()
                .filled(GcOperand::null(schema), limit, function),
            function,
        );
        let seen = self.emit_module_graph_list(&graph, function);
        let depth = schema.reserve_i64_local(function);
        let seen_count = schema.reserve_i64_local(function);
        for local in [depth, seen_count] {
            function.instruction(&Instruction::I64Const(0));
            local.store(function);
        }
        let candidate = schema
            .reserve_gc_local::<ModuleRecord, Nullable>(function)
            .initialize(origin.load(schema, function).nullable(), function);
        let found = schema.reserve_i32_local(function);
        let complete = schema.reserve_i32_local(function);
        let state = schema.reserve_i32_local(function);
        let kind = schema.reserve_i32_local(function);
        let position = schema.reserve_i32_local(function);
        let next = schema.reserve_i64_local(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let again = self.open_frame(ControlFrameKind::Loop, function);
        candidate.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let selected = schema.reserve_gc_local(function).initialize(
            candidate.load(schema, function).require_non_null(function),
            function,
        );
        self.emit_module_list_contains(&seen, seen_count, &selected, found, function);
        found.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_module_bounded_append(&seen, seen_count, &selected, function);
        self.emit_load_module_state_strict(&selected, state, function);
        self.emit_load_module_activation_kind_strict(&selected, kind, function);
        self.emit_module_scc_evaluated(&selected, complete, function);
        complete.load(function);
        function.instruction(&Instruction::I32Eqz);
        if matches!(purpose, ModuleTraversal::AsyncDependencies { .. }) {
            state.load(function);
            function.instruction(&Instruction::I32Const(GcI32Constant::encode(
                ModuleEvaluationState::Evaluating,
            )));
            function.instruction(&Instruction::I32Ne);
            function.instruction(&Instruction::I32And);
        }
        self.open_frame(ControlFrameKind::If, function);
        match purpose {
            ModuleTraversal::Readiness(ready) => {
                state.load(function);
                function.instruction(&Instruction::I32Const(GcI32Constant::encode(
                    ModuleEvaluationState::Evaluating,
                )));
                function.instruction(&Instruction::I32Eq);
                state.load(function);
                function.instruction(&Instruction::I32Const(GcI32Constant::encode(
                    ModuleEvaluationState::EvaluatingAsync,
                )));
                function.instruction(&Instruction::I32Eq);
                function.instruction(&Instruction::I32Or);
                kind.load(function);
                function.instruction(&Instruction::I32Const(GcI32Constant::encode(
                    ModuleActivationKind::Async,
                )));
                function.instruction(&Instruction::I32Eq);
                function.instruction(&Instruction::I32Or);
                self.open_frame(ControlFrameKind::If, function);
                function.instruction(&Instruction::I32Const(0));
                ready.store(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                ready.load(function);
                self.open_frame(ControlFrameKind::If, function);
            }
            ModuleTraversal::AsyncDependencies { output, count } => {
                kind.load(function);
                function.instruction(&Instruction::I32Const(GcI32Constant::encode(
                    ModuleActivationKind::Async,
                )));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_module_bounded_append(output, count, &selected, function);
                function.instruction(&Instruction::Else);
            }
        }
        depth.load(function);
        limit.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        depth.load(function);
        function.instruction(&Instruction::I32WrapI64);
        position.store(function);
        let frame = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<ModuleDfsFrame>().construct(
                (
                    GcOperand::reference(&selected, schema),
                    GcOperand::null(schema),
                    GcOperand::i64(0),
                    GcOperand::i64(0),
                    GcOperand::null(schema),
                ),
                function,
            ),
            function,
        );
        schema.array_type::<ModuleDfsStack>().write(
            &frames,
            position,
            GcOperand::nullable_reference(&frame, schema),
            schema,
            function,
        );
        frame.clear(function);
        depth.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        depth.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        selected.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        candidate.set_null(schema, function);
        if let ModuleTraversal::Readiness(ready) = purpose {
            ready.load(function);
            function.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(exit, function);
        }
        depth.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::BrIf(1));
        depth.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I32WrapI64);
        position.store(function);
        let frame = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ModuleDfsStack>()
                .read(&frames, position, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        let record = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleDfsFrame>()
                .field(ModuleDfsFrameSchema::MODULE)
                .read(&frame, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<ModuleDfsFrame>()
            .field(ModuleDfsFrameSchema::NEXT_DEPENDENCY)
            .read(&frame, schema, function)
            .store_i64(next, function);
        let requests = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleRecord>()
                .field(ModuleRecordSchema::REQUESTS)
                .read(&record, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        next.load(function);
        schema
            .array_type::<ModuleRequestTable>()
            .length(&requests, schema, function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, function);
        let request_index = schema.reserve_i32_local(function);
        next.load(function);
        function.instruction(&Instruction::I32WrapI64);
        request_index.store(function);
        let request = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ModuleRequestTable>()
                .read(&requests, request_index, schema, function)
                .reference(),
            function,
        );
        self.emit_load_module_request_phase_strict(&request, state, function);
        candidate.replace(
            schema
                .struct_type::<ModuleRequest>()
                .field(ModuleRequestSchema::TARGET)
                .read(&request, schema, function)
                .reference()
                .nullable(),
            function,
        );
        next.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        next.store(function);
        schema
            .struct_type::<ModuleDfsFrame>()
            .field(ModuleDfsFrameSchema::NEXT_DEPENDENCY)
            .write(&frame, GcOperand::i64_local(next), schema, function);
        request.clear(function);
        schema.release_i32_local(request_index, function);
        function.instruction(&Instruction::Else);
        schema.array_type::<ModuleDfsStack>().write(
            &frames,
            position,
            GcOperand::null(schema),
            schema,
            function,
        );
        depth.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        depth.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        requests.clear(function);
        record.clear(function);
        frame.clear(function);
        self.emit_branch_to_target(again, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i64_local(next, function);
        schema.release_i32_local(position, function);
        schema.release_i32_local(kind, function);
        schema.release_i32_local(state, function);
        schema.release_i32_local(complete, function);
        schema.release_i32_local(found, function);
        candidate.clear(function);
        schema.release_i64_local(seen_count, function);
        schema.release_i64_local(depth, function);
        seen.clear(function);
        frames.clear(function);
        schema.release_i32_local(limit, function);
        records.clear(function);
        graph.clear(function);
        Ok(())
    }

    pub(super) fn emit_load_module_request_phase_strict(
        &self,
        request: &GcLocal<ModuleRequest>,
        out: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        schema
            .struct_type::<ModuleRequest>()
            .field(ModuleRequestSchema::PHASE)
            .read(request, schema, function)
            .store(out, function);
        crate::gc_types::emit_i32_in(
            out,
            ModuleRequestPhase::ALL
                .iter()
                .copied()
                .map(|value| GcI32Constant::encode(value)),
            function,
        );
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
    }
}
