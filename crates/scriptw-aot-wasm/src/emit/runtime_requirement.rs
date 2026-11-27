use crate::data::StringPool;
use scriptw_ir::{
    ArithmeticBinaryOp, BlockIr, ExprIr, GlobalBindingPlan, GlobalDeclarationSetIr,
    GlobalPropertyInitializerIr, HostBuiltinId, ScriptIr, StatementIr, TypedExpr, ValueKind,
};
use std::collections::BTreeSet;

/// Elide Realm bootstrap only when the lowered script cannot observe it.
/// This is a conservative IR proof: every unrecognized operation retains the
/// ordinary runtime, including declarations, observable coercions, calls and
/// abrupt exits. Admitted expressions use the standard Wasm expression emitter.
#[derive(Debug)]
pub(crate) struct StandaloneAdmission<'script> {
    script: &'script ScriptIr,
    pool: StringPool,
}

impl<'script> StandaloneAdmission<'script> {
    pub(crate) fn new(script: &'script ScriptIr) -> Option<Self> {
        if !script_has_standalone_environment(script) || !script.host_builtins.is_empty() {
            return None;
        }
        let mut literals = BTreeSet::new();
        if !block_is_runtime_free(&script.body, &mut literals) {
            return None;
        }
        Some(Self {
            script,
            pool: StringPool::from_standalone_literals(literals),
        })
    }

    pub(crate) fn script(&self) -> &'script ScriptIr {
        self.script
    }

    pub(crate) fn pool(&self) -> &StringPool {
        &self.pool
    }
}

fn script_has_standalone_environment(script: &ScriptIr) -> bool {
    script.eval_environment.is_none()
        && script.foreign_modules.is_empty()
        && script.module_prelude.is_none()
        && script.prepared_scripts.is_empty()
        && script.runtime_declarations == Default::default()
        && script.script_entry_after_prelude.is_none()
        && script.prepared_dynamic_functions.is_empty()
        && script.functions.is_empty()
        && script.owned_env_bindings.is_empty()
        && script.global_bindings.lexical_bindings().is_empty()
        && script.top_level_this_uses == 0
}

/// A complete straight-line script whose only observable operations are calls
/// to the initial host print binding. Each span has already passed the host's
/// signed i32 bounds check and uses the linked print encoder's UTF-8 policy.
#[derive(Debug)]
pub(super) struct StandalonePrint {
    pub(super) bytes: Vec<u8>,
    pub(super) lines: Vec<(i32, i32)>,
}

impl StandalonePrint {
    pub(super) fn admit(script: &ScriptIr) -> Option<Self> {
        if script.host_builtins.as_slice() != [HostBuiltinId::Print]
            || !script_has_standalone_environment(script)
        {
            return None;
        }
        let print = HostBuiltinId::Print.as_str();
        let binding = script.global_bindings.get(print)?;
        if binding.initializer != GlobalPropertyInitializerIr::HostFunction(HostBuiltinId::Print)
            || script.body.lexical_environment.is_some()
            || script
                .global_bindings
                .iter()
                .any(|binding| binding.declarations != GlobalDeclarationSetIr::None)
        {
            return None;
        }

        let mut bytes = Vec::new();
        let mut lines = Vec::new();
        for statement in &script.body.statements {
            let argument = match statement {
                StatementIr::Empty => continue,
                StatementIr::Expression(TypedExpr {
                    expr: ExprIr::String(_),
                    ..
                }) if lines.is_empty() => continue,
                StatementIr::Expression(TypedExpr {
                    expr:
                        ExprIr::CallIndirect {
                            direct_eval: None,
                            this_arg: None,
                            static_regexp_compilation: None,
                            callee,
                            args,
                        },
                    ..
                }) if global_read_name(callee) == Some(print) => {
                    let [argument] = args.as_slice() else {
                        return None;
                    };
                    argument
                }
                _ => return None,
            };
            let line = constant_print_text(argument, &script.global_bindings)?;
            let start = i32::try_from(bytes.len()).ok()?;
            let len = i32::try_from(line.len()).ok()?;
            start.checked_add(len)?;
            bytes.extend_from_slice(line.as_bytes());
            lines.push((start, len));
        }
        (!lines.is_empty()).then_some(Self { bytes, lines })
    }
}

fn global_read_name(expression: &TypedExpr) -> Option<&str> {
    match &expression.expr {
        ExprIr::GlobalPropertyRead { name } | ExprIr::GlobalIdentifierRead { name } => Some(name),
        _ => None,
    }
}

