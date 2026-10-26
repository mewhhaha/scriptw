use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, RealmBuilder, RunOptions,
};

fn assert_proto_getter(source: &str, host_surface_policy: HostSurfacePolicy) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
            source,
            CompileOptions {
                host_surface_policy,
                ..CompileOptions::default()
            },
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|error| {
            panic!("prototype getter receiver check failed: {error}\n{source}")
        });
    assert!(
        outcome.note.contains("boolean(true)"),
        "{}\n{source}",
        outcome.note
    );
}

#[test]
fn prototype_getter_rejects_nullish_receivers_and_boxes_other_primitives() {
    assert_proto_getter(
        r#"
var get = Object.getOwnPropertyDescriptor(Object.prototype, '__proto__').get;
function rejects(receiver) {
  try { get.call(receiver); return false; }
  catch (error) { return error instanceof TypeError; }
}
var prototype = {};
var object = Object.create(prototype);
rejects(undefined) && rejects(null) &&
  get.call(object) === prototype && get.call(Object.create(null)) === null &&
  get.call(17) === Number.prototype && get.call(false) === Boolean.prototype &&
  get.call('value') === String.prototype && get.call(Symbol()) === Symbol.prototype &&
  get.call(1n) === BigInt.prototype && get.call(1n << 70n) === BigInt.prototype;
"#,
        HostSurfacePolicy::Product,
    );
}

#[test]
fn prototype_getter_preserves_proxy_throws_and_uses_its_defining_error_realm() {
    assert_proto_getter(
        r#"
var other = __scriptwCreateRealm().global;
var descriptor = Object.getOwnPropertyDescriptor(other.Object.prototype, '__proto__');
var get = descriptor.get;
var set = descriptor.set;
var local = Object.getOwnPropertyDescriptor(Object.prototype, '__proto__');
function rejects(receiver) {
  try { get.call(receiver); return false; }
  catch (error) {
    return Object.getPrototypeOf(error) === other.TypeError.prototype &&
      error instanceof other.TypeError && !(error instanceof TypeError);
  }
}
var marker = {};
var calls = 0;
var proxy = new Proxy({}, { getPrototypeOf() { calls++; throw marker; } });
var received;
try { get.call(proxy); } catch (error) { received = error; }
var setterErrors = 0;
try { set.call(null, {}); }
catch (error) { if (Object.getPrototypeOf(error) === other.TypeError.prototype) setterErrors++; }
var immutable = Object.preventExtensions({});
try { set.call(immutable, {}); }
catch (error) { if (Object.getPrototypeOf(error) === other.TypeError.prototype) setterErrors++; }
var object = {};
var prototype = {};
var assigned = set.call(object, prototype);
rejects(undefined) && rejects(null) && calls === 1 && received === marker &&
  !descriptor.enumerable && descriptor.configurable &&
  get !== local.get && set !== local.set &&
  Object.getPrototypeOf(get) === other.Function.prototype &&
  Object.getPrototypeOf(set) === other.Function.prototype &&
  get.call(17) === other.Number.prototype &&
  get.call(false) === other.Boolean.prototype &&
  get.call('value') === other.String.prototype &&
  get.call(Symbol()) === other.Symbol.prototype &&
  get.call(1n) === other.BigInt.prototype &&
  get.call(1n << 70n) === other.BigInt.prototype &&
  setterErrors === 2 && assigned === undefined && get.call(object) === prototype;
"#,
        HostSurfacePolicy::Test262,
    );
}

