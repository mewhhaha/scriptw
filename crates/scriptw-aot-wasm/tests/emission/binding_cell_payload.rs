use crate::property_read_plans::Module;
use scriptw_aot_wasm::{emit, GcHostField};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{
    CompositeInnerType, HeapType, Operator, UnpackedIndex, ValType, Validator, WasmFeatures,
};

#[test]
fn captured_writes_publish_value_words_without_allocating_a_snapshot() {
    std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
        let ir = lower(&parse("export function make() { let value = 0; return function update(next) { value = next; return value; }; }", ParseOptions::module()).unwrap());
        assert!(ir.is_wasm_supported());
        let artifact = emit(&ir).unwrap();
        let runtime = artifact.runtime().unwrap();
        for bytes in [artifact.bytes.as_slice(), runtime.bytes()] {
            Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
        }
        let p = Module::read(&artifact.bytes, true);
        let r = Module::read(runtime.bytes(), true);
        assert_eq!(&p.types[..r.types.len()], &r.types);
        let (get, _) = r.named("helper::environment_identifier_get_strict");
        let ValType::Ref(cell) = r.function_type(get).params()[4] else { panic!("actual cell reference"); };
        let HeapType::Concrete(UnpackedIndex::Module(cell)) = cell.heap_type() else { panic!("canonical cell type"); };
        let CompositeInnerType::Struct(cell_type) = &r.types[cell as usize].composite_type.inner else { panic!("cell struct"); };
        let fields = [GcHostField::BindingCellScalar, GcHostField::BindingCellTag, GcHostField::BindingCellReference];
        assert!(fields.iter().all(|f| cell_type.fields[f.definition().index() as usize].mutable));
        let (_, body) = p.named("js::update#");
        for field in fields {
            assert!(body.iter().any(|op| matches!(op, Operator::StructSet {struct_type_index, field_index} if *struct_type_index == cell && *field_index == field.definition().index())));
        }
        let scalar = fields[0].definition().index();
        let tag = fields[1].definition().index();
        let reference = fields[2].definition().index();
        // Non-null cell loads insert a type refinement before each field access.
        let writes: Vec<_> = body.iter().filter(|op| !matches!(op, Operator::RefAsNonNull)).cloned().collect();
        assert!(writes.windows(9).any(|ops| {
            let [Operator::LocalGet {local_index: a}, Operator::LocalGet {..},
                 Operator::StructSet {struct_type_index: sa, field_index: fa},
                 Operator::LocalGet {local_index: b}, Operator::LocalGet {..},
                 Operator::StructSet {struct_type_index: sb, field_index: fb},
                 Operator::LocalGet {local_index: c}, Operator::LocalGet {..},
                 Operator::StructSet {struct_type_index: sc, field_index: fc}] = ops else { return false; };
            a == b && b == c && *sa == cell && *sb == cell && *sc == cell
                && *fa == scalar && *fb == tag && *fc == reference
        }), "the canonical cell write publishes all three words with no allocation or callback; other function work may allocate");
    }).unwrap().join().unwrap();
}

/// Declaration instantiation initializes `var`, top-level function and
/// expression-free parameter cells before any closure exists or any body
/// statement runs, so their reads have no TDZ branch. A `let` and a
/// parameter of a list with expressions keep it.
#[test]
fn instantiated_cells_omit_tdz_while_lexical_and_expression_parameter_cells_keep_it() {
    std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
        let ir = lower(&parse(r#"
            export function make(simple) {
                var hoisted = 1;
                let lexical = 2;
                function declared() { return 0; }
                return [
                    function readVar() { return hoisted; },
                    function readLet() { return lexical; },
                    function readParam() { return simple; },
                    function readDeclared() { return declared; },
                ];
            }
            export function own(parameter) {
                var local = parameter;
                function hold() { return parameter + local; }
                return [hold, parameter + local];
            }
            export function withDefault(first = 0, later) {
                return function readLater() { return later; };
            }
        "#, ParseOptions::module()).unwrap());
        assert!(ir.is_wasm_supported(), "{:?}", ir.diagnostics);
        let artifact = emit(&ir).unwrap();
        let runtime = artifact.runtime().unwrap();
        for bytes in [artifact.bytes.as_slice(), runtime.bytes()] {
            Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
        }
        let p = Module::read(&artifact.bytes, true);
        let r = Module::read(runtime.bytes(), true);
        let (definition, _) = r.named("helper::lexical_binding_uninitialized");
        let tdz = p.imported_runtime_function(&r, definition);
        let calls = |name: &str| {
            p.named(name).1.iter()
                .filter(|op| matches!(op, Operator::Call { function_index } if *function_index == tdz))
                .count()
        };
        for name in ["js::readVar#", "js::readParam#", "js::readDeclared#", "js::hold#", "js::own#"] {
            assert_eq!(calls(name), 0, "{name}: an instantiated cell has no TDZ branch");
        }
        assert_eq!(calls("js::readLet#"), 1, "a captured let keeps its TDZ branch");
        assert_eq!(calls("js::readLater#"), 1,
            "a parameter of a list with expressions keeps its TDZ branch");
    }).unwrap().join().unwrap();
}
