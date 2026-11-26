//! Realm-owned legacy RegExp state is updated only by a completed builtin match.
use super::*;
use crate::builtins::regexp::ValidatedRegExpProgramLayoutLocals;
use crate::intrinsics::{IntrinsicInstall, IntrinsicKey};
use crate::objects::{AccessorDescriptorLocals, AccessorGetterLocals, AccessorSetterLocals};

pub(in crate::builtins) enum RegExpLegacyAccessorKind {
    Getter,
    InputSetter,
}

impl RegExpLegacySlot {
    pub(crate) const ALL: [Self; 14] = [
        Self::Input,
        Self::LastMatch,
        Self::LastParen,
        Self::LeftContext,
        Self::RightContext,
        Self::Paren1,
        Self::Paren2,
        Self::Paren3,
        Self::Paren4,
        Self::Paren5,
        Self::Paren6,
        Self::Paren7,
        Self::Paren8,
        Self::Paren9,
    ];

    pub(crate) const fn names(self) -> &'static [&'static str] {
        match self {
            Self::Input => &["input", "$_"],
            Self::LastMatch => &["lastMatch", "$&"],
            Self::LastParen => &["lastParen", "$+"],
            Self::LeftContext => &["leftContext", "$`"],
            Self::RightContext => &["rightContext", "$'"],
            Self::Paren1 => &["$1"],
            Self::Paren2 => &["$2"],
            Self::Paren3 => &["$3"],
            Self::Paren4 => &["$4"],
            Self::Paren5 => &["$5"],
            Self::Paren6 => &["$6"],
            Self::Paren7 => &["$7"],
            Self::Paren8 => &["$8"],
            Self::Paren9 => &["$9"],
        }
    }

    fn field(self) -> GcField<RegExpLegacyState, GcRef<StringValue>, Mutable, Nullable> {
        match self {
            Self::Input => RegExpLegacyStateSchema::INPUT,
            Self::LastMatch => RegExpLegacyStateSchema::LAST_MATCH,
            Self::LastParen => RegExpLegacyStateSchema::LAST_PAREN,
            Self::LeftContext => RegExpLegacyStateSchema::LEFT_CONTEXT,
            Self::RightContext => RegExpLegacyStateSchema::RIGHT_CONTEXT,
            Self::Paren1 => RegExpLegacyStateSchema::PAREN1,
            Self::Paren2 => RegExpLegacyStateSchema::PAREN2,
            Self::Paren3 => RegExpLegacyStateSchema::PAREN3,
            Self::Paren4 => RegExpLegacyStateSchema::PAREN4,
            Self::Paren5 => RegExpLegacyStateSchema::PAREN5,
            Self::Paren6 => RegExpLegacyStateSchema::PAREN6,
            Self::Paren7 => RegExpLegacyStateSchema::PAREN7,
            Self::Paren8 => RegExpLegacyStateSchema::PAREN8,
            Self::Paren9 => RegExpLegacyStateSchema::PAREN9,
        }
    }
}

