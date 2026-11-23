// Benchmark harness only: runs one Porffor-compiled Wasm program (https://porffor.dev)
// in Node's WebAssembly engine. It implements no part of Scriptw.
//
// usage: node wasm_peer_node.cjs PROGRAM.wasm EXPECTED_STDOUT SAMPLES WARMUPS
//
// Each entry gets a fresh Instance (fresh linear memory and globals), created
// before the timer like a fresh Wasmtime Store; the timer covers only the
// exported main call. Imports follow the Porffor 0.61 wrapper's order of
// creation (print, printChar, time, timeOrigin) and the same JS semantics.
// Failed or mismatched entries are reported, never thrown, so the driver can
// record them as failed attempts.
const fs = require('node:fs');
const crypto = require('node:crypto');
const [program, expected, sampleArg, warmupArg] = process.argv.slice(2);
const samples = Number(sampleArg), warmups = Number(warmupArg);
if (!program || expected === undefined || !Number.isInteger(samples) || samples < 3 || samples > 100 ||
    !Number.isInteger(warmups) || warmups < 1 || warmups > 1000) {
  throw new Error('usage: wasm_peer_node.cjs PROGRAM.wasm EXPECTED_STDOUT SAMPLES WARMUPS');
}
const bytes = fs.readFileSync(program);
const report = {schema: 'scriptw-wasm-peer-node-v1', engine: 'node-webassembly', node: process.version,
  v8: process.versions.v8, wasm_sha256: crypto.createHash('sha256').update(bytes).digest('hex'),
  wasm_bytes: bytes.length, samples, warmups, flags: process.execArgv, observations: []};
let wasmModule;
const compileBegin = process.hrtime.bigint();
try {
  wasmModule = new WebAssembly.Module(bytes);
} catch (error) {
  report.status = 'module-rejected';
  report.error = String(error);
  console.log(JSON.stringify(report));
  process.exit(0);
}
report.module_compilation_ns = Number(process.hrtime.bigint() - compileBegin);
const imports = WebAssembly.Module.imports(wasmModule);
const known = new Set(['a', 'b', 'c', 'd']);
const unknown = imports.filter(entry => entry.module !== '' || entry.kind !== 'function' || !known.has(entry.name));
report.imports = imports.map(entry => `${entry.module}.${entry.name}:${entry.kind}`);
if (unknown.length || !WebAssembly.Module.exports(wasmModule).some(entry => entry.name === 'm' && entry.kind === 'function')) {
  report.status = 'unsupported-interface';
  console.log(JSON.stringify(report));
  process.exit(0);
}
report.status = 'complete';
for (let index = 0; index < warmups + samples; index++) {
  let stdout = '';
  const host = {
    a: value => { stdout += value.toString(); },
    b: value => { stdout += String.fromCharCode(value); },
    c: () => performance.now(),
    d: () => performance.timeOrigin,
  };
  const instantiateBegin = process.hrtime.bigint();
  const instance = new WebAssembly.Instance(wasmModule, {'': host});
  const instantiation_ns = Number(process.hrtime.bigint() - instantiateBegin);
  const main = instance.exports.m;
  let completion = 'normal', error = '';
  const begin = process.hrtime.bigint();
  try {
    main();
  } catch (thrown) {
    completion = thrown instanceof WebAssembly.RuntimeError ? 'trap' : 'throw';
    error = String(thrown);
    const tag = instance.exports['0'];
    if (thrown instanceof WebAssembly.Exception && tag instanceof WebAssembly.Tag && thrown.is(tag)) {
      error = `uncaught Porffor exception (type tag ${thrown.getArg(tag, 1)})`;
    }
  }
  const execution_ns = Number(process.hrtime.bigint() - begin);
  report.observations.push({index, warmup: index < warmups, execution_ns, instantiation_ns, stdout,
    matched: completion === 'normal' && stdout === expected, completion, error});
  if (completion !== 'normal') break;
}
console.log(JSON.stringify(report));
