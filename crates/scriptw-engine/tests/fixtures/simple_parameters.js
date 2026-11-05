function same(actual, expected) {
    if (!Object.is(actual, expected)) throw new Error("parameter value mismatch");
}
function throws(action, expected) {
    var caught = false;
    try { action(); } catch (error) { caught = true; same(error, expected); }
    if (!caught) throw new Error("missing parameter failure");
}
function errorType(action, Type) {
    var caught = false;
    try { action(); } catch (error) { caught = true; same(error instanceof Type, true); }
    if (!caught) throw new Error("missing parameter error");
}
function collect(a,b,c,d,e) { "use strict"; return [a,b,c,d,e]; }
var marker = {}, symbol = Symbol("parameter"), nan = NaN;
var values = collect(marker, -0, nan, symbol, 12345678901234567890n, "extra");
same(values[0], marker); same(values[1], -0); same(values[2], nan);
same(values[3], symbol); same(values[4], 12345678901234567890n);
values = collect(marker); same(values[0], marker);
for (var i=1; i<5; ++i) same(values[i], undefined);

function mutate(a,b,c,d,e) {
    "use strict";
    var a; b=a; a=e;
    { let a=c; same(a,c); }
    return [a,b,c,d,e];
}
values = mutate(marker, 2, 3, 4, symbol);
same(values[0], symbol); same(values[1], marker); same(values[2], 3);
function observed(a,b,c,d,e) {
    "use strict";
    a=e; arguments[1]=c;
    return [a,b,arguments[0],arguments[1],arguments.length];
}
values = observed(marker, symbol, 3, 4, 5, 6);
same(values[0],5); same(values[1],symbol); same(values[2],marker);
same(values[3],3); same(values[4],6);

function keep(a, callback, expected, finalizer, e) {
    "use strict";
    try { callback(); } catch (error) { same(error,expected); }
    finally { finalizer(); }
    return [a,e];
}
var finalizers=0;
for (var thrown of [undefined, -0, NaN, 7n, symbol, marker]) {
    var pending=thrown;
    values=keep(marker, function () { collect(1,2,3,4,5); gc(); throw pending; },
        pending, function () { ++finalizers; gc(); }, symbol);
    same(values[0],marker); same(values[1],symbol);
}
same(finalizers,6);
function captured(a,b,c,d,e) {
    "use strict";
    return [function () { return a; }, function (next) { a=next; }, b,e];
}
var pair=captured(marker,2,3,4,symbol);
pair[1](symbol); gc(); same(pair[0](),symbol); same(pair[2],2); same(pair[3],symbol);
function Construct(a,b,c,d,e) { "use strict"; this.first=a; this.last=e; }
var made=new Construct(marker,2,3,4,symbol);
same(made.first,marker); same(made.last,symbol);
function redeclare(a,b,c) { "use strict"; function a() { return 17; } return [a(),b,c]; }
values=redeclare(0,marker,symbol); same(values[0],17); same(values[1],marker); same(values[2],symbol);
var named=function self(self,b,c) { "use strict"; return [self,b,c]; };
values=named(marker,symbol,3); same(values[0],marker); same(values[1],symbol);

// Non-simple FDI still has real TDZ and ordered abrupt pattern operations.
function later(a=b,b=1,c=2) { return a; }
function self(a=a,b=1,c=2) { return a; }
function lexical(a,b,c) { "use strict"; return hidden; let hidden=a; }
errorType(function () { later(); },ReferenceError);
errorType(function () { self(); },ReferenceError);
errorType(function () { lexical(1,2,3); },ReferenceError);
var trace=[];
function defaultOrder(a=(trace.push("a"),1),b=(trace.push("b"),a),c=3) { return b; }
same(defaultOrder(),1); same(trace.join(","),"a,b");
function pattern({value},b,c) { return value; }
throws(function () { pattern({get value(){throw marker;}},2,3); },marker);
function iterator([value],b,c) { return value; }
throws(function () { iterator({[Symbol.iterator](){return {next(){throw symbol;}};}},2,3); },symbol);
function rest(a,...tail) { return [a,tail.length,tail[0]]; }
values=rest(marker,symbol,3); same(values[0],marker); same(values[1],2); same(values[2],symbol);
function* resumed(a,b,c) { "use strict"; yield a; a=b; return a; }
var iteratorValue=resumed(marker,symbol,3);
same(iteratorValue.next().value,marker); gc(); same(iteratorValue.next().value,symbol);
class Base {}
class Derived extends Base { constructor(a,b,c) { this.before=a; super(); } }
errorType(function () { new Derived(1,2,3); },ReferenceError);
print("simple-parameters:ok");
