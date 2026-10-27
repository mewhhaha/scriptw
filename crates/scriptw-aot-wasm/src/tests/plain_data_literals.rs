//! Actual source-body call edges prove which literal definition path is used.
use super::*;

fn source_body_definition_calls(expression: &str) -> (usize, usize, usize) {
    let artifact = emit_script(&format!(
        // Ordinary initializers use the shared literal emitter. Directly
        // returned spread literals have a separate frontend staging path.
        "function make(input) {{ var result = {expression}; return result; }} make(globalThis.input);"
    ))
    .expect("literal control emits");
    expect_valid_module(&artifact, 1);
    let runtime = artifact.runtime().expect("literal links R");
    let runtime_names = function_names_in_bytes(runtime.bytes());
    let mut runtime_exports = BTreeMap::new();
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        if let Payload::ExportSection(exports) = payload.expect("runtime parses") {
            for export in exports {
                let export = export.expect("runtime export parses");
                if export.kind == wasmparser::ExternalKind::Func {
                    runtime_exports.insert(export.name.to_owned(), export.index);
                }
            }
        }
    }
    let mut program_imports = BTreeMap::new();
    let mut next_import = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::ImportSection(imports) = payload.expect("program parses") {
            for import in imports.into_imports() {
                let import = import.expect("program import parses");
                if matches!(
                    import.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                ) {
                    program_imports.insert(
                        next_import,
                        (import.module.to_owned(), import.name.to_owned()),
                    );
                    next_import += 1;
                }
            }
        }
    }
    let helper_index = |name: &str| {
        let definitions = runtime_names
            .iter()
            .filter_map(|(&index, actual)| (actual == name).then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(definitions.len(), 1, "one runtime definition of {name}");
        let exports = runtime_exports
            .iter()
            .filter_map(|(name, &index)| {
                (index == definitions[0]
                    && name
                        .strip_prefix('f')
                        .is_some_and(|ordinal| ordinal.parse::<u32>().is_ok()))
                .then_some(name)
            })
            .collect::<Vec<_>>();
        assert_eq!(exports.len(), 1, "one R function export for {name}");
        let imported = program_imports
            .iter()
            .filter_map(|(&index, (namespace, export))| {
                (namespace == crate::RUNTIME_IMPORT_NAMESPACE && export == exports[0])
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        assert!(
            imported.len() <= 1,
            "P imports the actual runtime definition of {name} at most once"
        );
        // Import compaction removes an unused helper. The per-case exact edge
        // assertions below still require its import whenever calls are expected.
        imported.first().copied()
    };
    let append = helper_index("helper::ordinary_property_append");
    let define = helper_index("helper::object_define_data");
    let compact = helper_index("helper::compact_literal_allocate");
    let make = function_names(&artifact)
        .into_iter()
        .find_map(|(index, name)| name.starts_with("js::make#").then_some(index))
        .expect("source body has its actual Wasm name");
    let mut index = imported_function_count(&artifact);
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("module parses") else {
            continue;
        };
        if index == make {
            let mut edges = (0, 0, 0);
            for operator in body.get_operators_reader().expect("operators parse") {
                if let Operator::Call { function_index } = operator.expect("operator parses") {
                    edges.0 += usize::from(Some(function_index) == append);
                    edges.1 += usize::from(Some(function_index) == define);
                    edges.2 += usize::from(Some(function_index) == compact);
                }
            }
            return edges;
        }
        index += 1;
    }
    panic!("source body is absent");
}

fn literal_definition_calls(
    expression: &str,
    control: (usize, usize, usize),
) -> (usize, usize, usize) {
    let actual = source_body_definition_calls(expression);
    // The ordinary source body can also publish implicit arguments metadata.
    // Its otherwise identical control owns those edges independently of the
    // literal path; count only the extra calls supplied by the literal itself.
    (
        actual
            .0
            .checked_sub(control.0)
            .expect("literal keeps ordinary-body append plumbing"),
        actual
            .1
            .checked_sub(control.1)
            .expect("literal keeps ordinary-body define plumbing"),
        actual
            .2
            .checked_sub(control.2)
            .expect("literal keeps ordinary-body compact plumbing"),
    )
}

#[test]
fn bounded_static_data_literals_reach_the_compact_allocator() {
    let control = source_body_definition_calls("input");
    assert_eq!(
        literal_definition_calls("({first: input, last: input})", control),
        (0, 0, 1)
    );
    assert_eq!(literal_definition_calls("({input})", control), (0, 0, 1));
    assert_eq!(literal_definition_calls("({})", control), (0, 0, 0));
    let sixteen = (0..16)
        .map(|index| format!("key{index}: input"))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(
        literal_definition_calls(&format!("({{{sixteen}}})"), control),
        (0, 0, 1)
    );
    let seventeen = format!("{sixteen},last:input");
    assert_eq!(
        literal_definition_calls(&format!("({{{seventeen}}})"), control),
        (17, 0, 0)
    );
}

#[test]
fn special_and_duplicate_literal_properties_keep_generic_definition() {
    let control = source_body_definition_calls("input");
    let mut unexpected = Vec::new();
    for (expression, callable_control) in [
        ("({first: input, first: 2})", None),
        ("({first: input, [globalThis.key]: 2})", None),
        ("({first: input, ...input})", None),
        (
            "({first: input, get last() { return 2; }})",
            Some("({get last() { return 2; }})"),
        ),
        (
            "({first: input, set last(value) {}})",
            Some("({set last(value) {}})"),
        ),
        (
            "({first: input, method() { return 2; }})",
            Some("({method() { return 2; }})"),
        ),
        ("({first: input, __proto__: null})", None),
    ] {
        // Creating the nested callable also publishes its own metadata. Match
        // that callable in the control, so its append calls are not attributed
        // to the ordinary data field's literal construction path.
        let control = callable_control.map_or(control, source_body_definition_calls);
        let edges = literal_definition_calls(expression, control);
        // Spread also emits a generic definition inside its property loop;
        // source-body call sites do not count runtime property definitions.
        if edges.0 != 0 || edges.1 == 0 || edges.2 != 0 {
            unexpected.push(format!("{expression}: {edges:?}"));
        }
    }
    let fields = (0..65)
        .map(|index| format!("key{index}: input"))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(
        literal_definition_calls(&format!("({{{fields}}})"), control),
        (0, 65, 0)
    );
    assert!(
        unexpected.is_empty(),
        "ordinary data fields must retain generic definition without direct append: {unexpected:?}"
    );
}
