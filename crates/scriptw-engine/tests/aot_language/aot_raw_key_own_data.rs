use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostSurfacePolicy, RealmBuilder, RunOptions,
};

fn check(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
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
        .unwrap_or_else(|error| panic!("raw-key own-data execution failed: {error}\n{source}"));
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        outcome.note.contains("boolean(true)"),
        "{}\n{source}",
        outcome.note
    );
}

#[test]
fn computed_string_reads_keep_live_values_after_promotion_and_gc() {
    check(
        r#"
function assert(value) { if (!value) throw new Error('raw-key data'); }
function read(object,key) { return object[key]; }
var marker = {}, other = {}, row = {x: marker, y: -0, empty: undefined};
var keys = ['x','y','prefix-x'.slice(7),String.fromCharCode(121)];
for (var i=0;i<16;i++) {
  assert(read(row,keys[i&3]) === row[keys[i&3]]);
}
assert(read(row,'empty') === undefined && Object.hasOwn(row,'empty'));
row.x = other; gc(); assert(read(row,keys[2]) === other);
row.extra = marker;
Object.defineProperty(row,'x',{configurable:true,get(){ gc(); return marker; }});
assert(read(row,keys[2]) === marker);
delete row.extra; delete row.x;
Object.defineProperty(row,'x',{value:other,writable:true,configurable:true});
gc(); assert(read(row,keys[2]) === other && Object.is(read(row,keys[3]),-0));
var symbol = Symbol('x'); row[symbol] = marker;
assert(read(row,symbol) === marker && read(row,'missing') === undefined);
var inherited = Object.create({get x(){ assert(this === inherited); gc(); return marker; }});
assert(read(inherited,keys[2]) === marker);
true;
"#,
    );
}

#[test]
fn numeric_array_reads_keep_holes_accessors_proxy_receivers_and_index_boundaries() {
    check(
        r#"
function assert(value) { if (!value) throw new Error('raw-key array'); }
function read(object,key) { return object[key]; }
var marker = {}, values = [marker,undefined,-0,NaN];
values['-0'] = 'string-minus-zero'; values['1.5'] = 'fraction'; values['4294967295'] = 'limit';
assert(read(values,-0) === marker && read(values,'-0') === 'string-minus-zero');
assert(read(values,1) === undefined && Object.hasOwn(values,'1'));
assert(Object.is(read(values,2),-0) && Number.isNaN(read(values,3)));
assert(read(values,1.5) === 'fraction' && read(values,4294967295) === 'limit');
values.NaN = 'nan'; values.Infinity = 'infinity';
assert(read(values,NaN) === 'nan' && read(values,Infinity) === 'infinity');
var prototype = Object.create(Array.prototype), holes = [1,,3], gets = 0;
Object.defineProperty(prototype,'1',{get(){ gets++; assert(this === holes); gc(); return marker; }});
Object.setPrototypeOf(holes,prototype);
assert(read(holes,1) === marker && gets === 1);
Object.defineProperty(values,'0',{configurable:true,get(){ gets++; gc(); return marker; }});
assert(read(values,0) === marker && gets === 2);
var proxy;
proxy = new Proxy(values,{get(target,key,receiver){ assert(receiver === proxy && key === '0'); gets++; gc(); return marker; }});
assert(read(proxy,0) === marker && gets === 3);
var revocable = Proxy.revocable(values,{}); revocable.revoke();
var rejected = false;
try { read(revocable.proxy,0); } catch(error) { rejected = error instanceof TypeError; }
assert(rejected);
true;
"#,
    );
}

