function check(value, message) { if (!value) throw message; }
function verify(value, isDDA) {
  check(value !== null && null !== value, 'HTMLDDA is never strictly null');
  check(value !== void 0 && void 0 !== value, 'HTMLDDA is never strictly undefined');
  check((value == null) === isDDA && (null == value) === isDDA, 'loose null remains special');
  check((value == void 0) === isDDA && (void 0 == value) === isDDA, 'loose undefined remains special');
}
var dda = __scriptwCreateHTMLDDA();
verify(dda, true);
verify(Object.create(dda), false);
verify(Function.prototype.bind.call(dda, null), false);
verify(new Proxy(dda, {}), false);
var revoked = Proxy.revocable(dda, {});
revoked.revoke();
verify(revoked.proxy, false);
var effects = 0;
function effect() { effects++; gc(); return dda; }
check(dda !== void effect() && effects === 1, 'strict comparison still runs void');
check(dda == void effect() && effects === 2, 'loose comparison remains unchanged');
print('ok');
