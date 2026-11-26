//! The hot entry of SerializeJSONProperty for a plain context.
//!
//! A JSON.stringify call is in a plain context when it has no replacer
//! function, no property list and an empty gap; the entry decides this once
//! and every value of the call then enters here, already fetched (no holder
//! is observable without a replacer function). Only two kinds of value are
//! serialized in this body:
//!
//! - an actual Array (`ArrayObject`), whose `IsArray` is true and whose
//!   "length" is its own data property;
//! - an actual ordinary object (`OrdinaryObject`), which is neither callable,
//!   nor an Array, nor a Proxy, a primitive wrapper or a raw JSON object;
//!
//! and only when the call-free chain walk proves, before anything else is
//! observed, that the value and every object on its prototype chain lack
//! "toJSON". That walk reads each object's live storage and runs nothing,
//! so its answer is exactly what Get(value, "toJSON") would return
//! (undefined), and steps 2 to 4 of SerializeJSONProperty have no effect:
//! no toJSON call, no replacer, no unboxing. The intrinsic prototypes are
//! walked like any other object; a protector that answers for an unchanged
//! %Object.prototype% / %Array.prototype% can replace the walk's last hops
//! at `emit_json_plain_to_json_absent_or_branch` without changing anything
//! else here.
//!
//! Every other value (a primitive root, a value with "toJSON" anywhere on
//! its chain or a chain the walk cannot prove, a Proxy, a wrapper, a
//! BigInt, a raw JSON object, a function, any exotic object) is handed
//! whole, by a tail call and before anything is observed, to the general
//! helper, which performs every step itself and hands its own members back
//! here. Member Gets, getter calls, enumeration order, the cycle TypeError
//! and every exception are the general walk's: this body is that walk with
//! the replacer, property-list and gap cases removed.
use super::*;
use crate::objects::{ChainAnswer, ChainKey};
use crate::runtime_helpers::{JsonStringifyPlainValueArguments, JsonStringifyPlainValueParameters};

