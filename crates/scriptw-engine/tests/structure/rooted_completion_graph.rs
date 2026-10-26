use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy, RealmBuilder,
    RunOptions, SnapshotLimits, SnapshotOutcome,
};
use scriptw_runtime::rooted_snapshot::{
    SnapshotBudgetDimension, SnapshotCompletionKind, SnapshotDescriptor, SnapshotIntrinsic,
    SnapshotKey, SnapshotObjectKind, SnapshotRejection, SnapshotSymbolOrigin, SnapshotValue,
    SnapshotWellKnownSymbol,
};

fn observe(source: &str, limits: SnapshotLimits) -> scriptw_engine::GraphRunOutcome {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    Engine::new(RealmBuilder::new().build())
        .observe_script_graph(
            source,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(120_000),
                ..RunOptions::default()
            },
            limits,
        )
        .unwrap()
}

fn property<'a>(
    node: &'a scriptw_runtime::rooted_snapshot::SnapshotNode,
    name: &str,
) -> &'a SnapshotDescriptor {
    let units: Vec<_> = name.encode_utf16().collect();
    &node
        .properties
        .iter()
        .find(|property| {
            matches!(&property.key,
        SnapshotKey::String { units: key } if key.units() == units)
        })
        .unwrap()
        .descriptor
}

#[test]
fn property_growth_snapshot_preserves_live_insertion_extent_without_spare_slots() {
    let outcome = observe(
        r#"
var object = Object.create(null);
for (var i = 0; i < 65; i++) object['key' + i] = i;
delete object.key0; delete object.key32; delete object.key64;
object.key0 = 1000;
object['2'] = 2; object['1'] = 1;
object[Symbol('last')] = object;
gc();
object;
"#,
        SnapshotLimits::default(),
    );
    let SnapshotOutcome::Captured { graph } = outcome.completion.outcome else {
        panic!("graph was rejected: {:?}", outcome.completion.outcome);
    };
    assert_eq!(graph.root(), &SnapshotValue::Object { id: 0 });
    let root = &graph.nodes()[0];
    assert_eq!(root.properties.len(), 66);
    let names = root
        .properties
        .iter()
        .filter_map(|property| match &property.key {
            SnapshotKey::String { units } => Some(String::from_utf16(units.units()).unwrap()),
            SnapshotKey::Symbol { .. } => None,
        })
        .collect::<Vec<_>>();
    let expected = ["1".into(), "2".into()]
        .into_iter()
        .chain(
            (1..64)
                .filter(|index| *index != 32)
                .map(|index| format!("key{index}")),
        )
        .chain(["key0".into()])
        .collect::<Vec<String>>();
    assert_eq!(names, expected);
    assert!(matches!(
        root.properties.last().unwrap().key,
        SnapshotKey::Symbol { .. }
    ));
    assert!(matches!(
        root.properties.last().unwrap().descriptor,
        SnapshotDescriptor::Data {
            value: SnapshotValue::Object { id: 0 },
            ..
        }
    ));
}

#[test]
fn array_snapshots_preserve_dense_holes_and_sparse_transition_descriptors() {
    for transition in [false, true] {
        let source = format!(
            r#"
var array=[0,undefined,-0,3,4],token={{}};
delete array[0];array[4]=array;
Object.defineProperty(array,'2',{{writable:false,configurable:false}});
Object.defineProperty(array,'7',{{get:function(){{throw 'snapshot invoked accessor';}},enumerable:true,configurable:true}});
{transition}
gc();array;
"#,
            transition = if transition {
                "array[4294967294]=token;"
            } else {
                ""
            }
        );
        let outcome = observe(&source, SnapshotLimits::default());
        let SnapshotOutcome::Captured { graph } = outcome.completion.outcome else {
            panic!("array graph was rejected: {:?}", outcome.completion.outcome);
        };
        assert_eq!(graph.root(), &SnapshotValue::Object { id: 0 });
        let root = &graph.nodes()[0];
        let names = root
            .properties
            .iter()
            .map(|property| match &property.key {
                SnapshotKey::String { units } => String::from_utf16(units.units()).unwrap(),
                SnapshotKey::Symbol { .. } => panic!("array control has no symbol keys"),
            })
            .collect::<Vec<_>>();
        let expected = if transition {
            vec!["1", "2", "3", "4", "7", "4294967294", "length"]
        } else {
            vec!["1", "2", "3", "4", "7", "length"]
        };
        assert_eq!(names, expected);
        assert!(matches!(
            property(root, "1"),
            SnapshotDescriptor::Data {
                value: SnapshotValue::Undefined {},
                ..
            }
        ));
        assert!(matches!(property(root, "2"), SnapshotDescriptor::Data {
            value: SnapshotValue::Number { bits }, writable: false, configurable: false, ..
        } if *bits == (-0f64).to_bits()));
        assert!(matches!(
            property(root, "4"),
            SnapshotDescriptor::Data {
                value: SnapshotValue::Object { id: 0 },
                ..
            }
        ));
        assert!(matches!(
            property(root, "7"),
            SnapshotDescriptor::Accessor {
                get: SnapshotValue::Object { .. },
                set: SnapshotValue::Undefined {},
                enumerable: true,
                configurable: true
            }
        ));
    }
}

