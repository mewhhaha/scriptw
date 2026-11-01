//! Inspect real encoded source bodies, independently of emitter scratch APIs.
use super::*;
use crate::gc_types::{GcNullability, GlobalLedger, NumberBitsI64Array, RuntimeModuleTypes};

fn number_array_type() -> u32 {
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Standalone)
        .finalize_globals(GlobalLedger::new(), false, 0);
    let wasm_encoder::HeapType::Concrete(index) = registered
        .globals()
        .schema()
        .reference_type::<NumberBitsI64Array>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("Number backing has a concrete registered type");
    };
    index
}

fn dense_number_loop(operators: &[Operator<'_>], number_array: u32) -> bool {
    operators.iter().any(|operator| {
        matches!(operator, Operator::ArrayGet { array_type_index } if *array_type_index == number_array)
    }) && operators.iter().all(|operator| {
        !matches!(
            operator,
            Operator::Call { .. }
                | Operator::ReturnCall { .. }
                | Operator::CallRef { .. }
                | Operator::ReturnCallRef { .. }
                | Operator::CallIndirect { .. }
                | Operator::ReturnCallIndirect { .. }
                | Operator::StructGet { .. }
                | Operator::StructSet { .. }
                | Operator::StructNew { .. }
                | Operator::ArrayNew { .. }
                | Operator::ArrayNewFixed { .. }
                | Operator::ArraySet { .. }
        )
    })
}

#[test]
fn original_sort_checksums_borrow_dense_numbers_outside_the_scalar_loop() {
    let number_array = number_array_type();
    for source in [
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/sort-reverse-128.js"
        ),
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/sort-reverse-512.js"
        ),
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/sort-sorted-512.js"
        ),
        include_str!("../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/to-sorted-512.js"),
    ] {
        with_source_artifact_variants(
            emit_script(source).expect("sort workload emits"),
            "workload",
            |_, operators| {
                let loops = loop_regions(operators);
                let dense = loops
                    .iter()
                    .filter(|region| dense_number_loop(region, number_array))
                    .collect::<Vec<_>>();
                assert_eq!(dense.len(), 2, "dense chunk and tail: {source}");
                let mut reads = dense.iter().map(|region| region.iter().filter(|op| matches!(op, Operator::ArrayGet { array_type_index } if *array_type_index == number_array)).count()).collect::<Vec<_>>();
                reads.sort_unstable();
                assert_eq!(
                    reads,
                    [1, 4],
                    "four ordered steps and a scalar tail: {source}"
                );
                for region in &dense {
                    assert!(
                        region.iter().all(|op| !matches!(
                            op,
                            Operator::I32TruncSatF64U
                                | Operator::I32TruncSatF64S
                                | Operator::I32TruncF64U
                                | Operator::I32TruncF64S
                        )),
                        "index conversion belongs outside both loops: {source}"
                    );
                }
                for expected in [
                    |op: &Operator<'_>| matches!(op, Operator::F64ReinterpretI64),
                    |op: &Operator<'_>| matches!(op, Operator::F64Mul),
                    |op: &Operator<'_>| matches!(op, Operator::F64Add),
                    |op: &Operator<'_>| matches!(op, Operator::I32LtU),
                ] {
                    assert!(
                        dense[0].iter().any(expected),
                        "direct Number loop: {source}"
                    );
                }
                assert!(
                    loops
                        .iter()
                        .any(|region| region.iter().any(|op| matches!(op, Operator::Call { .. }))),
                    "the ordered generic fallback remains: {source}"
                );
            },
        );
    }
}

#[test]
fn dense_number_region_admission_rejects_effects_aliases_and_unbounded_indices() {
    let number_array = number_array_type();
    for (source, expected) in [
        (
            "function varied(a){'use strict';var total=0;for(var i=0;i<a.length;i++)total+=(i+1)*a[i];return total;} varied([1,2,3]);",
            true,
        ),
        (
            "function varied(a){'use strict';var total=0;for(var i=0;i<a.length;i++)total+=a.length*a[i];return total;} varied([1,2,3]);",
            true,
        ),
        (
            "function varied(a){'use strict';var total=0;for(var i=0;i<=a.length;i++)total+=(i+1)*a[i];return total;} varied([1,2,3]);",
            false,
        ),
        (
            "function varied(a){'use strict';var total=0;for(var i=0;i<a.length;i++)total+=(i+1)*a[i+1];return total;} varied([1,2,3]);",
            false,
        ),
        (
            "function varied(a){'use strict';var total=0;for(var i=0;i<a.length;i-- )total+=(i+1)*a[i];return total;} varied([1,2,3]);",
            false,
        ),
        (
            "function varied(a,b){'use strict';var total=0;for(var i=0;i<a.length;i++)total+=(i+1)*b[i];return total;} varied([1,2,3],[1,2,3]);",
            false,
        ),
        (
            "function varied(a){'use strict';var total=0;for(var i=0;i<a.length;i++){total+=(i+1)*a[i];i++;}return total;} varied([1,2,3]);",
            false,
        ),
        (
            "function varied(a){'use strict';var total=0;for(var i=0;i<a.length;i++){total+=(i+1)*a[i];a[0]=7;}return total;} varied([1,2,3]);",
            false,
        ),
        (
            "function varied(a){'use strict';var total=0;for(var i=0;i<a.length;i++){total+=(i+1)*a[i];a=[9];}return total;} varied([1,2,3]);",
            false,
        ),
        (
            "function varied(a){'use strict';var total=0;function read(){return a;}for(var i=0;i<a.length;i++)total+=(i+1)*a[i];return read()[0]+total;} varied([1,2,3]);",
            false,
        ),
        (
            "function varied(a){'use strict';var total=0;for(var i=0;i<a.length;i++){total+=(i+1)*a[i];globalThis.observe(total);}return total;} varied([1,2,3]);",
            false,
        ),
    ] {
        with_source_artifact_variants(
            emit_script(source).expect("region control emits"),
            "varied",
            |_, operators| {
                assert_eq!(
                    loop_regions(operators)
                        .iter()
                        .any(|region| dense_number_loop(region, number_array)),
                    expected,
                    "{source}"
                );
            },
        );
    }
}

