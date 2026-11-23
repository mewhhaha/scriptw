//! Statement chains that test one binding: `instanceof` runs share
//! the walked [[Prototype]] chain, and typeof if-else chains dispatch once.
use super::*;

fn function_operators(artifact: &WasmArtifact, prefix: &str) -> Vec<Vec<Operator<'static>>> {
    let names = function_names(artifact);
    let mut index = imported_function_count(artifact);
    let mut bodies = Vec::new();
    // The artifact outlives the test; leak it so its operators can be kept.
    let bytes: &'static [u8] = Box::leak(artifact.bytes.clone().into_boxed_slice());
    for payload in Parser::new(0).parse_all(bytes) {
        let Payload::CodeSectionEntry(body) = payload.unwrap() else {
            continue;
        };
        if names
            .get(&index)
            .is_some_and(|name| name.starts_with(prefix))
        {
            bodies.push(crate::tests::emitted_operators(&body));
        }
        index += 1;
    }
    bodies
}

fn emit_validated(source: &str) -> WasmArtifact {
    let artifact = emit_script(source).expect("statement chain emits");
    Validator::new_with_features(crate::runtime_artifact::program_validation_features())
        .validate_all(&artifact.bytes)
        .expect("statement chain validates");
    artifact
}

/// A later run site compares the kept V with its own before reading the
/// slots: `local.get; local.get; ref.eq; i32.eqz; if`.
fn reused_walks(operators: &[Operator<'_>]) -> usize {
    operators
        .windows(5)
        .filter(|window| {
            matches!(
                window,
                [
                    Operator::LocalGet { .. },
                    Operator::LocalGet { .. },
                    Operator::RefEq,
                    Operator::I32Eqz,
                    Operator::If { .. }
                ]
            )
        })
        .count()
}

#[test]
fn instanceof_runs_reuse_the_first_sites_walk_only_between_abrupt_sites() {
    for (source, reused) in [
        // Three sites: the first walks, the other two reuse its slots.
        (
            "var M=Map,S=Set,E=Error;function f(o){if(o instanceof M)return 1;if(o instanceof S)return 2;if(o instanceof E){throw o;}return 0;} f;",
            2,
        ),
        // Any other statement between two sites ends the run.
        (
            "var M=Map,S=Set;function f(o){var n=0;if(o instanceof M)return 1;n=1;if(o instanceof S)return 2;return n;} f;",
            0,
        ),
        // A consequent that may complete normally reaches the next site.
        (
            "var M=Map,S=Set;function f(o){var n=0;if(o instanceof M)n++;if(o instanceof S)return 2;return n;} f;",
            0,
        ),
        // Different left operands are not one run.
        (
            "var M=Map,S=Set;function f(o,p){if(o instanceof M)return 1;if(p instanceof S)return 2;return 0;} f;",
            0,
        ),
    ] {
        let artifact = emit_validated(source);
        let bodies = function_operators(&artifact, "js::f#");
        assert_eq!(bodies.len(), 1, "{source}");
        assert_eq!(reused_walks(&bodies[0]), reused, "{source}");
    }
}

/// The chain's one table: a target for each of typeof's eight classes.
fn class_tables(operators: &[Operator<'_>]) -> usize {
    operators
        .iter()
        .filter(|operator| matches!(operator, Operator::BrTable { targets } if targets.len() == 8))
        .count()
}

#[test]
fn typeof_if_chains_on_one_binding_dispatch_from_one_classification() {
    for (source, tables) in [
        (
            "function f(v){if(typeof v==='undefined')return 1;else if(typeof v=='object')return 2;else if('function'===typeof v)return 3;else if(typeof v!=='number')return 4;return 5;} f;",
            1,
        ),
        // Two tests decide faster one by one.
        (
            "function f(v){if(typeof v==='string')return 1;else if(typeof v==='number')return 2;return 3;} f;",
            0,
        ),
        // Another operand ends the chain before it is long enough.
        (
            "function f(v,w){if(typeof v==='string')return 1;else if(typeof w==='number')return 2;else if(typeof v==='bigint')return 3;return 4;} f;",
            0,
        ),
    ] {
        let artifact = emit_validated(source);
        let bodies = function_operators(&artifact, "js::f#");
        assert_eq!(bodies.len(), 1, "{source}");
        assert_eq!(class_tables(&bodies[0]), tables, "{source}");
    }
}