fn constant_print_text(expression: &TypedExpr, globals: &GlobalBindingPlan) -> Option<String> {
    Some(match &expression.expr {
        ExprIr::String(text) => String::from_utf16_lossy(&scriptw_ir::decode_js_string_utf16(text)),
        ExprIr::Undefined => "undefined".into(),
        ExprIr::Null => "null".into(),
        ExprIr::Boolean(value) => value.to_string(),
        _ if global_read_name(expression)
            .and_then(|name| globals.get(name))
            .is_some_and(|binding| {
                binding.initializer == GlobalPropertyInitializerIr::Undefined
            }) =>
        {
            "undefined".into()
        }
        ExprIr::Void { expr } if expression_is_runtime_free(expr, &mut BTreeSet::new()) => {
            "undefined".into()
        }
        _ => {
            let number = constant_number(expression, globals)?;
            if number.is_nan() {
                "NaN".into()
            } else if number == f64::INFINITY {
                "Infinity".into()
            } else if number == f64::NEG_INFINITY {
                "-Infinity".into()
            } else if number.fract() == 0.0 && number.abs() < 9_007_199_254_740_992.0 {
                // Every safe integer has its exact decimal spelling here.
                // The conversion also gives both signed zeros the spelling 0.
                (number as i64).to_string()
            } else {
                return None;
            }
        }
    })
}

fn constant_number(expression: &TypedExpr, globals: &GlobalBindingPlan) -> Option<f64> {
    if expression.kind != ValueKind::Number {
        return None;
    }
    Some(match &expression.expr {
        ExprIr::Number(bits) => f64::from_bits(*bits),
        ExprIr::UnaryPlus { expr } => constant_number(expr, globals)?,
        ExprIr::UnaryMinusNumeric { expr } => -constant_number(expr, globals)?,
        ExprIr::BinaryNumber { op, lhs, rhs } | ExprIr::CoerciveBinaryNumber { op, lhs, rhs } => {
            let lhs = constant_number(lhs, globals)?;
            let rhs = constant_number(rhs, globals)?;
            match op {
                ArithmeticBinaryOp::Add => lhs + rhs,
                ArithmeticBinaryOp::Sub => lhs - rhs,
                ArithmeticBinaryOp::Mul => lhs * rhs,
                ArithmeticBinaryOp::Div => lhs / rhs,
                ArithmeticBinaryOp::Mod => lhs % rhs,
                ArithmeticBinaryOp::Exp => return None,
            }
        }
        _ => match globals.get(global_read_name(expression)?)?.initializer {
            GlobalPropertyInitializerIr::NaN => f64::NAN,
            GlobalPropertyInitializerIr::Infinity => f64::INFINITY,
            _ => return None,
        },
    })
}

fn block_is_runtime_free<'s>(block: &'s BlockIr, literals: &mut BTreeSet<&'s str>) -> bool {
    block.lexical_environment.is_none()
        && block
            .statements
            .iter()
            .all(|statement| statement_is_runtime_free(statement, literals))
}

fn statement_is_runtime_free<'s>(
    statement: &'s StatementIr,
    literals: &mut BTreeSet<&'s str>,
) -> bool {
    match statement {
        StatementIr::Empty => true,
        StatementIr::Expression(expression) => expression_is_runtime_free(expression, literals),
        StatementIr::Block(block) => block_is_runtime_free(block, literals),
        StatementIr::LexicalBlock(statements) => statements
            .iter()
            .all(|statement| statement_is_runtime_free(statement, literals)),
        _ => false,
    }
}

