//! Equal ordered data layouts share metadata across source bodies.
use crate::{linked_bodies::module_bodies, property_read_plans::Module};
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::lower;
use wasmparser::{HeapType, Operator, UnpackedIndex, ValType, Validator, WasmFeatures};

fn shape_slot(body: &[Operator<'_>]) -> i32 {
    let mut sites = Vec::new();
    // Trace the real paged-table id operand back to its emitted constant.
    for (index, ops) in body.windows(3).enumerate() {
        let [
            Operator::LocalGet { local_index },
            Operator::I32Const { value: 6 },
            Operator::I32ShrU,
        ] = ops
        else {
            continue;
        };
        let site = body[..index]
            .windows(2)
            .rev()
            .find_map(|ops| match ops {
                [
                    Operator::I32Const { value },
                    Operator::LocalSet {
                        local_index: target,
                    },
                ] if target == local_index => Some(*value),
                _ => None,
            })
            .expect("the real table id has a constant definition");
        sites.push(site);
    }
    assert_eq!(sites.len(), 1, "one actual birth-layout table lookup");
    sites[0]
}

fn assert_layout_slots() {
    let source = r#"
function one(value) { return {x:value,y:2}; }
function two(value) { return {x:value,y:3}; }
function reversed(value) { return {y:4,x:value}; }
function changed(value) { return {x:value,z:5}; }
function wide(value) { return {x:value,y:6,z:7}; }
globalThis.layouts=[one,two,reversed,changed,wide];
"#;
    let parsed = parse(source, ParseOptions::script()).unwrap();
    let artifact = scriptw_aot_wasm::emit(&lower(&parsed)).unwrap();
    for bytes in [
        artifact.bytes.as_slice(),
        artifact.runtime().unwrap().bytes(),
    ] {
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(bytes)
            .unwrap();
    }
    let program = Module::read(&artifact.bytes, true);
    let runtime = Module::read(artifact.runtime().unwrap().bytes(), false);
    let (allocate, _) = runtime.named("helper::compact_literal_allocate");
    let imported = program.imported_runtime_function(&runtime, allocate);
    let ValType::Ref(shape_parameter) = program.function_type(imported).params()[3] else {
        panic!("the actual allocator accepts a typed shape");
    };
    let HeapType::Concrete(UnpackedIndex::Module(shape)) = shape_parameter.heap_type() else {
        panic!("shape parameter retains its concrete P type");
    };
    let bodies = module_bodies(&artifact.bytes);
    let slots = |name: &str| {
        let entries = bodies
            .iter()
            .filter(|body| body.name.starts_with(&format!("js::{name}#")))
            .map(|body| {
                let (_, code) = program.named(&body.name);
                let births = code
                    .iter()
                    .filter(|op| {
                        matches!(op,
                    Operator::StructNew { struct_type_index } if *struct_type_index == shape)
                    })
                    .count();
                assert_eq!(births, 1, "the real allocator's shape is constructed");
                shape_slot(code)
            })
            .collect::<Vec<_>>();
        assert!(
            !entries.is_empty(),
            "all actual source variants are checked"
        );
        assert!(entries.iter().all(|slot| *slot == entries[0]));
        entries[0]
    };
    let first = slots("one");
    assert_eq!(
        first,
        slots("two"),
        "values and sites do not fragment identity"
    );
    let mut distinct = vec![first, slots("reversed"), slots("changed"), slots("wide")];
    distinct.sort();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        4,
        "order, key names and width remain distinct"
    );
}

#[test]
fn equal_ordered_literal_keys_share_the_actual_program_shape_table_slot() {
    std::thread::Builder::new()
        .name("compact-birth-layouts".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(assert_layout_slots)
        .unwrap()
        .join()
        .unwrap();
}
