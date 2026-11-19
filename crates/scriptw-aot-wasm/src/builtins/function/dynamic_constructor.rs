use super::*;
use scriptw_ir::{GeneratorPlanIr, ResumablePlanIr};
mod native_compiler;

#[derive(Clone, Copy)]
enum EmptyDerivedFunction {
    Generator,
    Async,
    AsyncGenerator,
}

impl EmptyDerivedFunction {
    const ALL: [Self; 3] = [Self::Generator, Self::Async, Self::AsyncGenerator];

    const fn id(self) -> &'static str {
        match self {
            Self::Generator => "scriptw:empty:generator",
            Self::Async => "scriptw:empty:async",
            Self::AsyncGenerator => "scriptw:empty:async-generator",
        }
    }

    const fn protocol(self) -> FunctionProtocolIr {
        match self {
            Self::Generator => FunctionProtocolIr::Generator,
            Self::Async => FunctionProtocolIr::Async,
            Self::AsyncGenerator => FunctionProtocolIr::AsyncGenerator,
        }
    }

    const fn source(self) -> &'static str {
        match self {
            Self::Generator => "function* anonymous(\n) {\n\n}",
            Self::Async => "async function anonymous(\n) {\n\n}",
            Self::AsyncGenerator => "async function* anonymous(\n) {\n\n}",
        }
    }

    fn body(self) -> FunctionIr {
        FunctionIr {
            template_source: None,
            eval_environment: None,
            id: self.id().to_string(),
            name: "anonymous".to_string(),
            to_string_representation: CallableToStringRepresentation::ExactSource(
                self.source().to_string(),
            ),
            protocol: self.protocol(),
            generator_plan: matches!(self, Self::Generator).then(|| GeneratorPlanIr {
                entry_state: 0,
                state_count: 1,
                suspension_points: Vec::new(),
            }),
            resumable_plan: matches!(self, Self::AsyncGenerator).then(ResumablePlanIr::empty),
            strict: false,
            class_element_execution_kind: ClassElementExecutionKind::None,
            class_heritage_kind: ClassHeritageKind::None,
            is_static_class_member: false,
            is_derived_constructor: false,
            is_synthetic_default_derived_constructor: false,
            class_instance_element_plan: None,
            super_constructor_target: None,
            uses_super: false,
            this_before_super: false,
            lexical_derived_activation: None,
            private_name_ids: BTreeMap::new(),
            captures_private_environment: false,
            is_nested: false,
            is_expression: true,
            is_named_expression: false,
            captures_lexical_this: false,
            captures_lexical_arguments: false,
            params: Vec::new(),
            body: BlockIr {
                statements: Vec::new(),
                result_kind: ValueKind::Undefined,
                lexical_environment: None,
            },
            return_kind: ValueKind::Object,
            return_shape: None,
            return_targets: FunctionTargetKnowledge::none(),
            constructor_instance: ValueInfo::undefined(),
            // Ordinary lowering owns this invocation state in every resumable
            // activation, including empty bodies, and assigns slots by name.
            owned_env_bindings: std::collections::BTreeSet::from([
                LEXICAL_ARGUMENTS_NAME,
                LEXICAL_NEW_TARGET_NAME,
                LEXICAL_THIS_NAME,
            ])
            .into_iter()
            .zip(0_u32..)
            .map(|(name, slot)| OwnedEnvBindingIr {
                mutability: scriptw_ir::EnvironmentBindingMutabilityIr::Mutable,
                name: name.to_string(),
                slot,
            })
            .collect(),
            captured_bindings: Vec::new(),
        }
    }
}

/// Whether `function` is one of the compiler-owned bodies that
/// `append_empty_dynamic_function_bodies` adds.
pub(crate) fn is_empty_dynamic_function_body(function: &FunctionIr) -> bool {
    is_empty_dynamic_function_id(&function.id)
}

pub(crate) fn is_empty_dynamic_function_id(id: &str) -> bool {
    EmptyDerivedFunction::ALL.iter().any(|kind| kind.id() == id)
}

pub(crate) fn append_empty_dynamic_function_bodies(script: &mut ScriptIr) {
    for kind in EmptyDerivedFunction::ALL {
        assert!(
            !script
                .functions
                .iter()
                .any(|function| function.id == kind.id()),
            "compiler-generated function identity `{}` is already occupied",
            kind.id(),
        );
        // Every Wasm artifact has heap-backed realm roots, and bootstrap
        // exposes all three constructors even without a direct source call.
        script.functions.push(kind.body());
    }
}

/// The wire word naming a constructor kind to the program hook.
fn dynamic_function_kind_code(kind: DynamicFunctionKind) -> i32 {
    match kind {
        DynamicFunctionKind::Ordinary => 0,
        DynamicFunctionKind::Generator => 1,
        DynamicFunctionKind::Async => 2,
        DynamicFunctionKind::AsyncGenerator => 3,
    }
}

