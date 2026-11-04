function check(value, message) { if (!value) throw message; }
var log = [], marker = {}, stored = {valueOf() { log.push('coerce'); gc(); return 4; }};
var row = {get x() { log.push('get'); gc(); return stored; },
           set x(value) { log.push('set:' + value); gc(); stored = value; }};
function strictWork(row, key, value, observe, token) {
  'use strict';
  var previous = row[key]++;
  row[key] = value;
  var sum = 0;
  for (var name in row) {
    if (name === key) sum += row[name];
  }
  try { observe(token); }
  catch (error) { return error === token && previous == 4 && sum === value; }
  return false;
}
function strictObserver(token) {
  var poisoned = false;
  try { strictWork.arguments; } catch (error) { poisoned = error instanceof TypeError; }
  check(poisoned, 'strict arguments reflection stays poisoned');
  gc();
  throw token;
}
check(strictWork(row, 'x', 9, strictObserver, marker), 'five-formal body and catch');
check(log.join('|') === 'get|coerce|set:5|set:9|get', 'getter coercion setter source order');

function equality(row, key, value, spare, extra) { 'use strict'; return row === arguments[0] && value == arguments[2]; }
function assignment(row, key, value, spare, extra) { 'use strict'; row[key] = arguments[2]; return row[key]; }
function assignmentKey(row, key, value, spare, extra) { 'use strict'; row[arguments[1]] = value; return row[key]; }
function update(row, key, value, spare, extra) { 'use strict'; return arguments[0][key]++; }
function updateKey(row, key, value, spare, extra) { 'use strict'; return row[arguments[1]]++; }
function caught(row, key, value, spare, extra) { 'use strict'; try { throw value; } catch (error) { return arguments[2] === error; } }
function enumerated(row, key, value, spare, extra) { 'use strict'; var total=0; for (var name in arguments[0]) total += row[name]; return total; }
function loopObserver(row, key, value, spare, extra) { 'use strict'; for (var name in row) return arguments[2]; }
function captured(row, key, value, spare, extra) { 'use strict'; try { throw value; } catch (error) { return () => arguments[2]; } }
var ordinary = {x: 3};
check(equality(ordinary, 'x', marker), 'equality nested arguments');
check(assignment(ordinary, 'x', 7) === 7 && assignmentKey(ordinary, 'x', 8) === 8, 'assignment nested arguments');
check(update(ordinary, 'x') === 8 && updateKey(ordinary, 'x') === 9, 'numeric update nested arguments');
check(caught(ordinary, 'x', marker), 'catch nested arguments');
check(enumerated(ordinary) === 10 && loopObserver(ordinary, 'x', marker) === marker, 'for-in nested arguments');
var retained = captured(ordinary, 'x', marker);
gc();
check(retained() === marker, 'catch-created arrow retains arguments');

function sloppy(row, key, value, observe, extra) {
  row[key] = value;
  try { observe(); } catch (error) { return error === value; }
  return false;
}
function reflect() {
  var active = sloppy.arguments;
  check(active[0] === ordinary && active[1] === 'x' && active[2] === marker && active[3] === reflect,
        'sloppy legacy reflection retains complete arguments');
  gc();
  throw active[2];
}
check(sloppy(ordinary, 'x', marker, reflect, 17), 'sloppy callback reentry');
true;