#[test]
fn empty_dense_arrays_release_backing_before_bounded_graph_observation() {
    for removal in [
        "array.length=0;",
        "for(var i=0;i<1024;i++){delete array[i];}",
    ] {
        // Remove the prototype graph so the budget measures this array. A
        // retained 1,024-slot backing alone would exceed the 512-work budget.
        let source = format!(
            "var array=[];Object.setPrototypeOf(array,null);for(var i=0;i<1024;i++){{array[i]=i;}}{removal}gc();array;"
        );
        let limits = SnapshotLimits::new(8, 8, 2, 8, 64, 16, 512, 8).unwrap();
        let outcome = observe(&source, limits);
        let SnapshotOutcome::Captured { graph } = outcome.completion.outcome else {
            panic!(
                "empty array retained backing: {:?}",
                outcome.completion.outcome
            );
        };
        let root = &graph.nodes()[0];
        assert_eq!(root.prototype, SnapshotValue::Null {});
        assert_eq!(root.properties.len(), 1);
        let expected_length = if removal == "array.length=0;" {
            0f64
        } else {
            1024f64
        };
        assert!(
            matches!(property(root, "length"), SnapshotDescriptor::Data {
            value: SnapshotValue::Number { bits }, ..
        } if *bits == expected_length.to_bits())
        );
    }
}