#[test]
fn dense_number_regions_resolve_shadowed_binding_identity() {
    let number_array = number_array_type();
    for globals in [
        "",
        "var a;",
        "var i;",
        "var total;",
        "var a,i;",
        "var a,total;",
        "var i,total;",
        "var a,i,total;",
    ] {
        let source = format!(
            "{globals}function varied(a){{'use strict';var total=0;for(var i=0;i<a.length;i++)total+=(i+1)*a[i];return total;}}varied([1,2,3]);"
        );
        with_source_artifact_variants(
            emit_script(&source).expect("shadow emits"),
            "varied",
            |_, operators| {
                assert!(
                    loop_regions(operators)
                        .iter()
                        .any(|region| dense_number_loop(region, number_array)),
                    "private shadow: {source}"
                );
            },
        );
    }
    for (source, expected) in [
        (
            "var a,i,total;function varied(){'use strict';{let a=[1,2,3],total=0;for(let i=0;i<a.length;i++)total+=(i+1)*a[i];return total;}}varied();",
            true,
        ),
        (
            "var a=[1,2,3];function varied(){'use strict';var total=0;for(var i=0;i<a.length;i++)total+=(i+1)*a[i];return total;}varied();",
            false,
        ),
        (
            "var i;function varied(a){'use strict';var total=0;for(i=0;i<a.length;i++)total+=(i+1)*a[i];return total;}varied([1,2,3]);",
            false,
        ),
        (
            "var total=0;function varied(a){'use strict';for(var i=0;i<a.length;i++)total+=(i+1)*a[i];return total;}varied([1,2,3]);",
            false,
        ),
        (
            "var a,i,total;function varied(a){var total=0;for(var i=0;i<a.length;i++)total+=(i+1)*a[i];return [total,arguments[0]];}varied([1,2,3]);",
            false,
        ),
    ] {
        with_source_artifact_variants(
            emit_script(source).expect("binding control emits"),
            "varied",
            |_, operators| {
                assert_eq!(
                    loop_regions(operators)
                        .iter()
                        .any(|region| dense_number_loop(region, number_array)),
                    expected,
                    "{source}"
                );
            },
        );
    }
}

#[test]
fn derived_checksum_sizes_enter_dense_number_regions() {
    let number_array = number_array_type();
    for size in [0, 1, 65, 513, 4096] {
        // Preserve the diagnostic kernels, including all three global collisions.
        let source = format!(
            "function checksum(a){{'use strict';var total=0;for(var i=0;i<a.length;i++)total+=(i+1)*a[i];return total;}}\nvar a=[];for(var i=1;i<={size};i++)a.push(i);var total=0;for(var repeat=0;repeat<16;repeat++)total+=checksum(a);print(total);\n"
        );
        with_source_artifact_variants(
            emit_script(&source).expect("checksum kernel emits"),
            "checksum",
            |_, operators| {
                assert_eq!(
                    loop_regions(operators)
                        .iter()
                        .filter(|region| dense_number_loop(region, number_array))
                        .count(),
                    2,
                    "size {size}"
                );
            },
        );
    }
}

#[test]
fn dense_number_body_budget_retains_scalar_admission() {
    let body = "total+=a[i];".repeat(24);
    let source = format!("function varied(a){{'use strict';var total=0;for(var i=0;i<a.length;i++){{{body}}}return total;}}varied([1,2,3]);");
    let number_array = number_array_type();
    with_source_artifact_variants(
        emit_script(&source).expect("budgeted region emits"),
        "varied",
        |_, operators| {
            let regions = loop_regions(operators);
            let dense = regions
                .iter()
                .filter(|region| dense_number_loop(region, number_array))
                .collect::<Vec<_>>();
            assert_eq!(dense.len(), 1, "large body keeps a scalar dense loop");
            assert_eq!(dense[0].iter().filter(|op| matches!(op, Operator::ArrayGet { array_type_index } if *array_type_index == number_array)).count(), 24, "no fourfold expansion");
            assert!(dense[0]
                .iter()
                .all(|op| !matches!(op, Operator::I32TruncSatF64U | Operator::I32TruncSatF64S)));
        },
    );
}

