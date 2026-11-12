//! Native String algorithms use immutable UTF-16 GC strings and whole completions.
use super::super::*;
use crate::gc_types::*;
use crate::operations::PropertyKeyLocals;
use scriptw_ir::NativeErrorKind;

mod concat;
mod constructor;
mod regexp_exec;
mod regexp_legacy;
pub(in crate::builtins) use regexp_legacy::RegExpLegacyAccessorKind;
mod regexp_protocol;
mod regexp_substitution;
mod replacement_pieces;
mod string_code_unit_range;
mod string_literal_replacement_scope;
mod string_well_formed_operation;
mod symbol_method;
mod unicode_operations;
pub(in crate::builtins) use unicode_operations::{StringCaseOperation, StringHtmlOperation};

pub(crate) enum StringNormalizationForm {
    Nfc,
    Nfd,
    Nfkc,
    Nfkd,
}
impl StringNormalizationForm {
    pub(crate) const ALL: [Self; 4] = [Self::Nfc, Self::Nfd, Self::Nfkc, Self::Nfkd];
    pub(crate) const fn spelling(&self) -> &'static str {
        match self {
            Self::Nfc => "NFC",
            Self::Nfd => "NFD",
            Self::Nfkc => "NFKC",
            Self::Nfkd => "NFKD",
        }
    }
    fn decomposition_table(&self, strings: &StringPool) -> (u32, u32) {
        match self {
            Self::Nfc | Self::Nfd => (
                strings.canonical_decomposition_table_ptr,
                strings.canonical_decomposition_count,
            ),
            Self::Nfkc | Self::Nfkd => (
                strings.compatibility_decomposition_table_ptr,
                strings.compatibility_decomposition_count,
            ),
        }
    }
    const fn composes(self) -> bool {
        match self {
            Self::Nfc | Self::Nfkc => true,
            Self::Nfd | Self::Nfkd => false,
        }
    }
}

enum StringCharacterOperation {
    CharAt,
    At,
    CharCodeAt,
    CodePointAt,
}
enum StringSearchOperation {
    IndexOf,
    LastIndexOf,
    StartsWith,
    EndsWith,
    Includes,
}
use crate::operations::EcmaTrimMode as StringTrimOperation;

