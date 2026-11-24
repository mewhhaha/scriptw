//! Native source remains parsed/lowered/emitted while compiled code shares fresh data.
use scriptw_engine::{
    CompileOptions, Engine, ObservedCompletion, ObservedJsValue, ObservedNumber, RealmBuilder,
    RunOptions,
};
fn succeeds(source: &str) {
    let engine = Engine::new(RealmBuilder::new().build());
    let outcome = engine
        .observe_script(
            source,
            CompileOptions::default(),
            RunOptions {
                timeout_ms: Some(60_000),
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(
        outcome.completion,
        ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(1.0)))
    );
}
#[test]
fn direct_eval_template_data_keeps_source_context_regex_and_fresh_object_identity() {
    succeeds(
        r#"
      var token = 'template_' + Date.now();
      function invoke(source) { let secret = 17; return eval(source); }
      for (var i = 0; i < 4; i++) {
        if (invoke('secret + 1 /* ' + token + i + ' */') !== 18) throw new Error('caller context');
        var a = invoke('/' + token + i + '/');
        var b = invoke('/' + token + i + '/');
        if (a === b || a.source !== token + i || b.source !== token + i) throw new Error('regex data/identity');
        a.lastIndex = 7;
        if (b.lastIndex !== 0) throw new Error('distinct regex state');
      }
      1;
    "#,
    );
}
#[test]
fn function_template_data_preserves_each_to_string_source_and_later_calls() {
    succeeds(
        r#"
      var token = 'template_' + Date.now();
      var a = Function('return "' + token + '_first";');
      var b = Function('return "' + token + '_other";');
      if (a() !== token + '_first' || b() !== token + '_other' || a() !== token + '_first') throw new Error('closure data');
      if (a.toString().indexOf(token + '_first') < 0 || b.toString().indexOf(token + '_other') < 0) throw new Error('source identity');
      1;
    "#,
    );
}

#[test]
fn literal_table_lengths_and_ordinals_remain_owned_by_each_native_program() {
    succeeds(
        r#"
      var a = Function('var value = "first"; return "first";');
      var b = Function('var value = "second"; return "third";');
      var c = Function('var value = "fourth"; return "fourth";');
      for (var i = 0; i < 3; i++) {
        if (a() !== 'first' || b() !== 'third' || c() !== 'fourth') throw new Error('P literal ownership');
      }
      if (a.toString().indexOf('first') < 0 || b.toString().indexOf('third') < 0) throw new Error('P source ownership');
      var one = Function('return 1;'), two = Function('return 2;');
      if (one() !== 1 || two() !== 2 || one() !== 1) throw new Error('numeric code identity');
      function evaluate(text) { return eval(text); }
      if (evaluate('var text="eval-first"; "eval-first";') !== 'eval-first') throw new Error('eval first');
      if (evaluate('var text="eval-second"; "eval-third";') !== 'eval-third') throw new Error('eval third');
      if (evaluate('var text="eval-fourth"; "eval-fourth";') !== 'eval-fourth') throw new Error('eval fourth');
      1;
    "#,
    );
}
#[test]
fn global_script_template_data_uses_actual_realm_and_keeps_each_payload() {
    succeeds(
        r#"
      var token = 'template_' + Date.now();
      var indirect = eval;
      indirect('globalThis["' + token + '_first"] = 13;');
      indirect('globalThis["' + token + '_other"] = 29;');
      if (globalThis[token + '_first'] !== 13 || globalThis[token + '_other'] !== 29) throw new Error('global data');
      1;
    "#,
    );
}

#[path = "aot_native_program_templates/annex_b_arguments.rs"]
mod annex_b_arguments;
#[path = "aot_native_program_templates/catalog_overlays.rs"]
mod catalog_overlays;
#[path = "aot_native_program_templates/program_owned_payload.rs"]
mod program_owned_payload;
#[path = "aot_native_program_templates/repeated_identifier_reads.rs"]
mod repeated_identifier_reads;
#[path = "aot_native_program_templates/shared_function_bodies.rs"]
mod shared_function_bodies;
#[path = "aot_native_program_templates/unobserved_arguments.rs"]
mod unobserved_arguments;

#[path = "aot_native_program_templates/empty_eval.rs"]
mod empty_eval;
#[path = "aot_native_program_templates/function_names.rs"]
mod function_names;
#[path = "aot_native_program_templates/lazy_source_text.rs"]
mod lazy_source_text;
#[path = "aot_native_program_templates/regexp_literals.rs"]
mod regexp_literals;
