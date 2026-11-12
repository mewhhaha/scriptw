use super::*;
use crate::gc_types::{
    NamedBinding, NamedBindingSchema, NamedBindingTable, ObjectEnvironment, StringValue,
};
use scriptw_ir::{
    EvalBindingDeclarationIr, EvalDeclarativeEnvironmentKindIr, EvalEnvironmentRoleIr,
    EvalVisibleBindingIr,
};

#[derive(Clone, Copy)]
pub(crate) enum NamedEnvironmentKind {
    Unexposed,
    Variable,
    GlobalLexical,
    Lexical,
    Catch,
    SimpleCatch,
    WithObject,
}

impl NamedEnvironmentKind {
    pub(crate) const fn code(self) -> i32 {
        match self {
            Self::Unexposed => 0,
            Self::Variable => 1,
            Self::GlobalLexical => 5,
            Self::Lexical => 2,
            Self::Catch => 3,
            Self::SimpleCatch => 6,
            Self::WithObject => 4,
        }
    }
}

impl FunctionBuilder<'_> {
    /// Complete the actual record before publication: named metadata retains
    /// the same cells that lexical access and module linking will use.
    pub(crate) fn emit_initialize_named_environment_header(
        &mut self,
        parent: &GcLocal<Environment, Nullable>,
        cells: &GcLocal<BindingCellTable>,
        role: Option<&EvalEnvironmentRoleIr>,
        function: &mut Function,
    ) -> Result<GcLocal<Environment>, EmitError> {
        let schema = self.runtime_schema();
        let named = schema
            .reserve_gc_local::<NamedBindingTable, Nullable>(function)
            .initialize_null(schema, function);
        let object = schema
            .reserve_gc_local::<ObjectEnvironment, Nullable>(function)
            .initialize_null(schema, function);
        let kind = match role {
            None => NamedEnvironmentKind::Unexposed,
            Some(EvalEnvironmentRoleIr::Declarative { kind, bindings }) => {
                let table =
                    self.emit_publish_named_declarative_environment(cells, bindings, function)?;
                named.replace(table.load(schema, function).nullable(), function);
                table.clear(function);
                match kind {
                    EvalDeclarativeEnvironmentKindIr::Variable => NamedEnvironmentKind::Variable,
                    EvalDeclarativeEnvironmentKindIr::GlobalLexical => {
                        NamedEnvironmentKind::GlobalLexical
                    }
                    EvalDeclarativeEnvironmentKindIr::Lexical
                    | EvalDeclarativeEnvironmentKindIr::Parameters => NamedEnvironmentKind::Lexical,
                    EvalDeclarativeEnvironmentKindIr::Catch => NamedEnvironmentKind::Catch,
                    EvalDeclarativeEnvironmentKindIr::SimpleCatch => {
                        NamedEnvironmentKind::SimpleCatch
                    }
                }
            }
            Some(EvalEnvironmentRoleIr::WithObject { object_slot }) => {
                let index = schema.reserve_i32_local(function);
                function.instruction(&Instruction::I32Const(*object_slot as i32));
                index.store(function);
                let cell = schema
                    .reserve_gc_local::<BindingCell, NonNullable>(function)
                    .initialize(
                        schema
                            .array_type::<BindingCellTable>()
                            .read(cells, index, schema, function)
                            .reference(),
                        function,
                    );
                object.replace(
                    schema
                        .struct_type::<ObjectEnvironment>()
                        .construct(
                            (
                                GcOperand::reference(&cell, schema),
                                GcOperand::boolean(true),
                            ),
                            function,
                        )
                        .nullable(),
                    function,
                );
                cell.clear(function);
                schema.release_i32_local(index, function);
                NamedEnvironmentKind::WithObject
            }
        };
        let environment =
            self.emit_environment_record(parent, cells, &named, &object, kind, function);
        object.clear(function);
        named.clear(function);
        Ok(environment)
    }

    fn emit_publish_named_declarative_environment(
        &mut self,
        cells: &GcLocal<BindingCellTable>,
        bindings: &[EvalVisibleBindingIr],
        function: &mut Function,
    ) -> Result<GcLocal<NamedBindingTable>, EmitError> {
        let schema = self.runtime_schema();
        let mut entries = Vec::with_capacity(bindings.len());
        for binding in bindings {
            let name = schema
                .reserve_gc_local::<StringValue, NonNullable>(function)
                .initialize(
                    self.emit_interned_string_reference(&binding.source_name, function)?,
                    function,
                );
            let index = schema.reserve_i32_local(function);
            function.instruction(&Instruction::I32Const(binding.slot as i32));
            index.store(function);
            let cell = schema
                .reserve_gc_local::<BindingCell, NonNullable>(function)
                .initialize(
                    schema
                        .array_type::<BindingCellTable>()
                        .read(cells, index, schema, function)
                        .reference(),
                    function,
                );
            let entry = schema
                .reserve_gc_local::<NamedBinding, NonNullable>(function)
                .initialize(
                    schema.struct_type::<NamedBinding>().construct(
                        (
                            GcOperand::reference(&name, schema),
                            GcOperand::reference(&cell, schema),
                            GcOperand::boolean(false),
                            GcOperand::boolean(true),
                            GcOperand::boolean(match binding.declaration {
                                EvalBindingDeclarationIr::Parameter
                                | EvalBindingDeclarationIr::Variable => false,
                                EvalBindingDeclarationIr::Lexical
                                | EvalBindingDeclarationIr::NamedFunctionExpression => true,
                            }),
                        ),
                        function,
                    ),
                    function,
                );
            cell.clear(function);
            schema.release_i32_local(index, function);
            name.clear(function);
            entries.push(entry);
        }
        let table = schema
            .reserve_gc_local::<NamedBindingTable, NonNullable>(function)
            .initialize(
                schema.array_type::<NamedBindingTable>().fixed(
                    entries
                        .iter()
                        .map(|entry| GcOperand::nullable_reference(entry, schema)),
                    function,
                ),
                function,
            );
        for entry in entries.into_iter().rev() {
            entry.clear(function);
        }
        Ok(table)
    }

    /// Find only this record's live binding. The caller owns parent traversal
    /// and dispatches Object/Global Records through their actual HasBinding.
    pub(crate) fn emit_find_own_named_binding<N: GcFieldNullability>(
        &mut self,
        environment: &GcLocal<Environment, N>,
        key: &GcLocal<StringValue>,
        found: &GcLocal<NamedBinding, Nullable>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        found.set_null(schema, function);
        let entries = schema
            .reserve_gc_local::<NamedBindingTable, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::NAMED_BINDINGS)
                    .read(environment, schema, function)
                    .reference(),
                function,
            );
        entries.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let count = schema.reserve_i32_local(function);
        schema
            .array_type::<NamedBindingTable>()
            .length(&entries, schema, function);
        count.store(function);
        let index = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let candidate = schema
            .reserve_gc_local::<NamedBinding, Nullable>(function)
            .initialize_null(schema, function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        candidate.replace(
            schema
                .array_type::<NamedBindingTable>()
                .read(&entries, index, schema, function)
                .reference(),
            function,
        );
        candidate.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .struct_type::<NamedBinding>()
            .field(NamedBindingSchema::PRESENT)
            .read(&candidate, schema, function);
        self.open_frame(ControlFrameKind::If, function);
        let name = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<NamedBinding>()
                    .field(NamedBindingSchema::NAME)
                    .read(&candidate, schema, function)
                    .reference(),
                function,
            );
        // The same String object is the same name, and names of different
        // lengths differ; only the rest compare code units.
        let equal = schema.reserve_i32_local(function);
        let _ = name.load(schema, function);
        let _ = key.load(schema, function);
        function.instruction(&Instruction::RefEq);
        equal.store(function);
        equal.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        for string in [&name, key] {
            let units = schema.reserve_gc_local(function).initialize(
                schema
                    .field(crate::gc_types::StringValueSchema::CODE_UNITS)
                    .read(string, schema, function)
                    .reference(),
                function,
            );
            schema
                .array_type::<crate::gc_types::CodeUnitArray>()
                .length(&units, schema, function);
            units.clear(function);
        }
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_string_payload_equality_i32(&name, key, function);
        equal.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        name.clear(function);
        equal.load(function);
        schema.release_i32_local(equal, function);
        self.open_frame(ControlFrameKind::If, function);
        found.replace(candidate.load(schema, function), function);
        function.instruction(&Instruction::Br(4));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        candidate.clear(function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        entries.clear(function);
    }
}