impl FunctionBuilder<'_> {
    fn emit_native_string_abrupt_exit(
        &mut self,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) {
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        output.copy_from(pending, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    fn emit_native_string_error_if(
        &mut self,
        message: RuntimeErrorMessage,
        kind: NativeErrorKind,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_error(kind, message, output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_native_string_require_coercible(
        &mut self,
        value: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_nullish_tagged_i32(value.tag(), f)?;
        self.emit_native_string_error_if(
            RuntimeErrorMessage::STRING_PROTOTYPE_METHOD_RECEIVER_IS_NULL_OR_UNDEFINED,
            NativeErrorKind::TypeError,
            output,
            exit,
            f,
        )
    }

    fn emit_with_native_string_receiver(
        &mut self,
        f: &mut Function,
        consume: impl FnOnce(
            &mut Self,
            &GcLocal<StringValue>,
            &CompletionLocals,
            ControlTarget,
            &mut Function,
        ) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let output = s.reserve_completion(f);
        let pending = s.reserve_completion(f);
        self.compile_this_to_locals(&receiver, f)?;
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_native_string_require_coercible(&receiver, &output, exit, f)?;
        self.emit_value_to_string_payload(&receiver, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, &output, exit, f);
        let string = s
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        consume(self, &string, &output, exit, f)?;
        string.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        pending.clear(f);
        output.clear(f);
        receiver.clear(f);
        Ok(())
    }

    pub(crate) fn emit_native_gc_string_length(
        &self,
        string: &GcLocal<StringValue>,
        length: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let units = s.reserve_gc_local(f).initialize(
            s.struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(string, s, f)
                .reference(),
            f,
        );
        s.array_type::<CodeUnitArray>().length(&units, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.store(f);
        units.clear(f);
    }

    fn emit_native_string_static(
        &self,
        text: &str,
        f: &mut Function,
    ) -> GcStackReference<StringValue> {
        let s = self.runtime_schema();
        let units: Vec<u16> = text.encode_utf16().collect();
        let length = s.reserve_i32_local(f);
        let index = s.reserve_i32_local(f);
        let unit = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(
            i32::try_from(units.len()).expect("literal fits GC String"),
        ));
        length.store(f);
        let construction = StringConstruction::allocate(s, s.reserve_gc_local(f), length, f);
        for (ordinal, code) in units.into_iter().enumerate() {
            f.instruction(&Instruction::I32Const(
                i32::try_from(ordinal).expect("literal unit fits GC String"),
            ));
            index.store(f);
            f.instruction(&Instruction::I32Const(i32::from(code)));
            unit.store(f);
            construction.write(index, unit, s, f);
        }
        let result = construction.publish(s, f);
        s.release_i32_local(unit, f);
        s.release_i32_local(index, f);
        s.release_i32_local(length, f);
        result
    }

    fn emit_native_string_normal_reference(
        &self,
        string: &GcLocal<StringValue>,
        output: &CompletionLocals,
        f: &mut Function,
    ) {
        let value = self.runtime_schema().reserve_value_local(f);
        value.set_reference(string, self.runtime_schema(), f);
        output.set_normal(&value, f);
        value.clear(f);
    }

    pub(crate) fn emit_string_prototype_value_of_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let output = s.reserve_completion(f);
        self.compile_this_to_locals(&receiver, f)?;
        value.copy_from(&receiver, f);
        receiver.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<PrimitiveBox>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let boxed = s
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<PrimitiveBox>(s, f), f);
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<PrimitiveBox>()
                .field(PrimitiveBoxSchema::PRIMITIVE)
                .read(&boxed, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&stored, &value, s, f);
        stored.clear(f);
        boxed.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_error(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::STRING_PROTOTYPE_METHOD_REQUIRES_A_STRING_RECEIVER,
            &output,
            f,
        )?;
        f.instruction(&Instruction::Else);
        output.set_normal(&value, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        value.clear(f);
        receiver.clear(f);
        Ok(())
    }

    pub(crate) fn emit_string_prototype_char_at_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_character(StringCharacterOperation::CharAt, f)
    }
    pub(crate) fn emit_string_prototype_at_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_character(StringCharacterOperation::At, f)
    }
    pub(crate) fn emit_string_prototype_char_code_at_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_character(StringCharacterOperation::CharCodeAt, f)
    }
    pub(crate) fn emit_string_prototype_code_point_at_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_character(StringCharacterOperation::CodePointAt, f)
    }

    fn emit_native_string_character(
        &mut self,
        operation: StringCharacterOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_string_receiver(f, |b, string, output, exit, f| {
            let s = b.runtime_schema();
            let argument = s.reserve_value_local(f);
            let pending = s.reserve_completion(f);
            let length = s.reserve_i64_local(f);
            let integer_bits = s.reserve_i64_local(f);
            let index = s.reserve_i64_local(f);
            let end = s.reserve_i64_local(f);
            let code = s.reserve_i32_local(f);
            let next_code = s.reserve_i32_local(f);
            let bits = s.reserve_i64_local(f);
            let result = s.reserve_value_local(f);
            b.emit_native_gc_string_length(string, length, f);
            b.emit_builtin_arg_to_value(0, &argument, f);
            b.emit_value_to_number_payload(&argument, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.emit_to_integer_or_infinity_number_payload_from_number_payload(
                pending.value().scalar(),
                integer_bits,
                f,
            );
            integer_bits.load(f);
            f.instruction(&Instruction::F64ReinterpretI64);
            f.instruction(&Instruction::I64TruncSatF64S);
            index.store(f);
            if matches!(&operation, StringCharacterOperation::At) {
                index.load(f);
                f.instruction(&Instruction::I64Const(0));
                f.instruction(&Instruction::I64LtS);
                b.open_frame(ControlFrameKind::If, f);
                index.load(f);
                length.load(f);
                f.instruction(&Instruction::I64Add);
                index.store(f);
                b.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
            index.load(f);
            f.instruction(&Instruction::I64Const(0));
            f.instruction(&Instruction::I64LtS);
            index.load(f);
            length.load(f);
            f.instruction(&Instruction::I64GeU);
            f.instruction(&Instruction::I32Or);
            b.open_frame(ControlFrameKind::If, f);
            match &operation {
                StringCharacterOperation::CharAt => {
                    let empty = s
                        .reserve_gc_local(f)
                        .initialize(b.emit_native_string_static("", f), f);
                    result.set_reference(&empty, s, f);
                    empty.clear(f);
                }
                StringCharacterOperation::CharCodeAt => {
                    f.instruction(&Instruction::I64Const(f64::NAN.to_bits() as i64));
                    bits.store(f);
                    result.set_number(bits, f);
                }
                StringCharacterOperation::At | StringCharacterOperation::CodePointAt => {
                    result.set_undefined(f)
                }
            }
            output.set_normal(&result, f);
            f.instruction(&Instruction::Else);
            b.emit_gc_string_code_unit_i32(string, index, f);
            code.store(f);
            match &operation {
                StringCharacterOperation::CharAt | StringCharacterOperation::At => {
                    let one = s.reserve_gc_local(f).initialize(string.load(s, f), f);
                    // The validated index selects one UTF-16 unit, including
                    // lone surrogates. Keep full-span immutable value reuse.
                    length.load(f);
                    f.instruction(&Instruction::I64Const(1));
                    f.instruction(&Instruction::I64Ne);
                    b.open_frame(ControlFrameKind::If, f);
                    let construction =
                        StringConstruction::from_code_unit(s, s.reserve_gc_local(f), code, f);
                    one.replace(construction.publish(s, f), f);
                    b.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    result.set_reference(&one, s, f);
                    one.clear(f);
                }
                StringCharacterOperation::CharCodeAt => {
                    code.load(f);
                    f.instruction(&Instruction::F64ConvertI32U);
                    f.instruction(&Instruction::I64ReinterpretF64);
                    bits.store(f);
                    result.set_number(bits, f);
                }
                StringCharacterOperation::CodePointAt => {
                    code.load(f);
                    f.instruction(&Instruction::I32Const(0xd800));
                    f.instruction(&Instruction::I32GeU);
                    code.load(f);
                    f.instruction(&Instruction::I32Const(0xdbff));
                    f.instruction(&Instruction::I32LeU);
                    f.instruction(&Instruction::I32And);
                    index.load(f);
                    f.instruction(&Instruction::I64Const(1));
                    f.instruction(&Instruction::I64Add);
                    end.store(f);
                    end.load(f);
                    length.load(f);
                    f.instruction(&Instruction::I64LtU);
                    f.instruction(&Instruction::I32And);
                    b.open_frame(ControlFrameKind::If, f);
                    b.emit_gc_string_code_unit_i32(string, end, f);
                    next_code.store(f);
                    next_code.load(f);
                    f.instruction(&Instruction::I32Const(0xdc00));
                    f.instruction(&Instruction::I32GeU);
                    next_code.load(f);
                    f.instruction(&Instruction::I32Const(0xdfff));
                    f.instruction(&Instruction::I32LeU);
                    f.instruction(&Instruction::I32And);
                    b.open_frame(ControlFrameKind::If, f);
                    code.load(f);
                    f.instruction(&Instruction::I32Const(0xd800));
                    f.instruction(&Instruction::I32Sub);
                    f.instruction(&Instruction::I32Const(10));
                    f.instruction(&Instruction::I32Shl);
                    next_code.load(f);
                    f.instruction(&Instruction::I32Const(0xdc00));
                    f.instruction(&Instruction::I32Sub);
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::I32Const(0x10000));
                    f.instruction(&Instruction::I32Add);
                    code.store(f);
                    b.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    b.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    code.load(f);
                    f.instruction(&Instruction::F64ConvertI32U);
                    f.instruction(&Instruction::I64ReinterpretF64);
                    bits.store(f);
                    result.set_number(bits, f);
                }
            }
            output.set_normal(&result, f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            result.clear(f);
            s.release_i64_local(bits, f);
            s.release_i32_local(next_code, f);
            s.release_i32_local(code, f);
            s.release_i64_local(end, f);
            s.release_i64_local(index, f);
            s.release_i64_local(integer_bits, f);
            s.release_i64_local(length, f);
            pending.clear(f);
            argument.clear(f);
            Ok(())
        })
    }

    pub(crate) fn compile_string_concat_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_string_receiver(f, |b, string, output, exit, f| {
            let s = b.runtime_schema();
            let arguments = s.reserve_gc_local(f).initialize(
                b.body_entry_locals()
                    .expect("native String entry")
                    .arguments()
                    .load(s, f),
                f,
            );
            let count = s.reserve_i32_local(f);
            s.array_type::<ValueArray>().length(&arguments, s, f);
            count.store(f);
            // Keep the existing allocation path for small argument lists.
            count.load(f);
            f.instruction(&Instruction::I32Const(4));
            f.instruction(&Instruction::I32LeU);
            b.open_frame(ControlFrameKind::If, f);
            let index = s.reserve_i32_local(f);
            let argument = s.reserve_value_local(f);
            let pending = s.reserve_completion(f);
            let current = s.reserve_gc_local(f).initialize(string.load(s, f), f);
            f.instruction(&Instruction::I32Const(0));
            index.store(f);
            let done = b.open_frame(ControlFrameKind::Block, f);
            let next = b.open_frame(ControlFrameKind::Loop, f);
            index.load(f);
            count.load(f);
            f.instruction(&Instruction::I32GeU);
            b.emit_branch_if_to_target(done, f);
            b.emit_argument_vector_entry_to_value(&arguments, index, &argument, f);
            b.emit_value_to_string_payload(&argument, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            let part = s
                .reserve_gc_local(f)
                .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
            current.replace(b.emit_concat_gc_strings(&current, &part, f), f);
            part.clear(f);
            index.load(f);
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Add);
            index.store(f);
            argument.set_undefined(f);
            b.emit_branch_to_target(next, f);
            b.pop_control(ControlFrameKind::Loop);
            f.instruction(&Instruction::End);
            b.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            b.emit_native_string_normal_reference(&current, output, f);
            current.clear(f);
            pending.clear(f);
            argument.clear(f);
            s.release_i32_local(index, f);
            f.instruction(&Instruction::Else);
            b.emit_batched_string_concat(string, &arguments, count, output, exit, f)?;
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            s.release_i32_local(count, f);
            arguments.clear(f);
            Ok(())
        })
    }

    pub(crate) fn emit_string_prototype_iterator_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_string_receiver(f, |b, string, output, _, f| {
            let s = b.runtime_schema();
            let iterator = s
                .reserve_gc_local(f)
                .initialize(b.emit_string_iterator_create_from_local(string, f)?, f);
            let value = s.reserve_value_local(f);
            value.set_reference(&iterator, s, f);
            output.set_normal(&value, f);
            value.clear(f);
            iterator.clear(f);
            Ok(())
        })
    }

    pub(crate) fn emit_string_prototype_locale_compare_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_string_receiver(f, |b, string, output, _, f| {
            let s = b.runtime_schema();
            let comparison = s.reserve_value_local(f);
            let locales = s.reserve_value_local(f);
            let options = s.reserve_value_local(f);
            b.emit_builtin_arg_to_value(0, &comparison, f);
            b.emit_builtin_arg_to_value(1, &locales, f);
            b.emit_builtin_arg_to_value(2, &options, f);
            b.emit_intrinsic_string_locale_compare(
                string,
                &comparison,
                &locales,
                &options,
                output,
                f,
            )?;
            options.clear(f);
            locales.clear(f);
            comparison.clear(f);
            Ok(())
        })
    }

    pub(crate) fn emit_string_prototype_trim_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_trim(StringTrimOperation::Both, f)
    }
    pub(crate) fn emit_string_prototype_trim_start_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_trim(StringTrimOperation::Start, f)
    }
    pub(crate) fn emit_string_prototype_trim_end_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_trim(StringTrimOperation::End, f)
    }
    fn emit_native_string_trim(
        &mut self,
        operation: StringTrimOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_string_receiver(f, |b, string, output, _, f| {
            let s = b.runtime_schema();
            let start = s.reserve_i64_local(f);
            let end = s.reserve_i64_local(f);
            b.emit_gc_string_trim_range_mode(string, start, end, operation, f);
            let selected = s
                .reserve_gc_local(f)
                .initialize(b.emit_gc_string_slice(string, start, end, f), f);
            b.emit_native_string_normal_reference(&selected, output, f);
            selected.clear(f);
            s.release_i64_local(end, f);
            s.release_i64_local(start, f);
            Ok(())
        })
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_string_prototype_index_of_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_search(StringSearchOperation::IndexOf, f)
    }
    pub(crate) fn emit_string_prototype_last_index_of_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_search(StringSearchOperation::LastIndexOf, f)
    }
    pub(crate) fn emit_string_prototype_starts_with_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_search(StringSearchOperation::StartsWith, f)
    }
    pub(crate) fn emit_string_prototype_ends_with_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_search(StringSearchOperation::EndsWith, f)
    }
    pub(crate) fn emit_string_prototype_includes_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_search(StringSearchOperation::Includes, f)
    }

    fn emit_native_string_search(
        &mut self,
        operation: StringSearchOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_string_receiver(f,|b,string,output,exit,f| {
            let s=b.runtime_schema();
            let argument=s.reserve_value_local(f); let position_argument=s.reserve_value_local(f);
            let pending=s.reserve_completion(f); let result=s.reserve_value_local(f);
            let length=s.reserve_i64_local(f); let needle_length=s.reserve_i64_local(f); let position=s.reserve_i64_local(f); let cursor=s.reserve_i64_local(f); let offset=s.reserve_i64_local(f); let source_index=s.reserve_i64_local(f); let found_index=s.reserve_i64_local(f); let bits=s.reserve_i64_local(f);
            let is_regexp=s.reserve_i32_local(f); let equal=s.reserve_i32_local(f); let found=s.reserve_i32_local(f); let unit=s.reserve_i32_local(f);
            b.emit_native_gc_string_length(string,length,f);
            b.emit_builtin_arg_to_value(0,&argument,f); b.emit_builtin_arg_to_value(1,&position_argument,f);
            let reject=match &operation {
                StringSearchOperation::StartsWith=>Some(RuntimeErrorMessage::FIRST_ARGUMENT_TO_STRING_PROTOTYPE_STARTSWITH_MUST_NOT_BE_A_REGEXP),
                StringSearchOperation::EndsWith=>Some(RuntimeErrorMessage::FIRST_ARGUMENT_TO_STRING_PROTOTYPE_ENDSWITH_MUST_NOT_BE_A_REGEXP),
                StringSearchOperation::Includes=>Some(RuntimeErrorMessage::FIRST_ARGUMENT_TO_STRING_PROTOTYPE_INCLUDES_MUST_NOT_BE_A_REGEXP),
                StringSearchOperation::IndexOf|StringSearchOperation::LastIndexOf=>None,
            };
            if let Some(message)=reject {
                b.emit_string_search_argument_is_regexp_to_local(&argument,is_regexp,&pending,f)?;
                b.emit_native_string_abrupt_exit(&pending,output,exit,f); is_regexp.load(f);
                b.emit_native_string_error_if(message,NativeErrorKind::TypeError,output,exit,f)?;
            }
            b.emit_value_to_string_payload(&argument,&pending,f)?;
            b.emit_native_string_abrupt_exit(&pending,output,exit,f);
            let needle=s.reserve_gc_local(f).initialize(pending.value().cast_reference::<StringValue>(s,f),f);
            b.emit_native_gc_string_length(&needle,needle_length,f);
            if matches!(&operation,StringSearchOperation::EndsWith) {
                position_argument.tag().load(f); f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag())); f.instruction(&Instruction::I32Eq);
                b.open_frame(ControlFrameKind::If,f); length.load(f); position.store(f);
                f.instruction(&Instruction::Else);
            }
            b.emit_value_to_number_payload(&position_argument,&pending,f)?;
            b.emit_native_string_abrupt_exit(&pending,output,exit,f); pending.value().scalar().load(f); bits.store(f);
            if matches!(&operation,StringSearchOperation::LastIndexOf) {
                bits.load(f); f.instruction(&Instruction::F64ReinterpretI64); bits.load(f); f.instruction(&Instruction::F64ReinterpretI64); f.instruction(&Instruction::F64Ne);
                b.open_frame(ControlFrameKind::If,f);
                f.instruction(&Instruction::I64Const(f64::INFINITY.to_bits() as i64)); bits.store(f);
                b.pop_control(ControlFrameKind::If); f.instruction(&Instruction::End);
            }
            b.emit_to_integer_clamped_to_string_len(bits,length,position,f);
            if matches!(&operation,StringSearchOperation::EndsWith) {
                b.pop_control(ControlFrameKind::If); f.instruction(&Instruction::End);
            }
            f.instruction(&Instruction::I32Const(0)); found.store(f);
            f.instruction(&Instruction::I64Const(-1)); found_index.store(f);
            let searched=b.open_frame(ControlFrameKind::Block,f);
            needle_length.load(f); length.load(f); f.instruction(&Instruction::I64GtU); b.emit_branch_if_to_target(searched,f);
            position.load(f); cursor.store(f);
            match &operation {
                StringSearchOperation::EndsWith=>{
                    needle_length.load(f); position.load(f); f.instruction(&Instruction::I64GtU); b.emit_branch_if_to_target(searched,f);
                    position.load(f); needle_length.load(f); f.instruction(&Instruction::I64Sub); cursor.store(f);
                }
                StringSearchOperation::LastIndexOf=>{
                    length.load(f); needle_length.load(f); f.instruction(&Instruction::I64Sub); source_index.store(f);
                    cursor.load(f); source_index.load(f); f.instruction(&Instruction::I64GtU);
                    b.open_frame(ControlFrameKind::If,f); source_index.load(f); cursor.store(f); b.pop_control(ControlFrameKind::If); f.instruction(&Instruction::End);
                }
                StringSearchOperation::IndexOf|StringSearchOperation::StartsWith|StringSearchOperation::Includes=>{}
            }
            let string_units=s.reserve_gc_local(f).initialize(s.field(StringValueSchema::CODE_UNITS).read(string,s,f).reference(),f);
            let needle_units=s.reserve_gc_local(f).initialize(s.field(StringValueSchema::CODE_UNITS).read(&needle,s,f).reference(),f);
            let at=s.reserve_i32_local(f);
            let next=b.open_frame(ControlFrameKind::Loop,f);
            cursor.load(f); length.load(f); needle_length.load(f); f.instruction(&Instruction::I64Sub); f.instruction(&Instruction::I64GtU); b.emit_branch_if_to_target(searched,f);
            f.instruction(&Instruction::I32Const(1)); equal.store(f);
            f.instruction(&Instruction::I64Const(0)); offset.store(f);
            let compared=b.open_frame(ControlFrameKind::Block,f); let compare=b.open_frame(ControlFrameKind::Loop,f);
            offset.load(f); needle_length.load(f); f.instruction(&Instruction::I64GeU); b.emit_branch_if_to_target(compared,f);
            cursor.load(f); offset.load(f); f.instruction(&Instruction::I64Add); source_index.store(f);
            source_index.load(f); f.instruction(&Instruction::I32WrapI64); at.store(f);
            s.array_type::<CodeUnitArray>().read(&string_units,at,s,f).store(unit,f);
            offset.load(f); f.instruction(&Instruction::I32WrapI64); at.store(f);
            s.array_type::<CodeUnitArray>().read(&needle_units,at,s,f); unit.load(f); f.instruction(&Instruction::I32Ne);
            b.open_frame(ControlFrameKind::If,f); f.instruction(&Instruction::I32Const(0)); equal.store(f); b.emit_branch_to_target(compared,f); b.pop_control(ControlFrameKind::If); f.instruction(&Instruction::End);
            offset.load(f); f.instruction(&Instruction::I64Const(1)); f.instruction(&Instruction::I64Add); offset.store(f); b.emit_branch_to_target(compare,f);
            b.pop_control(ControlFrameKind::Loop); f.instruction(&Instruction::End); b.pop_control(ControlFrameKind::Block); f.instruction(&Instruction::End);
            equal.load(f);
            b.open_frame(ControlFrameKind::If,f);
            f.instruction(&Instruction::I32Const(1)); found.store(f); cursor.load(f); found_index.store(f); b.emit_branch_to_target(searched,f);
            b.pop_control(ControlFrameKind::If); f.instruction(&Instruction::End);
            match &operation {
                StringSearchOperation::StartsWith|StringSearchOperation::EndsWith=>b.emit_branch_to_target(searched,f),
                StringSearchOperation::LastIndexOf=>{
                    cursor.load(f); f.instruction(&Instruction::I64Eqz); b.emit_branch_if_to_target(searched,f);
                    cursor.load(f); f.instruction(&Instruction::I64Const(1)); f.instruction(&Instruction::I64Sub); cursor.store(f); b.emit_branch_to_target(next,f);
                }
                StringSearchOperation::IndexOf|StringSearchOperation::Includes=>{
                    cursor.load(f); f.instruction(&Instruction::I64Const(1)); f.instruction(&Instruction::I64Add); cursor.store(f); b.emit_branch_to_target(next,f);
                }
            }
            b.pop_control(ControlFrameKind::Loop); f.instruction(&Instruction::End);
            s.release_i32_local(at,f); needle_units.clear(f); string_units.clear(f);
            b.pop_control(ControlFrameKind::Block); f.instruction(&Instruction::End);
            match operation {
                StringSearchOperation::IndexOf|StringSearchOperation::LastIndexOf=>{
                    found_index.load(f); f.instruction(&Instruction::F64ConvertI64S); f.instruction(&Instruction::I64ReinterpretF64); bits.store(f); result.set_number(bits,f);
                }
                StringSearchOperation::StartsWith|StringSearchOperation::EndsWith|StringSearchOperation::Includes=>result.set_boolean(found,f),
            }
            output.set_normal(&result,f);
            s.release_i32_local(unit,f); s.release_i32_local(found,f); s.release_i32_local(equal,f); s.release_i32_local(is_regexp,f);
            s.release_i64_local(bits,f); s.release_i64_local(found_index,f); s.release_i64_local(source_index,f); s.release_i64_local(offset,f); s.release_i64_local(cursor,f); s.release_i64_local(position,f); s.release_i64_local(needle_length,f); s.release_i64_local(length,f);
            needle.clear(f); result.clear(f); pending.clear(f); position_argument.clear(f); argument.clear(f); Ok(())
        })
    }

    pub(crate) fn emit_string_prototype_repeat_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_string_receiver(f, |b, string, output, exit, f| {
            let s = b.runtime_schema();
            let argument = s.reserve_value_local(f);
            let pending = s.reserve_completion(f);
            let bits = s.reserve_i64_local(f);
            let count = s.reserve_i64_local(f);
            let length = s.reserve_i64_local(f);
            let target = s.reserve_i64_local(f);
            let allocated = s.reserve_i32_local(f);
            b.emit_builtin_arg_to_value(0, &argument, f);
            b.emit_value_to_number_payload(&argument, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.emit_to_integer_or_infinity_number_payload_from_number_payload(
                pending.value().scalar(),
                bits,
                f,
            );
            bits.load(f);
            f.instruction(&Instruction::F64ReinterpretI64);
            f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
            f.instruction(&Instruction::F64Lt);
            bits.load(f);
            f.instruction(&Instruction::F64ReinterpretI64);
            f.instruction(&Instruction::F64Abs);
            f.instruction(&Instruction::F64Const(Ieee64::from(f64::INFINITY)));
            f.instruction(&Instruction::F64Eq);
            f.instruction(&Instruction::I32Or);
            b.emit_native_string_error_if(
                RuntimeErrorMessage::REPEAT_COUNT_MUST_BE_NON_NEGATIVE_AND_FINITE,
                NativeErrorKind::RangeError,
                output,
                exit,
                f,
            )?;
            bits.load(f);
            f.instruction(&Instruction::F64ReinterpretI64);
            f.instruction(&Instruction::I64TruncSatF64U);
            count.store(f);
            b.emit_native_gc_string_length(string, length, f);
            length.load(f);
            f.instruction(&Instruction::I64Eqz);
            f.instruction(&Instruction::I32Eqz);
            b.open_frame(ControlFrameKind::If, f);
            count.load(f);
            f.instruction(&Instruction::I64Const(u32::MAX as i64));
            length.load(f);
            f.instruction(&Instruction::I64DivU);
            f.instruction(&Instruction::I64GtU);
            b.emit_native_string_error_if(
                RuntimeErrorMessage::REPEAT_RESULT_WOULD_EXCEED_MAXIMUM_STRING_LENGTH,
                NativeErrorKind::RangeError,
                output,
                exit,
                f,
            )?;
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            count.load(f);
            length.load(f);
            f.instruction(&Instruction::I64Mul);
            target.store(f);
            target.load(f);
            f.instruction(&Instruction::I32WrapI64);
            allocated.store(f);
            let construction = StringConstruction::allocate(s, s.reserve_gc_local(f), allocated, f);
            let source_units = s.reserve_gc_local(f).initialize(
                s.struct_type::<StringValue>()
                    .field(StringValueSchema::CODE_UNITS)
                    .read(string, s, f)
                    .reference(),
                f,
            );
            let zero = s.reserve_i32_local(f);
            zero.set_constant(0, f);
            construction.copy_repeated_units_from(zero, &source_units, allocated, s, f);
            s.release_i32_local(zero, f);
            source_units.clear(f);
            let repeated = s
                .reserve_gc_local(f)
                .initialize(construction.publish(s, f), f);
            b.emit_native_string_normal_reference(&repeated, output, f);
            repeated.clear(f);
            s.release_i32_local(allocated, f);
            s.release_i64_local(target, f);
            s.release_i64_local(length, f);
            s.release_i64_local(count, f);
            s.release_i64_local(bits, f);
            pending.clear(f);
            argument.clear(f);
            Ok(())
        })
    }
}