fn exact_sum_routes(operators: &[Operator<'_>]) -> Vec<(i64, u64)> {
    let mut frames = Vec::new();
    let mut routes = Vec::new();
    for (index, operator) in operators.iter().enumerate() {
        match operator {
            Operator::Block { .. }
            | Operator::If { .. }
            | Operator::Loop { .. }
            | Operator::TryTable { .. } => frames.push((index, operator)),
            Operator::End => {
                frames.pop();
            }
            _ => {}
        }
        let Some(payload) = operators.get(index..index + 13) else {
            continue;
        };
        let [Operator::I64TruncSatF64S, Operator::I64Const { value: delta }, Operator::I64Add, Operator::F64ConvertI64S, Operator::LocalSet { local_index: sum }, Operator::LocalGet {
            local_index: read_sum,
        }, Operator::LocalSet {
            local_index: body_value,
        }, Operator::F64Const { value: limit }, Operator::LocalSet {
            local_index: counter,
        }, Operator::I32Const { value: 1 }, Operator::LocalSet {
            local_index: iterated,
        }, Operator::Br { relative_depth }, Operator::End] = payload
        else {
            continue;
        };
        assert_eq!(sum, read_sum);
        let mut states = vec![sum, body_value, counter, iterated];
        states.sort_unstable();
        states.dedup();
        assert_eq!(
            states.len(),
            4,
            "sum, counter, completion and iteration state remain distinct"
        );
        assert!(
            frames
                .iter()
                .all(|(_, op)| !matches!(op, Operator::Loop { .. })),
            "whole-trip route is outside loop backedges"
        );
        let (guard, op) = frames.last().expect("guarded arithmetic");
        assert!(matches!(op, Operator::If { .. }));
        let guard_ops = &operators[guard.saturating_sub(40)..*guard];
        for check in [
            |op: &Operator<'_>| matches!(op, Operator::F64Ge),
            |op: &Operator<'_>| matches!(op, Operator::F64Le),
            |op: &Operator<'_>| matches!(op, Operator::F64Trunc),
            |op: &Operator<'_>| matches!(op, Operator::I64Ne),
        ] {
            assert!(
                guard_ops.iter().any(check),
                "live integral seed bounds and signed-zero guard"
            );
        }
        let target = frames
            .len()
            .checked_sub(*relative_depth as usize + 1)
            .expect("enclosing exit");
        assert!(
            matches!(frames[target].1, Operator::Block { .. }),
            "success skips the retained scalar path"
        );
        routes.push((*delta, limit.bits()));
    }
    routes
}

#[test]
fn original_numeric_sums_have_a_guarded_whole_trip_route() {
    for (source, delta, limit) in [
        (
            include_str!(
                "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/numeric-loop-10000.js"
            ),
            149995000,
            10000.0_f64,
        ),
        (
            include_str!(
                "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/numeric-loop-50000.js"
            ),
            3749975000,
            50000.0_f64,
        ),
    ] {
        with_source_artifact_variants(
            emit_script(source).expect("numeric workload emits"),
            "workload",
            |_, operators| {
                assert_eq!(exact_sum_routes(operators), [(delta, limit.to_bits())]);
                assert!(
                    loop_regions(operators)
                        .iter()
                        .any(|region| scalar_arithmetic_loop(region)),
                    "guard miss retains ordered floating arithmetic"
                );
            },
        );
    }
    for term in [
        "(i+9007199254740991)-9007199254740991",
        "((i+9007199254740991)-9007199254740991)*0",
        "i-2",
        "i*i",
    ] {
        let source = format!("function varied(seed){{'use strict';var total=seed;for(var i=0;i<5;i++)total+={term};return total;}}varied(0);");
        with_source_artifact_variants(
            emit_script(&source).expect("sum proof miss emits"),
            "varied",
            |_, operators| assert!(exact_sum_routes(operators).is_empty(), "{term}"),
        );
    }
}

fn joined_length_loop(operators: &[Operator<'_>]) -> bool {
    operators.iter().any(|op| matches!(op, Operator::F64Add))
        && operators.iter().any(|op| matches!(op, Operator::F64Lt))
        && operators.iter().all(|op| {
            matches!(
                op,
                Operator::LocalGet { .. }
                    | Operator::LocalSet { .. }
                    | Operator::LocalTee { .. }
                    | Operator::F64Const { .. }
                    | Operator::F64Add
                    | Operator::F64Lt
                    | Operator::I32Const { .. }
                    | Operator::I32Eqz
                    | Operator::Br { .. }
                    | Operator::BrIf { .. }
                    | Operator::Block { .. }
                    | Operator::If { .. }
                    | Operator::Else
                    | Operator::End
                    | Operator::Drop
            )
        })
}

#[test]
fn original_small_joins_use_one_guarded_length_in_the_numeric_loop() {
    for source in [
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/join-empty-1024.js"
        ),
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/join-singleton-1024.js"
        ),
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/join-small-1024.js"
        ),
    ] {
        with_source_artifact_variants(
            emit_script(source).expect("join workload emits"),
            "workload",
            |_, operators| {
                let regions = loop_regions(operators);
                assert_eq!(
                    regions
                        .iter()
                        .filter(|region| joined_length_loop(region))
                        .count(),
                    1,
                    "one pure arithmetic loop: {source}"
                );
                assert!(
                    regions
                        .iter()
                        .any(|region| region.iter().any(|op| matches!(op, Operator::Call { .. }))),
                    "ordered join fallback remains"
                );
            },
        );
    }
    for (body, expected) in [
        ("total+=a.join(',').length;", true),
        ("total+=a.join('x').length;total+=a.join('y').length;", true),
        (
            "total+=a.join(',').length;total+=b.join(',').length;",
            false,
        ),
        ("total+=a.join(',').length;a=b;", false),
        ("a=b;total+=a.join(',').length;", false),
        ("total+=a.join(separator).length;", false),
        ("total+=a.join(',').length;observe();", false),
    ] {
        let source = format!("function varied(a,b,separator){{'use strict';var total=0;for(var i=0;i<3;i++){{{body}}}return total;}}varied(['x'],['y'],',');");
        with_source_artifact_variants(
            emit_script(&source).expect("join fact control emits"),
            "varied",
            |_, operators| {
                assert_eq!(
                    loop_regions(operators)
                        .iter()
                        .any(|region| joined_length_loop(region)),
                    expected,
                    "{body}"
                )
            },
        );
    }
}