#[test]
fn guard_misses_preserve_operand_key_conversion_and_throw_order() {
    check(
        r#"
function assert(value) { if (!value) throw new Error('raw-key order'); }
var trace = '', marker = {}, row = {x:1};
function hitTarget(){ trace += 'target|'; return row; }
function hitKey(){ trace += 'key|'; return String.fromCharCode(120); }
assert(hitTarget()[hitKey()] === 1 && trace === 'target|key|');
trace = '';
var packed = [marker];
function hitArray(){ trace += 'target|'; return packed; }
function hitIndex(){ trace += 'key|'; return -0; }
assert(hitArray()[hitIndex()] === marker && trace === 'target|key|');
trace = '';
var key = {[Symbol.toPrimitive](hint){ trace += 'coerce|'; assert(hint === 'string'); row.x = marker; gc(); return 'x'; }};
function target(){trace += 'target|'; return row;}
function operand(){trace += 'key|'; return key;}
assert(target()[operand()] === marker && trace === 'target|key|coerce|');
trace = '';
function nullTarget(){trace += 'target|'; return null;}
var threw = false;
try { nullTarget()[operand()]; } catch(error) { threw = error instanceof TypeError; }
assert(threw && trace === 'target|key|');
trace = ''; var thrown = {};
key[Symbol.toPrimitive] = function(){trace += 'coerce|'; gc(); throw thrown;};
var caught = false;
try { target()[operand()]; } catch(error) { caught = error === thrown; }
assert(caught && trace === 'target|key|coerce|');
trace = '';
function throwingKey(){trace += 'key|'; throw thrown;}
caught = false;
try { nullTarget()[throwingKey()]; } catch(error) { caught = error === thrown; }
assert(caught && trace === 'target|key|');
trace = ''; var revoked = Proxy.revocable(row,{}); revoked.revoke();
key[Symbol.toPrimitive] = function(){trace += 'coerce|'; return 'x';};
caught = false;
try { revoked.proxy[key]; } catch(error) { caught = error instanceof TypeError; }
assert(caught && trace === 'coerce|');
var receiver, reads = 0;
Object.defineProperty(Number.prototype,'rawRead',{configurable:true,get:function(){'use strict'; receiver = this; reads++; gc(); return marker;}});
function read(object,key){return object[key];}
assert(read(7,'rawRead') === marker && receiver === 7 && reads === 1);
delete Number.prototype.rawRead;
true;
"#,
    );
}

#[test]
fn indexed_callees_keep_this_holes_accessors_proxies_and_key_order() {
    // `steps[k](row)`: a packed element of an actual Array is the callee and
    // the Array is `this`; every other base or element takes the complete Get
    // with the same base and key, before the arguments.
    check(
        r#"
function assert(value, message) { if (!value) throw new Error(message); }
var steps = [function (row) { return [this, row]; }, function (row) { return row + 1; }];
function call0(row) { return steps[0](row); }
function callK(k, row) { var result = steps[k](row); return result; }
for (var i = 0; i < 8; i++) {
  var pair = call0(i);
  assert(pair[0] === steps && pair[1] === i, 'constant index this');
  assert(callK(1, i) === i + 1, 'variable index');
  assert(callK(0, i)[0] === steps, 'variable index this');
}
// A hole reads through the prototype chain with the Array as receiver.
var trace = '';
Object.defineProperty(Array.prototype, 2, {
  configurable: true,
  get: function () { trace += 'get;'; assert(this === steps, 'getter receiver'); return function () { return this; }; }
});
steps.length = 3;
assert(callK(2, 0) === steps && trace === 'get;', 'hole getter');
delete Array.prototype[2];
// An accessor element runs before the arguments are evaluated.
Object.defineProperty(steps, 1, { configurable: true, get: function () { trace += 'element;'; return function (v) { return v; }; } });
function callArgument(k) { return steps[k]((trace += 'argument;', 5)); }
assert(callArgument(1) === 5 && trace === 'get;element;argument;', 'element before arguments');
// Non-index keys and non-Array bases.
var object = { 0: function () { return this; }, '1.5': function () { return 15; } };
var bases = [object, 'xy', new Proxy([function () { return 'proxied'; }], {})];
steps = object;
assert(call0() === object && callK(1.5) === 15 && callK('0') === object, 'ordinary object');
steps = bases[2];
assert(call0() === 'proxied', 'proxy base');
steps = 'xy';
try { call0(); assert(false, 'string element is not callable'); } catch (error) { assert(error instanceof TypeError, 'string element call'); }
steps = undefined;
var keyRead = false;
try { callK({ toString: function () { keyRead = true; return '0'; } }); assert(false, 'undefined base'); }
catch (error) { assert(error instanceof TypeError && !keyRead, 'nullish base before key conversion'); }
steps = [];
try { call0(); assert(false, 'absent element'); } catch (error) { assert(error instanceof TypeError, 'absent element call'); }
steps = [function () { return 'zero'; }];
assert(callK(-0) === 'zero', 'numeric -0 is index 0');
for (var key of [NaN, 0.5, -1, 4294967295, Infinity]) {
  try { callK(key); assert(false, 'absent key ' + key); } catch (error) { assert(error instanceof TypeError, 'absent key call'); }
}
true;
"#,
    );
}