#[test]
fn prototype_payload_references_preserve_every_special_value_tag_and_identity() {
    assert_proto_getter(
        r#"
var args=(function(){return arguments;})(7),bound=(function(v){return v;}).bind(null,13);
var array=[3],fn=function(v){return v+1;},ordinary={},boxed=new Number(5);
var proxy=new Proxy(fn,{}),revocable=Proxy.revocable({},{}),revoked=revocable.proxy;
revocable.revoke();
var prototypes=[array,fn,bound,args,ordinary,boxed,proxy,revoked,null];
var ok=true;
for(var i=0;i<prototypes.length;i++){
  var prototype=prototypes[i],object=Object.create(prototype);gc();
  ok=ok && Object.getPrototypeOf(object)===prototype && Reflect.getPrototypeOf(object)===prototype;
  Object.preventExtensions(object);
  ok=ok && Reflect.setPrototypeOf(object,prototype);
}
var arrayChild=Object.create(array),functionChild=Object.create(fn),argsChild=Object.create(args);
ok && Object.getPrototypeOf(arrayChild)[0]===3 && Object.getPrototypeOf(functionChild)(8)===9
  && Object.getPrototypeOf(Object.create(bound))()===13 && Object.getPrototypeOf(argsChild)[0]===7
  && Object.getPrototypeOf(Object.create(proxy))(4)===5;
"#,
        HostSurfacePolicy::Test262,
    );
}

#[test]
fn prototype_reference_updates_keep_proxy_cycle_boundaries_and_primitive_rules() {
    assert_proto_getter(
        r#"
var a={},b=Object.create(a),calls=0,proxy=new Proxy({}, {
  getPrototypeOf:function(){calls++;throw 'unexpected cycle trap';}
});
var cycle=!Reflect.setPrototypeOf(a,b);
var object={};var acceptsProxy=Reflect.setPrototypeOf(object,proxy) && calls===0;
var rejected=false;try{Object.setPrototypeOf(object,17);}catch(error){rejected=error instanceof TypeError;}
var ignored={__proto__:17},nullish={__proto__:null},array=[];
var changed=Reflect.setPrototypeOf(object,array);gc();
cycle && acceptsProxy && rejected && changed && Object.getPrototypeOf(object)===array
  && Object.getPrototypeOf(ignored)===Object.prototype && Object.getPrototypeOf(nullish)===null
  && !Reflect.setPrototypeOf(Object.prototype,{});
"#,
        HostSurfacePolicy::Test262,
    );
}

#[test]
fn prototype_snapshot_edges_preserve_array_identity_and_arguments_rejection() {
    use scriptw_engine::{SnapshotLimits, SnapshotOutcome};
    use scriptw_runtime::rooted_snapshot::{
        SnapshotDescriptor, SnapshotExotic, SnapshotRejection, SnapshotValue,
    };
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let engine = Engine::new(RealmBuilder::new().build());
    let options = CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    };
    let run = RunOptions {
        backend: ExecutionBackend::WasmAot,
        ..RunOptions::default()
    };
    let outcome = engine
        .observe_script_graph(
            "var p=[-0,NaN];Object.setPrototypeOf(p,null);var o=Object.create(p);o.link=p;gc();o;",
            options.clone(),
            run.clone(),
            SnapshotLimits::default(),
        )
        .unwrap();
    let SnapshotOutcome::Captured { graph } = outcome.completion.outcome else {
        panic!("ordinary object with an Array prototype must be captured");
    };
    assert_eq!(graph.nodes().len(), 2);
    let object = &graph.nodes()[0];
    assert_eq!(
        object.prototype,
        SnapshotValue::Object {
            id: graph.nodes()[1].id
        }
    );
    assert!(
        matches!(&object.properties[0].descriptor, SnapshotDescriptor::Data { value, .. } if *value == object.prototype)
    );
    assert_eq!(graph.nodes()[1].prototype, SnapshotValue::Null {});
    let outcome = engine
        .observe_script_graph(
            "var a=(function(){return arguments;})(1);Object.create(a);",
            options,
            run,
            SnapshotLimits::default(),
        )
        .unwrap();
    assert!(matches!(
        outcome.completion.outcome,
        SnapshotOutcome::Rejected {
            reason: SnapshotRejection::UnsupportedExotic {
                exotic: SnapshotExotic::Arguments
            },
        }
    ));
}
