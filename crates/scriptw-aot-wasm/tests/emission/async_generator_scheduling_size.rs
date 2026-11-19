//! Scheduling is shared typed Wasm code rather than copied into every caller.

use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use std::collections::BTreeMap;
use wasmparser::{KnownCustom, Name, Operator, Parser, Payload, TypeRef};

const SCHEDULING_BODY_LIMIT: usize = 512 * 1024;
const BOOTSTRAP_BODY_LIMIT: usize = 3 * 1024 * 1024;
const ANY_BODY_LIMIT: usize = 1024 * 1024;

// Adapted from the emitted/runtime nested Array lifecycle cohort. Ordinary,
// Return and Throw requests all pass through a suspended default and a finally
// that both awaits and yields. No runtime or test-only host surface is needed.
const SOURCE: &str = r#"
function check(ok) { if (!ok) throw new Error('nested Array scheduling'); }
const whole = {marker: 83};
// Exercise the real public object operations as well as the iterator/request
// callers, so every shared operation has an actual non-self consumer.
const reflectionParent = {}, reflectionTarget = Object.create(reflectionParent);
check(Reflect.get(reflectionTarget, 'missing') === undefined);
check(Reflect.getPrototypeOf(reflectionTarget) === reflectionParent);
check(Reflect.setPrototypeOf(reflectionTarget, null));
reflectionTarget.marker = 17;
check(Reflect.deleteProperty(reflectionTarget, 'marker'));
check(Reflect.get(reflectionTarget, 'marker') === undefined);
check(Reflect.isExtensible(reflectionTarget));
check(Reflect.preventExtensions(reflectionTarget));
check(!Reflect.isExtensible(reflectionTarget));
function source(label, first, events) {
    let reads = 0, calls = 0;
    const iterator = {
        get next() {
            reads++;
            return function () {
                check(this === iterator); calls++;
                return {done: false, value: first};
            };
        },
        get return() {
            events.push('get:' + label);
            return function () {
                check(this === iterator); events.push('call:' + label);
                return {get then() { throw 'synchronous-close-must-not-await'; }};
            };
        }
    };
    return {input: {[Symbol.iterator]: function () { return iterator; }},
        reads: function () { return reads; }, calls: function () { return calls; }};
}
async function run(mode) {
    const events = [], inner = source('inner', undefined, events);
    const outer = source('outer', inner.input, events);
    async function* values() {
        try {
            const [[value = await (yield 'default')]] = outer.input;
            yield value;
        } finally { await Promise.resolve(0); yield 'finally'; }
        return whole;
    }
    const iterator = values();
    check((await iterator.next()).value === 'default');
    check(inner.reads() === 1 && outer.reads() === 1 && events.length === 0);
    if (mode === 'normal') {
        const pending = iterator.next(7), queued = iterator.next();
        check((await pending).value === 7);
        check((await queued).value === 'finally');
        const done = await iterator.next(); check(done.done && done.value === whole);
    } else {
        const pending = mode === 'return' ? iterator.return(whole) : iterator.throw(whole);
        const queued = iterator.next().then(function (result) { return {result: result}; },
            function (error) { return {error: error}; });
        const finalizer = await pending;
        check(!finalizer.done && finalizer.value === 'finally');
        const done = await queued;
        if (mode === 'return') check(done.result.done && done.result.value === whole);
        else check(done.error === whole);
        check((await iterator.next()).done);
    }
    check(events.join(',') === 'get:inner,call:inner,get:outer,call:outer');
    check(inner.calls() === 1 && outer.calls() === 1);
}
async function all() { await run('normal'); await run('return'); await run('throw'); }
all();
"#;

struct EncodedBody {
    bytes: usize,
    calls: Vec<(usize, u32)>,
}

