//! Inspect resolved source reads, their returned constants, and rejected plans.
use super::*;
use scriptw_ir::{GlobalBindingPlan, GlobalPropertyInitializerIr};

fn helper(artifact: &WasmArtifact, requested: &str) -> Option<u32> {
    let runtime = artifact.runtime().unwrap();
    let definition = function_names_in_bytes(runtime.bytes())
        .into_iter()
        .find_map(|(index, name)| (name == requested).then_some(index))
        .unwrap();
    let mut exported = None;
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        if let Payload::ExportSection(section) = payload.unwrap() {
            for export in section {
                let export = export.unwrap();
                if export.kind == wasmparser::ExternalKind::Func && export.index == definition {
                    exported = Some(export.name);
                }
            }
        }
    }
    assert!(exported.is_some(), "R exports {requested}");
    let mut index = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::ImportSection(section) = payload.unwrap() {
            for import in section.into_imports() {
                let import = import.unwrap();
                if matches!(
                    import.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                ) {
                    if import.module == crate::RUNTIME_IMPORT_NAMESPACE
                        && Some(import.name) == exported
                    {
                        return Some(index);
                    }
                    index += 1;
                }
            }
        }
    }
    None
}

fn with_bodies(
    artifact: &WasmArtifact,
    name: &str,
    mut inspect: impl FnMut(&str, &[Operator<'_>]),
) {
    expect_valid_module(artifact, 1);
    let names = function_names(artifact)
        .into_iter()
        .filter(|(_, actual)| actual.starts_with(&format!("js::{name}#")))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        names.len(),
        2,
        "ordinary and exact-context variants: {names:?}"
    );
    let mut index = imported_function_count(artifact);
    let mut checked = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if let Some(name) = names.get(&index) {
                let operators = body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                inspect(name, &operators);
                checked += 1;
            }
            index += 1;
        }
    }
    assert_eq!(checked, names.len());
}

#[derive(Debug)]
struct Lookups {
    global_reads: usize,
    keys: BTreeMap<i32, usize>,
}

fn lookups(artifact: &WasmArtifact, name: &str) -> BTreeMap<String, Lookups> {
    // A global read that is not folded resolves the name at run time through
    // one of the shared global identifier reads.
    let global_reads = [
        "helper::global_identifier_read_sloppy",
        "helper::global_identifier_read_strict",
        "helper::global_identifier_typeof_sloppy",
        "helper::global_identifier_typeof_strict",
    ]
    .into_iter()
    .filter_map(|name| helper(artifact, name))
    .collect::<Vec<_>>();
    let resolve = helper(artifact, "helper::pooled_string_resolve");
    let mut result = BTreeMap::new();
    with_bodies(artifact, name, |variant, operators| {
        let mut lookup = Lookups {
            global_reads: 0,
            keys: BTreeMap::new(),
        };
        for (index, operator) in operators.iter().enumerate() {
            let Operator::Call { function_index } = operator else {
                continue;
            };
            if global_reads.contains(function_index) {
                lookup.global_reads += 1;
            }
            if Some(*function_index) != resolve {
                continue;
            }
            let sequence = &operators[index.saturating_sub(3)..=index];
            let [Operator::I32Const { value }, Operator::LocalSet {
                local_index: stored,
            }, Operator::LocalGet {
                local_index: loaded,
            }, Operator::Call { .. }] = sequence
            else {
                panic!("{variant}: unknown pooled ordinal at {index}: {sequence:?}")
            };
            assert_eq!(stored, loaded);
            *lookup.keys.entry(*value).or_default() += 1;
        }
        result.insert(variant.to_owned(), lookup);
    });
    result
}

fn replace_initializer(
    script: &mut ScriptIr,
    name: &str,
    replacement: Option<GlobalPropertyInitializerIr>,
) {
    let mut bindings = script.global_bindings.iter().cloned().collect::<Vec<_>>();
    if let Some(initializer) = replacement {
        bindings
            .iter_mut()
            .find(|binding| binding.name == name)
            .unwrap()
            .initializer = initializer;
    } else {
        bindings.retain(|binding| binding.name != name);
    }
    script.global_bindings = GlobalBindingPlan::from_parts(
        bindings,
        script
            .global_bindings
            .lexical_bindings()
            .iter()
            .map(|(name, mode)| (name.clone(), *mode)),
    );
}

fn canonical_and_writable(source: &str, name: &str) -> (WasmArtifact, WasmArtifact) {
    let source = parse(source, ParseOptions::script()).unwrap();
    let mut program = lower_with_host_surface_policy(&source, HostSurfacePolicy::Test262);
    let canonical = emit(&program).unwrap();
    // Only initializer authority changes. Source References and the names in
    // the shared R pool stay identical; the control must retain the resolver.
    replace_initializer(
        program.script.as_mut().unwrap(),
        name,
        Some(GlobalPropertyInitializerIr::FreshUndefined),
    );
    let writable = emit(&program).unwrap();
    assert_eq!(
        canonical.runtime().unwrap().key(),
        writable.runtime().unwrap().key()
    );
    (canonical, writable)
}