#[test]
fn original_throw_is_rooted_through_jobs_without_running_getters_or_symbol_hooks() {
    let outcome = observe(
        r#"
let root = Object.create(null);
const local = Symbol('same'), second = Symbol('same'), registered = Symbol.for('registry');
Object.defineProperty(root, 'accessor', { get() { throw 'snapshot called getter'; }, enumerable: true });
root.self = root;
root.array = [, undefined, -0];
Object.defineProperty(root.array, '2', { writable: false, configurable: false });
root.local = local; root.again = local; root.second = second;
root.registered = registered; root.wellKnown = Symbol.iterator;
root[local] = 'symbol property';
root.fn = function(value) { return value; }; root.native = Object;
Promise.resolve().then(() => { root.afterJobs = 7; root = null; gc(); print('jobs settled'); });
throw root;
"#,
        SnapshotLimits::default(),
    );
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(outcome.completion.kind, SnapshotCompletionKind::Throw);
    assert_eq!(
        outcome.output_events,
        vec![HostOutputEvent::PrintLine("jobs settled".into())]
    );
    let SnapshotOutcome::Captured { graph } = outcome.completion.outcome else {
        panic!("graph was rejected: {:?}", outcome.completion.outcome);
    };
    assert_eq!(graph.root(), &SnapshotValue::Object { id: 0 });
    let root = &graph.nodes()[0];
    assert_eq!(root.prototype, SnapshotValue::Null {});
    assert!(matches!(
        property(root, "self"),
        SnapshotDescriptor::Data {
            value: SnapshotValue::Object { id: 0 },
            ..
        }
    ));
    assert!(
        matches!(property(root, "afterJobs"), SnapshotDescriptor::Data { value: SnapshotValue::Number { bits }, .. } if *bits == 7f64.to_bits())
    );
    assert!(matches!(
        property(root, "accessor"),
        SnapshotDescriptor::Accessor {
            get: SnapshotValue::Object { .. },
            set: SnapshotValue::Undefined {},
            ..
        }
    ));
    let symbol = |name| match property(root, name) {
        SnapshotDescriptor::Data {
            value: SnapshotValue::Symbol { id },
            ..
        } => *id,
        _ => panic!("missing Symbol"),
    };
    assert_eq!(symbol("local"), symbol("again"));
    assert_ne!(symbol("local"), symbol("second"));
    assert!(
        matches!(&graph.symbols()[symbol("registered") as usize].data.origin, SnapshotSymbolOrigin::Registry { key } if key.units() == "registry".encode_utf16().collect::<Vec<_>>())
    );
    assert_eq!(
        graph.symbols()[symbol("wellKnown") as usize].data.origin,
        SnapshotSymbolOrigin::WellKnown {
            name: SnapshotWellKnownSymbol::Iterator
        }
    );
    let array_id = match property(root, "array") {
        SnapshotDescriptor::Data {
            value: SnapshotValue::Object { id },
            ..
        } => *id,
        _ => panic!("missing Array"),
    };
    let array = &graph.nodes()[array_id as usize];
    assert_eq!(array.kind, SnapshotObjectKind::Array {});
    assert!(!array.properties.iter().any(
        |property| matches!(&property.key, SnapshotKey::String { units } if units.units() == [48])
    ));
    assert!(matches!(
        property(array, "1"),
        SnapshotDescriptor::Data {
            value: SnapshotValue::Undefined {},
            ..
        }
    ));
    assert!(
        matches!(property(array, "2"), SnapshotDescriptor::Data { value: SnapshotValue::Number { bits }, writable: false, configurable: false, .. } if *bits == (-0f64).to_bits())
    );
    assert!(matches!(
        property(array, "length"),
        SnapshotDescriptor::Data {
            enumerable: false,
            configurable: false,
            ..
        }
    ));
    assert!(graph.nodes().iter().any(|node| matches!(
        node.kind,
        SnapshotObjectKind::Function {
            constructable: true,
            realm: 0
        }
    )));
}

#[test]
fn created_realm_anchors_follow_retained_intrinsic_records_and_error_graphs_are_ordinary() {
    let outcome = observe(
        r#"
const foreign = __scriptwCreateRealm().global;
const root = Object.create(null);
root.object = new foreign.Object();
root.function = foreign.Object;
root.error = new foreign.TypeError('foreign');
root.entry = Object.prototype;
foreign.Object = 17;
throw root;
"#,
        SnapshotLimits::default(),
    );
    assert_eq!(outcome.completion.kind, SnapshotCompletionKind::Throw);
    let SnapshotOutcome::Captured { graph } = outcome.completion.outcome else {
        panic!("graph was rejected: {:?}", outcome.completion.outcome);
    };
    assert_eq!(graph.realm_count(), 2);
    let anchor = |intrinsic, realm| {
        graph.nodes().iter().any(|node| {
            node.anchors
                .iter()
                .any(|anchor| anchor.intrinsic == intrinsic && anchor.realm == realm)
        })
    };
    assert!(anchor(SnapshotIntrinsic::ObjectPrototype, 0));
    assert!(anchor(SnapshotIntrinsic::ObjectConstructor, 1));
    assert!(anchor(SnapshotIntrinsic::ObjectPrototype, 1));
    assert!(anchor(SnapshotIntrinsic::TypeErrorPrototype, 1));
    let root = &graph.nodes()[0];
    let error = match property(root, "error") {
        SnapshotDescriptor::Data {
            value: SnapshotValue::Object { id },
            ..
        } => &graph.nodes()[*id as usize],
        _ => panic!("missing Error"),
    };
    assert_eq!(error.kind, SnapshotObjectKind::Ordinary {});
    assert!(
        matches!(property(error, "message"), SnapshotDescriptor::Data { value: SnapshotValue::String { units }, .. } if units.units() == "foreign".encode_utf16().collect::<Vec<_>>())
    );
}

