//! The ordinary inline backedge stays identical to the published scalar owner.
#[path = "row_loop_operators.rs"]
mod row_loop_operators;
use crate::{property_read_plans::Module, scalar_masked_rows::type_group_sizes};
use row_loop_operators::{closed_number_loops, normalize};
use scriptw_aot_wasm::{emit_with_rooted_snapshot, GcSnapshotLayout, PromiseRejectionPolicy};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{
    CompositeInnerType, HeapType, Operator, UnpackedIndex, ValType, Validator, WasmFeatures,
};

fn reference(ty: ValType) -> u32 {
    let ValType::Ref(ty) = ty else {
        panic!("typed GC reference");
    };
    let HeapType::Concrete(UnpackedIndex::Module(index)) = ty.heap_type() else {
        panic!("module-owned GC type");
    };
    index
}

fn controls_before(body: &[Operator<'_>], until: usize) -> Vec<usize> {
    let mut controls = Vec::new();
    for (at, op) in body[..until].iter().enumerate() {
        match op {
            Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::If { .. }
            | Operator::TryTable { .. } => controls.push(at),
            Operator::End => {
                controls.pop();
            }
            _ => {}
        }
    }
    controls
}

#[test]
fn ordinary_inline_operators_match_published_baseline_and_dictionary_has_a_separate_backedge() {
    std::thread::Builder::new().name("scalar-row-modes".into()).stack_size(64 * 1024 * 1024).spawn(|| {
        // One-period control retains the published guarded inline backedge.
        // The unchanged repeated workload is covered by scalar_row_projection.
        let source = include_str!("fixtures/compact_inline_metadata.mjs").replace("i < 8192", "i < 64");
        let ir = lower(&parse(&source, ParseOptions::module()).unwrap());
        let artifact = emit_with_rooted_snapshot(&ir, PromiseRejectionPolicy::Ignore,
            &scriptw_intl::IntlCompilationProfile::Minimal).unwrap();
        for bytes in [artifact.bytes.as_slice(), artifact.runtime().unwrap().bytes()] {
            Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
        }
        let program = Module::read(&artifact.bytes, true);
        let runtime = Module::read(artifact.runtime().unwrap().bytes(), true);
        assert!(program.types.len() >= runtime.types.len());
        assert_eq!(&program.types[..runtime.types.len()], &runtime.types);
        let pg = type_group_sizes(&artifact.bytes);
        let rg = type_group_sizes(artifact.runtime().unwrap().bytes());
        assert_eq!(&pg[..rg.len()], &rg, "authenticate complete canonical recursive groups");
        let (_, body) = program.named("js::run#");
        let loops = closed_number_loops(body);
        assert_eq!(loops.len(), 2, "one inline and one dictionary body, sharing the original continuation");
        let expected = include_str!("fixtures/compact_inline_metadata.ops").lines()
            .filter(|line| !line.starts_with('#')).map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(expected.len(), 193, "receipt from the fingerprinted published P");
        let stored = runtime.witness_type(GcSnapshotLayout::Stored);
        let normalized = loops.iter().map(|region| normalize(body, region.clone(), stored, &runtime.types)).collect::<Vec<_>>();
        let inline = loops.iter().zip(&normalized).filter_map(|(region, actual)| (actual == &expected).then_some(region)).collect::<Vec<_>>();
        if inline.len() != 1 {
            for (region, actual) in loops.iter().zip(&normalized) {
                let first = actual.iter().zip(&expected).position(|(actual, expected)| actual != expected)
                    .unwrap_or(actual.len().min(expected.len()));
                eprintln!("loop {region:?}: {} operators; first difference at {first}: expected {:?}, actual {:?}; actual StoredValue type {stored}",
                    actual.len(), expected.get(first), actual.get(first));
            }
        }
        assert_eq!(inline.len(), 1, "exact old inline operators, immediates, roots, aliasing and control destinations");
        let dictionary_loop = loops.iter().find(|region| *region != inline[0]).unwrap();
        let (prepare, _) = runtime.named("helper::ordinary_property_prepare");
        let dictionary = reference(runtime.function_type(prepare).params()[0]);
        let CompositeInnerType::Struct(layout) = &runtime.types[dictionary as usize].composite_type.inner else { panic!("canonical dictionary storage"); };
        let table = reference(layout.fields[0].element_type.unpack());
        // The complete canonical type and recursive-group prefix above authenticates
        // these R-derived indices in P; P need not import this allocation helper.
        let dict = &body[dictionary_loop.start..dictionary_loop.end];
        assert!(dict.iter().any(|op| matches!(op, Operator::RefTestNonNull {
            hty: HeapType::Concrete(UnpackedIndex::Module(index)) } if *index == dictionary)));
        assert_eq!(dict.iter().filter(|op| matches!(op, Operator::ArrayGet {array_type_index} if *array_type_index == table)).count(), 2,
            "both current dictionary field entries remain guarded");
        let mut outer_mode = None;
        let mut completed_prefix_exit = None;
        for region in &loops {
            let controls = controls_before(body, region.start);
            let mode_if = *controls.iter().rev().find(|at| matches!(body[**at], Operator::If { .. })).unwrap();
            let Operator::LocalGet {local_index: mode} = body[mode_if - 1] else { panic!("single mode test before either loop"); };
            assert!(!body[region.clone()].iter().any(|op| matches!(op, Operator::LocalGet {local_index} if *local_index == mode)),
                "no per-field or per-row dynamic mode test remains on either backedge");
            if let Some(previous) = outer_mode { assert_eq!(previous, mode_if); }
            outer_mode = Some(mode_if);
            let first_bound = region.start + body[region.clone()].iter().position(|op| matches!(op, Operator::BrIf { .. })).unwrap();
            let Operator::BrIf {relative_depth} = body[first_bound] else { unreachable!() };
            let controls = controls_before(body, first_bound);
            let done = controls[controls.len() - 1 - relative_depth as usize];
            assert!(matches!(body[done], Operator::Block { .. }));
            if let Some(previous) = completed_prefix_exit { assert_eq!(previous, done); }
            completed_prefix_exit = Some(done);
        }
        assert!(body[loops.iter().map(|region| region.end).max().unwrap()..].iter().any(|op| matches!(op, Operator::Call { .. } | Operator::CallRef { .. })),
            "generic continuation remains after the shared publication and cleanup");
    }).unwrap().join().unwrap();
}
