function same(actual, expected, label) {
  if (!Object.is(actual,expected)) throw new Error(label + ': ' + String(actual) + ' != ' + String(expected));
}
function make(first) {
  var tick=first;
  function step(value,index) {
    'use strict';
    var original=value;
    value=index+tick;
    tick=(tick+1)&255;
    var record=original;
    return ((record.x^value)+record.y+2+tick)&1023;
  }
  return {step:step, tick:function(){return tick;}};
}
function reference(first) {
  var tick=first;
  function step(value,index,unobserved) {
    'use strict';
    var original=value;
    value=index+tick;
    tick=(tick+1)&255;
    var record=original;
    return ((record.x^value)+record.y+2+tick)&1023;
  }
  return {step:step, tick:function(){return tick;}};
}
var numbers=[0,-0,NaN,Infinity,-Infinity,1.5,-3.75,2147483648,4294967297,1e80];
for(var n=0;n<numbers.length;n++) {
  var a=make(numbers[n]), b=reference(numbers[n]);
  for(var j=0;j<numbers.length;j++) {
    var row={x:numbers[j],y:numbers[(j+3)%numbers.length]};
    same(a.step(row,j,99),b.step(row,j,99),'Number result');
    same(a.tick(),b.tick(),'Number write');
  }
}
for (var start of ['5',undefined,null,true]) {
  var generic=make(start), expected=reference(start);
  same(generic.step({x:4,y:8},2),expected.step({x:4,y:8},2),'capture type fallback');
  same(generic.tick(),expected.tick(),'capture type write');
}
for (var unusual of [1n,Symbol('x')]) {
  var throwing=make(0), threw=false;
  try { throwing.step({x:unusual,y:3},1); } catch(error) { threw=error instanceof TypeError; }
  same(threw,true,'non-Number throws');same(throwing.tick(),1,'non-Number prefix');
}
var first=make(3), second=make(100), row={x:4,y:8};
same(first.step(row,2),reference(3).step(row,2),'first identity');
same(second.step(row,2),reference(100).step(row,2),'second identity');
same(first.tick(),4,'separate captured cell');
var order='', state=make(7), marker={};
var accessor={get x(){order+='x';same(state.tick(),8,'write before first getter');gc();return 3;},get y(){order+='y';throw marker;}};
try { state.step(accessor,2); throw new Error('missing getter Throw'); } catch(error) { same(error,marker,'whole Throw'); }
same(order,'xy','getter order');same(state.tick(),8,'partial prefix');
var reentrant=make(2), nested={x:1,y:2};
var changing={get x(){same(reentrant.tick(),3,'reentry starts after write');reentrant.step(nested,0);return 4;},get y(){return reentrant.tick();}};
same(reentrant.step(changing,1),((4^3)+4+2+4)&1023,'reentry reload');same(reentrant.tick(),4,'nested write');
var coercions=make(4), numeric={valueOf:function(){same(coercions.tick(),5,'coercion after write');return 8;}};
same(coercions.step({x:numeric,y:2},1),((8^5)+2+2+5)&1023,'coercion fallback');
var traps='', proxied=new Proxy({x:5,y:6},{get:function(target,key,receiver){traps+=key;return Reflect.get(target,key,receiver);}});
var proxyState=make(0);same(proxyState.step(proxied,0),14,'proxy result');same(traps,'xy','proxy effects once');
var missing=make(0), proto={get x(){same(missing.tick(),1,'prototype getter after write');return 3;}}, inherited=Object.create(proto);inherited.y=5;
same(missing.step(inherited,2),9,'prototype fallback');
var absent=make(8), caught=false;
try { absent.step(); } catch(error) { caught=error instanceof TypeError; }
same(caught,true,'absent arguments Throw');same(absent.tick(),9,'absent prefix');
var dictionary={x:2,y:3,temporary:1};delete dictionary.temporary;
var dictionaryState=make(0);same(dictionaryState.step(dictionary,1),9,'dictionary');
var effects=0, held=make(1);function arg(){effects++;return {x:2,y:3};}
held.step(arg(),(effects++,4),(effects++,99));same(effects,3,'all operands once');
var constructed=make(0), instance=new constructed.step({x:1,y:2},3);
same(Object.getPrototypeOf(instance),constructed.step.prototype,'constructor entry');same(constructed.tick(),1,'constructor write');
print('numeric body ok');
