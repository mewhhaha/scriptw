//! Function header admission shares the live own-data decoder, not snapshots.
use crate::{
    linked_bodies::module_bodies,
    property_read_plans::{calls, Module},
};
use scriptw_aot_wasm::{
    emit_with_rooted_snapshot, GcSnapshotField, GcSnapshotLayout, PromiseRejectionPolicy,
};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{HeapType, Operator, Parser, Payload, UnpackedIndex, Validator, WasmFeatures};

fn groups(bytes: &[u8]) -> Vec<usize> {
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::TypeSection(groups) = payload.unwrap() {
            return groups
                .into_iter()
                .map(|group| group.unwrap().types().len())
                .collect();
        }
    }
    panic!("module declares types");
}

fn is_test(op: &Operator<'_>, ty: u32) -> bool {
    matches!(op, Operator::RefTestNonNull { hty: HeapType::Concrete(UnpackedIndex::Module(index)) } if *index == ty)
}

fn assert_direct_function_header(body: &[Operator<'_>], function: u32, object: u32) {
    let mut arms = Vec::new();
    for start in 0..body.len().saturating_sub(1) {
        if !is_test(&body[start], function) || !matches!(body[start + 1], Operator::If { .. }) {
            continue;
        }
        let mut depth = 0;
        let end = (start + 2..body.len())
            .find(|&index| {
                match &body[index] {
                    Operator::Block { .. }
                    | Operator::Loop { .. }
                    | Operator::If { .. }
                    | Operator::TryTable { .. } => depth += 1,
                    Operator::Else if depth == 0 => return true,
                    Operator::End if depth == 0 => return true,
                    Operator::End => depth -= 1,
                    _ => {}
                }
                false
            })
            .expect("actual callable arm closes");
        let arm = &body[start + 2..end];
        if arm.iter().any(|op| matches!(op, Operator::StructGet { struct_type_index, field_index }
            if *struct_type_index == function && *field_index == GcSnapshotField::FunctionObject.index())) {
            arms.push(arm);
        }
    }
    assert_eq!(
        arms.len(),
        1,
        "one concrete FunctionObject header admission"
    );
    let arm = arms[0];
    assert!(!arm.iter().any(|op| matches!(op,
        Operator::StructGet { struct_type_index, field_index }
            if *struct_type_index == object && *field_index == GcSnapshotField::ObjectProperties.index())),
        "header admission cannot exclude the current ordinary dictionary storage");
    assert!(
        !arm.iter().any(|op| matches!(
            op,
            Operator::If { .. }
                | Operator::Call { .. }
                | Operator::CallRef { .. }
                | Operator::RefTestNonNull { .. }
                | Operator::StructNew { .. }
        )),
        "the proven callable projects its canonical header without another guard, call or snapshot"
    );
}

#[test]
fn function_data_probe_admits_current_headers_in_r_and_computed_p_without_changing_fallbacks() {
    std::thread::Builder::new()
        .name("function-data-probe".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let ir = lower(
                &parse(
                    r#"
export function literal(fn, unused, extra) { return fn.name; }
export function computed(fn, key, extra) { return fn[key]; }
"#,
                    ParseOptions::module(),
                )
                .unwrap(),
            );
            let artifact = emit_with_rooted_snapshot(
                &ir,
                PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            )
            .unwrap();
            let runtime_bytes = artifact.runtime().unwrap().bytes();
            for bytes in [runtime_bytes, artifact.bytes.as_slice()] {
                Validator::new_with_features(WasmFeatures::all())
                    .validate_all(bytes)
                    .unwrap();
            }
            let runtime = Module::read(runtime_bytes, true);
            let program = Module::read(&artifact.bytes, true);
            // Only R exports opt-in witnesses. Prove exact shared type indices and
            // whole recursive-group boundaries before using them on P's operators.
            assert!(program.types.len() >= runtime.types.len());
            assert_eq!(&program.types[..runtime.types.len()], &runtime.types);
            let r_groups = groups(runtime_bytes);
            let p_groups = groups(&artifact.bytes);
            assert!(p_groups.len() >= r_groups.len());
            assert_eq!(&p_groups[..r_groups.len()], &r_groups);
            let function = runtime.witness_type(GcSnapshotLayout::Function);
            let object = runtime.witness_type(GcSnapshotLayout::Object);
            let (cache, cache_body) = runtime.named("helper::cached_property_read");
            let (get, get_body) = runtime.named("helper::object_read");
            for body in [cache_body, get_body] {
                assert_direct_function_header(body, function, object);
            }
            for layout in [GcSnapshotLayout::BoundFunction, GcSnapshotLayout::Proxy] {
                let ty = runtime.witness_type(layout);
                assert!(
                    !cache_body.iter().any(|op| is_test(op, ty)),
                    "cache header admission does not acquire BoundFunction or Proxy cases"
                );
            }
            let (find, _) = runtime.named("helper::ordinary_property_find");
            let (descriptor, _) = runtime.named("helper::object_get_own_property");
            assert!(
                calls(get_body, find),
                "data reads reuse the current ordinary entry lookup"
            );
            assert!(
                calls(get_body, descriptor),
                "accessor/exotic misses retain private descriptors"
            );
            assert!(
                calls(cache_body, get),
                "cache misses retain full Get with the original receiver"
            );
            let imported_cache = program.imported_runtime_function(&runtime, cache);
            let imported_get = program.imported_runtime_function(&runtime, get);
            assert_eq!(
                program.function_type(imported_cache),
                runtime.function_type(cache)
            );
            assert_eq!(
                program.function_type(imported_get),
                runtime.function_type(get)
            );
            let bodies = module_bodies(&artifact.bytes);
            for name in ["literal", "computed"] {
                let variants = bodies
                    .iter()
                    .filter(|body| body.name.starts_with(&format!("js::{name}#")))
                    .collect::<Vec<_>>();
                assert!(!variants.is_empty());
                for variant in variants {
                    let (_, body) = program.named(&variant.name);
                    if name == "literal" {
                        assert!(calls(body, imported_cache));
                    } else {
                        assert_direct_function_header(body, function, object);
                        assert!(!calls(body, imported_cache));
                        assert!(
                            calls(body, imported_get),
                            "computed misses keep full GetV semantics"
                        );
                    }
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
