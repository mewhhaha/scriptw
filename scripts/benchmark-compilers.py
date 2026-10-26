#!/usr/bin/env python3
"""Measure the complete 17-case corpus with Scriptw, Scriptc/WASI and V8."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys

from compiler_comparison import ADAPTER, CHARTS, FIXTURES, RUNTIMES, SCHEMA, digest, load_report, publish, require, render

ROOT = Path(__file__).resolve().parents[1]


def stamp():
    return datetime.now(timezone.utc).isoformat()


def measure(args):
    require(sys.platform == 'linux' and len(os.sched_getaffinity(0)) == 1, 'use the serial resource scope')
    require(not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT), 'commit the source before measuring')
    require(not os.environ.get('NODE_OPTIONS'), 'NODE_OPTIONS must be unset')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for name in ('sources', 'adapted', 'expected', 'logs', 'wasm'):
        (output / name).mkdir()
    env = dict(os.environ, NODE_DISABLE_COMPILE_CACHE='1', SCRIPTC_TARGET='wasm32-wasi')
    env.pop('NODE_COMPILE_CACHE', None)
    events = []

    def invoke(label, command):
        out, err = 'logs/' + label + '.stdout', 'logs/' + label + '.stderr'
        event = dict(label=label, argv=list(map(str, command)), started_at=stamp(), stdout=out, stderr=err)
        events.append(event)
        print(label, flush=True)
        with (output / out).open('w') as stdout, (output / err).open('w') as stderr:
            try:
                process = subprocess.Popen(event['argv'], cwd=ROOT, env=env, stdout=stdout, stderr=stderr,
                                           start_new_session=True)
                event['returncode'] = process.wait(timeout=900)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                event['returncode'] = 124
                event['timeout_seconds'] = 900
        event['finished_at'] = stamp()
        return event

    plan_path = ROOT / FIXTURES / 'manifest.json'
    plan = json.loads(plan_path.read_text())
    require(len(plan['execution']) == 17, 'unexpected corpus')
    shutil.copyfile(plan_path, output / 'sources/manifest.json')
    for case in plan['execution']:
        name = case['file']
        original = (ROOT / FIXTURES / name).read_bytes()
        (output / 'sources' / name).write_bytes(original)
        (output / 'adapted' / name).write_bytes(ADAPTER.encode() + original)
        (output / 'expected' / name).write_text(case['stdout'])
    tools = {}
    for name, command in [('node', [args.node, '--version']), ('zig', ['zig', 'version']),
                          ('scriptc', [args.scriptc, '--version']), ('rustc', ['rustc', '--version'])]:
        event = invoke('version-' + name, command)
        require(event['returncode'] == 0, 'tool version query failed: ' + name)
        executable = Path(shutil.which(str(command[0])) or str(command[0])).resolve()
        tools[name] = dict(version=(output / event['stdout']).read_text().strip(), sha256=digest(executable))
    tools['runtime_baseline'] = dict(sha256=digest(args.scriptw_runner))
    tools['wasi_baseline'] = dict(sha256=digest(args.wasi_runner))
    event = invoke('version-v8', [args.node, '-p', 'process.versions.v8'])
    require(event['returncode'] == 0, 'V8 version query failed')
    tools['node']['v8'] = (output / event['stdout']).read_text().strip()
    require('0.4.5' in tools['scriptc']['version'] and tools['zig']['version'] == '0.16.0', 'tool pins differ')
    if args.build_log:
        shutil.copyfile(args.build_log, output / 'cargo-build.jsonl')
    if args.package_lock:
        shutil.copyfile(args.package_lock, output / 'package-lock.json')
    # Prove the toolchain and WASI host work before treating case errors as data.
    smoke = output / 'adapted/smoke.js'
    smoke.write_text('console.log("scriptc-wasi-ok");\n')
    (output / 'expected/smoke.js').write_text('scriptc-wasi-ok\n')
    event = invoke('smoke-compile', [args.scriptc, 'build', smoke, '-o', output / 'wasm/smoke.wasm'])
    require(event['returncode'] == 0, 'Scriptc WASI smoke compilation failed')
    event = invoke('smoke-run', [args.wasi_runner, output / 'wasm/smoke.wasm', output / 'expected/smoke.js', '3', '1'])
    require(event['returncode'] == 0, 'WASI smoke execution failed')
    smoke_rows = json.loads((output / event['stdout']).read_text())['observations']
    require(len(smoke_rows) == 4 and all(row['matched'] for row in smoke_rows), 'WASI smoke observations failed')
    from importlib.util import module_from_spec, spec_from_file_location
    spec = spec_from_file_location('benchmark_v8', ROOT / 'scripts/benchmark-v8.py')
    module = module_from_spec(spec)
    spec.loader.exec_module(module)
    report = dict(schema=SCHEMA, complete=False, source_commit=subprocess.check_output(
        ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(), started_at=stamp(),
        github_run_url=os.environ.get('BENCHMARK_RUN_URL'), machine=module.machine(), tools=tools,
        samples=7, warmups=10, rounds=2, workloads=plan['execution'], attempts=[], commands=events,
        policy=dict(execution='fresh Store/context; source/Wasm compilation and instance/context setup excluded; output/completion included',
                    memory='post-execution process RSS while Store/context alive; includes compilation/runtime/warmup effects; not peak or live heap',
                    wasmtime='vendored 47.0.0; product Fast/OptLevel::None, single-pass, copying GC; Scriptw linked R retains its admitted build policy',
                    order='rotate first runtime by workload and round; second round reverses case order',
                    limitations='GitHub shared host; descriptive observations; no statistical-significance or conformance claim'))
    report['machine']['measurement_affinity'] = sorted(os.sched_getaffinity(0))
    for round_index in range(2):
        cases = plan['execution'] if round_index == 0 else list(reversed(plan['execution']))
        for case_index, case in enumerate(cases):
            name = case['file']
            first = (case_index + round_index) % 3
            for runtime in RUNTIMES[first:] + RUNTIMES[:first]:
                label = f'round-{round_index}-{runtime}-{name}'
                wasm_dir = output / 'wasm' / label
                wasm_dir.mkdir()
                wasm = wasm_dir / (name + '.wasm')
                attempt = dict(round=round_index, file=name, runtime=runtime, status='ok', logs=[])
                report['attempts'].append(attempt)
                if runtime == 'scriptc':
                    event = invoke(label + '-compile', [args.scriptc, 'build', output / 'adapted' / name, '-o', wasm])
                    attempt['logs'] += [event['stdout'], event['stderr']]
                    if event['returncode'] != 0:
                        attempt['status'] = 'compile-failed'
                        (output / 'incomplete-report.json').write_text(json.dumps(report, indent=2) + '\n')
                        continue
                    command = [args.wasi_runner, wasm, output / 'expected' / name, '7', '10']
                elif runtime == 'scriptw':
                    command = [args.scriptw_runner, output / 'sources', '7', '10', name, wasm_dir]
                else:
                    command = [args.node, ROOT / 'crates/scriptw-cli/tests/fixtures/v8_baseline/node-runtime.cjs',
                               output / 'sources', '7', '10', name]
                event = invoke(label, command)
                attempt['logs'] += [event['stdout'], event['stderr']]
                attempt['raw'] = event['stdout']
                if runtime != 'v8': attempt['wasm'] = str(wasm.relative_to(output))
                if event['returncode'] != 0:
                    require(runtime == 'scriptc', 'reference runtime failed: ' + label)
                    attempt['status'] = 'run-failed'
                elif runtime == 'scriptc' and not all(row['matched'] for row in json.loads(
                        (output / event['stdout']).read_text())['observations']):
                    attempt['status'] = 'checksum-mismatch'
                if runtime == 'scriptw' and event['returncode'] == 0:
                    raw = json.loads((output / event['stdout']).read_text())
                    report.setdefault('scriptw_source_fingerprint', raw['source_fingerprint'])
                    report.setdefault('scriptw_runtime_policy', raw['cases'][0]['observations'][0]['runtime_policy'])
                (output / 'incomplete-report.json').write_text(json.dumps(report, indent=2) + '\n')
    report.update(complete=True, finished_at=stamp())
    report['retained_files'] = [dict(path=str(path.relative_to(output)), bytes=path.stat().st_size, sha256=digest(path))
        for path in sorted(output.rglob('*')) if path.is_file() and path.name != 'incomplete-report.json'
        and '.scriptc' not in path.parts]
    target = output / 'report.json'
    target.write_text(json.dumps(report, indent=2) + '\n')
    load_report(target)
    print(target)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    run = commands.add_parser('run')
    for name in ('scriptw-runner', 'wasi-runner', 'scriptc', 'output'):
        run.add_argument('--' + name, required=True, type=Path)
    run.add_argument('--node', default='node')
    run.add_argument('--build-log', type=Path)
    run.add_argument('--package-lock', type=Path)
    for name in ('publish', 'check'):
        command = commands.add_parser(name)
        command.add_argument('report', type=Path)
        command.add_argument('--root', type=Path, default=ROOT)
    args = parser.parse_args()
    if args.command == 'run': measure(args)
    elif args.command == 'publish': publish(args.report, args.root)
    else:
        report, rows = load_report(args.report, args.report.with_suffix('.tar.gz'))
        for metric, chart in CHARTS.items():
            require((args.root / 'docs/rust-rewrite/assets' / chart['filename']).read_bytes()
                    == render(report, rows, metric), metric + ' graph is stale')
        print(f"Verified {len(rows)} workloads, three compilers, source {report['source_commit']}")


if __name__ == '__main__':
    main()
