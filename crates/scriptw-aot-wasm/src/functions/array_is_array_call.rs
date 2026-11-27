//! Inline fast path for `Array.isArray(x)`.
//!
//! Static target knowledge or `*.isArray` syntax only nominates the call site;
//! the runtime guard proves the actual once-read callee is `%Array.isArray%`
//! of any Realm. Non-Proxy `IsArray` never throws, allocates, or observes
//! Realm state, so sealed `ArrayObject` -> `true` and non-`ProxyObject` ->
//! `false` are cross-realm sound. `ProxyObject` (live or revoked) falls back
//! to R's canonical builtin with the already-evaluated argument.
use super::*;
use crate::gc_types::{ArrayObject, GcNullability, ProxyObject, ScalarValue, ValueLocals};

impl FunctionBuilder<'_> {
    /// Emits an inline `Array.isArray(arg)` check after proving the actual
    /// callee is `%Array.isArray%`. Admits both continuing and tail-return
    /// calls.
    pub(super) fn try_emit_array_is_array_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let [arg_expr] = arguments else {
            return Ok(false);
        };
        if matches!(
            arg_expr.expr,
            ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
        ) {
            return Ok(false);
        }
        let schema = self.runtime_schema();
        let arg = schema.reserve_value_local(function);
        self.compile_expr_to_value(arg_expr, &arg, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        let handled = schema.reserve_i32_local(function);
        handled.set_constant(0, function);

        let fallback = self.open_frame(ControlFrameKind::Block, function);
        self.emit_standard_builtin_or_branch(
            callee,
            StandardBuiltinId::ArrayIsArray,
            fallback,
            function,
        );
        // Matches `objects.rs::emit_is_array_i32`: `ArrayObject` and
        // `ProxyObject` are both sealed GC structs, and non-object values
        // hold null / StringValue / SymbolValue / BigIntValue (never either).
        arg.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        output.set_scalar(ScalarValue::Boolean(true), function);
        handled.set_constant(1, function);
        function.instruction(&Instruction::Else);
        arg.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        output.set_scalar(ScalarValue::Boolean(false), function);
        handled.set_constant(1, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_pre_evaluated_fallback_call(
            callee,
            receiver,
            &[&arg],
            continuation,
            output,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        schema.release_i32_local(handled, function);
        arg.clear(function);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use scriptw_front::{parse, ParseOptions};
    use scriptw_ir::{lower, lower_with_host_surface_policy, HostSurfacePolicy, StandardBuiltinId};
    use wasmparser::{Operator, Parser, Payload, Validator, WasmFeatures};

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

    fn has_inline_array_is_array(ops: &[Operator<'_>]) -> bool {
        let expected_builtin = StandardBuiltinId::ArrayIsArray.encode();
        let guards_builtin = ops
            .iter()
            .any(|op| matches!(op, Operator::I32Const { value } if *value == expected_builtin));
        let tests_non_proxy = ops.windows(3).any(|window| {
            matches!(
                window,
                [
                    Operator::RefTestNonNull { .. },
                    Operator::I32Eqz,
                    Operator::If { .. }
                ]
            )
        });
        guards_builtin && tests_non_proxy
    }

    #[test]
    fn array_is_array_fast_path_emits_for_single_arg_sites_and_validates() {
        for (source, expected) in [
            ("Array.isArray(globalThis.value);", true),
            (
                "const check = Array.isArray; check(globalThis.value);",
                true,
            ),
            (
                "for (let i = 0; i < 4; i++) { Array.isArray(globalThis.value); }",
                true,
            ),
            ("Array.isArray();", false),
            ("Array.isArray(globalThis.a, globalThis.b);", false),
            ("Array.isArray(...globalThis.items);", false),
        ] {
            let artifact = emit_script(source);
            let main_bodies = body_operators(&artifact.bytes, "scriptw::main");
            assert_eq!(main_bodies.len(), 1, "{source}");
            assert_eq!(
                has_inline_array_is_array(&main_bodies[0]),
                expected,
                "{source}"
            );
        }
    }

    #[test]
    fn array_is_array_fast_path_supports_strict_and_module_return_position() {
        let script = emit_script(
            r#"
            function strictCheck(x) { "use strict"; return Array.isArray(x); }
            function sloppyCheck(x) { return Array.isArray(x); }
            strictCheck(globalThis.x);
            sloppyCheck(globalThis.x);
            "#,
        );
        let strict_bodies = body_operators(&script.bytes, "js::strictCheck#");
        assert!(!strict_bodies.is_empty());
        assert!(strict_bodies.iter().any(|ops| {
            has_inline_array_is_array(ops)
                && ops
                    .iter()
                    .any(|op| matches!(op, Operator::ReturnCall { .. }))
                && ops
                    .iter()
                    .any(|op| matches!(op, Operator::ReturnCallRef { .. }))
        }));
        let sloppy_bodies = body_operators(&script.bytes, "js::sloppyCheck#");
        assert!(sloppy_bodies
            .iter()
            .any(|ops| has_inline_array_is_array(ops)));

        let module = emit(&lower(
            &parse(
                "export function isArr(x) { return Array.isArray(x); }",
                ParseOptions::module(),
            )
            .expect("module parses"),
        ))
        .expect("module emits");
        validate(&module);
        let module_bodies = body_operators(&module.bytes, "js::isArr#");
        assert!(module_bodies.iter().any(|ops| {
            has_inline_array_is_array(ops)
                && ops
                    .iter()
                    .any(|op| matches!(op, Operator::ReturnCall { .. }))
        }));
    }
}