// Follow encoded labels rather than recognizing a coefficient in isolation.
// A whole-trip route must skip both retained primitive loops, then use their
// common binding/body-completion publication under the original entry guard.
fn whole_trip_lanes(operators: &[Operator<'_>]) -> Vec<(u32, u32, u64, u64)> {
    let mut stack = Vec::new();
    let mut end = vec![None; operators.len()];
    let mut otherwise = vec![None; operators.len()];
    for (index, operator) in operators.iter().enumerate() {
        match operator {
            Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::If { .. }
            | Operator::TryTable { .. } => stack.push(index),
            Operator::Else => otherwise[*stack.last().unwrap()] = Some(index),
            Operator::End => {
                if let Some(start) = stack.pop() {
                    end[start] = Some(index);
                }
            }
            _ => {}
        }
    }
    let mut lanes = Vec::new();
    for (start, window) in operators.windows(5).enumerate() {
        let [Operator::LocalGet { local_index: state }, Operator::I32Const { value: multiplier }, Operator::I32Mul, Operator::I32Const { value: addend }, Operator::I32Add] =
            window
        else {
            continue;
        };
        let frames = (0..start)
            .filter(|index| end[*index].is_some_and(|last| last > start))
            .collect::<Vec<_>>();
        if frames
            .iter()
            .any(|index| matches!(operators[*index], Operator::Loop { .. }))
        {
            continue;
        }
        let Some(&guard) = frames.last() else {
            continue;
        };
        if !matches!(operators[guard], Operator::If { .. }) || guard < 4 {
            continue;
        }
        let [Operator::LocalGet {
            local_index: counter,
        }, Operator::I64ReinterpretF64, Operator::I64Const { value: initial }, Operator::I64Eq] =
            &operators[guard - 4..guard]
        else {
            continue;
        };
        assert_eq!(
            start,
            guard + 1,
            "the actual counter check immediately guards the affine payload"
        );
        let last = end[guard].unwrap();
        let payload = &operators[start..last];
        let [Operator::LocalGet { .. }, Operator::I32Const { .. }, Operator::I32Mul, Operator::I32Const { .. }, Operator::I32Add, Operator::LocalSet {
            local_index: written_state,
        }, Operator::LocalGet {
            local_index: body_source,
        }, Operator::LocalSet {
            local_index: body_value,
        }, Operator::F64Const { value: limit }, Operator::LocalSet {
            local_index: written_counter,
        }, Operator::I32Const { value: 1 }, Operator::LocalSet {
            local_index: iterated,
        }, Operator::Br { relative_depth }] = payload
        else {
            panic!(
                "the whole-trip route must update live state, body completion, counter and iterated before its common exit: {payload:?}"
            );
        };
        assert_eq!(state, written_state);
        assert_eq!(state, body_source);
        assert_eq!(counter, written_counter);
        let mut distinct = vec![*state, *counter, *body_value, *iterated];
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            4,
            "last body value must remain independent of the counter update"
        );
        assert!(
            (*relative_depth as usize) < frames.len(),
            "the whole-trip branch targets an enclosing encoded frame, not the function label"
        );
        let target = frames[frames.len() - 1 - *relative_depth as usize];
        assert!(
            matches!(operators[target], Operator::Block { .. }),
            "success exits a block, never an interruptible loop backedge"
        );
        let done = end[target].unwrap();
        let fallback = loop_regions(&operators[last + 1..done]);
        assert_eq!(
            fallback
                .iter()
                .filter(|region| scalar_integer_loop(region))
                .count(),
            2,
            "a missed actual-counter guard retains the composed chunk and scalar tail"
        );
        let outer = frames.iter().rev().copied().find(|index| *index < target
            && matches!(operators[*index], Operator::If { .. }) && otherwise[*index].is_some())
            .expect("the whole-trip route and primitive fallbacks remain inside the live Number/native entry guard");
        let generic = otherwise[outer].unwrap();
        assert!(done < generic);
        assert!(
            loop_regions(&operators[generic + 1..end[outer].unwrap()])
                .iter()
                .any(
                    |region| region.iter().any(|op| matches!(op, Operator::Call { .. }))
                        && region.iter().any(|op| matches!(op, Operator::I32Mul))
                ),
            "outer admission failure retains the full ordered callable/coercion path"
        );
        let publication = &operators[done + 1..generic];
        for local in [state, counter, body_value, iterated] {
            assert!(
                publication.iter().any(
                    |op| matches!(op, Operator::LocalGet { local_index } if local_index == local)
                ),
                "the common exit must use every selected live state/completion local"
            );
        }
        assert!(
            publication
                .iter()
                .any(|op| matches!(op, Operator::I64ReinterpretF64)),
            "common binding/completion publication restores Number representation"
        );
        let completion_guard = (done + 2..generic).find(|index| {
            matches!(operators[*index], Operator::If { .. })
                && matches!(operators[*index - 1], Operator::LocalGet { local_index } if local_index == *iterated)
        }).expect("common completion publication is guarded by the selected iterated flag");
        let completion_end = end[completion_guard].unwrap();
        assert!(
            completion_end < generic,
            "the body completion finishes within the admitted scalar exit"
        );
        assert!(
            operators[completion_guard + 1..completion_end]
                .windows(4)
                .any(|ops| matches!(ops,
            [Operator::LocalGet { local_index },
             Operator::F64ConvertI32S | Operator::F64ConvertI32U,
             Operator::I64ReinterpretF64, Operator::LocalSet { .. }]
                if local_index == body_value)),
            "the independent last body value is converted and stored only on the iterated exit"
        );
        lanes.push((
            *multiplier as u32,
            *addend as u32,
            *initial as u64,
            limit.bits(),
        ));
    }
    lanes
}

