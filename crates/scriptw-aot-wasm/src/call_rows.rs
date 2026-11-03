//! Compact code for long runs of ordinary calls with inert numeric arguments.
//!
//! The callee read and Call remain real Wasm in the source body. Only exact
//! numeric bits and argument counts move to private program-owned static data.
use super::*;
use crate::environments::environment_reference::EnvironmentIdentifierRead;
use crate::gc_types::{GcOperand, StoredValue, ValueArray};
use scriptw_ir::{
    EnvironmentIdentifierIr, EnvironmentIdentifierOperationIr, EnvironmentIdentifierResolutionStart,
};

// Amortize the loop and private construction against an otherwise large body.
const MIN_NUMERIC_CALL_ROWS: usize = 64;

/// The constructor proves that every row retains the same live Reference,
/// excludes direct eval/static constructor annotations, and has inert arguments.
pub(crate) struct NumericCallRun<'a> {
    reference: NumericCallReference<'a>,
    arguments: Vec<&'a [TypedExpr]>,
    encoded_bytes: usize,
}

/// Retain the original Reference operation. Environment-backed bare calls must
/// repeat ResolveBinding/GetValue and obtain their receiver from that held
/// Reference; a value-only callee would lose with-object receiver semantics.
#[derive(Clone, Copy)]
enum NumericCallReference<'a> {
    Value(&'a TypedExpr),
    Environment(&'a EnvironmentIdentifierIr),
}

impl NumericCallReference<'_> {
    fn same_reference(self, other: Self) -> bool {
        match (self, other) {
            (Self::Value(first), Self::Value(next)) => first.expr == next.expr,
            (Self::Environment(first), Self::Environment(next)) => {
                first.name == next.name
                    && first.strictness == next.strictness
                    && first.resolution_start() == next.resolution_start()
            }
            _ => false,
        }
    }
}

impl<'a> NumericCallRun<'a> {
    pub(crate) fn of_prefix(statements: &'a [StatementIr]) -> Option<Self> {
        let (reference, _) = Self::row(statements.first()?)?;
        let mut arguments = Vec::new();
        let mut encoded_bytes = 0usize;
        for statement in statements {
            let Some((next_reference, next_arguments)) = Self::row(statement) else {
                break;
            };
            if !reference.same_reference(next_reference) {
                break;
            }
            u32::try_from(next_arguments.len()).ok()?;
            encoded_bytes = encoded_bytes
                .checked_add(8)?
                .checked_add(next_arguments.len().checked_mul(8)?)?;
            u32::try_from(encoded_bytes).ok()?;
            arguments.push(next_arguments);
        }
        if arguments.len() < MIN_NUMERIC_CALL_ROWS {
            return None;
        }
        u32::try_from(arguments.len()).ok()?;
        Some(Self {
            reference,
            arguments,
            encoded_bytes,
        })
    }

    fn row(statement: &'a StatementIr) -> Option<(NumericCallReference<'a>, &'a [TypedExpr])> {
        let StatementIr::Expression(expression) = statement else {
            return None;
        };
        let (reference, args) = match &expression.expr {
            ExprIr::CallIndirect {
                direct_eval: None,
                callee,
                this_arg: None,
                args,
                static_regexp_compilation: None,
            } if matches!(
                &callee.expr,
                ExprIr::Identifier(_)
                    | ExprIr::GlobalIdentifierRead { .. }
                    | ExprIr::GlobalPropertyRead { .. }
            ) =>
            {
                (NumericCallReference::Value(callee), args.as_slice())
            }
            ExprIr::EnvironmentIdentifier(identifier) => match &identifier.operation {
                EnvironmentIdentifierOperationIr::Call {
                    args,
                    direct_eval: None,
                    ..
                } => (
                    NumericCallReference::Environment(identifier),
                    args.as_slice(),
                ),
                _ => return None,
            },
            _ => return None,
        };
        if !args
            .iter()
            .all(|argument| matches!(argument.expr, ExprIr::Number(_)))
        {
            return None;
        }
        Some((reference, args))
    }

