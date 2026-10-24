use super::replacement_pieces::ReplacementPieces;
use super::*;
use crate::functions::{NonArrayRealmIntrinsicSlot, OrdinaryDefaultPrototype};
use crate::runtime_helpers::{
    RegExpCompilerArguments, RegExpCompilerStatus, RegExpProgramCandidateArguments,
};
mod source_escape;

#[derive(Clone, Copy)]
pub(super) enum NativeRegExpFlag {
    HasIndices,
    Global,
    IgnoreCase,
    Multiline,
    DotAll,
    Unicode,
    UnicodeSets,
    Sticky,
}
impl NativeRegExpFlag {
    pub(super) const ALL: [Self; 8] = [
        Self::HasIndices,
        Self::Global,
        Self::IgnoreCase,
        Self::Multiline,
        Self::DotAll,
        Self::Unicode,
        Self::UnicodeSets,
        Self::Sticky,
    ];
    pub(super) const fn unit(self) -> i32 {
        match self {
            Self::HasIndices => 100,
            Self::Global => 103,
            Self::IgnoreCase => 105,
            Self::Multiline => 109,
            Self::DotAll => 115,
            Self::Unicode => 117,
            Self::UnicodeSets => 118,
            Self::Sticky => 121,
        }
    }
    pub(super) const fn mask(self) -> i32 {
        match self {
            Self::HasIndices => 1,
            Self::Global => 2,
            Self::IgnoreCase => 4,
            Self::Multiline => 8,
            Self::DotAll => 16,
            Self::Unicode => 32,
            Self::UnicodeSets => 64,
            Self::Sticky => 128,
        }
    }
    fn property(self) -> &'static str {
        match self {
            Self::HasIndices => "hasIndices",
            Self::Global => "global",
            Self::IgnoreCase => "ignoreCase",
            Self::Multiline => "multiline",
            Self::DotAll => "dotAll",
            Self::Unicode => "unicode",
            Self::UnicodeSets => "unicodeSets",
            Self::Sticky => "sticky",
        }
    }
    fn spelling(self) -> &'static str {
        match self {
            Self::HasIndices => "d",
            Self::Global => "g",
            Self::IgnoreCase => "i",
            Self::Multiline => "m",
            Self::DotAll => "s",
            Self::Unicode => "u",
            Self::UnicodeSets => "v",
            Self::Sticky => "y",
        }
    }
}

/// This owner exists only after flag validation and successful pure compilation.
/// Publication consumes every semantic edge together.
#[must_use]
struct CompletedRegExpConstruction {
    header: GcLocal<OrdinaryObject>,
    source: GcLocal<StringValue>,
    flags: GcLocal<StringValue>,
    program: GcLocal<RegExpProgram>,
    realm: GcLocal<RealmRecord>,
    legacy_enabled: I32Local,
}
impl CompletedRegExpConstruction {
    fn publish(
        self,
        b: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) -> GcStackReference<RegExpObject> {
        let s = b.runtime_schema();
        let value = s.struct_type::<RegExpObject>().construct(
            (
                GcOperand::reference(&self.header, s),
                GcOperand::reference(&self.source, s),
                GcOperand::reference(&self.flags, s),
                GcOperand::nullable_reference(&self.program, s),
                GcOperand::reference(&self.realm, s),
                GcOperand::boolean_local(self.legacy_enabled),
            ),
            f,
        );
        self.realm.clear(f);
        s.release_i32_local(self.legacy_enabled, f);
        self.program.clear(f);
        self.flags.clear(f);
        self.source.clear(f);
        self.header.clear(f);
        value
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_native_regexp_key(&self, name: &str, f: &mut Function) -> PropertyKeyLocals {
        let s = self.runtime_schema();
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_native_string_static(name, f), f);
        let key = PropertyKeyLocals::from_string(s, &text, f);
        text.clear(f);
        key
    }

    pub(super) fn emit_native_regexp_get(
        &mut self,
        receiver: &ValueLocals,
        name: &str,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let key = self.emit_native_regexp_key(name, f);
        self.emit_object_read(receiver, receiver, &key, result, f)?;
        key.clear(f);
        Ok(())
    }

    pub(super) fn emit_native_regexp_set_last_index(
        &mut self,
        receiver: &ValueLocals,
        index: I64Local,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let bits = s.reserve_i64_local(f);
        let value = s.reserve_value_local(f);
        index.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        bits.store(f);
        value.set_number(bits, f);
        let key = self.emit_native_regexp_key("lastIndex", f);
        self.emit_object_write_strict(receiver, &key, &value, result, f)?;
        key.clear(f);
        value.clear(f);
        s.release_i64_local(bits, f);
        Ok(())
    }

