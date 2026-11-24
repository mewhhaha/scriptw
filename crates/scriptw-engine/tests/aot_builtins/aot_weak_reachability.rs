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
                timeout_ms: Some(30_000),
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

#[test]
fn weak_storage_preserves_undefined_presence_symbols_and_callback_mutation() {
    run(r#"
        const key = {}, symbol = Symbol('key'); const map = new WeakMap([[key, undefined], [symbol, 3]]);
        if (!map.has(key) || map.get(key) !== undefined || map.get(symbol) !== 3) throw 'presence';
        let calls = 0;
        if (map.getOrInsertComputed(key, () => { calls++; }) !== undefined || calls) throw 'present callback';
        try { map.getOrInsertComputed(key, null); throw 'callable validation'; } catch (e) { if (!(e instanceof TypeError)) throw e; }
        const other = {};
        if (map.getOrInsertComputed(other, function(k) { if (k !== other) throw 'callback arg'; map.set(k, 7); return 9; }) !== 9 || map.get(other) !== 9) throw 'computed overwrite';
        const set = new WeakSet([symbol, Symbol.iterator, key]);
        if (!set.has(symbol) || !set.delete(key) || set.has(key)) throw 'weak set';
        for (const invalid of [null, 1, 'x', Symbol.for('registered')]) {
            if (map.has(invalid) || map.delete(invalid) || map.get(invalid) !== undefined || set.has(invalid)) throw 'invalid read';
        }
    "#);
}

#[test]
fn weak_ref_keeps_targets_through_construction_and_successful_deref_jobs() {
    let events = run(r#"
        let weak;
        (function() { weak = new WeakRef({marker: 19}); })();
        gc(); if (weak.deref().marker !== 19) throw 'constructor keep';
        Promise.resolve().then(function() {
            gc(); if (weak.deref() !== undefined) throw 'job clear';
            (function() { const target = {marker: 42}; weak = new WeakRef(target); })();
            gc(); if (weak.deref().marker !== 42) throw 'job constructor keep';
            gc(); if (weak.deref().marker !== 42) throw 'deref keep';
        }).then(function() { gc(); if (weak.deref() !== undefined) throw 'next job clear'; print('kept roots:ok'); });
    "#);
    assert_eq!(
        events,
        vec![HostOutputEvent::PrintLine("kept roots:ok".into())]
    );
}

#[test]
fn ephemeron_chains_survive_and_key_value_cycles_are_reclaimed() {
    let events = run(r#"
        const map = new WeakMap(); let live, last, cycle;
        (function() {
            const a = {}, b = {}, c = {marker: 31}; live = a; last = new WeakRef(c);
            map.set(b, c); map.set(a, b);
            const key = {}; map.set(key, {key}); cycle = new WeakRef(key);
            if (map.get(key).key !== key) throw 'cycle storage';
        })();
        Promise.resolve().then(function() {
            gc(); if (last.deref().marker !== 31 || cycle.deref() !== undefined) throw 'fixed point or cycle'; live = undefined;
        }).then(function() { gc(); if (last.deref() !== undefined) throw 'dead ephemeron chain'; print('ephemerons:ok'); });
    "#);
    assert_eq!(
        events,
        vec![HostOutputEvent::PrintLine("ephemerons:ok".into())]
    );
}

#[test]
fn an_unreachable_collection_does_not_retain_values_for_live_keys() {
    let events = run(r#"
        const key = {}; let value;
        (function() { const map = new WeakMap(); const target = {}; map.set(key, target); value = new WeakRef(target); })();
        Promise.resolve().then(function() { gc(); if (value.deref() !== undefined) throw 'dead collection owner'; print('dead owner:ok'); });
    "#);
    assert_eq!(
        events,
        vec![HostOutputEvent::PrintLine("dead owner:ok".into())]
    );
}

#[test]
fn cleanup_is_a_job_and_can_unregister_remaining_cleared_cells() {
    let events = run(r#"
        const token = Symbol('unregister'); let calls = 0;
        const registry = new FinalizationRegistry(function(holding) { 'use strict';
            if (this !== undefined || holding !== 2) throw 'cleanup arguments'; calls++;
            if (!registry.unregister(token)) throw 'unregister cleared cell'; print('cleanup:2');
        });
        (function() { registry.register({}, 1, token); registry.register({}, 2); })();
        gc(); if (calls) throw 'cleanup ran inside job';
        Promise.resolve().then(function() { if (calls) throw 'cleanup jumped queue'; print('promise'); });
    "#);
    assert_eq!(
        events,
        vec![
            HostOutputEvent::PrintLine("promise".into()),
            HostOutputEvent::PrintLine("cleanup:2".into())
        ]
    );
}

#[test]
fn cleanup_throw_is_reported_and_remaining_cells_wait_for_another_job() {
    let events = run(r#"
        const registry = new FinalizationRegistry(function(holding) {
            if (holding === 2) { Promise.resolve().then(() => print('between')); throw new Error('cleanup marker'); }
            print('cleanup:1');
        });
        (function() { registry.register({}, 1); registry.register({}, 2); })(); gc();
    "#);
    assert_eq!(events.len(), 3, "{events:?}");
    assert!(
        matches!(&events[0], HostOutputEvent::ReportedException(message) if message.contains("cleanup marker")),
        "{events:?}"
    );
    assert_eq!(
        events[1..],
        [
            HostOutputEvent::PrintLine("between".into()),
            HostOutputEvent::PrintLine("cleanup:1".into())
        ]
    );
}

#[test]
fn unregister_after_target_collection_prevents_cleanup() {
    let events = run(r#"
        const token = {}; const registry = new FinalizationRegistry(() => { throw 'unexpected cleanup'; });
        let weak;
        (function() { const target = {}; registry.register(target, 42, token); weak = new WeakRef(target); })();
        Promise.resolve().then(function() {
            gc(); if (weak.deref() !== undefined) throw 'target not collected';
            if (!registry.unregister(token) || registry.unregister(token)) throw 'unregister';
            print('unregister:ok');
        });
    "#);
    assert_eq!(
        events,
        vec![HostOutputEvent::PrintLine("unregister:ok".into())]
    );
}

#[test]
fn pending_reaction_closures_keep_cycles_until_settlement_and_release_them_afterward() {
    for directive in ["", "\"use strict\";\n"] {
        let events = run(&format!(
            r#"{directive}
                let weak, settle;
                (function() {{
                    const target = {{marker: 41, text: 'x\ud800\udfff'}};
                    target.self = target;
                    weak = new WeakRef(target);
                    const pending = new Promise(function(resolve) {{ settle = resolve; }});
                    pending.then(function(value) {{
                        gc();
                        if (value !== 19 || target.marker !== 41 || target.self !== target ||
                            target.text.length !== 3 || target.text.charCodeAt(1) !== 0xd800 ||
                            target.text.charCodeAt(2) !== 0xdfff) throw 'reaction roots';
                        print('reaction:ok');
                    }});
                }})();
                Promise.resolve().then(function() {{
                    gc();
                    const target = weak.deref();
                    if (target === undefined || target.marker !== 41 || target.self !== target)
                        throw 'pending reaction lost its cycle';
                    settle(19);
                    settle = undefined;
                }}).then(function() {{
                    gc();
                    if (weak.deref() !== undefined) throw 'settled reaction retained its cycle';
                    print('released:ok');
                }});
            "#
        ));
        assert_eq!(
            events,
            vec![
                HostOutputEvent::PrintLine("reaction:ok".into()),
                HostOutputEvent::PrintLine("released:ok".into()),
            ]
        );
    }
}

#[test]
fn emitted_cyclic_churn_reuses_the_product_gc_heap() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for directive in ["", "\"use strict\";\n"] {
        let source = format!(
            r#"{directive}
                const live = {{marker: 41}};
                live.self = live;
                const samples = [];
                function churn(round) {{
                    for (let index = 0; index < 512; index++) {{
                        const left = {{round, index}};
                        const right = {{left, marker: 73}};
                        left.right = right;
                        if (index === 0) samples.push(new WeakRef(left));
                        if (right.left !== left || left.right.marker !== 73)
                            throw 'cycle allocation';
                    }}
                }}
                for (let round = 0; round < 128; round++) {{
                    churn(round);
                    gc();
                    if (live.marker !== 41 || live.self !== live)
                        throw 'retained cycle';
                }}
                Promise.resolve().then(function() {{
                    gc();
                    for (const sample of samples) {{
                        if (sample.deref() !== undefined) throw 'released cycle retained';
                    }}
                    if (live.self !== live) throw 'live cycle lost';
                    print('churn:128:512:ok');
                }});
                void 0;
            "#
        );
        let engine = Engine::new(RealmBuilder::new().build());
        let compilation = engine
            .profile_script_compilation(
                &source,
                CompileOptions {
                    host_surface_policy: HostSurfacePolicy::Test262,
                    ..CompileOptions::default()
                },
            )
            .unwrap();
        let profile = engine
            .profile_wasm_execution(&compilation.artifact, Some(30_000), false)
            .unwrap();
        assert_eq!(profile.outcome.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(profile.outcome.completion, ObservedCompletion::Normal(_)),
            "{:?}",
            profile.outcome.completion
        );
        assert_eq!(
            profile.outcome.output_events,
            vec![HostOutputEvent::PrintLine("churn:128:512:ok".into())]
        );
        // These are reserved-capacity snapshots, not allocation/live-byte or
        // pause counters. The original product configuration is used intact.
        assert!(profile.heap.capacity_before_execution_bytes <= 16 * 1024 * 1024);
        assert!(
            profile.heap.capacity_after_execution_bytes <= 16 * 1024 * 1024,
            "cyclic churn grew the product GC heap: {:?}",
            profile.heap
        );
    }
}

#[test]
fn escaped_closure_roots_foreign_cycles_until_release_in_a_later_job() {
    for directive in ["", "\"use strict\";\n"] {
        let events = run(&format!(
            r#"{directive}
                let keep, weak;
                (function() {{
                    const foreign = __scriptwCreateRealm().global;
                    const key = new foreign.Object();
                    const local = {{marker: 73, foreign: key}};
                    key.marker = 41;
                    key.local = local;
                    key.self = key;
                    weak = new WeakRef(key);
                    keep = function() {{
                        gc();
                        if (key.marker !== 41 || key.self !== key ||
                            key.local !== local || local.foreign !== key ||
                            local.marker !== 73 ||
                            Object.getPrototypeOf(key) !== foreign.Object.prototype ||
                            Object.getPrototypeOf(local) !== Object.prototype)
                            throw 'cross-realm cycle roots';
                        print('foreign:rooted');
                    }};
                }})();
                Promise.resolve().then(function() {{
                    gc();
                    if (weak.deref() === undefined) throw 'closure lost foreign cycle';
                    keep();
                    keep = undefined;
                }}).then(function() {{
                    gc();
                    if (weak.deref() !== undefined) throw 'released foreign cycle retained';
                    print('foreign:released');
                }});
            "#
        ));
        assert_eq!(
            events,
            vec![
                HostOutputEvent::PrintLine("foreign:rooted".into()),
                HostOutputEvent::PrintLine("foreign:released".into()),
            ]
        );
    }
}

#[test]
fn weak_collection_indexes_preserve_key_identity_after_movement_and_dead_key_churn() {
    let source = r#"
        const map = new WeakMap(), set = new WeakSet();
        const object = {}, symbol = Symbol('local'), fn = function() {}, proxy = new Proxy({}, {});
        const equalShape = {}, keys = [object, symbol, fn, proxy, equalShape];
        const values = keys.map(function(key, index) { return {key, index}; });
        for (let i = 0; i < keys.length; i++) { map.set(keys[i], values[i]); set.add(keys[i]); }
        for (let round = 0; round < 32; round++) {
            (function() {
                for (let i = 0; i < 256; i++) { const dead = {}; map.set(dead, {round, i}); set.add(dead); }
            })();
            gc();
            for (let i = 0; i < keys.length; i++) {
                if (!map.has(keys[i]) || map.get(keys[i]) !== values[i] || !set.has(keys[i])) throw 'moved key identity';
            }
            const old = values[round % keys.length], key = keys[round % keys.length];
            if (!map.delete(key) || map.delete(key) || map.has(key) || map.get(key) !== undefined) throw 'map deletion';
            if (!set.delete(key) || set.delete(key) || set.has(key)) throw 'set deletion';
            map.set(key, old); set.add(key);
        }
        const watches = (function() {
            const a = {}, b = {}, c = {};
            map.set(a, b); map.set(b, a); set.add(c);
            return [new WeakRef(a), new WeakRef(b), new WeakRef(c)];
        })();
        Promise.resolve().then(function() {
            gc();
            for (const weak of watches) if (weak.deref() !== undefined) throw 'index retained dead key';
            for (let i = 0; i < keys.length; i++) if (map.get(keys[i]) !== values[i] || !set.has(keys[i])) throw 'job key identity';
            print('weak-index:ok');
        });
    "#;
    for strict in [false, true] {
        let source = if strict {
            format!("'use strict';{source}")
        } else {
            source.into()
        };
        assert_eq!(
            run(&source),
            vec![HostOutputEvent::PrintLine("weak-index:ok".into())]
        );
    }
}

#[test]
fn weak_collections_find_entries_by_identity_whatever_minted_it() {
    let events = run(r#"
        const map = new WeakMap(), set = new WeakSet();
        // A key no collection ever stored has no identity: every lookup
        // answers without one, and a lookup mints none.
        const fresh = {}, freshSymbol = Symbol('fresh');
        for (let round = 0; round < 2; round++) {
            for (const key of [fresh, freshSymbol, function() {}, [], new Proxy({}, {})]) {
                if (map.has(key) || map.get(key) !== undefined || map.delete(key)) throw 'fresh map lookup';
                if (set.has(key) || set.delete(key)) throw 'fresh set lookup';
            }
        }
        // Identities minted elsewhere (a Map or Set key, a Symbol property
        // key) are the ones the weak collections then use.
        const hashed = {}, named = Symbol('named'), strong = new Map([[hashed, 1]]);
        const holder = {[named]: 2}; new Set([named]);
        if (map.has(hashed) || map.has(named) || set.has(hashed) || set.has(named)) throw 'hashed before store';
        map.set(hashed, 'object'); map.set(named, 'symbol'); set.add(hashed); set.add(named);
        if (map.get(hashed) !== 'object' || map.get(named) !== 'symbol' || !set.has(hashed) || !set.has(named)) throw 'hashed after store';
        if (strong.get(hashed) !== 1 || holder[named] !== 2) throw 'strong users';
        // And the other way round.
        map.set(fresh, 'fresh'); set.add(freshSymbol);
        strong.set(fresh, 3); strong.set(freshSymbol, 4);
        if (strong.get(fresh) !== 3 || strong.get(freshSymbol) !== 4 || map.get(fresh) !== 'fresh' || !set.has(freshSymbol)) throw 'weak first';
        // A collection is an owner by its own identity, and a key like any other.
        const inner = new WeakMap(), outer = new WeakMap();
        if (outer.has(inner) || inner.has(outer)) throw 'collection as fresh key';
        outer.set(inner, inner); inner.set(outer, outer); inner.set(inner, 5); set.add(set);
        if (outer.get(inner) !== inner || inner.get(outer) !== outer || inner.get(inner) !== 5 || outer.has(outer) || !set.has(set)) throw 'collection keys';
        // Frozen, sealed and non-extensible keys and collections still store.
        const frozen = Object.freeze({}), sealed = Object.seal({}), fixed = Object.preventExtensions({});
        const frozenMap = Object.freeze(new WeakMap()), frozenSet = Object.freeze(new WeakSet());
        for (const key of [frozen, sealed, fixed]) {
            if (map.has(key) || frozenMap.has(key) || frozenSet.has(key)) throw 'frozen before';
            map.set(key, key); frozenMap.set(key, map); frozenSet.add(key);
            if (map.get(key) !== key || frozenMap.get(key) !== map || !frozenSet.has(key)) throw 'frozen after';
            if (Reflect.ownKeys(key).length !== 0 || !Object.isFrozen(frozen)) throw 'identity is not a property';
        }
        gc();
        if (map.get(hashed) !== 'object' || map.get(named) !== 'symbol' || map.get(fresh) !== 'fresh' ||
            outer.get(inner) !== inner || frozenMap.get(frozen) !== map || !frozenSet.has(sealed)) throw 'after collection';
        print('identities:ok');
    "#);
    assert_eq!(
        events,
        vec![HostOutputEvent::PrintLine("identities:ok".into())]
    );
}

#[test]
fn collections_sharing_keys_keep_separate_entries_through_deletion_and_death() {
    let events = run(r#"
        const keys = []; for (let i = 0; i < 64; i++) keys.push(i % 2 ? {i} : Symbol('k' + i));
        let maps = [], sets = [];
        for (let m = 0; m < 24; m++) { maps.push(new WeakMap()); sets.push(new WeakSet()); }
        for (let m = 0; m < maps.length; m++) for (let k = 0; k < keys.length; k++) {
            if ((m + k) % 3) { maps[m].set(keys[k], m * 1000 + k); sets[m].add(keys[k]); }
        }
        function check(label) {
            for (let m = 0; m < maps.length; m++) for (let k = 0; k < keys.length; k++) {
                const expected = (m + k) % 3 ? m * 1000 + k : undefined;
                if (maps[m].get(keys[k]) !== expected || maps[m].has(keys[k]) !== (expected !== undefined) ||
                    sets[m].has(keys[k]) !== (expected !== undefined)) throw label + ':' + m + ':' + k;
            }
        }
        check('stored'); gc(); check('moved');
        // Deleting from one collection leaves the others; deleting twice and
        // storing again reuses the entry's place.
        for (let k = 0; k < keys.length; k++) {
            const had = 5 % 3 !== 0 && (5 + k) % 3 !== 0;
            if (maps[5].delete(keys[k]) !== had || maps[5].delete(keys[k]) || maps[5].has(keys[k])) throw 'map delete';
            if (sets[5].delete(keys[k]) !== had || sets[5].delete(keys[k]) || sets[5].has(keys[k])) throw 'set delete';
        }
        for (let round = 0; round < 40; round++) for (let k = 0; k < keys.length; k++) {
            if ((5 + k) % 3) { maps[5].set(keys[k], round); if (!maps[5].delete(keys[k])) throw 'churn'; sets[5].add(keys[k]); sets[5].delete(keys[k]); }
        }
        for (let k = 0; k < keys.length; k++) if ((5 + k) % 3) { maps[5].set(keys[k], 5000 + k); sets[5].add(keys[k]); }
        check('restored'); gc(); check('restored and moved');
        // Collections that die before their keys release their values, and
        // later collections do not see the dead ones' entries.
        const watched = [];
        (function() {
            for (let m = 0; m < maps.length; m += 2) {
                const value = {m}; maps[m].set(keys[1], value); watched.push(new WeakRef(value));
            }
        })();
        const survivors = [maps[1], sets[1]];
        maps = [maps[1]]; sets = [sets[1]];
        Promise.resolve().then(function() {
            gc();
            for (const weak of watched) if (weak.deref() !== undefined) throw 'dead collection kept a value';
            const later = new WeakMap(), laterSet = new WeakSet();
            for (let k = 0; k < keys.length; k++) {
                if (later.has(keys[k]) || later.get(keys[k]) !== undefined || laterSet.has(keys[k])) throw 'later collection saw an old entry';
                const expected = (1 + k) % 3 ? 1000 + k : undefined;
                if (survivors[0].get(keys[k]) !== expected || survivors[1].has(keys[k]) !== (expected !== undefined)) throw 'survivor';
                later.set(keys[k], k); laterSet.add(keys[k]);
            }
            gc();
            for (let k = 0; k < keys.length; k++) if (later.get(keys[k]) !== k || !laterSet.has(keys[k])) throw 'later';
            print('shared keys:ok');
        });
    "#);
    assert_eq!(
        events,
        vec![HostOutputEvent::PrintLine("shared keys:ok".into())]
    );
}

#[test]
fn ten_thousand_keys_survive_index_growth_promotion_and_dead_key_reuse() {
    let events = run(r#"
        const COUNT = 10000, map = new WeakMap(), set = new WeakSet();
        let keys = [];
        // A Store's first full nursery continues in the old space and its
        // first collection is full: allocate past both first.
        for (let j = 0; j < 400000; j++) ({garbage: [j, 'text' + j]});
        // Allocation between the stores runs minor collections, so keys are
        // stored young and looked up after promotion, without gc().
        for (let i = 0; i < COUNT; i++) {
            const key = {i}; keys.push(key);
            map.set(key, i % 7 ? {key, i} : i); if (i % 2) set.add(key);
            if (i % 64 === 0) for (let j = 0; j < 400; j++) ({garbage: [i, j, 'text' + j]});
        }
        function check(label) {
            for (let i = 0; i < keys.length; i++) {
                const key = keys[i], value = map.get(key);
                if (key === undefined) continue;
                if (i % 7 ? value.key !== key || value.i !== i : value !== i) throw label + ' value ' + i;
                if (!map.has(key) || set.has(key) !== (i % 2 === 1)) throw label + ' membership ' + i;
            }
        }
        check('stored'); gc(); check('full collection');
        // Half the keys die. Their entries retire, and new keys take their
        // places in the index without disturbing the survivors.
        const dead = [];
        (function() { for (let i = 0; i < COUNT; i += 2) { if (i % 500 === 0) dead.push(new WeakRef(keys[i])); keys[i] = undefined; } })();
        Promise.resolve().then(function() {
            gc();
            for (const weak of dead) if (weak.deref() !== undefined) throw 'value kept its dead key';
            check('after deaths');
            const fresh = [];
            for (let i = 0; i < COUNT; i++) {
                const key = [i]; fresh.push(key);
                if (map.has(key) || set.has(key)) throw 'new key found';
                map.set(key, -i); set.add(key);
            }
            check('after refill');
            for (let i = 0; i < COUNT; i++) if (map.get(fresh[i]) !== -i || !set.has(fresh[i])) throw 'refill ' + i;
            for (let i = 0; i < COUNT; i += 3) if (!map.delete(fresh[i]) || !set.delete(fresh[i])) throw 'delete ' + i;
            gc();
            for (let i = 0; i < COUNT; i++) {
                if (map.get(fresh[i]) !== (i % 3 ? -i : undefined) || set.has(fresh[i]) !== (i % 3 !== 0)) throw 'after delete ' + i;
            }
            check('end');
            print('ten thousand:ok');
        });
    "#);
    assert_eq!(
        events,
        vec![HostOutputEvent::PrintLine("ten thousand:ok".into())]
    );
}

#[test]
fn weak_collections_of_two_realms_share_keys_without_sharing_entries() {
    let events = run(r#"
        const other = __scriptwCreateRealm().global;
        const here = new WeakMap(), there = new other.WeakMap(), hereSet = new WeakSet(), thereSet = new other.WeakSet();
        const local = {}, foreign = new other.Object(), localSymbol = Symbol('here'), foreignSymbol = other.Symbol('there');
        const keys = [local, foreign, localSymbol, foreignSymbol, other, other.WeakMap, WeakMap.prototype];
        for (let i = 0; i < keys.length; i++) {
            if (here.has(keys[i]) || there.has(keys[i]) || hereSet.has(keys[i]) || thereSet.has(keys[i])) throw 'before';
            here.set(keys[i], 'here' + i);
            if (there.has(keys[i]) || thereSet.has(keys[i])) throw 'entry crossed realms';
            there.set(keys[i], 'there' + i); if (i % 2) hereSet.add(keys[i]); else thereSet.add(keys[i]);
        }
        function check(label) {
            for (let i = 0; i < keys.length; i++) {
                // Each realm's methods read the other realm's collections.
                if (other.WeakMap.prototype.get.call(here, keys[i]) !== 'here' + i ||
                    WeakMap.prototype.get.call(there, keys[i]) !== 'there' + i ||
                    other.WeakSet.prototype.has.call(hereSet, keys[i]) !== (i % 2 === 1) ||
                    WeakSet.prototype.has.call(thereSet, keys[i]) !== (i % 2 === 0)) throw label + i;
            }
        }
        check('stored'); gc(); check('moved');
        if (!other.WeakMap.prototype.delete.call(here, foreign) || here.has(foreign) || there.get(foreign) !== 'there1') throw 'delete';
        try { WeakMap.prototype.get.call(hereSet, local); throw 'brand'; } catch (e) { if (!(e instanceof TypeError)) throw e; }
        try { other.WeakSet.prototype.has.call(there, local); throw 'foreign brand'; } catch (e) { if (!(e instanceof other.TypeError)) throw e; }
        print('realms:ok');
    "#);
    assert_eq!(events, vec![HostOutputEvent::PrintLine("realms:ok".into())]);
}