#[test]
fn whole_trip_affine_routes_keep_guards_fallbacks_and_common_completion() {
    for (source, name, expected) in [
        (
            include_str!("../../../scriptw-cli/tests/fixtures/v8_baseline/integer.js"),
            "workload",
            (
                1244127297,
                2845218640,
                0.0_f64.to_bits(),
                10000.0_f64.to_bits(),
            ),
        ),
        (
            "function run(state){\"use strict\";for(var i=3;i<22;i++)state=(Math.imul(state,3)+1)>>>0;return state+':'+i;} run(7);",
            "run",
            (1162261467, 581130733, 3.0_f64.to_bits(), 22.0_f64.to_bits()),
        ),
        (
            "function run(state){\"use strict\";for(var i=0;i<1;i++)state=(Math.imul(state,3)+1)>>>0;return state;} run(7);",
            "run",
            (3, 1, 0.0_f64.to_bits(), 1.0_f64.to_bits()),
        ),
        (
            "function run(state){\"use strict\";for(var i=3;i<4;i++)state=(Math.imul(state,5)-7)|0;return state+':'+i;} run(7);",
            "run",
            (5, (-7_i32) as u32, 3.0_f64.to_bits(), 4.0_f64.to_bits()),
        ),
        (
            "function run(state){\"use strict\";for(var i=0;i<2147483647;i++)state=(Math.imul(state,1)+1)>>>0;return state+':'+i;} run(7);",
            "run",
            (1, 2147483647, 0.0_f64.to_bits(), 2147483647.0_f64.to_bits()),
        ),
        (
            "function run(state){\"use strict\";for(var i=2147483646;i<2147483647;i++)state=(Math.imul(state,3)+1)>>>0;return state+':'+i;} run(7);",
            "run",
            (3, 1, 2147483646.0_f64.to_bits(), 2147483647.0_f64.to_bits()),
        ),
    ] {
        with_source_operators(source, name, |operators| {
            assert_eq!(whole_trip_lanes(operators), vec![expected], "{source}")
        });
    }
    for source in [
        "function run(state){\"use strict\";var i=3;for(;i<22;i++)state=(Math.imul(state,3)+1)>>>0;return state;} run(7);",
        "function run(state){\"use strict\";var i;for(i=3;i<22;i++)state=(Math.imul(state,3)+1)>>>0;return state;} run(7);",
        "function run(state){\"use strict\";for(var i=3,other=0;i<22;i++)state=(Math.imul(state,3)+1)>>>0;return state;} run(7);",
        "function run(state){\"use strict\";for(var i=3;(i+0)<22;i++)state=(Math.imul(state,3)+1)>>>0;return state;} run(7);",
        "function run(state){\"use strict\";for(var i=3;+i<22;i++)state=(Math.imul(state,3)+1)>>>0;return state;} run(7);",
        "function run(state){\"use strict\";for(var i=3;i<=22;i++)state=(Math.imul(state,3)+1)>>>0;return state;} run(7);",
        "function run(state){\"use strict\";for(var i=3;i<3;i++)state=(Math.imul(state,3)+1)>>>0;return state;} run(7);",
        "function run(state){\"use strict\";for(var i=0;i<2147483648;i++)state=(Math.imul(state,3)+1)>>>0;return state;} run(7);",
        "function run(state){\"use strict\";var limit=22;for(var i=3;i<limit;i++)state=(Math.imul(state,3)+1)>>>0;return state;} run(7);",
        "function run(state){\"use strict\";for(var i=3;i<22;i++){state=(Math.imul(state,3)+i)>>>0;}return state;} run(7);",
    ] {
        with_source_operators(source, "run", |operators| {
            assert!(
                whole_trip_lanes(operators).is_empty(),
                "unproved shape keeps existing execution: {source}"
            )
        });
    }
}

fn scalar_traffic(source: &str, name: &str) -> (u32, u32, usize) {
    let artifact = emit_script(source).expect("scalar traffic control emits");
    expect_valid_module(&artifact, 1);
    let target = function_names(&artifact)
        .into_iter()
        .find_map(|(index, actual)| actual.starts_with(&format!("js::{name}#")).then_some(index))
        .expect("source body has a Wasm name");
    let mut index = imported_function_count(&artifact);
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("module parses") else {
            continue;
        };
        if index == target {
            let mut tags = 0;
            let mut roots = 0;
            for local in body.get_locals_reader().expect("locals parse") {
                let (count, kind) = local.expect("local parses");
                match kind {
                    wasmparser::ValType::I32 => tags += count,
                    wasmparser::ValType::Ref(kind) if kind == wasmparser::RefType::EQREF => {
                        roots += count;
                    }
                    _ => {}
                }
            }
            let arithmetic = body
                .get_operators_reader()
                .expect("operators parse")
                .into_iter()
                .filter(|operator| {
                    matches!(
                        operator.as_ref().expect("operator parses"),
                        Operator::F64Add | Operator::F64Mul | Operator::F64Sub
                    )
                })
                .count();
            return (tags, roots, arithmetic);
        }
        index += 1;
    }
    panic!("named source body is absent");
}

