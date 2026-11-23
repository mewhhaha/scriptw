//! Inspect encoded source bodies: closed loops snapshot unwritten captured
//! cells at entry, and root-relative cell tables are computed once.
use super::*;
use crate::gc_types::{Environment, GcNullability, GlobalLedger, RuntimeModuleTypes};

/// Applies `inspect` to every encoded body whose Wasm name is `js::{name}#`.
fn each_body<R>(
    artifact: &WasmArtifact,
    name: &str,
    mut inspect: impl FnMut(&[Operator<'_>]) -> R,
) -> Vec<R> {
    expect_valid_module(artifact, 1);
    let names = function_names(artifact)
        .into_iter()
        .filter(|(_, function)| function.starts_with(&format!("js::{name}#")))
        .collect::<BTreeMap<_, _>>();
    assert!(!names.is_empty(), "named source body `{name}` is emitted");
    let mut index = imported_function_count(artifact);
    let mut result = Vec::new();
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.expect("module parses") {
            if names.contains_key(&index) {
                result.push(inspect(&crate::tests::emitted_operators(&body)));
            }
            index += 1;
        }
    }
    assert_eq!(result.len(), names.len(), "inspect every physical variant");
    result
}

/// The operators strictly inside each `loop ... end`.
fn loops<'a>(operators: &'a [Operator<'a>]) -> Vec<&'a [Operator<'a>]> {
    let mut frames = Vec::new();
    let mut regions = Vec::new();
    for (index, operator) in operators.iter().enumerate() {
        match operator {
            Operator::Loop { .. } => frames.push(Some(index)),
            Operator::Block { .. } | Operator::If { .. } | Operator::TryTable { .. } => {
                frames.push(None)
            }
            Operator::End => {
                if let Some(Some(start)) = frames.pop() {
                    regions.push(&operators[start + 1..index]);
                }
            }
            _ => {}
        }
    }
    regions
}

/// A loop with Number arithmetic that reads no heap field and calls nothing:
/// no environment walk, cell load, helper or source call per iteration.
fn has_closed_number_loop(operators: &[Operator<'_>]) -> bool {
    loops(operators).into_iter().any(|region| {
        region
            .iter()
            .any(|operator| matches!(operator, Operator::F64Add))
            && region.iter().all(|operator| {
                !matches!(
                    operator,
                    Operator::Call { .. }
                        | Operator::CallRef { .. }
                        | Operator::CallIndirect { .. }
                        | Operator::ReturnCall { .. }
                        | Operator::ReturnCallRef { .. }
                        | Operator::StructGet { .. }
                        | Operator::StructGetU { .. }
                        | Operator::StructGetS { .. }
                        | Operator::StructSet { .. }
                        | Operator::ArrayGet { .. }
                        | Operator::ArraySet { .. }
                )
            })
    })
}

#[test]
fn closed_loops_snapshot_unwritten_captured_cells_and_identity_tests() {
    let artifact = emit_script(
        r#"
function make(input) {
  var p = input + 1, q = input + 2, e = Math, b = e;
  function closedInner() {
    var sum = 0;
    for (var i = 0; i < 64; i++) { sum += p + q; if (b === e) sum++; }
    return sum;
  }
  function deepInner() {
    return function closedDeep() {
      var sum = 0;
      for (var i = 0; i < 64; i++) { sum += p; if (b !== e) sum += q; }
      return sum;
    };
  }
  return closedInner() + deepInner()();
}
make(1);
"#,
    )
    .expect("captured loops emit");
    for name in ["closedInner", "closedDeep"] {
        each_body(&artifact, name, |body| {
            assert!(
                has_closed_number_loop(body),
                "{name}: the captured reads leave the loop"
            );
            // The original loop remains the entry-miss path.
            assert!(loops(body).len() >= 2, "{name}: original loop retained");
        });
    }
}

#[test]
fn captured_writes_and_calls_keep_the_original_loop_only() {
    let artifact = emit_script(
        r#"
function make(input) {
  var p = input + 1;
  function tick() { p = p + 1; return 1; }
  function writes() {
    var sum = 0;
    for (var i = 0; i < 64; i++) { sum += p; p = sum; }
    return sum;
  }
  function calls() {
    var sum = 0;
    for (var i = 0; i < 64; i++) { sum += p + tick(); }
    return sum;
  }
  return writes() + calls();
}
make(1);
"#,
    )
    .expect("observable captured loops emit");
    for name in ["writes", "calls"] {
        each_body(&artifact, name, |body| {
            assert!(
                !has_closed_number_loop(body),
                "{name}: a writable captured cell stays on its owner"
            );
        });
    }
}

/// Environment reads outside and inside cell-table fill arms: an `if`
/// entered right after `local.get t; ref.is_null`, keyed by `t`.
#[derive(Debug, Default, PartialEq)]
struct EnvironmentReads {
    unguarded: usize,
    fills: BTreeMap<u32, usize>,
}

#[test]
fn root_relative_cell_tables_are_walked_once_per_invocation() {
    let environment_reads = |source: &str| {
        let artifact = emit_script(source).expect("captured reads emit");
        let registered =
            RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Program(
                artifact.runtime().expect("heap program links a runtime"),
            ))
            .finalize_globals(GlobalLedger::new(), false, 0);
        let wasm_encoder::HeapType::Concrete(environment) = registered
            .globals()
            .schema()
            .reference_type::<Environment>(GcNullability::NonNullable)
            .heap_type
        else {
            panic!("Environment has a concrete registered type");
        };
        each_body(&artifact, "readCaptured", |body| {
            let mut reads = EnvironmentReads::default();
            let mut frames: Vec<Option<u32>> = Vec::new();
            for (index, operator) in body.iter().enumerate() {
                match operator {
                    Operator::If { .. } => {
                        let fill = match index.checked_sub(2).map(|start| &body[start..index]) {
                            Some([Operator::LocalGet { local_index }, Operator::RefIsNull]) => {
                                Some(*local_index)
                            }
                            _ => None,
                        };
                        if let Some(local) = fill {
                            *reads.fills.entry(local).or_default() += 1;
                        }
                        frames.push(fill);
                    }
                    Operator::Block { .. } | Operator::Loop { .. } | Operator::TryTable { .. } => {
                        frames.push(None)
                    }
                    Operator::End => {
                        frames.pop();
                    }
                    Operator::StructGet {
                        struct_type_index, ..
                    } if *struct_type_index == environment
                        && !frames.iter().any(Option::is_some) =>
                    {
                        reads.unguarded += 1;
                    }
                    _ => {}
                }
            }
            reads
        })
    };
    let program = |reads: &str| {
        format!(
            "function outer(a) {{ function mid() {{ var m = 1; function readCaptured() {{ {reads} }} \
             return function () {{ return m; }}, readCaptured; }} return mid; }} outer(1)()();"
        )
    };
    let once = environment_reads(&program("return a;"));
    let thrice = environment_reads(&program("var x = a; var y = a; var z = a; return z;"));
    for (once, thrice) in once.iter().zip(&thrice) {
        // Every further read of the same root-relative cell walks parent
        // links only in a fill arm of the one table, which runs at most once
        // per invocation; no unguarded walk is added per read.
        assert_eq!(once.unguarded, thrice.unguarded, "{once:?} / {thrice:?}");
        // Exactly the one root table gains a fill arm per added read.
        let grown = thrice
            .fills
            .iter()
            .filter(|(local, count)| once.fills.get(local).copied().unwrap_or(0) != **count)
            .collect::<Vec<_>>();
        assert_eq!(grown.len(), 1, "{once:?} / {thrice:?}");
        let (local, count) = grown[0];
        assert_eq!(*count, once.fills.get(local).copied().unwrap_or(0) + 2);
        assert!(once.fills.contains_key(local), "{once:?} / {thrice:?}");
    }
}