impl FunctionBuilder<'_> {
    /// Every argument is converted to a String in order, abrupt completions
    /// propagating, before anything is matched. The program (when it prepared
    /// any literal `new Function(...)`) then answers through its hook; a miss
    /// invokes the native compiler once with the already converted sources.
    pub(super) fn emit_dynamic_constructor_body(
        &mut self,
        kind: DynamicFunctionKind,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        use crate::gc_types::{StoredValue, ValueArray};
        use crate::runtime_helpers::PreparedDynamicFunctionArguments;
        let schema = self.runtime_schema();
        let new_target = schema.reserve_value_local(function);
        self.emit_dynamic_constructor_new_target(&new_target, function)?;
        let count = schema.reserve_i32_local(function);
        self.body_entry_locals()
            .expect("dynamic constructor owns arguments")
            .argument_count()
            .load(function);
        function.instruction(&Instruction::I32WrapI64);
        count.store(function);
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let initial = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&undefined, function),
            function,
        );
        undefined.clear(function);
        let sources = schema.reserve_gc_local(function).initialize(
            schema.array_type::<ValueArray>().filled(
                crate::gc_types::GcOperand::reference(&initial, schema),
                count,
                function,
            ),
            function,
        );
        initial.clear(function);
        let argument = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        let index = schema.reserve_i32_local(function);
        index.set_constant(0, function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let converted = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(converted, function);
        self.emit_argument_vector_entry_to_value(
            self.body_entry_locals()
                .expect("dynamic constructor owns arguments")
                .arguments(),
            index,
            &argument,
            function,
        );
        self.emit_value_to_string_payload(&argument, &pending, function)?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&pending, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(pending.value(), function),
            function,
        );
        schema.array_type::<ValueArray>().write(
            &sources,
            index,
            crate::gc_types::GcOperand::reference(&stored, schema),
            schema,
            function,
        );
        stored.clear(function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        let outcome = schema.reserve_completion(function);
        outcome.initialize(function);
        let matched = schema.reserve_i32_local(function);
        matched.set_constant(0, function);
        let kind_word = schema.reserve_i32_local(function);
        kind_word.set_constant(dynamic_function_kind_code(kind), function);
        self.emit_program_hook_dispatch::<crate::gc_types::PreparedDynamicFunctionHook>(
            |builder, hook, function| {
                let context = builder
                    .current_function_context()
                    .expect("dynamic constructor owns its callable context");
                schema
                    .call_hook(
                        hook,
                        PreparedDynamicFunctionArguments::new(
                            kind_word,
                            &new_target,
                            &sources,
                            context,
                            builder.current_environment(),
                        ),
                        function,
                    )
                    .store(&outcome, matched, function);
                Ok(())
            },
            |_, _| Ok(()),
            function,
        )?;
        matched.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&outcome, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        match self
            .functions
            .gc_host_imports()
            .source_compilation_profile()
        {
            crate::RuntimeSourceCompilationProfile::Portable => {
                self.emit_reject_dynamic_source(
                    scriptw_ir::DynamicSourceRuntimeOperation::Function(kind),
                    function,
                );
            }
            crate::RuntimeSourceCompilationProfile::NativeHost => {
                self.emit_native_dynamic_function_compile(
                    kind_word,
                    &new_target,
                    &sources,
                    &outcome,
                    function,
                )?;
            }
        }
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(kind_word, function);
        schema.release_i32_local(matched, function);
        outcome.clear(function);
        schema.release_i32_local(index, function);
        pending.clear(function);
        argument.clear(function);
        sources.clear(function);
        schema.release_i32_local(count, function);
        new_target.clear(function);
        Ok(())
    }

    /// The program half: match the converted sources against the finite set of
    /// literal calls prepared at compile time. Status 1 means this hook owns
    /// the completion (a function, or the prepared SyntaxError); status 0 is a
    /// miss and the constructor continues exactly as if nothing was prepared.
    pub(crate) fn compile_prepared_dynamic_function_hook(&mut self) -> Result<Function, EmitError> {
        use crate::gc_types::StringValue;
        use crate::runtime_helpers::{HelperParameters, PreparedDynamicFunctionParameters};
        let mut function = self.begin_helper_body(RuntimeHelperId::PreparedDynamicFunction);
        let parameters = self.helper_parameters::<PreparedDynamicFunctionParameters>(&mut function);
        self.push_scope();
        let schema = self.runtime_schema();
        self.completion().initialize(&mut function);
        let matched = schema.reserve_i32_local(&mut function);
        matched.set_constant(0, &mut function);
        let length = schema.reserve_i32_local(&mut function);
        schema.array_type::<crate::gc_types::ValueArray>().length(
            &parameters.sources,
            schema,
            &mut function,
        );
        length.store(&mut function);
        let case_folding = schema.reserve_i32_local(&mut function);
        case_folding.set_constant(0, &mut function);
        let equal = schema.reserve_i32_local(&mut function);
        let all_equal = schema.reserve_i32_local(&mut function);
        let index = schema.reserve_i32_local(&mut function);
        let argument = schema.reserve_value_local(&mut function);
        let pending = schema.reserve_completion(&mut function);
        pending.initialize(&mut function);
        let entries = self.functions.prepared_dynamic_functions().to_vec();
        let done = self.open_frame(ControlFrameKind::Block, &mut function);
        for entry in &entries {
            parameters.kind.load(&mut function);
            function.instruction(&Instruction::I32Const(dynamic_function_kind_code(
                entry.kind,
            )));
            function.instruction(&Instruction::I32Eq);
            length.load(&mut function);
            function.instruction(&Instruction::I32Const(entry.arguments.len() as i32));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, &mut function);
            function.instruction(&Instruction::I32Const(1));
            all_equal.store(&mut function);
            for (position, expected_source) in entry.arguments.iter().enumerate() {
                index.set_constant(position as i32, &mut function);
                self.emit_argument_vector_entry_to_value(
                    &parameters.sources,
                    index,
                    &argument,
                    &mut function,
                );
                let actual = schema
                    .reserve_gc_local::<StringValue, crate::gc_types::NonNullable>(&mut function)
                    .initialize(
                        argument.cast_reference::<StringValue>(schema, &mut function),
                        &mut function,
                    );
                let expected = schema.reserve_gc_local(&mut function).initialize(
                    self.emit_interned_string_reference(expected_source, &mut function)?,
                    &mut function,
                );
                self.emit_gc_string_equality(
                    &actual,
                    &expected,
                    case_folding,
                    equal,
                    &mut function,
                );
                all_equal.load(&mut function);
                equal.load(&mut function);
                function.instruction(&Instruction::I32And);
                all_equal.store(&mut function);
                expected.clear(&mut function);
                actual.clear(&mut function);
            }
            all_equal.load(&mut function);
            self.open_frame(ControlFrameKind::If, &mut function);
            match &entry.outcome {
                scriptw_ir::PreparedDynamicFunctionOutcome::SyntaxError { .. } => {
                    let message = self.strings.source_runtime_error_message(
                        SourceRuntimeErrorMessage::PreparedFunction(&entry.outcome),
                    )?;
                    self.emit_throw_current_function_realm_error(
                        scriptw_ir::NativeErrorKind::SyntaxError,
                        message,
                        &pending,
                        &mut function,
                    )?;
                    self.completion().copy_from(&pending, &mut function);
                }
                scriptw_ir::PreparedDynamicFunctionOutcome::Compiled { function_id } => {
                    let body = self.functions.get(function_id).cloned().ok_or_else(|| {
                        EmitError::unsupported(
                            "prepared dynamic source has no planned callable body",
                        )
                    })?;
                    self.emit_dynamic_function_allocation(
                        entry.kind,
                        &body,
                        &parameters.new_target,
                        &mut function,
                    )?;
                }
            }
            function.instruction(&Instruction::I32Const(1));
            matched.store(&mut function);
            // Each converted source vector selects at most one prepared entry.
            self.emit_branch_to_target(done, &mut function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.clear(&mut function);
        argument.clear(&mut function);
        schema.release_i32_local(index, &mut function);
        schema.release_i32_local(all_equal, &mut function);
        schema.release_i32_local(equal, &mut function);
        schema.release_i32_local(case_folding, &mut function);
        schema.release_i32_local(length, &mut function);
        self.pop_scope();
        parameters.release(&mut function);
        self.completion().emit(&mut function);
        matched.load(&mut function);
        schema.release_i32_local(matched, &mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    pub(crate) fn compile_dynamic_function_constructor_builtin(
        &mut self,
        kind: DynamicFunctionKind,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_dynamic_constructor_body(kind, function)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};

    #[test]
    fn empty_dynamic_bodies_match_the_source_lowering_execution_plans() {
        for kind in EmptyDerivedFunction::ALL {
            let parsed = parse(kind.source(), ParseOptions::script())
                .expect("canonical empty function source must parse");
            let lowered = scriptw_ir::lower(&parsed);
            assert!(lowered.is_wasm_supported(), "{:?}", lowered.diagnostics);
            let source_function = lowered
                .script
                .expect("script IR")
                .functions
                .into_iter()
                .find(|function| function.name == "anonymous")
                .expect("canonical function must be lowered");
            let generated = kind.body();
            assert_eq!(generated.protocol, source_function.protocol);
            assert_eq!(generated.generator_plan, source_function.generator_plan);
            assert_eq!(generated.resumable_plan, source_function.resumable_plan);
            assert_eq!(generated.body, source_function.body);
            assert_eq!(generated.return_kind, source_function.return_kind);
            assert_eq!(generated.strict, source_function.strict);
            assert_eq!(
                generated.owned_env_bindings, source_function.owned_env_bindings,
                "{:?} activation bindings must match ordinary lowering",
                generated.protocol
            );
            assert_eq!(
                generated.captured_bindings,
                source_function.captured_bindings
            );
        }
    }
}
