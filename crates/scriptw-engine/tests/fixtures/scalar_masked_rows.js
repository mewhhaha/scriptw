function check(value, label) { if (!value) throw label; }
function record(weight, text) { return {weight:weight, text:text}; }
var rows;
function scan(seed) {
  var sum=seed;
  for(var i=0;i<8;i++) {
    var row=rows[i&3];
    sum+=row.weight+row.text.length;
  }
  return [sum,i,row];
}
function localScan(array, seed, unused) {
  var sum=seed;
  for(var i=1;i<8;i++) {
    var row=array[i&3];
    sum+=row.weight+row.text.length;
  }
  return [sum,i,row];
}
function caughtScan(seed) {
  var sum=seed, row='before';
  try {
    for(var i=0;i<8;i++) {
      var row=rows[i&3];
      sum+=row.weight+row.text.length;
    }
  } catch(error) { return [sum,i,row,error]; }
  throw 'expected an abrupt completion';
}
function caughtTwoStatements(seed) {
  var sum=seed, row='before';
  try {
    for(var i=0;i<8;i++) {
      var row=rows[i&3];
      sum+=row.weight;
      sum+=row.text.length;
    }
  } catch(error) { return [sum,i,row,error]; }
  throw 'expected a second-field abrupt completion';
}
rows=[record(1,''),record(2,'a'),record(3,'\ud83d\ude00'),record(4,'\ud800')];
var result=scan(5), local=localScan(rows,5,0);
check(result[0]===33 && result[1]===8 && result[2]===rows[3], 'full trip and final private alias');
check(local[0]===32 && local[1]===8 && local[2]===rows[3], 'positive literal offset');
rows[0].weight=9;rows[2].text='';gc();
check(scan(5)[0]===45, 'guards read live values on each invocation');
rows=[record(-0,''),record(-0,''),record(-0,''),record(-0,'')];
check(Object.is(scan(-0)[0],0), 'source addition preserves signed zero arithmetic');
rows=[record(-1e16,'a'),record(1e16,''),record(-1e16,''),record(1e16,'')];
check(scan(1e16)[0]===1e16, 'ordered binary64 additions, no algebraic folding');
rows[2].weight=NaN;check(Number.isNaN(scan(1)[0]), 'Number NaN');

// The first row whose fields fail a guard must execute in the original loop,
// with completed counter/sum/row state published, without replaying its prefix.
var trace='', replacement=[record(10,'z'),record(10,'z'),record(10,'z'),record(10,'z')];
var late={get weight(){check(this===late,'getter receiver');trace+='w';rows=replacement;gc();return 3;},text:'ccc'};
rows=[record(1,'a'),record(2,'bb'),late,record(4,'dddd')];
result=scan(0);
check(result[0]===67 && result[1]===8 && result[2]===replacement[3] && trace==='w', 'late accessor and live captured array replacement');
var token={}, abrupt={get weight(){trace+='t';throw token;},text:'x'};
rows=[record(1,'a'),record(2,'bb'),abrupt,record(4,'dddd')];
result=caughtScan(0);
check(result[0]===6 && result[1]===2 && result[2]===abrupt && result[3]===token && trace==='wt', 'late throw whole value and exact prefix publication');
rows=[abrupt,record(2,'b'),record(3,'c'),record(4,'d')];
result=caughtScan(7);
check(result[0]===7 && result[1]===0 && result[2]===abrupt && result[3]===token, 'first-row miss does not publish a speculative alias');

// A late second-field miss resumes the whole source iteration. Its first
// statement must commit before the second throws, with no replayed prefix.
trace='';var secondToken={};
var secondAbrupt={weight:5,get text(){trace+='s';gc();throw secondToken;}};
rows=[record(1,'a'),record(2,'bb'),secondAbrupt,record(4,'dddd')];
result=caughtTwoStatements(7);
check(result[0]===18 && result[1]===2 && result[2]===secondAbrupt && result[3]===secondToken && trace==='s', 'late second-field throw keeps partial generic iteration');

// Coercion, boxed String and dictionary/accessor transitions retain Get and
// ToPrimitive. The direct String leaf counts UTF-16 units, not code points.
trace='';var number={valueOf(){trace+='n';gc();return 3;}};
rows=[record(1,''),record(2,''),record(number,'x'),record(4,'')];
check(scan(0)[0]===22 && trace==='nn', 'object Number coercion stays generic');
rows=[record(1,''),record(2,''),record(3,new String('abc')),record(4,'')];
check(scan(0)[0]===26, 'boxed String fallback');
rows=[record(1,'a'),record(2,'bb'),record(3,'ccc'),record(4,'dddd')];
rows[2].extra=true;Object.defineProperty(rows[2],'text',{value:'q',writable:false});
check(scan(0)[0]===36, 'promoted storage and live attributes');
var inherited=rows[1];delete inherited.text;
Object.setPrototypeOf(inherited,{get text(){check(this===inherited,'inherited receiver');return 'z';}});
check(scan(0)[0]===34, 'missing own field uses prototype descriptor');

// Dense length and actual storage guards must reject holes and short arrays,
// even where an inherited index happens to produce an otherwise valid row.
var backing=[record(1,'a')],holeGets=0;
Object.setPrototypeOf(backing,{
  get 1(){holeGets++;return record(2,'b');},
  get 2(){holeGets++;return record(3,'c');},
  get 3(){holeGets++;return record(4,'d');}
});
rows=backing;check(scan(0)[0]===28 && holeGets===6, 'actual length is not the source trip limit');
var hole=Array(4);hole[0]=record(1,'a');hole[2]=record(3,'c');hole[3]=record(4,'d');
Object.setPrototypeOf(hole,{get 1(){holeGets++;return record(2,'b');}});
rows=hole;check(scan(0)[0]===28 && holeGets===8, 'packed count excludes holes');
trace='';var target=[record(1,''),record(2,''),record(3,''),record(4,'')];
rows=new Proxy(target,{get:function(array,key,receiver){trace+=key;check(receiver===rows,'array proxy receiver');return Reflect.get(array,key,receiver);}});
check(scan(0)[0]===20 && trace==='01230123', 'array proxy traps every source index once');
trace='';var proxyRow=new Proxy(record(3,'x'),{get:function(object,key,receiver){trace+=key+',';return Reflect.get(object,key,receiver);}});
rows=[record(1,''),record(2,''),proxyRow,record(4,'')];
check(scan(0)[0]===22 && trace==='weight,text,weight,text,', 'row proxy retains complete Gets');

// Neither a false source test nor a guard may turn an uninitialized canonical
// cell into an eager throw. A true test still throws in the original body.
function zeroTrip() {
  var total=7,row='unchanged';
  for(var i=8;i<8;i++){var row=later[i&3];total+=row.weight+row.text.length;}
  return [total,i,row];
}
function tdzTrip() {
  var total=7;
  for(var i=0;i<8;i++){var row=later[i&3];total+=row.weight+row.text.length;}
  return total;
}
result=zeroTrip();check(result[0]===7 && result[1]===8 && result[2]==='unchanged', 'zero-trip TDZ and previous alias');
var tdz=false;try{tdzTrip();}catch(error){tdz=error instanceof ReferenceError;}
check(tdz,'positive-trip TDZ');
let later=[];
print('scalar-masked-rows:ok');
