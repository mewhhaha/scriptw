//! 1-probe inline fast paths for `WeakMap.prototype.get(key)` and
//! `WeakMap.prototype.set(key, value)`.
//!
//! Layout and probe invariants match `builtins/weak/collections/map/table.rs`
//! byte-for-byte:
//! - Fibonacci hash multiplier: `0x9e37_79b9_7f4a_7c15_u64`
//! - Initial slot: `(((id * MULTIPLIER) as u64 >> 32) as i32) & (cap - 1)`
//! - `HASHES[slot] == 0` denotes a virgin (never-occupied) slot
//! - `REFERENCES[slot << 1]` holds the weak key reference (null for tombstones)
//! - `REFERENCES[(slot << 1) | 1]` holds the value reference
//! - `TAGS[slot]` and `SCALARS[slot]` hold the value tag and scalar payload
//! - Load limit before rebuild: `USED < cap - (cap >> 2)` (unsigned)
//!
//! Any guard miss, Symbol key, exotic/proxy key, unminted `KEY_HASH_ID`, null
//! table, hash collision, tombstone, or load-limit rehash falls back to R's
//! canonical dispatch with the exact once-evaluated operands.
use super::*;
use crate::gc_types::{
    ArrayObject, ArrayObjectSchema, CompactReferenceArray, CompactScalarArray, CompactTagArray,
    FunctionObject, FunctionObjectSchema, GcLocal, GcNullability, GcOperand, I32Local, I64Local,
    NumberBitsI64Array, OrdinaryObject, OrdinaryObjectSchema, ValueLocals, WeakMapObject,
    WeakMapObjectSchema, WeakMapTableSchema,
};
use wasm_encoder::RefType;

/// Fibonacci hash multiplier matching `builtins/weak/collections/map/table.rs`.
const WEAK_MAP_HASH_MULTIPLIER: i64 = 0x9e37_79b9_7f4a_7c15_u64 as i64;

