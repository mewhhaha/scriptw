#!/usr/bin/env python3
"""Measure release JS->Wasm/Wasmtime versus V8; compare retained v2 reports."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tomllib

from benchmark_metrics import (POLICY_SCHEMA, SCHEMA, boundary_scoreboard, compare, digest, load_report,
                               measure_process, render, render_boundary_scoreboard, summarize,
                               validate_native_policy, validate_workload_metadata)

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_FIXTURES = ROOT / 'crates/scriptw-cli/tests/fixtures/v8_baseline'


def machine():
    cpu = next((line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines()
                if line.startswith('model name')), platform.processor())
    cgroups = []
    membership = next(line[3:] for line in Path('/proc/self/cgroup').read_text().splitlines() if line.startswith('0::'))
    group = Path('/sys/fs/cgroup') / membership.lstrip('/')
    if '..' in group.parts:
        raise ValueError('invalid cgroup membership')
    while True:
        row = {}
        for name in ['cpu.max', 'memory.max', 'memory.swap.max']:
            path = group / name
            if path.exists():
                row[name] = path.read_text().strip()
        cgroups.append(row)
        if group == Path('/sys/fs/cgroup'):
            break
        group = group.parent
    memory_limits = [int(row['memory.max']) for row in cgroups if row.get('memory.max', 'max') != 'max']
    cpu_limits = [int(row['cpu.max'].split()[0]) / int(row['cpu.max'].split()[1]) for row in cgroups
                  if row.get('cpu.max', 'max').split()[0] != 'max']
    return dict(system=platform.platform(), architecture=platform.machine(), cpu_model=cpu,
                host_logical_cpus=os.cpu_count(), affinity_before_pin=sorted(os.sched_getaffinity(0)),
                cgroups=cgroups, effective_memory_bytes=min(memory_limits) if memory_limits else None,
                effective_cpu_quota=min(cpu_limits) if cpu_limits else None)


def workload_plan(root, selected, partition='all'):
    plan = json.loads((root / 'manifest.json').read_text())
    if plan['schema'] != 'scriptw-v8-workloads-v1':
        raise ValueError('unsupported workload manifest')
    names = []
    for case in [plan['startup'], *plan['execution']]:
        validate_workload_metadata(case)
        name = case['file']
        if Path(name).name != name or not name.endswith('.js') or name in names:
            raise ValueError('workloads must have distinct .js basenames')
        names.append(name)
        path = (root / name).resolve()
        if not path.is_relative_to(root) or not path.is_file() or not isinstance(case['stdout'], str):
            raise ValueError('invalid workload source or expected stdout')
    if partition not in ('all', 'tuning', 'holdout'):
        raise ValueError('partition must be all, tuning or holdout')
    if selected:
        if len(set(selected)) != len(selected) or set(selected) - {c['file'] for c in plan['execution']}:
            raise ValueError('--case must name distinct execution entries in the manifest')
        plan['execution'] = [c for c in plan['execution'] if c['file'] in selected]
    if partition != 'all':
        plan['execution'] = [c for c in plan['execution'] if c.get('partition', 'unassigned') == partition]
    if not plan['execution']:
        raise ValueError('execution inventory must not be empty')
    return plan


def require_producers(log, cli, runner):
    producers = {}
    for line in log.read_text().splitlines():
        row = json.loads(line)
        if row.get('reason') == 'compiler-artifact' and row.get('executable'):
            producers[Path(row['executable']).resolve()] = row
    for path in [cli, runner]:
        row = producers.get(path)
        if not row or row['profile']['opt_level'] != '3' or row['profile']['test'] or row['features']:
            raise ValueError('both executables need actual default-feature release Cargo producers')
        name, kind = ('scriptw', 'bin') if path == cli else ('runtime_baseline', 'example')
        if row['target']['name'] != name or kind not in row['target']['kind'] or Path(row['manifest_path']).resolve() != ROOT / 'crates/scriptw-cli/Cargo.toml':
            raise ValueError('Cargo producer target or package differs')
    return producers


def check_runtime_prime(trace, required):
    hit = 'runtime-native-bundle hit:' in trace
    if required and not hit:
        raise ValueError('default startup did not admit its embedded native runtime; retain trace and refresh coherent build metadata before benchmarking')
    return hit


def runtime_order(index, first):
    runtimes = ['wasmtime', 'v8']
    if (index % 2 == 1) != (first == 'v8'):
        runtimes.reverse()
    return runtimes


def native_policy_plan(native_mode=None, gc_initial_mib=None, regalloc=None):
    if native_mode is None and gc_initial_mib is None and regalloc is None:
        return None
    policy = dict(native_mode=native_mode or 'fast', gc_initial_mib=16 if gc_initial_mib is None else gc_initial_mib,
                  register_allocation=regalloc or 'single-pass')
    validate_native_policy(policy)
    return policy


def main():
    if sys.argv[1:2] == ['scoreboard']:
        parser = argparse.ArgumentParser(description='Derive separate performance boundaries from an admitted retained v2 report.')
        parser.add_argument('report', type=Path)
        parser.add_argument('--output', type=Path, help='retain derived JSON without overwriting existing evidence')
        args = parser.parse_args(sys.argv[2:])
        report = load_report(args.report)
        board = boundary_scoreboard(report)
        board['source_report'] = dict(path=str(args.report.resolve()), sha256=digest(args.report))
        if args.output:
            with args.output.open('x') as stream:
                stream.write(json.dumps(board, indent=2) + '\n')
        print(render_boundary_scoreboard(board))
        return
    if sys.argv[1:2] == ['compare']:
        parser = argparse.ArgumentParser(description='Verify retained artifacts and compare matching v2 benchmark reports.')
        parser.add_argument('baseline', type=Path)
        parser.add_argument('candidate', type=Path)
        args = parser.parse_args(sys.argv[2:])
        candidate = load_report(args.candidate)
        candidate['comparison'] = compare(load_report(args.baseline), candidate)
        print(render(candidate))
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--scriptw', type=Path, required=True)
    parser.add_argument('--runtime-runner', type=Path, required=True)
    parser.add_argument('--build-log', type=Path, required=True)
    parser.add_argument('--node', default='node')
    parser.add_argument('--time', default=shutil.which('gtime') or shutil.which('time') or 'time', help='GNU time executable')
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, default=DEFAULT_FIXTURES)
    parser.add_argument('--case', action='append', dest='cases', help='select an execution manifest entry; repeatable')
    parser.add_argument('--partition', choices=('all', 'tuning', 'holdout'), default='all',
                        help='select a declared corpus partition; holdout scores remain separate from tuning')
    parser.add_argument('--skip-startup', action='store_true', help='measure only selected execution workloads')
    parser.add_argument('--execution-cache', type=Path, help='reuse checked execution caches across runs; startup caches remain isolated')
    parser.add_argument('--first-runtime', choices=('wasmtime', 'v8'), default='wasmtime',
                        help='reverse paired run order; observations retain the actual order')
    parser.add_argument('--baseline', type=Path, help='compatible complete v2 report to compare')
    parser.add_argument('--native-mode', choices=('fast', 'size-optimized', 'speed'),
                        help='explicit runner policy cohort; requires --skip-startup')
    parser.add_argument('--gc-initial-mib', type=int, choices=(8, 16, 32, 64, 128),
                        help='explicit initial GC capacity cohort; requires --skip-startup')
    parser.add_argument('--regalloc', choices=('single-pass', 'backtracking'),
                        help='explicit native register allocation cohort; requires --skip-startup')
    parser.add_argument('--samples', type=int, default=7, choices=range(3, 101), metavar='3..100')
    parser.add_argument('--warmups', type=int, default=10, choices=range(1, 101), metavar='1..100')
    parser.add_argument('--timeout', type=int, default=900, choices=range(1, 3601), metavar='SECONDS')
    parser.add_argument('--idle-machine', action='store_true', required=True)
    args = parser.parse_args()
    native_policy = native_policy_plan(args.native_mode, args.gc_initial_mib, args.regalloc)
    if native_policy is not None and not args.skip_startup:
        parser.error('explicit native-policy cohorts require --skip-startup; CLI startup uses its ordinary product policy')
    cli, runner = args.scriptw.resolve(), args.runtime_runner.resolve()
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT):
        raise ValueError('commit the coherent source batch before measuring')
    producers = require_producers(args.build_log, cli, runner)
    fixtures = args.fixtures.resolve()
    plan = workload_plan(fixtures, args.cases, args.partition)
    baseline = load_report(args.baseline) if args.baseline else None
    env = dict(os.environ)
    if env.get('NODE_OPTIONS'):
        raise ValueError('NODE_OPTIONS must be unset for the normal V8 baseline')
    env.pop('NODE_COMPILE_CACHE', None)
    env['NODE_DISABLE_COMPILE_CACHE'] = '1'
    spec = machine()
    selected_cpu = min(os.sched_getaffinity(0))
    spec['measurement_affinity'] = [selected_cpu]
    report = dict(schema=POLICY_SCHEMA if native_policy is not None else SCHEMA, complete=False,
                  started_at=datetime.now(timezone.utc).isoformat(), machine=spec,
                  source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                  measurement_order=dict(first_runtime=args.first_runtime, alternation='per startup sample and execution case'),
                  cli_sha256=digest(cli), runtime_runner_sha256=digest(runner),
                  cargo_build_log_sha256=digest(args.build_log),
                  cargo_producers={str(path): producers[path] for path in (cli, runner)},
                  profile='release; default features; opt-level 3', samples=args.samples,
                  configuration=dict(startup=not args.skip_startup, warmups=args.warmups,
                                     timeout_seconds=args.timeout, native_jobs=1,
                                     selection_partition=args.partition,
                                     execution_cache=dict(kind='shared', path=str(args.execution_cache.resolve())) if args.execution_cache else dict(kind='fresh-per-report')),
                  process_samples=[], execution={r: dict(cases=[]) for r in ('wasmtime', 'v8')},
                  commands=[], retained_files=[],
                  policy=dict(version=2,
                              startup='parent spawn-to-reap monotonic wall time; GNU time supervisor and blocking wait4; receipts outside span; OS cache not flushed',
                              cold='unique empty application cache; shared runtime build image retained; normal Node without persistent compile cache',
                              hot='prewarmed scriptw disk cache; repeated new Node process without equivalent persistent code cache',
                              execution='compile once per case/process; equal warmup counts; fresh Store or vm Context; output/completion included; setup outside execution span',
                              memory='GNU time kernel high-water RSS covers the runtime child including compilation, warmups and caches; excludes inherited Python launcher high-water; no subtraction',
                              prime='SCRIPTW_WASM_TRACE=1 only for unmeasured startup priming; record whether embedded native runtime was admitted',
                              heap='Wasmtime GC capacity is reserved space, V8 heapUsed is process-wide used space; not equivalent live JS byte counts',
                              sampler='existing Wasmtime invocation RSS sampling every 10ms plus endpoints; Node memoryUsage endpoints; instrumentation is part of the measured configuration',
                              gc='normal runtime GC and V8 tiering; no forced collections; no unavailable Wasmtime allocation/live-heap/GC-pause claims',
                              wasmtime_compilation='product default Fast: Cranelift OptLevel::None',
                              limitations='single machine; no OS cache flush, DVFS control, statistical significance or conformance acceptance'))
    if native_policy is not None:
        report['configuration']['native_policy'] = native_policy
        report['policy']['wasmtime_compilation'] = 'explicit runner native-policy cohort; actual per-invocation options verified'
    report['harness_sha256'] = {str(path.relative_to(ROOT)): digest(path) for path in
        [Path(__file__).resolve(), ROOT / 'scripts/benchmark_metrics.py', DEFAULT_FIXTURES / 'node-runtime.cjs',
         ROOT / 'crates/scriptw-cli/examples/runtime_baseline.rs']}
    report['workloads'] = [dict(case, role=role, sha256=digest(fixtures / case['file']))
                           for role, case in [('startup', plan['startup']), *[('execution', c) for c in plan['execution']]]]
    report['node_versions'] = json.loads(subprocess.check_output([args.node, '-p', 'JSON.stringify(process.versions)'], env=env, text=True))
    report['time_version'] = subprocess.check_output([args.time, '--version'], text=True).strip()
    if 'GNU Time' not in report['time_version']:
        raise ValueError('--time must select GNU time')
    report['time_sha256'] = digest(Path(shutil.which(args.time) or args.time))
    report['wasmtime_version'] = next(p['version'] for p in tomllib.loads((ROOT / 'Cargo.lock').read_text())['package'] if p['name'] == 'wasmtime')
    # Reject incompatible baselines before spending time compiling/running JS.
    if baseline:
        compare(dict(baseline, summaries={}), dict(report, summaries={}))
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for name in ('logs', 'workload', 'wasm'):
        (output / name).mkdir()
    workload_root = output / 'workload'
    (workload_root / 'manifest.json').write_text(json.dumps(plan, indent=2) + '\n')
    for case in [plan['startup'], *plan['execution']]:
        (workload_root / case['file']).write_bytes((fixtures / case['file']).read_bytes())

    def save():
        temp = output / 'report.pending'
        temp.write_text(json.dumps(report, indent=2) + '\n')
        temp.replace(output / 'report.json')

    def invoke(label, command, environment):
        event = dict(label=label, command=[str(c) for c in command], state='running', timeout_seconds=args.timeout)
        report['commands'].append(event)
        save()
        with (output / 'logs' / (label + '.stdout')).open('xb') as out, (output / 'logs' / (label + '.stderr')).open('xb') as err:
            event.update(measure_process(command, cwd=ROOT, env=environment, stdout=out, stderr=err, timeout=args.timeout, time_binary=args.time))
        event.update(state='closed',
                     stdout_sha256=digest(output / 'logs' / (label + '.stdout')),
                     stderr_sha256=digest(output / 'logs' / (label + '.stderr')))
        save()
        if event['exit_code'] or event['timed_out']:
            raise RuntimeError(f'{label} failed: exit {event["exit_code"]}, timeout={event["timed_out"]}')
        return event, (output / 'logs' / (label + '.stdout')).read_text()

    original_affinity = os.sched_getaffinity(0)
    os.sched_setaffinity(0, {selected_cpu})
    save()
    try:
        _, text = invoke('compiler-identity', [cli, 'compiler-identity'], env)
        identity = report['compiler_identity'] = json.loads(text)
        if identity['source_revision'] != dict(kind='git-commit', commit=report['source_commit']) or identity['executable_sha256'] != report['cli_sha256']:
            raise ValueError('loaded compiler source/executable identity differs from the measured image')
        if not args.skip_startup:
            startup = workload_root / plan['startup']['file']
            suffix = 'run outcome: RunOutcome { backend_used: WasmAot, note: "wasm-aot completion: undefined(undefined)" }\n'
            node_command = [args.node, '-e', 'globalThis.print = console.log; require(process.argv[1]);', str(startup)]
            scriptw_command = [cli, '--jobs', '1', 'run', '--execution-backend', 'wasm', str(startup)]
            warm_env = dict(env, SCRIPTW_CACHE_DIR=str(output / 'hot-cache'))
            _, text = invoke('scriptw-prime-hot-cache', scriptw_command, dict(warm_env, SCRIPTW_WASM_TRACE='1'))
            if text != plan['startup']['stdout'] + suffix:
                raise ValueError('startup warmup output mismatch')
            trace = (output / 'logs/scriptw-prime-hot-cache.stderr').read_text()
            report['embedded_runtime_prime_hit'] = check_runtime_prime(trace, fixtures == DEFAULT_FIXTURES)
            save()
            for mode in ('cold', 'hot'):
                for index in range(args.samples):
                    for ordered_runtime in runtime_order(index, args.first_runtime):
                        runtime = 'scriptw' if ordered_runtime == 'wasmtime' else ordered_runtime
                        label = f'{mode}-{index:02}-{runtime}'
                        runtime_env = warm_env if mode == 'hot' else dict(env, SCRIPTW_CACHE_DIR=str(output / f'cold-cache-{index:02}'))
                        event, text = invoke(label, scriptw_command if runtime == 'scriptw' else node_command, runtime_env)
                        if text != plan['startup']['stdout'] + (suffix if runtime == 'scriptw' else ''):
                            raise ValueError(f'{label}: output mismatch; original retained')
                        report['process_samples'].append(dict(mode=mode, runtime=runtime, **event))
                        save()
        for index, case in enumerate(plan['execution']):
            for runtime in runtime_order(index, args.first_runtime):
                label = 'execution-' + runtime + '-' + case['file']
                command = ([runner, workload_root, str(args.samples), str(args.warmups), case['file'], output / 'wasm'] if runtime == 'wasmtime' else
                           [args.node, str(DEFAULT_FIXTURES / 'node-runtime.cjs'), workload_root, str(args.samples), str(args.warmups), case['file']])
                if runtime == 'wasmtime' and native_policy is not None:
                    command += ['--native-mode', native_policy['native_mode'], '--gc-initial-mib', str(native_policy['gc_initial_mib']),
                                '--regalloc', native_policy['register_allocation']]
                execution_cache = args.execution_cache.resolve() if args.execution_cache else output / 'execution-cache'
                report['execution_cache_directory'] = str(execution_cache)
                _, text = invoke(label, command, dict(env, SCRIPTW_CACHE_DIR=str(execution_cache)))
                result = json.loads(text)
                if result['samples'] != args.samples or result['warmups'] != args.warmups or len(result['cases']) != 1 or result['cases'][0]['file'] != case['file']:
                    raise ValueError('runtime sample plan differs')
                if runtime == 'wasmtime' and (result['source_commit'] != report['source_commit'] or result['source_fingerprint'] != identity['source_fingerprint'] or result['executable_sha256'] != report['runtime_runner_sha256']):
                    raise ValueError('runtime runner compiler/executable identity differs')
                if runtime == 'wasmtime' and native_policy is not None:
                    if result.get('schema') != 'scriptw-wasmtime-policy-baseline-v1' or result.get('requested_native_policy') != native_policy:
                        raise ValueError('runtime runner requested policy differs')
                    if any(row.get('actual_native_policy') != native_policy for row in result['cases'][0]['observations']):
                        raise ValueError('runtime runner actual policy differs')
                report['execution'][runtime]['cases'].append(dict(result['cases'][0], command_label=label))
                save()
        report['summaries'] = summarize(report)
        for directory in ('logs', 'workload', 'wasm'):
            report['retained_files'] += [dict(path=str(p.relative_to(output)), sha256=digest(p)) for p in sorted((output / directory).iterdir())]
        report.update(complete=True, finished_at=datetime.now(timezone.utc).isoformat())
        save()
        load_report(output / 'report.json')
        if baseline:
            report['comparison'] = compare(baseline, report)
            report['baseline'] = dict(path=str(args.baseline.resolve()), sha256=digest(args.baseline))
        save()
        (output / 'summary.md').write_text(render(report))
        print(render(report))
        print('Retained report:', output / 'report.json')
    except BaseException as error:
        report.update(complete=False, failure=f'{type(error).__name__}: {error}')
        save()
        raise
    finally:
        os.sched_setaffinity(0, original_affinity)


if __name__ == '__main__':
    main()
