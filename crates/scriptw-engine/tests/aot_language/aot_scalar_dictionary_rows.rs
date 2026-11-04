use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

#[test]
fn dictionary_row_regions_preserve_live_ownership_aliases_and_generic_prefixes() {
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    for (module, directive) in [(true, ""), (false, ""), (false, "\"use strict\";\n")] {
        let source = format!(
            "{directive}{}",
            include_str!("../fixtures/scalar_dictionary_rows.js")
        );
        let engine = Engine::new(RealmBuilder::new().build());
        let compile = CompileOptions {
            host_surface_policy: HostSurfacePolicy::Test262,
            ..CompileOptions::default()
        };
        let run = RunOptions {
            backend: ExecutionBackend::WasmAot,
            timeout_ms: Some(30_000),
            ..RunOptions::default()
        };
        let result = if module {
            engine.observe_module(&source, compile, run)
        } else {
            engine.observe_script(&source, compile, run)
        }
        .unwrap();
        assert_eq!(result.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(result.completion, ObservedCompletion::Normal(_)),
            "module={module}, directive={directive:?}: {:?}",
            result.completion
        );
        assert_eq!(
            result.output_events,
            [HostOutputEvent::PrintLine(
                "scalar-dictionary-rows:ok".into()
            )]
        );
    }
}

#[test]
fn sloppy_with_rows_keep_dynamic_binding_receivers_and_gets() {
    let source = r#"
function same(actual,expected){if(!Object.is(actual,expected))throw new Error('unexpected with result');}
var reads=0, actual={length:2,name:'ab'}, wrong={length:100,name:'wrong'};
var scope={get array(){reads++;return [actual,actual,actual,actual];}};
function scan(array,seed,environment) {
  var sum=seed;
  with(environment) {
    for(var i=0;i<8;i++){var row=array[i&3];sum+=row.length+row.name.length;}
  }
  return [sum,row];
}
var result=scan([wrong,wrong,wrong,wrong],0,scope);
same(result[0],32);same(result[1],actual);same(reads,8);
print('dictionary-with:ok');
"#;
    scriptw_engine::configure_compilation_jobs(1).unwrap();
    let result = Engine::new(RealmBuilder::new().build())
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
        matches!(result.completion, ObservedCompletion::Normal(_)),
        "{:?}",
        result.completion
    );
    assert_eq!(result.backend_used, ExecutionBackend::WasmAot);
    assert_eq!(
        result.output_events,
        [HostOutputEvent::PrintLine("dictionary-with:ok".into())]
    );
}