#[test]
fn numeric_expression_depth_does_not_grow_tag_or_reference_scratch() {
    fn source(depth: usize) -> String {
        let mut expression = "+input".to_string();
        for _ in 0..depth {
            expression = format!("({expression} + +input) * 1.5");
        }
        format!("function calculate(input) {{ return {expression}; }} calculate(globalThis.input);")
    }
    let shallow = scalar_traffic(&source(1), "calculate");
    let deep = scalar_traffic(&source(8), "calculate");
    assert!(
        deep.2 >= 16,
        "the arithmetic must survive lowering: {deep:?}"
    );
    assert!(
        deep.2 > shallow.2,
        "the deeper body must perform more arithmetic"
    );
    assert!(
        deep.0 <= shallow.0,
        "numeric intermediates add tags: {shallow:?} -> {deep:?}"
    );
    assert!(
        deep.1 <= shallow.1,
        "numeric intermediates add roots: {shallow:?} -> {deep:?}"
    );
}

#[test]
fn fixed_argument_count_does_not_grow_evaluation_tag_or_eqref_scratch() {
    fn source(count: usize) -> String {
        let arguments = vec!["input"; count].join(",");
        format!(
            "function invoke(input) {{ return globalThis.consume({arguments}); }} invoke(globalThis.input);"
        )
    }
    let one = scalar_traffic(&source(1), "invoke");
    let sixteen = scalar_traffic(&source(16), "invoke");
    assert!(
        sixteen.0 <= one.0,
        "fixed arguments add scratch tags: {one:?} -> {sixteen:?}"
    );
    assert!(
        sixteen.1 <= one.1,
        "fixed arguments add scratch roots: {one:?} -> {sixteen:?}"
    );
}

