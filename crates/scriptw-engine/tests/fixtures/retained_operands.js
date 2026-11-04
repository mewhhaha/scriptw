function same(actual,expected) {
  if (!Object.is(actual,expected)) throw new Error('retained operand mismatch');
}
var events=[], first={}, second={}, marker={};
function invoke(base,argument) {'use strict';return base().method(argument());}
second.method=function(value){same(this,second);events.push('inner');return value;};
Object.defineProperty(first,'method',{configurable:true,get:function(){
  events.push('get');
  return function(value){same(this,first);events.push('call');return value;};
}});
function base(){events.push('base');return first;}
function argument(){
  events.push('argument');gc();
  same(invoke(function(){return second;},function(){return 2;}),2);
  Object.defineProperty(first,'method',{configurable:true,value:function(){throw marker;}});
  return 7;
}
same(invoke(base,argument),7);
same(events.join(','),'base,get,argument,inner,call');
// A new invocation owns fresh temporary roots even after a nested Throw.
var argumentsRun=0;
Object.defineProperty(first,'method',{configurable:true,get:function(){gc();throw marker;}});
try {invoke(base,function(){argumentsRun++;});throw new Error('missing get Throw');}
catch(error){same(error,marker);}same(argumentsRun,0);
first=second;
try {invoke(base,function(){gc();throw marker;});throw new Error('missing argument Throw');}
catch(error){same(error,marker);}
same(invoke(base,function(){gc();return 9;}),9);

// The old property value is a complete snapshot before RHS reentry/coercion.
events=[];
var old={valueOf:function(){events.push('coerce');gc();return 4;}}, object={x:old};
object.x += (events.push('rhs'),object.x=100,gc(),3);
same(object.x,7);same(events.join(','),'rhs,coerce');
var scope={x:old,method:function(value){same(this,scope);return value;}};
with(scope) {
  x += (scope.x=100,gc(),3);
  same(method((scope.method=function(){throw marker;},gc(),8)),8);
}
same(scope.x,7);

// Super/private/optional consumers use the same private operand lifetime.
var setterReceiver, stored;
class Parent {
  get x(){return old;}
  set x(value){setterReceiver=this;stored=value;}
}
class Child extends Parent {
  #number=12;
  read(value){return value?.#number;}
  update(){return super.x += (gc(),3);}
}
var child=new Child();same(child.read(child),12);same(child.read(null),undefined);
same(child.update(),7);same(stored,7);same(setterReceiver,child);
var original={x:1}, selected=original;
same(delete selected?.[({toString:function(){selected={x:2};gc();return 'x';}})],true);
same(Object.hasOwn(original,'x'),false);same(selected.x,2);

// Yield owners keep their separate activation-backed callee/receiver state.
var suspended={method:function(value){same(this,suspended);return value;}};
function* pause(){return suspended.method(yield 1);}
var iterator=pause();same(iterator.next().value,1);
suspended.method=function(){throw marker;};gc();
var completed=iterator.next(23);same(completed.done,true);same(completed.value,23);
// Source lexical TDZ remains observable.
var caught=false;
try { { read(); let lexical=1; function read(){return lexical;} } }
catch(error){caught=error instanceof ReferenceError;}
same(caught,true);
true;
