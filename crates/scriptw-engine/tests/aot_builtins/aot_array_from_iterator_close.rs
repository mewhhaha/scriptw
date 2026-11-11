use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, ObservedCompletion, RealmBuilder,
    RunOptions,
};

fn assert_array_from_trace(source: &str, expected: &[&str]) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{FAILURE_HELPERS}\n{source}");
        let outcome = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &source,
                CompileOptions::default(),
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(30_000),
                    ..RunOptions::default()
                },
            )
            .expect("Array.from close control must compile and execute through Wasm AOT");
        assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
        assert!(
            matches!(outcome.completion, ObservedCompletion::Normal(_)),
            "{:?}",
            outcome.completion
        );
        let expected = expected
            .iter()
            .map(|line| HostOutputEvent::PrintLine((*line).to_string()))
            .collect::<Vec<_>>();
        assert_eq!(outcome.output_events, expected, "source:\n{source}");
    }
}

const FAILURE_HELPERS: &str = r#"
function failureTrace(stage, closeMode) {
  var trace = [];
  var original = { stage: stage };
  var closeError = { stage: 'close' };
  var iterator = {
    next: function () {
      trace.push('next');
      if (stage === 'next') throw original;
      return {
        get done() {
          trace.push('done');
          if (stage === 'done') throw original;
          return false;
        },
        get value() {
          trace.push('value');
          if (stage === 'value') throw original;
          return 7;
        }
      };
    }
  };
  Object.defineProperty(iterator, 'return', {
    get: function () {
      trace.push('get:return');
      if (closeMode === 'get-throw') throw closeError;
      if (closeMode === 'non-callable') return {};
      if (closeMode === 'undefined') return undefined;
      return function () {
        trace.push('return:' + (this === iterator));
        if (closeMode === 'call-throw') throw closeError;
        return closeMode === 'primitive' ? 0 : {};
      };
    }
  });
  var input = {};
  Object.defineProperty(input, Symbol.iterator, {
    get: function () {
      trace.push('get:iterator');
      return function () {
        trace.push('iterator');
        return iterator;
      };
    }
  });
  function Constructor() {
    trace.push('construct:' + arguments.length);
    return new Proxy({}, {
      defineProperty: function (target, key, descriptor) {
        trace.push('define:' + key + ':' + descriptor.value + ':' +
                   descriptor.writable + ':' + descriptor.enumerable + ':' +
                   descriptor.configurable);
        throw original;
      }
    });
  }
  function mapper(value, index) {
    trace.push('map:' + value + ':' + index);
    throw original;
  }
  var caught;
  try {
    Array.from.call(Constructor, input, stage === 'map' ? mapper : undefined);
  } catch (error) {
    caught = error;
    trace.push('caught:' + (error === original));
  } finally {
    trace.push('finally');
  }
  if (caught !== original) throw new Error('original completion was replaced');
  print(stage + '/' + closeMode + ':' + trace.join(','));
}

function arrayLikeFailureTrace(stage) {
  var trace = [];
  var original = { stage: stage };
  var input = {
    get length() { trace.push('length'); return 1; },
    get 0() {
      trace.push('get:0');
      if (stage === 'get') throw original;
      return 7;
    },
    get return() { throw new Error('array-like input must not close'); }
  };
  Object.defineProperty(input, Symbol.iterator, {
    get: function () { trace.push('get:iterator'); return undefined; }
  });
  function Constructor(length) {
    trace.push('construct:' + arguments.length + ':' + length);
    return new Proxy({}, {
      defineProperty: function (target, key, descriptor) {
        trace.push('define:' + key + ':' + descriptor.value);
        throw original;
      }
    });
  }
  function mapper(value, index) {
    trace.push('map:' + value + ':' + index);
    throw original;
  }
  var caught;
  try {
    Array.from.call(Constructor, input, stage === 'map' ? mapper : undefined);
  } catch (error) {
    caught = error;
    trace.push('caught:' + (error === original));
  }
  if (caught !== original) throw new Error('array-like completion was replaced');
  print('array-like/' + stage + ':' + trace.join(','));
}
"#;

#[test]
fn proxy_property_definition_throw_closes_once_before_outer_catch_and_finally() {
    assert_array_from_trace(
        "failureTrace('define', 'normal');",
        &[
            "define/normal:get:iterator,construct:0,iterator,next,done,value,define:0:7:true:true:true,get:return,return:true,caught:true,finally",
        ],
    );
}

