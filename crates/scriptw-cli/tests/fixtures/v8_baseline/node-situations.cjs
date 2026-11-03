// Opt-in V8 reference adapter. The generated source is the benchmark program.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');

async function main() {
  const [root, selected, sampleArg, warmupArg] = process.argv.slice(2);
  const samples = Number(sampleArg);
  const warmups = Number(warmupArg);
  if (process.argv.length !== 6 || !root || !selected ||
      !Number.isInteger(samples) || samples < 1 || samples > 100 ||
      !Number.isInteger(warmups) || warmups < 1 || warmups > 1000) {
    throw new Error('usage: node-situations.cjs FIXTURES CASE SAMPLES WARMUPS');
  }
  if (process.version !== 'v24.12.0') throw new Error('Node v24.12.0 is required');
  const flags = [...process.execArgv, process.env.NODE_OPTIONS || ''].join(' ');
  if (/--(?:allow[-_]natives[-_]syntax|jitless|expose[-_]gc|no[-_]opt|no[-_]turbo|no[-_]sparkplug)/.test(flags)) {
    throw new Error('normal JIT and natural GC are required; special runtime flags are forbidden');
  }
  if (typeof vm.SourceTextModule !== 'function') {
    throw new Error('Node requires --experimental-vm-modules');
  }
  const plan = JSON.parse(fs.readFileSync(path.join(root, 'manifest.json'), 'utf8'));
  if (plan.schema !== 'scriptw-retained-situations-v1' || plan.source_format !== 'module') {
    throw new Error('unsupported retained-situation manifest');
  }
  const entries = plan.execution.filter(entry => entry.file === selected);
  if (entries.length !== 1) throw new Error('CASE must select exactly one manifest entry');
  const entry = entries[0];
  if (path.basename(entry.file) !== entry.file || !entry.file.endsWith('.js')) {
    throw new Error('fixture must be a .js basename');
  }
  if (!Array.isArray(entry.invocations) || entry.invocations.length !== 4 ||
      entry.invocations.some((item, index) => item.input !== [3, 5, 7, 11][index] ||
        typeof item.stdout !== 'string' || !/^-?\d+\n$/.test(item.stdout)) ||
      entry.stdout !== entry.invocations[0].stdout) {
    throw new Error('expected checked 3,5,7,11 invocation matrix');
  }
  const bytes = fs.readFileSync(path.join(root, entry.file));
  const source = bytes.toString('utf8');
  if (/%[A-Za-z_$][\w$]*\s*\(/.test(source)) throw new Error('native intrinsic syntax is forbidden');
  let stdout = '';
  let stage = 'initialization';
  let unexpectedOutput = false;
  const context = vm.createContext({print: value => {
    if (stage !== 'run') {
      unexpectedOutput = true;
      throw new Error('unexpected output during ' + stage);
    }
    stdout += String(value) + '\n';
  }});
  const beforeParse = process.hrtime.bigint();
  const module = new vm.SourceTextModule(source, {context, identifier: entry.file});
  const parse = Number(process.hrtime.bigint() - beforeParse);
  const beforeLink = process.hrtime.bigint();
  await module.link(() => { throw new Error('standalone situations cannot import dependencies'); });
  const link = Number(process.hrtime.bigint() - beforeLink);
  const beforeInit = process.hrtime.bigint();
  const completion = await module.evaluate();
  const init = Number(process.hrtime.bigint() - beforeInit);
  if (unexpectedOutput) throw new Error('unexpected output during initialization');
  if (completion !== undefined) throw new Error('module initialization returned a value');
  const {prepare, run} = module.namespace;
  if (typeof prepare !== 'function' || typeof run !== 'function') {
    throw new Error('module must export prepare(input) and run()');
  }
  const observations = [];
  for (let index = 0; index < samples + warmups; index++) {
    const invocation_index = index % entry.invocations.length;
    const invocation = entry.invocations[invocation_index];
    stage = 'prepare';
    stdout = '';
    const beforePrepare = process.hrtime.bigint();
    const prepared = prepare(invocation.input);
    const prepare_ns = Number(process.hrtime.bigint() - beforePrepare);
    if (unexpectedOutput) throw new Error('unexpected output during prepare');
    if (prepared !== undefined) throw new Error('prepare must return undefined');
    stage = 'run';
    const beforeCall = process.hrtime.bigint();
    const result = run();
    const call_ns = Number(process.hrtime.bigint() - beforeCall);
    stage = 'validation';
    if (result !== undefined) throw new Error('run must return undefined');
    const matched = stdout === invocation.stdout;
    if (!matched) throw new Error(entry.file + ': invocation ' + invocation_index +
      ' checksum mismatch: ' + JSON.stringify(stdout));
    observations.push({index, invocation_index, input: invocation.input,
      warmup: index < warmups, call_ns, prepare_ns, stdout, matched});
  }
  console.log(JSON.stringify({schema: 'scriptw-v8-retained-situations-v1',
    file: entry.file, source_sha256: crypto.createHash('sha256').update(bytes).digest('hex'),
    node: process.version, v8: process.versions.v8, samples, warmups,
    native_syntax_absent: true, execution_policy: {jit: 'normal', gc: 'natural'},
    phase_ns: {parse, link, init}, observations}));
}
main().catch(error => { console.error(error.stack); process.exitCode = 1; });