fn check_lookup_elimination(canonical: &WasmArtifact, writable: &WasmArtifact, name: &str) {
    let mut controls = lookups(writable, name);
    for (variant, folded) in lookups(canonical, name) {
        let control = controls
            .remove(&variant)
            .expect("same physical source variant");
        assert_eq!(
            control.global_reads,
            folded.global_reads + 1,
            "exactly the admitted global read disappears in {variant}: {control:?} / {folded:?}"
        );
        // Removing the resolver also removes its lexical-TDZ diagnostic string.
        // Retained keys must keep their exact call counts; at least the source
        // key disappears, with no ordinal assumptions about the shared pool.
        assert!(
            folded.keys.len() < control.keys.len(),
            "source key work disappears in {variant}: {control:?} / {folded:?}"
        );
        for (key, count) in &folded.keys {
            assert_eq!(
                control.keys.get(key),
                Some(count),
                "retained key {key} has unchanged calls in {variant}: {control:?} / {folded:?}"
            );
        }
    }
    assert!(controls.is_empty());
}

fn check_returned_constant(variant: &str, operators: &[Operator<'_>], tag: i32, bits: i64) {
    // The final five-result ABI emission identifies actual completion locals,
    // independently of local allocation or unrelated prologue instructions.
    let outputs = operators
        .windows(5)
        .rev()
        .find_map(|ops| match ops {
            [Operator::LocalGet { local_index: tag }, Operator::LocalGet {
                local_index: scalar,
            }, Operator::LocalGet {
                local_index: reference,
            }, Operator::LocalGet { local_index: kind }, Operator::LocalGet { .. }] => {
                Some([*tag, *scalar, *reference, *kind])
            }
            _ => None,
        })
        .expect("source body emits the completion ABI");
    let returns = operators
        .windows(2)
        .enumerate()
        .filter_map(|(index, ops)| match ops {
            [Operator::I32Const { value }, Operator::LocalSet { local_index }]
                if *value == crate::COMPLETION_KIND_RETURN as i32 && *local_index == outputs[3] =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        returns.len(),
        1,
        "one source return in {variant}: {returns:?}"
    );
    let (copy, inputs) = operators[..returns[0]]
        .windows(6)
        .enumerate()
        .rev()
        .find_map(|(index, ops)| match ops {
            [Operator::LocalGet { local_index: tag }, Operator::LocalSet {
                local_index: out_tag,
            }, Operator::LocalGet {
                local_index: scalar,
            }, Operator::LocalSet {
                local_index: out_scalar,
            }, Operator::LocalGet {
                local_index: reference,
            }, Operator::LocalSet {
                local_index: out_reference,
            }] if [*out_tag, *out_scalar, *out_reference] == outputs[..3] => {
                Some((index, [*tag, *scalar, *reference]))
            }
            _ => None,
        })
        .expect("source return copies its value into the completion");
    for (part, local) in inputs.into_iter().enumerate() {
        let store = operators[..copy].iter().rposition(|op| matches!(op,
            Operator::LocalSet { local_index } | Operator::LocalTee { local_index } if *local_index == local))
            .expect("returned value has an actual defining instruction");
        let definition = &operators[store - 1];
        let correct = match (part, definition) {
            (0, Operator::I32Const { value }) => *value == tag,
            (1, Operator::I64Const { value }) => *value == bits,
            (2, Operator::RefNull { .. }) => true,
            _ => false,
        };
        assert!(correct, "{variant}: returned part {part} is {definition:?}, expected tag {tag}, bits {bits}, null reference");
    }
}

#[test]
fn canonical_primitive_reads_return_constants_without_global_lookup() {
    for (name, tag, bits) in [
        ("undefined", WasmRuntimeValueTag::Undefined, 0),
        (
            "NaN",
            WasmRuntimeValueTag::Number,
            f64::NAN.to_bits() as i64,
        ),
        (
            "Infinity",
            WasmRuntimeValueTag::Number,
            f64::INFINITY.to_bits() as i64,
        ),
    ] {
        let (canonical, writable) =
            canonical_and_writable(&format!("function read(){{return {name};}} read();"), name);
        check_lookup_elimination(&canonical, &writable, "read");
        for (variant, lookup) in lookups(&canonical, "read") {
            assert_eq!(lookup.global_reads, 0, "no global lookup in {variant}");
        }
        with_bodies(&canonical, "read", |variant, operators| {
            check_returned_constant(variant, operators, tag as i32, bits);
        });
    }
}

#[test]
fn mixed_named_function_environment_reaches_the_constant_global_read() {
    let (canonical, writable) = canonical_and_writable(
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/join-mixed-1024.js"
        ),
        "undefined",
    );
    check_lookup_elimination(&canonical, &writable, "workload");
}

#[test]
fn absent_writable_function_and_mismatched_initializer_plans_decline() {
    for replacement in 0..4 {
        let source = parse(
            "function read(){return undefined;} read();",
            ParseOptions::script(),
        )
        .unwrap();
        let mut program = lower_with_host_surface_policy(&source, HostSurfacePolicy::Test262);
        let script = program.script.as_mut().unwrap();
        let function = script
            .functions
            .iter()
            .find(|function| function.name == "read")
            .unwrap()
            .id
            .clone();
        let initializer = match replacement {
            0 => None,
            1 => Some(GlobalPropertyInitializerIr::FreshUndefined),
            2 => Some(GlobalPropertyInitializerIr::SourceFunction(function)),
            3 => Some(GlobalPropertyInitializerIr::Infinity),
            _ => unreachable!(),
        };
        replace_initializer(script, "undefined", initializer);
        let artifact = emit(&program).expect("noncanonical initializer control emits");
        for (variant, lookup) in lookups(&artifact, "read") {
            assert_eq!(
                lookup.global_reads, 1,
                "initializer {replacement} retains the resolver in {variant}"
            );
            assert!(
                !lookup.keys.is_empty(),
                "initializer {replacement} retains source key resolution in {variant}"
            );
        }
    }
}