impl FunctionBuilder<'_> {
    fn emit_realm_regexp_legacy_state(
        &self,
        realm: &GcLocal<RealmRecord>,
        f: &mut Function,
    ) -> GcStackReference<RegExpLegacyState> {
        let s = self.runtime_schema();
        let field = s.field(RealmRecordSchema::REGEXP_LEGACY);
        let state = s
            .reserve_gc_local(f)
            .initialize(field.read(realm, s, f).reference(), f);
        state.load(s, f).is_null(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        // A materialized state can itself contain null slots after a match
        // on a legacy-disabled RegExp. A null Realm field means every slot
        // still has its initial empty String value.
        let empty = s.reserve_gc_local(f).initialize(
            self.emit_interned_string_reference("", f)
                .expect("Realm bootstrap owns the canonical empty string"),
            f,
        );
        let initial = s.struct_type::<RegExpLegacyState>().construct(
            (
                GcOperand::nullable_reference(&empty, s),
                GcOperand::nullable_reference(&empty, s),
                GcOperand::nullable_reference(&empty, s),
                GcOperand::nullable_reference(&empty, s),
                GcOperand::nullable_reference(&empty, s),
                GcOperand::nullable_reference(&empty, s),
                GcOperand::nullable_reference(&empty, s),
                GcOperand::nullable_reference(&empty, s),
                GcOperand::nullable_reference(&empty, s),
                GcOperand::nullable_reference(&empty, s),
                GcOperand::nullable_reference(&empty, s),
                GcOperand::nullable_reference(&empty, s),
                GcOperand::nullable_reference(&empty, s),
                GcOperand::nullable_reference(&empty, s),
            ),
            f,
        );
        state.replace(initial.nullable(), f);
        empty.clear(f);
        field.write(realm, GcOperand::reference(&state, s), s, f);
        f.instruction(&Instruction::End);
        let result = state.load(s, f).require_non_null(f);
        state.clear(f);
        result
    }

    pub(crate) fn emit_install_regexp_legacy_accessors(
        &mut self,
        context: &IntrinsicInstall<'_>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        for slot in RegExpLegacySlot::ALL {
            let owner = s.reserve_gc_local(f).initialize(
                s.struct_type::<RegExpLegacyAccessorContext>()
                    .construct((GcOperand::constant(slot),), f),
                f,
            );
            let capture = s
                .reserve_gc_local::<BuiltinClosureCapture, Nullable>(f)
                .initialize(
                    s.struct_type::<BuiltinClosureCapture>()
                        .publish(BuiltinClosurePayload::RegExpLegacyAccessor(&owner), s, f)
                        .nullable(),
                    f,
                );
            // One accessor belongs to the captured slot. Aliases publish the
            // same completed function, with its canonical initial name.
            let name = slot.names()[0];
            let mut meta = self
                .functions
                .get(&StandardBuiltinId::RegExpLegacyStaticGetter.function_id())
                .cloned()
                .ok_or_else(|| EmitError::unsupported("missing planned RegExp legacy getter"))?;
            meta.name = format!("get {name}");
            meta.to_string_value =
                scriptw_ir::CallableToStringRepresentation::NativeNamed(meta.name.clone())
                    .materialize();
            let getter = s.reserve_gc_local(f).initialize(
                self.emit_function_value_payload_in_realm_with_capture(
                    &meta,
                    context.realm,
                    &capture,
                    f,
                )?,
                f,
            );
            let get_value = s.reserve_value_local(f);
            get_value.set_reference(&getter, s, f);
            let set_value = s.reserve_value_local(f);
            if slot == RegExpLegacySlot::Input {
                let mut meta = self
                    .functions
                    .get(&StandardBuiltinId::RegExpLegacyStaticSetter.function_id())
                    .cloned()
                    .ok_or_else(|| {
                        EmitError::unsupported("missing planned RegExp legacy setter")
                    })?;
                meta.name = format!("set {name}");
                meta.to_string_value =
                    scriptw_ir::CallableToStringRepresentation::NativeNamed(meta.name.clone())
                        .materialize();
                let setter = s.reserve_gc_local(f).initialize(
                    self.emit_function_value_payload_in_realm_with_capture(
                        &meta,
                        context.realm,
                        &capture,
                        f,
                    )?,
                    f,
                );
                set_value.set_reference(&setter, s, f);
                setter.clear(f);
            } else {
                set_value.set_undefined(f);
            }
            for name in slot.names() {
                self.emit_install_intrinsic_accessor_values(
                    context.constructor,
                    IntrinsicKey::Name(name),
                    AccessorDescriptorLocals::GetterAndSetter {
                        getter: AccessorGetterLocals::new(&get_value),
                        setter: AccessorSetterLocals::new(&set_value),
                    },
                    true,
                    f,
                )?;
            }
            set_value.clear(f);
            get_value.clear(f);
            getter.clear(f);
            capture.clear(f);
            owner.clear(f);
        }
        Ok(())
    }

    pub(in crate::builtins) fn emit_regexp_legacy_static_accessor(
        &mut self,
        access: RegExpLegacyAccessorKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let constructor = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let result = s.reserve_completion(f);
        self.compile_this_to_locals(&receiver, f)?;
        let realm = self.emit_execution_realm(f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::RegExpConstructor,
            &constructor,
            f,
        );
        result.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        receiver.reference().load(f);
        constructor.reference().load(f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::REGEXP_LEGACY_STATIC_ACCESSOR_RECEIVER_MUST_BE_REGEXP,
            NativeErrorKind::TypeError,
            &result,
            exit,
            f,
        )?;
        match access {
            RegExpLegacyAccessorKind::InputSetter => {
                // Receiver validation precedes ToString. A failed conversion
                // retains all state; restoring input does not restore other slots.
                let argument = s.reserve_value_local(f);
                self.emit_builtin_arg_to_value(0, &argument, f);
                self.emit_value_to_string_payload(&argument, &pending, f)?;
                self.emit_native_string_abrupt_exit(&pending, &result, exit, f);
                let text = s
                    .reserve_gc_local(f)
                    .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
                let state = s
                    .reserve_gc_local(f)
                    .initialize(self.emit_realm_regexp_legacy_state(&realm, f), f);
                s.field(RegExpLegacyStateSchema::INPUT).write(
                    &state,
                    GcOperand::nullable_reference(&text, s),
                    s,
                    f,
                );
                state.clear(f);
                text.clear(f);
                argument.clear(f);
            }
            RegExpLegacyAccessorKind::Getter => {
                let state = s
                    .reserve_gc_local(f)
                    .initialize(self.emit_realm_regexp_legacy_state(&realm, f), f);
                let context = self
                    .body_entry_locals()
                    .expect("legacy accessor entry")
                    .function_context()
                    .expect("actual legacy getter context");
                let capture = s.reserve_gc_local(f).initialize(
                    s.field(FunctionContextSchema::BUILTIN_CAPTURE)
                        .read(context, s, f)
                        .reference()
                        .require_non_null(f),
                    f,
                );
                let actual_kind = s.reserve_i32_local(f);
                s.field(BuiltinClosureCaptureSchema::KIND)
                    .read(&capture, s, f)
                    .store(actual_kind, f);
                actual_kind.load(f);
                f.instruction(&Instruction::I32Const(GcI32Constant::encode(
                    BuiltinClosureCaptureKind::RegExpLegacyAccessor,
                )));
                f.instruction(&Instruction::I32Ne);
                self.open_frame(ControlFrameKind::If, f);
                f.instruction(&Instruction::Unreachable);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                let owner = s.reserve_gc_local(f).initialize(
                    s.field(BuiltinClosureCaptureSchema::REGEXP_LEGACY_ACCESSOR)
                        .read(&capture, s, f)
                        .reference()
                        .require_non_null(f),
                    f,
                );
                let selected = s.reserve_i32_local(f);
                s.field(RegExpLegacyAccessorContextSchema::SLOT)
                    .read(&owner, s, f)
                    .store(selected, f);
                for slot in RegExpLegacySlot::ALL {
                    selected.load(f);
                    f.instruction(&Instruction::I32Const(GcI32Constant::encode(slot)));
                    f.instruction(&Instruction::I32Eq);
                    self.open_frame(ControlFrameKind::If, f);
                    let available = s
                        .reserve_gc_local(f)
                        .initialize(s.field(slot.field()).read(&state, s, f).reference(), f);
                    available.load(s, f);
                    f.instruction(&Instruction::RefIsNull);
                    self.emit_native_string_error_if(
                        RuntimeErrorMessage::REGEXP_LEGACY_STATIC_STATE_IS_UNAVAILABLE,
                        NativeErrorKind::TypeError,
                        &result,
                        exit,
                        f,
                    )?;
                    let text = s
                        .reserve_gc_local(f)
                        .initialize(available.load(s, f).require_non_null(f), f);
                    self.emit_native_string_normal_reference(&text, &result, f);
                    text.clear(f);
                    available.clear(f);
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                }
                s.release_i32_local(selected, f);
                owner.clear(f);
                s.release_i32_local(actual_kind, f);
                capture.clear(f);
                state.clear(f);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&result, f);
        result.clear(f);
        pending.clear(f);
        constructor.clear(f);
        receiver.clear(f);
        realm.clear(f);
        Ok(())
    }

    pub(super) fn emit_regexp_legacy_compile_guard(
        &mut self,
        object: &GcLocal<RegExpObject>,
        result: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let executing = self.emit_execution_realm(f);
        let defining = s.reserve_gc_local(f).initialize(
            s.field(RegExpObjectSchema::REALM)
                .read(object, s, f)
                .reference(),
            f,
        );
        executing.load(s, f);
        defining.load(s, f);
        f.instruction(&Instruction::RefEq);
        let enabled = s.reserve_i32_local(f);
        s.field(RegExpObjectSchema::LEGACY_FEATURES_ENABLED)
            .read(object, s, f)
            .store(enabled, f);
        enabled.load(f);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::REGEXP_LEGACY_COMPILE_REQUIRES_ORIGINAL_REALM_AND_ENABLED_RECEIVER,
            NativeErrorKind::TypeError,
            result,
            exit,
            f,
        )?;
        s.release_i32_local(enabled, f);
        defining.clear(f);
        executing.clear(f);
        Ok(())
    }

    pub(super) fn emit_regexp_legacy_match_update(
        &mut self,
        object: &GcLocal<RegExpObject>,
        input: &GcLocal<StringValue>,
        input_length: I64Local,
        layout: &ValidatedRegExpProgramLayoutLocals,
        start: I64Local,
        end: I64Local,
        completed: &CompletionLocals,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let executing = self.emit_execution_realm(f);
        let defining = s.reserve_gc_local(f).initialize(
            s.field(RegExpObjectSchema::REALM)
                .read(object, s, f)
                .reference(),
            f,
        );
        executing.load(s, f);
        defining.load(s, f);
        f.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, f);
        let state = s
            .reserve_gc_local(f)
            .initialize(self.emit_realm_regexp_legacy_state(&defining, f), f);
        let enabled = s.reserve_i32_local(f);
        s.field(RegExpObjectSchema::LEGACY_FEATURES_ENABLED)
            .read(object, s, f)
            .store(enabled, f);
        enabled.load(f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(RegExpLegacyStateSchema::INPUT).write(
            &state,
            GcOperand::nullable_reference(input, s),
            s,
            f,
        );
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_gc_string_slice(input, start, end, f), f);
        s.field(RegExpLegacyStateSchema::LAST_MATCH).write(
            &state,
            GcOperand::nullable_reference(&text, s),
            s,
            f,
        );
        text.clear(f);
        let zero = s.reserve_i64_local(f);
        f.instruction(&Instruction::I64Const(0));
        zero.store(f);
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_gc_string_slice(input, zero, start, f), f);
        s.field(RegExpLegacyStateSchema::LEFT_CONTEXT).write(
            &state,
            GcOperand::nullable_reference(&text, s),
            s,
            f,
        );
        text.clear(f);
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_gc_string_slice(input, end, input_length, f), f);
        s.field(RegExpLegacyStateSchema::RIGHT_CONTEXT).write(
            &state,
            GcOperand::nullable_reference(&text, s),
            s,
            f,
        );
        text.clear(f);
        let matches = s
            .reserve_gc_local(f)
            .initialize(completed.value().cast_reference::<ArrayObject>(s, f), f);
        let empty = s
            .reserve_gc_local(f)
            .initialize(self.emit_native_string_static("", f), f);
        let value = s.reserve_value_local(f);
        let index = s.reserve_i64_local(f);
        for (ordinal, slot) in [
            RegExpLegacySlot::Paren1,
            RegExpLegacySlot::Paren2,
            RegExpLegacySlot::Paren3,
            RegExpLegacySlot::Paren4,
            RegExpLegacySlot::Paren5,
            RegExpLegacySlot::Paren6,
            RegExpLegacySlot::Paren7,
            RegExpLegacySlot::Paren8,
            RegExpLegacySlot::Paren9,
        ]
        .into_iter()
        .enumerate()
        {
            value.set_reference(&empty, s, f);
            layout.capture_count().load(f);
            f.instruction(&Instruction::I64Const((ordinal + 1) as i64));
            f.instruction(&Instruction::I64GeU);
            self.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I64Const((ordinal + 1) as i64));
            index.store(f);
            self.emit_native_string_array_read(&matches, index, &value, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            self.emit_regexp_legacy_capture_store(&state, slot, &value, &empty, f);
        }
        value.set_reference(&empty, s, f);
        layout.capture_count().load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        layout.capture_count().load(f);
        index.store(f);
        self.emit_native_string_array_read(&matches, index, &value, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_regexp_legacy_capture_store(
            &state,
            RegExpLegacySlot::LastParen,
            &value,
            &empty,
            f,
        );
        s.release_i64_local(index, f);
        value.clear(f);
        empty.clear(f);
        matches.clear(f);
        s.release_i64_local(zero, f);
        f.instruction(&Instruction::Else);
        for slot in RegExpLegacySlot::ALL {
            s.field(slot.field())
                .write(&state, GcOperand::null(s), s, f);
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(enabled, f);
        state.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        defining.clear(f);
        executing.clear(f);
    }

    fn emit_regexp_legacy_capture_store(
        &mut self,
        state: &GcLocal<RegExpLegacyState>,
        slot: RegExpLegacySlot,
        value: &ValueLocals,
        empty: &GcLocal<StringValue>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        s.field(slot.field())
            .write(state, GcOperand::nullable_reference(empty, s), s, f);
        f.instruction(&Instruction::Else);
        let text = s
            .reserve_gc_local(f)
            .initialize(value.cast_reference::<StringValue>(s, f), f);
        s.field(slot.field())
            .write(state, GcOperand::nullable_reference(&text, s), s, f);
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }
}
