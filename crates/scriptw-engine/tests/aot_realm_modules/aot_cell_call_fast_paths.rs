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
