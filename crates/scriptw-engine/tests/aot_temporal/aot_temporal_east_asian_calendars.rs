use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, ObservedJsValue, ObservedNumber, RealmBuilder, RunOptions,
};

fn assert_modes(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("bounded compiler jobs");
    for directive in ["", "'use strict';\n"] {
        let source = format!("{directive}{source}");
        let observed = Engine::new(RealmBuilder::new().build())
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
            .unwrap_or_else(|error| {
                panic!("East Asian calendar control failed: {error}\n{source}")
            });
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(262.0))),
            "{source}"
        );
        assert_eq!(
            observed.output_events,
            vec![HostOutputEvent::PrintLine(expected.into())],
            "{source}"
        );
    }
}

#[test]
fn east_asian_projection_preserves_observations_and_retained_model_joins() {
    assert_modes(
        include_str!("../fixtures/temporal_east_asian_calendars/projection.js"),
        "east-asian-projection:ok",
    );
}

#[test]
fn east_asian_fields_preserve_code_agreement_overflow_and_hook_order() {
    assert_modes(
        include_str!("../fixtures/temporal_east_asian_calendars/fields.js"),
        "east-asian-fields:ok",
    );
}

#[test]
fn east_asian_arithmetic_uses_real_serials_and_both_virtual_comparisons() {
    assert_modes(
        include_str!("../fixtures/temporal_east_asian_calendars/arithmetic.js"),
        "east-asian-arithmetic:ok",
    );
}

#[test]
fn east_asian_partial_dates_use_complete_reference_table_and_year_codes() {
    assert_modes(
        include_str!("../fixtures/temporal_east_asian_calendars/partial_dates.js"),
        "east-asian-partial-dates:ok",
    );
}

#[test]
fn east_asian_duration_only_source_keeps_plain_and_zoned_calendar_paths() {
    assert_modes(
        include_str!("../fixtures/temporal_east_asian_calendars/relative_duration.js"),
        "east-asian-relative-duration:ok",
    );
}

#[test]
fn east_asian_limits_and_errors_use_native_year_and_called_realm() {
    assert_modes(
        include_str!("../fixtures/temporal_east_asian_calendars/limits_and_realms.js"),
        "east-asian-limits-and-realms:ok",
    );
}

