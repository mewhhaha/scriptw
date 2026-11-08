use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_cells(body: &str) {
    let source = format!(
        "function assert(ok) {{ if (!ok) throw new Error('binding cell'); }}\n{body}\nprint('ok');"
    );
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let result = Engine::new(RealmBuilder::new().build())
        .observe_script(
            &source,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("cell fixture compiles and executes through Wasm AOT");
    assert_eq!(result.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(result.completion, ObservedCompletion::Normal(_)),
        "{:?}\n{source}",
        result.completion
    );
    assert_eq!(
        result.output_events,
        vec![HostOutputEvent::PrintLine("ok".into())]
    );
}

#[test]
fn captured_value_changes_keep_mapped_arguments_and_detached_snapshots_distinct() {
    assert_cells(
        r#"
function make(value) {
  return {read: () => value, write: next => { value = next; }, args: arguments};
}
var first = {id: 1}, second = {id: 2}, state = make(first);
state.args[0] = second;
gc();
assert(state.read() === second);
state.write(19);
assert(state.args[0] === 19);
Object.defineProperty(state.args, '0', {writable: false});
state.write(first);
assert(state.read() === first && state.args[0] === 19);
var symbol = Symbol('cell');
for (var next of [undefined, null, true, -0, NaN, 8n, symbol, second, 'text']) {
  state.write(next);
  gc();
  assert(Object.is(state.read(), next) && state.args[0] === 19);
}
assert(first.id === 1 && second.id === 2);
"#,
    );
}

#[test]
fn suspended_closures_keep_values_and_abrupt_finally_updates() {
    assert_cells(
        r#"
function* make() {
  let value = {id: 1};
  const read = () => value;
  try {
    yield [read, value];
    value = 23;
    yield read;
    value = {id: 2};
    yield value;
  } finally { value = null; }
  return read;
}
var g = make(), first = g.next().value, read = first[0];
gc();
assert(read() === first[1]);
assert(g.next().value === read && read() === 23 && first[1].id === 1);
var second = g.next().value;
gc();
assert(read() === second && second.id === 2);
g.return('closed');
gc();
assert(read() === null && second.id === 2);
"#,
    );
}

#[test]
fn per_iteration_cells_preserve_tdz_const_and_distinct_capture_ownership() {
    assert_cells(
        r#"
var reads = [];
for (let i = 0; i < 4; i++) {
  let value = {i};
  reads.push(() => value);
  value = i * 2;
}
assert(reads.map(fn => fn()).join(',') === '0,2,4,6');
let observed = '';
try { const read = () => later; read(); let later = 1; }
catch (error) { observed = error.name; }
assert(observed === 'ReferenceError');
const immutable = {id: 3};
try { (() => { immutable = 4; })(); }
catch (error) { assert(error instanceof TypeError); }
assert(immutable.id === 3);
"#,
    );
}
