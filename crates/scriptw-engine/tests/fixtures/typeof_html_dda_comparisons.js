function check(value, message) { if (!value) throw message; }
var dda = __scriptwCreateHTMLDDA();
function verify(value, undefinedKind, functionKind, objectKind) {
  check((typeof value === 'undefined') === undefinedKind, 'HTMLDDA undefined');
  check(('function' === typeof value) === functionKind, 'HTMLDDA function');
  check((typeof value == 'object') === objectKind, 'HTMLDDA object');
  check((typeof value !== 'undefined') === !undefinedKind, 'HTMLDDA inverse');
  check(typeof value !== 'number' && typeof value !== 'string', 'HTMLDDA primitives');
}
verify(dda, true, false, false);
verify(Object.create(dda), false, false, true);
verify(Function.prototype.bind.call(dda, null), false, true, false);
verify(new Proxy(dda, {}), false, true, false);
var revoked = Proxy.revocable(dda, {});
revoked.revoke();
verify(revoked.proxy, false, true, false);
gc();
verify(dda, true, false, false);
print('ok');
