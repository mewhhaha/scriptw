function same(actual, expected) {
  if (!Object.is(actual, expected)) throw new Error('projection result mismatch');
}
function rows() {
  return [{length:1,name:'ab'},{name:'ab',length:1},
    {length:1,name:'ab'},{name:'ab',length:1}];
}
function scan(array, seed, extra) {
  'use strict';
  var sum=seed;
  for(var i=0;i<8;i++){var row=array[i&3];sum+=row.length+row.name.length;}
  return [sum,i,row];
}
function split(array, seed, extra) {
  'use strict';
  var sum=seed,row='before';
  try {
    for(var i=0;i<8;i++){var row=array[i&3];sum+=row.length;sum+=row.name.length;}
  } catch(error) {return [sum,i,row,error];}
  return [sum,i,row];
}
function numbers(array, seed, extra) {
  'use strict';
  var sum=seed;
  for(var i=0;i<8;i++){var row=array[i&3];sum+=row.value;}
  return [sum,row];
}
function offset(array, seed, extra) {
  'use strict';
  var sum=seed;
  for(var i=2;i<11;i++){var row=array[i&3];sum+=row.length+row.name.length;}
  return [sum,i,row];
}
function zero(array, seed, extra) {
  'use strict';
  var sum=seed,row='before';
  for(var i=0;i<0;i++){var row=array[i&3];sum+=row.length+row.name.length;}
  return [sum,row];
}

var array=rows(), result=scan(array,5,0);
same(result[0],29);same(result[1],8);same(result[2],array[3]);
result=offset(array,5,0);same(result[0],32);same(result[1],11);same(result[2],array[2]);
array[0].name='\uD83D\uDE00';array[1].name='\uD800x';
same(scan(array,5,0)[0],29); // UTF-16 length, including an unpaired surrogate.
var negative=[{value:-0},{value:-0},{value:-0},{value:-0}];
result=numbers(negative,-0,0);same(result[0],-0);same(result[1],negative[3]);
negative[1].value=NaN;same(Number.isNaN(numbers(negative,0,0)[0]),true);
array=rows();array[2].length=Infinity;same(scan(array,0,0)[0],Infinity);

// Real source functions retain original callable identity, not projected rows.
var functions=[function aa(x){'use strict';},function bb(x){'use strict';},
  function cc(x){'use strict';},function dd(x){'use strict';}];
result=scan(functions,5,0);same(result[0],29);same(result[2],functions[3]);
same(typeof result[2],'function');
Object.defineProperty(functions[2],'length',{value:7});
same(scan(functions,5,0)[0],41);

// A late second-field getter must cause whole-loop fallback, preserving the
// two completed bodies and the first statement of the failing third body.
array=rows();var sentinel={tag:'throw'}, events=[];
Object.defineProperty(array[2],'name',{get:function(){events.push('name');gc();throw sentinel;}});
result=split(array,5,0);same(result[0],12);same(result[1],2);
same(result[2],array[2]);same(result[3],sentinel);same(events.join(','),'name');

// A wrong primitive type must retain the original left/right coercion order.
array=rows();array[2].length=1n;events=[];
Object.defineProperty(array[2],'name',{get:function(){events.push('later');return 'ab';}});
result=split(array,5,0);same(result[0],11);same(result[1],2);
same(result[2],array[2]);same(result[3] instanceof TypeError,true);same(events.length,0);
array=rows();array[2].name=null;
result=split(array,5,0);same(result[0],12);same(result[1],2);
same(result[3] instanceof TypeError,true);

// Missing own fields, row Proxies and indexed accessors retain normal Get.
array=rows();var inherited=array[2], gets=0;
delete inherited.length;
Object.setPrototypeOf(inherited,{get length(){same(this,inherited);gets++;return 7;}});
same(scan(array,5,0)[0],41);same(gets,2);
array=rows();gets=0;
array[2]=new Proxy(array[2],{get:function(target,key,receiver){gets++;return Reflect.get(target,key,receiver);}});
same(scan(array,5,0)[0],29);same(gets,4);
array=rows();var replacement=array[2];gets=0;delete array[2];
var prototype=Object.create(Array.prototype);
Object.defineProperty(prototype,'2',{get:function(){same(this,array);gets++;return replacement;}});
Object.setPrototypeOf(array,prototype);
same(scan(array,5,0)[0],29);same(gets,2);
result=zero(new Proxy([], {get:function(){throw sentinel;}}),-0,0);
same(result[0],-0);same(result[1],'before');

// A callback invalidates the complete proof, so subsequent rows see writes.
function callback(array, seed, observe) {
  'use strict';var sum=seed;
  for(var i=0;i<8;i++){var row=array[i&3];sum+=row.length+row.name.length;observe(i);}
  return sum;
}
array=rows();var calls=0;
same(callback(array,5,function(i){calls++;if(i===0)array[1].length=7;}),41);same(calls,8);
// Current storage, not a first-row slot plan, owns every projected field.
// Three-property births use array compact backing; definitions promote rows.
array=[{length:1,name:'ab',extra:0},{length:1,name:'ab',extra:1},
  {length:1,name:'ab',extra:2},{length:1,name:'ab',extra:3}];
same(scan(array,5,0)[0],29);
Object.defineProperty(array[1],'extra',{get:function(){throw sentinel;}});
same(scan(array,5,0)[0],29); // An unrelated accessor is never observed.
array[2]=Array;array[3]=function dd(x){'use strict';};
result=scan(array,5,0);same(result[0],35);same(result[2],array[3]);

// Intrinsic constructors begin with recipe-backed canonical storage.
array=[Array,Map,Set,Date];
result=scan(array,5,0);same(result[0],51);same(result[2],Date);
// A nonnumeric DATA recipe is resolved internally, then the String type gate
// misses. The original source reads the real function's numeric length.
function methods(array,seed,extra) {
  'use strict';var sum=seed;
  for(var i=0;i<8;i++){var row=array[i&3];sum+=row.isArray.length;}
  return [sum,row];
}
result=methods([Array,Array,Array,Array],5,0);same(result[0],13);same(result[1],Array);
var method=Array.isArray;gc();same(method,Array.isArray);
array=[Array,Array,{isArray:'abcd'},Array];
same(methods(array,5,0)[0],19);
var seen=0;
Object.defineProperty(array[2],'isArray',{get:function(){seen++;gc();return method;}});
same(methods(array,5,0)[0],13);same(seen,2);
print('scalar-row-projection:ok');