    pub(super) fn emit_native_regexp_has_flag(
        &mut self,
        flags: &GcLocal<StringValue>,
        flag: NativeRegExpFlag,
        out: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        self.emit_native_gc_string_length(flags, length, f);
        f.instruction(&Instruction::I32Const(0));
        out.store(f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        self.emit_gc_string_code_unit_i32(flags, index, f);
        f.instruction(&Instruction::I32Const(flag.unit()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I32Const(1));
        out.store(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i64_local(index, f);
        s.release_i64_local(length, f);
    }

    fn emit_native_regexp_validate_flags(
        &mut self,
        flags: &GcLocal<StringValue>,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let unit = s.reserve_i32_local(f);
        let bit = s.reserve_i32_local(f);
        let seen = s.reserve_i32_local(f);
        self.emit_native_gc_string_length(flags, length, f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        f.instruction(&Instruction::I32Const(0));
        seen.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        self.emit_gc_string_code_unit_i32(flags, index, f);
        unit.store(f);
        f.instruction(&Instruction::I32Const(0));
        bit.store(f);
        for flag in NativeRegExpFlag::ALL {
            unit.load(f);
            f.instruction(&Instruction::I32Const(flag.unit()));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I32Const(flag.mask()));
            bit.store(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        bit.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::INVALID_REGULAR_EXPRESSION_FLAG,
            NativeErrorKind::SyntaxError,
            output,
            exit,
            f,
        )?;
        bit.load(f);
        seen.load(f);
        f.instruction(&Instruction::I32And);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::DUPLICATE_REGULAR_EXPRESSION_FLAG,
            NativeErrorKind::SyntaxError,
            output,
            exit,
            f,
        )?;
        seen.load(f);
        bit.load(f);
        f.instruction(&Instruction::I32Or);
        seen.store(f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        seen.load(f);
        f.instruction(&Instruction::I32Const(
            NativeRegExpFlag::Unicode.mask() | NativeRegExpFlag::UnicodeSets.mask(),
        ));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Const(
            NativeRegExpFlag::Unicode.mask() | NativeRegExpFlag::UnicodeSets.mask(),
        ));
        f.instruction(&Instruction::I32Eq);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::INVALID_REGULAR_EXPRESSION_FLAG,
            NativeErrorKind::SyntaxError,
            output,
            exit,
            f,
        )?;
        s.release_i32_local(seen, f);
        s.release_i32_local(bit, f);
        s.release_i32_local(unit, f);
        s.release_i64_local(index, f);
        s.release_i64_local(length, f);
        Ok(())
    }

    fn emit_native_regexp_compile_program(
        &mut self,
        source: &GcLocal<StringValue>,
        flags: &GcLocal<StringValue>,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<RegExpProgram>, EmitError> {
        let s = self.runtime_schema();
        self.emit_native_regexp_validate_flags(flags, output, exit, f)?;
        let status = s.reserve_i32_local(f);
        let cursor = s.reserve_i64_local(f);
        let detail = s.reserve_i64_local(f);
        let compiled = s
            .reserve_gc_local::<RegExpProgram, Nullable>(f)
            .initialize_null(s, f);
        status.set_constant(RegExpCompilerStatus::Compiled.abi_word() as i32, f);
        self.emit_program_hook_dispatch::<crate::gc_types::RegExpProgramCandidateHook>(
            |_, hook, f| {
                let candidate = s
                    .call_hook(hook, RegExpProgramCandidateArguments::new(source, flags), f)
                    .bind(
                        s,
                        s.reserve_gc_local::<RegExpProgram, Nullable>(f),
                        status,
                        cursor,
                        detail,
                        f,
                    );
                compiled.replace(candidate.load(s, f), f);
                candidate.clear(f);
                Ok(())
            },
            |_, _| Ok(()),
            f,
        )?;
        // A cached syntax/resource failure is an outcome, not a cache miss.
        self.emit_native_regexp_compiler_failures(status, output, exit, f)?;
        compiled.load(s, f);
        f.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, f);
        let fallback = s
            .call_helper(
                RegExpCompilerArguments::new(source, flags),
                self.runtime_helper_base()?,
                f,
            )
            .bind(
                s,
                s.reserve_gc_local::<RegExpProgram, Nullable>(f),
                status,
                cursor,
                detail,
                f,
            );
        compiled.replace(fallback.load(s, f), f);
        fallback.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_native_regexp_compiler_failures(status, output, exit, f)?;
        status.load(f);
        f.instruction(&Instruction::I32Const(
            RegExpCompilerStatus::Compiled.abi_word() as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        compiled.load(s, f);
        f.instruction(&Instruction::RefIsNull);
        f.instruction(&Instruction::I32Or);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::REGEXP_RUNTIME_COMPILER_PRODUCED_AN_INVALID_PROGRAM,
            NativeErrorKind::Error,
            output,
            exit,
            f,
        )?;
        let program = s
            .reserve_gc_local(f)
            .initialize(compiled.load(s, f).require_non_null(f), f);
        compiled.clear(f);
        s.release_i64_local(detail, f);
        s.release_i64_local(cursor, f);
        s.release_i32_local(status, f);
        Ok(program)
    }

    fn emit_native_regexp_compiler_failures(
        &mut self,
        status: I32Local,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        for outcome in RegExpCompilerStatus::ALL {
            let error=match outcome {
                RegExpCompilerStatus::Compiled=>None,
                RegExpCompilerStatus::SyntaxError=>Some((NativeErrorKind::SyntaxError,RuntimeErrorMessage::INVALID_REGULAR_EXPRESSION_PATTERN)),
                RegExpCompilerStatus::ResourceExhausted=>Some((NativeErrorKind::RangeError,RuntimeErrorMessage::REGEXP_RUNTIME_COMPILER_EXCEEDED_ITS_ADDRESSABLE_RESOURCE_LIMIT)),
                RegExpCompilerStatus::CorruptProgram=>Some((NativeErrorKind::Error,RuntimeErrorMessage::REGEXP_RUNTIME_COMPILER_PRODUCED_AN_INVALID_PROGRAM)),
            };
            if let Some((kind, message)) = error {
                status.load(f);
                f.instruction(&Instruction::I32Const(outcome.abi_word() as i32));
                f.instruction(&Instruction::I32Eq);
                self.emit_native_string_error_if(message, kind, output, exit, f)?;
            }
        }
        Ok(())
    }

    fn emit_native_regexp_string_argument(
        &mut self,
        value: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<StringValue>, EmitError> {
        let s = self.runtime_schema();
        let converted = s.reserve_completion(f);
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let empty = s
            .reserve_gc_local(f)
            .initialize(self.emit_native_string_static("", f), f);
        self.emit_native_string_normal_reference(&empty, &converted, f);
        empty.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_value_to_string_payload(value, &converted, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_native_string_abrupt_exit(&converted, output, exit, f);
        let text = s
            .reserve_gc_local(f)
            .initialize(converted.value().cast_reference::<StringValue>(s, f), f);
        converted.clear(f);
        Ok(text)
    }

    pub(crate) fn emit_regexp_constructor_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let pattern = s.reserve_value_local(f);
        let flags = s.reserve_value_local(f);
        let new_target = s.reserve_value_local(f);
        let active = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let output = s.reserve_completion(f);
        let is_regexp = s.reserve_i32_local(f);
        let function = s.reserve_gc_local(f).initialize(
            self.body_entry_locals()
                .expect("RegExp constructor")
                .function_object()
                .expect("actual RegExp callee")
                .load(s, f),
            f,
        );
        active.set_reference(&function, s, f);
        function.clear(f);
        self.emit_builtin_arg_to_value(0, &pattern, f);
        self.emit_builtin_arg_to_value(1, &flags, f);
        self.compile_new_target_to_locals(&new_target, f)?;
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_string_search_argument_is_regexp_to_local(&pattern, is_regexp, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, &output, exit, f);
        new_target.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        new_target.copy_from(&active, f);
        is_regexp.load(f);
        flags.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_native_regexp_get(&pattern, "constructor", &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, &output, exit, f);
        self.emit_tagged_payload_same_value_i32(pending.value(), &active, f)?;
        self.open_frame(ControlFrameKind::If, f);
        output.set_normal(&pattern, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        pattern.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<RegExpObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let branded = s
            .reserve_gc_local(f)
            .initialize(pattern.cast_reference::<RegExpObject>(s, f), f);
        flags.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let text = s.reserve_gc_local(f).initialize(
            s.struct_type::<RegExpObject>()
                .field(RegExpObjectSchema::ORIGINAL_FLAGS)
                .read(&branded, s, f)
                .reference(),
            f,
        );
        flags.set_reference(&text, s, f);
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let text = s.reserve_gc_local(f).initialize(
            s.struct_type::<RegExpObject>()
                .field(RegExpObjectSchema::SOURCE)
                .read(&branded, s, f)
                .reference(),
            f,
        );
        pattern.set_reference(&text, s, f);
        text.clear(f);
        branded.clear(f);
        f.instruction(&Instruction::Else);
        is_regexp.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let original = s.reserve_value_local(f);
        original.copy_from(&pattern, f);
        self.emit_native_regexp_get(&original, "source", &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, &output, exit, f);
        pattern.copy_from(pending.value(), f);
        flags.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_native_regexp_get(&original, "flags", &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, &output, exit, f);
        flags.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        original.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // RegExpAlloc observes the selected prototype before source/flags coercion.
        self.emit_get_prototype_from_constructor(
            &new_target,
            OrdinaryDefaultPrototype::RegExp,
            &pending,
            f,
        )?;
        self.emit_native_string_abrupt_exit(&pending, &output, exit, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(pending.value()), f)?,
            f,
        );
        let zero = s.reserve_value_local(f);
        let zero_bits = s.reserve_i64_local(f);
        f.instruction(&Instruction::I64Const(0));
        zero_bits.store(f);
        zero.set_number(zero_bits, f);
        s.release_i64_local(zero_bits, f);
        let key = self.emit_native_regexp_key("lastIndex", f);
        self.emit_object_append_data_property_with_flags(
            &header, &key, &zero, true, false, false, f,
        )?;
        key.clear(f);
        zero.clear(f);
        let source = self.emit_native_regexp_string_argument(&pattern, &output, exit, f)?;
        let flags_string = self.emit_native_regexp_string_argument(&flags, &output, exit, f)?;
        let program =
            self.emit_native_regexp_compile_program(&source, &flags_string, &output, exit, f)?;
        let realm = self.emit_execution_realm(f);
        let legacy_enabled = s.reserve_i32_local(f);
        self.emit_tagged_payload_same_value_i32(&new_target, &active, f)?;
        legacy_enabled.store(f);
        let object = s.reserve_gc_local(f).initialize(
            CompletedRegExpConstruction {
                header,
                source,
                flags: flags_string,
                program,
                realm,
                legacy_enabled,
            }
            .publish(self, f),
            f,
        );
        let value = s.reserve_value_local(f);
        value.set_reference(&object, s, f);
        output.set_normal(&value, f);
        value.clear(f);
        object.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        s.release_i32_local(is_regexp, f);
        output.clear(f);
        pending.clear(f);
        active.clear(f);
        new_target.clear(f);
        flags.clear(f);
        pattern.clear(f);
        Ok(())
    }

    pub(crate) fn emit_regexp_prototype_compile_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let pattern = s.reserve_value_local(f);
        let flags = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let output = s.reserve_completion(f);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_builtin_arg_to_value(0, &pattern, f);
        self.emit_builtin_arg_to_value(1, &flags, f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        receiver.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<RegExpObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::REGEXP_PROTOTYPE_EXEC_RECEIVER_IS_NOT_REGEXP,
            NativeErrorKind::TypeError,
            &output,
            exit,
            f,
        )?;
        let object = s
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<RegExpObject>(s, f), f);
        self.emit_regexp_legacy_compile_guard(&object, &output, exit, f)?;
        pattern.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<RegExpObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        flags.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.emit_native_string_error_if(RuntimeErrorMessage::REGEXP_PROTOTYPE_COMPILE_FLAGS_MUST_BE_UNDEFINED_WHEN_PATTERN_IS_REGEXP,NativeErrorKind::TypeError,&output,exit,f)?;
        let original = s
            .reserve_gc_local(f)
            .initialize(pattern.cast_reference::<RegExpObject>(s, f), f);
        let text = s.reserve_gc_local(f).initialize(
            s.struct_type::<RegExpObject>()
                .field(RegExpObjectSchema::ORIGINAL_FLAGS)
                .read(&original, s, f)
                .reference(),
            f,
        );
        flags.set_reference(&text, s, f);
        text.clear(f);
        let text = s.reserve_gc_local(f).initialize(
            s.struct_type::<RegExpObject>()
                .field(RegExpObjectSchema::SOURCE)
                .read(&original, s, f)
                .reference(),
            f,
        );
        pattern.set_reference(&text, s, f);
        text.clear(f);
        original.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let source = self.emit_native_regexp_string_argument(&pattern, &output, exit, f)?;
        let flags_string = self.emit_native_regexp_string_argument(&flags, &output, exit, f)?;
        let program =
            self.emit_native_regexp_compile_program(&source, &flags_string, &output, exit, f)?;
        // Failed conversion/compilation retains all old slots. Successful
        // initialization publishes new slots before the strict lastIndex Set.
        s.struct_type::<RegExpObject>()
            .field(RegExpObjectSchema::SOURCE)
            .write(&object, GcOperand::reference(&source, s), s, f);
        s.struct_type::<RegExpObject>()
            .field(RegExpObjectSchema::ORIGINAL_FLAGS)
            .write(&object, GcOperand::reference(&flags_string, s), s, f);
        s.struct_type::<RegExpObject>()
            .field(RegExpObjectSchema::PROGRAM)
            .write(&object, GcOperand::nullable_reference(&program, s), s, f);
        program.clear(f);
        flags_string.clear(f);
        source.clear(f);
        let index = s.reserve_i64_local(f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        self.emit_native_regexp_set_last_index(&receiver, index, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, &output, exit, f);
        output.set_normal(&receiver, f);
        s.release_i64_local(index, f);
        object.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        pending.clear(f);
        flags.clear(f);
        pattern.clear(f);
        receiver.clear(f);
        Ok(())
    }

    pub(crate) fn emit_regexp_prototype_flags_getter_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let output = s.reserve_completion(f);
        self.compile_this_to_locals(&receiver, f)?;
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_is_heap_object_like_tag_i32(receiver.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::REGEXP_PROTOTYPE_FLAGS_GETTER_RECEIVER_IS_NOT_AN_OBJECT,
            NativeErrorKind::TypeError,
            &output,
            exit,
            f,
        )?;
        let accumulated = s
            .reserve_gc_local(f)
            .initialize(self.emit_native_string_static("", f), f);
        for flag in NativeRegExpFlag::ALL {
            self.emit_native_regexp_get(&receiver, flag.property(), &pending, f)?;
            self.emit_native_string_abrupt_exit(&pending, &output, exit, f);
            self.compile_truthy_tagged_i32(pending.value(), f)?;
            self.open_frame(ControlFrameKind::If, f);
            let part = s
                .reserve_gc_local(f)
                .initialize(self.emit_native_string_static(flag.spelling(), f), f);
            accumulated.replace(self.emit_concat_gc_strings(&accumulated, &part, f), f);
            part.clear(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        self.emit_native_string_normal_reference(&accumulated, &output, f);
        accumulated.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        pending.clear(f);
        receiver.clear(f);
        Ok(())
    }

    fn emit_native_regexp_receiver_is_executing_prototype(
        &mut self,
        receiver: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let prototype = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::RegExpPrototype,
            &prototype,
            f,
        );
        self.emit_tagged_payload_same_value_i32(receiver, &prototype, f)?;
        prototype.clear(f);
        realm.clear(f);
        Ok(())
    }

    fn emit_native_regexp_flag_getter(
        &mut self,
        flag: NativeRegExpFlag,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let output = s.reserve_completion(f);
        let value = s.reserve_value_local(f);
        let present = s.reserve_i32_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        receiver.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<RegExpObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let object = s
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<RegExpObject>(s, f), f);
        let flags = s.reserve_gc_local(f).initialize(
            s.struct_type::<RegExpObject>()
                .field(RegExpObjectSchema::ORIGINAL_FLAGS)
                .read(&object, s, f)
                .reference(),
            f,
        );
        self.emit_native_regexp_has_flag(&flags, flag, present, f);
        value.set_boolean(present, f);
        output.set_normal(&value, f);
        flags.clear(f);
        object.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_native_regexp_receiver_is_executing_prototype(&receiver, f)?;
        self.open_frame(ControlFrameKind::If, f);
        value.set_undefined(f);
        output.set_normal(&value, f);
        f.instruction(&Instruction::Else);
        self.emit_throw_current_function_realm_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::REGEXP_PROTOTYPE_EXEC_RECEIVER_IS_NOT_REGEXP,
            &output,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        s.release_i32_local(present, f);
        value.clear(f);
        output.clear(f);
        receiver.clear(f);
        Ok(())
    }
    pub(crate) fn emit_regexp_prototype_has_indices_getter_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_regexp_flag_getter(NativeRegExpFlag::HasIndices, f)
    }
    pub(crate) fn emit_regexp_prototype_global_getter_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_regexp_flag_getter(NativeRegExpFlag::Global, f)
    }
    pub(crate) fn emit_regexp_prototype_ignore_case_getter_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_regexp_flag_getter(NativeRegExpFlag::IgnoreCase, f)
    }
    pub(crate) fn emit_regexp_prototype_multiline_getter_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_regexp_flag_getter(NativeRegExpFlag::Multiline, f)
    }
    pub(crate) fn emit_regexp_prototype_dot_all_getter_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_regexp_flag_getter(NativeRegExpFlag::DotAll, f)
    }
    pub(crate) fn emit_regexp_prototype_unicode_getter_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_regexp_flag_getter(NativeRegExpFlag::Unicode, f)
    }
    pub(crate) fn emit_regexp_prototype_unicode_sets_getter_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_regexp_flag_getter(NativeRegExpFlag::UnicodeSets, f)
    }
    pub(crate) fn emit_regexp_prototype_sticky_getter_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_regexp_flag_getter(NativeRegExpFlag::Sticky, f)
    }

    pub(crate) fn emit_regexp_prototype_to_string_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let converted = s.reserve_completion(f);
        let output = s.reserve_completion(f);
        self.compile_this_to_locals(&receiver, f)?;
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_is_heap_object_like_tag_i32(receiver.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::REGEXP_PROTOTYPE_TEST_RECEIVER_IS_NOT_AN_OBJECT,
            NativeErrorKind::TypeError,
            &output,
            exit,
            f,
        )?;
        let accumulated = s
            .reserve_gc_local(f)
            .initialize(self.emit_native_string_static("/", f), f);
        for name in ["source", "flags"] {
            self.emit_native_regexp_get(&receiver, name, &pending, f)?;
            self.emit_native_string_abrupt_exit(&pending, &output, exit, f);
            self.emit_value_to_string_payload(pending.value(), &converted, f)?;
            self.emit_native_string_abrupt_exit(&converted, &output, exit, f);
            let part = s
                .reserve_gc_local(f)
                .initialize(converted.value().cast_reference::<StringValue>(s, f), f);
            accumulated.replace(self.emit_concat_gc_strings(&accumulated, &part, f), f);
            part.clear(f);
            if name == "source" {
                let slash = s
                    .reserve_gc_local(f)
                    .initialize(self.emit_native_string_static("/", f), f);
                accumulated.replace(self.emit_concat_gc_strings(&accumulated, &slash, f), f);
                slash.clear(f);
            }
        }
        self.emit_native_string_normal_reference(&accumulated, &output, f);
        accumulated.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        converted.clear(f);
        pending.clear(f);
        receiver.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_regexp_prototype_source_getter_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let output = s.reserve_completion(f);
        self.compile_this_to_locals(&receiver, f)?;
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        receiver.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<RegExpObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let object = s
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<RegExpObject>(s, f), f);
        let source = s.reserve_gc_local(f).initialize(
            s.struct_type::<RegExpObject>()
                .field(RegExpObjectSchema::SOURCE)
                .read(&object, s, f)
                .reference(),
            f,
        );
        let length = s.reserve_i64_local(f);
        self.emit_native_gc_string_length(&source, length, f);
        length.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let empty = s.reserve_gc_local(f).initialize(
            self.emit_native_string_static(scriptw_ir::REGEXP_EMPTY_SOURCE, f),
            f,
        );
        self.emit_native_string_normal_reference(&empty, &output, f);
        empty.clear(f);
        f.instruction(&Instruction::Else);
        let escaped = self.emit_escaped_regexp_source(&source, &output, exit, f)?;
        self.emit_native_string_normal_reference(&escaped, &output, f);
        escaped.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(length, f);
        source.clear(f);
        object.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_native_regexp_receiver_is_executing_prototype(&receiver, f)?;
        self.open_frame(ControlFrameKind::If, f);
        let empty = s.reserve_gc_local(f).initialize(
            self.emit_native_string_static(scriptw_ir::REGEXP_EMPTY_SOURCE, f),
            f,
        );
        self.emit_native_string_normal_reference(&empty, &output, f);
        empty.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_throw_current_function_realm_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::REGEXP_PROTOTYPE_EXEC_RECEIVER_IS_NOT_REGEXP,
            &output,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        receiver.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    fn emit_with_native_regexp_protocol(
        &mut self,
        f: &mut Function,
        consume: impl FnOnce(
            &mut Self,
            &ValueLocals,
            &GcLocal<StringValue>,
            &CompletionLocals,
            ControlTarget,
            &mut Function,
        ) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let argument = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let output = s.reserve_completion(f);
        self.compile_this_to_locals(&receiver, f)?;
        self.emit_builtin_arg_to_value(0, &argument, f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_is_heap_object_like_tag_i32(receiver.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::REGEXP_PROTOTYPE_TEST_RECEIVER_IS_NOT_AN_OBJECT,
            NativeErrorKind::TypeError,
            &output,
            exit,
            f,
        )?;
        self.emit_value_to_string_payload(&argument, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, &output, exit, f);
        let input = s
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        consume(self, &receiver, &input, &output, exit, f)?;
        input.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        pending.clear(f);
        argument.clear(f);
        receiver.clear(f);
        Ok(())
    }

    pub(super) fn emit_native_regexp_observed_exec(
        &mut self,
        regexp: &ValueLocals,
        input: &GcLocal<StringValue>,
        output: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let pending = s.reserve_completion(f);
        let acquired = s.reserve_value_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_native_regexp_get(regexp, "exec", &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        acquired.copy_from(pending.value(), f);
        self.emit_regexp_exec_from_values(regexp, input, &acquired, output, f)?;
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        acquired.clear(f);
        pending.clear(f);
        Ok(())
    }

    fn emit_native_regexp_protocol_flags(
        &mut self,
        regexp: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<StringValue>, EmitError> {
        let s = self.runtime_schema();
        let pending = s.reserve_completion(f);
        let value = s.reserve_value_local(f);
        self.emit_native_regexp_get(regexp, "flags", &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        value.copy_from(pending.value(), f);
        self.emit_value_to_string_payload(&value, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        let flags = s
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        value.clear(f);
        pending.clear(f);
        Ok(flags)
    }

    fn emit_native_regexp_protocol_unicode(
        &mut self,
        flags: &GcLocal<StringValue>,
        unicode: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let second = s.reserve_i32_local(f);
        self.emit_native_regexp_has_flag(flags, NativeRegExpFlag::Unicode, unicode, f);
        self.emit_native_regexp_has_flag(flags, NativeRegExpFlag::UnicodeSets, second, f);
        unicode.load(f);
        second.load(f);
        f.instruction(&Instruction::I32Or);
        unicode.store(f);
        s.release_i32_local(second, f);
    }

    fn emit_native_regexp_species(
        &mut self,
        regexp: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let s = self.runtime_schema();
        let selected = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let constructor = s.reserve_value_local(f);
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::RegExpConstructor,
            &selected,
            f,
        );
        realm.clear(f);
        self.emit_native_regexp_get(regexp, "constructor", &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        constructor.copy_from(pending.value(), f);
        constructor.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_heap_object_like_tag_i32(constructor.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::REGEXP_PROTOTYPE_SYMBOL_SPLIT_CONSTRUCTOR_IS_NOT_AN_OBJECT,
            NativeErrorKind::TypeError,
            output,
            exit,
            f,
        )?;
        let symbol = s.reserve_gc_local(f).initialize(
            self.emit_well_known_symbol_reference(scriptw_ir::WellKnownSymbol::Species, f)?,
            f,
        );
        let key = PropertyKeyLocals::from_symbol(s, &symbol, f);
        symbol.clear(f);
        self.emit_object_read(&constructor, &constructor, &key, &pending, f)?;
        key.clear(f);
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        self.compile_nullish_tagged_i32(pending.value().tag(), f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_constructor_i32(pending.value(), f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::REGEXP_PROTOTYPE_SYMBOL_MATCHALL_SPECIES_IS_NOT_A_CONSTRUCTOR,
            NativeErrorKind::TypeError,
            output,
            exit,
            f,
        )?;
        selected.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        constructor.clear(f);
        pending.clear(f);
        Ok(selected)
    }

    pub(super) fn emit_native_string_list_result_array(
        &mut self,
        list: GcLocal<ValueArray>,
        f: &mut Function,
    ) -> Result<GcLocal<ArrayObject>, EmitError> {
        let s = self.runtime_schema();
        let length = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let ordinal = s.reserve_i32_local(f);
        let value = s.reserve_value_local(f);
        s.array_type::<ValueArray>().length(&list, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.store(f);
        let array = self.emit_native_string_result_array(length, f)?;
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        ordinal.store(f);
        self.emit_argument_vector_entry_to_value(&list, ordinal, &value, f);
        self.emit_native_string_array_write(&array, index, &value, f)?;
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.clear(f);
        s.release_i32_local(ordinal, f);
        s.release_i64_local(index, f);
        s.release_i64_local(length, f);
        list.clear(f);
        Ok(array)
    }

    pub(crate) fn emit_regexp_prototype_symbol_match_all_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_regexp_protocol(f, |b, regexp, input, output, exit, f| {
            let s = b.runtime_schema();
            let pending = s.reserve_completion(f);
            let matcher = s.reserve_value_local(f);
            let flag_value = s.reserve_value_local(f);
            let last = s.reserve_value_local(f);
            let index = s.reserve_i64_local(f);
            let global = s.reserve_i32_local(f);
            let unicode = s.reserve_i32_local(f);
            let constructor = b.emit_native_regexp_species(regexp, output, exit, f)?;
            let flags = b.emit_native_regexp_protocol_flags(regexp, output, exit, f)?;
            flag_value.set_reference(&flags, s, f);
            let arguments = b.emit_pre_evaluated_arg_vector(&[regexp, &flag_value], f);
            b.emit_function_or_proxy_construct_with_argv(
                &constructor,
                &constructor,
                &arguments,
                &pending,
                f,
            )?;
            arguments.clear(f);
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            matcher.copy_from(pending.value(), f);
            b.emit_native_regexp_get(regexp, "lastIndex", &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            last.copy_from(pending.value(), f);
            b.emit_to_length_i64_from_value_locals(&last, index, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.emit_native_regexp_set_last_index(&matcher, index, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.emit_native_regexp_has_flag(&flags, NativeRegExpFlag::Global, global, f);
            b.emit_native_regexp_protocol_unicode(&flags, unicode, f);
            let iterator = s.reserve_gc_local(f).initialize(
                b.emit_regexp_string_iterator_create_from_locals(
                    &matcher, input, global, unicode, f,
                )?,
                f,
            );
            last.set_reference(&iterator, s, f);
            output.set_normal(&last, f);
            iterator.clear(f);
            s.release_i32_local(unicode, f);
            s.release_i32_local(global, f);
            s.release_i64_local(index, f);
            flags.clear(f);
            constructor.clear(f);
            last.clear(f);
            flag_value.clear(f);
            matcher.clear(f);
            pending.clear(f);
            Ok(())
        })
    }

    pub(crate) fn emit_regexp_prototype_symbol_search_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_regexp_protocol(f, |b, regexp, input, output, exit, f| {
            let s = b.runtime_schema();
            let previous = s.reserve_value_local(f);
            let zero = s.reserve_value_local(f);
            let found = s.reserve_value_local(f);
            let pending = s.reserve_completion(f);
            zero.set_scalar(ScalarValue::NumberBits(0), f);
            b.emit_native_regexp_get(regexp, "lastIndex", &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            previous.copy_from(pending.value(), f);
            b.emit_tagged_payload_same_value_i32(&previous, &zero, f)?;
            f.instruction(&Instruction::I32Eqz);
            b.open_frame(ControlFrameKind::If, f);
            let key = b.emit_native_regexp_key("lastIndex", f);
            b.emit_object_write_strict(regexp, &key, &zero, &pending, f)?;
            key.clear(f);
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            b.emit_native_regexp_observed_exec(regexp, input, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            found.copy_from(pending.value(), f);
            b.emit_native_regexp_get(regexp, "lastIndex", &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.emit_tagged_payload_same_value_i32(pending.value(), &previous, f)?;
            f.instruction(&Instruction::I32Eqz);
            b.open_frame(ControlFrameKind::If, f);
            let key = b.emit_native_regexp_key("lastIndex", f);
            b.emit_object_write_strict(regexp, &key, &previous, &pending, f)?;
            key.clear(f);
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            found.tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
            f.instruction(&Instruction::I32Eq);
            b.open_frame(ControlFrameKind::If, f);
            zero.set_scalar(ScalarValue::NumberBits((-1.0_f64).to_bits() as i64), f);
            output.set_normal(&zero, f);
            f.instruction(&Instruction::Else);
            b.emit_native_regexp_get(&found, "index", output, f)?;
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            pending.clear(f);
            found.clear(f);
            zero.clear(f);
            previous.clear(f);
            Ok(())
        })
    }

    fn emit_native_regexp_advance_last_index(
        &mut self,
        regexp: &ValueLocals,
        input: &GcLocal<StringValue>,
        unicode: I32Local,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let pending = s.reserve_completion(f);
        let value = s.reserve_value_local(f);
        let index = s.reserve_i64_local(f);
        let width = s.reserve_i32_local(f);
        self.emit_native_regexp_get(regexp, "lastIndex", &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        value.copy_from(pending.value(), f);
        self.emit_to_length_i64_from_value_locals(&value, index, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        f.instruction(&Instruction::I32Const(1));
        width.store(f);
        unicode.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_native_iterator_string_width(input, index, width, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        index.load(f);
        width.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_native_regexp_set_last_index(regexp, index, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        s.release_i32_local(width, f);
        s.release_i64_local(index, f);
        value.clear(f);
        pending.clear(f);
        Ok(())
    }

    pub(crate) fn emit_regexp_prototype_symbol_match_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_regexp_protocol(f, |b, regexp, input, output, exit, f| {
            let s = b.runtime_schema();
            let pending = s.reserve_completion(f);
            let converted = s.reserve_completion(f);
            let match_result = s.reserve_value_local(f);
            let value = s.reserve_value_local(f);
            let global = s.reserve_i32_local(f);
            let unicode = s.reserve_i32_local(f);
            let zero = s.reserve_i64_local(f);
            let length = s.reserve_i64_local(f);
            let flags = b.emit_native_regexp_protocol_flags(regexp, output, exit, f)?;
            b.emit_native_regexp_has_flag(&flags, NativeRegExpFlag::Global, global, f);
            global.load(f);
            b.open_frame(ControlFrameKind::If, f);
            b.emit_native_regexp_protocol_unicode(&flags, unicode, f);
            f.instruction(&Instruction::I64Const(0));
            zero.store(f);
            b.emit_native_regexp_set_last_index(regexp, zero, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            let list = crate::functions::ArgumentListConstruction::new(b, f);
            let done = b.open_frame(ControlFrameKind::Block, f);
            let next = b.open_frame(ControlFrameKind::Loop, f);
            b.emit_native_regexp_observed_exec(regexp, input, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            pending.value().tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
            f.instruction(&Instruction::I32Eq);
            b.emit_branch_if_to_target(done, f);
            match_result.copy_from(pending.value(), f);
            b.emit_native_regexp_get(&match_result, "0", &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.emit_value_to_string_payload(pending.value(), &converted, f)?;
            b.emit_native_string_abrupt_exit(&converted, output, exit, f);
            list.append(converted.value(), s, f);
            let text = s
                .reserve_gc_local(f)
                .initialize(converted.value().cast_reference::<StringValue>(s, f), f);
            b.emit_native_gc_string_length(&text, length, f);
            text.clear(f);
            length.load(f);
            f.instruction(&Instruction::I64Eqz);
            b.open_frame(ControlFrameKind::If, f);
            b.emit_native_regexp_advance_last_index(regexp, input, unicode, output, exit, f)?;
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            b.emit_branch_to_target(next, f);
            b.pop_control(ControlFrameKind::Loop);
            f.instruction(&Instruction::End);
            b.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            let values = list.finish(b, f);
            s.array_type::<ValueArray>().length(&values, s, f);
            f.instruction(&Instruction::I32Eqz);
            b.open_frame(ControlFrameKind::If, f);
            value.set_scalar(ScalarValue::Null, f);
            output.set_normal(&value, f);
            f.instruction(&Instruction::Else);
            let copy = s.reserve_gc_local(f).initialize(values.load(s, f), f);
            let array = b.emit_native_string_list_result_array(copy, f)?;
            value.set_reference(&array, s, f);
            output.set_normal(&value, f);
            array.clear(f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            values.clear(f);
            f.instruction(&Instruction::Else);
            b.emit_native_regexp_observed_exec(regexp, input, output, f)?;
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            flags.clear(f);
            s.release_i64_local(length, f);
            s.release_i64_local(zero, f);
            s.release_i32_local(unicode, f);
            s.release_i32_local(global, f);
            value.clear(f);
            match_result.clear(f);
            converted.clear(f);
            pending.clear(f);
            Ok(())
        })
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_regexp_prototype_symbol_replace_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_regexp_protocol(f, |b, regexp, input, output, exit, f| {
            use super::regexp_substitution::SubstitutionCaptureList;
            let s = b.runtime_schema();
            let replacement = s.reserve_value_local(f);
            let result_value = s.reserve_value_local(f);
            let capture_value = s.reserve_value_local(f);
            let named_value = s.reserve_value_local(f);
            let input_value = s.reserve_value_local(f);
            let position_value = s.reserve_value_local(f);
            let undefined = s.reserve_value_local(f);
            let pending = s.reserve_completion(f);
            let converted = s.reserve_completion(f);
            let functional = s.reserve_i32_local(f);
            let global = s.reserve_i32_local(f);
            let unicode = s.reserve_i32_local(f);
            let ordinal = s.reserve_i32_local(f);
            let input_length = s.reserve_i64_local(f);
            let match_length = s.reserve_i64_local(f);
            let index = s.reserve_i64_local(f);
            let count = s.reserve_i64_local(f);
            let result_length = s.reserve_i64_local(f);
            let capture_count = s.reserve_i64_local(f);
            let capture_index = s.reserve_i64_local(f);
            let position = s.reserve_i64_local(f);
            let integer_bits = s.reserve_i64_local(f);
            let next_source = s.reserve_i64_local(f);
            let zero = s.reserve_i64_local(f);
            b.emit_builtin_arg_to_value(1, &replacement, f);
            b.emit_is_callable_i32(&replacement, f)?;
            functional.store(f);
            let replacement_template = s
                .reserve_gc_local(f)
                .initialize(b.emit_native_string_static("", f), f);
            functional.load(f);
            f.instruction(&Instruction::I32Eqz);
            b.open_frame(ControlFrameKind::If, f);
            b.emit_value_to_string_payload(&replacement, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            replacement_template.replace(pending.value().cast_reference::<StringValue>(s, f), f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            b.emit_native_gc_string_length(input, input_length, f);
            input_value.set_reference(input, s, f);
            undefined.set_undefined(f);
            let flags = b.emit_native_regexp_protocol_flags(regexp, output, exit, f)?;
            b.emit_native_regexp_has_flag(&flags, NativeRegExpFlag::Global, global, f);
            b.emit_native_regexp_protocol_unicode(&flags, unicode, f);
            f.instruction(&Instruction::I64Const(0));
            zero.store(f);
            global.load(f);
            b.open_frame(ControlFrameKind::If, f);
            b.emit_native_regexp_set_last_index(regexp, zero, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            let results = crate::functions::ArgumentListConstruction::new(b, f);
            let collected = b.open_frame(ControlFrameKind::Block, f);
            let collect_next = b.open_frame(ControlFrameKind::Loop, f);
            b.emit_native_regexp_observed_exec(regexp, input, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            pending.value().tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
            f.instruction(&Instruction::I32Eq);
            b.emit_branch_if_to_target(collected, f);
            results.append(pending.value(), s, f);
            result_value.copy_from(pending.value(), f);
            global.load(f);
            f.instruction(&Instruction::I32Eqz);
            b.emit_branch_if_to_target(collected, f);
            b.emit_native_regexp_get(&result_value, "0", &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.emit_value_to_string_payload(pending.value(), &converted, f)?;
            b.emit_native_string_abrupt_exit(&converted, output, exit, f);
            let matched = s
                .reserve_gc_local(f)
                .initialize(converted.value().cast_reference::<StringValue>(s, f), f);
            b.emit_native_gc_string_length(&matched, match_length, f);
            matched.clear(f);
            match_length.load(f);
            f.instruction(&Instruction::I64Eqz);
            b.open_frame(ControlFrameKind::If, f);
            b.emit_native_regexp_advance_last_index(regexp, input, unicode, output, exit, f)?;
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            b.emit_branch_to_target(collect_next, f);
            b.pop_control(ControlFrameKind::Loop);
            f.instruction(&Instruction::End);
            b.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            let results = results.finish(b, f);
            s.array_type::<ValueArray>().length(&results, s, f);
            f.instruction(&Instruction::I64ExtendI32U);
            count.store(f);
            // All observable exec/flag/lastIndex and replacement conversions
            // have completed before this empty-result identity return.
            count.load(f);
            f.instruction(&Instruction::I64Eqz);
            b.open_frame(ControlFrameKind::If, f);
            b.emit_native_string_normal_reference(input, output, f);
            b.emit_branch_to_target(exit, f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            let pieces = ReplacementPieces::new(b, f);
            let replacement_text = s
                .reserve_gc_local(f)
                .initialize(b.emit_native_string_static("", f), f);
            f.instruction(&Instruction::I64Const(0));
            index.store(f);
            f.instruction(&Instruction::I64Const(0));
            next_source.store(f);
            let done = b.open_frame(ControlFrameKind::Block, f);
            let next = b.open_frame(ControlFrameKind::Loop, f);
            index.load(f);
            count.load(f);
            f.instruction(&Instruction::I64GeU);
            b.emit_branch_if_to_target(done, f);
            index.load(f);
            f.instruction(&Instruction::I32WrapI64);
            ordinal.store(f);
            b.emit_argument_vector_entry_to_value(&results, ordinal, &result_value, f);
            b.emit_native_regexp_get(&result_value, "length", &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            capture_value.copy_from(pending.value(), f);
            b.emit_to_length_i64_from_value_locals(&capture_value, result_length, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            result_length.load(f);
            capture_count.store(f);
            capture_count.load(f);
            f.instruction(&Instruction::I64Eqz);
            f.instruction(&Instruction::I32Eqz);
            b.open_frame(ControlFrameKind::If, f);
            capture_count.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Sub);
            capture_count.store(f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            b.emit_native_regexp_get(&result_value, "0", &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.emit_value_to_string_payload(pending.value(), &converted, f)?;
            b.emit_native_string_abrupt_exit(&converted, output, exit, f);
            let matched = s
                .reserve_gc_local(f)
                .initialize(converted.value().cast_reference::<StringValue>(s, f), f);
            b.emit_native_gc_string_length(&matched, match_length, f);
            b.emit_native_regexp_get(&result_value, "index", &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            capture_value.copy_from(pending.value(), f);
            b.emit_value_to_number_payload(&capture_value, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.emit_to_integer_or_infinity_number_payload_from_number_payload(
                pending.value().scalar(),
                integer_bits,
                f,
            );
            b.emit_to_integer_clamped_to_string_len(integer_bits, input_length, position, f);
            let captures = SubstitutionCaptureList::new(b, f);
            f.instruction(&Instruction::I64Const(1));
            capture_index.store(f);
            let captures_done = b.open_frame(ControlFrameKind::Block, f);
            let captures_next = b.open_frame(ControlFrameKind::Loop, f);
            capture_index.load(f);
            capture_count.load(f);
            f.instruction(&Instruction::I64GtU);
            b.emit_branch_if_to_target(captures_done, f);
            let key = b.emit_native_string_numeric_key(capture_index, f)?;
            b.emit_object_read(&result_value, &result_value, &key, &pending, f)?;
            key.clear(f);
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            capture_value.copy_from(pending.value(), f);
            capture_value.tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
            f.instruction(&Instruction::I32Eq);
            b.open_frame(ControlFrameKind::If, f);
            captures.append_undefined(s, f);
            f.instruction(&Instruction::Else);
            b.emit_value_to_string_payload(&capture_value, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            let text = s
                .reserve_gc_local(f)
                .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
            captures.append_string(&text, s, f);
            text.clear(f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            capture_index.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Add);
            capture_index.store(f);
            b.emit_branch_to_target(captures_next, f);
            b.pop_control(ControlFrameKind::Loop);
            f.instruction(&Instruction::End);
            b.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            let captures = captures.finish(b, f);
            b.emit_native_regexp_get(&result_value, "groups", &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            named_value.copy_from(pending.value(), f);
            functional.load(f);
            b.open_frame(ControlFrameKind::If, f);
            let arguments = crate::functions::ArgumentListConstruction::new(b, f);
            capture_value.set_reference(&matched, s, f);
            arguments.append(&capture_value, s, f);
            f.instruction(&Instruction::I64Const(0));
            capture_index.store(f);
            let args_done = b.open_frame(ControlFrameKind::Block, f);
            let args_next = b.open_frame(ControlFrameKind::Loop, f);
            capture_index.load(f);
            capture_count.load(f);
            f.instruction(&Instruction::I64GeU);
            b.emit_branch_if_to_target(args_done, f);
            capture_index.load(f);
            f.instruction(&Instruction::I32WrapI64);
            ordinal.store(f);
            b.emit_argument_vector_entry_to_value(captures.values(), ordinal, &capture_value, f);
            arguments.append(&capture_value, s, f);
            capture_index.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Add);
            capture_index.store(f);
            b.emit_branch_to_target(args_next, f);
            b.pop_control(ControlFrameKind::Loop);
            f.instruction(&Instruction::End);
            b.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            position.load(f);
            f.instruction(&Instruction::F64ConvertI64U);
            f.instruction(&Instruction::I64ReinterpretF64);
            integer_bits.store(f);
            position_value.set_number(integer_bits, f);
            arguments.append(&position_value, s, f);
            arguments.append(&input_value, s, f);
            named_value.tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
            f.instruction(&Instruction::I32Ne);
            b.open_frame(ControlFrameKind::If, f);
            arguments.append(&named_value, s, f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            let arguments = arguments.finish(b, f);
            b.emit_function_or_proxy_call_with_argv(
                &replacement,
                &undefined,
                &arguments,
                &pending,
                f,
            )?;
            arguments.clear(f);
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            capture_value.copy_from(pending.value(), f);
            b.emit_value_to_string_payload(&capture_value, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            replacement_text.replace(pending.value().cast_reference::<StringValue>(s, f), f);
            f.instruction(&Instruction::Else);
            let named =
                b.emit_complete_named_substitution_captures(&named_value, output, exit, f)?;
            let substituted = b.emit_regexp_get_substitution(
                &replacement_template,
                input,
                &matched,
                position,
                &captures,
                &named,
                output,
                exit,
                f,
            )?;
            replacement_text.replace(substituted.load(s, f), f);
            substituted.clear(f);
            named.clear(f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            // Replacers and named getters still run for backwards positions;
            // only concatenation is omitted for an overlapping custom result.
            position.load(f);
            next_source.load(f);
            f.instruction(&Instruction::I64GeU);
            b.open_frame(ControlFrameKind::If, f);
            let preserved = s
                .reserve_gc_local(f)
                .initialize(b.emit_gc_string_slice(input, next_source, position, f), f);
            pieces.append(b, &preserved, output, exit, f)?;
            pieces.append(b, &replacement_text, output, exit, f)?;
            preserved.clear(f);
            position.load(f);
            match_length.load(f);
            f.instruction(&Instruction::I64Add);
            next_source.store(f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            captures.clear(f);
            matched.clear(f);
            index.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Add);
            index.store(f);
            b.emit_branch_to_target(next, f);
            b.pop_control(ControlFrameKind::Loop);
            f.instruction(&Instruction::End);
            b.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            next_source.load(f);
            input_length.load(f);
            f.instruction(&Instruction::I64LtU);
            b.open_frame(ControlFrameKind::If, f);
            let tail = s.reserve_gc_local(f).initialize(
                b.emit_gc_string_slice(input, next_source, input_length, f),
                f,
            );
            pieces.append(b, &tail, output, exit, f)?;
            tail.clear(f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            let accumulated = pieces.publish(b, f);
            b.emit_native_string_normal_reference(&accumulated, output, f);
            replacement_text.clear(f);
            accumulated.clear(f);
            results.clear(f);
            flags.clear(f);
            replacement_template.clear(f);
            for local in [
                zero,
                next_source,
                integer_bits,
                position,
                capture_index,
                capture_count,
                result_length,
                count,
                index,
                match_length,
                input_length,
            ] {
                s.release_i64_local(local, f);
            }
            for local in [ordinal, unicode, global, functional] {
                s.release_i32_local(local, f);
            }
            converted.clear(f);
            pending.clear(f);
            undefined.clear(f);
            position_value.clear(f);
            input_value.clear(f);
            named_value.clear(f);
            capture_value.clear(f);
            result_value.clear(f);
            replacement.clear(f);
            Ok(())
        })
    }
}

impl FunctionBuilder<'_> {
    fn emit_native_regexp_advance_cursor(
        &mut self,
        input: &GcLocal<StringValue>,
        unicode: I32Local,
        cursor: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let width = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(1));
        width.store(f);
        unicode.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_native_iterator_string_width(input, cursor, width, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cursor.load(f);
        width.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Add);
        cursor.store(f);
        s.release_i32_local(width, f);
    }

    pub(crate) fn emit_regexp_prototype_symbol_split_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_regexp_protocol(f, |b, regexp, input, output, exit, f| {
            let s = b.runtime_schema();
            let limit = s.reserve_value_local(f);
            let flag_value = s.reserve_value_local(f);
            let splitter = s.reserve_value_local(f);
            let result_value = s.reserve_value_local(f);
            let value = s.reserve_value_local(f);
            let pending = s.reserve_completion(f);
            let max = s.reserve_i64_local(f);
            let size = s.reserve_i64_local(f);
            let count = s.reserve_i64_local(f);
            let cursor = s.reserve_i64_local(f);
            let last_end = s.reserve_i64_local(f);
            let match_end = s.reserve_i64_local(f);
            let capture_count = s.reserve_i64_local(f);
            let capture_index = s.reserve_i64_local(f);
            let unicode = s.reserve_i32_local(f);
            let sticky = s.reserve_i32_local(f);
            b.emit_builtin_arg_to_value(1, &limit, f);
            let constructor = b.emit_native_regexp_species(regexp, output, exit, f)?;
            let flags = b.emit_native_regexp_protocol_flags(regexp, output, exit, f)?;
            b.emit_native_regexp_protocol_unicode(&flags, unicode, f);
            b.emit_native_regexp_has_flag(&flags, NativeRegExpFlag::Sticky, sticky, f);
            sticky.load(f);
            f.instruction(&Instruction::I32Eqz);
            b.open_frame(ControlFrameKind::If, f);
            let y = s
                .reserve_gc_local(f)
                .initialize(b.emit_native_string_static("y", f), f);
            flags.replace(b.emit_concat_gc_strings(&flags, &y, f), f);
            y.clear(f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            flag_value.set_reference(&flags, s, f);
            let arguments = b.emit_pre_evaluated_arg_vector(&[regexp, &flag_value], f);
            b.emit_function_or_proxy_construct_with_argv(
                &constructor,
                &constructor,
                &arguments,
                &pending,
                f,
            )?;
            arguments.clear(f);
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            splitter.copy_from(pending.value(), f);
            limit.tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
            f.instruction(&Instruction::I32Eq);
            b.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I64Const(u32::MAX as i64));
            max.store(f);
            f.instruction(&Instruction::Else);
            b.emit_value_to_number_payload(&limit, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.emit_to_uint32_i64_from_number_payload(pending.value().scalar(), max, f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            b.emit_native_gc_string_length(input, size, f);
            let list = crate::functions::ArgumentListConstruction::new(b, f);
            f.instruction(&Instruction::I64Const(0));
            count.store(f);
            f.instruction(&Instruction::I64Const(0));
            cursor.store(f);
            f.instruction(&Instruction::I64Const(0));
            last_end.store(f);
            let list_done = b.open_frame(ControlFrameKind::Block, f);
            max.load(f);
            f.instruction(&Instruction::I64Eqz);
            b.emit_branch_if_to_target(list_done, f);
            size.load(f);
            f.instruction(&Instruction::I64Eqz);
            b.open_frame(ControlFrameKind::If, f);
            b.emit_native_regexp_observed_exec(&splitter, input, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            pending.value().tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
            f.instruction(&Instruction::I32Eq);
            b.open_frame(ControlFrameKind::If, f);
            value.set_reference(input, s, f);
            list.append(&value, s, f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            b.emit_branch_to_target(list_done, f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            let search_done = b.open_frame(ControlFrameKind::Block, f);
            let next = b.open_frame(ControlFrameKind::Loop, f);
            cursor.load(f);
            size.load(f);
            f.instruction(&Instruction::I64GeU);
            b.emit_branch_if_to_target(search_done, f);
            b.emit_native_regexp_set_last_index(&splitter, cursor, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.emit_native_regexp_observed_exec(&splitter, input, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            result_value.copy_from(pending.value(), f);
            result_value.tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
            f.instruction(&Instruction::I32Eq);
            b.open_frame(ControlFrameKind::If, f);
            b.emit_native_regexp_advance_cursor(input, unicode, cursor, f);
            f.instruction(&Instruction::Else);
            b.emit_native_regexp_get(&splitter, "lastIndex", &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            value.copy_from(pending.value(), f);
            b.emit_to_length_i64_from_value_locals(&value, match_end, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            match_end.load(f);
            size.load(f);
            f.instruction(&Instruction::I64GtU);
            b.open_frame(ControlFrameKind::If, f);
            size.load(f);
            match_end.store(f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            match_end.load(f);
            last_end.load(f);
            f.instruction(&Instruction::I64Eq);
            b.open_frame(ControlFrameKind::If, f);
            b.emit_native_regexp_advance_cursor(input, unicode, cursor, f);
            f.instruction(&Instruction::Else);
            let prefix = s
                .reserve_gc_local(f)
                .initialize(b.emit_gc_string_slice(input, last_end, cursor, f), f);
            value.set_reference(&prefix, s, f);
            list.append(&value, s, f);
            prefix.clear(f);
            count.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Add);
            count.store(f);
            count.load(f);
            max.load(f);
            f.instruction(&Instruction::I64GeU);
            b.emit_branch_if_to_target(list_done, f);
            match_end.load(f);
            last_end.store(f);
            b.emit_native_regexp_get(&result_value, "length", &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            value.copy_from(pending.value(), f);
            b.emit_to_length_i64_from_value_locals(&value, capture_count, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            capture_count.load(f);
            f.instruction(&Instruction::I64Eqz);
            f.instruction(&Instruction::I32Eqz);
            b.open_frame(ControlFrameKind::If, f);
            capture_count.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Sub);
            capture_count.store(f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            f.instruction(&Instruction::I64Const(1));
            capture_index.store(f);
            let captures_done = b.open_frame(ControlFrameKind::Block, f);
            let captures_next = b.open_frame(ControlFrameKind::Loop, f);
            capture_index.load(f);
            capture_count.load(f);
            f.instruction(&Instruction::I64GtU);
            b.emit_branch_if_to_target(captures_done, f);
            let key = b.emit_native_string_numeric_key(capture_index, f)?;
            b.emit_object_read(&result_value, &result_value, &key, &pending, f)?;
            key.clear(f);
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            list.append(pending.value(), s, f);
            count.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Add);
            count.store(f);
            count.load(f);
            max.load(f);
            f.instruction(&Instruction::I64GeU);
            b.emit_branch_if_to_target(list_done, f);
            capture_index.load(f);
            f.instruction(&Instruction::I64Const(1));
            f.instruction(&Instruction::I64Add);
            capture_index.store(f);
            b.emit_branch_to_target(captures_next, f);
            b.pop_control(ControlFrameKind::Loop);
            f.instruction(&Instruction::End);
            b.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            last_end.load(f);
            cursor.store(f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            b.emit_branch_to_target(next, f);
            b.pop_control(ControlFrameKind::Loop);
            f.instruction(&Instruction::End);
            b.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            let tail = s
                .reserve_gc_local(f)
                .initialize(b.emit_gc_string_slice(input, last_end, size, f), f);
            value.set_reference(&tail, s, f);
            list.append(&value, s, f);
            tail.clear(f);
            b.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            let list = list.finish(b, f);
            let array = b.emit_native_string_list_result_array(list, f)?;
            value.set_reference(&array, s, f);
            output.set_normal(&value, f);
            array.clear(f);
            flags.clear(f);
            constructor.clear(f);
            s.release_i32_local(sticky, f);
            s.release_i32_local(unicode, f);
            for local in [
                capture_index,
                capture_count,
                match_end,
                last_end,
                cursor,
                count,
                size,
                max,
            ] {
                s.release_i64_local(local, f);
            }
            pending.clear(f);
            value.clear(f);
            result_value.clear(f);
            splitter.clear(f);
            flag_value.clear(f);
            limit.clear(f);
            Ok(())
        })
    }
}

impl FunctionBuilder<'_> {
    /// RegExpCreate performs RegExpAlloc/Initialize directly. In particular,
    /// it does not substitute the public constructor's IsRegExp source-copy
    /// policy when a String method's symbol hook was absent.
    pub(super) fn emit_native_regexp_create(
        &mut self,
        pattern: &ValueLocals,
        flags: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<RegExpObject>, EmitError> {
        let s = self.runtime_schema();
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let constructor = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::RegExpConstructor,
            &constructor,
            f,
        );
        realm.clear(f);
        self.emit_get_prototype_from_constructor(
            &constructor,
            OrdinaryDefaultPrototype::RegExp,
            &pending,
            f,
        )?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(pending.value()), f)?,
            f,
        );
        let zero = s.reserve_value_local(f);
        zero.set_scalar(ScalarValue::NumberBits(0), f);
        let key = self.emit_native_regexp_key("lastIndex", f);
        self.emit_object_append_data_property_with_flags(
            &header, &key, &zero, true, false, false, f,
        )?;
        key.clear(f);
        zero.clear(f);
        let source = self.emit_native_regexp_string_argument(pattern, output, exit, f)?;
        let flag_string = self.emit_native_regexp_string_argument(flags, output, exit, f)?;
        let program =
            self.emit_native_regexp_compile_program(&source, &flag_string, output, exit, f)?;
        let realm = self.emit_execution_realm(f);
        let legacy_enabled = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(1));
        legacy_enabled.store(f);
        let object = s.reserve_gc_local(f).initialize(
            CompletedRegExpConstruction {
                header,
                source,
                flags: flag_string,
                program,
                realm,
                legacy_enabled,
            }
            .publish(self, f),
            f,
        );
        pending.clear(f);
        constructor.clear(f);
        Ok(object)
    }
}
