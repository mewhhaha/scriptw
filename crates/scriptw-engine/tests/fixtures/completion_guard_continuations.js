function check(value, message) {
  if (!value) throw message;
}
var marker = {name: 'original'}, other = {name: 'replacement'};
var reasons = [undefined, null, -0, NaN, 3n, Symbol('reason'), marker];
var calls = 0, normal = {value: 7};
function fail(reason) {
  calls++;
  gc();
  throw reason;
}
for (var index = 0; index < reasons.length; index++) {
  var reason = reasons[index], reached = false, saved = 0;
  try {
    saved = normal.value;
    fail(reason);
    reached = true;
  } catch (error) {
    check(Object.is(error, reason), 'whole call throw');
  }
  check(saved === 7 && !reached, 'call stops publication');
}
check(calls === reasons.length, 'calls execute once');

var reads = 0, receiver, after = false;
var target = {
  get first() { reads++; gc(); return 11; },
  get second() { reads++; throw undefined; }
};
var proxy = new Proxy(target, {
  get: function (target, key, actualReceiver) {
    receiver = actualReceiver;
    return Reflect.get(target, key, actualReceiver);
  }
});
var last = 0;
try {
  last = proxy.first;
  last = proxy.second;
  after = true;
} catch (error) {
  check(error === undefined, 'getter undefined throw');
}
check(last === 11 && reads === 2 && receiver === proxy && !after,
      'get result and receiver');

var later = 0;
var coercible = {
  valueOf: function () { gc(); throw marker; }
};
try {
  var before = normal.value;
  var result = coercible + (later++, 1);
  later += result;
  throw 'missing coercion throw';
} catch (error) {
  check(error === marker && before === 7 && later === 1, 'coercion order');
}

var finalizers = 0;
function finish(replace) {
  try {
    return normal.value;
  } finally {
    finalizers++;
    gc();
    if (replace) throw other;
  }
}
check(finish(false) === 7, 'finally keeps return');
try { finish(true); throw 'missing finalizer throw'; }
catch (error) { check(error === other, 'finally replaces return'); }
function suppress() {
  try { fail(marker); }
  finally { return normal.value; }
}
check(suppress() === 7 && finalizers === 2, 'finally replaces throw');

var visits = 0, cleanups = 0;
outer: for (var i = 0; i < 3; i++) {
  for (var j = 0; j < 3; j++) {
    try {
      if (j === 0) continue;
      if (i === 1) break outer;
      visits += normal.value;
    } finally {
      cleanups++;
    }
  }
}
check(visits === 14 && cleanups === 5, 'loop completion joins');
function select(index) {
  switch (index) {
    case 0: return normal.value;
    case 1: return target.second;
    default: return 19;
  }
}
check(select(0) === 7 && select(2) === 19, 'switch normal arms');
try { select(1); throw 'missing switch throw'; }
catch (error) { check(error === undefined, 'switch abrupt arm'); }

var closed = 0;
var iterator = {
  [Symbol.iterator]: function () { return this; },
  next: function () { return {value: normal.value, done: false}; },
  return: function () { closed++; gc(); throw other; }
};
try {
  for (var value of iterator) {
    check(value === 7, 'iterator input');
    fail(marker);
  }
  throw 'missing iterator throw';
} catch (error) {
  check(error === marker && closed === 1, 'iterator keeps original throw');
}
check(normal.value === 7 && finish(false) === 7, 'normal after caught throws');
print('ok');
