function same(actual,expected) {
  if (!Object.is(actual,expected)) throw new Error('projection Realm mismatch');
}
function metadata(array,seed,extra) {
  'use strict';var sum=seed;
  for(var i=0;i<8;i++){var row=array[i&3];sum+=row.length+row.name.length;}
  return [sum,row];
}
function methodNames(array,seed,extra) {
  'use strict';var sum=seed;
  for(var i=0;i<8;i++){var row=array[i&3];sum+=row.isArray.length;}
  return [sum,row];
}
var realm=__scriptwCreateRealm(), foreign=realm.global, A=foreign.Array;
var rows=[A,A,A,A], result=metadata(rows,5,0);
same(result[0],53);same(result[1],A);
// The recipe is unobserved until the cold projection probe. Its wrong String
// type falls back, but normalization must keep the defining Realm and identity.
result=methodNames(rows,5,0);same(result[0],13);same(result[1],A);
var method=A.isArray;gc();same(A.isArray,method);
same(Object.getPrototypeOf(method),foreign.Function.prototype);
same(method([]),true);
// A fresh foreign constructor is promoted before its method recipe is read.
var other=__scriptwCreateRealm().global, B=other.Array;
Object.defineProperty(B,'extra',{value:1});
same(methodNames([B,B,B,B],5,0)[0],13);
same(Object.getPrototypeOf(B.isArray),other.Function.prototype);
// The first intrinsic row must not call a later accessor during preflight.
var getter=realm.evalScript('(function(){throw new TypeError("foreign row");})');
var late={length:1};Object.defineProperty(late,'name',{get:getter});
try {metadata([A,A,late,A],5,0);throw new Error('missing getter Throw');}
catch(error) {same(error instanceof foreign.TypeError,true);same(error instanceof TypeError,false);}
var revoked=Proxy.revocable(A,{});revoked.revoke();
try {metadata([A,A,revoked.proxy,A],5,0);throw new Error('missing proxy Throw');}
catch(error) {same(error instanceof TypeError,true);same(error instanceof foreign.TypeError,false);}
print('scalar-row-projection-realms:ok');
