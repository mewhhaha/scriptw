//! When a Store's first nursery (2 MiB of the 16 MiB heap) is full, it goes on
//! allocating in the old space (the vendored collector's pretenure window,
//! about 3 MiB) and its first collection is a full one. Nothing here calls
//! `gc()` before its checks: the collections are the ones allocation asks for.
use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn run(source: &str) -> Vec<HostOutputEvent> {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let outcome = Engine::new(RealmBuilder::new().build())
        .observe_script(
            source,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(60_000),
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        outcome.completion
    );
    outcome.output_events
}

/// Garbage of several times the window and the nursery together.
const CHURN: &str =
    "function churn() { for (let j = 0; j < 400000; j++) ({garbage: [j, 'text' + j]}); }";

#[test]
fn a_graph_larger_than_the_window_keeps_every_edge_through_natural_collections() {
    let events = run(&format!(
        r#"{CHURN}
        // About 6 MiB retained: the first nodes fill the first nursery, the
        // next the window, and later ones are allocated young and promoted.
        const COUNT = 60000, nodes = [];
        for (let i = 0; i < COUNT; i++) {{
            nodes.push({{i, previous: i ? nodes[i - 1] : null, payload: [i, 'n' + i, {{i}}], late: null}});
        }}
        function check(label) {{
            for (let i = 0; i < COUNT; i++) {{
                const node = nodes[i];
                if (node.i !== i || node.payload[0] !== i || node.payload[1] !== 'n' + i || node.payload[2].i !== i) throw label + ' node ' + i;
                if (i && node.previous !== nodes[i - 1]) throw label + ' previous ' + i;
                if (node.late !== null && (node.late.owner !== node || node.late.value !== -i)) throw label + ' late ' + i;
            }}
        }}
        check('built');
        // New objects stored into the oldest and the newest nodes, by
        // property writes, array element writes and array methods.
        const lates = [];
        for (let i = 0; i < COUNT; i += 7) {{
            const node = nodes[i]; node.late = {{owner: node, value: -i}};
            node.payload[2] = {{i}}; node.payload.push({{extra: i}}); lates.push(node.late);
        }}
        churn(); check('stored');
        for (let i = 0; i < COUNT; i += 7) if (nodes[i].payload[3].extra !== i || lates[i / 7] !== nodes[i].late) throw 'extra ' + i;
        nodes.copyWithin(0, 0); nodes.reverse(); nodes.reverse();
        churn(); check('end'); gc(); check('full');
        print('graph:ok');
    "#
    ));
    assert_eq!(events, vec![HostOutputEvent::PrintLine("graph:ok".into())]);
}

#[test]
fn window_objects_die_at_the_first_collection_and_are_finalized() {
    let events = run(&format!(
        r#"{CHURN}
        let cleaned = 0;
        const registry = new FinalizationRegistry(function(holding) {{
            cleaned += holding; if (cleaned === 136) print('finalized');
        }});
        const map = new WeakMap(), kept = {{name: 'kept'}}, weak = [];
        let keptValue;
        // Past the first nursery: targets, keys and values in the window,
        // which the bootstrap and this loop leave open.
        for (let j = 0; j < 20000; j++) ({{garbage: [j, 'text' + j]}});
        (function() {{
            for (let i = 1; i <= 16; i++) {{
                const target = {{i}}; registry.register(target, i); weak.push(new WeakRef(target));
                map.set(target, {{value: i}});
            }}
            map.set(kept, {{value: 'kept'}}); keptValue = new WeakRef(map.get(kept));
        }})();
        Promise.resolve().then(function() {{
            churn();
            for (const reference of weak) if (reference.deref() !== undefined) throw 'a dead window object survived';
            if (keptValue.deref() !== map.get(kept) || map.get(kept).value !== 'kept') throw 'live ephemeron value';
            // A key allocated young for a value held only by the entry.
            const young = {{}}; map.set(young, {{value: 'young'}}); map.set(kept, {{value: 'again'}});
            churn();
            if (map.get(young).value !== 'young' || map.get(kept).value !== 'again') throw 'young entries';
            print('observed');
        }});
    "#
    ));
    // Cleanup is a job of its own, queued by the collection that found the
    // targets dead: its order against the promise jobs is the host's.
    let mut events = events;
    events.sort_by_key(|event| format!("{event:?}"));
    assert_eq!(
        events,
        vec![
            HostOutputEvent::PrintLine("finalized".into()),
            HostOutputEvent::PrintLine("observed".into())
        ]
    );
}