impl FunctionBuilder<'_> {
    /// Extracts the `OrdinaryObject` header for a `key` whose GC reference is
    /// `OrdinaryObject`, `ArrayObject`, or `FunctionObject`. Branches to `miss`
    /// for all other values (including primitives, Symbols, Proxies, and other
    /// exotic objects).
    fn emit_weak_key_header_or_branch(
        &mut self,
        key: &ValueLocals,
        miss: ControlTarget,
        function: &mut Function,
    ) -> GcLocal<OrdinaryObject> {
        let s = self.runtime_schema();
        let header = self.open_reference_block::<OrdinaryObject>(s, function);
        // Fast path 1: OrdinaryObject directly (most common WeakMap key).
        key.reference().load(function);
        function.instruction(&Instruction::BrOnCast {
            relative_depth: 0,
            from_ref_type: RefType::EQREF,
            to_ref_type: s.reference_type::<OrdinaryObject>(GcNullability::NonNullable),
        });
        function.instruction(&Instruction::Drop);

        // Fast path 2: ArrayObject -> .OBJECT.
        let not_array = self.open_frame(ControlFrameKind::Block, function);
        let array = key.sealed_reference_or_branch::<ArrayObject>(
            s,
            self.local_branch_label(not_array),
            function,
        );
        header.branch_with(
            s.field(ArrayObjectSchema::OBJECT)
                .read_top(array, function)
                .reference(),
            function,
        );
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        // Fast path 3: FunctionObject -> .OBJECT; any other type branches to `miss`.
        let callable = key.sealed_reference_or_branch::<FunctionObject>(
            s,
            self.local_branch_label(miss),
            function,
        );
        header.branch_with(
            s.field(FunctionObjectSchema::OBJECT)
                .read_top(callable, function)
                .reference(),
            function,
        );

        let header = header.close(function);
        self.pop_control(ControlFrameKind::Block);
        s.reserve_non_null_gc_local(function)
            .initialize(header, function)
    }

    /// Computes the initial open-addressing probe slot matching
    /// `builtins/weak/collections/map/table.rs::Table::probe`:
    /// `slot = (((id * 0x9e37_79b9_7f4a_7c15) as u64 >> 32) as i32) & (capacity - 1)`.
    fn emit_weak_map_slot(
        id: I64Local,
        capacity: I32Local,
        slot: I32Local,
        function: &mut Function,
    ) {
        id.load(function);
        function.instruction(&Instruction::I64Const(WEAK_MAP_HASH_MULTIPLIER));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I32WrapI64);
        capacity.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        function.instruction(&Instruction::I32And);
        slot.store(function);
    }

    /// 1-probe inline fast path for `WeakMap.prototype.get(key)`.
    ///
    /// Admits both continuing and tail-return calls. Cross-realm sound because
    /// every fast-path arm runs after proving `receiver` is a `WeakMapObject`
    /// and never throws, allocates, invokes user code, or observes Realm state.
    pub(super) fn try_emit_weak_map_get_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let [key_expr] = arguments else {
            return Ok(false);
        };
        if matches!(
            key_expr.expr,
            ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
        ) {
            return Ok(false);
        }
        let s = self.runtime_schema();
        let key = s.reserve_value_local(function);
        self.compile_expr_to_value(key_expr, &key, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        let handled = s.reserve_i32_local(function);
        handled.set_constant(0, function);

        let fallback = self.open_frame(ControlFrameKind::Block, function);
        self.emit_standard_builtin_or_branch(
            callee,
            StandardBuiltinId::WeakMapPrototypeGet,
            fallback,
            function,
        );
        let wm = s.reserve_non_null_gc_local(function).initialize(
            receiver.sealed_reference_or_branch::<WeakMapObject>(
                s,
                self.local_branch_label(fallback),
                function,
            ),
            function,
        );

        let return_undefined = self.open_frame(ControlFrameKind::Block, function);

        // Non-weak primitives (everything except heap objects and Symbols) can
        // never be WeakMap keys -> guaranteed `undefined`. Symbols CAN be held
        // weakly (ES2023+ unregistered Symbols), so never fast-miss Symbols.
        self.emit_is_heap_object_like_tag_i32(key.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        key.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol.tag()));
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::I32And);
        self.emit_branch_if_to_target(return_undefined, function);

        // Extract OrdinaryObject header for OrdinaryObject / ArrayObject /
        // FunctionObject; Symbols, Proxies, and other exotic objects fall back
        // to R.
        let header = self.emit_weak_key_header_or_branch(&key, fallback, function);
        let id = s.reserve_i64_local(function);
        s.field(OrdinaryObjectSchema::KEY_HASH_ID)
            .read(&header, s, function)
            .store_i64(id, function);
        header.clear(function);

        // `KEY_HASH_ID == 0` -> object has never been inserted into any WeakMap.
        id.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(return_undefined, function);

        // Null `TABLE` -> this WeakMap has never had any entries inserted.
        let table = s.reserve_non_null_gc_local(function).initialize(
            s.field(WeakMapObjectSchema::TABLE)
                .read(&wm, s, function)
                .reference()
                .non_null_or_branch(self.local_branch_label(return_undefined), function),
            function,
        );
        wm.clear(function);

        let hashes = s.reserve_non_null_gc_local(function).initialize(
            s.field(WeakMapTableSchema::HASHES)
                .read(&table, s, function)
                .reference(),
            function,
        );
        let capacity = s.reserve_i32_local(function);
        s.array_type::<NumberBitsI64Array>()
            .length(&hashes, s, function);
        capacity.store(function);
        let slot = s.reserve_i32_local(function);
        Self::emit_weak_map_slot(id, capacity, slot, function);
        s.release_i32_local(capacity, function);

        let entry_hash = s.reserve_i64_local(function);
        s.array_type::<NumberBitsI64Array>()
            .read(&hashes, slot, s, function)
            .store_i64(entry_hash, function);
        hashes.clear(function);

        // Virgin slot (`hash == 0`) -> linear probe cannot have stepped past
        // this slot -> guaranteed miss (`undefined`).
        entry_hash.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(return_undefined, function);

        // Hash mismatch (collision or different key's tombstone) -> fall back.
        entry_hash.load(function);
        id.load(function);
        function.instruction(&Instruction::I64Ne);
        self.emit_branch_if_to_target(fallback, function);
        s.release_i64_local(entry_hash, function);
        s.release_i64_local(id, function);

        let references = s.reserve_non_null_gc_local(function).initialize(
            s.field(WeakMapTableSchema::REFERENCES)
                .read(&table, s, function)
                .reference(),
            function,
        );
        let ref_index = s.reserve_i32_local(function);
        slot.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Shl);
        ref_index.store(function);

        // Exact key identity check: tombstones store `null` at `slot << 1`,
        // which never equals the non-null `key.reference`.
        let _ = s
            .array_type::<CompactReferenceArray>()
            .read(&references, ref_index, s, function);
        key.reference().load(function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(fallback, function);

        // Guaranteed hit. Zero allocations or calls occurred since `ref.eq`,
        // and `key` remains rooted in its `ValueLocals`.
        let tags = s
            .field(WeakMapTableSchema::TAGS)
            .read(&table, s, function)
            .reference();
        s.array_type::<CompactTagArray>()
            .read_top(tags, slot, function)
            .store(output.tag(), function);
        let scalars = s
            .field(WeakMapTableSchema::SCALARS)
            .read(&table, s, function)
            .reference();
        s.array_type::<CompactScalarArray>()
            .read_top(scalars, slot, function)
            .store_i64(output.scalar(), function);
        ref_index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Or);
        ref_index.store(function);
        s.array_type::<CompactReferenceArray>()
            .read(&references, ref_index, s, function)
            .store_eq(output.reference(), function);
        s.release_i32_local(ref_index, function);
        references.clear(function);
        s.release_i32_local(slot, function);
        table.clear(function);
        handled.set_constant(1, function);
        self.emit_branch_to_target(fallback, function);

        // `return_undefined`: guaranteed miss -> `undefined`, `handled = 1`.
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        output.set_undefined(function);
        handled.set_constant(1, function);

        // End of `fallback` block.
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_pre_evaluated_fallback_call(
            callee,
            receiver,
            &[&key],
            continuation,
            output,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        s.release_i32_local(handled, function);
        key.clear(function);
        Ok(true)
    }

    /// 1-probe inline fast path for `WeakMap.prototype.set(key, value)`.
    ///
    /// Handles both in-place overwrites of an existing key at its home slot and
    /// virgin-slot insertions within the table's 3/4 load limit without
    /// allocation, host registration (the existing table is already registered
    /// with the copying collector), or Realm observation. Any unminted
    /// `KEY_HASH_ID`, null table, primitive/Symbol/exotic key, collision,
    /// tombstone, or load-limit rehash falls back to R with the exact
    /// once-evaluated operands.
    pub(super) fn try_emit_weak_map_set_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let [key_expr, val_expr] = arguments else {
            return Ok(false);
        };
        if [key_expr, val_expr].iter().any(|argument| {
            matches!(
                argument.expr,
                ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
            )
        }) {
            return Ok(false);
        }
        let s = self.runtime_schema();
        let key = s.reserve_value_local(function);
        let val = s.reserve_value_local(function);
        self.compile_expr_to_value(key_expr, &key, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.compile_expr_to_value(val_expr, &val, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        let handled = s.reserve_i32_local(function);
        handled.set_constant(0, function);

        let fallback = self.open_frame(ControlFrameKind::Block, function);
        self.emit_standard_builtin_or_branch(
            callee,
            StandardBuiltinId::WeakMapPrototypeSet,
            fallback,
            function,
        );
        let wm = s.reserve_non_null_gc_local(function).initialize(
            receiver.sealed_reference_or_branch::<WeakMapObject>(
                s,
                self.local_branch_label(fallback),
                function,
            ),
            function,
        );

        // Extract OrdinaryObject header for OrdinaryObject / ArrayObject /
        // FunctionObject; primitives, Symbols, Proxies, and other exotic
        // objects branch to `fallback` (R), preserving exact TypeErrors and
        // unregistered-Symbol validation.
        let header = self.emit_weak_key_header_or_branch(&key, fallback, function);
        let id = s.reserve_i64_local(function);
        s.field(OrdinaryObjectSchema::KEY_HASH_ID)
            .read(&header, s, function)
            .store_i64(id, function);
        header.clear(function);

        // Unminted `KEY_HASH_ID` -> fall back to R to mint it.
        id.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(fallback, function);

        // Null `TABLE` -> fall back to R to allocate and register the table.
        let table = s.reserve_non_null_gc_local(function).initialize(
            s.field(WeakMapObjectSchema::TABLE)
                .read(&wm, s, function)
                .reference()
                .non_null_or_branch(self.local_branch_label(fallback), function),
            function,
        );
        wm.clear(function);

        let hashes = s.reserve_non_null_gc_local(function).initialize(
            s.field(WeakMapTableSchema::HASHES)
                .read(&table, s, function)
                .reference(),
            function,
        );
        let capacity = s.reserve_i32_local(function);
        s.array_type::<NumberBitsI64Array>()
            .length(&hashes, s, function);
        capacity.store(function);
        let slot = s.reserve_i32_local(function);
        Self::emit_weak_map_slot(id, capacity, slot, function);

        let entry_hash = s.reserve_i64_local(function);
        s.array_type::<NumberBitsI64Array>()
            .read(&hashes, slot, s, function)
            .store_i64(entry_hash, function);

        let references = s.reserve_non_null_gc_local(function).initialize(
            s.field(WeakMapTableSchema::REFERENCES)
                .read(&table, s, function)
                .reference(),
            function,
        );
        let ref_index = s.reserve_i32_local(function);
        slot.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Shl);
        ref_index.store(function);

        // Two admitted cases share the value write + return tail:
        // 1. Virgin slot (`entry_hash == 0`): verify `USED < cap - (cap >> 2)`
        //    (matching `table.rs`), increment `USED`, store `hashes[slot] = id`
        //    and `references[slot << 1] = key.reference`.
        // 2. Existing key (`entry_hash == id && ref.eq(references[slot << 1], key)`):
        //    overwrite value in place without mutating `USED`, `hashes`, or key.
        // Collisions, tombstones, and load-limit growth branch to `fallback`.
        entry_hash.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let used = s.reserve_i32_local(function);
        s.field(WeakMapTableSchema::USED)
            .read(&table, s, function)
            .store(used, function);
        used.load(function);
        capacity.load(function);
        capacity.load(function);
        function.instruction(&Instruction::I32Const(2));
        function.instruction(&Instruction::I32ShrU);
        function.instruction(&Instruction::I32Sub);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(fallback, function);
        used.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        used.store(function);
        s.field(WeakMapTableSchema::USED)
            .write(&table, GcOperand::i32_local(used), s, function);
        s.release_i32_local(used, function);
        s.array_type::<NumberBitsI64Array>().write(
            &hashes,
            slot,
            GcOperand::i64_local(id),
            s,
            function,
        );
        s.array_type::<CompactReferenceArray>().write(
            &references,
            ref_index,
            GcOperand::eq_reference(key.reference()),
            s,
            function,
        );
        function.instruction(&Instruction::Else);
        entry_hash.load(function);
        id.load(function);
        function.instruction(&Instruction::I64Ne);
        self.emit_branch_if_to_target(fallback, function);
        let _ = s
            .array_type::<CompactReferenceArray>()
            .read(&references, ref_index, s, function);
        key.reference().load(function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(fallback, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        // Shared value writeback. `array.set` on `CompactReferenceArray` emits
        // Wasmtime's copying-GC write barrier automatically.
        let tags = s
            .field(WeakMapTableSchema::TAGS)
            .read(&table, s, function)
            .reference();
        s.array_type::<CompactTagArray>().write_top(
            tags,
            slot,
            GcOperand::i32_local(val.tag()),
            function,
        );
        let scalars = s
            .field(WeakMapTableSchema::SCALARS)
            .read(&table, s, function)
            .reference();
        s.array_type::<CompactScalarArray>().write_top(
            scalars,
            slot,
            GcOperand::i64_local(val.scalar()),
            function,
        );
        ref_index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Or);
        ref_index.store(function);
        s.array_type::<CompactReferenceArray>().write(
            &references,
            ref_index,
            GcOperand::eq_reference(val.reference()),
            s,
            function,
        );
        output.copy_from(receiver, function);
        handled.set_constant(1, function);

        s.release_i32_local(ref_index, function);
        references.clear(function);
        s.release_i64_local(entry_hash, function);
        s.release_i32_local(slot, function);
        s.release_i32_local(capacity, function);
        hashes.clear(function);
        table.clear(function);
        s.release_i64_local(id, function);

        // End of `fallback` block.
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_pre_evaluated_fallback_call(
            callee,
            receiver,
            &[&key, &val],
            continuation,
            output,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        s.release_i32_local(handled, function);
        val.clear(function);
        key.clear(function);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use scriptw_front::{parse, ParseOptions};
    use scriptw_ir::{lower, lower_with_host_surface_policy, HostSurfacePolicy, StandardBuiltinId};
    use wasmparser::{Operator, Parser, Payload, Validator, WasmFeatures};

    use super::WEAK_MAP_HASH_MULTIPLIER;
    use crate::gc_types::GcI32Constant;
    use crate::{emit, WasmArtifact};

    fn emit_script(source: &str) -> WasmArtifact {
        let parsed = parse(source, ParseOptions::script()).expect("script parses");
        let artifact = emit(&lower_with_host_surface_policy(
            &parsed,
            HostSurfacePolicy::Test262,
        ))
        .expect("script emits");
        validate(&artifact);
        artifact
    }

    fn validate(artifact: &WasmArtifact) {
        let features = WasmFeatures::default()
            | WasmFeatures::THREADS
            | WasmFeatures::FUNCTION_REFERENCES
            | WasmFeatures::GC
            | WasmFeatures::EXCEPTIONS;
        Validator::new_with_features(features)
            .validate_all(&artifact.bytes)
            .expect("emitted module validates");
    }

    fn imported_function_count(bytes: &[u8]) -> u32 {
        let mut count = 0u32;
        for payload in Parser::new(0).parse_all(bytes) {
            let Payload::ImportSection(reader) = payload.expect("module parses") else {
                continue;
            };
            for import in reader.into_imports() {
                let import = import.expect("import parses");
                if matches!(
                    import.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                ) {
                    count += 1;
                }
            }
        }
        count
    }

    fn function_names(bytes: &[u8]) -> BTreeMap<u32, String> {
        let mut names = BTreeMap::new();
        for payload in Parser::new(0).parse_all(bytes) {
            let Payload::CustomSection(section) = payload.expect("module parses") else {
                continue;
            };
            let wasmparser::KnownCustom::Name(subsections) = section.as_known() else {
                continue;
            };
            for subsection in subsections {
                if let wasmparser::Name::Function(map) = subsection.expect("name subsection parses")
                {
                    for naming in map {
                        let naming = naming.expect("function naming parses");
                        names.insert(naming.index, naming.name.to_string());
                    }
                }
            }
        }
        names
    }

    fn body_operators<'a>(bytes: &'a [u8], prefix: &str) -> Vec<Vec<Operator<'a>>> {
        let imports = imported_function_count(bytes);
        let names = function_names(bytes);
        let mut index = imports;
        let mut bodies = Vec::new();
        for payload in Parser::new(0).parse_all(bytes) {
            if let Payload::CodeSectionEntry(body) = payload.expect("module parses") {
                if names
                    .get(&index)
                    .is_some_and(|name| name == prefix || name.starts_with(prefix))
                {
                    let ops = body
                        .get_operators_reader()
                        .expect("operators parse")
                        .into_iter()
                        .map(|op| op.expect("operator parses"))
                        .collect();
                    bodies.push(ops);
                }
                index += 1;
            }
        }
        bodies
    }

    fn has_weak_map_probe(ops: &[Operator<'_>], builtin: StandardBuiltinId) -> bool {
        let expected_builtin = builtin.encode();
        let guards_builtin = ops
            .iter()
            .any(|op| matches!(op, Operator::I32Const { value } if *value == expected_builtin));
        let hashes_slot = ops.windows(5).any(|window| {
            matches!(
                window,
                [
                    Operator::I64Const { value },
                    Operator::I64Mul,
                    Operator::I64Const { value: 32 },
                    Operator::I64ShrU,
                    Operator::I32WrapI64
                ] if *value == WEAK_MAP_HASH_MULTIPLIER
            )
        });
        guards_builtin && hashes_slot
    }

    #[test]
    fn weak_map_get_and_set_fast_paths_emit_and_validate() {
        for (source, builtin, expected) in [
            (
                "const wm = new WeakMap(); wm.get(globalThis.k);",
                StandardBuiltinId::WeakMapPrototypeGet,
                true,
            ),
            (
                "const wm = new WeakMap(); wm.set(globalThis.k, globalThis.v);",
                StandardBuiltinId::WeakMapPrototypeSet,
                true,
            ),
            (
                "const wm = new WeakMap(); for (let i = 0; i < 4; i++) { wm.set(globalThis.k, i); wm.get(globalThis.k); }",
                StandardBuiltinId::WeakMapPrototypeGet,
                true,
            ),
            (
                "const wm = new WeakMap(); for (let i = 0; i < 4; i++) { wm.set(globalThis.k, i); wm.get(globalThis.k); }",
                StandardBuiltinId::WeakMapPrototypeSet,
                true,
            ),
            (
                "const wm = new WeakMap(); wm.get();",
                StandardBuiltinId::WeakMapPrototypeGet,
                false,
            ),
            (
                "const wm = new WeakMap(); wm.get(globalThis.k, 1);",
                StandardBuiltinId::WeakMapPrototypeGet,
                false,
            ),
            (
                "const wm = new WeakMap(); wm.set(globalThis.k);",
                StandardBuiltinId::WeakMapPrototypeSet,
                false,
            ),
            (
                "const wm = new WeakMap(); wm.set(globalThis.k, 1, 2);",
                StandardBuiltinId::WeakMapPrototypeSet,
                false,
            ),
            (
                "const wm = new WeakMap(); wm.get(...globalThis.args);",
                StandardBuiltinId::WeakMapPrototypeGet,
                false,
            ),
            (
                "const wm = new WeakMap(); wm.set(...globalThis.args);",
                StandardBuiltinId::WeakMapPrototypeSet,
                false,
            ),
        ] {
            let artifact = emit_script(source);
            let main_bodies = body_operators(&artifact.bytes, "scriptw::main");
            assert_eq!(main_bodies.len(), 1, "{source}");
            assert_eq!(
                has_weak_map_probe(&main_bodies[0], builtin),
                expected,
                "{source}"
            );
        }
    }

    #[test]
    fn weak_map_fast_paths_support_strict_and_module_return_position() {
        let script = emit_script(
            r#"
            function getStrict(wm, k) { "use strict"; return wm.get(k); }
            function setStrict(wm, k, v) { "use strict"; return wm.set(k, v); }
            const wm = new WeakMap();
            setStrict(wm, {}, 1);
            getStrict(wm, {});
            "#,
        );
        let get_bodies = body_operators(&script.bytes, "js::getStrict#");
        assert!(get_bodies.iter().any(|ops| {
            has_weak_map_probe(ops, StandardBuiltinId::WeakMapPrototypeGet)
                && ops
                    .iter()
                    .any(|op| matches!(op, Operator::ReturnCall { .. }))
                && ops
                    .iter()
                    .any(|op| matches!(op, Operator::ReturnCallRef { .. }))
        }));
        let set_bodies = body_operators(&script.bytes, "js::setStrict#");
        assert!(set_bodies.iter().any(|ops| {
            has_weak_map_probe(ops, StandardBuiltinId::WeakMapPrototypeSet)
                && ops
                    .iter()
                    .any(|op| matches!(op, Operator::ReturnCall { .. }))
                && ops
                    .iter()
                    .any(|op| matches!(op, Operator::ReturnCallRef { .. }))
        }));

        let module = emit(&lower(
            &parse(
                r#"
                export function getVal(wm, k) { return wm.get(k); }
                export function setVal(wm, k, v) { return wm.set(k, v); }
                "#,
                ParseOptions::module(),
            )
            .expect("module parses"),
        ))
        .expect("module emits");
        validate(&module);
        assert!(body_operators(&module.bytes, "js::getVal#")
            .iter()
            .any(|ops| {
                has_weak_map_probe(ops, StandardBuiltinId::WeakMapPrototypeGet)
                    && ops
                        .iter()
                        .any(|op| matches!(op, Operator::ReturnCall { .. }))
            }));
        assert!(body_operators(&module.bytes, "js::setVal#")
            .iter()
            .any(|ops| {
                has_weak_map_probe(ops, StandardBuiltinId::WeakMapPrototypeSet)
                    && ops
                        .iter()
                        .any(|op| matches!(op, Operator::ReturnCall { .. }))
            }));
    }
}