fn expression_is_runtime_free<'s>(
    expression: &'s TypedExpr,
    literals: &mut BTreeSet<&'s str>,
) -> bool {
    match &expression.expr {
        ExprIr::Undefined | ExprIr::Null | ExprIr::Boolean(_) | ExprIr::Number(_) => true,
        ExprIr::String(value) => {
            literals.insert(value.as_str());
            true
        }
        ExprIr::Void { expr } | ExprIr::DeleteValue { expr } | ExprIr::LogicalNot { expr } => {
            expression_is_runtime_free(expr, literals)
        }
        ExprIr::UnaryPlus { expr }
        | ExprIr::UnaryMinusNumeric { expr }
        | ExprIr::UnaryBitwiseNumeric { expr, .. } => number_is_runtime_free(expr, literals),
        ExprIr::BinaryNumber { op, lhs, rhs } | ExprIr::CoerciveBinaryNumber { op, lhs, rhs } => {
            // Exponentiation calls a separately planned host function. All
            // remaining Number operations emit scalar Wasm instructions.
            let scalar_operation = match op {
                ArithmeticBinaryOp::Add
                | ArithmeticBinaryOp::Sub
                | ArithmeticBinaryOp::Mul
                | ArithmeticBinaryOp::Div
                | ArithmeticBinaryOp::Mod => true,
                ArithmeticBinaryOp::Exp => false,
            };
            scalar_operation
                && expression.kind == ValueKind::Number
                && number_is_runtime_free(lhs, literals)
                && number_is_runtime_free(rhs, literals)
        }
        ExprIr::CompareNumber { lhs, rhs, .. } => {
            number_is_runtime_free(lhs, literals) && number_is_runtime_free(rhs, literals)
        }
        ExprIr::Comma { lhs, rhs } | ExprIr::LogicalShortCircuit { lhs, rhs, .. } => {
            expression_is_runtime_free(lhs, literals) && expression_is_runtime_free(rhs, literals)
        }
        ExprIr::Conditional {
            condition,
            then_expr,
            else_expr,
        } => {
            expression_is_runtime_free(condition, literals)
                && expression_is_runtime_free(then_expr, literals)
                && expression_is_runtime_free(else_expr, literals)
        }
        _ => false,
    }
}

fn number_is_runtime_free<'s>(expression: &'s TypedExpr, literals: &mut BTreeSet<&'s str>) -> bool {
    expression.kind == ValueKind::Number && expression_is_runtime_free(expression, literals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};
    use wasmparser::{DataKind, Parser, Payload, TypeRef, Validator, WasmFeatures};

    #[test]
    fn standalone_print_module_has_the_host_abi_and_exact_utf8_image() {
        let source = r#"
            "use strict";
            print("scriptw-startup-ok");
            print("\0\ud800x\udfff\u{1f600}");
            print((1 + 2) * 3);
            print(-0);
            print(0 / 0);
            print(Infinity);
            print(-1 / 0);
            print(false);
            print(null);
            print(undefined);
        "#;
        let program = scriptw_ir::lower(&parse(source, ParseOptions::script()).unwrap());
        let artifact = crate::emit(&program).unwrap();
        assert!(artifact.runtime().is_none());
        Validator::new_with_features(WasmFeatures::default() | WasmFeatures::GC)
            .validate_all(&artifact.bytes)
            .unwrap();
        let mut imports = 0;
        let mut memory_export = false;
        let mut image = None;
        for payload in Parser::new(0).parse_all(&artifact.bytes) {
            match payload.unwrap() {
                Payload::ImportSection(section) => {
                    for entry in section.into_imports() {
                        let entry = entry.unwrap();
                        assert_eq!(entry.module, crate::HOST_IMPORT_MODULE);
                        assert_eq!(entry.name, crate::HOST_IMPORT_PRINT_LINE_UTF8);
                        assert!(matches!(entry.ty, TypeRef::Func(index)
                            if index == crate::StaticSignature::HostPrint.type_index()));
                        imports += 1;
                    }
                }
                Payload::ExportSection(section) => {
                    for entry in section {
                        let entry = entry.unwrap();
                        memory_export |= entry.name == "memory"
                            && entry.kind == wasmparser::ExternalKind::Memory
                            && entry.index == 0;
                    }
                }
                Payload::DataSection(section) => {
                    assert_eq!(section.count(), 1);
                    let segment = section.into_iter().next().unwrap().unwrap();
                    assert!(matches!(
                        segment.kind,
                        DataKind::Active {
                            memory_index: 0,
                            ..
                        }
                    ));
                    image = Some(segment.data);
                }
                _ => {}
            }
        }
        assert_eq!(imports, 1);
        assert!(memory_export);
        assert_eq!(
            image.unwrap(),
            "scriptw-startup-ok\0�x�😀90NaNInfinity-Infinityfalsenullundefined".as_bytes()
        );
    }

    #[test]
    fn standalone_print_refuses_mutable_callees_and_unproved_conversions() {
        for source in [
            "var print; print('hello');",
            "delete print; print('hello');",
            "globalThis['print'] = 1; print('hello');",
            "with ({ print: 1 }) print('hello');",
            "eval(''); print('hello');",
            "globalThis.print('hello');",
            "print({});",
            "print(1.5);",
            "print(1, 2);",
            "print('hello'); 42;",
        ] {
            let program = scriptw_ir::lower(&parse(source, ParseOptions::script()).unwrap());
            assert!(
                StandalonePrint::admit(program.script.as_ref().unwrap()).is_none(),
                "{source}"
            );
        }
    }
}
