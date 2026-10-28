use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, RealmBuilder, RunOptions,
};

fn assert_bound_format_realm(source: &str) {
    crate::configure_compilation_jobs();
    let observation = Engine::new(RealmBuilder::new().build())
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
        .expect("Intl bound format Realm regression must execute through Wasm AOT");
    assert!(
        matches!(observation.completion, ObservedCompletion::Normal(_)),
        "{:?}\noutput: {:?}\n{source}",
        observation.completion,
        observation.output_events,
    );
    assert_eq!(
        observation.output_events,
        vec![HostOutputEvent::PrintLine("true".to_string())],
        "{source}",
    );
}

#[test]
fn bound_format_uses_its_getters_realm_and_preserves_function_descriptors() {
    assert_bound_format_realm(
        r#"
var foreign = __scriptwCreateRealm().global;
var constructors = [Intl.DateTimeFormat, foreign.Intl.DateTimeFormat];
var prototypes = [Function.prototype, foreign.Function.prototype];
for (var i = 0; i < constructors.length; i++) {
  var formatter = new constructors[i]('en', {timeZone:'UTC'});
  var format = formatter.format;
  if (Object.getPrototypeOf(format) !== prototypes[i] || format !== formatter.format) throw 'Realm or cache';
  if (format.name !== '' || format.length !== 1 || Object.prototype.hasOwnProperty.call(format,'prototype')) throw 'function shape';
  for (var name of ['name','length']) {
    var descriptor = Object.getOwnPropertyDescriptor(format,name);
    if (descriptor.writable || descriptor.enumerable || !descriptor.configurable) throw 'function descriptor';
  }
  var output = format(0);
  if (typeof output !== 'string' || output.length === 0 || format.call(null,0) !== output || format.call({},0) !== output) throw 'captured formatter';
  try { new format(0); throw 'accepted construction'; }
  catch (error) { if (!(error instanceof TypeError)) throw error; }
}
print(true);
"#,
    );
}

#[test]
fn borrowed_format_getter_realm_wins_only_on_first_materialization() {
    assert_bound_format_realm(
        r#"
var foreign = __scriptwCreateRealm().global;
var third = __scriptwCreateRealm().global;
var localGetter = Object.getOwnPropertyDescriptor(Intl.DateTimeFormat.prototype,'format').get;
var foreignGetter = Object.getOwnPropertyDescriptor(foreign.Intl.DateTimeFormat.prototype,'format').get;
var thirdGetter = Object.getOwnPropertyDescriptor(third.Intl.DateTimeFormat.prototype,'format').get;
var localFormatter = new Intl.DateTimeFormat('en', {timeZone:'UTC'});
var foreignFormatter = new foreign.Intl.DateTimeFormat('en', {timeZone:'UTC'});
var foreignFormat = foreignGetter.call(localFormatter);
var localFormat = localGetter.call(foreignFormatter);
if (Object.getPrototypeOf(foreignFormat) !== foreign.Function.prototype ||
    Object.getPrototypeOf(localFormat) !== Function.prototype) throw 'borrowed getter Realm';
if (localGetter.call(localFormatter) !== foreignFormat ||
    thirdGetter.call(localFormatter) !== foreignFormat ||
    foreignGetter.call(foreignFormatter) !== localFormat ||
    thirdGetter.call(foreignFormatter) !== localFormat) throw 'cached identity changed';
if (foreignFormat(0) !== localFormat(0)) throw 'borrowed capture';
var fresh = new Intl.DateTimeFormat('en', {timeZone:'UTC'});
var canonical = third.Function.prototype;
Object.setPrototypeOf(thirdGetter, null);
third.Intl = null;
third.Function = null;
var format = Reflect.apply(thirdGetter, fresh, []);
if (Object.getPrototypeOf(format) !== canonical || typeof format(0) !== 'string') throw 'mutable public Realm source';
print(true);
"#,
    );
}

#[test]
fn bound_format_errors_and_abrupt_coercion_use_the_cached_function_realm() {
    assert_bound_format_realm(
        r#"
var foreign = __scriptwCreateRealm().global;
var localGetter = Object.getOwnPropertyDescriptor(Intl.DateTimeFormat.prototype,'format').get;
var foreignGetter = Object.getOwnPropertyDescriptor(foreign.Intl.DateTimeFormat.prototype,'format').get;
var formatter = new Intl.DateTimeFormat('en', {timeZone:'UTC'});
var foreignFormat = foreignGetter.call(formatter);
var localFormat = localGetter.call(new foreign.Intl.DateTimeFormat('en', {timeZone:'UTC'}));
if (localGetter.call(formatter) !== foreignFormat) throw 'cache';
function throwsWithPrototype(format, input, prototype) {
  try { format(input); throw 'accepted invalid input'; }
  catch (error) { if (Object.getPrototypeOf(error) !== prototype) throw 'error Realm'; }
}

for (var input of [NaN, Infinity, -Infinity]) {
  throwsWithPrototype(foreignFormat, input, foreign.RangeError.prototype);
  throwsWithPrototype(localFormat, input, RangeError.prototype);
}
for (var input of [Symbol('time'), 1n, {[Symbol.toPrimitive]: function(){return Symbol('time');}}]) {
  throwsWithPrototype(foreignFormat, input, foreign.TypeError.prototype);
  throwsWithPrototype(localFormat, input, TypeError.prototype);
}
var marker = Symbol('coercion');
var trace = '';
var abrupt = {[Symbol.toPrimitive]: function(hint){trace += hint; throw marker;}};
try { foreignFormat(abrupt); throw 'lost abrupt completion'; }
catch (error) { if (error !== marker) throw 'changed thrown value'; }
if (trace !== 'number' || typeof foreignFormat(0) !== 'string') throw 'coercion order or retained capture';
print(true);
"#,
    );
}

