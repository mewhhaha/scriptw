function check(value, message) { if (!value) throw new Error(message); }
function fn(length, name) {
  var value = function sourceRow() {};
  Object.defineProperty(value, 'length', {value:length, configurable:true});
  Object.defineProperty(value, 'name', {value:name, configurable:true});
  return value;
}
function fresh() { return [fn(1,'A'),fn(2,'BB'),fn(3,'CCC'),fn(4,'DDDD')]; }
function scan(array, seed, unused) {
  var sum=seed;
  for(var i=0;i<8;i++) { var row=array[i&3]; sum+=row.length+row.name.length; }
  return [sum,i,row];
}
function caught(array, seed, unused) {
  var sum=seed,row='before';
  try { for(var i=0;i<8;i++) { var row=array[i&3]; sum+=row.length+row.name.length; } }
  catch(error) { return [sum,i,row,error]; }
  throw new Error('expected row failure');
}
function split(array, seed, unused) {
  var sum=seed,row='before';
  try { for(var i=0;i<8;i++) { var row=array[i&3]; sum+=row.length; sum+=row.name.length; } }
  catch(error) { return [sum,i,row,error]; }
  throw new Error('expected second-field failure');
}
var rows=fresh(),result=scan(rows,5,0);
check(result[0]===45 && result[1]===8 && result[2]===rows[3] && typeof result[2]==='function', 'source Function rows retain original alias and tag');
Object.defineProperty(rows[0],'length',{value:9});
Object.defineProperty(rows[2],'name',{value:'x'});gc();
check(scan(rows,0,0)[0]===52, 'next invocation reads current descriptors and values');
rows=fresh();rows[2]={length:3,name:'CCC'};
check(scan(rows,0,0)[0]===40, 'later dictionary-to-compact storage mode miss');
rows=fresh();rows[0]={length:1,name:'A'};
check(scan(rows,0,0)[0]===40, 'later compact-to-dictionary storage mode miss');

var trace='', token={};
rows=fresh();var late=rows[2];
Object.defineProperty(late,'name',{get:function(){check(this===late,'name receiver');trace+='n';gc();throw token;}});
result=split(rows,7,0);
check(result[0]===16 && result[1]===2 && result[2]===late && result[3]===token && trace==='n', 'completed prefix and first generic statement before second-field Throw');
result=caught(rows,7,0);
check(result[0]===13 && result[1]===2 && result[2]===late && result[3]===token && trace==='nn', 'single RHS does not commit before its late Throw');
rows=[late,fn(2,'BB'),fn(3,'CCC'),fn(4,'DDDD')];
result=caught(rows,7,0);
check(result[0]===7 && result[1]===0 && result[2]===late && result[3]===token, 'first-row miss publishes no speculative state');

rows=fresh();late=rows[2];trace='';
Object.defineProperty(late,'length',{value:{valueOf:function(){trace+='c';gc();throw token;}}});
Object.defineProperty(late,'name',{get:function(){trace+='g';return 'CCC';}});
result=caught(rows,0,0);
check(result[0]===6 && result[1]===2 && result[2]===late && result[3]===token && trace==='gc', 'type miss preserves Get-before-coercion ordering');
rows=fresh();late=rows[2];trace='';
delete late.name;
Object.setPrototypeOf(late,{get name(){check(this===late,'inherited receiver');trace+='p';return '?';}});
check(scan(rows,0,0)[0]===36 && trace==='pp', 'deleted entry retains inherited getter');

// Hints are insertion positions, not a shared dictionary shape. Tombstones,
// reordered keys and replacement tables must never authorize an old entry.
rows=fresh();late=rows[2];
delete late.length;delete late.name;
Object.defineProperty(late,'name',{value:'CCC',configurable:true});
Object.defineProperty(late,'length',{value:3,configurable:true});
for(var n=0;n<40;n++) late['spare'+n]=n;
for(var n=0;n<40;n++) delete late['spare'+n];
for(var n=0;n<40;n++) late['fresh'+n]=n;
check(scan(rows,0,0)[0]===40, 'reordered and compacted dictionary slots');
var first={padding:0,padding2:0,length:1,name:'A'};
Object.defineProperty(first,'extra',{value:0});
var short={};Object.defineProperty(short,'name',{value:'CCC',configurable:true});
trace='';Object.setPrototypeOf(short,{get length(){trace+='s';return 3;}});
rows=[first,first,short,fn(4,'DDDD')];
check(scan(rows,0,0)[0]===36 && trace==='ss', 'logical entry extent and absent own data');

rows=fresh();late=rows[2];trace='';var replacement=fn(10,'Z');
Object.defineProperty(late,'length',{get:function(){trace+='l';rows[0]=replacement;rows[3]=replacement;gc();return 3;}});
result=scan(rows,0,0);
check(result[0]===55 && trace==='ll' && result[2]===replacement, 'callback after decline sees live array replacements');
rows=fresh();var original=rows[2];trace='';
rows[2]=new Proxy(original,{get:function(target,key,receiver){trace+=key+',';return Reflect.get(target,key,receiver);}});
check(scan(rows,0,0)[0]===40 && trace==='length,name,length,name,', 'Proxy rows are excluded from direct storage');
rows=fresh();Object.defineProperty(rows[2],'name',{value:new String('CCC')});
check(scan(rows,0,0)[0]===40, 'boxed String leaf stays generic');
rows=fresh();rows[2]=(function three(a,b,c){}).bind(null);
check(scan(rows,0,0)[0]===56, 'bound callable stays generic');
rows=[Array.prototype.push,Array.prototype.pop,Array.prototype.shift,Array.prototype.unshift];
result=scan(rows,0,0);
check(result[0]===42 && result[2]===Array.prototype.unshift, 'concrete compact native callable metadata is also data');
rows=[Array,Map,Set,Number];
check(scan(rows,0,0)[0]===38, 'intrinsic recipe storage remains generic');

function capturedAlias(array, seed, unused) {
  var sum=seed;function observe(){return row;}
  for(var i=0;i<8;i++){var row=array[i&3];sum+=row.length+row.name.length;}
  return [sum,observe];
}
rows=fresh();result=capturedAlias(rows,0,0);
check(result[0]===40 && result[1]()===rows[3], 'captured alias stays canonical');
print('scalar-dictionary-rows:ok');