#[test]
fn nested_array_requests_share_bounded_async_generator_scheduling_helpers() {
    // Match other product-artifact controls: enough Rust stack for recursive
    // compiler emission, without executing the resulting Wasm.
    let artifact = std::thread::Builder::new()
        .name("async-generator-scheduling-size".to_owned())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let parsed = parse(SOURCE, ParseOptions::script()).expect("lifecycle source parses");
            let program = lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            scriptw_aot_wasm::emit(&program)
                .expect("nested Array and queued completions emit through the product compiler")
        })
        .expect("compiler worker starts")
        .join()
        .expect("compiler worker completes");

    let mut names = BTreeMap::<(usize, u32), String>::new();
    let mut bodies = BTreeMap::<(usize, u32), EncodedBody>::new();
    let runtime = artifact.runtime().expect("async scheduling links R");
    let modules = [runtime.bytes(), artifact.bytes.as_slice()];
    let imports = crate::linked_bodies::runtime_function_imports(&artifact.bytes, runtime.bytes());
    for (module, bytes) in modules.into_iter().enumerate() {
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
            .validate_all(bytes)
            .expect("both scheduling modules validate");
        let mut next_function = 0u32;
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.expect("emitted module decodes") {
                Payload::ImportSection(reader) => {
                    for import in reader.into_imports() {
                        if matches!(
                            import.expect("import decodes").ty,
                            TypeRef::Func(_) | TypeRef::FuncExact(_)
                        ) {
                            next_function += 1;
                        }
                    }
                }
                Payload::CodeSectionEntry(body) => {
                    let mut calls = Vec::new();
                    for operator in body.get_operators_reader().expect("body reader opens") {
                        match operator.expect("operator decodes") {
                            Operator::Call { function_index }
                            | Operator::ReturnCall { function_index } => {
                                let target = if module == 1 {
                                    imports
                                        .get(&function_index)
                                        .map(|index| (0, *index))
                                        .unwrap_or((module, function_index))
                                } else {
                                    (module, function_index)
                                };
                                calls.push(target);
                            }
                            _ => {}
                        }
                    }
                    assert!(
                        bodies
                            .insert(
                                (module, next_function),
                                EncodedBody {
                                    bytes: body.range().len(),
                                    calls
                                },
                            )
                            .is_none(),
                        "each module owns one body per function index"
                    );
                    next_function += 1;
                }
                Payload::CustomSection(section) => {
                    if let KnownCustom::Name(subsections) = section.as_known() {
                        for subsection in subsections {
                            if let Name::Function(map) =
                                subsection.expect("name subsection decodes")
                            {
                                for naming in map {
                                    let naming = naming.expect("function name decodes");
                                    assert!(
                                        names
                                            .insert((module, naming.index), naming.name.to_owned())
                                            .is_none(),
                                        "a function index must have one emitted name"
                                    );
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    eprintln!(
        "scheduling linked artifact: {} bytes, {} code-body bytes",
        modules.iter().map(|bytes| bytes.len()).sum::<usize>(),
        bodies.values().map(|body| body.bytes).sum::<usize>()
    );
    let mut largest_bodies = bodies.iter().collect::<Vec<_>>();
    largest_bodies.sort_unstable_by(|(left_index, left), (right_index, right)| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left_index.cmp(right_index))
    });
    for &(index, body) in largest_bodies.iter().take(12) {
        let name = names.get(index).map(String::as_str).unwrap_or("<unnamed>");
        eprintln!("largest body #{index:?} {name}: {} bytes", body.bytes);
    }
    let scheduling_helpers = [
        "helper::async_generator_start_body",
        "helper::async_generator_drain_queue",
        "helper::promise_drain_jobs",
        "helper::async_await_reactions",
        "helper::async_generator_await_reactions",
        "helper::async_generator_yield_reactions",
        "helper::async_generator_yield_return_reactions",
        "helper::async_generator_await_return_reactions",
    ];
    // Await/queued completion callers also coerce values and keys. These
    // original typed rows must be called rather than copied into each caller.
    let coercion_helpers = [
        "helper::value_to_primitive_default",
        "helper::value_to_primitive_number",
        "helper::value_to_primitive_string",
        "helper::value_to_property_key",
    ];
    let bootstrap_helpers = [
        "helper::function_metadata_publish",
        "helper::ordinary_property_append",
        "helper::ordinary_property_find",
        "helper::object_header_projection",
        "helper::ordinary_object_allocate",
        "helper::value_to_object",
    ];
    let object_helpers = [
        "helper::object_read",
        "helper::object_get_prototype_of",
        "helper::object_set_prototype_of",
        "helper::object_delete",
        "helper::object_is_extensible",
        "helper::object_prevent_extensions",
    ];
    for (&index, name) in &names {
        if name == "scriptw::main"
            || name.starts_with("js::run#")
            || scheduling_helpers.contains(&name.as_str())
            || coercion_helpers.contains(&name.as_str())
            || bootstrap_helpers.contains(&name.as_str())
            || object_helpers.contains(&name.as_str())
            || name == "builtin::Array.fromAsync Fulfilled Function"
        {
            let body = bodies
                .get(&index)
                .expect("named selected function has a code body");
            eprintln!("{name}: {} bytes", body.bytes);
        }
    }
    for (&index, body) in &bodies {
        let name = names.get(&index).map(String::as_str).unwrap_or("<unnamed>");
        assert!(
            body.bytes <= ANY_BODY_LIMIT,
            "{name} (#{index:?}) encoded body is {} bytes; limit is {ANY_BODY_LIMIT}",
            body.bytes
        );
    }
    for helper in scheduling_helpers
        .into_iter()
        .chain(coercion_helpers)
        .chain(bootstrap_helpers)
        .chain(object_helpers)
    {
        let indices = names
            .iter()
            .filter_map(|(&index, name)| (name == helper).then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(
            indices.len(),
            1,
            "{helper} must have one shared emitted body"
        );
        let index = indices[0];
        let body = bodies
            .get(&index)
            .expect("named shared helper has a code body");
        assert!(
            body.bytes <= SCHEDULING_BODY_LIMIT,
            "{helper} encoded body is {} bytes; limit is {SCHEDULING_BODY_LIMIT}",
            body.bytes
        );
        assert!(
            bodies
                .iter()
                .any(|(&caller, body)| caller != index && body.calls.contains(&index)),
            "{helper} must be consumed by a direct call outside its own body"
        );
    }

    let helper_index = |selected: &str| {
        *names
            .iter()
            .find(|(_, name)| name.as_str() == selected)
            .expect("shared bootstrap helper is named")
            .0
    };
    let initialize = helper_index("helper::realm_initialize_intrinsics");
    let metadata = helper_index("helper::function_metadata_publish");
    let append = helper_index("helper::ordinary_property_append");
    let projection = helper_index("helper::object_header_projection");
    assert!(
        names
            .iter()
            .any(|(index, name)| name == "scriptw::main"
                && bodies[index].calls.contains(&initialize)),
        "main must call the shared Realm bootstrap"
    );
    assert!(
        names.iter().any(
            |(index, name)| name.starts_with("js::") && bodies[index].calls.contains(&metadata)
        ),
        "source-created functions must call the same metadata publisher"
    );
    assert!(
        bodies[&metadata].calls.contains(&append),
        "metadata must use the original shared property append algorithm"
    );
    assert!(
        bodies[&initialize].calls.contains(&metadata),
        "shared Realm bootstrap must publish metadata through the same owner"
    );
    assert!(
        bodies[&initialize].calls.contains(&projection),
        "shared Realm bootstrap installation must call the actual header projection"
    );

    let mut run_bodies = 0;
    let mut bootstrap_bodies = 0;
    for (&index, name) in &names {
        let limit = if name.starts_with("js::run#") {
            run_bodies += 1;
            SCHEDULING_BODY_LIMIT
        } else if name == "scriptw::main" {
            bootstrap_bodies += 1;
            BOOTSTRAP_BODY_LIMIT
        } else {
            continue;
        };
        let body = bodies
            .get(&index)
            .expect("named product function has a code body");
        assert!(
            body.bytes <= limit,
            "{name} encoded body is {} bytes; limit is {limit}",
            body.bytes
        );
    }
    assert!(
        run_bodies > 0,
        "the real request-driving run function must be emitted"
    );
    assert_eq!(
        bootstrap_bodies, 1,
        "one actual product bootstrap must be bounded"
    );
}