#[test]
fn unsupported_exotic_and_copy_budget_are_execution_observations() {
    let outcome = observe(
        "throw new Proxy({}, { ownKeys() { throw 'hook'; } });",
        SnapshotLimits::default(),
    );
    assert_eq!(outcome.completion.kind, SnapshotCompletionKind::Throw);
    assert!(matches!(
        outcome.completion.outcome,
        SnapshotOutcome::Rejected {
            reason: SnapshotRejection::UnsupportedExotic { .. }
        }
    ));
    let limits = SnapshotLimits::new(8, 8, 2, 16, 4, 16, 4096, 8).unwrap();
    let outcome = observe("'abcde'", limits);
    assert_eq!(outcome.completion.kind, SnapshotCompletionKind::Normal);
    assert_eq!(
        outcome.completion.outcome,
        SnapshotOutcome::Rejected {
            reason: SnapshotRejection::BudgetExceeded {
                dimension: SnapshotBudgetDimension::Utf16Units
            }
        }
    );
    let outcome = observe("'\\uD800abc'", limits);
    let SnapshotOutcome::Captured { graph } = outcome.completion.outcome else {
        panic!("exact cap rejected");
    };
    assert!(
        matches!(graph.root(), SnapshotValue::String { units } if units.units() == [0xd800, 97, 98, 99])
    );
}

#[test]
fn compact_plain_literal_snapshot_reads_live_soa_values_without_descriptor_materialization() {
    let outcome = observe(
        r#"
var symbol=Symbol('kept');
function make(){return {'2':2,self:null,big:17n,symbol:symbol,minus:-0,missing:undefined};}
function read(record){return record.self;}
var result=make();read(result);result.self=result;
var descriptor=Object.getOwnPropertyDescriptor(result,'big');
if(descriptor.value!==17n || !descriptor.writable)throw 'descriptor snapshot';
gc();result;
"#,
        SnapshotLimits::default(),
    );
    let SnapshotOutcome::Captured { graph } = outcome.completion.outcome else {
        panic!(
            "compact graph was rejected: {:?}",
            outcome.completion.outcome
        );
    };
    assert_eq!(graph.root(), &SnapshotValue::Object { id: 0 });
    let root = &graph.nodes()[0];
    let names = root
        .properties
        .iter()
        .map(|property| match &property.key {
            SnapshotKey::String { units } => String::from_utf16(units.units()).unwrap(),
            SnapshotKey::Symbol { .. } => panic!("all literal keys are static strings"),
        })
        .collect::<Vec<_>>();
    assert_eq!(names, ["2", "self", "big", "symbol", "minus", "missing"]);
    assert!(matches!(
        property(root, "self"),
        SnapshotDescriptor::Data {
            value: SnapshotValue::Object { id: 0 },
            writable: true,
            enumerable: true,
            configurable: true
        }
    ));
    assert!(
        matches!(property(root,"minus"), SnapshotDescriptor::Data { value:SnapshotValue::Number { bits }, .. } if *bits == (-0f64).to_bits())
    );
    assert!(matches!(
        property(root, "missing"),
        SnapshotDescriptor::Data {
            value: SnapshotValue::Undefined {},
            ..
        }
    ));
    assert!(matches!(
        property(root, "symbol"),
        SnapshotDescriptor::Data {
            value: SnapshotValue::Symbol { .. },
            ..
        }
    ));
}

#[test]
fn snapshot_materializes_unread_intrinsic_method_descriptors_without_js_hooks() {
    let outcome = observe(
        r#"
Object.setPrototypeOf(Math, null);
Object.setPrototypeOf(Function.prototype, null);
delete Function.prototype.constructor;
gc();
Math;
"#,
        SnapshotLimits::default(),
    );
    let SnapshotOutcome::Captured { graph } = outcome.completion.outcome else {
        panic!("graph was rejected: {:?}", outcome.completion.outcome);
    };
    let SnapshotDescriptor::Data {
        value: SnapshotValue::Object { id },
        writable: true,
        enumerable: false,
        configurable: true,
    } = property(&graph.nodes()[0], "abs")
    else {
        panic!("Math.abs descriptor lost its intrinsic value or attributes");
    };
    let method = &graph.nodes()[*id as usize];
    assert!(matches!(
        method.kind,
        SnapshotObjectKind::Function {
            constructable: false,
            realm: 0,
        }
    ));
    assert!(
        matches!(property(method, "length"), SnapshotDescriptor::Data {
        value: SnapshotValue::Number { bits }, ..
    } if *bits == 1f64.to_bits())
    );
}
