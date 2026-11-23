// Benchmark-only Node preload for scripts/benchmark-density.py; never product code.
// It holds a finished program's live process (isolate, context and heap intact) at
// `beforeExit` until the density driver has read every concurrent process's
// /proc/<pid>/smaps_rollup. The handshake is the precompiled host's phase-barrier
// line protocol, so one driver serves both runtimes.
'use strict';
const fs = require('node:fs');

const fd = Number(process.env.SCRIPTW_DENSITY_HOLD_FD);
const nonce = process.env.SCRIPTW_DENSITY_HOLD_NONCE;
if (!Number.isInteger(fd) || fd < 3 || !/^[0-9a-f]{32}$/.test(nonce || '')) {
  throw new Error('density hold: SCRIPTW_DENSITY_HOLD_FD and SCRIPTW_DENSITY_HOLD_NONCE are required');
}
delete process.env.SCRIPTW_DENSITY_HOLD_FD;
delete process.env.SCRIPTW_DENSITY_HOLD_NONCE;

process.once('beforeExit', () => {
  const tuple = `${nonce} 3 after_execution 0\n`;
  fs.writeSync(fd, 'SWPHASE1 ' + tuple);
  const buffer = Buffer.alloc(128);
  let text = '';
  while (!text.endsWith('\n')) {
    const count = fs.readSync(fd, buffer, 0, buffer.length, null);
    if (count === 0) throw new Error('density hold: driver closed the socket');
    text += buffer.toString('latin1', 0, count);
  }
  if (text !== 'SWACK1 ' + tuple) throw new Error('density hold: unexpected acknowledgement');
  fs.closeSync(fd);
});
