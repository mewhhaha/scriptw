function same(a,b){if(!Object.is(a,b))throw new Error('cyclic key mismatch');}
function scan(rows,keys,seed) {
  var sum=seed;
  try { for(var i=0;i<16;i++)sum+=rows[i&3][keys[i&1]]; }
  catch(error){return [sum,i,error];}
  return [sum,i];
}
function largerKeys(rows,keys,seed) {
  var sum=seed;for(var i=0;i<16;i++){var row=rows[i&1];sum+=row[keys[i&3]];}return [sum,row];
}
function aliasKey(rows,seed) {
  var row=['x','y'],sum=seed;for(var i=0;i<16;i++){var row=rows[i&3];sum+=row[row[i&1]];}return sum;
}
function shifted(rows,keys,seed) {
  var sum=seed;for(var i=3;i<16;i++)sum+=rows[i&3][keys[i&1]];return sum;
}
function zero(rows,keys,seed) {
  var sum=seed;for(var i=0;i<0;i++)sum+=rows[i&3][keys[i&1]];return sum;
}
var rows=[{x:1,y:10},{x:2,y:20},{x:3,y:30},{x:4,y:40}];
var keys=['prefix-x'.slice(7),String.fromCharCode(121)];
same(scan(rows,keys,0)[0],256);same(shifted(rows,keys,0),232);
var pair=largerKeys(rows,['x','y','y','x'],0);same(pair[0],132);same(pair[1],rows[1]);
same(zero(null,null,-0),-0);
var aliasRows=[];for(var a=0;a<4;a++){var entry=['y','x'];entry.x=a+1;entry.y=(a+1)*10;aliasRows.push(entry);}
same(aliasKey(aliasRows,0),184);
// Mutated data dictionaries and equal-content, non-interned keys stay live.
for(var j=0;j<rows.length;j++){
  rows[j].extra=j;delete rows[j].extra;
  Object.defineProperty(rows[j],'x',{get:function(){return 99;},configurable:true});
  delete rows[j].x;Object.defineProperty(rows[j],'x',{value:j+1,writable:true,configurable:true});
}
same(scan(rows,keys,0)[0],256);rows[2].x=7;same(scan(rows,keys,0)[0],272);rows[2].x=3;
// A late own getter is never invoked speculatively; original completed prefix survives.
var marker={},calls=0;
Object.defineProperty(rows[2],'x',{get:function(){calls++;gc();throw marker;},configurable:true});
var result=scan(rows,keys,5);same(result[0],26);same(result[1],2);same(result[2],marker);same(calls,1);
Object.defineProperty(rows[2],'x',{value:3,writable:true,configurable:true});
// Key-array getters, inherited holes, proxies and non-Strings use original Get/ToPropertyKey.
var keyCalls=0,accessorKeys=['x','y'];
Object.defineProperty(accessorKeys,'1',{get:function(){keyCalls++;return 'y';},configurable:true});
same(scan(rows,accessorKeys,0)[0],256);same(keyCalls,8);
var holeKeys=['x',,'unused'],proto=Object.create(Array.prototype);
Object.defineProperty(proto,'1',{get:function(){keyCalls++;return 'y';},configurable:true});
Object.setPrototypeOf(holeKeys,proto);same(scan(rows,holeKeys,0)[0],256);same(keyCalls,16);
var conversions=0,objectKey={[Symbol.toPrimitive]:function(){conversions++;gc();return 'x';}};
same(scan(rows,[objectKey,'y'],0)[0],256);same(conversions,8);
var proxyCalls=0,proxyKeys=new Proxy(keys,{get:function(target,key,receiver){proxyCalls++;return Reflect.get(target,key,receiver);}});
same(scan(rows,proxyKeys,0)[0],256);same(proxyCalls,16);
var rowCalls=0,proxyRows=rows.slice();proxyRows[2]=new Proxy(rows[2],{get:function(target,key,receiver){rowCalls++;return Reflect.get(target,key,receiver);}});
same(scan(proxyRows,keys,0)[0],256);same(rowCalls,4);
var symbol=Symbol('field');for(var r of rows)r[symbol]=2;
same(scan(rows,[symbol,symbol],0)[0],32);
// Ordinary key conversion can mutate a later row: cold fallback cannot use a stale projection.
var mutateKey={toString:function(){rows[2].x=7;return 'x';}};
same(scan(rows,[mutateKey,'y'],0)[0],272);rows[2].x=3;
var badKeys=['x', {toString:function(){throw marker;}}];
result=scan(rows,badKeys,0);same(result[0],1);same(result[1],1);same(result[2],marker);
// Missing data/prototypes and non-Number values decline without early source effects.
var inherited=rows.slice(),getterCalls=0;
inherited[2]=Object.create({get x(){getterCalls++;return 3;}});
same(scan(inherited,keys,0)[0],256);same(getterCalls,4);
var wrong=rows.slice();wrong[2]={x:{valueOf:function(){throw marker;}},y:30};
result=scan(wrong,keys,0);same(result[0],21);same(result[1],2);same(result[2],marker);
var zeros=[{x:-0,y:-0},{x:-0,y:-0},{x:-0,y:-0},{x:-0,y:-0}];
same(scan(zeros,keys,-0)[0],-0);zeros[2].x=NaN;same(scan(zeros,keys,0)[0],NaN);
zeros[2].x=Infinity;zeros[3].y=-Infinity;same(scan(zeros,keys,0)[0],NaN);
print('computed-key-projection:ok');