impl FunctionBuilder<'_> {
    /// Whether this JSON.stringify call is in a plain context: the replacer
    /// is no function (`callable` is false), there is no property list and
    /// the gap is empty.
    pub(super) fn emit_json_plain_context_i32(
        &self,
        callable: I32Local,
        property_list: &GcLocal<ValueArray, Nullable>,
        gap: &GcLocal<StringValue>,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let plain = s.reserve_i32_local(f);
        let gap_length = s.reserve_i64_local(f);
        self.emit_native_gc_string_length(gap, gap_length, f);
        callable.load(f);
        f.instruction(&Instruction::I32Eqz);
        property_list.load(s, f).is_null(f);
        f.instruction(&Instruction::I32And);
        gap_length.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32And);
        plain.store(f);
        s.release_i64_local(gap_length, f);
        plain
    }

    /// Calls the hot entry for `value`, already fetched from its holder
    /// under `key`, and leaves the appended flag in `out`: Undefined when
    /// nothing was appended. Only a plain context may call it.
    pub(super) fn emit_json_stringify_plain_call(
        &mut self,
        value: &ValueLocals,
        key: &ValueLocals,
        context: &GcLocal<JsonStringifyContext>,
        indent: I64Local,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let result = s.reserve_completion(f);
        // Nested containers recurse through the hot entry as deeply as the
        // value nests: a guarded recursive edge, like the general helper's.
        s.emit_native_recursion_exhausted_i32(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_runtime_error(
            NativeErrorKind::RangeError,
            RuntimeErrorMessage::MAXIMUM_NATIVE_RECURSION_DEPTH_EXCEEDED,
            &result,
            f,
        )?;
        f.instruction(&Instruction::Else);
        let base = self.runtime_helper_base()?;
        let arguments = JsonStringifyPlainValueArguments::new(
            value,
            key,
            context,
            indent,
            self.current_program(),
        );
        s.emit_native_recursion_counted(f, |f| {
            s.call_helper(arguments, base, f).store(&result, f);
            Ok::<(), EmitError>(())
        })?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Only the abrupt edge writes this function's completion.
        self.emit_propagate_pending_throw(&result, &[], f);
        out.copy_from(result.value(), f);
        result.clear(f);
        Ok(())
    }

    pub(crate) fn compile_json_stringify_plain_value_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::JsonStringifyPlainValue);
        let parameters = self.helper_parameters::<JsonStringifyPlainValueParameters>(&mut f);
        self.completion().initialize(&mut f);
        let s = self.runtime_schema();
        let general = self.open_frame(ControlFrameKind::Block, &mut f);
        self.rarely_taken(general, &mut f);
        self.emit_json_plain_container_or_branch(
            &parameters.value,
            &parameters.context,
            parameters.indent,
            general,
            &mut f,
        )?;
        let appended = s.reserve_value_local(&mut f);
        appended.set_scalar(ScalarValue::Boolean(true), &mut f);
        self.completion().set_normal(&appended, &mut f);
        appended.clear(&mut f);
        self.completion().emit(&mut f);
        f.instruction(&Instruction::Return);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // Nothing has been observed yet: the general helper performs the
        // whole SerializeJSONProperty of the fetched value in this frame.
        s.return_call_helper(
            JsonStringifyValueArguments::new(
                &parameters.value,
                &parameters.key,
                GcIndex::Constant(1),
                &parameters.context,
                parameters.indent,
                self.current_program(),
            ),
            self.runtime_helper_base()?,
            &mut f,
        );
        parameters.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    /// Serializes an actual Array or ordinary object whose chain provably
    /// lacks "toJSON", and branches to `general`, having observed nothing,
    /// for every other value.
    fn emit_json_plain_container_or_branch(
        &mut self,
        value: &ValueLocals,
        context: &GcLocal<JsonStringifyContext>,
        indent: I64Local,
        general: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let is_array = s.reserve_i32_local(f);
        let selected_header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let selected = self.open_frame(ControlFrameKind::Block, f);
        let ordinary = self.open_frame(ControlFrameKind::Block, f);
        let array = s.reserve_gc_local(f).initialize(
            value.sealed_reference_or_branch::<ArrayObject>(
                s,
                self.local_branch_label(ordinary),
                f,
            ),
            f,
        );
        selected_header.replace(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&array, s, f)
                .reference()
                .nullable(),
            f,
        );
        array.clear(f);
        is_array.set_constant(1, f);
        self.emit_branch_to_target(selected, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        selected_header.replace(
            value
                .sealed_reference_or_branch::<OrdinaryObject>(
                    s,
                    self.local_branch_label(general),
                    f,
                )
                .nullable(),
            f,
        );
        is_array.set_constant(0, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let header = s
            .reserve_gc_local(f)
            .initialize(selected_header.load(s, f).require_non_null(f), f);
        selected_header.clear(f);
        let captured_compact = s
            .reserve_gc_local::<CompactDataStorage, Nullable>(f)
            .initialize_null(s, f);
        self.emit_json_plain_to_json_absent_or_branch(
            &header,
            &captured_compact,
            general,
            f,
        )?;
        // SerializeJSONProperty steps 2 to 4 had no effect; step 10 serializes
        // `value`, whose IsArray is exactly `is_array`.
        self.emit_json_cycle_enter(value, context, indent, f)?;
        is_array.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(value.cast_reference::<ArrayObject>(s, f), f);
        self.emit_json_plain_array_elements(value, &array, context, indent, f)?;
        array.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_json_plain_object_members(
            value,
            &header,
            &captured_compact,
            context,
            indent,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        captured_compact.clear(f);
        header.clear(f);
        s.release_i32_local(is_array, f);
        Ok(())
    }

    /// Falls through when the object whose header is `header` and every
    /// object on its prototype chain lack "toJSON", which is then exactly
    /// the undefined result of Get(value, "toJSON"); branches to `decline`,
    /// having observed nothing, for every chain the call-free walk cannot
    /// prove that of. "toJSON" is neither an array index nor "length", so an
    /// Array's header (and an Array prototype's) answers for it.
    ///
    /// When the fast compact path proves absence, `captured_compact` is
    /// populated if the storage also satisfies plain-object enumeration
    /// (`OWN_KEYS_ORDERED` and default data attributes), avoiding a second
    /// cast and shape check in `emit_json_plain_object_members`.
    fn emit_json_plain_to_json_absent_or_branch(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        captured_compact: &GcLocal<CompactDataStorage, Nullable>,
        decline: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let default_attributes = crate::heap::StoredPropertyAttributes::Data {
            writable: true,
            enumerable: true,
            configurable: true,
        }
        .descriptor_word()
        .as_i32();
        let proved = self.open_frame(ControlFrameKind::Block, f);
        let slow = self.open_frame(ControlFrameKind::Block, f);
        let prototype = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize(
                s.field(OrdinaryObjectSchema::PROTOTYPE)
                    .read(header, s, f)
                    .reference(),
                f,
            );
        self.emit_prototype_chain_lacks_to_json_or_branch(&prototype, slow, f);
        prototype.clear(f);
        // Prototype chain provably lacks "toJSON". Check only the object's own
        // header: compact shapes carry an immutable LACKS_TO_JSON witness, and
        // empty dictionaries (nearly all Arrays) return absent in 2 loads
        // without interning or hashing "toJSON".
        let storage = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(header, s, f)
                .reference(),
            f,
        );
        let fast_absent = s.reserve_i32_local(f);
        fast_absent.set_constant(0, f);
        let fast_done = self.open_frame(ControlFrameKind::Block, f);
        let try_array_compact = self.open_frame(ControlFrameKind::Block, f);
        let inline_compact = s.reserve_gc_local(f).initialize(
            storage.sealed_nested_subtype_or_branch::<CompactDataStorage, InlineCompactDataStorage>(
                s,
                self.local_branch_label(try_array_compact),
                f,
            ),
            f,
        );
        let shape = s.reserve_gc_local(f).initialize(
            s.field(InlineCompactDataStorageSchema::SHAPE)
                .read(&inline_compact, s, f)
                .reference(),
            f,
        );
        s.field(CompactDataShapeSchema::LACKS_TO_JSON)
            .read(&shape, s, f)
            .store(fast_absent, f);
        s.field(CompactDataShapeSchema::OWN_KEYS_ORDERED)
            .read(&shape, s, f);
        s.field(CompactDataShapeSchema::ATTRIBUTES)
            .read(&shape, s, f);
        f.instruction(&Instruction::I32Const(default_attributes));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        captured_compact.replace(
            inline_compact
                .load(s, f)
                .upcast::<CompactDataStorage>()
                .nullable(),
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        inline_compact.clear(f);
        shape.clear(f);
        self.emit_branch_to_target(fast_done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let try_empty_dict = self.open_frame(ControlFrameKind::Block, f);
        let array_compact = s.reserve_gc_local(f).initialize(
            storage.sealed_nested_subtype_or_branch::<CompactDataStorage, ArrayCompactDataStorage>(
                s,
                self.local_branch_label(try_empty_dict),
                f,
            ),
            f,
        );
        let shape = s.reserve_gc_local(f).initialize(
            s.field(ArrayCompactDataStorageSchema::SHAPE)
                .read(&array_compact, s, f)
                .reference(),
            f,
        );
        s.field(CompactDataShapeSchema::LACKS_TO_JSON)
            .read(&shape, s, f)
            .store(fast_absent, f);
        s.field(CompactDataShapeSchema::OWN_KEYS_ORDERED)
            .read(&shape, s, f);
        s.field(CompactDataShapeSchema::ATTRIBUTES)
            .read(&shape, s, f);
        f.instruction(&Instruction::I32Const(default_attributes));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        captured_compact.replace(
            array_compact
                .load(s, f)
                .upcast::<CompactDataStorage>()
                .nullable(),
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        array_compact.clear(f);
        shape.clear(f);
        self.emit_branch_to_target(fast_done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let dict = s.reserve_gc_local(f).initialize(
            storage.sealed_subtype_or_branch::<OrdinaryPropertyStorage>(
                s,
                self.local_branch_label(fast_done),
                f,
            ),
            f,
        );
        s.field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&dict, s, f);
        dict.clear(f);
        f.instruction(&Instruction::I32Eqz);
        fast_absent.store(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        storage.clear(f);
        fast_absent.load(f);
        s.release_i32_local(fast_absent, f);
        self.emit_branch_if_to_target(proved, f);
        let name = self.emit_json_string("toJSON", f)?;
        let key = crate::operations::PropertyKeyLocals::from_string(s, &name, f);
        let absent = self.emit_ordinary_header_own_property_absent_i32(header, &key, f)?;
        key.clear(f);
        name.clear(f);
        absent.load(f);
        s.release_i32_local(absent, f);
        self.emit_branch_if_to_target(proved, f);
        self.emit_branch_to_target(decline, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let name = self.emit_json_string("toJSON", f)?;
        let hash = s.reserve_i64_local(f);
        self.emit_string_key_hash(&name, hash, f);
        let named = s.reserve_i32_local(f);
        named.set_constant(1, f);
        let scratch = s.reserve_value_local(f);
        self.emit_chain_walk(
            header,
            ChainKey::String { name: &name, hash },
            named,
            ChainAnswer::Absence,
            &scratch,
            decline,
            f,
        );
        scratch.clear(f);
        s.release_i32_local(named, f);
        s.release_i64_local(hash, f);
        name.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        Ok(())
    }

    /// SerializeJSONArray of an actual Array in a plain context, after its
    /// cycle check: every element is Get(value, index) for an index below
    /// the length read once here, and an element that serializes to
    /// undefined is spelled null.
    fn emit_json_plain_array_elements(
        &mut self,
        value: &ValueLocals,
        array: &GcLocal<ArrayObject>,
        context: &GcLocal<JsonStringifyContext>,
        indent: I64Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let accumulator = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonStringifyContext>()
                .field(JsonStringifyContextSchema::OUTPUT)
                .read(context, s, f)
                .reference(),
            f,
        );
        self.emit_json_append_literal(&accumulator, "[", f);
        let length = s.reserve_i64_local(f);
        s.field(ArrayObjectSchema::LENGTH)
            .read(array, s, f)
            .store_i64(length, f);
        let child_depth = s.reserve_i64_local(f);
        indent.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        child_depth.store(f);
        let index = s.reserve_i64_local(f);
        let found = s.reserve_i32_local(f);
        let appended = s.reserve_i32_local(f);
        let key_value = s.reserve_value_local(f);
        let entry = s.reserve_value_local(f);
        let result = s.reserve_value_local(f);
        json_i64(index, 0, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        index.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_literal(&accumulator, ",", f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        index.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        key_value.scalar().store(f);
        key_value.set_number(key_value.scalar(), f);
        let serialized = self.open_frame(ControlFrameKind::Block, f);
        // Get(value, index): an own data element directly, and otherwise
        // (a hole, an accessor, an index past a shrunk storage) Get.
        self.emit_array_own_indexed_value(array, index, &entry, found, f);
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let text = s.reserve_gc_local(f).initialize(
            self.emit_number_to_string_payload(key_value.scalar(), f)?,
            f,
        );
        self.emit_json_get(value, &text, &entry, f)?;
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        json_i32(appended, 1, f);
        result.set_boolean(appended, f);
        self.emit_json_serialize_primitive(&entry, &accumulator, serialized, f)?;
        result.set_undefined(f);
        self.emit_json_tag_test(&entry, WasmRuntimeValueTag::Undefined, f);
        self.emit_branch_if_to_target(serialized, f);
        self.emit_json_stringify_plain_call(&entry, &key_value, context, child_depth, &result, f)?;
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_json_tag_test(&result, WasmRuntimeValueTag::Undefined, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_json_append_literal(&accumulator, "null", f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        json_increment(index, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_json_append_literal(&accumulator, "]", f);
        result.clear(f);
        entry.clear(f);
        key_value.clear(f);
        s.release_i32_local(appended, f);
        s.release_i32_local(found, f);
        s.release_i64_local(index, f);
        s.release_i64_local(child_depth, f);
        s.release_i64_local(length, f);
        accumulator.clear(f);
        Ok(())
    }

    /// SerializeJSONObject of an actual ordinary object in a plain context,
    /// after its cycle check. The keys are EnumerableOwnProperties(value,
    /// key), snapshotted before any member is read: a plain data object's
    /// shape keys, or the filtered own-keys table. Each member's separator
    /// and key are appended before its Get and truncated away again when the
    /// member serializes to undefined.
    fn emit_json_plain_object_members(
        &mut self,
        value: &ValueLocals,
        header: &GcLocal<OrdinaryObject>,
        captured_compact: &GcLocal<CompactDataStorage, Nullable>,
        context: &GcLocal<JsonStringifyContext>,
        indent: I64Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let accumulator = s.reserve_gc_local(f).initialize(
            s.struct_type::<JsonStringifyContext>()
                .field(JsonStringifyContextSchema::OUTPUT)
                .read(context, s, f)
                .reference(),
            f,
        );
        self.emit_json_append_literal(&accumulator, "{", f);
        let count = s.reserve_i32_local(f);
        let keys = s
            .reserve_gc_local::<PropertyKeyTable, Nullable>(f)
            .initialize_null(s, f);
        let plain_keys = s
            .reserve_gc_local::<CompactShapeKeys, Nullable>(f)
            .initialize_null(s, f);
        // Reuse the compact storage captured during the toJSON check when
        // available; only fall back to emit_plain_data_object_storage when the
        // slow prototype walk ran.
        let plain = s
            .reserve_gc_local::<CompactDataStorage, Nullable>(f)
            .initialize(captured_compact.load(s, f), f);
        plain.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let probed = self.emit_plain_data_object_storage(value, f);
        plain.replace(probed.load(s, f), f);
        probed.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        plain.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let snapshot = self.emit_json_enumerable_key_table(value, count, f)?;
        keys.replace(snapshot.load(s, f).nullable(), f);
        snapshot.clear(f);
        f.instruction(&Instruction::Else);
        // The shape's keys, in order, are EnumerableOwnProperties(value, key).
        let storage = s
            .reserve_gc_local(f)
            .initialize(plain.load(s, f).require_non_null(f), f);
        let shape_keys = self.emit_compact_shape_keys(&storage, f);
        s.array_type::<CompactShapeKeys>().length(&shape_keys, s, f);
        count.store(f);
        plain_keys.replace(shape_keys.load(s, f).nullable(), f);
        shape_keys.clear(f);
        storage.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let child_depth = s.reserve_i64_local(f);
        indent.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        child_depth.store(f);
        let item = s.reserve_i32_local(f);
        let emitted = s.reserve_i32_local(f);
        let mark = s.reserve_i32_local(f);
        let appended = s.reserve_i32_local(f);
        let key_value = s.reserve_value_local(f);
        let entry = s.reserve_value_local(f);
        let result = s.reserve_value_local(f);
        json_i32(item, 0, f);
        json_i32(emitted, 0, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        item.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, f);
        plain_keys.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let shape_keys = s
            .reserve_gc_local(f)
            .initialize(plain_keys.load(s, f).require_non_null(f), f);
        let name = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactShapeKeys>()
                .read(&shape_keys, item, s, f)
                .reference(),
            f,
        );
        key_value.set_reference(&name, s, f);
        name.clear(f);
        shape_keys.clear(f);
        f.instruction(&Instruction::Else);
        let table = s
            .reserve_gc_local(f)
            .initialize(keys.load(s, f).require_non_null(f), f);
        // The snapshot holds only accepted String keys.
        let stored = s.reserve_gc_local(f).initialize(
            s.array_type::<PropertyKeyTable>()
                .read(&table, item, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&stored, &key_value, s, f);
        stored.clear(f);
        table.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Save the used length before this member and emit optional `,`,
        // quoted key, and `:` under one cursor open/reserve/close.
        let name = s
            .reserve_gc_local(f)
            .initialize(key_value.cast_reference::<StringValue>(s, f), f);
        self.emit_json_quote_object_key_into(&accumulator, &name, emitted, mark, f)?;
        name.clear(f);
        let serialized = self.open_frame(ControlFrameKind::Block, f);
        // Get(value, key): the captured storage's slot while it is still
        // the object's storage, and otherwise the general own-data read or
        // Get.
        let got = self.open_frame(ControlFrameKind::Block, f);
        let changed = self.open_frame(ControlFrameKind::Block, f);
        plain.load(s, f).is_null(f);
        self.emit_branch_if_to_target(changed, f);
        let storage = s
            .reserve_gc_local(f)
            .initialize(plain.load(s, f).require_non_null(f), f);
        self.emit_plain_data_header_value_or_branch(header, &storage, item, &entry, changed, f);
        storage.clear(f);
        self.emit_branch_to_target(got, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_json_property_get(value, &key_value, &entry, f)?;
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // A primitive has no toJSON lookup and, here, no replacer call:
        // SerializeJSONProperty only appends it, or returns undefined.
        json_i32(appended, 1, f);
        result.set_boolean(appended, f);
        self.emit_json_serialize_primitive(&entry, &accumulator, serialized, f)?;
        result.set_undefined(f);
        self.emit_json_tag_test(&entry, WasmRuntimeValueTag::Undefined, f);
        self.emit_branch_if_to_target(serialized, f);
        self.emit_json_stringify_plain_call(&entry, &key_value, context, child_depth, &result, f)?;
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_json_tag_test(&result, WasmRuntimeValueTag::Undefined, f);
        self.open_frame(ControlFrameKind::If, f);
        // An undefined member contributes neither separator nor key.
        let output = StringAccumulatorCursor::open(&accumulator, s, f);
        output.truncate(mark, f);
        output.close(&accumulator, s, f);
        f.instruction(&Instruction::Else);
        json_i32(emitted, 1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        item.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        item.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_json_append_literal(&accumulator, "}", f);
        result.clear(f);
        entry.clear(f);
        key_value.clear(f);
        s.release_i32_local(appended, f);
        s.release_i32_local(mark, f);
        s.release_i32_local(emitted, f);
        s.release_i32_local(item, f);
        s.release_i64_local(child_depth, f);
        plain.clear(f);
        plain_keys.clear(f);
        keys.clear(f);
        s.release_i32_local(count, f);
        accumulator.clear(f);
        Ok(())
    }
}
