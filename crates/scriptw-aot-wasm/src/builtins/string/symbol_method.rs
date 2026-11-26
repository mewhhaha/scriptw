use super::*;
use scriptw_ir::WellKnownSymbol;

#[derive(Clone, Copy)]
pub(super) enum StringSymbolMethodKey {
    Match,
    MatchAll,
    Replace,
    Search,
    Split,
}
impl StringSymbolMethodKey {
    fn symbol(self) -> WellKnownSymbol {
        match self {
            Self::Match => WellKnownSymbol::Match,
            Self::MatchAll => WellKnownSymbol::MatchAll,
            Self::Replace => WellKnownSymbol::Replace,
            Self::Search => WellKnownSymbol::Search,
            Self::Split => WellKnownSymbol::Split,
        }
    }
}

/// GetMethod validates once and retains the acquired method with its receiver.
/// Call consumes this owner, so callers cannot accidentally repeat the Get.
#[must_use]
pub(super) struct NullableStringSymbolMethod {
    receiver: ValueLocals,
    method: ValueLocals,
}
impl NullableStringSymbolMethod {
    pub(super) fn get_method(
        b: &mut FunctionBuilder<'_>,
        receiver: &ValueLocals,
        key: StringSymbolMethodKey,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<Self, EmitError> {
        let s = b.runtime_schema();
        let captured = s.reserve_value_local(f);
        let method = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        captured.copy_from(receiver, f);
        method.set_undefined(f);
        b.emit_is_heap_object_like_tag_i32(receiver.tag(), f);
        b.open_frame(ControlFrameKind::If, f);
        let symbol = s
            .reserve_gc_local(f)
            .initialize(b.emit_well_known_symbol_reference(key.symbol(), f)?, f);
        let property = PropertyKeyLocals::from_symbol(s, &symbol, f);
        symbol.clear(f);
        b.emit_object_read(receiver, receiver, &property, &pending, f)?;
        property.clear(f);
        b.emit_native_string_abrupt_exit(&pending, output, exit, f);
        method.copy_from(pending.value(), f);
        b.compile_nullish_tagged_i32(method.tag(), f)?;
        f.instruction(&Instruction::I32Eqz);
        b.open_frame(ControlFrameKind::If, f);
        b.emit_is_callable_i32(&method, f)?;
        f.instruction(&Instruction::I32Eqz);
        b.emit_native_string_error_if(
            RuntimeErrorMessage::STRING_PROTOTYPE_SYMBOL_HOOK_IS_NOT_CALLABLE,
            NativeErrorKind::TypeError,
            output,
            exit,
            f,
        )?;
        b.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        b.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        pending.clear(f);
        Ok(Self {
            receiver: captured,
            method,
        })
    }
    pub(super) fn call_or_fallback(
        self,
        b: &mut FunctionBuilder<'_>,
        arguments: &[&ValueLocals],
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
        fallback: impl FnOnce(&mut FunctionBuilder<'_>, &mut Function) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        b.compile_nullish_tagged_i32(self.method.tag(), f)?;
        b.open_frame(ControlFrameKind::If, f);
        fallback(b, f)?;
        f.instruction(&Instruction::Else);
        let vector = b.emit_pre_evaluated_arg_vector(arguments, f);
        b.emit_function_or_proxy_call_with_argv(&self.method, &self.receiver, &vector, output, f)?;
        vector.clear(f);
        b.emit_native_string_abrupt_exit(output, output, exit, f);
        b.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.method.clear(f);
        self.receiver.clear(f);
        Ok(())
    }
}

/// Required Invoke consumes the acquired method and owns its missing-method error.
#[must_use]
struct RequiredStringSymbolMethod(NullableStringSymbolMethod);
impl RequiredStringSymbolMethod {
    fn get_method(
        b: &mut FunctionBuilder<'_>,
        receiver: &ValueLocals,
        key: StringSymbolMethodKey,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<Self, EmitError> {
        NullableStringSymbolMethod::get_method(b, receiver, key, output, exit, f).map(Self)
    }
    fn invoke(
        self,
        b: &mut FunctionBuilder<'_>,
        arguments: &[&ValueLocals],
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.0
            .call_or_fallback(b, arguments, output, exit, f, |b, f| {
                b.emit_throw_current_function_realm_error(
                    NativeErrorKind::TypeError,
                    RuntimeErrorMessage::STRING_PROTOTYPE_SYMBOL_HOOK_IS_NOT_CALLABLE,
                    output,
                    f,
                )?;
                b.emit_branch_to_target(exit, f);
                Ok(())
            })
    }
}

enum NativeStringProtocol {
    Match,
    MatchAll,
    Replace,
    ReplaceAll,
    Search,
    Split,
}
impl NativeStringProtocol {
    fn key(&self) -> StringSymbolMethodKey {
        match self {
            Self::Match => StringSymbolMethodKey::Match,
            Self::MatchAll => StringSymbolMethodKey::MatchAll,
            Self::Replace | Self::ReplaceAll => StringSymbolMethodKey::Replace,
            Self::Search => StringSymbolMethodKey::Search,
            Self::Split => StringSymbolMethodKey::Split,
        }
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_string_match_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_native_string_protocol(NativeStringProtocol::Match, f)
    }
    pub(crate) fn emit_string_match_all_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_protocol(NativeStringProtocol::MatchAll, f)
    }
    pub(crate) fn emit_string_replace_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_protocol(NativeStringProtocol::Replace, f)
    }
    pub(crate) fn emit_string_replace_all_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_protocol(NativeStringProtocol::ReplaceAll, f)
    }
    pub(crate) fn emit_string_search_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_native_string_protocol(NativeStringProtocol::Search, f)
    }
    pub(crate) fn emit_string_split_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_native_string_protocol(NativeStringProtocol::Split, f)
    }

    fn emit_native_string_protocol(
        &mut self,
        operation: NativeStringProtocol,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let pattern = s.reserve_value_local(f);
        let second = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let output = s.reserve_completion(f);
        let is_regexp = s.reserve_i32_local(f);
        let global = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_builtin_arg_to_value(0, &pattern, f);
        self.emit_builtin_arg_to_value(1, &second, f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_native_string_require_coercible(&receiver, &output, exit, f)?;
        match &operation {
            NativeStringProtocol::MatchAll | NativeStringProtocol::ReplaceAll => {
                // IsRegExp(argument) step 1 returns false for any non-Object
                // (including null, undefined, and primitive Strings).
                self.emit_is_heap_object_like_tag_i32(pattern.tag(), f);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_string_search_argument_is_regexp_to_local(
                    &pattern, is_regexp, &pending, f,
                )?;
                self.emit_native_string_abrupt_exit(&pending, &output, exit, f);
                is_regexp.load(f);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_native_regexp_get(&pattern, "flags", &pending, f)?;
                self.emit_native_string_abrupt_exit(&pending, &output, exit, f);
                self.emit_native_string_require_coercible(pending.value(), &output, exit, f)?;
                let flag_value = s.reserve_value_local(f);
                flag_value.copy_from(pending.value(), f);
                self.emit_value_to_string_payload(&flag_value, &pending, f)?;
                self.emit_native_string_abrupt_exit(&pending, &output, exit, f);
                let flags = s
                    .reserve_gc_local(f)
                    .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
                self.emit_native_regexp_has_flag(
                    &flags,
                    super::regexp_protocol::NativeRegExpFlag::Global,
                    global,
                    f,
                );
                global.load(f);
                f.instruction(&Instruction::I32Eqz);
                self.emit_native_string_error_if(
                    RuntimeErrorMessage::STRING_METHOD_REGEXP_FLAGS_MUST_CONTAIN_G,
                    NativeErrorKind::TypeError,
                    &output,
                    exit,
                    f,
                )?;
                flags.clear(f);
                flag_value.clear(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
            NativeStringProtocol::Match
            | NativeStringProtocol::Replace
            | NativeStringProtocol::Search
            | NativeStringProtocol::Split => {}
        }
        let method = NullableStringSymbolMethod::get_method(
            self,
            &pattern,
            operation.key(),
            &output,
            exit,
            f,
        )?;
        let arguments = match &operation {
            NativeStringProtocol::Match
            | NativeStringProtocol::MatchAll
            | NativeStringProtocol::Search => vec![&receiver],
            NativeStringProtocol::Replace
            | NativeStringProtocol::ReplaceAll
            | NativeStringProtocol::Split => vec![&receiver, &second],
        };
        method.call_or_fallback(self, &arguments, &output, exit, f, |b, f| {
            b.emit_value_to_string_payload(&receiver, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, &output, exit, f);
            let input = s
                .reserve_gc_local(f)
                .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
            match operation {
                NativeStringProtocol::Match
                | NativeStringProtocol::MatchAll
                | NativeStringProtocol::Search => {
                    let regexp = s.reserve_value_local(f);
                    let flags = s.reserve_value_local(f);
                    match &operation {
                        NativeStringProtocol::MatchAll => {
                            let g = s
                                .reserve_gc_local(f)
                                .initialize(b.emit_native_string_static("g", f), f);
                            flags.set_reference(&g, s, f);
                            g.clear(f);
                        }
                        NativeStringProtocol::Match
                        | NativeStringProtocol::Replace
                        | NativeStringProtocol::ReplaceAll
                        | NativeStringProtocol::Search
                        | NativeStringProtocol::Split => flags.set_undefined(f),
                    }
                    let created =
                        b.emit_native_regexp_create(&pattern, &flags, &output, exit, f)?;
                    regexp.set_reference(&created, s, f);
                    created.clear(f);
                    let required = RequiredStringSymbolMethod::get_method(
                        b,
                        &regexp,
                        operation.key(),
                        &output,
                        exit,
                        f,
                    )?;
                    let value = s.reserve_value_local(f);
                    value.set_reference(&input, s, f);
                    required.invoke(b, &[&value], &output, exit, f)?;
                    value.clear(f);
                    flags.clear(f);
                    regexp.clear(f);
                }
                NativeStringProtocol::Replace => b
                    .emit_string_replace_literal_first_occurrence_from_string_locals(
                        &input, &pattern, &second, &output, exit, f,
                    )?,
                NativeStringProtocol::ReplaceAll => b
                    .emit_string_replace_literal_all_occurrences_from_string_locals(
                        &input, &pattern, &second, &output, exit, f,
                    )?,
                NativeStringProtocol::Split => {
                    b.emit_native_string_split_literal(&input, &pattern, &second, &output, exit, f)?
                }
            }
            input.clear(f);
            Ok(())
        })?;
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        s.release_i32_local(global, f);
        s.release_i32_local(is_regexp, f);
        output.clear(f);
        pending.clear(f);
        second.clear(f);
        pattern.clear(f);
        receiver.clear(f);
        Ok(())
    }
}