#[test]
fn east_asian_rows_initialize_after_gc_in_independent_stores_and_data_orders() {
    use scriptw_engine::{EmbeddedEvaluation, EmbeddedRuntime, JsCompletion};

    scriptw_engine::configure_compilation_jobs(1).expect("bounded compiler jobs");
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_module(
            r#"
function check(value, message) {
  if (!value) { print('calendar-data failure: ' + message); throw new Error(message); }
}
const foreign = __scriptwCreateRealm().global;
function make(DateClass, calendar, year) {
  return DateClass.from({calendar, year, monthCode:'M01', day:1}, {overflow:'reject'});
}
function calendarChecks() {
  for (const calendar of ['chinese','dangi']) {
    for (const DateClass of [Temporal.PlainDate,foreign.Temporal.PlainDate]) {
      const modern = DateClass.from('2012-04-23[u-ca=' + calendar + ']');
      check(modern.monthCode === (calendar === 'chinese' ? 'M04' : 'M03L'), 'distinct retained calendar rows');
      check(Object.getPrototypeOf(modern) === DateClass.prototype, 'fresh defining Realm result');
      for (const year of [-10000,-3654,-3653,4703,4704,10000]) {
        const first = make(DateClass, calendar, year), count = first.monthsInYear;
        check(first.year === year && (count === 12 || count === 13), 'retained or astronomy year');
        check(first.withCalendar('iso8601').year === year, 'related year owns its New Year');
        const next = first.add({months:count});
        check(next.year === year + 1 && next.monthCode === 'M01' && next.day === 1, 'month serial crosses table/model boundary');
        check(next.subtract({months:count}).equals(first), 'inverse month serial after table publication');
      }
    }
  }
  return true;
}
export function retainedChinese() {
  const date = Temporal.PlainDate.from({calendar:'chinese',year:1999,monthCode:'M11',day:25});
  check(date.withCalendar('iso8601').toString() === '2000-01-01', 'Chinese retained-year first reader');
  return calendarChecks();
}
export function retainedDangi() {
  const date = foreign.Temporal.PlainDate.from({calendar:'dangi',year:1999,monthCode:'M11',day:25});
  check(date.withCalendar('iso8601').toString() === '2000-01-01', 'Dangi retained-year first reader');
  return calendarChecks();
}
export function lowerChinese() {
  // The origin is outside the retained interval. Its first table use is the
  // serial search reached by adding into -3653, rather than a retained year read.
  const before = make(Temporal.PlainDate, 'chinese', -3654);
  const next = before.add({months:before.monthsInYear});
  check(next.year === -3653 && next.monthCode === 'M01' && next.day === 1, 'lower astronomy-to-retained serial first reader');
  return calendarChecks();
}
export function upperDangi() {
  const after = make(foreign.Temporal.PlainDate, 'dangi', 4704);
  const previous = after.subtract({months:1});
  check(previous.year === 4703 && previous.month === previous.monthsInYear && previous.day === 1, 'upper astronomy-to-retained serial first reader');
  check(previous.add({months:1}).equals(after), 'upper model join round trip');
  return calendarChecks();
}
export function normalization() {
  check('A\u030a'.normalize('NFC') === '\u00c5', 'local normalization image');
  check(foreign.String.prototype.normalize.call('\u00e9','NFD') === 'e\u0301', 'foreign normalization image');
  check('\ufb03'.normalize('NFKC') === 'ffi', 'compatibility normalization image');
  return true;
}
function propertySource(name) {
  const units = [94,92,112,123];
  for (let i=0; i<name.length; i++) units.push(name.charCodeAt(i));
  units.push(125,36);
  let source = '';
  for (const unit of units) source += String.fromCharCode(unit);
  return source;
}
export function properties() {
  const greek = new foreign.RegExp(propertySource('Script=Greek'), 'u');
  const upper = new RegExp(propertySource('Uppercase_Letter'), 'u');
  check(greek.test('\u03a9') && !greek.test('A'), 'computed foreign property catalogue');
  check(upper.test('A') && !upper.test('a'), 'computed local property catalogue');
  return true;
}
"#,
            CompileOptions {
                host_surface_policy: HostSurfacePolicy::Test262,
                ..CompileOptions::default()
            },
        )
        .expect("calendar/data-order module compiles");
    let artifact = compiler.emit_wasm(&unit).expect("real P/R module emits");
    let mut config = wasmtime::Config::new();
    scriptw_engine::configure_wasm_runtime(&mut config);
    let engine = wasmtime::Engine::new(&config).expect("required experimental Wasmtime features");
    let runtime =
        EmbeddedRuntime::compile(&engine, &artifact).expect("one retained native P/R owner");
    let orders = [
        ["retainedChinese", "normalization", "properties"],
        ["properties", "normalization", "retainedDangi"],
        ["lowerChinese", "properties", "normalization"],
        ["normalization", "properties", "upperDangi"],
    ];
    // Reuse the actual compiled Modules, but never their mutable globals or
    // linear memory. Each instance has a fresh entry and created Realm.
    for repeat in 0..2 {
        for order in orders {
            let state = runtime.new_state(RealmBuilder::new().build(), ()).unwrap();
            let mut store = wasmtime::Store::new(&engine, state);
            store.limiter(|state| state.limits_mut());
            store.set_epoch_deadline(u64::MAX / 2);
            let mut linker = wasmtime::Linker::new(&engine);
            runtime.register(&mut store, &mut linker).unwrap();
            let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
            assert!(matches!(
                instance.initialize(&mut store).unwrap(),
                EmbeddedEvaluation::Settled(JsCompletion::Normal(_))
            ));
            // Module entry only declares the consumers and creates a Realm;
            // no calendar/normalization/property operation has run yet.
            store
                .gc(None)
                .expect("actual Store collection before first data use");
            for round in 0..2 {
                for name in order {
                    let callable = match instance.get_export(&mut store, name).unwrap() {
                        JsCompletion::Normal(value) => value,
                        JsCompletion::Throw(value) => panic!("export {name} threw: {value:?}"),
                    };
                    store
                        .gc(None)
                        .expect("retain callable across actual Store GC");
                    let result = instance.call(&mut store, &callable, None, &[]).unwrap();
                    assert!(
                        matches!(&result, JsCompletion::Normal(value) if value.as_boolean() == Some(true)),
                        "repeat {repeat}, order {order:?}, round {round}, {name}: {result:?}; output: {:?}",
                        instance.take_output(&mut store).unwrap(),
                    );
                }
            }
            assert!(instance.take_output(&mut store).unwrap().is_empty());
        }
    }
}
