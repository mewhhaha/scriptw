//! Real vector bodies guard object construction and every projected read.
use crate::{property_read_plans::Module, scalar_masked_rows::type_group_sizes};
use scriptw_aot_wasm::{GcSnapshotLayout, PromiseRejectionPolicy, emit_with_rooted_snapshot};
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::lower;
use wasmparser::{Operator, Validator, WasmFeatures};

fn if_arms<'a>(body: &'a [Operator<'a>], start: usize) -> (&'a [Operator<'a>], &'a [Operator<'a>]) {
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
                    let otherwise =
                        otherwise.expect("both nonempty and absent-index paths are encoded");
                    return (&body[start + 1..otherwise], &body[otherwise + 1..index]);
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

fn projected_count(body: &[Operator<'_>], arguments: u32) -> Option<(u32, usize)> {
    let guards = body
        .windows(4)
        .enumerate()
        .filter_map(|(index, ops)| {
            let [
                Operator::ArrayLen,
                Operator::LocalSet {
                    local_index: stored,
                },
                Operator::LocalGet {
                    local_index: loaded,
                },
                Operator::If { .. },
            ] = ops
            else {
                return None;
            };
            if stored != loaded {
                return None;
            }
            let (_, absent) = if_arms(body, index + 3);
            (allocations(absent, arguments) == 1).then_some((*stored, index + 3))
        })
        .collect::<Vec<_>>();
    assert!(guards.len() <= 1, "one invocation count decides all reads");
    guards.into_iter().next()
}

fn check_projection(body: &[Operator<'_>], arguments: u32, stored: u32) {
    let (count, entry) = projected_count(body, arguments).expect("real entry projects arguments");
    let (present, absent) = if_arms(body, entry);
    assert_eq!(allocations(body, arguments), 1);
    assert_eq!(
        allocations(present, arguments),
        0,
        "nonempty calls never construct the arguments object"
    );
    assert_eq!(
        allocations(absent, arguments),
        1,
        "empty calls eagerly retain the real object"
    );
    assert!(
        present
            .iter()
            .any(|op| matches!(op, Operator::ArrayGet { .. })),
        "first value is snapshotted from actual input before formals/body"
    );
    assert_eq!(present.iter().filter(|op| matches!(op, Operator::StructGet { struct_type_index, .. } if *struct_type_index == stored)).count(), 3, "snapshot preserves complete tag/scalar/reference");
    let reads = body
        .windows(2)
        .enumerate()
        .filter_map(|(index, ops)| match ops {
            [Operator::LocalGet { local_index }, Operator::If { .. }]
                if *local_index == count && index + 1 != entry =>
            {
                Some(index + 1)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        reads.len(),
        2,
        "index zero and length use the same invocation discriminator"
    );
    for index in reads {
        let (present, absent) = if_arms(body, index);
        assert!(
            present.iter().all(|op| !matches!(
                op,
                Operator::Call { .. }
                    | Operator::CallRef { .. }
                    | Operator::CallIndirect { .. }
                    | Operator::StructNew { .. }
                    | Operator::StructGet { .. }
                    | Operator::ArrayGet { .. }
            )),
            "snapshot reads have no lookup, allocation or callback"
        );
        assert!(
            absent.iter().any(|op| matches!(op, Operator::Call { .. })),
            "every empty-call read retains the original operation"
        );
    }
}

#[test]
fn actual_observed_workload_and_five_formals_keep_vector_abi_with_count_guarded_reads() {
    std::thread::Builder::new().name("strict-arguments-projection".into()).stack_size(64*1024*1024).spawn(|| {
        for (source, actual) in [
            (include_str!("../fixtures/strict_arguments_observed.js"), true),
            ("export function projected(a,b,c,d,e) { a=b; return [arguments[0],arguments.length]; }\nexport function escaped(a) { return arguments; }\nexport function reference(a) { return arguments[0](); }", false),
        ] {
            let ir = lower(&parse(source, ParseOptions::module()).unwrap());
            assert!(ir.is_wasm_supported(), "{:?}", ir.diagnostics);
            let candidates = ir.script.as_ref().unwrap().functions.iter().filter(|function| {
                if actual { function.params.len()==2 } else { function.name=="projected" }
            }).collect::<Vec<_>>();
            assert_eq!(candidates.len(), 1, "select the actual observed two-formal step or explicit five-formal control");
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
            assert_eq!(program.function_type(index).params().len(),11,"unchanged vector entry, including the observed two-formal source");
            assert_eq!(program.function_type(index).results().len(),5,"whole Completion unchanged");
            check_projection(body,arguments,stored);
            if !actual {
                for name in ["escaped","reference"] {
                    let (_,body)=program.named(&format!("js::{name}#"));
                    assert_eq!(allocations(body,arguments),1);
                    assert!(projected_count(body,arguments).is_none(),"{name} retains unconditional materialization");
                }
            }
        }
    }).unwrap().join().unwrap();
}
