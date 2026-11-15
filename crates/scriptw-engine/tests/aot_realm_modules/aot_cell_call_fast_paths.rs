use scriptw_engine::{
    CompileOptions, EmbeddedModuleEntryInput, EmbeddedModuleGoal, EmbeddedModuleGraph,
    EmbeddedModuleReferrer, EmbeddedModuleResolutionInput, EmbeddedModuleSourceInput, Engine,
    ExecutionBackend, HostOutputEvent, HostSurfacePolicy, ModuleLoadingPolicy, ObservedCompletion,
    RealmBuilder, RunOptions,
};

fn check(source: &str, module: bool, options: CompileOptions, expected: &[&str]) {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let execution = RunOptions {
        backend: ExecutionBackend::WasmAot,
        timeout_ms: Some(30_000),
        ..RunOptions::default()
    };
    let outcome = if module {
        engine.observe_module(source, options, execution)
    } else {
        engine.observe_script(source, options, execution)
    }
    .unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{:?}: {}",
        outcome.completion,
        outcome.note
    );
    assert_eq!(
        outcome.output_events,
        expected
            .iter()
            .map(|line| HostOutputEvent::PrintLine((*line).into()))
            .collect::<Vec<_>>()
    );
}
fn options() -> CompileOptions {
    CompileOptions {
        host_surface_policy: HostSurfacePolicy::Test262,
        ..CompileOptions::default()
    }
}

#[test]
fn strict_cells_hold_callee_before_args_and_keep_tdz_noncallable_this_and_live_rows() {
    let rows = (1..=64)
        .map(|index| format!("rowStep({index});"))
        .collect::<String>();
    let source = format!(
        r#"
let log=[], active=function(value){{return 'old:'+value+':'+(this===undefined);}};
function replacement(value){{return 'new:'+value+':'+(this===undefined);}}
function argument(){{log.push('arg');active=replacement;gc();return 'x';}}
function run(){{var value=active(argument());return value;}}
function next(){{return active('y');}}
let first=run(), second=next();
function make(){{let step=function(value){{return value+':'+(this===undefined);}};function captured(value){{return step(value);}}function replace(value){{step=value;}}return[captured,replace];}}
let pair=make(), held=pair[0]('capture');pair[1](replacement);let changed=pair[0]('changed');
let events=[];
function before(){{return later((events.push('bad-arg'),1));}}
let tdz=false;try{{before();}}catch(error){{tdz=error instanceof ReferenceError;}}
let later=function(value){{return value;}}, bad=17;
function noncallable(){{return bad((events.push('arg'),1));}}
let rejected=false;try{{noncallable();}}catch(error){{rejected=error instanceof TypeError;}}
let total=0,seen=0,correctThis=true;
let rowStep=function(value){{seen++;correctThis=correctThis&&this===undefined;total+=value;rowStep=function(value){{seen++;correctThis=correctThis&&this===undefined;total+=10*value;}};}};
function rows(){{{rows}return total;}}
print(first==='old:x:true'&&second==='new:y:true'&&held==='capture:true'&&changed==='new:changed:true'&&
  log.join(',')==='arg'&&tdz&&rejected&&events.join(',')==='arg'&&rows()===20791&&seen===64&&correctThis);
"#
    );
    check(&source, true, options(), &["true"]);
}

#[test]
fn live_import_target_rebinding_keeps_old_callee_for_current_arguments_and_new_for_next_call() {
    let provider = r#"
export let step=function(value){return 'old:'+value+':'+(this===undefined);};
export function replace(value){step=function(value){return 'new:'+value+':'+(this===undefined);};gc();return value;}
"#;
    let entry = r#"
import {step,replace} from './provider.js';
function run(){var result=step(replace('x'));return result;}
function next(){return step('y');}
print(run()+'|'+next());
"#;
    let graph = EmbeddedModuleGraph::try_new(
        EmbeddedModuleEntryInput {
            goal: EmbeddedModuleGoal::Module,
            identity: "entry.js".into(),
            source: entry.into(),
            meta_url: "scriptw://entry.js".into(),
        },
        vec![EmbeddedModuleSourceInput {
            identity: "provider.js".into(),
            source: provider.into(),
            meta_url: "scriptw://provider.js".into(),
        }],
        vec![EmbeddedModuleResolutionInput {
            referrer: EmbeddedModuleReferrer::Module("entry.js".into()),
            specifier: "./provider.js".into(),
            attributes: Vec::new(),
            target: "provider.js".into(),
        }],
    )
    .unwrap();
    let mut compile = options();
    compile.module_loading_policy = ModuleLoadingPolicy::Embedded(graph);
    check(entry, true, compile, &["old:x:true|new:y:true"]);
}