enum StringPadOperation {
    Start,
    End,
}
impl FunctionBuilder<'_> {
    pub(crate) fn emit_string_prototype_pad_start_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_pad(StringPadOperation::Start, f)
    }
    pub(crate) fn emit_string_prototype_pad_end_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_native_string_pad(StringPadOperation::End, f)
    }

    fn emit_native_string_pad(
        &mut self,
        operation: StringPadOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_with_native_string_receiver(f, |b, string, output, exit, f| {
            let s = b.runtime_schema();
            let target_argument = s.reserve_value_local(f);
            let filler_argument = s.reserve_value_local(f);
            let pending = s.reserve_completion(f);
            let length = s.reserve_i64_local(f);
            let target = s.reserve_i64_local(f);
            let fill_length = s.reserve_i64_local(f);
            let fill_count = s.reserve_i64_local(f);
            let allocated = s.reserve_i32_local(f);
            b.emit_native_gc_string_length(string, length, f);
            b.emit_builtin_arg_to_value(0, &target_argument, f);
            b.emit_builtin_arg_to_value(1, &filler_argument, f);
            b.emit_to_length_i64_from_value_locals(&target_argument, target, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.emit_native_string_normal_reference(string, output, f);
            target.load(f);
            length.load(f);
            f.instruction(&Instruction::I64LeU);
            b.emit_branch_if_to_target(exit, f);
            filler_argument.tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
            f.instruction(&Instruction::I32Eq);
            b.open_frame(ControlFrameKind::If, f);
            let space = s
                .reserve_gc_local(f)
                .initialize(b.emit_native_string_static(" ", f), f);
            pending.value().set_reference(&space, s, f);
            pending.set_normal(pending.value(), f);
            space.clear(f);
            f.instruction(&Instruction::Else);
            b.emit_value_to_string_payload(&filler_argument, &pending, f)?;
            b.emit_native_string_abrupt_exit(&pending, output, exit, f);
            b.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            let filler = s
                .reserve_gc_local(f)
                .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
            b.emit_native_gc_string_length(&filler, fill_length, f);
            fill_length.load(f);
            f.instruction(&Instruction::I64Eqz);
            b.emit_branch_if_to_target(exit, f);
            // The empty filler returns before enforcing the engine's GC extent.
            target.load(f);
            f.instruction(&Instruction::I64Const(u32::MAX as i64));
            f.instruction(&Instruction::I64GtU);
            b.emit_native_string_error_if(
                RuntimeErrorMessage::REPEAT_RESULT_WOULD_EXCEED_MAXIMUM_STRING_LENGTH,
                NativeErrorKind::RangeError,
                output,
                exit,
                f,
            )?;
            target.load(f);
            length.load(f);
            f.instruction(&Instruction::I64Sub);
            fill_count.store(f);
            target.load(f);
            f.instruction(&Instruction::I32WrapI64);
            allocated.store(f);
            let construction = StringConstruction::allocate(s, s.reserve_gc_local(f), allocated, f);
            let source_units = s.reserve_gc_local(f).initialize(
                s.struct_type::<StringValue>()
                    .field(StringValueSchema::CODE_UNITS)
                    .read(string, s, f)
                    .reference(),
                f,
            );
            let fill_units = s.reserve_gc_local(f).initialize(
                s.struct_type::<StringValue>()
                    .field(StringValueSchema::CODE_UNITS)
                    .read(&filler, s, f)
                    .reference(),
                f,
            );
            let source_count = s.reserve_i32_local(f);
            let fill_extent = s.reserve_i32_local(f);
            let zero = s.reserve_i32_local(f);
            length.load(f);
            f.instruction(&Instruction::I32WrapI64);
            source_count.store(f);
            fill_count.load(f);
            f.instruction(&Instruction::I32WrapI64);
            fill_extent.store(f);
            zero.set_constant(0, f);
            match &operation {
                StringPadOperation::Start => {
                    construction.copy_repeated_units_from(zero, &fill_units, fill_extent, s, f);
                    construction.copy_units_from(fill_extent, &source_units, source_count, s, f);
                }
                StringPadOperation::End => {
                    construction.copy_units_from(zero, &source_units, source_count, s, f);
                    construction.copy_repeated_units_from(
                        source_count,
                        &fill_units,
                        fill_extent,
                        s,
                        f,
                    );
                }
            }
            s.release_i32_local(zero, f);
            s.release_i32_local(fill_extent, f);
            s.release_i32_local(source_count, f);
            fill_units.clear(f);
            source_units.clear(f);
            let padded = s
                .reserve_gc_local(f)
                .initialize(construction.publish(s, f), f);
            b.emit_native_string_normal_reference(&padded, output, f);
            padded.clear(f);
            filler.clear(f);
            s.release_i32_local(allocated, f);
            s.release_i64_local(fill_count, f);
            s.release_i64_local(fill_length, f);
            s.release_i64_local(target, f);
            s.release_i64_local(length, f);
            pending.clear(f);
            filler_argument.clear(f);
            target_argument.clear(f);
            Ok(())
        })
    }
}