#[test]
fn eager_intl_getters_keep_descriptors_identity_and_defining_realm_across_gc() {
    assert_bound_format_realm(
        r#"
function check(value, message) { if (!value) { print(message); throw new Error(message); } }
var foreign=__scriptwCreateRealm().global,second=__scriptwCreateRealm().global;
var namespace=foreign.Intl,canonicalFunction=foreign.Function.prototype,canonicalTypeError=foreign.TypeError.prototype;
var localeNames=['language','script','region','baseName','calendar','collation','firstDayOfWeek','hourCycle','caseFirst','numeric','numberingSystem','variants'];
var rows=localeNames.map(function(name){return ['Locale',name];});
rows.push(['DateTimeFormat','format'],['NumberFormat','format'],['Collator','compare']);
var saved=[];
for(var row of rows) {
  var constructor=row[0],name=row[1],prototype=namespace[constructor].prototype;
  var descriptor=Object.getOwnPropertyDescriptor(prototype,name);
  var local=Object.getOwnPropertyDescriptor(Intl[constructor].prototype,name).get;
  var other=Object.getOwnPropertyDescriptor(second.Intl[constructor].prototype,name).get;
  check(typeof descriptor.get==='function' && descriptor.set===undefined && !descriptor.enumerable && descriptor.configurable && !('value' in descriptor) && !('writable' in descriptor),'real Intl getter descriptor: '+name);
  var getter=descriptor.get;
  check(getter!==local && getter!==other && Object.getPrototypeOf(getter)===canonicalFunction,'fresh eager getter defining Realm: '+name);
  check(getter.name==='get '+name && getter.length===0 && !Object.prototype.hasOwnProperty.call(getter,'prototype'),'getter callable metadata: '+name);
  for(var key of ['name','length']) {
    var metadata=Object.getOwnPropertyDescriptor(getter,key);
    check(!metadata.writable && !metadata.enumerable && metadata.configurable,'getter metadata descriptor: '+name+'/'+key);
  }
  saved.push({constructor:constructor,name:name,prototype:prototype,getter:getter,local:local});
}
check(saved.length===15,'complete represented Intl getter inventory');
var options={calendar:'gregory',collation:'phonebk',firstDayOfWeek:'mon',hourCycle:'h23',caseFirst:'upper',numeric:true,numberingSystem:'latn',variants:'1901'};
var locale=new Intl.Locale('de-Latn-AT',options),foreignLocale=new namespace.Locale('de-Latn-AT',options);
var receivers={DateTimeFormat:new Intl.DateTimeFormat('en',{timeZone:'UTC'}),NumberFormat:new Intl.NumberFormat('en'),Collator:new Intl.Collator('en')};
foreign.Intl=null;foreign.Function=null;gc();
for(var entry of saved) {
  check(Object.getOwnPropertyDescriptor(entry.prototype,entry.name).get===entry.getter,'getter identity after GC: '+entry.name);
  var thrown=undefined;
  try{entry.getter.call({});}catch(error){thrown=error;}
  check(thrown && Object.getPrototypeOf(thrown)===canonicalTypeError,'brand failure keeps defining Realm: '+entry.name);
  if(entry.constructor==='Locale') {
    check(Object.is(entry.getter.call(locale),entry.local.call(locale)) && Object.is(entry.getter.call(foreignLocale),entry.local.call(locale)),'borrowed Locale getter reads original slots: '+entry.name);
  } else {
    var receiver=receivers[entry.constructor],bound=entry.getter.call(receiver);gc();
    check(bound===entry.getter.call(receiver) && bound===entry.local.call(receiver) && Object.getPrototypeOf(bound)===canonicalFunction,'first bound callable keeps getter Realm and cached identity: '+entry.constructor);
    check(entry.constructor==='Collator' ? bound('a','b')<0 : typeof bound(0)==='string','bound callable retains its receiver: '+entry.constructor);
  }
}
var language=saved[0],original=language.getter,replacement=function replacement(){return 'replacement';};
Object.defineProperty(language.prototype,'language',{get:replacement});gc();
check(Object.getOwnPropertyDescriptor(language.prototype,'language').get===replacement && original.call(foreignLocale)==='de','descriptor replacement cannot mutate the original getter');
check(Object.getOwnPropertyDescriptor(second.Intl.Locale.prototype,'language').get!==replacement && second.Intl.Locale.prototype!==language.prototype,'descriptor mutation remains Realm-local');
print(true);
"#,
    );
}
