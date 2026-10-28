//! Source read plans retain guarded own-data loads and complete Get fallbacks.
use scriptw_aot_wasm::{emit, RUNTIME_IMPORT_NAMESPACE};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower, ExprIr, PropertyKeyIr, SpecOperationIr, StatementIr, ValueKind};
use std::collections::BTreeMap;
use wasmparser::{ExternalKind, KnownCustom, Name, Operator, Parser, Payload, TypeRef};

#[derive(Default)]
struct Module<'a> {
    names: BTreeMap<u32, String>,
    imports: BTreeMap<u32, (String, String)>,
    exports: BTreeMap<String, u32>,
    bodies: BTreeMap<u32, Vec<Operator<'a>>>,
}

impl<'a> Module<'a> {
    fn read(bytes: &'a [u8], operators: bool) -> Self {
        let mut module = Self::default();
        let mut next_function = 0;
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.expect("property-read module decodes") {
                Payload::ImportSection(reader) => {
                    for import in reader.into_imports() {
                        let import = import.expect("property-read import decodes");
                        if matches!(import.ty, TypeRef::Func(_) | TypeRef::FuncExact(_)) {
                            module.imports.insert(
                                next_function,
                                (import.module.to_owned(), import.name.to_owned()),
                            );
                            next_function += 1;
                        }
                    }
                }
                Payload::ExportSection(reader) => {
                    for export in reader {
                        let export = export.expect("property-read export decodes");
                        if export.kind == ExternalKind::Func {
                            module.exports.insert(export.name.to_owned(), export.index);
                        }
                    }
                }
                Payload::CodeSectionEntry(body) => {
                    let body = if operators {
                        body.get_operators_reader()
                            .expect("property-read function opens")
                            .into_iter()
                            .map(|operator| operator.expect("property-read operator decodes"))
                            .collect()
                    } else {
                        Vec::new()
                    };
                    module.bodies.insert(next_function, body);
                    next_function += 1;
                }
                Payload::CustomSection(section) => {
                    if let KnownCustom::Name(subsections) = section.as_known() {
                        for subsection in subsections {
                            if let Name::Function(map) =
                                subsection.expect("name subsection decodes")
                            {
                                for name in map {
                                    let name = name.expect("property-read function name decodes");
                                    assert!(module
                                        .names
                                        .insert(name.index, name.name.into())
                                        .is_none());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        module
    }

    fn named(&self, prefix: &str) -> (u32, &[Operator<'a>]) {
        let matches = self
            .names
            .iter()
            .filter(|(_, name)| {
                if prefix.ends_with('#') {
                    assert!(
                        prefix.starts_with("js::"),
                        "only an explicit source function prefix may end in '#': {prefix}"
                    );
                    name.starts_with(prefix)
                } else {
                    name.as_str() == prefix
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            1,
            "one encoded body matches {prefix}; matching names: {matches:?}"
        );
        let index = *matches[0].0;
        (
            index,
            self.bodies
                .get(&index)
                .expect("the named function has an actual encoded definition"),
        )
    }

    fn imported_runtime_function(&self, runtime: &Self, definition: u32) -> u32 {
        let matches = self
            .imports
            .iter()
            .filter_map(|(&index, (namespace, name))| {
                (namespace == RUNTIME_IMPORT_NAMESPACE
                    && runtime.exports.get(name) == Some(&definition))
                .then_some(index)
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "P imports the actual runtime definition");
        matches[0]
    }
}

fn count(body: &[Operator<'_>], predicate: impl Fn(&Operator<'_>) -> bool) -> usize {
    body.iter().filter(|operator| predicate(operator)).count()
}

fn calls(body: &[Operator<'_>], target: u32) -> bool {
    body.iter().any(|operator| {
        matches!(operator,
            Operator::Call { function_index } | Operator::ReturnCall { function_index }
                if *function_index == target)
    })
}

fn assert_numeric_probe_before_conversion(body: &[Operator<'_>], conversion: u32) {
    let conversion = body
        .iter()
        .position(|operator| {
            matches!(operator, Operator::Call { function_index } if *function_index == conversion)
        })
        .expect("the original property-key conversion remains available on a miss");
    let prefix = &body[..conversion];
    let cast = prefix
        .iter()
        .position(|operator| matches!(operator, Operator::I64TruncF64U))
        .expect("the admitted numeric key becomes an integer before conversion");
    let guard = &prefix[..cast];
    assert!(
        guard
            .iter()
            .any(|op| matches!(op, Operator::RefTestNonNull { .. }))
            && guard.iter().any(|op| matches!(op, Operator::F64Ge))
            && guard.iter().any(|op| matches!(op, Operator::F64Lt))
            && guard.iter().any(|op| matches!(op, Operator::F64Trunc))
            && guard.iter().any(|op| matches!(op, Operator::F64Eq)),
        "concrete Array and finite integral index guards precede the trapping cast"
    );
    let load = prefix
        .iter()
        .rposition(|op| matches!(op, Operator::ArrayGet { .. }))
        .expect("the source path loads existing packed data before allocating a key");
    assert!(
        load > cast,
        "the guarded index addresses the packed backing"
    );
    assert!(
        prefix[load..]
            .iter()
            .any(|op| matches!(op, Operator::BrIf { .. })),
        "a successful own-data load bypasses property-key conversion"
    );
    assert!(
        !prefix[cast..].iter().any(|op| matches!(
            op,
            Operator::StructNew { .. }
                | Operator::ArrayNew { .. }
                | Operator::ArrayNewDefault { .. }
                | Operator::ArrayNewFixed { .. }
                | Operator::Call { .. }
                | Operator::CallRef { .. }
        )),
        "the packed read arm allocates no descriptors or keys and calls no user code"
    );
}

#[test]
fn own_data_cache_and_numeric_array_reads_keep_guarded_loads_and_get_fallbacks() {
    std::thread::Builder::new()
        .name("property-read-plan-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            // Unknown arguments prevent replacing the loaded value with a
            // constant. Publishing the functions retains one unspecialized
            // body each; controls isolate ordinary function/literal plumbing.
            let source = r#"
function ownCandidate(value) { var record = {x: value}; return record.x; }
function ownControl(value) { var record = {x: value}; return record; }
function indexRead(value) { var array = [value]; return array[0]; }
function arrayControl(value) { var array = [value]; return array; }
function lengthRead(value) { var array = [value]; return array.length; }
function unresolvedRead(record) { return record.x; }
function computedRead(record, key) { return record[key]; }
[ownCandidate, ownControl, indexRead, arrayControl, lengthRead, unresolvedRead, computedRead];
"#;
            let parsed = parse(source, ParseOptions::script()).expect("property fixtures parse");
            let lowered = lower(&parsed);
            let script = lowered
                .script
                .as_ref()
                .expect("property fixtures retain source IR");
            let returned = |name: &str| {
                let function = script
                    .functions
                    .iter()
                    .find(|function| function.name == name)
                    .expect("published source function retains its IR");
                let StatementIr::Return(value) = function
                    .body
                    .statements
                    .last()
                    .expect("source read function returns its loaded value")
                else {
                    panic!("unexpected return staging for {name}: {:?}", function.body);
                };
                value
            };
            assert!(
                matches!(&returned("ownCandidate").expr,
                ExprIr::PropertyRead { target, key: PropertyKeyIr::StaticString(name) }
                    if name == "x" && target.possible_kinds.contains(ValueKind::Object)),
                "shaped own source read remains a real PropertyRead: {:?}",
                returned("ownCandidate")
            );
            assert!(
                matches!(&returned("unresolvedRead").expr,
                ExprIr::SpecOperation { operation: SpecOperationIr::GetV, operands }
                    if operands.len() == 2 && operands[0].possible_kinds.contains(ValueKind::Object)
                    && matches!(&operands[1].expr, ExprIr::String(name) if name == "x")),
                "unresolved static source read takes canonical GetV: {:?}",
                returned("unresolvedRead")
            );
            assert!(
                matches!(&returned("computedRead").expr,
                ExprIr::SpecOperation { operation: SpecOperationIr::GetV, operands }
                    if operands.len() == 2
                    && operands[1].possible_kinds.contains(ValueKind::Number)),
                "the indexed-read control exercises canonical GetV, not only PropertyRead"
            );
            let artifact = emit(&lowered).expect("property fixtures emit compiled Wasm");
            let program = Module::read(&artifact.bytes, true);
            let runtime = Module::read(
                artifact
                    .runtime()
                    .expect("property reads link their runtime")
                    .bytes(),
                true,
            );
            let (definition, dynamic_read) = runtime.named("helper::dynamic_property_read");
            let fallback = program.imported_runtime_function(&runtime, definition);
            let (object_read_definition, _) = runtime.named("helper::object_read");
            let canonical_get = program.imported_runtime_function(&runtime, object_read_definition);
            let (cached_definition, cached_body) = runtime.named("helper::cached_property_read");
            let cache = program.imported_runtime_function(&runtime, cached_definition);
            let (_, own) = program.named("js::ownCandidate#");
            let (_, unresolved) = program.named("js::unresolvedRead#");
            let (_, computed) = program.named("js::computedRead#");
            let (_, own_control) = program.named("js::ownControl#");
            let (_, indexed) = program.named("js::indexRead#");
            let (_, array_control) = program.named("js::arrayControl#");
            let (_, length) = program.named("js::lengthRead#");

            for (name, candidate, control) in [
                ("numeric index", indexed, array_control),
                ("array length", length, array_control),
            ] {
                assert!(
                    calls(candidate, fallback),
                    "{name} reaches the actual imported runtime Get"
                );
                assert!(
                    !calls(control, fallback),
                    "the matching construction control does not perform Get"
                );
            }
            assert!(
                calls(own, cache),
                "shaped PropertyRead reaches the actual cache kernel"
            );
            assert!(
                calls(unresolved, cache),
                "unresolved literal-key GetV reaches the actual cache kernel"
            );
            assert!(
                !calls(own_control, cache),
                "construction alone does not request a read cache"
            );
            assert!(
                !calls(computed, cache) && calls(computed, canonical_get),
                "computed GetV retains the complete canonical operation for guarded misses"
            );
            let (key_conversion, _) = runtime.named("helper::value_to_property_key");
            assert_numeric_probe_before_conversion(
                computed,
                program.imported_runtime_function(&runtime, key_conversion),
            );
            assert_numeric_probe_before_conversion(dynamic_read, key_conversion);
            assert!(
                calls(cached_body, object_read_definition),
                "cache miss retains full Get with its canonical key and explicit receiver"
            );
            assert!(
                count(cached_body, |operator| matches!(
                    operator,
                    Operator::RefTestNonNull { .. }
                )) >= 2,
                "R checks concrete ordinary receiver and compact storage"
            );
            assert!(
                count(cached_body, |operator| matches!(operator, Operator::RefEq)) >= 2,
                "R checks the primitive key and live immutable shape identity"
            );
            assert!(
                count(cached_body, |operator| matches!(
                    operator,
                    Operator::ArrayGet { .. }
                )) >= 2,
                "R loads current scalar/reference values directly from guarded storage"
            );
            for (name, read) in [("numeric index", indexed), ("array length", length)] {
                assert!(
                    !read.iter().any(|operator| matches!(
                        operator,
                        Operator::I64TruncSatF64U | Operator::F64Eq
                    )),
                    "{name} keeps numeric index classification in R"
                );
                assert_eq!(
                    count(read, |operator| matches!(
                        operator,
                        Operator::RefTestNonNull { .. }
                    )),
                    count(array_control, |operator| matches!(
                        operator,
                        Operator::RefTestNonNull { .. }
                    )),
                    "{name} adds no P-side array representation guard"
                );
            }
        })
        .expect("compiler worker starts")
        .join()
        .expect("property-read emission does not panic");
}