fn with_source_operators(source: &str, name: &str, check: impl FnOnce(&[Operator<'_>])) {
    let artifact = emit_script(source).expect("loop control emits");
    with_source_artifact_operators(artifact, name, check);
}

fn with_source_artifact_operators(
    artifact: WasmArtifact,
    name: &str,
    check: impl FnOnce(&[Operator<'_>]),
) {
    let mut first = Some(check);
    with_source_artifact_variants(artifact, name, |_, operators| {
        if let Some(check) = first.take() {
            check(operators);
        }
    });
}

fn with_source_artifact_variants(
    artifact: WasmArtifact,
    name: &str,
    mut check: impl FnMut(&str, &[Operator<'_>]),
) {
    expect_valid_module(&artifact, 1);
    let targets = function_names(&artifact)
        .into_iter()
        .filter(|(_, actual)| actual.starts_with(&format!("js::{name}#")))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert!(!targets.is_empty(), "source function has a Wasm name");
    let mut seen = 0;
    let mut index = imported_function_count(&artifact);
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("module parses") else {
            continue;
        };
        if let Some(name) = targets.get(&index) {
            let operators = body
                .get_operators_reader()
                .expect("source operators parse")
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .expect("source operators decode");
            check(name, &operators);
            seen += 1;
        }
        index += 1;
    }
    assert_eq!(seen, targets.len(), "every named source variant is checked");
}

fn loop_regions<'a>(operators: &'a [Operator<'a>]) -> Vec<&'a [Operator<'a>]> {
    let mut frames = Vec::new();
    let mut loops = Vec::new();
    for (index, operator) in operators.iter().enumerate() {
        match operator {
            Operator::Loop { .. } => frames.push(Some(index)),
            Operator::Block { .. } | Operator::If { .. } | Operator::TryTable { .. } => {
                frames.push(None)
            }
            Operator::End => {
                if let Some(Some(start)) = frames.pop() {
                    loops.push(&operators[start + 1..index]);
                }
            }
            _ => {}
        }
    }
    loops
}

fn scalar_arithmetic_loop(operators: &[Operator<'_>]) -> bool {
    operators.iter().any(|op| matches!(op, Operator::F64Mul))
        && operators.iter().any(|op| matches!(op, Operator::F64Lt))
        && operators.iter().all(|op| {
            matches!(
                op,
                Operator::LocalGet { .. }
                    | Operator::LocalSet { .. }
                    | Operator::LocalTee { .. }
                    | Operator::F64Const { .. }
                    | Operator::F64Add
                    | Operator::F64Sub
                    | Operator::F64Mul
                    | Operator::F64Div
                    | Operator::F64Lt
                    | Operator::I32Const { .. }
                    | Operator::I32Eqz
                    | Operator::Br { .. }
                    | Operator::BrIf { .. }
                    | Operator::Drop
            )
        })
}

#[test]
fn original_numeric_workloads_contain_a_primitive_loop_and_the_generic_fallback() {
    for source in [
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/numeric-loop-10000.js"
        ),
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/numeric-loop-50000.js"
        ),
    ] {
        with_source_operators(source, "workload", |operators| {
            let loops = loop_regions(operators);
            let mut multiplications = loops
                .iter()
                .filter(|region| scalar_arithmetic_loop(region))
                .map(|region| {
                    region
                        .iter()
                        .filter(|op| matches!(op, Operator::F64Mul))
                        .count()
                })
                .collect::<Vec<_>>();
            multiplications.sort_unstable();
            assert_eq!(
                multiplications,
                [1, 5],
                "the original recurrence needs an ordered five-step chunk and one-step tail, without calls, references, tags or completion copies"
            );
            assert!(
                loops.iter().any(
                    |region| region.iter().any(|op| matches!(op, Operator::F64Mul))
                        && region
                            .iter()
                            .any(|op| matches!(op, Operator::I64ReinterpretF64))
                ),
                "a failed Number guard must retain ordinary numeric/coercion dispatch"
            );
        });
    }
}

#[test]
fn numeric_loop_admission_uses_operations_and_binding_identity() {
    for (source, expected) in [
        (
            "function varied(total, stop) { \"use strict\"; for(var step=0;(step+0)<stop;step++) total+=step*2+0.5; return total; } varied(1,7);",
            true,
        ),
        (
            "function varied(total, stop) { \"use strict\"; function capture(){return total;} for(var step=0;(step+0)<stop;step++) total+=step*2+0.5; return capture(); } varied(1,7);",
            false,
        ),
        (
            "function varied(total, stop) { \"use strict\"; for(var step=0;(step+0)<stop;step++) { total+=step*2+0.5; globalThis.observe(total); } return total; } varied(1,7);",
            false,
        ),
        (
            "function varied(total, stop) { \"use strict\"; for(var step=0;(step+0)<stop;step++) { total+=step*2+0.5; if(step) break; } return total; } varied(1,7);",
            false,
        ),
        // Uncaptured `let` heads need no materialized per-iteration cells.
        // Admission follows actual local storage and IR environment ownership.
        (
            "function varied(total, stop) { \"use strict\"; for(let step=0;(step+0)<stop;step++) total+=step*2+0.5; return total; } varied(1,7);",
            true,
        ),
        (
            "function varied() { const total=0; for(var step=0;(step+0)<3;step++) total+=step*2+0.5; } varied();",
            false,
        ),
    ] {
        with_source_operators(source, "varied", |operators| {
            assert_eq!(
                loop_regions(operators)
                    .iter()
                    .any(|region| scalar_arithmetic_loop(region)),
                expected,
                "{source}"
            );
        });
    }
}

fn scalar_integer_loop(operators: &[Operator<'_>]) -> bool {
    operators.iter().any(|op| matches!(op, Operator::I32Mul))
        && operators.iter().any(|op| matches!(op, Operator::F64Lt))
        && operators.iter().all(|op| {
            matches!(
                op,
                Operator::LocalGet { .. }
                    | Operator::LocalSet { .. }
                    | Operator::F64Const { .. }
                    | Operator::I32Const { .. }
                    | Operator::F64Add
                    | Operator::F64Lt
                    | Operator::I32Mul
                    | Operator::I32Add
                    | Operator::I32Sub
                    | Operator::I32Or
                    | Operator::I32ShrU
                    | Operator::I32Eqz
                    | Operator::Br { .. }
                    | Operator::BrIf { .. }
                    | Operator::Drop
            )
        })
}

#[test]
fn original_integer_workload_contains_a_scalar_loop_with_invariant_native_guard() {
    with_source_operators(
        include_str!("../../../scriptw-cli/tests/fixtures/v8_baseline/integer.js"),
        "workload",
        |operators| {
            let loops = loop_regions(operators);
            let primitive_imul = loops
                .iter()
                .filter(|region| scalar_integer_loop(region))
                .collect::<Vec<_>>();
            assert_eq!(
                primitive_imul.len(),
                2,
                "the unchanged integer recurrence needs a composed chunk and original tail with no property/call/Realm/GC traffic or float/integer conversions"
            );
            assert!(
                primitive_imul
                    .iter()
                    .any(|region| region.windows(5).any(|ops| matches!(
                        ops,
                        [
                            Operator::I32Mul,
                            Operator::I32Const { value: 1013904223 },
                            Operator::I32Add,
                            Operator::I32Const { value: 0 },
                            Operator::I32ShrU
                        ]
                    ))),
                "the exact Number sum is observed only as a residue and must stay in i32"
            );
            assert!(primitive_imul.iter().any(|region| {
                region.iter().any(|op| matches!(op, Operator::F64Const { value } if value.bits() == 8.0_f64.to_bits()))
                    && region.iter().filter(|op| matches!(op, Operator::I32Mul)).count() == 1
                    && !region.iter().any(|op| matches!(op, Operator::I32ShrU))
            }), "eight exact residue steps collapse to one multiply/add and an exact counter advance");
            assert!(
                loops.iter().any(|region| region
                    .iter()
                    .any(|op| matches!(op, Operator::Call { .. }))
                    && region.iter().any(|op| matches!(op, Operator::I32Mul))),
                "failed invariant/native guards retain the existing ordered call path"
            );
        },
    );
}

#[test]
fn scalar_chunks_keep_floating_operations_ordered_and_compose_other_affine_coefficients() {
    // Strict parameters use private locals; sloppy mapped arguments correctly
    // retain environment storage and cannot enter this scalar region.
    // Numeric local bounds retain native-method facts in the generic body;
    // the floating control below separately covers dynamic Number bounds.
    for (source, signed) in [
        (
            "function run(state){\"use strict\";var stop=19;for(var i=0;i<stop;i++)state=(Math.imul(state,3)+1)>>>0;return state;} run(1);",
            false,
        ),
        (
            "function run(state){\"use strict\";var stop=2;for(var i=20;i>=stop;i--)state=(Math.imul(state,5)-7)|0;return state;} run(1);",
            true,
        ),
    ] {
        with_source_operators(source, "run", |operators| {
            let loops = loop_regions(operators);
            let chunk = loops.iter().find(|region| {
                region.iter().any(|op| matches!(op, Operator::I32Const { value } if *value == if signed {390625} else {6561}))
                    && !region.iter().any(|op| matches!(op, Operator::Call { .. }))
            }).expect("another affine map uses its mathematically composed eight-step coefficient");
            assert_eq!(
                chunk
                    .iter()
                    .filter(|op| matches!(op, Operator::I32Mul))
                    .count(),
                1
            );
            assert!(chunk.iter().any(|op| matches!(op, Operator::I32Const { value } if *value == if signed {-683592} else {3280})));
            assert_eq!(
                chunk
                    .iter()
                    .filter(|op| matches!(op, Operator::F64Lt | Operator::F64Ge))
                    .count(),
                1,
                "the chunk has one monotone last-position test"
            );
        });
    }
    with_source_operators(
        "function run(total,stop){ \"use strict\";for(var i=0;i<stop;i++)total=(total+i*0.1)*1.0000000000000002-i/7;return total;} run(0,19);",
        "run",
        |operators| {
            let loops = loop_regions(operators);
            let chunk = loops
                .iter()
                .find(|region| {
                    scalar_arithmetic_loop(region)
                        && region
                            .iter()
                            .filter(|op| matches!(op, Operator::F64Div))
                            .count()
                            == 5
                })
                .expect("five ordered floating iterations have one backedge");
            let arithmetic = chunk
                .iter()
                .filter_map(|op| match op {
                    Operator::F64Mul => Some('m'),
                    Operator::F64Add => Some('a'),
                    Operator::F64Div => Some('d'),
                    Operator::F64Sub => Some('s'),
                    _ => None,
                })
                .collect::<String>();
            assert_eq!(
                arithmetic,
                format!("a{}", "mam dsa".replace(' ', "").repeat(5)),
                "last-position lookahead precedes each unchanged multiply/add/multiply/divide/subtract and unit advance"
            );
        },
    );
}

#[test]
fn scalar_chunk_proofs_keep_written_tests_counters_and_full_values_on_their_owners() {
    for (source, expected_multiplications) in [
        (
            "function run(state){\"use strict\";var stop=20;for(var i=0;i<stop;i++){stop--;state=(Math.imul(state,3)+1)>>>0;}return state;} run(1);",
            vec![1],
        ),
        (
            "function run(state){\"use strict\";var stop=20;for(var i=0;i<stop;i++){i++;state=(Math.imul(state,3)+1)>>>0;}return state;} run(1);",
            vec![1],
        ),
        (
            "function run(state){\"use strict\";var stop=20;var i=0;for(;(i=i+1)<stop;i++)state=(Math.imul(state,3)+1)>>>0;return state;} run(1);",
            vec![1],
        ),
        (
            "function run(state){\"use strict\";var stop=20;for(var i=0;i<stop;i++)state=(Math.imul(state,state)+1)>>>0;return state;} run(1);",
            vec![1, 5],
        ),
        (
            "function run(state){\"use strict\";var stop=20;var full=0;for(var i=0;i<stop;i++)state=(full=Math.imul(state,3)+1)>>>0;return full;} run(1);",
            vec![1, 5],
        ),
        (
            "function run(state){\"use strict\";var stop=20;for(var i=0;i<stop;i++)state=(Math.imul(state,3)+i)>>>0;return state;} run(1);",
            vec![1, 5],
        ),
    ] {
        with_source_operators(source, "run", |operators| {
            let mut actual = loop_regions(operators)
                .into_iter()
                .filter(|region| {
                    region.iter().any(|op| matches!(op, Operator::I32Mul))
                        && !region.iter().any(|op| matches!(op, Operator::Call { .. }))
                })
                .map(|region| {
                    region
                        .iter()
                        .filter(|op| matches!(op, Operator::I32Mul))
                        .count()
                })
                .collect::<Vec<_>>();
            actual.sort_unstable();
            assert_eq!(actual, expected_multiplications, "{source}");
        });
    }
}

#[test]
fn integer_residue_consumers_preserve_full_number_assignment_stores() {
    with_source_operators(
        "function run(state) { \"use strict\"; var full=0; for(var i=0;i<2;i++) state=(full=Math.imul(state,1664525)+1013904223)>>>0; return full; } run(123456789);",
        "run",
        |operators| {
            let loops = loop_regions(operators);
            let scalar = loops
                .iter()
                .find(|region| {
                    region.iter().any(|op| matches!(op, Operator::I32Mul))
                        && !region.iter().any(|op| matches!(op, Operator::Call { .. }))
                })
                .expect("the assignment remains a scalar region");
            assert!(
                scalar.windows(5).any(|ops| matches!(
                    ops,
                    [
                        Operator::I32Mul,
                        Operator::F64ConvertI32S,
                        Operator::F64Const { .. },
                        Operator::F64Add,
                        Operator::LocalSet { .. }
                    ]
                )),
                "an enclosing ToUint32 cannot truncate the nested assignment's full Number"
            );
            assert!(
                scalar
                    .iter()
                    .any(|op| matches!(op, Operator::I64TruncSatF64S)),
                "the stored full exact sum is converted only after its assignment"
            );
        },
    );
}

#[test]
fn module_var_loops_use_scalars_only_when_activation_storage_is_private() {
    for (source, expected) in [
        (
            "export function varied(total, stop) { for(var step=0;(step+0)<stop;step++) { total+=step*2+0.5; } return total; }",
            true,
        ),
        (
            "export const run = function varied(total, stop) { for(var step=0;(step+0)<stop;step++) { total+=step*2+0.5; } return total; };",
            true,
        ),
        (
            "export function varied(total, stop) { function capture(){return total;} for(var step=0;(step+0)<stop;step++) { total+=step*2+0.5; } return capture(); }",
            false,
        ),
        (
            "export function varied(total, stop) { for(var step=0;(step+0)<stop;step++) { total+=step*2+0.5; } return total; } export function evaluate(){ return eval('0'); }",
            false,
        ),
    ] {
        let parsed = parse(source, ParseOptions::module()).expect("module loop parses");
        let program = lower(&parsed);
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let artifact = emit(&program).expect("module loop emits actual compiled Wasm");
        with_source_artifact_operators(artifact, "varied", |operators| {
            assert_eq!(
                loop_regions(operators)
                    .iter()
                    .any(|region| scalar_arithmetic_loop(region)),
                expected,
                "{source}"
            );
        });
    }
}