    pub(crate) fn len(&self) -> usize {
        self.arguments.len()
    }

    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.encoded_bytes);
        for arguments in &self.arguments {
            bytes.extend_from_slice(&(arguments.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&0u32.to_le_bytes());
            for argument in *arguments {
                let ExprIr::Number(bits) = &argument.expr else {
                    unreachable!("NumericCallRun owns only numeric arguments");
                };
                bytes.extend_from_slice(&bits.to_le_bytes());
            }
        }
        bytes
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NumericCallTable {
    address: u32,
    length: u32,
}
impl NumericCallTable {
    fn metadata_span(
        self,
        strings: &StringPool,
    ) -> Result<crate::data::ProgramMetadataSpan, EmitError> {
        crate::data::ProgramMetadataSpan::for_pool(strings, self.address, self.length)
    }
}

#[derive(Debug, Default)]
pub(crate) struct NumericCallTables {
    tables: BTreeMap<Vec<u8>, NumericCallTable>,
    error: Option<EmitError>,
}

impl NumericCallTables {
    pub(crate) fn collect(&mut self, run: &NumericCallRun<'_>, bytes: &mut crate::data::PoolBytes) {
        let key = run.encode();
        if self.tables.contains_key(&key) {
            return;
        }
        let bounds = (|| {
            let aligned = bytes.len().checked_add(7)? & !7;
            let end = aligned.checked_add(key.len())?;
            let address = STATIC_DATA_OFFSET.checked_add(u32::try_from(aligned).ok()?)?;
            STATIC_DATA_OFFSET.checked_add(u32::try_from(end).ok()?)?;
            Some((aligned, address))
        })();
        let Some((aligned, address)) = bounds else {
            self.error.get_or_insert_with(|| {
                EmitError::unsupported("numeric call rows exceed the program static address domain")
            });
            return;
        };
        bytes.resize(aligned, 0);
        bytes.extend_from_slice(&key);
        let length = u32::try_from(key.len()).expect("numeric table extent was checked");
        self.tables
            .insert(key, NumericCallTable { address, length });
    }

    pub(crate) fn validate(&self) -> Result<(), EmitError> {
        match &self.error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    pub(crate) fn find(&self, run: &NumericCallRun<'_>) -> Result<NumericCallTable, EmitError> {
        self.validate()?;
        self.tables.get(&run.encode()).copied().ok_or_else(|| {
            EmitError::unsupported(
                "compiler invariant: admitted numeric call rows were not collected",
            )
        })
    }
}

impl FunctionBuilder<'_> {
    fn emit_numeric_call_reference(
        &mut self,
        reference: NumericCallReference<'_>,
        callee: &crate::gc_types::ValueLocals,
        receiver: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match reference {
            NumericCallReference::Value(expression) => {
                self.compile_expr_to_value(expression, callee, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                receiver.set_undefined(function);
            }
            NumericCallReference::Environment(identifier) => {
                let schema = self.runtime_schema();
                let key = schema.reserve_gc_local(function).initialize(
                    self.emit_interned_string_reference(&identifier.name, function)?,
                    function,
                );
                let held = match identifier.resolution_start() {
                    EnvironmentIdentifierResolutionStart::CurrentEnvironment => self
                        .emit_resolve_environment_identifier(
                            &key,
                            identifier.strictness,
                            function,
                        )?,
                    EnvironmentIdentifierResolutionStart::GlobalEnvironment => {
                        self.emit_resolve_global_identifier(&key, identifier.strictness, function)?
                    }
                    EnvironmentIdentifierResolutionStart::Cell(proof) => self
                        .emit_resolve_module_cell_identifier(
                            &proof,
                            &identifier.name,
                            &key,
                            identifier.strictness,
                            function,
                        )?,
                };
                self.emit_environment_identifier_get(
                    &held,
                    EnvironmentIdentifierRead::Value,
                    callee,
                    function,
                )?;
                self.emit_environment_identifier_call_base(&held, receiver, function);
                self.release_environment_identifier_reference(held, function);
                key.clear(function);
            }
        }
        Ok(())
    }

    pub(crate) fn compile_numeric_call_run(
        &mut self,
        run: &NumericCallRun<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let table = self
            .strings
            .numeric_call_table(run)?
            .metadata_span(self.strings)?;
        let schema = self.runtime_schema();
        let metadata = self
            .current_program()
            .metadata_span(schema, table, function);
        let cursor = schema.reserve_i32_local(function);
        let row_index = schema.reserve_i32_local(function);
        let argument_count = schema.reserve_i32_local(function);
        let argument_index = schema.reserve_i32_local(function);
        let bits = schema.reserve_i64_local(function);
        let callee = schema.reserve_value_local(function);
        let receiver = schema.reserve_value_local(function);
        let value = schema.reserve_value_local(function);
        let result = schema.reserve_completion(function);
        cursor.set_constant(0, function);
        function.instruction(&Instruction::I32Const(0));
        row_index.store(function);

        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        row_index.load(function);
        function.instruction(&Instruction::I32Const(run.len() as i32));
        function.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        // GetValue is live on every row, before any argument construction.
        // An earlier call can replace the binding or install a throwing getter.
        self.emit_numeric_call_reference(run.reference, &callee, &receiver, function)?;
        metadata.read_u32(cursor, schema, function);
        argument_count.store(function);
        cursor.load(function);
        function.instruction(&Instruction::I32Const(8));
        function.instruction(&Instruction::I32Add);
        cursor.store(function);
        value.set_undefined(function);
        let empty = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&value, function),
            function,
        );
        let list = schema.reserve_gc_local(function).initialize(
            schema.array_type::<ValueArray>().filled(
                GcOperand::reference(&empty, schema),
                argument_count,
                function,
            ),
            function,
        );
        empty.clear(function);
        function.instruction(&Instruction::I32Const(0));
        argument_index.store(function);
        let arguments_done = self.open_frame(ControlFrameKind::Block, function);
        let argument_next = self.open_frame(ControlFrameKind::Loop, function);
        argument_index.load(function);
        argument_count.load(function);
        function.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(arguments_done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        metadata.read_u64(cursor, 0, schema, function);
        bits.store(function);
        value.set_number(bits, function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&value, function),
            function,
        );
        schema.array_type::<ValueArray>().write(
            &list,
            argument_index,
            GcOperand::reference(&stored, schema),
            schema,
            function,
        );
        stored.clear(function);
        cursor.load(function);
        function.instruction(&Instruction::I32Const(8));
        function.instruction(&Instruction::I32Add);
        cursor.store(function);
        argument_index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        argument_index.store(function);
        self.emit_branch_to_target(argument_next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        value.clear(function);

        self.emit_function_or_proxy_call_with_argv(&callee, &receiver, &list, &result, function)?;
        // Keep ExpressionStatement's whole abrupt completion and last value.
        self.completion().copy_from(&result, function);
        self.emit_propagate_current_throw_if_needed(function);
        self.set_completion_kind(CompletionKind::Normal, function);
        result.clear(function);
        list.clear(function);
        receiver.clear(function);
        callee.clear(function);
        row_index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        row_index.store(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        metadata.clear(schema, function);
        schema.release_i64_local(bits, function);
        schema.release_i32_local(argument_index, function);
        schema.release_i32_local(argument_count, function);
        schema.release_i32_local(row_index, function);
        schema.release_i32_local(cursor, function);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};
    use scriptw_ir::lower;
    use wasmparser::{Validator, WasmFeatures};

    fn emit_large_source(repetitions: usize) -> (u32, usize) {
        std::thread::Builder::new()
            .name(format!("numeric-call-body-{repetitions}"))
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let calls = (0..repetitions)
                    .map(|index| match index % 3 {
                        0 => format!("step({index});\n"),
                        1 => format!("step({index}, 2);\n"),
                        _ => format!("step({index}, 2, 3);\n"),
                    })
                    .collect::<String>();
                let source = format!(
                    "function processRows(seed) {{ let total = seed; \
                     function step(value, factor, bias) {{ total += value; }} \
                     {calls} return total; }} processRows(7);"
                );
                let parsed = parse(&source, ParseOptions::script()).expect("large source parses");
                let ir = lower(&parsed);
                assert!(ir.is_wasm_supported(), "{:?}", ir.diagnostics);
                let artifact = emit(&ir).expect("large ordinary source emits real Wasm");
                let features = WasmFeatures::default()
                    | WasmFeatures::THREADS
                    | WasmFeatures::FUNCTION_REFERENCES
                    | WasmFeatures::GC
                    | WasmFeatures::EXCEPTIONS;
                for bytes in [
                    &artifact.runtime().expect("source has R").bytes()[..],
                    artifact.bytes.as_slice(),
                ] {
                    Validator::new_with_features(features)
                        .validate_all(bytes)
                        .expect("numeric calls retain valid linked Wasm types and bodies");
                }
                let body = artifact
                    .function_sizes
                    .iter()
                    .filter(|body| body.name.starts_with("js::processRows#"))
                    .map(|body| body.body_bytes.bytes())
                    .max()
                    .expect("source processing body is attributed");
                (body, artifact.bytes.len())
            })
            .expect("bounded compiler stack worker starts")
            .join()
            .expect("large call emission does not panic")
    }

    #[test]
    fn large_general_numeric_call_body_remains_bounded_real_wasm() {
        let (body, _) = emit_large_source(4096);
        assert!(
            body < 32 * 1024,
            "large source body has {body} emitted bytes"
        );
    }

    #[test]
    fn numeric_call_growth_keeps_constants_out_of_function_code() {
        let (small_body, small_program) = emit_large_source(128);
        let (large_body, large_program) = emit_large_source(2048);
        assert!(
            large_program > small_program,
            "all distinct constant rows remain in P"
        );
        assert!(
            large_body <= small_body + 4096,
            "ordinary body growth is bounded: {small_body} -> {large_body}"
        );
        assert!(
            large_program < small_program + 128 * 1024,
            "whole P growth retains compact exact constants: {small_program} -> {large_program}"
        );
    }
}