#[test]
fn proxy_property_definition_throw_keeps_precedence_over_return_failures() {
    assert_array_from_trace(
        r#"
failureTrace('define', 'get-throw');
failureTrace('define', 'non-callable');
failureTrace('define', 'call-throw');
failureTrace('define', 'primitive');
failureTrace('define', 'undefined');
"#,
        &[
            "define/get-throw:get:iterator,construct:0,iterator,next,done,value,define:0:7:true:true:true,get:return,caught:true,finally",
            "define/non-callable:get:iterator,construct:0,iterator,next,done,value,define:0:7:true:true:true,get:return,caught:true,finally",
            "define/call-throw:get:iterator,construct:0,iterator,next,done,value,define:0:7:true:true:true,get:return,return:true,caught:true,finally",
            "define/primitive:get:iterator,construct:0,iterator,next,done,value,define:0:7:true:true:true,get:return,return:true,caught:true,finally",
            "define/undefined:get:iterator,construct:0,iterator,next,done,value,define:0:7:true:true:true,get:return,caught:true,finally",
        ],
    );
}

#[test]
fn mapper_throw_closes_without_defining_any_target_property() {
    assert_array_from_trace(
        r#"
failureTrace('map', 'normal');
failureTrace('map', 'call-throw');
"#,
        &[
            "map/normal:get:iterator,construct:0,iterator,next,done,value,map:7:0,get:return,return:true,caught:true,finally",
            "map/call-throw:get:iterator,construct:0,iterator,next,done,value,map:7:0,get:return,return:true,caught:true,finally",
        ],
    );
}

#[test]
fn iterator_next_done_and_value_errors_propagate_without_requesting_close() {
    assert_array_from_trace(
        r#"
failureTrace('next', 'normal');
failureTrace('done', 'normal');
failureTrace('value', 'normal');
"#,
        &[
            "next/normal:get:iterator,construct:0,iterator,next,caught:true,finally",
            "done/normal:get:iterator,construct:0,iterator,next,done,caught:true,finally",
            "value/normal:get:iterator,construct:0,iterator,next,done,value,caught:true,finally",
        ],
    );
}

#[test]
fn noniterable_array_like_failures_do_not_request_iterator_close() {
    assert_array_from_trace(
        r#"
arrayLikeFailureTrace('define');
arrayLikeFailureTrace('map');
arrayLikeFailureTrace('get');
"#,
        &[
            "array-like/define:get:iterator,length,construct:1:1,get:0,define:0:7,caught:true",
            "array-like/map:get:iterator,length,construct:1:1,get:0,map:7:0,caught:true",
            "array-like/get:get:iterator,length,construct:1:1,get:0,caught:true",
        ],
    );
}

#[test]
fn successful_proxy_target_definitions_continue_iteration_without_close() {
    assert_array_from_trace(
        r#"
var trace = [];
var index = 0;
var iterator = {
  next: function () {
    trace.push('next:' + index);
    return index < 2 ? { value: ++index, done: false } : { done: true };
  },
  get return() { throw new Error('completed iteration must not close'); }
};
var input = {};
Object.defineProperty(input, Symbol.iterator, {
  get: function () {
    trace.push('get:iterator');
    return function () { trace.push('iterator'); return iterator; };
  }
});
function Constructor() {
  trace.push('construct:' + arguments.length);
  return new Proxy({}, {
    defineProperty: function (target, key, descriptor) {
      trace.push('define:' + key + ':' + descriptor.value);
      Object.defineProperty(target, key, descriptor);
      return true;
    },
    set: function (target, key, value) {
      trace.push('set:' + key + ':' + value);
      target[key] = value;
      return true;
    }
  });
}
var result = Array.from.call(Constructor, input, function (value, index) {
  trace.push('map:' + value + ':' + index);
  return value + 10;
});
if (result[0] !== 11 || result[1] !== 12 || result.length !== 2)
  throw new Error('successful target values');
print(trace.join(','));
"#,
        &[
            "get:iterator,construct:0,iterator,next:0,map:1:0,define:0:11,next:1,map:2:1,define:1:12,next:2,set:length:2",
        ],
    );
}
