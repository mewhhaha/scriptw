//! Real vector bodies build no arguments object at entry and guard every
//! projected read on whether a read has needed the object yet.
use crate::{property_read_plans::Module, scalar_masked_rows::type_group_sizes};
use scriptw_aot_wasm::{emit_with_rooted_snapshot, GcSnapshotLayout, PromiseRejectionPolicy};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{Operator, Validator, WasmFeatures};

/// The arms of the `if` at `start`: its then-arm and, when present, its
/// else-arm.
fn if_parts<'a>(
    body: &'a [Operator<'a>],
    start: usize,
) -> (&'a [Operator<'a>], Option<&'a [Operator<'a>]>) {
    assert!(matches!(body[start], Operator::If { .. }));
    let mut depth = 1;
    let mut otherwise = None;
    for index in start + 1..body.len() {
        match body[index] {
            Operator::If { .. }
            | Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::TryTable { .. } => depth += 1,
            Operator::Else if depth == 1 => otherwise = Some(index),
            Operator::End => {
                depth -= 1;
                if depth == 0 {
                    return match otherwise {
                        Some(otherwise) => (
                            &body[start + 1..otherwise],
                            Some(&body[otherwise + 1..index]),
                        ),
                        None => (&body[start + 1..index], None),
                    };
                }
            }
            _ => {}
        }
    }
    panic!("guard ends inside source body");
}

fn allocations(body: &[Operator<'_>], ty: u32) -> usize {
    body.iter().filter(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == ty)).count()
}

/// The entry's count guard: the argument count stored once and tested to
/// snapshot index zero, with no object built on either side.
fn projected_entry(body: &[Operator<'_>], arguments: u32) -> Option<usize> {
    let guards = body
        .windows(4)
        .enumerate()
        .filter_map(|(index, ops)| {
            let [Operator::ArrayLen, Operator::LocalSet {
                local_index: stored,
            }, Operator::LocalGet {
                local_index: loaded,
            }, Operator::If { .. }] = ops
            else {
                return None;
            };
            if stored != loaded {
                return None;
            }
            let (snapshot, otherwise) = if_parts(body, index + 3);
            (otherwise.is_none()
                && allocations(snapshot, arguments) == 0
                && snapshot
                    .iter()
                    .any(|op| matches!(op, Operator::ArrayGet { .. })))
            .then_some(index + 3)
        })
        .collect::<Vec<_>>();
    assert!(guards.len() <= 1, "one invocation count decides all reads");
    guards.into_iter().next()
}

fn check_projection(body: &[Operator<'_>], arguments: u32, stored: u32, vector_entry: bool) {
    if vector_entry {
        let entry = projected_entry(body, arguments).expect("real entry projects arguments");
        let (snapshot, _) = if_parts(body, entry);
        assert_eq!(snapshot.iter().filter(|op| matches!(op, Operator::StructGet { struct_type_index, .. } if *struct_type_index == stored)).count(), 3, "snapshot preserves complete tag/scalar/reference");
    } else {
        assert!(projected_entry(body, arguments).is_none(), "wide local entry carries count directly without an argv length snapshot");
    }
    // Each read tests whether the real object exists (its local's null
    // reference sentinel) before answering from the local lanes or vector.
    let mut guards = std::collections::BTreeMap::<u32, Vec<usize>>::new();
    for (index, ops) in body.windows(2).enumerate() {
        if let [Operator::LocalGet { local_index }, Operator::RefIsNull] = ops {
            if let Some(offset) = body[index + 2..(index + 8).min(body.len())]
                .iter()
                .position(|op| matches!(op, Operator::If { .. }))
            {
                let start = index + 2 + offset;
                if let (_, Some(slow)) = if_parts(body, start) {
                    if allocations(slow, arguments) == 1 {
                        guards.entry(*local_index).or_default().push(start);
                    }
                }
            }
        }
    }
    assert_eq!(
        guards.len(),
        1,
        "one materialized-object word guards every read"
    );
    let reads = guards.into_values().next().unwrap();
    assert_eq!(reads.len(), 2, "index zero and length share that guard");
    assert_eq!(
        allocations(body, arguments),
        reads.len(),
        "the object is built only on a read's slow arm, never at entry"
    );
    for index in reads {
        let (fast, slow) = if_parts(body, index);
        assert!(
            fast.iter().all(|op| !matches!(
                op,
                Operator::Call { .. }
                    | Operator::CallRef { .. }
                    | Operator::CallIndirect { .. }
                    | Operator::StructNew { .. }
            )),
            "projected fast reads have no lookup, allocation or callback"
        );
        assert!(
            slow.unwrap()
                .iter()
                .any(|op| matches!(op, Operator::Call { .. })),
            "a read that needs the object retains the original operation"
        );
    }
}

#[test]
fn actual_observed_workload_and_five_formals_keep_vector_abi_with_count_guarded_reads() {
    std::thread::Builder::new().name("strict-arguments-projection".into()).stack_size(64*1024*1024).spawn(|| {
        for (source, actual, expected_params) in [
            (include_str!("../fixtures/strict_arguments_observed.js"), true, 29usize),
            ("export function projected(a,b,c,d,e) { a=b; return [arguments[0],arguments.length]; }\nexport function escaped(a) { return arguments; }\nexport function reference(a) { return arguments[0](); }", false, 29usize),
            ("export function projected(a,b,c,d,e,f,g) { a=b; return [arguments[0],arguments.length]; }", false, 11usize),
        ] {
            let ir = lower(&parse(source, ParseOptions::module()).unwrap());
            assert!(ir.is_wasm_supported(), "{:?}", ir.diagnostics);
            let candidates = ir.script.as_ref().unwrap().functions.iter().filter(|function| {
                if actual { function.params.len()==2 } else { function.name=="projected" }
            }).collect::<Vec<_>>();
            assert_eq!(candidates.len(), 1, "select the actual observed two-formal step or explicit projected control");
            let source_function = candidates[0];
            let artifact = emit_with_rooted_snapshot(&ir, PromiseRejectionPolicy::Ignore, &scriptw_intl::IntlCompilationProfile::Minimal).unwrap();
            let runtime_bytes = artifact.runtime().unwrap().bytes();
            for bytes in [artifact.bytes.as_slice(),runtime_bytes] {
                Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
            }
            let runtime = Module::read(runtime_bytes, false);
            let program = Module::read(&artifact.bytes, true);
            assert!(program.types.len()>=runtime.types.len());
            assert_eq!(&program.types[..runtime.types.len()], &runtime.types);
            let r_groups=type_group_sizes(runtime_bytes);
            let p_groups=type_group_sizes(&artifact.bytes);
            assert!(p_groups.len()>=r_groups.len());
            assert_eq!(&p_groups[..r_groups.len()],&r_groups);
            let arguments=runtime.witness_type(GcSnapshotLayout::Arguments);
            let stored=runtime.witness_type(GcSnapshotLayout::Stored);
            let (index,body)=program.named(&format!("js::{}#{}",source_function.name,source_function.id));
            assert_eq!(program.function_type(index).params().len(),expected_params,"expected wide-local (29) or vector (11) ABI");
            assert_eq!(program.function_type(index).results().len(),5,"whole Completion unchanged");
            check_projection(body,arguments,stored,expected_params == 11);
            if !actual && expected_params == 29 {
                for name in ["escaped","reference"] {
                    let (_,body)=program.named(&format!("js::{name}#"));
                    assert_eq!(allocations(body,arguments),1);
                    assert!(projected_entry(body,arguments).is_none(),"{name} retains unconditional materialization");
                }
            }
        }
    }).unwrap().join().unwrap();
}