#[test]
fn with_global_getter_and_direct_eval_calls_keep_generic_resolution_and_receiver_order() {
    check(
        r#"
var trace=[];
var scope={fn:function(value){return 'with:'+value+':'+(this===object);}};
var object=new Proxy(scope,{has(target,key){if(key==='fn')trace.push('has');return Reflect.has(target,key);},get(target,key,receiver){if(key==='fn')trace.push('get');return Reflect.get(target,key,receiver);}});
function argument(){trace.push('arg');scope.fn=function(){return 'changed';};return 'x';}
function withCall(){with(object){return fn(argument());}}
var first=withCall();
var replacement=function(value){'use strict';return 'new:'+value+':'+(this===undefined);};
Object.defineProperty(globalThis,'globalStep',{configurable:true,get(){trace.push('global-get');return function(value){'use strict';return 'global:'+value+':'+(this===undefined);};}});
function globalArg(){trace.push('global-arg');Object.defineProperty(globalThis,'globalStep',{configurable:true,value:replacement});gc();return 'y';}
function globalCall(){'use strict';return globalStep(globalArg());}
var held=globalCall();
function after(){'use strict';return globalStep('z');}
function evaluated(){'use strict';var step=eval("(function(value){return 'eval:'+value+':'+(this===undefined);})");return step(3);}
print(first);print(held);print(after());print(evaluated());print(trace.join('|'));
"#,
        false,
        options(),
        &[
            "with:x:true",
            "global:y:true",
            "new:z:true",
            "eval:3:true",
            // HasBinding and GetBindingValue each perform HasProperty.
            // Node24 omits the second trap in this control; ECMA-262 9.1.1.2.6
            // and aot_with_has_binding own the required observable check.
            "has|has|get|arg|global-get|global-arg",
        ],
    );
}

#[test]
fn arrow_functions_read_their_own_and_captured_cells_with_live_values_and_tdz() {
    // An arrow function with plain parameters reads proven cells like an
    // ordinary function: replaced callees, a binding in its temporal dead
    // zone, shadowing parameters, per-iteration loop bindings, a const
    // write, later declarations, class and catch bindings, and cells of an
    // enclosing function and generator. Expectations are Node's.
    let source = r##"
let log = [];
let step = (value) => "old:" + value;
const read = (value) => step(value);
const replace = (next) => { step = next; };
log.push(read(1));
replace((value) => "new:" + value);
log.push(read(2));
// A captured binding read before its initialization throws, and reads its value after.
const early = () => late;
try { early(); log.push("no error"); } catch (error) { log.push(error.constructor === ReferenceError); }
let late = "late";
log.push(early(), typeof early);
// Own parameters shadow captured names; nested arrows read every level.
const shadow = (step, late) => ((inner) => step + late + inner + log.length)("i");
log.push(shadow("s", "l"));
// Each loop iteration has its own binding, and a closure sees later writes to it.
const each = [];
for (let i = 0; i < 3; i++) { each.push(() => i); i += 10; each.push(() => i); i -= 10; }
log.push(each.map((f) => f()).join(","));
// A write through an arrow to a captured const throws; to a let it assigns.
const fixed = 1;
let counter = 0;
const bump = () => { counter += 2; return counter; };
const clobber = () => { fixed = 2; };
try { clobber(); log.push("no error"); } catch (error) { log.push(error.constructor === TypeError, fixed); }
log.push(bump(), bump(), counter);
// A function declared later, a class binding, a catch parameter and a compound call.
const callsLater = (value) => later(value) + Holder.tag;
function later(value) { return "later:" + value; }
class Holder { static tag = "#"; static make() { return () => Holder.tag + Holder.name; } }
log.push(callsLater(3), Holder.make()());
try { throw "caught"; } catch (reason) { log.push((() => reason)()); }
// An arrow inside an ordinary function reads that function's own and captured cells.
function outer(a) { let b = a + 1; const f = (c) => a + b + c + counter; b += 10; return f; }
log.push(outer(1)(100));
// An arrow under a generator reads through it.
function* generate(seed) { const add = (n) => seed + n + counter; yield add(1); seed += 5; yield add(2); }
log.push([...generate(10)].join(","));
print(log.join(" | "));
"##;
    check(
        source,
        true,
        options(),
        &["old:1 | new:2 | true | late | function | sli5 | 0,0,1,1,2,2 | true | 1 | 2 | 4 | 4 | later:3# | #Holder | caught | 117 | 15,21"],
    );
}
