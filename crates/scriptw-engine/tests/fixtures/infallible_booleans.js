function check(value, message) {
  if (!value) throw new Error(message);
}
function same(actual, expected, message) {
  if (!Object.is(actual, expected)) throw new Error(message);
}
function strict(left, right) { return left === right; }
function truth(value) { return !!value; }
var object = {}, array = [], callable = function () {}, symbol = Symbol("key");
var values = [undefined, null, false, true, 0, -0, NaN, 7, "", "seven", 0n, 7n, symbol, object, array, callable];
var expectedTruth = [false, false, false, true, false, false, false, true, false, true, false, true, true, true, true, true];
for (var i = 0; i < values.length; i++) {
  same(strict(values[i], values[i]), i !== 6, "same identity/type " + i);
  same(truth(values[i]), expectedTruth[i], "truth " + i);
}
same(strict(0, -0), true, "strict zero equality");
same(strict(7, 7n), false, "Number and BigInt differ");
same(strict("seven", "sev" + "en"), true, "String content equality");
same(strict(12345678901234567890n, 12345678901234567889n + 1n), true, "BigInt content equality");
same(strict(Symbol("key"), symbol), false, "Symbol identity");
same(strict({}, object), false, "object identity");

var effects = "";
var left = { get value() { effects += "L"; return object; } };
var right = { get value() { effects += "R"; return object; } };
same(left.value === right.value, true, "ordered operands");
same(effects, "LR", "GetValue order");
var abrupts = [undefined, -0, NaN, symbol, object];
for (var j = 0; j < abrupts.length; j++) {
  var thrown = abrupts[j];
  effects = "";
  var throwingLeft = { get value() { effects += "L"; throw thrown; } };
  try {
    throwingLeft.value === right.value;
    throw new Error("missing left Throw");
  } catch (error) { same(error, thrown, "left Throw payload"); }
  same(effects, "L", "left Throw skips right");
  effects = "";
  var throwingRight = { get value() { effects += "R"; throw thrown; } };
  try {
    left.value === throwingRight.value;
    throw new Error("missing right Throw");
  } catch (error) { same(error, thrown, "right Throw payload"); }
  same(effects, "LR", "right Throw follows left");
}

var poison = { valueOf() { throw new Error("strict equality coerced"); }, toString() { throw new Error("truth coerced"); } };
same(strict(poison, poison), true, "strict equality does not coerce");
same(truth(poison), true, "ToBoolean does not coerce");
var revoked = Proxy.revocable(function () {}, {});
revoked.revoke();
same(strict(revoked.proxy, revoked.proxy), true, "revoked Proxy identity");
same(truth(revoked.proxy), true, "revoked Proxy truth");
var coerced = { valueOf() { effects += "C"; return 7; } };
effects = "";
same(coerced == 7, true, "loose fallback");
same(effects, "C", "loose fallback coerces");
try {
  poison == 0;
  throw new Error("missing loose Throw");
} catch (error) { same(error.message, "strict equality coerced", "loose Throw retained"); }

function boundary(value) {
  try { throw object; }
  catch (caught) {
    check(caught === object, "caught identity");
    return value === value;
  } finally { effects += "F"; }
}
effects = "";
same(boundary(NaN), false, "Normal comparison after catch and through finally");
same(effects, "F", "finally ran once");
function preserveThrow() {
  try { throw symbol; }
  finally { same(strict(0, -0), true, "normal finalizer comparison"); }
}
try { preserveThrow(); throw new Error("missing saved Throw"); }
catch (error) { same(error, symbol, "normal finalizer preserves saved Throw"); }
print("infallible-booleans:ok");
