function same(a,b){if(!Object.is(a,b))throw new Error('typed number input mismatch');}
function sameList(a,b){same(a.length,b.length);for(var k=0;k<a.length;k++)same(a[k],b[k]);}
function scan(left,right,phase,start) {
  var sum=start,slot='untouched';
  try {
    for(var i=0;i<32;i++) {
      var slot=(i+phase)&3;
      if(left[slot]===right[slot])sum+=(slot&3)+1;
      else sum-=(slot&1)+1;
    }
  } catch(error){return [sum,i,slot,error];}
  return [sum,i,slot];
}
// Dynamic limit is an independent original-loop control, not a second typed path.
function original(left,right,phase,start,limit) {
  var sum=start,slot='untouched';
  for(var i=0;i<limit;i++) {
    var slot=(i+phase)&3;
    if(left[slot]===right[slot])sum+=(slot&3)+1;
    else sum-=(slot&1)+1;
  }
  return [sum,i,slot];
}
var left=[NaN,-0,Infinity,-Infinity],right=[NaN,0,Infinity,Infinity];
sameList(scan(left,right,0,0),[16,32,3]);
for(var phase of [0,1,-1,-0,2147483647,-2147483648,0.5,NaN,Infinity,-Infinity,'1']) {
  for(var start of [0,-0,9007199254740992,Infinity,-Infinity,NaN]) {
    sameList(scan(left,right,phase,start),original(left,right,phase,start,32));
  }
}
// Current values and storage are revalidated on every invocation.
right[3]=-Infinity;sameList(scan(left,right,0,0),[64,32,3]);
right[0]=0;sameList(scan(left,right,0,0),[64,32,3]);
var symbol=Symbol('same');left[0]=symbol;right[0]=symbol;
sameList(scan(left,right,0,0),[80,32,3]);
left[0]=1;right[0]=1;sameList(scan(left,right,0,0),[80,32,3]);
var object={valueOf:function(){throw new Error('strict equality never converts');}};
left[0]=object;right[0]=object;sameList(scan(left,right,0,0),[80,32,3]);
left[0]=1;right[0]=1;
// A late getter misses before any body execution, then preserves the real prefix.
var marker={},calls=0;
Object.defineProperty(right,'2',{get:function(){calls++;gc();throw marker;},configurable:true});
var result=scan(left,right,0,7);
sameList(result,[10,2,2,marker]);same(calls,1);
Object.defineProperty(right,'2',{value:Infinity,writable:true,configurable:true});
// Get left before right; a throwing left getter prevents the right read.
var log='',lhs=[1,2,3,4],rhs=[1,2,3,4];
Object.defineProperty(lhs,'0',{get:function(){log+='L';throw marker;},configurable:true});
Object.defineProperty(rhs,'0',{get:function(){log+='R';return 1;},configurable:true});
sameList(scan(lhs,rhs,0,9),[9,0,0,marker]);same(log,'L');
// The fallback uses current values after effects, including later-row mutation.
lhs=[1,2,3,4];rhs=[1,2,3,4];calls=0;
Object.defineProperty(lhs,'0',{get:function(){calls++;rhs[1]=99;return 1;},configurable:true});
sameList(scan(lhs,rhs,0,0),[48,32,3]);same(calls,8);
// A sparse own extent, an inherited getter, and Proxy receivers stay on complete Get.
lhs=[1,,3,4];rhs=[1,2,3,4];var proto=Object.create(Array.prototype);calls=0;
Object.defineProperty(proto,'1',{get:function(){same(this,lhs);calls++;return 2;}});
Object.setPrototypeOf(lhs,proto);sameList(scan(lhs,rhs,0,0),[80,32,3]);same(calls,8);
lhs=[1,2,3,4];calls=0;var proxy=new Proxy(lhs,{get:function(target,key,receiver){same(receiver,proxy);calls++;return Reflect.get(target,key,receiver);}});
sameList(scan(proxy,rhs,0,0),[80,32,3]);same(calls,32);
// Coercive phases remain source operations, repeated in source order on a miss.
log='';var coercive={valueOf:function(){log+='P';return 0;}};
sameList(scan(lhs,rhs,coercive,0),[80,32,3]);same(log.length,32);
var throwing={valueOf:function(){throw marker;}};
sameList(scan(lhs,rhs,throwing,5),[5,0,'untouched',marker]);
var short=[1];sameList(scan(short,rhs,0,0),original(short,rhs,0,0,32));
// Zero trip must not inspect any input, retain the old alias, or touch a TDZ cell.
function zero() {var slot=object,sum=-0;for(var i=0;i<0;i++){var slot=(i+phase)&3;if(a[slot]===b[slot])sum+=1;else sum-=1;}let phase=0,a=null,b=null;return [sum,i,slot];}
sameList(zero(),[-0,0,object]);
function tdz() {var sum=5;try{for(var i=0;i<32;i++){var slot=(i+phase)&3;if(a[slot]===b[slot])sum+=1;else sum-=1;}}catch(error){return [sum,i,slot,error instanceof ReferenceError];}let phase=0,a=lhs,b=rhs;}
sameList(tdz(),[5,0,undefined,true]);
// Pure strict comparison preserves all primitive and reference domains.
var coercions=0,text='text-value',copy=('prefix-'+text).slice(7);
var big=9007199254740993n,bigCopy=BigInt('9007199254740993');
var identity={},otherIdentity={},token=Symbol('same'),otherToken=Symbol('same');
Object.defineProperty(identity,Symbol.toPrimitive,{get:function(){coercions++;throw marker;}});
var fn=function(){},otherFn=function(){};
var revoked=Proxy.revocable(fn,{}),otherRevoked=Proxy.revocable(otherFn,{});
var revokedValue=revoked.proxy,otherRevokedValue=otherRevoked.proxy;
revoked.revoke();otherRevoked.revoke();
var groups=[
 [[text,big,token,identity],[copy,bigCopy,token,identity]],
 [[text,big,token,identity],[text+'x',big+1n,otherToken,otherIdentity]],
 [[fn,revokedValue,revokedValue,undefined],[fn,revokedValue,otherRevokedValue,undefined]],
 [[null,false,true,1],[undefined,false,false,'1']],
 [[NaN,-0,Infinity,-Infinity],[NaN,0,Infinity,Infinity]],
 [[new String(text),Object(big),Object(token),otherFn],[text,big,token,fn]],
 [[1,2,3,4],[1,2,3,'4']],
 [[1,2,3,'4'],[1,2,3,4]]
];
for(var group of groups) {
  for(var offset of [0,1,-1,2147483647,-2147483648]) {
    sameList(scan(group[0],group[1],offset,7),original(group[0],group[1],offset,7,32));
  }
}
sameList(scan(groups[0][0],groups[0][0],0,0),original(groups[0][0],groups[0][0],0,0,32));
same(coercions,0);gc();
// Successful projections are invocation-local; do not keep prior descriptors.
lhs=[text,big,token,identity];rhs=[copy,bigCopy,token,identity];
sameList(scan(lhs,rhs,0,0),[80,32,3]);
rhs[1]=1n;sameList(scan(lhs,rhs,0,0),[48,32,3]);
delete lhs[2];sameList(scan(lhs,rhs,0,0),original(lhs,rhs,0,0,32));
lhs[2]=token;Object.defineProperty(lhs,'1',{value:big,writable:false});
sameList(scan(lhs,rhs,0,0),original(lhs,rhs,0,0,32));
lhs.length=1;sameList(scan(lhs,rhs,0,0),original(lhs,rhs,0,0,32));
// Aliased input arrays still perform both source Gets after a descriptor miss.
lhs=[1,2,3,4];calls=0;
Object.defineProperty(lhs,'0',{get:function(){same(this,lhs);calls++;return calls;}});
sameList(scan(lhs,lhs,0,0),[64,32,3]);same(calls,16);
// The bounded counter uses one canonical local. -0 must retain the generic
// path because its first source reciprocal is observably negative infinity.
function negativeFirst(left,right,phase) {
  var sum=0,slot='before';
  for(var i=-0;i<8;i++) {var slot=(i+phase)&3;
    if(left[slot]===right[slot])sum+=1/i;else sum-=1/i;}
  return [sum,i,slot];
}
sameList(negativeFirst([1,2,3,4],[1,2,3,4],0),[-Infinity,8,3]);
function positiveFirst(left,right,phase) {
  var sum=0;
  for(var i=5;i<32;i++) {var slot=(i+phase)&3;
    if(left[slot]!==right[slot])sum+=(slot&3)+1;else sum-=(slot&1)+1;}
  return [sum,i,slot];
}
function positiveOriginal(left,right,phase,first,limit) {
  var sum=0;
  for(var i=first;i<limit;i++) {var slot=(i+phase)&3;
    if(left[slot]!==right[slot])sum+=(slot&3)+1;else sum-=(slot&1)+1;}
  return [sum,i,slot];
}
function lastBound(left,right,phase) {
  var sum=0;
  for(var i=1048570;i<1048576;i++) {var slot=(i+phase)&3;
    if(left[slot]!==right[slot])sum+=(slot&3)+1;else sum-=(slot&1)+1;}
  return [sum,i,slot];
}
for(var offset of [0,1,-1,2147483647,-2147483648]) {
  sameList(positiveFirst(groups[0][0],groups[1][1],offset),positiveOriginal(groups[0][0],groups[1][1],offset,5,32));
  sameList(lastBound(groups[0][0],groups[1][1],offset),positiveOriginal(groups[0][0],groups[1][1],offset,1048570,1048576));
}
// Callbacks/captured state and writes remain complete source iterations.
function observed(left,right,phase,observe) {
  var sum=0;function current(){return sum;}
  for(var i=0;i<8;i++){var slot=(i+phase)&3;
    if(left[slot]===right[slot])sum+=1;else sum-=1;
    observe(i,current);
  }
  return [sum,i,slot];
}
lhs=[1,2,3,4];rhs=[1,2,3,4];var prefixes=[1,0,1,2,3,2,3,4];calls=0;
sameList(observed(lhs,rhs,0,function(i,current){same(current(),prefixes[i]);calls++;if(i===0)rhs[1]=99;gc();}),[4,8,3]);same(calls,8);
function writes(left,right,phase) {
  var sum=0;
  for(var i=0;i<8;i++){var slot=(i+phase)&3;
    if(left[slot]===right[slot])sum+=1;else sum-=1;
    right[slot]=0;
  }
  return [sum,i,slot];
}
sameList(writes([1,2,3,4],[1,2,3,4],0),[0,8,3]);
function arrayTdz(right) {
  var sum=5,phase=0;
  try {for(var i=0;i<8;i++){var slot=(i+phase)&3;
    if(left[slot]===right[slot])sum+=1;else sum-=1;}}
  catch(error){return [sum,i,slot,error instanceof ReferenceError];}
  let left=[];
}
log='';rhs=[1,2,3,4];Object.defineProperty(rhs,'0',{get:function(){log+='R';return 1;}});
sameList(arrayTdz(rhs),[5,0,0,true]);same(log,'');
print('typed-number-inputs:ok');
